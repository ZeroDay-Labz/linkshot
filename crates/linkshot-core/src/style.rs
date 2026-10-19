//! Tool properties, Quick Styles, Themes (`.snagtheme`) and Simplify palettes (`.snagpalette`).

use crate::color::Argb;
use crate::enums::*;
use crate::geometry::Pt;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// The property set of one tool — also one entry of a theme's `QuickStyles`.
/// Field names/spellings follow the `.snagtheme` JSON.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct QuickStyle {
    pub version: String,
    pub is_default: bool,
    pub tool_mode: ToolMode,
    pub opacity: f64,
    pub stroke_width: f64,
    pub foreground_color: Argb,
    pub background_color: Argb,
    pub shadow_color: Argb,
    pub dash_type: DashType,
    pub drop_shadow_enabled: bool,
    pub shadow_blur: f64,
    pub shadow_opacity: f64,
    #[serde(rename = "ShadowDirectionX")]
    pub shadow_direction_x: f64,
    #[serde(rename = "ShadowDirectionY")]
    pub shadow_direction_y: f64,
    pub line_type: LineType,
    pub arrow_type: LineType,
    pub line_start_cap: LineCap,
    pub line_end_cap: LineCap,
    pub arrow_start: ArrowLineEndType,
    pub arrow_end: ArrowLineEndType,
    pub arrow_start_width: f64,
    pub arrow_end_width: f64,
    pub arrow_width: f64,
    pub arrow_color: Argb,
    pub tool_shape: ShapeShape,
    pub callout_shape: CalloutShape,
    pub callout_tails: Vec<Pt>,
    pub callout_gradient: bool,
    pub tail_style: CalloutTailStyle,
    pub tail_head_style: ArrowLineEndType,
    pub tail_line_style: DashType,
    pub tail_width: f64,
    pub tail_color: Argb,
    pub step_style: StepStyle,
    pub step_sequence_type: StepSequenceType,
    pub step_value: String,
    pub step_width: f64,
    pub step_height: f64,
    pub font_family: String,
    pub font_size: f64,
    pub tool_padding: f64,
    pub tool_max_padding: f64,
    pub text_selection_bold: bool,
    pub text_selection_italic: bool,
    pub text_selection_underline: bool,
    pub text_selection_color: Argb,
    pub text_outline_color: Argb,
    pub text_outline_width: f64,
    pub pen_shape: PenShape,
    pub smoothing: bool,
    pub antialiasing: bool,
    pub blur_type: BlurType,
    pub blur_intensity: f64,
    pub tool_horizontal_align: HAlign,
    pub tool_vertical_align: VAlign,
    pub tolerance: f64,
    pub magnify_scale: f64,
    pub magnify_is_offset: bool,
    pub cut_out_style: CutOutType,
    pub create_as_vector: bool,
    pub stamp_file: String,
    pub stamp_folder: String,
    pub simplify_file: String,
    pub simplify_folder: String,
    pub simplify_shape: SimplifyShape,
    pub simplify_text_height: f64,
    pub simplify_line_spacing: f64,
    /// linkshot extension: starred in Favorites.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub is_favorited: bool,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for QuickStyle {
    fn default() -> Self {
        Self {
            version: "2.0".into(),
            is_default: false,
            tool_mode: ToolMode::Arrow,
            opacity: 100.0,
            stroke_width: 10.0,
            foreground_color: Argb::rgb(0xFC, 0x42, 0x42),
            background_color: Argb::TRANSPARENT,
            shadow_color: Argb::BLACK,
            dash_type: DashType::Solid,
            drop_shadow_enabled: false,
            shadow_blur: 2.0,
            shadow_opacity: 60.0,
            shadow_direction_x: 0.0,
            shadow_direction_y: 3.0,
            line_type: LineType::Single,
            arrow_type: LineType::Single,
            line_start_cap: LineCap::Flat,
            line_end_cap: LineCap::Flat,
            arrow_start: ArrowLineEndType::Round,
            arrow_end: ArrowLineEndType::RoundedArrow,
            arrow_start_width: 3.0,
            arrow_end_width: 3.0,
            arrow_width: 10.0,
            arrow_color: Argb::rgb(0xFC, 0x42, 0x42),
            tool_shape: ShapeShape::Rectangle,
            callout_shape: CalloutShape::CTRoundedRectWithArrow,
            callout_tails: vec![Pt::ZERO],
            callout_gradient: false,
            tail_style: CalloutTailStyle::Arrow,
            tail_head_style: ArrowLineEndType::BarbedArrow,
            tail_line_style: DashType::Solid,
            tail_width: 10.0,
            tail_color: Argb::rgb(0xFC, 0x42, 0x42),
            step_style: StepStyle::Circle,
            step_sequence_type: StepSequenceType::Number,
            step_value: "1".into(),
            step_width: 0.0,
            step_height: 0.0,
            font_family: "Arial".into(),
            font_size: 14.0,
            tool_padding: 0.0,
            tool_max_padding: 100.0,
            text_selection_bold: false,
            text_selection_italic: false,
            text_selection_underline: false,
            text_selection_color: Argb::WHITE,
            text_outline_color: Argb::WHITE,
            text_outline_width: 0.0,
            pen_shape: PenShape::PenRound,
            smoothing: false,
            antialiasing: true,
            blur_type: BlurType::Gaussian,
            blur_intensity: 0.0,
            tool_horizontal_align: HAlign::Center,
            tool_vertical_align: VAlign::Center,
            tolerance: 10.0,
            magnify_scale: 200.0,
            magnify_is_offset: false,
            cut_out_style: CutOutType::HorzNoneCutOut,
            create_as_vector: true,
            stamp_file: String::new(),
            stamp_folder: String::new(),
            simplify_file: String::new(),
            simplify_folder: String::new(),
            simplify_shape: SimplifyShape::Rectangle,
            simplify_text_height: 16.0,
            simplify_line_spacing: 8.0,
            is_favorited: false,
            extra: Map::new(),
        }
    }
}

