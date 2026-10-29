//! Bóveda local cifrada: perfiles de firma y copias de los originales
//! (necesarias para re-posicionar una firma sin acumular estampados).

use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use uuid::Uuid;

use crate::crypto::VaultKey;
use crate::domain::SignatureProfile;
use crate::image_engine;
use crate::error::{FirmarError, FirmarResult};

const PROFILES_FILE: &str = "profiles.vault";
const ORIGINALS_DIR: &str = "originals";
const PROFILES_AAD: &[u8] = b"firmar/profiles/v1";
const MAX_SIGNATURE_PNG: usize = 8 * 1024 * 1024;

pub struct Vault {
    root: PathBuf,
}

impl Vault {
    pub fn open(root: impl Into<PathBuf>) -> FirmarResult<Self> {
        let root = root.into();
        fs::create_dir_all(root.join(ORIGINALS_DIR))?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn list_profiles(&self, key: &VaultKey) -> FirmarResult<Vec<SignatureProfile>> {
        let path = self.root.join(PROFILES_FILE);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let plain = key.open_blob(&fs::read(path)?, PROFILES_AAD)?;
        Ok(serde_json::from_slice(&plain)?)
    }

    pub fn get_profile(&self, key: &VaultKey, id: Uuid) -> FirmarResult<SignatureProfile> {
        self.list_profiles(key)?
            .into_iter()
            .find(|p| p.id == id)
            .ok_or_else(|| FirmarError::NotFound(format!("perfil de firma {id}")))
    }

    pub fn add_profile(&self, key: &VaultKey, name: &str, png: Vec<u8>) -> FirmarResult<SignatureProfile> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 80 {
            return Err(FirmarError::InvalidInput("el nombre debe tener entre 1 y 80 caracteres".into()));
        }
        if png.len() > MAX_SIGNATURE_PNG {
            return Err(FirmarError::InvalidInput("la imagen de la firma supera 8 MB".into()));
        }
        // Validamos que sea una imagen y la dejamos solo con la tinta.
        let img = image_engine::clean_signature(&image::load_from_memory(&png)?.into_rgba8())?;
        let (width, height) = img.dimensions();

        let profile = SignatureProfile {
            id: Uuid::new_v4(),
            name: name.to_owned(),
            created_at: Utc::now(),
            width,
            height,
            png: image_engine::encode_png(&img)?,
            cleanup_version: image_engine::CLEANUP_VERSION,
        };
        let mut profiles = self.list_profiles(key)?;
        profiles.push(profile.clone());
        self.write_profiles(key, &profiles)?;
        Ok(profile)
    }

    /// Re-limpia los perfiles guardados con una versión anterior del algoritmo
    /// (p. ej. firmas importadas con un recuadro o un velo blanco).
    pub fn migrate_profiles(&self, key: &VaultKey) -> FirmarResult<usize> {
        let mut profiles = self.list_profiles(key)?;
        let mut migrated = 0;
        for p in profiles.iter_mut().filter(|p| p.cleanup_version < image_engine::CLEANUP_VERSION) {
            let img = image::load_from_memory(&p.png)?.into_rgba8();
            if let Ok(clean) = image_engine::clean_signature(&img) {
                (p.width, p.height) = clean.dimensions();
                p.png = image_engine::encode_png(&clean)?;
            }
            p.cleanup_version = image_engine::CLEANUP_VERSION;
            migrated += 1;
        }
        if migrated > 0 {
            self.write_profiles(key, &profiles)?;
        }
        Ok(migrated)
    }

    pub fn delete_profile(&self, key: &VaultKey, id: Uuid) -> FirmarResult<()> {
        let mut profiles = self.list_profiles(key)?;
        let before = profiles.len();
        profiles.retain(|p| p.id != id);
        if profiles.len() == before {
            return Err(FirmarError::NotFound(format!("perfil de firma {id}")));
        }
        self.write_profiles(key, &profiles)
    }

    pub fn store_original(&self, key: &VaultKey, document_id: Uuid, bytes: &[u8]) -> FirmarResult<()> {
        let blob = key.seal_blob(bytes, &original_aad(document_id))?;
        atomic_write(&self.original_path(document_id), &blob)
    }

