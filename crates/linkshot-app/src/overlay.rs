//! The capture overlay: frozen screenshot on every monitor, orange crosshair, magnifier, window
//! highlight, region drag (may span monitors) and the All-in-One HUD (Image / Video / Scrolling).
//!
//! One `Overlay` holds the shared state; the app shows one fullscreen viewport per monitor and calls
//! [`Overlay::ui`] with that monitor's index. All geometry is kept in logical virtual-desktop
//! coordinates so a drag that crosses monitors just works.

use crate::icons;
use crate::theme::Palette;
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2};
use linkshot_capture::Snapshot;
use linkshot_core::enums::{CaptureMode, SelectionMode};
use linkshot_core::presets::CaptureSettings;
use linkshot_core::{Pt, Rect as LRect};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HudChoice { Image, Video, Scrolling }

pub enum OverlayResult {
    Cancel,
    Image { region: LRect, cursor: Option<Pt> },
    Video { region: LRect },
    Scrolling { region: LRect },
}

#[derive(Clone, Copy)]
enum Phase {
    /// Moving the crosshair; `Some` while dragging (logical start point, monitor the drag started on).
    Selecting { drag_start: Option<(Pt, usize)> },
    /// A region is selected (logical); the HUD is visible (All-in-One) or we auto-complete (Image/Video).
    Selected { rect: LRect },
}

pub struct Overlay {
    pub snapshot: Snapshot,
    pub settings: CaptureSettings,
    textures: Vec<Option<egui::TextureHandle>>,
    phase: Phase,
    pub show_magnifier: bool,
    pub result: Option<OverlayResult>,
    hover_window: Option<LRect>,
    hud_drag: Vec2,
    width_text: String,
    height_text: String,
    pub frames: u32,
    /// Set once the compositor has been asked to put each overlay window on its monitor.
    pub placement_started: bool,
    /// Monitor whose viewport last saw the pointer.
    pub active_monitor: usize,
    /// When only one overlay window can be shown (non-KDE Wayland): the monitor it ended up on,
    /// resolved from the viewport size.
    pub single_monitor: Option<usize>,
    single_resolved: bool,
    drag_point: std::cell::Cell<Pt>,
}

impl Overlay {
    pub fn new(snapshot: Snapshot, monitor_idx: usize, settings: CaptureSettings, show_magnifier: bool) -> Self {
        let n = snapshot.monitors.len();
        Self { snapshot, settings, textures: (0..n).map(|_| None).collect(), phase: Phase::Selecting { drag_start: None }, show_magnifier, result: None, hover_window: None, hud_drag: Vec2::ZERO, width_text: String::new(), height_text: String::new(), frames: 0, placement_started: false, active_monitor: monitor_idx, single_monitor: None, single_resolved: false, drag_point: std::cell::Cell::new(Pt::new(0.0, 0.0)) }
    }

    /// Logical rect of the monitor that best contains `r` (for placing HUDs next to it).
    pub fn monitor_for(&self, r: LRect) -> LRect {
        let c = r.center();
        self.snapshot.monitor_at(c.x, c.y).or(Some(self.active_monitor)).and_then(|i| self.snapshot.monitors.get(i)).map(|m| m.rect()).unwrap_or(r)
    }

    fn mrect(&self, m: usize) -> LRect { self.snapshot.monitors[m].rect() }

    /// Viewport points → logical virtual-desktop coordinates (linear, so it extends beyond the monitor
    /// while a drag is grabbed by this window).
    fn to_logical(&self, m: usize, view: Rect, p: Pos2) -> Pt {
        let mr = self.mrect(m);
        Pt::new(mr.min.x + (p.x - view.left()) as f64 / view.width() as f64 * mr.width(), mr.min.y + (p.y - view.top()) as f64 / view.height() as f64 * mr.height())
    }
    fn to_view(&self, m: usize, view: Rect, p: Pt) -> Pos2 {
        let mr = self.mrect(m);
        Pos2::new(view.left() + ((p.x - mr.min.x) / mr.width()) as f32 * view.width(), view.top() + ((p.y - mr.min.y) / mr.height()) as f32 * view.height())
    }
    fn rect_to_view(&self, m: usize, view: Rect, r: LRect) -> Rect {
        Rect::from_min_max(self.to_view(m, view, r.min), self.to_view(m, view, r.max))
    }

