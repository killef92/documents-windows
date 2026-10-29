//! Annotations : surlignage, formes, texte, notes et dessin.
//!
//! L'interface les décrit en « unités de page » : la page telle qu'elle s'affiche à 100 %
//! (rotation du PDF comprise, sans la rotation choisie dans l'appli), origine en haut à gauche.
//! À l'enregistrement, chacune devient une vraie annotation PDF, avec son apparence dessinée
//! ici : elle s'affiche pareil partout (Acrobat, Edge, Chrome…) et reste modifiable
//! dans ces logiciels.

use std::fmt::Write as _;

use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream, StringFormat};
use serde::{Deserialize, Serialize};

use crate::Rotation;

/// Marge intérieure d'une zone de texte, en unités de page.
pub const TEXT_PAD: f32 = 4.0;
/// Hauteur d'une ligne de texte, en proportion de la taille de la police.
pub const LINE_HEIGHT: f32 = 1.2;
/// Trait de soulignement : sous la ligne de base, épaisseur, en proportion de la taille.
pub const UNDERLINE_DROP: f32 = 0.12;
pub const UNDERLINE_WIDTH: f32 = 0.06;
/// Taille de l'icône d'une note, en unités de page.
pub const NOTE_SIZE: f32 = 24.0;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Une couleur, écrite « #rrggbb » côté interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Color(pub u8, pub u8, pub u8);

impl TryFrom<String> for Color {
    type Error = String;
    fn try_from(s: String) -> Result<Self, String> {
        let hex = s.trim_start_matches('#');
        let v = u32::from_str_radix(hex, 16).map_err(|_| format!("invalid color: {s}"))?;
        if hex.len() != 6 {
            return Err(format!("invalid color: {s}"));
        }
        Ok(Color((v >> 16) as u8, (v >> 8) as u8, v as u8))
    }
}

impl From<Color> for String {
    fn from(c: Color) -> String {
        format!("#{:02x}{:02x}{:02x}", c.0, c.1, c.2)
    }
}

/// Les trois polices standard des PDF : présentes dans tous les lecteurs, rien à intégrer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Font {
    #[default]
    Helvetica,
    Times,
    Courier,
}

impl Font {
    /// Le nom de la police standard, en gras et / ou en italique.
    fn base_name(self, bold: bool, italic: bool) -> &'static str {
        match (self, bold, italic) {
            (Font::Helvetica, false, false) => "Helvetica",
            (Font::Helvetica, true, false) => "Helvetica-Bold",
            (Font::Helvetica, false, true) => "Helvetica-Oblique",
            (Font::Helvetica, true, true) => "Helvetica-BoldOblique",
            (Font::Times, false, false) => "Times-Roman",
            (Font::Times, true, false) => "Times-Bold",
            (Font::Times, false, true) => "Times-Italic",
            (Font::Times, true, true) => "Times-BoldItalic",
            (Font::Courier, false, false) => "Courier",
            (Font::Courier, true, false) => "Courier-Bold",
            (Font::Courier, false, true) => "Courier-Oblique",
            (Font::Courier, true, true) => "Courier-BoldOblique",
        }
    }

    /// Distance entre le haut d'une ligne et la ligne de base, en proportion de la taille :
    /// la même que dans l'interface (Arial, Times New Roman, Courier New, interligne 1,2).
    fn baseline(self) -> f32 {
        match self {
            Font::Helvetica => 0.947,
            Font::Times => 0.938,
            Font::Courier => 0.867,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Annotation {
    /// Surlignage de texte : un rectangle par ligne.
    Highlight { rects: Vec<Rect>, color: Color },
    /// `angle` : rotation en degrés, dans le sens des aiguilles d'une montre, autour du centre.
    /// `shape` : rectangle simple, ou une autre forme dessinée dans la même zone.
    Rect {
        rect: Rect,
        stroke: Option<Color>,
        fill: Option<Color>,
        width: f32,
        #[serde(default)]
        angle: f32,
        #[serde(default)]
        shape: Shape,
    },
    Ellipse {
        rect: Rect,
        stroke: Option<Color>,
        fill: Option<Color>,
        width: f32,
        #[serde(default)]
        angle: f32,
    },
    Line {
        from: Point,
        to: Point,
        color: Color,
        width: f32,
        #[serde(default)]
        arrow: bool,
    },
    /// Dessin à main levée : un trait par appui.
    Ink { strokes: Vec<Vec<Point>>, color: Color, width: f32 },
    /// Zone de texte. Les lignes sont déjà coupées par l'interface, pour que le résultat
    /// soit identique à ce qui était à l'écran.
    Text {
        rect: Rect,
        lines: Vec<String>,
        #[serde(default)]
        font: Font,
        size: f32,
        color: Color,
        #[serde(default)]
        fill: Option<Color>,
        #[serde(default)]
        bold: bool,
        #[serde(default)]
        italic: bool,
        #[serde(default)]
        underline: bool,
        #[serde(default)]
        align: Align,
        /// Largeur de chaque ligne (mesurée par l'interface), pour centrer, aligner à droite
        /// et souligner.
        #[serde(default)]
        widths: Vec<f32>,
    },
    /// Note (petit post-it) : `at` est son coin en haut à gauche.
    Note { at: Point, text: String, color: Color },
    /// Image posée sur la page (signature à partir d'une photo) : PNG en base64.
    Image { rect: Rect, png: String },
}

/// Alignement des lignes d'une zone de texte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
}

/// Les formes du menu « Formes » qui se dessinent dans un rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Shape {
    #[default]
    Plain,
    /// Rectangle aux coins arrondis.
    Rounded,
    /// Bulle de bande dessinée (la pointe en bas à gauche).
    Bubble,
    /// Étoile à cinq branches.
    Star,
    Hexagon,
}

