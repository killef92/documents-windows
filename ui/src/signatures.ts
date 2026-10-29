// Signatures : on les crée une fois (dessin à la souris ou photo d'une signature sur papier),
// elles restent dans l'appli, et un clic les pose sur la page.

import { open as openDialog } from "@tauri-apps/plugin-dialog";
import * as api from "./api";
import { placeSignature, type Signature } from "./annotations";
import { closeMenu, showPanel } from "./menu";
import { t as tr, translatePage } from "./i18n";

const KEY = "apercu.signatures";
type Pt = { x: number; y: number };

function load(): Signature[] {
  try {
    const list = JSON.parse(localStorage.getItem(KEY) ?? "[]");
    return Array.isArray(list) ? list : [];
  } catch {
    return [];
  }
}

function store(list: Signature[]) {
  try {
    localStorage.setItem(KEY, JSON.stringify(list));
  } catch {
    // Stockage indisponible : la signature reste utilisable jusqu'à la fermeture.
  }
}

let signatures = load();

const NS = "http://www.w3.org/2000/svg";

/** Petit aperçu d'une signature, pour le menu. */
function preview(sig: Signature): Element {
  if (sig.kind === "image") {
    const img = document.createElement("img");
    img.src = sig.png;
    img.alt = "";
    return img;
  }
  const s = document.createElementNS(NS, "svg");
  s.setAttribute("viewBox", `0 0 ${sig.aspect * 100} 100`);
  for (const st of sig.strokes) {
    const path = document.createElementNS(NS, "path");
    path.setAttribute("d", st.map((p, i) => `${i ? "L" : "M"}${(p.x * sig.aspect * 100).toFixed(1)} ${(p.y * 100).toFixed(1)}`).join(""));
    s.append(path);
  }
  return s;
}

/** Le menu du bouton « Signer » : les signatures enregistrées, puis « Créer une signature… ». */
function showSignMenu(button: HTMLElement) {
  const box = document.createElement("div");
  box.className = "sign-menu";
  if (!signatures.length) {
    const empty = document.createElement("div");
    empty.className = "sign-empty";
    empty.textContent = tr("noSignatures");
    box.append(empty);
  }
  for (const sig of signatures) {
    const row = document.createElement("div");
    row.className = "sign-row";
    const pick = document.createElement("button");
    pick.className = "sign-pick";
    pick.title = tr("signPickTitle");
    pick.append(preview(sig));
    pick.addEventListener("click", () => {
      closeMenu();
      placeSignature(sig);
    });
    const del = document.createElement("button");
    del.className = "sign-delete";
    del.title = tr("deleteSignature");
    del.textContent = "×";
    del.addEventListener("click", () => {
      signatures = signatures.filter((s) => s.id !== sig.id);
      store(signatures);
      row.remove();
    });
    row.append(pick, del);
    box.append(row);
  }
  const create = document.createElement("button");
  create.className = "menu-item sign-create";
  create.textContent = tr("createSignature");
  create.addEventListener("click", () => {
    closeMenu();
    createDialog();
  });
  box.append(create);
  const r = button.getBoundingClientRect();
  showPanel(r.left, r.bottom + 6, box);
}

/** Ramène des traits dans un cadre de 0 à 1, en gardant leurs proportions. */
function normalize(strokes: Pt[][]): Signature | null {
  const all = strokes.flat();
  if (!all.length) return null;
  const pad = 4;
  const x0 = Math.min(...all.map((p) => p.x)) - pad;
  const y0 = Math.min(...all.map((p) => p.y)) - pad;
  const w = Math.max(...all.map((p) => p.x)) + pad - x0;
  const h = Math.max(...all.map((p) => p.y)) + pad - y0;
  return {
    id: `${Date.now()}`,
    kind: "ink",
    aspect: w / h,
    strokes: strokes.map((st) => st.map((p) => ({ x: +((p.x - x0) / w).toFixed(4), y: +((p.y - y0) / h).toFixed(4) }))),
  };
}

function aspectOf(dataUrl: string): Promise<number> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => resolve(img.naturalWidth / img.naturalHeight);
    img.onerror = reject;
    img.src = dataUrl;
  });
}

