//! Visual language: colour tokens (dark first, light optional), the bundled Inter font and egui style.

use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, Visuals};
use linkshot_core::settings::UiTheme;
use std::sync::Arc;

#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub main_panel: Color32,
    pub dark_panel: Color32,
    pub control: Color32,
    pub control_bright: Color32,
    pub control_brightest: Color32,
    pub control_dark: Color32,
    pub control_darker: Color32,
    pub control_outline: Color32,
    pub separator: Color32,
    pub text: Color32,
    pub text_bright: Color32,
    pub input_primary: Color32,
    pub input_secondary: Color32,
    pub product: Color32,
    pub product_bright: Color32,
    pub product_dark: Color32,
    pub capture_red: Color32,
    pub capture_red_hover: Color32,
    pub capture_red_pressed: Color32,
    pub big_red_outer: Color32,
    pub big_red_inner: Color32,
    pub big_red_pressed: Color32,
    pub record: Color32,
    pub selection_orange: Color32,
    pub favorites_star: Color32,
    pub favorites_grey: Color32,
    pub output_selected: Color32,
    pub output_hover: Color32,
    pub green: Color32,
    pub is_dark: bool,
}

const fn c(hex: u32) -> Color32 {
    Color32::from_rgb(((hex >> 16) & 0xFF) as u8, ((hex >> 8) & 0xFF) as u8, (hex & 0xFF) as u8)
}

pub const LIGHT: Palette = Palette {
    main_panel: c(0xF4F7FA),
    dark_panel: c(0xE4EAF0),
    control: c(0xFFFFFF),
    control_bright: c(0xFFFFFF),
    control_brightest: c(0xFFFFFF),
    control_dark: c(0xDCE3EA),
    control_darker: c(0xC2CCD6),
    control_outline: c(0xD3DBE3),
    separator: c(0xDCE3EA),
    text: c(0x3C4A56),
    text_bright: c(0x1C2630),
    input_primary: c(0xFFFFFF),
    input_secondary: c(0x7F8E9B),
    product: c(0x2C7CB2),
    product_bright: c(0x3A8ECB),
    product_dark: c(0x256A9A),
    capture_red: c(0xE5484D),
    capture_red_hover: c(0xF0585D),
    capture_red_pressed: c(0xD23C41),
    big_red_outer: c(0xC03A3F),
    big_red_inner: c(0xE5484D),
    big_red_pressed: c(0xCF3E43),
    record: c(0xE5484D),
    selection_orange: c(0xFBB901),
    favorites_star: c(0xF9D44C),
    favorites_grey: c(0xD3DBE3),
    output_selected: c(0xFBE694),
    output_hover: c(0xE8EFF6),
    green: c(0x4E9A35),
    is_dark: false,
};

pub const DARK: Palette = Palette {
    main_panel: c(0x1F242B),
    dark_panel: c(0x171B20),
    control: c(0x2C333B),
    control_bright: c(0x39424C),
    control_brightest: c(0x46515C),
    control_dark: c(0x262C33),
    control_darker: c(0x13161A),
    control_outline: c(0x3A434D),
    separator: c(0x2E353D),
    text: c(0xC9D1D9),
    text_bright: c(0xF3F6F9),
    input_primary: c(0x14181C),
    input_secondary: c(0x7B8794),
    product: c(0x3B8AD9),
    product_bright: c(0x4D9AE8),
    product_dark: c(0x2F72B8),
    capture_red: c(0xE5484D),
    capture_red_hover: c(0xF0585D),
    capture_red_pressed: c(0xD23C41),
    big_red_outer: c(0xB93A3F),
    big_red_inner: c(0xE5484D),
    big_red_pressed: c(0xCF3E43),
    record: c(0xE5484D),
    selection_orange: c(0xFBB901),
    favorites_star: c(0xF9D44C),
    favorites_grey: c(0x4A5560),
    output_selected: c(0x3A4656),
    output_hover: c(0x2A3139),
    green: c(0x5BAF4F),
    is_dark: true,
};

pub fn palette(theme: UiTheme) -> Palette {
    match theme { UiTheme::Light => LIGHT, UiTheme::Dark => DARK }
}

pub fn argb_to_color32(a: linkshot_core::Argb) -> Color32 {
    Color32::from_rgba_unmultiplied(a.r, a.g, a.b, a.a)
}
pub fn color32_to_argb(c: Color32) -> linkshot_core::Argb {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    linkshot_core::Argb::new(a, r, g, b)
}

pub const FONT_MEDIUM: &str = "InterMedium";
pub const FONT_SEMIBOLD: &str = "InterSemiBold";

/// Inter Medium at `size` (labels, tab names).
pub fn medium(size: f32) -> FontId { FontId::new(size, FontFamily::Name(FONT_MEDIUM.into())) }
/// Inter SemiBold at `size` (headings, primary buttons).
pub fn semibold(size: f32) -> FontId { FontId::new(size, FontFamily::Name(FONT_SEMIBOLD.into())) }

