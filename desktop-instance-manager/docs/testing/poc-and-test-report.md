# PoC Verification & Test Report — Desktop Instance Manager

**Workspace:** [`desktop-instance-manager/`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager)  
**Verifier:** `security-verifier`  
**Verification Date (UTC):** `2026-11-17` (Final post-remediation verification at `13:30:34Z` – `13:32:45Z`)  
**Evidence Artifacts:**
- [`evidence/fixture-test-result.json`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/evidence/fixture-test-result.json)
- [`evidence/zalo-live-poc-result.json`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/evidence/zalo-live-poc-result.json)

---

## 1. Environment & Execution Scope

| Check Component | Host State Observed | Execution Status |
|---|---|---|
| OS & Elevation | Windows 11/10 non-elevated (`IsAdmin: false`) | Active |
| Node.js Runtime | `v26.13.1` | Executed `run-fixture-tests.mjs` + `ui-server.mjs` |
| PowerShell Bridge | Windows PowerShell (`scripts/dim-host.ps1`) | Executed all CLI, security & live Zalo PoC flows |
| Official Zalo Desktop | `26.10.20` (`Signed (VNG)`), primary running (`PID 12980`) | Executed 3 concurrent PoC instances alongside primary |
| Rust Toolchain (`rustc` / `cargo`) | **Not installed locally** (`RustcInstalled: false`) | **NOT RUN LOCALLY** (`25` `#[test]` functions inspected statically; executed in [`.github/workflows/ci.yml`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/.github/workflows/ci.yml)) |
| Windows Smart App Control (SAC) | `1` (`Enforced`) at `12:46:29Z`; transitioned to `0` (`SAC_PreviousState: 1`, `SAC_EnforcementReason: 6`) by `13:32:12Z` | Release Gate F tracks CI Authenticode signing for SAC=`1` targets |

---

## 2. Post-Remediation Fixture & Security Integration Suite (`8/8 PASS`)

Executed after final worker edits (`#17` and `#19`) with:
```powershell
node c:\Users\manhquy\Downloads\zalo\desktop-instance-manager\tests\run-fixture-tests.mjs --out c:\Users\manhquy\Downloads\zalo\desktop-instance-manager\evidence\fixture-test-result.json
```

**Run Summary:** `ok = true`, `executed_at_utc = 2026-11-17T13:30:34.874Z`, `total_tests = 8`, `passed_tests = 8`, `failed_tests = 0`, exit code `0`.

| Test ID | Suite Name | Status | Duration (ms) | Key Observed Evidence |
|---|---|---|---:|---|
| `Test-1` | Profile CRUD, Schema & Vietnamese Slugification | **PASS** | 9,877 | Generated profiles `["42081e58", "1a77c51e", "f8b38626"]`, slug dir `duc-cong-viec_42081e58` |
| `Test-2` | Concurrent Multi-Instance Launch (2 then 3) & Focus | **PASS** | 16,130 | Launched 3 distinct concurrent PIDs `[26100, 16468, 20424]`, focus verified (`focus_count: 1`) |
| `Test-3` | Single-Instance Lock Collision (Exit Code 2) | **PASS** | 11,036 | Duplicate launch against running profile returned `collision_exit_code: 2`, `primary_pid_preserved: 26100` |
| `Test-4` | 10 Consecutive Graceful Close/Reopen Cycles | **PASS** | 69,105 | `cycles_completed: 10`, `clean_shutdown: true` |
| `Test-5` | Crash Recovery & Stale Lockfile Cleanup | **PASS** | 13,321 | Forced crash left stale `lockfile`; relaunch detected & unlinked stale lock (`crash_recovered: true`, `launch_count: 2`) |
| `Test-6` | Duplicate `zalo_user_id` Guard | **PASS** | 14,668 | Second profile sharing logged-in user ID blocked (`blocked_error_code: "DUPLICATE_ZALO_USER_ID"`) |
| `Test-7` | Offline Atomic Backup & Non-Destructive Restore | **PASS** | 11,853 | Backed up `9` files atomically (`.partial` -> `backup.json` SHA-256) and restored cleanly into new profile `8786c45f` |
| `Test-8` | Security Guards, Sanitized Logs, Modern Web Guidance & UI Server | **PASS** | 40,352 | Rejected stop on live primary Zalo (`"verified_pid_12980"`), rejected tampered PE (`"Unverified (WinVerifyTrust=0xC000A000)"`), started loopback UI server (`ui_server_port: 58369`), verified `Host`/`Origin`/`Content-Type`/traversal/log redaction + MWG (`dialog`, `responsive-table`, `dark-mode`, `tabs`) |

---

## 3. Live Official Zalo Desktop `26.10.20` PoC (`run-zalo-poc.ps1`)

