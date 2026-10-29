import { getCurrentWindow } from "@tauri-apps/api/window";
import { getVersion } from "@tauri-apps/api/app";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { listen } from "@tauri-apps/api/event";
import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";

import * as api from "./api";
import type { DocInfo, NormRect, SearchHit } from "./api";
import { drawIcons } from "./icons";
import * as annot from "./annotations";
import * as forms from "./forms";
import * as signatures from "./signatures";
import * as retouch from "./retouch";
import * as film from "./filmstrip";
import * as pan from "./pan";
import type { Annotation } from "./annotations";
import { ask, closeMenu, showMenu, type MenuItem } from "./menu";
import { checkForUpdate } from "./update";

drawIcons();

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;

const viewer = $("viewer");
const pagesEl = $("pages");
const thumbsEl = $("thumbs");
const resultsEl = $("results");
const searchInput = $<HTMLInputElement>("search");
const sidebar = $("sidebar");

/** Marge autour des pages et espace entre deux pages, en pixels. */
const PAD = 24;
const GAP = 16;
const THUMB_W = 128;
const ZOOM_STEP = 1.25;
const MIN_SCALE = 0.1;
const MAX_SCALE = 8;
const FILE_TYPES = ["pdf", "jpg", "jpeg", "png", "webp", "gif", "heic"];

interface Page {
  doc: DocInfo;
  index: number;
  /** Rotation choisie par l'utilisateur. Le fichier n'est modifié qu'à l'enregistrement. */
  rotation: number;
  el: HTMLElement;
  hlLayer: HTMLElement;
  /** Annotations ajoutées (jamais modifiées sur place : on remplace le tableau, pour Ctrl+Z). */
  annots: Annotation[];
  hlSvg: SVGSVGElement;
  annotSvg: SVGSVGElement;
  thumb: HTMLElement;
  thumbBox: HTMLElement;
  label: HTMLElement;
  visible: boolean;
  thumbVisible: boolean;
  /** Largeur (px) de l'image affichée / demandée, pour ne pas redemander la même. */
  shownWidth: number;
  wantedWidth: number;
  thumbKey: string;
}

type Zoom = { mode: "width" | "fit" } | { mode: "scale"; scale: number };

/** Ce qu'on garde pour pouvoir annuler : l'ordre des pages, leurs rotations et leurs annotations. */
type Snapshot = { page: Page; rotation: number; annots: Annotation[] }[];

const state = {
  docs: [] as DocInfo[],
  pages: [] as Page[],
  current: 0,
  selection: new Set<Page>(),
  anchor: 0,
  zoom: { mode: "width" } as Zoom,
  hits: [] as SearchHit[],
  activeHit: -1,
  undo: [] as Snapshot[],
  redo: [] as Snapshot[],
  /** L'état au dernier enregistrement (ou à l'ouverture), pour savoir s'il y a des modifications. */
  saved: [] as Snapshot,
  /** Fichier où « Enregistrer » écrit (choisi par « Enregistrer sous »). */
  saveTarget: null as string | null,
};

// ---------- Tailles et zoom ----------

const sideways = (p: Page) => p.rotation % 180 !== 0;

function baseSize(p: Page) {
  const s = p.doc.pages[p.index];
  return sideways(p) ? { w: s.height, h: s.width } : { w: s.width, h: s.height };
}

function availableSize() {
  return {
    w: Math.max(100, viewer.clientWidth - 2 * PAD),
    h: Math.max(100, viewer.clientHeight - 2 * PAD),
  };
}

function scaleOf(p: Page): number {
  const b = baseSize(p);
  const a = availableSize();
  switch (state.zoom.mode) {
    case "width":
      return a.w / b.w;
    case "fit":
      return Math.min(a.w / b.w, a.h / b.h);
    case "scale":
      return state.zoom.scale;
  }
}

function defaultZoom(): Zoom {
  return { mode: state.docs.some((d) => d.kind === "pdf") ? "width" : "fit" };
}

/** Garde la même partie de la page courante à l'écran pendant un changement de taille. */
function keepingPosition(change: () => void) {
  const p = state.pages[state.current];
  let ratio = 0;
  let offset = 0;
  if (p) {
    offset = viewer.scrollTop - p.el.offsetTop;
    ratio = offset / Math.max(1, p.el.offsetHeight);
  }
  change();
  const q = state.pages[state.current];
  if (p && q === p) viewer.scrollTop = p.el.offsetTop + (offset < 0 ? offset : ratio * p.el.offsetHeight);
}

function layout() {
  for (const p of state.pages) {
    const b = baseSize(p);
    const s = scaleOf(p);
    const w = Math.round(b.w * s);
    const h = Math.round(b.h * s);
    p.el.style.width = `${w}px`;
    p.el.style.height = `${h}px`;
    const t = thumbSize(p);
    p.thumbBox.style.width = `${t.w}px`;
    p.thumbBox.style.height = `${t.h}px`;
    annot.placeLayers(p, w, h);
    forms.place(p, w);
  }
  scheduleRender();
}

function setZoom(zoom: Zoom) {
  if (zoom.mode === "scale") zoom.scale = Math.min(MAX_SCALE, Math.max(MIN_SCALE, zoom.scale));
  state.zoom = zoom;
  keepingPosition(layout);
}

function zoomBy(factor: number) {
  const p = state.pages[state.current];
  if (p) setZoom({ mode: "scale", scale: scaleOf(p) * factor });
}

// ---------- Rendu des pages (image demandée au cœur, à la bonne résolution) ----------

const bucket = (px: number) => Math.min(6000, Math.ceil(px / 64) * 64);

function loadInto(box: HTMLElement, url: string, done: () => void) {
  const img = new Image();
  img.decoding = "async";
  img.draggable = false;
  img.src = url;
  img
    .decode()
    .then(() => {
      box.querySelector("img")?.remove();
      box.prepend(img);
      done();
    })
    .catch(() => {
      /* page illisible : on garde le fond blanc */
    });
}

function renderPage(p: Page) {
  if (!p.visible) return;
  const width = bucket(p.el.offsetWidth * devicePixelRatio);
  if (width === p.shownWidth || width === p.wantedWidth) return;
  p.wantedWidth = width;
  loadInto(p.el, api.pageUrl(p.doc.id, p.index, width, p.rotation, forms.hasForm(p.doc)), () => {
    if (p.wantedWidth === width) p.shownWidth = width;
  });
}

function renderThumb(p: Page) {
  if (!p.thumbVisible) return;
  const width = bucket(THUMB_W * devicePixelRatio);
  const key = `${width}/${p.rotation}`;
  if (key === p.thumbKey) return;
  p.thumbKey = key;
  loadInto(p.thumbBox, api.pageUrl(p.doc.id, p.index, width, p.rotation), () => {});
}

let renderTimer = 0;
function scheduleRender() {
  clearTimeout(renderTimer);
  // Petite attente : pendant un zoom continu, on ne redemande l'image qu'à la fin.
  renderTimer = window.setTimeout(() => state.pages.forEach(renderPage), 90);
}

const pageOf = new WeakMap<Element, Page>();

const pageObserver = new IntersectionObserver(
  (entries) => {
    for (const e of entries) {
      const p = pageOf.get(e.target);
      if (!p) continue;
      p.visible = e.isIntersecting;
      renderPage(p);
    }
  },
  { root: viewer, rootMargin: "60% 0px" },
);

const thumbObserver = new IntersectionObserver(
  (entries) => {
    for (const e of entries) {
      const p = pageOf.get(e.target);
      if (!p) continue;
      p.thumbVisible = e.isIntersecting;
      renderThumb(p);
    }
  },
  { root: thumbsEl, rootMargin: "50% 0px" },
);

// ---------- Miniatures ----------

function thumbSize(p: Page) {
  const b = baseSize(p);
  const s = Math.min(THUMB_W / b.w, 190 / b.h);
  return { w: Math.round(b.w * s), h: Math.round(b.h * s) };
}

/** « R26100303_261026…_19668_.jpg » → « R261003…668_.jpg ». */
function shortName(name: string, max = 18): string {
  if (name.length <= max) return name;
  const dot = name.lastIndexOf(".");
  const ext = dot > 0 ? name.slice(dot) : "";
  const stem = dot > 0 ? name.slice(0, dot) : name;
  const keep = Math.max(2, max - ext.length - 1);
  const head = Math.ceil(keep * 0.6);
  return `${stem.slice(0, head)}…${stem.slice(stem.length - (keep - head))}${ext}`;
}

// ---------- Création des pages ----------

function makePage(doc: DocInfo, index: number): Page {
  const el = document.createElement("div");
  el.className = "page" + (doc.kind === "pdf" ? "" : " picture");
  const hlLayer = document.createElement("div");
  hlLayer.className = "hl-layer";
  el.append(hlLayer);

  const thumb = document.createElement("button");
  thumb.className = "thumb";
  const thumbBox = document.createElement("div");
  thumbBox.className = "thumb-page" + (doc.kind === "pdf" ? "" : " picture");
  const label = document.createElement("div");
  label.className = "thumb-label";
  thumb.append(thumbBox, label);
  thumb.title = doc.kind === "pdf" ? `${doc.name} — page ${index + 1}` : doc.name;

  const { hlSvg, annotSvg } = annot.createLayers();
  const p: Page = {
    doc, index, rotation: 0, el, hlLayer, annots: [], hlSvg, annotSvg, thumb, thumbBox, label,
    visible: false, thumbVisible: false, shownWidth: 0, wantedWidth: 0, thumbKey: "",
  };
  annot.attach(p);
  forms.attach(p);
  pageOf.set(el, p);
  pageOf.set(thumb, p);
  thumb.addEventListener("click", (e) => !justDragged && clickThumb(p, e));
  thumb.addEventListener("contextmenu", (e) => thumbMenu(p, e));
  thumb.addEventListener("pointerdown", (e) => startDrag(p, e));
  return p;
}

