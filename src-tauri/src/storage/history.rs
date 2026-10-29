//! Historial cifrado de documentos firmados en este equipo.

use std::fs;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::crypto::VaultKey;
use crate::domain::DocumentFormat;
use crate::error::FirmarResult;
use crate::storage::{Vault, atomic_write};

const HISTORY_FILE: &str = "history.vault";
const HISTORY_AAD: &[u8] = b"firmar/history/v1";
const MAX_ENTRIES: usize = 5000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub document_id: Uuid,
    pub file_name: String,
    /// Ruta del archivo firmado la última vez que se guardó.
    pub output_path: String,
    pub format: DocumentFormat,
    pub signed_at: DateTime<Utc>,
    pub signatures: usize,
}

impl Vault {
    pub fn list_history(&self, key: &VaultKey) -> FirmarResult<Vec<HistoryEntry>> {
        let path = self.root().join(HISTORY_FILE);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let plain = zeroize::Zeroizing::new(key.open_blob(&fs::read(path)?, HISTORY_AAD)?);
        Ok(serde_json::from_slice(&plain)?)
    }

    /// Añade o actualiza (por documento) una entrada; la más reciente queda primero.
    pub fn record_history(&self, key: &VaultKey, entry: HistoryEntry) -> FirmarResult<()> {
        let mut entries = self.list_history(key)?;
        entries.retain(|e| e.document_id != entry.document_id);
        entries.insert(0, entry);
        entries.truncate(MAX_ENTRIES);
        self.write_history(key, &entries)
    }

    pub fn forget_history(&self, key: &VaultKey, document_id: Uuid) -> FirmarResult<()> {
        let mut entries = self.list_history(key)?;
        entries.retain(|e| e.document_id != document_id);
        self.write_history(key, &entries)
    }

    pub(crate) fn write_history(&self, key: &VaultKey, entries: &[HistoryEntry]) -> FirmarResult<()> {
        let plain = zeroize::Zeroizing::new(serde_json::to_vec(entries)?);
        atomic_write(&self.root().join(HISTORY_FILE), &key.seal_blob(&plain, HISTORY_AAD)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::KeyMaterial;

    fn entry(id: Uuid, name: &str) -> HistoryEntry {
        HistoryEntry {
            document_id: id,
            file_name: name.into(),
            output_path: format!("C:/docs/{name}"),
            format: DocumentFormat::Pdf,
            signed_at: Utc::now(),
            signatures: 1,
        }
    }

    #[test]
    fn upserts_by_document_and_is_encrypted() {
        let dir = tempfile::tempdir().unwrap();
        let keys = KeyMaterial::ephemeral();
        let vault = Vault::open(dir.path()).unwrap();
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());

        vault.record_history(&keys.vault_key, entry(a, "contrato.pdf")).unwrap();
        vault.record_history(&keys.vault_key, entry(b, "acta.pdf")).unwrap();
        vault.record_history(&keys.vault_key, entry(a, "contrato-v2.pdf")).unwrap();

        let list = vault.list_history(&keys.vault_key).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].file_name, "contrato-v2.pdf", "la última firma va primero");
        let raw = fs::read(dir.path().join(HISTORY_FILE)).unwrap();
        assert!(!raw.windows(5).any(|w| w == b"acta."));

        vault.forget_history(&keys.vault_key, b).unwrap();
        assert_eq!(vault.list_history(&keys.vault_key).unwrap().len(), 1);
    }
}
