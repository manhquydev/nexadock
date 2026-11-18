# Ubuntu Linux Multi-Instance Adapter — Implementation & Feasibility Report

**Date:** `2026-11-17`  
**Branch:** `feat/ubuntu-multi-instance`  
**Crate:** [`crates/platform-linux`](../../../crates/platform-linux/src/lib.rs) (`dim-platform-linux`)  
**Scope:** Ubuntu LTS multi-instance desktop adapter implementation, `VN-Linux-Family/zalo-for-linux` compatibility research, isolation security analysis, and automated test suites (strictly desktop — zero Zalo Web wrappers).

---

## 1. Acceptance & Verification Status Matrix

Per acceptance rules, verification states are strictly separated and **never** collapsed into a generic `"Supported"` badge:

| Status Token | Current Verdict | Evidence & Scope |
| :--- | :--- | :--- |
| **`OFFICIAL_ZALO_UNSUPPORTED`** | **`OFFICIAL_ZALO_UNSUPPORTED`** (`CONFIRMED`) | VNG publishes no official native Linux Zalo Desktop build (`app.asar` contains only `win32` and `darwin` native helper branches). `LinuxPlatformAdapter::discover_installation(None)` returns `SupportStatus::Unsupported` and `launch(OfficialZalo)` returns `UNSUPPORTED_PLATFORM`. |
| **`LINUX_ADAPTER_TESTED`** | **`NOT_TESTED` on live Ubuntu kernel** *(Cross-compiled & Clippy-clean for `x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu`; `19/19` portable + virtual-`/proc` Rust tests PASS)* | The workstation executing this build is Windows 11 (`x86_64-pc-windows-gnu`) without WSL, Docker, or a reachable Ubuntu runner. Static/cross-target checks and virtualized `/proc` tests are **not** substituted for live Linux kernel execution; `test_16_native_linux_adapter_end_to_end_launch_focus_and_stop` and `run-ubuntu-fixture-tests.mjs` are wired to assert live `/proc`, `statvfs`, `0o700`, and `SIGTERM`/`waitpid` on Ubuntu CI (`ubuntu-latest`). |
| **`DESKTOP_FIXTURE_TESTED`** | **`PORTABLE_HOST_VERIFIED` / `NOT_TESTED` on native Ubuntu kernel** | `15/15` scenarios in `tests/platform-linux/run-ubuntu-fixture-tests.mjs` and real concurrent `node fixture-app.mjs` multi-instance tests (`test_02`, `test_07`, `test_08`) passed on host with isolated `HOME`/`XDG_*` environments; native Ubuntu runner execution remains required before Promoting to Ubuntu `DESKTOP_FIXTURE_TESTED = PASS`. |
| **`COMMUNITY_PORT_TESTED`** | **`NOT_TESTED` / `BLOCKED`** | No Ubuntu desktop test machine and no user-supplied `.AppImage` binary or disposable Zalo accounts were available in this session. Live Zalo compatibility on Linux is **not** claimed from fixture tests. |
| **`BLOCKED` / `SECURITY_BLOCKED`** | **`CONFIRMED` active guards** | 1) Live community-port testing blocked by missing Ubuntu desktop host & user-supplied binary. 2) Unconfined Electron `.AppImage` launch on Ubuntu 24.04 is `SECURITY_BLOCKED` unless a targeted `/etc/apparmor.d/` `userns` profile exists (never auto-adding `--no-sandbox`). 3) Concurrent calls in `zalo-for-linux` are `BLOCKED` by hardcoded TCP ports `127.0.0.1:29631` and `127.0.0.1:29632`. |

---

## 2. Ubuntu Target Compatibility Matrix

Compiling for `x86_64-unknown-linux-gnu` or `aarch64-unknown-linux-gnu` does **not** imply universal support across all Ubuntu releases. Each target and desktop subsystem has distinct runtime behavior:

### 2.1. OS & Architecture Matrix

