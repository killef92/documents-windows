# Documents

A PDF and image viewer and light editor for Windows 10/11, written in Rust (Tauri 2).
Open, browse, annotate, sign, fill out forms, rearrange pages and touch up photos.

> **Heads-up: this software is entirely vibe-coded.** All of the code was written by an AI
> (Claude), guided and tested by a human who isn't a developer. It is used every day, but
> treat it accordingly: keep copies of important documents.

The app follows the Windows display language: English, French, Spanish, Italian, Portuguese,
Chinese (Simplified) and Japanese. Any other language falls back to English.

## Install
1. Open **[Releases → Latest version](https://github.com/killef92/documents-windows/releases/tag/derniere-version)**.
2. Download `Documents-setup.exe` and run it on the Windows PC (10 or 11, or Windows Server).
3. Choose "Only for me" or "For all users" (administrator), then Install.

Installed copies then offer to update themselves.

## Uninstall
**Settings → Apps → Installed apps → Documents → Uninstall**
(or Control Panel → Programs and Features). Your files are never touched;
signatures saved in the app stay in `%LOCALAPPDATA%\fr.marouteau.apercu`
(see [PRIVACY.md](PRIVACY.md)) and can be deleted by hand.

## Build the installer
**Actions → Installers → Run workflow** on `main`: the setup is published in
"Latest version", together with the file used for automatic updates.

## Automatic updates
Updates are signed: the secret key is stored in **Settings → Secrets and variables → Actions**
as `TAURI_SIGNING_PRIVATE_KEY`. Without it, the installers are still built,
but the automatic update is not published.

## Run in development
```
npm install
npm run tauri dev
```
To try another language: `DOCUMENTS_LANG=ja npm run tauri dev` (en, fr, es, it, pt, zh, ja).

## Repository layout
- `docs/` — specification and visual style.
- `crates/core` — the logic (no Tauri), tested with `cargo test`.
- `src-tauri` — the app.
- `ui/` — the interface (HTML, CSS, TypeScript, Lucide icons); translations in `ui/src/locales/`.
- `LICENSES.md` — what ships and under which license (everything is free to redistribute).
- `PRIVACY.md` — what the app keeps and sends (nothing about you or your files).
- `SECURITY.md` — report a security issue.

## Code signing policy
Free code signing provided by [SignPath.io](https://about.signpath.io), certificate by [SignPath Foundation](https://signpath.org) (application in progress).

- Author, reviewer and approver: [killef92](https://github.com/killef92) (Kilian Lefèvre).
- Every installer is built by GitHub Actions from this repository, then its signature is approved by hand.
- Privacy: the app sends no information to any other system, except checking GitHub for updates. See [PRIVACY.md](PRIVACY.md).
