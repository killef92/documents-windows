//! Lecture des photos et images (JPEG, PNG, WebP, GIF, BMP, TIFF, HEIC, JPEG 2000…).

use image::{DynamicImage, ImageDecoder, ImageReader, Rgb32FImage, Rgba32FImage};
use serde::Deserialize;
use std::io::Cursor;
use std::sync::Once;

use crate::document::{PageSize, Rotation};
use crate::tiffdir::{self, Tiff};
use crate::{heif, raw, DocumentKind, Error, Result};

/// Au-delà (2 Go une fois décodée, ou 250 millions de pixels), une image est refusée.
const MAX_DECODED_BYTES: u64 = 2 << 30;
const MAX_PIXELS: u64 = 250_000_000;
/// Le JPEG 2000 se décode lentement : limite plus basse, pour ne pas bloquer longtemps.
const MAX_JP2_PIXELS: u64 = 80_000_000;

pub(crate) struct Picture {
    image: DynamicImage,
    /// Le fichier d'origine, pour l'enregistrer sans perte quand c'est possible.
    pub bytes: Vec<u8>,
    /// Orientation EXIF lue dans le fichier (1 = normale).
    pub orientation: u16,
}

impl Picture {
    /// Décode l'image et la redresse selon l'orientation enregistrée par l'appareil photo.
    pub fn open(bytes: &[u8]) -> Result<Self> {
        let broken = |e: image::ImageError| Error::BrokenImage(e.to_string());
        let mut decoder = ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()?
            .into_decoder()
            .map_err(broken)?;
        // Taille annoncée absurde (fichier abîmé ou piégé) : refusée avant de réserver la mémoire.
        let (w, h) = decoder.dimensions();
        if decoder.total_bytes() > MAX_DECODED_BYTES || u64::from(w) * u64::from(h) > MAX_PIXELS {
            return Err(Error::BrokenImage("image too large".into()));
        }
        let orientation = decoder.orientation().map_err(broken)?;
        let mut image = DynamicImage::from_decoder(decoder).map_err(broken)?;
        image.apply_orientation(orientation);
        Ok(Self { image, bytes: bytes.to_vec(), orientation: orientation.to_exif() as u16 })
    }

    /// Lit une image de n'importe quel type connu (la première page pour un TIFF).
    pub fn load(kind: DocumentKind, bytes: &[u8]) -> Result<Self> {
        match kind {
            DocumentKind::Heic => Ok(Self { image: heif::decode(bytes)?, bytes: bytes.to_vec(), orientation: 1 }),
            DocumentKind::Raw => {
                let preview = raw::preview(bytes).ok_or(Error::RawWithoutPreview)?;
                let mut picture = Self::open(preview.jpeg)?;
                let orientation = image::metadata::Orientation::from_exif(preview.orientation as u8)
                    .unwrap_or(image::metadata::Orientation::NoTransforms);
                picture.image.apply_orientation(orientation);
                picture.bytes = bytes.to_vec();
                Ok(picture)
            }
            DocumentKind::Tiff => {
                let mut picture = Self::open(bytes)?;
                if let Some(tiff) = Tiff::new(bytes) {
                    if let Some(dir) = tiff.main_dirs().first() {
                        picture.image = square_pixels(std::mem::take(&mut picture.image), dir, picture.orientation);
                    }
                }
                Ok(picture)
            }
            DocumentKind::Jp2 => {
                static HOOK: Once = Once::new();
                HOOK.call_once(|| {
                    hayro_jpeg2000::integration::register_decoding_hook();
                });
                let header = hayro_jpeg2000::Image::new(bytes, &hayro_jpeg2000::DecodeSettings::default())
                    .map_err(|e| Error::BrokenImage(format!("{e:?}")))?;
                if u64::from(header.width()) * u64::from(header.height()) > MAX_JP2_PIXELS {
                    return Err(Error::BrokenImage("image too large".into()));
                }
                Self::open(bytes)
            }
            DocumentKind::Hdr | DocumentKind::Exr => {
                let mut picture = Self::open(bytes)?;
                picture.image = tone_map(std::mem::take(&mut picture.image));
                Ok(picture)
            }
            _ => Self::open(bytes),
        }
    }

