//! Whole-image effects (the "Effects" panel). All operate on straight-alpha RGBA images.

use image::{Rgba, RgbaImage};
use linkshot_core::Argb;

/// Adds a solid border of `width` px around the image (grows the image).
pub fn border(img: &RgbaImage, color: Argb, width: u32) -> RgbaImage {
    let (w, h) = img.dimensions();
    let mut out = RgbaImage::from_pixel(w + 2 * width, h + 2 * width, Rgba([color.r, color.g, color.b, color.a]));
    image::imageops::overlay(&mut out, img, width as i64, width as i64);
    out
}

/// Drop shadow around the whole image. `dir` is a 3×3 grid position (-1..1, -1..1); size = offset & blur.
pub fn shadow(img: &RgbaImage, color: Argb, size: u32, dir: (i32, i32)) -> RgbaImage {
    let (w, h) = img.dimensions();
    let pad = size * 2;
    let mut canvas = RgbaImage::new(w + 2 * pad, h + 2 * pad);
    let mut sh = RgbaImage::new(w + 2 * pad, h + 2 * pad);
    for y in 0..h { for x in 0..w { let a = img.get_pixel(x, y)[3]; sh.put_pixel(x + pad, y + pad, Rgba([color.r, color.g, color.b, ((a as u32 * color.a as u32) / 255) as u8])); } }
    let mut pm = crate::pixmap_from_image(&sh).unwrap();
    crate::blur::gaussian_blur(&mut pm, size as f32 * 0.6);
    let sh = crate::pixmap_to_image(&pm);
    let (dx, dy) = ((dir.0 * size as i32) as i64, (dir.1 * size as i32) as i64);
    image::imageops::overlay(&mut canvas, &sh, dx, dy);
    image::imageops::overlay(&mut canvas, img, pad as i64, pad as i64);
    out_trim_transparent(&canvas)
}

fn out_trim_transparent(img: &RgbaImage) -> RgbaImage {
    let (w, h) = img.dimensions();
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for y in 0..h { for x in 0..w { if img.get_pixel(x, y)[3] > 0 { x0 = x0.min(x); y0 = y0.min(y); x1 = x1.max(x); y1 = y1.max(y); } } }
    if x1 < x0 || y1 < y0 { return img.clone(); }
    image::imageops::crop_imm(img, x0, y0, x1 - x0 + 1, y1 - y0 + 1).to_image()
}

pub fn grayscale(img: &RgbaImage) -> RgbaImage {
    let mut out = img.clone();
    for p in out.pixels_mut() { let l = (0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32) as u8; p[0] = l; p[1] = l; p[2] = l; }
    out
}

pub fn invert(img: &RgbaImage) -> RgbaImage {
    let mut out = img.clone();
    for p in out.pixels_mut() { p[0] = 255 - p[0]; p[1] = 255 - p[1]; p[2] = 255 - p[2]; }
    out
}

/// Black & white with an intensity threshold (0..255).
pub fn monochrome(img: &RgbaImage, threshold: u8) -> RgbaImage {
    let mut out = img.clone();
    for p in out.pixels_mut() { let l = (0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32) as u8; let v = if l >= threshold { 255 } else { 0 }; p[0] = v; p[1] = v; p[2] = v; }
    out
}

pub fn opacity(img: &RgbaImage, percent: f32) -> RgbaImage {
    let mut out = img.clone();
    let k = (percent / 100.0).clamp(0.0, 1.0);
    for p in out.pixels_mut() { p[3] = (p[3] as f32 * k) as u8; }
    out
}

pub fn soften(img: &RgbaImage, amount: f32) -> RgbaImage {
    let mut pm = crate::pixmap_from_image(img).unwrap();
    crate::blur::gaussian_blur(&mut pm, amount.max(0.5));
    crate::pixmap_to_image(&pm)
}

/// `amount` ≥ 0: strength of the unsharp mask (sigma); 0 leaves the image untouched.
pub fn sharpen(img: &RgbaImage, amount: f32) -> RgbaImage {
    if amount <= 0.0 { return img.clone(); }
    image::imageops::unsharpen(img, (amount * 1.5).clamp(0.3, 12.0), 0)
}

/// Brightness/contrast/hue/saturation/gamma adjustment (all around neutral = 0 / 1.0).
pub fn color_adjust(img: &RgbaImage, brightness: f32, contrast: f32, hue_deg: f32, saturation: f32, gamma: f32) -> RgbaImage {
    let mut out = img.clone();
    let c = (contrast / 100.0).clamp(-1.0, 1.0);
    let cf = if c >= 0.0 { 1.0 + c * 3.0 } else { 1.0 + c };
    let g = if gamma > 0.0 { 1.0 / gamma } else { 1.0 };
    for p in out.pixels_mut() {
        let mut rgb = [p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0];
        if hue_deg != 0.0 || saturation != 0.0 {
            let (mut h, mut s, v) = rgb_to_hsv(rgb);
            h = (h + hue_deg).rem_euclid(360.0);
            s = (s * (1.0 + saturation / 100.0)).clamp(0.0, 1.0);
            rgb = hsv_to_rgb(h, s, v);
        }
        for ch in rgb.iter_mut() {
            *ch = ((*ch - 0.5) * cf + 0.5 + brightness / 100.0).clamp(0.0, 1.0);
            *ch = ch.powf(g);
        }
        p[0] = (rgb[0] * 255.0).round() as u8; p[1] = (rgb[1] * 255.0).round() as u8; p[2] = (rgb[2] * 255.0).round() as u8;
    }
    out
}

