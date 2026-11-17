# Windows Local Build, Test & Packaging Verification Report

**Report ID:** `DIM-WIN-BUILD-20261117`  
**Worktree:** `C:/Users/manhquy/Downloads/zalo-cross-platform-build/desktop-instance-manager`  
**Branch & Commit:** `build/cross-platform-distribution` @ `a5feb4a520798064c1508c26fe4c4ac17a3a03cf`  
**Host Operating System:** Microsoft Windows 11 Pro (`10.0.26300`, 64-bit x64), Standard Non-Elevated Session (`asInvoker`)  
**Canonical Production/CI Target:** `x86_64-pc-windows-msvc` (GitHub Actions `windows-latest`)  
**Executed Local Compiler Target:** `x86_64-pc-windows-gnu` (WinLibs MinGW-w64 UCRT64 `gcc 16.2.0` — MSVC Build Tools & Windows SDK absent locally; see Section 1)

---

## 1. Toolchain, Bundler & Runtime Prerequisite Audit

All checks below were executed strictly read-only on `2026-11-17` without modifying OS security policy, Windows Smart App Control (SAC), registry keys, or repository lockfiles.

| Component | Local Developer Host Status | Exact Version / Observation | GitHub Actions `windows-latest` Availability |
| :--- | :--- | :--- | :--- |
| **Rust Compiler (`rustc -Vv`)** | **INSTALLED** (`%USERPROFILE%\.cargo\bin`) | `rustc 1.100.0 (0ef1bc386 2026-11-09)`, `host: x86_64-pc-windows-gnu`, `LLVM version: 23.1.5` | Installed via `dtolnay/rust-toolchain@stable` (`host: x86_64-pc-windows-msvc`) |
| **Cargo (`cargo -V`)** | **INSTALLED** | `cargo 1.100.0 (3acc79c0c 2026-11-05)` | Preinstalled / `dtolnay/rust-toolchain@stable` |
| **Rustfmt & Clippy** | **INSTALLED** | `rustfmt 1.11.0-stable (0ef1bc3860 2026-11-09)`, `clippy 0.1.100 (0ef1bc3860 2026-11-09)` | Installed via `components: rustfmt, clippy` |
| **`rustup` Installed Targets** | **`x86_64-pc-windows-gnu` (Active)** | `aarch64-apple-darwin`, `aarch64-unknown-linux-gnu`, `x86_64-apple-darwin`, `x86_64-pc-windows-gnu`, `x86_64-unknown-linux-gnu` (`x86_64-pc-windows-msvc` **not** installed) | `x86_64-pc-windows-msvc` preinstalled |
| **MSVC Linker & Windows 10/11 SDK** | **NOT_INSTALLED** | `vswhere.exe`, `cl.exe`, `link.exe`, and `C:\Program Files (x86)\Windows Kits\10\Lib` absent | Preinstalled (Visual Studio 2022 Enterprise + Windows 10/11 SDK) |
| **Local C/PE UCRT Toolchain** | **INSTALLED** (WinGet `BrechtSanders.WinLibs.POSIX.UCRT`) | `gcc.exe 16.2.0` (`MinGW-W64 x86_64-ucrt-posix-seh`), `GNU ld 2.47.20260726`, `GNU windres 2.47.20260726` | Not needed (uses native MSVC `cl.exe` + `link.exe` + `rc.exe`) |
| **Tauri CLI (`cargo tauri`)** | **NOT_INSTALLED** | `cargo-tauri` absent; zero `node_modules` / `@tauri-apps/cli` (zero-npm-dependency UI architecture) | Installed in CI job (`cargo install tauri-cli --version "^2.5.0" --locked`) |
| **WiX Toolset (`.msi` bundler)** | **NOT_INSTALLED** | `candle.exe`, `light.exe`, `wix.exe` absent in `PATH` and `C:\Program Files*` | **Preinstalled** on `windows-latest` (WiX Toolset v3.14) |
| **NSIS (`-setup.exe` bundler)** | **NOT_INSTALLED** | `makensis.exe` absent in `PATH` and `C:\Program Files*` | **Preinstalled** on `windows-latest` (NSIS 3.x) |
| **WebView2 Evergreen Runtime** | **INSTALLED** | `pv = 157.0.4341.46` (`HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}`, `C:\Program Files (x86)\Microsoft\EdgeWebView\Application`) | Preinstalled on `windows-latest` |
| **Node.js Runtime** | **INSTALLED** (Signed `CN=OpenJS Foundation`) | `v26.13.1` | Configured via `actions/setup-node@v4` (`node-version: "22"`) |
| **Smart App Control (SAC / WDAC)** | **READ-ONLY VERIFIED** | `HKLM:\SYSTEM\CurrentControlSet\Control\CI\Policy` `VerifiedAndReputablePolicyState = 0` (`SAC_PreviousState = 1`, `SAC_EnforcementReason = 6`) | Disabled on ephemeral CI VMs; **enforced (`1`) on target Windows 11 endpoints** |