/// Le contour d'une forme dans la zone `r` (unités de page, y vers le bas), dans le sens des
/// aiguilles d'une montre à l'écran. Le même calcul que dans l'interface (annotations.ts).
pub fn outline(shape: Shape, r: Rect) -> Vec<Point> {
    let at = |fx: f32, fy: f32| Point { x: r.x + fx * r.w, y: r.y + fy * r.h };
    match shape {
        Shape::Plain => vec![at(0.0, 0.0), at(1.0, 0.0), at(1.0, 1.0), at(0.0, 1.0)],
        Shape::Hexagon => vec![at(0.5, 0.0), at(1.0, 0.25), at(1.0, 0.75), at(0.5, 1.0), at(0.0, 0.75), at(0.0, 0.25)],
        Shape::Star => {
            // Pointe en haut ; l'étoile remplit toute la zone.
            let (sx, top, bottom) = (18f32.to_radians().cos(), -1.0, 54f32.to_radians().sin());
            (0..10)
                .map(|i| {
                    let a = (-90.0 + 36.0 * i as f32).to_radians();
                    let k = if i % 2 == 0 { 1.0 } else { 0.382 };
                    at((k * a.cos() + sx) / (2.0 * sx), (k * a.sin() - top) / (bottom - top))
                })
                .collect()
        }
        Shape::Rounded => rounded(r, r.h, None),
        Shape::Bubble => rounded(r, r.h * 0.78, Some([at(0.42, 0.78), at(0.16, 1.0), at(0.26, 0.78)])),
    }
}

/// Rectangle arrondi de hauteur `h` ; `tail` : trois points ajoutés au milieu du bas (bulle).
fn rounded(r: Rect, h: f32, tail: Option<[Point; 3]>) -> Vec<Point> {
    let rad = (r.w.min(h) * 0.18).max(0.0);
    let corners = [
        (r.x + r.w - rad, r.y + rad, -90.0),
        (r.x + r.w - rad, r.y + h - rad, 0.0),
        (r.x + rad, r.y + h - rad, 90.0),
        (r.x + rad, r.y + rad, 180.0),
    ];
    let mut out = Vec::new();
    for (k, (cx, cy, start)) in corners.into_iter().enumerate() {
        for i in 0..=6 {
            let a = (start + 15.0 * i as f32).to_radians();
            out.push(Point { x: cx + rad * a.cos(), y: cy + rad * a.sin() });
        }
        if k == 1 {
            out.extend(tail.into_iter().flatten());
        }
    }
    out
}

/// Longueur de la pointe d'une flèche, en unités de page (la même que dans l'interface).
pub fn arrow_head(width: f32) -> f32 {
    width * 3.0 + 8.0
}

/// Les annotations d'une page du PDF enregistré.
pub(crate) struct PageAnnotations<'a> {
    pub annotations: &'a [Annotation],
    /// Rotation choisie dans l'appli (déjà appliquée au PDF enregistré).
    pub rotation: Rotation,
    /// Largeur de la page en unités de page, avant cette rotation.
    pub unit_width: f32,
}

/// Passage des unités de page à l'espace du PDF.
pub(crate) struct Frame {
    /// Espace « affiché » (points, origine en bas à gauche) → espace de la page PDF.
    matrix: [f32; 6],
    /// Unités de page par point.
    k: f32,
    /// Hauteur affichée, en points.
    height: f32,
}

impl Frame {
    /// `bbox` : la zone visible de la page ; `r` : comment elle est tournée à l'affichage.
    pub(crate) fn new([x0, y0, x1, y1]: [f32; 4], r: Rotation, unit_width: f32) -> Frame {
        let (bw, bh) = (x1 - x0, y1 - y0);
        let (dw, dh) = if r.is_sideways() { (bh, bw) } else { (bw, bh) };
        let matrix = match r {
            Rotation::R0 => [1.0, 0.0, 0.0, 1.0, x0, y0],
            Rotation::R90 => [0.0, 1.0, -1.0, 0.0, x1, y0],
            Rotation::R180 => [-1.0, 0.0, 0.0, -1.0, x1, y1],
            Rotation::R270 => [0.0, -1.0, 1.0, 0.0, x0, y1],
        };
        let k = if dw > 0.0 && unit_width > 0.0 { unit_width / dw } else { 1.0 };
        Frame { matrix, k, height: dh }
    }

    /// Unités de page → points affichés (origine en bas à gauche).
    fn d(&self, p: Point) -> (f32, f32) {
        (p.x / self.k, self.height - p.y / self.k)
    }

    fn len(&self, v: f32) -> f32 {
        v / self.k
    }

