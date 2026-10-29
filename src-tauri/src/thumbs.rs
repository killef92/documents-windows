//! Vignettes des images d'un dossier (bande sous une image seule), fabriquées à la demande.
//!
//! Plusieurs fils travaillent en parallèle et prennent toujours la demande la plus récente :
//! quand on fait défiler vite, ce qui est à l'écran passe avant ce qu'on a déjà dépassé.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};

pub struct Thumb {
    pub bytes: Vec<u8>,
    pub mime: &'static str,
}

type Key = (PathBuf, u32);
type Reply = Box<dyn FnOnce(Result<Arc<Thumb>, String>) + Send>;

/// Au-delà, les plus vieilles demandes sont abandonnées (l'interface redemande si besoin).
const MAX_PENDING: usize = 150;
/// Vignettes gardées en mémoire (environ 10 Ko chacune).
const MAX_CACHED: usize = 800;

#[derive(Default)]
struct State {
    queue: Vec<(Key, Reply)>,
    cache: HashMap<Key, Arc<Thumb>>,
    order: VecDeque<Key>,
}

pub struct Thumbs {
    shared: Arc<(Mutex<State>, Condvar)>,
}

impl Thumbs {
    pub fn start() -> Thumbs {
        let shared: Arc<(Mutex<State>, Condvar)> = Arc::default();
        let workers = std::thread::available_parallelism().map_or(2, |n| (n.get() / 2).clamp(2, 4));
        for _ in 0..workers {
            let shared = shared.clone();
            std::thread::spawn(move || loop {
                let (key, reply) = {
                    let (lock, ready) = &*shared;
                    let mut state = lock.lock().unwrap();
                    loop {
                        if let Some(job) = state.queue.pop() {
                            break job;
                        }
                        state = ready.wait(state).unwrap();
                    }
                };
                let result = documents_core::folder::thumbnail(&key.0, key.1)
                    .map(|(bytes, mime)| Arc::new(Thumb { bytes, mime }))
                    .map_err(|e| e.to_string());
                if let Ok(thumb) = &result {
                    let mut state = shared.0.lock().unwrap();
                    if state.cache.insert(key.clone(), thumb.clone()).is_none() {
                        state.order.push_back(key);
                        while state.order.len() > MAX_CACHED {
                            let old = state.order.pop_front().unwrap();
                            state.cache.remove(&old);
                        }
                    }
                }
                reply(result);
            });
        }
        Thumbs { shared }
    }

    /// Demande la vignette ; `reply` est appelé quand elle est prête (ou tout de suite si connue).
    pub fn get(&self, path: PathBuf, size: u32, reply: impl FnOnce(Result<Arc<Thumb>, String>) + Send + 'static) {
        let key = (path, size.clamp(32, 512));
        let (lock, ready) = &*self.shared;
        let mut state = lock.lock().unwrap();
        if let Some(thumb) = state.cache.get(&key).cloned() {
            drop(state);
            return reply(Ok(thumb));
        }
        state.queue.push((key, Box::new(reply)));
        let dropped: Vec<Reply> = if state.queue.len() > MAX_PENDING {
            let n = state.queue.len() - MAX_PENDING;
            state.queue.drain(..n).map(|(_, r)| r).collect()
        } else {
            Vec::new()
        };
        drop(state);
        ready.notify_one();
        for r in dropped {
            r(Err("cancelled".into()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn fabrique_puis_garde_en_memoire() {
        let thumbs = Thumbs::start();
        let photo = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/core/tests/fixtures/photo.png"));
        let (tx, rx) = mpsc::channel();
        let tx2 = tx.clone();
        thumbs.get(photo.clone(), 64, move |r| tx.send(r.map(|t| t.bytes.len())).unwrap());
        let first = rx.recv().unwrap().unwrap();
        assert!(first > 0);
        thumbs.get(photo, 64, move |r| tx2.send(r.map(|t| t.bytes.len())).unwrap());
        assert_eq!(rx.recv().unwrap().unwrap(), first);
        assert_eq!(thumbs.shared.0.lock().unwrap().cache.len(), 1);
    }
}
