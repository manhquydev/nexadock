# Cross-Platform Feasibility & Zalo Desktop Internal Mechanics

**Evidence Basis:** Direct static inspection of official Windows Zalo `26.10.20` (`Zalo.exe`, `app.asar`) and `ZaloSetup-26.11.12.exe` via REA `v6.9.0` + ASAR parser, combined with live Windows 11 x64 behavioral PoC evidence in [`multi-zalo-desktop/evidence/`](../../../multi-zalo-desktop/evidence/rea-evidence-summary.json).

---

## 1. Platform Support Taxonomy

Every platform and feature claim in this repository adheres to the following six-state enum (`dim_core::SupportStatus`):

| Status | Meaning |
| :--- | :--- |
| `SUPPORTED_AND_TESTED` | Mechanism verified directly on target OS hardware with passing automated/live evidence. |
| `PARTIALLY_SUPPORTED` | Core multi-instance isolation works and is tested, while specific secondary subsystems (e.g., concurrent calls) remain blocked or constrained. |
| `EXPERIMENTAL` | Implemented for synthetic test fixtures or user-supplied local Electron/AppImage binaries only; no official vendor binary exists. |
| `BLOCKED` | Verified technical conflict (e.g., global OS pipe) or missing test prerequisite (e.g., 2+ live logged-in accounts) prevents verification. |
| `UNSUPPORTED` | Official vendor binary does not exist for this platform; workarounds (Wine, Web wrappers, repackaged ASARs) are out of scope. |
| `NOT_TESTED` | Architecture derived from shipped static artifacts (`app.asar`) and MIT reference code, but unverified on physical OS hardware. |

---

## 2. Official Zalo Desktop Internals (`Zalo 26.10.20` / `ZaloSetup 26.11.12`)

### 2.1. Startup & Single-Instance Lock Chain (`CONFIRMED`)

From static analysis of `Programs\Zalo\Zalo.exe` (PE32 x86 stub, `790,568` bytes) and `Zalo-26.10.20\resources\app.asar` (`177,728,582` bytes, Electron `22.3.9` / Chromium `108.0.5359.215`):

1. **Stub Launcher (`Zalo.exe`):** Acquires short-lived mutex `ZaloStubExecutable` (`OpenMutexW`/`CreateMutexW`), scans `\Zalo-*` version directories for the newest valid `Zalo.exe` + `resources\app.asar`, logs to `%APPDATA%\ZaloData\log.log`, spawns `Zalo-<ver>\Zalo.exe` via `CreateProcessW` inheriting the environment block, waits up to `5,000 ms` (`WaitForInputIdle`), then releases `ZaloStubExecutable` and exits. Launching `Zalo-<ver>\Zalo.exe` directly avoids even this short-lived stub mutex.
2. **Pre-Lock UserData Override (`app.asar/bootstrap.js` -> `main-dist/migration.js`):**
   - `bootstrap.js` runs `require('./main-dist/migration')` **before** `app.requestSingleInstanceLock()`.
   - `migration.js` (`offset 600654..605000`) computes `d = path.join(app.getPath("appData"), "ZaloData")` and calls `app.setPath("userData", d)`.
   - **Consequence:** Chromium's `--user-data-dir` CLI flag is overwritten unless `app.getPath("appData")` itself resolves to an isolated directory per process.
3. **Single-Instance Enforcement (`app.requestSingleInstanceLock()`):**
   - On Windows, Chromium `ProcessSingleton` opens `<userData>\lockfile` with exclusive sharing (`FileShare.None`) and creates `Chrome_MessageWindow`.
   - On macOS/Linux, Chromium creates `<userData>/SingletonLock` (symlink `<hostname>-<pid>`) and `SingletonSocket`.
   - When a second instance encounters the lock, `bootstrap.js` loads `main-dist/second-instance.js`, appends a timestamp to `<appData>\ZaloData\startup.log`, triggers `second-instance` on the owning instance (`main.js:3565794` -> `setAlwaysOnTop(true)` + `focus()`), and calls `app.quit()`.

### 2.2. Electron Fuses & DPAPI Storage (`CONFIRMED`)

REA `inspect_electron_fuses` (`ev_8e05df8e...`) on `Zalo-26.10.20\Zalo.exe` confirms:
- `EnableCookieEncryption`: **`enabled`** (uses Windows `CRYPT32.dll` DPAPI `CryptProtectData` / `CryptUnprotectData` bound to the current Windows user SID on Windows, and Keychain `Safe Storage` on macOS).
- `EnableEmbeddedAsarIntegrityValidation`: `disabled`, `OnlyLoadAppFromAsar`: `disabled` (nonetheless, our architecture strictly forbids modifying `app.asar`).
- `Zalo.exe` PE hardening (`ev_fce14a1d...`): `DYNAMIC_BASE`, `NX_COMPAT`, `GUARD_CF`, `authenticode_present` (`CN=VNG GROUP JSC`).

