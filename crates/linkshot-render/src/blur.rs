//! Fast blurs on premultiplied RGBA8 buffers (shadows, Blur tool, Soften filter).

use tiny_skia::Pixmap;

fn box_blur_h(src: &[u8], dst: &mut [u8], w: usize, h: usize, r: usize) {
    let div = (2 * r + 1) as u32;
    for y in 0..h {
        let row = &src[y * w * 4..(y + 1) * w * 4];
        let mut acc = [0u32; 4];
        for x in 0..=r.min(w.saturating_sub(1)) {
            for c in 0..4 { acc[c] += row[x * 4 + c] as u32; }
        }
        // Edge handling: clamp (replicate edge pixel).
        for _ in 0..r { for c in 0..4 { acc[c] += row[c] as u32; } }
        for x in 0..w {
            let o = (y * w + x) * 4;
            for c in 0..4 { dst[o + c] = (acc[c] / div) as u8; }
            let add = (x + r + 1).min(w - 1);
            let sub = x.saturating_sub(r);
            let sub = if x < r { 0 } else { sub };
            for c in 0..4 {
                acc[c] += row[add * 4 + c] as u32;
                acc[c] -= row[sub * 4 + c] as u32;
            }
        }
    }
}

fn transpose(src: &[u8], dst: &mut [u8], w: usize, h: usize) {
    for y in 0..h {
        for x in 0..w {
            let s = (y * w + x) * 4;
            let d = (x * h + y) * 4;
            dst[d..d + 4].copy_from_slice(&src[s..s + 4]);
        }
    }
}

/// Approximates a gaussian blur with three box blurs. `sigma` in pixels.
pub fn gaussian_blur(pm: &mut Pixmap, sigma: f32) {
    if sigma <= 0.3 { return; }
    let w = pm.width() as usize;
    let h = pm.height() as usize;
    if w == 0 || h == 0 { return; }
    // Box radii for 3 passes (Ivan Kutskir's method).
    let w_ideal = (12.0 * sigma * sigma / 3.0 + 1.0).sqrt();
    let mut wl = w_ideal.floor() as i32;
    if wl % 2 == 0 { wl -= 1; }
    let wu = wl + 2;
    let m_ideal = (12.0 * sigma * sigma - 3.0 * (wl * wl) as f32 - 12.0 * wl as f32 - 9.0) / (-4.0 * wl as f32 - 4.0);
    let m = m_ideal.round() as i32;
    let radii: Vec<usize> = (0..3).map(|i| (if i < m { wl } else { wu } - 1) as usize / 2).collect();

    let mut a = pm.data().to_vec();
    let mut b = vec![0u8; a.len()];
    let mut t = vec![0u8; a.len()];
    for r in radii {
        if r == 0 { continue; }
        box_blur_h(&a, &mut b, w, h, r);
        transpose(&b, &mut t, w, h);
        box_blur_h(&t, &mut b, h, w, r);
        transpose(&b, &mut a, h, w);
    }
    pm.data_mut().copy_from_slice(&a);
}

/// Mosaic/pixelate with square cells of `cell` pixels (Blur tool "Pixelate").
pub fn pixelate(pm: &mut Pixmap, cell: u32) {
    let cell = cell.max(2) as usize;
    let w = pm.width() as usize;
    let h = pm.height() as usize;
    let data = pm.data_mut();
    let mut y = 0;
    while y < h {
        let mut x = 0;
        while x < w {
            let (mut acc, mut n) = ([0u32; 4], 0u32);
            for yy in y..(y + cell).min(h) {
                for xx in x..(x + cell).min(w) {
                    let o = (yy * w + xx) * 4;
                    for c in 0..4 { acc[c] += data[o + c] as u32; }
                    n += 1;
                }
            }
            if n > 0 {
                let avg = [(acc[0] / n) as u8, (acc[1] / n) as u8, (acc[2] / n) as u8, (acc[3] / n) as u8];
                for yy in y..(y + cell).min(h) {
                    for xx in x..(x + cell).min(w) {
                        let o = (yy * w + xx) * 4;
                        data[o..o + 4].copy_from_slice(&avg);
                    }
                }
            }
            x += cell;
        }
        y += cell;
    }
}

/// Replaces every pixel's colour with `color` keeping the alpha (for drop shadows).
pub fn tint_alpha(pm: &mut Pixmap, color: tiny_skia::Color, opacity: f32) {
    let (r, g, b) = (color.red(), color.green(), color.blue());
    for px in pm.data_mut().chunks_exact_mut(4) {
        let a = (px[3] as f32 * opacity).round().clamp(0.0, 255.0);
        px[0] = (r * a).round() as u8;
        px[1] = (g * a).round() as u8;
        px[2] = (b * a).round() as u8;
        px[3] = a as u8;
    }
}
