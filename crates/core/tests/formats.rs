//! Les formats ajoutés après JPEG, PNG, WebP et GIF : chacun s'ouvre, se dessine et s'enregistre.
//! Les exemples (tests/fixtures/formats/) montrent une image rouge à gauche et bleue à droite.

use std::path::PathBuf;

use documents_core::{DocumentKind, Library, PageRef, Rotation};
use image::DynamicImage;

fn fixture(nom: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/formats").join(nom)
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
    let dir = std::env::temp_dir().join(format!("documents-formats-{}-{nom}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn rendu(lib: &Library, id: u32, page: usize) -> DynamicImage {
    image::load_from_memory(&lib.render(id, page, 400, Rotation::R0).unwrap().bytes).unwrap()
}

/// Rouge à gauche, bleu à droite (à quelques nuances près : compression, conversion).
fn rouge_puis_bleu(img: &DynamicImage) -> bool {
    let rgb = img.to_rgb8();
    let (w, h) = (rgb.width(), rgb.height());
    let [r, _, b] = rgb.get_pixel(w / 6, h / 2).0.map(i32::from);
    let [r2, _, b2] = rgb.get_pixel(w * 5 / 6, h / 2).0.map(i32::from);
    r > b + 60 && b2 > r2 + 60
}

#[test]
fn ouvre_chaque_format_d_image() {
    let mut lib = library();
    for (nom, kind, writable) in [
        ("image.bmp", DocumentKind::Bmp, true),
        ("image.ppm", DocumentKind::Pnm, true),
        ("image.pgm", DocumentKind::Pnm, true),
        ("image.tif", DocumentKind::Tiff, true),
        ("image.jp2", DocumentKind::Jp2, false),
        ("image.hdr", DocumentKind::Hdr, false),
        ("image.exr", DocumentKind::Exr, false),
        ("photo.cr2", DocumentKind::Raw, false),
    ] {
        let info = lib.open(&fixture(nom)).unwrap_or_else(|e| panic!("{nom} : {e}"));
        assert_eq!(info.kind, kind, "{nom}");
        assert_eq!(info.writable, writable, "{nom}");
        assert!(!info.paged, "{nom}");
        assert_eq!(info.pages.len(), 1);
        assert_eq!((info.pages[0].width, info.pages[0].height), (if nom == "photo.cr2" { 60.0 } else { 40.0 }, if nom == "photo.cr2" { 40.0 } else { 30.0 }), "{nom}");
        let img = rendu(&lib, info.id, 0);
        if nom != "image.pgm" {
            assert!(rouge_puis_bleu(&img), "{nom} : couleurs inattendues");
        }
    }
}

#[test]
fn une_icone_montre_sa_plus_grande_taille() {
    let info = library().open(&fixture("icone.ico")).unwrap();
    assert_eq!(info.kind, DocumentKind::Ico);
    assert_eq!(info.pages[0].width, 32.0);
    assert!(!info.writable, "une icône garde ses autres tailles : on enregistre une copie");
}

#[test]
fn image_hdr_ramenee_a_l_ecran() {
    let lib_img = {
        let mut lib = library();
        let id = lib.open(&fixture("image.hdr")).unwrap().id;
        rendu(&lib, id, 0)
    };
    // 4.0 en rouge : pas « brûlé » en blanc, le rouge domine nettement.
    let p = lib_img.to_rgb8().get_pixel(5, 15).0;
    assert!(p[0] > 240 && p[1] < 200 && p[2] < 200, "{p:?}");
}

#[test]
fn photo_brute_dng_montre_son_plus_grand_apercu_redresse() {
    let mut lib = library();
    let info = lib.open(&fixture("photo.dng")).unwrap();
    assert_eq!(info.kind, DocumentKind::Raw);
    // Aperçu de 64 × 48, enregistré couché (orientation 6) : affiché debout.
    assert_eq!((info.pages[0].width, info.pages[0].height), (48.0, 64.0));
}

#[test]
fn photo_heic() {
    if !documents_core::heic_available() {
        eprintln!("libheif absente : test HEIC sauté");
        return;
    }
    let mut lib = library();
    let info = lib.open(&fixture("photo.heic")).unwrap();
    assert_eq!(info.kind, DocumentKind::Heic);
    assert_eq!((info.pages[0].width, info.pages[0].height), (40.0, 30.0));
    assert!(rouge_puis_bleu(&rendu(&lib, info.id, 0)));
}

#[test]
fn dessin_ai_ouvert_comme_un_pdf() {
    let mut lib = library();
    let info = lib.open(&fixture("dessin.ai")).unwrap();
    assert_eq!(info.kind, DocumentKind::Ai);
    assert!(info.paged);
    assert!(!info.writable, "« Enregistrer » propose un PDF à côté");
    assert_eq!(info.pages.len(), 1);
}

#[test]
fn fax_de_deux_pages_feuillete_comme_un_pdf() {
    let mut lib = library();
    let info = lib.open(&fixture("fax.tif")).unwrap();
    assert_eq!(info.kind, DocumentKind::Tiff);
    assert!(info.paged);
    assert!(!info.writable);
    assert_eq!(info.pages.len(), 2);
    // 64 × 32 points à 204 × 98 ppp : les pixels sont deux fois plus hauts, la page aussi.
    let size = info.pages[0];
    assert!((size.height / size.width - 1.04).abs() < 0.05, "{size:?}");
    // Page 1 : noir à gauche ; page 2 : noir à droite.
    for (page, noir_a_gauche) in [(0, true), (1, false)] {
        let img = rendu(&lib, info.id, page).to_luma8();
        let (w, h) = (img.width(), img.height());
        let gauche = img.get_pixel(w / 4, h / 2).0[0];
        let droite = img.get_pixel(w * 3 / 4, h / 2).0[0];
        assert_eq!(gauche < 100, noir_a_gauche, "page {page}");
        assert_eq!(droite < 100, !noir_a_gauche, "page {page}");
    }
    // Enregistré en PDF : deux pages.
    let dest = workdir("fax").join("fax.pdf");
    lib.save_pdf(&[PageRef::new(info.id, 0, 0), PageRef::new(info.id, 1, 0)], &dest).unwrap();
    assert_eq!(lib.open(&dest).unwrap().pages.len(), 2);
}

#[test]
fn enregistre_une_copie_quand_le_format_ne_se_reecrit_pas() {
    let dir = workdir("copie");
    for nom in ["image.jp2", "photo.cr2"] {
        std::fs::copy(fixture(nom), dir.join(nom)).unwrap();
    }
    let mut lib = library();
    let jp2 = lib.open(&dir.join("image.jp2")).unwrap();
    let dest = lib.save_path(jp2.id).unwrap();
    assert_eq!(dest, dir.join("image.jpg"));
    lib.save_picture(jp2.id, 90, &dest).unwrap();
    let saved = image::open(&dest).unwrap();
    assert_eq!((saved.width(), saved.height()), (30, 40), "tournée d'un quart de tour");
    // Une deuxième copie ne remplace pas la première.
    assert_eq!(lib.save_path(jp2.id).unwrap(), dir.join("image (2).jpg"));
    // L'original n'a pas bougé.
    assert_eq!(std::fs::read(dir.join("image.jp2")).unwrap(), std::fs::read(fixture("image.jp2")).unwrap());

    let raw = lib.open(&dir.join("photo.cr2")).unwrap();
    let dest = lib.save_path(raw.id).unwrap();
    assert_eq!(dest, dir.join("photo.jpg"));
    lib.save_picture(raw.id, 0, &dest).unwrap();
    assert!(rouge_puis_bleu(&image::open(&dest).unwrap()), "la copie est l'image, pas le fichier brut");
}

#[test]
fn reecrit_sur_place_dans_le_meme_format() {
    let dir = workdir("sur-place");
    let mut lib = library();
    for nom in ["image.bmp", "image.ppm", "image.tif"] {
        let path = dir.join(nom);
        std::fs::copy(fixture(nom), &path).unwrap();
        let info = lib.open(&path).unwrap();
        assert_eq!(lib.save_path(info.id).unwrap(), path);
        lib.save_picture(info.id, 180, &path).unwrap();
        lib.close(info.id);
        let again = lib.open(&path).unwrap();
        assert_eq!(again.kind, info.kind, "{nom} reste au même format");
        let img = rendu(&lib, again.id, 0).to_rgb8();
        // Tournée d'un demi-tour : le bleu est passé à gauche.
        assert!(img.get_pixel(2, 15).0[2] > 150, "{nom}");
    }
}

#[test]
fn bande_du_dossier_et_vignettes() {
    let list = documents_core::folder::images_in(&fixture("")).unwrap();
    let names: Vec<_> = list.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
    assert!(names.contains(&"photo.cr2".to_string()) && names.contains(&"fax.tif".to_string()));
    assert!(!names.contains(&"dessin.ai".to_string()), "un dessin n'est pas une image de la bande");
    for nom in ["photo.dng", "image.exr", "image.jp2", "fax.tif"] {
        let (bytes, _) = documents_core::folder::thumbnail(&fixture(nom), 32).unwrap_or_else(|e| panic!("{nom} : {e}"));
        let img = image::load_from_memory(&bytes).unwrap();
        assert!(img.width().max(img.height()) <= 32, "{nom}");
    }
}
