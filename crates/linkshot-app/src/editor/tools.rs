//! Tool interactions on the canvas: creating, selecting, moving, resizing and editing objects.

use crate::app::App;
use egui::{Color32, Pos2, Rect, Response, Stroke, Vec2};
use linkshot_core::document::{CaptureObject, Document};
use linkshot_core::enums::*;
use linkshot_core::style::QuickStyle;
use linkshot_core::undo::SnapshotCommand;
use linkshot_core::{Argb, Pt};

#[derive(Clone, Debug, PartialEq)]
pub enum Handle { N, S, E, W, NE, NW, SE, SW, Start, End, Tail(usize), /// Blue handle on a callout: dragging it adds a tail.
    AddTail }

pub enum Drag {
    Create { id: String, start: Pt },
    Move { start: Pt, before: Document },
    Resize { id: String, handle: Handle, before: Document },
    Marquee { start: Pt },
    Region { start: Pt },
    /// Dragging one of the eight crop/selection handles (index as drawn in `draw_overlays`).
    RegionResize { handle: usize, start: linkshot_core::Rect },
    Pen { id: String },
    /// Eraser stroke in document coordinates; applied to the background on release.
    Erase { points: Vec<Pt> },
}

pub struct TextEditState {
    pub id: String,
    pub text: String,
    pub fresh: bool,
}

#[derive(Clone, Debug)]
pub struct EffectsState {
    pub current: Option<EffectType>,
    pub border_color: Argb,
    pub border_width: u32,
    pub shadow_size: u32,
    pub shadow_dir: (i32, i32),
    pub shadow_color: Argb,
    pub edge_type: EdgeType,
    pub edge_size: u32,
    pub edges: [bool; 4],
    pub filter: FilterType,
    pub filter_amount: f32,
    pub brightness: f32,
    pub contrast: f32,
    pub hue: f32,
    pub saturation: f32,
    pub gamma: f32,
    pub replace_from: Argb,
    pub replace_to: Argb,
    pub replace_tolerance: u8,
    pub scale_percent: f32,
}

#[derive(Default)]
pub struct ToolState {
    pub selection: Vec<String>,
    pub drag: Option<Drag>,
    pub text_edit: Option<TextEditState>,
    /// Pending rectangular region for Crop / Selection tools (document space).
    pub region: Option<linkshot_core::Rect>,
    pub poly_points: Vec<Pt>,
    pub fx: EffectsState,
    pub render_nonce: usize,
    pub hover_handle: Option<Handle>,
    /// Snapshot taken when a creation drag started (committed as one undo step when it ends).
    pub pending_before: Option<Document>,
    /// Stamp picked in the side panel (index into `EditorState::stamps`); clicks place it.
    pub current_stamp: Option<usize>,
}

impl ToolState {
    /// Objects being created, moved or resized right now: the rasteriser skips them and
    /// `draw_overlays` paints a light preview instead, so a drag never re-renders the canvas.
    pub fn preview_ids(&self) -> Vec<String> {
        match &self.drag {
            Some(Drag::Create { id, .. }) | Some(Drag::Resize { id, .. }) | Some(Drag::Pen { id }) => vec![id.clone()],
            Some(Drag::Move { .. }) => self.selection.clone(),
            _ => vec![],
        }
    }
}

impl ToolState {
    pub fn clear_selection(&mut self) { self.selection.clear(); self.text_edit = None; self.region = None; self.poly_points.clear(); self.drag = None; }
    pub fn on_tool_changed(&mut self) { self.text_edit = None; self.region = None; self.poly_points.clear(); self.drag = None; }
}

impl Default for EffectsState {
    fn default() -> Self {
        Self { current: None, border_color: Argb::BLACK, border_width: 4, shadow_size: 12, shadow_dir: (1, 1), shadow_color: Argb::BLACK, edge_type: EdgeType::Torn, edge_size: 18, edges: [false, true, false, true], filter: FilterType::Grayscale, filter_amount: 50.0, brightness: 0.0, contrast: 0.0, hue: 0.0, saturation: 0.0, gamma: 1.0, replace_from: Argb::WHITE, replace_to: Argb::BLACK, replace_tolerance: 20, scale_percent: 100.0 }
    }
}

const HANDLE: f32 = 8.0;

/// Crop/selection region handles in document space: corners first, then edge midpoints.
fn region_handles(r: linkshot_core::Rect) -> [Pt; 8] {
    let (l, t, rr, b) = (r.min.x, r.min.y, r.max.x, r.max.y);
    let (cx, cy) = ((l + rr) / 2.0, (t + b) / 2.0);
    [Pt::new(l, t), Pt::new(rr, t), Pt::new(l, b), Pt::new(rr, b), Pt::new(cx, t), Pt::new(cx, b), Pt::new(l, cy), Pt::new(rr, cy)]
}

/// Moves the edge(s) owned by handle `i` (see `region_handles`) to `p`.
fn resize_region(start: linkshot_core::Rect, i: usize, p: Pt) -> linkshot_core::Rect {
    let (mut l, mut t, mut r, mut b) = (start.min.x, start.min.y, start.max.x, start.max.y);
    if matches!(i, 0 | 2 | 6) { l = p.x; }
    if matches!(i, 1 | 3 | 7) { r = p.x; }
    if matches!(i, 0 | 1 | 4) { t = p.y; }
    if matches!(i, 2 | 3 | 5) { b = p.y; }
    linkshot_core::Rect::from_points(Pt::new(l.min(r), t.min(b)), Pt::new(l.max(r), t.max(b))).round_out()
}

/// Builds a new object from the tool's current properties between two document points.
pub fn object_from_style(tool: ToolMode, q: &QuickStyle, a: Pt, b: Pt) -> Option<CaptureObject> {
    let mut o = CaptureObject::new(match tool {
        ToolMode::Arrow => "Arrow", ToolMode::Line => "Line", ToolMode::Shape => "Shape", ToolMode::Highlight => "Highlight",
        ToolMode::Text => "Text", ToolMode::Callouts => "Callout", ToolMode::Step => "Step", ToolMode::Pen => "Pen",
        ToolMode::Blur => "Blur", ToolMode::Magnify => "Magnify", _ => return None,
    });
    o.points_array = vec![a, b];
    o.opacity = Some(q.opacity);
    o.foreground_color = Some(q.foreground_color);
    o.background_color = Some(q.background_color);
    o.stroke_width = Some(q.stroke_width);
    o.dash_type = Some(q.dash_type);
    o.drop_shadow_enabled = Some(q.drop_shadow_enabled);
    o.shadow_color = Some(q.shadow_color);
    o.shadow_blur = Some(q.shadow_blur);
    o.shadow_opacity = Some(q.shadow_opacity);
    o.shadow_direction_x = Some(q.shadow_direction_x);
    o.shadow_direction_y = Some(q.shadow_direction_y);
    match tool {
        ToolMode::Arrow | ToolMode::Line => {
            o.arrow_start = Some(q.arrow_start);
            o.arrow_end = Some(if tool == ToolMode::Line && q.arrow_end == ArrowLineEndType::RoundedArrow { ArrowLineEndType::Flat } else { q.arrow_end });
            o.arrow_start_width = Some(q.arrow_start_width);
            o.arrow_end_width = Some(q.arrow_end_width);
            o.bezier_curve = Some(q.arrow_type == LineType::Bezier || q.line_type == LineType::Bezier);
            if o.bezier_curve == Some(true) { o.control_points = vec![a.lerp(b, 0.5) + Pt::new(0.0, -40.0)]; }
            o.background_color = Some(Argb::TRANSPARENT);
        }
        ToolMode::Shape => { o.tool_shape = Some(q.tool_shape); o.corner_radius_ratio = Some(0.35); }
        ToolMode::Highlight => { o.foreground_color = Some(q.foreground_color); }
        ToolMode::Text | ToolMode::Callouts => {
            o.font_family = Some(q.font_family.clone());
            o.font_size = Some(q.font_size);
            o.text_selection_bold = Some(q.text_selection_bold);
            o.text_selection_italic = Some(q.text_selection_italic);
            o.text_selection_underline = Some(q.text_selection_underline);
            o.text_selection_color = Some(q.text_selection_color);
            o.text_outline_color = Some(q.text_outline_color);
            o.text_outline_width = Some(q.text_outline_width);
            o.tool_horizontal_align = Some(q.tool_horizontal_align.as_str().into());
            o.tool_vertical_align = Some(q.tool_vertical_align.as_str().into());
            o.tool_padding = Some(q.tool_padding);
            if tool == ToolMode::Callouts {
                o.callout_shape = Some(q.callout_shape);
                o.tail_style = Some(q.tail_style);
                o.tail_head_style = Some(q.tail_head_style);
                o.tail_line_style = Some(q.tail_line_style);
                o.tail_width = Some(q.tail_width);
                o.tail_color = Some(q.tail_color);
                o.border_style = Some(BorderStyle::Middle);
                let r = linkshot_core::Rect::from_points(a, b);
                o.callout_tails = vec![Pt::new(r.min.x + r.width() * 0.25, r.max.y + r.height() * 0.6 + 20.0)];
            }
            o.set_plain_text("");
        }
        ToolMode::Step => {
            o.step_style = Some(q.step_style);
            o.step_sequence_type = Some(q.step_sequence_type);
            o.font_family = Some(q.font_family.clone());
            o.font_size = Some(q.font_size);
            o.stroke_width = Some(0.0);
        }
        ToolMode::Pen => { o.pen_shape = Some(q.pen_shape); o.smoothing = Some(q.smoothing); o.antialiasing = Some(q.antialiasing); o.background_color = Some(Argb::TRANSPARENT); }
        ToolMode::Blur => { o.blur_type = Some(q.blur_type); o.blur_intensity = Some(q.blur_intensity.max(10.0)); o.foreground_color = None; o.background_color = None; o.stroke_width = None; o.dash_type = None; o.drop_shadow_enabled = None; o.shadow_color = None; o.shadow_blur = None; o.shadow_opacity = None; o.shadow_direction_x = None; o.shadow_direction_y = None; o.opacity = None; }
        ToolMode::Magnify => { o.magnify_scale = Some(q.magnify_scale); o.magnify_is_offset = Some(false); o.tool_shape = Some(q.tool_shape); }
        _ => {}
    }
    Some(o)
}

