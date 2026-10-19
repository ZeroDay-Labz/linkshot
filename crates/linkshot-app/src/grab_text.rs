//! "Grab Text Results" window (600×420): shows the recognised text with Copy All.
//! Recognition runs on a worker thread; the models are downloaded on first use.

use crate::theme::Palette;
use crossbeam_channel::Receiver;
use egui::{Color32, Stroke};
use std::path::PathBuf;

pub enum Msg { Status(String), Engine { builtin: bool }, Done(anyhow::Result<linkshot_ocr::OcrResult>) }

pub struct GrabTextUi {
    rx: Receiver<Msg>,
    /// True when the bundled model (not Tesseract) produced the text; the window then suggests
    /// installing Tesseract, which is far more accurate on screenshots.
    builtin: bool,
    pub status: String,
    pub text: String,
    pub lines: usize,
    pub busy: bool,
    pub error: Option<String>,
    pub closed: bool,
    focused: bool,
}

impl GrabTextUi {
    pub fn start(img: image::RgbaImage, model_dir: PathBuf, lang: String, prefer_builtin: bool) -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        std::thread::Builder::new().name("grab-text".into()).spawn(move || {
            let use_tesseract = !prefer_builtin && linkshot_ocr::tesseract::available();
            if !use_tesseract && !linkshot_ocr::models_present(&model_dir) {
                let _ = tx.send(Msg::Status("Downloading text recognition models (one time, ~12 MB)…".into()));
                let tx2 = tx.clone();
                let mut last = 0u64;
                if let Err(e) = linkshot_ocr::download_models(&model_dir, |done, total| {
                    if done - last > 512 * 1024 || Some(done) == total { last = done; let _ = tx2.send(Msg::Status(match total { Some(t) => format!("Downloading text recognition models… {} / {} MB", done / 1_000_000, t / 1_000_000), None => format!("Downloading text recognition models… {} MB", done / 1_000_000) })); }
                }) { let _ = tx.send(Msg::Done(Err(e))); return; }
            }
            let _ = tx.send(Msg::Engine { builtin: !use_tesseract });
            let _ = tx.send(Msg::Status(if use_tesseract { "Recognizing text (Tesseract)…".into() } else { "Recognizing text (built-in model)…".into() }));
            let res = linkshot_ocr::recognize_best(&img, &model_dir, &lang, prefer_builtin);
            let _ = tx.send(Msg::Done(res));
        }).expect("spawn OCR thread");
        Self { rx, builtin: false, status: "Recognizing text…".into(), text: String::new(), lines: 0, busy: true, error: None, closed: false, focused: false }
    }

    /// A finished results window with the given text (headless screenshots / tests).
    pub fn with_text(text: &str) -> Self {
        let (_tx, rx) = crossbeam_channel::unbounded();
        let lines = text.lines().count();
        Self { rx, builtin: false, status: format!("{lines} lines of text"), text: text.to_string(), lines, busy: false, error: None, closed: false, focused: true }
    }

    pub fn viewport_builder() -> egui::ViewportBuilder {
        egui::ViewportBuilder::default()
            .with_title("Grab Text Results")
            .with_app_id(crate::app_id())
            .with_inner_size([600.0, 420.0])
            .with_min_inner_size([360.0, 240.0])
            .with_always_on_top()
    }

    fn poll(&mut self) {
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Msg::Status(s) => self.status = s,
                Msg::Engine { builtin } => self.builtin = builtin,
                Msg::Done(Ok(r)) => { self.lines = r.lines.len(); self.text = r.text(); self.busy = false; self.status = if r.lines.is_empty() { "No text was found in the capture.".into() } else { format!("{} line{} of text", r.lines.len(), if r.lines.len() == 1 { "" } else { "s" }) };  if self.builtin && !self.text.is_empty() { self.status.push_str("  ·  built-in OCR — install Tesseract (dnf install tesseract tesseract-langpack-eng) for accurate text"); } }
                Msg::Done(Err(e)) => { self.busy = false; self.error = Some(format!("{e:#}")); self.status = "Text recognition failed.".into(); }
            }
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, pal: &Palette) {
        self.poll();
        if !self.focused { self.focused = true; ctx.send_viewport_cmd(egui::ViewportCommand::Focus); }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) { self.closed = true; }
        egui::TopBottomPanel::bottom("grab-text-buttons").frame(egui::Frame::NONE.fill(pal.main_panel).inner_margin(10.0)).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(&self.status).color(if self.error.is_some() { pal.capture_red } else { pal.text }));
                if self.busy { ui.add(egui::Spinner::new().size(14.0)); }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(egui::Button::new(egui::RichText::new("Close").color(pal.text_bright)).fill(pal.control).stroke(Stroke::new(1.0_f32, pal.control_outline))).clicked() { self.closed = true; }
                    let copy = ui.add_enabled(!self.text.is_empty(), egui::Button::new(egui::RichText::new("Copy All").color(Color32::WHITE)).fill(pal.product).stroke(Stroke::new(1.0_f32, pal.product_dark)));
                    if copy.clicked() { if let Err(e) = linkshot_platform::clipboard::set_text(&self.text) { self.status = format!("Copy failed: {e}"); } else { self.status = "Copied to the clipboard.".into(); } }
                });
            });
        });
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(pal.main_panel).inner_margin(10.0)).show(ctx, |ui| {
            if let Some(e) = &self.error { ui.colored_label(pal.capture_red, e); ui.add_space(6.0); }
            egui::ScrollArea::vertical().id_salt("grab-text-scroll").auto_shrink([false, false]).show(ui, |ui| {
                ui.add_sized(ui.available_size(), egui::TextEdit::multiline(&mut self.text).font(egui::TextStyle::Monospace).desired_width(f32::INFINITY).hint_text(if self.busy { "" } else { "No text" }));
            });
        });
        if self.busy { ctx.request_repaint_after(std::time::Duration::from_millis(100)); }
    }
}
