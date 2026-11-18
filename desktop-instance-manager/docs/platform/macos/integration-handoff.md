# macOS Adapter Integration Handoff (`dim-platform-macos`)

**Target Consumers:** `dim-process-manager`, `apps/desktop/src-tauri`, `fixtures/test-desktop-app` (`dim-cli`), `tests/cross-platform`, and GitHub Actions CI  
**Scope Compliance:** Per strict branch isolation rules on `feat/macos-multi-instance`, **zero** files outside `crates/platform-macos/**`, `tests/platform-macos/**`, `docs/platform/macos/**`, and `plans/reports/macos/**` were modified, and `Cargo.lock` + `crates/platform-macos/Cargo.toml` remain 100% untouched.

---

## 1. Public Rust & Cross-Platform Test Contract

### 1.1. Preserved Backwards-Compatible Exports (`dim_platform_macos::*`)

`apps/desktop/src-tauri/src/commands.rs`, `fixtures/test-desktop-app/src/bin/dim-cli.rs`, and `tests/cross-platform` (`CT-03`, `XP-MAC-01`, `XP-MAC-03`) compile and pass unchanged against:
- `MacOsPlatformAdapter::new()` (`Default`, `Clone`, `Debug`, `Send`, `Sync`)
- Trait implementations on `MacOsPlatformAdapter`:
  - `dim_core::ApplicationLocator` (5-tier bundle + publisher identity verification; returns `SupportStatus::NotTested` honestly)
  - `dim_core::PlatformCapabilities` (`is_elevated()` -> `Some(geteuid() == 0)`, `check_profiles_acl_safe()` -> `Some((mode & 0o022) == 0 && uid_ok)`)
  - `dim_core::InstanceMonitor` (`inspect_primary`, `inspect_profile` reading `.runtime-pid` + `.runtime-launch.json` + `SingletonLock` `lstat` + `ps -o lstart=`)
  - `dim_core::InstanceLauncher` (`ensure_profile_layout` with isolated `0o700` `Downloads` & `Keychains` by default so `CT-03` `unlink_profile_junctions == Ok(0)` holds on both Windows and Darwin; idempotent `launch`; fail-closed `focus_or_restore`; 8-rule `stop`)
  - `dim_core::RecoveryManager` (`recover_stale_state` cleaning `ZaloData` lock remnants, `.runtime-pid`, `.runtime-launch.json`, and `/tmp/dz-<uid>/<id>` without following symlinks or touching `/tmp/socketzalo*2021`)
- Backwards-compatible functions & constants:
  - `read_singleton_or_json_pid`, `build_open_args`, `ensure_macos_profile_layout`, `ensure_macos_user_symlinks`, `extract_plist_short_version`, `inspect_call_sockets_at`, `macos_zalo_data_dir`, `parse_singleton_lock_target`, `MACOS_CALL_SOCKETS`, `MACOS_SYMLINK_DIRS`.

### 1.2. Stage-2 Hardening APIs (`dim_platform_macos::*`)

- **5-Tier Publisher Pinning (`P0-1`):**
  - `AppIdentityVerificationTier` (`BundlePresent`, `ValidMachO`, `ValidCodeSignature`, `GatekeeperAssessed`, `VerifiedPublisherIdentity`)
  - `PinnedPublisherPolicy::new(bundle_identifier, team_identifier)` & `MacOsPlatformAdapter::with_pinned_publisher(bundle_id, team_id)` (default policy is `None` — publisher trust is never inferred from `"Zalo.app"` or `"VNG"` substrings!)
  - `verify_macos_bundle_identity`, `enforce_signature_tier_or_err`
- **12-Surface Runtime Isolation (`P0-2`):**
  - `MACOS_12_ISOLATED_SURFACES`, `verify_12_surface_runtime_isolation`, `verify_external_db_path_isolated`, `derive_fixture_user_id`
- **Multi-Evidence `lstart` Process Ownership & Short `TMPDIR` (`P0-3`, `P1-1`):**
  - `RuntimeLaunchRecord`, `read_runtime_pid`, `read_runtime_launch_record`, `write_runtime_launch_record`, `clear_runtime_launch_files`, `collect_profile_owned_pids`, `cleanup_profile_short_tmpdir`, `DARWIN_SUN_PATH_MAX` (`104`), `MAC_CHROMIUM_TMPDIR_MAX_BYTES` (`49`)
- **Zero-TCC macOS 14+ Window Activation (`P1-3`):**
  - `WindowActivationOutcome`, `parse_activation_stdout`

---

## 2. Structured Error Code Mapping (`DimError::code()`)