impl QuickStyle {
    pub fn for_tool(tool: ToolMode) -> Self {
        let mut q = Self { tool_mode: tool, ..Default::default() };
        match tool {
            ToolMode::Shape => { q.stroke_width = 4.0; q.foreground_color = Argb::rgb(0xFC, 0x42, 0x42); q.background_color = Argb::TRANSPARENT; }
            ToolMode::Highlight => { q.foreground_color = Argb::rgb(0xFF, 0xF0, 0x00); q.background_color = Argb::rgb(0xFF, 0xF0, 0x00); q.opacity = 50.0; }
            ToolMode::Text => { q.foreground_color = Argb::TRANSPARENT; q.background_color = Argb::TRANSPARENT; q.text_selection_color = Argb::BLACK; q.font_size = 24.0; }
            ToolMode::Callouts => { q.stroke_width = 4.0; q.foreground_color = Argb::TRANSPARENT; q.background_color = Argb::rgb(0xFC, 0x42, 0x42); q.drop_shadow_enabled = true; }
            ToolMode::Step => { q.foreground_color = Argb::WHITE; q.background_color = Argb::rgb(0xFC, 0x42, 0x42); q.font_size = 16.0; q.stroke_width = 32.0; }
            ToolMode::Stamp => { q.stroke_width = 96.0; q.opacity = 100.0; }
            ToolMode::Pen => { q.stroke_width = 4.0; }
            ToolMode::Line => { q.stroke_width = 4.0; q.arrow_start = ArrowLineEndType::Flat; q.arrow_end = ArrowLineEndType::Flat; }
            ToolMode::Blur => { q.blur_intensity = 50.0; }
            ToolMode::Magnify => { q.tool_shape = ShapeShape::Ellipse; q.foreground_color = Argb::rgb(0xFC, 0x42, 0x42); q.background_color = Argb::WHITE; q.stroke_width = 4.0; }
            ToolMode::Fill => { q.background_color = Argb::rgb(0xFC, 0x42, 0x42); }
            ToolMode::Eraser => { q.stroke_width = 20.0; }
            _ => {}
        }
        q
    }
}

