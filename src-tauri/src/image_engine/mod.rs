//! Estampado de firmas sobre imágenes PNG y JPEG, y limpieza de la imagen de
//! la firma.

mod art;
mod cleanup;

pub use art::{CAPTION_LINE_RATIO, CAPTION_MAX_RATIO, SignatureArt, caption_lines};
pub use cleanup::{CLEANUP_VERSION, clean_signature};

use std::io::Cursor;

use image::codecs::jpeg::JpegEncoder;
use image::imageops::{self, FilterType};
use image::{DynamicImage, ImageFormat, RgbaImage};

use crate::domain::{DocumentFormat, SignaturePlacement};
use crate::error::{FirmarError, FirmarResult};

const JPEG_QUALITY: u8 = 92;

pub fn encode_png(img: &RgbaImage) -> FirmarResult<Vec<u8>> {
    let mut out = Vec::new();
    img.write_to(&mut Cursor::new(&mut out), ImageFormat::Png)?;
    Ok(out)
}

pub fn stamp(
    bytes: &[u8],
    format: DocumentFormat,
    art: &SignatureArt,
    placements: &[SignaturePlacement],
) -> FirmarResult<Vec<u8>> {
    let image_format = match format {
        DocumentFormat::Png => ImageFormat::Png,
        DocumentFormat::Jpeg => ImageFormat::Jpeg,
        other => return Err(FirmarError::UnsupportedFormat(format!("{other:?} no es una imagen"))),
    };
    let mut base = image::load_from_memory_with_format(bytes, image_format)?.into_rgba8();
    let (w, h) = (f64::from(base.width()), f64::from(base.height()));

    for p in placements {
        if p.page != 0 {
            return Err(FirmarError::InvalidInput("una imagen solo tiene la página 1".into()));
        }
        let tw = (p.width * w).round().max(1.0) as u32;
        let th = (p.height * h).round().max(1.0) as u32;
        let signature = art.render(p.caption.as_deref(), p.caption_align.unwrap_or_default(), f64::from(th) / f64::from(tw))?;
        let resized = imageops::resize(signature.as_ref(), tw, th, FilterType::Lanczos3);
        imageops::overlay(&mut base, &resized, (p.x * w).round() as i64, (p.y * h).round() as i64);
    }

    let mut out = Vec::new();
    match format {
        DocumentFormat::Jpeg => {
            let rgb = DynamicImage::ImageRgba8(base).into_rgb8();
            JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY).encode_image(&rgb)?;
        }
        _ => base.write_to(&mut Cursor::new(&mut out), ImageFormat::Png)?,
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlays_signature_in_png() {
        let base = RgbaImage::from_pixel(100, 50, image::Rgba([255, 255, 255, 255]));
        let mut png = Vec::new();
        base.write_to(&mut Cursor::new(&mut png), ImageFormat::Png).unwrap();

        let sig = SignatureArt::new(RgbaImage::from_pixel(10, 10, image::Rgba([0, 0, 0, 255])));
        let p = SignaturePlacement { page: 0, x: 0.5, y: 0.5, width: 0.2, height: 0.2, caption: None, caption_align: None };
        let out = stamp(&png, DocumentFormat::Png, &sig, &[p]).unwrap();

        let img = image::load_from_memory(&out).unwrap().into_rgba8();
        assert_eq!(img.get_pixel(55, 27).0, [0, 0, 0, 255]);
        assert_eq!(img.get_pixel(10, 10).0, [255, 255, 255, 255]);
    }
}
