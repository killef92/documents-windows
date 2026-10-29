"""Donne à l'appli le numéro de version 0.2.<n> (n = numéro de fabrication sur GitHub).

La mise à jour automatique ne propose une version que si son numéro est plus grand.
La série 0.1 était celle de l'ancien dépôt : 0.2.1 passe devant toutes ses versions.
"""
import json
import os
import sys

RACINE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
chemin = os.path.join(RACINE, "src-tauri", "tauri.conf.json")
conf = json.load(open(chemin, encoding="utf-8"))
conf["version"] = f"0.2.{int(sys.argv[1])}"
open(chemin, "w", encoding="utf-8").write(json.dumps(conf, indent=2, ensure_ascii=False) + "\n")
print("version", conf["version"])
