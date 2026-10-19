//! The vector document stored in a `.snagx` page JSON.
//!
//! Known keys are typed; every unknown key is preserved in `extra` so files written by other
//! versions round-trip without loss.

use crate::color::Argb;
use crate::enums::*;
#[allow(unused_imports)]
use crate::enums::{HAlign, VAlign};
use crate::geometry::{Pt, Rect};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

fn is_zero_u32(v: &u32) -> bool { *v == 0 }
fn version_default() -> String { "1.0".into() }
fn platform_default() -> String { "win".into() }
fn one_u32() -> u32 { 1 }
fn empty_expansion() -> Value { serde_json::json!(["0,0", "0,0"]) }
fn default_simplify() -> Value {
    serde_json::json!({"ActiveColorPaletteName": "ImageColors", "LockAllObjects": false, "Opacity": 100, "ShowOriginal": false, "SuiDetail": 0})
}

/// A vector object on the canvas (`CaptureObjects[]`).
///
/// Every property is optional so that files written by any version round-trip byte-for-byte
/// in content; use the accessor methods (`opacity()`, `fg()`, …) to get the defaults.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct CaptureObject {
    /// Object type: `Arrow`, `Line`, `Callout`, `Text`, `Shape`, `Highlight`, `Pen`, `Step`, `Stamp`,
    /// `Blur`, `Magnify`, `Image`, `Cursor`, `DropZone`, `RepeatableSection`, `Group`, …
    pub tool_mode: String,
    #[serde(rename = "ObjectID", default = "new_object_id")]
    pub object_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_point: Option<Pt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_point: Option<Pt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation_angle: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opacity: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_locked: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_flattened: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchored: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ignores_undo_all: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object_priority: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drop_zone_group_id: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aspect_ratio: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub foreground_color: Option<Argb>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background_color: Option<Argb>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke_width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dash_type: Option<DashType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drop_shadow_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow_color: Option<Argb>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow_blur: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow_opacity: Option<f64>,
    #[serde(default, rename = "ShadowDirectionX", skip_serializing_if = "Option::is_none")]
    pub shadow_direction_x: Option<f64>,
    #[serde(default, rename = "ShadowDirectionY", skip_serializing_if = "Option::is_none")]
    pub shadow_direction_y: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arrow_start: Option<ArrowLineEndType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arrow_end: Option<ArrowLineEndType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arrow_start_width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arrow_end_width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bezier_curve: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_shape: Option<ShapeShape>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corner_radius_ratio: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub callout_shape: Option<CalloutShape>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tail_style: Option<CalloutTailStyle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tail_head_style: Option<ArrowLineEndType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tail_line_style: Option<DashType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tail_width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tail_color: Option<Argb>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border_style: Option<BorderStyle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_scale: Option<f64>,
    #[serde(default, rename = "RTFEncodedText", skip_serializing_if = "Option::is_none")]
    pub rtf_encoded_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_selection_bold: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_selection_italic: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_selection_underline: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_selection_strikethrough: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_selection_color: Option<Argb>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_outline_color: Option<Argb>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_outline_width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_horizontal_align: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_vertical_align: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_padding: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_deselect_behavior: Option<TextDeselectBehavior>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pen_shape: Option<PenShape>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub smoothing: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub antialiasing: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step_style: Option<StepStyle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step_sequence_type: Option<StepSequenceType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step_value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blur_type: Option<BlurType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blur_intensity: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub magnify_scale: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub magnify_is_offset: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stamp_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stamp_folder: Option<String>,
    #[serde(default, rename = "Image", skip_serializing_if = "Option::is_none")]
    pub image_data: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stamp_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stamp_relative_path_to_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section_type: Option<SectionType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section_thumbnail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_required_version: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub points_array: Vec<Pt>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub control_points: Vec<Pt>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub callout_tails: Vec<Pt>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub section_graphics: Vec<CaptureObject>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub objects: Vec<CaptureObject>,
    /// Anything we don't model yet.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

