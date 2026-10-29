//! Relire les annotations d'un PDF à l'ouverture, pour qu'elles redeviennent modifiables.
//!
//! Celles que l'appli sait représenter (les siennes, et les surlignages, rectangles, ovales,
//! traits, flèches, dessins, zones de texte et notes des autres logiciels) sont détachées des
//! pages affichées : l'interface les dessine elle-même, comme celles qu'on vient d'ajouter.
//! Les autres (liens, champs, tampons, nuages, pointillés…) restent dans la page, telles quelles.
//! À l'enregistrement, une annotation qu'on n'a pas touchée est recopiée à l'identique
//! (voir `annotate::apply`) : aucune perte, et pas de doublon.

use std::collections::HashSet;

use image::{DynamicImage, RgbaImage};
use lopdf::{Dictionary, Document, Object, ObjectId};

use crate::annotate::{self, Align, Annotation, Color, Font, Frame, Point, Rect, Saved, Shape, DATA_KEY};
use crate::form::{decode_text, parse_da};
use crate::Rotation;

/// Une annotation reprise du PDF : son objet dans le fichier, et ce qu'elle devient dans l'appli.
#[derive(Debug, Clone)]
pub(crate) struct Imported {
    pub id: ObjectId,
    pub annotation: Annotation,
}

/// Le résultat de la lecture.
pub(crate) struct Reading {
    /// Le PDF, sans les annotations reprises dans la liste des annotations de leur page (leurs
    /// objets restent dans le fichier, pour pouvoir les recopier à l'enregistrement).
    pub bytes: Vec<u8>,
    /// Les annotations reprises, page par page.
    pub pages: Vec<Vec<Imported>>,
}

/// Indicateurs d'annotation qui empêchent de la reprendre : invisible, cachée, jamais affichée,
/// en lecture seule, verrouillée.
const UNTOUCHABLE: i64 = 1 | 2 | 32 | 64 | 128;
/// Au-delà, une image de signature est jugée anormale (pas de « bombe » de décompression).
const MAX_IMAGE_BYTES: usize = 64 << 20;

/// Lit les annotations d'un PDF. `widths` : largeur de chaque page à 100 % (rotation du PDF
/// comprise), en unités de page. `None` s'il n'y a rien à reprendre, ou si le PDF ne s'y prête
/// pas (chiffré, illisible…) : il s'affiche alors comme avant, annotations comprises.
pub(crate) fn read(bytes: &[u8], widths: &[f32]) -> Option<Reading> {
    // Un PDF abîmé ne doit jamais empêcher de l'ouvrir.
    std::panic::catch_unwind(|| read_pdf(bytes, widths)).ok().flatten()
}

fn read_pdf(bytes: &[u8], widths: &[f32]) -> Option<Reading> {
    let mut doc = Document::load_mem(bytes).ok()?;
    if doc.is_encrypted() || doc.trailer.has(b"Encrypt") {
        return None;
    }
    let page_ids: Vec<ObjectId> = doc.get_pages().into_values().collect();
    if page_ids.len() != widths.len() {
        return None;
    }
    let mut pages = Vec::with_capacity(page_ids.len());
    let mut changed: Vec<(ObjectId, Vec<Object>)> = Vec::new();
    for (index, &page_id) in page_ids.iter().enumerate() {
        let Ok(page) = doc.get_dictionary(page_id) else {
            pages.push(Vec::new());
            continue;
        };
        let annots = annots_of(&doc, page);
        if annots.is_empty() {
            pages.push(Vec::new());
            continue;
        }
        let bbox = annotate::page_box(&doc, page).unwrap_or([0.0, 0.0, 612.0, 792.0]);
        let frame = Frame::new(bbox, Rotation::from_degrees(annotate::page_rotation(&doc, page)), widths[index]);
        // Une annotation à laquelle on a répondu reste telle quelle, avec ses réponses.
        let replied: HashSet<ObjectId> = annots
            .iter()
            .filter_map(|o| doc.get_dictionary(o.as_reference().ok()?).ok()?.get(b"IRT").ok()?.as_reference().ok())
            .collect();
        let mut found = Vec::new();
        let mut taken = HashSet::new();
        for o in &annots {
            let Ok(id) = o.as_reference() else { continue };
            let Ok(dict) = doc.get_dictionary(id) else { continue };
            if replied.contains(&id) {
                continue;
            }
            let Some(annotation) = convert(&doc, dict, &frame) else { continue };
            taken.insert(id);
            found.push(Imported { id, annotation });
        }
        if taken.is_empty() {
            pages.push(Vec::new());
            continue;
        }
        // Leurs bulles de commentaire partent avec elles (et reviennent avec elles).
        let kept: Vec<Object> = annots
            .into_iter()
            .filter(|o| {
                let Ok(id) = o.as_reference() else { return true };
                if taken.contains(&id) {
                    return false;
                }
                let parent = doc.get_dictionary(id).ok().and_then(|d| d.get(b"Parent").ok()?.as_reference().ok());
                !(is_popup(&doc, id) && parent.is_some_and(|p| taken.contains(&p)))
            })
            .collect();
        changed.push((page_id, kept));
        pages.push(found);
    }
    if changed.is_empty() {
        return None;
    }
    for (page_id, kept) in changed {
        let page = doc.get_dictionary_mut(page_id).ok()?;
        if kept.is_empty() {
            page.remove(b"Annots");
        } else {
            page.set("Annots", Object::Array(kept));
        }
    }
    let mut out = Vec::new();
    doc.save_to(&mut out).ok()?;
    Some(Reading { bytes: out, pages })
}

