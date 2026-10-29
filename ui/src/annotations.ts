// Annotations : surligner, formes, flèches, zones de texte, notes, dessin à main levée.
//
// Elles sont dessinées en SVG par-dessus chaque page, en « unités de page » (la page à 100 %,
// origine en haut à gauche), et ne touchent au fichier qu'à l'enregistrement : le cœur Rust
// les écrit alors comme de vraies annotations PDF (crates/core/src/annotate.rs), avec les
// mêmes formes, tailles et polices qu'ici.

import * as api from "./api";
import type { DocInfo, PageText } from "./api";
import { closeMenu, showMenu, showPanel } from "./menu";
import { canPan, startPan } from "./pan";
import { t as tr } from "./i18n";

export type Color = string;
export interface Pt {
  x: number;
  y: number;
}
export interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}
export type Font = "helvetica" | "times" | "courier";
export type Align = "left" | "center" | "right";
/** Police d'une zone de texte. */
export interface TextStyle {
  font: Font;
  size: number;
  bold?: boolean;
  italic?: boolean;
  underline?: boolean;
  align?: Align;
}
/** Formes dessinées dans un rectangle (sans rien : un rectangle simple). */
export type Shape = "rounded" | "bubble" | "star" | "hexagon";

/** `origin` : l'annotation du PDF d'où elle vient (reprise à l'ouverture). Le cœur la recopie
 * telle quelle à l'enregistrement si elle n'a pas changé. */
export type Annotation = { origin?: string } & (
  | { kind: "highlight"; rects: Box[]; color: Color }
  | { kind: "rect" | "ellipse"; rect: Box; stroke: Color | null; fill: Color | null; width: number; angle?: number; shape?: Shape }
  | { kind: "line"; from: Pt; to: Pt; color: Color; width: number; arrow: boolean }
  | { kind: "ink"; strokes: Pt[][]; color: Color; width: number }
  | ({ kind: "text"; rect: Box; text: string; lines: string[]; color: Color; fill: Color | null; widths?: number[] } & TextStyle)
  | { kind: "note"; at: Pt; text: string; color: Color }
  | { kind: "image"; rect: Box; png: string }
);

// Les mêmes valeurs que dans le cœur (annotate.rs).
const TEXT_PAD = 4;
const LINE_HEIGHT = 1.2;
const NOTE_SIZE = 24;
const BASELINE: Record<Font, number> = { helvetica: 0.947, times: 0.938, courier: 0.867 };
const UNDERLINE_DROP = 0.12;
const UNDERLINE_WIDTH = 0.06;
const arrowHead = (width: number) => width * 3 + 8;

export const FONT_CSS: Record<Font, string> = {
  helvetica: "Arial, Helvetica, 'Liberation Sans', sans-serif",
  times: "'Times New Roman', Times, 'Liberation Serif', serif",
  courier: "'Courier New', Courier, 'Liberation Mono', monospace",
};
const FONT_NAMES: Record<Font, string> = { helvetica: "Arial", times: "Times New Roman", courier: "Courier New" };

export interface AnnotPage {
  doc: DocInfo;
  index: number;
  rotation: number;
  el: HTMLElement;
  annots: Annotation[];
  hlSvg: SVGSVGElement;
  annotSvg: SVGSVGElement;
  thumbBox: HTMLElement;
}

export interface Host {
  pages(): AnnotPage[];
  /** Garde l'état actuel pour Ctrl+Z, puis applique la modification. */
  change(modify: () => void): void;
  /** Après une modification : mettre à jour le titre (« Modifié »). */
  changed(): void;
  /** La zone à recadrer a été choisie, modifiée ou retirée. */
  cropChanged(): void;
}

type ShapeTool = "rect" | "ellipse" | "line" | "arrow" | Shape;
type Tool = "select" | "highlight" | "ink" | ShapeTool | "text" | "note" | "signature" | "crop";
const SHAPE_TOOLS: ShapeTool[] = ["line", "arrow", "rect", "rounded", "ellipse", "bubble", "star", "hexagon"];

/** Une signature enregistrée : un dessin (traits ramenés dans un carré de 0 à 1) ou une image. */
export type Signature =
  | { id: string; kind: "ink"; strokes: Pt[][]; aspect: number }
  | { id: string; kind: "image"; png: string; aspect: number };

export const PALETTE: [string, Color][] = [
  [tr("colorBlack"), "#000000"], [tr("colorGray"), "#8e8e93"], [tr("colorWhite"), "#ffffff"], [tr("colorRed"), "#ff3b30"],
  [tr("colorOrange"), "#ff9500"], [tr("colorYellow"), "#ffcc00"], [tr("colorGreen"), "#34c759"], [tr("colorTeal"), "#30b0c7"],
  [tr("colorBlue"), "#007aff"], [tr("colorIndigo"), "#5856d6"], [tr("colorPurple"), "#af52de"], [tr("colorPink"), "#ff2d55"],
];
export const HIGHLIGHTS: [string, Color][] = [
  [tr("colorYellow"), "#ffe14d"], [tr("colorGreen"), "#a6f08c"], [tr("colorBlue"), "#8fd3ff"], [tr("colorPink"), "#ffa3c8"], [tr("colorPurple"), "#d4b0ff"],
];
const NOTE_COLOR = "#ffd60a";
const WIDTHS: [string, number][] = [[tr("widthThin"), 1], [tr("widthMedium"), 2], [tr("widthThick"), 4], [tr("widthExtraThick"), 8]];

export const style = {
  stroke: "#ff3b30" as Color,
  fill: null as Color | null,
  width: 2,
  font: "helvetica" as Font,
  size: 16,
  bold: false,
  italic: false,
  underline: false,
  align: "left" as Align,
  textColor: "#000000" as Color,
  highlight: "#ffe14d" as Color,
};

let host: Host;
let tool: Tool = "select";
let selected: { page: AnnotPage; i: number } | null = null;
let textSel: { page: AnnotPage; from: number; to: number } | null = null;
let editing: { page: AnnotPage; i: number | null; fo: SVGForeignObjectElement; area: HTMLTextAreaElement; rect: Box } | null = null;

const SVG = "http://www.w3.org/2000/svg";

function svg<K extends keyof SVGElementTagNameMap>(tag: K, attrs: Record<string, string | number> = {}) {
  const el = document.createElementNS(SVG, tag);
  for (const [k, v] of Object.entries(attrs)) el.setAttribute(k, String(v));
  return el;
}

const sizeOf = (p: AnnotPage) => p.doc.pages[p.index];

/** Échelle des traits et du texte : une photo de 4000 pixels demande des traits plus épais qu'une page A4. */
function factorOf(p: AnnotPage): number {
  const { width, height } = sizeOf(p);
  return p.doc.kind === "pdf" ? 1 : Math.max(1, Math.max(width, height) / 1100);
}

// ---------- Calques et coordonnées ----------

/** Les deux calques d'une page : surlignages (en « produit », comme un vrai surligneur) et le reste. */
export function createLayers(): { hlSvg: SVGSVGElement; annotSvg: SVGSVGElement } {
  const hlSvg = svg("svg", { class: "annot-layer hl-svg", preserveAspectRatio: "none" });
  const annotSvg = svg("svg", { class: "annot-layer main-svg", preserveAspectRatio: "none" });
  return { hlSvg, annotSvg };
}

export function attach(p: AnnotPage) {
  const { width, height } = sizeOf(p);
  for (const s of [p.hlSvg, p.annotSvg]) s.setAttribute("viewBox", `0 0 ${width} ${height}`);
  p.el.append(p.hlSvg, p.annotSvg);
  p.el.addEventListener("pointerdown", (e) => onPointerDown(p, e));
}

/** Place les calques sur la page affichée (tournée par l'utilisateur ou non). */
export function placeLayers(p: AnnotPage, pageW: number, pageH: number) {
  const side = p.rotation % 180 !== 0;
  const w = side ? pageH : pageW;
  const h = side ? pageW : pageH;
  for (const s of [p.hlSvg, p.annotSvg]) {
    s.style.width = `${w}px`;
    s.style.height = `${h}px`;
    s.style.transform = `translate(-50%, -50%) rotate(${p.rotation}deg)`;
  }
  // Les poignées et les zones de clic dépendent du zoom.
  if (p.annots.length || textSel?.page === p) render(p);
}

/** Pixels d'écran par unité de page. */
function scaleOf(p: AnnotPage): number {
  const r = p.el.getBoundingClientRect();
  const side = p.rotation % 180 !== 0;
  return (side ? r.height : r.width) / sizeOf(p).width;
}

function toUnits(p: AnnotPage, e: { clientX: number; clientY: number }): Pt {
  const r = p.el.getBoundingClientRect();
  const side = p.rotation % 180 !== 0;
  const lw = side ? r.height : r.width;
  const lh = side ? r.width : r.height;
  const dx = e.clientX - (r.left + r.width / 2);
  const dy = e.clientY - (r.top + r.height / 2);
  const a = (-p.rotation * Math.PI) / 180;
  const ux = dx * Math.cos(a) - dy * Math.sin(a);
  const uy = dx * Math.sin(a) + dy * Math.cos(a);
  const { width, height } = sizeOf(p);
  return { x: ((ux + lw / 2) / lw) * width, y: ((uy + lh / 2) / lh) * height };
}

/** Coin en haut à gauche d'un point de la page, en coordonnées d'écran (pour les fenêtres flottantes). */
function toScreen(p: AnnotPage, u: Pt): Pt {
  const r = p.el.getBoundingClientRect();
  const side = p.rotation % 180 !== 0;
  const lw = side ? r.height : r.width;
  const lh = side ? r.width : r.height;
  const { width, height } = sizeOf(p);
  const ux = (u.x / width) * lw - lw / 2;
  const uy = (u.y / height) * lh - lh / 2;
  const a = (p.rotation * Math.PI) / 180;
  return {
    x: r.left + r.width / 2 + ux * Math.cos(a) - uy * Math.sin(a),
    y: r.top + r.height / 2 + ux * Math.sin(a) + uy * Math.cos(a),
  };
}

