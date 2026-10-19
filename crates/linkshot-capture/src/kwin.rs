//! KDE Plasma (KWin) screenshot backend via the `org.kde.KWin.ScreenShot2` D-Bus interface.
//!
//! KWin only answers callers whose desktop file declares
//! `X-KDE-DBUS-Restricted-Interfaces=org.kde.KWin.ScreenShot2`; [`ensure_authorization`] installs
//! such a desktop file for the running executable (this is how Spectacle is authorised too).

use anyhow::{Context, Result};
use image::RgbaImage;
use std::collections::HashMap;
use std::io::Read;
use std::os::fd::{AsFd, OwnedFd};
use zbus::zvariant::{OwnedValue, Value};

pub fn is_kde() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP").map(|d| d.to_uppercase().contains("KDE")).unwrap_or(false)
        || std::env::var("KDE_FULL_SESSION").is_ok()
}

fn pipe() -> Result<(OwnedFd, OwnedFd)> {
    let mut fds = [0i32; 2];
    // SAFETY: plain pipe(2) call with a valid out-array.
    let r = unsafe { libc_pipe(fds.as_mut_ptr()) };
    anyhow::ensure!(r == 0, "pipe() failed");
    // SAFETY: both descriptors were just created and are owned here.
    unsafe { Ok((OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1]))) }
}
use std::os::fd::FromRawFd;
unsafe extern "C" { #[link_name = "pipe"] fn libc_pipe(fds: *mut i32) -> i32; }

fn decode(reply: HashMap<String, OwnedValue>, mut data: Vec<u8>) -> Result<RgbaImage> {
    let get_u32 = |k: &str| -> Option<u32> { reply.get(k).and_then(|v| u32::try_from(v).ok()) };
    let width = get_u32("width").context("no width")?;
    let height = get_u32("height").context("no height")?;
    let stride = get_u32("stride").unwrap_or(width * 4) as usize;
    let format = get_u32("format").unwrap_or(5);
    anyhow::ensure!(data.len() >= stride * height as usize, "short screenshot data ({} < {})", data.len(), stride * height as usize);
    let mut out = RgbaImage::new(width, height);
    let bpp = match format { 4 | 5 | 6 | 13 | 16 | 17 | 18 | 19 => 4, 3 /* RGB888? */ => 3, _ => 4 };
    for y in 0..height as usize {
        let row = &mut data[y * stride..y * stride + width as usize * bpp];
        for x in 0..width as usize {
            let p = &row[x * bpp..x * bpp + bpp];
            // QImage ARGB32 is stored as 0xAARRGGBB in native (little) endian → bytes B,G,R,A.
            let (r, g, b, a) = match format {
                4 => (p[2], p[1], p[0], 255),                   // Format_RGB32
                5 => (p[2], p[1], p[0], p[3]),                  // Format_ARGB32
                6 => { let a = p[3] as u32; if a == 0 { (0, 0, 0, 0) } else { (((p[2] as u32 * 255) / a) as u8, ((p[1] as u32 * 255) / a) as u8, ((p[0] as u32 * 255) / a) as u8, a as u8) } } // premultiplied
                13 => (p[2], p[1], p[0], 255),                  // Format_RGBX8888? (treated as RGB)
                16 | 17 | 18 => (p[0], p[1], p[2], p[3]),       // RGBA8888 family
                19 => (p[0], p[1], p[2], 255),                  // RGBX8888
                _ => (p[2], p[1], p[0], 255),
            };
            out.put_pixel(x as u32, y as u32, image::Rgba([r, g, b, a]));
        }
    }
    Ok(out)
}

enum Args<'a> {
    None,
    Screen(&'a str),
    Area(i32, i32, u32, u32),
}

fn call(method: &str, args: Args<'_>, include_cursor: bool) -> Result<RgbaImage> {
    let t0 = std::time::Instant::now();
    let conn = zbus::blocking::Connection::session().context("session bus")?;
    let proxy = zbus::blocking::Proxy::new(&conn, "org.kde.KWin.ScreenShot2", "/org/kde/KWin/ScreenShot2", "org.kde.KWin.ScreenShot2").context("ScreenShot2 proxy")?;
    let t_conn = t0.elapsed();
    let (read_end, write_end) = pipe()?;
    let reply: HashMap<String, OwnedValue> = {
        let fd = zbus::zvariant::Fd::from(write_end.as_fd());
        let opts = options(include_cursor);
        let msg = match args {
            Args::None => proxy.call_method(method, &(opts, fd)),
            Args::Screen(name) => proxy.call_method(method, &(name, opts, fd)),
            Args::Area(x, y, w, h) => proxy.call_method(method, &(x, y, w, h, opts, fd)),
        }
        .map_err(|e| anyhow::anyhow!("KWin {method}: {e}"))?;
        msg.body().deserialize().context("decode reply")?
    };
    let t_call = t0.elapsed();
    drop(write_end);
    let mut data = Vec::new();
    let mut f = std::fs::File::from(read_end);
    f.read_to_end(&mut data).context("read screenshot pipe")?;
    let t_read = t0.elapsed();
    let img = decode(reply, data);
    tracing::debug!("KWin {method}: connect {:?}, call {:?}, read {:?}, decode {:?}", t_conn, t_call - t_conn, t_read - t_call, t0.elapsed() - t_read);
    img
}

fn options(include_cursor: bool) -> HashMap<&'static str, Value<'static>> {
    let mut o = HashMap::new();
    o.insert("include-cursor", Value::from(include_cursor));
    o.insert("include-decoration", Value::from(true));
    o.insert("native-resolution", Value::from(true));
    o
}

/// Captures one output by its connector name (e.g. `DP-1`) at native resolution.
pub fn capture_screen(name: &str, include_cursor: bool) -> Result<RgbaImage> {
    call("CaptureScreen", Args::Screen(name), include_cursor)
}

/// Captures the whole workspace (all outputs) in one image.
pub fn capture_workspace(include_cursor: bool) -> Result<RgbaImage> {
    call("CaptureWorkspace", Args::None, include_cursor)
}

/// Captures a rectangle in logical desktop coordinates (may span outputs).
pub fn capture_area(x: i32, y: i32, w: u32, h: u32, include_cursor: bool) -> Result<RgbaImage> {
    call("CaptureArea", Args::Area(x, y, w, h), include_cursor)
}

pub fn capture_active_window(include_cursor: bool) -> Result<RgbaImage> {
    call("CaptureActiveWindow", Args::None, include_cursor)
}

/// Writes `~/.local/share/applications/linkshot.desktop` naming the current executable so KWin
/// authorises the ScreenShot2 interface. Returns true when a file was written or updated.
pub fn ensure_authorization(app_name: &str, desktop_id: &str) -> Result<bool> {
    let exe = std::env::current_exe()?;
    let dir = std::env::var_os("XDG_DATA_HOME").map(std::path::PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".local/share"))).context("no home")?.join("applications");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{desktop_id}.desktop"));
    let icon = std::env::var("LINKSHOT_ICON").ok().unwrap_or_else(|| "camera-photo".into());
    let content = format!("[Desktop Entry]\nType=Application\nName={app_name}\nComment=Screen capture and editor\nExec={} %U\nTryExec={}\nIcon={icon}\nTerminal=false\nCategories=Graphics;Utility;\nStartupWMClass=linkshot\nX-KDE-DBUS-Restricted-Interfaces=org.kde.KWin.ScreenShot2\n", exe.display(), exe.display());
    if std::fs::read_to_string(&path).map(|c| c == content).unwrap_or(false) { return Ok(false); }
    std::fs::write(&path, content)?;
    // Refresh the KService cache so KWin sees the new desktop file.
    for tool in ["kbuildsycoca6", "kbuildsycoca5"] {
        if let Ok(mut c) = std::process::Command::new(tool).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn() { let _ = c.wait(); break; }
    }
    Ok(true)
}

