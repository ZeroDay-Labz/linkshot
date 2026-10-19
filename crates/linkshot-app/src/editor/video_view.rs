//! Video documents in the Editor: the player (frame view, transport bar, scrubber with in/out
//! points), the "Create Animated GIF" dialog and background GIF/MP4 export jobs.

use crate::app::App;
use crate::icons;
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2};
use linkshot_core::settings::{GifExportSettings, GifPreset, Paths, Settings};
use linkshot_video::{FrameSource, GifOptions, Mp4Options, Player, PlayerCmd, PlayerEvent, PlayerFrame, SpoolReader};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

pub const TRANSPORT_H: f32 = 58.0;

/// A video open in the Editor. The frames live in a spool (the lossless recording master or the
/// decoded cache of an opened GIF/MP4 file).
pub struct VideoDoc {
    /// The saved GIF/MP4 (None until the first save).
    pub file: Option<PathBuf>,
    /// The spool the player and the exporters read.
    pub master: Option<PathBuf>,
    loading: Option<crossbeam_channel::Receiver<anyhow::Result<PathBuf>>>,
    player: Option<Player>,
    wake: Arc<dyn Fn() + Send + Sync>,
    pub frame: Option<PlayerFrame>,
    texture: Option<egui::TextureHandle>,
    tex_index: Option<usize>,
    pub playing: bool,
    pub position_ms: u32,
    pub duration_ms: u32,
    pub width: u32,
    pub height: u32,
    pub frames: usize,
    pub in_ms: Option<u32>,
    pub out_ms: Option<u32>,
    pub looping: bool,
    pub error: Option<String>,
    scrub_drag: bool,
    /// Decoding deferred until the document is shown (the tray can hold many videos).
    pending_open: Option<(PathBuf, Option<PathBuf>, PathBuf)>,
    /// The mixed sound track (`<master stem>.wav`), when the recording or file has one.
    pub audio: Option<PathBuf>,
    pub audio_muted: bool,
    playback: Option<linkshot_audio::Playback>,
}

/// The sound track recorded with (or extracted for) a spool: `<spool stem>.wav`.
pub fn audio_for(master: &Path) -> Option<PathBuf> {
    let p = master.with_extension("wav");
    p.is_file().then_some(p)
}

/// Audio starts a little after it is asked for; begin that much later so it lines up with the frames.
const PLAYBACK_LATENCY_MS: u32 = 70;

impl VideoDoc {
    /// A video whose spool already exists (a finished recording, or a file decoded earlier).
    pub fn from_spool(master: PathBuf, file: Option<PathBuf>, wake: Arc<dyn Fn() + Send + Sync>) -> anyhow::Result<Self> {
        let reader = SpoolReader::open(&master)?;
        let (width, height, frames, duration_ms) = (reader.width(), reader.height(), reader.len(), reader.duration_ms());
        let player = Player::spawn(Box::new(reader), wake.clone());
        let audio = audio_for(&master);
        Ok(Self { file, master: Some(master), loading: None, player: Some(player), wake, frame: None, texture: None, tex_index: None, playing: false, position_ms: 0, duration_ms, width, height, frames, in_ms: None, out_ms: None, looping: false, error: None, scrub_drag: false, pending_open: None, audio, audio_muted: false, playback: None })
    }

    /// A GIF/MP4 file whose decoding waits until the document is first shown.
    pub fn deferred(file: PathBuf, master: Option<PathBuf>, cache_dir: PathBuf, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        // The lossless master (when it exists) is known up front so exports work before the first play.
        let known_master = master.clone().filter(|m| m.is_file());
        let audio = known_master.as_deref().and_then(audio_for);
        Self { file: Some(file.clone()), master: known_master, loading: None, player: None, wake, frame: None, texture: None, tex_index: None, playing: false, position_ms: 0, duration_ms: 0, width: 0, height: 0, frames: 0, in_ms: None, out_ms: None, looping: false, error: None, scrub_drag: false, pending_open: Some((file, master, cache_dir)), audio, audio_muted: false, playback: None }
    }

    /// Starts the deferred decode (called when the document becomes current).
    pub fn ensure_open(&mut self) {
        if let Some((file, master, cache_dir)) = self.pending_open.take() {
            let wake = self.wake.clone();
            *self = Self::open_file(file, master, cache_dir, wake);
        }
    }

    /// A GIF/MP4 file: decoded into the cache in the background (the poster is shown meanwhile).
    pub fn open_file(file: PathBuf, master: Option<PathBuf>, cache_dir: PathBuf, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        if let Some(m) = master.clone() { if m.is_file() { if let Ok(v) = Self::from_spool(m, Some(file.clone()), wake.clone()) { return v; } } }
        let (tx, rx) = crossbeam_channel::bounded(1);
        let f = file.clone();
        let w = wake.clone();
        std::thread::spawn(move || {
            let res = linkshot_video::import::open_video(&f, None, &cache_dir, &mut |_| true).map(|r| r.path().to_path_buf());
            let _ = tx.send(res);
            w();
        });
        Self { file: Some(file), master: None, loading: Some(rx), player: None, wake, frame: None, texture: None, tex_index: None, playing: false, position_ms: 0, duration_ms: 0, width: 0, height: 0, frames: 0, in_ms: None, out_ms: None, looping: false, error: None, scrub_drag: false, pending_open: None, audio: None, audio_muted: false, playback: None }
    }

