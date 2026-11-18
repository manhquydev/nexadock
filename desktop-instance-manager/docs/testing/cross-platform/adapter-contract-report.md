# Adapter & Public Interface Behavioral Contract Report

**Branch:** `test/cross-platform-reliability`  
**Baseline Commit:** `a5feb4a52079` (`main`)  
**Host Verification Run:** `2026-11-17T17:07:34Z` (Windows 11 `10.0.26200`, `rustc 1.100.0`)  
**Evidence Artifacts:**
- `plans/reports/reliability/windows-host-execution-evidence.json` (`CT-01` .. `CT-10`, `XP-WIN-*`, `XP-MAC-*`, `XP-LNX-*`)
- `plans/reports/reliability/defect-reproductions.json` (`DEFECT-01` .. `DEFECT-10`)

---

## 1. Contract Testing Principles & Scope

Per Mission Section 6, platform adapters are **not** forced to share identical internal OS mechanisms (`CreateProcessW` + `rstrtmgr.dll` on Windows, `open -n --env CFFIXED_USER_HOME` on macOS, `XDG_*` + optional `bwrap` on Ubuntu). Instead, `tests/cross-platform/src/suites/adapter_contracts.rs` (`CT-01` .. `CT-10`) and `cross_platform.rs` (`XP-*`) enforce **behavioral parity across supported public contracts** at four architectural boundaries:

1. **Core Rust Traits (`crates/core/src/traits.rs`):** `ApplicationLocator`, `InstanceLauncher`, `InstanceMonitor`, `PlatformCapabilities`, `ProfileStore`, `BackupManager`, `RecoveryManager`.
2. **OS Platform Adapters:** `WindowsPlatformAdapter` (`crates/platform-windows`), `MacOsPlatformAdapter` (`crates/platform-macos`), `LinuxPlatformAdapter` (`crates/platform-linux`).
3. **CLI & Signed-Host Automation Contract:** Native Rust CLI (`fixtures/test-desktop-app/src/bin/dim-cli.rs`) vs Windows PowerShell Signed-Host Bridge (`scripts/dim-host.ps1`).
4. **Tauri Desktop IPC Contract:** `apps/desktop/src-tauri/src/commands.rs`.

---

## 2. Contract Suite Execution Summary (`CT-01` .. `CT-10`)

