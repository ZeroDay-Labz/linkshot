//! The Editor window: caption/menu row, top panel with toolbar, canvas, side panel, footer, tray.

pub mod canvas;
pub mod side_panel;
pub mod stamps;
pub mod library_thumbs;
pub mod tools;
pub mod tray;
pub mod video_view;

use crate::app::{Action, App};
use crate::icons;
use crate::widgets;
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2};
use linkshot_core::document::Document;
use linkshot_core::enums::*;
use linkshot_core::presets::CaptureSettings;
use linkshot_core::settings::{Paths, Settings};
use linkshot_core::style::{QuickStyle, Theme, ToolBarItem};
use linkshot_core::undo::{SnapshotCommand, UndoStack};
use linkshot_render::Renderer;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

pub struct OpenDoc {
    pub doc: Document,
    pub version: u64,
    pub saved_version: u64,
    pub undo: UndoStack<Document>,
    pub thumb: Option<egui::TextureHandle>,
    pub thumb_version: u64,
    pub pinned: bool,
    pub kind: TrayItemType,
    pub opened: Instant,
    /// Last edit/undo/redo; the tray thumbnail is refreshed once edits pause.
    pub changed_at: Instant,
    /// Set for GIF/MP4 documents: the player and export source (`doc` holds the poster frame).
    pub video: Option<video_view::VideoDoc>,
    /// Name and time of the last edit, so a slider gesture collapses into one undo step.
    pub last_edit: Option<(String, Instant)>,
}

impl OpenDoc {
    pub fn new(doc: Document) -> Self {
        Self { doc, version: 1, saved_version: 0, undo: UndoStack::default(), thumb: None, thumb_version: 0, pinned: false, kind: TrayItemType::Image, opened: Instant::now(), changed_at: Instant::now(), video: None, last_edit: None }
    }
    pub fn is_dirty(&self) -> bool { self.version != self.saved_version }
    pub fn is_video(&self) -> bool { self.video.is_some() }
    /// Applies a document edit as an undoable step.
    pub fn edit(&mut self, name: &str, f: impl FnOnce(&mut Document)) {
        // Property sliders and colour pickers call this every frame; consecutive edits of the same
        // kind within a short time are merged into one undo step (they set absolute values).
        let coalesce = matches!(name, "Change Style" | "Canvas Color" | "Change Effect")
            && self.last_edit.as_ref().map(|(n, t)| n == name && t.elapsed() < std::time::Duration::from_millis(800)).unwrap_or(false);
        if coalesce { self.undo.undo(&mut self.doc); self.undo.clear_redo(); }
        self.last_edit = Some((name.to_string(), Instant::now()));
        let before = self.doc.clone();
        let mut after = self.doc.clone();
        f(&mut after);
        if after.pages == before.pages && after.blobs == before.blobs && after.metadata == before.metadata { return; }
        self.undo.push(Box::new(SnapshotCommand::new(name, before, after)), &mut self.doc);
        self.version += 1;
        self.changed_at = Instant::now();
    }
    pub fn undo(&mut self) { if self.undo.undo(&mut self.doc) { self.version += 1; self.changed_at = Instant::now(); } }
    pub fn redo(&mut self) { if self.undo.redo(&mut self.doc) { self.version += 1; self.changed_at = Instant::now(); } }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SidePanelMode { Properties, Effects }

pub struct EditorState {
    pub docs: Vec<OpenDoc>,
    pub current: Option<usize>,
    pub tool: ToolMode,
    pub last_tool: ToolMode,
    pub props: HashMap<ToolMode, QuickStyle>,
    pub themes: Vec<Theme>,
    pub current_theme: usize,
    pub toolbar: Vec<ToolBarItem>,
    pub more_tool: Option<ToolMode>,
    pub renderer: Renderer,
    pub stamps: stamps::StampLibrary,
    pub canvas: canvas::CanvasState,
    pub tools: tools::ToolState,
    pub side_panel: SidePanelMode,
    pub side_panel_visible: bool,
    pub tray_visible: bool,
    pub library_view: bool,
    pub focus_requested: bool,
    pub pending_share: Option<CaptureSettings>,
    pub status: Option<(String, Instant)>,
    pub show_customize_toolbar: bool,
    pub tag_popup: bool,
    pub zoom_popup: bool,
    pub size_popup: bool,
    pub resize_w: String,
    pub resize_h: String,
    pub resize_lock: bool,
    pub library: linkshot_core::library::LibraryIndex,
    pub library_scanned: bool,
    pub library_thumbs: Option<library_thumbs::LibraryThumbs>,
    pub library_scan: Option<library_thumbs::ScanJob>,
    pub library_filter: String,
    pub selected_library: Vec<PathBuf>,
    pub new_image_dialog: bool,
    pub new_w: String,
    pub new_h: String,
    pub canvas_color_dialog: bool,
    pub tray_dirty: bool,
    pub gif_dialog: Option<video_view::GifDialog>,
    pub export_job: Option<video_view::ExportJob>,
    /// Wakes the UI loop from worker threads (players, exports).
    pub wake: std::sync::Arc<dyn Fn() + Send + Sync>,
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct TrayFile { items: Vec<TrayEntry> }
#[derive(serde::Serialize, serde::Deserialize)]
struct TrayEntry { path: PathBuf, pinned: bool }

impl EditorState {
    pub fn new(_paths: &Paths, settings: &Settings, themes: &[Theme]) -> Self {
        let mut props = HashMap::new();
        for t in ToolMode::ALL { props.insert(*t, QuickStyle::for_tool(*t)); }
        let current_theme = themes.iter().position(|t| t.id == settings.current_theme_id).unwrap_or(0);
        // Seed tool properties with the theme's default quick style for each tool.
        if let Some(theme) = themes.get(current_theme) {
            for t in ToolMode::ALL { if let Some(q) = theme.styles_for(*t).next() { props.insert(*t, q.clone()); } }
        }
        Self {
            docs: vec![],
            current: None,
            tool: ToolMode::Arrow,
            last_tool: ToolMode::Arrow,
            props,
            themes: themes.to_vec(),
            current_theme,
            toolbar: if settings.toolbar.is_empty() { ToolBarItem::default_toolbar() } else { settings.toolbar.clone() },
            stamps: Default::default(),
            more_tool: None,
            renderer: Renderer::new(),
            canvas: canvas::CanvasState::default(),
            tools: tools::ToolState::default(),
            side_panel: if settings.effects_panel { SidePanelMode::Effects } else { SidePanelMode::Properties },
            side_panel_visible: settings.side_panel_visible,
            tray_visible: settings.tray_visible,
            library_view: false,
            focus_requested: false,
            pending_share: None,
            status: None,
            show_customize_toolbar: false,
            tag_popup: false,
            zoom_popup: false,
            size_popup: false,
            resize_w: String::new(),
            resize_h: String::new(),
            resize_lock: true,
            library: linkshot_core::library::LibraryIndex::default(),
            library_scanned: false,
            library_thumbs: None,
            library_scan: None,
            library_filter: String::new(),
            selected_library: vec![],
            new_image_dialog: false,
            new_w: settings.new_image_width.to_string(),
            new_h: settings.new_image_height.to_string(),
            canvas_color_dialog: false,
            tray_dirty: false,
            gif_dialog: None,
            export_job: None,
            wake: std::sync::Arc::new(|| {}),
        }
    }

    pub fn set_status(&mut self, s: impl Into<String>) { self.status = Some((s.into(), Instant::now())); }

    pub fn theme(&self) -> &Theme { &self.themes[self.current_theme.min(self.themes.len().saturating_sub(1))] }

    pub fn current_doc(&self) -> Option<&OpenDoc> { self.current.and_then(|i| self.docs.get(i)) }
    pub fn current_doc_mut(&mut self) -> Option<&mut OpenDoc> { self.current.and_then(move |i| self.docs.get_mut(i)) }

