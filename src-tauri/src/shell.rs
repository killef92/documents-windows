//! Liens avec Windows : se déclarer comme appli pour les PDF et les images (applis par
//! défaut, « Ouvrir avec »), et copier des fichiers dans le presse-papier.

use std::path::PathBuf;
use std::sync::Mutex;

use windows::core::{w, HSTRING, PCWSTR};

use crate::i18n::{tr, Text};
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardSequenceNumber, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Ole::CF_HDROP;
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteKeyValueW, RegDeleteTreeW, RegOpenKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ,
    KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ,
};
use windows::Win32::UI::Shell::{ShellExecuteW, SHChangeNotify, DROPFILES, SHCNE_ASSOCCHANGED, SHCNF_IDLIST};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

/// Le nom sous lequel Windows liste l'appli dans les applis par défaut.
const APP_NAME: &str = "Documents";
const CAPABILITIES: &str = r"Software\Documents\Capabilities";

/// Écrit une valeur texte dans le registre de l'utilisateur (la clé est créée au besoin).
fn set(path: &str, name: Option<&str>, value: &str) -> Result<(), String> {
    unsafe {
        let mut key = HKEY::default();
        RegCreateKeyExW(HKEY_CURRENT_USER, &HSTRING::from(path), None, None, REG_OPTION_NON_VOLATILE, KEY_WRITE, None, &mut key, None)
            .ok()
            .map_err(|e| e.to_string())?;
        let data: Vec<u16> = value.encode_utf16().chain(Some(0)).collect();
        let bytes = std::slice::from_raw_parts(data.as_ptr().cast::<u8>(), data.len() * 2);
        let name = name.map(HSTRING::from);
        let r = RegSetValueExW(key, name.as_ref().map_or(PCWSTR::null(), |n| PCWSTR(n.as_ptr())), None, REG_SZ, Some(bytes));
        let _ = RegCloseKey(key);
        r.ok().map_err(|e| e.to_string())
    }
}

/// Chaque extension que l'appli sait ouvrir, avec son type de fichier.
const EXTENSIONS: &[(&str, &str)] = &[
    (".pdf", "Apercu.PDF"),
    (".jpg", "Apercu.JPEG"),
    (".jpeg", "Apercu.JPEG"),
    (".jpe", "Apercu.JPEG"),
    (".jfif", "Apercu.JPEG"),
    (".png", "Apercu.PNG"),
    (".webp", "Apercu.WEBP"),
    (".gif", "Apercu.GIF"),
    (".heic", "Apercu.HEIC"),
    (".heif", "Apercu.HEIC"),
    (".tif", "Apercu.TIFF"),
    (".tiff", "Apercu.TIFF"),
    (".bmp", "Apercu.BMP"),
    (".dib", "Apercu.BMP"),
    (".cr2", "Apercu.RAW"),
    (".dng", "Apercu.RAW"),
    (".ai", "Apercu.AI"),
    (".jp2", "Apercu.JP2"),
    (".j2k", "Apercu.JP2"),
    (".j2c", "Apercu.JP2"),
    (".jpf", "Apercu.JP2"),
    (".jpx", "Apercu.JP2"),
    (".hdr", "Apercu.HDR"),
    (".exr", "Apercu.EXR"),
    (".ico", "Apercu.ICO"),
    (".ppm", "Apercu.PNM"),
    (".pgm", "Apercu.PNM"),
    (".pbm", "Apercu.PNM"),
    (".pnm", "Apercu.PNM"),
    (".pam", "Apercu.PNM"),
];

