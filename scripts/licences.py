"""Génère LICENCES-TIERS.txt : le texte de licence de chaque bibliothèque livrée dans l'appli.

Usage : python3 scripts/licences.py  (après `npm ci` et `cargo fetch`)
"""
import glob
import json
import os
import subprocess

RACINE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CIBLES = ["x86_64-pc-windows-msvc"]
MOTIFS = ["LICENSE*", "LICENCE*", "COPYING*", "NOTICE*", "UNLICENSE*", "license*"]


def textes(dossier):
    fichiers = sorted({f for m in MOTIFS for f in glob.glob(os.path.join(dossier, m)) if os.path.isfile(f)})
    return [(os.path.basename(f), open(f, encoding="utf-8", errors="replace").read().strip()) for f in fichiers]


paquets = {}
for cible in CIBLES:
    meta = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--format-version", "1", "--filter-platform", cible], cwd=RACINE))
    utilises = {n["id"] for n in meta["resolve"]["nodes"]}
    for p in meta["packages"]:
        if p["id"] in utilises and p["source"]:
            paquets[(p["name"], p["version"])] = (p["license"], os.path.dirname(p["manifest_path"]))

lock = json.load(open(os.path.join(RACINE, "package-lock.json"), encoding="utf-8"))
for chemin, p in lock["packages"].items():
    if chemin and not p.get("dev"):
        nom = chemin.split("node_modules/")[-1]
        paquets[(nom, p["version"])] = (p.get("license"), os.path.join(RACINE, chemin))

sortie = ["Documents — licences des bibliothèques tierces", "=" * 60, ""]
sans_texte = []
deja_vus = {}  # texte identique -> paquet où il est écrit en entier (évite des centaines de copies)
for (nom, version), (licence, dossier) in sorted(paquets.items()):
    sortie += ["-" * 60, f"{nom} {version} — {licence}", "-" * 60]
    t = textes(dossier)
    if not t:
        sans_texte.append(nom)
        sortie.append(f"Pas de fichier de licence fourni : texte standard de « {licence} » en fin de document.")
    for _, contenu in t:
        if contenu in deja_vus:
            sortie.append(f"(Texte identique à celui de {deja_vus[contenu]}, ci-dessus.)")
        else:
            deja_vus[contenu] = f"{nom} {version}"
            sortie += [contenu, ""]
    sortie.append("")

# PDFium, livré à côté de l'appli (voir scripts/pdfium.sh), et les bibliothèques qu'il contient
# (FreeType, libjpeg-turbo, libpng, zlib, OpenJPEG…), chacune avec son texte.
for f in sorted(glob.glob(os.path.join(RACINE, "vendor", "pdfium", "*", "LICENSE"))):
    sortie += ["-" * 60, f"PDFium ({os.path.basename(os.path.dirname(f))}) — BSD-3-Clause / Apache-2.0", "-" * 60,
               open(f, encoding="utf-8", errors="replace").read().strip(), ""]
    for g in sorted(glob.glob(os.path.join(os.path.dirname(f), "licenses", "*"))):
        sortie += ["-" * 60, f"Contenu dans PDFium : {os.path.basename(g)}", "-" * 60,
                   open(g, encoding="utf-8", errors="replace").read().strip(), ""]
    break

sortie += ["=" * 60, "Textes standard", "=" * 60, ""]
for f in sorted(glob.glob(os.path.join(RACINE, "scripts", "licences-standard", "*.txt"))):
    sortie += ["-" * 60, os.path.basename(f)[:-4], "-" * 60, open(f, encoding="utf-8").read().strip(), ""]

open(os.path.join(RACINE, "LICENCES-TIERS.txt"), "w", encoding="utf-8").write("\n".join(sortie))
print(f"{len(paquets)} bibliothèques, {len(sans_texte)} sans fichier de licence : {', '.join(sans_texte)}")
