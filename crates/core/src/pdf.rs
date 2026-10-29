//! Lecture des PDF avec PDFium (le moteur PDF de Chrome).

use std::path::PathBuf;
use std::sync::OnceLock;

use pdfium_render::prelude::*;

use crate::document::{DocDetails, NormRect, PageSize, PageText, Rotation};
use crate::{Error, Result};

/// 1 point PDF (1/72 de pouce) = 4/3 pixel d'écran (1/96 de pouce).
const PX_PER_PT: f32 = 96.0 / 72.0;

static ENGINE: OnceLock<std::result::Result<Pdfium, String>> = OnceLock::new();

/// Démarre PDFium une seule fois pour toute l'appli, en cherchant la bibliothèque
/// dans les dossiers donnés, puis dans le système.
pub(crate) fn engine(dirs: &[PathBuf]) -> Result<&'static Pdfium> {
    ENGINE
        .get_or_init(|| {
            let bindings = dirs
                .iter()
                .map(Pdfium::pdfium_platform_library_name_at_path)
                .filter(|p| p.exists())
                .find_map(|p| Pdfium::bind_to_library(p).ok())
                .map(Ok)
                .unwrap_or_else(Pdfium::bind_to_system_library)
                .map_err(|e| e.to_string())?;
            Ok(Pdfium::new(bindings))
        })
        .as_ref()
        .map_err(|e| Error::PdfEngineUnavailable(e.clone()))
}

pub(crate) struct PdfDoc {
    doc: PdfDocument<'static>,
    /// Le fichier d'origine, pour l'enregistrer en ne changeant que l'ordre des pages.
    pub bytes: Vec<u8>,
}

fn broken(e: PdfiumError) -> Error {
    match e {
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError) => {
            Error::PasswordProtected
        }
        e => Error::BrokenPdf(e.to_string()),
    }
}

fn intrinsic_rotation(page: &PdfPage) -> Rotation {
    match page.rotation() {
        Ok(PdfPageRenderRotation::Degrees90) => Rotation::R90,
        Ok(PdfPageRenderRotation::Degrees180) => Rotation::R180,
        Ok(PdfPageRenderRotation::Degrees270) => Rotation::R270,
        _ => Rotation::R0,
    }
}

fn to_pdfium(rotation: Rotation) -> PdfPageRenderRotation {
    match rotation {
        Rotation::R0 => PdfPageRenderRotation::None,
        Rotation::R90 => PdfPageRenderRotation::Degrees90,
        Rotation::R180 => PdfPageRenderRotation::Degrees180,
        Rotation::R270 => PdfPageRenderRotation::Degrees270,
    }
}

/// Une page du futur PDF : une page d'un PDF ouvert, ou une image.
pub(crate) enum Source<'a> {
    Page(&'a PdfDoc, usize, Rotation),
    /// Image déjà tournée, en JPEG prêt à intégrer (ou en image brute si elle a de la transparence).
    Jpeg(Vec<u8>, u32, u32),
    Image(image::DynamicImage),
}

/// Taille de page pour une image : 72 points par pixel, réduite pour tenir dans un A4.
fn picture_page_size(width: u32, height: u32) -> (f32, f32) {
    let (w, h) = (width as f32, height as f32);
    let (max_w, max_h) = if w > h { (842.0, 595.0) } else { (595.0, 842.0) };
    let s = (max_w / w).min(max_h / h).min(1.0);
    (w * s, h * s)
}

/// Fabrique un nouveau PDF à partir de pages de PDF ouverts et d'images.
pub(crate) fn compose(pdfium: &'static Pdfium, sources: Vec<Source>) -> Result<Vec<u8>> {
    let mut out = pdfium.create_new_pdf().map_err(broken)?;
    for source in sources {
        let index = out.pages().len();
        match source {
            Source::Page(doc, page, rotation) => {
                let from = i32::try_from(page).map_err(|_| Error::UnknownPage)?;
                out.pages_mut().copy_page_from_document(&doc.doc, from, index).map_err(broken)?;
                let mut copied = out.pages().get(index).map_err(broken)?;
                let total = intrinsic_rotation(&copied).then(rotation);
                copied.set_rotation(to_pdfium(total));
            }
            Source::Jpeg(bytes, width, height) => {
                let (w, h) = picture_page_size(width, height);
                let mut page = out
                    .pages_mut()
                    .create_page_at_end(PdfPagePaperSize::Custom(PdfPoints::new(w), PdfPoints::new(h)))
                    .map_err(broken)?;
                let mut object =
                    PdfPageImageObject::new_from_jpeg_reader(&out, std::io::Cursor::new(bytes)).map_err(broken)?;
                object.scale(w, h).map_err(broken)?;
                page.objects_mut().add_image_object(object).map_err(broken)?;
            }
            Source::Image(image) => {
                let (w, h) = picture_page_size(image.width(), image.height());
                let mut page = out
                    .pages_mut()
                    .create_page_at_end(PdfPagePaperSize::Custom(PdfPoints::new(w), PdfPoints::new(h)))
                    .map_err(broken)?;
                let object =
                    PdfPageImageObject::new_with_size(&out, &image, PdfPoints::new(w), PdfPoints::new(h)).map_err(broken)?;
                page.objects_mut().add_image_object(object).map_err(broken)?;
            }
        }
    }
    out.save_to_bytes().map_err(broken)
}

