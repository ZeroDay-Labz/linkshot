//! Prints the windows the capture overlay can offer for Window selection.
fn main() -> anyhow::Result<()> {
    let t0 = std::time::Instant::now();
    let ws = linkshot_capture::windows()?;
    println!("{} windows in {} ms (z 0 = top)", ws.len(), t0.elapsed().as_millis());
    for w in &ws { println!("  z{:<3} {:>5},{:>5} {:>5}x{:<5} {:<24} {}", w.z_order, w.x, w.y, w.width, w.height, w.app_name, w.title.chars().take(60).collect::<String>()); }
    Ok(())
}
