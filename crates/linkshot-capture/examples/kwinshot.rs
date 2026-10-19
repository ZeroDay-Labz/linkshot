//! `kwinshot <output-name> <out.png>` — screenshot one output via KWin (authorises itself first).
fn main() -> anyhow::Result<()> {
    let mut a = std::env::args().skip(1);
    let name = a.next().expect("output name");
    let out = a.next().expect("output path");
    let _ = linkshot_capture::kwin::ensure_authorization("linkshot kwinshot", "linkshot-kwinshot");
    let img = linkshot_capture::kwin::capture_screen(&name, false)?;
    let small = image::imageops::resize(&img, img.width() / 4, img.height() / 4, image::imageops::FilterType::Triangle);
    small.save(&out)?;
    println!("{}x{} -> {out}", img.width(), img.height());
    Ok(())
}
