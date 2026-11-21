//! The window: one card per app with its state and what can be done
//! (Install, Update, Open). Network and installs run on worker threads that
//! report back through a channel.

use crate::catalog::{self, AppDef, Catalog};
use crate::dev::{self, RepoState, Settings, Step};
use crate::github::{self, Release};
use crate::system::{self, Installed};
use crate::stage;
use egui::{pos2, vec2, Color32, CornerRadius, Frame, Margin, Rect, RichText, Shape, Stroke, Vec2};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant};

/// A source checkout, its git state, and why git failed there (if it did).
type RepoEntry = (Option<PathBuf>, Option<RepoState>, Option<String>);

const BG: Color32 = Color32::from_rgb(9, 10, 14);
const CARD: Color32 = Color32::from_rgb(20, 22, 29);
const LINE: Color32 = Color32::from_rgb(38, 42, 52);
/// The amber of a warm stage light: the suite's own accent.
const AMBER: Color32 = Color32::from_rgb(255, 196, 92);
const TEXT: Color32 = Color32::from_rgb(232, 235, 239);
const MUTED: Color32 = Color32::from_rgb(140, 146, 160);
const WARN: Color32 = Color32::from_rgb(245, 158, 11);
const ERROR: Color32 = Color32::from_rgb(239, 68, 68);
const OK: Color32 = Color32::from_rgb(34, 197, 94);

#[cfg(test)]
#[path = "ui_tests.rs"]
mod ui_tests;

/// This program as an entry of its own releases (self-update).
const SELF_RELEASES: &str = "Cheter0410/showtime-suite-releases";

enum Msg {
    Catalog(Catalog),
    Detected(String, Option<Installed>, bool),
    Latest(String, Result<Option<Release>, String>),
    Icon(String, Vec<u8>),
    Progress(String, u64, u64),
    Installing(String),
    Finished(String, Result<(), String>),
    SelfLatest(Option<Release>),
    /// Source checkouts found: (app id, (folder, state, git problem)).
    Repos(Vec<(String, RepoEntry)>),
    /// An app was uninstalled (or not).
    Removed(String, Result<(), String>),
    /// A source checkout was deleted (or not).
    Deleted(String, Result<String, String>),
}

#[derive(Clone, Copy, PartialEq)]
enum Job {
    Download(u64, u64),
    Install,
    Remove,
}

struct Card {
    def: AppDef,
    installed: Option<Installed>,
    detected: bool,
    running: bool,
    latest: Option<Result<Option<Release>, String>>,
    job: Option<Job>,
    message: Option<(String, bool)>,
    notes_open: bool,
    /// "Remove from this computer?" is being asked.
    confirm_remove: bool,
    icon: Option<egui::TextureHandle>,
}

pub struct Launcher {
    cards: Vec<Card>,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    last_detect: Instant,
    self_update: Option<Release>,
    self_job: Option<Job>,
    self_error: Option<String>,
    icons_wanted: HashMap<String, bool>,
    settings: Settings,
    /// App id -> (checkout, state, git problem), for the developer section.
    repos: HashMap<String, RepoEntry>,
    last_repo_scan: Option<Instant>,
    dev_message: Option<(String, bool)>,
    /// A source folder being typed in (app id, text).
    path_edit: Option<(String, String)>,
    /// App id whose checkout is asked to be deleted / being deleted.
    delete_ask: Option<String>,
    deleting: Option<String>,
    /// The stage banner and the tile glow (made on first use).
    banner: Option<egui::TextureHandle>,
    glow: Option<egui::TextureHandle>,
}

/// What the developer section asks for.
enum DevAction {
    Release(String),
    Install(String),
    Clone(String),
    ReleaseAll,
    InstallAll,
    EditPath(String),
    SetPath(String, String),
    CancelPath,
    /// Ask before deleting the checkout.
    Delete(String),
    DeleteConfirm(String),
    DeleteCancel,
}

