# linkshot — Architecture & Roadmap

LinkShot is a screen capture, recording and editing application written in Rust that runs on Linux (X11 and
Wayland) and Windows. Its native capture format is `.snagx` (a zip of PNG background plus JSON objects), and it
also reads and writes `.snagtheme`, `.snagpalette`, `.snagt` and `.snagpresets` files.

## Guiding rules

1. **Behaviour parity first, pixel parity second.** Every user-visible behaviour in the study has a home here.
2. **Own assets.** Icons, stamps, templates and themes are drawn/authored by us (SVG), never copied.
3. **One document model, two renderers.** The vector document (`linkshot-core`) is rendered by `tiny-skia`
   for export/thumbnails *and* for the on-screen canvas (rasterised at the current zoom, cached), so what you
   see is exactly what you save. UI chrome is `egui`.
4. **Platform code behind traits.** Capture, hotkeys, tray, clipboard, window enumeration are trait objects
   with X11 / Wayland (portal) / Windows back-ends.
5. **The file format is tested with real files.** A library of 1 777 `.snagx` files is the
   round-trip corpus (read → write → read must be lossless for known keys; unknown keys are preserved).

## Workspace layout

```
linkshot/
  Cargo.toml                 (workspace)
  crates/
    linkshot-core/           document model, enums (serialised spellings), geometry, colour, RTF text,
                             snagx/snagt/snagtheme/snagpalette IO, presets (JSON + .snagpresets XML),
                             settings, undo stack, auto-naming, library index (SQLite)
    linkshot-render/         tiny-skia rasteriser: objects, 25 callout shapes, arrow caps, dashes, shadows,
                             blur/pixelate, magnify, text (cosmic-text), effects (border, edges, shadow,
                             caption, filters, colour adjust/replace, perspective, page curl, watermark,
                             trim, scale), thumbnails
    linkshot-capture/        monitors, windows (enumerate/highlight/geometry), screen/region/window grab,
                             cursor capture, clipboard images, timed capture, scrolling capture (`stitch.rs`,
                             `scrolling.rs`, `scroll_input.rs`),
                             video recorder (later)
    linkshot-platform/       global hotkeys, tray icon, autostart, open-with/program destinations,
                             paths (XDG / %LOCALAPPDATA%), printing
    linkshot-app/            eframe application: Capture window, selection overlay + HUDs, Editor
                             (top panel, toolbar, canvas, side panel, effects, footer, tray, library),
                             preferences, share, dialogs; multi-viewport
  assets/                    our SVG icons, default theme/quick-style definitions, built-in templates
  docs/                      this file
```

## Technology choices

| Need | Crate | Why |
| --- | --- | --- |
| GUI | `eframe`/`egui` (wgpu) | immediate-mode, trivial custom styling, multi-viewport (capture window, editor, overlays), works on X11/Wayland/Windows |
| Raster/vector drawing | `tiny-skia` | fast CPU rasteriser, paths, strokes, dashes, gradients, blur-able masks; identical output everywhere |
| Text layout/raster | `cosmic-text` | cross-platform shaping + system fonts; RTF subset ⇄ attributed spans |
| Screen capture | `xcap`; KWin `org.kde.KWin.ScreenShot2` (zbus) + XDG Screenshot portal (`ashpd`) on Wayland; `wayland-client` xdg-output for monitor geometry | xcap covers X11/Windows; on Wayland xcap blocks, so the D-Bus paths are used |
| Global hotkeys | `global-hotkey`; `ashpd` GlobalShortcuts portal on Wayland | X11/Windows registration vs. portal-confirmed bindings |
| Headless testing | own software rasteriser for egui meshes (`headless.rs`) | `--shots` screenshots and `--selftest` flow test without a display |
| Tray | `ksni` (Linux, native StatusNotifierItem + dbusmenu); `tray-icon` elsewhere | appindicator-based trays never report clicks on Linux, `ksni` delivers `Activate` (left click) and `SecondaryActivate` (middle click) |
| Clipboard | `arboard` | image + text on all platforms |
| Files | `zip`, `serde_json` (preserve_order), `quick-xml` (serde), `image`, `gif`, `printpdf` | `.snagx` zip, JSON schema, presets XML, codecs |
| Library index | `rusqlite` (bundled) | tags, dates, app/website sources, search — replaces Windows Search |
| Dialogs | `rfd` | native file dialogs |
| Misc | `uuid`, `chrono`, `directories`, `anyhow`, `thiserror`, `tracing`, `rayon` | |

