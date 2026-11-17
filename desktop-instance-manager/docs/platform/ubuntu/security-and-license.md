# Security Findings & Community-Port License Assessment (Ubuntu Linux)

**Date:** `2026-11-17`  
**Branch:** `feat/ubuntu-multi-instance`  
**Workspace License:** `MIT OR Apache-2.0` ([`Cargo.toml`](../../../Cargo.toml), [`deny.toml`](../../../deny.toml))

---

## 1. License & Intellectual Property Assessment (`VN-Linux-Family/zalo-for-linux`)

### 1.1. Decomposition of Components & Rights (`CONFIRMED`)

| Component in `zalo-for-linux` (`bbb6f99`) | Observed Source / Path | License / Legal Status | Redistribution / Porting Eligibility |
| :--- | :--- | :--- | :--- |
| **Build scripts, JS AST patchers, Electron wrapper & plugins** | `main.js`, `scripts/**`, `plugins/**` (excluding submodule) | **`BSD-3-Clause`** (`LICENSE:1-28`) | Permissive open-source; inspected read-only for compatibility research (zero code copied). |
| **12 Rust N-API & daemon crates** | `native/addons/*` (10 crates), `native/engines/{zocr,zcall-audio}` | **`BSD-3-Clause`** (`Cargo.toml`, `LICENSE-zfile`, `LICENSE-zwalker`) | Permissive open-source; not needed in `dim-platform-linux` because we never rebuild `app.asar`. |
| **ZaDark integration submodule** | `plugins/zadark` (`quaric/zadark`) | **`MIT`** | Permissive open-source; out of scope for instance manager. |
| **Extracted macOS Zalo application bundle** | `ZaloSetup-universal-<version>.dmg` -> `Zalo.app/Contents/Resources/app.asar` & `ZaloCall` (`scripts/download.js:7`, `scripts/extract.js:124,271`) | **Proprietary — VNG Corporation (All Rights Reserved)** | **STRICTLY FORBIDDEN** to download, extract, patch, bundle, or redistribute. |
| **Extracted Windows Zalo OCR models & vault seed** | `ZaloSetup-<version>.exe` -> `plugins/ocr/zocr/models.zmdl` & `zocr-host.exe` `.text` key scan (`scripts/download.js:8`, `scripts/patches/patch-zocr-runtime.js:68-126,256-265`) | **Proprietary — VNG Corporation (All Rights Reserved)** | **STRICTLY FORBIDDEN** to extract, bundle, or redistribute. |
| **Compiled `.AppImage` Release Artifacts** | `Zalo-26.11.10-*.AppImage` (`nix/package.nix:87-88`) | **Composite Unfree (`lib.licenses.unfree`)** | **STRICTLY FORBIDDEN** to bundle, mirror, or auto-download inside `desktop-instance-manager`. |

### 1.2. Enforced Legal Invariants in `desktop-instance-manager`
1. **Zero Proprietary Redistribution:** Our repository contains zero bytes of VNG code, assets, keys, or community `.AppImage` binaries.
2. **Honest Support Reporting:** Official VNG Zalo on Linux is permanently classified `OFFICIAL_ZALO_UNSUPPORTED` (`SupportStatus::Unsupported`).
3. **Zero Locked Dependency Additions:** `dim-platform-linux` introduces zero external crates beyond existing workspace dependencies (`dim-core`, `dim-profile-store`, `serde_json`, `libc`), keeping `Cargo.lock` and `deny.toml` (`MIT OR Apache-2.0`) 100% compliant.

---

## 2. Security Audit of Community Port & User-Supplied Binaries

Having open-source build scripts in a GitHub repository does **not** make a community `.AppImage` inherently trustworthy:

1. **Disabled Code-Signature Verification (`CONFIRMED`):** `scripts/patches/patch-linux-guards.js:9` injects `if("darwin"!==process.platform){this.isAppSigned=!1;return!1;}` into `checkAppSigned`, because patching `app.asar` invalidates VNG's Apple code signature.
2. **Privileged Call Window Renderer (`CONFIRMED`):** `plugins/zcall/window.js:269` constructs the call `BrowserWindow` with `webPreferences: { contextIsolation: false, nodeIntegration: true }`. Although it loads local `ui/call.html`, it renders remote peer `avatar` URLs and call metadata (`window.js:268,443`) — any XSS or media-handling flaw inside that window would have direct Node.js `require('child_process')` access.
3. **Persistent Userscript Execution Surface (`CONFIRMED`):** `plugins/userscripts/index.js:198,264` reads `<userData>/userscripts.json` and executes enabled scripts via `win.webContents.executeJavaScript(...)` on every chat window `dom-ready`. If `<userData>` had weak POSIX permissions (`0o755`), another local unprivileged user or process could tamper with or read profile configs; `dim-platform-linux` mitigates local multi-user tampering by enforcing `0o700` on `<profile_dir>` and `.config/ZaloData`.
4. **Automated Daily Upstream Build Pipeline (`CONFIRMED`):** `.github/workflows/build.yml:4-5` runs a daily cron job that polls `zalo.me`, extracts binaries, applies patches, publishes releases, and pushes `nix/sources.json` via `github-actions[bot]`. Any user testing a local `.AppImage` must pin its SHA-256 digest (`with_expected_sha256`) and test only in an isolated non-production environment.

---