/// Déclare l'appli à Windows : un type de fichier par sorte de document (PDF, JPEG, HEIC…), chacun
/// avec son icône dans l'Explorateur, et la liste de ce qu'elle sait ouvrir. Rien n'est imposé :
/// l'utilisateur choisit dans les réglages de Windows. Refait à chaque lancement, pour suivre le
/// chemin de l'appli.
pub fn register_app() {
    let Ok(exe) = std::env::current_exe() else { return };
    let icons = exe.parent().map(|d| d.join("types")).unwrap_or_default();
    let exe = exe.display().to_string();
    let open = format!("\"{exe}\" \"%1\"");
    // Les types gardent leur nom « Apercu.* » (l'ancien nom de l'appli) : c'est sous ce nom que
    // Windows a retenu l'appli par défaut choisie par l'utilisateur.
    // « Apercu.Image » : l'ancien type commun aux images, gardé pour qui l'a déjà choisi.
    let types = [
        ("Apercu.PDF", tr(Text::KindPdf), "pdf.ico"),
        ("Apercu.JPEG", tr(Text::KindJpeg), "jpg.ico"),
        ("Apercu.PNG", tr(Text::KindPng), "png.ico"),
        ("Apercu.WEBP", tr(Text::KindWebp), "webp.ico"),
        ("Apercu.GIF", tr(Text::KindGif), "gif.ico"),
        ("Apercu.HEIC", tr(Text::KindHeic), "heic.ico"),
        ("Apercu.TIFF", tr(Text::KindTiff), "tif.ico"),
        ("Apercu.BMP", tr(Text::KindBmp), "bmp.ico"),
        ("Apercu.RAW", tr(Text::KindRaw), "raw.ico"),
        ("Apercu.AI", tr(Text::KindAi), "ai.ico"),
        ("Apercu.JP2", tr(Text::KindJp2), "jp2.ico"),
        ("Apercu.HDR", tr(Text::KindHdr), "hdr.ico"),
        ("Apercu.EXR", tr(Text::KindExr), "exr.ico"),
        ("Apercu.ICO", tr(Text::KindIco), "ico.ico"),
        ("Apercu.PNM", tr(Text::KindPnm), "ppm.ico"),
        ("Apercu.Image", tr(Text::KindImage), "image.ico"),
    ];
    let mut ok = true;
    for (prog_id, label, icon) in types {
        let base = format!(r"Software\Classes\{prog_id}");
        let icon = icons.join(icon);
        let icon = if icon.is_file() { format!("\"{}\"", icon.display()) } else { format!("\"{exe}\",0") };
        ok &= set(&base, None, label).is_ok();
        ok &= set(&format!(r"{base}\DefaultIcon"), None, &icon).is_ok();
        ok &= set(&format!(r"{base}\shell\open\command"), None, &open).is_ok();
    }
    ok &= set(CAPABILITIES, Some("ApplicationName"), APP_NAME).is_ok();
    ok &= set(CAPABILITIES, Some("ApplicationDescription"), tr(Text::AppDescription)).is_ok();
    for &(ext, prog_id) in EXTENSIONS {
        ok &= set(&format!(r"{CAPABILITIES}\FileAssociations"), Some(ext), prog_id).is_ok();
        ok &= set(&format!(r"Software\Classes\{ext}\OpenWithProgids"), Some(prog_id), "").is_ok();
        // Sans quoi « Ouvrir avec » proposerait l'appli deux fois.
        unsafe {
            let _ = RegDeleteKeyValueW(
                HKEY_CURRENT_USER,
                &HSTRING::from(format!(r"Software\Classes\{ext}\OpenWithProgids")),
                w!("Apercu.Image"),
            );
        }
    }
    ok &= set(r"Software\RegisteredApplications", Some(APP_NAME), CAPABILITIES).is_ok();
    forget_old_name();
    if ok {
        unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None) };
    }
}

/// Jusqu'à la version 0.1.28, l'appli avait un autre nom : le retire de la liste
/// des applis par défaut de Windows.
fn forget_old_name() {
    unsafe {
        let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, w!(r"Software\RegisteredApplications"), w!("Aperçu"));
        let _ = RegDeleteTreeW(HKEY_CURRENT_USER, w!(r"Software\Apercu"));
    }
}

/// L'appli a-t-elle déjà été déclarée pour cet utilisateur (« Utiliser par défaut », ou un
/// lancement normal) ?
fn registered_before() -> bool {
    [w!(r"Software\Classes\Apercu.PDF"), w!(r"Software\Classes\Apercu.Image")].into_iter().any(|path| unsafe {
        let mut key = HKEY::default();
        let found = RegOpenKeyExW(HKEY_CURRENT_USER, path, None, KEY_READ, &mut key).is_ok();
        if found {
            let _ = RegCloseKey(key);
        }
        found
    })
}

/// Remet à jour les réglages de l'utilisateur de cette session qui pointent vers le programme.
/// Le passage de l'ancien nom à « Documents » a changé le dossier de l'appli : sans cela, l'aperçu
/// rapide ne démarrerait plus à l'ouverture de session, et un double-clic sur un PDF ne
/// trouverait plus l'appli (pour qui l'avait choisie par défaut). Lancé à l'ouverture de session
/// après l'installation (`--reglages-utilisateur`), et en arrière-plan pour l'aperçu rapide.
/// Ne déclare rien de nouveau : ne touche qu'à ce qui existait déjà.
pub fn refresh_user_settings() {
    crate::apercu_rapide::refresh();
    if registered_before() {
        register_app();
    }
}

