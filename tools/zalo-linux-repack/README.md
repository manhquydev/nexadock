# zalo-linux repack pipeline (Experimental)

Clean-room rebuild pipeline that turns the **user-supplied macOS Zalo installer
(DMG from zalo.me)** into a runnable Linux desktop client: extract `app.asar`,
apply Linux compatibility patches and native addons, bundle an Electron runtime,
and optionally package a `.deb` or an AppImage. NexaDock can detect and launch
the resulting install (Experimental route).

## Layout

| Path | Purpose |
|---|---|
| `rebuild-zalo-linux.sh` | Main pipeline: DMG → patched app tree + Electron runtime |
| `zalo-instance.sh` | Launch one isolated Zalo instance (own `ZALO_DATA_DIR`) |
| `build-deb.sh` | Package the rebuilt tree into `zalo-linux_<version>_amd64.deb` |
| `build-appimage.sh` | Package the rebuilt tree into an AppImage (needs `appimagetool`) |
| `appimage/` | AppRun, desktop entry, icon for the AppImage |
| `patches/zalo-linux-shim.js` | Linux compatibility shim injected into the app |
| `patches/node_sqlite3.node` | N-API v6 Linux sqlite3 binding (MIT, npm `sqlite3` prebuilt) — fixes the Linux login freeze |
| `patches/db-cross-v4-native.node` | Linux message-sync DB addon (built from `db-cross-v4-src/`) |
| `patches/community/patch-network-and-sync-fix.js` | Community network-state + sync-message routing fix |
| `db-cross-v4-src/` | Clean-room Rust/NAPI-RS source for the db-cross-v4 addon (BSD-3-Clause, alolite specs) |

## Prerequisites

- `7z`, `unzip`, `node`, `npm` (npx is used to run `@electron/asar`)
- `rust` + `cargo` — only needed if `patches/db-cross-v4-native.node` is
  absent; `rebuild-zalo-linux.sh` then builds the addon from `db-cross-v4-src/`
  with `cargo build --release`
- For clipboard image paste (Ctrl+V inline image): `xclip` (X11) or
  `wl-clipboard` (Wayland) — the paste patch falls back to these when
  Electron's clipboard API cannot read the image

## Usage

Download `ZaloSetup-universal-<version>.dmg` yourself from [zalo.me](https://zalo.me) (the
pipeline never ships or downloads VNG's proprietary app payload), then:

```bash
./rebuild-zalo-linux.sh ~/Downloads/ZaloSetup-universal-26.11.10.dmg
./zalo-instance.sh default          # run it
sudo ./build-deb.sh && sudo dpkg -i zalo-linux_*_amd64.deb   # optional .deb
./build-appimage.sh                 # optional AppImage
```

Environment knobs (all optional):

- `ZALO_OUT_DIR` — output directory (default: script directory)
- `ZALO_PATCHES_DIR` — patches directory (default: `./patches`)
- `ELECTRON_ZIP` — pre-downloaded `electron-v<version>-linux-x64.zip` (must match
  the Electron version detected in the DMG, e.g. from the npmmirror CDN)
- `ZALO_SKIP_ELECTRON=1` — reuse an existing `$OUT/electron` (verification mode)

All scripts resolve paths relative to their own location, so the directory can
be vendored or moved freely.

## Clean-room rationale (CONTRIBUTING.md compliance)

This directory contains **no VNG/Zalo proprietary assets**: no `app.asar`, no
extracted app payload. The proprietary app code only ever exists transiently in
the user's `/tmp` working directory after the user extracts their own DMG. What
is committed here is exclusively:

- local build/patch orchestration scripts (this project),
- the Linux shim and community patch script (authored here / community),
- MIT-licensed `sqlite3` prebuilt binary (npm `sqlite3` project), and
- the db-cross-v4 Rust addon: BSD-3-Clause community clean-room code built from
  the open alolite wire/container specifications (source vendored in
  `db-cross-v4-src/`).

## Status: Experimental

Voice calls, message sync and other behaviours are not guaranteed; sync is
unverified. See `docs/operations.md` in the NexaDock repo for the full caveats
and how NexaDock detects this install.
