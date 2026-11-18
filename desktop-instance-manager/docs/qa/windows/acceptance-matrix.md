# Windows End-to-End Product & Installation Acceptance Matrix

- **QA Worktree / Branch:** `<WORKTREE_ROOT>/zalo-windows-acceptance` (`qa/windows-native-acceptance`)
- **Tested Integration Candidate SHA:** `cd62c523222cb3a02051347e85313226de8b6964` (`integration/reliability-hardening`, clean `0`-dirty handoff commit including `bff0674` Account Dock `recoveryRequired` + `DIM_NODE_FIXTURE_SCRIPT` + `cd62c52` remediation ledger)
- **Host OS & Runtime:** `Microsoft Windows 11 Pro 10.0.26300` (`64-bit`, standard user `asInvoker`, WebView2 Evergreen `157.0.4341.46`, Official VNG Zalo `26.10.20` signed `CN=VNG GROUP JSC`)
- **Candidate Workflow State:** `AWAITING_INTEGRATION_BUILD` (`BLK-01` resolved cleanly at `cd62c52`; Release Captain NSIS/MSI installer `NOT_BUILT_YET` under `BLK-03`/`BLK-04`) · `AWAITING_MANUAL_L3` (2-account human QR login unexecuted) · `UNSIGNED_DEVELOPMENT_ARTIFACT`
- **Unified Suite Runner:** `node desktop-instance-manager/tests/windows-acceptance/run-acceptance-suite.mjs` -> `evidence/acceptance-summary.json`

> **Canonical Status Vocabulary:** Every check row uses strictly **`PASS`**, **`FAIL`**, **`BLOCKED`**, or **`NOT_TESTED`** in **Gate Status**, paired with explicit **Workflow / Policy Markers** (`AWAITING_INTEGRATION_BUILD`, `AWAITING_MANUAL_L3`, `BLOCKED_BY_PLATFORM_POLICY`, `UNSIGNED_DEVELOPMENT_ARTIFACT`, `VERIFIED_SANDBOX_FIXTURE`, `VERIFIED_NATIVE_TAURI`) so fixture/sandbox automation is never conflated with installer or live 2-account Zalo verification.

---

## 1. Executive Gate Rollup (§2 – §6 on `cd62c523222cb3a02051347e85313226de8b6964`)

| Section | Track / Evidence Source | Total Checks | `PASS` | `FAIL` | `BLOCKED` | `NOT_TESTED` | Section Gate Status | Primary Workflow Marker |
| :--- | :--- | :---: | :---: | :---: | :---: | :---: | :---: | :--- |
| **§2.1 First Launch** | `journeys-and-errors.mjs` (`FL-01..03`) | `3` | `3` | `0` | `0` | `0` | **`PASS`** | `VERIFIED_SANDBOX_FIXTURE` |
| **§2.2 Account Management (10 Actions)** | `journeys-and-errors.mjs` (`AM-01..10`) | `10` | `10` | `0` | `0` | `0` | **`PASS`** *(Fixture)* | `VERIFIED_SANDBOX_FIXTURE` *(Live QR: `AWAITING_MANUAL_L3`)* |
| **§2.3 Daily Usage (6 Invariants)** | `journeys-and-errors.mjs` (`DU-01..06`) | `6` | `6` | `0` | `0` | `0` | **`PASS`** | `VERIFIED_SANDBOX_FIXTURE` |
| **§2.4 Error Handling (7 Scenarios)** | `journeys-and-errors.mjs` (`ERR-01..07`) | `7` | `7` | `0` | `0` | `0` | **`PASS`** *(Fail-Safe)* | `VERIFIED_SANDBOX_FIXTURE` *(UX polish: `IR-QA-UX-01,02,04..07`)* |
| **§3 Native Tauri vs Browser Mock** | `native-tauri-smoke.mjs` (`NT-01..12`) | `12` | `11` | `0` | `0` | `1` | **`PASS`** | `VERIFIED_NATIVE_TAURI` (`NT-07` tray not in MVP) |
| **§4 Windows Installer Acceptance** | `installer-sandbox-check.ps1` (`INST-01..14`) | `14` | `6` | `0` | `2` | `6` | **`NOT_TESTED`** | `AWAITING_INTEGRATION_BUILD` + `BLOCKED_BY_PLATFORM_POLICY` (SAC/Signing) |
| **§5 L3 Real-Account Acceptance** | `run-l3-guided-acceptance.ps1` (`L3-ACC-01..08`) | `8` | `0` | `0` | `0` | `8` | **`NOT_TESTED`** | `AWAITING_MANUAL_L3` |
| **Subtotal (§2 – §5 Product & Release Checks)** | `acceptance-summary.json` | **`60`** | **`43`** | **`0`** | **`2`** | **`15`** | **`BLOCKED` / `NOT_TESTED`** | `AWAITING_INTEGRATION_BUILD` · `AWAITING_MANUAL_L3` |
| **§6 Automated Harness & Host Safety** | `run-acceptance-suite.mjs` (`HARN-01..06`) | `6` | `6` | `0` | `0` | `0` | **`PASS`** | `VERIFIED_HOST_INVARIANCE` |