    /// Loads the tray from disk (recent captures still present in the library).
    pub fn load_tray(&mut self, paths: &Paths, settings: &Settings) {
        let f: TrayFile = std::fs::read(paths.tray_file()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        for e in f.items.into_iter().take(settings.tray_max_items as usize) {
            if let Ok(doc) = crate::share::open_file(&e.path) {
                let mut od = self.wrap_doc(doc, paths);
                od.saved_version = od.version;
                od.pinned = e.pinned;
                self.docs.push(od);
            }
        }
        if !self.docs.is_empty() { self.current = Some(self.docs.len() - 1); }
    }

    pub fn persist(&mut self, paths: &Paths) {
        if !self.tray_dirty { return; }
        let items: Vec<TrayEntry> = self.docs.iter().filter_map(|d| d.doc.path.clone().map(|p| TrayEntry { path: p, pinned: d.pinned })).collect();
        let _ = std::fs::write(paths.tray_file(), serde_json::to_vec(&TrayFile { items }).unwrap_or_default());
        self.tray_dirty = false;
    }

    /// Wraps a document for the tray; GIF/MP4 posters (see `share::open_file`) become video documents.
    pub fn wrap_doc(&self, doc: Document, paths: &Paths) -> OpenDoc {
        let mut od = OpenDoc::new(doc);
        if let Some(file) = od.doc.metadata.extra.get("VideoFile").and_then(|v| v.as_str()).map(PathBuf::from) {
            let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
            od.kind = if ext == "gif" { TrayItemType::AnimatedGif } else { TrayItemType::Video };
            let master = video_view::master_for(&file, paths);
            // Decoding starts when the item is first shown, not for every video in the tray at start-up.
            od.video = Some(video_view::VideoDoc::deferred(file, master, paths.video_cache_dir(), self.wake.clone()));
        }
        od
    }

    /// Adds a finished recording (its lossless spool) to the tray and, with "auto-save videos" on,
    /// starts saving it to the library in the default video format.
    pub fn add_recording(&mut self, spool: PathBuf, paths: &Paths, settings: &Settings) -> anyhow::Result<()> {
        use linkshot_video::FrameSource as _;
        let mut reader = linkshot_video::SpoolReader::open(&spool)?;
        let (w, h) = (reader.width(), reader.height());
        let first = reader.frame(0)?.to_vec();
        let poster = image::RgbaImage::from_raw(w, h, first).ok_or_else(|| anyhow::anyhow!("poster"))?;
        let png = linkshot_render::encode_png(&poster)?;
        drop(reader);
        let mut doc = Document::from_png(w, h, png);
        doc.metadata.extra.insert("Application".into(), serde_json::Value::String(String::new()));
        let mut od = OpenDoc::new(doc);
        od.saved_version = od.version;
        let gif = settings.default_video_format.eq_ignore_ascii_case("gif");
        od.kind = if gif { TrayItemType::AnimatedGif } else { TrayItemType::Video };
        od.video = Some(video_view::VideoDoc::from_spool(spool.clone(), None, self.wake.clone())?);
        self.docs.push(od);
        self.current = Some(self.docs.len() - 1);
        self.canvas.fit_requested = true;
        self.tools.clear_selection();
        self.library_view = false;
        self.tray_dirty = true;
        if settings.auto_save_videos {
            let dir = settings.library_dir(paths);
            let _ = std::fs::create_dir_all(&dir);
            let ext = if gif { "gif" } else { "mp4" };
            let target = linkshot_core::naming::library_file_name(&dir, chrono::Local::now(), ext);
            // Keep the master next to the file name so later exports stay lossless.
            if let Some(stem) = target.file_stem() {
                let master = paths.recordings_dir().join(format!("{}.lsv", stem.to_string_lossy()));
                if std::fs::rename(&spool, &master).is_ok() {
                    let (wav, master_wav) = (spool.with_extension("wav"), master.with_extension("wav"));
                    let has_audio = wav.is_file() && std::fs::rename(&wav, &master_wav).is_ok();
                    if let Some(v) = self.docs.last_mut().unwrap().video.as_mut() { v.master = Some(master.clone()); if has_audio { v.audio = Some(master_wav); } }
                    let format = if gif { video_view::ExportFormat::Gif(video_view::gif_options(&settings.gif_export, (None, None))) } else { video_view::ExportFormat::Mp4(video_view::mp4_options(settings, (None, None))) };
                    self.export_job = Some(video_view::start_export(master, target, format, true, self.wake.clone()));
                }
            }
        }
        Ok(())
    }

    /// Adds a document to the tray (newest on the right of pinned items), auto-saving it to the library.
    pub fn add_document(&mut self, _ctx: &egui::Context, mut doc: Document, paths: &Paths, settings: &Settings) {
        let is_video = doc.metadata.extra.contains_key("VideoFile");
        if settings.auto_save_images && doc.path.is_none() && !is_video { self.autosave(&mut doc, paths, settings); }
        let mut od = self.wrap_doc(doc, paths);
        od.saved_version = od.version;
        self.docs.push(od);
        // Enforce the tray limit on unpinned items (oldest first).
        while self.docs.len() > settings.tray_max_items.max(1) as usize {
            if let Some(i) = self.docs.iter().position(|d| !d.pinned) { self.docs.remove(i); } else { break; }
        }
        self.current = Some(self.docs.len() - 1);
        self.canvas.fit_requested = true;
        self.tools.clear_selection();
        self.library_view = false;
        self.tray_dirty = true;
    }

    pub fn autosave(&mut self, doc: &mut Document, paths: &Paths, settings: &Settings) {
        let dir = settings.library_dir(paths);
        let _ = std::fs::create_dir_all(&dir);
        let path = match &doc.path { Some(p) if p.extension().map(|e| e == "snagx").unwrap_or(false) => p.clone(), _ => linkshot_core::naming::library_file_name(&dir, chrono::Local::now(), "snagx") };
        if doc.thumbnail_png.is_none() { if let Some(t) = self.renderer.thumbnail(doc, doc.page(), 320, 180) { doc.thumbnail_png = linkshot_render::encode_png(&t).ok(); } }
        match linkshot_core::snagx::write_file(doc, &path) { Ok(()) => doc.path = Some(path), Err(e) => tracing::warn!("autosave failed: {e}") }
    }

    pub fn save_current(&mut self, paths: &Paths, settings: &Settings) {
        let Some(i) = self.current else { return };
        if self.docs[i].is_video() {
            if self.docs[i].doc.path.is_some() { self.set_status("Videos are saved when exported (Create GIF / Save as MP4)."); } else { self.set_status("Use Create GIF or Save as MP4 to save the recording."); }
            return;
        }
        let renderer = &mut self.renderer;
        let od = &mut self.docs[i];
        let path = match od.doc.path.clone() { Some(p) => p, None => settings.library_dir(paths).join(linkshot_core::naming::library_file_name(&settings.library_dir(paths), chrono::Local::now(), "snagx")) };
        if let Some(t) = renderer.thumbnail(&od.doc, od.doc.page(), 320, 180) { od.doc.thumbnail_png = linkshot_render::encode_png(&t).ok(); }
        match crate::share::export(renderer, &od.doc, &path, 90) {
            Ok(()) => { od.doc.path = Some(path); od.saved_version = od.version; self.tray_dirty = true; self.set_status("Saved."); }
            Err(e) => self.set_status(format!("Save failed: {e:#}")),
        }
    }

    /// Save As for the current document. Video documents are handled by the caller (see
    /// `video_view::save_video_as`), which needs the whole `App`; this returns false for them.
    pub fn save_as(&mut self, paths: &Paths, settings: &mut Settings) -> bool {
        let Some(i) = self.current else { return true };
        if self.docs[i].is_video() { return false; }
        let default_dir = settings.last_save_folder.clone().unwrap_or_else(|| settings.library_dir(paths));
        let stem = self.docs[i].doc.display_name();
        let ext = if settings.remember_last_save_format { settings.last_save_format.to_ascii_lowercase() } else { settings.default_save_format.to_ascii_lowercase() };
        let Some(path) = crate::share::save_as_dialog(Some(&default_dir), &stem, &ext) else { return true };
        let renderer = &mut self.renderer;
        let od = &mut self.docs[i];
        match crate::share::export(renderer, &od.doc, &path, 90) {
            Ok(()) => {
                settings.last_save_folder = path.parent().map(|p| p.to_path_buf());
                if let Some(e) = path.extension().and_then(|e| e.to_str()) { settings.last_save_format = e.to_ascii_uppercase(); }
                // Saving as another format puts both files in the tray; the snagx stays editable.
                if path.extension().map(|e| e == "snagx").unwrap_or(false) { od.doc.path = Some(path); od.saved_version = od.version; }
                else if let Ok(doc) = crate::share::open_file(&path) { let mut nd = OpenDoc::new(doc); nd.saved_version = nd.version; self.docs.push(nd); self.current = Some(self.docs.len() - 1); }
                self.tray_dirty = true;
                self.set_status("Saved.");
            }
            Err(e) => self.set_status(format!("Save failed: {e:#}")),
        }
        true
    }

    /// Shares a finished capture to its destinations (no preview, or after "Finish").
    pub fn share_now(&mut self, doc: &mut Document, settings: &CaptureSettings, paths: &Paths, app_settings: &Settings) {
        for id in settings.share_destination_ids.clone() {
            match id.as_str() {
                output_ids::NONE => {}
                output_ids::CLIPBOARD => { let _ = crate::share::copy_all(&mut self.renderer, doc); }
                output_ids::FILE => {
                    let f = &settings.share_settings_file;
                    let dir = if f.use_last_folder || f.folder_name.is_empty() { app_settings.last_save_folder.clone().unwrap_or_else(|| app_settings.library_dir(paths)) } else { PathBuf::from(&f.folder_name) };
                    let ext = match settings.share_settings_file_save.output_file_type.as_str() { "LastUsedSaveOptions" => app_settings.last_save_format.to_ascii_lowercase(), other => other.to_ascii_lowercase().replace("jpeg", "jpg") };
                    let path = if f.file_output_fixed { dir.join(format!("{}.{}", f.fixed_name, ext)) }
                        else if f.file_output_auto_name { linkshot_core::naming::next_auto_name(&dir, &settings.share_auto_name_settings, &ext, &Default::default()) }
                        else { match crate::share::save_as_dialog(Some(&dir), &doc.display_name(), &ext) { Some(p) => p, None => continue } };
                    if let Err(e) = crate::share::export(&mut self.renderer, doc, &path, 90) { self.set_status(format!("Share to File failed: {e:#}")); } else { self.set_status(format!("Saved {}", path.display())); }
                }
                output_ids::PROGRAM => {
                    let p = &settings.share_settings_program;
                    match crate::share::temp_png(&mut self.renderer, doc, &app_settings.library_dir(paths).join("Program"), &doc.display_name()) {
                        Ok(path) => { if p.executable.is_empty() { let _ = linkshot_platform::open_path(&path); } else if let Err(e) = linkshot_platform::run_program(&p.executable, &p.parameters, &path) { self.set_status(format!("Program failed: {e}")); } }
                        Err(e) => self.set_status(format!("Share failed: {e:#}")),
                    }
                }
                output_ids::EMAIL => {
                    let e = &settings.share_settings_email;
                    if let Ok(path) = crate::share::temp_png(&mut self.renderer, doc, &app_settings.library_dir(paths).join("Program"), &doc.display_name()) {
                        let body = if e.message_text.is_empty() { format!("Attached: {}", path.display()) } else { format!("{}\n\n{}", e.message_text, path.display()) };
                        let _ = linkshot_platform::compose_email(&e.address, &e.subject, &body);
                        let _ = linkshot_platform::reveal(&path);
                    }
                }
                output_ids::PRINT => {
                    match crate::share::temp_png(&mut self.renderer, doc, &paths.cache_dir.join("print"), &doc.display_name()) {
                        Ok(path) => match linkshot_platform::print_file(&path) { Ok(()) => self.set_status("Sent to the printer."), Err(e) => self.set_status(format!("Print failed: {e:#}")) },
                        Err(e) => self.set_status(format!("Print failed: {e:#}")),
                    }
                }
                other => { self.set_status(format!("Share destination {} is not available yet.", output_ids::title(other))); }
            }
        }
    }

    pub fn apply_capture_effect(&mut self, doc: &mut Document, e: &linkshot_core::presets::EffectSettings) {
        let Some(img) = self.renderer.flatten(doc, doc.page()) else { return };
        let out = match e.effect_type {
            EffectType::EffectBorder => linkshot_render::effects::border(&img, e.get_color("Color", linkshot_core::Argb::BLACK), e.get_u64("Width", 4).clamp(1, 200) as u32),
            EffectType::EffectShadow => {
                let dir = match e.get_str("Direction", "BottomRight").as_str() { "BottomLeft" => (-1, 1), "TopRight" => (1, -1), "TopLeft" => (-1, -1), "Center" => (0, 0), _ => (1, 1) };
                linkshot_render::effects::shadow(&img, e.get_color("Color", linkshot_core::Argb::BLACK), e.get_u64("Size", 12).clamp(1, 200) as u32, dir)
            }
            EffectType::EffectFilter => match e.get_str("Filter", "Grayscale").as_str() {
                "Invert" => linkshot_render::effects::invert(&img),
                "Monochrome" => linkshot_render::effects::monochrome(&img, 128),
                "Soften" => linkshot_render::effects::soften(&img, 2.0),
                "Sharpen" => linkshot_render::effects::sharpen(&img, 2.0),
                _ => linkshot_render::effects::grayscale(&img),
            },
            EffectType::EffectTrim => linkshot_render::effects::auto_trim(&img, e.get_u64("Tolerance", 8).min(255) as u8),
            EffectType::EffectEdge => {
                let kind = e.get_str("EdgeType", "Torn").parse::<EdgeType>().unwrap_or(EdgeType::Torn);
                linkshot_render::effects::edge(&img, kind, e.get_u64("Size", 18).clamp(2, 200) as u32, e.get_bool("Top", false), e.get_bool("Bottom", true), e.get_bool("Left", false), e.get_bool("Right", true))
            }
            _ => return,
        };
        self.replace_background(doc, out);
    }

    /// Replaces the document's flattened background with `img` and clears vector objects.
    pub fn replace_background(&mut self, doc: &mut Document, img: image::RgbaImage) {
        let Ok(png) = linkshot_render::encode_png(&img) else { return };
        let page = doc.page_mut();
        page.capture_canvas_width = img.width() as f64;
        page.capture_canvas_height = img.height() as f64;
        page.capture_background_location = linkshot_core::Pt::ZERO;
        page.capture_objects.clear();
        let name = page.capture_background_image.clone();
        doc.blobs.insert(name, std::sync::Arc::new(png));
        self.renderer.invalidate();
    }

    /// Scans the library folder on a worker thread (metadata only); the view updates when it is done.
    pub fn start_library_scan(&mut self, dir: PathBuf) {
        if self.library_scan.is_some() { return; }
        self.library_scanned = true;
        self.library_scan = Some(library_thumbs::ScanJob::start(self.library.clone(), dir, self.wake.clone()));
    }

    /// Picks up a finished scan.
    pub fn poll_library_scan(&mut self) {
        if let Some(idx) = self.library_scan.as_ref().and_then(|j| j.poll()) {
            self.library = idx;
            self.library_scan = None;
            if let Some(t) = self.library_thumbs.as_mut() { t.retain(&self.library); }
        }
    }

    pub fn current_props(&self) -> QuickStyle {
        self.props.get(&self.tool).cloned().unwrap_or_else(|| QuickStyle::for_tool(self.tool))
    }

    /// Undo on the current document; the renderer caches the background by blob name, so a raster
    /// undo (crop, resize, effect) needs the cache dropped to show the restored pixels.
    pub fn undo_current(&mut self) {
        if let Some(d) = self.current_doc_mut() { d.undo(); d.last_edit = None; }
        self.renderer.invalidate();
        self.canvas.invalidate();
        self.tools.clear_selection();
    }
    pub fn redo_current(&mut self) {
        if let Some(d) = self.current_doc_mut() { d.redo(); d.last_edit = None; }
        self.renderer.invalidate();
        self.canvas.invalidate();
    }

    pub fn select_tool(&mut self, t: ToolMode) {
        if t == ToolMode::Favorites || t == ToolMode::None { self.tool = t; return; }
        if matches!(t, ToolMode::Eraser | ToolMode::MagicWand | ToolMode::SimplifiedUI) {
            self.set_status(format!("{} is not available yet", t.label()));
            return;
        }
        if self.tool != t { self.last_tool = self.tool; }
        self.tool = t;
        self.tools.on_tool_changed();
        if !self.toolbar.iter().any(|i| i.item_type == ToolBarItemType::Tool && i.tool_mode == t) { self.more_tool = Some(t); }
        self.side_panel = SidePanelMode::Properties;
    }
}

const CAPTION_H: f32 = 26.0;
const TOP_PANEL_H: f32 = 72.0;
const FOOTER_H: f32 = 38.0;
const SIDE_W: f32 = 260.0;

pub fn editor_ui(app: &mut App, ctx: &egui::Context, actions: &mut Vec<Action>) {
    let pal = app.pal;
    // Keyboard shortcuts
    handle_shortcuts(app, ctx, actions);
    // Files dropped on the window open as new documents.
    let dropped: Vec<std::path::PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
    for path in dropped {
        match crate::share::open_file(&path) { Ok(doc) => app.editor.add_document(ctx, doc, &app.paths, &app.settings), Err(e) => app.editor.set_status(format!("Could not open {}: {e:#}", path.display())) }
    }

    egui::CentralPanel::default().frame(egui::Frame::NONE.fill(pal.main_panel)).show(ctx, |ui| {
        let full = ui.max_rect();
        // ---- caption row ----
        let caption = Rect::from_min_size(full.min, Vec2::new(full.width(), CAPTION_H));
        caption_row(app, ui, caption, ctx, actions);
        // ---- top panel ----
        let top = Rect::from_min_size(Pos2::new(full.left(), caption.bottom()), Vec2::new(full.width(), TOP_PANEL_H));
        top_panel(app, ui, top, actions);
        ui.painter().hline(full.x_range(), top.bottom(), Stroke::new(1.0_f32, pal.separator));
        ui.painter().rect_filled(Rect::from_min_size(Pos2::new(full.left(), top.bottom() + 1.0), Vec2::new(full.width(), 6.0)), 0.0, Color32::from_black_alpha(if pal.is_dark { 40 } else { 14 }));
        // ---- tray / footer ----
        let (tw, th) = app.settings.tray_thumbnail_size.pixels();
        let _ = tw;
        let tray_h = if app.editor.tray_visible { th + 20.0 } else { 0.0 };
        let tray_rect = Rect::from_min_max(Pos2::new(full.left(), full.bottom() - tray_h), full.max);
        let footer = Rect::from_min_max(Pos2::new(full.left(), tray_rect.top() - FOOTER_H), Pos2::new(full.right(), tray_rect.top()));
        // ---- centre: canvas + side panel ----
        let centre = Rect::from_min_max(Pos2::new(full.left(), top.bottom() + 1.0), Pos2::new(full.right(), footer.top()));
        let side_visible = app.editor.side_panel_visible && !app.editor.library_view;
        let side = Rect::from_min_max(Pos2::new(centre.right() - if side_visible { SIDE_W } else { 0.0 }, centre.top()), centre.max);
        let canvas_rect = Rect::from_min_max(centre.min, Pos2::new(side.left(), centre.bottom()));
        if app.editor.library_view {
            library_view(app, ui, canvas_rect, ctx);
        } else if app.editor.current_doc().map(|d| d.is_video()).unwrap_or(false) {
            video_view::video_canvas_ui(app, ui, canvas_rect, ctx);
        } else {
            canvas::canvas_ui(app, ui, canvas_rect, ctx);
        }
        video_view::export_progress_ui(app, ui, canvas_rect, ctx);
        if side_visible {
            ui.painter().rect_filled(side, 0.0, pal.main_panel);
            ui.painter().vline(side.left(), side.y_range(), Stroke::new(1.0_f32, pal.separator));
            side_panel::side_panel_ui(app, ui, side);
        }
        footer_ui(app, ui, footer, ctx, actions);
        if app.editor.tray_visible { tray::tray_ui(app, ui, tray_rect, ctx); }
        // "Finish sharing capture" bar
        if let Some(settings) = app.editor.pending_share.clone() {
            let bar = Rect::from_center_size(Pos2::new(footer.center().x, footer.top() - 40.0), Vec2::new(420.0, 44.0));
            ui.painter().rect_filled(bar, 3.0, pal.product);
            ui.scope_builder(egui::UiBuilder::new().max_rect(bar.shrink2(Vec2::new(12.0, 6.0))), |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(egui::RichText::new("Finish sharing capture").color(Color32::WHITE).size(13.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(egui::Button::new(egui::RichText::new("Cancel").color(Color32::WHITE)).fill(pal.product_dark)).clicked() { app.editor.pending_share = None; }
                        if ui.add(egui::Button::new(egui::RichText::new("Finish").color(pal.product)).fill(Color32::WHITE)).clicked() {
                            if let Some(i) = app.editor.current {
                                let mut doc = app.editor.docs[i].doc.clone();
                                let paths = app.paths.clone();
                                let s = app.settings.clone();
                                app.editor.share_now(&mut doc, &settings, &paths, &s);
                                app.editor.docs[i].doc = doc;
                            }
                            app.editor.pending_share = None;
                        }
                    });
                });
            });
        }
        // status toast
        let status = app.editor.status.clone().or_else(|| app.status.clone());
        if let Some((msg, t)) = status {
            if t.elapsed().as_secs_f32() < 4.0 {
                let galley = ui.painter().layout(msg.clone(), FontId::proportional(13.0), pal.text_bright, canvas_rect.width() - 60.0);
                let r = Rect::from_center_size(Pos2::new(canvas_rect.center().x, canvas_rect.bottom() - 34.0), galley.size() + Vec2::new(28.0, 14.0));
                ui.painter().rect_filled(r.translate(Vec2::new(0.0, 3.0)), 8.0, Color32::from_black_alpha(80));
                ui.painter().rect_filled(r, 8.0, pal.dark_panel);
                ui.painter().rect_stroke(r, 8.0, Stroke::new(1.0_f32, pal.control_outline), egui::StrokeKind::Inside);
                ui.painter().galley(r.min + Vec2::new(14.0, 7.0), galley, pal.text_bright);
                ctx.request_repaint_after(std::time::Duration::from_millis(500));
            } else { app.editor.status = None; }
        }
        // Last, so the edge strips win the hit test over whatever is drawn underneath them.
        resize_edges(ui, ctx, full);
    });
    dialogs(app, ctx, actions);
    video_view::gif_dialog_ui(app, ctx);
    video_view::poll_export(app, ctx);
}