/// Applies a quick style's visual properties to an existing object (property panel / quick style click).
pub fn apply_style(o: &mut CaptureObject, q: &QuickStyle) {
    let tool = tool_of(o);
    if let Some(mut n) = object_from_style(tool, q, o.p0(), o.p1()) {
        // keep geometry, text, id
        n.object_id = o.object_id.clone();
        n.points_array = o.points_array.clone();
        n.start_point = o.start_point; n.end_point = o.end_point;
        n.control_points = o.control_points.clone();
        n.callout_tails = if o.callout_tails.is_empty() { n.callout_tails } else { o.callout_tails.clone() };
        n.rotation_angle = o.rotation_angle;
        if o.is_text_like() {
            let text = o.plain_text();
            n.set_plain_text(&text);
        }
        n.step_value = o.step_value.clone();
        n.extra = o.extra.clone();
        *o = n;
    }
}

pub fn tool_of(o: &CaptureObject) -> ToolMode {
    match o.tool_mode.as_str() {
        "Arrow" => ToolMode::Arrow, "Line" => ToolMode::Line, "Shape" => ToolMode::Shape, "Highlight" => ToolMode::Highlight,
        "Text" => ToolMode::Text, "Callout" => ToolMode::Callouts, "Step" => ToolMode::Step, "Pen" => ToolMode::Pen,
        "Blur" => ToolMode::Blur, "Magnify" => ToolMode::Magnify, "Stamp" => ToolMode::Stamp, _ => ToolMode::VectorSelection,
    }
}

