# Windows Product Acceptance Verdict & Release Captain Handoff Report

- **Product:** Multi-Zalo Desktop — Instance Manager (`v0.1.0-beta.1` Candidate)
- **Mission:** Agent Mission G — Windows Native User Acceptance & Installation Lab
- **QA Worktree / Branch:** `<WORKTREE_ROOT>/zalo-windows-acceptance` (`qa/windows-native-acceptance`)
- **Tested Integration Commit SHA:** `cd62c523222cb3a02051347e85313226de8b6964` (`integration/reliability-hardening`, clean `0`-dirty handoff commit advancing `004ab7c` via `bff0674` + `cd62c52`)
- **Candidate Workflow Markers:** `AWAITING_INTEGRATION_BUILD` · `AWAITING_MANUAL_L3` · `UNSIGNED_DEVELOPMENT_ARTIFACT`
- **Product Acceptance Verdict:** **`BLOCKED`** *(Strictly selected from `{READY_FOR_INTERNAL_TESTING, CHANGES_REQUESTED, BLOCKED, READY_FOR_BETA}` — **not** `READY_FOR_BETA`)*

---

## 1. Executive Verdict & Justification

### Product Acceptance Verdict: **`BLOCKED`**

Central Integration has now completed a clean commit **`cd62c523222cb3a02051347e85313226de8b6964`** (`0` dirty files in `<WORKTREE_ROOT>/zalo-integration`, closing `BLK-01` as well as `IR-QA-UX-03` / `IR-NAT-01` `recoveryRequired` UI states and `IR-NAT-02` `DIM_NODE_FIXTURE_SCRIPT`). Against `cd62c52`, **100% of executed automated runtime, Account Dock UX, native Tauri WebView2 smoke, and host-safety checks passed (`43 PASS`, `0 FAIL`)** with zero interference on the user's running Primary Zalo (`11` live `Zalo*` processes and `%APPDATA%\ZaloData\config.json` SHA-256 `be8077e1...` unchanged).

However, end-to-end Windows release acceptance remains honestly classified as **`BLOCKED`** for four concrete reasons:

1. **Official Windows Installer Not Built Yet (`AWAITING_INTEGRATION_BUILD` — `BLK-02`, `BLK-03`, `BLK-04`):**
   - Zero NSIS (`*-setup.exe`) or WiX (`*.msi`) installer bundles (`is_installer: true`) exist yet because GitHub CLI lacks `workflow` scope (`BLK-03`), `windows-2022` MSVC CI has not yet run (`BLK-04`), and Release Captain has not yet completed post-handoff C2 regression intake of `cd62c52` (`BLK-02`). Rebuilt raw `dim-desktop.exe` (`is_installer: false`) is never conflated with an installer, leaving `6` installer lifecycle checks (`INST-01`, `INST-08..12`) **`NOT_TESTED`** and `2` packaging/signing checks (`INST-03`, `INST-06`) **`BLOCKED`**.
2. **Human 2-Account Real Zalo QR Gate Unexecuted (`AWAITING_MANUAL_L3` — `BLK-05`):**
   - Automated QR/OTP/cookie manipulation is strictly prohibited. All `8` real-account multi-session checks (`L3-ACC-01..08` in `l3-manual-test-checklist.md`) remain **`NOT_TESTED`** (`AWAITING_MANUAL_L3`) until a human tester completes `run-l3-guided-acceptance.ps1` with 2 authorized Zalo accounts.
3. **Unsigned Artifacts & Smart App Control Policy (`BLOCKED_BY_PLATFORM_POLICY` — `BLK-06`):**
   - Compiled binaries carry `UNSIGNED_DEVELOPMENT_ARTIFACT` (`Authenticode = NotSigned`). Windows 11 endpoints with Smart App Control enforced (`VerifiedAndReputablePolicyState = 1`) block unsigned PE installers/executables (`SAC` was never disabled; users on `SAC=1` hosts must use the Signed-Host Bridge `ui-server.mjs` + `dim-host.ps1` until Gate G3 signing is provisioned).
4. **Open Non-Blocking Product Error-Recovery UX & Parity Requests (`IR-QA-UX-01, 02, 04..08`):**
   - While `cd62c52` closed `IR-QA-UX-03` (`IR-NAT-01`) and `IR-NAT-02`, our non-technical UX audit recorded `2` open `P1`, `2` open `P2`, and `3` `P3` requests in [`integration-requests.md`](./integration-requests.md) for pre-beta polish (pre-launch corrupted `profiles.json` panic handling in `main.rs:9`, Tauri read-command error code preservation & `JSON_ERROR` copy, tampered-backup restore error copy, and PS1 NTFS ACL `IO_ERROR` parity).

