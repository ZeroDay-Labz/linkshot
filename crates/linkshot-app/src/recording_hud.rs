//! Video recording UI: a small always-on-top toolbar
//! placed outside the recorded region with Record / Pause / Stop, the countdown, the elapsed time,
//! the frame counter and the microphone / system-audio / webcam switches, plus four thin strips
//! framing the region so the user sees what is recorded. The strips never cover the region
//! itself, so they cannot leak into the frames.

use crate::theme::Palette;
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2};
use linkshot_audio::{CaptureSpec, Kind};
use linkshot_capture::video::{AudioStatus, Command, Event, RecordOptions, Recorder, Recording, WebcamOptions};
use linkshot_capture::webcam::{Placement, Shape};
use linkshot_core::enums::{PreviewClipShape, WebcamPlacement};
use linkshot_core::presets::CaptureSettings;
use linkshot_core::Rect as LRect;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const HUD_W: f32 = 660.0;
pub const HUD_H: f32 = 64.0;
pub const HUD_TITLE: &str = "LinkShot Recording";
pub const FRAME_TITLE: &str = "LinkShot Recording Frame";
/// Thickness of the frame strips (logical px).
pub const FRAME_W: f32 = 3.0;

pub fn placement(p: WebcamPlacement) -> Placement {
    match p { WebcamPlacement::Fullscreen => Placement::Fullscreen, WebcamPlacement::TopLeft => Placement::TopLeft, WebcamPlacement::TopRight => Placement::TopRight, WebcamPlacement::BottomLeft => Placement::BottomLeft, WebcamPlacement::BottomRight => Placement::BottomRight }
}

pub fn shape(s: PreviewClipShape) -> Shape {
    match s { PreviewClipShape::Circle => Shape::Circle, PreviewClipShape::Rectangle => Shape::Rectangle, PreviewClipShape::RoundedRectangle => Shape::RoundedRectangle, PreviewClipShape::Square => Shape::Square, PreviewClipShape::RoundedSquare => Shape::RoundedSquare }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Waiting for the Record button ("Show Ready to Record").
    Ready,
    /// 3-2-1 countdown.
    Countdown,
    /// Countdown is over; a short gap lets the compositor finish its fades before the first frame.
    Starting,
    Recording,
    Paused,
    /// Stop pressed; waiting for the spool to close.
    Finishing,
}

/// Everything the toolbar starts from (the preset plus the relevant preferences).
#[derive(Clone, Debug)]
pub struct RecordingSetup {
    pub region: LRect,
    pub monitor: LRect,
    pub monitors: Vec<LRect>,
    pub settings: CaptureSettings,
    pub fps: u32,
    pub max_size: Option<(u32, u32)>,
    pub spool_path: PathBuf,
    pub ready_first: bool,
    pub show_countdown: bool,
    /// "Show alert when the microphone is muted".
    pub mic_alert: bool,
    /// Display text of the mute hotkey for the hints.
    pub mute_hotkey: String,
    /// "Minimize controls when recording full screen on one monitor".
    pub minimize_fullscreen: bool,
    pub draw_hotkey: String,
}

pub const MINI_W: f32 = 236.0;
pub const MINI_H: f32 = 40.0;

pub struct RecordingUi {
    pub phase: Phase,
    pub region: LRect,
    pub monitor: LRect,
    pub monitors: Vec<LRect>,
    pub settings: CaptureSettings,
    pub cursor: bool,
    pub fps: u32,
    pub max_size: Option<(u32, u32)>,
    pub spool_path: PathBuf,
    pub show_countdown: bool,
    /// Record the microphone / system audio (switchable until Record is pressed).
    pub mic: bool,
    pub system_audio: bool,
    pub mic_muted: bool,
    pub mic_device: Option<String>,
    /// The camera to composite, when the preset asks for one and a camera exists.
    pub webcam: Option<WebcamOptions>,
    pub mic_alert: bool,
    pub mute_hotkey: String,
    pub audio: AudioStatus,
    audio_available: bool,
    /// Small bar in the region's corner (the only place left when the region fills the monitor).
    pub compact: bool,
    /// Screen Draw layer (created when the recorder starts).
    pub draw: Option<crate::screen_draw::ScreenDraw>,
    /// Display text of the draw-mode hotkey.
    pub draw_hotkey: String,
    recorder: Option<Recorder>,
    wake: Arc<dyn Fn() + Send + Sync>,
    phase_since: Instant,
    pub frames: u32,
    pub dropped: u32,
    pub elapsed_ms: u64,
    pub bytes: u64,
    pub error: Option<String>,
    pub new_error: Option<String>,
    /// Set when the recorder finished (`Some(None)` = cancelled / nothing recorded).
    pub result: Option<Option<Recording>>,
    ui_frames: u32,
    placed: bool,
    frames_placed: bool,
    drag_sent: bool,
    focus_sent: bool,
}

