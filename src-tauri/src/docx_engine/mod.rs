//! Estampado visible de firmas en documentos Word (`.docx`).
//!
//! Word no tiene páginas fijas: la paginación depende de las fuentes y de la
//! versión del programa. Por eso la firma se inserta como imagen flotante
//! (`wp:anchor`) posicionada respecto a la **página** con coordenadas absolutas,
//! y se ancla al primer o al último párrafo del cuerpo, que son las dos páginas
//! que se pueden identificar sin maquetar el documento:
//!
//! * `page = 0` → primera página
//! * `page = 1` → última página

use std::borrow::Cow;
use std::collections::HashMap;
use std::io::{Cursor, Read, Write};

use quick_xml::events::{BytesStart, Event};
use quick_xml::name::ResolveResult;
use quick_xml::XmlVersion;
use quick_xml::reader::NsReader;
use serde::Serialize;
use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::domain::SignaturePlacement;
use crate::image_engine::SignatureArt;
use crate::error::{FirmarError, FirmarResult};

pub const FIRST_PAGE: u32 = 0;
pub const LAST_PAGE: u32 = 1;

const NS_MAIN: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const NS_MAIN_STRICT: &str = "http://purl.oclc.org/ooxml/wordprocessingml/main";
const NS_WP: &str = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing";
const NS_A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const NS_PIC: &str = "http://schemas.openxmlformats.org/drawingml/2006/picture";
const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const REL_IMAGE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";
const REL_OFFICE_DOCUMENT: &str = "/officeDocument";

const EMU_PER_TWIP: f64 = 635.0;
const MAX_PART_BYTES: u64 = 64 * 1024 * 1024;

