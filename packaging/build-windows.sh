#!/bin/sh
# Builds the single-file Windows executable: the tool bundle (ffmpeg, tesseract, OCR models) is
# compressed into the binary and unpacked to %LOCALAPPDATA%\LinkShot\tools on first run.
# Needs: rustup target x86_64-pc-windows-gnu, cargo-zigbuild (+ zig or `pip install ziglang`), zstd.
set -eu
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
BUNDLE="$HERE/third-party/bundle/windows"
[ -d "$BUNDLE" ] || { echo "run fetch-tools.sh and make-bundles.sh first" >&2; exit 1; }
mkdir -p "$ROOT/dist" "$HERE/third-party"
TARBALL="$HERE/third-party/bundle-windows.tar.zst"
tar -C "$BUNDLE" --owner=0 --group=0 --numeric-owner -cf - . | zstd -19 -T0 -q -f -o "$TARBALL"
ID="$(sha256sum "$TARBALL" | cut -c1-16)"
VERSION="$(grep -m1 '^version' "$ROOT/Cargo.toml" | sed 's/.*"\(.*\)".*/\1/')"
# embed-resource (exe icon + version info) wants `windres`; LLVM's llvm-windres does the job.
if ! command -v windres >/dev/null 2>&1 && command -v llvm-windres >/dev/null 2>&1; then
  mkdir -p "$ROOT/target/tools-bin" && ln -sf "$(command -v llvm-windres)" "$ROOT/target/tools-bin/windres" && ln -sf "$(command -v llvm-windres)" "$ROOT/target/tools-bin/x86_64-w64-mingw32-windres" && PATH="$ROOT/target/tools-bin:$PATH"
fi
cd "$ROOT"
LINKSHOT_BUNDLE="$TARBALL" LINKSHOT_BUNDLE_ID="$ID" \
  cargo zigbuild --release --target x86_64-pc-windows-gnu -p linkshot --features bundled-tools
cp target/x86_64-pc-windows-gnu/release/linkshot.exe "dist/LinkShot-$VERSION-windows-x64.exe"
ls -la "dist/LinkShot-$VERSION-windows-x64.exe"