    pub fn is_ready(&self) -> bool { self.player.is_some() }
    pub fn send(&self, c: PlayerCmd) { if let Some(p) = &self.player { p.send(c); } }
    pub fn toggle_play(&mut self) { if self.playing { self.send(PlayerCmd::Pause); } else { self.send(PlayerCmd::Play); } }
    pub fn seek(&mut self, ms: u32) { self.position_ms = ms.min(self.duration_ms); self.send(PlayerCmd::Seek(self.position_ms)); self.playback = None; }
    pub fn has_audio(&self) -> bool { self.audio.is_some() }
    pub fn toggle_audio_mute(&mut self) {
        self.audio_muted = !self.audio_muted;
        if self.audio_muted { self.playback = None; } else if self.playing { let at = self.position_ms; self.start_audio(at); }
    }
    fn start_audio(&mut self, at_ms: u32) {
        self.playback = None;
        if self.audio_muted { return; }
        let Some(a) = &self.audio else { return };
        match linkshot_audio::Playback::start(a, at_ms + PLAYBACK_LATENCY_MS, 1.0) {
            Ok(p) => self.playback = Some(p),
            Err(e) => { tracing::warn!("audio playback: {e:#}"); self.audio_muted = true; }
        }
    }
    /// Keeps the sound track on the frames: (re)starts it after seeks, loops and drift.
    fn sync_audio(&mut self) {
        if !self.playing || self.audio.is_none() || self.audio_muted { return; }
        let restart = match &self.playback {
            Some(p) => (p.position_ms() as i64 - self.position_ms as i64).abs() > 400,
            None => true,
        };
        if restart { let at = self.position_ms; self.start_audio(at); }
    }
    /// Export range from the in/out points.
    pub fn range(&self) -> (Option<u32>, Option<u32>) { (self.in_ms, self.out_ms) }

    pub fn poll(&mut self) {
        self.ensure_open();
        if let Some(rx) = &self.loading {
            match rx.try_recv() {
                Ok(Ok(master)) => {
                    self.loading = None;
                    match SpoolReader::open(&master) {
                        Ok(reader) => {
                            self.width = reader.width(); self.height = reader.height(); self.frames = reader.len(); self.duration_ms = reader.duration_ms();
                            self.player = Some(Player::spawn(Box::new(reader), self.wake.clone()));
                            self.master = Some(master);
                        }
                        Err(e) => self.error = Some(format!("{e:#}")),
                    }
                    if let Some(m) = &self.master { self.audio = audio_for(m); }
                }
                Ok(Err(e)) => { self.loading = None; self.error = Some(format!("{e:#}")); }
                Err(_) => {}
            }
        }
        let mut events = vec![];
        if let Some(p) = &self.player { while let Ok(ev) = p.rx.try_recv() { events.push(ev); } }
        for ev in events {
            match ev {
                PlayerEvent::Frame(f) => { self.position_ms = f.pts_ms; self.frame = Some(f); self.sync_audio(); }
                PlayerEvent::Playing(b) => { self.playing = b; if b { self.sync_audio(); } else { self.playback = None; } }
                PlayerEvent::Ended => { self.playing = false; self.position_ms = self.duration_ms; self.playback = None; }
                PlayerEvent::Error(e) => self.error = Some(e),
            }
        }
    }

    /// The current frame as an image (Save Frame as Image, Copy Image).
    pub fn current_image(&self) -> Option<image::RgbaImage> {
        let f = self.frame.as_ref()?;
        image::RgbaImage::from_raw(f.width, f.height, f.rgba.as_ref().clone())
    }

    fn texture(&mut self, ctx: &egui::Context) -> Option<egui::TextureId> {
        let f = self.frame.as_ref()?;
        if self.tex_index != Some(f.index) || self.texture.is_none() {
            let ci = egui::ColorImage::from_rgba_unmultiplied([f.width as usize, f.height as usize], &f.rgba);
            let opts = egui::TextureOptions { magnification: egui::TextureFilter::Nearest, minification: egui::TextureFilter::Linear, wrap_mode: egui::TextureWrapMode::ClampToEdge, mipmap_mode: None };
            match self.texture.as_mut() { Some(t) => t.set(ci, opts), None => self.texture = Some(ctx.load_texture("video-frame", ci, opts)) }
            self.tex_index = Some(f.index);
        }
        self.texture.as_ref().map(|t| t.id())
    }
}

