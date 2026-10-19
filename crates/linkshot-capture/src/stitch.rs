//! Scrolling-capture stitching: joins successive frames of a scrolling region into one tall (or wide)
//! image, exactly pixel for pixel (no resampling, no blending).
//!
//! Each new frame is compared with the last accepted frame to find the integer scroll offset:
//! rows are hashed, matching rows vote for an offset, and every candidate is verified over its whole
//! overlap (nearly all informative rows must match). Rows that never change between frames (sticky
//! headers/footers, window chrome) are detected and kept only once. Frames that overlap nothing are
//! dropped rather than guessed at.

use image::RgbaImage;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis { Vertical, Horizontal }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Push {
    /// First frame accepted.
    First,
    /// Frame identical to the last one (nothing scrolled).
    NoChange,
    /// Content scrolled by this many pixels (positive = content moved towards the top/left, i.e. the
    /// user scrolled down/right); `added` new pixels were appended.
    Scrolled { axis: Axis, offset: i32, added: u32 },
    /// Frame could not be matched (scrolled too far or content changed); it was dropped.
    NoMatch,
}

struct Gray {
    w: usize,
    h: usize,
    px: Vec<u8>,
}

impl Gray {
    fn from(img: &RgbaImage) -> Gray {
        let (w, h) = (img.width() as usize, img.height() as usize);
        let mut px = Vec::with_capacity(w * h);
        for p in img.pixels() { px.push(((p[0] as u32 * 77 + p[1] as u32 * 151 + p[2] as u32 * 28) >> 8) as u8); }
        Gray { w, h, px }
    }
    #[inline] fn row(&self, y: usize) -> &[u8] { &self.px[y * self.w..(y + 1) * self.w] }
    /// Transposed copy (columns become rows) so the same row logic serves horizontal scrolling.
    fn transposed(&self) -> Gray {
        let mut px = vec![0u8; self.w * self.h];
        for y in 0..self.h { for x in 0..self.w { px[x * self.h + y] = self.px[y * self.w + x]; } }
        Gray { w: self.h, h: self.w, px }
    }
}

/// Mean absolute difference between two rows over sampled columns in the central 2..94 % band
/// (skips scrollbars on the right and a few px on the left).
fn row_diff(a: &[u8], b: &[u8]) -> f32 {
    let w = a.len();
    let x0 = w * 2 / 100;
    let x1 = (w * 94 / 100).max(x0 + 1);
    let mut sum = 0u32;
    for x in x0..x1 { sum += (a[x] as i32 - b[x] as i32).unsigned_abs(); }
    sum as f32 / (x1 - x0) as f32
}

fn row_hash(row: &[u8]) -> u64 {
    let w = row.len();
    let x0 = w * 2 / 100;
    let x1 = (w * 94 / 100).max(x0 + 1);
    let mut h: u64 = 0xcbf29ce484222325;
    for &v in &row[x0..x1] { h ^= v as u64; h = h.wrapping_mul(0x100000001b3); }
    h
}

/// Result of matching one frame against the previous one.
#[derive(Clone, Copy, Debug)]
struct Match {
    /// dy > 0 means cur[y] == prev[y + dy] (content moved up).
    dy: i32,
    overlap: usize,
    /// Weighted number of informative rows of `cur` (in the overlap) that match `prev` at `dy`.
    support: f32,
}

/// Largest row class (number of identical rows in the previous frame) that still carries positional
/// information. Blank rows and repeating patterns occur far more often and are ignored.
const MAX_CLASS: usize = 6;
/// Rows whose mean absolute difference is at most this still count as identical (lets anti-aliasing
/// jitter through without accepting real content differences).
const ROW_TOL: f32 = 2.0;

