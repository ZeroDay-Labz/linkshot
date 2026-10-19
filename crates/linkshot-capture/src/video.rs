//! Screen recording: grabs a region at a steady frame rate on one thread and spools the frames
//! losslessly on another (see `linkshot_video::spool`), while microphone / system-audio tracks are
//! captured alongside (see `linkshot_audio`) and an optional webcam is composited into the frames.
//! The UI drives it with [`Command`]s and shows [`Event`]s; Shift+F9 (pause/resume) and Shift+F10
//! (stop) map onto the commands.

use crate::webcam::{self, Placement, Shape, Webcam};
use anyhow::{Context, Result};
use crossbeam_channel::{bounded, unbounded, Receiver, Sender, TryRecvError};
use image::RgbaImage;
use linkshot_audio::{Capture as AudioCapture, CaptureSpec, Kind};
use linkshot_core::Rect;
use linkshot_video::spool::{SpoolInfo, SpoolWriter};
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct WebcamOptions {
    pub device: String,
    pub placement: Placement,
    pub shape: Shape,
    /// Composite from the first frame (otherwise the camera runs but stays hidden until toggled on).
    pub shown: bool,
}

#[derive(Clone, Debug)]
pub struct RecordOptions {
    /// Target frames per second (the grabber may deliver fewer on large regions).
    pub fps: u32,
    pub include_cursor: bool,
    /// Downscale frames so neither side exceeds this ("scale down to 1080p"); None = native.
    pub max_size: Option<(u32, u32)>,
    /// Where the spool is written. Audio goes next to it as `<stem>.wav`.
    pub spool_path: PathBuf,
    /// Audio tracks to capture (at most one microphone and one system-audio track).
    pub audio: Vec<CaptureSpec>,
    /// Start with the microphone muted.
    pub mic_muted: bool,
    pub webcam: Option<WebcamOptions>,
}

impl Default for RecordOptions {
    fn default() -> Self { Self { fps: 30, include_cursor: true, max_size: None, spool_path: PathBuf::new(), audio: vec![], mic_muted: false, webcam: None } }
}

#[derive(Clone, Debug)]
pub enum Command {
    Pause,
    Resume,
    Stop,
    Cancel,
    SetMicMuted(bool),
    /// Show / hide the webcam overlay.
    SetWebcam(bool),
    /// Swap the webcam between its corner and full screen (Ctrl+Shift+F in the toolbar).
    SwapWebcam,
}

#[derive(Clone, Debug)]
pub enum Event {
    /// Periodic status: frames captured, frames dropped (spool could not keep up), recorded time.
    Progress { frames: u32, dropped: u32, elapsed_ms: u64, paused: bool, bytes: u64, audio: AudioStatus },
    Error(String),
    /// Recording finished (`None` = cancelled or nothing captured).
    Done(Option<Recording>),
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AudioStatus {
    pub mic: bool,
    pub mic_muted: bool,
    pub mic_level: f32,
    pub system: bool,
    pub system_level: f32,
    pub webcam: bool,
    pub webcam_shown: bool,
    pub webcam_fullscreen: bool,
}

#[derive(Clone, Debug)]
pub struct Recording {
    pub spool: SpoolInfo,
    pub region: Rect,
    pub fps: u32,
    /// Mixed audio track (`<spool stem>.wav`), when any audio was recorded.
    pub audio: Option<PathBuf>,
}

pub struct Recorder {
    cmds: Sender<Command>,
    pub events: Receiver<Event>,
    pub region: Rect,
}

impl Recorder {
    /// Starts recording immediately. `wake` is called whenever an event is queued.
    pub fn start(region: Rect, opts: RecordOptions, wake: impl Fn() + Send + Sync + 'static) -> Recorder {
        let (cmds, crx) = unbounded::<Command>();
        let (etx, events) = unbounded::<Event>();
        let r = region;
        std::thread::Builder::new().name("linkshot-recorder".into()).spawn(move || {
            let res = run(r, opts, crx, &etx, &wake);
            match res {
                Ok(rec) => { let _ = etx.send(Event::Done(rec)); }
                Err(e) => { let _ = etx.send(Event::Error(format!("{e:#}"))); let _ = etx.send(Event::Done(None)); }
            }
            wake();
        }).expect("spawn recorder");
        Recorder { cmds, events, region }
    }