// ---------- Géométrie ----------

const normBox = (a: Pt, b: Pt): Box => ({
  x: Math.min(a.x, b.x),
  y: Math.min(a.y, b.y),
  w: Math.abs(a.x - b.x),
  h: Math.abs(a.y - b.y),
});

/** Le contour d'une forme dans la zone `r`, dans le sens des aiguilles d'une montre : le même
 * calcul que dans le cœur (annotate.rs, `outline`), pour un enregistrement identique. */
export function outline(shape: Shape, r: Box): Pt[] {
  const at = (fx: number, fy: number) => ({ x: r.x + fx * r.w, y: r.y + fy * r.h });
  switch (shape) {
    case "hexagon":
      return [at(0.5, 0), at(1, 0.25), at(1, 0.75), at(0.5, 1), at(0, 0.75), at(0, 0.25)];
    case "star": {
      const sx = Math.cos((18 * Math.PI) / 180);
      const bottom = Math.sin((54 * Math.PI) / 180);
      return Array.from({ length: 10 }, (_, i) => {
        const a = ((-90 + 36 * i) * Math.PI) / 180;
        const k = i % 2 === 0 ? 1 : 0.382;
        return at((k * Math.cos(a) + sx) / (2 * sx), (k * Math.sin(a) + 1) / (bottom + 1));
      });
    }
    case "rounded":
      return rounded(r, r.h, null);
    case "bubble":
      return rounded(r, r.h * 0.78, [at(0.42, 0.78), at(0.16, 1), at(0.26, 0.78)]);
  }
}

function rounded(r: Box, h: number, tail: Pt[] | null): Pt[] {
  const rad = Math.max(0, Math.min(r.w, h) * 0.18);
  const corners: [number, number, number][] = [
    [r.x + r.w - rad, r.y + rad, -90],
    [r.x + r.w - rad, r.y + h - rad, 0],
    [r.x + rad, r.y + h - rad, 90],
    [r.x + rad, r.y + rad, 180],
  ];
  const out: Pt[] = [];
  corners.forEach(([cx, cy, start], k) => {
    for (let i = 0; i <= 6; i++) {
      const a = ((start + 15 * i) * Math.PI) / 180;
      out.push({ x: cx + rad * Math.cos(a), y: cy + rad * Math.sin(a) });
    }
    if (k === 1 && tail) out.push(...tail);
  });
  return out;
}

function boundsOf(a: Annotation): Box {
  switch (a.kind) {
    case "highlight": {
      const xs = a.rects.flatMap((r) => [r.x, r.x + r.w]);
      const ys = a.rects.flatMap((r) => [r.y, r.y + r.h]);
      return normBox({ x: Math.min(...xs), y: Math.min(...ys) }, { x: Math.max(...xs), y: Math.max(...ys) });
    }
    case "rect":
    case "ellipse":
    case "text":
      return a.rect;
    case "line":
      return normBox(a.from, a.to);
    case "ink": {
      const pts = a.strokes.flat();
      const xs = pts.map((p) => p.x);
      const ys = pts.map((p) => p.y);
      const m = a.width / 2;
      return normBox({ x: Math.min(...xs) - m, y: Math.min(...ys) - m }, { x: Math.max(...xs) + m, y: Math.max(...ys) + m });
    }
    case "note":
      return { x: a.at.x, y: a.at.y, w: NOTE_SIZE, h: NOTE_SIZE };
    case "image":
      return a.rect;
  }
}

export function moved(a: Annotation, dx: number, dy: number): Annotation {
  const pt = (p: Pt) => ({ x: p.x + dx, y: p.y + dy });
  const box = (b: Box) => ({ ...b, x: b.x + dx, y: b.y + dy });
  switch (a.kind) {
    case "highlight":
      return { ...a, rects: a.rects.map(box) };
    case "rect":
    case "ellipse":
    case "text":
    case "image":
      return { ...a, rect: box(a.rect) } as Annotation;
    case "line":
      return { ...a, from: pt(a.from), to: pt(a.to) };
    case "ink":
      return { ...a, strokes: a.strokes.map((s) => s.map(pt)) };
    case "note":
      return { ...a, at: pt(a.at) };
  }
}

// ---------- Texte des zones de texte ----------

const measure = document.createElement("canvas").getContext("2d")!;

/** La police CSS d'une zone de texte (gras, italique compris). */
export const cssFont = (t: TextStyle) =>
  `${t.italic ? "italic " : ""}${t.bold ? "bold " : ""}${t.size}px ${FONT_CSS[t.font]}`;

/** Largeur d'une ligne, en unités de page. */
function lineWidth(line: string, t: TextStyle): number {
  measure.font = cssFont(t);
  return measure.measureText(line).width;
}

/** Coupe le texte en lignes qui tiennent dans la largeur de la zone (comme à l'enregistrement). */
export function wrap(text: string, t: TextStyle, width: number): string[] {
  const size = t.size;
  measure.font = cssFont(t);
  const max = Math.max(size, width - 2 * TEXT_PAD);
  const fits = (s: string) => measure.measureText(s).width <= max;
  const out: string[] = [];
  for (const para of text.split("\n")) {
    let line = "";
    for (const word of para.split(/(\s+)/)) {
      if (!word) continue;
      if (fits(line + word)) {
        line += word;
        continue;
      }
      if (line.trim()) out.push(line.trimEnd());
      line = word.trimStart();
      // Mot plus long que la zone : on le coupe lettre par lettre.
      while (line && !fits(line)) {
        let cut = 1;
        while (cut < line.length && fits(line.slice(0, cut + 1))) cut++;
        out.push(line.slice(0, cut));
        line = line.slice(cut);
      }
    }
    out.push(line.trimEnd());
  }
  return out;
}

const textHeight = (lines: number, size: number) => Math.max(1, lines) * size * LINE_HEIGHT + 2 * TEXT_PAD;

function withText(a: Extract<Annotation, { kind: "text" }>, changes: Partial<Extract<Annotation, { kind: "text" }>>) {
  const t = { ...a, ...changes };
  t.lines = wrap(t.text, t, t.rect.w);
  t.widths = t.lines.map((l) => lineWidth(l, t));
  t.rect = { ...t.rect, h: textHeight(t.lines.length, t.size) };
  return t;
}

/** Les annotations reprises d'un PDF : le texte d'une zone venue d'un autre logiciel est
 * recoupé ici, à la largeur de sa zone (sans changer la zone). */
export function imported(list: unknown[] | undefined): Annotation[] {
  return ((list ?? []) as Annotation[]).map((a) => {
    // Le cœur écrit « plain » pour un rectangle simple ; ici, c'est une forme absente.
    if (a.kind === "rect" && (a.shape as string) === "plain") {
      const { shape: _, ...plain } = a;
      return plain;
    }
    if (a.kind !== "text" || a.widths?.length) return a;
    const text = a.text ?? a.lines.join("\n");
    const lines = wrap(text, a, a.rect.w);
    return { ...a, text, lines, widths: lines.map((l) => lineWidth(l, a)) };
  });
}

// ---------- Dessin ----------