    /// Points affichés → espace de la page PDF.
    fn user(&self, (x, y): (f32, f32)) -> (f32, f32) {
        let m = self.matrix;
        (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
    }

    /// Espace de la page PDF → unités de page (l'inverse de `d` puis `user`).
    pub(crate) fn units(&self, (x, y): (f32, f32)) -> Point {
        let [a, b, c, d, e, f] = self.matrix;
        let det = a * d - b * c;
        let (x, y) = (x - e, y - f);
        let dx = (d * x - c * y) / det;
        let dy = (a * y - b * x) / det;
        Point { x: dx * self.k, y: (self.height - dy) * self.k }
    }

    fn user_box(&self, [x0, y0, x1, y1]: [f32; 4]) -> [f32; 4] {
        let corners = [self.user((x0, y0)), self.user((x1, y0)), self.user((x0, y1)), self.user((x1, y1))];
        let xs = corners.map(|c| c.0);
        let ys = corners.map(|c| c.1);
        [
            xs.iter().copied().fold(f32::MAX, f32::min),
            ys.iter().copied().fold(f32::MAX, f32::min),
            xs.iter().copied().fold(f32::MIN, f32::max),
            ys.iter().copied().fold(f32::MIN, f32::max),
        ]
    }

    /// Rectangle en unités de page → [gauche, bas, droite, haut] en points affichés.
    fn d_rect(&self, r: Rect) -> [f32; 4] {
        let (x0, y1) = self.d(Point { x: r.x, y: r.y });
        let (x1, y0) = self.d(Point { x: r.x + r.w, y: r.y + r.h });
        [x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1)]
    }
}

/// Un nombre court pour le flux PDF.
pub(crate) fn n(v: f32) -> String {
    let v = if v.is_finite() { v } else { 0.0 };
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" || s.is_empty() { "0".into() } else { s.into() }
}

pub(crate) fn reals(values: &[f32]) -> Object {
    Object::Array(values.iter().map(|&v| Object::Real(v)).collect())
}

pub(crate) fn rgb(c: Color) -> [f32; 3] {
    [c.0 as f32 / 255.0, c.1 as f32 / 255.0, c.2 as f32 / 255.0]
}

fn color_array(c: Color) -> Object {
    reals(&rgb(c))
}

/// Texte d'annotation (UTF-16 avec BOM : accents et tout le reste).
pub(crate) fn text_string(s: &str) -> Object {
    let mut bytes = vec![0xFE, 0xFF];
    for unit in s.encode_utf16() {
        bytes.extend(unit.to_be_bytes());
    }
    Object::String(bytes, StringFormat::Hexadecimal)
}

/// Encodage WinAnsi (celui des polices standard) ; ce qui n'y existe pas devient « ? ».
pub(crate) fn win_ansi(s: &str) -> Vec<u8> {
    const HIGH: [(char, u8); 27] = [
        ('€', 0x80), ('‚', 0x82), ('ƒ', 0x83), ('„', 0x84), ('…', 0x85), ('†', 0x86), ('‡', 0x87),
        ('ˆ', 0x88), ('‰', 0x89), ('Š', 0x8A), ('‹', 0x8B), ('Œ', 0x8C), ('Ž', 0x8E), ('‘', 0x91),
        ('’', 0x92), ('“', 0x93), ('”', 0x94), ('•', 0x95), ('–', 0x96), ('—', 0x97), ('˜', 0x98),
        ('™', 0x99), ('š', 0x9A), ('›', 0x9B), ('œ', 0x9C), ('ž', 0x9E), ('Ÿ', 0x9F),
    ];
    s.chars()
        .map(|c| match c as u32 {
            0x20..=0x7E | 0xA0..=0xFF => c as u8,
            0x09 => b' ',
            _ => HIGH.iter().find(|(h, _)| *h == c).map(|(_, b)| *b).unwrap_or(b'?'),
        })
        .collect()
}

/// Chaîne littérale PDF, avec les caractères spéciaux protégés.
pub(crate) fn literal(bytes: &[u8]) -> String {
    let mut out = String::from("(");
    for &b in bytes {
        match b {
            b'(' | b')' | b'\\' => {
                out.push('\\');
                out.push(b as char);
            }
            0x20..=0x7E => out.push(b as char),
            _ => {
                let _ = write!(out, "\\{b:03o}");
            }
        }
    }
    out.push(')');
    out
}

/// Une ellipse en 4 courbes de Bézier.
fn ellipse_path(ops: &mut String, [x0, y0, x1, y1]: [f32; 4]) {
    const K: f32 = 0.552_284_8;
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let (rx, ry) = ((x1 - x0) / 2.0, (y1 - y0) / 2.0);
    let (ox, oy) = (rx * K, ry * K);
    let _ = writeln!(ops, "{} {} m", n(cx + rx), n(cy));
    let _ = writeln!(ops, "{} {} {} {} {} {} c", n(cx + rx), n(cy + oy), n(cx + ox), n(cy + ry), n(cx), n(cy + ry));
    let _ = writeln!(ops, "{} {} {} {} {} {} c", n(cx - ox), n(cy + ry), n(cx - rx), n(cy + oy), n(cx - rx), n(cy));
    let _ = writeln!(ops, "{} {} {} {} {} {} c", n(cx - rx), n(cy - oy), n(cx - ox), n(cy - ry), n(cx), n(cy - ry));
    let _ = writeln!(ops, "{} {} {} {} {} {} c", n(cx + ox), n(cy - ry), n(cx + rx), n(cy - oy), n(cx + rx), n(cy));
}