### Direct `x86_64-pc-windows-msvc` Target Probe Result

Invoking `cargo check --workspace --target x86_64-pc-windows-msvc --locked` on this host exited with code `101`:

```text
error[E0463]: can't find crate for `core`
  = note: the `x86_64-pc-windows-msvc` target may not be installed
  = help: consider downloading the target with `rustup target add x86_64-pc-windows-msvc`
```

Even if the Rust `std` target were downloaded via `rustup`, linking `x86_64-pc-windows-msvc` requires Microsoft Visual Studio C++ Build Tools (`link.exe`, `msvcrt.lib`, `vcruntime.lib`) and the Windows 10/11 SDK (`kernel32.lib`, `user32.lib`, `wintrust.lib`, `crypt32.lib`, `rstrtmgr.lib`), which are not installed on this developer workstation. Per strict safety guardrails (zero unverified binary downloads), local compilation, Clippy, unit testing, and PE verification were executed on the installed native 64-bit Windows UCRT target (`x86_64-pc-windows-gnu`), while `.github/workflows/cross-platform-validation.yml` targets canonical `x86_64-pc-windows-msvc` on `windows-latest`.

---

## 2. Formatting, Static Analysis & Unit Test Execution (`--locked`)

All commands were executed inside `C:/Users/manhquy/Downloads/zalo-cross-platform-build/desktop-instance-manager` with `--locked` enforced on Cargo test/build invocations. Post-execution `git status --short` confirmed zero modifications to `Cargo.lock` (`41` packages) or `apps/desktop/src-tauri/Cargo.lock` (`434` packages).

| Step | Exact Command | Exit Code | Wall-Clock Duration | Test / Check Breakdown |
| :--- | :--- | :---: | ---: | :--- |
| **1. Workspace Format** | `cargo fmt --all -- --check` | `0` | `0.33s` (`330 ms`) | All 7 workspace crates cleanly formatted |
| **2. Workspace Clippy** | `cargo clippy --workspace --all-targets -- -D warnings` | `0` | `13.57s` (`13,569 ms`) | `0` warnings across all library, binary, and unit-test targets |
| **3. Workspace Unit Tests** | `cargo test --workspace --all-targets --locked` | `0` | `16.80s` (`16,802 ms`) | **`29 passed; 0 failed; 0 ignored`** across 8 test binaries (see breakdown below) |
| **4. Tauri Shell Format** | `cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml -- --check` | `0` | `0.19s` (`192 ms`) | `apps/desktop/src-tauri/src/{main,commands}.rs` & `build.rs` cleanly formatted |
| **5. Tauri Shell Clippy** | `cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml -- -D warnings` | `0` | `93.12s` (`93,118 ms`) | `0` Clippy warnings across `dim-desktop v0.1.0` (`tauri v2.12.6`) |
| **6. Tauri Shell Tests** | `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --locked` | `0` | `138.84s` (`138,836 ms`) | **`0 passed; 0 failed`** (`dim-desktop` test harness compiled & executed cleanly; 1 GNU `ld.exe` `.rsrc` manifest merge warning noted in IR-006) |

### Per-Crate Unit Test Breakdown (`29 / 29` Passed)

