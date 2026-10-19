//! Recent captures tray.

use crate::app::App;
use crate::icons;
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2};

pub fn tray_ui(app: &mut App, ui: &mut egui::Ui, rect: Rect, ctx: &egui::Context) {
    let pal = app.pal;
    ui.painter().rect_filled(rect, 0.0, pal.dark_panel);
    let (tw, th) = app.settings.tray_thumbnail_size.pixels();
    let mut order: Vec<usize> = (0..app.editor.docs.len()).collect();
    order.sort_by_key(|i| (!app.editor.docs[*i].pinned, *i));
    let mut select: Option<usize> = None;
    let mut close: Option<usize> = None;
    let mut toggle_pin: Option<usize> = None;
    let mut delete: Option<usize> = None;
    let mut copy: Option<usize> = None;
    ui.scope_builder(egui::UiBuilder::new().max_rect(Rect::from_min_max(rect.min + Vec2::new(4.0, 6.0), rect.max - Vec2::new(4.0, 6.0))), |ui| {
        egui::ScrollArea::horizontal().id_salt("tray-scroll").show(ui, |ui| {
            ui.horizontal(|ui| {
                for i in order.clone() {
                    let is_current = app.editor.current == Some(i);
                    // ensure thumbnail texture
                    // Refresh the thumbnail once edits pause for a moment, not on every stroke.
                    let d = &app.editor.docs[i];
                    let settled = d.thumb.is_none() || d.changed_at.elapsed() > std::time::Duration::from_millis(400);
                    let need = d.thumb.is_none() || d.thumb_version != d.version;
                    if need && !settled { ctx.request_repaint_after(std::time::Duration::from_millis(420)); }
                    if need && settled {
                        let doc = app.editor.docs[i].doc.clone();
                        if let Some(t) = app.editor.renderer.thumbnail(&doc, doc.page(), 320, 180) {
                            let ci = egui::ColorImage::from_rgba_unmultiplied([t.width() as usize, t.height() as usize], t.as_raw());
                            let od = &mut app.editor.docs[i];
                            od.thumb = Some(ctx.load_texture(format!("thumb-{i}"), ci, egui::TextureOptions::LINEAR));
                            od.thumb_version = od.version;
                        }
                    }
                    let (r, resp) = ui.allocate_exact_size(Vec2::new(tw + 8.0, th + 8.0), Sense::click());
                    let lift = ui.ctx().animate_bool(resp.id.with("lift"), resp.hovered() || is_current);
                    let inner = r.shrink(4.0).translate(Vec2::new(0.0, -2.0 * lift));
                    ui.painter().rect_filled(inner.translate(Vec2::new(0.0, 3.0 + 2.0 * lift)), 6.0, Color32::from_black_alpha((40.0 + 50.0 * lift) as u8));
                    ui.painter().rect_filled(inner, 6.0, pal.control_darker);
                    if let Some(t) = &app.editor.docs[i].thumb {
                        let ts = t.size_vec2();
                        let k = (inner.width() / ts.x).min(inner.height() / ts.y);
                        let dr = Rect::from_center_size(inner.center(), ts * k);
                        ui.painter().image(t.id(), dr, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                    }
                    // Video badge: duration (or format) bottom-right with a play glyph.
                    if let Some(v) = &app.editor.docs[i].video {
                        let label = if v.duration_ms > 0 { linkshot_video::format_duration(v.duration_ms) } else { app.editor.docs[i].kind.as_str().replace("AnimatedGif", "GIF").replace("Video", "MP4") };
                        let galley = ui.painter().layout_no_wrap(label, FontId::proportional(10.5), Color32::WHITE);
                        let br = Rect::from_min_size(inner.right_bottom() - Vec2::new(galley.size().x + 26.0, 22.0), Vec2::new(galley.size().x + 22.0, 18.0));
                        ui.painter().rect_filled(br, 4.0, Color32::from_black_alpha(170));
                        let c = Pos2::new(br.left() + 9.0, br.center().y);
                        ui.painter().add(egui::Shape::convex_polygon(vec![c + Vec2::new(-3.0, -4.5), c + Vec2::new(4.5, 0.0), c + Vec2::new(-3.0, 4.5)], Color32::WHITE, Stroke::NONE));
                        ui.painter().galley(Pos2::new(br.left() + 17.0, br.center().y - galley.size().y / 2.0), galley, Color32::WHITE);
                    }
                    if is_current { ui.painter().rect_stroke(inner.expand(2.0), 7.0, Stroke::new(2.0_f32, pal.product_bright), egui::StrokeKind::Inside); }
                    else if resp.hovered() { ui.painter().rect_stroke(inner.expand(2.0), 7.0, Stroke::new(1.5_f32, pal.control_brightest), egui::StrokeKind::Inside); }
                    // unsaved marker (orange sunburst)
                    if app.editor.docs[i].is_dirty() { let c = inner.left_bottom() + Vec2::new(10.0, -10.0); ui.painter().circle_filled(c, 6.0, pal.selection_orange); for k in 0..8 { let a = k as f32 * std::f32::consts::FRAC_PI_4; ui.painter().line_segment([c + Vec2::angled(a) * 7.0, c + Vec2::angled(a) * 9.5], Stroke::new(1.5_f32, pal.selection_orange)); } }
                    // pin
                    let pinned = app.editor.docs[i].pinned;
                    if pinned || resp.hovered() {
                        let pr = Rect::from_center_size(inner.left_top() + Vec2::new(12.0, 12.0), Vec2::splat(18.0));
                        let pin = ui.interact(pr, ui.id().with(("pin", i)), Sense::click());
                        ui.painter().circle_filled(pr.center(), 9.0, Color32::from_black_alpha(90));
                        icons::pin_icon(ui.painter(), pr, if pinned { pal.selection_orange } else { Color32::WHITE });
                        if pin.on_hover_text(if pinned { "Unpin" } else { "Pin to tray" }).clicked() { toggle_pin = Some(i); }
                    }
                    // close
                    if resp.hovered() || is_current {
                        let cr = Rect::from_center_size(inner.right_top() + Vec2::new(-12.0, 12.0), Vec2::splat(18.0));
                        let cl = ui.interact(cr, ui.id().with(("close", i)), Sense::click());
                        ui.painter().circle_filled(cr.center(), 9.0, Color32::from_black_alpha(90));
                        icons::close_icon(ui.painter(), cr, Color32::WHITE);
                        if cl.on_hover_text("Remove from tray (keeps the file)").clicked() { close = Some(i); }
                    }
                    if resp.clicked() { select = Some(i); }
                    resp.context_menu(|ui| {
                        if ui.button("Copy Image").clicked() { copy = Some(i); ui.close(); }
                        ui.separator();
                        if ui.button(if pinned { "Unpin" } else { "Pin" }).clicked() { toggle_pin = Some(i); ui.close(); }
                        if ui.button("Close").clicked() { close = Some(i); ui.close(); }
                        if ui.button("Delete").clicked() { delete = Some(i); ui.close(); }
                        ui.separator();
                        if ui.button("Details").clicked() { let d = &app.editor.docs[i]; app.editor.status = Some((format!("{} — {}×{} — {}", d.doc.display_name(), d.doc.page().capture_canvas_width, d.doc.page().capture_canvas_height, d.doc.path.as_ref().map(|p| p.display().to_string()).unwrap_or_default()), std::time::Instant::now())); ui.close(); }
                        ui.menu_button("Thumbnail Size", |ui| {
                            for (sz, name) in [(linkshot_core::enums::TrayThumbnailSize::Small, "Small"), (linkshot_core::enums::TrayThumbnailSize::Medium, "Medium"), (linkshot_core::enums::TrayThumbnailSize::Large, "Large")] {
                                if ui.selectable_label(app.settings.tray_thumbnail_size == sz, name).clicked() { app.settings.tray_thumbnail_size = sz; app.mark_settings_dirty(); ui.close(); }
                            }
                        });
                    });
                    if let Some(p) = &app.editor.docs[i].doc.path { resp.on_hover_text(p.display().to_string()); }
                }
                ui.add_space(8.0);
                let (lr, lresp) = ui.allocate_exact_size(Vec2::new(110.0, 36.0), Sense::click());
                if lresp.hovered() { ui.painter().rect_filled(lr, 8.0, pal.control); }
                icons::library_icon(ui.painter(), Rect::from_center_size(Pos2::new(lr.left() + 20.0, lr.center().y), Vec2::splat(18.0)), pal.text_bright);
                ui.painter().text(Pos2::new(lr.left() + 38.0, lr.center().y), Align2::LEFT_CENTER, "Library", FontId::proportional(14.0), pal.text_bright);
                if lresp.on_hover_text("Switch to Library").clicked() { app.editor.library_view = !app.editor.library_view; if app.editor.library_view && !app.editor.library_scanned { let dir = app.settings.library_dir(&app.paths); let _ = app.editor.library.scan(&dir); app.editor.library_scanned = true; } }
            });
        });
    });
    if let Some(i) = copy.filter(|i| app.editor.docs[*i].is_video()) { super::video_view::copy_frame(app, i); }
    else if let Some(i) = copy { let doc = app.editor.docs[i].doc.clone(); match crate::share::copy_all(&mut app.editor.renderer, &doc) { Ok(()) => app.editor.set_status("Image copied to the clipboard."), Err(e) => app.editor.set_status(format!("Copy failed: {e}")) } }
    if let Some(i) = select { app.editor.current = Some(i); app.editor.canvas.fit_requested = true; app.editor.tools.clear_selection(); app.editor.library_view = false; }
    if let Some(i) = toggle_pin { app.editor.docs[i].pinned = !app.editor.docs[i].pinned; app.editor.tray_dirty = true; }
    if let Some(i) = close { remove_doc(app, i, false); }
    if let Some(i) = delete { remove_doc(app, i, true); }
}

pub fn remove_doc(app: &mut App, i: usize, delete_file: bool) {
    if i >= app.editor.docs.len() { return; }
    if !delete_file && app.editor.docs[i].is_dirty() && !app.editor.docs[i].is_video() {
        let name = app.editor.docs[i].doc.path.as_ref().map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()).unwrap_or_else(|| "this capture".into());
        let answer = rfd::MessageDialog::new().set_title("Unsaved changes").set_description(format!("Save changes to {name} before closing?")).set_buttons(rfd::MessageButtons::YesNoCancel).show();
        match answer {
            rfd::MessageDialogResult::Yes => {
                app.editor.current = Some(i);
                app.editor.save_current(&app.paths, &app.settings);
                if app.editor.docs.get(i).map(|d| d.is_dirty()).unwrap_or(false) { return; } // save failed or was cancelled
            }
            rfd::MessageDialogResult::No => {}
            _ => return,
        }
    }
    if delete_file {
        if let Some(p) = &app.editor.docs[i].doc.path {
            if app.settings.prompt_for_delete {
                let ok = rfd::MessageDialog::new().set_title("Delete").set_description(format!("Permanently delete {}?", p.display())).set_buttons(rfd::MessageButtons::YesNo).show();
                if ok != rfd::MessageDialogResult::Yes { return; }
            }
            let _ = std::fs::remove_file(p);
        }
    }
    app.editor.docs.remove(i);
    app.editor.current = if app.editor.docs.is_empty() { None } else { Some(i.min(app.editor.docs.len() - 1)) };
    app.editor.tools.clear_selection();
    app.editor.canvas.fit_requested = true;
    app.editor.tray_dirty = true;
}

pub fn close_current(app: &mut App) { if let Some(i) = app.editor.current { remove_doc(app, i, false); } }
pub fn delete_current(app: &mut App) { if let Some(i) = app.editor.current { remove_doc(app, i, true); } }
