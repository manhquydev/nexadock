# ADR-003: Process Ownership, Primary Protection & Graceful Shutdown

**Status:** Accepted  
**Date:** 2026-11-17  
**Related:** [platform-feasibility.md](../research/platform-feasibility.md) | [repository-comparison.md](../research/repository-comparison.md) | [ADR-001](./adr-001-hybrid-isolation-architecture.md) | [ADR-004](./adr-004-backup-restore-and-data-safety.md)

---

## 1. Context & Failure Modes in Existing Tools

Managing multi-instance Electron processes (`Zalo.exe` main browser process + GPU, renderer, utility, `ZaloCall.exe`, `ZaloCap.exe`, `zocr-host.exe` child trees) presents three severe hazards demonstrated in existing tools:
1. **Primary Account Collateral Termination:** Tools like `kemonra/ZaloMulti` (`ZaloMulti.ps1:365`) execute `Get-Process Zalo | Stop-Process -Force`, killing the user's primary `%APPDATA%\ZaloData` session alongside all secondary profiles.
2. **SQLite WAL Corruption (`ftsm_*.db-wal`):** Abrupt `TerminateProcess` / `SIGKILL` without flushing Electron/SQLite WAL buffers risks database corruption and lost message state.
3. **False Running/Stopped Detection & Handle Leaks:** Relying on process names (`Zalo.exe`) cannot distinguish which profile owns which PID; relying on `ZaloStubExecutable` mutex fails because the stub exits within `<5s`; inheriting stdout/stderr pipe handles during launch hangs CLI parents; and Windows Job Objects with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` inadvertently kill Zalo windows if the manager UI closes.

---

## 2. Weighted Decision Matrix (Process Ownership & Shutdown)

Weights: Primary Account Safety (`30%`), Data / SQLite Flush Integrity (`25%`), Ownership Accuracy Across Crash/Update (`20%`), Decoupled Lifecycle (`15%`), Cross-Platform Clarity (`10%`). *(Note: macOS and Linux runtime shutdown timings are unmeasured locally.)*

| Option | Primary Safety (30%) | Flush Integrity (25%) | Ownership Accuracy (20%) | Decoupled Lifecycle (15%) | Cross-Platform (10%) | Weighted Score & Verdict |
| :--- | :---: | :---: | :---: | :---: | :---: | :---: |
| **A. Name-Based `Get-Process` + Force Kill** | 0/5 (0.00) | 0/5 (0.00) | 0/5 (0.00) | 5/5 (0.75) | 2/5 (0.20) | **0.95 / 5.00** (Rejected: catastrophic) |
| **B. Windows Job Object (`KILL_ON_JOB_CLOSE`)** | 4/5 (1.20) | 2/5 (0.50) | 3/5 (0.60) — dies if manager restarts | 0/5 (0.00) — couples UI lifetime to Zalo | 1/5 (0.10) | **2.40 / 5.00** (Rejected as primary mechanism) |
| **C. `--user-data-dir` Command-Line Scan Only** | 4/5 (1.20) | 4/5 (1.00) | 3.5/5 (0.70) — breaks if flags change | 5/5 (0.75) | 3/5 (0.30) | **3.95 / 5.00** (Used as fallback only) |
| **D. OS Lockfile Ownership (`rstrtmgr.dll` / `SingletonLock`) + `PrimaryProtectionGuard` + Graceful Quit Sequence** (**Selected**) | **5/5 (1.50)** | **5/5 (1.25)** | **5/5 (1.00)** | **5/5 (0.75)** | **4.5/5 (0.45)** | **4.95 / 5.00 (Selected)** |
| **E. Kernel Driver / Hook Tracking (`SbieDrv.sys`)** | 4/5 (1.20) | 3/5 (0.75) | 5/5 (1.00) | 4/5 (0.60) | 0/5 (0.00) | **3.55 / 5.00** (Rejected: requires Admin + `GPL-3.0`) |

---

## 3. Decision

### 3.1. Primary Instance Protection (`PrimaryProtectionGuard`)
Before any `stop`, `delete`, or destructive cleanup action, `dim-process-manager` (`crates/process-manager/src/guard.rs`) and `scripts/dim-host.ps1` inspect the OS primary instance (`InstanceMonitor::inspect_primary`):
- On Windows: queries `%APPDATA%\ZaloData\lockfile` via `rstrtmgr.dll` and checks path normalization against `%APPDATA%\ZaloData`.
- On macOS: inspects primary `~/Library/Application Support/ZaloData/SingletonLock`.
- **Invariant:** If the target profile's `zalo_data_dir` matches primary `zalo_data_dir`, OR if `main_pid` / any `process_ids` overlaps `primary.protected_pids`, the operation aborts immediately with `DimError::ProtectedPrimaryProcess` (`PROTECTED_PRIMARY_PROCESS`).

### 3.2. Stateless Lockfile Ownership & Crash Recovery
- **Windows Primary Ownership:** Call Windows Restart Manager API (`rstrtmgr.dll`: `RmStartSession`, `RmRegisterResources`, `RmGetList`) on `<profile_dir>\AppData\Roaming\ZaloData\lockfile`. Windows returns the exact owning Main PID even across independent manager restarts, with `--user-data-dir` command-line scanning as secondary fallback. Walk `CreateToolhelp32Snapshot` `ParentProcessId` to collect the exact scoped child tree (`Zalo.exe` renderers/GPU/utility, `ZaloCall.exe`, `ZaloCap.exe`, `zocr-host.exe`).
- **macOS & Linux Ownership:** Parse Chromium's `SingletonLock` symlink (`<hostname>-<pid>`) and validate live process identity via `NSRunningApplication` bundle ID (macOS) or `/proc` / `sysinfo` (Linux) plus fixture PID lockfiles.
- **Crash Recovery (`RecoveryManager`):** If an instance crashes leaving a stale `lockfile` or `.quit-signal` behind, `rstrtmgr.dll` / PID liveness returns empty (`running: false`), allowing `RecoveryManager::recover_stale_state` to clear stale lock/signal remnants safely before relaunch.
- **Live Rename Invariant:** Because `directory_name` (`<slug>_<8hex>`) is immutable after creation while display `name` lives in `profiles.json`, renaming a profile while running never disrupts open SQLite/Chromium file handles.

### 3.3. Clean Detached Process Creation
- **Windows:** `CreateProcessW` with `bInheritHandles = FALSE` and `dwCreationFlags = CREATE_UNICODE_ENVIRONMENT | CREATE_NEW_PROCESS_GROUP`, bypassing the `Zalo.exe` stub when launching `Zalo-<ver>\Zalo.exe` directly. Prevents CLI hangs and isolates console/signal groups.
- **macOS (`NOT_TESTED`):** `/usr/bin/open -n -a /Applications/Zalo.app --env CFFIXED_USER_HOME=<profile_home>`.
- **Linux (`EXPERIMENTAL`):** Clean child environment (`HOME`, `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `XDG_CACHE_HOME`, `TMPDIR`), optional `bwrap` isolation.

