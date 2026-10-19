//! Audio for recordings: microphone and system-audio ("what you hear") capture into WAV tracks,
//! mixing the tracks into one file with the bundled `ffmpeg`, and playback of a track for the
//! Editor's player.
//!
//! Every track is 48 kHz, 16-bit, stereo (`s16le`). On Linux the capture and playback devices are
//! driven through the bundled `ffmpeg` (`-f pulse`, which PipeWire and PulseAudio both serve); on
//! Windows through WASAPI (loopback for system audio). The [`Capture`] keeps its track in sync
//! with a video recording by padding the start with silence until the first samples arrive and by
//! dropping samples while the recording is paused.

use anyhow::{Context, Result};
use std::fs::File;
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux as backend;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as backend;
#[cfg(not(any(target_os = "linux", windows)))]
mod backend {
    use super::*;
    pub fn available() -> bool { false }
    pub fn devices() -> Vec<Device> { vec![] }
    pub fn open(_spec: &CaptureSpec) -> Result<Box<dyn Source>> { anyhow::bail!("audio capture is not supported on this platform") }
    pub struct Playback;
    impl Playback { pub fn start(_path: &Path, _start_ms: u32, _volume: f32) -> Result<Self> { anyhow::bail!("audio playback is not supported on this platform") } }
}

pub const SAMPLE_RATE: u32 = 48_000;
pub const CHANNELS: u16 = 2;
pub const BYTES_PER_FRAME: usize = (CHANNELS as usize) * 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind { Microphone, SystemAudio }

/// An input the user can pick. `id` is what the backend needs (a PulseAudio source name, a WASAPI
/// endpoint id); `name` is for the UI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub kind: Kind,
    pub is_default: bool,
}

/// True when the platform backend can capture at all (ffmpeg present on Linux, always on Windows).
pub fn available() -> bool { backend::available() }

/// Every input device, default devices first, microphones before monitors.
pub fn devices() -> Vec<Device> {
    let mut v = backend::devices();
    v.sort_by_key(|d| (d.kind != Kind::Microphone, !d.is_default));
    v
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaptureSpec {
    pub kind: Kind,
    /// Device id, or `None` for the default device of that kind.
    pub device: Option<String>,
}

/// A raw PCM source (`s16le`, 48 kHz, stereo) that blocks until data is available and returns 0
/// bytes once stopped.
pub trait Source: Send {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize>;
    /// Something that unblocks a pending `read` and ends the stream; callable from another thread.
    fn stopper(&self) -> Box<dyn Fn() + Send + Sync>;
}

#[derive(Clone, Debug)]
pub struct TrackInfo {
    pub path: PathBuf,
    pub duration_ms: u32,
    pub kind: Kind,
}

/// A running capture writing one WAV track.
pub struct Capture {
    pub kind: Kind,
    path: PathBuf,
    muted: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    level: Arc<AtomicU32>,
    error: Arc<Mutex<Option<String>>>,
    stop: Arc<Mutex<Option<Box<dyn Fn() + Send + Sync>>>>,
    stopped: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<Result<u64>>>,
}

impl Capture {
    /// Starts capturing into `path`. Audio time zero is `origin` (the recording's clock origin):
    /// silence is written for the time that passes before the first samples arrive. Fails when the
    /// device cannot be opened (waits up to 4 s for the backend to report).
    pub fn start(spec: &CaptureSpec, path: &Path, origin: Instant) -> Result<Capture> {
        if let Some(p) = path.parent() { std::fs::create_dir_all(p).ok(); }
        let muted = Arc::new(AtomicBool::new(false));
        let paused = Arc::new(AtomicBool::new(false));
        let level = Arc::new(AtomicU32::new(0));
        let error = Arc::new(Mutex::new(None));
        let stop: Arc<Mutex<Option<Box<dyn Fn() + Send + Sync>>>> = Arc::new(Mutex::new(None));
        let stopped = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<()>>();
        let kind = spec.kind;
        let spec = spec.clone();
        let path_t = path.to_path_buf();
        let (m, p, l, e, s, st) = (muted.clone(), paused.clone(), level.clone(), error.clone(), stop.clone(), stopped.clone());
        let thread = std::thread::Builder::new().name(format!("linkshot-audio-{:?}", spec.kind)).spawn(move || -> Result<u64> {
            let mut src = match backend::open(&spec) {
                Ok(src) => { let _ = ready_tx.send(Ok(())); src }
                Err(err) => { let _ = ready_tx.send(Err(anyhow::anyhow!("{err:#}"))); return Err(err); }
            };
            *s.lock().unwrap() = Some(src.stopper());
            if st.load(Ordering::SeqCst) { if let Some(f) = s.lock().unwrap().as_ref() { f(); } }
            let res = capture_loop(&mut *src, &path_t, origin, &m, &p, &l);
            if let Err(err) = &res { *e.lock().unwrap() = Some(format!("{err:#}")); }
            res
        }).context("spawn audio thread")?;
        match ready_rx.recv_timeout(Duration::from_secs(4)) {
            Ok(Ok(())) => {}
            Ok(Err(err)) => { let _ = thread.join(); return Err(err); }
            Err(_) => { stopped.store(true, Ordering::SeqCst); anyhow::bail!("the audio device did not answer"); }
        }
        Ok(Capture { kind, path: path.to_path_buf(), muted, paused, level, error, stop, stopped, thread: Some(thread) })
    }

    pub fn path(&self) -> &Path { &self.path }
    pub fn set_muted(&self, muted: bool) { self.muted.store(muted, Ordering::Relaxed); }
    pub fn muted(&self) -> bool { self.muted.load(Ordering::Relaxed) }
    pub fn set_paused(&self, paused: bool) { self.paused.store(paused, Ordering::Relaxed); }
    /// Peak level of the most recent chunk, 0..=1 (measured before muting, so a muted mic still shows activity).
    pub fn level(&self) -> f32 { f32::from_bits(self.level.load(Ordering::Relaxed)) }
    pub fn error(&self) -> Option<String> { self.error.lock().unwrap().clone() }

    /// Asks the capture to end without waiting (so several tracks stop at the same moment).
    pub fn request_stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        if let Some(f) = self.stop.lock().unwrap().as_ref() { f(); }
    }

    /// Ends the capture and finalises the WAV file.
    pub fn stop(mut self) -> Result<TrackInfo> {
        self.request_stop();
        let bytes = match self.thread.take() {
            Some(t) => t.join().map_err(|_| anyhow::anyhow!("audio thread panicked"))??,
            None => 0,
        };
        let duration_ms = (bytes / BYTES_PER_FRAME as u64 * 1000 / SAMPLE_RATE as u64) as u32;
        Ok(TrackInfo { path: self.path.clone(), duration_ms, kind: self.kind })
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::SeqCst);
        if let Some(f) = self.stop.lock().unwrap().as_ref() { f(); }
        if let Some(t) = self.thread.take() { let _ = t.join(); }
    }
}

