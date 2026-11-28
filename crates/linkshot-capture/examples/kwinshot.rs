//! `kwinshot <output-name> <out.png>` — screenshot one output via KWin (authorises itself first).
fn main() -> anyhow::Result<()> {
    let mut a = std::env::args().skip(1);
    let name = a.next().expect("output name");
    let out = a.next().expect("output path");
    let _ = linkshot_capture::kwin::ensure_authorization("linkshot kwinshot", "linkshot-kwinshot");
    // "area": the whole 1600×1000 area (nested test sessions with unknown output names).
    let img = if name == "area" { linkshot_capture::kwin::capture_area(0, 0, 1600, 1000, false)? } else { linkshot_capture::kwin::capture_screen(&name, false)? };
    let small = image::imageops::resize(&img, img.width() / 2, img.height() / 2, image::imageops::FilterType::Triangle);
    small.save(&out)?;
    println!("{}x{} -> {out}", img.width(), img.height());
    Ok(())
}
