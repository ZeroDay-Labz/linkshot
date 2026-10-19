//! Text extraction ("Grab Text") using the pure-Rust `ocrs` engine (rten models).
//!
//! The two models ship in the tool bundle (`ocrs/`); when a build lacks them they are downloaded
//! on first use into the app's data directory.

use anyhow::{Context, Result};
use image::RgbaImage;
use ocrs::{ImageSource, OcrEngine, OcrEngineParams, TextItem};
use std::path::{Path, PathBuf};

const DETECTION_URLS: &[&str] = &[
    "https://ocrs-models.s3-accelerate.amazonaws.com/text-detection.rten",
    "https://ocrs-models.s3-accelerate.amazonaws.com/text-detection.onnx",
];
const RECOGNITION_URLS: &[&str] = &[
    "https://ocrs-models.s3-accelerate.amazonaws.com/text-recognition.rten",
    "https://ocrs-models.s3-accelerate.amazonaws.com/text-recognition.onnx",
];

#[derive(Clone, Debug, PartialEq)]
pub struct OcrBox { pub x: i32, pub y: i32, pub w: i32, pub h: i32 }

#[derive(Clone, Debug, PartialEq)]
pub struct OcrWord { pub text: String, pub rect: OcrBox }

#[derive(Clone, Debug, PartialEq)]
pub struct OcrLine { pub text: String, pub rect: OcrBox, pub words: Vec<OcrWord> }

#[derive(Clone, Debug, Default, PartialEq)]
pub struct OcrResult {
    pub lines: Vec<OcrLine>,
    /// Scale that was applied to the image before recognition (boxes are already divided back).
    pub scale: f32,
}

impl OcrResult {
    /// Lines joined with newlines; blank line between paragraphs that are far apart vertically.
    pub fn text(&self) -> String {
        let mut out = String::new();
        let mut prev: Option<&OcrLine> = None;
        for l in &self.lines {
            if let Some(p) = prev {
                let gap = l.rect.y - (p.rect.y + p.rect.h);
                out.push('\n');
                if gap > p.rect.h.max(8) { out.push('\n'); }
            }
            out.push_str(&l.text);
            prev = Some(l);
        }
        out
    }
    pub fn is_empty(&self) -> bool { self.lines.is_empty() }
}

/// The bundled model directory when the installation ships one.
pub fn bundled_model_dir() -> Option<PathBuf> { linkshot_tools::ocrs_model_dir() }

/// Where the models live: `<dir>/text-detection.*` and `<dir>/text-recognition.*`.
pub fn model_paths(dir: &Path) -> Option<(PathBuf, PathBuf)> {
    let find = |stem: &str| ["rten", "onnx"].iter().map(|e| dir.join(format!("{stem}.{e}"))).find(|p| p.is_file());
    Some((find("text-detection")?, find("text-recognition")?))
}

pub fn models_present(dir: &Path) -> bool { model_paths(dir).is_some() }

/// Downloads both models into `dir` (blocking). `progress(done_bytes, total_bytes)` is called as data arrives.
pub fn download_models(dir: &Path, mut progress: impl FnMut(u64, Option<u64>)) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    let agent = ureq::Agent::config_builder().timeout_global(Some(std::time::Duration::from_secs(600))).build().new_agent();
    for (urls, stem) in [(DETECTION_URLS, "text-detection"), (RECOGNITION_URLS, "text-recognition")] {
        if ["rten", "onnx"].iter().any(|e| dir.join(format!("{stem}.{e}")).is_file()) { continue; }
        let mut last_err = None;
        for url in urls {
            let ext = url.rsplit('.').next().unwrap_or("rten");
            let tmp = dir.join(format!("{stem}.{ext}.part"));
            let res = (|| -> Result<()> {
                let resp = agent.get(*url).call().with_context(|| format!("GET {url}"))?;
                let total = resp.headers().get("content-length").and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u64>().ok());
                let mut reader = resp.into_body().into_reader();
                let mut file = std::fs::File::create(&tmp)?;
                let mut buf = vec![0u8; 64 * 1024];
                let mut done = 0u64;
                loop {
                    let n = std::io::Read::read(&mut reader, &mut buf)?;
                    if n == 0 { break; }
                    std::io::Write::write_all(&mut file, &buf[..n])?;
                    done += n as u64;
                    progress(done, total);
                }
                drop(file);
                std::fs::rename(&tmp, dir.join(format!("{stem}.{ext}")))?;
                Ok(())
            })();
            match res { Ok(()) => { last_err = None; break; } Err(e) => { let _ = std::fs::remove_file(&tmp); last_err = Some(e); } }
        }
        if let Some(e) = last_err { return Err(e); }
    }
    Ok(())
}