/// Draws the player in place of the canvas.
pub fn video_canvas_ui(app: &mut App, ui: &mut egui::Ui, rect: Rect, ctx: &egui::Context) {
    let pal = app.pal;
    let Some(i) = app.editor.current else { return };
    let ws = crate::theme::argb_to_color32(app.settings.editor_workspace_color);
    ui.painter().rect_filled(rect, 0.0, ws);
    app.editor.canvas.view = rect;
    let video_rect = Rect::from_min_max(rect.min, Pos2::new(rect.right(), rect.bottom() - TRANSPORT_H));
    let transport = Rect::from_min_max(video_rect.left_bottom(), rect.max);

    // Keyboard: Space play/pause, ←/→ step, Home/End.
    if !ctx.wants_keyboard_input() {
        let (space, left, right, home, end) = ctx.input(|inp| (inp.key_pressed(egui::Key::Space), inp.key_pressed(egui::Key::ArrowLeft), inp.key_pressed(egui::Key::ArrowRight), inp.key_pressed(egui::Key::Home), inp.key_pressed(egui::Key::End)));
        let v = app.editor.docs[i].video.as_mut().unwrap();
        if space { v.toggle_play(); }
        if left { v.send(PlayerCmd::Step(-1)); }
        if right { v.send(PlayerCmd::Step(1)); }
        if home { v.seek(0); }
        if end { let d = v.duration_ms; v.seek(d); }
    }

    let v = app.editor.docs[i].video.as_mut().unwrap();
    v.poll();
    // Frame: zoom like the image canvas (fit requested → fit).
    let (fw, fh) = if v.width > 0 { (v.width as f32, v.height as f32) } else { let p = app.editor.docs[i].doc.page(); (p.capture_canvas_width as f32, p.capture_canvas_height as f32) };
    let avail = video_rect.shrink(30.0);
    let fit_zoom = (avail.width() / fw.max(1.0)).min(avail.height() / fh.max(1.0)).min(1.0).max(0.02);
    if app.editor.canvas.fit_requested || app.editor.canvas.fit { app.editor.canvas.zoom = fit_zoom; app.editor.canvas.fit = true; app.editor.canvas.fit_requested = false; app.editor.canvas.pan = Vec2::ZERO; }
    let zoom = app.editor.canvas.zoom;
    let size = Vec2::new(fw * zoom, fh * zoom);
    let origin = Pos2::new(if size.x < video_rect.width() { video_rect.center().x - size.x / 2.0 } else { video_rect.left() + 20.0 }, if size.y < video_rect.height() { video_rect.center().y - size.y / 2.0 } else { video_rect.top() + 20.0 }) + app.editor.canvas.pan;
    app.editor.canvas.origin = origin;
    let img_rect = Rect::from_min_size(origin, size);
    let clip = ui.painter().with_clip_rect(video_rect);
    clip.rect_filled(img_rect.translate(Vec2::new(0.0, 2.0)).expand(1.0), 0.0, Color32::from_black_alpha(80));
    let v = app.editor.docs[i].video.as_mut().unwrap();
    match v.texture(ctx) {
        Some(tid) => { clip.image(tid, img_rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE); }
        None => {
            // Poster from the document until the spool is ready.
            clip.rect_filled(img_rect, 0.0, Color32::from_gray(30));
            let msg = if let Some(e) = &v.error { e.clone() } else if v.loading.is_some() { "Decoding video…".to_string() } else { "Loading…".to_string() };
            clip.text(img_rect.center(), Align2::CENTER_CENTER, msg, FontId::proportional(14.0), if v.error.is_some() { pal.capture_red } else { Color32::from_white_alpha(170) });
            if v.loading.is_some() { ctx.request_repaint_after(std::time::Duration::from_millis(200)); }
        }
    }
    // Wheel / middle drag like the image canvas.
    let resp = ui.interact(video_rect, ui.id().with("video-canvas"), Sense::click_and_drag());
    let (scroll, ctrl) = ctx.input(|inp| (inp.raw_scroll_delta, inp.modifiers.ctrl));
    if resp.hovered() && scroll != Vec2::ZERO {
        if ctrl { if scroll.y > 0.0 { app.editor.canvas.zoom_step(1); } else if scroll.y < 0.0 { app.editor.canvas.zoom_step(-1); } }
        else { app.editor.canvas.pan += scroll; app.editor.canvas.fit = false; }
    }
    if resp.dragged_by(egui::PointerButton::Middle) { app.editor.canvas.pan += resp.drag_delta(); }
    if resp.clicked() { app.editor.docs[i].video.as_mut().unwrap().toggle_play(); }
    if resp.hovered() && !app.editor.docs[i].video.as_ref().unwrap().playing && app.editor.docs[i].video.as_ref().unwrap().is_ready() {
        // Play glyph hint
        let c = img_rect.center();
        clip.circle_filled(c, 28.0, Color32::from_black_alpha(110));
        clip.add(egui::Shape::convex_polygon(vec![c + Vec2::new(-9.0, -13.0), c + Vec2::new(14.0, 0.0), c + Vec2::new(-9.0, 13.0)], Color32::from_white_alpha(230), Stroke::NONE));
    }
    resp.context_menu(|ui| video_context_menu(app, ui, i));

    transport_ui(app, ui, transport, ctx, i);
    let v = app.editor.docs[i].video.as_ref().unwrap();
    if v.playing { ctx.request_repaint_after(std::time::Duration::from_millis(16)); }
}

fn video_context_menu(app: &mut App, ui: &mut egui::Ui, i: usize) {
    ui.set_min_width(220.0);
    if ui.button("Copy Frame").clicked() { copy_frame(app, i); ui.close(); }
    if ui.button("Save Frame as Image").clicked() { save_frame_as_image(app, i); ui.close(); }
    ui.separator();
    if ui.button("Create Animated GIF…").clicked() { open_gif_dialog(app, i); ui.close(); }
    if ui.button("Save as MP4…").clicked() { save_video_as(app, i, "mp4"); ui.close(); }
    ui.separator();
    if ui.button("Zoom to Fit").clicked() { app.editor.canvas.fit_requested = true; ui.close(); }
    if ui.button("Actual Size").clicked() { app.editor.canvas.zoom = 1.0; app.editor.canvas.fit = false; ui.close(); }
}

