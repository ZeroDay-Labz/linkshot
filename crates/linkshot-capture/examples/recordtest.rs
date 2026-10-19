//! `cargo run -p linkshot-capture --example recordtest [seconds]`: records a 400×300 region of the
//! primary monitor with microphone + system audio and a synthetic webcam overlay, pauses and mutes
//! midway, then checks the spool and the mixed sound track and exports an MP4 with audio.
use linkshot_capture::video::{Command, Event, RecordOptions, Recorder, WebcamOptions};
use linkshot_capture::webcam::{Placement, Shape};
use linkshot_audio::{CaptureSpec, Kind};
use std::time::{Duration, Instant};

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    let secs: f32 = std::env::args().nth(1).and_then(|a| a.parse().ok()).unwrap_or(3.0);
    let mons = linkshot_capture::monitors()?;
    let m = mons.iter().find(|m| m.is_primary).or(mons.first()).expect("a monitor");
    let region = linkshot_core::Rect::from_xywh(m.x as f64 + 100.0, m.y as f64 + 100.0, 400.0, 300.0);
    let dir = std::env::temp_dir().join("linkshot-recordtest");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)?;
    #[cfg(target_os = "linux")]
    if linkshot_capture::is_wayland() && linkshot_capture::kwin::is_kde() {
        match linkshot_capture::kwin::ensure_authorization("linkshot rectest", "linkshot-rectest") { Ok(w) => println!("KWin ScreenShot2 authorisation {}", if w { "written" } else { "present" }), Err(e) => println!("authorisation: {e:#}") }
    }
    let spool = dir.join("rec.lsv");
    let opts = RecordOptions {
        fps: 30, include_cursor: true, max_size: None, spool_path: spool.clone(),
        audio: vec![CaptureSpec { kind: Kind::Microphone, device: None }, CaptureSpec { kind: Kind::SystemAudio, device: None }],
        mic_muted: false,
        webcam: Some(WebcamOptions { device: "lavfi:testsrc".into(), placement: Placement::BottomRight, shape: Shape::Circle, shown: true }),
    };
    let t0 = Instant::now();
    let rec = Recorder::start(region, opts, || {});
    let mut done = None;
    let mut paused = false;
    let mut muted = false;
    let mut swapped = false;
    let mut stopped = false;
    while done.is_none() {
        while let Ok(ev) = rec.events.try_recv() {
            match ev {
                Event::Progress { frames, dropped, elapsed_ms, audio, .. } => println!("  {elapsed_ms} ms: {frames} frames, {dropped} dropped, mic {:.2}{} sys {:.2} cam {}{}", audio.mic_level, if audio.mic_muted { " muted" } else { "" }, audio.system_level, audio.webcam_shown, if audio.webcam_fullscreen { " full" } else { "" }),
                Event::Error(e) => println!("  error: {e}"),
                Event::Done(r) => done = Some(r),
            }
        }
        let t = t0.elapsed().as_secs_f32();
        if t > secs * 0.3 && !muted { muted = true; rec.send(Command::SetMicMuted(true)); println!("-> mute"); }
        if t > secs * 0.45 && !paused { paused = true; rec.send(Command::Pause); println!("-> pause"); }
        if t > secs * 0.65 && paused && !swapped { swapped = true; rec.send(Command::Resume); rec.send(Command::SwapWebcam); println!("-> resume + swap webcam"); }
        if t > secs + 0.6 && !stopped { stopped = true; rec.send(Command::Stop); println!("-> stop"); }
        std::thread::sleep(Duration::from_millis(50));
    }
    let r = done.flatten().expect("a recording");
    println!("spool: {} frames {}x{} {} ms; audio: {:?}", r.spool.frames, r.spool.width, r.spool.height, r.spool.duration_ms, r.audio);
    let wav = r.audio.clone().expect("mixed audio");
    let wav_ms = linkshot_audio::wav_duration_ms(&wav).unwrap_or(0);
    println!("audio {wav_ms} ms vs video {} ms (the pause is excluded from both)", r.spool.duration_ms);
    anyhow::ensure!((wav_ms as i64 - r.spool.duration_ms as i64).abs() < 250, "audio/video length mismatch");
    let mp4 = dir.join("rec.mp4");
    let mut src = linkshot_video::SpoolReader::open(&spool)?;
    let rep = linkshot_video::export_mp4(&mut src, &linkshot_video::Mp4Options { audio: Some(wav), ..Default::default() }, &mp4, &mut |_| true)?;
    let probe = linkshot_video::ffmpeg::probe(&mp4)?;
    println!("mp4: {} frames, {} KB, {} -> probe {:?}", rep.frames, rep.bytes / 1024, rep.encoder, probe);
    anyhow::ensure!(probe.audio, "mp4 has no audio stream");
    // Dump a frame with the webcam overlay for a visual check.
    use linkshot_video::FrameSource as _;
    let i = src.index_at(src.duration_ms() / 4);
    let f = src.frame(i)?.to_vec();
    image::RgbaImage::from_raw(src.width(), src.height(), f).unwrap().save(dir.join("frame-pip.png"))?;
    let i = src.len() - 1;
    let f = src.frame(i)?.to_vec();
    image::RgbaImage::from_raw(src.width(), src.height(), f).unwrap().save(dir.join("frame-full.png"))?;
    println!("frames written to {}", dir.display());
    Ok(())
}
