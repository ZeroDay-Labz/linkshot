//! Upload destinations: Imgur (anonymous image hosting) and Nextcloud (your own server, WebDAV).
//!
//! Everything here blocks; callers run it on a worker thread. Nextcloud app passwords are kept in
//! the system keyring, never in the settings file.

use anyhow::{bail, Context, Result};
use base64::Engine;
use std::time::Duration;

fn user_agent() -> String { format!("LinkShot/{}", env!("CARGO_PKG_VERSION")) }

/// Where an upload ended up.
#[derive(Clone, Debug)]
pub struct Uploaded {
    /// Link to share.
    pub url: String,
    /// A link (Imgur) that deletes the upload again, if the service gives one.
    pub delete_url: Option<String>,
}

// ------------------------------------------------------------------ Imgur

/// Uploads an image anonymously to Imgur. `client_id` identifies the application (register one at
/// https://api.imgur.com/oauth2/addclient, "anonymous usage without user authorization").
pub fn imgur(bytes: &[u8], file_name: &str, client_id: &str) -> Result<Uploaded> {
    if client_id.trim().is_empty() { bail!("no Imgur client ID: add one under Preferences ▸ Sharing accounts"); }
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    let mut resp = ureq::post("https://api.imgur.com/3/image")
        .config().timeout_global(Some(Duration::from_secs(120))).http_status_as_error(false).build()
        .header("User-Agent", user_agent())
        .header("Authorization", format!("Client-ID {}", client_id.trim()))
        .send_form([("image", b64.as_str()), ("type", "base64"), ("name", file_name)])
        .context("could not reach Imgur")?;
    let status = resp.status();
    let text = resp.body_mut().read_to_string().unwrap_or_default();
    parse_imgur(status.as_u16(), &text)
}

fn parse_imgur(status: u16, text: &str) -> Result<Uploaded> {
    let v: serde_json::Value = serde_json::from_str(text).with_context(|| format!("Imgur answered {status} with something unexpected"))?;
    if status != 200 || !v["success"].as_bool().unwrap_or(false) {
        let msg = v["data"]["error"]["message"].as_str().or(v["data"]["error"].as_str()).unwrap_or("upload refused");
        bail!("Imgur: {msg} ({status})");
    }
    let url = v["data"]["link"].as_str().context("Imgur gave no link")?.to_string();
    let delete_url = v["data"]["deletehash"].as_str().map(|h| format!("https://imgur.com/delete/{h}"));
    Ok(Uploaded { url, delete_url })
}

// ------------------------------------------------------------------ Nextcloud

/// A Nextcloud account (the password is in the keyring under [`NextcloudAccount::keyring_entry`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NextcloudAccount {
    /// Server address, e.g. `https://cloud.example.com`.
    pub server: String,
    pub user: String,
    /// Folder for uploads, relative to the user's files (created when missing).
    pub folder: String,
    /// Create a public share link for each upload (otherwise the link needs a login).
    pub public_link: bool,
}

impl NextcloudAccount {
    fn base(&self) -> String { self.server.trim().trim_end_matches('/').to_string() }
    fn keyring_entry(&self) -> Result<keyring::Entry> {
        keyring::Entry::new("LinkShot Nextcloud", &format!("{}@{}", self.user, self.base())).context("system keyring")
    }
    /// Stores the app password in the system keyring.
    pub fn save_password(&self, password: &str) -> Result<()> { self.keyring_entry()?.set_password(password).context("could not store the password in the system keyring") }
    pub fn password(&self) -> Result<String> {
        self.keyring_entry()?.get_password().context("no stored password for this Nextcloud account: log in again under Preferences ▸ Sharing accounts")
    }
    pub fn forget_password(&self) { if let Ok(e) = self.keyring_entry() { let _ = e.delete_credential(); } }
    fn folder_path(&self) -> String { self.folder.trim().trim_matches('/').to_string() }
}