fn capture_loop(src: &mut dyn Source, path: &Path, origin: Instant, muted: &AtomicBool, paused: &AtomicBool, level: &AtomicU32) -> Result<u64> {
    let mut wav = WavWriter::create(path)?;
    let mut buf = vec![0u8; 32 * 1024];
    let mut zeros = vec![0u8; 32 * 1024];
    let mut first = true;
    loop {
        let n = src.read(&mut buf)?;
        if n == 0 { break; }
        let n = n - n % BYTES_PER_FRAME;
        if n == 0 { continue; }
        let chunk = &buf[..n];
        if first {
            first = false;
            // The chunk ends "now": pad up to its start.
            let chunk_dur = Duration::from_secs_f64(n as f64 / BYTES_PER_FRAME as f64 / SAMPLE_RATE as f64);
            let elapsed = origin.elapsed();
            if elapsed > chunk_dur && !paused.load(Ordering::Relaxed) {
                let pad_frames = ((elapsed - chunk_dur).as_secs_f64() * SAMPLE_RATE as f64) as usize;
                let mut left = pad_frames * BYTES_PER_FRAME;
                while left > 0 { let k = left.min(zeros.len()); wav.write(&zeros[..k])?; left -= k; }
            }
        }
        level.store(peak(chunk).to_bits(), Ordering::Relaxed);
        if paused.load(Ordering::Relaxed) { continue; }
        if muted.load(Ordering::Relaxed) {
            if zeros.len() < n { zeros.resize(n, 0); }
            wav.write(&zeros[..n])?;
        } else {
            wav.write(chunk)?;
        }
    }
    wav.finish()
}

/// Peak absolute sample value of an `s16le` chunk, 0..=1.
pub fn peak(chunk: &[u8]) -> f32 {
    let mut max = 0i32;
    for s in chunk.chunks_exact(2) {
        let v = i16::from_le_bytes([s[0], s[1]]) as i32;
        max = max.max(v.abs());
    }
    (max as f32 / 32768.0).min(1.0)
}