fn paint_ops(ops: &mut String, stroke: Option<Color>, fill: Option<Color>, width: f32) -> &'static str {
    if let Some(c) = stroke {
        let [r, g, b] = rgb(c);
        let _ = writeln!(ops, "{} {} {} RG {} w", n(r), n(g), n(b), n(width));
    }
    if let Some(c) = fill {
        let [r, g, b] = rgb(c);
        let _ = writeln!(ops, "{} {} {} rg", n(r), n(g), n(b));
    }
    match (stroke.is_some() && width > 0.0, fill.is_some()) {
        (true, true) => "B",
        (true, false) => "S",
        (false, true) => "f",
        (false, false) => "n",
    }
}

/// Rotation d'une forme autour du centre de sa zone (en points affichés).
struct Turn {
    cos: f32,
    sin: f32,
    cx: f32,
    cy: f32,
}

impl Turn {
    fn new([x0, y0, x1, y1]: [f32; 4], radians: f32) -> Turn {
        Turn { cos: radians.cos(), sin: radians.sin(), cx: (x0 + x1) / 2.0, cy: (y0 + y1) / 2.0 }
    }

    fn is_none(&self) -> bool {
        self.sin.abs() < 1e-6 && self.cos > 0.0
    }

    fn apply(&self, (x, y): (f32, f32)) -> (f32, f32) {
        let (dx, dy) = (x - self.cx, y - self.cy);
        (self.cx + dx * self.cos - dy * self.sin, self.cy + dx * self.sin + dy * self.cos)
    }

    /// La même rotation, en matrice PDF (`cm`).
    fn matrix(&self) -> [f32; 6] {
        let (c, s) = (self.cos, self.sin);
        [c, s, -s, c, self.cx - self.cx * c + self.cy * s, self.cy - self.cx * s - self.cy * c]
    }

    /// La zone qui contient la forme une fois tournée.
    fn bounds(&self, [x0, y0, x1, y1]: [f32; 4]) -> [f32; 4] {
        let pts = [(x0, y0), (x1, y0), (x0, y1), (x1, y1)].map(|p| self.apply(p));
        [
            pts.iter().map(|p| p.0).fold(f32::MAX, f32::min),
            pts.iter().map(|p| p.1).fold(f32::MAX, f32::min),
            pts.iter().map(|p| p.0).fold(f32::MIN, f32::max),
            pts.iter().map(|p| p.1).fold(f32::MIN, f32::max),
        ]
    }
}

fn grow([x0, y0, x1, y1]: [f32; 4], m: f32) -> [f32; 4] {
    [x0 - m, y0 - m, x1 + m, y1 + m]
}

fn border(width: f32) -> Object {
    Object::Dictionary(dictionary! { "W" => Object::Real(width), "S" => "S" })
}

/// Ce qu'il faut pour écrire une annotation : son dictionnaire (sans Rect ni apparence),
/// sa zone en points affichés, le dessin de son apparence et ses ressources.
struct Built {
    dict: Dictionary,
    bbox: [f32; 4],
    ops: String,
    resources: Dictionary,
}