---

## 2. Section 2 — Product Journeys & Non-Technical UX Acceptance (`journeys-and-errors.mjs`)

> **Surface Distinction:** UI DOM assertions in `journeys-and-errors.mjs` execute against live `apps/desktop/ui-server.mjs` + Edge Headless Chromium (`window.__TAURI_INTERNALS__` absent), cross-checking every backend contract against both native Rust `dim-cli.exe` (`cd62c52`, SHA-256 `6d741ddc...`) and Signed-Host PowerShell `scripts/dim-host.ps1`. Native Tauri `dim-desktop.exe` (`cd62c52`, SHA-256 `2168cacf...`) WebView2 IPC is verified separately in **Section 3** below.

### §2.1 First Launch (`3 / 3 PASS`)

| ID | Acceptance Criterion | Gate Status | Workflow Marker | Verified Evidence (`evidence/journeys-and-errors-report.json` & [`user-journeys.md`](./user-journeys.md)) |
| :---: | :--- | :---: | :--- | :--- |
| **`FL-01`** | Clean empty state & read-only Primary Zalo card on first launch | **`PASS`** | `VERIFIED_SANDBOX_FIXTURE` | `#profiles-empty` visible, `#search-wrap` hidden at `< 2` accounts, `#primary-zalo-section` visible with `0` close/delete buttons. |
| **`FL-02`** | Non-technical 2-step onboarding clarity ("separate space" mental model) | **`PASS`** | `VERIFIED_SANDBOX_FIXTURE` | Plain `vi-VN`/`en-US` copy explains user signs in inside Zalo Desktop (no password/OTP stored); `4` suggestion chips + `8` WCAG AA color swatches. |
| **`FL-03`** | Zero requirement to understand `PID`, `IPC`, `lease`, or `lockfile` | **`PASS`** | `VERIFIED_SANDBOX_FIXTURE` | `0` technical jargon tokens in default `vi-VN`/`en-US` views; technical fields stay collapsed inside `<details>` (`Thông tin nâng cao` / `Chi tiết kỹ thuật`). |

### §2.2 Account Management — All 10 Actions (`10 / 10 PASS` on Sandbox Fixtures)