/** La fenêtre « Créer une signature » : dessiner à la souris, ou partir d'une photo. */
function createDialog() {
  const veil = document.createElement("div");
  veil.className = "modal-veil";
  const box = document.createElement("div");
  box.className = "modal sign-dialog";
  box.innerHTML = `
    <div class="modal-title" data-i18n="createSignatureTitle"></div>
    <div class="sign-tabs" role="tablist">
      <button class="sign-tab active" data-tab="draw" data-i18n="signDraw"></button>
      <button class="sign-tab" data-tab="photo" data-i18n="signPhoto"></button>
    </div>
    <div class="sign-pane" data-pane="draw">
      <canvas class="sign-canvas"></canvas>
      <div class="sign-hint" data-i18n="signHint"></div>
    </div>
    <div class="sign-pane" data-pane="photo" hidden>
      <div class="sign-photo"><span data-i18n="signPhotoHint"></span></div>
      <div class="sign-hint sign-error"></div>
    </div>
    <div class="modal-buttons">
      <button class="modal-button" data-act="clear" data-i18n="clear"></button>
      <span class="sign-spacer"></span>
      <button class="modal-button" data-act="cancel" data-i18n="cancel"></button>
      <button class="modal-button primary" data-act="ok" data-i18n="save" disabled></button>
    </div>`;
  translatePage(box);
  veil.append(box);
  document.body.append(veil);

  const q = <T extends Element>(sel: string) => box.querySelector(sel) as T;
  const canvas = q<HTMLCanvasElement>(".sign-canvas");
  const ok = q<HTMLButtonElement>('[data-act="ok"]');
  const clear = q<HTMLButtonElement>('[data-act="clear"]');
  const photoBox = q<HTMLElement>(".sign-photo");
  const error = q<HTMLElement>(".sign-error");
  let tab: "draw" | "photo" = "draw";
  let strokes: Pt[][] = [];
  let photo: string | null = null;

  const ratio = devicePixelRatio || 1;
  const CW = 440;
  const CH = 180;
  canvas.width = CW * ratio;
  canvas.height = CH * ratio;
  canvas.style.width = `${CW}px`;
  canvas.style.height = `${CH}px`;
  const ctx = canvas.getContext("2d")!;
  ctx.scale(ratio, ratio);

  const ready = () => {
    ok.disabled = tab === "draw" ? !strokes.length : !photo;
  };
  const redraw = () => {
    ctx.clearRect(0, 0, CW, CH);
    ctx.strokeStyle = getComputedStyle(canvas).color;
    ctx.lineWidth = 2.2;
    ctx.lineCap = ctx.lineJoin = "round";
    for (const st of strokes) {
      ctx.beginPath();
      st.forEach((p, i) => (i ? ctx.lineTo(p.x, p.y) : ctx.moveTo(p.x, p.y)));
      if (st.length === 1) ctx.lineTo(st[0].x + 0.1, st[0].y);
      ctx.stroke();
    }
  };
  const at = (e: PointerEvent): Pt => {
    const r = canvas.getBoundingClientRect();
    return { x: e.clientX - r.left, y: e.clientY - r.top };
  };
  canvas.addEventListener("pointerdown", (e) => {
    e.preventDefault();
    canvas.setPointerCapture(e.pointerId);
    strokes = [...strokes, [at(e)]];
    redraw();
  });
  canvas.addEventListener("pointermove", (e) => {
    if (!canvas.hasPointerCapture(e.pointerId)) return;
    const p = at(e);
    const st = strokes[strokes.length - 1];
    const last = st[st.length - 1];
    if (Math.hypot(p.x - last.x, p.y - last.y) < 1.5) return;
    st.push(p);
    redraw();
  });
  canvas.addEventListener("pointerup", ready);

  const showTab = (t: "draw" | "photo") => {
    tab = t;
    for (const b of box.querySelectorAll<HTMLElement>(".sign-tab")) b.classList.toggle("active", b.dataset.tab === t);
    for (const p of box.querySelectorAll<HTMLElement>(".sign-pane")) p.hidden = p.dataset.pane !== t;
    clear.textContent = tr(t === "draw" ? "clear" : "chooseImage");
    ready();
  };
  for (const b of box.querySelectorAll<HTMLElement>(".sign-tab")) {
    b.addEventListener("click", () => {
      showTab(b.dataset.tab as "draw" | "photo");
      if (b.dataset.tab === "photo" && !photo) choosePhoto();
    });
  }

  async function choosePhoto() {
    if (!api.inTauri) return;
    const path = await openDialog({ filters: [{ name: tr("filterImages"), extensions: ["jpg", "jpeg", "png", "bmp", "gif", "tif", "tiff", "webp"] }] });
    if (!path || Array.isArray(path)) return;
    error.textContent = tr("readingSignature");
    try {
      photo = await api.signatureFromPhoto(path);
      const img = document.createElement("img");
      img.src = photo;
      img.alt = "";
      photoBox.replaceChildren(img);
      error.textContent = tr("paperTransparent");
    } catch (e) {
      photo = null;
      error.textContent = tr("imageUnusable", { error: String(e) });
    }
    ready();
  }

  const close = () => {
    veil.remove();
    document.removeEventListener("keydown", onKey, true);
  };
  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      e.stopPropagation();
      close();
    } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "z" && tab === "draw") {
      e.preventDefault();
      e.stopPropagation();
      strokes = strokes.slice(0, -1);
      redraw();
      ready();
    }
  };
  document.addEventListener("keydown", onKey, true);

  clear.addEventListener("click", () => {
    if (tab === "photo") return void choosePhoto();
    strokes = [];
    redraw();
    ready();
  });
  q<HTMLButtonElement>('[data-act="cancel"]').addEventListener("click", close);
  ok.addEventListener("click", async () => {
    let sig: Signature | null = null;
    if (tab === "draw") sig = normalize(strokes);
    else if (photo) sig = { id: `${Date.now()}`, kind: "image", png: photo, aspect: await aspectOf(photo) };
    if (!sig) return;
    signatures = [...signatures, sig];
    store(signatures);
    close();
    // Prête à poser : le prochain clic sur la page la place.
    placeSignature(sig);
  });
}

export function init() {
  const button = document.getElementById("tool-sign")!;
  button.addEventListener("click", () => showSignMenu(button));
}