/// Save As (Ctrl+Shift+S, menus): images through the format dialog, videos through GIF/MP4 export.
/// File ▸ Print: flattens the current image to a PNG in the cache and hands it to the system printer.
pub fn print_current(app: &mut App) {
    let Some(i) = app.editor.current else { return };
    if app.editor.docs[i].is_video() { app.editor.set_status("Videos cannot be printed; save a frame as an image first."); return; }
    let doc = app.editor.docs[i].doc.clone();
    let dir = app.paths.cache_dir.join("print");
    match crate::share::temp_png(&mut app.editor.renderer, &doc, &dir, &doc.display_name()) {
        Ok(path) => match linkshot_platform::print_file(&path) { Ok(()) => app.editor.set_status("Sent to the printer."), Err(e) => app.editor.set_status(format!("Print failed: {e:#}")) },
        Err(e) => app.editor.set_status(format!("Print failed: {e:#}")),
    }
}

pub fn save_as_current(app: &mut App) {
    let paths = app.paths.clone();
    if !app.editor.save_as(&paths, &mut app.settings) {
        if let Some(i) = app.editor.current {
            let ext = app.settings.default_video_format.to_ascii_lowercase();
            video_view::save_video_as(app, i, if ext == "mp4" { "mp4" } else { "gif" });
        }
    }
    app.mark_settings_dirty();
}

fn handle_shortcuts(app: &mut App, ctx: &egui::Context, actions: &mut Vec<Action>) {
    if ctx.wants_keyboard_input() && app.editor.tools.text_edit.is_none() { /* a text field has focus */ }
    if ctx.memory(|m| m.focused().is_some()) { return; }
    let (ctrl, shift, alt) = ctx.input(|i| (i.modifiers.command, i.modifiers.shift, i.modifiers.alt));
    let pressed = |k: egui::Key| ctx.input(|i| i.key_pressed(k));
    if ctrl && !shift && pressed(egui::Key::Z) { app.editor.undo_current(); }
    if ctrl && (pressed(egui::Key::Y) || (shift && pressed(egui::Key::Z))) { app.editor.redo_current(); }
    if ctrl && !shift && pressed(egui::Key::S) { app.editor.save_current(&app.paths, &app.settings); }
    if ctrl && shift && pressed(egui::Key::S) { save_as_current(app); }
    if ctrl && pressed(egui::Key::O) { if let Some(paths) = crate::share::open_dialog(None) { for p in paths { if let Ok(doc) = crate::share::open_file(&p) { app.editor.add_document(ctx, doc, &app.paths, &app.settings); } } } }
    if ctrl && shift && pressed(egui::Key::C) { if let Some(d) = app.editor.current_doc() { let doc = d.doc.clone(); let _ = crate::share::copy_all(&mut app.editor.renderer, &doc); app.editor.set_status("Copied to clipboard."); } }
    if ctrl && !shift && pressed(egui::Key::N) { app.editor.new_image_dialog = true; }
    if ctrl && shift && pressed(egui::Key::N) { if let Some(img) = linkshot_platform::clipboard::get_image() { if let Ok(png) = linkshot_render::encode_png(&img) { let doc = Document::from_png(img.width(), img.height(), png); app.editor.add_document(ctx, doc, &app.paths, &app.settings); } } }
    if ctrl && shift && pressed(egui::Key::T) { flatten_current(app); }
    if ctrl && !shift && pressed(egui::Key::Num0) { app.editor.canvas.zoom = 1.0; app.editor.canvas.fit = false; }
    if ctrl && !shift && (pressed(egui::Key::Plus) || pressed(egui::Key::Equals)) { app.editor.canvas.zoom_step(1); }
    if ctrl && !shift && pressed(egui::Key::Minus) { app.editor.canvas.zoom_step(-1); }
    // Interface scale (all windows): Ctrl+Shift +/-/0.
    if ctrl && shift && (pressed(egui::Key::Plus) || pressed(egui::Key::Equals)) { let z = app.settings.ui_scale + 0.1; app.set_ui_scale(ctx, z); }
    if ctrl && shift && pressed(egui::Key::Minus) { let z = app.settings.ui_scale - 0.1; app.set_ui_scale(ctx, z); }
    if ctrl && shift && pressed(egui::Key::Num0) { app.set_ui_scale(ctx, 1.0); }
    if ctrl && pressed(egui::Key::A) { if let Some(d) = app.editor.current_doc() { app.editor.tools.selection = d.doc.page().capture_objects.iter().map(|o| o.object_id.clone()).collect(); } }
    if ctrl && pressed(egui::Key::D) {
        // Ctrl+D: Deselect when something is selected, otherwise Duplicate the capture (as in the menus).
        if !app.editor.tools.selection.is_empty() { app.editor.tools.clear_selection(); }
        else if let Some(d) = app.editor.current_doc() { let mut doc = d.doc.clone(); doc.path = None; app.editor.add_document(ctx, doc, &app.paths, &app.settings); }
    }
    if ctrl && !shift && pressed(egui::Key::T) { tools::flatten_selection(app); }
    if ctrl && shift && pressed(egui::Key::F) { tools::reorder(app, 1); }
    if ctrl && shift && pressed(egui::Key::B) { tools::reorder(app, -1); }
    if ctrl && pressed(egui::Key::Semicolon) { app.settings.canvas_snapping = !app.settings.canvas_snapping; app.mark_settings_dirty(); }
    if ctrl && shift && pressed(egui::Key::P) { if let Some(d) = app.editor.current_doc_mut() { d.pinned = !d.pinned; app.editor.tray_dirty = true; } }
    if ctrl && shift && pressed(egui::Key::X) { actions.push(Action::ToggleCaptureWindow); }
    if ctrl && pressed(egui::Key::W) { tray::close_current(app); }
    if ctrl && shift && pressed(egui::Key::L) { app.editor.set_status("Share Link requires a cloud account; not available in this build."); }
    if alt && pressed(egui::Key::Enter) { app.editor.set_status(doc_details(app)); }
    // Ctrl+1..9 toolbar tools
    for (i, k) in [egui::Key::Num1, egui::Key::Num2, egui::Key::Num3, egui::Key::Num4, egui::Key::Num5, egui::Key::Num6, egui::Key::Num7, egui::Key::Num8, egui::Key::Num9].iter().enumerate() {
        if ctrl && pressed(*k) {
            let tools: Vec<ToolMode> = app.editor.toolbar.iter().filter(|t| t.item_type == ToolBarItemType::Tool).map(|t| t.tool_mode).collect();
            if let Some(t) = tools.get(i) { app.editor.select_tool(*t); }
        }
    }
    if pressed(egui::Key::Escape) && app.settings.escape_closes_editor && app.editor.tools.text_edit.is_none() && app.editor.tools.selection.is_empty() { ctx.send_viewport_cmd(egui::ViewportCommand::Close); }
}

/// Edit ▸ Grab Text: OCR of the current image (flattened with its annotations).
pub fn grab_text_current(app: &mut App) {
    let Some(d) = app.editor.current_doc() else { return };
    let doc = d.doc.clone();
    if let Some(img) = crate::share::flatten(&mut app.editor.renderer, &doc) { app.start_grab_text(img); }
}

pub fn flatten_current(app: &mut App) {
    if let Some(i) = app.editor.current {
        let EditorState { docs, renderer, .. } = &mut app.editor;
        if let Some(d) = docs.get_mut(i) { flatten_all(d, renderer); }
    }
}