| ID | User Action | Gate Status | Workflow Marker | Verified Evidence |
| :---: | :--- | :---: | :--- | :--- |
| **`AM-01`** | 1. Add account | **`PASS`** | `VERIFIED_SANDBOX_FIXTURE` | 2-step `<dialog>` creates `<data_dir>/Profiles/<slug>_<8hex>`, `.zalo-profile-id`, and atomic `profiles.json` entry. |
| **`AM-02`** | 2. Set memorable name | **`PASS`** | `VERIFIED_SANDBOX_FIXTURE` | Vietnamese Unicode `Công việc — Kinh doanh` preserved; inline `#create-step1-error` (`role="alert"`) keeps modal open on `INVALID_NAME`/`DUPLICATE_NAME`. |
| **`AM-03`** | 3. Choose distinguishing color | **`PASS`** | `VERIFIED_SANDBOX_FIXTURE` | `8` swatches (`>= 5.02:1` contrast on white) with `✓` checkmark & auto collision avoidance (`IR-QA-UX-05` filed for cross-origin persistence). |
| **`AM-04`** | 4. Open Zalo | **`PASS`** | `VERIFIED_SANDBOX_FIXTURE` | Launched `2` concurrent isolated fixture instances with distinct PIDs, isolated `USERPROFILE`/`APPDATA`/`LOCALAPPDATA`, and `zalo_user_id`s (live Zalo QR: `AWAITING_MANUAL_L3`). |
| **`AM-05`** | 5. Switch to the right window | **`PASS`** | `VERIFIED_SANDBOX_FIXTURE` | `Chuyển đến` / `Switch to window` routes `.focus-signal` exclusively to target profile; 6-condition `EnumWindows` excludes `352×82` `notiWin`. |
| **`AM-06`** | 6. Close 1 account | **`PASS`** | `VERIFIED_SANDBOX_FIXTURE` | `Đóng Zalo` / `Close Zalo` terminates target PID tree only; sibling secondary profile and `11` Primary `Zalo*` processes remain running. |
| **`AM-07`** | 7. Rename account | **`PASS`** | `VERIFIED_SANDBOX_FIXTURE` | Renamed to `Công việc VIP` + updated color swatch atomically while preserving `8-hex` ID and `<slug>_<8hex>` directory. |
| **`AM-08`** | 8. Back up account | **`PASS`** | `VERIFIED_SANDBOX_FIXTURE` | Disabled while running/unclean (`"Đóng Zalo trước"` hint); offline `.partial -> backup.json` writes per-file SHA-256 manifest with `0` `.partial` residue. |
| **`AM-09`** | 9. Restore account | **`PASS`** | `VERIFIED_SANDBOX_FIXTURE` | `Khôi phục thành tài khoản mới` verifies SHA-256, creates 3rd isolated profile inheriting source color without touching source or Primary Zalo. |
| **`AM-10`** | 10. Delete account | **`PASS`** | `VERIFIED_SANDBOX_FIXTURE` | `#delete-profile-dialog` sets `autofocus` on `Hủy` / `Cancel`; `confirm: true` unlinks NTFS junctions in 2 phases and removes only target directory. |

### §2.3 Daily Usage — All 6 Invariants (`6 / 6 PASS`)

| ID | Daily Invariant | Gate Status | Workflow Marker | Verified Evidence |
| :---: | :--- | :---: | :--- | :--- |
| **`DU-01`** | 1. Reopen manager cleanly | **`PASS`** | `VERIFIED_SANDBOX_FIXTURE` | Restarting `ui-server.mjs` and `dim-desktop.exe` (`1.74s` native HWND + DOM) reloads all accounts, notes, and backups intact. |
| **`DU-02`** | 2. Restore accurate running/closed/`recoveryRequired` state | **`PASS`** | `VERIFIED_SANDBOX_FIXTURE` | Live `.runtime-lease.json` + process inspection restores `Cửa sổ đang mở`, `Đã đóng`, and `Cần mở lại để khôi phục` (`IR-QA-UX-03` verified closed in `cd62c52`). |
| **`DU-03`** | 3. Never open the wrong account | **`PASS`** | `VERIFIED_SANDBOX_FIXTURE` | `.zalo-profile-id` marker verification + `launch_count` delta `0` on untouched profile + `DUPLICATE_ZALO_USER_ID` guard. |
| **`DU-04`** | 4. Never close primary Zalo | **`PASS`** | `VERIFIED_SANDBOX_FIXTURE` | `PROTECTED_PRIMARY_PATH` & `PROTECTED_PRIMARY_PROCESS` enforced across Rust & PS1; `%APPDATA%\ZaloData\config.json` SHA-256 & `11` PIDs unchanged. |
| **`DU-05`** | 5. Retain color badge | **`PASS`** | `VERIFIED_SANDBOX_FIXTURE` | `localStorage['multi-zalo.accountColors']` retained across restarts within same origin (`IR-QA-UX-05` filed for `profiles.json` storage). |
| **`DU-06`** | 6. Zero technical popups | **`PASS`** | `VERIFIED_SANDBOX_FIXTURE` | `0` `window.alert`/`confirm` dialogs, `0` uncaught JS exceptions, `0` console errors, `windowsHide: true` / `WINDOWS_GUI` subsystem. |

