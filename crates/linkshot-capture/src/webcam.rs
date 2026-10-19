//! Webcam frames for picture-in-picture recording, pulled from the bundled `ffmpeg`
//! (`-f v4l2` on Linux, `-f dshow` on Windows) as raw RGBA on a pipe. A device id starting with
//! `lavfi:` uses a synthetic source instead (tests, machines without a camera).

use anyhow::{Context, Result};
use image::RgbaImage;
use std::io::Read;
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Size frames are delivered at (4:3, enough for a corner overlay or a 1080p letterbox).
pub const WIDTH: u32 = 640;
pub const HEIGHT: u32 = 480;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebcamDevice { pub id: String, pub name: String }

fn ffmpeg() -> Option<linkshot_tools::Tool> { linkshot_tools::find("ffmpeg") }

pub fn available() -> bool { ffmpeg().is_some() }

/// Cameras ffmpeg can see. Empty when there is none (or no ffmpeg).
pub fn devices() -> Vec<WebcamDevice> {
    let Some(tool) = ffmpeg() else { return vec![] };
    let mut out = vec![];
    #[cfg(target_os = "linux")]
    {
        let Ok(res) = tool.command().args(["-hide_banner", "-sources", "v4l2"]).stdin(Stdio::null()).output() else { return out };
        let text = String::from_utf8_lossy(&res.stdout).into_owned() + &String::from_utf8_lossy(&res.stderr);
        for line in text.lines() {
            let t = line.trim().trim_start_matches('*').trim();
            if !t.starts_with("/dev/video") { continue; }
            let Some((id, rest)) = t.split_once(' ') else { continue };
            let name = rest.trim().strip_prefix('[').and_then(|r| r.rsplit_once(']')).map(|(n, _)| n.trim().to_string()).unwrap_or_else(|| id.to_string());
            out.push(WebcamDevice { id: id.to_string(), name });
        }
        if out.is_empty() {
            // `-sources` needs libv4l device probing; fall back to the device nodes that answer as capture devices.
            if let Ok(rd) = std::fs::read_dir("/dev") {
                let mut nodes: Vec<String> = rd.flatten().filter_map(|e| e.file_name().to_str().filter(|n| n.starts_with("video")).map(|n| format!("/dev/{n}"))).collect();
                nodes.sort();
                for id in nodes {
                    let name = std::fs::read_to_string(format!("/sys/class/video4linux/{}/name", id.trim_start_matches("/dev/"))).map(|s| s.trim().to_string()).unwrap_or_else(|_| id.clone());
                    let caps = std::fs::read_to_string(format!("/sys/class/video4linux/{}/device/..", id.trim_start_matches("/dev/"))).unwrap_or_default();
                    let _ = caps;
                    out.push(WebcamDevice { id, name });
                }
            }
        }
    }
    #[cfg(windows)]
    {
        let Ok(res) = tool.command().args(["-hide_banner", "-list_devices", "true", "-f", "dshow", "-i", "dummy"]).stdin(Stdio::null()).output() else { return out };
        let text = String::from_utf8_lossy(&res.stderr).into_owned();
        for line in text.lines() {
            let t = line.trim();
            // `[dshow @ …] "Integrated Camera" (video)`
            if !t.ends_with("(video)") { continue; }
            let Some(start) = t.find('"') else { continue };
            let Some(end) = t[start + 1..].find('"') else { continue };
            let name = t[start + 1..start + 1 + end].to_string();
            out.push(WebcamDevice { id: name.clone(), name });
        }
    }
    let _ = &tool;
    out
}

/// A running camera; `latest` is the most recent frame.
pub struct Webcam {
    latest: Arc<Mutex<Option<Arc<RgbaImage>>>>,
    error: Arc<Mutex<Option<String>>>,
    stop: Arc<AtomicBool>,
    child: Arc<Mutex<Option<Child>>>,
    thread: Option<std::thread::JoinHandle<()>>,
    pub device: String,
}

