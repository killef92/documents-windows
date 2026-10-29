#[cfg(windows)]
mod background;
#[cfg(windows)]
mod apercu_rapide;
mod i18n;
#[cfg(windows)]
mod shell;
mod thumbs;
mod worker;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use documents_core::form::{DocFill, FormField};
use documents_core::reduce::Level;
use documents_core::{DocId, DocInfo, PageRef, PageText, Retouch, Rotation, SearchHit};
use tauri::http::{header, Response, StatusCode};
use tauri::{Emitter, Manager};
use thumbs::Thumbs;
use worker::{OpenResult, RenderRequest, Worker};

/// Fichiers à ouvrir, par fenêtre, en attendant que son interface vienne les chercher
/// (double-clic dans l'Explorateur, autre programme qui ouvre des images…).
#[derive(Default)]
struct PendingFiles(Mutex<HashMap<String, Vec<PathBuf>>>);

/// La fenêtre qui a reçu des fichiers en dernier, et quand.
#[derive(Default)]
struct LastOpen(Mutex<Option<(String, Instant)>>);

/// Des fichiers qui arrivent les uns après les autres à moins de ce délai vont dans la même
/// fenêtre : un logiciel qui ouvre 25 photos d'un coup donne une fenêtre avec 25 vignettes.
/// Le délai compte à partir du moment où la fenêtre a pris ses fichiers (pas du lancement) :
/// un logiciel qui attend que la première fenêtre soit prête avant d'envoyer la photo
/// suivante ne doit pas en faire ouvrir une deuxième.
const BURST: Duration = Duration::from_secs(5);

/// Fenêtres d'aperçu rapide ouvertes (Windows) : le fichier montré par chacune.
#[derive(Default)]
struct ApercuRapideFiles(Mutex<HashMap<String, PathBuf>>);

/// L'appli a été lancée en arrière-plan pour l'aperçu rapide (Windows, ouverture de session).
fn background_mode() -> bool {
    std::env::args().any(|a| a == "--apercu-rapide")
}

/// Clé de lecture seule du dépôt où sont publiées les mises à jour, utile tant qu'il est privé
/// (vide en développement).
#[tauri::command]
fn update_token() -> Option<&'static str> {
    option_env!("UPDATER_TOKEN").filter(|t| !t.is_empty())
}

/// L'option « Aperçu rapide avec Espace » : None quand elle n'existe pas (hors Windows).
#[tauri::command]
fn apercu_rapide_enabled() -> Option<bool> {
    #[cfg(windows)]
    return Some(apercu_rapide::is_enabled());
    #[cfg(not(windows))]
    None
}

#[tauri::command]
fn set_apercu_rapide(enabled: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        apercu_rapide::set_enabled(enabled)?;
        if enabled {
            // Démarre tout de suite en arrière-plan (sans attendre la prochaine ouverture de session).
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            std::process::Command::new(exe).arg("--apercu-rapide").spawn().map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = enabled;
        Err("quick preview only exists on Windows".into())
    }
}

/// Le fichier à montrer si cette fenêtre est un aperçu rapide.
#[tauri::command]
fn apercu_rapide_path(window: tauri::WebviewWindow, files: tauri::State<'_, ApercuRapideFiles>) -> Option<PathBuf> {
    files.0.lock().unwrap().get(window.label()).cloned()
}

/// « Ouvrir avec Documents » depuis un aperçu rapide : une fenêtre normale, dans un nouveau programme.
#[tauri::command]
fn open_in_app(path: PathBuf) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    std::process::Command::new(exe).arg(path).spawn().map(|_| ()).map_err(|e| e.to_string())
}


