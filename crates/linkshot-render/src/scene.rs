//! Renders a page (background + vector objects) into a pixmap.

use crate::blur;
use crate::objects::*;
use crate::text::TextEngine;
use crate::{pixmap_from_image, skia_color, skia_color_alpha};
use linkshot_core::document::{CaptureObject, Document, Page};
use linkshot_core::enums::*;
use linkshot_core::geometry::{Pt, Rect};
use linkshot_core::rtf::RichText;
use std::collections::HashMap;
use std::sync::Arc;
use tiny_skia::{BlendMode, FillRule, LineCap as SkCap, LineJoin, Paint, Pixmap, PixmapPaint, Stroke, StrokeDash, Transform};

#[derive(Clone, Debug)]
pub struct RenderOptions {
    /// Device pixels per document pixel.
    pub scale: f32,
    /// Draw a checkerboard where the canvas is transparent (on-screen only).
    pub checkerboard: bool,
    /// Skip these object ids (e.g. the one being edited).
    pub skip_ids: Vec<String>,
    /// Render only this device-space rectangle (x, y, w, h) of the scaled page; the result is w×h.
    /// Lets the editor draw just the visible part of a huge (scrolling) capture.
    pub clip: Option<[f32; 4]>,
    /// On-screen preview: bilinear instead of bicubic when the background is enlarged (several
    /// times faster; exports always use the full-quality path).
    pub preview: bool,
}
impl Default for RenderOptions {
    fn default() -> Self { Self { scale: 1.0, checkerboard: false, skip_ids: vec![], clip: None, preview: false } }
}

/// A cached bitmap, and whether every pixel is opaque (checked once, when it is cached).
struct Bitmap { pm: Pixmap, opaque: bool }

impl Bitmap {
    fn new(pm: Pixmap) -> Self { let opaque = pm.pixels().iter().all(|p| p.alpha() == 255); Self { pm, opaque } }
}

impl std::ops::Deref for Bitmap {
    type Target = Pixmap;
    fn deref(&self) -> &Pixmap { &self.pm }
}

pub struct Renderer {
    pub text: TextEngine,
    /// Decoded background bitmaps keyed by blob name.
    /// Shared, not cloned, on every lookup: a frame used to copy the full-size capture just to read it.
    bg_cache: HashMap<String, Arc<Bitmap>>,
    /// Background pre-downscaled with a Lanczos filter, keyed by (member, w, h): drawing a 1:1 pixmap
    /// is both faster and far sharper than tiny-skia's bilinear sampling at small scales.
    bg_scaled: HashMap<(String, u32, u32), Arc<Bitmap>>,
    img_cache: HashMap<String, Arc<Pixmap>>,
}

impl Default for Renderer {
    fn default() -> Self { Self::new() }
}

impl Renderer {
    pub fn new() -> Self {
        Self { text: TextEngine::new(), bg_cache: HashMap::new(), bg_scaled: HashMap::new(), img_cache: HashMap::new() }
    }

    pub fn invalidate(&mut self) {
        self.bg_cache.clear();
        self.bg_scaled.clear();
        self.img_cache.clear();
    }

    fn background(&mut self, doc: &Document, page: &Page) -> Option<Arc<Bitmap>> {
        let key = page.capture_background_image.clone();
        if let Some(p) = self.bg_cache.get(&key) { return Some(p.clone()); }
        let bytes = doc.background_png(page)?;
        let img = crate::decode_png(bytes).ok()?;
        let pm = Arc::new(Bitmap::new(pixmap_from_image(&img)?));
        self.bg_cache.insert(key, pm.clone());
        Some(pm)
    }