/** Remet la page et les miniatures dans l'ordre de state.pages, puis met tout à jour. */
function refresh() {
  const pageEls = new Set(state.pages.map((p) => p.el));
  for (const el of [...pagesEl.children]) {
    if (!pageEls.has(el as HTMLElement)) {
      pageObserver.unobserve(el);
      el.remove();
    }
  }
  for (const el of [...thumbsEl.children]) {
    if (el === dropLine) continue;
    const p = pageOf.get(el);
    if (!p || !state.pages.includes(p)) {
      thumbObserver.unobserve(el);
      el.remove();
    }
  }
  state.pages.forEach((p, i) => {
    pagesEl.append(p.el);
    thumbsEl.append(p.thumb);
    pageObserver.observe(p.el);
    thumbObserver.observe(p.thumb);
    p.label.textContent = p.doc.kind === "pdf" ? String(i + 1).padStart(2, "0") : shortName(p.doc.name);
  });
  for (const p of [...state.selection]) if (!state.pages.includes(p)) state.selection.delete(p);
  document.body.classList.toggle("no-doc", state.pages.length === 0);
  updateFilmstrip();
  if (inSheet()) buildSheet();
  layout();
  drawHighlights();
  state.current = Math.min(state.current, Math.max(0, state.pages.length - 1));
  if (!state.selection.size && state.pages[state.current]) state.selection.add(state.pages[state.current]);
  paintSelection();
  updateTitle();
}

// ---------- Une image seule : la bande des images du dossier ----------

/** Une seule photo à l'écran (pas d'aperçu rapide) : la bande du dossier remplace la colonne. */
const singleImage = () =>
  state.pages.length === 1 && state.pages[0].doc.kind !== "pdf" && !document.body.classList.contains("apercu-rapide");

/** Avant même d'ouvrir les fichiers : outils et colonnes selon leur genre (d'après le nom).
 * La première image est déjà juste grâce à `window.__APERCU_GENRE__` (voir index.html). */
function presetLayout(paths: string[]) {
  const pictures = paths.every(isImageName);
  const pdfs = paths.every((p) => /\.pdf$/i.test(p));
  document.body.classList.toggle("only-pictures", pictures);
  document.body.classList.toggle("only-pdf", pdfs);
  document.body.classList.toggle("single-image", pictures && paths.length === 1 && !document.body.classList.contains("apercu-rapide"));
  $("view-switch").hidden = !pictures || paths.length < 2;
}

/** Les outils suivent le genre des documents ouverts : sans PDF, pas de formulaire, de
 * surlignage ni de recherche ; sans photo, pas de recadrage ni de réglage de couleur. */
const onlyPictures = () => state.pages.length > 0 && state.pages.every((p) => p.doc.kind !== "pdf");
const onlyPdf = () => state.pages.length > 0 && state.pages.every((p) => p.doc.kind === "pdf");

function updateFilmstrip() {
  const single = singleImage();
  document.body.classList.toggle("single-image", single);
  document.body.classList.toggle("only-pictures", onlyPictures());
  document.body.classList.toggle("only-pdf", onlyPdf());
  // Plusieurs photos : le bouton flottant passe de la photo à la planche de vignettes.
  document.body.classList.remove("many-pictures");
  $("view-switch").hidden = state.pages.length < 2 || onlyPdf() || document.body.classList.contains("apercu-rapide");
  if (single) film.show(state.pages[0].doc.path);
  else film.hide();
}

/** Flèches gauche / droite avec une image seule : l'image d'à côté dans le dossier. */
function openNeighbor(delta: number): boolean {
  if (!singleImage() || document.body.classList.contains("busy")) return false;
  const next = film.neighbor(delta);
  if (next) openFiles([next]);
  return !!next;
}

// ---------- Copier des fichiers (à coller dans l'Explorateur, un mail…) ----------

const isImageName = (p: string) => /\.(jpe?g|png|webp|gif|heic)$/i.test(p);

/**
 * Ctrl+C : les images choisies (bande du dossier ou colonne des miniatures).
 * Ctrl+Maj+C : ajoute l'image affichée à celles déjà copiées.
 */
async function copyFiles(add: boolean) {
  let paths: string[];
  if (add) paths = state.pages[state.current] ? [state.pages[state.current].doc.path] : [];
  else if (singleImage() && film.selectedPaths().length) paths = film.selectedPaths();
  else paths = [...new Set(selectedPages().map((p) => p.doc.path))];
  if (!paths.length) return;
  try {
    const n = await api.copyFiles(paths, add);
    const word = paths.every(isImageName) ? "image" : "fichier";
    toast(n === 1 ? `1 ${word} dans le presse-papier` : `${n} ${word}s dans le presse-papier`, 3000);
  } catch (e) {
    toast(`Impossible de copier : ${e}`);
  }
}

// ---------- Sélection ----------

function paintSelection() {
  for (const p of state.pages) {
    p.thumb.classList.toggle("selected", state.selection.has(p));
    sheetItems.get(p)?.classList.toggle("selected", state.selection.has(p));
  }
}

// ---------- Planche contact : toutes les pages en grille ----------

const sheetEl = $("sheet");
const sheetItems = new Map<Page, HTMLElement>();
const SHEET_W = 150;
const inSheet = () => document.body.classList.contains("sheet-mode");

const sheetObserver = new IntersectionObserver(
  (entries) => {
    for (const e of entries) {
      if (!e.isIntersecting) continue;
      const img = e.target.querySelector("img");
      if (img && !img.getAttribute("src")) img.src = img.dataset.src!;
      sheetObserver.unobserve(e.target);
    }
  },
  { root: sheetEl, rootMargin: "50% 0px" },
);

function buildSheet() {
  sheetItems.clear();
  sheetObserver.disconnect();
  if (!inSheet()) return sheetEl.replaceChildren();
  const groups: HTMLElement[] = [];
  let grid: HTMLElement | null = null;
  let lastDoc: DocInfo | null = null;
  state.pages.forEach((p, i) => {
    // Un titre par document (fusionnés : plusieurs titres), qu'on peut replier.
    if (p.doc !== lastDoc && (p.doc.kind === "pdf" || !lastDoc)) {
      const head = document.createElement("button");
      head.className = "sheet-head";
      head.innerHTML = `<svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2"><path d="M6 9l6 6 6-6"/></svg><span></span>`;
      head.querySelector("span")!.textContent = p.doc.kind === "pdf" ? p.doc.name : "Images";
      grid = document.createElement("div");
      grid.className = "sheet-grid";
      const g = grid;
      head.addEventListener("click", () => head.classList.toggle("folded", g.classList.toggle("folded")));
      groups.push(head, grid);
    }
    lastDoc = p.doc;
    const b = baseSize(p);
    const k = Math.min(SHEET_W / b.w, (SHEET_W * 1.42) / b.h);
    const item = document.createElement("button");
    item.className = "sheet-item";
    item.innerHTML = `<div class="sheet-page${p.doc.kind === "pdf" ? "" : " picture"}"><img alt="" draggable="false" /></div><span class="sheet-label"></span>`;
    const page = item.firstElementChild as HTMLElement;
    page.style.width = `${Math.round(b.w * k)}px`;
    page.style.height = `${Math.round(b.h * k)}px`;
    item.querySelector("img")!.dataset.src = api.pageUrl(p.doc.id, p.index, bucket(SHEET_W * devicePixelRatio), p.rotation);
    item.querySelector(".sheet-label")!.textContent = p.doc.kind === "pdf" ? String(i + 1) : shortName(p.doc.name);
    item.addEventListener("click", (e) => clickThumb(p, e));
    item.addEventListener("dblclick", () => setSheet(false, i));
    item.addEventListener("contextmenu", (e) => thumbMenu(p, e));
    grid!.append(item);
    sheetItems.set(p, item);
    sheetObserver.observe(item);
  });
  sheetEl.replaceChildren(...groups);
  paintSelection();
}

/** Ouvre ou ferme la planche contact ; en la fermant, on revient sur la page `goToIndex`. */
function setSheet(on: boolean, goToIndex = state.current) {
  document.body.classList.toggle("sheet-mode", on && state.pages.length > 0);
  buildSheet();
  if (inSheet()) {
    sheetEl.focus();
    sheetItems.get(state.pages[state.current])?.scrollIntoView({ block: "nearest" });
  } else {
    keepingPosition(layout);
    goTo(goToIndex);
  }
}

function viewMenu() {
  const r = $("view-menu").getBoundingClientRect();
  const sheet = inSheet();
  const hidden = document.body.classList.contains("no-sidebar");
  showMenu(r.left - 34, r.bottom + 6, [
    {
      label: "Miniatures",
      checked: !sheet && !hidden,
      action: () => {
        document.body.classList.remove("no-sidebar");
        if (sheet) setSheet(false);
      },
    },
    { label: "Planche contact", checked: sheet, disabled: !state.pages.length, action: () => setSheet(!sheet) },
    "-",
    {
      label: "Masquer la barre latérale",
      checked: !sheet && hidden,
      action: () => {
        document.body.classList.add("no-sidebar");
        if (sheet) setSheet(false);
      },
    },
  ]);
}

function selectOnly(p: Page) {
  state.selection = new Set([p]);
  state.anchor = state.pages.indexOf(p);
  paintSelection();
}