| Case ID | Contract Dimension | Scope | Verdict | Duration | Key Evidence / Finding |
| :--- | :--- | :--- | :--- | ---: | :--- |
| `CT-01` | `PlatformCapabilities` & honest `SupportStatus` reporting | Win + Mac + Linux | `PASS` | `< 1 ms` | All 3 adapters match `PlatformKind`, report non-empty `limitations` + `diagnostics`, mark `concurrent_calls_status` as `blocked`/`unsupported`, macOS `official_zalo_status = not_tested`, Linux `official_zalo_status = unsupported` & `appimage_status = experimental`. |
| `CT-02` | `ApplicationLocator::discover_installation` input/output schema | Win + Mac + Linux | `PASS` | `6 ms` | Missing/invalid custom install roots return `Ok(ApplicationInstallation { is_valid: false, error: Some(..), .. })` (never panicking) and round-trip cleanly through `serde_json` (`signature_status`: Win/Mac `"NotFound"`, Linux `"NotApplicable"`). |
| `CT-03` | `InstanceLauncher::ensure_profile_layout` idempotency & `.zalo-profile-id` | Win + Mac + Linux | `PASS` | `33 ms` | Double invocation on all 3 adapters is idempotent, writes `.zalo-profile-id` (`"cafebabe"`) at each OS's `ZaloData` root, and `unlink_profile_junctions` returns `Ok(0)` on clean trees. |
| `CT-04` | Exhaustive `DimError::code()` (22 variants) & `OperationOutcome<T>` | Core | `PASS` | `< 1 ms` | All `22` `DimError` enum variants map 1-to-1 onto `22` unique `UPPER_SNAKE_CASE` error codes and serialize cleanly via `OperationOutcome::from_err`. |
| `CT-05` | Unsupported operations (`UNSUPPORTED_PLATFORM`) & binary trust (`UNVERIFIED_BINARY`) | Win + Mac + Linux | `PASS` | `104 ms` | `LinuxPlatformAdapter::launch(OfficialZalo)` returns `UNSUPPORTED_PLATFORM` on all hosts; cross-OS `inspect_primary()` returns `UNSUPPORTED_PLATFORM`; unsigned `xf-desktop-fixture.exe` via `LaunchTarget::CustomExecutable` fails closed with `UNVERIFIED_BINARY` on Windows (`WinVerifyTrust`). |
| `CT-06` | Out-of-scope primary PID guard (`PROTECTED_PRIMARY_PROCESS`) | Win + Mac + Linux | `PASS` | `6 ms` | All 3 adapters check `protected_primary_pids` **before** entering `#[cfg(target_os = ...)]` kill syscalls and reject overlap on PID `42424` with `PROTECTED_PRIMARY_PROCESS`. |
| `CT-07` | Public CLI JSON output schema (`discover`, `status`, `diagnostics`, `verify-isolation`, `backups`) | CLI | `PASS` | `244 ms` | Verified required top-level JSON keys across all 5 read-only commands and exact `7`-check `IsolationVerificationReport` array schema. |
| `CT-08` | Differential CLI contract: 33..40 char profile slug parity | `dim-cli` vs `dim-host.ps1` | **`FAIL_DEFECT_REPRODUCED`** (`DEFECT-08`) | `1,991 ms` | Input `"Tai Khoan Kinh Doanh Khu Vuc Mien Nam So 1"` -> Rust slug `"tai-khoan-kinh-doanh-khu-vuc-mien-nam-so"` (`40` chars) vs PowerShell `"tai-khoan-kinh-doanh-khu-vuc-mie"` (`32` chars). |
| `CT-09` | Differential CLI contract: Restore duplicate `--name` & 64-char cap | `dim-cli` vs `dim-host.ps1` | **`FAIL_DEFECT_REPRODUCED`** (`DEFECT-08`) | `7,757 ms` | Duplicate `--name` returns `ok: false, error_code: "DUPLICATE_NAME"` in Rust vs silent `" #2"` rename (`ok: true`) in PowerShell; auto-restore of a 58-char profile caps at `64` chars in Rust vs overflows to `69` chars (`> MAX_PROFILE_NAME_LEN`) in PowerShell. |
| `CT-10` | Differential CLI contract: Corrupt/empty `profiles.json` & missing-arg error codes | `dim-cli` vs `dim-host.ps1` | **`FAIL_DEFECT_REPRODUCED`** (`DEFECT-08`) | `6,392 ms` | `0`-byte `profiles.json`: Rust `JSON_ERROR` (`ok: false`) vs PowerShell silent empty reset (`ok: true`); `{corrupt`: Rust `JSON_ERROR` vs PowerShell `UNHANDLED_EXCEPTION`; missing `create` arg: Rust `INVALID_NAME` vs PowerShell `MISSING_ARG`. |

---

## 3. Dimension-by-Dimension Behavioral Contract Audit

### 3.1 Input Schema & Validation Contract

| Input Surface | Canonical Rule (`dim-core` / `dim-profile-store`) | Windows Adapter | macOS Adapter | Linux Adapter | `scripts/dim-host.ps1` Parity |
| :--- | :--- | :--- | :--- | :--- | :--- |
| Profile `name` length | Trimmed `1..=64` Unicode scalar values (`MAX_PROFILE_NAME_LEN = 64`, `slug.rs:11`) | Aligned | Aligned | Aligned | Create/Rename enforce `1..=64`, **except** `restore` auto-suffix `" (Restored)"` overflows to `75` chars (`CT-09`, `DEFECT-08`) |
| Profile `name` characters | Control chars (`c.is_control()`) rejected (`INVALID_NAME`); Vietnamese NFC/NFD folded in slug | Aligned | Aligned | Aligned | Aligned on control chars; case-insensitivity uses `.NET OrdinalIgnoreCase` while Rust uses ASCII-only `eq_ignore_ascii_case` (`PM-04`, `DEFECT-10`) |
| Profile `id` format | Strictly 8 lowercase hex chars (`0-9a-f`) (`validate_profile_id_format`, `paths.rs:14-27`) | Aligned | Aligned | Aligned | Aligned |
| `directory_name` containment | Single Normal component, rejects `..`, `/`, `\`, `:`, Windows reserved device names (`CON`, `NUL`, `COM1`..`LPT9`), trailing `.`/space (`paths.rs:30-75`) | Aligned | Aligned | Aligned | Aligned |
| `LaunchTarget` tagged union | `#[serde(tag = "kind", rename_all = "snake_case")]`: `official_zalo`, `custom_executable`, `node_fixture` (`models.rs:151-173`) | Supports all 3 (`CustomExecutable` requires Authenticode signature in prod) | Supports all 3 (see `DEFECT-02` on `NodeFixture` args) | Rejects `OfficialZalo` (`UNSUPPORTED_PLATFORM`); supports `CustomExecutable` (`.AppImage`/ELF) & `NodeFixture` | Supports `--official`, `--app-path`, `--node-fixture` |

