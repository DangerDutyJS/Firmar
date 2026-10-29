//! Identidad pública del firmante y contactos de confianza.
//!
//! - **Mi nombre**: se incluye (firmado) en el sello de cada documento.
//! - **Tarjeta de identidad** (`.firmarid`): nombre + clave pública, firmada
//!   con la propia clave para demostrar que quien la entrega la controla.
//! - **Contactos**: tarjetas importadas. Al verificar un documento, si la clave
//!   del firmante es de un contacto se muestra su nombre como verificado.

use std::fs;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::container::normalize_signer_name;
use crate::crypto::{self, Identity, VaultKey};
use crate::error::{FirmarError, FirmarResult};
use crate::storage::{Vault, atomic_write};

const PROFILE_FILE: &str = "identity.vault";
const PROFILE_AAD: &[u8] = b"firmar/identity/v1";
const CONTACTS_FILE: &str = "contacts.vault";
const CONTACTS_AAD: &[u8] = b"firmar/contacts/v1";
const CARD_FORMAT: &str = "firmar-identity";
const CARD_VERSION: u32 = 1;
const CARD_DOMAIN: &[u8] = b"FIRMAR/idcard/v1\0";
const MAX_CONTACTS: usize = 10_000;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MyProfile {
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Contact {
    #[serde(with = "crate::domain::b64")]
    pub public_key: Vec<u8>,
    /// Nombre con el que lo guardaste (editable; al importar, el de su tarjeta).
    pub name: String,
    pub added_at: DateTime<Utc>,
}

impl Contact {
    pub fn fingerprint(&self) -> String {
        crypto::fingerprint(&self.public_key)
    }
}

/// Tarjeta de identidad que se comparte con quienes reciben tus documentos.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentityCard {
    format: String,
    v: u32,
    pub name: String,
    #[serde(with = "crate::domain::b64")]
    pub public_key: Vec<u8>,
    pub created_at: DateTime<Utc>,
    #[serde(with = "crate::domain::b64")]
    signature: Vec<u8>,
}

impl IdentityCard {
    pub fn issue(identity: &Identity, name: &str) -> FirmarResult<Self> {
        let name = normalize_signer_name(Some(name))?
            .ok_or_else(|| FirmarError::InvalidInput("pon tu nombre antes de exportar la tarjeta".into()))?;
        let public_key = identity.public_key().to_vec();
        let created_at = Utc::now();
        let signature = identity.sign(&card_message(&public_key, &name, &created_at)).to_vec();
        Ok(Self { format: CARD_FORMAT.into(), v: CARD_VERSION, name, public_key, created_at, signature })
    }

    /// Lee y verifica una tarjeta: la firma debe ser de la clave que contiene.
    pub fn parse(bytes: &[u8]) -> FirmarResult<Self> {
        let card: Self = serde_json::from_slice(bytes)
            .map_err(|_| FirmarError::InvalidInput("el archivo no es una tarjeta de identidad de Firmar Vecipp".into()))?;
        if card.format != CARD_FORMAT || card.v != CARD_VERSION {
            return Err(FirmarError::UnsupportedFormat("versión de tarjeta no compatible".into()));
        }
        normalize_signer_name(Some(&card.name))?;
        crypto::verify(&card.public_key, &card_message(&card.public_key, &card.name, &card.created_at), &card.signature)
            .map_err(|_| FirmarError::Integrity("la tarjeta fue alterada o no es auténtica".into()))?;
        Ok(card)
    }

    pub fn to_bytes(&self) -> FirmarResult<Vec<u8>> {
        Ok(serde_json::to_vec_pretty(self)?)
    }

    #[cfg(test)]
    pub fn fingerprint(&self) -> String {
        crypto::fingerprint(&self.public_key)
    }
}

fn card_message(public_key: &[u8], name: &str, created_at: &DateTime<Utc>) -> Vec<u8> {
    let created = created_at.to_rfc3339();
    let mut m = CARD_DOMAIN.to_vec();
    m.extend_from_slice(public_key);
    for part in [name.as_bytes(), created.as_bytes()] {
        m.extend_from_slice(&(part.len() as u32).to_be_bytes());
        m.extend_from_slice(part);
    }
    m
}

impl Vault {
    pub fn my_profile(&self, key: &VaultKey) -> FirmarResult<MyProfile> {
        let path = self.root().join(PROFILE_FILE);
        if !path.exists() {
            return Ok(MyProfile::default());
        }
        Ok(serde_json::from_slice(&key.open_blob(&fs::read(path)?, PROFILE_AAD)?)?)
    }

    pub fn set_my_name(&self, key: &VaultKey, name: Option<&str>) -> FirmarResult<MyProfile> {
        let profile = MyProfile { name: normalize_signer_name(name)? };
        self.write_my_profile(key, &profile)?;
        Ok(profile)
    }

    pub(crate) fn write_my_profile(&self, key: &VaultKey, profile: &MyProfile) -> FirmarResult<()> {
        atomic_write(&self.root().join(PROFILE_FILE), &key.seal_blob(&serde_json::to_vec(profile)?, PROFILE_AAD)?)
    }

    pub fn list_contacts(&self, key: &VaultKey) -> FirmarResult<Vec<Contact>> {
        let path = self.root().join(CONTACTS_FILE);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let plain = zeroize::Zeroizing::new(key.open_blob(&fs::read(path)?, CONTACTS_AAD)?);
        Ok(serde_json::from_slice(&plain)?)
    }