#[tauri::command]
async fn open_paths(worker: tauri::State<'_, Worker>, paths: Vec<PathBuf>) -> Result<OpenResult, String> {
    let rx = worker.open(paths);
    tauri::async_runtime::spawn_blocking(move || rx.recv().unwrap_or_default())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn close_docs(worker: tauri::State<'_, Worker>, ids: Vec<DocId>) {
    worker.close(ids);
}

#[tauri::command]
async fn search(worker: tauri::State<'_, Worker>, query: String) -> Result<Vec<SearchHit>, String> {
    let rx = worker.search(query);
    tauri::async_runtime::spawn_blocking(move || rx.recv().unwrap_or_default())
        .await
        .map_err(|e| e.to_string())
}

/// Attend la réponse du fil des documents sans bloquer l'interface.
async fn wait<T: Send + 'static>(rx: std::sync::mpsc::Receiver<Result<T, String>>) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || rx.recv().map_err(|e| e.to_string())?)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn save_pdf(
    worker: tauri::State<'_, Worker>,
    pages: Vec<PageRef>,
    fills: Option<Vec<DocFill>>,
    dest: PathBuf,
) -> Result<(), String> {
    wait(worker.save_pdf(pages, fills.unwrap_or_default(), dest)).await
}

/// « Exporter en taille réduite » : renvoie la taille avant et après, en octets.
#[tauri::command]
async fn save_pdf_reduced(
    worker: tauri::State<'_, Worker>,
    pages: Vec<PageRef>,
    fills: Option<Vec<DocFill>>,
    level: Level,
    dest: PathBuf,
) -> Result<(u64, u64), String> {
    wait(worker.save_pdf_reduced(pages, fills.unwrap_or_default(), level, dest)).await
}

#[tauri::command]
async fn form_fields(worker: tauri::State<'_, Worker>, doc: DocId) -> Result<Vec<FormField>, String> {
    wait(worker.form_fields(doc)).await
}

/// Retouche d'une photo (recadrage, couleurs) : renvoie le nouveau document, en mémoire.
#[tauri::command]
async fn retouch_picture(worker: tauri::State<'_, Worker>, doc: DocId, retouch: Retouch) -> Result<DocInfo, String> {
    wait(worker.retouch(doc, retouch)).await
}

#[tauri::command]
async fn save_picture(worker: tauri::State<'_, Worker>, doc: DocId, rotation: i32, dest: PathBuf) -> Result<(), String> {
    wait(worker.save_picture(doc, rotation, dest)).await
}

/// Photo annotée : l'interface envoie l'image finale (PNG) telle quelle, sans passer par du JSON.
/// Le document et le chemin sont dans les en-têtes (chemin encodé façon URL).
#[tauri::command]
async fn save_picture_pixels(worker: tauri::State<'_, Worker>, request: tauri::ipc::Request<'_>) -> Result<(), String> {
    let tauri::ipc::InvokeBody::Raw(png) = request.body() else {
        return Err("missing image".into());
    };
    let header = |name: &str| request.headers().get(name).and_then(|v| v.to_str().ok()).map(String::from);
    let doc: DocId = header("x-doc").and_then(|v| v.parse().ok()).ok_or("missing document")?;
    let dest = header("x-dest").map(|v| percent_decode(&v)).ok_or("missing path")?;
    wait(worker.save_picture_pixels(doc, png.clone(), PathBuf::from(dest))).await
}

#[tauri::command]
async fn page_text(worker: tauri::State<'_, Worker>, doc: DocId, page: usize) -> Result<PageText, String> {
    wait(worker.page_text(doc, page)).await
}

#[tauri::command]
async fn doc_details(worker: tauri::State<'_, Worker>, doc: DocId) -> Result<documents_core::DocDetails, String> {
    wait(worker.details(doc)).await
}

