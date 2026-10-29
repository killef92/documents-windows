use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

/// Les types de fichiers que l'appli sait (ou saura) ouvrir.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DocumentKind {
    Pdf,
    Jpeg,
    Png,
    Heic,
    Webp,
    Gif,
    Bmp,
    Ico,
    /// PPM, PGM, PBM, PAM.
    Pnm,
    Tiff,
    /// Image « haute dynamique » Radiance.
    Hdr,
    Exr,
    /// JPEG 2000.
    Jp2,
    /// Photo brute d'appareil photo (CR2, DNG) : on montre l'aperçu JPEG rangé dedans.
    Raw,
    /// Dessin vectoriel .ai : un PDF à l'intérieur.
    Ai,
}

/// Toutes les extensions que l'appli sait ouvrir (en minuscules).
pub const EXTENSIONS: &[&str] = &[
    "pdf", "ai", "jpg", "jpeg", "jpe", "jfif", "png", "webp", "gif", "heic", "heif", "bmp", "dib", "ico", "ppm", "pgm",
    "pbm", "pnm", "pam", "tif", "tiff", "hdr", "exr", "jp2", "j2k", "j2c", "jpf", "jpx", "cr2", "dng",
];

/// Extensions des photos brutes d'appareil photo (des TIFF un peu particuliers).
const RAW_EXTENSIONS: &[&str] = &["cr2", "dng"];

