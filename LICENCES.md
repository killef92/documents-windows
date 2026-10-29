# Licences et droits

Tout ce qui est livré dans l'appli (setup .exe) est libre de redistribution.
Le détail complet, avec le texte de chaque licence, est dans `LICENCES-TIERS.txt`
(régénéré à chaque fabrication par `scripts/licences.py`, et copié dans le dossier d'installation).

## Ce qui est livré

| Élément | Origine | Licence |
|---|---|---|
| Code de l'appli | ce dépôt | MIT (`LICENSE`) |
| Icône de l'appli | dessinée pour ce projet | MIT (comme le code) |
| Icônes des fichiers dans l'Explorateur (PDF, JPG, PNG, WEBP, GIF), `scripts/icones-types.py` | dessinées pour ce projet ; lettres tracées avec la police [Inter](https://rsms.me/inter/) | MIT (icônes) ; Inter : SIL OFL 1.1 (permet d'utiliser ses lettres dans des images) |
| Icônes de l'interface | [Lucide](https://lucide.dev) | ISC (libre, mention à conserver) |
| Polices de l'interface : Geist et Geist Mono (`@fontsource-variable/geist`, `@fontsource-variable/geist-mono`) | [Vercel / The Geist Project Authors](https://github.com/vercel/geist-font) | SIL OFL 1.1 (libre d'embarquer et de redistribuer dans une appli ; interdit seulement de vendre la police seule) |
| Tauri, WRY, TAO et ~260 bibliothèques Rust | crates.io | MIT, Apache-2.0, BSD, Zlib, Unicode, MPL-2.0 (voir ci-dessous) |
| PDFium (lecture des PDF), version chromium/8125 | Google, binaires de [pdfium-binaries](https://github.com/bblanchon/pdfium-binaries) | BSD-3-Clause / Apache-2.0 ; bibliothèques incluses (FreeType, libjpeg-turbo, libpng, zlib, OpenJPEG, Little CMS, HarfBuzz, ICU, Abseil…) : licences libres (FTL, IJG, BSD, zlib, MIT, Unicode, Apache), textes dans `LICENCES-TIERS.txt` |
| lopdf (réorganiser les pages sans abîmer les formulaires) | J-F-Liu et contributeurs | MIT |
| `windows` (aperçu rapide : barre espace dans l'Explorateur) et `tray-icon` (icône près de l'horloge), Windows seulement | Microsoft, Tauri | MIT ou Apache-2.0 |
| `tauri-plugin-single-instance` (une seule copie de l'appli, les fichiers ouverts ensuite lui sont passés) | Tauri | MIT ou Apache-2.0 |
| `@tauri-apps/api`, `@tauri-apps/plugin-dialog` | npm | MIT ou Apache-2.0 |
| Programme d'installation | NSIS | zlib (libre) |
| Installateur de WebView2 | Microsoft | redistribuable gratuitement (prévu pour ça par Microsoft) |

**Vérification du 2026-10-29** : les 321 bibliothèques Rust livrées sous Windows, les 7 paquets
de l'interface et PDFium sont tous sous licence libre de redistribution (MIT, Apache-2.0, BSD,
ISC, Zlib, Unicode-3.0, OFL pour les polices, MPL-2.0). Aucune licence GPL ni « non commerciale ».

**MPL-2.0** (5 petites bibliothèques de lecture CSS) : on peut les redistribuer librement ;
seule obligation, si on modifiait *leurs* fichiers, publier ces modifications. On ne les modifie pas.

## Ce qui n'est PAS livré

- **Aucune ressource d'un autre logiciel** : pas d'icône, de police ni d'image reprise ailleurs.
  Le style « Ardoise » a ses propres couleurs et polices, et des icônes Lucide.
- **Polices** : l'appli embarque Geist et Geist Mono (OFL, voir plus haut) ;
  si elles manquaient, elle se rabat sur Segoe UI, déjà présente dans Windows.
  La police Inter sert seulement à dessiner les lettres des icônes de fichiers ; elle n'est pas livrée.

## Points de vigilance pour la suite

- **Nom de l'appli** : « Documents » depuis la version 0.1.29. L'ancien nom ne
  reste que dans des noms internes invisibles (identifiant de l'appli, types de fichiers déclarés à
  Windows), gardés pour ne pas perdre les réglages, signatures et choix d'appli par défaut déjà faits.
- **HEIC** (plus tard) : `libheif` est en LGPL et le format HEVC est soumis à brevets.
  À traiter à ce moment-là (par exemple, passer par le décodeur HEIF de Windows).
