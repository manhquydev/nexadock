# Ubuntu Runtime Hardening & Native Validation Report

**Date:** `2026-11-18`  
**Branch:** `feat/ubuntu-multi-instance`  
**Baseline Commits:** `e25078a`, `c853fa2`, `9a10a9f` (atop `a5feb4a`)  
**Crate:** [`crates/platform-linux`](../../../crates/platform-linux/src/lib.rs) (`dim-platform-linux`)  
**Mission:** B2 — Sửa các vấn đề vòng đời và bảo mật đã phát hiện, chứng minh Linux adapter thực sự chạy được trên Ubuntu 24.04.

---

## 1. Verification Status Matrix (B2 Final)

| Status Token | Verdict | Evidence |
| :--- | :--- | :--- |
| **`LINUX_ADAPTER_TESTED`** | **`NOT_TESTED` on live Ubuntu kernel** | Host Windows 11 Pro không có WSL, Docker, Podman, VirtualBox, QEMU, hoặc SSH target. GitHub Actions bị chặn vì branch chưa được push (theo yêu cầu). `run-native-ubuntu-validation.sh` đã sẵn sàng cho Ubuntu 24.04 runner. |
| **`DESKTOP_FIXTURE_TESTED`** | **`PORTABLE_HOST_VERIFIED_UBUNTU_RUNTIME_NOT_TESTED`** | 45/45 Rust tests (16 unit + 29 integration) PASS. 29/29 Node fixture scenarios PASS. Cross-compiled Clippy-clean cho `x86_64-unknown-linux-gnu` và `aarch64-unknown-linux-gnu`. |
| **`BWRAP_LIFETIME_FIX`** | **`IMPLEMENTED` / `NOT_TESTED` on live kernel** | `LauncherLifetimePolicy::PersistentDetachedSession` default, `(pid, starttime_ticks)` PID-reuse-safe tracking, graceful `.quit-signal` + `SIGTERM` → `waitpid(WNOHANG)` → leaves-first `SIGKILL`. |
| **`LDD_REMOVAL`** | **`IMPLEMENTED` / `VERIFIED`** | Zero `Command::new("ldd")` hoặc `std::process` calls trong `discovery.rs`. Pure-Rust static ELF64 parser với bounds-checked `PT_INTERP`, `PT_DYNAMIC`, `PT_LOAD`. |
| **`APPARMOR_PREFLIGHT`** | **`IMPLEMENTED` / `NOT_TESTED` on live AppArmor** | Delimiter-bounded full-path matching, `#` comment stripping, `APPARMOR_PROFILE_NON_GUARANTEE_WARNING`. |
| **`WAYLAND_FOCUS_HONEST`** | **`IMPLEMENTED` / `VERIFIED`** | `focus_or_restore` returns `Err(DimError::UnsupportedPlatform("UNSUPPORTED_OPERATION: ..."))` trên Wayland/Headless. Không bao giờ trả về fake `Ok(())`. |

---

## 2. Bugs Reproduced & Fixed

### 2.1. P0 — `bwrap --die-with-parent` Kills Sessions on CLI Exit

**Root Cause:** `build_bwrap_desktop_args` truyền `--die-with-parent` vô điều kiện. `bwrap` gọi `PR_SET_PDEATHSIG(SIGKILL)` tied to parent PID. Khi `dim-cli launch <id>` thoát (tiến trình ngắn hạn), `bwrap` nhận `SIGKILL` → tất cả tiến trình con bị terminate ngay lập tức.

**Fix — `LauncherLifetimePolicy` enum:**

| Policy | Behavior | Use Case |
| :--- | :--- | :--- |
| `PersistentDetachedSession` **(default)** | `bwrap --new-session` không có `--die-with-parent`. Trên Unix: `setsid()` / `setpgid(0,0)` trong `pre_exec`. | Phiên Zalo tồn tại sau khi CLI thoát. |
| `DieWithParent` | `bwrap --die-with-parent` được giữ lại. | Test harness, bound subprocess. |

