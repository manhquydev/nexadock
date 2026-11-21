#!/usr/bin/env bash
# rebuild-zalo-linux.sh — rebuild the Zalo Linux desktop from the macOS DMG.
# Usage: rebuild-zalo-linux.sh [path-to-ZaloSetup-universal-*.dmg]
set -euo pipefail
set -x

DMG="${1:-$HOME/Downloads/ZaloSetup-universal-26.11.10.dmg}"
OUT="${ZALO_OUT_DIR:-$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)}"
PATCHES="${ZALO_PATCHES_DIR:-$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/patches}"
WORK="$(mktemp -d /tmp/zalo-rebuild.XXXXXX)"
trap 'rm -rf "$WORK"' EXIT

for cmd in 7z npx npm unzip; do
  command -v "$cmd" >/dev/null || { echo "FATAL: required command '$cmd' not found"; exit 1; }
done

# 1. Extract DMG (HFS+ volume) — 7z handles UDZO/HFS+
mkdir -p "$WORK/dmg"
7z x -y "$DMG" -o"$WORK/dmg" >/dev/null
APP_SRC="$(find "$WORK/dmg" -maxdepth 3 -name 'Zalo.app' | head -1)"
test -n "$APP_SRC"

# 2. Extract app.asar
npx --yes @electron/asar extract "$APP_SRC/Contents/Resources/app.asar" "$WORK/app"

# 3. Install shim + patches
install -m 644 "$PATCHES/zalo-linux-shim.js" "$WORK/app/zalo-linux-shim.js"
sed -i "1i require('./zalo-linux-shim');" "$WORK/app/bootstrap.js"
# Preload/utility bundles run in separate processes: load the shim there too.
for bundle in preload-sqlite utility-process-sqlite preload-render preload-noti preload-shared-worker mainless-worker compact-app-preload; do
  test -f "$WORK/app/main-dist/$bundle.js" && sed -i "1i require('../zalo-linux-shim');" "$WORK/app/main-dist/$bundle.js"
done
# Linux sqlite3 binding (community fix for the login freeze, zalo-for-linux
# issue #13): the login chain loads ./binding/napi-v6-linux-x64/node_sqlite3.node,
# which the macOS DMG does not ship. Install a real N-API v6 Linux build.
if [ ! -f "$PATCHES/node_sqlite3.node" ]; then
  echo "FATAL: $PATCHES/node_sqlite3.node missing — Linux login freeze without it"; exit 1
fi
mkdir -p "$WORK/app/native/nativelibs/sqlite3/binding/napi-v6-linux-x64"
install -m 755 "$PATCHES/node_sqlite3.node" \
  "$WORK/app/native/nativelibs/sqlite3/binding/napi-v6-linux-x64/node_sqlite3.node"
# checkAppSigned: skip macOS codesign verification on Linux (spawn codesign
# would ENOENT and break the login promise chain).
node -e '
const fs=require("fs");
const p=process.argv[1]; let s=fs.readFileSync(p,"utf8");
const m="async checkAppSigned(){";
const i=s.indexOf(m);
if(i>=0 && !s.includes("darwin\"!==process.platform){this.isAppSigned=!1;return!1;}")){
  s=s.slice(0,i)+m.slice(0,-1)+"{if(\"darwin\"!==process.platform){this.isAppSigned=!1;return!1;}"+s.slice(i+m.length);
  fs.writeFileSync(p,s); console.log("checkAppSigned patched");
}' "$WORK/app/main-dist/main.js"
# Window controls: macOS draws traffic lights for titleBarStyle:"hidden";
# Linux gets native decorations instead, and the login window gets a frame.
node -e '
const fs=require("fs");
const p=process.argv[1]; let s=fs.readFileSync(p,"utf8");
let a=0,b=0;
s=s.replace(/frame:!1,titleBarStyle:\"hidden\"/g,()=>{a++;return "frame:!0,titleBarStyle:\"hidden\"";});
s=s.replace(/titleBarStyle:\"hidden\"/g,()=>{b++;return "titleBarStyle:\"darwin\"===process.platform?\"hidden\":\"default\"";});
fs.writeFileSync(p,s); console.log("window patch:",a,b);
' "$WORK/app/main-dist/main.js"
# db-cross-v4: Rust native addon (community build) for message-sync DB access
# across processes. Install the prebuilt linux/electron/x64 binary and patch
# the JS loader, which only knows darwin/win32 paths.
DBX_BIN="$PATCHES/db-cross-v4-native.node"
if [ ! -f "$DBX_BIN" ]; then
  # Vendored prebuilt binary absent: build the clean-room Rust source shipped
  # alongside this script (db-cross-v4-src/, BSD-3-Clause community code) with cargo.
  SRC="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/db-cross-v4-src"
  test -f "$SRC/Cargo.toml" || { echo "FATAL: no prebuilt db-cross-v4-native.node and no db-cross-v4-src/ — message sync breaks without it"; exit 1; }
  command -v cargo >/dev/null || { echo "FATAL: cargo not found — needed to build db-cross-v4 from db-cross-v4-src/"; exit 1; }
  echo "Building db-cross-v4-native.node from db-cross-v4-src/ ..."
  (cd "$SRC" && RUSTFLAGS="--remap-path-prefix=$HOME/.cargo=/cargo-home --remap-path-prefix=$SRC=/build" cargo build --release)
  DBX_BIN="$SRC/target/release/libdb_cross_v4_native.so"
  test -f "$DBX_BIN" || { echo "FATAL: cargo build did not produce $DBX_BIN"; exit 1; }
