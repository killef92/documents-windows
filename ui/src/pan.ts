// Déplacer un document zoomé en l'attrapant à la souris (main qui saisit la page).

const viewer = document.getElementById("viewer")!;

/** Le document dépasse de la zone d'affichage : il y a quelque chose à déplacer. */
export const canPan = () => viewer.scrollWidth > viewer.clientWidth + 1 || viewer.scrollHeight > viewer.clientHeight + 1;

/** Suit la souris jusqu'au relâchement : le document bouge avec elle. */
export function startPan(e: PointerEvent) {
  e.preventDefault();
  const x0 = e.clientX;
  const y0 = e.clientY;
  const left = viewer.scrollLeft;
  const top = viewer.scrollTop;
  document.body.classList.add("panning");
  const move = (ev: PointerEvent) => {
    viewer.scrollLeft = left - (ev.clientX - x0);
    viewer.scrollTop = top - (ev.clientY - y0);
  };
  const end = () => {
    document.body.classList.remove("panning");
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", end);
    window.removeEventListener("pointercancel", end);
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", end);
  window.addEventListener("pointercancel", end);
}

export function init() {
  // Avant les outils d'annotation : bouton du milieu partout, ou clic sur le fond gris.
  viewer.addEventListener(
    "pointerdown",
    (e) => {
      if (!canPan()) return;
      const t = e.target as Element;
      const onBackground = e.button === 0 && !t.closest(".page");
      if (e.button === 1 || onBackground) {
        e.stopPropagation();
        startPan(e);
      }
    },
    { capture: true },
  );
}
