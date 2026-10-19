//! The pointer image for still captures on back-ends whose screenshot API cannot include it
//! (X11 through XFixes, Windows through GDI). KWin draws the cursor itself (`include-cursor`).

use image::RgbaImage;

/// The pointer bitmap and where its top-left corner sits in virtual-desktop pixels.
pub struct CursorImage { pub x: i32, pub y: i32, pub image: RgbaImage }

/// The current pointer, or `None` when hidden or unavailable.
pub fn grab() -> Option<CursorImage> {
    #[cfg(target_os = "linux")]
    { if crate::is_wayland() { return None; } x11::grab() }
    #[cfg(windows)]
    { win::grab() }
    #[cfg(not(any(target_os = "linux", windows)))]
    { None }
}

/// Draws the pointer into whichever monitor image of `snap` contains it.
pub fn composite(snap: &mut crate::Snapshot) {
    let Some(c) = grab() else { return };
    let (cw, ch) = (c.image.width() as i32, c.image.height() as i32);
    let (hx, hy) = (c.x + cw / 2, c.y + ch / 2);
    let Some(i) = snap.monitors.iter().position(|m| hx >= m.x && hx < m.x + m.width as i32 && hy >= m.y && hy < m.y + m.height as i32)
        .or_else(|| snap.monitors.iter().position(|m| c.x + cw > m.x && c.x < m.x + m.width as i32 && c.y + ch > m.y && c.y < m.y + m.height as i32)) else { return };
    let m = snap.monitors[i].clone();
    let img = &mut snap.images[i];
    // Monitor images may be denser than their logical size (HiDPI): scale the pointer along.
    let k = img.width() as f64 / m.width.max(1) as f64;
    let src = if (k - 1.0).abs() > 0.01 { image::imageops::resize(&c.image, ((cw as f64 * k).round() as u32).max(1), ((ch as f64 * k).round() as u32).max(1), image::imageops::FilterType::Triangle) } else { c.image };
    image::imageops::overlay(img, &src, ((c.x - m.x) as f64 * k).round() as i64, ((c.y - m.y) as f64 * k).round() as i64);
}

#[cfg(target_os = "linux")]
mod x11 {
    use super::CursorImage;
    use x11rb::protocol::xfixes::ConnectionExt as _;

    pub fn grab() -> Option<CursorImage> {
        let (conn, _) = x11rb::connect(None).ok()?;
        conn.xfixes_query_version(5, 0).ok()?.reply().ok()?;
        let r = conn.xfixes_get_cursor_image().ok()?.reply().ok()?;
        if r.width == 0 || r.height == 0 { return None; }
        let mut img = image::RgbaImage::new(r.width as u32, r.height as u32);
        for (px, v) in img.pixels_mut().zip(r.cursor_image.iter()) {
            let a = (v >> 24) & 255;
            let (mut rr, mut g, mut b) = ((v >> 16) & 255, (v >> 8) & 255, v & 255);
            if a > 0 && a < 255 { rr = (rr * 255 / a).min(255); g = (g * 255 / a).min(255); b = (b * 255 / a).min(255); }
            *px = image::Rgba([rr as u8, g as u8, b as u8, a as u8]);
        }
        Some(CursorImage { x: r.x as i32 - r.xhot as i32, y: r.y as i32 - r.yhot as i32, image: img })
    }
}

#[cfg(windows)]
mod win {
    use super::CursorImage;
    use windows::Win32::Graphics::Gdi::{CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, GetObjectW, ReleaseDC, SelectObject, BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HGDIOBJ};
    use windows::Win32::UI::WindowsAndMessaging::{DrawIconEx, GetCursorInfo, GetIconInfo, CURSORINFO, CURSOR_SHOWING, DI_NORMAL, HICON, ICONINFO};

    pub fn grab() -> Option<CursorImage> {
        unsafe {
            let mut ci = CURSORINFO { cbSize: std::mem::size_of::<CURSORINFO>() as u32, ..Default::default() };
            if GetCursorInfo(&mut ci).is_err() || ci.flags.0 & CURSOR_SHOWING.0 == 0 || ci.hCursor.is_invalid() { return None; }
            let icon = HICON(ci.hCursor.0);
            let mut ii = ICONINFO::default();
            if GetIconInfo(icon, &mut ii).is_err() { return None; }
            let mut bm = BITMAP::default();
            let probe: HGDIOBJ = if !ii.hbmColor.is_invalid() { ii.hbmColor.into() } else { ii.hbmMask.into() };
            if GetObjectW(probe, std::mem::size_of::<BITMAP>() as i32, Some(&mut bm as *mut _ as *mut _)) == 0 { cleanup(&ii); return None; }
            let w = bm.bmWidth.max(1);
            let h = if ii.hbmColor.is_invalid() { (bm.bmHeight / 2).max(1) } else { bm.bmHeight.max(1) };
            let screen = GetDC(None);
            let dc = CreateCompatibleDC(Some(screen));
            let info = BITMAPINFO { bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: w, biHeight: -h, biPlanes: 1, biBitCount: 32, biCompression: BI_RGB.0, ..Default::default() }, ..Default::default() };
            let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
            let Ok(dib) = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0) else { let _ = DeleteDC(dc); ReleaseDC(None, screen); cleanup(&ii); return None; };
            let old = SelectObject(dc, dib.into());
            let ok = DrawIconEx(dc, 0, 0, icon, w, h, 0, None, DI_NORMAL).is_ok();
            let n = (w * h * 4) as usize;
            let raw: Vec<u8> = if ok && !bits.is_null() { std::slice::from_raw_parts(bits as *const u8, n).to_vec() } else { vec![] };
            SelectObject(dc, old);
            let _ = DeleteObject(dib.into());
            let _ = DeleteDC(dc);
            ReleaseDC(None, screen);
            cleanup(&ii);
            if raw.is_empty() { return None; }
            let mut img = image::RgbaImage::new(w as u32, h as u32);
            let any_alpha = raw.chunks_exact(4).any(|p| p[3] != 0);
            for (px, p) in img.pixels_mut().zip(raw.chunks_exact(4)) {
                // BGRA; cursors without an alpha channel (classic arrows) come back with alpha 0 everywhere.
                let a = if any_alpha { p[3] } else if p[0] | p[1] | p[2] != 0 { 255 } else { 0 };
                *px = image::Rgba([p[2], p[1], p[0], a]);
            }
            Some(CursorImage { x: ci.ptScreenPos.x - ii.xHotspot as i32, y: ci.ptScreenPos.y - ii.yHotspot as i32, image: img })
        }
    }

    unsafe fn cleanup(ii: &ICONINFO) {
        unsafe {
            if !ii.hbmColor.is_invalid() { let _ = DeleteObject(ii.hbmColor.into()); }
            if !ii.hbmMask.is_invalid() { let _ = DeleteObject(ii.hbmMask.into()); }
        }
    }
}
