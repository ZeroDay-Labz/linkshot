//! Screen grabs through the XDG ScreenCast portal and PipeWire: the back-end for Wayland desktops
//! where KWin's ScreenShot2 is unavailable (GNOME, sway, and the Flatpak on KDE, where the sandbox
//! strips the KWin authorisation by design).
//!
//! One portal session per cursor mode stays open for the life of the process; the user is asked
//! once (the restore token is persisted by the app) and every monitor stream is delivered at its
//! own native size, so mixed-scale layouts need no guessing. The PipeWire streams are paused while
//! idle and woken for a grab, which costs a few frames (~50-100 ms) rather than a new connection.

use anyhow::{Context, Result};
use image::RgbaImage;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

/// Restore tokens the app persists between runs (one per cursor mode).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tokens { pub plain: Option<String>, pub cursor: Option<String> }

/// One stream of an open session: its PipeWire node and the logical monitor rectangle it shows.
#[derive(Clone, Debug)]
pub struct StreamInfo { pub node: u32, pub x: i32, pub y: i32, pub width: i32, pub height: i32 }

struct Shared { tokens: Tokens, changed: bool, sessions: HashMap<bool, Session> }

fn shared() -> &'static Mutex<Shared> {
    static S: OnceLock<Mutex<Shared>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(Shared { tokens: Tokens::default(), changed: false, sessions: HashMap::new() }))
}

/// Seeds the restore tokens from the settings (call once at start-up).
pub fn configure(tokens: Tokens) { shared().lock().unwrap().tokens = tokens; }

/// New tokens to persist, if a session was (re)created since the last call.
pub fn take_new_tokens() -> Option<Tokens> {
    let mut s = shared().lock().unwrap();
    if s.changed { s.changed = false; Some(s.tokens.clone()) } else { None }
}

/// True when the ScreenCast portal is reachable (does not open a session).
pub fn available() -> bool {
    static OK: OnceLock<bool> = OnceLock::new();
    *OK.get_or_init(|| async_io::block_on(async { ashpd::desktop::screencast::Screencast::new().await.is_ok() }))
}

/// Grabs one frame of every monitor stream; frames are keyed by stream, `streams()` tells which
/// monitor each one shows. Opens the session (one portal dialog) on first use.
pub fn grab_all(include_cursor: bool) -> Result<(Vec<StreamInfo>, HashMap<u32, RgbaImage>)> {
    let (cmd, streams) = {
        let mut s = shared().lock().unwrap();
        if !s.sessions.contains_key(&include_cursor) {
            let token = if include_cursor { s.tokens.cursor.clone() } else { s.tokens.plain.clone() };
            let session = Session::open(include_cursor, token)?;
            if include_cursor { s.tokens.cursor = session.token.clone(); } else { s.tokens.plain = session.token.clone(); }
            s.changed = true;
            s.sessions.insert(include_cursor, session);
        }
        let se = &s.sessions[&include_cursor];
        (se.cmd.clone(), se.streams.clone())
    };
    let (tx, rx) = crossbeam_channel::bounded(1);
    cmd.send(Cmd::Grab { reply: tx }).map_err(|_| anyhow::anyhow!("screencast session thread is gone"))?;
    let frames = rx.recv_timeout(Duration::from_secs(4)).context("no frames from the screencast session")?;
    if frames.is_empty() {
        // The compositor stopped the streams (session revoked): drop the session so the next grab reopens it.
        shared().lock().unwrap().sessions.remove(&include_cursor);
        anyhow::bail!("the screencast session delivered no frames");
    }
    Ok((streams, frames))
}

enum Cmd { Grab { reply: crossbeam_channel::Sender<HashMap<u32, RgbaImage>> } }

struct Session { cmd: pipewire::channel::Sender<Cmd>, streams: Vec<StreamInfo>, token: Option<String> }

