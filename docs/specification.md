# Specification — "Documents"

## The idea in one sentence
A fast, good-looking Windows app that opens PDFs and photos, lets you browse them, annotate them, sign them and rearrange pages.

## Who it's for
Me first (professional use): quotes, invoices, French administrative forms (CERFA), building-site photos, documents to sign.

## What the app must do — in stages

### Stage 1 — The viewer (the core)
- Open a PDF or an image (JPG, PNG, HEIC, WebP) by double-click or drag and drop.
- Open **several files in a single window** (like my 3 building-site photos: "3 documents, 3 pages in total").
- **Thumbnail column on the left**, the page or photo shown large on the right.
- Zoom +/–, actual size, fit to window.
- **Rotate** a page or a photo.
- **Search** text in a PDF.
- Title bar: file name + "Page 1 of 16".
- Near-instant opening (goal: under 1 second for a 20-page PDF).

### Stage 2 — Managing pages
- Drag thumbnails to **reorder** pages.
- **Delete** a page.
- **Merge**: drag a PDF into the thumbnail column to add it.
- Extract a page into a new PDF.
- Save / Save As / Export an image as PDF.

### Stage 3 — Annotating
- Highlight text.
- Rectangles, arrows, circles (with line and fill color).
- Text box (font and size).
- Note (small sticky note).
- Freehand drawing.

### Stage 4 — Signing and filling out
- Create a signature (drawn with the mouse or from a photo) and reuse it.
- Fill out PDF form fields (CERFA).

### Stage 5 — The extras
- Simple photo touch-ups (brightness, cropping).
- Reduce a PDF's size before sending it by email.
- Quick preview with the space bar from Windows File Explorer.

### Across all stages — Languages
- English by default; also French, Spanish, Italian, Portuguese, Chinese (Simplified) and Japanese.
- The language is the Windows display language, without asking anything; English for any other language.

## What we DON'T do
- No rewriting of a PDF's text (too complex, not useful).
- No account, no subscription, no cloud: everything stays on the PC.

## How we know it worked
- I stop opening other PDF tools for everyday work.
- A CERFA form opens, gets filled out, signed and saved without bugs.
- The app looks like the mockups in `docs/maquettes/`.