    pub fn load_original(&self, key: &VaultKey, document_id: Uuid) -> FirmarResult<Vec<u8>> {
        let path = self.original_path(document_id);
        if !path.exists() {
            return Err(FirmarError::NotFound(
                "la copia original de este documento no está en la bóveda local".into(),
            ));
        }
        key.open_blob(&fs::read(path)?, &original_aad(document_id))
    }

    /// Borra todo el contenido de la bóveda (firmas, originales, historial) y
    /// la deja vacía y lista para usarse.
    pub fn wipe(&self) -> FirmarResult<()> {
        if self.root.exists() {
            fs::remove_dir_all(&self.root)?;
        }
        fs::create_dir_all(self.root.join(ORIGINALS_DIR))?;
        Ok(())
    }

    /// Ids de todos los documentos cuyo original está en la bóveda.
    pub fn original_ids(&self) -> FirmarResult<Vec<Uuid>> {
        let mut ids = Vec::new();
        for entry in fs::read_dir(self.root.join(ORIGINALS_DIR))? {
            let name = entry?.file_name();
            let name = name.to_string_lossy();
            if let Some(id) = name.strip_suffix(".bin").and_then(|n| Uuid::parse_str(n).ok()) {
                ids.push(id);
            }
        }
        Ok(ids)
    }

    /// Escribe una bóveda completa cifrada con `key` en un directorio aparte.
    /// No afecta a la actual hasta llamar a [`StagedVault::commit`].
    pub fn stage_replacement(
        &self,
        key: &VaultKey,
        profiles: &[SignatureProfile],
        originals: &[(Uuid, Vec<u8>)],
        history: &[crate::storage::HistoryEntry],
        my_profile: &crate::storage::MyProfile,
        contacts: &[crate::storage::Contact],
    ) -> FirmarResult<StagedVault> {
        let name = self.root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "vault".into());
        let staging = self.root.with_file_name(format!("{name}.staging-{}", Uuid::new_v4().simple()));
        let staged = Vault::open(&staging)?;
        let result = (|| {
            staged.write_profiles(key, profiles)?;
            staged.write_history(key, history)?;
            staged.write_my_profile(key, my_profile)?;
            staged.write_contacts(key, contacts)?;
            for (id, bytes) in originals {
                staged.store_original(key, *id, bytes)?;
            }
            Ok(())
        })();
        if let Err(e) = result {
            let _ = fs::remove_dir_all(&staging);
            return Err(e);
        }
        Ok(StagedVault { staging, target: self.root.clone() })
    }

    fn original_path(&self, document_id: Uuid) -> PathBuf {
        self.root.join(ORIGINALS_DIR).join(format!("{document_id}.bin"))
    }

    fn write_profiles(&self, key: &VaultKey, profiles: &[SignatureProfile]) -> FirmarResult<()> {
        let plain = zeroize::Zeroizing::new(serde_json::to_vec(profiles)?);
        let blob = key.seal_blob(&plain, PROFILES_AAD)?;
        atomic_write(&self.root.join(PROFILES_FILE), &blob)
    }
}

pub struct StagedVault {
    staging: PathBuf,
    target: PathBuf,
}

impl StagedVault {
    /// Sustituye la bóveda actual por la preparada.
    pub fn commit(self) -> FirmarResult<()> {
        let retired = self.target.with_extension(format!("old-{}", Uuid::new_v4().simple()));
        fs::rename(&self.target, &retired)?;
        if let Err(e) = fs::rename(&self.staging, &self.target) {
            let _ = fs::rename(&retired, &self.target);
            return Err(e.into());
        }
        let _ = fs::remove_dir_all(retired);
        Ok(())
    }

    pub fn discard(self) {
        let _ = fs::remove_dir_all(&self.staging);
    }
}

fn original_aad(document_id: Uuid) -> Vec<u8> {
    let mut aad = b"firmar/original/v1/".to_vec();
    aad.extend_from_slice(document_id.as_bytes());
    aad
}

