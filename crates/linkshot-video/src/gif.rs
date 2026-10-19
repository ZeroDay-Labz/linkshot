//! Animated GIF export ("Create Animated GIF"): resamples the source to a fixed frame rate,
//! holds frames that did not change, and writes only the changed rectangle of each frame with
//! unchanged pixels transparent, which keeps screen recordings small.

use crate::quant::{quantize, Palette, TRANSPARENT};
use crate::source::{output_size, resize_rgba, FrameSource};
use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::io::{BufWriter, Write};
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GifOptions {
    /// Output frame rate (1–50).
    pub fps: f32,
    /// Output size; 0 keeps the source size / aspect ratio.
    pub width: u32,
    pub height: u32,
    pub dithering: bool,
    /// 0 = loop forever.
    pub loops: u16,
    /// Hold identical frames instead of repeating them.
    pub skip_similar: bool,
    /// Export range in ms (None = whole video).
    pub start_ms: Option<u32>,
    pub end_ms: Option<u32>,
}

impl Default for GifOptions {
    fn default() -> Self { Self { fps: 10.0, width: 0, height: 0, dithering: false, loops: 0, skip_similar: true, start_ms: None, end_ms: None } }
}

impl GifOptions {
    /// The "Optimize for" presets.
    pub fn screen_video() -> Self { Self::default() }
    pub fn high_motion() -> Self { Self { fps: 15.0, dithering: true, ..Self::default() } }
    pub fn reduced_file_size() -> Self { Self { fps: 5.0, width: 640, height: 0, ..Self::default() } }
}

#[derive(Clone, Debug, Default)]
pub struct GifReport {
    pub frames_written: usize,
    pub frames_sampled: usize,
    pub bytes: u64,
    pub duration_ms: u32,
    pub width: u32,
    pub height: u32,
    pub exact_palette: bool,
}

struct Pending {
    left: u16,
    top: u16,
    width: u16,
    height: u16,
    buffer: Vec<u8>,
    transparent: bool,
    hold_ms: f32,
}

fn write_pending<W: Write>(enc: &mut gif::Encoder<W>, p: &Pending, carry: &mut f32) -> Result<()> {
    let want = p.hold_ms + *carry;
    let mut cs = (want / 10.0).round().max(2.0);
    *carry = want - cs * 10.0;
    if cs > u16::MAX as f32 { cs = u16::MAX as f32; }
    let frame = gif::Frame {
        delay: cs as u16,
        dispose: gif::DisposalMethod::Keep,
        transparent: if p.transparent { Some(TRANSPARENT) } else { None },
        needs_user_input: false,
        top: p.top,
        left: p.left,
        width: p.width,
        height: p.height,
        interlaced: false,
        palette: None,
        buffer: Cow::Borrowed(&p.buffer),
    };
    enc.write_frame(&frame).context("write GIF frame")?;
    Ok(())
}