/// A script loaded into KWin (`org.kde.kwin.Scripting`). Scripts run inside the compositor, so
/// they can do what Wayland clients cannot: move windows between outputs, position them, and
/// activate them regardless of focus-stealing prevention.
struct LoadedScript { conn: zbus::blocking::Connection, path: std::path::PathBuf, id: i32 }

impl LoadedScript {
    fn load(script: &str) -> Result<Self> {
        let dir = std::env::temp_dir().join(format!("linkshot-kwin-{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let path = dir.join(format!("script-{}.js", SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
        std::fs::write(&path, script)?;
        let path_s = path.to_string_lossy().into_owned();
        let conn = zbus::blocking::Connection::session().context("session bus")?;
        let scripting = zbus::blocking::Proxy::new(&conn, "org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting").context("KWin scripting proxy")?;
        let id: i32 = scripting.call_method("loadScript", &(path_s.as_str(),)).context("loadScript")?.body().deserialize().context("loadScript reply")?;
        tracing::debug!("KWin script {path_s} loaded as id {id}");
        if std::env::var_os("LINKSHOT_KWIN_DEBUG").is_some() { eprintln!("KWin script {path_s} loaded as id {id}"); }
        if id < 0 { let _ = std::fs::remove_file(&path); anyhow::bail!("KWin refused the script"); }
        Ok(Self { conn, path, id })
    }

    /// Queues the script for execution inside KWin (it runs asynchronously).
    fn run(&self) -> Result<()> {
        let script_proxy = zbus::blocking::Proxy::new(&self.conn, "org.kde.KWin", format!("/Scripting/Script{}", self.id), "org.kde.kwin.Script").context("script proxy")?;
        script_proxy.call_method("run", &()).map(|_| ()).context("run KWin script")
    }

    /// Unloads the script. Unloading before KWin got round to running it cancels it, so callers
    /// wait (for a report, or a moment) first.
    fn unload(self) {
        if let Ok(scripting) = zbus::blocking::Proxy::new(&self.conn, "org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting") {
            let _ = scripting.call_method("unloadScript", &(self.path.to_string_lossy().as_ref(),));
        }
        if std::env::var_os("LINKSHOT_KWIN_DEBUG").is_none() { let _ = std::fs::remove_file(&self.path); }
    }
}

/// Loads, runs and unloads a KWin script without waiting for a result (prefer [`run_probe`]).
pub fn run_script(script: &str) -> Result<()> {
    let loaded = LoadedScript::load(script)?;
    let run = loaded.run();
    std::thread::sleep(std::time::Duration::from_millis(150));
    loaded.unload();
    run
}

/// Runs `body` inside KWin with error reporting: the script can call `REPORT(text)` once, and any
/// exception is reported as `error: ...` (which becomes an `Err` here). KWin itself never tells a
/// client that a script failed, so every helper below goes through this.
pub fn run_probe(body: &str) -> Result<String> {
    use std::sync::{Arc, Mutex};
    static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let conn = zbus::blocking::Connection::session().context("session bus")?;
    let name = format!("org.linkshot.Probe{}x{}", std::process::id(), SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
    let result: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    struct Probe { out: Arc<Mutex<Option<String>>> }
    #[zbus::interface(name = "org.linkshot.Probe")]
    impl Probe { fn report(&self, text: String) { *self.out.lock().unwrap() = Some(text); } }
    conn.object_server().at("/probe", Probe { out: result.clone() }).context("serve probe")?;
    conn.request_name(name.as_str()).context("request probe name")?;
    let script = format!(r#"
function REPORT(text) {{ callDBus({name}, "/probe", "org.linkshot.Probe", "Report", String(text)); }}
try {{
{body}
}} catch (e) {{ REPORT("error: " + e + " (line " + e.lineNumber + ")"); }}
"#, name = js_str(&name));
    let loaded = LoadedScript::load(&script)?;
    let run = loaded.run();
    // KWin runs the script when it gets round to it (it can be busy mapping our windows at
    // start-up); unloading earlier would cancel it, so wait for the report first.
    let t0 = std::time::Instant::now();
    let text = loop {
        if let Some(t) = result.lock().unwrap().take() { break Some(t); }
        if t0.elapsed() > std::time::Duration::from_secs(3) || run.is_err() { break None; }
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    let path = loaded.path.clone();
    loaded.unload();
    let _ = conn.release_name(name.as_str());
    run?;
    let Some(text) = text else { tracing::warn!("KWin script {} gave no reply (keep it with LINKSHOT_KWIN_DEBUG=1)", path.display()); anyhow::bail!("no reply from KWin") };
    if let Some(err) = text.strip_prefix("error: ") { anyhow::bail!("KWin script: {err}"); }
    Ok(text)
}

/// Moves windows to specific outputs and makes them fullscreen using KWin's scripting interface.
/// Wayland offers no way for a client to choose the output of a toplevel, so the compositor has to
/// do it. `windows` maps a window caption to an output name (e.g. "DP-1").
pub fn place_fullscreen(windows: &[(String, String)]) -> Result<()> {
    anyhow::ensure!(!windows.is_empty(), "nothing to place");
    let entries: Vec<String> = windows.iter().map(|(cap, out)| format!("[{}, {}]", js_str(cap), js_str(out))).collect();
    let script = format!(r#"
const map = [{}];
for (const [cap, outName] of map) {{
    const out = workspace.screens.find(s => s.name == outName);
    if (!out) continue;
    for (const w of workspace.windowList()) {{
        if (w.caption != cap) continue;
        if (w.output != out) {{
            if (w.fullScreen) w.fullScreen = false;
            if (typeof workspace.sendClientToScreen === "function") {{ workspace.sendClientToScreen(w, out); }} else {{ w.output = out; }}
            const g = out.geometry;
            w.frameGeometry = {{ x: g.x, y: g.y, width: g.width, height: g.height }};
        }}
        w.fullScreen = true;
        w.keepAbove = true;
    }}
}}
REPORT("ok");
"#, entries.join(", "));
    run_probe(&script).map(|_| ())
}

/// Moves the window with `caption` so its top-left corner is at logical desktop position (x, y).
pub fn place_window(caption: &str, x: i32, y: i32) -> Result<()> {
    let script = format!(r#"
for (const w of workspace.windowList()) {{
    if (w.caption != {cap}) continue;
    const g = w.frameGeometry;
    w.frameGeometry = {{ x: {x}, y: {y}, width: g.width, height: g.height }};
    w.keepAbove = true;
}}
REPORT("ok");
"#, cap = js_str(caption));
    run_probe(&script).map(|_| ())
}

/// The pointer position in logical desktop coordinates (Wayland clients cannot ask for it; KWin can).
pub fn cursor_pos() -> Result<(f64, f64)> {
    let text = run_probe(r#"REPORT(workspace.cursorPos.x + "	" + workspace.cursorPos.y);"#)?;
    let mut it = text.trim().split('\t');
    let x: f64 = it.next().and_then(|v| v.parse().ok()).context("cursor x")?;
    let y: f64 = it.next().and_then(|v| v.parse().ok()).context("cursor y")?;
    Ok((x, y))
}

/// Moves and resizes the window(s) whose caption starts with `caption` (logical desktop coordinates).
pub fn set_geometry(caption: &str, x: i32, y: i32, w: i32, h: i32) -> Result<()> {
    let script = format!(r#"
for (const w of workspace.windowList()) {{
    if (!w.caption.startsWith({cap})) continue;
    if (w.fullScreen) w.fullScreen = false;
    // Cross-output moves need the output changed first.
    const target = workspace.screens.find(s => {{ const g = s.geometry; return {x} >= g.x && {x} < g.x + g.width && {y} >= g.y && {y} < g.y + g.height; }});
    if (target && w.output != target) {{ if (typeof workspace.sendClientToScreen === "function") {{ workspace.sendClientToScreen(w, target); }} else {{ w.output = target; }} }}
    w.frameGeometry = {{ x: {x}, y: {y}, width: {w}, height: {h} }};
}}
REPORT("ok");
"#, cap = js_str(caption));
    run_probe(&script).map(|_| ())
}

/// Makes every window whose caption starts with `caption` a frameless always-on-top overlay at the
/// given geometry (no taskbar/switcher entry), waiting up to ~3 s for the window to appear and
/// re-asking until KWin reports it in place. Used for the recording frame strips and toolbars.
pub fn setup_overlay_window(caption: &str, x: i32, y: i32, w: i32, h: i32) -> Result<()> {
    let script = format!(r#"
let n = 0;
for (const win of workspace.windowList()) {{
    if (!win.caption.startsWith({cap})) continue;
    n++;
    if (win.fullScreen) win.fullScreen = false;
    const target = workspace.screens.find(s => {{ const g = s.geometry; return {x} >= g.x && {x} < g.x + g.width && {y} >= g.y && {y} < g.y + g.height; }});
    if (target && win.output != target) {{ if (typeof workspace.sendClientToScreen === "function") {{ workspace.sendClientToScreen(win, target); }} else {{ win.output = target; }} }}
    win.skipTaskbar = true; win.skipSwitcher = true; win.skipPager = true;
    win.noBorder = true; win.keepAbove = true;
    win.frameGeometry = {{ x: {x}, y: {y}, width: {w}, height: {h} }};
}}
REPORT(String(n));
"#, cap = js_str(caption));
    let t0 = std::time::Instant::now();
    let mut asked = false;
    while t0.elapsed() < std::time::Duration::from_secs(3) {
        let now = window_placements(caption)?;
        match now.iter().find(|(c, _, _)| c.starts_with(caption)) {
            Some((_, _, (wx, wy, ww, wh))) if asked && (wx - x as f64).abs() < 2.0 && (wy - y as f64).abs() < 2.0 && (ww - w as f64).abs() < 2.0 && (wh - h as f64).abs() < 2.0 => return Ok(()),
            Some(_) => { run_probe(&script)?; asked = true; std::thread::sleep(std::time::Duration::from_millis(80)); }
            None => std::thread::sleep(std::time::Duration::from_millis(80)),
        }
    }
    anyhow::bail!("window {caption:?} could not be set up")
}

/// Raises and activates the window with `caption` (bypasses focus-stealing prevention).
pub fn activate_window(caption: &str) -> Result<()> {
    let script = format!(r#"
for (const w of workspace.windowList()) {{
    if (w.caption != {cap}) continue;
    w.minimized = false;
    if (typeof workspace.activeWindow !== "undefined") {{ workspace.activeWindow = w; }} else {{ workspace.activeClient = w; }}
}}
REPORT("ok");
"#, cap = js_str(caption));
    run_probe(&script).map(|_| ())
}

/// Like [`place_window`], but waits for the window to appear (up to ~2 s) and re-asks until KWin
/// reports it at the requested position.
pub fn place_window_retry(caption: &str, x: i32, y: i32) -> Result<()> {
    let t0 = std::time::Instant::now();
    let mut asked = false;
    while t0.elapsed() < std::time::Duration::from_secs(3) {
        let now = window_placements(caption)?;
        match now.iter().find(|(c, _, _)| c == caption) {
            Some((_, _, (wx, wy, _, _))) if asked && (wx - x as f64).abs() < 2.0 && (wy - y as f64).abs() < 2.0 => return Ok(()),
            Some(_) => { place_window(caption, x, y)?; asked = true; std::thread::sleep(std::time::Duration::from_millis(80)); }
            None => std::thread::sleep(std::time::Duration::from_millis(80)),
        }
    }
    anyhow::bail!("window {caption:?} could not be placed")
}

/// Like [`activate_window`], but waits (up to ~1.5 s) for the window to appear first.
pub fn activate_window_retry(caption: &str) -> Result<()> {
    for _ in 0..15 {
        if window_placements(caption).map(|v| v.iter().any(|(c, _, _)| c == caption)).unwrap_or(false) { return activate_window(caption); }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    anyhow::bail!("window {caption:?} did not appear")
}

/// Keeps asking KWin to put each window on its output until it reports them there (windows may not
/// exist yet when the first request is made). `windows` maps captions to output names.
pub fn ensure_fullscreen_placement(windows: &[(String, String)]) -> Result<()> {
    let prefix = common_prefix(windows.iter().map(|(c, _)| c.as_str()));
    let t0 = std::time::Instant::now();
    let mut placed_once = false;
    while t0.elapsed() < std::time::Duration::from_secs(5) {
        let now = window_placements(&prefix)?;
        let present = windows.iter().filter(|(c, _)| now.iter().any(|(nc, _, _)| nc == c)).count();
        let correct = windows.iter().filter(|(c, o)| now.iter().any(|(nc, no, _)| nc == c && no == o)).count();
        tracing::debug!("overlay placement: {present}/{} windows present, {correct} on the right output", windows.len());
        if correct == windows.len() && placed_once { return Ok(()); }
        if present == windows.len() { place_fullscreen(windows)?; placed_once = true; std::thread::sleep(std::time::Duration::from_millis(120)); }
        else { std::thread::sleep(std::time::Duration::from_millis(80)); }
    }
    anyhow::bail!("overlay windows were not all placed within 5 s")
}

fn common_prefix<'a>(mut it: impl Iterator<Item = &'a str>) -> String {
    let Some(first) = it.next() else { return String::new() };
    let mut p = first.to_string();
    for s in it { let n = p.chars().zip(s.chars()).take_while(|(a, b)| a == b).count(); p = p.chars().take(n).collect(); }
    p
}

/// Reports, for each window whose caption starts with `prefix`, its output name and logical frame
/// geometry — used by the self-test and for debugging placement.
/// What KWin knows about one of our windows.
#[derive(Clone, Debug)]
pub struct WindowState {
    pub caption: String,
    pub output: String,
    /// Frame geometry in logical desktop coordinates (x, y, width, height).
    pub geometry: (f64, f64, f64, f64),
    pub active: bool,
    pub minimized: bool,
}

/// Asks KWin for every window whose caption starts with `prefix`.
pub fn window_states(prefix: &str) -> Result<Vec<WindowState>> {
    let script = format!(r#"
let lines = [];
const active = (typeof workspace.activeWindow !== "undefined") ? workspace.activeWindow : workspace.activeClient;
for (const w of workspace.windowList()) {{
    if (!w.caption.startsWith({prefix})) continue;
    const g = w.frameGeometry;
    lines.push(w.caption + "\t" + (w.output ? w.output.name : "") + "\t" + g.x + "\t" + g.y + "\t" + g.width + "\t" + g.height + "\t" + (w === active ? 1 : 0) + "\t" + (w.minimized ? 1 : 0));
}}
REPORT(lines.join("\n"));
"#, prefix = js_str(prefix));
    let text = run_probe(&script)?;
    Ok(text.lines().filter(|l| !l.is_empty()).filter_map(|l| {
        let f: Vec<&str> = l.split('\t').collect();
        if f.len() < 8 { return None; }
        let n = |i: usize| f[i].parse::<f64>().ok();
        Some(WindowState { caption: f[0].to_string(), output: f[1].to_string(), geometry: (n(2)?, n(3)?, n(4)?, n(5)?), active: f[6] == "1", minimized: f[7] == "1" })
    }).collect())
}

/// Caption, output name and frame geometry of every window whose caption starts with `prefix`.
pub fn window_placements(prefix: &str) -> Result<Vec<(String, String, (f64, f64, f64, f64))>> {
    Ok(window_states(prefix)?.into_iter().map(|w| (w.caption, w.output, w.geometry)).collect())
}

/// Turns the window with `caption` into an invisible helper: 2×2 px in the bottom-right corner of
/// its output, below every other window, out of the taskbar, switcher and pager. If KWin gave it
/// focus, focus goes back to the window with `refocus`. Waits (up to ~3 s) for it to appear.
pub fn tame_helper_window(caption: &str, refocus: &str) -> Result<()> {
    let t0 = std::time::Instant::now();
    while t0.elapsed() < std::time::Duration::from_secs(3) {
        if window_states(caption)?.iter().any(|w| w.caption == caption) {
            let script = format!(r#"
const active = (typeof workspace.activeWindow !== "undefined") ? workspace.activeWindow : workspace.activeClient;
for (const w of workspace.windowList()) {{
    if (w.caption != {cap}) continue;
    w.skipTaskbar = true; w.skipSwitcher = true; w.skipPager = true; w.noBorder = true; w.keepBelow = true;
    const o = w.output ? w.output.geometry : workspace.virtualScreenGeometry;
    w.frameGeometry = {{ x: o.x + o.width - 2, y: o.y + o.height - 2, width: 2, height: 2 }};
    if (w === active) {{
        for (const r of workspace.windowList()) {{ if (r.caption == {refocus}) {{ if (typeof workspace.activeWindow !== "undefined") {{ workspace.activeWindow = r; }} else {{ workspace.activeClient = r; }} }} }}
    }}
}}
REPORT("ok");
"#, cap = js_str(caption), refocus = js_str(refocus));
            return run_probe(&script).map(|_| ());
        }
        std::thread::sleep(std::time::Duration::from_millis(80));
    }
    anyhow::bail!("window {caption:?} did not appear")
}

/// If the window with `caption` is active, activates the topmost other normal window instead.
pub fn focus_away_from(caption: &str) -> Result<()> {
    let script = format!(r#"
const active = (typeof workspace.activeWindow !== "undefined") ? workspace.activeWindow : workspace.activeClient;
if (active && active.caption == {cap}) {{
    const stack = workspace.stackingOrder;
    for (let i = stack.length - 1; i >= 0; i--) {{
        const w = stack[i];
        if (w === active || w.minimized || !w.normalWindow || w.skipTaskbar || !w.onCurrentDesktop) continue;
        if (typeof workspace.activeWindow !== "undefined") {{ workspace.activeWindow = w; }} else {{ workspace.activeClient = w; }}
        break;
    }}
}}
REPORT("ok");
"#, cap = js_str(caption));
    run_probe(&script).map(|_| ())
}

/// Asks the window with `caption` to close (the client receives a close request).
pub fn close_window(caption: &str) -> Result<()> {
    let script = format!(r#"
for (const w of workspace.windowList()) {{ if (w.caption == {cap}) w.closeWindow(); }}
REPORT("ok");
"#, cap = js_str(caption));
    run_probe(&script).map(|_| ())
}

/// Restores (un-minimises) the window with `caption` without changing which window is active.
pub fn unminimize_window(caption: &str) -> Result<()> {
    let script = format!(r#"
for (const w of workspace.windowList()) {{ if (w.caption == {cap}) w.minimized = false; }}
REPORT("ok");
"#, cap = js_str(caption));
    run_probe(&script).map(|_| ())
}

/// Minimises the window with `caption` (used by tests and the `kwinplace` example).
pub fn minimize_window(caption: &str) -> Result<()> {
    let script = format!(r#"
for (const w of workspace.windowList()) {{ if (w.caption == {cap}) w.minimized = true; }}
REPORT("ok");
"#, cap = js_str(caption));
    run_probe(&script).map(|_| ())
}

fn js_str(s: &str) -> String {
    let mut out = String::from("\"");
    for ch in s.chars() { match ch { '\"' => out.push_str("\\\""), '\\' => out.push_str("\\\\"), '\n' => out.push_str("\\n"), c => out.push(c) } }
    out.push('"');
    out
}
