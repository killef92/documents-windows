//! Formulaires PDF (CERFA…) : lire les champs, puis les remplir à l'enregistrement.
//!
//! On travaille directement sur le fichier avec lopdf : la valeur de chaque champ est écrite,
//! et son apparence (ce qui s'affiche) est redessinée ici, pour que le formulaire rempli
//! s'affiche pareil dans tous les lecteurs, y compris ceux qui ne recalculent rien.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as _;

use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};
use serde::{Deserialize, Serialize};

use crate::annotate::{literal, n, page_box, page_rotation, reals, text_string, win_ansi, Frame, Rect};
use crate::Rotation;

/// Marge intérieure du texte dans un champ, en points (comme Acrobat).
pub const FIELD_PAD: f32 = 2.0;
/// Interligne des champs sur plusieurs lignes, en proportion de la taille.
pub const FIELD_LINE_HEIGHT: f32 = 1.15;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldKind {
    Text,
    Checkbox,
    Radio,
    /// Liste déroulante.
    Choice,
}

/// Un emplacement de champ sur une page (un bouton radio a plusieurs emplacements).
#[derive(Debug, Clone, Serialize)]
pub struct FormField {
    /// Identifiant du champ (le même pour tous les boutons d'un groupe radio).
    pub id: String,
    pub name: String,
    pub kind: FieldKind,
    pub page: usize,
    /// Place sur la page, en unités de page (comme les annotations).
    pub rect: Rect,
    /// Texte du champ, choix de la liste, ou bouton coché du groupe (vide si aucun).
    pub value: String,
    /// Case à cocher / bouton radio : le nom de l'état « coché » de cet emplacement.
    pub on: String,
    pub options: Vec<String>,
    pub multiline: bool,
    pub comb: bool,
    pub max_len: Option<u32>,
    pub read_only: bool,
    /// Taille du texte en points (0 = automatique).
    pub font_size: f32,
    pub courier: bool,
    /// 0 à gauche, 1 centré, 2 à droite.
    pub align: u8,
    pub color: String,
    /// Échelle unités de page / points (pour convertir les tailles).
    pub units_per_point: f32,
}

/// Une ligne de texte déjà mise en page par l'interface (largeur en points).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct TextLine {
    pub text: String,
    pub width: f32,
    /// Largeur de chaque caractère, pour les champs « en cases ».
    #[serde(default)]
    pub chars: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum FillValue {
    /// Champ texte ou liste déroulante. `size` en points.
    Text { text: String, lines: Vec<TextLine>, size: f32 },
    Check { on: bool },
    Radio { value: Option<String> },
}

/// Les champs remplis d'un document.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct DocFill {
    pub doc: crate::DocId,
    pub fields: Vec<FieldFill>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct FieldFill {
    pub id: String,
    #[serde(flatten)]
    pub value: FillValue,
}

const READ_ONLY: i64 = 1;
const MULTILINE: i64 = 1 << 12;
const RADIO: i64 = 1 << 15;
const PUSH_BUTTON: i64 = 1 << 16;
const COMB: i64 = 1 << 24;

fn id_string((num, gen): ObjectId) -> String {
    format!("{num}-{gen}")
}

fn parse_id(s: &str) -> Option<ObjectId> {
    let (a, b) = s.split_once('-')?;
    Some((a.parse().ok()?, b.parse().ok()?))
}

/// Valeur d'un attribut de champ, en remontant aux parents si besoin.
fn field_attr(doc: &Document, field: &Dictionary, key: &[u8]) -> Option<Object> {
    let mut dict = field.clone();
    for _ in 0..32 {
        if let Ok(v) = dict.get(key) {
            return doc.dereference(v).ok().map(|(_, o)| o.clone());
        }
        let parent = dict.get(b"Parent").and_then(Object::as_reference).ok()?;
        dict = doc.get_dictionary(parent).ok()?.clone();
    }
    None
}

/// Texte d'une chaîne PDF (UTF-16 avec BOM, UTF-8 avec BOM, ou PDFDocEncoding ≈ Latin-1).
pub(crate) fn decode_text(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let units: Vec<u16> = bytes[2..].as_chunks::<2>().0.iter().map(|c| u16::from_be_bytes(*c)).collect();
        return String::from_utf16_lossy(&units);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(rest).into_owned();
    }
    bytes.iter().map(|&b| b as char).collect()
}

