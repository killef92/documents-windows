//! Un fil de travail dédié qui garde les documents ouverts et fait tous les rendus,
//! pour que l'interface ne soit jamais bloquée.
//!
//! Les rendus sont traités du plus récent au plus ancien : quand on fait défiler,
//! la page qu'on regarde maintenant passe avant celles qu'on a déjà dépassées.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use documents_core::annotate::Placed;
use documents_core::form::{DocFill, FormField};
use documents_core::reduce::Level;
use documents_core::{DocDetails, DocId, DocInfo, Library, PageRef, PageText, RenderedImage, Retouch, Rotation, SearchHit};
use serde::Serialize;

use crate::i18n;

#[derive(Debug, Clone, Serialize)]
pub struct OpenError {
    pub name: String,
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct OpenResult {
    pub docs: Vec<DocInfo>,
    pub errors: Vec<OpenError>,
}

pub struct RenderRequest {
    pub doc: DocId,
    pub page: usize,
    pub width: u32,
    pub rotation: Rotation,
    /// Dessiner les champs de formulaire (non : l'interface les affiche, pour les remplir).
    pub forms: bool,
}

type RenderReply = Box<dyn FnOnce(Result<RenderedImage, String>) + Send>;

enum Job {
    Open(Vec<PathBuf>, Sender<OpenResult>),
    Close(Vec<DocId>),
    Search(String, Sender<Vec<SearchHit>>),
    Render(RenderRequest, RenderReply),
    SavePdf(Vec<PageRef>, Vec<DocFill>, PathBuf, Sender<Result<(), String>>),
    SavePdfReduced(Vec<PageRef>, Vec<DocFill>, Level, PathBuf, Sender<Result<(u64, u64), String>>),
    FormFields(DocId, Sender<Result<Vec<FormField>, String>>),
    Annotations(DocId, Sender<Result<Vec<Vec<Placed>>, String>>),
    SavePicture(DocId, i32, PathBuf, Sender<Result<(), String>>),
    SavePicturePixels(DocId, Vec<u8>, PathBuf, Sender<Result<(), String>>),
    PageText(DocId, usize, Sender<Result<PageText, String>>),
    Details(DocId, Sender<Result<DocDetails, String>>),
    Retouch(DocId, Retouch, Sender<Result<DocInfo, String>>),
}

#[derive(Clone)]
pub struct Worker {
    tx: Sender<Job>,
}

impl Worker {
    pub fn start(pdfium_dirs: Vec<PathBuf>) -> Self {
        let (tx, rx) = mpsc::channel();
        thread::Builder::new()
            .name("documents".into())
            .spawn(move || run(Library::new(pdfium_dirs), rx))
            .expect("impossible de démarrer le fil des documents");
        Self { tx }
    }

    pub fn open(&self, paths: Vec<PathBuf>) -> Receiver<OpenResult> {
        let (tx, rx) = mpsc::channel();
        let _ = self.tx.send(Job::Open(paths, tx));
        rx
    }

    pub fn close(&self, ids: Vec<DocId>) {
        let _ = self.tx.send(Job::Close(ids));
    }

    pub fn search(&self, query: String) -> Receiver<Vec<SearchHit>> {
        let (tx, rx) = mpsc::channel();
        let _ = self.tx.send(Job::Search(query, tx));
        rx
    }

    pub fn save_pdf(&self, pages: Vec<PageRef>, fills: Vec<DocFill>, dest: PathBuf) -> Receiver<Result<(), String>> {
        let (tx, rx) = mpsc::channel();
        let _ = self.tx.send(Job::SavePdf(pages, fills, dest, tx));
        rx
    }

    pub fn save_pdf_reduced(&self, pages: Vec<PageRef>, fills: Vec<DocFill>, level: Level, dest: PathBuf) -> Receiver<Result<(u64, u64), String>> {
        let (tx, rx) = mpsc::channel();
        let _ = self.tx.send(Job::SavePdfReduced(pages, fills, level, dest, tx));
        rx
    }

    pub fn form_fields(&self, doc: DocId) -> Receiver<Result<Vec<FormField>, String>> {
        let (tx, rx) = mpsc::channel();
        let _ = self.tx.send(Job::FormFields(doc, tx));
        rx
    }

    pub fn annotations(&self, doc: DocId) -> Receiver<Result<Vec<Vec<Placed>>, String>> {
        let (tx, rx) = mpsc::channel();
        let _ = self.tx.send(Job::Annotations(doc, tx));
        rx
    }