### 3.2 Output Schema Contract

- **Core Rust Models (`crates/core/src/models.rs`):**
  - `ApplicationInstallation`, `ProfileRecord`, `ProfilesDocument`, `ProfileRuntimeState`, `PrimaryInstanceStatus`, `IsolationVerificationReport`, `PlatformCapabilityReport`, `BackupManifest`, `BackupResult`, `RecoveryReport`, and `OperationOutcome<T>` all implement `Serialize + Deserialize + PartialEq + Eq` and round-trip deterministically (`CT-01`, `CT-02`, `CT-04`).
- **CLI Envelope (`dim-cli.rs` & `dim-host.ps1`):**
  - Top-level JSON responses use `"ok": bool` alongside command-specific payloads (`"data"`, `"profiles"`, `"backups"`, `"checks"`, etc.) and `"error_code"` + `"error"` on failure (`CT-07`).
  - Note: `OperationOutcome<T>` in `crates/core/src/error.rs:148-157` uses `"success": bool` / `"message": String`, whereas CLI commands in `dim-cli.rs` and Tauri `commands.rs` construct custom `serde_json::json!({ "ok": true, ... })` envelopes. Consumers must parse `"ok"` on CLI/IPC endpoints rather than `OperationOutcome::success`.
- **Tauri IPC `diagnostics_report` Matrix Duplication (Static Contract Finding):**
  - Location: `apps/desktop/src-tauri/src/commands.rs:465-494` vs `fixtures/test-desktop-app/src/bin/dim-cli.rs:580-591`.
  - `dim-cli diagnostics` emits a 1-element `platform_support_matrix` array for the active adapter, whereas `commands.rs::diagnostics_report` emits 4 hardcoded rows where element `[0]` dynamically switches `platform` label by `caps.platform` (`"Windows 10/11"`, `"macOS (Apple Silicon & Intel)"`, or `"Ubuntu / Linux"`) **while elements `[1]`, `[2]`, `[3]` unconditionally repeat macOS and Ubuntu rows**. On macOS or Linux builds, the Tauri UI receives duplicate platform rows in `platform_support_matrix`.

### 3.3 Canonical Error Code Contract (`22` Variants + Boundary Divergences)

`CT-04` verified all `22` canonical `DimError::code()` strings in `crates/core/src/error.rs:118-143`:

