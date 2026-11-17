# Mission C2 — Actionable Regression Handoff & Controlled Gate Specification

**Worktree / Branch:** `test/cross-platform-reliability`  
**Baseline Test Harness SHA:** `a2143550935d75c1569123e628eccb3358654834`  
**Product Baseline SHA (`main`):** `a5feb4a520798064c1508c26fe4c4ac17a3a03cf`  
**Baseline Evidence Timestamp (UTC):** `2026-11-17T17:07:34Z` (`seed = 0x20261117`)  
**Host Provenance:** Windows 11 Pro x86_64 (`Windows 10.0.26200` / kernel build `10.0.26300.9974`, non-elevated `asInvoker`), `rustc 1.100.0`, `node v26.13.1`  
**Companion Artifacts:**
- Frozen Baseline Manifest (`SHA-256: 8365d25f9658252a8085added689ec67d6898688e0ca979a575103e86bedbf58`, `96,947` B): [`plans/reports/reliability/baseline-manifest.json`](../../../plans/reports/reliability/baseline-manifest.json)
- Post-Fix Verification Ledger & Protocol: [`post-fix-verification.md`](./post-fix-verification.md)
- Frozen Windows Host Evidence (`SHA-256: e895ad959568f7bccd10f12ff1656fb50e037e17c32fe5db3b4f9c41cf1ed7b9`, `52,991` B): [`plans/reports/reliability/windows-host-execution-evidence.json`](../../../plans/reports/reliability/windows-host-execution-evidence.json)
- Frozen Defect Catalog (`SHA-256: 787b55c7a376f8932c11a828b6d4144c86f1c6e18f82a993f9e626964e09577b`, `11,030` B): [`plans/reports/reliability/defect-reproductions.json`](../../../plans/reports/reliability/defect-reproductions.json)
- Frozen Telemetry Benchmark (`SHA-256: bed0c193b7b6c82f2c45f7821c9bd9e033ebf3b6354e09a38bd7998dc2613ab4`, `3,140` B): [`plans/reports/reliability/metrics-benchmark-windows.json`](../../../plans/reports/reliability/metrics-benchmark-windows.json)

---

## 1. Quick-Start Reproduction Commands (Zero-Prose Entrypoint)

All commands below execute from the `desktop-instance-manager/` directory inside an isolated temporary sandbox (`IsolatedSandbox` `DIM_DATA_DIR`) without touching `%APPDATA%\ZaloData`, `~/Library/Application Support/ZaloData`, `~/.config/ZaloData`, or any running primary `Zalo*` processes, and **without modifying** frozen C1 JSON artifacts unless `--report-out <path>` and `--overwrite-baseline-reports` are explicitly supplied.

### 1.1 One-Time Prerequisite Build (Synthetic Fixture + Native CLI)

```bash
# Build native synthetic desktop fixture (xf-desktop-fixture / xf-desktop-fixture.exe)
cargo build --manifest-path fixtures/cross-platform/Cargo.toml --release

# Build native Rust CLI (dim-cli / dim-cli.exe) for differential contract checks (CT-07..CT-10)
cargo build -p dim-test-fixture --bin dim-cli --release --locked
```

### 1.2 Priority-Tier Release Regression Gates (Exits `2` While Unfixed; Must Exit `0` Post-Fix)

```bash
# P0 Release-Blocker Gate (4 cases: LC-07, LC-08, PM-08, CT-10)
cargo run --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner -- --mode release-regression --priority p0

# P1 Cross-Platform & Parity Gate (6 cases: LC-09, DS-08, XP-MAC-03, XP-LNX-02, CT-08, CT-09)
cargo run --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner -- --mode release-regression --priority p1

# Combined P0 + P1 Mandatory Gate (10 cases)
cargo run --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner -- --mode release-regression --priority p0-p1

# P2 Unicode Parity Gate (1 case: PM-04)
cargo run --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner -- --mode release-regression --priority p2

# Passing Regression-Guard Verification (44 non-defect baseline cases)
cargo run --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner -- --mode release-regression --priority regression-guard
```

### 1.3 Fast Single-Case Reproducers (Seconds Per Bug)

```bash
# Canonical single-case reproduction command (matches baseline-manifest.json):
cargo run --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner -- --mode release-regression --case LC-08
cargo run --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner -- --mode release-regression --case LC-07
cargo run --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner -- --mode release-regression --case PM-08
cargo run --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner -- --mode release-regression --case CT-10

# Comma-separated multi-case selector (case-insensitive):
cargo run --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner -- --mode release-regression --case LC-09,XP-LNX-02,XP-MAC-03,DS-08,CT-08,CT-09,PM-04

# Native-OS-mandatory runtime gates (exits 3 on non-matching host OS):
cargo run --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner -- --mode release-regression --require-native-os --case XP-MAC-04
cargo run --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner -- --mode release-regression --require-native-os --case XP-LNX-04
```

### 1.4 Cargo Test Mandatory Post-Fix Gates (`tests/integration_entry.rs`)

```bash
cargo test --manifest-path tests/cross-platform/Cargo.toml --release -- --ignored release_gate_p0_mandatory --nocapture
cargo test --manifest-path tests/cross-platform/Cargo.toml --release -- --ignored release_gate_p0_p1_mandatory --nocapture
cargo test --manifest-path tests/cross-platform/Cargo.toml --release -- --ignored strict_zero_product_defects_gate --nocapture
```

---

## 2. Prioritized Defect Handoff Cards (`P0` -> `P1` -> `P2`)

Every defect handoff card below contains all **7 mandatory engineering fields** required for deterministic reproduction, surgical remediation, bystander safety, and post-fix verification.

---

### 2.1 Tier `P0` — Release Blockers (Concurrency Races, Stealth Orphan Processes, & Store Safety)

#### [P0-1] `LC-08` (`DEFECT-04a`) — Concurrent Multi-Caller Launch Race in `InstanceCoordinator::launch_profile`

1. **Shortest reproduction command:**
   ```bash
   cargo run --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner -- --mode release-regression --case LC-08
   ```
2. **Input fixture & arguments:**
   - Fixture: `LaunchTarget::NodeFixture` (`fixtures/cross-platform/xf-fixture.mjs`)
   - Arguments: `["--user-id=980001", "--slow-startup-pre-lock-ms=220"]`
   - Driver: `4` `std::thread` callers synchronized on an `Arc<Barrier>` (`Barrier::new(4)`) invoking `InstanceCoordinator::launch_profile(&prof.id, &target)` simultaneously on the same stopped profile alongside an untouched bystander profile in the same `IsolatedSandbox`.
3. **Expected behavior:**
   - Per-profile launch execution is serialized (in-process mutex/lock-map keyed by `profile.id` **plus** cross-process per-profile advisory lockfile inside `<profile_dir>`).
   - Exactly `1` thread spawns the OS child process; the remaining `3` concurrent callers observe the in-flight/running instance and route idempotently to `focus_or_restore` (or return the single active `ProfileRuntimeState`), logging `0` `SECOND_INSTANCE_REJECTED` collisions in `ZaloData/startup.log`.
4. **Actual behavior:**
   - `InstanceCoordinator` holds zero synchronization around `inspect_profile` (`coordinator.rs:75`) -> `adapter.launch` (`coordinator.rs:95`). All `4` threads simultaneously observe `runtime_before.running == false` before `lockfile` is created and spawn `4` racing `node` OS processes against the same `ZaloData` directory.
5. **Concrete evidence (values + JSON path):**
   - `plans/reports/reliability/windows-host-execution-evidence.json` -> `$.outcomes[7]` (`id == "LC-08"`, `verdict == "FAIL_DEFECT_REPRODUCED"`, `duration_ms == 1420`):
     - `$.outcomes[7].evidence.concurrent_threads`: `4`
     - `$.outcomes[7].evidence.returned_pids`: `[5232, 5232, 5232, 5232]`
     - `$.outcomes[7].evidence.second_instance_collisions_logged`: `2` (`>= 2` racing duplicate spawns rejected by fixture lockfile check)
   - `plans/reports/reliability/defect-reproductions.json` -> `$.defects[3]` (`defect_id == "DEFECT-04"`).
