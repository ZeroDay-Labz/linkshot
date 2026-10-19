//! Side panel: Quick Styles gallery + Tool Properties, or the Effects panel.

use super::SidePanelMode;
use crate::app::App;
use crate::icons;
use crate::theme::{argb_to_color32, color32_to_argb};
use crate::widgets;
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2};
use linkshot_core::enums::*;
use linkshot_core::style::QuickStyle;
use linkshot_core::{Argb, Pt};

pub fn side_panel_ui(app: &mut App, ui: &mut egui::Ui, rect: Rect) {
    ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
        match app.editor.side_panel {
            SidePanelMode::Properties => properties_panel(app, ui),
            SidePanelMode::Effects => effects_panel(app, ui),
        }
    });
}

pub fn set_effect(app: &mut App, e: EffectType) { app.editor.tools.fx.current = Some(e); }

fn properties_panel(app: &mut App, ui: &mut egui::Ui) {
    let pal = app.pal;
    let tool = app.editor.tool;
    let gallery_tool = if tool == ToolMode::VectorSelection {
        // Move tool shows the properties of the selected object
        app.editor.current_doc().and_then(|d| app.editor.tools.selection.first().and_then(|id| d.doc.page().capture_objects.iter().find(|o| &o.object_id == id)).map(super::tools::tool_of)).unwrap_or(ToolMode::VectorSelection)
    } else { tool };
    // ---- Quick Styles ----
    let mut add_style = false;
    widgets::panel_header(ui, &pal, "Quick Styles", |ui| {
        if gallery_tool.supports_themes() && gallery_tool != ToolMode::Favorites {
            if widgets::icon_button(ui, 18.0, "Save the current properties as a Quick Style", pal.control_dark, |p, r| icons::plus_icon(p, r, pal.text)).clicked() { add_style = true; }
        }
    });
    if add_style {
        let mut q = app.editor.current_props();
        q.tool_mode = gallery_tool;
        q.is_default = false;
        let ti = app.editor.current_theme;
        app.editor.themes[ti].quick_styles.push(q);
        let t = app.editor.themes[ti].clone();
        let _ = t.write_file(&app.paths.themes_dir().join(format!("{}.snagtheme", t.id)));
    }
    if gallery_tool.supports_themes() {
        // Theme row (42 px)
        ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), 42.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.add_space(8.0);
            ui.label(egui::RichText::new("Theme:").color(pal.text));
            let names: Vec<String> = app.editor.themes.iter().map(|t| t.name.clone()).collect();
            let mut idx = app.editor.current_theme;
            egui::ComboBox::from_id_salt("theme-combo").width(150.0).selected_text(names.get(idx).cloned().unwrap_or_default()).show_ui(ui, |ui| {
                for (i, n) in names.iter().enumerate() { ui.selectable_value(&mut idx, i, n); }
                ui.separator();
                if ui.button("Import...").clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("LinkShot theme", &["snagtheme"]).pick_file() {
                        if let Ok(t) = linkshot_core::style::Theme::read_file(&p) { let _ = t.write_file(&app.paths.themes_dir().join(format!("{}.snagtheme", t.id))); app.editor.themes.push(t); idx = app.editor.themes.len() - 1; }
                    }
                }
                if ui.button("New Theme...").clicked() {
                    let mut t = linkshot_core::style::Theme { name: format!("Theme {}", app.editor.themes.len() + 1), theme_colors: app.editor.theme().theme_colors.clone(), ..Default::default() };
                    t.generate_quick_styles();
                    let _ = t.write_file(&app.paths.themes_dir().join(format!("{}.snagtheme", t.id)));
                    app.editor.themes.push(t); idx = app.editor.themes.len() - 1;
                }
                if ui.button("Export current...").clicked() {
                    let t = app.editor.theme().clone();
                    if let Some(p) = rfd::FileDialog::new().set_file_name(format!("{}.snagtheme", t.name)).add_filter("LinkShot theme", &["snagtheme"]).save_file() { let _ = t.write_file(&p); }
                }
            });
            if idx != app.editor.current_theme { app.editor.current_theme = idx; app.settings.current_theme_id = app.editor.themes[idx].id.clone(); app.mark_settings_dirty(); }
        });
        widgets::separator_h(ui, &pal);
    }
    // Gallery
    let gallery_h = app.settings.quick_styles_height.clamp(80.0, 400.0);
    let styles: Vec<QuickStyle> = if gallery_tool == ToolMode::Favorites {
        app.editor.themes.iter().flat_map(|t| t.quick_styles.iter().filter(|q| q.is_favorited).cloned()).collect()
    } else { app.editor.theme().styles_for(gallery_tool).cloned().collect() };
    let current = app.editor.current_props();
    let mut picked: Option<QuickStyle> = None;
    let mut toggle_fav: Option<usize> = None;
    egui::ScrollArea::vertical().max_height(gallery_h).id_salt("qs-scroll").show(ui, |ui| {
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            ui.add_space(8.0);
            ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
            for (i, q) in styles.iter().enumerate() {
                let (r, resp) = ui.allocate_exact_size(Vec2::splat(40.0), Sense::click());
                let selected = q.foreground_color == current.foreground_color && q.background_color == current.background_color && q.stroke_width == current.stroke_width && q.callout_shape == current.callout_shape && q.tool_shape == current.tool_shape && q.step_style == current.step_style && q.arrow_end == current.arrow_end && q.font_size == current.font_size && q.text_selection_color == current.text_selection_color;
                ui.painter().rect_filled(r, 6.0, if selected { pal.product.gamma_multiply(if pal.is_dark { 0.25 } else { 0.14 }) } else if resp.hovered() { pal.control } else { Color32::TRANSPARENT });
                if selected { ui.painter().rect_stroke(r, 6.0, Stroke::new(1.0_f32, pal.product_bright), egui::StrokeKind::Inside); }
                style_preview(ui.painter(), r.shrink(5.0), q, pal.text);
                // star
                if resp.hovered() || q.is_favorited {
                    let sr = Rect::from_center_size(r.right_top() + Vec2::new(-6.0, 6.0), Vec2::splat(10.0));
                    let star = ui.interact(sr, ui.id().with(("fav", i)), Sense::click());
                    icons::tool_icon(ui.painter(), sr, ToolMode::Favorites, if q.is_favorited { pal.favorites_star } else { pal.favorites_grey });
                    if star.clicked() { toggle_fav = Some(i); }
                }
                if resp.clicked() { picked = Some(q.clone()); }
                resp.context_menu(|ui| { if ui.button("Delete Quick Style").clicked() { toggle_fav = Some(usize::MAX - i); ui.close(); } });
            }
            if gallery_tool == ToolMode::Favorites && styles.is_empty() {
                ui.label(egui::RichText::new("Star Quick Styles from any tool to collect your favourites here.").color(pal.text.gamma_multiply(0.6)));
            }
        });
        ui.add_space(8.0);
    });
    if let Some(i) = toggle_fav {
        let ti = app.editor.current_theme;
        if i >= usize::MAX / 2 {
            let idx = usize::MAX - i;
            if let Some(q) = styles.get(idx) { let q = q.clone(); app.editor.themes[ti].quick_styles.retain(|x| *x != q); }
        } else if gallery_tool != ToolMode::Favorites {
            if let Some(q) = styles.get(i) { let q = q.clone(); if let Some(x) = app.editor.themes[ti].quick_styles.iter_mut().find(|x| **x == q) { x.is_favorited = !x.is_favorited; } }
        } else if let Some(q) = styles.get(i) { let q = q.clone(); for t in app.editor.themes.iter_mut() { if let Some(x) = t.quick_styles.iter_mut().find(|x| **x == q) { x.is_favorited = false; } } }
        let t = app.editor.themes[ti].clone();
        let _ = t.write_file(&app.paths.themes_dir().join(format!("{}.snagtheme", t.id)));
    }
    if let Some(q) = picked {
        let t = q.tool_mode;
        if gallery_tool == ToolMode::Favorites { app.editor.tool = t; }
        app.editor.props.insert(t, q.clone());
        apply_to_selection(app, &q);
    }
    // splitter
    let (sr, sresp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 6.0), Sense::drag());
    ui.painter().hline(sr.x_range(), sr.center().y, Stroke::new(1.0_f32, pal.separator));
    if sresp.dragged() { app.settings.quick_styles_height = (app.settings.quick_styles_height + sresp.drag_delta().y).clamp(80.0, 400.0); app.mark_settings_dirty(); }
    if sresp.hovered() { ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical); }
    // ---- Tool Properties ----
    widgets::panel_header(ui, &pal, "Tool Properties", |ui| {
        ui.label(egui::RichText::new(gallery_tool.label()).color(pal.text.gamma_multiply(0.7)).small());
    });
    egui::ScrollArea::vertical().id_salt("props-scroll").show(ui, |ui| {
        ui.add_space(8.0);
        egui::Frame::new().inner_margin(egui::Margin::symmetric(10, 0)).show(ui, |ui| {
            tool_properties(app, ui, gallery_tool);
        });
    });
}

