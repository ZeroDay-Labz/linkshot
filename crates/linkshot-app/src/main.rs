//! LinkShot — screen capture, recording and editing.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod capture_window;
mod editor;
mod headless;
mod hotkeys;
mod icons;
mod keepalive;
mod overlay;
mod raise;
mod recording_hud;

/// Window app id: the Flatpak id inside the sandbox (so the desktop matches our windows to the
/// exported desktop file and icon), `linkshot` otherwise.
pub fn app_id() -> String { std::env::var("FLATPAK_ID").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| "linkshot".to_string()) }
mod screen_draw;
mod live_sampler;
mod scrolling_hud;
mod grab_text;
mod share;
mod single_instance;
mod theme;
mod tray_icon;
mod widgets;

fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "linkshot=info,warn".into()))
        .init();
    // Unpack the embedded tool bundle (single-file builds) off the UI thread; lookups block on it only if used first.
    std::thread::spawn(linkshot_tools::prepare);
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--shots") {
        let dir = std::path::PathBuf::from(args.get(i + 1).cloned().unwrap_or_else(|| "shots".into()));
        let files: Vec<std::path::PathBuf> = args[i + 2..].iter().filter(|a| !a.starts_with("--")).map(std::path::PathBuf::from).collect();
        let dark = !args.iter().any(|a| a == "--light");
        if let Err(e) = headless::run_shots(&dir, &files, dark) { eprintln!("shots failed: {e:#}"); std::process::exit(1); }
        return Ok(());
    }
    if args.iter().any(|a| a == "--probe-kwin") {
        // Sandbox check: can KWin read our scripts and answer? (Flatpak: run with --command=linkshot.)
        #[cfg(target_os = "linux")]
        {
            println!("flatpak: {}  kde: {}  wayland: {}", std::env::var("FLATPAK_ID").unwrap_or_else(|_| "no".into()), linkshot_capture::kwin::is_kde(), linkshot_capture::is_wayland());
            if linkshot_capture::kwin::is_kde() && std::env::var_os("FLATPAK_ID").is_none() { let _ = linkshot_capture::kwin::ensure_authorization("LinkShot", "linkshot"); }
            println!("live backend: {:?}", linkshot_capture::live::probe());
            if linkshot_capture::kwin::is_kde() { match linkshot_capture::kwin::capture_area(0, 0, 2, 2, false) { Ok(i) => println!("KWin CaptureArea 2x2: ok {}x{}", i.width(), i.height()), Err(e) => println!("KWin CaptureArea 2x2 FAILED: {e:#}") } }
            match linkshot_capture::kwin::window_list() { Ok(w) => { println!("KWin window list: {} windows", w.len()); for x in w.iter().take(8) { println!("  {:>5},{:>5} {:>5}x{:<5} {}", x.x, x.y, x.width, x.height, x.title.chars().take(50).collect::<String>()); } } Err(e) => println!("KWin window list FAILED: {e:#}") }
            match linkshot_capture::kwin::cursor_pos() { Ok(p) => println!("KWin cursor: {p:?}"), Err(e) => println!("KWin cursor FAILED: {e:#}") }
        }
        return Ok(());
    }
    if args.iter().any(|a| a == "--probe-screencast") {
        #[cfg(target_os = "linux")]
        {
            println!("ScreenCast portal available: {}", linkshot_capture::screencast::available());
            let t0 = std::time::Instant::now();
            match linkshot_capture::screencast::grab_all(false) {
                Ok((streams, frames)) => { println!("{} streams, {} frames in {} ms", streams.len(), frames.len(), t0.elapsed().as_millis()); for s in &streams { println!("  node {} at {},{} {}x{} -> {:?}", s.node, s.x, s.y, s.width, s.height, frames.get(&s.node).map(|f| (f.width(), f.height()))); } }
                Err(e) => println!("grab FAILED: {e:#}"),
            }
        }
        return Ok(());
    }
    if args.iter().any(|a| a == "--selftest") {
        if let Err(e) = headless::run_selftest() { eprintln!("selftest FAILED: {e:#}"); std::process::exit(1); }
        return Ok(());
    }
    let background = args.iter().any(|a| a == "--background");
    // `--capture`: start a capture with the selected preset as soon as the app is up (hotkey daemons,
    // scripts and the self-test use this).
    let capture = args.iter().any(|a| a == "--capture");
    // Only one copy runs per user: a later launch asks the running one to show the Capture window
    // (or to capture) and exits.
    let request = if capture { single_instance::Request::Capture } else if background { single_instance::Request::Ping } else { single_instance::Request::Show };
    if single_instance::forward(request) { tracing::info!("linkshot is already running; request forwarded"); return Ok(()); }
    // The root window is a tiny invisible controller (see `keepalive.rs`); the Capture window,
    // the Editor and the overlays are child viewports. Minimising or hiding any of them therefore
    // never stops the event loop that handles hotkeys, tray clicks and timers.
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(keepalive::TITLE)
            .with_app_id(crate::app_id())
            .with_inner_size([2.0, 2.0])
            .with_min_inner_size([2.0, 2.0])
            .with_max_inner_size([2.0, 2.0])
            .with_position([-100.0, -100.0])
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(false)
            .with_taskbar(false)
            .with_active(false),
        // No vsync: with a swap interval of 1, Mesa's Wayland EGL makes `eglSwapBuffers` wait for
        // the frame callback of the surface being presented, and a minimised (or fully hidden)
        // window never receives one, which would hang the UI thread. egui repaints only on demand,
        // so nothing is lost.
        vsync: false,
        ..Default::default()
    };
    eframe::run_native("LinkShot", options, Box::new(move |cc| Ok(Box::new(app::App::new(cc, background || capture, capture)))))
}
