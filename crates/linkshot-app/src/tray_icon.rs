//! Notification-area icon with the tray menu.
//!
//! Linux uses a native StatusNotifierItem (`ksni`): a left click on the icon arrives as
//! `Activate` and opens the Capture window, a middle click starts a capture, and the right-click
//! menu is a D-Bus menu. (`libappindicator`-based trays never report clicks, which is why the
//! `tray-icon` crate is only used on the other platforms.)

use crossbeam_channel::Receiver;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrayEvent {
    OpenEditor,
    OpenCaptureWindow,
    ToggleWidget,
    Preferences,
    Exit,
    Help,
    /// Left click on the icon: bring the Capture window to the front.
    IconClicked,
    /// Middle click on the icon: capture with the selected preset.
    Capture,
}

pub struct Tray {
    rx: Receiver<TrayEvent>,
    pub available: bool,
}

impl Tray {
    pub fn poll(&self) -> Vec<TrayEvent> { self.rx.try_iter().collect() }

    pub fn disabled() -> Self {
        let (_tx, rx) = crossbeam_channel::unbounded::<TrayEvent>();
        Self { rx, available: false }
    }

    /// Creates the tray icon. `wake` is called whenever an event is queued so the UI thread
    /// repaints and handles it right away.
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Self {
        let (tx, rx) = crossbeam_channel::unbounded::<TrayEvent>();
        #[cfg(feature = "tray")]
        {
            let available = platform::start(tx, std::sync::Arc::new(wake));
            return Self { rx, available };
        }
        #[allow(unreachable_code)]
        { let _ = (tx, wake); Self { rx, available: false } }
    }
}

/// A simple generated icon (RGBA): blue rounded disc with a white lens ring.
#[cfg(feature = "tray")]
fn icon_rgba(size: u32) -> Vec<u8> {
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    let c = (size as f32 - 1.0) / 2.0;
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - c;
            let dy = y as f32 - c;
            let d = (dx * dx + dy * dy).sqrt();
            let i = ((y * size + x) * 4) as usize;
            if d <= c - 0.5 {
                let (r, g, b) = if d <= c * 0.35 { (255, 255, 255) } else if d <= c * 0.55 { (0x2C, 0x7C, 0xB2) } else if d <= c * 0.7 { (255, 255, 255) } else { (0x2C, 0x7C, 0xB2) };
                rgba[i..i + 4].copy_from_slice(&[r, g, b, 255]);
            }
        }
    }
    rgba
}

#[cfg(all(feature = "tray", target_os = "linux"))]
mod platform {
    use super::{icon_rgba, TrayEvent};
    use crossbeam_channel::Sender;
    use ksni::blocking::TrayMethods;
    use ksni::menu::{MenuItem, StandardItem};
    use std::sync::Arc;

    struct Sni { tx: Sender<TrayEvent>, wake: Arc<dyn Fn() + Send + Sync>, icons: Vec<ksni::Icon> }

    impl Sni {
        fn send(&self, e: TrayEvent) {
            // The UI loop may be asleep while the window is minimised: raise it from here.
            if matches!(e, TrayEvent::IconClicked | TrayEvent::OpenCaptureWindow) { crate::raise::raise_capture_window_now(); }
            let _ = self.tx.send(e);
            (self.wake)();
        }
        fn item(label: &str, e: TrayEvent) -> MenuItem<Self> {
            StandardItem { label: label.into(), activate: Box::new(move |t: &mut Self| t.send(e.clone())), ..Default::default() }.into()
        }
    }

    impl ksni::Tray for Sni {
        fn id(&self) -> String { "LinkShot".into() }
        fn title(&self) -> String { "LinkShot".into() }
        fn category(&self) -> ksni::Category { ksni::Category::ApplicationStatus }
        fn icon_pixmap(&self) -> Vec<ksni::Icon> { self.icons.clone() }
        fn tool_tip(&self) -> ksni::ToolTip { ksni::ToolTip { title: "LinkShot".into(), description: "Click: Capture window · Middle click: capture".into(), ..Default::default() } }
        fn activate(&mut self, _x: i32, _y: i32) { self.send(TrayEvent::IconClicked); }
        fn secondary_activate(&mut self, _x: i32, _y: i32) { self.send(TrayEvent::Capture); }
        fn menu(&self) -> Vec<MenuItem<Self>> {
            vec![
                Self::item("Open LinkShot Editor", TrayEvent::OpenEditor),
                Self::item("Open Capture Window", TrayEvent::OpenCaptureWindow),
                Self::item("Capture", TrayEvent::Capture),
                Self::item("LinkShot Capture Widget", TrayEvent::ToggleWidget),
                MenuItem::Separator,
                Self::item("Preferences...", TrayEvent::Preferences),
                Self::item("Help", TrayEvent::Help),
                MenuItem::Separator,
                Self::item("Exit linkshot", TrayEvent::Exit),
            ]
        }
    }

