# Windows 10/11 Packaging & Distribution Guide (`dim-desktop` & `dim-cli`)

**Document Scope:** Toolchain rationale, prerequisites, reproducible build/test/packaging workflows, PE32+ & manifest forensics, Smart App Control (SAC) governance, and Authenticode signing gates for **Multi-Zalo Desktop — Instance Manager** on Windows 10/11 (64-bit x64).  
**Primary Canonical Target:** **`x86_64-pc-windows-msvc`** (GitHub Actions `windows-latest`; MSVC ABI + Windows 10/11 SDK).  
**Local Workstation Verification Target:** `x86_64-pc-windows-gnu` (WinLibs MinGW-w64 UCRT64 `gcc 16.2.0` on Windows 11 Pro `10.0.26300` — see Section 1.2 & [`windows-local-build-report.md`](../../plans/reports/build/windows-local-build-report.md)).  
**Current Artifact Signing State:** **`UNSIGNED_DEVELOPMENT_ARTIFACT`** (`Authenticode = NotSigned`) — **strictly for development/CI validation, NEVER a production release**.  
**Windows Zalo Runtime Support Status:** **`PARTIALLY_SUPPORTED`** overall (`SUPPORTED_AND_TESTED` for L1/L2 multi-instance isolation, primary protection, and atomic backup/restore against user-installed official VNG Zalo Desktop; `BLOCKED` for concurrent voice/video calls and standalone unsigned PE execution under Smart App Control).

---

