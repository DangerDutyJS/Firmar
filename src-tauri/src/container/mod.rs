//! Contenedor `X-Firmar-VaultPayload`: sobre firmado con Ed25519 que lleva los
//! metadatos cifrados con AES-256-GCM.
//!
//! Disposición en disco (embebido al final del archivo):
//!
//! ```text
//! <contenido firmado C>
//! \n%X-Firmar-VaultPayload:<base64(JSON del sobre)>\n
//! [solo PDF] startxref\n<offset de C>\n%%EOF\n
//! ```
//!
//! La línea empieza por `%`, así que para un lector de PDF es un comentario, y el
//! `startxref` repetido mantiene válida la tabla de referencias original. Los
//! visores de PNG/JPEG ignoran los bytes posteriores a `IEND`/`EOI`.
//!
//! La firma Ed25519 cubre `SHA-256(C)`, el id del documento y el texto cifrado:
//! cualquier cambio en C o en el sobre invalida la verificación.

use std::borrow::Cow;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::crypto::{self, Identity, VaultKey};
use crate::domain::{DocumentFormat, FirmarMetadataPayload};
use crate::error::{FirmarError, FirmarResult};

pub const MARKER: &[u8] = b"\n%X-Firmar-VaultPayload:";
pub const SIDECAR_EXTENSION: &str = "firmar";
/// v1: Firmar 0.1.0. v2: añade el nombre del firmante, cubierto por la firma.
const ENVELOPE_VERSION: u32 = 2;
const SIGNATURE_DOMAIN_V1: &[u8] = b"FIRMAR/envelope/v1\0";
const SIGNATURE_DOMAIN_V2: &[u8] = b"FIRMAR/envelope/v2\0";
pub const MAX_SIGNER_NAME_CHARS: usize = 80;
const PAYLOAD_AAD_DOMAIN: &[u8] = b"firmar/payload/v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Envelope {
    pub v: u32,
    pub document_id: Uuid,
    pub content_sha256: String,
    #[serde(with = "crate::domain::b64")]
    pub public_key: Vec<u8>,
    #[serde(with = "crate::domain::b64")]
    pub nonce: Vec<u8>,
    #[serde(with = "crate::domain::b64")]
    pub ciphertext: Vec<u8>,
    #[serde(with = "crate::domain::b64")]
    pub signature: Vec<u8>,
    /// Nombre que el firmante se puso (público y firmado, pero no verificado
    /// por terceros: para eso están los contactos de confianza).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signer_name: Option<String>,
}

impl Envelope {
    pub fn seal(
        vault_key: &VaultKey,
        identity: &Identity,
        content: &[u8],
        metadata: &FirmarMetadataPayload,
        signer_name: Option<&str>,
    ) -> FirmarResult<Self> {
        let signer_name = normalize_signer_name(signer_name)?;
        let digest = crypto::sha256(content);
        let plain = zeroize::Zeroizing::new(serde_json::to_vec(metadata)?);
        let sealed = vault_key.seal(&plain, &payload_aad(metadata.document_id, &digest))?;
        let message = signed_message(
            ENVELOPE_VERSION,
            metadata.document_id,
            &digest,
            &sealed.nonce,
            &sealed.ciphertext,
            signer_name.as_deref(),
        );
        Ok(Self {
            v: ENVELOPE_VERSION,
            signer_name,
            document_id: metadata.document_id,
            content_sha256: crypto::hex(&digest),
            public_key: identity.public_key().to_vec(),
            nonce: sealed.nonce.to_vec(),
            signature: identity.sign(&message).to_vec(),
            ciphertext: sealed.ciphertext,
        })
    }

    /// Verifica que el sobre corresponde a `content` y que la firma es auténtica.
    pub fn verify(&self, content: &[u8]) -> FirmarResult<()> {
        if !(1..=ENVELOPE_VERSION).contains(&self.v) {
            return Err(FirmarError::Integrity(format!("versión de sobre desconocida: {}", self.v)));
        }
        if self.v == 1 && self.signer_name.is_some() {
            return Err(FirmarError::Integrity("el sobre v1 no admite nombre de firmante".into()));
        }
        let digest = crypto::sha256(content);
        if crypto::hex(&digest) != self.content_sha256 {
            return Err(FirmarError::Integrity(
                "el contenido del documento cambió después de firmarlo".into(),
            ));
        }
        let message = signed_message(
            self.v,
            self.document_id,
            &digest,
            &self.nonce,
            &self.ciphertext,
            self.signer_name.as_deref(),
        );
        crypto::verify(&self.public_key, &message, &self.signature)
    }

