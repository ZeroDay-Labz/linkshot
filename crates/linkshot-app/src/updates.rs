//! "Check for Updates": a small window that reports a newer GitHub release and installs it (see
//! `linkshot_platform::update`). Checks happen only when the user asks, or at start-up when they
//! turned that on; network work runs on a worker thread.

use crate::theme::Palette;
use crossbeam_channel::{Receiver, Sender};
use egui::{Align2, Color32, FontId, Pos2, Rect, Stroke, Vec2};
use linkshot_platform::update::{self, InstallKind, Release};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub const TITLE: &str = "LinkShot Update";
pub const SIZE: [f32; 2] = [430.0, 132.0];

enum Msg {
    Checked(anyhow::Result<Release>),
    Progress(u64, u64),
    Verifying,
    Installing,
    Installed,
    Failed(String),
}

#[derive(Clone)]
enum State {
    Hidden,
    Checking,
    UpToDate,
    Available(Release),
    Downloading { release: Release, got: u64, total: u64 },
    Verifying(Release),
    Installing(Release),
    Installed(Release),
    Failed(String, Option<String>),
}

/// What the window asks the app to do.
pub enum Request {
    /// Remember that this release was skipped.
    Skip(String),
    /// The update is installed: start the new version and quit.
    Restart(InstallKind),
}

pub struct Updater {
    state: State,
    /// The check was started by the user (show "up to date" and errors), not at start-up.
    manual: bool,
    kind: InstallKind,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    cancel: Arc<AtomicBool>,
    /// The window was placed on KDE Wayland (where clients cannot position themselves).
    placed: bool,
    /// Wakes the UI loop from the worker thread.
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl Default for Updater {
    fn default() -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        Self { state: State::Hidden, manual: false, kind: InstallKind::Unmanaged, tx, rx, cancel: Arc::new(AtomicBool::new(false)), placed: false, wake: None }
    }
}

impl Updater {
    /// Puts the window into a sample state for the headless screenshots (`--shots`).
    pub fn preview(&mut self, which: &str) {
        let release = Release { version: semver::Version::new(0, 6, 0), tag: "v0.6.0".into(), page_url: String::new(), notes: String::new(), assets: vec![] };
        self.kind = InstallKind::Rpm;
        self.state = match which {
            "available" => State::Available(release),
            "downloading" => State::Downloading { release, got: 38_000_000, total: 90_000_000 },
            "installed" => State::Installed(release),
            _ => State::UpToDate,
        };
    }

    pub fn visible(&self) -> bool { !matches!(self.state, State::Hidden) }
    fn busy(&self) -> bool { matches!(self.state, State::Checking | State::Downloading { .. } | State::Verifying(_) | State::Installing(_)) }

    /// Looks for a newer release. `manual`: the user asked, so the window opens right away and also
    /// reports "up to date" and failures; otherwise it only appears when there is something to
    /// install that the user has not skipped.
    pub fn check(&mut self, manual: bool, skipped: &str, wake: Arc<dyn Fn() + Send + Sync>) {
        if self.busy() { return; }
        self.manual = manual;
        self.kind = update::install_kind();
        self.wake = Some(wake.clone());
        self.placed = false;
        self.state = if manual { State::Checking } else { State::Hidden };
        let tx = self.tx.clone();
        let skipped = skipped.to_string();
        std::thread::Builder::new().name("update-check".into()).spawn(move || {
            let r = update::latest_release().and_then(|r| if !manual && r.tag == skipped { anyhow::bail!("skipped") } else { Ok(r) });
            let _ = tx.send(Msg::Checked(r));
            wake();
        }).ok();
    }

    fn start_install(&mut self, release: Release) {
        let Some(asset) = release.asset_for(self.kind).cloned() else {
            self.state = State::Failed(format!("This release has no package for this system ({}).", self.kind.label()), Some(release.page_url.clone()));
            return;
        };
        self.cancel.store(false, Ordering::Relaxed);
        self.state = State::Downloading { release: release.clone(), got: 0, total: asset.size };
        let (tx, cancel, kind, wake) = (self.tx.clone(), self.cancel.clone(), self.kind, self.wake.clone());
        std::thread::Builder::new().name("update-install".into()).spawn(move || {
            let wake = move || { if let Some(w) = &wake { w() } };
            let progress_tx = tx.clone();
            let mut last_pct = u64::MAX;
            let result = update::download(&asset, &update::download_dir(), |got, total| {
                let pct = if total > 0 { got * 100 / total } else { 0 };
                if pct != last_pct { last_pct = pct; let _ = progress_tx.send(Msg::Progress(got, total)); wake(); }
            }, &cancel)
            .and_then(|file| { let _ = tx.send(Msg::Verifying); wake(); update::verify(&release, &asset, &file).map(|_| file) })
            .and_then(|file| { let _ = tx.send(Msg::Installing); wake(); update::install(kind, &file) });
            let _ = tx.send(match result { Ok(()) => Msg::Installed, Err(e) => Msg::Failed(format!("{e:#}")) });
            wake();
        }).ok();
    }