fn rgb_to_hsv(c: [f32; 3]) -> (f32, f32, f32) {
    let max = c[0].max(c[1]).max(c[2]);
    let min = c[0].min(c[1]).min(c[2]);
    let d = max - min;
    let h = if d == 0.0 { 0.0 } else if max == c[0] { 60.0 * (((c[1] - c[2]) / d) % 6.0) } else if max == c[1] { 60.0 * ((c[2] - c[0]) / d + 2.0) } else { 60.0 * ((c[0] - c[1]) / d + 4.0) };
    (h.rem_euclid(360.0), if max == 0.0 { 0.0 } else { d / max }, max)
}
fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [f32; 3] {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h / 60.0) as i32 { 0 => (c, x, 0.0), 1 => (x, c, 0.0), 2 => (0.0, c, x), 3 => (0.0, x, c), 4 => (x, 0.0, c), _ => (c, 0.0, x) };
    [r + m, g + m, b + m]
}

/// Replaces `from` with `to` within `tolerance` (0..255 max channel distance).
pub fn color_replace(img: &RgbaImage, from: Argb, to: Argb, tolerance: u8, swap: bool) -> RgbaImage {
    let mut out = img.clone();
    let near = |p: &Rgba<u8>, c: Argb| (p[0] as i32 - c.r as i32).abs() <= tolerance as i32 && (p[1] as i32 - c.g as i32).abs() <= tolerance as i32 && (p[2] as i32 - c.b as i32).abs() <= tolerance as i32;
    for p in out.pixels_mut() {
        if near(p, from) { *p = Rgba([to.r, to.g, to.b, p[3]]); }
        else if swap && near(p, to) { *p = Rgba([from.r, from.g, from.b, p[3]]); }
    }
    out
}

/// Scales to the given size.
pub fn scale(img: &RgbaImage, w: u32, h: u32, smooth: bool) -> RgbaImage {
    image::imageops::resize(img, w.max(1), h.max(1), if smooth { image::imageops::FilterType::CatmullRom } else { image::imageops::FilterType::Nearest })
}

/// Automatic trim: removes edges that are a solid colour (within tolerance) ("Trim / Automatic").
pub fn auto_trim(img: &RgbaImage, tolerance: u8) -> RgbaImage {
    let (w, h) = img.dimensions();
    if w < 2 || h < 2 { return img.clone(); }
    let bg = *img.get_pixel(0, 0);
    let same = |p: &Rgba<u8>| (0..4).all(|i| (p[i] as i32 - bg[i] as i32).abs() <= tolerance as i32);
    let mut x0 = 0; while x0 < w - 1 && (0..h).all(|y| same(img.get_pixel(x0, y))) { x0 += 1; }
    let mut x1 = w - 1; while x1 > x0 && (0..h).all(|y| same(img.get_pixel(x1, y))) { x1 -= 1; }
    let mut y0 = 0; while y0 < h - 1 && (0..w).all(|x| same(img.get_pixel(x, y0))) { y0 += 1; }
    let mut y1 = h - 1; while y1 > y0 && (0..w).all(|x| same(img.get_pixel(x, y1))) { y1 -= 1; }
    image::imageops::crop_imm(img, x0, y0, x1 - x0 + 1, y1 - y0 + 1).to_image()
}

pub fn rotate(img: &RgbaImage, degrees: i32) -> RgbaImage {
    match degrees.rem_euclid(360) {
        90 => image::imageops::rotate90(img),
        180 => image::imageops::rotate180(img),
        270 => image::imageops::rotate270(img),
        _ => img.clone(),
    }
}
pub fn flip_h(img: &RgbaImage) -> RgbaImage { image::imageops::flip_horizontal(img) }
pub fn flip_v(img: &RgbaImage) -> RgbaImage { image::imageops::flip_vertical(img) }

/// Torn/wave/saw edge effect on the chosen sides: cuts the edge with a noisy/periodic profile.
pub fn edge(img: &RgbaImage, kind: linkshot_core::enums::EdgeType, size: u32, top: bool, bottom: bool, left: bool, right: bool) -> RgbaImage {
    use linkshot_core::enums::EdgeType::*;
    let (w, h) = img.dimensions();
    let mut out = img.clone();
    let size = size.max(1) as f32;
    let profile = |t: f32, seed: u32| -> f32 {
        match kind {
            Torn => { let n = ((t * 7.3 + seed as f32).sin() * 43758.5453).fract().abs(); 0.3 + 0.7 * n }
            Wave => 0.5 + 0.5 * (t * std::f32::consts::TAU * 2.0 / size).sin(),
            Saw => ((t / size) % 1.0).abs(),
            Sharktooth => 1.0 - ((t / size) % 1.0 - 0.5).abs() * 2.0,
            Fade => 1.0,
            Bevel => 1.0,
            NoEdge => 0.0,
        }
    };
    for y in 0..h {
        for x in 0..w {
            let p = out.get_pixel_mut(x, y);
            let mut a = p[3] as f32;
            if top { let depth = profile(x as f32, 1) * size; if (y as f32) < depth { a = if kind == Fade { a * (y as f32 / size).min(1.0) } else { 0.0 }; } }
            if bottom { let depth = profile(x as f32, 2) * size; if (y as f32) > h as f32 - 1.0 - depth { a = if kind == Fade { a * ((h as f32 - 1.0 - y as f32) / size).min(1.0) } else { 0.0 }; } }
            if left { let depth = profile(y as f32, 3) * size; if (x as f32) < depth { a = if kind == Fade { a * (x as f32 / size).min(1.0) } else { 0.0 }; } }
            if right { let depth = profile(y as f32, 4) * size; if (x as f32) > w as f32 - 1.0 - depth { a = if kind == Fade { a * ((w as f32 - 1.0 - x as f32) / size).min(1.0) } else { 0.0 }; } }
            p[3] = a as u8;
        }
    }
    out
}
