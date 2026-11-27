//! Updates from the project's GitHub releases. Nothing here runs on its own: the app calls it when
//! the user chooses *Check for Updates* (or opted in to a check at start-up).
//!
//! The package that matches how LinkShot was installed is downloaded from the release, checked
//! against the release's `SHA256SUMS.txt`, and installed with the system's own tool: `dnf`/`rpm`
//! or `apt-get` behind the polkit password prompt, `flatpak install` on the host, or by swapping
//! the portable Windows executable. Builds that no package manager owns (a development build) are
//! sent to the release page instead.

use anyhow::{bail, Context, Result};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// `owner/name` of the GitHub repository releases come from.
pub const REPO: &str = "ZeroDay-Labz/linkshot";

/// How this copy of LinkShot was installed, which decides how it is updated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallKind {
    Rpm,
    Deb,
    /// Flatpak; `system` is false for a per-user installation.
    Flatpak { system: bool },
    /// The portable Windows executable.
    WindowsExe,
    /// Not owned by a package manager (a development build): updates open the release page.
    Unmanaged,
}

impl InstallKind {
    pub fn label(self) -> &'static str {
        match self {
            InstallKind::Rpm => "RPM package",
            InstallKind::Deb => "Debian package",
            InstallKind::Flatpak { .. } => "Flatpak",
            InstallKind::WindowsExe => "Windows executable",
            InstallKind::Unmanaged => "development build",
        }
    }
}

pub fn install_kind() -> InstallKind {
    // Test override, to try a package update from a development build: rpm, deb, flatpak.
    match std::env::var("LINKSHOT_UPDATE_PRETEND_KIND").as_deref() {
        Ok("rpm") => return InstallKind::Rpm,
        Ok("deb") => return InstallKind::Deb,
        Ok("flatpak") => return InstallKind::Flatpak { system: true },
        _ => {}
    }
    if cfg!(windows) { return InstallKind::WindowsExe; }
    if std::env::var_os("FLATPAK_ID").is_some() {
        // /.flatpak-info names where the app is deployed: /var/lib/flatpak (system) or ~/.local/share/flatpak.
        let info = std::fs::read_to_string("/.flatpak-info").unwrap_or_default();
        let system = !info.lines().any(|l| l.starts_with("app-path=") && l.contains("/.local/share/flatpak/"));
        return InstallKind::Flatpak { system };
    }
    let Ok(exe) = std::env::current_exe() else { return InstallKind::Unmanaged };
    let exe = std::fs::canonicalize(&exe).unwrap_or(exe);
    let owned_by = |prog: &str, args: &[&str]| std::process::Command::new(prog).args(args).arg(&exe).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status().map(|s| s.success()).unwrap_or(false);
    if owned_by("rpm", &["-qf"]) { return InstallKind::Rpm; }
    if owned_by("dpkg", &["-S"]) { return InstallKind::Deb; }
    InstallKind::Unmanaged
}

#[derive(Clone, Debug)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
}

#[derive(Clone, Debug)]
pub struct Release {
    pub version: semver::Version,
    pub tag: String,
    /// The release's page on GitHub.
    pub page_url: String,
    /// Release notes (Markdown).
    pub notes: String,
    pub assets: Vec<Asset>,
}

/// The version of this build. `LINKSHOT_UPDATE_PRETEND_VERSION` overrides it, so the update flow
/// can be tried against the current release.
pub fn current_version() -> semver::Version {
    let v = std::env::var("LINKSHOT_UPDATE_PRETEND_VERSION").ok().unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());
    semver::Version::parse(v.trim_start_matches('v')).unwrap_or_else(|_| semver::Version::new(0, 0, 0))
}

fn user_agent() -> String { format!("LinkShot/{}", env!("CARGO_PKG_VERSION")) }

fn get(url: &str, timeout: Duration) -> Result<ureq::http::Response<ureq::Body>> {
    ureq::get(url)
        .config().timeout_global(Some(timeout)).build()
        .header("User-Agent", user_agent())
        .header("Accept", "application/vnd.github+json")
        .call()
        .with_context(|| format!("GET {url}"))
}

/// The newest published release.
pub fn latest_release() -> Result<Release> {
    let mut resp = get(&format!("https://api.github.com/repos/{REPO}/releases/latest"), Duration::from_secs(15))?;
    let text = resp.body_mut().read_to_string().context("read the release information")?;
    parse_release(&text)
}

fn parse_release(json: &str) -> Result<Release> {
    let v: serde_json::Value = serde_json::from_str(json).context("release information is not JSON")?;
    let tag = v["tag_name"].as_str().context("release has no tag")?.to_string();
    let version = semver::Version::parse(tag.trim_start_matches('v')).with_context(|| format!("release tag {tag} is not a version"))?;
    let assets = v["assets"].as_array().map(|a| a.iter().filter_map(|x| Some(Asset {
        name: x["name"].as_str()?.to_string(),
        url: x["browser_download_url"].as_str()?.to_string(),
        size: x["size"].as_u64().unwrap_or(0),
    })).collect()).unwrap_or_default();
    Ok(Release {
        version,
        page_url: v["html_url"].as_str().map(str::to_string).unwrap_or_else(|| format!("https://github.com/{REPO}/releases/tag/{tag}")),
        notes: v["body"].as_str().unwrap_or_default().to_string(),
        tag,
        assets,
    })
}

