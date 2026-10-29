#!/usr/bin/env bash
# Télécharge PDFium (moteur PDF de Chrome, licence BSD/Apache) dans vendor/pdfium/<plateforme>/.
# Usage : scripts/pdfium.sh win-x64 | linux-x64
# Binaires : https://github.com/bblanchon/pdfium-binaries
set -euo pipefail
VERSION="8125"
PLATFORM="${1:?plateforme attendue : win-x64 ou linux-x64}"
DEST="$(cd "$(dirname "$0")/.." && pwd)/vendor/pdfium/$PLATFORM"
mkdir -p "$DEST"
TMP="$(mktemp -d)"
curl -fsSL "https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F$VERSION/pdfium-$PLATFORM.tgz" -o "$TMP/pdfium.tgz"
tar -xzf "$TMP/pdfium.tgz" -C "$TMP"
cp "$TMP"/bin/pdfium.dll "$DEST/" 2>/dev/null || cp "$TMP"/lib/libpdfium.* "$DEST/"
cp "$TMP/LICENSE" "$DEST/LICENSE" 2>/dev/null || echo "PDFium : licence BSD-3-Clause / Apache-2.0 — https://pdfium.googlesource.com/pdfium/+/main/LICENSE" > "$DEST/LICENSE"
# Licences des bibliothèques contenues dans PDFium (reprises dans THIRD-PARTY-LICENSES.txt).
rm -rf "$DEST/licenses" && cp -r "$TMP/licenses" "$DEST/licenses" 2>/dev/null || true
rm -rf "$TMP"
ls -l "$DEST"
