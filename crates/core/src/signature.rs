//! Signature à partir d'une photo : on garde l'encre, le papier devient transparent.

use image::{imageops::FilterType, DynamicImage, GrayImage, RgbaImage};

use crate::picture::Picture;
use crate::{Error, Result};

/// Lit la photo d'une signature (sur papier clair) et renvoie un PNG transparent, recadré.
pub fn from_photo(bytes: &[u8]) -> Result<Vec<u8>> {
    let picture = Picture::open(bytes)?;
    let mut image = picture.full(crate::Rotation::R0);
    if image.width().max(image.height()) > 1600 {
        image = image.resize(1600, 1600, FilterType::Triangle);
    }
    let rgba = extract_ink(&image).ok_or_else(|| Error::BrokenImage("aucune signature trouvée sur cette image".into()))?;
    let mut out = Vec::new();
    DynamicImage::ImageRgba8(rgba)
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .map_err(|e| Error::BrokenImage(e.to_string()))?;
    Ok(out)
}

/// Le même PNG, prêt à afficher et à garder : « data:image/png;base64,… ».
pub fn to_data_url(png: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::from("data:image/png;base64,");
    for chunk in png.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let v = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for i in 0..4 {
            out.push(if i <= chunk.len() { T[(v >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
    }
    out
}

/// Seuil entre l'encre et le papier : bien en dessous de la teinte du papier (la valeur
/// la plus fréquente), en tenant compte de son grain.
fn ink_threshold(gray: &GrayImage) -> f32 {
    let mut hist = [0u64; 256];
    for p in gray.pixels() {
        hist[p[0] as usize] += 1;
    }
    let total: u64 = hist.iter().sum();
    let percentile = |q: f64| {
        let mut acc = 0;
        for (v, &c) in hist.iter().enumerate() {
            acc += c;
            if acc as f64 >= q * total as f64 {
                return v as f32;
            }
        }
        255.0
    };
    let paper = percentile(0.5);
    let grain = paper - percentile(0.1);
    paper - (grain * 3.0).max(40.0)
}

fn extract_ink(image: &DynamicImage) -> Option<RgbaImage> {
    let gray = image.to_luma8();
    let rgb = image.to_rgb8();
    let t = ink_threshold(&gray);
    // Moyenne de l'encre : pour des bords doux plutôt qu'un découpage en escalier.
    let (mut n, mut lum, mut color) = (0u64, 0.0, [0.0f64; 3]);
    for (g, c) in gray.pixels().zip(rgb.pixels()) {
        if (g[0] as f32) < t {
            n += 1;
            lum += g[0] as f64;
            for k in 0..3 {
                color[k] += c[k] as f64;
            }
        }
    }
    let total = (gray.width() * gray.height()) as u64;
    // Pas d'encre, ou presque tout est « encre » : ce n'est pas une signature sur papier.
    if n == 0 || n * 2 > total {
        return None;
    }
    let ink_lum = (lum / n as f64) as f32;
    let span = (t - ink_lum).max(8.0);
    // Couleur de l'encre (bleue, noire…), un peu foncée pour rester lisible.
    let ink = color.map(|c| ((c / n as f64) * 0.8) as u8);
    let (w, h) = gray.dimensions();
    let mut out = RgbaImage::new(w, h);
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for (x, y, g) in gray.enumerate_pixels() {
        let a = ((t - g[0] as f32) / span).clamp(0.0, 1.0);
        let a = (a * 255.0) as u8;
        out.put_pixel(x, y, image::Rgba([ink[0], ink[1], ink[2], a]));
        if a > 60 {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
    }
    if x1 < x0 || y1 < y0 {
        return None;
    }
    let margin = ((x1 - x0).max(y1 - y0) / 40).max(2);
    let (x0, y0) = (x0.saturating_sub(margin), y0.saturating_sub(margin));
    let (x1, y1) = ((x1 + margin).min(w - 1), (y1 + margin).min(h - 1));
    let cropped = image::imageops::crop_imm(&out, x0, y0, x1 - x0 + 1, y1 - y0 + 1).to_image();
    Some(if cropped.width() > 1000 {
        DynamicImage::ImageRgba8(cropped).resize(1000, 1000, FilterType::Triangle).into_rgba8()
    } else {
        cropped
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn photo_of_signature() -> Vec<u8> {
        // Papier gris clair un peu irrégulier, et un trait bleu foncé au milieu.
        let mut img = image::RgbImage::from_fn(400, 200, |x, y| {
            let v = 225 + ((x * 7 + y * 13) % 20) as u8;
            image::Rgb([v, v, v - 5])
        });
        for x in 100..300 {
            for dy in 0..6 {
                let y = 100 + ((x as f32 / 20.0).sin() * 20.0) as u32 + dy;
                img.put_pixel(x, y, image::Rgb([20, 30, 90]));
            }
        }
        let mut bytes = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png).unwrap();
        bytes
    }

    #[test]
    fn garde_l_encre_et_rend_le_papier_transparent() {
        let png = from_photo(&photo_of_signature()).unwrap();
        let out = image::load_from_memory(&png).unwrap().into_rgba8();
        // Recadrée autour du trait (200 px de large + marges).
        assert!(out.width() >= 200 && out.width() < 230, "{}", out.width());
        assert!(out.height() < 80, "{}", out.height());
        // Coin : papier, donc transparent ; quelque part, de l'encre bleue opaque.
        assert_eq!(out.get_pixel(0, 0)[3], 0);
        assert!(out.pixels().any(|p| p[3] > 200 && p[2] > p[0]));
    }

    #[test]
    fn encode_en_base64() {
        assert_eq!(to_data_url("Documents".as_bytes()), "data:image/png;base64,RG9jdW1lbnRz");
        assert_eq!(to_data_url(b"Ma"), "data:image/png;base64,TWE=");
    }

    #[test]
    fn refuse_une_image_sans_signature() {
        let img = image::RgbImage::from_pixel(50, 50, image::Rgb([240, 240, 240]));
        let mut bytes = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png).unwrap();
        assert!(from_photo(&bytes).is_err());
    }
}
