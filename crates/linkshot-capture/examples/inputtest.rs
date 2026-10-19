//! Checks synthetic input: moves the pointer to (x, y), scrolls, and reports whether the screen
//! region around the pointer changed.   inputtest <x> <y> [notches]
use linkshot_core::geometry::Rect;
fn main() -> anyhow::Result<()> {
    let _ = linkshot_capture::kwin::ensure_authorization("linkshot inputtest", "linkshot-inputtest");
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "debug".into())).with_writer(std::io::stderr).init();
    let a: Vec<String> = std::env::args().collect();
    let (x, y): (f64, f64) = (a[1].parse()?, a[2].parse()?);
    let notches: i32 = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(5);
    let token = { let home = std::env::var("HOME")?; let t = std::fs::read_to_string(format!("{home}/.config/linkshot/settings.json"))?; t.find("\"remote_desktop_token\"").map(|i| { let r = &t[i + 22..]; let q = r.find('"').unwrap() + 1; let e = r[q..].find('"').unwrap() + q; r[q..e].to_string() }) };
    println!("cursor before: {:?}", linkshot_capture::kwin::cursor_pos()?);
    let mut s = linkshot_capture::scroll_input::Scroller::new(token)?;
    s.move_to(x, y)?;
    std::thread::sleep(std::time::Duration::from_millis(200));
    println!("cursor after move: {:?}", linkshot_capture::kwin::cursor_pos()?);
    let region = Rect::from_xywh(x - 300.0, y - 200.0, 600.0, 400.0);
    let grab = || linkshot_capture::capture_region(region);
    let mut st = linkshot_capture::stitch::Stitcher::new();
    let first = grab()?; first.save("/tmp/claude-1000/-home-echo-RustroverProjects-linkshot/d8d94830-d142-4974-a135-c391f2cb933a/scratchpad/inputtest-before.png")?;
    st.push(first);
    let mut step = |label: &str, s: &mut linkshot_capture::scroll_input::Scroller, f: &dyn Fn(&mut linkshot_capture::scroll_input::Scroller) -> anyhow::Result<()>| -> anyhow::Result<()> {
        f(s)?;
        std::thread::sleep(std::time::Duration::from_millis(700));
        let r = st.push(grab()?);
        println!("{label}: {r:?}");
        Ok(())
    };
    step("discrete +30", &mut s, &|s| s.scroll(0, 30))?;
    step("discrete -30", &mut s, &|s| s.scroll(0, -30))?;
    step("discrete +3", &mut s, &|s| s.scroll(0, 3))?;
    step("discrete -3", &mut s, &|s| s.scroll(0, -3))?;
    step("pixels +300", &mut s, &|s| s.scroll_pixels(0.0, 300.0))?;
    step("pixels -300", &mut s, &|s| s.scroll_pixels(0.0, -300.0))?;
    step("pixels +60", &mut s, &|s| s.scroll_pixels(0.0, 60.0))?;
    step("pixels -60", &mut s, &|s| s.scroll_pixels(0.0, -60.0))?;
    Ok(())
}
