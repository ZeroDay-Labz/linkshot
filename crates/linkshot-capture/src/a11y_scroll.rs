//! Automatic scrolling through the accessibility bus (AT-SPI): instead of sending wheel events to
//! whatever is under the pointer, find the scroll bar of the view being captured and set its value.
//! Steps are then exact and known in advance, nothing depends on scroll acceleration or pointer
//! position, and the end of the content is the scroll bar's maximum.
//!
//! Works with toolkits that expose scroll bars with the Value interface (Qt/KDE apps, GTK apps).
//! Browsers and Electron apps do not expose their page scroll bars; the caller falls back to wheel
//! events for those. KDE only for now: the window under the region (and its process) comes from KWin.

use anyhow::{bail, Context, Result};
use std::time::Duration;
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::OwnedObjectPath;

const ACCESSIBLE: &str = "org.a11y.atspi.Accessible";
const COMPONENT: &str = "org.a11y.atspi.Component";
const VALUE: &str = "org.a11y.atspi.Value";
/// AT-SPI role numbers (AtspiRole).
const ROLE_SCROLL_BAR: u32 = 48;
const ROLE_FRAME: u32 = 23;
const ROLE_WINDOW: u32 = 69;
const ROLE_DIALOG: u32 = 16;
/// AtspiStateType bits.
const STATE_ACTIVE: u32 = 1;
const STATE_SHOWING: u32 = 25;
const STATE_VERTICAL: u32 = 29;
const STATE_HORIZONTAL: u32 = 12;
const STATE_VISIBLE: u32 = 30;
/// Coordinates relative to the window (`ATSPI_COORD_TYPE_WINDOW`): the only kind Wayland clients know.
const COORD_WINDOW: u32 = 1;

type Node = (String, OwnedObjectPath);

pub struct A11yScroller {
    conn: Connection,
    bar: Node,
    min: f64,
    max: f64,
    /// A first guess for one step, in the bar's units: the bar's length on screen, which is the
    /// visible length when the units are pixels. The scrolling loop calibrates it from what moved.
    pub initial_step: f64,
}

/// What the scroll bar is positioned at, and where it ends.
#[derive(Clone, Copy, Debug)]
pub struct Position { pub value: f64, pub max: f64 }

impl A11yScroller {
    /// Finds the scroll bar of the scrollable view under `center` (logical desktop coordinates) in
    /// the window there.
    pub fn find(center: (f64, f64), horizontal: bool) -> Result<A11yScroller> {
        if !crate::kwin::is_kde() { bail!("accessibility scrolling needs KWin to find the window"); }
        let win = crate::kwin::window_under(center.0, center.1)?;
        let conn = a11y_bus()?;
        let app = app_for_pid(&conn, win.pid).with_context(|| format!("{} (pid {}) is not on the accessibility bus", win.caption, win.pid))?;
        // The window's accessible: the frame with the window's title, preferring the active one.
        let frames: Vec<Node> = children(&conn, &app)?.into_iter().filter(|n| matches!(role(&conn, n), Ok(ROLE_FRAME | ROLE_WINDOW | ROLE_DIALOG))).collect();
        let frame = frames.iter().find(|n| name(&conn, n).map(|t| !t.is_empty() && win.caption.contains(&t)).unwrap_or(false))
            .or_else(|| frames.iter().find(|n| states(&conn, n).map(|s| has(s, STATE_ACTIVE)).unwrap_or(false)))
            .or(frames.first()).cloned().context("window not found in its accessibility tree")?;
        // The point in window coordinates. KWin reports the client area (without decorations), which
        // is what toolkits measure from.
        let (px, py) = ((center.0 - win.x) as i32, (center.1 - win.y) as i32);
        let mut bars = Vec::new();
        collect_scroll_bars(&conn, &frame, horizontal, &mut vec![frame.clone()], &mut 0, &mut bars);
        tracing::debug!("a11y: {} scroll bars in {:?}", bars.len(), win.caption);
        // The bar that belongs to the view under the point: the nearest ancestor of the bar that
        // contains the point is its scroll area (toolkits nest the bar in a container or two); the
        // innermost such area wins.
        let mut best: Option<(i64, Node)> = None;
        for (bar, ancestors) in bars {
            for node in ancestors.iter().rev().take(4) {
                let Ok((x, y, w, h)) = extents(&conn, node) else { continue };
                if w <= 0 || h <= 0 || px < x || py < y || px >= x + w || py >= y + h { continue; }
                let area = w as i64 * h as i64;
                if best.as_ref().map(|b| area < b.0).unwrap_or(true) { best = Some((area, bar.clone())); }
                break;
            }
        }
        let (_, bar) = best.context("no scroll bar for the view under the region")?;
        let (min, max): (f64, f64) = {
            let p = proxy(&conn, bar.0.as_str(), bar.1.as_str(), VALUE)?;
            (p.get_property("MinimumValue")?, p.get_property("MaximumValue")?)
        };
        if max <= min { bail!("the view is not scrollable"); }
        // Units differ between toolkits and views (pixels, lines, percent); start small, relative to
        // both the bar's length and its range, and let the caller calibrate.
        let (_, _, bw, bh) = extents(&conn, &bar)?;
        let visible = if horizontal { bw } else { bh }.max(40) as f64;
        let initial_step = (visible * 0.3).min((max - min) * 0.1).max(1.0);
        tracing::info!("accessibility scrolling: {:?}, range {min}..{max}", win.caption);
        Ok(A11yScroller { conn, bar, min, max, initial_step })
    }