    fn background_scaled(&mut self, doc: &Document, page: &Page, w: u32, h: u32) -> Option<Arc<Bitmap>> {
        let key = (page.capture_background_image.clone(), w, h);
        if let Some(p) = self.bg_scaled.get(&key) { return Some(p.clone()); }
        let bg = self.background(doc, page)?;
        // Pixmap data is premultiplied RGBA; resizing premultiplied pixels is the correct thing to do.
        let src = image::RgbaImage::from_raw(bg.width(), bg.height(), bg.data().to_vec())?;
        // Large reductions (fit view of a tall scrolling capture, thumbnails): box-average by an
        // integer factor first, which is one cheap pass over the source, then resample the rest.
        let factor = ((bg.width() as f32 / w.max(1) as f32).min(bg.height() as f32 / h.max(1) as f32) / 2.0).floor() as u32;
        let src = if factor >= 2 { crate::decimate(&src, factor) } else { src };
        let filter = if (w as u64 * h as u64) > 4_000_000 { image::imageops::FilterType::Triangle } else { image::imageops::FilterType::Lanczos3 };
        let small = image::imageops::resize(&src, w.max(1), h.max(1), filter);
        let pm = Arc::new(Bitmap::new(Pixmap::from_vec(small.into_raw(), tiny_skia::IntSize::from_wh(w.max(1), h.max(1))?)?));
        if self.bg_scaled.len() > 6 { self.bg_scaled.clear(); }
        self.bg_scaled.insert(key, pm.clone());
        Some(pm)
    }

    fn image_object(&mut self, obj: &CaptureObject) -> Option<Arc<Pixmap>> {
        let key = obj.object_id.clone();
        if let Some(p) = self.img_cache.get(&key) { return Some(p.clone()); }
        let b64 = obj.image_data.as_deref()?;
        let bytes = base64_decode(b64)?;
        let img = crate::decode_png(&bytes).ok()?;
        let pm = Arc::new(pixmap_from_image(&img)?);
        self.img_cache.insert(key, pm.clone());
        Some(pm)
    }

    /// Full page render at `opts.scale`.
    pub fn render_page(&mut self, doc: &Document, page: &Page, opts: &RenderOptions) -> Option<Pixmap> {
        let s = opts.scale;
        let (w, h, off) = match opts.clip {
            Some([x, y, cw, ch]) => (cw.ceil().max(1.0) as u32, ch.ceil().max(1.0) as u32, Transform::from_translate(-x, -y)),
            None => (((page.capture_canvas_width as f32) * s).ceil().max(1.0) as u32, ((page.capture_canvas_height as f32) * s).ceil().max(1.0) as u32, Transform::identity()),
        };
        let mut pm = Pixmap::new(w, h)?;
        self.draw_background(&mut pm, doc, page, opts);
        for obj in &page.capture_objects {
            if opts.skip_ids.iter().any(|id| *id == obj.object_id) { continue; }
            self.draw_object_at(&mut pm, doc, page, obj, s, off);
        }
        if opts.clip.is_none() {
            if let Some(r) = page.corner_radius_ratio.filter(|r| *r > 0.0) { apply_corner_mask(&mut pm, r as f32); }
        }
        Some(pm)
    }

    /// A thumbnail no larger than `max_w`×`max_h`, rendered directly at the reduced scale (much faster
    /// than flattening a large capture at full size first).
    pub fn thumbnail(&mut self, doc: &Document, page: &Page, max_w: u32, max_h: u32) -> Option<image::RgbaImage> {
        let (w, h) = (page.capture_canvas_width.max(1.0) as f32, page.capture_canvas_height.max(1.0) as f32);
        let scale = (max_w as f32 / w).min(max_h as f32 / h).min(1.0).max(0.001);
        let pm = self.render_page(doc, page, &RenderOptions { scale, checkerboard: false, skip_ids: vec![], clip: None, preview: false })?;
        Some(crate::pixmap_to_image(&pm))
    }

    /// The flattened image (background + objects) as straight-alpha RGBA — what gets exported.
    pub fn flatten(&mut self, doc: &Document, page: &Page) -> Option<image::RgbaImage> {
        let pm = self.render_page(doc, page, &RenderOptions::default())?;
        Some(crate::pixmap_to_image(&pm))
    }