fn dist_to_segment(p: Pt, a: Pt, b: Pt) -> f64 {
    let ab = b - a;
    let len2 = ab.x * ab.x + ab.y * ab.y;
    if len2 < 1e-9 { return p.distance(a); }
    let t = (((p.x - a.x) * ab.x + (p.y - a.y) * ab.y) / len2).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

/// Topmost object under `p` (document space), with a tolerance in document pixels.
pub fn hit_test(doc: &Document, p: Pt, tol: f64) -> Option<String> {
    for o in doc.page().capture_objects.iter().rev() {
        if o.locked() { continue; }
        let hit = match o.tool_mode.as_str() {
            "Arrow" | "Line" => dist_to_segment(p, o.p0(), o.p1()) <= (o.stroke_width() / 2.0 + tol).max(tol),
            "Pen" => o.points_array.windows(2).any(|w| dist_to_segment(p, w[0], w[1]) <= o.stroke_width() / 2.0 + tol),
            _ => o.bounds().expand(tol).contains(p),
        };
        if hit { return Some(o.object_id.clone()); }
    }
    None
}

fn handles_for(o: &CaptureObject, cv: &super::canvas::CanvasState) -> Vec<(Handle, Pos2)> {
    match o.tool_mode.as_str() {
        "Arrow" | "Line" => {
            let mut v = vec![(Handle::Start, cv.doc_to_screen(o.p0())), (Handle::End, cv.doc_to_screen(o.p1()))];
            if o.bezier_curve.unwrap_or(false) { if let Some(c) = o.control_points.first() { v.push((Handle::N, cv.doc_to_screen(*c))); } }
            v
        }
        "Pen" => vec![],
        _ => {
            let r = cv.doc_rect_to_screen(linkshot_core::Rect::from_points(o.p0(), o.p1()));
            let mut v = vec![(Handle::NW, r.left_top()), (Handle::N, r.center_top()), (Handle::NE, r.right_top()), (Handle::W, r.left_center()), (Handle::E, r.right_center()), (Handle::SW, r.left_bottom()), (Handle::S, r.center_bottom()), (Handle::SE, r.right_bottom())];
            for (i, t) in o.callout_tails.iter().enumerate() { v.push((Handle::Tail(i), cv.doc_to_screen(*t))); }
            if o.tool_mode == "Callout" { v.push((Handle::AddTail, Pos2::new(r.center().x + 18.0, r.bottom() + 14.0))); }
            v
        }
    }
}

fn begin(app: &mut App) -> Option<Document> { app.editor.current_doc().map(|d| d.doc.clone()) }
fn commit(app: &mut App, name: &str, before: Document) {
    if let Some(d) = app.editor.current_doc_mut() {
        if d.doc.pages != before.pages || d.doc.blobs != before.blobs {
            let after = d.doc.clone();
            d.undo.push(Box::new(SnapshotCommand::new(name, before, after)), &mut d.doc);
            d.version += 1;
        }
    }
}
// `changed_at` holds back the tray thumbnail (a full re-render) until a drag or stroke pauses.
fn bump(app: &mut App) { if let Some(d) = app.editor.current_doc_mut() { d.version += 1; d.changed_at = std::time::Instant::now(); } }

pub fn handle_input(app: &mut App, ui: &mut egui::Ui, resp: &Response, canvas: Rect, ctx: &egui::Context) {
    let Some(_) = app.editor.current else { return };
    let tool = app.editor.tool;
    let cv_zoom = app.editor.canvas.zoom;
    let pointer = resp.interact_pointer_pos().or_else(|| resp.hover_pos());
    let doc_pt = pointer.map(|p| app.editor.canvas.screen_to_doc(p));
    let shift = ctx.input(|i| i.modifiers.shift);
    let ctrl = ctx.input(|i| i.modifiers.ctrl);
    let primary_down = ctx.input(|i| i.pointer.primary_down());

    // Finish text editing when clicking elsewhere / pressing Escape.
    if let Some(te) = &app.editor.tools.text_edit {
        let id = te.id.clone();
        let obj_rect = app.editor.current_doc().and_then(|d| d.doc.page().capture_objects.iter().find(|o| o.object_id == id).map(|o| app.editor.canvas.doc_rect_to_screen(o.bounds())));
        let clicked_outside = resp.clicked() && pointer.map(|p| obj_rect.map(|r| !r.expand(4.0).contains(p)).unwrap_or(true)).unwrap_or(false);
        if clicked_outside || ctx.input(|i| i.key_pressed(egui::Key::Escape)) { commit_text(app); }
    }

    // Delete / Backspace removes the selection (unless a text field has focus).
    if !ctx.memory(|m| m.focused().is_some()) && ctx.input(|i| i.key_pressed(egui::Key::Delete)) && app.editor.tools.text_edit.is_none() {
        if app.editor.tools.region.is_some() && matches!(tool, ToolMode::Selection | ToolMode::Crop) { region_delete(app); } else { delete_selection(app); }
    }
    if !ctx.memory(|m| m.focused().is_some()) && ctx.input(|i| i.key_pressed(egui::Key::Escape)) { app.editor.tools.region = None; app.editor.tools.poly_points.clear(); if app.editor.tools.text_edit.is_none() { app.editor.tools.selection.clear(); } }
    // Enter applies Crop
    if tool == ToolMode::Crop && ctx.input(|i| i.key_pressed(egui::Key::Enter)) { crop_apply(app); }
    if tool == ToolMode::CutOut && ctx.input(|i| i.key_pressed(egui::Key::Enter)) { cut_out_apply(app); }
    // Ctrl+V pastes an image as an object
    if ctrl && ctx.input(|i| i.key_pressed(egui::Key::V)) && !ctx.memory(|m| m.focused().is_some()) { paste_image(app, ctx); }

    // Arrow keys nudge the selection
    if !ctx.memory(|m| m.focused().is_some()) && !app.editor.tools.selection.is_empty() {
        let mut d = Pt::ZERO;
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft)) { d.x -= 1.0; }
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowRight)) { d.x += 1.0; }
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowUp)) { d.y -= 1.0; }
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowDown)) { d.y += 1.0; }
        if d != Pt::ZERO {
            if shift { d = d * 10.0; }
            let sel = app.editor.tools.selection.clone();
            if let Some(before) = begin(app) {
                if let Some(doc) = app.editor.current_doc_mut() { for o in doc.doc.page_mut().capture_objects.iter_mut() { if sel.contains(&o.object_id) { o.translate(d); } } }
                commit(app, "Move", before);
            }
        }
    }

    let Some(dp) = doc_pt else { return };
    let tol = 6.0 / cv_zoom as f64;
    let hover = pointer.filter(|p| canvas.contains(*p));

    // Cursor + hover handle detection for Move tool
    app.editor.tools.hover_handle = None;
    if tool == ToolMode::VectorSelection && app.editor.tools.drag.is_none() {
        if let (Some(hp), Some(doc)) = (hover, app.editor.current_doc()) {
            let mut hover_handle: Option<Handle> = None;
            for id in &app.editor.tools.selection {
                if let Some(o) = doc.doc.page().capture_objects.iter().find(|o| &o.object_id == id) {
                    for (h, pos) in handles_for(o, &app.editor.canvas) { if pos.distance(hp) <= HANDLE { hover_handle = Some(h); } }
                }
            }
            let over = hover_handle.is_some() || hit_test(&doc.doc, dp, tol).is_some();
            app.editor.tools.hover_handle = hover_handle;
            ctx.set_cursor_icon(match &app.editor.tools.hover_handle { Some(Handle::N | Handle::S) => egui::CursorIcon::ResizeVertical, Some(Handle::E | Handle::W) => egui::CursorIcon::ResizeHorizontal, Some(Handle::NE | Handle::SW) => egui::CursorIcon::ResizeNeSw, Some(Handle::NW | Handle::SE) => egui::CursorIcon::ResizeNwSe, Some(_) => egui::CursorIcon::Grab, None if over => egui::CursorIcon::Move, None => egui::CursorIcon::Default });
        }
    } else if hover.is_some() && matches!(tool, ToolMode::Arrow | ToolMode::Line | ToolMode::Shape | ToolMode::Highlight | ToolMode::Blur | ToolMode::Magnify | ToolMode::Callouts | ToolMode::Text | ToolMode::Crop | ToolMode::Selection | ToolMode::Stamp | ToolMode::CutOut | ToolMode::Eraser) {
        ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
    }

    // ---------------- drag start ----------------
    if resp.drag_started_by(egui::PointerButton::Primary) && app.editor.tools.text_edit.is_none() {
        // Anchor at the press position, not where the drag threshold was crossed.
        let dp = ctx.input(|i| i.pointer.press_origin()).map(|p| app.editor.canvas.screen_to_doc(p)).unwrap_or(dp);
        match tool {
            ToolMode::VectorSelection | ToolMode::Favorites | ToolMode::Fill | ToolMode::Stamp | ToolMode::MagicWand | ToolMode::SimplifiedUI | ToolMode::None | ToolMode::DropZone | ToolMode::LastTool => {
                if tool == ToolMode::VectorSelection {
                    if let Some(h) = app.editor.tools.hover_handle.clone() {
                        // resize the (first) selected object that owns the handle
                        if let (Some(id), Some(before)) = (app.editor.tools.selection.first().cloned(), begin(app)) {
                            let h = if h == Handle::AddTail {
                                // The blue handle: a new tail starts under the pointer and is dragged like any other.
                                let mut idx = 0;
                                if let Some(d) = app.editor.current_doc_mut() { if let Some(o) = d.doc.page_mut().capture_objects.iter_mut().find(|o| o.object_id == id) { o.callout_tails.push(dp); idx = o.callout_tails.len() - 1; } }
                                Handle::Tail(idx)
                            } else { h };
                            app.editor.tools.drag = Some(Drag::Resize { id, handle: h, before });
                        }
                    } else {
                        let hit = app.editor.current_doc().and_then(|d| hit_test(&d.doc, dp, tol));
                        match hit {
                            Some(id) => {
                                if !app.editor.tools.selection.contains(&id) { if shift { app.editor.tools.selection.push(id); } else { app.editor.tools.selection = vec![id]; } }
                                if ctrl {
                                    // Ctrl+drag duplicates
                                    if let Some(before) = begin(app) {
                                        let sel = app.editor.tools.selection.clone();
                                        let mut new_ids = vec![];
                                        if let Some(d) = app.editor.current_doc_mut() {
                                            let page = d.doc.page_mut();
                                            let copies: Vec<CaptureObject> = page.capture_objects.iter().filter(|o| sel.contains(&o.object_id)).cloned().map(|mut o| { o.object_id = linkshot_core::document::new_object_id(); new_ids.push(o.object_id.clone()); o }).collect();
                                            page.capture_objects.extend(copies);
                                        }
                                        app.editor.tools.selection = new_ids;
                                        app.editor.tools.drag = Some(Drag::Move { start: dp, before });
                                    }
                                } else if let Some(before) = begin(app) { app.editor.tools.drag = Some(Drag::Move { start: dp, before }); }
                            }
                            None => { if !shift { app.editor.tools.selection.clear(); } app.editor.tools.drag = Some(Drag::Marquee { start: dp }); }
                        }
                    }
                }
            }
            ToolMode::Eraser => { app.editor.tools.drag = Some(Drag::Erase { points: vec![dp] }); }
            ToolMode::Crop | ToolMode::Selection | ToolMode::CutOut => {
                // A press on one of the region's handles resizes it; anywhere else starts a new region.
                let handle = app.editor.tools.region.and_then(|r| {
                    let press = ctx.input(|i| i.pointer.press_origin()).unwrap_or_default();
                    region_handles(r).iter().enumerate().find(|(_, h)| app.editor.canvas.doc_to_screen(**h).distance(press) <= 9.0).map(|(i, _)| i)
                });
                match (handle, app.editor.tools.region) {
                    (Some(handle), Some(start)) => app.editor.tools.drag = Some(Drag::RegionResize { handle, start }),
                    _ => { app.editor.tools.region = None; app.editor.tools.drag = Some(Drag::Region { start: dp }); }
                }
            }
            ToolMode::Pen => {
                let q = app.editor.current_props();
                if let Some(mut o) = object_from_style(ToolMode::Pen, &q, dp, dp) {
                    o.points_array = vec![dp];
                    let id = o.object_id.clone();
                    if let Some(before) = begin(app) { if let Some(d) = app.editor.current_doc_mut() { d.doc.page_mut().capture_objects.push(o); } app.editor.tools.drag = Some(Drag::Pen { id }); app.editor.tools.pending_before = Some(before); }
                }
            }
            ToolMode::Step => {}
            ToolMode::Shape if app.editor.current_props().tool_shape == linkshot_core::enums::ShapeShape::Polygon => {}
            _ => {
                let q = app.editor.current_props();
                if let Some(o) = object_from_style(tool, &q, dp, dp) {
                    let id = o.object_id.clone();
                    if let Some(before) = begin(app) { if let Some(d) = app.editor.current_doc_mut() { d.doc.page_mut().capture_objects.push(o); } app.editor.tools.drag = Some(Drag::Create { id, start: dp }); app.editor.tools.pending_before = Some(before); }
                }
            }
        }
    }

    // ---------------- dragging ----------------
    if primary_down && app.editor.tools.drag.is_some() {
        let snap = app.settings.canvas_snapping;
        let mut p2 = dp;
        if snap { p2 = Pt::new(p2.x.round(), p2.y.round()); }
        match app.editor.tools.drag.as_ref().unwrap() {
            Drag::Create { id, start } => {
                let (id, start) = (id.clone(), *start);
                let mut b = p2;
                if shift {
                    // square / straight
                    let d = b - start;
                    if matches!(tool, ToolMode::Arrow | ToolMode::Line) { if d.x.abs() > d.y.abs() { b.y = start.y; } else { b.x = start.x; } }
                    else { let m = d.x.abs().max(d.y.abs()); b = start + Pt::new(m * d.x.signum(), m * d.y.signum()); }
                }
                if let Some(d) = app.editor.current_doc_mut() {
                    if let Some(o) = d.doc.page_mut().capture_objects.iter_mut().find(|o| o.object_id == id) {
                        o.points_array = vec![start, b];
                        if o.tool_mode == "Callout" { let r = linkshot_core::Rect::from_points(start, b); o.callout_tails = vec![Pt::new(r.min.x + r.width() * 0.25, r.max.y + (r.height() * 0.6).max(24.0))]; }
                        if o.bezier_curve == Some(true) { let mid = start.lerp(b, 0.5); let n = Pt::new(-(b.y - start.y), b.x - start.x) * 0.25; o.control_points = vec![mid + n]; }
                    }
                }
            }
            Drag::Move { start, before } => {
                let delta = p2 - *start;
                let sel = app.editor.tools.selection.clone();
                // move relative to the drag-start snapshot stored in `before`
                let originals: Vec<CaptureObject> = before.page().capture_objects.iter().filter(|x| sel.contains(&x.object_id)).cloned().collect();
                if let Some(d) = app.editor.current_doc_mut() {
                    for o in d.doc.page_mut().capture_objects.iter_mut() {
                        if let Some(orig) = originals.iter().find(|x| x.object_id == o.object_id) { let mut c = orig.clone(); c.translate(delta); *o = c; }
                    }
                }
            }
            Drag::Resize { id, handle, before } => {
                let (id, handle) = (id.clone(), handle.clone());
                let orig = before.page().capture_objects.iter().find(|x| x.object_id == id).cloned();
                if let (Some(orig), Some(d)) = (orig, app.editor.current_doc_mut()) {
                    if let Some(o) = d.doc.page_mut().capture_objects.iter_mut().find(|o| o.object_id == id) {
                        let (a, b) = (orig.p0(), orig.p1());
                        match handle {
                            Handle::Start => o.set_endpoints(p2, b),
                            Handle::End => o.set_endpoints(a, p2),
                            Handle::Tail(i) => { if let Some(t) = o.callout_tails.get_mut(i) { *t = p2; } }
                            Handle::N if orig.bezier_curve == Some(true) => { o.control_points = vec![p2]; }
                            h => {
                                let r = linkshot_core::Rect::from_points(a, b);
                                let (mut min, mut max) = (r.min, r.max);
                                match h { Handle::N => min.y = p2.y, Handle::S => max.y = p2.y, Handle::W => min.x = p2.x, Handle::E => max.x = p2.x, Handle::NW => min = p2, Handle::SE => max = p2, Handle::NE => { min.y = p2.y; max.x = p2.x; } Handle::SW => { min.x = p2.x; max.y = p2.y; } _ => {} }
                                if shift { let w = (max.x - min.x).abs(); let h2 = (max.y - min.y).abs(); let m = w.max(h2); max = Pt::new(min.x + m * (max.x - min.x).signum(), min.y + m * (max.y - min.y).signum()); }
                                o.set_endpoints(min, max);
                            }
                        }
                    }
                }
            }
            Drag::Region { start } => {
                let mut r = linkshot_core::Rect::from_points(*start, p2).round_out();
                if tool == ToolMode::CutOut {
                    // A Cut Out band spans the whole canvas across the cut direction.
                    let (horizontal, _) = super::side_panel::cut_out_parts(app.editor.current_props().cut_out_style);
                    let page = app.editor.docs[app.editor.current.unwrap()].doc.page().canvas_rect();
                    if horizontal { r.min.x = page.min.x; r.max.x = page.max.x; } else { r.min.y = page.min.y; r.max.y = page.max.y; }
                }
                app.editor.tools.region = Some(r);
            }
            Drag::RegionResize { handle, start } => { app.editor.tools.region = Some(resize_region(*start, *handle, p2)); }
            Drag::Marquee { .. } => {}
            Drag::Erase { .. } => { if let Some(Drag::Erase { points }) = app.editor.tools.drag.as_mut() { if points.last().map(|l| l.distance(dp) > 1.0).unwrap_or(true) { points.push(dp); } } }
            Drag::Pen { id } => {
                let id = id.clone();
                if let Some(d) = app.editor.current_doc_mut() {
                    if let Some(o) = d.doc.page_mut().capture_objects.iter_mut().find(|o| o.object_id == id) {
                        let mut p = dp;
                        if shift { if let Some(f) = o.points_array.first() { let d2 = p - *f; if d2.x.abs() > d2.y.abs() { p.y = f.y; } else { p.x = f.x; } } }
                        if o.points_array.last().map(|l| l.distance(p) > 1.0).unwrap_or(true) { o.points_array.push(p); }
                    }
                }
            }
        }
    }

    // ---------------- drag end ----------------
    if resp.drag_stopped_by(egui::PointerButton::Primary) {
        // The dragged objects were skipped by the rasteriser: force one full render now.
        app.editor.tools.render_nonce += 1;
        if let Some(drag) = app.editor.tools.drag.take() {
            match drag {
                Drag::Create { id, start } => {
                    let tiny = dp.distance(start) < 3.0;
                    let before = app.editor.tools.pending_before.take();
                    if tiny && !matches!(tool, ToolMode::Text | ToolMode::Callouts) {
                        // too small: remove
                        if let Some(d) = app.editor.current_doc_mut() { d.doc.page_mut().capture_objects.retain(|o| o.object_id != id); d.version += 1; }
                    } else {
                        if tiny { // default sizes for text/callout clicks
                            if let Some(d) = app.editor.current_doc_mut() { if let Some(o) = d.doc.page_mut().capture_objects.iter_mut().find(|o| o.object_id == id) { let (w, h) = if tool == ToolMode::Text { (160.0, 40.0) } else { (180.0, 70.0) }; o.set_endpoints(start, start + Pt::new(w, h)); if tool == ToolMode::Callouts { o.callout_tails = vec![start + Pt::new(w * 0.25, h + 30.0)]; } } }
                        }
                        if let Some(b) = before { commit(app, &format!("Add {}", tool.label()), b); }
                        app.editor.tools.selection = vec![id.clone()];
                        if matches!(tool, ToolMode::Text | ToolMode::Callouts) { app.editor.tools.text_edit = Some(TextEditState { id, text: String::new(), fresh: true }); app.editor.tools.render_nonce += 1; }
                    }
                }
                Drag::Move { before, .. } => commit(app, "Move", before),
                Drag::Resize { id, handle: Handle::Tail(i), before } => {
                    // A tail dragged back into the callout body is removed.
                    if let Some(d) = app.editor.current_doc_mut() {
                        if let Some(o) = d.doc.page_mut().capture_objects.iter_mut().find(|o| o.object_id == id) {
                            let body = linkshot_core::Rect::from_points(o.p0(), o.p1());
                            if o.callout_tails.get(i).map(|t| body.contains(*t)).unwrap_or(false) { o.callout_tails.remove(i); }
                        }
                    }
                    commit(app, "Callout Tail", before);
                }
                Drag::Resize { before, .. } => commit(app, "Resize", before),
                Drag::Erase { points } => erase_apply(app, &points),
                Drag::Pen { .. } => { if let Some(b) = app.editor.tools.pending_before.take() { commit(app, "Pen", b); } }
                Drag::Marquee { start } => {
                    let r = linkshot_core::Rect::from_points(start, dp);
                    if let Some(d) = app.editor.current_doc() {
                        let ids: Vec<String> = d.doc.page().capture_objects.iter().filter(|o| { let b = o.bounds(); b.min.x >= r.min.x && b.min.y >= r.min.y && b.max.x <= r.max.x && b.max.y <= r.max.y }).map(|o| o.object_id.clone()).collect();
                        if shift { for id in ids { if !app.editor.tools.selection.contains(&id) { app.editor.tools.selection.push(id); } } } else { app.editor.tools.selection = ids; }
                    }
                }
                Drag::Region { .. } | Drag::RegionResize { .. } => { if let Some(r) = app.editor.tools.region { if r.width() < 2.0 || r.height() < 2.0 { app.editor.tools.region = None; } } }
            }
        }
    }

    // ---------------- clicks ----------------
    if resp.clicked_by(egui::PointerButton::Primary) && app.editor.tools.drag.is_none() {
        match tool {
            ToolMode::VectorSelection => {
                let hit = app.editor.current_doc().and_then(|d| hit_test(&d.doc, dp, tol));
                match hit { Some(id) => { if shift { if let Some(p) = app.editor.tools.selection.iter().position(|x| *x == id) { app.editor.tools.selection.remove(p); } else { app.editor.tools.selection.push(id); } } else { app.editor.tools.selection = vec![id]; } } None => { if !shift { app.editor.tools.selection.clear(); } } }
            }
            ToolMode::Stamp => {
                match app.editor.tools.current_stamp {
                    Some(i) => super::stamps::insert_stamp(app, i, dp),
                    None => app.editor.set_status("Pick a stamp in the panel, then click where it should go."),
                }
            }
            ToolMode::Step => {
                let q = app.editor.current_props();
                let size = (q.stroke_width.max(16.0)).max(q.font_size * 2.0);
                if let Some(before) = begin(app) {
                    let seq = q.step_sequence_type;
                    if let Some(d) = app.editor.current_doc_mut() {
                        let label = d.doc.page_mut().next_step_label(seq);
                        if let Some(mut o) = object_from_style(ToolMode::Step, &q, dp - Pt::new(size / 2.0, size / 2.0), dp + Pt::new(size / 2.0, size / 2.0)) {
                            o.step_value = Some(label);
                            let id = o.object_id.clone();
                            d.doc.page_mut().capture_objects.push(o);
                            app.editor.tools.selection = vec![id];
                        }
                    }
                    commit(app, "Add Step", before);
                }
            }
            ToolMode::Fill => fill_at(app, dp),
            ToolMode::Shape if app.editor.current_props().tool_shape == linkshot_core::enums::ShapeShape::Polygon => {
                // Polygon: every click adds a corner; a double-click or Enter closes it.
                app.editor.tools.poly_points.push(dp);
            }
            ToolMode::Text | ToolMode::Callouts | ToolMode::Shape | ToolMode::Arrow | ToolMode::Line | ToolMode::Highlight | ToolMode::Blur | ToolMode::Magnify => {
                // click on an existing object of the same kind selects it (and edits text)
                if let Some(id) = app.editor.current_doc().and_then(|d| hit_test(&d.doc, dp, tol)) {
                    let is_text = app.editor.current_doc().map(|d| d.doc.page().capture_objects.iter().any(|o| o.object_id == id && o.is_text_like())).unwrap_or(false);
                    app.editor.tools.selection = vec![id.clone()];
                    if is_text && matches!(tool, ToolMode::Text | ToolMode::Callouts) { start_text_edit(app, &id); }
                }
            }
            _ => {}
        }
    }
    if tool == ToolMode::Shape && app.editor.tools.poly_points.len() >= 3 && (resp.double_clicked() || ctx.input(|i| i.key_pressed(egui::Key::Enter))) {
        finish_polygon(app);
        return;
    }
    if resp.double_clicked() {
        if let Some(id) = app.editor.current_doc().and_then(|d| hit_test(&d.doc, dp, tol)) {
            let is_text = app.editor.current_doc().map(|d| d.doc.page().capture_objects.iter().any(|o| o.object_id == id && o.is_text_like())).unwrap_or(false);
            if is_text { app.editor.tools.selection = vec![id.clone()]; start_text_edit(app, &id); }
            else if tool == ToolMode::VectorSelection { if let Some(d) = app.editor.current_doc() { if let Some(o) = d.doc.page().capture_objects.iter().find(|o| o.object_id == id) { let t = tool_of(o); if t != ToolMode::VectorSelection { app.editor.tool = t; } } } }
        }
    }
    let _ = ui;
}

