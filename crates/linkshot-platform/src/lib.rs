//! OS integration: clipboard, opening files/URLs with other programs, autostart.

use anyhow::{Context, Result};
use image::RgbaImage;

/// LinkShot's application id: the desktop file id (`<APP_ID>.desktop`), the Wayland window app id,
/// the id the desktop portals file permissions and global shortcuts under, and the Flatpak id.
pub const APP_ID: &str = "io.github.zerodaylabz.LinkShot";

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
    std::process::Command::new(executable).args(program_args(parameters, file)).spawn().with_context(|| format!("run {executable}"))?;
    Ok(())
}

/// The arguments for the Program destination: `parameters` split like a command line first, then
/// `<CaptureFilename>` replaced inside each argument, so a file name with spaces or quotes stays one
/// argument. No parameters at all means just the file.
fn program_args(parameters: &str, file: &std::path::Path) -> Vec<String> {
    let f = file.to_string_lossy();
    if parameters.trim().is_empty() { return vec![f.into_owned()]; }
    shell_words(parameters).into_iter().map(|w| w.replace("<CaptureFilename>", &f)).collect()
}

#[cfg(test)]
mod program_args_tests {
    #[test]
    fn file_names_stay_one_argument() {
        let f = std::path::Path::new("/tmp/My \"best\" shot.png");
        assert_eq!(super::program_args("--open \"<CaptureFilename>\" -x", f), vec!["--open", "/tmp/My \"best\" shot.png", "-x"]);
        assert_eq!(super::program_args("", f), vec!["/tmp/My \"best\" shot.png"]);
        assert_eq!(super::program_args("<CaptureFilename>", f), vec!["/tmp/My \"best\" shot.png"]);
    }
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
pub mod update;
pub mod upload;
#[cfg(windows)]
pub mod start_menu;

pub mod autostart {
    //! "Run linkshot when the system starts".
    use anyhow::Result;
    /// Named after the app id: systemd's autostart generator turns the file name into the unit name,
    /// which is where the desktop portals read a host application's id from.
    #[cfg(target_os = "linux")]
    fn desktop_file() -> Option<std::path::PathBuf> {
        directories::BaseDirs::new().map(|b| b.config_dir().join(format!("autostart/{}.desktop", crate::APP_ID)))
    }
    /// The file name used up to 0.5.0.
    #[cfg(target_os = "linux")]
    fn legacy_desktop_file() -> Option<std::path::PathBuf> {
        directories::BaseDirs::new().map(|b| b.config_dir().join("autostart/linkshot.desktop"))
    }
    /// The program autostart should launch: the installed package when there is one, even while a
    /// development build is running (a build from source must not take over the login session).
    #[cfg(target_os = "linux")]
    fn startup_exe() -> Option<std::path::PathBuf> {
        let exe = std::env::current_exe().ok()?;
        if exe.starts_with("/usr/") || exe.starts_with("/opt/") { return Some(exe); }
        ["/usr/bin/linkshot", "/opt/linkshot/linkshot"].iter().map(std::path::PathBuf::from).find(|p| p.is_file()).or(Some(exe))
    }
    #[cfg(target_os = "linux")]
    fn entry(exe: &std::path::Path) -> String {
        let exe = if exe.to_string_lossy().contains(' ') { format!("\"{}\"", exe.display()) } else { exe.display().to_string() };
        format!("[Desktop Entry]\nType=Application\nName=LinkShot\nExec={exe} --background\nIcon=linkshot\nX-GNOME-Autostart-enabled=true\n")
    }

    /// Moves a legacy autostart entry to the current name and points an entry whose program no longer
    /// exists (a moved build, a reinstalled package) at the running executable.
    pub fn repair() {
        #[cfg(target_os = "linux")]
        {
            let (Some(p), Some(old)) = (desktop_file(), legacy_desktop_file()) else { return };
            let Some(exe) = startup_exe() else { return };
            if old.exists() {
                let _ = std::fs::remove_file(&old);
                if let Some(dir) = p.parent() { let _ = std::fs::create_dir_all(dir); }
                let _ = std::fs::write(&p, entry(&exe));
                return;
            }
            let Ok(text) = std::fs::read_to_string(&p) else { return };
            let target = text.lines().find_map(|l| l.strip_prefix("Exec=")).map(|e| {
                let e = e.trim();
                if let Some(rest) = e.strip_prefix('"') { rest.split('"').next().unwrap_or("").to_string() } else { e.split_whitespace().next().unwrap_or("").to_string() }
            });
            // Rewrite when the program is gone, or when a package is installed and the entry still
            // starts some other build (a development copy).
            let installed = exe.starts_with("/usr/") || exe.starts_with("/opt/");
            let stale = match &target {
                None => true,
                Some(t) => !std::path::Path::new(t).exists() || (installed && std::path::Path::new(t) != exe.as_path()),
            };
            if stale { let _ = std::fs::write(&p, entry(&exe)); }
        }
    }

    pub fn is_enabled() -> bool {
        #[cfg(target_os = "linux")]
        { desktop_file().map(|p| p.exists()).unwrap_or(false) || legacy_desktop_file().map(|p| p.exists()).unwrap_or(false)}
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
                let exe = startup_exe().ok_or_else(|| anyhow::anyhow!("no executable"))?;
                std::fs::write(&p, entry(&exe))?;
            } else if p.exists() {
                std::fs::remove_file(p)?;
            }
            if let Some(old) = legacy_desktop_file() { if old.exists() { let _ = std::fs::remove_file(old); } }
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
