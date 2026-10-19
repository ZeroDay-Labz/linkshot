//! Central application state and the capture → editor flow.

use crate::capture_window::CaptureWindowState;
use crate::editor::EditorState;
use crate::hotkeys::{HotkeyEvent, Hotkeys};
use crate::overlay::{Overlay, OverlayResult};
use crate::scrolling_hud::ScrollingUi;
use crate::recording_hud::RecordingUi;
use crate::grab_text::GrabTextUi;
use crate::theme::{self, Palette};
use crate::tray_icon::{Tray, TrayEvent};
use linkshot_core::document::{CaptureObject, Document};
use linkshot_core::enums::*;
use linkshot_core::presets::{CaptureSettings, SavedPresets};
use linkshot_core::settings::{Paths, Settings};
use linkshot_core::style::Theme;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

static SIMPLIFIED_CROSSHAIR: AtomicBool = AtomicBool::new(false);
pub fn is_simplified_crosshair() -> bool { SIMPLIFIED_CROSSHAIR.load(Ordering::Relaxed) }

#[derive(Clone, Debug)]
pub enum Action {
    StartCapture(CaptureSettings),
    RepeatLastCapture,
    OpenEditor,
    HideCaptureWindow,
    ShowCaptureWindow,
    ToggleCaptureWindow,
    ShowPreferences,
    ShowAbout,
    SetTheme(linkshot_core::settings::UiTheme),
    ResetCurrentSettings,
    AddPresetFromCurrent,
    AddPresetNew,
    EditPreset(String),
    /// Write the tab's settings back into the preset being edited.
    SavePresetEdits,
    CancelPresetEdit,
    DuplicatePreset(String),
    DeletePreset(String),
    ExportPreset(String),
    ImportPresets,
    ExportPresets,
    Quit,
}

struct PendingCapture {
    settings: CaptureSettings,
    due: Instant,
    /// The windows have been hidden for the capture (`hide_linkshot_during_capture`).
    hidden: bool,
    /// Show the "Capturing in N" bubble (Delay captures of a second or more).
    countdown: bool,
    /// Bottom-right corner position for the bubble (logical coordinates of the primary monitor).
    bubble_pos: Option<[f32; 2]>,
    bubble_placed: bool,
}

struct LiveCapture { rx: crossbeam_channel::Receiver<anyhow::Result<image::RgbaImage>>, cursor: Option<linkshot_core::Pt>, settings: CaptureSettings, started: Instant }

const COUNTDOWN_TITLE: &str = "LinkShot Countdown";
const COUNTDOWN_W: f32 = 170.0;
const COUNTDOWN_H: f32 = 64.0;

/// Viewport id of the Capture window (a child of the invisible root, see `keepalive.rs`).
pub fn capture_viewport_id() -> egui::ViewportId { egui::ViewportId::from_hash_of("capture-window") }

