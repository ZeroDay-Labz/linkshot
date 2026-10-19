//! `cargo run -p linkshot-audio --example audiotest [seconds]`: lists devices, records the default
//! microphone and the system monitor for a moment, mixes them and plays the result back.
use std::time::{Duration, Instant};
fn main() -> anyhow::Result<()> {
    let secs: f32 = std::env::args().nth(1).and_then(|a| a.parse().ok()).unwrap_or(1.5);
    println!("available: {}", linkshot_audio::available());
    for d in linkshot_audio::devices() { println!("  {:?} {:<60} {}{}", d.kind, d.id, d.name, if d.is_default { " (default)" } else { "" }); }
    let dir = std::env::temp_dir().join("linkshot-audiotest");
    std::fs::create_dir_all(&dir)?;
    let origin = Instant::now();
    let mic = linkshot_audio::Capture::start(&linkshot_audio::CaptureSpec { kind: linkshot_audio::Kind::Microphone, device: None }, &dir.join("mic.wav"), origin)?;
    let sys = linkshot_audio::Capture::start(&linkshot_audio::CaptureSpec { kind: linkshot_audio::Kind::SystemAudio, device: None }, &dir.join("sys.wav"), origin)?;
    let t0 = Instant::now();
    while t0.elapsed().as_secs_f32() < secs {
        std::thread::sleep(Duration::from_millis(250));
        if t0.elapsed().as_secs_f32() > secs / 2.0 { mic.set_muted(true); }
        println!("  mic {:.2}{} sys {:.2}", mic.level(), if mic.muted() { " (muted)" } else { "" }, sys.level());
    }
    let a = mic.stop()?; let b = sys.stop()?;
    println!("mic {} ms, sys {} ms (wanted ~{} ms since origin: {} ms)", a.duration_ms, b.duration_ms, (secs * 1000.0) as u32, origin.elapsed().as_millis());
    let out = dir.join("mix.wav");
    linkshot_audio::mix(&[a.path.clone(), b.path.clone()], &out)?;
    println!("mix {:?} {} ms", out, linkshot_audio::wav_duration_ms(&out).unwrap_or(0));
    let p = linkshot_audio::Playback::start(&out, 0, 1.0)?;
    std::thread::sleep(Duration::from_millis((secs * 1000.0) as u64 + 300));
    drop(p);
    println!("ok");
    Ok(())
}