    pub fn find_contact(&self, key: &VaultKey, public_key: &[u8]) -> FirmarResult<Option<Contact>> {
        Ok(self.list_contacts(key)?.into_iter().find(|c| c.public_key == public_key))
    }

    /// Añade (o actualiza el nombre de) un contacto a partir de su tarjeta.
    pub fn add_contact(&self, key: &VaultKey, card: &IdentityCard, own: &Identity) -> FirmarResult<Contact> {
        if card.public_key == own.public_key() {
            return Err(FirmarError::InvalidInput("esa tarjeta es la tuya".into()));
        }
        let mut contacts = self.list_contacts(key)?;
        contacts.retain(|c| c.public_key != card.public_key);
        if contacts.len() >= MAX_CONTACTS {
            return Err(FirmarError::InvalidInput("has alcanzado el máximo de contactos".into()));
        }
        let contact = Contact { public_key: card.public_key.clone(), name: card.name.clone(), added_at: Utc::now() };
        contacts.push(contact.clone());
        contacts.sort_by_key(|c| c.name.to_lowercase());
        self.write_contacts(key, &contacts)?;
        Ok(contact)
    }

    pub fn rename_contact(&self, key: &VaultKey, fingerprint: &str, name: &str) -> FirmarResult<()> {
        let name = normalize_signer_name(Some(name))?.ok_or_else(|| FirmarError::InvalidInput("el nombre está vacío".into()))?;
        let mut contacts = self.list_contacts(key)?;
        let contact = contacts
            .iter_mut()
            .find(|c| c.fingerprint() == fingerprint)
            .ok_or_else(|| FirmarError::NotFound("contacto".into()))?;
        contact.name = name;
        self.write_contacts(key, &contacts)
    }

    pub fn remove_contact(&self, key: &VaultKey, fingerprint: &str) -> FirmarResult<()> {
        let mut contacts = self.list_contacts(key)?;
        contacts.retain(|c| c.fingerprint() != fingerprint);
        self.write_contacts(key, &contacts)
    }

    pub(crate) fn write_contacts(&self, key: &VaultKey, contacts: &[Contact]) -> FirmarResult<()> {
        let plain = zeroize::Zeroizing::new(serde_json::to_vec(contacts)?);
        atomic_write(&self.root().join(CONTACTS_FILE), &key.seal_blob(&plain, CONTACTS_AAD)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::KeyMaterial;

    #[test]
    fn card_roundtrip_and_tamper_detection() {
        let juan = KeyMaterial::ephemeral();
        let card = IdentityCard::issue(&juan.identity, "Juan Pérez").unwrap();
        let parsed = IdentityCard::parse(&card.to_bytes().unwrap()).unwrap();
        assert_eq!(parsed.name, "Juan Pérez");
        assert_eq!(parsed.fingerprint(), crypto::fingerprint(&juan.identity.public_key()));

        // Cambiar el nombre de una tarjeta ajena la invalida.
        let forged = String::from_utf8(card.to_bytes().unwrap()).unwrap().replace("Juan Pérez", "Ana Gómez");
        assert!(IdentityCard::parse(forged.as_bytes()).is_err());
        assert!(IdentityCard::issue(&juan.identity, "  ").is_err());
    }

    #[test]
    fn contacts_are_encrypted_upserted_and_never_self() {
        let dir = tempfile::tempdir().unwrap();
        let me = KeyMaterial::ephemeral();
        let vault = Vault::open(dir.path()).unwrap();
        let juan = KeyMaterial::ephemeral();
        let card = IdentityCard::issue(&juan.identity, "Juan Pérez").unwrap();

        vault.add_contact(&me.vault_key, &card, &me.identity).unwrap();
        vault.add_contact(&me.vault_key, &card, &me.identity).unwrap();
        assert_eq!(vault.list_contacts(&me.vault_key).unwrap().len(), 1, "sin duplicados");
        let raw = fs::read(dir.path().join(CONTACTS_FILE)).unwrap();
        assert!(!raw.windows(4).any(|w| w == b"Juan"));

        let fp = card.fingerprint();
        vault.rename_contact(&me.vault_key, &fp, "Juan (contabilidad)").unwrap();
        let found = vault.find_contact(&me.vault_key, &juan.identity.public_key()).unwrap().unwrap();
        assert_eq!(found.name, "Juan (contabilidad)");

        let mine = IdentityCard::issue(&me.identity, "Yo").unwrap();
        assert!(vault.add_contact(&me.vault_key, &mine, &me.identity).is_err());

        vault.remove_contact(&me.vault_key, &fp).unwrap();
        assert!(vault.list_contacts(&me.vault_key).unwrap().is_empty());
    }

    #[test]
    fn my_name_is_validated() {
        let dir = tempfile::tempdir().unwrap();
        let me = KeyMaterial::ephemeral();
        let vault = Vault::open(dir.path()).unwrap();
        assert!(vault.my_profile(&me.vault_key).unwrap().name.is_none());
        vault.set_my_name(&me.vault_key, Some(" Yilmer Carrillo ")).unwrap();
        assert_eq!(vault.my_profile(&me.vault_key).unwrap().name.as_deref(), Some("Yilmer Carrillo"));
        assert!(vault.set_my_name(&me.vault_key, Some("a\nb")).is_err());
    }
}
