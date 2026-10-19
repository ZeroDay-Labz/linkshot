//! MP4 export and video probing/decoding through the bundled `ffmpeg` (see
//! `linkshot_tools::find`: `LINKSHOT_FFMPEG`, the shipped bundle, then `PATH`). Frames are piped as raw RGBA at a
//! constant frame rate; the encoder is the best H.264 encoder the build offers.

use crate::source::{output_size, resize_rgba, FrameSource};
use crate::spool::{SpoolInfo, SpoolWriter};
use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

/// The `ffmpeg` LinkShot ships with (or `LINKSHOT_FFMPEG`, or one on `PATH`).
pub fn find_ffmpeg() -> Option<PathBuf> { linkshot_tools::find("ffmpeg").map(|t| t.path) }

/// A command for a located ffmpeg/ffprobe with the bundle environment (no console window on Windows).
fn command(path: &Path) -> Command {
    let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("ffmpeg");
    match linkshot_tools::find(name).filter(|t| t.path == path) {
        Some(t) => t.command(),
        None => { let tool = linkshot_tools::Tool { path: path.to_path_buf(), env: vec![], bundled: false }; tool.command() }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum H264Encoder { Libx264, OpenH264, Mpeg4 }

/// Picks the video encoder once per process.
pub fn encoder(ffmpeg: &Path) -> H264Encoder {
    static ENC: OnceLock<H264Encoder> = OnceLock::new();
    *ENC.get_or_init(|| {
        let out = command(ffmpeg).args(["-hide_banner", "-encoders"]).stdin(Stdio::null()).stderr(Stdio::null()).output();
        let text = out.map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
        let has = |n: &str| text.lines().any(|l| l.split_whitespace().nth(1) == Some(n));
        if has("libx264") { H264Encoder::Libx264 } else if has("libopenh264") { H264Encoder::OpenH264 } else { H264Encoder::Mpeg4 }
    })
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Mp4Options {
    /// Output frame rate; 0 = the source's nominal rate.
    pub fps: f32,
    /// Output size; 0 keeps the source size / aspect ratio.
    pub width: u32,
    pub height: u32,
    /// libx264 constant-rate factor (lower = better; 18 is visually lossless).
    pub crf: u8,
    pub start_ms: Option<u32>,
    pub end_ms: Option<u32>,
    /// A WAV track to mux in (AAC), trimmed to the same range.
    pub audio: Option<PathBuf>,
}

impl Default for Mp4Options {
    fn default() -> Self { Self { fps: 0.0, width: 0, height: 0, crf: 18, start_ms: None, end_ms: None, audio: None } }
}

#[derive(Clone, Debug, Default)]
pub struct Mp4Report {
    pub frames: usize,
    pub bytes: u64,
    pub duration_ms: u32,
    pub width: u32,
    pub height: u32,
    pub encoder: String,
}

fn nominal_fps(src: &dyn FrameSource) -> f32 {
    let n = src.len();
    if n < 2 { return 10.0; }
    let span = src.pts_ms(n - 1).saturating_sub(src.pts_ms(0)).max(1) as f32;
    let fps = (n as f32 - 1.0) * 1000.0 / span;
    // Snap to the usual rates.
    [10.0f32, 12.0, 15.0, 20.0, 24.0, 25.0, 30.0, 50.0, 60.0].iter().copied().min_by(|a, b| (a - fps).abs().partial_cmp(&(b - fps).abs()).unwrap()).unwrap_or(30.0)
}

/// Exports `src` to an H.264 MP4. `progress(0..=1)` returns false to cancel.
pub fn export_mp4(src: &mut dyn FrameSource, opts: &Mp4Options, out: &Path, progress: &mut dyn FnMut(f32) -> bool) -> Result<Mp4Report> {
    ensure!(!src.is_empty(), "the video has no frames");
    let ffmpeg = find_ffmpeg().context("MP4 export needs ffmpeg (missing from this installation; set LINKSHOT_FFMPEG or install ffmpeg)")?;
    let enc = encoder(&ffmpeg);
    let (sw, sh) = (src.width(), src.height());
    let (ow, oh) = output_size(sw, sh, opts.width, opts.height);
    let fps = if opts.fps > 0.0 { opts.fps.clamp(1.0, 120.0) } else { nominal_fps(src) };
    let tick = 1000.0 / fps;
    let start = opts.start_ms.unwrap_or(0).min(src.duration_ms());
    let end = opts.end_ms.unwrap_or(src.duration_ms()).clamp(start + 1, src.duration_ms().max(start + 1));
    let ticks = (((end - start) as f32) / tick).round().max(1.0) as usize;
    if let Some(p) = out.parent() { std::fs::create_dir_all(p).ok(); }

    let mut cmd = command(&ffmpeg);
    cmd.args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y", "-f", "rawvideo", "-pix_fmt", "rgba"])
        .arg("-video_size").arg(format!("{ow}x{oh}"))
        .arg("-framerate").arg(format!("{fps}"))
        .args(["-i", "pipe:0"]);
    let audio = opts.audio.as_ref().filter(|a| a.is_file());
    match audio {
        Some(a) => {
            cmd.arg("-ss").arg(format!("{:.3}", start as f64 / 1000.0)).arg("-t").arg(format!("{:.3}", (end - start) as f64 / 1000.0)).arg("-i").arg(a)
                .args(["-map", "0:v:0", "-map", "1:a:0", "-c:a", "aac", "-b:a", "160k", "-ar", "48000", "-ac", "2"]);
        }
        None => { cmd.arg("-an"); }
    }
    // H.264 4:2:0 needs even dimensions: pad by a pixel rather than crop content away.
    cmd.args(["-vf", "pad=ceil(iw/2)*2:ceil(ih/2)*2:0:0:color=black"]);
    let enc_name = match enc {
        H264Encoder::Libx264 => { cmd.args(["-c:v", "libx264", "-preset", "medium", "-crf", &opts.crf.clamp(0, 51).to_string(), "-pix_fmt", "yuv420p", "-tune", "animation"]); "libx264" }
        H264Encoder::OpenH264 => {
            let kbps = ((ow as u64 * oh as u64 * fps as u64) / 180).clamp(800, 40_000);
            cmd.args(["-c:v", "libopenh264", "-b:v", &format!("{kbps}k"), "-pix_fmt", "yuv420p"]);
            "libopenh264"
        }
        H264Encoder::Mpeg4 => { cmd.args(["-c:v", "mpeg4", "-q:v", "3", "-pix_fmt", "yuv420p"]); "mpeg4" }
    };
    cmd.args(["-movflags", "+faststart"]).arg(out);
    let mut child = cmd.stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped()).spawn().with_context(|| format!("start {}", ffmpeg.display()))?;
    let mut stdin = child.stdin.take().context("ffmpeg stdin")?;
    let mut stderr = child.stderr.take().context("ffmpeg stderr")?;
    let err_thread = std::thread::spawn(move || { let mut s = String::new(); let _ = stderr.read_to_string(&mut s); s });

    let mut last_i = usize::MAX;
    let mut scaled: Vec<u8> = vec![];
    let mut write_err: Option<std::io::Error> = None;
    let mut cancelled = false;
    for k in 0..ticks {
        let t = start + (k as f32 * tick) as u32;
        let i = src.index_at(t);
        if i != last_i {
            let f = src.frame(i)?;
            scaled = resize_rgba(f, sw, sh, ow, oh);
            last_i = i;
        }
        if let Err(e) = stdin.write_all(&scaled) { write_err = Some(e); break; }
        if k % 8 == 0 && !progress(0.98 * k as f32 / ticks as f32) { cancelled = true; break; }
    }
    drop(stdin);
    let status = child.wait().context("wait for ffmpeg")?;
    let err_text = err_thread.join().unwrap_or_default();
    if cancelled { let _ = std::fs::remove_file(out); bail!("cancelled"); }
    if let Some(e) = write_err { let _ = std::fs::remove_file(out); bail!("ffmpeg stopped reading frames ({e}): {}", err_text.trim()); }
    if !status.success() { let _ = std::fs::remove_file(out); bail!("ffmpeg failed ({status}): {}", err_text.trim()); }
    progress(1.0);
    Ok(Mp4Report { frames: ticks, bytes: std::fs::metadata(out).map(|m| m.len()).unwrap_or(0), duration_ms: end - start, width: ow, height: oh, encoder: enc_name.into() })
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProbeInfo {
    pub width: u32,
    pub height: u32,
    pub fps: f32,
    pub duration_ms: u32,
    /// The file has an audio stream.
    pub audio: bool,
}


/// Reads size, frame rate and duration of a video file from `ffmpeg -i` (so only ffmpeg has to ship).
pub fn probe(path: &Path) -> Result<ProbeInfo> {
    let ffmpeg = find_ffmpeg().context("ffmpeg not found")?;
    let out = command(&ffmpeg).args(["-hide_banner", "-nostdin", "-i"]).arg(path).stdin(Stdio::null()).output().context("run ffmpeg")?;
    // ffmpeg exits with an error because no output was requested; the stream description is on stderr.
    let text = String::from_utf8_lossy(&out.stderr);
    parse_probe(&text).with_context(|| format!("no video stream found in {}", path.display()))
}

/// Parses the `Duration:` and `Stream #… Video:` lines of ffmpeg's input description.
fn parse_probe(text: &str) -> Option<ProbeInfo> {
    let mut duration_ms = 0u32;
    let mut video: Option<(u32, u32, f64)> = None;
    let mut audio = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with("Stream #") && t.contains("Audio:") { audio = true; }
        if let Some(rest) = t.strip_prefix("Duration: ") {
            let d = rest.split(',').next().unwrap_or("").trim();
            let parts: Vec<&str> = d.split(':').collect();
            if parts.len() == 3 {
                let h: f64 = parts[0].parse().unwrap_or(0.0);
                let m: f64 = parts[1].parse().unwrap_or(0.0);
                let s: f64 = parts[2].parse().unwrap_or(0.0);
                duration_ms = ((h * 3600.0 + m * 60.0 + s) * 1000.0).round() as u32;
            }
        } else if video.is_none() && t.starts_with("Stream #") && t.contains("Video:") {
            let mut size = None;
            let mut fps = None;
            let mut tbr = None;
            for field in t.split(',').map(str::trim) {
                let first = field.split_whitespace().next().unwrap_or("");
                if size.is_none() {
                    if let Some((w, h)) = first.split_once('x') {
                        if let (Ok(w), Ok(h)) = (w.parse::<u32>(), h.parse::<u32>()) { if w > 0 && h > 0 { size = Some((w, h)); } }
                    }
                }
                let mut words = field.split_whitespace();
                if let (Some(v), Some(unit)) = (words.next(), words.next()) {
                    match unit {
                        "fps" => fps = v.replace('k', "000").parse::<f64>().ok(),
                        "tbr" => tbr = v.replace('k', "000").parse::<f64>().ok(),
                        _ => {}
                    }
                }
            }
            let (w, h) = size?;
            let rate = fps.or(tbr).filter(|f| *f > 0.5 && *f <= 240.0).unwrap_or(30.0);
            video = Some((w, h, rate));
        }
    }
    let (width, height, fps) = video?;
    Some(ProbeInfo { width, height, fps: fps as f32, duration_ms, audio })
}

/// Decodes a video file into a spool at its nominal frame rate (constant frame rate output).
pub fn decode_to_spool(path: &Path, spool: &Path, progress: &mut dyn FnMut(f32) -> bool) -> Result<SpoolInfo> {
    let ffmpeg = find_ffmpeg().context("opening videos needs ffmpeg (missing from this installation; set LINKSHOT_FFMPEG or install ffmpeg)")?;
    let info = probe(path)?;
    let fps = info.fps.clamp(1.0, 60.0);
    let (w, h) = (info.width, info.height);
    let frame_len = (w * h * 4) as usize;
    let mut child = command(&ffmpeg)
        .args(["-hide_banner", "-loglevel", "error", "-nostdin"]).arg("-i").arg(path)
        .args(["-an", "-sn", "-fps_mode", "cfr", "-r", &format!("{fps}"), "-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"])
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().context("start ffmpeg")?;
    let mut stdout = child.stdout.take().context("ffmpeg stdout")?;
    let mut stderr = child.stderr.take().context("ffmpeg stderr")?;
    let err_thread = std::thread::spawn(move || { let mut s = String::new(); let _ = stderr.read_to_string(&mut s); s });
    let mut writer = SpoolWriter::create(spool, w, h, fps.round() as u32)?;
    let mut buf = vec![0u8; frame_len];
    let expected = ((info.duration_ms as f32 / 1000.0) * fps).max(1.0);
    let mut i = 0u32;
    loop {
        match read_full(&mut stdout, &mut buf) {
            Ok(true) => {}
            Ok(false) => break,
            Err(e) => { let _ = child.kill(); writer.abort(); bail!("read frames: {e}"); }
        }
        writer.push((i as f64 * 1000.0 / fps as f64).round() as u32, &buf)?;
        i += 1;
        if i % 8 == 0 && !progress((i as f32 / expected).min(0.98)) { let _ = child.kill(); writer.abort(); bail!("cancelled"); }
    }
    let status = child.wait()?;
    let err_text = err_thread.join().unwrap_or_default();
    if writer.frames() == 0 { writer.abort(); bail!("ffmpeg produced no frames ({status}): {}", err_text.trim()); }
    if !status.success() { tracing::warn!("ffmpeg exited with {status} after {} frames: {}", writer.frames(), err_text.trim()); }
    progress(1.0);
    writer.finish()
}

fn read_full(r: &mut impl Read, buf: &mut [u8]) -> std::io::Result<bool> {
    let mut filled = 0;
    while filled < buf.len() {
        let n = r.read(&mut buf[filled..])?;
        if n == 0 { return if filled == 0 { Ok(false) } else { Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "partial frame")) }; }
        filled += n;
    }
    Ok(true)
}

/// First frame of a video file as PNG bytes (poster for the tray), via ffmpeg.
pub fn poster_png(path: &Path) -> Result<Vec<u8>> {
    let ffmpeg = find_ffmpeg().context("ffmpeg not found")?;
    let out = command(&ffmpeg)
        .args(["-hide_banner", "-loglevel", "error", "-nostdin"]).arg("-i").arg(path)
        .args(["-an", "-frames:v", "1", "-f", "image2pipe", "-vcodec", "png", "pipe:1"])
        .stdin(Stdio::null()).output().context("run ffmpeg")?;
    ensure!(out.status.success() && !out.stdout.is_empty(), "ffmpeg poster failed: {}", String::from_utf8_lossy(&out.stderr).trim());
    Ok(out.stdout)
}
