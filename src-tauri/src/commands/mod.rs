//! Comandos IPC expuestos al frontend. Todo el trabajo pesado (E/S, cifrado,
//! PDF) corre en el pool bloqueante para no congelar la ventana.

pub mod identity;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use chrono::{DateTime, Utc};
use serde::Serialize;
use tauri::ipc::Response;
use tauri::{Emitter, Manager, State};
use uuid::Uuid;

use crate::crypto;
use crate::legal::{LegalStatus, LegalStore};
use crate::docx_engine::DocxLayout;
use crate::domain::{DocumentInspection, SignaturePlacement, SignatureProfile, SignedDocument};
use crate::error::{FirmarError, FirmarResult};
use crate::services::{self, BatchItem, PlacementTemplate, SigningService};
use crate::storage::HistoryEntry;
use crate::storage::backup::{self, BackupSummary};
use crate::storage::{AppLock, KeyMaterial, Vault};

const MAX_DOCUMENT_BYTES: u64 = 512 * 1024 * 1024;

pub struct AppState {
    vault: Vault,
    /// Las claves se cargan del llavero en el primer uso; si el usuario
    /// cancela un aviso del sistema, el siguiente intento vuelve a probar.
    keys: Mutex<Option<Arc<KeyMaterial>>>,
    /// Serializa las escrituras sobre la bóveda.
    write_lock: Mutex<()>,
    /// Archivo con el que se abrió la app («Firmar con Firmar» del Explorador).
    launch_file: Mutex<Option<PathBuf>>,
    legal: LegalStore,
    lock: Mutex<AppLock>,
}

impl AppState {
    pub fn new(vault: Vault, legal: LegalStore) -> Self {
        let lock = Mutex::new(AppLock::load(vault.root()));
        Self { vault, keys: Mutex::new(None), write_lock: Mutex::new(()), launch_file: Mutex::new(None), legal, lock }
    }

    /// Claves de la identidad. Falla con `Locked` si la app está bloqueada:
    /// así ningún comando puede tocar la bóveda sin desbloquear.
    fn keys(&self) -> FirmarResult<Arc<KeyMaterial>> {
        self.lock.lock().map_err(|_| poisoned())?.ensure_unlocked()?;
        let mut guard = self.keys.lock().map_err(|_| poisoned())?;
        if let Some(keys) = guard.as_ref() {
            return Ok(keys.clone());
        }
        let keys = Arc::new(KeyMaterial::load_or_create()?);
        *guard = Some(keys.clone());
        Ok(keys)
    }
}

pub(crate) fn poisoned() -> FirmarError {
    FirmarError::Crypto("estado interno bloqueado por un fallo previo".into())
}

pub(crate) type Shared<'a> = State<'a, Arc<AppState>>;

