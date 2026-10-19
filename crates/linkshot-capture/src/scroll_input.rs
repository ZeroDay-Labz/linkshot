//! Synthetic mouse-wheel input for Automatic Scrolling.
//!
//! * Wayland: the `org.freedesktop.portal.RemoteDesktop` portal (the desktop asks once; a restore token
//!   lets later sessions skip the dialog).
//! * X11 / Windows: `enigo` (XTest / SendInput).

use anyhow::{Context, Result};

pub struct Scroller {
    inner: Inner,
    /// Token to pass to the next `Scroller::new` so the portal does not prompt again (Wayland only).
    pub restore_token: Option<String>,
}

/// One synthetic input action.
#[derive(Clone, Copy, Debug)]
pub enum Cmd {
    /// Wheel notches (positive dy = content scrolls down).
    Scroll(i32, i32),
    /// Move the pointer to logical desktop coordinates.
    MoveTo(f64, f64),
    /// Move the pointer by a delta in logical pixels.
    MoveBy(f64, f64),
    /// Smooth (pixel) scroll by a delta in logical pixels (positive dy = content scrolls down).
    ScrollPixels(f64, f64),
    /// Left-button click at the current pointer position.
    Click,
}

enum Inner {
    #[cfg(target_os = "linux")]
    Portal(crossbeam_channel::Sender<Cmd>),
    #[cfg(any(target_os = "linux", windows))]
    Enigo(enigo::Enigo),
}

impl Scroller {
    /// Connects to the platform input facility. May block while the user confirms the portal dialog.
    pub fn new(restore_token: Option<String>) -> Result<Scroller> {
        #[cfg(target_os = "linux")]
        if crate::is_wayland() {
            let (tx, token) = portal::start(restore_token)?;
            return Ok(Scroller { inner: Inner::Portal(tx), restore_token: token });
        }
        #[cfg(any(target_os = "linux", windows))]
        {
            let e = enigo::Enigo::new(&enigo::Settings::default()).map_err(|e| anyhow::anyhow!("input: {e}"))?;
            Ok(Scroller { inner: Inner::Enigo(e), restore_token })
        }
        #[cfg(not(any(target_os = "linux", windows)))]
        { anyhow::bail!("synthetic scrolling is not supported on this platform") }
    }

    /// Scrolls by wheel notches; positive `dy` scrolls content down (towards the end), positive `dx` right.
    pub fn scroll(&mut self, dx: i32, dy: i32) -> Result<()> {
        // xdg-desktop-portal-kde forwards the step count of NotifyPointerAxisDiscrete unchanged as
        // the wl_pointer axis value, i.e. as *pixels*: 3 notches would scroll a browser by 3 px.
        // Scale it to what a wheel notch moves (measured: 30 steps ≈ 300 px).
        #[cfg(target_os = "linux")]
        if matches!(self.inner, Inner::Portal(_)) && crate::kwin::is_kde() { return self.send(Cmd::Scroll(dx * 40, dy * 40)); }
        self.send(Cmd::Scroll(dx, dy))
    }

    /// Smooth scroll by pixels (touchpad-style); falls back to wheel notches where unsupported.
    pub fn scroll_pixels(&mut self, dx: f64, dy: f64) -> Result<()> { self.send(Cmd::ScrollPixels(dx, dy)) }

    /// Moves the pointer to logical desktop coordinates (wheel events go to the window under it).
    ///
    /// The RemoteDesktop portal only accepts absolute positions relative to a screencast stream, so
    /// on KDE the position is reached with relative moves checked against KWin's cursor position.
    pub fn move_to(&mut self, x: f64, y: f64) -> Result<()> {
        #[cfg(target_os = "linux")]
        if matches!(self.inner, Inner::Portal(_)) {
            if !crate::kwin::is_kde() { anyhow::bail!("pointer position unknown on this compositor"); }
            for _ in 0..4 {
                let (cx, cy) = crate::kwin::cursor_pos()?;
                if (cx - x).abs() < 2.0 && (cy - y).abs() < 2.0 { return Ok(()); }
                self.send(Cmd::MoveBy(x - cx, y - cy))?;
                std::thread::sleep(std::time::Duration::from_millis(60));
            }
            let (cx, cy) = crate::kwin::cursor_pos()?;
            anyhow::ensure!((cx - x).abs() < 4.0 && (cy - y).abs() < 4.0, "pointer ended at ({cx:.0},{cy:.0}) instead of ({x:.0},{y:.0})");
            return Ok(());
        }
        self.send(Cmd::MoveTo(x, y))
    }

    /// Left click at the current pointer position.
    pub fn click(&mut self) -> Result<()> { self.send(Cmd::Click) }

