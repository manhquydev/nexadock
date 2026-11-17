# macOS Adapter Integration Handoff (`dim-platform-macos`)

**Target Consumers:** `dim-process-manager`, `apps/desktop/src-tauri`, `fixtures/test-desktop-app` (`dim-cli`), and GitHub Actions CI  
**Scope Note:** Per strict branch isolation rules on `feat/macos-multi-instance`, zero files outside `crates/platform-macos/**`, `tests/platform-macos/**`, `docs/platform/macos/**`, and `plans/reports/macos/**` were modified. All existing public traits and exports remain 100% backwards-compatible.

---

## 1. Public Rust & Tauri Contract

### 1.1. Preserved Backwards-Compatible Exports (`dim_platform_macos::*`)

`apps/desktop/src-tauri/src/commands.rs` and `fixtures/test-desktop-app/src/bin/dim-cli.rs` continue to compile unchanged against:
- `MacOsPlatformAdapter::new()` (`Default`, `Clone`, `Debug`, `Send`, `Sync`)
- Trait implementations on `MacOsPlatformAdapter`:
  - `dim_core::ApplicationLocator`
  - `dim_core::PlatformCapabilities` (now also implementing `is_elevated()` -> `Some(geteuid() == 0)` and `check_profiles_acl_safe()` -> `Some((mode & 0o022) == 0 && uid_ok)`)
  - `dim_core::InstanceMonitor` (`inspect_primary`, `inspect_profile` with `working_set_bytes`, `disk_usage_bytes`, `window_handle`, `window_title`, and `lstat`-accurate `stale_lock_detected`)
  - `dim_core::InstanceLauncher` (`ensure_profile_layout`, idempotent `launch`, `focus_or_restore`, `stop`)
  - `dim_core::RecoveryManager` (`recover_stale_state`)
- Existing helper functions & constants:
  - `read_singleton_or_json_pid`, `build_open_args`, `ensure_macos_profile_layout`, `ensure_macos_user_symlinks`, `extract_plist_short_version`, `inspect_call_sockets_at`, `macos_zalo_data_dir`, `parse_singleton_lock_target`, `MACOS_CALL_SOCKETS`, `MACOS_SYMLINK_DIRS`.

### 1.2. New Configuration & Diagnostic APIs

- `MacOsPlatformAdapter::with_system_ops(Arc<dyn MacOsSystemOps>)` & `MacOsPlatformAdapter::for_fixture_host(simulated_home, short_tmp_root)`
- Builder methods:
  - `.with_signature_enforcement(bool)` (defaults to `true`)
  - `.with_expected_publisher(Option<String>)` (defaults to `Some("VNG".to_owned())`)
  - `.with_layout_options(MacOsLayoutOptions { symlink_user_dirs, symlink_keychain })`
  - `.with_user_data_dir_arg(bool)` (defaults to `true`)
  - `.with_launch_poll_ms(u64)` (defaults to `800`)
- Standalone modules: `bundle`, `codesign`, `layout`, `ops`, `process`, `window`, plus `LIVE_ZALO_STATUS_MARKER` (`"LIVE_ZALO_NOT_TESTED"`).

---

## 2. Structured Error Code Mapping (`DimError::code()`)

| Condition in `dim-platform-macos` | `DimError` Variant | Machine Error Code |
| :--- | :--- | :--- |
| Path contains `\0`, control chars, `..`, forbidden symlink on `Application Support`/`Preferences`, `/tmp` symlink squatting, or `>= 1024` bytes (`MACOS_PATH_MAX`) | `DimError::PathTraversal` | `PATH_TRAVERSAL` |
| Profile or data path overlaps `~/Library/Application Support/ZaloData` (NFC or NFD!) | `DimError::ProtectedPrimaryPath` | `PROTECTED_PRIMARY_PATH` |
| `stop()` targets any PID in primary `Zalo.app` / primary `ZaloData` tree | `DimError::ProtectedPrimaryProcess` | `PROTECTED_PRIMARY_PROCESS` |
| Missing `/Applications/Zalo.app`, `custom_root`, `CustomExecutable`, or fixture script | `DimError::InstallNotFound` | `INSTALL_NOT_FOUND` |
| Unsigned, ad-hoc (`Signature=adhoc`), tampered, Gatekeeper-rejected, or wrong-publisher bundle/binary on macOS | `DimError::UnverifiedBinary` | `UNVERIFIED_BINARY` |
| `focus_or_restore()` invoked when profile has no verified live process | `DimError::NotRunning` | `NOT_RUNNING` |
| `recover_stale_state()` invoked while profile is still running | `DimError::ProfileRunning` | `PROFILE_RUNNING` |
| Concurrent duplicate `launch`/`focus`/`stop` blocked by `Mutex<HashSet>` or `<profile_dir>/tmp/.op-lock`, or `open -n` non-zero exit | `DimError::ProcessError` | `PROCESS_ERROR` |
| Non-macOS host calling unconfigured `MacOsPlatformAdapter::new()` runtime methods | `DimError::UnsupportedPlatform` | `UNSUPPORTED_PLATFORM` |
| Unwritable (`0o500`) directory or `chmod 0o700` failure | `DimError::Io` | `IO_ERROR` |

---

## 3. Concrete Follow-Up Integration Requests for Shared Crates & CI

When merging `feat/macos-multi-instance` into `main`, the coordinator / shared-crate maintainer may optionally apply these four small shared-crate enhancements:

1. **`crates/profile-store/src/store.rs:337` (Darwin Clippy Cleanliness):**
   - Currently `is_symlink_or_reparse` triggers `clippy::needless_bool` when cross-linting workspace dependencies for `aarch64-apple-darwin` / `x86_64-apple-darwin` without `--no-deps` because the `#[cfg(windows)]` block is compiled out on Unix.
   - Replace lines `336-349` in `crates/profile-store/src/store.rs` with:
     ```rust
     #[must_use]
     pub fn is_symlink_or_reparse(meta: &fs::Metadata) -> bool {
         #[cfg(windows)]
         {
             use std::os::windows::fs::MetadataExt;
             const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
             meta.file_type().is_symlink() || (meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT) != 0
         }
         #[cfg(not(windows))]
         {
             meta.file_type().is_symlink()
         }
     }
     ```
2. **`crates/process-manager/src/recovery.rs` (`STALE_DATA_DIR_ARTIFACTS`):**
   - Add `"SingletonSocket"`, `"SingletonCookie"`, and `".window-state.json"` alongside `"SingletonLock"` in `STALE_DATA_DIR_ARTIFACTS` (note: `MacOsPlatformAdapter::launch` already cleans these automatically via `recover_macos_stale_artifacts`).
3. **`apps/desktop/src-tauri/tauri.conf.json` & Entitlements (macOS Distribution):**
   - Sign `MultiZaloDesktop.app` with Hardened Runtime + Apple Notarization for Gatekeeper distribution; no `com.apple.security.app-sandbox` entitlement should be enabled on the launcher so `/usr/bin/open -n` and `ps` inspection operate unrestricted under standard user privileges.
4. **`.github/workflows/ci.yml` (macOS Runner Matrix):**
   - Add `macos-14` (`aarch64-apple-darwin`) and `macos-13` (`x86_64-apple-darwin`) jobs running:
     - `cargo clippy --workspace --all-targets --locked -- -D warnings`
     - `cargo test --workspace --all-targets --locked`
     - `node tests/platform-macos/run-macos-fixture-tests.mjs`