/// A `.snagtheme`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct Theme {
    pub name: String,
    pub id: String,
    pub version: String,
    pub quick_styles: Vec<QuickStyle>,
    pub theme_colors: Vec<Argb>,
    pub drop_shadow: bool,
    pub font_family: String,
    pub font_emphasis: u32,
    pub is_checked: bool,
    pub is_default: bool,
    pub exported_from_platform: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}
impl Default for Theme {
    fn default() -> Self {
        Self {
            name: "Untitled".into(),
            id: uuid::Uuid::new_v4().to_string(),
            version: "4.0".into(),
            quick_styles: vec![],
            theme_colors: vec![],
            drop_shadow: false,
            font_family: "Arial".into(),
            font_emphasis: 1,
            is_checked: false,
            is_default: false,
            exported_from_platform: "win".into(),
            extra: Map::new(),
        }
    }
}

impl Theme {
    /// Generates a complete quick-style set from the theme colours, the way
    /// "New Theme" does (callouts × shapes × colours, pens, highlights, arrows, shapes, lines, fills, steps, text, magnify).
    pub fn generate_quick_styles(&mut self) {
        let colors: Vec<Argb> = if self.theme_colors.is_empty() {
            vec![Argb::rgb(0xFC, 0x42, 0x42), Argb::rgb(0x63, 0x22, 0x89), Argb::rgb(0x06, 0xAF, 0x8F), Argb::rgb(0xF7, 0xAC, 0x08)]
        } else {
            self.theme_colors.clone()
        };
        let neutrals = [Argb::WHITE, Argb::rgb(0xA4, 0xA4, 0xA4), Argb::rgb(0x44, 0x44, 0x44), Argb::BLACK];
        let shadow = self.drop_shadow;
        let font = self.font_family.clone();
        let mut out = Vec::new();
        let base = |tool: ToolMode| {
            let mut q = QuickStyle::for_tool(tool);
            q.drop_shadow_enabled = shadow;
            q.font_family = font.clone();
            q
        };
        // Callouts: rounded rect with arrow (fill colour, white text), speech bubble, arrow-text, balloon (outline), speech bubble 1.
        for shape in [CalloutShape::CTRoundedRectWithArrow, CalloutShape::CTBasicSpeechBubble2, CalloutShape::CTBasicArrowText2, CalloutShape::CTBalloon5, CalloutShape::CTBasicSpeechBubble1] {
            for c in &colors {
                let mut q = base(ToolMode::Callouts);
                q.callout_shape = shape;
                q.callout_tails = vec![Pt::ZERO];
                if shape == CalloutShape::CTBalloon5 {
                    q.foreground_color = *c; q.background_color = Argb::WHITE; q.text_selection_color = Argb::BLACK; q.tail_color = *c; q.arrow_color = *c;
                } else {
                    q.foreground_color = Argb::TRANSPARENT; q.background_color = *c; q.tail_color = *c; q.arrow_color = *c;
                    q.text_selection_color = if c.luminance() > 0.6 { Argb::BLACK } else { Argb::WHITE };
                }
                out.push(q);
            }
        }
        for c in colors.iter().chain(neutrals.iter()) { let mut q = base(ToolMode::Pen); q.foreground_color = *c; q.arrow_color = *c; out.push(q); }
        for c in [Argb::rgb(0xFF, 0x5B, 0x5B), Argb::rgb(0xBB, 0x44, 0xFF), Argb::rgb(0x00, 0xFF, 0xCC), Argb::rgb(0xFF, 0xF0, 0x00)] { let mut q = base(ToolMode::Highlight); q.foreground_color = c; q.background_color = c; out.push(q); }
        for (end, w) in [(ArrowLineEndType::RoundedArrow, 10.0), (ArrowLineEndType::BarbedArrow, 6.0), (ArrowLineEndType::LineArrow, 4.0)] {
            for c in &colors { let mut q = base(ToolMode::Arrow); q.foreground_color = *c; q.arrow_color = *c; q.arrow_end = end; q.stroke_width = w; out.push(q); }
        }
        for c in &colors { let mut q = base(ToolMode::Shape); q.foreground_color = *c; out.push(q); }
        for (i, c) in colors.iter().enumerate() { let mut q = base(ToolMode::Shape); q.tool_shape = if i % 2 == 0 { ShapeShape::Ellipse } else { ShapeShape::RoundedRectangle }; q.foreground_color = *c; out.push(q); }
        for (fg, bg) in [(Argb::TRANSPARENT, Argb::BLACK), (Argb::TRANSPARENT, Argb::WHITE), (Argb::BLACK, Argb::TRANSPARENT), (Argb::WHITE, Argb::TRANSPARENT)] { let mut q = base(ToolMode::Shape); q.foreground_color = fg; q.background_color = bg; out.push(q); }
        for c in colors.iter().chain(neutrals.iter()) { let mut q = base(ToolMode::Line); q.foreground_color = *c; out.push(q); }
        for c in colors.iter().chain(neutrals.iter()) { let mut q = base(ToolMode::Fill); q.foreground_color = *c; q.background_color = *c; out.push(q); }
        for style in [StepStyle::Circle, StepStyle::Square, StepStyle::CirclePointed, StepStyle::SquarePointed] {
            for c in &colors { let mut q = base(ToolMode::Step); q.step_style = style; q.background_color = *c; q.foreground_color = if c.luminance() > 0.6 { Argb::BLACK } else { Argb::WHITE }; out.push(q); }
        }
        for size in [14.0, 18.0, 24.0, 36.0] {
            for c in colors.iter().chain(neutrals.iter().take(4)) { let mut q = base(ToolMode::Text); q.font_size = size; q.text_selection_color = *c; out.push(q); }
        }
        for shape in [ShapeShape::Ellipse, ShapeShape::Rectangle] {
            for c in colors.iter().take(3).chain([Argb::BLACK].iter()) { let mut q = base(ToolMode::Magnify); q.tool_shape = shape; q.foreground_color = *c; out.push(q); }
        }
        if let Some(first) = out.first_mut() { first.is_default = true; }
        self.quick_styles = out;
    }

