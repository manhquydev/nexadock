#!/usr/bin/env bash
# build-deb.sh — package the current tree into zalo-linux_<version>_amd64.deb.
# Payload: /opt/zalo-linux (app + electron + patches + launcher scripts),
# /usr/bin/zalo, desktop file and icon.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VERSION="$(node -p "require('$HERE/app/package.json').version")"
DEB="$HERE/zalo-linux_${VERSION}_amd64.deb"

WORK="$(mktemp -d /tmp/zalo-deb.XXXXXX)"
trap 'rm -rf "$WORK"' EXIT

install -d "$WORK/DEBIAN" "$WORK/opt/zalo-linux" "$WORK/usr/bin" \
  "$WORK/usr/share/applications" "$WORK/usr/share/icons/hicolor/1024x1024/apps"

cp -a "$HERE/app" "$WORK/opt/zalo-linux/app"
cp -a "$HERE/electron" "$WORK/opt/zalo-linux/electron"
cp -a "$HERE/patches" "$WORK/opt/zalo-linux/patches"
install -m 755 "$HERE/zalo-instance.sh" "$WORK/opt/zalo-linux/zalo-instance.sh"
install -m 644 "$HERE/README.md" "$WORK/opt/zalo-linux/README.md"

cat > "$WORK/usr/bin/zalo" <<'EOF'
#!/bin/bash
# /usr/bin/zalo — launch one isolated Zalo desktop instance.
# Usage: zalo [instance-name] [extra electron args...]
set -euo pipefail
INSTANCE="$(basename "${1:-default}")"
if [ "$INSTANCE" = "." ] || [ "$INSTANCE" = ".." ]; then
  echo "FATAL: invalid instance name" >&2; exit 1
fi
if [ "$#" -gt 0 ]; then shift; fi
DATA_ROOT="${ZALO_DATA_ROOT:-$HOME/.local/share/zalo-linux}"
export ZALO_DATA_DIR="$DATA_ROOT/$INSTANCE"
mkdir -p "$ZALO_DATA_DIR"
exec /opt/zalo-linux/electron/electron /opt/zalo-linux/app --no-sandbox "$@"
EOF
chmod 755 "$WORK/usr/bin/zalo"

cat > "$WORK/usr/share/applications/zalo.desktop" <<'EOF'
[Desktop Entry]
Type=Application
Name=Zalo
Comment=Zalo Desktop cho Linux (đa instance: zalo <tên>)
Exec=zalo
Icon=zalo
Categories=Network;InstantMessaging;
Terminal=false
StartupWMClass=Zalo
Actions=new-instance;

[Desktop Action new-instance]
Name=Cửa sổ Zalo mới (instance riêng)
Exec=zalo secondary
EOF

install -m 644 "$HERE/appimage/zalo.png" \
  "$WORK/usr/share/icons/hicolor/1024x1024/apps/zalo.png"

cat > "$WORK/DEBIAN/control" <<EOF
Package: zalo-linux
Version: $VERSION
Section: net
Priority: optional
Architecture: amd64
Depends: libgtk-3-0, libnss3, libasound2t64 | libasound2, libgbm1, libxss1, libxtst6, libxrandr2, libatk-bridge2.0-0, libatk1.0-0, libcups2, libdrm2, libxkbcommon0, libpango-1.0-0, libcairo2
Maintainer: NexaDock <local@localhost>
Description: Zalo Desktop for Linux (repack of the official macOS client)
 Runs the official Zalo $VERSION desktop client (macOS build) inside a
 bundled Electron runtime (version auto-detected from the DMG), with a compatibility shim for Linux.
 Supports multiple isolated instances via \`zalo <instance-name>\`.
 Unofficial — not affiliated with VNG/Zalo.
EOF
cat > "$WORK/DEBIAN/postinst" <<'EOF'
#!/bin/bash
set -e
update-desktop-database /usr/share/applications 2>/dev/null || true
gtk-update-icon-cache -f /usr/share/icons/hicolor 2>/dev/null || true
echo "zalo-linux installed. Chạy: zalo  (hoặc: zalo <tên-instance>)"
EOF
cat > "$WORK/DEBIAN/prerm" <<'EOF'
#!/bin/bash
set -e
update-desktop-database /usr/share/applications 2>/dev/null || true
# Dữ liệu người dùng tại ~/.local/share/zalo-linux KHÔNG bị xoá.
EOF
chmod 755 "$WORK/DEBIAN/postinst" "$WORK/DEBIAN/prerm"

rm -f "$DEB"
dpkg-deb --root-owner-group --build "$WORK" "$DEB"
echo "OK: $DEB"