/// Ouvre la page des applis par défaut de Windows, directement sur l'appli quand Windows le
/// permet (Windows 11) ; sinon la page générale.
pub fn open_default_apps() -> Result<(), String> {
    register_app();
    let target = format!("ms-settings:defaultapps?registeredAppUser={APP_NAME}");
    let r = unsafe { ShellExecuteW(None, w!("open"), &HSTRING::from(target), None, None, SW_SHOWNORMAL) };
    // ShellExecute renvoie une valeur > 32 quand tout va bien.
    if r.0 as isize > 32 { Ok(()) } else { Err(tr(Text::SettingsFailed).into()) }
}

/// Les fichiers copiés par l'appli, et le numéro du presse-papier juste après (s'il a changé,
/// quelqu'un d'autre a copié autre chose : on repart de zéro).
static COPIED: Mutex<(Vec<PathBuf>, u32)> = Mutex::new((Vec::new(), 0));

/// `owner` : la fenêtre de l'appli (Windows refuse un presse-papier sans propriétaire).
pub fn copy_files(owner: isize, paths: Vec<PathBuf>, add: bool) -> Result<usize, String> {
    let mut copied = COPIED.lock().unwrap();
    let still_ours = unsafe { GetClipboardSequenceNumber() } == copied.1;
    let mut list = if add && still_ours { copied.0.clone() } else { Vec::new() };
    for p in paths {
        if !list.contains(&p) {
            list.push(p);
        }
    }
    put_files(HWND(owner as *mut _), &list)?;
    let n = list.len();
    *copied = (list, unsafe { GetClipboardSequenceNumber() });
    Ok(n)
}

/// Met une liste de fichiers dans le presse-papier, comme « Copier » dans l'Explorateur.
fn put_files(owner: HWND, paths: &[PathBuf]) -> Result<(), String> {
    // DROPFILES, puis les chemins en UTF-16, chacun terminé par un zéro, et un zéro final.
    let mut names: Vec<u16> = Vec::new();
    for p in paths {
        names.extend(p.as_os_str().to_string_lossy().encode_utf16());
        names.push(0);
    }
    names.push(0);
    let header = std::mem::size_of::<DROPFILES>();
    let size = header + names.len() * 2;
    unsafe {
        OpenClipboard(Some(owner)).map_err(|e| e.to_string())?;
        let result = (|| {
            EmptyClipboard().map_err(|e| e.to_string())?;
            let files = global(size, |ptr| {
                let drop = ptr.cast::<DROPFILES>();
                drop.write(DROPFILES { pFiles: header as u32, fWide: true.into(), ..Default::default() });
                std::ptr::copy_nonoverlapping(names.as_ptr(), ptr.add(header).cast::<u16>(), names.len());
            })?;
            SetClipboardData(CF_HDROP.0 as u32, Some(HANDLE(files.0))).map_err(|e| {
                let _ = GlobalFree(Some(files));
                e.to_string()
            })?;
            // « Copier » et pas « Couper » : l'Explorateur colle une copie.
            let effect = RegisterClipboardFormatW(w!("Preferred DropEffect"));
            if effect != 0 {
                if let Ok(mem) = global(4, |ptr| ptr.cast::<u32>().write(1)) {
                    let _ = SetClipboardData(effect, Some(HANDLE(mem.0)));
                }
            }
            Ok(())
        })();
        let _ = CloseClipboard();
        result
    }
}

/// Un bloc de mémoire « global », comme le presse-papier les demande, rempli par `fill`.
unsafe fn global(size: usize, fill: impl FnOnce(*mut u8)) -> Result<HGLOBAL, String> {
    unsafe {
        let mem = GlobalAlloc(GMEM_MOVEABLE, size).map_err(|e| e.to_string())?;
        let ptr = GlobalLock(mem).cast::<u8>();
        if ptr.is_null() {
            let _ = GlobalFree(Some(mem));
            return Err("out of memory".into());
        }
        std::ptr::write_bytes(ptr, 0, size);
        fill(ptr);
        let _ = GlobalUnlock(mem);
        Ok(mem)
    }
}
