//! Measures live region grabbing (what the scrolling capture uses) on the current desktop.
use linkshot_core::geometry::Rect;
use std::time::Instant;

fn main() -> anyhow::Result<()> {
    let _ = linkshot_capture::kwin::ensure_authorization("linkshot scrollgrab", "linkshot-scrollgrab");
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::from_default_env()).with_writer(std::io::stderr).init();
    let mons = linkshot_capture::monitors()?;
    for m in &mons { println!("monitor {} {}x{} at ({},{}) scale {}", m.name, m.width, m.height, m.x, m.y, m.scale_factor); }
    let m = mons.iter().find(|m| m.is_primary).or(mons.first()).unwrap();
    let region = Rect::from_xywh(m.x as f64 + 200.0, m.y as f64 + 150.0, 900.0, 600.0);
    let t = Instant::now();
    let first = linkshot_capture::capture_region(region)?;
    println!("capture_region {:?} -> {}x{} in {:?}", region, first.width(), first.height(), t.elapsed());
    let t = Instant::now();
    let n = 10;
    let mut same = 0;
    for _ in 0..n { let f = linkshot_capture::capture_region(region)?; if linkshot_capture::images_identical(&first, &f) { same += 1; } }
    println!("{n} grabs in {:?} ({:.0} ms/frame), {same} identical to the first", t.elapsed(), t.elapsed().as_secs_f64() * 1000.0 / n as f64);
    let nonblack = first.pixels().filter(|p| p[0] > 8 || p[1] > 8 || p[2] > 8).count();
    println!("non-black pixels: {} / {}", nonblack, first.width() * first.height());
    let t = Instant::now();
    let snap = linkshot_capture::snapshot()?;
    println!("snapshot of {} monitors in {:?}", snap.monitors.len(), t.elapsed());
    let crop = snap.crop(region).unwrap();
    println!("snapshot.crop -> {}x{} (scale {:.3})", crop.width(), crop.height(), snap.scale_at(region));
    first.save("/tmp/claude-1000/-home-echo-RustroverProjects-linkshot/d8d94830-d142-4974-a135-c391f2cb933a/scratchpad/grab.png")?;
    Ok(())
}