pub fn new_object_id() -> String {
    uuid::Uuid::new_v4().to_string().to_uppercase()
}

impl Default for CaptureObject {
    fn default() -> Self {
        Self {
            tool_mode: "Shape".into(),
            object_id: new_object_id(),
            start_point: None,
            end_point: None,
            rotation_angle: None,
            opacity: None,
            is_locked: None,
            is_flattened: None,
            anchored: None,
            ignores_undo_all: None,
            object_priority: None,
            drop_zone_group_id: None,
            aspect_ratio: None,
            foreground_color: None,
            background_color: None,
            stroke_width: None,
            dash_type: None,
            drop_shadow_enabled: None,
            shadow_color: None,
            shadow_blur: None,
            shadow_opacity: None,
            shadow_direction_x: None,
            shadow_direction_y: None,
            arrow_start: None,
            arrow_end: None,
            arrow_start_width: None,
            arrow_end_width: None,
            bezier_curve: None,
            tool_shape: None,
            corner_radius_ratio: None,
            callout_shape: None,
            tail_style: None,
            tail_head_style: None,
            tail_line_style: None,
            tail_width: None,
            tail_color: None,
            border_style: None,
            current_scale: None,
            rtf_encoded_text: None,
            font_family: None,
            font_name: None,
            font_size: None,
            text_selection_bold: None,
            text_selection_italic: None,
            text_selection_underline: None,
            text_selection_strikethrough: None,
            text_selection_color: None,
            text_outline_color: None,
            text_outline_width: None,
            tool_horizontal_align: None,
            tool_vertical_align: None,
            tool_padding: None,
            text_deselect_behavior: None,
            pen_shape: None,
            smoothing: None,
            antialiasing: None,
            step_style: None,
            step_sequence_type: None,
            step_value: None,
            blur_type: None,
            blur_intensity: None,
            magnify_scale: None,
            magnify_is_offset: None,
            stamp_file: None,
            stamp_folder: None,
            image_data: None,
            stamp_source: None,
            stamp_relative_path_to_source: None,
            section_id: None,
            section_type: None,
            section_thumbnail: None,
            min_required_version: None,
            points_array: vec![],
            control_points: vec![],
            callout_tails: vec![],
            section_graphics: vec![],
            objects: vec![],
            extra: Map::new(),
        }
    }
}

/// Resolved drop-shadow parameters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadow {
    pub color: Argb,
    pub blur: f64,
    pub opacity: f64,
    pub dx: f64,
    pub dy: f64,
}

