# macOS Multi-Instance Test & Verification Evidence Report

**Date (UTC):** `2026-11-17`  
**Worktree:** `C:/Users/manhquy/Downloads/zalo-macos-worktree`  
**Branch:** `feat/macos-multi-instance` (branched from `a5feb4a520798064c1508c26fe4c4ac17a3a03cf`)  
**Host Environment:** Windows x64 (`rustc 1.100.0`, `cargo 1.100.0`, `clippy 0.1.100`, `node v26.13.1`, installed cross-targets `aarch64-apple-darwin` & `x86_64-apple-darwin`)  
**Live Official `Zalo.app` Execution Status:** **`LIVE_ZALO_NOT_TESTED`** (no physical Mac GUI attached; zero false claims of live `Zalo.app` runtime execution)

---

## 1. Compiler, Linter & Cross-Target Verification Summary

| Check Command | Target / Scope | Exit Code | Outcome |
| :--- | :--- | :---: | :--- |
| `cargo fmt --all -- --check` | Workspace formatting | `0` | **PASS** (zero diffs) |
| `cargo check --workspace --all-targets --locked` | All 7 workspace crates + tests + examples | `0` | **PASS** (0 errors, 0 warnings) |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | All 7 workspace crates + tests + examples | `0` | **PASS** (0 warnings) |
| `cargo test --workspace --all-targets --locked` | `38` Rust unit & integration tests across workspace | `0` | **PASS (`38/38` passed)** |
| `cargo check -p dim-platform-macos --target aarch64-apple-darwin --locked` | Apple Silicon (`arm64` macOS) | `0` | **PASS** (`#[cfg(target_os = "macos")]` & `#[cfg(unix)]` verified) |
| `cargo clippy -p dim-platform-macos --no-deps --target aarch64-apple-darwin --locked -- -D warnings` | Apple Silicon (`arm64` macOS) | `0` | **PASS** (0 warnings in `dim-platform-macos`)* |
| `cargo check -p dim-platform-macos --target x86_64-apple-darwin --locked` | Intel Mac (`x86_64` macOS) | `0` | **PASS** (`#[cfg(target_os = "macos")]` & `#[cfg(unix)]` verified) |
| `cargo clippy -p dim-platform-macos --no-deps --target x86_64-apple-darwin --locked -- -D warnings` | Intel Mac (`x86_64` macOS) | `0` | **PASS** (0 warnings in `dim-platform-macos`)* |
| `cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml --locked` | Tauri 2 desktop shell integration | `0` | **PASS** (backwards-compatible exports verified) |
| `node tests/platform-macos/run-macos-fixture-tests.mjs` | 13 live process multi-instance & safety scenarios | `0` | **PASS (`13/13` passed)** |

*\*Note on `--no-deps` for Darwin cross-target Clippy:* Untouched shared crate `crates/profile-store/src/store.rs:337` (which is outside the allowed `feat/macos-multi-instance` write scope) contains a pre-existing `clippy::needless_bool` when `#[cfg(windows)]` is compiled out on non-Windows targets. Passing `--no-deps` lints `dim-platform-macos` strictly with `-D warnings` (`0` warnings on both `aarch64-apple-darwin` and `x86_64-apple-darwin`); the 1-line cleanup for `dim-profile-store` is documented in [`docs/platform/macos/integration-handoff.md`](../../../docs/platform/macos/integration-handoff.md).

---

## 2. Rust Unit & Integration Test Breakdown (`crates/platform-macos`)

- **Unit tests (`crates/platform-macos/src/lib.rs`, `3/3 PASS`):**
  1. `tests::singleton_lock_parser_handles_hyphenated_hostnames`
  2. `tests::open_command_builder_sets_cffixed_user_home`
  3. `tests::macos_layout_and_plist_extraction_and_not_tested_status`
- **Integration & scenario tests (`crates/platform-macos/tests/macos_scenarios.rs`, `9/9 PASS`):**
  1. `macho_inspector_parses_arm64_x86_64_universal2_and_fat64`
  2. `bundle_discovery_bplist00_plutil_spotlight_and_arch_compatibility`
  3. `codesign_enforces_unverified_binary_on_unsigned_adhoc_tampered_and_wrong_publisher`
  4. `ps_parser_handles_spaces_in_app_paths_and_filters_zombies`
  5. `scenario_7_concurrent_duplicate_launch_guard_blocks_parallel_race`
  6. `scenario_8_and_9_dangling_singleton_lock_recycled_pid_and_primary_protection`
  7. `scenario_11_12_13_unicode_nfc_nfd_sun_len_path_max_and_permissions`
  8. `window_inspection_jxa_parser_and_focus_activation_parameterized_osascript`
  9. `scenarios_1_to_6_and_10_live_multi_session_isolation_relaunch_close_and_symlink_safety`

---

