# Windows 10/11 Packaging & Distribution Guide (`dim-desktop` & `dim-cli`)

**Document Scope:** Canonical MSVC vs local UCRT GNU toolchain architecture, prerequisites, 7-stage build/runtime-fixture/packaging pipeline, `IR-003`/`IR-006` overlay & atomic patches, PE32+ & manifest forensics, real `cargo-deny` RustSec advisory posture, and Windows Smart App Control (SAC) governance for **Multi-Zalo Desktop — Instance Manager** on Windows 10/11 (64-bit `x86_64`).  
**Primary Canonical CI Target:** **`x86_64-pc-windows-msvc`** (GitHub Actions `windows-2022`, Priority #2 CI job `windows-msvc-build`, `$0.010/min` / `2x` multiplier).  
**Local Workstation Verification Target:** `x86_64-pc-windows-gnu` (WinLibs MinGW-w64 UCRT64 `gcc 16.2.0` on Windows 11 Pro `10.0.26300` — see [`windows-local-build-report.md`](../../plans/reports/build/windows-local-build-report.md)).  
**Current Artifact Signing & Provenance State:** **`UNSIGNED_DEVELOPMENT_ARTIFACT`** (`Authenticode = NotSigned`, `provenance_trust_level: "SELF_REPORTED_UNATTESTED"`) — **strictly for development/CI validation, NEVER a production release**.  
**Windows Zalo Runtime Support Status:** **`PARTIALLY_SUPPORTED`** overall (`SUPPORTED_AND_TESTED` for L1/L2 multi-instance isolation, primary protection, and atomic backup/restore against user-installed official VNG Zalo Desktop; `BLOCKED` for concurrent voice/video calls and standalone unsigned PE execution under Smart App Control).

---

> [!IMPORTANT]
> **CRITICAL HONESTY & SAFETY RULES FOR WINDOWS ARTIFACTS**
>
> 1. **Compiled Executables Are NOT Installers:** `dim-desktop.exe`, `dim-cli.exe`, and `dim-test-fixture.exe` under `target/*/release/` are raw **compiled executables (`is_installer: false`)**. Only WiX `.msi` (`bundle/msi/*.msi`) and NSIS setup bundles (`bundle/nsis/*-setup.exe`) are **installers (`is_installer: true`)**.
> 2. **Unsigned Artifacts & Self-Reported Provenance:** Every locally compiled or CI-built Windows binary/installer lacking a trusted Authenticode signature (`IMAGE_DIRECTORY_ENTRY_SECURITY` empty) is strictly **`UNSIGNED_DEVELOPMENT_ARTIFACT`**, and local build manifests carry **`provenance_trust_level: "SELF_REPORTED_UNATTESTED"`** (`oidc_slsa_attestation_present: false`; an SBOM is a declared package inventory, **never** independent build provenance).
> 3. **Zero Smart App Control (SAC) or Execution-Policy Bypasses:** **Never** disable Windows Smart App Control (`VerifiedAndReputablePolicyState`), **never** weaken Windows Defender/WDAC policies, and **never** use signed PowerShell scripts or LOLBins to reflective-load or bypass OS application control for unsigned executables.
> 4. **Zero VNG Binary Policy (`vng_binary_embedded: false`):** Zero `Zalo.exe`, `ZaloCall.exe`, `ZaloCap.exe`, `Update.exe`, `ZaloSetup*.exe`, or `app.asar` payloads from VNG Corporation are ever bundled, copied, modified, or redistributed. End-users install official Zalo Desktop directly from VNG into `%LOCALAPPDATA%\Programs\Zalo\`, and `dim-platform-windows` verifies VNG's cryptographic `WinVerifyTrust` signature (`CN`/`O=VNG GROUP JSC`) before every launch.
> 5. **Remote CI Blocker Honesty:** `.github/workflows/cross-platform-validation.yml` (`windows-msvc-build` on `windows-2022`) passes `validate-workflow.mjs` (`6/6 PASS`), while remote GitHub Actions execution is **`BLOCKED`** (`0` remote runs — `origin/main` at `8cf4703` lacks workflow registration on `main` and local `gh` OAuth scopes `'gist, read:org, repo'` lack `'workflow'`; see [`ci-execution-and-artifact-validation.md`](./ci-execution-and-artifact-validation.md)).

---

## 1. Target Architecture & Toolchain Rationale (`x86_64-pc-windows-msvc` vs `x86_64-pc-windows-gnu`)

### 1.1. Why `x86_64-pc-windows-msvc` on `windows-2022` Is Canonical
All production and CI Windows builds target **`x86_64-pc-windows-msvc`** on explicit **`windows-2022`** runners (`timeout-minutes: 30`, fail-closed hardware/toolchain gate verifying `$env:PROCESSOR_ARCHITECTURE -eq "AMD64"`, `rustc -vV` `host: x86_64-pc-windows-msvc`, and `vswhere.exe` `Microsoft.VisualStudio.Component.VC.Tools.x86.x64`):

| Engineering Dimension | Canonical `x86_64-pc-windows-msvc` (`link.exe` + `rc.exe`/`mt.exe`) | Local Workstation `x86_64-pc-windows-gnu` (MinGW-w64 `ld.exe 2.47`) |
| :--- | :--- | :--- |
| **PE `.rsrc` Manifest Merging (`IR-006`)** | **Clean manifest linking:** `rc.exe`/`mt.exe` links resources cleanly; applying [`0006-IR-006-release-profile-and-windows-manifest.patch`](../../plans/reports/build/ir-patches/0006-IR-006-release-profile-and-windows-manifest.patch) embeds a single unified `WindowsAttributes::app_manifest` with `asInvoker`, Common Controls v6, `PerMonitorV2` DPI awareness, `longPathAware`, and Windows 10/11 GUID `{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}`. | **Duplicate manifest collision:** `tauri-build v2.7.5` emits a Common Controls v6 manifest while `rustc` injects a second `asInvoker` + `longPathAware` manifest, causing GNU `ld.exe` `.rsrc merge failure: multiple non-default manifests` (`blocks_count: 2`). |
| **PE Exploit Mitigations (`GUARD_CF`)** | Supports **Control Flow Guard** (`IMAGE_DLLCHARACTERISTICS_GUARD_CF` `0x4000` via `-C control-flow-guard=checks` in `0006-IR-006`) plus `HIGH_ENTROPY_VA \| DYNAMIC_BASE \| NX_COMPAT` (`0x0160`). | Emits `0x0160` (`HIGH_ENTROPY_VA \| DYNAMIC_BASE \| NX_COMPAT`) without `GUARD_CF`. |
| **Debug Symbols & Binary Footprint** | Separates `.pdb` debug symbols cleanly from stripped `.exe` binaries (`WinDbg`/WER compatible). | Retains DWARF sections unless `[profile.release] strip = "symbols"` is applied (`dim-desktop.exe` = `21,586,008` B unstripped). |
| **Tauri v2 WiX & NSIS Ecosystem** | First-class upstream ABI for `tauri-build v2.7.5`, `webview2-com v0.39.1`, WiX Toolset v3.14 (`.msi`), and NSIS 3.x (`-setup.exe`). | Compiles and passes native Windows x64 unit/fixture tests cleanly, but secondary for official MSI/NSIS release pipelines. |

### 1.2. Local Workstation vs `windows-2022` CI Reality (`DIM-WIN-BUILD-20261118-E2`)
Per [`windows-local-build-report.md`](../../plans/reports/build/windows-local-build-report.md):
- On the local Windows 11 Pro (`10.0.26300` x64) workstation, `rustc 1.100.0 (0ef1bc386 2026-11-09)` runs natively on `host: x86_64-pc-windows-gnu` (WinLibs MinGW-w64 UCRT64 `gcc 16.2.0`), while Visual Studio C++ Build Tools, Windows 10/11 SDK, `@tauri-apps/cli@2.12.5`, WiX, and NSIS are absent (`cargo check --workspace --target x86_64-pc-windows-msvc --locked` exited `101` `E0463`).
- Without downloading unverified third-party binaries, `@win-builder` executed full `--locked` formatting, `--locked` Clippy (`-D warnings`), `--locked` unit tests (`29/29 PASS`, `PASSED_NATIVE`), native runtime fixture smoke (`fixture-app.mjs` + `dim-cli.exe`, `PASSED_NATIVE_FIXTURE_AND_DIM_CLI`), real `cargo-deny 0.21.0` RustSec advisory scanning, and PE32+ verification on native `x86_64-pc-windows-gnu` (`bundle_status: "SKIPPED_MISSING_TOOLING"` locally).
- On GitHub Actions `windows-2022`, MSVC v143, Windows 10/11 SDK, WiX v3.14, NSIS 3.x, and WebView2 Evergreen are preinstalled.

---

## 2. Windows Build & Packaging Prerequisites

### 2.1. Required Developer / CI Toolchains
1. **Rust Toolchain (`>= 1.90`, `--locked` mandatory):**
   - [`apps/desktop/src-tauri/Cargo.toml`](../../apps/desktop/src-tauri/Cargo.toml) enforces `rust-version = "1.90"` (`tauri v2.12.6`, `tauri-build v2.7.5`, `wry v0.57.2`). Workspace `Cargo.toml` alignment (`1.85` -> `1.90`) is provided in [`0001-IR-001-align-workspace-msrv-1.90.patch`](../../plans/reports/build/ir-patches/0001-IR-001-align-workspace-msrv-1.90.patch).
   ```powershell
   rustup toolchain install stable-x86_64-pc-windows-msvc --profile minimal --component rustfmt clippy
   rustup target add x86_64-pc-windows-msvc
   ```
2. **Visual Studio 2022 C++ Build Tools & Windows 10/11 SDK:**
   - Install `Microsoft.VisualStudio.Workload.VCTools` (`cl.exe`, `link.exe`) + Windows 10/11 SDK (`rc.exe`, `mt.exe`, `signtool.exe`, `kernel32.lib`, `user32.lib`, `advapi32.lib`, `crypt32.lib`, `wintrust.lib`, `rstrtmgr.lib`, `userenv.lib`, `shell32.lib`).
3. **Node.js `>= 22` LTS (`CN=OpenJS Foundation`):**
   - Runs `scripts/ci-cross-platform/*.mjs`, `fixtures/test-desktop-app/fixture-app.mjs`, the SAC Signed-Host Bridge (`apps/desktop/ui-server.mjs`), and pinned `@tauri-apps/cli`.
4. **Exact Pinned Tauri CLI (`@tauri-apps/cli@2.12.5`):**
   - Required for `.msi` and NSIS `-setup.exe` bundling (`IR-004` — never use unpinned `@latest`):
   ```powershell
   npm.cmd install --global "@tauri-apps/cli@2.12.5"
   tauri.cmd --version
   ```
5. **Installer Bundlers (Preinstalled on `windows-2022`):**
   - **WiX Toolset v3 (`v3.14`):** `candle.exe` and `light.exe` for `.msi` generation.
   - **NSIS v3 (`3.x`):** `makensis.exe` for `-setup.exe` generation.

### 2.2. WebView2 Evergreen Runtime, `WebView2Loader.dll` & `IR-003` Overlay
- **System Runtime Detection (Read-Only):** `dim-desktop.exe` uses Microsoft's out-of-process **Edge WebView2 Evergreen Runtime** (AppId `{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}`; verified read-only on the local host at `pv = 157.0.4341.46` under `HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}`).
- **`WebView2Loader.dll` Sidecar:** `webview2-com-sys v0.39.1` places Microsoft-signed `WebView2Loader.dll` (`160,880` B, SHA-256 `86545b66cdb0603bc26b626fb9ad610cb6e71f28d468f5ea66df23b03dda96d5`, `CN=Microsoft Corporation`) next to `dim-desktop.exe`. Raw `dim-desktop.exe` execution requires `WebView2Loader.dll` in the same folder (`imports_webview2_loader: true`).
- **`downloadBootstrapper` Overlay & `0003-IR-003` Patch:**
  - At CI packaging time, [`scripts/ci-cross-platform/tauri-ci-bundle-overlay.json`](../../scripts/ci-cross-platform/tauri-ci-bundle-overlay.json) configures `"windows": { "webviewInstallMode": { "type": "downloadBootstrapper", "silent": true } }` via `tauri build --config` without modifying shared `tauri.conf.json`.
  - Standalone patch [`0003-IR-003-tauri-os-bundle-metadata.patch`](../../plans/reports/build/ir-patches/0003-IR-003-tauri-os-bundle-metadata.patch) permanently adds `webviewInstallMode` (`downloadBootstrapper`), `digestAlgorithm: "sha256"`, `wix.language: "en-US"`, and `nsis.installMode: "currentUser"` to `tauri.conf.json`.

---

## 3. Native 7-Stage Build, Runtime Fixture & Provenance Commands

### 3.1. Automated Cross-Platform Tooling Pipeline (`scripts/ci-cross-platform/*.mjs`)
```powershell
$OutDir = Join-Path $env:TEMP "dim-windows-msvc"
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

# 0. Run script header/manifest self-tests (16/16 PASS) & lean workflow validator (6/6 PASS)
node desktop-instance-manager/scripts/ci-cross-platform/selftest.mjs
node desktop-instance-manager/scripts/ci-cross-platform/validate-workflow.mjs

# 1. Check native AMD64 host, Rust >= 1.90, MSVC cl.exe/link.exe, WebView2 Evergreen, @tauri-apps/cli, WiX & NSIS
node desktop-instance-manager/scripts/ci-cross-platform/check-prerequisites.mjs `
  --target x86_64-pc-windows-msvc `
  --out "$OutDir/prereq.json"

# 2. Run 7-pillar dependency & real cargo-deny RustSec advisory audit across both Cargo.lock files (439 unique crates)
node desktop-instance-manager/scripts/ci-cross-platform/dependency-audit.mjs `
  --out-json "$OutDir/dependency-audit.json" `
  --out-md "$OutDir/dependency-audit.md"

# 3–6. Run --locked Clippy, --locked unit tests, release compile, native runtime fixture smoke, and MSI/NSIS bundle via overlay
node desktop-instance-manager/scripts/ci-cross-platform/build-and-package.mjs `
  --target x86_64-pc-windows-msvc `
  --bundles all `
  --out "$OutDir/build-summary.json"

# 7. Inspect PE32+ headers/manifests & generate Section 7 manifest (schema v2) + SHA256SUMS.txt
node desktop-instance-manager/scripts/ci-cross-platform/inspect-artifact.mjs `
  --file "desktop-instance-manager/apps/desktop/src-tauri/target/x86_64-pc-windows-msvc/release/dim-desktop.exe" `
  --out "$OutDir/inspect-dim-desktop.json"

node desktop-instance-manager/scripts/ci-cross-platform/generate-artifact-manifest.mjs `
  --scan-default-targets `
  --target x86_64-pc-windows-msvc `
  --build-summary "$OutDir/build-summary.json" `
  --os-label "windows-2022" `
  --arch x86_64 `
  --build-log-url "local-dev" `
  --out-manifest "$OutDir/artifact-manifest.json" `
  --out-sha256 "$OutDir/SHA256SUMS.txt" `
  --out-md "$OutDir/artifact-summary.md"
```

### 3.2. Verified Local Windows Build, Native Fixture & Advisory Baseline (`ac0df10`, `2026-11-18`)
Recorded from live execution on Windows 11 Pro (`10.0.26300`, `rustc 1.100.0`, native `x86_64-pc-windows-gnu` — full logs in [`windows-local-build-report.md`](../../plans/reports/build/windows-local-build-report.md) and [`dependency-audit-summary.md`](../../plans/reports/build/dependency-audit-summary.md)):
- **Formatting & `--locked` Clippy (`-D warnings`):** `PASS` (`0.33s` workspace fmt, `14.26s` workspace clippy `0` warnings, `0.19s` Tauri fmt, `93.12s` Tauri clippy `0` warnings).
- **Workspace Unit Tests (`--locked`):** **`29 / 29 PASS` (`PASSED_NATIVE`, `16.80s`)** across `dim-core` (9), `dim-platform-windows` (6, including live `WinVerifyTrust` Authenticode + X.500 `VNG GROUP JSC` RDN pinning + `TokenElevation` + NTFS DACL), `dim-profile-store` (4, including NTFS junction `mklink /J` safe unlink), `dim-platform-macos` (3), `dim-platform-linux` (3), `dim-process-manager` (2), and `dim-test-fixture` (2); plus `src-tauri` test harness (`138.84s`, `0` failures).
- **Native Runtime Fixture & `dim-cli.exe` Smoke (`PASSED_NATIVE_FIXTURE_AND_DIM_CLI`):** `node fixtures/test-desktop-app/fixture-app.mjs --stay-ms=100` (`zalo_user_id: "100001"`) + `dim-cli.exe --cli --json --data-dir <temp>/dim-data status` (`ok: true`, `platform: "windows"`) passed in `0.27s`.
- **Real `cargo-deny 0.21.0` RustSec Vulnerability Advisory Scan:** **`VERIFIED_CARGO_DENY_RUSTSEC_CLEAN`** against live `rustsec/advisory-db` @ `233155b4025b854c68a4b199c0e32d7fd53291a2` across both `Cargo.lock` (`41` crates) and `apps/desktop/src-tauri/Cargo.lock` (`434` crates; **`439` unique crates**, `431/431` `crates.io-index` SHA-256 checksums): **`0` vulnerabilities, `0` yanked crates, `0` banned OpenSSL crates, `0` SPDX license violations**.

---

## 4. Strict Classification: Compiled Executables vs MSI/NSIS Installers

Raw `.exe` files emitted by `cargo build --release` and packaged installer bundles emitted by `@tauri-apps/cli@2.12.5` **must never be conflated**:

| Artifact Category | Path Pattern (under `desktop-instance-manager/`) | `artifact_kind` | `is_installer` | Local Verified Build (`x86_64-pc-windows-gnu`, [`SHA256SUMS.txt`](../../plans/reports/build/SHA256SUMS.txt)) |
| :--- | :--- | :--- | :---: | :--- |
| **Desktop Manager GUI Executable** *(NOT an installer)* | `apps/desktop/src-tauri/target/<triple>/release/dim-desktop.exe` | `compiled-executable` | **`false`** | `21,586,008` B — SHA-256 `157117cbce3a4309cedb9f279fbce869580f75d8120b33550d445a579bf53f4c` (`UNSIGNED_DEVELOPMENT_ARTIFACT` / `SELF_REPORTED_UNATTESTED`) |
| **Native CLI Parity Runner** *(NOT an installer)* | `target/<triple>/release/dim-cli.exe` | `compiled-executable` | **`false`** | `2,209,966` B — SHA-256 `24132f61429179962f216e200cea04cc52a36a7f675f6408c460764c67c07c98` (`UNSIGNED_DEVELOPMENT_ARTIFACT` / `SELF_REPORTED_UNATTESTED`) |
| **Synthetic Isolation Test Fixture** *(NOT an installer)* | `target/<triple>/release/dim-test-fixture.exe` | `compiled-executable` | **`false`** | `1,616,671` B — SHA-256 `4ea0f248b17d087a6a630d9ac58c2f1e862ea9d86784a18164075289ff1bba7c` (`UNSIGNED_DEVELOPMENT_ARTIFACT` / `SELF_REPORTED_UNATTESTED`) |
| **WebView2 Loader Sidecar DLL** *(NOT an installer)* | `apps/desktop/src-tauri/target/<triple>/release/WebView2Loader.dll` | `shared-library-sidecar` | **`false`** | `160,880` B — SHA-256 `86545b66cdb0603bc26b626fb9ad610cb6e71f28d468f5ea66df23b03dda96d5` (Vendor-signed `CN=Microsoft Corporation`) |
| **NSIS Per-User Installer** *(Installer)* | `apps/desktop/src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/Multi-Zalo Desktop_0.1.0_x64-setup.exe` | `installer-nsis` | **`true`** | `SKIPPED_MISSING_TOOLING` locally (`makensis.exe` & `@tauri-apps/cli` absent); configured on CI `windows-2022` (`currentUser` user-scope install in `IR-003`) |
| **WiX Windows Installer** *(Installer)* | `apps/desktop/src-tauri/target/x86_64-pc-windows-msvc/release/bundle/msi/Multi-Zalo Desktop_0.1.0_x64_en-US.msi` | `installer-msi` | **`true`** | `SKIPPED_MISSING_TOOLING` locally (`candle.exe`/`light.exe` & `@tauri-apps/cli` absent); configured on CI `windows-2022` |

---

## 5. PE32+ Header & Embedded XML Manifest Forensics

Verified on disk via [`inspect-artifact.mjs`](../../scripts/ci-cross-platform/inspect-artifact.mjs) and recorded in [`artifact-validation-manifest.json`](../../plans/reports/build/artifact-validation-manifest.json) (`schema_version: 2`):
1. **PE32+ (`0x020b`) & COFF `AMD64` (`0x8664`):** `dim-desktop.exe` builds as `WINDOWS_GUI` (subsystem `2`, no background console window) with `HIGH_ENTROPY_VA | DYNAMIC_BASE | NX_COMPAT` (`0x0160`), importing `WebView2Loader.dll` and Windows UCRT (`api-ms-win-crt-*.dll`) with zero MinGW runtime DLL dependencies; `dim-cli.exe` and `dim-test-fixture.exe` build as `WINDOWS_CUI` (subsystem `3`).
2. **Strict Standard-User Execution (`asInvoker`, Zero UAC Tricks):** All three binaries embed `<requestedExecutionLevel level="asInvoker" uiAccess="false"/>` and `<longPathAware>true</longPathAware>`. Zero `requireAdministrator` / `highestAvailable` elevation prompts, zero kernel drivers, and zero `HKLM` writes are ever used.
3. **Unified DPI Awareness & Windows 10/11 GUIDs (`0006-IR-006` Patch):** Inspection of `tauri-build v2.7.5` (`windows-app-manifest.xml`) showed Tauri's default resource only declares Common Controls v6 while `rustc` injects a separate `asInvoker` + `longPathAware` manifest. Patch [`0006-IR-006-release-profile-and-windows-manifest.patch`](../../plans/reports/build/ir-patches/0006-IR-006-release-profile-and-windows-manifest.patch) configures `tauri_build::WindowsAttributes::new().app_manifest(...)` in `apps/desktop/src-tauri/build.rs` so `dim-desktop.exe` embeds a single clean manifest combining `asInvoker`, Common Controls v6, `PerMonitorV2` DPI awareness, `longPathAware=true`, and Windows 10/11 `<supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/>`, alongside `-C control-flow-guard=checks` (`GUARD_CF`) on `x86_64-pc-windows-msvc`.

---

## 6. Security, Authenticode Signing & Windows Smart App Control (SAC) Policy

### 6.1. Windows Smart App Control (SAC) Governance
Windows 11 **Smart App Control** (`HKLM:\SYSTEM\CurrentControlSet\Control\CI\Policy\VerifiedAndReputablePolicyState = 1`) blocks unsigned PE executables (`.exe`, `.dll`, `.msi`, `.node`) at kernel load time unless signed by a trusted Authenticode certificate with reputation.
- **Never Disable SAC:** Never modify `HKLM:\SYSTEM\CurrentControlSet\Control\CI\Policy` and never instruct users to turn off Smart App Control or Windows Defender (disabling SAC on Windows 11 is irreversible without an OS reset).
- **Never Use Signed PowerShell Bypasses:** Never use signed scripts, LOLBins, or reflection to load unsigned native PE bytes around WDAC/SAC.
- **Approved SAC-Compliant Local Path Prior to Gate G3 Signing:** Until Gate G3 Authenticode signing is provisioned, users on SAC-enforcing Windows 11 hosts run ADR-002's clean-room **Signed-Host Bridge** (`node apps/desktop/ui-server.mjs` + `scripts/dim-host.ps1` inside Microsoft-signed `powershell.exe` and OpenJS-signed `node.exe`; see [`windows-mvp-guide.md`](../user-guide/windows-mvp-guide.md)).

### 6.2. Production Authenticode Signing Requirements (Gate G3)
Before promoting any Windows binary or installer beyond `UNSIGNED_DEVELOPMENT_ARTIFACT` (Gate G3 in [`signing-and-release-gates.md`](./signing-and-release-gates.md)):
1. Provision an **EV/OV Code Signing Certificate** (FIPS 140-2 Level 3 HSM) or **Azure Trusted Signing** identity.
2. Dual/multi-layer sign inner `dim-desktop.exe` and `dim-cli.exe`, NSIS's generated `uninstall.exe` (via Tauri `signCommand`), and outer `*-setup.exe` / `*.msi` installers with SHA-256 and RFC 3161 trusted timestamping (`signtool.exe sign /fd sha256 /tr http://timestamp.acs.microsoft.com /td sha256`).
3. Verify cryptographically via `signtool.exe verify /pa /v` and `Get-AuthenticodeSignature` (`Status == Valid`).

---

## 7. Verified Atomic Integration Patches (`plans/reports/build/ir-patches/`)

All 7 upstream configuration enhancements are packaged as standalone single-concern `git format-patch` files in [`plans/reports/build/ir-patches/`](../../plans/reports/build/ir-patches/) (verified `git -C <REPO_ROOT> apply --check` **`exit 0`** both individually and sequentially across all 6 active worktrees — full audit in [`integration-requests.md`](../../plans/reports/build/integration-requests.md)):

| Atomic Patch File | Severity | Windows & Cross-Platform Impact |
| :--- | :---: | :--- |
| [`0001-IR-001-align-workspace-msrv-1.90.patch`](../../plans/reports/build/ir-patches/0001-IR-001-align-workspace-msrv-1.90.patch) | Medium | Aligns root `Cargo.toml` `rust-version = "1.90"` with `src-tauri/Cargo.toml` and refreshes ADR-002. |
| [`0002-IR-002-macos-icns-and-retina-icons.patch`](../../plans/reports/build/ir-patches/0002-IR-002-macos-icns-and-retina-icons.patch) | High | Adds valid `icon.icns` (`1,142` B) & `128x128@2x.png` (`716` B) to `src-tauri/icons/` and `tauri.conf.json` (mitigated today by `tauri-ci-bundle-overlay.json`). |
| [`0003-IR-003-tauri-os-bundle-metadata.patch`](../../plans/reports/build/ir-patches/0003-IR-003-tauri-os-bundle-metadata.patch) | High | Adds explicit `bundle.windows` (`downloadBootstrapper`, `digestAlgorithm: "sha256"`, `nsis.installMode: "currentUser"`), `bundle.macOS`, and `bundle.linux` blocks. |
| [`0004-IR-004-pin-tauri-cli-and-lockfile-reproducibility.patch`](../../plans/reports/build/ir-patches/0004-IR-004-pin-tauri-cli-and-lockfile-reproducibility.patch) | Medium | Pins `@tauri-apps/cli@2.12.5`, removes deprecated `version = 2` in `deny.toml` (`cargo-deny 0.21.0`), sets `publish = false` on `dim-desktop`, and adds `rust-toolchain.toml`. |
| [`0005-IR-005-src-tauri-workspace-lints-parity.patch`](../../plans/reports/build/ir-patches/0005-IR-005-src-tauri-workspace-lints-parity.patch) | Medium | Enforces `unsafe_op_in_unsafe_fn = "deny"`, `unwrap_used = "warn"`, `expect_used = "warn"` in `src-tauri` and removes `.expect(...)` in `src/main.rs`. |
| [`0006-IR-006-release-profile-and-windows-manifest.patch`](../../plans/reports/build/ir-patches/0006-IR-006-release-profile-and-windows-manifest.patch) | Low | Adds `[profile.release]` (`lto = "thin"`, `strip = "symbols"`), MSVC `GUARD_CF`, and unified `WindowsAttributes::app_manifest` (`asInvoker`, `PerMonitorV2`, Win10/11 GUID). |
| [`0007-IR-007-ci-yml-locked-and-ubuntu2404-ayatana.patch`](../../plans/reports/build/ir-patches/0007-IR-007-ci-yml-locked-and-ubuntu2404-ayatana.patch) | Medium | Enforces `--locked` and dual-`Cargo.lock` caching across push/PR `.github/workflows/ci.yml` and updates Ubuntu 24.04 Ayatana package. |

---

## 8. Related Documentation
- [CI Execution & Artifact Validation Report (`ci-execution-and-artifact-validation.md`)](./ci-execution-and-artifact-validation.md)
- [macOS Packaging Guide (`macos.md`)](./macos.md)
- [Ubuntu 24.04 LTS Packaging Guide (`ubuntu.md`)](./ubuntu.md)
- [Cross-Platform Build Matrix (`build-matrix.md`)](./build-matrix.md)
- [Code Signing & Release Gates (`signing-and-release-gates.md`)](./signing-and-release-gates.md)
- [Release Readiness Scorecard (`release-readiness.md`)](./release-readiness.md)
- [Windows Local Build Report (`windows-local-build-report.md`)](../../plans/reports/build/windows-local-build-report.md)
- [Artifact Validation Manifest (`artifact-validation-manifest.json`)](../../plans/reports/build/artifact-validation-manifest.json)
- [Out-of-Scope Integration Requests & Atomic Patches (`integration-requests.md`)](../../plans/reports/build/integration-requests.md)
- [Windows MVP User Guide (`windows-mvp-guide.md`)](../user-guide/windows-mvp-guide.md)