    pub fn position(&self) -> Result<Position> {
        let p = proxy(&self.conn, self.bar.0.as_str(), self.bar.1.as_str(), VALUE)?;
        Ok(Position { value: p.get_property("CurrentValue")?, max: self.max })
    }

    /// Scrolls to the start.
    pub fn to_start(&self) -> Result<()> { self.set(self.min) }

    /// Moves the bar by `delta` units (negative = back). Returns how far it actually moved (0 at the
    /// end in that direction).
    pub fn scroll_by(&self, delta: f64) -> Result<f64> {
        let cur = self.position()?.value;
        let target = (cur + delta).clamp(self.min, self.max);
        if (target - cur).abs() < 0.01 { return Ok(0.0); }
        self.set(target)?;
        Ok(target - cur)
    }

    pub fn range(&self) -> f64 { self.max - self.min }

    pub fn at_end(&self) -> bool { self.position().map(|p| p.value >= p.max - 0.5).unwrap_or(false) }

    fn set(&self, v: f64) -> Result<()> {
        let p = proxy(&self.conn, self.bar.0.as_str(), self.bar.1.as_str(), VALUE)?;
        p.set_property("CurrentValue", v).map_err(|e| anyhow::anyhow!("set scroll value: {e}"))?;
        Ok(())
    }
}

/// Connects to the accessibility bus, switching accessibility on first: browsers and some toolkits
/// only build their trees once `org.a11y.Status.IsEnabled` is set.
fn a11y_bus() -> Result<Connection> {
    let session = Connection::session().context("session bus")?;
    let status = Proxy::new(&session, "org.a11y.Bus", "/org/a11y/bus", "org.a11y.Status")?;
    if !status.get_property::<bool>("IsEnabled").unwrap_or(false) {
        let _ = status.set_property("IsEnabled", true);
        // Give applications a moment to register.
        std::thread::sleep(Duration::from_millis(300));
    }
    let bus = Proxy::new(&session, "org.a11y.Bus", "/org/a11y/bus", "org.a11y.Bus")?;
    let addr: String = bus.call("GetAddress", &()).context("accessibility bus address")?;
    let conn = zbus::blocking::connection::Builder::address(addr.as_str())?.method_timeout(Duration::from_millis(1500)).build().context("accessibility bus")?;
    Ok(conn)
}

fn app_for_pid(conn: &Connection, pid: u32) -> Result<Node> {
    let root: Node = ("org.a11y.atspi.Registry".into(), OwnedObjectPath::try_from("/org/a11y/atspi/accessible/root")?);
    let dbus = zbus::blocking::fdo::DBusProxy::new(conn)?;
    for app in children(conn, &root)? {
        let Ok(name) = zbus::names::BusName::try_from(app.0.as_str()) else { continue };
        if dbus.get_connection_unix_process_id(name).ok() == Some(pid) { return Ok(app); }
    }
    bail!("no accessible application with that process")
}

fn children(conn: &Connection, n: &Node) -> Result<Vec<Node>> {
    let p = proxy(conn, n.0.as_str(), n.1.as_str(), ACCESSIBLE)?;
    let kids: Vec<(String, OwnedObjectPath)> = p.call("GetChildren", &())?;
    Ok(kids)
}

