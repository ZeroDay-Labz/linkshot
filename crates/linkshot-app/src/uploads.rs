//! Uploads to Imgur and Nextcloud from the Share menus: run on worker threads, the link goes to the
//! clipboard when done, and recent uploads are remembered (with Imgur's delete link).

use crate::app::App;
use linkshot_core::enums::output_ids;
use linkshot_core::settings::{Settings, UploadRecord};
use linkshot_platform::upload::{self, NextcloudAccount};
use std::sync::Arc;
use std::time::Instant;

pub struct UploadJob {
    rx: crossbeam_channel::Receiver<anyhow::Result<UploadRecord>>,
    pub label: String,
    pub started: Instant,
}

/// The Imgur application id: the user's own, or the one release builds carry.
pub fn imgur_client_id(s: &Settings) -> String {
    if !s.imgur_client_id.trim().is_empty() { return s.imgur_client_id.trim().to_string(); }
    built_in_imgur_id().unwrap_or_default().to_string()
}

/// The Imgur id baked in at build time (release builds get it from the `IMGUR_CLIENT_ID` secret).
pub fn built_in_imgur_id() -> Option<&'static str> {
    option_env!("LINKSHOT_IMGUR_CLIENT_ID").filter(|s| !s.trim().is_empty())
}

pub fn nextcloud_account(s: &Settings) -> NextcloudAccount {
    NextcloudAccount { server: s.nextcloud_server.clone(), user: s.nextcloud_user.clone(), folder: s.nextcloud_folder.clone(), public_link: s.nextcloud_public_link }
}

/// Starts uploading `bytes` (named `file_name`) to `service` (an `output_ids` id).
pub fn start(service: &str, bytes: Vec<u8>, file_name: String, s: &Settings, wake: Arc<dyn Fn() + Send + Sync>) -> UploadJob {
    let (tx, rx) = crossbeam_channel::bounded(1);
    let title = output_ids::title(service).to_string();
    let label = format!("Uploading to {title}…");
    let (service, client_id, account) = (service.to_string(), imgur_client_id(s), nextcloud_account(s));
    std::thread::Builder::new().name("upload".into()).spawn(move || {
        let res = match service.as_str() {
            output_ids::IMGUR => upload::imgur(&bytes, &file_name, &client_id),
            output_ids::NEXTCLOUD => upload::nextcloud(&account, &bytes, &file_name),
            other => Err(anyhow::anyhow!("{} uploads are not supported", output_ids::title(other))),
        };
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
        let _ = tx.send(res.map(|u| UploadRecord { service: title, url: u.url, delete_url: u.delete_url, file_name, when: now }));
        wake();
    }).expect("spawn upload");
    UploadJob { rx, label, started: Instant::now() }
}

/// Collects finished uploads: link to the clipboard, a status message, the history entry.
pub fn poll(app: &mut App) {
    let mut i = 0;
    while i < app.editor.uploads.len() {
        let Ok(res) = app.editor.uploads[i].rx.try_recv() else { i += 1; continue };
        let job = app.editor.uploads.remove(i);
        match res {
            Ok(rec) => {
                let copied = linkshot_platform::clipboard::set_text(&rec.url).is_ok();
                app.editor.set_status(format!("{} in {:.1} s: {}{}", rec.service, job.started.elapsed().as_secs_f32(), rec.url, if copied { "  (link copied)" } else { "" }));
                app.settings.upload_history.insert(0, rec);
                app.settings.upload_history.truncate(50);
                app.mark_settings_dirty();
            }
            Err(e) => {
                tracing::warn!("upload failed: {e:#}");
                app.editor.set_status(format!("Upload failed: {e:#}"));
            }
        }
    }
}

/// Nextcloud browser login in progress.
pub struct NextcloudLogin {
    rx: crossbeam_channel::Receiver<LoginMsg>,
    cancel: Arc<std::sync::atomic::AtomicBool>,
    pub waiting_for_browser: bool,
}

enum LoginMsg { Opened, Done(anyhow::Result<(NextcloudAccount, String)>) }

impl NextcloudLogin {
    pub fn start(server: String, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let c = cancel.clone();
        std::thread::Builder::new().name("nextcloud-login".into()).spawn(move || {
            let res = (|| {
                let flow = upload::nextcloud_login_start(&server)?;
                linkshot_platform::open_url(&flow.login_url)?;
                let _ = tx.send(LoginMsg::Opened);
                wake();
                let (account, password) = flow.wait(&c)?;
                account.save_password(&password)?;
                Ok((account, password))
            })();
            let _ = tx.send(LoginMsg::Done(res));
            wake();
        }).expect("spawn login");
        Self { rx, cancel, waiting_for_browser: false }
    }
    pub fn cancel(&self) { self.cancel.store(true, std::sync::atomic::Ordering::Relaxed); }
    /// `Some` when the login finished (successfully or not).
    pub fn poll(&mut self) -> Option<anyhow::Result<NextcloudAccount>> {
        while let Ok(m) = self.rx.try_recv() {
            match m { LoginMsg::Opened => self.waiting_for_browser = true, LoginMsg::Done(r) => return Some(r.map(|(a, _)| a)) }
        }
        None
    }
}

