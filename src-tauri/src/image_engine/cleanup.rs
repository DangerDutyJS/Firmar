//! Limpieza de la imagen de una firma antes de guardarla o estamparla, para
//! que en el documento se vea solo la tinta, como una firma hecha a mano:
//!
//! 1. **Velo de papel**: los píxeles claros (papel, fondos casi blancos con
//!    algo de opacidad) pasan a ser transparentes. La tinta siempre es oscura.
//! 2. **Marcos**: se eliminan los trazos que viven solo en la franja exterior
//!    de la imagen (recuadros sólidos o punteados de capturas de formularios).
//!    Un trazo de la firma que toca el borde también se adentra en la imagen,
//!    así que se conserva.
//! 3. **Recorte** al área con tinta, con un margen mínimo.

use image::{RgbaImage, imageops};

use crate::error::{FirmarError, FirmarResult};

/// Versión del algoritmo; los perfiles guardados con una anterior se re-limpian.
pub const CLEANUP_VERSION: u32 = 1;

const INK_ALPHA: u8 = 12;
/// Por encima de esta luminosidad el píxel es papel (transparente).
const PAPER_LUMINANCE: f32 = 235.0;
/// Entre esta luminosidad y `PAPER_LUMINANCE` la opacidad baja gradualmente.
const FADE_LUMINANCE: f32 = 200.0;
const FRAME_BAND_RATIO: f32 = 0.06;

pub fn clean_signature(img: &RgbaImage) -> FirmarResult<RgbaImage> {
    let mut img = img.clone();
    remove_paper_veil(&mut img);
    remove_edge_frames(&mut img);
    trim_to_ink(&img).ok_or_else(|| FirmarError::InvalidInput("no se detectó tinta en la firma".into()))
}

fn luminance(p: &image::Rgba<u8>) -> f32 {
    0.2126 * f32::from(p[0]) + 0.7152 * f32::from(p[1]) + 0.0722 * f32::from(p[2])
}

fn remove_paper_veil(img: &mut RgbaImage) {
    for p in img.pixels_mut() {
        let l = luminance(p);
        if l >= PAPER_LUMINANCE {
            p[3] = 0;
        } else if l > FADE_LUMINANCE {
            let keep = (PAPER_LUMINANCE - l) / (PAPER_LUMINANCE - FADE_LUMINANCE);
            p[3] = (f32::from(p[3]) * keep).round() as u8;
        }
        if p[3] == 0 {
            // Color neutro en píxeles invisibles: evita halos al redimensionar.
            *p = image::Rgba([0, 0, 0, 0]);
        }
    }
}

fn remove_edge_frames(img: &mut RgbaImage) {
    let (w, h) = (img.width() as usize, img.height() as usize);
    if w == 0 || h == 0 {
        return;
    }
    let band = ((w.min(h) as f32 * FRAME_BAND_RATIO).round() as usize).max(4);
    if band * 4 >= w.min(h) {
        // Imagen tan pequeña que la franja lo cubriría casi todo.
        return;
    }
    let in_band = |x: usize, y: usize| x < band || y < band || x + band >= w || y + band >= h;
    let ink = |img: &RgbaImage, i: usize| img.as_raw()[i * 4 + 3] > INK_ALPHA;

    let mut seen = vec![false; w * h];
    let mut stack = Vec::new();
    let mut component = Vec::new();

    for start in 0..w * h {
        if seen[start] || !ink(img, start) {
            continue;
        }
        seen[start] = true;
        stack.push(start);
        component.clear();
        let mut only_band = true;

        while let Some(i) = stack.pop() {
            component.push(i);
            let (x, y) = (i % w, i / w);
            only_band &= in_band(x, y);
            for ny in y.saturating_sub(1)..=(y + 1).min(h - 1) {
                for nx in x.saturating_sub(1)..=(x + 1).min(w - 1) {
                    let j = ny * w + nx;
                    if !seen[j] && ink(img, j) {
                        seen[j] = true;
                        stack.push(j);
                    }
                }
            }
        }

        if only_band {
            let raw: &mut [u8] = img;
            for &i in &component {
                raw[i * 4..i * 4 + 4].copy_from_slice(&[0, 0, 0, 0]);
            }
        }
    }
}

