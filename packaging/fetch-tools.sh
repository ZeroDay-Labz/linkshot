#!/bin/sh
# Downloads the third-party programs LinkShot bundles (ffmpeg, Tesseract + language data, ocrs models)
# into $1 (default packaging/third-party/src) and extracts them. Run make-bundles.sh afterwards.
set -eu
SRC="${1:-$(dirname "$0")/third-party/src}"
mkdir -p "$SRC"
cd "$SRC"
get() { [ -f "$2" ] || curl -sSL -o "$2" "$1"; }
get https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-linux64-gpl.tar.xz ffmpeg-linux64.tar.xz
get https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip ffmpeg-win64.zip
get https://github.com/AlexanderP/tesseract-appimage/releases/download/v5.5.3/tesseract-5.5.3_lept-1.87-x86_64.AppImage tesseract.AppImage
get https://digi.bib.uni-mannheim.de/tesseract/tesseract-ocr-w64-setup-5.4.0.20240606.exe tesseract-w64-setup.exe
get https://ocrs-models.s3-accelerate.amazonaws.com/text-detection.rten text-detection.rten
get https://ocrs-models.s3-accelerate.amazonaws.com/text-recognition.rten text-recognition.rten
rm -rf x && mkdir x && cd x
tar xJf ../ffmpeg-linux64.tar.xz
unzip -q ../ffmpeg-win64.zip
chmod +x ../tesseract.AppImage && ../tesseract.AppImage --appimage-extract >/dev/null
mkdir tess-win && 7z x -otess-win ../tesseract-w64-setup.exe >/dev/null
echo "extracted into $SRC/x"