pub fn flatten_all(d: &mut OpenDoc, renderer: &mut Renderer) {
    let doc = d.doc.clone();
    if let Some(img) = renderer.flatten(&doc, doc.page()) {
        if let Ok(png) = linkshot_render::encode_png(&img) {
            d.edit("Flatten All", |doc| {
                let page = doc.page_mut();
                page.capture_objects.clear();
                page.capture_background_location = linkshot_core::Pt::ZERO;
                let name = page.capture_background_image.clone();
                doc.blobs.insert(name, std::sync::Arc::new(png));
            });
            renderer.invalidate();
        }
    }
}

fn doc_details(app: &App) -> String {
    match app.editor.current_doc() {
        Some(d) => format!("{} — {}×{} px, {} objects, {}", d.doc.display_name(), d.doc.page().capture_canvas_width, d.doc.page().capture_canvas_height, d.doc.page().capture_objects.len(), d.doc.path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "unsaved".into())),
        None => "No capture selected.".into(),
    }
}

/// The Editor has no system frame (`with_decorations(false)`), so it gets no resize border from the
/// window manager: thin strips along the edges and corners hand a drag to it as a native resize.
fn resize_edges(ui: &mut egui::Ui, ctx: &egui::Context, full: Rect) {
    use egui::{CursorIcon as C, ResizeDirection as D};
    if ctx.input(|i| i.viewport().maximized.unwrap_or(false) || i.viewport().fullscreen.unwrap_or(false)) { return; }
    const EDGE: f32 = 5.0;
    const CORNER: f32 = 12.0;
    let (l, r, t, b) = (full.left(), full.right(), full.top(), full.bottom());
    let zones = [
        (Rect::from_min_max(Pos2::new(l, t), Pos2::new(l + CORNER, t + CORNER)), D::NorthWest, C::ResizeNorthWest),
        (Rect::from_min_max(Pos2::new(r - CORNER, t), Pos2::new(r, t + CORNER)), D::NorthEast, C::ResizeNorthEast),
        (Rect::from_min_max(Pos2::new(l, b - CORNER), Pos2::new(l + CORNER, b)), D::SouthWest, C::ResizeSouthWest),
        (Rect::from_min_max(Pos2::new(r - CORNER, b - CORNER), Pos2::new(r, b)), D::SouthEast, C::ResizeSouthEast),
        (Rect::from_min_max(Pos2::new(l + CORNER, t), Pos2::new(r - CORNER, t + EDGE)), D::North, C::ResizeNorth),
        (Rect::from_min_max(Pos2::new(l + CORNER, b - EDGE), Pos2::new(r - CORNER, b)), D::South, C::ResizeSouth),
        (Rect::from_min_max(Pos2::new(l, t + CORNER), Pos2::new(l + EDGE, b - CORNER)), D::West, C::ResizeWest),
        (Rect::from_min_max(Pos2::new(r - EDGE, t + CORNER), Pos2::new(r, b - CORNER)), D::East, C::ResizeEast),
    ];
    for (i, (rect, dir, cursor)) in zones.into_iter().enumerate() {
        let resp = ui.interact(rect, ui.id().with(("resize-edge", i)), Sense::drag());
        if resp.hovered() || resp.dragged() { ctx.set_cursor_icon(cursor); }
        if resp.drag_started() { ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(dir)); }
    }
}

fn caption_row(app: &mut App, ui: &mut egui::Ui, rect: Rect, ctx: &egui::Context, actions: &mut Vec<Action>) {
    let pal = app.pal;
    ui.painter().rect_filled(rect, 0.0, pal.main_panel);
    let drag = ui.interact(Rect::from_min_max(Pos2::new(rect.left() + 420.0, rect.top()), Pos2::new(rect.right() - 150.0, rect.bottom())), ui.id().with("caption-drag"), Sense::click_and_drag());
    if drag.drag_started() { ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag); }
    if drag.double_clicked() { let max = ctx.input(|i| i.viewport().maximized.unwrap_or(false)); ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!max)); }
    icons::all_in_one_icon(ui.painter(), Rect::from_center_size(Pos2::new(rect.left() + 16.0, rect.center().y), Vec2::splat(14.0)), pal.product);
    ui.scope_builder(egui::UiBuilder::new().max_rect(Rect::from_min_max(Pos2::new(rect.left() + 30.0, rect.top()), Pos2::new(rect.left() + 420.0, rect.bottom()))), |ui| {
        ui.horizontal_centered(|ui| {
            ui.style_mut().visuals.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
            egui::MenuBar::new().ui(ui, |ui| { editor_menu(app, ui, ctx, actions); });
        });
    });
    // title
    let title = app.editor.current_doc().map(|d| { let mut t = d.doc.display_name(); if d.is_dirty() { t.push('*'); } t }).unwrap_or_default();
    ui.painter().text(Pos2::new(rect.center().x + 100.0, rect.center().y), Align2::CENTER_CENTER, format!("{title}  –  LinkShot Editor"), crate::theme::medium(12.5), pal.text);
    // window buttons 46×28
    let mut x = rect.right() - 23.0;
    for (i, (draw, tip)) in [(icons::close_icon as fn(&egui::Painter, Rect, Color32), "Close"), (icons::maximize_icon as fn(&egui::Painter, Rect, Color32), "Maximize"), (icons::minimize_icon as fn(&egui::Painter, Rect, Color32), "Minimize")].iter().enumerate() {
        let r = Rect::from_center_size(Pos2::new(x, rect.center().y), Vec2::new(46.0, CAPTION_H));
        let resp = ui.interact(r, ui.id().with(("wb", i)), Sense::click());
        if resp.hovered() { ui.painter().rect_filled(r.shrink2(Vec2::new(4.0, 3.0)), 6.0, if i == 0 { pal.capture_red } else { pal.control }); }
        draw(ui.painter(), Rect::from_center_size(r.center(), Vec2::splat(20.0)), if resp.hovered() && i == 0 { Color32::WHITE } else { pal.text });
        if resp.on_hover_text(*tip).clicked() {
            match i { 0 => ctx.send_viewport_cmd(egui::ViewportCommand::Close), 1 => { let max = ctx.input(|i| i.viewport().maximized.unwrap_or(false)); ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!max)); } _ => ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true)) }
        }
        x -= 46.0;
    }
    // help / feedback glyphs
    let help_r = Rect::from_center_size(Pos2::new(rect.right() - 150.0, rect.center().y), Vec2::splat(16.0));
    if ui.interact(help_r, ui.id().with("help"), Sense::click()).on_hover_text("Learning Center").clicked() { let _ = linkshot_platform::open_url("https://github.com/ZeroDay-Labz/linkshot#readme"); }
    ui.painter().circle_stroke(help_r.center(), 7.0, Stroke::new(1.5_f32, pal.text));
    ui.painter().text(help_r.center(), Align2::CENTER_CENTER, "?", FontId::proportional(11.0), pal.text);
}

