#!/usr/bin/env python3
"""Fabrique les icônes des fichiers dans l'Explorateur (une par type, marquée de son extension).

Une feuille blanche au coin replié, avec un bandeau de couleur portant l'extension.
Dessinées à chaque taille (de 16 à 256 px) pour rester nettes ; sous 32 px le texte serait
illisible : la feuille garde seulement son bandeau de couleur.

Police : Inter (SIL Open Font License), seulement pour dessiner les lettres dans les images.
Usage : python3 scripts/icones-types.py  →  src-tauri/icons/types/*.ico
"""

from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

FONT = "/usr/share/fonts/opentype/inter/Inter-ExtraBold.otf"
OUT = Path(__file__).resolve().parent.parent / "src-tauri" / "icons" / "types"
SIZES = [16, 20, 24, 32, 40, 48, 64, 96, 128, 256]
SS = 4  # suréchantillonnage, pour des bords doux

TYPES = {
    "pdf": ("PDF", (229, 50, 45)),
    "jpg": ("JPG", (40, 120, 220)),
    "png": ("PNG", (36, 160, 80)),
    "webp": ("WEBP", (140, 90, 215)),
    "gif": ("GIF", (225, 60, 140)),
    "image": ("IMG", (240, 140, 0)),
}


def draw(size: int, label: str, color: tuple[int, int, int]) -> Image.Image:
    s = size * SS
    im = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    # La feuille : un peu plus haute que large, comme une page.
    w = s * 0.74
    x0 = (s - w) / 2
    x1 = x0 + w
    y0 = s * 0.04
    y1 = s * 0.96
    fold = w * 0.30
    line = max(SS, s * 0.016)
    page = [(x0, y0), (x1 - fold, y0), (x1, y0 + fold), (x1, y1), (x0, y1)]
    d.polygon(page, fill=(255, 255, 255, 255), outline=(150, 150, 150, 255), width=round(line))
    # Le coin replié.
    d.polygon([(x1 - fold, y0), (x1 - fold, y0 + fold), (x1, y0 + fold)], fill=(222, 222, 222, 255), outline=(150, 150, 150, 255), width=round(line))
    # Le bandeau de l'extension : déborde un peu de la feuille à gauche, comme une étiquette.
    small = size < 32
    bh = s * (0.30 if small else 0.27)
    by1 = y1 - s * (0.10 if small else 0.12)
    by0 = by1 - bh
    bx0 = x0 - (0 if small else s * 0.06)
    bx1 = x1 - (0 if small else s * 0.04)
    d.rounded_rectangle([bx0, by0, bx1, by1], radius=s * 0.04, fill=color + (255,))
    if not small:
        # Le plus grand texte qui tient dans le bandeau.
        pt = int(bh * 0.72)
        while pt > 4:
            font = ImageFont.truetype(FONT, pt)
            l, t, r, b = d.textbbox((0, 0), label, font=font)
            if r - l <= (bx1 - bx0) * 0.86 and b - t <= bh * 0.62:
                break
            pt -= SS
        l, t, r, b = d.textbbox((0, 0), label, font=font)
        tx = (bx0 + bx1) / 2 - (l + r) / 2
        ty = (by0 + by1) / 2 - (t + b) / 2
        d.text((tx, ty), label, font=font, fill=(255, 255, 255, 255))
    else:
        # Petite taille : deux traits gris pour évoquer le contenu de la page.
        for k in range(2):
            ly = y0 + fold + s * 0.08 + k * s * 0.12
            d.rectangle([x0 + w * 0.18, ly, x1 - w * 0.18, ly + s * 0.05], fill=(190, 190, 190, 255))
    return im.resize((size, size), Image.LANCZOS)


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    for name, (label, color) in TYPES.items():
        images = [draw(sz, label, color) for sz in SIZES]
        big = images[-1]
        big.save(OUT / f"{name}.ico", sizes=[(sz, sz) for sz in SIZES], append_images=images[:-1])
    print("icônes écrites dans", OUT)


if __name__ == "__main__":
    main()