fn start_text_edit(app: &mut App, id: &str) {
    let text = app.editor.current_doc().and_then(|d| d.doc.page().capture_objects.iter().find(|o| o.object_id == id).map(|o| o.plain_text())).unwrap_or_default();
    app.editor.tools.text_edit = Some(TextEditState { id: id.to_string(), text, fresh: true });
    app.editor.tools.render_nonce += 1;
}

pub fn commit_text(app: &mut App) {
    let Some(te) = app.editor.tools.text_edit.take() else { return };
    app.editor.tools.render_nonce += 1;
    let Some(before) = begin(app) else { return };
    let text = te.text.clone();
    if let Some(d) = app.editor.current_doc_mut() {
        let page = d.doc.page_mut();
        if text.trim().is_empty() && page.capture_objects.iter().any(|o| o.object_id == te.id && o.tool_mode == "Text") {
            page.capture_objects.retain(|o| o.object_id != te.id);
            app.editor.tools.selection.retain(|s| *s != te.id);
        } else if let Some(o) = page.capture_objects.iter_mut().find(|o| o.object_id == te.id) {
            o.set_plain_text(&text);
            // auto-grow text boxes vertically to fit lines
            if o.tool_mode == "Text" {
                let lines = text.lines().count().max(1) as f64;
                let h = (o.font_size.unwrap_or(14.0) * 1.3 * lines + 12.0).max(o.bounds().height());
                let r = linkshot_core::Rect::from_points(o.p0(), o.p1());
                o.set_endpoints(r.min, Pt::new(r.max.x, r.min.y + h));
            }
        }
    }
    commit(app, "Edit Text", before);
}

