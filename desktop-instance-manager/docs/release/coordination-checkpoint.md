# Release Coordination Checkpoint — Multi-Zalo Desktop

- **Checkpoint ID:** `coord-ckpt-20261118T0850+0700`
- **Recorded At:** `2026-11-18T08:50:00+07:00`
- **Previous Checkpoint:** `phase0-20261118T0733+0700` (`2026-11-18T07:33:00+07:00`)
- **Session Verdict:** **`WAITING_FOR_AGENT_HANDOFF`**
- **Integration Worktree State:** **`INTEGRATION_WORKTREE_ACTIVE`**

---

## 1. Branch & Worktree Snapshot (Actual HEADs Verified via `git --no-optional-locks`)

| Workstream | Worktree Directory | Branch | Reported Ref SHA | Actual `HEAD` SHA (`40`-char) | Changed Since `07:33`? | `git status --porcelain=v1` | Pushed to `origin`? | Agent Active? |
| :--- | :--- | :--- | :---: | :--- | :---: | :--- | :---: | :---: |
| **Backend (`main`)** | `<WORKTREE_ROOT>/zalo` | `main` | `a5feb4a` | `a5feb4a520798064c1508c26fe4c4ac17a3a03cf` | No | Clean *(except uncommitted `docs/release/*.md` created by Release Captain)* | No (`ahead 1` over `origin/main` `8cf4703`) | No |
| **Account Dock UI** | `<WORKTREE_ROOT>/zalo-ui-worktree` | `feat/ui-vi-en-quiet-workspace` | `0f87302` | `0f873024f4ee16810b9d35cf206803f793fc55cb` | No | Clean (`0` files) | **Yes** (`origin/feat/ui-vi-en-quiet-workspace`) | No |
| **macOS A2** | `<WORKTREE_ROOT>/zalo-macos-worktree` | `feat/macos-multi-instance` | `fb2abcf` / `8628224` | `f2b8f9f182316224179658634f3c526ed9f5ff26` | No | Clean (`0` files) | No | No *(merged into `integration` @ `8a7861d`)* |
| **Ubuntu B2** | `<WORKTREE_ROOT>/zalo-ubuntu-worktree` | `feat/ubuntu-multi-instance` | `9a10a9f` / `62dc5d1` | `62dc5d1555799a902d9f8aa8a1788a47be8ac578` | No | Clean (`0` files) | No | No *(merged into `integration` @ `47a2807`)* |
| **Reliability C2** | `<WORKTREE_ROOT>/zalo-cross-platform-tests` | `test/cross-platform-reliability` | `5e802e0` | `5e802e08119efce56d7df157538122e16ed64801` | No | Clean (`0` files) | No | No *(merged into `integration` @ `5fa798c`)* |
| **Compatibility D2** | `<WORKTREE_ROOT>/zalo-compatibility-research` | `research/desktop-compatibility` | `0e5d44c` | `0e5d44c0148aa7c5f436e7274597453d5c5501c3` | No | Clean (`0` files) | No | No *(merged into `integration` @ `155efc3`)* |
| **Build E2** | `<WORKTREE_ROOT>/zalo-cross-platform-build` | `build/cross-platform-distribution` | `6c4150e` | `6c4150ecdb106f07aceca96098aaf3ab0d9a1326` | No | Clean (`0` files) | No | No *(merged into `integration` @ `ec46209`)* |
| **Central Integration** | `<WORKTREE_ROOT>/zalo-integration` | `integration/reliability-hardening` | `f0b5733` *(code `58dbd61`)* | **`004ab7cd1b31f8dd9c0e1d9bdd659d8f6920df5d`** | **Yes (`+6` first-parent commits)** | **`DIRTY` (`33` `M` + `2` `??`)** | No | **YES (`INTEGRATION_WORKTREE_ACTIVE`)** |

---

## 2. P0 Forensics — Worktree Ownership Conflict on `tests/cross-platform/src/harness.rs`