| # | `DimError` Variant | Canonical `code()` | Boundary Parity Status |
| ---: | :--- | :--- | :--- |
| 1 | `InvalidProfileName` | `INVALID_NAME` | Diverges on missing CLI arg: `dim-cli` emits `INVALID_NAME`, `dim-host.ps1` emits `MISSING_ARG` (`CT-10`) |
| 2 | `DuplicateProfileName` | `DUPLICATE_NAME` | Diverges on Vietnamese case (`PM-04`, `DEFECT-10`) and `restore --name` (`CT-09`, `DEFECT-08`) |
| 3 | `ProfileNotFound` | `NOT_FOUND` | Aligned |
| 4 | `ProfileRunning` | `PROFILE_RUNNING` | Bypassed if `ProfileStore::delete_profile` trait method is called directly (`PM-08`, `DEFECT-09`) |
| 5 | `ProfileIdentityMismatch` | `PROFILE_MARKER_MISMATCH` | Aligned across launch (`LC-05`) and backup (`DS-04`) |
| 6 | `ProtectedPrimaryProcess` | `PROTECTED_PRIMARY_PROCESS` | Aligned across all 3 adapters (`CT-06`, `LC-04`) |
| 7 | `ProtectedPrimaryPath` | `PROTECTED_PRIMARY_PATH` | Aligned in `dim-core`/`dim-profile-store`; note `commands.rs:42` emits non-canonical `[FORBIDDEN_DEST]` on backup destination inside `data_dir` |
| 8 | `DuplicateZaloUserId` | `DUPLICATE_ZALO_USER_ID` | Triggered prematurely on macOS/Linux multi-profile `NodeFixture` due to missing `--user-id` (`DEFECT-02`) |
| 9 | `InsufficientDiskSpace` | `INSUFFICIENT_DISK_SPACE` | Aligned (`DS-06`) |
| 10 | `PathTraversal` | `PATH_TRAVERSAL` | Aligned (`PM-07`, `DS-03`) |
| 11 | `FolderRedirectionBlocked` | `FOLDER_REDIRECTION_DETECTED` | Enforced on Windows (`XP-WIN-02`); `false` on macOS/Linux |
| 12 | `InstallNotFound` | `INSTALL_NOT_FOUND` | Aligned |
| 13 | `UnverifiedBinary` | `UNVERIFIED_BINARY` | Enforced via `WinVerifyTrust` on Windows (`CT-05`) |
| 14 | `NotRunning` | `NOT_RUNNING` | Aligned |
| 15 | `UnsupportedPlatform` | `UNSUPPORTED_PLATFORM` | Aligned (`CT-05`) |
| 16 | `BackupError` | `BACKUP_ERROR` | Aligned (`DS-04`) |
| 17 | `RestoreError` | `RESTORE_ERROR` | Aligned (`DS-05`) |
| 18 | `ProcessError` | `PROCESS_ERROR` | Aligned |
| 19 | `SchemaError` | `SCHEMA_ERROR` | Aligned on unsupported `schema_version` (`REL-06`) |
| 20 | `ConfirmationRequired` | `CONFIRM_REQUIRED` | Aligned (`PM-01`) |
| 21 | `Io` | `IO_ERROR` | Aligned (`PM-08`) |
| 22 | `Json` | `JSON_ERROR` | Diverges in `dim-host.ps1`: 0-byte `profiles.json` returns `ok: true`, corrupt JSON returns `UNHANDLED_EXCEPTION` (`CT-10`, `DEFECT-08`) |

### 3.4 Process Ownership & Lifecycle Contract Matrix

| Contract Capability | `WindowsPlatformAdapter` | `MacOsPlatformAdapter` | `LinuxPlatformAdapter` | Divergence / Defect ID |
| :--- | :--- | :--- | :--- | :--- |
| Primary lock mechanism | Win32 `RestartManager` (`rstrtmgr.dll`) + exclusive file-share probe + JSON PID fallback | `SingletonLock` symlink `<host>-<pid>` + JSON `lockfile` fallback + `kill(pid, 0)` | `SingletonLock` symlink `<host>-<pid>` + JSON `lockfile` fallback + `/proc/<pid>` | Aligned by design (OS-appropriate) |
| Stale `SingletonLock` symlink after crash | N/A (uses regular `lockfile`) | **BROKEN:** `singleton_lock.exists()` returns `false` on dangling symlink | **BROKEN:** `singleton_lock.exists()` returns `false` on dangling symlink | **`DEFECT-01`** (`XP-MAC-03`, `XP-LNX-02`) |
| `.runtime-pid` startup bridge | Written on launch; **ignored** unless `lockfile` is already exclusively locked (`adapter.rs:512-522`) | Written on launch (`adapter.rs:298`); **never read** in `inspect_profile` | Written on launch (`adapter.rs:298`); **never read** in `inspect_profile` | **`DEFECT-02`**, **`DEFECT-03`** (`LC-07`) |
| Post-spawn readiness polling | Polls up to `3,000 ms` (`25 x 120 ms`) (`adapter.rs:620-629`) | **None** — calls `inspect_profile` immediately after `spawn()` (`adapter.rs:317`) | **None** — calls `inspect_profile` immediately after `spawn()` (`adapter.rs:299`) | **`DEFECT-02`** |
| `NodeFixture` deterministic `--user-id` | Injects `--user-id 84<6-digits>` from FNV-1a hash of `profile.id` (`adapter.rs:600-607`) | **Missing** — defaults to `100001` across all profiles | **Missing** — defaults to `100001` across all profiles | **`DEFECT-02`** (`XP-MAC-03`) |
| Descendant process-tree tracking | Full BFS over `CreateToolhelp32Snapshot` (`expand_descendant_tree`) | None (`process_ids` contains only `[main_pid]`) | `/proc/*/stat` PPID scan (`collect_child_pids` — 1 level direct children, not recursive) | **`DEFECT-06`** (partial tree on Mac/Linux) |
| Graceful -> force `stop()` escalation | `WM_CLOSE` + `.shutdown-signal` -> `TerminateProcess` after `timeout_ms`, **but** gated by hardcoded `allowed_exe` list (`zalo*`, `zocr*`, `node.exe`, `dim-test-fixture*`) | `.shutdown-signal` + `SIGTERM` to `main_pid` only; **never escalates to `SIGKILL`** on hang | `.shutdown-signal` + `SIGTERM` to `process_ids` -> `SIGKILL` to `main_pid` only (children survive if they ignore `SIGTERM`) | **`DEFECT-06`** (`LC-09`) |
| `focus_or_restore` on running profile | Win32 `SetForegroundWindow` + `.focus-signal` fallback (`PASS` in `LC-06`) | `osascript` `Zalo` activation only (fails/ignores headless fixtures; never writes `.focus-signal`) | Silent no-op `Ok(())` (`adapter.rs:310-318`) | **`DEFECT-05`** |

