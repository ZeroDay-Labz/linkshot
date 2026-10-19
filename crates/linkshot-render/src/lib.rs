//! linkshot-render: rasterises documents with tiny-skia (used for the canvas, thumbnails and export).

pub mod blur;
pub mod effects;
pub mod objects;
pub mod scene;
pub mod text;

pub use scene::{RenderOptions, Renderer};
pub use tiny_skia::Pixmap;

use image::RgbaImage;

/// Converts straight-alpha RGBA bytes to a premultiplied tiny-skia pixmap.
pub fn pixmap_from_rgba(width: u32, height: u32, rgba: &[u8]) -> Option<Pixmap> {
    let mut pm = Pixmap::new(width, height)?;
    let data = pm.data_mut();
    for (dst, src) in data.chunks_exact_mut(4).zip(rgba.chunks_exact(4)) {
        let a = src[3] as u32;
        dst[0] = ((src[0] as u32 * a + 127) / 255) as u8;
        dst[1] = ((src[1] as u32 * a + 127) / 255) as u8;
        dst[2] = ((src[2] as u32 * a + 127) / 255) as u8;
        dst[3] = src[3];
    }
    Some(pm)
}

pub fn pixmap_from_image(img: &RgbaImage) -> Option<Pixmap> {
    pixmap_from_rgba(img.width(), img.height(), img.as_raw())
}

/// Converts a premultiplied pixmap back to straight-alpha RGBA.
pub fn pixmap_to_image(pm: &Pixmap) -> RgbaImage {
    let mut out = RgbaImage::new(pm.width(), pm.height());
    for (dst, src) in out.as_mut().chunks_exact_mut(4).zip(pm.data().chunks_exact(4)) {
        let a = src[3] as u32;
        if a == 0 {
            dst.copy_from_slice(&[0, 0, 0, 0]);
        } else {
            dst[0] = ((src[0] as u32 * 255 + a / 2) / a).min(255) as u8;
            dst[1] = ((src[1] as u32 * 255 + a / 2) / a).min(255) as u8;
            dst[2] = ((src[2] as u32 * 255 + a / 2) / a).min(255) as u8;
            dst[3] = src[3];
        }
    }
    out
}

pub fn decode_png(bytes: &[u8]) -> anyhow::Result<RgbaImage> {
    Ok(image::load_from_memory(bytes)?.to_rgba8())
}

pub fn encode_png(img: &RgbaImage) -> anyhow::Result<Vec<u8>> {
    let mut out = Vec::new();
    image::DynamicImage::ImageRgba8(img.clone()).write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)?;
    Ok(out)
}

/// Scales an image to fit within `max_w × max_h` (keeps aspect ratio).
/// Box-averages `factor`×`factor` blocks (one pass over the source); the fast first step of a large
/// reduction. Premultiplied or straight alpha both average correctly for this purpose.
pub fn decimate(img: &RgbaImage, factor: u32) -> RgbaImage {
    let f = factor.max(1);
    let (w, h) = (img.width() / f, img.height() / f);
    if f == 1 || w == 0 || h == 0 { return img.clone(); }
    let src = img.as_raw();
    let sw = img.width() as usize;
    let mut out = vec![0u8; (w * h * 4) as usize];
    let n = (f * f) as u32;
    for oy in 0..h as usize {
        let row = &mut out[oy * w as usize * 4..(oy + 1) * w as usize * 4];
        for ox in 0..w as usize {
            let mut acc = [0u32; 4];
            for dy in 0..f as usize {
                let base = ((oy * f as usize + dy) * sw + ox * f as usize) * 4;
                for dx in 0..f as usize { let p = &src[base + dx * 4..base + dx * 4 + 4]; for c in 0..4 { acc[c] += p[c] as u32; } }
            }
            for c in 0..4 { row[ox * 4 + c] = ((acc[c] + n / 2) / n) as u8; }
        }
    }
    RgbaImage::from_raw(w, h, out).unwrap()
}

pub fn thumbnail(img: &RgbaImage, max_w: u32, max_h: u32) -> RgbaImage {
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 { return img.clone(); }
    let k = (max_w as f64 / w as f64).min(max_h as f64 / h as f64).min(1.0);
    let (nw, nh) = (((w as f64 * k) as u32).max(1), ((h as f64 * k) as u32).max(1));
    image::imageops::resize(img, nw, nh, image::imageops::FilterType::Triangle)
}

pub(crate) fn skia_color(c: linkshot_core::Argb) -> tiny_skia::Color {
    tiny_skia::Color::from_rgba8(c.r, c.g, c.b, c.a)
}

pub(crate) fn skia_color_alpha(c: linkshot_core::Argb, opacity: f64) -> tiny_skia::Color {
    let a = (c.a as f64 * opacity.clamp(0.0, 1.0)).round() as u8;
    tiny_skia::Color::from_rgba8(c.r, c.g, c.b, a)
}
