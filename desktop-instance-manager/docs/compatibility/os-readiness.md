# OS Readiness & Compatibility Matrix

**Last Verified:** 2026-11-17  
**Reference Zalo Builds Inspected:** Windows `26.10.20.3253` (Stub `1.0.0.0`, Installer `ZaloSetup-26.11.12.exe`); static macOS branches inspected inside `app.asar` (`Zalo 26.10.20`).

---

## 1. Platform Readiness Overview

| OS Target | Official VNG Zalo Client | Overall Manager Status | Verification Level | Primary Delivery / Runtime Path |
| :--- | :--- | :--- | :--- | :--- |
| **Windows 10 (x64, non-SAC)** | Yes (PE32 x86 `Zalo.exe`) | **`PARTIALLY_SUPPORTED`** (`SUPPORTED_AND_TESTED` for L1/L2 isolation) | Verified locally (`29` Rust unit tests, Tauri 2 build, parity suite, live Zalo `26.10.20` PoC) | Native Rust/Tauri 2 (`dim-desktop.exe` / `dim-cli.exe`) or Signed-Host Bridge (`powershell.exe` + `node.exe`) |
| **Windows 11 (x64, Smart App Control ON)** | Yes (PE32 x86 `Zalo.exe`, `CN=VNG GROUP JSC`) | **`PARTIALLY_SUPPORTED`** (`SUPPORTED_AND_TESTED` via Signed-Host Bridge) | Host SAC verified (initially `1` at baseline; external transition to `0` logged cleanly) | **Signed-Host Bridge** (`scripts/dim-host.ps1` + `apps/desktop/ui-server.mjs`); standalone EXE distribution requires Authenticode cert (Gate F) |
| **macOS (Apple Silicon `arm64` & Intel `x86_64`)** | Yes (`/Applications/Zalo.app`) | **`NOT_TESTED`** | Static `app.asar` + MIT `ducngfi` code review only | `dim-platform-macos` (`open -n -a ... --env CFFIXED_USER_HOME=...`, `SingletonLock`) — awaiting physical Mac test |
| **Ubuntu 22.04 LTS / 24.04 LTS** | **No** (no VNG Linux build) | **`UNSUPPORTED`** (Official Zalo) / **`EXPERIMENTAL`** (Test fixture & local AppImage) | Static `app.asar` + `VN-Linux-Family` review only | `dim-platform-linux` (`HOME`/`XDG_*` + optional `bwrap`); Ubuntu 23.10+/24.04 AppArmor `--no-sandbox` caveat |

---

## 2. Feature Readiness Matrix by Operating System