/// Signature à partir d'une photo : PNG transparent, en « data URL » pour l'interface.
#[tauri::command]
async fn signature_from_photo(path: PathBuf) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let bytes = std::fs::read(&path).map_err(|e| i18n::error(&documents_core::Error::from(e)))?;
        documents_core::signature::from_photo(&bytes)
            .map(|png| documents_core::signature::to_data_url(&png))
            .map_err(|e| i18n::error(&e))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// « %C3%A9 » → « é ».
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Some(b) = std::str::from_utf8(&bytes[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Les fichiers en attente pour cette fenêtre (une seule fois).
#[tauri::command]
fn take_paths(window: tauri::WebviewWindow, files: tauri::State<'_, PendingFiles>, last: tauri::State<'_, LastOpen>) -> Vec<PathBuf> {
    let paths = files.0.lock().unwrap().remove(window.label()).unwrap_or_default();
    if !paths.is_empty() {
        // La page est prête et reçoit des fichiers : la fenêtre passe devant, avec le clavier.
        bring_forward(&window);
        let mut last = last.0.lock().unwrap();
        if last.as_ref().is_some_and(|(label, _)| label == window.label()) {
            *last = Some((window.label().to_string(), Instant::now()));
        }
    }
    paths
}

/// Les autres images du dossier de cette image, dans l'ordre de l'Explorateur.
#[tauri::command]
async fn folder_images(path: PathBuf) -> Result<Vec<PathBuf>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let dir = path.parent().ok_or("no folder")?;
        documents_core::folder::images_in(dir).map_err(|e| i18n::error(&e))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Copie des fichiers dans le presse-papier (à coller dans l'Explorateur, un mail…).
/// `add` : on ajoute aux fichiers déjà copiés par l'appli, s'ils y sont encore.
/// Renvoie le nombre de fichiers dans le presse-papier.
#[tauri::command]
fn copy_files(window: tauri::WebviewWindow, paths: Vec<PathBuf>, add: bool) -> Result<usize, String> {
    #[cfg(windows)]
    return shell::copy_files(window.hwnd().map_err(|e| e.to_string())?.0 as isize, paths, add);
    #[cfg(not(windows))]
    {
        let _ = (window, paths, add);
        Err("copying files only exists on Windows".into())
    }
}

/// « Utiliser Documents par défaut » : déclare l'appli à Windows et ouvre la page des applis par défaut.
#[tauri::command]
fn choose_default_app() -> Result<(), String> {
    #[cfg(windows)]
    return shell::open_default_apps();
    #[cfg(not(windows))]
    Err("only on Windows".into())
}

fn paths_from_args(args: impl IntoIterator<Item = impl Into<std::ffi::OsString>>) -> Vec<PathBuf> {
    args.into_iter().skip(1).map(|a| PathBuf::from(a.into())).filter(|p| p.is_file()).collect()
}

/// Le nom d'une nouvelle fenêtre de documents.
fn next_label() -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    static NEXT: AtomicU32 = AtomicU32::new(1);
    format!("fenetre-{}", NEXT.fetch_add(1, Ordering::Relaxed))
}

/// Le genre des fichiers qu'une fenêtre va ouvrir, donné à la page avant son premier affichage
/// (classes du corps de page) : les bons outils sont là tout de suite, sans étape intermédiaire.
fn genre_script(paths: &[PathBuf]) -> String {
    let ext = |p: &PathBuf| p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let picture = |p: &PathBuf| matches!(ext(p).as_str(), "jpg" | "jpeg" | "png" | "webp" | "gif" | "heic");
    let mut classes = Vec::new();
    if !paths.is_empty() && paths.iter().all(picture) {
        classes.push("only-pictures");
        classes.push(if paths.len() == 1 { "single-image" } else { "many-pictures" });
    } else if !paths.is_empty() && paths.iter().all(|p| ext(p) == "pdf") {
        classes.push("only-pdf");
    }
    format!("window.__APERCU_GENRE__ = {:?};", classes.join(" "))
}

/// Une nouvelle fenêtre de documents, comme la première.
fn new_window(app: &tauri::AppHandle, label: &str, genre: String) {
    let _ = tauri::WebviewWindowBuilder::new(app, label, tauri::WebviewUrl::App("index.html".into()))
        .initialization_script(i18n::script())
        .initialization_script(genre)
        .title("Documents")
        .inner_size(1280.0, 860.0)
        .min_inner_size(708.0, 224.0)
        .decorations(false)
        .shadow(true)
        .center()
        .focused(true)
        .build();
}

/// Met la fenêtre devant, avec le clavier dans la page : ouverte par un autre logiciel, elle
/// doit répondre tout de suite à Ctrl+A, sans clic préalable. Windows laisse faire parce que la
/// copie lancée par ce logiciel nous en a donné le droit (voir `allow_foreground`). Tout se fait
/// sur le fil de l'interface : depuis un autre fil, brancher l'entrée clavier sur celle d'un
/// logiciel occupé (l'ancienne méthode) pouvait bloquer la fenêtre.
fn bring_forward(window: &tauri::WebviewWindow) {
    let w = window.clone();
    let _ = window.run_on_main_thread(move || {
        let _ = w.unminimize();
        let _ = w.set_focus();
        let webview: &tauri::Webview = w.as_ref();
        let _ = webview.set_focus();
    });
}

/// Chaque copie lancée (double-clic, autre logiciel) autorise la copie déjà ouverte à passer au
/// premier plan, avant de lui transmettre ses fichiers et de s'arrêter.
fn allow_foreground() {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::{AllowSetForegroundWindow, ASFW_ANY};
        let _ = AllowSetForegroundWindow(ASFW_ANY);
    }
}

/// Verrou de lancement (Windows) : un logiciel qui ouvre 25 photos lance 25 copies de l'appli
/// presque en même temps. Sans ce verrou, une copie pouvait voir que l'appli tournait déjà sans
/// trouver encore où lui envoyer ses photos, et ouvrir alors sa propre fenêtre. Les copies
/// passent donc une par une ; la première rend le verrou une fois prête à recevoir les autres.
#[cfg(windows)]
static LAUNCH_LOCK: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

fn take_launch_lock() {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::System::Threading::{CreateMutexW, WaitForSingleObject};
        if let Ok(h) = CreateMutexW(None, false, windows::core::w!("Local\\Documents-lancement")) {
            // Au pire 20 s d'attente : une copie restée bloquée ne doit pas empêcher d'ouvrir.
            let _ = WaitForSingleObject(h, 20_000);
            LAUNCH_LOCK.store(h.0 as isize, std::sync::atomic::Ordering::SeqCst);
        }
    }
}

