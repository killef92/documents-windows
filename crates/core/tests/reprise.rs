//! Rouvrir un PDF annoté : les annotations redeviennent modifiables, et se réenregistrent
//! sans doublon ni perte.

use std::path::{Path, PathBuf};

use documents_core::annotate::{Align, Annotation, Color, Font, Placed, Point, Rect, Shape};
use documents_core::{Library, PageRef, Rotation};
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};

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
    let dir = std::env::temp_dir().join(format!("documents-reprise-{}-{nom}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const RED: Color = Color(255, 0, 0);
const BLUE: Color = Color(0, 0, 255);

fn p(x: f32, y: f32) -> Point {
    Point { x, y }
}

fn signature_png() -> String {
    let img = image::RgbaImage::from_fn(8, 4, |x, _| if x < 4 { image::Rgba([0, 0, 255, 255]) } else { image::Rgba([0, 0, 0, 0]) });
    let mut png = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
    documents_core::signature::to_data_url(&png)
}

/// Une annotation de chaque sorte.
fn all_kinds() -> Vec<Annotation> {
    vec![
        Annotation::Highlight { rects: vec![Rect { x: 50.0, y: 50.0, w: 100.0, h: 16.0 }], color: Color(255, 214, 10) },
        Annotation::Rect { rect: Rect { x: 40.0, y: 100.0, w: 120.0, h: 60.0 }, stroke: None, fill: Some(BLUE), width: 0.0, angle: 0.0, shape: Shape::Plain },
        Annotation::Ellipse { rect: Rect { x: 200.0, y: 200.0, w: 80.0, h: 40.0 }, stroke: Some(RED), fill: None, width: 2.0, angle: 0.0 },
        Annotation::Rect { rect: Rect { x: 100.0, y: 450.0, w: 80.0, h: 30.0 }, stroke: Some(RED), fill: None, width: 2.0, angle: 30.0, shape: Shape::Plain },
        Annotation::Rect { rect: Rect { x: 300.0, y: 500.0, w: 90.0, h: 60.0 }, stroke: Some(RED), fill: Some(BLUE), width: 4.0, angle: 0.0, shape: Shape::Star },
        Annotation::Line { from: p(10.0, 300.0), to: p(200.0, 350.0), color: RED, width: 3.0, arrow: true },
        Annotation::Ink { strokes: vec![vec![p(10.0, 10.0), p(20.0, 30.0), p(40.0, 20.0)], vec![p(60.0, 60.0)]], color: RED, width: 2.0 },
        Annotation::Text {
            rect: Rect { x: 300.0, y: 400.0, w: 200.0, h: 50.0 },
            lines: vec!["Vu le « devis »".into(), "(été)".into()],
            text: Some("Vu le « devis » (été)".into()),
            font: Font::Times,
            size: 14.0,
            color: Color(0, 0, 0),
            fill: Some(Color(255, 255, 200)),
            bold: true,
            italic: false,
            underline: true,
            align: Align::Center,
            widths: vec![100.0, 30.0],
        },
        Annotation::Note { at: p(500.0, 100.0), text: "À vérifier".into(), color: Color(255, 214, 10) },
        Annotation::Image { rect: Rect { x: 100.0, y: 700.0, w: 160.0, h: 80.0 }, png: signature_png() },
    ]
}

fn save(lib: &Library, pages: &[PageRef], dest: &Path) {
    lib.save_pdf(pages, dest).unwrap();
}

/// Les annotations (hors champs de formulaire) de la page `n` (à partir de 1) d'un fichier.
fn annots(path: &Path, n: u32) -> Vec<(ObjectId, Dictionary)> {
    let doc = Document::load(path).unwrap();
    let page = *doc.get_pages().get(&n).unwrap();
    let Ok(list) = doc.get_dictionary(page).unwrap().get(b"Annots") else { return Vec::new() };
    list.as_array()
        .unwrap()
        .iter()
        .map(|a| a.as_reference().unwrap())
        .map(|id| (id, doc.get_dictionary(id).unwrap().clone()))
        .filter(|(_, d)| d.get(b"Subtype").unwrap().as_name().unwrap() != b"Widget")
        .collect()
}

fn subtype(d: &Dictionary) -> String {
    String::from_utf8_lossy(d.get(b"Subtype").unwrap().as_name().unwrap()).into_owned()
}

/// Les mêmes, à un centième près (les nombres passent par le PDF).
fn close(a: &Annotation, b: &Annotation) -> bool {
    fn close_json(a: &serde_json::Value, b: &serde_json::Value) -> bool {
        use serde_json::Value as V;
        match (a, b) {
            (V::Number(x), V::Number(y)) => (x.as_f64().unwrap() - y.as_f64().unwrap()).abs() < 0.01,
            (V::Array(x), V::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(x, y)| close_json(x, y)),
            (V::Object(x), V::Object(y)) => x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| close_json(v, w))),
            _ => a == b,
        }
    }
    close_json(&serde_json::to_value(a).unwrap(), &serde_json::to_value(b).unwrap())
}

