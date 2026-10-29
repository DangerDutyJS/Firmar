use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

/// Error único de la aplicación. Se serializa como `{ kind, message }` para que
/// el frontend pueda reaccionar por tipo sin parsear texto.
#[derive(Debug, thiserror::Error)]
pub enum FirmarError {
    #[error("Error de E/S: {0}")]
    Io(#[from] std::io::Error),

    #[error("Error criptográfico: {0}")]
    Crypto(String),

    #[error("No se pudo acceder al llavero del sistema: {0}")]
    Keyring(String),

    #[error("Error procesando el PDF: {0}")]
    Pdf(String),

    #[error("Error procesando la imagen: {0}")]
    Image(String),

    #[error("Error de serialización: {0}")]
    Serialization(String),

    #[error("No encontrado: {0}")]
    NotFound(String),

    #[error("Entrada inválida: {0}")]
    InvalidInput(String),

    #[error("Formato no soportado: {0}")]
    UnsupportedFormat(String),

    #[error("Integridad comprometida: {0}")]
    Integrity(String),

    #[error("Este documento fue firmado por otra identidad")]
    NotOwner,
}

impl FirmarError {
    fn kind(&self) -> &'static str {
        match self {
            Self::Io(_) => "io",
            Self::Crypto(_) => "crypto",
            Self::Keyring(_) => "keyring",
            Self::Pdf(_) => "pdf",
            Self::Image(_) => "image",
            Self::Serialization(_) => "serialization",
            Self::NotFound(_) => "notFound",
            Self::InvalidInput(_) => "invalidInput",
            Self::UnsupportedFormat(_) => "unsupportedFormat",
            Self::Integrity(_) => "integrity",
            Self::NotOwner => "notOwner",
        }
    }
}

impl Serialize for FirmarError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut s = serializer.serialize_struct("FirmarError", 2)?;
        s.serialize_field("kind", self.kind())?;
        s.serialize_field("message", &self.to_string())?;
        s.end()
    }
}

impl From<lopdf::Error> for FirmarError {
    fn from(e: lopdf::Error) -> Self {
        Self::Pdf(e.to_string())
    }
}

impl From<image::ImageError> for FirmarError {
    fn from(e: image::ImageError) -> Self {
        Self::Image(e.to_string())
    }
}

impl From<serde_json::Error> for FirmarError {
    fn from(e: serde_json::Error) -> Self {
        Self::Serialization(e.to_string())
    }
}

impl From<base64::DecodeError> for FirmarError {
    fn from(e: base64::DecodeError) -> Self {
        Self::Serialization(format!("base64 inválido: {e}"))
    }
}

impl From<keyring::Error> for FirmarError {
    fn from(e: keyring::Error) -> Self {
        Self::Keyring(e.to_string())
    }
}

pub type FirmarResult<T> = Result<T, FirmarError>;
