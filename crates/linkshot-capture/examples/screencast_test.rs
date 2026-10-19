//! Opens a ScreenCast portal session (one dialog), grabs every shared monitor twice and saves the
//! frames: `cargo run -p linkshot-capture --example screencast_test`.
fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    println!("portal available: {}", linkshot_capture::screencast::available());
    let dir = std::env::temp_dir().join("linkshot-screencast-test");
    std::fs::create_dir_all(&dir)?;
    for round in 0..2 {
        let t0 = std::time::Instant::now();
        let (streams, frames) = linkshot_capture::screencast::grab_all(false)?;
        println!("round {round}: {} streams, {} frames in {} ms", streams.len(), frames.len(), t0.elapsed().as_millis());
        for s in &streams {
            match frames.get(&s.node) {
                Some(f) => { println!("  node {} at {},{} {}x{} -> frame {}x{}", s.node, s.x, s.y, s.width, s.height, f.width(), f.height()); if round == 1 { f.save(dir.join(format!("node-{}.png", s.node)))?; } }
                None => println!("  node {} at {},{}: no frame", s.node, s.x, s.y),
            }
        }
    }
    if let Some(t) = linkshot_capture::screencast::take_new_tokens() { println!("restore token: {}", t.plain.map(|t| format!("{}…", &t[..t.len().min(12)])).unwrap_or_default()); }
    println!("frames in {}", dir.display());
    Ok(())
}
