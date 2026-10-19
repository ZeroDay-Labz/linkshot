//! The Capture window: title strip + menu, three tabs on the left,
//! settings in the middle, big red Capture button on the right, presets drawer at the bottom.

use crate::app::{Action, App};
use crate::icons;
use crate::widgets;
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2};
use linkshot_core::enums::*;
use linkshot_core::presets::{CaptureSettings, HotKey};

#[derive(Default)]
pub struct CaptureWindowState {
    pub presets_expanded: bool,
    pub editing_hotkey_for: Option<String>,
    pub renaming: Option<String>,
    pub time_delay_popup: bool,
    pub all_in_one_expanded: bool,
}

const TAB_W: f32 = 69.0;
const TITLE_H: f32 = 35.0;
const RIGHT_W: f32 = 120.0;
const PRESET_ROW: f32 = 30.0;

impl App {
    /// The Capture window is sized to its content: the Image/Video tabs are wider than All-in-One.
    pub fn capture_window_size(&self) -> egui::Vec2 {
        let w = match self.presets.selected_mode { CaptureMode::CaptureModeAllInOne => if self.capture.all_in_one_expanded { 700.0 } else { 560.0 }, _ => 700.0 };
        let presets_h = if self.capture.presets_expanded { (self.presets.presets.len() as f32 * PRESET_ROW).min(240.0) + 2.0 } else { 0.0 };
        egui::Vec2::new(w - 18.0, 282.0 + presets_h)
    }