    fn poll(&mut self) {
        for msg in self.rx.try_iter().collect::<Vec<_>>() {
            self.state = match (msg, std::mem::replace(&mut self.state, State::Hidden)) {
                (Msg::Checked(Ok(r)), _) if r.is_newer() => State::Available(r),
                (Msg::Checked(Ok(_)), _) => if self.manual { State::UpToDate } else { State::Hidden },
                (Msg::Checked(Err(e)), _) => {
                    tracing::info!("update check: {e:#}");
                    if self.manual { State::Failed(format!("Could not check for updates: {e:#}"), Some(format!("https://github.com/{}/releases", update::REPO))) } else { State::Hidden }
                }
                (Msg::Progress(got, total), State::Downloading { release, .. }) => State::Downloading { release, got, total },
                (Msg::Verifying, State::Downloading { release, .. }) => State::Verifying(release),
                (Msg::Installing, State::Verifying(r)) => State::Installing(r),
                (Msg::Installed, State::Installing(r)) => State::Installed(r),
                (Msg::Failed(e), s) => {
                    let page = match &s { State::Downloading { release, .. } | State::Verifying(release) | State::Installing(release) => Some(release.page_url.clone()), _ => None };
                    if e == "cancelled" { State::Hidden } else { State::Failed(format!("The update failed: {e}"), page) }
                }
                (_, s) => s,
            };
        }
    }

