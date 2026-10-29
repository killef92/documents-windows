use std::path::{Path, PathBuf};

use documents_core::annotate::{Align, Annotation, Color, Font, Point, Rect, Shape};
use documents_core::{Library, PageRef};

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
    let dir = std::env::temp_dir().join(format!("documents-annot-{}-{nom}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const RED: Color = Color(255, 0, 0);
const BLUE: Color = Color(0, 0, 255);

/// Un rectangle bleu plein, de 10 % à 30 % de la largeur et de 10 % à 20 % de la hauteur.
fn blue_box(w: f32, h: f32) -> Annotation {
    Annotation::Rect {
        rect: Rect { x: 0.1 * w, y: 0.1 * h, w: 0.2 * w, h: 0.1 * h },
        stroke: None,
        fill: Some(BLUE),
        width: 0.0,
        angle: 0.0,
        shape: Shape::Plain,
    }
}

/// La couleur d'un point (en proportions) de la page rendue, vue comme dans l'interface.
fn pixel(lib: &mut Library, path: &Path, page: usize, view_rotation: i32, x: f32, y: f32) -> [u8; 3] {
    let id = lib.open_flat(path).unwrap().id;
    let img = lib.render(id, page, 400, documents_core::Rotation::from_degrees(view_rotation)).unwrap();
    lib.close(id);
    let img = image::load_from_memory(&img.bytes).unwrap().into_rgb8();
    let px = img.get_pixel((x * img.width() as f32) as u32, (y * img.height() as f32) as u32);
    px.0
}

fn is_blue(c: [u8; 3]) -> bool {
    c[2] > 200 && c[0] < 60 && c[1] < 60
}

/// Le rectangle doit arriver au même endroit que dans l'interface, quelle que soit la rotation
/// du PDF et celle choisie dans l'appli.
fn check_box(file: &str, user_rotation: i32) {
    let dir = workdir(&format!("{file}-{user_rotation}"));
    let mut lib = library();
    let info = lib.open(&fixture(file)).unwrap();
    let size = info.pages[0];
    let mut page = PageRef::new(info.id, 0, user_rotation);
    page.annotations.push(blue_box(size.width, size.height).into());
    let dest = dir.join("annote.pdf");
    lib.save_pdf(&[page], &dest).unwrap();
    // On regarde le résultat sans la rotation de l'appli : comme quand l'annotation a été posée.
    let back = -user_rotation;
    assert!(is_blue(pixel(&mut lib, &dest, 0, back, 0.2, 0.15)), "{file} {user_rotation} : pas de bleu au centre");
    assert!(!is_blue(pixel(&mut lib, &dest, 0, back, 0.8, 0.85)), "{file} {user_rotation} : bleu au mauvais endroit");
    assert!(!is_blue(pixel(&mut lib, &dest, 0, back, 0.35, 0.15)), "{file} {user_rotation} : trop large");
}

#[test]
fn rectangle_au_bon_endroit_sur_une_page_droite() {
    check_box("trois-pages.pdf", 0);
}

#[test]
fn rectangle_au_bon_endroit_sur_une_page_tournee_dans_l_appli() {
    check_box("trois-pages.pdf", 90);
    check_box("trois-pages.pdf", 180);
}

#[test]
fn rectangle_au_bon_endroit_sur_une_page_deja_tournee_dans_le_pdf() {
    check_box("page-pivotee.pdf", 0);
    check_box("page-pivotee.pdf", 270);
}

#[test]
fn rectangle_au_bon_endroit_sur_une_photo_fusionnee() {
    let dir = workdir("photo");
    let mut lib = library();
    let pdf = lib.open(&fixture("une-page.pdf")).unwrap().id;
    let photo = lib.open(&fixture("photo-pivotee.jpg")).unwrap();
    let size = photo.pages[0];
    let mut page = PageRef::new(photo.id, 0, 90);
    page.annotations.push(blue_box(size.width, size.height).into());
    let dest = dir.join("fusion.pdf");
    lib.save_pdf(&[PageRef::new(pdf, 0, 0), page], &dest).unwrap();
    assert!(is_blue(pixel(&mut lib, &dest, 1, -90, 0.2, 0.15)));
    assert!(!is_blue(pixel(&mut lib, &dest, 1, -90, 0.8, 0.85)));
}

#[test]
fn forme_tournee_au_bon_endroit() {
    // Une bande bleue horizontale, tournée d'un quart de tour : elle devient verticale.
    let dir = workdir("tournee");
    let mut lib = library();
    let info = lib.open(&fixture("trois-pages.pdf")).unwrap();
    let (w, h) = (info.pages[0].width, info.pages[0].height);
    let mut page = PageRef::new(info.id, 0, 0);
    page.annotations.push(Annotation::Rect {
        rect: Rect { x: 0.3 * w, y: 0.5 * h - 0.02 * w, w: 0.4 * w, h: 0.04 * w },
        stroke: None,
        fill: Some(BLUE),
        width: 0.0,
        angle: 90.0,
        shape: Shape::Plain,
    }.into());
    let dest = dir.join("tournee.pdf");
    lib.save_pdf(&[page], &dest).unwrap();
    assert!(is_blue(pixel(&mut lib, &dest, 0, 0, 0.5, 0.5)));
    assert!(!is_blue(pixel(&mut lib, &dest, 0, 0, 0.35, 0.5)), "encore horizontale");
    assert!(is_blue(pixel(&mut lib, &dest, 0, 0, 0.5, 0.5 - 0.15 * w / h)), "pas verticale");
}

#[test]
fn etoile_dessinee_dans_sa_zone() {
    // Une étoile bleue sur toute la page : le centre est bleu, les coins hauts ne le sont pas.
    let dir = workdir("etoile");
    let mut lib = library();
    let info = lib.open(&fixture("trois-pages.pdf")).unwrap();
    let (w, h) = (info.pages[0].width, info.pages[0].height);
    let mut page = PageRef::new(info.id, 0, 0);
    page.annotations.push(Annotation::Rect {
        rect: Rect { x: 0.0, y: 0.0, w, h },
        stroke: None,
        fill: Some(BLUE),
        width: 0.0,
        angle: 0.0,
        shape: Shape::Star,
    }.into());
    let dest = dir.join("etoile.pdf");
    lib.save_pdf(&[page], &dest).unwrap();
    assert!(is_blue(pixel(&mut lib, &dest, 0, 0, 0.5, 0.5)));
    assert!(is_blue(pixel(&mut lib, &dest, 0, 0, 0.5, 0.05)), "la pointe du haut manque");
    assert!(!is_blue(pixel(&mut lib, &dest, 0, 0, 0.1, 0.1)));
    assert!(!is_blue(pixel(&mut lib, &dest, 0, 0, 0.9, 0.1)));
}

#[test]
fn texte_aligne_a_droite_et_souligne() {
    // Un gros bloc de texte bleu aligné à droite : la moitié gauche de la zone reste blanche.
    let dir = workdir("aligne");
    let mut lib = library();
    let info = lib.open(&fixture("trois-pages.pdf")).unwrap();
    let w = info.pages[0].width;
    let mut page = PageRef::new(info.id, 0, 0);
    page.annotations.push(Annotation::Text {
        rect: Rect { x: 0.0, y: 100.0, w, h: 80.0 },
        lines: vec!["MMMMM".into()],
        text: None,
        font: Font::Helvetica,
        size: 60.0,
        color: BLUE,
        fill: None,
        bold: true,
        italic: false,
        underline: true,
        align: Align::Right,
        widths: vec![0.3 * w],
    }.into());
    let dest = dir.join("aligne.pdf");
    lib.save_pdf(&[page], &dest).unwrap();
    let h = info.pages[0].height;
    let mut row = |x: f32| pixel(&mut lib, &dest, 0, 0, x, (100.0 + 4.0 + 60.0 * (0.947 + 0.12)) / h);
    assert!(is_blue(row(0.85)), "pas de soulignement à droite");
    assert!(!is_blue(row(0.3)), "texte pas aligné à droite");
}

#[test]
fn toutes_les_sortes_d_annotations_sont_ecrites() {
    let dir = workdir("toutes");
    let mut lib = library();
    let info = lib.open(&fixture("formulaire.pdf")).unwrap();
    let mut page = PageRef::new(info.id, 0, 0);
    let p = |x, y| Point { x, y };
    page.annotations = vec![
        Annotation::Highlight { rects: vec![Rect { x: 50.0, y: 50.0, w: 100.0, h: 16.0 }], color: Color(255, 214, 10) },
        blue_box(400.0, 400.0),
        Annotation::Ellipse { rect: Rect { x: 200.0, y: 200.0, w: 80.0, h: 40.0 }, stroke: Some(RED), fill: None, width: 2.0, angle: 0.0 },
        Annotation::Rect { rect: Rect { x: 100.0, y: 450.0, w: 80.0, h: 30.0 }, stroke: Some(RED), fill: None, width: 2.0, angle: 30.0, shape: Shape::Plain },
        Annotation::Line { from: p(10.0, 300.0), to: p(200.0, 350.0), color: RED, width: 3.0, arrow: true },
        Annotation::Ink { strokes: vec![vec![p(10.0, 10.0), p(20.0, 30.0), p(40.0, 20.0)], vec![p(60.0, 60.0)]], color: RED, width: 2.0 },
        Annotation::Text {
            rect: Rect { x: 300.0, y: 400.0, w: 200.0, h: 50.0 },
            lines: vec!["Vu le « devis » (été)".into(), "ligne 2 \\ fin".into()],
            text: None,
            font: Font::Times,
            size: 14.0,
            color: Color(0, 0, 0),
            fill: Some(Color(255, 255, 200)),
            bold: true,
            italic: false,
            underline: true,
            align: Align::Center,
            widths: vec![120.0, 70.0],
        },
        Annotation::Note { at: p(500.0, 100.0), text: "À vérifier avec Kilian".into(), color: Color(255, 214, 10) },
    ]
    .into_iter()
    .map(Into::into)
    .collect();
    let dest = dir.join("toutes.pdf");
    lib.save_pdf(&[page], &dest).unwrap();

    let doc = lopdf::Document::load(&dest).unwrap();
    let page_id = *doc.get_pages().get(&1).unwrap();
    let annots = doc.get_dictionary(page_id).unwrap().get(b"Annots").unwrap().as_array().unwrap();
    let subtypes: Vec<String> = annots
        .iter()
        .filter_map(|a| doc.get_dictionary(a.as_reference().ok()?).ok())
        .map(|d| String::from_utf8_lossy(d.get(b"Subtype").unwrap().as_name().unwrap()).into_owned())
        .collect();
    // Les champs du formulaire sont gardés, les annotations ajoutées après.
    assert!(subtypes.contains(&"Widget".to_string()));
    for kind in ["Highlight", "Square", "Circle", "Polygon", "Line", "Ink", "FreeText", "Text"] {
        assert!(subtypes.contains(&kind.to_string()), "{kind} manque : {subtypes:?}");
    }
    // Chaque annotation ajoutée a son apparence.
    for a in annots {
        let d = doc.get_dictionary(a.as_reference().unwrap()).unwrap();
        if d.get(b"NM").is_ok() {
            assert!(d.get(b"AP").is_ok());
        }
    }
    // Le fichier se rouvre et s'affiche.
    let again = lib.open(&dest).unwrap();
    assert_eq!(again.pages.len(), 1);
    lib.render(again.id, 0, 300, documents_core::Rotation::R0).unwrap();
}

#[test]
fn sans_annotation_le_pdf_n_est_pas_retouche() {
    let dir = workdir("rien");
    let mut lib = library();
    let id = lib.open(&fixture("trois-pages.pdf")).unwrap().id;
    let dest = dir.join("a.pdf");
    lib.save_pdf(&[PageRef::new(id, 0, 0)], &dest).unwrap();
    let doc = lopdf::Document::load(&dest).unwrap();
    let page_id = *doc.get_pages().get(&1).unwrap();
    assert!(doc.get_dictionary(page_id).unwrap().get(b"Annots").is_err());
}

#[test]
fn texte_d_une_page_avec_la_place_des_lettres() {
    let mut lib = library();
    let id = lib.open(&fixture("trois-pages.pdf")).unwrap().id;
    let text = lib.page_text(id, 0).unwrap();
    assert_eq!(text.chars.len(), text.rects.len());
    let s: String = text.chars.iter().collect();
    assert!(s.contains("Recepisse"), "{s}");
    let r = text.rects[s.find('R').unwrap()];
    assert!(r.w > 0.0 && r.x >= 0.0 && r.x < 1.0 && r.y >= 0.0 && r.y < 1.0, "{r:?}");

    // Page tournée dans le PDF : les places sont celles de la page affichée.
    let id = lib.open(&fixture("page-pivotee.pdf")).unwrap().id;
    let text = lib.page_text(id, 0).unwrap();
    assert!(text.rects.iter().all(|r| r.x >= 0.0 && r.x <= 1.0 && r.y >= 0.0 && r.y <= 1.0));

    let photo = lib.open(&fixture("photo.png")).unwrap().id;
    assert!(lib.page_text(photo, 0).unwrap().chars.is_empty());
}

#[test]
fn photo_enregistree_avec_ses_annotations_dessinees() {
    let dir = workdir("pixels");
    let mut lib = library();
    let photo = lib.open(&fixture("photo-pivotee.jpg")).unwrap().id;
    let mut png = Vec::new();
    image::RgbImage::from_pixel(30, 20, image::Rgb([255, 0, 0]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    let dest = dir.join("photo.jpg");
    lib.save_picture_pixels(photo, &png, &dest).unwrap();
    let bytes = std::fs::read(&dest).unwrap();
    assert_eq!(&bytes[..2], &[0xFF, 0xD8], "toujours un JPEG");
    let again = lib.open(&dest).unwrap();
    assert_eq!((again.pages[0].width, again.pages[0].height), (30.0, 20.0));
}

fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let v = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(T[(v >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[test]
fn image_de_signature_posee_sur_la_page() {
    let dir = workdir("image");
    let mut lib = library();
    let info = lib.open(&fixture("trois-pages.pdf")).unwrap();
    let size = info.pages[0];
    // Une « signature » bleue opaque à gauche, transparente à droite.
    let img = image::RgbaImage::from_fn(40, 20, |x, _| if x < 20 { image::Rgba([0, 0, 255, 255]) } else { image::Rgba([0, 0, 0, 0]) });
    let mut png = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
    let mut page = PageRef::new(info.id, 0, 0);
    page.annotations.push(Annotation::Image {
        rect: Rect { x: 0.1 * size.width, y: 0.1 * size.height, w: 0.4 * size.width, h: 0.1 * size.height },
        png: format!("data:image/png;base64,{}", base64(&png)),
    }.into());
    let dest = dir.join("signe.pdf");
    lib.save_pdf(&[page], &dest).unwrap();
    assert!(is_blue(pixel(&mut lib, &dest, 0, 0, 0.2, 0.15)), "partie opaque de l'image");
    assert!(!is_blue(pixel(&mut lib, &dest, 0, 0, 0.4, 0.15)), "partie transparente");
}
