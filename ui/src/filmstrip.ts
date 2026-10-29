// Bande de vignettes sous une image ouverte seule : les autres images de son dossier.
//
// Le dossier peut contenir des milliers de photos : seules les vignettes visibles (et un peu
// autour) existent dans la page, et leur image n'est demandée au cœur que quand le défilement
// marque une pause. Le cœur les fabrique en parallèle, les plus récentes d'abord.

import * as api from "./api";

/** Largeur d'une case (vignette de 64 px + écart) et marge aux extrémités. */
const SLOT = 72;
const PAD = 12;
/** Cases gardées de part et d'autre de la partie visible. */
const AROUND = 6;

const strip = document.getElementById("filmstrip")!;
const track = document.getElementById("film-track")!;

let files: string[] = [];
let folder = "";
let current = -1;
let selection = new Set<number>();
let anchor = -1;
const mounted = new Map<number, HTMLElement>();
let open: (path: string) => void = () => {};
/** L'image affichée reste au milieu tant que l'utilisateur n'a pas fait défiler la bande
 * (la largeur change encore pendant que la colonne de gauche se replie). */
let centering = false;

function center() {
  strip.scrollLeft = PAD + current * SLOT - (strip.clientWidth - 64) / 2;
}
let seq = 0;

const isWindows = navigator.userAgent.includes("Windows");
const same = (a: string, b: string) => (isWindows ? a.toLowerCase() === b.toLowerCase() : a === b);
const dirOf = (p: string) => p.replace(/[\\/][^\\/]*$/, "");
const nameOf = (p: string) => p.split(/[\\/]/).pop() ?? p;

export const visible = () => !strip.hidden;

/** Montre la bande pour cette image (et va chercher la liste si le dossier a changé). */
export async function show(path: string) {
  const mine = ++seq;
  if (!same(dirOf(path), folder) || !files.length) {
    let list: string[] = [];
    try {
      list = await api.folderImages(path);
    } catch {
      /* dossier illisible : pas de bande */
    }
    if (mine !== seq) return;
    folder = dirOf(path);
    files = list;
    selection = new Set();
    anchor = -1;
    for (const el of mounted.values()) el.remove();
    mounted.clear();
    track.style.width = `${PAD * 2 + files.length * SLOT - (SLOT - 64)}px`;
    current = -1;
  }
  const first = current < 0;
  current = files.findIndex((f) => same(f, path));
  // Seule dans son dossier : la bande ne servirait à rien.
  strip.hidden = files.length < 2 || current < 0;
  if (strip.hidden) return;
  // Après la mise en page (la bande vient peut-être d'apparaître) : l'image affichée au
  // milieu la première fois, ensuite seulement si elle sort de la partie visible.
  requestAnimationFrame(() => {
    const x = PAD + current * SLOT;
    centering = first || centering;
    if (centering) center();
    else if (x < strip.scrollLeft + PAD) strip.scrollLeft = x - PAD;
    else if (x + 64 > strip.scrollLeft + strip.clientWidth - PAD) strip.scrollLeft = x + 64 + PAD - strip.clientWidth;
    render();
  });
}

export function hide() {
  seq++;
  strip.hidden = true;
}

/** L'image d'à côté dans le dossier (-1 : la précédente, 1 : la suivante). */
export function neighbor(delta: number): string | null {
  if (strip.hidden || current < 0) return null;
  return files[current + delta] ?? null;
}

/** Les images choisies avec Ctrl ou Maj + clic, dans l'ordre du dossier. */
export function selectedPaths(): string[] {
  return [...selection].sort((a, b) => a - b).map((i) => files[i]);
}

export function clearSelection() {
  selection.clear();
  paint();
}

function paint() {
  for (const [i, el] of mounted) {
    el.classList.toggle("current", i === current);
    el.classList.toggle("selected", selection.has(i));
  }
}