// ------------------------------------------------------------------ WAV

/// A minimal PCM WAV writer (header patched on `finish`).
pub struct WavWriter { out: BufWriter<File>, data: u64 }

impl WavWriter {
    pub fn create(path: &Path) -> Result<Self> {
        let file = File::create(path).with_context(|| format!("create {}", path.display()))?;
        let mut out = BufWriter::with_capacity(1 << 16, file);
        out.write_all(&wav_header(0))?;
        Ok(Self { out, data: 0 })
    }
    pub fn write(&mut self, pcm: &[u8]) -> Result<()> { self.out.write_all(pcm)?; self.data += pcm.len() as u64; Ok(()) }
    pub fn bytes(&self) -> u64 { self.data }
    /// Patches the sizes and returns the number of data bytes.
    pub fn finish(mut self) -> Result<u64> {
        self.out.flush()?;
        let mut f = self.out.into_inner().map_err(|e| anyhow::anyhow!("flush wav: {e}"))?;
        f.seek(SeekFrom::Start(0))?;
        f.write_all(&wav_header(self.data.min(u32::MAX as u64 - 44) as u32))?;
        f.flush()?;
        Ok(self.data)
    }
}

fn wav_header(data_len: u32) -> [u8; 44] {
    let mut h = [0u8; 44];
    h[0..4].copy_from_slice(b"RIFF");
    h[4..8].copy_from_slice(&(36 + data_len).to_le_bytes());
    h[8..12].copy_from_slice(b"WAVE");
    h[12..16].copy_from_slice(b"fmt ");
    h[16..20].copy_from_slice(&16u32.to_le_bytes());
    h[20..22].copy_from_slice(&1u16.to_le_bytes()); // PCM
    h[22..24].copy_from_slice(&CHANNELS.to_le_bytes());
    h[24..28].copy_from_slice(&SAMPLE_RATE.to_le_bytes());
    h[28..32].copy_from_slice(&(SAMPLE_RATE * BYTES_PER_FRAME as u32).to_le_bytes());
    h[32..34].copy_from_slice(&(BYTES_PER_FRAME as u16).to_le_bytes());
    h[34..36].copy_from_slice(&16u16.to_le_bytes());
    h[36..40].copy_from_slice(b"data");
    h[40..44].copy_from_slice(&data_len.to_le_bytes());
    h
}

/// Where the PCM data of a WAV file lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WavInfo { pub sample_rate: u32, pub channels: u16, pub bits: u16, pub data_offset: u64, pub data_len: u64 }

impl WavInfo {
    pub fn duration_ms(&self) -> u32 {
        let frame = (self.channels as u64 * self.bits as u64 / 8).max(1);
        (self.data_len / frame * 1000 / self.sample_rate.max(1) as u64).min(u32::MAX as u64) as u32
    }
}

/// Parses the RIFF chunks of a PCM WAV file.
pub fn wav_info(path: &Path) -> Result<WavInfo> {
    let mut f = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let total = f.metadata()?.len();
    let mut riff = [0u8; 12];
    f.read_exact(&mut riff).context("wav header")?;
    anyhow::ensure!(&riff[0..4] == b"RIFF" && &riff[8..12] == b"WAVE", "{} is not a WAV file", path.display());
    let mut pos = 12u64;
    let mut fmt: Option<(u32, u16, u16)> = None;
    while pos + 8 <= total {
        f.seek(SeekFrom::Start(pos))?;
        let mut h = [0u8; 8];
        f.read_exact(&mut h)?;
        let len = u32::from_le_bytes(h[4..8].try_into().unwrap()) as u64;
        match &h[0..4] {
            b"fmt " => {
                let mut b = [0u8; 16];
                f.read_exact(&mut b)?;
                let channels = u16::from_le_bytes(b[2..4].try_into().unwrap());
                let rate = u32::from_le_bytes(b[4..8].try_into().unwrap());
                let bits = u16::from_le_bytes(b[14..16].try_into().unwrap());
                fmt = Some((rate, channels, bits));
            }
            b"data" => {
                let (sample_rate, channels, bits) = fmt.context("wav: data before fmt")?;
                let data_len = if len == 0 || pos + 8 + len > total { total - pos - 8 } else { len };
                return Ok(WavInfo { sample_rate, channels, bits, data_offset: pos + 8, data_len });
            }
            _ => {}
        }
        pos += 8 + len + (len & 1);
    }
    anyhow::bail!("{}: no data chunk", path.display())
}

