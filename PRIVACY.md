# Privacy

**Documents collects nothing and sends nothing about you or your files.**

## What the app does on the Internet

Only one thing: at startup, it checks on GitHub whether a new version exists
(the repository's `latest.json` file). If so, it offers it to you; the new version
is only downloaded if you click "Update". This check sends no personal information:
as with any web page, GitHub only sees the connection address and the fact that a request was made.

Nothing else: no statistics, no ads, no account, no cloud.
Your PDFs and photos never leave your computer.

## What the app keeps on your computer

- **Your signatures** (drawn or photographed), so you can reuse them: in the app's folder of
  your Windows session (`%LOCALAPPDATA%\fr.marouteau.apercu`). Only your Windows account can
  access it. They are deleted if you uninstall with "Delete the application data" checked.
- **A few settings** in the registry of your Windows session: quick preview with the space bar,
  and the app's registration for "Open with" and default apps.

Your files are only changed when you click "Save".

## Language

The app reads the display language chosen in Windows settings to show its text in that language.
This setting is not stored or sent anywhere.

## Quick preview (space bar)

To open the preview when you press Space in File Explorer, the app watches the Space key.
It looks at no other key, records nothing and sends nothing.
The option can be turned off in the File menu, or from the icon near the clock.

## Windows components

Display goes through WebView2, a Microsoft component that comes with Windows. Its updates
are handled by Windows, under Microsoft's rules.