    /// Shows the window while there is something to report. Returns what the user asked for.
    pub fn ui(&mut self, ctx: &egui::Context, pal: &Palette, pos: Option<[f32; 2]>) -> Option<Request> {
        self.poll();
        if !self.visible() { return None; }
        #[cfg(target_os = "linux")]
        if let (false, Some(p)) = (self.placed, pos) {
            if linkshot_capture::is_wayland() && linkshot_capture::kwin::is_kde() {
                self.placed = true;
                std::thread::spawn(move || { std::thread::sleep(Duration::from_millis(150)); if let Err(e) = linkshot_capture::kwin::setup_overlay_window(TITLE, p[0].round() as i32, p[1].round() as i32, SIZE[0] as i32, SIZE[1] as i32) { tracing::debug!("update window placement: {e:#}"); } });
            }
        }
        let mut builder = egui::ViewportBuilder::default().with_title(TITLE).with_app_id(crate::app_id()).with_decorations(false).with_transparent(true)
            .with_always_on_top().with_taskbar(false).with_resizable(false).with_inner_size(SIZE);
        if let Some(p) = pos { builder = builder.with_position(p); }
        let mut request = None;
        let state = self.state.clone();
        let current = update::current_version();
        let kind = self.kind;
        let mut next: Option<State> = None;
        let mut install: Option<Release> = None;
        ctx.show_viewport_immediate(egui::ViewportId::from_hash_of("update-window"), builder, |ctx, _class| {
            egui::CentralPanel::default().frame(egui::Frame::NONE.fill(Color32::TRANSPARENT)).show(ctx, |ui| {
                let r = ui.max_rect().shrink(2.0);
                let painter = ui.painter().clone();
                painter.rect_filled(r, 12.0, pal.dark_panel);
                painter.rect_stroke(r, 12.0, Stroke::new(1.0_f32, pal.control_outline), egui::StrokeKind::Inside);
                // Press anywhere on the background to move the window (the buttons, drawn later, keep their clicks).
                crate::widgets::title_bar_drag(ui, r, ui.id().with("update-drag"), false);
                // Icon: a circle with an arrow pointing down.
                let ic = Pos2::new(r.left() + 30.0, r.top() + 34.0);
                let accent = match &state { State::Failed(..) => pal.capture_red, State::Installed(_) | State::UpToDate => pal.green, _ => pal.product_bright };
                painter.circle_filled(ic, 15.0, accent);
                painter.line_segment([ic + Vec2::new(0.0, -7.0), ic + Vec2::new(0.0, 6.0)], Stroke::new(2.2_f32, Color32::WHITE));
                painter.line_segment([ic + Vec2::new(-5.0, 1.5), ic + Vec2::new(0.0, 6.5)], Stroke::new(2.2_f32, Color32::WHITE));
                painter.line_segment([ic + Vec2::new(5.0, 1.5), ic + Vec2::new(0.0, 6.5)], Stroke::new(2.2_f32, Color32::WHITE));

                let (title, detail): (String, String) = match &state {
                    State::Hidden => (String::new(), String::new()),
                    State::Checking => ("Checking for updates…".into(), format!("LinkShot {current}")),
                    State::UpToDate => ("LinkShot is up to date".into(), format!("You have the newest version, {current}.")),
                    State::Available(rel) => (format!("LinkShot {} is available", rel.version), if kind == InstallKind::Unmanaged { format!("You have {current} (a development build): download it from the release page.") } else { format!("You have {current}. The new {} is downloaded from GitHub.", kind.label()) }),
                    State::Downloading { release, got, total } => (format!("Downloading LinkShot {}…", release.version), if *total > 0 { format!("{:.1} of {:.1} MB", *got as f64 / 1e6, *total as f64 / 1e6) } else { format!("{:.1} MB", *got as f64 / 1e6) }),
                    State::Verifying(rel) => (format!("Verifying LinkShot {}…", rel.version), "Checking the download against the release checksums.".into()),
                    State::Installing(rel) => (format!("Installing LinkShot {}…", rel.version), if matches!(kind, InstallKind::Rpm | InstallKind::Deb) { "Enter your password in the system prompt to continue.".into() } else { "This takes a moment.".into() }),
                    State::Installed(rel) => (format!("LinkShot {} is installed", rel.version), "Restart LinkShot to start using it.".into()),
                    State::Failed(msg, _) => ("Update failed".into(), msg.clone()),
                };
                let text_left = r.left() + 58.0;
                painter.text(Pos2::new(text_left, r.top() + 24.0), Align2::LEFT_CENTER, &title, crate::theme::semibold(14.5), pal.text_bright);
                let galley = painter.layout(detail, FontId::proportional(12.0), pal.text, r.right() - text_left - 14.0);
                painter.galley(Pos2::new(text_left, r.top() + 38.0), galley, pal.text);

                // Progress bar while downloading.
                if let State::Downloading { got, total, .. } = &state {
                    let bar = Rect::from_min_size(Pos2::new(text_left, r.bottom() - 52.0), Vec2::new(r.right() - text_left - 14.0, 6.0));
                    painter.rect_filled(bar, 3.0, pal.control);
                    let f = if *total > 0 { (*got as f32 / *total as f32).clamp(0.0, 1.0) } else { 0.0 };
                    painter.rect_filled(Rect::from_min_size(bar.min, Vec2::new(bar.width() * f, bar.height())), 3.0, pal.product_bright);
                }

                // Buttons, right-aligned along the bottom.
                let row = Rect::from_min_max(Pos2::new(r.left() + 12.0, r.bottom() - 40.0), Pos2::new(r.right() - 12.0, r.bottom() - 10.0));
                ui.scope_builder(egui::UiBuilder::new().max_rect(row).layout(egui::Layout::right_to_left(egui::Align::Center)), |ui| {
                    let primary = |ui: &mut egui::Ui, text: &str| ui.add(egui::Button::new(egui::RichText::new(text).color(Color32::WHITE)).fill(pal.product).min_size(Vec2::new(0.0, 26.0))).clicked();
                    let normal = |ui: &mut egui::Ui, text: &str| ui.add(egui::Button::new(text).min_size(Vec2::new(0.0, 26.0))).clicked();
                    match &state {
                        State::Checking | State::Verifying(_) | State::Installing(_) => { ui.add(egui::Spinner::new().size(18.0)); }
                        State::UpToDate => { if primary(ui, "OK") { next = Some(State::Hidden); } }
                        State::Available(rel) => {
                            if kind == InstallKind::Unmanaged {
                                if primary(ui, "Open Release Page") { let _ = linkshot_platform::open_url(&rel.page_url); next = Some(State::Hidden); }
                            } else if primary(ui, "Update Now") { install = Some(rel.clone()); }
                            if normal(ui, "Later") { next = Some(State::Hidden); }
                            if normal(ui, "Skip This Version") { request = Some(Request::Skip(rel.tag.clone())); next = Some(State::Hidden); }
                            if normal(ui, "What's New") { let _ = linkshot_platform::open_url(&rel.page_url); }
                        }
                        State::Downloading { .. } => { if normal(ui, "Cancel") { self.cancel.store(true, Ordering::Relaxed); } }
                        State::Installed(_) => {
                            if primary(ui, "Restart Now") { request = Some(Request::Restart(kind)); next = Some(State::Hidden); }
                            if normal(ui, "Later") { next = Some(State::Hidden); }
                        }
                        State::Failed(_, page) => {
                            if primary(ui, "Close") { next = Some(State::Hidden); }
                            if let Some(p) = page { if normal(ui, "Open Release Page") { let _ = linkshot_platform::open_url(p); next = Some(State::Hidden); } }
                        }
                        State::Hidden => {}
                    }
                });
            });
            if matches!(state, State::Checking | State::Downloading { .. } | State::Verifying(_) | State::Installing(_)) { ctx.request_repaint_after(Duration::from_millis(100)); }
        });
        if let Some(rel) = install { self.start_install(rel); }
        if let Some(s) = next { self.state = s; }
        request
    }
}
