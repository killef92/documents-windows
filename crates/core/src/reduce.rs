//! Réduire la taille d'un PDF (pour l'envoyer par mail) : les photos sont réduites et
//! recompressées en JPEG, le reste est compressé et rangé plus serré. Le texte et les dessins
//! restent nets : seules les images changent.

use std::collections::BTreeSet;
use std::io::Cursor;

use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, GrayImage, RgbImage};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream};
use serde::Deserialize;

/// Jusqu'où réduire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    /// Photos encore belles à l'écran et à l'impression (environ 150 ppp sur A4).
    Moyenne,
    /// Le plus petit possible, pour la lecture à l'écran.
    Forte,
}

impl Level {
    /// Plus grand côté d'une image, en pixels, et qualité JPEG.
    fn params(self) -> (u32, u8) {
        match self {
            Level::Moyenne => (1800, 75),
            Level::Forte => (1100, 55),
        }
    }
}

/// Plafond de décompression d'une image : protège d'un fichier piégé.
const MAX_IMAGE_BYTES: usize = 300 * 1024 * 1024;

/// Renvoie le PDF réduit ; s'il n'est pas plus petit, renvoie l'original.
pub fn reduce(bytes: &[u8], level: Level) -> Result<Vec<u8>, String> {
    let mut doc = Document::load_mem(bytes).map_err(|e| e.to_string())?;
    if doc.is_encrypted() {
        return Err("ce PDF est protégé par un mot de passe".into());
    }
    let (max_side, quality) = level.params();
    // Les masques de transparence restent tels quels (ils vont avec leur image).
    let masks: BTreeSet<ObjectId> = doc
        .objects
        .values()
        .filter_map(|o| o.as_stream().ok())
        .filter_map(|s| s.dict.get(b"SMask").and_then(Object::as_reference).ok())
        .collect();
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        if masks.contains(&id) {
            continue;
        }
        let Some(Object::Stream(stream)) = doc.objects.get(&id) else { continue };
        if !is_image(&stream.dict) {
            continue;
        }
        let Some(image) = decode(&doc, stream) else { continue };
        if let Some(smaller) = recompress(stream, image, max_side, quality) {
            doc.objects.insert(id, Object::Stream(smaller));
        }
    }
    doc.delete_zero_length_streams();
    doc.prune_objects();
    doc.compress();
    let mut out = Vec::new();
    doc.save_modern(&mut out).map_err(|e| e.to_string())?;
    Ok(if out.len() < bytes.len() { out } else { bytes.to_vec() })
}

fn is_image(d: &Dictionary) -> bool {
    d.get(b"Subtype").and_then(Object::as_name).is_ok_and(|n| n == b"Image")
}

fn filters(d: &Dictionary) -> Vec<Vec<u8>> {
    match d.get(b"Filter") {
        Ok(Object::Name(n)) => vec![n.clone()],
        Ok(Object::Array(a)) => a.iter().filter_map(|o| o.as_name().ok().map(<[u8]>::to_vec)).collect(),
        _ => vec![],
    }
}

/// Nombre de couleurs de l'image : 1 (gris) ou 3 (RGB) ; autre chose (CMJN, palette…) n'est pas touché.
fn components(doc: &Document, d: &Dictionary) -> Option<u8> {
    let cs = d.get(b"ColorSpace").ok()?;
    let cs = match cs {
        Object::Reference(r) => doc.get_object(*r).ok()?,
        o => o,
    };
    match cs {
        Object::Name(n) if n == b"DeviceRGB" => Some(3),
        Object::Name(n) if n == b"DeviceGray" => Some(1),
        Object::Array(a) if a.first().and_then(|o| o.as_name().ok()) == Some(b"ICCBased") => {
            let profile = doc.dereference(a.get(1)?).ok()?.1.as_stream().ok()?;
            match profile.dict.get(b"N").and_then(Object::as_i64).ok()? {
                1 => Some(1),
                3 => Some(3),
                _ => None,
            }
        }
        _ => None,
    }
}

/// L'image en pixels, si c'est une image simple qu'on sait refaire sans changer son aspect.
fn decode(doc: &Document, s: &Stream) -> Option<DynamicImage> {
    let d = &s.dict;
    // Masque par couleur, tableau Decode, masque en noir et blanc : on n'y touche pas.
    if d.has(b"Mask") || d.has(b"Decode") || d.get(b"ImageMask").and_then(Object::as_bool).unwrap_or(false) {
        return None;
    }
    let n = components(doc, d)?;
    let w = u32::try_from(d.get(b"Width").and_then(Object::as_i64).ok()?).ok()?;
    let h = u32::try_from(d.get(b"Height").and_then(Object::as_i64).ok()?).ok()?;
    match filters(d).as_slice() {
        [f] if f == b"DCTDecode" => {
            let img = image::load_from_memory_with_format(&s.content, image::ImageFormat::Jpeg).ok()?;
            (img.width() == w && img.height() == h).then_some(img)
        }
        [f] if f == b"FlateDecode" => {
            if d.get(b"BitsPerComponent").and_then(Object::as_i64).ok()? != 8 {
                return None;
            }
            let raw = s.decompressed_content_with_limit(MAX_IMAGE_BYTES).ok()?;
            let len = (w as usize).checked_mul(h as usize)?.checked_mul(n as usize)?;
            let raw = raw.get(..len)?.to_vec();
            Some(match n {
                1 => DynamicImage::ImageLuma8(GrayImage::from_raw(w, h, raw)?),
                _ => DynamicImage::ImageRgb8(RgbImage::from_raw(w, h, raw)?),
            })
        }
        _ => None,
    }
}

