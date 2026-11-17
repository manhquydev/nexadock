# Ubuntu Linux Adapter — Integration Contract & Handoff

**Date:** `2026-11-17`  
**Branch:** `feat/ubuntu-multi-instance`  
**Owned Paths Modified:**
- `desktop-instance-manager/crates/platform-linux/**`
- `desktop-instance-manager/tests/platform-linux/**`
- `desktop-instance-manager/docs/platform/ubuntu/**`
- `desktop-instance-manager/plans/reports/ubuntu/**`

Zero files in `crates/core`, `crates/profile-store`, `crates/process-manager`, `crates/platform-windows`, `crates/platform-macos`, `apps/desktop`, or `.github/workflows` were modified on this branch.

---

## 1. Implemented Trait & Public API Contract (`dim-platform-linux`)

`LinuxPlatformAdapter` implements all four platform traits from `dim-core` without breaking any existing signature:

| Trait & Method | Linux Behavior & Invariants |
| :--- | :--- |
| `ApplicationLocator::discover_installation(None)` | Returns `is_valid: false`, `support_status: SupportStatus::Unsupported` (`OFFICIAL_ZALO_UNSUPPORTED`), listing any discovered XDG/local community candidates as non-official informational `notes`. |
| `ApplicationLocator::discover_installation(Some(path))` | Validates `path` (`PATH_TRAVERSAL`, `PROTECTED_PRIMARY_PATH`), inspects ELF64/AppImage Type-2/`uruntime`/AppDir format, permissions, startup-script hazards (`SECURITY_BLOCKED` -> `SupportStatus::Blocked`), and optional pinned SHA-256 (`UNVERIFIED_BINARY`), returning `SupportStatus::Experimental` when valid. |
| `PlatformCapabilities::capabilities()` | Reports `official_zalo_status = Unsupported`, `fixture_status = Experimental`, `appimage_status = Some(Experimental)`, `concurrent_calls_status = Blocked`, plus live Ubuntu/AppArmor/Wayland/PipeWire diagnostics. |
| `PlatformCapabilities::free_disk_bytes(path)` | Calls `libc::statvfs` walking up to the nearest existing ancestor directory (`f_bavail * f_frsize`). |
| `PlatformCapabilities::is_elevated()` | Returns `Some(geteuid() == 0)` on Unix. |
| `PlatformCapabilities::check_profiles_acl_safe(dir)` | Verifies `dir` is a non-symlink directory owned by `geteuid()` without group/world write bits (`(mode & 0o022) == 0`), enabling `InstanceCoordinator::verify_isolation` check #6 (`RestrictiveNtfsAcls`) to pass on Linux instead of returning `NOT_TESTED`. |
| `InstanceMonitor::inspect_primary()` | Inspects `$XDG_CONFIG_HOME/ZaloData` (or `$HOME/.config/ZaloData`), collects the primary process tree and `VmRSS` bytes from `/proc`, and populates `protected_pids`. |
| `InstanceMonitor::inspect_profile(profile, dir)` | Inspects `<dir>/.config/ZaloData`, detects dangling `SingletonLock` symlinks via `symlink_metadata`, verifies PID ownership via `/proc/<pid>/environ` (`DIM_PROFILE_ID`) to defeat recycled-PID spoofing, aggregates `VmRSS` and `disk_usage_bytes`, and queries X11 `wmctrl -lp` window title/XID when `DISPLAY` is active. |
| `InstanceLauncher::ensure_profile_layout(profile, dir)` | Creates `0o700` isolated `HOME` + XDG directories, `.zalo-profile-id` (`PROFILE_MARKER_MISMATCH` on conflict), `zmeta.json`, `migrate-config.json`, and `.config/user-dirs.dirs`. |
| `InstanceLauncher::launch(target, profile, dir)` | Rejects `OfficialZalo` (`UNSUPPORTED_PLATFORM`), `UID == 0`, `--no-sandbox`, and unconfined AppImages blocked by Ubuntu 24.04 AppArmor (`PROCESS_ERROR` prefixed with `SECURITY_BLOCKED:`); spawns with `Command::env_clear()` + sanitized env (or `bwrap`), polling up to `1,500 ms` for lockfile readiness. |
| `InstanceLauncher::focus_or_restore(...)` | Atomically writes `.focus-signal` for fixtures; activates PID-verified XID via `wmctrl`/`xdotool` on X11 or triggers single-instance restore handoff on Wayland. |
| `InstanceLauncher::stop(state, protected_pids, timeout_ms)` | Enforces `PROTECTED_PRIMARY_PROCESS` across all tree PIDs, writes `.quit-signal`, sends `SIGTERM` to `main_pid`, polls with `waitpid(WNOHANG)`, and escalates to leaves-first `SIGKILL` strictly on verified profile PIDs. |

### Additional Linux-Specific Methods on `LinuxPlatformAdapter`
- `LinuxPlatformAdapter::with_bwrap(bool)` — Opt-in `bubblewrap` mount/IPC sandbox.
- `LinuxPlatformAdapter::with_expected_sha256(hex)` — Cryptographic SHA-256 pinning for user-supplied binaries.
- `LinuxPlatformAdapter::discover_compatible_applications() -> Vec<DiscoveredLinuxApp>` — User-agnostic XDG `.desktop` + AppImage directory scanner.
- `LinuxPlatformAdapter::inspect_local_binary(path) -> Result<BinaryInspectionReport, DimError>` — Detailed format, SHA-256, `ldd`, and script audit report.
- `LinuxPlatformAdapter::probe_ubuntu_environment() -> UbuntuRuntimeProbe` — OS, AppArmor, Wayland/X11, PipeWire/PulseAudio, FUSE, and V4L2 probe.
- `LinuxPlatformAdapter::prepare_restored_profile(profile, dir)` — Post-restore `tmp/` recreation and `0o700` permission hardening.