    pub fn retouch(&self, doc: DocId, retouch: Retouch) -> Receiver<Result<DocInfo, String>> {
        let (tx, rx) = mpsc::channel();
        let _ = self.tx.send(Job::Retouch(doc, retouch, tx));
        rx
    }

    pub fn save_picture(&self, doc: DocId, rotation: i32, dest: PathBuf) -> Receiver<Result<(), String>> {
        let (tx, rx) = mpsc::channel();
        let _ = self.tx.send(Job::SavePicture(doc, rotation, dest, tx));
        rx
    }

    pub fn save_picture_pixels(&self, doc: DocId, png: Vec<u8>, dest: PathBuf) -> Receiver<Result<(), String>> {
        let (tx, rx) = mpsc::channel();
        let _ = self.tx.send(Job::SavePicturePixels(doc, png, dest, tx));
        rx
    }

    pub fn page_text(&self, doc: DocId, page: usize) -> Receiver<Result<PageText, String>> {
        let (tx, rx) = mpsc::channel();
        let _ = self.tx.send(Job::PageText(doc, page, tx));
        rx
    }

    pub fn details(&self, doc: DocId) -> Receiver<Result<DocDetails, String>> {
        let (tx, rx) = mpsc::channel();
        let _ = self.tx.send(Job::Details(doc, tx));
        rx
    }

    pub fn render(&self, request: RenderRequest, reply: impl FnOnce(Result<RenderedImage, String>) + Send + 'static) {
        let _ = self.tx.send(Job::Render(request, Box::new(reply)));
    }
}

fn run(mut library: Library, rx: Receiver<Job>) {
    let mut pending: Vec<Job> = Vec::new();
    loop {
        if pending.is_empty() {
            match rx.recv() {
                Ok(job) => pending.push(job),
                Err(_) => return,
            }
        }
        pending.extend(rx.try_iter());
        // Ouvrir / fermer / chercher : dans l'ordre d'arrivée. Rendus : le plus récent d'abord.
        let job = match pending.iter().position(|j| !matches!(j, Job::Render(..))) {
            Some(i) => pending.remove(i),
            None => pending.pop().expect("au moins une tâche"),
        };
        match job {
            Job::Open(paths, reply) => {
                let mut result = OpenResult::default();
                for path in paths {
                    match library.open(&path) {
                        Ok(info) => result.docs.push(info),
                        Err(e) => result.errors.push(OpenError {
                            name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                            message: i18n::error(&e),
                        }),
                    }
                }
                let _ = reply.send(result);
            }
            Job::Close(ids) => ids.into_iter().for_each(|id| library.close(id)),
            Job::Search(query, reply) => {
                let _ = reply.send(library.search(&query));
            }
            Job::SavePdfReduced(pages, fills, level, dest, reply) => {
                let _ = reply.send(library.save_pdf_reduced(&pages, &fills, level, &dest).map_err(|e| i18n::error(&e)));
            }
            Job::SavePdf(pages, fills, dest, reply) => {
                let _ = reply.send(library.save_pdf_filled(&pages, &fills, &dest).map_err(|e| i18n::error(&e)));
            }
            Job::Retouch(doc, retouch, reply) => {
                let _ = reply.send(library.retouch_picture(doc, &retouch).map_err(|e| i18n::error(&e)));
            }
            Job::FormFields(doc, reply) => {
                let _ = reply.send(library.form_fields(doc).map_err(|e| i18n::error(&e)));
            }
            Job::Annotations(doc, reply) => {
                let _ = reply.send(library.annotations(doc).map_err(|e| i18n::error(&e)));
            }
            Job::SavePicture(doc, rotation, dest, reply) => {
                let _ = reply.send(library.save_picture(doc, rotation, &dest).map_err(|e| i18n::error(&e)));
            }
            Job::SavePicturePixels(doc, png, dest, reply) => {
                let _ = reply.send(library.save_picture_pixels(doc, &png, &dest).map_err(|e| i18n::error(&e)));
            }
            Job::Details(doc, reply) => {
                let _ = reply.send(library.details(doc).map_err(|e| i18n::error(&e)));
            }
            Job::PageText(doc, page, reply) => {
                let _ = reply.send(library.page_text(doc, page).map_err(|e| i18n::error(&e)));
            }
            Job::Render(r, reply) => reply(library.render_with(r.doc, r.page, r.width, r.rotation, r.forms).map_err(|e| i18n::error(&e))),
        }
    }
}
