//! Headless UI rendering for screenshots/tests: runs egui without a window and rasterises the output
//! meshes with a tiny software renderer. Used by `linkshot --shots DIR`.

use egui::epaint::{ImageData, Primitive, TextureId, Vertex};
use egui::{ClippedPrimitive, Color32, Pos2, Rect};
use anyhow::Context as _;
use image::RgbaImage;
use std::collections::HashMap;

pub struct SoftwareRenderer {
    textures: HashMap<TextureId, RgbaImage>,
}

impl Default for SoftwareRenderer { fn default() -> Self { Self::new() } }

impl SoftwareRenderer {
    pub fn new() -> Self { Self { textures: HashMap::new() } }

    pub fn update_textures(&mut self, delta: &egui::TexturesDelta) {
        for (id, d) in &delta.set {
            let img = match &d.image {
                ImageData::Color(c) => {
                    let mut out = RgbaImage::new(c.width() as u32, c.height() as u32);
                    for (i, p) in c.pixels.iter().enumerate() { let [r, g, b, a] = p.to_array(); out.put_pixel((i % c.width()) as u32, (i / c.width()) as u32, image::Rgba([r, g, b, a])); }
                    out
                }
            };
            match d.pos {
                None => { self.textures.insert(*id, img); }
                Some([x, y]) => {
                    if let Some(t) = self.textures.get_mut(id) { image::imageops::replace(t, &img, x as i64, y as i64); } else { self.textures.insert(*id, img); }
                }
            }
        }
        for id in &delta.free { self.textures.remove(id); }
    }

    fn sample(&self, id: TextureId, u: f32, v: f32) -> [f32; 4] {
        let Some(t) = self.textures.get(&id) else { return [1.0, 1.0, 1.0, 1.0] };
        let (w, h) = (t.width() as f32, t.height() as f32);
        let x = (u * w - 0.5).clamp(0.0, w - 1.0);
        let y = (v * h - 0.5).clamp(0.0, h - 1.0);
        let (x0, y0) = (x.floor() as u32, y.floor() as u32);
        let (x1, y1) = ((x0 + 1).min(t.width() - 1), (y0 + 1).min(t.height() - 1));
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        let px = |x: u32, y: u32| { let p = t.get_pixel(x, y); [p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0, p[3] as f32 / 255.0] };
        let (a, b, c, d) = (px(x0, y0), px(x1, y0), px(x0, y1), px(x1, y1));
        let mut out = [0.0; 4];
        for i in 0..4 { out[i] = (a[i] * (1.0 - fx) + b[i] * fx) * (1.0 - fy) + (c[i] * (1.0 - fx) + d[i] * fx) * fy; }
        out
    }

    /// Rasterises primitives into an image of `w × h` device pixels at `ppp`.
    pub fn render(&self, prims: &[ClippedPrimitive], w: u32, h: u32, ppp: f32, background: Color32) -> RgbaImage {
        // Premultiplied linear-ish float buffer.
        let mut buf = vec![[0.0f32; 4]; (w * h) as usize];
        let bg = background.to_array();
        for p in buf.iter_mut() { *p = [bg[0] as f32 / 255.0 * bg[3] as f32 / 255.0, bg[1] as f32 / 255.0 * bg[3] as f32 / 255.0, bg[2] as f32 / 255.0 * bg[3] as f32 / 255.0, bg[3] as f32 / 255.0]; }
        for cp in prims {
            let Primitive::Mesh(mesh) = &cp.primitive else { continue };
            let clip = Rect::from_min_max(cp.clip_rect.min * ppp, cp.clip_rect.max * ppp);
            for tri in mesh.indices.chunks_exact(3) {
                let v0 = &mesh.vertices[tri[0] as usize];
                let v1 = &mesh.vertices[tri[1] as usize];
                let v2 = &mesh.vertices[tri[2] as usize];
                self.triangle(&mut buf, w, h, ppp, clip, mesh.texture_id, v0, v1, v2);
            }
        }
        let mut out = RgbaImage::new(w, h);
        for (i, p) in buf.iter().enumerate() {
            let a = p[3].clamp(0.0, 1.0);
            let un = |c: f32| if a > 0.0 { (c / a).clamp(0.0, 1.0) } else { 0.0 };
            out.put_pixel((i as u32) % w, (i as u32) / w, image::Rgba([(un(p[0]) * 255.0).round() as u8, (un(p[1]) * 255.0).round() as u8, (un(p[2]) * 255.0).round() as u8, (a * 255.0).round() as u8]));
        }
        out
    }

