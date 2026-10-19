//! Times a full-page render against a view-sized tile for a tall scrolling capture.
use linkshot_render::{RenderOptions, Renderer};
fn main() {
    let (w, h) = (1500u32, 12000u32);
    let mut img = image::RgbaImage::new(w, h);
    for (x, y, p) in img.enumerate_pixels_mut() { *p = image::Rgba([(x % 256) as u8, (y % 256) as u8, ((x ^ y) % 256) as u8, 255]); }
    let png = linkshot_render::encode_png(&img).unwrap();
    let doc = linkshot_core::Document::from_png(w, h, png);
    let mut r = Renderer::new();
    let page = doc.page();
    let scale = 1.7f32;
    let t = std::time::Instant::now();
    let full = r.render_page(&doc, page, &RenderOptions { scale, checkerboard: true, skip_ids: vec![], clip: None, preview: true }).unwrap();
    println!("full page {}x{}: {:?}", full.width(), full.height(), t.elapsed());
    let t = std::time::Instant::now();
    let tile = r.render_page(&doc, page, &RenderOptions { scale, checkerboard: true, skip_ids: vec![], clip: Some([0.0, 5000.0, 2550.0, 2200.0]), preview: true }).unwrap();
    println!("tile {}x{}: {:?}", tile.width(), tile.height(), t.elapsed());
    let t = std::time::Instant::now();
    let th = r.thumbnail(&doc, page, 320, 180).unwrap();
    println!("thumbnail {}x{}: {:?}", th.width(), th.height(), t.elapsed());
    // tile must equal the same region of the full render
    let mut bad = 0;
    for y in 0..tile.height() { for x in 0..tile.width() { let a = tile.pixel(x, y).unwrap(); let b = full.pixel(x, y + 5000).unwrap(); if a != b { bad += 1; } } }
    println!("tile pixels differing from full render: {bad}");
}