/// Draws a small preview of a quick style (our own glyph artwork).
fn style_preview(p: &egui::Painter, r: Rect, q: &QuickStyle, text: Color32) {
    let fg = argb_to_color32(q.foreground_color);
    let bg = argb_to_color32(q.background_color);
    let c = r.center();
    match q.tool_mode {
        ToolMode::Arrow => {
            let a = Pos2::new(r.left(), r.bottom()); let b = Pos2::new(r.right(), r.top());
            let w = (q.stroke_width as f32 * 0.35).clamp(1.5, 6.0);
            p.line_segment([a, b], Stroke::new(w, fg));
            let d = (b - a).normalized(); let n = Vec2::new(-d.y, d.x); let h = 9.0 + w;
            let _ = p.add(egui::Shape::convex_polygon(vec![b, b - d * h + n * h * 0.5, b - d * h - n * h * 0.5], fg, Stroke::NONE));
        }
        ToolMode::Line => { p.line_segment([Pos2::new(r.left(), r.bottom()), Pos2::new(r.right(), r.top())], Stroke::new((q.stroke_width as f32 * 0.4).clamp(1.5, 6.0), fg)); }
        ToolMode::Shape => {
            let w = (q.stroke_width as f32 * 0.4).clamp(1.0, 5.0);
            match q.tool_shape { ShapeShape::Ellipse => { if bg.a() > 0 { p.circle_filled(c, r.width() / 2.0, bg); } p.circle_stroke(c, r.width() / 2.0 - w / 2.0, Stroke::new(w, fg)); } ShapeShape::RoundedRectangle => { if bg.a() > 0 { p.rect_filled(r, 5.0, bg); } p.rect_stroke(r, 5.0, Stroke::new(w, fg), egui::StrokeKind::Inside); } _ => { if bg.a() > 0 { p.rect_filled(r, 0.0, bg); } p.rect_stroke(r, 0.0, Stroke::new(w, fg), egui::StrokeKind::Inside); } }
        }
        ToolMode::Highlight => { p.rect_filled(Rect::from_center_size(c, Vec2::new(r.width(), r.height() * 0.45)), 0.0, fg.gamma_multiply(0.6)); }
        ToolMode::Pen => { let pts: Vec<Pos2> = (0..12).map(|i| { let t = i as f32 / 11.0; Pos2::new(r.left() + t * r.width(), c.y + (t * 6.28).sin() * r.height() * 0.3) }).collect(); let _ = p.add(egui::Shape::line(pts, Stroke::new((q.stroke_width as f32 * 0.4).clamp(1.5, 6.0), fg))); }
        ToolMode::Fill => { p.rect_filled(r, 3.0, bg); }
        ToolMode::Step => { p.circle_filled(c, r.width() / 2.0, bg); p.text(c, Align2::CENTER_CENTER, "1", FontId::proportional(14.0), fg); }
        ToolMode::Text => { p.text(c, Align2::CENTER_CENTER, "A", FontId::proportional((q.font_size as f32 * 0.9).clamp(12.0, 26.0)), argb_to_color32(q.text_selection_color)); }
        ToolMode::Callouts => {
            let body = Rect::from_min_max(r.left_top(), Pos2::new(r.right(), r.bottom() - 7.0));
            let fill = if bg.a() > 0 { bg } else { Color32::WHITE };
            p.rect_filled(body, 4.0, fill);
            if fg.a() > 0 { p.rect_stroke(body, 4.0, Stroke::new(1.5_f32, fg), egui::StrokeKind::Inside); }
            p.add(egui::Shape::convex_polygon(vec![Pos2::new(r.left() + 6.0, body.bottom() - 1.0), Pos2::new(r.left() + 14.0, body.bottom() - 1.0), Pos2::new(r.left() + 4.0, r.bottom())], fill, Stroke::NONE));
            p.text(body.center(), Align2::CENTER_CENTER, "A", FontId::proportional(11.0), argb_to_color32(q.text_selection_color));
        }
        ToolMode::Magnify => { p.circle_filled(c, r.width() / 2.0, bg); p.circle_stroke(c, r.width() / 2.0 - 1.0, Stroke::new(2.0_f32, fg)); p.text(c, Align2::CENTER_CENTER, "+", FontId::proportional(14.0), fg); }
        ToolMode::Blur => { for y in 0..3 { for x in 0..3 { p.rect_filled(Rect::from_min_size(r.left_top() + Vec2::new(x as f32 * r.width() / 3.0, y as f32 * r.height() / 3.0), Vec2::splat(r.width() / 3.0 - 1.0)), 0.0, Color32::from_gray(if (x + y) % 2 == 0 { 150 } else { 200 })); } } }
        _ => { p.rect_stroke(r, 2.0, Stroke::new(1.0_f32, text), egui::StrokeKind::Inside); }
    }
}