**PID-Reuse-Safe Ownership:**
- `(pid, starttime_ticks)` từ `/proc/<pid>/stat` field 22 (index 19 sau `)`).
- 128-bit `DIM_INSTANCE_NONCE` unique per launch.
- `DIM_PROFILE_ID` persisted trong `.runtime-pid` (mode `0600`).
- `stop()` re-verify `(pid, starttime_ticks)` trước mỗi signal delivery.
- Leaves-first `SIGKILL`: outer `bwrap` last, inner renderer/zygote first.
- `PROTECTED_PRIMARY_PROCESS` guard: không bao giờ terminate process ngoài phạm vi quản lý.

**Graceful Shutdown Sequence:**
1. Write `.quit-signal` file.
2. `SIGTERM` to verified app main PID.
3. Poll `waitpid(WNOHANG)` with timeout.
4. Escalate: leaves-first `SIGKILL` to remaining verified PIDs.
5. Clean `.runtime-pid` after confirmed exit.

### 2.2. P0 — Linux Desktop Runtime Compatibility (Minimal `bwrap` Binds)

**Root Cause:** `bwrap` environment sau `env_clear()` có thể thiếu critical runtime sockets, hoặc bind quá nhiều (toàn bộ `$HOME`, `/dev`, `$XDG_RUNTIME_DIR`).

**Fix — `plan_bwrap_desktop_sandbox`:**

| Resource | Bind Strategy | Security Tradeoff |
| :--- | :--- | :--- |
| **Wayland** (`$WAYLAND_DISPLAY`) | `--ro-bind-try` exact socket path | None — read-only display server. |
| **X11** (`$DISPLAY`) | `--ro-bind-try /tmp/.X11-unix/X<n>` (exact socket, not full `/tmp/.X11-unix/`) | `SANDBOX_TRADEOFF: X11 protocol allows input injection across clients.` |
| **D-Bus Session** | `--bind-try` parsed `unix:path=...` only. Reject `unix:abstract=*` and `/run/dbus/system_bus_socket`. | `SANDBOX_TRADEOFF: Session D-Bus exposes org.freedesktop.* interfaces.` |
| **PipeWire** | `--ro-bind-try $XDG_RUNTIME_DIR/pipewire-0` | `SANDBOX_TRADEOFF: PipeWire allows audio/video access.` |
| **PulseAudio** | `--ro-bind-try $XDG_RUNTIME_DIR/pulse/native` | `SANDBOX_TRADEOFF: PulseAudio allows microphone access.` |
| **File Chooser** | `GTK_USE_PORTAL=1` + xdg-desktop-portal over session D-Bus | None — portal-mediated. |
| **Full `$HOME`** | **NEVER bound** | — |
| **Full `/dev`** | **NEVER bound** | — |
| **Full `$XDG_RUNTIME_DIR`** | **NEVER bound** — empty `0700` tmpfs mounted, individual sockets bound. | — |
| **System D-Bus** | **NEVER bound** | — |

**`DesktopSocketSurface` enum:** Surfaces isolation reductions in `BwrapSandboxPlan::security_tradeoff_notes` and `capabilities()`.

**Wayland Focus:** `focus_or_restore` returns `Err(DimError::UnsupportedPlatform("UNSUPPORTED_OPERATION: Wayland does not support unprivileged external focus stealing"))`. X11 focus uses PID-verified `wmctrl -i -a` on exact XID.

### 2.3. P1 — `ldd` Executes Untrusted Code via `PT_INTERP`

**Root Cause:** `discovery.rs` gọi `Command::new("ldd")` trên binary chưa xác minh. Theo `ldd(1)`: *"Be aware that in some circumstances … ldd may attempt to directly execute the program"*. ELF binary với `PT_INTERP` chứa đường dẫn script/binary adversarial sẽ execute arbitrary code.

