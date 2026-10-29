//! Réorganiser les pages d'un PDF sans le reconstruire (avec lopdf) : l'ordre, les pages
//! supprimées et la rotation changent, tout le reste est gardé tel quel (formulaires,
//! liens, signets…).

use std::collections::HashSet;

use lopdf::{Dictionary, Document, Object, ObjectId};

use crate::Rotation;

/// Attributs qu'une page peut hériter de ses parents dans l'arbre des pages.
const INHERITED: [&[u8]; 4] = [b"Resources", b"MediaBox", b"CropBox", b"Rotate"];

pub(crate) fn inherited(doc: &Document, page: &Dictionary, key: &[u8]) -> Option<Object> {
    let mut parent = page.get(b"Parent").and_then(Object::as_reference).ok();
    let mut guard = 0;
    while let Some(id) = parent {
        let node = doc.get_dictionary(id).ok()?;
        if let Ok(value) = node.get(key) {
            return Some(value.clone());
        }
        parent = node.get(b"Parent").and_then(Object::as_reference).ok();
        guard += 1;
        if guard > 64 {
            return None;
        }
    }
    None
}

/// `order` : pour chaque page du résultat, son numéro d'origine (à partir de 0) et la rotation
/// ajoutée par l'utilisateur. Une page peut apparaître plusieurs fois.
pub(crate) fn rearrange(bytes: &[u8], order: &[(usize, Rotation)]) -> Result<Vec<u8>, String> {
    let mut doc = Document::load_mem(bytes).map_err(|e| e.to_string())?;
    if doc.is_encrypted() {
        return Err("encrypted PDF".into());
    }
    let pages = doc.get_pages();
    let root_pages: ObjectId = doc
        .catalog()
        .and_then(|c| c.get(b"Pages"))
        .and_then(Object::as_reference)
        .map_err(|e| e.to_string())?;

    let mut kids = Vec::with_capacity(order.len());
    let mut used = HashSet::new();
    for &(index, rotation) in order {
        let id = *pages.get(&(index as u32 + 1)).ok_or("page not found")?;
        let mut page = doc.get_dictionary(id).map_err(|e| e.to_string())?.clone();
        for key in INHERITED {
            if !page.has(key) {
                if let Some(value) = inherited(&doc, &page, key) {
                    page.set(key, value);
                }
            }
        }
        let current = page.get(b"Rotate").and_then(Object::as_i64).unwrap_or(0);
        page.set("Rotate", Object::Integer((current + rotation.degrees() as i64).rem_euclid(360)));
        page.set("Parent", Object::Reference(root_pages));
        let new_id = if used.insert(id) {
            doc.objects.insert(id, Object::Dictionary(page));
            id
        } else {
            doc.add_object(page)
        };
        kids.push(Object::Reference(new_id));
    }

    let root = doc.get_dictionary_mut(root_pages).map_err(|e| e.to_string())?;
    root.set("Count", Object::Integer(kids.len() as i64));
    root.set("Kids", Object::Array(kids));
    // Les pages supprimées et les nœuds intermédiaires ne sont plus utilisés : on les retire.
    doc.prune_objects();
    let mut out = Vec::new();
    doc.save_to(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}