    pub fn draw_background(&mut self, pm: &mut Pixmap, doc: &Document, page: &Page, opts: &RenderOptions) {
        let s = opts.scale;
        let canvas = page.capture_background_color;
        let off = opts.clip.map(|c| (-c[0], -c[1])).unwrap_or((0.0, 0.0));
        // Choose the background bitmap and its placement first: an opaque capture that covers the whole
        // tile hides the checkerboard and canvas colour, and is copied row by row instead of going
        // through the raster pipeline (this used to be a large fixed cost of every editor frame).
        let mut layer: Option<(Arc<Bitmap>, Transform, tiny_skia::FilterQuality)> = None;
        if let Some(bg) = self.background(doc, page) {
            let loc = page.capture_background_location;
            // mac files may scale the background (BackgroundSize); Windows draws 1:1.
            let (sx, sy) = match page.background_size {
                Some(sz) if sz.x > 0.0 && sz.y > 0.0 => (sz.x as f32 / bg.width() as f32, sz.y as f32 / bg.height() as f32),
                _ => (1.0, 1.0),
            };
            let (fx, fy) = (sx * s, sy * s);
            if fx < 0.999 || fy < 0.999 {
                let tw = (bg.width() as f32 * fx).round().max(1.0) as u32;
                let th = (bg.height() as f32 * fy).round().max(1.0) as u32;
                if let Some(small) = self.background_scaled(doc, page, tw, th) {
                    layer = Some((small, Transform::from_translate(loc.x as f32 * s + off.0, loc.y as f32 * s + off.1), tiny_skia::FilterQuality::Bilinear));
                }
            }
            if layer.is_none() {
                let quality = if (fx - fx.round()).abs() < 1e-3 && (fy - fy.round()).abs() < 1e-3 { tiny_skia::FilterQuality::Nearest } else if opts.preview { tiny_skia::FilterQuality::Bilinear } else { tiny_skia::FilterQuality::Bicubic };
                layer = Some((bg, Transform::from_scale(fx, fy).post_translate(loc.x as f32 * s + off.0, loc.y as f32 * s + off.1), quality));
            }
        }
        let blit = layer.as_ref().and_then(|(img, t, _)| {
            let whole = |v: f32| (v - v.round()).abs() < 1e-4;
            let unscaled = t.sx == 1.0 && t.sy == 1.0 && t.kx == 0.0 && t.ky == 0.0;
            (unscaled && whole(t.tx) && whole(t.ty) && img.opaque).then(|| (t.tx.round() as i32, t.ty.round() as i32))
        });
        let covers = match (&layer, blit) {
            (Some((img, _, _)), Some((x, y))) => x <= 0 && y <= 0 && x + img.width() as i32 >= pm.width() as i32 && y + img.height() as i32 >= pm.height() as i32,
            _ => false,
        };
        if !covers {
            if opts.checkerboard && canvas.a < 255 {
                draw_checkerboard(pm, (8.0 * s).max(4.0) as u32);
            }
            if canvas.a > 0 {
                pm.fill_rect(tiny_skia::Rect::from_xywh(0.0, 0.0, pm.width() as f32, pm.height() as f32).unwrap(), &solid(canvas, 1.0), Transform::identity(), None);
            }
        }
        match (layer, blit) {
            (Some((img, _, _)), Some((x, y))) => blit_opaque(pm, &img, x, y),
            (Some((img, t, quality)), None) => pm.draw_pixmap(0, 0, Pixmap::as_ref(&img), &PixmapPaint { opacity: 1.0, blend_mode: BlendMode::SourceOver, quality }, t, None),
            _ => {}
        }
    }

    /// Draws one object (with shadow and opacity) onto `pm`.
    pub fn draw_object(&mut self, pm: &mut Pixmap, doc: &Document, page: &Page, obj: &CaptureObject, s: f32) {
        self.draw_object_at(pm, doc, page, obj, s, Transform::identity());
    }