> **Immediate Internal Testing Guidance:** Although full release candidate sign-off is **`BLOCKED`** pending the `windows-2022` NSIS/MSI installer build, the compiled `cd62c52` runtime (`dim-desktop.exe` SHA-256 `2168cacf...` on `SAC=0` QA machines and `ui-server.mjs` Signed-Host Bridge) is **verified safe (`43/43` executed checks `PASS`, `0` data-safety defects, `100%` Primary Zalo invariance)** for the project owner to execute interactive `run-l3-guided-acceptance.ps1` right away, and will advance to **`READY_FOR_INTERNAL_TESTING`** (`INTERNAL_TEST_BUILD`) as soon as Release Captain builds the `windows-2022` NSIS/MSI installer from `cd62c52` and passes `installer-sandbox-check.ps1`.

---

## 2. Worktree, HEAD & Candidate Provenance Snapshot (`cd62c52`)

| Worktree / Artifact | Branch / Kind | Observed Ref / SHA-256 | Read/Write Role in Mission G |
| :--- | :--- | :--- | :--- |
| `<WORKTREE_ROOT>/zalo-windows-acceptance` | `qa/windows-native-acceptance` | `cd62c523222cb3a02051347e85313226de8b6964` | **Owned QA Worktree** — strictly `tests/windows-acceptance/**` and `docs/qa/windows/**` (`0` product code or workflow edits). |
| `<WORKTREE_ROOT>/zalo` | `main` | `a5feb4a520798064c1508c26fe4c4ac17a3a03cf` | **Read-Only** (`git --no-optional-locks`) — Release Captain `docs/release/*.md` untouched. |
| `<WORKTREE_ROOT>/zalo-integration` | `integration/reliability-hardening` | `cd62c523222cb3a02051347e85313226de8b6964` (`0` dirty files) | **Read-Only** (`git --no-optional-locks`) — `0` touches; clean handoff state verified (`BLK-01` resolved). |
| `<TEMP_DIR>/zalo-qa-win-acceptance-target/release/dim-desktop.exe` | `compiled-executable` (`is_installer: false`) | `22,672,392` B · SHA-256 `2168cacfba423a9f7012b46040b3998d96cad24bd754b3464d13996bb3e1376d` | Compiled from `cd62c52` (`stable-x86_64-pc-windows-gnu`, `rustc 1.100.0`, `--locked`, cold HWND + DOM `1,738.92 ms`) outside worktree. |
| `<TEMP_DIR>/zalo-qa-win-acceptance-target/release/dim-cli.exe` | `compiled-executable` (`is_installer: false`) | `2,567,980` B · SHA-256 `6d741ddc3fa5cbfe89b4c9c0cbb19a18fbc4597d7cc52cabcf6aec5026f89008` | Compiled from `cd62c52` (`284 ms` cold `--cli --json status`). |
| `<TEMP_DIR>/zalo-qa-win-acceptance-target/release/dim-test-fixture.exe` | `compiled-executable` (`is_installer: false`) | `1,750,426` B · SHA-256 `890c601a7b2b279c00b1841d90efe124f192d004ac9a59c0ca6b9ea78f44fe87` | Companion synthetic isolation binary. |
| Official NSIS / WiX Windows Installer | `installer-nsis` / `installer-msi` (`is_installer: true`) | **`NOT_BUILT_YET`** (`0` files) | `AWAITING_INTEGRATION_BUILD` (`BLK-04` — requires `windows-2022` CI MSVC + NSIS/WiX). |

---

## 3. Unified Acceptance Gate Rollup (`evidence/acceptance-summary.json`)

