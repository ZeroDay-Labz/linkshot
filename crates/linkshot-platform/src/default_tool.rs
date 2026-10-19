//! Making LinkShot the system's screenshot tool: the Print Screen key starts a LinkShot capture
//! system-wide (launching LinkShot if it is not running) and LinkShot starts at login.
//!
//! * KDE Plasma (X11 and Wayland): through `kglobalaccel`'s D-Bus interface. Whatever currently
//!   owns Print (Spectacle, or a stale registration) is unbound, and Print is bound to the
//!   `capture` desktop action of LinkShot's desktop file (`linkshot --capture`), which
//!   kglobalaccel launches itself, so it works even when LinkShot is closed.
//! * GNOME: `gsettings` clears the Shell's screenshot bindings and adds a custom media-key
//!   binding for Print running `linkshot --capture`.
//! * Windows: the "Use the Print screen key to open Snipping Tool" setting is turned off so the
//!   in-app hotkey receives the key; LinkShot is added to the Run key.
//!
//! On other desktops the in-app hotkey (X11 key grab) is all there is.

use anyhow::{bail, Context, Result};
use std::process::Command;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Desktop { Kde, Gnome, OtherLinux, Windows, Other }

pub fn desktop() -> Desktop {
    if cfg!(windows) { return Desktop::Windows; }
    if !cfg!(target_os = "linux") { return Desktop::Other; }
    let d = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default().to_uppercase();
    if d.contains("KDE") { Desktop::Kde } else if d.contains("GNOME") || d.contains("UBUNTU") { Desktop::Gnome } else { Desktop::OtherLinux }
}

/// What this desktop can do, for the prompt text.
pub fn supported() -> bool { matches!(desktop(), Desktop::Kde | Desktop::Gnome | Desktop::Windows) }

/// Human-readable outcome of [`make_default`] / [`undo`].
#[derive(Clone, Debug, Default)]
pub struct Report { pub steps: Vec<String> }

impl Report {
    fn step(&mut self, s: impl Into<String>) { let s = s.into(); tracing::info!("default tool: {s}"); self.steps.push(s); }
}

/// Identity of LinkShot's desktop entry / executable as the desktop sees it.
#[derive(Clone, Debug)]
pub struct Identity {
    /// Desktop file id without `.desktop` ("linkshot", or the Flatpak app id).
    pub desktop_id: String,
    /// Command that starts a capture from the outside.
    pub capture_command: Vec<String>,
}

impl Identity {
    pub fn detect() -> Self {
        if let Ok(id) = std::env::var("FLATPAK_ID") {
            return Self { desktop_id: id.clone(), capture_command: vec!["flatpak".into(), "run".into(), format!("--command=linkshot"), id, "--capture".into()] };
        }
        let exe = std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| "linkshot".into());
        Self { desktop_id: "linkshot".into(), capture_command: vec![exe, "--capture".into()] }
    }
    fn service_component(&self) -> String { format!("{}.desktop", self.desktop_id) }
}

/// Is Print Screen currently bound to LinkShot system-wide?
pub fn is_default(id: &Identity) -> bool {
    match desktop() {
        Desktop::Kde => kde::holders_of_print().map(|h| h.iter().any(|x| x.component == id.service_component())).unwrap_or(false),
        Desktop::Gnome => gnome::is_default(),
        Desktop::Windows => windows::is_default(),
        _ => false,
    }
}

/// Binds Print Screen to LinkShot system-wide and enables start at login.
pub fn make_default(id: &Identity) -> Result<Report> {
    let mut r = Report::default();
    match desktop() {
        Desktop::Kde => kde::make_default(id, &mut r)?,
        Desktop::Gnome => gnome::make_default(id, &mut r)?,
        Desktop::Windows => windows::make_default(&mut r)?,
        Desktop::OtherLinux | Desktop::Other => bail!("this desktop is not supported for system-wide Print Screen; the in-app hotkey is used instead"),
    }
    match crate::autostart::set_enabled(true) { Ok(()) => r.step("LinkShot starts at login"), Err(e) => r.step(format!("start at login not enabled: {e}")) }
    Ok(r)
}

/// Removes the system-wide binding again (the desktop's own screenshot tool is not restored).
pub fn undo(id: &Identity) -> Result<Report> {
    let mut r = Report::default();
    match desktop() {
        Desktop::Kde => kde::undo(id, &mut r)?,
        Desktop::Gnome => gnome::undo(&mut r)?,
        Desktop::Windows => windows::undo(&mut r)?,
        _ => {}
    }
    Ok(r)
}