fn apply_to_selection(app: &mut App, q: &QuickStyle) {
    let sel = app.editor.tools.selection.clone();
    if sel.is_empty() { return; }
    let q = q.clone();
    if let Some(d) = app.editor.current_doc_mut() {
        d.edit("Change Style", |doc| { for o in doc.page_mut().capture_objects.iter_mut() { if sel.contains(&o.object_id) && super::tools::tool_of(o) == q.tool_mode { super::tools::apply_style(o, &q); } } });
    }
}

fn color_row(ui: &mut egui::Ui, label: &str, c: &mut Argb) -> bool {
    let mut col = argb_to_color32(*c);
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.color_edit_button_srgba(&mut col).changed() { *c = color32_to_argb(col); changed = true; }
            if widgets::transparent_button(ui, if ui.visuals().dark_mode { &crate::theme::DARK } else { &crate::theme::LIGHT }).clicked() { *c = Argb::TRANSPARENT; changed = true; }
        });
    });
    changed
}

fn slider_row(ui: &mut egui::Ui, label: &str, v: &mut f64, range: std::ops::RangeInclusive<f64>, suffix: &str) -> bool {
    let mut f = *v as f32;
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let r = ui.add(egui::DragValue::new(&mut f).range(*range.start() as f32..=*range.end() as f32).suffix(suffix).speed(0.5));
            changed |= r.changed();
            changed |= ui.add(egui::Slider::new(&mut f, *range.start() as f32..=*range.end() as f32).show_value(false)).changed();
        });
    });
    if changed { *v = f as f64; }
    changed
}