**Fix — Pure-Rust Static ELF64 Parser:**
- `parse_elf64_dynamic_metadata`: Bounds-checked parsing `Elf64_Ehdr` → `Elf64_Phdr` → `PT_INTERP`, `PT_DYNAMIC`, `PT_LOAD`.
- `inspect_elf64_needed_libraries_at`: Extract `DT_NEEDED`, `DT_RUNPATH`, `DT_STRTAB` via file-backed `PT_LOAD` vaddr-to-file-offset translation. Reject BSS tail, truncated tables, oversized `DT_STRSZ`.
- `resolve_elf64_missing_libraries_at`: Check library existence via `fs::metadata` only — zero process execution.
- `validate_executable_format()`: Enforced in `adapter.launch()` path cho `CustomExecutable`. Reject corrupt `.AppImage` headers với `UNVERIFIED_BINARY`.

**SHA-256 Audit:**
- Self-implemented `Sha256State` using `u64 total_bytes * 8` (supports >512 MiB).
- Cross-verified against Node `crypto.createHash('sha256')` across 9 test vectors (0, 55, 56, 63, 64, 65, 128, 1000, 1048576 bytes).
- Docs explicitly state: SHA-256 proves byte-level integrity against trusted reference only, **NEVER** publisher identity or software origin.

### 2.4. P1 — AppArmor False-Match on Bare Filename

**Root Cause:** `has_targeted_apparmor_userns_profile` matched bare filename (`electron`) against unrelated profile paths. Ví dụ: `/home/user/Downloads/electron` matched profile cho `/usr/lib/electron/electron`.

**Fix:**
- Delimiter-bounded full-path matching (`contains_exact_profile_path`).
- `#` comment stripping in AppArmor profile parsing.
- `APPARMOR_PROFILE_NON_GUARANTEE_WARNING`: Explicitly warns rằng AppArmor `userns` profile existence không guarantee third-party AppImage sẽ hoạt động.

**4-Domain Preflight (`PreflightCategory`):**

| Category | Checks |
| :--- | :--- |
| `UserNamespaceAndAppArmor` | `apparmor_restrict_unprivileged_userns`, targeted profile matching, `CLONE_NEWUSER` availability. |
| `FilesystemAndMount` | `/proc/mounts` `noexec`/`nosuid` on binary mount and profile `tmp/` scratch mount. `+x` bit. `0o777` insecure permissions. |
| `RuntimeEnvironment` | `/dev/fuse`, headless display, missing shared libraries (static `DT_NEEDED`), ELF header/arch validation. |
| `SecurityPolicyGuard` | `UID == 0` root block, `--no-sandbox` rejection, `setuid` binary detection, unsafe wrapper script analysis. |

### 2.5. P0 — `focus_or_restore` Returns Fake `Ok(())`

**Root Cause:** Trên Wayland và Headless targets, `focus_or_restore` trả về `Ok(())` dù không thể thực hiện focus — misleading caller.

**Fix:** Returns `Err(DimError::UnsupportedPlatform("UNSUPPORTED_OPERATION: ..."))` trên Wayland và Headless. `OperationOutcome` serialization sử dụng `error_code = Some("UNSUPPORTED_PLATFORM")` với `"UNSUPPORTED_OPERATION:"` message prefix.

> **Integration Request:** Đề xuất thêm `DimError::UnsupportedOperation` variant riêng trong `dim-core` (hiện tại sử dụng `UnsupportedPlatform` với prefix).

### 2.6. Edge Cases Found During Verification (5 items)

| # | Finding | Fix |
| :--- | :--- | :--- |
| 1 | `entry_profile_id` rejected `bwrap dumpable=1` outer process (empty `/proc/<pid>/environ`) | Removed `.environ.is_empty()` gate — always check `cmdline_setenv_value` fallback. |
| 2 | `launch()` missing `validate_executable_format()` for `CustomExecutable` | Added `report.validate_executable_format()?` in `CustomExecutable` launch path. |
| 3 | `is_appimage` missed `ExtractedAppDir` for AppArmor check | Widened `matches!` to include `AppImageType2 | AppImageUruntimeDwarfs | ExtractedAppDir`. |
| 4 | Native validation script crash-teardown race | Added `wait_pid_dead "$crashed_pid" 40` after crash detection. |
| 5 | Validation script `dirname` not found on bare `PATH` | Replaced with pure bash `${var%/*}` parameter expansion. |

---

## 3. Regression & Security Test Suite (B2 Additions)

