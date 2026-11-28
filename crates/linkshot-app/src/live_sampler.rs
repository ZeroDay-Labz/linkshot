//! Feeds the live capture overlay: a worker thread grabs a small square of the screen around the
//! pointer for the magnifier loupe and provides the window list for the window highlight.

use image::RgbaImage;
use linkshot_capture::WindowInfo;
use linkshot_core::{Pt, Rect};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Side of the sampled square in logical pixels.
pub const SAMPLE: f64 = 56.0;

pub struct LiveSampler {
    pointer: Arc<Mutex<Option<Pt>>>,
    rx: crossbeam_channel::Receiver<(Pt, RgbaImage)>,
    wrx: crossbeam_channel::Receiver<Vec<WindowInfo>>,
    stop: Arc<AtomicBool>,
}

impl LiveSampler {
    pub fn start(sample_region: bool, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        let pointer: Arc<Mutex<Option<Pt>>> = Default::default();
        let (tx, rx) = crossbeam_channel::unbounded::<(Pt, RgbaImage)>();
        let (wtx, wrx) = crossbeam_channel::bounded::<Vec<WindowInfo>>(1);
        let stop = Arc::new(AtomicBool::new(false));
        let (p, s) = (pointer.clone(), stop.clone());
        std::thread::Builder::new().name("linkshot-live-sampler".into()).spawn(move || {
            // One picture of the desktop (without our overlay) when the capture starts; the loupe
            // is cut from it locally. Grabbing the screen over and over made the compositor itself
            // work on every pointer move, and the whole desktop stuttered.
            // Let the overlay's first frames reach the screen before KWin is asked for the picture.
            std::thread::sleep(Duration::from_millis(120));
            let snap = grab_once();
            let windows = match &snap { Some(s) if !s.windows.is_empty() => s.windows.clone(), _ => linkshot_capture::windows().unwrap_or_default() };
            let _ = wtx.try_send(windows);
            wake();
            let mut last: Option<Pt> = None;
            while !s.load(Ordering::Relaxed) {
                let cur = *p.lock().unwrap();
                match (sample_region, &snap, cur) {
                    (true, Some(snap), Some(at)) if cur != last => {
                        last = cur;
                        let r = Rect::from_xywh((at.x - SAMPLE / 2.0).round(), (at.y - SAMPLE / 2.0).round(), SAMPLE, SAMPLE);
                        if let Some(img) = snap.crop(r) { let _ = tx.send((at, img)); wake(); }
                    }
                    _ => std::thread::sleep(Duration::from_millis(8)),
                }
            }
        }).ok();
        Self { pointer, rx, wrx, stop }
    }

    pub fn set_pointer(&self, p: Pt) { *self.pointer.lock().unwrap() = Some(p); }
    /// The newest loupe sample, if a new one arrived.
    pub fn latest(&self) -> Option<(Pt, RgbaImage)> { let mut out = None; while let Ok(v) = self.rx.try_recv() { out = Some(v); } out }
    pub fn windows(&self) -> Option<Vec<WindowInfo>> { let mut out = None; while let Ok(v) = self.wrx.try_recv() { out = Some(v); } out }
}

/// The desktop without LinkShot's windows (KWin leaves them out), for the loupe.
fn grab_once() -> Option<linkshot_capture::Snapshot> {
    #[cfg(target_os = "linux")]
    { linkshot_capture::kwin::without_own_windows(linkshot_capture::snapshot).map_err(|e| tracing::warn!("magnifier snapshot: {e:#}")).ok() }
    #[cfg(not(target_os = "linux"))]
    { linkshot_capture::snapshot().ok() }
}

impl Drop for LiveSampler {
    fn drop(&mut self) { self.stop.store(true, Ordering::Relaxed); }
}