| Feature / Invariant | Windows 10/11 | macOS (`arm64` / `x86_64`) | Ubuntu LTS (Official Zalo) | Ubuntu LTS (Fixture / Local AppImage) |
| :--- | :--- | :--- | :--- | :--- |
| **Dynamic App Discovery** | **`SUPPORTED_AND_TESTED`** (`Zalo-*` semver + Authenticode check) | **`NOT_TESTED`** (`/Applications/Zalo.app` + Info.plist) | **`UNSUPPORTED`** | **`EXPERIMENTAL`** (explicit local path / `fixture-app.mjs`) |
| **Isolated Multi-Instance Launch** | **`SUPPORTED_AND_TESTED`** (`CreateProcessW`, isolated env + pre-seeds) | **`NOT_TESTED`** (`open -n --env CFFIXED_USER_HOME`) | **`UNSUPPORTED`** | **`EXPERIMENTAL`** (`XDG_*`/`HOME` + optional `bwrap`) |
| **Primary Session Protection** | **`SUPPORTED_AND_TESTED`** (`PROTECTED_PRIMARY_PROCESS` via `%APPDATA%\ZaloData\lockfile`) | **`NOT_TESTED`** (primary `SingletonLock` guard) | **`UNSUPPORTED`** | **`EXPERIMENTAL`** (primary XDG path guard) |
| **Stateless PID Ownership** | **`SUPPORTED_AND_TESTED`** (`rstrtmgr.dll` on `lockfile` + `--user-data-dir` fallback) | **`NOT_TESTED`** (`SingletonLock` symlink `<host>-<pid>` + bundle check) | **`UNSUPPORTED`** | **`EXPERIMENTAL`** (`SingletonLock` / PID lockfile + `/proc`) |
| **Live Rename Without Breaking Handles** | **`SUPPORTED_AND_TESTED`** (immutable `directory_name` `<slug>_<8hex>`) | **`NOT_TESTED`** (same `profiles.json` invariant) | **`UNSUPPORTED`** | **`EXPERIMENTAL`** (verified on fixture) |
| **Graceful Shutdown & Crash Recovery** | **`SUPPORTED_AND_TESTED`** (`--z-update-force-quit` + `WM_CLOSE` -> scoped child tree) | **`NOT_TESTED`** (`NSRunningApplication.terminate` + open-DB guard) | **`UNSUPPORTED`** | **`EXPERIMENTAL`** (`SIGTERM` / `.quit-signal`) |
| **Duplicate Logged-In Account Guard** | **`SUPPORTED_AND_TESTED`** (`Database/_production/<6+ digits>` + `config.json` `db_path`) | **`NOT_TESTED`** (same `Database/_production/` layout) | **`UNSUPPORTED`** | **`EXPERIMENTAL`** (verified on fixture) |
| **Offline Atomic Backup & Restore** | **`SUPPORTED_AND_TESTED`** (`.partial` -> `backup.json` SHA-256; restore to new profile) | **`NOT_TESTED`** (`dim-profile-store` portable logic) | **`UNSUPPORTED`** | **`EXPERIMENTAL`** (`dim-profile-store` portable logic) |
| **Notifications** | **`BLOCKED`** (requires 2+ live accounts; shares `AppUserModelId("com.vng.zalo")`) | **`NOT_TESTED`** (shared bundle ID in Notification Center) | **`UNSUPPORTED`** | **`NOT_TESTED`** |
| **File Send / Receive Isolation** | **`BLOCKED`** in live multi-account test (static paths resolve inside profile) | **`NOT_TESTED`** (requires user-folder symlinks for Finder expectations) | **`UNSUPPORTED`** | **`NOT_TESTED`** |
| **Concurrent Voice / Video Calls** | **`BLOCKED`** (`\\.\pipe\PipeZCallSend` / `PipeZCallRecv` global pipe collision) | **`BLOCKED`** (projected; `main.js:314800` unlinks `/tmp/socketzalorecv2021`) | **`UNSUPPORTED`** | **`UNSUPPORTED`** |

---

## 3. Environmental Preflight Rules & Failure Modes

### 3.1. Windows Folder Redirection Guard (`CONFIRMED`)
- **Check:** Inspect `HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\User Shell Folders` value `AppData` with `RegistryValueOptions.DoNotExpandEnvironmentNames`.
- **Pass Condition:** Value is absent or equals `%USERPROFILE%\AppData\Roaming` (case-insensitive, trailing slash trimmed).
- **Failure Mode:** If an enterprise GPO or OneDrive Known Folder Move redirects `AppData` to a hardcoded or UNC path, Windows `SHGetKnownFolderPath(FOLDERID_RoamingAppData)` ignores per-process `USERPROFILE` overrides and writes directly into the redirected primary store.
- **Manager Action:** Refuse isolated launch with an explicit diagnostic message rather than risking primary profile contamination.

### 3.2. Windows Smart App Control (`VerifiedAndReputablePolicyState = 1`)
- **Host Measurement:** Observed `VerifiedAndReputablePolicyState = 1`, `SAC_PreviousState = 2`, `SAC_EnforcementReason = 1`.
- **Operational Guidance:**
  - Never disable SAC or advise users to turn off WDAC.
  - On SAC-enforcing Windows 11 systems without an Authenticode-signed `desktop-instance-manager` release binary, launch through the **Signed-Host Bridge** (`powershell.exe` + `node.exe`).
  - Distributing standalone Rust/Tauri binaries (`apps/desktop/src-tauri/`) for end-users requires an Authenticode code-signing certificate in release CI (**Gate F**).

