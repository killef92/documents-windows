//! La « bibliothèque » : tous les documents ouverts dans une fenêtre.

use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::codecs::gif::{GifDecoder, GifEncoder, Repeat};
use image::{imageops, AnimationDecoder, DynamicImage, Frame};

use crate::annotate::{self, PageAnnotations};
use crate::document::{DocDetails, DocId, DocInfo, PageRef, PageText, RenderedImage, Rotation, SearchHit};
use crate::pdf::{self, PdfDoc, Source};
use crate::picture::{Picture, Retouch};
use crate::form::{self, DocFill, FormField};
use crate::{exif, rearrange, reduce};
use crate::{DocumentKind, Error, Result};

/// Au-dessous de cette largeur (miniatures), un GIF tourné est montré en image fixe.
const GIF_STILL_BELOW: u32 = 300;

enum Content {
    Pdf(PdfDoc),
    Picture(Picture),
}

struct Doc {
    kind: DocumentKind,
    content: Content,
    info: DocInfo,
    /// Le dernier GIF animé tourné, pour ne pas le refaire à chaque zoom.
    turned_gif: Mutex<Option<(Rotation, Vec<u8>)>>,
}

pub struct Library {
    pdfium_dirs: Vec<PathBuf>,
    docs: BTreeMap<DocId, Doc>,
    next_id: DocId,
}

impl Library {
    /// `pdfium_dirs` : dossiers où chercher la bibliothèque PDFium (en plus du système).
    pub fn new(pdfium_dirs: Vec<PathBuf>) -> Self {
        Self { pdfium_dirs, docs: BTreeMap::new(), next_id: 1 }
    }

    /// Ouvre un fichier. L'original est seulement lu, jamais modifié.
    pub fn open(&mut self, path: &Path) -> Result<DocInfo> {
        let bytes = std::fs::read(path)?;
        let kind = DocumentKind::from_bytes(&bytes).ok_or(Error::UnsupportedFormat)?;
        let (content, pages) = match kind {
            DocumentKind::Heic => return Err(Error::HeicNotYetSupported),
            DocumentKind::Pdf => {
                let doc = PdfDoc::open(pdf::engine(&self.pdfium_dirs)?, bytes)?;
                let pages = doc.page_sizes();
                (Content::Pdf(doc), pages)
            }
            _ => {
                let picture = Picture::open(&bytes)?;
                let pages = vec![picture.size()];
                (Content::Picture(picture), pages)
            }
        };
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let info = DocInfo { id: 0, name, path: path.to_path_buf(), kind, pages, edited: false };
        Ok(self.insert(kind, content, info))
    }

    fn insert(&mut self, kind: DocumentKind, content: Content, mut info: DocInfo) -> DocInfo {
        info.id = self.next_id;
        self.next_id += 1;
        self.docs.insert(info.id, Doc { kind, content, info: info.clone(), turned_gif: Mutex::default() });
        info
    }

    /// Retouche une photo : crée un nouveau document en mémoire (l'ancien reste ouvert, pour
    /// pouvoir annuler). Le fichier n'est réécrit qu'à l'enregistrement.
    pub fn retouch_picture(&mut self, id: DocId, retouch: &Retouch) -> Result<DocInfo> {
        let doc = self.doc(id)?;
        let Content::Picture(picture) = &doc.content else {
            return Err(Error::UnsupportedFormat);
        };
        let picture = picture.retouched(retouch)?;
        let info = DocInfo { pages: vec![picture.size()], edited: true, ..doc.info.clone() };
        let kind = doc.kind;
        Ok(self.insert(kind, Content::Picture(picture), info))
    }

    pub fn close(&mut self, id: DocId) {
        self.docs.remove(&id);
    }

    /// Rend une page en image (PNG pour les PDF, pour un texte net ; JPEG pour les photos).
    pub fn render(&self, id: DocId, page: usize, width: u32, rotation: Rotation) -> Result<RenderedImage> {
        self.render_with(id, page, width, rotation, true)
    }

