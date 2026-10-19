//! Windows backend: WASAPI shared-mode capture (loopback on the render endpoint for system audio)
//! and rendering, with the audio engine converting to 48 kHz stereo 16-bit.

use super::{CaptureSpec, Device, Kind, Source, BYTES_PER_FRAME, SAMPLE_RATE};
use anyhow::{Context, Result};
use std::collections::VecDeque;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use wasapi::{AudioClient, AudioCaptureClient, DeviceEnumerator, Direction, SampleType, StreamMode, WaveFormat};

pub fn available() -> bool { true }

fn com() { let _ = wasapi::initialize_mta(); }

fn format() -> WaveFormat { WaveFormat::new(16, 16, &SampleType::Int, SAMPLE_RATE as usize, 2, None) }

pub fn devices() -> Vec<Device> {
    com();
    let mut out = vec![
        Device { id: String::new(), name: "Default microphone".into(), kind: Kind::Microphone, is_default: true },
        Device { id: String::new(), name: "Default output (what you hear)".into(), kind: Kind::SystemAudio, is_default: true },
    ];
    let Ok(en) = DeviceEnumerator::new() else { return out };
    for (dir, kind) in [(Direction::Capture, Kind::Microphone), (Direction::Render, Kind::SystemAudio)] {
        let Ok(coll) = en.get_device_collection(&dir) else { continue };
        let n = coll.get_nbr_devices().unwrap_or(0);
        for i in 0..n {
            let Ok(d) = coll.get_device_at_index(i) else { continue };
            let (Ok(id), Ok(name)) = (d.get_id(), d.get_friendlyname()) else { continue };
            out.push(Device { id, name, kind, is_default: false });
        }
    }
    out
}

struct WasapiSource {
    _client: AudioClient,
    capture: AudioCaptureClient,
    stop: Arc<AtomicBool>,
    pending: VecDeque<u8>,
    started: Instant,
    delivered: u64,
}

// The client and capture interfaces are created and used on the capture thread only; COM objects
// from an MTA may be used from any thread, so handing the whole source to that thread is sound.
unsafe impl Send for WasapiSource {}