fn annots_of(doc: &Document, page: &Dictionary) -> Vec<Object> {
    match page.get(b"Annots").ok().and_then(|o| doc.dereference(o).ok()) {
        Some((_, Object::Array(a))) => a.clone(),
        _ => Vec::new(),
    }
}

fn is_popup(doc: &Document, id: ObjectId) -> bool {
    doc.get_dictionary(id).ok().and_then(|d| name(doc, d, b"Subtype")).is_some_and(|s| s == b"Popup")
}

/// La valeur d'une clé, en suivant les renvois.
fn get<'a>(doc: &'a Document, d: &'a Dictionary, key: &[u8]) -> Option<&'a Object> {
    doc.dereference(d.get(key).ok()?).ok().map(|(_, o)| o)
}

fn name<'a>(doc: &'a Document, d: &'a Dictionary, key: &[u8]) -> Option<&'a [u8]> {
    get(doc, d, key)?.as_name().ok()
}

fn num(doc: &Document, d: &Dictionary, key: &[u8]) -> Option<f32> {
    annotate::number(doc, get(doc, d, key)?)
}

fn nums(doc: &Document, o: &Object) -> Option<Vec<f32>> {
    match doc.dereference(o).ok()?.1 {
        Object::Array(a) => a.iter().map(|o| annotate::number(doc, o)).collect(),
        _ => None,
    }
}

fn text(doc: &Document, d: &Dictionary, key: &[u8]) -> Option<String> {
    match get(doc, d, key)? {
        Object::String(bytes, _) => Some(decode_text(bytes).replace("\r\n", "\n").replace('\r', "\n")),
        _ => None,
    }
}

/// Une couleur PDF (gris, RVB ou CMJN) ; aucune si la liste est vide (transparent).
fn color(doc: &Document, d: &Dictionary, key: &[u8]) -> Option<Color> {
    let v = nums(doc, d.get(key).ok()?)?;
    let c = |x: f32| (x.clamp(0.0, 1.0) * 255.0).round() as u8;
    match v[..] {
        [g] => Some(Color(c(g), c(g), c(g))),
        [r, g, b] => Some(Color(c(r), c(g), c(b))),
        [cy, m, y, k] => Some(Color(c((1.0 - cy) * (1.0 - k)), c((1.0 - m) * (1.0 - k)), c((1.0 - y) * (1.0 - k)))),
        _ => None,
    }
}

/// Épaisseur du trait, en points ; aucune si le trait n'est pas plein (pointillés, nuage…),
/// que l'appli ne sait pas dessiner.
fn border_width(doc: &Document, d: &Dictionary) -> Option<f32> {
    if let Some(Object::Dictionary(be)) = get(doc, d, b"BE") {
        if name(doc, be, b"S").is_some_and(|s| s == b"C") && num(doc, be, b"I").unwrap_or(0.0) > 0.0 {
            return None;
        }
    }
    if let Some(Object::Dictionary(bs)) = get(doc, d, b"BS") {
        if name(doc, bs, b"S").is_some_and(|s| s != b"S") {
            return None;
        }
        return Some(num(doc, bs, b"W").unwrap_or(1.0).max(0.0));
    }
    match get(doc, d, b"Border").and_then(|o| o.as_array().ok()) {
        Some(b) if b.len() >= 4 => None,
        Some(b) => Some(b.get(2).and_then(|w| annotate::number(doc, w)).unwrap_or(1.0).max(0.0)),
        None => Some(1.0),
    }
}

