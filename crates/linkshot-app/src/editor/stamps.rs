//! The Stamp tool: a library of small images dropped onto the canvas as movable Image objects.
//! A built-in set (check, cross, warning, star, heart, …) is drawn at start-up; the user's own
//! PNG/JPG/WebP files in the stamps folder (`<data>/stamps`) are listed after them.

use crate::app::App;
use linkshot_core::document::CaptureObject;
use linkshot_core::enums::ToolMode;
use linkshot_core::Pt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Stroke, Transform};

#[derive(Clone)]
pub struct Stamp {
    pub name: String,
    pub png: Arc<Vec<u8>>,
    pub width: u32,
    pub height: u32,
    pub path: Option<PathBuf>,
}

#[derive(Default)]
pub struct StampLibrary {
    pub stamps: Vec<Stamp>,
    textures: Vec<Option<egui::TextureHandle>>,
    loaded: bool,
}

impl StampLibrary {
    /// Built-in stamps followed by the files in `dir` (re-read when `force`).
    pub fn ensure_loaded(&mut self, dir: &Path, force: bool) {
        if self.loaded && !force { return; }
        self.loaded = true;
        let mut v = builtin().clone();
        v.extend(user_stamps(dir));
        self.stamps = v;
        self.textures = vec![];
    }

    pub fn texture(&mut self, ctx: &egui::Context, i: usize) -> Option<egui::TextureId> {
        if self.textures.len() != self.stamps.len() { self.textures = (0..self.stamps.len()).map(|_| None).collect(); }
        if self.textures.get(i)?.is_none() {
            let s = &self.stamps[i];
            let img = image::load_from_memory(&s.png).ok()?.to_rgba8();
            let thumb = if img.width() > 96 || img.height() > 96 { image::imageops::thumbnail(&img, 96, 96) } else { img };
            let ci = egui::ColorImage::from_rgba_unmultiplied([thumb.width() as usize, thumb.height() as usize], thumb.as_raw());
            self.textures[i] = Some(ctx.load_texture(format!("stamp-{i}"), ci, egui::TextureOptions::LINEAR));
        }
        self.textures[i].as_ref().map(|t| t.id())
    }
}

/// Reads the user's stamp files (PNG/JPG/WebP/BMP) from `dir`.
pub fn user_stamps(dir: &Path) -> Vec<Stamp> {
    let mut out = vec![];
    let Ok(rd) = std::fs::read_dir(dir) else { return out };
    let mut files: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().and_then(|e| e.to_str()).map(|e| matches!(e.to_ascii_lowercase().as_str(), "png" | "jpg" | "jpeg" | "webp" | "bmp")).unwrap_or(false)).collect();
    files.sort();
    for p in files {
        let Ok(img) = image::open(&p) else { continue };
        let img = img.to_rgba8();
        let Ok(png) = linkshot_render::encode_png(&img) else { continue };
        out.push(Stamp { name: p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(), png: Arc::new(png), width: img.width(), height: img.height(), path: Some(p) });
    }
    out
}

/// Copies image files into the stamps folder (the "Add stamps…" button).
pub fn import_files(dir: &Path, files: &[PathBuf]) -> usize {
    let _ = std::fs::create_dir_all(dir);
    let mut n = 0;
    for f in files {
        let Some(name) = f.file_name() else { continue };
        if std::fs::copy(f, dir.join(name)).is_ok() { n += 1; }
    }
    n
}

