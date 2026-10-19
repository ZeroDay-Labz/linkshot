//! A background player: owns a frame source, decodes frames on time and hands them to the UI.

use crate::source::FrameSource;
use crossbeam_channel::{bounded, unbounded, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub enum PlayerCmd {
    Play,
    Pause,
    /// Seek to a time (ms) and show that frame.
    Seek(u32),
    /// Step frames (±1).
    Step(i32),
    SetLoop(bool),
    Quit,
}

#[derive(Clone)]
pub struct PlayerFrame {
    pub index: usize,
    pub pts_ms: u32,
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<Vec<u8>>,
}

#[derive(Clone)]
pub enum PlayerEvent {
    Frame(PlayerFrame),
    Playing(bool),
    /// Reached the end (not looping).
    Ended,
    Error(String),
}

pub struct Player {
    tx: Sender<PlayerCmd>,
    pub rx: Receiver<PlayerEvent>,
    pub width: u32,
    pub height: u32,
    pub len: usize,
    pub duration_ms: u32,
    pub timestamps: Arc<Vec<u32>>,
}

impl Player {
    /// Starts the decoder thread; `wake` is called whenever an event is queued.
    pub fn spawn(mut src: Box<dyn FrameSource + Send>, wake: Arc<dyn Fn() + Send + Sync>) -> Player {
        let (tx, crx) = unbounded::<PlayerCmd>();
        let (etx, rx) = bounded::<PlayerEvent>(4);
        let (width, height, len, duration_ms) = (src.width(), src.height(), src.len(), src.duration_ms());
        let timestamps = Arc::new(src.timestamps());
        let ts = timestamps.clone();
        std::thread::Builder::new().name("linkshot-player".into()).spawn(move || {
            let mut cur = 0usize;
            let mut playing = false;
            let mut looping = false;
            let mut origin: Option<(Instant, u32)> = None;
            let emit = |i: usize, src: &mut Box<dyn FrameSource + Send>| -> Option<PlayerEvent> {
                match src.frame(i) {
                    Ok(f) => Some(PlayerEvent::Frame(PlayerFrame { index: i, pts_ms: ts[i], width, height, rgba: Arc::new(f.to_vec()) })),
                    Err(e) => Some(PlayerEvent::Error(format!("{e:#}"))),
                }
            };
            let send = |ev: PlayerEvent| { let _ = etx.try_send(ev); wake(); };
            if len > 0 { if let Some(ev) = emit(0, &mut src) { send(ev); } }
            loop {
                // Time until the next frame while playing.
                let timeout = if playing {
                    let (t0, p0) = origin.unwrap();
                    let now_ms = p0 as u64 + t0.elapsed().as_millis() as u64;
                    if cur + 1 < len {
                        let next = ts[cur + 1] as u64;
                        Duration::from_millis(next.saturating_sub(now_ms).max(1))
                    } else {
                        Duration::from_millis((duration_ms as u64).saturating_sub(now_ms).max(1))
                    }
                } else { Duration::from_millis(250) };
                let cmd = match crx.recv_timeout(timeout) {
                    Ok(c) => Some(c),
                    Err(RecvTimeoutError::Timeout) => None,
                    Err(RecvTimeoutError::Disconnected) => break,
                };
                match cmd {
                    Some(PlayerCmd::Quit) => break,
                    Some(PlayerCmd::SetLoop(l)) => looping = l,
                    Some(PlayerCmd::Play) => {
                        if len == 0 { continue; }
                        if cur + 1 >= len && ts[cur] as u64 + 1 >= duration_ms as u64 { cur = 0; if let Some(ev) = emit(0, &mut src) { send(ev); } }
                        playing = true;
                        origin = Some((Instant::now(), ts[cur]));
                        send(PlayerEvent::Playing(true));
                    }
                    Some(PlayerCmd::Pause) => { playing = false; origin = None; send(PlayerEvent::Playing(false)); }
                    Some(PlayerCmd::Seek(t)) => {
                        if len == 0 { continue; }
                        let i = src.index_at(t.min(duration_ms));
                        cur = i;
                        if playing { origin = Some((Instant::now(), ts[cur])); }
                        if let Some(ev) = emit(cur, &mut src) { send(ev); }
                    }
                    Some(PlayerCmd::Step(d)) => {
                        if len == 0 { continue; }
                        playing = false;
                        origin = None;
                        cur = (cur as i64 + d as i64).clamp(0, len as i64 - 1) as usize;
                        if let Some(ev) = emit(cur, &mut src) { send(ev); }
                        send(PlayerEvent::Playing(false));
                    }
                    None => {}
                }
                if playing {
                    let (t0, p0) = origin.unwrap();
                    let now_ms = (p0 as u64 + t0.elapsed().as_millis() as u64).min(u32::MAX as u64) as u32;
                    if now_ms >= duration_ms && cur + 1 >= len {
                        if looping { cur = 0; origin = Some((Instant::now(), 0)); if let Some(ev) = emit(0, &mut src) { send(ev); } }
                        else { playing = false; origin = None; send(PlayerEvent::Ended); }
                        continue;
                    }
                    let i = src.index_at(now_ms);
                    if i != cur {
                        cur = i;
                        if let Some(ev) = emit(cur, &mut src) { send(ev); }
                    }
                }
            }
        }).expect("spawn player");
        Player { tx, rx, width, height, len, duration_ms, timestamps }
    }

    pub fn send(&self, c: PlayerCmd) { let _ = self.tx.send(c); }
}

impl Drop for Player {
    fn drop(&mut self) { let _ = self.tx.send(PlayerCmd::Quit); }
}
