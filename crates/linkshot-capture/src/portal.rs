//! Screenshot via the XDG desktop portal (`org.freedesktop.portal.Screenshot`) — the sanctioned way on Wayland.

use anyhow::{Context, Result};
use image::RgbaImage;

/// Takes a full-desktop screenshot through the portal. The compositor may ask the user for permission
/// the first time. Returns the composite image of all monitors (physical pixels).
pub fn screenshot() -> Result<RgbaImage> {
    let uri = async_io::block_on(async {
        let resp = ashpd::desktop::screenshot::Screenshot::request().interactive(false).modal(false).send().await?.response()?;
        Ok::<String, ashpd::Error>(resp.uri().to_string())
    })
    .context("portal screenshot")?;
    let path = uri.strip_prefix("file://").map(|p| percent_decode(p)).with_context(|| format!("unexpected screenshot uri {uri}"))?;
    let img = image::open(&path).with_context(|| format!("open {path}"))?.to_rgba8();
    let _ = std::fs::remove_file(&path);
    Ok(img)
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or("zz"), 16) { out.push(v); i += 3; continue; }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
