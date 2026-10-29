// Petits éléments d'interface réutilisables : menu contextuel et fenêtre de question.

export type MenuItem =
  | { label: string; shortcut?: string; disabled?: boolean; checked?: boolean; action: () => void }
  | "-";

let openMenu: HTMLElement | null = null;
/** Le bouton qui a ouvert le menu : un nouveau clic dessus le referme au lieu de le rouvrir. */
let openAnchor: Element | null = null;
let blocked: Element | null = null;

export function closeMenu() {
  openMenu?.remove();
  openMenu = null;
  openAnchor = null;
}

/** Le bouton cliqué qui demande ce menu, s'il y en a un. */
function clickAnchor(): Element | null {
  const ev = window.event;
  return ev?.type === "click" && ev.currentTarget instanceof Element ? ev.currentTarget : null;
}

/** Vrai si ce clic vient de refermer le menu de ce même bouton : ne pas le rouvrir. */
function justClosed(anchor: Element | null) {
  const skip = !!anchor && anchor === blocked;
  blocked = null;
  return skip;
}

/** Affiche un menu à la position donnée (clic droit, ou sous un bouton). */
export function showMenu(x: number, y: number, items: MenuItem[]) {
  const anchor = clickAnchor();
  if (justClosed(anchor)) return;
  closeMenu();
  const menu = document.createElement("div");
  menu.className = "menu";
  if (items.some((i) => i !== "-" && i.checked !== undefined)) menu.classList.add("checks");
  for (const item of items) {
    if (item === "-") {
      const sep = document.createElement("div");
      sep.className = "menu-sep";
      menu.append(sep);
      continue;
    }
    const b = document.createElement("button");
    b.className = "menu-item" + (item.checked ? " checked" : "");
    b.disabled = !!item.disabled;
    b.innerHTML = `<span></span><span class="menu-shortcut"></span>`;
    b.children[0].textContent = item.label;
    b.children[1].textContent = item.shortcut ?? "";
    b.addEventListener("click", () => {
      closeMenu();
      item.action();
    });
    menu.append(b);
  }
  place(menu, x, y, anchor);
}

/** Affiche un petit panneau libre (palette de couleurs…) à la place d'un menu. */
export function showPanel(x: number, y: number, content: HTMLElement) {
  const anchor = clickAnchor();
  if (justClosed(anchor)) return;
  closeMenu();
  const menu = document.createElement("div");
  menu.className = "menu panel";
  menu.append(content);
  place(menu, x, y, anchor);
}

function place(menu: HTMLElement, x: number, y: number, anchor: Element | null) {
  document.body.append(menu);
  const r = menu.getBoundingClientRect();
  menu.style.left = `${Math.max(8, Math.min(x, innerWidth - r.width - 8))}px`;
  menu.style.top = `${Math.min(y, innerHeight - r.height - 8)}px`;
  openMenu = menu;
  openAnchor = anchor;
}

document.addEventListener("mousedown", (e) => {
  blocked = null;
  if (openMenu && !openMenu.contains(e.target as Node)) {
    if (openAnchor?.contains(e.target as Node)) blocked = openAnchor;
    closeMenu();
  }
});
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") closeMenu();
});
window.addEventListener("blur", closeMenu);

/** Pose une question avec plusieurs boutons ; renvoie l'index du bouton choisi (ou -1 si Échap). */
export function ask(title: string, text: string, buttons: string[], primary = 0, content?: HTMLElement): Promise<number> {
  return new Promise((resolve) => {
    const veil = document.createElement("div");
    veil.className = "modal-veil";
    const box = document.createElement("div");
    box.className = "modal alert";
    const h = document.createElement("div");
    h.className = "modal-title";
    h.textContent = title;
    const p = document.createElement("div");
    p.className = "modal-text";
    p.textContent = text;
    const row = document.createElement("div");
    row.className = "modal-buttons stacked";
    const done = (i: number) => {
      veil.remove();
      document.removeEventListener("keydown", onKey, true);
      resolve(i);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        done(-1);
      }
    };
    buttons.forEach((label, i) => {
      const b = document.createElement("button");
      b.className = "modal-button" + (i === primary ? " primary" : "");
      b.textContent = label;
      b.addEventListener("click", () => done(i));
      row.append(b);
    });
    box.append(h, ...(text ? [p] : []), ...(content ? [content] : []), row);
    if (content) {
      // Un formulaire : boutons côte à côte, à droite (Annuler, OK).
      box.classList.add("form");
      row.classList.remove("stacked");
    }
    veil.append(box);
    document.body.append(veil);
    document.addEventListener("keydown", onKey, true);
    if (content) {
      content.addEventListener("keydown", (e) => {
        if (e.key === "Enter") done(primary);
      });
      content.querySelector<HTMLElement>("input")?.focus();
    } else (row.children[primary] as HTMLElement)?.focus();
  });
}