### 3.1. Test Coverage Summary

| Test Layer | B1 Count | B2 Count | Total |
| :--- | :---: | :---: | :---: |
| **Rust Unit Tests** | 16 | 0 (existing cover new code) | **16** |
| **Rust Integration Tests** | 15 | +14 regression | **29** |
| **Node Fixture Scenarios** | 15 | +14 regression | **29** |
| **Native Ubuntu Script** | 0 | +1 (7 checks) | **1** |

All tests: **45/45 Rust PASS**, **29/29 Node PASS**, `0` failures.

### 3.2. New Regression Tests (14 scenarios)

| ID | Test Name | Verifies |
| :--- | :--- | :--- |
| R01 | `regression_01` / `P1-R01` | Launcher exit while child remains alive (PersistentDetachedSession). |
| R02 | `regression_02` / `P1-R02` | Manager graceful shutdown sequence. |
| R03 | `regression_03` / `P1-R03` | Unexpected parent termination (crash recovery). |
| R04 | `regression_04` / `P1-R04` | Stale lockfile detection (dangling `SingletonLock`). |
| R05 | `regression_05` / `P1-R05` | PID reuse with `starttime_ticks` validation. |
| R06 | `regression_06` / `P1-R06` | Dangling symlinks across inspect/discover/launch. |
| R07 | `regression_07` / `P1-R07` | HOME/XDG isolation + env stripping (`LD_PRELOAD`, `NODE_OPTIONS`, `ELECTRON_RUN_AS_NODE`). |
| R08 | `regression_08` / `P1-R08` | File chooser / Downloads portal (`GTK_USE_PORTAL=1`). |
| R09 | `regression_09` / `P1-R09` | Runtime socket permissions & minimal `bwrap` binds. |
| R10 | `regression_10` / `P1-R10` | Unsupported Wayland focus → `UNSUPPORTED_OPERATION`. |
| R11 | `regression_11` / `P1-R11` | Invalid `.AppImage` (garbage, truncated ELF, script masquerade). |
| R12 | `regression_12` / `P1-R12` | Untrusted ELF64 static inspection (zero process execution canary). |
| R13 | `regression_13` / `P1-R13` | Permission denial (`noexec`, AppArmor, root, `0o644`, `0o777`). |
| R14 | `regression_14` / `P1-R14` | Restart persistence (SQLite DB + Downloads survive relaunch). |

### 3.3. Native Ubuntu Validation Script

[`run-native-ubuntu-validation.sh`](../../../tests/platform-linux/run-native-ubuntu-validation.sh) — 7-check turnkey script cho Ubuntu 24.04:

1. **OS & AppArmor Preflight** — Detect Ubuntu version, AppArmor status, kernel `userns` restriction.
2. **Native Rust Compile** — `cargo build -p dim-platform-linux` on actual `x86_64-unknown-linux-gnu`.
3. **Unit + Integration Tests** — `cargo test -p dim-platform-linux` with live `/proc`, `statvfs`, `SIGTERM`/`waitpid`.
4. **Fixture Lifecycle** — Launch/focus/stop Node fixture with real `setsid` detach.
5. **Multi-Profile Isolation** — Concurrent profiles with verified `HOME`/`XDG` separation.
6. **Crash Recovery** — Kill fixture mid-run, verify stale lock detection + clean restart.
7. **Tauri Prerequisites** — Check `libwebkit2gtk-4.1`, `libayatana-appindicator3`, `librsvg2`.

**Host Guard:** Script refuses execution on non-Linux kernels (`--verify-host-guard --json` outputs `BLOCKED_NOT_LINUX_KERNEL`).

---

## 4. Native Ubuntu Execution — Blockers & Path Forward

### 4.1. Proven Blockers on Current Host

| Probe | Result | Evidence |
| :--- | :--- | :--- |
| WSL | Not installed | `wsl --status` exit code 50. |
| Docker / Podman | Not installed | `docker`, `podman` not found. |
| VirtualBox / QEMU | Not installed | `VBoxManage`, `qemu-system-x86_64` not found. |
| SSH targets | None configured | Zero `~/.ssh/config` host entries. |
| GitHub Actions | Blocked | Branch not pushed (forbidden without user authorization). Remote has 0 workflows. |