    #[allow(clippy::too_many_arguments)]
    fn triangle(&self, buf: &mut [[f32; 4]], w: u32, h: u32, ppp: f32, clip: Rect, tex: TextureId, v0: &Vertex, v1: &Vertex, v2: &Vertex) {
        let p0 = v0.pos * ppp; let p1 = v1.pos * ppp; let p2 = v2.pos * ppp;
        let min_x = p0.x.min(p1.x).min(p2.x).max(clip.min.x).floor().max(0.0) as i32;
        let max_x = p0.x.max(p1.x).max(p2.x).min(clip.max.x).ceil().min(w as f32) as i32;
        let min_y = p0.y.min(p1.y).min(p2.y).max(clip.min.y).floor().max(0.0) as i32;
        let max_y = p0.y.max(p1.y).max(p2.y).min(clip.max.y).ceil().min(h as f32) as i32;
        if min_x >= max_x || min_y >= max_y { return; }
        let area = (p1.x - p0.x) * (p2.y - p0.y) - (p2.x - p0.x) * (p1.y - p0.y);
        if area.abs() < 1e-6 { return; }
        let inv = 1.0 / area;
        let c0 = v0.color.to_array(); let c1 = v1.color.to_array(); let c2 = v2.color.to_array();
        for y in min_y..max_y {
            let py = y as f32 + 0.5;
            for x in min_x..max_x {
                let px = x as f32 + 0.5;
                let w0 = ((p1.x - px) * (p2.y - py) - (p2.x - px) * (p1.y - py)) * inv;
                let w1 = ((p2.x - px) * (p0.y - py) - (p0.x - px) * (p2.y - py)) * inv;
                let w2 = 1.0 - w0 - w1;
                if w0 < -1e-4 || w1 < -1e-4 || w2 < -1e-4 { continue; }
                let u = v0.uv.x * w0 + v1.uv.x * w1 + v2.uv.x * w2;
                let v = v0.uv.y * w0 + v1.uv.y * w1 + v2.uv.y * w2;
                let t = self.sample(tex, u, v);
                let mut col = [0.0f32; 4];
                for i in 0..4 { col[i] = (c0[i] as f32 * w0 + c1[i] as f32 * w1 + c2[i] as f32 * w2) / 255.0 * t[i]; }
                let dst = &mut buf[(y as u32 * w + x as u32) as usize];
                let sa = col[3];
                for i in 0..4 { dst[i] = col[i] + dst[i] * (1.0 - sa); }
            }
        }
    }
}

/// Runs one egui frame headlessly and returns the rendered image.
pub fn render_frame(ctx: &egui::Context, renderer: &mut SoftwareRenderer, size: egui::Vec2, ppp: f32, background: Color32, mut run: impl FnMut(&egui::Context)) -> RgbaImage {
    let raw = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)), max_texture_side: Some(16384), ..Default::default() };
    ctx.set_pixels_per_point(ppp);
    let out = ctx.run(raw, |ctx| run(ctx));
    renderer.update_textures(&out.textures_delta);
    let prims = ctx.tessellate(out.shapes, ppp);
    renderer.render(&prims, (size.x * ppp) as u32, (size.y * ppp) as u32, ppp, background)
}