### §2.4 Error Handling — All 7 Required Scenarios (`7 / 7 PASS` Fail-Safe; Open UX Polish Tracked in `IR-QA-UX-01,02,04..07`)

| ID | Failure Scenario | Gate Status | Workflow Marker | Verified Evidence & Linked Integration Requests |
| :---: | :--- | :---: | :--- | :--- |
| **`ERR-01`** | 1. Zalo Desktop missing | **`PASS`** | `VERIFIED_SANDBOX_FIXTURE` | Proactive `#zalo-install-alert` + `INSTALL_NOT_FOUND` plain `vi-VN`/`en-US` copy (`IR-QA-UX-06` suggests inline retry button). |
| **`ERR-02`** | 2. Backend unresponsive / timeout | **`PASS`** | `VERIFIED_SANDBOX_FIXTURE` | `504 HOST_TIMEOUT` & `HOST_EXEC_FAILED` show `#connection-pill` (`"Chưa kết nối"` / `"Disconnected"`) and recover cleanly on retry. |
| **`ERR-03`** | 3. Insufficient directory permissions | **`PASS`** | `VERIFIED_SANDBOX_FIXTURE` | NTFS write-deny ACL handled safely without crash: `dim-cli.exe` emits `IO_ERROR`; `dim-host.ps1` emits `UNHANDLED_EXCEPTION` (`IR-QA-UX-07`). |
| **`ERR-04`** | 4. Account already open | **`PASS`** | `VERIFIED_SANDBOX_FIXTURE` | Same-profile concurrent launches coalesce to 1 PID; cross-profile duplicate Zalo UID blocked with `DUPLICATE_ZALO_USER_ID`. |
| **`ERR-05`** | 5. Backup failed | **`PASS`** | `VERIFIED_SANDBOX_FIXTURE` | Rejects open profile (`PROFILE_RUNNING`) and unacknowledged external DB (`EXTERNAL_DB_OUTSIDE_PROFILE`) with `0` `.partial` residue. |
| **`ERR-06`** | 6. Insufficient disk space | **`PASS`** | `VERIFIED_SANDBOX_FIXTURE` | Sparse file `> free space` preflight aborts before copying bytes with `INSUFFICIENT_DISK_SPACE` (`IR-QA-UX-06` copy note). |
| **`ERR-07`** | 7. Corrupted `profiles.json` & backup archive | **`PASS`** | `VERIFIED_SANDBOX_FIXTURE` | `0`-byte/corrupt `profiles.json` preserved unmodified (`JSON_ERROR`/`SCHEMA_ERROR`); tampered backup SHA-256 rejected (`0` `.restore-*` residue). Open UX polish: `IR-QA-UX-01` (Tauri pre-launch corrupt `profiles.json` panic), `IR-QA-UX-02` (`JSON_ERROR` copy), and `IR-QA-UX-04` (`BACKUP_ERROR` copy on restore). |

---

## 3. Section 3 — Native Tauri WebView2 vs Browser Mock (`native-tauri-smoke.mjs` on `cd62c52`)

> **Explicit Separation:** `native-tauri-smoke.mjs` benchmarks both **Browser Mock** (`http://127.0.0.1:<port>`, `hasTauriInternals: false`, HTTP `fetch` fallback) and **Native Tauri** (`dim-desktop.exe` SHA-256 `2168cacfba423a9f7012b46040b3998d96cad24bd754b3464d13996bb3e1376d`, `22,672,392` B, `is_installer: false`, `http://tauri.localhost`, `window.__TAURI_INTERNALS__.invoke` -> `13` Rust commands). Full evidence in [`native-smoke-test-report.md`](./native-smoke-test-report.md).

