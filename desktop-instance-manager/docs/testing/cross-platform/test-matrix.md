# Cross-Platform Reliability & Integration Test Matrix

**Repository:** `https://github.com/manhquydev/zalo`  
**Worktree / Branch:** `test/cross-platform-reliability` (Baseline Commit: `a5feb4a52079`)  
**Execution Timestamp (UTC):** `2026-11-17T17:07:34Z`  
**Host Environment:** Windows 11 Pro (`10.0.26300.9974`, `x86_64`, 16 logical CPUs, non-elevated `asInvoker`), `rustc 1.100.0 (0ef1bc386 2026-11-09)`, `node v26.13.1`  
**Deterministic Seed:** `0x20261117`  
**Machine-Readable Evidence:** [`plans/reports/reliability/windows-host-execution-evidence.json`](../../../plans/reports/reliability/windows-host-execution-evidence.json)

---

## 1. Architecture Gap Analysis vs. Existing Repository Tests

Before designing this test system, we audited all existing unit tests (`29` tests across `crates/*`) and scripts (`tests/run-fixture-tests.mjs`, `tests/run-contract-parity-tests.mjs`, `evidence/zalo-live-poc-result.json`) to avoid redundant assertions and focus strictly on uncovered failure modes:

| Subsystem / Capability | Existing Baseline Coverage | Uncovered Gaps Addressed by This Suite |
|---|---|---|
| **Synthetic Desktop Fixture** | `fixtures/test-desktop-app` (`fixture-app.mjs` / `main.rs`) opens `lockfile` with `wx` and **immediately closes the file descriptor**, spawns zero child processes, and has no slow-startup, hang, or I/O fault modes. | Built [`fixtures/cross-platform/`](../../../fixtures/cross-platform/README.md) (`xf-desktop-fixture` + `xf-fixture.mjs`) holding **live OS file locks** (`CreateFileW` `dwShareMode=0` / `flock` + `SingletonLock`), spawning worker process trees (`--spawn-children`), and injecting slow pre/post-lock startup, process hang (`--hang-on-quit`), `mid-write` crashes, slow disk I/O, and controlled errors. |
| **Process Lifecycle** | Tested 2 and 3 instances via PowerShell `dim-host.ps1` sequentially. | Added 1/2/3/5 scaling on native Rust `InstanceCoordinator`, selective 1-of-5 shutdown, out-of-scope external PID survival, pre-lock slow-startup stop race (`LC-07` -> **DEFECT-03**), 4-thread concurrent launch race (`LC-08` -> **DEFECT-04**), and hung `CustomExecutable` cleanup (`LC-09` -> **DEFECT-06**). |
| **Profile Management** | Basic Vietnamese ASCII slug (`Đức - Công việc`) and 4 invalid strings. | Added 25-profile collision stress, NFC vs NFD combining diacritics, emoji-only fallback disambiguation, Vietnamese non-ASCII case-folding duplicate check (`PM-04` -> **DEFECT-10**), 63/64/65 multi-byte char boundaries, `profiles.json.tmp` write-block rollback, and trait `delete_profile` running-guard audit (`PM-08` -> **DEFECT-09**). |
| **Data Safety & Backup** | Single happy-path backup/restore + 1 tampered byte + Windows `mklink /J` delete. | Added junction non-exfiltration during backup, symlink backup-dest refusal, NTFS ADS (`:$DATA`) & zip-slip manifest traversal, mid-copy locked-file `.partial` rollback, exact `5 GiB + payload` boundary, 1,667 concurrent torn-free atomic JSON reads, and macOS/Linux restore `zalo_user_id` preservation (`DS-08` -> **DEFECT-07**). |
| **Cross-Platform & Contracts** | Parity script only tested Windows happy paths and short slugs. | Added real Win32 `RestartManager` kernel lock test, `bInheritHandles=FALSE` inheritable-handle leak test, `icacls` `Everyone:(W)` DACL failure test, macOS/Linux dangling `SingletonLock` audit (**DEFECT-01**, **DEFECT-02**), strict target-OS runtime gates (`SKIPPED` on non-matching OS), and differential CLI fuzzing (**DEFECT-08**). |

---

