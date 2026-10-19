//! Saving, exporting and sharing documents (File, Clipboard, Email, Program destinations).

use anyhow::{Context, Result};
use image::RgbaImage;
use linkshot_core::document::Document;
use linkshot_core::snagx;
use linkshot_render::Renderer;
use std::path::{Path, PathBuf};

pub const IMAGE_FORMATS: &[(&str, &str)] = &[
    ("SNAGX - LinkShot Capture File", "snagx"),
    ("PNG - Portable Network Graphics", "png"),
    ("JPG - JPEG Image", "jpg"),
    ("GIF - CompuServe GIF", "gif"),
    ("BMP - Windows Bitmap", "bmp"),
    ("TIF - Tagged Image File", "tif"),
    ("WEBP - WebP Image", "webp"),
    ("ICO - Windows Icon", "ico"),
];

pub fn format_for_extension(ext: &str) -> Option<image::ImageFormat> {
    match ext.to_ascii_lowercase().as_str() {
        "png" => Some(image::ImageFormat::Png),
        "jpg" | "jpeg" => Some(image::ImageFormat::Jpeg),
        "gif" => Some(image::ImageFormat::Gif),
        "bmp" => Some(image::ImageFormat::Bmp),
        "tif" | "tiff" => Some(image::ImageFormat::Tiff),
        "webp" => Some(image::ImageFormat::WebP),
        "ico" => Some(image::ImageFormat::Ico),
        _ => None,
    }
}

/// Flattens and writes the document to `path` in the format implied by its extension.
pub fn export(renderer: &mut Renderer, doc: &Document, path: &Path, jpeg_quality: u8) -> Result<()> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("png").to_ascii_lowercase();
    if ext == "snagx" {
        return snagx::write_file(doc, path);
    }
    let img = renderer.flatten(doc, doc.page()).context("render")?;
    if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
    let fmt = format_for_extension(&ext).unwrap_or(image::ImageFormat::Png);
    let mut file = std::io::BufWriter::new(std::fs::File::create(path)?);
    match fmt {
        image::ImageFormat::Jpeg => {
            let rgb = image::DynamicImage::ImageRgba8(img).to_rgb8();
            let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut file, jpeg_quality.clamp(1, 100));
            enc.encode_image(&rgb)?;
        }
        image::ImageFormat::Bmp => image::DynamicImage::ImageRgba8(img).to_rgb8().write_to(&mut file, fmt)?,
        _ => img.write_to(&mut file, fmt)?,
    }
    Ok(())
}

pub fn flatten(renderer: &mut Renderer, doc: &Document) -> Option<RgbaImage> {
    renderer.flatten(doc, doc.page())
}

/// Copies the flattened image to the clipboard ("Copy All").
pub fn copy_all(renderer: &mut Renderer, doc: &Document) -> Result<()> {
    let img = flatten(renderer, doc).context("render")?;
    linkshot_platform::clipboard::set_image(&img)
}

/// Native "Save As" dialog with the image format list; returns the chosen path.
pub fn save_as_dialog(default_dir: Option<&Path>, default_name: &str, default_ext: &str) -> Option<PathBuf> {
    let mut d = rfd::FileDialog::new().set_title("Save As").set_file_name(format!("{default_name}.{default_ext}"));
    if let Some(dir) = default_dir { d = d.set_directory(dir); }
    // Put the default format first.
    let mut fmts: Vec<&(&str, &str)> = IMAGE_FORMATS.iter().collect();
    fmts.sort_by_key(|(_, e)| if *e == default_ext { 0 } else { 1 });
    for (name, ext) in fmts { d = d.add_filter(*name, &[*ext]); }
    d.save_file()
}

pub const VIDEO_FORMATS: &[(&str, &str)] = &[
    ("GIF - Animated GIF", "gif"),
    ("MP4 - MPEG-4 Video", "mp4"),
];

/// "Save As" dialog for videos (GIF / MP4), default format first.
pub fn save_video_dialog(default_dir: Option<&Path>, default_name: &str, default_ext: &str) -> Option<PathBuf> {
    let mut d = rfd::FileDialog::new().set_title("Save Video As").set_file_name(format!("{default_name}.{default_ext}"));
    if let Some(dir) = default_dir { d = d.set_directory(dir); }
    let mut fmts: Vec<&(&str, &str)> = VIDEO_FORMATS.iter().collect();
    fmts.sort_by_key(|(_, e)| if *e == default_ext { 0 } else { 1 });
    for (name, ext) in fmts { d = d.add_filter(*name, &[*ext]); }
    d.save_file()
}

pub fn open_dialog(default_dir: Option<&Path>) -> Option<Vec<PathBuf>> {
    let mut d = rfd::FileDialog::new().set_title("Open")
        .add_filter("Images and videos", &["snagx", "png", "jpg", "jpeg", "gif", "bmp", "tif", "tiff", "webp", "ico", "mp4", "mov", "webm", "mkv"])
        .add_filter("Images", &["snagx", "png", "jpg", "jpeg", "bmp", "tif", "tiff", "webp", "ico"])
        .add_filter("Videos", &["gif", "mp4", "mov", "webm", "mkv"]);
    if let Some(dir) = default_dir { d = d.set_directory(dir); }
    d.pick_files()
}

/// Opens an image file (any supported format) as a document.
pub fn open_file(path: &Path) -> Result<Document> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    if ext == "snagx" {
        return snagx::read_file(path);
    }
    if linkshot_video::is_video_extension(&ext) {
        // Videos open as their first frame (poster); the editor turns the document into a player
        // when it sees the `VideoFile` marker (see `EditorState::wrap_doc`).
        let paths = linkshot_core::settings::Paths::discover();
        let master = path.file_stem().map(|st| paths.recordings_dir().join(format!("{}.lsv", st.to_string_lossy()))).filter(|p| p.is_file());
        let img = linkshot_video::import::poster(path, master.as_deref()).with_context(|| format!("open video {}", path.display()))?;
        let png = linkshot_render::encode_png(&img)?;
        let mut doc = Document::from_png(img.width(), img.height(), png);
        doc.path = Some(path.to_path_buf());
        doc.metadata.extra.insert("VideoFile".into(), serde_json::Value::String(path.to_string_lossy().into_owned()));
        return Ok(doc);
    }
    let img = image::open(path).with_context(|| format!("open {}", path.display()))?.to_rgba8();
    let png = linkshot_render::encode_png(&img)?;
    let mut doc = Document::from_png(img.width(), img.height(), png);
    // Keep the file identity: the title shows the file name and Save writes back to it.
    doc.path = Some(path.to_path_buf());
    doc.metadata.extra.insert("SourceFile".into(), serde_json::Value::String(path.to_string_lossy().into_owned()));
    Ok(doc)
}

/// Writes a temporary flattened PNG for sharing with other programs.
pub fn temp_png(renderer: &mut Renderer, doc: &Document, dir: &Path, stem: &str) -> Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("{}.png", linkshot_core::naming::sanitize(stem)));
    export(renderer, doc, &path, 90)?;
    Ok(path)
}