---

## 2. Error Code Mapping Consistency (`DimError::code()`)

| Failure Condition | `DimError` Variant | Machine Code |
| :--- | :--- | :--- |
| Official VNG Zalo launch or incompatible ELF CPU arch (`aarch64` vs `x86_64`) | `DimError::UnsupportedPlatform` | `UNSUPPORTED_PLATFORM` |
| Missing `node`, missing fixture script, or missing custom AppImage/AppDir | `DimError::InstallNotFound` | `INSTALL_NOT_FOUND` |
| SHA-256 mismatch against `with_expected_sha256` | `DimError::UnverifiedBinary` | `UNVERIFIED_BINARY` |
| `..` traversal, NUL byte, or pre-planted symlink inside profile layout | `DimError::PathTraversal` | `PATH_TRAVERSAL` |
| Custom binary or profile path overlaps `~/.config/ZaloData` | `DimError::ProtectedPrimaryPath` | `PROTECTED_PRIMARY_PATH` |
| Stop requested on PID overlapping primary Zalo tree | `DimError::ProtectedPrimaryProcess` | `PROTECTED_PRIMARY_PROCESS` |
| Profile directory `.zalo-profile-id` belongs to another profile | `DimError::ProfileIdentityMismatch` | `PROFILE_MARKER_MISMATCH` |
| Focus requested on stopped profile | `DimError::NotRunning` | `NOT_RUNNING` |
| `UID == 0`, `--no-sandbox`, setuid binary, unsafe script, or Ubuntu 24.04 AppArmor `userns` block | `DimError::ProcessError("SECURITY_BLOCKED: ...")` | `PROCESS_ERROR` |

---

## 3. Integration Requests for Shared Core, Tauri UI, and CI

During cross-target Linux compilation and end-to-end integration review, five concrete integration items outside our branch's file-ownership boundary were identified for the coordinator/integrator:

### IR-01 (High Priority — Blocks Linux CI Clippy): Fix `clippy::needless_bool` in `crates/profile-store/src/store.rs:337`
- **Observed Failure:** Running `cargo clippy --target x86_64-unknown-linux-gnu -- -D warnings` fails in `crates/profile-store/src/store.rs:337-349` (`is_symlink_or_reparse`) because when `#[cfg(windows)]` is compiled out on Linux/macOS, `if meta.file_type().is_symlink() { return true; } false` triggers `-D clippy::needless-bool`.
- **Recommended Fix in `crates/profile-store/src/store.rs`:**
  ```rust
  #[must_use]
  pub fn is_symlink_or_reparse(meta: &fs::Metadata) -> bool {
      #[cfg(windows)]
      {
          use std::os::windows::fs::MetadataExt;
          const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
          meta.file_type().is_symlink()
              || (meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT) != 0
      }
      #[cfg(not(windows))]
      {
          meta.file_type().is_symlink()
      }
  }
  ```

### IR-02 (Medium Priority — Tauri Linux Launch Target): Support `LaunchTarget::CustomExecutable` in `apps/desktop/src-tauri/src/commands.rs`
- **Observed Gap:** `commands.rs:271` (`launch_profile`) and `commands.rs:285` (`focus_profile`) hardcode `LaunchTarget::OfficialZalo { custom_install_root: None }`, which always returns `UNSUPPORTED_PLATFORM` on Linux.
- **Recommended Integration:** Allow `AppState` / Tauri IPC on Linux to accept an optional user-selected `custom_executable_path` (plus optional `expected_sha256` and `prefer_bwrap: bool`), delegating to `LinuxPlatformAdapter::discover_compatible_applications()` for UI picker suggestions and constructing `LaunchTarget::CustomExecutable`.

### IR-03 (Low Priority — Multi-OS Restore User ID Detection): Check `.config/ZaloData` in `crates/profile-store/src/backup.rs:339`
- **Observed Gap:** `FileBackupManager::restore_backup_as_new_profile` (`backup.rs:339-345`) checks `dest_profile_dir.join("AppData/Roaming/ZaloData")` before falling back to `manifest.zalo_user_id`, omitting `.config/ZaloData` (Linux) and `Library/Application Support/ZaloData` (macOS) if `manifest.zalo_user_id` was `None`. (Runtime `inspect_profile` on Linux already detects `.config/ZaloData` immediately.)

### IR-04 (Optional Trait/Error Enhancement — `crates/core`): Dedicated `DimError::SecurityBlocked` Variant
- Currently `dim-platform-linux` prefixes security-blocked errors with `"SECURITY_BLOCKED: ..."` inside `DimError::ProcessError` and `ApplicationInstallation.signature_status = "SECURITY_BLOCKED"` so `dim-core` stays untouched. Adding `DimError::SecurityBlocked(String)` -> `"SECURITY_BLOCKED"` in `crates/core/src/error.rs` will expose a dedicated top-level `error_code` to CLI/UI consumers.

### IR-05 (CI Workflow — `.github/workflows/ci.yml`): Wire Ubuntu Fixture Suite on `ubuntu-latest`
- Add `node tests/platform-linux/run-ubuntu-fixture-tests.mjs` on `runner.os == 'Linux'` in `.github/workflows/ci.yml` alongside `cargo test --workspace --all-targets` so every push/PR verifies both `test_16_native_linux_adapter_end_to_end_launch_focus_and_stop` and all 15 Node.js multi-instance XDG/HOME scenarios on a real Ubuntu 24.04 runner.
