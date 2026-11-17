# Mission C2 — Independent Post-Fix Verification Report & Defect Lifecycle Ledger

| Verification Header Field | Authoritative Value |
| :--- | :--- |
| **Worktree / Branch** | `test/cross-platform-reliability` |
| **Baseline Test SHA** | `a2143550935d75c1569123e628eccb3358654834` |
| **Product Baseline SHA (`main`)** | `a5feb4a520798064c1508c26fe4c4ac17a3a03cf` |
| **Integration SHA Verified** | **`NONE`** *(inspected `git show-ref` on `2026-11-17`; zero post-fix integration commits exist in local or remote refs)* |
| **Host OS** | `Windows 10.0.26200 x86_64` *(Windows 11 Pro 64-bit, kernel build `10.0.26300.9974`, non-elevated `asInvoker`)* |
| **Verification Verdict** | **`NOT_YET_VERIFIED`** |
| **Execution Boundary Status** | **`AWAITING_INTEGRATION_REVISION`** |
| **Frozen Baseline Manifest** | [`plans/reports/reliability/baseline-manifest.json`](../../../plans/reports/reliability/baseline-manifest.json) |
| **Actionable Handoff Specification** | [`regression-handoff.md`](./regression-handoff.md) |

> [!IMPORTANT]
> **Strict Non-Fabrication & No-Guessing Boundary:** Per Mission C2 Section 5 and Section 8, `git show-ref` was audited across the repository: `refs/heads/main` stands at the pre-fix product baseline `a5feb4a520798064c1508c26fe4c4ac17a3a03cf`, and no post-fix Integration Agent branch or commit SHA has been published yet. Rather than guessing, cherry-picking, or merging unverified branches, this report freezes the authoritative **Before** state, locks the `OPEN -> REPRODUCED -> FIXED_CANDIDATE -> VERIFIED` lifecycle ledger, and defines the exact mechanical verification protocol to execute the moment an Integration SHA is delivered.

---

## 1. Frozen Pre-Fix Baseline Integrity Summary (`a2143550935d`)

Before any post-fix verification run is accepted, the verifier confirms that the frozen C1 baseline artifacts remain byte-for-byte intact so before/after comparisons are cryptographically auditable:

| Frozen Baseline Artifact | Byte Size | SHA-256 Digest | Baseline Verdict Counts |
| :--- | ---: | :--- | :--- |
| [`plans/reports/reliability/windows-host-execution-evidence.json`](../../../plans/reports/reliability/windows-host-execution-evidence.json) | `52,991` B | `e895ad959568f7bccd10f12ff1656fb50e037e17c32fe5db3b4f9c41cf1ed7b9` | `55` total: `42` `PASS`, `11` `FAIL_DEFECT_REPRODUCED`, `2` `SKIPPED_HOST_PLATFORM`, `0` `FAIL_UNEXPECTED` |
| [`plans/reports/reliability/metrics-benchmark-windows.json`](../../../plans/reports/reliability/metrics-benchmark-windows.json) | `3,140` B | `bed0c193b7b6c82f2c45f7821c9bd9e033ebf3b6354e09a38bd7998dc2613ab4` | Native startup `p50 = 27.72 ms`; Coordinator+Node startup `p50 = 329.35 ms`; `0` residuals |
| [`plans/reports/reliability/defect-reproductions.json`](../../../plans/reports/reliability/defect-reproductions.json) | `11,030` B | `787b55c7a376f8932c11a828b6d4144c86f1c6e18f82a993f9e626964e09577b` | `9` dynamically cataloged defect entries (`DEFECT-01..04`, `DEFECT-06..10`) |
| [`plans/reports/reliability/baseline-manifest.json`](../../../plans/reports/reliability/baseline-manifest.json) | `96,947` B | `8365d25f9658252a8085added689ec67d6898688e0ca979a575103e86bedbf58` | `55` canonical case contracts + `2` static findings (`DEFECT-05`, `STATIC-TAURI-DIAG-01`) |

---

## 2. Defect Lifecycle State Ledger (`OPEN -> REPRODUCED -> FIXED_CANDIDATE -> VERIFIED`)

