//! Material de claves persistido en el llavero del SO (Windows Credential
//! Manager, macOS Keychain, Secret Service en Linux).

use keyring::Entry;

use crate::crypto::{Identity, VaultKey};
use crate::error::{FirmarError, FirmarResult};

const SERVICE: &str = "com.firmar.app";
const VAULT_KEY_ENTRY: &str = "vault-master-key";
const IDENTITY_ENTRY: &str = "ed25519-identity";

pub struct KeyMaterial {
    pub vault_key: VaultKey,
    pub identity: Identity,
}

impl KeyMaterial {
    /// Carga las claves del llavero o las crea en el primer arranque.
    pub fn load_or_create() -> FirmarResult<Self> {
        let vault_key = load_or_create_secret(VAULT_KEY_ENTRY, || {
            let k = VaultKey::generate()?;
            Ok(k.as_bytes().to_vec())
        })
        .and_then(|bytes| VaultKey::from_bytes(&bytes))?;

        let identity = load_or_create_secret(IDENTITY_ENTRY, || {
            let id = Identity::generate()?;
            Ok(id.seed().to_vec())
        })
        .and_then(|bytes| Identity::from_seed(&bytes))?;

        Ok(Self { vault_key, identity })
    }

    /// Sobrescribe las claves del llavero con estas (restauración de copia).
    pub fn store(&self) -> FirmarResult<()> {
        Entry::new(SERVICE, VAULT_KEY_ENTRY)?.set_secret(self.vault_key.as_bytes())?;
        Entry::new(SERVICE, IDENTITY_ENTRY)?.set_secret(self.identity.seed().as_ref())?;
        Ok(())
    }

    /// Elimina las claves del llavero (borrado total de datos). No falla si
    /// ya no existían.
    pub fn delete_from_keyring() -> FirmarResult<()> {
        for name in [VAULT_KEY_ENTRY, IDENTITY_ENTRY] {
            match Entry::new(SERVICE, name)?.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }

    /// Claves efímeras, solo para pruebas.
    #[cfg(test)]
    pub fn ephemeral() -> Self {
        Self {
            vault_key: VaultKey::generate().unwrap(),
            identity: Identity::generate().unwrap(),
        }
    }
}

fn load_or_create_secret(
    name: &str,
    create: impl FnOnce() -> FirmarResult<Vec<u8>>,
) -> FirmarResult<zeroize::Zeroizing<Vec<u8>>> {
    let entry = Entry::new(SERVICE, name)?;
    match entry.get_secret() {
        Ok(secret) => Ok(zeroize::Zeroizing::new(secret)),
        Err(keyring::Error::NoEntry) => {
            let secret = zeroize::Zeroizing::new(create()?);
            entry.set_secret(&secret)?;
            // Releer confirma que el almacén realmente persistió el secreto.
            let stored = zeroize::Zeroizing::new(entry.get_secret()?);
            if *stored != *secret {
                return Err(FirmarError::Keyring(format!(
                    "el llavero no devolvió el secreto '{name}' que se acaba de guardar"
                )));
            }
            Ok(stored)
        }
        Err(e) => Err(e.into()),
    }
}