fn transport_ui(app: &mut App, ui: &mut egui::Ui, rect: Rect, _ctx: &egui::Context, i: usize) {
    let pal = app.pal;
    ui.painter().rect_filled(rect, 0.0, pal.main_panel);
    ui.painter().hline(rect.x_range(), rect.top(), Stroke::new(1.0_f32, pal.separator));
    let (playing, ready, pos, dur, in_ms, out_ms, frames, fps) = {
        let v = app.editor.docs[i].video.as_ref().unwrap();
        let fps = if v.frames > 1 && v.duration_ms > 0 { (v.frames as f32 * 1000.0 / v.duration_ms as f32).round() } else { 0.0 };
        (v.playing, v.is_ready(), v.position_ms, v.duration_ms, v.in_ms, v.out_ms, v.frames, fps)
    };
    let cy = rect.center().y;
    let mut x = rect.left() + 12.0;
    let glyph_btn = |ui: &mut egui::Ui, x: &mut f32, w: f32, tip: &str, enabled: bool, draw: &dyn Fn(&egui::Painter, Rect)| -> bool {
        let r = Rect::from_center_size(Pos2::new(*x + w / 2.0, cy), Vec2::new(w, 32.0));
        *x += w + 4.0;
        let resp = ui.interact(r, ui.id().with(tip), if enabled { Sense::click() } else { Sense::hover() });
        if resp.hovered() && enabled { ui.painter().rect_filled(r, 6.0, pal.control_bright); }
        draw(ui.painter(), r.shrink(8.0));
        enabled && resp.on_hover_text(tip).clicked()
    };
    let fg = if ready { pal.text_bright } else { pal.text };
    // Step back
    if glyph_btn(ui, &mut x, 32.0, "Previous frame (←)", ready, &|p, r| { p.vline(r.left() + 2.0, r.y_range(), Stroke::new(2.0_f32, fg)); p.add(egui::Shape::convex_polygon(vec![r.right_top(), Pos2::new(r.left() + 4.0, r.center().y), r.right_bottom()], fg, Stroke::NONE)); }) {
        app.editor.docs[i].video.as_mut().unwrap().send(PlayerCmd::Step(-1));
    }
    // Play / pause
    if glyph_btn(ui, &mut x, 40.0, if playing { "Pause (Space)" } else { "Play (Space)" }, ready, &|p, r| {
        p.circle_filled(r.center(), 15.0, pal.product);
        if playing { p.rect_filled(Rect::from_center_size(r.center() + Vec2::new(-4.0, 0.0), Vec2::new(4.0, 12.0)), 1.0, Color32::WHITE); p.rect_filled(Rect::from_center_size(r.center() + Vec2::new(4.0, 0.0), Vec2::new(4.0, 12.0)), 1.0, Color32::WHITE); }
        else { let c = r.center() + Vec2::new(1.5, 0.0); p.add(egui::Shape::convex_polygon(vec![c + Vec2::new(-5.0, -7.0), c + Vec2::new(7.0, 0.0), c + Vec2::new(-5.0, 7.0)], Color32::WHITE, Stroke::NONE)); }
    }) { app.editor.docs[i].video.as_mut().unwrap().toggle_play(); }
    // Step forward
    if glyph_btn(ui, &mut x, 32.0, "Next frame (→)", ready, &|p, r| { p.vline(r.right() - 2.0, r.y_range(), Stroke::new(2.0_f32, fg)); p.add(egui::Shape::convex_polygon(vec![r.left_top(), Pos2::new(r.right() - 4.0, r.center().y), r.left_bottom()], fg, Stroke::NONE)); }) {
        app.editor.docs[i].video.as_mut().unwrap().send(PlayerCmd::Step(1));
    }
    x += 6.0;
    let time = format!("{} / {}", linkshot_video::format_duration(pos), linkshot_video::format_duration(dur));
    ui.painter().text(Pos2::new(x, cy), Align2::LEFT_CENTER, &time, crate::theme::medium(12.5), pal.text_bright);
    x += 118.0;
    // Speaker: the recording's sound track (click to mute).
    let (has_audio, muted) = { let v = app.editor.docs[i].video.as_ref().unwrap(); (v.has_audio(), v.audio_muted) };
    if has_audio {
        if glyph_btn(ui, &mut x, 32.0, if muted { "Unmute" } else { "Mute" }, true, &|p, r| {
            let c = if muted { pal.text } else { fg };
            let body = Rect::from_min_max(Pos2::new(r.left() + 1.0, r.center().y - 4.0), Pos2::new(r.left() + 6.0, r.center().y + 4.0));
            p.rect_filled(body, 1.0, c);
            p.add(egui::Shape::convex_polygon(vec![Pos2::new(r.left() + 6.0, r.center().y - 4.0), Pos2::new(r.left() + 12.0, r.center().y - 9.0), Pos2::new(r.left() + 12.0, r.center().y + 9.0), Pos2::new(r.left() + 6.0, r.center().y + 4.0)], c, Stroke::NONE));
            if muted { p.line_segment([Pos2::new(r.left() + 2.0, r.center().y + 9.0), Pos2::new(r.right() - 1.0, r.center().y - 9.0)], Stroke::new(2.0_f32, pal.capture_red)); }
            else { for (k, rad) in [4.0f32, 8.0].iter().enumerate() { p.add(egui::Shape::line(arc_points(Pos2::new(r.left() + 12.0, r.center().y), *rad + 2.0, -0.9, 0.9, 8), Stroke::new(1.5_f32, c.gamma_multiply(1.0 - k as f32 * 0.25)))); } }
        }) { app.editor.docs[i].video.as_mut().unwrap().toggle_audio_mute(); }
        x += 4.0;
    }

    // Right side: in/out + save buttons.
    let mut rx = rect.right() - 12.0;
    let text_btn = |ui: &mut egui::Ui, rx: &mut f32, label: &str, tip: &str, accent: bool| -> bool {
        let galley = ui.painter().layout_no_wrap(label.to_string(), FontId::proportional(12.5), Color32::WHITE);
        let w = galley.size().x + 18.0;
        let r = Rect::from_min_size(Pos2::new(*rx - w, cy - 14.0), Vec2::new(w, 28.0));
        *rx -= w + 6.0;
        let resp = ui.interact(r, ui.id().with(label), Sense::click());
        let fill = if accent { if resp.hovered() { pal.product_bright } else { pal.product } } else if resp.hovered() { pal.control_bright } else { pal.control };
        ui.painter().rect_filled(r, 5.0, fill);
        ui.painter().galley(r.min + Vec2::new(9.0, 14.0 - galley.size().y / 2.0), galley, if accent { Color32::WHITE } else { pal.text_bright });
        resp.on_hover_text(tip).clicked()
    };
    let default_gif = app.settings.default_video_format.eq_ignore_ascii_case("gif");
    if text_btn(ui, &mut rx, "Save as MP4", "Export the video (or the in/out range) as an MP4 file", !default_gif) { save_video_as(app, i, "mp4"); }
    if text_btn(ui, &mut rx, "Create GIF", "Export the video (or the in/out range) as an animated GIF", default_gif) { open_gif_dialog(app, i); }
    if text_btn(ui, &mut rx, "Save Frame", "Save the current frame as an image in the tray", false) { save_frame_as_image(app, i); }
    let has_range = in_ms.is_some() || out_ms.is_some();
    if has_range && text_btn(ui, &mut rx, "Clear", "Clear the in/out points", false) { let v = app.editor.docs[i].video.as_mut().unwrap(); v.in_ms = None; v.out_ms = None; }
    if text_btn(ui, &mut rx, "Out", "Set the out point at the playhead (O)", false) { let v = app.editor.docs[i].video.as_mut().unwrap(); v.out_ms = Some(pos.max(v.in_ms.unwrap_or(0) + 1)); }
    if text_btn(ui, &mut rx, "In", "Set the in point at the playhead (I)", false) { let v = app.editor.docs[i].video.as_mut().unwrap(); v.in_ms = Some(pos.min(v.out_ms.unwrap_or(dur).saturating_sub(1))); }
    rx -= 8.0;

    // Scrubber.
    let track = Rect::from_min_max(Pos2::new(x, cy - 7.0), Pos2::new(rx, cy + 7.0));
    if track.width() > 40.0 && dur > 0 {
        let line = Rect::from_center_size(track.center(), Vec2::new(track.width(), 4.0));
        ui.painter().rect_filled(line, 2.0, pal.control_dark);
        let to_x = |ms: u32| track.left() + track.width() * (ms as f32 / dur as f32).clamp(0.0, 1.0);
        // in/out shading
        if has_range {
            let (a, b) = (to_x(in_ms.unwrap_or(0)), to_x(out_ms.unwrap_or(dur)));
            ui.painter().rect_filled(Rect::from_min_max(Pos2::new(a, line.top()), Pos2::new(b, line.bottom())), 2.0, pal.product.gamma_multiply(0.6));
            if let Some(m) = in_ms { ui.painter().vline(to_x(m), (cy - 11.0)..=(cy + 11.0), Stroke::new(2.0_f32, pal.green)); }
            if let Some(m) = out_ms { ui.painter().vline(to_x(m), (cy - 11.0)..=(cy + 11.0), Stroke::new(2.0_f32, pal.capture_red)); }
        }
        let px = to_x(pos);
        ui.painter().rect_filled(Rect::from_min_max(line.min, Pos2::new(px, line.bottom())), 2.0, pal.product_bright);
        ui.painter().circle_filled(Pos2::new(px, cy), 7.0, Color32::WHITE);
        ui.painter().circle_stroke(Pos2::new(px, cy), 7.0, Stroke::new(1.0_f32, pal.control_outline));
        let resp = ui.interact(track.expand2(Vec2::new(0.0, 6.0)), ui.id().with("scrub"), Sense::click_and_drag());
        if (resp.dragged() || resp.clicked()) && ready {
            if let Some(p) = resp.interact_pointer_pos() {
                let t = ((p.x - track.left()) / track.width()).clamp(0.0, 1.0);
                let ms = (t * dur as f32) as u32;
                let v = app.editor.docs[i].video.as_mut().unwrap();
                if !v.scrub_drag && v.playing { v.send(PlayerCmd::Pause); }
                v.scrub_drag = true;
                v.seek(ms);
            }
        }
        if resp.drag_stopped() { app.editor.docs[i].video.as_mut().unwrap().scrub_drag = false; }
        if resp.hovered() { resp.on_hover_text(format!("{frames} frames · {} fps", fps)); }
    }
    // I / O keys
    if !_ctx.wants_keyboard_input() {
        let (ki, ko) = _ctx.input(|inp| (inp.key_pressed(egui::Key::I), inp.key_pressed(egui::Key::O)));
        let v = app.editor.docs[i].video.as_mut().unwrap();
        if ki { v.in_ms = Some(pos.min(v.out_ms.unwrap_or(dur).saturating_sub(1))); }
        if ko { v.out_ms = Some(pos.max(v.in_ms.unwrap_or(0) + 1)); }
    }
}

