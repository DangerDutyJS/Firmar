//! Estampado aplanado de firmas en PDF con `lopdf`.
//!
//! La firma se dibuja como un XObject de imagen dentro del flujo de contenido de
//! la página (no como anotación), así que forma parte del documento firmado.

use std::borrow::Cow;
use std::io::Write;

use flate2::Compression;
use flate2::write::ZlibEncoder;
use image::RgbaImage;
use lopdf::{Dictionary, Document, Object, ObjectId, Stream};

use crate::domain::SignaturePlacement;
use crate::image_engine::SignatureArt;
use crate::error::{FirmarError, FirmarResult};

const XOBJECT_PREFIX: &str = "FirmarSig";


pub fn stamp(pdf: &[u8], art: &SignatureArt, placements: &[SignaturePlacement]) -> FirmarResult<Vec<u8>> {
    let mut doc = load(pdf)?;
    let pages = doc.get_pages();

    let page_ids = placements
        .iter()
        .map(|p| {
            pages.get(&(p.page + 1)).copied().ok_or_else(|| {
                FirmarError::InvalidInput(format!(
                    "la página {} no existe (el documento tiene {})",
                    p.page + 1,
                    pages.len()
                ))
            })
        })
        .collect::<FirmarResult<Vec<_>>>()?;

    // La firma sin texto es la misma imagen en todas las páginas: un solo XObject.
    let mut shared_image: Option<ObjectId> = None;
    let mut extra_bytes = 0;

    for (placement, page_id) in placements.iter().zip(page_ids) {
        let geometry = PageGeometry::read(&doc, page_id)?;
        let (page_w, page_h) = geometry.display_size();
        let aspect = (placement.height * page_h) / (placement.width * page_w);
        let image_id = match art.render(placement.caption.as_deref(), aspect)? {
            Cow::Borrowed(img) => match shared_image {
                Some(id) => id,
                None => {
                    extra_bytes += img.len();
                    *shared_image.insert(add_signature_image(&mut doc, img)?)
                }
            },
            Cow::Owned(img) => {
                extra_bytes += img.len();
                add_signature_image(&mut doc, &img)?
            }
        };
        let name = attach_xobject(&mut doc, page_id, image_id)?;
        let [a, b, c, d, e, f] = geometry.image_matrix(placement);
        let ops = format!("q\n{a:.4} {b:.4} {c:.4} {d:.4} {e:.4} {f:.4} cm\n/{name} Do\nQ\n");
        wrap_page_contents(&mut doc, page_id, ops.into_bytes())?;
    }

    mark_catalog(&mut doc)?;

    let mut out = Vec::with_capacity(pdf.len() + extra_bytes);
    doc.save_to(&mut out)?;
    Ok(out)
}

/// Tamaño de cada página tal como se ve (puntos, con la rotación aplicada).
pub fn page_sizes(pdf: &[u8]) -> FirmarResult<Vec<(f64, f64)>> {
    let doc = load(pdf)?;
    doc.get_pages()
        .values()
        .map(|&id| Ok(PageGeometry::read(&doc, id)?.display_size()))
        .collect()
}

fn load(pdf: &[u8]) -> FirmarResult<Document> {
    let doc = Document::load_mem(pdf)?;
    if doc.is_encrypted() {
        return Err(FirmarError::UnsupportedFormat(
            "el PDF está protegido con contraseña; quítala antes de firmarlo".into(),
        ));
    }
    Ok(doc)
}