### 4.2. Recommended Execution Paths (For User)

**Option A — Push & GitHub Actions (Recommended):**
1. User authorize: `git push origin feat/ubuntu-multi-instance`.
2. Add CI job in `.github/workflows/ci.yml` with `runs-on: ubuntu-24.04`.
3. Run `tests/platform-linux/run-native-ubuntu-validation.sh`.
4. If all 7 checks PASS → promote `LINUX_ADAPTER_TESTED` to `PASS`.

**Option B — WSL2:**
1. `wsl --install -d Ubuntu-24.04`.
2. Inside WSL: `cd /mnt/c/.../zalo-ubuntu-worktree && bash tests/platform-linux/run-native-ubuntu-validation.sh`.
3. Note: WSL2 chạy Linux kernel thật nhưng không có GNOME/Wayland desktop → desktop integration tests sẽ SKIP.

**Option C — Ubuntu VM / Remote Machine:**
1. Clone branch vào Ubuntu 24.04 desktop.
2. `bash tests/platform-linux/run-native-ubuntu-validation.sh`.
3. Full GNOME/Wayland coverage.

---

## 5. What Cannot Be Tested From This Host

| Item | Reason | Workaround |
| :--- | :--- | :--- |
| Live `/proc/<pid>/stat` parsing | Requires Linux kernel | Ubuntu 24.04 runner (Option A/B/C). |
| `bubblewrap` sandbox activation | Requires Linux kernel + `bwrap` binary | Ubuntu VM with `bubblewrap` installed. |
| AppArmor profile loading | Requires Ubuntu 24.04 with AppArmor enabled | Ubuntu 24.04 desktop test machine. |
| Wayland/X11 window focus | Requires GNOME desktop session | Ubuntu 24.04 desktop VM (Option C). |
| Live Zalo community port | Requires user-supplied `.AppImage` + disposable accounts | User-supplied binary on Ubuntu desktop. |
| FUSE mount (`/dev/fuse`) | Requires Linux kernel with FUSE support | Ubuntu VM/native. |
| PipeWire/PulseAudio audio | Requires running audio daemon | Ubuntu desktop session. |

---

## 6. Security Impact Summary

### 6.1. Hardened (B2 vs B1)

| Attack Surface | B1 State | B2 State |
| :--- | :--- | :--- |
| **Process lifetime** | `--die-with-parent` kills session on CLI exit | `PersistentDetachedSession` default + PID-reuse-safe `(pid, starttime_ticks)` ownership. |
| **Untrusted binary inspection** | `ldd` on untrusted ELF → arbitrary code execution | Pure-Rust static ELF64 parser, zero process execution. |
| **AppArmor matching** | Bare-filename false match | Delimiter-bounded full-path match only. |
| **Wayland focus** | Fake `Ok(())` | Honest `UNSUPPORTED_OPERATION` error. |
| **bwrap binds** | Full `$XDG_RUNTIME_DIR` potential | Individual validated socket-only binds. |
| **Launch validation** | Missing format check for `CustomExecutable` | `validate_executable_format()` enforced. |
| **bwrap env detection** | Empty `/proc/environ` rejection | `cmdline_setenv_value` fallback for `dumpable=1`. |

### 6.2. Remaining Risks (Documented, Not Addressable by Adapter Alone)

| Risk | Mitigation Status | Note |
| :--- | :--- | :--- |
| X11 input injection | `SANDBOX_TRADEOFF` warning surfaced | Fundamental X11 limitation — Wayland resolves. |
| Session D-Bus exposure | `SANDBOX_TRADEOFF` warning surfaced | Required for xdg-desktop-portal. |
| Community port `app.asar` integrity | SHA-256 integrity-only, `UNVERIFIED_BINARY` on mismatch | Cannot prove origin/publisher identity. |
| Hardcoded call ports `29631`/`29632` | `concurrent_calls_status = Blocked` | Requires upstream fix in `zalo-for-linux`. |
| Electron `contextIsolation: false` in call window | Documented in `security-and-license.md` | Upstream community port design. |