    /// Like [`draw_object`], with an extra device-space transform `off` (the tile offset).
    pub fn draw_object_at(&mut self, pm: &mut Pixmap, doc: &Document, page: &Page, obj: &CaptureObject, s: f32, off: Transform) {
        let opacity = (obj.opacity() / 100.0).clamp(0.0, 1.0) as f32;
        let shadow = obj.shadow();
        let needs_layer = opacity < 1.0 || shadow.is_some() || obj.rotation() != 0.0;
        if !needs_layer {
            self.paint_object(pm, pm_clone_opt(pm, obj), doc, page, obj, s, off);
            return;
        }
        let Some(mut layer) = Pixmap::new(pm.width(), pm.height()) else { return };
        let bounds = obj.bounds();
        let c = bounds.center();
        let t = if obj.rotation() != 0.0 { Transform::from_rotate_at(obj.rotation() as f32, c.x as f32 * s, c.y as f32 * s).post_concat(off) } else { off };
        // Only Blur and Magnify read what is underneath them.
        self.paint_object(&mut layer, pm_clone_opt(pm, obj), doc, page, obj, s, t);
        // Shadow and compositing work on the painted part of the layer only: blurring and blending the
        // whole tile for every shadowed object made each frame of a drag cost several full-canvas blurs.
        let Some(area) = painted_bounds(&layer) else { return };
        if let Some(sh) = shadow {
            let sigma = (sh.blur as f32 * s).max(0.0) * 1.2;
            // Three box passes reach about 3σ; pad past that so the crop does not cut the blur off.
            let pad = (sigma * 3.5).ceil() as i32 + 2;
            let (x0, y0) = ((area.x() - pad).max(0), (area.y() - pad).max(0));
            let x1 = (area.x() + area.width() as i32 + pad).min(layer.width() as i32);
            let y1 = (area.y() + area.height() as i32 + pad).min(layer.height() as i32);
            let cut = tiny_skia::IntRect::from_ltrb(x0, y0, x1, y1).and_then(|r| layer.clone_rect(r));
            if let Some(shadow_pm) = cut.and_then(|c| blur::shadow(&c, skia_color(sh.color), (sh.opacity / 100.0) as f32, sigma)) {
                blend_over(pm, &shadow_pm, x0 + (sh.dx as f32 * s).round() as i32, y0 + (sh.dy as f32 * s).round() as i32, opacity);
            }
        }
        blend_over_rect(pm, &layer, area, opacity);
    }

