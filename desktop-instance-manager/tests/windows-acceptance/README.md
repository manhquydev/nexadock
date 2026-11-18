# Windows Native User Acceptance & Installation Lab (`tests/windows-acceptance`)

Independent, non-destructive end-to-end Windows 10/11 user acceptance, native Tauri WebView2 smoke, installer readiness, and guided L3 real-account verification suite for **Multi-Zalo Desktop — Instance Manager**.

---

## 1. Architecture & Track Inventory

| Track | Script | Evidence Artifact (`evidence/`) | QA Report (`../../docs/qa/windows/`) | Scope |
| :---: | :--- | :--- | :--- | :--- |
| **1. Product Journeys & Error UX** | `journeys-and-errors.mjs` | `journeys-and-errors-report.json` | `user-journeys.md`, `integration-requests.md` | First Launch, all 10 Account Management actions, all 6 Daily Usage invariants, and all 7 Error Handling scenarios (`vi-VN` & `en-US`) across `dim-cli.exe`, `dim-host.ps1`, and Account Dock UI contracts using sandbox fixtures. |
| **2. Native Tauri vs Browser Mock** | `native-tauri-smoke.mjs` | `native-tauri-smoke-report.json` | `native-smoke-test-report.md` | Explicit separation of Browser Mock (`127.0.0.1` HTTP) vs Native Tauri (`dim-desktop.exe` + WebView2 + Rust IPC), keyboard/focus traps, `<dialog>` modals, dark/light themes, HiDPI (`100%`–`200%`), small window (`<= 800×600`), and crash recovery. |
| **3. Installer & SAC Gate** | `installer-sandbox-check.ps1` | `installer-env-audit.json` | `installer-validation.md` | Release Captain NSIS/MSI candidate audit (`AWAITING_INTEGRATION_BUILD` when unbuilt), Authenticode (`UNSIGNED_DEVELOPMENT_ARTIFACT`), Smart App Control (`BLOCKED_BY_PLATFORM_POLICY` without ever disabling SAC), WebView2 Evergreen prerequisite, and profile-preserving install/uninstall checks. |
| **4. L3 Real-Account Guided Gate** | `run-l3-guided-acceptance.ps1` | *(Interactive recorder / `-NonInteractive` status)* | `l3-manual-test-checklist.md` | 2-account manual QR login checklist (`-NonInteractive` immediately emits `AWAITING_MANUAL_L3` / `NOT_TESTED` without faking `PASS` or touching credentials). |
| **Unified Orchestrator** | `run-acceptance-suite.mjs` | `acceptance-summary.json` | `acceptance-matrix.md`, `product-acceptance-verdict.md` | Sequential bounded execution of Tracks 1–4, per-track & suite-wide `%APPDATA%\ZaloData` SHA-256 invariance, zero-orphan process verification, and PII-leak audit across `evidence/` and `docs/qa/windows/`. |

---

## 2. Strict Host Safety & Privacy Guarantees

1. **100% Primary Zalo Non-Interference:** Every run snapshots `%APPDATA%\ZaloData\config.json` SHA-256 and active `Zalo*` PIDs before and after execution (`lib/common.mjs`). All automated tests run exclusively inside ephemeral `%TEMP%\zalo-win-qa-*` sandboxes using synthetic fixture processes.
2. **Zero Smart App Control (SAC) Modification:** Unsigned PE checks read `HKLM:\SYSTEM\CurrentControlSet\Control\CI\Policy\VerifiedAndReputablePolicyState` in read-only mode and record `BLOCKED_BY_PLATFORM_POLICY` if enforced. SAC is **never** disabled or bypassed.
3. **Zero Credential / OTP / QR Automation:** Automated scripts never interact with real Zalo login screens, cookies, DPAPI keys, SQLite chat databases, or message contents.
4. **Clean Worktree Target Isolation:** Set `CARGO_TARGET_DIR` to `<TEMP_DIR>/zalo-qa-win-acceptance-target` so Cargo compilation never writes untracked `target/` artifacts inside the repository worktree.
5. **Deterministic PII Redaction:** `lib/common.mjs` (`writeRedactedJson` + `assertZeroPiiLeaks`) scrubs `<USER_HOME>`, `<WORKTREE_ROOT>`, `<TEMP_DIR>`, OS usernames, hostnames, and credential-like tokens before writing any JSON artifact.

---

## 3. Quick Start Commands

Run from `<WORKTREE_ROOT>/zalo-windows-acceptance`:

```powershell
# 1. Point Cargo target outside the worktree (used if compiling dim-cli / dim-desktop)
$env:CARGO_TARGET_DIR = "$env:TEMP\zalo-qa-win-acceptance-target"

# 2. Execute the full 4-track Windows Native Acceptance Suite (non-interactive, deterministic)
node desktop-instance-manager/tests/windows-acceptance/run-acceptance-suite.mjs

# Optional: Supply a Release Captain NSIS (*-setup.exe) or WiX (*.msi) installer candidate to Track 3
node desktop-instance-manager/tests/windows-acceptance/run-acceptance-suite.mjs --candidate-path "<PATH_TO_INSTALLER>"
```

### Running Individual Tracks

```powershell
# Track 1: Product Journeys & 7 Error Scenarios
node desktop-instance-manager/tests/windows-acceptance/journeys-and-errors.mjs

# Track 2: Native Tauri WebView2 & Modern Web Guidance Smoke
node desktop-instance-manager/tests/windows-acceptance/native-tauri-smoke.mjs

# Track 3: Installer Readiness & SAC Audit
powershell -NoProfile -ExecutionPolicy Bypass -File desktop-instance-manager/tests/windows-acceptance/installer-sandbox-check.ps1

# Track 4a: Non-Interactive L3 Gate Check (emits AWAITING_MANUAL_L3)
powershell -NoProfile -ExecutionPolicy Bypass -File desktop-instance-manager/tests/windows-acceptance/run-l3-guided-acceptance.ps1 -NonInteractive

# Track 4b: Interactive Human 2-Account QR Session (run manually by human tester)
powershell -NoProfile -ExecutionPolicy Bypass -File desktop-instance-manager/tests/windows-acceptance/run-l3-guided-acceptance.ps1
```

---

## 4. Canonical Gate Statuses & Workflow Markers

- **Canonical Gate Statuses:** `PASS` | `FAIL` | `BLOCKED` | `NOT_TESTED`
- **Workflow / Policy Markers:**
  - `AWAITING_INTEGRATION_BUILD` — Central Integration branch is mid-wave or Release Captain NSIS/MSI installer (`is_installer: true`) has not yet been built/handed over.
  - `AWAITING_MANUAL_L3` — Requires human tester to scan QR codes with 2 authorized Zalo accounts (`NOT_TESTED` in automated runs).
  - `BLOCKED_BY_PLATFORM_POLICY` — Windows Smart App Control (`SAC = 1`) or Authenticode policy blocks an unsigned artifact, or upstream VNG `PipeZCall*` / `com.vng.zalo` OS-global resource limits apply.
