//! One running copy of linkshot per user. A second launch (the desktop launcher, a taskbar
//! button, a script) hands its request to the running instance and exits, so clicking the
//! app icon brings the Capture window to the front instead of starting another copy.
//!
//! Unix: a listening socket in `$XDG_RUNTIME_DIR` (falls back to the temp dir). Windows: a
//! loopback TCP port derived from the user name.

use crossbeam_channel::Receiver;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request {
    /// Show and raise the Capture window.
    Show,
    /// Start a capture with the selected preset.
    Capture,
    /// Only find out whether an instance is running (a `--background` launch).
    Ping,
}

pub struct Guard {
    rx: Receiver<Request>,
    #[cfg(unix)]
    path: Option<std::path::PathBuf>,
}

impl Guard {
    pub fn poll(&self) -> Vec<Request> { self.rx.try_iter().collect() }

    pub fn disabled() -> Self {
        let (_tx, rx) = crossbeam_channel::unbounded();
        Self { rx, #[cfg(unix)] path: None }
    }
}

#[cfg(unix)]
impl Drop for Guard {
    fn drop(&mut self) { if let Some(p) = &self.path { let _ = std::fs::remove_file(p); } }
}

#[cfg(unix)]
fn socket_path() -> std::path::PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map(std::path::PathBuf::from).filter(|p| p.is_dir()).unwrap_or_else(std::env::temp_dir);
    dir.join(format!("linkshot-{}.sock", uid()))
}

#[cfg(unix)]
fn uid() -> u32 {
    unsafe extern "C" { fn getuid() -> u32; }
    // SAFETY: getuid has no preconditions and cannot fail.
    unsafe { getuid() }
}

/// Hands `request` to a running instance. Returns `true` when one was found (the caller should
/// exit).
#[cfg(unix)]
pub fn forward(request: Request) -> bool {
    use std::io::Write;
    let path = socket_path();
    match std::os::unix::net::UnixStream::connect(&path) {
        Ok(mut s) => {
            let _ = s.set_write_timeout(Some(std::time::Duration::from_millis(500)));
            let _ = s.write_all(match request { Request::Capture => b"capture\n".as_slice(), Request::Show => b"show\n", Request::Ping => b"ping\n" });
            true
        }
        Err(_) => false,
    }
}

#[cfg(windows)]
fn port() -> u16 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    std::env::var("USERNAME").unwrap_or_default().hash(&mut h);
    47_200 + (h.finish() % 300) as u16
}

#[cfg(windows)]
pub fn forward(request: Request) -> bool {
    use std::io::{Read, Write};
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port()));
    match std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(400)) {
        Ok(mut s) => {
            let _ = s.set_write_timeout(Some(std::time::Duration::from_millis(500)));
            let _ = s.set_read_timeout(Some(std::time::Duration::from_millis(500)));
            // Another program could own the port: only a reply that says "linkshot" counts.
            if s.write_all(b"linkshot?\n").is_err() { return false; }
            let mut hello = [0u8; 9];
            if s.read_exact(&mut hello).is_err() || &hello != b"linkshot\n" { return false; }
            let _ = s.write_all(match request { Request::Capture => b"capture\n".as_slice(), Request::Show => b"show\n", Request::Ping => b"ping\n" });
            true
        }
        Err(_) => false,
    }
}

#[cfg(not(any(unix, windows)))]
pub fn forward(_request: Request) -> bool { false }

/// Starts listening for requests from later launches. `wake` is called when one arrives.
#[cfg(unix)]
pub fn listen(wake: impl Fn() + Send + 'static) -> Guard {
    use std::io::Read;
    let (tx, rx) = crossbeam_channel::unbounded();
    let path = socket_path();
    // A stale socket from a crashed instance refuses connections (checked by `forward`), so it
    // is safe to replace here.
    let _ = std::fs::remove_file(&path);
    let listener = match std::os::unix::net::UnixListener::bind(&path) {
        Ok(l) => l,
        Err(e) => { tracing::warn!("single-instance socket {}: {e}", path.display()); return Guard { rx, path: None }; }
    };
    std::thread::Builder::new().name("single-instance".into()).spawn(move || {
        for stream in listener.incoming() {
            let Ok(s) = stream else { continue };
            let _ = s.set_read_timeout(Some(std::time::Duration::from_millis(500)));
            let mut buf = String::new();
            let _ = s.take(64).read_to_string(&mut buf);
            let req = match buf.trim() { "capture" => Request::Capture, "show" => Request::Show, _ => continue };
            // The UI loop may be asleep while the window is minimised: raise it from here.
            if req == Request::Show { crate::raise::raise_capture_window_now(); }
            if tx.send(req).is_err() { break; }
            wake();
        }
    }).ok();
    Guard { rx, path: Some(path) }
}

#[cfg(windows)]
pub fn listen(wake: impl Fn() + Send + 'static) -> Guard {
    use std::io::{BufRead, Write};
    let (tx, rx) = crossbeam_channel::unbounded();
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port()));
    let listener = match std::net::TcpListener::bind(addr) {
        Ok(l) => l,
        Err(e) => { tracing::warn!("single-instance port {}: {e}", addr.port()); return Guard { rx }; }
    };
    std::thread::Builder::new().name("single-instance".into()).spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut s) = stream else { continue };
            let _ = s.set_read_timeout(Some(std::time::Duration::from_millis(500)));
            let mut reader = std::io::BufReader::new(match s.try_clone() { Ok(c) => c, Err(_) => continue });
            let mut line = String::new();
            if reader.read_line(&mut line).is_err() || line.trim() != "linkshot?" { continue; }
            if s.write_all(b"linkshot\n").is_err() { continue; }
            line.clear();
            if reader.read_line(&mut line).is_err() { continue; }
            let req = match line.trim() { "capture" => Request::Capture, "show" => Request::Show, _ => continue };
            if tx.send(req).is_err() { break; }
            wake();
        }
    }).ok();
    Guard { rx }
}

#[cfg(not(any(unix, windows)))]
pub fn listen(_wake: impl Fn() + Send + 'static) -> Guard { Guard::disabled() }
