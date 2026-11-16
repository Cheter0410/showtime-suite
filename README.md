# Showtime Suite

Installs, updates and starts the show tools: [Show Master](https://github.com/Cheter0410/Show-Master),
Beamweave and whatever comes next. Written in Rust (egui), for macOS, Windows and Linux.

- One card per app: installed version, newest release, **Install**, **Update**, **Open**, and what's new.
- Downloads come from each app's GitHub releases and are checked against the SHA-256 GitHub lists.
- A running app is never replaced (a show may be going on).
- **Remove…** on a card uninstalls the app (macOS: the bundle, Windows: its own uninstaller, Linux: the
  folder, menu entry and command). Settings and files made with it stay. A copy the system installed
  (Arch or Debian package, files in `/usr/local`) is removed through the package manager; the desktop
  asks for the password.
- Developer section: **Delete…** removes a source folder (build files included) after saying what would
  be lost (changed or new files, commits not on GitHub); **Clone** gets it back.
- Showtime Suite updates itself from `Cheter0410/showtime-suite-releases`.

Showtime Suite only downloads the finished release files (no source, no Git). All apps go into one folder:

| System  | Release file                    | Installed to                                     |
|---------|---------------------------------|--------------------------------------------------|
| macOS   | `.dmg`                          | `/Applications/Showtime Suite/` (or `~/Applications/Showtime Suite/`) |
| Windows | `…-Setup-x.y.z.exe` (Inno)      | `%LOCALAPPDATA%\Programs\Showtime Suite\<App>` (silent, per user; Showtime Suite itself in `…\Showtime Suite`) |
| Linux   | `…-linux-x86_64.tar.gz`         | `~/.local/share/showtime-suite/apps/<app>`, menu entry, `~/.local/bin` |

Copies in the old places (`/Applications/ShowMaster.app`, `…\Programs\Show Master`) are moved into the
folder with the next install or update. On macOS the folder gets its own icon
(`assets/macos/folder.svg`, rendered to `folder.png`; raise `FOLDER_ICON_LOOK` in `src/system.rs` when
it changes).

## Formerly Stagehand

Up to 0.1 the launcher was called Stagehand. At its first start Showtime Suite moves everything over:
its settings, the shared folder (`/Applications/Stagehand`, `~/.local/share/stagehand/apps`) with the
menu entries and commands pointing into it, and it removes the old launcher (Windows: through the old
launcher's own uninstaller; the apps in `…\Programs\Stagehand` move into the new folder with their
next update). An installed Stagehand cannot update itself to Showtime Suite – install it once by hand
from `Cheter0410/showtime-suite-releases`.

On Linux an app installed by the system (e.g. the Arch package of Show Master) is found on `PATH`.

## Adding an app

Add an entry to `apps.json` (name, tagline, colour, releases repository, release file names per system,
the Inno Setup `AppId`) and copy the same file to the default branch of `Cheter0410/showtime-suite-releases`:
installed copies read the catalog from there, so the new app appears without a new Showtime Suite. An app
that is not built in needs an `icon_url` (PNG).

## Build and install

Linux (no sudo: it goes into `~/.local/share/showtime-suite/apps`, the folder Showtime Suite installs all apps
into; run it again to update, `./install.sh --uninstall` removes it):

    ./install.sh

Mac (into `~/Desktop/dev/showtime-suite`):

    bash install-mac.sh

## Release

One command, the same on both computers (as in Show Master):

    ./release.sh            # Arch PC: Linux + Windows, Mac: the DMG - builds and uploads
    ./release.sh status     # what of the current version is up already

Each computer publishes on its own; nobody waits for the other. If the current version is published
and something changed since, the version is raised by one (the commit subjects since become "what's
new"), committed, tagged, pushed and published with this computer's files. If the newest release lacks
this computer's files, it builds exactly that tagged state and adds them. Until then the other systems
simply stay on the last version that exists for them - Showtime Suite always offers the newest release
that has a file for the system it runs on.
Missing tools (GitHub login, Rust, the Windows installer tools on the Arch PC) are set up on first use.

The app icon is `assets/icon.svg`; `node scripts/render-icon.mjs` renders the PNG and ICO from it.