6. **Implicated product files & exact functions/line ranges:**
   - [`crates/process-manager/src/coordinator.rs`](../../../crates/process-manager/src/coordinator.rs#L20-L24) (`struct InstanceCoordinator<A>` — lacks per-profile lock map)
   - [`crates/process-manager/src/coordinator.rs`](../../../crates/process-manager/src/coordinator.rs#L49-L104) (`InstanceCoordinator::launch_profile` — unguarded check-then-act window between `self.adapter.inspect_profile` at `L75` and `self.adapter.launch` at `L95`)
   - [`scripts/dim-host.ps1`](../../../scripts/dim-host.ps1#L2055-L2165) (`HostEngine.LaunchProfile` — lacks cross-process per-profile mutex/lockfile guard)
7. **Fix verification & closure criteria:**
   - **Pre-fix -> Post-fix transition:** `--mode release-regression --case LC-08` transitions from `FAIL_DEFECT_REPRODUCED` (exit `2`) to `PASS` (exit `0`) with `second_instance_collisions_logged == 0` and a single unique OS PID spawned.
   - **Zero bystander profile impact:** Sibling bystander profile in `LC-08` retains identical `ProfileRecord` metadata, `.zalo-profile-id`, and runtime state before and after the 4-thread race.
   - **Adjacent non-regression:** `LC-01`, `LC-02`, `LC-06` (sequential duplicate launch focus routing), and `REL-05` (5 concurrent profiles heartbeating + live renames) all remain `PASS` without deadlock.

---

#### [P0-2] `LC-08` & Store Concurrency (`DEFECT-04b`) — Unscoped Concurrent `profiles.json.tmp` Metadata Write Collision

1. **Shortest reproduction command:**
   ```bash
   cargo run --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner -- --mode release-regression --case LC-08,PM-08
   ```
2. **Input fixture & arguments:**
   - Fixture / Store API: Concurrent invocations of `JsonProfileStore` mutators (`create_profile`, `rename_profile`, `update_last_launched` during `LC-08`) across separate `JsonProfileStore` instances / CLI processes (`dim-cli` and `scripts/dim-host.ps1`) targeting the same `--data-dir <DIM_DATA_DIR>`.
3. **Expected behavior:**
   - Cross-process mutations on `<data_dir>/profiles.json` acquire an OS advisory lockfile (e.g., `<data_dir>/profiles.lock` via `CreateFileW` exclusive share mode / `flock(LOCK_EX)`) before read-modify-write, **and** stage writes to a unique per-write temporary path (`profiles.json.<pid>.<nonce>.tmp`) before `fs::rename` / `MoveFileExW(MOVEFILE_REPLACE_EXISTING)`, preventing cross-process clobbering or lost updates.
4. **Actual behavior:**
   - `JsonProfileStore` only synchronizes via an in-process `std::sync::Mutex<()>` (`store.rs:22`), and `write_document_atomic_locked` hardcodes a single shared filename `self.data_dir.join("profiles.json.tmp")` (`store.rs:171`). Likewise, `scripts/dim-host.ps1` `SaveStore` hardcodes `_metadataFile + ".tmp"` (`dim-host.ps1:404`) under an in-process `lock (_storeLock)`. Concurrent CLI or Tauri + CLI invocations overwrite each other's `profiles.json.tmp` bytes mid-flush, fail `File::create` with sharing violations (`IO_ERROR`), or silently drop concurrent `update_last_launched` / `create_profile` records.
5. **Concrete evidence (values + JSON path):**
   - `plans/reports/reliability/windows-host-execution-evidence.json` -> `$.outcomes[7].affected_files[1]`: `"crates/profile-store/src/store.rs:169-180"` (`LC-08` / `DEFECT-04`).
   - `plans/reports/reliability/windows-host-execution-evidence.json` -> `$.outcomes[16].evidence` (`id == "PM-08"`): `atomic_tmp_block_error_code = "IO_ERROR"` when `profiles.json.tmp` is contended/blocked on disk.
   - `plans/reports/reliability/defect-reproductions.json` -> `$.defects[3]` (`defect_id == "DEFECT-04"`, summary & `affected_files`).
6. **Implicated product files & exact functions/line ranges:**
   - [`crates/profile-store/src/store.rs`](../../../crates/profile-store/src/store.rs#L17-L23) (`JsonProfileStore` struct — in-process `lock: Mutex<()>` only)
   - [`crates/profile-store/src/store.rs`](../../../crates/profile-store/src/store.rs#L169-L180) (`JsonProfileStore::write_document_atomic_locked` — hardcoded `let tmp_path = self.data_dir.join("profiles.json.tmp");`)
   - [`scripts/dim-host.ps1`](../../../scripts/dim-host.ps1#L401-L415) (`HostEngine.SaveStore` — hardcoded `string tmpPath = _metadataFile + ".tmp";` without cross-process mutex/lockfile)
7. **Fix verification & closure criteria:**
   - **Pre-fix -> Post-fix transition:** `LC-08` and `PM-08` both exit `0` under `--mode release-regression` (note: `PM-08` uses a Windows `FILE_SHARE_READ`-only handle on `profiles.json` so atomic `fs::rename` rollback is verified even after `DEFECT-04b` switches to PID/nonce-scoped `.tmp` files!); zero leftover `profiles.json*.tmp` files remain in `<data_dir>` after failed or concurrent mutations.
   - **Zero bystander profile impact:** Existing profiles in `profiles.json` never suffer lost updates or schema corruption when a sibling profile updates `last_launched_at` concurrently.
   - **Adjacent non-regression:** `DS-07` (`90` atomic mutations across `1,600+` concurrent reader checks with `0` torn JSON reads), `PM-02` (`25`-profile bulk creation), and `REL-05` (`25` live renames under 5-instance load) all `PASS`.

---

#### [P0-3] `LC-07` (`DEFECT-03`) — Slow Pre-Lock Startup Visibility Blind Window & Stealth Orphan Process on `stop_profile`

1. **Shortest reproduction command:**
   ```bash
   cargo run --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner -- --mode release-regression --case LC-07
   ```
2. **Input fixture & arguments:**
   - Fixture: `LaunchTarget::NodeFixture` (`fixtures/cross-platform/xf-fixture.mjs`)
   - Arguments: `["--user-id=970001", "--slow-startup-pre-lock-ms=3100"]`
   - Sequence: Call `coordinator.launch_profile(&prof.id, &slow_target)` followed immediately by `coordinator.stop_profile(&prof.id)` while a sibling bystander profile resides in the sandbox.
3. **Expected behavior:**
   - After spawning a child process and recording `.runtime-pid`, `inspect_profile` (across Windows, macOS, and Linux) recognizes a live, profile-owned `.runtime-pid` process during its pre-lock startup phase (verifying PID liveness and isolated profile ownership / non-primary PID) so `running == true` and `main_pid == Some(pid)`.
   - Additionally, `WindowsPlatformAdapter::launch` waits for a committed/locked `lockfile` (not merely `0`-byte `lockfile.exists()`) across all `LaunchTarget` variants.
   - Calling `stop_profile` during slow startup terminates the starting `.runtime-pid` process cleanly (`stop_count >= 1`), removes `.runtime-pid`, and leaves **`0` surviving background processes**.
4. **Actual behavior:**
   - `WindowsPlatformAdapter::launch` (`adapter.rs:620-629`) polls at most `2,500 ms` (only for `NodeFixture`; `0 ms` for `OfficialZalo`/`CustomExecutable`) and breaks early even on a `0`-byte `lockfile`.
   - `WindowsPlatformAdapter::inspect_profile` (`adapter.rs:512`) gates reading `.runtime-pid` behind `&& is_file_exclusively_locked(&lockfile)` (which is `false` before lock acquisition!), while `MacOsPlatformAdapter::inspect_profile` and `LinuxPlatformAdapter::inspect_profile` never read `.runtime-pid` at all.
   - Consequently, `launch_profile` returns `running: false, main_pid: None`. Immediately calling `stop_profile` hits `if !state.running { return Ok(0); }` (`coordinator.rs:113-120`), leaving PID `20012` alive in the background as an untracked stealth orphan that subsequently acquires `lockfile`!
5. **Concrete evidence (values + JSON path):**
   - `plans/reports/reliability/windows-host-execution-evidence.json` -> `$.outcomes[6]` (`id == "LC-07"`, `verdict == "FAIL_DEFECT_REPRODUCED"`, `duration_ms == 2865`):
     - `$.outcomes[6].evidence.launch_returned_running`: `false`
     - `$.outcomes[6].evidence.launch_returned_main_pid`: `null`
     - `$.outcomes[6].evidence.runtime_pid_hint_on_disk`: `20012`
     - `$.outcomes[6].evidence.stop_profile_terminated_count`: `0`
     - `$.outcomes[6].evidence.orphan_process_survived_stop`: `true`
   - Empirical calibration race (`docs/testing/cross-platform/reliability-report.md` §2.4): `1/20` (`5%`) rapid `NodeFixture` launches hit `0`-byte `lockfile.exists() == true` before JSON PID write.
6. **Implicated product files & exact functions/line ranges:**
   - [`crates/platform-windows/src/adapter.rs`](../../../crates/platform-windows/src/adapter.rs#L512-L522) (`WindowsPlatformAdapter::inspect_profile` — `if main_pid.is_none() && crate::win32_ffi::is_file_exclusively_locked(&lockfile)` blocks `.runtime-pid` fallback prior to lock acquisition)
   - [`crates/platform-windows/src/adapter.rs`](../../../crates/platform-windows/src/adapter.rs#L618-L631) (`WindowsPlatformAdapter::launch` — `2,500 ms` poll restricted to `NodeFixture` and exits on `0`-byte `lockfile.exists()`)
   - [`crates/process-manager/src/coordinator.rs`](../../../crates/process-manager/src/coordinator.rs#L107-L128) (`InstanceCoordinator::stop_profile` — returns `Ok(0)` at `L113-L120` when `!state.running`, or deletes `.runtime-pid` via `recover_stale_state` while child is still booting)
   - [`crates/platform-macos/src/adapter.rs`](../../../crates/platform-macos/src/adapter.rs#L192-L199) & [`L298-L320`](../../../crates/platform-macos/src/adapter.rs#L298-L320) (`inspect_profile` & `launch`)
   - [`crates/platform-linux/src/adapter.rs`](../../../crates/platform-linux/src/adapter.rs#L189-L195) & [`L298-L300`](../../../crates/platform-linux/src/adapter.rs#L298-L300) (`inspect_profile` & `launch`)
7. **Fix verification & closure criteria:**
   - **Pre-fix -> Post-fix transition:** `--mode release-regression --case LC-07` exits `0` (`PASS`) with `launch_returned_running == true`, `stop_profile_terminated_count >= 1`, and `orphan_process_survived_stop == false`.
   - **Zero bystander profile impact:** Active or stopped bystander profiles in the sandbox are unaffected; `REL-08` PID-reuse spoof guard (`recycled_self_pid_running_reported == false`) MUST still pass so recycled unrelated OS PIDs in stale `.runtime-pid` files are never mistaken for live Zalo instances or killed!
   - **Adjacent non-regression:** `LC-04` (unmanaged & primary PID protection), `REL-01` (10 restart cycles), `REL-02` (100 soak cycles), and `REL-08` all `PASS`.

---

#### [P0-4] `PM-08` (`DEFECT-09`) — Unsafe `ProfileStore::delete_profile` Trait Path Bypasses `PROFILE_RUNNING` Guard

1. **Shortest reproduction command:**
   ```bash
   cargo run --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner -- --mode release-regression --case PM-08
   ```
2. **Input fixture & arguments:**
   - Fixture: `LaunchTarget::NodeFixture` (`fixtures/cross-platform/xf-fixture.mjs`) with `["--user-id=985001"]` launched via `coordinator.launch_profile(&running_prof.id, &target)`, plus a stopped bystander profile `"Keep Intact On Failure"`.
   - Call under test: `dim_core::ProfileStore::delete_profile(&*sandbox.store, &running_prof.id, true)`.
3. **Expected behavior:**
   - Calling `ProfileStore::delete_profile` on a running profile MUST NOT silently assume `is_running = false` and strip live profile directories/metadata out from under an active process. Either `JsonProfileStore::delete_profile` inspects active lockfile/`.runtime-pid` artifacts (`lockfile`, `SingletonLock`, `.runtime-pid` liveness/exclusivity) and returns `Err(DimError::ProfileRunning(_))` (`PROFILE_RUNNING`), or fails closed requiring deletion through `InstanceCoordinator::delete_profile` / verified lock-free check before unlinking any files or mutating `profiles.json`.
4. **Actual behavior:**
   - `<JsonProfileStore as ProfileStore>::delete_profile` (`crates/profile-store/src/store.rs:271-273`) delegates to `self.delete_profile_with_guard(id_or_name, confirm, false, &unlink_reparse_points_safe)`, hardcoding `is_running = false`!
   - When invoked while the profile's fixture process is actively running, it removes the profile from `profiles.json` and deletes unlocked profile files out from under the running instance (`trait_delete_profile_bypassed_running_guard == true`).
5. **Concrete evidence (values + JSON path):**
   - `plans/reports/reliability/windows-host-execution-evidence.json` -> `$.outcomes[16]` (`id == "PM-08"`, `verdict == "FAIL_DEFECT_REPRODUCED"`, `duration_ms == 636`):
     - `$.outcomes[16].evidence.atomic_tmp_block_error_code`: `"IO_ERROR"`
     - `$.outcomes[16].evidence.profiles_json_preserved_on_write_error`: `true`
     - `$.outcomes[16].evidence.coordinator_delete_guard`: `"PROFILE_RUNNING"`
     - `$.outcomes[16].evidence.trait_delete_profile_bypassed_running_guard`: `true`
   - `plans/reports/reliability/defect-reproductions.json` -> `$.defects[7]` (`defect_id == "DEFECT-09"`).
6. **Implicated product files & exact functions/line ranges:**
   - [`crates/profile-store/src/store.rs`](../../../crates/profile-store/src/store.rs#L271-L273) (`<JsonProfileStore as ProfileStore>::delete_profile` — hardcodes `false` for `is_running`)
   - [`crates/profile-store/src/store.rs`](../../../crates/profile-store/src/store.rs#L110-L144) (`JsonProfileStore::delete_profile_with_guard` — deletes directory `L136-L140` before/without checking lock artifacts if `is_running == false`)
7. **Fix verification & closure criteria:**
   - **Pre-fix -> Post-fix transition:** `--mode release-regression --case PM-08` exits `0` (`PASS`); `ProfileStore::delete_profile(&store, &running_prof.id, true)` returns `Err(DimError::ProfileRunning(_))` (`code() == "PROFILE_RUNNING"`), keeping `running_prof` in `profiles.json` and its directory completely intact while running, and succeeding cleanly once the profile is stopped.
   - **Zero bystander profile impact:** Bystander profile `"Keep Intact On Failure"` remains untouched in `profiles.json` and on disk across both the atomic write-failure rollback probe and the running-delete rejection check.
   - **Adjacent non-regression:** `PM-01` (confirmed delete of stopped profile), `DS-02` (NTFS junction safe unlink on delete), and `LC-02` all `PASS`.

---

#### [P0-5] `CT-10` (`DEFECT-08c`) — Corrupted / `0`-Byte `profiles.json` Silent Reset & CLI Error Code Divergence

1. **Shortest reproduction command:**
   ```bash
   cargo run --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner -- --mode release-regression --case CT-10
   ```
2. **Input fixture & arguments:**
   - Binaries under differential test: `target/release/dim-cli.exe` vs `powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts/dim-host.ps1` with `--cli --json --data-dir <isolated_dir> --node-fixture --app-path fixtures/cross-platform/xf-fixture.mjs`.
   - Three fault inputs:
     1. Truncate `<data_dir>/profiles.json` to `0` bytes (`b""`) and run `status`.
     2. Write malformed JSON `b"{corrupt"` to `<data_dir>/profiles.json` and run `status`.
     3. Invoke `create` with missing `<name>` positional argument.
3. **Expected behavior:**
   - **Fail-closed on corruption/truncation:** Both `dim-cli` and `scripts/dim-host.ps1` MUST treat an existing `0`-byte/whitespace-only `profiles.json` or malformed `{corrupt` `profiles.json` as storage corruption, returning `ok: false` (exit code `1`) with `error_code: "JSON_ERROR"` — **never** silently resetting to an empty profile list (which risks overwriting recoverable metadata on the next `SaveStore`) and never leaking `UNHANDLED_EXCEPTION`.
   - **Canonical missing-argument error code:** Missing `<name>` on `create` returns an identical canonical `error_code` across both `dim-cli` and `scripts/dim-host.ps1`.
4. **Actual behavior:**
   - **0-byte `profiles.json`:** `dim-cli` fails closed (`ok: false, error_code: "JSON_ERROR"`), whereas `scripts/dim-host.ps1` (`LoadStore`, `L381-L384`) executes `if (string.IsNullOrWhiteSpace(raw)) { return new ProfileStoreFile(); }` and returns `ok: true, profiles: []`, silently masking truncated metadata!
   - **Malformed `{corrupt` `profiles.json`:** `dim-cli` returns `error_code: "JSON_ERROR"`, whereas `scripts/dim-host.ps1` (`L385` -> `catch` at `L3049-L3055`) catches `JavaScriptSerializer.Deserialize` `ArgumentException` at top-level and emits `error_code: "UNHANDLED_EXCEPTION"`.
   - **Missing `create` argument:** `dim-cli` (`dim-cli.rs:171-174`) emits `error_code: "INVALID_NAME"`, whereas `scripts/dim-host.ps1` (`L3006`) emits `error_code: "MISSING_ARG"`.
5. **Concrete evidence (values + JSON path):**
   - `plans/reports/reliability/windows-host-execution-evidence.json` -> `$.outcomes[54]` (`id == "CT-10"`, `verdict == "FAIL_DEFECT_REPRODUCED"`, `duration_ms == 6392`):
     - `$.outcomes[54].evidence.empty_profiles_json`: `{"powershell_ok": true, "rust_code": "JSON_ERROR", "rust_ok": false}`
     - `$.outcomes[54].evidence.corrupt_profiles_json_codes`: `{"powershell": "UNHANDLED_EXCEPTION", "rust": "JSON_ERROR"}`
     - `$.outcomes[54].evidence.missing_arg_codes`: `{"powershell": "MISSING_ARG", "rust": "INVALID_NAME"}`
6. **Implicated product files & exact functions/line ranges:**
   - [`scripts/dim-host.ps1`](../../../scripts/dim-host.ps1#L377-L399) (`HostEngine.LoadStore` — `L381-L384` returns empty `ProfileStoreFile` on `0`-byte/whitespace file; `L385` `_json.Deserialize` throws uncaught exception; `L390-L393` coerces `schema_version <= 0` to `1` instead of `SCHEMA_ERROR`)
   - [`scripts/dim-host.ps1`](../../../scripts/dim-host.ps1#L3004-L3055) (`HostEngine.RunCli` — `L3006` `MISSING_ARG` vs `INVALID_NAME`, `L3049-L3055` generic `UNHANDLED_EXCEPTION` catch)
   - [`crates/profile-store/src/store.rs`](../../../crates/profile-store/src/store.rs#L157-L167) (`JsonProfileStore::read_document_locked` — canonical `JSON_ERROR` / `SCHEMA_ERROR` behavior)
   - [`fixtures/test-desktop-app/src/bin/dim-cli.rs`](../../../fixtures/test-desktop-app/src/bin/dim-cli.rs#L170-L175) (`run_cli` `"create"` branch)
7. **Fix verification & closure criteria:**
   - **Pre-fix -> Post-fix transition:** `--mode release-regression --case CT-10` transitions from `FAIL_DEFECT_REPRODUCED` (exit `2`) to `PASS` (exit `0`) with identical `ok == false` and matching `error_code` across `dim-cli` and `scripts/dim-host.ps1` for all 3 sub-cases.
   - **Zero bystander profile impact:** Bystander profile directories under `<data_dir>/Profiles/<slug>_<id>` remain untouched on disk when `profiles.json` fails closed with `JSON_ERROR`.
   - **Adjacent non-regression:** `REL-06` (corrupted metadata resilience), `CT-04` (22 `DimError::code()` variants), and `CT-07` (CLI schema contract on a fresh uninitialized `data_dir` where `profiles.json` does not yet exist) all `PASS`.

---

### 2.2 Tier `P1` — Cross-Platform Process Ownership, Stale Locks, Backup Parity, & CLI Contracts

#### [P1-1] `LC-09` (`DEFECT-06`) — Hung Process & Descendant Tree Termination Failures (`allowed_exe` Hardcode on Windows; Missing `SIGKILL` & Child Walk on POSIX)

1. **Shortest reproduction command:**
   ```bash
   cargo run --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner -- --mode release-regression --case LC-09
   ```
2. **Input fixture & arguments:**
   - Part A: `LaunchTarget::NodeFixture` (`xf-fixture.mjs`) with `["--user-id=990001", "--hang-on-quit"]`.
   - Part B (Windows): Native `xf-desktop-fixture.exe` spawned on isolated profile directory with `["--instance-id=hung-custom-exe", "--profile-dir", "<cust_dir>", "--user-id=990002", "--lock-mode=exclusive", "--hang-on-quit"]`, holding an active Win32 exclusive lock on `ZaloData/lockfile`.
3. **Expected behavior:**
   - **Windows (`WindowsPlatformAdapter::stop` & `dim-host.ps1::StopProfile`):** After writing `.quit-signal` / posting `WM_CLOSE` and waiting `timeout_ms`, any non-primary PID proven to belong to this isolated profile (via Win32 `RestartManager` locking `state.lockfile_path`, `.runtime-pid`, or descendant process tree of `state.main_pid`, strictly filtered through `PrimaryProtectionGuard::filter_protected_pids`) is forcefully terminated via `TerminateProcess` regardless of whether its binary name starts with `zalo*`/`node.exe`, returning `stopped_count >= 1` and releasing `lockfile`.
   - **macOS (`MacOsPlatformAdapter::stop`) & Linux (`LinuxPlatformAdapter::stop`):** Recursively collect descendant child PIDs of `main_pid` before killing, escalate from `SIGTERM` to `SIGKILL` after `timeout_ms` across the full isolated process tree (excluding `protected_primary_pids`), remove `.runtime-pid`, and return accurate `stopped_count >= 1`.
4. **Actual behavior:**
   - **Windows:** `WindowsPlatformAdapter::inspect_profile` detects `xf-desktop-fixture.exe` (`PID 9024`) as `running: true, main_pid: Some(9024)` via `rstrtmgr.dll` (`adapter.rs:488-489`), yet `WindowsPlatformAdapter::stop` (`adapter.rs:771-785`) skips `terminate_pid` because `allowed_exe` hardcodes `zalo*`, `zocr*`, `node.exe`, or `dim-test-fixture*`. `stop_profile` returns `Ok(0)` while the hung process remains alive holding `lockfile`!
   - **macOS:** `MacOsPlatformAdapter::stop` (`adapter.rs:386-404`) sends `SIGTERM` to `main_pid` only, never escalates to `SIGKILL`, never collects child processes, never deletes `.runtime-pid`, and returns `Ok(0)` when hung.
   - **Linux:** `LinuxPlatformAdapter::stop` (`adapter.rs:342-361`) sends `SIGTERM`/`SIGKILL` to `main_pid` only (`process_ids` in `inspect_profile` at `L206` contains only `main_pid`, omitting child workers) and leaves `.runtime-pid` on disk.
5. **Concrete evidence (values + JSON path):**
   - `plans/reports/reliability/windows-host-execution-evidence.json` -> `$.outcomes[8]` (`id == "LC-09"`, `verdict == "FAIL_DEFECT_REPRODUCED"`, `duration_ms == 9237`):
     - `$.outcomes[8].evidence.node_hang_escalated_cleanly`: `true`
     - `$.outcomes[8].evidence.custom_exe_detected_running_via_rstrtmgr`: `true`
     - `$.outcomes[8].evidence.custom_exe_pid`: `9024`
     - `$.outcomes[8].evidence.stop_returned_count`: `0`
     - `$.outcomes[8].evidence.custom_exe_survived_stop`: `true`
   - `plans/reports/reliability/defect-reproductions.json` -> `$.defects[4]` (`defect_id == "DEFECT-06"`).
6. **Implicated product files & exact functions/line ranges:**
   - [`crates/platform-windows/src/adapter.rs`](../../../crates/platform-windows/src/adapter.rs#L492-L498) (`inspect_profile` `is_expected_exe` closure) & [`L771-L785`](../../../crates/platform-windows/src/adapter.rs#L771-L785) (`stop` `allowed_exe` filter)
   - [`scripts/dim-host.ps1`](../../../scripts/dim-host.ps1#L1032-L1038) (`ExpandProcessTree`), [`L1109-L1125`](../../../scripts/dim-host.ps1#L1109-L1125) (`IsManagedAppProcessAlive`), & [`L2295-L2303`](../../../scripts/dim-host.ps1#L2295-L2303) (`StopProfile` process-name allowlist)
   - [`crates/platform-macos/src/adapter.rs`](../../../crates/platform-macos/src/adapter.rs#L210) (`inspect_profile` single-PID `process_ids`) & [`L386-L404`](../../../crates/platform-macos/src/adapter.rs#L386-L404) (`MacOsPlatformAdapter::stop`)
   - [`crates/platform-linux/src/adapter.rs`](../../../crates/platform-linux/src/adapter.rs#L206) (`inspect_profile` single-PID `process_ids`) & [`L342-L361`](../../../crates/platform-linux/src/adapter.rs#L342-L361) (`LinuxPlatformAdapter::stop`)
7. **Fix verification & closure criteria:**
   - **Pre-fix -> Post-fix transition:** `--mode release-regression --case LC-09` exits `0` (`PASS`) on Windows (both `NodeFixture` and `xf-desktop-fixture.exe` terminated, `lockfile` unlocked, `stop_returned_count >= 1`) and on macOS/Ubuntu runners (hung parent + child tree killed via `SIGKILL` escalation).
   - **Zero bystander profile impact:** Unmanaged external processes (`LC-04`), sibling active profiles (`LC-02`), and primary Zalo Desktop PIDs (`CT-06`, `PROTECTED_PRIMARY_PROCESS`) remain 100% untouched.
   - **Adjacent non-regression:** `LC-02`, `LC-03`, `LC-04`, `CT-06`, `REL-07` (`--spawn-children=2 --orphan-children --hang-on-quit`), and `REL-08` (PID-reuse spoof protection) all `PASS`.

---

#### [P1-2] `XP-LNX-02` (`DEFECT-01`) — Linux Adapter Fails to Detect Chromium Dangling `SingletonLock` Symlinks

1. **Shortest reproduction command:**
   ```bash
   cargo run --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner -- --mode release-regression --case XP-LNX-02
   ```
2. **Input fixture & arguments:**
   - Layout: Linux isolated profile `<profile_dir>/.config/ZaloData/SingletonLock` created as a Chromium-style dangling symlink pointing to `"ubuntu-host-99999"` (non-existent target file, dead PID `99999`, no JSON `lockfile`).
   - API under test: `LinuxPlatformAdapter::inspect_profile(&profile, &profile_dir)` (behavioral on native Linux; non-host static check tagged `STATIC_CONFIRMED_NON_HOST` on Windows).
3. **Expected behavior:**
   - `LinuxPlatformAdapter::inspect_profile` checks symlink entry existence via `fs::symlink_metadata(&singleton_lock).is_ok()` (or `singleton_lock.is_symlink()`), reporting `running: false`, `stale_lock_detected: true`, `lockfile_path: <zalo_data>/SingletonLock`, and `status_label: "Stopped (Stale Lock)"` so `StaleLockRecoveryManager::recover_stale_state` unlinks the dangling `SingletonLock` before relaunch.
4. **Actual behavior:**
   - `crates/platform-linux/src/adapter.rs:190-191` calls `singleton_lock.exists()`, which invokes `std::fs::metadata()` (following the symlink target). Because `"ubuntu-host-99999"` is not a file on disk, `singleton_lock.exists()` **always returns `false`**, causing `stale_lock_detected == false` and skipping stale-lock cleanup on relaunch.
5. **Concrete evidence (values + JSON path):**
   - `plans/reports/reliability/windows-host-execution-evidence.json` -> `$.outcomes[42]` (`id == "XP-LNX-02"`, `verdict == "FAIL_DEFECT_REPRODUCED"`, `duration_ms == 13`):
     - `$.outcomes[42].evidence.official_linux_status`: `"unsupported"`
     - `$.outcomes[42].evidence.appimage_status`: `"experimental"`
     - `$.outcomes[42].evidence.dangling_singleton_lock_uses_path_exists`: `true`
   - `plans/reports/reliability/defect-reproductions.json` -> `$.defects[0]` (`defect_id == "DEFECT-01"`).
6. **Implicated product files & exact functions/line ranges:**
   - [`crates/platform-linux/src/adapter.rs`](../../../crates/platform-linux/src/adapter.rs#L177-L220) (`<LinuxPlatformAdapter as InstanceMonitor>::inspect_profile`, specifically `L190-L195`: `let stale = pid.is_none() && (singleton_lock.exists() || fixture_lock.exists());`)
   - [`crates/process-manager/src/recovery.rs`](../../../crates/process-manager/src/recovery.rs) (`StaleLockRecoveryManager::recover_stale_state` — ensure dangling `SingletonLock` symlink is unlinked via `fs::symlink_metadata` rather than `.exists()`)
7. **Fix verification & closure criteria:**
   - **Pre-fix -> Post-fix transition:** `--mode release-regression --case XP-LNX-02` transitions from `FAIL_DEFECT_REPRODUCED` to `PASS` on Windows (non-host static check `STATIC_CONFIRMED_NON_HOST`) and passes `--require-native-os` on Ubuntu 24.04 via real dangling symlink creation + `inspect_profile` + `StaleLockRecoveryManager`.
   - **Zero bystander profile impact:** Sibling Linux profiles without stale `SingletonLock` continue reporting `stale_lock_detected: false` and `"Stopped"`.
   - **Adjacent non-regression:** `XP-LNX-01`, `XP-LNX-03`, `XP-LNX-04` (on Ubuntu), `CT-01`, and `CT-05` all `PASS`.

---

#### [P1-3] `XP-MAC-03` (`DEFECT-01`) — macOS Adapter Fails to Detect Stale Chromium Dangling `SingletonLock` Symlinks

1. **Shortest reproduction command:**
   ```bash
   cargo run --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner -- --mode release-regression --case XP-MAC-03
   ```
2. **Input fixture & arguments:**
   - Layout: macOS isolated profile `<profile_dir>/Library/Application Support/ZaloData/SingletonLock` symlinked to `"MacBook-Pro.local-99999"` (dead PID `99999`, dangling symlink target).
   - API under test: `MacOsPlatformAdapter::inspect_profile(&profile, &profile_dir)`.
3. **Expected behavior:**
   - `MacOsPlatformAdapter::inspect_profile` uses `fs::symlink_metadata(&singleton_lock).is_ok()` to detect the dangling `SingletonLock` symlink entry when `alive_pid.is_none()`, setting `stale_lock_detected: true`, `lockfile_path: singleton_lock`, and `status_label: "Stopped (Stale Lock)"`, enabling automatic recovery on relaunch.
4. **Actual behavior:**
   - `crates/platform-macos/src/adapter.rs:194-195` evaluates `singleton_lock.exists()`, which follows the broken `<hostname>-<pid>` symlink target and evaluates to `false`, leaving `stale_lock_detected == false` after any unclean Electron/Chromium crash on macOS.
5. **Concrete evidence (values + JSON path):**
   - `plans/reports/reliability/windows-host-execution-evidence.json` -> `$.outcomes[39]` (`id == "XP-MAC-03"`, `verdict == "FAIL_DEFECT_REPRODUCED"`, `duration_ms == 7`):
     - `$.outcomes[39].evidence.singleton_lock_uses_path_exists_bug`: `true`
   - `plans/reports/reliability/defect-reproductions.json` -> `$.defects[0]` (`DEFECT-01`) & `$.defects[1]` (`DEFECT-02`).
6. **Implicated product files & exact functions/line ranges:**
   - [`crates/platform-macos/src/adapter.rs`](../../../crates/platform-macos/src/adapter.rs#L180-L225) (`<MacOsPlatformAdapter as InstanceMonitor>::inspect_profile`, specifically `L194-L199`: `singleton_lock.exists()`)
7. **Fix verification & closure criteria:**
   - **Pre-fix -> Post-fix transition:** Together with `[P1-4]`, `--mode release-regression --case XP-MAC-03` transitions from `FAIL_DEFECT_REPRODUCED` to `PASS` (`STATIC_CONFIRMED_NON_HOST` on Windows; native behavioral `PASS` with `--require-native-os` on macOS runner).
   - **Zero bystander profile impact:** Clean stopped macOS profiles without `SingletonLock` report `stale_lock_detected == false`.
   - **Adjacent non-regression:** `XP-MAC-01`, `XP-MAC-02`, `XP-MAC-04` (on macOS), and `LC-05` crash recovery all `PASS`.

---

#### [P1-4] `XP-MAC-03` (`DEFECT-02` / `DEFECT-03`) — POSIX `.runtime-pid` Ownership, Post-Spawn Readiness Polling, & Fixture `--user-id` Derivation

1. **Shortest reproduction command:**
   ```bash
   cargo run --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner -- --mode release-regression --case XP-MAC-03
   ```
2. **Input fixture & arguments:**
   - Fixture: Two isolated profiles launched back-to-back with `LaunchTarget::NodeFixture` (`fixtures/cross-platform/xf-fixture.mjs`, `extra_args = []` — no explicit `--user-id` override).
   - APIs under test: `MacOsPlatformAdapter::launch` / `inspect_profile` and `LinuxPlatformAdapter::launch` / `inspect_profile`.
3. **Expected behavior:**
   - **Deterministic `NodeFixture` `--user-id` parity:** When `LaunchTarget::NodeFixture` omits `--user-id`, both `MacOsPlatformAdapter::launch` and `LinuxPlatformAdapter::launch` resolve `detect_zalo_user_id(&zalo_data)` -> `profile.zalo_user_id` -> deterministic per-profile FNV-1a `--user-id` (matching `WindowsPlatformAdapter::launch` at `crates/platform-windows/src/adapter.rs:600-607`), preventing false `DUPLICATE_ZALO_USER_ID` collisions (`"100001"`) on multi-profile fixture launches.
   - **`.runtime-pid` ownership & readiness polling:** Both POSIX adapters poll up to bounded readiness (`~2,500–3,000 ms`) for `SingletonLock`/`lockfile` commit or child exit after spawning, AND `inspect_profile` consults `.runtime-pid` (verifying PID liveness + profile ownership / non-primary PID) when `SingletonLock`/`lockfile` has not yet appeared, so `launch()` deterministically returns `running: true, main_pid: Some(pid)`.
4. **Actual behavior:**
   - Both `MacOsPlatformAdapter::launch` (`adapter.rs:298,317`) and `LinuxPlatformAdapter::launch` (`adapter.rs:298-299`) write `child.id()` to `.runtime-pid` and **immediately** invoke `self.inspect_profile(profile, profile_dir)` with `0 ms` readiness polling, while `inspect_profile` **never reads `.runtime-pid`**. `launch()` therefore returns `running: false, main_pid: None` right after spawn.
   - Neither POSIX adapter injects `--user-id` on `NodeFixture`, so launching 2 profiles defaults both to `"100001"`, failing the 2nd launch with `DUPLICATE_ZALO_USER_ID`.
5. **Concrete evidence (values + JSON path):**
   - `plans/reports/reliability/windows-host-execution-evidence.json` -> `$.outcomes[39]` (`id == "XP-MAC-03"`, `verdict == "FAIL_DEFECT_REPRODUCED"`):
     - `$.outcomes[39].evidence.writes_runtime_pid_but_never_reads_it`: `true`
     - `$.outcomes[39].evidence.missing_deterministic_fixture_user_id`: `true`
   - `plans/reports/reliability/defect-reproductions.json` -> `$.defects[1]` (`defect_id == "DEFECT-02"`).
6. **Implicated product files & exact functions/line ranges:**
   - [`crates/platform-macos/src/adapter.rs`](../../../crates/platform-macos/src/adapter.rs#L192-L199) (`MacOsPlatformAdapter::inspect_profile`) & [`L287-L321`](../../../crates/platform-macos/src/adapter.rs#L287-L321) (`MacOsPlatformAdapter::launch`)
   - [`crates/platform-linux/src/adapter.rs`](../../../crates/platform-linux/src/adapter.rs#L189-L196) (`LinuxPlatformAdapter::inspect_profile`) & [`L260-L300`](../../../crates/platform-linux/src/adapter.rs#L260-L300) (`LinuxPlatformAdapter::launch`)
   - Reference parity implementation: [`crates/platform-windows/src/adapter.rs`](../../../crates/platform-windows/src/adapter.rs#L600-L629)
7. **Fix verification & closure criteria:**
   - **Pre-fix -> Post-fix transition:** `--mode release-regression --case XP-MAC-03` exits `0`; on native macOS (`XP-MAC-04` / `LC-01`) and Ubuntu (`XP-LNX-04` / `LC-01`), launching 2+ `NodeFixture` profiles without explicit `--user-id` succeeds with `running == true`, `main_pid != None`, and distinct `detected_zalo_user_id` values.
   - **Zero bystander profile impact:** Profiles with existing `Database/_production/<uid>` or `profile.zalo_user_id` preserve their real UID; genuine duplicate `zalo_user_id` across two profiles is still blocked with `DUPLICATE_ZALO_USER_ID`.
   - **Adjacent non-regression:** `LC-01`, `LC-07`, `XP-MAC-04`, and `XP-LNX-04` all `PASS`.

---

#### [P1-5] `DS-08` (`DEFECT-07`) — Cross-OS Layout Backup & Restore Drops Post-Launch `zalo_user_id` on macOS & Linux

1. **Shortest reproduction command:**
   ```bash
   cargo run --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner -- --mode release-regression --case DS-08
   ```
2. **Input fixture & arguments:**
   - Synthetic profile 1 (macOS layout): `<profile_dir>/Library/Application Support/ZaloData/Database/_production/849777666` + `.zalo-profile-id`, with `profile.zalo_user_id = None` (simulating QR sign-in after initial `launch_profile`).
   - Synthetic profile 2 (Linux layout): `<profile_dir>/.config/ZaloData/Database/_production/849888999` + `.zalo-profile-id`, with `profile.zalo_user_id = None`.
   - API sequence: `FileBackupManager::backup_profile(&prof, &dir, None, false)` -> `FileBackupManager::restore_backup_as_new_profile(&bk.backup_dir, Some("..."))`.
3. **Expected behavior:**
   - `FileBackupManager::backup_profile` resolves `zalo_user_id` by probing all 3 OS `ZaloData` candidate paths (`AppData/Roaming/ZaloData`, `Library/Application Support/ZaloData`, `.config/ZaloData`) via `detect_zalo_user_id` and falling back to `profile.zalo_user_id`, recording `"849777666"` and `"849888999"` in `backup.json` (`BackupManifest.zalo_user_id`).
   - `FileBackupManager::restore_backup_as_new_profile` likewise probes all 3 OS `ZaloData` layouts inside `dest_profile_dir` (matching `rewrite_profile_identity_markers` at `backup.rs:440-447`) so `restored.zalo_user_id` equals `Some("849777666")` and `Some("849888999")`.
4. **Actual behavior:**
   - `backup_profile` (`backup.rs:143`) copies `profile.zalo_user_id.clone()` (`None`) without scanning disk, writing `"zalo_user_id": null` into `backup.json`.
   - `restore_backup_as_new_profile` (`backup.rs:339-345`) hardcodes `detect_zalo_user_id(&dest_profile_dir.join("AppData").join("Roaming").join("ZaloData"))`, completely missing macOS `Library/Application Support/ZaloData` and Linux `.config/ZaloData`, returning `zalo_user_id: None` on both!
5. **Concrete evidence (values + JSON path):**
   - `plans/reports/reliability/windows-host-execution-evidence.json` -> `$.outcomes[24]` (`id == "DS-08"`, `verdict == "FAIL_DEFECT_REPRODUCED"`, `duration_ms == 520`):
     - `$.outcomes[24].evidence.expected_mac_uid`: `"849777666"` vs `mac_manifest_uid`: `null`, `mac_restored_uid`: `null`
     - `$.outcomes[24].evidence.expected_linux_uid`: `"849888999"` vs `linux_manifest_uid`: `null`, `linux_restored_uid`: `null`
   - `plans/reports/reliability/defect-reproductions.json` -> `$.defects[5]` (`defect_id == "DEFECT-07"`).
6. **Implicated product files & exact functions/line ranges:**
   - [`crates/profile-store/src/backup.rs`](../../../crates/profile-store/src/backup.rs#L136-L147) (`<FileBackupManager as BackupManager>::backup_profile` — `L143` `zalo_user_id: profile.zalo_user_id.clone()`)
   - [`crates/profile-store/src/backup.rs`](../../../crates/profile-store/src/backup.rs#L339-L355) (`<FileBackupManager as BackupManager>::restore_backup_as_new_profile` — `L339-L345` Windows-only `AppData/Roaming/ZaloData` path)
   - [`crates/profile-store/src/backup.rs`](../../../crates/profile-store/src/backup.rs#L439-L462) (`rewrite_profile_identity_markers` — `L455-L460` fallback writes Windows path even on POSIX hosts)
7. **Fix verification & closure criteria:**
   - **Pre-fix -> Post-fix transition:** `--mode release-regression --case DS-08` exits `0` (`PASS`) with both `manifest.zalo_user_id` and `restored.zalo_user_id` matching `"849777666"` (macOS) and `"849888999"` (Linux).
   - **Zero bystander profile impact:** Source profiles (`mac_prof`, `linux_prof`) and any sibling profiles retain their original `.zalo-profile-id` markers and files untouched.
   - **Adjacent non-regression:** `DS-02`, `DS-03`, `DS-04`, `DS-05`, `DS-06`, and `REL-04` all `PASS`.

---

#### [P1-6] `CT-08` (`DEFECT-08a`) — Differential CLI Slug Length Cap Divergence (`40` Chars in Rust vs `32` Chars in PowerShell)

1. **Shortest reproduction command:**
   ```bash
   cargo run --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner -- --mode release-regression --case CT-08
   ```
2. **Input fixture & arguments:**
   - Command invoked on both `dim-cli.exe` and `scripts/dim-host.ps1`:
     `--cli --json --data-dir <dir> --node-fixture --app-path fixtures/cross-platform/xf-fixture.mjs create "Tai Khoan Kinh Doanh Khu Vuc Mien Nam So 1"` (`42` ASCII characters, valid `<= 64`).
3. **Expected behavior:**
   - Both CLIs enforce the canonical `MAX_SLUG_LEN = 40` rule from `dim-core` (`crates/core/src/slug.rs:14`), generating the exact `40`-character slug prefix `"tai-khoan-kinh-doanh-khu-vuc-mien-nam-so"` in `<slug>_<8hex>`.
4. **Actual behavior:**
   - `dim-cli` generates `"tai-khoan-kinh-doanh-khu-vuc-mien-nam-so_<id>"` (`40`-char slug), whereas `scripts/dim-host.ps1` (`SlugifyName`, `L582`) breaks at `if (clean.Length >= 32) break;`, generating `"tai-khoan-kinh-doanh-khu-vuc-mie_<id>"` (`32`-char slug).
5. **Concrete evidence (values + JSON path):**
   - `plans/reports/reliability/windows-host-execution-evidence.json` -> `$.outcomes[52]` (`id == "CT-08"`, `verdict == "FAIL_DEFECT_REPRODUCED"`, `duration_ms == 1991`):
     - `$.outcomes[52].evidence.input_name`: `"Tai Khoan Kinh Doanh Khu Vuc Mien Nam So 1"`
     - `$.outcomes[52].evidence.rust_slug_prefix`: `"tai-khoan-kinh-doanh-khu-vuc-mien-nam-so"` (`rust_slug_len: 40`)
     - `$.outcomes[52].evidence.powershell_slug_prefix`: `"tai-khoan-kinh-doanh-khu-vuc-mie"` (`powershell_slug_len: 32`)
6. **Implicated product files & exact functions/line ranges:**
   - [`scripts/dim-host.ps1`](../../../scripts/dim-host.ps1#L560-L586) (`HostEngine.SlugifyName` — `L582`: `if (clean.Length >= 32) break;` instead of `40`)
   - Canonical spec: [`crates/core/src/slug.rs`](../../../crates/core/src/slug.rs#L14) (`pub const MAX_SLUG_LEN: usize = 40;`) & [`L46-L81`](../../../crates/core/src/slug.rs#L46-L81) (`slugify_profile_name`)
7. **Fix verification & closure criteria:**
   - **Pre-fix -> Post-fix transition:** `--mode release-regression --case CT-08` exits `0` (`PASS`) with `rust_slug_prefix == powershell_slug_prefix` (`40` chars).
   - **Zero bystander profile impact:** Existing profiles with `32`-char `directory_name` values already recorded in `profiles.json` remain loadable because `GetProfilePath` resolves the stored `record.directory_name` rather than recomputing slugs.
   - **Adjacent non-regression:** `PM-01`, `PM-03` (Vietnamese NFC/NFD & emoji slugs), `PM-05` (`63`/`64`/`65` char boundaries), and `tests/run-contract-parity-tests.mjs` all `PASS`.

---

#### [P1-7] `CT-09` (`DEFECT-08b`) — Differential CLI Restore Duplicate `--name` & `64`-Char Overflow Divergence

1. **Shortest reproduction command:**
   ```bash
   cargo run --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner -- --mode release-regression --case CT-09
   ```
2. **Input fixture & arguments:**
   - Create a valid `58`-character profile (`"A".repeat(58)`), run `backup <id>`, then execute on both `dim-cli.exe` and `scripts/dim-host.ps1`:
     1. `restore <backup_dir> --name <58_char_existing_name>` (explicit collision)
     2. `restore <backup_dir>` (default auto-generated `" (Restored)"` name)
3. **Expected behavior:**
   - **Explicit `--name` collision:** Both CLIs reject an explicit `--name` that collides (case-insensitively) with an existing profile, returning `ok: false, error_code: "DUPLICATE_NAME"` without creating a profile or leaving `.restore-*` staging remnants.
   - **Auto-generated restore name length cap:** Both CLIs clamp auto-generated `"<source> (Restored)"` / `"<source> (Restored #N)"` names to `MAX_PROFILE_NAME_LEN = 64` characters so restored profiles never violate schema validation on subsequent operations.
4. **Actual behavior:**
   - `dim-cli` enforces both rules (`error_code: "DUPLICATE_NAME"`, `rust_auto_restore_name_len == 64`).
   - `scripts/dim-host.ps1` (`RestoreBackup`, `L2595-L2608`) silently rewrites explicit `--name` collisions to `"<name> #2"` (`ok: true`, `61` chars) and appends `" (Restored)"` without clamping, creating a **`69`-character** profile name (`> 64`) that later fails `TryValidateProfileName` on rename!
5. **Concrete evidence (values + JSON path):**
   - `plans/reports/reliability/windows-host-execution-evidence.json` -> `$.outcomes[53]` (`id == "CT-09"`, `verdict == "FAIL_DEFECT_REPRODUCED"`, `duration_ms == 7757`):
     - `$.outcomes[53].evidence.rust_duplicate_name_response`: `{"error_code": "DUPLICATE_NAME", "ok": false}`
     - `$.outcomes[53].evidence.powershell_duplicate_name_response`: `{"allocated_name": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA #2", "ok": true}`
     - `$.outcomes[53].evidence.rust_auto_restore_name_len`: `64`
     - `$.outcomes[53].evidence.powershell_auto_restore_name_len`: `69` (`max_profile_name_len: 64`)
6. **Implicated product files & exact functions/line ranges:**
   - [`scripts/dim-host.ps1`](../../../scripts/dim-host.ps1#L2583-L2608) (`HostEngine.RestoreBackup` — `L2595` unclamped `origName + " (Restored)"`, `L2603-L2607` `while (FindProfileByExactName(...))` applied even when `requestedName` was explicitly supplied and using exact-case lookup instead of case-insensitive comparison)
   - Canonical spec: [`crates/profile-store/src/backup.rs`](../../../crates/profile-store/src/backup.rs#L399-L437) (`choose_unique_restored_name`)
7. **Fix verification & closure criteria:**
   - **Pre-fix -> Post-fix transition:** `--mode release-regression --case CT-09` exits `0` (`PASS`) with PowerShell returning `ok: false, error_code: "DUPLICATE_NAME"` on explicit duplicate `--name` and clamping auto-restored names to `<= 64` chars.
   - **Zero bystander profile impact:** Existing source profile (`name_58`) and its backup directory remain untouched when duplicate `--name` restore is rejected.
   - **Adjacent non-regression:** `DS-05` (restore rollback), `PM-05` (`64`-char validation), and `tests/run-contract-parity-tests.mjs` all `PASS`.

---

#### [P1-8] `LC-06` Cross-Platform Audit (`DEFECT-05`) — `focus_or_restore` Silent No-Op on Linux & Missing Headless Fixture Signal on macOS

1. **Shortest reproduction command:**
   ```bash
   # Behavioral gate on native Linux/macOS runners (static contract finding when audited from Windows):
   cargo run --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner -- --mode release-regression --case LC-06
   ```
2. **Input fixture & arguments:**
   - Fixture: `LaunchTarget::NodeFixture` (`fixtures/cross-platform/xf-fixture.mjs`, `["--user-id=960001"]`).
   - Sequence: Call `coordinator.launch_profile(&prof.id, &target)` once, wait for readiness, then call `coordinator.launch_profile(&prof.id, &target)` (or `coordinator.focus_profile`) a second time while running and inspect `fixture-state.json` `focus_count`.
3. **Expected behavior:**
   - Duplicate `launch_profile` / `focus_profile` preserves the original `main_pid` (`boot_sequence == 1`) **and** delivers a focus/restore signal across all 3 OS adapters:
     - On Linux: writes `.focus-signal` for fixture targets and invokes window activation (`wmctrl`/`xdotool` if present or second-instance Electron argv handoff) rather than silently doing nothing.
     - On macOS: writes `.focus-signal` when targeting `NodeFixture` / headless `CustomExecutable` in addition to `osascript` `System Events` activation for GUI `.app` bundles, incrementing `focus_count >= 1`.
4. **Actual behavior:**
   - `WindowsPlatformAdapter::focus_or_restore` (`crates/platform-windows/src/adapter.rs:642-665`) focuses `hwnd` or spawns a second-instance handoff (which writes `.focus-signal` in `xf-fixture.mjs` and exits), so `LC-06` is `PASS` on Windows (`focus_count: 1`).
   - `LinuxPlatformAdapter::focus_or_restore` (`crates/platform-linux/src/adapter.rs:310-318`) ignores all 4 parameters and unconditionally returns `Ok(())` (`focus_count == 0`).
   - `MacOsPlatformAdapter::focus_or_restore` (`crates/platform-macos/src/adapter.rs:331-356`) runs `osascript` `System Events` only, which fails/no-ops on headless `node` fixtures without writing `.focus-signal` (`focus_count == 0`).
5. **Concrete evidence (values + JSON path):**
   - Windows baseline (`PASS`): `plans/reports/reliability/windows-host-execution-evidence.json` -> `$.outcomes[5]` (`id == "LC-06"`, `evidence: {"boot_sequence": 1, "focus_count": 1, "pid_preserved": 6692}`).
   - POSIX defect assertion wired in `tests/cross-platform/src/suites/lifecycle.rs:583-596` (`if fstate.focus_count == 0 -> CaseSuccess::defect("DEFECT-05", ...)`), documented in `plans/reports/reliability/baseline-manifest.json` -> `$.static_only_findings[?(@.finding_id=="DEFECT-05")]`.
6. **Implicated product files & exact functions/line ranges:**
   - [`crates/platform-linux/src/adapter.rs`](../../../crates/platform-linux/src/adapter.rs#L310-L318) (`<LinuxPlatformAdapter as InstanceLauncher>::focus_or_restore`)
   - [`crates/platform-macos/src/adapter.rs`](../../../crates/platform-macos/src/adapter.rs#L331-L356) (`<MacOsPlatformAdapter as InstanceLauncher>::focus_or_restore`)
7. **Fix verification & closure criteria:**
   - **Pre-fix -> Post-fix transition:** `LC-06` passes (`exit 0`, `pid_preserved == pid1`, `boot_sequence == 1`, `focus_count >= 1`) on Windows, macOS, and Ubuntu runners.
   - **Zero bystander profile impact:** Focusing profile A never focuses, restarts, or signals sibling profile B or primary Zalo Desktop.
   - **Adjacent non-regression:** `LC-01`, `LC-02`, and `LC-08` all `PASS`.

---

### 2.3 Tier `P2` — Unicode Case-Folding Parity & Tauri Diagnostics Matrix Polish

#### [P2-1] `PM-04` (`DEFECT-10`) — Non-ASCII Vietnamese Case-Insensitive Duplicate Profile Names (`Đức` vs `đức`)

1. **Shortest reproduction command:**
   ```bash
   cargo run --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner -- --mode release-regression --case PM-04
   ```
2. **Input fixture & arguments:**
   - Store API calls on `JsonProfileStore`:
     1. `store.create_profile("Đức Công Việc", "")`
     2. `store.create_profile("đức công việc", "")` (differs solely in non-ASCII Vietnamese letter case: `Đ`/`đ`, `ô`/`Ô`, `ệ`/`Ệ`).
3. **Expected behavior:**
   - Profile display name uniqueness, rename collision checks, restore name collision checks, and name-based `get_profile` lookups fold full Unicode case (e.g., NFC-normalized `.to_lowercase()` equality) rather than ASCII-only `A-Z`, rejecting `"đức công việc"` with `Err(DimError::DuplicateProfileName(_))` (`error_code: "DUPLICATE_NAME"`), matching PowerShell `scripts/dim-host.ps1` while still permitting case-only self-rename (`"Đức Công Việc"` -> `"ĐỨC CÔNG VIỆC"` on the same `profile.id`).
4. **Actual behavior:**
   - `JsonProfileStore` (`crates/profile-store/src/store.rs:100,216,260,439`) and `choose_unique_restored_name` (`crates/profile-store/src/backup.rs:406,416,428`) use `str::eq_ignore_ascii_case`, which leaves non-ASCII characters (`Đ` U+0110 vs `đ` U+0111, accented Vietnamese vowels) unfolded. Both `"Đức Công Việc"` and `"đức công việc"` are accepted into `profiles.json` in Rust (`dim-cli` / Tauri), diverging from `scripts/dim-host.ps1:1777`.
5. **Concrete evidence (values + JSON path):**
   - `plans/reports/reliability/windows-host-execution-evidence.json` -> `$.outcomes[12]` (`id == "PM-04"`, `verdict == "FAIL_DEFECT_REPRODUCED"`, `duration_ms == 202`):
     - `$.outcomes[12].evidence.first_profile`: `{"dir": "duc-cong-viec_b54cbf73", "id": "b54cbf73", "name": "Đức Công Việc"}`
     - `$.outcomes[12].evidence.second_profile_unexpectedly_allowed`: `{"dir": "duc-cong-viec_de052468", "id": "de052468", "name": "đức công việc"}`
   - `plans/reports/reliability/defect-reproductions.json` -> `$.defects[8]` (`defect_id == "DEFECT-10"`).
6. **Implicated product files & exact functions/line ranges:**
   - [`crates/profile-store/src/store.rs`](../../../crates/profile-store/src/store.rs#L97-L103) (`JsonProfileStore::insert_restored_profile` `L100`)
   - [`crates/profile-store/src/store.rs`](../../../crates/profile-store/src/store.rs#L213-L219) (`<JsonProfileStore as ProfileStore>::create_profile` `L216`)
   - [`crates/profile-store/src/store.rs`](../../../crates/profile-store/src/store.rs#L256-L263) (`<JsonProfileStore as ProfileStore>::rename_profile` `L260`)
   - [`crates/profile-store/src/store.rs`](../../../crates/profile-store/src/store.rs#L429-L442) (`find_profile_index` `L439-L440`)
   - [`crates/profile-store/src/backup.rs`](../../../crates/profile-store/src/backup.rs#L399-L433) (`choose_unique_restored_name` `L406,L416,L428`)
7. **Fix verification & closure criteria:**
   - **Pre-fix -> Post-fix transition:** `--mode release-regression --case PM-04` transitions from `FAIL_DEFECT_REPRODUCED` (exit `2`) to `PASS` (exit `0`) with `duplicate_vietnamese_case_rejected == true`.
   - **Zero bystander profile impact:** First profile `"Đức Công Việc"` remains intact and retrievable via `store.get_profile("đức công việc")`.
   - **Adjacent non-regression:** `PM-03` (NFC/NFD diacritics & emoji disambiguation), `PM-05` (`64`-char Vietnamese scalar count), and `PM-06` (ASCII duplicate rejection + case-only self-rename) all `PASS`.

---

#### [P2-2] Static IPC Contract (`STATIC-TAURI-DIAG-01`) — Tauri `diagnostics_report` Duplicates macOS / Ubuntu Rows in `platform_support_matrix`

1. **Shortest reproduction command:**
   ```bash
   cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml
   cargo run --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner -- --mode release-regression --case CT-01,CT-07
   ```
2. **Input fixture & arguments:**
   - Tauri IPC command: `apps/desktop/src-tauri/src/commands.rs::diagnostics_report(state: State<'_, AppState>)` when compiled/invoked on `PlatformKind::MacOs` or `PlatformKind::Linux` (compared against `PlatformKind::Windows` and `dim-cli diagnostics`).
3. **Expected behavior:**
   - `platform_support_matrix` in `commands.rs::diagnostics_report` emits distinct, non-duplicated platform rows regardless of compile-time `caps.platform` (`Windows 10/11`, `macOS (Apple Silicon & Intel)`, `Ubuntu LTS (Official Zalo)`, `Ubuntu LTS (Fixture / Local AppImage)`), populating the active host's live `caps` status/limits in place rather than prepending a duplicate macOS or Linux row at index `[0]`.
4. **Actual behavior:**
   - `apps/desktop/src-tauri/src/commands.rs:465-494` constructs a 4-element JSON array where element `[0]` sets `"platform"` dynamically from `caps.platform` (`"Windows 10/11"`, `"macOS (Apple Silicon & Intel)"`, or `"Ubuntu / Linux"`), while elements `[1]`, `[2]`, and `[3]` unconditionally hardcode `"macOS (Apple Silicon & Intel)"`, `"Ubuntu LTS (Official Zalo)"`, and `"Ubuntu LTS (Fixture / Local AppImage)"`. On macOS or Linux builds, the Tauri Diagnostics UI renders duplicate macOS or Ubuntu rows while omitting the Windows row!
5. **Concrete evidence (values + JSON path):**
   - Source lines: `apps/desktop/src-tauri/src/commands.rs:465-494` vs `fixtures/test-desktop-app/src/bin/dim-cli.rs:580-591`.
   - Cataloged in `plans/reports/reliability/baseline-manifest.json` -> `$.static_only_findings[?(@.finding_id=="STATIC-TAURI-DIAG-01")]` and `docs/testing/cross-platform/adapter-contract-report.md` §3.2 (`L59-L61`).
6. **Implicated product files & exact functions/line ranges:**
   - [`apps/desktop/src-tauri/src/commands.rs`](../../../apps/desktop/src-tauri/src/commands.rs#L465-L494) (`diagnostics_report` JSON construction of `"platform_support_matrix"`)
7. **Fix verification & closure criteria:**
   - **Pre-fix -> Post-fix transition:** `platform_support_matrix` contains unique platform entries across `PlatformKind::{Windows, MacOs, Linux}` with zero duplicate macOS/Ubuntu labels, and `cargo check` / `cargo clippy` on `apps/desktop/src-tauri` passes cleanly.
   - **Zero bystander profile impact:** Read-only diagnostics IPC command; mutates zero profile records or files.
   - **Adjacent non-regression:** `CT-01` (`PlatformCapabilities` honesty) and `CT-07` (`diagnostics` CLI JSON schema) remain `PASS`.

---

## 3. Harness vs. Product Defect Classification & Assertion Stability Audit

Per Mission C2 Section 3, all `55` harness cases (`37` `RUNTIME_EXECUTION`, `11` `CONTRACT_SCHEMA_CHECK`, `3` `DIFFERENTIAL_CLI_RUNTIME`, `2` `STATIC_SOURCE_ASSERTION` on Windows upgraded to behavioral on native POSIX, and `2` `NATIVE_OS_RUNTIME_GATE`) plus static findings were audited so harness implementation details are never confused with product bugs.

### 3.1 Defect Classification Taxonomy

| Classification Tag | Engineering Meaning |
| :--- | :--- |
| **`RUNTIME_REPRODUCED`** | Dynamically triggered by executing real product Rust/PowerShell code paths against live OS processes/filesystems on the Windows 11 host (`windows-host-execution-evidence.json`). |
| **`STATIC_CONFIRMED`** | Directly proven in product source code at `a5feb4a52079` (`file:line` verified), including non-host `#[cfg(target_os = ...)]` branches compiled out on Windows (`STATIC_CONFIRMED_NON_HOST`). |
| **`CONTRACT_MISMATCH`** | Public behavioral or schema divergence across product boundaries (`dim-cli` vs `scripts/dim-host.ps1`, trait `ProfileStore::delete_profile` vs `InstanceCoordinator::delete_profile`, or Tauri IPC vs CLI). |
| **`HOST_DEPENDENT`** | Runtime execution depends on OS-specific kernel/host primitives (`CreateProcessW`/`rstrtmgr.dll`/`powershell.exe` on Windows; POSIX symlinks/`open -n`/`SIGKILL`/`/proc` on macOS & Linux). |
| **`INFERENCE`** | Multi-process/timing race extrapolated from static code structure where single-process execution only exercises part of the hazard (specifically `DEFECT-04b` cross-process `profiles.json.tmp` collision alongside `DEFECT-04a`'s `RUNTIME_REPRODUCED` 4-thread launch race). |

### 3.2 Complete 11-Case + Static Findings Audit Matrix (Synchronized With `baseline-manifest.json`)

| Case / Finding ID | Priority | Linked Defect | `defect_classification` (`baseline-manifest.json`) | Windows Host vs. Native POSIX Execution Mechanism | Public-Behavior Assertion Stability Audit |
| :--- | :---: | :--- | :--- | :--- | :--- |
| **`LC-07`** | `P0` | `DEFECT-03` | `["RUNTIME_REPRODUCED", "STATIC_CONFIRMED", "HOST_DEPENDENT"]` | **Windows:** Live `NodeFixture` (`--slow-startup-pre-lock-ms=3100`) + `coordinator.stop_profile` (`PID 20012` survived). **macOS/Linux:** Same public `InstanceCoordinator` runtime test executes natively. | **`HIGH_PUBLIC_BEHAVIOR`:** Asserts public `rt_after_launch.running == true`, `!is_os_pid_alive(hint_pid)`, and untouched bystander profile. |
| **`LC-08`** | `P0` | `DEFECT-04` (`04a` + `04b`) | `["RUNTIME_REPRODUCED", "STATIC_CONFIRMED", "INFERENCE"]` | **All OSes:** `DEFECT-04a` 4-thread `launch_profile` barrier race is `RUNTIME_REPRODUCED` + `STATIC_CONFIRMED`; `DEFECT-04b` multi-process `profiles.json.tmp` collision (`store.rs:169-180`, `dim-host.ps1:401-415`) is `STATIC_CONFIRMED` + cross-process `INFERENCE`. | **`HIGH_PUBLIC_BEHAVIOR`:** Asserts `SECOND_INSTANCE_REJECTED == 0`, valid `profiles.json`, and untouched bystander profile; agnostic to internal mutex/lockfile naming. |
| **`PM-08`** | `P0` | `DEFECT-09` | `["RUNTIME_REPRODUCED", "STATIC_CONFIRMED", "CONTRACT_MISMATCH"]` | **All OSes:** Live `NodeFixture` launch + `ProfileStore::delete_profile(&store, &id, true)` trait call (`store.rs:271-273`). | **`HIGH_PUBLIC_BEHAVIOR` (Hardened in C2):** Asserts `ProfileStore::delete_profile` returns `Err` (`PROFILE_RUNNING`) + untouched bystander profile; Part 1 atomic-write rollback holds a Windows `FILE_SHARE_READ` lock on `profiles.json` so PID-suffixed `.tmp` fixes (`DEFECT-04b`) remain 100% compatible! |
| **`CT-10`** | `P0` | `DEFECT-08` (`08c`) | `["RUNTIME_REPRODUCED", "STATIC_CONFIRMED", "CONTRACT_MISMATCH", "HOST_DEPENDENT"]` | **Windows:** Differential subprocess run of `dim-cli.exe` vs `scripts/dim-host.ps1` on `0`-byte & `{corrupt` `profiles.json`. **macOS/Linux:** `SKIPPED_HOST_PLATFORM`. | **`HIGH_PUBLIC_BEHAVIOR`:** Asserts public CLI JSON `"ok": false`, matching `"error_code"`, and untouched bystander profile directory. |
| **`LC-09`** | `P1` | `DEFECT-06` | `["RUNTIME_REPRODUCED", "STATIC_CONFIRMED", "HOST_DEPENDENT"]` | **Windows:** Live `NodeFixture` + native `xf-desktop-fixture.exe --lock-mode=exclusive --hang-on-quit` (`PID 9024` survived `stop_profile`). **macOS/Linux:** `NodeFixture --hang-on-quit` exercises `SIGTERM` -> `SIGKILL` escalation natively. | **`HIGH_PUBLIC_BEHAVIOR`:** Asserts `!is_os_pid_alive(pid)` after `coordinator.stop_profile()`. |
| **`XP-LNX-02`** | `P1` | `DEFECT-01` | `["STATIC_CONFIRMED", "HOST_DEPENDENT"]` | **Windows (`#[cfg(not(target_os = "linux"))]`)**: Portable `discover_installation` + `read_linux_pid` + non-host static check tagged `"verification_kind": "STATIC_CONFIRMED_NON_HOST"` (NOT claimed as `RUNTIME_REPRODUCED` on Windows). **Ubuntu (`#[cfg(target_os = "linux"))]`)**: Creates real dangling `SingletonLock -> ubuntu-host-99999` symlink and invokes `LinuxPlatformAdapter::inspect_profile` behaviorally! | **Upgraded in C2:** Native Linux build asserts public `inspect_profile` (`stale_lock_detected == true`, `!running`) with zero source-string coupling; Windows non-host fallback honestly emits `STATIC_CONFIRMED_NON_HOST` and fails `--require-native-os` (exit `3`). |
| **`XP-MAC-03`** | `P1` | `DEFECT-02` (+ `DEFECT-01`) | `["STATIC_CONFIRMED", "CONTRACT_MISMATCH", "HOST_DEPENDENT"]` | **Windows (`#[cfg(not(target_os = "macos"))]`)**: At `a2143550935d`, checked `adapter.rs` strings (`singleton_lock.exists()`, `read_runtime_pid` private helper name, `--user-id`); in C2, tagged `"verification_kind": "STATIC_CONFIRMED_NON_HOST"` with broadened static patterns. **macOS (`#[cfg(target_os = "macos"))]`)**: Creates real dangling `SingletonLock` symlink + launches 2 `NodeFixture` profiles via `InstanceCoordinator` behaviorally! | **Upgraded in C2:** Removes private helper-name fragility (`read_runtime_pid`) on native macOS by testing public `inspect_profile` + 2-profile `launch_profile` (`running == true`, distinct `detected_zalo_user_id`s). |
| **`DS-08`** | `P1` | `DEFECT-07` | `["RUNTIME_REPRODUCED", "STATIC_CONFIRMED", "CONTRACT_MISMATCH"]` | **All OSes:** Portable `FileBackupManager` runtime execution over macOS & Linux `ZaloData` directory trees inside `IsolatedSandbox`. | **`HIGH_PUBLIC_BEHAVIOR`:** Asserts public `BackupManifest.zalo_user_id` and `ProfileRecord.zalo_user_id`. |
| **`CT-08`** | `P1` | `DEFECT-08` (`08a`) | `["RUNTIME_REPRODUCED", "STATIC_CONFIRMED", "CONTRACT_MISMATCH", "HOST_DEPENDENT"]` | **Windows:** Differential subprocess run of `dim-cli.exe` vs `scripts/dim-host.ps1` (`40` vs `32` char slug). **macOS/Linux:** `SKIPPED_HOST_PLATFORM`. | **`HIGH_PUBLIC_BEHAVIOR`:** Compares public JSON `data.directory_name` slug prefix. |
| **`CT-09`** | `P1` | `DEFECT-08` (`08b`) | `["RUNTIME_REPRODUCED", "STATIC_CONFIRMED", "CONTRACT_MISMATCH", "HOST_DEPENDENT"]` | **Windows:** Differential subprocess run of `dim-cli.exe` vs `scripts/dim-host.ps1` (`DUPLICATE_NAME` vs `#2` rename & `64` vs `69` char cap). **macOS/Linux:** `SKIPPED_HOST_PLATFORM`. | **`HIGH_PUBLIC_BEHAVIOR`:** Compares public JSON `"ok"`, `"error_code"`, and `data.name.chars().count() <= 64`. |
| **`PM-04`** | `P2` | `DEFECT-10` | `["RUNTIME_REPRODUCED", "STATIC_CONFIRMED", "CONTRACT_MISMATCH"]` | **All OSes:** Live `JsonProfileStore::create_profile("Đức Công Việc")` + `"đức công việc"`. | **`HIGH_PUBLIC_BEHAVIOR`:** Asserts `Err(DimError::DuplicateProfileName(_))` (`DUPLICATE_NAME`). |
| **`DEFECT-05`** *(Static)* | `P1` | `DEFECT-05` | `["STATIC_CONFIRMED", "CONTRACT_MISMATCH", "HOST_DEPENDENT"]` | **Windows:** `LC-06` is `PASS` (`focus_count == 1`). **macOS/Linux:** `STATIC_CONFIRMED` in `platform-linux/src/adapter.rs:310-318` & `platform-macos/src/adapter.rs:331-356`; `LC-06` enforces `focus_count >= 1` behaviorally on native POSIX runners. | **`HIGH_PUBLIC_BEHAVIOR`:** Asserts `pid1 == pid2` and `fstate.focus_count >= 1`. |
| **`STATIC-TAURI-DIAG-01`** *(Static)* | `P2` | `STATIC-TAURI-DIAG-01` | `["STATIC_CONFIRMED", "CONTRACT_MISMATCH", "HOST_DEPENDENT"]` | **All OSes:** Static inspection of `apps/desktop/src-tauri/src/commands.rs:465-494` vs `dim-cli.rs:580-591`. | **`HIGH_PUBLIC_BEHAVIOR`:** Public Tauri IPC `diagnostics_report` JSON `platform_support_matrix` array entries. |

---

## 4. Controlled Regression Modes & Release Gate Semantics

`xf-test-runner` implements two explicit execution modes (`--mode <baseline-audit|release-regression>`) combined with priority/case selectors (`--priority <p0|p1|p2|p0-p1|p0-p2|regression-guard|all>`, `--case <ID[,ID...]>`, `--require-native-os`) and write-protection for frozen C1 baseline reports:

### 4.1 Deterministic Exit-Code & Mode Contract

| Condition / Outcome | `--mode baseline-audit` *(Default Audit Mode)* | `--mode release-regression` *(Mandatory Post-Fix Gate)* |
| :--- | :--- | :--- |
| **All selected cases `PASS` (`0` defects, `0` unexpected fails)** | **Exit `0`** *(logs `[FIXED_CANDIDATE]` for any of the 11 baseline defect cases that transitioned to `PASS`)* | **Exit `0`** (`RELEASE_GATE_PASSED`) |
| **Known baseline defect reproduces (`FAIL_DEFECT_REPRODUCED`) with `0` unexpected fails** | **Exit `0`** *(logged honestly as `FAIL (DEFECT_REPRODUCED)`, never reported as `PASS`)* | **Exit `2`** (`RELEASE_GATE_FAILED` — prints blocking defect table) |
| **Unexpected test failure (`FAIL_UNEXPECTED`), invalid CLI selector, or protected-report write refusal** | **Exit `1`** | **Exit `2`** on `FAIL_UNEXPECTED` (`RELEASE_GATE_FAILED`); **Exit `1`** on invalid CLI flag / protected-path write refusal |
| **`--require-native-os` encounters `SKIPPED_HOST_PLATFORM` or `STATIC_CONFIRMED_NON_HOST`** | **Exit `3`** (`RELEASE_GATE_INCOMPLETE_NATIVE_OS`) | **Exit `3`** (`RELEASE_GATE_INCOMPLETE_NATIVE_OS`) |
| **Default File-Write Behavior (`plans/reports/reliability/*.json`)** | **Zero JSON writes by default;** frozen C1 paths are write-protected unless `--overwrite-baseline-reports` is explicitly passed. | **Zero JSON writes by default;** pass `--report-out <new_path>` to record post-fix evidence without touching frozen C1 files. |

### 4.2 Why `EXPECTED_FAILURE` / `FAIL_DEFECT_REPRODUCED` Never Passes `release-regression`

1. **Audit Completeness Without False Greens:** During pre-fix auditing (`--mode baseline-audit`), distinguishing `FAIL_DEFECT_REPRODUCED` from `FAIL_UNEXPECTED` allows the full 55-case suite to execute end-to-end, confirming that none of the 42 passing safety guards have regressed (`unexpected_fail == 0`) while cataloging all 11 open defect reproductions. Also, if an Integration Agent patches a bug locally and runs `baseline-audit`, the runner surfaces `[FIXED_CANDIDATE]` immediately.
2. **Strict Release Blocking:** Treating a known bug (`EXPECTED_FAILURE` / `FAIL_DEFECT_REPRODUCED`) as `PASS` in a release gate would allow `P0` concurrency races (`LC-08`), stealth orphan processes (`LC-07`), running-profile deletion (`PM-08`), or silent `0`-byte metadata resets (`CT-10`) to ship green!
3. **Mandatory Verification Gate:** `--mode release-regression` unconditionally rejects both `FAIL_DEFECT_REPRODUCED` and `FAIL_UNEXPECTED` with exit code `2` (`RELEASE_GATE_FAILED`), and with `--require-native-os` rejects non-host skips/static substitutes with exit code `3` (`RELEASE_GATE_INCOMPLETE_NATIVE_OS`). Only genuine behavioral `PASS` transitions clear the gate.
