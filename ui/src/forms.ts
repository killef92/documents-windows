// Formulaires PDF (CERFA…) : les champs sont affichés par-dessus la page, à remplir directement.
//
// La page est alors rendue sans ses champs (l'image ne montre que le fond du formulaire) ;
// à l'enregistrement, le cœur écrit les valeurs et redessine chaque champ
// (crates/core/src/form.rs) avec la même mise en page qu'ici.

import * as api from "./api";
import type { DocInfo, FormField } from "./api";

// Les mêmes valeurs que dans le cœur (form.rs).
const FIELD_PAD = 2;
const FIELD_LINE_HEIGHT = 1.15;

export interface FormPage {
  doc: DocInfo;
  index: number;
  rotation: number;
  el: HTMLElement;
}

type Value = string | boolean;

interface DocForm {
  fields: FormField[];
  /** Valeur actuelle de chaque champ (par identifiant). */
  values: Map<string, Value>;
  /** Valeurs au dernier enregistrement, pour savoir si quelque chose a changé. */
  saved: Map<string, Value>;
}

const forms = new Map<number, DocForm>();
const layers = new WeakMap<HTMLElement, HTMLElement>();
let onChange: () => void = () => {};

const initial = (f: FormField): Value => (f.kind === "checkbox" ? f.value !== "" && f.value !== "Off" : f.value);

/** Charge les champs d'un PDF qui vient d'être ouvert. */
export async function load(doc: DocInfo): Promise<boolean> {
  if (!doc.paged) return false;
  const fields = await api.formFields(doc.id).catch(() => [] as FormField[]);
  if (!fields.length) return false;
  const values = new Map<string, Value>();
  for (const f of fields) if (!values.has(f.id) || f.kind !== "radio") values.set(f.id, initial(f));
  forms.set(doc.id, { fields, values, saved: new Map(values) });
  return true;
}

export const hasForm = (doc: DocInfo) => forms.has(doc.id);

export function forget(docId: number) {
  forms.delete(docId);
}

export function isDirty(): boolean {
  for (const f of forms.values()) for (const [k, v] of f.values) if (f.saved.get(k) !== v) return true;
  return false;
}

export function markSaved() {
  for (const f of forms.values()) f.saved = new Map(f.values);
}

// ---------- Mise en page du texte (la même que celle écrite dans le PDF) ----------

const measure = document.createElement("canvas").getContext("2d")!;
const fontCss = (f: FormField) => (f.courier ? "'Courier New', Courier, 'Liberation Mono', monospace" : "Arial, Helvetica, 'Liberation Sans', sans-serif");

function widthOf(text: string, f: FormField, size: number) {
  measure.font = `${size}px ${fontCss(f)}`;
  return measure.measureText(text).width;
}

/** Taille du texte en points : celle du formulaire, ou « automatique » (comme Acrobat). */
function fontSize(f: FormField, text: string): number {
  if (f.font_size > 0) return f.font_size;
  const h = f.rect.h / f.units_per_point;
  if (f.multiline) return 10;
  let size = Math.max(4, Math.min(12, (h - 2 * FIELD_PAD) / FIELD_LINE_HEIGHT));
  const room = f.rect.w / f.units_per_point - 2 * FIELD_PAD;
  while (size > 4 && widthOf(text, f, size) > room) size -= 0.5;
  return size;
}

function wrap(text: string, f: FormField, size: number): string[] {
  const room = f.rect.w / f.units_per_point - 2 * FIELD_PAD;
  const out: string[] = [];
  for (const para of text.split("\n")) {
    let line = "";
    for (const word of para.split(/(\s+)/)) {
      if (!word) continue;
      if (widthOf(line + word, f, size) <= room || !line.trim()) line += word;
      else {
        out.push(line.trimEnd());
        line = word.trimStart();
      }
    }
    out.push(line.trimEnd());
  }
  return out;
}

/** Ce qu'on envoie au cœur pour remplir le PDF. */
export function fills(): api.DocFill[] {
  const out: api.DocFill[] = [];
  for (const [doc, form] of forms) {
    const fields: api.FieldFill[] = [];
    const done = new Set<string>();
    for (const f of form.fields) {
      if (done.has(f.id) || f.read_only) continue;
      done.add(f.id);
      const v = form.values.get(f.id);
      // Le cœur repart toujours du fichier d'origine : on envoie tout ce qui en diffère.
      if (v === undefined || v === initial(f)) continue;
      if (f.kind === "checkbox") fields.push({ id: f.id, kind: "check", on: v === true });
      else if (f.kind === "radio") fields.push({ id: f.id, kind: "radio", value: v ? String(v) : null });
      else {
        const text = String(v);
        const size = fontSize(f, text);
        const lines = (f.multiline ? wrap(text, f, size) : [text.replace(/\n/g, " ")]).map((t) => ({
          text: t,
          width: widthOf(t, f, size),
          chars: f.comb ? [...t].map((c) => widthOf(c, f, size)) : [],
        }));
        fields.push({ id: f.id, kind: "text", text, lines, size });
      }
    }
    if (fields.length) out.push({ doc, fields });
  }
  return out;
}

// ---------- Affichage sur la page ----------

function setValue(doc: number, id: string, v: Value) {
  const form = forms.get(doc);
  if (!form) return;
  form.values.set(id, v);
  // Les autres emplacements du même champ (un même nom sur deux pages, ou un groupe radio).
  for (const el of document.querySelectorAll<HTMLElement>(`[data-doc="${doc}"][data-field="${CSS.escape(id)}"]`)) {
    paint(el, form, id);
  }
  onChange();
}