All 7 read-only forensic checks required under Mission Update §2 were executed with `git --no-optional-locks` (zero index locks, zero mutations on `<WORKTREE_ROOT>/zalo-integration`):

1. **Actual `HEAD` of `integration/reliability-hardening`:**
   - Advanced from `f0b573371f844bc8221b8ad60e3d547241bce24b` (`2026-11-18 04:39:20 +0700`) to **`004ab7cd1b31f8dd9c0e1d9bdd659d8f6920df5d`** (`2026-11-18 08:17:59 +0700`).
2. **Exact `git status --porcelain=v1` on `<WORKTREE_ROOT>/zalo-integration` (at `08:49:12 +0700`):**
   - `desktop-instance-manager/tests/cross-platform/src/harness.rs` is **no longer dirty** — its uncommitted edit from `07:23:41 +0700` was committed cleanly in `f3310857d3885a22908315156d3c7741a9c9cb00` (`07:45:13 +0700`).
   - However, the same active integration agent is **still mid-wave** with **33 modified (`M`) files and 2 untracked (`??`) files**:
     - Product & test files (`9` `M` + `2` `??`): `apps/desktop/src-tauri/src/commands.rs`, `apps/desktop/ui/app.js`, `apps/desktop/ui/i18n/{vi-VN,en-US}.json`, `apps/desktop/ui/tests/ui-contracts.test.mjs`, `crates/platform-linux/src/{layout,procfs}.rs`, `fixtures/cross-platform/xf-fixture.mjs`, `tests/cross-platform/src/suites/reliability.rs`, `?? tests/run-tauri-native-e2e.mjs`, `?? evidence/tauri-native-e2e-result.json`.
     - Documentation & evidence path-redaction sweep (`24` `M`): `desktop-instance-manager/docs/**`, `desktop-instance-manager/plans/reports/**`, and `multi-zalo-desktop/{docs,evidence}/**` (modified `08:47:36–08:48:39 +0700`).
3. **Exact Diff of `tests/cross-platform/src/harness.rs` (Committed in `f331085`):**
   - Added `+12` lines in `collect_pids_from_profile_artifacts(profile_dir: &Path, out: &mut Vec<u32>)` reading `<profile_dir>/.runtime-lease.json -> $.pid` (`u32`) and registering it via `push_pid(pid)`.
   - Purpose: Ensures C2 sandbox teardown and orphan detection track pre-lock startup PIDs persisted by `crates/core/src/lease.rs` (`DEFECT-03` / `LC-07`).
4. **Active Agent Verification (`INTEGRATION_WORKTREE_ACTIVE`):**
   - Host process inspection confirmed active external agent processes (`codex.exe` PIDs `25240` and `27848`, CPU advancing from `8.55s / 25.45s` at `07:31` to `9.11s / 27.42s` at `08:46`), fresh Cargo compilation under `apps/desktop/src-tauri/target` (`08:31:08 +0700`), live execution of `tests/run-tauri-native-e2e.mjs` (`08:45:40–08:46:45 +0700`), and active documentation redaction writes at `08:48:39 +0700`.