/// La zone d'une annotation en unités de page ; `inner` : sans la marge /RD.
fn rect_of(doc: &Document, d: &Dictionary, f: &Frame, inner: bool) -> Option<Rect> {
    let r = nums(doc, d.get(b"Rect").ok()?)?;
    let [x0, y0, x1, y1] = r[..] else { return None };
    let (mut x0, mut y0, mut x1, mut y1) = (x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1));
    if inner {
        if let Some([l, b, rr, t]) = d.get(b"RD").ok().and_then(|o| nums(doc, o)).and_then(|v| <[f32; 4]>::try_from(v).ok()) {
            if l + rr < x1 - x0 && b + t < y1 - y0 {
                (x0, y0, x1, y1) = (x0 + l, y0 + b, x1 - rr, y1 - t);
            }
        }
    }
    Some(box_of(&[f.units((x0, y0)), f.units((x1, y1))]))
}

fn box_of(points: &[Point]) -> Rect {
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for p in points {
        (x0, y0, x1, y1) = (x0.min(p.x), y0.min(p.y), x1.max(p.x), y1.max(p.y));
    }
    Rect { x: x0, y: y0, w: x1 - x0, h: y1 - y0 }
}

/// Écrite par l'appli (même sans sa description, avant la version qui l'ajoute).
fn ours(doc: &Document, d: &Dictionary) -> bool {
    text(doc, d, b"NM").is_some_and(|nm| nm.starts_with("documents-") || nm.starts_with("apercu-"))
}