### 2.3. Isolated Directory Requirements (`CONFIRMED`)

Module `IpzU` (`main.js:1237000..1249000`), `migration.js:514200..605000`, and `isNewInstall` (`main.js:2230102`) resolve paths across `appData`, `home`, and `temp`:

- **`appData`:** `ZaloData/` (`lockfile`, `Local State`, `Partitions/zalo`, `ftsm_<uid>.db`, `media/`, `config.json`, `migrate-config.json`), plus `zmeta.json` directly under `appData`.
- **`home`:** `Documents/Zalo Received Files/`, legacy check `AppData/Local/ZaloPC/`, and `AppData/Roaming/ZaloData/Partitions`.
- **`temp`:** `Zalo Temp/`, `zTemp/`, `zpc-auc/`.
- **Mandatory Pre-Seed Files:** Before first launch of a new profile, write `AppData/Roaming/zmeta.json` (`{"failedMigrateCount":0}`) and `AppData/Roaming/ZaloData/migrate-config.json` (`{"zalopc_m_c":true}`) so `migration.js` never probes legacy paths in the host `%USERPROFILE%`.

---

## 3. Platform-by-Platform Feasibility Assessment

### 3.1. Windows 10 / 11 (`PARTIALLY_SUPPORTED` overall; `SUPPORTED_AND_TESTED` for Core Isolation)

- **Mechanism:** `CreateProcessW` with `bInheritHandles = FALSE`, `CREATE_UNICODE_ENVIRONMENT | CREATE_NEW_PROCESS_GROUP`, and per-process environment block overriding `USERPROFILE`, `APPDATA`, `LOCALAPPDATA`, `TEMP`, `TMP`, `HOMEDRIVE`, and `HOMEPATH` into `<data_dir>\Profiles\<slug>_<id>\`. Dynamic version discovery locates `%LOCALAPPDATA%\Programs\Zalo\Zalo-*\Zalo.exe` (preferring highest semver with valid `CN=VNG GROUP JSC` Authenticode signature).
- **Verified Capabilities (Historical Baseline + New Architecture Targets):**
  - Historical `multi-zalo-desktop` baseline ran **3 concurrent isolated Zalo `26.10.20` instances alongside 1 untouched primary Zalo instance (4 simultaneous windows)** with `verify-isolation` passing `100%` and zero byte changes to `%APPDATA%\ZaloData`.
  - Live `desktop-instance-manager` PoC verification (`scripts/run-zalo-poc.ps1`) is evaluated in [poc-and-test-report.md](../testing/poc-and-test-report.md).
- **Known Limitations & `BLOCKED` Subsystems:**
  1. **Concurrent Voice/Video Calls (`BLOCKED`):** `ZaloCall.exe` (`main.js:311822,314800`) binds hardcoded global named pipes `\\.\pipe\PipeZCallSend` and `\\.\pipe\PipeZCallRecv`. Secondary instances catch `EADDRINUSE` (`serverRecv address in use` / `serverSend address in use`) and stay running without crashing, but only one instance can hold the global call IPC server at a time without kernel pipe virtualization (rejected per [Repository Comparison](./repository-comparison.md#24-repo-d-sandboxie-plussandboxie-f23a6cfd50f506413634e8f2906bfc2888d2b1c3)).
  2. **Notifications (`BLOCKED` in multi-account live test; Shared AUMID):** `main.js:1063016,2720673` uses Electron `Notification` with shared `AppUserModelId("com.vng.zalo")`, grouping Windows toast notifications under one app identity.
  3. **Folder Redirection Hazard:** If `HKCU\...\User Shell Folders\AppData` is redirected (e.g., to a UNC share), `SHGetKnownFolderPath(FOLDERID_RoamingAppData)` ignores `%USERPROFILE%` and breaks isolation; blocked cleanly at preflight.
  4. **Windows Smart App Control (`VerifiedAndReputablePolicyState = 1`):** Blocks unsigned compiled Rust/Tauri PEs locally unless Authenticode-signed or invoked via the Signed-Host Bridge (`powershell.exe` + `node.exe`).
  5. **`zalo://` Deep Links:** System protocol handler invokes the default OS installation outside per-profile isolation.

### 3.2. macOS — Apple Silicon & Intel (`NOT_TESTED`)

