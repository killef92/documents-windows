use std::path::{Path, PathBuf};

use documents_core::{DocumentKind, Error, Library, PageRef};

fn fixture(nom: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(nom)
}

fn library() -> Library {
    let racine = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vendor/pdfium");
    let mut dirs: Vec<PathBuf> = std::env::var_os("PDFIUM_DIR").map(PathBuf::from).into_iter().collect();
    if let Ok(entries) = std::fs::read_dir(&racine) {
        dirs.extend(entries.flatten().map(|e| e.path()));
    }
    Library::new(dirs)
}

/// Un dossier de travail vide, propre à chaque test.
fn workdir(nom: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("documents-tests-{}-{nom}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn page(doc: u32, page: usize, rotation: i32) -> PageRef {
    PageRef::new(doc, page, rotation)
}

fn texts_of(lib: &mut Library, path: &Path, words: &[&str]) -> Vec<usize> {
    let id = lib.open(path).unwrap().id;
    let mut found = Vec::new();
    for w in words {
        found.push(lib.search(w).iter().find(|h| h.doc == id).map(|h| h.page).unwrap_or(usize::MAX));
    }
    lib.close(id);
    found
}

#[test]
fn reordonne_supprime_et_pivote_les_pages_d_un_pdf() {
    let dir = workdir("reordonne");
    let mut lib = library();
    let id = lib.open(&fixture("trois-pages.pdf")).unwrap().id;
    let dest = dir.join("resultat.pdf");
    // Page 3, puis page 1 tournée d'un quart de tour ; la page 2 est supprimée.
    lib.save_pdf(&[page(id, 2, 0), page(id, 0, 90)], &dest).unwrap();

    let info = lib.open(&dest).unwrap();
    assert_eq!(info.pages.len(), 2);
    assert!(info.pages[0].height > info.pages[0].width);
    assert!(info.pages[1].width > info.pages[1].height, "la page tournée est en paysage");
    lib.close(info.id);
    assert_eq!(texts_of(&mut lib, &dest, &["Signature", "Recepisse", "instruction"]), vec![0, 1, usize::MAX]);
    // Pas de fichier temporaire oublié.
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
}

#[test]
fn garde_les_champs_de_formulaire() {
    let dir = workdir("formulaire");
    let mut lib = library();
    let id = lib.open(&fixture("formulaire.pdf")).unwrap().id;
    let dest = dir.join("formulaire.pdf");
    lib.save_pdf(&[page(id, 1, 0), page(id, 0, 0)], &dest).unwrap();
    let bytes = std::fs::read(&dest).unwrap();
    let doc = lopdf::Document::load_mem(&bytes).unwrap();
    assert!(doc.catalog().unwrap().has(b"AcroForm"));
    assert_eq!(doc.get_pages().len(), 2);
}

#[test]
fn fusionne_un_pdf_et_une_photo() {
    let dir = workdir("fusion");
    let mut lib = library();
    let pdf = lib.open(&fixture("trois-pages.pdf")).unwrap().id;
    let photo = lib.open(&fixture("photo-pivotee.jpg")).unwrap().id;
    let dest = dir.join("fusion.pdf");
    lib.save_pdf(&[page(pdf, 1, 0), page(photo, 0, 0), page(pdf, 0, 180)], &dest).unwrap();

    let info = lib.open(&dest).unwrap();
    assert_eq!(info.kind, DocumentKind::Pdf);
    assert_eq!(info.pages.len(), 3);
    // La photo (20 × 40 px, une fois redressée) devient une page de 20 × 40 points.
    let p = info.pages[1];
    assert_eq!(((p.width * 0.75).round(), (p.height * 0.75).round()), (20.0, 40.0));
    lib.close(info.id);
    assert_eq!(texts_of(&mut lib, &dest, &["instruction", "Recepisse"]), vec![0, 2]);
}

#[test]
fn rien_a_enregistrer() {
    let lib = library();
    assert!(matches!(lib.save_pdf(&[], Path::new("x.pdf")), Err(Error::NothingToSave)));
}

#[test]
fn pivote_une_photo_jpeg_sans_la_recompresser() {
    let dir = workdir("jpeg");
    let mut lib = library();
    let id = lib.open(&fixture("photo-pivotee.jpg")).unwrap().id;
    let dest = dir.join("photo.jpg");
    lib.save_picture(id, 90, &dest).unwrap();

    let original = std::fs::read(fixture("photo-pivotee.jpg")).unwrap();
    let saved = std::fs::read(&dest).unwrap();
    assert_eq!(original.len(), saved.len(), "seule l'orientation change");
    let info = lib.open(&dest).unwrap();
    // 20 × 40 une fois redressée, puis un quart de tour : 40 × 20.
    assert_eq!((info.pages[0].width, info.pages[0].height), (40.0, 20.0));
}

#[test]
fn ajoute_l_orientation_a_un_jpeg_qui_n_en_a_pas() {
    let dir = workdir("jpeg-sans-exif");
    let src = dir.join("sans-exif.jpg");
    image::RgbImage::from_pixel(30, 10, image::Rgb([10, 200, 10])).save(&src).unwrap();
    let mut lib = library();
    let id = lib.open(&src).unwrap().id;
    lib.save_picture(id, 270, &src).unwrap();
    let info = lib.open(&src).unwrap();
    assert_eq!((info.pages[0].width, info.pages[0].height), (10.0, 30.0));
}

#[test]
fn pivote_une_image_png() {
    let dir = workdir("png");
    let mut lib = library();
    let id = lib.open(&fixture("photo.png")).unwrap().id;
    let dest = dir.join("photo.png");
    lib.save_picture(id, 90, &dest).unwrap();
    let info = lib.open(&dest).unwrap();
    assert_eq!(info.kind, DocumentKind::Png);
    assert_eq!((info.pages[0].width, info.pages[0].height), (20.0, 40.0));
}
