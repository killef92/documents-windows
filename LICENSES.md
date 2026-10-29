# Licenses

Everything shipped in the app (setup .exe) is free to redistribute.
The full details, with the text of each license, are in `THIRD-PARTY-LICENSES.txt`
(regenerated at every build by `scripts/licences.py`, and copied into the install folder).

## What ships

| Item | Origin | License |
|---|---|---|
| App code and its translations | this repository | MIT (`LICENSE`) |
| App icon | drawn for this project | MIT (like the code) |
| File icons in File Explorer (PDF, JPG, PNG, WEBP, GIF), `scripts/icones-types.py` | drawn for this project; letters traced with the [Inter](https://rsms.me/inter/) font | MIT (icons); Inter: SIL OFL 1.1 (allows using its letters in images) |
| Interface icons | [Lucide](https://lucide.dev) | ISC (free, notice to keep) |
| Interface fonts: Geist and Geist Mono (`@fontsource-variable/geist`, `@fontsource-variable/geist-mono`) | [Vercel / The Geist Project Authors](https://github.com/vercel/geist-font) | SIL OFL 1.1 (free to embed and redistribute in an app; only selling the font on its own is forbidden) |
| Tauri, WRY, TAO and ~260 Rust libraries | crates.io | MIT, Apache-2.0, BSD, Zlib, Unicode, MPL-2.0 (see below) |
| PDFium (PDF reading), version chromium/8125 | Google, binaries from [pdfium-binaries](https://github.com/bblanchon/pdfium-binaries) | BSD-3-Clause / Apache-2.0; bundled libraries (FreeType, libjpeg-turbo, libpng, zlib, OpenJPEG, Little CMS, HarfBuzz, ICU, Abseil…): free licenses (FTL, IJG, BSD, zlib, MIT, Unicode, Apache), texts in `THIRD-PARTY-LICENSES.txt` |
| lopdf (rearranging pages without breaking forms) | J-F-Liu and contributors | MIT |
| `windows` (quick preview: space bar in File Explorer, Windows display language) and `tray-icon` (icon near the clock), Windows only | Microsoft, Tauri | MIT or Apache-2.0 |
| `tauri-plugin-single-instance` (a single copy of the app; files opened afterwards are passed to it) | Tauri | MIT or Apache-2.0 |
| `@tauri-apps/api`, `@tauri-apps/plugin-dialog` | npm | MIT or Apache-2.0 |
| Installer | NSIS | zlib (free) |
| WebView2 installer | Microsoft | freely redistributable (designed for this by Microsoft) |

**Check of 2026-10-29**: the 321 Rust libraries shipped on Windows, the 7 interface packages
and PDFium are all under licenses that allow redistribution (MIT, Apache-2.0, BSD,
ISC, Zlib, Unicode-3.0, OFL for the fonts, MPL-2.0). No GPL and no "non-commercial" license.
The translations (2026-10-29) add no dependency.

**MPL-2.0** (5 small CSS-parsing libraries): they can be redistributed freely;
the only obligation, if we changed *their* files, would be to publish those changes. We don't change them.

## What does NOT ship

- **No resource from another piece of software**: no icon, font or image taken from elsewhere.
  The "Ardoise" style has its own colors and fonts, and Lucide icons.
- **Fonts**: the app embeds Geist and Geist Mono (OFL, see above);
  if they were missing, it falls back to Segoe UI, which already comes with Windows. Chinese and
  Japanese text uses the fonts that come with Windows.
  The Inter font is only used to draw the letters of the file icons; it does not ship.

## Website

The presentation website (`docs/index.html`, published with GitHub Pages) uses only
the same free resources: Geist and Geist Mono (SIL OFL 1.1, text in `docs/assets/fonts/OFL.txt`),
Lucide icons (ISC, text in `docs/assets/LUCIDE-LICENSE.txt`) and the app icon (MIT). The window
shown on the page is drawn in HTML; the sample document is made up.

## Things to watch

- **App name**: "Documents" since version 0.1.29. The former name only remains in invisible
  internal names (app identifier, file types registered with Windows), kept so that settings,
  signatures and default-app choices already made are not lost.
- **HEIC** (later): `libheif` is LGPL and the HEVC format is covered by patents.
  To be handled at that point (for example, by going through the Windows HEIF decoder).