impl Launcher {
    pub fn new(ctx: &egui::Context) -> Self {
        let mut l = Self::offline(ctx, Settings::load(), Catalog::load());
        l.refresh(ctx);
        // The live catalog (new apps without a new Showtime Suite).
        let (tx, c) = (l.tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            if let Ok(cat) = Catalog::fetch() {
                let _ = tx.send(Msg::Catalog(cat));
                c.request_repaint();
            }
        });
        // A newer Showtime Suite?
        let (tx, c) = (l.tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            let me = self_def();
            let r = github::latest(SELF_RELEASES, |r| system::pick_asset(&me, r).is_some())
                .ok()
                .flatten()
                .filter(|r| github::is_newer(&r.version, env!("CARGO_PKG_VERSION")));
            let _ = tx.send(Msg::SelfLatest(r));
            c.request_repaint();
        });
        l
    }

    /// The window without looking anything up yet (tests show it with made-up states).
    fn offline(ctx: &egui::Context, settings: Settings, catalog: Catalog) -> Self {
        style(ctx);
        let (tx, rx) = channel();
        let mut l = Self {
            cards: Vec::new(),
            tx,
            rx,
            last_detect: Instant::now(),
            self_update: None,
            self_job: None,
            self_error: None,
            icons_wanted: HashMap::new(),
            settings,
            repos: HashMap::new(),
            last_repo_scan: None,
            dev_message: None,
            path_edit: None,
            delete_ask: None,
            deleting: None,
            banner: None,
            glow: None,
        };
        l.set_catalog(catalog, ctx);
        l
    }

    fn set_catalog(&mut self, cat: Catalog, ctx: &egui::Context) {
        let mut old: HashMap<String, Card> = self
            .cards
            .drain(..)
            .map(|c| (c.def.id.clone(), c))
            .collect();
        for def in cat.apps {
            let card = match old.remove(&def.id) {
                Some(mut c) => {
                    c.def = def;
                    c
                }
                None => {
                    let icon =
                        catalog::built_in_icon(&def.id).and_then(|b| texture(ctx, &def.id, b));
                    if icon.is_none() {
                        if let Some(url) = def.icon_url.clone() {
                            if self.icons_wanted.insert(def.id.clone(), true).is_none() {
                                let (tx, id, c) = (self.tx.clone(), def.id.clone(), ctx.clone());
                                std::thread::spawn(move || {
                                    if let Ok(r) = crate::github::http().get(&url).call() {
                                        let mut v = Vec::new();
                                        if std::io::Read::read_to_end(&mut r.into_reader(), &mut v)
                                            .is_ok()
                                        {
                                            let _ = tx.send(Msg::Icon(id, v));
                                            c.request_repaint();
                                        }
                                    }
                                });
                            }
                        }
                    }
                    Card {
                        def,
                        installed: None,
                        detected: false,
                        running: false,
                        latest: None,
                        job: None,
                        message: None,
                        notes_open: false,
                        confirm_remove: false,
                        icon,
                    }
                }
            };
            self.cards.push(card);
        }
    }

    /// Look for installed copies and the newest releases (all apps).
    fn refresh(&mut self, ctx: &egui::Context) {
        for c in &mut self.cards {
            c.latest = None;
        }
        let defs: Vec<AppDef> = self.cards.iter().map(|c| c.def.clone()).collect();
        self.detect(ctx);
        for def in defs {
            let (tx, c) = (self.tx.clone(), ctx.clone());
            std::thread::spawn(move || {
                let r = github::latest(&def.releases, |r| system::pick_asset(&def, r).is_some());
                let _ = tx.send(Msg::Latest(def.id, r));
                c.request_repaint();
            });
        }
    }

    /// Which apps are installed and running (cheap, runs every few seconds).
    fn detect(&mut self, ctx: &egui::Context) {
        self.last_detect = Instant::now();
        let defs: Vec<AppDef> = self.cards.iter().map(|c| c.def.clone()).collect();
        let (tx, c) = (self.tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            for def in defs {
                let inst = system::installed(&def);
                let running = inst.as_ref().is_some_and(|i| system::is_running(&def, i));
                let _ = tx.send(Msg::Detected(def.id, inst, running));
            }
            c.request_repaint();
        });
    }

    fn card_mut(&mut self, id: &str) -> Option<&mut Card> {
        self.cards.iter_mut().find(|c| c.def.id == id)
    }

    fn drain(&mut self, ctx: &egui::Context) {
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Msg::Catalog(cat) => {
                    let new: Vec<AppDef> = cat
                        .apps
                        .iter()
                        .filter(|a| !self.cards.iter().any(|c| c.def.id == a.id))
                        .cloned()
                        .collect();
                    self.set_catalog(cat, ctx);
                    if !new.is_empty() {
                        self.refresh(ctx);
                    }
                }
                Msg::Detected(id, inst, running) => {
                    if let Some(c) = self.card_mut(&id) {
                        c.installed = inst;
                        c.running = running;
                        c.detected = true;
                    }
                }
                Msg::Latest(id, r) => {
                    if let Some(c) = self.card_mut(&id) {
                        c.latest = Some(r);
                    }
                }
                Msg::Icon(id, bytes) => {
                    let t = texture(ctx, &id, &bytes);
                    if let Some(c) = self.card_mut(&id) {
                        c.icon = t;
                    }
                }
                Msg::Progress(id, d, t) => {
                    if id == "showtime-suite" {
                        self.self_job = Some(Job::Download(d, t));
                    } else if let Some(c) = self.card_mut(&id) {
                        c.job = Some(Job::Download(d, t));
                    }
                }
                Msg::Installing(id) => {
                    if id == "showtime-suite" {
                        self.self_job = Some(Job::Install);
                    } else if let Some(c) = self.card_mut(&id) {
                        c.job = Some(Job::Install);
                    }
                }
                Msg::Finished(id, r) => {
                    if id == "showtime-suite" {
                        self.self_job = None;
                        self.self_error = r.err();
                        continue;
                    }
                    if let Some(c) = self.card_mut(&id) {
                        c.job = None;
                        c.message = Some(match r {
                            Ok(()) => (format!("{} is installed.", c.def.name), false),
                            Err(e) => (e, true),
                        });
                    }
                    self.detect(ctx);
                }
                Msg::SelfLatest(r) => self.self_update = r,
                Msg::Repos(list) => {
                    for (id, entry) in list {
                        self.repos.insert(id, entry);
                    }
                }
                Msg::Removed(id, r) => {
                    if let Some(c) = self.card_mut(&id) {
                        c.job = None;
                        c.message = Some(match r {
                            Ok(()) => (format!("{} was removed.", c.def.name), false),
                            Err(e) => (e, true),
                        });
                    }
                    self.detect(ctx);
                }
                Msg::Deleted(id, r) => {
                    self.deleting = None;
                    if r.is_ok() {
                        self.settings.repos.remove(&id);
                        self.settings.save();
                        self.repos.remove(&id);
                    }
                    self.dev_message = Some(match r {
                        Ok(m) => (m, false),
                        Err(e) => (e, true),
                    });
                    self.scan_repos(ctx);
                }
            }
        }
    }

    /// Apps with a source repository: the catalog's, then Showtime Suite itself.
    fn dev_apps(&self) -> Vec<AppDef> {
        let mut v: Vec<AppDef> = self
            .cards
            .iter()
            .filter(|c| c.def.dev.is_some())
            .map(|c| c.def.clone())
            .collect();
        v.push(self_def());
        v
    }

    /// Find the source checkouts and their git state (off the UI thread).
    fn scan_repos(&mut self, ctx: &egui::Context) {
        self.last_repo_scan = Some(Instant::now());
        let (apps, settings, tx, c) = (
            self.dev_apps(),
            self.settings.clone(),
            self.tx.clone(),
            ctx.clone(),
        );
        std::thread::spawn(move || {
            let list = apps
                .iter()
                .map(|a| {
                    let dir = dev::find_checkout(a, &settings);
                    let state = dir.as_deref().and_then(dev::repo_state);
                    let problem = match (&dir, &state) {
                        (Some(d), None) => Some(dev::git_problem(d)),
                        _ => None,
                    };
                    (a.id.clone(), (dir, state, problem))
                })
                .collect();
            let _ = tx.send(Msg::Repos(list));
            c.request_repaint();
        });
    }

    /// The checkout of an app, if it is a usable git checkout.
    fn checkout(&self, id: &str) -> Option<PathBuf> {
        self.repos
            .get(id)
            .filter(|(_, s, _)| s.is_some())
            .and_then(|(d, _, _)| d.clone())
    }

    fn dev_action(&mut self, action: DevAction, ctx: &egui::Context) {
        let apps = self.dev_apps();
        let release_step = |a: &AppDef, dir: PathBuf| {
            Some(Step {
                name: format!("Release {}", a.name),
                dir,
                command: a.dev.as_ref()?.release.clone()?,
            })
        };
        let install_step = |a: &AppDef, dir: PathBuf| {
            Some(Step {
                name: format!("Install {}", a.name),
                dir,
                command: dev::install_command(a)?,
            })
        };
        let one = |id: &str, make: &dyn Fn(&AppDef, PathBuf) -> Option<Step>| -> Vec<Step> {
            apps.iter()
                .find(|a| a.id == id)
                .and_then(|a| make(a, self.checkout(id)?))
                .into_iter()
                .collect()
        };
        let all = |make: &dyn Fn(&AppDef, PathBuf) -> Option<Step>| -> Vec<Step> {
            apps.iter()
                .filter_map(|a| make(a, self.checkout(&a.id)?))
                .collect()
        };
        let run = match action {
            DevAction::Release(id) => Some(one(&id, &release_step)),
            DevAction::Install(id) => Some(one(&id, &install_step)),
            DevAction::ReleaseAll => Some(all(&release_step)),
            DevAction::InstallAll => Some(all(&install_step)),
            DevAction::Clone(id) => {
                let others: Vec<PathBuf> =
                    self.repos.values().filter_map(|(d, _, _)| d.clone()).collect();
                let app = apps.iter().find(|a| a.id == id);
                match (app, app.and_then(|a| dev::default_dir(a, &others))) {
                    (Some(app), Some(target)) => {
                        let parent = target.parent().map(PathBuf::from).unwrap_or_default();
                        let _ = std::fs::create_dir_all(&parent);
                        let repo = app.dev.as_ref().map(|d| d.repo.clone()).unwrap_or_default();
                        self.settings.repos.insert(id, target.display().to_string());
                        self.settings.save();
                        Some(vec![Step {
                            name: format!("Clone {} into {}", app.name, target.display()),
                            dir: parent,
                            command: format!(
                                "git clone https://github.com/{repo}.git {}",
                                dev::quote(&target.display().to_string())
                            ),
                        }])
                    }
                    _ => None,
                }
            }
            DevAction::EditPath(id) => {
                let cur = self
                    .repos
                    .get(&id)
                    .and_then(|(d, _, _)| d.as_ref())
                    .map(|p| p.display().to_string())
                    .unwrap_or_default();
                self.path_edit = Some((id, cur));
                None
            }
            DevAction::SetPath(id, text) => {
                let t = text.trim();
                if t.is_empty() {
                    self.settings.repos.remove(&id);
                } else {
                    let p = match (t.strip_prefix("~/"), dirs::home_dir()) {
                        (Some(rest), Some(home)) => home.join(rest),
                        _ => PathBuf::from(t),
                    };
                    self.settings.repos.insert(id, p.display().to_string());
                }
                self.settings.save();
                self.path_edit = None;
                None
            }
            DevAction::CancelPath => {
                self.path_edit = None;
                None
            }
            DevAction::Delete(id) => {
                self.delete_ask = Some(id);
                None
            }
            DevAction::DeleteCancel => {
                self.delete_ask = None;
                None
            }
            DevAction::DeleteConfirm(id) => {
                self.delete_ask = None;
                if let Some(dir) = self.repos.get(&id).and_then(|(d, _, _)| d.clone()) {
                    self.deleting = Some(id.clone());
                    let (tx, c) = (self.tx.clone(), ctx.clone());
                    std::thread::spawn(move || {
                        let r = dev::delete_checkout(&dir)
                            .map(|()| format!("Deleted {}.", short_path(&dir)));
                        let _ = tx.send(Msg::Deleted(id, r));
                        c.request_repaint();
                    });
                }
                None
            }
        };
        if let Some(steps) = run {
            self.dev_message = Some(if steps.is_empty() {
                ("Nothing to do: no source folder found.".into(), true)
            } else {
                let title = if steps.len() == 1 {
                    steps[0].name.clone()
                } else if steps[0].name.starts_with("Release") {
                    "Release all apps".into()
                } else {
                    "Install all apps from source".into()
                };
                match dev::open_terminal(&title, &steps) {
                    Ok(()) => (format!("Opened a terminal: {title}"), false),
                    Err(e) => (e, true),
                }
            });
        }
        self.scan_repos(ctx);
    }

    fn start_remove(&mut self, id: &str, ctx: &egui::Context) {
        let tx = self.tx.clone();
        let Some(c) = self.card_mut(id) else { return };
        let Some(inst) = c.installed.clone() else {
            return;
        };
        c.confirm_remove = false;
        c.job = Some(Job::Remove);
        let (def, cx) = (c.def.clone(), ctx.clone());
        std::thread::spawn(move || {
            let r = system::uninstall(&def, &inst);
            let _ = tx.send(Msg::Removed(def.id.clone(), r));
            cx.request_repaint();
        });
    }

    fn start_install(&mut self, id: &str, ctx: &egui::Context) {
        let tx = self.tx.clone();
        let Some(c) = self.card_mut(id) else { return };
        let Some(Ok(Some(rel))) = c.latest.clone() else {
            return;
        };
        let Some(asset) = system::pick_asset(&c.def, &rel).cloned() else {
            return;
        };
        c.job = Some(Job::Download(0, asset.size));
        c.message = None;
        let (def, cx) = (c.def.clone(), ctx.clone());
        std::thread::spawn(move || {
            let id = def.id.clone();
            let tx2 = tx.clone();
            let cx2 = cx.clone();
            let progress = move |d, t| {
                let _ = tx2.send(Msg::Progress(id.clone(), d, t));
                cx2.request_repaint();
            };
            let result = github::download(&asset, &crate::paths::download_dir(), &progress)
                .and_then(|file| {
                    let _ = tx.send(Msg::Installing(def.id.clone()));
                    cx.request_repaint();
                    let r = system::install(&def, &file, &rel.version);
                    let _ = std::fs::remove_file(&file);
                    r
                });
            let _ = tx.send(Msg::Finished(def.id.clone(), result));
            cx.request_repaint();
        });
    }

    fn start_self_update(&mut self, ctx: &egui::Context) {
        let Some(rel) = self.self_update.clone() else {
            return;
        };
        let me = self_def();
        let Some(asset) = system::pick_asset(&me, &rel).cloned() else {
            system::open_url(&rel.page);
            return;
        };
        self.self_job = Some(Job::Download(0, asset.size));
        self.self_error = None;
        let (tx, cx) = (self.tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            let tx2 = tx.clone();
            let cx2 = cx.clone();
            let progress = move |d, t| {
                let _ = tx2.send(Msg::Progress("showtime-suite".into(), d, t));
                cx2.request_repaint();
            };
            let r = github::download(&asset, &crate::paths::download_dir(), &progress).and_then(
                |file| {
                    let _ = tx.send(Msg::Installing("showtime-suite".into()));
                    cx.request_repaint();
                    self_install(&me, &file, &rel.version)
                },
            );
            match r {
                // The new version takes over: quit (it was started already).
                Ok(()) => std::process::exit(0),
                Err(e) => {
                    let _ = tx.send(Msg::Finished("showtime-suite".into(), Err(e)));
                    cx.request_repaint();
                }
            }
        });
    }
}

