//! Replays frames dumped by a scrolling-capture session (see `scrolling::SessionDebug`) through the
//! stitcher with diagnostics on, to see why a frame was rejected.
//!
//!     cargo run -p linkshot-capture --example restitch -- <prev.png> <cur.png> [more frames…]
//!
//! The first image seeds the stitcher; each following image is pushed and the result printed.
fn main() -> anyhow::Result<()> {
    // SAFETY: single-threaded example, set before any other thread exists.
    unsafe { std::env::set_var("LINKSHOT_STITCH_DEBUG", "1"); }
    let files: Vec<String> = std::env::args().skip(1).collect();
    anyhow::ensure!(files.len() >= 2, "usage: restitch <prev.png> <cur.png> [more…]");
    let mut st = linkshot_capture::stitch::Stitcher::new();
    for f in &files {
        let img = image::open(f)?.to_rgba8();
        let r = st.push(img);
        println!("{f}: {r:?} (accepted={} length={} misses={} bands={:?})", st.accepted(), st.length(), st.misses(), st.static_bands());
    }
    if let Some(out) = st.finish() { out.save("restitch.png")?; println!("saved restitch.png ({}x{})", out.width(), out.height()); }
    Ok(())
}
