//! Canvas view: renders the current document at the current zoom and routes input to the tools.

use crate::app::App;
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2};
use linkshot_core::Pt;
use linkshot_render::RenderOptions;

pub const ZOOM_LEVELS: &[f32] = &[0.05, 0.1, 0.15, 0.2, 0.25, 0.33, 0.5, 0.67, 0.75, 1.0, 1.25, 1.5, 2.0, 3.0, 4.0, 6.0, 8.0];

pub struct CanvasState {
    pub zoom: f32,
    pub fit: bool,
    pub fit_requested: bool,
    pub pan: Vec2,
    texture: Option<egui::TextureHandle>,
    tex_key: Option<(usize, u64, i32, usize)>,
    /// Device-pixel rectangle of the document that the texture holds (x, y, w, h).
    tile: Option<(i32, i32, u32, u32)>,
    pub view: Rect,
    pub origin: Pos2,
}

impl Default for CanvasState {
    fn default() -> Self {
        Self { zoom: 1.0, fit: true, fit_requested: true, pan: Vec2::ZERO, texture: None, tex_key: None, tile: None, view: Rect::NOTHING, origin: Pos2::ZERO }
    }
}

impl CanvasState {
    pub fn zoom_step(&mut self, dir: i32) {
        let idx = ZOOM_LEVELS.iter().position(|z| (*z - self.zoom).abs() < 0.001);
        let next = match (idx, dir) {
            (Some(i), d) if d > 0 => ZOOM_LEVELS.get(i + 1).copied(),
            (Some(i), _) => if i > 0 { ZOOM_LEVELS.get(i - 1).copied() } else { None },
            (None, d) if d > 0 => ZOOM_LEVELS.iter().copied().find(|z| *z > self.zoom),
            (None, _) => ZOOM_LEVELS.iter().rev().copied().find(|z| *z < self.zoom),
        };
        if let Some(z) = next { self.zoom = z; }
        self.fit = false;
    }
    pub fn doc_to_screen(&self, p: Pt) -> Pos2 {
        Pos2::new(self.origin.x + p.x as f32 * self.zoom, self.origin.y + p.y as f32 * self.zoom)
    }
    pub fn screen_to_doc(&self, p: Pos2) -> Pt {
        Pt::new(((p.x - self.origin.x) / self.zoom) as f64, ((p.y - self.origin.y) / self.zoom) as f64)
    }
    pub fn doc_rect_to_screen(&self, r: linkshot_core::Rect) -> Rect {
        Rect::from_min_max(self.doc_to_screen(r.min), self.doc_to_screen(r.max))
    }
    pub fn invalidate(&mut self) { self.tex_key = None; self.tile = None; }
}

