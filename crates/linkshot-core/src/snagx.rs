//! Read/write `.snagx` (a zip of `index.json`, `metadata.json`, `thumbnail.png`, `{GUID}.json`, `{GUID}.png`).

use crate::document::{Document, Metadata, Page};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::Path;

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
struct Index {
    pages: Vec<String>,
    #[serde(default = "v1")]
    version: String,
}
fn v1() -> String { "1.0".into() }

pub fn read_bytes(bytes: &[u8]) -> Result<Document> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).context("not a zip archive")?;
    let mut members = std::collections::BTreeMap::new();
    for i in 0..zip.len() {
        let mut f = zip.by_index(i)?;
        if f.is_dir() { continue; }
        let mut buf = Vec::with_capacity(f.size() as usize);
        f.read_to_end(&mut buf)?;
        members.insert(f.name().to_string(), buf);
    }
    let index: Index = match members.remove("index.json") {
        Some(b) => serde_json::from_slice(&b).context("index.json")?,
        None => {
            // Tolerate files without an index: take every top-level json that is not metadata.
            let pages = members.keys().filter(|k| k.ends_with(".json") && *k != "metadata.json" && !k.ends_with(".backup.json")).cloned().collect();
            Index { pages, version: v1() }
        }
    };
    let metadata: Metadata = members
        .remove("metadata.json")
        .map(|b| serde_json::from_slice(&b))
        .transpose()
        .context("metadata.json")?
        .unwrap_or_default();
    let thumbnail_png = members.remove("thumbnail.png");
    let mut pages = Vec::new();
    for name in &index.pages {
        let b = members.remove(name).with_context(|| format!("missing page {name}"))?;
        let page: Page = serde_json::from_slice(&b).with_context(|| format!("page {name}"))?;
        pages.push(page);
    }
    anyhow::ensure!(!pages.is_empty(), "snagx has no pages");
    let blobs = members.into_iter().map(|(k, v)| (k, std::sync::Arc::new(v))).collect();
    Ok(Document { metadata, pages, blobs, thumbnail_png, path: None })
}

/// What a library listing needs from a `.snagx` without decoding its images: metadata, the first
/// page's canvas size and the stored thumbnail. Reads only those zip members.
#[derive(Clone, Debug, Default)]
pub struct Summary {
    pub metadata: Metadata,
    pub width: u32,
    pub height: u32,
    pub thumbnail_png: Option<Vec<u8>>,
}

pub fn read_summary(path: &Path) -> Result<Summary> {
    let file = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut zip = zip::ZipArchive::new(std::io::BufReader::new(file)).context("not a zip archive")?;
    let names: Vec<String> = zip.file_names().map(str::to_string).collect();
    let mut member = |name: &str| -> Option<Vec<u8>> {
        let mut f = zip.by_name(name).ok()?;
        let mut buf = Vec::with_capacity(f.size() as usize);
        f.read_to_end(&mut buf).ok()?;
        Some(buf)
    };
    let metadata: Metadata = member("metadata.json").and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    let thumbnail_png = member("thumbnail.png");
    let first_page = member("index.json").and_then(|b| serde_json::from_slice::<Index>(&b).ok()).and_then(|i| i.pages.first().cloned());
    let (mut width, mut height) = (0, 0);
    let page_name = match first_page {
        Some(n) => Some(n),
        // No index: the first top-level page json.
        None => names.iter().filter(|k| k.ends_with(".json") && k.as_str() != "metadata.json" && !k.ends_with(".backup.json")).min().cloned(),
    };
    if let Some(name) = page_name {
        if let Some(b) = member(&name) {
            if let Ok(page) = serde_json::from_slice::<Page>(&b) { width = page.capture_canvas_width as u32; height = page.capture_canvas_height as u32; }
        }
    }
    Ok(Summary { metadata, width, height, thumbnail_png })
}

pub fn read_file(path: &Path) -> Result<Document> {
    let bytes = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let mut doc = read_bytes(&bytes)?;
    doc.path = Some(path.to_path_buf());
    Ok(doc)
}

/// Serialises a page in canonical form (pretty JSON, 2-space indent, LF).
pub fn page_json(page: &Page) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let fmt = serde_json::ser::PrettyFormatter::with_indent(b"  ");
    let mut ser = serde_json::Serializer::with_formatter(&mut out, fmt);
    page.serialize(&mut ser)?;
    Ok(out)
}

pub fn write_bytes(doc: &Document) -> Result<Vec<u8>> {
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buf);
        let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let deflated = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let mut page_names = Vec::new();
        for (i, page) in doc.pages.iter().enumerate() {
            let id = page
                .capture_background_image
                .strip_suffix(".png")
                .filter(|s| s.starts_with('{'))
                .map(str::to_string)
                .unwrap_or_else(|| format!("{{{}}}", uuid::Uuid::new_v4().to_string().to_uppercase()));
            let name = format!("{id}.json");
            let _ = i;
            zip.start_file(&name, deflated)?;
            zip.write_all(&page_json(page)?)?;
            page_names.push(name);
        }
        zip.start_file("index.json", deflated)?;
        zip.write_all(serde_json::to_string_pretty(&Index { pages: page_names, version: v1() })?.as_bytes())?;
        zip.start_file("metadata.json", deflated)?;
        zip.write_all(serde_json::to_string_pretty(&doc.metadata)?.as_bytes())?;
        if let Some(t) = &doc.thumbnail_png {
            zip.start_file("thumbnail.png", stored)?;
            zip.write_all(t)?;
        }
        for (name, bytes) in &doc.blobs {
            zip.start_file(name, stored)?;
            zip.write_all(bytes.as_slice())?;
        }
        zip.finish()?;
    }
    Ok(buf.into_inner())
}

pub fn write_file(doc: &Document, path: &Path) -> Result<()> {
    let bytes = write_bytes(doc)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("snagx.tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::CaptureObject;

    #[test]
    fn roundtrip_minimal() {
        let mut doc = Document::from_rgba(2, 2, &[255u8; 16]).unwrap();
        let mut arrow = CaptureObject::new("Arrow");
        arrow.set_endpoints(crate::Pt::new(1.0, 2.0), crate::Pt::new(10.0, 20.0));
        arrow.extra.insert("SomethingNew".into(), serde_json::json!({"a": 1}));
        doc.page_mut().capture_objects.push(arrow);
        let bytes = write_bytes(&doc).unwrap();
        let back = read_bytes(&bytes).unwrap();
        assert_eq!(back.pages[0].capture_objects[0].extra["SomethingNew"]["a"], 1);
        assert_eq!(back.pages[0].capture_objects[0].p0(), crate::Pt::new(1.0, 2.0));
        assert_eq!(back.blobs.len(), 1);
    }
}