impl Webcam {
    /// Starts streaming `device` at [`WIDTH`]×[`HEIGHT`] / `fps`.
    pub fn start(device: &str, fps: u32) -> Result<Webcam> {
        let tool = ffmpeg().context("webcam recording needs ffmpeg, which is missing from this installation")?;
        let mut cmd = tool.command();
        cmd.args(["-hide_banner", "-loglevel", "error", "-nostdin"]);
        let fps = fps.clamp(1, 60);
        if let Some(src) = device.strip_prefix("lavfi:") {
            cmd.args(["-f", "lavfi", "-i", &format!("{src}=size={WIDTH}x{HEIGHT}:rate={fps}")]);
        } else {
            #[cfg(target_os = "linux")]
            cmd.args(["-f", "v4l2", "-framerate", &fps.to_string(), "-video_size", &format!("{WIDTH}x{HEIGHT}"), "-i", device]);
            #[cfg(windows)]
            cmd.args(["-f", "dshow", "-framerate", &fps.to_string(), "-i", &format!("video={device}")]);
            #[cfg(not(any(target_os = "linux", windows)))]
            anyhow::bail!("webcam capture is not supported on this platform");
        }
        cmd.args(["-vf", &format!("scale={WIDTH}:{HEIGHT}:force_original_aspect_ratio=increase,crop={WIDTH}:{HEIGHT}"), "-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"]);
        let mut child = cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().context("start ffmpeg")?;
        let mut stdout = child.stdout.take().context("ffmpeg stdout")?;
        let mut stderr = child.stderr.take();
        let latest: Arc<Mutex<Option<Arc<RgbaImage>>>> = Default::default();
        let error: Arc<Mutex<Option<String>>> = Default::default();
        let stop = Arc::new(AtomicBool::new(false));
        let child = Arc::new(Mutex::new(Some(child)));
        let (l, e, s, c) = (latest.clone(), error.clone(), stop.clone(), child.clone());
        let thread = std::thread::Builder::new().name("linkshot-webcam".into()).spawn(move || {
            let frame_len = (WIDTH * HEIGHT * 4) as usize;
            let mut buf = vec![0u8; frame_len];
            loop {
                if s.load(Ordering::Relaxed) { break; }
                match stdout.read_exact(&mut buf) {
                    Ok(()) => { if let Some(img) = RgbaImage::from_raw(WIDTH, HEIGHT, buf.clone()) { *l.lock().unwrap() = Some(Arc::new(img)); } }
                    Err(_) => {
                        if !s.load(Ordering::Relaxed) {
                            let mut msg = String::new();
                            if let Some(err) = stderr.as_mut() { let _ = err.read_to_string(&mut msg); }
                            let msg = msg.trim().lines().last().unwrap_or("the camera stopped sending frames").to_string();
                            *e.lock().unwrap() = Some(msg);
                        }
                        break;
                    }
                }
            }
            if let Some(mut ch) = c.lock().unwrap().take() { let _ = ch.kill(); let _ = ch.wait(); }
        }).context("spawn webcam thread")?;
        // Wait briefly for the first frame so a bad device fails here rather than silently.
        let t0 = std::time::Instant::now();
        while t0.elapsed() < std::time::Duration::from_secs(4) {
            if latest.lock().unwrap().is_some() { break; }
            if let Some(e) = error.lock().unwrap().clone() { stop.store(true, Ordering::Relaxed); let _ = thread.join(); anyhow::bail!("camera {device}: {e}"); }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        if latest.lock().unwrap().is_none() {
            stop.store(true, Ordering::Relaxed);
            if let Some(mut ch) = child.lock().unwrap().take() { let _ = ch.kill(); let _ = ch.wait(); }
            let _ = thread.join();
            anyhow::bail!("camera {device} sent no frames");
        }
        Ok(Webcam { latest, error, stop, child, thread: Some(thread), device: device.to_string() })
    }

    pub fn latest(&self) -> Option<Arc<RgbaImage>> { self.latest.lock().unwrap().clone() }
    pub fn error(&self) -> Option<String> { self.error.lock().unwrap().clone() }
}

impl Drop for Webcam {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(mut ch) = self.child.lock().unwrap().take() { let _ = ch.kill(); let _ = ch.wait(); }
        if let Some(t) = self.thread.take() { let _ = t.join(); }
    }
}

/// How the webcam is laid over the screen frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement { Fullscreen, TopLeft, TopRight, BottomLeft, BottomRight }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape { Circle, Rectangle, RoundedRectangle, Square, RoundedSquare }