pub fn canvas_ui(app: &mut App, ui: &mut egui::Ui, rect: Rect, ctx: &egui::Context) {
    let pal = app.pal;
    let ws = crate::theme::argb_to_color32(app.settings.editor_workspace_color);
    ui.painter().rect_filled(rect, 0.0, ws);
    app.editor.canvas.view = rect;
    let Some(i) = app.editor.current else {
        // Empty state: camera glyph, headline and a hint, centred on the workspace.
        let c = rect.center();
        let ring = Rect::from_center_size(c - Vec2::new(0.0, 46.0), Vec2::splat(72.0));
        ui.painter().circle_filled(ring.center(), 36.0, pal.control);
        ui.painter().circle_stroke(ring.center(), 36.0, Stroke::new(1.0_f32, pal.control_outline));
        crate::icons::camera_icon(ui.painter(), ring.shrink(20.0), pal.text);
        ui.painter().text(c + Vec2::new(0.0, 12.0), Align2::CENTER_CENTER, "Take a capture or open an image to get started", crate::theme::semibold(16.0), pal.text);
        ui.painter().text(c + Vec2::new(0.0, 38.0), Align2::CENTER_CENTER, "Press Print Screen, click Capture, or drop an image here", FontId::proportional(13.0), pal.input_secondary);
        return;
    };
    let (dw, dh) = { let p = app.editor.docs[i].doc.page(); (p.capture_canvas_width as f32, p.capture_canvas_height as f32) };
    // Fit / shrink-to-fit
    let avail = rect.shrink(40.0);
    let fit_zoom = (avail.width() / dw.max(1.0)).min(avail.height() / dh.max(1.0));
    if app.editor.canvas.fit_requested {
        app.editor.canvas.zoom = if app.settings.shrink_to_fit_new_documents { fit_zoom.min(1.0) } else { fit_zoom }.max(0.02);
        app.editor.canvas.fit = true;
        app.editor.canvas.fit_requested = false;
        app.editor.canvas.pan = Vec2::ZERO;
    } else if app.editor.canvas.fit && app.settings.shrink_to_fit_new_documents {
        app.editor.canvas.zoom = fit_zoom.min(1.0).max(0.02);
    }
    let zoom = app.editor.canvas.zoom;
    let size = Vec2::new(dw * zoom, dh * zoom);
    // Centre (or top-left when larger than view) + pan
    let mut origin = Pos2::new(if size.x < rect.width() { rect.center().x - size.x / 2.0 } else { rect.left() + 20.0 }, if size.y < rect.height() { rect.center().y - size.y / 2.0 } else { rect.top() + 20.0 });
    origin += app.editor.canvas.pan;
    app.editor.canvas.origin = origin;
    let img_rect = Rect::from_min_size(origin, size);

    // Interaction surface
    let resp = ui.interact(rect, ui.id().with("canvas"), Sense::click_and_drag());
    // Wheel: ctrl = zoom, otherwise pan
    let (scroll, ctrl) = ctx.input(|inp| (inp.raw_scroll_delta, inp.modifiers.ctrl));
    if resp.hovered() && scroll != Vec2::ZERO {
        if ctrl { if scroll.y > 0.0 { app.editor.canvas.zoom_step(1); } else if scroll.y < 0.0 { app.editor.canvas.zoom_step(-1); } }
        else { app.editor.canvas.pan += scroll; app.editor.canvas.fit = false; }
    }
    // Middle-drag pans
    if resp.dragged_by(egui::PointerButton::Middle) { app.editor.canvas.pan += resp.drag_delta(); }

    // Texture: rendered at zoom × pixels-per-point so every screen pixel is a real rendered pixel
    // (no GPU upscaling on HiDPI displays). Only a tile around the visible part is rendered, so a
    // 20,000 px scrolling capture at 100 % stays as snappy as a small one; the tile is re-rendered
    // when the view leaves it or the document changes.
    let ppp = ctx.pixels_per_point();
    let skip = app.editor.tools.text_edit.as_ref().map(|t| t.id.clone());
    let version = app.editor.docs[i].version;
    let render_scale = zoom * ppp;
    let key = (i, version, (render_scale * 1000.0) as i32, skip.as_ref().map(|s| s.len()).unwrap_or(0) + app.editor.tools.render_nonce);
    let dev_w = (dw * render_scale).ceil().max(1.0) as i32;
    let dev_h = (dh * render_scale).ceil().max(1.0) as i32;
    let vis = img_rect.intersect(rect);
    let visible = !vis.is_negative() && vis.width() > 0.0 && vis.height() > 0.0;
    if visible {
        // Visible part of the document in device pixels.
        let vx0 = ((vis.left() - img_rect.left()) * ppp).floor().max(0.0) as i32;
        let vy0 = ((vis.top() - img_rect.top()) * ppp).floor().max(0.0) as i32;
        let vx1 = ((vis.right() - img_rect.left()) * ppp).ceil().min(dev_w as f32) as i32;
        let vy1 = ((vis.bottom() - img_rect.top()) * ppp).ceil().min(dev_h as f32) as i32;
        let inside = |t: (i32, i32, u32, u32)| t.0 <= vx0 && t.1 <= vy0 && t.0 + t.2 as i32 >= vx1 && t.1 + t.3 as i32 >= vy1;
        let stale = app.editor.canvas.tex_key != Some(key) || !app.editor.canvas.tile.map(inside).unwrap_or(false);
        if stale {
            // Tile = visible rect plus half a view of margin on each side, clamped to the document.
            let (mx, my) = ((rect.width() * ppp * 0.5) as i32, (rect.height() * ppp * 0.5) as i32);
            let tx0 = (vx0 - mx).max(0);
            let ty0 = (vy0 - my).max(0);
            let tx1 = (vx1 + mx).min(dev_w);
            let ty1 = (vy1 + my).min(dev_h);
            let tile = (tx0, ty0, (tx1 - tx0).max(1) as u32, (ty1 - ty0).max(1) as u32);
            let whole = tile.0 == 0 && tile.1 == 0 && tile.2 as i32 == dev_w && tile.3 as i32 == dev_h;
            let doc = &app.editor.docs[i].doc;
            let opts = RenderOptions { scale: render_scale, checkerboard: true, skip_ids: skip.into_iter().collect(), clip: if whole { None } else { Some([tile.0 as f32, tile.1 as f32, tile.2 as f32, tile.3 as f32]) }, preview: true };
            if let Some(pm) = app.editor.renderer.render_page(doc, doc.page(), &opts) {
                let img = linkshot_render::pixmap_to_image(&pm);
                let ci = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
                let topts = egui::TextureOptions { magnification: egui::TextureFilter::Nearest, minification: egui::TextureFilter::Linear, wrap_mode: egui::TextureWrapMode::ClampToEdge, mipmap_mode: None };
                match app.editor.canvas.texture.as_mut() {
                    Some(t) => t.set(ci, topts),
                    None => app.editor.canvas.texture = Some(ctx.load_texture("canvas", ci, topts)),
                }
                app.editor.canvas.tex_key = Some(key);
                app.editor.canvas.tile = Some((tile.0, tile.1, pm.width(), pm.height()));
            }
        }
    }
    let clip = ui.painter().with_clip_rect(rect);
    clip.rect_filled(img_rect.translate(Vec2::new(0.0, 2.0)).expand(1.0), 0.0, Color32::from_black_alpha(80));
    if let (Some(t), Some(tile)) = (&app.editor.canvas.texture, app.editor.canvas.tile) {
        let tr = Rect::from_min_size(img_rect.min + Vec2::new(tile.0 as f32 / ppp, tile.1 as f32 / ppp), Vec2::new(tile.2 as f32 / ppp, tile.3 as f32 / ppp));
        clip.image(t.id(), tr, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
    }
    super::tools::handle_input(app, ui, &resp, rect, ctx);
    super::tools::draw_overlays(app, ui, rect, ctx);
    resp.context_menu(|ui| context_menu(app, ui, ctx));
}

/// Right-click menu on the image: copy/paste, selection, crop, save.
fn context_menu(app: &mut App, ui: &mut egui::Ui, ctx: &egui::Context) {
    ui.set_min_width(220.0);
    let has_region = app.editor.tools.region.is_some();
    let has_sel = !app.editor.tools.selection.is_empty();
    let item = |ui: &mut egui::Ui, label: &str, key: &str, enabled: bool| -> bool {
        let r = ui.add_enabled(enabled, egui::Button::new(label).shortcut_text(key));
        if r.clicked() { ui.close(); }
        r.clicked()
    };
    if has_region {
        if item(ui, "Copy Selection", "", true) { super::tools::region_copy(app); }
    }
    if item(ui, "Copy Image", "Ctrl+Shift+C", true) {
        if let Some(d) = app.editor.current_doc() { let doc = d.doc.clone(); match crate::share::copy_all(&mut app.editor.renderer, &doc) { Ok(()) => app.editor.set_status("Image copied to the clipboard."), Err(e) => app.editor.set_status(format!("Copy failed: {e}")) } }
    }
    if item(ui, "Paste", "Ctrl+V", true) { super::tools::paste_image(app, ctx); }
    ui.separator();
    if item(ui, "Select All", "Ctrl+A", true) { if let Some(d) = app.editor.current_doc() { app.editor.tools.selection = d.doc.page().capture_objects.iter().map(|o| o.object_id.clone()).collect(); } }
    if item(ui, "Deselect", "Ctrl+D", has_sel || has_region) { app.editor.tools.clear_selection(); }
    if item(ui, "Delete", "Del", has_sel || has_region) { if has_region { super::tools::region_delete(app); } else { super::tools::delete_selection(app); } }
    if has_sel {
        if item(ui, "Bring to Front", "", true) { super::tools::reorder(app, i32::MAX); }
        if item(ui, "Send to Back", "", true) { super::tools::reorder(app, i32::MIN); }
        if item(ui, "Flatten Selection", "", true) { super::tools::flatten_selection(app); }
    }
    if has_region { if item(ui, "Crop to Selection", "Enter", true) { super::tools::crop_apply(app); } }
    ui.separator();
    if item(ui, "Flatten All", "Ctrl+Shift+T", true) { super::flatten_current(app); }
    if item(ui, "Grab Text…", "", true) { super::grab_text_current(app); }
    if item(ui, "Save As…", "Ctrl+Shift+S", true) { let paths = app.paths.clone(); app.editor.save_as(&paths, &mut app.settings); app.mark_settings_dirty(); }
    ui.separator();
    if item(ui, "Zoom to Fit", "", true) { app.editor.canvas.fit_requested = true; }
    if item(ui, "Actual Size", "Ctrl+0", true) { app.editor.canvas.zoom = 1.0; app.editor.canvas.fit = false; }
}