fn is_blue(c: [u8; 3]) -> bool {
    c[2] > 200 && c[0] < 60 && c[1] < 60
}

fn pixel(lib: &Library, id: u32, x: f32, y: f32) -> [u8; 3] {
    let img = lib.render(id, 0, 400, Rotation::R0).unwrap();
    let img = image::load_from_memory(&img.bytes).unwrap().into_rgb8();
    img.get_pixel((x * img.width() as f32) as u32, (y * img.height() as f32) as u32).0
}

#[test]
fn les_annotations_de_l_appli_reviennent_a_l_identique() {
    let dir = workdir("identique");
    let mut lib = library();
    for file in ["trois-pages.pdf", "page-pivotee.pdf"] {
        let info = lib.open(&fixture(file)).unwrap();
        assert!(lib.annotations(info.id).unwrap().iter().all(Vec::is_empty));
        let mut page = PageRef::new(info.id, 0, 0);
        page.annotations = all_kinds().into_iter().map(Into::into).collect();
        let dest = dir.join(file);
        save(&lib, &[page], &dest);

        let again = lib.open(&dest).unwrap();
        let back = lib.annotations(again.id).unwrap();
        assert_eq!(back.len(), 1);
        let back: Vec<&Annotation> = back[0].iter().map(|p| &p.annotation).collect();
        assert_eq!(back.len(), all_kinds().len(), "{file}");
        for (a, b) in all_kinds().iter().zip(back) {
            assert!(close(a, b), "{file} : {a:?}\n≠ {b:?}");
        }
    }
}

#[test]
fn la_page_rouverte_ne_les_dessine_plus() {
    let dir = workdir("dessin");
    let mut lib = library();
    let info = lib.open(&fixture("trois-pages.pdf")).unwrap();
    let size = info.pages[0];
    let mut page = PageRef::new(info.id, 0, 0);
    page.annotations.push(
        Annotation::Rect {
            rect: Rect { x: 0.1 * size.width, y: 0.1 * size.height, w: 0.2 * size.width, h: 0.1 * size.height },
            stroke: None,
            fill: Some(BLUE),
            width: 0.0,
            angle: 0.0,
            shape: Shape::Plain,
        }
        .into(),
    );
    let dest = dir.join("bleu.pdf");
    save(&lib, &[page], &dest);
    // Ouverte « à plat » : le rectangle est dans la page.
    let flat = lib.open_flat(&dest).unwrap().id;
    assert!(is_blue(pixel(&lib, flat, 0.2, 0.15)));
    // Ouverte normalement : c'est l'interface qui le dessine (sinon il serait en double).
    let id = lib.open(&dest).unwrap().id;
    assert!(!is_blue(pixel(&lib, id, 0.2, 0.15)));
    assert_eq!(lib.annotations(id).unwrap()[0].len(), 1);
}