| ID | Native Tauri Acceptance Check | Gate Status | Workflow Marker | Measured Evidence on `cd62c52` (`evidence/native-tauri-smoke-report.json`) |
| :---: | :--- | :---: | :--- | :--- |
| **`NT-01`** | 1. WebView2 runtime & native window creation + clean exit (`0` orphans) | **`PASS`** | `VERIFIED_NATIVE_TAURI` | WebView2 `157.0.4341.46`; Win32 `HWND` (`1160×780` client, `120 DPI`); cold CDP ready `695.69 ms`, DOM interactive `1,738.92 ms`, graceful `WM_CLOSE` exit `0` in `499.85 ms`, `0` orphan `dim-*` or `msedgewebview2.exe`. |
| **`NT-02`** | 2. Tauri IPC (`window.__TAURI_INTERNALS__`) vs Browser Mock separation | **`PASS`** | `VERIFIED_NATIVE_TAURI` | Live Rust IPC (`get_status` `14.0 ms`, `get_diagnostics` `3.2 ms`, `verify_isolation` `17.2 ms`, `list_backups` `0.8 ms`); Settings shows `"Đã kết nối (Ứng dụng máy tính)"` vs Browser Mock `"Chưa kết nối"`. |
| **`NT-03`** | 3. Frontend loading, CSP enforcement & Account Dock initial render | **`PASS`** | `VERIFIED_NATIVE_TAURI` | `script-src 'self'` blocked inline `<script>` (`securitypolicyviolation`); empty hero & read-only Primary Zalo card rendered cleanly. |
| **`NT-04`** | 4. Keyboard navigation (`Tab`, `:focus-visible`, `Space`/`Enter`, `Escape`, focus trap) | **`PASS`** | `VERIFIED_NATIVE_TAURI` | `0` positive `tabindex`, `0` focusable `aria-hidden` descendants, `5.38:1` `:focus-visible` ring, `12/12` `Tab` cycles trapped in `:modal`, `Escape` restores invoker focus, `⋯` menu `ArrowDown`/`Escape` verified. |
| **`NT-05`** | 5. Native `<dialog>` modals (add, rename, backup/restore, delete) | **`PASS`** | `VERIFIED_NATIVE_TAURI` | All `5` `<dialog>` modals use `showModal()`, omit redundant `role`/`aria-modal`, reset `returnValue`, enforce `overscroll-behavior: contain`, and `autofocus` `Cancel` on delete. |
| **`NT-06`** | 6. Window focus & multi-window handoff behavior | **`PASS`** | `VERIFIED_NATIVE_TAURI` | Launched `2` concurrent fixtures (`PID 34564`, `PID 26796`) and focused Profile 1 (`focus_count >= 1`, sibling `0`) **directly over live WebView2 -> Tauri 2 IPC** (`IR-NAT-02` closed in `cd62c52`); single-account stop preserved sibling + Primary Zalo (live Zalo GUI: `AWAITING_MANUAL_L3`). |
| **`NT-07`** | 7. System tray verification (`tauri.conf.json` / `src-tauri`) | **`NOT_TESTED`** | `NOT_IN_MVP_SCOPE` | Verified honestly against source: no `trayIcon` in `tauri.conf.json`, `features = []` in `src-tauri/Cargo.toml`, no `TrayIconBuilder` in `main.rs`; closing manager exits cleanly while Zalo instances stay open. |
| **`NT-08`** | 8. Light / Dark mode (`prefers-color-scheme`, toggle & WCAG contrast) | **`PASS`** | `VERIFIED_NATIVE_TAURI` | FOUC-free `theme-init.js` + `color-scheme: light dark`; WCAG 2.2 AA verified in Light (`17.6:1` primary, `4.71:1` muted) & Dark (`14.89:1` primary, `5.26:1` muted) + `8` swatches (`5.02:1–7.90:1`) + `forced-colors`. |
| **`NT-09`** | 9. Tiếng Việt (`vi-VN`) / English (`en-US`) completeness & zero jargon | **`PASS`** | `VERIFIED_NATIVE_TAURI` | `275 / 275` leaf keys in `vi-VN` & `en-US` (`0` missing keys), diacritics preserved, `0` leaks of `PID`, `IPC`, `lockfile`, `.runtime-lease`, or Rust panics. |
| **`NT-10`** | 10. HiDPI & Windows scaling (`100%`, `125%`, `150%`, `200%` + `200%` zoom) | **`PASS`** | `VERIFIED_NATIVE_TAURI` | `0 px` horizontal overflow and button heights `>= 32 px` across all 4 DPI tiers and `200%` zoom (`IR-QA-UX-08` / `IR-NAT-03` logged for `rem` root font size). |
| **`NT-11`** | 11. Small window (`800×600`, `640×560`, `420×540` min-size) | **`PASS`** | `VERIFIED_NATIVE_TAURI` | Win32 `MoveWindow` verified `0 px` overflow, `1`-column grid collapse `< 720px`, `static` topbar unpinning, and unclipped `<dialog>` modals at `420×540`. |
| **`NT-12`** | 12. Manager crash & relaunch recovery (`taskkill /F` -> `.runtime-lease.json`) | **`PASS`** | `VERIFIED_NATIVE_TAURI` | Hard-killing `dim-desktop.exe` left running fixture (`PID 33008`) + `11` Primary `Zalo*` processes alive; relaunch reconciled `running: true`. Corrupted `.runtime-lease.json` rendered dedicated `"Cần mở lại để khôi phục"` / `"Reopen to recover"` badge (`06-recovery-required-badge.png`), disabled offline menu items, and blocked `backup`/`delete` (`IR-QA-UX-03` / `IR-NAT-01` verified closed in `cd62c52`). |

