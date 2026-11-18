# Windows Installer Validation & Host Readiness Report (`Section 4`)

- **Worktree / Branch:** `<WORKTREE_ROOT>/zalo-windows-acceptance` (`qa/windows-native-acceptance`)
- **Evaluated Integration Commit:** `cd62c523222cb3a02051347e85313226de8b6964` *(clean fast-forward over `bff0674` + `004ab7c`)*
- **Harness Script:** `desktop-instance-manager/tests/windows-acceptance/installer-sandbox-check.ps1`
- **Redacted Evidence Artifact:** `desktop-instance-manager/tests/windows-acceptance/evidence/installer-env-audit.json`
- **Section 4 Gate Status:** **`NOT_TESTED`** *(Installer Lifecycle)* / **`BLOCKED`** *(Local MSVC/NSIS/WiX Bundlers `BLK-04` & Public Authenticode Signing `BLK-06`)*
- **Workflow State:** **`AWAITING_INTEGRATION_BUILD`**

---

## 1. Executive Summary

Independent Windows 11 host packaging and installer readiness validation was executed against integrated candidate SHA `cd62c523222cb3a02051347e85313226de8b6964` (`0` dirty files in `<WORKTREE_ROOT>/zalo-integration`) without modifying product source code, sibling worktrees, Windows Smart App Control (`SAC`), or the user's live primary Zalo session (`%APPDATA%\ZaloData`).

1. **Official Installer Candidate Presence (`AWAITING_INTEGRATION_BUILD`):**
   - Inspection of Release Captain's handoff ledger (`<WORKTREE_ROOT>/zalo/desktop-instance-manager/docs/release/integration-readiness-matrix.md` Track 10 and `release-blocker-register.md`) plus all local bundle target paths confirmed **zero NSIS (`*-setup.exe`) or WiX (`*.msi`) installer bundles (`is_installer: true`) exist yet**.
   - While Central Integration (`<WORKTREE_ROOT>/zalo-integration`) has now committed a clean tree at `cd62c523222cb3a02051347e85313226de8b6964` (`bff0674` + `cd62c52`), Release Captain Track 10 remains **`NOT_BUILT_YET` (`BLOCKED_TOOLCHAIN_AND_RC_SHA`)** pending final C2 regression on `cd62c52` (`BLK-02`), GitHub `workflow` OAuth scope refresh (`BLK-03`), and canonical `windows-2022` MSVC CI execution (`BLK-04`).
2. **Strict Gate G2 Non-Conflation Enforced:**
   - Raw compiled PE executables observed on the host (`dim-desktop.exe` `22,672,392` B @ SHA-256 `2168cacfba423a9f7012b46040b3998d96cad24bd754b3464d13996bb3e1376d` compiled from `cd62c52` in `<USER_HOME>\AppData\Local\Temp\zalo-qa-win-acceptance-target\release\dim-desktop.exe`, and `21,577,450` B @ SHA-256 `56fe670d0d4f4cc99e931e3564c741489a51d91f6868dae56462ea55dc4978b6` in `<WORKTREE_ROOT>\zalo`) are explicitly classified as **`compiled-executable` (`is_installer: false`)** and rejected (`exit 1`, `NOT_AN_INSTALLER_RAW_EXECUTABLE`) if passed to `-CandidatePath`.
3. **Host Runtime Prerequisites Ready (`PASS`):**
   - **WebView2 Evergreen Runtime** (`{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}`) is installed system-wide at **`v157.0.4341.46`** (`msedgewebview2.exe` verified on disk) alongside `downloadBootstrapper` configuration in `scripts/ci-cross-platform/tauri-ci-bundle-overlay.json`.
   - **Official VNG Zalo Desktop** (`Zalo-26.10.20\Zalo.exe`) is installed in `%LOCALAPPDATA%\Programs\Zalo` with a valid cryptographic Authenticode signature (`CN=VNG GROUP JSC, O=VNG GROUP JSC`), while **zero VNG binaries** exist inside `Multi-Zalo Desktop` bundle inputs (`Gate G1 PASS`).
