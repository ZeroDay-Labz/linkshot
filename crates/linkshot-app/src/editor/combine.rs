//! Combine Images / Image from Template: puts several captures into one new image, laid out side by
//! side, stacked, in a grid, as a before/after pair or as numbered steps. The captures become image
//! objects and the labels and step numbers ordinary annotations, so everything stays editable.

use crate::app::App;
use image::RgbaImage;
use linkshot_core::document::CaptureObject;
use linkshot_core::enums::ToolMode;
use linkshot_core::{Document, Pt};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout { SideBySide, Stacked, Grid, BeforeAfter, Steps }

impl Layout {
    pub const ALL: [Layout; 5] = [Layout::SideBySide, Layout::Stacked, Layout::Grid, Layout::BeforeAfter, Layout::Steps];
    pub fn label(self) -> &'static str {
        match self { Layout::SideBySide => "Side by side", Layout::Stacked => "Stacked", Layout::Grid => "Grid", Layout::BeforeAfter => "Before / after", Layout::Steps => "Numbered steps" }
    }
    pub fn hint(self) -> &'static str {
        match self {
            Layout::SideBySide => "One row, all the same height.",
            Layout::Stacked => "One column, all the same width.",
            Layout::Grid => "Rows and columns of equal cells.",
            Layout::BeforeAfter => "Two images next to each other, labelled Before and After.",
            Layout::Steps => "One column with a numbered badge on each image.",
        }
    }
}

/// Where each image goes (x, y, w, h) and the canvas size, for images of the given sizes. Images are
/// only scaled down (to the smallest height, width or cell), never up.
pub fn arrange(sizes: &[(u32, u32)], layout: Layout, gap: f64, label_h: f64) -> (f64, f64, Vec<(f64, f64, f64, f64)>) {
    let n = sizes.len();
    if n == 0 { return (1.0, 1.0, vec![]); }
    let sz: Vec<(f64, f64)> = sizes.iter().map(|&(w, h)| (w.max(1) as f64, h.max(1) as f64)).collect();
    let mut rects = Vec::with_capacity(n);
    match layout {
        Layout::SideBySide | Layout::BeforeAfter => {
            let h = sz.iter().map(|s| s.1).fold(f64::MAX, f64::min);
            let top = gap + label_h;
            let mut x = gap;
            for &(w0, h0) in &sz { let w = w0 * h / h0; rects.push((x, top, w, h)); x += w + gap; }
            (x, top + h + gap, rects)
        }
        Layout::Stacked | Layout::Steps => {
            let w = sz.iter().map(|s| s.0).fold(f64::MAX, f64::min);
            let mut y = gap;
            for &(w0, h0) in &sz { let h = h0 * w / w0; rects.push((gap, y, w, h)); y += h + gap; }
            (w + 2.0 * gap, y, rects)
        }
        Layout::Grid => {
            let cols = (n as f64).sqrt().ceil() as usize;
            let rows = n.div_ceil(cols);
            // Cell: the smallest width, and the tallest height that width gives.
            let cw = sz.iter().map(|s| s.0).fold(f64::MAX, f64::min);
            let ch = sz.iter().map(|&(w, h)| h * cw / w).fold(0.0, f64::max);
            for (i, &(w0, h0)) in sz.iter().enumerate() {
                let (c, r) = (i % cols, i / cols);
                let h = h0 * cw / w0;
                let (x, y) = (gap + c as f64 * (cw + gap), gap + r as f64 * (ch + gap) + (ch - h) / 2.0);
                rects.push((x, y, cw, h));
            }
            (gap + cols as f64 * (cw + gap), gap + rows as f64 * (ch + gap), rects)
        }
    }
}

pub struct CombineDialog {
    pub open: bool,
    /// Tray documents chosen, by object identity (index at the time the dialog opened).
    pub picked: Vec<bool>,
    pub layout: Layout,
    pub gap: f64,
    pub background: egui::Color32,
}

impl Default for CombineDialog {
    fn default() -> Self { Self { open: false, picked: vec![], layout: Layout::SideBySide, gap: 24.0, background: egui::Color32::WHITE } }
}