impl Session {
    fn open(include_cursor: bool, restore_token: Option<String>) -> Result<Session> {
        use ashpd::desktop::screencast::{CursorMode, Screencast, SelectSourcesOptions, SourceType, StartCastOptions, OpenPipeWireRemoteOptions};
        use ashpd::desktop::{CreateSessionOptions, PersistMode};
        // The portal dialog and the D-Bus work run on their own thread; the session object lives there.
        let (ready_tx, ready_rx) = crossbeam_channel::bounded::<Result<(Vec<StreamInfo>, Option<String>, std::os::fd::OwnedFd)>>(1);
        let (cmd_tx, cmd_rx) = pipewire::channel::channel::<Cmd>();
        std::thread::Builder::new().name("linkshot-screencast".into()).spawn(move || {
            let setup = async_io::block_on(async {
                let sc = Screencast::new().await?;
                let session = sc.create_session(CreateSessionOptions::default()).await?;
                let opts = SelectSourcesOptions::default()
                    .set_sources(enumflags2::BitFlags::from(SourceType::Monitor))
                    .set_multiple(true)
                    .set_cursor_mode(if include_cursor { CursorMode::Embedded } else { CursorMode::Hidden })
                    .set_persist_mode(PersistMode::ExplicitlyRevoked)
                    .set_restore_token(restore_token.as_deref());
                sc.select_sources(&session, opts).await?.response()?;
                let streams = sc.start(&session, None, StartCastOptions::default()).await?.response()?;
                let infos: Vec<StreamInfo> = streams.streams().iter().map(|st| {
                    let (x, y) = st.position().unwrap_or((0, 0));
                    let (w, h) = st.size().unwrap_or((0, 0));
                    StreamInfo { node: st.pipe_wire_node_id(), x, y, width: w, height: h }
                }).collect();
                let token = streams.restore_token().map(|t| t.to_string());
                let fd = sc.open_pipe_wire_remote(&session, OpenPipeWireRemoteOptions::default()).await?;
                Ok::<_, ashpd::Error>((infos, token, std::os::fd::OwnedFd::from(fd), session, sc))
            });
            let (infos, token, fd, _session, _sc) = match setup {
                Ok(v) => v,
                Err(e) => { let _ = ready_tx.send(Err(anyhow::anyhow!("ScreenCast portal: {e}"))); return; }
            };
            let nodes: Vec<u32> = infos.iter().map(|s| s.node).collect();
            let _ = ready_tx.send(Ok((infos, token, fd.try_clone().expect("dup fd"))));
            if let Err(e) = run_loop(fd, nodes, cmd_rx) { tracing::warn!("screencast loop: {e:#}"); }
        }).context("spawn screencast thread")?;
        let (streams, token, _fd) = ready_rx.recv_timeout(Duration::from_secs(120)).context("screencast portal did not answer")??;
        anyhow::ensure!(!streams.is_empty(), "no monitors were shared");
        Ok(Session { cmd: cmd_tx, streams, token })
    }
}