    /// Les pages d'un TIFF qui en a plusieurs (scan, fax), dans l'ordre ; une seule page : vide.
    pub fn tiff_pages(bytes: &[u8]) -> Result<Vec<DynamicImage>> {
        let Some(tiff) = Tiff::new(bytes) else { return Ok(Vec::new()) };
        // Les images « réduites » (vignettes) ne sont pas des pages.
        let dirs: Vec<_> = tiff.main_dirs().into_iter().filter(|d| d.first(tiffdir::NEW_SUBFILE_TYPE).unwrap_or(0) & 1 == 0).collect();
        if dirs.len() < 2 {
            return Ok(Vec::new());
        }
        dirs.iter()
            .map(|dir| {
                let picture = Self::open(&tiff.with_first_dir(dir.offset))?;
                Ok(square_pixels(picture.image, dir, picture.orientation))
            })
            .collect()
    }

    /// Une copie retouchée : recadrée, puis luminosité, contraste et saturation.
    pub fn retouched(&self, r: &Retouch) -> Result<Self> {
        let mut image = match r.crop {
            Some(c) => {
                let (w, h) = (self.image.width(), self.image.height());
                let x = (c.x.max(0.0).round() as u32).min(w.saturating_sub(1));
                let y = (c.y.max(0.0).round() as u32).min(h.saturating_sub(1));
                let cw = (c.w.round() as u32).clamp(1, w - x);
                let ch = (c.h.round() as u32).clamp(1, h - y);
                self.image.crop_imm(x, y, cw, ch)
            }
            None => self.image.clone(),
        };
        if r.brightness != 0.0 || r.contrast != 0.0 || r.saturation != 0.0 {
            image = adjust(image, r);
        }
        if r.flip_h {
            image = image.fliph();
        }
        if r.flip_v {
            image = image.flipv();
        }
        if let Some((w, h)) = r.resize {
            let (w, h) = (w.clamp(1, 30_000), h.clamp(1, 30_000));
            if (w, h) != (image.width(), image.height()) {
                image = image.resize_exact(w, h, image::imageops::FilterType::Lanczos3);
            }
        }
        Ok(Self { image, bytes: Vec::new(), orientation: 1 })
    }

    pub fn size(&self) -> PageSize {
        PageSize { width: self.image.width() as f32, height: self.image.height() as f32 }
    }

    pub fn has_alpha(&self) -> bool {
        self.image.color().has_alpha()
    }

    /// L'image entière, tournée.
    pub fn full(&self, rotation: Rotation) -> DynamicImage {
        self.render(u32::MAX, rotation)
    }

    /// Réduit l'image à la largeur voulue (jamais agrandie), après rotation.
    pub fn render(&self, width: u32, rotation: Rotation) -> DynamicImage {
        let (w, h) = (self.image.width(), self.image.height());
        let (rw, rh) = if rotation.is_sideways() { (h, w) } else { (w, h) };
        let width = width.clamp(1, rw);
        let height = ((width as f64) * rh as f64 / rw as f64).round().max(1.0) as u32;
        // On réduit avant de tourner : c'est beaucoup plus rapide sur une grosse photo.
        let (tw, th) = if rotation.is_sideways() { (height, width) } else { (width, height) };
        let small = if tw == w {
            self.image.clone()
        } else if tw * 2 < w {
            self.image.thumbnail_exact(tw, th)
        } else {
            self.image.resize_exact(tw, th, image::imageops::FilterType::Triangle)
        };
        match rotation {
            Rotation::R0 => small,
            Rotation::R90 => small.rotate90(),
            Rotation::R180 => small.rotate180(),
            Rotation::R270 => small.rotate270(),
        }
    }
}

/// Un fax « normal » a des pixels deux fois plus hauts que larges (204 × 98 points par pouce) :
/// on étire l'image pour qu'elle ne paraisse pas écrasée.
fn square_pixels(image: DynamicImage, dir: &tiffdir::Dir, orientation: u16) -> DynamicImage {
    let (Some(x), Some(y)) = (dir.first(tiffdir::X_RESOLUTION), dir.first(tiffdir::Y_RESOLUTION)) else { return image };
    if x == 0 || y == 0 || (x as f32 / y as f32 - 1.0).abs() < 0.1 {
        return image;
    }
    // Résolutions données avant redressement : couchée, l'image a ses axes échangés.
    let (x, y) = if orientation >= 5 { (y, x) } else { (x, y) };
    let (w, h) = (image.width(), image.height());
    let (nw, nh) = if x > y {
        (w, ((h as f64) * x as f64 / y as f64).round() as u32)
    } else {
        (((w as f64) * y as f64 / x as f64).round() as u32, h)
    };
    if nw.max(nh) > 30_000 {
        return image;
    }
    image.resize_exact(nw, nh, image::imageops::FilterType::Triangle)
}