/// Finds the vertical offset between `prev` and `cur` within rows `[top, bottom)`.
///
/// Candidate offsets come from rows voting by hash; each candidate is then verified over its whole
/// overlap: nearly every informative row of `cur` has to be found in `prev` at that offset. Among the
/// candidates that pass, the one with the most matching rows wins; near-ties (periodic content) are
/// resolved by closeness to `prior`, the offset measured for the previous frame.
fn find_offset(prev: &Gray, cur: &Gray, top: usize, bottom: usize, prior: Option<i32>) -> Option<Match> {
    let band = bottom.saturating_sub(top);
    if band < 16 || prev.w != cur.w { return None; }
    let prev_hash: Vec<u64> = (0..prev.h).map(|y| if y >= top && y < bottom { row_hash(prev.row(y)) } else { 0 }).collect();
    let mut prev_rows: HashMap<u64, Vec<usize>> = HashMap::new();
    for y in top..bottom { prev_rows.entry(prev_hash[y]).or_default().push(y); }
    let cur_hash: Vec<u64> = (0..cur.h).map(|y| if y >= top && y < bottom { row_hash(cur.row(y)) } else { 0 }).collect();
    let mut cur_count: HashMap<u64, usize> = HashMap::new();
    for y in top..bottom { *cur_count.entry(cur_hash[y]).or_default() += 1; }
    // Weight of a cur row: 1/n where n is how often its class occurs (in prev, or in cur for rows prev
    // does not contain — new content, which can never match); frequent rows (blank space, uniform
    // fills) carry no positional information and weigh nothing.
    let weight = |y: usize| -> f32 {
        let n = match prev_rows.get(&cur_hash[y]) { Some(ys) => ys.len().max(cur_count[&cur_hash[y]]), None => cur_count[&cur_hash[y]] };
        if n <= MAX_CLASS { 1.0 / n as f32 } else { 0.0 }
    };
    let mut votes: HashMap<i32, f32> = HashMap::new();
    for y in top..bottom {
        if let Some(ys) = prev_rows.get(&cur_hash[y]) {
            if ys.len() > MAX_CLASS { continue; }
            let wgt = 1.0 / ys.len() as f32;
            for py in ys { *votes.entry(*py as i32 - y as i32).or_default() += wgt; }
        }
    }
    if votes.is_empty() { return blank_overlap(&prev_hash, &cur_hash, top, bottom); }
    let mut cands: Vec<(i32, f32)> = votes.into_iter().collect();
    cands.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.abs().cmp(&b.0.abs())));
    let mut dys: Vec<i32> = cands.iter().take(16).map(|c| c.0).collect();
    if let Some(p) = prior { if !dys.contains(&p) && cands.iter().any(|c| c.0 == p) { dys.push(p); } }

    let mut passed: Vec<Match> = Vec::new();
    for dy in dys {
        let (cy0, cy1) = if dy >= 0 { (top, bottom.saturating_sub(dy as usize)) } else { (top + (-dy) as usize, bottom) };
        if cy1 <= cy0 { continue; }
        let overlap = cy1 - cy0;
        if overlap < 8 { continue; }
        let (mut support, mut total, mut bad_rows) = (0.0f32, 0.0f32, 0usize);
        for y in cy0..cy1 {
            let py = (y as i32 + dy) as usize;
            let same = cur_hash[y] == prev_hash[py] || row_diff(cur.row(y), prev.row(py)) <= ROW_TOL;
            let w = weight(y);
            total += w;
            if same { support += w; } else { bad_rows += 1; }
        }
        // Evidence: enough informative rows, nearly all of them matching, and not too much of the
        // overlap disagreeing (a tooltip or caret may change a few rows; a wrong offset changes many).
        if support < 2.0 { continue; }
        if support / total < 0.6 { continue; }
        if bad_rows as f32 > overlap as f32 * 0.35 { continue; }
        // Weak evidence (a single text line or two): then the whole overlap has to agree.
        if support < 6.0 && bad_rows > 0 { continue; }
        passed.push(Match { dy, overlap, support });
    }
    if std::env::var_os("LINKSHOT_STITCH_DEBUG").is_some() { eprintln!("  cands={:?} passed={:?}", cands.iter().take(8).collect::<Vec<_>>(), passed); }
    if passed.is_empty() { return blank_overlap(&prev_hash, &cur_hash, top, bottom); }
    let best_support = passed.iter().map(|m| m.support).fold(0.0f32, f32::max);
    // Near-ties: prefer the offset closest to the previous one (steady scrolling), then the smaller move.
    let mut ties: Vec<Match> = passed.into_iter().filter(|m| m.support >= best_support * 0.9).collect();
    ties.sort_by_key(|m| (prior.map(|p| (m.dy - p).abs()).unwrap_or(0), m.dy.abs()));
    ties.into_iter().next()
}

/// Fallback when the overlap holds no informative rows (blank space, uniform fills): accept an offset
/// only if every row of a large overlap (at least 40 % of the band) is pixel-identical, preferring
/// the largest overlap. Whitespace may come out a little shorter than on the page, but no content is
/// lost or duplicated, and a frame that skipped content entirely still fails.
fn blank_overlap(prev_hash: &[u64], cur_hash: &[u64], top: usize, bottom: usize) -> Option<Match> {
    let band = bottom - top;
    let min_overlap = (band * 2 / 5).max(48);
    let max_dy = (band - min_overlap) as i32;
    let mut best: Option<Match> = None;
    for dy in -max_dy..=max_dy {
        let (cy0, cy1) = if dy >= 0 { (top, bottom - dy as usize) } else { (top + (-dy) as usize, bottom) };
        let overlap = cy1 - cy0;
        if best.map(|b| overlap <= b.overlap).unwrap_or(false) { continue; }
        if (cy0..cy1).all(|y| cur_hash[y] == prev_hash[(y as i32 + dy) as usize]) { best = Some(Match { dy, overlap, support: 0.0 }); }
    }
    best
}

