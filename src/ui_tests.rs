//! The window with made-up app states, drawn by egui_kittest. Nothing is looked up or installed, and
//! the settings are never saved. `SUITE_SHOTS=<folder> cargo test` writes a picture of each.

use super::*;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;

fn release(def: &AppDef, version: &str) -> Release {
    // A download that fits this computer, so Install / Update are offered.
    let name = if cfg!(target_os = "macos") {
        format!("{}-{version}.dmg", def.id)
    } else if cfg!(target_os = "windows") {
        let w = def.windows.as_ref().map(|w| (w.asset_prefix.clone(), w.asset_suffix.clone())).unwrap_or_default();
        format!("{}{version}{}", w.0, w.1)
    } else {
        format!("{}-{version}{}", def.id, def.linux.as_ref().map(|l| l.asset_suffix.clone()).unwrap_or_default())
    };
    Release {
        version: version.into(),
        notes: "- Start screen and setup wizard.\n- Fixes.".into(),
        page: String::new(),
        assets: vec![github::Asset { name, url: String::new(), size: 48_000_000, sha256: None }],
    }
}

fn installed(version: &str) -> Option<Installed> {
    Some(Installed { version: Some(version.into()), path: PathBuf::from("/Applications/Example.app") })
}

/// Show Master with an update, Beamweave running, Lantern not installed.
fn launcher(size: [f32; 2], dev_mode: bool) -> Harness<'static, Launcher> {
    let mut h = Harness::builder().with_size(egui::vec2(size[0], size[1])).wgpu().build_eframe(move |cc| {
        let settings = Settings { dev_mode, ..Settings::default() };
        let mut l = Launcher::offline(&cc.egui_ctx, settings, Catalog::built_in());
        // Never look at what is really installed during the test.
        l.last_detect = Instant::now() + Duration::from_secs(3600);
        for c in &mut l.cards {
            c.detected = true;
            match c.def.id.as_str() {
                "showmaster" => {
                    c.installed = installed("0.5.0");
                    c.latest = Some(Ok(Some(release(&c.def, "0.5.1"))));
                }
                "beamweave" => {
                    c.installed = installed("0.7.2");
                    c.running = true;
                    c.latest = Some(Ok(Some(release(&c.def, "0.7.2"))));
                }
                _ => c.latest = Some(Ok(Some(release(&c.def, "0.11.6")))),
            }
        }
        l
    });
    h.run_steps(4);
    h
}

fn shot(h: &mut Harness<'static, Launcher>, name: &str) {
    let Some(dir) = std::env::var_os("SUITE_SHOTS").map(PathBuf::from) else { return };
    let _ = std::fs::create_dir_all(&dir);
    match h.render() {
        Ok(img) => {
            let _ = img.save(dir.join(format!("{name}.png")));
        }
        Err(e) => eprintln!("no picture of {name}: {e}"),
    }
}

#[test]
fn every_app_has_a_tile_with_what_can_be_done() {
    let mut h = launcher([1040.0, 720.0], false);
    h.get_by_label("Show Master");
    h.get_by_label("Beamweave");
    h.get_by_label("Lantern");
    h.get_by_label("Blueprint");
    h.get_by_label("Update to 0.5.1");
    assert_eq!(h.get_all_by_label("Install").count(), 2);
    assert_eq!(h.get_all_by_label("Open").count(), 2);
    h.get_by_label("1 update ready");
    h.get_by_label("1 running");
    h.get_by_label("2 installed");
    shot(&mut h, "suite-1-wide");
}

#[test]
fn whats_new_opens_the_notes_in_the_tile() {
    let mut h = launcher([1040.0, 720.0], false);
    if let Some(n) = h.get_all_by_label("What's new").next() {
        n.click();
    }
    h.run_steps(4);
    assert!(h.state().cards.iter().any(|c| c.notes_open));
    shot(&mut h, "suite-2-notes");
}

#[test]
fn remove_asks_first() {
    let mut h = launcher([1040.0, 720.0], false);
    if let Some(n) = h.get_all_by_label("Remove…").next() {
        n.click();
    }
    h.run_steps(4);
    let c = h.state().cards.iter().find(|c| c.def.id == "showmaster").map(|c| c.confirm_remove);
    assert_eq!(c, Some(true), "Show Master asks before it is removed");
    h.get_by_label("Cancel").click();
    h.run_steps(4);
    assert!(h.state().cards.iter().all(|c| !c.confirm_remove));
}

#[test]
fn narrow_window_stacks_the_tiles() {
    let mut h = launcher([520.0, 640.0], false);
    let a = h.get_by_label("Show Master").rect();
    let b = h.get_by_label("Beamweave").rect();
    assert!(b.top() > a.bottom(), "one tile under the other: {a:?} {b:?}");
    shot(&mut h, "suite-3-narrow");
}

#[test]
fn medium_window_has_two_columns() {
    let mut h = launcher([820.0, 600.0], false);
    let a = h.get_by_label("Show Master").rect();
    let b = h.get_by_label("Beamweave").rect();
    assert!((a.top() - b.top()).abs() < 1.0 && b.left() > a.right(), "side by side: {a:?} {b:?}");
    shot(&mut h, "suite-4-medium");
}

#[test]
fn developer_section_shows_in_developer_mode() {
    if !dev::available() {
        return;
    }
    let mut h = launcher([1040.0, 900.0], true);
    h.get_by_label("Release all");
    shot(&mut h, "suite-5-developer");
}