/// Duration of a WAV file, if it is one.
pub fn wav_duration_ms(path: &Path) -> Option<u32> { wav_info(path).ok().map(|i| i.duration_ms()) }

// ------------------------------------------------------------------ mixing

fn ffmpeg() -> Result<linkshot_tools::Tool> { linkshot_tools::find("ffmpeg").context("ffmpeg is missing from this installation") }

/// Mixes the tracks into `out` (48 kHz stereo PCM). One track is simply moved.
pub fn mix(tracks: &[PathBuf], out: &Path) -> Result<()> {
    anyhow::ensure!(!tracks.is_empty(), "no audio tracks");
    if tracks.len() == 1 {
        if std::fs::rename(&tracks[0], out).is_err() { std::fs::copy(&tracks[0], out)?; let _ = std::fs::remove_file(&tracks[0]); }
        return Ok(());
    }
    let tool = ffmpeg()?;
    let mut cmd = tool.command();
    cmd.args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"]);
    for t in tracks { cmd.arg("-i").arg(t); }
    let inputs: String = (0..tracks.len()).map(|i| format!("[{i}:a]")).collect();
    cmd.arg("-filter_complex").arg(format!("{inputs}amix=inputs={}:duration=longest:dropout_transition=0:normalize=0", tracks.len()));
    cmd.args(["-ac", "2", "-ar", "48000", "-c:a", "pcm_s16le"]).arg(out);
    let output = cmd.stdin(std::process::Stdio::null()).output().context("run ffmpeg")?;
    anyhow::ensure!(output.status.success(), "ffmpeg amix failed: {}", String::from_utf8_lossy(&output.stderr).trim());
    for t in tracks { let _ = std::fs::remove_file(t); }
    Ok(())
}

/// Extracts the audio of a video file into a 48 kHz stereo WAV; `Ok(false)` when the file has no audio.
pub fn extract(video: &Path, out: &Path) -> Result<bool> {
    let tool = ffmpeg()?;
    let output = tool.command().args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y", "-i"]).arg(video)
        .args(["-vn", "-ac", "2", "-ar", "48000", "-c:a", "pcm_s16le"]).arg(out).stdin(std::process::Stdio::null()).output().context("run ffmpeg")?;
    if output.status.success() && out.is_file() && std::fs::metadata(out).map(|m| m.len() > 44).unwrap_or(false) { return Ok(true); }
    let _ = std::fs::remove_file(out);
    let err = String::from_utf8_lossy(&output.stderr);
    if err.contains("does not contain any stream") || err.contains("Output file is empty") || err.contains("Stream map") || err.is_empty() { return Ok(false); }
    tracing::debug!("audio extract {}: {}", video.display(), err.trim());
    Ok(false)
}

// ------------------------------------------------------------------ playback

/// Plays a WAV track from `start_ms` until dropped.
pub struct Playback { _inner: backend::Playback, pub started: Instant, pub start_ms: u32 }

impl Playback {
    pub fn start(path: &Path, start_ms: u32, volume: f32) -> Result<Playback> {
        let inner = backend::Playback::start(path, start_ms, volume.clamp(0.0, 1.0))?;
        Ok(Playback { _inner: inner, started: Instant::now(), start_ms })
    }
    /// Where playback should be now, by the wall clock.
    pub fn position_ms(&self) -> u32 { self.start_ms.saturating_add(self.started.elapsed().as_millis().min(u32::MAX as u128) as u32) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_roundtrip() {
        let dir = std::env::temp_dir().join(format!("linkshot-audio-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("t.wav");
        let mut w = WavWriter::create(&p).unwrap();
        let pcm: Vec<u8> = (0..SAMPLE_RATE as usize * BYTES_PER_FRAME / 2).map(|i| (i % 251) as u8).collect(); // 0.5 s
        w.write(&pcm).unwrap();
        assert_eq!(w.finish().unwrap(), pcm.len() as u64);
        let info = wav_info(&p).unwrap();
        assert_eq!((info.sample_rate, info.channels, info.bits, info.data_offset), (48_000, 2, 16, 44));
        assert_eq!(info.duration_ms(), 500);
        assert_eq!(wav_duration_ms(&p), Some(500));
        assert_eq!(peak(&[0, 0, 0x00, 0x40]), 0.5);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
