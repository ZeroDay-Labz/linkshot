//! A scrolling-capture session: grabs the selected region repeatedly, stitches the frames and
//! optionally drives the scrolling itself (Automatic Scrolling).

use crate::stitch::{Axis, Push, Stitcher};
use anyhow::Result;
use crossbeam_channel::{Receiver, Sender};
use image::RgbaImage;
use linkshot_core::geometry::Rect;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct ScrollOptions {
    /// Drive the scrolling with synthetic wheel events ("Automatic Scrolling").
    pub auto: bool,
    /// Scroll back to the top before starting (automatic mode).
    pub scroll_from_top: bool,
    pub horizontal: bool,
    /// Wheel notches per automatic step.
    pub notches: i32,
    /// Token for the Wayland RemoteDesktop portal from a previous run.
    pub restore_token: Option<String>,
}

impl Default for ScrollOptions {
    fn default() -> Self { Self { auto: false, scroll_from_top: true, horizontal: false, notches: 3, restore_token: None } }
}

#[derive(Clone, Debug)]
pub enum Event {
    /// Progress update after each grabbed frame.
    Progress { frames: u32, accepted: usize, length: u32, last: Option<Push>, axis: Option<Axis>, auto: bool, misses: u32 },
    /// Automatic scrolling reached the end of the content.
    AutoFinished,
    /// New portal restore token to persist.
    RestoreToken(String),
    Error(String),
    /// Final stitched image (None when cancelled or nothing was captured).
    Done(Option<RgbaImage>),
}

#[derive(Clone, Copy, Debug)]
pub enum Command {
    /// Begin grabbing (sessions start paused when the user must press Start).
    Start,
    SetAuto(bool),
    Finish,
    Cancel,
}

#[derive(Clone)]
pub struct CommandSender(Sender<Command>);
impl CommandSender { pub fn send(&self, c: Command) { let _ = self.0.send(c); } }

pub struct Session {
    pub region: Rect,
    pub events: Receiver<Event>,
    cmds: Sender<Command>,
}

impl Session {
    pub fn start(region: Rect, opts: ScrollOptions, paused: bool) -> Session {
        let (etx, erx) = crossbeam_channel::unbounded();
        let (ctx, crx) = crossbeam_channel::unbounded();
        std::thread::Builder::new().name("scrolling-capture".into()).spawn(move || run(region, opts, paused, etx, crx)).expect("spawn scrolling thread");
        Session { region, events: erx, cmds: ctx }
    }
    pub fn send(&self, c: Command) { let _ = self.cmds.send(c); }
    /// A handle that can send commands from another thread.
    pub fn cmds_clone(&self) -> CommandSender { CommandSender(self.cmds.clone()) }

    /// A session with no capture thread (headless rendering / tests): events can be injected with the
    /// returned sender.
    pub fn detached(region: Rect) -> (Session, Sender<Event>) {
        let (etx, erx) = crossbeam_channel::unbounded();
        let (ctx, _crx) = crossbeam_channel::unbounded();
        (Session { region, events: erx, cmds: ctx }, etx)
    }
}

fn grab(region: Rect) -> Result<RgbaImage> { crate::capture_region(region) }

/// Grabs until two consecutive frames are identical (smooth-scroll animation settled) or `max` elapses.
fn grab_settled(region: Rect, max: Duration) -> Result<RgbaImage> {
    let t0 = Instant::now();
    let mut last = grab(region)?;
    while t0.elapsed() < max {
        std::thread::sleep(Duration::from_millis(60));
        let next = grab(region)?;
        if crate::images_identical(&last, &next) { return Ok(next); }
        last = next;
    }
    Ok(last)
}

