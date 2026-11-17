# macOS Multi-Instance Test & Verification Evidence Report (Stage 2 Hardened)

**Date (UTC):** `2026-11-17` (`2026-11-18+07:00`)  
**Worktree:** `C:/Users/manhquy/Downloads/zalo-macos-worktree`  
**Branch:** `feat/macos-multi-instance` (baseline `main` = `a5feb4a520798064c1508c26fe4c4ac17a3a03cf`)  
**Host Environment:** Windows x64 (`rustc 1.100.0`, `cargo 1.100.0`, `clippy 0.1.100`, `node v26.13.1`, installed cross-targets `aarch64-apple-darwin` & `x86_64-apple-darwin`)  
**6-State Verification Summary:**
- **Compiler / Lint / Cross-Targets:** **`STATIC_VERIFIED`**
- **Automated Rust + Live Node Fixture Suites:** **`WINDOWS_HOST_FIXTURE_TESTED`** (`39/39` Rust tests + `15/15` E2E CLI scenarios)
- **Native Darwin Kernel Runtime (`macos-15`, `macos-15-intel`):** **`NOT_TESTED`** (`0` false claims of `DARWIN_NATIVE_TESTED`)
- **Live Official `/Applications/Zalo.app` Runtime:** **`NOT_TESTED`** (`LIVE_ZALO_NOT_TESTED`, `0` false claims of `LIVE_ZALO_TESTED`)

---

## 1. Compiler, Linter & Cross-Target Verification Matrix

| Check Command | Target / Scope | Exit Code | Verification State | Outcome |
| :--- | :--- | :---: | :---: | :--- |
| `cargo fmt --all -- --check` | Workspace formatting | `0` | `STATIC_VERIFIED` | **PASS** (zero diffs) |
| `cargo check --workspace --all-targets --locked` | All 7 workspace crates + tests + examples | `0` | `STATIC_VERIFIED` | **PASS** (0 errors, 0 warnings) |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | All 7 workspace crates + tests + examples | `0` | `STATIC_VERIFIED` | **PASS** (0 warnings) |
| `cargo test --workspace --all-targets --locked` | `39` Rust unit & integration tests across workspace | `0` | `WINDOWS_HOST_FIXTURE_TESTED` | **PASS (`39/39` passed)** |
| `cargo check -p dim-platform-macos --target aarch64-apple-darwin --locked` | Apple Silicon (`arm64` macOS) | `0` | `STATIC_VERIFIED` | **PASS** (`#[cfg(target_os = "macos")]` & `#[cfg(unix)]` verified) |
| `cargo clippy -p dim-platform-macos --no-deps --target aarch64-apple-darwin --locked -- -D warnings` | Apple Silicon (`arm64` macOS) | `0` | `STATIC_VERIFIED` | **PASS** (0 warnings in `dim-platform-macos`)* |
| `cargo check -p dim-platform-macos --target x86_64-apple-darwin --locked` | Intel Mac (`x86_64` macOS) | `0` | `STATIC_VERIFIED` | **PASS** (`#[cfg(target_os = "macos")]` & `#[cfg(unix)]` verified) |
| `cargo clippy -p dim-platform-macos --no-deps --target x86_64-apple-darwin --locked -- -D warnings` | Intel Mac (`x86_64` macOS) | `0` | `STATIC_VERIFIED` | **PASS** (0 warnings in `dim-platform-macos`)* |
| `cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml --locked` | Tauri 2 desktop shell integration | `0` | `STATIC_VERIFIED` | **PASS** (backwards-compatible exports verified) |
| `node tests/platform-macos/run-macos-fixture-tests.mjs` | `15` live detached-process E2E scenarios | `0` | `WINDOWS_HOST_FIXTURE_TESTED` | **PASS (`15/15` passed)** |

*\*Note on `--no-deps` for Darwin cross-target Clippy:* Untouched shared crate `crates/profile-store/src/store.rs:337` (outside the allowed `feat/macos-multi-instance` write scope) triggers `clippy::needless_bool` on non-Windows targets; handed off under `IR-CORE-01` in [`docs/platform/macos/integration-handoff.md`](../../../docs/platform/macos/integration-handoff.md).

---

## 2. Rust Unit & Integration Test Breakdown (`crates/platform-macos`, `13/13 PASS`)

