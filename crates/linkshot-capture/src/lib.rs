//! Screen, monitor and window capture.
//!
//! Backed by `xcap` (X11 / Wayland / Windows). Coordinates are in the virtual desktop space in
//! physical pixels, which is what the selection overlay and `.snagx` documents use.

use anyhow::{Context, Result};
use image::RgbaImage;
use linkshot_core::geometry::Rect;

#[cfg(target_os = "linux")]
pub mod kwin;
/// On other platforms the KWin helpers exist but report "not KDE" so callers need no `cfg` guards.
#[cfg(not(target_os = "linux"))]
pub mod kwin {
    use anyhow::{bail, Result};
    pub fn is_kde() -> bool { false }
    pub fn tame_helper_window(_caption: &str, _refocus: &str) -> Result<()> { bail!("not running under KWin") }
    pub fn focus_away_from(_caption: &str) -> Result<()> { bail!("not running under KWin") }
}
#[cfg(target_os = "linux")]
pub mod portal;
#[cfg(target_os = "linux")]
pub mod wayland_outputs;

pub mod stitch;
pub mod video;
pub mod scroll_input;
pub mod scrolling;

/// True when running under a Wayland compositor (xcap's X11/wlr paths hang on KDE/GNOME, so we use the portal).
pub fn is_wayland() -> bool {
    #[cfg(target_os = "linux")]
    { std::env::var_os("WAYLAND_DISPLAY").is_some() && std::env::var("XDG_SESSION_TYPE").map(|s| s != "x11").unwrap_or(true) }
    #[cfg(not(target_os = "linux"))]
    { false }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MonitorInfo {
    pub id: u32,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f32,
    pub is_primary: bool,
}

impl MonitorInfo {
    pub fn rect(&self) -> Rect {
        Rect::from_xywh(self.x as f64, self.y as f64, self.width as f64, self.height as f64)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowInfo {
    pub id: u32,
    pub title: String,
    pub app_name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub is_minimized: bool,
    pub is_maximized: bool,
    pub z_order: usize,
}

impl WindowInfo {
    pub fn rect(&self) -> Rect {
        Rect::from_xywh(self.x as f64, self.y as f64, self.width as f64, self.height as f64)
    }
}

/// A frozen snapshot of every monitor, taken before the selection overlay appears.
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub monitors: Vec<MonitorInfo>,
    pub images: Vec<RgbaImage>,
    pub windows: Vec<WindowInfo>,
    pub taken_at: std::time::Instant,
}

impl Snapshot {
    /// Bounding box of all monitors (the virtual desktop).
    pub fn virtual_rect(&self) -> Rect {
        self.monitors.iter().map(MonitorInfo::rect).reduce(|a, b| a.union(b)).unwrap_or_default()
    }
    pub fn monitor_at(&self, x: f64, y: f64) -> Option<usize> {
        self.monitors.iter().position(|m| m.rect().contains(linkshot_core::Pt::new(x, y)))
    }
    /// Pixel density (image pixels per logical unit) of the densest monitor touching `region`.
    pub fn scale_at(&self, region: Rect) -> f64 {
        let mut scale = 0.0f64;
        for (m, img) in self.monitors.iter().zip(&self.images) {
            let mr = m.rect();
            let inter = Rect { min: region.min.max(mr.min), max: region.max.min(mr.max) };
            if inter.is_empty() { continue; }
            scale = scale.max(img.width() as f64 / mr.width().max(1.0));
        }
        scale
    }

    /// Pixels of `region` (virtual-desktop / logical coordinates) composed from the monitor images at
    /// **native resolution**: on a HiDPI monitor the result has more pixels than the logical region
    /// (no downscaling). Monitors with the same density are copied row by row, pixel-exact.
    pub fn crop(&self, region: Rect) -> Option<RgbaImage> {
        let region = region.round_out();
        if region.is_empty() { return None; }
        let scale = self.scale_at(region);
        if scale <= 0.0 { return None; }
        let w = (region.width() * scale).round() as u32;
        let h = (region.height() * scale).round() as u32;
        if w == 0 || h == 0 { return None; }
        let mut out = RgbaImage::new(w, h);
        let mut any = false;
        for (m, img) in self.monitors.iter().zip(&self.images) {
            let mr = m.rect();
            let inter = Rect { min: region.min.max(mr.min), max: region.max.min(mr.max) };
            if inter.is_empty() { continue; }
            any = true;
            let sx = img.width() as f64 / mr.width().max(1.0);
            let sy = img.height() as f64 / mr.height().max(1.0);
            // Destination block (output pixels) and source origin (image pixels).
            let dx0 = ((inter.min.x - region.min.x) * scale).round() as i64;
            let dy0 = ((inter.min.y - region.min.y) * scale).round() as i64;
            let dx1 = (((inter.max.x - region.min.x) * scale).round() as i64).min(w as i64);
            let dy1 = (((inter.max.y - region.min.y) * scale).round() as i64).min(h as i64);
            let sx0 = ((inter.min.x - mr.min.x) * sx).round() as i64;
            let sy0 = ((inter.min.y - mr.min.y) * sy).round() as i64;
            let same_density = (sx - scale).abs() < 1e-6 && (sy - scale).abs() < 1e-6;
            let (iw, ih) = (img.width() as i64, img.height() as i64);
            for dy in dy0.max(0)..dy1 {
                let iy = if same_density { sy0 + (dy - dy0) } else { (((dy as f64 / scale) + region.min.y - mr.min.y) * sy) as i64 };
                if iy < 0 || iy >= ih { continue; }
                if same_density {
                    let x0 = dx0.max(0);
                    let count = (dx1 - x0).min(iw - (sx0 + (x0 - dx0))).max(0) as usize;
                    if count == 0 { continue; }
                    let src_off = ((iy * iw + sx0 + (x0 - dx0)) * 4) as usize;
                    let dst_off = ((dy * w as i64 + x0) * 4) as usize;
                    out.as_mut()[dst_off..dst_off + count * 4].copy_from_slice(&img.as_raw()[src_off..src_off + count * 4]);
                } else {
                    for dx in dx0.max(0)..dx1 {
                        let ix = (((dx as f64 / scale) + region.min.x - mr.min.x) * sx) as i64;
                        if ix < 0 || ix >= iw { continue; }
                        out.put_pixel(dx as u32, dy as u32, *img.get_pixel(ix as u32, iy as u32));
                    }
                }
            }
        }
        any.then_some(out)
    }
    /// The topmost non-minimized window containing the point (ignores our own overlay windows by title).
    pub fn window_at(&self, x: f64, y: f64, ignore_title_prefix: &str) -> Option<&WindowInfo> {
        self.windows
            .iter()
            .filter(|w| !w.is_minimized && w.width > 0 && w.height > 0 && !(!ignore_title_prefix.is_empty() && w.title.starts_with(ignore_title_prefix)))
            .filter(|w| w.rect().contains(linkshot_core::Pt::new(x, y)))
            .min_by_key(|w| w.z_order)
    }
}

pub fn monitors() -> Result<Vec<MonitorInfo>> {
    #[cfg(target_os = "linux")]
    if is_wayland() {
        let outs = wayland_outputs::outputs()?;
        let mut v: Vec<MonitorInfo> = outs.iter().enumerate().map(|(i, o)| MonitorInfo {
            id: i as u32, name: if o.name.is_empty() { o.description.clone() } else { o.name.clone() }, x: o.x, y: o.y,
            width: o.width as u32, height: o.height as u32, scale_factor: o.scale as f32, is_primary: o.x == 0 && o.y == 0,
        }).collect();
        if !v.iter().any(|m| m.is_primary) { if let Some(m) = v.first_mut() { m.is_primary = true; } }
        return Ok(v);
    }
    let list = xcap::Monitor::all().context("enumerate monitors")?;
    let mut out = Vec::new();
    for m in list {
        out.push(MonitorInfo {
            id: m.id().unwrap_or(0),
            name: m.name().unwrap_or_default(),
            x: m.x().unwrap_or(0),
            y: m.y().unwrap_or(0),
            width: m.width().unwrap_or(0),
            height: m.height().unwrap_or(0),
            scale_factor: m.scale_factor().unwrap_or(1.0),
            is_primary: m.is_primary().unwrap_or(false),
        });
    }
    Ok(out)
}

pub fn windows() -> Result<Vec<WindowInfo>> {
    if is_wayland() { return Ok(vec![]); }
    let list = xcap::Window::all().context("enumerate windows")?;
    let mut out = Vec::new();
    for (z, w) in list.into_iter().enumerate() {
        out.push(WindowInfo {
            id: w.id().unwrap_or(0),
            title: w.title().unwrap_or_default(),
            app_name: w.app_name().unwrap_or_default(),
            x: w.x().unwrap_or(0),
            y: w.y().unwrap_or(0),
            width: w.width().unwrap_or(0),
            height: w.height().unwrap_or(0),
            is_minimized: w.is_minimized().unwrap_or(false),
            is_maximized: w.is_maximized().unwrap_or(false),
            z_order: z,
        });
    }
    Ok(out)
}

pub fn capture_monitor(id: u32) -> Result<RgbaImage> {
    if is_wayland() {
        let snap = snapshot()?;
        let idx = snap.monitors.iter().position(|m| m.id == id).unwrap_or(0);
        return Ok(snap.images[idx].clone());
    }
    let m = xcap::Monitor::all()?.into_iter().find(|m| m.id().ok() == Some(id)).context("monitor not found")?;
    Ok(m.capture_image().context("capture monitor")?)
}

pub fn capture_window(id: u32) -> Result<RgbaImage> {
    anyhow::ensure!(!is_wayland(), "window capture is not available on Wayland");
    let w = xcap::Window::all()?.into_iter().find(|w| w.id().ok() == Some(id)).context("window not found")?;
    Ok(w.capture_image().context("capture window")?)
}

/// Captures every monitor (used for the frozen overlay background and fullscreen captures).
pub fn snapshot() -> Result<Snapshot> {
    #[cfg(target_os = "linux")]
    if is_wayland() {
        let monitors = monitors()?;
        anyhow::ensure!(!monitors.is_empty(), "no monitors found");
        // Preferred on Plasma: KWin's ScreenShot2 (no prompt once authorised), per output at native resolution.
        if kwin::is_kde() {
            let mut images = Vec::new();
            let mut ok = true;
            for m in &monitors {
                match kwin::capture_screen(&m.name, false) {
                    Ok(img) => images.push(img),
                    Err(e) => { tracing::warn!("KWin ScreenShot2 unavailable ({e:#}); falling back to the portal"); ok = false; break; }
                }
            }
            if ok { return Ok(Snapshot { monitors, images, windows: vec![], taken_at: std::time::Instant::now() }); }
        }
        let composite = portal::screenshot()?;
        // The portal returns one image covering the whole (logical) desktop in physical pixels.
        let virt = monitors.iter().map(MonitorInfo::rect).reduce(|a, b| a.union(b)).unwrap_or_default();
        let sx = composite.width() as f64 / virt.width().max(1.0);
        let sy = composite.height() as f64 / virt.height().max(1.0);
        let mut images = Vec::new();
        for m in &monitors {
            let x = ((m.x as f64 - virt.min.x) * sx).round().max(0.0) as u32;
            let y = ((m.y as f64 - virt.min.y) * sy).round().max(0.0) as u32;
            let w = ((m.width as f64) * sx).round() as u32;
            let h = ((m.height as f64) * sy).round() as u32;
            let w = w.min(composite.width().saturating_sub(x)).max(1);
            let h = h.min(composite.height().saturating_sub(y)).max(1);
            images.push(image::imageops::crop_imm(&composite, x, y, w, h).to_image());
        }
        return Ok(Snapshot { monitors, images, windows: vec![], taken_at: std::time::Instant::now() });
    }
    let mons = xcap::Monitor::all().context("enumerate monitors")?;
    anyhow::ensure!(!mons.is_empty(), "no monitors found");
    let mut monitors = Vec::new();
    let mut images = Vec::new();
    for m in mons {
        let info = MonitorInfo {
            id: m.id().unwrap_or(0),
            name: m.name().unwrap_or_default(),
            x: m.x().unwrap_or(0),
            y: m.y().unwrap_or(0),
            width: m.width().unwrap_or(0),
            height: m.height().unwrap_or(0),
            scale_factor: m.scale_factor().unwrap_or(1.0),
            is_primary: m.is_primary().unwrap_or(false),
        };
        let img = m.capture_image().with_context(|| format!("capture monitor {}", info.name))?;
        monitors.push(info);
        images.push(img);
    }
    let windows = windows().unwrap_or_default();
    Ok(Snapshot { monitors, images, windows, taken_at: std::time::Instant::now() })
}

/// Captures a region of the live screen (logical virtual-desktop coordinates) at native resolution,
/// without the cursor. Used for scrolling-capture frames, so it must be fast and pixel-exact.
pub fn capture_region(region: Rect) -> Result<RgbaImage> { capture_region_with_cursor(region, false) }

/// [`capture_region`] with the pointer drawn in when the back-end supports it (KWin); used by the
/// video recorder. Other back-ends return the frame without the cursor.
pub fn capture_region_with_cursor(region: Rect, include_cursor: bool) -> Result<RgbaImage> {
    let r = region.round_out();
    anyhow::ensure!(!r.is_empty(), "empty region");
    #[cfg(not(target_os = "linux"))]
    let _ = include_cursor;
    #[cfg(target_os = "linux")]
    if is_wayland() && kwin::is_kde() {
        match kwin::capture_area(r.min.x as i32, r.min.y as i32, r.width() as u32, r.height() as u32, include_cursor) {
            Ok(img) => return Ok(img),
            Err(e) => tracing::warn!("KWin CaptureArea failed ({e:#}); falling back to a full snapshot"),
        }
    }
    let snap = snapshot_intersecting(r)?;
    snap.crop(r).context("region is off-screen")
}

/// Like [`snapshot`] but captures only the monitors that intersect `region` (faster for frame grabbing).
pub fn snapshot_intersecting(region: Rect) -> Result<Snapshot> {
    #[cfg(target_os = "linux")]
    if is_wayland() { return snapshot(); }
    let mons = xcap::Monitor::all().context("enumerate monitors")?;
    let mut monitors = Vec::new();
    let mut images = Vec::new();
    for m in mons {
        let info = MonitorInfo {
            id: m.id().unwrap_or(0), name: m.name().unwrap_or_default(), x: m.x().unwrap_or(0), y: m.y().unwrap_or(0),
            width: m.width().unwrap_or(0), height: m.height().unwrap_or(0), scale_factor: m.scale_factor().unwrap_or(1.0), is_primary: m.is_primary().unwrap_or(false),
        };
        let inter = Rect { min: region.min.max(info.rect().min), max: region.max.min(info.rect().max) };
        if inter.is_empty() { continue; }
        let img = m.capture_image().with_context(|| format!("capture monitor {}", info.name))?;
        monitors.push(info);
        images.push(img);
    }
    anyhow::ensure!(!monitors.is_empty(), "region is off-screen");
    Ok(Snapshot { monitors, images, windows: vec![], taken_at: std::time::Instant::now() })
}

/// Fullscreen capture of the monitor containing the point, or the primary one.
pub fn capture_fullscreen_at(x: f64, y: f64) -> Result<RgbaImage> {
    let snap = snapshot()?;
    let idx = snap.monitor_at(x, y).or_else(|| snap.monitors.iter().position(|m| m.is_primary)).unwrap_or(0);
    Ok(snap.images[idx].clone())
}

/// Returns true if two images are identical (used by "discard identical captures").
pub fn images_identical(a: &RgbaImage, b: &RgbaImage) -> bool {
    a.dimensions() == b.dimensions() && a.as_raw() == b.as_raw()
}
