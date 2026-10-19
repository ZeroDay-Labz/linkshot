//! Reusable widgets: toggle switch, flat buttons, combo-menu, colour swatch button.

use crate::theme::Palette;
use egui::{Color32, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2};

/// iOS-style toggle switch with a label on the right.
pub fn toggle(ui: &mut Ui, p: &Palette, on: &mut bool, label: &str) -> Response {
    let size = Vec2::new(30.0, 16.0);
    let (rect, mut resp) = ui.allocate_exact_size(Vec2::new(size.x + 8.0 + ui.painter().layout_no_wrap(label.to_string(), egui::FontId::proportional(13.0), p.text).size().x, 22.0), Sense::click());
    if resp.clicked() { *on = !*on; resp.mark_changed(); }
    let t = ui.ctx().animate_bool(resp.id, *on);
    let sw = Rect::from_min_size(Pos2::new(rect.left(), rect.center().y - size.y / 2.0), size);
    let fill = Color32::from_rgb(
        (p.control_darker.r() as f32 + (p.product.r() as f32 - p.control_darker.r() as f32) * t) as u8,
        (p.control_darker.g() as f32 + (p.product.g() as f32 - p.control_darker.g() as f32) * t) as u8,
        (p.control_darker.b() as f32 + (p.product.b() as f32 - p.control_darker.b() as f32) * t) as u8,
    );
    ui.painter().rect_filled(sw, size.y / 2.0, fill);
    if t < 1.0 { ui.painter().rect_stroke(sw, size.y / 2.0, Stroke::new(1.0_f32, p.control_outline.gamma_multiply(1.0 - t)), egui::StrokeKind::Inside); }
    let knob_x = sw.left() + size.y / 2.0 + t * (size.x - size.y);
    ui.painter().circle_filled(Pos2::new(knob_x, sw.center().y + 1.0), size.y / 2.0 - 2.0, Color32::from_black_alpha(50));
    ui.painter().circle_filled(Pos2::new(knob_x, sw.center().y), size.y / 2.0 - 2.0, Color32::WHITE);
    ui.painter().text(Pos2::new(sw.right() + 8.0, rect.center().y), egui::Align2::LEFT_CENTER, label, egui::FontId::proportional(13.0), p.text);
    resp
}

/// Flat text button in the app style; `accent` makes it the blue primary action.
pub fn flat_button(ui: &mut Ui, p: &Palette, text: &str, accent: bool) -> Response {
    let fill = if accent { p.product } else { p.control };
    let fg = if accent { Color32::WHITE } else { p.text };
    let b = egui::Button::new(egui::RichText::new(text).color(fg)).fill(fill).stroke(Stroke::new(1.0_f32, if accent { p.product_dark } else { p.control_outline })).corner_radius(6.0).min_size(Vec2::new(0.0, 28.0));
    ui.add(b)
}

/// A small square icon button; `draw` paints the glyph.
pub fn icon_button(ui: &mut Ui, size: f32, tooltip: &str, hover_fill: Color32, draw: impl FnOnce(&egui::Painter, Rect)) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let t = ui.ctx().animate_bool(resp.id, resp.hovered());
    if t > 0.0 { ui.painter().rect_filled(rect, 6.0, hover_fill.gamma_multiply(t)); }
    draw(ui.painter(), rect);
    if tooltip.is_empty() { resp } else { resp.on_hover_text(tooltip) }
}

/// Colour swatch button (opens nothing by itself).
pub fn swatch(ui: &mut Ui, p: &Palette, color: Color32, size: Vec2) -> Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    if color.a() < 255 {
        // checkerboard for transparency
        let cell = 4.0;
        let mut y = rect.top();
        let mut odd = false;
        while y < rect.bottom() {
            let mut x = rect.left();
            let mut o = odd;
            while x < rect.right() {
                ui.painter().rect_filled(Rect::from_min_max(Pos2::new(x, y), Pos2::new((x + cell).min(rect.right()), (y + cell).min(rect.bottom()))), 0.0, if o { Color32::from_gray(200) } else { Color32::WHITE });
                o = !o; x += cell;
            }
            odd = !odd; y += cell;
        }
    }
    ui.painter().rect_filled(rect, 2.0, color);
    ui.painter().rect_stroke(rect, 2.0, Stroke::new(1.0_f32, if resp.hovered() { p.product } else { p.control_outline }), egui::StrokeKind::Inside);
    resp
}

/// A "combo menu" header: label text with a chevron, used for Selection:/Effects:/Share: dropdowns.
pub fn combo_header(ui: &mut Ui, p: &Palette, text: &str, width: f32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 30.0), Sense::click());
    let fill = if resp.hovered() { p.control_bright } else { p.control };
    ui.painter().rect_filled(rect, 6.0, fill);
    ui.painter().rect_stroke(rect, 6.0, Stroke::new(1.0_f32, if resp.hovered() { p.control_brightest } else { p.control_outline }), egui::StrokeKind::Inside);
    ui.painter().text(Pos2::new(rect.left() + 8.0, rect.center().y), egui::Align2::LEFT_CENTER, text, egui::FontId::proportional(13.0), p.text);
    crate::icons::chevron_down(ui.painter(), Rect::from_center_size(Pos2::new(rect.right() - 12.0, rect.center().y), Vec2::splat(16.0)), p.text);
    resp
}