fn trim_to_ink(img: &RgbaImage) -> Option<RgbaImage> {
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for (x, y, p) in img.enumerate_pixels() {
        if p[3] > INK_ALPHA {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
    }
    if x0 == u32::MAX {
        return None;
    }
    let pad = ((x1 - x0).max(y1 - y0) / 50).max(2);
    let x0 = x0.saturating_sub(pad);
    let y0 = y0.saturating_sub(pad);
    let x1 = (x1 + pad).min(img.width() - 1);
    let y1 = (y1 + pad).min(img.height() - 1);
    Some(imageops::crop_imm(img, x0, y0, x1 - x0 + 1, y1 - y0 + 1).to_image())
}

/// `true` si la imagen deja ver algo que no es tinta (útil en pruebas).
#[cfg(test)]
fn has_light_visible_pixels(img: &RgbaImage) -> bool {
    img.pixels().any(|p| p[3] > 0 && luminance(p) >= PAPER_LUMINANCE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    /// Firma diagonal dentro de un recuadro punteado, sobre un velo blanco al 19 %.
    fn framed_signature() -> RgbaImage {
        let (w, h) = (300u32, 200u32);
        let mut img = RgbaImage::from_pixel(w, h, Rgba([255, 255, 255, 49]));
        for x in 20..280 {
            let y = 160 - (x - 20) * 120 / 260;
            for t in 0..4 {
                img.put_pixel(x, y + t, Rgba([60, 60, 60, 255]));
            }
        }
        for i in (0..w).step_by(12) {
            for d in 0..6.min(w - i) {
                for t in 1..3 {
                    img.put_pixel(i + d, t, Rgba([90, 170, 170, 255]));
                    img.put_pixel(i + d, h - 1 - t, Rgba([90, 170, 170, 255]));
                }
            }
        }
        for j in (0..h).step_by(12) {
            for d in 0..6.min(h - j) {
                for t in 1..3 {
                    img.put_pixel(t, j + d, Rgba([90, 170, 170, 255]));
                    img.put_pixel(w - 1 - t, j + d, Rgba([90, 170, 170, 255]));
                }
            }
        }
        img
    }

    #[test]
    fn removes_frame_and_veil_and_trims() {
        let clean = clean_signature(&framed_signature()).unwrap();
        assert!(!has_light_visible_pixels(&clean), "sin velo blanco");
        assert!(
            clean.pixels().all(|p| p[3] == 0 || p[1] < 120),
            "no quedan píxeles del marco verde azulado"
        );
        // Recortada al trazo (260×124) más un margen pequeño.
        assert!(clean.width() < 275 && clean.height() < 140, "{:?}", clean.dimensions());
        let ink = clean.pixels().filter(|p| p[3] > 200).count();
        assert!(ink > 900, "el trazo se conserva ({ink} píxeles)");
    }

    #[test]
    fn keeps_strokes_that_touch_the_edge() {
        let mut img = RgbaImage::from_pixel(100, 60, Rgba([0, 0, 0, 0]));
        for x in 0..100 {
            for t in 28..32 {
                img.put_pixel(x, t, Rgba([20, 20, 20, 255]));
            }
        }
        let clean = clean_signature(&img).unwrap();
        assert_eq!(clean.width(), 100, "una línea que cruza toda la imagen no es un marco");
    }

    #[test]
    fn empty_signature_is_rejected() {
        let img = RgbaImage::from_pixel(50, 50, Rgba([255, 255, 255, 255]));
        assert!(clean_signature(&img).is_err());
    }
}