/// Imagen RGB + máscara alfa (`/SMask`), ambas con FlateDecode.
fn add_signature_image(doc: &mut Document, img: &RgbaImage) -> FirmarResult<ObjectId> {
    let (w, h) = img.dimensions();
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    let mut alpha = Vec::with_capacity((w * h) as usize);
    for px in img.pixels() {
        rgb.extend_from_slice(&px.0[..3]);
        alpha.push(px.0[3]);
    }

    let image_dict = |color_space: &str| {
        let mut d = Dictionary::new();
        d.set("Type", Object::Name(b"XObject".to_vec()));
        d.set("Subtype", Object::Name(b"Image".to_vec()));
        d.set("Width", i64::from(w));
        d.set("Height", i64::from(h));
        d.set("ColorSpace", Object::Name(color_space.as_bytes().to_vec()));
        d.set("BitsPerComponent", 8);
        d.set("Filter", Object::Name(b"FlateDecode".to_vec()));
        d
    };

    let smask_id = doc.add_object(Stream::new(image_dict("DeviceGray"), deflate(&alpha)?).with_compression(false));
    let mut dict = image_dict("DeviceRGB");
    dict.set("SMask", Object::Reference(smask_id));
    Ok(doc.add_object(Stream::new(dict, deflate(&rgb)?).with_compression(false)))
}

fn deflate(data: &[u8]) -> FirmarResult<Vec<u8>> {
    let mut enc = ZlibEncoder::new(Vec::new(), Compression::best());
    enc.write_all(data)?;
    Ok(enc.finish()?)
}

/// Busca una clave heredable (`MediaBox`, `Resources`, `Rotate`...) subiendo por `/Parent`.
fn inherited(doc: &Document, page_id: ObjectId, key: &[u8]) -> FirmarResult<Option<Object>> {
    let mut current = Some(page_id);
    let mut guard = 0;
    while let Some(id) = current {
        guard += 1;
        if guard > 64 {
            return Err(FirmarError::Pdf("árbol de páginas cíclico".into()));
        }
        let dict = doc.get_dictionary(id)?;
        if let Ok(value) = dict.get(key) {
            return Ok(Some(doc.dereference(value)?.1.clone()));
        }
        current = dict.get(b"Parent").and_then(Object::as_reference).ok();
    }
    Ok(None)
}

fn resolved_dict(doc: &Document, obj: &Object) -> FirmarResult<Dictionary> {
    Ok(doc.dereference(obj)?.1.as_dict()?.clone())
}

/// Registra la imagen en los recursos propios de la página. Copiamos el
/// diccionario (heredado o compartido) en vez de mutarlo para no afectar a
/// otras páginas.
fn attach_xobject(doc: &mut Document, page_id: ObjectId, image_id: ObjectId) -> FirmarResult<String> {
    let mut resources = match inherited(doc, page_id, b"Resources")? {
        Some(obj) => resolved_dict(doc, &obj)?,
        None => Dictionary::new(),
    };
    let mut xobjects = match resources.get(b"XObject") {
        Ok(obj) => resolved_dict(doc, obj)?,
        Err(_) => Dictionary::new(),
    };
    let name = (1..)
        .map(|i| format!("{XOBJECT_PREFIX}{i}"))
        .find(|n| !xobjects.has(n.as_bytes()))
        .expect("rango infinito");
    xobjects.set(name.as_bytes(), Object::Reference(image_id));
    resources.set("XObject", Object::Dictionary(xobjects));

    doc.get_object_mut(page_id)?
        .as_dict_mut()?
        .set("Resources", Object::Dictionary(resources));
    Ok(name)
}

/// Encierra el contenido existente en `q ... Q` para aislar su matriz de
/// transformación y añade el estampado al final.
fn wrap_page_contents(doc: &mut Document, page_id: ObjectId, stamp_ops: Vec<u8>) -> FirmarResult<()> {
    let existing: Vec<Object> = match doc.get_dictionary(page_id)?.get(b"Contents") {
        Ok(Object::Reference(id)) => match doc.get_object(*id)? {
            Object::Array(arr) => arr.clone(),
            _ => vec![Object::Reference(*id)],
        },
        Ok(Object::Array(arr)) => arr.clone(),
        Ok(_) => return Err(FirmarError::Pdf("/Contents con formato inesperado".into())),
        Err(_) => Vec::new(),
    };

    let open_id = doc.add_object(Stream::new(Dictionary::new(), b"q\n".to_vec()));
    let mut closing = b"\nQ\n".to_vec();
    closing.extend_from_slice(&stamp_ops);
    let close_id = doc.add_object(Stream::new(Dictionary::new(), closing));

    let mut contents = Vec::with_capacity(existing.len() + 2);
    contents.push(Object::Reference(open_id));
    contents.extend(existing);
    contents.push(Object::Reference(close_id));

    doc.get_object_mut(page_id)?
        .as_dict_mut()?
        .set("Contents", Object::Array(contents));
    Ok(())
}

