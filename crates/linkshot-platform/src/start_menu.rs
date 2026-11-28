//! Windows: a Start menu entry for the single-file executable, so LinkShot can be found and pinned
//! like an installed program. Written on start when missing, and pointed at the running executable
//! when the old one moved or was replaced.

use anyhow::{Context, Result};
use windows::core::{Interface, HSTRING};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, IPersistFile, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};

fn shortcut_path() -> Option<std::path::PathBuf> {
    std::env::var_os("APPDATA").map(|a| std::path::PathBuf::from(a).join(r"Microsoft\Windows\Start Menu\Programs\LinkShot.lnk"))
}

/// Creates or repairs `Start Menu\Programs\LinkShot.lnk`. Returns true when it was written.
pub fn ensure() -> Result<bool> {
    let exe = std::env::current_exe()?;
    let path = shortcut_path().context("no APPDATA")?;
    // The target is recorded beside the shortcut, so an unchanged shortcut costs no COM call.
    let marker = path.with_extension("target");
    if path.exists() && std::fs::read_to_string(&marker).map(|t| t == exe.to_string_lossy()).unwrap_or(false) { return Ok(false); }
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).context("shell link")?;
        link.SetPath(&HSTRING::from(exe.as_os_str()))?;
        if let Some(dir) = exe.parent() { link.SetWorkingDirectory(&HSTRING::from(dir.as_os_str()))?; }
        link.SetDescription(&HSTRING::from("Screenshots, scrolling capture, screen recording and an image editor"))?;
        link.SetIconLocation(&HSTRING::from(exe.as_os_str()), 0)?;
        let file: IPersistFile = link.cast()?;
        if let Some(dir) = path.parent() { std::fs::create_dir_all(dir)?; }
        file.Save(&HSTRING::from(path.as_os_str()), true).context("save the Start menu shortcut")?;
    }
    let _ = std::fs::write(&marker, exe.to_string_lossy().as_bytes());
    Ok(true)
}