### 3.5 Profile Status (`ProfileRuntimeState`) Field Parity

| Field | Windows | macOS | Linux | Contract Assessment |
| :--- | :--- | :--- | :--- | :--- |
| `running`, `main_pid`, `stale_lock_detected` | Populated (subject to `DEFECT-03` pre-lock race) | Populated (subject to `DEFECT-01` & `DEFECT-02`) | Populated (subject to `DEFECT-01` & `DEFECT-02`) | Requires `DEFECT-01..03` fixes |
| `process_ids`, `process_count` | Full recursive process tree (`2` PIDs per fixture with worker) | Single `[main_pid]` only (`1` PID even with child tree) | Main + direct `/proc` children (non-recursive) | macOS/Linux under-report multi-level Electron helper trees |
| `working_set_bytes` | Real `K32GetProcessMemoryInfo` sum across `process_ids` | Hardcoded `0` (`adapter.rs:219`) | Hardcoded `0` (`adapter.rs:219`) | Telemetry gap on macOS/Linux (`ps -o rss=` / `/proc/<pid>/statm` available) |
| `disk_usage_bytes` | Computed via `directory_size_bytes(profile_dir)` (`inspect_profile_sized`) | Hardcoded `0` (`adapter.rs:223`) | Hardcoded `0` (`adapter.rs:223`) | UI/CLI profile size always displays `0 B` on macOS/Linux |
| `detected_zalo_user_id` | Scanned via `detect_zalo_user_id(&zalo_data_dir)` | Scanned in `inspect_profile`, **lost** during `restore_backup_as_new_profile` (`DEFECT-07`) | Scanned in `inspect_profile`, **lost** during `restore_backup_as_new_profile` (`DEFECT-07`) | Fix hardcoded Windows path in `crates/profile-store/src/backup.rs:455-460` |

### 3.6 Unsupported Operations & Capability Honesty Contract

- Verified in `CT-01` and `CT-05`:
  - `WindowsPlatformAdapter`: `overall_status = PartiallySupported`, `official_zalo_status = PartiallySupported`, `fixture_status = SupportedAndTested`, `concurrent_calls_status = Blocked` (`\\.\pipe\PipeZCallSend` / `PipeZCallRecv`).
  - `MacOsPlatformAdapter`: `overall_status = NotTested`, `official_zalo_status = NotTested`, `fixture_status = Experimental`, `concurrent_calls_status = Blocked` (`/tmp/socketzalorecv2021`).
  - `LinuxPlatformAdapter`: `overall_status = Unsupported`, `official_zalo_status = Unsupported` (`launch(OfficialZalo)` deterministically returns `Err(DimError::UnsupportedPlatform)`), `appimage_status = Some(Experimental)`, `fixture_status = Experimental`, `concurrent_calls_status = Unsupported`.
  - Non-host runtime calls (`MacOsPlatformAdapter::inspect_primary()` or `LinuxPlatformAdapter::inspect_primary()` invoked on Windows) fail closed with `UNSUPPORTED_PLATFORM` in `< 1 ms` without panicking.