fn editor_menu(app: &mut App, ui: &mut egui::Ui, ctx: &egui::Context, actions: &mut Vec<Action>) {
    let paths = app.paths.clone();
    ui.menu_button("File", |ui| {
        ui.menu_button("New", |ui| {
            if ui.button("New Image\tCtrl+N").clicked() { app.editor.new_image_dialog = true; ui.close(); }
            if ui.button("New Capture").clicked() { let s = app.presets.selected().clone(); actions.push(Action::StartCapture(s)); ui.close(); }
            if ui.button("New from Clipboard\tCtrl+Shift+N").clicked() { if let Some(img) = linkshot_platform::clipboard::get_image() { if let Ok(png) = linkshot_render::encode_png(&img) { let doc = Document::from_png(img.width(), img.height(), png); app.editor.add_document(ctx, doc, &paths, &app.settings); } } ui.close(); }
        });
        if ui.button("Open...\tCtrl+O").clicked() { if let Some(ps) = crate::share::open_dialog(None) { for p in ps { if let Ok(doc) = crate::share::open_file(&p) { app.editor.add_document(ctx, doc, &paths, &app.settings); } } } ui.close(); }
        ui.separator();
        if ui.button("Save\tCtrl+S").clicked() { app.editor.save_current(&paths, &app.settings); ui.close(); }
        if ui.button("Save As...\tCtrl+Shift+S").clicked() { save_as_current(app); ui.close(); }
        if ui.button("Print...\tCtrl+P").clicked() { print_current(app); ui.close(); }
        if ui.button("Duplicate\tCtrl+D").clicked() { if let Some(d) = app.editor.current_doc() { let mut doc = d.doc.clone(); doc.path = None; app.editor.add_document(ctx, doc, &paths, &app.settings); } ui.close(); }
        ui.separator();
        if ui.button("Delete").clicked() { tray::delete_current(app); ui.close(); }
        ui.separator();
        if ui.button("Exit LinkShot Editor").clicked() { ctx.send_viewport_cmd(egui::ViewportCommand::Close); ui.close(); }
    });
    ui.menu_button("Edit", |ui| {
        let (cu, cr) = app.editor.current_doc().map(|d| (d.undo.can_undo(), d.undo.can_redo())).unwrap_or((false, false));
        if ui.add_enabled(cu, egui::Button::new("Undo\tCtrl+Z")).clicked() { app.editor.undo_current(); ui.close(); }
        if ui.add_enabled(cr, egui::Button::new("Redo\tCtrl+Y")).clicked() { app.editor.redo_current(); ui.close(); }
        if ui.add_enabled(cu, egui::Button::new("Undo All")).clicked() { if let Some(d) = app.editor.current_doc_mut() { let mut doc = d.doc.clone(); d.undo.undo_all(&mut doc); d.doc = doc; d.version += 1; } ui.close(); }
        ui.separator();
        if ui.button("Copy All\tCtrl+Shift+C").clicked() { if let Some(d) = app.editor.current_doc() { let doc = d.doc.clone(); let _ = crate::share::copy_all(&mut app.editor.renderer, &doc); } ui.close(); }
        if ui.button("Paste\tCtrl+V").clicked() { tools::paste_image(app, ctx); ui.close(); }
        if ui.button("Delete\tDelete").clicked() { tools::delete_selection(app); ui.close(); }
        ui.separator();
        if ui.add_enabled(app.editor.current_doc().is_some(), egui::Button::new("Grab Text...")).clicked() { grab_text_current(app); ui.close(); }
        ui.separator();
        if ui.button("Select All\tCtrl+A").clicked() { if let Some(d) = app.editor.current_doc() { app.editor.tools.selection = d.doc.page().capture_objects.iter().map(|o| o.object_id.clone()).collect(); } ui.close(); }
        if ui.button("Deselect All\tCtrl+D").clicked() { app.editor.tools.clear_selection(); ui.close(); }
        ui.separator();
        if ui.button("Editor Preferences...").clicked() { app.show_prefs = true; ui.close(); }
    });
    ui.menu_button("Image", |ui| {
        ui.menu_button("Tools", |ui| { for t in ToolMode::TOOLBAR_TOOLS { if ui.button(t.label()).clicked() { app.editor.select_tool(*t); ui.close(); } } });
        ui.menu_button("Effects", |ui| { for e in [EffectType::EffectBorder, EffectType::EffectEdge, EffectType::EffectShadow, EffectType::EffectFilter, EffectType::EffectColorAdjustment, EffectType::EffectTrim] { if ui.button(e.label()).clicked() { app.editor.side_panel = SidePanelMode::Effects; app.editor.side_panel_visible = true; side_panel::set_effect(app, e); ui.close(); } } });
        ui.separator();
        ui.menu_button("Arrange", |ui| {
            if ui.button("Bring Forward\tCtrl+Shift+F").clicked() { tools::reorder(app, 1); ui.close(); }
            if ui.button("Bring to Front").clicked() { tools::reorder(app, i32::MAX); ui.close(); }
            if ui.button("Send Backward\tCtrl+Shift+B").clicked() { tools::reorder(app, -1); ui.close(); }
            if ui.button("Send to Back").clicked() { tools::reorder(app, i32::MIN); ui.close(); }
        });
        ui.separator();
        if ui.button("Flatten\tCtrl+T").clicked() { tools::flatten_selection(app); ui.close(); }
        if ui.button("Flatten All\tCtrl+Shift+T").clicked() { flatten_current(app); ui.close(); }
        ui.separator();
        if ui.button("Auto Trim").clicked() { tools::whole_image_op(app, "Auto Trim", |img| linkshot_render::effects::auto_trim(img, 8)); ui.close(); }
        if ui.button("Resize Image...").clicked() {
            if let Some((w, h)) = app.editor.current_doc().map(|d| (d.doc.page().capture_canvas_width as i64, d.doc.page().capture_canvas_height as i64)) { app.editor.resize_w = w.to_string(); app.editor.resize_h = h.to_string(); }
            app.editor.size_popup = true; ui.close();
        }
        ui.menu_button("Rotate", |ui| {
            if ui.button("Rotate Clockwise").clicked() { tools::whole_image_op(app, "Rotate", |img| linkshot_render::effects::rotate(img, 90)); ui.close(); }
            if ui.button("Rotate Counterclockwise").clicked() { tools::whole_image_op(app, "Rotate", |img| linkshot_render::effects::rotate(img, 270)); ui.close(); }
            if ui.button("Rotate 180°").clicked() { tools::whole_image_op(app, "Rotate", |img| linkshot_render::effects::rotate(img, 180)); ui.close(); }
            ui.separator();
            if ui.button("Flip Horizontal").clicked() { tools::whole_image_op(app, "Flip", linkshot_render::effects::flip_h); ui.close(); }
            if ui.button("Flip Vertical").clicked() { tools::whole_image_op(app, "Flip", linkshot_render::effects::flip_v); ui.close(); }
        });
        let mut snap = app.settings.canvas_snapping;
        if ui.checkbox(&mut snap, "Canvas Snapping\tCtrl+;").changed() { app.settings.canvas_snapping = snap; app.mark_settings_dirty(); }
        if ui.button("Canvas Color...").clicked() { app.editor.canvas_color_dialog = true; ui.close(); }
    });
    ui.menu_button("Video", |ui| {
        let vi = app.editor.current.filter(|i| app.editor.docs[*i].is_video());
        let en = vi.is_some();
        let playing = vi.map(|i| app.editor.docs[i].video.as_ref().unwrap().playing).unwrap_or(false);
        if ui.add_enabled(en, egui::Button::new(if playing { "Pause\tSpace" } else { "Play\tSpace" })).clicked() { if let Some(i) = vi { app.editor.docs[i].video.as_mut().unwrap().toggle_play(); } ui.close(); }
        if ui.add_enabled(en, egui::Button::new("Previous Frame\t←")).clicked() { if let Some(i) = vi { app.editor.docs[i].video.as_ref().unwrap().send(linkshot_video::PlayerCmd::Step(-1)); } ui.close(); }
        if ui.add_enabled(en, egui::Button::new("Next Frame\t→")).clicked() { if let Some(i) = vi { app.editor.docs[i].video.as_ref().unwrap().send(linkshot_video::PlayerCmd::Step(1)); } ui.close(); }
        ui.separator();
        if ui.add_enabled(en, egui::Button::new("Set In Point\tI")).clicked() { if let Some(i) = vi { let v = app.editor.docs[i].video.as_mut().unwrap(); v.in_ms = Some(v.position_ms); } ui.close(); }
        if ui.add_enabled(en, egui::Button::new("Set Out Point\tO")).clicked() { if let Some(i) = vi { let v = app.editor.docs[i].video.as_mut().unwrap(); v.out_ms = Some(v.position_ms.max(1)); } ui.close(); }
        if ui.add_enabled(en, egui::Button::new("Clear In/Out Points")).clicked() { if let Some(i) = vi { let v = app.editor.docs[i].video.as_mut().unwrap(); v.in_ms = None; v.out_ms = None; } ui.close(); }
        ui.separator();
        if ui.add_enabled(en, egui::Button::new("Create Animated GIF...")).clicked() { if let Some(i) = vi { video_view::open_gif_dialog(app, i); } ui.close(); }
        if ui.add_enabled(en, egui::Button::new("Save as MP4...")).clicked() { if let Some(i) = vi { video_view::save_video_as(app, i, "mp4"); } ui.close(); }
        if ui.add_enabled(en, egui::Button::new("Save Frame as Image")).clicked() { if let Some(i) = vi { video_view::save_frame_as_image(app, i); } ui.close(); }
        if ui.add_enabled(en, egui::Button::new("Copy Frame")).clicked() { if let Some(i) = vi { video_view::copy_frame(app, i); } ui.close(); }
        ui.separator();
        ui.label(egui::RichText::new(format!("Default format: {}  (Preferences ▸ Video)", app.settings.default_video_format)).small());
    });
    ui.menu_button("Share", |ui| {
        for id in app.settings.enabled_outputs.clone() {
            if ui.button(output_ids::title(&id)).clicked() {
                if let Some(i) = app.editor.current {
                    let mut s = CaptureSettings::default();
                    s.share_destination_ids = vec![id.clone()];
                    s.share_settings_file.file_output_prompt = true;
                    let mut doc = app.editor.docs[i].doc.clone();
                    let st = app.settings.clone();
                    app.editor.share_now(&mut doc, &s, &paths, &st);
                    app.editor.docs[i].doc = doc;
                }
                ui.close();
            }
        }
        ui.separator();
        if ui.button("Save As Preferences...").clicked() { app.show_prefs = true; ui.close(); }
    });
    ui.menu_button("View", |ui| {
        if ui.button("Zoom In\tCtrl++").clicked() { app.editor.canvas.zoom_step(1); ui.close(); }
        if ui.button("Zoom Out\tCtrl+-").clicked() { app.editor.canvas.zoom_step(-1); ui.close(); }
        if ui.button("Zoom to Fit").clicked() { app.editor.canvas.fit_requested = true; ui.close(); }
        let mut shrink = app.settings.shrink_to_fit_new_documents;
        if ui.checkbox(&mut shrink, "Shrink to Fit").changed() { app.settings.shrink_to_fit_new_documents = shrink; app.mark_settings_dirty(); }
        if ui.button("Actual Size\tCtrl+0").clicked() { app.editor.canvas.zoom = 1.0; app.editor.canvas.fit = false; ui.close(); }
        ui.separator();
        ui.menu_button(format!("Interface Scale ({}%)", (app.settings.ui_scale * 100.0).round() as i32), |ui| {
            for pct in [75, 90, 100, 110, 125, 150, 175, 200] {
                let z = pct as f32 / 100.0;
                if ui.radio((app.settings.ui_scale * 100.0).round() as i32 == pct, format!("{pct}%")).clicked() { let c = ui.ctx().clone(); app.set_ui_scale(&c, z); ui.close(); }
            }
            ui.separator();
            ui.label(egui::RichText::new("Ctrl+Shift + / −, Ctrl+Shift+0 resets. Applies to every LinkShot window, on top of the monitor's own scaling.").small());
        });
        ui.separator();
        let dark = app.settings.ui_theme == linkshot_core::settings::UiTheme::Dark;
        if ui.radio(dark, "Dark Theme").clicked() { actions.push(Action::SetTheme(linkshot_core::settings::UiTheme::Dark)); ui.close(); }
        if ui.radio(!dark, "Light Theme").clicked() { actions.push(Action::SetTheme(linkshot_core::settings::UiTheme::Light)); ui.close(); }
        ui.separator();
        ui.menu_button("Background Color", |ui| {
            for (name, c) in [("Black", 0x000000u32), ("Dark Gray", 0x1C1915), ("Gray", 0x7F7F7F), ("Light Gray", 0xD3D3D3), ("White", 0xFFFFFF)] {
                if ui.button(name).clicked() { app.settings.editor_workspace_color = linkshot_core::Argb::from_u32(0xFF00_0000 | c); app.mark_settings_dirty(); ui.close(); }
            }
        });
        ui.separator();
        if ui.checkbox(&mut (app.editor.side_panel == SidePanelMode::Effects && app.editor.side_panel_visible), "Effects").clicked() { app.editor.side_panel = SidePanelMode::Effects; app.editor.side_panel_visible = true; }
        if ui.checkbox(&mut (app.editor.side_panel == SidePanelMode::Properties && app.editor.side_panel_visible), "Properties").clicked() { app.editor.side_panel = SidePanelMode::Properties; app.editor.side_panel_visible = true; }
        if ui.checkbox(&mut app.editor.side_panel_visible, "Side Bar").changed() { app.settings.side_panel_visible = app.editor.side_panel_visible; app.mark_settings_dirty(); }
        ui.separator();
        if ui.button("Details\tAlt+Enter").clicked() { let s = doc_details(app); app.editor.set_status(s); ui.close(); }
        if ui.checkbox(&mut app.editor.library_view, "Library").changed() {
            if app.editor.library_view && !app.editor.library_scanned { let dir = app.settings.library_dir(&app.paths); app.editor.start_library_scan(dir); }
            ui.close();
        }
        if ui.checkbox(&mut app.editor.tray_visible, "Recent Captures").changed() { app.settings.tray_visible = app.editor.tray_visible; app.mark_settings_dirty(); }
    });
    ui.menu_button("Help", |ui| {
        if ui.button("Tutorials").clicked() { let _ = linkshot_platform::open_url("https://github.com/ZeroDay-Labz/linkshot#readme"); ui.close(); }
        ui.separator();
        if ui.button("About LinkShot").clicked() { app.show_about = true; ui.close(); }
    });
}

fn top_panel(app: &mut App, ui: &mut egui::Ui, rect: Rect, actions: &mut Vec<Action>) {
    let pal = app.pal;
    ui.painter().rect_filled(rect, 0.0, pal.main_panel);
    // left group
    ui.scope_builder(egui::UiBuilder::new().max_rect(Rect::from_min_size(rect.min + Vec2::new(8.0, 6.0), Vec2::new(300.0, rect.height() - 12.0))), |ui| {
        ui.horizontal_centered(|ui| {
            let lib = widgets::icon_button(ui, 34.0, "Library", pal.control_dark, |p, r| icons::library_icon(p, r.shrink(6.0), if app.editor.library_view { pal.product } else { pal.text }));
            if lib.clicked() { app.editor.library_view = !app.editor.library_view; if app.editor.library_view && !app.editor.library_scanned { let dir = app.settings.library_dir(&app.paths); app.editor.start_library_scan(dir); } }
            ui.add_space(4.0);
            // Capture button (red circle with camera)
            let (r, resp) = ui.allocate_exact_size(Vec2::new(94.0, 34.0), Sense::click());
            let hov = ui.ctx().animate_bool(resp.id.with("glow"), resp.hovered());
            ui.painter().rect_filled(r.expand(4.0), 21.0, pal.capture_red.gamma_multiply(0.12 + hov * 0.14));
            ui.painter().rect_filled(r.translate(Vec2::new(0.0, 2.0)), 17.0, Color32::from_black_alpha(60));
            ui.painter().rect_filled(r, 17.0, if resp.hovered() { pal.capture_red_hover } else { pal.capture_red });
            ui.painter().circle_filled(Pos2::new(r.left() + 17.0, r.center().y), 11.0, Color32::from_white_alpha(40));
            icons::camera_icon(ui.painter(), Rect::from_center_size(Pos2::new(r.left() + 17.0, r.center().y), Vec2::splat(16.0)), Color32::WHITE);
            ui.painter().text(Pos2::new(r.left() + 36.0, r.center().y), Align2::LEFT_CENTER, "Capture", crate::theme::semibold(13.0), Color32::WHITE);
            if resp.on_hover_text("Start a new capture").clicked() { let s = app.presets.selected().clone(); actions.push(Action::StartCapture(s)); }
            ui.add_space(4.0);
            let create = widgets::dropdown_button(ui, &pal, "Create", false, 0.0);
            egui::Popup::menu(&create).show(|ui| {
                if ui.button("Image from Template...").clicked() { app.editor.set_status("Templates are planned for a later phase."); ui.close(); }
                if ui.button("Video from Images...").clicked() { app.editor.set_status("Video from Images is planned for a later phase."); ui.close(); }
            });
        });
    });
    // right group
    ui.scope_builder(egui::UiBuilder::new().max_rect(Rect::from_min_max(Pos2::new(rect.right() - 330.0, rect.top() + 6.0), Pos2::new(rect.right() - 8.0, rect.bottom() - 6.0))), |ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let share = widgets::dropdown_button(ui, &pal, "Share", true, 0.0).on_hover_text("Share to a specific app or save to your computer");
            egui::Popup::menu(&share).show(|ui| {
                for id in app.settings.enabled_outputs.clone() {
                    if ui.button(output_ids::title(&id)).clicked() {
                        if let Some(i) = app.editor.current {
                            let mut s = CaptureSettings::default();
                            s.share_destination_ids = vec![id.clone()];
                            let mut doc = app.editor.docs[i].doc.clone();
                            let paths = app.paths.clone(); let st = app.settings.clone();
                            app.editor.share_now(&mut doc, &s, &paths, &st);
                            app.editor.docs[i].doc = doc;
                        }
                        ui.close();
                    }
                }
            });
            // "Share Link" (cloud upload) is not part of this build; the button would only disappoint.
            let copy = ui.add(egui::Button::new(egui::RichText::new("⧉  Copy All").color(pal.text_bright)).fill(pal.control).stroke(Stroke::new(1.0_f32, pal.control_outline)));
            if copy.on_hover_text("Copy the image to the clipboard (Ctrl+Shift+C)").clicked() { if let Some(d) = app.editor.current_doc() { let doc = d.doc.clone(); match crate::share::copy_all(&mut app.editor.renderer, &doc) { Ok(()) => app.editor.set_status("Copied"), Err(e) => app.editor.set_status(format!("Copy failed: {e}")) } } }
        });
    });
    // toolbar (centred)
    let tb = Rect::from_min_max(Pos2::new(rect.left() + 300.0, rect.top() + 3.0), Pos2::new(rect.right() - 330.0, rect.bottom() - 2.0));
    toolbar_ui(app, ui, tb);
}

