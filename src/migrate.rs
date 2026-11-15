//! Moving over from the old name. Up to 0.1 the launcher was called Stagehand:
//! its settings, the folder all apps share, the menu entries and commands that
//! point into it, and the old launcher itself. Runs at every start and does
//! nothing once everything is moved.

use std::path::Path;

/// The old name in file and folder names.
const OLD_ID: &str = "stagehand";
/// The new one.
const ID: &str = "showtime-suite";

pub fn run() {
    if let Some(c) = dirs::config_dir() {
        move_dir(&c.join(OLD_ID), &c.join(ID));
    }
    if let Some(c) = dirs::cache_dir() {
        let _ = std::fs::remove_dir_all(c.join(OLD_ID));
    }
    #[cfg(target_os = "macos")]
    mac();
    #[cfg(target_os = "linux")]
    linux();
    #[cfg(target_os = "windows")]
    windows();
}

/// Move the folder `old` to `new`. When `new` is there already, what is only
/// in `old` goes into it. `old` is removed when it ends up empty. Whether
/// there was anything to move.
pub fn move_dir(old: &Path, new: &Path) -> bool {
    if !old.is_dir() {
        return false;
    }
    if !new.exists() {
        if let Some(p) = new.parent() {
            let _ = std::fs::create_dir_all(p);
        }
        if std::fs::rename(old, new).is_ok() {
            return true;
        }
    }
    if std::fs::create_dir_all(new).is_ok() {
        if let Ok(entries) = std::fs::read_dir(old) {
            for e in entries.flatten() {
                let to = new.join(e.file_name());
                if !to.exists() {
                    let _ = std::fs::rename(e.path(), to);
                }
            }
        }
    }
    let _ = std::fs::remove_dir(old);
    true
}

/// /Applications/Stagehand (or the one in the home folder) becomes
/// /Applications/Showtime Suite; the old launcher in it goes.
#[cfg(target_os = "macos")]
fn mac() {
    use crate::system::{FOLDER, OLD_FOLDER};
    let mut bases = vec![std::path::PathBuf::from("/Applications")];
    bases.extend(dirs::home_dir().map(|h| h.join("Applications")));
    for base in bases {
        move_dir(&base.join(OLD_FOLDER), &base.join(FOLDER));
        for folder in [FOLDER, OLD_FOLDER] {
            let old_app = base.join(folder).join("Stagehand.app");
            if old_app.join("Contents/Info.plist").exists() {
                let _ = std::fs::remove_dir_all(old_app);
            }
        }
    }
}

/// ~/.local/share/stagehand/apps becomes ~/.local/share/showtime-suite/apps.
/// Menu entries and the commands in ~/.local/bin are pointed at the new
/// place; the old launcher goes.
#[cfg(target_os = "linux")]
fn linux() {
    let Some(data) = dirs::data_dir() else { return };
    let old = data.join(OLD_ID).join("apps");
    let new = data.join(ID).join("apps");
    if move_dir(&old, &new) {
        let _ = std::fs::remove_dir(data.join(OLD_ID));
    }
    let menu = data.join("applications");
    // The old launcher: its folder, menu entry and command.
    let _ = std::fs::remove_dir_all(new.join(OLD_ID));
    let old_entry = menu.join(format!("{OLD_ID}.desktop"));
    if std::fs::read_to_string(&old_entry).is_ok_and(|t| t.contains("X-Stagehand=1")) {
        let _ = std::fs::remove_file(&old_entry);
    }
    fix_menu(&menu, &old, &new);
    if let Some(home) = dirs::home_dir() {
        let bin = home.join(".local/bin");
        fix_links(&bin, &old, &new);
        let link = bin.join(OLD_ID);
        if std::fs::read_link(&link).is_ok_and(|t| t.starts_with(&new) || t.starts_with(&old)) {
            let _ = std::fs::remove_file(link);
        }
    }
}

/// Our menu entries in `menu`: paths into `old` point into `new`, the old
/// mark becomes the new one.
#[cfg(any(target_os = "linux", test))]
fn fix_menu(menu: &Path, old: &Path, new: &Path) {
    let Ok(entries) = std::fs::read_dir(menu) else { return };
    let (o, n) = (old.to_string_lossy(), new.to_string_lossy());
    for e in entries.flatten() {
        let p = e.path();
        let Ok(text) = std::fs::read_to_string(&p) else { continue };
        if !text.lines().any(|l| l == "X-Stagehand=1") {
            continue;
        }
        let fixed = text
            .replace(&format!("{o}/"), &format!("{n}/"))
            .replace("X-Stagehand=1", "X-Showtime-Suite=1");
        let _ = std::fs::write(&p, fixed);
    }
}

