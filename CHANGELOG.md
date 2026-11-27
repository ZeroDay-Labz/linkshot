# Changelog

## 0.6.0 — 2026-11-27

- **Check for Updates.** Help ▸ Check for Updates… (Capture window, Editor and tray menu) looks for
  a newer release on GitHub; Preferences can also check when LinkShot starts (off by default, at most
  once a day). Nothing contacts GitHub otherwise. A small window shows what is new and installs the
  update straight from the release: the package matching your installation is downloaded, checked
  against the release's SHA256SUMS.txt, and installed with your system's tool (dnf/rpm or apt behind
  the password prompt, Flatpak, or by replacing the Windows executable), then LinkShot restarts.
  Skip This Version keeps automatic checks quiet until the next release.
- **Smooth across monitors with different scaling.** LinkShot now draws with wgpu (Vulkan on NVIDIA,
  AMD and Intel; OpenGL ES where Vulkan is missing; DirectX 12 on Windows). The OpenGL renderer left
  trails and cut-off windows when a window moved between, say, a 4K monitor at 170 % and a 1080p one;
  it remains as an automatic fallback (or `LINKSHOT_RENDERER=glow`).
- **Title bars move the window on the first press**, like native ones (double-click still maximises
  the Editor). Before, the first press-and-drag on an inactive window was often lost.
- Buttons in the update window react to the first click.

## 0.5.3 — 2026-11-27

- **Print Screen works on KDE Plasma.** The key used to be bound to an action of LinkShot's desktop
  file; KDE stores such a binding but never listens for the key, so pressing Print did nothing. Print
  now goes to the running LinkShot's global shortcut (LinkShot starts at login and stays in the tray),
  and existing setups are moved over automatically.
- **Faster capture start.** LinkShot no longer waits for its own windows to disappear when none is
  showing: from the tray, the capture overlay appears about 100 ms after the key press (was ~400 ms).
- **Hotkey fields:** pressing a key the desktop delivers to LinkShot (Print Screen, or another of
  LinkShot's hotkeys) while a field is listening fills in the field instead of starting a capture.
- **Readable hotkeys:** `Ctrl+;`, `Ctrl+'`, `Ctrl+/`, `Ctrl+Page Up`, `Super` instead of internal
  key names such as `Oem1`.

## 0.5.2 — 2026-11-27

- **Setting Print Screen as a hotkey works.** The key cannot be pressed into a hotkey field (the
  toolkit has no Print Screen key and the desktop grabs it first), so a listening field now shows a
  **PrtSc** chip; hold Ctrl/Alt/Shift while clicking it to combine. Choosing plain Print Screen as the
  capture hotkey makes LinkShot the system screenshot tool, so the key actually reaches it.
- **Scrolling capture of photos and noisy content.** Pages without sharp, repeating rows (photos,
  gradients, video, compression noise) used to drop every frame; they now stitch exactly. Ambiguous
  frames are still dropped rather than guessed.
- **Automatic scrolling recovers** when a window ignores wheel steps: it re-aims and switches to
  smooth scrolling, and tells you if the window still does not scroll, instead of finishing with a
  single frame. It also waits briefly for lazily loaded content before grabbing a blank strip.
- The installed package points the login autostart entry at itself.

## 0.5.1 — 2026-11-27

- **Print Screen works again on Wayland.** Outside the Flatpak, LinkShot identified itself to the
  desktop as `linkshot`, which the GlobalShortcuts portal rejects ("An app id is required") when
  LinkShot is started from a terminal, an IDE or the autostart entry. Every build now uses the id
  `io.github.zerodaylabz.LinkShot` (desktop file, window id, autostart entry, shortcut component) and
  registers it with the portal at startup. Old `linkshot.desktop` files and bindings are migrated.
- **Print Screen repairs itself.** When LinkShot was made the screenshot tool but the binding has gone
  missing or is held by a stale entry (after a reinstall, or switching between the Flatpak and the
  package), it is bound again at startup.
- Global shortcut sessions are closed when the hotkeys change instead of piling up; shortcut errors
  are shown next to the hotkey settings.
- **Scrolling capture memory.** Frames whose content is fully shown by their neighbours are dropped
  as the capture goes, so a long smooth-scrolled page keeps a few dozen frames instead of hundreds
  (gigabytes at 1080p). The stitched result is unchanged. The HUD shows the stitched length.
- **Editor polish:** cool dark workspace matching the panels, a proper Fill tool icon, no empty band
  under panel headers, readable arrow-end and line-style names that fit the panel, a clean window
  title and an empty-state hint in the recent captures tray.

## 0.5.0 — 2026-10-20

- **Live capture overlay.** The selection overlay no longer takes a screenshot of every monitor and
  shows it back: it is a transparent layer over the real screen on every monitor, with the crosshair,
  window highlight, size label, handles and HUD drawn on top. The region is grabbed from the live
  screen once the selection is made (the overlay goes fully transparent for a moment first, so none of
  it is in the picture). The magnifier samples the live screen around the pointer. The old behaviour
  is still there as Preferences ▸ Capture ▸ "Freeze the screen during capture" (needed for menus and
  tooltips that close when they lose focus) and is used automatically where a transparent overlay
  cannot work (X11 without a compositor). The dashed video-size guides are gone.
- **Flatpak on multi-monitor KDE fixed.** The sandbox could not hand KWin the paths of its window
  placement scripts (they live under a different host path), so the per-monitor overlay windows piled
  up on one screen showing the wrong monitor; the controller window also showed in the taskbar. The
  paths are translated now. KWin's prompt-free screenshots can never work from a Flatpak (Flatpak
  strips that permission from exported desktop files by design), so the Flatpak captures through the
  **ScreenCast portal**: one share dialog, remembered with a restore token, then every monitor is
  delivered at its native size through PipeWire. The same backend serves GNOME and other Wayland
  desktops, which previously got one composite screenshot split with a single scale factor.