pub fn copy_frame(app: &mut App, i: usize) {
    let img = app.editor.docs[i].video.as_ref().and_then(|v| v.current_image());
    match img {
        Some(img) => match linkshot_platform::clipboard::set_image(&img) { Ok(()) => app.editor.set_status("Frame copied to the clipboard."), Err(e) => app.editor.set_status(format!("Clipboard: {e}")) },
        None => app.editor.set_status("No frame to copy yet."),
    }
}

pub fn save_frame_as_image(app: &mut App, i: usize) {
    let Some(img) = app.editor.docs[i].video.as_ref().and_then(|v| v.current_image()) else { app.editor.set_status("No frame to save yet."); return };
    let Ok(png) = linkshot_render::encode_png(&img) else { return };
    let mut doc = linkshot_core::document::Document::from_png(img.width(), img.height(), png);
    let name = app.editor.docs[i].doc.display_name();
    doc.metadata.extra.insert("Application".into(), serde_json::Value::String(format!("Frame of {name}")));
    let paths = app.paths.clone();
    let s = app.settings.clone();
    app.editor.add_document(&egui::Context::default(), doc, &paths, &s);
    app.editor.set_status("Frame added to the tray as an image.");
}

// ------------------------------------------------------------------ export jobs

pub enum ExportMsg { Progress(f32), Done(anyhow::Result<PathBuf>) }