## 3. End-to-End 13-Scenario Fixture Suite (`tests/platform-macos/run-macos-fixture-tests.mjs`)

All 13 scenarios execute live detached Node.js fixture processes (`fixture-macos-app.mjs`) driven through the compiled Rust `dim_macos_cli` binary (`crates/platform-macos/examples/dim_macos_cli.rs`) inside an auto-cleaned OS temporary directory:

1. **Scenario 1 (`single_session` — `PASS`):** Single isolated session launch, `.window-state.json` window ID/title detection, and `focus` incrementing `focus_count`.
2. **Scenario 2 (`two_sessions` — `PASS`):** 2 concurrent isolated sessions with distinct PIDs and window titles.
3. **Scenario 3 (`three_sessions` — `PASS`):** 3 concurrent isolated sessions with 3 distinct PIDs, 3 distinct `detected_zalo_user_id`s, non-zero `working_set_bytes`, and non-zero `disk_usage_bytes`.
4. **Scenario 4 (`isolated_data` — `PASS`):** Verified isolated `ZaloData`, `Library/Preferences/com.vng.zalo.plist`, `Library/Caches/com.vng.zalo/cache-meta.json`, `.macos-profile-meta.json`, and `DUPLICATE_ZALO_USER_ID` collision refusal.
5. **Scenario 5 (`relaunch` — `PASS`):** Idempotent running launch (same PID preserved), clean stop + relaunch (`launch_count == 2`, SQLite `-wal` flushed), and offline SHA-256 backup + non-destructive restore to a new profile.
6. **Scenario 6 (`individual_close` — `PASS`):** Stopping Profile 2 individually terminates only Profile 2 while Profiles 1 and 3 continue running on their original PIDs.
7. **Scenario 7 (`duplicate_operation_guard` — `PASS`):** Concurrent cross-process `launch` contention on `<profile_dir>/tmp/.op-lock` fails closed with `PROCESS_ERROR`.
8. **Scenario 8 (`crash_recovery` — `PASS`):** `--crash` leaves `SingletonLock`, `SingletonSocket`, `SingletonCookie`, and `lockfile` behind (`running == false`, `stale_lock_detected == true`); next `launch` recovers cleanly (`crash_recovered == true`, `running == true`).
9. **Scenario 9 (`ownership_and_primary_protection` — `PASS`):** Foreign recycled PID (`Safari.app` PID `88421`) is rejected as unowned (`stale_lock_detected == true`, `0` processes killed); primary `Zalo.app` PID (`77100`) is protected (`PROTECTED_PRIMARY_PROCESS`); primary `ZaloData` path overlap fails with `PROTECTED_PRIMARY_PATH`.
10. **Scenario 10 (`safe_symlink_deletion` — `PASS`):** Enforces `CONFIRM_REQUIRED` without `--confirm`, unlinks external `Downloads` symlink/junction before profile removal (`unlinked_symlinks == 1`), and leaves external host files untouched.
11. **Scenario 11 (`unicode_nfc_nfd` — `PASS`):** Verifies NFD decomposed Vietnamese primary path guard (`PROTECTED_PRIMARY_PATH`) and creates/launches/stops/deletes profiles under NFD Vietnamese + Japanese CJK + Korean Hangul directory trees.
12. **Scenario 12 (`long_paths_sun_len_path_max` — `PASS`):** Long profile path triggers `short_tmpdir_fallback == true` (`socket_path_bytes == 72 <= 103` under `--enforce-sun-len`); `>= 1024`-byte path is rejected with `PATH_TRAVERSAL`.
13. **Scenario 13 (`permission_failures` — `PASS`):** Verifies `profiles_acl_safe == true` under `0700`, `false` under `0777`, and `IO_ERROR` failure when attempting layout creation inside read-only `0500` storage.

---

## 4. Security & Scope Compliance Audit

- **Zero Forbidden File Edits:** `Cargo.lock`, root `Cargo.toml`, `crates/platform-macos/Cargo.toml`, `crates/core`, `crates/profile-store`, `crates/process-manager`, `crates/platform-windows`, `crates/platform-linux`, `apps/desktop`, `.github`, and `scripts` are **100% untouched**.
- **Zero Production Panics:** Zero `.unwrap()` or `.expect()` calls exist anywhere in `crates/platform-macos/src/*.rs` (`clippy::unwrap_used` & `clippy::expect_used` verified with `-D warnings`).
- **Zero Command Injection:** Every external command (`open`, `codesign`, `spctl`, `plutil`, `mdfind`, `xattr`, `ps`, `osascript`) uses `Command::new(program).args(discrete_args)` with parameterized AppleScript/JXA numeric `argv` — never `sh -c` or interpolated strings.
- **Zero Symlink-Following Deletions:** All cleanup and unlink operations check `fs::symlink_metadata` (`lstat`) before unlinking.