pub struct App {
    pub paths: Paths,
    pub settings: Settings,
    pub presets: SavedPresets,
    pub themes: Vec<Theme>,
    pub pal: Palette,
    pub capture: CaptureWindowState,
    pub editor: EditorState,
    pub overlay: Option<Overlay>,
    pub scrolling: Option<ScrollingUi>,
    pub recording: Option<RecordingUi>,
    pub grab_text: Option<GrabTextUi>,
    pub ctx: egui::Context,
    pending: Option<PendingCapture>,
    pub capture_visible: bool,
    pub editor_visible: bool,
    /// Editor geometry chosen when its viewport opened. egui turns any change in a viewport's builder
    /// into a move/resize command, so feeding back the remembered rect every frame fights the user's drag.
    editor_open_geometry: Option<(Option<[f32; 2]>, [f32; 2], [f32; 2])>,
    pub capture_toggled: bool,
    pub hotkeys: Hotkeys,
    pub tray: Tray,
    instance: crate::single_instance::Guard,
    keepalive: crate::keepalive::Keepalive,
    headless: bool,
    pub last_capture: Option<CaptureSettings>,
    presets_dirty: bool,
    settings_dirty: bool,
    last_save: Instant,
    pub show_prefs: bool,
    pub show_about: bool,
    /// Which hotkey field in Preferences is listening for keys.
    pub prefs_hotkey_editing: Option<String>,
    /// First-run question: make LinkShot the system's screenshot tool (Print Screen).
    pub show_default_prompt: bool,
    /// Live overlay: the region grab running after the overlay went blank.
    live_capture: Option<LiveCapture>,
    /// Logical size of the primary monitor (windows are fitted to it).
    pub screen_size: Option<(f32, f32)>,
    /// Was the Capture window visible when the current capture started? Only then does it come back.
    pub capture_was_visible: bool,
    /// Preset being edited in place through the tab (Edit in the preset menu).
    pub editing_preset: Option<String>,
    pub status: Option<(String, Instant)>,
    interval_timer: Option<(CaptureSettings, Instant, Option<image::RgbaImage>)>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, background: bool, capture_now: bool) -> Self {
        let mut app = Self::build(&cc.egui_ctx, background, None);
        if capture_now {
            let settings = app.presets.selected().clone();
            app.last_capture = Some(settings.clone());
            app.pending = Some(PendingCapture { settings, due: Instant::now() + Duration::from_millis(50), hidden: false, countdown: false, bubble_pos: None, bubble_placed: false });
        }
        app
    }

    /// Construction without eframe (headless screenshots / tests). Uses a temporary config dir.
    pub fn new_headless(ctx: &egui::Context, dark: bool) -> Self {
        let mut app = Self::build(ctx, false, Some(dark));
        app.hotkeys = Hotkeys::disabled();
        app
    }

    fn build(ctx: &egui::Context, background: bool, headless_dark: Option<bool>) -> Self {
        let paths = Paths::discover();
        let _ = paths.ensure();
        let mut settings = Settings::load(&paths.settings_file());
        if let Some(dark) = headless_dark { settings.ui_theme = if dark { linkshot_core::settings::UiTheme::Dark } else { linkshot_core::settings::UiTheme::Light }; }
        let presets = SavedPresets::load(&paths.presets_file()).unwrap_or_default();
        let mut themes = vec![Theme::starter(), Theme::basic(), Theme::industrial()];
        if let Ok(rd) = std::fs::read_dir(paths.themes_dir()) {
            for e in rd.flatten() {
                if e.path().extension().map(|x| x == "snagtheme").unwrap_or(false) {
                    if let Ok(t) = Theme::read_file(&e.path()) { if !themes.iter().any(|x| x.id == t.id) { themes.push(t); } }
                }
            }
        }
        let pal = theme::palette(settings.ui_theme);
        theme::apply(ctx, &pal);
        SIMPLIFIED_CROSSHAIR.store(settings.simplified_crosshairs, Ordering::Relaxed);
        let mut editor = EditorState::new(&paths, &settings, &themes);
        { let c = ctx.clone(); editor.wake = std::sync::Arc::new(move || crate::keepalive::wake(&c)); }
        let _ = std::fs::create_dir_all(paths.recordings_dir());
        if headless_dark.is_none() { editor.load_tray(&paths, &settings); }
        let mut app = Self {
            paths,
            settings,
            presets,
            themes,
            pal,
            capture: CaptureWindowState::default(),
            editor,
            overlay: None,
            scrolling: None,
            recording: None,
            ctx: ctx.clone(),
            grab_text: None,
            pending: None,
            capture_visible: !background,
            editor_visible: false,
            editor_open_geometry: None,
            capture_toggled: false,
            hotkeys: if headless_dark.is_some() { Hotkeys::disabled() } else { let c = ctx.clone(); Hotkeys::new(move || crate::keepalive::wake(&c)) },
            tray: if headless_dark.is_some() { Tray::disabled() } else { let c = ctx.clone(); Tray::new(move || crate::keepalive::wake(&c)) },
            instance: if headless_dark.is_some() { crate::single_instance::Guard::disabled() } else { let c = ctx.clone(); crate::single_instance::listen(move || crate::keepalive::wake(&c)) },
            keepalive: Default::default(),
            headless: headless_dark.is_some(),
            last_capture: None,
            presets_dirty: false,
            settings_dirty: false,
            last_save: Instant::now(),
            show_prefs: false,
            prefs_hotkey_editing: None,
            show_about: false,
            show_default_prompt: false,
            live_capture: None,
            screen_size: linkshot_capture::monitors().ok().and_then(|ms| ms.iter().find(|m| m.is_primary).or(ms.first()).map(|m| (m.width as f32, m.height as f32))),
            capture_was_visible: false,
            editing_preset: None,
            status: None,
            interval_timer: None,
        };
        app.register_hotkeys();
        if let Some(e) = app.hotkeys.error.clone() { tracing::warn!("{e}"); }
        #[cfg(target_os = "linux")]
        linkshot_capture::screencast::configure(linkshot_capture::screencast::Tokens { plain: app.settings.screencast_token.clone(), cursor: app.settings.screencast_token_cursor.clone() });
        #[cfg(target_os = "linux")]
        if headless_dark.is_none() && linkshot_capture::kwin::is_kde() && std::env::var_os("FLATPAK_ID").is_none() {
            // The desktop file authorises KWin's ScreenShot2 interface and carries the "capture" action
            // that the system-wide Print Screen shortcut launches (see `default_tool`).
            match linkshot_capture::kwin::ensure_authorization("LinkShot", "linkshot") {
                Ok(true) => tracing::info!("installed desktop file for KWin screenshot authorisation"),
                Ok(false) => {}
                Err(e) => tracing::warn!("could not install desktop file: {e}"),
            }
        }
        if headless_dark.is_none() && !background && !app.settings.default_tool_prompted && linkshot_platform::default_tool::supported() {
            app.show_default_prompt = true;
        }
        app
    }

    /// Binds Print Screen to LinkShot system-wide (and starts LinkShot at login); see `default_tool`.
    pub fn make_default_tool(&mut self) {
        let id = linkshot_platform::default_tool::Identity::detect();
        match linkshot_platform::default_tool::make_default(&id) {
            Ok(report) => {
                self.settings.system_print_screen = true;
                self.settings.run_at_startup = true;
                self.set_status("LinkShot is now your screenshot tool: press Print Screen (Fn+PrtSc on compact keyboards) to capture");
                for step in report.steps { tracing::info!("{step}"); }
            }
            Err(e) => {
                tracing::warn!("make default: {e:#}");
                self.settings.system_print_screen = false;
                self.set_status(format!("Could not take over Print Screen: {e:#}"));
            }
        }
        self.settings.default_tool_prompted = true;
        self.mark_settings_dirty();
        self.register_hotkeys();
    }

    /// Gives the Print Screen key back to the desktop.
    pub fn undo_default_tool(&mut self) {
        let id = linkshot_platform::default_tool::Identity::detect();
        match linkshot_platform::default_tool::undo(&id) {
            Ok(_) => self.set_status("Print Screen is no longer bound to LinkShot system-wide"),
            Err(e) => self.set_status(format!("Could not release Print Screen: {e:#}")),
        }
        self.settings.system_print_screen = false;
        self.mark_settings_dirty();
        self.register_hotkeys();
    }

    pub fn mark_presets_dirty(&mut self) { self.presets_dirty = true; }
    pub fn mark_settings_dirty(&mut self) { self.settings_dirty = true; }

    /// A closure worker threads call to wake the UI loop.
    pub fn wake(&self) -> std::sync::Arc<dyn Fn() + Send + Sync> {
        let c = self.ctx.clone();
        std::sync::Arc::new(move || crate::keepalive::wake(&c))
    }

    pub fn set_status(&mut self, s: impl Into<String>) {
        self.status = Some((s.into(), Instant::now()));
    }

    pub fn register_hotkeys(&mut self) {
        let mut b = vec![
            (self.presets.global_hot_key.clone(), HotkeyEvent::GlobalCapture),
            (self.settings.hotkey_show_hide.clone(), HotkeyEvent::ShowHide),
            (self.settings.hotkey_repeat_last.clone(), HotkeyEvent::RepeatLast),
            (self.settings.hotkey_video_start_pause.clone(), HotkeyEvent::VideoStartPause),
            (self.settings.hotkey_video_stop.clone(), HotkeyEvent::VideoStop),
            (self.settings.hotkey_mute_mic.clone(), HotkeyEvent::MuteMic),
            (self.settings.hotkey_draw_mode.clone(), HotkeyEvent::DrawMode),
        ];
        for p in &self.presets.presets { b.push((p.hot_key.clone(), HotkeyEvent::Preset(p.id.clone()))); }
        // When the desktop itself launches `linkshot --capture` on Print Screen, claiming the key here too
        // would conflict (Linux); on Windows the in-app registration is what receives the key.
        if self.settings.system_print_screen && cfg!(target_os = "linux") {
            b.retain(|(hk, _)| !matches!(hk.key.as_str(), "PrintScreen" | "Snapshot") || hk.ctrl || hk.alt || hk.shift || hk.win);
        }
        self.hotkeys.apply(&b);
    }

    fn persist(&mut self, force: bool) {
        if !force && self.last_save.elapsed() < Duration::from_secs(2) { return; }
        if self.presets_dirty { if let Err(e) = self.presets.save(&self.paths.presets_file()) { tracing::warn!("save presets: {e}"); } self.presets_dirty = false; }
        if self.settings_dirty { if let Err(e) = self.settings.save(&self.paths.settings_file()) { tracing::warn!("save settings: {e}"); } self.settings_dirty = false; }
        self.editor.persist(&self.paths);
        self.last_save = Instant::now();
    }

    pub fn apply_theme(&mut self, ctx: &egui::Context) {
        self.pal = theme::palette(self.settings.ui_theme);
        theme::apply(ctx, &self.pal);
        // egui would otherwise rescale every window on Ctrl +/-/0, which the Editor uses for the
        // canvas; the interface scale is an explicit setting instead (View ▸ Interface Scale).
        ctx.options_mut(|o| o.zoom_with_keyboard = false);
        self.apply_ui_scale(ctx);
        SIMPLIFIED_CROSSHAIR.store(self.settings.simplified_crosshairs, Ordering::Relaxed);
    }

    /// Applies `settings.ui_scale` to every window (the zoom factor is shared by all viewports).
    pub fn apply_ui_scale(&mut self, ctx: &egui::Context) {
        let z = self.settings.ui_scale;
        if !(0.5..=3.0).contains(&z) { self.settings.ui_scale = 1.0; }
        let z = self.settings.ui_scale;
        if (ctx.zoom_factor() - z).abs() > 0.001 { ctx.set_zoom_factor(z); }
    }

    pub fn set_ui_scale(&mut self, ctx: &egui::Context, scale: f32) {
        self.settings.ui_scale = (scale * 100.0).round() / 100.0;
        self.settings.ui_scale = self.settings.ui_scale.clamp(0.6, 2.5);
        self.mark_settings_dirty();
        self.apply_ui_scale(ctx);
    }

    /// Default / saved Editor window geometry, kept inside the primary monitor.
    fn editor_window_geometry(&self) -> (Option<[f32; 2]>, [f32; 2], [f32; 2]) {
        let (sw, sh) = self.screen_size.unwrap_or((1920.0, 1080.0));
        let max_w = (sw - 40.0).max(640.0);
        let max_h = (sh - 80.0).max(480.0);
        let min = [900.0f32.min(max_w), 600.0f32.min(max_h)];
        match self.settings.editor_window_rect {
            Some((x, y, w, h)) => {
                let w = w.clamp(min[0], max_w);
                let h = h.clamp(min[1], max_h);
                let pos = if linkshot_capture::is_wayland() { None } else { Some([x.max(0.0), y.max(0.0)]) };
                (pos, [w, h], min)
            }
            None => (None, [1280.0f32.min(max_w), 820.0f32.min(max_h)], min),
        }
    }

    // ------------------------------------------------------------------ actions

    pub fn dispatch(&mut self, ctx: &egui::Context, action: Action) {
        match action {
            Action::StartCapture(settings) => self.start_capture(ctx, settings),
            Action::RepeatLastCapture => { if let Some(s) = self.last_capture.clone() { self.start_capture(ctx, s); } }
            Action::OpenEditor => { self.editor_visible = true; self.editor.focus_requested = true; }
            Action::HideCaptureWindow => { self.capture_visible = false; }
            Action::ShowCaptureWindow => self.show_capture_window(ctx, true),
            Action::ToggleCaptureWindow => { if self.capture_visible { self.dispatch(ctx, Action::HideCaptureWindow) } else { self.dispatch(ctx, Action::ShowCaptureWindow) } }
            Action::ShowPreferences => { self.show_prefs = true; self.editor_visible = true; }
            Action::ShowAbout => self.show_about = true,
            Action::SetTheme(t) => {
                self.settings.ui_theme = t;
                self.mark_settings_dirty();
                self.apply_theme(ctx);
            }
            Action::ResetCurrentSettings => {
                let mode = self.presets.selected_mode;
                let fresh = SavedPresets::default();
                *self.presets.selected_mut() = match mode { CaptureMode::CaptureModeAllInOne => fresh.all_in_one, CaptureMode::CaptureModeImage => fresh.image, CaptureMode::CaptureModeVideo => fresh.video };
                self.mark_presets_dirty();
            }
            Action::AddPresetFromCurrent => {
                let mut p = self.presets.selected().clone();
                p.id = uuid::Uuid::new_v4().to_string();
                p.is_default = false;
                p.display_name = p.generate_name();
                p.auto_generate_name = true;
                p.hot_key = linkshot_core::presets::HotKey::none();
                let id = p.id.clone();
                self.presets.presets.push(p);
                self.capture.presets_expanded = true;
                self.capture.renaming = Some(id);
                self.mark_presets_dirty();
            }
            Action::AddPresetNew => {
                let mut p = CaptureSettings { capture_mode: CaptureMode::CaptureModeImage, ..Default::default() };
                p.display_name = "New Preset".into();
                p.auto_generate_name = false;
                let id = p.id.clone();
                self.presets.presets.push(p);
                self.capture.presets_expanded = true;
                self.capture.renaming = Some(id);
                self.mark_presets_dirty();
            }
            Action::EditPreset(id) => {
                // Load the preset into its tab; the presets bar offers Save / Stop editing while it is loaded.
                if let Some(p) = self.presets.presets.iter().find(|p| p.id == id).cloned() {
                    self.presets.selected_mode = p.capture_mode;
                    *self.presets.selected_mut() = CaptureSettings { id: self.presets.selected().id.clone(), ..p.clone() };
                    self.editing_preset = Some(id);
                    self.set_status(format!("Editing preset '{}': change the tab, then press Save in the presets bar.", p.effective_name()));
                    self.mark_presets_dirty();
                }
            }
            Action::SavePresetEdits => {
                if let Some(id) = self.editing_preset.take() {
                    let tab = self.presets.selected().clone();
                    if let Some(p) = self.presets.presets.iter_mut().find(|p| p.id == id) {
                        let keep = (p.id.clone(), p.hot_key.clone(), p.display_name.clone(), p.auto_generate_name, p.is_default);
                        *p = CaptureSettings { id: keep.0, hot_key: keep.1, display_name: keep.2, auto_generate_name: keep.3, is_default: keep.4, ..tab };
                        if p.auto_generate_name { p.display_name = p.generate_name(); }
                        let name = p.effective_name();
                        self.set_status(format!("Preset '{name}' saved."));
                        self.mark_presets_dirty();
                    }
                }
            }
            Action::CancelPresetEdit => { self.editing_preset = None; self.set_status("Stopped editing the preset (the tab keeps its settings)."); }
            Action::DuplicatePreset(id) => {
                if let Some(p) = self.presets.presets.iter().find(|p| p.id == id).cloned() {
                    let mut d = p.clone();
                    d.id = uuid::Uuid::new_v4().to_string();
                    d.display_name = format!("{} copy", p.effective_name());
                    d.auto_generate_name = false;
                    d.hot_key = linkshot_core::presets::HotKey::none();
                    self.presets.presets.push(d);
                    self.mark_presets_dirty();
                }
            }
            Action::DeletePreset(id) => { self.presets.presets.retain(|p| p.id != id); self.mark_presets_dirty(); self.register_hotkeys(); }
            Action::ExportPreset(id) => {
                if let Some(p) = self.presets.presets.iter().find(|p| p.id == id).cloned() {
                    if let Some(path) = rfd::FileDialog::new().set_file_name(format!("{}.json", linkshot_core::naming::sanitize(&p.effective_name()))).add_filter("linkshot preset", &["json"]).save_file() {
                        if let Err(e) = std::fs::write(&path, serde_json::to_string_pretty(&vec![p]).unwrap_or_default()) { self.set_status(format!("Export failed: {e}")); } else { self.set_status("Preset exported."); }
                    }
                }
            }
            Action::ExportPresets => {
                if let Some(path) = rfd::FileDialog::new().set_file_name("presets.json").add_filter("linkshot presets", &["json"]).save_file() {
                    let _ = std::fs::write(&path, serde_json::to_string_pretty(&self.presets.presets).unwrap_or_default());
                    self.set_status("Presets exported.");
                }
            }
            Action::ImportPresets => {
                if let Some(path) = rfd::FileDialog::new().add_filter("Presets", &["snagpresets", "xml", "json"]).pick_file() {
                    match std::fs::read(&path) {
                        Ok(bytes) => {
                            let imported: Vec<CaptureSettings> = if path.extension().map(|e| e == "json").unwrap_or(false) {
                                serde_json::from_slice(&bytes).unwrap_or_default()
                            } else {
                                SavedPresets::import_linkshot_xml(&bytes).unwrap_or_default().into_iter().filter(|p| !p.id.starts_with('{') || p.id.contains("Def")).collect()
                            };
                            let n = imported.len();
                            for mut p in imported { if self.presets.presets.iter().any(|x| x.id == p.id) { p.id = uuid::Uuid::new_v4().to_string(); } self.presets.presets.push(p); }
                            self.capture.presets_expanded = true;
                            self.mark_presets_dirty();
                            self.register_hotkeys();
                            self.set_status(format!("Imported {n} preset(s)."));
                        }
                        Err(e) => self.set_status(format!("Import failed: {e}")),
                    }
                }
            }
            Action::Quit => { self.persist(true); ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Close); }
        }
    }

    // ------------------------------------------------------------------ capture flow

    pub fn start_capture(&mut self, ctx: &egui::Context, settings: CaptureSettings) {
        self.last_capture = Some(settings.clone());
        // Interval/Schedule: toggle start/stop.
        if settings.time_delay && settings.timed_capture_type == TimedCaptureType::Interval {
            if self.interval_timer.is_some() { self.interval_timer = None; self.capture_toggled = false; self.set_status("Interval capture stopped."); return; }
            self.capture_toggled = true;
            self.interval_timer = Some((settings, Instant::now(), None));
            self.set_status("Interval capture started.");
            return;
        }
        // Schedule: the button reads Start/Stop like Interval.
        if settings.time_delay && settings.timed_capture_type == TimedCaptureType::Schedule && self.pending.is_some() && self.capture_toggled {
            self.pending = None; self.capture_toggled = false; self.set_status("Scheduled capture cancelled."); return;
        }
        // One capture at a time: a second trigger while the overlay is up would snapshot the overlay itself.
        if self.overlay.is_some() || self.pending.is_some() || self.scrolling.is_some() || self.recording.is_some() {
            self.set_status("A capture is already in progress.");
            return;
        }
        let mut delay = Duration::from_millis(50);
        let mut schedule = false;
        let mut countdown = false;
        if settings.time_delay && settings.timed_capture_type == TimedCaptureType::Delay {
            delay += Duration::from_secs(settings.delay_duration.max(1) as u64);
            countdown = true;
        }
        if settings.time_delay && settings.timed_capture_type == TimedCaptureType::Schedule {
            match chrono::NaiveDateTime::parse_from_str(settings.scheduled_capture_time.trim(), "%Y-%m-%d %H:%M") {
                Err(_) => { self.set_status("Scheduled time must look like 2026-10-18 14:30 (set it in the Time Delay options)."); return; }
                Ok(t) => {
                    let now = chrono::Local::now().naive_local();
                    if t <= now { self.set_status("The scheduled time is in the past."); return; }
                    delay += (t - now).to_std().unwrap_or_default();
                    schedule = true;
                    self.capture_toggled = true;
                    self.set_status(format!("Capture scheduled for {} (press Stop to cancel).", t.format("%Y-%m-%d %H:%M")));
                }
            }
        }
        self.capture_was_visible = self.capture_visible;
        let mut hidden = false;
        if self.settings.hide_linkshot_during_capture && !schedule {
            // Not showing the viewports closes their windows (works on Wayland too, unlike
            // set_visible); `restore_windows` brings the Capture window back afterwards.
            self.capture_visible = false;
            self.editor_visible = false;
            hidden = true;
            delay += Duration::from_millis(300);
        }
        let bubble_pos = if countdown {
            linkshot_capture::monitors().ok().and_then(|ms| ms.iter().find(|m| m.is_primary).or(ms.first()).map(|m| [m.x as f32 + m.width as f32 - COUNTDOWN_W - 24.0, m.y as f32 + m.height as f32 - COUNTDOWN_H - 72.0]))
        } else { None };
        self.pending = Some(PendingCapture { settings, due: Instant::now() + delay, hidden, countdown, bubble_pos, bubble_placed: false });
        crate::keepalive::wake_after(ctx, delay);
    }

    fn run_pending(&mut self, ctx: &egui::Context) {
        let Some(p) = self.pending.as_ref() else { return };
        if Instant::now() < p.due { crate::keepalive::wake_after(ctx, p.due - Instant::now()); return; }
        if self.settings.hide_linkshot_during_capture && !p.hidden {
            // Scheduled capture: the windows are hidden only now, just before the screen is grabbed.
            self.capture_visible = false;
            self.editor_visible = false;
            let p = self.pending.as_mut().unwrap();
            p.hidden = true;
            p.due = Instant::now() + Duration::from_millis(350);
            crate::keepalive::wake_after(ctx, Duration::from_millis(350));
            return;
        }
        self.capture_toggled = false;
        let settings = self.pending.take().unwrap().settings;
        // Interactive selection modes draw over the live screen unless the user prefers a frozen one
        // (or no live back-end exists, e.g. X11 without a compositor).
        let interactive = !matches!(settings.selection_mode, SelectionMode::SelectionModeFullscreen | SelectionMode::SelectionModeClipboard | SelectionMode::SelectionModeFile)
            && !(settings.selection_mode == SelectionMode::SelectionModeRegion && settings.selection_settings_region.region_selection_type == RegionSelectionType::RegionSelectionFixed);
        let backend = if interactive && !self.settings.freeze_screen_during_capture { linkshot_capture::live::probe() } else { None };
        let grabbed = match backend { Some(_) => linkshot_capture::layout(), None => linkshot_capture::snapshot_with_cursor(settings.capture_cursor) };
        match grabbed {
            Err(e) => { self.set_status(format!("Capture failed: {e:#}")); self.restore_windows(ctx); }
            Ok(snap) => {
                let primary = snap.monitors.iter().position(|m| m.is_primary).unwrap_or(0);
                match settings.selection_mode {
                    SelectionMode::SelectionModeFullscreen => {
                        let img = snap.images[primary].clone();
                        self.finish_capture(ctx, img, None, &settings);
                    }
                    SelectionMode::SelectionModeClipboard => {
                        match linkshot_platform::clipboard::get_image() { Some(img) => self.finish_capture(ctx, img, None, &settings), None => { self.set_status("No image on the clipboard."); self.restore_windows(ctx); } }
                    }
                    SelectionMode::SelectionModeFile => {
                        if let Some(paths) = crate::share::open_dialog(None) {
                            for p in paths { if let Ok(doc) = crate::share::open_file(&p) { self.editor.add_document(ctx, doc, &self.paths, &self.settings); } }
                            self.editor_visible = true;
                        } else { self.restore_windows(ctx); }
                    }
                    SelectionMode::SelectionModeRegion if settings.selection_settings_region.region_selection_type == RegionSelectionType::RegionSelectionFixed => {
                        let r = settings.selection_settings_region.rect();
                        match snap.crop(r) { Some(img) => self.finish_capture(ctx, img, None, &settings), None => { self.set_status("Fixed region is off-screen."); self.restore_windows(ctx); } }
                    }
                    _ => {
                        // Start on the monitor under the pointer, not the primary one.
                        let idx = linkshot_capture::pointer_position().and_then(|p| snap.monitor_at(p.0, p.1)).unwrap_or(primary);
                        let mut o = Overlay::new(snap, idx, settings, self.settings.show_magnifier);
                        if let Some(b) = backend { o.sampler = Some(crate::live_sampler::LiveSampler::start(b.supports_sampling() && self.settings.show_magnifier, self.wake())); }
                        self.overlay = Some(o);
                    }
                }
            }
        }
    }

    /// Shows the Capture window and brings it to the front. `raise` runs the compositor-side
    /// activation (needed on Wayland, where focus requests from unfocused apps are ignored); the
    /// tray and single-instance threads have already done that when they queue their events.
    fn show_capture_window(&mut self, ctx: &egui::Context, raise: bool) {
        self.capture_visible = true;
        let id = capture_viewport_id();
        ctx.send_viewport_cmd_to(id, egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd_to(id, egui::ViewportCommand::Focus);
        if raise { crate::raise::raise_capture_window_now(); }
    }

    /// After a cancelled or windowless capture: the Capture window comes back only if it was visible
    /// before (or there is no tray icon to bring it back from).
    fn restore_windows(&mut self, ctx: &egui::Context) {
        if !self.editor_visible && (self.capture_was_visible || !self.tray.available) { self.show_capture_window(ctx, true); }
    }

    fn start_scrolling(&mut self, region: linkshot_core::Rect, monitor: linkshot_core::Rect, settings: CaptureSettings) {
        let auto = settings.selection_mode == SelectionMode::SelectionModeScrollingWindow;
        let opts = linkshot_capture::scrolling::ScrollOptions {
            auto,
            scroll_from_top: settings.selection_settings_scrolling_window.scroll_from_top,
            horizontal: false,
            notches: 3,
            restore_token: self.settings.remote_desktop_token.clone(),
        };
        let paused = self.settings.require_start_button_for_scrolling;
        let session = linkshot_capture::scrolling::Session::start(region, opts, paused);
        self.scrolling = Some(ScrollingUi::new(session, settings, region, monitor, auto, !paused));
    }

    fn scrolling_update(&mut self, ctx: &egui::Context) {
        let pal = self.pal;
        let Some(sc) = self.scrolling.as_mut() else { return };
        sc.poll();
        let builder = sc.viewport_builder();
        let mut outcome = None;
        ctx.show_viewport_immediate(egui::ViewportId::from_hash_of("scrolling-hud"), builder, |ctx, _class| {
            sc.ui(ctx, &pal);
            if ctx.input(|i| i.viewport().close_requested()) { sc.cancel(); }
        });
        let token = sc.new_token.take();
        let error = sc.new_error.take();
        if let Some(done) = sc.result.take() { outcome = Some((done, sc.settings.clone(), sc.frames)); }
        if let Some(t) = token { self.settings.remote_desktop_token = Some(t); self.mark_settings_dirty(); }
        if let Some(e) = error { self.set_status(e); }
        match outcome {
            None => ctx.request_repaint_after(Duration::from_millis(80)),
            Some((img, settings, frames)) => {
                self.scrolling = None;
                match img {
                    Some(img) => { self.set_status(format!("Scrolling capture: stitched {frames} frames into {}×{} px.", img.width(), img.height())); self.finish_capture(ctx, img, None, &settings); }
                    None => self.restore_windows(ctx),
                }
            }
        }
    }

    fn start_recording(&mut self, ctx: &egui::Context, region: linkshot_core::Rect, monitor: linkshot_core::Rect, monitors: Vec<linkshot_core::Rect>, settings: CaptureSettings) {
        if region.width() < 2.0 || region.height() < 2.0 { self.set_status("The region is too small to record."); self.restore_windows(ctx); return; }
        let fps = self.settings.video_quality.fps();
        let max_size = if self.settings.scale_video_to_1080p { Some((1920, 1080)) } else { None };
        let stamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
        let spool = self.paths.recordings_dir().join(format!("recording-{stamp}.lsv"));
        let ready_first = self.settings.show_ready_to_record && !self.settings.auto_start_recording;
        let mut settings = settings;
        if !self.settings.enable_webcam { settings.enable_webcam = false; }
        let setup = crate::recording_hud::RecordingSetup {
            region, monitor, monitors, settings, fps, max_size, spool_path: spool, ready_first,
            show_countdown: self.settings.show_video_countdown, mic_alert: self.settings.mic_muted_alert, mute_hotkey: self.settings.hotkey_mute_mic.display(),
            minimize_fullscreen: self.settings.minimize_fullscreen_controls, draw_hotkey: self.settings.hotkey_draw_mode.display(),
        };
        self.recording = Some(RecordingUi::new(setup, self.wake()));
    }

    fn recording_update(&mut self, ctx: &egui::Context) {
        let pal = self.pal;
        let Some(rec) = self.recording.as_mut() else { return };
        rec.poll();
        let builder = rec.viewport_builder();
        ctx.show_viewport_immediate(egui::ViewportId::from_hash_of("recording-hud"), builder, |ctx, _class| {
            rec.ui(ctx, &pal);
            if ctx.input(|i| i.viewport().close_requested()) { rec.cancel(); }
        });
        // Screen Draw layer over the region (click-through until draw mode is on).
        let can_place = !linkshot_capture::is_wayland() || cfg!(target_os = "linux") && linkshot_capture::kwin::is_kde();
        if can_place && rec.result.is_none() {
            if let Some(d) = rec.draw.as_mut() {
                if d.needs_window() {
                    let b = d.viewport_builder();
                    ctx.show_viewport_immediate(egui::ViewportId::from_hash_of("screen-draw"), b, |ctx, _class| { d.ui(ctx, &pal); });
                }
            }
        }
        // Frame strips: only where we can position windows (KDE Wayland via KWin, or X11/Windows).
        if can_place && rec.result.is_none() {
            for (i, strip) in rec.frame_strips().into_iter().enumerate() {
                let b = rec.frame_builder(i, strip);
                ctx.show_viewport_immediate(egui::ViewportId::from_hash_of(("recording-frame", i)), b, |ctx, _class| { rec.frame_ui(ctx, &pal); });
            }
        }
        let new_error = rec.new_error.take();
        let result = rec.result.take();
        let settings = rec.settings.clone();
        if let Some(e) = new_error { self.set_status(e); }
        let Some(result) = result else { ctx.request_repaint_after(Duration::from_millis(100)); return };
        self.recording = None;
        match result {
            None => { self.set_status("Recording cancelled."); self.restore_windows(ctx); }
            Some(recording) => {
                let info = &recording.spool;
                self.set_status(format!("Recorded {} frames ({}×{}, {}){}.", info.frames, info.width, info.height, linkshot_video::format_duration(info.duration_ms), if recording.audio.is_some() { " with audio" } else { "" }));
                self.settings.capture_count += 1;
                self.mark_settings_dirty();
                let paths = self.paths.clone();
                let app_settings = self.settings.clone();
                match self.editor.add_recording(info.path.clone(), &paths, &app_settings) {
                    Ok(()) => {
                        if settings.preview_in_editor || !self.settings.auto_save_videos {
                            self.editor_visible = true;
                            self.editor.focus_requested = true;
                        } else { self.restore_windows(ctx); }
                    }
                    Err(e) => { self.set_status(format!("Recording could not be opened: {e:#}")); self.restore_windows(ctx); }
                }
            }
        }
    }

    /// Runs OCR on `img` in the background and shows the "Grab Text Results" window.
    pub fn start_grab_text(&mut self, img: image::RgbaImage) {
        let model_dir = linkshot_ocr::bundled_model_dir().unwrap_or_else(|| self.paths.data_dir.join("models"));
        let lang = self.settings.ocr_language.clone();
        let prefer_builtin = self.settings.ocr_prefer_builtin;
        self.grab_text = Some(GrabTextUi::start(img, model_dir, lang, prefer_builtin));
    }

    pub fn finish_capture(&mut self, ctx: &egui::Context, img: image::RgbaImage, cursor: Option<linkshot_core::Pt>, settings: &CaptureSettings) {
        let _ = cursor;
        if settings.selection_mode == SelectionMode::SelectionModeOcr {
            self.start_grab_text(img);
            return;
        }
        let (w, h) = img.dimensions();
        let mut doc = match linkshot_render::encode_png(&img) { Ok(png) => Document::from_png(w, h, png), Err(e) => { self.set_status(format!("encode failed: {e}")); return; } };
        if let Some(c) = cursor {
            // The cursor is approximated with a small arrow glyph object.
            let mut obj = CaptureObject::new("Cursor");
            obj.points_array = vec![linkshot_core::Pt::new(c.x - 2.0, c.y - 2.0), linkshot_core::Pt::new(c.x + 14.0, c.y + 20.0)];
            let _ = obj; // TODO: capture the real cursor bitmap per platform.
        }
        doc.metadata.extra.insert("Application".into(), serde_json::Value::String(String::new()));
        self.settings.capture_count += 1;
        self.mark_settings_dirty();
        // Capture-time effects.
        for e in &settings.capture_effects { self.editor.apply_capture_effect(&mut doc, e); }
        if settings.copy_to_clipboard { if let Err(e) = crate::share::copy_all(&mut self.editor.renderer, &doc) { tracing::warn!("clipboard: {e}"); } }
        // Share destinations without preview.
        if !settings.preview_in_editor {
            self.editor.share_now(&mut doc, settings, &self.paths, &self.settings);
            if self.settings.auto_save_images { self.editor.autosave(&mut doc, &self.paths, &self.settings); }
            // The capture is done: the Capture window stays out of the way (tray icon / hotkey bring it
            // back), unless there is no tray to get it back from.
            if !self.tray.available { self.restore_windows(ctx); }
            self.set_status("Captured.");
            return;
        }
        self.editor.pending_share = if settings.share_destination_ids.iter().any(|d| d != output_ids::NONE) { Some(settings.clone()) } else { None };
        self.editor.add_document(ctx, doc, &self.paths, &self.settings);
        self.editor_visible = true;
        self.editor.focus_requested = true;
    }

    fn poll_events(&mut self, ctx: &egui::Context) {
        for ev in self.hotkeys.poll() {
            match ev {
                HotkeyEvent::GlobalCapture => { let s = self.presets.selected().clone(); self.start_capture(ctx, s); }
                HotkeyEvent::ShowHide => self.dispatch(ctx, Action::ToggleCaptureWindow),
                HotkeyEvent::RepeatLast => self.dispatch(ctx, Action::RepeatLastCapture),
                HotkeyEvent::Preset(id) => { if let Some(p) = self.presets.presets.iter().find(|p| p.id == id).cloned() { self.start_capture(ctx, p); } }
                HotkeyEvent::VideoStartPause => {
                    match self.recording.as_mut() {
                        Some(r) => r.toggle_pause(),
                        None => { let s = self.presets.video.clone(); self.start_capture(ctx, s); }
                    }
                }
                HotkeyEvent::VideoStop => { if let Some(r) = self.recording.as_mut() { r.stop(); } }
                HotkeyEvent::MuteMic => { if let Some(r) = self.recording.as_mut() { r.toggle_mic_mute(); } }
                HotkeyEvent::DrawMode => { if let Some(r) = self.recording.as_mut() { r.toggle_draw(); } }
            }
        }
        for ev in self.tray.poll() {
            tracing::debug!("tray event {ev:?}");
            match ev {
                TrayEvent::OpenEditor => self.dispatch(ctx, Action::OpenEditor),
                TrayEvent::OpenCaptureWindow | TrayEvent::IconClicked => self.show_capture_window(ctx, false),
                TrayEvent::Capture => { let s = self.presets.selected().clone(); self.start_capture(ctx, s); }
                TrayEvent::RepeatLast => self.dispatch(ctx, Action::RepeatLastCapture),
                TrayEvent::Preferences => self.dispatch(ctx, Action::ShowPreferences),
                TrayEvent::Help => { let _ = linkshot_platform::open_url("https://github.com/ZeroDay-Labz/linkshot#readme"); }
                TrayEvent::Exit => self.dispatch(ctx, Action::Quit),
            }
        }
        for req in self.instance.poll() {
            tracing::debug!("single-instance request {req:?}");
            match req {
                crate::single_instance::Request::Show => self.show_capture_window(ctx, false),
                crate::single_instance::Request::Capture => { let s = self.presets.selected().clone(); self.start_capture(ctx, s); }
                crate::single_instance::Request::Ping => {}
            }
        }
        // Interval capture tick.
        if let Some((settings, last, prev)) = self.interval_timer.as_mut() {
            let secs = match settings.interval_units { IntervalUnits::Seconds => 1, IntervalUnits::Minutes => 60, IntervalUnits::Hours => 3600 } * settings.interval_duration.max(1) as u64;
            if last.elapsed() >= Duration::from_secs(secs) {
                *last = Instant::now();
                let settings = settings.clone();
                let discard = settings.interval_discard_identical;
                if let Ok(snap) = linkshot_capture::snapshot_with_cursor(settings.capture_cursor) {
                    let idx = snap.monitors.iter().position(|m| m.is_primary).unwrap_or(0);
                    let img = match settings.selection_mode {
                        SelectionMode::SelectionModeRegion if settings.selection_settings_region.region_selection_type == RegionSelectionType::RegionSelectionFixed => snap.crop(settings.selection_settings_region.rect()),
                        _ => Some(snap.images[idx].clone()),
                    };
                    if let Some(img) = img {
                        let identical = discard && prev.as_ref().map(|p| linkshot_capture::images_identical(p, &img)).unwrap_or(false);
                        let first = prev.is_none();
                        *prev = Some(img.clone());
                        if !identical {
                            self.finish_capture(ctx, img, None, &settings);
                            // Later ticks must not keep yanking the Editor to the front.
                            if !first { self.editor.focus_requested = false; }
                        }
                    }
                }
            }
            crate::keepalive::wake_after(ctx, Duration::from_millis(250));
        }
    }
}