    pub fn capture_window_ui(&mut self, ctx: &egui::Context) {
        let pal = self.pal;
        let mut actions: Vec<Action> = Vec::new();
        let wanted = self.capture_window_size();
        if (ctx.content_rect().size() - wanted).length() > 1.0 && !ctx.input(|i| i.viewport().minimized.unwrap_or(false)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(wanted));
        }
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(Color32::TRANSPARENT)).show(ctx, |ui| {
            let full = ui.max_rect();
            // The card fills the window: a transparent shadow margin would still be part of the window
            // surface and swallow clicks meant for whatever is behind it.
            let win = full.shrink(1.0);
            ui.painter().rect_filled(win, 10.0, pal.main_panel);
            ui.painter().rect_stroke(win, 10.0, Stroke::new(1.0_f32, pal.control_outline), egui::StrokeKind::Inside);

            // ---- title strip (accent) ----
            let title = Rect::from_min_size(win.min, Vec2::new(win.width(), TITLE_H));
            ui.painter().rect_filled(title, egui::CornerRadius { nw: 10, ne: 10, sw: 0, se: 0 }, pal.product);
            ui.painter().rect_filled(Rect::from_min_max(Pos2::new(title.left(), title.bottom() - 1.0), title.max), 0.0, pal.product_dark);
            let drag = ui.interact(title, ui.id().with("title-drag"), Sense::click_and_drag());
            if drag.drag_started() { ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag); }
            // app glyph
            icons::all_in_one_icon(ui.painter(), Rect::from_center_size(Pos2::new(title.left() + 22.0, title.center().y), Vec2::splat(16.0)), Color32::WHITE);
            // menu bar
            ui.scope_builder(egui::UiBuilder::new().max_rect(Rect::from_min_max(Pos2::new(title.left() + 40.0, title.top()), Pos2::new(title.right() - 80.0, title.bottom()))), |ui| {
                ui.horizontal_centered(|ui| {
                    ui.style_mut().visuals.override_text_color = Some(Color32::WHITE);
                    ui.style_mut().visuals.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
                    ui.style_mut().visuals.widgets.hovered.weak_bg_fill = pal.product_dark;
                    ui.style_mut().visuals.widgets.open.weak_bg_fill = pal.product_dark;
                    egui::MenuBar::new().ui(ui, |ui| {
                        ui.menu_button("File", |ui| {
                            ui.style_mut().visuals.override_text_color = Some(pal.text);
                            if ui.button("Import Presets...").clicked() { actions.push(Action::ImportPresets); ui.close(); }
                            if ui.button("Export All Presets...").clicked() { actions.push(Action::ExportPresets); ui.close(); }
                            ui.separator();
                            if ui.button("Close Capture Window").clicked() { actions.push(Action::HideCaptureWindow); ui.close(); }
                            ui.separator();
                            if ui.button("Capture Preferences...").clicked() { actions.push(Action::ShowPreferences); ui.close(); }
                            if ui.button("Exit linkshot").clicked() { actions.push(Action::Quit); ui.close(); }
                        });
                        ui.menu_button("Edit", |ui| {
                            ui.style_mut().visuals.override_text_color = Some(pal.text);
                            if ui.button("Reset Current Settings").clicked() { actions.push(Action::ResetCurrentSettings); ui.close(); }
                            if ui.button("Manage Program Destinations...").clicked() { actions.push(Action::ShowPreferences); ui.close(); }
                        });
                        ui.menu_button("View", |ui| {
                            ui.style_mut().visuals.override_text_color = Some(pal.text);
                            if ui.button("Share Preferences...").clicked() { actions.push(Action::ShowPreferences); ui.close(); }
                            if ui.button(if self.settings.capture_interface == linkshot_core::settings::CaptureInterface::CaptureWindowAndWidget { "Hide Capture Widget" } else { "Show Capture Widget" }).clicked() { actions.push(Action::ToggleWidget); ui.close(); }
                            ui.separator();
                            let dark = self.settings.ui_theme == linkshot_core::settings::UiTheme::Dark;
                            if ui.radio(dark, "Dark Theme").clicked() { actions.push(Action::SetTheme(linkshot_core::settings::UiTheme::Dark)); ui.close(); }
                            if ui.radio(!dark, "Light Theme").clicked() { actions.push(Action::SetTheme(linkshot_core::settings::UiTheme::Light)); ui.close(); }
                        });
                        ui.menu_button("Help", |ui| {
                            ui.style_mut().visuals.override_text_color = Some(pal.text);
                            if ui.button("Tutorials").clicked() { let _ = linkshot_platform::open_url("https://github.com/ZeroDay-Labz/linkshot#readme"); ui.close(); }
                            if ui.button("Support").clicked() { let _ = linkshot_platform::open_url("https://github.com/"); ui.close(); }
                            ui.separator();
                            if ui.button("About LinkShot").clicked() { actions.push(Action::ShowAbout); ui.close(); }
                        });
                    });
                });
            });
            // window buttons
            let btn = |ui: &mut egui::Ui, center: Pos2, draw: fn(&egui::Painter, Rect, Color32)| -> bool {
                let r = Rect::from_center_size(center, Vec2::splat(28.0));
                let resp = ui.interact(r, ui.id().with(center.x as i32), Sense::click());
                let t = ui.ctx().animate_bool(resp.id, resp.hovered());
                if t > 0.0 { ui.painter().rect_filled(r, 6.0, Color32::from_white_alpha((40.0 * t) as u8)); }
                draw(ui.painter(), r, Color32::WHITE);
                resp.clicked()
            };
            if btn(ui, Pos2::new(title.right() - 56.0, title.center().y), icons::minimize_icon) { ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true)); }
            if btn(ui, Pos2::new(title.right() - 20.0, title.center().y), icons::close_icon) { actions.push(Action::HideCaptureWindow); }

            // ---- main settings area ----
            let presets_h = PRESET_ROW + 1.0 + if self.capture.presets_expanded { (self.presets.presets.len() as f32 * PRESET_ROW).min(240.0) + 2.0 } else { 0.0 };
            let main = Rect::from_min_max(Pos2::new(win.left(), title.bottom()), Pos2::new(win.right(), win.bottom() - presets_h));
            let tabs = Rect::from_min_size(main.min, Vec2::new(TAB_W, main.height()));
            let right = Rect::from_min_max(Pos2::new(main.right() - RIGHT_W, main.top()), main.max);
            let content = Rect::from_min_max(Pos2::new(tabs.right(), main.top()), Pos2::new(right.left(), main.bottom()));
            // tab rail: a darker column; the selected tab lifts to the panel colour with an accent bar
            ui.painter().rect_filled(Rect::from_min_max(tabs.min, Pos2::new(tabs.right(), tabs.bottom())), egui::CornerRadius { nw: 0, ne: 0, sw: 0, se: 0 }, pal.dark_panel);
            let modes = [(CaptureMode::CaptureModeAllInOne, "All-in-One"), (CaptureMode::CaptureModeImage, "Image"), (CaptureMode::CaptureModeVideo, "Video")];
            for (i, (mode, label)) in modes.iter().enumerate() {
                let r = Rect::from_min_size(Pos2::new(tabs.left(), tabs.top() + 6.0 + i as f32 * 50.0), Vec2::new(TAB_W, 48.0));
                let selected = self.presets.selected_mode == *mode;
                let resp = ui.interact(r, ui.id().with(("tab", i)), Sense::click());
                let t = ui.ctx().animate_bool(resp.id.with("sel"), selected);
                let h = ui.ctx().animate_bool(resp.id.with("hov"), resp.hovered() && !selected);
                if t > 0.0 { ui.painter().rect_filled(r, egui::CornerRadius { nw: 0, ne: 8, sw: 0, se: 8 }, pal.main_panel.gamma_multiply(t)); }
                if h > 0.0 { ui.painter().rect_filled(r.shrink2(Vec2::new(4.0, 0.0)), 8.0, pal.control.gamma_multiply(h * 0.8)); }
                if t > 0.0 { ui.painter().rect_filled(Rect::from_min_size(Pos2::new(r.left(), r.top() + 8.0), Vec2::new(3.0, (r.height() - 16.0) * t)), 2.0, pal.product_bright); }
                let icon_c = if selected { pal.product_bright } else { pal.text };
                let ir = Rect::from_center_size(Pos2::new(r.center().x, r.top() + 17.0), Vec2::splat(20.0));
                match mode { CaptureMode::CaptureModeAllInOne => icons::all_in_one_icon(ui.painter(), ir, icon_c), CaptureMode::CaptureModeImage => icons::camera_icon(ui.painter(), ir, icon_c), CaptureMode::CaptureModeVideo => icons::video_icon(ui.painter(), ir, icon_c) }
                ui.painter().text(Pos2::new(r.center().x, r.bottom() - 11.0), Align2::CENTER_CENTER, *label, if selected { crate::theme::medium(11.5) } else { FontId::proportional(11.5) }, if selected { pal.text_bright } else { pal.text });
                if resp.on_hover_text(match mode { CaptureMode::CaptureModeAllInOne => "Capture an image, video or scrolling screenshot", CaptureMode::CaptureModeImage => "Image capture with effects and share options", CaptureMode::CaptureModeVideo => "Record your screen" }).clicked() { self.presets.selected_mode = *mode; self.mark_presets_dirty(); }
            }
            // content
            ui.scope_builder(egui::UiBuilder::new().max_rect(content.shrink2(Vec2::new(14.0, 12.0))), |ui| {
                match self.presets.selected_mode {
                    CaptureMode::CaptureModeAllInOne => self.all_in_one_tab(ui, &mut actions),
                    CaptureMode::CaptureModeImage => self.image_tab(ui, &mut actions),
                    CaptureMode::CaptureModeVideo => self.video_tab(ui, &mut actions),
                }
            });
            // right column: separator, reset, capture button, hotkey
            ui.painter().vline(right.left(), right.y_range(), Stroke::new(1.0_f32, pal.separator));
            let reset_r = Rect::from_center_size(Pos2::new(right.right() - 16.0, right.top() + 16.0), Vec2::splat(22.0));
            let reset = ui.interact(reset_r, ui.id().with("reset"), Sense::click());
            icons::reset_icon(ui.painter(), reset_r, if reset.hovered() { pal.product } else { pal.text });
            if reset.on_hover_text("Reset the capture settings to the defaults.").clicked() { actions.push(Action::ResetCurrentSettings); }
            let selected = self.presets.selected().clone();
            let cb_center = Pos2::new(right.center().x, right.top() + 18.0 + 40.0);
            if self.big_capture_button(ui, cb_center, &selected) { actions.push(Action::StartCapture(selected.clone())); }
            // hotkey field
            let hk_rect = Rect::from_center_size(Pos2::new(right.center().x, cb_center.y + 60.0), Vec2::new(100.0, 22.0));
            self.hotkey_field(ui, hk_rect, "global");

            // ---- presets bar ----
            let bar = Rect::from_min_size(Pos2::new(win.left(), main.bottom()), Vec2::new(win.width(), PRESET_ROW + 1.0));
            ui.painter().hline(bar.x_range(), bar.top(), Stroke::new(1.0_f32, pal.separator));
            ui.scope_builder(egui::UiBuilder::new().max_rect(Rect::from_min_max(Pos2::new(bar.left(), bar.top() + 1.0), bar.max)), |ui| {
                ui.horizontal_centered(|ui| {
                    ui.add_space(8.0);
                    let (r, resp) = ui.allocate_exact_size(Vec2::new(90.0, 26.0), Sense::click());
                    if resp.hovered() { ui.painter().rect_filled(r, 6.0, pal.control); }
                    let chev = Rect::from_center_size(Pos2::new(r.left() + 10.0, r.center().y), Vec2::splat(14.0));
                    if self.capture.presets_expanded { icons::chevron_down(ui.painter(), chev, pal.text); } else { icons::chevron_right(ui.painter(), chev, pal.text); }
                    ui.painter().text(Pos2::new(r.left() + 22.0, r.center().y), Align2::LEFT_CENTER, "Presets", FontId::proportional(13.0), pal.text);
                    if resp.on_hover_text("Show/hide the list of previously saved capture settings.").clicked() { self.capture.presets_expanded = !self.capture.presets_expanded; }
                    let add = widgets::dropdown_button(ui, &pal, "+ Add Preset", false, 0.0).on_hover_text("Save capture settings as a new preset.");
                    egui::Popup::menu(&add).show(|ui| {
                        if ui.button("New preset from current settings").clicked() { actions.push(Action::AddPresetFromCurrent); ui.close(); }
                        if ui.button("New preset").clicked() { actions.push(Action::AddPresetNew); ui.close(); }
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(8.0);
                        if widgets::flat_button(ui, &pal, "Open Editor", false).on_hover_text("Open the Editor for editing and sharing captures.").clicked() { actions.push(Action::OpenEditor); }
                    });
                });
            });
            if self.capture.presets_expanded {
                let list = Rect::from_min_max(Pos2::new(win.left(), bar.bottom()), win.max);
                ui.painter().rect_filled(list, egui::CornerRadius { nw: 0, ne: 0, sw: 10, se: 10 }, pal.dark_panel);
                ui.scope_builder(egui::UiBuilder::new().max_rect(list), |ui| {
                    egui::ScrollArea::vertical().max_height(240.0).show(ui, |ui| {
                        let n = self.presets.presets.len();
                        for i in 0..n { self.preset_row(ui, i, &mut actions); }
                    });
                });
            }
        });
        for a in actions { self.dispatch(ctx, a); }
    }

    fn big_capture_button(&mut self, ui: &mut egui::Ui, center: Pos2, settings: &CaptureSettings) -> bool {
        let pal = self.pal;
        let r = Rect::from_center_size(center, Vec2::splat(80.0));
        let resp = ui.interact(r, ui.id().with("capture-btn"), Sense::click());
        let toggled = settings.is_toggleable() && self.capture_toggled;
        let hov = ui.ctx().animate_bool(resp.id.with("hover"), resp.hovered());
        let down = resp.is_pointer_button_down_on();
        let radius = 40.0 + hov * 2.0 - if down { 1.5 } else { 0.0 };
        // soft glow + drop shadow
        ui.painter().circle_filled(center, radius + 10.0, pal.capture_red.gamma_multiply(0.10 + hov * 0.12));
        ui.painter().circle_filled(center + Vec2::new(0.0, 4.0), radius, Color32::from_black_alpha(80));
        ui.painter().circle_filled(center, radius, pal.big_red_outer);
        ui.painter().circle_filled(center, radius - 2.5, if down { pal.big_red_pressed } else if resp.hovered() { pal.capture_red_hover } else { pal.big_red_inner });
        // highlight sheen on the upper half
        ui.painter().circle_filled(center - Vec2::new(0.0, radius * 0.35), radius * 0.55, Color32::from_white_alpha(14));
        let label = if toggled { "Stop" } else if settings.is_toggleable() { "Start" } else { "Capture" };
        ui.painter().text(center, Align2::CENTER_CENTER, label, crate::theme::semibold(16.0), Color32::WHITE);
        if resp.has_focus() { ui.painter().circle_stroke(center, radius + 5.0, Stroke::new(1.5_f32, pal.product_bright)); }
        resp.on_hover_text("Start a capture with the selected settings.").clicked()
    }

    /// Hotkey capture field: click, then press a key combination.
    fn hotkey_field(&mut self, ui: &mut egui::Ui, rect: Rect, key: &str) {
        let pal = self.pal;
        let editing = self.capture.editing_hotkey_for.as_deref() == Some(key);
        let current: HotKey = if key == "global" { self.presets.global_hot_key.clone() } else { self.presets.presets.iter().find(|p| p.id == key).map(|p| p.hot_key.clone()).unwrap_or_else(HotKey::none) };
        let resp = ui.interact(rect, ui.id().with(("hk", key)), Sense::click());
        let fill = if editing { pal.input_primary } else if resp.hovered() { pal.control_bright } else { pal.control };
        ui.painter().rect_filled(rect, 11.0, fill);
        ui.painter().rect_stroke(rect, 11.0, Stroke::new(1.0_f32, if editing { pal.product_bright } else { pal.control_outline }), egui::StrokeKind::Inside);
        let text = if editing { "Press keys…".to_string() } else { current.display() };
        ui.painter().text(rect.center(), Align2::CENTER_CENTER, text, FontId::proportional(12.0), if current.is_none() && !editing { pal.input_secondary } else { pal.text });
        if resp.clicked() { self.capture.editing_hotkey_for = if editing { None } else { Some(key.to_string()) }; }
        if editing {
            let (keys, mods) = ui.ctx().input(|i| (i.keys_down.clone(), i.modifiers));
            if ui.ctx().input(|i| i.key_pressed(egui::Key::Escape)) { self.capture.editing_hotkey_for = None; return; }
            if ui.ctx().input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace)) {
                self.set_hotkey(key, HotKey::none());
                self.capture.editing_hotkey_for = None;
                return;
            }
            for k in keys {
                if let Some(name) = crate::hotkeys::egui_key_to_wpf(k) {
                    if matches!(k, egui::Key::Delete | egui::Key::Backspace | egui::Key::Escape) { continue; }
                    let hk = HotKey { key: name.into(), ctrl: mods.ctrl, alt: mods.alt, shift: mods.shift, win: mods.command && !mods.ctrl };
                    self.set_hotkey(key, hk);
                    self.capture.editing_hotkey_for = None;
                    break;
                }
            }
            if resp.clicked_elsewhere() { self.capture.editing_hotkey_for = None; }
        }
    }

    fn set_hotkey(&mut self, key: &str, hk: HotKey) {
        if key == "global" { self.presets.global_hot_key = hk; } else if let Some(p) = self.presets.presets.iter_mut().find(|p| p.id == key) { p.hot_key = hk; }
        self.mark_presets_dirty();
        self.register_hotkeys();
    }

    fn preset_row(&mut self, ui: &mut egui::Ui, i: usize, actions: &mut Vec<Action>) {
        let pal = self.pal;
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), PRESET_ROW), Sense::click());
        let hovered = resp.hovered() || ui.rect_contains_pointer(rect);
        if hovered { ui.painter().rect_filled(rect.shrink2(Vec2::new(6.0, 1.0)), 6.0, pal.control); }
        let p = self.presets.presets[i].clone();
        // mode glyph / red capture button on hover
        let glyph_r = Rect::from_center_size(Pos2::new(rect.left() + 34.0, rect.center().y), Vec2::splat(14.0));
        if hovered {
            let br = Rect::from_center_size(glyph_r.center(), Vec2::splat(26.0));
            let b = ui.interact(br, ui.id().with(("preset-cap", i)), Sense::click());
            ui.painter().circle_filled(br.center(), 13.0, if b.hovered() { pal.capture_red_hover } else { pal.capture_red });
            ui.painter().circle_stroke(br.center(), 13.0, Stroke::new(1.0_f32, Color32::from_rgb(0xF2, 0xF2, 0xF2)));
            match p.capture_mode { CaptureMode::CaptureModeAllInOne => icons::all_in_one_icon(ui.painter(), glyph_r, Color32::WHITE), CaptureMode::CaptureModeImage => icons::camera_icon(ui.painter(), glyph_r, Color32::WHITE), CaptureMode::CaptureModeVideo => icons::video_icon(ui.painter(), glyph_r, Color32::WHITE) }
            if b.on_hover_text("Start capture with preset.").clicked() { actions.push(Action::StartCapture(p.clone())); }
        } else {
            match p.capture_mode { CaptureMode::CaptureModeAllInOne => icons::all_in_one_icon(ui.painter(), glyph_r, pal.text), CaptureMode::CaptureModeImage => icons::camera_icon(ui.painter(), glyph_r, pal.text), CaptureMode::CaptureModeVideo => icons::video_icon(ui.painter(), glyph_r, pal.text) }
        }
        // name (inline rename)
        let name_r = Rect::from_min_max(Pos2::new(rect.left() + 56.0, rect.top()), Pos2::new(rect.right() - 150.0, rect.bottom()));
        if self.capture.renaming.as_deref() == Some(&p.id) {
            let mut name = p.display_name.clone();
            let te = ui.put(name_r.shrink2(Vec2::new(0.0, 4.0)), egui::TextEdit::singleline(&mut name));
            if te.changed() { if let Some(pp) = self.presets.presets.get_mut(i) { pp.display_name = name; pp.auto_generate_name = false; } self.mark_presets_dirty(); }
            if te.lost_focus() || ui.input(|i| i.key_pressed(egui::Key::Enter)) { self.capture.renaming = None; }
            te.request_focus();
        } else {
            ui.painter().text(Pos2::new(name_r.left(), rect.center().y), Align2::LEFT_CENTER, p.effective_name(), FontId::proportional(13.0), pal.text);
        }
        // hotkey + gear
        let hk_r = Rect::from_center_size(Pos2::new(rect.right() - 90.0, rect.center().y), Vec2::new(95.0, 22.0));
        let id = p.id.clone();
        self.hotkey_field(ui, hk_r, &id);
        let gear_r = Rect::from_center_size(Pos2::new(rect.right() - 22.0, rect.center().y), Vec2::splat(20.0));
        let gear = ui.interact(gear_r, ui.id().with(("gear", i)), Sense::click());
        icons::gear_icon(ui.painter(), gear_r, if gear.hovered() { pal.product } else { pal.text });
        crate::widgets::popup(&gear, false, egui::PopupCloseBehavior::CloseOnClick, |ui| {
            ui.set_min_width(110.0);
            if ui.button("Edit").clicked() { actions.push(Action::EditPreset(id.clone())); }
            if ui.button("Rename").clicked() { self.capture.renaming = Some(id.clone()); }
            if ui.button("Duplicate").clicked() { actions.push(Action::DuplicatePreset(id.clone())); }
            if ui.button("Delete").clicked() { actions.push(Action::DeletePreset(id.clone())); }
            if ui.button("Export").clicked() { actions.push(Action::ExportPreset(id.clone())); }
        });
        // tooltip summary
        if hovered && !gear.hovered() {
            resp.on_hover_ui(|ui| {
                ui.label(egui::RichText::new(match p.capture_mode { CaptureMode::CaptureModeAllInOne => "All-in-One", CaptureMode::CaptureModeImage => "Image", CaptureMode::CaptureModeVideo => "Video" }).strong());
                ui.label(format!("Selection: {}", p.selection_mode.label()));
                let effects: Vec<&str> = p.capture_effects.iter().filter(|e| e.effect_type != EffectType::EffectNone).map(|e| e.effect_type.label()).collect();
                ui.label(format!("Effects: {}", if effects.is_empty() { "None".to_string() } else { effects.join(", ") }));
                let shares: Vec<&str> = p.share_destination_ids.iter().map(|d| output_ids::title(d)).collect();
                ui.label(format!("Share: {}", shares.join(", ")));
                let mut opts = vec![];
                if p.copy_to_clipboard { opts.push("Copy to Clipboard"); }
                if p.preview_in_editor { opts.push("Preview in Editor"); }
                if p.capture_cursor { opts.push("Capture Cursor"); }
                if p.time_delay { opts.push("Time Delay"); }
                if !opts.is_empty() { ui.label(format!("Options: {}", opts.join(", "))); }
            });
        }
    }

    // ---------------- tabs ----------------

    fn selection_combo(&mut self, ui: &mut egui::Ui, width: f32, items: &[SelectionMode], advanced: &[SelectionMode]) {
        let pal = self.pal;
        let current = self.presets.selected().selection_mode;
        let resp = widgets::combo_header(ui, &pal, current.label(), width);
        let mut pick: Option<SelectionMode> = None;
        crate::widgets::popup(&resp, false, egui::PopupCloseBehavior::CloseOnClick, |ui| {
            ui.set_min_width(width);
            for m in items {
                if ui.selectable_label(*m == current, m.label()).clicked() { pick = Some(*m); }
            }
            if !advanced.is_empty() {
                ui.menu_button("Advanced", |ui| {
                    for m in advanced { if ui.selectable_label(*m == current, m.label()).clicked() { pick = Some(*m); ui.close(); } }
                });
            }
        });
        if let Some(m) = pick { self.presets.selected_mut().selection_mode = m; self.mark_presets_dirty(); }
    }

    fn share_combo(&mut self, ui: &mut egui::Ui, width: f32) {
        let pal = self.pal;
        let ids = self.presets.selected().share_destination_ids.clone();
        let label = ids.iter().map(|d| output_ids::title(d)).collect::<Vec<_>>().join(", ");
        let resp = widgets::combo_header(ui, &pal, if label.is_empty() { "None" } else { &label }, width);
        let mut pick: Option<String> = None;
        let enabled = self.settings.enabled_outputs.clone();
        crate::widgets::popup(&resp, false, egui::PopupCloseBehavior::CloseOnClick, |ui| {
            ui.set_min_width(width);
            if ui.selectable_label(ids == vec![output_ids::NONE.to_string()], "None").clicked() { pick = Some(output_ids::NONE.into()); }
            for id in &enabled { if ui.selectable_label(ids.contains(id), output_ids::title(id)).clicked() { pick = Some(id.clone()); } }
        });
        if let Some(id) = pick {
            let s = self.presets.selected_mut();
            if id == output_ids::NONE { s.share_destination_ids = vec![id]; } else { s.share_destination_ids = vec![id]; }
            if s.auto_generate_name { s.display_name = s.generate_name(); }
            self.mark_presets_dirty();
        }
    }

    fn effects_combo(&mut self, ui: &mut egui::Ui, width: f32) {
        let pal = self.pal;
        let current: Vec<EffectType> = self.presets.selected().capture_effects.iter().map(|e| e.effect_type).filter(|e| *e != EffectType::EffectNone).collect();
        let label = if current.is_empty() { "None".to_string() } else { current.iter().map(|e| e.label()).collect::<Vec<_>>().join(", ") };
        let resp = widgets::combo_header(ui, &pal, &label, width);
        let mut pick: Option<EffectType> = None;
        crate::widgets::popup(&resp, false, egui::PopupCloseBehavior::CloseOnClick, |ui| {
            ui.set_min_width(width);
            if ui.selectable_label(current.is_empty(), "None").clicked() { pick = Some(EffectType::EffectNone); }
            for e in [EffectType::EffectBorder, EffectType::EffectEdge, EffectType::EffectCaption, EffectType::EffectShadow, EffectType::EffectPerspectiveShear, EffectType::EffectPageCurl, EffectType::EffectFilter, EffectType::EffectColorAdjustment, EffectType::EffectColorReplacement, EffectType::EffectWatermark, EffectType::EffectScale, EffectType::EffectTrim] {
                if ui.selectable_label(current.contains(&e), e.label()).clicked() { pick = Some(e); }
            }
            if !current.is_empty() { ui.separator(); if ui.button("Remove this effect").clicked() { pick = Some(EffectType::EffectImageResolution); } }
        });
        if let Some(e) = pick {
            let s = self.presets.selected_mut();
            match e {
                EffectType::EffectNone => s.capture_effects.clear(),
                EffectType::EffectImageResolution => { s.capture_effects.pop(); }
                e => { if let Some(pos) = s.capture_effects.iter().position(|x| x.effect_type == e) { s.capture_effects.remove(pos); } else { s.capture_effects.push(linkshot_core::presets::EffectSettings { effect_type: e, params: Default::default() }); } }
            }
            if s.auto_generate_name { s.display_name = s.generate_name(); }
            self.mark_presets_dirty();
        }
    }

    fn option_toggles(&mut self, ui: &mut egui::Ui, with_time_delay_popup: bool) {
        let pal = self.pal;
        let mut changed = false;
        let s = self.presets.selected_mut();
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 8.0;
            changed |= widgets::toggle(ui, &pal, &mut s.preview_in_editor, "Preview in Editor").on_hover_text("Open the capture in the Editor to preview, edit, and share the image or video.").changed();
            changed |= widgets::toggle(ui, &pal, &mut s.copy_to_clipboard, "Copy to Clipboard").on_hover_text("Copy the capture to Clipboard for quick pasting into another program or location after capture.").changed();
            changed |= widgets::toggle(ui, &pal, &mut s.capture_cursor, "Capture Cursor").on_hover_text("Include the cursor in the capture").changed();
            ui.horizontal(|ui| {
                let label = if with_time_delay_popup { "Time Delay" } else { "5 Second Delay" };
                changed |= widgets::toggle(ui, &pal, &mut s.time_delay, label).on_hover_text("Delay the start of the capture to allow for additional setup time.").changed();
                if with_time_delay_popup {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let g = widgets::icon_button(ui, 18.0, "Specify Time Delay options.", pal.control_dark, |p, r| icons::gear_icon(p, r, pal.text));
                        crate::widgets::popup(&g, false, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
                            ui.set_min_width(220.0);
                            ui.label(egui::RichText::new("Time Delay").strong());
                            changed |= ui.radio_value(&mut s.timed_capture_type, TimedCaptureType::Delay, "Delay").changed();
                            if s.timed_capture_type == TimedCaptureType::Delay {
                                ui.horizontal(|ui| { ui.add_space(16.0); changed |= ui.add(egui::DragValue::new(&mut s.delay_duration).range(1..=600)).changed(); ui.label("seconds"); });
                            }
                            changed |= ui.radio_value(&mut s.timed_capture_type, TimedCaptureType::Interval, "Interval").changed();
                            if s.timed_capture_type == TimedCaptureType::Interval {
                                ui.horizontal(|ui| {
                                    ui.add_space(16.0); ui.label("Every");
                                    changed |= ui.add(egui::DragValue::new(&mut s.interval_duration).range(1..=3600)).changed();
                                    egui::ComboBox::from_id_salt("units").selected_text(s.interval_units.as_str()).show_ui(ui, |ui| { for u in IntervalUnits::ALL { changed |= ui.selectable_value(&mut s.interval_units, *u, u.as_str()).changed(); } });
                                });
                                ui.horizontal(|ui| { ui.add_space(16.0); changed |= ui.checkbox(&mut s.interval_discard_identical, "Discard identical captures").changed(); });
                            }
                            changed |= ui.radio_value(&mut s.timed_capture_type, TimedCaptureType::Schedule, "Schedule").changed();
                            if s.timed_capture_type == TimedCaptureType::Schedule {
                                ui.horizontal(|ui| { ui.add_space(16.0); ui.label("Date/time:"); changed |= ui.add(egui::TextEdit::singleline(&mut s.scheduled_capture_time).hint_text("YYYY-MM-DD HH:MM").desired_width(130.0)).changed(); });
                            }
                        });
                    });
                }
            });
        });
        if changed { self.mark_presets_dirty(); }
    }

    fn all_in_one_tab(&mut self, ui: &mut egui::Ui, _actions: &mut Vec<Action>) {
        let pal = self.pal;
        ui.horizontal_top(|ui| {
            if self.capture.all_in_one_expanded {
                ui.vertical(|ui| {
                    ui.set_width(150.0);
                    ui.label(egui::RichText::new("Selection:").color(pal.text));
                    self.selection_combo(ui, 150.0, SelectionMode::ALL_IN_ONE_MENU, &[]);
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new("Share:").color(pal.text));
                    self.share_combo(ui, 150.0);
                });
                ui.add_space(13.0);
            }
            let (r, resp) = ui.allocate_exact_size(Vec2::new(14.0, 100.0), Sense::click());
            if self.capture.all_in_one_expanded { icons::chevron_right(ui.painter(), Rect::from_center_size(Pos2::new(r.center().x, r.top() + 40.0), Vec2::splat(14.0)), pal.text); } else { icons::chevron_right(ui.painter(), Rect::from_center_size(Pos2::new(r.center().x, r.top() + 40.0), Vec2::splat(14.0)), pal.text); }
            if resp.on_hover_text("Show/hide additional options").clicked() { self.capture.all_in_one_expanded = !self.capture.all_in_one_expanded; }
            ui.add_space(6.0);
            self.option_toggles(ui, false);
        });
    }

    fn image_tab(&mut self, ui: &mut egui::Ui, _actions: &mut Vec<Action>) {
        let pal = self.pal;
        ui.horizontal_top(|ui| {
            egui::Grid::new("image-grid").num_columns(2).spacing([6.0, 10.0]).min_col_width(80.0).show(ui, |ui| {
                ui.label(egui::RichText::new("Selection:").color(pal.text));
                self.selection_combo(ui, 164.0, SelectionMode::IMAGE_MENU, SelectionMode::IMAGE_MENU_ADVANCED);
                ui.end_row();
                ui.label(egui::RichText::new("Effects:").color(pal.text));
                self.effects_combo(ui, 164.0);
                ui.end_row();
                ui.label(egui::RichText::new("Share:").color(pal.text));
                self.share_combo(ui, 164.0);
                ui.end_row();
            });
            ui.add_space(12.0);
            self.option_toggles(ui, true);
        });
    }

    fn video_tab(&mut self, ui: &mut egui::Ui, _actions: &mut Vec<Action>) {
        let pal = self.pal;
        ui.horizontal_top(|ui| {
            egui::Grid::new("video-grid").num_columns(2).spacing([6.0, 8.0]).min_col_width(80.0).show(ui, |ui| {
                ui.label(egui::RichText::new("Selection:").color(pal.text));
                self.selection_combo(ui, 164.0, SelectionMode::VIDEO_MENU, &[]);
                ui.end_row();
                ui.label(egui::RichText::new("Share:").color(pal.text));
                self.share_combo(ui, 164.0);
                ui.end_row();
                let mut changed = false;
                let s = self.presets.selected_mut();
                ui.label(egui::RichText::new("Webcam:").color(pal.text));
                let l1 = if s.enable_webcam { "On" } else { "Off" };
                changed |= widgets::toggle(ui, &pal, &mut s.enable_webcam, l1).changed();
                ui.end_row();
                ui.label(egui::RichText::new("Screen Draw:").color(pal.text));
                let l2 = if s.screen_draw_starts_on { "On" } else { "Off" };
                changed |= widgets::toggle(ui, &pal, &mut s.screen_draw_starts_on, l2).changed();
                ui.end_row();
                if changed { self.mark_presets_dirty(); }
            });
            ui.add_space(12.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                let mut changed = false;
                let s = self.presets.selected_mut();
                changed |= widgets::toggle(ui, &pal, &mut s.preview_in_editor, "Preview in Editor").changed();
                changed |= widgets::toggle(ui, &pal, &mut s.capture_cursor, "Capture Cursor").changed();
                changed |= widgets::toggle(ui, &pal, &mut s.enable_microphone, "Record Microphone").changed();
                changed |= widgets::toggle(ui, &pal, &mut s.enable_system_audio, "Record System Audio").changed();
                if changed { self.mark_presets_dirty(); }
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Save as:").color(pal.text));
                    let mut gif = self.settings.default_video_format.eq_ignore_ascii_case("gif");
                    let a = ui.radio_value(&mut gif, true, "GIF").changed();
                    let b = ui.radio_value(&mut gif, false, "MP4").changed();
                    if a || b { self.settings.default_video_format = if gif { "GIF".into() } else { "MP4".into() }; self.mark_settings_dirty(); }
                });
                ui.label(egui::RichText::new(format!("{} fps · Shift+F9 pause · Shift+F10 stop · audio not yet recorded", self.settings.video_quality.fps())).small().color(pal.input_secondary));
            });
        });
    }
}
