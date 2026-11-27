//! Simplify: turns a screenshot of an interface into a clean schematic of it: flat panels in their
//! own colours, text lines as rounded bars, pictures as placeholders. The result is made of ordinary
//! shapes, so every part stays editable (recolour, move, delete).
//!
//! Detection works on pixels only (no text recognition), so it is fast and needs nothing installed:
//! - the page background is the most common colour;
//! - panels are large areas of one flat colour that differ from the background;
//! - text is clusters of short edges, merged along the line; long straight edges (borders, rules) are
//!   left out so a box around text does not swallow it;
//! - pictures are large clusters of dense edges.

use image::RgbaImage;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind { Background, Panel, Text, Image }

#[derive(Clone, Copy, Debug)]
pub struct Element {
    pub kind: Kind,
    /// x, y, width, height in image pixels.
    pub rect: (u32, u32, u32, u32),
    pub color: [u8; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Palette { Sampled, Light, Dark }

impl Palette {
    pub const ALL: [Palette; 3] = [Palette::Sampled, Palette::Light, Palette::Dark];
    pub fn label(self) -> &'static str { match self { Palette::Sampled => "Colours of the image", Palette::Light => "Light greys", Palette::Dark => "Dark greys" } }
}

fn luma(p: [u8; 4]) -> u8 { ((p[0] as u32 * 77 + p[1] as u32 * 151 + p[2] as u32 * 28) >> 8) as u8 }
fn lerp(a: [u8; 4], b: [u8; 4], t: f32) -> [u8; 4] {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round().clamp(0.0, 255.0) as u8;
    [m(a[0], b[0]), m(a[1], b[1]), m(a[2], b[2]), 255]
}

/// Maps a detected colour into the chosen palette.
pub fn recolor(c: [u8; 4], kind: Kind, palette: Palette) -> [u8; 4] {
    let l = luma(c) as f32 / 255.0;
    let grey = |v: f32| { let g = (v * 255.0).round().clamp(0.0, 255.0) as u8; [g, g, g, 255] };
    match palette {
        Palette::Sampled => c,
        Palette::Light => match kind { Kind::Background => grey(0.97), Kind::Panel => grey(0.80 + 0.14 * l), Kind::Text => grey(0.62), Kind::Image => grey(0.74) },
        Palette::Dark => match kind { Kind::Background => grey(0.12), Kind::Panel => grey(0.18 + 0.14 * l), Kind::Text => grey(0.48), Kind::Image => grey(0.32) },
    }
}

/// Finds the elements of `img`. `detail` (0..1): higher keeps smaller elements and separates words.
pub fn detect(img: &RgbaImage, detail: f32) -> Vec<Element> {
    let (w, h) = (img.width() as usize, img.height() as usize);
    if w < 8 || h < 8 { return vec![]; }
    let detail = detail.clamp(0.0, 1.0);
    let px = |x: usize, y: usize| img.get_pixel(x as u32, y as u32).0;
    let gray: Vec<u8> = img.pixels().map(|p| luma(p.0)).collect();
    let g = |x: usize, y: usize| gray[y * w + x] as i32;

    // Background: the most common colour (quantised), averaged.
    let bg = dominant(img, 0, 0, w, h);
    let mut out = vec![Element { kind: Kind::Background, rect: (0, 0, w as u32, h as u32), color: bg }];

    // Edge mask.
    let mut edge = vec![false; w * h];
    for y in 0..h - 1 { for x in 0..w - 1 {
        let d = (g(x + 1, y) - g(x, y)).abs().max((g(x, y + 1) - g(x, y)).abs());
        if d > 28 { edge[y * w + x] = true; }
    } }
    // Long straight runs are borders and rules, not text: take them out of the text mask.
    let max_run_h = 36usize;
    let max_run_v = 28usize;
    let mut text_mask = edge.clone();
    for y in 0..h {
        let mut x = 0;
        while x < w {
            if !edge[y * w + x] { x += 1; continue; }
            let s = x;
            while x < w && edge[y * w + x] { x += 1; }
            if x - s >= max_run_h { for k in s..x { text_mask[y * w + k] = false; } }
        }
    }
    for x in 0..w {
        let mut y = 0;
        while y < h {
            if !edge[y * w + x] { y += 1; continue; }
            let s = y;
            while y < h && edge[y * w + x] { y += 1; }
            if y - s >= max_run_v { for k in s..y { text_mask[k * w + x] = false; } }
        }
    }
    // Merge characters into lines: dilate horizontally (gap shrinks with more detail), 1 px vertically.
    let gap = (3.0 + (1.0 - detail) * 9.0).round() as usize;
    let mut merged = vec![false; w * h];
    for y in 0..h {
        let mut last: Option<usize> = None;
        for x in 0..w {
            if text_mask[y * w + x] {
                if let Some(l) = last { if x - l <= gap { for k in l..=x { merged[y * w + k] = true; } } }
                merged[y * w + x] = true;
                last = Some(x);
            }
        }
    }
    let mut dil = merged.clone();
    for y in 1..h - 1 { for x in 0..w { if merged[(y - 1) * w + x] || merged[(y + 1) * w + x] { dil[y * w + x] = true; } } }

    // Connected clusters of the merged mask → text lines or pictures.
    let min_text_h = 4usize;
    let max_text_h = 44usize;
    for (x0, y0, x1, y1, _) in components(&dil, w, h) {
        let (cw, ch) = (x1 - x0 + 1, y1 - y0 + 1);
        let n_edges = (y0..=y1).map(|y| (x0..=x1).filter(|&x| text_mask[y * w + x]).count()).sum::<usize>();
        let density = n_edges as f32 / (cw * ch) as f32;
        if ch >= min_text_h && ch <= max_text_h && cw >= ch + ch / 2 && cw >= 8 {
            // Bar colour: between the ink and the local background.
            let local = dominant(img, x0, y0, cw, ch);
            let ink = ink_color(img, x0, y0, cw, ch, local);
            out.push(Element { kind: Kind::Text, rect: (x0 as u32, y0 as u32, cw as u32, ch as u32), color: lerp(local, ink, 0.55) });
        } else if ch > max_text_h && cw > 24 && density > 0.06 {
            out.push(Element { kind: Kind::Image, rect: (x0 as u32, y0 as u32, cw as u32, ch as u32), color: lerp(bg, [128, 128, 128, 255], 0.5) });
        }
    }

    // Words on the same line become one bar (fewer, cleaner bars at lower detail).
    merge_lines(&mut out, detail);

    // Panels: 4×4 blocks of one flat colour, joined with neighbours of the same colour.
    let bs = 4usize;
    let (bw, bh) = (w / bs, h / bs);
    let q = |c: [u8; 4]| ((c[0] as u32 >> 3) << 10) | ((c[1] as u32 >> 3) << 5) | (c[2] as u32 >> 3);
    let mut block: Vec<Option<u32>> = vec![None; bw * bh];
    for by in 0..bh { for bx in 0..bw {
        let first = px(bx * bs, by * bs);
        let flat = first[3] > 200 && (0..bs).all(|dy| (0..bs).all(|dx| { let p = px(bx * bs + dx, by * bs + dy); (0..3).all(|k| (p[k] as i32 - first[k] as i32).abs() <= 6) }));
        if flat { block[by * bw + bx] = Some(q(first)); }
    } }
    let bgq = q(bg);
    let min_blocks = (40.0 - detail * 30.0) as usize;
    let mut seen = vec![false; bw * bh];
    let mut panels = Vec::new();
    for start in 0..bw * bh {
        let Some(c) = block[start] else { continue };
        if seen[start] || c == bgq { continue; }
        let (mut x0, mut y0, mut x1, mut y1, mut n) = (usize::MAX, usize::MAX, 0, 0, 0usize);
        let mut stack = vec![start];
        seen[start] = true;
        while let Some(i) = stack.pop() {
            let (bx, by) = (i % bw, i / bw);
            x0 = x0.min(bx); y0 = y0.min(by); x1 = x1.max(bx); y1 = y1.max(by); n += 1;
            let mut push = |j: usize| { if !seen[j] && block[j] == Some(c) { seen[j] = true; stack.push(j); } };
            if bx > 0 { push(i - 1); } if bx + 1 < bw { push(i + 1); } if by > 0 { push(i - bw); } if by + 1 < bh { push(i + bw); }
        }
        let area = (x1 - x0 + 1) * (y1 - y0 + 1);
        // Text inside a panel leaves holes; the panel still counts when its colour covers most of it.
        if n >= min_blocks && n as f32 >= area as f32 * 0.45 && x1 > x0 && y1 > y0 {
            let r = ((x0 * bs) as u32, (y0 * bs) as u32, ((x1 - x0 + 1) * bs) as u32, ((y1 - y0 + 1) * bs) as u32);
            // The colour of a block of this panel (its bounding box corner may lie outside it).
            let color = px((start % bw) * bs + bs / 2, (start / bw) * bs + bs / 2);
            panels.push((area, Element { kind: Kind::Panel, rect: r, color: [color[0], color[1], color[2], 255] }));
        }
    }
    // Large panels first so smaller ones (buttons on a toolbar) are drawn on top.
    panels.sort_by_key(|p| std::cmp::Reverse(p.0));
    let text_and_images: Vec<Element> = out.drain(1..).collect();
    out.extend(panels.into_iter().map(|p| p.1));
    // Pictures, then text on top.
    out.extend(text_and_images.iter().filter(|e| e.kind == Kind::Image).copied());
    out.extend(text_and_images.iter().filter(|e| e.kind == Kind::Text).copied());
    out
}

/// Joins text boxes that sit on the same line with a small gap between them.
fn merge_lines(els: &mut Vec<Element>, detail: f32) {
    let mut texts: Vec<Element> = els.iter().filter(|e| e.kind == Kind::Text).copied().collect();
    els.retain(|e| e.kind != Kind::Text);
    texts.sort_by_key(|e| (e.rect.1, e.rect.0));
    let mut merged: Vec<Element> = Vec::new();
    for t in texts {
        let (x, y, w, h) = t.rect;
        let joined = merged.iter_mut().rev().take(8).find(|m| {
            let (mx, my, mw, mh) = m.rect;
            let overlap = (y + h).min(my + mh) as i64 - y.max(my) as i64;
            let gap = x as i64 - (mx + mw) as i64;
            let max_gap = (h.max(mh) as f32 * (2.0 - 1.5 * detail)) as i64;
            overlap as f32 >= h.min(mh) as f32 * 0.6 && gap >= -2 && gap <= max_gap
        });
        match joined {
            Some(m) => {
                let (mx, my, mw, mh) = m.rect;
                let (x0, y0) = (mx.min(x), my.min(y));
                let (x1, y1) = ((mx + mw).max(x + w), (my + mh).max(y + h));
                m.rect = (x0, y0, x1 - x0, y1 - y0);
            }
            None => merged.push(t),
        }
    }
    els.extend(merged);
}

/// Bounding boxes (x0, y0, x1, y1, pixel count) of 4-connected regions of `mask`.
fn components(mask: &[bool], w: usize, h: usize) -> Vec<(usize, usize, usize, usize, usize)> {
    let mut seen = vec![false; w * h];
    let mut out = Vec::new();
    let mut stack = Vec::new();
    for start in 0..w * h {
        if !mask[start] || seen[start] { continue; }
        let (mut x0, mut y0, mut x1, mut y1, mut n) = (usize::MAX, usize::MAX, 0, 0, 0);
        seen[start] = true;
        stack.push(start);
        while let Some(i) = stack.pop() {
            let (x, y) = (i % w, i / w);
            x0 = x0.min(x); y0 = y0.min(y); x1 = x1.max(x); y1 = y1.max(y); n += 1;
            let mut push = |j: usize| { if mask[j] && !seen[j] { seen[j] = true; stack.push(j); } };
            if x > 0 { push(i - 1); } if x + 1 < w { push(i + 1); } if y > 0 { push(i - w); } if y + 1 < h { push(i + w); }
        }
        out.push((x0, y0, x1, y1, n));
    }
    out
}

/// Most common colour (5 bits per channel) in a rectangle, averaged over the pixels of that bucket.
fn dominant(img: &RgbaImage, x0: usize, y0: usize, w: usize, h: usize) -> [u8; 4] {
    let mut counts: std::collections::HashMap<u32, (u32, [u64; 3])> = std::collections::HashMap::new();
    let step = if w * h > 400_000 { 3 } else { 1 };
    for y in (y0..y0 + h).step_by(step) { for x in (x0..x0 + w).step_by(step) {
        let p = img.get_pixel(x as u32, y as u32).0;
        let k = ((p[0] as u32 >> 3) << 10) | ((p[1] as u32 >> 3) << 5) | (p[2] as u32 >> 3);
        let e = counts.entry(k).or_insert((0, [0; 3]));
        e.0 += 1; for c in 0..3 { e.1[c] += p[c] as u64; }
    } }
    let (_, (n, sum)) = counts.into_iter().max_by_key(|(_, (n, _))| *n).unwrap_or((0, (1, [255; 3])));
    let n = n.max(1) as u64;
    [(sum[0] / n) as u8, (sum[1] / n) as u8, (sum[2] / n) as u8, 255]
}

/// Average of the pixels in a rectangle that clearly differ from `bg` (the glyphs).
fn ink_color(img: &RgbaImage, x0: usize, y0: usize, w: usize, h: usize, bg: [u8; 4]) -> [u8; 4] {
    let (mut n, mut sum) = (0u64, [0u64; 3]);
    for y in y0..y0 + h { for x in x0..x0 + w {
        let p = img.get_pixel(x as u32, y as u32).0;
        if (luma(p) as i32 - luma(bg) as i32).abs() > 60 { n += 1; for c in 0..3 { sum[c] += p[c] as u64; } }
    } }
    if n == 0 { return if luma(bg) > 128 { [60, 60, 60, 255] } else { [200, 200, 200, 255] }; }
    [(sum[0] / n) as u8, (sum[1] / n) as u8, (sum[2] / n) as u8, 255]
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn fill(img: &mut RgbaImage, x: u32, y: u32, w: u32, h: u32, c: [u8; 4]) {
        for yy in y..y + h { for xx in x..x + w { img.put_pixel(xx, yy, Rgba(c)); } }
    }
    /// A line of fake text: glyph-like strokes 2 px wide with gaps.
    fn text(img: &mut RgbaImage, x: u32, y: u32, chars: u32, c: [u8; 4]) {
        for i in 0..chars {
            let gx = x + i * 9;
            if i % 6 == 5 { continue; } // word gap
            fill(img, gx, y, 2, 12, c);
            fill(img, gx + 2, y + 5, 3, 2, c);
            fill(img, gx + 5, y + 2, 2, 10, c);
        }
    }

    fn ui_mock() -> RgbaImage {
        let mut img = RgbaImage::from_pixel(800, 500, Rgba([248, 248, 250, 255]));
        // Toolbar panel with a title.
        fill(&mut img, 0, 0, 800, 60, [40, 90, 200, 255]);
        text(&mut img, 20, 24, 20, [255, 255, 255, 255]);
        // A bordered box with two lines of text in it.
        for x in 40..400 { img.put_pixel(x, 100, Rgba([180, 180, 180, 255])); img.put_pixel(x, 220, Rgba([180, 180, 180, 255])); }
        for y in 100..=220 { img.put_pixel(40, y, Rgba([180, 180, 180, 255])); img.put_pixel(399, y, Rgba([180, 180, 180, 255])); }
        text(&mut img, 60, 130, 30, [30, 30, 30, 255]);
        text(&mut img, 60, 170, 24, [30, 30, 30, 255]);
        // A "photo": noisy block.
        let mut s = 7u32;
        for y in 260..440 { for x in 460..740 { s ^= s << 13; s ^= s >> 17; s ^= s << 5; img.put_pixel(x, y, Rgba([(s & 255) as u8, ((s >> 8) & 255) as u8, ((s >> 16) & 255) as u8, 255])); } }
        img
    }

    #[test]
    fn finds_panels_text_and_pictures() {
        let img = ui_mock();
        let els = detect(&img, 0.5);
        assert_eq!(els[0].kind, Kind::Background);
        assert!((els[0].color[0] as i32 - 248).abs() < 4);
        let panels: Vec<_> = els.iter().filter(|e| e.kind == Kind::Panel).collect();
        assert!(panels.iter().any(|p| p.rect.2 >= 780 && p.rect.3 >= 52 && p.rect.1 == 0), "toolbar panel: {panels:?}");
        let texts: Vec<_> = els.iter().filter(|e| e.kind == Kind::Text).collect();
        // The title and both lines inside the bordered box, each found as its own line.
        assert!(texts.iter().any(|t| t.rect.1 >= 20 && t.rect.1 <= 26 && t.rect.2 > 120), "title: {texts:?}");
        assert!(texts.iter().any(|t| (126..=132).contains(&t.rect.1)), "first box line: {texts:?}");
        assert!(texts.iter().any(|t| (166..=172).contains(&t.rect.1)), "second box line: {texts:?}");
        assert!(texts.iter().all(|t| t.rect.3 <= 20), "lines must not merge: {texts:?}");
        let images: Vec<_> = els.iter().filter(|e| e.kind == Kind::Image).collect();
        assert!(images.iter().any(|i| i.rect.0 <= 462 && i.rect.2 >= 270 && i.rect.3 >= 170), "photo: {images:?}");
        // Text on the blue toolbar gets a bar between blue and white.
        let title = texts.iter().find(|t| t.rect.1 < 40).unwrap();
        assert!(title.color[2] > 200 && title.color[0] > 100, "{:?}", title.color);
    }

    /// LINKSHOT_SIMPLIFY_IN=shot.png LINKSHOT_SIMPLIFY_OUT=out.png cargo test simplify_file -- --ignored
    #[test]
    #[ignore]
    fn simplify_file() {
        let img = image::open(std::env::var("LINKSHOT_SIMPLIFY_IN").unwrap()).unwrap().to_rgba8();
        let t0 = std::time::Instant::now();
        let els = detect(&img, 0.5);
        eprintln!("{} elements in {} ms", els.len(), t0.elapsed().as_millis());
        let mut out = img.clone();
        for e in &els {
            let (x, y, w, h) = e.rect;
            let (y, h) = if e.kind == Kind::Text { (y + h / 5, (h * 3 / 5).max(2)) } else { (y, h) };
            fill(&mut out, x, y, w.min(img.width() - x), h.min(img.height() - y), e.color);
        }
        out.save(std::env::var("LINKSHOT_SIMPLIFY_OUT").unwrap()).unwrap();
    }

    #[test]
    fn grey_palettes() {
        assert_eq!(recolor([10, 200, 30, 255], Kind::Text, Palette::Light), recolor([200, 10, 30, 255], Kind::Text, Palette::Light));
        assert!(luma(recolor([0, 0, 0, 255], Kind::Background, Palette::Dark)) < 40);
    }
}

/// Simplify settings in the side panel.
#[derive(Clone, Copy, Debug)]
pub struct Options { pub palette: Palette, pub detail: f32, pub keep_background: bool }
impl Default for Options { fn default() -> Self { Self { palette: Palette::Sampled, detail: 0.5, keep_background: false } } }

/// Builds the shapes for the elements (canvas coordinates = image pixels).
pub fn shapes(els: &[Element], opts: &Options) -> Vec<linkshot_core::document::CaptureObject> {
    use linkshot_core::document::CaptureObject;
    use linkshot_core::enums::ShapeShape;
    use linkshot_core::{Argb, Pt};
    els.iter().filter(|e| !(opts.keep_background && e.kind == Kind::Background)).map(|e| {
        let c = recolor(e.color, e.kind, opts.palette);
        let (x, y, w, h) = (e.rect.0 as f64, e.rect.1 as f64, e.rect.2 as f64, e.rect.3 as f64);
        // Text becomes a rounded bar about the height of the letters' bodies.
        let (y, h) = if e.kind == Kind::Text { (y + h * 0.22, (h * 0.56).max(3.0)) } else { (y, h) };
        let mut o = CaptureObject::new("Shape");
        o.points_array = vec![Pt::new(x, y), Pt::new(x + w, y + h)];
        let argb = Argb::new(255, c[0], c[1], c[2]);
        o.background_color = Some(argb);
        o.foreground_color = Some(argb);
        o.stroke_width = Some(0.0);
        o.opacity = Some(100.0);
        o.drop_shadow_enabled = Some(false);
        o.tool_shape = Some(if e.kind == Kind::Text { ShapeShape::RoundedRectangle } else { ShapeShape::Rectangle });
        o.corner_radius_ratio = Some(if e.kind == Kind::Text { 1.0 } else { 0.0 });
        o
    }).collect()
}

/// Simplifies the current image: detects its elements and adds them as shapes (one undo step).
pub fn apply(app: &mut crate::app::App) {
    let Some(i) = app.editor.current else { return };
    if app.editor.docs[i].is_video() { app.editor.set_status("Simplify works on images."); return; }
    let opts = app.editor.simplify;
    // Detect on the picture itself, not on annotations already added.
    let mut bg_doc = app.editor.docs[i].doc.clone();
    bg_doc.page_mut().capture_objects.clear();
    let Some(img) = app.editor.renderer.flatten(&bg_doc, bg_doc.page()) else { return };
    let t0 = std::time::Instant::now();
    let els = detect(&img, opts.detail);
    let objs = shapes(&els, &opts);
    let n = objs.len();
    app.editor.docs[i].edit("Simplify", |doc| doc.page_mut().capture_objects.extend(objs));
    app.editor.renderer.invalidate();
    app.editor.canvas.invalidate();
    let texts = els.iter().filter(|e| e.kind == Kind::Text).count();
    app.editor.set_status(format!("Simplified into {n} shapes ({texts} text lines) in {} ms. They are ordinary shapes: select, recolour or delete them; Undo removes them all.", t0.elapsed().as_millis()));
}