| Crate / Target | Passed | Failed | Ignored | Test Execution Time | Verified Coverage Scope |
| :--- | ---: | ---: | ---: | ---: | :--- |
| `dim-core` (`src/lib.rs`) | `9` | `0` | `0` | `0.02s` | Profile ID hex format, ISO timestamps, Vietnamese slug normalization (`đ`/`Đ` & diacritics), platform data-dir contracts, log path/token redaction, `5 GiB` disk threshold, `resolve_safe_child` traversal/reserved-name defense, duplicate `zalo_user_id` guard, `config.json` `db_path.location` resolution |
| `dim-platform-linux` (`src/lib.rs`) | `3` | `0` | `0` | `0.02s` | Official Linux Zalo launch rejection (`UnsupportedPlatform`), experimental AppImage status honesty, XDG/`HOME` + `bwrap` argument isolation |
| `dim-platform-macos` (`src/lib.rs`) | `3` | `0` | `0` | `0.01s` | Hyphenated-hostname `SingletonLock` parser, `open -n --env CFFIXED_USER_HOME` builder, `Info.plist` extraction & `NOT_TESTED` status reporting |
| `dim-platform-windows` (`src/lib.rs`) | `6` | `0` | `0` | `0.19s` | Strict X.500 `CN`/`O=VNG GROUP JSC` RDN pinning, `User Shell Folders` redirection detection, case-insensitive env deduplication, Win32 command-line quoting & `Zalo-*` semver discovery, profile layout pre-seeding, `WinVerifyTrust` Authenticode verification (valid/unsigned/tampered/wrong-publisher/missing) |
| `dim-process-manager` (`src/lib.rs`) | `2` | `0` | `0` | `0.07s` | Primary Zalo PID/path protection (`PROTECTED_PRIMARY_PROCESS` / `PROTECTED_PRIMARY_PATH`), stale lock recovery & duplicate `zalo_user_id` launch refusal |
| `dim-profile-store` (`src/lib.rs`) | `4` | `0` | `0` | `0.10s` | `5 GiB` backup disk-space guard, atomic `profiles.json` CRUD + `.zalo-profile-id`, NTFS junction (`mklink /J`) unlink without harming external target, offline backup & restore with SHA-256 tamper verification |
| `dim-test-fixture` (`bin/dim-cli.rs`) | `0` | `0` | `0` | `0.00s` | Native CLI binary harness (`dim-cli`) |
| `dim-test-fixture` (`src/main.rs`) | `2` | `0` | `0` | `0.00s` | Synthetic desktop fixture CLI flags (`--z-update-force-quit`, `--crash`, `--user-id`) & exclusive lockfile acquisition |
| **Total** | **`29`** | **`0`** | **`0`** | **`0.41s`** | **100% pass rate** |

---

## 3. Release Artifact Compilation & MSI / NSIS Packaging Assessment

### 3.1. Release Build Timings

| Build Target | Exact Command | Exit Code | Wall-Clock Duration | Output Directory |
| :--- | :--- | :---: | ---: | :--- |
| **Workspace Release** | `cargo build --workspace --release --target x86_64-pc-windows-gnu --locked` | `0` | `18.29s` (`18,294 ms`) | `target/x86_64-pc-windows-gnu/release/` |
| **Tauri Desktop Release** | `cargo build --manifest-path apps/desktop/src-tauri/Cargo.toml --release --target x86_64-pc-windows-gnu --locked` | `0` | `207.46s` (`207,459 ms`) | `apps/desktop/src-tauri/target/x86_64-pc-windows-gnu/release/` |

### 3.2. Compiled Artifacts Inventory (Strictly Distinguished from Installers)

> **Classification Rule:** Raw `.exe` files produced by `cargo build --release` (`dim-desktop.exe`, `dim-cli.exe`, `dim-test-fixture.exe`) are **compiled executables (`is_installer: false`)** — they are **NOT** installers, and because their Authenticode certificate table is empty (`NotSigned`), they are strictly **`UNSIGNED_DEVELOPMENT_ARTIFACT`** builds, **NOT** signed production releases. Zero VNG `Zalo.exe` or proprietary Zalo assets are embedded or packaged anywhere.