pub struct ExportJob {
    pub rx: crossbeam_channel::Receiver<ExportMsg>,
    pub label: String,
    pub progress: f32,
    pub started: Instant,
    /// Document the result belongs to (by master path, since indices shift).
    pub master: Option<PathBuf>,
    pub target: PathBuf,
    /// True when this is the automatic library save of a fresh recording.
    pub autosave: bool,
}

#[derive(Clone, Debug)]
pub enum ExportFormat { Gif(GifOptions), Mp4(Mp4Options) }

impl ExportFormat {
    pub fn extension(&self) -> &'static str { match self { ExportFormat::Gif(_) => "gif", ExportFormat::Mp4(_) => "mp4" } }
}

/// Starts a background export of `master` to `target`.
pub fn start_export(master: PathBuf, target: PathBuf, format: ExportFormat, autosave: bool, wake: Arc<dyn Fn() + Send + Sync>) -> ExportJob {
    let (tx, rx) = crossbeam_channel::unbounded();
    let label = format!("Saving {}…", target.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default());
    let (m, t) = (master.clone(), target.clone());
    // MP4 carries the sound track that sits next to the master.
    let format = match format { ExportFormat::Mp4(mut o) => { if o.audio.is_none() { o.audio = audio_for(&master); } ExportFormat::Mp4(o) } g => g };
    std::thread::Builder::new().name("linkshot-export".into()).spawn(move || {
        let res = (|| -> anyhow::Result<PathBuf> {
            let mut src = SpoolReader::open(&m)?;
            let tx2 = tx.clone();
            let w2 = wake.clone();
            let mut progress = move |p: f32| { let _ = tx2.send(ExportMsg::Progress(p)); w2(); true };
            match &format {
                ExportFormat::Gif(o) => { let r = linkshot_video::export_gif(&mut src, o, &t, &mut progress)?; tracing::info!("GIF {}: {} frames, {} KB, exact palette {}", t.display(), r.frames_written, r.bytes / 1024, r.exact_palette); }
                ExportFormat::Mp4(o) => { let r = linkshot_video::export_mp4(&mut src, o, &t, &mut progress)?; tracing::info!("MP4 {}: {} frames, {} KB, {}", t.display(), r.frames, r.bytes / 1024, r.encoder); }
            }
            Ok(t)
        })();
        let _ = tx.send(ExportMsg::Done(res));
        wake();
    }).expect("spawn export");
    ExportJob { rx, label, progress: 0.0, started: Instant::now(), master: Some(master), target, autosave }
}

/// GIF options from the saved dialog settings.
pub fn gif_options(s: &GifExportSettings, range: (Option<u32>, Option<u32>)) -> GifOptions {
    GifOptions { fps: s.fps, width: s.width, height: s.height, dithering: s.dithering, loops: if s.loop_forever { 0 } else { 1 }, skip_similar: true, start_ms: range.0, end_ms: range.1 }
}

pub fn mp4_options(settings: &Settings, range: (Option<u32>, Option<u32>)) -> Mp4Options {
    Mp4Options { fps: 0.0, width: 0, height: 0, crf: settings.mp4_crf, start_ms: range.0, end_ms: range.1, audio: None }
}

/// The "Create Animated GIF" dialog state.
pub struct GifDialog { pub doc_master: PathBuf, pub settings: GifExportSettings, pub src_w: u32, pub src_h: u32, pub range: (Option<u32>, Option<u32>), pub w_text: String, pub h_text: String, pub stem: String, pub target: Option<PathBuf> }

pub fn open_gif_dialog(app: &mut App, i: usize) {
    let Some(v) = app.editor.docs[i].video.as_ref() else { return };
    let Some(master) = v.master.clone() else { app.editor.set_status("The video is still being decoded."); return };
    let s = app.settings.gif_export.clone();
    app.editor.gif_dialog = Some(GifDialog { doc_master: master, src_w: v.width, src_h: v.height, range: v.range(), w_text: if s.width == 0 { v.width.to_string() } else { s.width.to_string() }, h_text: if s.height == 0 { v.height.to_string() } else { s.height.to_string() }, settings: s, stem: app.editor.docs[i].doc.display_name(), target: None });
}

