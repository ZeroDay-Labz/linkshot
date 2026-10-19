//! Finds the helper programs and data files LinkShot ships with so that nothing has to be
//! installed separately: `ffmpeg`/`ffprobe` (MP4 export and import), `tesseract` with its
//! language data (Grab Text) and the fallback `ocrs` models.
//!
//! The bundle is a directory with this layout (every entry optional):
//!
//! ```text
//! ffmpeg(.exe)  ffprobe(.exe)
//! tesseract/tesseract(.exe) [+ *.dll | lib/*.so]  tesseract/tessdata/*.traineddata
//! ocrs/text-detection.rten  ocrs/text-recognition.rten
//! ```
//!
//! It is looked for in `$LINKSHOT_TOOLS`, next to the executable (`tools/` or the directory
//! itself), in `../lib/linkshot` and `../share/linkshot` relative to the executable (deb/rpm and
//! Flatpak layouts), and, with the `embedded` feature, in the copy unpacked from the executable
//! itself. Anything not found there is finally looked up on `PATH`.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

/// A located program plus the environment it needs to run from the bundle.
#[derive(Clone, Debug)]
pub struct Tool {
    pub path: PathBuf,
    pub env: Vec<(String, String)>,
    /// True when it came from LinkShot's own bundle rather than the system.
    pub bundled: bool,
}

impl Tool {
    /// A `Command` for the tool with its environment applied (and no console window on Windows).
    pub fn command(&self) -> Command {
        let mut c = Command::new(&self.path);
        for (k, v) in &self.env { c.env(k, v); }
        #[cfg(windows)]
        { use std::os::windows::process::CommandExt; c.creation_flags(0x0800_0000); }
        c
    }
}

fn exe_name(name: &str) -> String { if cfg!(windows) { format!("{name}.exe") } else { name.to_string() } }

/// Candidate bundle directories, most specific first. Only existing directories are returned.
pub fn dirs() -> Vec<PathBuf> {
    static DIRS: OnceLock<Vec<PathBuf>> = OnceLock::new();
    DIRS.get_or_init(|| {
        let mut out: Vec<PathBuf> = Vec::new();
        if let Some(p) = std::env::var_os("LINKSHOT_TOOLS") { out.push(PathBuf::from(p)); }
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                out.push(dir.join("tools"));
                out.push(dir.to_path_buf());
                if let Some(prefix) = dir.parent() {
                    out.push(prefix.join("lib").join("linkshot"));
                    out.push(prefix.join("lib64").join("linkshot"));
                    out.push(prefix.join("share").join("linkshot"));
                }
            }
        }
        #[cfg(feature = "embedded")]
        if let Some(p) = embedded::dir() { out.push(p); }
        out.into_iter().filter(|p| p.is_dir()).collect()
    }).clone()
}

/// First existing `dir/<rel>` among the bundle directories.
pub fn resource(rel: &str) -> Option<PathBuf> {
    dirs().into_iter().map(|d| d.join(rel)).find(|p| p.exists())
}

/// Finds `name` (e.g. "ffmpeg", "ffprobe", "tesseract"): `$LINKSHOT_<NAME>` first, then the bundle,
/// then `PATH`.
pub fn find(name: &str) -> Option<Tool> {
    let env_key = format!("LINKSHOT_{}", name.to_ascii_uppercase());
    if let Ok(p) = std::env::var(&env_key) { let p = PathBuf::from(p); if p.is_file() { return Some(Tool { path: p, env: vec![], bundled: false }); } }
    let file = exe_name(name);
    for dir in dirs() {
        // `<name>/AppRun` is a relocatable AppImage tree (its launcher sets up loader and data paths).
        for rel in [dir.join(&file), dir.join(name).join(&file), dir.join(name).join("bin").join(&file), dir.join(name).join("AppRun")] {
            if rel.is_file() { return Some(bundled_tool(name, rel)); }
        }
    }
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let p = dir.join(&file);
        if p.is_file() { return Some(Tool { path: p, env: vec![], bundled: false }); }
    }
    None
}

