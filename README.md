# LinkShot

Screen capture, scrolling capture, video recording, text extraction and an image editor for
**Linux and Windows**, written in Rust. Every download is self-contained: video export (ffmpeg) and
text recognition (Tesseract) ship inside the package, so nothing else has to be installed.

LinkShot is developed by [ZeroDay-Labz](https://github.com/ZeroDay-Labz) and licensed under MIT OR Apache-2.0.

## Download

Releases are on the [Releases](https://github.com/ZeroDay-Labz/linkshot/releases) page:

| Platform | File | Install |
| --- | --- | --- |
| Windows 10/11 (x64) | `LinkShot-<version>-windows-x64.exe` | Run it. The first start unpacks the bundled tools into `%LOCALAPPDATA%\LinkShot\tools`. |
| Fedora / RHEL / openSUSE | `linkshot-<version>.x86_64.rpm` | `sudo dnf install ./linkshot-<version>.x86_64.rpm` |
| Debian / Ubuntu / Mint | `linkshot_<version>_amd64.deb` | `sudo apt install ./linkshot_<version>_amd64.deb` |
| Any Linux (Flatpak) | `LinkShot-<version>.flatpak` | `flatpak install --user LinkShot-<version>.flatpak` (needs the Flathub remote for the runtime) |

## What it does

* **Capture window** with All-in-One / Image / Video tabs, presets with hotkeys, time delay, interval and
  scheduled captures, a big red Capture button and a presets drawer.
* **Region / window / fullscreen / fixed-region / clipboard / file captures** from a frozen-screen overlay
  with crosshair, window highlight, magnifier (M), aspect locks (Shift / Ctrl / Ctrl+Shift), dimension boxes
  and the All-in-One HUD (Image / Video / Scrolling). The overlay covers every monitor at once and a region
  may span screens.
* **Scrolling capture**: choose *Scrolling* in the HUD or the *Scrolling Capture* preset (Ctrl+Alt+/), scroll
  the page yourself or press *Auto-scroll*, then *Done*. Frames are grabbed at native resolution and
  stitched pixel-exactly: sticky headers/footers kept once, pinned toolbars kept at their real position,
  scrollbars ignored, upward and horizontal scrolling supported, and a frame that overlaps nothing is
  reported ("Lost the overlap – scroll back a little") rather than guessed at.
* **Video recording**: *Video* in the HUD or the Video tab, Record / Pause / Stop toolbar with countdown and a
  frame around the region, Shift+F9 pause / Shift+F10 stop. Frames are grabbed at native resolution at
  10/20/30 fps with the pointer and spooled losslessly. The Editor plays the recording (scrubber, frame
  stepping, in/out points) and saves it as an **animated GIF** (default; presets Screen Video / High
  Motion / Reduced File Size / Custom) or **MP4** (H.264 through the bundled ffmpeg). Exports always start
  from the lossless master. Existing GIF/MP4 files open in the player too. Audio and webcam are not
  recorded yet.
* **Grab Text**: the *Grab Text* selection mode or Edit ▸ Grab Text extracts text with the bundled
  Tesseract into a results window with Copy All.
* **Editor**: Move, Arrow, Line, Shape, Highlight, Text, Callout, Step, Pen, Blur, Magnify, Fill, Crop and
  Selection tools with properties and quick styles (themes with import/export); undo/redo; arrange; flatten;
  effects (border, shadow, edges, filters, colour adjustment, colour replacement, scale, trim); rotate/flip;
  resize; canvas colour; recent-captures tray with pinning; library browser; Copy All; Save / Save As
  (SNAGX, PNG, JPG, GIF, BMP, TIF, WebP, ICO); share to File / Clipboard / Program / Email. Dark mode is the
  default (View ▸ Theme), the UI uses the bundled Inter font and renders at the display's native pixel
  density.
* **Desktop integration**: global hotkeys (X11 and Windows natively; Wayland through the GlobalShortcuts
  portal), tray icon (left click opens the Capture window, middle click captures), one instance per user
  (launching again raises the running Capture window; `linkshot --capture` starts a capture from a script),
  prompt-free capture on KDE Plasma Wayland (KWin `ScreenShot2`), the XDG Screenshot portal on other Wayland
  desktops, `xcap` on X11 and Windows.

Not yet: audio and webcam in recordings, stamps, Simplify, Magic Wand, Cut Out, Eraser, templates, cloud
share destinations, printing.

## Screenshots (rendered headlessly from the real code)

| Capture window | Editor |
| --- | --- |
| ![capture](docs/screenshots/capture_window_image.png) | ![editor](docs/screenshots/editor.png) |

![scrolling HUD](docs/screenshots/scrolling_hud.png) ![Grab Text results](docs/screenshots/grab_text_results.png)

## Building from source

```sh
cargo build -p linkshot --release          # Linux
cargo test                                 # format, spool and GIF encoder tests
cargo run -p linkshot -- --selftest        # drives capture -> editor -> save -> record -> GIF/MP4 headlessly
cargo run -p linkshot -- --shots DIR       # renders the UI to PNGs without a display
```

Linux build dependencies (Fedora names): `gtk3-devel libxcb-devel libxkbcommon-devel wayland-devel
pipewire-devel dbus-devel fontconfig-devel mesa-libgbm-devel`.

### Packages

The bundled third-party tools (see [`packaging/THIRD-PARTY.md`](packaging/THIRD-PARTY.md)) are fetched once:

```sh
./packaging/fetch-tools.sh && ./packaging/make-bundles.sh
cargo deb -p linkshot                       # dist: target/debian/*.deb
cargo generate-rpm -p linkshot              # dist: target/generate-rpm/*.rpm
./packaging/build-windows.sh                # dist/LinkShot-<version>-windows-x64.exe (cargo-zigbuild + zig)
flatpak run org.flatpak.Builder --user --force-clean --install-deps-from=flathub \
    --repo=packaging/flatpak/repo packaging/flatpak/build packaging/flatpak/io.github.zerodaylabz.LinkShot.yml
flatpak build-bundle packaging/flatpak/repo dist/LinkShot-<version>.flatpak io.github.zerodaylabz.LinkShot
```

A system `ffmpeg` or `tesseract` is used when the bundle is missing (development builds), and
`LINKSHOT_FFMPEG` / `LINKSHOT_TESSERACT` / `LINKSHOT_TOOLS` override the lookup.

See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for the crate layout and the window model.