fn run(region: Rect, opts: ScrollOptions, paused: bool, etx: Sender<Event>, crx: Receiver<Command>) {
    let mut stitcher = Stitcher::new();
    let mut auto = opts.auto;
    let mut running = !paused;
    let mut scroller: Option<crate::scroll_input::Scroller> = None;
    let mut unchanged_after_scroll = 0u32;
    let mut auto_primed = false;
    let mut finished = false;
    let mut cancelled = false;
    let mut last_push: Option<Push> = None;
    let mut first_grab_done = false;
    let mut debug = SessionDebug::new();

    let ensure_scroller = |scroller: &mut Option<crate::scroll_input::Scroller>, etx: &Sender<Event>| -> bool {
        if scroller.is_some() { return true; }
        match crate::scroll_input::Scroller::new(opts.restore_token.clone()) {
            Ok(s) => { if let Some(t) = &s.restore_token { let _ = etx.send(Event::RestoreToken(t.clone())); } *scroller = Some(s); true }
            Err(e) => { let _ = etx.send(Event::Error(format!("Automatic scrolling unavailable: {e:#}"))); false }
        }
    };

    loop {
        // Commands.
        let timeout = if running { Duration::from_millis(if auto { 10 } else { 40 }) } else { Duration::from_millis(100) };
        match crx.recv_timeout(timeout) {
            Ok(Command::Start) => running = true,
            Ok(Command::SetAuto(a)) => { auto = a; auto_primed = false; unchanged_after_scroll = 0; }
            Ok(Command::Finish) => { finished = true; }
            Ok(Command::Cancel) => { cancelled = true; }
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => { cancelled = true; }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
        }
        if finished || cancelled { break; }
        if !running { continue; }
        if !first_grab_done {
            // The capture overlay (and the HUD) are still being faded out by the compositor when the
            // session starts; a frame grabbed now would blend them into the page. Wait for the screen
            // to settle before the first frame.
            std::thread::sleep(Duration::from_millis(350));
            match grab_settled(region, Duration::from_millis(1500)) {
                Ok(f) => { let p = stitcher.push(f); debug.record(&stitcher, p); last_push = Some(p); first_grab_done = true; }
                Err(e) => { let _ = etx.send(Event::Error(format!("{e:#}"))); std::thread::sleep(Duration::from_millis(300)); continue; }
            }
            let _ = etx.send(Event::Progress { frames: stitcher.frames, accepted: stitcher.accepted(), length: stitcher.length(), last: last_push, axis: stitcher.axis(), auto, misses: stitcher.misses() });
            continue;
        }

        if auto {
            if !ensure_scroller(&mut scroller, &etx) { auto = false; continue; }
            let s = scroller.as_mut().unwrap();
            let (dx, dy) = if opts.horizontal { (opts.notches, 0) } else { (0, opts.notches) };
            if !auto_primed {
                // Wheel events go to the window under the pointer, which is still on the HUD button
                // the user just pressed: put the pointer in the middle of the region first.
                let c = region.center();
                if let Err(e) = s.move_to(c.x, c.y) { tracing::warn!("move pointer: {e:#}"); }
                std::thread::sleep(Duration::from_millis(120));
                // Scroll back to the start first, so the capture begins at the top/left edge.
                if opts.scroll_from_top {
                    let mut prev = grab(region).ok();
                    for _ in 0..60 {
                        if s.scroll(-dx * 10, -dy * 10).is_err() { break; }
                        let cur = grab_settled(region, Duration::from_millis(400)).ok();
                        let same = matches!((&prev, &cur), (Some(a), Some(b)) if crate::images_identical(a, b));
                        prev = cur;
                        if same { break; }
                        if let Ok(c) = crx.try_recv() { if matches!(c, Command::Cancel) { cancelled = true; break; } if matches!(c, Command::Finish) { finished = true; break; } }
                    }
                    if finished || cancelled { break; }
                    stitcher = Stitcher::new();
                }
                auto_primed = true;
                match grab(region) { Ok(f) => { let p = stitcher.push(f); debug.record(&stitcher, p); last_push = Some(p); } Err(e) => { let _ = etx.send(Event::Error(format!("{e:#}"))); } }
            } else {
                if s.scroll(dx, dy).is_err() { auto = false; continue; }
                match grab_settled(region, Duration::from_millis(900)) {
                    Ok(f) => {
                        let prev = stitcher.last_frame().cloned();
                        let p = stitcher.push(f.clone());
                        debug.record_pair(&stitcher, p, prev, f);
                        last_push = Some(p);
                        if matches!(p, Push::NoChange) { unchanged_after_scroll += 1; } else { unchanged_after_scroll = 0; }
                        if unchanged_after_scroll >= 2 { let _ = etx.send(Event::AutoFinished); auto = false; }
                    }
                    Err(e) => { let _ = etx.send(Event::Error(format!("{e:#}"))); }
                }
            }
        } else {
            match grab(region) {
                Ok(f) => {
                    let prev = stitcher.last_frame().cloned();
                    let p = stitcher.push(f.clone());
                    debug.record_pair(&stitcher, p, prev, f);
                    last_push = Some(p);
                }
                Err(e) => { let _ = etx.send(Event::Error(format!("{e:#}"))); std::thread::sleep(Duration::from_millis(300)); }
            }
        }
        let _ = etx.send(Event::Progress { frames: stitcher.frames, accepted: stitcher.accepted(), length: stitcher.length(), last: last_push, axis: stitcher.axis(), auto, misses: stitcher.misses() });
    }
    let img = if cancelled { None } else { stitcher.finish() };
    debug.finish(&stitcher, img.as_ref());
    let _ = etx.send(Event::Done(img));
}