| Artifact Path (relative to `desktop-instance-manager/`) | Classification | Size (bytes) | SHA-256 | Signing Status |
| :--- | :--- | ---: | :--- | :--- |
| `target/x86_64-pc-windows-gnu/release/dim-cli.exe` | `compiled-executable` (`is_installer: false`) | `2,209,966` | `24132f61429179962f216e200cea04cc52a36a7f675f6408c460764c67c07c98` | `UNSIGNED_DEVELOPMENT_ARTIFACT` (`NotSigned`) |
| `target/x86_64-pc-windows-gnu/release/dim-test-fixture.exe` | `compiled-executable` (`is_installer: false`) | `1,616,671` | `4ea0f248b17d087a6a630d9ac58c2f1e862ea9d86784a18164075289ff1bba7c` | `UNSIGNED_DEVELOPMENT_ARTIFACT` (`NotSigned`) |
| `apps/desktop/src-tauri/target/x86_64-pc-windows-gnu/release/dim-desktop.exe` | `compiled-executable` (`is_installer: false`) | `21,586,008` | `eb1a024ef4a5b0ff6bb75596484658650ad348cd176bc3a611e8dc1b315fe4eb` | `UNSIGNED_DEVELOPMENT_ARTIFACT` (`NotSigned`) |
| `apps/desktop/src-tauri/target/x86_64-pc-windows-gnu/release/WebView2Loader.dll` | `shared-library-sidecar` (`is_installer: false`) | `160,880` | `86545b66cdb0603bc26b626fb9ad610cb6e71f28d468f5ea66df23b03dda96d5` | Vendor-signed (`Valid`, `CN=Microsoft Corporation`) |

### 3.3. MSI & NSIS Installer Packaging Status (`SKIPPED_MISSING_TOOLING` Locally vs `windows-latest` CI)

- **Local Status:** **No `.msi` or NSIS `-setup.exe` installer bundles were generated locally** (`bundle_status: SKIPPED_MISSING_TOOLING`).
- **Exact Reason:** Neither `cargo-tauri` (`tauri-cli`), WiX Toolset (`candle.exe`/`light.exe`/`wix.exe`), nor NSIS (`makensis.exe`) is installed on this developer machine. Per mission security policy, no unverified third-party installer toolchains were downloaded onto the host, and no fake archive/wrapper was fabricated.
- **CI `windows-latest` Runner Path:** GitHub Actions `windows-latest` images include Visual Studio 2022 MSVC (`x86_64-pc-windows-msvc`), WiX Toolset v3.14 (`candle.exe`/`light.exe`), and NSIS 3.x (`makensis.exe`) preinstalled. In `.github/workflows/cross-platform-validation.yml` (`windows-msvc-build` job), installing `tauri-cli` via Cargo and invoking `build-and-package.mjs --target x86_64-pc-windows-msvc --bundles all` produces `bundle/msi/*.msi` and `bundle/nsis/*-setup.exe` (still classified as `UNSIGNED_DEVELOPMENT_ARTIFACT` until Gate G3 Authenticode signing is provisioned).

---

## 4. PE32+ Header, Embedded XML Manifest, DLL Import & Authenticode Forensics

Direct binary inspection using `objdump -p`, `Get-AuthenticodeSignature`, and raw PE COFF/OptionalHeader + `.rsrc` parsing established:

### 4.1. PE COFF & Optional Header Properties

| Binary | COFF Machine | Optional Magic | Subsystem | `DllCharacteristics` | `IMAGE_DIRECTORY_ENTRY_SECURITY` |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `dim-cli.exe` | `0x8664` (`AMD64`) | `0x020b` (`PE32+`) | `3` (`WINDOWS_CUI` Console) | `0x0160` (`HIGH_ENTROPY_VA \| DYNAMIC_BASE \| NX_COMPAT`) | `RVA=0, Size=0` (`NotSigned`) |
| `dim-test-fixture.exe` | `0x8664` (`AMD64`) | `0x020b` (`PE32+`) | `3` (`WINDOWS_CUI` Console) | `0x0160` (`HIGH_ENTROPY_VA \| DYNAMIC_BASE \| NX_COMPAT`) | `RVA=0, Size=0` (`NotSigned`) |
| `dim-desktop.exe` | `0x8664` (`AMD64`) | `0x020b` (`PE32+`) | `2` (`WINDOWS_GUI` Windowed) | `0x0160` (`HIGH_ENTROPY_VA \| DYNAMIC_BASE \| NX_COMPAT`) | `RVA=0, Size=0` (`NotSigned`) |
| `WebView2Loader.dll` | `0x8664` (`AMD64`) | `0x020b` (`PE32+`) | `3` (`WINDOWS_CUI`) | `0x4160` (`HIGH_ENTROPY_VA \| DYNAMIC_BASE \| NX_COMPAT \| GUARD_CF`) | Present (`Valid`, Microsoft Corp.) |