/// Showtime Suite in its own releases (for self-update).
fn self_def() -> AppDef {
    AppDef {
        id: "showtime-suite".into(),
        name: "Showtime Suite".into(),
        tagline: String::new(),
        color: String::new(),
        releases: SELF_RELEASES.into(),
        icon_url: None,
        macos: Some(catalog::MacDef {
            asset_suffix: ".dmg".into(),
            app: "Showtime Suite.app".into(),
        }),
        windows: Some(catalog::WinDef {
            asset_prefix: "Showtime-Suite-Setup-".into(),
            asset_suffix: ".exe".into(),
            inno_app_id: "{D832F8E4-E533-492A-BD70-E4C6692C12B6}".into(),
            exe: "showtime-suite.exe".into(),
        }),
        linux: Some(catalog::LinuxDef {
            asset_suffix: "-linux-x86_64.tar.gz".into(),
            bin: "showtime-suite".into(),
        }),
        dev: Some(catalog::DevDef {
            repo: "Cheter0410/showtime-suite".into(),
            folder: "showtime-suite".into(),
            release: Some("./release.sh".into()),
            install_macos: Some("bash install-mac.sh".into()),
            install_linux: Some("./install.sh".into()),
        }),
    }
}

/// Install a new Showtime Suite over this one and start it.
fn self_install(me: &AppDef, file: &std::path::Path, version: &str) -> Result<(), String> {
    if cfg!(target_os = "windows") {
        // The installer replaces the running program; it closes this one.
        std::process::Command::new(file)
            .args([
                "/SILENT",
                "/SUPPRESSMSGBOXES",
                "/NORESTART",
                "/CURRENTUSER",
                "/CLOSEAPPLICATIONS",
            ])
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Could not run the installer: {e}"))
    } else {
        system::install_files(me, file, version)?;
        if let Some(inst) = system::installed(me) {
            // Started after this process has quit.
            let path = inst.path.display().to_string();
            let cmd = if cfg!(target_os = "macos") {
                format!("sleep 1; open -n \"{path}\"")
            } else {
                format!("sleep 1; \"{path}\" &")
            };
            let _ = std::process::Command::new("sh").args(["-c", &cmd]).spawn();
        }
        Ok(())
    }
}

