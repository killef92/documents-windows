//! Fichiers abîmés ou piégés : l'appli doit refuser proprement (un message d'erreur), jamais
//! planter ni se bloquer. Les fichiers sont fabriqués ici, à partir des exemples de `fixtures/`
//! (coupés, ou modifiés au hasard octet par octet), pour ne pas stocker de fichiers dangereux.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use documents_core::{Error, Library, PageRef, Rotation};

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

/// Un dossier de travail vide, propre à chaque test.
fn workdir(nom: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("documents-abimes-{}-{nom}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Petit générateur pseudo-aléatoire (xorshift) : les mêmes fichiers à chaque lancement.
struct Hasard(u64);

impl Hasard {
    fn suivant(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn jusqua(&mut self, n: usize) -> usize {
        (self.suivant() % n as u64) as usize
    }
}

/// Une copie abîmée : quelques octets remplacés, ou un morceau coupé.
fn abime(original: &[u8], hasard: &mut Hasard) -> Vec<u8> {
    let mut bytes = original.to_vec();
    match hasard.jusqua(3) {
        0 => {
            for _ in 0..1 + hasard.jusqua(8) {
                let i = hasard.jusqua(bytes.len());
                bytes[i] = hasard.suivant() as u8;
            }
        }
        1 => bytes.truncate(1 + hasard.jusqua(bytes.len())),
        _ => {
            let debut = hasard.jusqua(bytes.len());
            let fin = (debut + 1 + hasard.jusqua(64)).min(bytes.len());
            bytes.drain(debut..fin);
        }
    }
    bytes
}

/// Ouvre le fichier et fait tout ce que l'appli fait avec : afficher, rechercher, lire les
/// champs, enregistrer. Les erreurs sont normales ; seul un plantage fait échouer le test.
fn utilise(lib: &mut Library, path: &Path, dest: &Path) {
    let Ok(info) = lib.open(path) else { return };
    for page in 0..info.pages.len().min(3) {
        let _ = lib.render(info.id, page, 200, Rotation::default());
        let _ = lib.page_text(info.id, page);
    }
    let _ = lib.details(info.id);
    let _ = lib.form_fields(info.id);
    let _ = lib.search("a");
    let pages: Vec<PageRef> = (0..info.pages.len().min(3)).map(|p| PageRef::new(info.id, p, 0)).collect();
    if !pages.is_empty() {
        let _ = lib.save_pdf(&pages, dest);
    }
    lib.close(info.id);
}

/// Pour chaque exemple : `n` copies abîmées, chacune ouverte et utilisée sans plantage.
/// DOCUMENTS_ESSAIS=2000 cargo test --release … : beaucoup plus d'essais, pour chercher à fond.
fn resiste(exemples: &[&str], n: usize, graine: u64) {
    let n = std::env::var("DOCUMENTS_ESSAIS").ok().and_then(|v| v.parse().ok()).unwrap_or(n);
    let dir = workdir(&format!("hasard-{graine}"));
    let mut lib = library();
    let mut hasard = Hasard(graine);
    for nom in exemples {
        let original = std::fs::read(fixture(nom)).unwrap();
        let extension = Path::new(nom).extension().unwrap().to_string_lossy().into_owned();
        for essai in 0..n {
            let bytes = abime(&original, &mut hasard);
            let path = dir.join(format!("abime.{extension}"));
            std::fs::write(&path, &bytes).unwrap();
            let debut = Instant::now();
            let resultat = catch_unwind(AssertUnwindSafe(|| utilise(&mut lib, &path, &dir.join("sortie.pdf"))));
            if resultat.is_err() {
                let garde = dir.join(format!("plantage-{nom}-{essai}.{extension}"));
                std::fs::write(&garde, &bytes).unwrap();
                panic!("{nom}, essai {essai} : plantage (fichier gardé : {})", garde.display());
            }
            assert!(debut.elapsed() < Duration::from_secs(10), "{nom}, essai {essai} : trop long");
        }
    }
}

#[test]
fn les_pdf_abimes_ne_font_pas_planter() {
    resiste(&["trois-pages.pdf", "formulaire.pdf", "cerfa-test.pdf", "page-pivotee.pdf"], 60, 0x5eed_0001);
}

#[test]
fn les_images_abimees_ne_font_pas_planter() {
    resiste(&["photo.png", "photo-pivotee.jpg", "photo.webp", "anime.gif"], 150, 0x5eed_0002);
}

#[test]
fn un_fichier_vide_est_refuse() {
    let dir = workdir("vide");
    for nom in ["vide.pdf", "vide.png"] {
        let path = dir.join(nom);
        std::fs::write(&path, b"").unwrap();
        assert!(matches!(library().open(&path), Err(Error::UnsupportedFormat)), "{nom}");
    }
}

#[test]
fn du_charabia_deguise_en_pdf_est_refuse() {
    let dir = workdir("charabia");
    let mut hasard = Hasard(42);
    let mut bytes = b"%PDF-1.7\n".to_vec();
    bytes.extend((0..4000).map(|_| hasard.suivant() as u8));
    let path = dir.join("charabia.pdf");
    std::fs::write(&path, &bytes).unwrap();
    assert!(library().open(&path).is_err());
}

/// « Bombe » : une image minuscule qui annonce 60 000 × 60 000 pixels (14 Go une fois décodée).
/// Elle doit être refusée tout de suite, sans essayer de réserver toute cette mémoire.
#[test]
fn une_image_geante_est_refusee_sans_bloquer() {
    fn chunk(out: &mut Vec<u8>, kind: &[u8], data: &[u8]) {
        out.extend((data.len() as u32).to_be_bytes());
        let start = out.len();
        out.extend(kind);
        out.extend(data);
        let crc = crc32(&out[start..]);
        out.extend(crc.to_be_bytes());
    }
    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = !0u32;
        for &b in bytes {
            crc ^= b as u32;
            for _ in 0..8 {
                crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
            }
        }
        !crc
    }
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::new();
    ihdr.extend(60_000u32.to_be_bytes());
    ihdr.extend(60_000u32.to_be_bytes());
    ihdr.extend([8, 6, 0, 0, 0]); // 8 bits, RGBA
    chunk(&mut png, b"IHDR", &ihdr);
    chunk(&mut png, b"IDAT", &[0x78, 0x9c, 0x03, 0x00, 0x00, 0x00, 0x00, 0x01]);
    chunk(&mut png, b"IEND", &[]);

    let dir = workdir("bombe");
    let path = dir.join("bombe.png");
    std::fs::write(&path, &png).unwrap();
    let debut = Instant::now();
    assert!(matches!(library().open(&path), Err(Error::BrokenImage(_))));
    assert!(debut.elapsed() < Duration::from_secs(2));
}