impl Source for WasapiSource {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        loop {
            if self.stop.load(Ordering::Relaxed) { return Ok(0); }
            if !self.pending.is_empty() {
                let n = self.pending.len().min(buf.len());
                for (i, b) in self.pending.drain(..n).enumerate() { buf[i] = b; }
                self.delivered += n as u64;
                return Ok(n);
            }
            match self.capture.get_next_packet_size() {
                Ok(Some(0)) | Ok(None) => {
                    // Loopback endpoints go quiet while nothing plays: keep the clock running with silence.
                    let expected = (self.started.elapsed().as_secs_f64() * SAMPLE_RATE as f64) as u64 * BYTES_PER_FRAME as u64;
                    let behind = expected.saturating_sub(self.delivered);
                    if behind >= SAMPLE_RATE as u64 / 5 * BYTES_PER_FRAME as u64 {
                        let n = (behind as usize).min(buf.len()) / BYTES_PER_FRAME * BYTES_PER_FRAME;
                        buf[..n].fill(0);
                        self.delivered += n as u64;
                        return Ok(n);
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
                Ok(Some(_)) => { self.capture.read_from_device_to_deque(&mut self.pending).map_err(|e| anyhow::anyhow!("wasapi read: {e}"))?; }
                Err(e) => return Err(anyhow::anyhow!("wasapi: {e}")),
            }
        }
    }
    fn stopper(&self) -> Box<dyn Fn() + Send + Sync> {
        let s = self.stop.clone();
        Box::new(move || s.store(true, Ordering::Relaxed))
    }
}

pub fn open(spec: &CaptureSpec) -> Result<Box<dyn Source>> {
    com();
    let en = DeviceEnumerator::new().map_err(|e| anyhow::anyhow!("audio devices: {e}"))?;
    let dir = match spec.kind { Kind::Microphone => Direction::Capture, Kind::SystemAudio => Direction::Render };
    let device = match spec.device.as_deref().filter(|d| !d.is_empty()) {
        Some(id) => en.get_device(id).map_err(|e| anyhow::anyhow!("audio device {id}: {e}"))?,
        None => en.get_default_device(&dir).map_err(|e| anyhow::anyhow!("default audio device: {e}"))?,
    };
    let mut client = device.get_iaudioclient().map_err(|e| anyhow::anyhow!("audio client: {e}"))?;
    let mode = StreamMode::PollingShared { autoconvert: true, buffer_duration_hns: 4_000_000 };
    client.initialize_client(&format(), &Direction::Capture, &mode).map_err(|e| anyhow::anyhow!("open audio capture: {e}"))?;
    let capture = client.get_audiocaptureclient().map_err(|e| anyhow::anyhow!("capture client: {e}"))?;
    client.start_stream().map_err(|e| anyhow::anyhow!("start audio stream: {e}"))?;
    Ok(Box::new(WasapiSource { _client: client, capture, stop: Arc::new(AtomicBool::new(false)), pending: VecDeque::new(), started: Instant::now(), delivered: 0 }))
}

pub struct Playback { stop: Arc<AtomicBool>, thread: Option<std::thread::JoinHandle<()>> }

impl Playback {
    pub fn start(path: &Path, start_ms: u32, volume: f32) -> Result<Self> {
        let info = super::wav_info(path)?;
        anyhow::ensure!(info.bits == 16, "only 16-bit WAV playback is supported");
        let mut file = std::fs::File::open(path)?;
        let frame = info.channels as u64 * 2;
        let skip = (start_ms as u64 * info.sample_rate as u64 / 1000) * frame;
        file.seek(SeekFrom::Start(info.data_offset + skip.min(info.data_len)))?;
        let mut remaining = info.data_len.saturating_sub(skip);
        let stop = Arc::new(AtomicBool::new(false));
        let s = stop.clone();
        let (tx, rx) = std::sync::mpsc::channel::<Result<()>>();
        let (rate, channels) = (info.sample_rate as usize, info.channels as usize);
        let thread = std::thread::Builder::new().name("linkshot-audio-play".into()).spawn(move || {
            com();
            let res = (|| -> Result<(AudioClient, wasapi::AudioRenderClient, usize)> {
                let en = DeviceEnumerator::new().map_err(|e| anyhow::anyhow!("{e}"))?;
                let device = en.get_default_device(&Direction::Render).map_err(|e| anyhow::anyhow!("default output: {e}"))?;
                let mut client = device.get_iaudioclient().map_err(|e| anyhow::anyhow!("{e}"))?;
                let fmt = WaveFormat::new(16, 16, &SampleType::Int, rate, channels, None);
                client.initialize_client(&fmt, &Direction::Render, &StreamMode::PollingShared { autoconvert: true, buffer_duration_hns: 3_000_000 }).map_err(|e| anyhow::anyhow!("open audio output: {e}"))?;
                let render = client.get_audiorenderclient().map_err(|e| anyhow::anyhow!("{e}"))?;
                let bpf = channels * 2;
                Ok((client, render, bpf))
            })();
            let (client, render, bpf) = match res { Ok(v) => { let _ = tx.send(Ok(())); v } Err(e) => { let _ = tx.send(Err(e)); return; } };
            let mut started = false;
            let mut buf = vec![0u8; 0];
            while !s.load(Ordering::Relaxed) && remaining > 0 {
                let Ok(space) = client.get_available_space_in_frames() else { break };
                if space == 0 { std::thread::sleep(Duration::from_millis(5)); continue; }
                let want = ((space as u64) * bpf as u64).min(remaining) as usize / bpf * bpf;
                if want == 0 { break; }
                buf.resize(want, 0);
                let Ok(()) = file.read_exact(&mut buf) else { break };
                remaining -= want as u64;
                if (volume - 1.0).abs() > 0.01 {
                    for sm in buf.chunks_exact_mut(2) { let v = (i16::from_le_bytes([sm[0], sm[1]]) as f32 * volume) as i16; sm.copy_from_slice(&v.to_le_bytes()); }
                }
                if render.write_to_device(want / bpf, &buf, None).is_err() { break; }
                if !started { started = true; let _ = client.start_stream(); }
            }
            if started && !s.load(Ordering::Relaxed) {
                // Let the buffer drain.
                let total = client.get_buffer_size().unwrap_or(0) as u64;
                std::thread::sleep(Duration::from_millis((total * 1000 / rate as u64).min(500)));
            }
            let _ = client.stop_stream();
        }).context("spawn playback thread")?;
        match rx.recv_timeout(Duration::from_secs(3)) {
            Ok(Ok(())) => Ok(Self { stop, thread: Some(thread) }),
            Ok(Err(e)) => Err(e),
            Err(_) => { stop.store(true, Ordering::Relaxed); anyhow::bail!("the audio output did not answer") }
        }
    }
}

impl Drop for Playback {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() { let _ = t.join(); }
    }
}
