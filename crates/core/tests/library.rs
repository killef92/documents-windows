use std::path::PathBuf;

use documents_core::{DocumentKind, Error, Library, Rotation};

fn fixture(nom: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(nom)
}

/// PDFium : dossier donné par PDFIUM_DIR, sinon vendor/pdfium/<plateforme> (voir scripts/pdfium).
fn library() -> Library {
    let racine = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vendor/pdfium");
    let mut dirs: Vec<PathBuf> = std::env::var_os("PDFIUM_DIR").map(PathBuf::from).into_iter().collect();
    if let Ok(entries) = std::fs::read_dir(&racine) {
        dirs.extend(entries.flatten().map(|e| e.path()));
    }
    Library::new(dirs)
}

fn png_size(bytes: &[u8]) -> (u32, u32) {
    let img = image::load_from_memory(bytes).unwrap();
    (img.width(), img.height())
}

#[test]
fn ouvre_un_pdf_de_trois_pages_au_format_a4() {
    let info = library().open(&fixture("trois-pages.pdf")).unwrap();
    assert_eq!(info.kind, DocumentKind::Pdf);
    assert_eq!(info.name, "trois-pages.pdf");
    assert_eq!(info.pages.len(), 3);
    // A4 = 595 × 842 points = 794 × 1123 pixels.
    assert_eq!(info.pages[0].width.round(), 794.0);
    assert_eq!(info.pages[0].height.round(), 1123.0);
}

#[test]
fn une_page_paysage_ou_pivotee_est_plus_large_que_haute() {
    let mut lib = library();
    for nom in ["paysage.pdf", "page-pivotee.pdf"] {
        let page = lib.open(&fixture(nom)).unwrap().pages[0];
        assert!(page.width > page.height, "{nom}");
    }
}

#[test]
fn rend_une_page_a_la_largeur_demandee() {
    let mut lib = library();
    let id = lib.open(&fixture("trois-pages.pdf")).unwrap().id;
    let img = lib.render(id, 0, 300, Rotation::R0).unwrap();
    assert_eq!(img.mime, "image/png");
    assert_eq!(png_size(&img.bytes), (300, 424));
    // Pivotée d'un quart de tour : la page devient paysage.
    let img = lib.render(id, 0, 300, Rotation::R90).unwrap();
    assert_eq!(png_size(&img.bytes), (300, 212));
    // La page rendue n'est pas toute blanche : le texte est bien dessiné.
    let pixels = image::load_from_memory(&img.bytes).unwrap().into_luma8();
    assert!(pixels.pixels().any(|p| p.0[0] < 100));
}

#[test]
fn page_ou_document_inexistant() {
    let mut lib = library();
    let id = lib.open(&fixture("trois-pages.pdf")).unwrap().id;
    assert!(matches!(lib.render(id, 3, 100, Rotation::R0), Err(Error::UnknownPage)));
    lib.close(id);
    assert!(matches!(lib.render(id, 0, 100, Rotation::R0), Err(Error::UnknownDocument)));
}

#[test]
fn cherche_du_texte_sans_tenir_compte_des_majuscules() {
    let mut lib = library();
    let id = lib.open(&fixture("trois-pages.pdf")).unwrap().id;
    let hits = lib.search("DELAI");
    assert_eq!(hits.len(), 2);
    assert!(hits.iter().all(|h| h.doc == id && h.page == 1));
    assert_eq!(hits[0].context, "Delai d'instruction : deux mois");
    let r = hits[0].rects[0];
    // « Delai » est en haut à gauche de la page.
    assert!(r.x > 0.0 && r.x < 0.2 && r.y > 0.0 && r.y < 0.3 && r.w > 0.0 && r.h > 0.0);
    assert!(lib.search("   ").is_empty());
    assert!(lib.search("introuvable").is_empty());
}

#[test]
fn la_recherche_suit_la_rotation_du_pdf() {
    let mut lib = library();
    lib.open(&fixture("page-pivotee.pdf")).unwrap();
    let hits = lib.search("texte");
    assert_eq!(hits.len(), 1);
    // Le texte écrit en haut à gauche se retrouve en haut à droite une fois la page tournée.
    let r = hits[0].rects[0];
    assert!(r.x > 0.8 && r.y < 0.3, "{r:?}");
}