### 2.1 State Transition Rules

1. **`OPEN`:** Defect proven by static source audit (`STATIC_CONFIRMED` / `STATIC_CONFIRMED_NON_HOST`) on a non-target host OS where live target-kernel behavioral execution has not yet run (e.g., POSIX symlink/signal paths audited from Windows).
2. **`REPRODUCED`:** Defect deterministically reproduced at runtime on the target/host OS (`RUNTIME_REPRODUCED` in `windows-host-execution-evidence.json` at `a5feb4a52079`).
3. **`FIXED_CANDIDATE`:** Integration Agent delivers an explicit commit SHA (`<INTEGRATION_SHA>`) claiming remediation (or `--mode baseline-audit` logs `[FIXED_CANDIDATE]`), awaiting independent full-gate verification.
4. **`VERIFIED`:** Independently verified at `<INTEGRATION_SHA>` via `--mode release-regression` (exit `0`), with bystander profile invariance confirmed, adjacent suites non-regressed (`0` `FAIL_UNEXPECTED`), `REL-01` (10 restarts) + `REL-02` (100 soak cycles) green, and native OS runner execution completed for OS-specific paths. **A Windows run can never mark macOS-only or Linux-only runtime behavior `VERIFIED`.**

### 2.2 Complete Before / After Defect Verification Table