impl RecordingUi {
    pub fn new(setup: RecordingSetup, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        let s = &setup.settings;
        let cursor = s.capture_cursor;
        let audio_available = linkshot_audio::available();
        let mic_device = Some(s.selected_microphone.device_path.clone()).filter(|d| !d.is_empty());
        let webcam = if s.enable_webcam && linkshot_capture::webcam::available() {
            let device = Some(s.selected_webcam_device.device_path.clone()).filter(|d| !d.is_empty()).or_else(|| linkshot_capture::webcam::devices().first().map(|d| d.id.clone()));
            device.map(|device| WebcamOptions { device, placement: if s.start_webcam { Placement::Fullscreen } else { placement(s.selected_webcam_placement) }, shape: shape(s.pip_shape), shown: true })
        } else { None };
        let mut ui = Self {
            phase: Phase::Ready, region: setup.region, monitor: setup.monitor, monitors: setup.monitors, cursor, fps: setup.fps, max_size: setup.max_size, spool_path: setup.spool_path, show_countdown: setup.show_countdown,
            mic: s.enable_microphone && audio_available, system_audio: s.enable_system_audio && audio_available, mic_muted: false, mic_device, webcam,
            mic_alert: setup.mic_alert, mute_hotkey: setup.mute_hotkey, audio: AudioStatus::default(), audio_available, compact: false, draw: None, draw_hotkey: setup.draw_hotkey.clone(),
            settings: setup.settings, recorder: None, wake, phase_since: Instant::now(), frames: 0, dropped: 0, elapsed_ms: 0, bytes: 0,
            error: None, new_error: None, result: None, ui_frames: 0, placed: false, frames_placed: false, drag_sent: false, focus_sent: false,
        };
        let (_, inside) = ui.position();
        ui.compact = inside && setup.minimize_fullscreen;
        if !setup.ready_first { ui.begin(); }
        ui
    }

    /// Toolbar size (compact bar or the full toolbar).
    pub fn size(&self) -> [f32; 2] { if self.compact { [MINI_W, MINI_H] } else { [HUD_W, HUD_H] } }

    fn set_phase(&mut self, p: Phase) { self.phase = p; self.phase_since = Instant::now(); }

    /// Record pressed (or auto-start): countdown, then the recorder.
    pub fn begin(&mut self) {
        if self.phase != Phase::Ready { return; }
        self.set_phase(if self.show_countdown { Phase::Countdown } else { Phase::Starting });
    }

    pub fn toggle_pause(&mut self) {
        match self.phase {
            Phase::Ready => self.begin(),
            Phase::Recording => { if let Some(r) = &self.recorder { r.send(Command::Pause); } self.set_phase(Phase::Paused); }
            Phase::Paused => { if let Some(r) = &self.recorder { r.send(Command::Resume); } self.set_phase(Phase::Recording); }
            _ => {}
        }
    }

    pub fn stop(&mut self) {
        match self.phase {
            Phase::Recording | Phase::Paused => { if let Some(r) = &self.recorder { r.send(Command::Stop); } self.set_phase(Phase::Finishing); }
            Phase::Ready | Phase::Countdown | Phase::Starting => self.result = Some(None),
            Phase::Finishing => {}
        }
    }

    pub fn cancel(&mut self) {
        match self.phase {
            Phase::Recording | Phase::Paused => { if let Some(r) = &self.recorder { r.send(Command::Cancel); } self.set_phase(Phase::Finishing); }
            Phase::Finishing => {}
            _ => self.result = Some(None),
        }
    }

    /// Mute / unmute the microphone (toolbar button, M, or the global hotkey).
    pub fn toggle_mic_mute(&mut self) {
        if !self.mic { return; }
        self.mic_muted = !self.mic_muted;
        if let Some(r) = &self.recorder { r.send(Command::SetMicMuted(self.mic_muted)); }
    }