- **Unit tests (`crates/platform-macos/src/lib.rs`, `3/3 PASS`):**
  1. `tests::singleton_lock_parser_handles_hyphenated_hostnames`
  2. `tests::open_command_builder_sets_cffixed_user_home`
  3. `tests::macos_layout_defaults_isolated_and_not_tested_status`
- **Stage-2 hardened integration & safety regression suites (`crates/platform-macos/tests/macos_scenarios.rs`, `10/10 PASS`):**
  1. `macho_inspector_parses_arm64_x86_64_universal2_and_fat64` — thin `MH_MAGIC_64`, `FAT_MAGIC`, `FAT_MAGIC_64`, corrupt header rejection.
  2. `p0_1_all_seven_bundle_and_codesign_identity_failure_modes_plus_tier5_verified_publisher` — unsigned/adhoc, tampered nested `ZaloHelper.app`, wrong `TeamIdentifier` (even with `"VNG"` substring), missing bundle, malformed XML & failed `bplist00` `plutil`, `arm64`-on-`x86_64`, unpinned default policy, positive Tier-5 `-R=...` `VerifiedPublisher`, and `XP-MAC-01`.
  3. `p0_2_twelve_surface_data_isolation_ct03_and_external_db_path_guard` — 3 live profiles verified across all 12 surfaces (`verify_12_surface_runtime_isolation`), `CT-03` (`unlink_profile_junctions == Ok(0)`), close/relaunch preservation, untouched primary `ZaloData`, and `F-CORE-01` `config.json -> db_path.location` guard.
  4. `p0_3_all_eight_process_ownership_pid_reuse_and_primary_protection_cases` — `ps -o lstart=` 5-token parser, same-UID/same-binary `Zalo.app` PID reuse rejection, unmanaged `Zalo.app` protection, primary `Zalo.app` protection, manager restart re-attachment, stale `.runtime-pid`, helper + `ppid == 1` orphan tree collection, and isolated shutdown.
  5. `p0_4_defects_01_to_06_xp_mac_03_static_and_runtime_contracts` — `XP-MAC-03` static source assertions, 8-thread concurrent `ensure_profile_layout` (`LC-08`), `--slow-startup-pre-lock-ms=900` immediate ownership (`LC-07`), and empty-`extra_args` distinct `zalo_user_id` derivation.
  6. `p1_1_sun_len_short_tmpdir_symlink_squatting_and_foreign_slot_hardening` — `/tmp/dz-501/<id>` (`<= 49` bytes, `66`-byte socket `<= 103`), symlink-squatting rejection (`PATH_TRAVERSAL`), `0o777` rejection, traversal/non-ASCII ID rejection, and cleanup preserving `/tmp/socketzalorecv2021`.
  7. `p1_2_all_seven_graceful_shutdown_timeout_hung_and_orphan_helper_scenarios` — `--delayed-exit-ms=160` exiting via `graceful_quit_signal`, `--hang-on-quit` failing closed with `PROCESS_ERROR` while keeping `.runtime-launch.json`, crashed/stale `Ok(0)`, and manager-restart `--orphan-helper` (`ppid == 1`) reaping.
  8. `p1_3_all_four_macos_14_window_activation_and_zero_tcc_cases` — zero `System Events` assertions, cooperative `activated=false` -> `PROCESS_ERROR`, `activated=true` -> `Ok`, unowned live PID rejected before `osascript`, and dead PID -> `NOT_RUNNING`.
  9. `scenario_7_concurrent_duplicate_launch_guard_blocks_parallel_race` — in-process `Barrier` contention returning `PROCESS_ERROR`.
  10. `scenario_10_11_12_13_symlink_unlink_unicode_nfc_nfd_path_max_and_permissions` — Vietnamese/Hangul NFC/NFD normalization, `PATH_MAX` (`1024B`), `0700`/`0777`/`0500` permission checks, and safe junction/symlink unlinking before deletion.

---

## 3. End-to-End 15-Scenario CLI Fixture Suite (`tests/platform-macos/run-macos-fixture-tests.mjs`, `15/15 PASS`)