fn text_of(o: &Object) -> Option<String> {
    match o {
        Object::String(bytes, _) => Some(decode_text(bytes)),
        Object::Name(name) => Some(String::from_utf8_lossy(name).into_owned()),
        _ => None,
    }
}

/// Le nom de l'état « coché » d'un emplacement de case ou de bouton radio.
fn on_state(doc: &Document, widget: &Dictionary) -> String {
    let normal = widget
        .get(b"AP")
        .ok()
        .and_then(|ap| doc.dereference(ap).ok())
        .and_then(|(_, ap)| ap.as_dict().ok())
        .and_then(|ap| ap.get(b"N").ok())
        .and_then(|nn| doc.dereference(nn).ok())
        .and_then(|(_, nn)| nn.as_dict().ok().cloned());
    normal
        .and_then(|d| d.iter().map(|(k, _)| k.clone()).find(|k| k != b"Off"))
        .map(|k| String::from_utf8_lossy(&k).into_owned())
        .unwrap_or_else(|| "Yes".into())
}

/// Ce que dit /DA (« /Helv 10 Tf 0 g ») : police, taille et couleur.
fn parse_da(da: &str) -> (String, f32, String) {
    let parts: Vec<&str> = da.split_whitespace().collect();
    let mut font = String::new();
    let mut size = 0.0;
    let mut color = "#000000".to_string();
    for (i, p) in parts.iter().enumerate() {
        match *p {
            "Tf" if i >= 2 => {
                font = parts[i - 2].trim_start_matches('/').to_string();
                size = parts[i - 1].parse().unwrap_or(0.0);
            }
            "g" if i >= 1 => {
                let g = (parts[i - 1].parse::<f32>().unwrap_or(0.0) * 255.0) as u8;
                color = format!("#{g:02x}{g:02x}{g:02x}");
            }
            "rg" if i >= 3 => {
                let c = |k: usize| (parts[i - k].parse::<f32>().unwrap_or(0.0).clamp(0.0, 1.0) * 255.0) as u8;
                color = format!("#{:02x}{:02x}{:02x}", c(3), c(2), c(1));
            }
            _ => {}
        }
    }
    (font, size, color)
}

/// Les champs terminaux (ceux qui ont une valeur) et leurs emplacements (widgets).
fn terminal_fields(doc: &Document) -> Vec<(ObjectId, Vec<ObjectId>)> {
    let Some(form) = acro_form(doc) else { return Vec::new() };
    let Ok(fields) = form.get(b"Fields").and_then(|f| doc.dereference(f).map(|(_, o)| o)).and_then(Object::as_array) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut stack: Vec<ObjectId> = fields.iter().filter_map(|f| f.as_reference().ok()).rev().collect();
    let mut seen = HashSet::new();
    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        let Ok(dict) = doc.get_dictionary(id) else { continue };
        let kids: Vec<ObjectId> = dict
            .get(b"Kids")
            .ok()
            .and_then(|k| doc.dereference(k).ok())
            .and_then(|(_, k)| k.as_array().ok().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|k| k.as_reference().ok())
            .collect();
        // Des enfants avec un nom (/T) sont des sous-champs ; sans nom, ce sont les emplacements.
        let sub_fields: Vec<ObjectId> =
            kids.iter().copied().filter(|k| doc.get_dictionary(*k).is_ok_and(|d| d.has(b"T"))).collect();
        if !sub_fields.is_empty() {
            stack.extend(sub_fields.into_iter().rev());
            continue;
        }
        let widgets = if kids.is_empty() { vec![id] } else { kids };
        out.push((id, widgets));
    }
    out
}

fn acro_form(doc: &Document) -> Option<&Dictionary> {
    let catalog = doc.catalog().ok()?;
    let form = catalog.get(b"AcroForm").ok()?;
    doc.dereference(form).ok()?.1.as_dict().ok()
}

fn full_name(doc: &Document, id: ObjectId) -> String {
    let mut parts = Vec::new();
    let mut current = Some(id);
    for _ in 0..32 {
        let Some(cid) = current else { break };
        let Ok(d) = doc.get_dictionary(cid) else { break };
        if let Ok(t) = d.get(b"T") {
            if let Some(t) = text_of(t) {
                parts.push(t);
            }
        }
        current = d.get(b"Parent").and_then(Object::as_reference).ok();
    }
    parts.reverse();
    parts.join(".")
}