    pub fn send(&self, c: Command) { let _ = self.cmds.send(c); }
}

fn target_size(w: u32, h: u32, max: Option<(u32, u32)>) -> (u32, u32) {
    match max {
        Some((mw, mh)) if w > mw || h > mh => {
            let k = (mw as f64 / w as f64).min(mh as f64 / h as f64);
            (((w as f64 * k).round() as u32).max(2) & !1, ((h as f64 * k).round() as u32).max(2) & !1)
        }
        _ => (w, h),
    }
}

fn fit(img: RgbaImage, w: u32, h: u32) -> RgbaImage {
    if img.width() == w && img.height() == h { return img; }
    image::imageops::resize(&img, w, h, image::imageops::FilterType::Triangle)
}

/// Audio captures started for a recording.
struct Audio { mic: Option<AudioCapture>, system: Option<AudioCapture> }

impl Audio {
    /// Opens the requested tracks concurrently (each takes a few hundred ms to come up). A track that
    /// fails is reported and skipped so the video still records.
    fn start(specs: &[CaptureSpec], dir: &std::path::Path, stem: &str, origin: Instant, mic_muted: bool, etx: &Sender<Event>) -> Audio {
        let mut out = Audio { mic: None, system: None };
        let results: Vec<(Kind, Result<AudioCapture>)> = std::thread::scope(|s| {
            let handles: Vec<_> = specs.iter().map(|spec| {
                let path = dir.join(format!("{stem}.{}.wav", match spec.kind { Kind::Microphone => "mic", Kind::SystemAudio => "sys" }));
                s.spawn(move || (spec.kind, AudioCapture::start(spec, &path, origin)))
            }).collect();
            handles.into_iter().map(|h| h.join().unwrap_or_else(|_| (Kind::Microphone, Err(anyhow::anyhow!("audio thread panicked"))))).collect()
        });
        for (kind, res) in results {
            match res {
                Ok(c) => match kind { Kind::Microphone => { c.set_muted(mic_muted); out.mic = Some(c); } Kind::SystemAudio => out.system = Some(c) },
                Err(e) => { let _ = etx.send(Event::Error(format!("{}: {e:#}", match kind { Kind::Microphone => "Microphone", Kind::SystemAudio => "System audio" }))); }
            }
        }
        out
    }
    fn set_paused(&self, p: bool) { for c in [&self.mic, &self.system].into_iter().flatten() { c.set_paused(p); } }
    fn status(&self, webcam: Option<(bool, bool)>) -> AudioStatus {
        AudioStatus {
            mic: self.mic.is_some(),
            mic_muted: self.mic.as_ref().map(|c| c.muted()).unwrap_or(false),
            mic_level: self.mic.as_ref().map(|c| c.level()).unwrap_or(0.0),
            system: self.system.is_some(),
            system_level: self.system.as_ref().map(|c| c.level()).unwrap_or(0.0),
            webcam: webcam.is_some(),
            webcam_shown: webcam.map(|w| w.0).unwrap_or(false),
            webcam_fullscreen: webcam.map(|w| w.1).unwrap_or(false),
        }
    }
    /// Stops the tracks and mixes them into `out`. Returns the mixed path when anything was recorded.
    fn finish(self, out: &std::path::Path, etx: &Sender<Event>) -> Option<PathBuf> {
        let mut tracks = vec![];
        let captures: Vec<AudioCapture> = [self.mic, self.system].into_iter().flatten().collect();
        for c in &captures { c.request_stop(); }
        for c in captures {
            match c.stop() {
                Ok(t) if t.duration_ms > 0 => tracks.push(t.path),
                Ok(t) => { let _ = std::fs::remove_file(&t.path); }
                Err(e) => { let _ = etx.send(Event::Error(format!("audio track: {e:#}"))); }
            }
        }
        if tracks.is_empty() { return None; }
        match linkshot_audio::mix(&tracks, out) {
            Ok(()) => Some(out.to_path_buf()),
            Err(e) => {
                let _ = etx.send(Event::Error(format!("audio mix: {e:#}")));
                // Keep the first track rather than losing everything.
                if std::fs::rename(&tracks[0], out).is_ok() { for t in &tracks[1..] { let _ = std::fs::remove_file(t); } Some(out.to_path_buf()) } else { None }
            }
        }
    }
}

fn run(region: Rect, opts: RecordOptions, crx: Receiver<Command>, etx: &Sender<Event>, wake: &(dyn Fn() + Send + Sync)) -> Result<Option<Recording>> {
    let fps = opts.fps.clamp(1, 60);
    let interval = Duration::from_micros(1_000_000 / fps as u64);
    let stem = opts.spool_path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "recording".into());
    let dir = opts.spool_path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
    let audio_out = dir.join(format!("{stem}.wav"));

    // Webcam first (it needs a moment to deliver frames); a failure is reported, not fatal.
    let mut cam: Option<Webcam> = None;
    let mut cam_shown = false;
    let mut cam_placement = Placement::BottomRight;
    let mut cam_shape = Shape::Circle;
    if let Some(w) = &opts.webcam {
        match Webcam::start(&w.device, fps.max(15)) {
            Ok(c) => { cam = Some(c); cam_shown = w.shown; cam_placement = w.placement; cam_shape = w.shape; }
            Err(e) => { let _ = etx.send(Event::Error(format!("Webcam: {e:#}"))); }
        }
    }
    let mut corner_placement = if cam_placement == Placement::Fullscreen { Placement::BottomRight } else { cam_placement };

    // Audio and the clock origin: frame 0 and the audio tracks both start at `start`.
    let start = Instant::now();
    let audio = Audio::start(&opts.audio, &dir, &stem, start, opts.mic_muted, etx);

