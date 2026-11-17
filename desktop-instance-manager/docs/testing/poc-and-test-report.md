# PoC Verification & Test Report — Desktop Instance Manager

**Workspace:** [`desktop-instance-manager/`](../../)  
**Latest Verification Date (UTC):** `2026-11-17` (Post-hardening verification finalized at `2026-11-17T15:38:14Z`)  
**Evidence Artifacts:**
- [`evidence/fixture-test-result.json`](../../evidence/fixture-test-result.json)
- [`evidence/contract-parity-result.json`](../../evidence/contract-parity-result.json)
- [`evidence/zalo-live-poc-result.json`](../../evidence/zalo-live-poc-result.json)
- [`evidence/artifact-manifest.json`](../../evidence/artifact-manifest.json)
- [`evidence/sbom-cyclonedx.json`](../../evidence/sbom-cyclonedx.json)
- [`evidence/dependency-audit.json`](../../evidence/dependency-audit.json)

---

## 1. Environment & Execution Scope

| Check Component | Host State Observed | Execution Status |
|---|---|---|
| OS & Elevation | Windows 11 Pro `10.0.26300` x64, non-elevated (`is_elevated: false`) | Active |
| Node.js Runtime | `v26.13.1` | Executed `run-fixture-tests.mjs`, `run-contract-parity-tests.mjs`, `ui-server.mjs`, `generate-build-provenance.mjs` |
| PowerShell Bridge | Windows PowerShell (`scripts/dim-host.ps1`) | Executed all CLI, security & live Zalo PoC flows |
| Official Zalo Desktop | `26.10.20` (`Signed (VNG)`), primary running (`PID 12980`) | Executed 3 concurrent PoC instances alongside primary |
| Rust Toolchain (`rustc` / `cargo`) | `rustc 1.100.0` (`stable-x86_64-pc-windows-gnu`) + WinLibs MinGW-w64 `16.2.0` (`BrechtSanders.WinLibs.POSIX.UCRT`) | **Executed locally:** `29` workspace tests + Tauri 2 compile/clippy/test + release binaries built ([`../../.github/workflows/ci.yml`](../../../.github/workflows/ci.yml) moves CI to repo root) |
| Windows Smart App Control (SAC) | `1` (`Enforced`) at `12:46:29Z`; transitioned externally to `0` (`SAC_PreviousState: 1`, `SAC_EnforcementReason: 6`) prior to `13:32:12Z` | Release binaries built locally remain **unsigned** (Gate F requires Authenticode cert for SAC=`1` targets) |

---

## 2. Rust Workspace, Tauri 2 & Contract Parity Suite (Post-Hardening)

Executed from `desktop-instance-manager/` after final remediations:

- **`cargo fmt --all -- --check`** & **`cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml -- --check`**: **PASS** (exit `0`).
- **`cargo clippy --workspace --all-targets -- -D warnings`** & **`cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml -- -D warnings`**: **PASS** (exit `0`, zero clippy warnings).
- **`cargo test --workspace --all-targets`**: **29/29 PASS** (`dim-core`: `9`, `dim-platform-linux`: `3`, `dim-platform-macos`: `3`, `dim-platform-windows`: `6` incl. real `WinVerifyTrust` & exact VNG RDN pinning, `dim-process-manager`: `2`, `dim-profile-store`: `4` incl. real Windows NTFS junction unlinking, `dim-test-fixture`: `2`).
- **`cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml`**: Compiled and passed cleanly (`0` unit tests defined in the Tauri binary crate).
- **Release Binaries Built:** `target/release/dim-cli.exe` (`2,210,406` bytes), `target/release/dim-test-fixture.exe` (`1,616,934` bytes), `apps/desktop/src-tauri/target/release/dim-desktop.exe` (`21,577,450` bytes).
- **`node tests/run-contract-parity-tests.mjs --out evidence/contract-parity-result.json`**: **PASS** (`ok: true`, `executed_at_utc: 2026-11-17T15:30:53.306Z`, `powershell_parity_tested: true`). Verified identical outputs across Rust `dim-cli.exe` and PowerShell `dim-host.ps1` for `discover` (tampered PE reports `valid: false` and `WinVerifyTrust=0xC000A000`), `UNVERIFIED_BINARY` launch refusal, Vietnamese slugification, focus-while-stopped `NOT_RUNNING`, 7/7 `verify-isolation` `PASS`, `diagnostics`, backup/restore SHA-256 tamper rejection, NTFS junction preservation, and all error guards.
- **`node scripts/generate-build-provenance.mjs`**: **PASS** (`artifacts_count: 3`, `sbom_components: 439`, `dep_audit_passed: true`, covering all `41` workspace + `434` Tauri `Cargo.lock` entries with `0` policy violations).

