use std::path::PathBuf;

use documents_core::{CropRect, Library, PageRef, Retouch};

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

fn workdir(nom: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("documents-retouche-{}-{nom}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn recadre_une_photo_puis_l_enregistre() {
    let dir = workdir("recadre");
    // Une copie : on enregistre « par-dessus », comme le bouton Enregistrer.
    let path = dir.join("photo.jpg");
    std::fs::copy(fixture("photo-pivotee.jpg"), &path).unwrap();
    let before = std::fs::read(&path).unwrap();

    let mut lib = library();
    let info = lib.open(&path).unwrap();
    let (w, h) = (info.pages[0].width, info.pages[0].height);
    let crop = CropRect { x: 2.0, y: 3.0, w: w / 2.0, h: h / 2.0 };
    let edited = lib.retouch_picture(info.id, &Retouch { crop: Some(crop), brightness: 0.2, ..Default::default() }).unwrap();
    assert!(edited.edited && edited.id != info.id);
    assert_eq!((edited.name.as_str(), edited.path.as_path()), (info.name.as_str(), path.as_path()));
    assert_eq!((edited.pages[0].width, edited.pages[0].height), ((w / 2.0).round(), (h / 2.0).round()));
    // Rien n'est écrit avant d'enregistrer.
    assert_eq!(std::fs::read(&path).unwrap(), before);

    lib.save_picture(edited.id, 0, &path).unwrap();
    let again = lib.open(&path).unwrap();
    assert_eq!((again.pages[0].width, again.pages[0].height), ((w / 2.0).round(), (h / 2.0).round()));
    assert!(!again.edited);

    // L'original retouché garde son état : on peut annuler.
    assert_eq!(lib.render(info.id, 0, 1000, Default::default()).unwrap().width as f32, w);
}

#[test]
fn photo_retouchee_dans_un_pdf() {
    let dir = workdir("pdf");
    let mut lib = library();
    let info = lib.open(&fixture("photo.png")).unwrap();
    let crop = CropRect { x: 0.0, y: 0.0, w: 10.0, h: 10.0 };
    let edited = lib.retouch_picture(info.id, &Retouch { crop: Some(crop), ..Default::default() }).unwrap();
    let dest = dir.join("photo.pdf");
    lib.save_pdf(&[PageRef::new(edited.id, 0, 0)], &dest).unwrap();
    let doc = lopdf::Document::load(&dest).unwrap();
    assert_eq!(doc.get_pages().len(), 1);
}

#[test]
fn enregistre_un_pdf_en_taille_reduite() {
    let dir = workdir("reduit");
    let mut lib = library();
    let info = lib.open(&fixture("trois-pages.pdf")).unwrap();
    let pages: Vec<_> = (0..3).map(|i| PageRef::new(info.id, i, 0)).collect();
    let dest = dir.join("reduit.pdf");
    let (before, after) = lib.save_pdf_reduced(&pages, &[], documents_core::reduce::Level::Forte, &dest).unwrap();
    assert!(after <= before);
    assert_eq!(std::fs::metadata(&dest).unwrap().len(), after);
    let again = lib.open(&dest).unwrap();
    assert_eq!(again.pages.len(), 3);
}
