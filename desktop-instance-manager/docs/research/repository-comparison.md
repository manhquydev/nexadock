# Reference Repository & Ecosystem Comparison (`ak-xia --compare`)

This comparative study applies the `ak-xia --compare` methodology (`Recon -> Map -> Analyze -> Challenge`) against four open-source reference repositories, one commercial closed-source service, and the cross-platform Rust/OS isolation ecosystem. Every technical assertion is explicitly labeled:

- **`CONFIRMED`**: Directly observed in repository source files, commit metadata, license files, or local binary/environment checks.
- **`AUTHOR CLAIM`**: Stated in project README, documentation, or website without independent runtime verification.
- **`INFERENCE`**: Engineering deduction derived from observed code and documented OS/Electron semantics.
- **`UNVERIFIED`**: Closed-source internals or platform behavior not yet executed on physical hardware.

> **IP & License Boundary:** All fetched repositories and websites were inspected strictly read-only. No source code from unlicensed (`kemonra/ZaloMulti`), copyleft (`sandboxie-plus/Sandboxie`), or proprietary VNG (`app.asar`) artifacts is copied into `desktop-instance-manager`.

---

## 1. Source Manifest

| Ref | Repository / Target | Pinned Commit / Version | Primary Stack | License | Classification |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Repo A** | [`kemonra/ZaloMulti`](https://github.com/kemonra/ZaloMulti) | `c71e7f2209647721f299a1ef464316d6de2502e1` | PowerShell + WPF (`ZaloMulti.ps1`, `.xaml`, `.bat`) | **None declared** (`All rights reserved` by default) | Unsafe Windows launcher prototype |
| **Repo B** | [`ducngfi/zalo-multi-accounts`](https://github.com/ducngfi/zalo-multi-accounts) | `0b3c5aa4416760427d4721e0b0dac752b43e52be` | Swift (`macos/`) + C# .NET (`windows/`) | **`MIT`** | Well-engineered dual-OS profile manager (main-slot swap + isolated) |
| **Repo C** | [`VN-Linux-Family/zalo-for-linux`](https://github.com/VN-Linux-Family/zalo-for-linux) | `bbb6f99e4fe5ead55895825ebc5387181ebf21c5` | Node.js patcher + 12 Rust N-API crates + Electron AppImage | **`BSD-3-Clause`** (repo scripts/addons) + **PROPRIETARY VNG** (extracted macOS `app.asar`) | **Category 2: Community port** (NOT official Linux Zalo) |
| **Repo D** | [`sandboxie-plus/Sandboxie`](https://github.com/sandboxie-plus/Sandboxie) | `f23a6cfd50f506413634e8f2906bfc2888d2b1c3` | C/C++ (`SbieDrv.sys` kernel driver, `SbieSvc.exe`, `SbieDll.dll`, Qt UI) | **`GPL-3.0`** (`LICENSE.Classic`) + commercial cert terms (`LICENSE.Plus`) | Kernel-mode Windows sandbox / IPC virtualization |
| **Ref E** | [`https://lza.lol/`](https://lza.lol/) | Site live `2026-11-17` (claims `ZaloMulti 3.0.1` / `zDesk Business Pro`) | Closed-source installers via `dl.lza.lol` | **Proprietary / Commercial** | Closed-source multi-account service (`@congtruongit`) |

---

## 2. Detailed Technical & Security Analysis

### 2.1. Repo A: `kemonra/ZaloMulti` (`c71e7f2209647721f299a1ef464316d6de2502e1`)

- **License Status (`CONFIRMED`):** No `LICENSE` file exists in the 25-file tree; GitHub API returns `license: null`. **Code cannot be copied or reused.**
- **Core Mechanism (`CONFIRMED`):** `Start-ZaloInstance` (`ZaloMulti.ps1:165-179`) overrides `USERPROFILE`, `APPDATA`, and `LOCALAPPDATA` on `ProcessStartInfo` pointing inside `C:\Zalo_Clone_Profiles\<name>`, then launches hardcoded `Zalo.exe`.
- **Internal Contradiction (`CONFIRMED`):** `lessons_learned.md` claims `"Fix: Use the PWA (Chromium App Mode) approach with --user-data-dir"`, whereas `ZaloMulti.ps1` actually runs desktop `Zalo.exe` with environment redirection.
- **Critical Flaws & Vulnerabilities (`CONFIRMED`):**
  1. **Destructive Global Kill (`BtnKillAll`):** `ZaloMulti.ps1:364-365` executes:
     ```powershell
     Get-Process Zalo | Stop-Process -Force -ErrorAction SilentlyContinue
     ```
     This forcibly kills **every** `Zalo` process on the machine—including the user's primary `%APPDATA%\ZaloData` session—without SQLite WAL (`ftsm_*.db-wal`) flush or graceful shutdown (`INFERENCE`: high risk of database corruption and unsaved message loss).
  2. **Weak Root-Directory Storage & ACLs:** `$Global:ProfileRoot = "C:\Zalo_Clone_Profiles"` (`ZaloMulti.ps1:28`) stores session cookies, SQLite message databases, and media directly under `C:\`, where default NTFS root inheritance exposes read/traverse permissions more broadly than per-user `%LOCALAPPDATA%` (`INFERENCE`: local multi-user exposure on shared Windows machines).
  3. **Plaintext PII (`phone.txt`):** `ZaloMulti.ps1:199,300` writes and reads phone numbers in plaintext `phone.txt` inside each profile folder.
  4. **Unisolated `TEMP`/`TMP`:** Only `USERPROFILE`, `APPDATA`, and `LOCALAPPDATA` are overridden (`ZaloMulti.ps1:177-179`). `TEMP` and `TMP` remain inherited, causing Zalo's `%TEMP%\Zalo Temp`, `%TEMP%\zTemp`, and `%TEMP%\zpc-auc` artifacts to collide across concurrent instances.
  5. **Destructive Shortcut Cleanup & Hardcoded Paths:** `New-AppShortcut` (`ZaloMulti.ps1:144,319`) recreates `Zalo *.lnk` shortcuts on Desktop; `$Global:ZaloPath = "C:\Users\$($env:USERNAME)\AppData\Local\Programs\Zalo\Zalo.exe"` (`ZaloMulti.ps1:27`) breaks if `USERNAME` differs from the profile directory or Zalo is installed elsewhere; `ZaloMulti.bat:5,7` hardcodes author path `C:\Users\truongit\ZaloMulti\ZaloMulti.ps1` with `-ExecutionPolicy Bypass`.
  6. **Mutable Directory Renames (`Add_LostFocus`):** `ZaloMulti.ps1:260` renames profile directories directly on UI focus loss, failing or corrupting open handles if the instance is running (`INFERENCE`).

---

### 2.2. Repo B: `ducngfi/zalo-multi-accounts` (`0b3c5aa4416760427d4721e0b0dac752b43e52be`)

- **License Status (`CONFIRMED`):** Permissive **`MIT`** (`LICENSE:1-22`). Concepts and interoperable conventions can be referenced with attribution.
- **macOS Implementation (`CONFIRMED`):**
  - Launches isolated instances via `/usr/bin/open -n -a /Applications/Zalo.app --env CFFIXED_USER_HOME=<profile_home>` (`macos/Sources/ZaloProfilesCore/ZaloControl.swift:104,153`). CoreFoundation resolves `CFFIXED_USER_HOME` for `NSHomeDirectory` / `NSApplicationSupportDirectory`, routing Zalo's `appData` to `<profile_home>/Library/Application Support/ZaloData` while keeping VNG's signed/notarized `.app` bundle unmodified.
  - Detects running instances by reading Chromium's `SingletonLock` symlink (`<host>-<pid>`, `ZaloControl.swift:24-30`) and confirming `NSRunningApplication(processIdentifier: pid)?.bundleIdentifier == ZaloApp.bundleID`.
  - Symlinks `Library/Keychains` plus `Downloads`, `Desktop`, `Documents`, `Pictures`, `Movies`, `Music` from the real user home (`ProfileManager.swift:373-378`) so macOS Keychain (`Zalo Safe Storage`) encryption keys remain accessible.
  - Graceful termination uses `NSRunningApplication.terminate()` and inspects whether an account DB is open (`hasAccountOpen`, `ZaloControl.swift:44-81`) before escalating.
- **Windows Implementation (`CONFIRMED`):**
  - **Folder Redirection Guard:** `SystemZaloControl.IsolationProblem()` (`ZaloControl.cs:101-108`) checks `HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\User Shell Folders\AppData` using `RegistryValueOptions.DoNotExpandEnvironmentNames`, refusing isolated launch if redirected away from `%USERPROFILE%\AppData\Roaming`.
  - **Graceful Shutdown (`--z-update-force-quit`):** `ZaloControl.cs:28,110-115` launches the same isolated env with `--z-update-force-quit`, letting Electron's single-instance handoff trigger Zalo's native updater-quit sequence rather than killing processes (`"Zalo is never killed"`, `ZaloControl.cs:60`).
  - **Duplicate Account ID Guard:** `ProfileManager.ZaloUserId()` (`ProfileManager.cs:419-430,481-514`) reads `config.json -> db_path.location` (supporting external message folders) and scans `Database/_production/<6+ digits>` directories, refusing simultaneous launch if two profiles share the same logged-in Zalo user ID (`ProfileManager.cs:365-393`).
  - **Atomic Backup & Non-Destructive Restore:** `Backup.cs:74,122-196` and `Backup.swift:99,145-216` refuse backup while running, write snapshots to `<name>.partial` with `backup.json` (file count + byte count), atomically promote on completion, clean abandoned `.partial` / `.restore-*` folders older than 12 hours, and **always** restore into a brand-new profile (never overwriting an existing profile).
  - **Junction Unlinking Before Deletion:** `ProfileManager.RemoveLinks()` (`ProfileManager.cs:166,415-433`) recursively finds `FileAttributes.ReparsePoint` junctions and deletes only the reparse points (`Directory.Delete(child, recursive: false)`) before sending the profile folder to Recycle Bin.
  - **Vietnamese Slug Folding:** `Fold()` (`ProfileManager.cs:456-477`, `ProfileManager.swift:499-515`) normalizes `đ`/`Đ` -> `d`/`D` and strips Unicode diacritics.
- **Comparison & Gaps vs `desktop-instance-manager` (`CONFIRMED` + `INFERENCE`):**
  1. *Main-Slot Swapping vs Always-Isolated:* Repo B maintains a "Main Slot" that moves live `%APPDATA%\ZaloData` in/out via `MoveFileEx` (`ProfileManager.cs:254-290`). Our architecture **never** moves, touches, or swaps the user's primary `%APPDATA%\ZaloData`, eliminating cross-slot rename failures and protecting primary data (`PROTECTED_PRIMARY_PROCESS`).
  2. *PID Discovery Mechanism:* Repo B parses `--user-data-dir` from process command lines (`ZaloControl.cs:113-130`). We use Windows Restart Manager (`rstrtmgr.dll`) against `<profile>\AppData\Roaming\ZaloData\lockfile` as the authoritative OS lock owner query, keeping `--user-data-dir` only as secondary fallback.
  3. *Backup Integrity:* Repo B verifies `files` count and `bytes` total (`Backup.cs:196`); our ADR-004 upgrades `backup.json` to include a per-file **SHA-256 manifest**.
  4. *Junction Isolation Trade-off:* Repo B unconditionally creates junctions to `Desktop`, `Documents`, `Downloads`, `Music`, `Pictures`, `Videos`. Our design isolates `Documents/Zalo Received Files/` inside each profile by default (making host junctions opt-in) to avoid unintended cross-profile exposure of received files.

---

### 2.3. Repo C: `VN-Linux-Family/zalo-for-linux` (`bbb6f99e4fe5ead55895825ebc5387181ebf21c5`)

- **License & Legal Classification (`CONFIRMED`):**
  - Repository scripts, JS patches, plugins, and 12 Rust N-API crates (`Cargo.toml:1-15`) are licensed **`BSD-3-Clause`** (`LICENSE:1-29`).
  - However, the application payload is extracted from VNG's official macOS `.dmg` (`docs/dev/architecture.md:10-14`, `scripts/extract.js:39-100`, `.github/workflows/build.yml`) containing **proprietary VNG `app.asar`**.
  - **Category 2 Classification (`CONFIRMED`):** `README.md:16` explicitly describes it as *"An unofficial, community-driven port of the Zalo desktop application for Linux only, created by repackaging the official macOS client into a standard AppImage"*. It is **not** an official VNG Linux client.
- **Technical Insights (`CONFIRMED`):**
  - Demonstrates that Linux support requires rewriting 12+ native addons (`db-cross-v4`, `file-utilities`, `os-sys-info`, `zcall-audio`, `zocr`, etc.) in Rust N-API and patching `app.asar` JS (`scripts/patches/*`).
  - Documents Ubuntu 23.10+/24.04 AppArmor restrictions on unprivileged user namespaces breaking Electron's `chrome-sandbox`, forcing `--no-sandbox` or AppArmor overrides (`docs/user/known-issues.md:9,20`).
- **Legal & Architectural Blocker (`CONFIRMED` + `INFERENCE`):**
  - `desktop-instance-manager` **cannot** bundle, download, extract, patch, or redistribute VNG's proprietary `app.asar`. Patching `app.asar` also breaks binary integrity and auto-updates. Consequently, official Linux Zalo remains `UNSUPPORTED`, while local XDG/`bwrap` support in `dim-platform-linux` is strictly `EXPERIMENTAL` for test fixtures or user-supplied local AppImages.

---

### 2.4. Repo D: `sandboxie-plus/Sandboxie` (`f23a6cfd50f506413634e8f2906bfc2888d2b1c3`)

- **License Status (`CONFIRMED`):** **`GPL-3.0`** (`LICENSE.Classic:1-25`) plus contributor/certificate terms for Sandboxie-Plus (`LICENSE.Plus:1-45`). Copyleft GPL-3.0 is incompatible with our `MIT OR Apache-2.0` workspace (`deny.toml`).
- **Kernel & IPC Virtualization Architecture (`CONFIRMED`):**
  - Kernel driver `SbieDrv.sys` (`Sandboxie/core/drv/driver.c:173`, `ipc.c:148-278,671`) hooks Windows Object Manager / IPC types (`Event`, `Mutant`, `Section`, `SymLink`, `Directory`) and `\??\pipe\*` namespaces per sandbox box.
  - User-mode injected DLL `SbieDll.dll` (`Sandboxie/core/dll/file_pipe.c:424-758`) intercepts `NtCreateNamedPipeFile` and `NtCreateFile`, rewriting pipe paths via `File_GetBoxedPipeName` (`\Device\NamedPipe\<BoxIpcPath>\...`) and proxying restricted pipes through `SbieSvc.exe`.
- **Why It Solves `PipeZCallRecv` in Theory (`INFERENCE`):**
  - Official `ZaloCall.exe` binds global named pipes `\\.\pipe\PipeZCallSend` and `\\.\pipe\PipeZCallRecv` (`multi-zalo-desktop/docs/research/instance-behavior.md:112-114`). Under Sandboxie's per-box `\Device\NamedPipe\` prefix rewriting, two sandboxed instances receive disjoint pipe namespaces and avoid `EADDRINUSE`.
- **Why Unsuitable to Build or Embed (`CONFIRMED`):**
  1. Requires Administrator privileges, kernel driver installation (`SbieDrv.sys`), and EV/WHQL kernel driver signing.
  2. Uses invasive system-call / DLL hooking incompatible with Windows Smart App Control (`VerifiedAndReputablePolicyState = 1`) stability and non-elevated `asInvoker` design.
  3. `GPL-3.0` / `LICENSE.Plus` licensing conflicts with our permissive dual license (`MIT OR Apache-2.0`).

---

### 2.5. Reference E: `https://lza.lol/` (`ZaloMulti 3.0.1` / `zDesk Business Pro`)

- **Claims (`AUTHOR CLAIM`):** Live page (`2026-11-17`, status `200`, `47,513` bytes HTML) advertises *"ZaloMulti — Nhiều tài khoản Zalo PC trên Windows, macOS & Linux"* (`v3.0.1`, updated `10/11/2026`), separated data/login/shortcuts, free tier (`2` accounts free, mockup showing `2 / 5` limit, paid upgrades from `10.000₫`, `zDesk Business Pro` cloud backup/proxy), distributed via `dl.lza.lol` with login dashboard and Telegram contact `@congtruongit` (matching `kemonra/ZaloMulti`'s `README.md` author handle).
- **Unverified Internals (`UNVERIFIED`):** Closed-source binaries; unknown whether macOS/Linux builds wrap `zalo-for-linux` or browser shells, what telemetry/license calls reach `lza.lol`, or how credentials/backups are handled.
- **Decision (`CONFIRMED`):** Reject any dependency on external auth servers, account quotas, closed-source telemetry, or cloud relays.

---

## 3. Cross-Platform Rust & OS Isolation Ecosystem Survey

| Mechanism / Crate | Platform | Role & Characteristics | Suitability for `desktop-instance-manager` |
| :--- | :--- | :--- | :--- |
| **Tauri 2** | Win / macOS / Linux | Rust backend + OS native WebView (WebView2 / WKWebView / WebKitGTK), small footprint vs bundling Chromium | **Selected** for native app shell, paired with SAC Signed-Host Bridge on Windows 11 (`INFERENCE`: avoids bundling second Electron runtime) |
| **`sysinfo` crate** | Win / macOS / Linux | Cross-platform `/proc`, `libproc`, and Win32 process enumeration (`PID`, `parent`, `cmd`, memory) | Useful cross-platform process metadata helper; **not** a substitute for `rstrtmgr.dll` lockfile ownership on Windows |
| **Win32 `rstrtmgr.dll` + `CreateToolhelp32Snapshot`** | Windows | Queries exact OS file-lock owner PID on `lockfile`, then walks `ParentProcessId` tree | **Selected** primary ownership detector on Windows (`CONFIRMED` in `multi-zalo-desktop` PoC/IT) |
| **Windows Job Objects (`AssignProcessToJobObject`)** | Windows | Kernel process-tree grouping (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`) | **Rejected as primary control** (`INFERENCE`): `KILL_ON_JOB_CLOSE` kills Zalo windows if the manager UI exits, whereas users expect Zalo instances to persist independently; optional only for transient tests |
| **`CFFIXED_USER_HOME` + `open -n`** | macOS | CoreFoundation home override + LaunchServices multi-instance flag | **Selected architecture** for `dim-platform-macos` (`CONFIRMED` in Repo B source; `NOT_TESTED` on physical Mac here) |
| **XDG Base Dir + `bwrap` (`bubblewrap`)** | Linux | `HOME`/`XDG_CONFIG_HOME`/`XDG_DATA_HOME`/`XDG_CACHE_HOME` + optional user/mount namespace sandbox | **Selected (`EXPERIMENTAL`)** for `dim-platform-linux` test fixture & user-supplied local AppImages; note Ubuntu 23.10+/24.04 AppArmor `userns` constraint (`CONFIRMED` in Repo C) |

---

## 4. Challenge Matrix (`ak-xia` Phase 4)

| Architectural Concern | Reference Pattern (Source's Way) | Our Pattern (`desktop-instance-manager`) | Challenge Verdict & Rationale |
| :--- | :--- | :--- | :--- |
| **Primary Account Slot** | Repo B swaps live `%APPDATA%\ZaloData` via `MoveFileEx`; Repo A kills all `Zalo` processes | Primary `%APPDATA%\ZaloData` is **strictly read-only/never touched** (`PROTECTED_PRIMARY_PROCESS`) | **Reject swapping & global kill.** Zero risk to the user's primary account data or running session. |
| **PID Ownership Detection** | Repo A uses `Get-Process Zalo`; Repo B parses `--user-data-dir` | Windows `rstrtmgr.dll` on `lockfile` (primary) + `--user-data-dir` fallback; macOS `SingletonLock` symlink | **Upgrade to OS lockfile ownership.** Survives argument changes and handles stale crash lockfiles accurately. |
| **Host Folder Junctions** | Repo B unconditionally creates junctions for `Desktop`..`Videos` | Keep `Documents/Zalo Received Files` isolated inside profile by default; unlink any optional junctions before delete | **Minimize cross-profile leakage** while preserving Repo B's safe `ReparsePoint`-first unlink invariant. |
| **Backup Verification** | Repo B verifies file count + byte size in `backup.json` | Offline-only `.partial` staging + `backup.json` with **per-file SHA-256 manifest** + restore-to-new-profile-only | **Strengthen integrity verification** against silent bit flips or partial copies. |
| **Linux & Call Pipes** | Repo C repackages macOS `app.asar`; Repo D hooks kernel pipes (`GPL-3.0`) | Official Linux `UNSUPPORTED`; concurrent Windows calls `BLOCKED` (honestly documented, zero kernel hooks/ASAR patches) | **Strict legal & security boundary.** Never redistribute VNG IP or ship unsigned/GPL kernel drivers. |
| **Windows SAC Execution** | Repo A uses `-ExecutionPolicy Bypass` + hardcoded `truongit` path; Repo B ships compiled `.exe` | Rust/Tauri 2 workspace + CI signing (Gate F) + **Signed-Host Bridge** (`powershell.exe` + `node.exe` + `127.0.0.1` UI) | **Compliant on SAC-enforcing Windows 11 today** while keeping a clean path to signed native Tauri releases. |

**Related Documents:** [environment-readiness.md](./environment-readiness.md) | [platform-feasibility.md](./platform-feasibility.md) | [adr-001-hybrid-isolation-architecture.md](../architecture/adr-001-hybrid-isolation-architecture.md) | [adr-003-process-ownership-and-graceful-shutdown.md](../architecture/adr-003-process-ownership-and-graceful-shutdown.md) | [adr-004-backup-restore-and-data-safety.md](../architecture/adr-004-backup-restore-and-data-safety.md)