    /// Comme [`Library::render`] ; `forms` à faux : sans les champs de formulaire, que
    /// l'interface affiche elle-même pour qu'on puisse les remplir.
    pub fn render_with(&self, id: DocId, page: usize, width: u32, rotation: Rotation, forms: bool) -> Result<RenderedImage> {
        let doc = self.docs.get(&id).ok_or(Error::UnknownDocument)?;
        match &doc.content {
            Content::Pdf(pdf) => encode_png(pdf.render(page, width, rotation, forms)?),
            Content::Picture(picture) => {
                if page != 0 {
                    return Err(Error::UnknownPage);
                }
                // Un GIF reste un GIF : le navigateur l'anime et le met lui-même à la bonne taille.
                // Tourné, il est refait image par image (sauf pour les petites miniatures).
                if doc.kind == DocumentKind::Gif && !doc.info.edited && (rotation == Rotation::R0 || width > GIF_STILL_BELOW) {
                    let size = picture.size();
                    let (w, h) = (size.width as u32, size.height as u32);
                    let bytes = if rotation == Rotation::R0 {
                        picture.bytes.clone()
                    } else {
                        let mut turned = doc.turned_gif.lock().unwrap();
                        match &*turned {
                            Some((r, bytes)) if *r == rotation => bytes.clone(),
                            _ => {
                                let bytes = rotate_gif(&picture.bytes, rotation)?;
                                *turned = Some((rotation, bytes.clone()));
                                bytes
                            }
                        }
                    };
                    let (width, height) = if rotation.is_sideways() { (h, w) } else { (w, h) };
                    return Ok(RenderedImage { bytes, mime: "image/gif", width, height });
                }
                let image = picture.render(width, rotation);
                if picture.has_alpha() || doc.kind == DocumentKind::Png {
                    encode_png(image)
                } else {
                    encode_jpeg(image)
                }
            }
        }
    }

    /// Les informations de l'inspecteur : taille et dates du fichier, et celles du PDF.
    pub fn details(&self, id: DocId) -> Result<DocDetails> {
        let doc = self.doc(id)?;
        let mut d = DocDetails::default();
        if let Ok(meta) = std::fs::metadata(&doc.info.path) {
            let ms = |t: std::io::Result<std::time::SystemTime>| {
                t.ok()?.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_millis() as i64)
            };
            d.file_size = meta.len();
            d.file_created = ms(meta.created());
            d.file_modified = ms(meta.modified());
        }
        if let Content::Pdf(pdf) = &doc.content {
            pdf.details(&mut d);
        }
        Ok(d)
    }

    /// Cherche un texte dans tous les PDF ouverts, dans l'ordre d'ouverture.
    pub fn search(&self, query: &str) -> Vec<SearchHit> {
        let query = query.trim();
        if query.is_empty() {
            return Vec::new();
        }
        self.docs
            .iter()
            .filter_map(|(id, doc)| match &doc.content {
                Content::Pdf(pdf) => Some((*id, pdf)),
                Content::Picture(_) => None,
            })
            .flat_map(|(doc, pdf)| {
                pdf.search(query)
                    .into_iter()
                    .map(move |(page, rects, context)| SearchHit { doc, page, rects, context })
            })
            .collect()
    }
}

impl Library {
    fn doc(&self, id: DocId) -> Result<&Doc> {
        self.docs.get(&id).ok_or(Error::UnknownDocument)
    }

    /// Enregistre les pages données, dans cet ordre, en un seul PDF.
    ///
    /// Si toutes les pages viennent d'un même PDF, on garde ce PDF et on ne change que l'ordre
    /// et la rotation (les formulaires, liens et signets restent intacts). Sinon on fabrique
    /// un nouveau PDF avec PDFium.
    pub fn save_pdf(&self, pages: &[PageRef], dest: &Path) -> Result<()> {
        self.save_pdf_filled(pages, &[], dest)
    }

    /// Comme [`Library::save_pdf`], en remplissant aussi les champs de formulaire donnés.
    pub fn save_pdf_filled(&self, pages: &[PageRef], fills: &[DocFill], dest: &Path) -> Result<()> {
        write_atomically(dest, &self.pdf_bytes(pages, fills)?)
    }

    /// Comme [`Library::save_pdf_filled`], puis réduit la taille du fichier (photos allégées).
    /// Renvoie la taille avant et après, en octets.
    pub fn save_pdf_reduced(&self, pages: &[PageRef], fills: &[DocFill], level: reduce::Level, dest: &Path) -> Result<(u64, u64)> {
        let bytes = self.pdf_bytes(pages, fills)?;
        let smaller = reduce::reduce(&bytes, level)?;
        write_atomically(dest, &smaller)?;
        Ok((bytes.len() as u64, smaller.len() as u64))
    }

