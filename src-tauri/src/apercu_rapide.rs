//! Aperçu rapide (Windows) : Espace dans l'Explorateur montre le fichier sélectionné.
//!
//! L'appli tourne alors en arrière-plan (lancée à l'ouverture de session, avec
//! `--apercu-rapide`). Un « crochet » clavier repère la barre espace quand l'Explorateur
//! ou le bureau est au premier plan et que la liste des fichiers a le focus ; on demande
//! alors à l'Explorateur quel fichier est sélectionné.

use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use windows::core::{w, Interface, PCWSTR};
use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, IServiceProvider, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteKeyValueW, RegGetValueW, RegOpenKeyExW, RegSetKeyValueW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
    KEY_SET_VALUE, REG_DWORD, REG_SZ, RRF_RT_ANY, RRF_RT_REG_SZ,
};
use windows::Win32::System::Threading::{AttachThreadInput, CreateMutexW, GetCurrentThreadId};
use windows::Win32::System::Variant::{VARIANT, VARIANT_0, VARIANT_0_0, VARIANT_0_0_0, VT_I4};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT, VK_SPACE};
use windows::Win32::UI::Shell::{
    IFolderView2, IShellBrowser, IShellWindows, IWebBrowserApp, ShellWindows, SID_STopLevelBrowser, SIGDN_FILESYSPATH,
    SWC_DESKTOP, SWFO_NEEDDISPATCH,
};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, CallNextHookEx, GetAncestor, GetClassNameW, GetForegroundWindow, GetGUIThreadInfo, GetMessageW,
    GetWindowThreadProcessId, IsWindowVisible, SetForegroundWindow, SetWindowPos, SetWindowsHookExW, GA_ROOT, GUITHREADINFO,
    HC_ACTION, HWND_NOTOPMOST, HWND_TOPMOST, KBDLLHOOKSTRUCT, MSG, SWP_NOMOVE, SWP_NOSIZE, WH_KEYBOARD_LL, WM_KEYDOWN,
};

/// Fichiers qu'on sait montrer.
const EXTENSIONS: &[&str] = &["pdf", "jpg", "jpeg", "png", "webp", "gif"];

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const RUN_VALUE: PCWSTR = w!("Documents - apercu rapide");
/// Le même réglage sous l'ancien nom de l'appli (jusqu'à la version 0.1.28).
const OLD_RUN_VALUE: PCWSTR = w!("Apercu - apercu rapide");

fn has_run_value(name: PCWSTR) -> bool {
    let mut size = 0u32;
    unsafe { RegGetValueW(HKEY_CURRENT_USER, RUN_KEY, name, RRF_RT_REG_SZ, None, None, Some(&mut size)) }.is_ok()
}

/// L'option est-elle cochée ? (= l'appli est lancée en arrière-plan à l'ouverture de session)
pub fn is_enabled() -> bool {
    has_run_value(RUN_VALUE) || has_run_value(OLD_RUN_VALUE)
}

/// Marque « déjà activé par défaut une fois », pour respecter ensuite le choix de l'utilisateur.
const SETTINGS_KEY: PCWSTR = w!("Software\\Documents");
const DEFAULT_DONE: PCWSTR = w!("Apercu rapide active par defaut");

/// L'aperçu rapide est activé d'office, une seule fois par utilisateur : s'il le désactive
/// ensuite, il reste désactivé.
pub fn apply_default() {
    unsafe {
        if RegGetValueW(HKEY_CURRENT_USER, SETTINGS_KEY, DEFAULT_DONE, RRF_RT_ANY, None, None, None).is_ok() {
            return;
        }
        if set_enabled(true).is_ok() {
            let one = 1u32.to_le_bytes();
            let _ = RegSetKeyValueW(HKEY_CURRENT_USER, SETTINGS_KEY, DEFAULT_DONE, REG_DWORD.0, Some(one.as_ptr().cast()), 4);
        }
    }
}

/// Option cochée : remet dans le registre le chemin actuel de l'appli. Après le passage
/// de l'ancien nom à « Documents », le réglage pointait vers l'ancien dossier, qui n'existe plus.
pub fn refresh() {
    if is_enabled() {
        let _ = set_enabled(true);
    }
}

