# Multi-Zalo Central Integration & Critical Defect Remediation Report

> **Integration Branch:** `integration/reliability-hardening` (`<WORKTREE_ROOT>/zalo-integration`)  
> **Report Date:** `2026-11-18`  
> **Baseline Commit (`main`):** `a5feb4a520798064c1508c26fe4c4ac17a3a03cf`  
> **Verified Code & Evidence Commit (`<FINAL_CODE_SHA>`):** `bff0674444496bc057ba42fcc26c4c5b2d93dd7b`  
> **Final Branch Tip (`HEAD`):** `HEAD (direct documentation child of bff0674444496bc057ba42fcc26c4c5b2d93dd7b)` *(documentation commit recording `bff0674444496bc057ba42fcc26c4c5b2d93dd7b` and GitHub/CI sync status)*

---

## 1. Baseline SHA

- **Authoritative Baseline (`main`):** `a5feb4a520798064c1508c26fe4c4ac17a3a03cf` (`a5feb4a`)
- **Baseline Integrity:** `<REPO_ROOT>` (`main`) was never checked out, reset, cleaned, stashed, or mutated during integration (`0` tracked file modifications against `a5feb4a520798064c1508c26fe4c4ac17a3a03cf`).

---

## 2. Source Commits Used Across Workstreams

All integration work started from `a5feb4a520798064c1508c26fe4c4ac17a3a03cf` inside the dedicated worktree `<WORKTREE_ROOT>/zalo-integration`. Both the initial mission-briefed SHAs and the final clean worktree tips advanced by each domain stream are recorded below for complete provenance:

| Workstream / Branch | Initial Briefed SHA | Final Clean Worktree Tip SHA | Merge / Landing Commit on `integration/reliability-hardening` | Scope |
| :--- | :---: | :---: | :---: | :--- |
| `main` (`<REPO_ROOT>`) | `a5feb4a520798064c1508c26fe4c4ac17a3a03cf` | `a5feb4a520798064c1508c26fe4c4ac17a3a03cf` | *(branch base)* | Baseline workspace, Windows PoC/MVP core, Tauri shell |
| `test/cross-platform-reliability` | `a2143550935d75c1569123e628eccb3358654834` (C1) | `5e802e08119efce56d7df157538122e16ed64801` (C2) | `5fa798c` | 55-case cross-platform reliability & contract harness (`xf-test-runner`), synthetic `xf-desktop-fixture` |
| `research/desktop-compatibility` | `bf08f10310402122e57e76840a4c246f0fa12f80` (D1) | `0e5d44c0148aa7c5f436e7274597453d5c5501c3` (D2) | `155efc3` | Calibrated compatibility triage (`F-CORE-*`, `F-WIN-*`, `F-MAC-*`, `F-UBU-*`), `TC-H1..H8` specs, `IR-*` backlog |
| `build/cross-platform-distribution` | `ac0df10c795208d983a2fb8d03f95ff627adf59c` (E1) | `6c4150ecdb106f07aceca96098aaf3ab0d9a1326` (E2) | `ec46209` (+ `f83059e`, `a7f1413`, `5616bcb`, `8aa7a62`, `20d1729`) | Manual `workflow_dispatch` 7-job native OS CI, 7-pillar `cargo-deny` audit, artifact provenance manifests, selective `IR-001..007` |
| `feat/ui-vi-en-quiet-workspace` | `0f873024f4ee16810b9d35cf206803f793fc55cb` | `0f873024f4ee16810b9d35cf206803f793fc55cb` | `1867beb` *(merged AFTER P0 core/store fixes `e53a0dc..7024a1f`)* | Quiet Bilingual Account Dock (`vi-VN` / `en-US`), light/dark/system themes, 8-color identity tokens |
| `feat/macos-multi-instance` | `fb2abcfc419c56b9579eee124a3a169697ec2740` (A1) | `f2b8f9f182316224179658634f3c526ed9f5ff26` (A2) | `8a7861d` (+ `0aee2db` reconciliation) | 5-tier codesign/publisher pinning, 12-surface isolation, `<104B` short `TMPDIR`, `lstart` PID guard, zero-TCC AppKit focus, WAL-safe `SIGKILL` |
| `feat/ubuntu-multi-instance` | `9a10a9f777d1e36c2ffceafd4e9d437221516fed` (B1) | `62dc5d1555799a902d9f8aa8a1788a47be8ac578` (B2) | `47a2807` (+ `004ab7c` reconciliation & P0/P1 fixes) | `setsid` + `bwrap --new-session`, XDG/GUI/D-Bus/Wayland mounts, `(pid, starttime_ticks, nonce)` ownership, AppArmor `userns` preflight, zero `--no-sandbox` |

---

## 3. Integrated Branches & Worktree Hygiene