    /// Le PDF tel qu'il serait enregistré, en mémoire.
    fn pdf_bytes(&self, pages: &[PageRef], fills: &[DocFill]) -> Result<Vec<u8>> {
        if pages.is_empty() {
            return Err(Error::NothingToSave);
        }
        // D'abord les formulaires remplis : une copie en mémoire de chaque PDF concerné.
        let mut filled: BTreeMap<DocId, Vec<u8>> = BTreeMap::new();
        for f in fills.iter().filter(|f| !f.fields.is_empty()) {
            if let Content::Pdf(pdf) = &self.doc(f.doc)?.content {
                let bytes = form::fill(&pdf.bytes, &f.fields)
                    .map_err(|e| Error::SaveFailed(format!("the form couldn't be filled ({e})")))?;
                filled.insert(f.doc, bytes);
            }
        }
        let first = pages[0].doc;
        if pages.iter().all(|p| p.doc == first) {
            if let Content::Pdf(pdf) = &self.doc(first)?.content {
                let order: Vec<_> = pages.iter().map(|p| (p.page, Rotation::from_degrees(p.rotation))).collect();
                let source = filled.get(&first).unwrap_or(&pdf.bytes);
                if let Ok(bytes) = rearrange::rearrange(source, &order) {
                    return self.annotate(bytes, pages);
                }
                // PDF chiffré ou trop particulier pour lopdf : on passe par PDFium.
            }
        }
        let pdfium = pdf::engine(&self.pdfium_dirs)?;
        let mut reopened: BTreeMap<DocId, PdfDoc> = BTreeMap::new();
        for (id, bytes) in filled {
            reopened.insert(id, PdfDoc::open(pdfium, bytes)?);
        }
        let mut sources = Vec::with_capacity(pages.len());
        for p in pages {
            let rotation = Rotation::from_degrees(p.rotation);
            sources.push(match &self.doc(p.doc)?.content {
                Content::Pdf(pdf) => Source::Page(reopened.get(&p.doc).unwrap_or(pdf), p.page, rotation),
                Content::Picture(picture) => {
                    let image = picture.full(rotation);
                    if picture.has_alpha() {
                        Source::Image(image)
                    } else {
                        let (w, h) = (image.width(), image.height());
                        Source::Jpeg(encode_jpeg(image)?.bytes, w, h)
                    }
                }
            });
        }
        let bytes = pdf::compose(pdfium, sources)?;
        // Les champs de formulaire ne survivent pas à la fusion : on les dessine dans les pages.
        let bytes = form::flatten_orphan_widgets(bytes).map_err(Error::SaveFailed)?;
        self.annotate(bytes, pages)
    }

    /// Ajoute aux pages du PDF fabriqué les annotations de chaque page.
    fn annotate(&self, bytes: Vec<u8>, pages: &[PageRef]) -> Result<Vec<u8>> {
        let mut list = Vec::with_capacity(pages.len());
        for p in pages {
            let unit_width = match &self.doc(p.doc)?.content {
                Content::Pdf(pdf) => pdf.page_width(p.page)?,
                Content::Picture(picture) => picture.size().width,
            };
            list.push(PageAnnotations {
                annotations: &p.annotations,
                rotation: Rotation::from_degrees(p.rotation),
                unit_width,
            });
        }
        annotate::apply(bytes, &list).map_err(Error::SaveFailed)
    }

    /// Les champs de formulaire d'un PDF (aucun pour une photo).
    pub fn form_fields(&self, id: DocId) -> Result<Vec<FormField>> {
        match &self.doc(id)?.content {
            Content::Pdf(pdf) => Ok(form::fields(&pdf.bytes)),
            Content::Picture(_) => Ok(Vec::new()),
        }
    }

    /// Le texte d'une page de PDF (vide pour une photo).
    pub fn page_text(&self, id: DocId, page: usize) -> Result<PageText> {
        match &self.doc(id)?.content {
            Content::Pdf(pdf) => pdf.page_text(page),
            Content::Picture(_) => Ok(PageText::default()),
        }
    }

    /// Enregistre une photo à partir de son image finale (annotations dessinées dedans par
    /// l'interface), dans le format du fichier d'origine.
    pub fn save_picture_pixels(&self, id: DocId, png: &[u8], dest: &Path) -> Result<()> {
        let doc = self.doc(id)?;
        let image = image::load_from_memory_with_format(png, image::ImageFormat::Png)
            .map_err(|e| Error::BrokenImage(e.to_string()))?;
        let bytes = match doc.kind {
            DocumentKind::Jpeg => encode_jpeg(image)?.bytes,
            DocumentKind::Webp => {
                let mut out = Cursor::new(Vec::new());
                image.write_to(&mut out, image::ImageFormat::WebP).map_err(|e| Error::SaveFailed(e.to_string()))?;
                out.into_inner()
            }
            DocumentKind::Gif => encode_gif(image)?,
            _ => encode_png(image)?.bytes,
        };
        write_atomically(dest, &bytes)
    }