## 2. Summary of Execution Verdicts (55 Scenarios)

| Suite Category | Total | `PASS` | `FAIL (DEFECT_REPRODUCED)` | `SKIPPED (REQUIRES_TARGET_OS)` | `FAIL (UNEXPECTED)` | Duration (ms) |
|---|---:|---:|---:|---:|---:|---:|
| **Process Lifecycle (`LC-01..09`)** | 9 | 6 | 3 | 0 | 0 | 33,372 |
| **Profile Management (`PM-01..08`)** | 8 | 6 | 2 | 0 | 0 | 3,800 |
| **Data Safety (`DS-01..08`)** | 8 | 7 | 1 | 0 | 0 | 6,960 |
| **Reliability & Soak (`REL-01..08`)** | 8 | 8 | 0 | 0 | 0 | 26,328 |
| **Cross-Platform Differences (`XP-*`)** | 12 | 8 | 2 | 2 | 0 | 1,300 |
| **Adapter & CLI Contracts (`CT-01..10`)** | 10 | 7 | 3 | 0 | 0 | 16,533 |
| **Total** | **55** | **42** | **11** | **2** | **0** | **88,319** |

> [!IMPORTANT]
> **Cross-Platform Honesty Rule Enforced:** Results executed on Windows 11 are **never** extrapolated to mark macOS (`XP-MAC-04`) or Ubuntu (`XP-LNX-04`) OS-kernel runtime gates as `PASS`. Those two gates remain explicitly `SKIPPED (REQUIRES_TARGET_OS)` until executed via `run-macos.sh` on Apple hardware/runner and `run-ubuntu.sh` on an Ubuntu LTS runner.

---

## 3. Complete Test Case Matrix (IDs, Verdicts, Timings, Evidence & Defects)

### 3.1. Process Lifecycle (`LC-01` – `LC-09`)

| Case ID | Scenario | Scope | Verdict | Time (ms) | Observed Evidence & Linked Defect |
|---|---|---|---|---:|---|
| `LC-01` | Launch 1, 2, 3, and 5 concurrent isolated fixture instances | `all` | **PASS** | 7,005 | Checkpoints `1`, `2`, `3`, `5`: distinct PIDs, dirs, and `zalo_user_id`s; `7/7` `verify_isolation` checks `PASS` alongside `11` protected primary Zalo PIDs. |
| `LC-02` | Selective stop of 1 instance while 4 sibling instances stay active | `all` | **PASS** | 5,234 | Stopped profile `10cb7f43` (PID `29184`); all `4` sibling instances kept original PIDs and active locks. |
| `LC-03` | Stop all managed instances and verify clean shutdown persistence | `all` | **PASS** | 3,838 | Stopped `4/4` managed profiles; `clean_shutdown_count == 1` persisted; `surviving_pids = []`. |
| `LC-04` | Protect unmanaged external processes and primary Zalo PIDs | `all` | **PASS** | 724 | Unmanaged external fixture outside `DIM_DATA_DIR` survived managed stop; protected PID rejected with `PROTECTED_PRIMARY_PROCESS`. |
| `LC-05` | Relaunch and recover state after `mid-write` simulated crash | `all` | **PASS** | 1,707 | Detected stale lock + torn `torn_write.bin.tmp`; relaunched cleanly (`boot_sequence = 2`, `unclean_previous_boot = true`, `persisted_kv` preserved). |
| `LC-06` | Duplicate launch while running routes to focus idempotently | `all` | **PASS** | 1,342 | Preserved single owning PID (`boot_sequence = 1`, `focus_count = 1`). *(Note: POSIX headless focus defect tracked in `DEFECT-05`.)* |
| `LC-07` | Slow pre-lock startup (`--slow-startup-pre-lock-ms=3100`) stop safety | `all` | **FAIL (DEFECT_REPRODUCED)** | 2,865 | **`DEFECT-03`**: `launch_profile` returned `running=false, main_pid=null` while `.runtime-pid` (`16580`) was alive; `stop_profile` returned `0` leaving orphan process alive. |
| `LC-08` | Concurrent 4-thread launch/stop race on same profile | `all` | **FAIL (DEFECT_REPRODUCED)** | 1,420 | **`DEFECT-04`**: All 4 concurrent `launch_profile` threads bypassed `if runtime_before.running` simultaneously, spawning racing OS processes (`second_instance_collisions_logged >= 2`). |
| `LC-09` | Simulated process hang (`--hang-on-quit`) forced cleanup | `all` | **FAIL (DEFECT_REPRODUCED)** | 9,237 | **`DEFECT-06`**: `node.exe` hang escalated cleanly, but hung `xf-desktop-fixture.exe` (`lock-mode=exclusive`, detected `running=true` via `rstrtmgr.dll`) survived `stop_profile` (`stop_returned_count = 0`) due to hardcoded `allowed_exe` list. |

