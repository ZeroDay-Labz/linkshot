//! Startup clean-up of LinkShot's own files, so they do not grow without bound: raw recordings
//! nothing refers to any more, the decoded-video and thumbnail caches, and temporary share exports.

use linkshot_core::settings::Paths;
use std::path::Path;
use std::time::{Duration, SystemTime};

/// Size limits for the caches; the least recently used files go first.
const VIDEO_CACHE_MAX: u64 = 2 << 30;
const THUMBNAIL_CACHE_MAX: u64 = 500 << 20;

/// Runs on a background thread at startup.
pub fn run(paths: Paths) {
    let day = Duration::from_secs(24 * 3600);
    // Raw recordings still named `recording-<time>` were never saved under a library name: nothing
    // can open them after a restart (masters are found by the saved file's name).
    let removed = remove_matching(&paths.recordings_dir(), |name| name.starts_with("recording-") && (name.ends_with(".lsv") || name.ends_with(".wav")), day);
    let video = cap_dir(&paths.video_cache_dir(), VIDEO_CACHE_MAX);
    let thumbs = cap_dir(&paths.thumbnails_dir(), THUMBNAIL_CACHE_MAX);
    // Program/Email/Print exports are only needed while the other program has them open.
    let temps = ["share", "print"].iter().map(|d| remove_matching(&paths.cache_dir.join(d), |_| true, day)).sum::<u64>();
    let total = removed + video + thumbs + temps;
    if total > 0 { tracing::info!("housekeeping: freed {} MB", total >> 20); }
}

fn age(meta: &std::fs::Metadata) -> Duration {
    let t = meta.modified().or_else(|_| meta.accessed()).unwrap_or(SystemTime::now());
    SystemTime::now().duration_since(t).unwrap_or_default()
}

/// Deletes files in `dir` (not recursive) whose name matches and that are older than `min_age`.
/// Returns the bytes freed.
fn remove_matching(dir: &Path, matches: impl Fn(&str) -> bool, min_age: Duration) -> u64 {
    let Ok(rd) = std::fs::read_dir(dir) else { return 0 };
    let mut freed = 0;
    for e in rd.flatten() {
        let Ok(meta) = e.metadata() else { continue };
        if !meta.is_file() || !matches(&e.file_name().to_string_lossy()) || age(&meta) < min_age { continue; }
        if std::fs::remove_file(e.path()).is_ok() { freed += meta.len(); }
    }
    freed
}

/// Deletes the least recently used files in `dir` until it holds at most `max` bytes.
pub fn cap_dir(dir: &Path, max: u64) -> u64 {
    let Ok(rd) = std::fs::read_dir(dir) else { return 0 };
    let mut files: Vec<(SystemTime, u64, std::path::PathBuf)> = rd.flatten().filter_map(|e| {
        let m = e.metadata().ok()?;
        m.is_file().then(|| (m.accessed().or_else(|_| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH).max(m.modified().unwrap_or(SystemTime::UNIX_EPOCH)), m.len(), e.path()))
    }).collect();
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    if total <= max { return 0; }
    files.sort_by_key(|f| f.0);
    let mut freed = 0;
    for (_, len, path) in files {
        if total <= max { break; }
        if std::fs::remove_file(&path).is_ok() { total -= len; freed += len; }
    }
    freed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cap_dir_removes_oldest_first() {
        let dir = std::env::temp_dir().join(format!("linkshot-cap-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (i, name) in ["a", "b", "c"].iter().enumerate() {
            let p = dir.join(name);
            std::fs::write(&p, vec![0u8; 1000]).unwrap();
            let t = SystemTime::now() - Duration::from_secs(1000 - i as u64 * 100);
            let f = std::fs::File::options().write(true).open(&p).unwrap();
            f.set_times(std::fs::FileTimes::new().set_accessed(t).set_modified(t)).unwrap();
        }
        assert_eq!(cap_dir(&dir, 2500), 1000);
        assert!(!dir.join("a").exists() && dir.join("b").exists() && dir.join("c").exists());
        assert_eq!(cap_dir(&dir, 2500), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