4. **100% Host & Primary Zalo Safety Invariance (`PASS`):**
   - Primary `%APPDATA%\ZaloData\config.json` SHA-256 (`be8077e13b0f59e6976a7baa549b836a46569ca397f2bdf5d3e0f3a46be07734`) and all **`11` live primary `Zalo*` processes** remained completely untouched before and after harness execution (`0` orphan processes left behind).

---

## 2. Release Captain Handoff & Artifact Classification Audit

### 2.1. Active Packaging & Release Blockers Cross-Referenced

| Blocker ID | Track | Status | Impact on Section 4 Installer Acceptance |
| :--- | :--- | :---: | :--- |
| **`BLK-01`** | Central Integration | `RESOLVED_IN_INTEGRATION_PENDING_RC_SYNC` | `<WORKTREE_ROOT>/zalo-integration` is now clean (`0` dirty files) at `cd62c523222cb3a02051347e85313226de8b6964`, awaiting Release Captain intake into `<WORKTREE_ROOT>/zalo`. |
| **`BLK-02`** | C2 Verification | `PENDING` | Post-`58dbd61` code changes (`f331085..cd62c52`) require one final C2 `release-regression` run on `cd62c52` before packaging. |
| **`BLK-03`** | GitHub CI Auth | `BLOCKED` | Local `gh` CLI lacks `workflow` OAuth scope and `.github/workflows/cross-platform-validation.yml` is not yet on `origin/main`. |
| **`BLK-04`** | Windows Bundler | `BLOCKED` | Local workstation runs `rustc 1.100.0` (`x86_64-pc-windows-gnu`) without MSVC `cl.exe`/`link.exe`, NSIS `makensis.exe`, WiX `candle.exe`/`light.exe`, or `@tauri-apps/cli@2.12.5`. Canonical NSIS/MSI packaging runs on GitHub Actions `windows-2022`. |
| **`BLK-06`** | Code Signing (`G3`) | `BLOCKED` *(Public)* | Zero OV/EV Authenticode or Azure Trusted Signing credentials configured; CI/local builds carry `UNSIGNED_DEVELOPMENT_ARTIFACT`. |

### 2.2. Strict Artifact Classification (`Gate G2`)