/// Une annotation du PDF → la même dans l'appli, si elle sait la représenter fidèlement.
fn convert(doc: &Document, d: &Dictionary, f: &Frame) -> Option<Annotation> {
    let flags = get(doc, d, b"F").and_then(|o| o.as_i64().ok()).unwrap_or(0);
    if flags & UNTOUCHABLE != 0 || d.has(b"OC") || d.has(b"IRT") {
        return None;
    }
    if let Some(a) = saved(doc, d, f) {
        return a;
    }
    let subtype = name(doc, d, b"Subtype")?;
    // Transparence : l'appli ne sait pas la refaire (sauf pour le surligneur, toujours « produit »).
    let opaque = num(doc, d, b"CA").is_none_or(|ca| ca >= 0.99);
    let units = |w: f32| f.unit_len(w);
    match subtype {
        b"Highlight" => {
            let quads = nums(doc, d.get(b"QuadPoints").ok()?)?;
            let rects: Vec<Rect> = quads
                .chunks_exact(8)
                .map(|q| box_of(&q.chunks_exact(2).map(|p| f.units((p[0], p[1]))).collect::<Vec<_>>()))
                .filter(|r| r.w > 0.0 && r.h > 0.0)
                .collect();
            if rects.is_empty() {
                return None;
            }
            Some(Annotation::Highlight { rects, color: color(doc, d, b"C")? })
        }
        b"Square" | b"Circle" if opaque => {
            let w = border_width(doc, d)?;
            let stroke = color(doc, d, b"C").filter(|_| w > 0.0);
            let fill = color(doc, d, b"IC");
            if stroke.is_none() && fill.is_none() {
                return None;
            }
            let rect = rect_of(doc, d, f, true)?;
            let width = if stroke.is_some() { units(w) } else { 0.0 };
            Some(if subtype == b"Square" {
                Annotation::Rect { rect, stroke, fill, width, angle: 0.0, shape: Shape::Plain }
            } else {
                Annotation::Ellipse { rect, stroke, fill, width, angle: 0.0 }
            })
        }
        b"Line" if opaque => {
            let [x0, y0, x1, y1] = nums(doc, d.get(b"L").ok()?)?[..] else { return None };
            // Lignes de cote et légendes : pas dans l'appli.
            let leader = ["LL", "LLE", "LLO"].iter().any(|k| num(doc, d, k.as_bytes()).is_some_and(|v| v != 0.0));
            if leader || get(doc, d, b"Cap").and_then(|o| o.as_bool().ok()).unwrap_or(false) {
                return None;
            }
            let ends: Vec<&[u8]> = match get(doc, d, b"LE").and_then(|o| o.as_array().ok()) {
                Some(le) => le.iter().map(|o| o.as_name().unwrap_or(b"None")).collect(),
                None => vec![b"None", b"None"],
            };
            let arrow = |e: &[u8]| e == b"OpenArrow" || e == b"ClosedArrow";
            let (mut from, mut to) = (f.units((x0, y0)), f.units((x1, y1)));
            let pointed = match (ends.first().copied().unwrap_or(b"None"), ends.get(1).copied().unwrap_or(b"None")) {
                (b"None", b"None") => false,
                (b"None", e) if arrow(e) => true,
                (s, b"None") if arrow(s) => {
                    std::mem::swap(&mut from, &mut to);
                    true
                }
                _ => return None,
            };
            let w = border_width(doc, d)?;
            Some(Annotation::Line { from, to, color: color(doc, d, b"C")?, width: units(w.max(0.1)), arrow: pointed })
        }
        b"Ink" if opaque => {
            let strokes: Vec<Vec<Point>> = get(doc, d, b"InkList")?
                .as_array()
                .ok()?
                .iter()
                .filter_map(|s| nums(doc, s))
                .map(|s| s.chunks_exact(2).map(|p| f.units((p[0], p[1]))).collect::<Vec<_>>())
                .filter(|s| !s.is_empty())
                .collect();
            if strokes.is_empty() {
                return None;
            }
            let w = border_width(doc, d)?;
            Some(Annotation::Ink { strokes, color: color(doc, d, b"C")?, width: units(w.max(0.1)) })
        }
        b"FreeText" if opaque => free_text(doc, d, f),
        b"Text" => {
            let r = rect_of(doc, d, f, false)?;
            let color = color(doc, d, b"C").unwrap_or(Color(0xff, 0xd6, 0x0a));
            Some(Annotation::Note { at: Point { x: r.x, y: r.y }, text: text(doc, d, b"Contents").unwrap_or_default(), color })
        }
        // Forme tournée ou dessinée par l'appli (avant qu'elle range sa description).
        b"Polygon" if ours(doc, d) && opaque => polygon(doc, d, f),
        // Signature posée par l'appli (avant qu'elle range sa description).
        b"Stamp" if ours(doc, d) && upright(doc, d, f) => Some(Annotation::Image { rect: rect_of(doc, d, f, false)?, png: image_of(doc, d)? }),
        _ => None,
    }
}

/// Les formes que l'appli écrit en polygone (`None` : l'ovale), avec leur nombre de sommets.
const POLYGONS: [(Option<Shape>, usize); 6] =
    [(Some(Shape::Plain), 4), (None, 48), (Some(Shape::Rounded), 28), (Some(Shape::Bubble), 31), (Some(Shape::Star), 10), (Some(Shape::Hexagon), 6)];

/// Les sommets d'une forme dans la zone `r` (y vers le bas), dans l'ordre où l'appli les écrit
/// (voir `annotate::build`).
fn vertices_of(shape: Option<Shape>, r: Rect) -> Vec<Point> {
    let at = |fx: f32, fy: f32| Point { x: r.x + fx * r.w, y: r.y + fy * r.h };
    match shape {
        Some(Shape::Plain) => vec![at(0.0, 1.0), at(1.0, 1.0), at(1.0, 0.0), at(0.0, 0.0)],
        Some(shape) => annotate::outline(shape, r),
        None => (0..48)
            .map(|i| {
                let t = i as f32 * std::f32::consts::TAU / 48.0;
                at(0.5 + 0.5 * t.cos(), 0.5 - 0.5 * t.sin())
            })
            .collect(),
    }
}