    fn argb_icon(size: u32) -> ksni::Icon {
        let rgba = icon_rgba(size);
        let mut data = Vec::with_capacity(rgba.len());
        for px in rgba.chunks_exact(4) { data.extend_from_slice(&[px[3], px[0], px[1], px[2]]); }
        ksni::Icon { width: size as i32, height: size as i32, data }
    }

    pub fn start(tx: Sender<TrayEvent>, wake: Arc<dyn Fn() + Send + Sync>) -> bool {
        let tray = Sni { tx, wake, icons: vec![argb_icon(22), argb_icon(32), argb_icon(48), argb_icon(64)] };
        match tray.spawn() {
            Ok(handle) => { std::mem::forget(handle); true }
            Err(e) => { tracing::warn!("tray icon unavailable: {e}"); false }
        }
    }
}

#[cfg(all(feature = "tray", not(target_os = "linux")))]
mod platform {
    use super::{icon_rgba, TrayEvent};
    use crossbeam_channel::Sender;
    use std::sync::Arc;

    pub fn start(tx: Sender<TrayEvent>, wake: Arc<dyn Fn() + Send + Sync>) -> bool {
        let icon = tray_icon::Icon::from_rgba(icon_rgba(32), 32, 32).expect("icon");
        std::thread::Builder::new().name("tray".into()).spawn(move || {
            use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
            let menu = Menu::new();
            let open_editor = MenuItem::new("Open LinkShot Editor", true, None);
            let open_capture = MenuItem::new("Open Capture Window", true, None);
            let capture = MenuItem::new("Capture", true, None);
            let widget = MenuItem::new("LinkShot Capture Widget", true, None);
            let prefs = MenuItem::new("Preferences...", true, None);
            let help = MenuItem::new("Help", true, None);
            let exit = MenuItem::new("Exit linkshot", true, None);
            let _ = menu.append_items(&[&open_editor, &open_capture, &capture, &widget, &PredefinedMenuItem::separator(), &prefs, &help, &PredefinedMenuItem::separator(), &exit]);
            let tray = tray_icon::TrayIconBuilder::new().with_menu(Box::new(menu)).with_tooltip("LinkShot").with_icon(icon).build();
            let _tray = match tray { Ok(t) => t, Err(e) => { tracing::warn!("tray icon failed: {e}"); return; } };
            let ids = [(open_editor.id().clone(), TrayEvent::OpenEditor), (open_capture.id().clone(), TrayEvent::OpenCaptureWindow), (capture.id().clone(), TrayEvent::Capture), (widget.id().clone(), TrayEvent::ToggleWidget), (prefs.id().clone(), TrayEvent::Preferences), (help.id().clone(), TrayEvent::Help), (exit.id().clone(), TrayEvent::Exit)];
            let menu_rx = MenuEvent::receiver();
            let icon_rx = tray_icon::TrayIconEvent::receiver();
            loop {
                let mut any = false;
                while let Ok(ev) = menu_rx.try_recv() { if let Some((_, e)) = ids.iter().find(|(id, _)| *id == ev.id) { let _ = tx.send(e.clone()); any = true; } }
                while let Ok(ev) = icon_rx.try_recv() {
                    match ev {
                        tray_icon::TrayIconEvent::Click { button: tray_icon::MouseButton::Left, button_state: tray_icon::MouseButtonState::Up, .. } => { let _ = tx.send(TrayEvent::IconClicked); any = true; }
                        tray_icon::TrayIconEvent::Click { button: tray_icon::MouseButton::Middle, button_state: tray_icon::MouseButtonState::Up, .. } => { let _ = tx.send(TrayEvent::Capture); any = true; }
                        _ => {}
                    }
                }
                if any { wake(); }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }).ok();
        true
    }
}
