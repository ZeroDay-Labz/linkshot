# Improvement plan

A prioritised list from a full read-through of the code on 2026-10-18. Line numbers refer to the
tree at that date. Items marked **done** were fixed in 0.2.0.

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
