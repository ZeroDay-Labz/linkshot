//! Global hotkeys (Print Screen etc.). Uses `global-hotkey` on X11/Windows; unavailable on Wayland.

use crossbeam_channel::{Receiver, Sender};
use linkshot_core::presets::HotKey;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HotkeyEvent {
    GlobalCapture,
    ShowHide,
    RepeatLast,
    VideoStartPause,
    VideoStop,
    DrawMode,
    Preset(String),
}

pub struct Hotkeys {
    rx: Receiver<HotkeyEvent>,
    #[allow(dead_code)]
    tx: Sender<HotkeyEvent>,
    /// Called when an event is queued so the UI thread repaints and handles it.
    wake: std::sync::Arc<dyn Fn() + Send + Sync>,
    #[cfg(feature = "hotkeys")]
    manager: Option<global_hotkey::GlobalHotKeyManager>,
    #[cfg(feature = "hotkeys")]
    registered: Vec<(global_hotkey::hotkey::HotKey, HotkeyEvent)>,
    #[cfg(feature = "hotkeys")]
    id_map: Option<std::sync::Arc<parking_lot::Mutex<Vec<(u32, HotkeyEvent)>>>>,
    pub available: bool,
    pub error: Option<String>,
    #[cfg(target_os = "linux")]
    portal: Option<portal::PortalShortcuts>,
}