fn texture(ctx: &egui::Context, id: &str, bytes: &[u8]) -> Option<egui::TextureHandle> {
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    let color = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
    Some(ctx.load_texture(format!("app-{id}"), color, egui::TextureOptions::LINEAR))
}

fn style(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = CARD;
    v.override_text_color = Some(TEXT);
    v.widgets.inactive.weak_bg_fill = Color32::from_rgb(36, 41, 50);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(48, 54, 65);
    v.widgets.active.weak_bg_fill = Color32::from_rgb(58, 65, 78);
    ctx.set_visuals(v);
    let mut s = (*ctx.global_style()).clone();
    s.spacing.button_padding = Vec2::new(14.0, 7.0);
    s.spacing.item_spacing = Vec2::new(10.0, 8.0);
    ctx.set_global_style(s);
}

fn os_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "macOS"
    } else if cfg!(target_os = "windows") {
        "Windows"
    } else {
        "Linux"
    }
}

fn fmt_mb(b: u64) -> String {
    format!("{:.1} MB", b as f64 / 1_048_576.0)
}

/// What a card's buttons offer.
enum Action {
    Install,
    Open,
    Notes,
    Page(String),
    /// Show / hide the "remove?" question.
    AskRemove(bool),
    Remove,
}

impl eframe::App for Launcher {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.drain(&ctx);
        // Running / installed state changes outside Showtime Suite: look again.
        if self.last_detect.elapsed() > Duration::from_secs(4)
            && self.cards.iter().all(|c| c.job.is_none())
        {
            self.detect(&ctx);
        }
        ctx.request_repaint_after(Duration::from_secs(2));

        let root = ui.max_rect();
        ui.painter().rect_filled(root, 0.0, BG);
        // The stage: a lit banner across the top, the title on its dark side.
        let hero_h = (root.height() * 0.30).clamp(150.0, 220.0);
        let hero = Rect::from_min_size(root.min, vec2(root.width(), hero_h));
        let refresh = self.hero(ui, hero, &ctx);
        if refresh {
            self.refresh(&ctx);
        }
        let body = Rect::from_min_max(pos2(root.left(), hero.bottom()), root.max);
        let glow = self
            .glow
            .get_or_insert_with(|| ctx.load_texture("tile-glow", stage::glow_image(96), egui::TextureOptions::LINEAR))
            .id();