impl CaptureObject {
    pub fn new(tool_mode: &str) -> Self {
        Self { tool_mode: tool_mode.into(), ..Default::default() }
    }
    /// First defining point (arrow tail / box corner). `PointsArray` wins over Start/EndPoint because the
    /// mac build writes unrelated values into Start/EndPoint for some object types.
    pub fn p0(&self) -> Pt {
        if self.points_array.len() >= 2 { return self.points_array[0]; }
        self.start_point.or_else(|| self.points_array.first().copied()).unwrap_or(Pt::ZERO)
    }
    /// Second defining point (arrow head / opposite corner).
    pub fn p1(&self) -> Pt {
        if self.points_array.len() >= 2 { return self.points_array[1]; }
        self.end_point.unwrap_or_else(|| self.p0())
    }
    /// Sets both defining points (Windows style: `PointsArray`; we also keep Start/EndPoint if present).
    pub fn set_endpoints(&mut self, a: Pt, b: Pt) {
        if self.start_point.is_some() { self.start_point = Some(a); }
        if self.end_point.is_some() { self.end_point = Some(b); }
        if self.points_array.len() >= 2 { self.points_array[0] = a; self.points_array[1] = b; } else { self.points_array = vec![a, b]; }
    }
    pub fn opacity(&self) -> f64 { self.opacity.unwrap_or(100.0) }
    pub fn fg(&self) -> Argb { self.foreground_color.unwrap_or(Argb::BLACK) }
    pub fn bg(&self) -> Argb { self.background_color.unwrap_or(Argb::TRANSPARENT) }
    pub fn stroke_width(&self) -> f64 { self.stroke_width.unwrap_or(0.0) }
    pub fn dash(&self) -> DashType { self.dash_type.unwrap_or(DashType::Solid) }
    pub fn rotation(&self) -> f64 { self.rotation_angle.unwrap_or(0.0) }
    pub fn locked(&self) -> bool { self.is_locked.unwrap_or(0) != 0 }
    pub fn shadow(&self) -> Option<Shadow> {
        if !self.drop_shadow_enabled.unwrap_or(false) { return None; }
        Some(Shadow {
            color: self.shadow_color.unwrap_or(Argb::BLACK),
            blur: self.shadow_blur.unwrap_or(0.0),
            opacity: self.shadow_opacity.unwrap_or(60.0),
            dx: self.shadow_direction_x.unwrap_or(0.0),
            dy: self.shadow_direction_y.unwrap_or(0.0),
        })
    }
    pub fn set_shadow(&mut self, s: Option<Shadow>) {
        match s {
            None => self.drop_shadow_enabled = Some(false),
            Some(s) => {
                self.drop_shadow_enabled = Some(true);
                self.shadow_color = Some(s.color); self.shadow_blur = Some(s.blur); self.shadow_opacity = Some(s.opacity);
                self.shadow_direction_x = Some(s.dx); self.shadow_direction_y = Some(s.dy);
            }
        }
    }
    /// Axis-aligned bounds from the defining points (ignores rotation and stroke).
    pub fn bounds(&self) -> Rect {
        let mut r = Rect::from_points(self.p0(), self.p1());
        for p in &self.points_array { r = r.union(Rect::from_points(*p, *p)); }
        for p in &self.callout_tails { r = r.union(Rect::from_points(*p, *p)); }
        r
    }
    pub fn translate(&mut self, d: Pt) {
        if let Some(p) = self.start_point.as_mut() { *p = *p + d; }
        if let Some(p) = self.end_point.as_mut() { *p = *p + d; }
        for p in &mut self.points_array { *p = *p + d; }
        for p in &mut self.control_points { *p = *p + d; }
        for p in &mut self.callout_tails { *p = *p + d; }
    }
    pub fn is_text_like(&self) -> bool {
        matches!(self.tool_mode.as_str(), "Text" | "Callout")
    }
    pub fn h_align(&self) -> HAlign {
        self.tool_horizontal_align.as_deref().and_then(|s| s.parse().ok()).unwrap_or(HAlign::Center)
    }
    pub fn v_align(&self) -> VAlign {
        self.tool_vertical_align.as_deref().and_then(|s| s.parse().ok()).unwrap_or(VAlign::Center)
    }
    /// Rich text decoded from the RTF payload.
    pub fn rich_text(&self) -> crate::rtf::RichText {
        self.rtf_encoded_text.as_deref().map(crate::rtf::decode_base64_rtf).unwrap_or_default()
    }
    pub fn plain_text(&self) -> String {
        self.rich_text().plain()
    }
    pub fn text_style(&self) -> crate::rtf::TextStyle {
        crate::rtf::TextStyle {
            bold: self.text_selection_bold.unwrap_or(false),
            italic: self.text_selection_italic.unwrap_or(false),
            underline: self.text_selection_underline.unwrap_or(false),
            strike: self.text_selection_strikethrough.unwrap_or(false),
            color: self.text_selection_color.unwrap_or(Argb::BLACK),
            font_family: self.font_family.clone().unwrap_or_else(|| "Arial".into()),
            font_size: self.font_size.unwrap_or(14.0),
            align: self.h_align(),
        }
    }
    pub fn set_plain_text(&mut self, text: &str) {
        let style = self.text_style();
        self.rtf_encoded_text = Some(crate::rtf::encode_plain_to_base64_rtf(text, &style));
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct GradientBackground {
    #[serde(default)]
    pub gradient_angle: f64,
    #[serde(default)]
    pub gradient_colors: Vec<Argb>,
    #[serde(default)]
    pub gradient_style: GradientStyle,
}
impl Default for GradientBackground {
    fn default() -> Self {
        Self { gradient_angle: -90.0, gradient_colors: vec![Argb::TRANSPARENT], gradient_style: GradientStyle::GradientStyleSolid }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct SimplifySettings {
    #[serde(default)]
    pub current_color_palette: Vec<Argb>,
    #[serde(default)]
    pub is_locked: bool,
    #[serde(default)]
    pub show_original: bool,
    #[serde(default)]
    pub simplify_opacity: f64,
    #[serde(default)]
    pub sui_detail: u32,
}
impl Default for SimplifySettings {
    fn default() -> Self {
        let g = Argb::rgb(0x92, 0x92, 0x92);
        Self { current_color_palette: vec![g; 4], is_locked: false, show_original: false, simplify_opacity: 0.5, sui_detail: 8 }
    }
}

/// One page of a `.snagx` document (`{GUID}.json`).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct Page {
    #[serde(default = "version_default")]
    pub version: String,
    #[serde(default)]
    pub software_version: String,
    #[serde(default = "platform_default")]
    pub exported_from_platform: String,
    /// mac writes 72; Windows omits it (implied 96).
    #[serde(default, rename = "DPI", skip_serializing_if = "Option::is_none")]
    pub dpi: Option<f64>,
    #[serde(default)]
    pub capture_canvas_width: f64,
    #[serde(default)]
    pub capture_canvas_height: f64,
    /// Zip member name of the flattened background PNG.
    #[serde(default)]
    pub capture_background_image: String,
    #[serde(default)]
    pub capture_background_location: Pt,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background_size: Option<Pt>,
    #[serde(default)]
    pub capture_background_color: Argb,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gradient_background_color: Option<GradientBackground>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capture_color_space: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corner_radius_ratio: Option<f64>,
    /// Windows: `["0,0","0,0"]`; mac: `"0,0,0,0"`. Kept opaque.
    #[serde(default = "empty_expansion")]
    pub image_expansion_from_objects: Value,
    #[serde(default = "empty_expansion")]
    pub view_box_expansion_from_objects: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub should_auto_zoom_to_fit: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show_section_guides: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pre_snagx: Option<bool>,
    #[serde(default = "one_u32")]
    pub step_tool_sequence_numeric: u32,
    #[serde(default = "one_u32")]
    pub step_tool_sequence_uppercase_letter: u32,
    #[serde(default = "one_u32")]
    pub step_tool_sequence_lowercase_letter: u32,
    /// Platform-specific shape; kept opaque (see [`SimplifySettings`] for the mac layout).
    #[serde(default = "default_simplify")]
    pub simplify_settings: Value,
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub add_section_count: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sections: Vec<CaptureObject>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layout_array: Vec<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capture_objects: Vec<CaptureObject>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for Page {
    fn default() -> Self {
        Self {
            version: "1.0".into(),
            software_version: crate::SOFTWARE_VERSION.into(),
            exported_from_platform: "win".into(),
            dpi: None,
            capture_canvas_width: 0.0,
            capture_canvas_height: 0.0,
            capture_background_image: String::new(),
            capture_background_location: Pt::ZERO,
            background_size: None,
            capture_background_color: Argb::TRANSPARENT,
            gradient_background_color: None,
            capture_color_space: None,
            corner_radius_ratio: Some(0.0),
            image_expansion_from_objects: empty_expansion(),
            view_box_expansion_from_objects: empty_expansion(),
            should_auto_zoom_to_fit: None,
            show_section_guides: None,
            pre_snagx: None,
            step_tool_sequence_numeric: 1,
            step_tool_sequence_uppercase_letter: 1,
            step_tool_sequence_lowercase_letter: 1,
            simplify_settings: default_simplify(),
            add_section_count: 0,
            sections: vec![],
            layout_array: vec![],
            effects: vec![],
            capture_objects: vec![],
            extra: Map::new(),
        }
    }
}

impl Page {
    pub fn dpi(&self) -> f64 { self.dpi.unwrap_or(crate::DEFAULT_DPI) }
    pub fn canvas_rect(&self) -> Rect {
        Rect::from_xywh(0.0, 0.0, self.capture_canvas_width, self.capture_canvas_height)
    }
    /// Next label for the step tool and advance the counter.
    pub fn next_step_label(&mut self, seq: StepSequenceType) -> String {
        let counter = match seq {
            StepSequenceType::Number => &mut self.step_tool_sequence_numeric,
            StepSequenceType::UppercaseLetter => &mut self.step_tool_sequence_uppercase_letter,
            StepSequenceType::LowercaseLetter => &mut self.step_tool_sequence_lowercase_letter,
        };
        let label = seq.label(*counter);
        *counter += 1;
        label
    }
    pub fn restart_steps(&mut self) {
        self.step_tool_sequence_numeric = 1;
        self.step_tool_sequence_uppercase_letter = 1;
        self.step_tool_sequence_lowercase_letter = 1;
    }
}

/// `metadata.json`
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct Metadata {
    /// `YYYY-MM-DD HH:MM:SS` local time.
    #[serde(default)]
    pub capture_date: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default = "version_default")]
    pub version: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}
impl Default for Metadata {
    fn default() -> Self {
        Self {
            capture_date: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            tags: vec![],
            version: "1.0".into(),
            extra: Map::new(),
        }
    }
}

/// A whole document: pages plus their background bitmaps.
#[derive(Clone, Debug, Default)]
pub struct Document {
    pub metadata: Metadata,
    pub pages: Vec<Page>,
    /// Zip members (PNG bytes) keyed by member name; the page's `capture_background_image` points here.
    /// Shared so cloning a document for undo or a drag copies refcounts, not megabytes.
    pub blobs: std::collections::BTreeMap<String, std::sync::Arc<Vec<u8>>>,
    pub thumbnail_png: Option<Vec<u8>>,
    /// Where this document lives on disk, if saved.
    pub path: Option<std::path::PathBuf>,
}

impl Document {
    /// Creates a single-page document around a captured RGBA image.
    pub fn from_rgba(width: u32, height: u32, rgba: &[u8]) -> anyhow::Result<Document> {
        let img = image::RgbaImage::from_raw(width, height, rgba.to_vec())
            .ok_or_else(|| anyhow::anyhow!("rgba buffer size mismatch"))?;
        let mut png = Vec::new();
        image::DynamicImage::ImageRgba8(img).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)?;
        Ok(Self::from_png(width, height, png))
    }
    pub fn from_png(width: u32, height: u32, png: Vec<u8>) -> Document {
        let id = format!("{{{}}}", uuid::Uuid::new_v4().to_string().to_uppercase());
        let member = format!("{id}.png");
        let page = Page {
            capture_canvas_width: width as f64,
            capture_canvas_height: height as f64,
            capture_background_image: member.clone(),
            ..Default::default()
        };
        let mut blobs = std::collections::BTreeMap::new();
        blobs.insert(member, std::sync::Arc::new(png));
        Document { metadata: Metadata::default(), pages: vec![page], blobs, thumbnail_png: None, path: None }
    }
    pub fn page(&self) -> &Page {
        &self.pages[0]
    }
    pub fn page_mut(&mut self) -> &mut Page {
        &mut self.pages[0]
    }
    pub fn background_png(&self, page: &Page) -> Option<&[u8]> {
        self.blobs.get(&page.capture_background_image).map(|v| v.as_slice())
    }
    pub fn display_name(&self) -> String {
        self.path.as_ref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Untitled".into())
    }
}
