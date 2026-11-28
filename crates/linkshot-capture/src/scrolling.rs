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
    /// Something the user should know that is not a failure: automatic scrolling slowed down after
    /// overshooting, or the end of what was scrolled to could not be joined.
    Warning(String),
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

/// Lazy-loading pages show an empty placeholder where new content is about to appear. When the
/// part of the view that scrolling just revealed (the bottom quarter, above any sticky footer) is
/// blank, re-grab for a little while until something appears. Returns the newest frame.
fn wait_for_lazy_content(region: Rect, frame: RgbaImage, footer: usize) -> RgbaImage {
    if !bottom_band_blank(&frame, footer) { return frame; }
    let t0 = Instant::now();
    let mut last = frame;
    while t0.elapsed() < Duration::from_millis(1500) {
        std::thread::sleep(Duration::from_millis(150));
        let Ok(next) = grab(region) else { break };
        let blank = bottom_band_blank(&next, footer);
        last = next;
        if !blank { return grab_settled(region, Duration::from_millis(600)).unwrap_or(last); }
    }
    last
}

/// True when every row of the bottom quarter (above `footer` rows) has (almost) no contrast.
fn bottom_band_blank(img: &RgbaImage, footer: usize) -> bool {
    let (w, h) = (img.width() as usize, img.height() as usize);
    if w < 16 || h < 32 || footer >= h / 2 { return false; }
    let y1 = h - footer;
    let y0 = y1.saturating_sub(h / 4);
    let (x0, x1) = (w * 2 / 100, w * 94 / 100);
    let raw = img.as_raw();
    (y0..y1).all(|y| {
        let (mut lo, mut hi) = (255u8, 0u8);
        for x in x0..x1 {
            let i = (y * w + x) * 4;
            let v = ((raw[i] as u32 * 77 + raw[i + 1] as u32 * 151 + raw[i + 2] as u32 * 28) >> 8) as u8;
            lo = lo.min(v); hi = hi.max(v);
        }
        hi - lo < 24
    })
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
    // Automatic scrolling recovery: whether anything has scrolled yet, and how far the "window does
    // not react" fallbacks have gone (0: wheel notches, 1: smooth pixel scrolling).
    let mut ever_scrolled = false;
    let mut pixel_mode = false;
    // Automatic step size (wheel notches, or pixels in smooth mode). Halved whenever a step jumps
    // past the overlap with the last frame, after scrolling back to it, so no content is skipped.
    let mut step = opts.notches.max(1);
    let mut px_step = opts.notches.max(1) as f64 * 100.0;
    let mut slowed_down = false;
    // Scrolling through the accessibility bus (exact steps, no pointer, no input permission), tried
    // first on KDE; `a11y_step` is in the scroll bar's units, calibrated from the measured offsets.
    #[cfg(target_os = "linux")]
    let mut a11y: Option<crate::a11y_scroll::A11yScroller> = None;
    #[cfg(target_os = "linux")]
    let mut a11y_tried = false;
    #[cfg(target_os = "linux")]
    let mut a11y_step = 0.0f64;
    // Set when switching from accessibility to wheel scrolling mid-capture: aim the pointer, but do
    // not scroll back to the top and restart.
    let mut keep_position = false;

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
            Ok(Command::SetAuto(a)) => { auto = a; auto_primed = false; #[cfg(target_os = "linux")] { a11y_tried = false; a11y = None; } unchanged_after_scroll = 0; pixel_mode = false; step = opts.notches.max(1); px_step = step as f64 * 100.0; }
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

        #[cfg(target_os = "linux")]
        if auto && !a11y_tried && !auto_primed {
            a11y_tried = true;
            let c = region.center();
            match crate::a11y_scroll::A11yScroller::find((c.x, c.y), opts.horizontal) {
                Ok(a) => { a11y_step = a.initial_step; a11y = Some(a); }
                Err(e) => tracing::info!("accessibility scrolling unavailable ({e:#}); using wheel events"),
            }
        }
        #[cfg(target_os = "linux")]
        if auto && a11y.is_some() {
            let a = a11y.as_ref().unwrap();
            if !auto_primed {
                if opts.scroll_from_top {
                    if a.to_start().is_ok() { std::thread::sleep(Duration::from_millis(150)); }
                    stitcher = Stitcher::new();
                }
                auto_primed = true;
                match grab_settled(region, Duration::from_millis(900)) { Ok(f) => { let p = stitcher.push(f); debug.record(&stitcher, p); last_push = Some(p); } Err(e) => { let _ = etx.send(Event::Error(format!("{e:#}"))); } }
            } else {
                let moved = match a.scroll_by(a11y_step) {
                    Ok(m) => m,
                    Err(e) => { tracing::info!("accessibility scrolling failed ({e:#}); using wheel events"); a11y = None; auto_primed = false; keep_position = true; continue; }
                };
                if moved == 0.0 {
                    // The scroll bar is at its end: so is the content.
                    let _ = etx.send(Event::AutoFinished);
                    auto = false;
                } else {
                    match grab_settled(region, Duration::from_millis(900)) {
                        Ok(f) => {
                            let f = if opts.horizontal { f } else { wait_for_lazy_content(region, f, stitcher.static_bands().1) };
                            let prev = stitcher.last_frame().cloned();
                            let p = stitcher.push(f.clone());
                            debug.record_pair(&stitcher, p, prev, f);
                            last_push = Some(p);
                            // The visible length in frame pixels, without the sticky bands.
                            let (st, sb) = stitcher.static_bands();
                            let view = stitcher.last_frame().map(|f| if opts.horizontal { f.width() as f64 } else { f.height() as f64 - (st + sb) as f64 }).unwrap_or(400.0).max(40.0);
                            match p {
                                Push::Scrolled { offset, .. } => {
                                    ever_scrolled = true;
                                    // Pixels per unit from this step; aim for 60 % of the view per step.
                                    let px_per_unit = offset.unsigned_abs() as f64 / moved.abs();
                                    if px_per_unit > 0.0 { a11y_step = (view * 0.6 / px_per_unit).max(0.01).min(a.range()); }
                                }
                                Push::NoChange if !ever_scrolled => {
                                    // The bar moved but the picture did not: not the bar of this view.
                                    tracing::info!("accessibility scrolling: the view did not follow its scroll bar; using wheel events");
                                    let _ = a.scroll_by(-moved);
                                    a11y = None; auto_primed = false; keep_position = true;
                                }
                                Push::NoMatch => {
                                    // Too far for an overlap: go back and take smaller steps.
                                    let _ = a.scroll_by(-moved);
                                    if let Ok(f) = grab_settled(region, Duration::from_millis(900)) { let p = stitcher.push(f); last_push = Some(p); }
                                    a11y_step /= 2.0;
                                    if a11y_step < 0.01 || stitcher.misses() > 3 {
                                        let _ = etx.send(Event::Warning("Lost the page. Scroll on by hand, or press Done.".into()));
                                        auto = false;
                                    }
                                }
                                _ => {}
                            }
                        }
                        Err(e) => { let _ = etx.send(Event::Error(format!("{e:#}"))); }
                    }
                }
            }
            let _ = etx.send(Event::Progress { frames: stitcher.frames, accepted: stitcher.accepted(), length: stitcher.length(), last: last_push, axis: stitcher.axis(), auto, misses: stitcher.misses() });
            continue;
        }

        if auto {
            if !ensure_scroller(&mut scroller, &etx) { auto = false; continue; }
            let s = scroller.as_mut().unwrap();
            let (dx, dy) = if opts.horizontal { (step, 0) } else { (0, step) };
            // One automatic step forwards (sign 1) or back (-1).
            let scroll_step = |s: &mut crate::scroll_input::Scroller, pixel_mode: bool, sign: i32| -> Result<()> {
                if pixel_mode {
                    let px = px_step * sign as f64;
                    s.scroll_pixels(if opts.horizontal { px } else { 0.0 }, if opts.horizontal { 0.0 } else { px })
                } else { s.scroll(dx * sign, dy * sign) }
            };
            if !auto_primed {
                // Wheel events go to the window under the pointer, which is still on the HUD button
                // the user just pressed: put the pointer in the middle of the region first.
                let c = region.center();
                match s.move_to(c.x, c.y) {
                    Ok(()) => tracing::info!("automatic scrolling: pointer at {:.0},{:.0} (region {:?})", c.x, c.y, region),
                    Err(e) => tracing::warn!("automatic scrolling: could not move the pointer into the region: {e:#}"),
                }
                std::thread::sleep(Duration::from_millis(120));
                // Scroll back to the start first, so the capture begins at the top/left edge.
                if opts.scroll_from_top && !keep_position {
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
                if keep_position { keep_position = false; let _ = etx.send(Event::Progress { frames: stitcher.frames, accepted: stitcher.accepted(), length: stitcher.length(), last: last_push, axis: stitcher.axis(), auto, misses: stitcher.misses() }); continue; }
                match grab(region) { Ok(f) => { let p = stitcher.push(f); debug.record(&stitcher, p); last_push = Some(p); } Err(e) => { let _ = etx.send(Event::Error(format!("{e:#}"))); } }
            } else {
                // Smooth (touchpad-style) scrolling sends about a wheel notch's worth of pixels per notch.
                if scroll_step(s, pixel_mode, 1).is_err() { auto = false; continue; }
                match grab_settled(region, Duration::from_millis(900)) {
                    Ok(f) => {
                        let f = if opts.horizontal { f } else { wait_for_lazy_content(region, f, stitcher.static_bands().1) };
                        let prev = stitcher.last_frame().cloned();
                        let p = stitcher.push(f.clone());
                        debug.record_pair(&stitcher, p, prev, f);
                        last_push = Some(p);
                        if matches!(p, Push::Scrolled { .. }) { ever_scrolled = true; }
                        if matches!(p, Push::NoChange) { unchanged_after_scroll += 1; } else { unchanged_after_scroll = 0; }
                        // Also before anything stitched: a first step that already jumps too far
                        // (a short region, fast acceleration) must back up too, not run on blindly.
                        if matches!(p, Push::NoMatch) {
                            // The step jumped past the last frame (scroll acceleration, a short
                            // viewport) or the content changed. Go back to where it overlapped and
                            // continue with smaller steps; give up only when the steps cannot shrink.
                            let can_shrink = if pixel_mode { px_step > 30.0 } else { step > 1 };
                            let _ = scroll_step(s, pixel_mode, -1);
                            if let Ok(f) = grab_settled(region, Duration::from_millis(900)) {
                                let prev = stitcher.last_frame().cloned();
                                let p = stitcher.push(f.clone());
                                debug.record_pair(&stitcher, p, prev, f);
                                last_push = Some(p);
                            }
                            if can_shrink {
                                if pixel_mode { px_step = (px_step / 2.0).max(25.0); } else { step = (step / 2).max(1); }
                                tracing::info!("automatic scrolling: overshot, stepping back; now {} per step", if pixel_mode { format!("{px_step} px") } else { format!("{step} notches") });
                                if !slowed_down { slowed_down = true; let _ = etx.send(Event::Warning("Slowing down so nothing is skipped".into())); }
                            } else if stitcher.misses() > 0 {
                                let _ = etx.send(Event::Warning("Lost the page. Scroll on by hand, or press Done.".into()));
                                auto = false;
                            }
                        }
                        if unchanged_after_scroll >= 2 {
                            if ever_scrolled {
                                // It scrolled before and stopped moving: the end of the content.
                                let _ = etx.send(Event::AutoFinished);
                                auto = false;
                            } else if !pixel_mode {
                                // Nothing has moved at all. Some applications ignore discrete wheel
                                // steps or lost the pointer: aim at the region again and switch to
                                // smooth scrolling before giving up.
                                tracing::info!("automatic scrolling: no movement with wheel steps, trying smooth scrolling");
                                let c = region.center();
                                let _ = s.move_to(c.x, c.y);
                                std::thread::sleep(Duration::from_millis(120));
                                pixel_mode = true;
                                unchanged_after_scroll = 0;
                            } else {
                                let _ = etx.send(Event::Error("This window did not scroll. Click inside it once, then press Auto-scroll again (or scroll by hand).".into()));
                                auto = false;
                                pixel_mode = false;
                                unchanged_after_scroll = 0;
                            }
                        }
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
    if !cancelled && stitcher.misses() > 0 && stitcher.accepted() > 0 {
        // The last frames could not be joined to the capture: what was on screen at the end is missing.
        let _ = etx.send(Event::Warning("The last part you scrolled to could not be joined and is not in the capture (it scrolled too far from the previous frame).".into()));
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