---

## 4. Section 4 — Windows Installer Acceptance (`installer-sandbox-check.ps1` on `cd62c52`)

> **Gate G2 Non-Conflation Rule:** Rebuilt raw `dim-desktop.exe` (`22,672,392` B, SHA-256 `2168cacf...`, `is_installer: false`) is **never** treated as an installer. Full host audit and runbook in [`installer-validation.md`](./installer-validation.md).

| ID | Installer & Packaging Gate | Gate Status | Workflow / Policy Marker | Verified Evidence (`evidence/installer-env-audit.json`) |
| :---: | :--- | :---: | :--- | :--- |
| **`INST-01`** | Official NSIS (`*-setup.exe`) / WiX (`*.msi`) candidate presence | **`NOT_TESTED`** | `AWAITING_INTEGRATION_BUILD` | `0` installer bundles exist yet (`BLK-02`/`BLK-03`/`BLK-04`); script rejects raw `.exe` with `NOT_AN_INSTALLER_RAW_EXECUTABLE`. |
| **`INST-02`** | Bundle metadata & CI `downloadBootstrapper` overlay (`tauri.conf.json`) | **`PASS`** | `VERIFIED_STATIC` | `productName="Multi-Zalo Desktop"`, `version="0.1.0"`, `identifier="com.multizalodesktop.manager"`, valid `icon.ico` header, `tauri-ci-bundle-overlay.json` valid. |
| **`INST-03`** | Cryptographic SHA-256 & Authenticode code signing (Gate G3) | **`BLOCKED`** | `UNSIGNED_DEVELOPMENT_ARTIFACT` | No OV/EV or Azure Trusted Signing cert provisioned (`BLK-06`). |
| **`INST-04`** | Windows Smart App Control (`SAC`) non-bypass policy gate | **`PASS`** | `BLOCKED_BY_PLATFORM_POLICY` *(when `SAC=1`)* | Read-only `VerifiedAndReputablePolicyState = 0` (`OFF`) on QA host (`policy_modified: false`); `-SimulatedSacState 1` verified immediate `BLOCKED_BY_PLATFORM_POLICY` halt on unsigned installers. |
| **`INST-05`** | WebView2 Evergreen Runtime prerequisite (`{F3017226-...}`) | **`PASS`** | `VERIFIED_HOST` | Installed (`v157.0.4341.46`) and `msedgewebview2.exe` verified on disk. |
| **`INST-06`** | Local MSVC / NSIS / WiX / `@tauri-apps/cli@2.12.5` bundler availability | **`BLOCKED`** | `DELEGATED_TO_WINDOWS_2022_CI` | Local host has `x86_64-pc-windows-gnu` without MSVC/NSIS/WiX (`BLK-04`); canonical build runs on GitHub Actions `windows-2022`. |
| **`INST-07`** | Zero bundled official VNG Zalo binary policy (Gate G1) | **`PASS`** | `ENFORCED` | `0` `Zalo.exe`, `ZaloCall.exe`, or `app.asar` files in `src-tauri` bundle inputs; payload scanner armed for RC installer. |
| **`INST-08`** | Per-user silent install lifecycle (`asInvoker`, zero UAC) | **`NOT_TESTED`** | `AWAITING_INTEGRATION_BUILD` | Armed in `installer-sandbox-check.ps1 -CandidatePath`; awaiting `windows-2022` CI NSIS/MSI build. |
| **`INST-09`** | Start Menu & Desktop shortcut creation & target verification | **`NOT_TESTED`** | `AWAITING_INTEGRATION_BUILD` | Armed in `installer-sandbox-check.ps1`; awaiting RC installer build. |
| **`INST-10`** | In-place update / upgrade path verification | **`NOT_TESTED`** | `AWAITING_INTEGRATION_BUILD` | Armed via `-UpgradeFromPath` + `-CandidatePath`; awaiting RC installer build. |
| **`INST-11`** | User profile data retention across upgrade & uninstall (`0` profile deletion) | **`NOT_TESTED`** | `AWAITING_INTEGRATION_BUILD` | Host `<LOCALAPPDATA>\MultiZaloDesktop` verified untouched; installer retention check awaits RC build. |
| **`INST-12`** | Clean silent uninstall lifecycle | **`NOT_TESTED`** | `AWAITING_INTEGRATION_BUILD` | Armed in `installer-sandbox-check.ps1`; awaiting RC installer build. |
| **`INST-13`** | Primary `%APPDATA%\ZaloData` & live `Zalo*` invariance | **`PASS`** | `VERIFIED_INVARIANT` | `config.json` SHA-256 (`be8077e1...`) & `11` running primary `Zalo*` processes `100%` unchanged. |
| **`INST-14`** | Zero orphan manager or installer processes post-check | **`PASS`** | `VERIFIED_INVARIANT` | `0` orphan processes left after audit. |

