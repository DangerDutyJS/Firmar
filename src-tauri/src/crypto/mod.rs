//! Primitivas criptográficas: AES-256-GCM, Ed25519 y SHA-256.

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::error::{FirmarError, FirmarResult};

pub const KEY_LEN: usize = 32;
pub const NONCE_LEN: usize = 12;

pub fn random_bytes<const N: usize>() -> FirmarResult<[u8; N]> {
    let mut buf = [0u8; N];
    getrandom::fill(&mut buf).map_err(|e| FirmarError::Crypto(format!("RNG del sistema: {e}")))?;
    Ok(buf)
}

pub fn sha256(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data).into()
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Clave simétrica de la bóveda. Se borra de memoria al soltarse.
pub struct VaultKey(Zeroizing<[u8; KEY_LEN]>);

/// Texto cifrado con su nonce.
pub struct Sealed {
    pub nonce: [u8; NONCE_LEN],
    pub ciphertext: Vec<u8>,
}

impl VaultKey {
    pub fn generate() -> FirmarResult<Self> {
        Ok(Self(Zeroizing::new(random_bytes()?)))
    }

    pub fn from_bytes(bytes: &[u8]) -> FirmarResult<Self> {
        let arr: [u8; KEY_LEN] = bytes
            .try_into()
            .map_err(|_| FirmarError::Crypto("longitud de clave de bóveda inválida".into()))?;
        Ok(Self(Zeroizing::new(arr)))
    }

    pub fn as_bytes(&self) -> &[u8; KEY_LEN] {
        &self.0
    }

    fn cipher(&self) -> FirmarResult<Aes256Gcm> {
        Aes256Gcm::new_from_slice(self.0.as_ref())
            .map_err(|_| FirmarError::Crypto("clave AES inválida".into()))
    }

    pub fn seal(&self, plaintext: &[u8], aad: &[u8]) -> FirmarResult<Sealed> {
        let nonce = random_bytes::<NONCE_LEN>()?;
        let ciphertext = self
            .cipher()?
            .encrypt(&Nonce::from(nonce), Payload { msg: plaintext, aad })
            .map_err(|_| FirmarError::Crypto("fallo al cifrar".into()))?;
        Ok(Sealed { nonce, ciphertext })
    }

    pub fn open(&self, nonce: &[u8], ciphertext: &[u8], aad: &[u8]) -> FirmarResult<Vec<u8>> {
        let nonce: [u8; NONCE_LEN] = nonce
            .try_into()
            .map_err(|_| FirmarError::Crypto("nonce inválido".into()))?;
        self.cipher()?
            .decrypt(&Nonce::from(nonce), Payload { msg: ciphertext, aad })
            .map_err(|_| FirmarError::Crypto("no se pudo descifrar (clave incorrecta o datos alterados)".into()))
    }

    /// Formato compacto `nonce || ciphertext` para archivos de la bóveda.
    pub fn seal_blob(&self, plaintext: &[u8], aad: &[u8]) -> FirmarResult<Vec<u8>> {
        let sealed = self.seal(plaintext, aad)?;
        let mut out = Vec::with_capacity(NONCE_LEN + sealed.ciphertext.len());
        out.extend_from_slice(&sealed.nonce);
        out.extend_from_slice(&sealed.ciphertext);
        Ok(out)
    }

    pub fn open_blob(&self, blob: &[u8], aad: &[u8]) -> FirmarResult<Vec<u8>> {
        if blob.len() < NONCE_LEN {
            return Err(FirmarError::Crypto("blob cifrado truncado".into()));
        }
        let (nonce, ct) = blob.split_at(NONCE_LEN);
        self.open(nonce, ct, aad)
    }
}

/// Identidad de firma Ed25519 del propietario.
pub struct Identity(SigningKey);

impl Identity {
    pub fn generate() -> FirmarResult<Self> {
        let seed = Zeroizing::new(random_bytes::<KEY_LEN>()?);
        Ok(Self(SigningKey::from_bytes(&seed)))
    }

    pub fn from_seed(seed: &[u8]) -> FirmarResult<Self> {
        let seed: Zeroizing<[u8; KEY_LEN]> = Zeroizing::new(
            seed.try_into()
                .map_err(|_| FirmarError::Crypto("longitud de semilla Ed25519 inválida".into()))?,
        );
        Ok(Self(SigningKey::from_bytes(&seed)))
    }

    pub fn seed(&self) -> Zeroizing<[u8; KEY_LEN]> {
        Zeroizing::new(self.0.to_bytes())
    }

    pub fn public_key(&self) -> [u8; 32] {
        self.0.verifying_key().to_bytes()
    }

    pub fn sign(&self, message: &[u8]) -> [u8; 64] {
        self.0.sign(message).to_bytes()
    }
}

pub fn verify(public_key: &[u8], message: &[u8], signature: &[u8]) -> FirmarResult<()> {
    let pk: [u8; 32] = public_key
        .try_into()
        .map_err(|_| FirmarError::Integrity("clave pública malformada".into()))?;
    let sig: [u8; 64] = signature
        .try_into()
        .map_err(|_| FirmarError::Integrity("firma malformada".into()))?;
    let vk = VerifyingKey::from_bytes(&pk)
        .map_err(|_| FirmarError::Integrity("clave pública inválida".into()))?;
    vk.verify_strict(message, &Signature::from_bytes(&sig))
        .map_err(|_| FirmarError::Integrity("la firma Ed25519 no es válida".into()))
}

/// Huella corta y legible de una clave pública (primeros 8 bytes del SHA-256).
pub fn fingerprint(public_key: &[u8]) -> String {
    let h = sha256(public_key);
    h[..8]
        .chunks(2)
        .map(|c| hex(c).to_uppercase())
        .collect::<Vec<_>>()
        .join(":")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aead_roundtrip_and_aad_binding() {
        let key = VaultKey::generate().unwrap();
        let blob = key.seal_blob(b"hola", b"ctx").unwrap();
        assert_eq!(key.open_blob(&blob, b"ctx").unwrap(), b"hola");
        assert!(key.open_blob(&blob, b"otro").is_err());

        let other = VaultKey::generate().unwrap();
        assert!(other.open_blob(&blob, b"ctx").is_err());
    }

    #[test]
    fn ed25519_sign_verify() {
        let id = Identity::generate().unwrap();
        let sig = id.sign(b"mensaje");
        assert!(verify(&id.public_key(), b"mensaje", &sig).is_ok());
        assert!(verify(&id.public_key(), b"mensajE", &sig).is_err());

        let restored = Identity::from_seed(id.seed().as_ref()).unwrap();
        assert_eq!(restored.public_key(), id.public_key());
    }
}