| Target Environment | Adapter Build Check | Runtime Status | Key OS-Specific Technical Factors |
| :--- | :--- | :--- | :--- |
| **Ubuntu 24.04 LTS (`noble`, `x86_64`)** *(Primary)* | **PASS** (`cargo clippy --target x86_64-unknown-linux-gnu -- -D warnings`) | `NOT_TESTED` locally (Ready for `ubuntu-latest` CI) | Kernel 6.8 enables `kernel.apparmor_restrict_unprivileged_userns = 1` by default; `t64` ABI transition renames packages (`libfuse2t64`, `libasound2t64`, `libgtk-3-0t64`); ships `libwebkit2gtk-4.1-0` natively for Tauri 2 and `libfuse3-3` for `uruntime` AppImages. |
| **Ubuntu 22.04 LTS (`jammy`, `x86_64`)** *(Secondary)* | **PASS** (glibc-compatible target) | `NOT_TESTED` | Kernel 5.15/6.5 HWE leaves unprivileged `userns` unrestricted by default (`apparmor_restrict_unprivileged_userns` absent/0); uses pre-`t64` package names (`libfuse2`, `libasound2`); requires `libwebkit2gtk-4.1-0` from `jammy-updates`/`security` for Tauri 2. |
| **Ubuntu 24.04 / 22.04 LTS (`aarch64` / ARM64)** | **PASS** (`cargo clippy --target aarch64-unknown-linux-gnu -- -D warnings`) | `NOT_TESTED` | Requires matching `aarch64` (`EM_AARCH64 = 0xB7`) target binary; `LinuxPlatformAdapter` rejects `x86_64` ELF binaries on `aarch64` hosts (and vice versa) with `UNSUPPORTED_PLATFORM` before spawn. |

### 2.2. Ubuntu Desktop Subsystem Matrix

| Subsystem | Observed Mechanics & Multi-Instance Behavior | Adapter Handling |
| :--- | :--- | :--- |
| **Wayland (Ubuntu 24.04 default)** | GNOME 46 `mutter` forbids external unprivileged focus stealing and hides global window-to-PID tables; Electron 22 has a `gtk_shell::get_gtk_surface` `SIGTRAP` bug on `hide()`->`show()` (mitigated in `zalo-for-linux` via `wayland-titlebar`). | Preserves `WAYLAND_DISPLAY` & `XDG_RUNTIME_DIR`; uses single-instance restore handoff for focus on Wayland rather than broken X11 calls. |
| **X11 / XWayland** | `_NET_WM_PID` is exposed on top-level windows. | Queries `wmctrl -lp` filtered strictly by `state.process_ids` (ignoring `Shared Worker`/`SQLite` helper windows) and activates exact XID (`wmctrl -i -a`) or `xdotool --pid`. |
| **GNOME Desktop & System Tray** | Stock Ubuntu GNOME has Ubuntu AppIndicators (`org.kde.StatusNotifierWatcher`), whereas vanilla GNOME without the extension lacks a tray host (`plugins/tray-host/index.js` quits on window close instead of hiding). | Independent per-profile `zalo-linux-window-state.json` inside `<profile>/.config/ZaloData`; shared tray icons lack visual profile badges unless distinguished by tooltip/account. |
| **WebKitGTK (Tauri 2 Host)** | Tauri 2 links against `webkit2gtk-4.1` (`libwebkit2gtk-4.1-0`), `librsvg2-2`, and `libayatana-appindicator3-1`. | Mapped in `discovery::map_soname_to_ubuntu_packages` for preflight diagnostics. |
| **AppArmor & User Namespaces** | Ubuntu 24.04 restricts `CLONE_NEWUSER` for unconfined binaries; ships `/etc/apparmor.d/bwrap-userns-restrict` for `/usr/bin/bwrap`. | Probed via `isolation::probe_ubuntu_runtime_at`; enforces `SECURITY_BLOCKED` instead of `--no-sandbox`. |
| **Filesystem Permissions** | Shared multi-user Linux hosts expose `0o755` directories if default `umask 0022` is left unchanged. | Enforces `0o700` on `Profiles/` and `<profile_dir>` subdirectories and `0o600` on seed/marker files; verified by `check_profiles_acl_safe`. |
| **Notifications (`org.freedesktop.Notifications`)** | Routed over session D-Bus (`$XDG_RUNTIME_DIR/bus`). Multiple instances share `app_name = "Zalo"` / `zalo.desktop`. | Session D-Bus preserved; shared desktop identity documented as UX grouping limitation. |
| **PipeWire / PulseAudio** | Ubuntu 24.04 runs PipeWire with `pipewire-pulse` at `$XDG_RUNTIME_DIR/pulse/native`; `zcall-audio` uses `libpulse-simple.so.0`. | Preserves `XDG_RUNTIME_DIR`, `PULSE_SERVER`, and `PIPEWIRE_REMOTE` across sanitized spawns and `bwrap` socket binds. |
| **Camera / Microphone** | Mic shares cleanly via PipeWire; raw V4L2 `/dev/video0` allows one reader at a time unless mediated by PipeWire camera portal; concurrent `zcall` blocked earlier by TCP `29631`/`29632`. | Probes `/dev/video0`, `pipewire-0`, and `pulse/native` in `UbuntuRuntimeProbe`; marks `concurrent_calls_status = Blocked`. |

