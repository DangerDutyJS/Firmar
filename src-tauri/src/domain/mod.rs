//! Modelos de dominio puros: sin E/S, sin criptografía.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{FirmarError, FirmarResult};

/// Firma manuscrita guardada en la bóveda (PNG RGBA con fondo transparente).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureProfile {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub width: u32,
    pub height: u32,
    #[serde(with = "crate::domain::b64")]
    pub png: Vec<u8>,
    /// Versión de la limpieza aplicada (sin velo, sin marcos, recortada).
    #[serde(default)]
    pub cleanup_version: u32,
}

/// Ubicación de una firma sobre una página, en fracciones (0..1) del tamaño de
/// la página y con origen en la esquina superior izquierda. Así no depende del
/// zoom del visor ni de las unidades del formato.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignaturePlacement {
    pub page: u32,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// Texto bajo la firma (nombre, fecha…), hasta 5 líneas.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    /// Alineación del texto; por defecto, centrado.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption_align: Option<CaptionAlign>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CaptionAlign {
    Left,
    #[default]
    Center,
    Right,
}

impl SignaturePlacement {
    pub fn validate(&self) -> FirmarResult<()> {
        let in_unit = |v: f64| v.is_finite() && (0.0..=1.0).contains(&v);
        if !(in_unit(self.x) && in_unit(self.y) && in_unit(self.width) && in_unit(self.height)) {
            return Err(FirmarError::InvalidInput(
                "las coordenadas de la firma deben estar entre 0 y 1".into(),
            ));
        }
        if self.width <= 0.0 || self.height <= 0.0 {
            return Err(FirmarError::InvalidInput("la firma debe tener tamaño".into()));
        }
        if self.x + self.width > 1.0 + 1e-6 || self.y + self.height > 1.0 + 1e-6 {
            return Err(FirmarError::InvalidInput(
                "la firma se sale de los límites de la página".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DocumentFormat {
    Pdf,
    Png,
    Jpeg,
    /// Documento de Word (`.docx`).
    Docx,
    /// Cualquier otro archivo: se firma con un sidecar `.firmar` sin estampado visual.
    Binary,
}

impl DocumentFormat {
    pub fn detect(bytes: &[u8]) -> Self {
        if bytes.starts_with(b"%PDF-") {
            Self::Pdf
        } else if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
            Self::Png
        } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
            Self::Jpeg
        } else if crate::docx_engine::is_docx(bytes) {
            Self::Docx
        } else {
            Self::Binary
        }
    }

    pub fn supports_visual_stamp(self) -> bool {
        !matches!(self, Self::Binary)
    }
}

/// Metadatos privados del propietario. Viajan cifrados dentro del documento.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FirmarMetadataPayload {
    pub version: u32,
    pub document_id: Uuid,
    pub profile_id: Uuid,
    pub profile_name: String,
    pub source_file_name: String,
    pub format: DocumentFormat,
    pub placements: Vec<SignaturePlacement>,
    pub signed_at: DateTime<Utc>,
    /// SHA-256 del documento original antes de estampar la firma.
    pub original_sha256: String,
}

/// Resultado de una operación de firmado.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignedDocument {
    pub document_id: Uuid,
    pub output_path: String,
    /// Archivo `.firmar` adicional cuando el formato no admite contenedor embebido.
    pub sidecar_path: Option<String>,
    pub format: DocumentFormat,
    pub content_sha256: String,
    pub signed_at: DateTime<Utc>,
}

/// Estado de un documento abierto en Firmar.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase", tag = "state")]
pub enum SignatureStatus {
    /// El archivo no contiene un contenedor de Firmar.
    Unsigned,
    /// Firma íntegra. `metadata` solo está presente si la firmó esta máquina.
    Valid {
        owned: bool,
        signer_fingerprint: String,
        /// Nombre que el firmante declaró en el sello (no verificado).
        signer_name: Option<String>,
        /// Nombre del contacto de confianza con esa clave, si lo es.
        contact_name: Option<String>,
        metadata: Option<FirmarMetadataPayload>,
    },
    /// El contenido firmado sigue intacto pero alguien añadió datos después
    /// (p. ej. un guardado incremental de otro editor de PDF).
    ModifiedAfterSigning {
        owned: bool,
        signer_fingerprint: String,
        signer_name: Option<String>,
        contact_name: Option<String>,
        metadata: Option<FirmarMetadataPayload>,
    },
    /// El contenido o la firma criptográfica no coinciden.
    Tampered { reason: String },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentInspection {
    pub path: String,
    pub file_name: String,
    pub format: DocumentFormat,
    pub size: u64,
    pub status: SignatureStatus,
}

pub(crate) mod b64 {
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        STANDARD.decode(s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placement(x: f64, y: f64, w: f64, h: f64) -> SignaturePlacement {
        SignaturePlacement { page: 0, x, y, width: w, height: h, caption: None, caption_align: None }
    }

    #[test]
    fn placement_validation() {
        assert!(placement(0.1, 0.1, 0.3, 0.1).validate().is_ok());
        assert!(placement(0.8, 0.1, 0.3, 0.1).validate().is_err());
        assert!(placement(0.1, 0.1, 0.0, 0.1).validate().is_err());
        assert!(placement(f64::NAN, 0.1, 0.2, 0.1).validate().is_err());
    }

    #[test]
    fn format_detection() {
        assert_eq!(DocumentFormat::detect(b"%PDF-1.7\n"), DocumentFormat::Pdf);
        assert_eq!(DocumentFormat::detect(&[0xFF, 0xD8, 0xFF, 0xE0]), DocumentFormat::Jpeg);
        assert_eq!(DocumentFormat::detect(b"PK\x03\x04"), DocumentFormat::Binary);
    }
}