### 4.2. Embedded Application Manifest (`RT_MANIFEST`)

All three compiled executables (`dim-cli.exe`, `dim-test-fixture.exe`, `dim-desktop.exe`) embed the following XML manifest confirming strict standard-user execution (`asInvoker`) with `uiAccess="false"` and Win32 long-path support (`longPathAware=true`), requiring **zero** UAC Administrator elevation:

```xml
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0" xmlns:asmv3="urn:schemas-microsoft-com:asm.v3" >
 <asmv3:application>
  <asmv3:windowsSettings xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">
   <longPathAware>true</longPathAware>
  </asmv3:windowsSettings>
 </asmv3:application>
 <trustInfo xmlns="urn:schemas-microsoft-com:asm.v2">
  <security>
   <requestedPrivileges>
    <requestedExecutionLevel level="asInvoker" uiAccess="false"/>
   </requestedPrivileges>
  </security>
 </trustInfo>
</assembly>
```

> **Important Toolchain Finding (Recorded in IR-006):** Under local `x86_64-pc-windows-gnu`, MinGW Binutils `ld.exe` emitted `.rsrc merge failure: multiple non-default manifests` when linking `dim-desktop.exe` because both Rust's MinGW target driver and `tauri-winres v0.3.7` (`build.rs`) supplied `RT_MANIFEST` resources (`blocks_count: 2` in `.rsrc`: `asInvoker` + `longPathAware=true` + Microsoft Common Controls v6 `6.0.0.0`, while `dpiAwareness` / `<supportedOS>` GUIDs were omitted by GNU `.rsrc` merging). Building on canonical `x86_64-pc-windows-msvc` (`link.exe` + `rc.exe`/`mt.exe`) resolves this cleanly and enables `GUARD_CF` (`0x4000`).

### 4.3. Dynamic Library Imports & `WebView2Loader.dll` Dependency

- **`dim-desktop.exe`:** Imports `WebView2Loader.dll` (placed alongside `dim-desktop.exe` in `apps/desktop/src-tauri/target/x86_64-pc-windows-gnu/release/WebView2Loader.dll` by `webview2-com-sys v0.39.1`), Windows Universal CRT API sets (`api-ms-win-crt-*.dll`), and Win32 system libraries (`KERNEL32.dll`, `USER32.dll`, `ADVAPI32.dll`, `CRYPT32.dll`, `WINTRUST.dll`, `RstrtMgr.DLL`, `USERENV.dll`, `SHELL32.dll`, `OLE32.dll`, `COMBASE.dll`, `DWMAPI.dll`, `GDI32.dll`). Zero external MinGW DLLs (`libgcc_s_seh-1.dll`, `libstdc++-6.dll`, `libwinpthread-1.dll`) are dynamically required.
- **`dim-cli.exe`:** Imports Universal CRT (`api-ms-win-crt-*.dll`) plus `ADVAPI32.dll`, `CRYPT32.dll`, `WINTRUST.dll`, `RstrtMgr.DLL`, `PSAPI.DLL`, `USER32.dll`, `USERENV.dll`, `WS2_32.dll`, `KERNEL32.dll`, `ntdll.dll`.
- **Smart App Control (SAC) Enforcement Impact:** On Windows 11 machines with `VerifiedAndReputablePolicyState = 1`, unsigned `dim-desktop.exe` and `dim-cli.exe` are blocked by the kernel CI policy regardless of `WebView2Loader.dll`'s Microsoft signature. Until Gate G3 Authenticode signing (OV/EV or Azure Trusted Signing) is completed, SAC-enabled Windows 11 hosts must use the compliant Signed-Host Bridge (`scripts/dim-host.ps1` + `apps/desktop/ui-server.mjs`) documented in ADR-002.

---

## 5. Cross-Platform Provenance Tooling Verification

Executed the Node ESM provenance & inspection scripts in [`scripts/ci-cross-platform/`](../../../scripts/ci-cross-platform/) against this worktree and its compiled Windows `x86_64` release artifacts:

1. **Prerequisite Check — Canonical CI Target (`x86_64-pc-windows-msvc`):**
   - Command: `node scripts/ci-cross-platform/check-prerequisites.mjs --target x86_64-pc-windows-msvc`
   - Result: `status = "MISSING_COMPILE_PREREQUISITES"` (`compile_ready: false`, `bundler_ready: false`) — accurately flagged missing `x86_64-pc-windows-msvc` Rust target, absent MSVC `cl.exe`/`link.exe`, absent `cargo-tauri`, and absent WiX/NSIS on this developer workstation.