    pub fn basic() -> Theme {
        let mut t = Theme { name: "Basic".into(), id: "9b3ad279-dd50-45af-8f21-1aabda7d92a6".into(), theme_colors: vec![Argb::rgb(0xFC, 0x42, 0x42), Argb::rgb(0x63, 0x22, 0x89), Argb::rgb(0x06, 0xAF, 0x8F), Argb::rgb(0xF7, 0xAC, 0x08)], is_default: true, ..Default::default() };
        t.generate_quick_styles();
        t
    }
    pub fn industrial() -> Theme {
        let mut t = Theme { name: "Industrial".into(), id: "dd1dc970-7ae3-4d3a-bab6-82cd39a3dbd0".into(), theme_colors: vec![Argb::rgb(0xF4, 0xBD, 0x21), Argb::rgb(0xF5, 0x86, 0x62), Argb::rgb(0x2C, 0x36, 0x49), Argb::rgb(0x99, 0x84, 0x94)], font_family: "Trebuchet MS".into(), is_default: true, ..Default::default() };
        t.generate_quick_styles();
        t
    }
    /// LinkShot's own default theme.
    pub fn starter() -> Theme {
        let mut t = Theme { name: "Starter".into(), id: "dda8f802-448a-4b75-a7e9-25e24bf1a749".into(), theme_colors: vec![Argb::rgb(0xFF, 0x59, 0x4B), Argb::rgb(0x2C, 0x7C, 0xB2), Argb::rgb(0x2B, 0xB6, 0x73), Argb::rgb(0xFF, 0xB8, 0x00), Argb::rgb(0x7D, 0x4F, 0xC9)], font_family: "Arial".into(), is_default: true, is_checked: true, drop_shadow: true, ..Default::default() };
        t.generate_quick_styles();
        t
    }
    pub fn styles_for(&self, tool: ToolMode) -> impl Iterator<Item = &QuickStyle> {
        self.quick_styles.iter().filter(move |q| q.tool_mode == tool)
    }
    pub fn read_file(path: &std::path::Path) -> anyhow::Result<Theme> {
        Ok(serde_json::from_slice(&std::fs::read(path)?)?)
    }
    pub fn write_file(&self, path: &std::path::Path) -> anyhow::Result<()> {
        Ok(std::fs::write(path, serde_json::to_string_pretty(self)?)?)
    }
}