        let mut actions: Vec<(String, Action)> = Vec::new();
        let mut dev_actions: Vec<DevAction> = Vec::new();
        let mut self_go = false;
        let dev_on = self.settings.dev_mode && dev::available();
        if dev_on && self.last_repo_scan.is_none_or(|t| t.elapsed() > Duration::from_secs(10)) {
            self.scan_repos(&ctx);
        }
        let dev_apps = if dev_on { self.dev_apps() } else { Vec::new() };
        ui.scope_builder(egui::UiBuilder::new().max_rect(body), |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                Frame::new().inner_margin(Margin { left: 28, right: 28, top: 4, bottom: 20 }).show(ui, |ui| {
                    // A newer Showtime Suite.
                    if let Some(r) = self.self_update.clone() {
                        self_go = self_update_bar(ui, &r, self.self_job, self.self_error.as_deref());
                        ui.add_space(14.0);
                    }
                    section_title(ui, "SHOW TOOLS", None);
                    let gap = 16.0;
                    let avail = ui.available_width();
                    let cols = (((avail + gap) / (290.0 + gap)).floor() as usize).clamp(1, 3);
                    let w = ((avail - gap * (cols as f32 - 1.0)) / cols as f32).floor();
                    for row in self.cards.chunks(cols) {
                        ui.horizontal_top(|ui| {
                            ui.spacing_mut().item_spacing.x = gap;
                            for c in row {
                                ui.allocate_ui_with_layout(vec2(w, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                                    ui.set_width(w);
                                    if let Some(a) = tile(ui, c, glow) {
                                        actions.push((c.def.id.clone(), a));
                                    }
                                });
                            }
                        });
                        ui.add_space(gap);
                    }
                    if dev_on {
                        ui.add_space(8.0);
                        section_title(ui, "BACKSTAGE", Some("Developer: runs the projects' own scripts in a terminal"));
                        let del = DeleteState { ask: self.delete_ask.as_deref(), busy: self.deleting.as_deref() };
                        dev_section(ui, &dev_apps, &self.repos, &mut self.path_edit, del, &self.dev_message, &mut dev_actions);
                        ui.add_space(10.0);
                    }
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new("Apps are installed from their GitHub releases. A running app is never replaced.")
                            .small()
                            .color(MUTED),
                    );
                });
            });
        });
        if self_go {
            self.start_self_update(&ctx);
        }
        for a in dev_actions {
            self.dev_action(a, &ctx);
        }
        for (id, a) in actions {
            match a {
                Action::Install => self.start_install(&id, &ctx),
                Action::Open => {
                    if let Some(c) = self.card_mut(&id) {
                        if let Some(inst) = c.installed.clone() {
                            c.message = system::launch(&inst).err().map(|e| (e, true));
                        }
                    }
                }
                Action::Notes => {
                    if let Some(c) = self.card_mut(&id) {
                        c.notes_open = !c.notes_open;
                    }
                }
                Action::Page(url) => system::open_url(&url),
                Action::AskRemove(on) => {
                    if let Some(c) = self.card_mut(&id) {
                        c.confirm_remove = on;
                        c.message = None;
                    }
                }
                Action::Remove => self.start_remove(&id, &ctx),
            }
        }
    }
}

impl Launcher {
    /// The banner: stage lights on the right, title and what is going on on the left, the buttons
    /// top right. Returns whether "Check for updates" was clicked.
    fn hero(&mut self, ui: &mut egui::Ui, rect: Rect, ctx: &egui::Context) -> bool {
        let tex = self
            .banner
            .get_or_insert_with(|| ctx.load_texture("stage-banner", stage::banner_image(1600, 360), egui::TextureOptions::LINEAR));
        // Cover the banner area, keep the right side (the lights) in view.
        let [tw, th] = tex.size().map(|v| v as f32);
        let scale = (rect.width() / tw).max(rect.height() / th);
        let (vw, vh) = (rect.width() / scale / tw, rect.height() / scale / th);
        let uv = Rect::from_min_max(pos2(1.0 - vw, 0.5 - vh / 2.0), pos2(1.0, 0.5 + vh / 2.0));
        let p = ui.painter_at(rect);
        p.image(tex.id(), rect, uv, Color32::WHITE);
        // Darker towards the title, and fading into the window at the bottom.
        let mid = rect.left() + rect.width() * 0.62;
        gradient(&p, Rect::from_min_max(rect.min, pos2(mid, rect.bottom())), [stage::alpha(BG, 0.92), stage::alpha(BG, 0.0)], true);
        gradient(&p, Rect::from_min_max(pos2(rect.left(), rect.bottom() - 46.0), rect.max), [stage::alpha(BG, 0.0), BG], false);

        let mut refresh = false;
        let inner = rect.shrink2(vec2(28.0, 20.0));
        ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    ui.label(RichText::new(format!("v{}", env!("CARGO_PKG_VERSION"))).small().color(MUTED));
                    refresh = ui
                        .add(glass_button("Check for updates", false))
                        .on_hover_text("Look for new versions of every app")
                        .clicked();
                    if dev::available()
                        && ui
                            .add(glass_button("Developer", self.settings.dev_mode))
                            .on_hover_text("Release apps and install them from source")
                            .clicked()
                    {
                        self.settings.dev_mode = !self.settings.dev_mode;
                        self.settings.save();
                    }
                });
            });
            // Title and the board at the bottom of the banner.
            ui.add_space((ui.available_height() - 92.0).max(0.0));
            ui.horizontal(|ui| {
                if let Some(t) = texture_logo(ctx) {
                    ui.add(egui::Image::new(&t).fit_to_exact_size(Vec2::splat(52.0)));
                }
                ui.add_space(4.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(RichText::new("Showtime Suite").size(30.0).strong().color(TEXT));
                    ui.label(RichText::new("The show tools for stage and events – installed, up to date and ready.").color(MUTED));
                });
            });
            ui.add_space(10.0);
            // What is going on, like the board at front of house.
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                let installed = self.cards.iter().filter(|c| c.installed.is_some()).count();
                let running = self.cards.iter().filter(|c| c.running).count();
                let updates = self.cards.iter().filter(|c| has_update(c)).count();
                chip(ui, &format!("{} tools", self.cards.len()), MUTED);
                chip(ui, &format!("{installed} installed"), if installed > 0 { OK } else { MUTED });
                if running > 0 {
                    chip(ui, &format!("{running} running"), ERROR);
                }
                if updates > 0 {
                    chip(ui, &format!("{updates} update{} ready", if updates == 1 { "" } else { "s" }), AMBER);
                }
            });
        });
        refresh
    }
}

/// A two-colour gradient over `rect`, left to right (`across`) or top to bottom.
fn gradient(p: &egui::Painter, rect: Rect, [a, b]: [Color32; 2], across: bool) {
    let mut m = egui::Mesh::default();
    let (tl, tr, br, bl) = if across { (a, b, b, a) } else { (a, a, b, b) };
    for (pos, c) in [(rect.left_top(), tl), (rect.right_top(), tr), (rect.right_bottom(), br), (rect.left_bottom(), bl)] {
        m.colored_vertex(pos, c);
    }
    m.add_triangle(0, 1, 2);
    m.add_triangle(0, 2, 3);
    p.add(Shape::mesh(m));
}

/// A button that sits on the banner: dark glass, amber when switched on.
fn glass_button(text: &str, on: bool) -> egui::Button<'_> {
    let label = RichText::new(text).color(if on { Color32::BLACK } else { TEXT });
    egui::Button::new(if on { label.strong() } else { label })
        .fill(if on { AMBER } else { Color32::from_black_alpha(150) })
        .stroke(Stroke::new(1.0, if on { AMBER } else { Color32::from_white_alpha(40) }))
        .corner_radius(CornerRadius::same(16))
        .min_size(vec2(0.0, 30.0))
}