| Defect / Finding ID | Priority | Linked Test ID(s) | Implicated Product Files (`a5feb4a52079`) | Pre-Fix Windows Baseline (`a5feb4a52079`) | Current Lifecycle State | Post-Fix Candidate SHA | Post-Fix Windows Verdict | Post-Fix macOS / Ubuntu Native Verdict | Bystander & Adjacent Non-Regression |
| :--- | :---: | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **`DEFECT-04a`** *(Concurrent 4-thread launch race)* | **`P0`** | `LC-08` | `crates/process-manager/src/coordinator.rs:49-104`, `scripts/dim-host.ps1:2055-2165` | `FAIL_DEFECT_REPRODUCED` (`second_instance_collisions_logged = 2`) | **`REPRODUCED`** | `NONE` | `NOT_YET_VERIFIED` | `NOT_YET_VERIFIED` | `PENDING` (`LC-01`, `LC-06`, `REL-05`) |
| **`DEFECT-04b`** *(Unscoped `profiles.json.tmp` write)* | **`P0`** | `LC-08`, `PM-08` | `crates/profile-store/src/store.rs:169-180`, `scripts/dim-host.ps1:401-415` | `STATIC_CONFIRMED` + `INFERENCE` (`LC-08` / `PM-08`) | **`REPRODUCED`** | `NONE` | `NOT_YET_VERIFIED` | `NOT_YET_VERIFIED` | `PENDING` (`DS-07`, `PM-02`, `PM-08`) |
| **`DEFECT-03`** *(Slow pre-lock startup blind window & orphan)* | **`P0`** | `LC-07` | `crates/platform-windows/src/adapter.rs:512-522,620-631`, `crates/process-manager/src/coordinator.rs:113-120`, `crates/platform-{macos,linux}/src/adapter.rs` | `FAIL_DEFECT_REPRODUCED` (`PID 20012` orphaned, `stop = 0`) | **`REPRODUCED`** *(Win)* / **`OPEN (STATIC_CONFIRMED_NON_HOST)`** *(Mac/Lnx)* | `NONE` | `NOT_YET_VERIFIED` | `NOT_YET_VERIFIED` | `PENDING` (`LC-04`, `REL-01`, `REL-08`) |
| **`DEFECT-09`** *(Unsafe `ProfileStore::delete_profile` trait)* | **`P0`** | `PM-08` | `crates/profile-store/src/store.rs:271-273` | `FAIL_DEFECT_REPRODUCED` (`trait_delete_profile_bypassed_running_guard = true`) | **`REPRODUCED`** | `NONE` | `NOT_YET_VERIFIED` | `NOT_YET_VERIFIED` | `PENDING` (`PM-01`, `DS-02`, `LC-02`) |
| **`DEFECT-08c`** *(`0`-byte / corrupt `profiles.json` silent reset)* | **`P0`** | `CT-10` | `scripts/dim-host.ps1:380-399,3005-3055`, `crates/profile-store/src/store.rs:157-167`, `dim-cli.rs:171-175` | `FAIL_DEFECT_REPRODUCED` (`powershell_ok = true` on `0`-byte JSON) | **`REPRODUCED`** | `NONE` | `NOT_YET_VERIFIED` | `N/A (Win PowerShell parity; Rust covered by REL-06)` | `PENDING` (`REL-06`, `CT-04`, `CT-07`) |
| **`DEFECT-06`** *(Hung `CustomExecutable` & POSIX `SIGKILL`/tree kill)* | **`P1`** | `LC-09`, `REL-07` *(POSIX)* | `crates/platform-windows/src/adapter.rs:492-498,771-785`, `scripts/dim-host.ps1:1032-1036,1117-1119,2295-2298`, `crates/platform-macos/src/adapter.rs:386-404`, `crates/platform-linux/src/adapter.rs:206,342-360` | `FAIL_DEFECT_REPRODUCED` (`PID 9024` survived `stop_profile`) | **`REPRODUCED`** *(Win)* / **`OPEN (STATIC_CONFIRMED_NON_HOST)`** *(Mac/Lnx)* | `NONE` | `NOT_YET_VERIFIED` | `NOT_YET_VERIFIED` | `PENDING` (`LC-04`, `CT-06`, `REL-07`) |
| **`DEFECT-01` (Linux)** *(Dangling `SingletonLock` symlink `.exists()`)* | **`P1`** | `XP-LNX-02` | `crates/platform-linux/src/adapter.rs:190-195` | `FAIL_DEFECT_REPRODUCED` (`STATIC_CONFIRMED_NON_HOST` on Win) | **`OPEN (STATIC_CONFIRMED_NON_HOST)`** | `NONE` | `NOT_YET_VERIFIED` *(Static check only)* | `NOT_YET_VERIFIED` *(Requires Ubuntu 24.04)* | `PENDING` (`XP-LNX-01..04`) |
| **`DEFECT-01` (macOS)** *(Dangling `SingletonLock` symlink `.exists()`)* | **`P1`** | `XP-MAC-03` | `crates/platform-macos/src/adapter.rs:194-199` | `FAIL_DEFECT_REPRODUCED` (`STATIC_CONFIRMED_NON_HOST` on Win) | **`OPEN (STATIC_CONFIRMED_NON_HOST)`** | `NONE` | `NOT_YET_VERIFIED` *(Static check only)* | `NOT_YET_VERIFIED` *(Requires macOS)* | `PENDING` (`XP-MAC-01..04`) |
| **`DEFECT-02`** *(POSIX `.runtime-pid`, readiness poll, & `--user-id`)* | **`P1`** | `XP-MAC-03` | `crates/platform-macos/src/adapter.rs:192-196,298,305-320`, `crates/platform-linux/src/adapter.rs:189,262-275,298-300` | `FAIL_DEFECT_REPRODUCED` (`STATIC_CONFIRMED_NON_HOST` on Win) | **`OPEN (STATIC_CONFIRMED_NON_HOST)`** | `NONE` | `NOT_YET_VERIFIED` *(Static check only)* | `NOT_YET_VERIFIED` *(Requires macOS & Ubuntu)* | `PENDING` (`LC-01`, `XP-MAC-04`, `XP-LNX-04`) |
| **`DEFECT-07`** *(macOS/Linux backup & restore `zalo_user_id` loss)* | **`P1`** | `DS-08` | `crates/profile-store/src/backup.rs:143,339-345,455-460` | `FAIL_DEFECT_REPRODUCED` (`mac_restored_uid = null`, `linux_restored_uid = null`) | **`REPRODUCED`** | `NONE` | `NOT_YET_VERIFIED` | `NOT_YET_VERIFIED` | `PENDING` (`DS-02..06`, `REL-04`) |
| **`DEFECT-08a`** *(Slug cap `40` in Rust vs `32` in PowerShell)* | **`P1`** | `CT-08` | `scripts/dim-host.ps1:582`, `crates/core/src/slug.rs:14,70` | `FAIL_DEFECT_REPRODUCED` (`40` vs `32` chars) | **`REPRODUCED`** | `NONE` | `NOT_YET_VERIFIED` | `N/A (Win PowerShell parity)` | `PENDING` (`PM-03`, `PM-05`) |
| **`DEFECT-08b`** *(Restore duplicate `--name` & `69`-char overflow)* | **`P1`** | `CT-09` | `scripts/dim-host.ps1:2595-2608`, `crates/profile-store/src/backup.rs:399-437` | `FAIL_DEFECT_REPRODUCED` (`ok: true` `#2` rename; `69 > 64` chars) | **`REPRODUCED`** | `NONE` | `NOT_YET_VERIFIED` | `N/A (Win PowerShell parity)` | `PENDING` (`DS-05`, `PM-05`) |
| **`DEFECT-05`** *(POSIX `focus_or_restore` no-op / headless signal)* | **`P1`** | `LC-06` *(POSIX)* | `crates/platform-linux/src/adapter.rs:310-318`, `crates/platform-macos/src/adapter.rs:331-356` | `PASS` on Windows (`focus_count = 1`); `STATIC_CONFIRMED` on POSIX | **`OPEN (STATIC_CONFIRMED_NON_HOST)`** | `NONE` | `PASS` *(Windows baseline)* | `NOT_YET_VERIFIED` *(Requires macOS & Ubuntu)* | `PENDING` (`LC-06` on Mac/Lnx) |
| **`DEFECT-10`** *(Vietnamese `Đức` vs `đức` duplicate name check)* | **`P2`** | `PM-04` | `crates/profile-store/src/store.rs:100,216,260,439`, `crates/profile-store/src/backup.rs:406,416,428` | `FAIL_DEFECT_REPRODUCED` (both profiles allowed in Rust) | **`REPRODUCED`** | `NONE` | `NOT_YET_VERIFIED` | `NOT_YET_VERIFIED` | `PENDING` (`PM-03`, `PM-06`) |
| **`STATIC-TAURI-DIAG-01`** *(Tauri `diagnostics_report` matrix row duplication)* | **`P2`** | `CT-01`, `CT-07` + Static IPC Check | `apps/desktop/src-tauri/src/commands.rs:465-494` | `STATIC_CONFIRMED` (duplicates macOS/Ubuntu rows on non-Win) | **`OPEN (STATIC_CONFIRMED)`** | `NONE` | `NOT_YET_VERIFIED` | `NOT_YET_VERIFIED` | `PENDING` (`CT-01`, `CT-07`) |