function clickThumb(p: Page, e: MouseEvent) {
  const i = state.pages.indexOf(p);
  if (e.shiftKey) {
    const [a, b] = [Math.min(state.anchor, i), Math.max(state.anchor, i)];
    state.selection = new Set(state.pages.slice(a, b + 1));
  } else if (e.ctrlKey || e.metaKey) {
    if (state.selection.has(p) && state.selection.size > 1) state.selection.delete(p);
    else state.selection.add(p);
    state.anchor = i;
  } else {
    state.selection = new Set([p]);
    state.anchor = i;
  }
  paintSelection();
  goTo(i, false);
}

/** Les pages sélectionnées, dans l'ordre du document. */
function selectedPages(): Page[] {
  const sel = state.pages.filter((p) => state.selection.has(p));
  return sel.length ? sel : state.pages[state.current] ? [state.pages[state.current]] : [];
}

// ---------- Annuler / rétablir ----------

const snapshot = (): Snapshot => state.pages.map((page) => ({ page, rotation: page.rotation, annots: page.annots }));
const sameSnapshot = (a: Snapshot, b: Snapshot) =>
  a.length === b.length && a.every((x, i) => x.page === b[i].page && x.rotation === b[i].rotation && x.annots === b[i].annots);
const isModified = () => state.pages.length > 0 && (!sameSnapshot(snapshot(), state.saved) || forms.isDirty());

function restore(s: Snapshot) {
  keepingPosition(() => {
    for (const { page, rotation, annots } of s) {
      page.annots = annots;
      if (page.rotation !== rotation) {
        page.rotation = rotation;
        page.shownWidth = page.wantedWidth = 0;
        page.thumbKey = "";
      }
    }
    state.pages = s.map((x) => x.page);
    refresh();
    annot.forget();
    annot.renderAll();
    const cur = state.pages[state.current];
    if (cur && state.selection.size <= 1) selectOnly(cur);
    state.pages.forEach(renderThumb);
  });
}

/** Enregistre l'état actuel pour Ctrl+Z, puis applique la modification. */
function change(modify: () => void) {
  state.undo.push(snapshot());
  if (state.undo.length > 200) state.undo.shift();
  state.redo = [];
  modify();
}

function undo() {
  const s = state.undo.pop();
  if (!s) return;
  state.redo.push(snapshot());
  restore(s);
}

function redo() {
  const s = state.redo.pop();
  if (!s) return;
  state.undo.push(snapshot());
  restore(s);
}

// ---------- Page courante, titre, navigation ----------

function updateTitle() {
  const name = $("title-name");
  const sub = $("title-sub");
  const p = state.pages[state.current];
  if (!p) {
    toggleInfo(false);
    name.textContent = "Documents";
    sub.textContent = "";
    if (api.inTauri) getCurrentWindow().setTitle("Documents");
    return;
  }
  name.textContent = p.doc.name;
  const total = state.pages.length;
  const docsShown = new Set(state.pages.map((x) => x.doc)).size;
  let text: string;
  if (docsShown > 1) {
    text = `${docsShown} documents, ${total} pages au total`;
  } else if (p.doc.kind === "pdf") {
    text = `Page ${state.current + 1} sur ${total}`;
  } else {
    const s = baseSize(p);
    text = `${Math.round(s.w)} × ${Math.round(s.h)} pixels`;
  }
  const modified = isModified();
  sub.textContent = modified ? `${text} — Modifié` : text;
  scheduleInspector();
  if (api.inTauri) getCurrentWindow().setTitle(`${modified ? "• " : ""}${p.doc.name} — Documents`);
}

let inspectorTimer = 0;
function scheduleInspector() {
  if (!inspectorOpen()) return;
  clearTimeout(inspectorTimer);
  inspectorTimer = window.setTimeout(renderInspector, 60);
}

function setCurrent(i: number) {
  if (i === state.current) return;
  state.current = i;
  const p = state.pages[i];
  if (p) {
    if (state.selection.size <= 1) selectOnly(p);
    p.thumb.scrollIntoView({ block: "nearest" });
    sheetItems.get(p)?.scrollIntoView({ block: "nearest" });
  }
  updateTitle();
}

function goTo(i: number, select = true) {
  const p = state.pages[Math.max(0, Math.min(state.pages.length - 1, i))];
  if (!p) return;
  viewer.scrollTop = p.el.offsetTop - PAD;
  const k = state.pages.indexOf(p);
  if (select) selectOnly(p);
  state.current = -1;
  setCurrent(k);
}

/** La page courante est celle qui occupe le haut de la zone d'affichage. */
function pageAtScroll(): number {
  const y = viewer.scrollTop + viewer.clientHeight * 0.35;
  let lo = 0;
  let hi = state.pages.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (state.pages[mid].el.offsetTop - GAP / 2 <= y) lo = mid;
    else hi = mid - 1;
  }
  return lo;
}

let scrollFrame = 0;
viewer.addEventListener("scroll", () => {
  cancelAnimationFrame(scrollFrame);
  scrollFrame = requestAnimationFrame(() => {
    if (state.pages.length) setCurrent(pageAtScroll());
  });
});

new ResizeObserver(() => {
  if (state.zoom.mode !== "scale") keepingPosition(layout);
}).observe(viewer);

// ---------- Modifier les pages : pivoter, supprimer, déplacer ----------

function rotate(delta: number) {
  const pages = selectedPages();
  if (!pages.length) return;
  change(() =>
    keepingPosition(() => {
      for (const p of pages) {
        p.rotation = (p.rotation + delta + 360) % 360;
        p.shownWidth = p.wantedWidth = 0;
      }
      layout();
      pages.forEach(renderThumb);
      drawHighlights();
      updateTitle();
    }),
  );
}

function deleteSelected() {
  const doomed = new Set(selectedPages());
  if (!doomed.size) return;
  const first = state.pages.findIndex((p) => doomed.has(p));
  change(() => {
    state.pages = state.pages.filter((p) => !doomed.has(p));
    state.selection.clear();
    state.current = Math.min(first, state.pages.length - 1);
    refresh();
    if (state.pages.length) goTo(state.current);
  });
}

/** Déplace les pages données pour qu'elles commencent à la position `to` (avant le déplacement). */
function movePages(moving: Page[], to: number) {
  const set = new Set(moving);
  const before = state.pages.slice(0, to).filter((p) => !set.has(p));
  const after = state.pages.slice(to).filter((p) => !set.has(p));
  const order = [...before, ...moving, ...after];
  if (order.every((p, i) => p === state.pages[i])) return;
  change(() => {
    state.pages = order;
    refresh();
    goTo(state.pages.indexOf(moving[0]), false);
  });
}

// ---------- Glisser les miniatures ----------
// À la souris plutôt qu'en « glisser-déposer » HTML : sous Windows, celui-ci est réservé
// aux fichiers qui arrivent de l'Explorateur.

const dropLine = document.createElement("div");
dropLine.className = "drop-line";
let drag: { pages: Page[]; startY: number; ghost: HTMLElement | null; pointer: number } | null = null;
let justDragged = false;

function startDrag(p: Page, e: PointerEvent) {
  if (e.button !== 0 || e.shiftKey || e.ctrlKey || e.metaKey) return;
  drag = { pages: [], startY: e.clientY, ghost: null, pointer: e.pointerId };
  const move = (ev: PointerEvent) => {
    if (!drag) return;
    if (!drag.ghost) {
      if (Math.abs(ev.clientY - drag.startY) < 6) return;
      if (!state.selection.has(p)) selectOnly(p);
      drag.pages = selectedPages();
      const ghost = p.thumbBox.cloneNode(true) as HTMLElement;
      ghost.className = "drag-ghost";
      if (drag.pages.length > 1) ghost.dataset.count = String(drag.pages.length);
      document.body.append(ghost);
      drag.ghost = ghost;
      document.body.classList.add("dragging-pages");
    }
    drag.ghost.style.left = `${ev.clientX + 8}px`;
    drag.ghost.style.top = `${ev.clientY + 8}px`;
    showDropLine(insertionIndexAt(ev.clientY));
    const r = thumbsEl.getBoundingClientRect();
    if (ev.clientY < r.top + 40) thumbsEl.scrollTop -= 12;
    else if (ev.clientY > r.bottom - 40) thumbsEl.scrollTop += 12;
  };
  const up = (ev: PointerEvent) => {
    document.removeEventListener("pointermove", move);
    document.removeEventListener("pointerup", up);
    const d = drag;
    drag = null;
    if (!d?.ghost) return;
    d.ghost.remove();
    hideDropLine();
    document.body.classList.remove("dragging-pages");
    justDragged = true;
    setTimeout(() => (justDragged = false), 0);
    const r = sidebar.getBoundingClientRect();
    if (ev.clientX >= r.left && ev.clientX <= r.right + 40) movePages(d.pages, insertionIndexAt(ev.clientY));
  };
  document.addEventListener("pointermove", move);
  document.addEventListener("pointerup", up);
}

/** Position d'insertion (0…n) sous la souris dans la colonne des miniatures. */
function insertionIndexAt(clientY: number): number {
  for (let i = 0; i < state.pages.length; i++) {
    const r = state.pages[i].thumb.getBoundingClientRect();
    if (clientY < r.top + r.height / 2) return i;
  }
  return state.pages.length;
}

function showDropLine(index: number) {
  const ref = state.pages[index]?.thumb ?? state.pages[state.pages.length - 1]?.thumb;
  if (!ref) return;
  const top = index < state.pages.length ? ref.offsetTop - GAP / 2 : ref.offsetTop + ref.offsetHeight + GAP / 2;
  dropLine.style.top = `${top - 1}px`;
  if (!dropLine.isConnected) thumbsEl.append(dropLine);
}

function hideDropLine() {
  dropLine.remove();
}

// ---------- Ouverture des fichiers ----------