## 3. Ubuntu 24.04 AppArmor, `bubblewrap`, and `--no-sandbox` Policy (`SECURITY_BLOCKED`)

### 3.1. Why `--no-sandbox` and Kernel Weakening Are Forbidden
On Ubuntu 23.10+ and **Ubuntu 24.04 LTS**, `kernel.apparmor_restrict_unprivileged_userns = 1` prevents unconfined binaries from calling `unshare(CLONE_NEWUSER)`. Because files inside an unprivileged FUSE/DwarFS AppImage are mounted `nosuid`, Electron's internal `chrome-sandbox` cannot fall back to SUID root and fails at startup.

Upstream `zalo-for-linux` (`docs/user/known-issues.md:9,20`) suggests passing `--no-sandbox` or disabling Ubuntu's kernel restriction. **Both workarounds are unacceptable for `desktop-instance-manager`:**
- Passing `--no-sandbox` disables Chromium's renderer, GPU, and utility sandboxes — turning any renderer vulnerability in untrusted chat messages/media into immediate arbitrary code execution.
- Disabling `kernel.apparmor_restrict_unprivileged_userns` weakens kernel exploit mitigations across the entire Ubuntu host.
- Running desktop chat clients as `root` (`UID 0`) exposes the entire OS to compromise.

### 3.2. Enforced `SECURITY_BLOCKED` Controls in `dim-platform-linux`
Implemented in [`isolation.rs`](../../../crates/platform-linux/src/isolation.rs) and [`adapter.rs`](../../../crates/platform-linux/src/adapter.rs):
- **`RunningAsRoot`:** Refuses launch when `geteuid() == 0` (`is_elevated() == Some(true)`).
- **`NoSandboxForbidden`:** Rejects `--no-sandbox`, `--disable-setuid-sandbox`, and `--disable-namespace-sandbox` in `CustomExecutable.args` and wrapper scripts, and **never** injects `--no-sandbox` in `build_bwrap_args`.
- **`AppArmorUsernsRestricted`:** When `probe_ubuntu_environment()` detects `apparmor_enabled && apparmor_restrict_unprivileged_userns == Some(1)` and no `/etc/apparmor.d/` `userns` profile covers the target Electron AppImage, `launch()` fails closed with `SECURITY_BLOCKED` and explains the safe alternative.
- **`BubblewrapBlocked`:** When `.with_bwrap(true)` is enabled, verifies `bwrap` presence and `/etc/apparmor.d/bwrap*` readiness before launch; binds the external `.AppImage` read-only (`--ro-bind`) and binds **only** `<profile_dir>` read-write as `$HOME` (`--unshare-ipc --die-with-parent --tmpfs /tmp`), hiding the user's real `~/.ssh`, `~/.gnupg`, and sibling profiles.

### 3.3. Safe Alternative on Ubuntu 24.04 LTS
Rather than disabling `chrome-sandbox` or global AppArmor userns protection, a system administrator on a dedicated Ubuntu 24.04 machine can grant `userns` **solely** to a SHA-256-verified AppImage path (matching Canonical's own pattern in `/etc/apparmor.d/bwrap-userns-restrict` and `/etc/apparmor.d/chromium`):

```text
abi <abi/4.0>,
include <tunables/global>

profile dim_zalo_appimage "/opt/zalo/Zalo-verified.AppImage" flags=(unconfined) {
  userns,
}
```

`dim-platform-linux` provides `render_recommended_apparmor_profile()` for operator review only and **never** invokes `sudo`, `pkexec`, or `apparmor_parser` automatically.

---

## 4. Adapter Defense-in-Depth Summary

| Threat | Mitigation in `dim-platform-linux` | Verification Test |
| :--- | :--- | :--- |
| Broad process kill closing other profiles or primary Zalo | Zero `killall`/`pkill`; `SIGTERM` -> `waitpid(WNOHANG)` -> leaves-first `SIGKILL` strictly on `/proc` PIDs verified via `DIM_PROFILE_ID` & `PrimaryProtectionGuard` | `test_03`, `test_07` |
| Recycled PID spoofing after crash (`gnome-shell` reuses dead PID) | `entry_matches_profile` inspects `/proc/<pid>/environ` & `cmdline` (`DIM_PROFILE_ID`, `XDG_CONFIG_HOME`, `HOME`) | `test_09` |
| Cross-profile data overwrite | `.zalo-profile-id` verified before layout/launch (`PROFILE_MARKER_MISMATCH`) | `test_04`, Node `#04` |
| Received files leaking into real `~/Downloads` or `~/Documents` | Pre-seeds `.config/user-dirs.dirs` + `XDG_DOWNLOAD_DIR` / `XDG_DOCUMENTS_DIR` inside `<profile_dir>` | `test_05`, Node `#05` |
| Symlink escape / destructive delete following symlinks | `reject_if_symlink` on layout creation + `unlink_profile_junctions` unlinks symlinks before recursive delete | `test_10`, `test_11` |
| Ambient environment code injection (`LD_PRELOAD`, `NODE_OPTIONS`) | `Command::env_clear()` with strict `PASSTHROUGH_SESSION_ENV_KEYS` allowlist | `test_05` |
| Root execution / setuid binaries / malicious wrapper scripts | `geteuid() == 0`, `mode & 0o6000`, `mode & 0o002`, and `analyze_startup_script` blocked (`SECURITY_BLOCKED`) | `test_14` |