impl PdfDoc {
    /// Le fichier est lu en mémoire : l'original n'est jamais verrouillé ni modifié.
    pub fn open(pdfium: &'static Pdfium, bytes: Vec<u8>) -> Result<Self> {
        let doc = pdfium.load_pdf_from_byte_vec(bytes.clone(), None).map_err(broken)?;
        Ok(Self { doc, bytes })
    }

    fn page(&self, index: usize) -> Result<PdfPage<'_>> {
        let index = i32::try_from(index).map_err(|_| Error::UnknownPage)?;
        self.doc.pages().get(index).map_err(|_| Error::UnknownPage)
    }

    /// Les informations du PDF (titre, auteur, logiciel, dates, version).
    pub fn details(&self, d: &mut DocDetails) {
        let meta = self.doc.metadata();
        let tag = |t| meta.get(t).map(|m| m.value().trim().to_string()).filter(|v| !v.is_empty());
        d.title = tag(PdfDocumentMetadataTagType::Title);
        d.author = tag(PdfDocumentMetadataTagType::Author);
        d.subject = tag(PdfDocumentMetadataTagType::Subject);
        d.keywords = tag(PdfDocumentMetadataTagType::Keywords);
        d.creator = tag(PdfDocumentMetadataTagType::Creator);
        d.producer = tag(PdfDocumentMetadataTagType::Producer);
        d.created = tag(PdfDocumentMetadataTagType::CreationDate).and_then(|v| pdf_date(&v));
        d.modified = tag(PdfDocumentMetadataTagType::ModificationDate).and_then(|v| pdf_date(&v));
        d.version = header_version(&self.bytes);
    }

    /// Largeur d'une page à 100 %, en pixels (rotation du PDF comprise).
    pub fn page_width(&self, index: usize) -> Result<f32> {
        Ok(self.page(index)?.width().value * PX_PER_PT)
    }

    /// Taille de chaque page telle qu'elle s'affiche (rotation du PDF comprise).
    pub fn page_sizes(&self) -> Vec<PageSize> {
        self.doc
            .pages()
            .iter()
            .map(|p| PageSize { width: p.width().value * PX_PER_PT, height: p.height().value * PX_PER_PT })
            .collect()
    }

    /// Rend une page à la largeur voulue (en pixels), après la rotation demandée.
    /// `forms` : dessiner aussi les champs de formulaire (sinon l'interface les affiche elle-même).
    pub fn render(&self, index: usize, width: u32, rotation: Rotation, forms: bool) -> Result<image::DynamicImage> {
        let page = self.page(index)?;
        let (w, h) = (page.width().value, page.height().value);
        let (w, h) = if rotation.is_sideways() { (h, w) } else { (w, h) };
        let width = width.clamp(1, 8000);
        let height = ((width as f32) * h / w).round().max(1.0) as i32;
        let config = PdfRenderConfig::new()
            .set_target_size(width as i32, height)
            .rotate(to_pdfium(rotation), false)
            .render_form_data(forms)
            .render_annotations(true);
        page.render_with_config(&config)
            .and_then(|bitmap| bitmap.as_image())
            .map_err(broken)
    }

    /// Cherche un texte (sans tenir compte des majuscules) dans toutes les pages.
    pub fn search(&self, query: &str) -> Vec<(usize, Vec<NormRect>, String)> {
        let options = PdfSearchOptions::new().match_case(false);
        let mut hits = Vec::new();
        for (index, page) in self.doc.pages().iter().enumerate() {
            let Ok(text) = page.text() else { continue };
            let Ok(search) = text.search(query, &options) else { continue };
            let flat = Flat::of(&page);
            for segments in search.iter(PdfSearchDirection::SearchForward) {
                let mut rects = Vec::new();
                let mut context = String::new();
                for segment in segments.iter() {
                    let b = segment.bounds();
                    rects.push(flat.norm(&b));
                    if context.is_empty() {
                        // Toute la ligne où se trouve le mot.
                        let line = PdfRect::new_from_values(b.bottom().value, flat.left, b.top().value, flat.left + flat.w);
                        context = text.inside_rect(line).split_whitespace().collect::<Vec<_>>().join(" ");
                    }
                }
                if !rects.is_empty() {
                    hits.push((index, rects, context));
                }
            }
        }
        hits
    }

    /// Le texte d'une page, avec la place de chaque caractère.
    pub fn page_text(&self, index: usize) -> Result<PageText> {
        let page = self.page(index)?;
        let text = page.text().map_err(broken)?;
        let flat = Flat::of(&page);
        let mut out = PageText::default();
        for ch in text.chars().iter() {
            let Some(c) = ch.unicode_char() else { continue };
            if c == '\r' {
                continue; // PDFium met « \r\n » en fin de ligne : on garde seulement « \n ».
            }
            let generated = ch.is_generated().unwrap_or(false);
            let rect = match ch.loose_bounds() {
                Ok(b) if !generated && !c.is_whitespace() => flat.norm(&b),
                _ => NormRect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 },
            };
            out.chars.push(c);
            out.rects.push(rect);
        }
        Ok(out)
    }
}