| Video | `linkshot-video`: `lz4_flex` spool, `gif` + `color_quant` encoder, external `ffmpeg` for MP4 | see "Video recording" below |

Deferred/optional (feature flags): OCR (`tesseract`/`ocrs`), Smart Move / AutoFill inpainting, a virtual printer
(CUPS backend / Windows port monitor), cloud shares, audio/webcam in recordings.

## Video recording

Recording reuses the screenshot back-end instead of a screencast stream: `capture::video::Recorder` grabs
the region with `capture_region_with_cursor` (KWin `CaptureArea` with `include-cursor`) at a steady
10/20/30 fps on one thread and hands frames to a spool thread (bounded channel; frames are dropped, not
delayed, if the disk cannot keep up). The spool (`linkshot-video/src/spool.rs`, `.lsv`) is lossless:
each frame is either a key frame or the XOR delta against the previous frame, LZ4 compressed, with a
key frame every 90 frames and an index + footer appended on finish (an unfinished spool is still
readable by scanning). Everything downstream — the Editor player (`video_view.rs`, a decoder thread
driven by `linkshot_video::Player`), the GIF encoder and the MP4 exporter — reads frames through
`FrameSource`. Opened GIF/MP4 files are decoded into a spool in the cache (`image` for GIF, `ffmpeg`
for the rest) so the same code paths apply; a recording's master is kept as
`recordings/<library stem>.lsv` so later exports stay lossless.

GIF export (`gif.rs`): one global 255-colour palette (exact when the clip has ≤ 255 colours, else
NeuQuant), index 255 reserved for "unchanged" transparency; the clip is resampled to the chosen frame
rate, identical frames extend the previous delay, and only the changed rectangle of each frame is
written with disposal *Keep*. Optional Floyd–Steinberg dithering through a 6-bit lookup table. MP4
export pipes raw RGBA at a constant frame rate into `ffmpeg` (`libx264` CRF 18 `-tune animation`, else
`libopenh264`, else `mpeg4`; odd sizes are padded, never cropped). The recording UI
(`recording_hud.rs`) is a toolbar viewport outside the region plus four 3-px frame-strip viewports
around it (never over it, so they cannot leak into frames), placed by KWin on Plasma Wayland
(`kwin::setup_overlay_window`). Audio and webcam are not recorded yet (ffmpeg could mux a WAV later).

## Mapping concepts → modules

| Concept | Module |
| --- | --- |
| `CaptureSettings` / Presets6.xml | `core::presets::{CaptureSettings, PresetStore}` (JSON on disk, `.snagpresets` import/export) |
| `SelectionMode`, `CaptureMode`, timing | `capture::{Selection, CaptureJob, Timing}` + `app::capture_flow` state machine |
| Crosshair overlay / HUDs | `app::overlay` (fullscreen transparent viewport per monitor) + `app::hud::{AllInOne, Panoramic, Video}` |
| `.snagx` page JSON | `core::document::{Document, CaptureObject, ObjectKind}` with `serde` using the stored key names and `#[serde(flatten)] extra: Map` for forward compatibility |
| `CPaintOp*` undo stack | `core::undo::{Command, UndoStack}` (every edit is a reversible command) |
| Tools2.xml / QuickStyle | `core::style::{ToolProperties, QuickStyle, Theme, Palette}` |
| Effects | `render::effects::*` (pure functions `fn(&Pixmap, &EffectSettings) -> Pixmap`) |
| Tray.bin / thumbnails | `core::tray::TrayState` (JSON) + `render::thumbnail` |
| Library (Windows Search) | `core::library::{Scanner, Index}` (SQLite, watches library folder) |
| Share destinations | `app::share::{Destination trait: File, Clipboard, Email(mailto), Program, Print, Ftp}`; cloud via plugins later |
| Registry preferences | `core::settings::Settings` (JSON, same option names as the study) |