### 3.2. Profile Management (`PM-01` – `PM-08`)

| Case ID | Scenario | Scope | Verdict | Time (ms) | Observed Evidence & Linked Defect |
|---|---|---|---|---:|---|
| `PM-01` | Create, lookup, live-safe rename, and confirmed delete | `all` | **PASS** | 35 | Trimmed inputs, preserved immutable `directory_name`, enforced `CONFIRM_REQUIRED` without `--confirm`, returned `NOT_FOUND` post-delete. |
| `PM-02` | Bulk creation of 25 profiles | `all` | **PASS** | 2,269 | `25/25` unique 8-hex IDs and collision-free `<slug>_<8hex>` directories. |
| `PM-03` | Unicode Vietnamese NFC/NFD diacritics, `Đ`/`đ`, and emoji slugs | `all` | **PASS** | 182 | Both NFC and NFD `"Đặng Thị Ánh Tuyết"` folded to `"dang-thi-anh-tuyet"`; emoji names `"🚀🏢✨"` & `"🔥💼🎯"` disambiguated via `_<8hex>`. |
| `PM-04` | Unicode Vietnamese case-insensitive duplicate name rejection | `all` | **FAIL (DEFECT_REPRODUCED)** | 202 | **`DEFECT-10`**: `JsonProfileStore` (`eq_ignore_ascii_case`) allowed creating both `"Đức Công Việc"` and `"đức công việc"`, whereas `dim-host.ps1` rejects with `DUPLICATE_NAME`. |
| `PM-05` | Long name boundaries (`63`, `64`, `65` ASCII & Vietnamese chars) | `all` | **PASS** | 272 | Accepted `64`-char Vietnamese (`128` UTF-8 bytes) and rejected `65`-char names with `INVALID_NAME`. |
| `PM-06` | Duplicate ASCII names & case-only self-rename | `all` | **PASS** | 204 | Blocked cross-profile case/whitespace duplicates (`DUPLICATE_NAME`); allowed self-rename `"Sales Team"` -> `"SALES TEAM"`. |
| `PM-07` | Invalid profile names and unsafe/primary storage roots | `all` | **PASS** | < 1 | Rejected `11` control/separator/traversal names (`INVALID_NAME`), `4` bad roots (`PATH_TRAVERSAL`), and primary `ZaloData` (`PROTECTED_PRIMARY_PATH`). |
| `PM-08` | Write-failure rollback on `profiles.json.tmp` & trait `delete_profile` guard | `all` | **FAIL (DEFECT_REPRODUCED)** | 636 | Blocked `profiles.json.tmp` returned `IO_ERROR` without corrupting `profiles.json`; **`DEFECT-09`**: trait method `ProfileStore::delete_profile` bypassed `PROFILE_RUNNING` (`is_running = false` hardcoded). |

### 3.3. Data Safety (`DS-01` – `DS-08`)