impl eframe::App for App {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] { [0.0, 0.0, 0.0, 0.0] }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        tracing::trace!("update pass {}", ctx.cumulative_pass_nr());
        #[cfg(target_os = "linux")]
        if let Some(t) = linkshot_capture::screencast::take_new_tokens() { self.settings.screencast_token = t.plain; self.settings.screencast_token_cursor = t.cursor; self.mark_settings_dirty(); }
        self.poll_events(ctx);
        self.run_pending(ctx);

        // Overlay viewports: one fullscreen, borderless, always-on-top window per monitor so the
        // pointer can move between screens during a capture.
        if self.overlay.is_some() {
            let pal = self.pal;
            let wayland = linkshot_capture::is_wayland();
            #[cfg(target_os = "linux")]
            let kde = wayland && linkshot_capture::kwin::is_kde();
            #[cfg(not(target_os = "linux"))]
            let kde = false;
            let monitors: Vec<(usize, String, linkshot_core::Rect)> = { let o = self.overlay.as_ref().unwrap(); o.snapshot.monitors.iter().enumerate().map(|(i, m)| (i, m.name.clone(), m.rect())).collect() };
            let single = wayland && !kde && monitors.len() > 1;
            if single { if let Some(o) = self.overlay.as_mut() { if o.single_monitor.is_none() { o.single_monitor = Some(o.active_monitor); } } }
            let shown: Vec<(usize, String, linkshot_core::Rect)> = if single { vec![monitors[self.overlay.as_ref().unwrap().active_monitor.min(monitors.len() - 1)].clone()] } else { monitors.clone() };
            let mut result: Option<OverlayResult> = None;
            for (mi, name, mr) in &shown {
                let title = format!("linkshot capture overlay {name}");
                let mut builder = egui::ViewportBuilder::default()
                    .with_title(&title)
                    .with_app_id(crate::app_id())
                    .with_decorations(false)
                    .with_transparent(true)
                    .with_always_on_top()
                    .with_taskbar(false)
                    .with_resizable(false);
                if wayland { builder = builder.with_fullscreen(true); }
                else { builder = builder.with_position([mr.min.x as f32, mr.min.y as f32]).with_inner_size([mr.width() as f32, mr.height() as f32]); }
                let id = egui::ViewportId::from_hash_of(("overlay", *mi));
                ctx.show_viewport_immediate(id, builder, |ctx, _class| {
                    if let Some(o) = self.overlay.as_mut() {
                        o.ui(ctx, &pal, *mi);
                        if let Some(r) = o.result.take() { result = Some(r); }
                    }
                    if ctx.input(|i| i.viewport().close_requested()) { result = Some(OverlayResult::Cancel); }
                });
            }
            // KDE Wayland: a background thread asks KWin to put each overlay on its output as soon
            // as the windows exist, and checks that they got there.
            #[cfg(target_os = "linux")]
            if kde && shown.len() > 1 {
                if let Some(o) = self.overlay.as_mut() {
                    if !o.placement_started {
                        o.placement_started = true;
                        let map: Vec<(String, String)> = shown.iter().map(|(_, name, _)| (format!("linkshot capture overlay {name}"), name.clone())).collect();
                        std::thread::spawn(move || { if let Err(e) = linkshot_capture::kwin::ensure_fullscreen_placement(&map) { tracing::warn!("KWin placement: {e:#}"); } });
                    }
                }
            }
            // Live mode: once every overlay window is transparent, grab the region from the real screen;
            // the blank windows stay until the grab is done so nothing of ours is in the picture.
            if self.live_capture.is_none() {
                if let Some(o) = self.overlay.as_ref() {
                    if o.blank_ready() {
                        if let Some(OverlayResult::Image { region, cursor }) = o.blank_result() {
                            let (region, cursor, settings) = (*region, *cursor, o.settings.clone());
                            let (tx, rx) = crossbeam_channel::bounded(1);
                            let wake = self.wake();
                            let include_cursor = settings.capture_cursor;
                            std::thread::spawn(move || { let _ = tx.send(linkshot_capture::capture_region_with_cursor(region, include_cursor)); wake(); });
                            self.live_capture = Some(LiveCapture { rx, cursor, settings, started: Instant::now() });
                        }
                    }
                }
            }
            if let Some(lc) = self.live_capture.as_ref() {
                let done = match lc.rx.try_recv() { Ok(r) => Some(r), Err(_) if lc.started.elapsed() > Duration::from_secs(6) => Some(Err(anyhow::anyhow!("the screen capture did not finish"))), Err(_) => None };
                if let Some(res) = done {
                    let lc = self.live_capture.take().unwrap();
                    self.overlay = None;
                    crate::keepalive::wake(ctx);
                    match res {
                        Ok(img) => self.finish_capture(ctx, img, lc.cursor, &lc.settings),
                        Err(e) => { self.set_status(format!("Capture failed: {e:#}")); self.restore_windows(ctx); }
                    }
                } else { ctx.request_repaint_after(Duration::from_millis(16)); }
            }
            if let Some(r) = result {
                let overlay = self.overlay.take().unwrap();
                let settings = overlay.settings.clone();
                // The overlay windows were drawn this pass; egui only removes them at the end of the
                // next root pass, so ask for it right away instead of waiting for the next input event.
                crate::keepalive::wake(ctx);
                match r {
                    OverlayResult::Cancel => self.restore_windows(ctx),
                    OverlayResult::Image { region, cursor } => match overlay.snapshot.crop(region) { Some(img) => self.finish_capture(ctx, img, cursor, &settings), None => self.restore_windows(ctx) },
                    OverlayResult::Scrolling { region } => {
                        let monitor = overlay.monitor_for(region);
                        self.start_scrolling(region, monitor, settings);
                    }
                    OverlayResult::Video { region } => {
                        let monitor = overlay.monitor_for(region);
                        let monitors: Vec<linkshot_core::Rect> = overlay.snapshot.monitors.iter().map(|m| m.rect()).collect();
                        self.start_recording(ctx, region, monitor, monitors, settings);
                    }
                }
            }
        }

        // "Capturing in N" bubble for delayed captures (bottom-right of the primary monitor).
        if let Some(p) = self.pending.as_mut() {
            let remaining = p.due.saturating_duration_since(Instant::now());
            if p.countdown && remaining > Duration::from_millis(400) {
                if let Some(pos) = p.bubble_pos {
                    #[cfg(target_os = "linux")]
                    if !p.bubble_placed && linkshot_capture::is_wayland() && linkshot_capture::kwin::is_kde() {
                        p.bubble_placed = true;
                        std::thread::spawn(move || { std::thread::sleep(Duration::from_millis(150)); if let Err(e) = linkshot_capture::kwin::setup_overlay_window(COUNTDOWN_TITLE, pos[0].round() as i32, pos[1].round() as i32, COUNTDOWN_W as i32, COUNTDOWN_H as i32) { tracing::debug!("countdown placement: {e:#}"); } });
                    }
                    let pal = self.pal;
                    let secs = remaining.as_secs_f32().ceil().max(1.0) as u32;
                    let builder = egui::ViewportBuilder::default().with_title(COUNTDOWN_TITLE).with_app_id(crate::app_id()).with_decorations(false).with_transparent(true)
                        .with_always_on_top().with_taskbar(false).with_resizable(false).with_mouse_passthrough(true).with_inner_size([COUNTDOWN_W, COUNTDOWN_H]).with_position(pos);
                    ctx.show_viewport_immediate(egui::ViewportId::from_hash_of("countdown"), builder, |ctx, _class| {
                        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(egui::Color32::TRANSPARENT)).show(ctx, |ui| {
                            let r = ui.max_rect().shrink(2.0);
                            let p = ui.painter();
                            p.rect_filled(r, 10.0, pal.dark_panel);
                            p.rect_stroke(r, 10.0, egui::Stroke::new(1.0_f32, pal.control_outline), egui::StrokeKind::Inside);
                            p.text(egui::Pos2::new(r.left() + 14.0, r.center().y), egui::Align2::LEFT_CENTER, "Capturing in", egui::FontId::proportional(14.0), pal.text_bright);
                            p.circle_filled(egui::Pos2::new(r.right() - 32.0, r.center().y), 22.0, pal.capture_red);
                            p.text(egui::Pos2::new(r.right() - 32.0, r.center().y), egui::Align2::CENTER_CENTER, secs.to_string(), crate::theme::semibold(24.0), egui::Color32::WHITE);
                        });
                        ctx.request_repaint_after(Duration::from_millis(200));
                    });
                }
            }
        }

        // Scrolling-capture HUD (small always-on-top toolbar outside the region).
        if self.scrolling.is_some() { self.scrolling_update(ctx); }
        // Video recording toolbar + frame strips.
        if self.recording.is_some() { self.recording_update(ctx); }

        // Grab Text Results window.
        if let Some(gt) = self.grab_text.as_mut() {
            let pal = self.pal;
            let mut close = false;
            ctx.show_viewport_immediate(egui::ViewportId::from_hash_of("grab-text"), GrabTextUi::viewport_builder(), |ctx, _class| {
                gt.ui(ctx, &pal);
                if ctx.input(|i| i.viewport().close_requested()) { close = true; }
            });
            if close || gt.closed { self.grab_text = None; if !self.editor_visible { self.restore_windows(ctx); } }
        }

        // Root viewport: the invisible controller window (see keepalive.rs).
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(egui::Color32::TRANSPARENT)).show(ctx, |_ui| {});
        if ctx.input(|i| i.viewport().close_requested()) { self.persist(true); }

        // Capture window viewport. Hiding it (tray, "hide during capture") just stops showing it.
        if self.capture_visible {
            let builder = egui::ViewportBuilder::default()
                .with_title(crate::raise::CAPTURE_TITLE)
                .with_app_id(crate::app_id())
                .with_inner_size(self.capture_window_size())
                .with_min_inner_size([420.0, 220.0])
                .with_decorations(false)
                .with_transparent(true)
                .with_resizable(false);
            let mut close = false;
            ctx.show_viewport_immediate(capture_viewport_id(), builder, |ctx, _class| {
                self.capture_window_ui(ctx);
                if ctx.input(|i| i.viewport().close_requested()) { close = true; }
            });
            if close {
                // Closing the Capture window hides it to the tray (if we have one); otherwise quit.
                if self.tray.available { self.dispatch(ctx, Action::HideCaptureWindow); } else { self.dispatch(ctx, Action::Quit); }
            }
        }

        // Editor viewport.
        if !self.editor_visible { self.editor_open_geometry = None; }
        if self.editor_visible {
            let geometry = match self.editor_open_geometry { Some(g) => g, None => self.editor_window_geometry() };
            self.editor_open_geometry = Some(geometry);
            let (pos, size, min) = geometry;
            let mut builder = egui::ViewportBuilder::default()
                .with_title("LinkShot Editor")
                .with_app_id(crate::app_id())
                .with_decorations(false)
                .with_inner_size(size)
                .with_min_inner_size(min);
            if let Some(p) = pos { builder = builder.with_position(p); }
            let mut close = false;
            let mut actions: Vec<Action> = vec![];
            let mut seen: Option<(Option<egui::Rect>, Option<egui::Rect>)> = None;
            ctx.show_viewport_immediate(egui::ViewportId::from_hash_of("editor"), builder, |ctx, _class| {
                if self.editor.focus_requested { ctx.send_viewport_cmd(egui::ViewportCommand::Focus); self.editor.focus_requested = false; }
                crate::editor::editor_ui(self, ctx, &mut actions);
                seen = Some(ctx.input(|i| (i.viewport().inner_rect, i.viewport().outer_rect)));
                if ctx.input(|i| i.viewport().close_requested()) { close = true; }
            });
            // Remember the window size (and, where the platform reports it, the position).
            if let Some((Some(inner), outer)) = seen {
                let (w, h) = (inner.width().round(), inner.height().round());
                let (x, y) = match outer { Some(o) if !linkshot_capture::is_wayland() => (o.left().round(), o.top().round()), _ => self.settings.editor_window_rect.map(|r| (r.0, r.1)).unwrap_or((0.0, 0.0)) };
                if w >= 200.0 && h >= 200.0 && self.settings.editor_window_rect != Some((x, y, w, h)) {
                    self.settings.editor_window_rect = Some((x, y, w, h));
                    self.mark_settings_dirty();
                }
            }
            for a in actions { self.dispatch(ctx, a); }
            if close {
                self.editor_visible = false;
                let busy = self.overlay.is_some() || self.pending.is_some() || self.scrolling.is_some() || self.recording.is_some() || self.interval_timer.is_some();
                if !self.capture_visible && !busy && (self.capture_was_visible || !self.tray.available) { self.dispatch(ctx, Action::ShowCaptureWindow); }
            }
        }

        // Status toast (shared by both windows, drawn on the root).
        if let Some((_, t)) = &self.status { if t.elapsed() > Duration::from_secs(4) { self.status = None; } }
        self.persist(false);
        if self.pending.is_some() || self.interval_timer.is_some() { crate::keepalive::wake_after(ctx, Duration::from_millis(100)); }
        self.keepalive.tick(ctx, self.headless);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.persist(true);
    }
}