### 3.1 Integration Sequence Enforced
1. **Phase 1 & Phase 2 First (Core Data, Locking & Process Lifecycle Hardening):** Merged `test/cross-platform-reliability` (`5fa798c`), `research/desktop-compatibility` (`155efc3`), and `build/cross-platform-distribution` (`ec46209`); landed all P0/P1/P2 Shared Core and Windows fixes (`e53a0dc`, `5f95211`, `186e922`, `7024a1f`, `fbee1ed`, `31591f1`, `c796fdf`) **before** merging UI (`1867beb`) and **before** merging POSIX platform branches.
2. **Independent Security & Concurrency Audit Remediation (`f331085` -> `e602741`):** Closed `VerifiedDeletionState` compile-time/runtime enforcement (`f331085`) and remediated all P0/Security findings from independent code audit (`e602741`: relocated per-profile OS lockfiles to `<data_dir>/locks/<id>.lock`, closed distinct-profile launch admission TOCTOU via `LaunchAdmissionClaim`, and routed Tauri `get_status` through `InstanceCoordinator::inspect_profile_record`).
3. **Phase 3 Controlled Platform Merges (`8a7861d`..`004ab7c`):** Merged `feat/macos-multi-instance` (`f2b8f9f` -> `8a7861d` + `0aee2db`) and `feat/ubuntu-multi-instance` (`62dc5d1` -> `47a2807` + `004ab7c`) one branch at a time, aligning `VerifiedDeletionState`, `ProfileLifecycleState::Starting`, Linux `NodeFixture` FNV-1a `--user-id`, Linux `/proc/<pid>/fd` open SQLite DB/WAL protection before `SIGKILL`, and `XP-LNX-03`.
4. **Phase 4 & Phase 5 End-to-End Verification (`bff0674444496bc057ba42fcc26c4c5b2d93dd7b`):** Updated Account Dock UI for all 5 `ProfileLifecycleState` states, hardened `xf-fixture.mjs` atomic state replacement on Windows NTFS and `XP-WIN-03` multi-threaded handle isolation, executed the real compiled `dim-desktop.exe` 12-step Win32 + WebView2 E2E suite, and scrubbed all local workstation paths.

### 3.2 Sibling Worktree Preservation Summary
All 6 feature/test/research/build worktrees (`zalo-cross-platform-tests`, `zalo-ui-worktree`, `zalo-macos-worktree`, `zalo-ubuntu-worktree`, `zalo-compatibility-research`, `zalo-cross-platform-build`) have `0` modified/untracked files (`git status --short` empty), and `zalo` (`main` @ `a5feb4a`) has `0` modified tracked files and was never touched by integration.

---

## 4. Defect IDs & Canonical Status Matrix

Statuses strictly follow the separate epistemic state machine:
- **`OPEN`** — Confirmed gap with no complete remediation committed yet.
- **`REPRODUCED`** — Dynamically triggered at runtime prior to fix.
- **`FIXED`** — Code remediation committed and compiled/unit/static-tested, **awaiting native target-OS runtime execution** (`FIXED` is **never** equated with `VERIFIED`).
- **`VERIFIED`** — Code remediation committed AND verified at runtime on the target host OS with passing regression and concurrency gates.
- **`BLOCKED`** — Permanent vendor architecture, OS security boundary, or hardware single-opener constraint that cannot be bypassed without forbidden binary patching or security weakening.