/// Registers the bundled Inter font (SIL OFL, see assets/fonts/LICENSE-Inter.txt) once per context.
pub fn install_fonts(ctx: &egui::Context) {
    let id = egui::Id::new("linkshot-fonts-installed");
    if ctx.data(|d| d.get_temp::<bool>(id)).unwrap_or(false) { return; }
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert("Inter".into(), Arc::new(FontData::from_static(include_bytes!("../../../assets/fonts/Inter-Regular.ttf"))));
    fonts.font_data.insert(FONT_MEDIUM.into(), Arc::new(FontData::from_static(include_bytes!("../../../assets/fonts/Inter-Medium.ttf"))));
    fonts.font_data.insert(FONT_SEMIBOLD.into(), Arc::new(FontData::from_static(include_bytes!("../../../assets/fonts/Inter-SemiBold.ttf"))));
    let fallbacks: Vec<String> = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let mut prop = vec!["Inter".to_string()];
    prop.extend(fallbacks.iter().cloned());
    fonts.families.insert(FontFamily::Proportional, prop);
    for name in [FONT_MEDIUM, FONT_SEMIBOLD] {
        let mut fam = vec![name.to_string(), "Inter".to_string()];
        fam.extend(fallbacks.iter().cloned());
        fonts.families.insert(FontFamily::Name(name.into()), fam);
    }
    ctx.set_fonts(fonts);
    ctx.data_mut(|d| d.insert_temp(id, true));
}

/// Applies the linkshot look to egui: Inter, rounded 6 px controls, soft shadows, accent focus.
pub fn apply(ctx: &egui::Context, p: &Palette) {
    install_fonts(ctx);
    let mut visuals = if p.is_dark { Visuals::dark() } else { Visuals::light() };
    visuals.panel_fill = p.main_panel;
    visuals.window_fill = p.main_panel;
    visuals.window_stroke = Stroke::new(1.0_f32, p.control_outline);
    visuals.extreme_bg_color = p.input_primary;
    visuals.faint_bg_color = p.control_dark;
    visuals.code_bg_color = p.control_darker;
    visuals.override_text_color = Some(p.text);
    visuals.selection.bg_fill = p.product.gamma_multiply(if p.is_dark { 0.55 } else { 0.35 });
    visuals.selection.stroke = Stroke::new(1.0_f32, p.product_bright);
    visuals.hyperlink_color = p.product_bright;
    visuals.window_corner_radius = CornerRadius::same(10);
    visuals.menu_corner_radius = CornerRadius::same(8);
    visuals.popup_shadow = egui::epaint::Shadow { offset: [0, 6], blur: 18, spread: 0, color: Color32::from_black_alpha(if p.is_dark { 110 } else { 50 }) };
    visuals.window_shadow = egui::epaint::Shadow { offset: [0, 10], blur: 30, spread: 0, color: Color32::from_black_alpha(if p.is_dark { 140 } else { 60 }) };
    visuals.window_highlight_topmost = false;
    visuals.slider_trailing_fill = true;
    visuals.handle_shape = egui::style::HandleShape::Circle;
    let w = |w: &mut egui::style::WidgetVisuals, bg: Color32, stroke: Color32, fg: Color32, radius: u8| {
        w.bg_fill = bg;
        w.weak_bg_fill = bg;
        w.bg_stroke = Stroke::new(1.0_f32, stroke);
        w.corner_radius = CornerRadius::same(radius);
        w.fg_stroke = Stroke::new(1.0_f32, fg);
        w.expansion = 0.0;
    };
    w(&mut visuals.widgets.noninteractive, p.main_panel, p.separator, p.text, 6);
    w(&mut visuals.widgets.inactive, p.control, p.control_outline, p.text, 6);
    w(&mut visuals.widgets.hovered, p.control_bright, p.control_brightest, p.text_bright, 6);
    w(&mut visuals.widgets.active, p.control_dark, p.product, p.text_bright, 6);
    w(&mut visuals.widgets.open, p.control_dark, p.control_outline, p.text_bright, 6);
    visuals.widgets.hovered.expansion = 0.5;
    visuals.text_cursor.stroke = Stroke::new(2.0_f32, p.product_bright);
    let mut style = (*ctx.style()).clone();
    style.visuals = visuals;
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(10.0, 5.0);
    style.spacing.interact_size = egui::vec2(28.0, 24.0);
    style.spacing.menu_margin = egui::Margin::same(6);
    style.spacing.window_margin = egui::Margin::same(12);
    style.spacing.combo_width = 150.0;
    style.spacing.slider_width = 100.0;
    style.spacing.icon_width = 16.0;
    style.spacing.icon_width_inner = 9.0;
    style.spacing.tooltip_width = 360.0;
    style.interaction.tooltip_delay = 0.45;
    style.interaction.selectable_labels = false;
    style.text_styles.insert(egui::TextStyle::Body, FontId::proportional(13.5));
    style.text_styles.insert(egui::TextStyle::Button, medium(13.5));
    style.text_styles.insert(egui::TextStyle::Small, FontId::proportional(11.5));
    style.text_styles.insert(egui::TextStyle::Heading, semibold(17.0));
    style.text_styles.insert(egui::TextStyle::Monospace, FontId::monospace(12.5));
    ctx.set_style(style);
}