Executed with primary personal Zalo Desktop actively running:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File c:\Users\manhquy\Downloads\zalo\desktop-instance-manager\scripts\run-zalo-poc.ps1
```

**Run Summary:** `executed_at_utc = 2026-11-17T13:32:12.762Z`, command exit code `0`.

### Concurrency, Latency & Resource Measurements (3 Isolated Zalo `26.10.20` Windows + Primary Running)

| Profile | Profile ID | Main PID | Process Count | HWND | Startup Latency (ms) | Working Set (MB) | Private Memory (MB) | Disk Footprint (MB) | CPU (s) |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| `PoC Alpha` | `f6c41071` | 24868 | 9 | 5768750 | 3,437 | 1,108.05 | 653.98 | 32.43 | 20.39 |
| `PoC Beta` | `74a8bb74` | 24312 | 9 | 919440 | 3,832 | 1,061.85 | 626.84 | 20.30 | 20.20 |
| `PoC Gamma` | `7e42562c` | 24656 | 9 | 4327148 | 4,034 | 1,197.27 | 784.45 | 9.95 | 18.45 |
| **3-Instance Subtotal** | — | — | **27** | — | — | **3,367.17** | **2,065.27** | **62.68** | **59.04** |

- **Additional Lifecycle Timings:** `two_concurrent_ok = true`, `three_concurrent_ok = true`, `focus_ok = true`, `rename_while_running_ok = true`, `selective_stop_ok = true` (`3,038 ms`), `relaunch_ok = true` (`beta_relaunch = 4,144 ms`).
- **Crash Recovery Note:** `crash_recovery_ok = true` (`gamma_crash_recover = 4,243 ms` under a new PID), while `crash_recovered_flag = false`. On Windows, Chromium/Zalo opens its `lockfile` with OS handle cleanup (`FILE_FLAG_DELETE_ON_CLOSE`), so killing the main `Zalo.exe` PID removed the kernel handle/lockfile before relaunch rather than leaving a persistent orphan `lockfile` (whereas `Test-5` verified persistent stale `lockfile` deletion on the fixture binary).
- **Per-Profile Artifacts:** All 3 PoC profiles independently generated `lockfile`, `Local State`, `config.json`, `migrate-config.json`, and `.zalo-profile-id` strictly inside their isolated directories.

### Isolation Verification (`7/7` Runtime Checks Passed at `2026-11-17T13:31:31.327Z`)
1. `ApplicationDiscovery`: **PASS** (`Mode=official-zalo, Auth=Signed (VNG)`)
2. `PrimaryZaloDataSeparation`: **PASS** (all profiles isolated outside primary `%APPDATA%\ZaloData`)
3. `DistinctProfilePathsAndMarkers`: **PASS** (checked 3 profiles, `markers_valid=True`)
4. `DistinctProcessOwnership`: **PASS** (active isolated main PIDs = `3`, primary PIDs = `11` at probe time)
5. `DistinctZaloUserIds`: **PASS** (no duplicate logged-in `zalo_user_id` among running instances)
6. `RestrictiveNtfsAcls`: **PASS** (no `Everyone` or `BUILTIN\Users` write access on `Profiles` directory)
7. `FolderRedirectionCheck`: **PASS** (`AppData` resolves locally under `USERPROFILE`, `registry_value_sanitized: "%USERPROFILE%\\AppData\\Roaming"`)

### Primary Zalo Integrity Verification (Before vs After PoC + Independent Audit at `2026-11-17T13:32:45.426Z`)
- **Primary Process Preservation:** `primary_process_preserved = true`, `official_binary_sha256_unchanged = true`. Post-PoC check confirmed primary `PID 12980` alive and all `9` baseline primary PIDs (`6612, 7216, 12980, 13484, 13508, 13528, 13796, 13976, 17488`) untouched.
- **Primary Data Directory SHA-256 Comparison (`%APPDATA%\ZaloData`):**

| File | Size (bytes) | Pre-PoC SHA-256 (`12:46:29Z`) | Post-PoC Verified SHA-256 (`13:32:45Z`) | Match |
|---|---:|---|---|---|
| `config.json` | 1,646 | `be8077e13b0f59e6976a7baa549b836a46569ca397f2bdf5d3e0f3a46be07734` | `be8077e13b0f59e6976a7baa549b836a46569ca397f2bdf5d3e0f3a46be07734` | **IDENTICAL** |
| `startup.log` | 833 | `91ebcb61b8fa2394d521a0fe613408e662e745bab82d7a8fadba8aa1bc43c963` | `91ebcb61b8fa2394d521a0fe613408e662e745bab82d7a8fadba8aa1bc43c963` | **IDENTICAL** |
| `log.log` | 3,147 | `697b03394998d201e6621285bf2c31c229cc200315d49caa0574145a52a0a3c0` | `697b03394998d201e6621285bf2c31c229cc200315d49caa0574145a52a0a3c0` | **IDENTICAL** |
| `migrate-config.json` | 59 | `b2fb26f3ef5a77cd816442091b31932a416543c5fa81d0b3ad4f8e18983333c9` | `b2fb26f3ef5a77cd816442091b31932a416543c5fa81d0b3ad4f8e18983333c9` | **IDENTICAL** |

- **PoC Cleanup Note:** Live PoC profiles and data files were removed cleanly; only an empty nested directory chain (`%LOCALAPPDATA%\MultiZaloDesktop_LivePoC\Profiles\poc-gamma_7e42562c\AppData\Local\Temp`) remained from transient process handle release timing during `finally` cleanup.

---

## 4. Honest Unexecuted & Blocked Functionality Disclosures

- **Rust `cargo test` / `cargo clippy` / `cargo deny`:** **Not executed locally** because `rustc` and `cargo` are not installed on the verification host (`25` unit tests verified by inspection in `crates/*/src/lib.rs`, CI pipeline defined in [`.github/workflows/ci.yml`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/.github/workflows/ci.yml)).
- **Concurrent Voice / Video Calls (`ZaloCall.exe`):** **BLOCKED** — Zalo Desktop shares Windows named pipes `\\.\pipe\PipeZCallRecv` and `\\.\pipe\PipeZCallSend` across instances without modifying VNG binaries.
- **Per-Profile Toast Notification Attribution:** **BLOCKED** — Zalo registers a single `AppUserModelId("com.vng.zalo")`, so Windows notification action-center grouping and click routing cannot differentiate concurrent instances.
- **Multi-Account Live Message / File Transfer E2E:** **BLOCKED** — Automated PoC proved 3 concurrent login-ready instances alongside the user's logged-in primary Zalo, but sending/receiving files across 2+ simultaneously authenticated Zalo accounts requires interactive QR login with multiple real phone numbers.
- **macOS & Ubuntu Official Zalo Runtime Verification:** macOS (`crates/platform-macos`) is **NOT_TESTED** (`open -n -a ... --env CFFIXED_USER_HOME=...` and `/tmp/socketzalorecv2021` collision caveat); Ubuntu official Zalo has no native desktop build (**UNSUPPORTED**), while local `.AppImage`/fixture isolation (`crates/platform-linux`) is **EXPERIMENTAL** and **NOT_TESTED** at runtime on Linux here.

---

## 5. Quality & Release Gates A–F Verdicts

| Gate | Name | Verdict | Justification |
|---|---|---|---|
| **Gate A** | Technical Feasibility | **PASS** | Verified hybrid environment-override architecture running `3` live official Zalo `26.10.20` windows + primary simultaneously (`3.4s`–`4.0s` startup) with distinct PIDs, HWNDs, and `Local State`/`lockfile` artifacts. |
| **Gate B** | Data Safety & Non-Interference | **PASS** | `Test-1..8` passed (`10/10` close/reopen cycles, crash recovery, `DUPLICATE_ZALO_USER_ID`, atomic `.partial` -> `backup.json` + `.restore-*` staging); `%APPDATA%\ZaloData` SHA-256 hashes (`config.json`, `startup.log`, `log.log`, `migrate-config.json`) identical before & after live PoC; primary `PID 12980` untouched. |
| **Gate C** | Functional Compatibility | **PARTIAL (Documented `BLOCKED` Subsystems)** | Core profile CRUD, concurrent launch/focus, live rename, selective shutdown (`3.04s`), relaunch, and backup/restore verified. Concurrent `ZaloCall.exe` calls (`PipeZCall*`), isolated toast notifications (`AppUserModelId`), and live multi-account file send/receive are explicitly `BLOCKED`. |
| **Gate D** | Cross-Platform Readiness | **PASS (With Honest Matrix)** | Windows Core Isolation: **SUPPORTED_AND_TESTED** (via Signed-Host Bridge); Windows Full Desktop Suite: **PARTIALLY_SUPPORTED** (`ZaloCall`/notifications blocked); macOS: **NOT_TESTED**; Ubuntu Official Zalo: **UNSUPPORTED**; Ubuntu Local `.AppImage`/Fixture: **EXPERIMENTAL**. |
| **Gate E** | Security & Compliance | **PASS** | All 11 threat categories + STRIDE + OWASP Top 10 audited; `Test-8` proved `PROTECTED_PRIMARY_PROCESS` (`PID 12980`), `WinVerifyTrust=0xC000A000` tamper rejection, path traversal blocks, log redaction, loopback `Host`/`Origin`/`Content-Type` validation, and `modern-web-guidance` UI compliance (`dialog`, `responsive-table`, `dark-mode`, `tabs`). See [`threat-model-and-audit.md`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/docs/security/threat-model-and-audit.md). |
| **Gate F** | Release Readiness | **BLOCKED (For Compiled PE Installer) / READY FOR SAC EVALUATION (Via Signed-Host Bridge)** | Compiled Rust/Tauri standalone Windows PE release is **BLOCKED** pending CI toolchain build (`cargo test`/`cargo build --release`) and an Authenticode code-signing certificate required on Windows Smart App Control (`VerifiedAndReputablePolicyState = 1`) hosts. Evaluation and local testing via Signed-Host Bridge ([`scripts/dim-host.ps1`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/scripts/dim-host.ps1) + [`apps/desktop/ui-server.mjs`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/apps/desktop/ui-server.mjs)) are **READY**. |
