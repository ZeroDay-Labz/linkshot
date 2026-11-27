//! Tries the update path without installing: `updatecheck [download] [rpm|deb|flatpak|exe]`.
//! `LINKSHOT_UPDATE_PRETEND_VERSION=0.0.1` makes any release look newer.
use linkshot_platform::update::{self, InstallKind};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let kind = match args.get(1).map(String::as_str) {
        Some("rpm") => InstallKind::Rpm, Some("deb") => InstallKind::Deb, Some("flatpak") => InstallKind::Flatpak { system: true }, Some("exe") => InstallKind::WindowsExe,
        _ => update::install_kind(),
    };
    println!("current {} ({:?})", update::current_version(), kind);
    let r = update::latest_release()?;
    println!("latest {} newer={} page={}", r.version, r.is_newer(), r.page_url);
    let Some(a) = r.asset_for(kind) else { println!("no package for {kind:?}"); return Ok(()) };
    println!("asset {} ({} bytes)", a.name, a.size);
    if args.first().map(String::as_str) == Some("download") {
        let dir = std::env::temp_dir().join("linkshot-updatecheck");
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let mut last = 0;
        let f = update::download(a, &dir, |g, t| { let pct = if t > 0 { g * 100 / t } else { 0 }; if pct >= last + 25 { last = pct; println!("  {pct}%"); } }, &cancel)?;
        update::verify(&r, a, &f)?;
        println!("downloaded and verified {}", f.display());
        let _ = std::fs::remove_dir_all(dir);
    }
    Ok(())
}