---

## 3. Rust Linux Adapter Architecture (`dim-platform-linux`)

The crate is organized into six cohesive modules with zero `unwrap`/`expect` in production code and `missing_docs = "warn"` / `unsafe_op_in_unsafe_fn = "deny"` clean across `x86_64` and `aarch64`:

1. [`status.rs`](../../../crates/platform-linux/src/status.rs): Defines `VerificationLabel`, `SecurityBlockReason` (`SECURITY_BLOCKED` formatting), and `UbuntuTargetTier`.
2. [`discovery.rs`](../../../crates/platform-linux/src/discovery.rs):
   - Streaming FIPS 180-4 SHA-256 implementation (`sha256_file`, `sha256_bytes`) verified against NIST test vectors (requiring zero `Cargo.lock` modifications).
   - ELF64 (`x86_64`/`aarch64`), AppImage Type-2 (`AI\x02`), `uruntime`/DwarFS, extracted `AppDir`, and wrapper-script classifier.
   - Read-only `KNOWN_COMMUNITY_RELEASES` SHA-256 reference ledger (`26.11.10`, `26.10.20`) plus caller-pinned SHA-256 enforcement (`UNVERIFIED_BINARY`).
   - Startup-script static security scanner (`analyze_startup_script`) catching `sudo`, `pkexec`, `--no-sandbox`, `sysctl`, `apparmor_parser`, `killall`/`pkill`, `LD_PRELOAD`, and hardcoded `/home/<user>`.
   - XDG `.desktop` parser (`Exec=` quoting, `%U` stripping, `StartupWMClass`) and user-agnostic AppImage scanner (`$XDG_DATA_HOME/applications`, `~/.local/share/applications`, `$XDG_DATA_DIRS`, `~/Applications`, `~/.local/bin`, `/opt/zalo`).
   - `ldd` missing-library parser mapping SONAMEs to Ubuntu 24.04 (`t64`) and Ubuntu 22.04 `apt` packages.
3. [`layout.rs`](../../../crates/platform-linux/src/layout.rs):
   - Creates `<profile_dir>/{.config/ZaloData,.config/Zalo,.local/share,.local/state,.cache,tmp,Downloads,Documents/Zalo Received Files}` with `0o700` permissions, rejecting any symlink escape (`PATH_TRAVERSAL`) or `.zalo-profile-id` mismatch (`PROFILE_MARKER_MISMATCH`).
   - Pre-seeds `.config/zmeta.json`, `.config/ZaloData/migrate-config.json`, and `.config/user-dirs.dirs` + `user-dirs.locale`.
   - `build_sanitized_linux_spawn_env` for `Command::env_clear()` (stripping `LD_PRELOAD`, `LD_AUDIT`, `NODE_OPTIONS`, `APPDATA`, `USERPROFILE`) and `build_bwrap_desktop_args` (binding external AppImages read-only without exposing real `$HOME`).
