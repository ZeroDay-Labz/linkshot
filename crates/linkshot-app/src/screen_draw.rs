//! Screen Draw: while recording, a transparent layer over the region where the user draws arrows,
//! squares and numbered steps that fade after a few seconds. The layer is a click-through window
//! until draw mode is entered (Ctrl+Shift+D, the toolbar button), so it never gets in the way, and
//! because the recorder grabs the screen the drawings end up in the video exactly as shown.

use crate::theme::Palette;
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2};
use linkshot_core::Rect as LRect;
use std::time::{Duration, Instant};

pub const DRAW_TITLE: &str = "LinkShot Screen Draw";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool { Arrow, Square, Step }

enum Kind { Arrow(Pos2, Pos2), Square(Rect), Step(Pos2, u32) }

struct Item { kind: Kind, created: Instant, color: Color32 }

pub const COLORS: [Color32; 5] = [
    Color32::from_rgb(0xE8, 0x08, 0x08),
    Color32::from_rgb(0xFB, 0xB9, 0x01),
    Color32::from_rgb(0x2E, 0xC2, 0x6A),
    Color32::from_rgb(0x2C, 0x7C, 0xB2),
    Color32::WHITE,
];

pub struct ScreenDraw {
    pub region: LRect,
    pub active: bool,
    pub tool: Tool,
    pub fade: Duration,
    pub color_index: usize,
    items: Vec<Item>,
    step_n: u32,
    drag: Option<(Pos2, Pos2)>,
    placed: bool,
    passthrough_sent: Option<bool>,
    focus_pending: bool,
}

impl ScreenDraw {
    pub fn new(region: LRect, fade_secs: u32, active: bool) -> Self {
        Self { region, active, tool: Tool::Arrow, fade: Duration::from_secs(fade_secs.clamp(1, 60) as u64), color_index: 0, items: vec![], step_n: 0, drag: None, placed: false, passthrough_sent: None, focus_pending: active }
    }

    pub fn toggle(&mut self) { self.set_active(!self.active); }

    pub fn set_active(&mut self, on: bool) {
        if self.active == on { return; }
        self.active = on;
        self.drag = None;
        if on { self.focus_pending = true; }
    }

    /// The window only exists while drawing or while something is still fading.
    pub fn needs_window(&self) -> bool { self.active || !self.items.is_empty() }

    pub fn viewport_builder(&self) -> egui::ViewportBuilder {
        let r = self.region;
        egui::ViewportBuilder::default()
            .with_title(DRAW_TITLE)
            .with_app_id(crate::app_id())
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_taskbar(false)
            .with_resizable(false)
            .with_mouse_passthrough(!self.active)
            .with_inner_size([r.width().max(1.0) as f32, r.height().max(1.0) as f32])
            .with_position([r.min.x as f32, r.min.y as f32])
    }

    fn place_on_kde(&mut self) {
        #[cfg(target_os = "linux")]
        {
            if self.placed || !(linkshot_capture::is_wayland() && linkshot_capture::kwin::is_kde()) { return; }
            self.placed = true;
            let r = self.region;
            std::thread::spawn(move || { if let Err(e) = linkshot_capture::kwin::setup_overlay_window(DRAW_TITLE, r.min.x.round() as i32, r.min.y.round() as i32, r.width().round().max(1.0) as i32, r.height().round().max(1.0) as i32) { tracing::warn!("screen draw placement: {e:#}"); } });
        }
    }

    pub fn color(&self) -> Color32 { COLORS[self.color_index % COLORS.len()] }