/// Pour chaque annotation, la page où elle se trouve.
fn widget_pages(doc: &Document) -> HashMap<ObjectId, (usize, ObjectId)> {
    let mut map = HashMap::new();
    for (i, (_, page_id)) in doc.get_pages().into_iter().enumerate() {
        let Ok(page) = doc.get_dictionary(page_id) else { continue };
        let Ok(annots) = page.get(b"Annots") else { continue };
        let Ok((_, Object::Array(annots))) = doc.dereference(annots) else { continue };
        for a in annots {
            if let Ok(id) = a.as_reference() {
                map.insert(id, (i, page_id));
            }
        }
    }
    map
}

fn rect_of(doc: &Document, widget: &Dictionary) -> Option<[f32; 4]> {
    let (_, r) = doc.dereference(widget.get(b"Rect").ok()?).ok()?;
    let v: Vec<f32> = r.as_array().ok()?.iter().filter_map(|o| crate::annotate::number(doc, o)).collect();
    (v.len() == 4).then(|| [v[0].min(v[2]), v[1].min(v[3]), v[0].max(v[2]), v[1].max(v[3])])
}

fn flags(doc: &Document, field: &Dictionary) -> i64 {
    field_attr(doc, field, b"Ff").and_then(|o| o.as_i64().ok()).unwrap_or(0)
}

/// Lit tous les champs du formulaire d'un PDF.
pub(crate) fn fields(bytes: &[u8]) -> Vec<FormField> {
    let Ok(doc) = Document::load_mem(bytes) else { return Vec::new() };
    let pages = widget_pages(&doc);
    let default_da = acro_form(&doc)
        .and_then(|f| f.get(b"DA").ok())
        .and_then(text_of)
        .unwrap_or_default();
    let mut out = Vec::new();
    let mut frames: BTreeMap<usize, Frame> = BTreeMap::new();
    for (field_id, widgets) in terminal_fields(&doc) {
        let Ok(field) = doc.get_dictionary(field_id) else { continue };
        let ft = field_attr(&doc, field, b"FT").and_then(|o| o.as_name().ok().map(<[u8]>::to_vec));
        let ff = flags(&doc, field);
        let kind = match ft.as_deref() {
            Some(b"Tx") => FieldKind::Text,
            Some(b"Ch") => FieldKind::Choice,
            Some(b"Btn") if ff & PUSH_BUTTON != 0 => continue,
            Some(b"Btn") if ff & RADIO != 0 => FieldKind::Radio,
            Some(b"Btn") => FieldKind::Checkbox,
            _ => continue, // signatures, boutons…
        };
        let value = field_attr(&doc, field, b"V").and_then(|v| match v {
            Object::Array(a) => a.first().and_then(text_of),
            v => text_of(&v),
        });
        let value = match (kind, value) {
            (FieldKind::Checkbox | FieldKind::Radio, Some(v)) if v == "Off" => String::new(),
            (_, v) => v.unwrap_or_default(),
        };
        let options: Vec<String> = field_attr(&doc, field, b"Opt")
            .and_then(|o| o.as_array().ok().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|o| match doc.dereference(o).ok()?.1 {
                Object::Array(pair) => pair.get(1).and_then(text_of),
                o => text_of(o),
            })
            .collect();
        let max_len = field_attr(&doc, field, b"MaxLen").and_then(|o| o.as_i64().ok()).map(|v| v.max(0) as u32);
        let align = field_attr(&doc, field, b"Q").and_then(|o| o.as_i64().ok()).unwrap_or(0).clamp(0, 2) as u8;
        let name = full_name(&doc, field_id);
        for widget_id in widgets {
            let Ok(widget) = doc.get_dictionary(widget_id) else { continue };
            let Some(&(page, page_id)) = pages.get(&widget_id) else { continue };
            let Some(rect) = rect_of(&doc, widget) else { continue };
            let frame = frames.entry(page).or_insert_with(|| {
                let page_dict = doc.get_dictionary(page_id).cloned().unwrap_or_default();
                let bbox = page_box(&doc, &page_dict).unwrap_or([0.0, 0.0, 612.0, 792.0]);
                let r = Rotation::from_degrees(page_rotation(&doc, &page_dict));
                let dw = if r.is_sideways() { bbox[3] - bbox[1] } else { bbox[2] - bbox[0] };
                Frame::new(bbox, r, dw * 4.0 / 3.0)
            });
            let a = frame.units((rect[0], rect[1]));
            let b = frame.units((rect[2], rect[3]));
            let da = widget
                .get(b"DA")
                .ok()
                .and_then(text_of)
                .or_else(|| field_attr(&doc, field, b"DA").as_ref().and_then(text_of))
                .unwrap_or_else(|| default_da.clone());
            let (font, font_size, color) = parse_da(&da);
            let on = if matches!(kind, FieldKind::Checkbox | FieldKind::Radio) { on_state(&doc, widget) } else { String::new() };
            out.push(FormField {
                id: id_string(field_id),
                name: name.clone(),
                kind,
                page,
                rect: Rect { x: a.x.min(b.x), y: a.y.min(b.y), w: (a.x - b.x).abs(), h: (a.y - b.y).abs() },
                value: value.clone(),
                on,
                options: options.clone(),
                multiline: ff & MULTILINE != 0,
                comb: ff & COMB != 0 && max_len.is_some(),
                max_len,
                read_only: ff & READ_ONLY != 0,
                font_size,
                courier: font.starts_with("Co"),
                align,
                color,
                units_per_point: 4.0 / 3.0,
            });
        }
    }
    out
}

