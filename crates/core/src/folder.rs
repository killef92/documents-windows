//! Les autres images du dossier : la bande de vignettes sous une image ouverte seule.
//!
//! On ne liste que les noms (rapide, même pour des milliers de photos) ; chaque vignette
//! est fabriquée à la demande, quand elle devient visible.

use std::cmp::Ordering;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use image::codecs::jpeg::JpegEncoder;
use image::ImageFormat;

use crate::document::Rotation;
use crate::picture::Picture;
use crate::Result;

/// Les extensions montrées dans la bande (celles qu'on sait ouvrir).
const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif"];

pub fn is_image_name(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| IMAGE_EXTENSIONS.iter().any(|x| x.eq_ignore_ascii_case(e)))
}

/// Les images du dossier, dans l'ordre de l'Explorateur (« photo 2 » avant « photo 10 »).
pub fn images_in(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut list: Vec<PathBuf> = std::fs::read_dir(dir)?
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.path())
        .filter(|p| is_image_name(p))
        .collect();
    list.sort_by(|a, b| natural_cmp(&name_of(a), &name_of(b)));
    Ok(list)
}

fn name_of(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default()
}

/// Compare deux noms comme l'Explorateur : les nombres par leur valeur, le reste sans casse.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let take = |it: &mut std::iter::Peekable<std::str::Chars>| {
                    let mut s = String::new();
                    while let Some(c) = it.peek().copied().filter(char::is_ascii_digit) {
                        s.push(c);
                        it.next();
                    }
                    s
                };
                let (na, nb) = (take(&mut a), take(&mut b));
                let (ta, tb) = (na.trim_start_matches('0'), nb.trim_start_matches('0'));
                let o = ta.len().cmp(&tb.len()).then_with(|| ta.cmp(tb)).then_with(|| na.len().cmp(&nb.len()));
                if o != Ordering::Equal {
                    return o;
                }
            }
            (Some(x), Some(y)) => {
                let o = x.to_lowercase().cmp(y.to_lowercase());
                if o != Ordering::Equal {
                    return o;
                }
                a.next();
                b.next();
            }
        }
    }
}

/// Petite image (au plus `max` pixels de côté) d'une photo, redressée : JPEG, ou PNG si
/// elle a de la transparence.
pub fn thumbnail(path: &Path, max: u32) -> Result<(Vec<u8>, &'static str)> {
    let bytes = std::fs::read(path)?;
    let picture = Picture::open(&bytes)?;
    let size = picture.size();
    let width = if size.width >= size.height {
        max
    } else {
        ((max as f32) * size.width / size.height).round().max(1.0) as u32
    };
    let small = picture.render(width, Rotation::R0);
    let mut out = Cursor::new(Vec::new());
    if picture.has_alpha() {
        small.write_to(&mut out, ImageFormat::Png).map_err(|e| crate::Error::BrokenImage(e.to_string()))?;
        return Ok((out.into_inner(), "image/png"));
    }
    small
        .into_rgb8()
        .write_with_encoder(JpegEncoder::new_with_quality(&mut out, 80))
        .map_err(|e| crate::Error::BrokenImage(e.to_string()))?;
    Ok((out.into_inner(), "image/jpeg"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixtures() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
    }

    #[test]
    fn ordre_de_l_explorateur() {
        let mut names = vec!["photo 10.jpg", "Photo 2.jpg", "photo 1.jpg", "album.png", "photo 02b.jpg"];
        names.sort_by(|a, b| natural_cmp(&a.to_lowercase(), &b.to_lowercase()));
        assert_eq!(names, ["album.png", "photo 1.jpg", "Photo 2.jpg", "photo 02b.jpg", "photo 10.jpg"]);
    }

    #[test]
    fn liste_les_images_du_dossier() {
        let list = images_in(&fixtures()).unwrap();
        let names: Vec<_> = list.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["anime.gif", "photo-pivotee.jpg", "photo.png", "photo.webp", "pixel.png"]);
    }

    #[test]
    fn fabrique_une_vignette() {
        let (bytes, mime) = thumbnail(&fixtures().join("photo-pivotee.jpg"), 160).unwrap();
        assert_eq!(mime, "image/jpeg");
        let img = image::load_from_memory(&bytes).unwrap();
        assert!(img.width().max(img.height()) <= 160);
        // La photo est enregistrée couchée avec une orientation EXIF : la vignette est redressée.
        let original = Picture::open(&std::fs::read(fixtures().join("photo-pivotee.jpg")).unwrap()).unwrap().size();
        assert_eq!(img.width() > img.height(), original.width > original.height);
        assert!(thumbnail(&fixtures().join("pas-un-document.txt"), 160).is_err());
    }
}
