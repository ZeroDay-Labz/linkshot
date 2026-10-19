//! The floating toolbar shown during a scrolling capture: Start, Auto-scroll, frame counter, Done, Cancel.
//!
//! It is a separate small always-on-top viewport placed just outside the captured region so it never
//! appears in the frames.

use crate::theme::Palette;
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2};
use linkshot_capture::scrolling::{Command, Event, Session};
use linkshot_capture::stitch::{Axis, Push};
use linkshot_core::presets::CaptureSettings;

pub const HUD_W: f32 = 560.0;
pub const HUD_H: f32 = 64.0;

pub struct ScrollingUi {
    pub session: Session,
    pub settings: CaptureSettings,
    pub region: linkshot_core::Rect,
    pub monitor: linkshot_core::Rect,
    pub started: bool,
    pub auto: bool,
    pub frames: u32,
    pub accepted: usize,
    pub length: u32,
    pub axis: Option<Axis>,
    pub last: Option<Push>,
    pub finishing: bool,
    pub error: Option<String>,
    pub new_error: Option<String>,
    pub new_token: Option<String>,
    /// Set when the session reports its final image (`Some(None)` = cancelled).
    pub result: Option<Option<image::RgbaImage>>,
    hud_rect_sent: bool,
    /// Consecutive frames the stitcher could not place (the user scrolled too far).
    pub misses: u32,
    ui_frames: u32,
    placed: bool,
    drag_sent: bool,
}

pub const HUD_TITLE: &str = "LinkShot Scrolling Capture";

impl ScrollingUi {
    pub fn new(session: Session, settings: CaptureSettings, region: linkshot_core::Rect, monitor: linkshot_core::Rect, auto: bool, started: bool) -> Self {
        Self { session, settings, region, monitor, started, auto, frames: 0, accepted: 0, length: 0, axis: None, last: None, finishing: false, error: None, new_error: None, new_token: None, result: None, hud_rect_sent: false, misses: 0, ui_frames: 0, placed: false, drag_sent: false }
    }

    /// Where the toolbar goes: centred under the region, or above it when there is no room (logical px).
    pub fn position(&self) -> [f32; 2] {
        let x = (self.region.center().x as f32 - HUD_W / 2.0).max(self.monitor.min.x as f32 + 8.0).min(self.monitor.max.x as f32 - HUD_W - 8.0);
        let below = self.region.max.y as f32 + 14.0;
        let y = if below + HUD_H + 8.0 <= self.monitor.max.y as f32 { below } else { (self.region.min.y as f32 - 14.0 - HUD_H).max(self.monitor.min.y as f32 + 8.0) };
        [x, y]
    }

    pub fn viewport_builder(&self) -> egui::ViewportBuilder {
        let [x, y] = self.position();
        egui::ViewportBuilder::default()
            .with_title(HUD_TITLE)
            .with_app_id(crate::app_id())
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_taskbar(false)
            .with_resizable(false)
            .with_inner_size([HUD_W, HUD_H])
            .with_position([x, y])
    }

    pub fn poll(&mut self) {
        while let Ok(ev) = self.session.events.try_recv() {
            match ev {
                Event::Progress { frames, accepted, length, last, axis, auto, misses } => { self.frames = frames; self.accepted = accepted; self.length = length; self.last = last; self.axis = axis; self.auto = auto; self.misses = misses; }
                Event::AutoFinished => { self.auto = false; self.finish(); }
                Event::RestoreToken(t) => self.new_token = Some(t),
                Event::Error(e) => { self.error = Some(e.clone()); self.new_error = Some(e); }
                Event::Done(img) => self.result = Some(img),
            }
        }
    }

    pub fn finish(&mut self) { if !self.finishing { self.finishing = true; self.session.send(Command::Finish); } }
    pub fn cancel(&mut self) { self.finishing = true; self.session.send(Command::Cancel); }