    /// Enregistre une photo tournée. Pour un JPEG, sans perte : seule l'orientation change.
    pub fn save_picture(&self, id: DocId, rotation: i32, dest: &Path) -> Result<()> {
        let doc = self.doc(id)?;
        let Content::Picture(picture) = &doc.content else {
            return Err(Error::UnsupportedFormat);
        };
        let rotation = Rotation::from_degrees(rotation);
        if doc.kind == DocumentKind::Jpeg && !doc.info.edited {
            if let Some(current) = exif::rotation_of(picture.orientation) {
                let orientation = exif::orientation_of(current.then(rotation));
                if let Some(bytes) = exif::with_orientation(&picture.bytes, orientation) {
                    return write_atomically(dest, &bytes);
                }
            }
        }
        if rotation == Rotation::R0 && !doc.info.edited {
            return write_atomically(dest, &picture.bytes);
        }
        if doc.kind == DocumentKind::Gif && !doc.info.edited {
            // GIF animé tourné : chaque image est tournée, l'animation est gardée.
            return write_atomically(dest, &rotate_gif(&picture.bytes, rotation)?);
        }
        let image = picture.full(rotation);
        let bytes = match doc.kind {
            DocumentKind::Jpeg => encode_jpeg(image)?.bytes,
            DocumentKind::Webp => {
                let mut out = Cursor::new(Vec::new());
                image.write_to(&mut out, image::ImageFormat::WebP).map_err(|e| Error::SaveFailed(e.to_string()))?;
                out.into_inner()
            }
            DocumentKind::Gif => encode_gif(image)?,
            _ => encode_png(image)?.bytes,
        };
        write_atomically(dest, &bytes)
    }
}

/// Écrit d'abord un fichier temporaire à côté, puis le renomme : si quelque chose se passe mal,
/// l'ancien fichier reste intact.
fn write_atomically(dest: &Path, bytes: &[u8]) -> Result<()> {
    let name = dest.file_name().ok_or_else(|| Error::SaveFailed("invalid file name".into()))?;
    let tmp = dest.with_file_name(format!(".{}.documents-tmp", name.to_string_lossy()));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, dest).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })?;
    Ok(())
}

fn encode_png(image: DynamicImage) -> Result<RenderedImage> {
    let (width, height) = (image.width(), image.height());
    let mut bytes = Vec::new();
    let encoder = PngEncoder::new_with_quality(&mut bytes, CompressionType::Fast, FilterType::Adaptive);
    image.write_with_encoder(encoder).map_err(|e| Error::BrokenImage(e.to_string()))?;
    Ok(RenderedImage { bytes, mime: "image/png", width, height })
}

/// Un GIF d'une seule image (après retouche ou annotation : l'animation n'a plus de sens).
fn encode_gif(image: DynamicImage) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    GifEncoder::new_with_speed(&mut out, 10)
        .encode(image.to_rgba8().as_raw(), image.width(), image.height(), image::ExtendedColorType::Rgba8)
        .map_err(|e| Error::SaveFailed(e.to_string()))?;
    Ok(out)
}

/// Tourne toutes les images d'un GIF, en gardant leurs durées et la répétition.
fn rotate_gif(bytes: &[u8], rotation: Rotation) -> Result<Vec<u8>> {
    let broken = |e: image::ImageError| Error::BrokenImage(e.to_string());
    let frames = GifDecoder::new(Cursor::new(bytes)).map_err(broken)?.into_frames().collect_frames().map_err(broken)?;
    let frames = frames.into_iter().map(|f| {
        let delay = f.delay();
        let buffer = f.into_buffer();
        let buffer = match rotation {
            Rotation::R0 => buffer,
            Rotation::R90 => imageops::rotate90(&buffer),
            Rotation::R180 => imageops::rotate180(&buffer),
            Rotation::R270 => imageops::rotate270(&buffer),
        };
        Frame::from_parts(buffer, 0, 0, delay)
    });
    let mut out = Vec::new();
    {
        let mut encoder = GifEncoder::new_with_speed(&mut out, 10);
        encoder.set_repeat(Repeat::Infinite).map_err(|e| Error::SaveFailed(e.to_string()))?;
        encoder.encode_frames(frames).map_err(|e| Error::SaveFailed(e.to_string()))?;
    }
    Ok(out)
}

fn encode_jpeg(image: DynamicImage) -> Result<RenderedImage> {
    let (width, height) = (image.width(), image.height());
    let mut bytes = Cursor::new(Vec::new());
    let encoder = JpegEncoder::new_with_quality(&mut bytes, 90);
    image.into_rgb8().write_with_encoder(encoder).map_err(|e| Error::BrokenImage(e.to_string()))?;
    Ok(RenderedImage { bytes: bytes.into_inner(), mime: "image/jpeg", width, height })
}