/// A proxy that reads properties on demand: Qt answers `Properties.Get` but not `GetAll` for some
/// AT-SPI interfaces, and zbus would otherwise prefetch with `GetAll`.
fn proxy<'a>(conn: &Connection, dest: &'a str, path: &'a str, iface: &'a str) -> zbus::Result<Proxy<'a>> {
    zbus::blocking::proxy::Builder::new(conn).destination(dest)?.path(path)?.interface(iface)?.cache_properties(zbus::proxy::CacheProperties::No).build()
}

fn interfaces(conn: &Connection, n: &Node) -> Vec<String> {
    proxy(conn, n.0.as_str(), n.1.as_str(), ACCESSIBLE).ok().and_then(|p| p.call("GetInterfaces", &()).ok()).unwrap_or_default()
}

fn role(conn: &Connection, n: &Node) -> Result<u32> {
    let p = proxy(conn, n.0.as_str(), n.1.as_str(), ACCESSIBLE)?;
    Ok(p.call("GetRole", &())?)
}

fn name(conn: &Connection, n: &Node) -> Result<String> {
    let p = proxy(conn, n.0.as_str(), n.1.as_str(), ACCESSIBLE)?;
    Ok(p.get_property("Name")?)
}

fn states(conn: &Connection, n: &Node) -> Result<Vec<u32>> {
    let p = proxy(conn, n.0.as_str(), n.1.as_str(), ACCESSIBLE)?;
    Ok(p.call("GetState", &())?)
}

fn has(states: Vec<u32>, bit: u32) -> bool { states.get((bit / 32) as usize).map(|w| w & (1 << (bit % 32)) != 0).unwrap_or(false) }

fn extents(conn: &Connection, n: &Node) -> Result<(i32, i32, i32, i32)> {
    let p = proxy(conn, n.0.as_str(), n.1.as_str(), COMPONENT)?;
    Ok(p.call("GetExtents", &(COORD_WINDOW,))?)
}


/// Showing scroll bars of the wanted orientation below the last node of `path`, each with its
/// ancestors (root first). Large containers (list and table rows, document paragraphs) are skipped:
/// scroll bars sit next to the view, not inside its items.
fn collect_scroll_bars(conn: &Connection, n: &Node, horizontal: bool, path: &mut Vec<Node>, visited: &mut usize, out: &mut Vec<(Node, Vec<Node>)>) {
    if path.len() > 24 || *visited > 4000 { return; }
    *visited += 1;
    let Ok(kids) = children(conn, n) else { return };
    if kids.len() > 300 { return; }
    for k in kids {
        let Ok(st) = states(conn, &k) else { continue };
        if !has(st.clone(), STATE_SHOWING) || !has(st.clone(), STATE_VISIBLE) { continue; }
        if role(conn, &k).ok() == Some(ROLE_SCROLL_BAR) {
            let orient = if horizontal { STATE_HORIZONTAL } else { STATE_VERTICAL };
            // Toolkits that set neither orientation state: judge by shape.
            let ok = has(st.clone(), orient) || (!has(st.clone(), STATE_VERTICAL) && !has(st, STATE_HORIZONTAL) && extents(conn, &k).map(|(_, _, w, h)| (w > h) == horizontal).unwrap_or(false));
            // Only bars that can be set (some custom bars, like an editor's minimap, cannot).
            if ok && interfaces(conn, &k).iter().any(|i| i == VALUE) { out.push((k, path.clone())); }
            continue;
        }
        path.push(k.clone());
        collect_scroll_bars(conn, &k, horizontal, path, visited, out);
        path.pop();
    }
}

/// Prints the accessibility tree of the window under `center` (diagnostics for `examples/a11yscroll`).
pub fn dump_tree(center: (f64, f64), max_depth: usize) -> Result<()> {
    let win = crate::kwin::window_under(center.0, center.1)?;
    let conn = a11y_bus()?;
    let app = app_for_pid(&conn, win.pid)?;
    fn walk(conn: &Connection, n: &Node, d: usize, max: usize) {
        let st = states(conn, n).unwrap_or_default();
        println!("{}{} {:?} role={} showing={} ext={:?}", "  ".repeat(d), n.1.as_str(), name(conn, n).unwrap_or_default(), role(conn, n).unwrap_or(0), has(st, STATE_SHOWING), extents(conn, n).ok());
        let kids = children(conn, n).unwrap_or_default();
        if d >= max || kids.len() > 60 { return; }
        for k in kids { walk(conn, &k, d + 1, max); }
    }
    walk(&conn, &app, 0, max_depth);
    Ok(())
}