/**
 * Ouvre des fichiers. `insertAt` : position dans la colonne (fusion) ; sans position,
 * les fichiers remplacent ce qui est ouvert.
 */
async function openFiles(paths: string[], insertAt?: number, asOpened = false) {
  if (!paths.length) return;
  if (insertAt === undefined && !(await confirmDiscard())) return;
  document.body.classList.add("busy");
  try {
    const result = await api.openPaths(paths);
    for (const e of result.errors) toast(`« ${e.name} » : ${e.message}`);
    if (!result.docs.length) return;
    // Les formulaires d'abord : leurs pages s'affichent sans les champs, remplis ici.
    const withForms = await Promise.all(result.docs.map(forms.load));
    if (withForms.some(Boolean)) toast("Ce document contient un formulaire : cliquez dans les champs pour le remplir.");
    const fresh = result.docs.flatMap((doc) => doc.pages.map((_, i) => makePage(doc, i)));
    if (insertAt === undefined || !state.pages.length) {
      if (state.docs.length) {
        api.closeDocs(state.docs.map((d) => d.id));
        state.docs.forEach((d) => forms.forget(d.id));
      }
      state.docs = [...result.docs];
      state.pages = fresh;
      state.undo = [];
      state.redo = [];
      state.selection.clear();
      state.current = 0;
      state.saveTarget = null;
      state.zoom = defaultZoom();
      clearSearch();
      refresh();
      annot.forget();
      state.saved = snapshot();
      updateTitle();
      goTo(0);
    } else if (asOpened) {
      // Fichiers envoyés à la suite par Windows : comme s'ils avaient été ouverts ensemble
      // (pas « Modifié », rien à annuler), et on reste sur l'image affichée.
      state.docs.push(...result.docs);
      state.pages.splice(insertAt, 0, ...fresh);
      refresh();
      state.saved = snapshot();
      updateTitle();
    } else {
      state.docs.push(...result.docs);
      change(() => {
        state.pages.splice(insertAt, 0, ...fresh);
        refresh();
        state.selection = new Set(fresh);
        paintSelection();
        goTo(insertAt, false);
      });
    }
    if (searchInput.value.trim()) runSearch();
  } catch (e) {
    toast(String(e));
  } finally {
    document.body.classList.remove("busy");
  }
}

async function chooseFiles(insertAt?: number) {
  if (!api.inTauri) return;
  const picked = await openDialog({
    multiple: true,
    filters: [{ name: "PDF et images", extensions: FILE_TYPES }],
  });
  if (picked) openFiles(Array.isArray(picked) ? picked : [picked], insertAt);
}

// ---------- Enregistrer ----------

const pageRefs = (pages: Page[]): api.PageRef[] =>
  pages.map((p) => ({ doc: p.doc.id, page: p.index, rotation: p.rotation, annotations: p.annots }));

function markSaved() {
  state.saved = snapshot();
  forms.markSaved();
  updateTitle();
}

const stem = (name: string) => name.replace(/\.[^.]+$/, "");

async function askPdfPath(defaultName: string): Promise<string | null> {
  const first = state.pages[0]?.doc.path ?? "";
  const dir = first.replace(/[\\/][^\\/]*$/, "");
  const sep = first.includes("\\") ? "\\" : "/";
  return saveDialog({
    defaultPath: dir ? `${dir}${sep}${defaultName}` : defaultName,
    filters: [{ name: "Document PDF", extensions: ["pdf"] }],
  });
}

async function withBusy(work: () => Promise<void>) {
  document.body.classList.add("busy");
  try {
    await work();
  } catch (e) {
    toast(`L'enregistrement a échoué : ${e}`);
    throw e;
  } finally {
    document.body.classList.remove("busy");
  }
}

/**
 * « Enregistrer » : si la fenêtre ne contient que des photos, on enregistre leur rotation
 * dans chaque fichier ; sinon tout part dans le PDF d'origine (ou celui choisi avant).
 */
async function save(): Promise<boolean> {
  if (!state.pages.length) return true;
  const onlyPictures = state.pages.every((p) => p.doc.kind !== "pdf");
  const target = state.saveTarget ?? (state.pages[0].doc.kind === "pdf" ? state.pages[0].doc.path : null);
  if (onlyPictures && !state.saveTarget) {
    await withBusy(async () => {
      for (const p of state.pages) {
        // Annotée : l'image finale est dessinée ici, puis réenregistrée dans son format.
        if (p.annots.length) await api.savePicturePixels(p.doc.id, await annot.flatten(p), p.doc.path);
        else if (p.rotation || p.doc.edited) await api.savePicture(p.doc.id, p.rotation, p.doc.path);
      }
    });
    markSaved();
    toast("Photos enregistrées.");
    return true;
  }
  if (!target) return saveAs();
  await withBusy(() => api.savePdf(pageRefs(state.pages), target, forms.fills()));
  markSaved();
  toast(`« ${target.split(/[\\/]/).pop()} » enregistré.`);
  return true;
}

async function saveAs(): Promise<boolean> {
  if (!state.pages.length) return false;
  const dest = await askPdfPath(`${stem(state.pages[0].doc.name)}.pdf`);
  if (!dest) return false;
  await withBusy(() => api.savePdf(pageRefs(state.pages), dest, forms.fills()));
  state.saveTarget = dest;
  markSaved();
  toast(`« ${dest.split(/[\\/]/).pop()} » enregistré.`);
  return true;
}

/** Exporte seulement les pages sélectionnées dans un nouveau PDF (sans rien changer ici). */
async function exportSelection() {
  const pages = selectedPages();
  if (!pages.length) return;
  const first = pages[0];
  const name =
    pages.length === 1 && first.doc.kind === "pdf"
      ? `${stem(first.doc.name)} - page ${state.pages.indexOf(first) + 1}.pdf`
      : `${stem(first.doc.name)}.pdf`;
  const dest = await askPdfPath(name);
  if (!dest) return;
  await withBusy(() => api.savePdf(pageRefs(pages), dest, forms.fills()));
  toast(`« ${dest.split(/[\\/]/).pop()} » créé.`);
}

const sizeText = (bytes: number) =>
  bytes >= 1024 * 1024
    ? `${(bytes / 1024 / 1024).toLocaleString("fr-FR", { maximumFractionDigits: 1 })} Mo`
    : `${Math.max(1, Math.round(bytes / 1024))} Ko`;

/** Un PDF plus léger pour l'envoyer par mail : les photos sont allégées, le texte reste net. */
async function exportReduced() {
  if (!state.pages.length) return;
  const choice = await ask(
    "Réduire la taille du PDF",
    "Les photos et images scannées sont allégées ; le texte reste net. Le document ouvert ne change pas.",
    ["Taille minimale", "Bonne qualité", "Annuler"],
    1,
  );
  if (choice < 0 || choice === 2) return;
  const level: api.ReduceLevel = choice === 0 ? "forte" : "moyenne";
  const dest = await askPdfPath(`${stem(state.pages[0].doc.name)} (réduit).pdf`);
  if (!dest) return;
  let sizes: [number, number] = [0, 0];
  await withBusy(async () => {
    sizes = await api.savePdfReduced(pageRefs(state.pages), dest, level, forms.fills());
  });
  const [before, after] = sizes;
  const name = dest.split(/[\\/]/).pop();
  toast(
    after < before * 0.95
      ? `« ${name} » créé : ${sizeText(before)} → ${sizeText(after)}.`
      : `« ${name} » créé (${sizeText(after)}) : ce document était déjà léger.`,
  );
}

/** Avant de fermer ou de remplacer les documents : proposer d'enregistrer. Renvoie false pour annuler. */
async function confirmDiscard(): Promise<boolean> {
  if (!isModified()) return true;
  const name = state.pages[0]?.doc.name ?? "";
  const choice = await ask(
    `Souhaitez-vous enregistrer les modifications apportées au document « ${name} » ?`,
    "Si vous n'enregistrez pas, vos modifications seront perdues.",
    ["Enregistrer", "Ne pas enregistrer", "Annuler"],
  );
  if (choice === 0) return save().catch(() => false);
  return choice === 1;
}

// ---------- Recherche ----------

let searchTimer = 0;
let searchSeq = 0;

async function runSearch() {
  const query = searchInput.value.trim();
  const seq = ++searchSeq;
  if (!query) return clearSearch();
  const hits = (await api.search(query)).filter((h) => pageIndexOfHit(h) >= 0);
  if (seq !== searchSeq) return; // une recherche plus récente est partie entre-temps
  hits.sort((a, b) => pageIndexOfHit(a) - pageIndexOfHit(b));
  state.hits = hits;
  state.activeHit = -1;
  showResults(query);
  drawHighlights();
  if (hits.length) goToHit(0);
}

function clearSearch() {
  searchSeq++;
  state.hits = [];
  state.activeHit = -1;
  resultsEl.hidden = true;
  thumbsEl.hidden = false;
  $("find-bar").hidden = true;
  drawHighlights();
}

/** Tri des résultats : par ordre des pages, ou les pages qui en ont le plus d'abord. */
let sortByRank = false;

const escapeHtml = (s: string) =>
  s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);

function highlightIn(text: string, query: string) {
  let i = text.toLowerCase().indexOf(query.toLowerCase());
  if (i < 0) return escapeHtml(text);
  // Ligne trop longue : on la coupe pour que le mot trouvé reste visible.
  if (i > 30) {
    const cut = text.indexOf(" ", i - 30);
    const start = cut >= 0 && cut < i ? cut + 1 : i - 30;
    text = "…" + text.slice(start);
    i = i - start + 1;
  }
  return `${escapeHtml(text.slice(0, i))}<mark>${escapeHtml(text.slice(i, i + query.length))}</mark>${escapeHtml(text.slice(i + query.length))}`;
}