| Track | Scope | Harness Script | `PASS` | `FAIL` | `BLOCKED` | `NOT_TESTED` | Track Gate | Workflow Marker |
| :---: | :--- | :--- | :---: | :---: | :---: | :---: | :---: | :--- |
| **1** | §2 First Launch (3), Account Mgmt (10), Daily Usage (6), Error UX (7) | `journeys-and-errors.mjs` | `26` | `0` | `0` | `0` | **`PASS`** | `VERIFIED_SANDBOX_FIXTURE` |
| **2** | §3 Native Tauri WebView2 vs Browser Mock & Modern Web Guidance (12) | `native-tauri-smoke.mjs` | `11` | `0` | `0` | `1` | **`PASS`** | `VERIFIED_NATIVE_TAURI` (`NT-07` tray not in MVP) |
| **3** | §4 Installer Readiness, Authenticode, SAC & Lifecycle (14) | `installer-sandbox-check.ps1` | `6` | `0` | `2` | `6` | **`NOT_TESTED`** | `AWAITING_INTEGRATION_BUILD` |
| **4** | §5 L3 Real 2-Account Zalo QR Guided Gate (8) | `run-l3-guided-acceptance.ps1 -NonInteractive` | `0` | `0` | `0` | `8` | **`NOT_TESTED`** | `AWAITING_MANUAL_L3` |
| **Total** | **Sections 2 – 5 Combined (`60` Checks)** | **`run-acceptance-suite.mjs`** | **`43`** | **`0`** | **`2`** | **`15`** | **`BLOCKED`** | `AWAITING_INTEGRATION_BUILD` · `AWAITING_MANUAL_L3` |
| **Host Safety** | §6 Primary Zalo Invariance, Zero Orphans, SAC Integrity, PII Scrubbing | `run-acceptance-suite.mjs` | **`6`** | **`0`** | **`0`** | **`0`** | **`PASS`** | `VERIFIED_HOST_INVARIANCE` |

---

## 4. Categorized Findings: Release Blockers, Integration Requests & Vendor Limits

### 4.1. Release & Packaging Blockers

| ID | Category | Current State at `cd62c52` | Required Resolution |
| :--- | :--- | :--- | :--- |
| **`BLK-01`** | Central Integration Clean Handoff | **RESOLVED (`cd62c52`, `0` dirty files)** | Release Captain intakes `cd62c523222cb3a02051347e85313226de8b6964`. |
| **`BLK-02`** | Delta C2 Regression Pass on Final SHA | Pending Release Captain intake | Run 1 final C2 `xf-test-runner --mode release-regression --suite all --cycles 100` pass on `cd62c52`. |
| **`BLK-03` / `BLK-04`** | `windows-2022` CI NSIS/MSI Build | `AWAITING_INTEGRATION_BUILD` (`0` installers) | Refresh `gh auth` with `workflow` scope, merge reviewed PR to `main`, build `x86_64-pc-windows-msvc` NSIS (`*-setup.exe`) + `.msi`, and run `installer-sandbox-check.ps1 -CandidatePath <artifact>`. |
| **`BLK-05`** | Human 2-Account L3 QR Gate | `AWAITING_MANUAL_L3` (`8/8 NOT_TESTED`) | Project owner runs interactive `run-l3-guided-acceptance.ps1` with 2 authorized Zalo accounts (or labels candidate `INTERNAL_TEST_BUILD` until completed). |
| **`BLK-06`** | Authenticode Code Signing (Gate G3) | `UNSIGNED_DEVELOPMENT_ARTIFACT` | Provision OV/EV or Azure Trusted Signing cert before public distribution to `SAC=1` Windows 11 users. |

### 4.2. Product UX, Error-Copy & Parity Integration Requests ([`integration-requests.md`](./integration-requests.md))

| Request ID | Priority | Summary | Status at `cd62c52` |
| :--- | :---: | :--- | :---: |
| **`IR-QA-UX-01`** | **`P1`** | Replace `AppState::initialize().expect(...)` in `src-tauri/src/main.rs:9` with degraded recovery UI if `profiles.json` was corrupted prior to Tauri launch. | **OPEN** *(Blocks Beta)* |
| **`IR-QA-UX-02`** | **`P1`** | Preserve `error_code` on Tauri read commands (`get_status`, `get_diagnostics`, `verify_isolation`), update `JSON_ERROR` copy on corrupt `profiles.json`, and alias `PATH_TRAVERSAL_REJECTED`. | **OPEN** *(Blocks Beta)* |
| **`IR-QA-UX-03`** (`IR-NAT-01`) | **`P1`** | Render `lifecycle_state: "recovery_required"` (`"Cần mở lại để khôi phục"` / `"Reopen to recover"`) and disable Backup/Delete when unclean. | **VERIFIED CLOSED** |
| **`IR-QA-UX-04`** | **`P2`** | Map `verify_backup` failures inside `restore_backup_as_new_profile` to `RESTORE_ERROR` / `CORRUPT_BACKUP` instead of `BACKUP_ERROR` ("Cannot back up while Zalo is open"). | **OPEN** |
| **`IR-QA-UX-05`** | **`P3`** | Persist optional `color` badge on `ProfileRecord` (`profiles.json` + `backup.json`) across Bridge/Tauri origins and restores. | **OPEN** |
| **`IR-QA-UX-06`** (`IR-NAT-02`) | **`P3`** | `DIM_NODE_FIXTURE_SCRIPT` Tauri hook (**CLOSED**); remaining copy polish (`5 GB` disk message, `profiles_acl_safe` row, localized OS matrix limits, retry button). | **Hook CLOSED / Copy OPEN** |
| **`IR-QA-UX-07`** | **`P2`** | Map `UnauthorizedAccessException` to `IO_ERROR` in `scripts/dim-host.ps1` (matching Rust `dim-cli.exe`). | **OPEN** |
| **`IR-QA-UX-08`** (`IR-NAT-03`) | **`P3`** | Use browser-inherited `rem` root font sizing + `<meta name="text-scale" content="scale">` in `styles.css` / `index.html`. | **OPEN** |