function drawAnnot(a: Annotation, scale: number): { hl?: SVGElement; main?: SVGElement } {
  const g = svg("g", { class: "annot" });
  const hit = (el: SVGElement, width: number) => {
    // Un trait invisible plus large, pour attraper facilement les traits fins.
    const h = el.cloneNode() as SVGElement;
    h.setAttribute("stroke", "transparent");
    h.setAttribute("stroke-width", String(width + 10 / scale));
    h.setAttribute("fill", "none");
    h.setAttribute("class", "hit");
    g.append(h);
  };
  switch (a.kind) {
    case "highlight": {
      for (const r of a.rects) g.append(svg("rect", { x: r.x, y: r.y, width: r.w, height: r.h, fill: a.color }));
      return { hl: g };
    }
    case "rect":
    case "ellipse": {
      const w = a.stroke ? a.width : 0;
      const r = a.rect;
      const paint = {
        fill: a.fill ?? "none",
        stroke: a.stroke ?? "none",
        "stroke-width": w,
        "pointer-events": "visible",
      };
      const inset = { x: r.x + w / 2, y: r.y + w / 2, w: Math.max(0, r.w - w), h: Math.max(0, r.h - w) };
      const shape =
        a.kind === "rect" && a.shape
          ? svg("polygon", { points: outline(a.shape, inset).map((q) => `${q.x},${q.y}`).join(" "), "stroke-linejoin": "round", ...paint })
          : a.kind === "rect"
          ? svg("rect", { x: r.x + w / 2, y: r.y + w / 2, width: Math.max(0, r.w - w), height: Math.max(0, r.h - w), ...paint })
          : svg("ellipse", { cx: r.x + r.w / 2, cy: r.y + r.h / 2, rx: Math.max(0, (r.w - w) / 2), ry: Math.max(0, (r.h - w) / 2), ...paint });
      if (a.angle) shape.setAttribute("transform", turnAttr(r, a.angle));
      g.append(shape);
      return { main: g };
    }
    case "line": {
      const { from, to, width } = a;
      const len = Math.hypot(to.x - from.x, to.y - from.y) || 0.001;
      const ux = (to.x - from.x) / len;
      const uy = (to.y - from.y) / len;
      const head = a.arrow ? Math.min(arrowHead(width), len) : 0;
      const end = a.arrow ? { x: to.x - ux * head * 0.8, y: to.y - uy * head * 0.8 } : to;
      const line = svg("line", {
        x1: from.x, y1: from.y, x2: end.x, y2: end.y,
        stroke: a.color, "stroke-width": width, "stroke-linecap": "round",
      });
      hit(svg("line", { x1: from.x, y1: from.y, x2: to.x, y2: to.y }), width);
      g.append(line);
      if (a.arrow) {
        const bx = to.x - ux * head;
        const by = to.y - uy * head;
        const half = head * 0.5;
        g.append(
          svg("polygon", {
            points: `${to.x},${to.y} ${bx + uy * half},${by - ux * half} ${bx - uy * half},${by + ux * half}`,
            fill: a.color,
          }),
        );
      }
      return { main: g };
    }
    case "ink": {
      for (const stroke of a.strokes) {
        if (stroke.length === 1) {
          g.append(svg("circle", { cx: stroke[0].x, cy: stroke[0].y, r: a.width / 2, fill: a.color }));
          continue;
        }
        const pl = svg("polyline", {
          points: stroke.map((p) => `${p.x},${p.y}`).join(" "),
          fill: "none", stroke: a.color, "stroke-width": a.width,
          "stroke-linecap": "round", "stroke-linejoin": "round",
        });
        hit(pl, a.width);
        g.append(pl);
      }
      return { main: g };
    }
    case "text": {
      const r = a.rect;
      g.append(svg("rect", { x: r.x, y: r.y, width: r.w, height: r.h, fill: a.fill ?? "none", "pointer-events": "visible" }));
      const text = svg("text", {
        "font-family": FONT_CSS[a.font],
        "font-size": a.size,
        "font-weight": a.bold ? "bold" : "normal",
        "font-style": a.italic ? "italic" : "normal",
        fill: a.color,
        "xml:space": "preserve",
      });
      const room = r.w - 2 * TEXT_PAD;
      a.lines.forEach((line, i) => {
        const w = a.widths?.[i] ?? 0;
        const x = r.x + TEXT_PAD + (a.align === "center" ? (room - w) / 2 : a.align === "right" ? room - w : 0);
        const y = r.y + TEXT_PAD + a.size * BASELINE[a.font] + i * a.size * LINE_HEIGHT;
        const t = svg("tspan", { x, y });
        t.textContent = line;
        text.append(t);
        if (a.underline && w > 0) {
          const uy = y + a.size * UNDERLINE_DROP;
          g.append(svg("line", { x1: x, y1: uy, x2: x + w, y2: uy, stroke: a.color, "stroke-width": a.size * UNDERLINE_WIDTH }));
        }
      });
      g.append(text);
      return { main: g };
    }
    case "note": {
      const { x, y } = a.at;
      const s = NOTE_SIZE;
      const f = s * 0.3;
      g.append(
        svg("path", {
          d: `M${x},${y + s} L${x + s - f},${y + s} L${x + s},${y + s - f} L${x + s},${y} L${x},${y} Z`,
          fill: a.color, stroke: "#59501a", "stroke-width": s * 0.04, "pointer-events": "visible",
        }),
        svg("path", {
          d: `M${x + s - f},${y + s} L${x + s - f},${y + s - f} L${x + s},${y + s - f}`,
          fill: "none", stroke: "#59501a", "stroke-width": s * 0.04,
        }),
      );
      for (let i = 1; i <= 3; i++) {
        const ly = y + s * 0.22 * i;
        g.append(svg("line", { x1: x + s * 0.18, y1: ly, x2: x + s - s * (i === 3 ? 0.42 : 0.18), y2: ly, stroke: "#59501a", "stroke-width": s * 0.04 }));
      }
      return { main: g };
    }
    case "image": {
      const r = a.rect;
      g.append(svg("image", { href: a.png, x: r.x, y: r.y, width: r.w, height: r.h, preserveAspectRatio: "none", "pointer-events": "visible" }));
      return { main: g };
    }
  }
}

/** Redessine les annotations d'une page, avec la sélection et le texte sélectionné. */
export function render(p: AnnotPage) {
  const scale = scaleOf(p) || 1;
  const hl: SVGElement[] = [];
  const main: SVGElement[] = [];
  p.annots.forEach((a, i) => {
    if (editing?.page === p && editing.i === i) return;
    const d = drawAnnot(a, scale);
    for (const g of [d.hl, d.main]) if (g) g.dataset.i = String(i);
    if (d.hl) hl.push(d.hl);
    if (d.main) main.push(d.main);
  });
  if (textSel?.page === p) hl.push(...selectionRects(p));
  if (cropSel?.page === p) main.push(cropFrame(p, cropSel.rect, scale));
  if (selected?.page === p && p.annots[selected.i]) main.push(selectionFrame(p, p.annots[selected.i], scale));
  p.hlSvg.replaceChildren(...hl);
  const fo = editing?.page === p ? editing.fo : null;
  p.annotSvg.replaceChildren(...main, ...(fo ? [fo] : []));
  renderThumb(p);
}

/** Les annotations aussi sur la miniature. */
function renderThumb(p: AnnotPage) {
  overlay(p, p.thumbBox);
}

/** Dessine les annotations de la page par-dessus une petite image de la page (miniature,
 * planche contact), dont `box` a la taille. */
export function overlay(p: AnnotPage, box: HTMLElement) {
  let t = box.querySelector<SVGSVGElement>(":scope > svg.thumb-annots");
  if (!p.annots.length) {
    t?.remove();
    return;
  }
  if (!t) {
    t = svg("svg", { class: "thumb-annots", preserveAspectRatio: "none" });
    box.append(t);
  }
  const { width, height } = sizeOf(p);
  t.setAttribute("viewBox", `0 0 ${width} ${height}`);
  const side = p.rotation % 180 !== 0;
  const bw = parseFloat(box.style.width) || 0;
  const bh = parseFloat(box.style.height) || 0;
  t.style.width = `${side ? bh : bw}px`;
  t.style.height = `${side ? bw : bh}px`;
  t.style.transform = `translate(-50%, -50%) rotate(${p.rotation}deg)`;
  t.replaceChildren(
    ...p.annots.flatMap((a) => {
      const d = drawAnnot(a, 1);
      d.hl?.setAttribute("opacity", "0.55");
      return [d.hl, d.main].filter((g): g is SVGElement => !!g);
    }),
  );
}

export function renderAll() {
  host.pages().forEach(render);
}

/** Rotation d'une forme autour de son centre, pour l'attribut SVG « transform ». */
const turnAttr = (r: Box, angle: number) => `rotate(${angle} ${r.x + r.w / 2} ${r.y + r.h / 2})`;

/** Un point tourné de `angle` degrés (sens des aiguilles d'une montre à l'écran) autour de `c`. */
function turnPt(pt: Pt, c: Pt, angle: number): Pt {
  const a = (angle * Math.PI) / 180;
  const dx = pt.x - c.x;
  const dy = pt.y - c.y;
  return { x: c.x + dx * Math.cos(a) - dy * Math.sin(a), y: c.y + dx * Math.sin(a) + dy * Math.cos(a) };
}

const centerOf = (r: Box): Pt => ({ x: r.x + r.w / 2, y: r.y + r.h / 2 });

/** Distance de la poignée de rotation au coin en bas à droite, en pixels d'écran. */
const ROTATE_GAP = 20;

function selectionFrame(_p: AnnotPage, a: Annotation, scale: number): SVGElement {
  const g = svg("g", { class: "selection" });
  const b = boundsOf(a);
  // Rectangle, ovale : le cadre tourne avec la forme, et une poignée en plus sert à la tourner.
  const shape = a.kind === "rect" || a.kind === "ellipse";
  if (shape && a.angle) g.setAttribute("transform", turnAttr(b, a.angle));
  const m = 3 / scale;
  g.append(
    svg("rect", {
      x: b.x - m, y: b.y - m, width: b.w + 2 * m, height: b.h + 2 * m,
      fill: "none", stroke: "#0a84ff", "stroke-width": 1 / scale, "stroke-dasharray": `${4 / scale} ${3 / scale}`,
      "pointer-events": "none",
    }),
  );
  // Des points bleus aux coins et au milieu des bords (zone d'accroche plus large).
  const handle = (name: string, x: number, y: number) => {
    const s = 9 / scale;
    const grip = svg("g", { "data-handle": name, class: `handle handle-${name}` });
    grip.append(
      svg("circle", { cx: x, cy: y, r: 9 / scale, fill: "transparent" }),
      svg("circle", { cx: x, cy: y, r: s / 2, fill: "#0a84ff", stroke: "#ffffff", "stroke-width": 1.2 / scale }),
    );
    g.append(grip);
  };
  if (a.kind === "line") {
    handle("from", a.from.x, a.from.y);
    handle("to", a.to.x, a.to.y);
  } else if (shape) {
    const d = ROTATE_GAP / scale;
    const s = 17 / scale;
    g.append(
      svg("line", {
        x1: b.x + b.w, y1: b.y + b.h, x2: b.x + b.w + d * 0.7, y2: b.y + b.h + d * 0.7,
        stroke: "#0a84ff", "stroke-width": 1 / scale, "pointer-events": "none",
      }),
    );
    handle("nw", b.x, b.y);
    handle("n", b.x + b.w / 2, b.y);
    handle("ne", b.x + b.w, b.y);
    handle("e", b.x + b.w, b.y + b.h / 2);
    handle("se", b.x + b.w, b.y + b.h);
    handle("s", b.x + b.w / 2, b.y + b.h);
    handle("sw", b.x, b.y + b.h);
    handle("w", b.x, b.y + b.h / 2);
    // La poignée de rotation : un rond bleu avec une flèche en arc.
    const rot = svg("g", { "data-handle": "rotate", class: "handle handle-rotate" });
    const cx = b.x + b.w + d;
    const cy = b.y + b.h + d;
    const k = s * 0.28;
    rot.append(
      svg("circle", { cx, cy, r: s / 2, fill: "#0a84ff", stroke: "#ffffff", "stroke-width": 1.5 / scale }),
      svg("path", {
        d: `M${cx + k} ${cy - k} A${k * 1.4} ${k * 1.4} 0 1 0 ${cx + k * 1.4} ${cy + k * 0.2}`,
        fill: "none", stroke: "#ffffff", "stroke-width": 1.3 / scale, "stroke-linecap": "round",
      }),
      svg("path", {
        d: `M${cx + k * 0.15} ${cy - k * 1.05} L${cx + k * 1.05} ${cy - k} L${cx + k * 1.1} ${cy - k * 0.05}`,
        fill: "none", stroke: "#ffffff", "stroke-width": 1.3 / scale, "stroke-linecap": "round", "stroke-linejoin": "round",
      }),
    );
    g.append(rot);
  } else if (a.kind === "image" || a.kind === "ink") {
    handle("nw", b.x, b.y);
    handle("ne", b.x + b.w, b.y);
    handle("sw", b.x, b.y + b.h);
    handle("se", b.x + b.w, b.y + b.h);
  } else if (a.kind === "text") {
    handle("w", b.x, b.y + b.h / 2);
    handle("e", b.x + b.w, b.y + b.h / 2);
  }
  return g;
}

