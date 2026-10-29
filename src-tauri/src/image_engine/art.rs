//! Imagen final que se estampa: la firma y, opcionalmente, un texto debajo
//! (nombre, fecha…) dentro del mismo rectángulo.
//!
//! La disposición es la misma que dibuja el visor (`src/features/signer/caption.ts`):
//! cada línea de texto ocupa `CAPTION_LINE_RATIO` del alto del rectángulo, el
//! tamaño de letra es `CAPTION_FONT_RATIO` de esa línea y la firma llena el
//! resto, centrada y sin deformarse. La tipografía es Liberation Sans, con las
//! mismas medidas que Arial, que es la que usa la vista previa.

use std::borrow::Cow;
use std::sync::LazyLock;

use ab_glyph::{Font, FontRef, PxScale, ScaleFont, point};
use image::imageops::{self, FilterType};
use image::{Rgba, RgbaImage};

use crate::domain::CaptionAlign;
use crate::error::{FirmarError, FirmarResult};

pub const CAPTION_LINE_RATIO: f64 = 0.17;
pub const CAPTION_MAX_RATIO: f64 = 0.5;
pub const CAPTION_FONT_RATIO: f32 = 0.78;
/// Margen horizontal para el texto (fracción del ancho).
pub const CAPTION_MAX_WIDTH: f32 = 0.96;
pub const MAX_CAPTION_LINES: usize = 5;
pub const MAX_CAPTION_LINE_CHARS: usize = 80;

const TEXT_COLOR: [u8; 3] = [31, 41, 55];
const MIN_RENDER_WIDTH: u32 = 480;
const MAX_RENDER_WIDTH: u32 = 1600;

static FONT: LazyLock<FontRef<'static>> = LazyLock::new(|| {
    FontRef::try_from_slice(include_bytes!("../../assets/fonts/LiberationSans-Regular.ttf"))
        .expect("tipografía embebida válida")
});

pub struct SignatureArt {
    signature: RgbaImage,
}

impl SignatureArt {
    pub fn new(signature: RgbaImage) -> Self {
        Self { signature }
    }

    /// Imagen para un rectángulo de proporción `aspect` (alto / ancho). Sin
    /// texto devuelve la firma tal cual, que se estira al rectángulo.
    pub fn render(&self, caption: Option<&str>, align: CaptionAlign, aspect: f64) -> FirmarResult<Cow<'_, RgbaImage>> {
        let lines = caption_lines(caption)?;
        if lines.is_empty() {
            return Ok(Cow::Borrowed(&self.signature));
        }
        if !(aspect.is_finite() && aspect > 0.0) {
            return Err(FirmarError::InvalidInput("rectángulo de firma inválido".into()));
        }

        let width = self.signature.width().clamp(MIN_RENDER_WIDTH, MAX_RENDER_WIDTH);
        let height = ((f64::from(width) * aspect).round() as u32).clamp(8, 4 * MAX_RENDER_WIDTH);
        let mut canvas = RgbaImage::new(width, height);

        let caption_ratio = (CAPTION_LINE_RATIO * lines.len() as f64).min(CAPTION_MAX_RATIO);
        let line_h = f64::from(height) * caption_ratio / lines.len() as f64;
        let sig_h = (f64::from(height) - line_h * lines.len() as f64).floor().max(1.0) as u32;

        // Firma: contenida en el área superior, centrada.
        let (sw, sh) = self.signature.dimensions();
        let scale = (f64::from(width) / f64::from(sw)).min(f64::from(sig_h) / f64::from(sh));
        let (tw, th) = ((f64::from(sw) * scale).round().max(1.0) as u32, (f64::from(sh) * scale).round().max(1.0) as u32);
        let resized = imageops::resize(&self.signature, tw, th, FilterType::Lanczos3);
        imageops::overlay(&mut canvas, &resized, i64::from((width - tw) / 2), i64::from((sig_h - th) / 2));

        for (i, line) in lines.iter().enumerate() {
            let top = f64::from(sig_h) + line_h * i as f64;
            draw_line(&mut canvas, line, align, top as f32, line_h as f32);
        }
        Ok(Cow::Owned(canvas))
    }
}

/// Normaliza y valida el texto: como mucho 5 líneas de 80 caracteres.
pub fn caption_lines(caption: Option<&str>) -> FirmarResult<Vec<String>> {
    let lines: Vec<String> = caption
        .unwrap_or("")
        .lines()
        .map(|l| l.trim().to_owned())
        .filter(|l| !l.is_empty())
        .collect();
    if lines.len() > MAX_CAPTION_LINES {
        return Err(FirmarError::InvalidInput(format!("el texto admite como mucho {MAX_CAPTION_LINES} líneas")));
    }
    if lines.iter().any(|l| l.chars().count() > MAX_CAPTION_LINE_CHARS) {
        return Err(FirmarError::InvalidInput(format!(
            "cada línea de texto admite como mucho {MAX_CAPTION_LINE_CHARS} caracteres"
        )));
    }
    Ok(lines)
}