/// Environment for a bundled program: shared-library directory and, for tesseract, its language data.
fn bundled_tool(name: &str, path: PathBuf) -> Tool {
    let mut env = Vec::new();
    let bin_dir = path.parent().unwrap_or(Path::new("."));
    #[cfg(not(windows))]
    {
        let lib_dirs: Vec<PathBuf> = [bin_dir.join("lib"), bin_dir.join("../lib"), bin_dir.join("../lib64")].into_iter().filter(|p| p.is_dir()).collect();
        if !lib_dirs.is_empty() {
            let mut paths: Vec<PathBuf> = lib_dirs;
            if let Some(old) = std::env::var_os("LD_LIBRARY_PATH") { paths.extend(std::env::split_paths(&old)); }
            if let Ok(joined) = std::env::join_paths(paths) { env.push(("LD_LIBRARY_PATH".into(), joined.to_string_lossy().into_owned())); }
        }
    }
    if name == "tesseract" {
        for cand in [bin_dir.join("tessdata"), bin_dir.join("usr/share/tesseract-ocr/5/tessdata"), bin_dir.join("../share/tessdata"), bin_dir.join("../share/tesseract-ocr/5/tessdata")] {
            if cand.is_dir() { env.push(("TESSDATA_PREFIX".into(), cand.to_string_lossy().into_owned())); break; }
        }
    }
    Tool { path, env, bundled: true }
}

/// Directory holding the bundled `ocrs` models, if the bundle has them.
pub fn ocrs_model_dir() -> Option<PathBuf> {
    resource("ocrs").filter(|d| d.join("text-detection.rten").is_file() && d.join("text-recognition.rten").is_file())
}

/// Unpacks the embedded bundle (if this build has one) so [`dirs`] can see it. Cheap when already done.
pub fn prepare() {
    #[cfg(feature = "embedded")]
    { let _ = embedded::dir(); }
}

#[cfg(feature = "embedded")]
mod embedded {
    use std::path::PathBuf;
    use std::sync::OnceLock;

    static BUNDLE: &[u8] = include_bytes!(env!("LINKSHOT_BUNDLE"));
    const ID: &str = env!("LINKSHOT_BUNDLE_ID");

    /// Where the bundle is unpacked: `<local data>/LinkShot/tools/<id>/`. Unpacks on first call.
    pub fn dir() -> Option<PathBuf> {
        static DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
        DIR.get_or_init(|| {
            let base = directories::ProjectDirs::from("", "", "LinkShot").map(|p| p.data_local_dir().join("tools"))?;
            let target = base.join(ID);
            let marker = target.join(".complete");
            if marker.is_file() { return Some(target); }
            tracing::info!("unpacking bundled tools ({} MB) to {}", BUNDLE.len() / 1_000_000, target.display());
            let tmp = base.join(format!("{ID}.part-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&tmp);
            if let Err(e) = unpack(&tmp) { tracing::error!("unpacking bundled tools failed: {e:#}"); let _ = std::fs::remove_dir_all(&tmp); return None; }
            let _ = std::fs::remove_dir_all(&target);
            if let Err(e) = std::fs::rename(&tmp, &target) { tracing::error!("installing bundled tools failed: {e}"); return None; }
            let _ = std::fs::write(&marker, ID);
            Some(target)
        }).clone()
    }

    fn unpack(dir: &std::path::Path) -> anyhow::Result<()> {
        std::fs::create_dir_all(dir)?;
        let dec = ruzstd::decoding::StreamingDecoder::new(BUNDLE)?;
        let mut ar = tar::Archive::new(dec);
        ar.set_preserve_permissions(true);
        ar.unpack(dir)?;
        Ok(())
    }
}

#[cfg(all(test, feature = "embedded"))]
mod tests {
    /// Unpacks the bundle this test binary was built with and checks the finder sees it.
    #[test]
    fn embedded_bundle_unpacks() {
        let dir = super::embedded::dir().expect("bundle unpacked");
        assert!(dir.join(".complete").is_file());
        let entries: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        assert!(entries.iter().any(|e| e.starts_with("ffmpeg")), "ffmpeg missing from {entries:?}");
        assert!(dir.join("tesseract").is_dir() && dir.join("ocrs").join("text-detection.rten").is_file(), "{entries:?}");
        assert!(super::dirs().contains(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