/// Retrouve la zone et l'angle d'une forme d'après ses sommets. Toutes ces formes touchent les
/// quatre bords de leur zone : pour un angle donné, la zone est le cadre des sommets redressés.
/// On garde l'angle pour lequel la forme redessinée colle le mieux.
fn fit(points: &[Point], shape: Option<Shape>) -> Option<(Rect, f32)> {
    let try_angle = |deg: f32| -> Option<(Rect, f32)> {
        let (sin, cos) = deg.to_radians().sin_cos();
        let straight: Vec<Point> = points.iter().map(|p| Point { x: p.x * cos + p.y * sin, y: -p.x * sin + p.y * cos }).collect();
        let b = box_of(&straight);
        if b.w <= 0.0 || b.h <= 0.0 {
            return None;
        }
        // Le centre, retourné dans le repère de la page.
        let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
        let c = Point { x: cx * cos - cy * sin, y: cx * sin + cy * cos };
        let rect = Rect { x: c.x - b.w / 2.0, y: c.y - b.h / 2.0, w: b.w, h: b.h };
        let error = vertices_of(shape, rect)
            .iter()
            .zip(points)
            .map(|(q, v)| {
                let (dx, dy) = (q.x - c.x, q.y - c.y);
                (c.x + dx * cos - dy * sin - v.x).hypot(c.y + dx * sin + dy * cos - v.y)
            })
            .fold(0.0, f32::max);
        Some((rect, error))
    };
    let best = |from: f32, to: f32, step: f32| {
        let mut best: Option<(f32, Rect, f32)> = None;
        let mut deg = from;
        while deg < to {
            if let Some((r, e)) = try_angle(deg) {
                if best.is_none_or(|b| e < b.2) {
                    best = Some((deg, r, e));
                }
            }
            deg += step;
        }
        best
    };
    let (coarse, _, _) = best(-180.0, 180.0, 1.0)?;
    let (deg, rect, error) = best(coarse - 1.0, coarse + 1.0, 0.01)?;
    (error < 0.5).then_some((rect, (deg * 100.0).round() / 100.0))
}

/// Un polygone écrit par l'appli : un rectangle ou un ovale tourné, ou une forme du menu.
fn polygon(doc: &Document, d: &Dictionary, f: &Frame) -> Option<Annotation> {
    let vertices = nums(doc, d.get(b"Vertices").ok()?)?;
    let points: Vec<Point> = vertices.chunks_exact(2).map(|p| f.units((p[0], p[1]))).collect();
    let (shape, _) = POLYGONS.into_iter().find(|(_, n)| *n == points.len())?;
    let (inset, angle) = fit(&points, shape)?;
    let w = border_width(doc, d)?;
    let stroke = color(doc, d, b"C").filter(|_| w > 0.0);
    let fill = color(doc, d, b"IC");
    let width = if stroke.is_some() { f.unit_len(w) } else { 0.0 };
    // Le contour était tracé dans la zone réduite de la moitié du trait.
    let rect = Rect { x: inset.x - width / 2.0, y: inset.y - width / 2.0, w: inset.w + width, h: inset.h + width };
    let angle = if angle.abs() < 0.01 { 0.0 } else { angle };
    Some(match shape {
        Some(shape) => Annotation::Rect { rect, stroke, fill, width, angle, shape },
        None => Annotation::Ellipse { rect, stroke, fill, width, angle },
    })
}

/// Zone de texte simple (sans cadre, sans flèche de légende, pas tournée).
fn free_text(doc: &Document, d: &Dictionary, f: &Frame) -> Option<Annotation> {
    if name(doc, d, b"IT").is_some_and(|it| it != b"FreeText" && it != b"FreeTextTypeWriter")
        || d.has(b"CL")
        || num(doc, d, b"Rotate").is_some_and(|r| r != 0.0)
        || !upright(doc, d, f)
    {
        return None;
    }
    let content = text(doc, d, b"Contents")?;
    if content.trim().is_empty() {
        return None;
    }
    let mine = ours(doc, d);
    // Les autres logiciels ne s'accordent pas sur le sens de /C et /IC pour une zone de texte :
    // on ne reprend que celles sans fond ni cadre.
    let fill = if mine { color(doc, d, b"C") } else { None };
    if !mine && (border_width(doc, d)? > 0.0 || color(doc, d, b"C").is_some() || color(doc, d, b"IC").is_some()) {
        return None;
    }
    let da = text(doc, d, b"DA").unwrap_or_default();
    let (da_font, size, hex) = parse_da(&da);
    let size = if size > 0.0 { size } else { 12.0 };
    let color = Color::try_from(hex).unwrap_or(Color(0, 0, 0));
    // La police : celle de l'apparence si c'est une police standard, sinon celle de /DA.
    let font_name = appearance_font(doc, d).unwrap_or(da_font);
    let font = if font_name.contains("Cour") {
        Font::Courier
    } else if font_name.contains("Times") || font_name.starts_with("TiRo") {
        Font::Times
    } else {
        Font::Helvetica
    };
    let align = match num(doc, d, b"Q").map(|q| q as i32) {
        Some(1) => Align::Center,
        Some(2) => Align::Right,
        _ => Align::Left,
    };
    // Soulignée par l'appli : des traits après le texte, dans son apparence.
    let underline = mine
        && appearance(doc, d)
            .and_then(|s| if s.dict.has(b"Filter") { s.decompressed_content().ok() } else { Some(s.content.clone()) })
            .and_then(|ops| String::from_utf8_lossy(&ops).split_once("ET").map(|(_, after)| after.contains(" l S")))
            .unwrap_or(false);
    Some(Annotation::Text {
        rect: rect_of(doc, d, f, true)?,
        lines: content.split('\n').map(String::from).collect(),
        text: Some(content),
        font,
        size: f.unit_len(size),
        color,
        fill,
        bold: font_name.contains("Bold"),
        italic: font_name.contains("Italic") || font_name.contains("Oblique"),
        underline,
        align,
        widths: Vec::new(),
    })
}