    /// Show / hide the webcam overlay while recording.
    pub fn toggle_webcam(&mut self) {
        let Some(w) = self.webcam.as_mut() else { return };
        match &self.recorder {
            Some(r) => { let on = !self.audio.webcam_shown; r.send(Command::SetWebcam(on)); self.audio.webcam_shown = on; }
            None => w.shown = !w.shown,
        }
    }

    /// Webcam full screen ⇄ corner (Ctrl+Shift+F).
    pub fn swap_webcam(&mut self) {
        let Some(w) = self.webcam.as_mut() else { return };
        match &self.recorder {
            Some(r) => { r.send(Command::SwapWebcam); self.audio.webcam_shown = true; self.audio.webcam_fullscreen = !self.audio.webcam_fullscreen; }
            None => { w.shown = true; w.placement = if w.placement == Placement::Fullscreen { placement(self.settings.selected_webcam_placement) } else { Placement::Fullscreen }; }
        }
    }

    pub fn is_active(&self) -> bool { matches!(self.phase, Phase::Recording | Phase::Paused) }

    /// Enter / leave Screen Draw (toolbar button, D, or the global hotkey).
    pub fn toggle_draw(&mut self) {
        let active = self.is_active();
        if let Some(d) = self.draw.as_mut() { if active { d.toggle(); } }
    }

    fn record_options(&self) -> RecordOptions {
        let mut audio = vec![];
        if self.mic { audio.push(CaptureSpec { kind: Kind::Microphone, device: self.mic_device.clone() }); }
        if self.system_audio { audio.push(CaptureSpec { kind: Kind::SystemAudio, device: None }); }
        RecordOptions { fps: self.fps, include_cursor: self.cursor, max_size: self.max_size, spool_path: self.spool_path.clone(), audio, mic_muted: self.mic_muted, webcam: self.webcam.clone() }
    }

    /// Advances timers and drains recorder events.
    pub fn poll(&mut self) {
        match self.phase {
            Phase::Countdown if self.phase_since.elapsed() >= Duration::from_secs(3) => self.set_phase(Phase::Starting),
            Phase::Starting if self.phase_since.elapsed() >= Duration::from_millis(350) => {
                let opts = self.record_options();
                let wake = self.wake.clone();
                self.recorder = Some(Recorder::start(self.region, opts, move || wake()));
                self.draw = Some(crate::screen_draw::ScreenDraw::new(self.region, self.settings.screen_draw_fade_delay, self.settings.screen_draw_starts_on));
                self.set_phase(Phase::Recording);
            }
            _ => {}
        }
        let mut done = None;
        if let Some(r) = &self.recorder {
            while let Ok(ev) = r.events.try_recv() {
                match ev {
                    Event::Progress { frames, dropped, elapsed_ms, bytes, audio, .. } => {
                        self.frames = frames; self.dropped = dropped; self.elapsed_ms = elapsed_ms; self.bytes = bytes; self.audio = audio;
                        if audio.mic { self.mic_muted = audio.mic_muted; }
                    }
                    Event::Error(e) => { self.error = Some(e.clone()); self.new_error = Some(e); }
                    Event::Done(rec) => done = Some(rec),
                }
            }
        }
        if let Some(rec) = done { self.recorder = None; self.result = Some(rec); }
    }

    /// Toolbar position: centred under the region, above it when there is no room, on another
    /// monitor when the region fills this one, else inside the bottom edge (and then recorded).
    pub fn position(&self) -> ([f32; 2], bool) {
        let m = self.monitor;
        let r = self.region;
        if self.compact { return ([(r.max.x as f32 - MINI_W - 12.0).max(m.min.x as f32), (r.max.y as f32 - MINI_H - 12.0).max(m.min.y as f32)], true); }
        let x = |m: LRect| (r.center().x as f32 - HUD_W / 2.0).max(m.min.x as f32 + 8.0).min(m.max.x as f32 - HUD_W - 8.0);
        let below = r.max.y as f32 + 14.0;
        if below + HUD_H + 8.0 <= m.max.y as f32 { return ([x(m), below], false); }
        let above = r.min.y as f32 - 14.0 - HUD_H;
        if above >= m.min.y as f32 + 8.0 { return ([x(m), above], false); }
        if let Some(o) = self.monitors.iter().find(|o| **o != m) {
            return ([o.center().x as f32 - HUD_W / 2.0, o.max.y as f32 - HUD_H - 40.0], false);
        }
        ([x(m), (r.max.y as f32 - HUD_H - 24.0).max(m.min.y as f32)], true)
    }