---

## 3. Independent Post-Fix Verification Protocol (Section 5 Runbook)

Once the Integration Agent delivers an explicit candidate commit SHA (`<INTEGRATION_SHA>`), the independent verification agent MUST execute all **9 sequential gates** below in order without skipping steps, auto-merging branches, or mutating frozen C1 files.

### Step 0 — Revision Pinning & Primary Zalo Invariance Snapshot

1. Confirm `<INTEGRATION_SHA>` exists in git object storage (`git cat-file -t <INTEGRATION_SHA>` == `commit`) and record `git rev-parse <INTEGRATION_SHA>`. **Never guess a branch tip or perform an ad-hoc merge.**
2. Check out `<INTEGRATION_SHA>` in a clean, isolated verification worktree (leaving `main` untouched) with `tests/cross-platform` and `fixtures/cross-platform` present at or rebased cleanly onto `<INTEGRATION_SHA>`.
3. Snapshot live host primary Zalo Desktop invariance before running any command:
   ```powershell
   # Record running primary Zalo PIDs and primary config.json SHA-256 (read-only)
   Get-Process Zalo* -ErrorAction SilentlyContinue | Select-Object Id, ProcessName
   if (Test-Path "$env:APPDATA\ZaloData\config.json") {
       Get-FileHash "$env:APPDATA\ZaloData\config.json" -Algorithm SHA256
   }
   ```