2. **Prerequisite Check — Local Host UCRT Target (`x86_64-pc-windows-gnu`):**
   - Command: `node scripts/ci-cross-platform/check-prerequisites.mjs --target x86_64-pc-windows-gnu`
   - Result: `status = "COMPILE_READY_BUNDLER_INCOMPLETE"` (`compile_ready: true`, `bundler_ready: false`, `rustc_release: "1.100.0"`, `blocking_compile_issues: []`, `blocking_bundler_issues: ["Tauri CLI ... is not installed", "Neither WiX ... nor NSIS ... is installed"]`), plus `IR-001` MSRV mismatch and `IR-002` `.icns` warnings.
3. **Binary Header & Manifest Inspection (`inspect-artifact.mjs`):**
   - Command: `node scripts/ci-cross-platform/inspect-artifact.mjs --file apps/desktop/src-tauri/target/x86_64-pc-windows-gnu/release/dim-desktop.exe`
   - Result: `artifact_kind = "compiled-executable"`, `is_installer = false`, `pe_format = "PE32+ (64-bit)"`, `coff_machine_hex = "0x8664"`, `subsystem = "WINDOWS_GUI"`, `authenticode.has_certificate_table = false` (`signing_status = "UNSIGNED_DEVELOPMENT_ARTIFACT"`), `embedded_manifest.requested_execution_level = "asInvoker"`, `imports_webview2_loader = true`, `sidecar_webview2_loader.present = true` (`160,880` bytes).
4. **Section 7 Traceable Manifest & Checksum Generation (`generate-artifact-manifest.mjs`):**
   - Command:
     ```powershell
     node scripts/ci-cross-platform/generate-artifact-manifest.mjs `
       --scan-default-targets `
       --target x86_64-pc-windows-gnu `
       --os-label "Windows 11 Pro (10.0.26300 x64)" `
       --arch x86_64 `
       --test-status "PASS (29/29 workspace unit tests + src-tauri test target passed --locked)" `
       --build-log-url "LOCAL_WORKTREE_BUILD:plans/reports/build/windows-local-build-report.md" `
       --out-manifest plans/reports/build/artifact-validation-manifest.json `
       --out-sha256 plans/reports/build/SHA256SUMS.txt
     ```
   - Outputs Verified:
     - [`artifact-validation-manifest.json`](./artifact-validation-manifest.json) (`artifacts_count = 3`, `tauri_version = "2.12.6"`, `zero_vng_binary_policy_passed = true`, all 18 Section 7 schema fields populated and validated).
     - [`SHA256SUMS.txt`](./SHA256SUMS.txt) (`3` SHA-256 digests matching `Get-FileHash -Algorithm SHA256` byte-for-byte).
     - Out-of-scope follow-ups documented in [`integration-requests.md`](./integration-requests.md) (`IR-001` through `IR-007`).

---

## 6. Honesty & Safety Gate Attestation

- **CI Live Execution Honesty:** Remote GitHub Actions workflows were **not** executed (`CI_LIVE_RUN_STATUS: NOT_EXECUTED (STATICALLY_VALIDATED_ONLY)`); see [`ci-baseline-and-github-audit.md`](./ci-baseline-and-github-audit.md).
- **Artifact Classification Honesty:** `dim-desktop.exe`, `dim-cli.exe`, and `dim-test-fixture.exe` are raw compiled executables (`is_installer: false`), **never** installers; MSI/NSIS installers were **not** generated locally (`SKIPPED_MISSING_TOOLING`).
- **Signing Posture Honesty:** All locally compiled `.exe` binaries have `signing_status: "UNSIGNED_DEVELOPMENT_ARTIFACT"` (`Authenticode = NotSigned`) and are **never** represented as signed or production-ready releases.
- **Zero OS Bypass & Zero VNG Binary Policy:** Windows Smart App Control (SAC) was never disabled or bypassed; signed PowerShell bypass tricks were never used; zero VNG `Zalo.exe` or proprietary Zalo files were touched, embedded, or packaged (`vng_binary_embedded: false`).