fn toolbar_ui(app: &mut App, ui: &mut egui::Ui, rect: Rect) {
    let pal = app.pal;
    let labels = app.settings.show_toolbar_labels;
    let item_w = if labels { 56.0 } else { 44.0 };
    let items = app.editor.toolbar.clone();
    let visible_tools: Vec<ToolMode> = items.iter().filter(|i| i.item_type == ToolBarItemType::Tool).map(|i| i.tool_mode).collect();
    let more_tool = app.editor.more_tool.filter(|t| !visible_tools.contains(t));
    let count = items.len() as f32 + if more_tool.is_some() { 1.0 } else { 0.0 };
    let total_w = count * item_w;
    let start_x = (rect.center().x - total_w / 2.0).max(rect.left());
    let mut x = start_x;
    let h = rect.height();
    let mut i_more = 0;
    let mut to_select: Option<ToolMode> = None;
    for (idx, item) in items.iter().enumerate() {
        let r = Rect::from_min_size(Pos2::new(x, rect.top()), Vec2::new(item_w, h));
        match item.item_type {
            ToolBarItemType::Spacer => { ui.painter().vline(r.center().x, (r.top() + 14.0)..=(r.bottom() - 14.0), Stroke::new(1.0_f32, pal.separator)); }
            ToolBarItemType::Tool => {
                if tool_button(app, ui, r, item.tool_mode, labels, idx) { to_select = Some(item.tool_mode); }
            }
            ToolBarItemType::EditMore => {
                // a tool picked from "More" is shown right before the More button
                if let Some(t) = more_tool {
                    if tool_button(app, ui, r, t, labels, 900) { to_select = Some(t); }
                    x += item_w;
                }
                let r = Rect::from_min_size(Pos2::new(x, rect.top()), Vec2::new(item_w, h));
                let resp = ui.interact(r, ui.id().with(("tb-more", idx)), Sense::click());
                if resp.hovered() { ui.painter().rect_filled(r.shrink2(Vec2::new(2.0, 4.0)), 8.0, pal.control); }
                ui.painter().text(Pos2::new(r.center().x, r.center().y - if labels { 6.0 } else { 0.0 }), Align2::CENTER_CENTER, "More", FontId::proportional(12.5), pal.text);
                icons::chevron_down(ui.painter(), Rect::from_center_size(Pos2::new(r.center().x, r.center().y + if labels { 10.0 } else { 12.0 }), Vec2::splat(12.0)), pal.text);
                crate::widgets::popup(&resp, false, egui::PopupCloseBehavior::CloseOnClick, |ui| {
                    ui.set_min_width(200.0);
                    for t in ToolMode::TOOLBAR_TOOLS.iter().filter(|t| !visible_tools.contains(t)) {
                        let (rr, rs) = ui.allocate_exact_size(Vec2::new(200.0, 36.0), Sense::click());
                        if rs.hovered() { ui.painter().rect_filled(rr, 6.0, pal.control); }
                        icons::tool_icon(ui.painter(), Rect::from_center_size(Pos2::new(rr.left() + 20.0, rr.center().y), Vec2::splat(22.0)), *t, pal.text);
                        ui.painter().text(Pos2::new(rr.left() + 40.0, rr.center().y), Align2::LEFT_CENTER, t.label(), FontId::proportional(13.0), pal.text);
                        if rs.clicked() { to_select = Some(*t); }
                    }
                    ui.separator();
                    if ui.button("Customize Toolbar...").clicked() { app.editor.show_customize_toolbar = true; }
                });
                i_more += 1;
            }
            ToolBarItemType::Command => {
                let resp = ui.interact(r, ui.id().with(("tb-cmd", idx)), Sense::click());
                let (cu, cr) = app.editor.current_doc().map(|d| (d.undo.can_undo(), d.undo.can_redo())).unwrap_or((false, false));
                let enabled = match item.command_id { ToolBarCommandType::Undo => cu, ToolBarCommandType::Redo => cr, _ => app.editor.current.is_some() };
                let c = if enabled { pal.text } else { pal.text.gamma_multiply(0.35) };
                if resp.hovered() && enabled { ui.painter().rect_filled(r.shrink2(Vec2::new(2.0, 4.0)), 8.0, pal.control); }
                let ir = Rect::from_center_size(Pos2::new(r.center().x, r.center().y - if labels { 8.0 } else { 0.0 }), Vec2::splat(24.0));
                match item.command_id {
                    ToolBarCommandType::Undo => icons::undo_icon(ui.painter(), ir, c, false),
                    ToolBarCommandType::Redo => icons::undo_icon(ui.painter(), ir, c, true),
                    other => { ui.painter().text(ir.center(), Align2::CENTER_CENTER, &other.as_str()[..1], FontId::proportional(16.0), c); }
                }
                if labels { ui.painter().text(Pos2::new(r.center().x, r.bottom() - 10.0), Align2::CENTER_CENTER, item.command_id.as_str(), FontId::proportional(11.0), c); }
                if resp.clicked() && enabled {
                    match item.command_id {
                        ToolBarCommandType::Undo => app.editor.undo_current(),
                        ToolBarCommandType::Redo => app.editor.redo_current(),
                        ToolBarCommandType::Save => app.editor.save_current(&app.paths, &app.settings),
                        ToolBarCommandType::FlattenAll => flatten_current(app),
                        ToolBarCommandType::DeleteObject => tools::delete_selection(app),
                        ToolBarCommandType::AutoTrim => tools::whole_image_op(app, "Auto Trim", |img| linkshot_render::effects::auto_trim(img, 8)),
                        ToolBarCommandType::RotateImage => tools::whole_image_op(app, "Rotate", |img| linkshot_render::effects::rotate(img, 90)),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        x += item_w;
    }
    let _ = i_more;
    if let Some(t) = to_select { app.editor.select_tool(t); }
}

fn tool_button(app: &App, ui: &mut egui::Ui, r: Rect, tool: ToolMode, labels: bool, idx: usize) -> bool {
    let pal = app.pal;
    let selected = app.editor.tool == tool;
    let resp = ui.interact(r, ui.id().with(("tb-tool", idx, tool.as_str())), Sense::click());
    let rr = r.shrink2(Vec2::new(2.0, 4.0));
    let sel = ui.ctx().animate_bool(resp.id.with("sel"), selected);
    let hov = ui.ctx().animate_bool(resp.id.with("hov"), resp.hovered() && !selected);
    if sel > 0.0 { ui.painter().rect_filled(rr, 8.0, pal.product.gamma_multiply(if pal.is_dark { 0.28 } else { 0.16 } * sel)); ui.painter().rect_stroke(rr, 8.0, Stroke::new(1.0_f32, pal.product.gamma_multiply(0.6 * sel)), egui::StrokeKind::Inside); }
    if hov > 0.0 { ui.painter().rect_filled(rr, 8.0, pal.control.gamma_multiply(hov)); }
    let c = if selected { pal.product_bright } else { pal.text };
    let ir = Rect::from_center_size(Pos2::new(r.center().x, r.center().y - if labels { 8.0 } else { 0.0 }), Vec2::splat(24.0));
    icons::tool_icon(ui.painter(), ir, tool, c);
    if labels { ui.painter().text(Pos2::new(r.center().x, r.bottom() - 11.0), Align2::CENTER_CENTER, tool.label(), if selected { crate::theme::medium(11.0) } else { FontId::proportional(11.0) }, if selected { pal.text_bright } else { c }); }
    resp.on_hover_text(tool.label()).clicked()
}

fn footer_ui(app: &mut App, ui: &mut egui::Ui, rect: Rect, _ctx: &egui::Context, _actions: &mut Vec<Action>) {
    let pal = app.pal;
    ui.painter().rect_filled(rect, 0.0, pal.main_panel);
    ui.painter().hline(rect.x_range(), rect.top(), Stroke::new(1.0_f32, pal.separator));
    ui.scope_builder(egui::UiBuilder::new().max_rect(rect.shrink2(Vec2::new(8.0, 4.0))), |ui| {
        ui.horizontal_centered(|ui| {
            let t = if app.editor.tray_visible { "Hide Recent" } else { "Show Recent" };
            if ui.add(egui::Button::new(egui::RichText::new(t).size(13.5)).frame(false)).clicked() { app.editor.tray_visible = !app.editor.tray_visible; app.settings.tray_visible = app.editor.tray_visible; app.mark_settings_dirty(); }
            ui.add_space(8.0);
            let tag = ui.add(egui::Button::new(egui::RichText::new("Tag").size(13.5)).frame(false));
            let tag_id = ui.id().with("tag-popup");
            crate::widgets::popup(&tag, true, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
                ui.set_min_width(240.0);
                ui.label(egui::RichText::new("Tags").strong());
                if let Some(i) = app.editor.current {
                    let tags = app.editor.docs[i].doc.metadata.tags.clone();
                    for (ti, t) in tags.iter().enumerate() { ui.horizontal(|ui| { ui.label(t); if ui.small_button("✕").clicked() { app.editor.docs[i].edit("Remove Tag", |d| { d.metadata.tags.remove(ti); }); } }); }
                    let mut new = ui.data_mut(|d| d.get_temp::<String>(tag_id).unwrap_or_default());
                    let te = ui.text_edit_singleline(&mut new);
                    if te.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !new.trim().is_empty() {
                        let t = new.trim().to_string();
                        app.editor.docs[i].edit("Add Tag", |d| d.metadata.tags.push(t));
                        new.clear();
                    }
                    ui.data_mut(|d| d.insert_temp(tag_id, new));
                } else { ui.label("Select a capture first."); }
            });
            // centre: zoom + dimensions
            let centre_w = 240.0;
            let cx = rect.center().x;
            ui.scope_builder(egui::UiBuilder::new().max_rect(Rect::from_center_size(Pos2::new(cx, rect.center().y), Vec2::new(centre_w, rect.height() - 8.0))), |ui| {
                ui.horizontal_centered(|ui| {
                    ui.painter().vline(ui.cursor().left(), rect.y_range().shrink(8.0), Stroke::new(1.0_f32, pal.separator));
                    let z = ui.add(egui::Button::new(egui::RichText::new(format!("{}%", (app.editor.canvas.zoom * 100.0).round() as i32)).size(12.0)).frame(false).min_size(Vec2::new(110.0, 24.0)));
                    let z = z.on_hover_text("Zoom");
                    crate::widgets::popup(&z, true, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
                        ui.set_min_width(230.0);
                        ui.horizontal(|ui| { if ui.button("Fit to Screen").clicked() { app.editor.canvas.fit_requested = true; } if ui.button("Actual Size").clicked() { app.editor.canvas.zoom = 1.0; app.editor.canvas.fit = false; } });
                        let mut pct = app.editor.canvas.zoom * 100.0;
                        ui.horizontal(|ui| { ui.label("Zoom"); if ui.add(egui::Slider::new(&mut pct, 5.0..=800.0).logarithmic(true).show_value(false)).changed() { app.editor.canvas.zoom = pct / 100.0; app.editor.canvas.fit = false; } ui.label(format!("{}%", pct.round() as i32)); });
                        let mut shrink = app.settings.shrink_to_fit_new_documents;
                        if ui.checkbox(&mut shrink, "Shrink to fit new documents").changed() { app.settings.shrink_to_fit_new_documents = shrink; app.mark_settings_dirty(); }
                    });
                    ui.painter().vline(ui.cursor().left(), rect.y_range().shrink(8.0), Stroke::new(1.0_f32, pal.separator));
                    let dims = app.editor.current_doc().map(|d| { let p = d.doc.page(); let corner = p.corner_radius_ratio.filter(|r| *r > 0.0).map(|r| format!("  ⌜{}", (r * 100.0).round() as i32)).unwrap_or_default(); format!("{} × {}{}", p.capture_canvas_width as i64, p.capture_canvas_height as i64, corner) }).unwrap_or_default();
                    let dbtn = ui.add(egui::Button::new(egui::RichText::new(dims).size(12.0)).frame(false).min_size(Vec2::new(110.0, 24.0)));
                    let dbtn = dbtn.on_hover_text("Resize Image");
                    if dbtn.clicked() { let dims = app.editor.current_doc().map(|d| (d.doc.page().capture_canvas_width as i64, d.doc.page().capture_canvas_height as i64)); if let Some((w, h)) = dims { app.editor.resize_w = w.to_string(); app.editor.resize_h = h.to_string(); } }
                    crate::widgets::popup(&dbtn, true, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
                        ui.set_min_width(200.0);
                        resize_popup(app, ui);
                    });
                    ui.painter().vline(ui.cursor().left(), rect.y_range().shrink(8.0), Stroke::new(1.0_f32, pal.separator));
                });
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // pill with Effects / Properties toggles
                egui::Frame::new().fill(pal.dark_panel).stroke(Stroke::new(1.0_f32, pal.control_outline)).corner_radius(8.0).inner_margin(3.0).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let props = app.editor.side_panel == SidePanelMode::Properties && app.editor.side_panel_visible;
                        let fx = app.editor.side_panel == SidePanelMode::Effects && app.editor.side_panel_visible;
                        if ui.add(egui::Button::new(egui::RichText::new("Properties").size(13.5).color(if props { pal.text_bright } else { pal.text })).fill(if props { pal.control_bright } else { Color32::TRANSPARENT }).corner_radius(6.0).min_size(Vec2::new(89.0, 26.0))).on_hover_text("Show the tool properties panel").clicked() { if props { app.editor.side_panel_visible = false; } else { app.editor.side_panel = SidePanelMode::Properties; app.editor.side_panel_visible = true; } app.settings.side_panel_visible = app.editor.side_panel_visible; app.settings.effects_panel = false; app.mark_settings_dirty(); }
                        if ui.add(egui::Button::new(egui::RichText::new("Effects").size(13.5).color(if fx { pal.text_bright } else { pal.text })).fill(if fx { pal.control_bright } else { Color32::TRANSPARENT }).corner_radius(6.0).min_size(Vec2::new(89.0, 26.0))).on_hover_text("Show effects panel").clicked() { if fx { app.editor.side_panel_visible = false; } else { app.editor.side_panel = SidePanelMode::Effects; app.editor.side_panel_visible = true; } app.settings.side_panel_visible = app.editor.side_panel_visible; app.settings.effects_panel = true; app.mark_settings_dirty(); }
                    });
                });
            });
        });
    });
}