4. [`isolation.rs`](../../../crates/platform-linux/src/isolation.rs):
   - 5-tier isolation comparison (`compare_linux_isolation_mechanisms`), `UbuntuRuntimeProbe`, `enforce_launch_security_policy`, non-root `UID != 0` guard, and `render_recommended_apparmor_profile`.
5. [`procfs.rs`](../../../crates/platform-linux/src/procfs.rs):
   - `/proc/<pid>/{stat,status,environ,cmdline}` parser safe for spaces/parentheses in `comm` and filtering dead zombies (`state == 'Z'`).
   - Profile ownership verification (`DIM_PROFILE_ID` / `XDG_CONFIG_HOME` / `HOME`) defeating recycled-PID spoofing and capturing reparented helper daemons (`zocr-host`, `zcall-native`), plus `VmRSS` aggregation and PID-filtered `wmctrl -lp` parsing.
6. [`adapter.rs`](../../../crates/platform-linux/src/adapter.rs):
   - Implements `ApplicationLocator`, `PlatformCapabilities`, `InstanceMonitor`, and `InstanceLauncher` (`SIGTERM` -> `waitpid(WNOHANG)` poll -> leaves-first verified `SIGKILL` fallback; zero `killall`/`pkill`).

---

## 4. Isolation Mechanisms Comparison

> **Critical Security Distinction:** Redirecting `HOME` and `XDG_*` (`SeparateHomeAndXdgProfile`) provides **application data and lockfile separation** between cooperative instances, **NOT** a security sandbox. Without `bubblewrap` or an LSM boundary, a compromised process still runs with the user's full ambient POSIX UID authority and can open `/home/<user>/.ssh` directly by path.

| Mechanism | Profile Data Separation | True OS Security Sandbox | Loopback TCP (`29631`/`29632`) Isolation | Stock Ubuntu 24.04 Unprivileged Viability | Assessment |
| :--- | :---: | :---: | :---: | :---: | :--- |
| **A. Process Env Only (`XDG_*`)** | Partial (`$HOME` leaks) | **No** | No | Yes | Insufficient: `getpwuid`/`$HOME` callers leak into real `~`. |
| **B. Separate `HOME` + XDG + `user-dirs.dirs` (`0o700`)** | **Yes** | **No** | No | **Yes** | **Selected default data-isolation layer.** |
| **C. `bubblewrap` (`bwrap`)** | **Yes** | **Yes** (mount + IPC ns; hides real `$HOME`) | Only with `--unshare-net` (breaks internet unless `pasta`/`slirp4netns` added) | Outer `bwrap` permitted by `/etc/apparmor.d/bwrap-userns-restrict`; inner Electron `chrome-sandbox` nested `userns` can still fail | **Selected opt-in filesystem/IPC sandbox (`with_bwrap(true)`)**; fails closed with `SECURITY_BLOCKED` if unavailable. |
| **D. Raw User Namespaces (`CLONE_NEWUSER`)** | No (needs mount setup) | **Yes** | Only with netns + user-mode NAT | **Blocked** for unconfined binaries (`apparmor_restrict_unprivileged_userns=1`) | Blocked by Ubuntu 24.04 AppArmor without `/etc/apparmor.d/` `userns` grant. |
| **E. Complementary OS Controls (`Landlock`, `systemd --user` cgroups, targeted AppArmor)** | No (complements B) | **Yes** | No | **Yes** (Landlock needs no `userns`; AppArmor profile is admin-installed once) | Best defense-in-depth pairing with Layer B on Ubuntu 24.04 without `--no-sandbox`. |

---

## 5. Zalo Live Compatibility Matrix (Section 8 Assessment)

Because no Ubuntu test desktop and no user-supplied Zalo `.AppImage` were present in this session, **live Zalo community-port testing was NOT executed** (`COMMUNITY_PORT_TESTED = NOT_TESTED`, overall live compatibility = `BLOCKED`):

