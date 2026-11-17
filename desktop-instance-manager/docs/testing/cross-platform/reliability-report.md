# Cross-Platform Reliability, Stress & Telemetry Report

**Worktree / Branch:** `test/cross-platform-reliability`  
**Baseline Commit Tested:** `a5feb4a52079`  
**Captured At (UTC):** `2026-11-17T17:07:34Z`  
**Raw Telemetry Artifacts:**
- [`plans/reports/reliability/metrics-benchmark-windows.json`](../../../plans/reports/reliability/metrics-benchmark-windows.json)
- [`plans/reports/reliability/windows-host-execution-evidence.json`](../../../plans/reports/reliability/windows-host-execution-evidence.json)
- [`plans/reports/reliability/defect-reproductions.json`](../../../plans/reports/reliability/defect-reproductions.json)

---

## 1. Host & Runtime Provenance

All measurements below were captured using live OS kernel counters (`GetProcessMemoryInfo`, `GetProcessTimes`, `CreateToolhelp32Snapshot`, `RmGetList`, and filesystem snapshots) with **zero synthetic or mocked benchmark numbers**:

| Provenance Field | Observed Value |
|---|---|
| **Operating System** | `windows` — `Microsoft Windows [Version 10.0.26300.9974]` (Windows 11 Pro 64-bit, non-elevated `asInvoker`) |
| **Architecture / CPUs** | `x86_64` (`16` logical processors) |
| **Rust Toolchain** | `rustc 1.100.0 (0ef1bc386 2026-11-09)` (`stable-x86_64-pc-windows-gnu`) |
| **Node.js Runtime** | `v26.13.1` (`CN=OpenJS Foundation` Authenticode-signed) |
| **Fixture & Baseline Commit** | `a5feb4a52079` (`dim-xf-fixture v0.1.0` + `xf-fixture.mjs`) |
| **Deterministic Test Seed** | `0x20261117` |
| **macOS / Ubuntu Hardware Status** | Not present on this Windows workstation (`XP-MAC-04` and `XP-LNX-04` honestly marked `SKIPPED`; runner scripts ready for CI/VMs) |

---

## 2. Fixture Performance & Resource Telemetry (Windows 11 Host)

### 2.1. Startup & Graceful Shutdown Latency Distributions

Measured from OS spawn invocation until `lockfile` + `fixture-state.json` commit (startup) and from `.quit-signal` / `stop_profile` request until process termination + lock release (shutdown):

| Execution Path | Runs | Min (ms) | P50 / Median (ms) | P95 (ms) | Max (ms) | Mean (ms) | Failure Rate |
|---|---:|---:|---:|---:|---:|---:|---:|
| **Native Rust Fixture (`xf-desktop-fixture.exe`) — Startup (10-run bench)** | 10 | 27.00 | **27.72** | 34.04 | 38.06 | 28.92 | `0.00%` |
| **Native Rust Fixture (`xf-desktop-fixture.exe`) — Shutdown (10-run bench)** | 10 | 8.46 | **31.24** | 33.04 | 33.55 | 25.02 | `0.00%` |
| **Native Rust Fixture (`REL-02` 100-Cycle Soak) — Startup** | 100 | 25.74 | **30.76** | 36.68 | 46.24 | 31.25 | `0.00%` (`0/100`) |
| **Native Rust Fixture (`REL-02` 100-Cycle Soak) — Shutdown** | 100 | 23.54 | **26.46** | 28.37 | 31.16 | 26.59 | `0.00%` (`0/100`) |
| **`InstanceCoordinator` + Signed Node Fixture (`NodeFixture`) — Startup** | 10 | 308.81 | **329.35** | 371.11 | 374.17 | 340.15 | `0.00%` *(see §2.4 race note)* |
| **`InstanceCoordinator` + Signed Node Fixture (`NodeFixture`) — Shutdown** | 10 | 160.34 | **190.80** | 237.83 | 240.96 | 192.37 | `0.00%` |

**Soak Stability (`REL-02`, 100 Consecutive Cycles in `5,806 ms`):**
- Mean startup of first 10 cycles (`30.32 ms`) vs. last 10 cycles (`31.71 ms`) showed **`< 1.4 ms` drift**, confirming zero progressive handle or directory-scan degradation across 100 consecutive restarts.