| Defect / Finding ID | Linked Test(s) | Scope | Pre-Fix Status | Current Status | Fix Commit(s) |
| :--- | :--- | :--- | :---: | :---: | :---: |
| `DEFECT-01` *(macOS)* | `XP-MAC-03`, `XP-MAC-04` | `crates/platform-macos` (`SingletonLock` dangling symlink `symlink_metadata`) | `OPEN` *(static)* | **`FIXED`** *(static `PASS` on Win; awaits native Darwin run)* | `8a7861d` (`f2b8f9f`) |
| `DEFECT-01` *(Linux)* | `XP-LNX-02`, `XP-LNX-04` | `crates/platform-linux` (`SingletonLock` dangling symlink `symlink_metadata`) | `OPEN` *(static)* | **`FIXED`** *(static `PASS` on Win; awaits native Ubuntu run)* | `47a2807` (`62dc5d1`) |
| `DEFECT-02` *(macOS)* | `XP-MAC-03`, `LC-01` | `crates/platform-macos` (`NodeFixture` FNV-1a `--user-id`, `.runtime-pid`, readiness poll) | `OPEN` *(static)* | **`FIXED`** *(unit/static `PASS`; awaits native Darwin run)* | `8a7861d` (`f2b8f9f`) |
| `DEFECT-02` *(Linux)* | `XP-LNX-04`, `LC-01` | `crates/platform-linux` (`NodeFixture` FNV-1a `--user-id` derivation + `.runtime-pid`) | `OPEN` *(static)* | **`FIXED`** *(46/46 Linux unit/suite tests `PASS`; awaits native Ubuntu run)* | `004ab7c` |
| `DEFECT-03` *(Core/Win)* | `LC-07`, `REL-08` | `crates/core/src/lease.rs`, `crates/platform-windows`, `crates/process-manager` | `REPRODUCED` | **`VERIFIED`** | `5f95211`, `f331085` |
| `DEFECT-03` *(macOS/Linux)* | `LC-07` *(POSIX)* | `crates/platform-macos`, `crates/platform-linux` (pre-lock `Starting` ownership & stop) | `OPEN` *(static)* | **`FIXED`** *(awaits native Darwin/Ubuntu run)* | `0aee2db`, `004ab7c` |
| `DEFECT-04a` | `LC-08`, `REL-05` | Per-profile launch coalescing (`op_mutex` + `<data_dir>/locks/<id>.lock`) | `REPRODUCED` | **`VERIFIED`** | `e53a0dc`, `e602741` |
| `DEFECT-04b` | `LC-08`, `DS-07` | Cross-process `profiles.lock` + unique `.tmp` + `sync_all` + atomic `MoveFileExW` | `REPRODUCED` | **`VERIFIED`** | `e53a0dc`, `f331085` |
| `DEFECT-05` *(Windows)* | `LC-06`, `TC-H6` | `focus_or_restore` Foreground Lock check + isolated `second-instance` handoff | `REPRODUCED` | **`VERIFIED`** | `c796fdf` |
| `DEFECT-05` *(macOS/Linux X11)* | `LC-06` *(POSIX)* | `.focus-signal` + zero-TCC AppKit `activateWithOptions:3` (macOS) / `wmctrl`/`xdotool` (X11) | `OPEN` *(static)* | **`FIXED`** *(awaits native Darwin/Ubuntu run)* | `8a7861d`, `47a2807` |
| `DEFECT-05` *(Linux Wayland GUI)* | `TC-H8A` (`F-UBU-05`) | Wayland GUI `CustomExecutable` `second-instance` re-exec fallback (currently returns honest `UnsupportedPlatform`) | `OPEN` | **`OPEN`** *(Ubuntu experimental P1 backlog)* | — |
| `DEFECT-06` *(Windows)* | `LC-09`, `REL-07` | Hung `NodeFixture`/`CustomExecutable` tree termination via `RestartManager`/lease + `PrimaryProtectionGuard` | `REPRODUCED` | **`VERIFIED`** | `5f95211`, `58dbd61` |
| `DEFECT-06` *(macOS/Linux)* | `LC-09`, `F-UBU-03` | WAL/open-DB check (`lsof` on macOS; `/proc/<pid>/fd` on Linux) before `SIGKILL` escalation | `OPEN` *(static)* | **`FIXED`** *(unit tests `PASS`; awaits native Darwin/Ubuntu run)* | `8a7861d` (`f2b8f9f`), `004ab7c` |
| `DEFECT-07` | `DS-08`, `REL-04` | Cross-OS `zalo_user_id` preservation across Windows/macOS/Linux `ZaloData` paths during backup/restore | `REPRODUCED` | **`VERIFIED`** | `31591f1` |
| `DEFECT-08a` | `CT-08` | Rust `dim-cli` vs PowerShell `dim-host.ps1` `MAX_SLUG_LEN = 40` parity | `REPRODUCED` | **`VERIFIED`** | `31591f1` |
| `DEFECT-08b` | `CT-09`, `DS-05` | `dim-host.ps1` explicit restore collision (`DUPLICATE_NAME`) & `<= 64` char clamp | `REPRODUCED` | **`VERIFIED`** | `31591f1` |
| `DEFECT-08c` | `CT-10`, `REL-06` | Fail-closed `JSON_ERROR`/`SCHEMA_ERROR` on `0`-byte/corrupt `profiles.json` without overwriting file | `REPRODUCED` | **`VERIFIED`** | `7024a1f`, `f331085` |
| `DEFECT-09` | `PM-08`, `TC-H7` | `ProfileStore::delete_profile_verified` `VerifiedDeletionState`, OS locks, two-phase reparse-point check | `REPRODUCED` | **`VERIFIED`** | `186e922`, `f331085`, `e602741` |
| `DEFECT-10` | `PM-04`, `PM-06` | Full Unicode NFC case-folding duplicate profile name rejection (`Đức` vs `đức`) | `REPRODUCED` | **`VERIFIED`** | `31591f1` |
| `STATIC-TAURI-DIAG-01` | `CT-01`, `Tauri E2E` | Deduplicated 4-row `platform_support_matrix` in `apps/desktop/src-tauri/src/commands.rs` | `OPEN` *(static)* | **`VERIFIED`** | `31591f1` |
| `DEFECT-C2-RM-01` | Parallel `cargo test` | Process-wide `RM_SESSION_MUTEX` around `rstrtmgr.dll` + causal `GetProcessTimes` parent-PID filter | `REPRODUCED` | **`VERIFIED`** | `58dbd61` |
| `AUDIT-43-01` | `p0_coordinator_concurrency` | Cross-process `launch`/`stop`/`backup` vs `delete_profile` directory resurrection & `ERROR_DIR_NOT_EMPTY` (`os error 145`) | `OPEN` *(audit)* | **`VERIFIED`** | `e602741` |
| `AUDIT-43-02` | `p0_coordinator_concurrency` | Simultaneous distinct-profile launch TOCTOU bypassing `DUPLICATE_ZALO_USER_ID` & `SHARED_EXTERNAL_DB_PATH` | `OPEN` *(audit)* | **`VERIFIED`** | `e602741` |
| `AUDIT-43-03` | `Tauri E2E`, `ui-contracts` | Tauri `commands::get_status` bypassing `InstanceCoordinator` `Stopping` lifecycle state | `OPEN` *(audit)* | **`VERIFIED`** | `e602741`, `bff0674444496bc057ba42fcc26c4c5b2d93dd7b` |
| `AUDIT-43-04` | `git grep <LOCAL_USER>` | Local workstation path strings in merged docs/evidence reports | `OPEN` *(audit)* | **`VERIFIED`** | `bff0674444496bc057ba42fcc26c4c5b2d93dd7b` |
| `DEFECT-XF-WIN-RACE` | `REL-01`, `XP-WIN-03` | `xf-fixture.mjs` pre-lock stale signal clear + NTFS `MoveFileExW` `EPERM` retry; `XP-WIN-03` multi-thread handle isolation | `REPRODUCED` | **`VERIFIED`** | `bff0674444496bc057ba42fcc26c4c5b2d93dd7b` |
| `F-UBU-01` *(Official VNG Linux)* | `XP-LNX-02`, `CT-01` | VNG ships zero official Linux Zalo Desktop builds (`app.asar` throws `Unsupported OS: linux`) | `BLOCKED` | **`BLOCKED`** *(`OFFICIAL_ZALO_UNSUPPORTED`)* | Preserved in `47a2807` |
| `F-WIN-01/05/06` & `F-MAC-04/05` | `CT-01`, `diagnostics` | Shared `HKCU` `zalo://` URI, shared `AppUserModelId("com.vng.zalo")` / `CFBundleIdentifier`, global `PipeZCall*` / `/tmp/socketzalo*2021` | `BLOCKED` | **`BLOCKED`** *(vendor/OS boundaries documented)* | `c796fdf`, `8a7861d` |

---

## 5. Root Causes

