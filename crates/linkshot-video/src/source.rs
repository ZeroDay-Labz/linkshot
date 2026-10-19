//! A sequence of RGBA frames with presentation timestamps.

use anyhow::Result;

/// Random-access source of RGBA frames (`width * height * 4` bytes each) with timestamps in ms.
pub trait FrameSource {
    fn width(&self) -> u32;
    fn height(&self) -> u32;
    /// Number of frames.
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool { self.len() == 0 }
    /// Presentation time of frame `i` in milliseconds from the start.
    fn pts_ms(&self, i: usize) -> u32;
    /// Total duration (the last frame is shown until this time).
    fn duration_ms(&self) -> u32;
    /// Decodes frame `i`. Sequential access is the fast path; random access may decode from the
    /// previous key frame.
    fn frame(&mut self, i: usize) -> Result<&[u8]>;
    /// Index of the frame shown at time `t_ms` (the last frame whose pts is <= t).
    fn index_at(&self, t_ms: u32) -> usize {
        let n = self.len();
        if n == 0 { return 0; }
        let (mut lo, mut hi) = (0usize, n);
        while lo + 1 < hi {
            let mid = (lo + hi) / 2;
            if self.pts_ms(mid) <= t_ms { lo = mid; } else { hi = mid; }
        }
        lo
    }
    /// All timestamps (for scrubbers).
    fn timestamps(&self) -> Vec<u32> { (0..self.len()).map(|i| self.pts_ms(i)).collect() }
}

/// Frames held in memory (tests, tiny clips).
pub struct MemorySource {
    width: u32,
    height: u32,
    frames: Vec<(u32, Vec<u8>)>,
    duration_ms: u32,
}

impl MemorySource {
    pub fn new(width: u32, height: u32) -> Self { Self { width, height, frames: vec![], duration_ms: 0 } }
    /// Appends a frame shown for `duration_ms`.
    pub fn push(&mut self, rgba: Vec<u8>, duration_ms: u32) {
        assert_eq!(rgba.len(), (self.width * self.height * 4) as usize, "frame size");
        self.frames.push((self.duration_ms, rgba));
        self.duration_ms += duration_ms.max(1);
    }
}

impl FrameSource for MemorySource {
    fn width(&self) -> u32 { self.width }
    fn height(&self) -> u32 { self.height }
    fn len(&self) -> usize { self.frames.len() }
    fn pts_ms(&self, i: usize) -> u32 { self.frames[i].0 }
    fn duration_ms(&self) -> u32 { self.duration_ms }
    fn frame(&mut self, i: usize) -> Result<&[u8]> { Ok(&self.frames[i].1) }
}

/// Resizes an RGBA frame (box/triangle filter; exact copy when the size matches).
pub fn resize_rgba(rgba: &[u8], sw: u32, sh: u32, dw: u32, dh: u32) -> Vec<u8> {
    if sw == dw && sh == dh { return rgba.to_vec(); }
    let img = image::RgbaImage::from_raw(sw, sh, rgba.to_vec()).expect("frame size");
    let filter = if dw < sw || dh < sh { image::imageops::FilterType::Triangle } else { image::imageops::FilterType::CatmullRom };
    image::imageops::resize(&img, dw.max(1), dh.max(1), filter).into_raw()
}

/// Output size for a requested `(width, height)` where 0 means "keep the aspect ratio" (both 0 =
/// source size).
pub fn output_size(sw: u32, sh: u32, width: u32, height: u32) -> (u32, u32) {
    match (width, height) {
        (0, 0) => (sw, sh),
        (w, 0) => (w.max(1), ((sh as f64 * w as f64 / sw.max(1) as f64).round() as u32).max(1)),
        (0, h) => (((sw as f64 * h as f64 / sh.max(1) as f64).round() as u32).max(1), h.max(1)),
        (w, h) => (w.max(1), h.max(1)),
    }
}