/// Escribe en un temporal y renombra, para no dejar la bóveda a medias si el
/// proceso muere a mitad de escritura.
pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> FirmarResult<()> {
    let tmp = path.with_extension(format!("tmp-{}", Uuid::new_v4().simple()));
    fs::write(&tmp, bytes)?;
    if let Err(e) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(e.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::KeyMaterial;

    pub(crate) fn tiny_png() -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(40, 20, image::Rgba([0, 0, 0, 255]));
        let mut out = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
        out
    }

    #[test]
    fn profiles_roundtrip_encrypted() {
        let dir = tempfile::tempdir().unwrap();
        let keys = KeyMaterial::ephemeral();
        let vault = Vault::open(dir.path()).unwrap();

        let p = vault.add_profile(&keys.vault_key, "Mi firma", tiny_png()).unwrap();
        assert_eq!(p.cleanup_version, image_engine::CLEANUP_VERSION);
        assert_eq!(vault.list_profiles(&keys.vault_key).unwrap().len(), 1);

        let raw = fs::read(dir.path().join(PROFILES_FILE)).unwrap();
        assert!(!raw.windows(8).any(|w| w == b"Mi firma"), "el archivo debe estar cifrado");

        let other = KeyMaterial::ephemeral();
        assert!(vault.list_profiles(&other.vault_key).is_err());

        vault.delete_profile(&keys.vault_key, p.id).unwrap();
        assert!(vault.list_profiles(&keys.vault_key).unwrap().is_empty());
    }

    #[test]
    fn legacy_profiles_are_cleaned_once() {
        let dir = tempfile::tempdir().unwrap();
        let keys = KeyMaterial::ephemeral();
        let vault = Vault::open(dir.path()).unwrap();
        // Perfil antiguo: fondo blanco semiopaco alrededor del trazo.
        let mut img = image::RgbaImage::from_pixel(200, 100, image::Rgba([255, 255, 255, 49]));
        for x in 30..170 {
            img.put_pixel(x, 50, image::Rgba([0, 0, 0, 255]));
        }
        let legacy = SignatureProfile {
            id: Uuid::new_v4(),
            name: "vieja".into(),
            created_at: Utc::now(),
            width: 200,
            height: 100,
            png: image_engine::encode_png(&img).unwrap(),
            cleanup_version: 0,
        };
        vault.write_profiles(&keys.vault_key, &[legacy]).unwrap();

        assert_eq!(vault.migrate_profiles(&keys.vault_key).unwrap(), 1);
        let p = &vault.list_profiles(&keys.vault_key).unwrap()[0];
        assert!(p.width < 200 && p.height < 20, "recortada a la tinta: {}x{}", p.width, p.height);
        assert_eq!(vault.migrate_profiles(&keys.vault_key).unwrap(), 0);
    }

    #[test]
    fn wipe_leaves_an_empty_usable_vault() {
        let dir = tempfile::tempdir().unwrap();
        let keys = KeyMaterial::ephemeral();
        let vault = Vault::open(dir.path().join("v")).unwrap();
        vault.add_profile(&keys.vault_key, "Mía", tiny_png()).unwrap();
        vault.store_original(&keys.vault_key, Uuid::new_v4(), b"x").unwrap();

        vault.wipe().unwrap();
        let fresh = KeyMaterial::ephemeral();
        assert!(vault.list_profiles(&fresh.vault_key).unwrap().is_empty());
        assert!(vault.original_ids().unwrap().is_empty());
        vault.add_profile(&fresh.vault_key, "Nueva", tiny_png()).unwrap();
    }

    #[test]
    fn originals_are_bound_to_document_id() {
        let dir = tempfile::tempdir().unwrap();
        let keys = KeyMaterial::ephemeral();
        let vault = Vault::open(dir.path()).unwrap();
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());

        vault.store_original(&keys.vault_key, a, b"contenido").unwrap();
        assert_eq!(vault.load_original(&keys.vault_key, a).unwrap(), b"contenido");

        fs::copy(vault.original_path(a), vault.original_path(b)).unwrap();
        assert!(vault.load_original(&keys.vault_key, b).is_err());
    }
}