/// Une image « haute dynamique » (HDR, OpenEXR) contient plus de lumière qu'un écran n'en
/// montre : on compresse les hautes lumières (Reinhard étendu) puis on passe en sRGB.
fn tone_map(image: DynamicImage) -> DynamicImage {
    let has_alpha = image.color().has_alpha();
    let mut rgba: Rgba32FImage = match image {
        DynamicImage::ImageRgb32F(_) | DynamicImage::ImageRgba32F(_) => image.into_rgba32f(),
        // Déjà en 8 ou 16 bits : rien à faire.
        other => return other,
    };
    let lum = |p: &image::Rgba<f32>| (0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2]).max(0.0);
    let mut sorted: Vec<f32> = rgba.pixels().map(lum).filter(|l| l.is_finite()).collect();
    let white = if sorted.is_empty() {
        1.0
    } else {
        let k = (sorted.len() as f64 * 0.995) as usize;
        let k = k.min(sorted.len() - 1);
        *sorted.select_nth_unstable_by(k, f32::total_cmp).1
    };
    let srgb = |v: f32| {
        let v = v.clamp(0.0, 1.0);
        if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 }
    };
    if white > 1.0 {
        let w2 = white * white;
        for p in rgba.pixels_mut() {
            let l = lum(p);
            if l > 0.0 && l.is_finite() {
                let k = (1.0 + l / w2) / (1.0 + l);
                for c in 0..3 {
                    p[c] *= k;
                }
            }
        }
    }
    for p in rgba.pixels_mut() {
        for c in 0..3 {
            p[c] = if p[c].is_finite() { srgb(p[c]) } else { 1.0 };
        }
        p[3] = if p[3].is_finite() { p[3].clamp(0.0, 1.0) } else { 1.0 };
    }
    let image = DynamicImage::ImageRgba32F(rgba);
    if has_alpha {
        DynamicImage::ImageRgba8(image.to_rgba8())
    } else {
        let rgb: Rgb32FImage = image.into_rgb32f();
        DynamicImage::ImageRgb8(DynamicImage::ImageRgb32F(rgb).to_rgb8())
    }
}