/// Draws `cam` onto `frame` according to `placement` / `shape`. The overlay is 22 % of the frame width
/// (at least 96 px), 16 px from the edges; `Fullscreen` letterboxes the camera over the whole frame.
pub fn composite(frame: &mut RgbaImage, cam: &RgbaImage, placement: Placement, shape: Shape) {
    let (fw, fh) = frame.dimensions();
    if fw < 8 || fh < 8 { return; }
    if placement == Placement::Fullscreen {
        for p in frame.pixels_mut() { *p = image::Rgba([0, 0, 0, 255]); }
        let k = (fw as f64 / cam.width() as f64).min(fh as f64 / cam.height() as f64);
        let (w, h) = (((cam.width() as f64 * k).round() as u32).max(1), ((cam.height() as f64 * k).round() as u32).max(1));
        let scaled = image::imageops::resize(cam, w, h, image::imageops::FilterType::Triangle);
        image::imageops::overlay(frame, &scaled, ((fw - w) / 2) as i64, ((fh - h) / 2) as i64);
        return;
    }
    let square = matches!(shape, Shape::Circle | Shape::Square | Shape::RoundedSquare);
    let w = ((fw as f64 * 0.22).round() as u32).max(96).min(fw - 2);
    let h = if square { w } else { ((w as f64 * cam.height() as f64 / cam.width().max(1) as f64).round() as u32).max(1) }.min(fh - 2);
    let margin = 16i64.min((fw / 20) as i64);
    let (x, y) = match placement {
        Placement::TopLeft => (margin, margin),
        Placement::TopRight => (fw as i64 - w as i64 - margin, margin),
        Placement::BottomLeft => (margin, fh as i64 - h as i64 - margin),
        _ => (fw as i64 - w as i64 - margin, fh as i64 - h as i64 - margin),
    };
    // Source crop: centre square for the square shapes.
    let src = if square {
        let s = cam.width().min(cam.height());
        image::imageops::crop_imm(cam, (cam.width() - s) / 2, (cam.height() - s) / 2, s, s).to_image()
    } else { cam.clone() };
    let mut scaled = image::imageops::resize(&src, w, h, image::imageops::FilterType::Triangle);
    // Mask + 2 px white border.
    let radius = match shape { Shape::Circle => w as f32 / 2.0, Shape::RoundedRectangle | Shape::RoundedSquare => (w.min(h) as f32 * 0.12).max(6.0), _ => 0.0 };
    if radius > 0.0 {
        let (cw, ch) = (w as f32, h as f32);
        for (px, py, p) in scaled.enumerate_pixels_mut() {
            let (fx, fy) = (px as f32 + 0.5, py as f32 + 0.5);
            let d = rounded_rect_distance(fx, fy, cw, ch, radius);
            let a = (0.5 - d).clamp(0.0, 1.0); // antialiased edge
            let border = (d + 2.5).clamp(0.0, 1.0) * a; // 0 inside, up to 1 at the rim
            let b = (border * 255.0) as u8;
            if b > 0 { let t = b as u16; for c in 0..3 { p[c] = ((p[c] as u16 * (255 - t) + 255 * t) / 255) as u8; } }
            p[3] = (a * 255.0) as u8;
        }
    } else {
        for (px, py, p) in scaled.enumerate_pixels_mut() {
            if px < 2 || py < 2 || px + 2 >= w || py + 2 >= h { *p = image::Rgba([255, 255, 255, 255]); }
        }
    }
    image::imageops::overlay(frame, &scaled, x, y);
}

/// Signed distance from a point to the edge of a rounded rectangle (negative inside).
fn rounded_rect_distance(x: f32, y: f32, w: f32, h: f32, r: f32) -> f32 {
    let r = r.min(w / 2.0).min(h / 2.0);
    let (cx, cy) = (w / 2.0, h / 2.0);
    let (qx, qy) = ((x - cx).abs() - (cx - r), (y - cy).abs() - (cy - r));
    let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
    outside + qx.max(qy).min(0.0) - r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composite_places_overlay() {
        let mut frame = RgbaImage::from_pixel(400, 300, image::Rgba([10, 20, 30, 255]));
        let cam = RgbaImage::from_pixel(WIDTH, HEIGHT, image::Rgba([200, 0, 0, 255]));
        composite(&mut frame, &cam, Placement::BottomRight, Shape::Circle);
        // Centre of the overlay is camera red; a corner of the overlay box stays screen colour (circle mask).
        let w = (400.0f64 * 0.22).round() as u32; // 88 -> clamped to 96
        let w = w.max(96);
        let (x, y) = (400 - w as i64 - 16, 300 - w as i64 - 16);
        let centre = frame.get_pixel((x + w as i64 / 2) as u32, (y + w as i64 / 2) as u32);
        assert_eq!(centre.0[0], 200);
        let corner = frame.get_pixel(x as u32, y as u32);
        assert_eq!(corner.0, [10, 20, 30, 255]);
        composite(&mut frame, &cam, Placement::Fullscreen, Shape::Rectangle);
        assert_eq!(frame.get_pixel(200, 150).0[0], 200);
    }
}
