//! Copia de seguridad de la identidad: claves del llavero, perfiles de firma y
//! originales de la bóveda, cifrados con una contraseña (Argon2id + AES-256-GCM).
//!
//! Sin esta copia, reinstalar el sistema o cambiar de equipo pierde la clave
//! del llavero y con ella la posibilidad de gestionar los documentos firmados.

use argon2::{Algorithm, Argon2, Params, Version};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::crypto::{self, Identity, KEY_LEN, VaultKey};
use crate::domain::SignatureProfile;
use crate::error::{FirmarError, FirmarResult};
use crate::storage::{HistoryEntry, KeyMaterial, Vault};

const FORMAT: &str = "firmar-backup";
const VERSION: u32 = 1;
const AAD: &[u8] = b"firmar/backup/v1";
pub const MIN_PASSWORD_CHARS: usize = 10;

// Parámetros recomendados por OWASP para Argon2id (64 MiB, 3 pasadas).
const M_COST_KIB: u32 = 64 * 1024;
const T_COST: u32 = 3;
const P_COST: u32 = 1;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackupFile {
    format: String,
    v: u32,
    kdf: KdfParams,
    #[serde(with = "crate::domain::b64")]
    nonce: Vec<u8>,
    #[serde(with = "crate::domain::b64")]
    ciphertext: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct KdfParams {
    alg: String,
    m_cost_kib: u32,
    t_cost: u32,
    p_cost: u32,
    #[serde(with = "crate::domain::b64")]
    salt: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackupPayload {
    #[serde(with = "crate::domain::b64")]
    vault_key: Vec<u8>,
    #[serde(with = "crate::domain::b64")]
    identity_seed: Vec<u8>,
    profiles: Vec<SignatureProfile>,
    originals: Vec<StoredOriginal>,
    #[serde(default)]
    history: Vec<HistoryEntry>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredOriginal {
    document_id: Uuid,
    #[serde(with = "crate::domain::b64")]
    bytes: Vec<u8>,
}

impl Drop for BackupPayload {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.vault_key.zeroize();
        self.identity_seed.zeroize();
    }
}

/// Resumen de lo que contiene una copia, para confirmar antes de restaurar.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupSummary {
    pub fingerprint: String,
    pub profiles: usize,
    pub documents: usize,
}

pub fn export(keys: &KeyMaterial, vault: &Vault, password: &str) -> FirmarResult<Vec<u8>> {
    check_password(password)?;
    let payload = BackupPayload {
        vault_key: keys.vault_key.as_bytes().to_vec(),
        identity_seed: keys.identity.seed().to_vec(),
        profiles: vault.list_profiles(&keys.vault_key)?,
        originals: vault
            .original_ids()?
            .into_iter()
            .map(|id| Ok(StoredOriginal { document_id: id, bytes: vault.load_original(&keys.vault_key, id)? }))
            .collect::<FirmarResult<_>>()?,
        history: vault.list_history(&keys.vault_key)?,
    };
    let plain = Zeroizing::new(serde_json::to_vec(&payload)?);

    let salt = crypto::random_bytes::<16>()?;
    let key = derive_key(password, &salt, M_COST_KIB, T_COST, P_COST)?;
    let sealed = key.seal(&plain, AAD)?;
    let file = BackupFile {
        format: FORMAT.into(),
        v: VERSION,
        kdf: KdfParams {
            alg: "argon2id".into(),
            m_cost_kib: M_COST_KIB,
            t_cost: T_COST,
            p_cost: P_COST,
            salt: salt.to_vec(),
        },
        nonce: sealed.nonce.to_vec(),
        ciphertext: sealed.ciphertext,
    };
    Ok(serde_json::to_vec_pretty(&file)?)
}

pub fn inspect(bytes: &[u8], password: &str) -> FirmarResult<BackupSummary> {
    let payload = decrypt(bytes, password)?;
    let identity = Identity::from_seed(&payload.identity_seed)?;
    Ok(BackupSummary {
        fingerprint: crypto::fingerprint(&identity.public_key()),
        profiles: payload.profiles.len(),
        documents: payload.originals.len(),
    })
}

/// Restaura la copia. Los perfiles y originales actuales se conservan y se
/// re-cifran con la clave restaurada. Devuelve las claves nuevas.
pub fn restore(bytes: &[u8], password: &str, current: &KeyMaterial, vault: &Vault) -> FirmarResult<KeyMaterial> {
    let payload = decrypt(bytes, password)?;
    let restored = KeyMaterial {
        vault_key: VaultKey::from_bytes(&payload.vault_key)?,
        identity: Identity::from_seed(&payload.identity_seed)?,
    };

    let mut profiles = payload.profiles.clone();
    for p in vault.list_profiles(&current.vault_key)? {
        if !profiles.iter().any(|q| q.id == p.id) {
            profiles.push(p);
        }
    }
    let mut originals: Vec<(Uuid, Vec<u8>)> =
        payload.originals.iter().map(|o| (o.document_id, o.bytes.clone())).collect();
    for id in vault.original_ids()? {
        if !originals.iter().any(|(o, _)| *o == id) {
            originals.push((id, vault.load_original(&current.vault_key, id)?));
        }
    }

    let mut history = payload.history.clone();
    for entry in vault.list_history(&current.vault_key)? {
        if !history.iter().any(|h| h.document_id == entry.document_id) {
            history.push(entry);
        }
    }
    history.sort_by_key(|h| std::cmp::Reverse(h.signed_at));

    // Primero se prepara la bóveda nueva, luego se guardan las claves y solo
    // entonces se activa: un fallo intermedio deja la bóveda anterior intacta.
    let staged = vault.stage_replacement(&restored.vault_key, &profiles, &originals, &history)?;
    if let Err(e) = restored.store() {
        // Si solo se escribió una de las dos claves, volvemos a las anteriores.
        let _ = current.store();
        staged.discard();
        return Err(e);
    }
    staged.commit()?;
    Ok(restored)
}

fn decrypt(bytes: &[u8], password: &str) -> FirmarResult<BackupPayload> {
    let file: BackupFile = serde_json::from_slice(bytes)
        .map_err(|_| FirmarError::InvalidInput("el archivo no es una copia de seguridad de Firmar Vecipp".into()))?;
    if file.format != FORMAT || file.v != VERSION || file.kdf.alg != "argon2id" {
        return Err(FirmarError::UnsupportedFormat("versión de copia de seguridad no compatible".into()));
    }
    // Límites para que un archivo manipulado no agote la memoria.
    if file.kdf.m_cost_kib > 1024 * 1024 || file.kdf.t_cost > 20 || file.kdf.p_cost > 16 {
        return Err(FirmarError::InvalidInput("parámetros de la copia fuera de rango".into()));
    }
    let key = derive_key(password, &file.kdf.salt, file.kdf.m_cost_kib, file.kdf.t_cost, file.kdf.p_cost)?;
    let plain = Zeroizing::new(
        key.open(&file.nonce, &file.ciphertext, AAD)
            .map_err(|_| FirmarError::InvalidInput("contraseña incorrecta o copia dañada".into()))?,
    );
    let payload: BackupPayload = serde_json::from_slice(&plain)?;
    if payload.vault_key.len() != KEY_LEN || payload.identity_seed.len() != KEY_LEN {
        return Err(FirmarError::Integrity("la copia contiene claves inválidas".into()));
    }
    Ok(payload)
}

fn derive_key(password: &str, salt: &[u8], m: u32, t: u32, p: u32) -> FirmarResult<VaultKey> {
    let params = Params::new(m, t, p, Some(KEY_LEN)).map_err(|e| FirmarError::Crypto(e.to_string()))?;
    let mut out = Zeroizing::new([0u8; KEY_LEN]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(password.as_bytes(), salt, out.as_mut())
        .map_err(|e| FirmarError::Crypto(e.to_string()))?;
    VaultKey::from_bytes(out.as_ref())
}

fn check_password(password: &str) -> FirmarResult<()> {
    if password.chars().count() < MIN_PASSWORD_CHARS {
        return Err(FirmarError::InvalidInput(format!(
            "la contraseña debe tener al menos {MIN_PASSWORD_CHARS} caracteres"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png() -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(40, 20, image::Rgba([0, 0, 0, 255]));
        crate::image_engine::encode_png(&img).unwrap()
    }

    #[test]
    fn export_restore_roundtrip_merges_current_vault() {
        let dir = tempfile::tempdir().unwrap();
        let old = KeyMaterial::ephemeral();
        let old_vault = Vault::open(dir.path().join("a")).unwrap();
        let p = old_vault.add_profile(&old.vault_key, "Mía", png()).unwrap();
        let doc = Uuid::new_v4();
        old_vault.store_original(&old.vault_key, doc, b"original").unwrap();
        let backup = export(&old, &old_vault, "contraseña larga").unwrap();
        assert!(!backup.windows(3).any(|w| w == "Mía".as_bytes()), "la copia está cifrada");

        let summary = inspect(&backup, "contraseña larga").unwrap();
        assert_eq!((summary.profiles, summary.documents), (1, 1));
        assert!(inspect(&backup, "otra contraseña").is_err());

        // Equipo nuevo con su propia firma: se restaura sin perderla.
        let new = KeyMaterial::ephemeral();
        let new_vault = Vault::open(dir.path().join("b")).unwrap();
        let mine = new_vault.add_profile(&new.vault_key, "Nueva", png()).unwrap();
        let staged_keys = {
            let payload = decrypt(&backup, "contraseña larga").unwrap();
            KeyMaterial {
                vault_key: VaultKey::from_bytes(&payload.vault_key).unwrap(),
                identity: Identity::from_seed(&payload.identity_seed).unwrap(),
            }
        };
        // Sin llavero en pruebas: ejercitamos la parte de la bóveda directamente.
        let mut profiles = new_vault.list_profiles(&new.vault_key).unwrap();
        profiles.extend(old_vault.list_profiles(&old.vault_key).unwrap());
        let staged = new_vault
            .stage_replacement(&staged_keys.vault_key, &profiles, &[(doc, b"original".to_vec())], &[])
            .unwrap();
        staged.commit().unwrap();

        let ids: Vec<Uuid> = new_vault.list_profiles(&staged_keys.vault_key).unwrap().iter().map(|p| p.id).collect();
        assert!(ids.contains(&p.id) && ids.contains(&mine.id));
        assert_eq!(new_vault.load_original(&staged_keys.vault_key, doc).unwrap(), b"original");
        assert_eq!(staged_keys.identity.public_key(), old.identity.public_key());
    }

    #[test]
    fn rejects_weak_password_and_garbage() {
        let dir = tempfile::tempdir().unwrap();
        let keys = KeyMaterial::ephemeral();
        let vault = Vault::open(dir.path()).unwrap();
        assert!(export(&keys, &vault, "corta").is_err());
        assert!(inspect(b"{}", "contraseña larga").is_err());
    }
}
