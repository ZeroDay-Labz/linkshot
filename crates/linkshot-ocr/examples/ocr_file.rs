//! `cargo run -p linkshot-ocr --example ocr_file -- <models-dir> <image>`
fn main() -> anyhow::Result<()> {
    let mut a = std::env::args().skip(1);
    let dir = std::path::PathBuf::from(a.next().expect("models dir"));
    let img = a.next().expect("image");
    if !linkshot_ocr::models_present(&dir) { linkshot_ocr::download_models(&dir, |d, t| eprint!("\r{d} / {t:?}"))?; eprintln!(); }
    let t = std::time::Instant::now();
    let ocr = linkshot_ocr::Ocr::load(&dir)?;
    eprintln!("models loaded in {:?}", t.elapsed());
    let img = image::open(&img)?.to_rgba8();
    let t = std::time::Instant::now();
    let res = ocr.recognize(&img)?;
    eprintln!("{} lines in {:?} (scale {})", res.lines.len(), t.elapsed(), res.scale);
    println!("{}", res.text());
    Ok(())
}