fn enc(segment: &str) -> String {
    // Percent-encode one path segment (RFC 3986 unreserved characters stay as they are).
    segment.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

fn basic(user: &str, password: &str) -> String { format!("Basic {}", base64::engine::general_purpose::STANDARD.encode(format!("{user}:{password}"))) }

/// Uploads a file into the account's folder and returns a link to it.
pub fn nextcloud(account: &NextcloudAccount, bytes: &[u8], file_name: &str) -> Result<Uploaded> {
    if account.server.trim().is_empty() || account.user.is_empty() { bail!("no Nextcloud account: log in under Preferences ▸ Sharing accounts"); }
    nextcloud_with_password(account, &account.password()?, bytes, file_name)
}

/// [`nextcloud`] with the password given instead of read from the keyring.
pub fn nextcloud_with_password(account: &NextcloudAccount, password: &str, bytes: &[u8], file_name: &str) -> Result<Uploaded> {
    let auth = basic(&account.user, password);
    let base = account.base();
    let dav = format!("{base}/remote.php/dav/files/{}", enc(&account.user));
    let folder = account.folder_path();
    // Create the folder (each level) if needed; "already exists" (405) is fine.
    let mut path = String::new();
    for seg in folder.split('/').filter(|s| !s.is_empty()) {
        path.push('/');
        path.push_str(&enc(seg));
        let req = ureq::http::Request::builder().method("MKCOL").uri(format!("{dav}{path}"))
            .header("User-Agent", user_agent()).header("Authorization", &auth).body(())?;
        let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(30))).http_status_as_error(false).allow_non_standard_methods(true).build().into();
        let resp = agent.run(req);
        match resp {
            Ok(r) if r.status() == 401 => bail!("Nextcloud refused the login (401): log in again under Preferences ▸ Sharing accounts"),
            Ok(_) => {}
            Err(e) => return Err(anyhow::Error::from(e).context(format!("could not reach {base}"))),
        }
    }
    let remote = format!("{path}/{}", enc(file_name));
    let resp = ureq::put(&format!("{dav}{remote}"))
        .config().timeout_global(Some(Duration::from_secs(300))).http_status_as_error(false).build()
        .header("User-Agent", user_agent()).header("Authorization", &auth)
        .send(bytes).with_context(|| format!("could not reach {base}"))?;
    match resp.status().as_u16() {
        200..=299 => {}
        401 => bail!("Nextcloud refused the login (401): log in again under Preferences ▸ Sharing accounts"),
        507 => bail!("Nextcloud: not enough storage space (507)"),
        s => bail!("Nextcloud upload failed ({s})"),
    }
    let share_path = format!("{}/{}", if folder.is_empty() { String::new() } else { format!("/{folder}") }, file_name);
    if !account.public_link {
        return Ok(Uploaded { url: format!("{base}/apps/files/?dir={}", enc(&format!("/{folder}"))), delete_url: None });
    }
    // Public link through the OCS sharing API.
    let mut resp = ureq::post(&format!("{base}/ocs/v2.php/apps/files_sharing/api/v1/shares?format=json"))
        .config().timeout_global(Some(Duration::from_secs(30))).http_status_as_error(false).build()
        .header("User-Agent", user_agent()).header("Authorization", &auth).header("OCS-APIRequest", "true")
        .send_form([("path", share_path.as_str()), ("shareType", "3")])
        .context("could not create the share link")?;
    let status = resp.status().as_u16();
    let text = resp.body_mut().read_to_string().unwrap_or_default();
    let v: serde_json::Value = serde_json::from_str(&text).with_context(|| format!("Nextcloud answered {status} with something unexpected"))?;
    let url = v["ocs"]["data"]["url"].as_str().with_context(|| format!("Nextcloud did not create a share link: {}", v["ocs"]["meta"]["message"].as_str().unwrap_or("unknown error")))?;
    Ok(Uploaded { url: url.to_string(), delete_url: None })
}

/// Nextcloud's browser login (Login Flow v2): returns the page to open and a poller that yields the
/// account and app password once the user has approved LinkShot in the browser.
pub struct LoginFlow { pub login_url: String, endpoint: String, token: String, server: String }

pub fn nextcloud_login_start(server: &str) -> Result<LoginFlow> {
    let mut base = server.trim().trim_end_matches('/').to_string();
    if !base.starts_with("http://") && !base.starts_with("https://") { base = format!("https://{base}"); }
    let mut resp = ureq::post(&format!("{base}/index.php/login/v2"))
        .config().timeout_global(Some(Duration::from_secs(20))).build()
        .header("User-Agent", user_agent())
        .send_empty().with_context(|| format!("could not reach {base}"))?;
    let v: serde_json::Value = serde_json::from_str(&resp.body_mut().read_to_string()?).context("this does not look like a Nextcloud server")?;
    Ok(LoginFlow {
        login_url: v["login"].as_str().context("no login page in the answer")?.to_string(),
        endpoint: v["poll"]["endpoint"].as_str().context("no poll endpoint")?.to_string(),
        token: v["poll"]["token"].as_str().context("no poll token")?.to_string(),
        server: base,
    })
}

impl LoginFlow {
    /// Polls until the user approved the login (or `cancel` is set, or 10 minutes pass).
    pub fn wait(&self, cancel: &std::sync::atomic::AtomicBool) -> Result<(NextcloudAccount, String)> {
        let t0 = std::time::Instant::now();
        while t0.elapsed() < Duration::from_secs(600) {
            if cancel.load(std::sync::atomic::Ordering::Relaxed) { bail!("cancelled"); }
            let resp = ureq::post(&self.endpoint)
                .config().timeout_global(Some(Duration::from_secs(20))).http_status_as_error(false).build()
                .header("User-Agent", user_agent())
                .send_form([("token", self.token.as_str())]);
            if let Ok(mut r) = resp {
                if r.status() == 200 {
                    let v: serde_json::Value = serde_json::from_str(&r.body_mut().read_to_string()?)?;
                    let server = v["server"].as_str().map(str::to_string).unwrap_or_else(|| self.server.clone());
                    let user = v["loginName"].as_str().context("no login name")?.to_string();
                    let password = v["appPassword"].as_str().context("no app password")?.to_string();
                    return Ok((NextcloudAccount { server, user, folder: "LinkShot".into(), public_link: true }, password));
                }
            }
            std::thread::sleep(Duration::from_secs(2));
        }
        bail!("the login was not approved within 10 minutes")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_imgur_answers() {
        let ok = r#"{"data":{"id":"abc","link":"https://i.imgur.com/abc.png","deletehash":"xyz"},"success":true,"status":200}"#;
        let u = parse_imgur(200, ok).unwrap();
        assert_eq!(u.url, "https://i.imgur.com/abc.png");
        assert_eq!(u.delete_url.as_deref(), Some("https://imgur.com/delete/xyz"));
        let bad = r#"{"data":{"error":"Invalid client_id","request":"/3/image","method":"POST"},"success":false,"status":403}"#;
        let e = parse_imgur(403, bad).unwrap_err().to_string();
        assert!(e.contains("Invalid client_id") && e.contains("403"), "{e}");
    }

    #[test]
    fn encodes_path_segments() {
        assert_eq!(enc("My Shots/ä"), "My%20Shots%2F%C3%A4");
        assert_eq!(enc("a-b_c.d~e"), "a-b_c.d~e");
    }
}