/// Closes the polygon being clicked out with the Shape tool into a Shape object.
pub fn finish_polygon(app: &mut App) {
    let pts = std::mem::take(&mut app.editor.tools.poly_points);
    if pts.len() < 3 { return; }
    let q = app.editor.current_props();
    let (min, max) = pts.iter().fold((pts[0], pts[0]), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
    let Some(mut o) = object_from_style(ToolMode::Shape, &q, min, max) else { return };
    o.tool_shape = Some(linkshot_core::enums::ShapeShape::Polygon);
    o.points_array = pts;
    let id = o.object_id.clone();
    if let Some(before) = begin(app) {
        if let Some(d) = app.editor.current_doc_mut() { d.doc.page_mut().capture_objects.push(o); }
        app.editor.tools.selection = vec![id];
        commit(app, "Polygon", before);
    }
}

/// Eraser: paints the stroke onto the background with the canvas colour (transparent canvases erase
/// to transparency), width and opacity from the tool's properties. Vector objects are untouched.
pub fn erase_apply(app: &mut App, points: &[Pt]) {
    if points.is_empty() { return; }
    let Some(i) = app.editor.current else { return };
    let q = app.editor.current_props();
    let radius = (q.stroke_width.max(1.0) / 2.0) as f64;
    let opacity = (q.opacity / 100.0).clamp(0.0, 1.0);
    let doc = app.editor.docs[i].doc.clone();
    let page = doc.page();
    let Some(png) = doc.background_png(page) else { return };
    let Ok(mut img) = linkshot_render::decode_png(png) else { return };
    let (w, h) = img.dimensions();
    let (ox, oy) = (page.capture_background_location.x, page.capture_background_location.y);
    let c = page.capture_background_color;
    let target = [c.r, c.g, c.b, c.a];
    let mut stamp = |cx: f64, cy: f64| {
        let r2 = radius * radius;
        let (x0, x1) = (((cx - radius).floor()).max(0.0) as i64, ((cx + radius).ceil()).min(w as f64 - 1.0) as i64);
        let (y0, y1) = (((cy - radius).floor()).max(0.0) as i64, ((cy + radius).ceil()).min(h as f64 - 1.0) as i64);
        for y in y0..=y1 { for x in x0..=x1 {
            let (dx, dy) = (x as f64 + 0.5 - cx, y as f64 + 0.5 - cy);
            if dx * dx + dy * dy > r2 { continue; }
            let p = img.get_pixel_mut(x as u32, y as u32);
            for k in 0..4 { p.0[k] = (p.0[k] as f64 * (1.0 - opacity) + target[k] as f64 * opacity).round() as u8; }
        } }
    };
    let mut prev: Option<Pt> = None;
    for p in points {
        let (px, py) = (p.x - ox, p.y - oy);
        match prev {
            Some(q0) => {
                let (qx, qy) = (q0.x - ox, q0.y - oy);
                let n = ((px - qx).abs().max((py - qy).abs()) / (radius * 0.5).max(1.0)).ceil().max(1.0) as usize;
                for k in 1..=n { let t = k as f64 / n as f64; stamp(qx + (px - qx) * t, qy + (py - qy) * t); }
            }
            None => stamp(px, py),
        }
        prev = Some(*p);
    }
    let Ok(out) = linkshot_render::encode_png(&img) else { return };
    app.editor.docs[i].edit("Eraser", |doc| { let n = doc.page().capture_background_image.clone(); doc.blobs.insert(n, std::sync::Arc::new(out)); });
    app.editor.renderer.invalidate();
}

/// A light egui rendering of one object (no shadow, no text layout) for drags.
fn paint_preview(painter: &egui::Painter, cv: &super::canvas::CanvasState, o: &CaptureObject) {
    use crate::theme::argb_to_color32;
    let op = (o.opacity() / 100.0).clamp(0.0, 1.0) as f32;
    let fg = argb_to_color32(o.fg()).gamma_multiply(op);
    let bg = argb_to_color32(o.bg()).gamma_multiply(op);
    let w = (o.stroke_width() as f32 * cv.zoom).max(1.0);
    let (a, b) = (cv.doc_to_screen(o.p0()), cv.doc_to_screen(o.p1()));
    let r = Rect::from_two_pos(a, b);
    match o.tool_mode.as_str() {
        "Arrow" | "Line" => {
            painter.line_segment([a, b], Stroke::new(w, fg));
            if o.tool_mode == "Arrow" {
                let dir = (b - a).normalized();
                if dir.x.is_finite() {
                    let head = (w * 3.0).max(10.0);
                    let base = b - dir * head;
                    let n = Vec2::new(-dir.y, dir.x) * head * 0.5;
                    painter.add(egui::Shape::convex_polygon(vec![b, base + n, base - n], fg, Stroke::NONE));
                }
            }
        }
        "Pen" => {
            let pts: Vec<Pos2> = o.points_array.iter().map(|p| cv.doc_to_screen(*p)).collect();
            if pts.len() >= 2 { painter.add(egui::Shape::line(pts, Stroke::new(w, fg))); } else if let Some(p) = pts.first() { painter.circle_filled(*p, w / 2.0, fg); }
        }
        "Highlight" => { painter.rect_filled(r, 0.0, bg); }
        "Step" => {
            painter.circle_filled(r.center(), r.width().min(r.height()) / 2.0, bg);
            if let Some(v) = &o.step_value { painter.text(r.center(), egui::Align2::CENTER_CENTER, v, egui::FontId::proportional((r.height() * 0.5).clamp(8.0, 64.0)), fg); }
        }
        "Callout" => {
            painter.rect_filled(r, 4.0, bg);
            painter.rect_stroke(r, 4.0, Stroke::new(w.max(1.0), fg), egui::StrokeKind::Outside);
            if let Some(t) = o.callout_tails.first() {
                let tip = cv.doc_to_screen(*t);
                let c = r.center();
                let n = Vec2::new(-(tip.y - c.y), tip.x - c.x).normalized() * (r.width() * 0.12).max(6.0);
                painter.add(egui::Shape::convex_polygon(vec![tip, c + n, c - n], bg, Stroke::NONE));
            }
        }
        "Shape" => {
            match o.tool_shape { Some(linkshot_core::enums::ShapeShape::Ellipse) => { painter.add(egui::Shape::ellipse_filled(r.center(), r.size() / 2.0, bg)); painter.add(egui::Shape::ellipse_stroke(r.center(), r.size() / 2.0, Stroke::new(w, fg))); }
                _ => { let rad = if o.tool_shape == Some(linkshot_core::enums::ShapeShape::RoundedRectangle) { (r.width().min(r.height()) * 0.15).max(2.0) } else { 0.0 }; painter.rect_filled(r, rad, bg); painter.rect_stroke(r, rad, Stroke::new(w, fg), egui::StrokeKind::Outside); } }
        }
        _ => {
            if o.bg().a > 0 { painter.rect_filled(r, 0.0, bg); }
            painter.rect_stroke(r, 0.0, Stroke::new(w.max(1.0), if o.bg().a > 0 { fg } else { fg.gamma_multiply(0.8) }), egui::StrokeKind::Outside);
        }
    }
}

/// Draws selection handles, the in-progress region, and the floating text editor.
pub fn draw_overlays(app: &mut App, ui: &mut egui::Ui, canvas: Rect, ctx: &egui::Context) {
    let pal = app.pal;
    let painter = ui.painter().with_clip_rect(canvas);
    let Some(d) = app.editor.current_doc() else { return };
    let cv = &app.editor.canvas;
    // Selection handles
    for id in &app.editor.tools.selection {
        if let Some(o) = d.doc.page().capture_objects.iter().find(|o| &o.object_id == id) {
            let r = cv.doc_rect_to_screen(linkshot_core::Rect::from_points(o.p0(), o.p1()));
            if !matches!(o.tool_mode.as_str(), "Arrow" | "Line" | "Pen") { painter.rect_stroke(r, 0.0, Stroke::new(1.0_f32, pal.product), egui::StrokeKind::Outside); }
            else { let b = cv.doc_rect_to_screen(o.bounds()); painter.rect_stroke(b.expand(2.0), 0.0, Stroke::new(1.0_f32, pal.product.gamma_multiply(0.5)), egui::StrokeKind::Outside); }
            for (h, pos) in handles_for(o, cv) {
                let (fill, size) = match h { Handle::Tail(_) => (pal.selection_orange, 9.0), Handle::AddTail => (pal.product, 9.0), Handle::N if o.bezier_curve == Some(true) && matches!(o.tool_mode.as_str(), "Arrow" | "Line") => (Color32::WHITE, 8.0), _ => (Color32::WHITE, HANDLE) };
                painter.rect_filled(Rect::from_center_size(pos, Vec2::splat(size)), 1.0, fill);
                painter.rect_stroke(Rect::from_center_size(pos, Vec2::splat(size)), 1.0, Stroke::new(1.0_f32, pal.input_secondary), egui::StrokeKind::Outside);
            }
        }
    }
    // Objects in a drag: painted here instead of by the rasteriser.
    for id in app.editor.tools.preview_ids() {
        if let Some(o) = d.doc.page().capture_objects.iter().find(|o| o.object_id == id) { paint_preview(&painter, cv, o); }
    }
    // Polygon under construction (Shape tool, Polygon): corners so far plus a rubber band to the pointer.
    if app.editor.tool == ToolMode::Shape && !app.editor.tools.poly_points.is_empty() {
        let mut pts: Vec<Pos2> = app.editor.tools.poly_points.iter().map(|p| cv.doc_to_screen(*p)).collect();
        if let Some(p) = ctx.input(|i| i.pointer.hover_pos()) { if canvas.contains(p) { pts.push(p); } }
        painter.add(egui::Shape::line(pts.clone(), Stroke::new(1.5_f32, pal.product)));
        for p in &pts { painter.circle_filled(*p, 3.5, pal.product); }
        if let (Some(a), Some(b)) = (pts.first(), pts.last()) { painter.line_segment([*a, *b], Stroke::new(1.0_f32, pal.product.gamma_multiply(0.4))); }
    }
    // Eraser stroke in progress: shown in the canvas colour.
    if let Some(Drag::Erase { points }) = &app.editor.tools.drag {
        let q = app.editor.current_props();
        let page = d.doc.page();
        let c = page.capture_background_color;
        let color = if c.a == 0 { Color32::from_rgba_unmultiplied(255, 255, 255, 160) } else { crate::theme::argb_to_color32(c).gamma_multiply((q.opacity / 100.0) as f32) };
        let w = (q.stroke_width as f32 * cv.zoom).max(1.0);
        let pts: Vec<Pos2> = points.iter().map(|p| cv.doc_to_screen(*p)).collect();
        if pts.len() >= 2 { painter.add(egui::Shape::line(pts, Stroke::new(w, color))); } else if let Some(p) = pts.first() { painter.circle_filled(*p, w / 2.0, color); }
    }
    // Marquee
    if let Some(Drag::Marquee { start }) = &app.editor.tools.drag {
        if let Some(p) = ctx.input(|i| i.pointer.hover_pos()) {
            let r = Rect::from_two_pos(cv.doc_to_screen(*start), p);
            painter.rect_filled(r, 0.0, pal.product.gamma_multiply(0.15));
            painter.rect_stroke(r, 0.0, Stroke::new(1.0_f32, pal.product), egui::StrokeKind::Inside);
        }
    }
    // Crop / Selection region
    if let Some(r) = app.editor.tools.region {
        let sr = cv.doc_rect_to_screen(r);
        if app.editor.tool == ToolMode::Crop {
            let img = cv.doc_rect_to_screen(d.doc.page().canvas_rect());
            let dim = Color32::from_black_alpha(110);
            painter.rect_filled(Rect::from_min_max(img.min, Pos2::new(img.right(), sr.top())), 0.0, dim);
            painter.rect_filled(Rect::from_min_max(Pos2::new(img.left(), sr.bottom()), img.max), 0.0, dim);
            painter.rect_filled(Rect::from_min_max(Pos2::new(img.left(), sr.top()), Pos2::new(sr.left(), sr.bottom())), 0.0, dim);
            painter.rect_filled(Rect::from_min_max(Pos2::new(sr.right(), sr.top()), Pos2::new(img.right(), sr.bottom())), 0.0, dim);
            painter.rect_stroke(sr, 0.0, Stroke::new(1.5_f32, pal.product), egui::StrokeKind::Outside);
            for p in region_handles(r).map(|h| app.editor.canvas.doc_to_screen(h)) { painter.rect_filled(Rect::from_center_size(p, Vec2::splat(8.0)), 1.0, pal.product); }
            // Crop floaty
            let fr = Rect::from_center_size(Pos2::new(sr.center().x, (sr.bottom() + 28.0).min(canvas.bottom() - 20.0)), Vec2::new(150.0, 30.0));
            painter.rect_filled(fr, 3.0, pal.main_panel);
            ui.scope_builder(egui::UiBuilder::new().max_rect(fr.shrink2(Vec2::new(6.0, 3.0))), |ui| {
                ui.horizontal_centered(|ui| {
                    if crate::widgets::flat_button(ui, &pal, "Crop", true).clicked() { crop_apply(app); }
                    if ui.button("Cancel").clicked() { app.editor.tools.region = None; }
                });
            });
        } else if app.editor.tool == ToolMode::CutOut {
            // The band to remove: tinted red, with a Cut Out floaty.
            painter.rect_filled(sr, 0.0, pal.capture_red.gamma_multiply(0.28));
            painter.rect_stroke(sr, 0.0, Stroke::new(1.5_f32, pal.capture_red), egui::StrokeKind::Outside);
            let fr = Rect::from_center_size(Pos2::new(sr.center().x, (sr.bottom() + 28.0).min(canvas.bottom() - 20.0)), Vec2::new(170.0, 30.0));
            painter.rect_filled(fr, 3.0, pal.main_panel);
            ui.scope_builder(egui::UiBuilder::new().max_rect(fr.shrink2(Vec2::new(6.0, 3.0))), |ui| {
                ui.horizontal_centered(|ui| {
                    if crate::widgets::flat_button(ui, &pal, "Cut Out", true).on_hover_text("Remove the band and join the rest (Enter)").clicked() { cut_out_apply(app); }
                    if ui.button("Cancel").clicked() { app.editor.tools.region = None; }
                });
            });
        } else {
            // marching-ants style selection
            painter.rect_stroke(sr, 0.0, Stroke::new(1.0_f32, Color32::WHITE), egui::StrokeKind::Outside);
            painter.rect_stroke(sr, 0.0, Stroke::new(1.0_f32, Color32::from_black_alpha(160)), egui::StrokeKind::Inside);
            let fr = Rect::from_center_size(Pos2::new(sr.center().x, (sr.bottom() + 28.0).min(canvas.bottom() - 20.0)), Vec2::new(300.0, 30.0));
            painter.rect_filled(fr, 3.0, pal.main_panel);
            ui.scope_builder(egui::UiBuilder::new().max_rect(fr.shrink2(Vec2::new(6.0, 3.0))), |ui| {
                ui.horizontal_centered(|ui| {
                    if ui.button("Delete").on_hover_text("Clear the selected pixels (Delete)").clicked() { region_delete(app); }
                    if ui.button("Copy").clicked() { region_copy(app); }
                    if ui.button("Crop to selection").clicked() { crop_apply(app); }
                    if ui.button("Cancel").clicked() { app.editor.tools.region = None; }
                });
            });
        }
    }
    // Floating text editor
    if let Some(te) = app.editor.tools.text_edit.as_mut() {
        let id = te.id.clone();
        if let Some(o) = app.editor.docs[app.editor.current.unwrap()].doc.page().capture_objects.iter().find(|o| o.object_id == id) {
            let r = app.editor.canvas.doc_rect_to_screen(linkshot_core::Rect::from_points(o.p0(), o.p1()));
            let style = o.text_style();
            let font_px = style.font_size as f32 * app.editor.canvas.zoom;
            let text_color = crate::theme::argb_to_color32(style.color);
            let bg = if o.tool_mode == "Callout" { crate::theme::argb_to_color32(o.bg()) } else { Color32::TRANSPARENT };
            painter.rect_filled(r, 4.0, bg);
            painter.rect_stroke(r, 2.0, Stroke::new(1.0_f32, pal.product), egui::StrokeKind::Outside);
            let pad = (6.0 * app.editor.canvas.zoom).max(2.0);
            let inner = r.shrink(pad);
            let te_widget = egui::TextEdit::multiline(&mut te.text).frame(false).font(egui::FontId::proportional(font_px.max(6.0))).text_color(text_color).desired_width(inner.width()).horizontal_align(match style.align { HAlign::Left => egui::Align::Min, HAlign::Center => egui::Align::Center, HAlign::Right => egui::Align::Max });
            let resp = ui.put(inner, te_widget);
            if te.fresh { resp.request_focus(); te.fresh = false; }
            // Ctrl+Enter commits
            if resp.has_focus() && ctx.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.ctrl) { commit_text(app); }
        } else { app.editor.tools.text_edit = None; }
    }
}