impl CombineDialog {
    pub fn show_with(&mut self, layout: Layout, docs: usize, current: Option<usize>) {
        self.open = true;
        self.layout = layout;
        // Start with the current capture and the one before it.
        self.picked = (0..docs).map(|i| Some(i) == current || current.map(|c| i + 1 == c).unwrap_or(false)).collect();
        if self.picked.iter().filter(|p| **p).count() < 2 { for p in self.picked.iter_mut().rev().take(2) { *p = true; } }
    }
}

/// The dialog (Editor ▸ Image ▸ Combine Images…, Create ▸ Image from Template…).
pub fn dialog(app: &mut App, ctx: &egui::Context) {
    if !app.editor.combine.open { return; }
    let pal = app.pal;
    let n = app.editor.docs.len();
    app.editor.combine.picked.resize(n, false);
    let mut open = true;
    let mut create = false;
    egui::Window::new("Combine Images").open(&mut open).collapsible(false).resizable(false).default_width(420.0).show(ctx, |ui| {
        let d = &mut app.editor.combine;
        ui.label(egui::RichText::new("Layout").strong());
        for l in Layout::ALL { ui.radio_value(&mut d.layout, l, l.label()).on_hover_text(l.hint()); }
        ui.label(egui::RichText::new(d.layout.hint()).small().color(pal.input_secondary));
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label("Spacing");
            ui.add(egui::Slider::new(&mut d.gap, 0.0..=96.0).suffix(" px"));
            ui.label("Background");
            ui.color_edit_button_srgba(&mut d.background);
        });
        ui.add_space(6.0);
        ui.label(egui::RichText::new("Images (from the tray)").strong());
        egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
            for (i, doc) in app.editor.docs.iter().enumerate() {
                if doc.is_video() { continue; }
                let p = doc.doc.page();
                ui.checkbox(&mut d.picked[i], format!("{}  ({} × {})", doc.doc.display_name(), p.capture_canvas_width as u32, p.capture_canvas_height as u32));
            }
        });
        let count = d.picked.iter().enumerate().filter(|(i, p)| **p && !app.editor.docs[*i].is_video()).count();
        let need = if d.layout == Layout::BeforeAfter { 2 } else { 1 };
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let ok = count >= need && !(d.layout == Layout::BeforeAfter && count != 2);
            if ui.add_enabled(ok, egui::Button::new(egui::RichText::new("Create").color(egui::Color32::WHITE)).fill(pal.product)).clicked() { create = true; }
            if d.layout == Layout::BeforeAfter && count != 2 { ui.label(egui::RichText::new("Pick exactly two images.").small()); }
            else if count < need { ui.label(egui::RichText::new("Pick at least one image.").small()); }
        });
    });
    if create { build(app, ctx); app.editor.combine.open = false; }
    if !open { app.editor.combine.open = false; }
}

/// Builds with the dialog's current choices (headless screenshots).
pub fn build_for_test(app: &mut App, ctx: &egui::Context) { build(app, ctx); app.editor.combine.open = false; }