    pub fn ui(&mut self, ctx: &egui::Context, pal: &Palette) {
        self.poll();
        self.ui_frames += 1;
        if !self.hud_rect_sent { self.hud_rect_sent = true; ctx.send_viewport_cmd(egui::ViewportCommand::Focus); }
        // Wayland ignores the requested position; on KDE ask the compositor to move the toolbar out
        // of the captured region once the window exists.
        #[cfg(target_os = "linux")]
        if self.ui_frames == 2 && !self.placed && linkshot_capture::is_wayland() && linkshot_capture::kwin::is_kde() {
            self.placed = true;
            let [x, y] = self.position();
            std::thread::spawn(move || { if let Err(e) = linkshot_capture::kwin::place_window_retry(HUD_TITLE, x.round() as i32, y.round() as i32) { tracing::warn!("HUD placement: {e:#}"); } });
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) { self.cancel(); }
        if ctx.input(|i| i.key_pressed(egui::Key::Enter)) { if self.started { self.finish(); } else { self.started = true; self.session.send(Command::Start); } }
        let orange = pal.selection_orange;
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(Color32::TRANSPARENT)).show(ctx, |ui| {
            let view = ui.max_rect();
            let panel = view.shrink(2.0);
            let painter = ui.painter_at(view);
            painter.rect_filled(panel, 6.0, pal.dark_panel);
            painter.rect_stroke(panel, 6.0, Stroke::new(1.0_f32, pal.control_outline), egui::StrokeKind::Inside);
            // The status area drags the window (Wayland cannot position it for us). The buttons are
            // kept out of the drag region, and the move only starts once the pointer really moves:
            // egui would otherwise hand a plain click to the compositor as a window move.
            let drag_rect = Rect::from_min_max(panel.min, Pos2::new(panel.right() - 300.0, panel.bottom()));
            let bg = ui.interact(drag_rect, ui.id().with("hud-drag"), Sense::drag());
            let decided = ctx.input(|i| i.pointer.is_decidedly_dragging());
            if bg.dragged() && decided && !self.drag_sent { self.drag_sent = true; ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag); }
            if !bg.dragged() { self.drag_sent = false; }
            // grip dots
            for k in 0..3 { for j in 0..2 { painter.circle_filled(Pos2::new(panel.left() + 7.0 + j as f32 * 4.0, panel.center().y - 4.0 + k as f32 * 4.0), 1.1, pal.text.gamma_multiply(0.6)); } }

            // Status text on the left.
            let (title, sub) = if self.finishing {
                ("Stitching…".to_string(), format!("{} frames", self.accepted))
            } else if !self.started {
                ("Ready to capture".to_string(), "Press Start, then scroll the content".to_string())
            } else if self.misses >= 4 {
                ("Lost the overlap".to_string(), "Scroll back a little so the last frame is visible again".to_string())
            } else if self.auto {
                ("Automatic scrolling…".to_string(), Self::progress_text(self.accepted, self.length, self.axis))
            } else {
                ("Scroll the content, then press Done".to_string(), Self::progress_text(self.accepted, self.length, self.axis))
            };
            let lost = self.misses >= 4 && !self.finishing;
            painter.circle_filled(Pos2::new(panel.left() + 26.0, panel.center().y), 6.0, if self.finishing { pal.green } else if lost { orange } else if self.started { pal.capture_red } else { orange });
            painter.text(Pos2::new(panel.left() + 40.0, panel.center().y - 9.0), Align2::LEFT_CENTER, &title, crate::theme::medium(13.0), pal.text_bright);
            painter.text(Pos2::new(panel.left() + 40.0, panel.center().y + 9.0), Align2::LEFT_CENTER, self.error.clone().unwrap_or(sub), FontId::proportional(11.5), if self.error.is_some() { pal.capture_red } else if lost { orange } else { pal.text });

            // Buttons on the right.
            let mut x = panel.right() - 8.0;
            let btn_h = 30.0;
            let mut button = |ui: &mut egui::Ui, label: &str, w: f32, accent: Option<Color32>, enabled: bool| -> bool {
                x -= w;
                let r = Rect::from_min_size(Pos2::new(x, panel.center().y - btn_h / 2.0), Vec2::new(w, btn_h));
                x -= 6.0;
                let resp = ui.interact(r, ui.id().with(label), if enabled { Sense::click() } else { Sense::hover() });
                let fill = match (accent, resp.hovered() && enabled) { (Some(c), true) => c.gamma_multiply(1.15), (Some(c), false) => c, (None, true) => pal.control_bright, (None, false) => pal.control };
                ui.painter().rect_filled(r, 4.0, if enabled { fill } else { pal.control_dark });
                let fg = match (accent, enabled) { (Some(_), true) => Color32::WHITE, (None, true) => pal.text_bright, (_, false) => pal.text };
                ui.painter().text(r.center(), Align2::CENTER_CENTER, label, FontId::proportional(13.0), fg);
                enabled && resp.clicked()
            };
            if button(ui, "Cancel", 70.0, None, !self.finishing) { self.cancel(); }
            if !self.started {
                if button(ui, "Start", 84.0, Some(pal.green), !self.finishing) { self.started = true; self.session.send(Command::Start); }
            } else {
                if button(ui, "Done", 84.0, Some(pal.green), !self.finishing) { self.finish(); }
                let auto_label = if self.auto { "Stop auto-scroll" } else { "Auto-scroll" };
                if button(ui, auto_label, if self.auto { 118.0 } else { 96.0 }, if self.auto { Some(pal.product) } else { None }, !self.finishing) {
                    self.auto = !self.auto;
                    self.session.send(Command::SetAuto(self.auto));
                }
            }
        });
        ctx.request_repaint_after(std::time::Duration::from_millis(80));
    }

    fn progress_text(accepted: usize, length: u32, axis: Option<Axis>) -> String {
        match axis {
            Some(Axis::Horizontal) => format!("{accepted} frames · {length} px wide"),
            _ if accepted > 1 => format!("{accepted} frames · {length} px tall"),
            _ => format!("{accepted} frame"),
        }
    }
}