/// L'apparence normale d'une annotation (son dessin).
fn appearance<'a>(doc: &'a Document, d: &'a Dictionary) -> Option<&'a lopdf::Stream> {
    let ap = get(doc, d, b"AP")?.as_dict().ok()?;
    get(doc, ap, b"N")?.as_stream().ok()
}

/// Vrai si l'apparence de l'annotation (texte, image) apparaît droite sur la page affichée.
fn upright(doc: &Document, d: &Dictionary, f: &Frame) -> bool {
    let matrix = appearance(doc, d)
        .and_then(|s| s.dict.get(b"Matrix").ok())
        .and_then(|m| nums(doc, m))
        .unwrap_or_else(|| vec![1.0, 0.0, 0.0, 1.0]);
    matrix.len() >= 4 && f.upright([matrix[0], matrix[1], matrix[2], matrix[3]])
}

/// Le nom de la police utilisée par l'apparence d'une zone de texte (« Times-Bold »).
fn appearance_font(doc: &Document, d: &Dictionary) -> Option<String> {
    let stream = appearance(doc, d)?;
    let resources = get(doc, &stream.dict, b"Resources")?.as_dict().ok()?;
    let fonts = get(doc, resources, b"Font")?.as_dict().ok()?;
    let (_, font) = fonts.iter().next()?;
    let font = doc.dereference(font).ok()?.1.as_dict().ok()?;
    let base = name(doc, font, b"BaseFont")?;
    Some(String::from_utf8_lossy(base).into_owned())
}

/// L'image d'une signature posée par l'appli, reprise de son apparence (PNG en « data URL »).
fn image_of(doc: &Document, d: &Dictionary) -> Option<String> {
    let stream = appearance(doc, d)?;
    let resources = get(doc, &stream.dict, b"Resources")?.as_dict().ok()?;
    let xobjects = get(doc, resources, b"XObject")?.as_dict().ok()?;
    let (_, picture) = xobjects.iter().next()?;
    let picture = doc.dereference(picture).ok()?.1.as_stream().ok()?;
    let size = |s: &lopdf::Stream, key: &[u8]| num(doc, &s.dict, key).map(|v| v as u32).filter(|v| *v > 0);
    let (w, h) = (size(picture, b"Width")?, size(picture, b"Height")?);
    if name(doc, &picture.dict, b"ColorSpace") != Some(b"DeviceRGB") || num(doc, &picture.dict, b"BitsPerComponent") != Some(8.0) {
        return None;
    }
    let pixels = (w as usize).checked_mul(h as usize)?;
    let decode = |s: &lopdf::Stream| {
        if s.dict.has(b"Filter") { s.decompressed_content_with_limit(MAX_IMAGE_BYTES).ok() } else { Some(s.content.clone()) }
    };
    let rgb = decode(picture)?;
    let alpha = match get(doc, &picture.dict, b"SMask").and_then(|o| o.as_stream().ok()) {
        Some(mask) => decode(mask).filter(|a| a.len() >= pixels)?,
        None => vec![255; pixels],
    };
    if rgb.len() < pixels * 3 {
        return None;
    }
    let rgba: Vec<u8> = (0..pixels).flat_map(|i| [rgb[3 * i], rgb[3 * i + 1], rgb[3 * i + 2], alpha[i]]).collect();
    let mut png = Vec::new();
    DynamicImage::ImageRgba8(RgbaImage::from_raw(w, h, rgba)?)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .ok()?;
    Some(crate::signature::to_data_url(&png))
}

