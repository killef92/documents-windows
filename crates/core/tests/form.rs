use std::path::{Path, PathBuf};

use documents_core::form::{DocFill, FieldFill, FieldKind, FillValue, TextLine};
use documents_core::{Library, PageRef, Rotation};

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
    let dir = std::env::temp_dir().join(format!("documents-form-{}-{nom}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn text(id: &str, value: &str, size: f32) -> FieldFill {
    let lines = value
        .split('\n')
        .map(|l| TextLine { text: l.into(), width: l.chars().count() as f32 * size * 0.5, chars: vec![size * 0.6; l.chars().count()] })
        .collect();
    FieldFill { id: id.into(), value: FillValue::Text { text: value.into(), lines, size } }
}

#[test]
fn lit_les_champs_du_formulaire() {
    let mut lib = library();
    let id = lib.open(&fixture("cerfa-test.pdf")).unwrap().id;
    let fields = lib.form_fields(id).unwrap();
    let get = |name: &str| fields.iter().find(|f| f.name == name).unwrap_or_else(|| panic!("{name} manque"));

    let nom = get("nom");
    assert_eq!((nom.kind, nom.page), (FieldKind::Text, 0));
    // x = 120 pt, haut = 842 - 790 pt ; 1 pt = 4/3 unité.
    assert!((nom.rect.x - 160.0).abs() < 1.0 && (nom.rect.y - 69.2).abs() < 1.0, "{:?}", nom.rect);
    assert!((nom.rect.w - 333.3).abs() < 1.0);

    let cp = get("cp");
    assert!(cp.comb && cp.max_len == Some(5));
    assert!(cp.courier);
    assert!(get("obs").multiline);
    assert_eq!((get("certifie").kind, get("certifie").on.as_str()), (FieldKind::Checkbox, "Yes"));
    let radios: Vec<_> = fields.iter().filter(|f| f.name == "statut").collect();
    assert_eq!(radios.len(), 2);
    assert!(radios.iter().all(|r| r.kind == FieldKind::Radio && r.id == radios[0].id));
    assert_eq!(radios.iter().map(|r| r.on.as_str()).collect::<Vec<_>>(), ["particulier", "entreprise"]);
    let dep = get("dep");
    assert_eq!((dep.kind, dep.value.as_str()), (FieldKind::Choice, "Rhône"));
    assert_eq!(dep.options, ["Ain", "Isère", "Rhône", "Savoie"]);
    assert!(get("ref").read_only && get("ref").value == "REF-001");
    assert_eq!(get("ville").page, 1);

    let photo = lib.open(&fixture("photo.png")).unwrap().id;
    assert!(lib.form_fields(photo).unwrap().is_empty());
}

fn fills(lib: &Library, doc: u32) -> DocFill {
    let fields = lib.form_fields(doc).unwrap();
    let id = |name: &str| fields.iter().find(|f| f.name == name).unwrap().id.clone();
    DocFill {
        doc,
        fields: vec![
            text(&id("nom"), "Kilian Lefèvre", 11.0),
            text(&id("cp"), "69003", 12.0),
            text(&id("obs"), "Première ligne\nDeuxième ligne", 10.0),
            FieldFill { id: id("certifie"), value: FillValue::Check { on: true } },
            FieldFill { id: id("statut"), value: FillValue::Radio { value: Some("entreprise".into()) } },
            text(&id("dep"), "Isère", 11.0),
            text(&id("ville"), "Lyon", 11.0),
        ],
    }
}

/// Une zone de la page (en proportions) contient-elle de l'encre sombre ?
fn has_ink(lib: &mut Library, path: &Path, page: usize, (x0, y0, x1, y1): (f32, f32, f32, f32)) -> bool {
    let id = lib.open(path).unwrap().id;
    let img = lib.render(id, page, 600, Rotation::R0).unwrap();
    lib.close(id);
    let img = image::load_from_memory(&img.bytes).unwrap().into_rgb8();
    let (w, h) = (img.width() as f32, img.height() as f32);
    let mut dark = 0;
    for y in (y0 * h) as u32..(y1 * h) as u32 {
        for x in (x0 * w) as u32..(x1 * w) as u32 {
            let p = img.get_pixel(x, y).0;
            if p.iter().all(|&c| c < 90) {
                dark += 1;
            }
        }
    }
    dark > 10
}

#[test]
fn remplit_et_enregistre_le_formulaire() {
    let dir = workdir("remplir");
    let mut lib = library();
    let info = lib.open(&fixture("cerfa-test.pdf")).unwrap();
    let dest = dir.join("rempli.pdf");
    // Zone du champ « nom » (en proportions de la page) : vide avant.
    let nom_zone = (125.0 / 595.0, 1.0 - 789.0 / 842.0, 360.0 / 595.0, 1.0 - 771.0 / 842.0);
    assert!(!has_ink(&mut lib, &fixture("cerfa-test.pdf"), 0, nom_zone));

    let pages = [PageRef::new(info.id, 0, 0), PageRef::new(info.id, 1, 0)];
    lib.save_pdf_filled(&pages, &[fills(&lib, info.id)], &dest).unwrap();

    let again = lib.open(&dest).unwrap().id;
    let fields = lib.form_fields(again).unwrap();
    let value = |name: &str| fields.iter().find(|f| f.name == name).unwrap().value.clone();
    assert_eq!(value("nom"), "Kilian Lefèvre");
    assert_eq!(value("cp"), "69003");
    assert_eq!(value("obs"), "Première ligne\nDeuxième ligne");
    assert_eq!(value("certifie"), "Yes");
    assert_eq!(value("statut"), "entreprise");
    assert_eq!(value("dep"), "Isère");
    assert_eq!(value("ref"), "REF-001", "le champ non modifié garde sa valeur");
    lib.close(again);
    // Le texte s'affiche bien (apparence redessinée).
    assert!(has_ink(&mut lib, &dest, 0, nom_zone));
    // L'original n'a pas bougé.
    assert!(!has_ink(&mut lib, &fixture("cerfa-test.pdf"), 0, nom_zone));
}

#[test]
fn formulaire_rempli_puis_pages_reordonnees_ou_fusionnees() {
    let dir = workdir("fusion");
    let mut lib = library();
    let info = lib.open(&fixture("cerfa-test.pdf")).unwrap();
    let photo = lib.open(&fixture("photo.png")).unwrap().id;
    // Page 2 d'abord, puis la page 1, puis une photo : le PDF est reconstruit.
    let pages = [PageRef::new(info.id, 1, 0), PageRef::new(info.id, 0, 0), PageRef::new(photo, 0, 0)];
    let dest = dir.join("fusion.pdf");
    lib.save_pdf_filled(&pages, &[fills(&lib, info.id)], &dest).unwrap();
    let nom_zone = (125.0 / 595.0, 1.0 - 789.0 / 842.0, 360.0 / 595.0, 1.0 - 771.0 / 842.0);
    assert!(has_ink(&mut lib, &dest, 1, nom_zone), "le nom rempli apparaît sur la page déplacée");
    let ville_zone = (125.0 / 595.0, 1.0 - 759.0 / 842.0, 300.0 / 595.0, 1.0 - 741.0 / 842.0);
    assert!(has_ink(&mut lib, &dest, 0, ville_zone));
}

#[test]
fn page_rendue_sans_les_champs_pour_les_remplir_a_l_ecran() {
    let mut lib = library();
    let id = lib.open(&fixture("cerfa-test.pdf")).unwrap().id;
    // Le champ en lecture seule « REF-001 » : dessiné avec les champs, absent sans.
    let zone = (152.0 / 595.0, 1.0 - 449.0 / 842.0, 298.0 / 595.0, 1.0 - 431.0 / 842.0);
    let ink = |forms: bool| {
        let img = lib.render_with(id, 0, 600, Rotation::R0, forms).unwrap();
        let img = image::load_from_memory(&img.bytes).unwrap().into_rgb8();
        let (w, h) = (img.width() as f32, img.height() as f32);
        let mut dark = 0;
        for y in (zone.1 * h) as u32..(zone.3 * h) as u32 {
            for x in (zone.0 * w) as u32..(zone.2 * w) as u32 {
                if img.get_pixel(x, y).0.iter().all(|&c| c < 90) {
                    dark += 1;
                }
            }
        }
        dark
    };
    assert!(ink(true) > 10);
    assert!(ink(false) < 5);
}