    pub fn is_owned_by(&self, identity: &Identity) -> bool {
        self.public_key == identity.public_key()
    }

    pub fn signer_fingerprint(&self) -> String {
        crypto::fingerprint(&self.public_key)
    }

    /// Descifra los metadatos. Llamar solo después de [`Envelope::verify`].
    pub fn decrypt(&self, vault_key: &VaultKey, content: &[u8]) -> FirmarResult<FirmarMetadataPayload> {
        let digest = crypto::sha256(content);
        let plain = zeroize::Zeroizing::new(vault_key.open(
            &self.nonce,
            &self.ciphertext,
            &payload_aad(self.document_id, &digest),
        )?);
        let metadata: FirmarMetadataPayload = serde_json::from_slice(&plain)?;
        if metadata.document_id != self.document_id {
            return Err(FirmarError::Integrity("los metadatos no corresponden al sobre".into()));
        }
        Ok(metadata)
    }

    fn encode(&self) -> FirmarResult<String> {
        Ok(STANDARD.encode(serde_json::to_vec(self)?))
    }

    fn decode(text: &[u8]) -> FirmarResult<Self> {
        Ok(serde_json::from_slice(&STANDARD.decode(text)?)?)
    }

    pub fn to_sidecar(&self) -> FirmarResult<Vec<u8>> {
        Ok(serde_json::to_vec_pretty(self)?)
    }

    pub fn from_sidecar(bytes: &[u8]) -> FirmarResult<Self> {
        Ok(serde_json::from_slice(bytes)?)
    }
}

fn payload_aad(document_id: Uuid, digest: &[u8; 32]) -> Vec<u8> {
    let mut aad = PAYLOAD_AAD_DOMAIN.to_vec();
    aad.extend_from_slice(document_id.as_bytes());
    aad.extend_from_slice(digest);
    aad
}

/// Nombre de firmante válido: sin saltos de línea y de longitud razonable.
pub fn normalize_signer_name(name: Option<&str>) -> FirmarResult<Option<String>> {
    let Some(name) = name.map(str::trim).filter(|n| !n.is_empty()) else { return Ok(None) };
    if name.chars().count() > MAX_SIGNER_NAME_CHARS || name.chars().any(char::is_control) {
        return Err(FirmarError::InvalidInput(format!(
            "el nombre debe tener como mucho {MAX_SIGNER_NAME_CHARS} caracteres, sin saltos de línea"
        )));
    }
    Ok(Some(name.to_owned()))
}

fn signed_message(
    version: u32,
    document_id: Uuid,
    digest: &[u8; 32],
    nonce: &[u8],
    ciphertext: &[u8],
    signer_name: Option<&str>,
) -> Vec<u8> {
    let domain = if version == 1 { SIGNATURE_DOMAIN_V1 } else { SIGNATURE_DOMAIN_V2 };
    let mut m = Vec::with_capacity(domain.len() + 16 + 32 + nonce.len() + ciphertext.len() + 96);
    m.extend_from_slice(domain);
    m.extend_from_slice(document_id.as_bytes());
    m.extend_from_slice(digest);
    m.extend_from_slice(&(nonce.len() as u32).to_be_bytes());
    m.extend_from_slice(nonce);
    m.extend_from_slice(&(ciphertext.len() as u32).to_be_bytes());
    m.extend_from_slice(ciphertext);
    if version >= 2 {
        let name = signer_name.unwrap_or("").as_bytes();
        m.extend_from_slice(&(name.len() as u32).to_be_bytes());
        m.extend_from_slice(name);
    }
    m
}