5. **Timeline & Branch Ownership Correlation (`04:39` -> `08:49 +0700`):**
   - **Owner:** Central Integration / Post-Audit Reliability & E2E Hardening session operating exclusively in `<WORKTREE_ROOT>/zalo-integration`:
     - `04:39:20` (`f0b5733`): Recorded Cycle-2 Windows D2/C2 verification ledger for `58dbd61`.
     - `07:23:41–07:45:13` (`f331085`): Edited `harness.rs` + added `VerifiedDeletionState` trait contract and subprocess lock race suites (`p0_coordinator_concurrency.rs`, `p0_store_reliability.rs`).
     - `08:11:15` (`e602741`): Relocated per-profile locks to `<data_dir>/locks`, closed launch admission TOCTOU, routed Tauri `get_status` through `InstanceCoordinator`, and split `is_symlink_or_reparse` (`#[cfg(windows)]` / `#[cfg(not(windows))]`) in `store.rs`.
     - `08:11:45` (`8a7861d`) & `08:13:31` (`0aee2db`): Merged `feat/macos-multi-instance` (`f2b8f9f`) and aligned `VerifiedDeletionState` + `Starting` lifecycle state in `crates/platform-macos`.
     - `08:13:32` (`47a2807`) & `08:17:59` (`004ab7c`): Merged `feat/ubuntu-multi-instance` (`62dc5d1`), added deterministic FNV-1a `--user-id` (`DEFECT-02`), added `/proc/<pid>/fd` open-DB guard before `SIGKILL` (`F-UBU-03`), and aligned `XP-LNX-03` (`--new-session`).
     - `08:20:20–08:48:39+` *(uncommitted, in flight)*: Added `recoveryRequired` UI state, `DIM_NODE_FIXTURE_SCRIPT` E2E target hook in `commands.rs`, `tests/run-tauri-native-e2e.mjs` (`12/12 PASS` on live `dim-desktop.exe` WebView2), `REL-01` cycle barrier refinement, and repo-wide PII path redaction.
6. **Task Classification:**
   - All commits `f331085..004ab7c` and current uncommitted edits belong to a single coherent **Post-`f0b5733` Security Remediation + POSIX (`A2`/`B2`) Branch Integration + Native Tauri WebView2 E2E + Pre-Push PII Redaction** wave by the Central Integration owner.
7. **Impact on Previously Verified Code SHA (`58dbd61b9454bab96493819bc1517032bbf5b968`):**
   - Git commit `58dbd61b9454bab96493819bc1517032bbf5b968` and its evidence artifact `plans/reports/reliability/post-fix-integration-windows-evidence.json` (`SHA-256: 5407666a6856c5d03c5430f65ff1f8cde8d1b05c052f8408d8e4d56dd6a49c27`) remain **100% intact and authoritative for `58dbd61`**.
   - **However, `58dbd61` is no longer the candidate release code SHA:** commits `f331085..004ab7c` and the active uncommitted wave modify `crates/core`, `crates/profile-store`, `crates/process-manager`, `crates/platform-macos`, `crates/platform-linux`, `apps/desktop/src-tauri`, `apps/desktop/ui`, and `scripts/dim-host.ps1`. Per §5 (*"C2 Reliability: chỉ chạy độc lập đối với revision mới có thay đổi product code"*), once Central Integration commits and hands off its final clean SHA, C2 `release-regression` must be verified against that **new final SHA** before tagging or packaging.

---

## 3. Accepted Historical Evidence (Reused Without Redundant Rework)