| Case ID | Scenario | Scope | Verdict | Time (ms) | Observed Evidence & Linked Defect |
|---|---|---|---|---:|---|
| `DS-01` | Strict 3-profile data separation & SHA-256 chunk verification | `all` | **PASS** | 2,487 | `3` concurrent profiles wrote `4 x 2048 B` (`8,192 B`) chunks each; `3` distinct SHA-256 streams verified on disk with zero cross-profile leakage. |
| `DS-02` | Symlinks & Windows NTFS junctions (`mklink /J`) on backup/delete | `all` | **PASS** | 98 | Backup skipped junction (`0` external files leaked); symlink `dest_dir` rejected (`BACKUP_ERROR`); profile delete unlinked junction while keeping external vault file intact. |
| `DS-03` | Path traversal, reserved names, NTFS ADS (`:$DATA`), & zip-slip manifest | `all` | **PASS** | 16 | Rejected all `21` traversal/device/ADS vectors (`PATH_TRAVERSAL`) and blocked forged `../victim.txt` `backup.json` entry (`PATH_TRAVERSAL`). |
| `DS-04` | Backup partial failure mid-copy & `.partial` rollback | `all` | **PASS** | 124 | Exclusive read-deny lock on 2nd file triggered `IO_ERROR` mid-backup and rolled back `<backup>.partial` (`leftover_partial_dirs = 0`). |
| `DS-05` | Restore tamper/interruption & duplicate-name rollback | `all` | **PASS** | 168 | Duplicate target name (`DUPLICATE_NAME`) and tampered byte (`BACKUP_ERROR`) aborted cleanly with `0` `.restore-*` remnants and untouched source profile. |
| `DS-06` | Disk full simulation (`5 GiB + payload` exact boundary) | `all` | **PASS** | 119 | `5 GiB + 10,000 - 1` bytes rejected (`INSUFFICIENT_DISK_SPACE`) on both backup and restore; exact `5 GiB + 10,000` bytes succeeded. |
| `DS-07` | Concurrent atomic `profiles.json` reads/writes & staging sweep | `all` | **PASS** | 3,428 | `90` rapid atomic mutations observed across `1,667` concurrent JSON reads with `0` torn/invalid frames; swept `2` stale staging dirs while preserving fresh `<1h` staging dir. |
| `DS-08` | Cross-platform macOS/Linux backup & restore `zalo_user_id` preservation | `all` | **FAIL (DEFECT_REPRODUCED)** | 520 | **`DEFECT-07`**: Post-launch QR logins in macOS (`Library/Application Support/ZaloData`) and Linux (`.config/ZaloData`) lost `zalo_user_id` (`null`) on backup & restore because `backup.rs` hardcodes Windows `AppData/Roaming/ZaloData`. |

### 3.4. Reliability & Stress (`REL-01` – `REL-08`)

| Case ID | Scenario | Scope | Verdict | Time (ms) | Observed Evidence & Linked Defect |
|---|---|---|---|---:|---|
| `REL-01` | 10 consecutive full coordinator restart cycles | `all` | **PASS** | 7,150 | `10/10` cycles clean (`clean_shutdown_count = 10`, `surviving_pids = 0`). |
| `REL-02` | 100 native fixture launch/stop soak cycles | `all` | **PASS** | 5,806 | `100/100` cycles (`failure_rate = 0.0`, startup `p50 = 30.76 ms`, `p95 = 36.68 ms`; shutdown `p50 = 26.46 ms`; `surviving_pids = 0`). |
| `REL-03` | Crash & controlled error injection matrix | `all` | **PASS** | 480 | Verified exit codes & artifacts across `disk-full` (`28`), `eacces` (`13`), `corrupt-cfg` (`65`), `immediate` (`101`), `after-lock` (`137`), `corrupt-state` (`139`). |
| `REL-04` | Slow disk I/O simulation (`--slow-io-ms=25`) + slow space probe | `all` | **PASS** | 980 | Backed up and verified SHA-256 across `17` files (`28,436` bytes) under throttled I/O. |
| `REL-05` | 5 concurrent heartbeating profiles under live status/rename load | `all` | **PASS** | 7,335 | Completed `25` live renames while `5` instances recorded `375` WAL heartbeats without lock contention. |
| `REL-06` | Corrupted metadata resilience | `all` | **PASS** | 52 | Verified `PROFILE_MARKER_MISMATCH` on tampered `.zalo-profile-id`, `JSON_ERROR` on empty/truncated `profiles.json`, `SCHEMA_ERROR` on `v0`/`v99`, and non-panicking `config.json` parse. |
| `REL-07` | Process tree cleanup (`--spawn-children=2 --orphan-children --hang-on-quit`) | `all` | **PASS** | 4,394 | On Windows, `ProcessSnapshot::collect_tree` found all descendant `node.exe` workers (`inspected_process_count = 6`) and left `0` surviving children. *(POSIX lack of tree kill documented in `DEFECT-06`.)* |
| `REL-08` | Stale locks, binary garbage lockfile, & PID-reuse spoof protection | `all` | **PASS** | 131 | Cleaned `4` stale lock/signal artifacts; writing current test runner PID into `lockfile` reported `running = false` (preventing false PID-reuse collision on Windows). |