/// Flattens transparency onto white and drops alpha.
fn composite_rgb(img: &RgbaImage) -> image::RgbImage {
    let (w, h) = img.dimensions();
    let mut rgb = image::RgbImage::new(w, h);
    for (x, y, p) in img.enumerate_pixels() {
        let a = p[3] as u32;
        let blend = |c: u8| ((c as u32 * a + 255 * (255 - a)) / 255) as u8;
        rgb.put_pixel(x, y, image::Rgb([blend(p[0]), blend(p[1]), blend(p[2])]));
    }
    rgb
}

/// The Tesseract command-line tool shipped in LinkShot's tool bundle (or installed on the system):
/// more accurate than the fallback models on screen text and code, and it supports many languages.
pub mod tesseract {
    use super::*;

    /// The tesseract LinkShot ships with, or one installed on the system.
    pub fn find() -> Option<linkshot_tools::Tool> { linkshot_tools::find("tesseract") }

    pub fn available() -> bool {
        static OK: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *OK.get_or_init(|| find().map(|t| t.command().arg("--version").stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status().map(|s| s.success()).unwrap_or(false)).unwrap_or(false))
    }

    /// Runs `tesseract` with TSV output and groups words into lines. `lang` is e.g. "eng" or "eng+deu".
    pub fn recognize(img: &RgbaImage, lang: &str) -> Result<OcrResult> {
        let (w, h) = img.dimensions();
        anyhow::ensure!(w > 0 && h > 0, "empty image");
        let rgb = composite_rgb(img);
        // Tesseract likes ~30 px text; upscale small captures (it works on 300 dpi scans).
        let scale = if w.max(h) < 1600 { 2.0 } else { 1.0 };
        let rgb = if scale != 1.0 { image::imageops::resize(&rgb, (w as f32 * scale) as u32, (h as f32 * scale) as u32, image::imageops::FilterType::CatmullRom) } else { rgb };
        let dir = std::env::temp_dir().join(format!("linkshot-ocr-{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let png = dir.join("in.png");
        rgb.save(&png)?;
        let tool = find().context("tesseract not found")?;
        let out = tool.command().arg(&png).arg("stdout").args(["-l", lang, "--psm", "6", "tsv"]).output().context("run tesseract")?;
        let _ = std::fs::remove_dir_all(&dir);
        anyhow::ensure!(out.status.success(), "tesseract failed: {}", String::from_utf8_lossy(&out.stderr));
        let tsv = String::from_utf8_lossy(&out.stdout);
        let mut result = OcrResult { lines: vec![], scale };
        let mut current: Option<((i32, i32, i32), OcrLine)> = None; // (block, par, line) key
        for row in tsv.lines().skip(1) {
            let f: Vec<&str> = row.split('\t').collect();
            if f.len() < 12 { continue; }
            let level: i32 = f[0].parse().unwrap_or(0);
            if level != 5 { continue; }
            let key = (f[2].parse().unwrap_or(0), f[3].parse().unwrap_or(0), f[4].parse().unwrap_or(0));
            let (x, y, bw, bh): (i32, i32, i32, i32) = (f[6].parse().unwrap_or(0), f[7].parse().unwrap_or(0), f[8].parse().unwrap_or(0), f[9].parse().unwrap_or(0));
            let text = f[11].trim();
            if text.is_empty() { continue; }
            let rect = OcrBox { x: (x as f32 / scale).round() as i32, y: (y as f32 / scale).round() as i32, w: (bw as f32 / scale).round().max(1.0) as i32, h: (bh as f32 / scale).round().max(1.0) as i32 };
            match &mut current {
                Some((k, line)) if *k == key => {
                    line.text.push(' ');
                    line.text.push_str(text);
                    let right = (line.rect.x + line.rect.w).max(rect.x + rect.w);
                    let bottom = (line.rect.y + line.rect.h).max(rect.y + rect.h);
                    line.rect.x = line.rect.x.min(rect.x); line.rect.y = line.rect.y.min(rect.y);
                    line.rect.w = right - line.rect.x; line.rect.h = bottom - line.rect.y;
                    line.words.push(OcrWord { text: text.to_string(), rect });
                }
                _ => {
                    if let Some((_, l)) = current.take() { result.lines.push(l); }
                    current = Some((key, OcrLine { text: text.to_string(), rect: rect.clone(), words: vec![OcrWord { text: text.to_string(), rect }] }));
                }
            }
        }
        if let Some((_, l)) = current.take() { result.lines.push(l); }
        Ok(result)
    }
}

/// Recognises text with the best available engine: Tesseract when installed (unless `prefer_builtin`),
/// otherwise the bundled ocrs models in `model_dir`.
pub fn recognize_best(img: &RgbaImage, model_dir: &Path, lang: &str, prefer_builtin: bool) -> Result<OcrResult> {
    if !prefer_builtin && tesseract::available() {
        match tesseract::recognize(img, lang) { Ok(r) => return Ok(r), Err(e) => tracing::warn!("tesseract failed, using built-in OCR: {e:#}") }
    }
    Ocr::load(model_dir)?.recognize(img)
}

pub struct Ocr { engine: OcrEngine }

impl Ocr {
    pub fn load(dir: &Path) -> Result<Ocr> {
        let (det, rec) = model_paths(dir).context("OCR models not downloaded")?;
        let detection_model = rten::Model::load_file(&det).with_context(|| format!("load {}", det.display()))?;
        let recognition_model = rten::Model::load_file(&rec).with_context(|| format!("load {}", rec.display()))?;
        let engine = OcrEngine::new(OcrEngineParams { detection_model: Some(detection_model), recognition_model: Some(recognition_model), ..Default::default() })?;
        Ok(Ocr { engine })
    }

    /// Recognises all text in the image. The image is upscaled adaptively so that the detected text
    /// height reaches what the models were trained on (~28 px); boxes come back in original coordinates.
    pub fn recognize(&self, img: &RgbaImage) -> Result<OcrResult> {
        let (w, h) = img.dimensions();
        anyhow::ensure!(w > 0 && h > 0, "empty image");
        let rgb = composite_rgb(img);
        // Pass 1: detect at 1× to estimate the typical text height.
        let scale = match std::env::var("LINKSHOT_OCR_SCALE").ok().and_then(|s| s.parse::<f32>().ok()) {
            Some(s) => s,
            None => {
                let src = ImageSource::from_bytes(rgb.as_raw(), rgb.dimensions()).map_err(|e| anyhow::anyhow!("{e}"))?;
                let input = self.engine.prepare_input(src)?;
                let words = self.engine.detect_words(&input)?;
                let mut heights: Vec<f32> = words.iter().map(|r| { use rten_imageproc::BoundingRect; r.bounding_rect().height() as f32 }).filter(|h| *h > 2.0).collect();
                heights.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                let median = heights.get(heights.len() / 2).copied().unwrap_or(28.0);
                (28.0_f32 / median).clamp(1.0, 3.0)
            }
        };
        let scale = if (scale - 1.0).abs() < 0.1 { 1.0 } else { scale };
        let rgb = if scale != 1.0 { image::imageops::resize(&rgb, (w as f32 * scale).round() as u32, (h as f32 * scale).round() as u32, image::imageops::FilterType::CatmullRom) } else { rgb };
        let src = ImageSource::from_bytes(rgb.as_raw(), rgb.dimensions()).map_err(|e| anyhow::anyhow!("{e}"))?;
        let input = self.engine.prepare_input(src)?;
        let words = self.engine.detect_words(&input)?;
        let line_rects = self.engine.find_text_lines(&input, &words);
        let lines = self.engine.recognize_text(&input, &line_rects)?;
        let to_box = |r: rten_imageproc::Rect| OcrBox { x: (r.left() as f32 / scale).round() as i32, y: (r.top() as f32 / scale).round() as i32, w: (r.width() as f32 / scale).round().max(1.0) as i32, h: (r.height() as f32 / scale).round().max(1.0) as i32 };
        let mut out = OcrResult { lines: vec![], scale };
        for line in lines.into_iter().flatten() {
            let text = line.to_string();
            if text.trim().is_empty() { continue; }
            let words = line.words().map(|wd| OcrWord { text: wd.to_string(), rect: to_box(wd.bounding_rect()) }).collect();
            out.lines.push(OcrLine { text: text.trim_end().to_string(), rect: to_box(line.bounding_rect()), words });
        }
        Ok(out)
    }
}