/// La description rangée par l'appli, si l'annotation n'a pas été modifiée ailleurs depuis :
/// `Some(Some(…))` à reprendre, `Some(None)` à laisser telle quelle, `None` à lire comme
/// celles des autres logiciels.
fn saved(doc: &Document, d: &Dictionary, f: &Frame) -> Option<Option<Annotation>> {
    let Object::String(raw, _) = get(doc, d, DATA_KEY)? else { return None };
    let saved: Saved = serde_json::from_slice(raw).ok()?;
    let (check, contents) = annotate::fingerprint(doc, d);
    let unchanged = check.len() == saved.check.len()
        && check.iter().zip(&saved.check).all(|(a, b)| (a - b).abs() < 0.01)
        && contents == saved.contents;
    if !unchanged {
        return None;
    }
    let mut a = saved.annotation;
    // Page tournée depuis l'enregistrement : la description est dans l'ancien repère.
    let rotation = Rotation::from_degrees(saved.rotation);
    if rotation != f.rotation() || (saved.scale - f.scale()).abs() > 1e-4 {
        let Some(moved) = annotate::reframe(&a, &Frame::with_scale(f.bbox(), rotation, saved.scale), f) else { return Some(None) };
        a = moved;
    }
    if let Annotation::Image { png, .. } = &mut a {
        if png.is_empty() {
            let Some(found) = image_of(doc, d) else { return Some(None) };
            *png = found;
        }
    }
    Some(Some(a))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::dictionary;

    fn frame() -> Frame {
        // Page A4 droite, 4/3 d'unité par point (comme dans l'interface).
        Frame::new([0.0, 0.0, 595.0, 842.0], Rotation::R0, 595.0 * 4.0 / 3.0)
    }

    fn reals(v: &[f32]) -> Object {
        Object::Array(v.iter().map(|&x| Object::Real(x)).collect())
    }

    #[test]
    fn rectangle_d_un_autre_logiciel() {
        let doc = Document::with_version("1.7");
        let d = dictionary! {
            "Subtype" => "Square",
            "Rect" => reals(&[100.0, 700.0, 200.0, 800.0]),
            "C" => reals(&[1.0, 0.0, 0.0]),
            "BS" => dictionary! { "W" => 3 },
        };
        let Some(Annotation::Rect { rect, stroke, fill, width, .. }) = convert(&doc, &d, &frame()) else { panic!() };
        assert_eq!(stroke, Some(Color(255, 0, 0)));
        assert_eq!(fill, None);
        assert!((width - 4.0).abs() < 1e-3);
        // En haut à gauche de l'écran : y vers le bas.
        assert!((rect.x - 133.333).abs() < 0.01 && (rect.y - 56.0).abs() < 0.01, "{rect:?}");
        assert!((rect.w - 133.333).abs() < 0.01);
    }

    #[test]
    fn pointilles_nuages_et_caches_restent_tels_quels() {
        let doc = Document::with_version("1.7");
        let base = || dictionary! { "Subtype" => "Square", "Rect" => reals(&[0.0, 0.0, 10.0, 10.0]), "C" => reals(&[0.0]) };
        let mut dashed = base();
        dashed.set("BS", dictionary! { "S" => "D", "W" => 1 });
        assert!(convert(&doc, &dashed, &frame()).is_none());
        let mut cloudy = base();
        cloudy.set("BE", dictionary! { "S" => "C", "I" => 1 });
        assert!(convert(&doc, &cloudy, &frame()).is_none());
        let mut hidden = base();
        hidden.set("F", 2);
        assert!(convert(&doc, &hidden, &frame()).is_none());
        let mut locked = base();
        locked.set("F", 128 | 4);
        assert!(convert(&doc, &locked, &frame()).is_none());
        assert!(convert(&doc, &base(), &frame()).is_some());
    }

    #[test]
    fn fleche_dans_le_bon_sens() {
        let doc = Document::with_version("1.7");
        let d = dictionary! {
            "Subtype" => "Line",
            "Rect" => reals(&[0.0, 0.0, 100.0, 100.0]),
            "L" => reals(&[10.0, 10.0, 90.0, 90.0]),
            "LE" => vec![Object::from("ClosedArrow"), Object::from("None")],
            "C" => reals(&[0.0, 0.0, 1.0]),
        };
        let Some(Annotation::Line { from, to, arrow, .. }) = convert(&doc, &d, &frame()) else { panic!() };
        assert!(arrow);
        // La pointe était au début : elle passe à la fin.
        assert!((from.x - 120.0).abs() < 0.01 && (to.x - 13.333).abs() < 0.01, "{from:?} {to:?}");
    }

    #[test]
    fn texte_note_dessin_et_surlignage() {
        let doc = Document::with_version("1.7");
        let text = dictionary! {
            "Subtype" => "FreeText",
            "Rect" => reals(&[50.0, 500.0, 250.0, 530.0]),
            "Contents" => annotate::text_string("Bonjour\r\nà tous"),
            "DA" => Object::string_literal("/Cour 12 Tf 0 0 1 rg"),
            "BS" => dictionary! { "W" => 0 },
            "Q" => 1,
        };
        let Some(Annotation::Text { text: Some(t), font, size, color, align, .. }) = convert(&doc, &text, &frame()) else { panic!() };
        assert_eq!(t, "Bonjour\nà tous");
        assert_eq!((font, align, color), (Font::Courier, Align::Center, Color(0, 0, 255)));
        assert!((size - 16.0).abs() < 1e-3);

        let note = dictionary! { "Subtype" => "Text", "Rect" => reals(&[300.0, 800.0, 318.0, 818.0]), "Contents" => annotate::text_string("À voir") };
        let Some(Annotation::Note { at, text, .. }) = convert(&doc, &note, &frame()) else { panic!() };
        assert_eq!(text, "À voir");
        assert!((at.x - 400.0).abs() < 0.01 && (at.y - 32.0).abs() < 0.01);

        let ink = dictionary! {
            "Subtype" => "Ink",
            "Rect" => reals(&[0.0, 0.0, 100.0, 100.0]),
            "InkList" => vec![reals(&[0.0, 842.0, 30.0, 812.0])],
            "C" => reals(&[0.0, 0.0, 0.0, 1.0]),
        };
        let Some(Annotation::Ink { strokes, color, .. }) = convert(&doc, &ink, &frame()) else { panic!() };
        assert_eq!(color, Color(0, 0, 0));
        assert_eq!(strokes[0].len(), 2);
        assert!((strokes[0][1].y - 40.0).abs() < 0.01);

        let hl = dictionary! {
            "Subtype" => "Highlight",
            "Rect" => reals(&[0.0, 0.0, 100.0, 100.0]),
            "QuadPoints" => reals(&[10.0, 30.0, 40.0, 30.0, 10.0, 20.0, 40.0, 20.0]),
            "C" => reals(&[1.0, 1.0, 0.0]),
        };
        let Some(Annotation::Highlight { rects, .. }) = convert(&doc, &hl, &frame()) else { panic!() };
        assert_eq!(rects.len(), 1);
        assert!((rects[0].h - 13.333).abs() < 0.01);
    }

    #[test]
    fn zone_de_texte_encadree_d_un_autre_logiciel_laissee() {
        let doc = Document::with_version("1.7");
        let d = dictionary! {
            "Subtype" => "FreeText",
            "Rect" => reals(&[50.0, 500.0, 250.0, 530.0]),
            "Contents" => Object::string_literal("Cadre"),
            "DA" => Object::string_literal("/Helv 12 Tf 0 g"),
        };
        // Sans /BS ni /Border : un cadre d'un point.
        assert!(convert(&doc, &d, &frame()).is_none());
    }

    #[test]
    fn couleurs_grises_et_cmjn() {
        let doc = Document::with_version("1.7");
        let d = dictionary! { "A" => reals(&[0.5]), "B" => reals(&[0.0, 1.0, 1.0, 0.0]), "E" => reals(&[]) };
        assert_eq!(color(&doc, &d, b"A"), Some(Color(128, 128, 128)));
        assert_eq!(color(&doc, &d, b"B"), Some(Color(255, 0, 0)));
        assert_eq!(color(&doc, &d, b"E"), None);
    }
}