---

## 5. Section 5 — L3 Real-Account Acceptance (`run-l3-guided-acceptance.ps1` on `cd62c52`)

> **Human-Only QR Gate:** Automated QR/OTP/session manipulation is strictly prohibited. Running `run-l3-guided-acceptance.ps1 -NonInteractive` immediately emits `AWAITING_MANUAL_L3` (`NOT_TESTED`) without faking `PASS`. Full tester runbook in [`l3-manual-test-checklist.md`](./l3-manual-test-checklist.md).

| ID | Real 2-Account Zalo Acceptance Check | Gate Status | Workflow / Policy Marker | Current Evidence & Boundary Note |
| :---: | :--- | :---: | :--- | :--- |
| **`L3-ACC-01`** | 1. Independent manual QR login of 2 authorized test Zalo accounts | **`NOT_TESTED`** | `AWAITING_MANUAL_L3` | Official VNG Zalo `26.10.20` (`CN=VNG GROUP JSC`) verified installed; awaits human QR scan (`BLK-05`). |
| **`L3-ACC-02`** | 2. Both real Zalo accounts open simultaneously in separate windows | **`NOT_TESTED`** | `AWAITING_MANUAL_L3` | L1/L2 multi-process launch proven; authenticated 2-account session awaits human L3 run. |
| **`L3-ACC-03`** | 3. Zero cross-logout across simultaneous use & manager/PC restart (DPAPI isolation) | **`NOT_TESTED`** | `AWAITING_MANUAL_L3` | Per-profile `USERPROFILE`/`APPDATA`/`LOCALAPPDATA` verified on fixtures; live DPAPI token survival awaits human L3. |
| **`L3-ACC-04`** | 4. Accurate window switching (`Chuyển đến` / `Switch to window`) to the right account | **`NOT_TESTED`** | `AWAITING_MANUAL_L3` | 6-condition `EnumWindows` + `second-instance` tray handoff verified at L1/L2; live 2-account verification awaits human L3. |
| **`L3-ACC-05`** | 5. Independent single-account close (`Đóng Zalo`) without closing sibling or Primary Zalo | **`NOT_TESTED`** | `AWAITING_MANUAL_L3` | PID-tree + Restart Manager exclusion proven on fixtures; live authenticated check awaits human L3. |
| **`L3-ACC-06`** | 6. Clean reopen (`Mở Zalo`) with authenticated session preserved (no QR re-scan) | **`NOT_TESTED`** | `AWAITING_MANUAL_L3` | Awaits human L3 execution. |
| **`L3-ACC-07`** | 7. Bidirectional test message & dummy `.txt` file transfer (`0` PII/chat capture) | **`NOT_TESTED`** | `AWAITING_MANUAL_L3` | Checklist enforces strict rule: **never** capture chat text, phone numbers, avatars, or screenshots. |
| **`L3-ACC-08`** | 8. Honest verification & user disclosure of Voice/Video call & Toast boundaries | **`NOT_TESTED`** | `AWAITING_MANUAL_L3` · `BLOCKED_BY_PLATFORM_POLICY` *(Concurrent Calls)* | Host preflight confirmed `ZaloCall.exe` holds `\\.\pipe\PipeZCall*` while Primary Zalo runs (`BLOCKED` for simultaneous 2-account calls; sequential single-account call only) and shared `com.vng.zalo` Toast vs per-instance `notiWin` banner; disclosed in `#view-help` & `windows-mvp-guide.md`. |