fn build(a: &Annotation, f: &Frame, doc: &mut Document) -> Option<Built> {
    let mut ops = String::new();
    let mut resources = Dictionary::new();
    let (dict, bbox) = match a {
        Annotation::Highlight { rects, color } => {
            if rects.is_empty() {
                return None;
            }
            let [r, g, b] = rgb(*color);
            let _ = writeln!(ops, "/GS0 gs {} {} {} rg", n(r), n(g), n(b));
            let mut quads = Vec::new();
            let mut bbox = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
            for rect in rects {
                let [x0, y0, x1, y1] = f.d_rect(*rect);
                let _ = writeln!(ops, "{} {} {} {} re f", n(x0), n(y0), n(x1 - x0), n(y1 - y0));
                // Coins dans l'ordre attendu par Acrobat : haut-gauche, haut-droite, bas-gauche, bas-droite.
                for corner in [(x0, y1), (x1, y1), (x0, y0), (x1, y0)] {
                    let (x, y) = f.user(corner);
                    quads.extend([x, y]);
                }
                bbox = [bbox[0].min(x0), bbox[1].min(y0), bbox[2].max(x1), bbox[3].max(y1)];
            }
            resources.set(
                "ExtGState",
                dictionary! { "GS0" => dictionary! { "Type" => "ExtGState", "BM" => "Multiply" } },
            );
            let dict = dictionary! {
                "Subtype" => "Highlight",
                "QuadPoints" => reals(&quads),
                "C" => color_array(*color),
            };
            (dict, bbox)
        }
        Annotation::Rect { rect, stroke, fill, width, angle, .. } | Annotation::Ellipse { rect, stroke, fill, width, angle } => {
            let w = if stroke.is_some() { f.len(*width).max(0.0) } else { 0.0 };
            let outer = f.d_rect(*rect);
            let inner = grow(outer, -w / 2.0);
            let paint = paint_ops(&mut ops, *stroke, *fill, w);
            let ellipse = matches!(a, Annotation::Ellipse { .. });
            let shape = match a {
                Annotation::Rect { shape, .. } => *shape,
                _ => Shape::Plain,
            };
            // Autres formes : leur contour, dans la zone réduite de la moitié du trait.
            let half = if stroke.is_some() { width.max(0.0) / 2.0 } else { 0.0 };
            let inset = Rect { x: rect.x + half, y: rect.y + half, w: (rect.w - 2.0 * half).max(0.0), h: (rect.h - 2.0 * half).max(0.0) };
            let custom: Vec<(f32, f32)> =
                if shape == Shape::Plain { Vec::new() } else { outline(shape, inset).into_iter().map(|p| f.d(p)).collect() };
            // Forme tournée : dessinée droite, dans un repère tourné autour de son centre
            // (à l'écran l'angle va dans le sens des aiguilles d'une montre, ici l'axe y monte).
            let turn = Turn::new(outer, -angle.to_radians());
            if !turn.is_none() {
                let [a, b, c, d, e, g] = turn.matrix();
                let _ = writeln!(ops, "q {} {} {} {} {} {} cm", n(a), n(b), n(c), n(d), n(e), n(g));
            }
            if ellipse {
                ellipse_path(&mut ops, inner);
            } else if !custom.is_empty() {
                for (i, (x, y)) in custom.iter().enumerate() {
                    let _ = writeln!(ops, "{} {} {}", n(*x), n(*y), if i == 0 { "m" } else { "l" });
                }
                ops.push_str("h\n");
            } else {
                let _ = writeln!(ops, "{} {} {} {} re", n(inner[0]), n(inner[1]), n(inner[2] - inner[0]), n(inner[3] - inner[1]));
            }
            ops.push_str(paint);
            ops.push('\n');
            if !turn.is_none() {
                ops.push_str("Q\n");
            }
            let mut dict = if turn.is_none() && custom.is_empty() {
                dictionary! { "Subtype" => if ellipse { "Circle" } else { "Square" } }
            } else {
                // Pas de carré ni d'ovale tourné dans les PDF : un polygone, que tous les
                // logiciels savent afficher et modifier.
                let outline: Vec<(f32, f32)> = if ellipse {
                    let (cx, cy) = ((inner[0] + inner[2]) / 2.0, (inner[1] + inner[3]) / 2.0);
                    let (rx, ry) = ((inner[2] - inner[0]) / 2.0, (inner[3] - inner[1]) / 2.0);
                    (0..48)
                        .map(|i| {
                            let t = i as f32 * std::f32::consts::TAU / 48.0;
                            (cx + rx * t.cos(), cy + ry * t.sin())
                        })
                        .collect()
                } else if !custom.is_empty() {
                    custom
                } else {
                    vec![(inner[0], inner[1]), (inner[2], inner[1]), (inner[2], inner[3]), (inner[0], inner[3])]
                };
                let vertices: Vec<f32> = outline.into_iter().flat_map(|p| {
                    let (x, y) = f.user(turn.apply(p));
                    [x, y]
                }).collect();
                dictionary! { "Subtype" => "Polygon", "Vertices" => reals(&vertices) }
            };
            dict.set("BS", border(w));
            if let Some(c) = stroke {
                dict.set("C", color_array(*c));
            }
            if let Some(c) = fill {
                dict.set("IC", color_array(*c));
            }
            (dict, turn.bounds(outer))
        }
        Annotation::Line { from, to, color, width, arrow } => {
            let w = f.len(*width).max(0.1);
            let (x0, y0) = f.d(*from);
            let (x1, y1) = f.d(*to);
            let [r, g, b] = rgb(*color);
            let _ = writeln!(ops, "{} {} {} RG {} {} {} rg {} w 1 J 1 j", n(r), n(g), n(b), n(r), n(g), n(b), n(w));
            let (dx, dy) = (x1 - x0, y1 - y0);
            let length = (dx * dx + dy * dy).sqrt().max(0.001);
            let (ux, uy) = (dx / length, dy / length);
            let head = if *arrow { f.len(arrow_head(*width)).min(length) } else { 0.0 };
            let (ex, ey) = (x1 - ux * head * 0.8, y1 - uy * head * 0.8);
            let _ = writeln!(ops, "{} {} m {} {} l S", n(x0), n(y0), n(if *arrow { ex } else { x1 }), n(if *arrow { ey } else { y1 }));
            let mut bbox = [x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1)];
            if *arrow {
                let (bx, by) = (x1 - ux * head, y1 - uy * head);
                let half = head * 0.5;
                let (lx, ly) = (bx - uy * half, by + ux * half);
                let (rx, ry) = (bx + uy * half, by - ux * half);
                let _ = writeln!(ops, "{} {} m {} {} l {} {} l h f", n(x1), n(y1), n(lx), n(ly), n(rx), n(ry));
                bbox = [bbox[0].min(lx).min(rx), bbox[1].min(ly).min(ry), bbox[2].max(lx).max(rx), bbox[3].max(ly).max(ry)];
            }
            let (ua, ub) = (f.user((x0, y0)), f.user((x1, y1)));
            let mut dict = dictionary! {
                "Subtype" => "Line",
                "L" => reals(&[ua.0, ua.1, ub.0, ub.1]),
                "C" => color_array(*color),
                "BS" => border(w),
            };
            if *arrow {
                dict.set("LE", Object::Array(vec!["None".into(), "ClosedArrow".into()]));
                dict.set("IC", color_array(*color));
            }
            (dict, grow(bbox, w))
        }
        Annotation::Ink { strokes, color, width } => {
            let strokes: Vec<_> = strokes.iter().filter(|s| !s.is_empty()).collect();
            if strokes.is_empty() {
                return None;
            }
            let w = f.len(*width).max(0.1);
            let [r, g, b] = rgb(*color);
            let _ = writeln!(ops, "{} {} {} RG {} w 1 J 1 j", n(r), n(g), n(b), n(w));
            let mut bbox = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
            let mut ink = Vec::new();
            for stroke in strokes {
                let mut list = Vec::new();
                for (i, p) in stroke.iter().enumerate() {
                    let (x, y) = f.d(*p);
                    let _ = write!(ops, "{} {} {} ", n(x), n(y), if i == 0 { "m" } else { "l" });
                    if stroke.len() == 1 {
                        // Un simple point : un trait de longueur nulle, arrondi.
                        let _ = write!(ops, "{} {} l ", n(x), n(y));
                    }
                    let (ux, uy) = f.user((x, y));
                    list.extend([ux, uy]);
                    bbox = [bbox[0].min(x), bbox[1].min(y), bbox[2].max(x), bbox[3].max(y)];
                }
                ops.push_str("S\n");
                ink.push(reals(&list));
            }
            let dict = dictionary! {
                "Subtype" => "Ink",
                "InkList" => Object::Array(ink),
                "C" => color_array(*color),
                "BS" => border(w),
            };
            (dict, grow(bbox, w))
        }
        Annotation::Text { rect, lines, font, size, color, fill, bold, italic, underline, align, widths } => {
            let outer = f.d_rect(*rect);
            if let Some(c) = fill {
                let [r, g, b] = rgb(*c);
                let _ = writeln!(ops, "{} {} {} rg {} {} {} {} re f", n(r), n(g), n(b), n(outer[0]), n(outer[1]), n(outer[2] - outer[0]), n(outer[3] - outer[1]));
            }
            let pts = f.len(*size);
            let pad = f.len(TEXT_PAD);
            let [r, g, b] = rgb(*color);
            // Chaque ligne à sa place : à gauche, au centre ou à droite de la zone.
            let room = outer[2] - outer[0] - 2.0 * pad;
            let starts: Vec<(f32, f32, f32)> = lines
                .iter()
                .enumerate()
                .map(|(i, _)| {
                    let w = f.len(widths.get(i).copied().unwrap_or(0.0));
                    let x = outer[0] + pad + match align {
                        Align::Left => 0.0,
                        Align::Center => (room - w) / 2.0,
                        Align::Right => room - w,
                    };
                    (x, outer[3] - pad - pts * font.baseline() - i as f32 * pts * LINE_HEIGHT, w)
                })
                .collect();
            let _ = writeln!(ops, "BT /F1 {} Tf {} {} {} rg", n(pts), n(r), n(g), n(b));
            for (line, (x, y, _)) in lines.iter().zip(&starts) {
                let _ = writeln!(ops, "1 0 0 1 {} {} Tm {} Tj", n(*x), n(*y), literal(&win_ansi(line)));
            }
            ops.push_str("ET\n");
            if *underline {
                let _ = writeln!(ops, "{} {} {} RG {} w", n(r), n(g), n(b), n(pts * UNDERLINE_WIDTH));
                for (x, y, w) in starts.iter().filter(|s| s.2 > 0.0) {
                    let uy = y - pts * UNDERLINE_DROP;
                    let _ = writeln!(ops, "{} {} m {} {} l S", n(*x), n(uy), n(x + w), n(uy));
                }
            }
            resources.set(
                "Font",
                dictionary! {
                    "F1" => dictionary! {
                        "Type" => "Font",
                        "Subtype" => "Type1",
                        "BaseFont" => font.base_name(*bold, *italic),
                        "Encoding" => "WinAnsiEncoding",
                    },
                },
            );
            let da = format!("/Helv {} Tf {} {} {} rg", n(pts), n(r), n(g), n(b));
            let mut dict = dictionary! {
                "Subtype" => "FreeText",
                "Contents" => text_string(&lines.join("\n")),
                "DA" => Object::string_literal(da),
                "BS" => border(0.0),
            };
            if let Some(c) = fill {
                dict.set("C", color_array(*c));
            }
            (dict, outer)
        }
        Annotation::Image { rect, png } => {
            let image = image::load_from_memory(&base64(png)?).ok()?.into_rgba8();
            let (w, h) = image.dimensions();
            let rgb: Vec<u8> = image.pixels().flat_map(|p| [p[0], p[1], p[2]]).collect();
            let alpha: Vec<u8> = image.pixels().map(|p| p[3]).collect();
            let image_dict = |space: &str| {
                dictionary! {
                    "Type" => "XObject",
                    "Subtype" => "Image",
                    "Width" => w as i64,
                    "Height" => h as i64,
                    "ColorSpace" => space,
                    "BitsPerComponent" => 8,
                }
            };
            let mut mask = Stream::new(image_dict("DeviceGray"), alpha);
            let _ = mask.compress();
            let mask_id = doc.add_object(mask);
            let mut dict = image_dict("DeviceRGB");
            dict.set("SMask", Object::Reference(mask_id));
            let mut picture = Stream::new(dict, rgb);
            let _ = picture.compress();
            let picture_id = doc.add_object(picture);
            resources.set("XObject", dictionary! { "Im0" => Object::Reference(picture_id) });
            let [x0, y0, x1, y1] = f.d_rect(*rect);
            let _ = writeln!(ops, "q {} 0 0 {} {} {} cm /Im0 Do Q", n(x1 - x0), n(y1 - y0), n(x0), n(y0));
            (dictionary! { "Subtype" => "Stamp", "Name" => "Signature" }, [x0, y0, x1, y1])
        }
        Annotation::Note { at, text, color } => {
            let outer = f.d_rect(Rect { x: at.x, y: at.y, w: NOTE_SIZE, h: NOTE_SIZE });
            let [x0, y0, x1, y1] = outer;
            let s = x1 - x0;
            let fold = s * 0.3;
            let [r, g, b] = rgb(*color);
            // Un post-it : carré coloré, coin replié, trois lignes d'écriture.
            let _ = writeln!(ops, "{} {} {} rg 0.35 0.3 0.1 RG {} w", n(r), n(g), n(b), n(s * 0.04));
            let _ = writeln!(
                ops,
                "{} {} m {} {} l {} {} l {} {} l {} {} l h B",
                n(x0), n(y0), n(x1 - fold), n(y0), n(x1), n(y0 + fold), n(x1), n(y1), n(x0), n(y1)
            );
            let _ = writeln!(ops, "{} {} m {} {} l {} {} l S", n(x1 - fold), n(y0), n(x1 - fold), n(y0 + fold), n(x1), n(y0 + fold));
            for i in 1..=3 {
                let y = y1 - s * 0.22 * i as f32;
                let _ = writeln!(ops, "{} {} m {} {} l S", n(x0 + s * 0.18), n(y), n(x1 - s * (if i == 3 { 0.42 } else { 0.18 })), n(y));
            }
            let dict = dictionary! {
                "Subtype" => "Text",
                "Contents" => text_string(text),
                "Name" => "Comment",
                "Open" => false,
                "C" => color_array(*color),
            };
            (dict, outer)
        }
    };
    Some(Built { dict, bbox, ops, resources })
}