---

## 3. Fixture & Security Integration Suite (`8/8 PASS` Post-Hardening)

Executed from `desktop-instance-manager/`:
```powershell
node tests/run-fixture-tests.mjs --out evidence/fixture-test-result.json
```

**Run Summary:** `ok = true`, `executed_at_utc = 2026-11-17T15:38:11.589Z`, `total_tests = 8`, `passed_tests = 8`, `failed_tests = 0`, exit code `0`.

| Test ID | Suite Name | Status | Duration (ms) | Key Observed Evidence |
|---|---|---|---:|---|
| `Test-1` | Profile CRUD, Schema & Vietnamese Slugification | **PASS** | 11,555 | Generated profiles `["75e1abd9", "9f6a3dfa", "5f39d6bf"]`, slug dir `duc-cong-viec_75e1abd9` |
| `Test-2` | Concurrent Multi-Instance Launch (2 then 3) & Focus | **PASS** | 17,886 | Launched 3 distinct concurrent PIDs `[6408, 1912, 13784]`, focus verified (`focus_count: 1`) |
| `Test-3` | Single-Instance Lock Collision (Exit Code 2) | **PASS** | 11,524 | Duplicate launch against running profile returned `collision_exit_code: 2`, `primary_pid_preserved: 6408` |
| `Test-4` | 10 Consecutive Graceful Close/Reopen Cycles | **PASS** | 72,981 | `cycles_completed: 10`, `clean_shutdown: true` |
| `Test-5` | Crash Recovery & Stale Lockfile Cleanup | **PASS** | 14,453 | Forced crash left stale `lockfile`; relaunch detected & unlinked stale lock (`crash_recovered: true`, `launch_count: 2`) |
| `Test-6` | Duplicate `zalo_user_id` Guard | **PASS** | 15,574 | Second profile sharing logged-in user ID blocked (`blocked_error_code: "DUPLICATE_ZALO_USER_ID"`) |
| `Test-7` | Offline Atomic Backup & Non-Destructive Restore | **PASS** | 12,646 | Backed up `9` files atomically (`.partial` -> `backup.json` SHA-256) and restored cleanly into new profile `76c6e714` |
| `Test-8` | Security Guards, Sanitized Logs, Modern Web Guidance & UI Server | **PASS** | 46,574 | Rejected stop on live primary Zalo (`"verified_pid_12980"`), blocked primary `ZaloData` and nested paths, rejected tampered PE (`"Unverified (WinVerifyTrust=0xC000A000)"`, `valid: false`), enforced loopback UI server auth (`401 UNAUTHORIZED`, `403 FORBIDDEN_DEST`, `403 FORBIDDEN_HOST`, `403 FORBIDDEN_ORIGIN`, `415 UNSUPPORTED_MEDIA_TYPE`, `ui_server_port: 60391`) + MWG (`dialog`, `responsive-table`, `dark-mode`, `tabs` with `role="tablist"`) |

---

## 4. Live Official Zalo Desktop `26.10.20` PoC (`run-zalo-poc.ps1`)

