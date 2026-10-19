fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("debug").with_writer(std::io::stderr).init();
    eprintln!("is_wayland = {}", linkshot_capture::is_wayland());
    #[cfg(target_os = "linux")]
    if linkshot_capture::kwin::is_kde() { eprintln!("kwin auth file written: {:?}", linkshot_capture::kwin::ensure_authorization("linkshot (dev snap)", "linkshot-snap")); }
    let t = std::time::Instant::now();
    for m in linkshot_capture::monitors()? { println!("monitor {:?}", m); }
    let snap = linkshot_capture::snapshot()?;
    println!("snapshot: {} monitors, {} windows in {:?}", snap.monitors.len(), snap.windows.len(), t.elapsed());
    for (m, img) in snap.monitors.iter().zip(&snap.images) { println!("  {} {}x{} image {}x{}", m.name, m.width, m.height, img.width(), img.height()); }
    for w in snap.windows.iter().take(15) { println!("  window {:?} {} @{},{} {}x{}", w.title.chars().take(40).collect::<String>(), w.app_name, w.x, w.y, w.width, w.height); }
    let out = std::env::temp_dir().join("linkshot-snap-test.png");
    snap.images[0].save(&out)?;
    println!("saved {}", out.display());
    Ok(())
}