### 4.3. Permanent Upstream Vendor Boundaries (Disclosed Honestly — Non-Defects)

- **Concurrent Voice/Video Calls (`\\.\pipe\PipeZCallRecv` / `PipeZCallSend`):** Permanently **`BLOCKED`** for simultaneous calls across 2 Windows accounts without forbidden binary modification; sequential single-account call handoff works and is disclosed in `#view-help` and `windows-mvp-guide.md`.
- **Windows Action Center Toast Grouping (`com.vng.zalo`):** **`PARTIALLY_SUPPORTED`**; official Zalo shares `AppUserModelId("com.vng.zalo")`, so users are guided to enable Zalo's per-instance in-app `notiWin` banner.

---

## 5. Definition-of-Done Assessment & Promotion Roadmap

| Definition-of-Done Gate | Current Verdict | Evidence Artifact |
| :--- | :---: | :--- |
| **1. Zero interference with Primary Zalo (`%APPDATA%\ZaloData`)** | **`PASS`** | `acceptance-summary.json` (`configSha256Identical: true`, `11/11` primary PIDs intact) |
| **2. Non-technical UX clarity (`vi-VN` & `en-US`, zero jargon)** | **`PASS`** *(+ `IR-QA-UX` polish)* | `user-journeys.md`, `journeys-and-errors-report.json` (`26/26 PASS`) |
| **3. Native Tauri WebView2 runtime, accessibility & crash recovery** | **`PASS`** | `native-smoke-test-report.md`, `native-tauri-smoke-report.json` (`11/12 PASS`, `0` fails, `IR-NAT-01/02` closed) |
| **4. Official Windows NSIS/MSI installer install/upgrade/uninstall** | **`NOT_TESTED` (`AWAITING_INTEGRATION_BUILD`)** | `installer-validation.md`, `installer-env-audit.json` |
| **5. Real 2-account Zalo manual QR session & DPAPI persistence** | **`NOT_TESTED` (`AWAITING_MANUAL_L3`)** | `l3-manual-test-checklist.md` |
| **6. Authenticode signing & Smart App Control (`SAC=1`) readiness** | **`BLOCKED` (`UNSIGNED_DEVELOPMENT_ARTIFACT`)** | `installer-validation.md` (`SAC` never disabled; Signed-Host Bridge verified) |

### Step-by-Step Unblock Sequence for Release Captain

1. **Intake Clean `cd62c52` Candidate & Run Final C2 Pass (`BLK-02`):** Verify `cd62c523222cb3a02051347e85313226de8b6964` against C2 `release-regression`.
2. **Build Windows x64 MSVC Installer on CI (`BLK-03`, `BLK-04`):** Refresh `gh auth` with `workflow` scope, run `windows-2022` CI, and execute:
   ```powershell
   node desktop-instance-manager/tests/windows-acceptance/run-acceptance-suite.mjs --candidate-path "<PATH_TO_NSIS_SETUP_EXE>"
   ```
   -> Once `INST-01` & `INST-08..12` pass (`SAC=0` host + `UNSIGNED_DEVELOPMENT_ARTIFACT` disclosure), promote candidate to **`READY_FOR_INTERNAL_TESTING`** (`INTERNAL_TEST_BUILD`).
3. **Execute Guided Human L3 2-Account QR Gate (`BLK-05`):** Run `run-l3-guided-acceptance.ps1` interactively with 2 authorized test accounts.
4. **Resolve `P1` UX Requests (`IR-QA-UX-01`, `02`) & Provision Authenticode Signing (`BLK-06`):** Dual-sign inner binaries + NSIS/MSI installers and re-verify `INST-03` & `INST-04` (`SAC=1` pass) before advancing to **`READY_FOR_BETA`**.
