//! Linux backend: the bundled `ffmpeg` talks to PulseAudio / PipeWire (`-f pulse`) for capture
//! (`@DEFAULT_SOURCE@`, `@DEFAULT_MONITOR@` or a named source) and playback.

use super::{CaptureSpec, Device, Kind, Source};
use anyhow::{Context, Result};
use std::io::Read;
use std::path::Path;
use std::process::{Child, ChildStdout, Command, Stdio};

fn ffmpeg() -> Option<linkshot_tools::Tool> { linkshot_tools::find("ffmpeg") }

pub fn available() -> bool { ffmpeg().is_some() }

fn base(tool: &linkshot_tools::Tool) -> Command {
    let mut c = tool.command();
    c.args(["-hide_banner", "-loglevel", "error", "-nostdin"]);
    c
}

/// Parses `ffmpeg -sources pulse`: lines like `* alsa_input.pci… [Built-in Audio] (none)`.
pub fn devices() -> Vec<Device> {
    let mut out = vec![
        Device { id: "@DEFAULT_SOURCE@".into(), name: "Default microphone".into(), kind: Kind::Microphone, is_default: true },
        Device { id: "@DEFAULT_MONITOR@".into(), name: "Default output (what you hear)".into(), kind: Kind::SystemAudio, is_default: true },
    ];
    let Some(tool) = ffmpeg() else { return out };
    let Ok(res) = tool.command().args(["-hide_banner", "-sources", "pulse"]).stdin(Stdio::null()).output() else { return out };
    let text = String::from_utf8_lossy(&res.stdout).into_owned() + &String::from_utf8_lossy(&res.stderr);
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with("Auto-detected") { continue; }
        let t = t.trim_start_matches('*').trim();
        let Some((id, rest)) = t.split_once(' ') else { continue };
        let name = rest.trim().strip_prefix('[').and_then(|r| r.rsplit_once(']')).map(|(n, _)| n.to_string()).unwrap_or_else(|| id.to_string());
        let kind = if id.ends_with(".monitor") { Kind::SystemAudio } else { Kind::Microphone };
        let name = name.trim_start_matches("Monitor of ").to_string();
        out.push(Device { id: id.to_string(), name, kind, is_default: false });
    }
    out
}

struct FfmpegSource { child: Child, stdout: ChildStdout, pid: u32 }

impl Source for FfmpegSource {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        loop {
            match self.stdout.read(buf) {
                Ok(n) => return Ok(n),
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e.into()),
            }
        }
    }
    fn stopper(&self) -> Box<dyn Fn() + Send + Sync> {
        let pid = self.pid;
        Box::new(move || unsafe { libc_kill(pid as i32); })
    }
}

impl Drop for FfmpegSource {
    fn drop(&mut self) { let _ = self.child.kill(); let _ = self.child.wait(); }
}

unsafe extern "C" { fn kill(pid: i32, sig: i32) -> i32; }
unsafe fn libc_kill(pid: i32) { unsafe { kill(pid, 15); } }

pub fn open(spec: &CaptureSpec) -> Result<Box<dyn Source>> {
    let tool = ffmpeg().context("audio capture needs ffmpeg, which is missing from this installation")?;
    let device = match (&spec.device, spec.kind) {
        (Some(d), _) if !d.is_empty() => d.clone(),
        (_, Kind::Microphone) => "@DEFAULT_SOURCE@".into(),
        (_, Kind::SystemAudio) => "@DEFAULT_MONITOR@".into(),
    };
    let mut cmd = base(&tool);
    cmd.args(["-f", "pulse", "-fragment_size", "3840", "-i", &device, "-ac", "2", "-ar", "48000", "-f", "s16le", "pipe:1"]);
    let mut child = cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().context("start ffmpeg")?;
    let stdout = child.stdout.take().context("ffmpeg stdout")?;
    let pid = child.id();
    // ffmpeg exits at once when the source is unknown: give it a moment and report its message.
    std::thread::sleep(std::time::Duration::from_millis(120));
    if let Ok(Some(status)) = child.try_wait() {
        let mut err = String::new();
        if let Some(mut e) = child.stderr.take() { let _ = e.read_to_string(&mut err); }
        anyhow::bail!("audio device '{device}' could not be opened ({status}): {}", err.trim());
    }
    Ok(Box::new(FfmpegSource { child, stdout, pid }))
}

pub struct Playback { child: Child }

impl Playback {
    pub fn start(path: &Path, start_ms: u32, volume: f32) -> Result<Self> {
        let tool = ffmpeg().context("audio playback needs ffmpeg")?;
        let mut cmd = base(&tool);
        cmd.arg("-ss").arg(format!("{:.3}", start_ms as f64 / 1000.0)).arg("-i").arg(path);
        if (volume - 1.0).abs() > 0.01 { cmd.arg("-af").arg(format!("volume={volume:.2}")); }
        cmd.args(["-f", "pulse", "-device", "default", "LinkShot"]);
        let child = cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().context("start ffmpeg")?;
        Ok(Self { child })
    }
}

impl Drop for Playback {
    fn drop(&mut self) { let _ = self.child.kill(); let _ = self.child.wait(); }
}
