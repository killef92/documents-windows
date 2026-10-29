use std::path::PathBuf;

use documents_core::DocumentKind;

fn fixture(nom: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(nom)
}

#[test]
fn detecte_un_pdf() {
    assert_eq!(DocumentKind::detect(&fixture("une-page.pdf")).unwrap(), Some(DocumentKind::Pdf));
}

#[test]
fn detecte_un_png() {
    assert_eq!(DocumentKind::detect(&fixture("pixel.png")).unwrap(), Some(DocumentKind::Png));
}

#[test]
fn fichier_absent_renvoie_une_erreur() {
    assert!(DocumentKind::detect(&fixture("absent.pdf")).is_err());
}
