//! Which back-end can grab a region of the *live* screen quickly enough for the transparent capture
//! overlay (and its magnifier loupe). `probe()` is cheap and runs once per capture.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    /// KWin ScreenShot2 `CaptureArea` (KDE Plasma Wayland, authorised).
    Kwin,
    /// xcap monitor grabs (X11 with a compositor, Windows).
    Xcap,
    /// XDG ScreenCast portal + PipeWire (Wayland, Flatpak): per-monitor frames after one prompt.
    ScreenCast,
    /// XDG Screenshot portal (any Wayland): one composite per grab, too slow for a loupe.
    PortalScreenshot,
}

impl Backend {
    /// A small region can be grabbed many times a second (magnifier, window highlight refresh).
    pub fn supports_sampling(self) -> bool { matches!(self, Backend::Kwin | Backend::Xcap) }
}

/// The live back-end for this session, or `None` when only a frozen capture can work (X11 without a
/// compositor, where a transparent overlay would be a black window).
pub fn probe() -> Option<Backend> {
    #[cfg(target_os = "linux")]
    {
        if crate::is_wayland() {
            if crate::kwin::is_kde() && crate::kwin::capture_area(0, 0, 2, 2, false).is_ok() { return Some(Backend::Kwin); }
            return Some(if crate::screencast::available() { Backend::ScreenCast } else { Backend::PortalScreenshot });
        }
        return if x11_has_compositor() { Some(Backend::Xcap) } else { None };
    }
    #[cfg(windows)]
    { Some(Backend::Xcap) }
    #[cfg(not(any(target_os = "linux", windows)))]
    { None }
}

/// X11: a compositing manager owns the `_NET_WM_CM_S<screen>` selection.
#[cfg(target_os = "linux")]
fn x11_has_compositor() -> bool {
    use x11rb::protocol::xproto::ConnectionExt as _;
    let Ok((conn, screen)) = x11rb::connect(None) else { return false };
    let name = format!("_NET_WM_CM_S{screen}");
    let Ok(cookie) = conn.intern_atom(false, name.as_bytes()) else { return false };
    let Ok(atom) = cookie.reply() else { return false };
    let Ok(cookie) = conn.get_selection_owner(atom.atom) else { return false };
    cookie.reply().map(|r| r.owner != x11rb::NONE).unwrap_or(false)
}