function pageIndexOfHit(h: SearchHit) {
  return state.pages.findIndex((p) => p.doc.id === h.doc && p.index === h.page);
}

/** Les résultats regroupés par page (une ligne par page). */
function showResults(query: string) {
  thumbsEl.hidden = true;
  resultsEl.hidden = false;
  const byPage = new Map<number, number[]>();
  state.hits.forEach((h, i) => {
    const pi = pageIndexOfHit(h);
    byPage.set(pi, [...(byPage.get(pi) ?? []), i]);
  });
  const groups = [...byPage.entries()];
  if (sortByRank) groups.sort((a, b) => b[1].length - a[1].length || a[0] - b[0]);
  const n = state.hits.length;
  const pages = groups.length;
  $("find-count").textContent =
    n === 0 ? "Aucun résultat" : pages === 1 ? `Trouvé sur 1 page` : `Trouvé sur ${pages} pages`;
  $("find-bar").hidden = false;
  const several = new Set(state.pages.map((p) => p.doc)).size > 1;
  const items: HTMLElement[] = groups.map(([pi, hits]) => {
    const p = state.pages[pi];
    const b = document.createElement("button");
    b.className = "result";
    b.dataset.hits = hits.join(",");
    const where = several ? `${shortName(p.doc.name, 16)} · p. ${pi + 1}` : `Page ${pi + 1}`;
    const count = hits.length === 1 ? "1 résultat" : `${hits.length} résultats`;
    b.innerHTML = `<div class="result-thumb"><img alt="" draggable="false" /></div>
      <div class="result-main"><div class="result-head"><span class="result-page"></span><span class="result-count"></span></div>
      <div class="result-text">${highlightIn(state.hits[hits[0]].context, query)}</div></div>`;
    b.querySelector(".result-page")!.textContent = where;
    b.querySelector(".result-count")!.textContent = String(hits.length);
    b.title = `${where} : ${count}`;
    const img = b.querySelector("img")!;
    img.src = api.pageUrl(p.doc.id, p.index, bucket(48 * devicePixelRatio), p.rotation);
    b.addEventListener("click", () => goToHit(hits[0]));
    return b;
  });
  if (!items.length) {
    const none = document.createElement("div");
    none.className = "results-head";
    none.textContent = "Aucun résultat";
    items.push(none);
  }
  resultsEl.replaceChildren(...items);
  paintActiveResult();
}

function paintActiveResult() {
  resultsEl.querySelectorAll<HTMLElement>(".result").forEach((el) => {
    const on = (el.dataset.hits ?? "").split(",").includes(String(state.activeHit));
    el.classList.toggle("selected", on);
    if (on) el.scrollIntoView({ block: "nearest" });
  });
}

/** Un rectangle de la page « à plat », une fois la page tournée par l'utilisateur. */
function rotateRect(r: NormRect, rotation: number): NormRect {
  switch (rotation) {
    case 90:
      return { x: 1 - r.y - r.h, y: r.x, w: r.h, h: r.w };
    case 180:
      return { x: 1 - r.x - r.w, y: 1 - r.y - r.h, w: r.w, h: r.h };
    case 270:
      return { x: r.y, y: 1 - r.x - r.w, w: r.h, h: r.w };
    default:
      return r;
  }
}

function drawHighlights() {
  for (const p of state.pages) p.hlLayer.replaceChildren();
  state.hits.forEach((h, i) => {
    const p = state.pages[pageIndexOfHit(h)];
    if (!p) return;
    for (const r0 of h.rects) {
      const r = rotateRect(r0, p.rotation);
      const box = document.createElement("div");
      box.className = "hl" + (i === state.activeHit ? " active" : "");
      // Un peu de marge autour du mot.
      box.style.left = `calc(${r.x * 100}% - 2px)`;
      box.style.top = `calc(${r.y * 100}% - 1px)`;
      box.style.width = `calc(${r.w * 100}% + 4px)`;
      box.style.height = `calc(${r.h * 100}% + 2px)`;
      p.hlLayer.append(box);
    }
  });
}

function goToHit(i: number) {
  const n = state.hits.length;
  if (!n) return;
  state.activeHit = ((i % n) + n) % n;
  const h = state.hits[state.activeHit];
  const pi = pageIndexOfHit(h);
  const p = state.pages[pi];
  drawHighlights();
  paintActiveResult();
  if (!p) return;
  const r = rotateRect(h.rects[0], p.rotation);
  const y = p.el.offsetTop + r.y * p.el.offsetHeight;
  const x = p.el.offsetLeft + r.x * p.el.offsetWidth;
  // On ne fait défiler que si le résultat n'est pas déjà bien visible.
  if (y < viewer.scrollTop + 40 || y > viewer.scrollTop + viewer.clientHeight - 60) {
    viewer.scrollTop = y - viewer.clientHeight / 3;
  }
  if (x < viewer.scrollLeft || x > viewer.scrollLeft + viewer.clientWidth - 40) {
    viewer.scrollLeft = x - viewer.clientWidth / 3;
  }
  selectOnly(p);
  setCurrent(pi);
}

searchInput.addEventListener("input", () => {
  searchInput.parentElement!.classList.toggle("filled", searchInput.value !== "");
  clearTimeout(searchTimer);
  searchTimer = window.setTimeout(runSearch, 250);
});

searchInput.addEventListener("keydown", (e) => {
  if (e.key === "Enter") {
    e.preventDefault();
    if (state.hits.length) goToHit(state.activeHit + (e.shiftKey ? -1 : 1));
    else runSearch();
  } else if (e.key === "Escape") {
    searchInput.value = "";
    searchInput.parentElement!.classList.remove("filled");
    clearSearch();
    viewer.focus();
  }
});

// ---------- Inspecteur : informations du document, liste des annotations ----------

const kindNames: Record<string, string> = {
  pdf: "Document PDF",
  jpeg: "Image JPEG",
  png: "Image PNG",
  webp: "Image WebP",
  gif: "Image GIF",
  heic: "Image HEIC",
};

const ANNOT_NAMES: Record<string, string> = {
  highlight: "Surlignage", rect: "Rectangle", ellipse: "Ovale", ink: "Dessin", text: "Texte",
  note: "Note", image: "Signature", rounded: "Rectangle arrondi", bubble: "Bulle", star: "Étoile", hexagon: "Hexagone",
};

let inspectorTab: "infos" | "annots" = "infos";
const details = new Map<number, Promise<api.DocDetails | null>>();

const inspectorOpen = () => document.body.classList.contains("inspector-open");

function toggleInfo(force?: boolean) {
  const open = force ?? !inspectorOpen();
  document.body.classList.toggle("inspector-open", open && state.pages.length > 0);
  $("show-info").classList.toggle("toggled", inspectorOpen());
  if (inspectorOpen()) renderInspector();
}

function setInspectorTab(tab: typeof inspectorTab) {
  inspectorTab = tab;
  $("tab-infos").classList.toggle("on", tab === "infos");
  $("tab-annots").classList.toggle("on", tab === "annots");
  renderInspector();
}

const dateText = (d: Date) =>
  isNaN(d.getTime())
    ? ""
    : d.toLocaleString("fr-FR", { day: "numeric", month: "short", year: "numeric", hour: "2-digit", minute: "2-digit" }).replace(",", " à");

const cm = (px: number) => ((px * 2.54) / 96).toLocaleString("fr-FR", { minimumFractionDigits: 2, maximumFractionDigits: 2 });

let inspectorSeq = 0;
async function renderInspector() {
  if (!inspectorOpen()) return;
  const body = $("inspector-body");
  const seq = ++inspectorSeq;
  if (inspectorTab === "annots") return renderAnnotList(body);
  const p = state.pages[state.current];
  if (!p) return body.replaceChildren();
  let d = details.get(p.doc.id);
  if (!d) {
    d = api.docDetails(p.doc.id).catch(() => null);
    details.set(p.doc.id, d);
  }
  const info = await d;
  if (seq !== inspectorSeq) return;
  const s = p.doc.pages[p.index];
  const groups: [string, string | null | undefined][][] = [
    [["Nom du fichier", p.doc.name], ["Type de document", kindNames[p.doc.kind] ?? p.doc.kind]],
    p.doc.kind === "pdf"
      ? [["Version PDF", info?.version], ["Nombre de pages", String(p.doc.pages.length)], ["Format de papier", `${cm(s.width)} × ${cm(s.height)} cm`]]
      : [["Dimensions", `${Math.round(s.width)} × ${Math.round(s.height)} pixels`]],
    [
      ["Taille", info ? sizeText(info.file_size) : null],
      ["Emplacement", p.doc.path],
      ["Date de création", info?.file_created ? dateText(new Date(info.file_created)) : null],
      ["Date de modification", info?.file_modified ? dateText(new Date(info.file_modified)) : null],
    ],
    [
      ["Titre", info?.title], ["Auteur", info?.author], ["Objet", info?.subject], ["Mots-clés", info?.keywords],
      ["Créé avec", info?.creator], ["Produit par", info?.producer],
      ["Créé le", info?.created ? dateText(new Date(info.created)) : null],
      ["Modifié le", info?.modified ? dateText(new Date(info.modified)) : null],
    ],
  ];
  body.replaceChildren(
    ...groups
      .map((g) => g.filter(([, v]) => v))
      .filter((g) => g.length)
      .map((g) => {
        const card = document.createElement("div");
        card.className = "inspector-card";
        for (const [k, v] of g) {
          const row = document.createElement("div");
          row.className = "inspector-row" + (k.length + v!.length > 36 ? " long" : "");
          row.innerHTML = `<span class="inspector-key"></span><span class="inspector-value"></span>`;
          row.children[0].textContent = k;
          row.children[1].textContent = v!;
          (row.children[1] as HTMLElement).title = v!;
          card.append(row);
        }
        return card;
      }),
  );
}