function render() {
  if (strip.hidden) return;
  const from = Math.max(0, Math.floor((strip.scrollLeft - PAD) / SLOT) - AROUND);
  const to = Math.min(files.length - 1, Math.ceil((strip.scrollLeft + strip.clientWidth) / SLOT) + AROUND);
  for (const [i, el] of mounted) {
    if (i < from || i > to) {
      el.remove();
      mounted.delete(i);
    }
  }
  for (let i = from; i <= to; i++) {
    if (mounted.has(i)) continue;
    const b = document.createElement("button");
    b.className = "film-item";
    b.style.left = `${PAD + i * SLOT}px`;
    b.title = nameOf(files[i]);
    b.dataset.i = String(i);
    const img = document.createElement("img");
    img.alt = "";
    img.draggable = false;
    img.decoding = "async";
    img.addEventListener("load", () => img.classList.add("loaded"));
    // Demande abandonnée (défilement rapide) ou image illisible : on réessaiera plus tard.
    img.addEventListener("error", () => {
      img.removeAttribute("src");
      img.dataset.failed = String(Number(img.dataset.failed ?? 0) + 1);
      if (Number(img.dataset.failed) < 3) setTimeout(scheduleLoad, 500);
    });
    b.append(img);
    track.append(b);
    mounted.set(i, b);
  }
  paint();
  scheduleLoad();
}

let loadTimer = 0;
function scheduleLoad() {
  clearTimeout(loadTimer);
  loadTimer = window.setTimeout(() => {
    const size = Math.min(256, Math.ceil(96 * (devicePixelRatio || 1)));
    for (const [i, el] of mounted) {
      const img = el.firstElementChild as HTMLImageElement;
      if (!img.getAttribute("src") && Number(img.dataset.failed ?? 0) < 3) img.src = api.thumbUrl(files[i], size);
    }
  }, 70);
}

let frame = 0;
function schedule() {
  cancelAnimationFrame(frame);
  frame = requestAnimationFrame(render);
}

export function init(onOpen: (path: string) => void) {
  open = onOpen;
  strip.addEventListener("scroll", schedule);
  new ResizeObserver(() => {
    if (centering && current >= 0) center();
    schedule();
  }).observe(strip);
  for (const ev of ["wheel", "pointerdown", "keydown"]) strip.addEventListener(ev, () => (centering = false));
  // La molette fait défiler la bande de gauche à droite.
  strip.addEventListener(
    "wheel",
    (e) => {
      if (Math.abs(e.deltaY) <= Math.abs(e.deltaX)) return;
      e.preventDefault();
      strip.scrollLeft += e.deltaY;
    },
    { passive: false },
  );
  // Un clic ailleurs que sur une vignette (fond de la bande, photo…) annule le choix de
  // plusieurs images : il ne reste que l'image affichée. Clic droit et menus : on garde.
  document.addEventListener("mousedown", (e) => {
    if (!selection.size || e.button !== 0) return;
    const t = e.target as Element;
    if (t.closest(".film-item, .menu, .modal-veil")) return;
    selection.clear();
    anchor = -1;
    paint();
  });
  track.addEventListener("click", (e) => {
    const el = (e.target as Element).closest<HTMLElement>(".film-item");
    if (!el) return;
    const i = Number(el.dataset.i);
    if (e.ctrlKey || e.metaKey) {
      // Choisir plusieurs images (pour les copier) sans changer celle qui est affichée.
      if (!selection.size && current >= 0) selection.add(current);
      if (selection.has(i)) selection.delete(i);
      else selection.add(i);
      anchor = i;
    } else if (e.shiftKey) {
      const a = anchor >= 0 ? anchor : current;
      selection = new Set();
      for (let k = Math.min(a, i); k <= Math.max(a, i); k++) selection.add(k);
    } else {
      selection.clear();
      anchor = i;
      if (i !== current) open(files[i]);
    }
    paint();
  });
}