| Condition in `dim-platform-macos` | `DimError` Variant | Machine Error Code |
| :--- | :--- | :--- |
| Path contains `\0`, control chars, `..`, bundle symlink escape, forbidden symlink on isolated surface, `/tmp/dz-<uid>/<id>` symlink squatting, external `db_path.location` cross-profile escape, or `>= 1024` bytes (`MACOS_PATH_MAX`) | `DimError::PathTraversal` | `PATH_TRAVERSAL` |
| Profile path or `config.json -> db_path.location` overlaps `~/Library/Application Support/ZaloData` (NFC or NFD) | `DimError::ProtectedPrimaryPath` | `PROTECTED_PRIMARY_PATH` |
| `stop()` targets any PID in primary `Zalo.app` / unmanaged `Zalo.app` protected set | `DimError::ProtectedPrimaryProcess` | `PROTECTED_PRIMARY_PROCESS` |
| Missing `/Applications/Zalo.app`, malformed `Info.plist`, wrong CPU slice, or missing `CustomExecutable`/fixture | `DimError::InstallNotFound` | `INSTALL_NOT_FOUND` |
| Unsigned, ad-hoc (`Signature=adhoc`), tampered root/nested helper, Gatekeeper-rejected, unpinned publisher (`PinnedPublisherPolicy == None`), or `-R=...` `TeamIdentifier` mismatch on macOS | `DimError::UnverifiedBinary` | `UNVERIFIED_BINARY` |
| `focus_or_restore()` invoked when profile has no live process (`status=missing` / dead PID) | `DimError::NotRunning` | `NOT_RUNNING` |
| `recover_stale_state()` invoked while profile is still running | `DimError::ProfileRunning` | `PROFILE_RUNNING` |
| Duplicate concurrent `.op-lock`, `stop()` timeout on hung PID, macOS 14+ `activate()` returning `false`, or `focus_or_restore()` called on unowned/recycled live PID | `DimError::ProcessError` | `PROCESS_ERROR` |
| Non-macOS host calling unconfigured `MacOsPlatformAdapter::new()` runtime methods | `DimError::UnsupportedPlatform` | `UNSUPPORTED_PLATFORM` |
| Unwritable (`0o500`) directory or insecure (`0o777`) short-tmp slot | `DimError::Io` | `IO_ERROR` |

---

## 3. Structured Cross-Worktree Integration Requests

Per strict file-ownership rules, the following shared-crate / CI / Tauri changes were **not** edited on `feat/macos-multi-instance` and are handed off with complete 6-field contracts:

### `IR-MACOS-CI-01` — Additive macOS Fixture & Strict Release-Regression Step in `cross-platform-validation.yml`
- **Current behavior:** In `integration/reliability-hardening` (`67b7447`), `.github/workflows/cross-platform-validation.yml` already defines opt-in native Darwin jobs `3. macos-arm64-build` (`macos-15`, `aarch64-apple-darwin`, `default: false`) and `4. macos-intel-build` (`macos-15-intel`, `x86_64-apple-darwin`, `default: false`; `macos-13` retired) running `clippy -D warnings`, `build-and-package.mjs` (`cargo test --workspace`), and `cargo test --manifest-path tests/cross-platform/Cargo.toml --locked`. Remote execution is currently gated because `.github/workflows/cross-platform-validation.yml` is not yet on default branch `origin/main`, local `gh` lacks `'workflow'` scope (grant least-privilege via `gh auth refresh -h github.com -s workflow`), and private-repo Actions billing uses USD-based metering (`$12.00/mo` Free credit, `$18.00/mo` Pro; `ubuntu-24.04` `$0.006/min` `1x`, `windows-2022` `$0.010/min` `2x`, `macos-15` / `macos-15-intel` `$0.062/min` `10x` multiplier; default Ubuntu+Windows dispatch `~$0.246/run` vs full 4-OS `~$2.044/run` = **88.0% savings**).
- **Desired behavior:** Append our 15-scenario `dim_macos_cli` E2E runner and C2's strict `--mode release-regression --require-native-os` macOS wrapper to both `macos-arm64-build` and `macos-intel-build` with JSON artifact upload.
- **Proposed contract:** Add an additive step in `macos-arm64-build` (`macos-15`) and `macos-intel-build` (`macos-15-intel`) running `node tests/platform-macos/run-macos-fixture-tests.mjs --out "${OUT_DIR}/macos-e2e-summary.json"` and `bash tests/cross-platform/scripts/run-macos.sh --mode release-regression --require-native-os` after `cargo test --manifest-path tests/cross-platform/Cargo.toml --locked`.
- **Compatibility implications:** Additive verification step on opt-in Darwin jobs only; preserves default dispatch cost (`~$0.246/run`) while advancing `DARWIN_NATIVE_TESTED` once run on `macos-15` & `macos-15-intel`.
- **Required test:** `node tests/platform-macos/run-macos-fixture-tests.mjs --out "${OUT_DIR}/macos-e2e-summary.json"` (`15/15 PASS`) + `bash tests/cross-platform/scripts/run-macos.sh --mode release-regression --require-native-os` exiting `0` on both `macos-15` and `macos-15-intel`.
- **File owner:** Central Integration / CI Maintainer (`.github/workflows/cross-platform-validation.yml`).

