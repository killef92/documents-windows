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
            && matches!(&head[8..12], b"heic" | b"heix" | b"mif1" | b"msf1" | b"heim" | b"heis")
        {
            Some(Self::Heic)
        } else {
            None
        }
    }

    /// Lit le début du fichier et en déduit le type.
    pub fn detect(path: &Path) -> io::Result<Option<Self>> {
        let mut head = Vec::with_capacity(16);
        File::open(path)?.take(16).read_to_end(&mut head)?;
        Ok(Self::from_bytes(&head))
    }

    pub fn is_picture(self) -> bool {
        !matches!(self, Self::Pdf)
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
        assert_eq!(DocumentKind::from_bytes(b"bonjour"), None);
        assert_eq!(DocumentKind::from_bytes(b""), None);
    }

    #[test]
    fn seul_le_pdf_n_est_pas_une_image() {
        assert!(!DocumentKind::Pdf.is_picture());
        assert!(DocumentKind::Jpeg.is_picture());
    }
}