1. **Metadata Race & Non-Atomic Staging (`DEFECT-04b`):** Baseline `JsonProfileStore` wrote to a single fixed `profiles.json.tmp` without cross-process OS locking or `File::sync_all()`, allowing concurrent processes/threads to clobber `.tmp` or lose updates across read-modify-write cycles.
2. **Corrupted Metadata Silent Reset / Crash (`DEFECT-08c`):** `dim-host.ps1` treated `0`-byte `profiles.json` as an empty profile list (`ok: true`) and threw `UNHANDLED_EXCEPTION` on malformed `{corrupt` JSON instead of failing closed with `JSON_ERROR` while preserving the file untouched.
3. **Pre-Lock Startup Blind Window (`DEFECT-03`):** Between `CreateProcessW` / `Command::spawn` and Electron's creation of `ZaloData/lockfile` / `SingletonLock`, no verified process lease existed on disk; calling `stop_profile` during a slow startup returned `0` and left an orphaned child process behind (`LC-07`).
4. **Concurrent Same-Profile Launch Race & Distinct-Profile Admission TOCTOU (`DEFECT-04a`, `AUDIT-43-02`):** Parallel `launch_profile` calls on the same profile lacked per-profile mutex/lockfile coordination (`LC-08`), while parallel launches of *distinct* profiles ran `DUPLICATE_ZALO_USER_ID` and `SHARED_EXTERNAL_DB_PATH` checks before either profile wrote `.runtime-lease.json`.
5. **Unsafe Direct Store Deletion & Lockfile Resurrection (`DEFECT-09`, `AUDIT-43-01`):** Calling `ProfileStore::delete_profile` directly bypassed `InstanceCoordinator` runtime checks (`PM-08`), and placing `.dim-profile.lock` *inside* `profile_dir` allowed a contending OS process to either resurrect `profile_dir` via `create_dir_all` after deletion or hold an open file handle inside `profile_dir` causing NTFS `ERROR_DIR_NOT_EMPTY` (`os error 145`) mid-delete.
6. **Parallel Win32 `RestartManager` & Handle-Inheritance Interference (`DEFECT-C2-RM-01`, `DEFECT-XF-WIN-RACE`):** Concurrent `RmStartSession` sessions in one process cross-contaminated locking-PID results; recycled Windows parent PIDs needed causal `GetProcessTimes` (`child_start >= parent_start`) filtering; `xf-fixture.mjs` needed pre-lock stale `.quit-signal` clearing and bounded Windows NTFS `EPERM` retry on `fs.renameSync`; and `XP-WIN-03`'s `bInheritHandle = 1` probe needed single-process isolation when sibling `cargo test` threads spawned piped `Command::output()` subprocesses.

---

## 6. Fixes Implemented

1. **Cross-Process Exclusive Metadata Locking & Atomic Replacement (`crates/core/src/fs_lock.rs`, `crates/profile-store/src/store.rs`, `scripts/dim-host.ps1`):**
   - Exclusive OS advisory lock `<data_dir>/profiles.lock` (`LockFileEx` `LOCKFILE_EXCLUSIVE_LOCK` on Windows, `flock(LOCK_EX)` on Unix) held across every read-modify-write operation.
   - Unique same-directory temp file (`profiles.json.<pid>.<counter>.<ts>.tmp`), `write_all` + `File::sync_all()`, handle close, and atomic replacement (`MoveFileExW(..., MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)` on Windows; `fs::rename` + parent dir sync on Unix), with automatic `.tmp` rollback on error.
2. **Fail-Closed Corrupted Metadata Handling (`store.rs`, `dim-host.ps1`):**
   - Missing `profiles.json` initializes a clean `schema_version: 1` document; existing `0`-byte, whitespace-only, invalid UTF-8, invalid JSON (`JSON_ERROR`), or unsupported `schema_version` (`SCHEMA_ERROR`) fails closed with recovery guidance and **never** overwrites or truncates `profiles.json`.
3. **Explicit `ProfileLifecycleState` & Anti-PID-Reuse Runtime Lease (`crates/core/src/{models.rs,lease.rs}`, `crates/platform-*`):**
   - Added `ProfileLifecycleState` (`Stopped`, `Starting`, `Running`, `Stopping`, `RecoveryRequired`).
   - Immediately after spawning, adapters atomically write `.runtime-lease.json` (`{ pid, process_create_time, executable_path, profile_id, spawned_at_utc }`). Ownership checks verify OS liveness, creation timestamp (`GetProcessTimes` / `/proc/<pid>/stat` ticks / `ps -o lstart=`), executable image path, and `protected_primary_pids` exclusion. Calling `stop_profile` during `Starting` cleanly terminates the verified lease tree (`LC-07`).
4. **Relocated Per-Profile OS Locks & Fast Pre-Spawn Admission Claims (`crates/core/src/{fs_lock.rs,lease.rs}`, `crates/process-manager/src/coordinator.rs`, `scripts/dim-host.ps1`):**
   - Moved per-profile advisory locks out of `profile_dir` into `<data_dir>/locks/<profile_id>.lock` (`AUDIT-43-01`) with `remove_dir_with_retry`, completely eliminating directory resurrection and `ERROR_DIR_NOT_EMPTY`.
   - Added `<data_dir>/locks/_launch_admission.lock` + `claim_<id>.json` + in-memory `in_flight_claims` (`AUDIT-43-02`) held for `<5ms` pre-spawn and released before `adapter.launch()` readiness polling so distinct profiles launch in parallel without TOCTOU races.
5. **Compile-Time & Runtime `VerifiedDeletionState` (`crates/core/src/traits.rs`, `store.rs`, `coordinator.rs`):**
   - Replaced bare boolean deletion flags with `VerifiedDeletionState` (`ConfirmedStopped`, `ActiveOrStarting`, `Stopping`, `Indeterminate`) plus independent lockfile/lease/marker defense-in-depth inside `JsonProfileStore` and two-phase `verify_zero_reparse_points_remaining` before recursive file removal.
