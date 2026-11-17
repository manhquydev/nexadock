# `ak-xia` Technical & Compatibility Study: `VN-Linux-Family/zalo-for-linux`

**Workflow:** `ak-xia --compare` (`Recon -> Map -> Analyze -> Challenge`)  
**Date:** `2026-11-17`  
**Branch:** `feat/ubuntu-multi-instance`  
**Claim Discipline (`ak-fable-thinking`):**
- **`CONFIRMED`**: Directly observed in repository source files, commit/release metadata, or compiled tests in this session.
- **`INFERENCE`**: Deterministic engineering consequence derived from observed code and documented Linux/Chromium semantics.
- **`AUTHOR CLAIM`**: Stated in upstream documentation without local live runtime execution on Ubuntu hardware in this session.

> **Strict IP & Security Boundary:** All inspected repository content was processed strictly as untrusted read-only data in an isolated scratch directory. Zero proprietary VNG files (`app.asar`, `.dmg`, `.exe`, `models.zmdl`) and zero community binaries were copied, bundled, or redistributed into this repository.

---

## 1. Recon — Source Manifest & Local Integration Surface

### 1.1. Source Manifest (`CONFIRMED`)
- **Source Repository:** `https://github.com/VN-Linux-Family/zalo-for-linux`
- **Default Branch & Pinned Commit:** `main` @ `bbb6f99e4fe5ead55895825ebc5387181ebf21c5` (`2026-11-13T15:23:44Z`, *"chore(nix): update sources for 26.11.10 (e080c70)"*)
- **Latest Published Release Inspected:** `26.11.10` (`2026-11-02T16:19:38Z`, build commit `e080c70`), shipping 4 `.AppImage` binaries + 4 `.zsync` metadata files (`Original` and `+ZaDark-26.3` across `x86_64` and `aarch64`)
- **Repomix Pack Summary:** `320` source files, `524,229` tokens (`1,911,789` chars), Secretlint security check: `0` suspicious files
- **Repository Code License:** `BSD-3-Clause` (`LICENSE:1-28`; `native/addons/zfile/LICENSE-zfile`, `native/addons/zwalker/LICENSE-zwalker`); `plugins/zadark` submodule (`MIT`)
- **Packaged Artifact License (`CONFIRMED`):** **Unfree / Proprietary VNG composite** — explicitly declared by the upstream maintainers themselves in `nix/package.nix:87-88` (`license = lib.licenses.unfree; sourceProvenance = [ lib.sourceTypes.binaryNativeCode ];`) because the build extracts and embeds VNG's proprietary macOS `app.asar` (`scripts/extract.js:124,271`) and Windows `ZaloSetup-<ver>.exe` OCR model vault (`scripts/extract.js:229-241`, `scripts/patches/patch-zocr-runtime.js:256-265`).

### 1.2. Local Integration Surface (`CONFIRMED`)
- **Target Crate:** `desktop-instance-manager/crates/platform-linux` (`dim-platform-linux`)
- **Consumed Traits (`dim-core`):** `ApplicationLocator`, `PlatformCapabilities`, `InstanceMonitor`, `InstanceLauncher`
- **Orchestration Consumers:** `dim-process-manager::InstanceCoordinator`, `dim-profile-store::{JsonProfileStore, FileBackupManager}`, `dim-test-fixture` (`dim-cli`), and `apps/desktop/src-tauri` (`commands.rs`)

---

## 2. Map — Component Inventory & Dependency Matrix