/// Places stamp `i` centred at `at` (document coordinates) as an Image object sized by the tool's
/// Size property, and selects it.
pub fn insert_stamp(app: &mut App, i: usize, at: Pt) {
    let Some(s) = app.editor.stamps.stamps.get(i).cloned() else { return };
    if app.editor.current.is_none() { app.editor.set_status("Open or capture an image first."); return; }
    use base64::Engine as _;
    let b64 = base64::engine::general_purpose::STANDARD.encode(s.png.as_slice());
    let q = app.editor.props.get(&ToolMode::Stamp).cloned().unwrap_or_else(|| linkshot_core::style::QuickStyle::for_tool(ToolMode::Stamp));
    let size = q.stroke_width.clamp(8.0, 2048.0);
    let aspect = s.width as f64 / s.height.max(1) as f64;
    let (w, h) = if aspect >= 1.0 { (size, size / aspect) } else { (size * aspect, size) };
    let mut o = CaptureObject::new("Image");
    o.points_array = vec![Pt::new(at.x - w / 2.0, at.y - h / 2.0), Pt::new(at.x + w / 2.0, at.y + h / 2.0)];
    o.image_data = Some(b64);
    o.aspect_ratio = Some(aspect);
    let id = o.object_id.clone();
    if let Some(d) = app.editor.current_doc_mut() { d.edit("Stamp", |doc| doc.page_mut().capture_objects.push(o)); }
    app.editor.tools.selection = vec![id];
    app.editor.renderer.invalidate();
}

// ------------------------------------------------------------------ built-in set

pub fn builtin() -> &'static Vec<Stamp> {
    static SET: OnceLock<Vec<Stamp>> = OnceLock::new();
    SET.get_or_init(|| {
        let mut v = vec![];
        let mut add = |name: &str, draw: &dyn Fn(&mut Pixmap)| {
            let mut pm = Pixmap::new(SIZE, SIZE).expect("pixmap");
            draw(&mut pm);
            if let Ok(png) = pm.encode_png() { v.push(Stamp { name: name.into(), png: Arc::new(png), width: SIZE, height: SIZE, path: None }); }
        };
        add("Check", &|pm| { disc(pm, GREEN); check(pm, WHITE); });
        add("Cross", &|pm| { disc(pm, RED); cross(pm, WHITE); });
        add("Plus", &|pm| { disc(pm, GREEN); plus(pm, WHITE, true); });
        add("Minus", &|pm| { disc(pm, RED); plus(pm, WHITE, false); });
        add("Exclamation", &|pm| { disc(pm, RED); bang(pm, WHITE); });
        add("Info", &|pm| { disc(pm, BLUE); info(pm, WHITE); });
        add("Question", &|pm| { disc(pm, BLUE); question(pm, WHITE); });
        add("Warning", &|pm| { triangle(pm, YELLOW); bang(pm, BLACK); });
        add("Star", &|pm| star(pm, YELLOW));
        add("Heart", &|pm| heart(pm, RED));
        add("Arrow", &|pm| { disc(pm, BLUE); chevron(pm, WHITE); });
        add("Flag", &|pm| flag(pm, RED));
        add("Pin", &|pm| pin(pm, RED));
        add("Lightning", &|pm| lightning(pm, YELLOW));
        v
    })
}

const SIZE: u32 = 128;
const GREEN: [u8; 4] = [0x2E, 0xC2, 0x6A, 255];
const RED: [u8; 4] = [0xE8, 0x3A, 0x3A, 255];
const BLUE: [u8; 4] = [0x2C, 0x7C, 0xB2, 255];
const YELLOW: [u8; 4] = [0xFB, 0xB9, 0x01, 255];
const WHITE: [u8; 4] = [255, 255, 255, 255];
const BLACK: [u8; 4] = [0x20, 0x20, 0x20, 255];

fn paint(c: [u8; 4]) -> Paint<'static> { let mut p = Paint::default(); p.set_color_rgba8(c[0], c[1], c[2], c[3]); p.anti_alias = true; p }
fn fill(pm: &mut Pixmap, path: &tiny_skia::Path, c: [u8; 4]) { pm.fill_path(path, &paint(c), FillRule::Winding, Transform::identity(), None); }
fn stroke(pm: &mut Pixmap, path: &tiny_skia::Path, c: [u8; 4], width: f32) {
    let s = Stroke { width, line_cap: tiny_skia::LineCap::Round, line_join: tiny_skia::LineJoin::Round, ..Default::default() };
    pm.stroke_path(path, &paint(c), &s, Transform::identity(), None);
}
fn s(v: f32) -> f32 { v * SIZE as f32 / 128.0 }