impl Hotkeys {
    pub fn disabled() -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        Self {
            rx, tx, wake: std::sync::Arc::new(|| {}),
            #[cfg(feature = "hotkeys")] manager: None,
            #[cfg(feature = "hotkeys")] registered: vec![],
            #[cfg(feature = "hotkeys")] id_map: None,
            available: false, error: None,
            #[cfg(target_os = "linux")] portal: None,
        }
    }

    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        let wake: std::sync::Arc<dyn Fn() + Send + Sync> = std::sync::Arc::new(wake);
        #[cfg(feature = "hotkeys")]
        {
            #[cfg(target_os = "linux")]
            if linkshot_capture::is_wayland() {
                // Wayland: use the GlobalShortcuts portal (the desktop asks the user to confirm the bindings once).
                return Self { rx, tx, wake, manager: None, registered: vec![], id_map: None, available: true, error: None, portal: Some(portal::PortalShortcuts::default()) };
            }
            match global_hotkey::GlobalHotKeyManager::new() {
                Ok(m) => {
                    let tx2 = tx.clone();
                    let wake2 = wake.clone();
                    let registered: std::sync::Arc<parking_lot::Mutex<Vec<(u32, HotkeyEvent)>>> = Default::default();
                    let reg2 = registered.clone();
                    global_hotkey::GlobalHotKeyEvent::set_event_handler(Some(move |ev: global_hotkey::GlobalHotKeyEvent| {
                        if ev.state() != global_hotkey::HotKeyState::Pressed { return; }
                        if let Some((_, e)) = reg2.lock().iter().find(|(id, _)| *id == ev.id()) { let _ = tx2.send(e.clone()); wake2(); }
                    }));
                    return Self { rx, tx, wake, manager: Some(m), registered: vec![], id_map: Some(registered), available: true, error: None, #[cfg(target_os = "linux")] portal: None };
                }
                Err(e) => return Self { rx, tx, wake, manager: None, registered: vec![], id_map: None, available: false, error: Some(e.to_string()), #[cfg(target_os = "linux")] portal: None },
            }
        }
        #[cfg(not(feature = "hotkeys"))]
        Self { rx, tx, wake, available: false, error: Some("built without hotkey support".into()), #[cfg(target_os = "linux")] portal: None }
    }

    pub fn poll(&self) -> Vec<HotkeyEvent> {
        self.rx.try_iter().collect()
    }

    /// Re-registers all hotkeys from the current bindings.
    pub fn apply(&mut self, bindings: &[(HotKey, HotkeyEvent)]) {
        #[cfg(target_os = "linux")]
        if let Some(p) = self.portal.as_mut() {
            p.apply(bindings, self.tx.clone(), self.wake.clone());
            return;
        }
        #[cfg(feature = "hotkeys")]
        {
            let Some(m) = self.manager.as_ref() else { return };
            for (hk, _) in self.registered.drain(..) { let _ = m.unregister(hk); }
            if let Some(map) = &self.id_map { map.lock().clear(); }
            for (hk, ev) in bindings {
                if hk.is_none() { continue; }
                let Some(code) = wpf_key_to_code(&hk.key) else { continue };
                let mut mods = global_hotkey::hotkey::Modifiers::empty();
                if hk.ctrl { mods |= global_hotkey::hotkey::Modifiers::CONTROL; }
                if hk.alt { mods |= global_hotkey::hotkey::Modifiers::ALT; }
                if hk.shift { mods |= global_hotkey::hotkey::Modifiers::SHIFT; }
                if hk.win { mods |= global_hotkey::hotkey::Modifiers::SUPER; }
                let ghk = global_hotkey::hotkey::HotKey::new(if mods.is_empty() { None } else { Some(mods) }, code);
                match m.register(ghk) {
                    Ok(()) => { if let Some(map) = &self.id_map { map.lock().push((ghk.id(), ev.clone())); } self.registered.push((ghk, ev.clone())); }
                    Err(e) => tracing::warn!("hotkey {} not registered: {e}", hk.display()),
                }
            }
        }
        #[cfg(not(feature = "hotkeys"))]
        { let _ = bindings; }
    }
}

#[cfg(feature = "hotkeys")]
fn wpf_key_to_code(key: &str) -> Option<global_hotkey::hotkey::Code> {
    use global_hotkey::hotkey::Code;
    Some(match key {
        "PrintScreen" | "Snapshot" => Code::PrintScreen,
        "Pause" => Code::Pause,
        "Scroll" => Code::ScrollLock,
        "Insert" => Code::Insert,
        "Delete" => Code::Delete,
        "Home" => Code::Home,
        "End" => Code::End,
        "PageUp" | "Prior" => Code::PageUp,
        "PageDown" | "Next" => Code::PageDown,
        "Space" => Code::Space,
        "Tab" => Code::Tab,
        "Return" | "Enter" => Code::Enter,
        "Escape" => Code::Escape,
        "Back" => Code::Backspace,
        "Oem2" | "OemQuestion" => Code::Slash,
        "Oem3" | "OemTilde" => Code::Backquote,
        "OemMinus" => Code::Minus,
        "OemPlus" => Code::Equal,
        "Oem4" | "OemOpenBrackets" => Code::BracketLeft,
        "Oem6" | "OemCloseBrackets" => Code::BracketRight,
        "Oem5" | "OemPipe" => Code::Backslash,
        "Oem1" | "OemSemicolon" => Code::Semicolon,
        "Oem7" | "OemQuotes" => Code::Quote,
        "OemComma" => Code::Comma,
        "OemPeriod" => Code::Period,
        "Up" => Code::ArrowUp, "Down" => Code::ArrowDown, "Left" => Code::ArrowLeft, "Right" => Code::ArrowRight,
        k if k.len() == 1 && k.chars().all(|c| c.is_ascii_uppercase()) => match k { "A" => Code::KeyA, "B" => Code::KeyB, "C" => Code::KeyC, "D" => Code::KeyD, "E" => Code::KeyE, "F" => Code::KeyF, "G" => Code::KeyG, "H" => Code::KeyH, "I" => Code::KeyI, "J" => Code::KeyJ, "K" => Code::KeyK, "L" => Code::KeyL, "M" => Code::KeyM, "N" => Code::KeyN, "O" => Code::KeyO, "P" => Code::KeyP, "Q" => Code::KeyQ, "R" => Code::KeyR, "S" => Code::KeyS, "T" => Code::KeyT, "U" => Code::KeyU, "V" => Code::KeyV, "W" => Code::KeyW, "X" => Code::KeyX, "Y" => Code::KeyY, "Z" => Code::KeyZ, _ => return None },
        k if k.len() == 2 && k.starts_with('D') => match &k[1..] { "0" => Code::Digit0, "1" => Code::Digit1, "2" => Code::Digit2, "3" => Code::Digit3, "4" => Code::Digit4, "5" => Code::Digit5, "6" => Code::Digit6, "7" => Code::Digit7, "8" => Code::Digit8, "9" => Code::Digit9, _ => return None },
        k if k.starts_with('F') && k[1..].parse::<u8>().is_ok() => match &k[1..] { "1" => Code::F1, "2" => Code::F2, "3" => Code::F3, "4" => Code::F4, "5" => Code::F5, "6" => Code::F6, "7" => Code::F7, "8" => Code::F8, "9" => Code::F9, "10" => Code::F10, "11" => Code::F11, "12" => Code::F12, _ => return None },
        _ => return None,
    })
}

/// Converts an egui key press into the stored key name (for the hotkey capture field).
pub fn egui_key_to_wpf(key: egui::Key) -> Option<&'static str> {
    use egui::Key as K;
    Some(match key {
        K::A => "A", K::B => "B", K::C => "C", K::D => "D", K::E => "E", K::F => "F", K::G => "G", K::H => "H", K::I => "I", K::J => "J", K::K => "K", K::L => "L", K::M => "M",
        K::N => "N", K::O => "O", K::P => "P", K::Q => "Q", K::R => "R", K::S => "S", K::T => "T", K::U => "U", K::V => "V", K::W => "W", K::X => "X", K::Y => "Y", K::Z => "Z",
        K::Num0 => "D0", K::Num1 => "D1", K::Num2 => "D2", K::Num3 => "D3", K::Num4 => "D4", K::Num5 => "D5", K::Num6 => "D6", K::Num7 => "D7", K::Num8 => "D8", K::Num9 => "D9",
        K::F1 => "F1", K::F2 => "F2", K::F3 => "F3", K::F4 => "F4", K::F5 => "F5", K::F6 => "F6", K::F7 => "F7", K::F8 => "F8", K::F9 => "F9", K::F10 => "F10", K::F11 => "F11", K::F12 => "F12",
        K::Slash => "Oem2", K::Backtick => "Oem3", K::Minus => "OemMinus", K::Plus | K::Equals => "OemPlus", K::OpenBracket => "Oem4", K::CloseBracket => "Oem6", K::Backslash => "Oem5", K::Semicolon => "Oem1", K::Quote => "Oem7", K::Comma => "OemComma", K::Period => "OemPeriod",
        K::Insert => "Insert", K::Delete => "Delete", K::Home => "Home", K::End => "End", K::PageUp => "PageUp", K::PageDown => "PageDown", K::Space => "Space", K::Tab => "Tab", K::Enter => "Return",
        K::ArrowUp => "Up", K::ArrowDown => "Down", K::ArrowLeft => "Left", K::ArrowRight => "Right",
        _ => return None,
    })
}

