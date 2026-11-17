# AppImage & `VN-Linux-Family/zalo-for-linux` Compatibility Reference

**Date:** `2026-11-17`  
**Upstream Reference Inspected:** [`VN-Linux-Family/zalo-for-linux`](https://github.com/VN-Linux-Family/zalo-for-linux) @ commit `bbb6f99e4fe5ead55895825ebc5387181ebf21c5` (release `26.11.10`)  
**Official VNG Linux Status:** **`OFFICIAL_ZALO_UNSUPPORTED`**  
**User-Supplied Local AppImage Status:** **`EXPERIMENTAL`** (adapter ready; live Zalo compatibility **`BLOCKED`** pending Ubuntu desktop + user-supplied binary + targeted AppArmor profile)

---

## 1. Classification Boundary

- **Not Official Zalo:** `VN-Linux-Family/zalo-for-linux` is an unofficial community port that downloads VNG's official macOS `.dmg` (`ZaloSetup-universal-<version>.dmg`) and Windows `.exe` (`ZaloSetup-<version>.exe`), extracts `app.asar` and OCR assets, applies 33 JavaScript AST/regex patches, replaces macOS native modules with 10 Rust N-API crates + 2 standalone Rust/Node engines (`zocr`, `zcall`), and packages the result into an AppImage.
- **No Bundling or Endorsement:** `desktop-instance-manager` never downloads, builds, patches, bundles, or redistributes any `.AppImage` or `app.asar`. It only allows an Ubuntu user to point `dim-platform-linux` at a local compatible binary/AppImage they chose to obtain under their own legal responsibility.

---

## 2. AppImage Packaging & Runtime Anatomy (`CONFIRMED`)

### 2.1. Two-Stage Build (`electron-builder` -> `quick-sharun` / `uruntime` DwarFS)
Inspection of `scripts/build.js` (`lines 160-288`), `scripts/build-stage2.sh` (`lines 60-146`), `scripts/trim-appdir.js`, and `nix/package.nix` (`lines 38-43`) confirms a two-stage packaging pipeline:

1. **Stage 1 (`electron-builder --linux`):** Wraps Electron `^22.3.27` (`main.js`, `plugins/*`, patched `app/`, `zcall-native/`, `app/ocr/`) into an initial SquashFS AppImage.
2. **AppDir Trimming (`scripts/trim-appdir.js`):** Removes unused Chromium `.pak` locale files (retaining `vi`, `en-US`, `en-GB`).
3. **Stage 2 (`quick-sharun.sh --make-appimage`):** Replaces Electron's `AppRun` with `sharun` and repacks the AppDir using **`uruntime` + DwarFS (`zstd`)** (`DWARFS_COMP="zstd:level=19 -B2 -S26"`), embedding `.zsync` update metadata (`UPINFO`).

### 2.2. Why `dim-platform-linux` Supports Both Type-2 Magic & `uruntime` DwarFS
- Classic AppImage Type-2 files carry ELF64 magic `\x7fELF` followed by `0x41 0x49 0x02` (`"AI\x02"`) at byte offset `8` and require `libfuse.so.2` (`libfuse2t64` on Ubuntu 24.04).
- `quick-sharun` / `uruntime` DwarFS AppImages are static/relocatable ELF64 executables (`x86_64` `0x3E` or `aarch64` `0xB7`) embedding `DWARFS` / `uruntime` / `gh-releases-zsync` markers (`nix/package.nix:38` notes standard `appimageTools.extractType2` cannot read DwarFS). `uruntime` natively uses `libfuse3-3` (`fusermount3`, preinstalled on Ubuntu 24.04 Desktop) and falls back to self-extraction in `$TMPDIR` when `/dev/fuse` is unavailable.
- [`discovery::classify_binary_bytes`](../../../crates/platform-linux/src/discovery.rs) detects **both** `BinaryFormatKind::AppImageType2` and `BinaryFormatKind::AppImageUruntimeDwarfs`, as well as extracted `AppDir` directories (`AppRun` / `zalo`) and ELF64 binaries.

### 2.3. Published Release Digest Reference (`26.11.10`, commit `e080c70`)
Recorded in `discovery::KNOWN_COMMUNITY_RELEASES` strictly for read-only provenance transparency when inspecting user-supplied files:

| Asset Filename | Arch | Size (bytes) | Published SHA-256 |
| :--- | :---: | ---: | :--- |
| `Zalo-26.11.10-Original-e080c70-x86_64.AppImage` | `x86_64` | `156,891,519` | `ea150bcb290838959075d9d8948ac4bbab386ac885a99a293ce4e614c68d96ed` |
| `Zalo-26.11.10+ZaDark-26.3-e080c70-x86_64.AppImage` | `x86_64` | `157,013,306` | `565f328b64085e3b61d355a0ee38b6437e3cd62c7a4ca06630a30ec7a712fa4a` |
| `Zalo-26.11.10-Original-e080c70-aarch64.AppImage` | `aarch64` | `155,137,596` | `1df516bc6ba536588c1d6215a73c98adab6293977261e81a816d9977b0b953b5` |
| `Zalo-26.11.10+ZaDark-26.3-e080c70-aarch64.AppImage` | `aarch64` | `155,267,699` | `4800ea36e420ddb1ee93e8f4419697edbd486147657aa139b525a411a2bd4704` |

---

## 3. Ubuntu 24.04 LTS vs 22.04 LTS Dependency Matrix

| Capability / Component | Shared Object / Binary | Ubuntu 24.04 LTS (`noble`) | Ubuntu 22.04 LTS (`jammy`) | Missing-Dependency Impact |
| :--- | :--- | :--- | :--- | :--- |
| **`uruntime` DwarFS AppImage mount** | `fusermount3`, `libfuse3.so.3` | `fuse3`, `libfuse3-3` (default) | `fuse3`, `libfuse3-3` | Falls back to slower `$TMPDIR` self-extraction if `/dev/fuse` unavailable. |
| **Classic Type-2 AppImage mount** | `libfuse.so.2` | `libfuse2t64` | `libfuse2` | AppImage fails to mount without `--appimage-extract-and-run`. |
| **Voice / Video Call Audio (`zcall-audio`)** | `libopus.so.0`, `libpulse-simple.so.0`, `pactl`, `parec`, `python3` | `libopus0`, `libpulse0`, `pulseaudio-utils`, `python3` | `libopus0`, `libpulse0`, `pulseaudio-utils`, `python3` | Calls rejected by `__zcallPrepare()` (`plugins/zcall/index.js:104-131`); chat still runs. |
| **Tray / Autostart / Badge D-Bus** | `gdbus` | `libglib2.0-bin` | `libglib2.0-bin` | `tray-host` detects `available = false`; closing window quits app instead of minimizing to tray. |
| **Clipboard Image Paste (`Ctrl+V`)** | `wl-paste` (Wayland) / `xclip` (X11) | `wl-clipboard` / `xclip` | `wl-clipboard` / `xclip` | Image paste degrades to file attachment or no-op (`known-issues.md:11`). |
| **Screenshots & Wayland Screen Share** | `org.freedesktop.portal.Screenshot` / PipeWire | `xdg-desktop-portal`, `xdg-desktop-portal-gnome` | `xdg-desktop-portal`, `xdg-desktop-portal-gnome` | Built-in screenshot / Wayland screen share fails. |
| **`desktop-instance-manager` Tauri UI** | `libwebkit2gtk-4.1.so.0`, `libayatana-appindicator3.so.1` | `libwebkit2gtk-4.1-0`, `libayatana-appindicator3-1`, `librsvg2-2` | `libwebkit2gtk-4.1-0` (via `jammy-updates`), `libayatana-appindicator3-1`, `librsvg2-2` | Tauri GUI fails to start (`dim-cli` headless binary unaffected). |

---

## 4. Mandatory Verification Protocol for User-Supplied Binaries

Before running any user-supplied `.AppImage` on an Ubuntu test workstation:

1. **Origin & Checksum Pinning:** Record exact download URL/build origin and pass the expected SHA-256 to `LinuxPlatformAdapter::new().with_expected_sha256("<64-hex>")` (rejects mismatches with `UNVERIFIED_BINARY`).
2. **Startup Script & Permission Inspection:** Run `adapter.inspect_local_binary(path)` to confirm non-setuid/non-world-writable permissions (`0o755`) and zero unsafe wrapper commands (`sudo`, `--no-sandbox`, `killall`, `LD_PRELOAD`).
3. **AppArmor 4.0 Profile (Ubuntu 24.04 LTS):** Do **not** pass `--no-sandbox` (`SECURITY_BLOCKED`) and do **not** disable `kernel.apparmor_restrict_unprivileged_userns`. Instead, a system administrator on a dedicated test machine may review and load a narrow per-binary AppArmor profile (`isolation::render_recommended_apparmor_profile`) granting `userns,` only to that verified AppImage path.
4. **Disposable Account Workspace:** Point `DIM_DATA_DIR` to a scratch directory containing zero production Zalo data or personal credentials.

---

## 5. Known Multi-Instance Constraints in `zalo-for-linux`

1. **Concurrent Calls Blocked (`127.0.0.1:29631` & `127.0.0.1:29632`):** `scripts/patches/patch-zcall-native.js:49-56` hardcodes loopback TCP ports `29631` and `29632`. Two instances in the same network namespace cannot bind both ports simultaneously (`EADDRINUSE`).
2. **Shared Desktop/Notification Identity (`StartupWMClass=Zalo`, `zalo.desktop`):** Dock badges (`plugins/launcher-badge/index.js`) and desktop notifications group under `Zalo`.
3. **Update Mechanism (`.zsync`):** Updating the `.AppImage` via `AM` or `Gear Lever` replaces only the executable file; isolated profiles under `$XDG_DATA_HOME/MultiZaloDesktop/Profiles/<slug>_<id>/` remain untouched, though any path-specific `/etc/apparmor.d/` rule must match the updated filename or stable symlink path (e.g. `~/Applications/Zalo.AppImage`).