    /// Paints the raw object geometry (no shadow/opacity) with transform `t` (device space).
    fn paint_object(&mut self, pm: &mut Pixmap, backdrop: Option<Pixmap>, doc: &Document, page: &Page, obj: &CaptureObject, s: f32, t: Transform) {
        let ts = Transform::from_scale(s, s).post_concat(t);
        let b = obj.bounds();
        match obj.tool_mode.as_str() {
            "Arrow" | "Line" => {
                let a = obj.p0();
                let bp = obj.p1();
                let ctrl = if obj.bezier_curve.unwrap_or(false) { obj.control_points.first().copied() } else { None };
                let w = obj.stroke_width().max(1.0);
                let start_kind = obj.arrow_start.unwrap_or(ArrowLineEndType::Flat);
                let end_kind = obj.arrow_end.unwrap_or(if obj.tool_mode == "Arrow" { ArrowLineEndType::RoundedArrow } else { ArrowLineEndType::Flat });
                let d_end = end_direction(a, bp, ctrl, true);
                let d_start = end_direction(a, bp, ctrl, false);
                let (head_end, shorten_end) = end_cap(end_kind, bp, d_end, w, obj.arrow_end_width.unwrap_or(3.0));
                let (head_start, shorten_start) = end_cap(start_kind, a, d_start, w, obj.arrow_start_width.unwrap_or(3.0));
                let a2 = a + d_start * (-shorten_start);
                let b2 = bp + d_end * (-shorten_end);
                let paint = solid(obj.fg(), 1.0);
                if let Some(path) = line_path(a2, b2, ctrl) {
                    let mut stroke = Stroke { width: w as f32, line_cap: SkCap::Butt, line_join: LineJoin::Round, ..Default::default() };
                    if start_kind == ArrowLineEndType::Round && end_kind == ArrowLineEndType::Round { stroke.line_cap = SkCap::Round; }
                    if let Some(d) = dash_pattern(obj.dash(), w as f32) { stroke.dash = StrokeDash::new(d, 0.0); }
                    pm.stroke_path(&path, &paint, &stroke, ts, None);
                }
                for head in [head_start, head_end].into_iter().flatten() {
                    pm.fill_path(&head, &paint, FillRule::Winding, ts, None);
                }
            }
            "Shape" => {
                let shape = obj.tool_shape.unwrap_or(ShapeShape::Rectangle);
                let pts = if shape == ShapeShape::Polygon { obj.points_array.clone() } else { vec![] };
                let Some(path) = shape_path(shape, b, obj.corner_radius_ratio.unwrap_or(0.35), &pts) else { return };
                fill_and_stroke(pm, &path, obj, ts);
            }
            "Highlight" => {
                let Some(path) = rect_path(b) else { return };
                let mut paint = solid(obj.fg(), 1.0);
                paint.blend_mode = BlendMode::Multiply;
                pm.fill_path(&path, &paint, FillRule::Winding, ts, None);
            }
            "Pen" => {
                let pts: Vec<Pt> = if obj.points_array.len() >= 2 { obj.points_array.clone() } else { vec![obj.p0(), obj.p1()] };
                let Some(path) = polygon_path(&pts, false) else { return };
                let round = matches!(obj.pen_shape, Some(PenShape::PenRound) | None);
                let stroke = Stroke { width: obj.stroke_width().max(1.0) as f32, line_cap: if round { SkCap::Round } else { SkCap::Square }, line_join: if round { LineJoin::Round } else { LineJoin::Miter }, ..Default::default() };
                pm.stroke_path(&path, &solid(obj.fg(), 1.0), &stroke, ts, None);
            }
            "Step" => {
                let style = obj.step_style.unwrap_or(StepStyle::Circle);
                let Some(path) = step_path(style, b) else { return };
                let fill = solid(obj.bg(), 1.0);
                pm.fill_path(&path, &fill, FillRule::Winding, ts, None);
                if obj.stroke_width() > 0.0 && !obj.fg().is_transparent() && obj.fg() != obj.bg() {
                    // Some themes outline the badge with the foreground colour at thin widths only.
                }
                let label = obj.step_value.clone().unwrap_or_else(|| obj.plain_text());
                let label = if label.is_empty() { "1".to_string() } else { label };
                let mut style_t = obj.text_style();
                style_t.color = obj.fg();
                style_t.align = HAlign::Center;
                style_t.bold = true;
                if obj.font_size.is_none() { style_t.font_size = (b.height() * 0.5).max(8.0); }
                let rt = RichText::from_plain(&label, &style_t);
                let r = dev_rect(b, s);
                self.text.draw_in_rect(pm, &rt, r, VAlign::Center, s, 1.0);
            }
            "Text" => {
                let rt = obj.rich_text();
                let pad = obj.tool_padding.unwrap_or(0.0) as f32 * s;
                let r = dev_rect(b, s);
                let inner = tiny_skia::Rect::from_ltrb(r.left() + pad, r.top() + pad, (r.right() - pad).max(r.left() + pad + 1.0), (r.bottom() - pad).max(r.top() + pad + 1.0)).unwrap_or(r);
                if !obj.bg().is_transparent() { if let Some(p) = rect_path(b) { pm.fill_path(&p, &solid(obj.bg(), 1.0), FillRule::Winding, ts, None); } }
                self.text.draw_in_rect(pm, &rt, inner, obj.v_align(), s, 1.0);
            }
            "Callout" => {
                let shape = obj.callout_shape.unwrap_or(CalloutShape::CTRoundedRectWithArrow);
                let tails: Vec<Pt> = if obj.tail_style == Some(CalloutTailStyle::None) { vec![] } else { obj.callout_tails.clone() };
                let body = Rect::from_points(obj.p0(), obj.p1());
                let Some(path) = callout_path(shape, body, &tails, obj.tail_width.unwrap_or(10.0)) else { return };
                let mut o2 = obj.clone();
                // Callouts: BackgroundColor fills, ForegroundColor outlines.
                o2.background_color = Some(obj.bg());
                fill_and_stroke(pm, &path, &o2, ts);
                let rt = obj.rich_text();
                let pad = (obj.tool_padding.unwrap_or(0.0) as f32 + 6.0) * s;
                let r = dev_rect(body, s);
                let inner = tiny_skia::Rect::from_ltrb(r.left() + pad, r.top() + pad, (r.right() - pad).max(r.left() + pad + 1.0), (r.bottom() - pad).max(r.top() + pad + 1.0)).unwrap_or(r);
                self.text.draw_in_rect(pm, &rt, inner, obj.v_align(), s, 1.0);
            }
            "Blur" => {
                let Some(src) = backdrop.as_ref() else { return };
                let r = shift_rect(dev_rect(b, s), t);
                let Some(mut region) = crop_pixmap(src, r) else { return };
                let intensity = obj.blur_intensity.unwrap_or(50.0).clamp(0.0, 100.0);
                match obj.blur_type.unwrap_or(BlurType::Gaussian) {
                    BlurType::Gaussian => blur::gaussian_blur(&mut region, (1.0 + intensity as f32 / 100.0 * 24.0) * s),
                    BlurType::Pixellate => blur::pixelate(&mut region, ((4.0 + intensity / 100.0 * 28.0) * s as f64) as u32),
                }
                pm.draw_pixmap(r.left().round() as i32, r.top().round() as i32, region.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
            }
            "Magnify" => {
                let Some(src) = backdrop.as_ref() else { return };
                let scale = (obj.magnify_scale.unwrap_or(200.0) / 100.0).max(1.0);
                let target = b;
                // Source: the target centre scaled down (or an explicit third/fourth point when offset).
                let source = if obj.magnify_is_offset.unwrap_or(false) && obj.points_array.len() >= 4 {
                    Rect::from_points(obj.points_array[2], obj.points_array[3])
                } else {
                    let c = target.center();
                    Rect::from_xywh(c.x - target.width() / (2.0 * scale), c.y - target.height() / (2.0 * scale), target.width() / scale, target.height() / scale)
                };
                let Some(region) = crop_pixmap(src, shift_rect(dev_rect(source, s), t)) else { return };
                let shape = obj.tool_shape.unwrap_or(ShapeShape::Ellipse);
                let Some(clip_path) = shape_path(shape, target, 0.3, &[]) else { return };
                let mut mask = tiny_skia::Mask::new(pm.width(), pm.height()).unwrap();
                mask.fill_path(&clip_path, FillRule::Winding, true, ts);
                let tr = dev_rect(target, s);
                let t = Transform::from_scale(tr.width() / region.width() as f32, tr.height() / region.height() as f32).post_translate(tr.left(), tr.top()).post_concat(t);
                pm.draw_pixmap(0, 0, region.as_ref(), &PixmapPaint { quality: tiny_skia::FilterQuality::Bicubic, ..Default::default() }, t, Some(&mask));
                if obj.stroke_width() > 0.0 {
                    let stroke = Stroke { width: obj.stroke_width() as f32, ..Default::default() };
                    pm.stroke_path(&clip_path, &solid(obj.fg(), 1.0), &stroke, ts, None);
                }
            }
            "Image" | "Stamp" | "Cursor" => {
                let Some(img) = self.image_object(obj) else { return };
                let tr = dev_rect(b, s);
                let t = Transform::from_scale(tr.width() / img.width() as f32, tr.height() / img.height() as f32).post_translate(tr.left(), tr.top()).post_concat(t);
                pm.draw_pixmap(0, 0, Pixmap::as_ref(&img), &PixmapPaint { quality: tiny_skia::FilterQuality::Bilinear, ..Default::default() }, t, None);
            }
            "Group" => {
                for child in &obj.objects { self.draw_object_at(pm, doc, page, child, s, t); }
            }
            "RepeatableSection" => {
                for child in &obj.section_graphics { self.draw_object(pm, doc, page, child, s); }
            }
            "DropZone" => {
                // Empty drop zones show as a dashed light rectangle (template placeholder).
                if let Some(path) = rect_path(b) {
                    let stroke = Stroke { width: 1.0, dash: StrokeDash::new(vec![6.0, 4.0], 0.0), ..Default::default() };
                    pm.stroke_path(&path, &solid(linkshot_core::Argb::rgb(0x99, 0xA6, 0xB2), 1.0), &stroke, ts, None);
                }
            }
            _ => {}
        }
    }
}

/// SourceOver of `src` onto `dst` at whole-pixel offset (`x`, `y`), scaled by `opacity`. Same float
/// arithmetic as the highp pipeline tiny-skia uses for `draw_pixmap`, so results match it exactly,
/// without the per-pixel pipeline overhead.
fn blend_over(dst: &mut Pixmap, src: &Pixmap, x: i32, y: i32, opacity: f32) {
    if let Some(r) = tiny_skia::IntRect::from_xywh(0, 0, src.width(), src.height()) { blend_rows(dst, src, r, x, y, opacity); }
}

/// [`blend_over`] of the `area` part of `src`, which lies at the same coordinates as `dst`.
fn blend_over_rect(dst: &mut Pixmap, src: &Pixmap, area: tiny_skia::IntRect, opacity: f32) {
    blend_rows(dst, src, area, area.x(), area.y(), opacity);
}

fn blend_rows(dst: &mut Pixmap, src: &Pixmap, area: tiny_skia::IntRect, x: i32, y: i32, opacity: f32) {
    const F: f32 = 1.0 / 255.0;
    // load_8888 → scale_1_float → source_over (d·(1−sa)+s, no FMA) → store_8888 (clamp, round half to even).
    let store = |v: f32| (v.max(0.0).min(1.0) * 255.0).round_ties_even() as u8;
    let (dw, dh, sw) = (dst.width() as i32, dst.height() as i32, src.width() as i32);
    let (x0, x1) = (x.max(0), (x + area.width() as i32).min(dw));
    let (y0, y1) = (y.max(0), (y + area.height() as i32).min(dh));
    if x0 >= x1 || y0 >= y1 { return; }
    let (s, d) = (src.data(), dst.data_mut());
    for yy in y0..y1 {
        let srow = ((yy - y + area.y()) * sw + (x0 - x + area.x())) as usize * 4;
        let drow = (yy * dw + x0) as usize * 4;
        let n = (x1 - x0) as usize * 4;
        for (sp, dp) in s[srow..srow + n].chunks_exact(4).zip(d[drow..drow + n].chunks_exact_mut(4)) {
            if sp[3] == 0 { continue; }
            let sa = sp[3] as f32 * F * opacity;
            let inv = 1.0 - sa;
            for k in 0..4 { dp[k] = store(dp[k] as f32 * F * inv + sp[k] as f32 * F * opacity); }
        }
    }
}

/// Copies the opaque `src` onto `dst` with its top-left at (`x`, `y`): SourceOver of an opaque pixel
/// is the pixel itself, so this matches `draw_pixmap` at a whole-pixel offset.
fn blit_opaque(dst: &mut Pixmap, src: &Pixmap, x: i32, y: i32) {
    let (dw, dh, sw, sh) = (dst.width() as i32, dst.height() as i32, src.width() as i32, src.height() as i32);
    let (x0, x1, y0, y1) = (x.max(0), (x + sw).min(dw), y.max(0), (y + sh).min(dh));
    if x0 >= x1 || y0 >= y1 { return; }
    let n = ((x1 - x0) * 4) as usize;
    let (s, d) = (src.data(), dst.data_mut());
    for yy in y0..y1 {
        let so = (((yy - y) * sw + (x0 - x)) * 4) as usize;
        let dof = ((yy * dw + x0) * 4) as usize;
        d[dof..dof + n].copy_from_slice(&s[so..so + n]);
    }
}

/// The smallest rectangle holding every pixel of `pm` that is not fully transparent.
fn painted_bounds(pm: &Pixmap) -> Option<tiny_skia::IntRect> {
    let w = pm.width() as usize;
    let px = pm.pixels();
    let row_has = |y: usize| px[y * w..(y + 1) * w].iter().any(|p| p.alpha() != 0);
    let top = (0..pm.height() as usize).find(|&y| row_has(y))?;
    let bottom = (top..pm.height() as usize).rev().find(|&y| row_has(y))?;
    let (mut left, mut right) = (w, 0);
    for y in top..=bottom {
        let row = &px[y * w..(y + 1) * w];
        if let Some(l) = row[..left].iter().position(|p| p.alpha() != 0) { left = l; }
        if let Some(r) = row[right..].iter().rposition(|p| p.alpha() != 0) { right += r; }
    }
    tiny_skia::IntRect::from_ltrb(left as i32, top as i32, right as i32 + 1, bottom as i32 + 1)
}

fn pm_clone_opt(pm: &Pixmap, obj: &CaptureObject) -> Option<Pixmap> {
    if matches!(obj.tool_mode.as_str(), "Blur" | "Magnify") { Some(pm.clone()) } else { None }
}

fn fill_and_stroke(pm: &mut Pixmap, path: &tiny_skia::Path, obj: &CaptureObject, ts: Transform) {
    let bg = obj.bg();
    if !bg.is_transparent() {
        pm.fill_path(path, &solid(bg, 1.0), FillRule::Winding, ts, None);
    }
    let w = obj.stroke_width();
    let fg = obj.fg();
    if w > 0.0 && !fg.is_transparent() {
        let mut stroke = Stroke { width: w as f32, line_join: LineJoin::Round, ..Default::default() };
        if let Some(d) = dash_pattern(obj.dash(), w as f32) { stroke.dash = StrokeDash::new(d, 0.0); }
        pm.stroke_path(path, &solid(fg, 1.0), &stroke, ts, None);
    }
}

pub fn solid(c: linkshot_core::Argb, opacity: f64) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(skia_color_alpha(c, opacity));
    p.anti_alias = true;
    p
}