> [!IMPORTANT]
> **CRITICAL HONESTY & SAFETY RULES FOR WINDOWS ARTIFACTS**
>
> 1. **Compiled Executables Are NOT Installers:** `dim-desktop.exe`, `dim-cli.exe`, and `dim-test-fixture.exe` under `target/*/release/` are raw **compiled executables (`is_installer: false`)**. Only WiX `.msi` (`bundle/msi/*.msi`) and NSIS setup bundles (`bundle/nsis/*-setup.exe`) are **installers (`is_installer: true`)**.
> 2. **Unsigned Artifacts Are NEVER Production Releases:** Every locally compiled or CI-built Windows binary/installer lacking a trusted Authenticode signature (`IMAGE_DIRECTORY_ENTRY_SECURITY` empty) is strictly classified as **`UNSIGNED_DEVELOPMENT_ARTIFACT`**.
> 3. **Zero Smart App Control (SAC) or Execution-Policy Bypasses:** **Never** disable Windows Smart App Control (`VerifiedAndReputablePolicyState`), **never** weaken Windows Defender/WDAC policies, and **never** use signed PowerShell scripts or LOLBins to reflective-load or bypass OS application control for unsigned executables.
> 4. **Zero VNG Binary Policy (`vng_binary_embedded: false`):** Zero `Zalo.exe`, `ZaloCall.exe`, `ZaloCap.exe`, `Update.exe`, `ZaloSetup*.exe`, or `app.asar` payloads from VNG Corporation are ever bundled, copied, modified, or redistributed. End-users install official Zalo Desktop directly from VNG into `%LOCALAPPDATA%\Programs\Zalo\`, and `dim-platform-windows` verifies VNG's cryptographic `WinVerifyTrust` signature (`CN`/`O=VNG GROUP JSC`) prior to every launch.
> 5. **CI Execution Honesty:** `.github/workflows/cross-platform-validation.yml` (`windows-msvc-build`) is statically validated (`47/47 PASS`), while live GitHub Actions execution was **not** run in this audit (`CI_LIVE_RUN_STATUS: NOT_EXECUTED (STATICALLY_VALIDATED_ONLY)` — see [`ci-baseline-and-github-audit.md`](../../plans/reports/build/ci-baseline-and-github-audit.md)).

---

## 1. Target Architecture & Toolchain Rationale

### 1.1. Why `x86_64-pc-windows-msvc` Is the Canonical Windows Target
All production and CI Windows builds target **`x86_64-pc-windows-msvc`** rather than `x86_64-pc-windows-gnu` for four concrete engineering reasons:

| Engineering Dimension | Canonical `x86_64-pc-windows-msvc` (`link.exe` + `rc.exe`/`mt.exe`) | Local Fallback `x86_64-pc-windows-gnu` (MinGW-w64 `ld.exe` + `windres.exe`) |
| :--- | :--- | :--- |
| **PE `.rsrc` Manifest Merging** | **Clean single-manifest merge:** `rc.exe`/`mt.exe` merges Tauri's `tauri-winres` `RT_MANIFEST` (`asInvoker`, `longPathAware=true`, `PerMonitorV2` DPI awareness, and Windows 10/11 `<supportedOS>` GUIDs) cleanly. | **Duplicate manifest collision (**IR-006**):** GNU `ld.exe 2.47` emits `.rsrc merge failure: multiple non-default manifests` (`blocks_count: 2`), keeping `asInvoker` + `longPathAware` from Rust's driver while dropping Tauri's `PerMonitorV2` DPI and `<supportedOS>` block. |
| **PE Hardening (`GUARD_CF`)** | Supports **Control Flow Guard** (`IMAGE_DLLCHARACTERISTICS_GUARD_CF` `0x4000` via `-C control-flow-guard=checks`) alongside `HIGH_ENTROPY_VA \| DYNAMIC_BASE \| NX_COMPAT` (`0x0160`). | Emits `0x0160` (`HIGH_ENTROPY_VA \| DYNAMIC_BASE \| NX_COMPAT`) without `GUARD_CF`. |
| **Debug Symbols & Binary Size** | Emits external `.pdb` debug databases cleanly separated from the stripped `.exe` payload, compatible with WinDbg and Windows Error Reporting (WER). | Embeds DWARF debug sections directly in the PE image unless explicitly stripped (`dim-desktop.exe` = `21,586,008` bytes unstripped — see **IR-006**). |
| **Tauri v2 Bundler & WebView2 Ecosystem** | First-class upstream target for `tauri-build v2.7.5`, `webview2-com v0.39.1`, WiX Toolset v3 (`.msi`), and NSIS v3 (`-setup.exe`). | Supported for compilation/testing, but secondary to MSVC for official Windows installer toolchains. |

### 1.2. Local Workstation vs GitHub Actions `windows-latest` Reality (`DIM-WIN-BUILD-20261117`)
Per [`windows-local-build-report.md`](../../plans/reports/build/windows-local-build-report.md):
- On the local Windows 11 Pro (`10.0.26300` x64) workstation, `rustc 1.100.0 (0ef1bc386 2026-11-09)` is installed with `host: x86_64-pc-windows-gnu` and WinLibs MinGW-w64 UCRT64 (`gcc 16.2.0`), while Visual Studio C++ Build Tools (`cl.exe`/`link.exe`), the Windows 10/11 SDK, `cargo-tauri`, WiX, and NSIS are **not installed** (`cargo check --workspace --target x86_64-pc-windows-msvc --locked` exited `101` `E0463`, and `check-prerequisites.mjs --target x86_64-pc-windows-msvc` accurately reported `MISSING_COMPILE_PREREQUISITES`).
- Respecting the strict ban on downloading unverified binaries, `@win-builder` executed full `--locked` formatting, Clippy (`-D warnings`), unit tests (`29/29 PASS`), release compilation, and PE32+ header/manifest verification against `x86_64-pc-windows-gnu` (`SKIPPED_MISSING_TOOLING` for local MSI/NSIS bundles).
- On GitHub Actions `windows-latest` (`windows-msvc-build` job in [`.github/workflows/cross-platform-validation.yml`](../../../.github/workflows/cross-platform-validation.yml)), Visual Studio 2022 Enterprise MSVC, Windows 10/11 SDK, WiX Toolset v3.14, NSIS 3.x, and WebView2 Evergreen come preinstalled out of the box.

---

## 2. Windows Build & Packaging Prerequisites

### 2.1. Required Developer / CI Toolchains
1. **Rust Toolchain (`>= 1.90`):**
   - [`apps/desktop/src-tauri/Cargo.toml`](../../apps/desktop/src-tauri/Cargo.toml) enforces `rust-version = "1.90"` (required by locked `tauri v2.12.6`, `tauri-build v2.7.5`, `tauri-runtime-wry v2.12.4`, `wry v0.57.2`).
   - Note: Root [`desktop-instance-manager/Cargo.toml`](../../Cargo.toml) declares `rust-version = "1.85"`; **`1.90`** is the true effective MSRV for building `dim-desktop.exe` (tracked as **IR-001** in [`integration-requests.md`](../../plans/reports/build/integration-requests.md)).
   ```powershell
   rustup toolchain install stable-x86_64-pc-windows-msvc --profile minimal --component rustfmt clippy
   rustup target add x86_64-pc-windows-msvc
   ```
2. **Visual Studio 2022 C++ Build Tools & Windows 10/11 SDK:**
   - Install **Desktop development with C++** (`Microsoft.VisualStudio.Workload.VCTools`), including:
     - MSVC v143 `x64/x86` build tools (`cl.exe`, `link.exe`)
     - Windows 10 SDK (`10.0.19041.0+`) or Windows 11 SDK (`10.0.22621.0+` / `10.0.26100.0+`) providing `rc.exe`, `mt.exe`, `signtool.exe`, and Win32 import libraries (`kernel32.lib`, `user32.lib`, `advapi32.lib`, `crypt32.lib`, `wintrust.lib`, `rstrtmgr.lib`, `userenv.lib`, `shell32.lib`).
3. **Node.js `>= 22` LTS (`CN=OpenJS Foundation`):**
   - Required to run the zero-npm-dependency ESM scripts in [`scripts/ci-cross-platform/`](../../scripts/ci-cross-platform/) and the local Signed-Host Bridge (`apps/desktop/ui-server.mjs`).
4. **Tauri CLI v2 (`cargo-tauri`):**
   - Required only when building `.msi` and NSIS `-setup.exe` installers (raw `dim-desktop.exe` and `dim-cli.exe` compile directly via `cargo build --release`):
   ```powershell
   cargo install tauri-cli --version "^2.0.0" --locked
   ```
   *(Pinning to an exact version such as `=2.5.0` is tracked in **IR-004**.)*
5. **Installer Bundlers (Preinstalled on GitHub Actions `windows-latest`):**
   - **WiX Toolset v3 (`v3.14`):** `candle.exe` and `light.exe` on `PATH` for `.msi` generation.
   - **NSIS v3 (`3.x`):** `makensis.exe` on `PATH` for `-setup.exe` generation.

### 2.2. WebView2 Evergreen Runtime & `WebView2Loader.dll` Behavior
- **System Runtime Detection (Read-Only):** `dim-desktop.exe` renders [`apps/desktop/ui/`](../../apps/desktop/ui/) through Microsoft's out-of-process **Edge WebView2 Evergreen Runtime** (AppId `{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}`), preinstalled on all Windows 11 machines and maintained via Windows Update (verified locally at `157.0.4341.46` under `HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}`).
- **`WebView2Loader.dll` Sidecar Requirement:**
  - During `cargo build --manifest-path apps/desktop/src-tauri/Cargo.toml --release`, `webview2-com-sys v0.39.1` copies Microsoft-signed **`WebView2Loader.dll`** (`160,880` bytes, SHA-256 `86545b66cdb0603bc26b626fb9ad610cb6e71f28d468f5ea66df23b03dda96d5`, `CN=Microsoft Corporation`) into `apps/desktop/src-tauri/target/<triple>/release/WebView2Loader.dll`.
  - When running the raw compiled `dim-desktop.exe` directly from a developer build folder, **`WebView2Loader.dll` MUST remain in the same directory as `dim-desktop.exe`** (verified automatically by `inspect-artifact.mjs` via `imports_webview2_loader` and `sidecar_webview2_loader.present`).
- **Installer Bootstrapper Behavior (`downloadBootstrapper`):**
  - Tauri's MSI and NSIS installers automatically bundle `WebView2Loader.dll` alongside `dim-desktop.exe` and check the registry for WebView2 Evergreen at install time. If missing on an older Windows 10 machine, Tauri's default `downloadBootstrapper` mode downloads Microsoft's official signed `MicrosoftEdgeWebview2Setup.exe` bootstrapper (explicit configuration in `tauri.conf.json` is proposed in **IR-003**).

---

## 3. Exact Build, Test & Provenance Validation Commands

All commands below assume the repository root (`C:\Users\manhquy\Downloads\zalo-cross-platform-build`) unless `cd desktop-instance-manager` is shown.

### 3.1. Automated Cross-Platform Tooling Pipeline (`scripts/ci-cross-platform/*.mjs`)

```powershell
$OutDir = Join-Path $env:TEMP "dim-windows-msvc"
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

# 1. Verify CI script syntax & header/manifest self-tests (15/15 selftest + 47/47 workflow policy checks)
node desktop-instance-manager/scripts/ci-cross-platform/selftest.mjs
node desktop-instance-manager/scripts/ci-cross-platform/validate-workflow.mjs

# 2. Check Rust >= 1.90, MSVC cl.exe/link.exe, WebView2 Evergreen (read-only), Tauri CLI, WiX, and NSIS
node desktop-instance-manager/scripts/ci-cross-platform/check-prerequisites.mjs `
  --target x86_64-pc-windows-msvc `
  --out "$OutDir/prereq.json"

# 3. Audit Cargo.lock (41 crates) + src-tauri/Cargo.lock (434 crates) against deny.toml (439 unique crates)
node desktop-instance-manager/scripts/ci-cross-platform/dependency-audit.mjs `
  --out-json "$OutDir/dependency-audit.json" `
  --out-md "$OutDir/dependency-audit.md"

# 4. Run --locked unit tests, compile release binaries, and build MSI + NSIS bundles (if tooling present)
node desktop-instance-manager/scripts/ci-cross-platform/build-and-package.mjs `
  --target x86_64-pc-windows-msvc `
  --bundles all `
  --out "$OutDir/build-summary.json"

# 5. Inspect PE32+ headers, embedded XML manifest (asInvoker), Authenticode table, and WebView2Loader.dll
node desktop-instance-manager/scripts/ci-cross-platform/inspect-artifact.mjs `
  --file "desktop-instance-manager/apps/desktop/src-tauri/target/x86_64-pc-windows-msvc/release/dim-desktop.exe" `
  --out "$OutDir/inspect-dim-desktop.json"

node desktop-instance-manager/scripts/ci-cross-platform/inspect-artifact.mjs `
  --file "desktop-instance-manager/target/x86_64-pc-windows-msvc/release/dim-cli.exe" `
  --out "$OutDir/inspect-dim-cli.json"

# 6. Emit Section 7 traceable artifact manifest + SHA256SUMS.txt
node desktop-instance-manager/scripts/ci-cross-platform/generate-artifact-manifest.mjs `
  --scan-default-targets `
  --target x86_64-pc-windows-msvc `
  --build-summary "$OutDir/build-summary.json" `
  --os-label "windows-latest" `
  --arch x86_64 `
  --build-log-url "local-dev" `
  --out-manifest "$OutDir/artifact-manifest.json" `
  --out-sha256 "$OutDir/SHA256SUMS.txt" `
  --out-md "$OutDir/artifact-summary.md"
```

### 3.2. Direct Cargo & Tauri Verification Commands (`--locked`)
```powershell
Set-Location desktop-instance-manager

# Formatting & Clippy static analysis (0 warnings allowed)
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --locked -- -D warnings

# Unit test suites (--locked preserves Cargo.lock byte-for-byte)
cargo test --workspace --all-targets --locked
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --locked

# Release executables (dim-cli.exe, dim-test-fixture.exe, dim-desktop.exe + WebView2Loader.dll)
cargo build --workspace --release --target x86_64-pc-windows-msvc --locked
cargo build --manifest-path apps/desktop/src-tauri/Cargo.toml --release --target x86_64-pc-windows-msvc --locked

# Optional MSI & NSIS installer generation (requires cargo-tauri + WiX v3 + NSIS v3)
cargo tauri build --target x86_64-pc-windows-msvc --bundles msi,nsis -- --locked
```

### 3.3. Verified Local Windows Build & Test Baseline (`2026-11-17`, Commit `a5feb4a`)
Recorded from live execution on Windows 11 Pro (`10.0.26300`, `rustc 1.100.0`, target `x86_64-pc-windows-gnu` — full timings in [`windows-local-build-report.md`](../../plans/reports/build/windows-local-build-report.md)):
- **Workspace Format & Clippy:** `PASS` (`0.33s` fmt, `13.57s` clippy, `0` warnings).
- **Workspace Unit Tests (`--locked`):** **`29 / 29 PASS`** in `16.80s` (`dim-core`: 9, `dim-platform-windows`: 6, `dim-profile-store`: 4, `dim-platform-macos`: 3, `dim-platform-linux`: 3, `dim-process-manager`: 2, `dim-test-fixture`: 2), including live Win32 `WinVerifyTrust` Authenticode verification (`CN/O=VNG GROUP JSC` RDN pinning, tampered PE rejection, wrong-publisher rejection), `TokenElevation`, NTFS DACL safety, and NTFS junction (`mklink /J`) safe unlinking.
- **Tauri Desktop Crate Format, Clippy & Tests (`--locked`):** `PASS` (`0.19s` fmt, `93.12s` clippy with `0` warnings, `138.84s` test target compilation & run).
- **Dependency & License Audit ([`dependency-audit-report.json`](../../plans/reports/build/dependency-audit-report.json)):** **`PASS`** across **439** unique crates (`41` workspace + `434` Tauri; `431/431` `crates.io-index` SHA-256 checksums verified, `0` banned OpenSSL crates, `0` SPDX license violations, `0` VNG binaries).

---

## 4. Strict Classification: Compiled Executables vs MSI/NSIS Installers

Raw `.exe` files emitted by `cargo build --release` and packaged installer bundles emitted by `cargo tauri build` serve fundamentally different purposes and **must never be confused in documentation, manifests, or release notes**:

### 4.1. Artifact Taxonomy & Local Verified Digests

| Artifact Category | Path Pattern (under `desktop-instance-manager/`) | `artifact_kind` | `is_installer` | Local Verified Build (`x86_64-pc-windows-gnu`, [`SHA256SUMS.txt`](../../plans/reports/build/SHA256SUMS.txt)) |
| :--- | :--- | :--- | :---: | :--- |
| **Desktop Manager GUI Executable** *(NOT an installer)* | `apps/desktop/src-tauri/target/<triple>/release/dim-desktop.exe` | `compiled-executable` | **`false`** | `21,586,008` B — SHA-256 `eb1a024ef4a5b0ff6bb75596484658650ad348cd176bc3a611e8dc1b315fe4eb` (`UNSIGNED_DEVELOPMENT_ARTIFACT`) |
| **Native CLI Parity Runner** *(NOT an installer)* | `target/<triple>/release/dim-cli.exe` | `compiled-executable` | **`false`** | `2,209,966` B — SHA-256 `24132f61429179962f216e200cea04cc52a36a7f675f6408c460764c67c07c98` (`UNSIGNED_DEVELOPMENT_ARTIFACT`) |
| **Synthetic Isolation Test Fixture** *(NOT an installer)* | `target/<triple>/release/dim-test-fixture.exe` | `compiled-executable` | **`false`** | `1,616,671` B — SHA-256 `4ea0f248b17d087a6a630d9ac58c2f1e862ea9d86784a18164075289ff1bba7c` (`UNSIGNED_DEVELOPMENT_ARTIFACT`) |
| **WebView2 Loader Sidecar DLL** *(NOT an installer)* | `apps/desktop/src-tauri/target/<triple>/release/WebView2Loader.dll` | `shared-library-sidecar` | **`false`** | `160,880` B — SHA-256 `86545b66cdb0603bc26b626fb9ad610cb6e71f28d468f5ea66df23b03dda96d5` (Signed `CN=Microsoft Corporation`) |
| **NSIS Per-User Installer** *(Installer)* | `apps/desktop/src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/Multi-Zalo Desktop_0.1.0_x64-setup.exe` | `installer-nsis` | **`true`** | `SKIPPED_MISSING_TOOLING` locally (`makensis.exe` & `cargo-tauri` absent); configured in CI `windows-msvc-build` |
| **WiX Windows Installer** *(Installer)* | `apps/desktop/src-tauri/target/x86_64-pc-windows-msvc/release/bundle/msi/Multi-Zalo Desktop_0.1.0_x64_en-US.msi` | `installer-msi` | **`true`** | `SKIPPED_MISSING_TOOLING` locally (`candle.exe`/`light.exe` & `cargo-tauri` absent); configured in CI `windows-msvc-build` |

### 4.2. NSIS vs WiX MSI Installer Characteristics
- **NSIS (`*-setup.exe`):** Preferred consumer installer format for Multi-Zalo Desktop when configured with `"installMode": "currentUser"` (**IR-003**). Installs into `%LOCALAPPDATA%\Programs\Multi-Zalo Desktop\` under standard user permissions (`asInvoker`), matching official VNG Zalo's own per-user installation model (`%LOCALAPPDATA%\Programs\Zalo\`) without triggering UAC Administrator prompts.
- **WiX Toolset v3 (`*.msi`):** Standard Windows Installer database for enterprise software management (Intune / SCCM), generating Start Menu shortcuts, Add/Remove Programs (`ARP`) metadata, and deterministic upgrade GUIDs derived from `com.multizalodesktop.manager`.

---

## 5. PE32+ Header & Embedded XML Manifest Forensics

Every compiled Windows executable is verified by [`scripts/ci-cross-platform/inspect-artifact.mjs`](../../scripts/ci-cross-platform/inspect-artifact.mjs) (full JSON telemetry recorded in [`artifact-validation-manifest.json`](../../plans/reports/build/artifact-validation-manifest.json)):

1. **PE32+ Architecture & Subsystem:**
   - DOS `MZ` + `PE\0\0` signature, COFF Machine `0x8664` (`IMAGE_FILE_MACHINE_AMD64`), Optional Header Magic `0x020b` (`PE32+` 64-bit).
   - `dim-desktop.exe` compiles with `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]` (`WINDOWS_GUI` subsystem `2`, preventing an unwanted background console window on launch), whereas `dim-cli.exe` and `dim-test-fixture.exe` use `WINDOWS_CUI` (subsystem `3`).
2. **Least-Privilege UAC Execution Level (`asInvoker`):**
   - Embedded XML `RT_MANIFEST` explicitly declares:
     ```xml
     <requestedPrivileges>
       <requestedExecutionLevel level="asInvoker" uiAccess="false"/>
     </requestedPrivileges>
     ```
   - **Zero UAC Elevation Tricks:** `dim-desktop.exe` and `dim-cli.exe` never request `requireAdministrator` or `highestAvailable`, never set `uiAccess="true"`, never install kernel drivers or Windows Services, and never write to `HKLM`. At runtime, `win32_ffi::is_current_process_elevated()` queries `OpenProcessToken` + `TokenElevation` directly so diagnostics truthfully report privilege state.
3. **Win32 Long-Path Support (`longPathAware`):**
   - `<longPathAware>true</longPathAware>` is embedded in all three executables, preventing legacy `MAX_PATH` (`260`-character) path errors inside deeply nested Chromium/Electron `Partitions\zalo\IndexedDB` directories under `%LOCALAPPDATA%\MultiZaloDesktop\Profiles\<slug>_<8hex>\`.
4. **DPI Awareness & Windows 10/11 Compatibility GUIDs:**
   - On canonical `x86_64-pc-windows-msvc` builds (`rc.exe`/`mt.exe`), `tauri-build` (`tauri-winres v0.3.7`) embeds `PerMonitorV2` DPI awareness (`<dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2, PerMonitor</dpiAwareness>`) for crisp multi-monitor scaling and `<supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/>` so `RtlGetVersion` / Win32 APIs enable full Windows 10/11 behavior.
   - *(On the local `x86_64-pc-windows-gnu` fallback build, GNU `ld.exe` dropped Tauri's second manifest resource block due to `.rsrc` duplicate-manifest merging — documented in **IR-006**.)*

---

## 6. Security, Authenticode Signing & Windows Smart App Control (SAC) Policy

### 6.1. Windows Smart App Control (SAC) Governance
Windows 11 **Smart App Control** (`HKLM:\SYSTEM\CurrentControlSet\Control\CI\Policy\VerifiedAndReputablePolicyState = 1`) blocks execution of any unsigned PE binary (`.exe`, `.dll`, `.msi`, `.node`) that lacks either a trusted Authenticode signature with established reputation or an Organization/EV code-signing chain rooted in the Microsoft Trusted Root Program.

**Non-Negotiable SAC & Policy Rules:**
1. **Never Disable Smart App Control:** Never modify `HKLM:\SYSTEM\CurrentControlSet\Control\CI\Policy` and never instruct users to turn off Smart App Control, Windows Defender, or Kernel-mode Code Integrity. (Turning off SAC on Windows 11 is irreversible without resetting/reinstalling Windows.)
2. **Never Bypass Execution Policy for Unsigned Binaries:** Never use signed scripts, WMI, `rundll32`, `mshta`, or PowerShell reflection to load or execute unsigned native binaries around SAC/AppLocker/WDAC.
3. **Approved SAC-Enforcing Workflow Prior to Gate G3 Signing (ADR-002 Signed-Host Bridge):**
   - While native Rust/Tauri `.exe` builds remain `UNSIGNED_DEVELOPMENT_ARTIFACT` (prior to Gate G3 Authenticode signing), users on SAC-enforcing Windows 11 machines run the clean-room **Signed-Host Bridge** ([`ADR-002`](../architecture/adr-002-rust-tauri-and-sac-host-bridge.md) & [`windows-mvp-guide.md`](../user-guide/windows-mvp-guide.md)):
     ```powershell
     cd desktop-instance-manager
     node apps/desktop/ui-server.mjs
     ```
   - This serves the identical `apps/desktop/ui/` frontend on loopback `127.0.0.1` (`X-DIM-Session-Token` + strict CSP) backed by `scripts/dim-host.ps1` running inside Microsoft-signed `powershell.exe` and OpenJS-signed `node.exe`, verified for 100% contract parity against `dim-cli.exe` (`tests/run-contract-parity-tests.mjs`).

### 6.2. Production Authenticode Signing Requirements (Gate G3)
Before any native Windows executable or installer may exit `UNSIGNED_DEVELOPMENT_ARTIFACT` status and be published to end-users (see Gate G3 in [`signing-and-release-gates.md`](./signing-and-release-gates.md)), a human-gated release pipeline must complete:
1. **Trusted Code-Signing Identity:** Provision an **EV Code Signing Certificate** (hardware HSM / cloud KMS) or **Azure Artifact / Trusted Signing** identity (`digestAlgorithm: "sha256"`).
2. **Comprehensive Dual / Multi-Layer Signing (`signtool.exe`):**
   - Sign `dim-cli.exe` and `dim-desktop.exe` **before** installer packaging.
   - Configure Tauri's Windows `signCommand` so NSIS's generated `uninstall.exe` inside `*-setup.exe` is also Authenticode-signed (an unsigned uninstaller triggers SmartScreen/SAC blocks during uninstallation!).
   - Sign the outer `Multi-Zalo Desktop_0.1.0_x64-setup.exe` and `Multi-Zalo Desktop_0.1.0_x64_en-US.msi` containers with SHA-256 and an RFC 3161 trusted timestamp server (`/fd sha256 /tr http://timestamp.acs.microsoft.com /td sha256`).
3. **Post-Sign Cryptographic Verification:**
   ```powershell
   signtool.exe verify /pa /v "Multi-Zalo Desktop_0.1.0_x64-setup.exe"
   Get-AuthenticodeSignature "Multi-Zalo Desktop_0.1.0_x64-setup.exe" | Format-List Status, SignerCertificate, TimeStamperCertificate
   # Required: Status == Valid
   ```

### 6.3. Zero VNG Binary Policy & Runtime Signature Pinning
- Multi-Zalo Desktop distributes **zero** VNG code. `build-and-package.mjs`, `generate-artifact-manifest.mjs`, and CI's `artifact-verification` job scan all output directories and fail immediately if any `Zalo.exe` or `ZaloSetup*.exe` binary is present (`zero_vng_binary_policy_passed: true`).
- At runtime on the user's machine, [`crates/platform-windows/src/win32_ffi.rs`](../../crates/platform-windows/src/win32_ffi.rs) validates `%LOCALAPPDATA%\Programs\Zalo\Zalo-<ver>\Zalo.exe` via Win32 `WinVerifyTrust(WINTRUST_ACTION_GENERIC_VERIFY_V2)` and pins the X.500 certificate subject (`CN=VNG GROUP JSC` / `O=VNG GROUP JSC`), failing closed (`UNVERIFIED_BINARY`) on unsigned, tampered, or wrong-publisher binaries.

---

## 7. Cross-Reference: Out-of-Scope Integration Requests (`integration-requests.md`)

Because `Cargo.toml`, `Cargo.lock`, `tauri.conf.json`, `icons/*`, and `.github/workflows/ci.yml` were frozen during this packaging audit, all required follow-up configuration changes are tracked in [`plans/reports/build/integration-requests.md`](../../plans/reports/build/integration-requests.md):

| ID | Severity | Windows Packaging Relevance |
| :--- | :---: | :--- |
| **[IR-001](../../plans/reports/build/integration-requests.md#ir-001--rust-msrv-alignment-185-vs-190)** | Medium | Align workspace `Cargo.toml` `rust-version` (`1.85`) with `apps/desktop/src-tauri/Cargo.toml` (`1.90`). |
| **[IR-002](../../plans/reports/build/integration-requests.md#ir-002--missing-macos-icns--retina-icons--placeholder-icon-assets)** | High | Replace `137`-byte placeholder `icons/icon.ico` with a multi-resolution RGBA `.ico` (`16px`–`256px`) for crisp Windows taskbar, WiX ARP, and NSIS installer icons (using original artwork only, never VNG's logo). |
| **[IR-003](../../plans/reports/build/integration-requests.md#ir-003--explicit-os-bundle-configuration-blocks-in-tauriconfjson)** | High | Add explicit `bundle.windows` block in `tauri.conf.json`: `digestAlgorithm: "sha256"`, `webviewInstallMode: { type: "downloadBootstrapper", silent: true }`, and `nsis: { installMode: "currentUser" }`. |
| **[IR-004](../../plans/reports/build/integration-requests.md#ir-004--dual-workspace--split-cargolock--unpinned-tauri-cli-version)** | Medium | Resolve split `[workspace]` / dual `Cargo.lock` crate duplication (`windows-sys`, `sha2`, `thiserror`) and pin exact `tauri-cli` version in CI. |
| **[IR-005](../../plans/reports/build/integration-requests.md#ir-005--missing-workspace-lint-parity-in-appsdesktopsrc-tauricargotoml)** | Medium | Add `[lints.rust]` and `[lints.clippy]` parity (`unsafe_op_in_unsafe_fn = "deny"`, `unwrap_used = "warn"`, `expect_used = "warn"`) to `apps/desktop/src-tauri/Cargo.toml`. |
| **[IR-006](../../plans/reports/build/integration-requests.md#ir-006--explicit-profilerelease-optimization-msvc-guard_cf--gnu-rsrc-manifest-merge-fix)** | Low | Add `[profile.release]` (`lto = "thin"`, `codegen-units = 1`, `strip = "symbols"`), enable MSVC `GUARD_CF`, and document GNU `ld.exe` `.rsrc` duplicate-manifest merge limitation. |
| **[IR-007](../../plans/reports/build/integration-requests.md#ir-007--existing-githubworkflowsciyml-ubuntu-2404-package----locked-alignment)** | Medium | Add `--locked` and `apps/desktop/src-tauri/Cargo.lock` cache hashing to existing push/PR `.github/workflows/ci.yml`. |

---

## 8. Related Documentation
- [macOS Packaging Guide (`macos.md`)](./macos.md)
- [Ubuntu 24.04 LTS Packaging Guide (`ubuntu.md`)](./ubuntu.md)
- [Cross-Platform Build Matrix (`build-matrix.md`)](./build-matrix.md)
- [Code Signing & Release Gates (`signing-and-release-gates.md`)](./signing-and-release-gates.md)
- [Release Readiness Scorecard (`release-readiness.md`)](./release-readiness.md)
- [Windows Local Build Report (`windows-local-build-report.md`)](../../plans/reports/build/windows-local-build-report.md)
- [Artifact Validation Manifest (`artifact-validation-manifest.json`)](../../plans/reports/build/artifact-validation-manifest.json)
- [Out-of-Scope Integration Requests (`integration-requests.md`)](../../plans/reports/build/integration-requests.md)
- [Windows MVP User Guide (`windows-mvp-guide.md`)](../user-guide/windows-mvp-guide.md)
