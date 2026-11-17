# `ak-xia --compare` Architectural Analysis: `ducngfi/zalo-multi-accounts` vs `dim-platform-macos`

**Date:** `2026-11-17`  
**Target Reference:** [`https://github.com/ducngfi/zalo-multi-accounts`](https://github.com/ducngfi/zalo-multi-accounts) (branch `main`, pinned commit `0b3c5aa4416760427d4721e0b0dac752b43e52be`, release `v1.2.0`)  
**Methodology:** `ak-xia` (`Recon -> Map -> Analyze -> Challenge`, `--compare` mode)  
**IP & Security Boundary:** External repository treated strictly as untrusted read-only reference data. License verified as permissive **`MIT`** (`LICENSE:1-22`, Copyright (c) 2026 `ducngfi`), compatible with our `MIT OR Apache-2.0` workspace (`deny.toml`). Zero source files copied; all Rust code in `crates/platform-macos` authored cleanly from scratch.

---

## 1. Recon — Source Manifest & Scope

- **macOS Swift Package (`macos/Package.swift`, Swift `5.9`, `.macOS(.v13)`):**
  - Core library: `macos/Sources/ZaloProfilesCore/{Layout,ZaloControl,ProfileManager,Backup,Disk,ProfileError}.swift`
  - Menu-bar SwiftUI app: `macos/Sources/ZaloSwitcher/{AppDelegate,AppModel,Menus,MainView,SettingsView}.swift`
  - CLI entrypoint: `macos/Sources/zalo-profiles/main.swift`
  - Unit/integration tests: `macos/Tests/ZaloProfilesCoreTests/{ProfileManagerTests,BackupTests,DemoTests}.swift`
- **Cross-Platform Reference (`windows/` & `docs/`):**
  - `docs/how-it-works.md`, `README.md`
  - `.NET 8` Windows implementation: `windows/src/ZaloProfiles.Core/{Layout,ZaloControl,ProfileManager,Backup,Native}.cs`

---

## 2. Map — Component-by-Component Mapping

| Source Component (`ducngfi`) | Source Mechanism | `desktop-instance-manager` Equivalent (`crates/platform-macos`) | Disposition |
| :--- | :--- | :--- | :--- |
| `Layout.swift:246-307` | Resolves real home via `getpwuid(getuid())->pw_dir`; finds `/Applications/Zalo.app` + `NSWorkspace` bundle ID fallback; stores profiles under `~/Library/Application Support/Zalo Profiles/<slug>/` | `ops::NativeMacOsSystemOps::user_home_dir` (`getpwuid_r` + `$HOME` fallback) + `bundle::discover_zalo_bundle` (`/Applications/Zalo.app`, `~/Applications/Zalo.app`, `mdfind` fallback) + `~/Library/Application Support/MultiZaloDesktop/Profiles/<slug>_<8hex>` | **Upgraded** (added pure-Rust `Info.plist`/`bplist00` + Mach-O slice + `PATH_MAX` validation) |
| `ZaloControl.swift:512-516` | Launches `/usr/bin/open -n -a <app> --env CFFIXED_USER_HOME=<profile_home>` (omits `HOME`, `TMPDIR`, `--args`) | `layout::build_open_launch_args` (`open -n -a <bundle> --env CFFIXED_USER_HOME=<dir> --env HOME=<dir> --env TMPDIR=<safe_tmp> --env DIM_PROFILE_ID=<id> --args --user-data-dir=<zalo_data>`) | **Hardened** (full env isolation + `SUN_LEN`-safe `TMPDIR` + child ownership argv tag) |
| `ZaloControl.swift:434-444` | Reads `SingletonLock` (`<host>-<pid>`), probes `kill(pid, 0)`, checks `NSRunningApplication.bundleIdentifier == "com.vng.zalo"` | `process::resolve_owned_profile_tree` (`SingletonLock` / `lockfile` / `.runtime-pid` cross-checked against `ps -axo pid=,ppid=,uid=,stat=,rss=,args=` for UID, zombie state, and profile/bundle identity) | **Hardened** (recycled/foreign PID defense + full descendant PID tree + RSS calculation) |
| `ProfileManager.swift:286-373` | **Main-Slot Swapping (`makeMain` / `switchTo`):** Quits primary Zalo and swaps live `~/Library/Application Support/ZaloData` via `renamex_np(..., RENAME_EXCL)` | **Rejected by design:** Primary `~/Library/Application Support/ZaloData` is strictly read-only and never mutated (`DimError::ProtectedPrimaryPath` + `DimError::ProtectedPrimaryProcess`) | **Rejected** (eliminates primary session corruption risk) |
| `ProfileManager.swift:390-405` & `L210-215` | Unconditionally symlinks `Library/Keychains`, `Downloads`..`Music`; `remove()` moves folder to `~/.Trash` **with live symlinks still attached** | `layout::ensure_macos_user_symlinks_with_options` (configurable user/Keychain symlinks; forbids `Application Support`/`Preferences` symlinks) + `unlink_reparse_points_safe` before deletion | **Hardened** (mandatory symlink unlink prior to deletion) |
| `ZaloControl.swift:450-503` | `NSRunningApplication.terminate()`, checks `--type=renderer` + `lsof` on `Database/_production/`; sends `SIGTERM` after `3s` *only* if at login screen, else waits `60s` and throws | `process::stop_isolated_profile_tree` (`.quit-signal` -> `NSRunningApplication.terminate()` -> SQLite `-wal` flush window -> bounded `SIGTERM` escalation on verified isolated tree) | **Adapted** (resolves both login-screen hang and wedged renderer quit safely) |
| `Backup.swift:628-831` | Briefly stops running Zalo, takes APFS `clonefile(2)` snapshot, writes `backup.json` with file count + byte total only | `dim-profile-store::FileBackupManager` (strictly offline `.partial` staging -> per-file **SHA-256** `backup.json` -> restore strictly as new profile) | **Retained ours** (cryptographic integrity check) |

---

## 3. Analyze — Key macOS Contracts Uncovered

1. **Why `--user-data-dir` Alone Is Overwritten by Zalo (`bootstrap.js` -> `migration.js`):**
   - Official Zalo runs `main-dist/migration.js` *before* `app.requestSingleInstanceLock()`, calling `app.setPath("userData", path.join(app.getPath("appData"), "ZaloData"))`.
   - On macOS, Electron resolves `app.getPath("appData")` via Cocoa `NSSearchPathForDirectoriesInDomains(NSApplicationSupportDirectory, NSUserDomainMask, YES)`, which calls CoreFoundation `CFCopyHomeDirectoryURL()`.
   - Passing `--env CFFIXED_USER_HOME=<profile_dir>` to `/usr/bin/open -n` redirects `CFCopyHomeDirectoryURL()` into `<profile_dir>` without modifying `Zalo.app` or weakening Gatekeeper/SIP.
2. **Why Both `CFFIXED_USER_HOME` and `HOME` + `TMPDIR` Are Needed:**
   - Node.js `os.homedir()` reads `getenv("HOME")` and `os.tmpdir()` reads `getenv("TMPDIR")`, bypassing CoreFoundation's `CFFIXED_USER_HOME`. Passing only `CFFIXED_USER_HOME` leaks Node-layer temp folders (`Zalo Temp/`, `zTemp/`, `zpc-auc/`) into the host user's shared temp directory.
3. **Why `SUN_LEN` (`104` bytes) Breaks Naive `TMPDIR=<profile_dir>/tmp`:**
   - Darwin `<sys/un.h>` limits `sockaddr_un.sun_path` to `104` bytes (`103` UTF-8 bytes + NUL).
   - Chromium's `ProcessSingleton` binds `SingletonSocket` at `<TMPDIR>/.org.chromium.Chromium.XXXXXX/SingletonSocket` (`+46` bytes). Any `TMPDIR` exceeding `54` bytes risks overflowing `103` bytes (`ENAMETOOLONG` / Chromium `CHECK` crash).
   - Our `layout::resolve_effective_tmpdir` uses `<profile_dir>/tmp` when `<= 54` bytes and automatically falls back to `/tmp/dim-zalo-<uid>-<id>` (`26` bytes, `0o700`, non-symlink, UID-verified) for longer profile paths.
4. **Why macOS Zalo Hangs at the QR Login Screen on `⌘Q` (`NSRunningApplication.terminate()`):**
   - Empirical findings in `ducngfi` (`ZaloControl.swift:463-480`, `README.md:1688`) show that when no account is logged in, `terminate()` closes the renderer window while the main Electron browser process hangs indefinitely until sent `SIGTERM`. Our bounded `terminate()` -> poll -> isolated `SIGTERM` escalation handles both logged-in WAL flushes and login-screen hangs.

---

## 4. Challenge — Security & Stability Flaws in Source & Head-to-Head Matrix

### Flaws Identified in `ducngfi/zalo-multi-accounts` (All Mitigated in `dim-platform-macos`)

1. **Primary `ZaloData` Swap Hazard (`makeMain` / `switchTo`):** Moving live `~/Library/Application Support/ZaloData` risks corrupting or losing the user's primary Zalo session if interrupted or if `.zalo-profile-id` is cleared on logout.
2. **Unisolated `HOME` and `TMPDIR`:** Setting only `CFFIXED_USER_HOME` leaks Node `os.homedir()` and `os.tmpdir()` operations across profiles.
3. **No Code-Signature or Publisher Verification:** Launching any bundle matching `com.vng.zalo` without `codesign --verify --deep --strict` + `spctl` exposes users to trojanized local `.app` bundles.
4. **Live Symlinks Left Inside Deleted/Trashed Profiles:** Moving `<profile_home>` to `~/.Trash` without unlinking `Library/Keychains` and `Documents`/`Downloads` symlinks exposes host files to recursive cleaner tools.
5. **No External `db_path.location` Parsing on macOS:** `ProfileManager.swift` misses externalized Zalo DB folders configured in `config.json` (supported by `dim_core::detect_zalo_user_id`).
6. **Hung Quit When Logged-In Renderer Deadlocks:** Never escalating to `SIGTERM` if `--type=renderer` remains stuck leaves wedged instances unstoppable.
7. **Count/Size-Only Backup Validation:** Lacks per-file SHA-256 tamper/corruption verification.

### Head-to-Head Decision Matrix

| Decision | Source's Way (`ducngfi/zalo-multi-accounts`) | Our Way (`dim-platform-macos`) | Final Recommendation |
| :--- | :--- | :--- | :--- |
| **Primary Session Safety** | Swaps `~/Library/Application Support/ZaloData` via `renamex_np` | Strictly untouched (`ProtectedPrimaryPath` + `ProtectedPrimaryProcess`) | **Keep Our Way** |
| **Launch Invocation** | `open -n -a <app> --env CFFIXED_USER_HOME=<dir>` | `open -n -a <bundle> --env CFFIXED_USER_HOME=<dir> --env HOME=<dir> --env TMPDIR=<safe_tmp> --env DIM_PROFILE_ID=<id> --args --user-data-dir=<zalo_data>` | **Keep Our Way** |
| **`SUN_LEN` (104B) Socket Safety** | Uses shared OS `TMPDIR` (leaks temp files across profiles) | `<profile_dir>/tmp` when `<= 54B`, else `0o700` UID-verified `/tmp/dim-zalo-<uid>-<id>` (`26B`) | **Adopted Hybrid Short-TMP** |
| **Binary Signature Trust** | None (`NSWorkspace` lookup only) | Pure-Rust `Info.plist`/`bplist00` + Mach-O check + `codesign --verify --deep --strict` + `codesign -dv` + `spctl` (`DimError::UnverifiedBinary`) | **Keep Our Way** |
| **PID Ownership & Recycled PIDs** | `SingletonLock` + `kill(pid, 0)` + `NSRunningApplication.bundleIdentifier` | `SingletonLock` (`lstat`) + `ps` UID, zombie filter, `--user-data-dir`/bundle match + full child tree | **Combined Best of Both** |
| **Symlink Cleanup on Delete** | Leaves symlinks intact when trashing profile | Mandatory `unlink_reparse_points_safe` (`symlink_metadata`) before directory removal | **Keep Our Way** |
| **Concurrency Protection** | UI-level serial calls only | In-process `Mutex<HashSet<String>>` + cross-process `<profile_dir>/tmp/.op-lock` | **Keep Our Way** |