pub fn dev_rect(r: Rect, s: f32) -> tiny_skia::Rect {
    tiny_skia::Rect::from_xywh(r.min.x as f32 * s, r.min.y as f32 * s, (r.width() as f32 * s).max(1.0), (r.height() as f32 * s).max(1.0)).unwrap()
}

/// Moves a device rect by the translation part of `t` (tile offset).
fn shift_rect(r: tiny_skia::Rect, t: Transform) -> tiny_skia::Rect {
    tiny_skia::Rect::from_xywh(r.left() + t.tx, r.top() + t.ty, r.width(), r.height()).unwrap_or(r)
}

fn crop_pixmap(src: &Pixmap, r: tiny_skia::Rect) -> Option<Pixmap> {
    let x0 = r.left().floor().max(0.0) as i32;
    let y0 = r.top().floor().max(0.0) as i32;
    let x1 = (r.right().ceil() as i32).min(src.width() as i32);
    let y1 = (r.bottom().ceil() as i32).min(src.height() as i32);
    if x1 <= x0 || y1 <= y0 { return None; }
    let mut out = Pixmap::new((x1 - x0) as u32, (y1 - y0) as u32)?;
    out.draw_pixmap(-x0, -y0, src.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
    Some(out)
}

fn draw_checkerboard(pm: &mut Pixmap, cell: u32) {
    let (w, h) = (pm.width(), pm.height());
    let light = [0xFF, 0xFF, 0xFF, 0xFF];
    let dark = [0xCC, 0xCC, 0xCC, 0xFF];
    let data = pm.data_mut();
    for y in 0..h {
        for x in 0..w {
            let c = if ((x / cell) + (y / cell)) % 2 == 0 { light } else { dark };
            let o = ((y * w + x) * 4) as usize;
            data[o..o + 4].copy_from_slice(&c);
        }
    }
}

/// Rounded-corner mask for the whole canvas (`CornerRadiusRatio`, 0..1 of half the shorter side).
fn apply_corner_mask(pm: &mut Pixmap, ratio: f32) {
    let w = pm.width() as f32;
    let h = pm.height() as f32;
    let radius = (w.min(h) / 2.0 * ratio.clamp(0.0, 1.0)) as f64;
    let Some(path) = rounded_rect_path(Rect::from_xywh(0.0, 0.0, w as f64, h as f64), radius) else { return };
    let mut mask = tiny_skia::Mask::new(pm.width(), pm.height()).unwrap();
    mask.fill_path(&path, FillRule::Winding, true, Transform::identity());
    let mask_data = mask.data();
    for (i, px) in pm.data_mut().chunks_exact_mut(4).enumerate() {
        let m = mask_data[i] as u32;
        if m == 255 { continue; }
        for c in px.iter_mut() { *c = ((*c as u32 * m + 127) / 255) as u8; }
    }
}

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    // Tiny standalone base64 decoder (avoids pulling the crate into render).
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0;
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' | b'\n' | b'\r' | b' ' => continue,
            _ => return None,
        } as u32;
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 { bits -= 8; out.push((buf >> bits) as u8); buf &= (1 << bits) - 1; }
    }
    Some(out)
}