    // First frame decides the size; later frames are fitted to it.
    let first = linkshot_capture_frame(region, opts.include_cursor).context("first frame")?;
    let (tw, th) = target_size(first.width(), first.height(), opts.max_size);
    let mut first = fit(first, tw, th);
    if cam_shown { if let Some(f) = cam.as_ref().and_then(|c| c.latest()) { webcam::composite(&mut first, &f, cam_placement, cam_shape); } }
    let mut writer = SpoolWriter::create(&opts.spool_path, tw, th, fps)?;

    // Writer thread: compresses and spools frames so grabbing never waits on disk.
    let (ftx, frx) = bounded::<(u32, RgbaImage)>(4);
    let writer_thread = std::thread::Builder::new().name("linkshot-spool".into()).spawn(move || -> Result<SpoolWriter> {
        for (pts, img) in frx { writer.push(pts, img.as_raw())?; }
        Ok(writer)
    }).expect("spawn spool writer");

    let mut paused_total = Duration::ZERO;
    let mut pause_started: Option<Instant> = None;
    let mut frames = 0u32;
    let mut dropped = 0u32;
    let mut cancelled = false;
    let mut next = start;
    let mut last_progress = Instant::now();
    let mut bytes_est = 0u64;
    let _ = ftx.send((0, first));
    frames += 1;
    'outer: loop {
        // Commands.
        loop {
            match crx.try_recv() {
                Ok(Command::Pause) => { if pause_started.is_none() { pause_started = Some(Instant::now()); audio.set_paused(true); } }
                Ok(Command::Resume) => { if let Some(p) = pause_started.take() { paused_total += p.elapsed(); next = Instant::now(); audio.set_paused(false); } }
                Ok(Command::Stop) => break 'outer,
                Ok(Command::Cancel) => { cancelled = true; break 'outer; }
                Ok(Command::SetMicMuted(m)) => { if let Some(c) = &audio.mic { c.set_muted(m); } }
                Ok(Command::SetWebcam(on)) => { cam_shown = on && cam.is_some(); }
                Ok(Command::SwapWebcam) => {
                    if cam.is_some() {
                        cam_shown = true;
                        if cam_placement == Placement::Fullscreen { cam_placement = corner_placement; } else { corner_placement = cam_placement; cam_placement = Placement::Fullscreen; }
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => { cancelled = true; break 'outer; }
            }
        }
        let paused = pause_started.is_some();
        if last_progress.elapsed() >= Duration::from_millis(200) {
            last_progress = Instant::now();
            let elapsed = start.elapsed() - paused_total - pause_started.map(|p| p.elapsed()).unwrap_or_default();
            let status = audio.status(cam.as_ref().map(|_| (cam_shown, cam_placement == Placement::Fullscreen)));
            let _ = etx.send(Event::Progress { frames, dropped, elapsed_ms: elapsed.as_millis() as u64, paused, bytes: bytes_est, audio: status });
            if let Some(e) = cam.as_ref().and_then(|c| c.error()) { let _ = etx.send(Event::Error(format!("Webcam: {e}"))); cam = None; cam_shown = false; }
            wake();
        }
        if paused { std::thread::sleep(Duration::from_millis(40)); continue; }
        let now = Instant::now();
        if now < next { std::thread::sleep((next - now).min(Duration::from_millis(50))); if Instant::now() < next { continue; } }
        next += interval;
        if next < Instant::now() { next = Instant::now(); } // fell behind: do not burst
        let mut img = match linkshot_capture_frame(region, opts.include_cursor) {
            Ok(i) => fit(i, tw, th),
            Err(e) => { tracing::warn!("frame grab failed: {e:#}"); std::thread::sleep(Duration::from_millis(100)); continue; }
        };
        if cam_shown { if let Some(f) = cam.as_ref().and_then(|c| c.latest()) { webcam::composite(&mut img, &f, cam_placement, cam_shape); } }
        let pts = (start.elapsed() - paused_total).as_millis() as u32;
        bytes_est += (tw as u64 * th as u64 * 4) / 20;
        match ftx.try_send((pts, img)) {
            Ok(()) => frames += 1,
            Err(crossbeam_channel::TrySendError::Full(_)) => dropped += 1,
            Err(crossbeam_channel::TrySendError::Disconnected(_)) => break,
        }
    }
    drop(ftx);
    drop(cam);
    let audio_path = audio.finish(&audio_out, etx);
    let writer = match writer_thread.join() { Ok(Ok(w)) => w, Ok(Err(e)) => return Err(e.context("spool")), Err(_) => anyhow::bail!("spool writer panicked") };
    if cancelled || writer.frames() == 0 {
        writer.abort();
        if let Some(a) = audio_path { let _ = std::fs::remove_file(a); }
        return Ok(None);
    }
    let spool = writer.finish()?;
    Ok(Some(Recording { spool, region, fps, audio: audio_path }))
}

fn linkshot_capture_frame(region: Rect, cursor: bool) -> Result<RgbaImage> { crate::capture_region_with_cursor(region, cursor) }