### 3.7 Safety Guarantees Contract

1. **Primary Instance Process Protection (`PROTECTED_PRIMARY_PROCESS`):** Verified across all 3 adapters (`CT-06`) and against the live host Zalo Desktop tree (`11` real PIDs untouched in `LC-04`, `REL-07`).
2. **Primary Data Path Protection (`PROTECTED_PRIMARY_PATH`):** Verified on backup destination (`DS-04`). Note minor error-code divergence in Tauri `commands.rs:42` (`[FORBIDDEN_DEST]` when `dest_dir` is inside `state.data_dir`).
3. **Cryptographic Binary Trust (`UNVERIFIED_BINARY`):** Verified on Windows (`CT-05`): `WindowsPlatformAdapter::new()` enforces `WinVerifyTrust` (`require_authenticode_trust = true`), rejecting unsigned `CustomExecutable` binaries before `CreateProcessW`.
4. **Destructive Confirmation (`CONFIRM_REQUIRED`):** Verified in `PM-01`: `delete_profile(..., confirm = false)` fails closed without touching files or `profiles.json`.
5. **Directory Containment & Reparse Unlinking (`PATH_TRAVERSAL`):** Verified in `DS-02`, `DS-03`, `PM-07`: NTFS junctions and symlinks inside a profile are unlinked via `fs::remove_dir` / `fs::remove_file` without recursing into external targets, and backup walks skip all reparse points (`file_type.is_symlink()`).

---

## 4. Differential CLI & Static Parity Catalog (`DEFECT-08` + Auxiliary Divergences)

### 4.1 Dynamically Reproduced CLI Divergences (`CT-08`, `CT-09`, `CT-10`)

1. **`CT-08` — Slug Truncation Cap (`40` vs `32` characters):**
   - `crates/core/src/slug.rs:14,70` sets `MAX_SLUG_LEN = 40`, producing `tai-khoan-kinh-doanh-khu-vuc-mien-nam-so_<id>`.
   - `scripts/dim-host.ps1:582` truncates `SlugifyName` at `if ($sb.Length -ge 32) { break }`, producing `tai-khoan-kinh-doanh-khu-vuc-mie_<id>`.
2. **`CT-09` — Restore Name Collision & Length Overflow (`64` vs `69+` characters):**
   - `crates/profile-store/src/backup.rs:399-437`: Explicit `--name` matching an existing profile returns `Err(DimError::DuplicateProfileName)` (`DUPLICATE_NAME`), and auto-generated `"<name> (Restored)"` truncates the base prefix so the final name never exceeds `64` characters.
   - `scripts/dim-host.ps1:2595-2608`: Appends `" (Restored)"` without length clamping (producing a `69`-char profile name from a `58`-char source profile — violating `MAX_PROFILE_NAME_LEN = 64` so the restored profile cannot later be renamed to itself!) and silently renames explicit `--name` collisions to `"<name> #2"` (`ok: true`).
3. **`CT-10` — Empty/Corrupt `profiles.json` Fail-Closed & Missing-Arg Codes:**
   - `0`-byte `profiles.json`: `dim-cli` fails closed with `JSON_ERROR` (`ok: false`), whereas `scripts/dim-host.ps1:380-399` treats empty/whitespace `profiles.json` as an uninitialized store (`ok: true, profiles: []`), silently masking disk truncation!
   - Malformed `{corrupt` `profiles.json`: `dim-cli` returns `JSON_ERROR`, whereas `scripts/dim-host.ps1` throws an uncaught `ConvertFrom-Json` error wrapped at top-level as `UNHANDLED_EXCEPTION`.
   - Missing CLI argument (`create` with no `<name>`): `dim-cli.rs:171-175` passes `""` into `create_profile` returning `INVALID_NAME`, whereas `scripts/dim-host.ps1:3005-3055` returns `MISSING_ARG`.

### 4.2 Additional Cross-Stack Divergences Identified During Code & Contract Review

- **Vietnamese Case-Insensitive Uniqueness (`DEFECT-10`, `PM-04`):**
  - `crates/profile-store/src/store.rs:100,216,260,439` uses `eq_ignore_ascii_case`, allowing `"Đức Công Việc"` and `"đức công việc"` to coexist in Rust while `scripts/dim-host.ps1:1777` (`OrdinalIgnoreCase`) rejects the second profile with `DUPLICATE_NAME`.