#[cfg(target_os = "linux")]
fn run(cmd: &[&str]) -> Result<String> {
    let (prog, args) = cmd.split_first().context("empty command")?;
    // Inside a Flatpak the desktop's settings tools live on the host.
    let out = if std::path::Path::new("/.flatpak-info").exists() {
        Command::new("flatpak-spawn").arg("--host").arg(prog).args(args).output()
    } else {
        Command::new(prog).args(args).output()
    }.with_context(|| format!("run {prog}"))?;
    if !out.status.success() { bail!("{prog} failed: {}", String::from_utf8_lossy(&out.stderr).trim()); }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(target_os = "linux")]
pub mod kde {
    use super::{Identity, Report};
    use anyhow::{Context, Result};

    /// `Qt::Key_Print`.
    pub const PRINT: i32 = 0x0100_0009;
    const SERVICE: &str = "org.kde.kglobalaccel";
    const PATH: &str = "/kglobalaccel";
    const IFACE: &str = "org.kde.KGlobalAccel";

    #[derive(Clone, Debug)]
    pub struct Holder { pub component: String, pub component_friendly: String, pub action: String, pub action_friendly: String }

    fn conn() -> Result<zbus::blocking::Connection> { zbus::blocking::Connection::session().context("session bus") }

    /// Every action currently bound to Print (any modifier-free match).
    pub fn holders_of_print() -> Result<Vec<Holder>> {
        let c = conn()?;
        let reply = c.call_method(Some(SERVICE), PATH, Some(IFACE), "globalShortcutsByKey", &((vec![PRINT],), (0i32,))).context("globalShortcutsByKey")?;
        let infos: Vec<(String, String, String, String, String, String, Vec<i32>, Vec<i32>)> = reply.body().deserialize().context("decode shortcut infos")?;
        Ok(infos.into_iter().map(|(action, action_friendly, component, component_friendly, _, _, _, _)| Holder { component, component_friendly, action, action_friendly }).collect())
    }

    fn set_keys(c: &zbus::blocking::Connection, component: &str, action: &str, cf: &str, af: &str, keys: Vec<i32>) -> Result<()> {
        let id = vec![component.to_string(), action.to_string(), cf.to_string(), af.to_string()];
        c.call_method(Some(SERVICE), PATH, Some(IFACE), "setForeignShortcut", &(id, keys)).map(|_| ()).with_context(|| format!("setForeignShortcut {component}/{action}"))
    }

    pub fn make_default(id: &Identity, r: &mut Report) -> Result<()> {
        let c = conn()?;
        let ours = id.service_component();
        for h in holders_of_print()? {
            if h.component == ours && h.action == "capture" { continue; }
            set_keys(&c, &h.component, &h.action, &h.component_friendly, &h.action_friendly, vec![])?;
            r.step(format!("Print Screen taken away from {} ({})", h.component_friendly, h.action_friendly));
            // A LinkShot shortcut that the portal filed under another application's name (it happens
            // when LinkShot was started from an IDE or terminal launcher) is removed entirely.
            if h.action == "capture" && h.action_friendly == "Capture" && h.component != "linkshot" {
                let _ = c.call_method(Some(SERVICE), PATH, Some(IFACE), "unregister", &(h.component.as_str(), h.action.as_str()));
            }
        }
        let action_id = vec![ours.clone(), "capture".to_string(), "LinkShot".to_string(), "Capture now".to_string()];
        c.call_method(Some(SERVICE), PATH, Some(IFACE), "doRegister", &(action_id.clone(),)).context("doRegister")?;
        c.call_method(Some(SERVICE), PATH, Some(IFACE), "setForeignShortcut", &(action_id, vec![PRINT])).context("setForeignShortcut")?;
        let ok = holders_of_print()?.iter().any(|h| h.component == ours);
        anyhow::ensure!(ok, "kglobalaccel did not accept the binding");
        r.step("Print Screen now starts a LinkShot capture (KDE global shortcut)");
        Ok(())
    }

    pub fn undo(id: &Identity, r: &mut Report) -> Result<()> {
        let c = conn()?;
        let ours = id.service_component();
        let _ = c.call_method(Some(SERVICE), PATH, Some(IFACE), "unregister", &(ours.as_str(), "capture"));
        r.step("Print Screen no longer bound to LinkShot");
        Ok(())
    }
}

#[cfg(target_os = "linux")]
pub mod gnome {
    use super::{run, Identity, Report};
    use anyhow::Result;