fn draw_line(canvas: &mut RgbaImage, text: &str, align: CaptionAlign, top: f32, line_h: f32) {
    let font = &*FONT;
    let mut px = line_h * CAPTION_FONT_RATIO;
    let width = text_width(text, px);
    let max = canvas.width() as f32 * CAPTION_MAX_WIDTH;
    if width > max {
        px *= max / width;
    }
    let scaled = font.as_scaled(PxScale::from(px));
    // Igual que CSS: el área ascendente+descendente centrada en la línea.
    let content = scaled.ascent() - scaled.descent();
    let baseline = top + (line_h - content) / 2.0 + scaled.ascent();
    // Mismo margen lateral que el visor: (1 - CAPTION_MAX_WIDTH) / 2 a cada lado.
    let margin = canvas.width() as f32 * (1.0 - CAPTION_MAX_WIDTH) / 2.0;
    let free = canvas.width() as f32 - text_width(text, px);
    let mut x = match align {
        CaptionAlign::Left => margin,
        CaptionAlign::Center => free / 2.0,
        CaptionAlign::Right => free - margin,
    };

    let mut previous = None;
    for c in text.chars() {
        let id = font.glyph_id(c);
        if let Some(prev) = previous {
            x += scaled.kern(prev, id);
        }
        let glyph = id.with_scale_and_position(px, point(x, baseline));
        x += scaled.h_advance(id);
        previous = Some(id);
        let Some(outline) = font.outline_glyph(glyph) else { continue };
        let bounds = outline.px_bounds();
        outline.draw(|gx, gy, coverage| {
            let (cx, cy) = (bounds.min.x as i64 + i64::from(gx), bounds.min.y as i64 + i64::from(gy));
            if cx < 0 || cy < 0 || cx >= i64::from(canvas.width()) || cy >= i64::from(canvas.height()) {
                return;
            }
            let pixel = canvas.get_pixel_mut(cx as u32, cy as u32);
            let alpha = (coverage.clamp(0.0, 1.0) * 255.0).round() as u8;
            if alpha > pixel[3] {
                *pixel = Rgba([TEXT_COLOR[0], TEXT_COLOR[1], TEXT_COLOR[2], alpha]);
            }
        });
    }
}

fn text_width(text: &str, px: f32) -> f32 {
    let font = &*FONT;
    let scaled = font.as_scaled(PxScale::from(px));
    let mut width = 0.0;
    let mut previous = None;
    for c in text.chars() {
        let id = font.glyph_id(c);
        if let Some(prev) = previous {
            width += scaled.kern(prev, id);
        }
        width += scaled.h_advance(id);
        previous = Some(id);
    }
    width
}

#[cfg(test)]
mod tests {
    use super::*;

    fn art() -> SignatureArt {
        SignatureArt::new(RgbaImage::from_pixel(200, 80, Rgba([0, 0, 0, 255])))
    }

    #[test]
    fn without_caption_returns_signature_untouched() {
        let a = art();
        assert!(matches!(a.render(None, CaptionAlign::Center, 0.4).unwrap(), Cow::Borrowed(_)));
        assert!(matches!(a.render(Some("  \n "), CaptionAlign::Center, 0.4).unwrap(), Cow::Borrowed(_)));
    }

    #[test]
    fn caption_is_drawn_below_signature() {
        let img = art().render(Some("Juan Pérez\n28/10/2026"), CaptionAlign::Center, 0.6).unwrap().into_owned();
        let (w, h) = img.dimensions();
        assert_eq!(w, MIN_RENDER_WIDTH);
        assert_eq!(h, (f64::from(w) * 0.6).round() as u32);
        let caption_top = (f64::from(h) * (1.0 - 2.0 * CAPTION_LINE_RATIO)) as u32;
        let text = |y0: u32, y1: u32| {
            (y0..y1).flat_map(|y| (0..w).map(move |x| (x, y))).filter(|&(x, y)| img.get_pixel(x, y).0 == [31, 41, 55, 255]).count()
        };
        assert!(text(caption_top, h) > 100, "hay texto en la franja inferior");
        assert_eq!(text(0, caption_top), 0, "el texto no invade la firma");
    }

    #[test]
    fn alignment_moves_text() {
        let ink_x = |align| {
            let img = art().render(Some("Ana"), align, 0.6).unwrap().into_owned();
            let top = (f64::from(img.height()) * (1.0 - CAPTION_LINE_RATIO)) as u32;
            (0..img.width())
                .find(|&x| (top..img.height()).any(|y| img.get_pixel(x, y).0 == [31, 41, 55, 255]))
                .unwrap()
        };
        let (left, center, right) = (ink_x(CaptionAlign::Left), ink_x(CaptionAlign::Center), ink_x(CaptionAlign::Right));
        assert!(left < center && center < right, "{left} < {center} < {right}");
    }

    #[test]
    fn long_lines_shrink_to_fit_and_limits_apply() {
        let long = "x".repeat(80);
        let img = art().render(Some(&long), CaptionAlign::Center, 0.5).unwrap().into_owned();
        let (w, h) = img.dimensions();
        let caption_top = (f64::from(h) * (1.0 - CAPTION_LINE_RATIO)).ceil() as u32;
        let edge_ink = (caption_top..h).any(|y| img.get_pixel(0, y)[3] > 0 || img.get_pixel(w - 1, y)[3] > 0);
        assert!(!edge_ink, "el texto se reduce para no salirse");
        assert!(caption_lines(Some("a\nb\nc\nd\ne")).is_ok());
        assert!(caption_lines(Some("a\nb\nc\nd\ne\nf")).is_err());
        assert!(caption_lines(Some(&"x".repeat(81))).is_err());
    }
}