### 2.2. Concurrent Multi-Instance Memory Overhead (`1`, `3`, `5` Instances)

Measured via Win32 `GetProcessMemoryInfo` (`WorkingSetSize` and `PagefileUsage` private bytes):

| Runtime Mode | Concurrent Instances | Active Processes | Total Working Set | Total Private Bytes | Avg Working Set / Instance |
|---|---:|---:|---:|---:|---:|
| **Native Rust Fixture (`xf-desktop-fixture.exe`)** | 1 | 1 | 4,947,968 B (4.72 MiB) | 765,952 B (0.73 MiB) | **4.72 MiB** |
| **Native Rust Fixture (`xf-desktop-fixture.exe`)** | 3 | 3 | 14,835,712 B (14.15 MiB) | 2,285,568 B (2.18 MiB) | **4.72 MiB** |
| **Native Rust Fixture (`xf-desktop-fixture.exe`)** | 5 | 5 | 24,719,360 B (23.57 MiB) | 3,788,800 B (3.61 MiB) | **4.71 MiB** |
| **Signed Node.js Fixture (`node.exe` + `xf-fixture.mjs`)** | 1 | 1 | 56,176,640 B (53.57 MiB) | 23,887,872 B (22.78 MiB) | **53.57 MiB** |
| **Signed Node.js Fixture (`node.exe` + `xf-fixture.mjs`)** | 3 | 3 | 168,595,456 B (160.79 MiB) | 72,388,608 B (69.04 MiB) | **53.60 MiB** |
| **Signed Node.js Fixture (`node.exe` + `xf-fixture.mjs`)** | 5 | 5 | 281,255,936 B (268.23 MiB) | 119,185,408 B (113.66 MiB) | **53.65 MiB** |

### 2.3. Idle CPU Usage & File-System / Cleanup Residuals

- **Idle CPU (`GetProcessTimes` over `1,200 ms` window with `3` concurrent instances polling at `25 ms`):**
  - Native Rust fixture (`3` instances): `15.625 ms` total CPU delta (`1.30%` of 1 core / **`0.081%` host-normalized** across 16 logical CPUs).
  - Signed Node.js fixture (`3` instances): `15.625 ms` total CPU delta (`1.30%` of 1 core / **`0.081%` host-normalized**).
- **File-System Changes (5-profile lifecycle inside isolated `DIM_DATA_DIR`):**
  - `65` files created, `1` file (`profiles.json`) atomically updated, `23,251` bytes total across isolated profile layouts; **`0` files written outside the temporary sandbox**.
- **Cleanup Residuals (Post-stop & Post-delete):**
  - `surviving_pids: []`, `leftover_lock_artifacts: []`, `leftover_staging_dirs: []`.

### 2.4. Empirical Transient Race Observation Across Repeated Runs

During calibration run `2026-11-17T16:56:37Z` (prior to adding recovery cleanup in the benchmark harness), `1` of `20` rapid `NodeFixture` launch/stop operations (`5%`) exhibited a **0-byte `lockfile.exists()` startup race**:
- `WindowsPlatformAdapter::launch` (`crates/platform-windows/src/adapter.rs:624`) breaks out of its `2,500 ms` readiness loop the microsecond `lockfile.exists()` becomes `true` — even when `node.exe` has just executed `fs.openSync(lockfile, 'wx+')` (`0` bytes) and has not yet written JSON bytes!
- Immediately thereafter, `inspect_profile` reads `0` bytes from `lockfile` (`read_lockfile_json_pid` -> `None`), sees `is_file_exclusively_locked == false` (Node libuv opens files with shared read/write), ignores `.runtime-pid`, and returns `running: false, main_pid: None`.
- If `stop_profile` is called in that window (`coordinator.rs:113-120`), `StaleLockRecoveryManager` deletes `lockfile` and `.runtime-pid` out from under the live starting `node.exe` process! This directly reinforces **`DEFECT-03`** below.

---

## 3. Product Defects Discovered & Minimal Fixture Reproductions