| Observed / Expected Artifact | Location | `artifact_kind` | `is_installer` | Acceptance Treatment |
| :--- | :--- | :--- | :---: | :--- |
| `dim-desktop.exe` (`22,672,392` B, `2168cacf...`) | `<USER_HOME>\AppData\Local\Temp\zalo-qa-win-acceptance-target\release\` | `compiled-executable` | **`false`** | **Rejected as installer** (`NOT_AN_INSTALLER_RAW_EXECUTABLE`); tested under Section 3 Native Tauri Smoke only. |
| `dim-cli.exe` (`2,567,980` B, `6d741ddc...`) | `<USER_HOME>\AppData\Local\Temp\zalo-qa-win-acceptance-target\release\` | `compiled-executable` | **`false`** | **Rejected as installer**; native CLI backend binary. |
| `Multi-Zalo Desktop_0.1.0_x64-setup.exe` | `bundle/nsis/*-setup.exe` *(pending `windows-2022` CI)* | `installer-nsis` | **`true`** | **Target RC NSIS per-user installer** (`AWAITING_INTEGRATION_BUILD`). |
| `Multi-Zalo Desktop_0.1.0_x64_en-US.msi` | `bundle/msi/*.msi` *(pending `windows-2022` CI)* | `installer-msi` | **`true`** | **Target RC WiX MSI package** (`AWAITING_INTEGRATION_BUILD`). |

---

## 3. Section 4 Check-by-Check Validation Results (`INST-01` – `INST-14`)

Summary counts from `evidence/installer-env-audit.json` (`duration_ms: 2533` on `cd62c523222cb3a02051347e85313226de8b6964`): **`6 PASS` | `0 FAIL` | `2 BLOCKED` | `6 NOT_TESTED`**.

| Check ID | Acceptance Criterion | Gate Status | Workflow Marker | Verified Host Evidence |
| :--- | :--- | :---: | :---: | :--- |
| **`INST-01`** | Official NSIS/MSI Installer Candidate Presence & Gate G2 Classification | `NOT_TESTED` | `AWAITING_INTEGRATION_BUILD` | `0` NSIS/MSI bundles present; raw `dim-desktop.exe` binaries explicitly classified `is_installer: false` and rejected if passed via `-CandidatePath`. |
| **`INST-02`** | Bundle Configuration & CI Overlay Audit (`tauri.conf.json` + `IR-003`) | `PASS` | `VERIFIED_STATIC` | `productName="Multi-Zalo Desktop"`, `version="0.1.0"`, `identifier="com.multizalodesktop.manager"`, `bundle.active=true`, all 5 icons (`32x32.png`, `128x128.png`, `128x128@2x.png`, `icon.icns`, `icon.ico` magic `00 00 01 00`) present on disk; `tauri-ci-bundle-overlay.json` (`downloadBootstrapper`) and `0003-IR-003` / `0006-IR-006` patches present. |
| **`INST-03`** | Cryptographic SHA-256 & Authenticode Code-Signing Gate (`Gate G3`) | `BLOCKED` | `UNSIGNED_DEVELOPMENT_ARTIFACT` | `signing_certificate_configured=false`, `signtool_available=false`; unauthenticated candidates are labeled `UNSIGNED_DEVELOPMENT_ARTIFACT` (`BLK-06` blocks public `SAC=1` release). |
| **`INST-04`** | Windows Smart App Control (`SAC`) Non-Bypass Policy Gate | `PASS` | `READY` | Read-only check of `HKLM:\SYSTEM\CurrentControlSet\Control\CI\Policy\VerifiedAndReputablePolicyState` returned `0` (`OFF`) on this QA host (`policy_modified=false`). Negative self-test (`-SimulatedSacState 1`) verified immediate `BLOCKED_BY_PLATFORM_POLICY` halt on unsigned installers. |
| **`INST-05`** | WebView2 Evergreen Runtime Prerequisite (`{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}`) | `PASS` | `VERIFIED_HOST` | Verified `pv = 157.0.4341.46` under `HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\{Clients,ClientState}\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}` and `C:\Program Files (x86)\Microsoft\EdgeWebView\Application\157.0.4341.46\msedgewebview2.exe` on disk. |
| **`INST-06`** | Local Windows Packaging Toolchain vs `windows-2022` CI | `BLOCKED` | `DELEGATED_TO_WINDOWS_2022_CI` | Host has `rustc 1.100.0` (`x86_64-pc-windows-gnu`), lacking MSVC `cl.exe`, `vswhere.exe`, NSIS `makensis.exe`, WiX `candle.exe`/`light.exe`, and `@tauri-apps/cli@2.12.5` (`BLK-04`). |
| **`INST-07`** | Zero Bundled Official VNG Zalo Binary Policy (`Gate G1`) | `PASS` | `ENFORCED` | `0` VNG binaries (`Zalo.exe`, `ZaloCall.exe`, `ZaloCap.exe`, `ZaloSetup*.exe`, `app.asar`) in `apps/desktop/src-tauri`; pre-install container scan and post-install sandbox file-tree scan implemented in `installer-sandbox-check.ps1`. |
| **`INST-08`** | Sandboxed Per-User Silent Install Lifecycle (`asInvoker`, Zero UAC) | `NOT_TESTED` | `AWAITING_INTEGRATION_BUILD` | Sandbox installer runner (`/S /CurrentUser /D=<sandbox>` & `msiexec /a`) ready; awaits RC NSIS/MSI artifact. |
| **`INST-09`** | Start Menu & Desktop Shortcut Creation & Target Verification | `NOT_TESTED` | `AWAITING_INTEGRATION_BUILD` | Shortcut delta detector (`%APPDATA%\Microsoft\Windows\Start Menu\Programs` & Desktop `.lnk`) ready; awaits RC NSIS/MSI artifact. |
| **`INST-10`** | In-Place Update / Upgrade Path Verification | `NOT_TESTED` | `AWAITING_INTEGRATION_BUILD` | Supports `-UpgradeFromPath <prev>` + idempotent in-place upgrade over existing sandbox installation; awaits RC NSIS/MSI artifact. |
| **`INST-11`** | User Profile Data Retention Across Upgrade & Uninstall (Zero Profile Deletion) | `NOT_TESTED` | `AWAITING_INTEGRATION_BUILD` | Real `%LOCALAPPDATA%\MultiZaloDesktop` verified untouched (`unchanged=true`); sandbox sentinel profile (`p-retain01` SHA-256 invariance across upgrade + uninstall) armed for RC run. |
| **`INST-12`** | Clean Silent Uninstall Lifecycle | `NOT_TESTED` | `AWAITING_INTEGRATION_BUILD` | Synchronous NSIS `uninstall.exe /S _?=<sandbox>` + shortcut cleanup verifier armed; awaits RC NSIS/MSI artifact. |
| **`INST-13`** | Primary `%APPDATA%\ZaloData` & Live Primary Zalo Invariance | `PASS` | `VERIFIED_INVARIANT` | `config.json` SHA-256 identical before & after (`be8077e13b0f59e6976a7baa549b836a46569ca397f2bdf5d3e0f3a46be07734`); all `11` running primary `Zalo*` processes untouched. |
| **`INST-14`** | Zero Orphan Manager or Installer Processes Post-Check | `PASS` | `VERIFIED_INVARIANT` | `0` orphan `dim-desktop`, `dim-cli`, or setup processes left running after audit. |

---

## 4. Smart App Control (`SAC`) & Authenticode Policy Gate Behavior

`installer-sandbox-check.ps1` enforces **strict OS policy respect** (ADR-002 & Gate G3) and never alters `HKLM:\SYSTEM\CurrentControlSet\Control\CI\Policy`, Windows Defender, or UAC settings:

| Host `VerifiedAndReputablePolicyState` | Candidate `Get-AuthenticodeSignature` | Harness Behavior & Output Marker | End-User Guidance |
| :--- | :--- | :--- | :--- |
| **`1` (`ENFORCED`)** | `NotSigned` (`UNSIGNED_DEVELOPMENT_ARTIFACT`) or `NotTrusted` | **Halts before process creation** (and catches Win32 `4551` / `0x800711C7` if blocked by WDAC), emitting `overall_gate_status: "BLOCKED"` and `workflow_status: "BLOCKED_BY_PLATFORM_POLICY"`. | **Never disable SAC** (irreversible on Windows 11 without OS reset). Use Signed-Host Bridge (`node apps/desktop/ui-server.mjs` + `scripts/dim-host.ps1`) until Gate G3 OV/EV or Azure Trusted Signing lands. |
| **`0` (`OFF`)** *(Current QA Host)* or **`2` (`EVALUATION`)** | `NotSigned` (`UNSIGNED_DEVELOPMENT_ARTIFACT`) | Executes isolated temp-sandbox silent install/upgrade/uninstall, records `INST-08..12` results, and keeps `INST-03` & overall release gate **`BLOCKED`** (`INSTALLER_VERIFIED_UNSIGNED_DEV_BUILD`) until signed. | Suitable for internal QA evaluation (`INTERNAL_TEST_BUILD`) only; **not** eligible for public Windows 11 end-user distribution. |
| **`0`, `1`, or `2`** | `Valid` (`VALID_AUTHENTICODE_SIGNED` + RFC 3161 timestamp) | Executes full sandbox install, shortcut, upgrade, profile retention, VNG binary scan, and uninstall verification; advances `INST-03` & `INST-08..12` to **`PASS`**. | Eligible for Gate Track A6/A7 sign-off once paired with human L3 QR gate (`A5`). |

### Harness Self-Verification Evidence (Executed Non-Destructively)
- **Negative Test 1 — Raw Executable Rejection (`Gate G2`):** Running `installer-sandbox-check.ps1 -CandidatePath "$env:TEMP\zalo-qa-win-acceptance-target\release\dim-desktop.exe" -NoWriteJson` returned **`exit 1`**, `overall_gate_status: "FAIL"`, `workflow_status: "NOT_AN_INSTALLER_RAW_EXECUTABLE"`.
- **Negative Test 2 — `SAC=1` Policy Block (`Gate G3`):** Running `installer-sandbox-check.ps1 -CandidatePath <unsigned-setup.exe> -SimulatedSacState 1 -NoWriteJson` returned **`exit 0`**, `overall_gate_status: "BLOCKED"`, `workflow_status: "BLOCKED_BY_PLATFORM_POLICY"` without spawning the unsigned executable.

---

## 5. Step-by-Step Execution Runbook When Release Captain Delivers RC Installer

Once `BLK-01`, `BLK-02`, `BLK-03`, and `BLK-04` are resolved and the `windows-2022` CI job produces `Multi-Zalo Desktop_0.1.0_x64-setup.exe` and `Multi-Zalo Desktop_0.1.0_x64_en-US.msi` from `<FINAL_INTEGRATION_SHA>`:

1. **Download RC Bundle Artifacts to Temp Staging (Outside Git Worktree):**
   ```powershell
   $RcStage = Join-Path $env:TEMP "dim-rc-installer-stage"
   New-Item -ItemType Directory -Force -Path $RcStage | Out-Null
   # Place Multi-Zalo Desktop_0.1.0_x64-setup.exe and Multi-Zalo Desktop_0.1.0_x64_en-US.msi into $RcStage
   ```
2. **Run Static Preflight Only (Verify SHA-256, `is_installer: true`, Signature, SAC, Zero VNG Payload):**
   ```powershell
   powershell -NoProfile -ExecutionPolicy Bypass `
     -File desktop-instance-manager/tests/windows-acceptance/installer-sandbox-check.ps1 `
     -CandidatePath "$RcStage\Multi-Zalo Desktop_0.1.0_x64-setup.exe" `
     -DryRunOnly `
     -JsonOut "$RcStage\nsis-preflight.json"
   ```
3. **Execute Full Per-User Sandbox Install -> Upgrade -> Data-Safe Uninstall Lifecycle (NSIS):**
   ```powershell
   powershell -NoProfile -ExecutionPolicy Bypass `
     -File desktop-instance-manager/tests/windows-acceptance/installer-sandbox-check.ps1 `
     -CandidatePath "$RcStage\Multi-Zalo Desktop_0.1.0_x64-setup.exe" `
     -JsonOut desktop-instance-manager/tests/windows-acceptance/evidence/installer-env-audit.json
   ```
   *(Optional upgrade test from a prior version: append `-UpgradeFromPath "$RcStage\prev-setup.exe"`.)*
4. **Execute MSI Administrative Sandbox Extraction & Payload Audit:**
   ```powershell
   powershell -NoProfile -ExecutionPolicy Bypass `
     -File desktop-instance-manager/tests/windows-acceptance/installer-sandbox-check.ps1 `
     -CandidatePath "$RcStage\Multi-Zalo Desktop_0.1.0_x64_en-US.msi" `
     -JsonOut "$RcStage\msi-audit.json"
   ```
5. **Confirm Post-Run Acceptance Exit Criteria:**
   - `INST-01`, `INST-02`, `INST-05`, `INST-07`, `INST-08`, `INST-09`, `INST-10`, `INST-11`, `INST-12`, `INST-13`, `INST-14` == **`PASS`**.
   - `safety_invariants.primary_zalo_unchanged == true` and `safety_invariants.zero_orphans == true`.
   - `INST-03` == `BLOCKED` (`UNSIGNED_DEVELOPMENT_ARTIFACT` -> internal build only) until Gate G3 OV/EV or Azure Trusted Signing produces `VALID_AUTHENTICODE_SIGNED` (`PASS`).