fn build(app: &mut App, ctx: &egui::Context) {
    let d = &app.editor.combine;
    let (layout, gap, bg) = (d.layout, d.gap, d.background);
    let picked: Vec<usize> = d.picked.iter().enumerate().filter(|(i, p)| **p && *i < app.editor.docs.len() && !app.editor.docs[*i].is_video()).map(|(i, _)| i).collect();
    let mut images: Vec<RgbaImage> = Vec::new();
    for i in &picked {
        let doc = app.editor.docs[*i].doc.clone();
        if let Some(img) = app.editor.renderer.flatten(&doc, doc.page()) { images.push(img); }
    }
    if images.is_empty() { return; }
    let label_h = if layout == Layout::BeforeAfter { 56.0 } else { 0.0 };
    let sizes: Vec<(u32, u32)> = images.iter().map(|i| i.dimensions()).collect();
    let (cw, ch, rects) = arrange(&sizes, layout, gap, label_h);
    let (cw, ch) = (cw.ceil().max(1.0) as u32, ch.ceil().max(1.0) as u32);
    let canvas = RgbaImage::from_pixel(cw, ch, image::Rgba([bg.r(), bg.g(), bg.b(), bg.a()]));
    let Ok(png) = linkshot_render::encode_png(&canvas) else { return };
    let mut doc = Document::from_png(cw, ch, png);
    use base64::Engine as _;
    for (img, &(x, y, w, h)) in images.iter().zip(&rects) {
        let Ok(png) = linkshot_render::encode_png(img) else { continue };
        let mut o = CaptureObject::new("Image");
        o.points_array = vec![Pt::new(x, y), Pt::new(x + w, y + h)];
        o.image_data = Some(base64::engine::general_purpose::STANDARD.encode(&png));
        o.aspect_ratio = Some(w / h.max(1.0));
        doc.page_mut().capture_objects.push(o);
    }
    // Labels and step numbers as normal annotations in the current quick styles.
    let style = |t: ToolMode| app.editor.props.get(&t).cloned().unwrap_or_else(|| linkshot_core::style::QuickStyle::for_tool(t));
    if layout == Layout::BeforeAfter {
        let mut q = style(ToolMode::Text);
        q.font_size = 30.0;
        q.text_selection_bold = true;
        // Dark text on light backgrounds, light text on dark ones.
        let light = (bg.r() as u32 + bg.g() as u32 + bg.b() as u32) > 380;
        q.text_selection_color = if light { linkshot_core::Argb::new(255, 30, 30, 30) } else { linkshot_core::Argb::new(255, 240, 240, 240) };
        q.background_color = linkshot_core::Argb::TRANSPARENT;
        q.text_outline_width = 0.0;
        for (label, &(x, _, w, _)) in ["Before", "After"].iter().zip(&rects) {
            if let Some(mut o) = super::tools::object_from_style(ToolMode::Text, &q, Pt::new(x, gap), Pt::new(x + w, gap + label_h - 8.0)) {
                o.tool_horizontal_align = Some("Center".into());
                o.set_plain_text(label);
                doc.page_mut().capture_objects.push(o);
            }
        }
    }
    if layout == Layout::Steps {
        let q = style(ToolMode::Step);
        for (k, &(x, y, w, _)) in rects.iter().enumerate() {
            // Badge size follows the image, so it reads at the zoom the image is viewed at.
            let r = (w * 0.035).clamp(20.0, 64.0);
            if let Some(mut o) = super::tools::object_from_style(ToolMode::Step, &q, Pt::new(x - r * 0.4, y - r * 0.4), Pt::new(x + r * 1.6, y + r * 1.6)) {
                o.font_size = Some(r * 1.1);
                o.set_plain_text(&(k + 1).to_string());
                doc.page_mut().capture_objects.push(o);
            }
        }
    }
    let (paths, settings) = (app.paths.clone(), app.settings.clone());
    app.editor.add_document(ctx, doc, &paths, &settings);
    app.editor.set_status(format!("Combined {} images ({}). Each image, label and number can be moved or changed.", images.len(), layout.label().to_lowercase()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn side_by_side_scales_to_the_smallest_height() {
        let (w, h, r) = arrange(&[(400, 300), (200, 100)], Layout::SideBySide, 10.0, 0.0);
        assert_eq!(r[0], (10.0, 10.0, 400.0 * 100.0 / 300.0, 100.0));
        assert_eq!(r[1], (10.0 + 400.0 / 3.0 + 10.0, 10.0, 200.0, 100.0));
        assert!((w - (10.0 + 400.0 / 3.0 + 10.0 + 200.0 + 10.0)).abs() < 1e-9);
        assert_eq!(h, 120.0);
    }

    #[test]
    fn stacked_and_grid() {
        let (w, h, r) = arrange(&[(300, 100), (600, 400)], Layout::Stacked, 0.0, 0.0);
        assert_eq!((w, h), (300.0, 100.0 + 200.0));
        assert_eq!(r[1], (0.0, 100.0, 300.0, 200.0));
        let (w, h, r) = arrange(&[(100, 100); 5], Layout::Grid, 10.0, 0.0);
        assert_eq!(r.len(), 5);
        // 5 images: 3 columns, 2 rows.
        assert_eq!((w, h), (10.0 + 3.0 * 110.0, 10.0 + 2.0 * 110.0));
        assert_eq!(r[4], (10.0 + 110.0, 120.0, 100.0, 100.0));
    }
}