fn shadow_rows(ui: &mut egui::Ui, q: &mut QuickStyle) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label("Shadow");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // 3×3 direction picker
            let (r, _) = ui.allocate_exact_size(Vec2::splat(54.0), Sense::hover());
            for row in 0..3 { for col in 0..3 {
                let cell = Rect::from_min_size(r.min + Vec2::new(col as f32 * 18.0, row as f32 * 18.0), Vec2::splat(17.0));
                let (dx, dy) = ((col as f64 - 1.0) * 3.0, (row as f64 - 1.0) * 3.0);
                let on = q.drop_shadow_enabled && (q.shadow_direction_x.signum() == dx.signum() || dx == 0.0 && q.shadow_direction_x == 0.0) && (q.shadow_direction_y.signum() == dy.signum() || dy == 0.0 && q.shadow_direction_y == 0.0) && !(dx == 0.0 && dy == 0.0);
                let resp = ui.interact(cell, ui.id().with(("sh", row, col)), Sense::click());
                ui.painter().rect_filled(cell, 2.0, if on { ui.visuals().selection.bg_fill } else if resp.hovered() { ui.visuals().widgets.hovered.bg_fill } else { ui.visuals().widgets.inactive.bg_fill });
                if row == 1 && col == 1 { ui.painter().rect_filled(cell.shrink(5.0), 1.0, ui.visuals().text_color()); }
                if resp.clicked() {
                    if dx == 0.0 && dy == 0.0 || on { q.drop_shadow_enabled = false; } else { q.drop_shadow_enabled = true; q.shadow_direction_x = dx; q.shadow_direction_y = dy; }
                    changed = true;
                }
            } }
        });
    });
    if q.drop_shadow_enabled {
        ui.indent("shadow-adv", |ui| {
            changed |= color_row(ui, "Color", &mut q.shadow_color);
            changed |= slider_row(ui, "Blur", &mut q.shadow_blur, 0.0..=30.0, "");
            changed |= slider_row(ui, "Opacity", &mut q.shadow_opacity, 0.0..=100.0, "%");
            let mut dist = (q.shadow_direction_x.abs()).max(q.shadow_direction_y.abs());
            if slider_row(ui, "Distance", &mut dist, 0.0..=40.0, "") { q.shadow_direction_x = q.shadow_direction_x.signum() * dist; q.shadow_direction_y = q.shadow_direction_y.signum() * dist; changed = true; }
        });
    }
    changed
}