fn extension_of(path: &Path) -> String {
    path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

/// Un fichier que l'appli sait ouvrir, d'après son extension.
pub fn is_openable_name(path: &Path) -> bool {
    EXTENSIONS.contains(&extension_of(path).as_str())
}

/// Une image (pas un PDF ni un dessin .ai), d'après son extension.
pub fn is_picture_name(path: &Path) -> bool {
    let ext = extension_of(path);
    EXTENSIONS.contains(&ext.as_str()) && !matches!(ext.as_str(), "pdf" | "ai")
}

impl DocumentKind {
    /// Reconnaît le type d'un fichier d'après ses premiers octets
    /// (plus fiable que l'extension, qui peut mentir).
    pub fn from_bytes(head: &[u8]) -> Option<Self> {
        if head.starts_with(b"%PDF-") {
            Some(Self::Pdf)
        } else if head.starts_with(&[0xFF, 0xD8, 0xFF]) {
            Some(Self::Jpeg)
        } else if head.starts_with(b"\x89PNG\r\n\x1a\n") {
            Some(Self::Png)
        } else if head.len() >= 12 && &head[0..4] == b"RIFF" && &head[8..12] == b"WEBP" {
            Some(Self::Webp)
        } else if head.starts_with(b"GIF87a") || head.starts_with(b"GIF89a") {
            Some(Self::Gif)
        } else if head.len() >= 12
            && &head[4..8] == b"ftyp"
            && matches!(&head[8..12], b"heic" | b"heix" | b"mif1" | b"msf1" | b"heim" | b"heis" | b"hevc" | b"hevx")
        {
            Some(Self::Heic)
        } else if head.len() >= 10 && head.starts_with(b"II*\0") && &head[8..10] == b"CR" {
            Some(Self::Raw)
        } else if [b"II*\0", b"MM\0*", b"II+\0", b"MM\0+"].iter().any(|m| head.starts_with(*m)) {
            Some(Self::Tiff)
        } else if head.starts_with(b"\0\0\0\x0CjP  \r\n\x87\n") || head.starts_with(&[0xFF, 0x4F, 0xFF, 0x51]) {
            Some(Self::Jp2)
        } else if head.starts_with(&[0x76, 0x2F, 0x31, 0x01]) {
            Some(Self::Exr)
        } else if head.starts_with(b"#?RADIANCE") || head.starts_with(b"#?RGBE") {
            Some(Self::Hdr)
        } else if head.len() >= 18 && head.starts_with(b"BM") && matches!(head[14], 12 | 40 | 52 | 56 | 64 | 108 | 124) && head[15..18] == [0, 0, 0] {
            Some(Self::Bmp)
        } else if head.len() >= 6 && head.starts_with(&[0, 0, 1, 0]) && head[4..6] != [0, 0] {
            Some(Self::Ico)
        } else if head.len() >= 3 && head[0] == b'P' && (b'1'..=b'7').contains(&head[1]) && head[2].is_ascii_whitespace() {
            Some(Self::Pnm)
        } else {
            None
        }
    }

    /// Le format qu'annonce l'extension d'un fichier, pour ceux qu'on sait écrire.
    pub fn from_extension(path: &Path) -> Option<Self> {
        Some(match extension_of(path).as_str() {
            "jpg" | "jpeg" | "jpe" | "jfif" => Self::Jpeg,
            "png" => Self::Png,
            "webp" => Self::Webp,
            "gif" => Self::Gif,
            "bmp" | "dib" => Self::Bmp,
            "ppm" | "pgm" | "pbm" | "pnm" | "pam" => Self::Pnm,
            "tif" | "tiff" => Self::Tiff,
            "pdf" => Self::Pdf,
            _ => return None,
        })
    }

    /// Précise le type quand les premiers octets ne suffisent pas : un .ai est un PDF, une
    /// photo brute DNG est un TIFF.
    pub fn refine(self, path: &Path, bytes: &[u8]) -> Self {
        let ext = extension_of(path);
        match self {
            Self::Pdf if ext == "ai" => Self::Ai,
            Self::Tiff if RAW_EXTENSIONS.contains(&ext.as_str()) || crate::raw::is_dng(bytes) => Self::Raw,
            other => other,
        }
    }

    /// Lit le début du fichier et en déduit le type.
    pub fn detect(path: &Path) -> io::Result<Option<Self>> {
        let mut head = Vec::with_capacity(32);
        File::open(path)?.take(32).read_to_end(&mut head)?;
        Ok(Self::from_bytes(&head))
    }

    pub fn is_picture(self) -> bool {
        !matches!(self, Self::Pdf | Self::Ai)
    }

    /// « Enregistrer » peut réécrire le fichier d'origine dans son format. Sinon (photo HEIC,
    /// brute, JPEG 2000, image HDR, icône, dessin .ai…), on enregistre une copie à côté.
    pub fn is_writable(self) -> bool {
        matches!(self, Self::Pdf | Self::Jpeg | Self::Png | Self::Webp | Self::Gif | Self::Bmp | Self::Pnm | Self::Tiff)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconnait_les_signatures() {
        assert_eq!(DocumentKind::from_bytes(b"%PDF-1.7\n"), Some(DocumentKind::Pdf));
        assert_eq!(DocumentKind::from_bytes(&[0xFF, 0xD8, 0xFF, 0xE0]), Some(DocumentKind::Jpeg));
        assert_eq!(DocumentKind::from_bytes(b"RIFF\0\0\0\0WEBPVP8 "), Some(DocumentKind::Webp));
        assert_eq!(DocumentKind::from_bytes(b"GIF89a\x01\0"), Some(DocumentKind::Gif));
        assert_eq!(DocumentKind::from_bytes(b"\0\0\0\x18ftypheic"), Some(DocumentKind::Heic));
        assert_eq!(DocumentKind::from_bytes(b"II*\0\x10\0\0\0CR\x02\0"), Some(DocumentKind::Raw));
        assert_eq!(DocumentKind::from_bytes(b"II*\0\x08\0\0\0"), Some(DocumentKind::Tiff));
        assert_eq!(DocumentKind::from_bytes(b"MM\0*\0\0\0\x08"), Some(DocumentKind::Tiff));
        assert_eq!(DocumentKind::from_bytes(b"\0\0\0\x0CjP  \r\n\x87\n"), Some(DocumentKind::Jp2));
        assert_eq!(DocumentKind::from_bytes(&[0xFF, 0x4F, 0xFF, 0x51, 0]), Some(DocumentKind::Jp2));
        assert_eq!(DocumentKind::from_bytes(&[0x76, 0x2F, 0x31, 0x01, 2, 0]), Some(DocumentKind::Exr));
        assert_eq!(DocumentKind::from_bytes(b"#?RADIANCE\n"), Some(DocumentKind::Hdr));
        assert_eq!(DocumentKind::from_bytes(b"BM\x46\0\0\0\0\0\0\0\x36\0\0\0\x28\0\0\0"), Some(DocumentKind::Bmp));
        assert_eq!(DocumentKind::from_bytes(b"BMW est une marque"), None);
        assert_eq!(DocumentKind::from_bytes(&[0, 0, 1, 0, 1, 0, 16, 16]), Some(DocumentKind::Ico));
        assert_eq!(DocumentKind::from_bytes(b"P6\n2 2\n255\n"), Some(DocumentKind::Pnm));
        assert_eq!(DocumentKind::from_bytes(b"Paris"), None);
        assert_eq!(DocumentKind::from_bytes(b"bonjour"), None);
        assert_eq!(DocumentKind::from_bytes(b""), None);
    }

    #[test]
    fn seul_le_pdf_n_est_pas_une_image() {
        assert!(!DocumentKind::Pdf.is_picture());
        assert!(!DocumentKind::Ai.is_picture());
        assert!(DocumentKind::Jpeg.is_picture());
        assert!(DocumentKind::Raw.is_picture());
    }

    #[test]
    fn precise_le_type_avec_l_extension() {
        let pdf = b"%PDF-1.5\n";
        assert_eq!(DocumentKind::Pdf.refine(Path::new("logo.ai"), pdf), DocumentKind::Ai);
        assert_eq!(DocumentKind::Pdf.refine(Path::new("logo.pdf"), pdf), DocumentKind::Pdf);
        let tiff = b"II*\0\x08\0\0\0\0\0\0\0\0\0";
        assert_eq!(DocumentKind::Tiff.refine(Path::new("IMG_0001.DNG"), tiff), DocumentKind::Raw);
        assert_eq!(DocumentKind::Tiff.refine(Path::new("scan.tif"), tiff), DocumentKind::Tiff);
    }

    #[test]
    fn reconnait_les_noms_de_fichiers() {
        assert!(is_openable_name(Path::new("C:/a/Plan.PDF")));
        assert!(is_openable_name(Path::new("photo.HEIC")));
        assert!(!is_openable_name(Path::new("notes.txt")));
        assert!(is_picture_name(Path::new("scan.tiff")));
        assert!(!is_picture_name(Path::new("logo.ai")));
        assert!(!is_picture_name(Path::new("sans-extension")));
    }

    #[test]
    fn format_annonce_par_l_extension() {
        assert_eq!(DocumentKind::from_extension(Path::new("a/photo.JPG")), Some(DocumentKind::Jpeg));
        assert_eq!(DocumentKind::from_extension(Path::new("scan.tiff")), Some(DocumentKind::Tiff));
        assert_eq!(DocumentKind::from_extension(Path::new("photo.heic")), None);
    }

    #[test]
    fn formats_reecrits_sur_place() {
        assert!(DocumentKind::Jpeg.is_writable());
        assert!(DocumentKind::Bmp.is_writable());
        assert!(!DocumentKind::Heic.is_writable());
        assert!(!DocumentKind::Raw.is_writable());
        assert!(!DocumentKind::Ai.is_writable());
    }
}
