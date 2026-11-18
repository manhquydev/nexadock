# Ubuntu / Linux Platform Adapter Test Suite

This directory contains the runtime multi-instance fixture & P1 security/regression harness for `dim-platform-linux` ([`run-ubuntu-fixture-tests.mjs`](./run-ubuntu-fixture-tests.mjs)) and the native Ubuntu 24.04 kernel/AppArmor validation runner ([`run-native-ubuntu-validation.sh`](./run-native-ubuntu-validation.sh)), paired with the Cargo integration test suite in [`../../crates/platform-linux/tests/ubuntu_adapter_suite.rs`](../../crates/platform-linux/tests/ubuntu_adapter_suite.rs).

## Coverage Summary

- **Rust Unit + Integration Suite (`dim-platform-linux`)**:
  - 16 unit tests in [`crates/platform-linux/src/`](../../crates/platform-linux/src/lib.rs)
  - 15 portable core integration tests + 1 native Linux E2E launch/focus/stop test (`test_01`..`test_16`)
  - 14 P1 regression & security integration tests (`regression_01`..`regression_14`) in [`ubuntu_adapter_suite.rs`](../../crates/platform-linux/tests/ubuntu_adapter_suite.rs)
- **Node.js Multi-Instance Fixture & Security Suite ([`run-ubuntu-fixture-tests.mjs`](./run-ubuntu-fixture-tests.mjs))**:
  - 15 core multi-instance scenarios (`#01`..`#15`) + 14 P1 regression & security scenarios (`#16`..`#29` / `P1-R01`..`P1-R14`) = **29 total scenarios**:
    1. **Launcher exit while child remains alive**: `PersistentDetachedSession` (`--new-session` without `--die-with-parent`) + real short-lived launcher exit proving detached grandchild `fixture-app.mjs` survives and answers `.focus-signal`.
    2. **Manager graceful shutdown**: `.quit-signal` + `SIGTERM` clean shutdown leaving sibling profiles responsive, plus virtual `/proc` outer-`bwrap` + inner-Electron leaves-first escalation ordering.
    3. **Unexpected parent termination**: `LauncherLifetimePolicy::DieWithParent` (`--die-with-parent`) + orphaned/reparented `ppid=1` helper capture via `DIM_PROFILE_ID` + `DIM_INSTANCE_NONCE`.
    4. **Stale lockfile**: Dead-PID `lockfile` JSON & `SingletonLock` stale state detection and clean `STALE_LOCK_RECOVERED` recovery.
    5. **PID reuse**: `(pid, starttime_ticks)` (`/proc/<pid>/stat` field 22) + 128-bit `DIM_INSTANCE_NONCE` validation rejecting recycled PIDs even with identical binary names and never signaling unrelated processes.
    6. **Dangling symlinks**: Dangling `SingletonLock` detection via `symlink_metadata` / `lstatSync` (`Path::exists` is `false`) and `PATH_TRAVERSAL` rejection of broken binary/profile symlinks.
    7. **`HOME`/XDG isolation**: Full `HOME`, `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `XDG_STATE_HOME`, `XDG_CACHE_HOME`, `TMPDIR` confinement and stripping of `LD_PRELOAD`, `LD_AUDIT`, `LD_LIBRARY_PATH`, `NODE_OPTIONS`, `ELECTRON_RUN_AS_NODE`, `APPDATA`, `USERPROFILE`, `CFFIXED_USER_HOME`.
    8. **File chooser / Downloads**: `GTK_USE_PORTAL=1`, `XDG_DOWNLOAD_DIR`, `XDG_DOCUMENTS_DIR`, `Documents/Zalo Received Files`, and `.config/user-dirs.dirs` confinement.
    9. **Runtime socket permissions & minimal `bwrap` binds**: Isolated `0700` `$XDG_RUNTIME_DIR` tmpfs, rejection of `/run/dbus/system_bus_socket` & `unix:abstract=*`, zero full `$HOME`/`/dev`/`$XDG_RUNTIME_DIR` binds, and X11/D-Bus/PipeWire/PulseAudio `SANDBOX_TRADEOFF` diagnostics.
    10. **Unsupported Wayland focus**: Honest `Err(DimError::UnsupportedPlatform("UNSUPPORTED_OPERATION: ..."))` on Wayland and Headless `CustomExecutable` focus (never fake `Ok(())`), while preserving deterministic `.focus-signal` IPC for `NodeFixture`.
    11. **Invalid `.AppImage`**: Garbage, truncated ELF64 header, out-of-bounds `Elf64_Phdr`, and script-masquerade `.AppImage` rejection across `inspect_user_binary`, `validate_executable_format` (`UNVERIFIED_BINARY`), `evaluate_appimage_preflight` (`INVALID_EXECUTABLE_HEADER`), `discover_compatible_apps_in_roots`, and `adapter.launch`.
    12. **Untrusted executable metadata inspection**: Zero-execution static ELF64 `PT_INTERP` (`"/tmp/untrusted-interp"`), `PT_LOAD`, and `PT_DYNAMIC` (`DT_NEEDED`, `DT_STRTAB`, `DT_STRSZ`, `DT_RUNPATH` `$ORIGIN/lib`) parsing (`ldd` never invoked), adversarial header rejection, FIPS 180-4 SHA-256 verification across `0B`, `55B`, `56B`, `63B`, `64B`, `65B`, `1000B`, and `1 MiB`, and integrity-only provenance notes.
    13. **Permission denial**: Non-executable binary (`!is_executable` / `MISSING_EXECUTABLE_BIT`), `/proc/mounts` `noexec` (`MOUNT_OPTION_NOEXEC` with octal `\040` unescaping), `0o777` world-writable `Profiles` (`INSECURE_FILESYSTEM_PERMISSIONS`), exact-path AppArmor `userns` matching, and `UID == 0` root `SECURITY_BLOCKED` guard.
    14. **Restart persistence**: Launch -> write persistent session DB & `Downloads` -> graceful stop -> relaunch -> verify `launch_count == 2`, `zalo_user_id`, and preserved session DB across restart.

## Running Verification

```bash
cd desktop-instance-manager