| Workflow | Official VNG Zalo | Community Port (`VN-Linux-Family/zalo-for-linux` `26.11.10`) | Static / Architectural Notes |
| :--- | :--- | :--- | :--- |
| **Open 2 concurrent instances** | `OFFICIAL_ZALO_UNSUPPORTED` | `BLOCKED` / `NOT_TESTED` | `SingletonLock` isolates under `<profile>/.config/ZaloData`, but Ubuntu 24.04 requires an admin-provisioned AppArmor `userns` profile (since `--no-sandbox` is `SECURITY_BLOCKED`). |
| **Independent QR / password login** | `OFFICIAL_ZALO_UNSUPPORTED` | `NOT_TESTED` (`BLOCKED`) | Cookies & `Partitions/zalo` reside under isolated `userData`; Chromium `basic` / keyring cookie store behavior across two profiles requires live Ubuntu GNOME Keyring test. |
| **Session persistence across restart** | `OFFICIAL_ZALO_UNSUPPORTED` | `NOT_TESTED` (`BLOCKED`) | SQLite `Database/_production/<uid>` and `Local State` persist in `<profile>/.config/ZaloData`. |
| **Desktop notifications** | `OFFICIAL_ZALO_UNSUPPORTED` | `NOT_TESTED` (`BLOCKED`) | Uses `org.freedesktop.Notifications`; both instances report app identity `Zalo` (`zalo.desktop`). |
| **Text messaging & E2EE sync** | `OFFICIAL_ZALO_UNSUPPORTED` | `NOT_TESTED` (`BLOCKED`) | Depends on clean-room Rust `db-cross-v4` N-API addon inside the community AppImage. |
| **File send / receive** | `OFFICIAL_ZALO_UNSUPPORTED` | `NOT_TESTED` (`BLOCKED`) | Isolated via `XDG_DOWNLOAD_DIR`, `XDG_DOCUMENTS_DIR`, and `.config/user-dirs.dirs` inside `<profile_dir>`. |
| **Voice & video calls** | `OFFICIAL_ZALO_UNSUPPORTED` | **`BLOCKED`** (`CONFIRMED` port conflict) | Single-instance calls require host `libopus0`, `libpulse0`, `pulseaudio-utils`, `python3`; **concurrent multi-instance calls fail on hardcoded `127.0.0.1:29631` / `127.0.0.1:29632` (`patch-zcall-native.js:51-52`)**. |

---

## 6. B2 Runtime Hardening (2026-11-18)

Mission B2 hardened this adapter with:
- **Process Lifetime Fix:** `LauncherLifetimePolicy::PersistentDetachedSession` default (replaces unconditional `--die-with-parent`).
- **PID-Reuse-Safe Tracking:** `(pid, starttime_ticks)` + 128-bit `DIM_INSTANCE_NONCE`.
- **Static ELF64 Parser:** Pure-Rust bounds-checked parser replacing all `ldd` calls.
- **Minimal `bwrap` Binds:** Individual validated socket binds instead of broad mounts.
- **Honest Wayland Focus:** `UNSUPPORTED_OPERATION` error instead of fake `Ok(())`.
- **AppArmor Delimiter Match:** Full-path matching only, no bare-filename bypass.
- **14 Regression Tests:** Covering launcher exit, PID reuse, stale locks, symlinks, permission denial.
- **Native Validation Script:** 7-check `run-native-ubuntu-validation.sh` for Ubuntu 24.04.

Updated test counts: **45/45** Rust (16 unit + 29 integration), **29/29** Node fixture scenarios.

See [runtime-hardening-and-native-validation.md](./runtime-hardening-and-native-validation.md) for full details.

---

## 7. Related Deliverables

- [runtime-hardening-and-native-validation.md](./runtime-hardening-and-native-validation.md) ← **B2 Report**
- [appimage-compatibility.md](./appimage-compatibility.md)
- [security-and-license.md](./security-and-license.md)
- [integration-handoff.md](./integration-handoff.md)
- [xia-zalo-for-linux-report.md](../../../plans/reports/ubuntu/xia-zalo-for-linux-report.md)
