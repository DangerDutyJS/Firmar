//! Aceptación de los términos de uso, guardada fuera de la bóveda (no es un
//! secreto) para que sobreviva a un borrado de datos.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::FirmarResult;
use crate::storage::atomic_write;

const SETTINGS_FILE: &str = "legal.json";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegalStatus {
    /// Versión de los términos aceptada (p. ej. "2026-10-28"); `None` si nunca.
    pub accepted_version: Option<String>,
    pub accepted_at: Option<DateTime<Utc>>,
}

pub struct LegalStore {
    path: PathBuf,
}

impl LegalStore {
    pub fn new(app_data_dir: &Path) -> Self {
        Self { path: app_data_dir.join(SETTINGS_FILE) }
    }

    pub fn status(&self) -> LegalStatus {
        fs::read(&self.path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn accept(&self, version: &str) -> FirmarResult<LegalStatus> {
        let status = LegalStatus { accepted_version: Some(version.to_owned()), accepted_at: Some(Utc::now()) };
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        atomic_write(&self.path, &serde_json::to_vec_pretty(&status)?)?;
        Ok(status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remembers_accepted_version() {
        let dir = tempfile::tempdir().unwrap();
        let store = LegalStore::new(dir.path());
        assert!(store.status().accepted_version.is_none());
        store.accept("2026-10-28").unwrap();
        assert_eq!(LegalStore::new(dir.path()).status().accepted_version.as_deref(), Some("2026-10-28"));
    }
}