fn release_launch_lock() {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::System::Threading::ReleaseMutex;
        let h = LAUNCH_LOCK.swap(0, std::sync::atomic::Ordering::SeqCst);
        if h != 0 {
            let _ = ReleaseMutex(HANDLE(h as *mut _));
        }
    }
}

/// L'appli est relancée (double-clic sur un fichier, autre logiciel…) alors qu'elle tourne déjà :
/// les fichiers vont dans la dernière fenêtre s'ils arrivent en rafale (ou si elle n'a pas encore
/// pris ses fichiers, parce qu'elle se charge encore), sinon dans une nouvelle.
///
/// La fenêtre est créée après avoir relâché les verrous : la créer demande au fil de
/// l'interface, qui peut justement attendre l'un de ces verrous dans `take_paths` (c'est ce
/// qui figeait l'appli quand un logiciel envoyait plusieurs photos d'un coup).
fn open_from_launch(app: &tauri::AppHandle, paths: Vec<PathBuf>) {
    let now = Instant::now();
    let last_state = app.state::<LastOpen>();
    let pending_state = app.state::<PendingFiles>();
    let (target, create, genre) = {
        let mut last = last_state.0.lock().unwrap();
        let mut pending = pending_state.0.lock().unwrap();
        let reuse = match &*last {
            Some((label, t)) if !paths.is_empty() => {
                let waiting = pending.get(label).is_some_and(|p| !p.is_empty());
                let alive = waiting || app.get_webview_window(label).is_some();
                (alive && (now - *t < BURST || waiting)).then(|| label.clone())
            }
            _ => None,
        };
        let (target, create) = match reuse {
            Some(label) => (label, false),
            None => (next_label(), true),
        };
        if !paths.is_empty() {
            *last = Some((target.clone(), now));
        }
        let genre = genre_script(&paths);
        pending.entry(target.clone()).or_default().extend(paths);
        (target, create, genre)
    };
    if create {
        new_window(app, &target, genre);
    } else {
        let _ = app.emit_to(target.as_str(), "paths-pending", ());
    }
}

