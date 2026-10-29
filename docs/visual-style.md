# Visual style — "Ardoise"

References: `docs/maquettes/ardoise-sombre.png` (dark) and `docs/maquettes/ardoise-clair.png` (light), approved by Kilian on 2026-10-28.
The layout is classic (toolbar at the top, thumbnails on the left, document in the middle), with a look of our own.
Golden rule: **sober, calm, the document is the star**. Blue-gray, a single bright color (mint), thin icons.

## Window structure
1. **Window bar** (~38 px): only the **document name in the middle**, followed by "Page 1 of 6" (separated by a thin line), and Minimize / Maximize / Close on the right. Clicking the name opens the File menu.
2. **Toolbar** (~56 px), right below: thumbnails button on the left · **all tools grouped in a single centered "island"** · "Search" field on the right (with the "Ctrl F" reminder).
3. **Markup bar** (~40 px): appears below the toolbar when you click "Annotate".
4. **Thumbnail column** on the left (~168 px), can be hidden. Inspector (Ctrl+I) on the right, ~272 px.
5. **Document area**: very dark background (or light blue-gray), white page centered with a soft shadow.

## Colors
The theme follows the Windows light / dark setting. Exact values: top of `ui/styles.css`.

| Role | Dark | Light |
|---|---|---|
| Window bar | `#0F1115` | `#EBEDF1` |
| Toolbar, thumbnail column | `#14161B` | `#F5F6F8` |
| Tool island, search field | `#1D2027` | `#FFFFFF` (thin border) |
| Area around the document | `#0D0F13` | `#E9ECF0` |
| Main text | `#E6E8EC` | `#1A1D23` |
| Icons | `#B4B9C3` | `#4A505B` |
| Secondary text | `#7A818E` | `#737A86` |
| **Mint accent** (selection, Annotate button, primary buttons) | `#5FD8A8` (text on it: `#0D1A14`) | `#139A6B` (text on it: white) |
| Separators | `rgba(255,255,255,0.07)` | `rgba(20,28,40,0.08)` |

No system blue: the accent is always mint.

## Typography
- Font: **Geist** (free, OFL, embedded in the app); fallback: Segoe UI.
- Figures and markers ("Page 1 of 6", thumbnail numbers, "Ctrl F"): **Geist Mono**.
- Title: 13 px, semibold (600). Subtitle: 11 px, mono, secondary gray.
- Numbers under thumbnails: `01`, `02`… in mono 11 px, gray; mint when selected.
- Chinese and Japanese: the Windows fonts for those languages take over automatically (same sizes).

## Texts and languages
- Every interface text lives in `ui/src/locales/` (English is the reference, `en.ts`); the build fails if a language misses a text.
- Keep labels short: they must fit in Spanish and Italian, which are often longer than English.

## Buttons and icons
- **Thin-line icons** (1.5 px), monochrome, ~18 px: Lucide.
- Button groups have no background: they sit in the island, separated by a thin vertical line.
- "Annotate": rounded square tinted mint; solid mint when the markup bar is open.
- Hover: slightly lighter background. No hard borders.
- Small "⌄" arrow next to buttons that open a menu.

## Thumbnails
- Selection: **mint bar on the left**, full-opacity thumbnail with a shadow; the others are slightly faded (full on hover).
- 4 px rounded corners. Under the thumbnail: page number (PDF) or file name shortened in the middle (`R261003…668_.jpg`).

## Window
- Rounded corners (Windows 11 does it by itself).

## Animations
- Short (150–200 ms), gentle. The markup bar slides open / closed.
- Nothing that blinks or bounces.