/// Añade el sobre al final de `content`.
pub fn embed(content: &[u8], envelope: &Envelope, format: DocumentFormat) -> FirmarResult<Vec<u8>> {
    let encoded = envelope.encode()?;
    if format == DocumentFormat::Docx {
        return embed_zip_comment(content, &encoded);
    }
    let suffix = expected_suffix(content, format)?;
    let mut out = Vec::with_capacity(content.len() + MARKER.len() + encoded.len() + 1 + suffix.len());
    out.extend_from_slice(content);
    out.extend_from_slice(MARKER);
    out.extend_from_slice(encoded.as_bytes());
    out.push(b'\n');
    out.extend_from_slice(&suffix);
    Ok(out)
}

pub struct Extracted<'a> {
    /// Bytes exactos que se firmaron.
    pub content: Cow<'a, [u8]>,
    pub envelope: FirmarResult<Envelope>,
    /// `true` si hay bytes añadidos después del sobre.
    pub appended_after: bool,
}

/// Busca el último sobre embebido. `None` si el archivo nunca fue firmado.
pub fn extract(bytes: &[u8], format: DocumentFormat) -> Option<Extracted<'_>> {
    if format == DocumentFormat::Docx {
        return extract_zip_comment(bytes);
    }
    let pos = rfind(bytes, MARKER)?;
    let content = Cow::Borrowed(&bytes[..pos]);
    let after_marker = &bytes[pos + MARKER.len()..];
    let Some(line_end) = after_marker.iter().position(|&b| b == b'\n') else {
        return Some(Extracted {
            content,
            envelope: Err(FirmarError::Integrity("contenedor de Firmar Vecipp truncado".into())),
            appended_after: false,
        });
    };
    let envelope = Envelope::decode(&after_marker[..line_end])
        .map_err(|e| FirmarError::Integrity(format!("contenedor de Firmar Vecipp ilegible: {e}")));
    let rest = &after_marker[line_end + 1..];
    let appended_after = match expected_suffix(&content, format) {
        Ok(suffix) => rest != suffix.as_slice(),
        Err(_) => true,
    };
    Some(Extracted { content, envelope, appended_after })
}

// ---------------------------------------------------------------------------
// DOCX: el sobre vive en el comentario del registro EOCD del ZIP. Es parte
// estándar del formato, así que Word y cualquier descompresor lo ignoran, y
// el contenido firmado es el mismo ZIP con comentario vacío.
// ---------------------------------------------------------------------------

const EOCD_SIGNATURE: &[u8] = b"PK\x05\x06";
const EOCD_LEN: usize = 22;
const ZIP_COMMENT_MARKER: &[u8] = b"X-Firmar-VaultPayload:";

fn embed_zip_comment(content: &[u8], encoded: &str) -> FirmarResult<Vec<u8>> {
    let eocd = content
        .len()
        .checked_sub(EOCD_LEN)
        .filter(|&at| &content[at..at + 4] == EOCD_SIGNATURE && content[at + 20..at + 22] == [0, 0])
        .ok_or_else(|| FirmarError::UnsupportedFormat("el .docx no termina en un directorio ZIP sin comentario".into()))?;
    let comment_len = ZIP_COMMENT_MARKER.len() + encoded.len();
    let comment_len = u16::try_from(comment_len)
        .map_err(|_| FirmarError::InvalidInput("los metadatos de la firma no caben en el .docx".into()))?;

    let mut out = Vec::with_capacity(content.len() + comment_len as usize);
    out.extend_from_slice(&content[..eocd + 20]);
    out.extend_from_slice(&comment_len.to_le_bytes());
    out.extend_from_slice(ZIP_COMMENT_MARKER);
    out.extend_from_slice(encoded.as_bytes());
    Ok(out)
}