/// A small label with a coloured dot, on dark glass.
fn chip(ui: &mut egui::Ui, text: &str, color: Color32) {
    Frame::new()
        .fill(Color32::from_black_alpha(140))
        .stroke(Stroke::new(1.0, Color32::from_white_alpha(28)))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin { left: 10, right: 11, top: 4, bottom: 4 })
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                dot(ui, color);
                ui.label(RichText::new(text).size(12.5).color(TEXT));
            });
        });
}

/// A little lamp: a dot with a soft halo.
fn dot(ui: &mut egui::Ui, color: Color32) {
    let (r, _) = ui.allocate_exact_size(vec2(8.0, 8.0), egui::Sense::hover());
    ui.painter().circle_filled(r.center(), 6.0, stage::alpha(color, 0.18));
    ui.painter().circle_filled(r.center(), 3.5, color);
}

/// A small spaced heading with a hairline after it.
fn section_title(ui: &mut egui::Ui, text: &str, note: Option<&str>) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(text).size(11.5).strong().extra_letter_spacing(2.0).color(MUTED));
        if let Some(n) = note {
            ui.label(RichText::new(n).small().color(Color32::from_rgb(96, 102, 116)));
        }
        let r = ui.available_rect_before_wrap();
        let y = r.center().y;
        ui.painter().hline(r.left() + 6.0..=r.right(), y, Stroke::new(1.0, LINE));
    });
    ui.add_space(8.0);
}

/// "Showtime Suite x is available" with its button or progress. Returns whether to update now.
fn self_update_bar(ui: &mut egui::Ui, r: &Release, job: Option<Job>, error: Option<&str>) -> bool {
    let mut go = false;
    Frame::new()
        .fill(Color32::from_rgb(38, 30, 14))
        .stroke(Stroke::new(1.0, stage::alpha(AMBER, 0.6)))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::symmetric(16, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                dot(ui, AMBER);
                ui.label(RichText::new(format!("Showtime Suite {} is available.", r.version)).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| match job {
                    Some(Job::Download(d, t)) => {
                        ui.add(egui::ProgressBar::new(if t > 0 { d as f32 / t as f32 } else { 0.0 }).desired_width(180.0));
                    }
                    Some(Job::Install | Job::Remove) => {
                        ui.label("Installing…");
                        ui.spinner();
                    }
                    None => {
                        go = ui.add(primary_button("Update and restart", AMBER)).clicked();
                    }
                });
            });
            if let Some(e) = error {
                ui.label(RichText::new(e).color(ERROR));
            }
        });
    go
}

fn primary_button(text: &str, accent: Color32) -> egui::Button<'_> {
    egui::Button::new(RichText::new(text).strong().color(Color32::BLACK))
        .fill(accent)
        .corner_radius(CornerRadius::same(8))
        .min_size(vec2(112.0, 34.0))
}

fn has_update(c: &Card) -> bool {
    match (c.installed.as_ref().and_then(|i| i.version.as_deref()), &c.latest) {
        (Some(v), Some(Ok(Some(r)))) => github::is_newer(&r.version, v),
        _ => false,
    }
}

fn texture_logo(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    let id = egui::Id::unique("showtime-suite-logo");
    if let Some(t) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(id)) {
        return Some(t);
    }
    let t = texture(ctx, "showtime-suite", include_bytes!("../assets/icon.png"))?;
    ctx.data_mut(|d| d.insert_temp(id, t.clone()));
    Some(t)
}