/// Preferences ▸ Sharing accounts. Returns true when settings changed.
pub fn accounts_ui(app: &mut App, ui: &mut egui::Ui) -> bool {
    let pal = app.pal;
    let mut changed = false;
    // Nextcloud.
    ui.label(egui::RichText::new("Nextcloud").strong());
    if let Some(res) = app.editor.nextcloud_login.as_mut().and_then(|l| l.poll()) {
        app.editor.nextcloud_login = None;
        match res {
            Ok(a) => {
                app.settings.nextcloud_server = a.server;
                app.settings.nextcloud_user = a.user;
                if app.settings.nextcloud_folder.trim().is_empty() { app.settings.nextcloud_folder = a.folder; }
                app.editor.set_status(format!("Connected to Nextcloud as {}.", app.settings.nextcloud_user));
                changed = true;
            }
            Err(e) => app.editor.nextcloud_error = Some(format!("{e:#}")),
        }
    }
    let s = &mut app.settings;
    if !s.nextcloud_user.is_empty() {
        ui.label(format!("Connected as {} on {}", s.nextcloud_user, s.nextcloud_server));
        ui.horizontal(|ui| { ui.label("Upload folder:"); changed |= ui.text_edit_singleline(&mut s.nextcloud_folder).changed(); });
        changed |= ui.checkbox(&mut s.nextcloud_public_link, "Create a public link for each upload (otherwise the link needs a login)").changed();
        if ui.button("Log out").clicked() {
            nextcloud_account(s).forget_password();
            s.nextcloud_user.clear();
            changed = true;
        }
    } else if let Some(login) = app.editor.nextcloud_login.as_ref() {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(if login.waiting_for_browser { "Approve LinkShot in your browser…" } else { "Contacting the server…" });
            if ui.button("Cancel").clicked() { login.cancel(); }
        });
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(300));
    } else {
        ui.horizontal(|ui| {
            ui.label("Server:");
            ui.add(egui::TextEdit::singleline(&mut s.nextcloud_server).hint_text("https://cloud.example.com").desired_width(240.0));
            if ui.add_enabled(!s.nextcloud_server.trim().is_empty(), egui::Button::new("Log in…")).on_hover_text("Opens your Nextcloud in the browser to grant LinkShot an app password; nothing is typed here").clicked() {
                app.editor.nextcloud_error = None;
                app.editor.nextcloud_login = Some(NextcloudLogin::start(s.nextcloud_server.clone(), app.editor.wake.clone()));
                changed = true;
            }
        });
    }
    if let Some(e) = &app.editor.nextcloud_error { ui.label(egui::RichText::new(e).small().color(pal.capture_red)); }
    ui.add_space(8.0);
    // Imgur.
    let s = &mut app.settings;
    ui.label(egui::RichText::new("Imgur").strong());
    ui.label(egui::RichText::new("Uploads are anonymous: anyone with the link can see the image. Delete links are kept below.").small());
    ui.horizontal(|ui| {
        ui.label("Client ID:");
        let hint = if built_in_imgur_id().is_some() { "built in (optional: your own)" } else { "required: get one free at api.imgur.com" };
        changed |= ui.add(egui::TextEdit::singleline(&mut s.imgur_client_id).hint_text(hint).desired_width(240.0)).changed();
    });
    // Recent uploads.
    if !s.upload_history.is_empty() {
        ui.add_space(8.0);
        ui.label(egui::RichText::new("Recent uploads").strong());
        let mut remove = None;
        egui::Grid::new("upload-history").num_columns(3).spacing([10.0, 4.0]).show(ui, |ui| {
            for (i, r) in s.upload_history.iter().take(10).enumerate() {
                ui.label(egui::RichText::new(&r.service).small());
                ui.hyperlink_to(egui::RichText::new(&r.file_name).small(), &r.url).on_hover_text(&r.url);
                ui.horizontal(|ui| {
                    if ui.small_button("Copy link").clicked() { let _ = linkshot_platform::clipboard::set_text(&r.url); }
                    if let Some(d) = &r.delete_url { if ui.small_button("Delete…").on_hover_text("Opens the service's delete page").clicked() { let _ = linkshot_platform::open_url(d); } }
                    if ui.small_button("Forget").clicked() { remove = Some(i); }
                });
                ui.end_row();
            }
        });
        if let Some(i) = remove { s.upload_history.remove(i); changed = true; }
    }
    changed
}
