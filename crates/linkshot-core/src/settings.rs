//! Application settings, stored as JSON.

use crate::enums::TrayThumbnailSize;
use crate::style::ToolBarItem;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum UiTheme { Light, #[default] Dark }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CaptureInterface { #[default] CaptureWindow, CaptureWindowAndWidget }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TooltipMode { Minimal, #[default] TextOnly, VideoAndText }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum VideoQuality { Low, Medium, #[default] High }

/// The GIF "Optimize for" presets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum GifPreset { #[default] ScreenVideo, HighMotion, ReducedFileSize, Custom }

/// The "Create Animated GIF" dialog values.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GifExportSettings {
    pub preset: GifPreset,
    pub fps: f32,
    /// 0 = original size.
    pub width: u32,
    pub height: u32,
    pub lock_aspect: bool,
    pub dithering: bool,
    pub loop_forever: bool,
}

impl Default for GifExportSettings {
    fn default() -> Self { Self { preset: GifPreset::ScreenVideo, fps: 10.0, width: 0, height: 0, lock_aspect: true, dithering: false, loop_forever: true } }
}

impl GifExportSettings {
    /// Values for a preset (Custom keeps the current ones).
    pub fn apply_preset(&mut self, p: GifPreset) {
        self.preset = p;
        match p {
            GifPreset::ScreenVideo => { self.fps = 10.0; self.width = 0; self.height = 0; self.dithering = false; }
            GifPreset::HighMotion => { self.fps = 15.0; self.width = 0; self.height = 0; self.dithering = true; }
            GifPreset::ReducedFileSize => { self.fps = 5.0; self.width = 640; self.height = 0; self.dithering = false; }
            GifPreset::Custom => {}
        }
    }
}

impl VideoQuality {
    /// Recording frame rate for a quality level.
    pub fn fps(self) -> u32 { match self { VideoQuality::Low => 10, VideoQuality::Medium => 20, VideoQuality::High => 30 } }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    // General
    pub ui_theme: UiTheme,
    /// Interface scale on top of the monitor's own scale factor (1.0 = native).
    pub ui_scale: f32,
    /// Bumped when the default theme changes so existing settings files pick up the new default once.
    pub theme_generation: u32,
    pub language: String,
    pub run_at_startup: bool,
    /// The "make LinkShot your default screenshot tool" question was answered (or dismissed for good).
    pub default_tool_prompted: bool,
    /// Print Screen is bound to LinkShot by the desktop itself (see `linkshot_platform::default_tool`), so the
    /// in-app hotkey must not claim it too.
    pub system_print_screen: bool,
    pub capture_interface: CaptureInterface,
    pub auto_check_updates: bool,
    pub show_onboarding: bool,
    pub tooltip_mode: TooltipMode,

    // Capture
    pub hide_linkshot_during_capture: bool,
    pub open_system_screenshots: bool,
    pub show_magnifier: bool,
    pub simplified_crosshairs: bool,
    pub require_start_button_for_scrolling: bool,
    /// Wayland RemoteDesktop portal token so Automatic Scrolling does not prompt every time.
    pub remote_desktop_token: Option<String>,
    /// ScreenCast portal restore tokens (without / with the pointer embedded) so screen grabs on
    /// Wayland desktops without KWin access prompt only once.
    pub screencast_token: Option<String>,
    pub screencast_token_cursor: Option<String>,
    /// Grab Text: Tesseract language(s) when the CLI is installed (e.g. "eng", "eng+deu").
    pub ocr_language: String,
    /// Grab Text: always use the bundled models even when Tesseract is installed.
    pub ocr_prefer_builtin: bool,
    pub mic_muted_alert: bool,
    pub hdr_capture: bool,
    pub auto_start_recording: bool,
    pub show_video_countdown: bool,
    pub show_ready_to_record: bool,
    pub minimize_fullscreen_controls: bool,
    pub scale_video_to_1080p: bool,
    pub enable_webcam: bool,
    pub disable_hw_recording: bool,
    pub video_quality: VideoQuality,
    /// Freeze the screen (one screenshot per monitor) before the selection overlay instead of drawing
    /// the crosshairs over the live screen and capturing afterwards.
    pub freeze_screen_during_capture: bool,
    /// Format a finished recording is saved as: "GIF" (default) or "MP4". Save As offers both.
    pub default_video_format: String,
    /// Last used "Create Animated GIF" settings.
    pub gif_export: GifExportSettings,
    /// MP4 quality (libx264 CRF; 18 = visually lossless).
    pub mp4_crf: u8,
    pub allow_hotkey_override: bool,
    /// Capture hotkeys other than the global capture key (which lives with the presets).
    pub hotkey_show_hide: crate::presets::HotKey,
    pub hotkey_repeat_last: crate::presets::HotKey,
    pub hotkey_video_start_pause: crate::presets::HotKey,
    pub hotkey_video_stop: crate::presets::HotKey,
    pub hotkey_draw_mode: crate::presets::HotKey,
    /// Mute / unmute the microphone while recording.
    pub hotkey_mute_mic: crate::presets::HotKey,

