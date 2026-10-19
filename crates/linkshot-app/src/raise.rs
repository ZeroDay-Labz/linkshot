//! Bringing the Capture window to the front from outside the UI thread.
//!
//! On Wayland the compositor stops sending frame callbacks to a minimised window, so winit does
//! not deliver redraws and the egui `update` loop (where tray and hotkey events are normally
//! handled) does not run. The tray icon and the single-instance listener therefore raise the
//! window themselves, right from the thread that received the request; once KWin has restored it
//! the UI loop resumes and handles the queued event.

pub const CAPTURE_TITLE: &str = "LinkShot Capture";

/// Raises and activates the Capture window through the compositor (Plasma Wayland only; other
/// platforms raise it from the UI loop with viewport commands). Returns immediately.
pub fn raise_capture_window_now() {
    #[cfg(target_os = "linux")]
    if linkshot_capture::is_wayland() && linkshot_capture::kwin::is_kde() {
        std::thread::spawn(|| {
            if let Err(e) = linkshot_capture::kwin::activate_window_retry(CAPTURE_TITLE) { tracing::warn!("raise capture window: {e:#}"); }
        });
    }
}
