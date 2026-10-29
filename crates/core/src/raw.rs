//! Photos brutes d'appareil photo (CR2, DNG) : on montre l'aperçu JPEG que l'appareil range
//! dans le fichier (pleine taille sur la plupart des appareils). Le « développement » complet
//! des données brutes n'est pas fait : c'est le travail d'un logiciel de retouche.

use crate::tiffdir::{self, Tiff};

/// Un fichier TIFF qui porte la balise DNGVersion : une photo brute DNG.
pub(crate) fn is_dng(bytes: &[u8]) -> bool {
    Tiff::new(bytes).is_some_and(|t| t.main_dirs().first().is_some_and(|d| d.get(tiffdir::DNG_VERSION).is_some()))
}

pub(crate) struct Preview<'a> {
    pub jpeg: &'a [u8],
    /// Orientation enregistrée par l'appareil (1 = normale), à appliquer à l'aperçu.
    pub orientation: u16,
}

/// Le plus grand aperçu JPEG rangé dans la photo brute.
pub(crate) fn preview(bytes: &[u8]) -> Option<Preview<'_>> {
    let tiff = Tiff::new(bytes)?;
    let dirs = tiff.all_dirs();
    let orientation = dirs.first().and_then(|d| d.first(tiffdir::ORIENTATION)).unwrap_or(1) as u16;
    let mut best: Option<(u64, &[u8])> = None;
    for dir in &dirs {
        let mut candidates = Vec::new();
        if let (Some(at), Some(len)) = (dir.first(tiffdir::JPEG_OFFSET), dir.first(tiffdir::JPEG_LENGTH)) {
            candidates.push((at, len));
        }
        // Une seule bande compressée en JPEG (6 : ancien JPEG, 7 : JPEG) : souvent l'aperçu.
        if matches!(dir.first(tiffdir::COMPRESSION), Some(6 | 7)) {
            if let (Some([at]), Some([len])) = (dir.get(tiffdir::STRIP_OFFSETS), dir.get(tiffdir::STRIP_BYTE_COUNTS)) {
                candidates.push((*at, *len));
            }
        }
        for (at, len) in candidates {
            let Some(jpeg) = bytes.get(at as usize..(at as usize).saturating_add(len as usize)) else { continue };
            if let Some((w, h)) = jpeg_size(jpeg) {
                let area = u64::from(w) * u64::from(h);
                if best.is_none_or(|(a, _)| area > a) {
                    best = Some((area, jpeg));
                }
            }
        }
    }
    best.map(|(_, jpeg)| Preview { jpeg, orientation })
}

/// La taille d'un JPEG ordinaire (ni sans perte, ni arithmétique : ceux-là sont les données
/// brutes du capteur, pas un aperçu), lue dans son en-tête.
fn jpeg_size(jpeg: &[u8]) -> Option<(u16, u16)> {
    if !jpeg.starts_with(&[0xFF, 0xD8]) {
        return None;
    }
    let mut at = 2;
    loop {
        if *jpeg.get(at)? != 0xFF {
            return None;
        }
        let marker = *jpeg.get(at + 1)?;
        if marker == 0xFF {
            at += 1;
            continue;
        }
        let len = u16::from_be_bytes([*jpeg.get(at + 2)?, *jpeg.get(at + 3)?]) as usize;
        match marker {
            // Débuts d'image : de base, étendu, progressif.
            0xC0..=0xC2 => {
                let h = u16::from_be_bytes([*jpeg.get(at + 5)?, *jpeg.get(at + 6)?]);
                let w = u16::from_be_bytes([*jpeg.get(at + 7)?, *jpeg.get(at + 8)?]);
                return (w > 0 && h > 0).then_some((w, h));
            }
            0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF | 0xD9 | 0xDA => return None,
            _ => at += 2 + len,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tiffdir::tests::Builder;
    use std::io::Cursor;

    pub(crate) fn jpeg(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbImage::from_pixel(w, h, image::Rgb([200, 100, 50]));
        let mut out = Vec::new();
        img.write_to(&mut Cursor::new(&mut out), image::ImageFormat::Jpeg).unwrap();
        out
    }

    #[test]
    fn lit_la_taille_d_un_jpeg() {
        assert_eq!(jpeg_size(&jpeg(30, 20)), Some((30, 20)));
        assert_eq!(jpeg_size(b"\xFF\xD8\xFF\xC3\0\x0B\x08\0\x10\0\x10\x01"), None, "JPEG sans perte : données brutes");
        assert_eq!(jpeg_size(b"pas un jpeg"), None);
        assert_eq!(jpeg_size(b"\xFF\xD8\xFF\xE0\xFF"), None);
    }

    #[test]
    fn trouve_le_plus_grand_apercu() {
        let mut b = Builder::new();
        let small = jpeg(16, 12);
        let big = jpeg(64, 48);
        let s = b.data(&small);
        let g = b.data(&big);
        // Comme un DNG : une vignette dans le répertoire principal, l'aperçu dans un sous-répertoire.
        let sub = b.dir(&[(tiffdir::COMPRESSION, vec![7]), (tiffdir::STRIP_OFFSETS, vec![g]), (tiffdir::STRIP_BYTE_COUNTS, vec![big.len() as u32])], 0);
        let main = b.dir(
            &[
                (tiffdir::JPEG_OFFSET, vec![s]),
                (tiffdir::JPEG_LENGTH, vec![small.len() as u32]),
                (tiffdir::ORIENTATION, vec![6]),
                (tiffdir::SUB_IFDS, vec![sub]),
                (tiffdir::DNG_VERSION, vec![0x0104_0000]),
            ],
            0,
        );
        b.first(main);
        assert!(is_dng(&b.bytes));
        let p = preview(&b.bytes).unwrap();
        assert_eq!(p.jpeg, &big[..]);
        assert_eq!(p.orientation, 6);
    }

    #[test]
    fn sans_apercu() {
        let mut b = Builder::new();
        let d = b.dir(&[(0x100, vec![10])], 0);
        b.first(d);
        assert!(!is_dng(&b.bytes));
        assert!(preview(&b.bytes).is_none());
        assert!(preview(b"pas un tiff").is_none());
    }
}