    /// Single-window fallback (non-KDE Wayland): match the viewport size against the monitors.
    fn resolve_single(&mut self, ctx: &egui::Context) -> usize {
        if let Some(i) = self.single_monitor { if self.single_resolved { return i; } }
        let size = ctx.content_rect().size();
        let ppp = ctx.pixels_per_point();
        let best = self.snapshot.monitors.iter().enumerate().min_by_key(|(_, m)| {
            let dl = (m.width as f32 - size.x).abs() + (m.height as f32 - size.y).abs();
            let dp = (m.width as f32 - size.x * ppp).abs() + (m.height as f32 - size.y * ppp).abs();
            (dl.min(dp) * 10.0) as i64
        });
        let i = best.map(|(i, _)| i).unwrap_or(0);
        if self.frames > 3 || best.map(|(_, m)| ((m.width as f32 - size.x).abs() + (m.height as f32 - size.y).abs()) < 8.0).unwrap_or(false) { self.single_resolved = true; }
        self.single_monitor = Some(i);
        i
    }

    /// Draws the overlay for monitor `m` into the current viewport.
    pub fn ui(&mut self, ctx: &egui::Context, pal: &Palette, m: usize) {
        self.frames += 1;
        let m = if self.single_monitor.is_some() { self.resolve_single(ctx) } else { m.min(self.snapshot.monitors.len().saturating_sub(1)) };
        if self.textures[m].is_none() {
            let img = &self.snapshot.images[m];
            let ci = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
            // Linear when the frozen screen is drawn 1:1 or minified, nearest when the magnifier blows it up.
            let topts = egui::TextureOptions { magnification: egui::TextureFilter::Nearest, minification: egui::TextureFilter::Linear, wrap_mode: egui::TextureWrapMode::ClampToEdge, mipmap_mode: None };
            self.textures[m] = Some(ctx.load_texture(format!("overlay-bg-{m}"), ci, topts));
        }
        let orange = pal.selection_orange;
        let virt = self.snapshot.virtual_rect();
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
            let view = ui.max_rect();
            let painter = ui.painter_at(view);
            if let Some(t) = &self.textures[m] {
                painter.image(t.id(), view, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
            }
            let resp = ui.interact(view, ui.id().with(("overlay", m)), Sense::click_and_drag());
            let pointer_here = ctx.input(|i| i.pointer.latest_pos());
            let has_pointer = pointer_here.is_some();
            if has_pointer { self.active_monitor = m; }
            let pointer = pointer_here.unwrap_or(view.center());
            let lptr = self.to_logical(m, view, pointer);
            let mods = ctx.input(|i| i.modifiers);
            let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));
            let enter = ctx.input(|i| i.key_pressed(egui::Key::Enter));
            if esc { self.result = Some(OverlayResult::Cancel); return; }
            if ctx.input(|i| i.key_pressed(egui::Key::M)) { self.show_magnifier = !self.show_magnifier; }

            // Window under the cursor (not while dragging).
            if has_pointer && matches!(self.phase, Phase::Selecting { drag_start: None }) {
                self.hover_window = self.snapshot.window_at(lptr.x, lptr.y, "linkshot").map(|w| w.rect());
            } else if has_pointer { self.hover_window = None; }