function renderAnnotList(body: HTMLElement) {
  const head = document.createElement("div");
  head.className = "annot-row annot-head";
  head.innerHTML = "<span>Page</span><span>Type</span><span>Contenu</span>";
  const rows: HTMLElement[] = [];
  state.pages.forEach((p, pi) =>
    p.annots.forEach((a, i) => {
      const row = document.createElement("button");
      row.className = "annot-row";
      const color = "color" in a ? a.color : "stroke" in a ? (a.stroke ?? a.fill) : null;
      const kind = a.kind === "rect" && a.shape ? a.shape : a.kind === "line" ? (a.arrow ? "Flèche" : "Ligne") : a.kind;
      const content = a.kind === "text" || a.kind === "note" ? a.text.replace(/\s+/g, " ") : "";
      row.innerHTML = `<span></span><span class="annot-type"><i class="annot-color"></i><span></span></span><span class="annot-content"></span>`;
      row.children[0].textContent = String(pi + 1);
      (row.querySelector(".annot-color") as HTMLElement).style.background = color ?? "transparent";
      row.querySelector(".annot-type span")!.textContent = ANNOT_NAMES[kind] ?? kind;
      row.children[2].textContent = content;
      row.addEventListener("click", () => {
        goTo(pi);
        annot.selectAt(p, i);
      });
      rows.push(row);
    }),
  );
  if (!rows.length) {
    const empty = document.createElement("div");
    empty.className = "inspector-empty";
    empty.textContent = "Aucune annotation";
    rows.push(empty);
  }
  body.replaceChildren(head, ...rows);
}

// ---------- Messages ----------

function toast(message: string, duration = 5000) {
  const t = document.createElement("div");
  t.className = "toast";
  t.textContent = message;
  $("toasts").append(t);
  setTimeout(() => t.classList.add("leaving"), duration - 300);
  setTimeout(() => t.remove(), duration);
}

/** Message qui reste affiché, avec un bouton (ex. « Mettre à jour »). */
function banner(message: string, action: string, run: () => Promise<void>) {
  const t = document.createElement("div");
  t.className = "toast banner";
  const text = document.createElement("span");
  text.textContent = message;
  const later = document.createElement("button");
  later.className = "toast-button";
  later.textContent = "Plus tard";
  later.addEventListener("click", () => t.remove());
  const go = document.createElement("button");
  go.className = "toast-button primary";
  go.textContent = action;
  go.addEventListener("click", async () => {
    go.disabled = later.disabled = true;
    text.textContent = "Téléchargement de la mise à jour…";
    try {
      await run();
    } catch (e) {
      text.textContent = `La mise à jour a échoué : ${e}`;
      later.disabled = false;
    }
  });
  t.append(text, later, go);
  $("toasts").append(t);
}

// ---------- Aperçu rapide (Windows : Espace dans l'Explorateur) ----------

/** L'option est-elle cochée ? null : elle n'existe pas sur cet ordinateur. */
let apercuRapideOption: boolean | null = null;
/** Numéro de version de l'appli, rappelé en bas du menu du titre. */
let appVersion = "";

async function toggleApercuRapide() {
  const on = !apercuRapideOption;
  try {
    await api.setApercuRapide(on);
    apercuRapideOption = on;
    toast(
      on
        ? "Aperçu rapide activé : dans l'Explorateur, sélectionnez un PDF ou une image et appuyez sur Espace."
        : "Aperçu rapide désactivé.",
    );
  } catch (e) {
    toast(`Impossible de changer l'option : ${e}`);
  }
}

/** Cette fenêtre est un aperçu rapide : document seul, Espace ou Échap pour fermer. */
function startApercuRapide(path: string) {
  document.body.classList.add("apercu-rapide", "no-sidebar");
  // La page entière dans la fenêtre.
  openFiles([path]).then(() => setZoom({ mode: "fit" }));
  const win = getCurrentWindow();
  $("quick-open").addEventListener("click", async () => {
    await api.openInApp(path).catch((e) => toast(String(e)));
    win.destroy();
  });
  document.addEventListener(
    "keydown",
    (e) => {
      const typing = e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement;
      if (e.key === "Escape" || (e.key === " " && !typing)) {
        e.preventDefault();
        e.stopPropagation();
        win.destroy();
      }
    },
    true,
  );
}

/** Windows ne laisse pas une appli se mettre par défaut toute seule : on ouvre ses réglages. */
async function chooseDefaultApp() {
  try {
    await api.chooseDefaultApp();
    toast("Dans les réglages de Windows, choisissez Documents pour les fichiers .pdf, .jpg, .png, .webp et .gif.", 9000);
  } catch (e) {
    toast(String(e));
  }
}

// ---------- Menus ----------

function fileMenuItems(): MenuItem[] {
  const none = !state.pages.length;
  const n = selectedPages().length;
  const picture = !!state.pages[state.current] && state.pages[state.current].doc.kind !== "pdf";
  const items: MenuItem[] = [
    { label: "Ouvrir…", shortcut: "Ctrl+O", action: () => chooseFiles() },
    { label: "Insérer un fichier…", disabled: none, action: () => chooseFiles(state.current + 1) },
    "-",
    { label: "Enregistrer", shortcut: "Ctrl+S", disabled: none, action: () => save() },
    { label: "Enregistrer sous…", shortcut: "Ctrl+Maj+S", disabled: none, action: () => saveAs() },
    {
      label: n > 1 ? `Exporter les ${n} pages en PDF…` : "Exporter la page en PDF…",
      shortcut: "Ctrl+E",
      disabled: none,
      action: () => exportSelection(),
    },
    { label: "Exporter en taille réduite…", disabled: none, action: () => exportReduced() },
    { label: "Imprimer…", shortcut: "Ctrl+P", disabled: none, action: () => printDoc() },
    "-",
    ...(onlyPdf()
      ? []
      : ([
          { label: "Réglage de la couleur…", disabled: !picture, action: () => adjustColor() },
          { label: "Ajuster la taille…", disabled: !picture, action: () => resizePicture() },
          { label: "Retourner horizontalement", disabled: !picture, action: () => flip(true) },
          { label: "Retourner verticalement", disabled: !picture, action: () => flip(false) },
        ] as MenuItem[])),
    ...(apercuRapideOption === null
      ? []
      : ([
          "-",
          { label: "Aperçu rapide avec Espace (Explorateur)", checked: apercuRapideOption, action: () => toggleApercuRapide() },
          { label: "Utiliser Documents par défaut…", action: () => chooseDefaultApp() },
        ] as MenuItem[])),
    "-",
    ...(appVersion ? [{ label: `Documents ${appVersion}`, disabled: true, action: () => {} }] : []),
  ];
  // Pas deux séparateurs de suite (sections vides), ni en fin de menu.
  return items.filter((it, i) => it !== "-" || (i + 1 < items.length && items[i + 1] !== "-"));
}

function thumbMenu(p: Page, e: MouseEvent) {
  e.preventDefault();
  if (!state.selection.has(p)) {
    selectOnly(p);
    goTo(state.pages.indexOf(p), false);
  }
  const n = selectedPages().length;
  const pages = n > 1 ? `les ${n} pages` : "la page";
  showMenu(e.clientX, e.clientY, [
    { label: "Pivoter à gauche", shortcut: "Ctrl+L", action: () => rotate(-90) },
    { label: "Pivoter à droite", shortcut: "Ctrl+R", action: () => rotate(90) },
    "-",
    { label: `Exporter ${pages} en PDF…`, shortcut: "Ctrl+E", action: () => exportSelection() },
    { label: "Insérer un fichier après…", action: () => chooseFiles(state.pages.indexOf(p) + 1) },
    "-",
    { label: `Supprimer ${pages}`, shortcut: "Suppr", action: () => deleteSelected() },
  ]);
}

// ---------- Retouche des photos ----------

/** Remplace la page d'une photo par sa version retouchée (Ctrl+Z revient à l'ancienne). */
async function retouchPage(p: Page, r: api.Retouch) {
  let doc: DocInfo;
  try {
    doc = await api.retouchPicture(p.doc.id, r);
  } catch (e) {
    return toast(`La retouche a échoué : ${e}`);
  }
  state.docs.push(doc);
  const fresh = makePage(doc, 0);
  fresh.rotation = p.rotation;
  // Les annotations suivent la photo recadrée.
  fresh.annots = r.crop ? p.annots.map((a) => annot.moved(a, -r.crop!.x, -r.crop!.y)) : p.annots;
  const i = state.pages.indexOf(p);
  if (i < 0) return;
  change(() => {
    state.pages = state.pages.map((x) => (x === p ? fresh : x));
    state.selection.clear();
    refresh();
    annot.renderAll();
    goTo(i);
  });
}

/** Recadrer : avec une zone choisie, on la garde ; sinon, un cadre à ajuster sur la photo. */
function cropCurrent() {
  const sel = annot.cropSelection();
  if (sel) {
    const p = sel.page as Page;
    if (p.doc.kind === "pdf") {
      annot.clearCrop();
      return toast("Le recadrage marche sur les photos.");
    }
    const r = sel.rect;
    annot.clearCrop();
    annot.resetTool();
    return retouchPage(p, { crop: { x: Math.round(r.x), y: Math.round(r.y), w: Math.round(r.w), h: Math.round(r.h) } });
  }
  const p = state.pages[state.current];
  if (!p || p.doc.kind === "pdf") return toast("Le recadrage marche sur les photos : ouvrez une image.");
  annot.startCropFrame(p);
}