    pub fn viewport_builder(&self) -> egui::ViewportBuilder {
        let ([x, y], _) = self.position();
        egui::ViewportBuilder::default()
            .with_title(HUD_TITLE)
            .with_app_id("linkshot")
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_taskbar(false)
            .with_resizable(false)
            .with_inner_size(self.size())
            .with_position([x, y])
    }

    /// The four strips around the region (logical rects), clipped to the monitor.
    pub fn frame_strips(&self) -> Vec<Rect> {
        let r = self.region;
        let m = self.monitor;
        let w = FRAME_W as f64;
        let strips = [
            LRect::from_xywh(r.min.x - w, r.min.y - w, r.width() + 2.0 * w, w),
            LRect::from_xywh(r.min.x - w, r.max.y, r.width() + 2.0 * w, w),
            LRect::from_xywh(r.min.x - w, r.min.y, w, r.height()),
            LRect::from_xywh(r.max.x, r.min.y, w, r.height()),
        ];
        strips.iter().filter_map(|s| {
            let c = LRect { min: s.min.max(m.min), max: s.max.min(m.max) };
            if c.width() >= 1.0 && c.height() >= 1.0 { Some(Rect::from_min_max(Pos2::new(c.min.x as f32, c.min.y as f32), Pos2::new(c.max.x as f32, c.max.y as f32))) } else { None }
        }).collect()
    }

    pub fn frame_builder(&self, i: usize, r: Rect) -> egui::ViewportBuilder {
        egui::ViewportBuilder::default()
            .with_title(format!("{FRAME_TITLE} {i}"))
            .with_app_id("linkshot")
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_taskbar(false)
            .with_resizable(false)
            .with_mouse_passthrough(true)
            .with_inner_size([r.width().max(1.0), r.height().max(1.0)])
            .with_position([r.left(), r.top()])
    }

