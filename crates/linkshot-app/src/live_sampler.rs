//! Feeds the live capture overlay: a worker thread grabs a small square of the screen around the
//! pointer for the magnifier loupe (~15 fps) and refreshes the window list for the window highlight.

use image::RgbaImage;
use linkshot_capture::WindowInfo;
use linkshot_core::{Pt, Rect};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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
            let mut last: Option<Pt> = None;
            let mut last_grab = Instant::now() - Duration::from_secs(1);
            let mut last_windows = Instant::now() - Duration::from_secs(1);
            while !s.load(Ordering::Relaxed) {
                if last_windows.elapsed() >= Duration::from_millis(500) {
                    last_windows = Instant::now();
                    if let Ok(w) = linkshot_capture::windows() { let _ = wtx.try_send(w); }
                }
                let cur = *p.lock().unwrap();
                let due = cur.is_some() && (cur != last || last_grab.elapsed() >= Duration::from_millis(250)) && last_grab.elapsed() >= Duration::from_millis(66);
                if sample_region && due {
                    let at = cur.unwrap();
                    last = cur;
                    last_grab = Instant::now();
                    let r = Rect::from_xywh((at.x - SAMPLE / 2.0).round(), (at.y - SAMPLE / 2.0).round(), SAMPLE, SAMPLE);
                    if let Ok(img) = linkshot_capture::capture_region(r) {
                        let _ = tx.send((at, img));
                        wake();
                    }
                } else {
                    std::thread::sleep(Duration::from_millis(16));
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

impl Drop for LiveSampler {
    fn drop(&mut self) { self.stop.store(true, Ordering::Relaxed); }
}
