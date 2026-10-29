"""Écrit latest.json : la fiche que l'appli lit pour savoir s'il existe une nouvelle version.

Usage : python3 scripts/latest_json.py <dossier des signatures> <release.json> <numéro> <sortie>

release.json : la release « derniere-version » telle que la donne l'API GitHub. Dépôt privé
(DEPOT_PRIVE=true) : chaque fichier est désigné par son adresse dans l'API (téléchargée avec la
clé de l'appli). Dépôt public : par son lien de téléchargement ordinaire.
"""
import datetime
import json
import os
import sys

signatures, release, numero, sortie = sys.argv[1], sys.argv[2], int(sys.argv[3]), sys.argv[4]
prive = os.environ.get("DEPOT_PRIVE", "true") != "false"
lien = "url" if prive else "browser_download_url"
fichiers = {a["name"]: a[lien] for a in json.load(open(release, encoding="utf-8"))["assets"]}


def plateforme(fichier):
    sig = os.path.join(signatures, fichier + ".sig")
    if not os.path.exists(sig) or fichier not in fichiers:
        return None
    return {"signature": open(sig, encoding="utf-8").read().strip(), "url": fichiers[fichier]}


plateformes = {}
if (w := plateforme("Documents-setup.exe")):
    plateformes["windows-x86_64"] = w

if not plateformes:
    print("Pas de signature : pas de mise à jour automatique publiée.")
    sys.exit(0)

fiche = {
    "version": f"0.2.{numero}",
    "notes": "Nouvelle version de Documents.",
    "pub_date": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    "platforms": plateformes,
}
open(sortie, "w", encoding="utf-8").write(json.dumps(fiche, indent=2))
print(json.dumps(fiche, indent=2))