impl Release {
    pub fn is_newer(&self) -> bool { self.version > current_version() }

    /// The package for `kind` (release builds are x86-64 only).
    pub fn asset_for(&self, kind: InstallKind) -> Option<&Asset> {
        if !cfg!(target_arch = "x86_64") { return None; }
        let wanted = |n: &str| match kind {
            InstallKind::Rpm => n.ends_with(".x86_64.rpm"),
            InstallKind::Deb => n.ends_with("_amd64.deb"),
            InstallKind::Flatpak { .. } => n.ends_with(".flatpak"),
            InstallKind::WindowsExe => n.ends_with("-windows-x64.exe"),
            InstallKind::Unmanaged => false,
        };
        self.assets.iter().find(|a| wanted(&a.name))
    }
}

/// Where downloads go. Inside the Flatpak the cache directory is also visible to the host under the
/// same path (`~/.var/app/<id>/cache`), which `flatpak install` on the host needs.
pub fn download_dir() -> PathBuf {
    directories::BaseDirs::new().map(|b| b.cache_dir().join("linkshot").join("updates")).unwrap_or_else(|| std::env::temp_dir().join("linkshot-updates"))
}

/// Downloads `asset` into `dir`, calling `progress(received, total)`; `cancel` aborts it.
pub fn download(asset: &Asset, dir: &Path, mut progress: impl FnMut(u64, u64), cancel: &AtomicBool) -> Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let dest = dir.join(&asset.name);
    let part = dir.join(format!("{}.part", asset.name));
    let mut resp = get(&asset.url, Duration::from_secs(30 * 60))?;
    let total = resp.body().content_length().unwrap_or(asset.size);
    let mut reader = resp.body_mut().with_config().limit(1 << 30).reader();
    let mut out = std::fs::File::create(&part).with_context(|| format!("create {}", part.display()))?;
    let mut buf = vec![0u8; 64 * 1024];
    let mut got = 0u64;
    loop {
        if cancel.load(Ordering::Relaxed) { drop(out); let _ = std::fs::remove_file(&part); bail!("cancelled"); }
        let n = reader.read(&mut buf).context("download")?;
        if n == 0 { break; }
        out.write_all(&buf[..n])?;
        got += n as u64;
        progress(got, total);
    }
    out.flush()?;
    drop(out);
    if asset.size > 0 && got != asset.size { let _ = std::fs::remove_file(&part); bail!("download incomplete ({got} of {} bytes)", asset.size); }
    std::fs::rename(&part, &dest)?;
    Ok(dest)
}

/// Checks `file` against the release's `SHA256SUMS.txt`. A release without checksums is refused.
pub fn verify(release: &Release, asset: &Asset, file: &Path) -> Result<()> {
    let sums = release.assets.iter().find(|a| a.name == "SHA256SUMS.txt").context("the release has no SHA256SUMS.txt; not installing an unverified download")?;
    let text = get(&sums.url, Duration::from_secs(30))?.body_mut().read_to_string()?;
    let expected = expected_sum(&text, &asset.name).with_context(|| format!("SHA256SUMS.txt has no entry for {}", asset.name))?;
    let actual = sha256_file(file)?;
    if !actual.eq_ignore_ascii_case(&expected) { bail!("checksum mismatch for {} (expected {expected}, got {actual}); the download was discarded", asset.name); }
    Ok(())
}

fn expected_sum(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let mut it = l.split_whitespace();
        let (sum, file) = (it.next()?, it.next()?);
        (file.trim_start_matches('*') == name).then(|| sum.to_string())
    })
}

