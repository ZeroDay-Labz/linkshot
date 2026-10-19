//! Text layout and rasterisation with cosmic-text.

use cosmic_text::{Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, Style, SwashCache, SwashContent, Weight};
use linkshot_core::enums::HAlign;
use linkshot_core::rtf::RichText;
use tiny_skia::Pixmap;

pub struct TextEngine {
    pub fonts: FontSystem,
    pub swash: SwashCache,
}

impl Default for TextEngine {
    fn default() -> Self { Self::new() }
}

impl TextEngine {
    pub fn new() -> Self {
        Self { fonts: FontSystem::new(), swash: SwashCache::new() }
    }

    /// Lays out rich text inside a box of `width` logical pixels (None = unbounded) at `scale`.
    /// Returns the buffer plus the measured (width, height) in device pixels.
    pub fn layout(&mut self, rt: &RichText, width: Option<f32>, scale: f32) -> (Buffer, f32, f32) {
        let size = rt.runs.first().map(|r| r.style.font_size as f32).unwrap_or(14.0) * scale;
        let metrics = Metrics::new(size, size * 1.2);
        let mut buffer = Buffer::new(&mut self.fonts, metrics);
        buffer.set_size(&mut self.fonts, width.map(|w| w * scale), None);
        let mut spans: Vec<(&str, Attrs)> = Vec::new();
        for run in &rt.runs {
            let fam = Family::Name(&run.style.font_family);
            let c = run.style.color;
            let mut attrs = Attrs::new().family(fam).color(Color::rgba(c.r, c.g, c.b, c.a));
            attrs = attrs.weight(if run.style.bold { Weight::BOLD } else { Weight::NORMAL });
            attrs = attrs.style(if run.style.italic { Style::Italic } else { Style::Normal });
            attrs = attrs.metrics(Metrics::new(run.style.font_size as f32 * scale, run.style.font_size as f32 * 1.2 * scale));
            spans.push((run.text.as_str(), attrs));
        }
        buffer.set_rich_text(&mut self.fonts, spans.iter().map(|(t, a)| (*t, a.clone())), &Attrs::new(), Shaping::Advanced, None);
        let align = match rt.align { HAlign::Left => cosmic_text::Align::Left, HAlign::Center => cosmic_text::Align::Center, HAlign::Right => cosmic_text::Align::Right };
        for line in buffer.lines.iter_mut() { line.set_align(Some(align)); }
        buffer.shape_until_scroll(&mut self.fonts, false);
        let mut w: f32 = 0.0;
        let mut h: f32 = 0.0;
        for run in buffer.layout_runs() {
            w = w.max(run.line_w);
            h = h.max(run.line_top + run.line_height);
        }
        (buffer, w, h)
    }

    /// Draws a laid-out buffer at (x, y) device pixels, optionally overriding the colour.
    pub fn draw(&mut self, pm: &mut Pixmap, buffer: &Buffer, x: f32, y: f32, color_override: Option<Color>, opacity: f32) {
        let w = pm.width() as i32;
        let h = pm.height() as i32;
        let data = pm.data_mut();
        for run in buffer.layout_runs() {
            for glyph in run.glyphs.iter() {
                let physical = glyph.physical((x, y), 1.0);
                let color = color_override.unwrap_or(glyph.color_opt.unwrap_or(Color::rgb(0, 0, 0)));
                let Some(img) = self.swash.get_image(&mut self.fonts, physical.cache_key) else { continue };
                let gx = physical.x + img.placement.left;
                let gy = physical.y + run.line_y as i32 - img.placement.top;
                let gw = img.placement.width as i32;
                let gh = img.placement.height as i32;
                match img.content {
                    SwashContent::Mask => {
                        for row in 0..gh {
                            for col in 0..gw {
                                let px = gx + col;
                                let py = gy + row;
                                if px < 0 || py < 0 || px >= w || py >= h { continue; }
                                let cov = img.data[(row * gw + col) as usize] as f32 / 255.0;
                                let a = cov * (color.a() as f32 / 255.0) * opacity;
                                if a <= 0.0 { continue; }
                                blend(data, (py * w + px) as usize * 4, color, a);
                            }
                        }
                    }
                    SwashContent::Color => {
                        for row in 0..gh {
                            for col in 0..gw {
                                let px = gx + col;
                                let py = gy + row;
                                if px < 0 || py < 0 || px >= w || py >= h { continue; }
                                let i = ((row * gw + col) * 4) as usize;
                                let c = Color::rgba(img.data[i], img.data[i + 1], img.data[i + 2], img.data[i + 3]);
                                let a = c.a() as f32 / 255.0 * opacity;
                                blend(data, (py * w + px) as usize * 4, c, a);
                            }
                        }
                    }
                    SwashContent::SubpixelMask => {}
                }
            }
        }
    }

    /// Convenience: layout + draw a rich text centred/aligned inside `rect` (device px).
    pub fn draw_in_rect(&mut self, pm: &mut Pixmap, rt: &RichText, rect: tiny_skia::Rect, valign: linkshot_core::enums::VAlign, scale: f32, opacity: f32) {
        let (buffer, _w, h) = self.layout(rt, Some(rect.width() / scale), scale);
        let y = match valign {
            linkshot_core::enums::VAlign::Top => rect.top(),
            linkshot_core::enums::VAlign::Center => rect.top() + (rect.height() - h) / 2.0,
            linkshot_core::enums::VAlign::Bottom => rect.bottom() - h,
        };
        self.draw(pm, &buffer, rect.left(), y, None, opacity);
    }
}

#[inline]
fn blend(data: &mut [u8], o: usize, c: Color, a: f32) {
    // Source-over onto premultiplied destination.
    let sr = c.r() as f32 * a;
    let sg = c.g() as f32 * a;
    let sb = c.b() as f32 * a;
    let sa = 255.0 * a;
    let inv = 1.0 - a;
    data[o] = (sr + data[o] as f32 * inv).round().min(255.0) as u8;
    data[o + 1] = (sg + data[o + 1] as f32 * inv).round().min(255.0) as u8;
    data[o + 2] = (sb + data[o + 2] as f32 * inv).round().min(255.0) as u8;
    data[o + 3] = (sa + data[o + 3] as f32 * inv).round().min(255.0) as u8;
}