- **Editor drawing is instant.** While an object is created, moved or resized it is drawn by the GPU
  as a preview; the canvas is rasterised once when the mouse button is released.
- **Eraser** tool (width, opacity; erases to the canvas colour or to transparency, annotations
  untouched), **polygon shapes** by clicking corners (double-click or Enter closes), **callout tails**:
  the blue handle adds a tail, dragging a tail back into the callout removes it, and **Edit Value…** on
  a step's right-click menu.
- Diagnostics: `linkshot --probe-kwin` and `linkshot --probe-screencast` report what the capture
  back-ends can do (inside the Flatpak: `flatpak run --command=linkshot io.github.zerodaylabz.LinkShot --probe-kwin`).

## 0.4.0 — 2026-10-19

- **Cut Out tool** (Editor): drag a band across the rows or columns to remove it; the two parts are
  joined with a seamless, fade, tear, sharktooth, wave or saw seam. Annotations in the band go, the
  ones past it move up with the image.
- **Pointer in still captures on X11 and Windows** (XFixes / GDI), matching what KDE already did.
- **Library at scale.** The library scan reads only metadata (no image decoding) on a worker thread,
  the grid lays out only the visible rows, and each card shows a thumbnail cached as a small PNG.
  Tray videos are decoded when first shown rather than all at start-up.
- **Memory.** A document's image blobs are shared between undo steps and drag copies instead of being
  copied, so editing a large scrolling capture no longer multiplies its size per step.
