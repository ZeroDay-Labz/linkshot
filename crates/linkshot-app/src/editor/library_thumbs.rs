//! Library thumbnails and the background library scan, so a folder with thousands of captures
//! opens instantly: the scan reads only metadata on a worker thread, and thumbnails are made (and
//! cached as small PNGs under the cache dir) one at a time as cards scroll into view.

use crossbeam_channel::{unbounded, Receiver, Sender};
use image::RgbaImage;
use linkshot_core::library::{LibraryEntry, LibraryIndex};
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const THUMB_W: u32 = 176;
pub const THUMB_H: u32 = 100;

pub struct LibraryThumbs {
    tex: HashMap<PathBuf, Option<egui::TextureHandle>>,
    pending: HashSet<PathBuf>,
    tx: Sender<(PathBuf, u64, PathBuf)>,
    rx: Receiver<(PathBuf, Option<RgbaImage>)>,
}

impl LibraryThumbs {
    pub fn new(wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        let (tx, jobs) = unbounded::<(PathBuf, u64, PathBuf)>();
        let (done, rx) = unbounded::<(PathBuf, Option<RgbaImage>)>();
        std::thread::Builder::new().name("linkshot-thumbs".into()).spawn(move || {
            for (path, key, cache_dir) in jobs {
                let img = make_thumb(&path, key, &cache_dir);
                if done.send((path, img)).is_err() { break; }
                wake();
            }
        }).ok();
        Self { tex: HashMap::new(), pending: HashSet::new(), tx, rx }
    }

    /// Drops cached textures of files that are no longer in the index (after a rescan).
    pub fn retain(&mut self, index: &LibraryIndex) { self.tex.retain(|p, _| index.entries.contains_key(p)); }

    /// The thumbnail texture for `e`, requesting it in the background the first time.
    pub fn texture(&mut self, ctx: &egui::Context, e: &LibraryEntry, cache_dir: &Path) -> Option<egui::TextureId> {
        while let Ok((path, img)) = self.rx.try_recv() {
            self.pending.remove(&path);
            let handle = img.map(|img| ctx.load_texture(format!("lib-{}", path.display()), egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw()), egui::TextureOptions::LINEAR));
            self.tex.insert(path, handle);
        }
        if let Some(t) = self.tex.get(&e.path) { return t.as_ref().map(|h| h.id()); }
        if self.pending.insert(e.path.clone()) {
            let _ = self.tx.send((e.path.clone(), thumb_key(e), cache_dir.to_path_buf()));
        }
        None
    }
}

fn thumb_key(e: &LibraryEntry) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    e.path.hash(&mut h);
    e.modified.timestamp_millis().hash(&mut h);
    e.size.hash(&mut h);
    h.finish()
}

/// Builds (or loads from the cache) a thumbnail that fits `THUMB_W`×`THUMB_H`.
fn make_thumb(path: &Path, key: u64, cache_dir: &Path) -> Option<RgbaImage> {
    let cached = cache_dir.join(format!("{key:016x}.png"));
    if let Ok(img) = image::open(&cached) { return Some(img.to_rgba8()); }
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let full = match ext.as_str() {
        "snagx" => {
            let sum = linkshot_core::snagx::read_summary(path).ok()?;
            match sum.thumbnail_png.as_deref().and_then(|b| image::load_from_memory(b).ok()) {
                Some(t) => t.to_rgba8(),
                // Older files without a stored thumbnail: render from the document once.
                None => { let doc = linkshot_core::snagx::read_file(path).ok()?; let mut r = linkshot_render::Renderer::default(); r.thumbnail(&doc, doc.page(), THUMB_W * 2, THUMB_H * 2)? }
            }
        }
        "gif" | "mp4" | "mov" | "webm" | "mkv" => linkshot_video::import::poster(path, None).ok()?,
        _ => image::open(path).ok()?.to_rgba8(),
    };
    let (w, h) = full.dimensions();
    let k = (THUMB_W as f64 / w.max(1) as f64).min(THUMB_H as f64 / h.max(1) as f64).min(1.0);
    let (tw, th) = (((w as f64 * k).round() as u32).max(1), ((h as f64 * k).round() as u32).max(1));
    let small = if (tw, th) == (w, h) { full } else { image::imageops::resize(&full, tw, th, image::imageops::FilterType::Triangle) };
    let _ = std::fs::create_dir_all(cache_dir);
    let _ = small.save(&cached);
    Some(small)
}

/// A library scan running on a worker thread.
pub struct ScanJob { rx: Receiver<LibraryIndex> }

impl ScanJob {
    pub fn start(mut index: LibraryIndex, dir: PathBuf, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        let (tx, rx) = crossbeam_channel::bounded(1);
        std::thread::Builder::new().name("linkshot-library-scan".into()).spawn(move || {
            if let Err(e) = index.scan(&dir) { tracing::warn!("library scan {}: {e:#}", dir.display()); }
            let _ = tx.send(index);
            wake();
        }).ok();
        Self { rx }
    }
    pub fn poll(&self) -> Option<LibraryIndex> { self.rx.try_recv().ok() }
}
