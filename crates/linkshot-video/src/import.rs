//! Opening existing videos: GIF files are decoded with the `image` crate, everything else through
//! ffmpeg; both land in a spool in the cache directory so the player and exporters see one format.

use crate::source::FrameSource;
use crate::spool::{SpoolReader, SpoolWriter};
use anyhow::{bail, Context, Result};
use image::AnimationDecoder;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

/// Where the decoded spool for `path` lives in `cache_dir` (keyed by path, size and mtime).
pub fn cache_spool_path(path: &Path, cache_dir: &Path) -> PathBuf {
    let md = std::fs::metadata(path).ok();
    let mut h = DefaultHasher::new();
    path.hash(&mut h);
    md.as_ref().map(|m| m.len()).unwrap_or(0).hash(&mut h);
    md.and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis()).unwrap_or(0).hash(&mut h);
    cache_dir.join(format!("{:016x}.lsv", h.finish()))
}

/// Decodes an animated GIF into a spool.
pub fn gif_to_spool(path: &Path, spool: &Path, progress: &mut dyn FnMut(f32) -> bool) -> Result<SpoolReader> {
    let file = std::io::BufReader::new(std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?);
    let dec = image::codecs::gif::GifDecoder::new(file).context("decode GIF")?;
    let mut writer: Option<SpoolWriter> = None;
    let mut t_ms = 0u32;
    let mut n = 0u32;
    for frame in dec.into_frames() {
        let frame = frame.context("GIF frame")?;
        let (num, den) = frame.delay().numer_denom_ms();
        let delay = if den == 0 { 100 } else { (num / den).max(10) };
        let buf = frame.buffer();
        if writer.is_none() { writer = Some(SpoolWriter::create(spool, buf.width(), buf.height(), (1000 / delay).clamp(1, 50))?); }
        let w = writer.as_mut().unwrap();
        if buf.width() != w.width() || buf.height() != w.height() { bail!("GIF frames change size"); }
        w.push(t_ms, buf.as_raw())?;
        t_ms += delay;
        n += 1;
        if n % 10 == 0 && !progress(0.5) { w.abort_ref(); bail!("cancelled"); }
    }
    let Some(w) = writer else { bail!("GIF has no frames") };
    let info = w.finish()?;
    // A single-frame GIF is a still image: one frame "shown" for its delay.
    let _ = info;
    progress(1.0);
    SpoolReader::open(spool)
}

impl SpoolWriter {
    fn abort_ref(&mut self) { let _ = std::fs::remove_file(self.path()); }
}

/// Opens any supported video as a frame source. `.lsv` spools open directly; other formats are
/// decoded into `cache_dir` once and reused while the file is unchanged. `master`, when given, is a
/// lossless spool recorded alongside the file and is preferred.
pub fn open_video(path: &Path, master: Option<&Path>, cache_dir: &Path, progress: &mut dyn FnMut(f32) -> bool) -> Result<SpoolReader> {
    if let Some(m) = master { if m.is_file() { if let Ok(r) = SpoolReader::open(m) { return Ok(r); } } }
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    if ext == "lsv" { return SpoolReader::open(path); }
    std::fs::create_dir_all(cache_dir).ok();
    let spool = cache_spool_path(path, cache_dir);
    if spool.is_file() { if let Ok(r) = SpoolReader::open(&spool) { if !r.is_empty() { return Ok(r); } } }
    let tmp = spool.with_extension("part");
    let result = if ext == "gif" { gif_to_spool(path, &tmp, progress).map(|_| ()) } else { crate::ffmpeg::decode_to_spool(path, &tmp, progress).map(|_| ()) };
    match result {
        Ok(()) => {
            std::fs::rename(&tmp, &spool).context("move decoded spool")?;
            // The sound track, when the file has one, sits next to the spool so the player and the
            // MP4 exporter find it the same way as for a recording.
            if ext != "gif" {
                let wav = spool.with_extension("wav");
                match linkshot_audio::extract(path, &wav) { Ok(true) => {}, Ok(false) => { let _ = std::fs::remove_file(&wav); }, Err(e) => tracing::warn!("audio of {}: {e:#}", path.display()) }
            }
            SpoolReader::open(&spool)
        }
        Err(e) => { let _ = std::fs::remove_file(&tmp); Err(e) }
    }
}

/// The first frame of a video file (tray thumbnail / poster), without decoding the whole clip.
pub fn poster(path: &Path, master: Option<&Path>) -> Result<image::RgbaImage> {
    if let Some(m) = master { if m.is_file() { if let Ok(mut r) = SpoolReader::open(m) { if !r.is_empty() { let (w, h) = (r.width(), r.height()); let f = r.frame(0)?.to_vec(); return image::RgbaImage::from_raw(w, h, f).context("poster frame"); } } } }
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "gif" => Ok(image::open(path).with_context(|| format!("open {}", path.display()))?.to_rgba8()),
        "lsv" => { let mut r = SpoolReader::open(path)?; let (w, h) = (r.width(), r.height()); let f = r.frame(0)?.to_vec(); image::RgbaImage::from_raw(w, h, f).context("poster frame") }
        _ => { let png = crate::ffmpeg::poster_png(path)?; Ok(image::load_from_memory(&png).context("decode poster")?.to_rgba8()) }
    }
}