fn color_ops(c: &Object, stroke: bool) -> Option<String> {
    let v: Vec<f32> = c
        .as_array()
        .ok()?
        .iter()
        .filter_map(|o| o.as_float().ok().or_else(|| o.as_i64().ok().map(|i| i as f32)))
        .collect();
    let op = |s: &str| if stroke { s.to_uppercase() } else { s.to_string() };
    match v.len() {
        1 => Some(format!("{} {}", n(v[0]), op("g"))),
        3 => Some(format!("{} {} {} {}", n(v[0]), n(v[1]), n(v[2]), op("rg"))),
        4 => Some(format!("{} {} {} {} {}", n(v[0]), n(v[1]), n(v[2]), n(v[3]), op("k"))),
        _ => None,
    }
}

/// Dessine l'apparence d'un champ texte rempli.
fn text_appearance(doc: &Document, field: &Dictionary, widget: &Dictionary, lines: &[TextLine], size: f32) -> Option<Stream> {
    let r = rect_of(doc, widget)?;
    let (w, h) = (r[2] - r[0], r[3] - r[1]);
    let mk = widget.get(b"MK").ok().and_then(|m| doc.dereference(m).ok()).and_then(|(_, m)| m.as_dict().ok().cloned());
    let border = widget
        .get(b"BS")
        .ok()
        .and_then(|b| doc.dereference(b).ok())
        .and_then(|(_, b)| b.as_dict().ok().cloned())
        .and_then(|b| b.get(b"W").ok().and_then(|o| o.as_float().ok().or_else(|| o.as_i64().ok().map(|i| i as f32))))
        .unwrap_or(1.0);
    let mut ops = String::new();
    let mut bw = 0.0;
    if let Some(mk) = &mk {
        if let Some(bg) = mk.get(b"BG").ok().and_then(|c| color_ops(c, false)) {
            let _ = writeln!(ops, "{bg} 0 0 {} {} re f", n(w), n(h));
        }
        if let Some(bc) = mk.get(b"BC").ok().and_then(|c| color_ops(c, true)) {
            if border > 0.0 {
                bw = border;
                let _ = writeln!(ops, "{bc} {} w {} {} {} {} re S", n(bw), n(bw / 2.0), n(bw / 2.0), n(w - bw), n(h - bw));
            }
        }
    }
    let da = widget
        .get(b"DA")
        .ok()
        .and_then(text_of)
        .or_else(|| field_attr(doc, field, b"DA").as_ref().and_then(text_of))
        .unwrap_or_default();
    let (font, _, color) = parse_da(&da);
    let courier = font.starts_with("Co");
    let hex = u32::from_str_radix(color.trim_start_matches('#'), 16).unwrap_or(0);
    let (cr, cg, cb) = ((hex >> 16) as f32 / 255.0, ((hex >> 8) & 0xFF) as f32 / 255.0, (hex & 0xFF) as f32 / 255.0);
    let ff = flags(doc, field);
    let align = field_attr(doc, field, b"Q").and_then(|o| o.as_i64().ok()).unwrap_or(0);
    let max_len = field_attr(doc, field, b"MaxLen").and_then(|o| o.as_i64().ok()).unwrap_or(0);
    let pad = FIELD_PAD + bw;

    let _ = writeln!(ops, "/Tx BMC q {} {} {} {} re W n", n(bw), n(bw), n(w - 2.0 * bw), n(h - 2.0 * bw));
    let _ = writeln!(ops, "BT /FA {} Tf {} {} {} rg", n(size), n(cr), n(cg), n(cb));
    if ff & COMB != 0 && max_len > 0 {
        // Champ « en cases » : un caractère centré dans chaque case.
        let cell = w / max_len as f32;
        let y = h / 2.0 - size * 0.3;
        if let Some(line) = lines.first() {
            for (i, ch) in line.text.chars().take(max_len as usize).enumerate() {
                let cw = line.chars.get(i).copied().unwrap_or(size * 0.6);
                let x = i as f32 * cell + (cell - cw) / 2.0;
                let _ = writeln!(ops, "1 0 0 1 {} {} Tm {} Tj", n(x), n(y), literal(&win_ansi(&ch.to_string())));
            }
        }
    } else {
        let multiline = ff & MULTILINE != 0;
        for (i, line) in lines.iter().enumerate() {
            let x = match align {
                1 => (w - line.width) / 2.0,
                2 => w - pad - line.width,
                _ => pad,
            };
            let y = if multiline {
                h - pad - size * 0.92 - i as f32 * size * FIELD_LINE_HEIGHT
            } else {
                h / 2.0 - size * 0.3
            };
            let _ = writeln!(ops, "1 0 0 1 {} {} Tm {} Tj", n(x), n(y), literal(&win_ansi(&line.text)));
            if !multiline {
                break;
            }
        }
    }
    ops.push_str("ET Q EMC\n");
    let font_name = if courier { "Courier" } else { "Helvetica" };
    Some(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => reals(&[0.0, 0.0, w, h]),
            "Resources" => dictionary! {
                "Font" => dictionary! {
                    "FA" => dictionary! {
                        "Type" => "Font",
                        "Subtype" => "Type1",
                        "BaseFont" => font_name,
                        "Encoding" => "WinAnsiEncoding",
                    },
                },
            },
        },
        ops.into_bytes(),
    ))
}