/// Coche ou décoche l'option : lancement à l'ouverture de session, ajouté ou retiré.
pub fn set_enabled(on: bool) -> Result<(), String> {
    unsafe {
        if !on {
            let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, OLD_RUN_VALUE);
            let r = RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, RUN_VALUE);
            return if r.is_ok() || !is_enabled() { Ok(()) } else { Err(format!("registry: {r:?}")) };
        }
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let command = format!("\"{}\" --apercu-rapide", exe.display());
        let data: Vec<u16> = command.encode_utf16().chain(Some(0)).collect();
        let bytes = std::slice::from_raw_parts(data.as_ptr().cast::<u8>(), data.len() * 2);
        let mut key = HKEY::default();
        RegOpenKeyExW(HKEY_CURRENT_USER, RUN_KEY, None, KEY_SET_VALUE, &mut key).ok().map_err(|e| e.to_string())?;
        let r = RegSetValueExW(key, RUN_VALUE, None, REG_SZ, Some(bytes));
        let _ = RegCloseKey(key);
        r.ok().map_err(|e| e.to_string())?;
        let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, OLD_RUN_VALUE);
        Ok(())
    }
}

/// Une seule copie en arrière-plan à la fois : renvoie false si elle tourne déjà.
pub fn first_instance() -> bool {
    unsafe {
        // Le verrou reste pris tant que le programme tourne (il n'est jamais refermé).
        let mutex = CreateMutexW(None, true, w!("Local\\Documents-apercu-rapide"));
        mutex.is_ok() && GetLastError() != ERROR_ALREADY_EXISTS
    }
}

static TRIGGER: OnceLock<Sender<isize>> = OnceLock::new();
static LAST: std::sync::Mutex<Option<Instant>> = std::sync::Mutex::new(None);

/// Lance la surveillance de la barre espace ; `show` reçoit le fichier à montrer.
pub fn start(show: impl Fn(PathBuf) + Send + 'static) {
    let (tx, rx) = mpsc::channel::<isize>();
    if TRIGGER.set(tx).is_err() {
        return;
    }
    // Le crochet clavier : il doit répondre très vite, il ne fait que prévenir l'autre fil.
    std::thread::spawn(|| unsafe {
        if SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook), None, 0).is_err() {
            return;
        }
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {}
    });
    // Les questions à l'Explorateur (COM) : sur leur propre fil.
    std::thread::spawn(move || unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        for hwnd in rx {
            if let Some(path) = selected_file(HWND(hwnd as *mut _)) {
                let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
                if EXTENSIONS.contains(&ext.as_str()) {
                    show(path);
                }
            }
        }
    });
}