/// Detección barata: ZIP que contiene la parte principal de Word.
pub fn is_docx(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK\x03\x04")
        && bytes.windows(b"word/document.xml".len()).any(|w| w == b"word/document.xml")
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageSetup {
    /// Tamaño y márgenes en puntos (1/72 pulgada).
    pub width: f64,
    pub height: f64,
    pub margin_top: f64,
    pub margin_right: f64,
    pub margin_bottom: f64,
    pub margin_left: f64,
}

impl Default for PageSetup {
    /// Carta con márgenes de 1 pulgada, el valor por defecto de Word.
    fn default() -> Self {
        Self { width: 612.0, height: 792.0, margin_top: 72.0, margin_right: 72.0, margin_bottom: 72.0, margin_left: 72.0 }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocxLayout {
    /// `[primera, última]`, o solo `[primera]` si el documento tiene una página.
    pub pages: Vec<PageSetup>,
    /// Número de páginas que Word guardó en `docProps/app.xml`, si existe.
    pub pages_hint: Option<u32>,
}

pub fn layout(bytes: &[u8]) -> FirmarResult<DocxLayout> {
    let mut pkg = Package::open(bytes)?;
    let xml = pkg.read_string(&pkg.main_part.clone())?;
    let scan = scan_document(&xml)?;
    let pages_hint = pkg.read_string("docProps/app.xml").ok().and_then(|s| pages_from_app_xml(&s));

    let first = scan.first_section.unwrap_or_default();
    let last = scan.final_section.or(scan.first_section).unwrap_or_default();
    let pages = if pages_hint == Some(1) { vec![first] } else { vec![first, last] };
    Ok(DocxLayout { pages, pages_hint })
}

/// Devuelve un `.docx` nuevo con la firma insertada. El ZIP resultante no
/// tiene comentario: ese hueco lo usa el contenedor criptográfico.
pub fn stamp(bytes: &[u8], art: &SignatureArt, placements: &[SignaturePlacement]) -> FirmarResult<Vec<u8>> {
    if let Some(p) = placements.iter().find(|p| p.page > LAST_PAGE) {
        return Err(FirmarError::InvalidInput(format!(
            "en Word solo se puede firmar la primera o la última página (recibido {})",
            p.page
        )));
    }

    let mut pkg = Package::open(bytes)?;
    let main_part = pkg.main_part.clone();
    let rels_part = rels_path_for(&main_part);
    let document_xml = pkg.read_string(&main_part)?;
    let rels_xml = pkg
        .read_string(&rels_part)
        .unwrap_or_else(|_| r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"></Relationships>"#.to_string());
    let content_types = pkg.read_string("[Content_Types].xml")?;

    let scan = scan_document(&document_xml)?;
    let (Some(first_slot), Some(last_slot)) = (scan.first_paragraph.clone(), scan.last_paragraph.clone()) else {
        return Err(FirmarError::UnsupportedFormat("el documento de Word no tiene párrafos".into()));
    };

    let main_dir = main_part.rsplit_once('/').map(|(d, _)| d).unwrap_or("").to_owned();
    let mut media = Media { dir: main_dir, rels: rels_xml.clone(), files: Vec::new(), shared: None };

    // Agrupamos las firmas por párrafo de anclaje (pueden coincidir si el
    // documento tiene un solo párrafo).
    let mut runs: Vec<(ParagraphSlot, String)> = Vec::new();
    for (index, p) in (0u32..).zip(placements) {
        let (slot, page) = if p.page == FIRST_PAGE {
            (first_slot.clone(), scan.first_section.unwrap_or_default())
        } else {
            (last_slot.clone(), scan.final_section.or(scan.first_section).unwrap_or_default())
        };
        let aspect = (p.height * page.height) / (p.width * page.width);
        let rel_id = match art.render(p.caption.as_deref(), p.caption_align.unwrap_or_default(), aspect)? {
            Cow::Borrowed(img) => match media.shared.clone() {
                Some(id) => id,
                None => {
                    let id = media.add(img)?;
                    media.shared = Some(id.clone());
                    id
                }
            },
            Cow::Owned(img) => media.add(&img)?,
        };
        // Los id de wp:docPr deben ser únicos en todo el documento.
        let run = anchored_picture_run(p, &page, &rel_id, scan.max_doc_pr_id + 1 + index, index);
        match runs.iter_mut().find(|(s, _)| s.key() == slot.key()) {
            Some((_, existing)) => existing.push_str(&run),
            None => runs.push((slot, run)),
        }
    }

    let new_document = apply_runs(&document_xml, runs);
    let new_content_types = ensure_png_content_type(&content_types)?;

    let mut replacements: HashMap<String, Vec<u8>> = HashMap::new();
    replacements.insert(main_part, new_document.into_bytes());
    replacements.insert(rels_part, media.rels.into_bytes());
    replacements.insert("[Content_Types].xml".into(), new_content_types.into_bytes());

    pkg.rewrite(replacements, media.files)
}

/// Imágenes nuevas del documento y sus relaciones.
struct Media {
    dir: String,
    rels: String,
    files: Vec<(String, Vec<u8>)>,
    /// Relación de la firma sin texto, reutilizada por todas las que no lo llevan.
    shared: Option<String>,
}

impl Media {
    fn add(&mut self, img: &image::RgbaImage) -> FirmarResult<String> {
        let name = format!("firmar-signature-{}.png", Uuid::new_v4().simple());
        let rel_id = unused_relationship_id(&self.rels);
        self.rels = insert_before_closing(
            &self.rels,
            "</Relationships>",
            &format!(r#"<Relationship Id="{rel_id}" Type="{REL_IMAGE}" Target="media/{name}"/>"#),
        )?;
        let zip_path = if self.dir.is_empty() { format!("media/{name}") } else { format!("{}/media/{name}", self.dir) };
        self.files.push((zip_path, crate::image_engine::encode_png(img)?));
        Ok(rel_id)
    }
}

// ---------------------------------------------------------------------------
// Paquete OPC (ZIP)
// ---------------------------------------------------------------------------

struct Package<'a> {
    archive: ZipArchive<Cursor<&'a [u8]>>,
    main_part: String,
}

impl<'a> Package<'a> {
    fn open(bytes: &'a [u8]) -> FirmarResult<Self> {
        let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(zip_err)?;
        let root_rels = read_entry(&mut archive, "_rels/.rels")?;
        let main_part = main_part_from_rels(&String::from_utf8_lossy(&root_rels))
            .unwrap_or_else(|| "word/document.xml".into());
        Ok(Self { archive, main_part })
    }

    fn read_string(&mut self, name: &str) -> FirmarResult<String> {
        String::from_utf8(read_entry(&mut self.archive, name)?)
            .map_err(|_| FirmarError::UnsupportedFormat(format!("{name} no está en UTF-8")))
    }

    fn rewrite(mut self, mut replacements: HashMap<String, Vec<u8>>, extra: Vec<(String, Vec<u8>)>) -> FirmarResult<Vec<u8>> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

        for i in 0..self.archive.len() {
            let file = self.archive.by_index(i).map_err(zip_err)?;
            let name = file.name().map_err(zip_err)?.into_owned();
            match replacements.remove(&name) {
                Some(data) => {
                    drop(file);
                    writer.start_file(name, deflated).map_err(zip_err)?;
                    writer.write_all(&data)?;
                }
                None => writer.raw_copy_file(file).map_err(zip_err)?,
            }
        }
        // Partes que no existían (p. ej. el .rels del documento).
        for (name, data) in replacements {
            writer.start_file(name, deflated).map_err(zip_err)?;
            writer.write_all(&data)?;
        }
        // Los PNG ya van comprimidos.
        let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        for (name, data) in extra {
            writer.start_file(name, stored).map_err(zip_err)?;
            writer.write_all(&data)?;
        }

        Ok(writer.finish().map_err(zip_err)?.into_inner())
    }
}

fn read_entry<R: Read + std::io::Seek>(archive: &mut ZipArchive<R>, name: &str) -> FirmarResult<Vec<u8>> {
    let file = archive
        .by_name(name)
        .map_err(|_| FirmarError::UnsupportedFormat(format!("falta la parte {name} en el .docx")))?;
    if file.size() > MAX_PART_BYTES {
        return Err(FirmarError::UnsupportedFormat(format!("la parte {name} es demasiado grande")));
    }
    let mut out = Vec::with_capacity(file.size() as usize);
    file.take(MAX_PART_BYTES + 1).read_to_end(&mut out)?;
    Ok(out)
}

fn zip_err(e: zip::result::ZipError) -> FirmarError {
    FirmarError::UnsupportedFormat(format!("el .docx no es un ZIP válido: {e}"))
}

fn main_part_from_rels(xml: &str) -> Option<String> {
    let mut reader = NsReader::from_str(xml);
    loop {
        match reader.read_event().ok()? {
            Event::Empty(e) | Event::Start(e) if e.local_name().as_ref() == "Relationship" => {
                let ty = attr(&e, "Type")?;
                if ty.ends_with(REL_OFFICE_DOCUMENT) {
                    let target = attr(&e, "Target")?;
                    return Some(target.trim_start_matches('/').to_owned());
                }
            }
            Event::Eof => return None,
            _ => {}
        }
    }
}

fn rels_path_for(part: &str) -> String {
    match part.rsplit_once('/') {
        Some((dir, file)) => format!("{dir}/_rels/{file}.rels"),
        None => format!("_rels/{part}.rels"),
    }
}

fn pages_from_app_xml(xml: &str) -> Option<u32> {
    let mut reader = NsReader::from_str(xml);
    let mut inside = false;
    loop {
        match reader.read_event().ok()? {
            Event::Start(e) => inside = e.local_name().as_ref() == "Pages",
            Event::Text(t) if inside => return t.trim().parse().ok(),
            Event::End(_) => inside = false,
            Event::Eof => return None,
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Análisis de word/document.xml
// ---------------------------------------------------------------------------

/// Dónde insertar runs dentro de un párrafo de primer nivel.
#[derive(Debug, Clone)]
enum ParagraphSlot {
    /// Offset del `</w:p>` de cierre.
    BeforeClose(usize),
    /// `<w:p .../>` vacío: rango del tag y su nombre calificado.
    SelfClosing { start: usize, end: usize, qname: String },
}

impl ParagraphSlot {
    fn key(&self) -> usize {
        match self {
            Self::BeforeClose(at) => *at,
            Self::SelfClosing { start, .. } => *start,
        }
    }
}

#[derive(Default)]
struct DocumentScan {
    first_paragraph: Option<ParagraphSlot>,
    last_paragraph: Option<ParagraphSlot>,
    first_section: Option<PageSetup>,
    final_section: Option<PageSetup>,
    max_doc_pr_id: u32,
}

fn scan_document(xml: &str) -> FirmarResult<DocumentScan> {
    let mut reader = NsReader::from_str(xml);
    let mut scan = DocumentScan::default();
    // Pila de (es_wordml, nombre_local).
    let mut stack: Vec<(bool, String)> = Vec::new();
    let mut section: Option<PageSetup> = None;
    let mut strict = false;

    loop {
        let before = reader.buffer_position() as usize;
        let (ns, event) = reader
            .read_resolved_event()
            .map_err(|e| FirmarError::UnsupportedFormat(format!("document.xml malformado: {e}")))?;
        let is_main = matches!(ns, ResolveResult::Bound(n) if n.as_ref() == NS_MAIN);
        strict |= matches!(ns, ResolveResult::Bound(n) if n.as_ref() == NS_MAIN_STRICT);
        let parent_is_body = matches!(stack.last(), Some((true, name)) if name == "body");
        // Un <w:sectPr> dentro de <w:sectPrChange> es la versión anterior (control de cambios).
        let parent_is_revision = matches!(stack.last(), Some((true, name)) if name == "sectPrChange");

        match event {
            Event::Start(e) => {
                let local = e.local_name().as_ref().to_owned();
                if is_main && local == "sectPr" && !parent_is_revision {
                    section = Some(PageSetup::default());
                }
                track_doc_pr(&e, &mut scan);
                stack.push((is_main, local));
            }
            Event::Empty(e) => {
                let local = e.local_name();
                let local = local.as_ref();
                if is_main && local == "p" && parent_is_body {
                    let slot = ParagraphSlot::SelfClosing {
                        start: before,
                        end: reader.buffer_position() as usize,
                        qname: e.name().as_ref().to_owned(),
                    };
                    scan.first_paragraph.get_or_insert_with(|| slot.clone());
                    scan.last_paragraph = Some(slot);
                } else if is_main && local == "sectPr" && !parent_is_revision {
                    // <w:sectPr/> vacío: usa los valores por defecto.
                    close_section(&mut scan, PageSetup::default(), parent_is_body);
                } else if let Some(setup) = section.as_mut()
                    && is_main && !stack.iter().any(|(m, n)| *m && n == "sectPrChange") {
                        read_page_setup(&e, local, setup);
                    }
                track_doc_pr(&e, &mut scan);
            }
            Event::End(_) => {
                let Some((was_main, local)) = stack.pop() else { continue };
                let parent_is_body = matches!(stack.last(), Some((true, name)) if name == "body");
                let parent_is_revision = matches!(stack.last(), Some((true, name)) if name == "sectPrChange");
                if was_main && local == "p" && parent_is_body {
                    let slot = ParagraphSlot::BeforeClose(before);
                    scan.first_paragraph.get_or_insert_with(|| slot.clone());
                    scan.last_paragraph = Some(slot);
                } else if was_main && local == "sectPr" && !parent_is_revision
                    && let Some(setup) = section.take() {
                        close_section(&mut scan, setup, parent_is_body);
                    }
            }
            Event::Eof => break,
            _ => {}
        }
    }

    if strict {
        return Err(FirmarError::UnsupportedFormat(
            "documentos en formato «Strict Open XML» no son compatibles; guárdalo como .docx normal".into(),
        ));
    }
    Ok(scan)
}

fn close_section(scan: &mut DocumentScan, setup: PageSetup, body_level: bool) {
    scan.first_section.get_or_insert(setup);
    if body_level {
        scan.final_section = Some(setup);
    }
}

fn read_page_setup(e: &BytesStart<'_>, local: &str, setup: &mut PageSetup) {
    let pt = |name: &str| attr(e, name).and_then(|v| v.parse::<f64>().ok()).map(|twips| twips / 20.0);
    match local {
        "pgSz" => {
            if let (Some(w), Some(h)) = (pt("w"), pt("h")) {
                setup.width = w;
                setup.height = h;
            }
        }
        "pgMar" => {
            setup.margin_top = pt("top").map(f64::abs).unwrap_or(setup.margin_top);
            setup.margin_right = pt("right").unwrap_or(setup.margin_right);
            setup.margin_bottom = pt("bottom").map(f64::abs).unwrap_or(setup.margin_bottom);
            setup.margin_left = pt("left").unwrap_or(setup.margin_left);
        }
        _ => {}
    }
}

fn track_doc_pr(e: &BytesStart<'_>, scan: &mut DocumentScan) {
    if e.local_name().as_ref() == "docPr"
        && let Some(id) = attr(e, "id").and_then(|v| v.parse::<u32>().ok()) {
            scan.max_doc_pr_id = scan.max_doc_pr_id.max(id);
        }
}

/// Valor de un atributo comparando solo el nombre local (`w:w` → `w`).
fn attr(e: &BytesStart<'_>, local: &str) -> Option<String> {
    e.attributes()
        .with_checks(false)
        .flatten()
        .find(|a| a.key.local_name().as_ref() == local)
        .and_then(|a| a.normalized_value(XmlVersion::Implicit1_0).ok().map(|v| v.into_owned()))
}

// ---------------------------------------------------------------------------
// Edición
// ---------------------------------------------------------------------------

fn apply_runs(xml: &str, mut runs: Vec<(ParagraphSlot, String)>) -> String {
    // De atrás hacia delante para que los offsets sigan siendo válidos.
    runs.sort_by_key(|(slot, _)| std::cmp::Reverse(slot.key()));
    let mut out = xml.to_owned();
    for (slot, run) in runs {
        match slot {
            ParagraphSlot::BeforeClose(at) => out.insert_str(at, &run),
            ParagraphSlot::SelfClosing { start, end, qname } => {
                let tag = &out[start..end];
                let open = tag.trim_end().trim_end_matches("/>").trim_end();
                let replacement = format!("{open}>{run}</{qname}>");
                out.replace_range(start..end, &replacement);
            }
        }
    }
    out
}

fn insert_before_closing(xml: &str, closing: &str, fragment: &str) -> FirmarResult<String> {
    let at = xml
        .rfind(closing)
        .ok_or_else(|| FirmarError::UnsupportedFormat(format!("no se encontró {closing}")))?;
    let mut out = xml.to_owned();
    out.insert_str(at, fragment);
    Ok(out)
}

fn ensure_png_content_type(xml: &str) -> FirmarResult<String> {
    let mut reader = NsReader::from_str(xml);
    loop {
        match reader
            .read_event()
            .map_err(|e| FirmarError::UnsupportedFormat(format!("[Content_Types].xml malformado: {e}")))?
        {
            Event::Empty(e) | Event::Start(e) if e.local_name().as_ref() == "Default" => {
                if attr(&e, "Extension").is_some_and(|ext| ext.eq_ignore_ascii_case("png")) {
                    return Ok(xml.to_owned());
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    insert_before_closing(xml, "</Types>", r#"<Default Extension="png" ContentType="image/png"/>"#)
}

fn unused_relationship_id(rels_xml: &str) -> String {
    (1..)
        .map(|i| format!("rIdFirmar{i}"))
        .find(|id| !rels_xml.contains(&format!("\"{id}\"")))
        .expect("rango infinito")
}

fn anchored_picture_run(p: &SignaturePlacement, page: &PageSetup, rel_id: &str, doc_pr_id: u32, index: u32) -> String {
    let emu = |points: f64| (points * 20.0 * EMU_PER_TWIP).round() as i64;
    let x = emu(p.x * page.width);
    let y = emu(p.y * page.height);
    let cx = emu(p.width * page.width).max(1);
    let cy = emu(p.height * page.height).max(1);
    let z = 251_659_264 + index;
    format!(
        concat!(
            r#"<w:r xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:drawing>"#,
            r#"<wp:anchor xmlns:wp="{wp}" distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="{z}" behindDoc="0" locked="1" layoutInCell="1" allowOverlap="1">"#,
            r#"<wp:simplePos x="0" y="0"/>"#,
            r#"<wp:positionH relativeFrom="page"><wp:posOffset>{x}</wp:posOffset></wp:positionH>"#,
            r#"<wp:positionV relativeFrom="page"><wp:posOffset>{y}</wp:posOffset></wp:positionV>"#,
            r#"<wp:extent cx="{cx}" cy="{cy}"/><wp:effectExtent l="0" t="0" r="0" b="0"/><wp:wrapNone/>"#,
            r#"<wp:docPr id="{id}" name="Firmar {id}" descr="Firma estampada con Firmar Vecipp"/>"#,
            r#"<wp:cNvGraphicFramePr><a:graphicFrameLocks xmlns:a="{a}" noChangeAspect="1"/></wp:cNvGraphicFramePr>"#,
            r#"<a:graphic xmlns:a="{a}"><a:graphicData uri="{pic}"><pic:pic xmlns:pic="{pic}">"#,
            r#"<pic:nvPicPr><pic:cNvPr id="0" name="firma.png"/><pic:cNvPicPr/></pic:nvPicPr>"#,
            r#"<pic:blipFill><a:blip xmlns:r="{r}" r:embed="{rel}"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>"#,
            r#"<pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr>"#,
            r#"</pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#,
        ),
        wp = NS_WP,
        a = NS_A,
        pic = NS_PIC,
        r = NS_R,
        rel = rel_id,
        id = doc_pr_id,
        z = z,
        x = x,
        y = y,
        cx = cx,
        cy = cy,
    )
}


#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn sample_docx(body: &str) -> Vec<u8> {
        let mut w = ZipWriter::new(Cursor::new(Vec::new()));
        let opts = SimpleFileOptions::default();
        let files = [
            ("[Content_Types].xml", r#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#.to_owned()),
            ("_rels/.rels", r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#.to_owned()),
            ("word/document.xml", format!(r#"<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}</w:body></w:document>"#)),
            ("docProps/app.xml", r#"<?xml version="1.0"?><Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"><Pages>3</Pages></Properties>"#.to_owned()),
        ];
        for (name, content) in files {
            w.start_file(name, opts).unwrap();
            w.write_all(content.as_bytes()).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    fn signature_png() -> SignatureArt {
        SignatureArt::new(image::RgbaImage::from_pixel(40, 10, image::Rgba([0, 0, 0, 255])))
    }

    fn read(bytes: &[u8], name: &str) -> String {
        let mut z = ZipArchive::new(Cursor::new(bytes)).unwrap();
        String::from_utf8(read_entry(&mut z, name).unwrap()).unwrap()
    }

    const BODY: &str = r#"<w:p><w:r><w:t>Primero</w:t></w:r></w:p><w:tbl><w:tr><w:tc><w:p><w:r><w:t>celda</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p><w:pPr><w:sectPr><w:pgSz w:w="16838" w:h="11906"/></w:sectPr></w:pPr></w:p><w:p><w:r><w:t>Último</w:t></w:r></w:p><w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1417" w:right="1701" w:bottom="1417" w:left="1701"/></w:sectPr>"#;

    #[test]
    fn layout_reads_sections_and_page_count() {
        let l = layout(&sample_docx(BODY)).unwrap();
        assert_eq!(l.pages_hint, Some(3));
        assert_eq!(l.pages.len(), 2);
        assert!((l.pages[0].width - 841.9).abs() < 0.1, "primera sección apaisada");
        assert!((l.pages[1].width - 595.3).abs() < 0.1, "última sección A4 vertical");
        assert!((l.pages[1].margin_left - 85.05).abs() < 0.01);
    }

    #[test]
    fn stamps_first_and_last_page() {
        let docx = sample_docx(BODY);
        let placements = [
            SignaturePlacement { page: FIRST_PAGE, x: 0.1, y: 0.1, width: 0.2, height: 0.05, caption: None, caption_align: None },
            SignaturePlacement { page: LAST_PAGE, x: 0.6, y: 0.8, width: 0.3, height: 0.075, caption: None, caption_align: None },
        ];
        let out = stamp(&docx, &signature_png(), &placements).unwrap();

        let doc = read(&out, "word/document.xml");
        assert_eq!(doc.matches("<wp:anchor").count(), 2);
        let first = doc.find("Primero").unwrap();
        let first_anchor = doc.find("<wp:anchor").unwrap();
        let table = doc.find("<w:tbl>").unwrap();
        assert!(first < first_anchor && first_anchor < table, "ancla en el primer párrafo");
        let last = doc.rfind("Último").unwrap();
        assert!(doc.rfind("<wp:anchor").unwrap() > last, "ancla en el último párrafo");
        assert!(!doc[table..doc.find("</w:tbl>").unwrap()].contains("wp:anchor"), "nunca dentro de tablas");
        // Posición: 0.6 × 595.3 pt × 12700 EMU/pt
        assert!(doc.contains("<wp:posOffset>4536186</wp:posOffset>"));
        scan_document(&doc).expect("el XML resultante debe seguir siendo válido");

        let rels = read(&out, "word/_rels/document.xml.rels");
        assert!(rels.contains("rIdFirmar1") && rels.contains("media/firmar-signature-"));
        assert!(read(&out, "[Content_Types].xml").contains(r#"Extension="png""#));

        let z = ZipArchive::new(Cursor::new(out.as_slice())).unwrap();
        let media = z.file_names().flatten().filter(|n| n.starts_with("word/media/firmar-signature-")).count();
        assert_eq!(media, 1);
        assert!(z.comment().is_empty());
    }

    #[test]
    fn captioned_signature_gets_its_own_image() {
        let docx = sample_docx(BODY);
        let plain = SignaturePlacement { page: FIRST_PAGE, x: 0.1, y: 0.1, width: 0.2, height: 0.05, caption: None, caption_align: None };
        let text = SignaturePlacement { page: LAST_PAGE, caption: Some("Juan".into()), height: 0.1, ..plain.clone() };
        let out = stamp(&docx, &signature_png(), &[plain, text]).unwrap();
        let z = ZipArchive::new(Cursor::new(out.as_slice())).unwrap();
        let media = z.file_names().flatten().filter(|n| n.starts_with("word/media/firmar-signature-")).count();
        assert_eq!(media, 2);
        let rels = read(&out, "word/_rels/document.xml.rels");
        assert!(rels.contains("rIdFirmar1") && rels.contains("rIdFirmar2"));
    }

    #[test]
    fn handles_single_empty_paragraph() {
        let docx = sample_docx(r#"<w:p w:rsidR="00AB"/><w:sectPr/>"#);
        let p = SignaturePlacement { page: LAST_PAGE, x: 0.0, y: 0.0, width: 0.5, height: 0.1, caption: None, caption_align: None };
        let out = stamp(&docx, &signature_png(), &[p.clone(), SignaturePlacement { page: FIRST_PAGE, ..p }]).unwrap();
        let doc = read(&out, "word/document.xml");
        assert!(doc.contains(r#"<w:p w:rsidR="00AB"><w:r "#));
        assert_eq!(doc.matches("<wp:anchor").count(), 2);
        assert!(doc.contains("</w:p><w:sectPr/>"));
        scan_document(&doc).unwrap();
    }

    #[test]
    fn rejects_other_pages_and_bad_zip() {
        let p = SignaturePlacement { page: 2, x: 0.0, y: 0.0, width: 0.1, height: 0.1, caption: None, caption_align: None };
        assert!(stamp(&sample_docx(BODY), &signature_png(), &[p]).is_err());
        assert!(layout(b"PK\x03\x04 no es zip word/document.xml").is_err());
        assert!(is_docx(&sample_docx(BODY)));
    }
}