- Editor drawing is much faster with drop shadows, translucency and large captures, and the window
  no longer jiggles while it is moved and can be resized from its edges and corners (plexscript, PRs
  #1 and #2).
- Windows: one instance per user (a second launch raises the running one or starts a capture).
- Flatpak: windows carry the Flatpak app id so the desktop shows the right icon for them.
- The selection overlay no longer repaints in a busy loop while idle.

## 0.3.1 — 2026-10-19

- **Flatpak: overlays landed on the wrong screens.** Inside the sandbox LinkShot was not allowed to own
  the D-Bus name its KWin helper replies to, and KWin's ScreenShot2 service was not on the talk list,
  so on a multi-monitor KDE desktop the per-screen overlay windows all stacked on the active screen,
  each showing another monitor's image, and screenshots went through the slower portal. The helper now
  uses a name under the app id and the manifest allows `org.kde.KWin.ScreenShot2`. Native packages were
  not affected.
- **Interface scale.** egui's built-in keyboard zoom rescaled every LinkShot window whenever Ctrl +/−/0
  zoomed the Editor canvas. That is off now; View ▸ Interface Scale (or Ctrl+Shift +/−, Ctrl+Shift+0)
  and a Preferences slider set the interface scale explicitly, on top of each monitor's own scaling.
- The Editor remembers its window size (and position where the platform reports it) and is fitted to
  the primary monitor instead of opening larger than the screen.

## 0.3.0 — 2026-10-19

- **Audio in recordings.** The Video tab records the microphone (device selectable) and system audio
  ("what you hear"); the recording toolbar has Mic / Audio switches, a level meter, a mute button (M, or
  the new Ctrl+Shift+M hotkey) and a blinking "microphone is muted" alert. Tracks are kept in sync with
  the frames (silence while paused), mixed into one WAV next to the lossless master, played back in the
  Editor (speaker button mutes) and muxed as AAC into MP4 exports. Opened MP4/MOV/WebM files bring their
  sound track along. Linux captures through the bundled ffmpeg (PipeWire / PulseAudio), Windows through
  WASAPI (loopback for system audio).
- **Webcam.** Record Webcam with device, placement (corners or full screen), shape (circle, rounded,
  square) and "start full screen"; W hides the overlay, Ctrl+Shift+F swaps corner / full screen.
- **Screen Draw.** D or Ctrl+Shift+D while recording opens a draw layer over the region: A arrow, S
  square, 1 numbered steps, C colour, Esc done; drawings fade after the configured seconds and are part
  of the video.
- **Stamp tool** in the Editor: 14 built-in stamps (check, cross, warning, star, heart, pin, …) plus
  any PNG/JPG in the stamps folder ("Add stamps…"), placed by clicking the canvas, sized by the Size
  property.
- Printing: share destination *Printer* and File ▸ Print (CUPS on Linux, the Print verb on Windows).
- Capture: the pointer is included in still captures on KDE Plasma when Capture Cursor is on; a
  "Capturing in N" bubble counts down delayed captures; Window selection works on KDE Plasma Wayland;
  G toggles 1080p/720p/640×480/320×240 guides and arrow keys nudge the selection in the overlay;
  capture-time effects have options (border colour/width, shadow size/direction, edge style/sides,
  filter, trim tolerance); recording a whole monitor uses a compact toolbar in the corner.
- Preferences: hotkeys (show/hide, repeat, video, mute, draw mode) are editable; webcam, muted-mic alert
  and compact-toolbar switches.
- Deleting a saved recording from the tray also removes its lossless master and sound track.

## 0.2.1 — 2026-10-18

- A second capture trigger while the selection overlay was open re-snapshotted the overlay itself, so
  the frozen "preview" stayed in the next capture. Triggers are now ignored while a capture is in
  progress, and the overlay windows close on the very next frame after a selection.
- The Capture window no longer pops back after every capture; it returns only if it was visible
  before the capture (cancelling still brings it back).
- Selection overlay: resize the region by its handles; dimension boxes always filled; the "Click a
  window" hint only where windows are known.
- Capture window: Schedule has Start/Stop and validates the time; interval captures stop yanking the
  Editor to the front; preset Edit writes back to the preset; Repeat Last Capture in File menu and
  tray; several share destinations per preset; menus list only working selection modes, effects and
  destinations; webcam/audio toggles greyed out until they record; Print Screen can be restored with
  Home in the hotkey field; bare letter hotkeys are refused.
- Editor: Undo/Redo of crop, resize, effects, flatten and fill showed stale pixels and could save
  them; Resize Image… now opens a dialog; crop/selection handles are draggable; unsaved changes
  prompt when closing a capture; slider gestures merge into one undo step; Ctrl+T, Ctrl+Shift+F/B,
  Ctrl+; bound, Ctrl+D duplicates when nothing is selected; Sharpen strength fixed; Shadow effect has
  its own colour; opened PNG/JPG files keep their name and save in place; View ▸ Library scans;
  drag-and-drop opens files; themed empty state; tools that are not implemented say so.
- The improvement plan (`docs/IMPROVEMENT-PLAN.md`) now records both UI/UX audits.

## 0.2.0 — 2026-10-18

- **Print Screen works system-wide.** On first start LinkShot asks to become the default screenshot
  tool. On KDE Plasma the key is bound through the desktop's global shortcuts (Spectacle's Print
  binding is cleared, stale registrations are removed), on GNOME through a custom media-key binding,
  and on Windows the "Print screen key opens Snipping Tool" option is turned off. The desktop launches
  `linkshot --capture`, so the key works even when LinkShot is closed. Preferences ▸ Capture can turn
  it on or off again.
- LinkShot starts at login when it is the default tool (Linux autostart entry, Windows Run key).
- README with badges, CI workflow, social preview image.

## 0.1.0 — 2026-10-18

First public release: capture, scrolling capture, video recording (GIF/MP4), Grab Text, editor;
self-contained packages for Windows, Fedora/RHEL, Debian/Ubuntu and Flatpak.
