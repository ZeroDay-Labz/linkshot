# Third-party programs shipped with LinkShot

LinkShot's packages include these separately developed programs so that video export and text
recognition work without installing anything else. Each remains under its own licence, reproduced
in this directory; LinkShot itself is licensed under MIT OR Apache-2.0.

| Component | Version / build | Licence | Source |
| --- | --- | --- | --- |
| FFmpeg (`ffmpeg`) | BtbN "master-latest" GPL static build | GPL-3.0-or-later (see `ffmpeg-LICENSE.txt`) | https://github.com/BtbN/FFmpeg-Builds · https://ffmpeg.org |
| Tesseract OCR (`tesseract`, Linux) | 5.5.3 with Leptonica 1.87, AppImage by AlexanderP | Apache-2.0 (`tesseract-LICENSE`) | https://github.com/AlexanderP/tesseract-appimage · https://github.com/tesseract-ocr/tesseract |
| Tesseract OCR (`tesseract.exe`, Windows) | 5.4.0 (UB Mannheim build) | Apache-2.0 (`tesseract-LICENSE`) | https://github.com/UB-Mannheim/tesseract |
| Tesseract language data (`tessdata`) | eng, osd (+ deu, fin, fra, por, rus, spa on Linux) | Apache-2.0 | https://github.com/tesseract-ocr/tessdata |
| ocrs models (`ocrs/*.rten`) | text-detection, text-recognition | see the ocrs project | https://github.com/robertknight/ocrs |

The DLLs beside `tesseract.exe` (Leptonica, libpng, libjpeg-turbo, libtiff, libwebp, OpenJPEG, giflib,
libarchive, zlib, zstd, lz4, xz, bzip2, libexpat, libiconv, OpenSSL, GCC runtime) are the MSYS2 builds
distributed with the UB Mannheim installer and keep their respective licences.

Source code for the GPL components is available from the linked projects.
