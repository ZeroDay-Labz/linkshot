# Changelog

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