### 3.5. Cross-Platform Differences (`XP-WIN-*`, `XP-MAC-*`, `XP-LNX-*`)

| Case ID | Scenario | Scope | Verdict | Time (ms) | Observed Evidence & Linked Defect |
|---|---|---|---|---:|---|
| `XP-WIN-01` | Windows `rstrtmgr.dll` & `CreateFileW(ShareMode=0)` kernel lock | `windows` | **PASS** | 120 | `get_locking_pids` matched native fixture PID; `exclusively_locked = true` while running and `false` on exit. |
| `XP-WIN-02` | Windows `AppData` UTF-16 env block, quoting & Folder Redirection | `windows` | **PASS** | 542 | Double-NUL UTF-16 block (`497` units), child `APPDATA` confirmed isolated inside profile dir, `4` redirection patterns verified. |
| `XP-WIN-03` | Windows `CreateProcessW(..., bInheritHandles=0)` handle isolation | `windows` | **PASS** | 538 | Explicitly inheritable (`bInheritHandle = 1`) parent Win32 file handle was **not** inherited by child process. |
| `XP-WIN-04` | Windows NTFS DACL check (`GetNamedSecurityInfoW` + `icacls`) | `windows` | **PASS** | 31 | Default `Profiles/` returned `Some(true)`; granting `*S-1-1-0:(OI)(CI)(W)` (`Everyone` write) flipped check to `Some(false)`. |
| `XP-MAC-01` | macOS `Application Support`, layout seeds, & `Info.plist` discovery | `macos-contract` | **PASS** | 22 | Resolved `~/Library/Application Support/MultiZaloDesktop`, parsed `26.10.20` from `Zalo.app`, reported `NotTested`. |
| `XP-MAC-02` | macOS `open -n --env CFFIXED_USER_HOME` & call socket conflict | `macos-contract` | **PASS** | 6 | Verified `11`-arg `/usr/bin/open` vector, `7` symlink dirs (`Library/Keychains`, etc.), and `/tmp/socketzalorecv2021` warning. |
| `XP-MAC-03` | macOS `SingletonLock` dangling symlink & `NodeFixture` launch parity | `macos-contract` | **FAIL (DEFECT_REPRODUCED)** | 7 | **`DEFECT-02`** (and **`DEFECT-01`**): `MacOsPlatformAdapter` uses `singleton_lock.exists()`, ignores `.runtime-pid`, and omits deterministic `--user-id` on `NodeFixture` launch. |
| `XP-MAC-04` | macOS Runtime Gate (`CFFIXED_USER_HOME`, `osascript`, APFS `0700`) | `macos` | **SKIPPED (REQUIRES_TARGET_OS)** | 0 | Skipped honestly on Windows host; execute via `tests/cross-platform/scripts/run-macos.sh` on macOS. |
| `XP-LNX-01` | Ubuntu XDG Base Directory precedence & isolated env map | `linux-contract` | **PASS** | 11 | Verified `XDG_DATA_HOME` override over `~/.local/share` and `7` isolated env keys (`XDG_CONFIG_HOME`, `TMPDIR`, etc.). |
| `XP-LNX-02` | Ubuntu `.AppImage` discovery, `OfficialZalo` refusal & `SingletonLock` | `linux-contract` | **FAIL (DEFECT_REPRODUCED)** | 13 | **`DEFECT-01`**: Official Zalo -> `Unsupported`, `.AppImage` -> `Experimental`, but `singleton_lock.exists()` fails on Chromium's dangling `SingletonLock` symlinks. |
| `XP-LNX-03` | Ubuntu Bubblewrap (`bwrap`) user-namespace argv & AppArmor note | `linux-contract` | **PASS** | < 1 | Verified `41`-arg `bwrap` sandbox (`--die-with-parent`, `--ro-bind /usr /usr`) and Ubuntu 23.10+/24.04 AppArmor `--no-sandbox` note. |
| `XP-LNX-04` | Ubuntu Runtime Gate (`/proc`, `SIGTERM`->`SIGKILL`, `bwrap` userns) | `linux` | **SKIPPED (REQUIRES_TARGET_OS)** | 0 | Skipped honestly on Windows host; execute via `tests/cross-platform/scripts/run-ubuntu.sh` on Ubuntu LTS. |