/// La page « à plat » (sans sa rotation), dans laquelle PDFium donne la place du texte.
struct Flat {
    left: f32,
    bottom: f32,
    w: f32,
    h: f32,
    rotation: Rotation,
}

impl Flat {
    fn of(page: &PdfPage) -> Flat {
        let rotation = intrinsic_rotation(page);
        let (w, h) = (page.width().value, page.height().value);
        let (w, h) = if rotation.is_sideways() { (h, w) } else { (w, h) };
        let (left, bottom) = page
            .boundaries()
            .bounding()
            .map(|b| (b.bounds.left().value, b.bounds.bottom().value))
            .unwrap_or((0.0, 0.0));
        Flat { left, bottom, w, h, rotation }
    }

    /// Rectangle PDF → proportions de la page telle qu'elle s'affiche.
    fn norm(&self, b: &PdfRect) -> NormRect {
        NormRect {
            x: (b.left().value - self.left) / self.w,
            y: (self.bottom + self.h - b.top().value) / self.h,
            w: (b.right().value - b.left().value) / self.w,
            h: (b.top().value - b.bottom().value) / self.h,
        }
        .rotated(self.rotation)
    }
}

/// « %PDF-1.7 » au début du fichier → « 1.7 ».
pub(crate) fn header_version(bytes: &[u8]) -> Option<String> {
    let head = &bytes[..bytes.len().min(1024)];
    let at = head.windows(5).position(|w| w == b"%PDF-")? + 5;
    let v: String = head[at..].iter().take_while(|b| b.is_ascii_digit() || **b == b'.').map(|b| *b as char).collect();
    (!v.is_empty()).then_some(v)
}

/// Une date de PDF (« D:20261027140500+01'00' ») → « 2026-10-27T14:05:00 » (heure locale du fichier).
pub(crate) fn pdf_date(raw: &str) -> Option<String> {
    let digits: String = raw.trim().trim_start_matches("D:").chars().take_while(char::is_ascii_digit).collect();
    if digits.len() < 4 {
        return None;
    }
    let part = |from: usize, default: &str| digits.get(from..from + 2).unwrap_or(default).to_string();
    Some(format!("{}-{}-{}T{}:{}:{}", &digits[..4], part(4, "01"), part(6, "01"), part(8, "00"), part(10, "00"), part(12, "00")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_et_version_des_pdf() {
        assert_eq!(pdf_date("D:20261027140503+01'00'").as_deref(), Some("2026-10-27T14:05:03"));
        assert_eq!(pdf_date("D:2025").as_deref(), Some("2025-01-01T00:00:00"));
        assert_eq!(pdf_date("hier"), None);
        assert_eq!(header_version(b"%PDF-1.7\n%...").as_deref(), Some("1.7"));
        assert_eq!(header_version(b"pas un pdf"), None);
    }
}