/// Rows of a frame pair (that scrolled) belonging to an element pinned to the viewport: a run of at
/// least 8 rows identical at the same screen position in both frames, containing real content
/// (at least two rows that are not flat). Isolated identical rows — blank lines that happen to
/// coincide — are not pinned, so they keep counting as real content.
fn pinned_rows(prev: &Gray, cur: &Gray, top: usize, bottom: usize) -> Vec<bool> {
    let h = cur.h;
    // A pinned element may not span the whole width (a code block narrower than the page): a row
    // counts as the same when nearly all of its columns agree.
    let same: Vec<bool> = (0..h).map(|y| y >= top && y < bottom && row_same_fraction(prev.row(y), cur.row(y)) >= 0.96).collect();
    let mut pinned = vec![false; h];
    let mut y = 0;
    while y < h {
        if !same[y] { y += 1; continue; }
        let start = y;
        while y < h && same[y] { y += 1; }
        let run = start..y;
        let textured = run.clone().filter(|&r| !row_is_flat(cur.row(r))).count();
        if run.len() >= 8 && textured >= 2 { for r in run { pinned[r] = true; } }
    }
    pinned
}

/// Fraction of columns (central band) whose values agree within 2 levels.
fn row_same_fraction(a: &[u8], b: &[u8]) -> f32 {
    let w = a.len();
    let x0 = w * 2 / 100;
    let x1 = (w * 94 / 100).max(x0 + 1);
    let same = (x0..x1).filter(|&x| (a[x] as i32 - b[x] as i32).abs() <= 2).count();
    same as f32 / (x1 - x0) as f32
}

/// `true` when a row has (almost) no contrast across the central band.
fn row_is_flat(row: &[u8]) -> bool {
    let w = row.len();
    let x0 = w * 2 / 100;
    let x1 = (w * 94 / 100).max(x0 + 1);
    let (mut lo, mut hi) = (255u8, 0u8);
    for &v in &row[x0..x1] { lo = lo.min(v); hi = hi.max(v); }
    hi - lo < 24
}

/// Counts leading/trailing rows that are (near) identical between the two frames.
fn static_bands(prev: &Gray, cur: &Gray) -> (usize, usize) {
    let h = prev.h.min(cur.h);
    let mut top = 0;
    while top < h && row_diff(prev.row(top), cur.row(top)) < 1.0 { top += 1; }
    let mut bottom = 0;
    while bottom + top < h && row_diff(prev.row(h - 1 - bottom), cur.row(h - 1 - bottom)) < 1.0 { bottom += 1; }
    (top, bottom)
}

pub struct Stitcher {
    w: u32,
    h: u32,
    axis: Option<Axis>,
    /// Accepted frames in order (RGBA), used for the final assembly.
    strips: Vec<Strip>,
    prev: Option<Gray>,
    prev_frame: Option<RgbaImage>,
    static_top: usize,
    static_bottom: usize,
    misses: u32,
    /// Offset measured for the previous accepted frame (prior for near-tie resolution).
    last_offset: Option<i32>,
    pub frames: u32,
    pub total_scroll: i64,
}

struct Strip {
    img: RgbaImage,
    /// Offset of this strip's origin relative to the first frame, along the scroll axis.
    pos: i64,
    /// Rows (vertical scrolling) that stayed identical at the same screen position although the
    /// content scrolled: elements pinned to the viewport (sticky code-block headers, floating
    /// toolbars). They are used only where no frame shows the real content.
    pinned: Vec<bool>,
}

impl Stitcher {
    pub fn new() -> Self {
        Self { w: 0, h: 0, axis: None, strips: vec![], prev: None, prev_frame: None, static_top: 0, static_bottom: 0, misses: 0, last_offset: None, frames: 0, total_scroll: 0 }
    }

    pub fn axis(&self) -> Option<Axis> { self.axis }
    pub fn accepted(&self) -> usize { self.strips.len() }

    /// Current stitched length along the scroll axis, in pixels.
    pub fn length(&self) -> u32 {
        if self.strips.is_empty() { return 0; }
        let (min, max) = self.extent();
        (max - min) as u32
    }

    fn extent(&self) -> (i64, i64) {
        let len = |s: &Strip| match self.axis { Some(Axis::Horizontal) => s.img.width() as i64, _ => s.img.height() as i64 };
        let min = self.strips.iter().map(|s| s.pos).min().unwrap_or(0);
        let max = self.strips.iter().map(|s| s.pos + len(s)).max().unwrap_or(0);
        (min, max)
    }