/** Barre « Annuler / Recadrer » sous la photo, tant qu'une zone est choisie sur une photo. */
function showCropBar() {
  const bar = $("crop-bar");
  const sel = annot.cropSelection();
  bar.hidden = !sel || (sel.page as Page).doc.kind === "pdf";
  document.body.classList.toggle("cropping", !bar.hidden);
}

/** La photo affichée, sans annotation (on ne sait pas les retourner ni les agrandir avec elle). */
function plainPicture(): Page | null {
  const p = state.pages[state.current];
  if (!p || p.doc.kind === "pdf") {
    toast("Cet outil marche sur les photos : ouvrez une image.");
    return null;
  }
  if (p.annots.length) {
    toast("Enregistrez d'abord les annotations de cette photo (Ctrl+S), puis recommencez.");
    return null;
  }
  return p;
}

function flip(horizontal: boolean) {
  const p = plainPicture();
  // Photo tournée dans l'appli : « horizontal » reste ce qu'on voit à l'écran.
  if (p) retouchPage(p, sideways(p) !== horizontal ? { flip_h: true } : { flip_v: true });
}

/** « Ajuster la taille » : nouvelle largeur et hauteur en pixels, proportionnelles ou non. */
function resizePicture() {
  const p = plainPicture();
  if (!p) return;
  const s = p.doc.pages[p.index];
  const W = Math.round(s.width);
  const H = Math.round(s.height);
  const box = document.createElement("div");
  box.className = "resize-form";
  box.innerHTML = `
    <label class="resize-row"><span>Ajuster à :</span><select id="rs-preset">
      <option value="">Personnalisée</option><option value="640">640 × 480</option><option value="800">800 × 600</option>
      <option value="1024">1024 × 768</option><option value="1280">1280 × 1024</option><option value="1920">1920 × 1080</option>
      <option value="50%">50 %</option><option value="25%">25 %</option></select></label>
    <label class="resize-row"><span>Largeur :</span><input id="rs-w" type="number" min="1" max="30000" /><em>pixels</em></label>
    <label class="resize-row"><span>Hauteur :</span><input id="rs-h" type="number" min="1" max="30000" /><em>pixels</em></label>
    <label class="resize-check"><input id="rs-lock" type="checkbox" checked /> Échelle proportionnelle</label>
    <div class="resize-result" id="rs-result"></div>`;
  const q = <T extends HTMLElement>(id: string) => box.querySelector<T>(`#${id}`)!;
  const wIn = q<HTMLInputElement>("rs-w");
  const hIn = q<HTMLInputElement>("rs-h");
  const lock = q<HTMLInputElement>("rs-lock");
  const preset = q<HTMLSelectElement>("rs-preset");
  wIn.value = String(W);
  hIn.value = String(H);
  const show = () => {
    const pct = Math.round((Number(wIn.value) / W) * 100);
    q("rs-result").textContent = `Taille produite : ${pct} % de l'original (${W} × ${H} pixels).`;
  };
  wIn.addEventListener("input", () => {
    preset.value = "";
    if (lock.checked) hIn.value = String(Math.max(1, Math.round((Number(wIn.value) * H) / W)));
    show();
  });
  hIn.addEventListener("input", () => {
    preset.value = "";
    if (lock.checked) wIn.value = String(Math.max(1, Math.round((Number(hIn.value) * W) / H)));
    show();
  });
  preset.addEventListener("change", () => {
    const v = preset.value;
    if (!v) return;
    // Un pourcentage, ou un cadre (largeur × hauteur) dans lequel la photo doit tenir.
    const frames: Record<string, number> = { 640: 480, 800: 600, 1024: 768, 1280: 1024, 1920: 1080 };
    const [fw, fh] = W >= H ? [Number(v), frames[v]] : [frames[v], Number(v)];
    const k = v.endsWith("%") ? parseInt(v) / 100 : Math.min(fw / W, fh / H);
    wIn.value = String(Math.max(1, Math.round(W * k)));
    hIn.value = String(Math.max(1, Math.round(H * k)));
    show();
  });
  show();
  ask("Dimensions de l'image", "", ["Annuler", "OK"], 1, box).then((choice) => {
    const w = Math.round(Number(wIn.value));
    const h = Math.round(Number(hIn.value));
    if (choice !== 1 || !(w >= 1 && h >= 1) || (w === W && h === H)) return;
    // Photo tournée dans l'appli : les cases parlent de ce qu'on voit, le cœur de la photo droite.
    retouchPage(p, { resize: sideways(p) ? [h, w] : [w, h] });
  });
}

// ---------- Imprimer ----------

/** Imprime toutes les pages (avec leurs annotations) grâce à l'impression de Windows. */
async function printDoc() {
  if (!state.pages.length) return;
  const root = document.createElement("div");
  root.className = "print-root";
  // Zone imprimable d'une feuille A4 avec 10 mm de marge, en pixels CSS.
  const maxW = 718;
  const maxH = 1046;
  const loads: Promise<unknown>[] = [];
  for (const p of state.pages) {
    const b = baseSize(p);
    const k = Math.min(maxW / b.w, maxH / b.h);
    const w = Math.round(b.w * k);
    const h = Math.round(b.h * k);
    const sheet = document.createElement("div");
    sheet.className = "print-page";
    sheet.style.width = `${w}px`;
    sheet.style.height = `${h}px`;
    const img = new Image();
    img.src = api.pageUrl(p.doc.id, p.index, bucket(w * 2.5), p.rotation);
    loads.push(img.decode().catch(() => {}));
    sheet.append(img);
    for (const layer of [p.hlSvg, p.annotSvg]) {
      const copy = layer.cloneNode(true) as SVGSVGElement;
      copy.querySelectorAll(".selection, foreignObject").forEach((el) => el.remove());
      const side = sideways(p);
      copy.style.width = `${side ? h : w}px`;
      copy.style.height = `${side ? w : h}px`;
      copy.style.transform = `translate(-50%, -50%) rotate(${p.rotation}deg)`;
      sheet.append(copy);
    }
    root.append(sheet);
  }
  document.body.append(root);
  document.body.classList.add("busy");
  await Promise.all(loads);
  document.body.classList.remove("busy");
  window.print();
  root.remove();
}

function adjustColor() {
  const p = state.pages[state.current];
  if (!p || p.doc.kind === "pdf") return toast("Les réglages de couleur marchent sur les photos : ouvrez une image.");
  retouch.open($("tool-adjust"), p, (r) => retouchPage(p, r));
}

// ---------- Boutons ----------

$("toggle-sidebar").addEventListener("click", () => {
  if (inSheet()) return setSheet(false);
  document.body.classList.toggle(singleImage() ? "no-filmstrip" : "no-sidebar");
});
$("view-menu").addEventListener("click", () => viewMenu());
$("view-switch").addEventListener("click", () => setSheet(!inSheet()));

// Petite fenêtre : sous 1100 px la recherche devient une loupe, sous 900 px
// les boutons de droite passent dans le menu « » ».
new ResizeObserver(() => {
  const w = document.body.clientWidth;
  document.body.classList.toggle("compact", w < 1100);
  document.body.classList.toggle("narrow", w < 900);
}).observe(document.body);

$("more-tools").addEventListener("click", () => {
  const r = $("more-tools").getBoundingClientRect();
  const none = !state.pages.length;
  const markup = document.body.classList.contains("markup-open");
  showMenu(r.left, r.bottom + 6, [
    { label: "Annoter", checked: markup, action: () => $("toggle-markup").click() },
    "-",
    { label: "Pivoter à gauche", shortcut: "Ctrl+L", disabled: none, action: () => rotate(-90) },
    { label: "Pivoter à droite", shortcut: "Ctrl+R", disabled: none, action: () => rotate(90) },
    ...(onlyPictures() ? [] : [{ label: "Remplir le formulaire", disabled: none, action: () => $("form-fill").click() }]),
    "-",
    { label: "Inspecteur", shortcut: "Ctrl+I", checked: inspectorOpen(), disabled: none, action: () => toggleInfo() },
  ]);
});
// Loupe (petite fenêtre) : un clic ouvre le champ de recherche.
document.querySelector(".search")!.addEventListener("mousedown", (e) => {
  if (!document.body.classList.contains("compact") || e.target === searchInput) return;
  e.preventDefault();
  searchInput.focus();
});
$("toggle-markup").addEventListener("click", (e) => {
  const open = document.body.classList.toggle("markup-open");
  (e.currentTarget as HTMLElement).setAttribute("aria-pressed", String(open));
  $("markup").setAttribute("aria-hidden", String(!open));
});
$("zoom-in").addEventListener("click", () => zoomBy(ZOOM_STEP));
$("zoom-out").addEventListener("click", () => zoomBy(1 / ZOOM_STEP));
$("zoom-toggle").addEventListener("click", () => {
  const actual = state.zoom.mode === "scale" && state.zoom.scale === 1;
  setZoom(actual ? defaultZoom() : { mode: "scale", scale: 1 });
});
$("rotate-left").addEventListener("click", () => rotate(-90));
$("crop").addEventListener("click", () => cropCurrent());
$("tool-adjust").addEventListener("click", () => adjustColor());
$("show-info").addEventListener("click", () => toggleInfo());
$("find-prev").addEventListener("click", () => goToHit(state.activeHit - 1));
$("find-next").addEventListener("click", () => goToHit(state.activeHit + 1));
$("find-done").addEventListener("click", () => {
  searchInput.value = "";
  searchInput.parentElement!.classList.remove("filled");
  clearSearch();
  viewer.focus();
});
for (const [id, rank] of [["sort-rank", true], ["sort-pages", false]] as const) {
  $(id).addEventListener("click", () => {
    sortByRank = rank;
    $("sort-rank").classList.toggle("on", rank);
    $("sort-pages").classList.toggle("on", !rank);
    if (state.hits.length) showResults(searchInput.value.trim());
  });
}
$("tab-infos").addEventListener("click", () => setInspectorTab("infos"));
$("tab-annots").addEventListener("click", () => setInspectorTab("annots"));
$("open-docs").addEventListener("click", () => chooseFiles());
$("add-docs").addEventListener("click", () => chooseFiles(state.pages.length));
// Le nom du document et sa flèche ouvrent le même menu.
$("title-row").addEventListener("click", () => {
  const r = $("title-row").getBoundingClientRect();
  const under = $("title-sub").getBoundingClientRect().bottom || r.bottom;
  showMenu(r.left, under + 6, fileMenuItems());
});