    pub fn send(&mut self, cmd: Cmd) -> Result<()> {
        match &mut self.inner {
            #[cfg(target_os = "linux")]
            Inner::Portal(tx) => tx.send(cmd).context("scroll thread gone"),
            #[cfg(any(target_os = "linux", windows))]
            Inner::Enigo(e) => {
                use enigo::Mouse;
                let err = |e: enigo::InputError| anyhow::anyhow!("input: {e}");
                match cmd {
                    Cmd::Scroll(dx, dy) => {
                        if dy != 0 { e.scroll(dy, enigo::Axis::Vertical).map_err(err)?; }
                        if dx != 0 { e.scroll(dx, enigo::Axis::Horizontal).map_err(err)?; }
                    }
                    Cmd::MoveTo(x, y) => e.move_mouse(x.round() as i32, y.round() as i32, enigo::Coordinate::Abs).map_err(err)?,
                    Cmd::MoveBy(dx, dy) => e.move_mouse(dx.round() as i32, dy.round() as i32, enigo::Coordinate::Rel).map_err(err)?,
                    Cmd::ScrollPixels(dx, dy) => {
                        let (nx, ny) = ((dx / 40.0).round() as i32, (dy / 40.0).round() as i32);
                        if ny != 0 { e.scroll(ny, enigo::Axis::Vertical).map_err(err)?; }
                        if nx != 0 { e.scroll(nx, enigo::Axis::Horizontal).map_err(err)?; }
                    }
                    Cmd::Click => e.button(enigo::Button::Left, enigo::Direction::Click).map_err(err)?,
                }
                Ok(())
            }
        }
    }
}

#[cfg(target_os = "linux")]
mod portal {
    use anyhow::{Context, Result};
    use std::time::Duration;

    pub fn start(restore_token: Option<String>) -> Result<(crossbeam_channel::Sender<super::Cmd>, Option<String>)> {
        use super::Cmd;
        let (tx, rx) = crossbeam_channel::unbounded::<Cmd>();
        let (ready_tx, ready_rx) = crossbeam_channel::bounded::<Result<Option<String>>>(1);
        std::thread::Builder::new().name("remote-desktop-scroll".into()).spawn(move || {
            let ready_err = ready_tx.clone();
            let res: Result<(), ashpd::Error> = async_io::block_on(async move {
                use ashpd::desktop::remote_desktop::{Axis, DeviceType, KeyState, RemoteDesktop, SelectDevicesOptions, StartOptions, NotifyPointerAxisDiscreteOptions, NotifyPointerMotionAbsoluteOptions, NotifyPointerMotionOptions, NotifyPointerButtonOptions, NotifyPointerAxisOptions};
                use ashpd::desktop::PersistMode;
                let rd = RemoteDesktop::new().await?;
                let session = rd.create_session(ashpd::desktop::CreateSessionOptions::default()).await?;
                let opts = SelectDevicesOptions::default().set_devices(enumflags2::BitFlags::from(DeviceType::Pointer)).set_persist_mode(PersistMode::ExplicitlyRevoked).set_restore_token(restore_token.as_deref());
                rd.select_devices(&session, opts).await?.response()?;
                let devices = rd.start(&session, None, StartOptions::default()).await?.response()?;
                let token = devices.restore_token().map(|s| s.to_string());
                let _ = ready_tx.send(Ok(token));
                while let Ok(cmd) = rx.recv() {
                    // One refused event (e.g. an absolute move without a stream) must not end the
                    // session: log it and keep serving.
                    let r: Result<(), ashpd::Error> = match cmd {
                        Cmd::Scroll(dx, dy) => {
                            let mut r = Ok(());
                            if dy != 0 { r = rd.notify_pointer_axis_discrete(&session, Axis::Vertical, dy, NotifyPointerAxisDiscreteOptions::default()).await; }
                            if r.is_ok() && dx != 0 { r = rd.notify_pointer_axis_discrete(&session, Axis::Horizontal, dx, NotifyPointerAxisDiscreteOptions::default()).await; }
                            r
                        }
                        // Stream 0 = no screencast stream; KDE rejects this ("Invalid position"), which is
                        // why `Scroller::move_to` uses relative moves there.
                        Cmd::MoveTo(x, y) => rd.notify_pointer_motion_absolute(&session, 0, x, y, NotifyPointerMotionAbsoluteOptions::default()).await,
                        Cmd::MoveBy(dx, dy) => rd.notify_pointer_motion(&session, dx, dy, NotifyPointerMotionOptions::default()).await,
                        Cmd::ScrollPixels(dx, dy) => rd.notify_pointer_axis(&session, dx, dy, NotifyPointerAxisOptions::default().set_finish(true)).await,
                        Cmd::Click => {
                            const BTN_LEFT: i32 = 0x110;
                            match rd.notify_pointer_button(&session, BTN_LEFT, KeyState::Pressed, NotifyPointerButtonOptions::default()).await {
                                Ok(()) => rd.notify_pointer_button(&session, BTN_LEFT, KeyState::Released, NotifyPointerButtonOptions::default()).await,
                                Err(e) => Err(e),
                            }
                        }
                    };
                    if let Err(e) = r { tracing::warn!("RemoteDesktop portal: {cmd:?} failed: {e}"); }
                }
                Ok(())
            });
            if let Err(e) = res { tracing::warn!("RemoteDesktop portal: {e}"); let _ = ready_err.try_send(Err(anyhow::anyhow!("RemoteDesktop portal: {e}"))); }
        }).context("spawn scroll thread")?;
        let token = ready_rx.recv_timeout(Duration::from_secs(120)).context("RemoteDesktop portal did not answer")??;
        Ok((tx, token))
    }
}