    pub fn ui(&mut self, ctx: &egui::Context, pal: &Palette) {
        if self.passthrough_sent != Some(!self.active) {
            self.passthrough_sent = Some(!self.active);
            ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(!self.active));
        }
        if self.focus_pending { self.focus_pending = false; ctx.send_viewport_cmd(egui::ViewportCommand::Focus); }
        self.place_on_kde();
        // Expire faded items.
        let fade = self.fade;
        self.items.retain(|i| i.created.elapsed() < fade);
        if self.active {
            let (esc, a, s, one, c) = ctx.input(|i| (i.key_pressed(egui::Key::Escape), i.key_pressed(egui::Key::A), i.key_pressed(egui::Key::S), i.key_pressed(egui::Key::Num1), i.key_pressed(egui::Key::C)));
            if esc { self.set_active(false); }
            if a { self.tool = Tool::Arrow; }
            if s { self.tool = Tool::Square; }
            if one { self.tool = Tool::Step; self.step_n = 0; }
            if c { self.color_index = (self.color_index + 1) % COLORS.len(); }
        }
        let color = self.color();
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(Color32::TRANSPARENT)).show(ctx, |ui| {
            let view = ui.max_rect();
            let painter = ui.painter_at(view);
            if self.active {
                ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
                let resp = ui.interact(view, ui.id().with("draw-layer"), Sense::click_and_drag());
                let pointer = ctx.input(|i| i.pointer.latest_pos());
                if resp.drag_started() { if let Some(p) = ctx.input(|i| i.pointer.press_origin()) { self.drag = Some((p, p)); } }
                if resp.dragged() { if let (Some(d), Some(p)) = (self.drag.as_mut(), pointer) { d.1 = p; } }
                if resp.drag_stopped() {
                    if let Some((p0, p1)) = self.drag.take() {
                        if (p1 - p0).length() >= 6.0 {
                            let kind = match self.tool { Tool::Square => Kind::Square(Rect::from_two_pos(p0, p1)), _ => Kind::Arrow(p0, p1) };
                            self.items.push(Item { kind, created: Instant::now(), color });
                        }
                    }
                }
                if resp.clicked() {
                    if let Some(p) = pointer {
                        if self.tool == Tool::Step { self.step_n += 1; self.items.push(Item { kind: Kind::Step(p, self.step_n), created: Instant::now(), color }); }
                    }
                }
                // Thin tinted frame so the user sees where drawing is possible, plus a hint.
                painter.rect_stroke(view.shrink(1.0), 0.0, Stroke::new(2.0_f32, color.gamma_multiply(0.7)), egui::StrokeKind::Inside);
                let tool = match self.tool { Tool::Arrow => "Arrow", Tool::Square => "Square", Tool::Step => "Step" };
                let hint = format!("Screen Draw: {tool}  •  A arrow  •  S square  •  1 steps  •  C colour  •  Esc done");
                let hint_r = Rect::from_center_size(Pos2::new(view.center().x, view.top() + 22.0), Vec2::new(430.0_f32.min(view.width() - 8.0), 26.0));
                painter.rect_filled(hint_r, 13.0, Color32::from_rgba_unmultiplied(0x17, 0x1B, 0x20, 225));
                painter.text(hint_r.center(), Align2::CENTER_CENTER, hint, FontId::proportional(12.0), pal.text_bright);
                if let Some((p0, p1)) = self.drag {
                    match self.tool { Tool::Square => draw_square(&painter, Rect::from_two_pos(p0, p1), color), _ => draw_arrow(&painter, p0, p1, color) }
                }
            }
            for it in &self.items {
                let age = it.created.elapsed().as_secs_f32();
                let total = fade.as_secs_f32();
                let alpha = if age > total - 0.6 { ((total - age) / 0.6).clamp(0.0, 1.0) } else { 1.0 };
                let c = it.color.gamma_multiply(alpha);
                match it.kind {
                    Kind::Arrow(a, b) => draw_arrow(&painter, a, b, c),
                    Kind::Square(r) => draw_square(&painter, r, c),
                    Kind::Step(p, n) => {
                        painter.circle_filled(p, 15.0, c);
                        painter.circle_stroke(p, 15.0, Stroke::new(2.0_f32, Color32::WHITE.gamma_multiply(alpha)));
                        painter.text(p, Align2::CENTER_CENTER, n.to_string(), crate::theme::semibold(15.0), Color32::WHITE.gamma_multiply(alpha));
                    }
                }
            }
        });
        ctx.request_repaint_after(Duration::from_millis(if self.active { 16 } else { 100 }));
    }
}

fn draw_arrow(p: &egui::Painter, a: Pos2, b: Pos2, c: Color32) {
    let stroke = Stroke::new(4.0_f32, c);
    let dir = (b - a).normalized();
    if !dir.x.is_finite() { return; }
    let head = 16.0;
    let base = b - dir * head;
    p.line_segment([a, base], stroke);
    let n = Vec2::new(-dir.y, dir.x) * 9.0;
    p.add(egui::Shape::convex_polygon(vec![b, base + n, base - n], c, Stroke::NONE));
}

fn draw_square(p: &egui::Painter, r: Rect, c: Color32) {
    p.rect_stroke(r, 3.0, Stroke::new(4.0_f32, c), egui::StrokeKind::Outside);
}