fn sha256_file(path: &Path) -> Result<String> {
    use sha2::Digest;
    let mut f = std::fs::File::open(path)?;
    let mut h = sha2::Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop { let n = f.read(&mut buf)?; if n == 0 { break; } h.update(&buf[..n]); }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

fn have(prog: &str) -> bool {
    std::env::var_os("PATH").map(|p| std::env::split_paths(&p).any(|d| d.join(prog).is_file())).unwrap_or(false)
}

fn run(cmd: &mut std::process::Command, what: &str) -> Result<()> {
    let out = cmd.output().with_context(|| format!("start {what}"))?;
    if out.status.success() { return Ok(()); }
    // pkexec: 126 = the password prompt was dismissed, 127 = not authorised.
    if matches!(out.status.code(), Some(126) | Some(127)) && what.starts_with("pkexec") { bail!("the update was not authorised (password prompt cancelled)"); }
    let err = String::from_utf8_lossy(&out.stderr);
    bail!("{what} failed: {}", err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("unknown error").trim())
}

/// Installs a verified download. Blocks while the system asks for the password.
pub fn install(kind: InstallKind, file: &Path) -> Result<()> {
    match kind {
        InstallKind::Rpm => {
            if have("dnf") { run(std::process::Command::new("pkexec").args(["dnf", "install", "-y"]).arg(file), "pkexec dnf install") }
            else { run(std::process::Command::new("pkexec").args(["rpm", "-U", "--replacepkgs"]).arg(file), "pkexec rpm -U") }
        }
        InstallKind::Deb => run(std::process::Command::new("pkexec").args(["apt-get", "install", "-y"]).arg(file), "pkexec apt-get install"),
        InstallKind::Flatpak { system } => run(
            std::process::Command::new("flatpak-spawn").args(["--host", "flatpak", "install", "-y", "--noninteractive", "--reinstall", if system { "--system" } else { "--user" }]).arg(file),
            "flatpak install",
        ),
        InstallKind::WindowsExe => {
            let exe = std::env::current_exe()?;
            let old = exe.with_extension("exe.old");
            let _ = std::fs::remove_file(&old);
            // A running executable cannot be overwritten on Windows, but it can be renamed.
            std::fs::rename(&exe, &old).context("move the running executable aside")?;
            if let Err(e) = std::fs::copy(file, &exe) { let _ = std::fs::rename(&old, &exe); return Err(e).context("put the new executable in place"); }
            Ok(())
        }
        InstallKind::Unmanaged => bail!("this build is not managed by a package manager"),
    }
}

/// Removes what an earlier update left behind (the old Windows executable, downloaded packages).
pub fn clean_up() {
    if cfg!(windows) {
        if let Ok(exe) = std::env::current_exe() { let _ = std::fs::remove_file(exe.with_extension("exe.old")); }
    }
    let _ = std::fs::remove_dir_all(download_dir());
}

/// Starts the updated LinkShot a moment after this process exits (the caller then quits).
pub fn relaunch(kind: InstallKind) -> Result<()> {
    match kind {
        InstallKind::Flatpak { .. } => {
            let id = std::env::var("FLATPAK_ID").unwrap_or_else(|_| crate::APP_ID.to_string());
            std::process::Command::new("flatpak-spawn").args(["--host", "sh", "-c", &format!("sleep 1; exec flatpak run {id}")]).spawn().context("relaunch")?;
        }
        InstallKind::WindowsExe => {
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                let exe = std::env::current_exe()?;
                std::process::Command::new("cmd").args(["/C", &format!("timeout /T 2 /NOBREAK >NUL & start \"\" \"{}\"", exe.display())]).creation_flags(0x0800_0000).spawn().context("relaunch")?;
            }
        }
        _ => {
            let exe = if matches!(kind, InstallKind::Rpm | InstallKind::Deb) { PathBuf::from("/usr/bin/linkshot") } else { std::env::current_exe()? };
            std::process::Command::new("sh").args(["-c", "sleep 1; exec \"$0\"", &exe.to_string_lossy()]).spawn().context("relaunch")?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_github_release() {
        let json = r#"{"tag_name":"v0.6.0","html_url":"https://github.com/ZeroDay-Labz/linkshot/releases/tag/v0.6.0","body":"notes","assets":[
            {"name":"linkshot-0.6.0-1.x86_64.rpm","browser_download_url":"https://example.invalid/a.rpm","size":10},
            {"name":"linkshot_0.6.0-1_amd64.deb","browser_download_url":"https://example.invalid/a.deb","size":11},
            {"name":"LinkShot-0.6.0.flatpak","browser_download_url":"https://example.invalid/a.flatpak","size":12},
            {"name":"LinkShot-0.6.0-windows-x64.exe","browser_download_url":"https://example.invalid/a.exe","size":13},
            {"name":"SHA256SUMS.txt","browser_download_url":"https://example.invalid/s","size":1}]}"#;
        let r = parse_release(json).unwrap();
        assert_eq!(r.version, semver::Version::new(0, 6, 0));
        assert_eq!(r.asset_for(InstallKind::Rpm).unwrap().size, 10);
        assert_eq!(r.asset_for(InstallKind::Deb).unwrap().size, 11);
        assert_eq!(r.asset_for(InstallKind::Flatpak { system: true }).unwrap().size, 12);
        assert_eq!(r.asset_for(InstallKind::WindowsExe).unwrap().size, 13);
        assert!(r.asset_for(InstallKind::Unmanaged).is_none());
    }

    #[test]
    fn finds_checksums() {
        let sums = "abc123  linkshot-0.6.0-1.x86_64.rpm\ndef456 *LinkShot-0.6.0-windows-x64.exe\n";
        assert_eq!(expected_sum(sums, "linkshot-0.6.0-1.x86_64.rpm").as_deref(), Some("abc123"));
        assert_eq!(expected_sum(sums, "LinkShot-0.6.0-windows-x64.exe").as_deref(), Some("def456"));
        assert_eq!(expected_sum(sums, "other"), None);
    }

    #[test]
    fn hashes_files() {
        let p = std::env::temp_dir().join(format!("linkshot-sha-test-{}", std::process::id()));
        std::fs::write(&p, b"abc").unwrap();
        assert_eq!(sha256_file(&p).unwrap(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        let _ = std::fs::remove_file(p);
    }
}
