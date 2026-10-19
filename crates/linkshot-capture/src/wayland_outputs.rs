//! Monitor enumeration on Wayland via `wl_output` + `zxdg_output_v1` (no screenshot permission needed).

use anyhow::{Context, Result};
use wayland_client::protocol::{wl_output, wl_registry};
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_protocols::xdg::xdg_output::zv1::client::{zxdg_output_manager_v1, zxdg_output_v1};

#[derive(Default, Debug, Clone)]
pub struct Output {
    pub name: String,
    pub description: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub phys_w: i32,
    pub phys_h: i32,
    pub scale: i32,
    pub done: bool,
}

#[derive(Default)]
struct State {
    outputs: Vec<(wl_output::WlOutput, Output)>,
    manager: Option<zxdg_output_manager_v1::ZxdgOutputManagerV1>,
    xdg_outputs: Vec<(zxdg_output_v1::ZxdgOutputV1, usize)>,
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(state: &mut Self, registry: &wl_registry::WlRegistry, event: wl_registry::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
        if let wl_registry::Event::Global { name, interface, version } = event {
            if interface == "wl_output" {
                let idx = state.outputs.len();
                let out = registry.bind::<wl_output::WlOutput, _, _>(name, version.min(4), qh, idx);
                state.outputs.push((out, Output { scale: 1, ..Default::default() }));
            } else if interface == "zxdg_output_manager_v1" {
                state.manager = Some(registry.bind::<zxdg_output_manager_v1::ZxdgOutputManagerV1, _, _>(name, version.min(3), qh, ()));
            }
        }
    }
}

impl Dispatch<wl_output::WlOutput, usize> for State {
    fn event(state: &mut Self, _: &wl_output::WlOutput, event: wl_output::Event, idx: &usize, _: &Connection, _: &QueueHandle<Self>) {
        let Some((_, o)) = state.outputs.get_mut(*idx) else { return };
        match event {
            wl_output::Event::Geometry { x, y, .. } => { if o.width == 0 { o.x = x; o.y = y; } }
            wl_output::Event::Mode { flags, width, height, .. } => {
                if flags.into_result().map(|f| f.contains(wl_output::Mode::Current)).unwrap_or(true) { o.phys_w = width; o.phys_h = height; }
            }
            wl_output::Event::Scale { factor } => o.scale = factor,
            wl_output::Event::Name { name } => o.name = name,
            wl_output::Event::Description { description } => o.description = description,
            wl_output::Event::Done => o.done = true,
            _ => {}
        }
    }
}

impl Dispatch<zxdg_output_manager_v1::ZxdgOutputManagerV1, ()> for State {
    fn event(_: &mut Self, _: &zxdg_output_manager_v1::ZxdgOutputManagerV1, _: zxdg_output_manager_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<zxdg_output_v1::ZxdgOutputV1, usize> for State {
    fn event(state: &mut Self, _: &zxdg_output_v1::ZxdgOutputV1, event: zxdg_output_v1::Event, idx: &usize, _: &Connection, _: &QueueHandle<Self>) {
        let Some((_, o)) = state.outputs.get_mut(*idx) else { return };
        match event {
            zxdg_output_v1::Event::LogicalPosition { x, y } => { o.x = x; o.y = y; }
            zxdg_output_v1::Event::LogicalSize { width, height } => { o.width = width; o.height = height; }
            zxdg_output_v1::Event::Name { name } => { if o.name.is_empty() { o.name = name; } }
            zxdg_output_v1::Event::Description { description } => { if o.description.is_empty() { o.description = description; } }
            _ => {}
        }
    }
}

/// Lists outputs with logical geometry (what the compositor uses for window placement).
pub fn outputs() -> Result<Vec<Output>> {
    tracing::debug!("wayland: connecting");
    let conn = Connection::connect_to_env().context("connect to wayland display")?;
    tracing::debug!("wayland: connected");
    let display = conn.display();
    let mut queue = conn.new_event_queue::<State>();
    let qh = queue.handle();
    let _registry = display.get_registry(&qh, ());
    let mut state = State::default();
    queue.roundtrip(&mut state).context("registry roundtrip")?;
    tracing::debug!("wayland: registry done, {} outputs, manager={}", state.outputs.len(), state.manager.is_some());
    queue.roundtrip(&mut state).context("outputs roundtrip")?;
    tracing::debug!("wayland: outputs done");
    if let Some(m) = state.manager.clone() {
        for (i, (out, _)) in state.outputs.iter().enumerate() {
            let xo = m.get_xdg_output(out, &qh, i);
            state.xdg_outputs.push((xo, i));
        }
        queue.roundtrip(&mut state).context("xdg-output roundtrip")?;
    }
    let mut result: Vec<Output> = state.outputs.into_iter().map(|(_, o)| o).collect();
    for o in result.iter_mut() {
        if o.width == 0 || o.height == 0 {
            // No xdg-output: derive logical size from mode / scale.
            o.width = o.phys_w / o.scale.max(1);
            o.height = o.phys_h / o.scale.max(1);
        }
        if o.phys_w == 0 { o.phys_w = o.width * o.scale.max(1); o.phys_h = o.height * o.scale.max(1); }
    }
    result.retain(|o| o.width > 0 && o.height > 0);
    Ok(result)
}