#[test]
fn reenregistrer_sans_rien_toucher_ne_double_rien() {
    let dir = workdir("double");
    let mut lib = library();
    let info = lib.open(&fixture("formulaire.pdf")).unwrap();
    let mut page = PageRef::new(info.id, 0, 0);
    page.annotations = all_kinds().into_iter().map(Into::into).collect();
    let first = dir.join("1.pdf");
    save(&lib, &[page], &first);
    let before = annots(&first, 1);

    // Rouvrir, réenregistrer deux fois de suite.
    let mut path = first.clone();
    for round in 2..4 {
        let again = lib.open(&path).unwrap();
        let mut page = PageRef::new(again.id, 0, 0);
        page.annotations = lib.annotations(again.id).unwrap().remove(0);
        let next = dir.join(format!("{round}.pdf"));
        save(&lib, &[page], &next);
        path = next;
    }
    let after = annots(&path, 1);
    assert_eq!(after.len(), before.len(), "doublons ou pertes");
    // Chaque annotation est recopiée telle quelle, apparence comprise.
    let doc_before = Document::load(&first).unwrap();
    let doc_after = Document::load(&path).unwrap();
    for ((_, a), (_, b)) in before.iter().zip(&after) {
        for key in ["Subtype", "Rect", "C", "IC", "NM", "Contents", "DocumentsData"] {
            assert_eq!(a.get(key.as_bytes()).ok(), b.get(key.as_bytes()).ok(), "{} : {key}", subtype(a));
        }
        let ap = |doc: &Document, d: &Dictionary| {
            let n = d.get(b"AP").unwrap().as_dict().unwrap().get(b"N").unwrap().as_reference().unwrap();
            doc.get_object(n).unwrap().as_stream().unwrap().decompressed_content().unwrap_or_else(|_| doc.get_object(n).unwrap().as_stream().unwrap().content.clone())
        };
        assert_eq!(ap(&doc_before, a), ap(&doc_after, b), "{} : apparence changée", subtype(a));
    }
    // Le formulaire est toujours là.
    let id = lib.open(&path).unwrap().id;
    assert!(!lib.form_fields(id).unwrap().is_empty());
}

#[test]
fn deplacer_modifier_supprimer_puis_reenregistrer() {
    let dir = workdir("modifier");
    let mut lib = library();
    let info = lib.open(&fixture("trois-pages.pdf")).unwrap();
    let mut page = PageRef::new(info.id, 0, 0);
    page.annotations = all_kinds().into_iter().map(Into::into).collect();
    let first = dir.join("1.pdf");
    save(&lib, &[page], &first);

    let again = lib.open(&first).unwrap();
    let mut list = lib.annotations(again.id).unwrap().remove(0);
    // Le rectangle bleu bouge, l'ovale devient vert, la note disparaît.
    if let Annotation::Rect { rect, .. } = &mut list[1].annotation {
        rect.x += 100.0;
    }
    if let Annotation::Ellipse { stroke, .. } = &mut list[2].annotation {
        *stroke = Some(Color(0, 200, 0));
    }
    list.retain(|a| !matches!(a.annotation, Annotation::Note { .. }));
    let expected: Vec<Annotation> = list.iter().map(|a| a.annotation.clone()).collect();
    let mut page = PageRef::new(again.id, 0, 0);
    page.annotations = list;
    let second = dir.join("2.pdf");
    save(&lib, &[page], &second);

    assert_eq!(annots(&second, 1).len(), all_kinds().len() - 1);
    let third = lib.open(&second).unwrap();
    let back: Vec<Annotation> = lib.annotations(third.id).unwrap().remove(0).into_iter().map(|p| p.annotation).collect();
    assert_eq!(back.len(), expected.len());
    for a in &expected {
        assert!(back.iter().any(|b| close(a, b)), "manque : {a:?}");
    }
}