// ---------- Clavier et souris ----------

document.addEventListener("keydown", (e) => {
  if (document.querySelector(".modal-veil")) return;
  const ctrl = e.ctrlKey || e.metaKey;
  const typing =
    e.target instanceof HTMLInputElement ||
    e.target instanceof HTMLTextAreaElement ||
    e.target instanceof HTMLSelectElement ||
    (e.target instanceof HTMLElement && !!e.target.closest(".form-layer"));
  if (!typing && e.key === "Enter" && annot.cropSelection() && !$("crop-bar").hidden) {
    e.preventDefault();
    return cropCurrent();
  }
  if (!typing && annot.handleKey(e)) return;
  const k = e.key.toLowerCase();
  if (ctrl) {
    const hasPages = state.pages.length > 0;
    const actions: Record<string, () => void> = {
      o: () => chooseFiles(),
      s: () => (e.shiftKey ? saveAs() : save()),
      e: () => exportSelection(),
      f: () => {
        if (onlyPictures()) return; // pas de texte dans les photos
        searchInput.focus();
        searchInput.select();
      },
      "+": () => zoomBy(ZOOM_STEP),
      "=": () => zoomBy(ZOOM_STEP),
      "-": () => zoomBy(1 / ZOOM_STEP),
      "0": () => setZoom(defaultZoom()),
      "1": () => setZoom({ mode: "scale", scale: 1 }),
      r: () => rotate(90),
      k: () => cropCurrent(),
      l: () => rotate(-90),
      i: () => toggleInfo(),
      z: () => (typing ? undefined : e.shiftKey ? redo() : undo()),
      y: () => (typing ? undefined : redo()),
      a: () => {
        if (typing) return;
        state.selection = new Set(state.pages);
        paintSelection();
      },
      p: () => printDoc(),
      c: () => (typing ? undefined : copyFiles(e.shiftKey)),
    };
    const action = actions[k];
    if (action && !(typing && (k === "a" || k === "z" || k === "y" || k === "c"))) {
      e.preventDefault();
      if (hasPages || k === "o") action();
    }
    return;
  }
  if (e.key === "F5") e.preventDefault(); // pas de rechargement de la page
  if (typing) return;
  const nav: Record<string, number> = { PageDown: 1, ArrowRight: 1, PageUp: -1, ArrowLeft: -1 };
  // Haut / bas : de vignette en vignette quand on vient de cliquer dans la colonne des
  // miniatures (une ligne entière sur la planche contact) ; sinon la page défile.
  const vertical = e.key === "ArrowUp" || e.key === "ArrowDown";
  const step = e.key === "ArrowUp" ? -1 : 1;
  if ((e.key === "ArrowLeft" || e.key === "ArrowRight") && openNeighbor(e.key === "ArrowLeft" ? -1 : 1)) {
    e.preventDefault();
  } else if (vertical && inSheet()) {
    e.preventDefault();
    (document.activeElement as HTMLElement | null)?.blur();
    goTo(state.current + step * sheetColumns());
  } else if (vertical && thumbsFocused) {
    e.preventDefault();
    // Sinon le contour de focus resterait sur la vignette cliquée au départ.
    (document.activeElement as HTMLElement | null)?.blur();
    if (!openNeighbor(step)) goTo(state.current + step);
  } else if (e.key in nav) {
    e.preventDefault();
    if (thumbsFocused) (document.activeElement as HTMLElement | null)?.blur();
    goTo(state.current + nav[e.key]);
  } else if (e.key === "Home") {
    e.preventDefault();
    goTo(0);
  } else if (e.key === "End") {
    e.preventDefault();
    goTo(state.pages.length - 1);
  } else if (e.key === "Delete" || e.key === "Backspace") {
    e.preventDefault();
    deleteSelected();
  } else if (e.key === "Escape" && inSheet()) {
    setSheet(false);
  } else if (e.key === "Enter" && inSheet()) {
    setSheet(false);
  }
});

/** Vrai après un clic dans la colonne des miniatures, la bande ou la planche : les flèches
 * haut / bas passent alors d'une vignette à l'autre. Un clic dans le document l'annule. */
let thumbsFocused = false;
document.addEventListener("pointerdown", (e) => {
  const t = e.target as Element;
  if (t.closest(".menu, .modal-veil, .titlebar, .toolbar, .markup")) return;
  thumbsFocused = !!t.closest("#sidebar, #filmstrip, #sheet");
});

/** Nombre de vignettes par ligne sur la planche contact. */
function sheetColumns(): number {
  const grid = sheetItems.get(state.pages[state.current])?.parentElement;
  const items = grid ? [...grid.children] as HTMLElement[] : [];
  if (!items.length) return 1;
  const n = items.findIndex((el) => el.offsetTop !== items[0].offsetTop);
  return n > 0 ? n : items.length;
}

// Ctrl + molette (ou pincement du pavé tactile) : zoom autour de la page courante.
viewer.addEventListener(
  "wheel",
  (e) => {
    if (!(e.ctrlKey || e.metaKey) || !state.pages.length) return;
    e.preventDefault();
    zoomBy(Math.exp(-e.deltaY / 300));
  },
  { passive: false },
);

document.addEventListener("contextmenu", (e) => {
  if (!(e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement)) e.preventDefault();
});

// ---------- Fenêtre, glisser-déposer, fichiers au lancement ----------

if (api.inTauri) {
  const win = getCurrentWindow();
  $("wc-min").addEventListener("click", () => win.minimize());
  $("wc-max").addEventListener("click", () => win.toggleMaximize());
  $("wc-close").addEventListener("click", () => win.close());

  win.onCloseRequested(async (e) => {
    e.preventDefault();
    if (await confirmDiscard()) win.destroy();
  });

  // Fichiers glissés depuis l'Explorateur : dans la colonne des miniatures, ils
  // s'insèrent à l'endroit indiqué (fusion) ; ailleurs, ils remplacent ce qui est ouvert.
  const veil = $("drop-veil");
  const overSidebar = (x: number, y: number) => {
    const r = sidebar.getBoundingClientRect();
    return state.pages.length > 0 && !document.body.classList.contains("no-sidebar") &&
      x >= r.left && x <= r.right && y >= r.top && y <= r.bottom;
  };
  getCurrentWebview().onDragDropEvent((e) => {
    const t = e.payload.type;
    if (t === "leave") {
      veil.classList.remove("shown");
      hideDropLine();
      return;
    }
    const x = e.payload.position.x / devicePixelRatio;
    const y = e.payload.position.y / devicePixelRatio;
    const inSidebar = overSidebar(x, y);
    if (t === "enter" || t === "over") {
      veil.classList.toggle("shown", !inSidebar);
      if (inSidebar) showDropLine(insertionIndexAt(y));
      else hideDropLine();
    } else if (t === "drop") {
      veil.classList.remove("shown");
      hideDropLine();
      openFiles(e.payload.paths, inSidebar ? insertionIndexAt(y) : undefined);
    }
  });

  // Fichiers envoyés par Windows (double-clic, autre logiciel) : dans une fenêtre vide ils
  // s'ouvrent, sinon ils s'ajoutent à la fin (plusieurs photos ouvertes d'un coup).
  // Les uns après les autres : une rafale de fichiers ne doit pas se remplacer elle-même.
  let arriving = Promise.resolve();
  const takePaths = () =>
    (arriving = arriving.then(async () => {
      const paths = await api.takePaths();
      if (!paths.length) return;
      if (state.pages.length) await openFiles(paths, state.pages.length, !isModified());
      else {
        presetLayout(paths);
        await openFiles(paths);
      }
    }));
  api.apercuRapidePath().then((quick) => {
    if (quick) return startApercuRapide(quick);
    listen("paths-pending", takePaths);
    takePaths();
    api.apercuRapideEnabled().then((on) => (apercuRapideOption = on)).catch(() => {});
    getVersion().then((v) => (appVersion = v)).catch(() => {});
    if (!import.meta.env.DEV && win.label === "main") setTimeout(() => checkForUpdate(banner), 3000);
  });
}

window.addEventListener("blur", closeMenu);
forms.init(updateTitle);
document.getElementById("form-fill")!.addEventListener("click", () => {
  const field = document.querySelector<HTMLElement>(".form-layer .field:not(.read-only)");
  if (!field) return toast("Ce document n'a pas de champs à remplir.");
  field.scrollIntoView({ block: "center", behavior: "smooth" });
  field.focus({ preventScroll: true });
});
signatures.init();
film.init((path) => openFiles([path]));
pan.init();
annot.init({
  pages: () => state.pages,
  change,
  changed: updateTitle,
  cropChanged: showCropBar,
});
$("crop-ok").addEventListener("click", () => cropCurrent());
$("crop-cancel").addEventListener("click", () => annot.clearCrop());
refresh();