/// One app as a tile: lit from above in its colour, icon and name, what it does, its state, and
/// what can be done. Returns what the user asked for.
fn tile(ui: &mut egui::Ui, c: &Card, glow: egui::TextureId) -> Option<Action> {
    let accent = catalog::parse_color(&c.def.color)
        .map(|[r, g, b]| Color32::from_rgb(r, g, b))
        .unwrap_or(OK);
    let supported = system::supported(&c.def);
    let release = match &c.latest {
        Some(Ok(Some(r))) => Some(r),
        _ => None,
    };
    let asset = release.and_then(|r| system::pick_asset(&c.def, r));
    let installed_version = c.installed.as_ref().and_then(|i| i.version.clone());
    let update = has_update(c);
    let mut action = None;
    // The background goes under the content once its size is known.
    let bg = ui.painter().add(Shape::Noop);
    let inner = Frame::new().inner_margin(Margin { left: 18, right: 18, top: 20, bottom: 16 }).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.set_min_height(214.0);
        ui.horizontal(|ui| {
            match &c.icon {
                Some(t) => {
                    ui.add(egui::Image::new(t).fit_to_exact_size(Vec2::splat(56.0)));
                }
                None => {
                    let (r, _) = ui.allocate_exact_size(Vec2::splat(56.0), egui::Sense::hover());
                    ui.painter().rect_filled(r, 14.0, accent);
                }
            }
            ui.add_space(4.0);
            ui.vertical(|ui| {
                ui.add_space(4.0);
                ui.label(RichText::new(&c.def.name).size(20.0).strong().color(TEXT));
                if c.running {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        dot(ui, ERROR);
                        ui.label(RichText::new("RUNNING").size(11.0).strong().extra_letter_spacing(1.5).color(ERROR));
                    });
                } else {
                    let (text, color) = match (&c.installed, update) {
                        (Some(_), true) => ("UPDATE READY", AMBER),
                        (Some(_), false) => ("INSTALLED", accent),
                        (None, _) => ("NOT INSTALLED", MUTED),
                    };
                    ui.label(RichText::new(text).size(11.0).strong().extra_letter_spacing(1.5).color(color));
                }
            });
        });
        ui.add_space(10.0);
        // The tagline gets the same room on every tile, so the buttons line up.
        ui.allocate_ui_with_layout(vec2(ui.available_width(), 54.0), egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.set_min_height(54.0);
            if !c.def.tagline.is_empty() {
                ui.add(egui::Label::new(RichText::new(&c.def.tagline).color(MUTED)).wrap());
            }
        });
        let (status, color) = status_line(c, supported, release, update, &installed_version);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 7.0;
            dot(ui, color);
            ui.add(egui::Label::new(RichText::new(status).size(12.5).color(color)).truncate());
        });
        ui.add_space(12.0);
        ui.horizontal(|ui| match c.job {
            Some(Job::Download(d, t)) => {
                let f = if t > 0 { d as f32 / t as f32 } else { 0.0 };
                ui.add(
                    egui::ProgressBar::new(f)
                        .desired_width(ui.available_width())
                        .fill(accent)
                        .text(RichText::new(format!("{} / {}", fmt_mb(d), fmt_mb(t))).color(Color32::BLACK)),
                );
            }
            Some(Job::Install) => {
                ui.spinner();
                ui.label("Installing…");
            }
            Some(Job::Remove) => {
                ui.spinner();
                ui.label("Removing…");
            }
            None if c.confirm_remove => {
                if ui.add(egui::Button::new(RichText::new("Remove").strong().color(Color32::WHITE)).fill(ERROR).min_size(vec2(96.0, 34.0))).clicked() {
                    action = Some(Action::Remove);
                }
                if ui.add(egui::Button::new("Cancel").min_size(vec2(0.0, 34.0))).clicked() {
                    action = Some(Action::AskRemove(false));
                }
            }
            None => {
                if c.installed.is_some() {
                    if update && asset.is_some() {
                        let b = ui
                            .add_enabled(!c.running, primary_button(&format!("Update to {}", release.map_or("", |r| r.version.as_str())), AMBER))
                            .on_disabled_hover_text(format!("Quit {} first - it is running.", c.def.name));
                        if b.clicked() {
                            action = Some(Action::Install);
                        }
                        if ui.add(egui::Button::new("Open").min_size(vec2(0.0, 34.0))).clicked() {
                            action = Some(Action::Open);
                        }
                    } else if ui.add(primary_button("Open", accent)).clicked() {
                        action = Some(Action::Open);
                    }
                } else if supported && asset.is_some() && ui.add(primary_button("Install", accent)).clicked() {
                    action = Some(Action::Install);
                }
            }
        });
        if c.job.is_none() && !c.confirm_remove {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().button_padding = vec2(0.0, 2.0);
                ui.spacing_mut().item_spacing.x = 18.0;
                if release.is_some_and(|r| !r.notes.is_empty())
                    && ui
                        .add(egui::Button::new(RichText::new("What's new").small().color(if c.notes_open { accent } else { MUTED })).frame(false))
                        .clicked()
                {
                    action = Some(Action::Notes);
                }
                if let Some(inst) = &c.installed {
                    let removable = system::removable(&c.def, inst);
                    let b = ui
                        .add_enabled(removable && !c.running, egui::Button::new(RichText::new("Remove…").small().color(MUTED)).frame(false))
                        .on_hover_text("Uninstall it from this computer")
                        .on_disabled_hover_text(if c.running {
                            format!("Quit {} first - it is running.", c.def.name)
                        } else {
                            format!("Installed by the system ({}) - remove it with the package manager.", inst.path.display())
                        });
                    if b.clicked() {
                        action = Some(Action::AskRemove(true));
                    }
                }
            });
        }
        if c.confirm_remove && c.job.is_none() {
            ui.add_space(6.0);
            ui.add(
                egui::Label::new(
                    RichText::new(format!("Remove {} from this computer? Its settings and your files (shows, presentations) stay.", c.def.name))
                        .color(WARN),
                )
                .wrap(),
            );
        }
        if let Some((m, err)) = &c.message {
            ui.add_space(6.0);
            ui.add(egui::Label::new(RichText::new(m).color(if *err { ERROR } else { OK })).wrap());
        }
        if c.notes_open {
            if let Some(r) = release {
                ui.add_space(8.0);
                ui.separator();
                ui.label(RichText::new(format!("Version {}", r.version)).strong());
                ui.add(egui::Label::new(RichText::new(&r.notes).color(MUTED)).wrap());
                if !r.page.is_empty() && ui.link("Release page").clicked() {
                    action = Some(Action::Page(r.page.clone()));
                }
            }
        }
    });
    let r = inner.response.rect;
    let hovered = ui.rect_contains_pointer(r);
    let radius = CornerRadius::same(14);
    let light = if c.running || hovered { 0.30 } else { 0.20 };
    let glow_rect = Rect::from_min_max(r.min + vec2(1.0, 1.0), pos2(r.right() - 1.0, r.top() + r.height().min(260.0)));
    ui.painter().set(
        bg,
        Shape::Vec(vec![
            Shape::rect_filled(r, radius, CARD),
            Shape::image(glow, glow_rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), stage::alpha(accent, light)),
            // The lamp: a strip of light along the top edge.
            Shape::rect_filled(Rect::from_min_max(pos2(r.left() + 22.0, r.top()), pos2(r.right() - 22.0, r.top() + 3.0)), CornerRadius::same(2), accent),
            Shape::rect_stroke(
                r,
                radius,
                Stroke::new(1.0, if hovered { stage::alpha(accent, 0.55) } else { LINE }),
                egui::StrokeKind::Inside,
            ),
        ]),
    );
    action
}

fn status_line(
    c: &Card,
    supported: bool,
    release: Option<&Release>,
    update: bool,
    installed_version: &Option<String>,
) -> (String, Color32) {
    let latest = match &c.latest {
        None => "checking…".to_string(),
        Some(Ok(Some(r))) => format!("latest {}", r.version),
        Some(Ok(None)) => "no release published yet".to_string(),
        Some(Err(e)) => format!("can't check: {e}"),
    };
    let asset_missing = release.is_some()
        && release
            .and_then(|r| system::pick_asset(&c.def, r))
            .is_none();
    match (&c.installed, supported) {
        _ if !c.detected => ("looking…".into(), MUTED),
        (Some(_), _) if update => (
            format!(
                "Installed {} · update to {} available",
                installed_version.as_deref().unwrap_or("?"),
                release.map_or("", |r| r.version.as_str())
            ),
            WARN,
        ),
        (Some(_), _) => (
            match (installed_version, release) {
                (Some(v), Some(_)) => format!("Installed {v} · up to date"),
                (Some(v), None) => format!("Installed {v} · {latest}"),
                (None, _) => format!("Installed · {latest}"),
            },
            OK,
        ),
        (None, false) => (format!("Not available for {} yet", os_name()), MUTED),
        (None, true) if asset_missing => (
            format!("{latest} · no download for {} in this release", os_name()),
            MUTED,
        ),
        (None, true) => (format!("Not installed · {latest}"), MUTED),
    }
}

/// The developer section: per app its source folder and git state, with
/// Release / Install (or Clone), and both for all apps at once.
/// Deleting a checkout: which app is asked about / being deleted.
#[derive(Clone, Copy)]
struct DeleteState<'a> {
    ask: Option<&'a str>,
    busy: Option<&'a str>,
}