/// The PipeWire loop: one input stream per shared monitor, paused when idle.
fn run_loop(fd: std::os::fd::OwnedFd, nodes: Vec<u32>, cmd_rx: pipewire::channel::Receiver<Cmd>) -> Result<()> {
    use pipewire::spa::param::format::{FormatProperties, MediaSubtype, MediaType};
    use pipewire::spa::param::video::{VideoFormat, VideoInfoRaw};
    use pipewire::spa::param::ParamType;
    use pipewire::spa::pod::{self, serialize::PodSerializer, Pod};
    use pipewire::spa::utils::{Direction, Fraction, Rectangle, SpaTypes};
    use pipewire::stream::StreamFlags;
    use std::cell::RefCell;
    use std::rc::Rc;

    pipewire::init();
    let main_loop = pipewire::main_loop::MainLoopRc::new(None).context("pipewire main loop")?;
    let context = pipewire::context::ContextRc::new(&main_loop, None).context("pipewire context")?;
    let core = context.connect_fd_rc(fd, None).context("connect to the portal's pipewire remote")?;

    struct Pending { reply: crossbeam_channel::Sender<HashMap<u32, RgbaImage>>, frames: HashMap<u32, RgbaImage>, wanted: usize }
    let pending: Rc<RefCell<Option<Pending>>> = Rc::new(RefCell::new(None));
    let mut streams = Vec::new();
    let mut listeners = Vec::new();
    for node in nodes.iter().copied() {
        let stream = pipewire::stream::StreamRc::new(core.clone(), "LinkShot", pipewire::properties::properties! {
            *pipewire::keys::MEDIA_TYPE => "Video", *pipewire::keys::MEDIA_CATEGORY => "Capture", *pipewire::keys::MEDIA_ROLE => "Screen",
        }).context("pipewire stream")?;
        #[derive(Default)]
        struct Ud { format: VideoInfoRaw }
        let p = pending.clone();
        let ml = main_loop.clone();
        let listener = stream.add_local_listener_with_user_data(Ud::default())
            .param_changed(|_, ud, id, param| {
                let Some(param) = param else { return };
                if id != ParamType::Format.as_raw() { return; }
                let Ok((mt, ms)) = pipewire::spa::param::format_utils::parse_format(param) else { return };
                if mt != MediaType::Video || ms != MediaSubtype::Raw { return; }
                let _ = ud.format.parse(param);
            })
            .process(move |stream, ud| {
                let Some(mut buffer) = stream.dequeue_buffer() else { return };
                let mut pend = p.borrow_mut();
                let Some(pending) = pend.as_mut() else { return };
                if pending.frames.contains_key(&node) { return; }
                let size = ud.format.size();
                let datas = buffer.datas_mut();
                if datas.is_empty() { return; }
                let stride = datas[0].chunk().stride().max(0) as usize;
                let Some(data) = datas[0].data() else { return };
                let (w, h) = (size.width as usize, size.height as usize);
                if w == 0 || h == 0 { return; }
                let stride = if stride == 0 { w * 4 } else { stride };
                let mut out = vec![0u8; w * h * 4];
                for y in 0..h {
                    let Some(row) = data.get(y * stride..y * stride + w * 4) else { return };
                    out[y * w * 4..(y + 1) * w * 4].copy_from_slice(row);
                }
                match ud.format.format() {
                    VideoFormat::BGRx | VideoFormat::BGRA => { for px in out.chunks_exact_mut(4) { px.swap(0, 2); px[3] = 255; } }
                    VideoFormat::RGBx => { for px in out.chunks_exact_mut(4) { px[3] = 255; } }
                    VideoFormat::RGBA => {}
                    other => { tracing::warn!("screencast: unsupported pixel format {other:?}"); return; }
                }
                if let Some(img) = RgbaImage::from_raw(w as u32, h as u32, out) { pending.frames.insert(node, img); }
                if pending.frames.len() >= pending.wanted {
                    let done = pend.take().unwrap();
                    let _ = done.reply.send(done.frames);
                    // Pause everything again (from a timer, outside the process callback).
                    let _ = ml.loop_().add_timer(|_| {}).update_timer(Some(Duration::from_millis(1)), None);
                }
            })
            .register().context("stream listener")?;
        let obj = pod::object!(
            SpaTypes::ObjectParamFormat, ParamType::EnumFormat,
            pod::property!(FormatProperties::MediaType, Id, MediaType::Video),
            pod::property!(FormatProperties::MediaSubtype, Id, MediaSubtype::Raw),
            pod::property!(FormatProperties::VideoFormat, Choice, Enum, Id, VideoFormat::BGRx, VideoFormat::BGRA, VideoFormat::RGBx, VideoFormat::RGBA),
            pod::property!(FormatProperties::VideoSize, Choice, Range, Rectangle, Rectangle { width: 1920, height: 1080 }, Rectangle { width: 1, height: 1 }, Rectangle { width: 16384, height: 16384 }),
            pod::property!(FormatProperties::VideoFramerate, Choice, Range, Fraction, Fraction { num: 30, denom: 1 }, Fraction { num: 0, denom: 1 }, Fraction { num: 1000, denom: 1 }),
        );
        let bytes = PodSerializer::serialize(std::io::Cursor::new(Vec::new()), &pod::Value::Object(obj)).map_err(|e| anyhow::anyhow!("pod: {e:?}"))?.0.into_inner();
        let mut params = [Pod::from_bytes(&bytes).context("format pod")?];
        stream.connect(Direction::Input, Some(node), StreamFlags::AUTOCONNECT | StreamFlags::MAP_BUFFERS | StreamFlags::INACTIVE, &mut params).context("connect stream")?;
        streams.push(stream);
        listeners.push(listener);
    }

    // Commands from `grab_all`: wake the streams, collect one frame each, pause again.
    let streams_rc = Rc::new(streams);
    let (s2, p2, ml2) = (streams_rc.clone(), pending.clone(), main_loop.clone());
    let _attached = cmd_rx.attach(main_loop.loop_(), move |cmd| {
        match cmd {
            Cmd::Grab { reply } => {
                *p2.borrow_mut() = Some(Pending { reply, frames: HashMap::new(), wanted: s2.len() });
                for s in s2.iter() { let _ = s.set_active(true); }
                // Deadline: answer with whatever arrived.
                let p3 = p2.clone();
                let s3 = s2.clone();
                let timer = ml2.loop_().add_timer(move |_| {
                    if let Some(done) = p3.borrow_mut().take() { let _ = done.reply.send(done.frames); }
                    for s in s3.iter() { let _ = s.set_active(false); }
                });
                timer.update_timer(Some(Duration::from_millis(2500)), None).into_result().ok();
                std::mem::forget(timer);
            }
        }
    });
    // Pause streams once a grab completed (checked 20× a second; cheap).
    let (s4, p4) = (streams_rc.clone(), pending.clone());
    let idle = main_loop.loop_().add_timer(move |_| { if p4.borrow().is_none() { for s in s4.iter() { let _ = s.set_active(false); } } });
    idle.update_timer(Some(Duration::from_millis(50)), Some(Duration::from_millis(50))).into_result().ok();
    main_loop.run();
    Ok(())
}