---

## 7. New Public API Exports (B2)

### `layout.rs`
- `LauncherLifetimePolicy` — `PersistentDetachedSession` / `DieWithParent`.
- `DesktopSocketSurface` — X11, Wayland, DbusSession, PipeWire, PulseAudio.
- `BwrapSandboxOptions`, `BwrapSandboxPlan`.
- `DIM_INSTANCE_NONCE_ENV`, `DIM_PROFILE_ID_ENV`.
- `generate_instance_nonce`, `build_sanitized_linux_spawn_env_with_nonce`.
- `build_bwrap_desktop_args_with_policy`, `plan_bwrap_desktop_sandbox`.
- Socket path parsers: `parse_dbus_session_socket_path`, `parse_x11_display_socket_path`, `resolve_wayland_socket_path`, `resolve_pipewire_socket_path`, `resolve_pulse_socket_path`.
- `is_safe_bindable_socket_or_file`.

### `procfs.rs`
- `RuntimePidHint`, `read_proc_stat_starttime`.
- `runtime_hint_matches_entry`.
- `ProcEntry::starttime_ticks`, `ProfileProcessTree::process_starttimes`.

### `discovery.rs`
- `inspect_elf64_needed_libraries`, `inspect_elf64_needed_libraries_at`.
- `parse_elf64_dynamic_metadata`, `read_elf64_dynamic_metadata`.
- `resolve_elf64_missing_libraries_at`.
- `Elf64DynamicMetadata`, `Elf64ParseError`.

### `isolation.rs`
- `evaluate_appimage_preflight`, `find_mount_for_path`.
- `parse_proc_mounts`, `preflight_live_appimage`.
- `AppImagePreflightReport`, `PreflightDiagnostic`, `ProcMountEntry`.
- `APPARMOR_PROFILE_NON_GUARANTEE_WARNING`.

### `status.rs`
- `PreflightCategory`, `PreflightDiagnosticKind`, `PreflightSeverity`.

---

## 8. Integration Requests (Cumulative B1 + B2)

| ID | Priority | Target | Description | Status |
| :--- | :--- | :--- | :--- | :--- |
| IR-01 | **High** | `crates/profile-store/src/store.rs:337` | Fix `clippy::needless_bool` — blocks Linux CI Clippy. | OPEN |
| IR-02 | Medium | `apps/desktop/src-tauri/src/commands.rs` | Support `LaunchTarget::CustomExecutable` in Tauri Linux IPC. | OPEN |
| IR-03 | Low | `crates/profile-store/src/backup.rs:339` | Check `.config/ZaloData` path for Linux restore. | OPEN |
| IR-04 | Optional | `crates/core/src/error.rs` | Dedicated `DimError::SecurityBlocked` variant. | OPEN |
| IR-05 | Medium | `.github/workflows/ci.yml` | Wire `run-native-ubuntu-validation.sh` on `ubuntu-24.04` runner. | OPEN |
| **IR-06** (NEW) | Optional | `crates/core/src/error.rs` | Dedicated `DimError::UnsupportedOperation` variant (currently `UnsupportedPlatform` with `"UNSUPPORTED_OPERATION:"` prefix). | OPEN |

---

## 9. Community Port vs Adapter Separation

> **Critical Distinction:** `dim-platform-linux` là adapter quản lý phiên desktop. Nó KHÔNG phải là community port, KHÔNG tải, KHÔNG build, KHÔNG patch, và KHÔNG phân phối bất kỳ component nào của VNG Zalo.

| Aspect | `dim-platform-linux` (This Adapter) | `VN-Linux-Family/zalo-for-linux` (Community Port) |
| :--- | :--- | :--- |
| **Purpose** | Multi-instance lifecycle management | Port macOS Zalo to Linux AppImage. |
| **Contains VNG binary** | ❌ Zero bytes | ✅ `app.asar` extracted from macOS `.dmg`. |
| **License** | `MIT OR Apache-2.0` | `BSD-3-Clause` (scripts) + Unfree (compiled AppImage). |
| **Relationship** | Tests compatibility with user-supplied AppImage. | Provides the AppImage being tested. |
| **Responsibility** | Adapter bugs → our fix. | Community port bugs → upstream issue. |
| **Testing** | Fixture tests (Node mock) ≠ Zalo tests. | Live Zalo requires user-supplied binary + account. |