6. **Platform Adapter Reconciliation & Fixes (`crates/platform-macos`, `crates/platform-linux`):**
   - Merged `feat/macos-multi-instance` (`f2b8f9f`) and `feat/ubuntu-multi-instance` (`62dc5d1`), reconciled `VerifiedDeletionState` and `Starting` status labels, added deterministic FNV-1a `--user-id` derivation and `/proc/<pid>/fd` open SQLite DB/WAL `SIGKILL` protection in `crates/platform-linux/src/adapter.rs` (`004ab7c`), and preserved `OFFICIAL_ZALO_UNSUPPORTED` on Linux.
7. **Selective Packaging IR Decisions & RFC 7396 Overlay Fix (`ec46209`..`20d1729`):**
   - Accepted `IR-002` (macOS `.icns` + `@2x` retina PNGs in `tauri.conf.json`), partial `IR-004` (`cargo-deny 0.21` `deny.toml` + `tauri-cli = "2.12.5"` metadata), and `IR-007` (`ci.yml` `--locked` + `libayatana-appindicator3-dev`). Rejected `IR-001` (kept root workspace `rust-version = "1.85"` for C2 compatibility while `src-tauri` uses `1.90`), `IR-005`, and `IR-006`; deferred `IR-003` until remote runner verification; stripped all JSON arrays from `tauri-ci-bundle-overlay.json` to prevent RFC 7396 array clobbering (`selftest.mjs` `16/16 PASS`).

---

## 7. Baseline Defect Reproductions Recorded

Before applying P0–P2 product fixes on `a5feb4a` / `155efc3`, the C2 harness reproduced all host-observable defects deterministically (`plans/reports/reliability/defect-reproductions.json`, SHA-256 `787b55c7a376f8932c11a828b6d4144c86f1c6e18f82a993f9c626964e09577b`):

- **`LC-08` (`DEFECT-04a`):** `FAIL_DEFECT_REPRODUCED` (exit `2`) — 4 concurrent `launch_profile` calls spawned colliding processes (`second_instance_collisions_logged = 2`).
- **`LC-07` (`DEFECT-03`):** `FAIL_DEFECT_REPRODUCED` (exit `2`) — `stop_profile` during `--slow-startup-pre-lock-ms=3100` returned `0` and left stealth orphan child alive after stop.
- **`PM-08` (`DEFECT-09`):** `FAIL_DEFECT_REPRODUCED` (exit `2`) — `<JsonProfileStore as ProfileStore>::delete_profile` deleted an actively running profile (`trait_delete_profile_bypassed_running_guard = true`).
- **`CT-10` (`DEFECT-08c`):** `FAIL_DEFECT_REPRODUCED` (exit `2`) — `dim-host.ps1` returned `ok: true` on `0`-byte `profiles.json`, `UNHANDLED_EXCEPTION` on `{corrupt`, and mismatched `MISSING_ARG` vs `INVALID_NAME`.
- **`LC-09` (`DEFECT-06`), `DS-08` (`DEFECT-07`), `CT-08` (`DEFECT-08a`), `CT-09` (`DEFECT-08b`), `PM-04` (`DEFECT-10`):** All 5 reproduced `FAIL_DEFECT_REPRODUCED` on unpatched `a5feb4a` and transitioned to `PASS` post-fix.

---

## 8. Regression Test Results at `bff0674444496bc057ba42fcc26c4c5b2d93dd7b`

All automated verification suites were executed on Windows 11 Pro x86_64 (`10.0.26300`, non-elevated `asInvoker`, `rustc 1.100.0`, `node v26.13.1`) using isolated temporary `DIM_DATA_DIR` sandboxes:

| Verification Gate | Command | Result | Details |
| :--- | :--- | :---: | :--- |
| Formatting & Strict Clippy (All Manifests) | `cargo fmt --check` + `cargo clippy --all-targets -- -D warnings` (workspace, `src-tauri`, `tests/cross-platform`) | **PASS (`0`)** | `0` warnings across all 9 crates |
| Workspace Unit & OS Adapter Suites | `cargo test --workspace --locked` | **PASS (`103/103`)** | `dim-core` (`9`), `dim-profile-store` (`9` incl. 4-subprocess lock stress), `dim-process-manager` (`14` incl. 2-subprocess delete/launch race & admission TOCTOU), `dim-platform-windows` (`9` incl. `TC-H2`, `TC-H6`, `TC-H7`), `dim-platform-macos` (`14`), `dim-platform-linux` (`46`), `dim-test-fixture` (`2`) |
| Tauri Async State Concurrency Tests | `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --locked` | **PASS (`2/2`)** | Same-profile async launch coalescing + parallel distinct-profile launch |
| C2 55-Case Release-Regression Gate | `xf-test-runner --mode release-regression --priority all --cycles 25 --skip-metrics --seed 0x20261117` | **PASS (`0`)** | `RELEASE_GATE_PASSED`: **`53/55 PASS`**, `11/11` `fixed_candidates`, `2` `SKIPPED_HOST_PLATFORM` (`XP-MAC-04`, `XP-LNX-04`), `0` `defects_reproduced`, `0` `unexpected_fail` |
| C2 Multi-Threaded Cargo Integration Runner | `cargo test --manifest-path tests/cross-platform/Cargo.toml --release --locked` | **PASS (`7/7`)** | `suite_01`..`suite_07` running concurrently in parallel (`0` failures) |
| PowerShell vs Rust CLI Differential Parity | `node tests/run-contract-parity-tests.mjs` | **PASS (`0`)** | `ok: true`; error codes, Unicode slugs, corrupt JSON fail-closed parity |
| Legacy Fixture & Bridge Security Suite | `node tests/run-fixture-tests.mjs` | **PASS (`8/8`)** | Includes `401` missing/bad `X-DIM-Session-Token`, `403` cross-origin, `415` content-type, `FORBIDDEN_DEST`, traversal rejection, primary Zalo `11` PIDs untouched |
| Account Dock UI & i18n Contract Tests | `node --test apps/desktop/ui/tests/i18n.test.mjs apps/desktop/ui/tests/ui-contracts.test.mjs` | **PASS (`21/21`)** | Bilingual `vi-VN`/`en-US` key parity, all 5 `lifecycle_state` mappings, color persistence, XSS/CSP checks |
| Headless Browser Visual CDP Audit | `node apps/desktop/ui/tests/visual-cdp.mjs` | **PASS (`0`)** | Light/dark/locale layouts verified with `0` console/runtime errors |
| Real Native `dim-desktop.exe` Win32 + WebView2 E2E | `node tests/run-tauri-native-e2e.mjs` | **PASS (`12/12`)** | Real Tauri IPC (`window.__TAURI_INTERNALS__ === true` on `http://tauri.localhost`), restart persistence, `0` runtime errors |
| CI Workflow & Supply-Chain Self-Tests | `node scripts/ci-cross-platform/selftest.mjs` & `validate-workflow.mjs` | **PASS (`16/16` & `6/6`)** | `cargo-deny` 0 advisories across 439 locked crates |

