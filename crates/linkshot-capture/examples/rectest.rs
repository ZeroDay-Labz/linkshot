//! Records a screen region for a few seconds with the video recorder and exports it as GIF and MP4.
//! No input is injected; it only takes screenshots.
//!
//!     cargo run -p linkshot-capture --example rectest -- <x> <y> <w> <h> [--seconds N] [--fps N] [--out DIR]

use linkshot_capture::video::{Command, Event, RecordOptions, Recorder};
use linkshot_video::FrameSource as _;
use std::time::{Duration, Instant};

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into())).init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let num = |i: usize| -> f64 { args.get(i).and_then(|a| a.parse().ok()).expect("x y w h") };
    let region = linkshot_core::Rect::from_xywh(num(0), num(1), num(2), num(3));
    let opt = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned());
    let seconds: f64 = opt("--seconds").and_then(|s| s.parse().ok()).unwrap_or(4.0);
    let fps: u32 = opt("--fps").and_then(|s| s.parse().ok()).unwrap_or(30);
    let out = std::path::PathBuf::from(opt("--out").unwrap_or_else(|| std::env::temp_dir().join("linkshot-rectest").to_string_lossy().into_owned()));
    std::fs::create_dir_all(&out)?;
    #[cfg(target_os = "linux")]
    if linkshot_capture::is_wayland() && linkshot_capture::kwin::is_kde() {
        match linkshot_capture::kwin::ensure_authorization("linkshot rectest", "linkshot-rectest") { Ok(w) => println!("KWin ScreenShot2 authorisation {}", if w { "written" } else { "present" }), Err(e) => println!("authorisation: {e:#}") }
    }
    let spool = out.join("rectest.lsv");
    let t0 = Instant::now();
    let rec = Recorder::start(region, RecordOptions { fps, include_cursor: true, max_size: None, spool_path: spool.clone(), ..Default::default() }, || {});
    let mut stopped = false;
    let result = loop {
        if !stopped && t0.elapsed().as_secs_f64() >= seconds { rec.send(Command::Stop); stopped = true; }
        match rec.events.recv_timeout(Duration::from_millis(100)) {
            Ok(Event::Progress { frames, dropped, elapsed_ms, .. }) => println!("  {elapsed_ms:>6} ms  {frames} frames  {dropped} dropped"),
            Ok(Event::Error(e)) => println!("error: {e}"),
            Ok(Event::Done(r)) => break r,
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
            Err(_) => break None,
        }
    };
    let Some(r) = result else { anyhow::bail!("nothing recorded") };
    let info = &r.spool;
    println!("recorded {} frames {}x{} in {} ms, spool {} KB ({:.1} KB/frame)", info.frames, info.width, info.height, info.duration_ms, info.bytes / 1024, info.bytes as f64 / 1024.0 / info.frames.max(1) as f64);
    let mut src = linkshot_video::SpoolReader::open(&spool)?;
    let t = Instant::now();
    let gif = out.join("rectest.gif");
    let rep = linkshot_video::export_gif(&mut src, &linkshot_video::GifOptions::default(), &gif, &mut |_| true)?;
    println!("GIF  {}: {} frames written / {} sampled, {} KB, exact palette {}, {:.1} s", gif.display(), rep.frames_written, rep.frames_sampled, rep.bytes / 1024, rep.exact_palette, t.elapsed().as_secs_f32());
    match linkshot_video::ffmpeg::find_ffmpeg() {
        Some(_) => {
            let t = Instant::now();
            let mp4 = out.join("rectest.mp4");
            let rep = linkshot_video::export_mp4(&mut src, &linkshot_video::Mp4Options::default(), &mp4, &mut |_| true)?;
            println!("MP4  {}: {} frames, {} KB, {}, {:.1} s", mp4.display(), rep.frames, rep.bytes / 1024, rep.encoder, t.elapsed().as_secs_f32());
            println!("probe: {:?}", linkshot_video::ffmpeg::probe(&mp4)?);
        }
        None => println!("ffmpeg not found; MP4 skipped"),
    }
    // Save a middle frame for a visual check.
    let mid = src.len() / 2;
    let f = src.frame(mid)?.to_vec();
    image::RgbaImage::from_raw(src.width(), src.height(), f).unwrap().save(out.join("rectest-mid.png"))?;
    println!("middle frame saved to {}", out.join("rectest-mid.png").display());
    Ok(())
}