/// Save As for a video document: `ext` is "gif" or "mp4" (GIF opens the options dialog first).
pub fn save_video_as(app: &mut App, i: usize, ext: &str) {
    if ext == "gif" { open_gif_dialog(app, i); return; }
    let Some(v) = app.editor.docs[i].video.as_ref() else { return };
    let Some(master) = v.master.clone() else { app.editor.set_status("The video is still being decoded."); return };
    let range = v.range();
    let default_dir = app.settings.last_save_folder.clone().unwrap_or_else(|| app.settings.library_dir(&app.paths));
    let stem = app.editor.docs[i].doc.display_name();
    let Some(path) = crate::share::save_video_dialog(Some(&default_dir), &stem, ext) else { return };
    let chosen_ext = path.extension().and_then(|e| e.to_str()).unwrap_or(ext).to_ascii_lowercase();
    app.settings.last_save_folder = path.parent().map(|p| p.to_path_buf());
    app.mark_settings_dirty();
    let format = if chosen_ext == "gif" { ExportFormat::Gif(gif_options(&app.settings.gif_export, range)) } else { ExportFormat::Mp4(mp4_options(&app.settings, range)) };
    let wake = app.wake();
    app.editor.export_job = Some(start_export(master, path, format, false, wake));
}

/// Draws the GIF dialog; returns true while open.
pub fn gif_dialog_ui(app: &mut App, ctx: &egui::Context) {
    let Some(mut d) = app.editor.gif_dialog.take() else { return };
    let pal = app.pal;
    let mut open = true;
    let mut create = false;
    let mut cancel = false;
    egui::Window::new("Create Animated GIF").collapsible(false).resizable(false).open(&mut open).anchor(Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
        ui.set_width(380.0);
        ui.label(egui::RichText::new("Optimize for").color(pal.text));
        ui.horizontal_wrapped(|ui| {
            for (p, name) in [(GifPreset::ScreenVideo, "Screen Video"), (GifPreset::HighMotion, "High Motion"), (GifPreset::ReducedFileSize, "Reduced File Size"), (GifPreset::Custom, "Custom")] {
                if ui.selectable_label(d.settings.preset == p, name).clicked() {
                    d.settings.apply_preset(p);
                    let (w, h) = linkshot_video::source::output_size(d.src_w.max(1), d.src_h.max(1), d.settings.width, d.settings.height);
                    d.w_text = w.to_string();
                    d.h_text = h.to_string();
                }
            }
        });
        ui.add_space(6.0);
        let custom = d.settings.preset == GifPreset::Custom;
        egui::Grid::new("gif-grid").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
            ui.label("Frame rate");
            ui.add_enabled_ui(custom, |ui| { if ui.add(egui::Slider::new(&mut d.settings.fps, 1.0..=30.0).suffix(" fps").step_by(1.0)).changed() {} });
            ui.end_row();
            ui.label("Dimensions");
            ui.horizontal(|ui| {
                ui.add_enabled_ui(custom, |ui| {
                    let wr = ui.add(egui::TextEdit::singleline(&mut d.w_text).desired_width(56.0));
                    ui.label("×");
                    let hr = ui.add(egui::TextEdit::singleline(&mut d.h_text).desired_width(56.0));
                    if wr.changed() && d.settings.lock_aspect { if let Ok(w) = d.w_text.trim().parse::<u32>() { d.h_text = ((w as f64 * d.src_h as f64 / d.src_w.max(1) as f64).round() as u32).max(1).to_string(); } }
                    if hr.changed() && d.settings.lock_aspect { if let Ok(h) = d.h_text.trim().parse::<u32>() { d.w_text = ((h as f64 * d.src_w as f64 / d.src_h.max(1) as f64).round() as u32).max(1).to_string(); } }
                    ui.checkbox(&mut d.settings.lock_aspect, "Lock aspect ratio");
                });
            });
            ui.end_row();
            ui.label("Options");
            ui.vertical(|ui| {
                ui.add_enabled(custom, egui::Checkbox::new(&mut d.settings.dithering, "Dithering (smoother gradients, larger file)"));
                ui.checkbox(&mut d.settings.loop_forever, "Loop forever");
            });
            ui.end_row();
        });
        if let (Some(a), b) = d.range { ui.label(egui::RichText::new(format!("Range: {} – {}", linkshot_video::format_duration(a), b.map(linkshot_video::format_duration).unwrap_or_else(|| "end".into()))).small().color(pal.text)); }
        else if let (None, Some(b)) = d.range { ui.label(egui::RichText::new(format!("Range: start – {}", linkshot_video::format_duration(b))).small().color(pal.text)); }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(egui::Button::new(egui::RichText::new("Create").color(Color32::WHITE)).fill(pal.product)).clicked() { create = true; }
                if ui.button("Cancel").clicked() { cancel = true; }
            });
        });
    });
    if create {
        // Commit dimensions (0 = original).
        let w = d.w_text.trim().parse::<u32>().unwrap_or(0);
        let h = d.h_text.trim().parse::<u32>().unwrap_or(0);
        if w == d.src_w && h == d.src_h { d.settings.width = 0; d.settings.height = 0; } else { d.settings.width = w; d.settings.height = h; }
        app.settings.gif_export = d.settings.clone();
        app.mark_settings_dirty();
        let default_dir = app.settings.last_save_folder.clone().unwrap_or_else(|| app.settings.library_dir(&app.paths));
        let target = match d.target.clone() { Some(t) => Some(t), None => crate::share::save_video_dialog(Some(&default_dir), &d.stem, "gif") };
        if let Some(path) = target {
            app.settings.last_save_folder = path.parent().map(|p| p.to_path_buf());
            let format = if path.extension().map(|e| e.eq_ignore_ascii_case("mp4")).unwrap_or(false) { ExportFormat::Mp4(mp4_options(&app.settings, d.range)) } else { ExportFormat::Gif(gif_options(&d.settings, d.range)) };
            let wake = app.wake();
            app.editor.export_job = Some(start_export(d.doc_master.clone(), path, format, false, wake));
        }
        return;
    }
    if open && !cancel { app.editor.gif_dialog = Some(d); }
}