fn tool_properties(app: &mut App, ui: &mut egui::Ui, tool: ToolMode) {
    let pal = app.pal;
    let mut q = app.editor.props.get(&tool).cloned().unwrap_or_else(|| QuickStyle::for_tool(tool));
    let mut changed = false;
    ui.spacing_mut().item_spacing.y = 8.0;
    match tool {
        ToolMode::Arrow | ToolMode::Line => {
            changed |= color_row(ui, "Color", &mut q.foreground_color);
            changed |= shadow_rows(ui, &mut q);
            ui.label("End Style");
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                egui::ComboBox::from_id_salt("arrow-start").width(104.0).selected_text(q.arrow_start.as_str()).show_ui(ui, |ui| { for e in ArrowLineEndType::ALL { changed |= ui.selectable_value(&mut q.arrow_start, *e, e.as_str()).changed(); } });
                egui::ComboBox::from_id_salt("arrow-end").width(104.0).selected_text(q.arrow_end.as_str()).show_ui(ui, |ui| { for e in ArrowLineEndType::ALL { changed |= ui.selectable_value(&mut q.arrow_end, *e, e.as_str()).changed(); } });
            });
            ui.horizontal(|ui| { ui.label("Line Style"); ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { egui::ComboBox::from_id_salt("dash").selected_text(q.dash_type.as_str()).show_ui(ui, |ui| { for d in DashType::ALL { changed |= ui.selectable_value(&mut q.dash_type, *d, d.as_str()).changed(); } }); }); });
            changed |= slider_row(ui, "Width", &mut q.stroke_width, 1.0..=60.0, " px");
            changed |= slider_row(ui, "Opacity", &mut q.opacity, 0.0..=100.0, "%");
            changed |= slider_row(ui, "Start/End Size", &mut q.arrow_end_width, 0.5..=8.0, "");
            if changed { q.arrow_start_width = q.arrow_end_width; }
            let mut bez = q.arrow_type == LineType::Bezier;
            if ui.checkbox(&mut bez, "Bezier Curve").changed() { q.arrow_type = if bez { LineType::Bezier } else { LineType::Single }; q.line_type = q.arrow_type; changed = true; }
        }
        ToolMode::Shape => {
            changed |= color_row(ui, "Fill", &mut q.background_color);
            changed |= color_row(ui, "Outline", &mut q.foreground_color);
            ui.horizontal(|ui| { ui.label("Shape"); ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { for s in [ShapeShape::Rectangle, ShapeShape::RoundedRectangle, ShapeShape::Ellipse] { changed |= ui.selectable_value(&mut q.tool_shape, s, match s { ShapeShape::Rectangle => "▭", ShapeShape::RoundedRectangle => "▢", _ => "◯" }).changed(); } }); });
            changed |= shadow_rows(ui, &mut q);
            changed |= slider_row(ui, "Thickness", &mut q.stroke_width, 0.0..=40.0, " px");
            changed |= slider_row(ui, "Opacity", &mut q.opacity, 0.0..=100.0, "%");
            ui.horizontal(|ui| { ui.label("Line Style"); ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { egui::ComboBox::from_id_salt("dash2").selected_text(q.dash_type.as_str()).show_ui(ui, |ui| { for d in DashType::ALL { changed |= ui.selectable_value(&mut q.dash_type, *d, d.as_str()).changed(); } }); }); });
        }
        ToolMode::Highlight => {
            changed |= color_row(ui, "Color", &mut q.foreground_color);
            changed |= slider_row(ui, "Opacity", &mut q.opacity, 0.0..=100.0, "%");
        }
        ToolMode::Text | ToolMode::Callouts => {
            if tool == ToolMode::Callouts {
                changed |= color_row(ui, "Fill", &mut q.background_color);
                changed |= color_row(ui, "Outline", &mut q.foreground_color);
                ui.horizontal(|ui| { ui.label("Shape"); ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { egui::ComboBox::from_id_salt("callout-shape").selected_text(q.callout_shape.as_str().trim_start_matches("CT")).show_ui(ui, |ui| { for s in CalloutShape::ALL { changed |= ui.selectable_value(&mut q.callout_shape, *s, s.as_str().trim_start_matches("CT")).changed(); } }); }); });
                changed |= slider_row(ui, "Outline Size", &mut q.stroke_width, 0.0..=20.0, " px");
                ui.horizontal(|ui| { ui.label("Tail"); ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { for t in [CalloutTailStyle::Arrow, CalloutTailStyle::Triangle, CalloutTailStyle::None] { changed |= ui.selectable_value(&mut q.tail_style, t, t.as_str()).changed(); } }); });
                changed |= slider_row(ui, "Arrow Size", &mut q.tail_width, 2.0..=60.0, "");
            } else {
                changed |= color_row(ui, "Fill", &mut q.text_selection_color);
                changed |= color_row(ui, "Outline", &mut q.text_outline_color);
                changed |= slider_row(ui, "Line Width", &mut q.text_outline_width, 0.0..=10.0, "");
            }
            changed |= shadow_rows(ui, &mut q);
            ui.label(egui::RichText::new("Font").strong());
            ui.horizontal(|ui| { ui.label("Font"); ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { egui::ComboBox::from_id_salt("font").selected_text(&q.font_family).show_ui(ui, |ui| { for f in ["Arial", "Helvetica", "DejaVu Sans", "Liberation Sans", "Noto Sans", "Segoe UI", "Verdana", "Georgia", "Times New Roman", "Courier New", "Trebuchet MS"] { changed |= ui.selectable_value(&mut q.font_family, f.to_string(), f).changed(); } }); }); });
            if tool == ToolMode::Callouts { changed |= color_row(ui, "Font Color", &mut q.text_selection_color); }
            changed |= slider_row(ui, "Font Size", &mut q.font_size, 6.0..=120.0, " pt");
            ui.horizontal(|ui| {
                ui.label("Style");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    changed |= ui.toggle_value(&mut q.text_selection_underline, egui::RichText::new("U").underline()).changed();
                    changed |= ui.toggle_value(&mut q.text_selection_italic, egui::RichText::new("I").italics()).changed();
                    changed |= ui.toggle_value(&mut q.text_selection_bold, egui::RichText::new("B").strong()).changed();
                });
            });
            ui.horizontal(|ui| {
                ui.label("Alignment");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    for (v, l) in [(VAlign::Bottom, "⤓"), (VAlign::Center, "↕"), (VAlign::Top, "⤒")] { changed |= ui.selectable_value(&mut q.tool_vertical_align, v, l).changed(); }
                    ui.add_space(6.0);
                    for (h, l) in [(HAlign::Right, "⇥"), (HAlign::Center, "≡"), (HAlign::Left, "⇤")] { changed |= ui.selectable_value(&mut q.tool_horizontal_align, h, l).changed(); }
                });
            });
            changed |= slider_row(ui, "Padding", &mut q.tool_padding, 0.0..=60.0, "");
            changed |= slider_row(ui, "Opacity", &mut q.opacity, 0.0..=100.0, "%");
        }
        ToolMode::Step => {
            changed |= color_row(ui, "Fill", &mut q.background_color);
            changed |= color_row(ui, "Text Color", &mut q.foreground_color);
            ui.horizontal(|ui| { ui.label("Shape"); ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { for s in StepStyle::ALL { changed |= ui.selectable_value(&mut q.step_style, *s, s.as_str()).changed(); } }); });
            changed |= shadow_rows(ui, &mut q);
            changed |= slider_row(ui, "Size", &mut q.stroke_width, 12.0..=120.0, " px");
            changed |= slider_row(ui, "Font Size", &mut q.font_size, 6.0..=80.0, " pt");
            changed |= slider_row(ui, "Opacity", &mut q.opacity, 0.0..=100.0, "%");
            ui.horizontal(|ui| { ui.label("Type"); ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { egui::ComboBox::from_id_salt("seq").selected_text(match q.step_sequence_type { StepSequenceType::Number => "1… 2… 3…", StepSequenceType::UppercaseLetter => "A… B… C…", StepSequenceType::LowercaseLetter => "a… b… c…" }).show_ui(ui, |ui| { changed |= ui.selectable_value(&mut q.step_sequence_type, StepSequenceType::Number, "1… 2… 3…").changed(); changed |= ui.selectable_value(&mut q.step_sequence_type, StepSequenceType::UppercaseLetter, "A… B… C…").changed(); changed |= ui.selectable_value(&mut q.step_sequence_type, StepSequenceType::LowercaseLetter, "a… b… c…").changed(); }); }); });
            if ui.button("Restart Sequence").on_hover_text("Restart the lettering or numbering sequence").clicked() { if let Some(d) = app.editor.current_doc_mut() { d.edit("Restart Sequence", |doc| doc.page_mut().restart_steps()); } }
        }
        ToolMode::Pen => {
            changed |= color_row(ui, "Color", &mut q.foreground_color);
            ui.horizontal(|ui| { ui.label("Shape"); ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { changed |= ui.selectable_value(&mut q.pen_shape, PenShape::PenSquare, "■").changed(); changed |= ui.selectable_value(&mut q.pen_shape, PenShape::PenRound, "●").changed(); }); });
            changed |= shadow_rows(ui, &mut q);
            changed |= slider_row(ui, "Width", &mut q.stroke_width, 1.0..=80.0, " px");
            changed |= slider_row(ui, "Opacity", &mut q.opacity, 0.0..=100.0, "%");
            changed |= ui.checkbox(&mut q.create_as_vector, "Create as Vector").changed();
            changed |= ui.checkbox(&mut q.antialiasing, "Antialias").changed();
            changed |= ui.checkbox(&mut q.smoothing, "Smooth").changed();
        }
        ToolMode::Blur => {
            ui.horizontal(|ui| { ui.label("Type"); ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { changed |= ui.selectable_value(&mut q.blur_type, BlurType::Pixellate, "Pixelate").changed(); changed |= ui.selectable_value(&mut q.blur_type, BlurType::Gaussian, "Smooth").changed(); }); });
            changed |= slider_row(ui, "Intensity", &mut q.blur_intensity, 1.0..=100.0, "");
        }
        ToolMode::Magnify => {
            changed |= color_row(ui, "Outline", &mut q.foreground_color);
            ui.horizontal(|ui| { ui.label("Shape"); ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { changed |= ui.selectable_value(&mut q.tool_shape, ShapeShape::Rectangle, "▭").changed(); changed |= ui.selectable_value(&mut q.tool_shape, ShapeShape::Ellipse, "◯").changed(); }); });
            changed |= slider_row(ui, "Magnify", &mut q.magnify_scale, 110.0..=800.0, "%");
            changed |= slider_row(ui, "Line Width", &mut q.stroke_width, 0.0..=20.0, " px");
        }
        ToolMode::Fill => {
            changed |= color_row(ui, "Fill", &mut q.background_color);
            changed |= slider_row(ui, "Tolerance", &mut q.tolerance, 0.0..=100.0, "");
            ui.label(egui::RichText::new("Click the canvas to fill a region with the colour.").small().color(pal.input_secondary));
        }
        ToolMode::Crop => { ui.label("Drag the blue handles to select the area, then click Crop."); }
        ToolMode::Stamp => {
            changed |= slider_row(ui, "Size", &mut q.stroke_width, 16.0..=512.0, " px");
            let dir = app.paths.stamps_dir();
            app.editor.stamps.ensure_loaded(&dir, false);
            ui.label(egui::RichText::new("Click a stamp to add it, then click the canvas to place more.").small().color(pal.input_secondary));
            let n = app.editor.stamps.stamps.len();
            let mut picked: Option<usize> = None;
            let ctx = ui.ctx().clone();
            egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::Vec2::splat(6.0);
                    for i in 0..n {
                        let Some(tid) = app.editor.stamps.texture(&ctx, i) else { continue };
                        let selected = app.editor.tools.current_stamp == Some(i);
                        let name = app.editor.stamps.stamps[i].name.clone();
                        let (w, h) = (app.editor.stamps.stamps[i].width as f32, app.editor.stamps.stamps[i].height as f32);
                        let fit = if w >= h { egui::Vec2::new(44.0, 44.0 * h / w.max(1.0)) } else { egui::Vec2::new(44.0 * w / h.max(1.0), 44.0) };
                        let (rect, resp) = ui.allocate_exact_size(egui::Vec2::splat(52.0), egui::Sense::click());
                        ui.painter().rect_filled(rect, 6.0, if selected { pal.product } else if resp.hovered() { pal.control_bright } else { pal.control });
                        let img_rect = Rect::from_center_size(rect.center(), fit);
                        ui.painter().image(tid, img_rect, Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::new(1.0, 1.0)), Color32::WHITE);
                        if resp.on_hover_text(name).clicked() { picked = Some(i); }
                    }
                });
            });
            if let Some(i) = picked {
                app.editor.tools.current_stamp = Some(i);
                let at = app.editor.current_doc().map(|d| { let p = d.doc.page(); Pt::new(p.capture_canvas_width / 2.0, p.capture_canvas_height / 2.0) }).unwrap_or(Pt::new(100.0, 100.0));
                super::stamps::insert_stamp(app, i, at);
            }
            ui.horizontal(|ui| {
                if ui.button("Add stamps…").on_hover_text("Copy your own PNG/JPG files into the stamps folder").clicked() {
                    if let Some(files) = rfd::FileDialog::new().set_title("Add stamps").add_filter("Images", &["png", "jpg", "jpeg", "webp", "bmp"]).pick_files() {
                        let n = super::stamps::import_files(&dir, &files);
                        app.editor.stamps.ensure_loaded(&dir, true);
                        app.editor.set_status(format!("Added {n} stamp(s)."));
                    }
                }
                if ui.button("Open folder").clicked() { let _ = std::fs::create_dir_all(&dir); let _ = linkshot_platform::open_path(&dir); }
                if ui.button("Refresh").clicked() { app.editor.stamps.ensure_loaded(&dir, true); }
            });
        }
        ToolMode::Selection => {
            ui.label("Drag to select an area. Delete clears it, Copy copies it.");
        }
        ToolMode::VectorSelection => {
            ui.label(egui::RichText::new("Select an object to see its properties.").color(pal.input_secondary));
            ui.label(egui::RichText::new("Smart Move is planned for a later phase.").small().color(pal.input_secondary));
        }
        ToolMode::CutOut => {
            let (horizontal, edge) = cut_out_parts(q.cut_out_style);
            ui.label("Direction");
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                let mut h = horizontal;
                if ui.selectable_value(&mut h, true, "Horizontal").changed() || ui.selectable_value(&mut h, false, "Vertical").changed() { q.cut_out_style = cut_out_style(h, &edge); changed = true; }
            });
            ui.label("Edge");
            ui.horizontal_wrapped(|ui| {
                ui.add_space(8.0);
                for name in ["None", "Fade", "Tear", "Sharktooth", "Wave", "Saw"] {
                    if ui.selectable_label(edge == name, name).clicked() { q.cut_out_style = cut_out_style(horizontal, name); changed = true; }
                }
            });
            ui.label(egui::RichText::new(if horizontal { "Drag across the rows to remove; the parts above and below are joined." } else { "Drag across the columns to remove; the parts left and right are joined." }).small().color(pal.input_secondary));
            if app.editor.tools.region.is_some() { if crate::widgets::flat_button(ui, &pal, "Cut Out", true).clicked() { super::tools::cut_out_apply(app); } }
        }
        ToolMode::Eraser => {
            changed |= slider_row(ui, "Width", &mut q.stroke_width, 1.0..=200.0, " px");
            changed |= slider_row(ui, "Opacity", &mut q.opacity, 0.0..=100.0, "%");
            ui.label(egui::RichText::new("Drag over the image to erase to the canvas colour (transparent canvases erase to transparency). Annotations are not affected.").small().color(pal.input_secondary));
        }
        ToolMode::MagicWand | ToolMode::SimplifiedUI => {
            ui.label(egui::RichText::new(format!("{} is planned for a later phase.", tool.label())).color(pal.input_secondary));
        }
        _ => {}
    }
    if changed {
        app.editor.props.insert(tool, q.clone());
        apply_to_selection(app, &q);
    }
}