/// A `.snagpalette`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct Palette {
    pub id: String,
    pub name: String,
    pub palette_colors: Vec<Argb>,
}
impl Default for Palette {
    fn default() -> Self {
        Self { id: uuid::Uuid::new_v4().to_string(), name: "Untitled".into(), palette_colors: vec![] }
    }
}
impl Palette {
    pub fn dark_grayscale() -> Palette {
        Palette { id: "88587969-d55c-4ad7-9885-a87214d15cd4".into(), name: "Dark Grayscale".into(), palette_colors: vec![Argb::rgb(0xAC, 0xAC, 0xAC), Argb::rgb(0x76, 0x76, 0x76), Argb::rgb(0x43, 0x43, 0x43), Argb::rgb(0x15, 0x15, 0x15)] }
    }
    pub fn light_grayscale() -> Palette {
        Palette { id: "88587969-d55c-4ad7-9885-a87214d15cd3".into(), name: "Light Grayscale".into(), palette_colors: vec![Argb::rgb(0xF2, 0xF2, 0xF2), Argb::rgb(0xD9, 0xD9, 0xD9), Argb::rgb(0xBF, 0xBF, 0xBF), Argb::rgb(0xA6, 0xA6, 0xA6)] }
    }
}

/// Toolbar item (`Tools2.xml` / `ToolBarItemVm`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ToolBarItem {
    pub item_type: ToolBarItemType,
    #[serde(default)]
    pub tool_mode: ToolMode,
    #[serde(default)]
    pub effect_type: EffectType,
    #[serde(default)]
    pub command_id: ToolBarCommandType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_id: Option<String>,
}
impl ToolBarItem {
    pub fn tool(t: ToolMode) -> Self { Self { item_type: ToolBarItemType::Tool, tool_mode: t, effect_type: EffectType::EffectNone, command_id: ToolBarCommandType::None, output_id: None } }
    pub fn command(c: ToolBarCommandType) -> Self { Self { item_type: ToolBarItemType::Command, tool_mode: ToolMode::None, effect_type: EffectType::EffectNone, command_id: c, output_id: None } }
    pub fn spacer() -> Self { Self { item_type: ToolBarItemType::Spacer, tool_mode: ToolMode::None, effect_type: EffectType::EffectNone, command_id: ToolBarCommandType::None, output_id: None } }
    pub fn more() -> Self { Self { item_type: ToolBarItemType::EditMore, tool_mode: ToolMode::None, effect_type: EffectType::EffectNone, command_id: ToolBarCommandType::None, output_id: None } }
    /// LinkShot's default toolbar.
    pub fn default_toolbar() -> Vec<ToolBarItem> {
        vec![
            Self::spacer(), Self::tool(ToolMode::Favorites), Self::tool(ToolMode::Arrow), Self::tool(ToolMode::Text),
            Self::tool(ToolMode::Callouts), Self::tool(ToolMode::Shape), Self::tool(ToolMode::Stamp), Self::tool(ToolMode::Step), Self::tool(ToolMode::Fill),
            Self::tool(ToolMode::VectorSelection), Self::more(), Self::command(ToolBarCommandType::Undo),
            Self::command(ToolBarCommandType::Redo), Self::spacer(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn theme_generation_and_json() {
        let t = Theme::basic();
        assert!(t.quick_styles.len() > 100);
        let json = serde_json::to_string(&t).unwrap();
        let back: Theme = serde_json::from_str(&json).unwrap();
        assert_eq!(back.quick_styles.len(), t.quick_styles.len());
        assert!(json.contains("\"CalloutShape\":\"CTRoundedRectWithArrow\""));
    }
}