fn extract_zip_comment(bytes: &[u8]) -> Option<Extracted<'_>> {
    let eocd = bytes.windows(EOCD_SIGNATURE.len()).enumerate().rev().find_map(|(at, w)| {
        (w == EOCD_SIGNATURE && bytes.get(at + EOCD_LEN..)?.starts_with(ZIP_COMMENT_MARKER)).then_some(at)
    })?;
    let comment_len = u16::from_le_bytes([bytes[eocd + 20], bytes[eocd + 21]]) as usize;
    let comment_end = eocd + EOCD_LEN + comment_len;

    let mut content = bytes[..eocd + 20].to_vec();
    content.extend_from_slice(&[0, 0]);

    let envelope = match bytes.get(eocd + EOCD_LEN + ZIP_COMMENT_MARKER.len()..comment_end) {
        Some(encoded) => Envelope::decode(encoded)
            .map_err(|e| FirmarError::Integrity(format!("contenedor de Firmar Vecipp ilegible: {e}"))),
        None => Err(FirmarError::Integrity("contenedor de Firmar Vecipp truncado".into())),
    };
    Some(Extracted { content: Cow::Owned(content), envelope, appended_after: bytes.len() > comment_end })
}

fn expected_suffix(content: &[u8], format: DocumentFormat) -> FirmarResult<Vec<u8>> {
    match format {
        DocumentFormat::Pdf => {
            let offset = last_startxref(content)
                .ok_or_else(|| FirmarError::Pdf("no se encontró startxref en el PDF".into()))?;
            Ok(format!("startxref\n{offset}\n%%EOF\n").into_bytes())
        }
        _ => Ok(Vec::new()),
    }
}

fn last_startxref(pdf: &[u8]) -> Option<u64> {
    let pos = rfind(pdf, b"startxref")?;
    let tail = &pdf[pos + b"startxref".len()..];
    let digits: String = tail
        .iter()
        .skip_while(|b| b.is_ascii_whitespace())
        .take_while(|b| b.is_ascii_digit())
        .map(|&b| b as char)
        .collect();
    digits.parse().ok()
}