#[test]
fn pages_reordonnees_tournees_et_fusionnees() {
    let dir = workdir("fusion");
    let mut lib = library();
    let info = lib.open(&fixture("trois-pages.pdf")).unwrap();
    let mut first = PageRef::new(info.id, 1, 0);
    first.annotations = vec![all_kinds()[1].clone().into()];
    let annotated = dir.join("a.pdf");
    save(&lib, &[PageRef::new(info.id, 0, 0), first], &annotated);

    // Même PDF : pages inversées et tournées.
    let again = lib.open(&annotated).unwrap();
    let list = lib.annotations(again.id).unwrap();
    assert!(list[0].is_empty() && list[1].len() == 1);
    let mut moved = PageRef::new(again.id, 1, 90);
    moved.annotations = list[1].clone();
    let turned = dir.join("b.pdf");
    save(&lib, &[moved.clone(), PageRef::new(again.id, 0, 0)], &turned);
    assert_eq!(annots(&turned, 1).len(), 1);
    assert_eq!(annots(&turned, 2).len(), 0);
    let back = lib.open(&turned).unwrap();
    let rect = &lib.annotations(back.id).unwrap()[0][0].annotation;
    // Page tournée d'un quart de tour : le rectangle tourne avec elle.
    let Annotation::Rect { rect, .. } = rect else { panic!() };
    assert!((rect.w - 60.0).abs() < 0.01 && (rect.h - 120.0).abs() < 0.01, "{rect:?}");

    // Avec une page d'un autre PDF : le PDF est refait, les annotations suivent.
    let other = lib.open(&fixture("une-page.pdf")).unwrap();
    let merged = dir.join("c.pdf");
    save(&lib, &[PageRef::new(other.id, 0, 0), moved], &merged);
    assert_eq!(annots(&merged, 1).len(), 0);
    assert_eq!(annots(&merged, 2).len(), 1);
    assert_eq!(subtype(&annots(&merged, 2)[0].1), "Square");
}

/// Un PDF avec des annotations d'un autre logiciel : un rectangle commenté (bulle, auteur),
/// une zone de texte sans cadre, et un rectangle en pointillés (que l'appli ne sait pas refaire).
fn other_app_pdf(dest: &Path) {
    let mut doc = Document::load(fixture("une-page.pdf")).unwrap();
    let page = *doc.get_pages().get(&1).unwrap();
    let appearance = |doc: &mut Document, ops: &str| {
        doc.add_object(Stream::new(
            dictionary! { "Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(), 0.into(), 100.into(), 50.into()] },
            ops.as_bytes().to_vec(),
        ))
    };
    let square_ap = appearance(&mut doc, "0 0 1 RG 2 w 1 1 98 48 re S");
    let square = doc.new_object_id();
    let popup = doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Popup", "Rect" => vec![300.into(), 600.into(), 450.into(), 700.into()],
        "Parent" => Object::Reference(square), "Open" => false,
    });
    doc.objects.insert(
        square,
        Object::Dictionary(dictionary! {
            "Type" => "Annot", "Subtype" => "Square", "Rect" => vec![100.into(), 600.into(), 200.into(), 650.into()],
            "C" => vec![0.into(), 0.into(), 1.into()], "BS" => dictionary! { "W" => 2 },
            "T" => Object::string_literal("Marie"), "Contents" => Object::string_literal("A revoir"),
            "NM" => Object::string_literal("autre-1"), "Popup" => Object::Reference(popup),
            "AP" => dictionary! { "N" => Object::Reference(square_ap) }, "P" => Object::Reference(page),
        }),
    );
    let text_ap = appearance(&mut doc, "BT /Helv 12 Tf 0 0 TD (Bonjour) Tj ET");
    let text = doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "FreeText", "Rect" => vec![100.into(), 500.into(), 250.into(), 520.into()],
        "Contents" => Object::string_literal("Bonjour"), "DA" => Object::string_literal("/Helv 12 Tf 1 0 0 rg"),
        "BS" => dictionary! { "W" => 0 }, "AP" => dictionary! { "N" => Object::Reference(text_ap) },
    });
    let dashed_ap = appearance(&mut doc, "0 0 1 RG [3] 0 d 1 1 98 48 re S");
    let dashed = doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Square", "Rect" => vec![100.into(), 300.into(), 200.into(), 350.into()],
        "C" => vec![0.into(), 0.into(), 1.into()], "BS" => dictionary! { "W" => 1, "S" => "D", "D" => vec![3.into()] },
        "AP" => dictionary! { "N" => Object::Reference(dashed_ap) },
    });
    let refs = [square, popup, text, dashed].map(Object::Reference).to_vec();
    doc.get_dictionary_mut(page).unwrap().set("Annots", Object::Array(refs));
    doc.save(dest).unwrap();
}

