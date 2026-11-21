#!/usr/bin/env bash
# build-appimage.sh — package the current tree into a distributable AppImage.
# Prereq: appimagetool (AppImage) available; set APPIMAGETOOL to its path.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APPIMAGETOOL="${APPIMAGETOOL:-appimagetool}"
command -v "$APPIMAGETOOL" >/dev/null || { echo "FATAL: appimagetool not found (set APPIMAGETOOL=/path/to/appimagetool)" >&2; exit 1; }
VERSION="$(node -p "require('$HERE/app/package.json').version")"

WORK="$(mktemp -d /tmp/zalo-appimage.XXXXXX)"
trap 'rm -rf "$WORK"' EXIT
install -d "$WORK/usr/bin" "$WORK/usr/lib"
cp -a "$HERE/app" "$WORK/usr/lib/zalo-app"
cp -a "$HERE/electron" "$WORK/usr/bin/electron"
install -m 755 "$HERE/appimage/AppRun" "$WORK/AppRun"
install -m 644 "$HERE/appimage/zalo.desktop" "$WORK/zalo.desktop"
install -m 644 "$HERE/appimage/zalo.png" "$WORK/.DirIcon"
install -m 644 "$HERE/appimage/zalo.png" "$WORK/zalo.png"

ARCH=x86_64 "$APPIMAGETOOL"  --comp zstd "$WORK" "$HERE/Zalo-$VERSION-linux-x86_64.AppImage"
echo "OK: $HERE/Zalo-$VERSION-linux-x86_64.AppImage"