            let phase = self.phase;
            match phase {
                Phase::Selecting { drag_start } => {
                    let mut drag_start = drag_start;
                    if resp.drag_started() { drag_start = Some((ctx.input(|i| i.pointer.press_origin()).map(|p| self.to_logical(m, view, p)).unwrap_or(lptr), m)); }
                    let dragging_here = drag_start.map(|(_, dm)| dm == m).unwrap_or(false);
                    if let Some((start, _)) = drag_start {
                        // The dragging viewport updates the rect; every viewport draws its part of it.
                        let mut cur = if dragging_here { lptr } else { Pt::new(f64::NAN, f64::NAN) };
                        if cur.x.is_nan() { cur = self.last_drag_point(); }
                        let mut r = LRect::from_points(start, cur);
                        let aspect = if mods.ctrl && mods.shift { Some(4.0 / 3.0) } else if mods.ctrl { Some(16.0 / 9.0) } else if mods.shift { Some(1.0) } else { None };
                        if let Some(a) = aspect {
                            let w = r.width().max(r.height() * a);
                            let h = w / a;
                            let sx = if cur.x >= start.x { 1.0 } else { -1.0 };
                            let sy = if cur.y >= start.y { 1.0 } else { -1.0 };
                            r = LRect::from_points(start, Pt::new(start.x + w * sx, start.y + h * sy));
                        }
                        r = LRect { min: r.min.max(virt.min), max: r.max.min(virt.max) };
                        self.set_last_drag_point(cur);
                        let vr = self.rect_to_view(m, view, r);
                        dim_outside(&painter, view, vr);
                        painter.rect_stroke(vr, 0.0, Stroke::new(1.5_f32, orange), egui::StrokeKind::Outside);
                        draw_handles(&painter, vr.intersect(view), orange);
                        let r_round = r.round_out();
                        if dragging_here {
                            let label = format!("{} × {}", r_round.width() as i64, r_round.height() as i64);
                            label_box(&painter, Pos2::new(vr.center().x.clamp(view.left() + 60.0, view.right() - 60.0), (vr.bottom() + 18.0).min(view.bottom() - 14.0)), &label, pal);
                            if self.show_magnifier { self.draw_magnifier(&painter, m, view, pointer, pal); }
                            if resp.drag_stopped() {
                                if r_round.width() >= 2.0 && r_round.height() >= 2.0 {
                                    self.width_text = format!("{}", r_round.width() as i64);
                                    self.height_text = format!("{}", r_round.height() as i64);
                                    self.phase = Phase::Selected { rect: r_round };
                                    self.hud_drag = Vec2::ZERO;
                                    return;
                                } else { drag_start = None; }
                            }
                        }
                        self.phase = Phase::Selecting { drag_start };
                    } else {
                        self.phase = Phase::Selecting { drag_start };
                        if !has_pointer { return; }
                        // window highlight
                        if let Some(w) = self.hover_window {
                            let vw = self.rect_to_view(m, view, w).intersect(view);
                            painter.rect_stroke(vw, 0.0, Stroke::new(3.0_f32, orange), egui::StrokeKind::Inside);
                            painter.rect_filled(vw, 0.0, Color32::from_rgba_unmultiplied(0xFB, 0xB9, 0x01, 18));
                        }
                        // crosshair
                        if !crate::app::is_simplified_crosshair() {
                            painter.line_segment([Pos2::new(view.left(), pointer.y), Pos2::new(view.right(), pointer.y)], Stroke::new(1.0_f32, orange));
                            painter.line_segment([Pos2::new(pointer.x, view.top()), Pos2::new(pointer.x, view.bottom())], Stroke::new(1.0_f32, orange));
                        } else {
                            painter.line_segment([pointer - Vec2::new(14.0, 0.0), pointer + Vec2::new(14.0, 0.0)], Stroke::new(1.0_f32, orange));
                            painter.line_segment([pointer - Vec2::new(0.0, 14.0), pointer + Vec2::new(0.0, 14.0)], Stroke::new(1.0_f32, orange));
                        }
                        if self.show_magnifier { self.draw_magnifier(&painter, m, view, pointer, pal); }
                        else { label_box(&painter, pointer + Vec2::new(60.0, -28.0), &format!("{}, {}", lptr.x as i64, lptr.y as i64), pal); }
                        // click (no drag) on a window = capture that window; Enter = capture hover window or this monitor
                        if resp.clicked() || enter {
                            if let Some(w) = self.hover_window { self.width_text.clear(); self.phase = Phase::Selected { rect: w.round_out() }; self.hud_drag = Vec2::ZERO; }
                            else if enter { self.phase = Phase::Selected { rect: self.mrect(m) }; self.hud_drag = Vec2::ZERO; }
                        }
                        // help hint
                        let hint = "Drag to select a region  •  Click a window  •  Enter: fullscreen  •  Shift 1:1 / Ctrl 16:9  •  M: magnifier  •  Esc: cancel";
                        let hint_r = Rect::from_center_size(Pos2::new(view.center().x, view.top() + 30.0), Vec2::new(840.0, 32.0));
                        painter.rect_filled(hint_r.translate(Vec2::new(0.0, 2.0)), 16.0, Color32::from_black_alpha(60));
                        painter.rect_filled(hint_r, 16.0, Color32::from_rgba_unmultiplied(0x17, 0x1B, 0x20, 225));
                        painter.text(hint_r.center(), Align2::CENTER_CENTER, hint, FontId::proportional(13.0), Color32::from_rgb(0xE6, 0xEB, 0xF0));
                    }
                }
                Phase::Selected { rect } => {
                    let r = rect;
                    let vr = self.rect_to_view(m, view, r);
                    dim_outside(&painter, view, vr);
                    painter.rect_stroke(vr, 0.0, Stroke::new(1.5_f32, orange), egui::StrokeKind::Outside);
                    draw_handles(&painter, vr.intersect(view), orange);
                    if self.show_magnifier && has_pointer { self.draw_magnifier(&painter, m, view, pointer, pal); }
                    let mode = self.settings.capture_mode;
                    let wants_hud = mode == CaptureMode::CaptureModeAllInOne || mode == CaptureMode::CaptureModeVideo;
                    if !wants_hud {
                        // Image tab: capture immediately once the region is chosen.
                        let cursor = if self.settings.capture_cursor { Some(lptr) } else { None };
                        self.result = Some(match self.settings.selection_mode {
                            SelectionMode::SelectionModePanoramic | SelectionMode::SelectionModeScrollingWindow => OverlayResult::Scrolling { region: r },
                            _ => OverlayResult::Image { region: r, cursor },
                        });
                        return;
                    }
                    // The HUD lives on the monitor holding the selection's centre.
                    let hud_monitor = self.snapshot.monitor_at(r.center().x, r.center().y).unwrap_or(self.active_monitor);
                    let mut choice: Option<HudChoice> = None;
                    let mut cancel = false;
                    let mut redo = false;
                    let mut hud_rect = Rect::NOTHING;
                    if hud_monitor == m {
                        let hud_size = Vec2::new(if mode == CaptureMode::CaptureModeVideo { 330.0 } else { 378.0 }, 50.0);
                        let below = vr.bottom() + 12.0 + hud_size.y <= view.bottom();
                        let base = if below { Pos2::new(vr.center().x, vr.bottom() + 12.0 + hud_size.y / 2.0) } else { Pos2::new(vr.center().x, (vr.top() - 12.0 - hud_size.y / 2.0).max(view.top() + hud_size.y / 2.0)) };
                        hud_rect = Rect::from_center_size(base + self.hud_drag, hud_size);
                        hud_rect = hud_rect.translate(Vec2::new((view.left() - hud_rect.left()).max(0.0) + (view.right() - hud_rect.right()).min(0.0), (view.top() - hud_rect.top()).max(0.0) + (view.bottom() - hud_rect.bottom()).min(0.0)));
                        let hud_resp = ui.interact(hud_rect, ui.id().with("hud"), Sense::click_and_drag());
                        painter.rect_filled(hud_rect.translate(Vec2::new(0.0, 4.0)).expand(2.0), 10.0, Color32::from_black_alpha(70));
                        painter.rect_filled(hud_rect, 8.0, pal.dark_panel);
                        painter.rect_stroke(hud_rect, 8.0, Stroke::new(1.0_f32, pal.control_outline), egui::StrokeKind::Inside);
                        ui.scope_builder(egui::UiBuilder::new().max_rect(hud_rect.shrink2(Vec2::new(9.0, 9.0))), |ui| {
                            ui.horizontal_centered(|ui| {
                                let btn = |ui: &mut egui::Ui, tip: &str, draw: &dyn Fn(&egui::Painter, Rect, Color32)| -> bool {
                                    let (rr, resp) = ui.allocate_exact_size(Vec2::splat(32.0), Sense::click());
                                    let t = ui.ctx().animate_bool(resp.id, resp.hovered());
                                    if t > 0.0 { ui.painter().rect_filled(rr, 6.0, pal.control_bright.gamma_multiply(t)); }
                                    draw(ui.painter(), rr.shrink(4.0), pal.text_bright);
                                    resp.on_hover_text(tip).clicked()
                                };
                                if mode == CaptureMode::CaptureModeAllInOne {
                                    if btn(ui, "Image capture (I)", &|p, r, c| icons::camera_icon(p, r, c)) { choice = Some(HudChoice::Image); }
                                    if btn(ui, "Video capture (V)", &|p, r, c| icons::video_icon(p, r, c)) { choice = Some(HudChoice::Video); }
                                    if btn(ui, "Scrolling capture (S)", &|p, r, c| icons::scrolling_icon(p, r, c)) { choice = Some(HudChoice::Scrolling); }
                                } else if btn(ui, "Record (Shift+F9)", &|p, r, _c| { p.circle_filled(r.center(), 11.0, pal.record); }) { choice = Some(HudChoice::Video); }
                                ui.add_space(6.0);
                                let field = |ui: &mut egui::Ui, text: &mut String| {
                                    egui::Frame::new().fill(pal.input_primary).stroke(Stroke::new(1.0_f32, pal.control_outline)).corner_radius(6.0).inner_margin(egui::Margin::symmetric(4, 3)).show(ui, |ui| {
                                        ui.add(egui::TextEdit::singleline(text).desired_width(42.0).frame(false).horizontal_align(egui::Align::Center));
                                    }).response
                                };
                                field(ui, &mut self.width_text);
                                let xr = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover()).0;
                                icons::close_icon(ui.painter(), xr, pal.text);
                                field(ui, &mut self.height_text);
                                ui.add_space(6.0);
                                if btn(ui, "Redo selection", &|p, r, c| icons::redo_selection_icon(p, r, c)) { redo = true; }
                                if btn(ui, "Cancel (Esc)", &|p, r, c| icons::close_icon(p, r, c)) { cancel = true; }
                            });
                        });
                        // Typed width/height adjust the region (anchored at top-left).
                        if let (Ok(w), Ok(h)) = (self.width_text.trim().parse::<f64>(), self.height_text.trim().parse::<f64>()) {
                            if (w - r.width()).abs() >= 1.0 || (h - r.height()).abs() >= 1.0 {
                                if w >= 2.0 && h >= 2.0 {
                                    let nr = LRect::from_xywh(r.min.x, r.min.y, w.min(virt.max.x - r.min.x), h.min(virt.max.y - r.min.y));
                                    self.phase = Phase::Selected { rect: nr };
                                }
                            }
                        }
                        if hud_resp.dragged() { self.hud_drag += hud_resp.drag_delta(); }
                    }
                    if ctx.input(|i| i.key_pressed(egui::Key::I)) && mode == CaptureMode::CaptureModeAllInOne { choice = Some(HudChoice::Image); }
                    if ctx.input(|i| i.key_pressed(egui::Key::V)) { choice = Some(HudChoice::Video); }
                    if ctx.input(|i| i.key_pressed(egui::Key::S)) && mode == CaptureMode::CaptureModeAllInOne { choice = Some(HudChoice::Scrolling); }
                    if enter && mode == CaptureMode::CaptureModeAllInOne { choice = Some(HudChoice::Image); }
                    // Clicking outside the HUD and the selection restarts selection.
                    if resp.clicked() && !hud_rect.contains(pointer) && !vr.contains(pointer) { redo = true; }
                    if redo { self.phase = Phase::Selecting { drag_start: None }; return; }
                    if cancel { self.result = Some(OverlayResult::Cancel); return; }
                    if let Some(c) = choice {
                        let cursor = if self.settings.capture_cursor { Some(lptr) } else { None };
                        self.result = Some(match c {
                            HudChoice::Image => OverlayResult::Image { region: r, cursor },
                            HudChoice::Video => OverlayResult::Video { region: r },
                            HudChoice::Scrolling => OverlayResult::Scrolling { region: r },
                        });
                    }
                }
            }
        });
        ctx.request_repaint();
    }

    // Last pointer position of an in-progress drag (shared so non-dragging viewports can draw the rect).
    fn last_drag_point(&self) -> Pt { self.drag_point.get() }
    fn set_last_drag_point(&self, p: Pt) { self.drag_point.set(p); }

    /// Magnifier: a 240 px loupe at 4.5× (about 53×53 points of screen) that follows the pointer, with
    /// the pixel coordinates and the colour under the crosshair. Shown while hovering, dragging and
    /// after selection.
    fn draw_magnifier(&self, painter: &egui::Painter, m: usize, view: Rect, pointer: Pos2, pal: &Palette) {
        let Some(t) = &self.textures[m] else { return };
        let size = 240.0;
        let zoom = 4.5;
        let ppp = painter.ctx().pixels_per_point().max(0.5);
        let label_h = 22.0;
        let gap = 24.0;
        let mut pos = pointer + Vec2::new(gap, gap);
        if pos.x + size > view.right() { pos.x = pointer.x - gap - size; }
        if pos.y + size + label_h > view.bottom() { pos.y = pointer.y - gap - size - label_h; }
        pos.x = pos.x.max(view.left());
        pos.y = pos.y.max(view.top());
        let r = Rect::from_min_size(pos, Vec2::splat(size));
        let half = size / zoom / 2.0;
        let uv = Rect::from_min_max(
            Pos2::new((pointer.x - half - view.left()) / view.width(), (pointer.y - half - view.top()) / view.height()),
            Pos2::new((pointer.x + half - view.left()) / view.width(), (pointer.y + half - view.top()) / view.height()),
        );
        let frame = Rect::from_min_max(r.min, r.max + Vec2::new(0.0, label_h));
        painter.rect_filled(frame.expand(3.0).translate(Vec2::new(0.0, 3.0)), 8.0, Color32::from_black_alpha(110));
        painter.rect_filled(frame.expand(2.0), 7.0, pal.dark_panel);
        painter.image(t.id(), r, uv, Color32::WHITE);
        // One grid cell = one physical screen pixel; skip the grid when cells get too small to read.
        let step = zoom / ppp;
        if step >= 4.0 {
            let mut x = c_align(r.left(), r.center().x, step);
            while x <= r.right() + 0.1 { painter.line_segment([Pos2::new(x, r.top()), Pos2::new(x, r.bottom())], Stroke::new(0.5_f32, Color32::from_black_alpha(50))); x += step; }
            let mut y = c_align(r.top(), r.center().y, step);
            while y <= r.bottom() + 0.1 { painter.line_segment([Pos2::new(r.left(), y), Pos2::new(r.right(), y)], Stroke::new(0.5_f32, Color32::from_black_alpha(50))); y += step; }
        }
        let c = r.center();
        painter.line_segment([Pos2::new(r.left(), c.y), Pos2::new(r.right(), c.y)], Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(0xFB, 0xB9, 0x01, 140)));
        painter.line_segment([Pos2::new(c.x, r.top()), Pos2::new(c.x, r.bottom())], Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(0xFB, 0xB9, 0x01, 140)));
        painter.rect_stroke(Rect::from_center_size(c, Vec2::splat(step)), 0.0, Stroke::new(1.5_f32, pal.selection_orange), egui::StrokeKind::Outside);
        painter.rect_stroke(r, 0.0, Stroke::new(1.0_f32, pal.control_outline), egui::StrokeKind::Outside);
        let lp = self.to_logical(m, view, pointer);
        let img = &self.snapshot.images[m];
        let mr = self.mrect(m);
        let sx = img.width() as f64 / mr.width().max(1.0);
        let sy = img.height() as f64 / mr.height().max(1.0);
        let px = (((lp.x - mr.min.x) * sx).max(0.0) as u32).min(img.width().saturating_sub(1));
        let py = (((lp.y - mr.min.y) * sy).max(0.0) as u32).min(img.height().saturating_sub(1));
        let p = img.get_pixel(px, py);
        let text = format!("{}, {}   #{:02X}{:02X}{:02X}", lp.x as i64, lp.y as i64, p[0], p[1], p[2]);
        let lr = Rect::from_min_max(Pos2::new(r.left(), r.bottom()), Pos2::new(r.right(), r.bottom() + label_h));
        painter.rect_filled(Rect::from_center_size(Pos2::new(lr.left() + 11.0, lr.center().y), Vec2::splat(12.0)), 3.0, Color32::from_rgb(p[0], p[1], p[2]));
        painter.text(Pos2::new(lr.left() + 22.0, lr.center().y), Align2::LEFT_CENTER, text, FontId::monospace(12.0), pal.text_bright);
    }
}

