//! Photos HEIC (celles des téléphones récents), lues par la bibliothèque libheif.
//!
//! libheif (et libde265, qui décode les images) sont sous licence LGPL : elles sont livrées
//! en fichiers séparés à côté de l'appli et chargées à la demande, comme PDFium. On peut donc
//! les remplacer par une autre version sans toucher à l'appli. Sous Linux (développement),
//! c'est la libheif du système qui sert.

use std::ffi::{c_char, c_int, c_void, CStr};
use std::path::PathBuf;
use std::sync::OnceLock;

use image::{DynamicImage, RgbImage, RgbaImage};
use libloading::Library;

use crate::{Error, Result};

/// Où chercher libheif (dossiers de l'appli), donnés une fois au démarrage.
static DIRS: OnceLock<Vec<PathBuf>> = OnceLock::new();
static API: OnceLock<std::result::Result<Api, String>> = OnceLock::new();

pub(crate) fn set_dirs(dirs: Vec<PathBuf>) {
    let _ = DIRS.set(dirs);
}

#[repr(C)]
#[derive(Clone, Copy)]
struct HeifError {
    code: c_int,
    subcode: c_int,
    message: *const c_char,
}

// Valeurs des énumérations de heif.h.
const COLORSPACE_RGB: c_int = 1;
const CHROMA_INTERLEAVED_RGB: c_int = 10;
const CHROMA_INTERLEAVED_RGBA: c_int = 11;
const CHANNEL_INTERLEAVED: c_int = 10;

type Ctx = c_void;
type Handle = c_void;
type Img = c_void;

struct Api {
    _lib: Library,
    context_alloc: unsafe extern "C" fn() -> *mut Ctx,
    context_free: unsafe extern "C" fn(*mut Ctx),
    read_from_memory: unsafe extern "C" fn(*mut Ctx, *const c_void, usize, *const c_void) -> HeifError,
    primary_handle: unsafe extern "C" fn(*mut Ctx, *mut *mut Handle) -> HeifError,
    has_alpha: unsafe extern "C" fn(*const Handle) -> c_int,
    decode: unsafe extern "C" fn(*const Handle, *mut *mut Img, c_int, c_int, *const c_void) -> HeifError,
    width: unsafe extern "C" fn(*const Img, c_int) -> c_int,
    height: unsafe extern "C" fn(*const Img, c_int) -> c_int,
    plane: unsafe extern "C" fn(*const Img, c_int, *mut c_int) -> *const u8,
    image_release: unsafe extern "C" fn(*const Img),
    handle_release: unsafe extern "C" fn(*const Handle),
}

// Les fonctions de libheif peuvent être appelées depuis plusieurs fils (contextes séparés).
unsafe impl Send for Api {}
unsafe impl Sync for Api {}

/// Noms possibles du fichier de libheif.
#[cfg(windows)]
const NAMES: &[&str] = &["heif.dll", "libheif.dll"];
#[cfg(not(windows))]
const NAMES: &[&str] = &["libheif.so.1", "libheif.so"];

fn open_library() -> std::result::Result<Library, String> {
    let dirs = DIRS.get().cloned().unwrap_or_default();
    let mut last = String::from("libheif not found");
    for dir in &dirs {
        for name in NAMES {
            let path = dir.join(name);
            if path.is_file() {
                match load(&path) {
                    Ok(lib) => return Ok(lib),
                    Err(e) => last = e.to_string(),
                }
            }
        }
    }
    // Sinon, celle du système (Linux) ou à côté du programme.
    for name in NAMES {
        // SAFETY : libheif n'exécute rien de particulier au chargement.
        match unsafe { Library::new(name) } {
            Ok(lib) => return Ok(lib),
            Err(e) => last = e.to_string(),
        }
    }
    Err(last)
}

/// Charge la bibliothèque ; sous Windows, ses propres dépendances (libde265) sont cherchées
/// dans son dossier.
fn load(path: &std::path::Path) -> std::result::Result<Library, libloading::Error> {
    #[cfg(windows)]
    {
        use libloading::os::windows::{Library as WinLibrary, LOAD_WITH_ALTERED_SEARCH_PATH};
        // SAFETY : libheif n'exécute rien de particulier au chargement.
        unsafe { WinLibrary::load_with_flags(path, LOAD_WITH_ALTERED_SEARCH_PATH) }.map(Library::from)
    }
    #[cfg(not(windows))]
    {
        // SAFETY : libheif n'exécute rien de particulier au chargement.
        unsafe { Library::new(path) }
    }
}

