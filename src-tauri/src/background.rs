//! Mode arrière-plan (Windows) : pas de fenêtre, une icône près de l'horloge, et la barre
//! espace surveillée dans l'Explorateur pour l'aperçu rapide.

use std::path::PathBuf;

use tauri::Manager;

use crate::i18n::{tr, Text};
use crate::{apercu_rapide, i18n, shell};
use crate::ApercuRapideFiles;

/// Ouvre (ou remplace) la fenêtre d'aperçu rapide pour ce fichier.
fn show_apercu_rapide(app: &tauri::AppHandle, path: PathBuf) {
    use std::sync::atomic::{AtomicU32, Ordering};
    static NEXT: AtomicU32 = AtomicU32::new(1);
    for (label, window) in app.webview_windows() {
        if label.starts_with("apercu-rapide") {
            let _ = window.close();
            app.state::<ApercuRapideFiles>().0.lock().unwrap().remove(&label);
        }
    }
    let label = format!("apercu-rapide-{}", NEXT.fetch_add(1, Ordering::Relaxed));
    let title = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    app.state::<ApercuRapideFiles>().0.lock().unwrap().insert(label.clone(), path);
    let window = tauri::WebviewWindowBuilder::new(app, label, tauri::WebviewUrl::App("index.html".into()))
        .title(title)
        .inner_size(960.0, 720.0)
        .min_inner_size(400.0, 300.0)
        .decorations(false)
        .shadow(true)
        .center()
        .focused(true)
        .initialization_script(i18n::script())
        .initialization_script("window.__APERCU_RAPIDE__ = true;")
        .build();
    if let Ok(window) = window {
        if let Ok(hwnd) = window.hwnd() {
            apercu_rapide::bring_to_front(hwnd.0 as isize);
        }
        let _ = window.set_focus();
    }
}

/// Mode arrière-plan : pas de fenêtre, une icône près de l'horloge, et la barre espace surveillée.
pub fn start(app: &tauri::App) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::TrayIconBuilder;

    // Lancé dans chaque session juste après une installation : l'occasion de remettre à jour
    // les réglages de l'utilisateur, même s'il n'utilise pas l'aperçu rapide.
    shell::refresh_user_settings();
    apercu_rapide::apply_default();
    if !apercu_rapide::first_instance() || !apercu_rapide::is_enabled() {
        std::process::exit(0);
    }
    if let Some(window) = app.get_webview_window("main") {
        window.destroy()?;
    }
    let info = MenuItem::with_id(app, "info", tr(Text::QuickInfo), false, None::<&str>)?;
    let off = MenuItem::with_id(app, "off", tr(Text::QuickOff), true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", tr(Text::Quit), true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&info, &off, &quit])?;
    let mut tray = TrayIconBuilder::with_id("apercu-rapide").tooltip(tr(Text::QuickTooltip)).menu(&menu).on_menu_event(
        |app, event| match event.id.as_ref() {
            "off" => {
                let _ = apercu_rapide::set_enabled(false);
                app.exit(0);
            }
            "quit" => app.exit(0),
            _ => {}
        },
    );
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;

    let handle = app.handle().clone();
    apercu_rapide::start(move |path| show_apercu_rapide(&handle, path));
    // Option décochée depuis une fenêtre de l'appli : on s'arrête.
    let handle = app.handle().clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(5));
        if !apercu_rapide::is_enabled() {
            handle.exit(0);
        }
    });
    Ok(())
}