pub fn delete_selection(app: &mut App) {
    if app.editor.tools.selection.is_empty() { return; }
    let sel = app.editor.tools.selection.clone();
    if let Some(d) = app.editor.current_doc_mut() { d.edit("Delete", |doc| doc.page_mut().capture_objects.retain(|o| !sel.contains(&o.object_id))); }
    app.editor.tools.selection.clear();
}

/// Moves selected objects in z-order: +1/-1 step, or to front/back with ±MAX.
pub fn reorder(app: &mut App, dir: i32) {
    let sel = app.editor.tools.selection.clone();
    if sel.is_empty() { return; }
    if let Some(d) = app.editor.current_doc_mut() {
        d.edit("Arrange", |doc| {
            let objs = &mut doc.page_mut().capture_objects;
            let selected: Vec<CaptureObject> = objs.iter().filter(|o| sel.contains(&o.object_id)).cloned().collect();
            if dir == i32::MAX { objs.retain(|o| !sel.contains(&o.object_id)); objs.extend(selected); }
            else if dir == i32::MIN { objs.retain(|o| !sel.contains(&o.object_id)); let mut v = selected; v.extend(objs.drain(..)); *objs = v; }
            else if dir > 0 { for i in (0..objs.len().saturating_sub(1)).rev() { if sel.contains(&objs[i].object_id) && !sel.contains(&objs[i + 1].object_id) { objs.swap(i, i + 1); } } }
            else { for i in 1..objs.len() { if sel.contains(&objs[i].object_id) && !sel.contains(&objs[i - 1].object_id) { objs.swap(i, i - 1); } } }
        });
    }
}