### `IR-TAURI-01` — Pinned Official VNG Apple `TeamIdentifier` Wiring in Tauri Settings / Commands
- **Current behavior:** `apps/desktop/src-tauri/src/commands.rs` instantiates `MacOsPlatformAdapter::new()`, whose `pinned_publisher_policy` defaults fail-closed to `None` (`UnverifiedPublisher`) until the real VNG Apple Developer `TeamIdentifier` is cryptographically confirmed on a Mac via `codesign -dv --verbose=4 /Applications/Zalo.app`.
- **Desired behavior:** Allow `commands.rs` (or a verified build-time constant once recorded on physical macOS hardware) to supply `.with_pinned_publisher("com.vng.zalo", "<VERIFIED_10CHAR_APPLE_TEAM_ID>")` and sign `MultiZaloDesktop.app` with Hardened Runtime (without App Sandbox, so `/usr/bin/open -n` and `/bin/ps` work under normal user privileges).
- **Proposed contract:** Keep `MacOsPlatformAdapter::new()` fail-closed (`None`) in library code until physical Mac `codesign -dv` output is captured in `live-zalo-test-checklist.md`, then pass `.with_pinned_publisher("com.vng.zalo", TEAM_ID)` in `apps/desktop/src-tauri/src/commands.rs`.
- **Compatibility implications:** Additive builder call on `MacOsPlatformAdapter`; preserves strict fail-closed publisher verification on macOS while leaving Windows/Linux unaffected.
- **Required test:** `p0_1_all_seven_bundle_and_codesign_identity_failure_modes_plus_tier5_verified_publisher` + live Mac `discover_installation(None)` returning `signature_status = "VerifiedPublisher (TeamID=..., Notarized Developer ID)"`.
- **File owner:** Desktop / Tauri Maintainer (`apps/desktop/src-tauri/**`).

### `IR-CORE-01` — Non-Windows `clippy::needless_bool` Cleanup in `dim-profile-store` & `db_path.location` Check in `dim-core`
- **Current behavior:** (1) `crates/profile-store/src/store.rs:337` (`is_symlink_or_reparse`) triggers `clippy::needless_bool` when cross-linting workspace dependencies for `aarch64-apple-darwin` / `x86_64-apple-darwin` without `--no-deps`. (2) `crates/core/src/zalo.rs` (`detect_zalo_user_id`) scans `<zalo_data_dir>/Database/_production/<digits>` without reading `<zalo_data_dir>/config.json -> db_path.location` (`F-CORE-01`, currently guarded on macOS inside `dim_platform_macos::verify_external_db_path_isolated`).
- **Desired behavior:** (1) Split `is_symlink_or_reparse` cleanly into `#[cfg(windows)]` and `#[cfg(not(windows))]` arms so `cargo clippy --workspace --target aarch64-apple-darwin --locked -- -D warnings` passes without `--no-deps`. (2) Optionally extend `detect_zalo_user_id` to inspect validated `config.json -> db_path.location` across all platforms.
- **Proposed contract:** Preserve `pub fn is_symlink_or_reparse(meta: &fs::Metadata) -> bool` and `pub fn detect_zalo_user_id(zalo_data_dir: &Path) -> Option<String>` signatures unchanged.
- **Compatibility implications:** 100% backwards-compatible across `dim-core`, `dim-profile-store`, Windows, Linux, and macOS.
- **Required test:** `cargo clippy --workspace --all-targets --target aarch64-apple-darwin --locked -- -D warnings` + unit test for `detect_zalo_user_id` with custom `config.json` `db_path.location`.
- **File owner:** Core / Profile-Store Maintainer (`crates/profile-store/**`, `crates/core/**`).

### `IR-RECOVERY-01` — Shared `STALE_DATA_DIR_ARTIFACTS` Parity in `dim-process-manager`
- **Current behavior:** `crates/process-manager/src/recovery.rs` lists `"SingletonLock"` and `"lockfile"` in `STALE_DATA_DIR_ARTIFACTS`, whereas macOS `ZaloData` also creates `"SingletonSocket"`, `"SingletonCookie"`, and `".window-state.json"` (already cleaned directly by `MacOsPlatformAdapter::recover_stale_state` and `MacOsPlatformAdapter::launch`).
- **Desired behavior:** Include `"SingletonSocket"` and `"SingletonCookie"` in `dim-process-manager` shared documentation/constants if generic recovery helpers are invoked directly.
- **Proposed contract:** Keep non-symlink-following `lstat` unlink semantics (`fs::symlink_metadata`) and never touch `/tmp/socketzalo*2021`.
- **Compatibility implications:** Safe no-op when files are absent on Windows; aligns Linux and macOS Chromium lock cleanup.
- **Required test:** `p0_4_defects_01_to_06_xp_mac_03_static_and_runtime_contracts` (`LC-06`).
- **File owner:** Process-Manager Maintainer (`crates/process-manager/**`).
