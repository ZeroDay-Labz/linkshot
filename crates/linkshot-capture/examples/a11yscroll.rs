//! a11yscroll X Y [steps] (or `auto` for X Y): finds the scroll bar for the view at logical (X, Y) through AT-SPI and
//! scrolls it step by step, printing the value.
fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("linkshot_capture=debug").init();
    let a: Vec<String> = std::env::args().collect();
    // "auto": the centre of the topmost window.
    let (x, y): (f64, f64) = if a[1] == "auto" {
        let w = linkshot_capture::kwin::window_list()?.into_iter().min_by_key(|w| w.z_order).ok_or_else(|| anyhow::anyhow!("no windows"))?;
        println!("window {:?} at {},{} {}x{}", w.title, w.x, w.y, w.width, w.height);
        (w.x as f64 + w.width as f64 * 0.4, w.y as f64 + w.height as f64 * 0.5)
    } else { (a[1].parse()?, a[2].parse()?) };
    let steps: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(3);
    if std::env::var_os("DUMP").is_some() { return linkshot_capture::a11y_scroll::dump_tree((x, y), 14); }
    let t0 = std::time::Instant::now();
    let s = linkshot_capture::a11y_scroll::A11yScroller::find((x, y), false)?;
    println!("found in {} ms: {:?}", t0.elapsed().as_millis(), s.position()?);
    for _ in 0..steps { let moved = s.scroll_by(s.initial_step)?; std::thread::sleep(std::time::Duration::from_millis(300)); println!("moved={moved} {:?}", s.position()?); }
    Ok(())
}