---

## 10. Files Modified (B2)

| File | Change Summary | Lines Δ |
| :--- | :--- | :--- |
| `adapter.rs` | `LauncherLifetimePolicy`, honest `focus_or_restore`, validated launch, PID-reuse-safe stop. | +549 |
| `layout.rs` | Minimal `bwrap` binds, `DesktopSocketSurface`, socket parsers, portal support. | +925 |
| `procfs.rs` | `starttime_ticks`, `RuntimePidHint`, instance nonce, `runtime_hint_matches_entry`. | +388 |
| `lib.rs` | All new `pub` re-exports. | +613 |
| `discovery.rs` | Static ELF64 parser, `ldd` removed, SHA-256 audit. | +1778 |
| `isolation.rs` | `AppImagePreflightReport`, `/proc/mounts` parsing, fixed AppArmor matching. | +1187 |
| `status.rs` | `PreflightCategory`, `PreflightDiagnosticKind`, `PreflightSeverity`. | +307 |
| `ubuntu_adapter_suite.rs` | 29 integration tests (14 new regression). | +2610 |
| `run-ubuntu-fixture-tests.mjs` | 29 scenarios (14 new P1-R*). | +2295 |
| `run-native-ubuntu-validation.sh` | NEW: 7-check turnkey Ubuntu 24.04 script. | NEW |
| `README.md` | Updated test documentation. | +45 |
| `fixture-suite-host-evidence.json` | Regenerated 29/29 results. | +148 |

**Total:** ~10,238 insertions across 12 files.

---

## 11. Recommended Commit Structure

```
fix(platform-linux): harden bwrap lifecycle, desktop socket binds, and Wayland focus

- Introduce LauncherLifetimePolicy enum (PersistentDetachedSession default)
- PID-reuse-safe (pid, starttime_ticks) + DIM_INSTANCE_NONCE ownership
- Minimal validated bwrap socket binds (never full $HOME, /dev, $XDG_RUNTIME_DIR)
- Honest UNSUPPORTED_OPERATION error on Wayland/Headless focus
- Graceful .quit-signal + SIGTERM → waitpid → leaves-first SIGKILL
```

```
fix(platform-linux): replace ldd with static ELF64 parser and refine AppArmor preflight

- Remove all Command::new("ldd") calls from discovery.rs
- Pure-Rust bounds-checked ELF64 parser (PT_INTERP, PT_DYNAMIC, PT_LOAD)
- Delimiter-bounded AppArmor profile matching (no bare-filename bypass)
- 4-domain preflight: UserNamespaceAndAppArmor, Filesystem, Runtime, SecurityPolicy
- validate_executable_format() enforced in CustomExecutable launch path
```

```
test(platform-linux): add 14 regression scenarios and native Ubuntu validation harness

- 14 new Rust integration tests (regression_01..14)
- 14 new Node fixture scenarios (P1-R01..14)
- run-native-ubuntu-validation.sh: 7-check turnkey Ubuntu 24.04 script
- All 45/45 Rust + 29/29 Node tests PASS
```

```
docs(ubuntu): add runtime hardening and native validation report

- Comprehensive B2 hardening report with reproduced bugs and fixes
- Native Ubuntu execution blockers and recommended paths
- Security impact analysis (B1 vs B2)
- Updated integration requests (IR-01..IR-06)
```

---

## 12. Related Documents

- [implementation-report.md](./implementation-report.md) — B1 implementation report.
- [security-and-license.md](./security-and-license.md) — License & security audit.
- [integration-handoff.md](./integration-handoff.md) — Integration contract & IRs.
- [appimage-compatibility.md](./appimage-compatibility.md) — AppImage & dependency matrix.
- [xia-zalo-for-linux-report.md](../../../plans/reports/ubuntu/xia-zalo-for-linux-report.md) — Community port research.