#[test]
fn ouvre_des_images() {
    let mut lib = library();
    let png = lib.open(&fixture("photo.png")).unwrap();
    assert_eq!(png.kind, DocumentKind::Png);
    assert_eq!((png.pages[0].width, png.pages[0].height), (40.0, 20.0));
    let webp = lib.open(&fixture("photo.webp")).unwrap();
    assert_eq!(webp.kind, DocumentKind::Webp);
    let img = lib.render(png.id, 0, 20, Rotation::R0).unwrap();
    assert_eq!(png_size(&img.bytes), (20, 10));
    // Jamais agrandie au-delà de sa taille réelle.
    let img = lib.render(png.id, 0, 4000, Rotation::R0).unwrap();
    assert_eq!(png_size(&img.bytes), (40, 20));
}

#[test]
fn une_photo_est_redressee_selon_son_orientation() {
    let mut lib = library();
    let jpg = lib.open(&fixture("photo-pivotee.jpg")).unwrap();
    assert_eq!(jpg.kind, DocumentKind::Jpeg);
    assert_eq!((jpg.pages[0].width, jpg.pages[0].height), (20.0, 40.0));
    let img = lib.render(jpg.id, 0, 20, Rotation::R90).unwrap();
    assert_eq!(img.mime, "image/jpeg");
    assert_eq!(png_size(&img.bytes), (20, 10));
}

#[test]
fn refuse_ce_qui_n_est_pas_un_document() {
    let mut lib = library();
    assert!(matches!(lib.open(&fixture("pas-un-document.txt")), Err(Error::UnsupportedFormat)));
    assert!(matches!(lib.open(&fixture("absent.pdf")), Err(Error::Io(_))));
}

#[test]
fn informations_pour_l_inspecteur() {
    let mut lib = library();
    let pdf = lib.open(&fixture("trois-pages.pdf")).unwrap();
    let d = lib.details(pdf.id).unwrap();
    assert_eq!(d.version.as_deref(), Some("1.3"));
    assert_eq!(d.producer.as_deref(), Some("ReportLab PDF Library - (opensource)"));
    assert!(d.created.as_deref().is_some_and(|c| c.len() == 19 && c.as_bytes()[10] == b'T'));
    assert_eq!(d.file_size, std::fs::metadata(fixture("trois-pages.pdf")).unwrap().len());
    assert!(d.file_modified.is_some());
    // Une photo : seulement le fichier.
    let photo = lib.open(&fixture("photo.png")).unwrap();
    let d = lib.details(photo.id).unwrap();
    assert!(d.file_size > 0);
    assert_eq!(d.version, None);
    assert_eq!(d.title, None);
}

#[test]
fn un_gif_reste_anime() {
    let mut lib = library();
    let gif = lib.open(&fixture("anime.gif")).unwrap();
    assert_eq!(gif.kind, DocumentKind::Gif);
    assert_eq!((gif.pages[0].width, gif.pages[0].height), (40.0, 20.0));
    // Droit : le fichier tel quel, pour que l'affichage l'anime.
    let img = lib.render(gif.id, 0, 20, Rotation::R0).unwrap();
    assert_eq!(img.mime, "image/gif");
    assert_eq!(img.bytes, std::fs::read(fixture("anime.gif")).unwrap());
    // Tourné : toujours animé dans la fenêtre…
    let img = lib.render(gif.id, 0, 800, Rotation::R90).unwrap();
    assert_eq!(img.mime, "image/gif");
    assert_eq!((img.width, img.height), (20, 40));
    // … mais une image fixe pour une petite miniature.
    let img = lib.render(gif.id, 0, 10, Rotation::R90).unwrap();
    assert_eq!(png_size(&img.bytes), (10, 20));
}

#[test]
fn un_gif_tourne_garde_son_animation() {
    use image::AnimationDecoder;
    let mut lib = library();
    let gif = lib.open(&fixture("anime.gif")).unwrap();
    let dest = std::env::temp_dir().join(format!("documents-gif-{}.gif", std::process::id()));
    lib.save_picture(gif.id, 90, &dest).unwrap();
    let bytes = std::fs::read(&dest).unwrap();
    std::fs::remove_file(&dest).unwrap();
    let frames = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(bytes))
        .unwrap()
        .into_frames()
        .collect_frames()
        .unwrap();
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0].buffer().dimensions(), (20, 40));
    // La première image est rouge, la seconde bleue.
    assert!(frames[0].buffer().get_pixel(5, 5)[0] > 150);
    assert!(frames[1].buffer().get_pixel(5, 5)[2] > 150);
}