#[cfg(target_os = "linux")]
mod portal {
    //! `org.freedesktop.portal.GlobalShortcuts` (KDE Plasma 5.27+/6, GNOME 48+).
    use super::HotkeyEvent;
    use linkshot_core::presets::HotKey;

    #[derive(Default)]
    pub struct PortalShortcuts {
        generation: u64,
        thread: Option<std::thread::JoinHandle<()>>,
        stop: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    }

    fn trigger(hk: &HotKey) -> String {
        // xdg-shortcuts syntax: "CTRL+SHIFT+x", keys are XKB keysym names.
        let mut parts = vec![];
        if hk.ctrl { parts.push("CTRL".to_string()); }
        if hk.alt { parts.push("ALT".to_string()); }
        if hk.shift { parts.push("SHIFT".to_string()); }
        if hk.win { parts.push("LOGO".to_string()); }
        let key = match hk.key.as_str() {
            "PrintScreen" | "Snapshot" => "Print".to_string(),
            "Oem2" => "slash".to_string(), "Oem3" => "grave".to_string(), "OemMinus" => "minus".to_string(), "OemPlus" => "equal".to_string(),
            "Oem4" => "bracketleft".to_string(), "Oem6" => "bracketright".to_string(), "Oem5" => "backslash".to_string(), "Oem1" => "semicolon".to_string(),
            "Oem7" => "apostrophe".to_string(), "OemComma" => "comma".to_string(), "OemPeriod" => "period".to_string(),
            "PageUp" => "Prior".to_string(), "PageDown" => "Next".to_string(), "Back" => "BackSpace".to_string(),
            "Return" => "Return".to_string(), "Space" => "space".to_string(), "Up" => "Up".to_string(), "Down" => "Down".to_string(), "Left" => "Left".to_string(), "Right" => "Right".to_string(),
            k if k.len() == 2 && k.starts_with('D') => k[1..].to_string(),
            k if k.len() == 1 => k.to_lowercase(),
            k => k.to_string(),
        };
        parts.push(key);
        parts.join("+")
    }