/// Où chercher PDFium : à côté de l'appli installée, puis dans vendor/ pendant le développement.
fn pdfium_dirs(app: &tauri::App) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(res) = app.path().resource_dir() {
        dirs.push(res.join("pdfium"));
        dirs.push(res);
    }
    if let Some(exe_dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(PathBuf::from)) {
        dirs.push(exe_dir);
    }
    if cfg!(debug_assertions) {
        if let Ok(entries) = std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../vendor/pdfium")) {
            dirs.extend(entries.flatten().map(|e| e.path()));
        }
    }
    dirs
}

/// Adresse d'une page : page://localhost/<document>/<page>/<largeur>/<rotation>[/sans-champs]
fn parse_page_uri(path: &str) -> Option<RenderRequest> {
    let mut parts = path.trim_matches('/').split('/');
    let mut request = RenderRequest {
        doc: parts.next()?.parse().ok()?,
        page: parts.next()?.parse().ok()?,
        width: parts.next()?.parse().ok()?,
        rotation: Rotation::from_degrees(parts.next()?.parse().ok()?),
        forms: true,
    };
    match parts.next() {
        None => {}
        Some("sans-champs") => request.forms = false,
        Some(_) => return None,
    }
    parts.next().is_none().then_some(request)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Lancé une fois par Windows à l'ouverture de session de chaque utilisateur, après
    // l'installation pour tous : remet ses réglages à jour, puis s'arrête (pas de fenêtre).
    #[cfg(windows)]
    if std::env::args().any(|a| a == "--reglages-utilisateur") {
        shell::refresh_user_settings();
        return;
    }
    allow_foreground();
    let background = background_mode();
    if !background {
        take_launch_lock();
    }
    let startup = paths_from_args(std::env::args_os());
    let startup_genre = startup.clone();
    let mut builder = tauri::Builder::default();
    // Une seule copie de l'appli (sauf l'aperçu rapide, qui tourne à part) : les lancements
    // suivants lui passent leurs fichiers.
    if !background {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            // Hors du fil de la fenêtre : créer une fenêtre depuis ce fil bloquerait Windows.
            let app = app.clone();
            let paths = paths_from_args(argv);
            std::thread::spawn(move || open_from_launch(&app, paths));
        }));
    }
    let first_open = (!startup.is_empty()).then(|| ("main".to_string(), Instant::now()));
    let app = builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(PendingFiles(Mutex::new(HashMap::from([("main".to_string(), startup)]))))
        .manage(LastOpen(Mutex::new(first_open)))
        .manage(ApercuRapideFiles::default())
        .setup(move |app| {
            // Ici, l'appli sait déjà recevoir les fichiers des copies suivantes.
            release_launch_lock();
            // La première fenêtre, décrite dans tauri.conf.json, avec le genre de ses fichiers.
            if !background {
                if let Some(config) = app.config().app.windows.iter().find(|w| w.label == "main") {
                    tauri::WebviewWindowBuilder::from_config(app.handle(), config)?
                        .initialization_script(i18n::script())
                        .initialization_script(genre_script(&startup_genre))
                        .build()?;
                }
            }
            app.manage(Worker::start(pdfium_dirs(app)));
            app.manage(Thumbs::start());
            #[cfg(windows)]
            if background {
                background::start(app)?;
            } else {
                // Déclare l'appli à Windows (pour « Ouvrir avec » et les applis par défaut),
                // avec le bon chemin même après une mise à jour ; de même pour l'aperçu rapide.
                std::thread::spawn(|| {
                    apercu_rapide::refresh();
                    shell::register_app();
                });
            }
            #[cfg(windows)]
            if !background {
                apercu_rapide::apply_default();
            }
            #[cfg(windows)]
            if !background && apercu_rapide::is_enabled() {
                // Aperçu rapide coché mais pas lancé (après une mise à jour, l'installateur l'a
                // arrêté) : on le relance. S'il tourne déjà, la copie s'arrête aussitôt.
                if let Ok(exe) = std::env::current_exe() {
                    let _ = std::process::Command::new(exe).arg("--apercu-rapide").spawn();
                }
            }
            Ok(())
        })
        .register_asynchronous_uri_scheme_protocol("page", |ctx, request, responder| {
            let Some(page) = parse_page_uri(request.uri().path()) else {
                responder.respond(Response::builder().status(StatusCode::BAD_REQUEST).body(Vec::new()).unwrap());
                return;
            };
            ctx.app_handle().state::<Worker>().render(page, move |result| {
                let response = match result {
                    Ok(img) => Response::builder()
                        .header(header::CONTENT_TYPE, img.mime)
                        .header(header::CACHE_CONTROL, "max-age=31536000, immutable")
                        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                        .body(img.bytes),
                    Err(message) => Response::builder()
                        .status(StatusCode::UNPROCESSABLE_ENTITY)
                        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
                        .body(message.into_bytes()),
                };
                responder.respond(response.unwrap());
            });
        })
        .register_asynchronous_uri_scheme_protocol("vignette", |ctx, request, responder| {
            // vignette://localhost/<taille>/<chemin encodé façon URL>
            let path = request.uri().path().trim_start_matches('/').to_string();
            let Some((size, file)) = path.split_once('/').and_then(|(s, f)| Some((s.parse::<u32>().ok()?, f))) else {
                responder.respond(Response::builder().status(StatusCode::BAD_REQUEST).body(Vec::new()).unwrap());
                return;
            };
            ctx.app_handle().state::<Thumbs>().get(PathBuf::from(percent_decode(file)), size, move |result| {
                let response = match result {
                    Ok(t) => Response::builder()
                        .header(header::CONTENT_TYPE, t.mime)
                        .header(header::CACHE_CONTROL, "max-age=600")
                        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                        .body(t.bytes.clone()),
                    Err(message) => Response::builder()
                        .status(StatusCode::SERVICE_UNAVAILABLE)
                        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
                        .body(message.into_bytes()),
                };
                responder.respond(response.unwrap());
            });
        })
        .invoke_handler(tauri::generate_handler![
            open_paths, close_docs, search, take_paths, folder_images, copy_files, choose_default_app, save_pdf, save_picture,
            save_picture_pixels, page_text, doc_details, form_fields, signature_from_photo,
            retouch_picture, save_pdf_reduced, apercu_rapide_enabled, set_apercu_rapide, apercu_rapide_path, open_in_app,
            update_token
        ])
        .build(tauri::generate_context!())
        .expect("erreur au lancement de l'appli");

    #[allow(unused_variables)]
    app.run(move |app, event| {
        // En arrière-plan, fermer la dernière fenêtre d'aperçu ne quitte pas l'appli.
        if let tauri::RunEvent::ExitRequested { api, code: None, .. } = &event {
            if background {
                api.prevent_exit();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lit_l_adresse_d_une_page() {
        let r = parse_page_uri("/3/12/800/270").unwrap();
        assert_eq!((r.doc, r.page, r.width, r.rotation), (3, 12, 800, Rotation::R270));
        assert!(parse_page_uri("/3/12/800").is_none());
        assert!(parse_page_uri("/a/b/c/d").is_none());
        assert!(parse_page_uri("/1/2/3/4/5").is_none());
        assert!(r.forms);
        assert!(!parse_page_uri("/3/12/800/0/sans-champs").unwrap().forms);
    }

    #[test]
    fn decode_un_chemin_encode() {
        assert_eq!(percent_decode("C%3A%5CUsers%5Cr%C3%A9sum%C3%A9.jpg"), "C:\\Users\\résumé.jpg");
        assert_eq!(percent_decode("a%2"), "a%2");
        assert_eq!(percent_decode("100%"), "100%");
    }
}