/// `linkshot --shots DIR [files...]`: writes capture_window.png, editor.png and overlay.png.
pub fn run_shots(dir: &std::path::Path, files: &[std::path::PathBuf], dark: bool) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir)?;
    let ctx = egui::Context::default();
    let cc_ctx = ctx.clone();
    // Build the app without eframe.
    let mut app = crate::app::App::new_headless(&cc_ctx, dark);
    let mut renderer = SoftwareRenderer::new();
    for f in files { if let Ok(doc) = crate::share::open_file(f) { let paths = app.paths.clone(); let settings = app.settings.clone(); app.editor.add_document(&ctx, doc, &paths, &settings); } }
    let bg = Color32::TRANSPARENT;
    // warm-up frames so layout/animations settle
    let shot = |app: &mut crate::app::App, renderer: &mut SoftwareRenderer, name: &str| -> anyhow::Result<()> {
        let size = app.capture_window_size();
        for _ in 0..3 { let _ = render_frame(&ctx, renderer, size, 2.0, bg, |ctx| app.capture_window_ui(ctx)); }
        let img = render_frame(&ctx, renderer, size, 2.0, bg, |ctx| app.capture_window_ui(ctx));
        img.save(dir.join(name))?;
        Ok(())
    };
    shot(&mut app, &mut renderer, "capture_window.png")?;
    app.capture.presets_expanded = true;
    shot(&mut app, &mut renderer, "capture_window_presets.png")?;
    app.presets.selected_mode = linkshot_core::enums::CaptureMode::CaptureModeImage;
    shot(&mut app, &mut renderer, "capture_window_image.png")?;
    app.presets.selected_mode = linkshot_core::enums::CaptureMode::CaptureModeVideo;
    shot(&mut app, &mut renderer, "capture_window_video.png")?;
    app.presets.selected_mode = linkshot_core::enums::CaptureMode::CaptureModeAllInOne;
    app.capture.presets_expanded = false;
    // Editor
    let ectx = egui::Context::default();
    crate::theme::apply(&ectx, &app.pal);
    let mut actions = vec![];
    for _ in 0..3 { let _ = render_frame(&ectx, &mut renderer, egui::vec2(1280.0, 820.0), 1.5, bg, |ctx| crate::editor::editor_ui(&mut app, ctx, &mut actions)); }
    let img = render_frame(&ectx, &mut renderer, egui::vec2(1280.0, 820.0), 1.5, bg, |ctx| crate::editor::editor_ui(&mut app, ctx, &mut actions));
    img.save(dir.join("editor.png"))?;
    app.editor.side_panel = crate::editor::SidePanelMode::Effects;
    let img = render_frame(&ectx, &mut renderer, egui::vec2(1280.0, 820.0), 1.5, bg, |ctx| crate::editor::editor_ui(&mut app, ctx, &mut actions));
    img.save(dir.join("editor_effects.png"))?;
    // Right-click menu on the canvas (with a document open).
    {
        use egui::{Event, PointerButton};
        if app.editor.current.is_none() {
            let snap = synthetic_snapshot();
            let img = snap.images[0].clone();
            if let Ok(png) = linkshot_render::encode_png(&img) { let paths = app.paths.clone(); let settings = app.settings.clone(); app.editor.add_document(&ectx, linkshot_core::Document::from_png(img.width(), img.height(), png), &paths, &settings); }
        }
        for _ in 0..2 { let _ = render_frame(&ectx, &mut renderer, egui::vec2(1280.0, 820.0), 1.5, bg, |ctx| crate::editor::editor_ui(&mut app, ctx, &mut actions)); }
        let c = app.editor.canvas.view.center();
        let esize = egui::vec2(1280.0, 820.0);
        let mut feed = |events: Vec<Event>, app: &mut crate::app::App| {
            let raw = egui::RawInput { screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, esize)), max_texture_side: Some(16384), events, ..Default::default() };
            let _ = ectx.run(raw, |ctx| crate::editor::editor_ui(app, ctx, &mut actions));
        };
        feed(vec![Event::PointerMoved(c)], &mut app);
        feed(vec![Event::PointerButton { pos: c, button: PointerButton::Secondary, pressed: true, modifiers: Default::default() }], &mut app);
        feed(vec![Event::PointerButton { pos: c, button: PointerButton::Secondary, pressed: false, modifiers: Default::default() }], &mut app);
        feed(vec![], &mut app);
        let img = render_frame(&ectx, &mut renderer, esize, 1.5, bg, |ctx| crate::editor::editor_ui(&mut app, ctx, &mut actions));
        img.save(dir.join("editor_context_menu.png"))?;
    }
    // Overlay with a synthetic or real snapshot
    let snap = if std::env::var_os("LINKSHOT_SHOTS_REAL").is_some() { linkshot_capture::snapshot().unwrap_or_else(|_| synthetic_snapshot()) } else { synthetic_snapshot() };
    let _ = snap.images[0].save(dir.join("snapshot_monitor0.png"));
    let mut overlay = crate::overlay::Overlay::new(snap, 0, app.presets.all_in_one.clone(), true);
    let octx = egui::Context::default();
    let (mw, mh) = { let m = &overlay.snapshot.monitors[0]; (m.width as f32, m.height as f32) };
    let size = egui::vec2(mw.min(1600.0), mh.min(900.0));
    let pal = app.pal;
    // Put the pointer on the monitor so the crosshair, hint and magnifier are drawn.
    {
        let raw = egui::RawInput { screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, size)), max_texture_side: Some(16384), events: vec![egui::Event::PointerMoved(egui::Pos2::new(size.x * 0.42, size.y * 0.47))], ..Default::default() };
        let out = octx.run(raw, |ctx| overlay.ui(ctx, &pal, 0));
        renderer.update_textures(&out.textures_delta);
    }
    for _ in 0..2 { let _ = render_frame(&octx, &mut renderer, size, 1.0, bg, |ctx| overlay.ui(ctx, &pal, 0)); }
    let img = render_frame(&octx, &mut renderer, size, 1.0, bg, |ctx| overlay.ui(ctx, &pal, 0));
    img.save(dir.join("overlay.png"))?;
    // Scrolling-capture HUD (detached session, simulated progress).
    {
        use linkshot_capture::scrolling::Event;
        let region = linkshot_core::Rect::from_xywh(200.0, 150.0, 900.0, 600.0);
        let (session, etx) = linkshot_capture::scrolling::Session::detached(region);
        let mut hud = crate::scrolling_hud::ScrollingUi::new(session, app.presets.all_in_one.clone(), region, linkshot_core::Rect::from_xywh(0.0, 0.0, 1600.0, 900.0), false, true);
        let _ = etx.send(Event::Progress { frames: 41, accepted: 7, length: 3860, last: None, axis: Some(linkshot_capture::stitch::Axis::Vertical), auto: false, misses: 0 });
        let hctx = egui::Context::default();
        crate::theme::apply(&hctx, &pal);
        let hsize = egui::vec2(crate::scrolling_hud::HUD_W, crate::scrolling_hud::HUD_H);
        for _ in 0..2 { let _ = render_frame(&hctx, &mut renderer, hsize, 2.0, bg, |ctx| hud.ui(ctx, &pal)); }
        let img = render_frame(&hctx, &mut renderer, hsize, 2.0, bg, |ctx| hud.ui(ctx, &pal));
        img.save(dir.join("scrolling_hud.png"))?;
    }
    // Recording toolbar (ready, recording with audio + webcam, compact) and the Screen Draw layer.
    {
        use crate::recording_hud::{Phase, RecordingSetup, RecordingUi, HUD_H, HUD_W, MINI_H, MINI_W};
        let region = linkshot_core::Rect::from_xywh(200.0, 150.0, 900.0, 600.0);
        let monitor = linkshot_core::Rect::from_xywh(0.0, 0.0, 1600.0, 900.0);
        let mut vs = app.presets.video.clone();
        vs.enable_microphone = true;
        vs.enable_system_audio = true;
        vs.enable_webcam = false;
        let setup = RecordingSetup { region, monitor, monitors: vec![monitor], settings: vs, fps: 30, max_size: None, spool_path: dir.join("hud.lsv"), ready_first: true, show_countdown: true, mic_alert: true, mute_hotkey: "Ctrl+Shift+M".into(), minimize_fullscreen: true, draw_hotkey: "Ctrl+Shift+D".into() };
        let mut hud = RecordingUi::new(setup.clone(), std::sync::Arc::new(|| {}));
        hud.mic = true;
        hud.system_audio = true;
        let hctx = egui::Context::default();
        crate::theme::apply(&hctx, &pal);
        let hsize = egui::vec2(HUD_W, HUD_H);
        for _ in 0..2 { let _ = render_frame(&hctx, &mut renderer, hsize, 2.0, bg, |ctx| hud.ui(ctx, &pal)); }
        render_frame(&hctx, &mut renderer, hsize, 2.0, bg, |ctx| hud.ui(ctx, &pal)).save(dir.join("recording_hud_ready.png"))?;
        hud.phase = Phase::Recording;
        hud.frames = 412; hud.elapsed_ms = 13_700;
        hud.audio = linkshot_capture::video::AudioStatus { mic: true, mic_muted: false, mic_level: 0.45, system: true, system_level: 0.2, webcam: true, webcam_shown: true, webcam_fullscreen: false };
        hud.webcam = Some(linkshot_capture::video::WebcamOptions { device: "lavfi:testsrc".into(), placement: linkshot_capture::webcam::Placement::BottomRight, shape: linkshot_capture::webcam::Shape::Circle, shown: true });
        hud.draw = Some(crate::screen_draw::ScreenDraw::new(region, 3, false));
        for _ in 0..2 { let _ = render_frame(&hctx, &mut renderer, hsize, 2.0, bg, |ctx| hud.ui(ctx, &pal)); }
        render_frame(&hctx, &mut renderer, hsize, 2.0, bg, |ctx| hud.ui(ctx, &pal)).save(dir.join("recording_hud_recording.png"))?;
        // Compact bar (whole-monitor recording).
        let mut small = RecordingUi::new(RecordingSetup { region: monitor, ..setup }, std::sync::Arc::new(|| {}));
        small.mic = true;
        small.phase = Phase::Recording;
        small.elapsed_ms = 65_400;
        small.mic_muted = true;
        let msize = egui::vec2(MINI_W, MINI_H);
        for _ in 0..2 { let _ = render_frame(&hctx, &mut renderer, msize, 2.0, bg, |ctx| small.ui(ctx, &pal)); }
        render_frame(&hctx, &mut renderer, msize, 2.0, bg, |ctx| small.ui(ctx, &pal)).save(dir.join("recording_hud_compact.png"))?;
        // Screen Draw: an arrow, a square and two steps, drawn with synthetic pointer input.
        let mut draw = crate::screen_draw::ScreenDraw::new(region, 30, true);
        let dctx = egui::Context::default();
        crate::theme::apply(&dctx, &pal);
        let dsize = egui::vec2(region.width() as f32, region.height() as f32);
        let mut feed = |events: Vec<egui::Event>, draw: &mut crate::screen_draw::ScreenDraw| {
            let raw = egui::RawInput { screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, dsize)), max_texture_side: Some(16384), events, ..Default::default() };
            let out = dctx.run(raw, |ctx| draw.ui(ctx, &pal));
            renderer.update_textures(&out.textures_delta);
        };
        let drag = |feed: &mut dyn FnMut(Vec<egui::Event>, &mut crate::screen_draw::ScreenDraw), draw: &mut crate::screen_draw::ScreenDraw, a: egui::Pos2, b: egui::Pos2| {
            use egui::{Event, PointerButton};
            feed(vec![Event::PointerMoved(a), Event::PointerButton { pos: a, button: PointerButton::Primary, pressed: true, modifiers: Default::default() }], draw);
            let mid = egui::Pos2::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
            feed(vec![Event::PointerMoved(mid)], draw);
            feed(vec![Event::PointerMoved(b)], draw);
            feed(vec![Event::PointerButton { pos: b, button: PointerButton::Primary, pressed: false, modifiers: Default::default() }], draw);
            feed(vec![], draw);
        };
        feed(vec![], &mut draw); // egui hit-tests against the previous frame's widgets: lay the layer out once first
        drag(&mut feed, &mut draw, egui::Pos2::new(120.0, 400.0), egui::Pos2::new(380.0, 220.0));
        feed(vec![egui::Event::Key { key: egui::Key::S, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() }], &mut draw);
        drag(&mut feed, &mut draw, egui::Pos2::new(420.0, 160.0), egui::Pos2::new(700.0, 330.0));
        feed(vec![egui::Event::Key { key: egui::Key::Num1, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() }], &mut draw);
        for p in [egui::Pos2::new(520.0, 450.0), egui::Pos2::new(640.0, 500.0)] {
            use egui::{Event, PointerButton};
            feed(vec![Event::PointerMoved(p), Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: true, modifiers: Default::default() }], &mut draw);
            feed(vec![Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: false, modifiers: Default::default() }], &mut draw);
            feed(vec![], &mut draw);
        }
        let mut canvas = RgbaImage::from_pixel(region.width() as u32, region.height() as u32, image::Rgba([54, 60, 70, 255]));
        let layer = render_frame(&dctx, &mut renderer, dsize, 1.0, Color32::TRANSPARENT, |ctx| draw.ui(ctx, &pal));
        image::imageops::overlay(&mut canvas, &layer, 0, 0);
        canvas.save(dir.join("screen_draw.png"))?;
    }
    // Grab Text Results window.
    {
        let mut gt = crate::grab_text::GrabTextUi::with_text("Capture settings\n\nSelection: Region\nEffects: None\nShare: Editor\n\nPress the Capture button or PrintScreen to start a capture.\nDrag to select a region, click a window, or press Enter for fullscreen.");
        let gctx = egui::Context::default();
        crate::theme::apply(&gctx, &pal);
        let gsize = egui::vec2(600.0, 420.0);
        for _ in 0..2 { let _ = render_frame(&gctx, &mut renderer, gsize, 2.0, bg, |ctx| gt.ui(ctx, &pal)); }
        let img = render_frame(&gctx, &mut renderer, gsize, 2.0, bg, |ctx| gt.ui(ctx, &pal));
        img.save(dir.join("grab_text_results.png"))?;
    }
    // Built-in stamps as a contact sheet.
    {
        let set = crate::editor::stamps::builtin();
        let cell = 140u32;
        let cols = 7u32;
        let rows = (set.len() as u32 + cols - 1) / cols;
        let mut sheet = RgbaImage::from_pixel(cols * cell, rows * cell, image::Rgba([240, 240, 240, 255]));
        for (i, st) in set.iter().enumerate() {
            let img = image::load_from_memory(&st.png)?.to_rgba8();
            image::imageops::overlay(&mut sheet, &img, ((i as u32 % cols) * cell + 6) as i64, ((i as u32 / cols) * cell + 6) as i64);
        }
        sheet.save(dir.join("stamps.png"))?;
    }
    println!("wrote screenshots to {}", dir.display());
    Ok(())
}