    impl PortalShortcuts {
        pub fn apply(&mut self, bindings: &[(HotKey, HotkeyEvent)], tx: crossbeam_channel::Sender<HotkeyEvent>, wake: std::sync::Arc<dyn Fn() + Send + Sync>) {
            if let Some(s) = &self.stop { s.store(true, std::sync::atomic::Ordering::Relaxed); }
            self.generation += 1;
            let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            self.stop = Some(stop.clone());
            let bindings: Vec<(String, String, String, HotkeyEvent)> = bindings.iter().filter(|(hk, _)| !hk.is_none()).enumerate().map(|(i, (hk, ev))| {
                let (id, desc) = match ev {
                    HotkeyEvent::GlobalCapture => ("capture".to_string(), "Capture".to_string()),
                    HotkeyEvent::ShowHide => ("show-hide".to_string(), "Show/hide linkshot".to_string()),
                    HotkeyEvent::RepeatLast => ("repeat".to_string(), "Repeat last capture".to_string()),
                    HotkeyEvent::VideoStartPause => ("video-start".to_string(), "Video start/pause/resume".to_string()),
                    HotkeyEvent::VideoStop => ("video-stop".to_string(), "Video stop".to_string()),
                    HotkeyEvent::DrawMode => ("draw-mode".to_string(), "Enter/exit draw mode".to_string()),
                    HotkeyEvent::Preset(p) => (format!("preset-{i}-{}", p.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>()), "Capture with preset".to_string()),
                };
                (id, desc, trigger(hk), ev.clone())
            }).collect();
            if bindings.is_empty() { return; }
            self.thread = std::thread::Builder::new().name("global-shortcuts".into()).spawn(move || {
                let res = async_io::block_on(async move {
                    use ashpd::desktop::global_shortcuts::{GlobalShortcuts, NewShortcut};
                    use futures_util::StreamExt;
                    let gs = GlobalShortcuts::new().await?;
                    let session = gs.create_session(ashpd::desktop::CreateSessionOptions::default()).await?;
                    let shortcuts: Vec<NewShortcut> = bindings.iter().map(|(id, desc, trig, _)| NewShortcut::new(id.clone(), desc.clone()).preferred_trigger(Some(trig.as_str()))).collect();
                    let _bound = gs.bind_shortcuts(&session, &shortcuts, None, ashpd::desktop::global_shortcuts::BindShortcutsOptions::default()).await?.response()?;
                    let mut activated = gs.receive_activated().await?;
                    while let Some(ev) = activated.next().await {
                        if stop.load(std::sync::atomic::Ordering::Relaxed) { break; }
                        if let Some((_, _, _, e)) = bindings.iter().find(|(id, _, _, _)| id == ev.shortcut_id()) { let _ = tx.send(e.clone()); wake(); }
                    }
                    Ok::<(), ashpd::Error>(())
                });
                if let Err(e) = res { tracing::warn!("GlobalShortcuts portal: {e}"); }
            }).ok();
        }
    }
}