fn widgets_of(doc: &Document, field_id: ObjectId) -> Vec<ObjectId> {
    let Ok(field) = doc.get_dictionary(field_id) else { return Vec::new() };
    let kids: Vec<ObjectId> = field
        .get(b"Kids")
        .ok()
        .and_then(|k| doc.dereference(k).ok())
        .and_then(|(_, k)| k.as_array().ok().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|k| k.as_reference().ok())
        .collect();
    if kids.is_empty() { vec![field_id] } else { kids }
}

/// Une valeur de champ en chaîne PDF (Latin-1 si possible, sinon UTF-16).
fn pdf_text(s: &str) -> Object {
    if s.chars().all(|c| (c as u32) < 0x80) {
        Object::string_literal(s)
    } else {
        text_string(s)
    }
}

/// Écrit les valeurs dans le formulaire d'un PDF (le fichier en mémoire, pas l'original).
pub(crate) fn fill(bytes: &[u8], fills: &[FieldFill]) -> Result<Vec<u8>, String> {
    let mut doc = Document::load_mem(bytes).map_err(|e| e.to_string())?;
    if doc.is_encrypted() {
        return Err("PDF chiffré".into());
    }
    for f in fills {
        let Some(field_id) = parse_id(&f.id) else { continue };
        let Ok(field) = doc.get_dictionary(field_id).cloned() else { continue };
        let widgets = widgets_of(&doc, field_id);
        match &f.value {
            FillValue::Text { text, lines, size } => {
                let mut appearances = Vec::new();
                for &w in &widgets {
                    let Ok(widget) = doc.get_dictionary(w) else { continue };
                    if let Some(ap) = text_appearance(&doc, &field, widget, lines, *size) {
                        appearances.push((w, ap));
                    }
                }
                for (w, ap) in appearances {
                    let ap_id = doc.add_object(ap);
                    if let Ok(widget) = doc.get_dictionary_mut(w) {
                        widget.set("AP", dictionary! { "N" => Object::Reference(ap_id) });
                    }
                }
                if let Ok(field) = doc.get_dictionary_mut(field_id) {
                    field.set("V", pdf_text(text));
                }
            }
            FillValue::Check { on } => {
                for &w in &widgets {
                    let state = doc.get_dictionary(w).map(|d| on_state(&doc, d)).unwrap_or_else(|_| "Yes".into());
                    let value = if *on { state } else { "Off".into() };
                    if let Ok(widget) = doc.get_dictionary_mut(w) {
                        widget.set("AS", Object::Name(value.clone().into_bytes()));
                    }
                    if let Ok(field) = doc.get_dictionary_mut(field_id) {
                        field.set("V", Object::Name(value.into_bytes()));
                    }
                }
            }
            FillValue::Radio { value } => {
                let chosen = value.clone().unwrap_or_else(|| "Off".into());
                for &w in &widgets {
                    let state = doc.get_dictionary(w).map(|d| on_state(&doc, d)).unwrap_or_default();
                    let shown = if state == chosen { state } else { "Off".into() };
                    if let Ok(widget) = doc.get_dictionary_mut(w) {
                        widget.set("AS", Object::Name(shown.into_bytes()));
                    }
                }
                if let Ok(field) = doc.get_dictionary_mut(field_id) {
                    field.set("V", Object::Name(chosen.into_bytes()));
                }
            }
        }
    }
    // Un formulaire XFA (ancien format d'Adobe) masquerait nos valeurs dans Acrobat : on le retire.
    if let Ok(catalog) = doc.catalog() {
        if let Ok(form) = catalog.get(b"AcroForm") {
            let form = form.clone();
            match form {
                Object::Reference(id) => {
                    if let Ok(d) = doc.get_dictionary_mut(id) {
                        d.remove(b"XFA");
                    }
                }
                Object::Dictionary(mut d) => {
                    d.remove(b"XFA");
                    if let Ok(catalog) = doc.catalog_mut() {
                        catalog.set("AcroForm", Object::Dictionary(d));
                    }
                }
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    doc.save_to(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}

/// Après une fusion, les champs n'appartiennent plus à aucun formulaire : on les dessine
/// directement dans la page (« aplatir »), pour qu'ils restent visibles partout.
pub(crate) fn flatten_orphan_widgets(bytes: Vec<u8>) -> Result<Vec<u8>, String> {
    let mut doc = Document::load_mem(&bytes).map_err(|e| e.to_string())?;
    if acro_form(&doc).is_some() {
        return Ok(bytes);
    }
    let mut changed = false;
    for (_, page_id) in doc.get_pages() {
        let Ok(page) = doc.get_dictionary(page_id).cloned() else { continue };
        let Ok(annots) = page.get(b"Annots") else { continue };
        let Ok((_, Object::Array(annots))) = doc.dereference(annots).map(|(i, o)| (i, o.clone())) else { continue };
        let mut keep = Vec::new();
        let mut draws = String::new();
        let mut xobjects = Vec::new();
        for a in &annots {
            let Some(widget) = a.as_reference().ok().and_then(|id| doc.get_dictionary(id).ok()) else {
                keep.push(a.clone());
                continue;
            };
            if widget.get(b"Subtype").and_then(Object::as_name).ok() != Some(b"Widget".as_slice()) {
                keep.push(a.clone());
                continue;
            }
            let hidden = widget.get(b"F").and_then(Object::as_i64).unwrap_or(0) & 2 != 0;
            let appearance = appearance_stream(&doc, widget);
            let (Some(rect), Some(ap_id), false) = (rect_of(&doc, widget), appearance, hidden) else { continue };
            let Ok(ap) = doc.get_object(ap_id).and_then(Object::as_stream) else { continue };
            let nums = |key: &[u8]| -> Option<Vec<f32>> {
                let (_, o) = doc.dereference(ap.dict.get(key).ok()?).ok()?;
                Some(o.as_array().ok()?.iter().filter_map(|v| crate::annotate::number(&doc, v)).collect())
            };
            let Some(bbox) = nums(b"BBox").filter(|b| b.len() == 4) else { continue };
            let m = nums(b"Matrix").filter(|m| m.len() == 6).unwrap_or(vec![1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
            // La boîte transformée par la matrice du dessin, puis ramenée sur l'emplacement.
            let pts = [(bbox[0], bbox[1]), (bbox[2], bbox[1]), (bbox[0], bbox[3]), (bbox[2], bbox[3])]
                .map(|(x, y)| (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]));
            let (tx0, tx1) = (pts.iter().map(|p| p.0).fold(f32::MAX, f32::min), pts.iter().map(|p| p.0).fold(f32::MIN, f32::max));
            let (ty0, ty1) = (pts.iter().map(|p| p.1).fold(f32::MAX, f32::min), pts.iter().map(|p| p.1).fold(f32::MIN, f32::max));
            if tx1 - tx0 <= 0.0 || ty1 - ty0 <= 0.0 {
                continue;
            }
            let (sx, sy) = ((rect[2] - rect[0]) / (tx1 - tx0), (rect[3] - rect[1]) / (ty1 - ty0));
            let name = format!("DocumentsChamp{}", xobjects.len());
            let _ = writeln!(
                draws,
                "q {} 0 0 {} {} {} cm /{name} Do Q",
                n(sx),
                n(sy),
                n(rect[0] - tx0 * sx),
                n(rect[1] - ty0 * sy)
            );
            xobjects.push((name, ap_id));
        }
        if xobjects.is_empty() {
            continue;
        }
        changed = true;
        // Ressources de la page (copiées sur la page si elles étaient partagées ou héritées).
        let mut resources = page
            .get(b"Resources")
            .ok()
            .cloned()
            .or_else(|| crate::rearrange::inherited(&doc, &page, b"Resources"))
            .and_then(|r| doc.dereference(&r).ok().and_then(|(_, o)| o.as_dict().ok().cloned()))
            .unwrap_or_default();
        let mut xobject_dict = resources
            .get(b"XObject")
            .ok()
            .and_then(|x| doc.dereference(x).ok())
            .and_then(|(_, x)| x.as_dict().ok().cloned())
            .unwrap_or_default();
        for (name, id) in xobjects {
            xobject_dict.set(name, Object::Reference(id));
        }
        resources.set("XObject", Object::Dictionary(xobject_dict));
        // Le contenu d'origine est entouré de q/Q pour que nos dessins partent d'un état propre.
        let before = doc.add_object(Stream::new(Dictionary::new(), b"q\n".to_vec()));
        let after = doc.add_object(Stream::new(Dictionary::new(), format!("Q\n{draws}").into_bytes()));
        let mut contents = vec![Object::Reference(before)];
        match page.get(b"Contents").ok().map(|c| doc.dereference(c).map(|(i, o)| (i, o.clone()))) {
            Some(Ok((_, Object::Array(a)))) => contents.extend(a),
            Some(Ok((Some(id), _))) => contents.push(Object::Reference(id)),
            Some(Ok((None, Object::Stream(s)))) => contents.push(Object::Reference(doc.add_object(s))),
            _ => {}
        }
        contents.push(Object::Reference(after));
        let page = doc.get_dictionary_mut(page_id).map_err(|e| e.to_string())?;
        page.set("Resources", Object::Dictionary(resources));
        page.set("Contents", Object::Array(contents));
        page.set("Annots", Object::Array(keep));
    }
    if !changed {
        return Ok(bytes);
    }
    let mut out = Vec::new();
    doc.save_to(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}

/// Le dessin « normal » d'un emplacement (selon son état /AS pour une case à cocher).
fn appearance_stream(doc: &Document, widget: &Dictionary) -> Option<ObjectId> {
    let (_, ap) = doc.dereference(widget.get(b"AP").ok()?).ok()?;
    let normal = ap.as_dict().ok()?.get(b"N").ok()?;
    match normal {
        Object::Reference(id) => match doc.get_object(*id).ok()? {
            Object::Stream(_) => Some(*id),
            Object::Dictionary(states) => pick_state(doc, widget, states),
            _ => None,
        },
        Object::Dictionary(states) => pick_state(doc, widget, states),
        _ => None,
    }
}

fn pick_state(doc: &Document, widget: &Dictionary, states: &Dictionary) -> Option<ObjectId> {
    let state = widget.get(b"AS").and_then(Object::as_name).ok()?;
    let _ = doc;
    states.get(state).ok()?.as_reference().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lit_la_description_du_texte() {
        assert_eq!(parse_da("/Helv 10 Tf 0 g"), ("Helv".into(), 10.0, "#000000".into()));
        assert_eq!(parse_da("/Cour 0 Tf 1 0 0 rg").2, "#ff0000");
        assert_eq!(parse_da("").1, 0.0);
    }

    #[test]
    fn decode_les_textes_pdf() {
        assert_eq!(decode_text(b"Rh\xf4ne"), "Rhône");
        assert_eq!(decode_text(&[0xFE, 0xFF, 0x00, 0xE9]), "é");
        assert_eq!(parse_id("12-0"), Some((12, 0)));
        assert_eq!(parse_id("x"), None);
    }
}
