# Confidentialité

**Documents ne collecte rien et n'envoie rien sur vous ni sur vos fichiers.**

## Ce que l'appli fait sur Internet

Une seule chose : au démarrage, elle regarde sur GitHub si une nouvelle version existe
(fichier `latest.json` du dépôt). Si c'est le cas, elle vous le propose ; la nouvelle version
n'est téléchargée que si vous cliquez sur « Mettre à jour ». Cette vérification n'envoie aucune
information personnelle : GitHub voit seulement, comme pour toute page web, l'adresse de
connexion et le fait qu'une demande a eu lieu.

Rien d'autre : pas de statistiques, pas de publicité, pas de compte, pas de « nuage ».
Vos PDF et vos photos ne quittent jamais votre ordinateur.

## Ce que l'appli garde sur votre ordinateur

- **Vos signatures** (dessinées ou prises en photo), pour les réutiliser : dans le dossier de
  l'appli de votre session Windows (`%LOCALAPPDATA%\fr.marouteau.apercu`). Seul votre compte
  Windows y a accès. Elles sont supprimées si vous désinstallez en cochant « Supprimer les
  données de l'application ».
- **Quelques réglages** dans le registre de votre session Windows : l'aperçu rapide avec la
  barre espace, et la déclaration de l'appli pour « Ouvrir avec » et les applis par défaut.

Vos fichiers ne sont modifiés que lorsque vous cliquez sur « Enregistrer ».

## Aperçu rapide (barre espace)

Pour ouvrir l'aperçu quand vous appuyez sur Espace dans l'Explorateur, l'appli surveille la
touche Espace. Elle ne regarde aucune autre touche, n'enregistre rien et n'envoie rien.
L'option se désactive dans le menu Fichier, ou par l'icône près de l'horloge.

## Composants de Windows

L'affichage passe par WebView2, un composant de Microsoft fourni avec Windows. Ses mises à jour
sont gérées par Windows, selon les règles de Microsoft.