4. Verify frozen C1 baseline files (`windows-host-execution-evidence.json`, `metrics-benchmark-windows.json`, `defect-reproductions.json`) still match the exact SHA-256 digests in Section 1.

### Step 1 — Rust Workspace, Tauri Crate, & Harness Static Lint Gate

```bash
# From desktop-instance-manager/:
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked

# Verify Tauri backend compiles cleanly (including STATIC-TAURI-DIAG-01 fix in commands.rs:465-494):
cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml

# Verify standalone fixture & cross-platform harness crates compile with 0 warnings:
cargo fmt --manifest-path fixtures/cross-platform/Cargo.toml -- --check
cargo fmt --manifest-path tests/cross-platform/Cargo.toml -- --check
cargo clippy --manifest-path fixtures/cross-platform/Cargo.toml --all-targets -- -D warnings
cargo clippy --manifest-path tests/cross-platform/Cargo.toml --all-targets -- -D warnings
```
- **Pass Criteria:** Exit `0` across all commands; `0` compiler or clippy warnings; all unit tests in `dim-core`, `dim-profile-store`, `dim-process-manager`, `dim-platform-windows`, `dim-platform-macos`, and `dim-platform-linux` pass.

### Step 2 — Release Binary Build & Existing Fixture / Parity Scripts

```bash
cargo build --manifest-path fixtures/cross-platform/Cargo.toml --release
cargo build -p dim-test-fixture --bin dim-cli --release --locked
node tests/run-fixture-tests.mjs
node tests/run-contract-parity-tests.mjs
```
- **Pass Criteria:** Both legacy fixture and contract parity test suites exit `0` without regressions.

### Step 3 — Mandatory Controlled `release-regression` Priority Gates (`P0`, `P1`, `P2`)

```bash
# 1. P0 Gate (LC-07, LC-08, PM-08, CT-10) -> MUST exit 0 (pre-fix exited 2)
cargo run --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner -- --mode release-regression --priority p0

# 2. P1 Gate (LC-09, DS-08, XP-MAC-03, XP-LNX-02, CT-08, CT-09) -> MUST exit 0 on Windows (with XP-MAC-03/XP-LNX-02 noted STATIC_CONFIRMED_NON_HOST until Step 9 native OS runs)
cargo run --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner -- --mode release-regression --priority p1

# 3. P2 Gate (PM-04) -> MUST exit 0
cargo run --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner -- --mode release-regression --priority p2

# 4. Cargo #[ignore] mandatory post-fix gates
cargo test --manifest-path tests/cross-platform/Cargo.toml --release -- --ignored release_gate_p0_mandatory --nocapture
cargo test --manifest-path tests/cross-platform/Cargo.toml --release -- --ignored release_gate_p0_p1_mandatory --nocapture
cargo test --manifest-path tests/cross-platform/Cargo.toml --release -- --ignored strict_zero_product_defects_gate --nocapture
```
- **Pass Criteria:** Every command exits `0`; `defects_reproduced_cases == 0` and `unexpected_failure_cases == 0`.

### Step 4 — Individual Single-Case Reproducer & Bystander Non-Regression Audit

Execute each individual defect reproducer to confirm clean `PASS` evidence and untouched sibling/bystander profiles:

```bash
for case_id in LC-08 LC-07 PM-08 CT-10 LC-09 XP-LNX-02 XP-MAC-03 DS-08 CT-08 CT-09 PM-04 LC-06; do
  cargo run --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner -- --mode release-regression --case "${case_id}"
done
```
- **Pass Criteria:** Each case transitions from `FAIL_DEFECT_REPRODUCED` to `PASS` (`exit 0`), and bystander profiles inside `LC-07`, `LC-08`, `PM-08`, and `CT-10` report zero unintended mutations.

### Step 5 — Restart & Long-Haul Soak Stability (`REL-01` 10 Cycles + `REL-02` 100 Cycles)

