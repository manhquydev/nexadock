# Cross-Platform Synthetic Desktop Fixture (`dim-xf-fixture`)

Standalone multi-instance desktop test application used to validate `Desktop Instance Manager` process lifecycle, OS file locks, environment isolation, crash recovery, and fault tolerance across **Windows 10/11**, **macOS**, and **Ubuntu LTS** without using real Zalo Desktop credentials or user data.

## Runtimes Provided

1. **Native Rust Binary (`xf-desktop-fixture` + `dim_xf_fixture` library):**
   - Built from `fixtures/cross-platform/Cargo.toml` (standalone Cargo workspace; requires zero edits to the root `Cargo.toml`).
   - Exercises real Win32 `CreateFileW` share-mode locks (`dwShareMode = 0` or `FILE_SHARE_READ`) on Windows and `flock(LOCK_EX | LOCK_NB)` + Chromium-style dangling `SingletonLock` (`<hostname>-<pid>`) symlinks on macOS/Linux.
2. **Signed-Host Node.js Companion (`xf-fixture.mjs`):**
   - Runs under vendor-signed `node.exe` (`LaunchTarget::NodeFixture`) for Windows 11 Smart App Control (`VerifiedAndReputablePolicyState = 1`) compatibility, sharing the exact same CLI flags and JSON state schemas.

## Supported Capabilities & CLI Flags

| Capability | CLI Flag(s) | Behavior |
|---|---|---|
| **Instance ID** | `--instance-id <id>` (or `DIM_INSTANCE_ID` / `DIM_PROFILE_ID`) | Bound into `lockfile`, `fixture-state.json`, and deterministic chunk seed |
| **Profile Directory** | `--profile-dir <path>`, `--user-data-dir <path>` | Resolves isolated `ZaloData` via `APPDATA` (Windows), `CFFIXED_USER_HOME` (macOS), or `XDG_CONFIG_HOME` / `HOME` (Ubuntu) |
| **Process ID & Tree** | `--spawn-children <N>`, `--orphan-children` | Records `pid`, `child_pids` (`children.json`), and spawns real worker child processes |
| **Startup Timestamp** | Automatic | Persists `started_at_utc`, `ready_at_utc`, and monotonic `startup_duration_ms` |
| **Graceful Shutdown** | `.quit-signal`, `--z-update-force-quit`, `SIGTERM`, `SIGINT`, `--stay-ms <ms>` | Terminates owned workers, increments `clean_shutdown_count`, and releases/unlinks locks |
| **Simulated Crash** | `--crash-mode <immediate\|after-lock\|mid-write\|corrupt-state>`, `--crash-delay-ms <ms>` | Exits `101`/`137`/`138`/`139` leaving realistic stale locks, `.tmp` torn writes, or malformed caches |
| **File Writes & WAL** | `--write-files <N>`, `--write-bytes <B>`, `--seed <u64>`, `--heartbeat-ms <ms>` | Writes deterministic SplitMix64 chunks + SHA-256 `fixture-files.json` + `wal.log` |
| **OS File Locks** | `--lock-mode <exclusive\|read-shared\|none>` | Keeps OS file descriptor open throughout lifetime (visible to Windows `rstrtmgr.dll` & POSIX `flock`) |
| **State Persistence** | `--set-kv <key=value>` | Persists `boot_sequence`, `pid_history`, `persisted_kv`, and `observed_env` across restarts |
| **Slow Startup** | `--slow-startup-pre-lock-ms <ms>`, `--slow-startup-post-lock-ms <ms>`, `--slow-io-ms <ms>` | Injects pre-lock, post-lock, and per-file I/O delays |
| **Process Hang** | `--hang-on-quit`, `--quit-delay-ms <ms>` | Ignores graceful quit/signals and logs `HANG_ON_QUIT_IGNORED` to test forced kill escalation |
| **Controlled Errors** | `--error-mode <exit-code\|disk-full\|permission-denied\|corrupt-config\|lock-conflict>`, `--error-code <N>` | Writes `last-error.json` and exits with deterministic error code (`28`, `13`, `65`, `2`, or `<N>`) |
