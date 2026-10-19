//! Colour quantisation for animated GIFs: one global 255-colour palette (index 255 is reserved for
//! "unchanged pixel" transparency) built from sample frames, with exact colours when the recording
//! uses at most 255 of them, otherwise NeuQuant. Pixel mapping is cached per exact colour (screen
//! recordings reuse a few thousand colours), and dithering uses a coarse lookup table.

use std::collections::{HashMap, HashSet};

pub const TRANSPARENT: u8 = 255;
const LUT_BITS: u32 = 6;
const LUT_UNSET: u16 = u16::MAX;

pub struct Palette {
    /// 255 entries at most.
    pub rgb: Vec<[u8; 3]>,
    pub exact: bool,
    nq: Option<color_quant::NeuQuant>,
    cache: HashMap<u32, u8>,
    lut: Vec<u16>,
}

fn key(r: u8, g: u8, b: u8) -> u32 { (r as u32) << 16 | (g as u32) << 8 | b as u32 }

impl Palette {
    /// Builds the palette from RGBA sample frames. Pixels are subsampled so building stays fast
    /// regardless of frame size.
    pub fn build(samples: &[&[u8]]) -> Palette {
        let total: usize = samples.iter().map(|s| s.len() / 4).sum();
        let stride = (total / 300_000).max(1);
        let mut colours: HashSet<u32> = HashSet::new();
        let mut sub: Vec<u8> = Vec::with_capacity((total / stride + 1) * 4);
        let mut k = 0usize;
        for s in samples {
            for px in s.chunks_exact(4) {
                if k % stride == 0 {
                    sub.extend_from_slice(&[px[0], px[1], px[2], 255]);
                    if colours.len() <= 255 { colours.insert(key(px[0], px[1], px[2])); }
                }
                k += 1;
            }
        }
        if colours.len() <= 255 && !colours.is_empty() {
            // Exact palette: check all pixels (not just the subsample) still fit.
            let mut all = colours.clone();
            'outer: for s in samples {
                for px in s.chunks_exact(4) {
                    all.insert(key(px[0], px[1], px[2]));
                    if all.len() > 255 { break 'outer; }
                }
            }
            if all.len() <= 255 {
                let mut rgb: Vec<[u8; 3]> = all.into_iter().map(|c| [(c >> 16) as u8, (c >> 8) as u8, c as u8]).collect();
                rgb.sort();
                let cache = rgb.iter().enumerate().map(|(i, c)| (key(c[0], c[1], c[2]), i as u8)).collect();
                return Palette { rgb, exact: true, nq: None, cache, lut: vec![] };
            }
        }
        if sub.len() < 4 * 16 { sub.resize(4 * 16, 0); }
        let nq = color_quant::NeuQuant::new(10, 255, &sub);
        let map = nq.color_map_rgb();
        let rgb: Vec<[u8; 3]> = map.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
        Palette { rgb, exact: false, nq: Some(nq), cache: HashMap::new(), lut: vec![] }
    }

    /// 256 × RGB bytes for the GIF global colour table (index 255 = transparency placeholder).
    pub fn gif_colour_table(&self) -> Vec<u8> {
        let mut t = Vec::with_capacity(768);
        for c in &self.rgb { t.extend_from_slice(c); }
        while t.len() < 768 { t.extend_from_slice(&[0, 0, 0]); }
        t
    }

    fn nearest(&self, r: u8, g: u8, b: u8) -> u8 {
        if let Some(nq) = &self.nq {
            return (nq.index_of(&[r, g, b, 255]) as u8).min(254);
        }
        let mut best = (u32::MAX, 0u8);
        for (i, c) in self.rgb.iter().enumerate() {
            let d = (c[0] as i32 - r as i32).pow(2) as u32 + (c[1] as i32 - g as i32).pow(2) as u32 + (c[2] as i32 - b as i32).pow(2) as u32;
            if d < best.0 { best = (d, i as u8); if d == 0 { break; } }
        }
        best.1
    }

    /// Palette index for an exact colour (cached).
    pub fn index(&mut self, r: u8, g: u8, b: u8) -> u8 {
        let k = key(r, g, b);
        if let Some(i) = self.cache.get(&k) { return *i; }
        let i = self.nearest(r, g, b);
        if self.cache.len() < 2_000_000 { self.cache.insert(k, i); }
        i
    }

    /// Palette index through a 6-bit-per-channel table (used while dithering, where every pixel
    /// carries a slightly different error-adjusted colour).
    pub fn lut_index(&mut self, r: u8, g: u8, b: u8) -> u8 {
        if self.lut.is_empty() { self.lut = vec![LUT_UNSET; 1 << (3 * LUT_BITS)]; }
        let sh = 8 - LUT_BITS;
        let k = ((r as usize >> sh) << (2 * LUT_BITS)) | ((g as usize >> sh) << LUT_BITS) | (b as usize >> sh);
        let v = self.lut[k];
        if v != LUT_UNSET { return v as u8; }
        // Cell centre.
        let half = 1u8 << (sh - 1);
        let i = self.nearest((r >> sh << sh) | half, (g >> sh << sh) | half, (b >> sh << sh) | half);
        self.lut[k] = i as u16;
        i
    }
}

/// Maps an RGBA frame to palette indices, optionally with Floyd–Steinberg dithering.
pub fn quantize(pal: &mut Palette, rgba: &[u8], width: u32, height: u32, dither: bool, out: &mut Vec<u8>) {
    let (w, h) = (width as usize, height as usize);
    out.clear();
    out.resize(w * h, 0);
    if !dither || pal.exact {
        for (px, o) in rgba.chunks_exact(4).zip(out.iter_mut()) { *o = pal.index(px[0], px[1], px[2]); }
        return;
    }
    let mut err_cur = vec![[0i32; 3]; w + 2];
    let mut err_next = vec![[0i32; 3]; w + 2];
    for y in 0..h {
        for e in err_next.iter_mut() { *e = [0; 3]; }
        let serpentine = y % 2 == 1;
        for step in 0..w {
            let x = if serpentine { w - 1 - step } else { step };
            let p = &rgba[(y * w + x) * 4..][..3];
            let e = err_cur[x + 1];
            let r = (p[0] as i32 + e[0] / 16).clamp(0, 255) as u8;
            let g = (p[1] as i32 + e[1] / 16).clamp(0, 255) as u8;
            let b = (p[2] as i32 + e[2] / 16).clamp(0, 255) as u8;
            let idx = pal.lut_index(r, g, b);
            out[y * w + x] = idx;
            let c = pal.rgb[idx as usize];
            let d = [r as i32 - c[0] as i32, g as i32 - c[1] as i32, b as i32 - c[2] as i32];
            let (right, left) = if serpentine { (x, x + 2) } else { (x + 2, x) };
            for ch in 0..3 {
                err_cur[right][ch] += d[ch] * 7;
                err_next[left][ch] += d[ch] * 3;
                err_next[x + 1][ch] += d[ch] * 5;
                err_next[right][ch] += d[ch];
            }
        }
        std::mem::swap(&mut err_cur, &mut err_next);
    }
}
