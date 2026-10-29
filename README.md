# Documents

A free PDF and image viewer and light editor for Windows 10/11 and Windows Server.

> **Heads-up: this software is entirely vibe-coded.** All of the code was written by an AI
> (Claude), guided and tested by a human who isn't a developer. It is used every day, but
> treat it accordingly: keep copies of important documents.

## What it does
- Open PDFs and photos (JPG, PNG, WebP, GIF) and flip through pages with thumbnails.
- Zoom, rotate, search the text of a PDF.
- Annotate: highlight, shapes, arrows, text, notes, freehand drawing.
- Fill in PDF forms and sign them.
- Rearrange, delete, rotate and merge pages; export a smaller PDF.
- Touch up photos: crop, colors, resize, flip.
- Quick preview: select a file in File Explorer and press Space.
- Light and dark themes, following Windows.

The app follows the Windows display language: English, French, Spanish, Italian, Portuguese,
Chinese (Simplified) and Japanese. Any other language falls back to English.

## Install
1. Open **[Releases → Latest version](https://github.com/killef92/documents-windows/releases/tag/derniere-version)**.
2. Download `Documents-setup.exe` and run it.
3. Choose "Only for me" or "For all users" (administrator), then Install.

Documents then checks for new versions on its own and offers to install them.

## Uninstall
**Settings → Apps → Installed apps → Documents → Uninstall**
(or Control Panel → Programs and Features). Your files are never touched;
signatures saved in the app stay in `%LOCALAPPDATA%\fr.marouteau.apercu`
and can be deleted by hand.

## Privacy and security
- Documents sends nothing about you or your files. The only network access is the update check on GitHub. Details in [PRIVACY.md](PRIVACY.md).
- Found a security issue? Please report it privately, as explained in [SECURITY.md](SECURITY.md).
- Updates are signed and are only accepted if the signature matches.

## License
Free and open source under the [MIT license](LICENSE). Everything shipped with the app is free
to redistribute: see [LICENSES.md](LICENSES.md) and [THIRD-PARTY-LICENSES.txt](THIRD-PARTY-LICENSES.txt).

## Code signing policy
Free code signing provided by [SignPath.io](https://about.signpath.io), certificate by [SignPath Foundation](https://signpath.org) (application in progress).

- Author, reviewer and approver: [killef92](https://github.com/killef92) (Kilian Lefèvre).
- Every installer is built by GitHub Actions from this repository, then its signature is approved by hand.
- Privacy: the app sends no information to any other system, except checking GitHub for updates. See [PRIVACY.md](PRIVACY.md).

## For developers
Rust (Tauri 2) with the PDFium engine, and a TypeScript interface.
```
npm install
scripts/pdfium.sh linux-x64   # or win-x64
npm run tauri dev
```
The logic lives in `crates/core` (tested with `cargo test`), the app in `src-tauri`, the
interface in `ui/` (translations in `ui/src/locales/`). Windows installers are built by
GitHub Actions.