### 3.6. Adapter & CLI Contracts (`CT-01` – `CT-10`)

| Case ID | Scenario | Scope | Verdict | Time (ms) | Observed Evidence & Linked Defect |
|---|---|---|---|---:|---|
| `CT-01` | `PlatformCapabilities` contract across all 3 OS adapters | `all` | **PASS** | < 1 | All 3 adapters emit valid `PlatformCapabilityReport`, non-empty limitations/diagnostics, and `Blocked`/`Unsupported` concurrent calls. |
| `CT-02` | `ApplicationLocator` input/output schema roundtrip | `all` | **PASS** | 6 | Missing roots fail closed (`is_valid = false`) and round-trip identically via JSON across Windows, macOS, and Linux. |
| `CT-03` | `ensure_profile_layout` idempotency & `.zalo-profile-id` contract | `all` | **PASS** | 33 | Double-invoking layout creation succeeded on all 3 adapters with verified `cafebabe` identity markers. |
| `CT-04` | Exhaustive 22-variant `DimError::code()` & `OperationOutcome<T>` | `all` | **PASS** | < 1 | All `22` `DimError` variants map to `22` distinct stable upper-snake-case error codes. |
| `CT-05` | Unsupported platform & unsigned `CustomExecutable` safety | `all` | **PASS** | 104 | Linux `OfficialZalo` -> `UNSUPPORTED_PLATFORM`; cross-OS `inspect_primary` -> `UNSUPPORTED_PLATFORM`; Windows unsigned PE -> `UNVERIFIED_BINARY`. |
| `CT-06` | Pre-syscall `PROTECTED_PRIMARY_PROCESS` guard on all 3 `stop()` impls | `all` | **PASS** | 6 | Windows, macOS, and Linux `stop()` all reject overlapping primary PIDs before invoking any OS kill API. |
| `CT-07` | CLI JSON output schema (`discover`, `status`, `diagnostics`, `verify-isolation`, `backups`) | `all` | **PASS** | 244 | All `5` read commands match required top-level envelope keys and `7`-check isolation array. |
| `CT-08` | Differential CLI contract: 33..40 char slug length (`dim-cli` vs `dim-host.ps1`) | `windows+ps-parity` | **FAIL (DEFECT_REPRODUCED)** | 1,991 | **`DEFECT-08` (a)**: `42`-char name truncated to `40` chars in Rust vs `32` chars in PowerShell. |
| `CT-09` | Differential CLI contract: Restore duplicate name & `64`-char cap | `windows+ps-parity` | **FAIL (DEFECT_REPRODUCED)** | 7,757 | **`DEFECT-08` (b)**: Rust rejected duplicate `--name` (`DUPLICATE_NAME`) and capped auto-restore at `64` chars; PowerShell silently renamed (`#2`) and overflowed to `69` chars. |
| `CT-10` | Differential CLI contract: Empty/corrupt `profiles.json` & missing-arg codes | `windows+ps-parity` | **FAIL (DEFECT_REPRODUCED)** | 6,392 | **`DEFECT-08` (c)**: On `0`-byte `profiles.json`, Rust failed closed (`JSON_ERROR`) while PowerShell silently returned `ok: true` with empty profile list. |

---

## 4. Related Documentation

- Reliability & Telemetry Analysis: [`reliability-report.md`](./reliability-report.md)
- Adapter & CLI Contract Analysis: [`adapter-contract-report.md`](./adapter-contract-report.md)
- Authorized Human Live Zalo Checklist: [`live-zalo-authorized-checklist.md`](./live-zalo-authorized-checklist.md)
- Shared Workspace & GitHub Actions Integration: [`workspace-and-ci-integration.md`](./workspace-and-ci-integration.md)