---

## 6. Section 6 — Automated Acceptance Harness & Safety Invariants (`run-acceptance-suite.mjs`)

| ID | Harness & Isolation Gate | Gate Status | Workflow Marker | Verified Evidence (`evidence/acceptance-summary.json`) |
| :---: | :--- | :---: | :--- | :--- |
| **`HARN-01`** | Sequential 4-track orchestration with bounded timeouts (`0` infinite wait loops) | **`PASS`** | `VERIFIED_HOST_INVARIANCE` | Executes `journeys-and-errors.mjs`, `native-tauri-smoke.mjs`, `installer-sandbox-check.ps1`, and `run-l3-guided-acceptance.ps1 -NonInteractive` deterministically on `cd62c52`. |
| **`HARN-02`** | 100% Primary `%APPDATA%\ZaloData\config.json` SHA-256 & live `Zalo*` process invariance | **`PASS`** | `VERIFIED_HOST_INVARIANCE` | SHA-256 `be8077e13b0f59e6976a7baa549b836a46569ca397f2bdf5d3e0f3a46be07734` and all `11` live `Zalo*` processes verified identical before and after every track. |
| **`HARN-03`** | Zero orphan `dim-*`, fixture `node.exe`, `ui-server.mjs`, or `msedgewebview2.exe` processes | **`PASS`** | `VERIFIED_HOST_INVARIANCE` | `diffOrphanProcesses` confirmed `orphanCount: 0` after each track and after full suite teardown. |
| **`HARN-04`** | Worktree & build-artifact isolation (`CARGO_TARGET_DIR` outside worktree, `0` product diffs) | **`PASS`** | `VERIFIED_HOST_INVARIANCE` | Compiled into `<TEMP_DIR>/zalo-qa-win-acceptance-target`; only `tests/windows-acceptance/**` and `docs/qa/windows/**` touched on `qa/windows-native-acceptance`; sibling `zalo` & `zalo-integration` untouched. |
| **`HARN-05`** | Windows Smart App Control (`SAC`) non-modification invariant | **`PASS`** | `VERIFIED_HOST_INVARIANCE` | `smartAppControlModified: false` across all tracks. |
| **`HARN-06`** | Automated PII & credential scrubbing gate (`assertZeroPiiLeaks`) | **`PASS`** | `VERIFIED_HOST_INVARIANCE` | `0` personal usernames, machine hostnames, raw `Users` paths, phone numbers, or session tokens in `evidence/*.json` or `docs/qa/windows/*.md`. |