fn effects_panel(app: &mut App, ui: &mut egui::Ui) {
    let pal = app.pal;
    widgets::panel_header(ui, &pal, "Quick Effects", |_ui| {});
    ui.add_space(8.0);
    ui.horizontal_wrapped(|ui| {
        ui.add_space(8.0);
        for (name, e) in [("Border", EffectType::EffectBorder), ("Shadow", EffectType::EffectShadow), ("Torn edge", EffectType::EffectEdge), ("Grayscale", EffectType::EffectFilter), ("Trim", EffectType::EffectTrim)] {
            if ui.add(egui::Button::new(name).min_size(Vec2::new(70.0, 50.0))).clicked() {
                let fx = app.editor.tools.fx.clone();
                apply_effect(app, e, &fx);
            }
        }
    });
    ui.add_space(8.0);
    widgets::panel_header(ui, &pal, "Effects", |_ui| {});
    let effects = [EffectType::EffectBorder, EffectType::EffectEdge, EffectType::EffectCaption, EffectType::EffectPerspectiveShear, EffectType::EffectPageCurl, EffectType::EffectShadow, EffectType::EffectFilter, EffectType::EffectColorAdjustment, EffectType::EffectColorReplacement, EffectType::EffectSpotlightMagnify, EffectType::EffectWatermark, EffectType::EffectScale, EffectType::EffectTrim];
    egui::ScrollArea::vertical().id_salt("effects-list").show(ui, |ui| {
        for e in effects {
            let on = app.editor.tools.fx.current == Some(e);
            let r = ui.add(egui::Button::new(egui::RichText::new(e.label()).color(if on { pal.text_bright } else { pal.text })).fill(if on { pal.control_dark } else { Color32::TRANSPARENT }).min_size(Vec2::new(ui.available_width(), 32.0)));
            if r.clicked() { app.editor.tools.fx.current = if on { None } else { Some(e) }; }
            if on {
                egui::Frame::new().fill(pal.control).inner_margin(8.0).show(ui, |ui| {
                    let fx = &mut app.editor.tools.fx;
                    let implemented = match e {
                        EffectType::EffectBorder => { color_row(ui, "Color", &mut fx.border_color); let mut w = fx.border_width as f64; slider_row(ui, "Width", &mut w, 1.0..=100.0, " px"); fx.border_width = w as u32; true }
                        EffectType::EffectShadow => { let mut s = fx.shadow_size as f64; slider_row(ui, "Size", &mut s, 1.0..=60.0, " px"); fx.shadow_size = s as u32; color_row(ui, "Color", &mut fx.shadow_color); ui.horizontal(|ui| { ui.label("Position"); for (dx, dy, l) in [(-1, -1, "↖"), (0, -1, "↑"), (1, -1, "↗"), (-1, 0, "←"), (0, 0, "•"), (1, 0, "→"), (-1, 1, "↙"), (0, 1, "↓"), (1, 1, "↘")] { if ui.selectable_label(fx.shadow_dir == (dx, dy), l).clicked() { fx.shadow_dir = (dx, dy); } } }); true }
                        EffectType::EffectEdge => { ui.horizontal(|ui| { ui.label("Style"); egui::ComboBox::from_id_salt("edge").selected_text(fx.edge_type.as_str()).show_ui(ui, |ui| { for t in [EdgeType::Torn, EdgeType::Wave, EdgeType::Saw, EdgeType::Sharktooth, EdgeType::Fade] { ui.selectable_value(&mut fx.edge_type, t, t.as_str()); } }); }); let mut s = fx.edge_size as f64; slider_row(ui, "Size", &mut s, 2.0..=80.0, " px"); fx.edge_size = s as u32; ui.horizontal(|ui| { ui.label("Edges"); ui.checkbox(&mut fx.edges[0], "Top"); ui.checkbox(&mut fx.edges[1], "Bottom"); ui.checkbox(&mut fx.edges[2], "Left"); ui.checkbox(&mut fx.edges[3], "Right"); }); true }
                        EffectType::EffectFilter => { ui.horizontal(|ui| { ui.label("Filter"); egui::ComboBox::from_id_salt("filter").selected_text(fx.filter.as_str()).show_ui(ui, |ui| { for f in FilterType::ALL { ui.selectable_value(&mut fx.filter, *f, f.as_str()); } }); }); if matches!(fx.filter, FilterType::Soften | FilterType::Sharpen | FilterType::Opacity | FilterType::Monochrome) { let mut a = fx.filter_amount as f64; slider_row(ui, "Amount", &mut a, 0.0..=100.0, ""); fx.filter_amount = a as f32; } true }
                        EffectType::EffectColorAdjustment => { { let mut d = fx.brightness as f64; slider_row(ui, "Brightness", &mut d, -100.0..=100.0, ""); fx.brightness = d as f32; } { let mut d = fx.contrast as f64; slider_row(ui, "Contrast", &mut d, -100.0..=100.0, ""); fx.contrast = d as f32; } { let mut d = fx.hue as f64; slider_row(ui, "Hue", &mut d, -100.0..=100.0, ""); fx.hue = d as f32; } { let mut d = fx.saturation as f64; slider_row(ui, "Saturation", &mut d, -100.0..=100.0, ""); fx.saturation = d as f32; } let mut g = fx.gamma as f64; slider_row(ui, "Gamma", &mut g, 0.2..=3.0, ""); fx.gamma = g as f32; true }
                        EffectType::EffectColorReplacement => { color_row(ui, "Old color", &mut fx.replace_from); color_row(ui, "New color", &mut fx.replace_to); let mut t = fx.replace_tolerance as f64; slider_row(ui, "Accuracy", &mut t, 0.0..=128.0, ""); fx.replace_tolerance = t as u8; true }
                        EffectType::EffectScale => { let mut s = fx.scale_percent as f64; slider_row(ui, "Scale", &mut s, 5.0..=400.0, "%"); fx.scale_percent = s as f32; true }
                        EffectType::EffectTrim => { ui.label("Automatically trims solid-colour edges."); true }
                        _ => { ui.label(egui::RichText::new("This effect is planned for a later phase.").color(pal.input_secondary)); false }
                    };
                    if implemented {
                        ui.horizontal(|ui| {
                            if widgets::flat_button(ui, &pal, "Apply", true).clicked() { let fx = app.editor.tools.fx.clone(); apply_effect(app, e, &fx); }
                            if ui.button("Cancel").clicked() { app.editor.tools.fx.current = None; }
                        });
                    }
                });
            }
        }
    });
}

