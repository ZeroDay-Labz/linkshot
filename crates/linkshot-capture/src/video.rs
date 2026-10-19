//! Screen recording: grabs a region at a steady frame rate on one thread and spools the frames
//! losslessly on another (see `linkshot_video::spool`). The UI drives it with [`Command`]s and
//! shows [`Event`]s; Shift+F9 (pause/resume) and Shift+F10 (stop) map onto the commands.

use anyhow::{Context, Result};
use crossbeam_channel::{bounded, unbounded, Receiver, Sender, TryRecvError};
use image::RgbaImage;
use linkshot_core::Rect;
use linkshot_video::spool::{SpoolInfo, SpoolWriter};
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct RecordOptions {
    /// Target frames per second (the grabber may deliver fewer on large regions).
    pub fps: u32,
    pub include_cursor: bool,
    /// Downscale frames so neither side exceeds this ("scale down to 1080p"); None = native.
    pub max_size: Option<(u32, u32)>,
    /// Where the spool is written.
    pub spool_path: PathBuf,
}

#[derive(Clone, Debug)]
pub enum Command { Pause, Resume, Stop, Cancel }

#[derive(Clone, Debug)]
pub enum Event {
    /// Periodic status: frames captured, frames dropped (spool could not keep up), recorded time.
    Progress { frames: u32, dropped: u32, elapsed_ms: u64, paused: bool, bytes: u64 },
    Error(String),
    /// Recording finished (`None` = cancelled or nothing captured).
    Done(Option<Recording>),
}

#[derive(Clone, Debug)]
pub struct Recording {
    pub spool: SpoolInfo,
    pub region: Rect,
    pub fps: u32,
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

fn run(region: Rect, opts: RecordOptions, crx: Receiver<Command>, etx: &Sender<Event>, wake: &(dyn Fn() + Send + Sync)) -> Result<Option<Recording>> {
    let fps = opts.fps.clamp(1, 60);
    let interval = Duration::from_micros(1_000_000 / fps as u64);
    // First frame decides the size; later frames are fitted to it.
    let first = linkshot_capture_frame(region, opts.include_cursor).context("first frame")?;
    let (tw, th) = target_size(first.width(), first.height(), opts.max_size);
    let first = fit(first, tw, th);
    let mut writer = SpoolWriter::create(&opts.spool_path, tw, th, fps)?;

    // Writer thread: compresses and spools frames so grabbing never waits on disk.
    let (ftx, frx) = bounded::<(u32, RgbaImage)>(4);
    let writer_thread = std::thread::Builder::new().name("linkshot-spool".into()).spawn(move || -> Result<SpoolWriter> {
        for (pts, img) in frx { writer.push(pts, img.as_raw())?; }
        Ok(writer)
    }).expect("spawn spool writer");

    let start = Instant::now();
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
                Ok(Command::Pause) => { if pause_started.is_none() { pause_started = Some(Instant::now()); } }
                Ok(Command::Resume) => { if let Some(p) = pause_started.take() { paused_total += p.elapsed(); next = Instant::now(); } }
                Ok(Command::Stop) => break 'outer,
                Ok(Command::Cancel) => { cancelled = true; break 'outer; }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => { cancelled = true; break 'outer; }
            }
        }
        let paused = pause_started.is_some();
        if last_progress.elapsed() >= Duration::from_millis(200) {
            last_progress = Instant::now();
            let elapsed = start.elapsed() - paused_total - pause_started.map(|p| p.elapsed()).unwrap_or_default();
            let _ = etx.send(Event::Progress { frames, dropped, elapsed_ms: elapsed.as_millis() as u64, paused, bytes: bytes_est });
            wake();
        }
        if paused { std::thread::sleep(Duration::from_millis(40)); continue; }
        let now = Instant::now();
        if now < next { std::thread::sleep((next - now).min(Duration::from_millis(50))); if Instant::now() < next { continue; } }
        next += interval;
        if next < Instant::now() { next = Instant::now(); } // fell behind: do not burst
        let img = match linkshot_capture_frame(region, opts.include_cursor) {
            Ok(i) => fit(i, tw, th),
            Err(e) => { tracing::warn!("frame grab failed: {e:#}"); std::thread::sleep(Duration::from_millis(100)); continue; }
        };
        let pts = (start.elapsed() - paused_total).as_millis() as u32;
        bytes_est += (tw as u64 * th as u64 * 4) / 20;
        match ftx.try_send((pts, img)) {
            Ok(()) => frames += 1,
            Err(crossbeam_channel::TrySendError::Full(_)) => dropped += 1,
            Err(crossbeam_channel::TrySendError::Disconnected(_)) => break,
        }
    }
    drop(ftx);
    let writer = match writer_thread.join() { Ok(Ok(w)) => w, Ok(Err(e)) => return Err(e.context("spool")), Err(_) => anyhow::bail!("spool writer panicked") };
    if cancelled || writer.frames() == 0 { writer.abort(); return Ok(None); }
    let spool = writer.finish()?;
    Ok(Some(Recording { spool, region, fps }))
}

fn linkshot_capture_frame(region: Rect, cursor: bool) -> Result<RgbaImage> { crate::capture_region_with_cursor(region, cursor) }