/// Marca visible en el catálogo; el sobre criptográfico real va al final del archivo.
fn mark_catalog(doc: &mut Document) -> FirmarResult<()> {
    let mut marker = Dictionary::new();
    marker.set("Version", 1);
    marker.set("Container", Object::string_literal("X-Firmar-VaultPayload"));
    doc.catalog_mut()?.set("FirmarPayload", Object::Dictionary(marker));
    Ok(())
}

/// Caja visible de la página y su rotación, tal como las usa pdf.js para
/// pintar el visor (CropBox si existe, si no MediaBox).
struct PageGeometry {
    llx: f64,
    lly: f64,
    urx: f64,
    ury: f64,
    rotate: i64,
}

impl PageGeometry {
    fn read(doc: &Document, page_id: ObjectId) -> FirmarResult<Self> {
        let rect = match inherited(doc, page_id, b"CropBox")? {
            Some(obj) => obj,
            None => inherited(doc, page_id, b"MediaBox")?
                .ok_or_else(|| FirmarError::Pdf("la página no tiene MediaBox".into()))?,
        };
        let nums = rect
            .as_array()?
            .iter()
            .map(|o| doc.dereference(o).and_then(|(_, o)| o.as_float()).map(f64::from))
            .collect::<Result<Vec<_>, _>>()?;
        let [x0, y0, x1, y1] = nums[..] else {
            return Err(FirmarError::Pdf("rectángulo de página inválido".into()));
        };
        let rotate = match inherited(doc, page_id, b"Rotate")? {
            Some(obj) => obj.as_i64().unwrap_or(0),
            None => 0,
        };
        Ok(Self {
            llx: x0.min(x1),
            lly: y0.min(y1),
            urx: x0.max(x1),
            ury: y0.max(y1),
            rotate: rotate.rem_euclid(360),
        })
    }

    /// Tamaño de la página tal como se ve (con la rotación aplicada).
    fn display_size(&self) -> (f64, f64) {
        let (w, h) = (self.urx - self.llx, self.ury - self.lly);
        if self.rotate == 90 || self.rotate == 270 { (h, w) } else { (w, h) }
    }

    /// Convierte coordenadas normalizadas del visor (origen arriba-izquierda,
    /// página ya rotada) al espacio de usuario del PDF.
    fn to_user_space(&self, u: f64, v: f64) -> (f64, f64) {
        let (w, h) = (self.urx - self.llx, self.ury - self.lly);
        match self.rotate {
            90 => (self.llx + v * w, self.lly + u * h),
            180 => (self.urx - u * w, self.lly + v * h),
            270 => (self.urx - v * w, self.ury - u * h),
            _ => (self.llx + u * w, self.ury - v * h),
        }
    }