// ---------- Sélection de texte (pour copier ou surligner) ----------

const texts = new Map<string, Promise<PageText>>();

function pageText(p: AnnotPage): Promise<PageText> {
  const key = `${p.doc.id}/${p.index}`;
  let t = texts.get(key);
  if (!t) {
    t = p.doc.kind === "pdf" ? api.pageText(p.doc.id, p.index).catch(() => ({ chars: [], rects: [] })) : Promise.resolve({ chars: [], rects: [] });
    texts.set(key, t);
  }
  return t;
}

const loadedTexts = new Map<string, PageText>();

function charAt(p: AnnotPage, t: PageText, u: Pt): number {
  const { width, height } = sizeOf(p);
  let best = -1;
  let bestD = Infinity;
  t.rects.forEach((r, i) => {
    if (!r.w) return;
    const x0 = r.x * width;
    const y0 = r.y * height;
    const x1 = x0 + r.w * width;
    const y1 = y0 + r.h * height;
    const dx = u.x < x0 ? x0 - u.x : u.x > x1 ? u.x - x1 : 0;
    const dy = u.y < y0 ? y0 - u.y : u.y > y1 ? u.y - y1 : 0;
    const d = dy * 4 + dx;
    if (d < bestD) {
      bestD = d;
      best = i;
    }
  });
  return best;
}

/** Vrai si le point touche du texte (ou si le texte de la page n'est pas encore connu). */
function overText(p: AnnotPage, u: Pt): boolean {
  if (p.doc.kind !== "pdf") return false;
  const t = loadedTexts.get(`${p.doc.id}/${p.index}`);
  if (!t) return true;
  const { width, height } = sizeOf(p);
  const m = 4 * factorOf(p);
  return t.rects.some(
    (r) => r.w > 0 && u.x >= r.x * width - m && u.x <= (r.x + r.w) * width + m && u.y >= r.y * height - m && u.y <= (r.y + r.h) * height + m,
  );
}