## Scrolling capture and Grab Text

**Scrolling capture** (`linkshot-capture::scrolling` + `stitch`): after the region is chosen the overlay
closes and a worker thread grabs the live region at native resolution (KWin `CaptureArea` on Plasma
Wayland, ~20 ms per frame; monitor capture + crop elsewhere). Each frame goes into `Stitcher::push`:
frames are converted to grey, rows that are identical to the previous frame at the top/bottom are
every row is hashed (central 2–94 % of the width, so scrollbars don't vote) and rows vote for an
integer offset, weighted by how rare their row class is (blank rows and repeated patterns such as
identical code-block headers weigh nothing). Every candidate offset is then verified over its *whole*
overlap: nearly all informative rows of the new frame must be found in the previous frame at that
offset (a tooltip, caret or hover highlight may change a few rows; a wrong offset changes most). The
candidate with the most matching rows wins; near-ties on periodic content go to the offset closest to
the previous frame's. An overlap without informative rows (blank space) is accepted only when every
row of a ≥ 40 % overlap is identical. A frame that matches nothing is dropped and reported
(`Stitcher::misses`, the HUD shows "Lost the overlap") — scrolling back a little resynchronises;
nothing is ever placed by guesswork. Sticky bands (header/footer/window chrome) are learned only from
frames that actually scrolled (rows identical despite the scroll) and shrink as evidence accumulates.
Elements pinned to the viewport in the middle of the frame (chat UIs pin a code block's header
while the block scrolls by, floating toolbars) are detected per frame pair as runs of ≥ 8 textured
rows that stayed identical at the same screen position although the content moved (`pinned_rows`);
`finish()` never lets a pinned row overwrite real content and always lets real content replace a
pinned row, so such a header appears once, at its natural position. `finish()` writes the body from
the measured positions, the header from the first frame and the footer from the last — no resampling
or blending, so the output is pixel-exact (verified in `--selftest` and the stitcher's unit tests,
which include a dark chat page with repeated code headers, 70 % jumps and pinned headers).
The first frame is taken only after the screen has settled (the compositor is still fading the
overlay out when the session starts).
The floating HUD (`scrolling_hud.rs`, a small always-on-top viewport placed outside the region; on
Plasma Wayland `kwin::place_window` moves it there since Wayland ignores client positions) offers
Start (when "Require Start button" is on), Auto-scroll, Done and Cancel; Enter/Esc work too. Only the
status area drags the window, and only once the pointer really moves: egui starts a drag on the first
press of a drag-only widget, which used to turn the first click on *Done* into a window move.
*Automatic Scrolling* injects wheel events (`scroll_input.rs`: RemoteDesktop portal on Wayland with a
persisted restore token, `enigo` on X11/Windows), scrolls to the top first, then steps until two
consecutive frames stop changing, and finishes on its own.

**Grab Text** (`linkshot-ocr`): Tesseract CLI when installed (TSV output grouped into lines), otherwise
the bundled pure-Rust `ocrs` models (downloaded once into `<data>/models`, ~12 MB). The image is
upscaled adaptively so the median detected word height is ~28 px. Results open in the 600×420 "Grab Text
Results" window (`grab_text.rs`) with Copy All; the Editor's Edit ▸ Grab Text runs it on the flattened
current image.

**Quality rules**: captures are never downscaled — `Snapshot::crop` returns native pixels for the densest
monitor touched (block-copied when densities match), frames are grabbed without the cursor, and every
export of a capture is lossless PNG inside `.snagx`.

## Multi-monitor overlay and theming

The overlay (`overlay.rs`) keeps one shared state (snapshot of all monitors, phase, selection in logical
virtual-desktop coordinates) and the app shows one fullscreen viewport per monitor, each calling
`Overlay::ui(ctx, pal, monitor_idx)`. The viewport that holds the pointer draws the crosshair/magnifier;
a drag that started on one monitor keeps its implicit grab, so the selection can extend across screens
and every viewport draws its own part of it. Placement: X11/Windows position each window at its monitor;
on Plasma Wayland the windows are created fullscreen and a background thread
(`kwin::ensure_fullscreen_placement`) runs KWin scripts over `org.kde.kwin.Scripting` until KWin
reports every overlay on its output (`kwin::window_placements` gets the answer back through a
transient D-Bus name the script calls). The same mechanism raises the Capture window when the tray
icon is clicked (`kwin::activate_window`; compositors ignore focus requests from unfocused apps).
Other Wayland compositors get a single overlay on the active monitor (resolved from the viewport size).
`linkshot --capture` starts a capture immediately, which is how the placement was verified live.
Every KWin script runs through `kwin::run_probe`, which wraps it in `try/catch` and waits for a
`REPORT(...)` call back over D-Bus before unloading it: KWin never tells a client that a script failed
or that it had not run yet when it was unloaded (both bit us). `examples/kwinplace` exposes the helpers
(`state`, `move`, `fullscreen`, `activate`, `minimize`, `unminimize`, `close`, `tame`, `focusaway`).

## Window model: an invisible root, everything else a viewport

The eframe root window is a 2×2 px transparent "controller" (`keepalive.rs`, caption `linkshot`), and
the Capture window, Editor, overlays, scrolling HUD and Grab Text results are all child viewports (the
Capture window is `ViewportId::from_hash_of("capture-window")`). Reasons, all found the hard way on
Plasma Wayland:

* winit cannot hide a Wayland window (`set_visible` is a no-op), but an immediate viewport that is
  simply not shown is destroyed, so "hide during capture" and "close to tray" work by toggling
  `capture_visible`; the window is re-created (and raised through KWin) on show.
* A minimised Wayland window gets no frame callbacks. winit then withholds its redraws, and eframe
  routes *every* repaint, including child-viewport repaints, through the root window. With the Capture
  window as root, a minimised app never ran `update`: hotkeys, tray clicks, timers and second-launch
  requests were dead until the user restored it. The controller is never minimised, so the loop runs.
* Mesa's Wayland EGL blocks `eglSwapBuffers` until the previous frame callback arrives when the swap
  interval is 1, so painting a minimised child twice hung the UI thread inside `swap_buffers`. eframe
  runs with `vsync: false` (egui repaints on demand only; nothing is lost).

On Plasma Wayland a KWin script (`kwin::tame_helper_window`, retried for a few seconds after start-up)
moves the controller to the bottom-right corner of its output, sets skip-taskbar/switcher/pager and
keep-below, and hands focus to the Capture window if KWin gave it to the controller; `Keepalive::tick`
passes focus on again (`kwin::focus_away_from`) whenever the controller becomes active later. On X11
and Windows the controller is a 2×2 tool window placed off-screen.

Background threads that need `update` to run (tray, hotkeys, single-instance listener) call
`keepalive::wake`, which repaints the root explicitly (`request_repaint_of(ViewportId::ROOT)`); timers
use `keepalive::wake_after`. The tray left click and a second launch also raise the Capture window
directly from their own thread (`raise::raise_capture_window_now`, KWin activation with `minimized =
false`) so the raise does not depend on the UI loop at all.

**Single instance** (`single_instance.rs`): a Unix socket in `$XDG_RUNTIME_DIR`. A second launch sends
`show` (or `capture` for `--capture`, `ping` for `--background`) and exits, so the desktop launcher or
a taskbar button brings the running Capture window to the front instead of starting another copy.
Windows has no guard yet.

Theme (`theme.rs`): dark palette by default (`Settings.theme_generation` migrates older settings files),
light palette available from View ▸ Theme; Inter (OFL) is bundled from `assets/fonts`; egui style uses
6 px control radius, soft shadows and animated hover states. The Editor canvas renders at
`zoom × pixels_per_point` and the renderer pre-downscales backgrounds with Lanczos when zoomed out, so
nothing is resampled by the GPU.

## Rendering model

* Document space = canvas pixels at document DPI. Objects store geometry as f64 points.
* `render::scene` builds tiny-skia paths per object: shapes, rounded rects, polygons, bezier arrows with cap
  geometry (Flat, Round, RoundAnchor, SquareAnchor, DiamondAnchor, TArrow, EquilateralArrow, BarbedArrow,
  LineArrow, RoundedArrow), 25 callout outlines with tails, step badges, highlight (multiply), blur/pixelate
  (sampled from the flattened background), magnify (scaled copy with outline), stamps (SVG via `resvg`),
  pasted images, cursor. Shadows: offset + gaussian blur of the alpha mask with `ShadowColor`/`ShadowOpacity`.
* Text: RTF (`RTFEncodedText`) ⇄ `TextRuns` (font, size, b/i/u/strike, colour, alignment) → cosmic-text layout.
* Canvas view = background bitmap + objects rasterised at zoom × pixels-per-point into an egui texture;
  handles drawn by egui. Only a tile (visible area plus half a view of margin) is rendered
  (`RenderOptions::clip`), re-rendered when the view leaves it or the document changes, so a 20,000 px
  scrolling capture pans and edits as quickly as a small one. On-screen renders use `preview` (bilinear
  background enlargement); large reductions box-decimate first (`linkshot_render::decimate`), and
  tray thumbnails are rendered directly at thumbnail scale once edits pause for 400 ms.
  `cargo run --release -p linkshot-render --example bench_tile` times these paths.
* Export = render at 100 % then encode; `.snagx` keeps background PNG + objects, other formats flatten.

## Roadmap

**Phase 1 — foundation (done 2026-10-18, see README "Status")**: workspace; core model + `.snagx` read/write round-trip; enums; settings;
presets model; capture of region/window/fullscreen (X11/Wayland/Windows) with cursor; Capture window UI
(tabs, options, Capture button, presets drawer); selection overlay (orange crosshair, magnifier, dimensions,
window highlight, aspect locks, keys); All-in-One HUD; Editor shell (caption/menu, top panel, toolbar, canvas
with zoom, side panel, footer, tray); Move/Arrow/Shape/Text/Highlight/Step basic tools; save PNG/JPG/snagx;
Copy All; tray icon; global hotkey; auto-save to library.

**Phase 2 — editor parity**: all tools with full property panels; quick styles, themes (import/export
`.snagtheme`), favourites; effects + quick effects; crop/cut-out/eraser/fill/magic wand/selection with
transparent fill; canvas resize/corner radius/snapping; context menus; undo all; Details; tags; library view
with search/filters; preferences dialog; keyboard shortcuts; light/dark theme.

**Phase 3 — capture parity**: timed/interval/scheduled capture; menu/object/multiple-area/freehand/clipboard/file
modes; scrolling capture (manual + automatic — **done**); presets hotkeys/import/export; capture-time effects and share;
Capture widget; printer capture (CUPS/Windows); Grab Text (OCR — **done**); batch convert; print / print preview; templates
(`.snagt`), combine images, video-from-images.

**Phase 4 — video & cloud**: screen recording with cursor, pause/resume, countdown, GIF (default) and MP4
export, player with in/out range — **done**; still open: webcam PiP, mic/system audio, screen draw, cursor
effects, cut out / combine, video from images; share link/cloud destinations as plugins; Smart Move,
AutoFill, Smart Redact.