pub(crate) async fn blocking<T, F>(state: &Shared<'_>, f: F) -> FirmarResult<T>
where
    T: Send + 'static,
    F: FnOnce(&AppState) -> FirmarResult<T> + Send + 'static,
{
    let state = Arc::clone(state.inner());
    tauri::async_runtime::spawn_blocking(move || f(&state))
        .await
        .map_err(|e| FirmarError::Io(std::io::Error::other(e.to_string())))?
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentityInfo {
    fingerprint: String,
    vault_path: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileView {
    id: Uuid,
    name: String,
    created_at: DateTime<Utc>,
    width: u32,
    height: u32,
    data_url: String,
}

impl From<SignatureProfile> for ProfileView {
    fn from(p: SignatureProfile) -> Self {
        Self {
            id: p.id,
            name: p.name,
            created_at: p.created_at,
            width: p.width,
            height: p.height,
            data_url: format!("data:image/png;base64,{}", STANDARD.encode(&p.png)),
        }
    }
}

#[tauri::command]
pub async fn identity_info(state: Shared<'_>) -> FirmarResult<IdentityInfo> {
    blocking(&state, |s| {
        let keys = s.keys()?;
        Ok(IdentityInfo {
            fingerprint: crypto::fingerprint(&keys.identity.public_key()),
            vault_path: s.vault.root().display().to_string(),
        })
    })
    .await
}

#[tauri::command]
pub async fn vault_list(state: Shared<'_>) -> FirmarResult<Vec<ProfileView>> {
    blocking(&state, |s| {
        let keys = s.keys()?;
        {
            let _w = s.write_lock.lock().map_err(|_| poisoned())?;
            s.vault.migrate_profiles(&keys.vault_key)?;
        }
        let mut profiles = s.vault.list_profiles(&keys.vault_key)?;
        profiles.sort_by_key(|p| std::cmp::Reverse(p.created_at));
        Ok(profiles.into_iter().map(ProfileView::from).collect())
    })
    .await
}

#[tauri::command]
pub async fn vault_save(state: Shared<'_>, name: String, png_base64: String) -> FirmarResult<ProfileView> {
    blocking(&state, move |s| {
        let data = png_base64.strip_prefix("data:image/png;base64,").unwrap_or(&png_base64);
        let png = STANDARD.decode(data.trim())?;
        let keys = s.keys()?;
        let _w = s.write_lock.lock().map_err(|_| poisoned())?;
        Ok(s.vault.add_profile(&keys.vault_key, &name, png)?.into())
    })
    .await
}

#[tauri::command]
pub async fn vault_delete(state: Shared<'_>, id: Uuid) -> FirmarResult<()> {
    blocking(&state, move |s| {
        let keys = s.keys()?;
        let _w = s.write_lock.lock().map_err(|_| poisoned())?;
        s.vault.delete_profile(&keys.vault_key, id)
    })
    .await
}

/// Devuelve los bytes crudos como `ArrayBuffer` (sin pasar por JSON).
#[tauri::command]
pub async fn document_read(state: Shared<'_>, path: PathBuf) -> FirmarResult<Response> {
    blocking(&state, move |_| Ok(Response::new(read_bounded(&path)?))).await
}

#[tauri::command]
pub async fn document_inspect(state: Shared<'_>, path: PathBuf) -> FirmarResult<DocumentInspection> {
    blocking(&state, move |s| {
        check_size(&path)?;
        let keys = s.keys()?;
        SigningService { keys: &keys, vault: &s.vault }.inspect(&path)
    })
    .await
}

/// Bytes del original sin firma (solo para el propietario), para re-posicionar.
#[tauri::command]
pub async fn document_original(state: Shared<'_>, path: PathBuf) -> FirmarResult<Response> {
    blocking(&state, move |s| {
        check_size(&path)?;
        let keys = s.keys()?;
        Ok(Response::new(SigningService { keys: &keys, vault: &s.vault }.original(&path)?))
    })
    .await
}

#[tauri::command]
pub async fn docx_layout(state: Shared<'_>, path: PathBuf) -> FirmarResult<DocxLayout> {
    blocking(&state, move |s| {
        check_size(&path)?;
        let keys = s.keys()?;
        SigningService { keys: &keys, vault: &s.vault }.docx_layout(&path)
    })
    .await
}

#[tauri::command]
pub async fn document_sign(
    state: Shared<'_>,
    input_path: PathBuf,
    output_path: Option<PathBuf>,
    profile_id: Uuid,
    placements: Vec<SignaturePlacement>,
) -> FirmarResult<SignedDocument> {
    blocking(&state, move |s| {
        check_size(&input_path)?;
        // Sin ruta: junto al original como `nombre.firmado.ext`, sin sobrescribir nada.
        let output_path = output_path.unwrap_or_else(|| services::default_output_path(&input_path));
        let keys = s.keys()?;
        let _w = s.write_lock.lock().map_err(|_| poisoned())?;
        SigningService { keys: &keys, vault: &s.vault }.sign(&input_path, &output_path, profile_id, &placements)
    })
    .await
}

#[tauri::command]
pub async fn document_resign(
    state: Shared<'_>,
    signed_path: PathBuf,
    output_path: Option<PathBuf>,
    profile_id: Uuid,
    placements: Vec<SignaturePlacement>,
) -> FirmarResult<SignedDocument> {
    blocking(&state, move |s| {
        check_size(&signed_path)?;
        // Sin ruta: se actualiza el mismo archivo firmado.
        let output_path = output_path.unwrap_or_else(|| signed_path.clone());
        let keys = s.keys()?;
        let _w = s.write_lock.lock().map_err(|_| poisoned())?;
        SigningService { keys: &keys, vault: &s.vault }.resign(&signed_path, &output_path, profile_id, &placements)
    })
    .await
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct BatchProgress {
    done: usize,
    total: usize,
    current: String,
}

/// Firma varios archivos con la misma posición. Emite `batch-progress`.
#[tauri::command]
pub async fn document_sign_batch(
    app: tauri::AppHandle,
    state: Shared<'_>,
    paths: Vec<PathBuf>,
    profile_id: Uuid,
    template: PlacementTemplate,
) -> FirmarResult<Vec<BatchItem>> {
    if paths.is_empty() || paths.len() > 500 {
        return Err(FirmarError::InvalidInput("elige entre 1 y 500 archivos".into()));
    }
    blocking(&state, move |s| {
        let keys = s.keys()?;
        let _w = s.write_lock.lock().map_err(|_| poisoned())?;
        let total = paths.len();
        SigningService { keys: &keys, vault: &s.vault }.sign_batch(&paths, profile_id, &template, |done, current| {
            let current = current.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let _ = app.emit("batch-progress", BatchProgress { done, total, current });
        })
    })
    .await
}

#[tauri::command]
pub async fn history_list(state: Shared<'_>) -> FirmarResult<Vec<HistoryEntry>> {
    blocking(&state, |s| s.vault.list_history(&s.keys()?.vault_key)).await
}

#[tauri::command]
pub async fn history_forget(state: Shared<'_>, document_id: Uuid) -> FirmarResult<()> {
    blocking(&state, move |s| {
        let keys = s.keys()?;
        let _w = s.write_lock.lock().map_err(|_| poisoned())?;
        s.vault.forget_history(&keys.vault_key, document_id)
    })
    .await
}

/// Devuelve (una sola vez) el archivo pasado al abrir la app.
#[tauri::command]
pub fn take_launch_file(state: Shared<'_>) -> Option<String> {
    state.launch_file.lock().ok()?.take().map(|p| p.display().to_string())
}

/// Primer argumento que sea un archivo existente (lo que pasa el Explorador).
pub fn file_argument(args: impl IntoIterator<Item = String>) -> Option<PathBuf> {
    args.into_iter().skip(1).map(PathBuf::from).find(|p| p.is_file())
}

#[tauri::command]
pub fn legal_status(state: Shared<'_>) -> LegalStatus {
    state.legal.status()
}

#[tauri::command]
pub fn legal_accept(state: Shared<'_>, version: String) -> FirmarResult<LegalStatus> {
    if version.is_empty() || version.len() > 32 {
        return Err(FirmarError::InvalidInput("versión de términos inválida".into()));
    }
    state.legal.accept(&version)
}

/// Texto que el usuario debe escribir para confirmar el borrado total.
const WIPE_CONFIRMATION: &str = "BORRAR";

/// Derecho de supresión: elimina firmas, originales, historial y las claves
/// del llavero. Los documentos ya firmados siguen siendo verificables, pero
/// ya no se podrán re-posicionar desde este equipo.
#[tauri::command]
pub async fn data_wipe(state: Shared<'_>, confirmation: String) -> FirmarResult<()> {
    if confirmation.trim() != WIPE_CONFIRMATION {
        return Err(FirmarError::InvalidInput(format!("escribe {WIPE_CONFIRMATION} para confirmar")));
    }
    blocking(&state, |s| {
        s.lock.lock().map_err(|_| poisoned())?.ensure_unlocked()?;
        let _w = s.write_lock.lock().map_err(|_| poisoned())?;
        s.vault.wipe()?;
        KeyMaterial::delete_from_keyring()?;
        *s.keys.lock().map_err(|_| poisoned())? = None;
        s.lock.lock().map_err(|_| poisoned())?.reset();
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn backup_export(state: Shared<'_>, path: PathBuf, password: String) -> FirmarResult<()> {
    let password = zeroize::Zeroizing::new(password);
    blocking(&state, move |s| {
        let keys = s.keys()?;
        let _w = s.write_lock.lock().map_err(|_| poisoned())?;
        crate::storage::atomic_write(&path, &backup::export(&keys, &s.vault, &password)?)
    })
    .await
}

#[tauri::command]
pub async fn backup_inspect(state: Shared<'_>, path: PathBuf, password: String) -> FirmarResult<BackupSummary> {
    let password = zeroize::Zeroizing::new(password);
    blocking(&state, move |_| {
        check_size(&path)?;
        backup::inspect(&std::fs::read(&path)?, &password)
    })
    .await
}

#[tauri::command]
pub async fn backup_restore(state: Shared<'_>, path: PathBuf, password: String) -> FirmarResult<IdentityInfo> {
    let password = zeroize::Zeroizing::new(password);
    blocking(&state, move |s| {
        check_size(&path)?;
        let bytes = std::fs::read(&path)?;
        let current = s.keys()?;
        let _w = s.write_lock.lock().map_err(|_| poisoned())?;
        let restored = Arc::new(backup::restore(&bytes, &password, &current, &s.vault)?);
        *s.keys.lock().map_err(|_| poisoned())? = Some(restored.clone());
        Ok(IdentityInfo {
            fingerprint: crypto::fingerprint(&restored.identity.public_key()),
            vault_path: s.vault.root().display().to_string(),
        })
    })
    .await
}

/// Muestra el archivo seleccionado en el Explorador.
#[tauri::command]
pub fn reveal_path(app: tauri::AppHandle, path: PathBuf) -> FirmarResult<()> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .reveal_item_in_dir(&path)
        .map_err(|e| FirmarError::Io(std::io::Error::other(e.to_string())))
}

/// Abre el archivo con el programa predeterminado del sistema.
#[tauri::command]
pub fn open_path(app: tauri::AppHandle, path: PathBuf) -> FirmarResult<()> {
    use tauri_plugin_opener::OpenerExt;
    if !path.is_file() {
        return Err(FirmarError::NotFound(path.display().to_string()));
    }
    app.opener()
        .open_path(path.display().to_string(), None::<&str>)
        .map_err(|e| FirmarError::Io(std::io::Error::other(e.to_string())))
}

pub(crate) fn check_size(path: &std::path::Path) -> FirmarResult<()> {
    let meta = std::fs::metadata(path)?;
    if !meta.is_file() {
        return Err(FirmarError::InvalidInput("la ruta no es un archivo".into()));
    }
    if meta.len() > MAX_DOCUMENT_BYTES {
        return Err(FirmarError::InvalidInput("el archivo supera el límite de 512 MB".into()));
    }
    Ok(())
}

fn read_bounded(path: &std::path::Path) -> FirmarResult<Vec<u8>> {
    check_size(path)?;
    Ok(std::fs::read(path)?)
}

pub fn init_state(app: &tauri::App) -> FirmarResult<Arc<AppState>> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| FirmarError::Io(std::io::Error::other(e.to_string())))?;
    let state = AppState::new(Vault::open(data_dir.join("vault"))?, LegalStore::new(&data_dir));
    *state.launch_file.lock().map_err(|_| poisoned())? = file_argument(std::env::args());
    Ok(Arc::new(state))
}
