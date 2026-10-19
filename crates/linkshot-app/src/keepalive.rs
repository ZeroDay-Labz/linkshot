//! The root window is a 2×2 px transparent controller that is never minimised; everything the
//! user sees (Capture window, Editor, overlays, HUD) is a child viewport.
//!
//! Why: Wayland compositors stop sending frame callbacks to a minimised window and winit then
//! holds back its redraws. eframe routes every repaint — including repaints of child viewports —
//! through the root window, so an app whose root is minimised never runs `update`: hotkeys, tray
//! clicks, timed captures and requests from a second launch all stall until the user restores
//! the window. With the root kept mapped (KWin is asked to keep it out of the taskbar, switcher
//! and pager and below every other window) the loop always runs, and hiding the Capture window
//! is simply not showing its viewport, which also works on Wayland where `set_visible` does not.

use std::sync::OnceLock;
use std::time::Duration;

/// Caption of the root controller window.
pub const TITLE: &str = "linkshot";

fn kde_wayland() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| cfg!(target_os = "linux") && linkshot_capture::is_wayland() && linkshot_capture::kwin::is_kde())
}

/// Repaint the root (and so run `update`) now, from any thread.
pub fn wake(ctx: &egui::Context) { ctx.request_repaint_of(egui::ViewportId::ROOT); }

/// Repaint the root after `d`: timers that must fire whatever the visible windows are doing.
pub fn wake_after(ctx: &egui::Context, d: Duration) { ctx.request_repaint_after_for(d, egui::ViewportId::ROOT); }

#[derive(Default)]
pub struct Keepalive { frames: u32, tamed: bool, root_focused: bool, tame_result: Option<std::sync::Arc<std::sync::atomic::AtomicU8>>, attempts: u32, last_attempt: Option<std::time::Instant> }

impl Keepalive {
    /// Call once per root `update`, on the UI thread with the root context.
    pub fn tick(&mut self, ctx: &egui::Context, headless: bool) {
        if headless || !kde_wayland() { return; }
        self.frames += 1;
        // Tame the controller window once it exists; KWin can be busy mapping our windows right
        // after start-up, so retry a few times.
        let pending = self.tame_result.as_ref().map(|r| r.load(std::sync::atomic::Ordering::Relaxed));
        if !self.tamed && self.frames >= 2 && self.attempts < 6 && pending != Some(0) && self.last_attempt.is_none_or(|t| t.elapsed() > Duration::from_millis(700)) {
            match pending {
                Some(1) => { self.tamed = true; }
                _ => {
                    let flag = std::sync::Arc::new(std::sync::atomic::AtomicU8::new(0));
                    self.tame_result = Some(flag.clone());
                    self.attempts += 1;
                    self.last_attempt = Some(std::time::Instant::now());
                    let attempt = self.attempts;
                    std::thread::spawn(move || {
                        match linkshot_capture::kwin::tame_helper_window(TITLE, crate::raise::CAPTURE_TITLE) {
                            Ok(()) => flag.store(1, std::sync::atomic::Ordering::Relaxed),
                            Err(e) => { tracing::warn!("controller window (attempt {attempt}): {e:#}"); flag.store(2, std::sync::atomic::Ordering::Relaxed); }
                        }
                    });
                }
            }
            wake_after(ctx, Duration::from_millis(750));
        } else if pending == Some(1) && !self.tamed { self.tamed = true; }
        // KWin hands focus to the controller when the window that had it goes away; pass it on.
        let focused = ctx.input(|i| i.viewport().focused.unwrap_or(false));
        if focused && !self.root_focused && self.tamed {
            std::thread::spawn(|| { if let Err(e) = linkshot_capture::kwin::focus_away_from(TITLE) { tracing::warn!("controller focus: {e:#}"); } });
        }
        self.root_focused = focused;
    }
}
