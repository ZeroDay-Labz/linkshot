//! Capture settings and presets.

use crate::enums::*;
use crate::geometry::Rect;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct RegionSettings {
    pub region_selection_type: RegionSelectionType,
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
    pub set_position: bool,
}
impl Default for RegionSettings {
    fn default() -> Self {
        Self { region_selection_type: RegionSelectionType::RegionSelectionPrompt, left: 0, top: 0, width: 800, height: 600, set_position: false }
    }
}
impl RegionSettings {
    pub fn rect(&self) -> Rect {
        Rect::from_xywh(self.left as f64, self.top as f64, self.width as f64, self.height as f64)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
pub struct WindowSettings {
    pub window_selection_type: WindowSelectionType,
    pub window_size_selection_type: WindowSizeSelectionType,
    pub width: i32,
    pub height: i32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct ScrollingSettings {
    pub scroll_from_top: bool,
    pub scroll_from_left: bool,
    pub delay_ms: u32,
}
impl Default for ScrollingSettings {
    fn default() -> Self { Self { scroll_from_top: true, scroll_from_left: true, delay_ms: 0 } }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct MenuSettings {
    pub delay_seconds: u32,
    pub capture_cascading: bool,
}
impl Default for MenuSettings {
    fn default() -> Self { Self { delay_seconds: 1, capture_cascading: false } }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct ShareAutoNameSettings {
    /// Pattern using `<Prefix>`, `<AutoNum>`, `<Date>`, `<Time>`, `<Year>`, `<Month>`, `<Day>`, `<Hour>`, `<Minute>`, `<Second>`, `<Application>`, `<WindowTitle>`.
    pub file_format: String,
    pub prefix: String,
    pub digits: u32,
    pub start_number: u32,
    pub overwrite: bool,
}
impl Default for ShareAutoNameSettings {
    fn default() -> Self {
        Self { file_format: "<Prefix><AutoNum>".into(), prefix: "SNAG-".into(), digits: 4, start_number: 0, overwrite: false }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct ShareSettingsFile {
    pub file_output_prompt: bool,
    pub file_output_fixed: bool,
    pub fixed_name: String,
    pub file_output_auto_name: bool,
    pub use_last_folder: bool,
    pub folder_name: String,
}
impl Default for ShareSettingsFile {
    fn default() -> Self {
        Self { file_output_prompt: true, file_output_fixed: false, fixed_name: "LINKSHOT".into(), file_output_auto_name: false, use_last_folder: true, folder_name: String::new() }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct ShareSettingsEmail {
    pub prompt_for_settings: bool,
    pub recipient_name: String,
    pub address: String,
    pub subject: String,
    pub message_text: String,
}
impl Default for ShareSettingsEmail {
    fn default() -> Self { Self { prompt_for_settings: true, recipient_name: String::new(), address: String::new(), subject: String::new(), message_text: String::new() } }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct ShareSettingsFtp {
    pub server: String,
    pub target_directory: String,
    pub target_file: String,
    pub user: String,
    pub encrypted_password: String,
    pub proxy_server: String,
    pub auth_required: bool,
    pub use_rename: bool,
    pub passive: bool,
    pub use_proxy: bool,
    pub port: u16,
    pub sequence_limit: u32,
    pub show_progress: bool,
    pub auto_naming: bool,
}
impl Default for ShareSettingsFtp {
    fn default() -> Self {
        Self { server: String::new(), target_directory: String::new(), target_file: String::new(), user: String::new(), encrypted_password: String::new(), proxy_server: String::new(), auth_required: false, use_rename: false, passive: false, use_proxy: false, port: 21, sequence_limit: 0, show_progress: false, auto_naming: false }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct ShareSettingsProgram {
    pub file_output_prompt: bool,
    pub file_output_auto_name: bool,
    pub folder_name: String,
    /// linkshot: the program to run (`<CaptureFilename>` placeholder in `parameters`).
    pub executable: String,
    pub parameters: String,
    pub display_name: String,
}
impl Default for ShareSettingsProgram {
    fn default() -> Self {
        Self { file_output_prompt: true, file_output_auto_name: false, folder_name: String::new(), executable: String::new(), parameters: "\"<CaptureFilename>\"".into(), display_name: String::new() }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct ShareSettingsFileSave {
    /// `LastUsedSaveOptions`, `PNG`, `JPEG`, `GIF`, `BMP`, `TIF`, `PDF`, `SNAGX`, `WEBP`…
    pub output_file_type: String,
    pub bits_per_pixel: u32,
    pub sub_file_type: String,
    pub quality: u32,
    pub transparency_enabled: bool,
    pub is_multipage: bool,
    pub pdf_page_height: u32,
    pub pdf_page_width: u32,
    pub pdf_image_layout: u32,
    pub pdf_page_size: u32,
    pub pdf_landscape: bool,
    pub pdf_margin_top: u32,
    pub pdf_margin_bottom: u32,
    pub pdf_margin_left: u32,
    pub pdf_margin_right: u32,
    pub pdf_author: String,
    pub pdf_title: String,
    pub pdf_keywords: String,
}
impl Default for ShareSettingsFileSave {
    fn default() -> Self {
        Self { output_file_type: "LastUsedSaveOptions".into(), bits_per_pixel: 0, sub_file_type: "SUB_BMP".into(), quality: 0, transparency_enabled: false, is_multipage: false, pdf_page_height: 0, pdf_page_width: 0, pdf_image_layout: 0, pdf_page_size: 0, pdf_landscape: false, pdf_margin_top: 0, pdf_margin_bottom: 0, pdf_margin_left: 0, pdf_margin_right: 0, pdf_author: String::new(), pdf_title: String::new(), pdf_keywords: String::new() }
    }
}

/// An effect applied at capture time (settings are stored as a JSON object keyed by effect type).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct EffectSettings {
    pub effect_type: EffectType,
    #[serde(flatten)]
    pub params: serde_json::Map<String, serde_json::Value>,
}
impl Default for EffectSettings {
    fn default() -> Self { Self { effect_type: EffectType::EffectNone, params: Default::default() } }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct HardwareDeviceInfo {
    pub availability: String,
    pub display_name: String,
    pub device_path: String,
}
impl Default for HardwareDeviceInfo {
    fn default() -> Self { Self { availability: "Available".into(), display_name: String::new(), device_path: String::new() } }
}

/// A key combination (`HotKey` + `HotKeyModifiers`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
pub struct HotKey {
    /// WPF `System.Windows.Input.Key` name, e.g. `PrintScreen`, `F9`, `Oem2`, `D1`, `A`; `None` = unassigned.
    pub key: String,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
}
impl HotKey {
    pub fn none() -> Self { Self { key: "None".into(), ..Default::default() } }
    pub fn is_none(&self) -> bool { self.key.is_empty() || self.key == "None" }
    pub fn parse_modifiers(s: &str) -> (bool, bool, bool, bool) {
        let l = s.to_lowercase();
        (l.contains("control") || l.contains("ctrl"), l.contains("alt"), l.contains("shift"), l.contains("windows") || l.contains("win"))
    }
    pub fn modifiers_string(&self) -> String {
        let mut v = vec![];
        if self.alt { v.push("Alt"); }
        if self.ctrl { v.push("Control"); }
        if self.shift { v.push("Shift"); }
        if self.win { v.push("Windows"); }
        if v.is_empty() { "None".into() } else { v.join(" ") }
    }
    pub fn display(&self) -> String {
        if self.is_none() { return "Add Hotkey".into(); }
        let mut parts = vec![];
        if self.ctrl { parts.push("Ctrl".to_string()); }
        if self.alt { parts.push("Alt".to_string()); }
        if self.shift { parts.push("Shift".to_string()); }
        if self.win { parts.push("Win".to_string()); }
        parts.push(match self.key.as_str() {
            "PrintScreen" | "Snapshot" => "Print Screen".to_string(),
            "Oem2" | "OemQuestion" => "/".to_string(),
            "Oem3" => "`".to_string(),
            "OemMinus" => "-".to_string(),
            "OemPlus" => "=".to_string(),
            k if k.len() == 2 && k.starts_with('D') => k[1..].to_string(),
            k => k.to_string(),
        });
        parts.join("+")
    }
}

/// Everything a capture needs — a preset is just a named one of these.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct CaptureSettings {
    pub id: String,
    pub auto_generate_name: bool,
    pub is_default: bool,
    pub display_name: String,
    pub capture_mode: CaptureMode,
    pub selection_mode: SelectionMode,
    pub selection_settings_region: RegionSettings,
    pub selection_settings_window: WindowSettings,
    pub selection_settings_scrolling_window: ScrollingSettings,
    pub selection_settings_menu: MenuSettings,
    pub share_auto_name_settings: ShareAutoNameSettings,
    pub share_settings_file: ShareSettingsFile,
    pub share_settings_email: ShareSettingsEmail,
    pub share_settings_ftp: ShareSettingsFtp,
    pub share_settings_program: ShareSettingsProgram,
    pub share_settings_file_save: ShareSettingsFileSave,
    pub scheduled_capture_time: String,
    pub time_delay: bool,
    pub timed_capture_type: TimedCaptureType,
    pub interval_units: IntervalUnits,
    pub interval_discard_identical: bool,
    pub delay_duration: u32,
    pub interval_duration: u32,
    pub capture_cursor: bool,
    pub preview_in_editor: bool,
    pub copy_to_clipboard: bool,
    pub capture_effects: Vec<EffectSettings>,
    pub share_destination_ids: Vec<String>,
    pub enable_microphone: bool,
    pub enable_system_audio: bool,
    pub enable_webcam: bool,
    pub start_webcam: bool,
    pub selected_webcam_device: HardwareDeviceInfo,
    pub selected_webcam_placement: WebcamPlacement,
    pub pip_shape: PreviewClipShape,
    pub screen_draw_starts_on: bool,
    pub screen_draw_fade_delay: u32,
    pub cursor_highlight_enabled: bool,
    pub cursor_highlight_color: crate::Argb,
    pub click_animation_enabled: bool,
    pub click_animation_color: crate::Argb,
    pub selected_microphone: HardwareDeviceInfo,
    pub hot_key: HotKey,
}

impl Default for CaptureSettings {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            auto_generate_name: true,
            is_default: false,
            display_name: String::new(),
            capture_mode: CaptureMode::CaptureModeAllInOne,
            selection_mode: SelectionMode::SelectionModeRegion,
            selection_settings_region: Default::default(),
            selection_settings_window: Default::default(),
            selection_settings_scrolling_window: Default::default(),
            selection_settings_menu: Default::default(),
            share_auto_name_settings: Default::default(),
            share_settings_file: Default::default(),
            share_settings_email: Default::default(),
            share_settings_ftp: Default::default(),
            share_settings_program: Default::default(),
            share_settings_file_save: Default::default(),
            scheduled_capture_time: String::new(),
            time_delay: false,
            timed_capture_type: TimedCaptureType::Delay,
            interval_units: IntervalUnits::Seconds,
            interval_discard_identical: false,
            delay_duration: 5,
            interval_duration: 1,
            capture_cursor: false,
            preview_in_editor: true,
            copy_to_clipboard: false,
            capture_effects: vec![],
            share_destination_ids: vec![output_ids::NONE.into()],
            enable_microphone: false,
            enable_system_audio: false,
            enable_webcam: false,
            start_webcam: false,
            selected_webcam_device: Default::default(),
            selected_webcam_placement: WebcamPlacement::BottomRight,
            pip_shape: PreviewClipShape::Circle,
            screen_draw_starts_on: false,
            screen_draw_fade_delay: 3,
            cursor_highlight_enabled: false,
            cursor_highlight_color: crate::Argb::rgb(0xFF, 0xFF, 0x00),
            click_animation_enabled: false,
            click_animation_color: crate::Argb::rgb(0xE8, 0x08, 0x08),
            selected_microphone: Default::default(),
            hot_key: HotKey::none(),
        }
    }
}

impl CaptureSettings {
    pub fn is_image(&self) -> bool { self.capture_mode == CaptureMode::CaptureModeImage }
    pub fn is_video(&self) -> bool { self.capture_mode == CaptureMode::CaptureModeVideo }
    pub fn is_all_in_one(&self) -> bool { self.capture_mode == CaptureMode::CaptureModeAllInOne }
    pub fn is_ocr(&self) -> bool { self.selection_mode == SelectionMode::SelectionModeOcr }
    /// Interval/Schedule timed captures and automatic scrolling turn the Capture button into Start/Stop.
    pub fn is_toggleable(&self) -> bool {
        (self.time_delay && self.timed_capture_type != TimedCaptureType::Delay) || self.selection_mode == SelectionMode::SelectionModeScrollingWindow
    }
    /// The auto-generated preset name: "Image with effects to File, Clipboard".
    pub fn generate_name(&self) -> String {
        let mut s = match self.capture_mode {
            CaptureMode::CaptureModeAllInOne => "All-in-One".to_string(),
            CaptureMode::CaptureModeVideo => "Video".to_string(),
            CaptureMode::CaptureModeImage => {
                let n = self.capture_effects.iter().filter(|e| e.effect_type != EffectType::EffectNone).count();
                match n { 0 => "Image".to_string(), 1 => "Image with effect".to_string(), _ => "Image with effects".to_string() }
            }
        };
        let dests: Vec<&str> = self.share_destination_ids.iter().filter(|d| d.as_str() != output_ids::NONE).map(|d| output_ids::title(d)).collect();
        if !dests.is_empty() { s.push_str(" to "); s.push_str(&dests.join(", ")); }
        s
    }
    pub fn effective_name(&self) -> String {
        if self.auto_generate_name || self.display_name.is_empty() { self.generate_name() } else { self.display_name.clone() }
    }
}

/// The preset store.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct SavedPresets {
    pub version: u32,
    pub selected_mode: CaptureMode,
    pub all_in_one: CaptureSettings,
    pub image: CaptureSettings,
    pub video: CaptureSettings,
    pub one_click: CaptureSettings,
    pub presets: Vec<CaptureSettings>,
    /// Global capture hotkey (default Print Screen).
    pub global_hot_key: HotKey,
}

impl Default for SavedPresets {
    fn default() -> Self {
        let mut all_in_one = CaptureSettings { capture_mode: CaptureMode::CaptureModeAllInOne, display_name: "All-in-One".into(), ..Default::default() };
        all_in_one.id = "{AllInOne}".into();
        let mut image = CaptureSettings { capture_mode: CaptureMode::CaptureModeImage, display_name: "Image".into(), ..Default::default() };
        image.id = "{Image}".into();
        let mut video = CaptureSettings { capture_mode: CaptureMode::CaptureModeVideo, display_name: "Video".into(), capture_cursor: true, enable_microphone: true, ..Default::default() };
        video.id = "{Video}".into();
        let mut one_click = CaptureSettings { capture_mode: CaptureMode::CaptureModeAllInOne, ..Default::default() };
        one_click.id = "{OneClickCapture}".into();
        let scrolling = CaptureSettings {
            id: "{v6_DefPanoramic}".into(),
            auto_generate_name: false,
            display_name: "Scrolling Capture".into(),
            capture_mode: CaptureMode::CaptureModeImage,
            selection_mode: SelectionMode::SelectionModePanoramic,
            hot_key: HotKey { key: "Oem2".into(), ctrl: true, alt: true, shift: false, win: false },
            ..Default::default()
        };
        let grab_text = CaptureSettings {
            id: "{v6_DefGrabText}".into(),
            auto_generate_name: false,
            display_name: "Grab Text".into(),
            capture_mode: CaptureMode::CaptureModeImage,
            selection_mode: SelectionMode::SelectionModeOcr,
            ..Default::default()
        };
        Self {
            version: 6,
            selected_mode: CaptureMode::CaptureModeAllInOne,
            all_in_one,
            image,
            video,
            one_click,
            presets: vec![scrolling, grab_text],
            global_hot_key: HotKey { key: "PrintScreen".into(), ..Default::default() },
        }
    }
}

impl SavedPresets {
    pub fn selected(&self) -> &CaptureSettings {
        match self.selected_mode {
            CaptureMode::CaptureModeAllInOne => &self.all_in_one,
            CaptureMode::CaptureModeImage => &self.image,
            CaptureMode::CaptureModeVideo => &self.video,
        }
    }
    pub fn selected_mut(&mut self) -> &mut CaptureSettings {
        match self.selected_mode {
            CaptureMode::CaptureModeAllInOne => &mut self.all_in_one,
            CaptureMode::CaptureModeImage => &mut self.image,
            CaptureMode::CaptureModeVideo => &mut self.video,
        }
    }
    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        Ok(serde_json::from_slice(&std::fs::read(path)?)?)
    }
    pub fn save(&self, path: &std::path::Path) -> anyhow::Result<()> {
        if let Some(p) = path.parent() { std::fs::create_dir_all(p)?; }
        Ok(std::fs::write(path, serde_json::to_string_pretty(self)?)?)
    }

    /// Imports presets from a `.snagpresets` XML file (UTF-16 or UTF-8 XML).
    pub fn import_linkshot_xml(bytes: &[u8]) -> anyhow::Result<Vec<CaptureSettings>> {
        let text = decode_text(bytes);
        let mut out = Vec::new();
        let mut reader = quick_xml::Reader::from_str(&text);
        reader.config_mut().trim_text(true);
        let mut path: Vec<String> = Vec::new();
        let mut cur: Option<CaptureSettings> = None;
        let mut current_text = String::new();
        loop {
            match reader.read_event()? {
                quick_xml::events::Event::Start(e) => {
                    let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    path.push(name.clone());
                    // A preset is any direct child of <Presets>, or the AllInOne/Image/Video/OneClick nodes.
                    if path.len() == 2 && matches!(name.as_str(), "AllInOne" | "Image" | "Video" | "OneClick") || (path.len() == 3 && path[1] == "Presets") {
                        cur = Some(CaptureSettings { id: if path.len() == 2 { format!("{{{name}}}") } else { uuid::Uuid::new_v4().to_string() }, ..Default::default() });
                    }
                    current_text.clear();
                }
                quick_xml::events::Event::Text(t) => current_text = t.unescape()?.to_string(),
                quick_xml::events::Event::End(_) => {
                    let name = path.pop().unwrap_or_default();
                    if let Some(c) = cur.as_mut() {
                        let v = current_text.trim();
                        let parent = path.last().map(String::as_str).unwrap_or("");
                        match (parent, name.as_str()) {
                            (_, "DisplayName") if path.len() <= 3 => c.display_name = v.into(),
                            (_, "AutoGenerateName") => c.auto_generate_name = v == "true",
                            (_, "IsDefault") => c.is_default = v == "true",
                            (_, "CaptureMode") => c.capture_mode = v.parse().unwrap_or_default(),
                            (_, "SelectionMode") => c.selection_mode = v.parse().unwrap_or_default(),
                            ("SelectionSettingsRegion", "RegionSelectionType") => c.selection_settings_region.region_selection_type = v.parse().unwrap_or_default(),
                            ("Region", "Left") => c.selection_settings_region.left = v.parse().unwrap_or(0),
                            ("Region", "Top") => c.selection_settings_region.top = v.parse().unwrap_or(0),
                            ("Region", "Width") => c.selection_settings_region.width = v.parse().unwrap_or(800),
                            ("Region", "Height") => c.selection_settings_region.height = v.parse().unwrap_or(600),
                            ("ShareAutoNameSettings", "FileFormat") => c.share_auto_name_settings.file_format = v.into(),
                            ("ShareAutoNameSettings", "Prefix") => c.share_auto_name_settings.prefix = v.into(),
                            ("ShareAutoNameSettings", "Digits") => c.share_auto_name_settings.digits = v.parse().unwrap_or(4),
                            ("ShareSettingsFile", "FileOutputPrompt") => c.share_settings_file.file_output_prompt = v == "true",
                            ("ShareSettingsFile", "FileOutputFixed") => c.share_settings_file.file_output_fixed = v == "true",
                            ("ShareSettingsFile", "FixedName") => c.share_settings_file.fixed_name = v.into(),
                            ("ShareSettingsFile", "FileOutputAutoName") => c.share_settings_file.file_output_auto_name = v == "true",
                            ("ShareSettingsFile", "UseLastFolder") => c.share_settings_file.use_last_folder = v == "true",
                            ("ShareSettingsFile", "FolderName") => c.share_settings_file.folder_name = v.into(),
                            ("ShareSettingsFileSave", "OutputFileType") => c.share_settings_file_save.output_file_type = v.into(),
                            (_, "TimeDelay") => c.time_delay = v == "true",
                            (_, "TimedCaptureType") => c.timed_capture_type = v.parse().unwrap_or_default(),
                            (_, "IntervalUnits") => c.interval_units = v.parse().unwrap_or_default(),
                            (_, "IntervalDiscardIdentical") => c.interval_discard_identical = v == "true",
                            (_, "DelayDuration") => c.delay_duration = v.parse().unwrap_or(5),
                            (_, "IntervalDuration") => c.interval_duration = v.parse().unwrap_or(1),
                            (_, "CaptureCursor") => c.capture_cursor = v == "true",
                            (_, "PreviewInEditor") => c.preview_in_editor = v == "true",
                            (_, "CopyToClipboard") => c.copy_to_clipboard = v == "true",
                            ("ShareDestinationIds", "string") => { if c.share_destination_ids == vec![output_ids::NONE.to_string()] { c.share_destination_ids.clear(); } c.share_destination_ids.push(v.into()); }
                            (_, "EnableMicrophone") => c.enable_microphone = v == "true",
                            (_, "EnableSystemAudio") => c.enable_system_audio = v == "true",
                            (_, "EnableWebcam") => c.enable_webcam = v == "true",
                            (_, "SelectedWebcamPlacement") => c.selected_webcam_placement = v.parse().unwrap_or_default(),
                            (_, "PipShape") => c.pip_shape = v.parse().unwrap_or_default(),
                            (_, "ScreenDrawStartsOn") => c.screen_draw_starts_on = v == "true",
                            (_, "ScreenDrawFadeDelay") => c.screen_draw_fade_delay = v.parse().unwrap_or(3),
                            (_, "CursorHighlightEnabled") => c.cursor_highlight_enabled = v == "true",
                            (_, "CursorHighlightColorString") => c.cursor_highlight_color = crate::Argb::parse(v).unwrap_or(c.cursor_highlight_color),
                            (_, "ClickAnimationEnabled") => c.click_animation_enabled = v == "true",
                            (_, "ClickAnimationColorString") => c.click_animation_color = crate::Argb::parse(v).unwrap_or(c.click_animation_color),
                            (_, "HotKey") => c.hot_key.key = v.into(),
                            (_, "HotKeyModifiers") => { let (ctrl, alt, shift, win) = HotKey::parse_modifiers(v); c.hot_key.ctrl = ctrl; c.hot_key.alt = alt; c.hot_key.shift = shift; c.hot_key.win = win; }
                            _ => {}
                        }
                        for e in c.capture_effects.iter_mut() { let _ = e; }
                        if (path.len() == 1 && matches!(name.as_str(), "AllInOne" | "Image" | "Video" | "OneClick")) || (path.len() == 2 && path[1] == "Presets") {
                            if c.share_destination_ids.is_empty() { c.share_destination_ids.push(output_ids::NONE.into()); }
                            out.push(cur.take().unwrap());
                        }
                    }
                    current_text.clear();
                }
                quick_xml::events::Event::Eof => break,
                _ => {}
            }
        }
        Ok(out)
    }
}

fn decode_text(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let u16s: Vec<u16> = bytes[2..].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&u16s)
    } else if bytes.starts_with(&[0xFE, 0xFF]) {
        let u16s: Vec<u16> = bytes[2..].chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&u16s)
    } else {
        let s = String::from_utf8_lossy(bytes).into_owned();
        s.strip_prefix('\u{feff}').map(str::to_string).unwrap_or(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names_and_import() {
        let p = SavedPresets::default();
        assert_eq!(p.presets[0].display_name, "Scrolling Capture");
        assert_eq!(p.presets[0].hot_key.display(), "Ctrl+Alt+/");
        let mut img = CaptureSettings { capture_mode: CaptureMode::CaptureModeImage, ..Default::default() };
        img.capture_effects.push(EffectSettings { effect_type: EffectType::EffectBorder, params: Default::default() });
        img.share_destination_ids = vec![output_ids::FILE.into(), output_ids::CLIPBOARD.into()];
        assert_eq!(img.generate_name(), "Image with effect to File, Clipboard");
        let xml = r#"<?xml version="1.0" encoding="utf-16"?><SavedPresets><Version>6</Version><SelectedMode>CaptureModeAllInOne</SelectedMode>
<Image><DisplayName>Image</DisplayName><CaptureMode>CaptureModeImage</CaptureMode><SelectionMode>SelectionModeRegion</SelectionMode><HotKey>None</HotKey><HotKeyModifiers>None</HotKeyModifiers></Image>
<Presets><CaptureSettings><DisplayName>Scrolling Capture</DisplayName><CaptureMode>CaptureModeImage</CaptureMode><SelectionMode>SelectionModePanoramic</SelectionMode><ShareDestinationIds><string>25235</string></ShareDestinationIds><HotKey>Oem2</HotKey><HotKeyModifiers>Alt Control</HotKeyModifiers></CaptureSettings></Presets></SavedPresets>"#;
        let imported = SavedPresets::import_linkshot_xml(xml.as_bytes()).unwrap();
        assert_eq!(imported.len(), 2);
        let sc = &imported[1];
        assert_eq!(sc.display_name, "Scrolling Capture");
        assert_eq!(sc.selection_mode, SelectionMode::SelectionModePanoramic);
        assert!(sc.hot_key.ctrl && sc.hot_key.alt && !sc.hot_key.shift);
        assert_eq!(sc.share_destination_ids, vec![output_ids::FILE.to_string()]);
    }
}