fn dev_section(
    ui: &mut egui::Ui,
    apps: &[AppDef],
    repos: &HashMap<String, RepoEntry>,
    path_edit: &mut Option<(String, String)>,
    del: DeleteState,
    message: &Option<(String, bool)>,
    out: &mut Vec<DevAction>,
) {
    Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0, LINE))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(Margin::symmetric(18, 14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            for (i, a) in apps.iter().enumerate() {
                let (dir, state, problem) = repos.get(&a.id).cloned().unwrap_or((None, None, None));
                let accent = catalog::parse_color(&a.color)
                    .map(|[r, g, b]| Color32::from_rgb(r, g, b))
                    .unwrap_or(OK);
                if i > 0 {
                    ui.add_space(4.0);
                    ui.separator();
                }
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(Vec2::new(4.0, 34.0), egui::Sense::hover());
                    ui.painter().rect_filled(r, 2.0, accent);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let d = a.dev.as_ref();
                        if del.busy == Some(a.id.as_str()) {
                            ui.label(RichText::new("Deleting…").color(MUTED));
                            ui.spinner();
                        } else if del.ask == Some(a.id.as_str()) {
                            if ui.button("Cancel").clicked() {
                                out.push(DevAction::DeleteCancel);
                            }
                            if ui
                                .add(egui::Button::new(RichText::new("Delete").strong().color(Color32::WHITE)).fill(ERROR))
                                .clicked()
                            {
                                out.push(DevAction::DeleteConfirm(a.id.clone()));
                            }
                        } else if dir.is_some() && state.is_some() {
                            if dev::install_command(a).is_some()
                                && ui
                                    .button("Install")
                                    .on_hover_text("Get the latest source, build it and install it on this computer")
                                    .clicked()
                            {
                                out.push(DevAction::Install(a.id.clone()));
                            }
                            if d.and_then(|d| d.release.as_ref()).is_some()
                                && ui
                                    .add(egui::Button::new(RichText::new("Release").strong().color(Color32::BLACK)).fill(accent))
                                    .on_hover_text("Build a new version and upload it (release.sh)")
                                    .clicked()
                            {
                                out.push(DevAction::Release(a.id.clone()));
                            }
                            if ui
                                .add(egui::Button::new(RichText::new("Delete…").small().color(MUTED)).frame(false))
                                .on_hover_text("Delete this source folder (with its build files) from this computer")
                                .clicked()
                            {
                                out.push(DevAction::Delete(a.id.clone()));
                            }
                        } else if d.is_some()
                            && dir.is_none()
                            && ui.button("Clone").on_hover_text("Get the source from GitHub").clicked()
                        {
                            out.push(DevAction::Clone(a.id.clone()));
                        }
                        ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                            ui.label(RichText::new(&a.name).strong());
                            match path_edit.as_mut().filter(|(id, _)| id == &a.id) {
                                Some((_, text)) => {
                                    ui.horizontal(|ui| {
                                        let r = ui.add(
                                            egui::TextEdit::singleline(text)
                                                .hint_text("~/dev/…  (empty = search automatically)")
                                                .desired_width((ui.available_width() - 120.0).max(120.0)),
                                        );
                                        let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                                        if ui.small_button("OK").clicked() || enter {
                                            out.push(DevAction::SetPath(a.id.clone(), text.clone()));
                                        } else if ui.small_button("Cancel").clicked() {
                                            out.push(DevAction::CancelPath);
                                        }
                                    });
                                }
                                None if del.ask == Some(a.id.as_str()) => {
                                    let place = dir.as_deref().map(short_path).unwrap_or_default();
                                    let (text, color) = match state.as_ref().and_then(RepoState::would_lose) {
                                        Some(lost) => (format!("Delete {place}? Lost for good: {lost}."), ERROR),
                                        None => (
                                            format!("Delete {place}? Everything in it is on GitHub; Clone gets it back."),
                                            WARN,
                                        ),
                                    };
                                    ui.add(egui::Label::new(RichText::new(text).small().color(color)).wrap());
                                }
                                None => {
                                    ui.horizontal(|ui| {
                                        if ui
                                            .add(egui::Button::new(RichText::new("Folder…").small().color(MUTED)).small())
                                            .on_hover_text("Use another source folder")
                                            .clicked()
                                        {
                                            out.push(DevAction::EditPath(a.id.clone()));
                                        }
                                        let (text, color) = match (&dir, &state) {
                                            (Some(d), Some(s)) => {
                                                let mut t = format!("{} · {}", short_path(d), s.branch);
                                                if s.changes > 0 {
                                                    t.push_str(&format!(" · {} changed file(s)", s.changes));
                                                }
                                                if s.behind > 0 {
                                                    t.push_str(&format!(" · {} new commit(s) on GitHub", s.behind));
                                                }
                                                (t, if s.changes > 0 { WARN } else { MUTED })
                                            }
                                            (Some(d), None) => (
                                                format!(
                                                    "{} · git: {}",
                                                    short_path(d),
                                                    problem.as_deref().unwrap_or("not a usable checkout")
                                                ),
                                                ERROR,
                                            ),
                                            (None, _) => ("source not on this computer".into(), MUTED),
                                        };
                                        let label = ui.add(egui::Label::new(RichText::new(&text).small().color(color)).truncate());
                                        if dir.is_some() && state.is_none() {
                                            label.on_hover_text(format!(
                                                "{text}\n\nRun git in that folder in a terminal to see more. A folder that belongs to another user: sudo chown -R \"$USER\": <folder>"
                                            ));
                                        }
                                    });
                                }
                            }
                        });
                    });
                });
            }
            ui.add_space(4.0);
            ui.separator();
            ui.horizontal(|ui| {
                let any = apps
                    .iter()
                    .any(|a| repos.get(&a.id).is_some_and(|(d, s, _)| d.is_some() && s.is_some()));
                if ui
                    .add_enabled(any, egui::Button::new("Release all"))
                    .on_hover_text("One terminal that releases every app in turn (Showtime Suite last)")
                    .clicked()
                {
                    out.push(DevAction::ReleaseAll);
                }
                if ui
                    .add_enabled(any, egui::Button::new("Install all"))
                    .on_hover_text("One terminal that builds and installs every app from source (Showtime Suite last)")
                    .clicked()
                {
                    out.push(DevAction::InstallAll);
                }
                if let Some((m, err)) = message {
                    ui.label(RichText::new(m).small().color(if *err { ERROR } else { OK }));
                }
            });
        });
}

/// A folder with the home folder shown as `~`.
fn short_path(p: &std::path::Path) -> String {
    match dirs::home_dir().and_then(|h| p.strip_prefix(h).ok().map(PathBuf::from)) {
        Some(rest) => format!("~/{}", rest.display()),
        None => p.display().to_string(),
    }
}