    pub fn push(&mut self, frame: RgbaImage) -> Push {
        self.frames += 1;
        if self.strips.is_empty() {
            self.w = frame.width();
            self.h = frame.height();
            self.prev = Some(Gray::from(&frame));
            self.strips.push(Strip { img: frame.clone(), pos: 0, pinned: Vec::new() });
            self.prev_frame = Some(frame);
            return Push::First;
        }
        if frame.width() != self.w || frame.height() != self.h { return Push::NoMatch; }
        let cur = Gray::from(&frame);
        let prev = self.prev.as_ref().unwrap();
        let (st, sb) = static_bands(prev, &cur);
        if st + sb >= self.h as usize - 8 { return Push::NoChange; }
        let h = self.h as usize;
        let axis = self.axis;
        let try_vertical = axis != Some(Axis::Horizontal);
        let try_horizontal = axis != Some(Axis::Vertical);
        let prior = self.last_offset;
        let mut result: Option<(Axis, i32)> = None;
        if try_vertical {
            // Match over everything except the sticky bands learned so far. Rows identical in this pair
            // alone may just be unchanged content (nothing scrolled), so they are only excluded as a
            // second attempt when the full band finds nothing.
            let learned = (self.static_top, h - self.static_bottom);
            let narrow = (self.static_top.max(st), h - self.static_bottom.max(sb));
            if std::env::var_os("LINKSHOT_STITCH_DEBUG").is_some() { eprintln!("frame {}: st={st} sb={sb} learned={learned:?} narrow={narrow:?} prior={prior:?}", self.frames); }
            let m = find_offset(prev, &cur, learned.0, learned.1, prior).or_else(|| if narrow != learned { find_offset(prev, &cur, narrow.0, narrow.1, prior).filter(|m| m.dy != 0) } else { None });
            if let Some(m) = m { result = Some((Axis::Vertical, m.dy)); }
        }
        if result.is_none() && try_horizontal {
            let pt = prev.transposed();
            let ct = cur.transposed();
            if let Some(m) = find_offset(&pt, &ct, 0, pt.h, prior) { result = Some((Axis::Horizontal, m.dy)); }
        }
        // Sticky header/footer: rows that stay identical although the content scrolled. Learned from
        // scrolled frames only; keep the smallest seen so coincidences get corrected later.
        if matches!(result, Some((Axis::Vertical, dy)) if dy != 0) {
            if self.strips.len() == 1 && self.static_top == 0 && self.static_bottom == 0 { self.static_top = st; self.static_bottom = sb; }
            else { self.static_top = self.static_top.min(st); self.static_bottom = self.static_bottom.min(sb); }
        }
        match result {
            Some((_, 0)) => {
                // Same scroll position, some pixels changed (hover, caret, lazy-loaded content):
                // keep the newest pixels, nothing moved.
                self.prev = Some(cur);
                if let Some(s) = self.strips.last_mut() { s.img = frame.clone(); }
                self.prev_frame = Some(frame);
                self.misses = 0;
                Push::NoChange
            }
            Some((ax, off)) => {
                self.axis.get_or_insert(ax);
                if self.axis != Some(ax) { self.misses += 1; return Push::NoMatch; }
                self.misses = 0;
                self.last_offset = Some(off);
                let last_pos = self.strips.last().map(|s| s.pos).unwrap_or(0);
                let pos = last_pos + off as i64;
                // Rows identical at the same screen position in both frames although the content
                // moved are pinned elements; flag them in both frames.
                let pinned: Vec<bool> = if ax == Axis::Vertical && off.abs() >= 3 {
                    pinned_rows(prev, &cur, self.static_top, self.h as usize - self.static_bottom)
                } else { Vec::new() };
                if let Some(last) = self.strips.last_mut() {
                    if last.pinned.len() != pinned.len() { last.pinned = pinned.clone(); }
                    else { for (a, b) in last.pinned.iter_mut().zip(&pinned) { *a |= *b; } }
                }
                let added = self.append(frame.clone(), pos, ax, pinned);
                self.total_scroll += off as i64;
                self.prev = Some(cur);
                self.prev_frame = Some(frame);
                Push::Scrolled { axis: ax, offset: off, added }
            }
            None => {
                // No overlap with the last accepted frame (scrolled too far, or the content jumped).
                // The frame is dropped; the user can scroll back a little to resynchronise.
                self.misses += 1;
                Push::NoMatch
            }
        }
    }