/// Diagnostics for a session: a per-frame log and, for the first few frames the stitcher could
/// not place, the rejected frame next to the frame it had to overlap. Written to
/// `<temp>/linkshot-scroll-debug/<timestamp>/` (PNG encoding happens on a helper thread so the
/// capture loop is not slowed down). The directory is mentioned in the log when something was
/// dropped, so a bad stitch can be analysed offline with `examples/restitch`.
struct SessionDebug {
    dir: std::path::PathBuf,
    t0: Instant,
    log: Vec<String>,
    dumped: u32,
    workers: Vec<std::thread::JoinHandle<()>>,
}

impl SessionDebug {
    const MAX_DUMPS: u32 = 8;

    fn new() -> Self {
        let stamp = chrono_free_stamp();
        let dir = std::env::temp_dir().join("linkshot-scroll-debug").join(stamp);
        Self { dir, t0: Instant::now(), log: Vec::new(), dumped: 0, workers: Vec::new() }
    }

    fn record(&mut self, st: &Stitcher, p: Push) {
        self.log.push(format!("{:8.3}s frame {:4} {:?} accepted={} length={} misses={}", self.t0.elapsed().as_secs_f64(), st.frames, p, st.accepted(), st.length(), st.misses()));
    }

    fn record_pair(&mut self, st: &Stitcher, p: Push, prev: Option<RgbaImage>, cur: RgbaImage) {
        self.record(st, p);
        // Frames are screen content: they only go to disk when diagnostics were asked for.
        if std::env::var_os("LINKSHOT_SCROLL_DEBUG").is_none() { return; }
        if !matches!(p, Push::NoMatch) || self.dumped >= Self::MAX_DUMPS { return; }
        self.dumped += 1;
        let n = self.dumped;
        let dir = self.dir.clone();
        let frame_no = st.frames;
        self.workers.push(std::thread::spawn(move || {
            let _ = std::fs::create_dir_all(&dir);
            if let Some(prev) = prev { let _ = prev.save(dir.join(format!("miss{n:02}-frame{frame_no:04}-prev.png"))); }
            let _ = cur.save(dir.join(format!("miss{n:02}-frame{frame_no:04}-cur.png")));
        }));
    }

    fn finish(mut self, st: &Stitcher, img: Option<&RgbaImage>) {
        self.log.push(format!("done: {} frames, {} accepted, {} px, axis {:?}, static bands {:?}", st.frames, st.accepted(), st.length(), st.axis(), st.static_bands()));
        let dropped = self.log.iter().filter(|l| l.contains("NoMatch")).count();
        if dropped > 0 { tracing::info!("scrolling capture: {dropped} frame(s) dropped (set LINKSHOT_SCROLL_DEBUG=1 to keep diagnostics)"); }
        if std::env::var_os("LINKSHOT_SCROLL_DEBUG").is_none() { return; }
        let _ = std::fs::create_dir_all(&self.dir);
        let _ = std::fs::write(self.dir.join("frames.log"), self.log.join("\n") + "\n");
        for w in self.workers.drain(..) { let _ = w.join(); }
        if let Some(img) = img { let _ = img.save(self.dir.join("result.png")); }
        tracing::info!("scrolling capture: {dropped} frame(s) dropped; diagnostics in {}", self.dir.display());
    }
}

fn chrono_free_stamp() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    format!("{secs}-{}", std::process::id())
}