function paint(el: HTMLElement, form: DocForm, id: string) {
  const v = form.values.get(id);
  if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement || el instanceof HTMLSelectElement) {
    if (el.value !== String(v ?? "") && document.activeElement !== el) el.value = String(v ?? "");
  } else if (el.dataset.on !== undefined) {
    el.classList.toggle("checked", el.dataset.kind === "checkbox" ? v === true : v === el.dataset.on);
    el.setAttribute("aria-checked", String(el.classList.contains("checked")));
  }
}

function makeField(doc: DocInfo, f: FormField, form: DocForm): HTMLElement {
  const v = form.values.get(f.id);
  let el: HTMLElement;
  if (f.kind === "checkbox" || f.kind === "radio") {
    el = document.createElement("button");
    el.className = `field field-${f.kind}`;
    el.dataset.on = f.on;
    el.dataset.kind = f.kind;
    el.setAttribute("role", f.kind);
    if (f.kind === "checkbox") {
      el.innerHTML = `<svg viewBox="0 0 20 20"><path d="M4 10.5l4 4 8-9" fill="none" stroke="#1d1d1f" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"/></svg>`;
    }
    el.addEventListener("click", () => {
      if (f.read_only) return;
      const now = form.values.get(f.id);
      if (f.kind === "checkbox") setValue(doc.id, f.id, now !== true);
      else setValue(doc.id, f.id, now === f.on ? "" : f.on);
    });
    el.addEventListener("keydown", (e) => {
      if (e.key === " " || e.key === "Enter") {
        e.preventDefault();
        el.click();
      }
    });
  } else if (f.kind === "choice") {
    const select = document.createElement("select");
    select.className = "field field-choice";
    const options = f.options.includes(String(v)) || !v ? f.options : [String(v), ...f.options];
    for (const o of ["", ...options]) {
      const opt = document.createElement("option");
      opt.value = o;
      opt.textContent = o;
      select.append(opt);
    }
    select.value = String(v ?? "");
    select.addEventListener("change", () => setValue(doc.id, f.id, select.value));
    el = select;
  } else {
    const input = f.multiline ? document.createElement("textarea") : document.createElement("input");
    input.className = `field field-text${f.comb ? " comb" : ""}`;
    input.value = String(v ?? "");
    input.spellcheck = false;
    if (f.max_len) input.maxLength = f.max_len;
    input.addEventListener("input", () => {
      setValue(doc.id, f.id, input.value);
      fitText(input, f);
    });
    el = input;
  }
  if (f.read_only) {
    el.classList.add("read-only");
    if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) el.readOnly = true;
    if (el instanceof HTMLSelectElement || el instanceof HTMLButtonElement) el.disabled = true;
  }
  el.dataset.doc = String(doc.id);
  el.dataset.field = f.id;
  el.title = f.name;
  Object.assign(el.style, {
    left: `${f.rect.x}px`,
    top: `${f.rect.y}px`,
    width: `${f.rect.w}px`,
    height: `${f.rect.h}px`,
    color: f.color,
    fontFamily: fontCss(f),
  });
  paint(el, form, f.id);
  if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement || el instanceof HTMLSelectElement) fitText(el, f);
  return el;
}

/** Taille et espacement du texte d'un champ, en unités de page (le calque est mis à l'échelle). */
function fitText(el: HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement, f: FormField) {
  const size = fontSize(f, el.value);
  const upp = f.units_per_point;
  el.style.fontSize = `${size * upp}px`;
  el.style.padding = `0 ${FIELD_PAD * upp}px`;
  if (el instanceof HTMLTextAreaElement) {
    el.style.lineHeight = String(FIELD_LINE_HEIGHT);
    el.style.padding = `${FIELD_PAD * upp}px`;
  }
  if (f.comb && f.max_len) {
    // Un caractère par case : on espace les lettres de la largeur d'une case.
    const cell = f.rect.w / f.max_len;
    const ch = widthOf("0", f, size) * upp;
    el.style.letterSpacing = `${cell - ch}px`;
    el.style.padding = `0 0 0 ${(cell - ch) / 2}px`;
  }
  if (f.align === 1) el.style.textAlign = "center";
  if (f.align === 2) el.style.textAlign = "right";
}

/** Crée (une seule fois) le calque des champs d'une page. */
export function attach(p: FormPage) {
  const form = forms.get(p.doc.id);
  if (!form || layers.has(p.el)) return;
  const fields = form.fields.filter((f) => f.page === p.index);
  if (!fields.length) return;
  const layer = document.createElement("div");
  layer.className = "form-layer";
  const { width, height } = p.doc.pages[p.index];
  layer.style.width = `${width}px`;
  layer.style.height = `${height}px`;
  layer.append(...fields.map((f) => makeField(p.doc, f, form)));
  p.el.append(layer);
  layers.set(p.el, layer);
}

/** Met le calque à l'échelle et dans le sens de la page affichée. */
export function place(p: FormPage, pageW: number) {
  const layer = layers.get(p.el);
  if (!layer) return;
  const { width, height } = p.doc.pages[p.index];
  const side = p.rotation % 180 !== 0;
  const scale = pageW / (side ? height : width);
  layer.style.transform = `translate(-50%, -50%) rotate(${p.rotation}deg) scale(${scale})`;
}

export function init(changed: () => void) {
  onChange = changed;
}