/// First grid line at or after `start` such that the grid passes through `center`.
fn c_align(start: f32, center: f32, step: f32) -> f32 {
    let n = ((start - center) / step).ceil();
    center + n * step
}

fn dim_outside(p: &egui::Painter, view: Rect, r: Rect) {
    let r = r.intersect(view);
    let dim = Color32::from_black_alpha(110);
    if r.is_negative() || r.width() <= 0.0 || r.height() <= 0.0 { p.rect_filled(view, 0.0, dim); return; }
    p.rect_filled(Rect::from_min_max(view.min, Pos2::new(view.right(), r.top())), 0.0, dim);
    p.rect_filled(Rect::from_min_max(Pos2::new(view.left(), r.bottom()), view.max), 0.0, dim);
    p.rect_filled(Rect::from_min_max(Pos2::new(view.left(), r.top()), Pos2::new(r.left(), r.bottom())), 0.0, dim);
    p.rect_filled(Rect::from_min_max(Pos2::new(r.right(), r.top()), Pos2::new(view.right(), r.bottom())), 0.0, dim);
}

fn draw_handles(p: &egui::Painter, r: Rect, c: Color32) {
    if r.width() <= 0.0 || r.height() <= 0.0 { return; }
    for pt in [r.left_top(), r.center_top(), r.right_top(), r.left_center(), r.right_center(), r.left_bottom(), r.center_bottom(), r.right_bottom()] {
        p.rect_filled(Rect::from_center_size(pt, Vec2::splat(7.0)), 1.5, Color32::WHITE);
        p.rect_stroke(Rect::from_center_size(pt, Vec2::splat(7.0)), 1.5, Stroke::new(1.0_f32, c), egui::StrokeKind::Outside);
    }
}

fn label_box(p: &egui::Painter, center: Pos2, text: &str, pal: &Palette) {
    let galley = p.layout_no_wrap(text.to_string(), FontId::proportional(12.5), pal.text_bright);
    let r = Rect::from_center_size(center, galley.size() + Vec2::new(16.0, 8.0));
    p.rect_filled(r.translate(Vec2::new(0.0, 2.0)), 6.0, Color32::from_black_alpha(70));
    p.rect_filled(r, 6.0, pal.dark_panel);
    p.galley(r.min + Vec2::new(8.0, 4.0), galley, pal.text_bright);
}