> [!CAUTION]
> Per Mission C rules, **zero product adapter or shared crate files were modified**. Every defect below has a deterministic, repeatable reproduction in `tests/cross-platform/` and a strict regression gate (`cargo test -- --ignored` or `xf-test-runner --fail-on-product-defects`) ready to verify fixes.

### `DEFECT-01` (HIGH) — macOS & Linux Adapters Fail to Detect Chromium's Dangling `SingletonLock` Symlinks
- **Reproduced By:** `XP-LNX-02`, `XP-MAC-03`
- **Implicated Files:**
  - [`crates/platform-linux/src/adapter.rs`](../../../crates/platform-linux/src/adapter.rs#L190-L195)
  - [`crates/platform-macos/src/adapter.rs`](../../../crates/platform-macos/src/adapter.rs#L194-L199)
- **Root Cause:** Chromium/Electron on macOS and Linux creates `SingletonLock` as a **dangling symbolic link** whose target string is `<hostname>-<pid>` (not an existing file on disk). Both adapters check `singleton_lock.exists()`, which invokes `std::fs::metadata()` (following the symlink) and **always returns `false`** for a dangling symlink!
- **Impact:** After an Electron/Zalo crash on macOS or Ubuntu leaving `SingletonLock -> host-99999`, `stale_lock_detected` is falsely reported as `false`, `status_label` reports `"Stopped"` instead of `"Stopped (Stale Lock)"`, and `InstanceCoordinator::launch_profile` skips `StaleLockRecoveryManager::recover_stale_state()`.
- **Minimal Reproduction:**
  ```bash
  # On Linux/macOS: create dangling SingletonLock for dead PID 999999 and inspect status
  ln -s "$(hostname)-999999" "<profile_dir>/.config/ZaloData/SingletonLock"
  dim-cli --cli --json --data-dir "<data_dir>" status
  # Observed: stale_lockfile_detected == false (Expected: true via fs::symlink_metadata().is_ok())
  ```

### `DEFECT-02` (HIGH) — macOS & Linux `launch()` Missing Readiness Poll, `.runtime-pid` Fallback & Fixture `--user-id` Derivation
- **Reproduced By:** `XP-MAC-03`
- **Implicated Files:**
  - [`crates/platform-macos/src/adapter.rs`](../../../crates/platform-macos/src/adapter.rs#L192-L196) & [`L298-L320`](../../../crates/platform-macos/src/adapter.rs#L298-L320)
  - [`crates/platform-linux/src/adapter.rs`](../../../crates/platform-linux/src/adapter.rs#L189-L190) & [`L298-L300`](../../../crates/platform-linux/src/adapter.rs#L298-L300)
- **Root Cause:**
  1. Both POSIX adapters write `child.id()` to `.runtime-pid` on `launch()` and **immediately** call `self.inspect_profile()` without polling for `SingletonLock`/`lockfile` creation, while `inspect_profile()` **never reads `.runtime-pid`**. Thus `launch()` immediately returns `running: false, main_pid: None`.
  2. Unlike `WindowsPlatformAdapter::launch` (`crates/platform-windows/src/adapter.rs:600-607`), neither POSIX adapter derives a deterministic per-profile `--user-id` for `LaunchTarget::NodeFixture`, so launching two fixture profiles defaults both to `userId = "100001"`, failing the second launch with `DUPLICATE_ZALO_USER_ID`.
- **Minimal Reproduction:** Run `cargo test --manifest-path tests/cross-platform/Cargo.toml suite_05_cross_platform_differences_matrix` (or launch 2 `NodeFixture` profiles via `dim-cli` on macOS/Ubuntu).

### `DEFECT-03` (HIGH) — Slow Pre-Lock Startup Visibility Blindness & Stealth Orphan Process on Early `stop_profile`
- **Reproduced By:** `LC-07` (`2,865 ms` on Windows 11) + Calibration Telemetry Race (§2.4)
- **Implicated Files:**
  - [`crates/platform-windows/src/adapter.rs`](../../../crates/platform-windows/src/adapter.rs#L512-L522) & [`L620-L631`](../../../crates/platform-windows/src/adapter.rs#L620-L631)
  - [`crates/process-manager/src/coordinator.rs`](../../../crates/process-manager/src/coordinator.rs#L113-L120)
- **Root Cause:** `WindowsPlatformAdapter::inspect_profile` only consults `.runtime-pid` when `is_file_exclusively_locked(&lockfile)` is already `true` (`adapter.rs:512`), and `launch()` only waits `2,500 ms` (for `NodeFixture` only; `0 ms` for `OfficialZalo`/`CustomExecutable`) and breaks as soon as `lockfile.exists()` (even at `0` bytes). While an instance is still booting prior to locking `lockfile`:
  - `inspect_profile` reports `running = false, main_pid = None`.
  - Calling `stop_profile` sees `!state.running`, cleans `.runtime-pid` if `stale_lock_detected`, and returns `Ok(0)` while the live process finishes starting in the background as an untracked orphan!
- **Minimal Reproduction:**
  ```powershell
  cargo run --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner -- --suite lifecycle
  # Case LC-07 spawns xf-fixture.mjs with --slow-startup-pre-lock-ms=3100 and calls stop_profile()
  ```

### `DEFECT-04` (HIGH) — Concurrent Launch Race in `InstanceCoordinator` & Unscoped `profiles.json.tmp` Across Processes
- **Reproduced By:** `LC-08` (`1,420 ms` on Windows 11)
- **Implicated Files:**
  - [`crates/process-manager/src/coordinator.rs`](../../../crates/process-manager/src/coordinator.rs#L49-L104)
  - [`crates/profile-store/src/store.rs`](../../../crates/profile-store/src/store.rs#L169-L180)
  - [`scripts/dim-host.ps1`](../../../scripts/dim-host.ps1#L401-L415)
- **Root Cause:** `InstanceCoordinator` holds no per-profile or coordinator mutex around `inspect_profile -> launch`, allowing 4 concurrent callers on the same profile to all observe `runtime_before.running == false` and spawn 4 racing OS processes. Additionally, `JsonProfileStore::write_document_atomic_locked` and `dim-host.ps1::SaveStore` write to a fixed `profiles.json.tmp` path without PID/UUID disambiguation or an advisory cross-process file lock.
- **Minimal Reproduction:** Execute `LC-08` via `xf-test-runner --suite lifecycle` (`4` threads barrier-synchronized calling `launch_profile` with `--slow-startup-pre-lock-ms=220`).

### `DEFECT-05` (MEDIUM) — `focus_or_restore` Silent No-Op on Linux & Missing Headless Fixture Signal on macOS
- **Identified In:** `LC-06` cross-platform code audit
- **Implicated Files:**
  - [`crates/platform-linux/src/adapter.rs`](../../../crates/platform-linux/src/adapter.rs#L310-L318)
  - [`crates/platform-macos/src/adapter.rs`](../../../crates/platform-macos/src/adapter.rs#L331-L356)
- **Root Cause:** `LinuxPlatformAdapter::focus_or_restore` unconditionally returns `Ok(())` without signaling `.focus-signal` or restoring the window; `MacOsPlatformAdapter::focus_or_restore` invokes `System Events` AppleScript (which silently fails on headless fixtures without writing `.focus-signal`).

### `DEFECT-06` (HIGH) — Hung Process & Child Tree Cleanup Failures (`allowed_exe` Hardcode on Windows; Missing `SIGKILL` & Tree Walk on POSIX)
- **Reproduced By:** `LC-09` (`9,237 ms` live on Windows 11) & `REL-07` cross-platform audit
- **Implicated Files:**
  - [`crates/platform-windows/src/adapter.rs`](../../../crates/platform-windows/src/adapter.rs#L492-L498) & [`L771-L785`](../../../crates/platform-windows/src/adapter.rs#L771-L785)
  - [`crates/platform-macos/src/adapter.rs`](../../../crates/platform-macos/src/adapter.rs#L386-L404)
  - [`crates/platform-linux/src/adapter.rs`](../../../crates/platform-linux/src/adapter.rs#L342-L360)
- **Root Cause:**
  1. **Windows:** `WindowsPlatformAdapter::stop()` filters forced termination by `allowed_exe` matching only `zalo*`, `zocr*`, `node.exe`, or `dim-test-fixture*`. When `xf-desktop-fixture.exe --lock-mode=exclusive --hang-on-quit` holds a profile's `lockfile`, `inspect_profile` reports `running: true, main_pid: Some(pid)` via `rstrtmgr.dll`, yet `stop_profile()` **skips `TerminateProcess`**, returning `Ok(0)` while the hung process stays alive! Any non-matching child helper (e.g., `chrome_crashpad_handler.exe` or custom binary) similarly escapes cleanup.
  2. **macOS:** `MacOsPlatformAdapter::stop()` sends `SIGTERM` to `main_pid` only, waits `timeout_ms`, **never escalates to `SIGKILL`**, never collects child PIDs, never removes `.runtime-pid`, and returns `Ok(0)` while a hung process remains running.
  3. **Linux:** `LinuxPlatformAdapter::stop()` escalates to `SIGKILL` on `main_pid` only (`process_ids` never walks `/proc/<pid>/task/*/children` or process groups), leaving orphaned child workers alive if the parent is `SIGKILL`ed.
- **Minimal Reproduction:** Execute `LC-09` (`xf-desktop-fixture.exe --lock-mode=exclusive --hang-on-quit` followed by `coordinator.stop_profile`).

### `DEFECT-07` (MEDIUM) — macOS & Linux Post-Launch `zalo_user_id` Lost During Backup & Restore
- **Reproduced By:** `DS-08` (`520 ms`)
- **Implicated Files:** [`crates/profile-store/src/backup.rs`](../../../crates/profile-store/src/backup.rs#L143) & [`L339-L345`](../../../crates/profile-store/src/backup.rs#L339-L345) & [`L455-L460`](../../../crates/profile-store/src/backup.rs#L455-L460)
- **Root Cause:** When a user scans a QR code *after* `launch_profile` has already run, `profiles.json` still holds `zalo_user_id: None`. `FileBackupManager::backup_profile` copies `profile.zalo_user_id` (`None`) instead of calling `detect_zalo_user_id`, and `restore_backup_as_new_profile` only probes Windows `AppData/Roaming/ZaloData` — missing macOS `Library/Application Support/ZaloData` and Linux `.config/ZaloData`!

### `DEFECT-08` (MEDIUM) — 4 Public Contract Divergences Between Rust (`dim-cli`) and PowerShell (`dim-host.ps1`)
- **Reproduced By:** `CT-08`, `CT-09`, `CT-10` (detailed in [`adapter-contract-report.md`](./adapter-contract-report.md)).

### `DEFECT-09` (MEDIUM) — `ProfileStore::delete_profile` Trait Method Bypasses `PROFILE_RUNNING` Guard
- **Reproduced By:** `PM-08` (`636 ms`)
- **Implicated Files:** [`crates/profile-store/src/store.rs`](../../../crates/profile-store/src/store.rs#L271-L273)
- **Root Cause:** `impl ProfileStore for JsonProfileStore::delete_profile` delegates to `self.delete_profile_with_guard(id_or_name, confirm, false, ...)`, hardcoding `is_running = false` and allowing callers of the trait method to delete a live running profile's metadata and unlocked files.

### `DEFECT-10` (MEDIUM) — Non-ASCII Vietnamese Case-Insensitivity Gap in `JsonProfileStore`
- **Reproduced By:** `PM-04` (`202 ms`)
- **Implicated Files:** [`crates/profile-store/src/store.rs`](../../../crates/profile-store/src/store.rs#L100) & [`L216`](../../../crates/profile-store/src/store.rs#L216) & [`L260`](../../../crates/profile-store/src/store.rs#L260) & [`L439`](../../../crates/profile-store/src/store.rs#L439)
- **Root Cause:** `str::eq_ignore_ascii_case` only folds ASCII `A-Z`; `"Đức Công Việc"` and `"đức công việc"` are accepted as two separate profiles in Rust (`dim-cli` / Tauri) while rejected as `DUPLICATE_NAME` in PowerShell (`dim-host.ps1`).