    const MEDIA_KEYS: &str = "org.gnome.settings-daemon.plugins.media-keys";
    const OURS: &str = "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/linkshot/";
    fn schema() -> String { format!("{MEDIA_KEYS}.custom-keybinding:{OURS}") }

    pub fn is_default() -> bool {
        run(&["gsettings", "get", &schema(), "binding"]).map(|v| v.contains("Print")).unwrap_or(false)
    }

    pub fn make_default(id: &Identity, r: &mut Report) -> Result<()> {
        for key in ["show-screenshot-ui", "screenshot", "screenshot-window", "show-screen-recording-ui"] {
            if run(&["gsettings", "set", "org.gnome.shell.keybindings", key, "[]"]).is_ok() { r.step(format!("GNOME Shell {key} shortcut cleared")); }
        }
        let list = run(&["gsettings", "get", MEDIA_KEYS, "custom-keybindings"]).unwrap_or_else(|_| "[]".into());
        if !list.contains(OURS) {
            let mut items: Vec<String> = list.trim().trim_start_matches("@as ").trim_start_matches('[').trim_end_matches(']').split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
            items.push(format!("'{OURS}'"));
            run(&["gsettings", "set", MEDIA_KEYS, "custom-keybindings", &format!("[{}]", items.join(", "))])?;
        }
        let command = id.capture_command.iter().map(|a| if a.contains(' ') { format!("\"{a}\"") } else { a.clone() }).collect::<Vec<_>>().join(" ");
        run(&["gsettings", "set", &schema(), "name", "LinkShot capture"])?;
        run(&["gsettings", "set", &schema(), "command", &command])?;
        run(&["gsettings", "set", &schema(), "binding", "Print"])?;
        r.step("Print Screen now starts a LinkShot capture (GNOME custom shortcut)");
        Ok(())
    }

    pub fn undo(r: &mut Report) -> Result<()> {
        let _ = run(&["gsettings", "set", &schema(), "binding", "''"]);
        r.step("Print Screen no longer bound to LinkShot");
        Ok(())
    }
}

#[cfg(not(target_os = "linux"))]
pub mod kde {
    pub fn holders_of_print() -> anyhow::Result<Vec<Holder>> { Ok(vec![]) }
    pub struct Holder { pub component: String }
    pub fn make_default(_: &super::Identity, _: &mut super::Report) -> anyhow::Result<()> { anyhow::bail!("not KDE") }
    pub fn undo(_: &super::Identity, _: &mut super::Report) -> anyhow::Result<()> { Ok(()) }
}
#[cfg(not(target_os = "linux"))]
pub mod gnome {
    pub fn is_default() -> bool { false }
    pub fn make_default(_: &super::Identity, _: &mut super::Report) -> anyhow::Result<()> { anyhow::bail!("not GNOME") }
    pub fn undo(_: &mut super::Report) -> anyhow::Result<()> { Ok(()) }
}

pub mod windows {
    use super::Report;
    use anyhow::Result;

    #[cfg(windows)]
    fn reg(args: &[&str]) -> Result<String> {
        use std::os::windows::process::CommandExt;
        let out = super::Command::new("reg").args(args).creation_flags(0x0800_0000).output()?;
        anyhow::ensure!(out.status.success(), "reg {} failed: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim());
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    /// True when Windows no longer hands Print Screen to the Snipping Tool.
    pub fn is_default() -> bool {
        #[cfg(windows)]
        { return reg(&["query", r"HKCU\Control Panel\Keyboard", "/v", "PrintScreenKeyForSnippingEnabled"]).map(|o| o.contains("0x0")).unwrap_or(true); }
        #[cfg(not(windows))]
        { false }
    }

    pub fn make_default(r: &mut Report) -> Result<()> {
        #[cfg(windows)]
        {
            reg(&["add", r"HKCU\Control Panel\Keyboard", "/v", "PrintScreenKeyForSnippingEnabled", "/t", "REG_DWORD", "/d", "0", "/f"])?;
            r.step("Print Screen no longer opens the Snipping Tool; LinkShot's hotkey receives it");
            Ok(())
        }
        #[cfg(not(windows))]
        { let _ = r; anyhow::bail!("not Windows") }
    }

    pub fn undo(r: &mut Report) -> Result<()> {
        #[cfg(windows)]
        { let _ = reg(&["add", r"HKCU\Control Panel\Keyboard", "/v", "PrintScreenKeyForSnippingEnabled", "/t", "REG_DWORD", "/d", "1", "/f"]); }
        r.step("Print Screen handed back to the Snipping Tool");
        Ok(())
    }
}
