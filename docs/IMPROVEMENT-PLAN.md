# Improvement plan

A prioritised list from a full read-through of the code on 2026-10-18, followed by two UI/UX audits
(capture side and Editor) against the intended behaviour. Items marked **done** were fixed in 0.2.0/0.2.1.

## Critical

1. **Panic on non-ASCII names** — `core/src/naming.rs` `out.truncate(120)` panics when byte 120 falls
   inside a multi-byte character (long Japanese/emoji preset names, file stems). Truncate on a char
   boundary. **done**
2. **Silent settings/preset wipe** — a parse error in `settings.json`/`presets.json` becomes `Default`
   and the 2 s autosave overwrites the file; saves are not atomic. Keep the broken file as
   `*.broken-<timestamp>`, write via temp file + rename. **done**
3. **Corrupt `.lsv` crashes or OOMs** — `video/src/spool.rs` computes `width*height*4` in `u32` and
   trusts the header; a damaged spool overflows or allocates gigabytes. Compute in `u64`, bound
   the frame size, use `saturating_sub` for timestamps. **done**
4. **Screen content in world-readable `/tmp`** — the scrolling stitcher dumped rejected frames and the
   stitched page to `/tmp/linkshot-scroll-debug` whenever a frame missed; OCR wrote its input PNG to
   `/tmp`. Dump only with `LINKSHOT_SCROLL_DEBUG`, keep temporaries in the cache dir and delete them.
   **done**
5. **KWin script directory** — scripts KWin executes were written to a predictable `/tmp/linkshot-kwin-<pid>`
   directory; use `$XDG_RUNTIME_DIR` (0700). Same for the single-instance socket fallback. **done**
6. **Memory blow-up on large captures** — every edit clones the whole `Document` including PNG blobs,
   drags clone again, `commit` byte-compares blobs, and the undo stack keeps before+after for 200
   steps. Store blobs as `Arc<Vec<u8>>` so clones are refcounts and compares are pointer checks.

## Important

7. **Blocking work on the UI thread** — `snapshot()` runs inline for timed/interval captures (a portal
   round-trip on GNOME); the library scan reads every `.snagx` fully on a toolbar click. Move both
   to worker threads; have the scanner read only the metadata and page JSON.
8. **Non-KDE Wayland scrolling/video** — region frames fall back to a full portal screenshot per frame,
   so the 40 ms scrolling loop and the 30 fps recorder hammer the portal. Add a ScreenCast/PipeWire
   frame source (the `screencast` feature is already enabled but unused) or disable those modes
   there with a clear message. The recording toolbar is only kept out of the region on KDE.
9. **Desktop file** — the self-written `~/.local/share/applications/linkshot.desktop` used
   `Icon=camera-photo` and could shadow the packaged one. It now carries the capture action and
   the packaged icon name when installed. **partly done**
10. **GlobalShortcuts portal sessions** — every `apply` spawns a new thread and portal session; old
    ones linger until their next activation. Keep the session handle and close it; use the preset
    UUID alone as shortcut id so deleting a preset does not rebind the others.
11. **Disk growth** — recording masters are never pruned, tray *Delete* leaves the master behind,
    `cache/video` is unbounded, Program/Email temp exports land inside the library. Delete masters
    with their files, cap the caches, write temps to the cache dir.
12. **Startup decodes every tray video** — one ffmpeg decode per video in the tray even if never
    viewed. Decode lazily when the item becomes current.
13. **Single export slot** — starting a second export drops the receiver of the running one; the file
    is written but the tray is not updated. Queue exports or refuse while busy.
14. **Windows gaps** — no single-instance guard, cursor not drawn on still captures, window capture
    unavailable on Wayland (KWin `CaptureActiveWindow` exists but is unused). `run_program` re-splits
    the substituted command line, so a file name containing `"` breaks the Program destination.
    (Autostart via the Run key: **done**.)
15. **Swallowed errors** — tray.json serialise errors, Export Presets write errors, bad preset JSON
    ("Imported 0 preset(s)"), unparseable schedule time (captures immediately). Route through the
    status line and `tracing::warn!`.

## Nice-to-have

16. **Documentation drift** — `docs/ARCHITECTURE.md` mentions a SQLite library, `app::hud::*`,
    `core::library::{Scanner,Index}` and a `share::Destination` trait that do not exist.
17. **Structure** — `editor/mod.rs` (1 170 lines) mixes state, menus, toolbar, footer, library view,
    dialogs and preferences; split into `menu.rs`, `preferences.rs`, `library_view.rs`. Pull the
    overlay block out of `App::update`.
18. **Dead code** — `generation` in the portal shortcuts, no-op loops in `presets.rs`/`library.rs`,
    identical if/else branches in the share combo, an `egui::Context::default()` built only to
    satisfy an unused parameter, "Support" pointing at `https://github.com/`.
19. **Tests** — none in `linkshot-app`, `linkshot-render`, `linkshot-ocr`, `linkshot-platform`.
    Cheap targets: `naming::sanitize`, `Snapshot::crop`, a corrupt spool header, preset XML import,
    `kwin::decode`. Add `cargo clippy -D warnings` and `cargo fmt --check` to CI.
20. **Smaller notes** — `scene.rs` `dev_rect(...).unwrap()` panics on NaN coordinates from a hostile
    `.snagx`; `snagx.rs` pre-allocates from the zip header; the overlay clears `width_text` but not
    `height_text` after a window click; two full-canvas layers are allocated per shadowed object.