1. **Scenario 1 (`single_session` — `PASS`):** Single isolated session launch, window state detection, and `focus` (`focus_count == 1`).
2. **Scenario 2 (`two_sessions` — `PASS`):** 2 concurrent isolated sessions with distinct PIDs and window titles.
3. **Scenario 3 (`three_sessions` — `PASS`):** 3 concurrent isolated sessions with 3 distinct PIDs, 3 distinct `zalo_user_id`s, non-zero RSS & disk bytes.
4. **Scenario 4 (`isolated_data_12_surfaces` — `PASS`):** `verify-isolation` confirms all `12` surfaces, `3` distinct `TMPDIR`s, `3` distinct `Caches`, non-symlinked `Downloads`, untouched primary `ZaloData`, and `DUPLICATE_ZALO_USER_ID` block.
5. **Scenario 5 (`relaunch` — `PASS`):** Idempotent running launch (same PID), clean stop + relaunch (`launch_count == 2`), offline SHA-256 backup + restore.
6. **Scenario 6 (`individual_close` — `PASS`):** Stopping Profile 2 preserves Profiles 1 & 3 on their original PIDs.
7. **Scenario 7 (`duplicate_operation_guard` — `PASS`):** Cross-process `.op-lock` contention returns `PROCESS_ERROR`.
8. **Scenario 8 (`crash_recovery` — `PASS`):** `--crash` sets `stale_lock_detected == true`; subsequent `launch` recovers (`crash_recovered == true`).
9. **Scenario 9 (`ownership_and_primary_protection` — `PASS`):** Foreign `Safari` PID untouched; same-UID/same-binary `Zalo.app` PID reuse with mismatched `lstart` rejected (`same_binary_pid_reuse_rejected == true`); unowned `focus` rejected (`PROCESS_ERROR`); primary PID (`PROTECTED_PRIMARY_PROCESS`) and primary path (`PROTECTED_PRIMARY_PATH`) protected.
10. **Scenario 10 (`safe_symlink_deletion` — `PASS`):** Enforces `CONFIRM_REQUIRED`, unlinks external `Downloads/HostShared` link (`unlinked_symlinks == 1`), preserves host file.
11. **Scenario 11 (`unicode_nfc_nfd` — `PASS`):** Blocks NFD decomposed Vietnamese primary path (`PROTECTED_PRIMARY_PATH`) and runs NFD Vietnamese + CJK + Hangul profile cleanly.
12. **Scenario 12 (`long_paths_sun_len_path_max` — `PASS`):** Allocates `/tmp/dz-<uid>/<id>` (`socket_path_bytes == 66 <= 103`), cleans short slot upon `stop()`, rejects `> 1024B` path (`PATH_TRAVERSAL`).
13. **Scenario 13 (`permission_failures` — `PASS`):** `profiles_acl_safe: true -> false` on `0777`; `IO_ERROR` on read-only `0500` parent.
14. **Scenario 14 (`slow_prelock_and_auto_userid` — `PASS`):** `--slow-startup-pre-lock-ms 2000` returns `running == true` immediately (`LC-07`); zero-arg profiles derive distinct `84xxxxxxx` `zalo_user_id`s (`XP-MAC-03`).
15. **Scenario 15 (`graceful_delayed_hung_orphan_shutdown` — `PASS`):** `--delayed-exit-ms 160` exits via `graceful_quit_signal`; `--orphan-helper` reaps `ppid == 1` orphan PID; `--hang-on-quit` fails closed (`PROCESS_ERROR`) while preserving `.runtime-launch.json`.

---

## 4. Security & Scope Compliance Audit

- **Zero Out-of-Scope Modifications:** `Cargo.lock`, root `Cargo.toml`, `crates/platform-macos/Cargo.toml`, `crates/core`, `crates/profile-store`, `crates/process-manager`, `crates/platform-windows`, `crates/platform-linux`, `apps/desktop`, `.github`, `scripts`, `fixtures`, and all 6 sibling worktrees are **100% untouched**.
- **Zero Production `.unwrap()` / `.expect()`:** Verified across `crates/platform-macos/src/*.rs` with `clippy::unwrap_used` and `clippy::expect_used` under `-D warnings`.
- **Zero Command Injection or TCC Escalation:** All external binaries use discrete `Command::args` arrays with numeric `osascript` `argv` and zero `System Events` AppleEvents.