/** Les rectangles d'une plage de caractères, regroupés par ligne. */
function lineRects(p: AnnotPage, t: PageText, from: number, to: number): Box[] {
  const { width, height } = sizeOf(p);
  const out: Box[] = [];
  for (let i = from; i <= to; i++) {
    const r = t.rects[i];
    if (!r?.w) continue;
    const b = { x: r.x * width, y: r.y * height, w: r.w * width, h: r.h * height };
    const last = out[out.length - 1];
    const sameLine = last && Math.abs(b.y + b.h / 2 - (last.y + last.h / 2)) < Math.min(b.h, last.h) * 0.6 && b.x >= last.x - 2;
    if (sameLine) {
      const x0 = Math.min(last.x, b.x);
      const y0 = Math.min(last.y, b.y);
      const x1 = Math.max(last.x + last.w, b.x + b.w);
      const y1 = Math.max(last.y + last.h, b.y + b.h);
      out[out.length - 1] = { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
    } else {
      out.push(b);
    }
  }
  return out;
}

function selectionRects(p: AnnotPage): SVGElement[] {
  const t = loadedTexts.get(`${p.doc.id}/${p.index}`);
  if (!t || !textSel) return [];
  const color = tool === "highlight" ? style.highlight : "#8fc4ff";
  return lineRects(p, t, textSel.from, textSel.to).map((r) =>
    svg("rect", { x: r.x, y: r.y, width: r.w, height: r.h, fill: color, "pointer-events": "none" }),
  );
}

function selectedText(): string {
  if (!textSel) return "";
  const t = loadedTexts.get(`${textSel.page.doc.id}/${textSel.page.index}`);
  return t ? t.chars.slice(textSel.from, textSel.to + 1).join("") : "";
}

function clearTextSelection() {
  if (!textSel) return;
  const p = textSel.page;
  textSel = null;
  render(p);
}

/** Transforme le texte sélectionné en surlignage. */
function highlightSelection(): boolean {
  if (!textSel) return false;
  const p = textSel.page;
  const t = loadedTexts.get(`${p.doc.id}/${p.index}`);
  const rects = t ? lineRects(p, t, textSel.from, textSel.to) : [];
  textSel = null;
  if (rects.length) add(p, { kind: "highlight", rects, color: style.highlight }, false);
  else render(p);
  return rects.length > 0;
}

// ---------- Modifier les annotations ----------

function add(p: AnnotPage, a: Annotation, select = true) {
  host.change(() => {
    p.annots = [...p.annots, a];
  });
  if (select) selectAnnot(p, p.annots.length - 1);
  else render(p);
  host.changed();
}

function replace(p: AnnotPage, i: number, a: Annotation) {
  p.annots = p.annots.map((x, k) => (k === i ? a : x));
}

function selectAnnot(p: AnnotPage, i: number | null) {
  const before = selected?.page;
  selected = i === null ? null : { page: p, i };
  if (before && before !== p) render(before);
  render(p);
  updateSwatches();
}

/** Choisit une annotation depuis la liste de l'inspecteur. */
export function selectAt(p: AnnotPage, i: number) {
  if (p.annots[i]) selectAnnot(p, i);
}

export function deselect() {
  if (!selected) return;
  const p = selected.page;
  selected = null;
  render(p);
  updateSwatches();
}

function deleteSelected() {
  if (!selected) return;
  const { page, i } = selected;
  selected = null;
  host.change(() => {
    page.annots = page.annots.filter((_, k) => k !== i);
  });
  render(page);
  host.changed();
  updateSwatches();
}

/** Applique un changement de style à l'annotation sélectionnée (si elle est concernée). */
function restyle(change: (a: Annotation, f: number) => Annotation | null) {
  if (!selected) return;
  const { page, i } = selected;
  const a = page.annots[i];
  const b = a && change(a, factorOf(page));
  if (!b) return;
  host.change(() => replace(page, i, b));
  render(page);
  host.changed();
}

// ---------- Souris ----------

function drag(
  e: PointerEvent,
  onMove: (ev: PointerEvent) => void,
  onUp: (ev: PointerEvent, moved: boolean) => void,
) {
  e.preventDefault();
  const sx = e.clientX;
  const sy = e.clientY;
  let movedOnce = false;
  const move = (ev: PointerEvent) => {
    if (!movedOnce && Math.hypot(ev.clientX - sx, ev.clientY - sy) < 3) return;
    movedOnce = true;
    onMove(ev);
  };
  const up = (ev: PointerEvent) => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
    onUp(ev, movedOnce);
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
}

function onPointerDown(p: AnnotPage, e: PointerEvent) {
  if (e.button !== 0) return;
  if ((e.target as Element).closest?.(".form-layer")) return; // un champ de formulaire
  if (editing) {
    if (editing.fo.contains(e.target as Node)) return;
    finishEditing();
  }
  closeMenu();
  const target = e.target as Element;
  const grip = target.closest?.("[data-crop]") as SVGElement | null;
  if (grip && cropSel?.page === p) return startCropAdjust(p, e, grip.dataset.crop!);
  // Cadre de recadrage ouvert par le bouton « Recadrer » : un clic à côté ne fait rien.
  if (cropSel && tool !== "crop") return;
  const handle = target.closest?.("[data-handle]") as SVGElement | null;
  if (handle && selected?.page === p) return startResize(p, e, handle.dataset.handle!);
  const hit = target.closest?.("[data-i]") as SVGElement | null;
  if (hit && tool === "select") {
    const i = Number(hit.dataset.i);
    const now = performance.now();
    // Double-clic sur une zone de texte : on la modifie.
    const again = lastDown.page === p && lastDown.i === i && now - lastDown.time < 450;
    lastDown = { time: now, page: p, i };
    const a = p.annots[i];
    if (again && a?.kind === "text") {
      e.preventDefault();
      return editText(p, i, a.rect);
    }
    return startMove(p, e, i);
  }

  deselect();
  clearTextSelection();
  const u = toUnits(p, e);
  // Document zoomé plus grand que la fenêtre : on l'attrape pour le déplacer (photo, ou
  // endroit d'une page PDF sans texte).
  if (tool === "select" && canPan() && !overText(p, u)) return startPan(e);
  switch (tool) {
    case "select":
    case "highlight":
      return startTextSelection(p, e, u);
    case "ink":
      return startInk(p, e, u);
    case "crop":
      return startCrop(p, e, u);
    case "rect":
    case "ellipse":
    case "line":
    case "arrow":
    case "rounded":
    case "bubble":
    case "star":
    case "hexagon":
      return startShape(p, e, u, tool);
    case "text":
      e.preventDefault();
      setTool("select");
      return editText(p, null, {
        x: u.x,
        y: u.y - style.size * factorOf(p) * 0.6,
        w: 200 * factorOf(p),
        h: textHeight(1, style.size * factorOf(p)),
      });
    case "note": {
      e.preventDefault();
      setTool("select");
      if (p.doc.kind !== "pdf") return; // une photo n'a pas de notes : rien pour les afficher
      add(p, { kind: "note", at: { x: u.x - NOTE_SIZE / 2, y: u.y - NOTE_SIZE / 2 }, text: "", color: NOTE_COLOR });
      return openNote(p, p.annots.length - 1);
    }
    case "signature": {
      e.preventDefault();
      setTool("select");
      if (pendingSignature) add(p, signatureAt(pendingSignature, u, factorOf(p)));
      pendingSignature = null;
      return;
    }
  }
}

let pendingSignature: Signature | null = null;

/** Choisir une signature : le prochain clic sur une page la pose. */
export function placeSignature(sig: Signature) {
  pendingSignature = sig;
  setTool("signature");
}

/** La signature posée, centrée sur le clic (environ 4 cm de large sur une page A4). */
function signatureAt(sig: Signature, at: Pt, f: number): Annotation {
  const w = 150 * f;
  const h = w / sig.aspect;
  const x = at.x - w / 2;
  const y = at.y - h / 2;
  if (sig.kind === "image") return { kind: "image", rect: { x, y, w, h }, png: sig.png };
  return {
    kind: "ink",
    strokes: sig.strokes.map((st) => st.map((pt) => ({ x: x + pt.x * w, y: y + pt.y * h }))),
    color: "#0b1f66",
    width: Math.max(1, w * 0.012),
  };
}

/** Le style choisi pour les prochaines zones de texte. */
const textDefaults = (): TextStyle => ({
  font: style.font, size: style.size, bold: style.bold, italic: style.italic, underline: style.underline, align: style.align,
});

let lastDown = { time: 0, page: null as AnnotPage | null, i: -1 };

function startMove(p: AnnotPage, e: PointerEvent, i: number) {
  clearTextSelection();
  selectAnnot(p, i);
  const start = toUnits(p, e);
  const original = p.annots[i];
  let first = true;
  drag(
    e,
    (ev) => {
      if (first) {
        host.change(() => {});
        first = false;
      }
      const u = toUnits(p, ev);
      replace(p, i, moved(original, u.x - start.x, u.y - start.y));
      render(p);
    },
    (_ev, didMove) => {
      if (didMove) host.changed();
      else if (original.kind === "note") openNote(p, i);
    },
  );
}

function startResize(p: AnnotPage, e: PointerEvent, name: string) {
  const { i } = selected!;
  const original = p.annots[i];
  let first = true;
  // Rotation : là où la souris a commencé à tourner.
  let startPt: Pt | null = name === "rotate" ? toUnits(p, e) : null;
  drag(
    e,
    (ev) => {
      if (first) {
        host.change(() => {});
        first = false;
      }
      const u = toUnits(p, ev);
      let next: Annotation = original;
      if (original.kind === "line") {
        const fixed = name === "from" ? original.to : original.from;
        const end = ev.shiftKey ? snap45(fixed, u) : u;
        next = name === "from" ? { ...original, from: end } : { ...original, to: end };
      } else if ((original.kind === "rect" || original.kind === "ellipse") && name === "rotate") {
        // Tourner : l'angle suit la souris autour du centre ; Maj : par pas de 15°.
        const c = centerOf(original.rect);
        const start = startPt ?? u;
        startPt = start;
        let angle = (original.angle ?? 0) + ((Math.atan2(u.y - c.y, u.x - c.x) - Math.atan2(start.y - c.y, start.x - c.x)) * 180) / Math.PI;
        if (ev.shiftKey) angle = Math.round(angle / 15) * 15;
        angle = ((angle % 360) + 540) % 360 - 180;
        next = { ...original, angle: Math.abs(angle) < 0.01 ? 0 : angle };
      } else if (original.kind === "rect" || original.kind === "ellipse") {
        // On redimensionne dans le repère de la forme (droite), le coin opposé ne bouge pas.
        const r = original.rect;
        const angle = original.angle ?? 0;
        const c = centerOf(r);
        const local = turnPt(u, c, -angle);
        // Un coin bouge les deux côtés qui le touchent, le milieu d'un bord seulement ce bord.
        let [x0, y0, x1, y1] = [r.x, r.y, r.x + r.w, r.y + r.h];
        if (name.includes("w")) x0 = local.x;
        if (name.includes("e")) x1 = local.x;
        if (name.includes("n")) y0 = local.y;
        if (name.includes("s")) y1 = local.y;
        const box = normBox({ x: x0, y: y0 }, { x: x1, y: y1 });
        const nc = turnPt(centerOf(box), c, angle);
        next = { ...original, rect: { ...box, x: nc.x - box.w / 2, y: nc.y - box.h / 2 } };
      } else if (original.kind === "image" || original.kind === "ink") {
        // Signature, dessin : on agrandit sans déformer, depuis le coin opposé.
        const r = boundsOf(original);
        const fixed = { x: name.includes("w") ? r.x + r.w : r.x, y: name.includes("n") ? r.y + r.h : r.y };
        const k = Math.max(0.05, Math.max(Math.abs(u.x - fixed.x) / r.w, Math.abs(u.y - fixed.y) / r.h));
        const nx = name.includes("w") ? fixed.x - r.w * k : fixed.x;
        const ny = name.includes("n") ? fixed.y - r.h * k : fixed.y;
        const map = (pt: Pt) => ({ x: nx + (pt.x - r.x) * k, y: ny + (pt.y - r.y) * k });
        next =
          original.kind === "image"
            ? { ...original, rect: { x: nx, y: ny, w: r.w * k, h: r.h * k } }
            : { ...original, strokes: original.strokes.map((st) => st.map(map)), width: original.width * k };
      } else if (original.kind === "text") {
        const r = original.rect;
        const right = r.x + r.w;
        const rect = name === "e" ? { ...r, w: Math.max(30, u.x - r.x) } : { ...r, x: Math.min(u.x, right - 30), w: Math.max(30, right - u.x) };
        next = withText(original, { rect });
      }
      replace(p, i, next);
      render(p);
    },
    (_ev, didMove) => {
      if (didMove) host.changed();
    },
  );
}

/** Ligne à l'horizontale, à la verticale ou à 45° (touche Maj). */
function snap45(from: Pt, to: Pt): Pt {
  const dx = to.x - from.x;
  const dy = to.y - from.y;
  const len = Math.hypot(dx, dy);
  const angle = Math.round(Math.atan2(dy, dx) / (Math.PI / 4)) * (Math.PI / 4);
  return { x: from.x + len * Math.cos(angle), y: from.y + len * Math.sin(angle) };
}

function startTextSelection(p: AnnotPage, e: PointerEvent, u: Pt) {
  clearTextSelection();
  if (p.doc.kind !== "pdf") return;
  e.preventDefault();
  const key = `${p.doc.id}/${p.index}`;
  let start = -1;
  let last: PointerEvent | null = null;
  const update = (ev: PointerEvent) => {
    const t = loadedTexts.get(key);
    if (!t) return;
    if (start < 0) start = charAt(p, t, u);
    const end = charAt(p, t, toUnits(p, ev));
    if (start < 0 || end < 0) return;
    textSel = { page: p, from: Math.min(start, end), to: Math.max(start, end) };
    render(p);
  };
  pageText(p).then((t) => {
    loadedTexts.set(key, t);
    if (last) update(last);
  });
  drag(
    e,
    (ev) => {
      last = ev;
      update(ev);
    },
    (_ev, didMove) => {
      if (!didMove) clearTextSelection();
      else if (tool === "highlight") highlightSelection();
    },
  );
}

function startInk(p: AnnotPage, e: PointerEvent, u: Pt) {
  const points: Pt[] = [u];
  const minStep = 1.5 / (scaleOf(p) || 1);
  const preview = svg("polyline", {
    fill: "none", stroke: style.stroke, "stroke-width": style.width * factorOf(p),
    "stroke-linecap": "round", "stroke-linejoin": "round", points: `${u.x},${u.y}`,
  });
  p.annotSvg.append(preview);
  drag(
    e,
    (ev) => {
      const q = toUnits(p, ev);
      const last = points[points.length - 1];
      if (Math.hypot(q.x - last.x, q.y - last.y) < minStep) return;
      points.push(q);
      preview.setAttribute("points", points.map((pt) => `${pt.x},${pt.y}`).join(" "));
    },
    () => {
      preview.remove();
      add(p, { kind: "ink", strokes: [simplify(points, minStep / 2)], color: style.stroke, width: style.width * factorOf(p) }, false);
    },
  );
}

/** Retire les points inutiles d'un tracé (Ramer-Douglas-Peucker), pour un fichier léger. */
function simplify(points: Pt[], tolerance: number): Pt[] {
  if (points.length < 3) return points;
  const keep = new Uint8Array(points.length);
  keep[0] = keep[points.length - 1] = 1;
  const stack: [number, number][] = [[0, points.length - 1]];
  while (stack.length) {
    const [a, b] = stack.pop()!;
    const pa = points[a];
    const pb = points[b];
    const len = Math.hypot(pb.x - pa.x, pb.y - pa.y) || 1;
    let worst = -1;
    let worstD = tolerance;
    for (let i = a + 1; i < b; i++) {
      const d = Math.abs((pb.x - pa.x) * (pa.y - points[i].y) - (pa.x - points[i].x) * (pb.y - pa.y)) / len;
      if (d > worstD) {
        worstD = d;
        worst = i;
      }
    }
    if (worst > 0) {
      keep[worst] = 1;
      stack.push([a, worst], [worst, b]);
    }
  }
  return points.filter((_, i) => keep[i]);
}

function shapeFor(kind: ShapeTool, a: Pt, b: Pt, f: number): Annotation {
  const width = style.width * f;
  if (kind === "line" || kind === "arrow") {
    return { kind: "line", from: a, to: b, color: style.stroke, width, arrow: kind === "arrow" };
  }
  const rect = normBox(a, b);
  if (kind === "rect" || kind === "ellipse") return { kind, rect, stroke: style.stroke, fill: style.fill, width };
  return { kind: "rect", shape: kind, rect, stroke: style.stroke, fill: style.fill, width };
}

function startShape(p: AnnotPage, e: PointerEvent, u: Pt, kind: ShapeTool) {
  let current: Annotation | null = null;
  let preview: SVGElement | null = null;
  const f = factorOf(p);
  drag(
    e,
    (ev) => {
      let q = toUnits(p, ev);
      if (ev.shiftKey) {
        if (kind === "line" || kind === "arrow") q = snap45(u, q);
        else {
          const s = Math.max(Math.abs(q.x - u.x), Math.abs(q.y - u.y));
          q = { x: u.x + Math.sign(q.x - u.x || 1) * s, y: u.y + Math.sign(q.y - u.y || 1) * s };
        }
      }
      current = shapeFor(kind, u, q, f);
      preview?.remove();
      preview = drawAnnot(current, scaleOf(p)).main!;
      p.annotSvg.append(preview);
    },
    (_ev, didMove) => {
      preview?.remove();
      setTool("select");
      // Un simple clic pose une forme de taille normale.
      const shape: Annotation =
        didMove && current
          ? current
          : kind === "line" || kind === "arrow"
            ? shapeFor(kind, { x: u.x - 60 * f, y: u.y }, { x: u.x + 60 * f, y: u.y }, f)
            : shapeFor(kind, { x: u.x - 60 * f, y: u.y - 40 * f }, { x: u.x + 60 * f, y: u.y + 40 * f }, f);
      add(p, shape);
    },
  );
}

// ---------- Sélection rectangulaire (pour recadrer une photo) ----------

let cropSel: { page: AnnotPage; rect: Box } | null = null;

/** La zone choisie avec l'outil « Sélection rectangulaire », en unités de page. */
export function cropSelection() {
  return cropSel && cropSel.rect.w >= 2 && cropSel.rect.h >= 2 ? cropSel : null;
}

export function clearCrop() {
  const p = cropSel?.page;
  cropSel = null;
  if (p) render(p);
  host.cropChanged();
}

/** Bouton « Recadrer » : un cadre sur toute la photo, à resserrer avec ses poignées. */
export function startCropFrame(p: AnnotPage) {
  deselect();
  clearTextSelection();
  const old = cropSel?.page;
  const { width, height } = sizeOf(p);
  cropSel = { page: p, rect: { x: 0, y: 0, w: width, h: height } };
  if (old && old !== p) render(old);
  render(p);
  host.cropChanged();
}

/** Déplace un bord, un coin ou tout le cadre de recadrage (sans sortir de la photo). */
function startCropAdjust(p: AnnotPage, e: PointerEvent, name: string) {
  if (!cropSel) return;
  const start = toUnits(p, e);
  const r0 = { ...cropSel.rect };
  const { width, height } = sizeOf(p);
  const min = Math.max(8, Math.min(width, height) * 0.02);
  drag(
    e,
    (ev) => {
      const u = toUnits(p, ev);
      const dx = u.x - start.x;
      const dy = u.y - start.y;
      let { x, y, w, h } = r0;
      if (name === "move") {
        x = Math.max(0, Math.min(width - w, x + dx));
        y = Math.max(0, Math.min(height - h, y + dy));
      } else {
        let x1 = x + w;
        let y1 = y + h;
        if (name.includes("w")) x = Math.max(0, Math.min(x1 - min, x + dx));
        if (name.includes("e")) x1 = Math.min(width, Math.max(x + min, x1 + dx));
        if (name.includes("n")) y = Math.max(0, Math.min(y1 - min, y + dy));
        if (name.includes("s")) y1 = Math.min(height, Math.max(y + min, y1 + dy));
        w = x1 - x;
        h = y1 - y;
      }
      cropSel = { page: p, rect: { x, y, w, h } };
      render(p);
    },
    () => host.cropChanged(),
  );
}

/** Revenir à l'outil de sélection (après un recadrage). */
export function resetTool() {
  setTool("select");
}

function startCrop(p: AnnotPage, e: PointerEvent, u: Pt) {
  const old = cropSel?.page;
  cropSel = null;
  if (old && old !== p) render(old);
  host.cropChanged();
  const { width, height } = sizeOf(p);
  const clamp = (q: Pt) => ({ x: Math.max(0, Math.min(width, q.x)), y: Math.max(0, Math.min(height, q.y)) });
  const a = clamp(u);
  drag(
    e,
    (ev) => {
      const b = clamp(toUnits(p, ev));
      cropSel = { page: p, rect: { x: Math.min(a.x, b.x), y: Math.min(a.y, b.y), w: Math.abs(b.x - a.x), h: Math.abs(b.y - a.y) } };
      render(p);
    },
    () => {
      if (!cropSelection()) cropSel = null;
      render(p);
      host.cropChanged();
    },
  );
}

/** Le reste de la photo assombri, et un cadre en pointillés autour de la zone gardée. */
function cropFrame(p: AnnotPage, r: Box, scale: number): SVGElement {
  const { width, height } = sizeOf(p);
  const g = svg("g", { "pointer-events": "none" });
  g.append(
    svg("path", {
      d: `M0 0H${width}V${height}H0Z M${r.x} ${r.y}v${r.h}h${r.w}v${-r.h}Z`,
      fill: "rgba(0,0,0,0.4)",
      "fill-rule": "evenodd",
    }),
    svg("rect", { x: r.x, y: r.y, width: r.w, height: r.h, fill: "none", stroke: "#ffffff", "stroke-width": 1.5 / scale }),
    svg("rect", {
      x: r.x, y: r.y, width: r.w, height: r.h, fill: "none", stroke: "#000000",
      "stroke-width": 1.5 / scale, "stroke-dasharray": `${5 / scale} ${5 / scale}`,
    }),
  );
  // Des coins en équerre et des barrettes au milieu des bords, épais et
  // blancs, qu'on attrape pour resserrer le cadre ; l'intérieur se déplace d'un bloc.
  const grab = (name: string, el: SVGElement) => {
    el.setAttribute("data-crop", name);
    el.setAttribute("class", `crop-grip crop-${name}`);
    el.setAttribute("pointer-events", "all");
    g.append(el);
  };
  grab("move", svg("rect", { x: r.x, y: r.y, width: r.w, height: r.h, fill: "transparent" }));
  const t = 4 / scale;
  const arm = Math.min(24 / scale, r.w / 3, r.h / 3);
  const bar = Math.min(26 / scale, r.w / 3, r.h / 3);
  const line = { fill: "none", stroke: "#ffffff", "stroke-width": t, "stroke-linecap": "square" };
  // Dessinés juste à l'intérieur du cadre : au bord de la photo, ils restent visibles.
  const x0 = r.x + t / 2;
  const y0 = r.y + t / 2;
  const x1 = r.x + r.w - t / 2;
  const y1 = r.y + r.h - t / 2;
  const cx = r.x + r.w / 2;
  const cy = r.y + r.h / 2;
  const pad = 14 / scale; // zone d'accroche plus large que le dessin
  const zone = (name: string, x: number, y: number, w: number, h: number) =>
    grab(name, svg("rect", { x: x - w / 2, y: y - h / 2, width: w, height: h, fill: "transparent" }));
  for (const [name, x, y] of [["n", cx, r.y], ["s", cx, r.y + r.h], ["w", r.x, cy], ["e", r.x + r.w, cy]] as const) {
    const vertical = name === "w" || name === "e";
    zone(name, x, y, vertical ? 2 * pad : Math.max(r.w - 2 * arm, bar), vertical ? Math.max(r.h - 2 * arm, bar) : 2 * pad);
    const d = vertical
      ? `M${name === "w" ? x0 : x1} ${cy - bar / 2}V${cy + bar / 2}`
      : `M${cx - bar / 2} ${name === "n" ? y0 : y1}H${cx + bar / 2}`;
    g.append(svg("path", { d, ...line, "pointer-events": "none" }));
  }
  for (const [name, x, y, sx, sy] of [["nw", x0, y0, 1, 1], ["ne", x1, y0, -1, 1], ["sw", x0, y1, 1, -1], ["se", x1, y1, -1, -1]] as const) {
    zone(name, x, y, 2 * pad + arm, 2 * pad + arm);
    g.append(svg("path", { d: `M${x} ${y + sy * arm}V${y}H${x + sx * arm}`, ...line, "pointer-events": "none" }));
  }
  return g;
}

// ---------- Zone de texte : modification sur place ----------

function editText(p: AnnotPage, i: number | null, rect: Box) {
  const a = i === null ? null : p.annots[i];
  const t = a?.kind === "text" ? a : null;
  const ts: TextStyle = t ?? { ...textDefaults(), size: style.size * factorOf(p) };
  const size = ts.size;
  const fo = svg("foreignObject", { x: rect.x, y: rect.y, width: rect.w, height: rect.h });
  const area = document.createElement("textarea");
  area.className = "annot-edit";
  area.spellcheck = true;
  area.value = t?.text ?? "";
  Object.assign(area.style, {
    font: cssFont(ts),
    textAlign: ts.align ?? "left",
    textDecoration: ts.underline ? "underline" : "none",
    lineHeight: String(LINE_HEIGHT),
    padding: `${TEXT_PAD}px`,
    color: t?.color ?? style.textColor,
    background: t?.fill ?? "transparent",
    width: `${rect.w}px`,
    height: `${rect.h}px`,
  });
  fo.append(area);
  selected = null;
  editing = { page: p, i, fo, area, rect };
  render(p);
  const grow = () => {
    const lines = wrap(area.value, ts, rect.w);
    const h = textHeight(lines.length, size);
    fo.setAttribute("height", String(h));
    area.style.height = `${h}px`;
  };
  grow();
  area.addEventListener("input", grow);
  area.addEventListener("keydown", (ev) => {
    if (ev.key === "Escape") {
      ev.stopPropagation();
      finishEditing();
    }
  });
  area.addEventListener("blur", () => setTimeout(() => editing?.area === area && finishEditing(), 0));
  requestAnimationFrame(() => {
    area.focus();
    area.select();
  });
}

function finishEditing() {
  const e = editing;
  if (!e) return;
  editing = null;
  const { page: p, i, area, rect } = e;
  const text = area.value.replace(/\s+$/, "");
  const old = i === null ? null : p.annots[i];
  if (old?.kind === "text") {
    if (!text) {
      host.change(() => (p.annots = p.annots.filter((_, k) => k !== i)));
      host.changed();
    } else if (text !== old.text) {
      host.change(() => replace(p, i!, withText(old, { text })));
      host.changed();
    }
    render(p);
    if (text) selectAnnot(p, i!);
  } else if (text) {
    add(p, withText(
      { kind: "text", rect, text: "", lines: [], ...textDefaults(), size: style.size * factorOf(p), color: style.textColor, fill: null },
      { text },
    ));
  } else {
    render(p);
  }
}

// ---------- Notes ----------

let noteBox: HTMLElement | null = null;

function closeNote() {
  noteBox?.remove();
  noteBox = null;
}

function openNote(p: AnnotPage, i: number) {
  closeNote();
  const a = p.annots[i];
  if (a?.kind !== "note") return;
  const box = document.createElement("div");
  box.className = "note-box";
  const area = document.createElement("textarea");
  area.placeholder = tr("notePlaceholder");
  area.value = a.text;
  box.append(area);
  document.body.append(box);
  const at = toScreen(p, { x: a.at.x + NOTE_SIZE, y: a.at.y });
  const r = box.getBoundingClientRect();
  box.style.left = `${Math.min(at.x + 8, innerWidth - r.width - 8)}px`;
  box.style.top = `${Math.min(Math.max(8, at.y), innerHeight - r.height - 8)}px`;
  noteBox = box;
  const commit = () => {
    if (noteBox !== box) return;
    closeNote();
    const now = p.annots[i];
    if (now?.kind === "note" && now.text !== area.value) {
      host.change(() => replace(p, i, { ...now, text: area.value }));
      host.changed();
    }
  };
  area.addEventListener("blur", commit);
  area.addEventListener("keydown", (ev) => {
    if (ev.key === "Escape") {
      ev.stopPropagation();
      area.blur();
    }
  });
  requestAnimationFrame(() => area.focus());
}

// ---------- Barre d'annotation ----------

const $ = (id: string) => document.getElementById(id)!;

const TOOL_BUTTONS: [string, Tool][] = [
  ["tool-select", "select"],
  ["tool-ink", "ink"],
  ["tool-text", "text"],
  ["tool-note", "note"],
  ["tool-sign", "signature"],
  ["tool-crop", "crop"],
  ["highlight", "highlight"],
];

function setTool(t: Tool) {
  if (t !== "crop" && cropSel) clearCrop();
  tool = t;
  document.body.dataset.tool = t;
  for (const [id, tt] of TOOL_BUTTONS) $(id).classList.toggle("active", tt === t);
  $("tool-shapes").classList.toggle("active", (SHAPE_TOOLS as string[]).includes(t));
}

function current(): Annotation | null {
  return selected ? (selected.page.annots[selected.i] ?? null) : null;
}

function strokeOf(a: Annotation | null): Color | null {
  if (!a) return style.stroke;
  switch (a.kind) {
    case "rect":
    case "ellipse":
      return a.stroke;
    case "text":
      return a.color;
    case "highlight":
      return a.color;
    case "image":
      return null;
    default:
      return a.color;
  }
}

function fillOf(a: Annotation | null): Color | null {
  if (!a) return style.fill;
  return a.kind === "rect" || a.kind === "ellipse" || a.kind === "text" ? a.fill : null;
}

function updateSwatches() {
  const a = current();
  const stroke = strokeOf(a);
  const fill = fillOf(a);
  const s = $("swatch-stroke");
  s.style.borderColor = stroke ?? "transparent";
  s.classList.toggle("none", !stroke);
  const f = $("swatch-fill");
  f.style.background = fill ?? "transparent";
  f.classList.toggle("none", !fill);
  $("swatch-highlight").style.background = style.highlight;
}

/** Une couleur HSL (teinte en degrés, saturation et luminosité de 0 à 1) en « #rrggbb ». */
function hsl(h: number, sat: number, l: number): Color {
  const k = (n: number) => (n + h / 30) % 12;
  const a = sat * Math.min(l, 1 - l);
  const f = (n: number) => l - a * Math.max(-1, Math.min(k(n) - 3, Math.min(9 - k(n), 1)));
  return "#" + [f(0), f(8), f(4)].map((v) => Math.round(v * 255).toString(16).padStart(2, "0")).join("");
}

/** Teintes des colonnes du nuancier, et luminosités de ses lignes (du foncé au clair). */
const HUES = [200, 215, 240, 265, 290, 330, 0, 20, 38, 52, 70, 100];
const LIGHTS = [0.16, 0.24, 0.32, 0.4, 0.48, 0.58, 0.68, 0.78, 0.88];
const GRAYS = Array.from({ length: 11 }, (_, i) => hsl(0, 0, 1 - i / 10));

/** Couleurs choisies avec « Autres couleurs… », gardées pour la prochaine fois. */
let recent: Color[] = [];
const colorInput = document.createElement("input");
colorInput.type = "color";
colorInput.className = "color-input";
document.body.append(colorInput);

/**
 * Le nuancier : les couleurs vives en haut, puis « aucune », les gris et les nuances,
 * et un bouton pour choisir n'importe quelle autre couleur.
 */
function palette(colors: [string, Color][], currentColor: Color | null, none: string | null, pick: (c: Color | null) => void) {
  const box = document.createElement("div");
  box.className = "palette-box";
  const swatch = (c: Color | null, name: string, cls = "palette-color") => {
    const b = document.createElement("button");
    b.className = cls + (c === currentColor ? " current" : "");
    b.title = name;
    if (c) b.style.background = c;
    b.addEventListener("click", () => {
      closeMenu();
      pick(c);
    });
    return b;
  };
  const row = (cls: string) => {
    const r = document.createElement("div");
    r.className = cls;
    box.append(r);
    return r;
  };
  // Surlignage : quelques couleurs seulement, en pastilles rondes espacées.
  const few = colors === HIGHLIGHTS;
  if (few) box.classList.add("palette-few");
  const main = row(few ? "palette-dots" : "palette");
  for (const [name, c] of colors) main.append(swatch(c, name, few ? "palette-dot" : "palette-color"));
  if (!few) {
    const grid = row("palette-grid");
    grid.append(none ? swatch(null, none, "palette-color palette-none") : document.createElement("span"));
    GRAYS.forEach((c, i) => grid.append(swatch(c, tr(i === 0 ? "colorWhite" : i === 10 ? "colorBlack" : "colorGray"))));
    for (const l of LIGHTS) for (const h of HUES) grid.append(swatch(hsl(h, 0.75, l), tr("colorShade")));
    if (recent.length) {
      const r = row("palette");
      for (const c of recent) r.append(swatch(c, tr("colorRecent")));
    }
    const more = document.createElement("button");
    more.className = "palette-more";
    more.textContent = tr("moreColors");
    // Le sélecteur de Windows s'ouvre par-dessus : le champ reste dans la page même si le
    // panneau se ferme pendant ce temps.
    more.addEventListener("click", () => {
      colorInput.value = currentColor ?? "#ff3b30";
      colorInput.onchange = () => {
        const c = colorInput.value;
        recent = [c, ...recent.filter((x) => x !== c)].slice(0, 12);
        closeMenu();
        pick(c);
      };
      colorInput.click();
    });
    box.append(more);
  }
  return box;
}

/** Applique un réglage de texte à la zone choisie et aux prochaines zones. */
function setText(changes: Partial<TextStyle>) {
  const { size, ...rest } = changes;
  Object.assign(style, rest);
  if (size !== undefined) style.size = size;
  restyle((a, f) => (a.kind === "text" ? withText(a, { ...rest, ...(size !== undefined ? { size: size * f } : {}) }) : null));
}

/** Panneau « Aa » : police, couleur, taille, gras / italique / souligné, alignement. */
function textPanel(): HTMLElement {
  const a = current();
  const t = a?.kind === "text" ? a : null;
  const now: TextStyle = t ? { ...t, size: selected ? t.size / factorOf(selected.page) : t.size } : textDefaults();
  const color = t?.color ?? style.textColor;
  const panel = document.createElement("div");
  panel.className = "text-panel";

  const font = document.createElement("select");
  font.className = "text-font";
  for (const f of Object.keys(FONT_NAMES) as Font[]) {
    const o = new Option(FONT_NAMES[f], f, false, f === now.font);
    o.style.fontFamily = FONT_CSS[f];
    font.append(o);
  }
  font.addEventListener("change", () => setText({ font: font.value as Font }));
  const swatch = document.createElement("button");
  swatch.className = "text-color";
  swatch.title = tr("textColor");
  swatch.style.background = color;
  swatch.addEventListener("click", () => {
    const r = swatch.getBoundingClientRect();
    showPanel(r.left, r.bottom + 6, palette(PALETTE, color, null, (c) => {
      if (!c) return;
      style.textColor = c;
      restyle((a) => (a.kind === "text" ? { ...a, color: c } : null));
      updateSwatches();
    }));
  });

  const size = document.createElement("input");
  size.type = "number";
  size.className = "text-size";
  size.min = "4";
  size.max = "200";
  size.value = String(Math.round(now.size * 10) / 10);
  size.title = tr("textSize");
  size.addEventListener("change", () => {
    const v = Number(size.value);
    if (v >= 4 && v <= 200) setText({ size: v });
  });
  const unit = document.createElement("span");
  unit.className = "text-unit";
  unit.textContent = "pt";

  const toggles = document.createElement("div");
  toggles.className = "segmented";
  for (const key of ["bold", "italic", "underline"] as const) {
    const [label, title] = [tr(`${key}Letter`), tr(key)];
    const b = document.createElement("button");
    b.className = `seg seg-${key}` + (now[key] ? " on" : "");
    b.textContent = label;
    b.title = title;
    b.addEventListener("click", () => {
      const on = !b.classList.contains("on");
      b.classList.toggle("on", on);
      setText({ [key]: on });
    });
    toggles.append(b);
  }

  const aligns = document.createElement("div");
  aligns.className = "segmented wide";
  for (const [value, icon, title] of [
    ["left", "M4 6h16M4 10h10M4 14h16M4 18h10", "alignLeft"],
    ["center", "M4 6h16M7 10h10M4 14h16M7 18h10", "alignCenter"],
    ["right", "M4 6h16M10 10h10M4 14h16M10 18h10", "alignRight"],
  ] as const) {
    const b = document.createElement("button");
    b.className = "seg" + ((now.align ?? "left") === value ? " on" : "");
    b.title = tr(title);
    const i = svg("svg", { viewBox: "0 0 24 24", width: 16, height: 16, fill: "none", stroke: "currentColor", "stroke-width": 1.8, "stroke-linecap": "round" });
    i.append(svg("path", { d: icon }));
    b.append(i);
    b.addEventListener("click", () => {
      aligns.querySelectorAll(".seg").forEach((x) => x.classList.toggle("on", x === b));
      setText({ align: value });
    });
    aligns.append(b);
  }

  const row = (...els: HTMLElement[]) => {
    const r = document.createElement("div");
    r.className = "text-row";
    r.append(...els);
    return r;
  };
  panel.append(row(font, swatch), row(size, unit, toggles), row(aligns));
  return panel;
}

const SHAPE_NAMES: Record<ShapeTool, string> = {
  line: tr("annotLine"), arrow: tr("annotArrow"), rect: tr("annotRect"), rounded: tr("annotRounded"),
  ellipse: tr("annotEllipse"), bubble: tr("annotBubble"), star: tr("annotStar"), hexagon: tr("annotHexagon"),
};

/** Petit dessin d'une forme pour le menu (tracé ici, au trait comme les autres icônes). */
function shapeIcon(t: ShapeTool): SVGSVGElement {
  const icon = svg("svg", { viewBox: "0 0 24 24", width: 22, height: 22, fill: "none", stroke: "currentColor", "stroke-width": 1.6, "stroke-linejoin": "round", "stroke-linecap": "round" });
  const box = { x: 3, y: 5, w: 18, h: 14 };
  if (t === "line") icon.append(svg("line", { x1: 4, y1: 20, x2: 20, y2: 4 }));
  else if (t === "arrow") icon.append(svg("path", { d: "M4 20 L19 5 M11 5 H19 V13" }));
  else if (t === "rect") icon.append(svg("rect", { x: box.x, y: box.y, width: box.w, height: box.h }));
  else if (t === "ellipse") icon.append(svg("ellipse", { cx: 12, cy: 12, rx: 9, ry: 7 }));
  else {
    const r = t === "star" || t === "hexagon" ? { x: 3, y: 3, w: 18, h: 18 } : box;
    icon.append(svg("polygon", { points: outline(t, r).map((q) => `${q.x},${q.y}`).join(" ") }));
  }
  return icon;
}

function shapeGrid(): HTMLElement {
  const grid = document.createElement("div");
  grid.className = "shape-grid";
  for (const t of SHAPE_TOOLS) {
    const b = document.createElement("button");
    b.className = "shape-pick" + (tool === t ? " current" : "");
    b.title = SHAPE_NAMES[t];
    b.append(shapeIcon(t));
    b.addEventListener("click", () => {
      closeMenu();
      setTool(t);
    });
    grid.append(b);
  }
  return grid;
}

const below = (id: string) => {
  const r = $(id).getBoundingClientRect();
  return [r.left, r.bottom + 6] as const;
};

function initToolbar() {
  $("tool-select").addEventListener("click", () => setTool("select"));
  $("tool-crop").addEventListener("click", () => setTool(tool === "crop" ? "select" : "crop"));
  $("tool-ink").addEventListener("click", () => setTool(tool === "ink" ? "select" : "ink"));
  $("tool-text").addEventListener("click", () => setTool(tool === "text" ? "select" : "text"));
  $("tool-note").addEventListener("click", () => setTool(tool === "note" ? "select" : "note"));
  $("highlight").addEventListener("click", () => {
    if (highlightSelection()) return setTool("highlight");
    setTool(tool === "highlight" ? "select" : "highlight");
  });
  $("highlight-menu").addEventListener("click", () => {
    const [x, y] = below("highlight-menu");
    showPanel(x, y, palette(HIGHLIGHTS, style.highlight, null, (c) => {
      style.highlight = c!;
      restyle((a) => (a.kind === "highlight" ? { ...a, color: c! } : null));
      updateSwatches();
      if (!highlightSelection()) setTool("highlight");
      else setTool("highlight");
    }));
  });
  $("tool-shapes").addEventListener("click", () => {
    const [x, y] = below("tool-shapes");
    showPanel(x, y, shapeGrid());
  });
  $("style-width").addEventListener("click", () => {
    const [x, y] = below("style-width");
    const a = current();
    const w = a && "width" in a && selected ? a.width / factorOf(selected.page) : style.width;
    showMenu(
      x,
      y,
      WIDTHS.map(([label, v]) => ({
        label,
        checked: Math.abs(v - w) < 0.01,
        action: () => {
          style.width = v;
          restyle((a, f) => ("width" in a ? ({ ...a, width: v * f } as Annotation) : null));
        },
      })),
    );
  });
  $("style-stroke").addEventListener("click", () => {
    const [x, y] = below("style-stroke");
    const a = current();
    const shape = !a || a.kind === "rect" || a.kind === "ellipse";
    showPanel(x, y, palette(PALETTE, strokeOf(a), shape ? tr("strokeNone") : null, (c) => {
      const sel = current();
      if (!sel) {
        if (c) style.stroke = c;
      } else if (sel.kind === "text") {
        if (c) style.textColor = c;
      } else if (c) {
        style.stroke = c;
      }
      restyle((a) => {
        switch (a.kind) {
          case "rect":
          case "ellipse":
            return { ...a, stroke: c };
          case "highlight":
          case "image":
            return null;
          default:
            return c ? ({ ...a, color: c } as Annotation) : null;
        }
      });
      updateSwatches();
    }));
  });
  $("style-fill").addEventListener("click", () => {
    const [x, y] = below("style-fill");
    showPanel(x, y, palette(PALETTE, fillOf(current()), tr("fillNone"), (c) => {
      if (!current()) style.fill = c;
      restyle((a) => (a.kind === "rect" || a.kind === "ellipse" ? { ...a, fill: c } : a.kind === "text" ? withText(a, { fill: c }) : null));
      updateSwatches();
    }));
  });
  $("style-font").addEventListener("click", () => {
    const [x, y] = below("style-font");
    showPanel(x, y, textPanel());
  });
  setTool("select");
  updateSwatches();
}

// ---------- Clavier ----------

/** Raccourcis propres aux annotations. Renvoie true si la touche a été utilisée. */
export function handleKey(e: KeyboardEvent): boolean {
  const ctrl = e.ctrlKey || e.metaKey;
  if (ctrl && e.key.toLowerCase() === "c" && textSel) {
    navigator.clipboard?.writeText(selectedText()).catch(() => {});
    e.preventDefault();
    return true;
  }
  if (ctrl) return false;
  if ((e.key === "Delete" || e.key === "Backspace") && selected) {
    e.preventDefault();
    deleteSelected();
    return true;
  }
  if (e.key === "Escape") {
    if (cropSel) {
      clearCrop();
      return true;
    }
    if (selected) {
      deselect();
      return true;
    }
    if (textSel) {
      clearTextSelection();
      return true;
    }
    if (tool !== "select") {
      setTool("select");
      return true;
    }
  }
  return false;
}

/** À appeler quand les pages changent (annuler, supprimer…) : on oublie ce qui n'existe plus. */
export function forget() {
  const pages = host.pages();
  if (selected && (!pages.includes(selected.page) || !selected.page.annots[selected.i])) selected = null;
  if (textSel && !pages.includes(textSel.page)) textSel = null;
  if (editing && !pages.includes(editing.page)) editing = null;
  closeNote();
  updateSwatches();
}

export function hasSelection(): boolean {
  return selected !== null;
}

// ---------- Photos : l'image finale, annotations dessinées dedans ----------

function standalone(p: AnnotPage, layer: "hl" | "main"): string {
  const { width, height } = sizeOf(p);
  const root = svg("svg", { xmlns: SVG, width, height, viewBox: `0 0 ${width} ${height}` });
  for (const a of p.annots) {
    const g = drawAnnot(a, 1)[layer];
    if (g) root.append(g);
  }
  for (const h of root.querySelectorAll(".hit")) h.remove();
  return new XMLSerializer().serializeToString(root);
}

function svgImage(source: string): Promise<HTMLImageElement> {
  const img = new Image();
  img.src = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(source)}`;
  return img.decode().then(() => img);
}

/** L'image d'une photo, tournée et avec ses annotations, en PNG. */
export async function flatten(p: AnnotPage): Promise<Uint8Array> {
  const { width, height } = sizeOf(p);
  const W = Math.round(width);
  const H = Math.round(height);
  const photo = new Image();
  photo.crossOrigin = "anonymous";
  photo.src = api.pageUrl(p.doc.id, p.index, W, 0);
  await photo.decode();
  const side = p.rotation % 180 !== 0;
  const canvas = document.createElement("canvas");
  canvas.width = side ? H : W;
  canvas.height = side ? W : H;
  const ctx = canvas.getContext("2d")!;
  ctx.translate(canvas.width / 2, canvas.height / 2);
  ctx.rotate((p.rotation * Math.PI) / 180);
  ctx.translate(-W / 2, -H / 2);
  ctx.drawImage(photo, 0, 0, W, H);
  ctx.globalCompositeOperation = "multiply";
  ctx.drawImage(await svgImage(standalone(p, "hl")), 0, 0, W, H);
  ctx.globalCompositeOperation = "source-over";
  ctx.drawImage(await svgImage(standalone(p, "main")), 0, 0, W, H);
  const blob = await new Promise<Blob>((resolve, reject) =>
    canvas.toBlob((b) => (b ? resolve(b) : reject(new Error("image too large"))), "image/png"),
  );
  return new Uint8Array(await blob.arrayBuffer());
}

export function init(h: Host) {
  host = h;
  initToolbar();
  window.addEventListener("blur", () => noteBox?.querySelector("textarea")?.blur());
}
