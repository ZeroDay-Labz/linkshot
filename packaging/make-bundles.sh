#!/bin/sh
# Assembles the tool bundles that ship inside the LinkShot packages (see crates/linkshot-tools):
#   $OUT/linux/    ffmpeg, tesseract/ (relocatable AppImage tree), ocrs/, LICENSES/
#   $OUT/windows/  ffmpeg.exe, tesseract/ (exe + DLLs + tessdata), ocrs/, LICENSES/
# Usage: make-bundles.sh [SRC-from-fetch-tools.sh] [OUT]
set -eu
HERE="$(cd "$(dirname "$0")" && pwd)"
SRC="${1:-$HERE/third-party/src}"
OUT="${2:-$HERE/third-party/bundle}"
X="$SRC/x"
rm -rf "$OUT"; mkdir -p "$OUT/linux/ocrs" "$OUT/linux/LICENSES" "$OUT/windows/ocrs" "$OUT/windows/LICENSES" "$OUT/windows/tesseract/tessdata"

# --- Linux ---
cp "$X"/ffmpeg-master-latest-linux64-gpl/bin/ffmpeg "$OUT/linux/ffmpeg"
cp -a "$X/squashfs-root" "$OUT/linux/tesseract"
rm -rf "$OUT/linux/tesseract/usr/share/man" "$OUT/linux/tesseract/usr/share/doc" "$OUT/linux/tesseract/tesseract.png" "$OUT/linux/tesseract/.DirIcon" "$OUT/linux/tesseract/tesseract-env.desktop"
cp "$SRC/text-detection.rten" "$SRC/text-recognition.rten" "$OUT/linux/ocrs/"

# --- Windows ---
cp "$X"/ffmpeg-master-latest-win64-gpl/bin/ffmpeg.exe "$OUT/windows/ffmpeg.exe"
TW="$X/tess-win"
cp "$TW/tesseract.exe" "$OUT/windows/tesseract/"
# Only the DLLs tesseract.exe (transitively) imports; the installer also carries training tools.
python3 - "$TW" "$OUT/windows/tesseract" <<'PY'
import os, subprocess, sys, shutil
src, dst = sys.argv[1], sys.argv[2]
have = {f.lower(): f for f in os.listdir(src) if f.lower().endswith('.dll')}
need, todo = set(), [os.path.join(src, 'tesseract.exe')]
while todo:
    out = subprocess.run(['objdump', '-p', todo.pop()], capture_output=True, text=True).stdout
    for line in out.splitlines():
        if 'DLL Name:' in line:
            k = line.split('DLL Name:')[1].strip().lower()
            if k in have and have[k] not in need:
                need.add(have[k]); todo.append(os.path.join(src, have[k]))
for f in sorted(need): shutil.copy(os.path.join(src, f), dst)
print(f"{len(need)} DLLs")
PY
# The installer's DLLs carry debug info (libtesseract alone is ~100 MB); strip it.
for f in "$OUT"/windows/tesseract/*.dll "$OUT"/windows/tesseract/tesseract.exe; do mingw-strip --strip-all "$f" 2>/dev/null || x86_64-w64-mingw32-strip --strip-all "$f" 2>/dev/null || llvm-strip --strip-all "$f"; done
cp "$TW/tessdata/eng.traineddata" "$TW/tessdata/osd.traineddata" "$TW/tessdata/pdf.ttf" "$OUT/windows/tesseract/tessdata/"
cp -r "$TW/tessdata/configs" "$TW/tessdata/tessconfigs" "$OUT/windows/tesseract/tessdata/"
cp "$SRC/text-detection.rten" "$SRC/text-recognition.rten" "$OUT/windows/ocrs/"

# --- Licences and provenance ---
for os_ in linux windows; do
  cp "$HERE/THIRD-PARTY.md" "$OUT/$os_/LICENSES/README.md"
  [ -f "$X/ffmpeg-master-latest-linux64-gpl/LICENSE.txt" ] && cp "$X/ffmpeg-master-latest-linux64-gpl/LICENSE.txt" "$OUT/$os_/LICENSES/ffmpeg-LICENSE.txt"
  for f in "$X"/squashfs-root/usr/share/doc/tesseract-ocr/*; do [ -f "$f" ] && cp "$f" "$OUT/$os_/LICENSES/tesseract-$(basename "$f")"; done
done
du -sh "$OUT/linux" "$OUT/windows"