- **Architectural Evidence (`app.asar` + MIT `ducngfi/zalo-multi-accounts`):**
  - `main.js:21950` uses `~/Library/Application Support/...` on `"darwin"`; `migration.js:600654` sets `userData` inside `app.getPath("appData")` (Cocoa `NSApplicationSupportDirectory`).
  - `main.js:314463,982174` invokes `ZaloHelper.app/Contents/MacOS/ZaloCall` and `ZaloCap` inside `/Applications/Zalo.app`. Modifying the `.app` bundle breaks macOS Gatekeeper / Notarization / Hardened Runtime.
  - Launching `/usr/bin/open -n -a /Applications/Zalo.app --env CFFIXED_USER_HOME=<profile_home>` redirects `CFFIXED_USER_HOME` inside CoreFoundation so Electron stores `ZaloData` at `<profile_home>/Library/Application Support/ZaloData`, while symlinked `Library/Keychains` preserves Keychain DPAPI-equivalent encryption under the active macOS login user.
  - Process tracking reads `<profile_home>/Library/Application Support/ZaloData/SingletonLock` symlink (`<host>-<pid>`) paired with `NSRunningApplication` bundle ID validation (`com.vng.zalo` / Zalo bundle ID).
- **Critical Unverified macOS Risk (`CONFIRMED` in `main.js:311822,314800`):**
  - `main.js` sets `y = "/tmp/socketzalosend2021"` and `v = "/tmp/socketzalorecv2021"`. On `EADDRINUSE`, macOS/POSIX code executes **`fs.unlink(v, ...)` and re-binds**! Unlike Windows (where `EADDRINUSE` leaves the first instance's pipe intact), launching a second Zalo instance on macOS may unlink and hijack `/tmp/socketzalorecv2021` from instance #1 during call initialization.
- **Feasibility Verdict:** **`NOT_TESTED`**. Physical Apple Silicon and Intel hardware verification is mandatory before claiming macOS support.

### 3.3. Ubuntu LTS — 22.04 / 24.04 (`UNSUPPORTED` Official Zalo; `EXPERIMENTAL` Fixture & Local AppImage)

- **Official VNG Zalo Desktop (`UNSUPPORTED`):**
  - Direct inspection of `main.js:314392` (`ZaloCall`) and `main.js:981878` (`ZaloCap`) confirms only two runtime branches exist: `"win32" === process.platform` and an `else` branch referencing macOS `ZaloHelper.app`. No official Linux binaries, `.node` addons, or VNG Linux installer exist.
  - Community Linux ports (`VN-Linux-Family/zalo-for-linux`) extract and patch proprietary macOS `app.asar` — legally ineligible for bundling or redistribution.
- **Synthetic Fixture & User-Supplied Local AppImage (`EXPERIMENTAL`):**
  - `dim-platform-linux` supports `HOME`, `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `XDG_CACHE_HOME`, and `TMPDIR` isolation plus optional `bwrap` (`bubblewrap`) sandboxing strictly for `dim-test-fixture` / `fixture-app.mjs` and user-supplied local Electron/AppImage paths.
  - **Ubuntu 23.10+/24.04 AppArmor Restriction:** Unprivileged user namespaces are restricted by default (`kernel.apparmor_restrict_unprivileged_userns = 1`), blocking Chromium's `chrome-sandbox` (`--no-sandbox` workaround documented in `zalo-for-linux`) and potentially restricting `bwrap` unless permitted by an AppArmor profile.

---

## 4. Summary Feasibility Matrix

| Platform | Official VNG Binary Exists? | Multi-Instance Core Status | Concurrent Voice/Video Calls | Primary Technical Constraint |
| :--- | :--- | :--- | :--- | :--- |
| **Windows 10/11 (x64)** | Yes (`Zalo.exe`, PE32 x86) | **`PARTIALLY_SUPPORTED`** (`SUPPORTED_AND_TESTED` for isolation/launch/stop/backup) | **`BLOCKED`** (`\\.\pipe\PipeZCallRecv` / `PipeZCallSend`) | Smart App Control (`VerifiedAndReputablePolicyState = 1`); Folder Redirection check; shared AUMID |
| **macOS (Apple Silicon & Intel)** | Yes (`Zalo.app`) | **`NOT_TESTED`** | Likely **`BLOCKED`** (`/tmp/socketzalorecv2021` `fs.unlink` hazard) | Needs physical Mac test for `CFFIXED_USER_HOME`, Keychain prompt, and `/tmp/socket*` collision |
| **Ubuntu LTS (22.04 / 24.04)** | **No** | **`UNSUPPORTED`** (Official Zalo) / **`EXPERIMENTAL`** (Fixture & user-supplied local AppImage) | **`UNSUPPORTED`** | No VNG Linux release; AppArmor userns restriction on Ubuntu 24.04 |

**Related Documents:** [os-readiness.md](../compatibility/os-readiness.md) | [environment-readiness.md](./environment-readiness.md) | [repository-comparison.md](./repository-comparison.md) | [adr-001-hybrid-isolation-architecture.md](../architecture/adr-001-hybrid-isolation-architecture.md)