```bash
# Run full 55-case suite with 100 soak cycles in release-regression mode, writing to a NEW post-fix evidence file:
cargo run --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner -- \
  --mode release-regression \
  --suite all \
  --cycles 100 \
  --metrics-runs 10 \
  --seed 0x20261117 \
  --report-out plans/reports/reliability/post-fix-windows-execution-evidence.json
```
- **Pass Criteria:**
  - `REL-01`: `10/10` full `InstanceCoordinator` launch/stop cycles (`clean_shutdown_count == 10`, `surviving_pids == 0`).
  - `REL-02`: `100/100` fixture launch/stop soak cycles (`failure_rate == 0.0`, `surviving_pids == 0`, no progressive handle/latency degradation).
  - Frozen C1 `windows-host-execution-evidence.json` remains byte-for-byte unchanged (`SHA-256: e895ad95...`).

### Step 6 — Metadata & Backup Integrity Verification

- Inspect outcomes of `DS-01`..`DS-08`, `REL-06`, `PM-08`, and `CT-10` in `post-fix-windows-execution-evidence.json`:
  - `DS-07`: `0` torn reads across concurrent `profiles.json` readers + abandoned `.partial`/`.restore-*` staging sweep intact.
  - `PM-08` & `LC-08`: Atomic `profiles.json` writes survive contention/rename locks without corrupting `profiles.json` or leaving `.tmp` debris.
  - `CT-10` & `REL-06`: `0`-byte, truncated, and bad-schema `profiles.json` files fail closed (`JSON_ERROR` / `SCHEMA_ERROR`) across both `dim-cli` and `scripts/dim-host.ps1`.
  - `DS-08`: macOS and Linux post-launch `zalo_user_id` values (`849777666`, `849888999`) survive offline backup & restore.

### Step 7 — Process Ownership & Post-Run Zero-Orphan Sweep

- Verify `LC-02` (selective 1-of-5 stop), `LC-03` (stop all 4), `LC-07` (pre-lock startup stop), `LC-09` (hung `NodeFixture` + `xf-desktop-fixture.exe` forced kill), `REL-07` (descendant worker tree cleanup), and `REL-08` (PID-reuse spoof protection).
- Run host OS process sweep immediately after suite completion:
  ```powershell
  Get-Process xf-desktop-fixture -ErrorAction SilentlyContinue
  # Must return empty (0 surviving fixture processes)
  ```

### Step 8 — Primary Zalo Desktop Protection Invariance

- Verify `LC-04` (`PROTECTED_PRIMARY_PROCESS` + unmanaged external process survival), `CT-06` (pre-syscall PID guard across all 3 adapters), `PM-07` (`PROTECTED_PRIMARY_PATH`), and `DS-04`.
- Re-run the Step 0 primary Zalo PID and `%APPDATA%\ZaloData\config.json` SHA-256 check and confirm **100% identity** with the pre-test snapshot.

---

## 4. Native OS Validation Matrix & Platform Blockers (Section 6)

Per Mission C2 Section 6, cross-platform verification strictly separates what has been proven on the current Windows 11 workstation from what requires native Linux and macOS kernels — and strictly separates **synthetic fixture verification** from **official VNG Zalo Desktop qualification**.

### 4.1 Per-OS Verification Matrix

