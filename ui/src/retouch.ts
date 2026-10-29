// Retouche des photos : fenêtre « Ajuster la couleur » (luminosité, contraste, saturation).
//
// Pendant les réglages, l'image affichée est seulement filtrée (filtres CSS) ; « Appliquer »
// demande au cœur la vraie image retouchée (picture.rs, mêmes formules que les filtres CSS).

import type { Retouch } from "./api";

export interface RetouchPage {
  el: HTMLElement;
  thumbBox: HTMLElement;
}

type Setting = "brightness" | "contrast" | "saturation";

const SETTINGS: [Setting, string, number][] = [
  // Nom, libellé, réglage maximal (le curseur va de -max à +max).
  ["brightness", "Luminosité", 0.6],
  ["contrast", "Contraste", 0.6],
  ["saturation", "Saturation", 1],
];

/** Le filtre CSS qui montre le même résultat que le cœur. */
export function cssFilter(r: Retouch): string {
  const b = 1 + (r.brightness ?? 0);
  const c = 1 + (r.contrast ?? 0);
  const s = 1 + (r.saturation ?? 0);
  return `brightness(${b}) contrast(${c}) saturate(${s})`;
}

function preview(p: RetouchPage, r: Retouch | null) {
  for (const el of [p.el, p.thumbBox]) {
    if (r) el.style.setProperty("--retouch", cssFilter(r));
    else el.style.removeProperty("--retouch");
  }
}

let panel: HTMLElement | null = null;
let closePanel: () => void = () => {};

export const isOpen = () => panel !== null;

/** Ouvre la fenêtre de réglages sous le bouton, pour la photo donnée. */
export function open(button: HTMLElement, p: RetouchPage, apply: (r: Retouch) => Promise<void>) {
  closePanel();
  const values: Record<Setting, number> = { brightness: 0, contrast: 0, saturation: 0 };
  const box = document.createElement("div");
  box.className = "menu panel retouch-panel";
  const title = document.createElement("div");
  title.className = "retouch-title";
  title.textContent = "Ajuster la couleur";
  box.append(title);
  const sliders: HTMLInputElement[] = [];
  for (const [key, label, max] of SETTINGS) {
    const row = document.createElement("label");
    row.className = "retouch-row";
    const name = document.createElement("span");
    name.textContent = label;
    const input = document.createElement("input");
    input.type = "range";
    input.min = "-100";
    input.max = "100";
    input.value = "0";
    input.addEventListener("input", () => {
      values[key] = (Number(input.value) / 100) * max;
      preview(p, values);
    });
    // Double-clic sur un curseur : retour à zéro.
    input.addEventListener("dblclick", () => {
      input.value = "0";
      input.dispatchEvent(new Event("input"));
    });
    sliders.push(input);
    row.append(name, input);
    box.append(row);
  }
  const buttons = document.createElement("div");
  buttons.className = "modal-buttons";
  const reset = document.createElement("button");
  reset.className = "modal-button";
  reset.textContent = "Réinitialiser";
  const spacer = document.createElement("span");
  spacer.style.flex = "1";
  const cancel = document.createElement("button");
  cancel.className = "modal-button";
  cancel.textContent = "Annuler";
  const ok = document.createElement("button");
  ok.className = "modal-button primary";
  ok.textContent = "Appliquer";
  buttons.append(reset, spacer, cancel, ok);
  box.append(buttons);

  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      e.stopPropagation();
      closePanel();
    }
  };
  closePanel = () => {
    preview(p, null);
    box.remove();
    panel = null;
    document.removeEventListener("keydown", onKey, true);
    closePanel = () => {};
  };
  reset.addEventListener("click", () => {
    for (const s of sliders) s.value = "0";
    values.brightness = values.contrast = values.saturation = 0;
    preview(p, null);
  });
  cancel.addEventListener("click", () => closePanel());
  ok.addEventListener("click", async () => {
    if (!values.brightness && !values.contrast && !values.saturation) return closePanel();
    ok.disabled = true;
    try {
      await apply({ ...values });
    } finally {
      closePanel();
    }
  });
  document.addEventListener("keydown", onKey, true);

  document.body.append(box);
  const r = button.getBoundingClientRect();
  const w = box.getBoundingClientRect().width;
  box.style.left = `${Math.max(8, Math.min(r.right - w, innerWidth - w - 8))}px`;
  box.style.top = `${r.bottom + 6}px`;
  panel = box;
}

export function close() {
  closePanel();
}