    // Editor
    pub editor_workspace_color: crate::Argb,
    pub tray_thumbnail_size: TrayThumbnailSize,
    pub auto_expand_canvas: bool,
    pub escape_closes_editor: bool,
    pub show_quickstyle_popup: bool,
    pub show_toolbar_labels: bool,
    pub spellcheck_enabled: bool,
    pub spellcheck_language: String,
    pub canvas_snapping: bool,
    pub shrink_to_fit_new_documents: bool,
    pub last_canvas_color: crate::Argb,
    pub new_image_width: u32,
    pub new_image_height: u32,
    pub new_image_color: crate::Argb,
    pub toolbar: Vec<ToolBarItem>,
    pub current_theme_id: String,
    pub side_panel_visible: bool,
    pub effects_panel: bool,
    pub tray_visible: bool,
    pub quick_styles_height: f32,
    pub quick_effects_height: f32,

    // Share
    pub disable_contextual_file_name: bool,
    pub html_clipboard: bool,
    pub disable_clipboard_transparency: bool,
    pub embed_images_in_email: bool,
    pub remember_last_save_format: bool,
    pub default_save_format: String,
    pub last_save_format: String,
    pub last_save_folder: Option<PathBuf>,
    pub drag_drop_file_type: String,
    pub enabled_outputs: Vec<String>,

    // Library
    pub library_directory: Option<PathBuf>,
    pub tray_max_items: u32,
    pub auto_save_images: bool,
    pub auto_save_videos: bool,
    pub prompt_for_delete: bool,
    pub remove_unpinned_on_close: bool,

    // Advanced
    pub keep_editor_running: bool,
    pub delete_cropped_content: bool,
    pub auto_flatten_paste: bool,

    // Window state
    pub capture_window_pos: Option<(f32, f32)>,
    pub editor_window_rect: Option<(f32, f32, f32, f32)>,
    pub capture_count: u64,
}

impl Default for Settings {
    fn default() -> Self {
        use crate::presets::HotKey;
        Self {
            ui_theme: UiTheme::Dark,
            theme_generation: 2,
            language: "en-US".into(),
            run_at_startup: false,
            default_tool_prompted: false,
            system_print_screen: false,
            ui_scale: 1.0,
            capture_interface: CaptureInterface::CaptureWindow,
            auto_check_updates: false,
            show_onboarding: true,
            tooltip_mode: TooltipMode::TextOnly,
            hide_linkshot_during_capture: true,
            open_system_screenshots: false,
            show_magnifier: true,
            simplified_crosshairs: false,
            require_start_button_for_scrolling: false,
            remote_desktop_token: None,
            screencast_token: None,
            screencast_token_cursor: None,
            ocr_language: "eng".into(),
            ocr_prefer_builtin: false,
            mic_muted_alert: true,
            hdr_capture: false,
            auto_start_recording: false,
            show_video_countdown: true,
            show_ready_to_record: true,
            minimize_fullscreen_controls: true,
            scale_video_to_1080p: false,
            enable_webcam: true,
            disable_hw_recording: false,
            video_quality: VideoQuality::High,
            freeze_screen_during_capture: false,
            default_video_format: "GIF".into(),
            gif_export: GifExportSettings::default(),
            mp4_crf: 18,
            allow_hotkey_override: true,
            hotkey_show_hide: HotKey { key: "X".into(), ctrl: true, shift: true, ..Default::default() },
            hotkey_repeat_last: HotKey::none(),
            hotkey_video_start_pause: HotKey { key: "F9".into(), shift: true, ..Default::default() },
            hotkey_video_stop: HotKey { key: "F10".into(), shift: true, ..Default::default() },
            hotkey_draw_mode: HotKey { key: "D".into(), ctrl: true, shift: true, ..Default::default() },
            hotkey_mute_mic: HotKey { key: "M".into(), ctrl: true, shift: true, ..Default::default() },
            editor_workspace_color: crate::Argb::rgb(0x1C, 0x19, 0x15),
            tray_thumbnail_size: TrayThumbnailSize::Medium,
            auto_expand_canvas: true,
            escape_closes_editor: false,
            show_quickstyle_popup: false,
            show_toolbar_labels: true,
            spellcheck_enabled: true,
            spellcheck_language: "en-us".into(),
            canvas_snapping: true,
            shrink_to_fit_new_documents: true,
            last_canvas_color: crate::Argb::WHITE,
            new_image_width: 300,
            new_image_height: 200,
            new_image_color: crate::Argb::WHITE,
            toolbar: ToolBarItem::default_toolbar(),
            current_theme_id: "dda8f802-448a-4b75-a7e9-25e24bf1a749".into(),
            side_panel_visible: true,
            effects_panel: false,
            tray_visible: true,
            quick_styles_height: 253.0,
            quick_effects_height: 132.0,
            disable_contextual_file_name: false,
            html_clipboard: false,
            disable_clipboard_transparency: false,
            embed_images_in_email: true,
            remember_last_save_format: true,
            default_save_format: "PNG".into(),
            last_save_format: "PNG".into(),
            last_save_folder: None,
            drag_drop_file_type: "png".into(),
            enabled_outputs: vec![
                crate::enums::output_ids::FILE.into(), crate::enums::output_ids::CLIPBOARD.into(),
                crate::enums::output_ids::EMAIL.into(), crate::enums::output_ids::PROGRAM.into(),
                crate::enums::output_ids::PRINT.into(), crate::enums::output_ids::FTP.into(),
            ],
            library_directory: None,
            tray_max_items: 25,
            auto_save_images: true,
            auto_save_videos: true,
            prompt_for_delete: true,
            remove_unpinned_on_close: false,
            keep_editor_running: true,
            delete_cropped_content: true,
            auto_flatten_paste: false,
            capture_window_pos: None,
            editor_window_rect: None,
            capture_count: 0,
        }
    }
}

