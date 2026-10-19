# Changelog

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