/// Section header strip used in the side panel ("Quick Styles", "Tool Properties").
pub fn panel_header(ui: &mut Ui, p: &Palette, text: &str, right: impl FnOnce(&mut Ui)) {
    let h = 30.0;
    let rect = ui.available_rect_before_wrap();
    let rect = Rect::from_min_size(rect.min, Vec2::new(rect.width(), h));
    ui.painter().rect_filled(rect, 0.0, p.dark_panel);
    ui.painter().hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0_f32, p.separator));
    ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
        ui.horizontal_centered(|ui| {
            ui.add_space(10.0);
            ui.label(egui::RichText::new(text).family(egui::FontFamily::Name(crate::theme::FONT_SEMIBOLD.into())).color(p.text_bright).size(12.5));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { ui.add_space(6.0); right(ui); });
        });
    });
    ui.add_space(h);
}

pub fn separator_h(ui: &mut Ui, p: &Palette) {
    let rect = ui.available_rect_before_wrap();
    ui.painter().hline(rect.x_range(), rect.top(), Stroke::new(1.0_f32, p.separator));
    ui.add_space(1.0);
}

/// Anchored popup that opens when `anchor` is clicked and closes per `behavior` (egui 0.33 `Popup`).
pub fn popup(anchor: &Response, above: bool, behavior: egui::PopupCloseBehavior, add: impl FnOnce(&mut Ui)) {
    let mut p = egui::Popup::from_toggle_button_response(anchor).close_behavior(behavior);
    if above { p = p.align(egui::RectAlign::TOP_START); } else { p = p.align(egui::RectAlign::BOTTOM_START); }
    p.show(add);
}

/// Flat button with a trailing chevron ("Create ▾"); open a menu with `egui::Popup::menu(&resp)`.
pub fn dropdown_button(ui: &mut Ui, p: &Palette, text: &str, accent: bool, min_w: f32) -> Response {
    let galley = ui.painter().layout_no_wrap(text.to_string(), egui::FontId::proportional(13.0), p.text);
    let w = (galley.size().x + 30.0).max(min_w);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 28.0), Sense::click());
    let fill = if accent { if resp.hovered() { p.product_bright } else { p.product } } else if resp.hovered() { p.control_bright } else { p.control };
    let fg = if accent { Color32::WHITE } else { p.text };
    ui.painter().rect_filled(rect, 6.0, fill);
    ui.painter().rect_stroke(rect, 6.0, Stroke::new(1.0_f32, if accent { p.product_dark } else if resp.hovered() { p.control_brightest } else { p.control_outline }), egui::StrokeKind::Inside);
    ui.painter().text(Pos2::new(rect.left() + 8.0, rect.center().y), egui::Align2::LEFT_CENTER, text, egui::FontId::proportional(13.0), fg);
    crate::icons::chevron_down(ui.painter(), Rect::from_center_size(Pos2::new(rect.right() - 11.0, rect.center().y), Vec2::splat(14.0)), fg);
    resp
}

/// Small "transparent" swatch button (checkerboard with a red slash).
pub fn transparent_button(ui: &mut Ui, p: &Palette) -> Response {
    let r = swatch(ui, p, Color32::TRANSPARENT, Vec2::new(18.0, 18.0));
    ui.painter().line_segment([r.rect.left_bottom() + Vec2::new(3.0, -3.0), r.rect.right_top() + Vec2::new(-3.0, 3.0)], Stroke::new(1.5_f32, Color32::from_rgb(0xE9, 0x42, 0x37)));
    r.on_hover_text("Transparent")
}

/// A click-then-press hotkey field. `editing` holds the id of the field currently listening.
/// Returns `(changed, warning)`; bare letters/digits are refused unless `allow_bare`.
pub fn hotkey_editor(ui: &mut Ui, p: &Palette, hk: &mut linkshot_core::presets::HotKey, editing: &mut Option<String>, id: &str, allow_bare: bool) -> (bool, Option<String>) {
    use linkshot_core::presets::HotKey;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(170.0, 22.0), Sense::click());
    let is_editing = editing.as_deref() == Some(id);
    let fill = if is_editing { p.input_primary } else if resp.hovered() { p.control_bright } else { p.control };
    ui.painter().rect_filled(rect, 11.0, fill);
    ui.painter().rect_stroke(rect, 11.0, egui::Stroke::new(1.0_f32, if is_editing { p.product_bright } else { p.control_outline }), egui::StrokeKind::Inside);
    let text = if is_editing { "Press keys… (Del clears)".to_string() } else { hk.display() };
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, text, egui::FontId::proportional(12.0), if hk.is_none() && !is_editing { p.input_secondary } else { p.text });
    if resp.clicked() { *editing = if is_editing { None } else { Some(id.to_string()) }; }
    let mut changed = false;
    let mut warning = None;
    if is_editing {
        let (keys, mods) = ui.ctx().input(|i| (i.keys_down.clone(), i.modifiers));
        if ui.ctx().input(|i| i.key_pressed(egui::Key::Escape)) { *editing = None; return (false, None); }
        if ui.ctx().input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace)) { *hk = HotKey::none(); *editing = None; return (true, None); }
        for k in keys {
            if let Some(name) = crate::hotkeys::egui_key_to_wpf(k) {
                if matches!(k, egui::Key::Delete | egui::Key::Backspace | egui::Key::Escape) { continue; }
                let plain = name.len() == 1 || (name.len() == 2 && name.starts_with('D'));
                if plain && !allow_bare && !(mods.ctrl || mods.alt || mods.command) {
                    warning = Some("Add Ctrl, Alt or the Super key: a bare letter or digit would block typing everywhere.".to_string());
                    continue;
                }
                *hk = HotKey { key: name.into(), ctrl: mods.ctrl, alt: mods.alt, shift: mods.shift, win: mods.command && !mods.ctrl };
                *editing = None;
                changed = true;
                break;
            }
        }
        if resp.clicked_elsewhere() { *editing = None; }
    }
    (changed, warning)
}
