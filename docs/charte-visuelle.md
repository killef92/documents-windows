# Charte visuelle — style « Ardoise »

Références : `docs/maquettes/ardoise-sombre.png` et `docs/maquettes/ardoise-clair.png` (validées par Kilian le 2026-10-28).
La disposition est classique (barre d'outils en haut, miniatures à gauche, document au centre), avec un look qui nous est propre.
Règle d'or : **sobre, calme, le document est la star**. Gris-bleu, une seule couleur vive (menthe), des icônes fines.

## Structure de la fenêtre
1. **Barre de la fenêtre** (~38 px) : seulement le **nom du document au centre**, suivi de « Page 1 sur 6 » (séparé par un filet), et Réduire / Agrandir / Fermer à droite. Clic sur le nom : menu Fichier.
2. **Barre d'outils** (~56 px), juste en dessous : bouton miniatures à gauche · **tous les outils réunis dans un seul « îlot » centré** · champ « Rechercher » à droite (avec le rappel « Ctrl F »).
3. **Barre d'annotation** (~40 px) : apparaît sous la barre d'outils quand on clique sur « Annoter ».
4. **Colonne de miniatures** à gauche (~168 px), masquable. Inspecteur (Ctrl+I) à droite, ~272 px.
5. **Zone document** : fond très foncé (ou gris-bleu clair), page blanche centrée avec ombre douce.

## Couleurs
Le thème suit le réglage clair / sombre de Windows. Valeurs exactes : début de `ui/styles.css`.

| Rôle | Sombre | Clair |
|---|---|---|
| Barre de la fenêtre | `#0F1115` | `#EBEDF1` |
| Barre d'outils, colonne de miniatures | `#14161B` | `#F5F6F8` |
| Îlot des outils, champ de recherche | `#1D2027` | `#FFFFFF` (bord fin) |
| Zone autour du document | `#0D0F13` | `#E9ECF0` |
| Texte principal | `#E6E8EC` | `#1A1D23` |
| Icônes | `#B4B9C3` | `#4A505B` |
| Texte secondaire | `#7A818E` | `#737A86` |
| **Accent menthe** (sélection, bouton Annoter, boutons principaux) | `#5FD8A8` (texte dessus : `#0D1A14`) | `#139A6B` (texte dessus : blanc) |
| Séparateurs | `rgba(255,255,255,0.07)` | `rgba(20,28,40,0.08)` |

Pas de bleu système : l'accent est toujours la menthe.

## Typographie
- Police : **Geist** (libre, OFL, embarquée dans l'appli) ; secours : Segoe UI.
- Chiffres et repères (« Page 1 sur 6 », numéros des miniatures, « Ctrl F ») : **Geist Mono**.
- Titre : 13 px, gras (600). Sous-titre : 11 px, mono, gris secondaire.
- Numéros sous les miniatures : `01`, `02`… en mono 11 px, gris ; en menthe si sélectionnée.

## Boutons et icônes
- Icônes **au trait fin** (1,5 px), monochromes, ~18 px : Lucide.
- Les groupes de boutons n'ont pas de fond : ils sont posés dans l'îlot, séparés par un filet vertical.
- « Annoter » : carré arrondi teinté menthe ; plein menthe quand la barre d'annotation est ouverte.
- Survol : fond légèrement plus clair. Pas de bordures dures.
- Petite flèche « ⌄ » à côté des boutons qui ouvrent un menu.

## Miniatures
- Sélection : **barre menthe à gauche**, miniature pleine avec ombre ; les autres sont un peu estompées (pleines au survol).
- Coins arrondis 4 px. Sous la miniature : numéro de page (PDF) ou nom de fichier raccourci au milieu (`R261003…668_.jpg`).

## Fenêtre
- Coins arrondis (Windows 11 le fait tout seul).

## Animations
- Courtes (150–200 ms), douces. Ouverture / fermeture de la barre d'annotation en glissement.
- Rien qui clignote ou qui rebondit.