| Upstream `zalo-for-linux` Layer | Key Source Anchors | Local `dim-platform-linux` Relationship | Status |
| :--- | :--- | :--- | :--- |
| **AppImage Stage 1 & 2 Packaging** | `package.json:54-113`, `scripts/build.js:160-350`, `scripts/build-stage2.sh:60-146`, `nix/package.nix:38-43` | **Inspect only (`discovery.rs`)**: Verify ELF64 + Type-2 (`AI\x02`) and `uruntime`/DwarFS formats & SHA-256 digests; never build or redistribute AppImages. | `NEW` (implemented in `discovery.rs`) |
| **Electron Shell & Lifecycle** | `main.js:1-329`, `plugins/tray-host/index.js:1-58`, `plugins/wayland-titlebar/index.js:1-83` | **Manage externally (`adapter.rs`, `procfs.rs`)**: Spawn with isolated env, track `SingletonLock` + `/proc` tree, send graceful `SIGTERM` so `before-quit`/`will-quit` (`main.js:180-194`) flush state. | `NEW` (implemented in `adapter.rs` & `procfs.rs`) |
| **XDG User Paths & Storage** | `scripts/patches/patch-xdg-user-dirs.js:38-178`, `main.js:20,307,322` | **Isolate (`layout.rs`)**: Isolate `HOME`, `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `XDG_STATE_HOME`, `XDG_CACHE_HOME`, `TMPDIR`, plus `.config/user-dirs.dirs` so `getPath("documents"\|"downloads")` stays inside `<profile_dir>`. | `IMPROVED` (`layout.rs`) |
| **33 JS Bundle Patches & 10 Rust N-API Addons** | `scripts/apply-patches.js:9-43`, `Cargo.toml:1-16` | **Out of legal/architectural scope**: Modifying VNG's `app.asar` violates our zero-patch/zero-redistribution invariant. | `CONFLICT` (do NOT port) |
| **Native Call Engine (`zcall`)** | `scripts/patches/patch-zcall-native.js:49-56`, `plugins/zcall/index.js:45-175`, `plugins/zcall/window.js:534-553` | **Compatibility constraint (`adapter.rs` capabilities)**: Hardcoded loopback TCP `127.0.0.1:29631` & `127.0.0.1:29632` causes `EADDRINUSE` across concurrent instances -> `SupportStatus::Blocked`. | `CONFIRMED` constraint |
| **Native OCR Daemon (`zocr`)** | `native/engines/zocr/src/ipc/unix_socket.rs:49-63,128-131` | **Multi-instance safe**: Uses randomized `zocr-<16hex>.sock` (`0o600`) + `SO_PEERCRED` peer check; no cross-instance socket collision. | `CONFIRMED` compatible |

---

## 3. Analyze — Deep Dissection of All 13 Technical Subsystems

1. **AppImage Packaging (`CONFIRMED`):**
   - `scripts/build.js:204,212` runs `electron-builder --linux` (`target: "AppImage"`), producing a SquashFS AppImage.
   - `scripts/build-stage2.sh:87,108,122,135` extracts `--appimage-extract` (`dist/squashfs-root`), runs `scripts/trim-appdir.js` (removing unused Chromium `.pak` files, keeping `vi`, `en-US`, `en-GB`), and repacks with `quick-sharun.sh --make-appimage` using **`uruntime` + DwarFS (`zstd`)** (`nix/package.nix:38-43`).
   - *Implication:* Unlike legacy `electron-builder` Type-2 SquashFS AppImages that hard-fail on Ubuntu 24.04 without `libfuse2t64`, `uruntime` supports `libfuse3-3` / `fusermount3` (preinstalled on Ubuntu 24.04 Desktop) and falls back to `$TMPDIR` extraction when `/dev/fuse` is absent.
2. **AppRun (`CONFIRMED`):**
   - Stage 2 replaces Electron's default `AppRun` with `sharun` (`APP_NAME="Zalo"`, `STARTUPWMCLASS="zalo"`, `DESKTOP="zalo.desktop"` in `build-stage2.sh:60-63`), setting relocatable library paths while inheriting `HOME` and `XDG_*` from the caller.
3. **Desktop Integration (`CONFIRMED`):**
   - `package.json:55,106` sets `appId: "net.vietnamlinuxfamily.zalo_for_linux"` and `StartupWMClass: "Zalo"`.
   - `plugins/launcher-badge/index.js:64-122` emits Unity/GNOME dock `LauncherEntry.Update` signals over session D-Bus derived from `$APPIMAGE` or `zalo.desktop` — meaning unread badge counts from multiple instances merge onto the same desktop icon.
   - `scripts/patches/patch-auto-launch.js:33-34` calls `org.freedesktop.portal.Background.RequestBackground` with fixed `APP_ID="net.vietnamlinuxfamily.zalo_for_linux"`. File-based autostart entries (`~/.config/autostart/zalo.desktop`) remain isolated inside `<profile_dir>/.config/autostart/`.
4. **Electron Runtime (`CONFIRMED`):**
   - Uses Electron `^22.3.27` (`package.json:47`, Chromium 108). Because files inside FUSE-mounted AppImages are mounted `nosuid`, `chrome-sandbox` cannot use setuid root and relies on unprivileged user namespaces (`CLONE_NEWUSER`), which Ubuntu 24.04 AppArmor blocks for unconfined binaries (`docs/user/known-issues.md:9,20`).
5. **Home Directory & XDG Base Directory (`CONFIRMED`):**
   - `main.js:20` calls `app.setName('Zalo')`, then `main.js:322` loads VNG's `app/bootstrap.js`, whose `migration.js` sets `userData` to `path.join(app.getPath('appData'), 'ZaloData')` (`$XDG_CONFIG_HOME/ZaloData`).
   - `scripts/patches/patch-xdg-user-dirs.js:52-62` redirects `Zalo Received Files` and `Downloads` to Electron's `getPath("documents")` and `getPath("downloads")`. On Linux, Chromium reads `$XDG_CONFIG_HOME/user-dirs.dirs`; therefore pre-seeding `<profile_dir>/.config/user-dirs.dirs` inside each profile is essential to prevent received files from escaping to the host user's `~/Documents` or `~/Downloads`.
6. **Temporary Files (`CONFIRMED`):**
   - `plugins/screenshot/capture.js:20,130` writes `os.tmpdir()/zalo-screencap` (and extracts `portal.py` if packed). Setting `TMPDIR=<profile_dir>/tmp` isolates Node `os.tmpdir()`, Chromium shared-memory temp artifacts, VNG `Zalo Temp`/`zTemp`/`zpc-auc`, and `uruntime` fallback extraction.
7. **Process Ownership (`CONFIRMED`):**
   - Chromium creates `<profile_dir>/.config/ZaloData/SingletonLock` as a symlink `<hostname>-<pid>`. Helper processes include `zygote`, `gpu-process`, `utility`, `zocr-host`, and `zcall-native.js` (`ELECTRON_RUN_AS_NODE=1`) + `zcall-audio` / `python3 audio-io.py`.
   - `plugins/zcall/index.js:45-64,154,173` scans `/proc` for `zcall-native.js`/`audio-io.py`: on `stop()` it only signals `p.ppid === process.pid`, and on `start()` it only `SIGKILL`s orphans (`ppid === 1` or `systemd`).
8. **Native Dependencies (`CONFIRMED`):**
   - Host call/audio stack (`plugins/zcall/index.js:74-116`, `native/engines/zcall-audio/src/audio_backend.rs:55-84`): `libopus0` (`libopus.so.0`), `libpulse0` (`libpulse-simple.so.0`), `pulseaudio-utils` (`pactl`, `parec`, `pacat`), `python3`.
   - Host desktop utilities: `gdbus` (`libglib2.0-bin` for `tray-host`, `launcher-badge`, `auto-launch`, `flatpak-file-transfer`), `wl-clipboard` (Wayland) or `xclip` (X11) for `Ctrl+V` image paste.
9. **Update Mechanisms (`CONFIRMED`):**
   - Embeds `.zsync` `UPINFO` (`gh-releases-zsync|VN-Linux-Family|zalo-for-linux|latest|...AppImage.zsync`, `scripts/build.js:38-44`) for external `AM` (`am -u`) or `Gear Lever` updates. Profile data in `$XDG_DATA_HOME/MultiZaloDesktop/Profiles/` is decoupled from the `.AppImage` binary path.
10. **Audio/Video Components (`CONFIRMED`):**
    - `main.js:93-97` defaults `PULSE_SERVER=unix:${XDG_RUNTIME_DIR}/pulse/native` (compatible with Ubuntu 24.04 PipeWire `pipewire-pulse`). `plugins/zcall/index.js:142-149` appends `--enable-features=...,WebRTCPipeWireCapturer` on Wayland. Camera uses V4L2/Chromium `enumerateDevices` + WebCodecs H.264 (`plugins/zcall/window.js:28-38,144-153`).
11. **File Access (`CONFIRMED`):**
    - Unconfined AppImage runs with full ambient user filesystem permissions unless wrapped in `bwrap` (`BubblewrapNamespaceSandbox`). `scripts/patches/patch-disk-space.js:19-20` patches `analyzeMainDisk()` to stat `getZaloDirSync()` instead of `/`.
12. **IPC & Multi-Instance Call Blocker (`CONFIRMED`):**
    - **Critical Discovery (`scripts/patches/patch-zcall-native.js:49-56`):** Replaces Windows named pipes / macOS `/tmp/socketzalo*2021` with **hardcoded loopback TCP ports `127.0.0.1:29631` and `127.0.0.1:29632`**. Any second instance sharing the host network namespace hits `EADDRINUSE` on those ports (`SupportStatus::Blocked` for concurrent calls).
    - Conversely, `plugins/zcall/window.js:551` binds `127.0.0.1:0` (ephemeral port) and `native/engines/zocr/src/ipc/unix_socket.rs:49-63` binds randomized `zocr-<16hex>.sock` (`0o600` + `SO_PEERCRED`).
13. **Security Audit of Community Port (`CONFIRMED`):**
    - `scripts/patches/patch-linux-guards.js:9` disables code-signature checks (`this.isAppSigned=!1;return!1;`).
    - `plugins/zcall/window.js:269` opens the call `BrowserWindow` with `contextIsolation: false, nodeIntegration: true`.
    - `plugins/userscripts/index.js:198` injects arbitrary JS from `<userData>/userscripts.json` into the chat renderer on every `dom-ready`.

---

## 4. Challenge — Decision Matrix & Risk Score (`ak-xia` Phase 4)

| # | Decision | `zalo-for-linux` Way | `dim-platform-linux` Way | Risk | Verdict |
| :--- | :--- | :--- | :--- | :--- | :--- |
| 1 | **Proprietary VNG Assets** | Downloads macOS `.dmg` + Windows `.exe`, patches `app.asar`, ships unfree AppImage | Never download, patch, bundle, or redistribute VNG assets; manage only fixtures or user-supplied local binaries | Critical (Legal/IP) | **Local strict boundary (`OFFICIAL_ZALO_UNSUPPORTED`)** |
| 2 | **Ubuntu 24.04 AppArmor `userns`** | Recommends `--no-sandbox` or disabling kernel `userns` restriction (`known-issues.md:9`) | **Forbid `--no-sandbox` and kernel weakening (`SECURITY_BLOCKED`)**; detect targeted `/etc/apparmor.d/` `userns` profile | Critical (Security) | **Fail closed (`SECURITY_BLOCKED`) + safe AppArmor profile guidance** |
| 3 | **Profile Directory Confinement** | Single user `~/.config/ZaloData` + host XDG `Documents`/`Downloads` | Per-profile `HOME`, `XDG_*`, `TMPDIR`, pre-seeded `.config/user-dirs.dirs`, `zmeta.json`, `.zalo-profile-id`, mode `0o700` | Medium | **Full per-profile `HOME` + XDG + `user-dirs.dirs` confinement** |
| 4 | **Process Termination** | Relies on single-instance quit / `process.kill` | Verify PID tree via `/proc/<pid>/environ` (`DIM_PROFILE_ID`) + `SingletonLock`, `SIGTERM` -> `waitpid` -> scoped `SIGKILL`; zero `killall`/`pkill` | High (Data safety) | **PID-verified tree shutdown + `PROTECTED_PRIMARY_PROCESS`** |
| 5 | **Concurrent Voice/Video Calls** | Hardcodes `127.0.0.1:29631` & `29632` (`patch-zcall-native.js:51-52`) | Report `concurrent_calls_status = SupportStatus::Blocked` honestly | Low | **Document & report `BLOCKED` honestly** |

- **Critical Risk Count Resolved Before Implementation:** `2` (IP redistribution & `--no-sandbox` security downgrade — both mitigated via fail-closed guards). **Residual Architectural Risk:** **Low**.

---

## 5. Deliver — Implementation Outcome

Implemented idiomatically in Rust inside [`../../crates/platform-linux/`](../../crates/platform-linux/) and verified with [`../../crates/platform-linux/tests/ubuntu_adapter_suite.rs`](../../crates/platform-linux/tests/ubuntu_adapter_suite.rs) and [`../../tests/platform-linux/run-ubuntu-fixture-tests.mjs`](../../tests/platform-linux/run-ubuntu-fixture-tests.mjs). Full Ubuntu reports are published under [`../../docs/platform/ubuntu/`](../../docs/platform/ubuntu/).