- **Cross-Platform Backup Account Detection (`DEFECT-07`, `DS-08`):**
  - `crates/profile-store/src/backup.rs:455-460` hardcodes `AppData/Roaming/ZaloData` during `restore_backup_as_new_profile`, losing `zalo_user_id` on macOS (`Library/Application Support/ZaloData`) and Ubuntu (`.config/ZaloData`).
- **Backup Temporary Directory Exclusion Asymmetry:**
  - `crates/profile-store/src/backup.rs:566-573` skips both `appdata/local/temp` (Windows) and `tmp` (POSIX profile temp dir), whereas `scripts/dim-host.ps1:2478` only skips `AppData\Local\Temp`.
- **Zalo Account ID Regex Fallback Asymmetry:**
  - `scripts/dim-host.ps1:1215` checks `Database/_production/<digits>` first and falls back to parsing `"zalo_<6+ digits>"` inside `ZaloData/Partitions/zalo/IndexedDB` or `Local Storage` (`db_path.location`), whereas `crates/profile-store/src/zalo_identity.rs:20-38` only checks `Database/_production/<6+ digits>` and `IndexedDB/zalo_<6+ digits>` directory names.

---

## 5. Recommended Contract Remediations (For Owning Product Agents)

1. **Unify `SingletonLock` & `.runtime-pid` Inspection (`crates/platform-macos/src/adapter.rs`, `crates/platform-linux/src/adapter.rs`, `crates/platform-windows/src/adapter.rs`):**
   - Replace `singleton_lock.exists()` with `fs::symlink_metadata(&singleton_lock).is_ok()` (`DEFECT-01`).
   - Read `.runtime-pid` during startup/inspection when no lockfile exists yet, verify process liveness + profile ownership, add bounded readiness polling (`3,000 ms`) on macOS/Linux, and derive deterministic `NodeFixture` `--user-id` across all 3 adapters (`DEFECT-02`, `DEFECT-03`).
2. **Align Process-Tree Termination (`crates/platform-windows/src/adapter.rs`, `crates/platform-macos/src/adapter.rs`, `crates/platform-linux/src/adapter.rs`):**
   - On Windows, permit `TerminateProcess` for PIDs proven to belong to the isolated profile via `rstrtmgr.dll` or `LaunchTarget::CustomExecutable` path match (still strictly excluding `protected_primary_pids`) (`DEFECT-06`).
   - On macOS and Linux, recursively collect descendant PIDs and escalate to `SIGKILL` after `timeout_ms` (`DEFECT-06`).
3. **Align `scripts/dim-host.ps1` & `dim-core` / `dim-profile-store` Contracts (`DEFECT-08`, `DEFECT-10`):**
   - Align `MAX_SLUG_LEN` (`40` chars) in `scripts/dim-host.ps1:582`.
   - Enforce `DUPLICATE_NAME` on explicit `restore --name` collisions and clamp auto-generated `" (Restored)"` names to `64` chars in `scripts/dim-host.ps1:2595-2608`.
   - Fail closed with `JSON_ERROR` on 0-byte or malformed `profiles.json` in `scripts/dim-host.ps1:380-399`.
   - Replace `eq_ignore_ascii_case` in `crates/profile-store/src/store.rs` with Unicode lowercase comparison (`a.to_lowercase() == b.to_lowercase()`) (`DEFECT-10`).
4. **Cross-Platform `ZaloData` Resolution in `FileBackupManager` (`crates/profile-store/src/backup.rs`):**
   - Probe Windows (`AppData/Roaming/ZaloData`), macOS (`Library/Application Support/ZaloData`), and Linux (`.config/ZaloData`) layouts during both `backup_profile` and `restore_backup_as_new_profile`, and compute `disk_usage_bytes` + RSS telemetry on macOS/Linux (`DEFECT-07`).
5. **Deduplicate `platform_support_matrix` in `apps/desktop/src-tauri/src/commands.rs:465-494`:**
   - Emit distinct rows for Windows, macOS, and Ubuntu (Official + Fixture/AppImage) regardless of compile-time `caps.platform`.