    /// Consecutive frames that could not be matched (resets on the next match).
    pub fn misses(&self) -> u32 { self.misses }
    /// Learned sticky header/footer heights in pixels.
    pub fn static_bands(&self) -> (usize, usize) { (self.static_top, self.static_bottom) }
    /// The last frame that was accepted (what the next frame has to overlap).
    pub fn last_frame(&self) -> Option<&RgbaImage> { self.prev_frame.as_ref() }

    /// Records a frame at `pos`; returns how many new pixels it contributes along the axis.
    fn append(&mut self, frame: RgbaImage, pos: i64, axis: Axis, pinned: Vec<bool>) -> u32 {
        let (min, max) = self.extent();
        let len = match axis { Axis::Vertical => self.h as i64, Axis::Horizontal => self.w as i64 };
        let new_max = (pos + len).max(max);
        let new_min = pos.min(min);
        let added = ((new_max - max) + (min - new_min)) as u32;
        self.strips.push(Strip { img: frame, pos, pinned });
        added
    }

    /// Assembles the final image. Static top/bottom bands (vertical) come from the first frame
    /// (header) and the last frame (footer); everything else is placed by its measured offset, later
    /// frames overwriting earlier ones where they overlap.
    pub fn finish(&self) -> Option<RgbaImage> {
        if self.strips.is_empty() { return None; }
        let axis = self.axis.unwrap_or(Axis::Vertical);
        let (min, max) = self.extent();
        match axis {
            Axis::Vertical => {
                let st = self.static_top as i64;
                let sb = self.static_bottom as i64;
                let body_len = (max - min) - st - sb;
                let total_h = (st + body_len.max(0) + sb) as u32;
                let mut out = RgbaImage::new(self.w, total_h.max(1));
                // Body: strips contribute their middle band, mapped to canvas row (pos - min). Later
                // frames overwrite earlier ones, except that a pinned row never overwrites real content
                // and real content always replaces a pinned row.
                let mut real = vec![false; total_h as usize];
                if std::env::var_os("LINKSHOT_STITCH_DEBUG").is_some() { for (i, s) in self.strips.iter().enumerate() { let p: Vec<usize> = s.pinned.iter().enumerate().filter(|(_, b)| **b).map(|(y, _)| y).collect(); eprintln!("strip {i} pos {} pinned rows {:?}..{:?} ({})", s.pos, p.first(), p.last(), p.len()); } }
                for s in &self.strips {
                    for y in self.static_top..(self.h as usize - self.static_bottom) {
                        let cy = s.pos - min + y as i64;
                        if cy < 0 || cy >= total_h as i64 { continue; }
                        let is_pinned = s.pinned.get(y).copied().unwrap_or(false);
                        if is_pinned && real[cy as usize] { continue; }
                        let src = &s.img.as_raw()[y * self.w as usize * 4..(y + 1) * self.w as usize * 4];
                        let dst = &mut out.as_mut()[cy as usize * self.w as usize * 4..(cy as usize + 1) * self.w as usize * 4];
                        dst.copy_from_slice(src);
                        if !is_pinned { real[cy as usize] = true; }
                    }
                }
                // Header from first frame, footer from last frame.
                let first = &self.strips[0].img;
                let last = &self.strips[self.strips.len() - 1].img;
                let stride = self.w as usize * 4;
                for y in 0..self.static_top { out.as_mut()[y * stride..(y + 1) * stride].copy_from_slice(&first.as_raw()[y * stride..(y + 1) * stride]); }
                for i in 0..self.static_bottom {
                    let sy = self.h as usize - self.static_bottom + i;
                    let dy = total_h as usize - self.static_bottom + i;
                    out.as_mut()[dy * stride..(dy + 1) * stride].copy_from_slice(&last.as_raw()[sy * stride..(sy + 1) * stride]);
                }
                Some(out)
            }
            Axis::Horizontal => {
                let total_w = (max - min) as u32;
                let mut out = RgbaImage::new(total_w.max(1), self.h);
                for s in &self.strips {
                    let x0 = s.pos - min;
                    image::imageops::replace(&mut out, &s.img, x0, 0);
                }
                Some(out)
            }
        }
    }
}

impl Default for Stitcher { fn default() -> Self { Self::new() } }

#[cfg(test)]
mod tests_helpers {
    use image::{Rgba, RgbaImage};