/// Flattens the selected objects into the background bitmap.
pub fn flatten_selection(app: &mut App) {
    let sel = app.editor.tools.selection.clone();
    if sel.is_empty() { return; }
    let Some(i) = app.editor.current else { return };
    let mut doc = app.editor.docs[i].doc.clone();
    // Render only background + selected objects, then keep the rest as objects.
    let rest: Vec<CaptureObject> = doc.page().capture_objects.iter().filter(|o| !sel.contains(&o.object_id)).cloned().collect();
    doc.page_mut().capture_objects.retain(|o| sel.contains(&o.object_id));
    if let Some(img) = app.editor.renderer.flatten(&doc, doc.page()) {
        if let Ok(png) = linkshot_render::encode_png(&img) {
            let d = &mut app.editor.docs[i];
            d.edit("Flatten", |doc| { let page = doc.page_mut(); page.capture_objects = rest; page.capture_background_location = Pt::ZERO; let name = page.capture_background_image.clone(); doc.blobs.insert(name, std::sync::Arc::new(png)); });
            app.editor.renderer.invalidate();
        }
    }
    app.editor.tools.selection.clear();
}

/// Applies `f` to the flattened image and makes the result the new (object-free) document image.
pub fn whole_image_op(app: &mut App, name: &str, f: impl FnOnce(&image::RgbaImage) -> image::RgbaImage) {
    let Some(i) = app.editor.current else { return };
    let doc = app.editor.docs[i].doc.clone();
    let Some(img) = app.editor.renderer.flatten(&doc, doc.page()) else { return };
    let out = f(&img);
    let Ok(png) = linkshot_render::encode_png(&out) else { return };
    let (w, h) = (out.width() as f64, out.height() as f64);
    app.editor.docs[i].edit(name, |doc| { let page = doc.page_mut(); page.capture_objects.clear(); page.capture_canvas_width = w; page.capture_canvas_height = h; page.capture_background_location = Pt::ZERO; page.corner_radius_ratio = Some(0.0); let n = page.capture_background_image.clone(); doc.blobs.insert(n, std::sync::Arc::new(png)); });
    app.editor.renderer.invalidate();
    app.editor.tools.clear_selection();
    app.editor.canvas.fit_requested = true;
}

/// Crop to the pending region (Crop tool) — flattens first.
pub fn crop_apply(app: &mut App) {
    let Some(r) = app.editor.tools.region.take() else { return };
    let Some(i) = app.editor.current else { return };
    let page = app.editor.docs[i].doc.page().canvas_rect();
    let r = linkshot_core::Rect { min: r.min.max(page.min), max: r.max.min(page.max) }.round_out();
    if r.is_empty() { return; }
    whole_image_op(app, "Crop", move |img| image::imageops::crop_imm(img, r.min.x as u32, r.min.y as u32, r.width() as u32, r.height() as u32).to_image());
}