### 3.3. Free Disk Space Safety Threshold (`5 GiB`)
- **Constant:** `MIN_FREE_DISK_BYTES = 5 * 1024 * 1024 * 1024` (`5 GiB`, defined in [`crates/core/src/models.rs`](../../crates/core/src/models.rs)).
- **Rule:** Before creating a new profile, starting an offline backup, or staging a restore, verify at least `5 GiB` remains free *after* accounting for the estimated payload size, preventing partial SQLite/cache writes on full volumes.

### 3.4. Zalo Desktop Auto-Update Resilience & Future Watchlist
- **Observed Update Mechanism:** `Zalo.exe` (`Update.exe --meta-file update_meta.json`) installs new versions into sibling folders `%LOCALAPPDATA%\Programs\Zalo\Zalo-<ver>\` while preserving `%APPDATA%\ZaloData`.
- **Why We Survive Updates:**
  1. `ApplicationLocator` dynamically discovers the highest valid `Zalo-*` directory at launch time (never hardcoding `Zalo-26.10.20`).
  2. Managed profiles live completely outside `Programs\Zalo` (`%LOCALAPPDATA%\MultiZaloDesktop\Profiles\`).
  3. Zero edits to `Zalo.exe` or `app.asar` mean VNG updates apply cleanly without breaking patches.
- **Operational Caveat During Multi-Instance Updates:** When multiple secondary instances remain open while `Update.exe` runs, old version folders (e.g., `Zalo-26.9.10`, `Zalo-26.10.11`) may remain temporarily locked until open instances close. Recommend closing secondary instances before applying a Zalo version upgrade.
- **Future Regression Watchlist:**
  - *Global Singleton Mutex:* If a future Zalo build moves from path-scoped `requestSingleInstanceLock()` (`lockfile`) to a global mutex (`Global\Zalo...`), environment redirection alone would fail.
  - *Direct Registry AppData Lookup:* If a future build bypasses `SHGetKnownFolderPath` / `%APPDATA%` to read `User Shell Folders` directly, additional CLI/shim adaptation would be needed.

### 3.5. macOS & Ubuntu Readiness Gates
- **macOS Exit Criteria (from `NOT_TESTED` to `PARTIALLY_SUPPORTED`):**
  1. Execute on physical Apple Silicon (`arm64`) and Intel (`x86_64`) macOS 14+ with official `/Applications/Zalo.app`.
  2. Verify `open -n -a /Applications/Zalo.app --env CFFIXED_USER_HOME=...` isolates `Library/Application Support/ZaloData` without breaking Gatekeeper/notarization.
  3. Measure macOS Keychain prompt behavior (`Library/Keychains` symlink) across 2+ concurrent profiles.
  4. Empirically measure whether `/tmp/socketzalorecv2021` `fs.unlink` disrupts active calls when a second instance initializes.
- **Ubuntu LTS Exit Criteria (`EXPERIMENTAL` Fixture/Local AppImage):**
  1. Verify `dim-platform-linux` XDG/`HOME` isolation and `dim-test-fixture` on Ubuntu 22.04/24.04 CI runners.
  2. Document AppArmor `kernel.apparmor_restrict_unprivileged_userns = 1` behavior for `bwrap` and user-supplied AppImages (`--no-sandbox` risk disclosure).

**Related Documents:** [platform-feasibility.md](../research/platform-feasibility.md) | [environment-readiness.md](../research/environment-readiness.md) | [adr-002-rust-tauri-and-sac-host-bridge.md](../architecture/adr-002-rust-tauri-and-sac-host-bridge.md) | [plan.md](../../plans/261117-zalo-instance-manager/plan.md)