/// Décode du base64 (avec ou sans le préfixe « data:…; base64, »).
fn base64(s: &str) -> Option<Vec<u8>> {
    let s = s.rsplit(',').next()?;
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0);
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' | b'\n' | b'\r' | b' ' => continue,
            _ => return None,
        };
        acc = (acc << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

pub(crate) fn number(doc: &Document, o: &Object) -> Option<f32> {
    match doc.dereference(o).ok()?.1 {
        Object::Integer(i) => Some(*i as f32),
        Object::Real(r) => Some(*r),
        _ => None,
    }
}

pub(crate) fn page_box(doc: &Document, page: &Dictionary) -> Option<[f32; 4]> {
    for key in [b"CropBox".as_slice(), b"MediaBox"] {
        let value = page.get(key).ok().cloned().or_else(|| crate::rearrange::inherited(doc, page, key));
        let Some(value) = value else { continue };
        let Ok((_, Object::Array(values))) = doc.dereference(&value) else { continue };
        let v: Vec<f32> = values.iter().filter_map(|o| number(doc, o)).collect();
        if v.len() == 4 {
            return Some([v[0].min(v[2]), v[1].min(v[3]), v[0].max(v[2]), v[1].max(v[3])]);
        }
    }
    None
}

/// La rotation (/Rotate) d'une page, héritée ou non, en degrés.
pub(crate) fn page_rotation(doc: &Document, page: &Dictionary) -> i32 {
    page.get(b"Rotate")
        .ok()
        .cloned()
        .or_else(|| crate::rearrange::inherited(doc, page, b"Rotate"))
        .and_then(|o| number(doc, &o))
        .unwrap_or(0.0) as i32
}

/// Ajoute les annotations aux pages d'un PDF (déjà réorganisé), page par page.
pub(crate) fn apply(bytes: Vec<u8>, pages: &[PageAnnotations]) -> Result<Vec<u8>, String> {
    if pages.iter().all(|p| p.annotations.is_empty()) {
        return Ok(bytes);
    }
    let mut doc = Document::load_mem(&bytes).map_err(|e| e.to_string())?;
    let ids = doc.get_pages();
    let mut serial = 0;
    for (index, p) in pages.iter().enumerate() {
        if p.annotations.is_empty() {
            continue;
        }
        let page_id: ObjectId = *ids.get(&(index as u32 + 1)).ok_or("page not found")?;
        let page = doc.get_dictionary(page_id).map_err(|e| e.to_string())?.clone();
        let bbox = page_box(&doc, &page).unwrap_or([0.0, 0.0, 612.0, 792.0]);
        let rotate = page_rotation(&doc, &page);
        // La rotation du PDF enregistré, moins celle choisie dans l'appli : celle de l'affichage
        // dans lequel l'interface a placé les annotations.
        let shown = Rotation::from_degrees(rotate - p.rotation.degrees());
        let frame = Frame::new(bbox, shown, p.unit_width);

        let mut annots: Vec<Object> = match page.get(b"Annots") {
            Ok(o) => match doc.dereference(o) {
                Ok((_, Object::Array(a))) => a.clone(),
                _ => Vec::new(),
            },
            Err(_) => Vec::new(),
        };
        for a in p.annotations {
            let Some(built) = build(a, &frame, &mut doc) else { continue };
            let bbox = built.bbox;
            let appearance = Stream::new(
                dictionary! {
                    "Type" => "XObject",
                    "Subtype" => "Form",
                    "BBox" => reals(&bbox),
                    "Matrix" => reals(&frame.matrix),
                    "Resources" => built.resources,
                },
                built.ops.into_bytes(),
            );
            let appearance_id = doc.add_object(appearance);
            serial += 1;
            let mut dict = built.dict;
            dict.set("Type", "Annot");
            dict.set("Rect", reals(&frame.user_box(bbox)));
            dict.set("P", Object::Reference(page_id));
            dict.set("F", Object::Integer(4)); // imprimable
            dict.set("NM", Object::string_literal(format!("documents-{index}-{serial}")));
            dict.set("AP", dictionary! { "N" => Object::Reference(appearance_id) });
            annots.push(Object::Reference(doc.add_object(dict)));
        }
        let page = doc.get_dictionary_mut(page_id).map_err(|e| e.to_string())?;
        page.set("Annots", Object::Array(annots));
    }
    let mut out = Vec::new();
    doc.save_to(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn couleur_depuis_et_vers_le_texte() {
        assert_eq!(Color::try_from("#ff8000".to_string()), Ok(Color(255, 128, 0)));
        assert!(Color::try_from("rouge".to_string()).is_err());
        assert_eq!(String::from(Color(1, 2, 255)), "#0102ff");
    }

    #[test]
    fn accents_en_winansi() {
        assert_eq!(win_ansi("été €"), vec![0xE9, b't', 0xE9, b' ', 0x80]);
        assert_eq!(win_ansi("中"), vec![b'?']);
        assert_eq!(literal(b"a(b)\\"), "(a\\(b\\)\\\\)");
    }

    #[test]
    fn coin_haut_gauche_selon_la_rotation() {
        // Page A4 portrait ; à 100 %, 1 point = 4/3 unité.
        let bbox = [0.0, 0.0, 595.0, 842.0];
        let f = Frame::new(bbox, Rotation::R0, 595.0 * 4.0 / 3.0);
        let p = f.user(f.d(Point { x: 0.0, y: 0.0 }));
        assert!((p.0 - 0.0).abs() < 0.01 && (p.1 - 842.0).abs() < 0.01);
        // Page affichée tournée d'un quart de tour : le coin en haut à gauche de l'écran
        // est le coin en bas à gauche de la page.
        let f = Frame::new(bbox, Rotation::R90, 842.0 * 4.0 / 3.0);
        let p = f.user(f.d(Point { x: 0.0, y: 0.0 }));
        assert!((p.0 - 0.0).abs() < 0.01 && (p.1 - 0.0).abs() < 0.01, "{p:?}");
    }

    #[test]
    fn aller_retour_unites_et_page() {
        let bbox = [10.0, 20.0, 605.0, 862.0];
        for r in [Rotation::R0, Rotation::R90, Rotation::R180, Rotation::R270] {
            let dw = if r.is_sideways() { 842.0 } else { 595.0 };
            let f = Frame::new(bbox, r, dw * 4.0 / 3.0);
            let p = Point { x: 123.0, y: 456.0 };
            let back = f.units(f.user(f.d(p)));
            assert!((back.x - p.x).abs() < 0.01 && (back.y - p.y).abs() < 0.01, "{r:?} {back:?}");
        }
    }

    #[test]
    fn decode_le_base64() {
        assert_eq!(base64("RG9jdW1lbnRz").unwrap(), "Documents".as_bytes());
        assert_eq!(base64("data:image/png;base64,TWE=").unwrap(), b"Ma");
        assert!(base64("!!").is_none());
    }

    #[test]
    fn rotation_autour_du_centre() {
        let t = Turn::new([0.0, 0.0, 20.0, 10.0], std::f32::consts::FRAC_PI_2);
        let (x, y) = t.apply((20.0, 5.0));
        assert!((x - 10.0).abs() < 1e-4 && (y - 15.0).abs() < 1e-4, "{x} {y}");
        let [a, b, c, d, e, f] = t.matrix();
        let (mx, my) = (a * 20.0 + c * 5.0 + e, b * 20.0 + d * 5.0 + f);
        assert!((mx - x).abs() < 1e-4 && (my - y).abs() < 1e-4);
        let bb = t.bounds([0.0, 0.0, 20.0, 10.0]);
        assert!((bb[0] - 5.0).abs() < 1e-4 && (bb[3] - 15.0).abs() < 1e-4, "{bb:?}");
        assert!(Turn::new([0.0, 0.0, 1.0, 1.0], 0.0).is_none());
    }

    #[test]
    fn contours_des_formes() {
        let r = Rect { x: 10.0, y: 20.0, w: 100.0, h: 50.0 };
        for shape in [Shape::Plain, Shape::Rounded, Shape::Bubble, Shape::Star, Shape::Hexagon] {
            let pts = outline(shape, r);
            let (x0, x1) = pts.iter().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.x), b.max(p.x)));
            let (y0, y1) = pts.iter().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.y), b.max(p.y)));
            // Chaque forme remplit sa zone, sans en sortir.
            for (got, want) in [(x0, 10.0), (x1, 110.0), (y0, 20.0), (y1, 70.0)] {
                assert!((got - want).abs() < 0.01, "{shape:?} : {got} au lieu de {want}");
            }
        }
        assert_eq!(outline(Shape::Star, r).len(), 10);
        // La pointe de la bulle descend jusqu'en bas, à gauche.
        let tail = outline(Shape::Bubble, r).into_iter().max_by(|a, b| a.y.total_cmp(&b.y)).unwrap();
        assert!(tail.x < 40.0);
    }

    #[test]
    fn nombres_courts() {
        assert_eq!(n(1.0), "1");
        assert_eq!(n(0.5), "0.5");
        assert_eq!(n(-0.0001), "0");
        assert_eq!(n(12.3456), "12.346");
    }
}