fi
DBX="$WORK/app/native/nativelibs/db-cross-v4"
mkdir -p "$DBX/prebuilt/linux/electron/x64"
install -m 755 "$DBX_BIN" \
  "$DBX/prebuilt/linux/electron/x64/db-cross-v4-native.node"
node -e '
const fs=require("fs");
const p=process.argv[1]; let s=fs.readFileSync(p,"utf8");
if (!s.includes("process.platform === \"linux\"") && /else \{\s*if \(process\.arch === .x64.\)/.test(s)) {
  s=s.replace(/else \{\s*if \(process\.arch === .x64.\)/,
    "else if (process.platform === \x27linux\x27) {\n    addon = require(\x27../prebuilt/linux/electron/x64/db-cross-v4-native.node\x27);\n}\nelse {\n    if (process.arch === \x27x64\x27)");
  fs.writeFileSync(p,s); console.log("db-cross-v4 binding.js patched for linux");
}
' "$DBX/dist/binding.js"
  node -e '
const fs=require("fs");
const p=process.argv[1]; let s=fs.readFileSync(p,"utf8");
if (s.includes("case\"LINUX\":return 25;")) {
  s=s.replaceAll("case\"LINUX\":return 25;","case\"LINUX\":return 23;");
  fs.writeFileSync(p,s); console.log("platform ID 25 -> 23 (macOS) patched");
}
' "$WORK/app/main-dist/main.js"
# Community network-and-sync fix: real network state for gwig callers,
# sync-message routing to SyncMessageController (V1), SQLite backup adapter
# flags, call-feature flags. Vendored standalone script, no external deps.
if [ ! -f "$PATCHES/community/patch-network-and-sync-fix.js" ]; then
  echo "FATAL: $PATCHES/community/patch-network-and-sync-fix.js missing — sync/network breaks without it"; exit 1
fi
ZALO_APP_DIR="$WORK/app" node "$PATCHES/community/patch-network-and-sync-fix.js"

# 4. Install Electron runtime (version must match the DMG's Electron Framework)
ELECTRON_VERSION="$(grep -ao 'Electron/[0-9.]*' "$APP_SRC/Contents/Frameworks/Electron Framework.framework/Electron Framework" | head -1 | cut -d/ -f2)"
test -n "$ELECTRON_VERSION" || { echo "FATAL: could not detect Electron version from the DMG"; exit 1; }
mkdir -p "$OUT"
if [ "${ZALO_SKIP_ELECTRON:-0}" = 1 ] && [ -d "$OUT/electron" ]; then
  # Test/verification mode: reuse the existing electron runtime dir.
  echo "Reusing existing Electron runtime at $OUT/electron"
elif [ -n "${ELECTRON_ZIP:-}" ]; then
  test -f "$ELECTRON_ZIP" || { echo "FATAL: ELECTRON_ZIP='$ELECTRON_ZIP' is not a file"; exit 1; }
  UNZIP_LIST="$(unzip -l "$ELECTRON_ZIP")"
  echo "$UNZIP_LIST" | grep -q "electron-v$ELECTRON_VERSION-linux-x64" || {
    echo "FATAL: ELECTRON_ZIP does not match Electron $ELECTRON_VERSION linux-x64 detected in the DMG"; exit 1; }
  rm -rf "$OUT/electron" && mkdir -p "$OUT/electron"
  unzip -qo "$ELECTRON_ZIP" -d "$OUT/electron"
else
  WORKRT="$WORK/rt"; mkdir -p "$WORKRT" && cd "$WORKRT"
  printf '{"name":"zalo-electron-rt","version":"1.0.0","private":true}\n' > package.json
  npm install --no-audit --no-fund --save-dev "electron@$ELECTRON_VERSION"
  rm -rf "$OUT/electron" && mv node_modules/electron/dist "$OUT/electron"
fi

# 5. Ship app tree
rm -rf "$OUT/app"
mkdir -p "$OUT"
mv "$WORK/app" "$OUT/app"
[ -f "$OUT/zalo-instance.sh" ] && chmod +x "$OUT/zalo-instance.sh"

echo "OK: Zalo Linux rebuilt at $OUT (Electron $ELECTRON_VERSION)"