/// Met la fenêtre d'aperçu devant toutes les autres, avec le clavier.
///
/// Windows refuse qu'un programme en arrière-plan passe au premier plan : on se branche
/// un instant sur la fenêtre active (l'Explorateur), ce qui autorise le changement.
pub fn bring_to_front(hwnd: isize) {
    let hwnd = HWND(hwnd as *mut _);
    unsafe {
        let fg_thread = GetWindowThreadProcessId(GetForegroundWindow(), None);
        let me = GetCurrentThreadId();
        let attached = fg_thread != 0 && fg_thread != me && AttachThreadInput(me, fg_thread, true).as_bool();
        // Passer « toujours devant » puis revenir : la fenêtre monte au-dessus des autres.
        let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
        let _ = SetWindowPos(hwnd, Some(HWND_NOTOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
        let _ = BringWindowToTop(hwnd);
        let _ = SetForegroundWindow(hwnd);
        if attached {
            let _ = AttachThreadInput(me, fg_thread, false);
        }
    }
}

/// Un nombre entier, tel que COM l'attend.
fn int_variant(n: i32) -> VARIANT {
    VARIANT {
        Anonymous: VARIANT_0 {
            Anonymous: std::mem::ManuallyDrop::new(VARIANT_0_0 { vt: VT_I4, Anonymous: VARIANT_0_0_0 { lVal: n }, ..Default::default() }),
        },
    }
}

fn class_of(hwnd: HWND) -> String {
    let mut buf = [0u16; 128];
    let n = unsafe { GetClassNameW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn key_down(vk: u16) -> bool {
    unsafe { GetAsyncKeyState(vk as i32) as u16 & 0x8000 != 0 }
}

/// La fenêtre au premier plan est-elle une liste de fichiers de l'Explorateur (ou le bureau),
/// et pas une zone de saisie (renommer, rechercher, barre d'adresse) ?
fn explorer_list_focused() -> Option<HWND> {
    unsafe {
        let fg = GetForegroundWindow();
        if !matches!(class_of(fg).as_str(), "CabinetWClass" | "ExploreWClass" | "Progman" | "WorkerW") {
            return None;
        }
        let thread = GetWindowThreadProcessId(fg, None);
        let mut info = GUITHREADINFO { cbSize: std::mem::size_of::<GUITHREADINFO>() as u32, ..Default::default() };
        GetGUIThreadInfo(thread, &mut info).ok()?;
        matches!(class_of(info.hwndFocus).as_str(), "DirectUIHWND" | "SysListView32").then_some(fg)
    }
}

unsafe extern "system" fn keyboard_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 && wparam.0 == WM_KEYDOWN as usize {
        let key = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        let modifiers = [VK_SHIFT, VK_CONTROL, VK_MENU, VK_LWIN, VK_RWIN].iter().any(|k| key_down(k.0));
        if key.vkCode == VK_SPACE.0 as u32 && !modifiers {
            if let Some(fg) = explorer_list_focused() {
                // Barre espace maintenue : une seule fois.
                let mut last = LAST.lock().unwrap();
                let now = Instant::now();
                if last.is_none_or(|t| now - t > Duration::from_millis(400)) {
                    if let Some(tx) = TRIGGER.get() {
                        let _ = tx.send(fg.0 as isize);
                    }
                }
                *last = Some(now);
                return LRESULT(1); // l'Explorateur ne reçoit pas cet Espace
            }
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// Le fichier sélectionné dans la fenêtre de l'Explorateur (ou sur le bureau) donnée.
unsafe fn selected_file(fg: HWND) -> Option<PathBuf> {
    let windows: IShellWindows = unsafe { CoCreateInstance(&ShellWindows, None, CLSCTX_ALL) }.ok()?;
    let is_desktop = matches!(class_of(fg).as_str(), "Progman" | "WorkerW");
    let mut candidates = Vec::new();
    if is_desktop {
        let mut hwnd = 0i32;
        let csidl_desktop = int_variant(0);
        let empty = VARIANT::default();
        if let Ok(disp) = unsafe { windows.FindWindowSW(&csidl_desktop, &empty, SWC_DESKTOP, &mut hwnd, SWFO_NEEDDISPATCH) } {
            candidates.push(disp);
        }
    } else {
        let count = unsafe { windows.Count() }.ok()?;
        for i in 0..count {
            let Ok(disp) = (unsafe { windows.Item(&int_variant(i)) }) else { continue };
            let Ok(app) = disp.cast::<IWebBrowserApp>() else { continue };
            let Ok(h) = (unsafe { app.HWND() }) else { continue };
            if unsafe { GetAncestor(HWND(h.0 as *mut _), GA_ROOT) } == fg || h.0 == fg.0 as isize {
                candidates.push(disp);
            }
        }
    }
    // Plusieurs onglets dans la même fenêtre (Windows 11) : on prend celui qui est visible.
    for disp in candidates {
        let Ok(sp) = disp.cast::<IServiceProvider>() else { continue };
        let Ok(browser) = (unsafe { sp.QueryService::<IShellBrowser>(&SID_STopLevelBrowser) }) else { continue };
        let Ok(view) = (unsafe { browser.QueryActiveShellView() }) else { continue };
        if !is_desktop {
            let Ok(view_hwnd) = (unsafe { view.GetWindow() }) else { continue };
            if !unsafe { IsWindowVisible(view_hwnd) }.as_bool() {
                continue;
            }
        }
        let Ok(folder) = view.cast::<IFolderView2>() else { continue };
        let Ok(items) = (unsafe { folder.GetSelection(false) }) else { continue };
        if unsafe { items.GetCount() }.ok()? == 0 {
            return None;
        }
        let item = unsafe { items.GetItemAt(0) }.ok()?;
        let name = unsafe { item.GetDisplayName(SIGDN_FILESYSPATH) }.ok()?;
        let path = unsafe { name.to_string() }.ok();
        unsafe { CoTaskMemFree(Some(name.0 as *const _)) };
        return path.map(PathBuf::from);
    }
    None
}
