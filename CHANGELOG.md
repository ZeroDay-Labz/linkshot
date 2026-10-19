# Changelog

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