fn disc(pm: &mut Pixmap, c: [u8; 4]) {
    let p = PathBuilder::from_circle(s(64.0), s(64.0), s(60.0)).unwrap();
    fill(pm, &p, [0, 0, 0, 40]);
    let p = PathBuilder::from_circle(s(64.0), s(62.0), s(58.0)).unwrap();
    fill(pm, &p, c);
}
fn check(pm: &mut Pixmap, c: [u8; 4]) {
    let mut b = PathBuilder::new();
    b.move_to(s(36.0), s(66.0)); b.line_to(s(56.0), s(86.0)); b.line_to(s(94.0), s(42.0));
    stroke(pm, &b.finish().unwrap(), c, s(13.0));
}
fn cross(pm: &mut Pixmap, c: [u8; 4]) {
    let mut b = PathBuilder::new();
    b.move_to(s(42.0), s(40.0)); b.line_to(s(86.0), s(84.0)); b.move_to(s(86.0), s(40.0)); b.line_to(s(42.0), s(84.0));
    stroke(pm, &b.finish().unwrap(), c, s(13.0));
}
fn plus(pm: &mut Pixmap, c: [u8; 4], vertical: bool) {
    let mut b = PathBuilder::new();
    b.move_to(s(36.0), s(62.0)); b.line_to(s(92.0), s(62.0));
    if vertical { b.move_to(s(64.0), s(34.0)); b.line_to(s(64.0), s(90.0)); }
    stroke(pm, &b.finish().unwrap(), c, s(14.0));
}
fn bang(pm: &mut Pixmap, c: [u8; 4]) {
    let mut b = PathBuilder::new();
    b.move_to(s(64.0), s(34.0)); b.line_to(s(64.0), s(72.0));
    stroke(pm, &b.finish().unwrap(), c, s(14.0));
    fill(pm, &PathBuilder::from_circle(s(64.0), s(92.0), s(8.0)).unwrap(), c);
}
fn info(pm: &mut Pixmap, c: [u8; 4]) {
    fill(pm, &PathBuilder::from_circle(s(64.0), s(36.0), s(8.0)).unwrap(), c);
    let mut b = PathBuilder::new();
    b.move_to(s(64.0), s(54.0)); b.line_to(s(64.0), s(92.0));
    stroke(pm, &b.finish().unwrap(), c, s(14.0));
}
fn question(pm: &mut Pixmap, c: [u8; 4]) {
    let mut b = PathBuilder::new();
    b.move_to(s(46.0), s(50.0));
    b.cubic_to(s(46.0), s(28.0), s(84.0), s(28.0), s(84.0), s(50.0));
    b.cubic_to(s(84.0), s(64.0), s(64.0), s(62.0), s(64.0), s(78.0));
    stroke(pm, &b.finish().unwrap(), c, s(12.0));
    fill(pm, &PathBuilder::from_circle(s(64.0), s(96.0), s(7.5)).unwrap(), c);
}
fn triangle(pm: &mut Pixmap, c: [u8; 4]) {
    let mut b = PathBuilder::new();
    b.move_to(s(64.0), s(10.0)); b.line_to(s(122.0), s(112.0)); b.line_to(s(6.0), s(112.0)); b.close();
    let p = b.finish().unwrap();
    fill(pm, &p, c);
    stroke(pm, &p, c, s(10.0));
}
fn star(pm: &mut Pixmap, c: [u8; 4]) {
    let mut b = PathBuilder::new();
    for i in 0..10 {
        let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
        let r = if i % 2 == 0 { s(60.0) } else { s(26.0) };
        let (x, y) = (s(64.0) + r * a.cos(), s(66.0) + r * a.sin());
        if i == 0 { b.move_to(x, y); } else { b.line_to(x, y); }
    }
    b.close();
    let p = b.finish().unwrap();
    fill(pm, &p, c);
    stroke(pm, &p, c, s(6.0));
}
fn heart(pm: &mut Pixmap, c: [u8; 4]) {
    let mut b = PathBuilder::new();
    b.move_to(s(64.0), s(112.0));
    b.cubic_to(s(10.0), s(72.0), s(14.0), s(20.0), s(44.0), s(20.0));
    b.cubic_to(s(56.0), s(20.0), s(62.0), s(30.0), s(64.0), s(36.0));
    b.cubic_to(s(66.0), s(30.0), s(72.0), s(20.0), s(84.0), s(20.0));
    b.cubic_to(s(114.0), s(20.0), s(118.0), s(72.0), s(64.0), s(112.0));
    b.close();
    fill(pm, &b.finish().unwrap(), c);
}
fn chevron(pm: &mut Pixmap, c: [u8; 4]) {
    let mut b = PathBuilder::new();
    b.move_to(s(34.0), s(62.0)); b.line_to(s(90.0), s(62.0)); b.move_to(s(66.0), s(38.0)); b.line_to(s(92.0), s(62.0)); b.line_to(s(66.0), s(86.0));
    stroke(pm, &b.finish().unwrap(), c, s(13.0));
}
fn flag(pm: &mut Pixmap, c: [u8; 4]) {
    let mut b = PathBuilder::new();
    b.move_to(s(30.0), s(14.0)); b.line_to(s(30.0), s(118.0));
    stroke(pm, &b.finish().unwrap(), BLACK, s(8.0));
    let mut b = PathBuilder::new();
    b.move_to(s(34.0), s(16.0)); b.line_to(s(110.0), s(16.0)); b.line_to(s(90.0), s(42.0)); b.line_to(s(110.0), s(68.0)); b.line_to(s(34.0), s(68.0)); b.close();
    fill(pm, &b.finish().unwrap(), c);
}
fn pin(pm: &mut Pixmap, c: [u8; 4]) {
    let mut b = PathBuilder::new();
    b.move_to(s(64.0), s(120.0));
    b.cubic_to(s(64.0), s(120.0), s(22.0), s(74.0), s(22.0), s(50.0));
    b.cubic_to(s(22.0), s(26.0), s(42.0), s(10.0), s(64.0), s(10.0));
    b.cubic_to(s(86.0), s(10.0), s(106.0), s(26.0), s(106.0), s(50.0));
    b.cubic_to(s(106.0), s(74.0), s(64.0), s(120.0), s(64.0), s(120.0));
    b.close();
    fill(pm, &b.finish().unwrap(), c);
    fill(pm, &PathBuilder::from_circle(s(64.0), s(50.0), s(16.0)).unwrap(), WHITE);
}
fn lightning(pm: &mut Pixmap, c: [u8; 4]) {
    let mut b = PathBuilder::new();
    b.move_to(s(72.0), s(6.0)); b.line_to(s(28.0), s(72.0)); b.line_to(s(60.0), s(72.0)); b.line_to(s(50.0), s(122.0)); b.line_to(s(100.0), s(52.0)); b.line_to(s(68.0), s(52.0)); b.close();
    let p = b.finish().unwrap();
    fill(pm, &p, c);
    stroke(pm, &p, BLACK, s(3.0));
}

#[cfg(test)]
mod tests {
    #[test]
    fn builtin_stamps_decode() {
        let set = super::builtin();
        assert!(set.len() >= 12);
        for s in set { let img = image::load_from_memory(&s.png).unwrap().to_rgba8(); assert_eq!((img.width(), img.height()), (super::SIZE, super::SIZE), "{}", s.name); assert!(img.pixels().any(|p| p.0[3] > 0), "{} is empty", s.name); }
    }
}