Executed with primary personal Zalo Desktop actively running:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run-zalo-poc.ps1
```

**Run Summary (`evidence/zalo-live-poc-result.json`):** `executed_at_utc = 2026-11-17T13:32:12.762Z`, command exit code `0`.

### Concurrency, Latency & Resource Measurements (3 Isolated Zalo `26.10.20` Windows + Primary Running)

| Profile | Profile ID | Main PID | Process Count | HWND | Startup Latency (ms) | Working Set (MB) | Private Memory (MB) | Disk Footprint (MB) | CPU (s) |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| `PoC Alpha` | `f6c41071` | 24868 | 9 | 5768750 | 3,437 | 1,108.05 | 653.98 | 32.43 | 20.39 |
| `PoC Beta` | `74a8bb74` | 24312 | 9 | 919440 | 3,832 | 1,061.85 | 626.84 | 20.30 | 20.20 |
| `PoC Gamma` | `7e42562c` | 24656 | 9 | 4327148 | 4,034 | 1,197.27 | 784.45 | 9.95 | 18.45 |
| **3-Instance Subtotal** | — | — | **27** | — | — | **3,367.17** | **2,065.27** | **62.68** | **59.04** |

- **Additional Lifecycle Timings:** `two_concurrent_ok = true`, `three_concurrent_ok = true`, `focus_ok = true`, `rename_while_running_ok = true`, `selective_stop_ok = true` (`3,038 ms`), `relaunch_ok = true` (`beta_relaunch = 4,144 ms`).
- **Crash Recovery Note:** `crash_recovery_ok = true` (`gamma_crash_recover = 4,243 ms` under a new PID), while `crash_recovered_flag = false` (Chromium's `FILE_FLAG_DELETE_ON_CLOSE` lockfile handle cleans up at process kill rather than leaving a persistent orphan `lockfile`; `Test-5` proved persistent stale `lockfile` cleanup on the fixture).
- **L1–L5 Live Zalo Classification:**
  - `L1` (Process & Storage Isolation): **PASS**
  - `L2` (Lifecycle & Binary Integrity): **PASS** (`official_binary_sha256_unchanged = true`, `%APPDATA%\ZaloData` pre/post SHA-256 identical)
  - `L3` (Authenticated Multi-Account Chat / File / Sync): **NOT_TESTED** (requires manual QR sign-in via [`scripts/run-interactive-l3-checklist.ps1`](../../scripts/run-interactive-l3-checklist.ps1))
  - `L4` (Per-Profile Notifications & Concurrent Voice/Video Calls): **BLOCKED** (`AppUserModelId("com.vng.zalo")` & `\\.\pipe\PipeZCallRecv`/`PipeZCallSend`)
  - `L5` (Installer & Zalo Auto-Upgrade Resilience): **PARTIAL** (binary invariance verified; MSI/NSIS installer & auto-update soak not tested)

### Primary Zalo Integrity Verification (`%APPDATA%\ZaloData`)
`primary_process_preserved = true` (`PID 12980` and all `9` baseline primary PIDs untouched). Identical SHA-256 pre- and post-PoC across `config.json`, `startup.log`, `log.log`, and `migrate-config.json`.

---

## 5. Honest Unexecuted & Blocked Checks

- **GitHub Actions Remote Execution:** `.github/workflows/ci.yml` is relocated to repository root and valid statically, **however** `gh auth status` scopes (`gist`, `read:org`, `repo`) lack `workflow` scope (`0` workflows currently registered on `origin`). Remote CI execution is **BLOCKED** until pushed with `workflow` scope.
- **Tauri GUI Interactive Smoke:** `dim-desktop.exe` compiles cleanly in release mode (`apps/desktop/src-tauri/target/release/dim-desktop.exe`), but interactive GUI launch testing was not executed and binaries remain unsigned (**Gate F BLOCKED**).
- **macOS & Linux Hardware Tests:** `dim-platform-macos` (`NOT_TESTED`) and `dim-platform-linux` (`UNSUPPORTED` for official Zalo / `EXPERIMENTAL` `NOT_TESTED` for local AppImage) were verified only via compilation and unit tests on Windows (`crates/platform-macos` & `crates/platform-linux`).

---

## 6. Quality & Release Gates A–F Verdicts

| Gate | Name | Verdict | Justification |
|---|---|---|---|
| **Gate A** | Technical Feasibility | **PASS** | Hybrid env/storage isolation verified with 3 concurrent Zalo `26.10.20` windows + primary, native Rust `dim-cli.exe`, and native Tauri 2 `dim-desktop.exe`. |
| **Gate B** | Data Safety & Non-Interference | **PASS** | `29/29` Rust tests + `8/8` fixture tests + Rust/PS parity suite + `%APPDATA%\ZaloData` SHA-256 invariance + `PROTECTED_PRIMARY_PROCESS` (`PID 12980`) + Windows NTFS junction unlinking. |
| **Gate C** | Functional Compatibility | **PARTIAL** | L1/L2 **PASS**; L3 **NOT_TESTED** (guided QR script ready); L4 **BLOCKED** (`PipeZCall*` & `com.vng.zalo` AUMID); L5 **PARTIAL**. |
| **Gate D** | Cross-Platform Readiness | **PASS (Honest Matrix)** | Windows Core Isolation **SUPPORTED_AND_TESTED**; macOS **NOT_TESTED**; Linux Official Zalo **UNSUPPORTED**; Linux Fixture/AppImage **EXPERIMENTAL** (`NOT_TESTED` on Linux host). |
| **Gate E** | Security & Compliance | **PASS** | All Phase 10 `code-reviewer` and `tester` Critical/High/Medium findings remediated and re-verified across Rust, Tauri, Web UI, PowerShell, and `439`-package dependency/SPDX provenance audit. See [`threat-model-and-audit.md`](../security/threat-model-and-audit.md). |
| **Gate F** | Release Readiness | **BLOCKED (Public Unsigned PE) / READY FOR LOCAL EVAL** | Locally compiled `.exe` binaries remain unsigned (`IMAGE_DIRECTORY_ENTRY_SECURITY` absent) and remote CI push is blocked pending `workflow` OAuth scope; local evaluation via Signed-Host Bridge ([`scripts/dim-host.ps1`](../../scripts/dim-host.ps1) + [`apps/desktop/ui-server.mjs`](../../apps/desktop/ui-server.mjs)) or local `dim-desktop.exe` build is ready. |
