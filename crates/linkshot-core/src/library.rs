//! The capture library: a folder of `.snagx`/`.mp4`/`.gif` files plus a lightweight JSON index
//! (tags, dates, sources). We keep our own index.

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct LibraryEntry {
    pub path: PathBuf,
    pub kind: crate::enums::TrayItemType,
    pub created: DateTime<Local>,
    pub modified: DateTime<Local>,
    pub size: u64,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub application: String,
    #[serde(default)]
    pub website: String,
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LibraryIndex {
    pub entries: BTreeMap<PathBuf, LibraryEntry>,
}

impl LibraryIndex {
    pub fn load(path: &Path) -> LibraryIndex {
        std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }
    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(p) = path.parent() { std::fs::create_dir_all(p)?; }
        Ok(std::fs::write(path, serde_json::to_vec(self)?)?)
    }
    /// Scans `dir` (non-recursive) and refreshes entries for files that changed.
    pub fn scan(&mut self, dir: &Path) -> anyhow::Result<()> {
        let mut seen = std::collections::HashSet::new();
        for e in std::fs::read_dir(dir)?.flatten() {
            let p = e.path();
            let ext = p.extension().and_then(|s| s.to_str()).map(|s| s.to_ascii_lowercase()).unwrap_or_default();
            let kind = match ext.as_str() {
                "snagx" => crate::enums::TrayItemType::Image,
                "mp4" => crate::enums::TrayItemType::Video,
                "gif" => crate::enums::TrayItemType::AnimatedGif,
                _ => continue,
            };
            let md = match e.metadata() { Ok(m) => m, Err(_) => continue };
            let modified: DateTime<Local> = md.modified().map(Into::into).unwrap_or_else(|_| Local::now());
            let created: DateTime<Local> = md.created().map(Into::into).unwrap_or(modified);
            seen.insert(p.clone());
            let needs = self.entries.get(&p).map(|x| x.modified != modified || x.size != md.len()).unwrap_or(true);
            if needs {
                let mut entry = LibraryEntry { path: p.clone(), kind, created, modified, size: md.len(), tags: vec![], application: String::new(), website: String::new(), width: 0, height: 0 };
                if kind == crate::enums::TrayItemType::Image {
                    // Only the metadata and page json are read; the images stay compressed in the file.
                    if let Ok(sum) = crate::snagx::read_summary(&p) {
                        entry.tags = sum.metadata.tags.clone();
                        entry.width = sum.width;
                        entry.height = sum.height;
                        if let Some(v) = sum.metadata.extra.get("Application").and_then(|v| v.as_str()) { entry.application = v.into(); }
                        if let Some(v) = sum.metadata.extra.get("Url").and_then(|v| v.as_str()) { entry.website = v.into(); }
                    }
                }
                self.entries.insert(p, entry);
            }
        }
        self.entries.retain(|p, _| seen.contains(p));
        Ok(())
    }
    pub fn all_tags(&self) -> Vec<String> {
        let mut t: Vec<String> = self.entries.values().flat_map(|e| e.tags.iter().cloned()).collect();
        t.sort();
        t.dedup();
        t
    }
}