    /// Paints one frame strip: red while recording, orange before, grey while paused.
    pub fn frame_ui(&self, ctx: &egui::Context, pal: &Palette) {
        let color = match self.phase { Phase::Recording | Phase::Finishing => pal.record, Phase::Paused => pal.input_secondary, _ => pal.selection_orange };
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(color)).show(ctx, |_ui| {});
    }

    /// Asks KWin to place the toolbar and the strips (Wayland ignores requested positions).
    fn place_on_kde(&mut self) {
        #[cfg(target_os = "linux")]
        {
            if !(linkshot_capture::is_wayland() && linkshot_capture::kwin::is_kde()) { return; }
            if !self.placed {
                self.placed = true;
                let ([x, y], _) = self.position();
                let [w, h] = self.size();
                std::thread::spawn(move || { if let Err(e) = linkshot_capture::kwin::setup_overlay_window(HUD_TITLE, x.round() as i32, y.round() as i32, w as i32, h as i32) { tracing::warn!("recording HUD placement: {e:#}"); } });
            }
            if !self.frames_placed {
                self.frames_placed = true;
                let strips = self.frame_strips();
                std::thread::spawn(move || {
                    for (i, s) in strips.iter().enumerate() {
                        let title = format!("{FRAME_TITLE} {i}");
                        if let Err(e) = linkshot_capture::kwin::setup_overlay_window(&title, s.left().round() as i32, s.top().round() as i32, s.width().round().max(1.0) as i32, s.height().round().max(1.0) as i32) { tracing::warn!("frame strip {i}: {e:#}"); }
                    }
                });
            }
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, pal: &Palette) {
        self.poll();
        self.ui_frames += 1;
        if !self.focus_sent { self.focus_sent = true; ctx.send_viewport_cmd(egui::ViewportCommand::Focus); }
        if self.ui_frames == 2 { self.place_on_kde(); }
        let (esc, enter, space, key_m, key_w, swap, key_d) = ctx.input(|i| (
            i.key_pressed(egui::Key::Escape), i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Space),
            i.key_pressed(egui::Key::M) && !i.modifiers.any(), i.key_pressed(egui::Key::W) && !i.modifiers.any(),
            i.key_pressed(egui::Key::F) && i.modifiers.ctrl && i.modifiers.shift,
            i.key_pressed(egui::Key::D) && !i.modifiers.any(),
        ));
        if key_d { self.toggle_draw(); }
        if esc { self.cancel(); }
        if enter { match self.phase { Phase::Ready => self.begin(), Phase::Recording | Phase::Paused => self.stop(), _ => {} } }
        if space { self.toggle_pause(); }
        if key_m { self.toggle_mic_mute(); }
        if key_w { self.toggle_webcam(); }
        if swap { self.swap_webcam(); }
        let (_, inside) = self.position();
        if self.compact { self.compact_ui(ctx, pal); ctx.request_repaint_after(Duration::from_millis(100)); return; }
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(Color32::TRANSPARENT)).show(ctx, |ui| {
            let view = ui.max_rect();
            let panel = view.shrink(2.0);
            let painter = ui.painter_at(view);
            painter.rect_filled(panel, 6.0, pal.dark_panel);
            painter.rect_stroke(panel, 6.0, Stroke::new(1.0_f32, pal.control_outline), egui::StrokeKind::Inside);
            // Drag handle on the status area (see scrolling_hud.rs for why StartDrag is gated).
            let drag_rect = Rect::from_min_max(panel.min, Pos2::new(panel.right() - 430.0, panel.bottom()));
            let bg = ui.interact(drag_rect, ui.id().with("rec-drag"), Sense::drag());
            let decided = ctx.input(|i| i.pointer.is_decidedly_dragging());
            if bg.dragged() && decided && !self.drag_sent { self.drag_sent = true; ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag); }
            if !bg.dragged() { self.drag_sent = false; }
            for k in 0..3 { for j in 0..2 { painter.circle_filled(Pos2::new(panel.left() + 7.0 + j as f32 * 4.0, panel.center().y - 4.0 + k as f32 * 4.0), 1.1, pal.text.gamma_multiply(0.6)); } }

            let dims = format!("{}×{}", self.region.width().round() as i64, self.region.height().round() as i64);
            let time = linkshot_video::format_duration(self.elapsed_ms.min(u32::MAX as u64) as u32);
            let mut audio_note = String::new();
            if self.mic || self.system_audio {
                let mut parts = vec![];
                if self.mic { parts.push(if self.mic_muted { "mic muted" } else { "mic" }); }
                if self.system_audio { parts.push("system audio"); }
                audio_note = format!(" · {}", parts.join(" + "));
            }
            if self.webcam.is_some() { audio_note.push_str(" · webcam"); }
            let (title, sub, dot) = match self.phase {
                Phase::Ready => ("Ready to record".to_string(), format!("{dims} · {} fps{audio_note}{}", self.fps, if inside { " · toolbar is inside the region" } else { "" }), pal.selection_orange),
                Phase::Countdown => { let n = 3 - (self.phase_since.elapsed().as_secs_f32().floor() as u32).min(2); (format!("Recording in {n}…"), dims.clone(), pal.selection_orange) }
                Phase::Starting => ("Starting…".to_string(), dims.clone(), pal.selection_orange),
                Phase::Recording => ("Recording".to_string(), format!("{time} · {} frames{}{audio_note}", self.frames, if self.dropped > 0 { format!(" · {} dropped", self.dropped) } else { String::new() }), pal.record),
                Phase::Paused => ("Paused".to_string(), format!("{time} · {} frames{audio_note}", self.frames), pal.input_secondary),
                Phase::Finishing => ("Finishing…".to_string(), format!("{time} · {} frames", self.frames), pal.green),
            };
            let blink = self.phase != Phase::Recording || (self.phase_since.elapsed().as_millis() / 600) % 2 == 0;
            if blink { painter.circle_filled(Pos2::new(panel.left() + 26.0, panel.center().y), 6.0, dot); }
            // The muted-microphone alert replaces the status line while it applies.
            let muted_alert = self.mic_alert && self.mic && self.mic_muted && matches!(self.phase, Phase::Recording | Phase::Paused) && (self.phase_since.elapsed().as_millis() / 700) % 2 == 0;
            let (sub_text, sub_color) = if let Some(e) = &self.error { (e.clone(), pal.capture_red) } else if muted_alert { (format!("Microphone is muted — press M or {} to unmute", self.mute_hotkey), pal.capture_red) } else { (sub, pal.text) };
            if self.phase == Phase::Countdown {
                let n = 3 - (self.phase_since.elapsed().as_secs_f32().floor() as u32).min(2);
                painter.text(Pos2::new(panel.right() - 200.0, panel.center().y), Align2::CENTER_CENTER, n.to_string(), crate::theme::semibold(34.0), pal.selection_orange);
            }

            // Buttons, right to left. `meter` draws a level bar along the bottom edge. A label
            // starting with '|' gets painted pause bars instead of a glyph the font may lack.
            let mut x = panel.right() - 8.0;
            let btn_h = 30.0;
            let mut button = |ui: &mut egui::Ui, label: &str, w: f32, accent: Option<Color32>, enabled: bool, tip: &str, meter: Option<f32>| -> bool {
                x -= w;
                let r = Rect::from_min_size(Pos2::new(x, panel.center().y - btn_h / 2.0), Vec2::new(w, btn_h));
                x -= 6.0;
                let resp = ui.interact(r, ui.id().with(label), if enabled { Sense::click() } else { Sense::hover() });
                let fill = match (accent, resp.hovered() && enabled) { (Some(c), true) => c.gamma_multiply(1.15), (Some(c), false) => c, (None, true) => pal.control_bright, (None, false) => pal.control };
                ui.painter().rect_filled(r, 4.0, if enabled { fill } else { pal.control_dark });
                let fg = match (accent, enabled) { (Some(_), true) => Color32::WHITE, (None, true) => pal.text_bright, (_, false) => pal.text };
                let (bars, label) = match label.strip_prefix('|') { Some(rest) => (true, rest), None => (false, label) };
                let text_pos = r.center() - Vec2::new(0.0, if meter.is_some() { 2.0 } else { 0.0 }) + Vec2::new(if bars { 7.0 } else { 0.0 }, 0.0);
                let galley = ui.painter().layout_no_wrap(label.to_string(), FontId::proportional(13.0), fg);
                if bars {
                    let bx = text_pos.x - galley.size().x / 2.0 - 12.0;
                    ui.painter().rect_filled(Rect::from_center_size(Pos2::new(bx - 3.0, r.center().y), Vec2::new(3.5, 12.0)), 1.0, fg);
                    ui.painter().rect_filled(Rect::from_center_size(Pos2::new(bx + 3.0, r.center().y), Vec2::new(3.5, 12.0)), 1.0, fg);
                }
                ui.painter().galley(text_pos - galley.size() / 2.0, galley, fg);
                if let Some(level) = meter {
                    let bar = Rect::from_min_max(Pos2::new(r.left() + 6.0, r.bottom() - 6.0), Pos2::new(r.right() - 6.0, r.bottom() - 3.0));
                    ui.painter().rect_filled(bar, 1.5, Color32::from_black_alpha(90));
                    let lvl = level.clamp(0.0, 1.0).sqrt();
                    let c = if lvl > 0.9 { pal.capture_red } else { pal.green };
                    ui.painter().rect_filled(Rect::from_min_max(bar.min, Pos2::new(bar.left() + bar.width() * lvl, bar.bottom())), 1.5, c);
                }
                let resp = if tip.is_empty() { resp } else { resp.on_hover_text(tip) };
                enabled && resp.clicked()
            };
            let finishing = self.phase == Phase::Finishing;
            if button(ui, "Cancel", 70.0, None, !finishing, "Discard the recording (Esc)", None) { self.cancel(); }
            match self.phase {
                Phase::Ready => {
                    if button(ui, "● Record", 96.0, Some(pal.record), true, "Start recording (Enter, Shift+F9)", None) { self.begin(); }
                    let c = self.cursor;
                    if button(ui, if c { "Cursor: on" } else { "Cursor: off" }, 88.0, if c { Some(pal.product) } else { None }, true, "Record the mouse pointer", None) { self.cursor = !c; }
                    let a = self.audio_available;
                    let m = self.mic;
                    if button(ui, if m { "Mic: on" } else { "Mic: off" }, 72.0, if m { Some(pal.product) } else { None }, a, if a { "Record the microphone (M mutes while recording)" } else { "Audio capture is not available (ffmpeg missing)" }, None) { self.mic = !m; }
                    let s = self.system_audio;
                    if button(ui, if s { "Audio: on" } else { "Audio: off" }, 82.0, if s { Some(pal.product) } else { None }, a, if a { "Record what you hear (system audio)" } else { "Audio capture is not available (ffmpeg missing)" }, None) { self.system_audio = !s; }
                    if let Some(w) = &self.webcam {
                        let shown = w.shown;
                        let full = w.placement == Placement::Fullscreen;
                        let label = if !shown { "Cam: off" } else if full { "Cam: full" } else { "Cam: on" };
                        if button(ui, label, 76.0, if shown { Some(pal.product) } else { None }, true, "Webcam overlay (W toggles, Ctrl+Shift+F swaps full screen / corner)", None) { self.toggle_webcam(); }
                    }
                }
                Phase::Countdown | Phase::Starting => { let _ = button(ui, "● Record", 96.0, Some(pal.record.gamma_multiply(0.6)), false, "", None); }
                Phase::Recording | Phase::Paused | Phase::Finishing => {
                    if button(ui, "■ Stop", 84.0, Some(pal.green), !finishing, "Stop and open in the Editor (Enter, Shift+F10)", None) { self.stop(); }
                    let paused = self.phase == Phase::Paused;
                    if button(ui, if paused { "▶ Resume" } else { "|Pause" }, 96.0, None, !finishing, "Pause / resume (Space, Shift+F9)", None) { self.toggle_pause(); }
                    if self.mic {
                        let muted = self.mic_muted;
                        let tip = format!("{} the microphone (M, {})", if muted { "Unmute" } else { "Mute" }, self.mute_hotkey);
                        if button(ui, if muted { "Mic muted" } else { "Mic" }, 84.0, if muted { Some(pal.capture_red) } else { None }, !finishing, &tip, Some(if muted { 0.0 } else { self.audio.mic_level })) { self.toggle_mic_mute(); }
                    }
                    if self.webcam.is_some() {
                        let shown = self.audio.webcam_shown;
                        let full = self.audio.webcam_fullscreen;
                        let label = if !shown { "Cam: off" } else if full { "Cam: full" } else { "Cam: on" };
                        if button(ui, label, 76.0, if shown { Some(pal.product) } else { None }, !finishing, "Webcam overlay (W toggles, Ctrl+Shift+F swaps full screen / corner)", None) { self.toggle_webcam(); }
                    }
                    let drawing = self.draw.as_ref().map(|d| d.active).unwrap_or(false);
                    let tip = format!("Screen Draw: arrows, squares and steps on the recording (D, {})", self.draw_hotkey);
                    if button(ui, "Draw", 58.0, if drawing { Some(pal.product) } else { None }, !finishing, &tip, None) { self.toggle_draw(); }
                }
            }
            // Status text in whatever room the buttons left, clipped so it never runs under them.
            let text_clip = Rect::from_min_max(panel.min, Pos2::new(x - 4.0, panel.bottom()));
            let tp = painter.with_clip_rect(text_clip);
            tp.text(Pos2::new(panel.left() + 40.0, panel.center().y - 9.0), Align2::LEFT_CENTER, &title, crate::theme::medium(13.0), pal.text_bright);
            tp.text(Pos2::new(panel.left() + 40.0, panel.center().y + 9.0), Align2::LEFT_CENTER, sub_text, FontId::proportional(11.5), sub_color);
        });
        ctx.request_repaint_after(Duration::from_millis(if self.phase == Phase::Ready { 250 } else { 100 }));
    }

    /// The compact bar: status dot, time, and Pause / Mic / Stop / Cancel glyph buttons.
    fn compact_ui(&mut self, ctx: &egui::Context, pal: &Palette) {
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(Color32::TRANSPARENT)).show(ctx, |ui| {
            let view = ui.max_rect();
            let panel = view.shrink(1.0);
            let painter = ui.painter_at(view);
            painter.rect_filled(panel, 8.0, pal.dark_panel.gamma_multiply(0.92));
            painter.rect_stroke(panel, 8.0, Stroke::new(1.0_f32, pal.control_outline), egui::StrokeKind::Inside);
            let dot = match self.phase { Phase::Recording | Phase::Finishing => pal.record, Phase::Paused => pal.input_secondary, _ => pal.selection_orange };
            let blink = self.phase != Phase::Recording || (self.phase_since.elapsed().as_millis() / 600) % 2 == 0;
            if blink { painter.circle_filled(Pos2::new(panel.left() + 14.0, panel.center().y), 5.0, dot); }
            let text = match self.phase {
                Phase::Ready => "Ready".to_string(),
                Phase::Countdown => format!("{}…", 3 - (self.phase_since.elapsed().as_secs_f32().floor() as u32).min(2)),
                Phase::Starting => "…".to_string(),
                _ => linkshot_video::format_duration(self.elapsed_ms.min(u32::MAX as u64) as u32),
            };
            painter.text(Pos2::new(panel.left() + 26.0, panel.center().y), Align2::LEFT_CENTER, text, crate::theme::medium(12.5), pal.text_bright);
            let mut x = panel.right() - 6.0;
            let mut glyph = |ui: &mut egui::Ui, id: &str, tip: &str, accent: Option<Color32>, enabled: bool, draw: &dyn Fn(&egui::Painter, Rect, Color32)| -> bool {
                x -= 30.0;
                let r = Rect::from_min_size(Pos2::new(x, panel.center().y - 13.0), Vec2::new(28.0, 26.0));
                x -= 4.0;
                let resp = ui.interact(r, ui.id().with(id), if enabled { Sense::click() } else { Sense::hover() });
                let fill = match (accent, resp.hovered() && enabled) { (Some(c), true) => c.gamma_multiply(1.15), (Some(c), false) => c, (None, true) => pal.control_bright, (None, false) => pal.control };
                ui.painter().rect_filled(r, 5.0, if enabled { fill } else { pal.control_dark });
                draw(ui.painter(), r.shrink(8.0), if accent.is_some() { Color32::WHITE } else { pal.text_bright });
                enabled && resp.on_hover_text(tip).clicked()
            };
            let cross = |p: &egui::Painter, r: Rect, c: Color32| { p.line_segment([r.left_top(), r.right_bottom()], Stroke::new(2.0_f32, c)); p.line_segment([r.right_top(), r.left_bottom()], Stroke::new(2.0_f32, c)); };
            let circle = |p: &egui::Painter, r: Rect, c: Color32| { p.circle_filled(r.center(), r.width() / 2.0, c); };
            let square = |p: &egui::Painter, r: Rect, c: Color32| { p.rect_filled(r, 1.0, c); };
            let pause = |p: &egui::Painter, r: Rect, c: Color32| { p.rect_filled(Rect::from_min_max(r.left_top(), Pos2::new(r.left() + r.width() * 0.38, r.bottom())), 1.0, c); p.rect_filled(Rect::from_min_max(Pos2::new(r.right() - r.width() * 0.38, r.top()), r.right_bottom()), 1.0, c); };
            let play = |p: &egui::Painter, r: Rect, c: Color32| { p.add(egui::Shape::convex_polygon(vec![r.left_top(), Pos2::new(r.right(), r.center().y), r.left_bottom()], c, Stroke::NONE)); };
            let mic_on = |p: &egui::Painter, r: Rect, c: Color32| { p.text(r.center(), Align2::CENTER_CENTER, "M", FontId::proportional(12.0), c); };
            let mic_off = |p: &egui::Painter, r: Rect, c: Color32| { p.text(r.center(), Align2::CENTER_CENTER, "M", FontId::proportional(12.0), c); p.line_segment([r.left_bottom(), r.right_top()], Stroke::new(2.0_f32, c)); };
            let pen = |p: &egui::Painter, r: Rect, c: Color32| { p.line_segment([r.left_bottom(), r.right_top()], Stroke::new(2.5_f32, c)); p.circle_filled(r.left_bottom(), 2.0, c); };
            let finishing = self.phase == Phase::Finishing;
            if glyph(ui, "cancel", "Discard the recording (Esc)", None, !finishing, &cross) { self.cancel(); }
            match self.phase {
                Phase::Ready => { if glyph(ui, "record", "Start recording (Enter)", Some(pal.record), true, &circle) { self.begin(); } }
                Phase::Countdown | Phase::Starting => { let _ = glyph(ui, "record", "", Some(pal.record.gamma_multiply(0.6)), false, &circle); }
                _ => {
                    if glyph(ui, "stop", "Stop and open in the Editor (Enter)", Some(pal.green), !finishing, &square) { self.stop(); }
                    let paused = self.phase == Phase::Paused;
                    if glyph(ui, "pause", "Pause / resume (Space)", None, !finishing, if paused { &play } else { &pause }) { self.toggle_pause(); }
                    if self.mic {
                        let muted = self.mic_muted;
                        if glyph(ui, "mic", if muted { "Unmute the microphone (M)" } else { "Mute the microphone (M)" }, if muted { Some(pal.capture_red) } else { None }, !finishing, if muted { &mic_off } else { &mic_on }) { self.toggle_mic_mute(); }
                    }
                    let drawing = self.draw.as_ref().map(|d| d.active).unwrap_or(false);
                    if glyph(ui, "draw", "Screen Draw (D)", if drawing { Some(pal.product) } else { None }, !finishing, &pen) { self.toggle_draw(); }
                }
            }
        });
    }
}
