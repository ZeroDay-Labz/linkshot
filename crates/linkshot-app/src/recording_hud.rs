//! Video recording UI: a small always-on-top toolbar
//! placed outside the recorded region with Record / Pause / Stop, the countdown, the elapsed time
//! and the frame counter, plus four thin strips framing the region so the user sees what is
//! recorded. The strips never cover the region itself, so they cannot leak into the frames.

use crate::theme::Palette;
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2};
use linkshot_capture::video::{Command, Event, RecordOptions, Recorder, Recording};
use linkshot_core::presets::CaptureSettings;
use linkshot_core::Rect as LRect;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const HUD_W: f32 = 500.0;
pub const HUD_H: f32 = 64.0;
pub const HUD_TITLE: &str = "LinkShot Recording";
pub const FRAME_TITLE: &str = "LinkShot Recording Frame";
/// Thickness of the frame strips (logical px).
pub const FRAME_W: f32 = 3.0;

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
    pub fn new(region: LRect, monitor: LRect, monitors: Vec<LRect>, settings: CaptureSettings, fps: u32, max_size: Option<(u32, u32)>, spool_path: PathBuf, ready_first: bool, show_countdown: bool, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        let cursor = settings.capture_cursor;
        let mut s = Self {
            phase: Phase::Ready, region, monitor, monitors, settings, cursor, fps, max_size, spool_path, show_countdown,
            recorder: None, wake, phase_since: Instant::now(), frames: 0, dropped: 0, elapsed_ms: 0, bytes: 0,
            error: None, new_error: None, result: None, ui_frames: 0, placed: false, frames_placed: false, drag_sent: false, focus_sent: false,
        };
        if !ready_first { s.begin(); }
        s
    }

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

    pub fn is_active(&self) -> bool { matches!(self.phase, Phase::Recording | Phase::Paused) }

    /// Advances timers and drains recorder events.
    pub fn poll(&mut self) {
        match self.phase {
            Phase::Countdown if self.phase_since.elapsed() >= Duration::from_secs(3) => self.set_phase(Phase::Starting),
            Phase::Starting if self.phase_since.elapsed() >= Duration::from_millis(350) => {
                let opts = RecordOptions { fps: self.fps, include_cursor: self.cursor, max_size: self.max_size, spool_path: self.spool_path.clone() };
                let wake = self.wake.clone();
                self.recorder = Some(Recorder::start(self.region, opts, move || wake()));
                self.set_phase(Phase::Recording);
            }
            _ => {}
        }
        let mut done = None;
        if let Some(r) = &self.recorder {
            while let Ok(ev) = r.events.try_recv() {
                match ev {
                    Event::Progress { frames, dropped, elapsed_ms, bytes, .. } => { self.frames = frames; self.dropped = dropped; self.elapsed_ms = elapsed_ms; self.bytes = bytes; }
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
            .with_inner_size([HUD_W, HUD_H])
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
                std::thread::spawn(move || { if let Err(e) = linkshot_capture::kwin::setup_overlay_window(HUD_TITLE, x.round() as i32, y.round() as i32, HUD_W as i32, HUD_H as i32) { tracing::warn!("recording HUD placement: {e:#}"); } });
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
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) { self.cancel(); }
        if ctx.input(|i| i.key_pressed(egui::Key::Enter)) { match self.phase { Phase::Ready => self.begin(), Phase::Recording | Phase::Paused => self.stop(), _ => {} } }
        if ctx.input(|i| i.key_pressed(egui::Key::Space)) { self.toggle_pause(); }
        let (_, inside) = self.position();
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(Color32::TRANSPARENT)).show(ctx, |ui| {
            let view = ui.max_rect();
            let panel = view.shrink(2.0);
            let painter = ui.painter_at(view);
            painter.rect_filled(panel, 6.0, pal.dark_panel);
            painter.rect_stroke(panel, 6.0, Stroke::new(1.0_f32, pal.control_outline), egui::StrokeKind::Inside);
            // Drag handle on the status area (see scrolling_hud.rs for why StartDrag is gated).
            let drag_rect = Rect::from_min_max(panel.min, Pos2::new(panel.right() - 270.0, panel.bottom()));
            let bg = ui.interact(drag_rect, ui.id().with("rec-drag"), Sense::drag());
            let decided = ctx.input(|i| i.pointer.is_decidedly_dragging());
            if bg.dragged() && decided && !self.drag_sent { self.drag_sent = true; ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag); }
            if !bg.dragged() { self.drag_sent = false; }
            for k in 0..3 { for j in 0..2 { painter.circle_filled(Pos2::new(panel.left() + 7.0 + j as f32 * 4.0, panel.center().y - 4.0 + k as f32 * 4.0), 1.1, pal.text.gamma_multiply(0.6)); } }

            let dims = format!("{}×{}", self.region.width().round() as i64, self.region.height().round() as i64);
            let time = linkshot_video::format_duration(self.elapsed_ms.min(u32::MAX as u64) as u32);
            let (title, sub, dot) = match self.phase {
                Phase::Ready => ("Ready to record".to_string(), format!("{dims} · {} fps{}", self.fps, if inside { " · toolbar is inside the region" } else { "" }), pal.selection_orange),
                Phase::Countdown => { let n = 3 - (self.phase_since.elapsed().as_secs_f32().floor() as u32).min(2); (format!("Recording in {n}…"), dims.clone(), pal.selection_orange) }
                Phase::Starting => ("Starting…".to_string(), dims.clone(), pal.selection_orange),
                Phase::Recording => ("Recording".to_string(), format!("{time} · {} frames{}", self.frames, if self.dropped > 0 { format!(" · {} dropped", self.dropped) } else { String::new() }), pal.record),
                Phase::Paused => ("Paused".to_string(), format!("{time} · {} frames", self.frames), pal.input_secondary),
                Phase::Finishing => ("Finishing…".to_string(), format!("{time} · {} frames", self.frames), pal.green),
            };
            let blink = self.phase != Phase::Recording || (self.phase_since.elapsed().as_millis() / 600) % 2 == 0;
            if blink { painter.circle_filled(Pos2::new(panel.left() + 26.0, panel.center().y), 6.0, dot); }
            painter.text(Pos2::new(panel.left() + 40.0, panel.center().y - 9.0), Align2::LEFT_CENTER, &title, crate::theme::medium(13.0), pal.text_bright);
            painter.text(Pos2::new(panel.left() + 40.0, panel.center().y + 9.0), Align2::LEFT_CENTER, self.error.clone().unwrap_or(sub), FontId::proportional(11.5), if self.error.is_some() { pal.capture_red } else { pal.text });
            if self.phase == Phase::Countdown {
                let n = 3 - (self.phase_since.elapsed().as_secs_f32().floor() as u32).min(2);
                painter.text(Pos2::new(panel.right() - 160.0, panel.center().y), Align2::CENTER_CENTER, n.to_string(), crate::theme::semibold(34.0), pal.selection_orange);
            }

            // Buttons, right to left.
            let mut x = panel.right() - 8.0;
            let btn_h = 30.0;
            let mut button = |ui: &mut egui::Ui, label: &str, w: f32, accent: Option<Color32>, enabled: bool, tip: &str| -> bool {
                x -= w;
                let r = Rect::from_min_size(Pos2::new(x, panel.center().y - btn_h / 2.0), Vec2::new(w, btn_h));
                x -= 6.0;
                let resp = ui.interact(r, ui.id().with(label), if enabled { Sense::click() } else { Sense::hover() });
                let fill = match (accent, resp.hovered() && enabled) { (Some(c), true) => c.gamma_multiply(1.15), (Some(c), false) => c, (None, true) => pal.control_bright, (None, false) => pal.control };
                ui.painter().rect_filled(r, 4.0, if enabled { fill } else { pal.control_dark });
                let fg = match (accent, enabled) { (Some(_), true) => Color32::WHITE, (None, true) => pal.text_bright, (_, false) => pal.text };
                ui.painter().text(r.center(), Align2::CENTER_CENTER, label, FontId::proportional(13.0), fg);
                let resp = if tip.is_empty() { resp } else { resp.on_hover_text(tip) };
                enabled && resp.clicked()
            };
            let finishing = self.phase == Phase::Finishing;
            if button(ui, "Cancel", 70.0, None, !finishing, "Discard the recording (Esc)") { self.cancel(); }
            match self.phase {
                Phase::Ready => {
                    if button(ui, "● Record", 96.0, Some(pal.record), true, "Start recording (Enter, Shift+F9)") { self.begin(); }
                    let c = self.cursor;
                    if button(ui, if c { "Cursor: on" } else { "Cursor: off" }, 92.0, if c { Some(pal.product) } else { None }, true, "Record the mouse pointer") { self.cursor = !c; }
                    let _ = button(ui, "Mic", 44.0, None, false, "Microphone recording is not available yet");
                }
                Phase::Countdown | Phase::Starting => { let _ = button(ui, "● Record", 96.0, Some(pal.record.gamma_multiply(0.6)), false, ""); }
                Phase::Recording | Phase::Paused | Phase::Finishing => {
                    if button(ui, "■ Stop", 84.0, Some(pal.green), !finishing, "Stop and open in the Editor (Enter, Shift+F10)") { self.stop(); }
                    let paused = self.phase == Phase::Paused;
                    if button(ui, if paused { "▶ Resume" } else { "❚❚ Pause" }, 96.0, None, !finishing, "Pause / resume (Space, Shift+F9)") { self.toggle_pause(); }
                }
            }
        });
        ctx.request_repaint_after(Duration::from_millis(if self.phase == Phase::Ready { 250 } else { 100 }));
    }
}