/// Links in `bin` that point into `old` point into `new`.
#[cfg(all(unix, any(target_os = "linux", test)))]
fn fix_links(bin: &Path, old: &Path, new: &Path) {
    let Ok(entries) = std::fs::read_dir(bin) else { return };
    for e in entries.flatten() {
        let p = e.path();
        let Ok(target) = std::fs::read_link(&p) else { continue };
        if let Ok(rest) = target.strip_prefix(old) {
            let _ = std::fs::remove_file(&p);
            let _ = std::os::unix::fs::symlink(new.join(rest), &p);
        }
    }
}

/// The old launcher's own uninstaller. It only takes its own files; the apps
/// in the old folder stay and move into the new one with their next update.
#[cfg(target_os = "windows")]
fn windows() {
    const OLD_APP_ID: &str = "{3E5A9C21-7D4B-4F6E-9A1C-5B8D2E0F7A63}";
    let Some((_, dir)) = crate::system::inno_entry(OLD_APP_ID) else {
        return;
    };
    let unins = dir.join("unins000.exe");
    if unins.exists() {
        let _ = std::process::Command::new(unins)
            .args(["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART"])
            .spawn();
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn temp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("sts-migrate-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn moves_the_folder_or_what_is_missing() {
        let d = temp("move");
        let (old, new) = (d.join("stagehand"), d.join("showtime-suite"));
        assert!(!move_dir(&old, &new), "nothing there");
        std::fs::create_dir_all(old.join("lantern")).unwrap();
        std::fs::write(old.join("settings.json"), "{}").unwrap();
        assert!(move_dir(&old, &new));
        assert!(!old.exists() && new.join("lantern").is_dir());
        // Both there: only what is missing moves, the new one wins.
        std::fs::create_dir_all(old.join("beamweave")).unwrap();
        std::fs::write(old.join("settings.json"), "old").unwrap();
        move_dir(&old, &new);
        assert!(new.join("beamweave").is_dir());
        assert_eq!(std::fs::read_to_string(new.join("settings.json")).unwrap(), "{}");
        assert!(old.join("settings.json").exists(), "kept, not overwritten");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[cfg(unix)]
    #[test]
    fn menu_entries_and_commands_follow() {
        let d = temp("menu");
        let (old, new) = (d.join("stagehand/apps"), d.join("showtime-suite/apps"));
        let (menu, bin) = (d.join("applications"), d.join("bin"));
        std::fs::create_dir_all(&menu).unwrap();
        std::fs::create_dir_all(&bin).unwrap();
        let entry = format!(
            "[Desktop Entry]\nName=Lantern\nExec=\"{0}/lantern/lantern\" %f\nIcon={0}/lantern/icon.png\nX-Stagehand=1\n",
            old.display()
        );
        std::fs::write(menu.join("lantern.desktop"), entry).unwrap();
        std::fs::write(menu.join("other.desktop"), "[Desktop Entry]\nName=Other\n").unwrap();
        std::os::unix::fs::symlink(old.join("lantern/lantern"), bin.join("lantern")).unwrap();
        std::os::unix::fs::symlink("/usr/bin/true", bin.join("other")).unwrap();
        fix_menu(&menu, &old, &new);
        fix_links(&bin, &old, &new);
        let e = std::fs::read_to_string(menu.join("lantern.desktop")).unwrap();
        assert!(e.contains(&format!("Exec=\"{}/lantern/lantern\" %f", new.display())));
        assert!(e.contains(&format!("Icon={}/lantern/icon.png", new.display())));
        assert!(e.ends_with("X-Showtime-Suite=1\n") && !e.contains("stagehand"));
        assert_eq!(
            std::fs::read_to_string(menu.join("other.desktop")).unwrap(),
            "[Desktop Entry]\nName=Other\n"
        );
        assert_eq!(std::fs::read_link(bin.join("lantern")).unwrap(), new.join("lantern/lantern"));
        assert_eq!(std::fs::read_link(bin.join("other")).unwrap(), Path::new("/usr/bin/true"));
        let _ = std::fs::remove_dir_all(&d);
    }
}