/// Cut Out: removes the band under the region and joins the two remaining parts, with the chosen
/// seam (none, fade, tear, sharktooth, wave, saw). Annotations in the band go; those past it move up.
pub fn cut_out_apply(app: &mut App) {
    use linkshot_core::enums::EdgeType;
    let Some(r) = app.editor.tools.region.take() else { return };
    let Some(i) = app.editor.current else { return };
    let (horizontal, edge) = super::side_panel::cut_out_parts(app.editor.current_props().cut_out_style);
    let kind = match edge.as_str() { "Fade" => EdgeType::Fade, "Tear" => EdgeType::Torn, "Sharktooth" => EdgeType::Sharktooth, "Wave" => EdgeType::Wave, "Saw" => EdgeType::Saw, _ => EdgeType::NoEdge };
    let doc = app.editor.docs[i].doc.clone();
    let page_rect = doc.page().canvas_rect();
    let band = linkshot_core::Rect { min: r.min.max(page_rect.min), max: r.max.min(page_rect.max) }.round_out();
    if band.is_empty() { return; }
    // Only the background is cut; the annotations are moved as objects.
    let mut bg_doc = doc.clone();
    bg_doc.page_mut().capture_objects.clear();
    let Some(img) = app.editor.renderer.flatten(&bg_doc, bg_doc.page()) else { return };
    let (w, h) = img.dimensions();
    let (a0, a1) = if horizontal { (band.min.y.max(0.0) as u32, (band.max.y as u32).min(h)) } else { (band.min.x.max(0.0) as u32, (band.max.x as u32).min(w)) };
    if a1 <= a0 { return; }
    let whole = if horizontal { h } else { w };
    if a0 == 0 && a1 >= whole { app.editor.set_status("That would remove the whole image."); return; }
    let size = 14u32;
    let overlap = if kind == EdgeType::NoEdge { 0 } else { size };
    let crop = |x: u32, y: u32, cw: u32, ch: u32| image::imageops::crop_imm(&img, x, y, cw, ch).to_image();
    let (first, second) = if horizontal { (crop(0, 0, w, a0), crop(0, a1, w, h - a1)) } else { (crop(0, 0, a0, h), crop(a1, 0, w - a1, h)) };
    let (first, second) = if kind == EdgeType::NoEdge { (first, second) } else {
        let f = if first.width() > 0 && first.height() > 0 { linkshot_render::effects::edge(&first, kind, size, false, horizontal, false, !horizontal) } else { first };
        let s = if second.width() > 0 && second.height() > 0 { linkshot_render::effects::edge(&second, kind, size, horizontal, false, !horizontal, false) } else { second };
        (f, s)
    };
    let (ow, oh) = if horizontal { (w, (first.height() + second.height()).saturating_sub(overlap).max(1)) } else { ((first.width() + second.width()).saturating_sub(overlap).max(1), h) };
    let mut out = image::RgbaImage::new(ow, oh);
    if first.width() > 0 && first.height() > 0 { image::imageops::overlay(&mut out, &first, 0, 0); }
    if second.width() > 0 && second.height() > 0 {
        let (sx, sy) = if horizontal { (0i64, first.height() as i64 - overlap as i64) } else { (first.width() as i64 - overlap as i64, 0i64) };
        image::imageops::overlay(&mut out, &second, sx.max(0), sy.max(0));
    }
    let Ok(png) = linkshot_render::encode_png(&out) else { return };
    let shift = (a1 - a0) as f64 + overlap as f64;
    let (ow, oh) = (ow as f64, oh as f64);
    app.editor.docs[i].edit("Cut Out", |doc| {
        let page = doc.page_mut();
        page.capture_objects.retain_mut(|o| {
            let c = o.bounds().center();
            let v = if horizontal { c.y } else { c.x };
            if v >= a0 as f64 && v < a1 as f64 { return false; }
            if v >= a1 as f64 { o.translate(if horizontal { Pt::new(0.0, -shift) } else { Pt::new(-shift, 0.0) }); }
            true
        });
        page.capture_canvas_width = ow;
        page.capture_canvas_height = oh;
        page.capture_background_location = Pt::ZERO;
        let n = page.capture_background_image.clone();
        doc.blobs.insert(n, std::sync::Arc::new(png));
    });
    app.editor.renderer.invalidate();
    app.editor.tools.clear_selection();
    app.editor.set_status(format!("Cut out {} {}.", a1 - a0, if horizontal { "rows" } else { "columns" }));
}

/// Clears the pending region to transparency (Selection tool → Delete).
pub fn region_delete(app: &mut App) {
    let Some(r) = app.editor.tools.region.take() else { return };
    whole_image_op(app, "Delete Selection", move |img| {
        let mut out = img.clone();
        for y in r.min.y.max(0.0) as u32..(r.max.y.min(img.height() as f64)) as u32 { for x in r.min.x.max(0.0) as u32..(r.max.x.min(img.width() as f64)) as u32 { out.put_pixel(x, y, image::Rgba([0, 0, 0, 0])); } }
        out
    });
}

pub fn region_copy(app: &mut App) {
    let Some(r) = app.editor.tools.region else { return };
    let Some(i) = app.editor.current else { return };
    let doc = app.editor.docs[i].doc.clone();
    if let Some(img) = app.editor.renderer.flatten(&doc, doc.page()) {
        let rr = linkshot_core::Rect { min: r.min.max(Pt::ZERO), max: r.max.min(Pt::new(img.width() as f64, img.height() as f64)) }.round_out();
        if !rr.is_empty() { let sub = image::imageops::crop_imm(&img, rr.min.x as u32, rr.min.y as u32, rr.width() as u32, rr.height() as u32).to_image(); let _ = linkshot_platform::clipboard::set_image(&sub); app.editor.set_status("Selection copied."); }
    }
}

/// Flood fill on the flattened background at `p` with the Fill tool's colour and tolerance.
pub fn fill_at(app: &mut App, p: Pt) {
    let q = app.editor.current_props();
    let color = q.background_color;
    let tol = q.tolerance.clamp(0.0, 100.0) as f64 / 100.0 * 255.0;
    let Some(i) = app.editor.current else { return };
    let doc = app.editor.docs[i].doc.clone();
    let Some(img) = app.editor.renderer.flatten(&doc, doc.page()) else { return };
    let (w, h) = img.dimensions();
    let (sx, sy) = (p.x as i64, p.y as i64);
    if sx < 0 || sy < 0 || sx >= w as i64 || sy >= h as i64 { return; }
    let mut out = img.clone();
    let target = *img.get_pixel(sx as u32, sy as u32);
    let near = |px: &image::Rgba<u8>| (0..4).all(|c| (px[c] as f64 - target[c] as f64).abs() <= tol);
    let mut visited = vec![false; (w * h) as usize];
    let mut stack = vec![(sx as u32, sy as u32)];
    let fill = image::Rgba([color.r, color.g, color.b, color.a]);
    while let Some((x, y)) = stack.pop() {
        let idx = (y * w + x) as usize;
        if visited[idx] { continue; }
        if !near(img.get_pixel(x, y)) { continue; }
        visited[idx] = true;
        out.put_pixel(x, y, fill);
        if x > 0 { stack.push((x - 1, y)); }
        if y > 0 { stack.push((x, y - 1)); }
        if x + 1 < w { stack.push((x + 1, y)); }
        if y + 1 < h { stack.push((x, y + 1)); }
    }
    // Keep vector objects: write the fill into the background only.
    let Ok(png) = linkshot_render::encode_png(&out) else { return };
    let has_objects = !doc.page().capture_objects.is_empty();
    app.editor.docs[i].edit("Fill", |doc| { let page = doc.page_mut(); if has_objects { page.capture_objects.clear(); } page.capture_background_location = Pt::ZERO; let n = page.capture_background_image.clone(); doc.blobs.insert(n, std::sync::Arc::new(png)); });
    app.editor.renderer.invalidate();
}

/// Pastes a clipboard image as a movable Image object (or a new document if none is open).
pub fn paste_image(app: &mut App, ctx: &egui::Context) {
    let Some(img) = linkshot_platform::clipboard::get_image() else { return };
    let Ok(png) = linkshot_render::encode_png(&img) else { return };
    if app.editor.current.is_none() {
        let doc = Document::from_png(img.width(), img.height(), png);
        app.editor.add_document(ctx, doc, &app.paths, &app.settings);
        return;
    }
    if app.settings.auto_flatten_paste {
        let paste = img.clone();
        whole_image_op(app, "Paste", move |base| { let mut out = base.clone(); image::imageops::overlay(&mut out, &paste, 0, 0); out });
        return;
    }
    use base64::Engine as _;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&png);
    let mut o = CaptureObject::new("Image");
    let (w, h) = (img.width() as f64, img.height() as f64);
    o.points_array = vec![Pt::new(20.0, 20.0), Pt::new(20.0 + w, 20.0 + h)];
    o.image_data = Some(b64);
    o.aspect_ratio = Some(w / h.max(1.0));
    let id = o.object_id.clone();
    if let Some(d) = app.editor.current_doc_mut() { d.edit("Paste", |doc| doc.page_mut().capture_objects.push(o)); }
    app.editor.tools.selection = vec![id];
    app.editor.tool = ToolMode::VectorSelection;
}