fn api() -> Result<&'static Api> {
    API.get_or_init(|| {
        let lib = open_library()?;
        // SAFETY : les signatures suivent heif.h (API C stable de libheif depuis la 1.3).
        unsafe {
            if let Ok(init) = lib.get::<unsafe extern "C" fn(*const c_void) -> HeifError>(b"heif_init\0") {
                init(std::ptr::null());
            }
            Ok(Api {
                context_alloc: sym(&lib, "heif_context_alloc")?,
                context_free: sym(&lib, "heif_context_free")?,
                read_from_memory: sym(&lib, "heif_context_read_from_memory_without_copy")?,
                primary_handle: sym(&lib, "heif_context_get_primary_image_handle")?,
                has_alpha: sym(&lib, "heif_image_handle_has_alpha_channel")?,
                decode: sym(&lib, "heif_decode_image")?,
                width: sym(&lib, "heif_image_get_width")?,
                height: sym(&lib, "heif_image_get_height")?,
                plane: sym(&lib, "heif_image_get_plane_readonly")?,
                image_release: sym(&lib, "heif_image_release")?,
                handle_release: sym(&lib, "heif_image_handle_release")?,
                _lib: lib,
            })
        }
    })
    .as_ref()
    .map_err(|e| Error::HeicUnavailable(e.clone()))
}

/// Une fonction de la bibliothèque, par son nom.
///
/// SAFETY : `T` doit être la signature exacte de la fonction en C.
unsafe fn sym<T: Copy>(lib: &Library, name: &str) -> std::result::Result<T, String> {
    let name = format!("{name}\0");
    unsafe { lib.get::<T>(name.as_bytes()) }.map(|s| *s).map_err(|e| e.to_string())
}

fn check(e: HeifError) -> Result<()> {
    if e.code == 0 {
        return Ok(());
    }
    let message = if e.message.is_null() {
        format!("error {}", e.code)
    } else {
        // SAFETY : libheif garantit un texte terminé par un zéro, valable tant que la bibliothèque l'est.
        unsafe { CStr::from_ptr(e.message) }.to_string_lossy().into_owned()
    };
    Err(Error::BrokenImage(message))
}

/// Libère ce que libheif a alloué, même en cas d'erreur.
struct Guard<'a> {
    api: &'a Api,
    ctx: *mut Ctx,
    handle: *mut Handle,
    image: *mut Img,
}

impl Drop for Guard<'_> {
    fn drop(&mut self) {
        // SAFETY : chaque pointeur vient de libheif et n'est libéré qu'une fois, ici.
        unsafe {
            if !self.image.is_null() {
                (self.api.image_release)(self.image);
            }
            if !self.handle.is_null() {
                (self.api.handle_release)(self.handle);
            }
            if !self.ctx.is_null() {
                (self.api.context_free)(self.ctx);
            }
        }
    }
}

/// Décode l'image principale d'un fichier HEIC, déjà redressée (libheif applique la rotation
/// enregistrée par le téléphone).
pub(crate) fn decode(bytes: &[u8]) -> Result<DynamicImage> {
    let api = api()?;
    // SAFETY : appels conformes à heif.h ; `bytes` vit plus longtemps que le contexte (Guard).
    unsafe {
        let mut g = Guard { api, ctx: (api.context_alloc)(), handle: std::ptr::null_mut(), image: std::ptr::null_mut() };
        if g.ctx.is_null() {
            return Err(Error::BrokenImage("out of memory".into()));
        }
        check((api.read_from_memory)(g.ctx, bytes.as_ptr().cast(), bytes.len(), std::ptr::null()))?;
        check((api.primary_handle)(g.ctx, &mut g.handle))?;
        let alpha = (api.has_alpha)(g.handle) != 0;
        let chroma = if alpha { CHROMA_INTERLEAVED_RGBA } else { CHROMA_INTERLEAVED_RGB };
        check((api.decode)(g.handle, &mut g.image, COLORSPACE_RGB, chroma, std::ptr::null()))?;
        let w = (api.width)(g.image, CHANNEL_INTERLEAVED);
        let h = (api.height)(g.image, CHANNEL_INTERLEAVED);
        let mut stride: c_int = 0;
        let plane = (api.plane)(g.image, CHANNEL_INTERLEAVED, &mut stride);
        if plane.is_null() || w <= 0 || h <= 0 || stride <= 0 {
            return Err(Error::BrokenImage("empty image".into()));
        }
        let (w, h, stride) = (w as usize, h as usize, stride as usize);
        let channels = if alpha { 4 } else { 3 };
        if stride < w * channels {
            return Err(Error::BrokenImage("invalid image layout".into()));
        }
        let mut pixels = Vec::with_capacity(w * h * channels);
        for y in 0..h {
            let row = std::slice::from_raw_parts(plane.add(y * stride), w * channels);
            pixels.extend_from_slice(row);
        }
        let broken = || Error::BrokenImage("invalid image size".into());
        Ok(if alpha {
            DynamicImage::ImageRgba8(RgbaImage::from_raw(w as u32, h as u32, pixels).ok_or_else(broken)?)
        } else {
            DynamicImage::ImageRgb8(RgbImage::from_raw(w as u32, h as u32, pixels).ok_or_else(broken)?)
        })
    }
}

/// libheif est-elle là (et peut-elle décoder) ? Pour les tests et le diagnostic.
pub fn available() -> bool {
    api().is_ok()
}