# 1. Lint & execute Rust unit + 30 integration tests (29 portable + 1 native Linux E2E)
cargo fmt --package dim-platform-linux -- --check
cargo clippy -p dim-platform-linux --all-targets --locked -- -D warnings
cargo clippy -p dim-platform-linux --no-deps --all-targets --target x86_64-unknown-linux-gnu --locked -- -D warnings
cargo test -p dim-platform-linux --locked

# 2. Execute the 29-scenario Node.js multi-instance XDG/HOME & P1 regression suite
node --check tests/platform-linux/run-ubuntu-fixture-tests.mjs
node tests/platform-linux/run-ubuntu-fixture-tests.mjs --report-out plans/reports/ubuntu/fixture-suite-host-evidence.json

# 3. On a native Ubuntu 24.04 LTS desktop/host ONLY (hard-blocks non-Linux hosts):
bash tests/platform-linux/run-native-ubuntu-validation.sh --report-out plans/reports/ubuntu/native-ubuntu-validation.json
```

## Acceptance Status Semantics

The test runners strictly separate verification states and never collapse them into a single `"Supported"` badge:

- `LINUX_ADAPTER_TESTED`: Emitted as `PASS` **only** when executed natively on a Linux kernel (`process.platform === 'linux'`). Remains `NOT_TESTED` on non-Linux hosts.
- `DESKTOP_FIXTURE_TESTED`: Emitted as `PASS` when `fixture-app.mjs` multi-instance XDG/HOME/lockfile/recovery scenarios pass on Linux (`PORTABLE_HOST_VERIFIED_UBUNTU_RUNTIME_NOT_TESTED` when run on non-Linux hosts).
- `COMMUNITY_PORT_TESTED`: Remains `NOT_TESTED` / `BLOCKED` unless a user-supplied `.AppImage` is manually verified in a disposable non-production Ubuntu desktop session.
- `OFFICIAL_ZALO_UNSUPPORTED`: Always enforced (`CONFIRMED_UNSUPPORTED`) because VNG publishes no official Linux desktop application.
