# Projet : Documents

Visionneuse / éditeur léger de PDF et d'images pour Windows 10/11. Écrit en Rust.
L'appli s'appelle **« Documents »** (depuis la 0.1.29 ; l'ancien nom est une marque déposée : ne plus l'utiliser).
Noms internes gardés exprès pour ne rien perdre chez les utilisateurs : identifiant `fr.marouteau.apercu`,
types de fichiers `Apercu.*`, clé `apercu.signatures`, argument `--apercu-rapide`. L'« aperçu rapide » (Espace) garde son nom.

## À lire avant de coder
- `docs/cahier-des-charges.md` — ce que fait l'appli, découpé en 5 étapes.
- `docs/charte-visuelle.md` — couleurs, typo, disposition. **À respecter.**
- `docs/maquettes/ardoise-*.png` — le style « Ardoise » (choisi par Kilian le 2026-10-28) : la référence visuelle. Les regarder avant tout travail d'interface.
- Captures de référence (disposition, menus, dialogues) : dans le dossier privé du projet `/mnt/project-files/inspiration/`, **jamais dans le dépôt** (il est public).

## Qui est l'utilisateur
Kilian, pas développeur. Claude écrit le code et fait les tests ; Kilian teste le setup .exe sur un serveur Windows.
Il ne compile pas lui-même : la version Windows est fabriquée par la GitHub Action `.github/workflows/installateurs.yml` (setup .exe Windows seulement).
- Lui parler **en français**, court, sans jargon.
- Après chaque tâche : dire en 2-3 phrases ce qui marche maintenant et comment le tester (quelle commande, quoi cliquer).
- Ne pas lui demander de choix techniques : les faire, et les expliquer simplement si besoin.

## Choix techniques (déjà décidés)
- **Tauri 2** : cœur en Rust, interface en HTML/CSS/TypeScript (permet de coller fidèlement à la référence).
- **PDF** : `pdfium-render` (moteur PDF de Chrome) pour afficher, rechercher, annoter, remplir les formulaires. `lopdf` en appoint pour réorganiser / fusionner les pages.
- **Images** : crate `image` ; HEIC via `libheif-rs` (étape 2 ou plus tard).
- **Interface** : TypeScript sans framework lourd (ou Svelte si ça devient gros). Icônes Lucide.
- Structure en workspace Cargo :
  - `crates/core` — toute la logique (ouvrir, rendre une page en image, réordonner, fusionner, annoter). **Aucune dépendance à Tauri.** Testée avec `cargo test`.
  - `src-tauri` — l'appli, qui appelle `core`.
  - `ui/` — l'interface.
- Cible : Windows x64 uniquement. Le développement peut se faire sous Linux (Tauri tourne partout), mais tester régulièrement sur Windows.
- Livraison : **un seul setup .exe** (installateur NSIS de Tauri), pas de .msi. Il installe WebView2 s'il manque (Windows Server ne l'a pas toujours).
- Thème : l'appli suit le réglage clair / sombre de l'ordinateur ; les deux thèmes doivent être soignés.
- Fenêtre sans cadre système : la barre du haut sert de barre de titre, avec Réduire / Agrandir / Fermer à droite.

## Règles de travail
- **Dépôt public** (`killef92/documents-windows`, depuis le 2026-10-29) : aucun secret, aucune capture d'un autre logiciel, et aucune mention d'une marque ou d'un système d'un autre éditeur (le nom de l'appli et le code doivent rester neutres). L'ancien dépôt privé est figé : ne plus y toucher.
- **Une étape du cahier des charges à la fois.** Commencer par l'étape 1. Ne pas commencer une étape tant que la précédente ne marche pas.
- Petits pas : chaque fonctionnalité finie doit pouvoir se lancer et se voir.
- **Minutes GitHub Actions limitées** (2000/mois pour tous les projets de Kilian ; Windows compte ×2). La construction des installateurs ne se lance qu'à la main (workflow_dispatch sur main), une fois par version à livrer. Tests et Clippy se font en local, pas dans Actions.
- Tests : chaque fonction de `crates/core` a au moins un test, avec de petits PDF / images d'exemple dans `crates/core/tests/fixtures/`.
- Avant de dire « c'est fini » : `cargo test` passe, `cargo clippy` sans avertissement, l'appli se lance avec `npm run tauri dev`, la GitHub Action Windows est verte (lancée à la main pour la version à livrer), et l'écran a été comparé aux maquettes de `docs/maquettes/`.
- Performance : rendre d'abord la page visible et les miniatures visibles, le reste en arrière-plan. Ne jamais bloquer l'interface.
- Ne jamais modifier le fichier d'origine sans « Enregistrer » explicite ; garder un moyen d'annuler (Ctrl+Z).
- **Licences** : tout ce qui est livré doit être libre de redistribution (MIT, Apache, BSD, ISC, zlib…). Aucune ressource reprise d'un autre logiciel (icônes, polices, images). Avant d'ajouter une dépendance, vérifier sa licence et mettre à jour `LICENCES.md`.
- Raccourcis clavier Windows : Ctrl+O, Ctrl+S, Ctrl+F, Ctrl+ +/-, Ctrl+0, Ctrl+R (pivoter), Suppr (supprimer page).

## Première mission (étape 1)
1. Créer le workspace (core + src-tauri + ui) et vérifier que la fenêtre vide s'ouvre.
2. Construire la fenêtre selon la charte : barre du haut, colonne de miniatures, zone document (sans fonctionnalité, juste l'apparence) — et la comparer aux captures.
3. Ouvrir un PDF : afficher la page 1, puis les miniatures, puis navigation entre pages.
4. Ouvrir une ou plusieurs images dans la même fenêtre.
5. Zoom, pivoter, recherche de texte.
6. Associer l'appli aux fichiers .pdf / .jpg / .png pour l'ouvrir par double-clic.