fn resize_popup(app: &mut App, ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("Resize Image").strong());
    let mut lock = app.editor.resize_lock;
    let (ow, oh) = app.editor.current_doc().map(|d| (d.doc.page().capture_canvas_width, d.doc.page().capture_canvas_height)).unwrap_or((1.0, 1.0));
    ui.horizontal(|ui| {
        ui.label("Width");
        let w = ui.add(egui::TextEdit::singleline(&mut app.editor.resize_w).desired_width(60.0));
        if w.changed() && lock { if let Ok(v) = app.editor.resize_w.trim().parse::<f64>() { app.editor.resize_h = ((v / ow * oh).round() as i64).to_string(); } }
        ui.label("px");
    });
    ui.horizontal(|ui| {
        ui.label("Height");
        let h = ui.add(egui::TextEdit::singleline(&mut app.editor.resize_h).desired_width(60.0));
        if h.changed() && lock { if let Ok(v) = app.editor.resize_h.trim().parse::<f64>() { app.editor.resize_w = ((v / oh * ow).round() as i64).to_string(); } }
        ui.label("px");
    });
    if ui.checkbox(&mut lock, "Lock aspect ratio").changed() { app.editor.resize_lock = lock; }
    ui.horizontal(|ui| {
        if ui.button("Apply").clicked() {
            if let (Ok(w), Ok(h)) = (app.editor.resize_w.trim().parse::<u32>(), app.editor.resize_h.trim().parse::<u32>()) {
                if w > 0 && h > 0 && w <= 25000 && h <= 25000 {
                    tools::whole_image_op(app, "Resize Image", move |img| linkshot_render::effects::scale(img, w, h, true));
                    app.editor.size_popup = false;
                }
            }
            ui.close();
        }
        if ui.button("Cancel").clicked() { ui.close(); }
    });
}

fn library_view(app: &mut App, ui: &mut egui::Ui, rect: Rect, ctx: &egui::Context) {
    let pal = app.pal;
    ui.painter().rect_filled(rect, 0.0, pal.main_panel);
    let search_w = rect.width() * 0.285;
    let search = Rect::from_min_size(rect.min, Vec2::new(search_w, rect.height()));
    ui.painter().rect_filled(search, 0.0, pal.dark_panel);
    ui.scope_builder(egui::UiBuilder::new().max_rect(search.shrink(10.0)), |ui| {
        ui.label(egui::RichText::new("Library").strong().size(14.0));
        ui.add(egui::TextEdit::singleline(&mut app.editor.library_filter).hint_text("Search"));
        ui.add_space(8.0);
        let n_img = app.editor.library.entries.values().filter(|e| e.kind == TrayItemType::Image).count();
        let n_vid = app.editor.library.entries.values().filter(|e| e.kind == TrayItemType::Video).count();
        ui.label(format!("Images ({n_img})"));
        ui.label(format!("Videos ({n_vid})"));
        ui.add_space(8.0);
        ui.label(egui::RichText::new("Tags").strong());
        for t in app.editor.library.all_tags() { if ui.selectable_label(app.editor.library_filter == t, &t).clicked() { app.editor.library_filter = t; } }
        ui.add_space(8.0);
        if ui.button("Refresh").clicked() { let dir = app.settings.library_dir(&app.paths); app.editor.library_scanned = false; app.editor.start_library_scan(dir); }
        if app.editor.library_scan.is_some() { ui.label(egui::RichText::new("Scanning…").small().color(pal.input_secondary)); }
        ui.label(egui::RichText::new(app.settings.library_dir(&app.paths).display().to_string()).small().color(pal.input_secondary));
    });
    app.editor.poll_library_scan();
    if app.editor.library_thumbs.is_none() { app.editor.library_thumbs = Some(library_thumbs::LibraryThumbs::new(app.editor.wake.clone())); }
    let grid = Rect::from_min_max(Pos2::new(search.right(), rect.top()), rect.max);
    ui.scope_builder(egui::UiBuilder::new().max_rect(grid.shrink(10.0)), |ui| {
        let filter = app.editor.library_filter.to_lowercase();
        let mut entries: Vec<_> = app.editor.library.entries.values().filter(|e| filter.is_empty() || e.path.to_string_lossy().to_lowercase().contains(&filter) || e.tags.iter().any(|t| t.to_lowercase().contains(&filter))).cloned().collect();
        entries.sort_by(|a, b| b.modified.cmp(&a.modified));
        if entries.is_empty() {
            ui.add_space(40.0);
            ui.vertical_centered(|ui| { ui.label(egui::RichText::new(if app.editor.library_scan.is_some() { "Scanning the library…" } else { "No captures in the library yet." }).color(pal.input_secondary)); });
            return;
        }
        // Only the visible rows are laid out, so a library of thousands stays instant.
        const CARD_W: f32 = library_thumbs::THUMB_W as f32 + 8.0;
        const CARD_H: f32 = library_thumbs::THUMB_H as f32 + 30.0;
        let cols = ((ui.available_width() + 8.0) / (CARD_W + 8.0)).floor().max(1.0) as usize;
        let rows = entries.len().div_ceil(cols);
        let cache_dir = app.paths.thumbnails_dir();
        let mut open: Option<PathBuf> = None;
        egui::ScrollArea::vertical().show_rows(ui, CARD_H + 8.0, rows, |ui, range| {
            for row in range {
                ui.horizontal(|ui| {
                    for e in entries.iter().skip(row * cols).take(cols) {
                        let (r, resp) = ui.allocate_exact_size(Vec2::new(CARD_W, CARD_H), Sense::click());
                        ui.painter().rect_filled(r.translate(Vec2::new(0.0, 2.0)), 8.0, Color32::from_black_alpha(40));
                        ui.painter().rect_filled(r, 8.0, pal.control);
                        ui.painter().rect_stroke(r, 8.0, Stroke::new(1.0_f32, pal.control_outline), egui::StrokeKind::Inside);
                        let thumb_area = Rect::from_min_size(r.min + Vec2::new(4.0, 4.0), Vec2::new(library_thumbs::THUMB_W as f32, library_thumbs::THUMB_H as f32));
                        match app.editor.library_thumbs.as_mut().unwrap().texture(ctx, e, &cache_dir) {
                            Some(tid) => {
                                let size = ctx.tex_manager().read().meta(tid).map(|m| Vec2::new(m.size[0] as f32, m.size[1] as f32)).unwrap_or(thumb_area.size());
                                let img_rect = Rect::from_center_size(thumb_area.center(), size.min(thumb_area.size()));
                                ui.painter().image(tid, img_rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                            }
                            None => { ui.painter().text(thumb_area.center(), Align2::CENTER_CENTER, format!("{}×{}", e.width, e.height), FontId::proportional(12.0), pal.input_secondary); }
                        }
                        let name = e.path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                        let badge = match e.kind { TrayItemType::Video => " · MP4", TrayItemType::AnimatedGif => " · GIF", _ => "" };
                        ui.painter().with_clip_rect(r).text(Pos2::new(r.center().x, r.bottom() - 12.0), Align2::CENTER_CENTER, format!("{name}{badge}"), FontId::proportional(11.0), pal.text);
                        if resp.hovered() { ui.painter().rect_stroke(r, 8.0, Stroke::new(2.0_f32, pal.product), egui::StrokeKind::Inside); resp.clone().on_hover_text(format!("{}\n{}×{} · {}", e.path.display(), e.width, e.height, e.modified.format("%Y-%m-%d %H:%M"))); }
                        if resp.double_clicked() { open = Some(e.path.clone()); }
                    }
                });
            }
        });
        if let Some(path) = open {
            if let Some(i) = app.editor.docs.iter().position(|d| d.doc.path.as_ref() == Some(&path)) { app.editor.current = Some(i); }
            else if let Ok(doc) = crate::share::open_file(&path) { app.editor.add_document(ctx, doc, &app.paths, &app.settings); }
            app.editor.library_view = false;
        }
    });
}

fn dialogs(app: &mut App, ctx: &egui::Context, actions: &mut Vec<Action>) {
    let pal = app.pal;
    if app.editor.show_customize_toolbar {
        egui::Window::new("Customize Toolbar").collapsible(false).resizable(false).default_width(570.0).show(ctx, |ui| {
            ui.label("Click a tool to add it to or remove it from the toolbar.");
            ui.label(egui::RichText::new("Drawing tools").size(14.0));
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                for t in ToolMode::TOOLBAR_TOOLS {
                    let on = app.editor.toolbar.iter().any(|i| i.item_type == ToolBarItemType::Tool && i.tool_mode == *t);
                    if ui.selectable_label(on, t.label()).clicked() {
                        if on { app.editor.toolbar.retain(|i| !(i.item_type == ToolBarItemType::Tool && i.tool_mode == *t)); }
                        else { let pos = app.editor.toolbar.iter().position(|i| i.item_type == ToolBarItemType::EditMore).unwrap_or(app.editor.toolbar.len()); app.editor.toolbar.insert(pos, ToolBarItem::tool(*t)); }
                        app.settings.toolbar = app.editor.toolbar.clone(); app.mark_settings_dirty();
                    }
                }
            });
            ui.label(egui::RichText::new("Menu items").size(14.0));
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                for c in [ToolBarCommandType::Undo, ToolBarCommandType::Redo, ToolBarCommandType::Save, ToolBarCommandType::FlattenAll, ToolBarCommandType::DeleteObject, ToolBarCommandType::AutoTrim, ToolBarCommandType::RotateImage] {
                    let on = app.editor.toolbar.iter().any(|i| i.item_type == ToolBarItemType::Command && i.command_id == c);
                    if ui.selectable_label(on, c.as_str()).clicked() {
                        if on { app.editor.toolbar.retain(|i| !(i.item_type == ToolBarItemType::Command && i.command_id == c)); } else { app.editor.toolbar.push(ToolBarItem::command(c)); }
                        app.settings.toolbar = app.editor.toolbar.clone(); app.mark_settings_dirty();
                    }
                }
            });
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Restore Defaults").clicked() { app.editor.toolbar = ToolBarItem::default_toolbar(); app.settings.toolbar = app.editor.toolbar.clone(); app.mark_settings_dirty(); }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { if widgets::flat_button(ui, &pal, "Done", true).clicked() { app.editor.show_customize_toolbar = false; } });
            });
        });
    }
    if app.editor.new_image_dialog {
        egui::Window::new("New Image").collapsible(false).resizable(false).show(ctx, |ui| {
            ui.horizontal(|ui| { ui.label("Width"); ui.add(egui::TextEdit::singleline(&mut app.editor.new_w).desired_width(60.0)); ui.label("Height"); ui.add(egui::TextEdit::singleline(&mut app.editor.new_h).desired_width(60.0)); });
            let mut c = crate::theme::argb_to_color32(app.settings.new_image_color);
            ui.horizontal(|ui| { ui.label("Canvas color"); if ui.color_edit_button_srgba(&mut c).changed() { app.settings.new_image_color = crate::theme::color32_to_argb(c); app.mark_settings_dirty(); } });
            ui.horizontal(|ui| {
                if widgets::flat_button(ui, &pal, "Create", true).clicked() {
                    if let (Ok(w), Ok(h)) = (app.editor.new_w.trim().parse::<u32>(), app.editor.new_h.trim().parse::<u32>()) {
                        let col = app.settings.new_image_color;
                        let img = image::RgbaImage::from_pixel(w.clamp(1, 25000), h.clamp(1, 25000), image::Rgba([col.r, col.g, col.b, col.a]));
                        if let Ok(png) = linkshot_render::encode_png(&img) { let doc = Document::from_png(img.width(), img.height(), png); app.editor.add_document(ctx, doc, &app.paths, &app.settings); }
                        app.settings.new_image_width = w; app.settings.new_image_height = h; app.mark_settings_dirty();
                    }
                    app.editor.new_image_dialog = false;
                }
                if ui.button("Cancel").clicked() { app.editor.new_image_dialog = false; }
            });
        });
    }
    if app.editor.canvas_color_dialog {
        egui::Window::new("Canvas Color").collapsible(false).resizable(false).show(ctx, |ui| {
            let mut c = app.editor.current_doc().map(|d| crate::theme::argb_to_color32(d.doc.page().capture_background_color)).unwrap_or(Color32::TRANSPARENT);
            ui.horizontal(|ui| { ui.label("Color"); if ui.color_edit_button_srgba(&mut c).changed() { if let Some(d) = app.editor.current_doc_mut() { let a = crate::theme::color32_to_argb(c); d.edit("Canvas Color", move |doc| doc.page_mut().capture_background_color = a); } } });
            ui.horizontal(|ui| { if ui.button("Transparent").clicked() { if let Some(d) = app.editor.current_doc_mut() { d.edit("Canvas Color", |doc| doc.page_mut().capture_background_color = linkshot_core::Argb::TRANSPARENT); } } if ui.button("Close").clicked() { app.editor.canvas_color_dialog = false; } });
        });
    }
    if app.editor.size_popup {
        let mut open = true;
        egui::Window::new("Resize Image").collapsible(false).resizable(false).open(&mut open).anchor(Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| { resize_popup(app, ui); });
        if !open || app.editor.current.is_none() { app.editor.size_popup = false; }
    }
    if app.show_about {
        egui::Window::new("About LinkShot").collapsible(false).resizable(false).show(ctx, |ui| {
            ui.heading("LinkShot");
            ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
            ui.label("Screen capture, scrolling capture, video recording and an image editor for Linux and Windows.");
            ui.hyperlink("https://github.com/ZeroDay-Labz/linkshot");
            if let Some(e) = &app.hotkeys.error { ui.label(egui::RichText::new(e).small().color(pal.input_secondary)); }
            if ui.button("Close").clicked() { app.show_about = false; }
        });
    }
    if app.show_prefs { preferences_window(app, ctx, actions); }
}