/// Retouche d'une photo. Les réglages vont de -1 à 1 (0 = inchangé) et suivent exactement les
/// filtres CSS brightness(), contrast() et saturate() utilisés pour l'aperçu à l'écran.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
pub struct Retouch {
    /// Zone gardée, en pixels de la photo redressée (avant la rotation choisie dans l'appli).
    pub crop: Option<CropRect>,
    #[serde(default)]
    pub brightness: f32,
    #[serde(default)]
    pub contrast: f32,
    #[serde(default)]
    pub saturation: f32,
    /// Retourner comme dans un miroir (gauche ↔ droite, haut ↔ bas).
    #[serde(default)]
    pub flip_h: bool,
    #[serde(default)]
    pub flip_v: bool,
    /// Nouvelle taille en pixels (« Ajuster la taille »), après recadrage.
    #[serde(default)]
    pub resize: Option<(u32, u32)>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct CropRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

fn adjust(image: DynamicImage, r: &Retouch) -> DynamicImage {
    let b = 1.0 + r.brightness;
    let c = 1.0 + r.contrast;
    let s = 1.0 + r.saturation;
    // Matrice de saturate() (Filter Effects, feColorMatrix type="saturate").
    let m = [
        [0.213 + 0.787 * s, 0.715 - 0.715 * s, 0.072 - 0.072 * s],
        [0.213 - 0.213 * s, 0.715 + 0.285 * s, 0.072 - 0.072 * s],
        [0.213 - 0.213 * s, 0.715 - 0.715 * s, 0.072 + 0.928 * s],
    ];
    // Table pour luminosité puis contraste (chaque filtre CSS borne son résultat).
    let lut: Vec<f32> = (0..256)
        .map(|v| {
            let v = (v as f32 / 255.0 * b).clamp(0.0, 1.0);
            ((v - 0.5) * c + 0.5).clamp(0.0, 1.0)
        })
        .collect();
    let to_u8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    let has_alpha = image.color().has_alpha();
    let mut rgba = image.into_rgba8();
    for p in rgba.pixels_mut() {
        let v = [lut[p[0] as usize], lut[p[1] as usize], lut[p[2] as usize]];
        for (k, row) in m.iter().enumerate() {
            p[k] = to_u8(row[0] * v[0] + row[1] * v[1] + row[2] * v[2]);
        }
    }
    if has_alpha {
        DynamicImage::ImageRgba8(rgba)
    } else {
        DynamicImage::ImageRgb8(DynamicImage::ImageRgba8(rgba).into_rgb8())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn photo() -> Picture {
        let img = image::RgbImage::from_fn(40, 20, |x, _| if x < 20 { image::Rgb([200, 60, 60]) } else { image::Rgb([100, 100, 100]) });
        let mut bytes = Vec::new();
        img.write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png).unwrap();
        Picture::open(&bytes).unwrap()
    }

    fn pixel(p: &Picture, x: u32, y: u32) -> [u8; 3] {
        p.image.to_rgb8().get_pixel(x, y).0
    }

    #[test]
    fn recadre() {
        let crop = CropRect { x: 15.0, y: 5.0, w: 10.0, h: 8.0 };
        let p = photo().retouched(&Retouch { crop: Some(crop), ..Default::default() }).unwrap();
        assert_eq!((p.size().width, p.size().height), (10.0, 8.0));
        assert_eq!(pixel(&p, 0, 0), [200, 60, 60]);
        assert_eq!(pixel(&p, 9, 7), [100, 100, 100]);
        // Une zone qui dépasse est ramenée dans la photo.
        let big = CropRect { x: 30.0, y: -5.0, w: 100.0, h: 100.0 };
        let p = photo().retouched(&Retouch { crop: Some(big), ..Default::default() }).unwrap();
        assert_eq!((p.size().width, p.size().height), (10.0, 20.0));
    }

    #[test]
    fn retourne_et_change_la_taille() {
        let p = photo().retouched(&Retouch { flip_h: true, ..Default::default() }).unwrap();
        assert_eq!(pixel(&p, 39, 0), pixel(&photo(), 0, 0));
        let p = photo().retouched(&Retouch { flip_v: true, ..Default::default() }).unwrap();
        assert_eq!(pixel(&p, 0, 19), pixel(&photo(), 0, 0));
        let p = photo().retouched(&Retouch { resize: Some((20, 10)), ..Default::default() }).unwrap();
        assert_eq!((p.size().width, p.size().height), (20.0, 10.0));
    }

    #[test]
    fn regle_comme_les_filtres_css() {
        let r = |brightness, contrast, saturation| Retouch { brightness, contrast, saturation, ..Default::default() };
        // brightness(1.5) : 100 → 150.
        assert_eq!(pixel(&photo().retouched(&r(0.5, 0.0, 0.0)).unwrap(), 30, 0), [150, 150, 150]);
        // contrast(2) : 100 → (0.392 - 0.5) × 2 + 0.5 = 0.2843 → 72.5, arrondi à 73.
        assert_eq!(pixel(&photo().retouched(&r(0.0, 1.0, 0.0)).unwrap(), 30, 0), [73, 73, 73]);
        // saturate(0) : noir et blanc (0,213 × 200 + 0,787 × 60 = 90) ; le gris ne change pas.
        let bw = photo().retouched(&r(0.0, 0.0, -1.0)).unwrap();
        let [a, b, c] = pixel(&bw, 0, 0);
        assert!(a == b && b == c && a == 90, "{a} {b} {c}");
        assert_eq!(pixel(&bw, 30, 0), [100, 100, 100]);
        // Sans réglage : identique.
        assert_eq!(pixel(&photo().retouched(&r(0.0, 0.0, 0.0)).unwrap(), 0, 0), [200, 60, 60]);
    }
}
