//! Help ▸ About LinkShot: version, licence, credits, links and a copyable system summary for bug
//! reports. Its own window, so it opens from the Capture window, the Editor and the tray alike.

use crate::theme::Palette;
use egui::{Color32, RichText, Vec2};

pub const TITLE: &str = "About LinkShot";
pub const SIZE: [f32; 2] = [580.0, 790.0];
const REPO: &str = "https://github.com/ZeroDay-Labz/linkshot";

/// What the About window asks the app to do.
pub enum Request {
    CheckForUpdates,
}

/// Third-party software LinkShot ships or is built on: name, what it does here, licence, link.
const CREDITS: &[(&str, &str, &str, &str)] = &[
    ("FFmpeg", "video export, audio and webcam capture", "GPL-3.0-or-later", "https://ffmpeg.org"),
    ("Tesseract OCR", "Grab Text", "Apache-2.0", "https://github.com/tesseract-ocr/tesseract"),
    ("ocrs", "fallback text recognition", "MIT / Apache-2.0", "https://github.com/robertknight/ocrs"),
    ("egui / eframe", "user interface", "MIT / Apache-2.0", "https://github.com/emilk/egui"),
    ("wgpu", "GPU rendering", "MIT / Apache-2.0", "https://wgpu.rs"),
    ("tiny-skia", "annotation rendering", "BSD-3-Clause", "https://github.com/linebender/tiny-skia"),
    ("Inter", "typeface", "SIL OFL 1.1", "https://rsms.me/inter"),
];

#[derive(Default)]
pub struct About {
    icon: Option<egui::TextureHandle>,
    /// Renderer and GPU, filled in by the app from the eframe frame ("wgpu · Vulkan · AMD Radeon …").
    pub renderer: String,
    system: Option<String>,
    copied_at: Option<std::time::Instant>,
}

impl About {
    fn icon(&mut self, ctx: &egui::Context) -> Option<egui::TextureId> {
        if self.icon.is_none() {
            let img = image::load_from_memory(include_bytes!("../../../packaging/icons/linkshot-128.png")).ok()?.to_rgba8();
            let ci = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
            self.icon = Some(ctx.load_texture("about-icon", ci, egui::TextureOptions::LINEAR));
        }
        self.icon.as_ref().map(|t| t.id())
    }

    /// The system summary shown in the window and copied for bug reports.
    fn system(&mut self) -> &str {
        if self.system.is_none() {
            let mut lines = vec![
                format!("LinkShot {} ({})", env!("CARGO_PKG_VERSION"), linkshot_platform::update::install_kind().label()),
                format!("System: {}", os_name()),
            ];
            #[cfg(target_os = "linux")]
            {
                let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_else(|_| "unknown".into());
                let session = if linkshot_capture::is_wayland() { "Wayland" } else { "X11" };
                lines.push(format!("Desktop: {desktop} ({session}){}", if std::env::var_os("FLATPAK_ID").is_some() { ", Flatpak sandbox" } else { "" }));
                use linkshot_capture::live::Backend;
                let backend = match linkshot_capture::live::probe() {
                    Some(Backend::Kwin) => "KWin ScreenShot2".to_string(),
                    Some(Backend::Xcap) => "X11 (xcap)".to_string(),
                    Some(Backend::ScreenCast) => "ScreenCast portal (PipeWire)".to_string(),
                    Some(other) => format!("{other:?} portal"),
                    None => "frozen screenshot".to_string(),
                };
                lines.push(format!("Capture: {backend}"));
            }
            if !self.renderer.is_empty() { lines.push(format!("Renderer: {}", self.renderer)); }
            lines.push(format!("Architecture: {}", std::env::consts::ARCH));
            self.system = Some(lines.join("\n"));
        }
        self.system.as_deref().unwrap_or_default()
    }