---

## 9. Tests Not Executed (Epistemic Honesty Disclosure)

1. **Windows Live 2-Account Authenticated `L3` Qualification (`TC-H1B` Stage 1 / `F-WIN-07`):** Real QR/password login with two live user Zalo accounts, live encrypted chat/file isolation, and live `ZaloCall.exe` 2-account call handoff (`TC-H1B` Stage 2 / `TC-H1A`) are strictly human-opt-in (`scripts/run-interactive-l3-checklist.ps1`) and were **not** automated.
2. **Native macOS 14/15 Kernel Execution (`XP-MAC-04`, `TC-H3..H5`, `run-macos-fixture-tests.mjs`):** Host OS is Windows 11; `XP-MAC-04` was honestly skipped (`SKIPPED_HOST_PLATFORM`), and `dim-platform-macos` runtime behavior remains **`FIXED` (awaiting native `macos-15` / `macos-15-intel` runner execution)**, not `VERIFIED`.
3. **Native Ubuntu 22.04/24.04 Kernel Execution (`XP-LNX-04`, `TC-H8A..8C`):** Live Linux `bwrap` user-namespace mounting, `/proc/<pid>/stat` kernel ticks, X11 `wmctrl`/`xdotool`, and Ubuntu 24.04 `apparmor_restrict_unprivileged_userns` runtime probes require a native `ubuntu-24.04` kernel (`XP-LNX-04` `SKIPPED_HOST_PLATFORM`); Ubuntu adapter runtime items remain **`FIXED`**, not `VERIFIED`.
4. **Live Vendor In-App Auto-Updater Soak (`L5` / `F-CORE-02`):** Live VNG `Update.exe` / `Squirrel.Mac` version swap across an upstream Zalo release was not executed (mitigated operationally via `stop-all`).

---

## 10. Open Defects & Architectural Limitations

### 10.1 Permanent Architectural / Vendor Boundaries (`BLOCKED`)
- **Official VNG Zalo on Ubuntu/Linux (`F-UBU-01`):** Permanently `UNSUPPORTED` (`OFFICIAL_ZALO_UNSUPPORTED`) because VNG ships no official Linux client and `app.asar` throws `Error("Unsupported OS: linux")`. Linux support is strictly limited to synthetic fixtures and user-supplied local Electron/AppImage builds (`EXPERIMENTAL`).
- **Simultaneous Multi-Account Voice/Video Calls (`F-WIN-06`, `F-MAC-05`, `F-UBU-07`):** `ZaloCall` binds global IPC endpoints (`\\.\pipe\PipeZCallRecv`/`Send` on Windows, `/tmp/socketzalo*2021` on macOS, loopback `29631/29632` + single-opener V4L2 `/dev/video0` on Linux community ports). Hooking pipes or patching `app.asar` is forbidden.
- **OS Notification Center & `zalo://` Protocol Grouping (`F-WIN-01`, `F-WIN-05`, `F-MAC-04`):** Official Zalo hardcodes `AppUserModelId("com.vng.zalo")` / `CFBundleIdentifier("com.vng.zalo")` and writes `HKCU\Software\Classes\zalo`. Cloning/re-signing `Zalo.app` or modifying `Zalo.lnk` is forbidden; users are guided to enable Zalo's per-window floating banner (`notiWin`) on Windows and per-instance Dock badges on macOS.

### 10.2 Remaining Non-Blocking POSIX Experimental Backlog (`OPEN` `P1`/`P2`)
- **`F-UBU-05` (`P1`, Ubuntu Wayland GUI):** `LinuxPlatformAdapter::focus_or_restore` handles `NodeFixture` `.focus-signal` and X11 `wmctrl`/`xdotool`, and currently fails closed with `UnsupportedPlatform` on Wayland GUI targets pending isolated `CustomExecutable` `second-instance` re-exec verification on native Ubuntu Wayland.
- **`IR-MAC-04` & `IR-UBU-05` (`P2` Wave 3 Diagnostics):** Optional `lsof -U` socket-owner PID reporting on macOS and passive `/proc/net/tcp` `29631/29632` + optional rootless `pasta` netns wrapper on Ubuntu remain Wave 3 enhancements.
- **`IR-TAURI-01` (macOS Production Publisher Pinning):** `MacOsPlatformAdapter::new()` defaults fail-closed (`PinnedPublisherPolicy == None` -> `UNVERIFIED_BINARY` for `OfficialZalo`) until VNG's 10-character Apple Developer `TeamIdentifier` is captured via `codesign -dv` on physical macOS hardware.

---

## 11. API & Storage Contract Changes