#[test]
fn annotations_d_un_autre_logiciel() {
    let dir = workdir("autre");
    let source = dir.join("autre.pdf");
    other_app_pdf(&source);
    let mut lib = library();
    let info = lib.open(&source).unwrap();
    let list = lib.annotations(info.id).unwrap().remove(0);
    assert_eq!(list.len(), 2, "{list:?}");
    assert!(matches!(list[0].annotation, Annotation::Rect { stroke: Some(BLUE), .. }));
    assert!(matches!(&list[1].annotation, Annotation::Text { text: Some(t), color: RED, .. } if t == "Bonjour"));

    // Enregistré sans y toucher : tout est gardé tel quel, sans doublon.
    let mut page = PageRef::new(info.id, 0, 0);
    page.annotations = list.clone();
    let same = dir.join("pareil.pdf");
    save(&lib, &[page], &same);
    let kept = annots(&same, 1);
    let kinds: Vec<String> = kept.iter().map(|(_, d)| subtype(d)).collect();
    assert_eq!(kinds.iter().filter(|k| *k == "Square").count(), 2, "{kinds:?}");
    assert_eq!(kinds.iter().filter(|k| *k == "FreeText").count(), 1, "{kinds:?}");
    assert_eq!(kinds.iter().filter(|k| *k == "Popup").count(), 1, "{kinds:?}");
    let square = kept.iter().find(|(_, d)| d.get(b"T").is_ok()).unwrap();
    assert!(square.1.get(b"DocumentsData").is_err(), "pas réécrite");
    let popup = square.1.get(b"Popup").unwrap().as_reference().unwrap();
    let doc = Document::load(&same).unwrap();
    assert_eq!(doc.get_dictionary(popup).unwrap().get(b"Parent").unwrap().as_reference().unwrap(), square.0);

    // Déplacé : redessiné par l'appli, mais l'auteur, le commentaire et la bulle restent.
    let mut list = list;
    if let Annotation::Rect { rect, .. } = &mut list[0].annotation {
        rect.y += 50.0;
    }
    let again = lib.open(&same).unwrap();
    let mut origins = lib.annotations(again.id).unwrap().remove(0);
    for (o, changed) in origins.iter_mut().zip(&list) {
        o.annotation = changed.annotation.clone();
    }
    let mut page = PageRef::new(again.id, 0, 0);
    page.annotations = origins;
    let moved = dir.join("deplace.pdf");
    save(&lib, &[page], &moved);
    let kept = annots(&moved, 1);
    assert_eq!(kept.len(), 4);
    let square = kept.iter().find(|(_, d)| d.get(b"T").is_ok()).unwrap();
    assert!(square.1.get(b"DocumentsData").is_ok(), "réécrite");
    assert_eq!(square.1.get(b"Contents").unwrap().as_str().unwrap(), b"A revoir");
    assert_eq!(square.1.get(b"NM").unwrap().as_str().unwrap(), b"autre-1");
    let popup = square.1.get(b"Popup").unwrap().as_reference().unwrap();
    let doc = Document::load(&moved).unwrap();
    assert_eq!(doc.get_dictionary(popup).unwrap().get(b"Parent").unwrap().as_reference().unwrap(), square.0);
    let rect = square.1.get(b"Rect").unwrap().as_array().unwrap();
    let bottom = rect[1].as_float().unwrap();
    assert!((bottom - (600.0 - 50.0 * 0.75)).abs() < 2.0, "{rect:?}");
}

