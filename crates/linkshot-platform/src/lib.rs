//! OS integration: clipboard, opening files/URLs with other programs, autostart.

use anyhow::{Context, Result};
use image::RgbaImage;

pub mod clipboard {
    use super::*;

    pub fn set_image(img: &RgbaImage) -> Result<()> {
        let mut cb = arboard::Clipboard::new().context("open clipboard")?;
        cb.set_image(arboard::ImageData {
            width: img.width() as usize,
            height: img.height() as usize,
            bytes: std::borrow::Cow::Borrowed(img.as_raw()),
        })
        .context("set clipboard image")?;
        Ok(())
    }
    pub fn set_text(text: &str) -> Result<()> {
        let mut cb = arboard::Clipboard::new()?;
        cb.set_text(text.to_string())?;
        Ok(())
    }
    pub fn get_text() -> Option<String> {
        arboard::Clipboard::new().ok()?.get_text().ok()
    }
    pub fn get_image() -> Option<RgbaImage> {
        let mut cb = arboard::Clipboard::new().ok()?;
        let d = cb.get_image().ok()?;
        RgbaImage::from_raw(d.width as u32, d.height as u32, d.bytes.into_owned())
    }
}

/// Opens a file or URL with the system default handler (Program share destination, mailto:, help links).
pub fn open_path(path: &std::path::Path) -> Result<()> {
    open::that_detached(path).with_context(|| format!("open {}", path.display()))
}
pub fn open_url(url: &str) -> Result<()> {
    open::that_detached(url).with_context(|| format!("open {url}"))
}
/// Runs an arbitrary program with `<CaptureFilename>` substituted (the "Program" share output).
pub fn run_program(executable: &str, parameters: &str, file: &std::path::Path) -> Result<()> {
    let args_str = parameters.replace("<CaptureFilename>", &file.to_string_lossy());
    let args = shell_words(&args_str);
    std::process::Command::new(executable).args(args).spawn().with_context(|| format!("run {executable}"))?;
    Ok(())
}

fn shell_words(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    for c in s.chars() {
        match (c, quote) {
            ('"' | '\'', None) => quote = Some(c),
            (q, Some(open)) if q == open => quote = None,
            (' ', None) => { if !cur.is_empty() { out.push(std::mem::take(&mut cur)); } }
            _ => cur.push(c),
        }
    }
    if !cur.is_empty() { out.push(cur); }
    out
}

/// A `mailto:` compose (Email share destination without an attachment API).
pub fn compose_email(address: &str, subject: &str, body: &str) -> Result<()> {
    fn enc(s: &str) -> String {
        s.bytes().map(|b| match b { b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(), _ => format!("%{b:02X}") }).collect()
    }
    open_url(&format!("mailto:{}?subject={}&body={}", address, enc(subject), enc(body)))
}

/// Sends an image file to the printer: CUPS (`lp`, then `lpr`) on Linux, the shell "Print" verb on
/// Windows (which opens the system print dialog for pictures).
pub fn print_file(path: &std::path::Path) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        for (tool, args) in [("lp", vec!["-o", "fit-to-page"]), ("lpr", vec![])] {
            match std::process::Command::new(tool).args(&args).arg(path).stdin(std::process::Stdio::null()).output() {
                Ok(o) if o.status.success() => return Ok(()),
                Ok(o) => anyhow::bail!("{tool}: {}", String::from_utf8_lossy(&o.stderr).trim()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => anyhow::bail!("{tool}: {e}"),
            }
        }
        anyhow::bail!("no print spooler found (install CUPS: lp / lpr)")
    }
    #[cfg(windows)]
    {
        let p = path.to_string_lossy().replace('\'', "''");
        std::process::Command::new("powershell").args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &format!("Start-Process -FilePath '{p}' -Verb Print")]).spawn().context("start the print dialog")?;
        Ok(())
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    { open_path(path) }
}

/// Reveals a file in the file manager when possible, otherwise opens its folder.
pub fn reveal(path: &std::path::Path) -> Result<()> {
    if let Some(parent) = path.parent() { open_path(parent) } else { Ok(()) }
}

pub mod default_tool;

pub mod autostart {
    //! "Run linkshot when the system starts".
    use anyhow::Result;
    #[cfg(target_os = "linux")]
    fn desktop_file() -> Option<std::path::PathBuf> {
        directories::BaseDirs::new().map(|b| b.config_dir().join("autostart/linkshot.desktop"))
    }
    pub fn is_enabled() -> bool {
        #[cfg(target_os = "linux")]
        { return desktop_file().map(|p| p.exists()).unwrap_or(false); }
        #[cfg(windows)]
        { return windows_run_key(&["query", RUN_KEY, "/v", "LinkShot"]).is_ok(); }
        #[cfg(not(any(target_os = "linux", windows)))]
        { false }
    }

    #[cfg(windows)]
    const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
    #[cfg(windows)]
    fn windows_run_key(args: &[&str]) -> Result<()> {
        use std::os::windows::process::CommandExt;
        let out = std::process::Command::new("reg").args(args).creation_flags(0x0800_0000).output()?;
        anyhow::ensure!(out.status.success(), "reg failed: {}", String::from_utf8_lossy(&out.stderr).trim());
        Ok(())
    }
    pub fn set_enabled(enabled: bool) -> Result<()> {
        #[cfg(target_os = "linux")]
        {
            let Some(p) = desktop_file() else { return Ok(()); };
            if enabled {
                std::fs::create_dir_all(p.parent().unwrap())?;
                let exe = std::env::current_exe()?;
                std::fs::write(&p, format!("[Desktop Entry]\nType=Application\nName=LinkShot\nExec={} --background\nX-GNOME-Autostart-enabled=true\n", exe.display()))?;
            } else if p.exists() {
                std::fs::remove_file(p)?;
            }
        }
        #[cfg(windows)]
        {
            if enabled {
                let exe = std::env::current_exe()?;
                windows_run_key(&["add", RUN_KEY, "/v", "LinkShot", "/t", "REG_SZ", "/d", &format!("\"{}\" --background", exe.display()), "/f"])?;
            } else {
                let _ = windows_run_key(&["delete", RUN_KEY, "/v", "LinkShot", "/f"]);
            }
        }
        Ok(())
    }
}