### 3.4. Graceful-First Shutdown Sequence (`GracefulQuitSequence`)
All platforms attempt clean application exit before bounded tree cleanup (default timeout `DEFAULT_GRACEFUL_QUIT_TIMEOUT_MS = 3,500 ms`):
1. **Windows (Real Zalo):** Spawn a short-lived signal invocation of `Zalo.exe` in the same isolated profile environment with `--z-update-force-quit` (triggering Electron's updater-quit handler) and send `WM_CLOSE` to visible profile windows; wait for the scoped PID tree to exit cleanly and flush SQLite WAL. Only after timeout, terminate remaining PIDs filtered through `PrimaryProtectionGuard::filter_protected_pids`.
2. **Windows / POSIX (Synthetic Fixture `fixture-app.mjs`):** Pass `--z-update-force-quit` (creating `.quit-signal` for clean flush + `lockfile` removal) or send `SIGTERM`/`SIGINT` before fallback termination.
3. **macOS (`NOT_TESTED`):** Call `NSRunningApplication.terminate()`, monitor child PIDs and open account DBs, and avoid abrupt termination while logged-in DBs are still flushing.
4. **Linux (`EXPERIMENTAL`):** Send `SIGTERM` to isolated PID tree, wait bounded timeout, then `SIGKILL` only on isolated PIDs.
