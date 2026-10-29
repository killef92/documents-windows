# Documents

Visionneuse et petit éditeur de PDF et d'images pour Windows 10/11, en Rust (Tauri 2).
Ouvrir, feuilleter, annoter, signer, remplir les formulaires, réorganiser les pages, retoucher les photos.

## Installer
1. Ouvrir **[Releases → Dernière version](https://github.com/killef92/documents-windows/releases/tag/derniere-version)**.
2. Télécharger `Documents-setup.exe` et l'ouvrir sur le PC Windows (10 ou 11, ou Windows Server).
3. Choisir « pour moi seulement » ou « pour tous les utilisateurs » (administrateur), puis Installer.

Les versions installées proposent ensuite toutes seules de se mettre à jour.

## Désinstaller
**Paramètres → Applications → Applications installées → Documents → Désinstaller**
(ou Panneau de configuration → Programmes et fonctionnalités). Vos fichiers ne sont jamais touchés ;
les signatures enregistrées dans l'appli restent dans `%LOCALAPPDATA%\fr.marouteau.apercu`
(voir [CONFIDENTIALITE.md](CONFIDENTIALITE.md)) et peuvent être supprimées à la main.

## Construire l'installateur
Onglet **Actions → Installateurs → Run workflow** sur `main` : le setup est publié dans
« Dernière version », avec le fichier qui sert à la mise à jour automatique.

## Mise à jour automatique
Les mises à jour sont signées : la clé secrète est rangée dans **Settings → Secrets and variables → Actions**
sous le nom `TAURI_SIGNING_PRIVATE_KEY`. Sans elle, les installateurs sont fabriqués quand même,
mais la mise à jour automatique n'est pas publiée.

## Lancer en développement
```
npm install
npm run tauri dev
```

## Le dossier
- `CLAUDE.md` — les instructions que Claude lit à chaque fois.
- `docs/` — cahier des charges et charte visuelle.
- `crates/core` — la logique (sans Tauri), testée avec `cargo test`.
- `src-tauri` — l'appli.
- `ui/` — l'interface (HTML, CSS, TypeScript, icônes Lucide).
- `LICENCES.md` — ce qui est livré et sous quelle licence (tout est libre de redistribution).
- `CONFIDENTIALITE.md` — ce que l'appli garde et envoie (rien sur vous ni vos fichiers).
- `SECURITY.md` — signaler une faille de sécurité.

## Code signing policy
Free code signing provided by [SignPath.io](https://about.signpath.io), certificate by [SignPath Foundation](https://signpath.org) (demande en cours).

- Auteur, relecteur et approbateur : [killef92](https://github.com/killef92) (Kilian Lefèvre).
- Chaque installateur est construit par GitHub Actions à partir de ce dépôt, puis sa signature est approuvée à la main.
- Confidentialité : l'appli n'envoie aucune information sur un autre système, sauf la recherche de mises à jour sur GitHub. Voir [CONFIDENTIALITE.md](CONFIDENTIALITE.md).