    /// Draws the window while `open`; clears it when closed.
    pub fn ui(&mut self, ctx: &egui::Context, pal: &Palette, open: &mut bool) -> Option<Request> {
        if !*open { return None; }
        let mut request = None;
        let builder = egui::ViewportBuilder::default().with_title(TITLE).with_app_id(crate::app_id()).with_inner_size(SIZE).with_resizable(false).with_minimize_button(false).with_maximize_button(false);
        let icon = self.icon(ctx);
        let system = self.system().to_string();
        let copied = self.copied_at.is_some_and(|t| t.elapsed().as_secs_f32() < 2.0);
        let mut copy = false;
        let mut close = false;
        ctx.show_viewport_immediate(egui::ViewportId::from_hash_of("about-window"), builder, |ctx, _class| {
            if ctx.input(|i| i.viewport().close_requested() || i.key_pressed(egui::Key::Escape)) { close = true; }
            egui::CentralPanel::default().frame(egui::Frame::new().fill(pal.main_panel).inner_margin(egui::Margin::same(22))).show(ctx, |ui| {
                // Header: icon, name, version, description.
                ui.horizontal(|ui| {
                    if let Some(id) = icon { ui.add(egui::Image::new((id, Vec2::splat(64.0)))); }
                    ui.add_space(10.0);
                    ui.vertical(|ui| {
                        ui.add_space(4.0);
                        ui.label(RichText::new("LinkShot").font(crate::theme::semibold(26.0)).color(pal.text_bright));
                        ui.label(RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION"))).color(pal.text));
                    });
                });
                ui.add_space(10.0);
                ui.label(RichText::new("Screenshots, scrolling capture, screen recording with audio and webcam, text recognition and an image editor, for Linux and Windows.").color(pal.text));
                ui.add_space(12.0);
                ui.horizontal_wrapped(|ui| {
                    if ui.button("GitHub").on_hover_text(REPO).clicked() { let _ = linkshot_platform::open_url(REPO); }
                    if ui.button("Release Notes").clicked() { let _ = linkshot_platform::open_url(&format!("{REPO}/releases")); }
                    if ui.button("Report a Problem").on_hover_text("Opens the issue tracker; paste the system info below into your report.").clicked() { let _ = linkshot_platform::open_url(&format!("{REPO}/issues/new")); }
                    if ui.button("Check for Updates…").clicked() { request = Some(Request::CheckForUpdates); }
                });

                // Footer first (bottom-up), so the scrollable middle takes what is left.
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Max), |ui| {
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.add(egui::Button::new(RichText::new("Close").color(Color32::WHITE)).fill(pal.product).min_size(Vec2::new(84.0, 28.0))).clicked() { close = true; }
                            if ui.add(egui::Button::new(if copied { "Copied" } else { "Copy System Info" }).min_size(Vec2::new(0.0, 28.0))).clicked() { copy = true; }
                        });
                    });
                    ui.add_space(8.0);
                    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                        ui.add_space(14.0);
                        section(ui, pal, "License");
                        ui.label(RichText::new("LinkShot is free and open-source software, licensed under the MIT License or the Apache License 2.0, at your option.").color(pal.text));
                        ui.label(RichText::new("Copyright © 2026 ZeroDay-Labz and contributors.").color(pal.input_secondary));
                        ui.horizontal(|ui| {
                            ui.hyperlink_to("MIT License", format!("{REPO}/blob/main/LICENSE-MIT"));
                            ui.label(RichText::new("·").color(pal.input_secondary));
                            ui.hyperlink_to("Apache License 2.0", format!("{REPO}/blob/main/LICENSE-APACHE"));
                        });

                        ui.add_space(14.0);
                        section(ui, pal, "Third-party software");
                        egui::Grid::new("about-credits").num_columns(3).spacing([14.0, 4.0]).show(ui, |ui| {
                            for (name, role, licence, url) in CREDITS {
                                ui.hyperlink_to(RichText::new(*name).color(pal.product_bright), *url);
                                ui.label(RichText::new(*role).color(pal.text).small());
                                ui.label(RichText::new(*licence).color(pal.input_secondary).small());
                                ui.end_row();
                            }
                        });
                        ui.add_space(4.0);
                        ui.hyperlink_to(RichText::new("Full list and license texts").color(pal.product_bright).small(), format!("{REPO}/blob/main/packaging/THIRD-PARTY.md"));

                        ui.add_space(14.0);
                        section(ui, pal, "System");
                        egui::Frame::new().fill(pal.dark_panel).corner_radius(6.0).inner_margin(egui::Margin::same(10)).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(RichText::new(&system).monospace().size(11.5).color(pal.text));
                        });
                        });
                    });
                });
            });
        });
        if copy {
            if linkshot_platform::clipboard::set_text(&system).is_ok() { self.copied_at = Some(std::time::Instant::now()); }
        }
        if close { *open = false; self.system = None; }
        request
    }
}

fn section(ui: &mut egui::Ui, pal: &Palette, title: &str) {
    ui.label(RichText::new(title).font(crate::theme::semibold(13.5)).color(pal.text_bright));
    ui.add_space(2.0);
}

/// "Fedora Linux 44 (KDE Plasma Desktop Edition)", "Windows 11", …
fn os_name() -> String {
    #[cfg(target_os = "linux")]
    {
        let path = if std::path::Path::new("/run/host/os-release").exists() { "/run/host/os-release" } else { "/etc/os-release" };
        if let Ok(text) = std::fs::read_to_string(path) {
            if let Some(v) = text.lines().find_map(|l| l.strip_prefix("PRETTY_NAME=")) { return v.trim_matches('"').to_string(); }
        }
        "Linux".into()
    }
    #[cfg(windows)]
    { "Windows".into() }
    #[cfg(not(any(target_os = "linux", windows)))]
    { std::env::consts::OS.into() }
}