#[test]
fn annotations_d_une_ancienne_version_de_l_appli() {
    // Avant cette version, l'appli ne rangeait pas sa description dans l'annotation.
    let dir = workdir("ancienne");
    let mut lib = library();
    let info = lib.open(&fixture("trois-pages.pdf")).unwrap();
    let mut page = PageRef::new(info.id, 0, 0);
    page.annotations = all_kinds().into_iter().map(Into::into).collect();
    let saved = dir.join("neuve.pdf");
    save(&lib, &[page], &saved);
    let mut doc = Document::load(&saved).unwrap();
    for o in doc.objects.values_mut() {
        if let Object::Dictionary(d) = o {
            d.remove(b"DocumentsData");
            d.remove(b"Q");
        }
    }
    let old = dir.join("ancienne.pdf");
    doc.save(&old).unwrap();

    let again = lib.open(&old).unwrap();
    let back: Vec<Annotation> = lib.annotations(again.id).unwrap().remove(0).into_iter().map(|p| p.annotation).collect();
    // Tout revient : même les formes tournées ou dessinées (des polygones), retrouvées d'après
    // leurs sommets ; les zones de texte avec leur texte, leur police et leur soulignement.
    assert_eq!(back.len(), all_kinds().len(), "{back:?}");
    for a in all_kinds() {
        let found = match &a {
            Annotation::Text { rect, font, bold, size, underline, .. } => back.iter().any(|b| {
                matches!(b, Annotation::Text { rect: r, font: f, bold: bb, size: s, text: Some(t), underline: u, .. }
                    if (r.x - rect.x).abs() < 0.01 && f == font && bb == bold && (s - size).abs() < 0.01 && u == underline
                        && t == "Vu le « devis » (été)")
            }),
            Annotation::Rect { rect, angle, shape, width, .. } => back.iter().any(|b| {
                matches!(b, Annotation::Rect { rect: r, angle: g, shape: s, width: w, .. }
                    if s == shape && (g - angle).abs() < 0.05 && (w - width).abs() < 0.01
                        && (r.x - rect.x).abs() < 0.05 && (r.y - rect.y).abs() < 0.05
                        && (r.w - rect.w).abs() < 0.05 && (r.h - rect.h).abs() < 0.05)
            }),
            _ => back.iter().any(|b| close(&a, b)),
        };
        assert!(found, "manque : {a:?}\n{back:?}");
    }
}

#[test]
fn placed_garde_son_origine_en_json() {
    let placed = Placed { origin: Some("12-0".into()), annotation: all_kinds()[1].clone() };
    let json = serde_json::to_string(&placed).unwrap();
    assert!(json.contains("\"origin\":\"12-0\"") && json.contains("\"kind\":\"rect\""), "{json}");
    let back: Placed = serde_json::from_str(&json).unwrap();
    assert_eq!(back, placed);
}

#[test]
fn page_tournee_puis_rouverte() {
    let dir = workdir("tournee");
    let mut lib = library();
    let info = lib.open(&fixture("trois-pages.pdf")).unwrap();
    let kinds = all_kinds();
    let (star, text) = (kinds[4].clone(), kinds[7].clone());
    let mut page = PageRef::new(info.id, 0, 90);
    page.annotations = vec![star.into(), text.into()];
    let dest = dir.join("tournee.pdf");
    save(&lib, &[page], &dest);

    let again = lib.open(&dest).unwrap();
    let list = lib.annotations(again.id).unwrap().remove(0);
    // L'étoile tourne avec la page ; le texte, qu'on ne sait pas montrer penché, reste dans la page.
    assert_eq!(list.len(), 1, "{list:?}");
    let Annotation::Rect { rect, angle, shape: Shape::Star, .. } = list[0].annotation else { panic!() };
    assert!((angle - 90.0).abs() < 0.01 && (rect.w - 90.0).abs() < 0.01, "{angle} {rect:?}");
    // Centre de l'étoile : (345 ; 530) sur la page droite, donc (H − 530 ; 345) une fois tournée.
    let h = info.pages[0].height;
    assert!((rect.x + rect.w / 2.0 - (h - 530.0)).abs() < 0.05 && (rect.y + rect.h / 2.0 - 345.0).abs() < 0.05, "{rect:?}");
    assert_eq!(annots(&dest, 1).len(), 2);
    // Réenregistrée telle quelle : toujours deux annotations.
    let mut page = PageRef::new(again.id, 0, 0);
    page.annotations = list;
    let twice = dir.join("deux.pdf");
    save(&lib, &[page], &twice);
    assert_eq!(annots(&twice, 1).len(), 2);
}