fn rfind(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.len() > haystack.len() {
        return None;
    }
    haystack.windows(needle.len()).rposition(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::SignaturePlacement;
    use crate::storage::KeyMaterial;
    use chrono::Utc;

    fn metadata() -> FirmarMetadataPayload {
        FirmarMetadataPayload {
            version: 1,
            document_id: Uuid::new_v4(),
            profile_id: Uuid::new_v4(),
            profile_name: "Firma".into(),
            source_file_name: "a.png".into(),
            format: DocumentFormat::Png,
            placements: vec![SignaturePlacement { page: 0, x: 0.1, y: 0.1, width: 0.2, height: 0.1, caption: None, caption_align: None }],
            signed_at: Utc::now(),
            original_sha256: "00".into(),
        }
    }

    #[test]
    fn embed_extract_verify_decrypt() {
        let keys = KeyMaterial::ephemeral();
        let content = b"\x89PNG fake image bytes".to_vec();
        let meta = metadata();
        let env = Envelope::seal(&keys.vault_key, &keys.identity, &content, &meta, None).unwrap();
        let file = embed(&content, &env, DocumentFormat::Png).unwrap();

        let ex = extract(&file, DocumentFormat::Png).unwrap();
        assert_eq!(&*ex.content, content.as_slice());
        assert!(!ex.appended_after);
        let env = ex.envelope.unwrap();
        env.verify(&ex.content).unwrap();
        assert!(env.is_owned_by(&keys.identity));
        assert_eq!(env.decrypt(&keys.vault_key, &ex.content).unwrap().document_id, meta.document_id);

        let stranger = KeyMaterial::ephemeral();
        assert!(!env.is_owned_by(&stranger.identity));
        assert!(env.decrypt(&stranger.vault_key, &ex.content).is_err());
    }

    #[test]
    fn detects_content_tampering_and_appends() {
        let keys = KeyMaterial::ephemeral();
        let content = b"contenido original".to_vec();
        let env = Envelope::seal(&keys.vault_key, &keys.identity, &content, &metadata(), None).unwrap();
        let mut file = embed(&content, &env, DocumentFormat::Binary).unwrap();

        file[0] ^= 1;
        let ex = extract(&file, DocumentFormat::Binary).unwrap();
        assert!(ex.envelope.unwrap().verify(&ex.content).is_err());
        file[0] ^= 1;

        file.extend_from_slice(b"datos extra");
        let ex = extract(&file, DocumentFormat::Binary).unwrap();
        assert!(ex.appended_after);
        assert!(ex.envelope.unwrap().verify(&ex.content).is_ok());
    }

    #[test]
    fn signer_name_is_signed_and_cannot_be_swapped() {
        let keys = KeyMaterial::ephemeral();
        let content = b"doc".to_vec();
        let mut env = Envelope::seal(&keys.vault_key, &keys.identity, &content, &metadata(), Some(" Juan Pérez ")).unwrap();
        assert_eq!(env.signer_name.as_deref(), Some("Juan Pérez"));
        env.verify(&content).unwrap();

        env.signer_name = Some("Otra Persona".into());
        assert!(env.verify(&content).is_err(), "cambiar el nombre invalida la firma");
        env.signer_name = None;
        assert!(env.verify(&content).is_err(), "quitar el nombre también");

        assert!(Envelope::seal(&keys.vault_key, &keys.identity, &content, &metadata(), Some("a\nb")).is_err());
    }

    #[test]
    fn version_1_envelopes_still_verify() {
        // Sobre tal como lo producía Firmar 0.1.0.
        let keys = KeyMaterial::ephemeral();
        let content = b"documento antiguo".to_vec();
        let meta = metadata();
        let digest = crypto::sha256(&content);
        let sealed = keys.vault_key.seal(&serde_json::to_vec(&meta).unwrap(), &payload_aad(meta.document_id, &digest)).unwrap();
        let message = signed_message(1, meta.document_id, &digest, &sealed.nonce, &sealed.ciphertext, None);
        let v1 = Envelope {
            v: 1,
            document_id: meta.document_id,
            content_sha256: crypto::hex(&digest),
            public_key: keys.identity.public_key().to_vec(),
            nonce: sealed.nonce.to_vec(),
            ciphertext: sealed.ciphertext,
            signature: keys.identity.sign(&message).to_vec(),
            signer_name: None,
        };
        let json = serde_json::to_string(&v1).unwrap();
        assert!(!json.contains("signerName"), "el JSON v1 no cambia");
        v1.verify(&content).unwrap();
        assert_eq!(v1.decrypt(&keys.vault_key, &content).unwrap().document_id, meta.document_id);
    }

    #[test]
    fn forged_signature_is_rejected() {
        let keys = KeyMaterial::ephemeral();
        let attacker = KeyMaterial::ephemeral();
        let content = b"doc".to_vec();
        let mut env = Envelope::seal(&keys.vault_key, &keys.identity, &content, &metadata(), None).unwrap();
        env.public_key = attacker.identity.public_key().to_vec();
        assert!(env.verify(&content).is_err());
    }

    #[test]
    fn docx_envelope_lives_in_zip_comment() {
        let keys = KeyMaterial::ephemeral();
        let docx = crate::docx_engine::tests::sample_docx("<w:p/>");
        let env = Envelope::seal(&keys.vault_key, &keys.identity, &docx, &metadata(), None).unwrap();
        let file = embed(&docx, &env, DocumentFormat::Docx).unwrap();

        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(file.as_slice())).unwrap();
        assert!(zip.comment().starts_with(ZIP_COMMENT_MARKER), "el comentario es parte válida del ZIP");
        assert!(zip.by_name("word/document.xml").is_ok());

        let ex = extract(&file, DocumentFormat::Docx).unwrap();
        assert_eq!(&*ex.content, docx.as_slice(), "se recupera el ZIP firmado byte a byte");
        assert!(!ex.appended_after);
        ex.envelope.unwrap().verify(&ex.content).unwrap();

        assert!(extract(&docx, DocumentFormat::Docx).is_none());
    }

    #[test]
    fn pdf_suffix_repeats_startxref() {
        let pdf = b"%PDF-1.7\n...xref...\nstartxref\n1234\n%%EOF\n".to_vec();
        let keys = KeyMaterial::ephemeral();
        let env = Envelope::seal(&keys.vault_key, &keys.identity, &pdf, &metadata(), None).unwrap();
        let file = embed(&pdf, &env, DocumentFormat::Pdf).unwrap();
        assert!(file.ends_with(b"startxref\n1234\n%%EOF\n"));
        assert!(!extract(&file, DocumentFormat::Pdf).unwrap().appended_after);
    }
}