1. **`ProfileLifecycleState` (`crates/core/src/models.rs`):** Added `stopped | starting | running | stopping | recovery_required` serialized as `lifecycle_state` in `get_status` (Tauri IPC & HTTP bridge) alongside backward-compatible `running: bool`.
2. **`VerifiedDeletionState` (`crates/core/src/traits.rs`):** Replaced boolean running flags on `ProfileStore::delete_profile_verified` with explicit `VerifiedDeletionState` (`ConfirmedStopped`, `ActiveOrStarting`, `Stopping`, `Indeterminate`).
3. **Lock & Lease File Layout (`<data_dir>` & `<profile_dir>`):**
   - `<data_dir>/profiles.lock` — cross-process exclusive metadata lock.
   - `<data_dir>/locks/<profile_id>.lock` — per-profile cross-process lifecycle/delete serialization lock (outside `profile_dir` so deletion never hits `ERROR_DIR_NOT_EMPTY` or resurrects directories).
   - `<data_dir>/locks/_launch_admission.lock` + `claim_<id>.json` — `<5ms` pre-spawn cross-profile UID/DB admission claim.
   - `<profile_dir>/.runtime-lease.json` — anti-PID-reuse process ownership lease.
4. **HTTP Bridge Session Authentication (`apps/desktop/ui-server.mjs`, `apps/desktop/ui/app.js`):** Loopback HTTP bridge generates a 256-bit random per-session token injected via `<meta name="dim-session-token">` and verified via `crypto.timingSafeEqual` on `X-DIM-Session-Token` (`401 UNAUTHORIZED` on missing/mismatched token).
5. **Optional Fixture Target Env Override (`apps/desktop/src-tauri/src/commands.rs`):** `DIM_NODE_FIXTURE_SCRIPT` selects `LaunchTarget::NodeFixture` for isolated native Tauri E2E testing while production defaults strictly to `LaunchTarget::OfficialZalo`.

---

## 12. Account Dock Integration Result

- Merged `feat/ui-vi-en-quiet-workspace` (`1867beb`) **after** P0 backend fixes and extended `apps/desktop/ui/app.js` + `i18n/{vi-VN,en-US}.json` (`bff0674444496bc057ba42fcc26c4c5b2d93dd7b`) to render all 5 `ProfileLifecycleState` states (`stopped`, `starting`, `running`, `stopping`, `recovery_required`):
  - `Starting` displays localized `Đang khởi động Zalo…` / `Starting Zalo…`, disables `Focus`, enables `Stop` (`LC-07` parity), and disables offline-only overflow actions (`Backup` / `Delete`).
  - `Stopping` disables both `Focus` and `Stop` until clean exit completes.
  - `RecoveryRequired` surfaces localized warning state and blocks offline `Backup` / `Delete` until stale/corrupt state is resolved.
- Verified `100%` bilingual dictionary parity (`i18n.test.mjs`), `21/21` UI contract tests (`ui-contracts.test.mjs`), `8/8` HTTP bridge + security fixture tests (`run-fixture-tests.mjs`), and `visual-cdp.mjs` (`0` runtime errors).

---

## 13. Real Windows Native Tauri (`dim-desktop.exe`) E2E Result

Executed `node tests/run-tauri-native-e2e.mjs` against the compiled release binary `apps/desktop/src-tauri/target/release/dim-desktop.exe` (`22,675,554` B, SHA-256 `78332379c9ed702578ce3b0d9adc4690d913cd68ab86bb3631c6409669c813c2`) over WebView2 CDP (`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=<port>"`) with isolated `<TEMP_E2E_ROOT>/dim-data` and `DIM_NODE_FIXTURE_SCRIPT=fixtures/cross-platform/xf-fixture.mjs`:

- **Structured Proof Artifact:** [`evidence/tauri-native-e2e-result.json`](../../evidence/tauri-native-e2e-result.json) (`ok: true`, `real_tauri_ipc_verified: true`, `passed_steps: 12`, `failed_steps: 0`, `webview_origin: "http://tauri.localhost"`, `has_tauri_internals: true`, `webview_runtime_errors: 0`).
- **All 12 Mandatory Steps Passed via Real Rust Tauri IPC (`window.__TAURI_INTERNALS__.invoke`):**
  1. Launched Win32 + WebView2 `dim-desktop.exe` (`http://tauri.localhost`, default `vi` locale).
  2. Verified `get_status`, `get_diagnostics` (`4` platform matrix rows), and `verify_isolation` (`7/7` isolation checks `PASS`, `11` live primary Zalo PIDs protected).
  3. Created profile `Đức — E2E Tauri Thật` (`duc-e2e-tauri-that_f164a947`) with `coral` identity swatch via Account Dock dialog.
  4. Renamed profile to `Đức — Tauri Đã Đổi Tên` via overflow menu and verified `directory_name` preservation.
  5. Launched isolated `xf-fixture.mjs` instance via Account Dock button (`lifecycle_state: "running"`, deterministic UID `"6950471"`).
  6. Focused running instance via Account Dock button (`focus_count: 1` verified in `fixture-state.json`).
  7. Stopped instance via Account Dock button (`clean_shutdown: true`, PID terminated, `.runtime-lease.json` removed).
  8. Created offline atomic SHA-256 backup (`13` files, `4,440` B, `backup.json` verified).
  9. Restored backup into brand-new isolated profile `Đức — Khôi Phục E2E` (`2` total profiles).
  10. Deleted restored profile via confirmation dialog and verified directory removal while survivor remained intact.
  11. Switched UI to `dark` theme + `en-US` locale, terminated `dim-desktop.exe`, spawned a fresh OS `dim-desktop.exe` process on the same `DIM_DATA_DIR`, and verified persistence of profile name, UID `"6950471"`, `var(--id-coral)` avatar color, `dark` theme, `en` locale, and backup count.
  12. Verified backend error guards (`DUPLICATE_NAME` on lowercase Vietnamese `"đức — tauri đã đổi tên"`, `INVALID_NAME` on whitespace/70-char names, `PROFILE_RUNNING` on `delete_profile(confirm: true)` while running, `CONFIRM_REQUIRED` on `confirm: false`, and `stop_all_profiles` stopping `1` secondary instance cleanly).

