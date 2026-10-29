//! Casos de uso: firmar, inspeccionar y re-posicionar. Orquesta dominio,
//! criptografía, almacenamiento y motores de formato.

use std::borrow::Cow;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use image::RgbaImage;
use uuid::Uuid;

use crate::container::{self, Envelope, SIDECAR_EXTENSION};
use crate::crypto;
use crate::domain::{
    DocumentFormat, DocumentInspection, FirmarMetadataPayload, SignaturePlacement, SignatureStatus,
    SignedDocument,
};
use crate::error::{FirmarError, FirmarResult};
use crate::storage::{HistoryEntry, KeyMaterial, Vault, atomic_write};
use crate::{docx_engine, image_engine, pdf_engine};

const PAYLOAD_VERSION: u32 = 1;
const MAX_PLACEMENTS: usize = 200;

pub struct SigningService<'a> {
    pub keys: &'a KeyMaterial,
    pub vault: &'a Vault,
}

/// Documento firmado ya leído, verificado y (si es nuestro) descifrado.
struct Opened {
    envelope: Envelope,
    metadata: Option<FirmarMetadataPayload>,
}

impl SigningService<'_> {
    pub fn sign(
        &self,
        input: &Path,
        output: &Path,
        profile_id: Uuid,
        placements: &[SignaturePlacement],
    ) -> FirmarResult<SignedDocument> {
        let original = fs::read(input)?;
        let format = DocumentFormat::detect(&original);
        if self.read_container(input, &original, format)?.is_some() {
            return Err(FirmarError::InvalidInput(
                "este archivo ya está firmado con Firmar Vecipp; usa «re-posicionar» para cambiar la firma".into(),
            ));
        }
        self.produce(Uuid::new_v4(), input, output, &original, format, profile_id, placements)
    }

    /// Vuelve a firmar partiendo de la copia original guardada en la bóveda, de
    /// modo que la firma anterior desaparece en vez de acumularse.
    pub fn resign(
        &self,
        signed: &Path,
        output: &Path,
        profile_id: Uuid,
        placements: &[SignaturePlacement],
    ) -> FirmarResult<SignedDocument> {
        let metadata = self.owned_metadata(signed)?;
        let original = self.vault.load_original(&self.keys.vault_key, metadata.document_id)?;
        if crypto::hex(&crypto::sha256(&original)) != metadata.original_sha256 {
            return Err(FirmarError::Integrity("la copia original de la bóveda no coincide".into()));
        }
        let source = PathBuf::from(&metadata.source_file_name);
        self.produce(metadata.document_id, &source, output, &original, metadata.format, profile_id, placements)
    }

    /// Firma varios documentos con la misma plantilla de posición. Cada archivo
    /// se guarda junto a su original; los errores no detienen el lote.
    pub fn sign_batch(
        &self,
        inputs: &[PathBuf],
        profile_id: Uuid,
        template: &PlacementTemplate,
        mut progress: impl FnMut(usize, &Path),
    ) -> FirmarResult<Vec<BatchItem>> {
        let profile = self.vault.get_profile(&self.keys.vault_key, profile_id)?;
        let signature_aspect = f64::from(profile.height) / f64::from(profile.width);
        let mut results = Vec::with_capacity(inputs.len());
        for (i, input) in inputs.iter().enumerate() {
            progress(i, input);
            let outcome = fs::read(input).map_err(FirmarError::from).and_then(|bytes| {
                let format = DocumentFormat::detect(&bytes);
                let placements = template.placements(&bytes, format, signature_aspect)?;
                let output = default_output_path(input);
                self.sign(input, &output, profile_id, &placements)
            });
            results.push(match outcome {
                Ok(signed) => BatchItem { path: input.display().to_string(), output_path: Some(signed.output_path), error: None },
                Err(e) => BatchItem { path: input.display().to_string(), output_path: None, error: Some(e.to_string()) },
            });
        }
        progress(inputs.len(), Path::new(""));
        Ok(results)
    }

    /// Bytes originales (sin firma) de un documento nuestro, para el visor.
    pub fn original(&self, signed: &Path) -> FirmarResult<Vec<u8>> {
        let metadata = self.owned_metadata(signed)?;
        self.vault.load_original(&self.keys.vault_key, metadata.document_id)
    }

    /// Tamaño de página de un .docx (primera y última), para el visor esquemático.
    pub fn docx_layout(&self, path: &Path) -> FirmarResult<docx_engine::DocxLayout> {
        let bytes = fs::read(path)?;
        if DocumentFormat::detect(&bytes) != DocumentFormat::Docx {
            return Err(FirmarError::UnsupportedFormat("el archivo no es un documento de Word".into()));
        }
        docx_engine::layout(&bytes)
    }

    pub fn inspect(&self, path: &Path) -> FirmarResult<DocumentInspection> {
        let bytes = fs::read(path)?;
        let format = DocumentFormat::detect(&bytes);
        let status = match self.read_container(path, &bytes, format) {
            Ok(None) => SignatureStatus::Unsigned,
            Ok(Some((opened, appended_after))) => {
                let owned = opened.envelope.is_owned_by(&self.keys.identity);
                let signer_fingerprint = opened.envelope.signer_fingerprint();
                let signer_name = opened.envelope.signer_name.clone();
                let contact_name = if owned {
                    None
                } else {
                    self.vault.find_contact(&self.keys.vault_key, &opened.envelope.public_key)?.map(|c| c.name)
                };
                let metadata = opened.metadata;
                if appended_after {
                    SignatureStatus::ModifiedAfterSigning { owned, signer_fingerprint, signer_name, contact_name, metadata }
                } else {
                    SignatureStatus::Valid { owned, signer_fingerprint, signer_name, contact_name, metadata }
                }
            }
            Err(FirmarError::Integrity(reason)) => SignatureStatus::Tampered { reason },
            Err(e) => return Err(e),
        };
        Ok(DocumentInspection {
            path: path.display().to_string(),
            file_name: file_name(path),
            format,
            size: bytes.len() as u64,
            status,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn produce(
        &self,
        document_id: Uuid,
        source: &Path,
        output: &Path,
        original: &[u8],
        format: DocumentFormat,
        profile_id: Uuid,
        placements: &[SignaturePlacement],
    ) -> FirmarResult<SignedDocument> {
        if placements.len() > MAX_PLACEMENTS {
            return Err(FirmarError::InvalidInput("demasiadas firmas en un mismo documento".into()));
        }
        for p in placements {
            p.validate()?;
            image_engine::caption_lines(p.caption.as_deref())?;
        }
        if format.supports_visual_stamp() && placements.is_empty() {
            return Err(FirmarError::InvalidInput("coloca la firma en el documento antes de firmar".into()));
        }

        let profile = self.vault.get_profile(&self.keys.vault_key, profile_id)?;
        // Se limpia también aquí por si el perfil viene de una versión anterior.
        let signature: RgbaImage = image_engine::clean_signature(&image::load_from_memory(&profile.png)?.into_rgba8())?;
        let art = image_engine::SignatureArt::new(signature);

        let content = match format {
            DocumentFormat::Pdf => pdf_engine::stamp(original, &art, placements)?,
            DocumentFormat::Png | DocumentFormat::Jpeg => image_engine::stamp(original, format, &art, placements)?,
            DocumentFormat::Docx => docx_engine::stamp(original, &art, placements)?,
            DocumentFormat::Binary => original.to_vec(),
        };

        let signed_at = Utc::now();
        let metadata = FirmarMetadataPayload {
            version: PAYLOAD_VERSION,
            document_id,
            profile_id: profile.id,
            profile_name: profile.name.clone(),
            source_file_name: file_name(source),
            format,
            placements: if format.supports_visual_stamp() { placements.to_vec() } else { Vec::new() },
            signed_at,
            original_sha256: crypto::hex(&crypto::sha256(original)),
        };
        let signer_name = self.vault.my_profile(&self.keys.vault_key)?.name;
        let envelope =
            Envelope::seal(&self.keys.vault_key, &self.keys.identity, &content, &metadata, signer_name.as_deref())?;

        // La copia original va primero: si falla, no dejamos un documento
        // firmado imposible de re-posicionar.
        self.vault.store_original(&self.keys.vault_key, document_id, original)?;

        let sidecar_path = if format.supports_visual_stamp() {
            atomic_write(output, &container::embed(&content, &envelope, format)?)?;
            None
        } else {
            atomic_write(output, &content)?;
            let sidecar = sidecar_path(output);
            atomic_write(&sidecar, &envelope.to_sidecar()?)?;
            Some(sidecar.display().to_string())
        };

        // El historial es secundario: si falla, el documento ya está firmado.
        let _ = self.vault.record_history(
            &self.keys.vault_key,
            HistoryEntry {
                document_id,
                file_name: file_name(output),
                output_path: output.display().to_string(),
                format,
                signed_at,
                signatures: metadata.placements.len(),
            },
        );

        Ok(SignedDocument {
            document_id,
            output_path: output.display().to_string(),
            sidecar_path,
            format,
            content_sha256: envelope.content_sha256,
            signed_at,
        })
    }

    /// Lee y verifica el contenedor. `Ok(None)` = sin firmar; errores de
    /// integridad salen como `FirmarError::Integrity`.
    fn read_container(
        &self,
        path: &Path,
        bytes: &[u8],
        format: DocumentFormat,
    ) -> FirmarResult<Option<(Opened, bool)>> {
        let (envelope, content, appended_after) = if format.supports_visual_stamp() {
            match container::extract(bytes, format) {
                None => return Ok(None),
                Some(ex) => (ex.envelope?, ex.content, ex.appended_after),
            }
        } else {
            let sidecar = sidecar_path(path);
            if !sidecar.exists() {
                return Ok(None);
            }
            let envelope = Envelope::from_sidecar(&fs::read(&sidecar)?)
                .map_err(|e| FirmarError::Integrity(format!("sidecar ilegible: {e}")))?;
            (envelope, Cow::Borrowed(bytes), false)
        };

        envelope.verify(&content)?;
        let metadata = if envelope.is_owned_by(&self.keys.identity) {
            Some(envelope.decrypt(&self.keys.vault_key, &content)?)
        } else {
            None
        };
        Ok(Some((Opened { envelope, metadata }, appended_after)))
    }

    fn owned_metadata(&self, signed: &Path) -> FirmarResult<FirmarMetadataPayload> {
        let bytes = fs::read(signed)?;
        let format = DocumentFormat::detect(&bytes);
        let (opened, _) = self
            .read_container(signed, &bytes, format)?
            .ok_or_else(|| FirmarError::InvalidInput("el archivo no está firmado con Firmar Vecipp".into()))?;
        opened.metadata.ok_or(FirmarError::NotOwner)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PageTarget {
    First,
    Last,
    All,
}

/// Posición común para un lote: dónde (x, y, ancho) y en qué páginas. El alto
/// se calcula para cada página según la proporción de la firma.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlacementTemplate {
    pub pages: PageTarget,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    #[serde(default)]
    pub caption: Option<String>,
    #[serde(default)]
    pub caption_align: Option<crate::domain::CaptionAlign>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchItem {
    pub path: String,
    pub output_path: Option<String>,
    pub error: Option<String>,
}

impl PlacementTemplate {
    fn placements(&self, bytes: &[u8], format: DocumentFormat, signature_aspect: f64) -> FirmarResult<Vec<SignaturePlacement>> {
        // (índice de página, ancho, alto) de las páginas elegidas.
        let pages: Vec<(u32, f64, f64)> = match format {
            DocumentFormat::Pdf => {
                let sizes = pdf_engine::page_sizes(bytes)?;
                let indexed = (0u32..).zip(sizes).map(|(i, (w, h))| (i, w, h));
                match self.pages {
                    PageTarget::All => indexed.collect(),
                    PageTarget::First => indexed.take(1).collect(),
                    PageTarget::Last => indexed.last().into_iter().collect(),
                }
            }
            DocumentFormat::Png | DocumentFormat::Jpeg => {
                let (w, h) = image::load_from_memory(bytes)?.into_rgba8().dimensions();
                vec![(0, f64::from(w), f64::from(h))]
            }
            DocumentFormat::Docx => {
                // En Word solo existen la primera y la última página.
                let layout = docx_engine::layout(bytes)?;
                let first = layout.pages[0];
                let last = *layout.pages.last().unwrap_or(&first);
                let single = layout.pages.len() == 1;
                let first = (docx_engine::FIRST_PAGE, first.width, first.height);
                let last = (if single { docx_engine::FIRST_PAGE } else { docx_engine::LAST_PAGE }, last.width, last.height);
                match self.pages {
                    PageTarget::First => vec![first],
                    PageTarget::Last => vec![last],
                    PageTarget::All if single => vec![first],
                    PageTarget::All => vec![first, last],
                }
            }
            DocumentFormat::Binary => return Ok(Vec::new()),
        };
        let caption_lines = image_engine::caption_lines(self.caption.as_deref())?.len();
        Ok(pages
            .into_iter()
            .map(|(page, w, h)| {
                let mut p = fit_placement(page, w / h, signature_aspect, caption_lines, self.width, self.x, self.y, self.caption.clone());
                p.caption_align = self.caption_align;
                p
            })
            .collect())
    }
}

/// Misma regla que `sizedPlacement` en el visor: el alto respeta la proporción
/// de la firma (más el texto) y el rectángulo no se sale de la página.
#[allow(clippy::too_many_arguments)]
fn fit_placement(
    page: u32,
    page_aspect_wh: f64,
    signature_aspect: f64,
    caption_lines: usize,
    width: f64,
    x: f64,
    y: f64,
    caption: Option<String>,
) -> SignaturePlacement {
    let caption_ratio = (image_engine::CAPTION_LINE_RATIO * caption_lines as f64).min(image_engine::CAPTION_MAX_RATIO);
    let mut w = width.clamp(0.03, 0.9);
    let mut h = w * signature_aspect * page_aspect_wh / (1.0 - caption_ratio);
    if h > 0.9 {
        w *= 0.9 / h;
        h = 0.9;
    }
    SignaturePlacement { page, x: x.clamp(0.0, 1.0 - w), y: y.clamp(0.0, 1.0 - h), width: w, height: h, caption, caption_align: None }
}

/// `contrato.pdf` → `contrato.firmado.pdf`, o `contrato.firmado (2).pdf` si ya existe.
pub fn default_output_path(input: &Path) -> PathBuf {
    let stem = input.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "documento".into());
    let ext = input.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    let dir = input.parent().unwrap_or_else(|| Path::new(""));
    (1..)
        .map(|n| {
            let suffix = if n == 1 { String::new() } else { format!(" ({n})") };
            dir.join(format!("{stem}.firmado{suffix}{ext}"))
        })
        .find(|p| !p.exists() && !sidecar_path(p).exists())
        .expect("rango infinito")
}

pub fn sidecar_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".");
    name.push(SIDECAR_EXTENSION);
    PathBuf::from(name)
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "documento".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    struct Fixture {
        dir: tempfile::TempDir,
        keys: KeyMaterial,
        vault: Vault,
        profile: Uuid,
    }

    impl Fixture {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let keys = KeyMaterial::ephemeral();
            let vault = Vault::open(dir.path().join("vault")).unwrap();
            let sig = RgbaImage::from_pixel(20, 8, image::Rgba([10, 20, 120, 255]));
            let mut png = Vec::new();
            sig.write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
            let profile = vault.add_profile(&keys.vault_key, "Firma", png).unwrap().id;
            Self { dir, keys, vault, profile }
        }

        fn service(&self) -> SigningService<'_> {
            SigningService { keys: &self.keys, vault: &self.vault }
        }

        fn path(&self, name: &str) -> PathBuf {
            self.dir.path().join(name)
        }
    }

    fn placement(page: u32) -> SignaturePlacement {
        SignaturePlacement { page, x: 0.6, y: 0.85, width: 0.3, height: 0.1, caption: None, caption_align: None }
    }

    #[test]
    fn pdf_sign_inspect_tamper_and_resign() {
        let fx = Fixture::new();
        let svc = fx.service();
        let input = fx.path("contrato.pdf");
        fs::write(&input, pdf_engine::tests::sample_pdf(2)).unwrap();
        let output = fx.path("contrato.firmado.pdf");

        let signed = svc.sign(&input, &output, fx.profile, &[placement(1)]).unwrap();
        assert!(lopdf::Document::load(&output).is_ok(), "el PDF firmado debe seguir siendo válido");

        let status = svc.inspect(&output).unwrap().status;
        let SignatureStatus::Valid { owned: true, metadata: Some(meta), .. } = status else {
            panic!("se esperaba firma válida y propia: {status:?}");
        };
        assert_eq!(meta.document_id, signed.document_id);
        assert_eq!(meta.placements, vec![placement(1)]);

        assert!(svc.sign(&output, &fx.path("otra.pdf"), fx.profile, &[placement(0)]).is_err());

        let mut bytes = fs::read(&output).unwrap();
        let idx = bytes.windows(4).position(|w| w == b"/Kid").unwrap();
        bytes[idx + 1] = b'X';
        let tampered = fx.path("alterado.pdf");
        fs::write(&tampered, &bytes).unwrap();
        assert!(matches!(svc.inspect(&tampered).unwrap().status, SignatureStatus::Tampered { .. }));

        let moved = fx.path("movido.pdf");
        svc.resign(&output, &moved, fx.profile, &[placement(0)]).unwrap();
        let doc = lopdf::Document::load(&moved).unwrap();
        let pages = doc.get_pages();
        let p1 = String::from_utf8_lossy(&doc.get_page_content(pages[&1])).into_owned();
        let p2 = String::from_utf8_lossy(&doc.get_page_content(pages[&2])).into_owned();
        assert!(p1.contains("FirmarSig") && !p2.contains("FirmarSig"), "la firma anterior no debe quedar");
        let SignatureStatus::Valid { metadata: Some(meta), .. } = svc.inspect(&moved).unwrap().status else {
            panic!("re-firma inválida");
        };
        assert_eq!(meta.document_id, signed.document_id);
    }

    #[test]
    fn pdf_incremental_update_is_reported() {
        let fx = Fixture::new();
        let svc = fx.service();
        let input = fx.path("a.pdf");
        fs::write(&input, pdf_engine::tests::sample_pdf(1)).unwrap();
        let output = fx.path("b.pdf");
        svc.sign(&input, &output, fx.profile, &[placement(0)]).unwrap();

        let mut bytes = fs::read(&output).unwrap();
        bytes.extend_from_slice(b"1 0 obj\n<<>>\nendobj\nxref\n...\nstartxref\n999\n%%EOF\n");
        fs::write(&output, bytes).unwrap();
        assert!(matches!(
            svc.inspect(&output).unwrap().status,
            SignatureStatus::ModifiedAfterSigning { owned: true, .. }
        ));
    }

    #[test]
    fn signer_name_and_trusted_contacts() {
        let fx = Fixture::new();
        fx.vault.set_my_name(&fx.keys.vault_key, Some("Juan Pérez")).unwrap();
        let input = fx.path("a.pdf");
        fs::write(&input, pdf_engine::tests::sample_pdf(1)).unwrap();
        let output = fx.path("b.pdf");
        fx.service().sign(&input, &output, fx.profile, &[placement(0)]).unwrap();

        // Otra persona, sin la tarjeta de Juan: ve el nombre declarado, no verificado.
        let dir = tempfile::tempdir().unwrap();
        let ana = KeyMaterial::ephemeral();
        let ana_vault = Vault::open(dir.path()).unwrap();
        let ana_svc = SigningService { keys: &ana, vault: &ana_vault };
        let SignatureStatus::Valid { signer_name, contact_name, owned: false, .. } = ana_svc.inspect(&output).unwrap().status else {
            panic!("se esperaba firma válida ajena");
        };
        assert_eq!(signer_name.as_deref(), Some("Juan Pérez"));
        assert!(contact_name.is_none());

        // Con la tarjeta importada, el nombre pasa a ser el del contacto.
        let card = crate::storage::IdentityCard::issue(&fx.keys.identity, "Juan Pérez").unwrap();
        ana_vault.add_contact(&ana.vault_key, &card, &ana.identity).unwrap();
        let SignatureStatus::Valid { contact_name, .. } = ana_svc.inspect(&output).unwrap().status else { panic!() };
        assert_eq!(contact_name.as_deref(), Some("Juan Pérez"));
    }

    #[test]
    fn foreign_identity_sees_valid_but_cannot_decrypt() {
        let fx = Fixture::new();
        let input = fx.path("a.pdf");
        fs::write(&input, pdf_engine::tests::sample_pdf(1)).unwrap();
        let output = fx.path("b.pdf");
        fx.service().sign(&input, &output, fx.profile, &[placement(0)]).unwrap();

        let stranger = KeyMaterial::ephemeral();
        let svc = SigningService { keys: &stranger, vault: &fx.vault };
        let status = svc.inspect(&output).unwrap().status;
        assert!(matches!(status, SignatureStatus::Valid { owned: false, metadata: None, .. }));
        assert!(matches!(svc.original(&output), Err(FirmarError::NotOwner)));
    }

    #[test]
    fn batch_signs_every_file_and_reports_failures() {
        let fx = Fixture::new();
        let svc = fx.service();
        let a = fx.path("a.pdf");
        let b = fx.path("b.pdf");
        let broken = fx.path("roto.pdf");
        fs::write(&a, pdf_engine::tests::sample_pdf(3)).unwrap();
        fs::write(&b, pdf_engine::tests::sample_pdf(1)).unwrap();
        fs::write(&broken, b"%PDF-1.7 no es un pdf").unwrap();
        let template = PlacementTemplate {
            pages: PageTarget::All,
            x: 0.6,
            y: 0.8,
            width: 0.25,
            caption: Some("Juan".into()),
            caption_align: Some(crate::domain::CaptionAlign::Left),
        };

        let mut ticks = Vec::new();
        let results = svc.sign_batch(&[a, b, broken], fx.profile, &template, |i, _| ticks.push(i)).unwrap();
        assert_eq!(ticks, vec![0, 1, 2, 3]);
        assert!(results[0].output_path.is_some() && results[1].output_path.is_some());
        assert!(results[2].error.is_some());

        let SignatureStatus::Valid { metadata: Some(meta), .. } = svc.inspect(&fx.path("a.firmado.pdf")).unwrap().status else {
            panic!("lote mal firmado");
        };
        assert_eq!(meta.placements.len(), 3, "una firma por página");
        assert!(meta.placements.iter().all(|p| p.caption.as_deref() == Some("Juan")));
        assert!(meta.placements.iter().all(|p| p.caption_align == Some(crate::domain::CaptionAlign::Left)));
        assert_eq!(fx.vault.list_history(&fx.keys.vault_key).unwrap().len(), 2);
    }

    #[test]
    fn default_output_never_overwrites() {
        let fx = Fixture::new();
        let input = fx.path("acta.pdf");
        assert_eq!(default_output_path(&input), fx.path("acta.firmado.pdf"));
        fs::write(fx.path("acta.firmado.pdf"), b"x").unwrap();
        assert_eq!(default_output_path(&input), fx.path("acta.firmado (2).pdf"));
    }

    #[test]
    fn binary_uses_sidecar() {
        let fx = Fixture::new();
        let svc = fx.service();
        let input = fx.path("informe.docx");
        fs::write(&input, b"PK\x03\x04 contenido zip").unwrap();
        let output = fx.path("informe.firmado.docx");

        let signed = svc.sign(&input, &output, fx.profile, &[]).unwrap();
        assert!(signed.sidecar_path.is_some());
        assert!(matches!(svc.inspect(&output).unwrap().status, SignatureStatus::Valid { owned: true, .. }));

        fs::write(&output, b"PK\x03\x04 otro contenido").unwrap();
        assert!(matches!(svc.inspect(&output).unwrap().status, SignatureStatus::Tampered { .. }));
    }

    #[test]
    fn docx_sign_inspect_and_resign() {
        let fx = Fixture::new();
        let svc = fx.service();
        let input = fx.path("contrato.docx");
        fs::write(&input, docx_engine::tests::sample_docx("<w:p><w:r><w:t>Hola</w:t></w:r></w:p><w:sectPr/>")).unwrap();
        let output = fx.path("contrato.firmado.docx");

        let signed = svc.sign(&input, &output, fx.profile, &[placement(docx_engine::LAST_PAGE)]).unwrap();
        assert!(signed.sidecar_path.is_none(), "la firma va dentro del .docx");
        assert!(!services_sidecar_exists(&output));
        assert!(matches!(svc.inspect(&output).unwrap().status, SignatureStatus::Valid { owned: true, .. }));
        assert_eq!(svc.docx_layout(&output).unwrap().pages.len(), 2);

        // Un editor que reescribe el ZIP cambia los bytes firmados.
        let mut bytes = fs::read(&output).unwrap();
        let idx = bytes.windows(4).position(|w| w == b"PK\x01\x02").unwrap();
        bytes[idx + 12] ^= 0xFF;
        let tampered = fx.path("alterado.docx");
        fs::write(&tampered, &bytes).unwrap();
        assert!(matches!(svc.inspect(&tampered).unwrap().status, SignatureStatus::Tampered { .. }));

        let moved = fx.path("movido.docx");
        svc.resign(&output, &moved, fx.profile, &[placement(docx_engine::FIRST_PAGE)]).unwrap();
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(fs::read(&moved).unwrap())).unwrap();
        let mut xml = String::new();
        std::io::Read::read_to_string(&mut zip.by_name("word/document.xml").unwrap(), &mut xml).unwrap();
        assert_eq!(xml.matches("<wp:anchor").count(), 1, "la firma anterior no se acumula");
    }

    fn services_sidecar_exists(path: &Path) -> bool {
        sidecar_path(path).exists()
    }

    #[test]
    fn jpeg_sign_roundtrip() {
        let fx = Fixture::new();
        let svc = fx.service();
        let base = image::RgbImage::from_pixel(64, 48, image::Rgb([240, 240, 240]));
        let mut jpg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new(&mut jpg).encode_image(&base).unwrap();
        let input = fx.path("foto.jpg");
        fs::write(&input, jpg).unwrap();
        let output = fx.path("foto.firmada.jpg");

        svc.sign(&input, &output, fx.profile, &[placement(0)]).unwrap();
        assert!(image::open(&output).is_ok());
        assert!(matches!(svc.inspect(&output).unwrap().status, SignatureStatus::Valid { owned: true, .. }));
    }
}