fn synthetic_snapshot() -> linkshot_capture::Snapshot {
    let (w, h) = (1600u32, 900u32);
    let mut img = RgbaImage::new(w, h);
    for y in 0..h { for x in 0..w { img.put_pixel(x, y, image::Rgba([(x * 255 / w) as u8, (y * 255 / h) as u8, 120, 255])); } }
    linkshot_capture::Snapshot {
        monitors: vec![linkshot_capture::MonitorInfo { id: 0, name: "synthetic".into(), x: 0, y: 0, width: w, height: h, scale_factor: 1.0, is_primary: true }],
        images: vec![img], windows: vec![], taken_at: std::time::Instant::now(),
    }
}

/// Drives the capture → editor flow with synthetic input (no window): used by `linkshot --selftest`.
pub fn run_selftest() -> anyhow::Result<()> {
    use egui::{Event, PointerButton, Pos2};
    let ctx = egui::Context::default();
    let mut app = crate::app::App::new_headless(&ctx, false);
    let mut renderer = SoftwareRenderer::new();
    let pal = app.pal;
    // 1. Overlay: drag a region, choose Image.
    let snap = synthetic_snapshot();
    let mut overlay = crate::overlay::Overlay::new(snap, 0, app.presets.all_in_one.clone(), true);
    let octx = egui::Context::default();
    let size = egui::vec2(1600.0, 900.0);
    let mut feed = |events: Vec<Event>, overlay: &mut crate::overlay::Overlay| {
        let raw = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)), max_texture_side: Some(16384), events, ..Default::default() };
        let out = octx.run(raw, |ctx| overlay.ui(ctx, &pal, 0));
        renderer.update_textures(&out.textures_delta);
    };
    feed(vec![], &mut overlay);
    feed(vec![Event::PointerMoved(Pos2::new(100.0, 100.0))], &mut overlay);
    feed(vec![Event::PointerButton { pos: Pos2::new(100.0, 100.0), button: PointerButton::Primary, pressed: true, modifiers: Default::default() }], &mut overlay);
    for i in 1..=10 { feed(vec![Event::PointerMoved(Pos2::new(100.0 + 40.0 * i as f32, 100.0 + 30.0 * i as f32))], &mut overlay); }
    feed(vec![Event::PointerButton { pos: Pos2::new(500.0, 400.0), button: PointerButton::Primary, pressed: false, modifiers: Default::default() }], &mut overlay);
    feed(vec![], &mut overlay);
    anyhow::ensure!(overlay.result.is_none(), "All-in-One should wait for the HUD choice");
    feed(vec![Event::Key { key: egui::Key::I, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() }], &mut overlay);
    let result = overlay.result.take().context("overlay produced no result")?;
    let region = match result { crate::overlay::OverlayResult::Image { region, .. } => region, _ => anyhow::bail!("expected image result") };
    println!("selftest: overlay region = {:?} ({}x{})", region, region.width(), region.height());
    anyhow::ensure!((region.width() - 400.0).abs() <= 2.0 && (region.height() - 300.0).abs() <= 2.0, "unexpected region size");
    let img = overlay.snapshot.crop(region).context("crop")?;
    // Native-resolution cropping: a 2× monitor must yield 2× the logical pixels, copied exactly.
    {
        let mut hi = synthetic_snapshot();
        let big = image::imageops::resize(&hi.images[0], 3200, 1800, image::imageops::FilterType::Nearest);
        hi.images[0] = big.clone();
        let c = hi.crop(linkshot_core::Rect::from_xywh(10.0, 20.0, 100.0, 50.0)).context("hidpi crop")?;
        anyhow::ensure!(c.dimensions() == (200, 100), "HiDPI crop should be 200x100, got {:?}", c.dimensions());
        anyhow::ensure!(c.pixels().zip(image::imageops::crop_imm(&big, 20, 40, 200, 100).to_image().pixels()).all(|(a, b)| a == b), "HiDPI crop is not pixel-exact");
        println!("selftest: HiDPI crop 100x50 logical -> {}x{} px, pixel-exact", c.width(), c.height());
    }
    // Scrolling capture: stitch synthetic frames of a tall page scrolled in uneven steps.
    {
        use linkshot_capture::stitch::{Push, Stitcher};
        let (w, ph, vh) = (640u32, 2400u32, 480u32);
        let mut page = image::RgbaImage::from_pixel(w, ph, image::Rgba([255, 255, 255, 255]));
        let mut seed = 7u64;
        for _ in 0..300 { seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17; let x = (seed % 560) as u32; let y = (seed / 7 % (ph as u64 - 20)) as u32; let c = image::Rgba([(seed % 200) as u8, (seed / 3 % 200) as u8, (seed / 5 % 200) as u8, 255]); for yy in y..y + 12 { for xx in x..x + 60 { if (xx + yy) % 5 != 0 { page.put_pixel(xx, yy, c); } } } }
        let mut st = Stitcher::new();
        let mut pos = 0u32;
        for step in [0u32, 150, 310, 90, 420, 200, 260, 330, 400, 500] {
            pos = (pos + step).min(ph - vh);
            let r = st.push(image::imageops::crop_imm(&page, 0, pos, w, vh).to_image());
            anyhow::ensure!(step == 0 || matches!(r, Push::Scrolled { .. } | Push::NoChange), "stitch step {step}: {r:?}");
        }
        let out = st.finish().context("stitch")?;
        anyhow::ensure!(out.height() == pos + vh, "stitched height {} != {}", out.height(), pos + vh);
        anyhow::ensure!(out.pixels().zip(image::imageops::crop_imm(&page, 0, 0, w, pos + vh).to_image().pixels()).all(|(a, b)| a == b), "stitched image differs from the page");
        println!("selftest: stitched {} frames into {}x{} px, pixel-exact", st.accepted(), out.width(), out.height());
    }
    let settings = app.presets.all_in_one.clone();
    let tmp = std::env::temp_dir().join(format!("linkshot-selftest-{}", std::process::id()));
    app.settings.library_directory = Some(tmp.clone());
    app.finish_capture(&ctx, img, None, &settings);
    anyhow::ensure!(app.editor_visible, "editor should open after capture");
    let doc = app.editor.current_doc().context("no document")?;
    anyhow::ensure!(doc.doc.page().capture_canvas_width == 400.0 && doc.doc.page().capture_canvas_height == 300.0, "document size mismatch");
    anyhow::ensure!(doc.doc.path.as_ref().map(|p| p.exists()).unwrap_or(false), "capture was not auto-saved to the library");
    println!("selftest: capture auto-saved to {}", doc.doc.path.as_ref().unwrap().display());
    // 2. Editor: add an arrow via a drag with the Arrow tool, then undo/redo.
    let ectx = egui::Context::default();
    crate::theme::apply(&ectx, &pal);
    let esize = egui::vec2(1280.0, 820.0);
    let mut actions = vec![];
    let mut efeed = |events: Vec<Event>, app: &mut crate::app::App| {
        let raw = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, esize)), max_texture_side: Some(16384), events, ..Default::default() };
        let _ = ectx.run(raw, |ctx| crate::editor::editor_ui(app, ctx, &mut actions));
    };
    for _ in 0..3 { efeed(vec![], &mut app); }
    app.editor.select_tool(linkshot_core::enums::ToolMode::Arrow);
    let c = app.editor.canvas.view.center();
    let a = c - egui::vec2(100.0, 60.0);
    let b = c + egui::vec2(100.0, 60.0);
    efeed(vec![Event::PointerMoved(a)], &mut app);
    efeed(vec![Event::PointerButton { pos: a, button: PointerButton::Primary, pressed: true, modifiers: Default::default() }], &mut app);
    for i in 1..=8 { efeed(vec![Event::PointerMoved(a + (b - a) * (i as f32 / 8.0))], &mut app); }
    efeed(vec![Event::PointerButton { pos: b, button: PointerButton::Primary, pressed: false, modifiers: Default::default() }], &mut app);
    efeed(vec![], &mut app);
    let n = app.editor.current_doc().unwrap().doc.page().capture_objects.len();
    anyhow::ensure!(n == 1, "expected 1 object after arrow drag, got {n}");
    let obj = &app.editor.current_doc().unwrap().doc.page().capture_objects[0];
    println!("selftest: arrow {:?} -> {:?} width {}", obj.p0(), obj.p1(), obj.stroke_width());
    anyhow::ensure!(obj.tool_mode == "Arrow" && (obj.p1() - obj.p0()).x > 100.0, "arrow geometry wrong");
    app.editor.current_doc_mut().unwrap().undo();
    anyhow::ensure!(app.editor.current_doc().unwrap().doc.page().capture_objects.is_empty(), "undo failed");
    app.editor.current_doc_mut().unwrap().redo();
    anyhow::ensure!(app.editor.current_doc().unwrap().doc.page().capture_objects.len() == 1, "redo failed");
    // 3. Save as PNG and SNAGX and read back.
    let png = tmp.join("out.png");
    let snagx = tmp.join("out.snagx");
    let doc = app.editor.current_doc().unwrap().doc.clone();
    crate::share::export(&mut app.editor.renderer, &doc, &png, 90)?;
    crate::share::export(&mut app.editor.renderer, &doc, &snagx, 90)?;
    let back = linkshot_core::snagx::read_file(&snagx)?;
    anyhow::ensure!(back.page().capture_objects.len() == 1, "snagx round-trip lost the arrow");
    let pimg = image::open(&png)?;
    anyhow::ensure!(pimg.width() == 400 && pimg.height() == 300, "png size wrong");
    // 4. Flatten + effects.
    crate::editor::flatten_current(&mut app);
    anyhow::ensure!(app.editor.current_doc().unwrap().doc.page().capture_objects.is_empty(), "flatten failed");
    crate::editor::tools::whole_image_op(&mut app, "Border", |img| linkshot_render::effects::border(img, linkshot_core::Argb::BLACK, 5));
    let p = app.editor.current_doc().unwrap().doc.page();
    anyhow::ensure!(p.capture_canvas_width == 410.0 && p.capture_canvas_height == 310.0, "border effect size wrong");
    // 5. Video: a synthetic recording spool → tray document → automatic GIF save → reopen as video.
    {
        use linkshot_video::FrameSource as _;
        let (w, h) = (160u32, 120u32);
        let spool = tmp.join("rec.lsv");
        let mut wr = linkshot_video::SpoolWriter::create(&spool, w, h, 30)?;
        for k in 0..45u32 {
            let mut f = vec![0u8; (w * h * 4) as usize];
            for (i, px) in f.chunks_exact_mut(4).enumerate() {
                let (x, y) = (i as u32 % w, i as u32 / w);
                let inside = x >= k * 2 && x < k * 2 + 30 && y >= 30 && y < 70;
                px.copy_from_slice(if inside { &[230, 90, 40, 255] } else if (x / 20 + y / 20) % 2 == 0 { &[40, 44, 52, 255] } else { &[48, 52, 60, 255] });
            }
            wr.push(k * 33, &f)?;
        }
        let info = wr.finish()?;
        anyhow::ensure!(info.frames == 45, "spool frames");
        app.settings.auto_save_videos = true;
        app.settings.default_video_format = "GIF".into();
        app.settings.gif_export = Default::default(); // the test counts frames at the default 10 fps
        app.paths.data_dir = tmp.join("data");
        app.paths.cache_dir = tmp.join("cache");
        std::fs::create_dir_all(app.paths.recordings_dir())?;
        let paths = app.paths.clone();
        let s = app.settings.clone();
        app.editor.add_recording(spool.clone(), &paths, &s)?;
        anyhow::ensure!(app.editor.current_doc().map(|d| d.is_video()).unwrap_or(false), "recording should open as a video document");
        anyhow::ensure!(app.editor.export_job.is_some(), "auto-save should start a GIF export");
        let t0 = std::time::Instant::now();
        while app.editor.export_job.is_some() && t0.elapsed().as_secs() < 30 { crate::editor::video_view::poll_export(&mut app, &ectx); std::thread::sleep(std::time::Duration::from_millis(20)); }
        let gif = app.editor.current_doc().and_then(|d| d.doc.path.clone()).context("video was not saved")?;
        anyhow::ensure!(gif.extension().map(|e| e == "gif").unwrap_or(false) && gif.exists(), "expected a GIF in the library, got {}", gif.display());
        let master = crate::editor::video_view::master_for(&gif, &app.paths).context("master spool should sit next to the GIF name")?;
        {
            use image::AnimationDecoder;
            let dec = image::codecs::gif::GifDecoder::new(std::io::BufReader::new(std::fs::File::open(&gif)?))?;
            let frames = dec.into_frames().collect_frames()?;
            anyhow::ensure!(frames.len() >= 12 && frames.len() <= 16, "10 fps GIF of a 1.5 s clip should have ~15 frames, got {}", frames.len());
            anyhow::ensure!(frames[0].buffer().get_pixel(5, 50).0 == [230, 90, 40, 255], "GIF colours must be exact for a 3-colour recording");
            println!("selftest: recording saved as {} ({} frames, {} KB)", gif.display(), frames.len(), std::fs::metadata(&gif)?.len() / 1024);
        }
        // Reopen the GIF from disk as a document: becomes a video doc backed by the master.
        let doc = crate::share::open_file(&gif)?;
        anyhow::ensure!(doc.metadata.extra.contains_key("VideoFile"), "GIF should open as a video poster");
        app.editor.add_document(&ectx, doc, &paths, &s);
        let d = app.editor.current_doc().unwrap();
        anyhow::ensure!(d.is_video() && d.video.as_ref().unwrap().master.as_deref() == Some(master.as_path()), "reopened GIF should use the lossless master");
        // MP4 when ffmpeg is available.
        match linkshot_video::ffmpeg::find_ffmpeg() {
            Some(ff) => {
                let mp4 = tmp.join("rec.mp4");
                let mut src = linkshot_video::SpoolReader::open(&master)?;
                let rep = linkshot_video::export_mp4(&mut src, &linkshot_video::Mp4Options::default(), &mp4, &mut |_| true)?;
                anyhow::ensure!(mp4.exists() && rep.bytes > 0, "mp4 export");
                let probe = linkshot_video::ffmpeg::probe(&mp4)?;
                anyhow::ensure!(probe.width == 160 && probe.height == 120, "mp4 size {probe:?}");
                // and back: decoding the MP4 yields a spool of the right size
                let cache = app.paths.video_cache_dir();
                let mut back = linkshot_video::import::open_video(&mp4, None, &cache, &mut |_| true)?;
                anyhow::ensure!(back.width() == 160 && back.len() >= 40, "mp4 decode: {}x{} {} frames", back.width(), back.height(), back.len());
                let px = &back.frame(0)?[(50 * 160 + 5) * 4..][..3];
                anyhow::ensure!(px[0] > 180 && px[1] < 140 && px[2] < 100, "mp4 frame 0 colour {px:?}");
                println!("selftest: MP4 via {} ({}, {} KB), decoded back to {} frames", ff.display(), rep.encoder, rep.bytes / 1024, back.len());
                // Audio: two synthetic tracks (as the recorder writes them) mixed, muxed into an MP4 and extracted back.
                let mk = |name: &str, freq: f32| -> anyhow::Result<std::path::PathBuf> {
                    let p = tmp.join(name);
                    let mut w = linkshot_audio::WavWriter::create(&p)?;
                    let n = 48_000 * 3 / 2;
                    let mut buf = Vec::with_capacity(n * 4);
                    for i in 0..n { let v = ((i as f32 / 48_000.0 * freq * 2.0 * std::f32::consts::PI).sin() * 12_000.0) as i16; buf.extend_from_slice(&v.to_le_bytes()); buf.extend_from_slice(&v.to_le_bytes()); }
                    w.write(&buf)?;
                    w.finish()?;
                    Ok(p)
                };
                let mixed = master.with_extension("wav");
                linkshot_audio::mix(&[mk("mic.wav", 440.0)?, mk("sys.wav", 660.0)?], &mixed)?;
                let mix_ms = linkshot_audio::wav_duration_ms(&mixed).unwrap_or(0);
                anyhow::ensure!((1400..=1600).contains(&mix_ms), "mixed track should be ~1500 ms, got {mix_ms}");
                let mp4a = tmp.join("rec-audio.mp4");
                let mut src = linkshot_video::SpoolReader::open(&master)?;
                let opts = linkshot_video::Mp4Options { audio: Some(mixed.clone()), start_ms: Some(300), ..Default::default() };
                linkshot_video::export_mp4(&mut src, &opts, &mp4a, &mut |_| true)?;
                let probe = linkshot_video::ffmpeg::probe(&mp4a)?;
                anyhow::ensure!(probe.audio, "the MP4 should carry an audio stream");
                let back_wav = tmp.join("back.wav");
                anyhow::ensure!(linkshot_audio::extract(&mp4a, &back_wav)?, "audio should extract from the MP4");
                let back_ms = linkshot_audio::wav_duration_ms(&back_wav).unwrap_or(0);
                anyhow::ensure!((1000..=1350).contains(&back_ms), "trimmed audio should be ~1200 ms, got {back_ms}");
                anyhow::ensure!(crate::editor::video_view::audio_for(&master).is_some(), "audio_for should find the track next to the master");
                println!("selftest: audio mixed ({mix_ms} ms), muxed into MP4 from 0.3 s and extracted back ({back_ms} ms)");
            }
            None => println!("selftest: ffmpeg not found, MP4 export skipped"),
        }
        app.editor.docs.clear();
        app.editor.current = None;
    }
    let _ = std::fs::remove_dir_all(&tmp);
    println!("selftest: OK");
    Ok(())
}