---

## 14. macOS & Ubuntu Integration Status

- **`feat/macos-multi-instance` (`f2b8f9f` -> `8a7861d` + `0aee2db`):** Cleanly integrated into workspace and `src-tauri` `cfg(target_os = "macos")` targets. Compiles with `0` warnings and passes all `14/14` portable unit + scenario tests (`macos_scenarios.rs`) plus C2 `XP-MAC-01..03` static/contract checks on Windows. Runtime classification: **`FIXED` (awaiting native `macos-15` / `macos-15-intel` execution)**.
- **`feat/ubuntu-multi-instance` (`62dc5d1` -> `47a2807` + `004ab7c`):** Cleanly integrated with deterministic FNV-1a `--user-id` fixture injection and `/proc/<pid>/fd` open SQLite DB/WAL check prior to `SIGKILL`. Compiles with `0` warnings and passes all `46/46` unit + `ubuntu_adapter_suite` tests plus C2 `XP-LNX-01..03` static/contract checks on Windows. **`OFFICIAL_ZALO_UNSUPPORTED` (`SupportStatus::Unsupported`) is strictly preserved for official Zalo on Linux**, and `--no-sandbox` remains hard-blocked (`ForbiddenNoSandboxFlag`). Runtime classification: **`FIXED` (awaiting native `ubuntu-24.04` execution)**.

---

## 15. Security Findings & Remediations (`ak-security` + Independent Audit `#43`)

- **Binary & Data Safety:** `0` proprietary VNG binaries or user credentials tracked; `WinVerifyTrust` + `CN=VNG GROUP JSC` subject check on Windows; 5-tier codesign + fail-closed `PinnedPublisherPolicy` on macOS; `PrimaryProtectionGuard` protecting all `11` live primary Zalo processes throughout every test run (`100%` unchanged `%APPDATA%\ZaloData` SHA-256).
- **WebView / Bridge Hardening:** Strict Tauri CSP (`default-src 'self'`, zero `unsafe-inline`/`unsafe-eval` scripts), zero `innerHTML`/`eval` sinks in `apps/desktop/ui`, loopback-only `127.0.0.1` binding + `X-DIM-Session-Token` (`timingSafeEqual`) + Origin/Host validation in `ui-server.mjs`.
- **Audit `#43` Closure (`e602741` & `bff0674444496bc057ba42fcc26c4c5b2d93dd7b`):**
  1. Relocated per-profile OS locks to `<data_dir>/locks/<id>.lock` (`0` directory resurrections, `0` `ERROR_DIR_NOT_EMPTY` failures across 2-subprocess race tests).
  2. Closed simultaneous distinct-profile launch TOCTOU via `LaunchAdmissionClaim`.
  3. Routed Tauri `get_status` through `InstanceCoordinator` for accurate `Stopping` visibility.
  4. Scrubbed all local workstation paths (`<REPO_ROOT>`, `<WORKTREE_ROOT>`, `<USERPROFILE>`) across tracked documentation and evidence JSONs (`0` matches outside repository URL `github.com/manhquydev/zalo`).
- **Supply Chain & CI Security:** `0` GitHub Actions `${{ github.event.* }}` script-injection sinks, `permissions: contents: read`, 34 SHA-pinned official actions, and `cargo-deny 0.21` passing with `0` advisories across 439 crates.

---

## 16. CI & GitHub Sync Status

- **Remote Repository:** `origin` -> `https://github.com/manhquydev/zalo.git`
- **Local CI Validation:** `.github/workflows/cross-platform-validation.yml` (`6/6` `validate-workflow.mjs` checks `PASS`) and `scripts/ci-cross-platform/selftest.mjs` (`16/16` checks `PASS`) verified locally.
- **GitHub Push & Remote Execution Gate (`GATED_MISSING_OAUTH_WORKFLOW_SCOPE`):**
  - `gh auth status` is authenticated as `manhquydev` over HTTPS with token scopes `'gist', 'read:org', 'repo'` (**missing `'workflow'` scope**).
  - Because `integration/reliability-hardening` adds/updates `.github/workflows/cross-platform-validation.yml` and `.github/workflows/ci.yml`, GitHub's HTTPS OAuth receive hook rejects pushes modifying `.github/workflows/*` until the user grants `workflow` scope (`gh auth refresh -h github.com -s workflow`).
  - Additionally, GitHub Actions requires a `workflow_dispatch` workflow file to exist on the repository's default branch (`origin/main`) before manual runs can be triggered, and per mission rules `integration/reliability-hardening` was **not** merged into `main` or force-pushed.
  - Exact non-force push check result: `git push -u origin integration/reliability-hardening returned ! [remote rejected] integration/reliability-hardening -> integration/reliability-hardening (refusing to allow an OAuth App to create or update workflow .github/workflows/ci.yml without workflow scope) (exit ). Re-run gh auth refresh -h github.com -s workflow followed by git push -u origin integration/reliability-hardening (no --force required).`

---

## 17. Final Git Commit SHA

- **Verified Code, Native Tauri E2E Evidence & Path-Scrub Commit:** `bff0674444496bc057ba42fcc26c4c5b2d93dd7b`
- **Final Branch Tip (`HEAD` on `integration/reliability-hardening`):** Recorded as the parent + tip commit on `integration/reliability-hardening` (`git log -n 2` in `<WORKTREE_ROOT>/zalo-integration`; since a Git commit cannot embed its own SHA-1 digest, `bff0674444496bc057ba42fcc26c4c5b2d93dd7b` is the exact code/evidence tree and its direct child commit updates only `docs/reviews/{integration-and-defect-remediation.md,integration-defect-ledger.md}`).