fn apply_effect(app: &mut App, e: EffectType, fx: &super::tools::EffectsState) {
    let fx = fx.clone();
    let name = e.label();
    match e {
        EffectType::EffectBorder => super::tools::whole_image_op(app, name, move |img| linkshot_render::effects::border(img, fx.border_color, fx.border_width)),
        EffectType::EffectShadow => super::tools::whole_image_op(app, name, move |img| linkshot_render::effects::shadow(img, fx.shadow_color, fx.shadow_size, fx.shadow_dir)),
        EffectType::EffectEdge => super::tools::whole_image_op(app, name, move |img| linkshot_render::effects::edge(img, fx.edge_type, fx.edge_size, fx.edges[0], fx.edges[1], fx.edges[2], fx.edges[3])),
        EffectType::EffectFilter => super::tools::whole_image_op(app, name, move |img| match fx.filter {
            FilterType::Grayscale => linkshot_render::effects::grayscale(img),
            FilterType::Invert => linkshot_render::effects::invert(img),
            FilterType::Monochrome => linkshot_render::effects::monochrome(img, (fx.filter_amount / 100.0 * 255.0) as u8),
            FilterType::Opacity => linkshot_render::effects::opacity(img, fx.filter_amount),
            FilterType::Soften => linkshot_render::effects::soften(img, fx.filter_amount / 10.0),
            FilterType::Sharpen => linkshot_render::effects::sharpen(img, fx.filter_amount / 20.0),
        }),
        EffectType::EffectColorAdjustment => super::tools::whole_image_op(app, name, move |img| linkshot_render::effects::color_adjust(img, fx.brightness, fx.contrast, fx.hue * 1.8, fx.saturation, fx.gamma)),
        EffectType::EffectColorReplacement => super::tools::whole_image_op(app, name, move |img| linkshot_render::effects::color_replace(img, fx.replace_from, fx.replace_to, fx.replace_tolerance, false)),
        EffectType::EffectScale => super::tools::whole_image_op(app, name, move |img| { let k = fx.scale_percent / 100.0; linkshot_render::effects::scale(img, (img.width() as f32 * k) as u32, (img.height() as f32 * k) as u32, true) }),
        EffectType::EffectTrim => super::tools::whole_image_op(app, name, |img| linkshot_render::effects::auto_trim(img, 8)),
        _ => {}
    }
    app.editor.tools.fx.current = None;
}

/// (`horizontal`, edge name) of a Cut Out style such as `HorzTearCutOut`.
pub fn cut_out_parts(style: CutOutType) -> (bool, String) {
    let s = style.as_str();
    let horizontal = !s.starts_with("Vert");
    let edge = s.trim_start_matches("Horz").trim_start_matches("Vert").trim_end_matches("CutOut");
    (horizontal, if edge.is_empty() || edge == "Max" { "None".to_string() } else { edge.to_string() })
}

pub fn cut_out_style(horizontal: bool, edge: &str) -> CutOutType {
    format!("{}{}CutOut", if horizontal { "Horz" } else { "Vert" }, edge).parse().unwrap_or_default()
}
