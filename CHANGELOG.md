# Changelog

## 0.2.1

- Catalog: Beamweave is released with `./release.sh` and installed from source with
  `bash install-mac.sh` / `./install.sh`, like the other apps.

## 0.2.0

- Stagehand is now **Showtime Suite** (program `showtime-suite`, app `Showtime Suite.app`, repositories
  `Cheter0410/showtime-suite` and `Cheter0410/showtime-suite-releases`).
- Moves over from the old name at the first start: settings, the shared app folder
  (`/Applications/Stagehand` → `/Applications/Showtime Suite`, `~/.local/share/stagehand/apps` →
  `~/.local/share/showtime-suite/apps`) with menu entries and commands, and removes the old launcher
  (Windows: with its own uninstaller). The install scripts do the same.
- Installed Stagehand copies do not update themselves to it: install Showtime Suite once by hand.

## 0.1.1

- CLAUDE.md: how two people (and their Claude Code sessions) work on this repository – straight on
  `main`, pull before work and before push, version and changelog with every change, what to do on
  conflicts.
- `.gitignore` leaves out `.DS_Store`.

## 0.1.0

- First version: Show Master and Beamweave as cards; install, update and start them from their GitHub
  releases (macOS, Windows, Linux). New apps appear through the live catalog without a new Stagehand.
- Updates itself from Cheter0410/stagehand-releases.
