//! Comandos de identidad pública, contactos de confianza y bloqueo de la app.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::Serialize;

use super::{Shared, blocking, check_size, poisoned};
use crate::crypto;
use crate::error::{FirmarError, FirmarResult};
use crate::storage::{Contact, IdentityCard, LockStatus, atomic_write, hello};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MyIdentity {
    name: Option<String>,
    fingerprint: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContactView {
    name: String,
    fingerprint: String,
    added_at: DateTime<Utc>,
}

impl From<Contact> for ContactView {
    fn from(c: Contact) -> Self {
        Self { fingerprint: c.fingerprint(), name: c.name, added_at: c.added_at }
    }
}

#[tauri::command]
pub async fn identity_get(state: Shared<'_>) -> FirmarResult<MyIdentity> {
    blocking(&state, |s| {
        let keys = s.keys()?;
        Ok(MyIdentity {
            name: s.vault.my_profile(&keys.vault_key)?.name,
            fingerprint: crypto::fingerprint(&keys.identity.public_key()),
        })
    })
    .await
}

#[tauri::command]
pub async fn identity_set_name(state: Shared<'_>, name: Option<String>) -> FirmarResult<MyIdentity> {
    blocking(&state, move |s| {
        let keys = s.keys()?;
        let _w = s.write_lock.lock().map_err(|_| poisoned())?;
        let profile = s.vault.set_my_name(&keys.vault_key, name.as_deref())?;
        Ok(MyIdentity { name: profile.name, fingerprint: crypto::fingerprint(&keys.identity.public_key()) })
    })
    .await
}

/// Guarda tu tarjeta de identidad (`.firmarid`) para compartirla.
#[tauri::command]
pub async fn identity_export_card(state: Shared<'_>, path: PathBuf) -> FirmarResult<()> {
    blocking(&state, move |s| {
        let keys = s.keys()?;
        let name = s.vault.my_profile(&keys.vault_key)?.name.unwrap_or_default();
        atomic_write(&path, &IdentityCard::issue(&keys.identity, &name)?.to_bytes()?)
    })
    .await
}

#[tauri::command]
pub async fn contacts_list(state: Shared<'_>) -> FirmarResult<Vec<ContactView>> {
    blocking(&state, |s| Ok(s.vault.list_contacts(&s.keys()?.vault_key)?.into_iter().map(ContactView::from).collect())).await
}

/// Importa la tarjeta de otra persona como contacto de confianza.
#[tauri::command]
pub async fn contact_import(state: Shared<'_>, path: PathBuf) -> FirmarResult<ContactView> {
    blocking(&state, move |s| {
        check_size(&path)?;
        let bytes = std::fs::read(&path)?;
        if bytes.len() > 64 * 1024 {
            return Err(FirmarError::InvalidInput("el archivo no es una tarjeta de identidad".into()));
        }
        let card = IdentityCard::parse(&bytes)?;
        let keys = s.keys()?;
        let _w = s.write_lock.lock().map_err(|_| poisoned())?;
        Ok(s.vault.add_contact(&keys.vault_key, &card, &keys.identity)?.into())
    })
    .await
}

#[tauri::command]
pub async fn contact_rename(state: Shared<'_>, fingerprint: String, name: String) -> FirmarResult<()> {
    blocking(&state, move |s| {
        let keys = s.keys()?;
        let _w = s.write_lock.lock().map_err(|_| poisoned())?;
        s.vault.rename_contact(&keys.vault_key, &fingerprint, &name)
    })
    .await
}

#[tauri::command]
pub async fn contact_remove(state: Shared<'_>, fingerprint: String) -> FirmarResult<()> {
    blocking(&state, move |s| {
        let keys = s.keys()?;
        let _w = s.write_lock.lock().map_err(|_| poisoned())?;
        s.vault.remove_contact(&keys.vault_key, &fingerprint)
    })
    .await
}

// --- Bloqueo -------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LockView {
    #[serde(flatten)]
    status: LockStatus,
    hello_available: bool,
}

#[tauri::command]
pub async fn lock_status(state: Shared<'_>) -> FirmarResult<LockView> {
    blocking(&state, |s| {
        let status = s.lock.lock().map_err(|_| poisoned())?.status();
        Ok(LockView { status, hello_available: hello::available() })
    })
    .await
}

#[tauri::command]
pub async fn lock_set_pin(state: Shared<'_>, pin: String) -> FirmarResult<()> {
    let pin = zeroize::Zeroizing::new(pin);
    blocking(&state, move |s| s.lock.lock().map_err(|_| poisoned())?.set_pin(&pin)).await
}

#[tauri::command]
pub async fn lock_disable(state: Shared<'_>) -> FirmarResult<()> {
    blocking(&state, |s| s.lock.lock().map_err(|_| poisoned())?.disable()).await
}

#[tauri::command]
pub async fn lock_set_options(state: Shared<'_>, hello: bool, auto_lock_minutes: u32) -> FirmarResult<()> {
    blocking(&state, move |s| {
        if hello && !crate::storage::hello::available() {
            return Err(FirmarError::UnsupportedFormat("Windows Hello no está configurado en este equipo".into()));
        }
        s.lock.lock().map_err(|_| poisoned())?.set_options(hello, auto_lock_minutes)
    })
    .await
}

#[tauri::command]
pub async fn lock_unlock_pin(state: Shared<'_>, pin: String) -> FirmarResult<()> {
    let pin = zeroize::Zeroizing::new(pin);
    blocking(&state, move |s| s.lock.lock().map_err(|_| poisoned())?.unlock_with_pin(&pin)).await
}

#[tauri::command]
pub async fn lock_unlock_hello(window: tauri::WebviewWindow, state: Shared<'_>) -> FirmarResult<()> {
    let hwnd = window_handle(&window)?;
    blocking(&state, move |s| {
        if !s.lock.lock().map_err(|_| poisoned())?.status().hello {
            return Err(FirmarError::InvalidInput("Windows Hello no está activado para Firmar Vecipp".into()));
        }
        // El diálogo de Windows puede tardar: no se retiene el candado mientras tanto.
        if hello::verify(hwnd, "Confirma tu identidad para desbloquear Firmar Vecipp")? {
            s.lock.lock().map_err(|_| poisoned())?.unlock_verified()
        } else {
            Err(FirmarError::InvalidInput("no se pudo verificar tu identidad".into()))
        }
    })
    .await
}

#[tauri::command]
pub fn lock_now(state: Shared<'_>) -> FirmarResult<()> {
    state.lock.lock().map_err(|_| poisoned())?.lock_now();
    Ok(())
}

#[cfg(windows)]
fn window_handle(window: &tauri::WebviewWindow) -> FirmarResult<isize> {
    window.hwnd().map(|h| h.0 as isize).map_err(|e| FirmarError::Io(std::io::Error::other(e.to_string())))
}

#[cfg(not(windows))]
fn window_handle(_window: &tauri::WebviewWindow) -> FirmarResult<isize> {
    Ok(0)
}