#[test]
fn photo_fusionnee_dans_un_pdf() {
    let dir = workdir("photo");
    let mut lib = library();
    let photo = lib.open(&fixture("photo.png")).unwrap();
    let size = photo.pages[0];
    let mut page = PageRef::new(photo.id, 0, 0);
    page.annotations.push(
        Annotation::Rect {
            rect: Rect { x: 0.25 * size.width, y: 0.5 * size.height, w: 0.5 * size.width, h: 0.25 * size.height },
            stroke: Some(RED),
            fill: None,
            width: 2.0,
            angle: 0.0,
            shape: Shape::Plain,
        }
        .into(),
    );
    let dest = dir.join("photo.pdf");
    save(&lib, &[page], &dest);
    let again = lib.open(&dest).unwrap();
    let pdf = again.pages[0];
    let list = lib.annotations(again.id).unwrap().remove(0);
    let Annotation::Rect { rect, .. } = list[0].annotation else { panic!() };
    // Mêmes proportions, dans les unités de la page du PDF.
    assert!((rect.x / pdf.width - 0.25).abs() < 0.001 && (rect.h / pdf.height - 0.25).abs() < 0.001, "{rect:?} {pdf:?}");
}

#[test]
fn formes_tournees_d_une_ancienne_version() {
    let dir = workdir("polygones");
    let mut lib = library();
    let info = lib.open(&fixture("page-pivotee.pdf")).unwrap();
    let shapes = [
        Annotation::Ellipse { rect: Rect { x: 200.0, y: 200.0, w: 80.0, h: 40.0 }, stroke: Some(RED), fill: Some(BLUE), width: 3.0, angle: -45.0 },
        Annotation::Rect { rect: Rect { x: 100.0, y: 100.0, w: 90.0, h: 60.0 }, stroke: None, fill: Some(BLUE), width: 0.0, angle: 120.0, shape: Shape::Bubble },
        Annotation::Rect { rect: Rect { x: 300.0, y: 300.0, w: 50.0, h: 70.0 }, stroke: Some(RED), fill: None, width: 1.0, angle: 10.5, shape: Shape::Hexagon },
        Annotation::Rect { rect: Rect { x: 50.0, y: 400.0, w: 60.0, h: 60.0 }, stroke: Some(RED), fill: None, width: 2.0, angle: 0.0, shape: Shape::Rounded },
    ];
    let mut page = PageRef::new(info.id, 0, 0);
    page.annotations = shapes.iter().cloned().map(Into::into).collect();
    let saved = dir.join("neuve.pdf");
    save(&lib, &[page], &saved);
    let mut doc = Document::load(&saved).unwrap();
    for o in doc.objects.values_mut() {
        if let Object::Dictionary(d) = o {
            d.remove(b"DocumentsData");
        }
    }
    let old = dir.join("ancienne.pdf");
    doc.save(&old).unwrap();
    let again = lib.open(&old).unwrap();
    let back: Vec<Annotation> = lib.annotations(again.id).unwrap().remove(0).into_iter().map(|p| p.annotation).collect();
    assert_eq!(back.len(), shapes.len());
    for (a, b) in shapes.iter().zip(&back) {
        let (ra, ga, rb, gb) = match (a, b) {
            (Annotation::Ellipse { rect: ra, angle: ga, .. }, Annotation::Ellipse { rect: rb, angle: gb, .. }) => (ra, ga, rb, gb),
            (Annotation::Rect { rect: ra, angle: ga, shape: sa, .. }, Annotation::Rect { rect: rb, angle: gb, shape: sb, .. }) if sa == sb => (ra, ga, rb, gb),
            _ => panic!("{a:?}\n≠ {b:?}"),
        };
        // Un ovale tourné de −45° ou de 135°, c'est le même.
        let turn = (ga - gb).rem_euclid(if matches!(a, Annotation::Ellipse { .. }) { 180.0 } else { 360.0 });
        assert!(turn.min(360.0 - turn) < 0.05 || (turn - 180.0).abs() < 0.05 && matches!(a, Annotation::Ellipse { .. }), "{a:?}\n≠ {b:?}");
        for (x, y) in [(ra.x, rb.x), (ra.y, rb.y), (ra.w, rb.w), (ra.h, rb.h)] {
            assert!((x - y).abs() < 0.1, "{a:?}\n≠ {b:?}");
        }
    }
}