/// L'image réduite et en JPEG, seulement si elle y gagne vraiment.
fn recompress(original: &Stream, image: DynamicImage, max_side: u32, quality: u8) -> Option<Stream> {
    let (w, h) = (image.width(), image.height());
    // Petites images (logos, icônes) : ça n'en vaut pas la peine, et le JPEG les abîmerait.
    if w.max(h) < 64 {
        return None;
    }
    let image = if w.max(h) > max_side {
        image.resize(max_side, max_side, image::imageops::FilterType::Triangle)
    } else {
        image
    };
    let gray = matches!(image, DynamicImage::ImageLuma8(_));
    let mut jpeg = Cursor::new(Vec::new());
    let encoder = JpegEncoder::new_with_quality(&mut jpeg, quality);
    let ok = if gray { image.into_luma8().write_with_encoder(encoder) } else { image.into_rgb8().write_with_encoder(encoder) };
    ok.ok()?;
    let jpeg = jpeg.into_inner();
    if jpeg.len() * 10 > original.content.len() * 9 {
        return None;
    }
    let decoded = image::load_from_memory_with_format(&jpeg, image::ImageFormat::Jpeg).ok()?;
    let mut dict = original.dict.clone();
    dict.set("Filter", "DCTDecode");
    dict.set("Width", decoded.width() as i64);
    dict.set("Height", decoded.height() as i64);
    dict.set("BitsPerComponent", 8);
    dict.remove(b"DecodeParms");
    dict.remove(b"Length");
    // Une image réduite est forcément dans l'espace de couleur de base, sauf profil ICC gardé.
    if !dict.get(b"ColorSpace").is_ok_and(|c| matches!(c, Object::Array(_) | Object::Reference(_))) {
        dict.set("ColorSpace", if gray { "DeviceGray" } else { "DeviceRGB" });
    }
    let mut stream = Stream::new(dict, jpeg);
    stream.allows_compression = false;
    Some(stream)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::dictionary;

    /// Un PDF d'une page avec une grande photo « bruitée » non compressée en JPEG.
    fn pdf_with_photo(w: u32, h: u32) -> Vec<u8> {
        let mut doc = Document::with_version("1.5");
        // Dégradé avec un peu de grain, comme une photo (le grain résiste à la compression sans perte).
        let pixels: Vec<u8> = (0..w * h * 3)
            .map(|i| {
                let (x, y) = ((i / 3) % w, (i / 3) / w);
                let grain = (i.wrapping_mul(2_654_435_761) >> 27) as u8;
                ((x * 160 / w + y * 60 / h) as u8).wrapping_add(grain)
            })
            .collect();
        let mut img = Stream::new(
            dictionary! {
                "Type" => "XObject", "Subtype" => "Image", "Width" => w as i64, "Height" => h as i64,
                "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8,
            },
            pixels,
        );
        img.compress().unwrap();
        let img_id = doc.add_object(img);
        let content = doc.add_object(Stream::new(dictionary! {}, b"q 500 0 0 700 50 50 cm /Im0 Do Q".to_vec()));
        let pages_id = doc.new_object_id();
        let page = doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages_id, "MediaBox" => vec![0.into(), 0.into(), 600.into(), 800.into()],
            "Contents" => content, "Resources" => dictionary! { "XObject" => dictionary! { "Im0" => img_id } },
        });
        doc.objects.insert(pages_id, Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 }));
        let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog);
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        out
    }

    fn image_of(bytes: &[u8]) -> Dictionary {
        let doc = Document::load_mem(bytes).unwrap();
        doc.objects.values().filter_map(|o| o.as_stream().ok()).find(|s| is_image(&s.dict)).unwrap().dict.clone()
    }

    #[test]
    fn reduit_les_photos() {
        let original = pdf_with_photo(2400, 1600);
        let moyenne = reduce(&original, Level::Moyenne).unwrap();
        let forte = reduce(&original, Level::Forte).unwrap();
        assert!(moyenne.len() * 4 < original.len(), "{} → {}", original.len(), moyenne.len());
        assert!(forte.len() < moyenne.len());
        let d = image_of(&moyenne);
        assert_eq!(d.get(b"Filter").unwrap().as_name().unwrap(), b"DCTDecode");
        assert_eq!((d.get(b"Width").unwrap().as_i64().unwrap(), d.get(b"Height").unwrap().as_i64().unwrap()), (1800, 1200));
        assert_eq!(Document::load_mem(&forte).unwrap().get_pages().len(), 1);
    }

    #[test]
    fn ne_grossit_jamais_un_pdf() {
        // Une toute petite image : rien à gagner, on rend le fichier tel quel ou plus petit.
        let original = pdf_with_photo(40, 30);
        let out = reduce(&original, Level::Forte).unwrap();
        assert!(out.len() <= original.len());
        assert_eq!(image_of(&out).get(b"Width").unwrap().as_i64().unwrap(), 40);
    }
}