/// Polls the running export; on completion attaches the file to its document (or adds it to the
/// tray) and refreshes the library.
pub fn poll_export(app: &mut App, ctx: &egui::Context) {
    let Some(job) = app.editor.export_job.as_mut() else { return };
    let mut done: Option<anyhow::Result<PathBuf>> = None;
    while let Ok(m) = job.rx.try_recv() {
        match m { ExportMsg::Progress(p) => job.progress = p, ExportMsg::Done(r) => done = Some(r) }
    }
    let Some(res) = done else { ctx.request_repaint_after(std::time::Duration::from_millis(200)); return };
    let job = app.editor.export_job.take().unwrap();
    match res {
        Ok(path) => {
            let idx = job.master.as_ref().and_then(|m| app.editor.docs.iter().position(|d| d.video.as_ref().and_then(|v| v.master.as_ref()) == Some(m)));
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
            let kind = if ext == "gif" { linkshot_core::enums::TrayItemType::AnimatedGif } else { linkshot_core::enums::TrayItemType::Video };
            match idx {
                Some(i) if app.editor.docs[i].doc.path.is_none() || job.autosave => {
                    let d = &mut app.editor.docs[i];
                    d.doc.path = Some(path.clone());
                    d.kind = kind;
                    d.saved_version = d.version;
                    if let Some(v) = d.video.as_mut() { v.file = Some(path.clone()); }
                }
                _ => {
                    // The exported file goes in the tray next to the original.
                    let paths = app.paths.clone();
                    let s = app.settings.clone();
                    if let Ok(doc) = crate::share::open_file(&path) {
                        let mut od = super::OpenDoc::new(doc);
                        od.saved_version = od.version;
                        od.kind = kind;
                        od.video = Some(VideoDoc::open_file(path.clone(), job.master.clone(), paths.video_cache_dir(), app.wake()));
                        let _ = s;
                        app.editor.docs.push(od);
                        app.editor.current = Some(app.editor.docs.len() - 1);
                        app.editor.canvas.fit_requested = true;
                    }
                }
            }
            app.editor.tray_dirty = true;
            app.editor.library_scanned = false;
            let secs = job.started.elapsed().as_secs_f32();
            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            app.editor.set_status(format!("Saved {} ({}) in {secs:.1} s.", path.display(), human_size(size)));
        }
        Err(e) => app.editor.set_status(format!("Export failed: {e:#}")),
    }
}

pub fn human_size(bytes: u64) -> String {
    if bytes >= 1 << 20 { format!("{:.1} MB", bytes as f64 / (1u64 << 20) as f64) } else { format!("{} KB", (bytes + 1023) / 1024) }
}

/// Progress toast for the running export.
pub fn export_progress_ui(app: &App, ui: &mut egui::Ui, canvas_rect: Rect, ctx: &egui::Context) {
    let Some(job) = app.editor.export_job.as_ref() else { return };
    let pal = app.pal;
    let r = Rect::from_center_size(Pos2::new(canvas_rect.center().x, canvas_rect.top() + 36.0), Vec2::new(360.0, 44.0));
    ui.painter().rect_filled(r.translate(Vec2::new(0.0, 3.0)), 8.0, Color32::from_black_alpha(80));
    ui.painter().rect_filled(r, 8.0, pal.dark_panel);
    ui.painter().rect_stroke(r, 8.0, Stroke::new(1.0_f32, pal.control_outline), egui::StrokeKind::Inside);
    ui.painter().text(Pos2::new(r.left() + 14.0, r.top() + 14.0), Align2::LEFT_CENTER, &job.label, FontId::proportional(12.5), pal.text_bright);
    let bar = Rect::from_min_max(Pos2::new(r.left() + 14.0, r.bottom() - 14.0), Pos2::new(r.right() - 14.0, r.bottom() - 8.0));
    ui.painter().rect_filled(bar, 3.0, pal.control_dark);
    ui.painter().rect_filled(Rect::from_min_max(bar.min, Pos2::new(bar.left() + bar.width() * job.progress.clamp(0.0, 1.0), bar.bottom())), 3.0, pal.product);
    let _ = icons::camera_icon;
    ctx.request_repaint_after(std::time::Duration::from_millis(100));
}

fn arc_points(c: Pos2, r: f32, a0: f32, a1: f32, n: usize) -> Vec<Pos2> {
    (0..=n).map(|k| { let a = a0 + (a1 - a0) * k as f32 / n as f32; Pos2::new(c.x + r * a.cos(), c.y + r * a.sin()) }).collect()
}

/// Finds the lossless master recorded alongside a saved video (`recordings/<stem>.lsv`).
pub fn master_for(file: &Path, paths: &Paths) -> Option<PathBuf> {
    let stem = file.file_stem()?;
    let p = paths.recordings_dir().join(format!("{}.lsv", stem.to_string_lossy()));
    p.is_file().then_some(p)
}