21. **Feature gaps** — several share destinations per preset, printing, templates, stamps, webcam and
    microphone in recordings, Windows cursor capture, a prompt before the OCR model download when a
    build lacks the bundled models.

## Suggested order

1. Item 6 (`Arc` blobs), then 7 (snapshot and scan off the UI thread).
2. Items 10 and 11 before the next package release.
3. Item 8, the biggest platform gap.
4. Items 12 to 15, then the structural, test and documentation clean-up.

## UI/UX audit: capture side (2026-10-18)

Fixed in 0.2.1 (**done**): a second trigger during an open overlay re-snapshotted the overlay itself
(now ignored); the Capture window came back after every capture even when it had been hidden (it
returns only if it was visible before); a too-small video region left no window open; Schedule had no
Stop and swallowed bad dates (now Start/Stop, validated, windows hidden only at capture time); interval
captures forced the Editor to the front every tick; selection handles were decorative (now resizable,
with cursor feedback); dimension boxes went blank after a window click or Enter; the Effects combo
listed unimplemented effects; the Share combo offered Print/FTP and could hold one destination (now
several); the draw-mode hotkey was grabbed system-wide for nothing; Ctrl+Shift+D style letter hotkeys
without modifiers are refused and Home resets the global key to Print Screen; "Capture Widget" menu
items toggled nothing (removed); preset Edit now writes back in place (Save / Stop editing in the presets
bar); Repeat Last Capture is in the File menu and the tray; unimplemented selection modes are no longer
offered; "Click a window" hint only where windows are known; own windows are ignored case-insensitively;
XKB names for PageUp/PageDown/Backspace in portal triggers; Webcam / Screen Draw / audio toggles greyed
out until they work.

Still open:
- **Capture Cursor** for still images is a no-op: the frozen snapshot is taken without the pointer.
  Take the snapshot with the cursor (KWin `include-cursor`) when the option is on, or draw a cursor
  object at the recorded position.
- **Window selection on Wayland**: `windows()` is empty, so Window mode behaves as Region. A KWin
  script can list windows and their geometry.
- **Fullscreen recording on a single monitor** places the toolbar inside the region; Snagit shrinks it
  to a mini bar. Hide it and rely on the hotkeys / tray, or move it to the tray.
- **Delay countdown**: no visual countdown while the windows are hidden; add a small bottom-right bubble.
- `--capture` at launch bypasses delay/schedule settings; fullscreen selection always takes the primary
  monitor (use the pointer's); scrolling direction keys (H/V/B) and horizontal mode; several preferences
  are stored but never read (`show_video_guides`, `mic_muted_alert`, `hdr_capture`, `tooltip_mode`,
  `capture_window_pos`); arrow-key nudge / Shift+Enter / G guides in the overlay.
- The GlobalShortcuts portal session is re-created on every rebind (see item 10 above).

## UI/UX audit: Editor (2026-10-18)

Fixed in 0.2.1 (**done**): Undo/Redo of raster operations (crop, resize, effects, flatten, fill) showed
the stale cached bitmap and saved the wrong pixels (renderer cache now invalidated); Image ▸ Resize
Image… did nothing (opens a dialog); crop/selection handles were decorative (now draggable); closing a
capture with unsaved changes was silent (Save / Don't save / Cancel); slider and colour-picker gestures
produced one undo step per frame (merged); Ctrl+T / Ctrl+Shift+F / Ctrl+Shift+B / Ctrl+; were shown
but not bound, Ctrl+D now duplicates when nothing is selected; Sharpen strength was inverted; the Shadow
effect used the Border colour; an opened PNG/JPG lost its file identity (Save now writes back to it);
View ▸ Library showed an empty grid until Refresh; "drop an image here" did nothing (drag-and-drop
opens files); the empty-state text was invisible on light workspaces; the transparent-colour button used
the light palette in dark mode; the decorative canvas edge handles are gone until they work; the Stamp,
Eraser, Magic Wand, Simplify and Cut Out tools say they are not available instead of doing nothing;
"Share Link" is hidden.

Still open:
- **Memory**: every edit clones the whole document including PNG blobs (item 6 above).
- **Whole-image operations flatten all annotations** (Crop, Rotate, Resize, effects, Fill, paste).
  Crop should translate objects; Fill should keep objects; warn before a destructive flatten.
- **Property controls that are not rendered**: Text outline/width and underline, Pen "vector /
  antialias / smooth", Callout tail "Arrow" vs "Triangle" (identical), tail colour/style.
- **Capture-preset effects ignore their parameters** (hard-coded black 4 px border, 12 px shadow,
  torn edge). Add per-effect settings in the Capture window.
- **Save As to PNG/JPG** leaves the original dirty and appends a flattened copy that becomes current.
- **Object handles**: no rotate handle, one callout tail only, no magnify offset drag; Step size slider
  does not resize a selected step; no right-click "Edit Value".
- **Tray**: no Ctrl+click multi-select, Delete/C keys, drag reorder, drag-to-canvas, rename.
- **Text editing**: the edit box uses the UI font and ignores vertical alignment, so text jumps on
  commit; a callout's tail vanishes while editing.
- Canvas edge handles (expand canvas) and the rounded-corner diamond, once implemented.
- Minor: caption title overlaps window buttons for long names; fixed-width floaties may clip;
  Ctrl+wheel zoom not anchored under the cursor; torn edge reads as static; Create ▾ items are
  placeholders; share errors swallowed; dead `tag_popup` / `zoom_popup` state; Favorites delete only
  searches the current theme; Shift while drawing an arrow constrains to H/V only, not 45°.