    /// A tall "web page": random blocks of colour with text-like texture, plus a sticky header/footer.
    pub fn make_page(w: u32, h: u32, seed: u64) -> RgbaImage {
        let mut img = RgbaImage::from_pixel(w, h, Rgba([250, 250, 252, 255]));
        let mut s = seed;
        let mut rnd = || { s ^= s << 13; s ^= s >> 7; s ^= s << 17; s };
        for _ in 0..400 {
            let x = (rnd() % w as u64) as u32; let y = (rnd() % h as u64) as u32;
            let bw = (rnd() % 220 + 10) as u32; let bh = (rnd() % 24 + 4) as u32;
            let c = Rgba([(rnd() % 256) as u8, (rnd() % 256) as u8, (rnd() % 256) as u8, 255]);
            for yy in y..(y + bh).min(h) { for xx in x..(x + bw).min(w) { if (xx + yy) % 3 != 0 { img.put_pixel(xx, yy, c); } } }
        }
        img
    }

    pub fn frame(page: &RgbaImage, header: &RgbaImage, footer: &RgbaImage, scroll: u32, vh: u32, thumb: u32) -> RgbaImage {
        let w = page.width();
        let hh = header.height();
        let fh = footer.height();
        let mut f = RgbaImage::new(w, vh);
        image::imageops::replace(&mut f, header, 0, 0);
        let body = image::imageops::crop_imm(page, 0, scroll, w, vh - hh - fh).to_image();
        image::imageops::replace(&mut f, &body, 0, hh as i64);
        image::imageops::replace(&mut f, footer, 0, (vh - fh) as i64);
        // scrollbar on the right whose thumb moves
        for y in 0..vh { for x in w - 12..w { f.put_pixel(x, y, Rgba([230, 230, 230, 255])); } }
        for y in thumb..(thumb + 60).min(vh) { for x in w - 10..w - 2 { f.put_pixel(x, y, Rgba([120, 120, 120, 255])); } }
        f
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use super::tests_helpers::*;
    use image::RgbaImage;

    #[test]
    fn stitches_vertical_scroll_with_sticky_bands() {
        let (w, ph, vh) = (800, 4000, 700);
        let page = make_page(w, ph, 42);
        let header = image::imageops::crop_imm(&make_page(w, 60, 7), 0, 0, w, 60).to_image();
        let footer = image::imageops::crop_imm(&make_page(w, 40, 9), 0, 0, w, 40).to_image();
        let body_vh = vh - 100;
        let mut st = Stitcher::new();
        let mut scroll = 0u32;
        let steps = [0u32, 120, 300, 55, 250, 180, 0, 400, 90, 333, 150, 200, 250, 300, 100, 95, 200];
        let mut expected_last = 0;
        for (i, step) in steps.iter().enumerate() {
            scroll = (scroll + step).min(ph - body_vh);
            expected_last = scroll;
            let thumb = scroll * (vh - 60) / ph;
            let r = st.push(frame(&page, &header, &footer, scroll, vh, thumb));
            if i == 0 { assert_eq!(r, Push::First); }
            else if *step == 0 || scroll == expected_last && step == &0 { assert!(matches!(r, Push::NoChange), "{r:?}"); }
            else { assert!(matches!(r, Push::Scrolled { axis: Axis::Vertical, .. }), "frame {i}: {r:?}"); }
        }
        let out = st.finish().unwrap();
        let expected_h = 60 + (expected_last + body_vh) + 40;
        assert_eq!(out.height(), expected_h, "stitched height");
        // Body must equal the page pixels (except the scrollbar columns).
        let mut bad = 0;
        for y in 0..(expected_last + body_vh) { for x in 0..w - 12 { if out.get_pixel(x, y + 60) != page.get_pixel(x, y) { bad += 1; } } }
        assert_eq!(bad, 0, "mismatching body pixels");
        for x in 0..w - 12 { assert_eq!(out.get_pixel(x, 5), header.get_pixel(x, 5)); assert_eq!(out.get_pixel(x, expected_h - 10), footer.get_pixel(x, 30)); }
    }

    /// A dark chat page like the one in the user's report: identical code-block headers every few
    /// hundred pixels, long blank gaps, text lines; scrolled with a mix of tiny and very large steps,
    /// with a tooltip flickering in some frames and a caret blinking in others.
    #[test]
    fn stitches_repetitive_dark_page_with_big_steps() {
        use image::Rgba;
        let (w, ph, vh) = (1400u32, 9000u32, 1000u32);
        let mut page = RgbaImage::from_pixel(w, ph, Rgba([33, 33, 35, 255]));
        let mut seed = 7u64;
        let mut rnd = || { seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17; seed };
        // text lines: short bars of random length, 18 px apart, with blank paragraphs and gaps
        let mut y = 20u32;
        while y + 20 < ph {
            let r = rnd() % 10;
            if r == 0 { y += 220; continue; } // big blank gap
            let len = 300 + (rnd() % 900) as u32;
            let shade = 150 + (rnd() % 100) as u8;
            for yy in y..y + 9 { for x in 40..(40 + len).min(w - 60) { if (x / 7 + yy) % 3 != 0 { page.put_pixel(x, yy, Rgba([shade, shade, shade, 255])); } } }
            y += 18;
        }
        // identical "JavaScript" code-block headers (same pixels each time) every ~330 px
        let mut hy = 140u32;
        while hy + 34 < ph {
            for yy in hy..hy + 30 { for x in 60..w - 80 { let v = if (x / 9 + (yy - hy) / 5) % 4 == 0 { 90 } else { 48 }; page.put_pixel(x, yy, Rgba([v, v, v + 6, 255])); } }
            hy += 330;
        }
        let frame_at = |scroll: u32, tooltip: bool, caret: bool| -> RgbaImage {
            let mut f = image::imageops::crop_imm(&page, 0, scroll, w, vh).to_image();
            if tooltip { for yy in 400..470 { for x in 500..900 { f.put_pixel(x, yy, Rgba([240, 240, 200, 255])); } } }
            if caret { for yy in 100..118 { f.put_pixel(300, yy, Rgba([255, 255, 255, 255])); } }
            // scrollbar
            let thumb = scroll * (vh - 80) / ph;
            for yy in 0..vh { for x in w - 14..w { f.put_pixel(x, yy, Rgba([50, 50, 50, 255])); } }
            for yy in thumb..(thumb + 80).min(vh) { for x in w - 12..w - 2 { f.put_pixel(x, yy, Rgba([140, 140, 140, 255])); } }
            f
        };
        let mut st = Stitcher::new();
        let mut scroll = 0u32;
        let steps: [(i32, bool, bool); 22] = [(0, false, false), (40, false, true), (330, false, false), (660, true, false), (15, false, false), (700, false, false), (0, true, false), (0, false, true), (690, false, false), (330, false, false), (120, true, true), (-200, false, false), (650, false, false), (700, false, false), (330, false, false), (5, false, false), (700, false, false), (330, false, true), (660, false, false), (400, false, false), (680, false, false), (600, false, false)];
        for (i, (step, tooltip, caret)) in steps.iter().enumerate() {
            scroll = (scroll as i32 + step).clamp(0, (ph - vh) as i32) as u32;
            let r = st.push(frame_at(scroll, *tooltip, *caret));
            match r {
                Push::First => assert_eq!(i, 0),
                Push::NoChange => assert_eq!(*step, 0, "frame {i}: unexpected NoChange"),
                Push::Scrolled { axis: Axis::Vertical, offset, .. } => assert_eq!(offset, *step, "frame {i}: wrong offset"),
                other => panic!("frame {i}: {other:?}"),
            }
        }
        let out = st.finish().unwrap();
        assert_eq!(out.height(), scroll + vh, "stitched height");
        let mut bad = 0;
        for y in 0..out.height() { for x in 0..w - 14 { if out.get_pixel(x, y) != page.get_pixel(x, y) { bad += 1; } } }
        // Only the tooltip/caret pixels of the last frames that covered them may differ.
        assert!(bad < 70 * 400 + 20, "mismatching pixels: {bad}");
    }

    /// Chat UIs pin a code block's header to the top of the viewport while the block scrolls by.
    /// The header must appear once, at its natural position, and the code it covered must be
    /// recovered from the other frames.
    #[test]
    fn pinned_code_block_headers_appear_once() {
        use image::Rgba;
        let (w, ph, vh) = (900u32, 5000u32, 800u32);
        let mut page = RgbaImage::from_pixel(w, ph, Rgba([30, 30, 32, 255]));
        let mut seed = 3u64;
        let mut rnd = || { seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17; seed };
        for y in (10..ph - 20).step_by(16) { let len = 200 + (rnd() % 600) as u32; let v = 120 + (rnd() % 120) as u8; for yy in y..y + 8 { for x in 30..30 + len { if (x / 5 + yy) % 3 != 0 { page.put_pixel(x, yy, Rgba([v, v, v, 255])); } } } }
        // Two code blocks with a 28 px header each; the header is pinned to the top of the content
        // area (below a 40 px page toolbar) while the block top is scrolled out of view.
        let blocks = [(1200u32, 1600u32), (3300u32, 1100u32)];
        let header = |img: &mut RgbaImage, y0: u32| { for yy in y0..y0 + 28 { for x in 40..w - 40 { let v = if (x / 11 + (yy - y0) / 4) % 5 == 0 { 110 } else { 55 }; img.put_pixel(x, yy, Rgba([v, v, v + 8, 255])); } } };
        for (top, _) in blocks { header(&mut page, top); }
        let toolbar = 40u32;
        let frame_at = |scroll: u32| -> RgbaImage {
            let mut f = image::imageops::crop_imm(&page, 0, scroll, w, vh).to_image();
            for (top, len) in blocks { if scroll > top && scroll + toolbar < top + len { header(&mut f, toolbar); } }
            for yy in 0..toolbar { for x in 0..w { f.put_pixel(x, yy, Rgba([60, 60, 70, 255])); } }
            f
        };
        let mut st = Stitcher::new();
        let mut scroll = 0u32;
        for step in [0u32, 300, 450, 300, 250, 400, 350, 300, 450, 400, 300, 350, 250] {
            scroll = (scroll + step).min(ph - vh);
            let r = st.push(frame_at(scroll));
            assert!(matches!(r, Push::First | Push::Scrolled { .. } | Push::NoChange), "{r:?}");
        }
        let out = st.finish().unwrap();
        if std::env::var_os("LINKSHOT_STITCH_DEBUG").is_some() { let _ = out.save("/tmp/linkshot-pinned-test.png"); let _ = page.save("/tmp/linkshot-pinned-page.png"); }
        assert_eq!(out.height(), scroll + vh);
        // Frame row r is page row scroll + r, so output row y is page row y; the page rows under the
        // toolbar in the first frame are never visible, so compare below it only.
        let mut bad = 0;
        let mut first_bad = None;
        let mut bad_rows = vec![];
        for y in toolbar..out.height() { let mut rb = 0; for x in 0..w { if out.get_pixel(x, y) != page.get_pixel(x, y) { bad += 1; rb += 1; first_bad.get_or_insert(y); } } if rb > 0 { bad_rows.push(y); } }
        if std::env::var_os("LINKSHOT_STITCH_DEBUG").is_some() { let mut ranges = vec![]; for y in bad_rows { match ranges.last_mut() { Some((_, e)) if *e + 1 == y => *e = y, _ => ranges.push((y, y)) } } eprintln!("bad row ranges: {ranges:?}"); }
        assert_eq!(bad, 0, "pixels differing from the page: {bad} (first bad row {first_bad:?}, accepted {})", st.accepted());
    }

    #[test]
    fn drops_frames_without_overlap_instead_of_guessing() {
        let (w, ph, vh) = (700u32, 4000u32, 500u32);
        let page = make_page(w, ph, 11);
        let mut st = Stitcher::new();
        let f = |s: u32| image::imageops::crop_imm(&page, 0, s, w, vh).to_image();
        assert_eq!(st.push(f(0)), Push::First);
        assert!(matches!(st.push(f(300)), Push::Scrolled { offset: 300, .. }));
        // Jump far beyond the viewport: no overlap → dropped, not seamed.
        for _ in 0..12 { assert_eq!(st.push(f(2000)), Push::NoMatch); }
        assert_eq!(st.accepted(), 2);
        // Scrolling back into overlap resynchronises.
        assert!(matches!(st.push(f(600)), Push::Scrolled { offset: 300, .. }));
        let out = st.finish().unwrap();
        assert_eq!(out.height(), 600 + vh);
        assert!(out.pixels().zip(image::imageops::crop_imm(&page, 0, 0, w, 600 + vh).to_image().pixels()).all(|(a, b)| a == b));
    }

    #[test]
    fn stitches_upward_and_horizontal() {
        let (w, ph, vh) = (600, 2000, 500);
        let page = make_page(w, ph, 99);
        let empty = RgbaImage::new(w, 0);
        let mut st = Stitcher::new();
        let mut scroll = 1500u32;
        for step in [0i32, -200, -300, -150, -400, -450] {
            scroll = (scroll as i32 + step).max(0) as u32;
            let f = image::imageops::crop_imm(&page, 0, scroll, w, vh).to_image();
            let _ = (&empty, st.push(f));
        }
        let out = st.finish().unwrap();
        assert_eq!(out.height(), 1500 + vh);
        assert!(out.pixels().zip(image::imageops::crop_imm(&page, 0, 0, w, 1500 + vh).to_image().pixels()).all(|(a, b)| a == b));
        // horizontal
        let wide = make_page(3000, 400, 5);
        let mut st = Stitcher::new();
        let mut x = 0u32;
        for step in [0u32, 200, 350, 100, 400] {
            x += step;
            let f = image::imageops::crop_imm(&wide, x, 0, 700, 400).to_image();
            let r = st.push(f);
            if step > 0 { assert!(matches!(r, Push::Scrolled { axis: Axis::Horizontal, .. }), "{r:?}"); }
        }
        let out = st.finish().unwrap();
        assert_eq!(out.width(), x + 700);
        assert!(out.pixels().zip(image::imageops::crop_imm(&wide, 0, 0, x + 700, 400).to_image().pixels()).all(|(a, b)| a == b));
    }
}