/// Exports `src` to an animated GIF. `progress(0..=1)` returns false to cancel.
pub fn export_gif(src: &mut dyn FrameSource, opts: &GifOptions, out: &Path, progress: &mut dyn FnMut(f32) -> bool) -> Result<GifReport> {
    ensure!(!src.is_empty(), "the video has no frames");
    let (sw, sh) = (src.width(), src.height());
    let (ow, oh) = output_size(sw, sh, opts.width, opts.height);
    ensure!(ow <= 65535 && oh <= 65535, "GIF frames are limited to 65535 px");
    let fps = opts.fps.clamp(1.0, 50.0);
    let tick = 1000.0 / fps;
    let start = opts.start_ms.unwrap_or(0).min(src.duration_ms());
    let end = opts.end_ms.unwrap_or(src.duration_ms()).clamp(start + 1, src.duration_ms().max(start + 1));
    let ticks = (((end - start) as f32) / tick).ceil().max(1.0) as usize;

    // Sample up to 12 frames spread over the range for the palette.
    let n_samples = ticks.min(12).max(1);
    let mut samples: Vec<Vec<u8>> = Vec::with_capacity(n_samples);
    for k in 0..n_samples {
        let t = start + ((k as f32 + 0.5) / n_samples as f32 * (end - start) as f32) as u32;
        let i = src.index_at(t);
        let f = src.frame(i)?;
        samples.push(resize_rgba(f, sw, sh, ow, oh));
    }
    let refs: Vec<&[u8]> = samples.iter().map(|s| s.as_slice()).collect();
    let mut pal = Palette::build(&refs);
    drop(samples);
    if !progress(0.02) { bail!("cancelled"); }

    if let Some(p) = out.parent() { std::fs::create_dir_all(p).ok(); }
    let file = std::fs::File::create(out).with_context(|| format!("create {}", out.display()))?;
    let mut enc = gif::Encoder::new(BufWriter::with_capacity(1 << 20, file), ow as u16, oh as u16, &pal.gif_colour_table()).context("GIF header")?;
    enc.set_repeat(if opts.loops == 0 { gif::Repeat::Infinite } else { gif::Repeat::Finite(opts.loops) }).context("GIF loop")?;

    let mut report = GifReport { width: ow, height: oh, exact_palette: pal.exact, ..Default::default() };
    let mut prev_src: Option<usize> = None;
    let mut prev_idx: Vec<u8> = vec![];
    let mut cur_idx: Vec<u8> = vec![];
    let mut pending: Option<Pending> = None;
    let mut carry = 0.0f32;
    let (w, h) = (ow as usize, oh as usize);
    for k in 0..ticks {
        let t = start + (k as f32 * tick) as u32;
        let i = src.index_at(t);
        let hold = if k + 1 == ticks { (end as f32 - (start as f32 + k as f32 * tick)).max(tick * 0.5) } else { tick };
        if prev_src == Some(i) && opts.skip_similar {
            if let Some(p) = pending.as_mut() { p.hold_ms += hold; }
            continue;
        }
        prev_src = Some(i);
        report.frames_sampled += 1;
        let scaled;
        let frame: &[u8] = {
            let f = src.frame(i)?;
            if sw == ow && sh == oh { f } else { scaled = resize_rgba(f, sw, sh, ow, oh); &scaled }
        };
        quantize(&mut pal, frame, ow, oh, opts.dithering, &mut cur_idx);
        if prev_idx.is_empty() {
            pending = Some(Pending { left: 0, top: 0, width: ow as u16, height: oh as u16, buffer: cur_idx.clone(), transparent: false, hold_ms: hold });
            prev_idx = cur_idx.clone();
        } else {
            // Bounding box of changed pixels.
            let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0usize, 0usize);
            for y in 0..h {
                let row = &cur_idx[y * w..(y + 1) * w];
                let prow = &prev_idx[y * w..(y + 1) * w];
                if row == prow { continue; }
                let first = row.iter().zip(prow).position(|(a, b)| a != b).unwrap();
                let last = w - 1 - row.iter().zip(prow).rev().position(|(a, b)| a != b).unwrap();
                x0 = x0.min(first);
                x1 = x1.max(last);
                y0 = y0.min(y);
                y1 = y1.max(y);
            }
            if x1 < x0 {
                // identical after quantisation: hold the previous frame longer
                if opts.skip_similar {
                    if let Some(p) = pending.as_mut() { p.hold_ms += hold; }
                    continue;
                }
                (x0, y0, x1, y1) = (0, 0, 0, 0);
            }
            if let Some(p) = pending.take() { write_pending(&mut enc, &p, &mut carry)?; report.frames_written += 1; }
            let (bw, bh) = (x1 - x0 + 1, y1 - y0 + 1);
            let mut buf = vec![TRANSPARENT; bw * bh];
            for y in 0..bh {
                let row = &cur_idx[(y0 + y) * w + x0..][..bw];
                let prow = &prev_idx[(y0 + y) * w + x0..][..bw];
                for x in 0..bw { if row[x] != prow[x] { buf[y * bw + x] = row[x]; } }
            }
            pending = Some(Pending { left: x0 as u16, top: y0 as u16, width: bw as u16, height: bh as u16, buffer: buf, transparent: true, hold_ms: hold });
            std::mem::swap(&mut prev_idx, &mut cur_idx);
        }
        if k % 4 == 0 && !progress(0.02 + 0.97 * (k as f32 / ticks as f32)) {
            drop(enc);
            let _ = std::fs::remove_file(out);
            bail!("cancelled");
        }
    }
    if let Some(p) = pending.take() { write_pending(&mut enc, &p, &mut carry)?; report.frames_written += 1; }
    let mut w = enc.into_inner().context("finish GIF")?;
    w.flush()?;
    drop(w);
    report.bytes = std::fs::metadata(out).map(|m| m.len()).unwrap_or(0);
    report.duration_ms = end - start;
    progress(1.0);
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::MemorySource;
    use image::AnimationDecoder;

    fn solid(w: u32, h: u32, rgb: [u8; 3]) -> Vec<u8> {
        let mut v = Vec::with_capacity((w * h * 4) as usize);
        for _ in 0..w * h { v.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]); }
        v
    }

    #[test]
    fn exact_palette_roundtrip_with_held_frames() {
        let (w, h) = (40u32, 30u32);
        let mut src = MemorySource::new(w, h);
        // frame 0 red 0-300 ms, frame 1 identical to frame 0 (held), frame 2 blue with a green box.
        src.push(solid(w, h, [200, 30, 30]), 300);
        src.push(solid(w, h, [200, 30, 30]), 300);
        let mut f2 = solid(w, h, [30, 30, 200]);
        for y in 5..15 { for x in 10..20 { let i = ((y * w + x) * 4) as usize; f2[i..i + 3].copy_from_slice(&[20, 220, 20]); } }
        src.push(f2.clone(), 400);
        let dir = std::env::temp_dir().join(format!("linkshot-gif-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("a.gif");
        let rep = export_gif(&mut src, &GifOptions { fps: 10.0, ..Default::default() }, &out, &mut |_| true).unwrap();
        assert!(rep.exact_palette);
        assert_eq!(rep.frames_written, 2, "{rep:?}");
        let dec = image::codecs::gif::GifDecoder::new(std::io::BufReader::new(std::fs::File::open(&out).unwrap())).unwrap();
        let frames = dec.into_frames().collect_frames().unwrap();
        assert_eq!(frames.len(), 2);
        let (n, d) = frames[0].delay().numer_denom_ms();
        assert_eq!(n / d, 600, "first frame held for both red source frames");
        let last = frames[1].buffer();
        assert_eq!(last.get_pixel(15, 10).0, [20, 220, 20, 255]);
        assert_eq!(last.get_pixel(0, 0).0, [30, 30, 200, 255]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn neuquant_palette_and_dithering_produce_frames() {
        let (w, h) = (64u32, 64u32);
        let mut src = MemorySource::new(w, h);
        for k in 0..3u32 {
            let mut f = vec![0u8; (w * h * 4) as usize];
            for (i, px) in f.chunks_exact_mut(4).enumerate() { let x = i as u32 % w; let y = i as u32 / w; px.copy_from_slice(&[(x * 4) as u8, (y * 4) as u8, (k * 80) as u8, 255]); }
            src.push(f, 100);
        }
        let dir = std::env::temp_dir().join(format!("linkshot-gif-test2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("b.gif");
        let rep = export_gif(&mut src, &GifOptions { fps: 10.0, dithering: true, width: 32, ..Default::default() }, &out, &mut |_| true).unwrap();
        assert!(!rep.exact_palette);
        assert_eq!(rep.frames_written, 3);
        assert_eq!((rep.width, rep.height), (32, 32));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