| Baseline / Evidence Artifact | Verified Code SHA | Key Proven Metrics | Reuse Policy |
| :--- | :---: | :--- | :--- |
| `docs/reviews/integration-defect-ledger.md` & `plans/reports/reliability/post-fix-integration-windows-evidence.json` | `58dbd61b9454` | Windows P0 `4/4 PASS` (`LC-07`, `LC-08`, `PM-08`, `CT-10`), Windows/Core defects `10/10 PASS`, parallel Cargo suites `7/7 PASS` (`DEFECT-C2-RM-01` closed), `REL-01` `10/10`, `REL-02` `100/100` soak, full suite `51/55 PASS` (`0` unexpected fails), primary Zalo `11` processes & `config.json` SHA-256 unchanged | Accepted as closure proof for Windows/Core P0–P2 design; run one final delta C2 gate on the post-`004ab7c` handoff SHA. |
| `docs/design/account-dock-final-acceptance.md` | `0f873024f4ee` | `20/20` unit/i18n/contract tests `PASS`, `visual-cdp.mjs` `16` matrix combos + Journeys A–E `PASS`, `271/271` `vi-VN`/`en-US` parity | Accepted; supplemented by live `dim-desktop.exe` WebView2 E2E (`12/12 PASS` in `tauri-native-e2e-result.json`, awaiting commit). |
| `docs/platform/macos/native-runtime-hardening-report.md` | `f2b8f9f18231` | `40/40` Rust + `15/15` CLI fixture scenarios `PASS` on host, `0` warnings for `aarch64-apple-darwin` & `x86_64-apple-darwin`; `XP-MAC-01`, WAL-safe `SIGKILL`, and `lsof -U` resolved | Merged at `8a7861d` + `0aee2db`; requires native Darwin CI (`macos-15` / `macos-15-intel`) before advancing `NOT_TESTED`. |
| `docs/platform/ubuntu/runtime-hardening-and-native-validation.md` | `62dc5d155579` + `004ab7c` | `45/45` Rust + `29/29` Node scenarios `PASS`; ELF64 static parser (`0` `ldd`); `004ab7c` closed `DEFECT-02` FNV-1a `--user-id`, `F-UBU-03` `/proc/<pid>/fd` WAL guard, and `XP-LNX-03` | Merged at `47a2807` + `004ab7c`; requires native `ubuntu-24.04` CI run; official VNG Zalo Linux remains permanently `UNSUPPORTED`. |
| `docs/packaging/ci-execution-and-artifact-validation.md` | `6c4150ecdb10` + `8aa7a62` | `validate-workflow.mjs` `6/6 PASS`, `selftest.mjs` `16/16 PASS`, `cargo-deny 0.21.0` `0` advisories across `439` crates (`3` lockfiles) | Accepted; awaits `gh auth refresh -h github.com -s workflow` and default-branch PR merge to execute live CI. |

---

## 4. Delta-Only Resume Protocol (Next Turn Entrypoint)

When the active session in `<WORKTREE_ROOT>/zalo-integration` finishes and hands off a clean commit:

1. **Verify Clean Handoff State (Read-Only):**
   - Run `git --no-optional-locks -C <WORKTREE_ROOT>/zalo-integration status --porcelain=v1` -> must output empty (`0` dirty/untracked files) and `0` active `cargo`/`rustc`/`xf-test-runner` processes.
   - Record `<FINAL_INTEGRATION_SHA> = git rev-parse integration/reliability-hardening` and inspect only `git log 004ab7c..<FINAL_INTEGRATION_SHA> --stat`.
2. **Independent Post-Handoff Verification on `<FINAL_INTEGRATION_SHA>` (Because Product Code Changed After `58dbd61`):**
   - Run `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace --locked`, `cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml --locked`.
   - Run full C2 `xf-test-runner --mode release-regression --suite all --cycles 100` (expecting `53/55 PASS` + `2 SKIPPED_HOST_PLATFORM` `XP-MAC-04`/`XP-LNX-04` and **`0` `FAIL_DEFECT_REPRODUCED`** now that `f2b8f9f` + `62dc5d1` + `004ab7c` are merged!).
   - Verify `node tests/run-tauri-native-e2e.mjs`, `node tests/run-fixture-tests.mjs`, `node tests/run-contract-parity-tests.mjs`, and primary Zalo SHA-256 invariance.
3. **GitHub Sync, CI Execution & Packaging (Phases 4–6):**
   - Confirm `gh auth status` includes `workflow` scope, run secret/PII diff scan against `origin/main` (`8cf4703`), push `main` (`a5feb4a`) + `integration/reliability-hardening`, open reviewed PR -> `main`, and dispatch Wave 1 CI (`ubuntu-24.04` + `windows-2022` MSVC) to produce and locally smoke-test the Windows x64 NSIS/MSI installer (`v0.1.0-beta.1`, classified `INTERNAL_TEST_BUILD` until human L3 QR passes).
4. **Deferred Worktree Cleanup (Phase 7):**
   - Generate per-worktree cleanup table and await explicit owner approval before running any `git worktree remove`.