| Target Platform | Runner / Host Environment | Current Baseline Status (`a2143550935d`) | Pending Native Runtime Gates & Defects Requiring Native Kernel | Exact Native Verification Command |
| :--- | :--- | :--- | :--- | :--- |
| **Windows 10 / 11 (`x86_64`)** | Local workstation (`Windows 10.0.26200`, `rustc 1.100.0`, `node v26.13.1`, PowerShell `5.1`) | **Baseline Executed (`53/55` active; `2` foreign-OS gates skipped)** — `42` `PASS`, `11` `FAIL_DEFECT_REPRODUCED`, `0` `FAIL_UNEXPECTED` | Ready to verify `LC-07`, `LC-08`, `LC-09` (Win32 `rstrtmgr.dll`), `PM-04`, `PM-08`, `DS-08`, `CT-08`, `CT-09`, `CT-10` (`dim-host.ps1`) immediately when `<INTEGRATION_SHA>` arrives. | `cargo run --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner -- --mode release-regression --suite all --cycles 100 --report-out plans/reports/reliability/post-fix-windows-execution-evidence.json` |
| **Ubuntu 24.04 LTS (`x86_64`)** | Linux CI / VM runner (`ubuntu-24.04`, `bubblewrap` installed) | **Native Runtime Pending (`BLOCKED_NO_LINUX_HOST_IN_SESSION`)** — `XP-LNX-04` honestly marked `SKIPPED_HOST_PLATFORM`; `XP-LNX-02` checked statically (`STATIC_CONFIRMED_NON_HOST`) | Must execute live `/proc` inspection, real dangling `SingletonLock` symlink recovery (`XP-LNX-02` / `DEFECT-01`), `.runtime-pid` + `--user-id` launch parity (`DEFECT-02`/`03`), `LC-06` `.focus-signal` (`DEFECT-05`), recursive child `SIGTERM`->`SIGKILL` (`LC-09`, `REL-07` / `DEFECT-06`), and `bwrap` / AppArmor userns (`XP-LNX-04`). | `bash tests/cross-platform/scripts/run-ubuntu.sh --mode release-regression --require-native-os --report-out plans/reports/reliability/post-fix-ubuntu-execution-evidence.json` |
| **macOS 14+ (`arm64` / `x86_64`)** | Apple Silicon / Intel macOS runner (`macos-14`) | **Native Runtime Pending (`BLOCKED_NO_MACOS_HOST_IN_SESSION`)** — `XP-MAC-04` honestly marked `SKIPPED_HOST_PLATFORM`; `XP-MAC-03` checked statically (`STATIC_CONFIRMED_NON_HOST`) | Must execute live Launch Services (`open -n --env CFFIXED_USER_HOME`), APFS `0700` perms (`XP-MAC-04`), real dangling `SingletonLock` symlink + 2-profile `NodeFixture` launch (`XP-MAC-03` / `DEFECT-01`, `DEFECT-02`), `LC-06` focus (`DEFECT-05`), and `SIGTERM`->`SIGKILL` hang escalation + child tree cleanup (`LC-09`, `REL-07` / `DEFECT-06`). | `bash tests/cross-platform/scripts/run-macos.sh --mode release-regression --require-native-os --report-out plans/reports/reliability/post-fix-macos-execution-evidence.json` |

### 4.2 Non-Negotiable Platform & Product Honesty Guardrails

1. **Zero Cross-OS Extrapolation:** Passing `--mode release-regression` on Windows 11 clears the Windows gate only. If `--require-native-os` is passed on Windows for `XP-MAC-04`, `XP-LNX-04`, `XP-MAC-03`, or `XP-LNX-02`, `xf-test-runner` exits with code **`3` (`RELEASE_GATE_INCOMPLETE_NATIVE_OS`)** so CI pipelines cannot accidentally mark macOS or Ubuntu verified from a Windows runner.
2. **Synthetic Fixture (`xf-desktop-fixture` / `xf-fixture.mjs`) != Official VNG Zalo Desktop:**
   - Automated repository suites prove adapter, store, backup, process-tree, and isolation mechanics against deterministic synthetic fixtures without user credentials.
   - **Windows Official Zalo (`Zalo.exe`):** Remains `PARTIALLY_SUPPORTED` even after all `P0`–`P2` defects are `VERIFIED` because concurrent voice/video calls remain architecturally blocked by global named pipes (`\\.\pipe\PipeZCallSend` / `PipeZCallRecv`) and toast notifications share `AppUserModelId("com.vng.zalo")`.
   - **macOS Official Zalo (`Zalo.app`):** Remains `NOT_TESTED` until both native `run-macos.sh --mode release-regression --require-native-os` passes **and** an authorized human operator completes [`live-zalo-authorized-checklist.md`](./live-zalo-authorized-checklist.md) (`L-MAC-01`..`L-MAC-04`) on physical Apple hardware.
   - **Ubuntu / Linux Official Zalo:** Permanently **`UNSUPPORTED`** because VNG distributes no official Linux Zalo Desktop client; Linux support applies strictly to synthetic fixtures and user-supplied local `.AppImage` binaries (`EXPERIMENTAL`).
