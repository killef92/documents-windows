# Cahier des charges — « Documents »

## L'idée en une phrase
Une appli Windows, rapide et jolie, qui sait ouvrir PDF et photos, les feuilleter, les annoter, les signer, réorganiser les pages.

## Pour qui
Moi d'abord (usage pro chez Marouteau) : devis, factures, CERFA, photos de chantier, documents à signer.

## Ce que l'appli doit faire — par étapes

### Étape 1 — La visionneuse (le cœur)
- Ouvrir un PDF ou une image (JPG, PNG, HEIC, WebP) par double-clic ou glisser-déposer.
- Ouvrir **plusieurs fichiers dans une seule fenêtre** (comme mes 3 photos de chantier : « 3 documents, 3 pages au total »).
- Colonne de **miniatures à gauche**, page ou photo en grand à droite.
- Zoom +/–, taille réelle, ajuster à la fenêtre.
- Faire **pivoter** une page ou une photo.
- **Rechercher** du texte dans un PDF.
- Barre de titre : nom du fichier + « Page 1 sur 16 ».
- Ouverture quasi instantanée (objectif : moins d'1 seconde pour un PDF de 20 pages).

### Étape 2 — Gérer les pages
- Glisser les miniatures pour **réordonner** les pages.
- **Supprimer** une page.
- **Fusionner** : glisser un PDF dans la colonne de miniatures pour l'ajouter.
- Extraire une page en nouveau PDF.
- Enregistrer / Enregistrer sous / Exporter une image en PDF.

### Étape 3 — Annoter
- Surligner du texte.
- Rectangles, flèches, ronds (avec couleur de trait et de remplissage).
- Zone de texte (police et taille).
- Note (petit post-it).
- Dessin à main levée.

### Étape 4 — Signer et remplir
- Créer une signature (dessinée à la souris ou à partir d'une photo) et la réutiliser.
- Remplir les champs des formulaires PDF (CERFA).

### Étape 5 — Le petit plus
- Retouche simple des photos (luminosité, recadrage).
- Réduire la taille d'un PDF avant envoi par mail.
- Aperçu rapide avec la barre espace depuis l'Explorateur Windows.

## Ce qu'on ne fait PAS
- Pas de réécriture du texte d'un PDF (trop complexe, pas utile).
- Pas de compte, pas d'abonnement, pas de cloud : tout reste sur le PC.

## Comment on sait que c'est réussi
- J'arrête d'ouvrir PDFgear ou Acrobat pour le quotidien.
- Un CERFA s'ouvre, se remplit, se signe et s'enregistre sans bug.
- L'appli ressemble aux maquettes de `docs/maquettes/`.