fn preferences_window(app: &mut App, ctx: &egui::Context, _actions: &mut Vec<Action>) {
    let mut default_tool_action: Option<bool> = None;
    let mut open = true;
    let mut changed = false;
    let mut theme_changed = false;
    let mut rebind = false;
    let mut ui_scale_changed = false;
    let mut hotkey_warning: Option<String> = None;
    let pal = app.pal;
    egui::Window::new("LinkShot Editor Preferences").open(&mut open).default_width(520.0).show(ctx, |ui| {
        let s = &mut app.settings;
        ui.horizontal(|ui| {
            ui.label("Editor theme:");
            theme_changed |= ui.selectable_value(&mut s.ui_theme, linkshot_core::settings::UiTheme::Light, "Light").changed();
            theme_changed |= ui.selectable_value(&mut s.ui_theme, linkshot_core::settings::UiTheme::Dark, "Dark").changed();
        });
        ui.horizontal(|ui| {
            ui.label("Interface scale:");
            let mut pct = (s.ui_scale * 100.0).round();
            if ui.add(egui::Slider::new(&mut pct, 60.0..=250.0).suffix("%").step_by(5.0)).changed() { s.ui_scale = pct / 100.0; changed = true; ui_scale_changed = true; }
            if ui.button("Reset").clicked() { s.ui_scale = 1.0; changed = true; ui_scale_changed = true; }
        });
        ui.collapsing("Capture", |ui| {
            changed |= ui.checkbox(&mut s.hide_linkshot_during_capture, "Hide LinkShot during capture and remove LinkShot controls from completed captures").changed();
            changed |= ui.checkbox(&mut s.show_magnifier, "Show magnifier").changed();
            changed |= ui.checkbox(&mut s.simplified_crosshairs, "Simplified crosshairs").changed();
            changed |= ui.checkbox(&mut s.show_video_guides, "Show video guides").changed();
            changed |= ui.checkbox(&mut s.run_at_startup, "Run LinkShot when the system starts").changed();
            ui.label(egui::RichText::new("Hotkeys").strong());
            ui.label(egui::RichText::new(format!("Capture: {} (set it under the Capture button)", app.presets.global_hot_key.display())).small());
            let editing = &mut app.prefs_hotkey_editing;
            egui::Grid::new("hotkeys-grid").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                for (label, id, hk) in [
                    ("Show / hide LinkShot", "show-hide", &mut s.hotkey_show_hide),
                    ("Repeat last capture", "repeat", &mut s.hotkey_repeat_last),
                    ("Video start / pause / resume", "video-start", &mut s.hotkey_video_start_pause),
                    ("Video stop", "video-stop", &mut s.hotkey_video_stop),
                    ("Mute / unmute microphone", "mute-mic", &mut s.hotkey_mute_mic),
                    ("Enter / exit Screen Draw", "draw-mode", &mut s.hotkey_draw_mode),
                ] {
                    ui.label(label);
                    let (c, warn) = crate::widgets::hotkey_editor(ui, &pal, hk, editing, id, false);
                    if c { changed = true; rebind = true; }
                    if let Some(w) = warn { hotkey_warning = Some(w); }
                    ui.end_row();
                }
            });
            if let Some(w) = &hotkey_warning { ui.label(egui::RichText::new(w).small().color(pal.capture_red)); }
            if let Some(e) = &app.hotkeys.error { ui.label(egui::RichText::new(e).small()); }
            ui.label(egui::RichText::new("Default screenshot tool").strong());
            if s.system_print_screen {
                ui.label("Print Screen starts a LinkShot capture system-wide (Fn+PrtSc on compact keyboards).");
                if ui.button("Give Print Screen back to the desktop").clicked() { default_tool_action = Some(false); }
            } else {
                ui.label("Print Screen is handled by the desktop's own screenshot tool; LinkShot only gets it while it runs and the desktop allows it.");
                if ui.button("Make LinkShot the default screenshot tool").clicked() { default_tool_action = Some(true); }
            }
        });
        ui.collapsing("Video", |ui| {
            ui.horizontal(|ui| {
                ui.label("Save recordings as:");
                let mut gif = s.default_video_format.eq_ignore_ascii_case("gif");
                if ui.radio_value(&mut gif, true, "Animated GIF").changed() || ui.radio_value(&mut gif, false, "MP4").changed() { s.default_video_format = if gif { "GIF".into() } else { "MP4".into() }; changed = true; }
            });
            ui.horizontal(|ui| {
                ui.label("Recording quality:");
                for (q, name) in [(linkshot_core::settings::VideoQuality::Low, "Low (10 fps)"), (linkshot_core::settings::VideoQuality::Medium, "Medium (20 fps)"), (linkshot_core::settings::VideoQuality::High, "High (30 fps)")] {
                    changed |= ui.selectable_value(&mut s.video_quality, q, name).changed();
                }
            });
            changed |= ui.checkbox(&mut s.show_ready_to_record, "Show Ready to Record (press Record to start)").changed();
            changed |= ui.checkbox(&mut s.show_video_countdown, "Show countdown before recording").changed();
            changed |= ui.checkbox(&mut s.scale_video_to_1080p, "Scale down recordings larger than 1920×1080").changed();
            changed |= ui.checkbox(&mut s.minimize_fullscreen_controls, "Minimize the recording controls when recording a whole monitor").changed();
            changed |= ui.checkbox(&mut s.enable_webcam, "Enable webcam recording").changed();
            changed |= ui.checkbox(&mut s.mic_muted_alert, "Show an alert when the microphone is muted while recording").changed();
            changed |= ui.checkbox(&mut s.auto_save_videos, "Automatically save new recordings in the Library").changed();
            ui.horizontal(|ui| { ui.label("MP4 quality (CRF, lower is better):"); changed |= ui.add(egui::Slider::new(&mut s.mp4_crf, 10..=30)).changed(); });
            match linkshot_video::ffmpeg::find_ffmpeg() { Some(p) => ui.label(egui::RichText::new(format!("MP4 export and opening MP4 files use ffmpeg at {}", p.display())).small()), None => ui.label(egui::RichText::new("ffmpeg was not found: MP4 export and opening MP4 files are unavailable until it is installed (GIF works without it).").small().color(app.pal.capture_red)) };
        });
        ui.collapsing("Editor", |ui| {
            let mut c = crate::theme::argb_to_color32(s.editor_workspace_color);
            ui.horizontal(|ui| { ui.label("Editor Workspace Color"); if ui.color_edit_button_srgba(&mut c).changed() { s.editor_workspace_color = crate::theme::color32_to_argb(c); changed = true; } });
            ui.horizontal(|ui| { ui.label("Recent files thumbnail size"); for (sz, name) in [(TrayThumbnailSize::Small, "Small"), (TrayThumbnailSize::Medium, "Medium"), (TrayThumbnailSize::Large, "Large")] { changed |= ui.selectable_value(&mut s.tray_thumbnail_size, sz, name).changed(); } });
            changed |= ui.checkbox(&mut s.auto_expand_canvas, "Automatically expand the canvas to fit objects").changed();
            changed |= ui.checkbox(&mut s.escape_closes_editor, "Escape key closes Editor").changed();
            changed |= ui.checkbox(&mut s.show_toolbar_labels, "Show Toolbar Labels").changed();
            changed |= ui.checkbox(&mut s.canvas_snapping, "Canvas snapping").changed();
        });
        ui.collapsing("Share", |ui| {
            changed |= ui.checkbox(&mut s.disable_contextual_file_name, "Do not include capture source in shared file names and titles").changed();
            changed |= ui.checkbox(&mut s.remember_last_save_format, "Remember last used Save As format").changed();
            ui.horizontal(|ui| { ui.label("Always use this file format:"); for (_, ext) in crate::share::IMAGE_FORMATS { let up = ext.to_ascii_uppercase(); changed |= ui.selectable_value(&mut s.default_save_format, up.clone(), up).changed(); } });
            ui.horizontal(|ui| { ui.label("Drag and drop file type:"); for ext in ["png", "jpg", "snagx"] { changed |= ui.selectable_value(&mut s.drag_drop_file_type, ext.to_string(), ext).changed(); } });
        });
        ui.collapsing("Library", |ui| {
            let mut n = s.tray_max_items as i32;
            if ui.add(egui::Slider::new(&mut n, 1..=200).text("Recent captures in tray")).changed() { s.tray_max_items = n as u32; changed = true; }
            changed |= ui.checkbox(&mut s.auto_save_images, "Automatically save new image captures in the Library").changed();
            changed |= ui.checkbox(&mut s.prompt_for_delete, "Prompt before deleting files").changed();
            changed |= ui.checkbox(&mut s.remove_unpinned_on_close, "Remove unpinned items from the Tray when closing Editor").changed();
            ui.horizontal(|ui| {
                ui.label(format!("Library location: {}", s.library_dir(&app.paths).display()));
                if ui.button("Choose location...").clicked() { if let Some(d) = rfd::FileDialog::new().pick_folder() { s.library_directory = Some(d); changed = true; } }
            });
        });
        ui.collapsing("Advanced", |ui| {
            changed |= ui.checkbox(&mut s.keep_editor_running, "Always keep Editor running in the background").changed();
            changed |= ui.checkbox(&mut s.delete_cropped_content, "Delete cropped content").changed();
            changed |= ui.checkbox(&mut s.auto_flatten_paste, "Automatically flatten pasted images").changed();
            if ui.button("Reset to Default Settings").clicked() { *s = Settings::default(); changed = true; theme_changed = true; }
        });
    });
    if theme_changed { app.apply_theme(ctx); changed = true; let _ = linkshot_platform::autostart::set_enabled(app.settings.run_at_startup); }
    if changed { app.mark_settings_dirty(); let _ = linkshot_platform::autostart::set_enabled(app.settings.run_at_startup); }
    if !open { app.show_prefs = false; }
    match default_tool_action { Some(true) => app.make_default_tool(), Some(false) => app.undo_default_tool(), None => {} }
    if rebind { app.register_hotkeys(); }
    if ui_scale_changed { app.apply_ui_scale(ctx); }
}