/// Standard locations (XDG on Linux, %LOCALAPPDATA% on Windows, ~/Library on macOS).
#[derive(Clone, Debug)]
pub struct Paths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub default_library_dir: PathBuf,
}

impl Paths {
    pub fn discover() -> Paths {
        let proj = directories::ProjectDirs::from("dev", "linkshot", "linkshot");
        let user = directories::UserDirs::new();
        let (config_dir, data_dir, cache_dir) = match &proj {
            Some(p) => (p.config_dir().to_path_buf(), p.data_dir().to_path_buf(), p.cache_dir().to_path_buf()),
            None => (PathBuf::from(".linkshot"), PathBuf::from(".linkshot"), PathBuf::from(".linkshot/cache")),
        };
        let docs = user.as_ref().and_then(|u| u.document_dir().map(Path::to_path_buf)).unwrap_or_else(|| PathBuf::from("."));
        Paths { config_dir, data_dir, cache_dir, default_library_dir: docs.join("LinkShot") }
    }
    pub fn settings_file(&self) -> PathBuf { self.config_dir.join("settings.json") }
    pub fn presets_file(&self) -> PathBuf { self.config_dir.join("presets.json") }
    pub fn tools_file(&self) -> PathBuf { self.config_dir.join("tools.json") }
    pub fn tray_file(&self) -> PathBuf { self.data_dir.join("tray.json") }
    pub fn themes_dir(&self) -> PathBuf { self.data_dir.join("themes") }
    pub fn palettes_dir(&self) -> PathBuf { self.data_dir.join("palettes") }
    pub fn stamps_dir(&self) -> PathBuf { self.data_dir.join("stamps") }
    pub fn thumbnails_dir(&self) -> PathBuf { self.cache_dir.join("thumbnails") }
    pub fn library_db(&self) -> PathBuf { self.data_dir.join("library.sqlite") }
    /// Lossless masters (`.lsv`) of recordings, kept next to the saved GIF/MP4 so a later export
    /// does not re-encode a lossy file.
    pub fn recordings_dir(&self) -> PathBuf { self.data_dir.join("recordings") }
    /// Decoded spools of opened GIF/MP4 files.
    pub fn video_cache_dir(&self) -> PathBuf { self.cache_dir.join("video") }
    pub fn ensure(&self) -> std::io::Result<()> {
        for d in [&self.config_dir, &self.data_dir, &self.cache_dir, &self.themes_dir(), &self.palettes_dir(), &self.stamps_dir(), &self.thumbnails_dir()] {
            std::fs::create_dir_all(d)?;
        }
        Ok(())
    }
}

/// Writes `bytes` to `path` through a temporary file and a rename, so a crash never leaves a
/// half-written settings or presets file behind.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    if let Some(p) = path.parent() { std::fs::create_dir_all(p)?; }
    let tmp = path.with_extension(format!("{}.tmp-{}", path.extension().and_then(|e| e.to_str()).unwrap_or("json"), std::process::id()));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// A file that failed to parse is kept aside as `<name>.broken-<unix time>` instead of being
/// overwritten by the next autosave; the user keeps a chance to recover it.
pub fn keep_broken(path: &Path, error: &str) {
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let aside = path.with_extension(format!("{}.broken-{stamp}", path.extension().and_then(|e| e.to_str()).unwrap_or("json")));
    tracing::warn!("{} could not be parsed ({error}); keeping it as {} and starting from defaults", path.display(), aside.display());
    let _ = std::fs::rename(path, &aside);
}

impl Settings {
    pub fn load(path: &Path) -> Settings {
        let mut s: Settings = match std::fs::read(path) {
            Ok(b) => match serde_json::from_slice(&b) { Ok(s) => s, Err(e) => { keep_broken(path, &e.to_string()); Settings::default() } },
            Err(_) => Settings::default(),
        };
        if s.theme_generation < 2 { s.ui_theme = UiTheme::Dark; s.theme_generation = 2; }
        s
    }
    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        write_atomic(path, serde_json::to_string_pretty(self)?.as_bytes())
    }
    pub fn library_dir(&self, paths: &Paths) -> PathBuf {
        self.library_directory.clone().unwrap_or_else(|| paths.default_library_dir.clone())
    }
}