    /// Matriz `cm` que lleva el cuadrado unidad de la imagen al rectángulo
    /// elegido, con la firma derecha aunque la página esté rotada.
    fn image_matrix(&self, p: &SignaturePlacement) -> [f64; 6] {
        let bottom_left = self.to_user_space(p.x, p.y + p.height);
        let bottom_right = self.to_user_space(p.x + p.width, p.y + p.height);
        let top_left = self.to_user_space(p.x, p.y);
        [
            bottom_right.0 - bottom_left.0,
            bottom_right.1 - bottom_left.1,
            top_left.0 - bottom_left.0,
            top_left.1 - bottom_left.1,
            bottom_left.0,
            bottom_left.1,
        ]
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use lopdf::content::{Content, Operation};
    use lopdf::dictionary;

    pub fn sample_pdf(pages: usize) -> Vec<u8> {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();
        let kids = (0..pages)
            .map(|_| {
                let content = Content {
                    operations: vec![Operation::new("re", vec![10.into(), 10.into(), 50.into(), 50.into()])],
                };
                let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
                Object::Reference(doc.add_object(dictionary! {
                    "Type" => "Page",
                    "Parent" => pages_id,
                    "Contents" => content_id,
                }))
            })
            .collect::<Vec<_>>();
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => kids,
                "Count" => pages as i64,
                "MediaBox" => vec![0.into(), 0.into(), 600.into(), 800.into()],
                "Resources" => dictionary! {},
            }),
        );
        let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog_id);
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        out
    }

    fn geometry(rotate: i64) -> PageGeometry {
        PageGeometry { llx: 0.0, lly: 0.0, urx: 600.0, ury: 800.0, rotate }
    }

    #[test]
    fn matrix_without_rotation() {
        let p = SignaturePlacement { page: 0, x: 0.5, y: 0.0, width: 0.25, height: 0.1, caption: None };
        let m = geometry(0).image_matrix(&p);
        assert_eq!(m, [150.0, 0.0, 0.0, 80.0, 300.0, 720.0]);
    }

    #[test]
    fn matrix_with_rotation_90_keeps_size() {
        // Vista rotada: 800 de ancho x 600 de alto.
        let p = SignaturePlacement { page: 0, x: 0.0, y: 0.0, width: 0.5, height: 0.5, caption: None };
        let [a, b, c, d, ..] = geometry(90).image_matrix(&p);
        let width = (a * a + b * b).sqrt();
        let height = (c * c + d * d).sqrt();
        assert!((width - 400.0).abs() < 1e-9);
        assert!((height - 300.0).abs() < 1e-9);
    }

    #[test]
    fn stamps_requested_page_and_keeps_others() {
        let pdf = sample_pdf(2);
        let sig = SignatureArt::new(RgbaImage::from_pixel(8, 4, image::Rgba([0, 0, 255, 200])));
        let p = SignaturePlacement { page: 1, x: 0.1, y: 0.8, width: 0.3, height: 0.1, caption: None };
        let out = stamp(&pdf, &sig, &[p]).unwrap();

        let doc = Document::load_mem(&out).unwrap();
        let pages = doc.get_pages();
        let content = String::from_utf8_lossy(&doc.get_page_content(pages[&2])).into_owned();
        assert!(content.contains("/FirmarSig1 Do"));
        assert!(content.trim_start().starts_with('q'));
        let first = String::from_utf8_lossy(&doc.get_page_content(pages[&1])).into_owned();
        assert!(!first.contains("FirmarSig"));
        assert!(doc.catalog().unwrap().has(b"FirmarPayload"));
    }

    #[test]
    fn captions_get_their_own_image_and_plain_signatures_share_one() {
        let pdf = sample_pdf(3);
        let sig = SignatureArt::new(RgbaImage::from_pixel(40, 16, image::Rgba([0, 0, 0, 255])));
        let plain = |page| SignaturePlacement { page, x: 0.1, y: 0.1, width: 0.3, height: 0.1, caption: None };
        let with_text = SignaturePlacement { caption: Some("Juan Pérez\n28/10/2026".into()), height: 0.15, ..plain(2) };
        let out = stamp(&pdf, &sig, &[plain(0), plain(1), with_text]).unwrap();

        let doc = Document::load_mem(&out).unwrap();
        let images = doc
            .objects
            .values()
            .filter(|o| matches!(o, Object::Stream(s) if s.dict.get(b"SMask").is_ok()))
            .count();
        assert_eq!(images, 2, "una imagen compartida + una con texto");
    }

    #[test]
    fn rejects_missing_page() {
        let sig = SignatureArt::new(RgbaImage::from_pixel(2, 2, image::Rgba([0, 0, 0, 255])));
        let p = SignaturePlacement { page: 5, x: 0.1, y: 0.1, width: 0.1, height: 0.1, caption: None };
        assert!(matches!(stamp(&sample_pdf(1), &sig, &[p]), Err(FirmarError::InvalidInput(_))));
    }
}
