# Changelog

## 0.3.1

- macOS: the Showtime Suite folder in Applications gets its own icon (a dark folder with the Showtime
  Suite mark under a stage light), so it is easy to spot. Set at start and with every install.

## 0.3.0

- New look, like a stage before the show: a banner with a lighting rig (beams in the colours of the
  apps through haze, light on the stage floor, the audience in front), the title on its dark side and a
  board of what is going on (installed, running, updates ready).
- The apps are tiles in a grid (three, two or one per row, by window size), each lit from above in its
  own colour, with its state in plain words and the main action as a large button. "What's new" and
  "Remove…" sit below it.
- The developer section is "Backstage", in the same style. The window opens larger (1040 × 720).
- UI tests with egui_kittest and made-up app states; `SUITE_SHOTS=<folder> cargo test` writes pictures.

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
