# Windows Local Build, Native Runtime Fixture & Packaging Verification Report

**Report ID:** `DIM-WIN-BUILD-20261118-E2`  
**Worktree:** `C:/Users/manhquy/Downloads/zalo-cross-platform-build/desktop-instance-manager`  
**Branch & Commit:** `build/cross-platform-distribution` @ `ac0df10c795208d983a2fb8d03f95ff627adf59c` (Base `main` @ `a5feb4a520798064c1508c26fe4c4ac17a3a03cf`)  
**Host Operating System:** Microsoft Windows 11 Pro (`10.0.26300`, 64-bit `AMD64`/`x86_64`), Standard Non-Elevated Session (`asInvoker`)  
**Canonical Production/CI Target:** `x86_64-pc-windows-msvc` (GitHub Actions `windows-2022`)  
**Executed Local Compiler Target:** `x86_64-pc-windows-gnu` (WinLibs MinGW-w64 UCRT64 `gcc 16.2.0` — native `x86_64` Windows host execution; MSVC Build Tools & Windows SDK absent locally)

---

## 1. Toolchain, Bundler & Runtime Prerequisite Audit

All checks below were executed strictly read-only without modifying OS security policy, Windows Smart App Control (SAC), registry keys, or repository lockfiles.

| Component | Local Developer Host Status | Exact Version / Observation | GitHub Actions `windows-2022` Availability |
| :--- | :--- | :--- | :--- |
| **Rust Compiler (`rustc -Vv`)** | **INSTALLED** (`%USERPROFILE%\.cargo\bin`) | `rustc 1.100.0 (0ef1bc386 2026-11-09)`, `host: x86_64-pc-windows-gnu`, `LLVM version: 23.1.5` | Installed via `dtolnay/rust-toolchain@stable` (`host: x86_64-pc-windows-msvc`) |
| **Cargo (`cargo -V`)** | **INSTALLED** | `cargo 1.100.0 (3acc79c0c 2026-11-05)` | Preinstalled / `dtolnay/rust-toolchain@stable` |
| **Rustfmt & Clippy** | **INSTALLED** | `rustfmt 1.11.0-stable (0ef1bc3860 2026-11-09)`, `clippy 0.1.100 (0ef1bc3860 2026-11-09)` | Installed via `components: rustfmt, clippy` |
| **`cargo-deny` Advisory Scanner** | **INSTALLED** (`%USERPROFILE%\.cargo\bin`) | `cargo-deny 0.21.0` (`rustsec/advisory-db` @ `233155b4025b854c68a4b199c0e32d7fd53291a2`, `0` vulnerabilities across `439` unique crates) | Executed in `dependency-audit` job via `EmbarkStudios/cargo-deny-action@v2` |
| **`rustup` Installed Targets** | **`x86_64-pc-windows-gnu` (Active)** | `aarch64-apple-darwin`, `aarch64-unknown-linux-gnu`, `x86_64-apple-darwin`, `x86_64-pc-windows-gnu`, `x86_64-unknown-linux-gnu` (`x86_64-pc-windows-msvc` **not** installed) | `x86_64-pc-windows-msvc` preinstalled |
| **MSVC Linker & Windows 10/11 SDK** | **NOT_INSTALLED** | `vswhere.exe`, `cl.exe`, `link.exe`, and `C:\Program Files (x86)\Windows Kits\10\Lib` absent | Preinstalled (Visual Studio 2022 Enterprise + Windows 10/11 SDK) |
| **Local C/PE UCRT Toolchain** | **INSTALLED** (WinGet `BrechtSanders.WinLibs.POSIX.UCRT`) | `gcc.exe 16.2.0` (`MinGW-W64 x86_64-ucrt-posix-seh`), `GNU ld 2.47.20260726`, `GNU windres 2.47.20260726` | Not needed (uses native MSVC `cl.exe` + `link.exe` + `rc.exe`) |
| **Tauri CLI (`@tauri-apps/cli@2.12.5`)** | **NOT_INSTALLED LOCALLY** | `cargo-tauri` absent; zero `node_modules` (zero-npm-dependency UI architecture). Latest npm 2.x verified read-only via `npm view @tauri-apps/cli version` -> `2.12.5` (pinned in CI & `IR-004`) | Installed in CI via pinned `@tauri-apps/cli@2.12.5` |
| **WiX Toolset (`.msi` bundler)** | **NOT_INSTALLED** | `candle.exe`, `light.exe`, `wix.exe` absent in `PATH` and `C:\Program Files*` | **Preinstalled** on `windows-2022` (WiX Toolset v3.14) |
| **NSIS (`-setup.exe` bundler)** | **NOT_INSTALLED** | `makensis.exe` absent in `PATH` and `C:\Program Files*` | **Preinstalled** on `windows-2022` (NSIS 3.x) |
| **WebView2 Evergreen Runtime** | **INSTALLED** | `pv = 157.0.4341.46` (`HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}`, `C:\Program Files (x86)\Microsoft\EdgeWebView\Application`) | Preinstalled on `windows-2022` |
| **Node.js Runtime** | **INSTALLED** (Signed `CN=OpenJS Foundation`) | `v26.13.1` | Configured via `actions/setup-node@v4` (`node-version: "22"`) |
| **Smart App Control (SAC / WDAC)** | **READ-ONLY VERIFIED** | `HKLM:\SYSTEM\CurrentControlSet\Control\CI\Policy` `VerifiedAndReputablePolicyState = 0` (`SAC_PreviousState = 1`, `SAC_EnforcementReason = 6`) | Disabled on ephemeral CI VMs; **enforced (`1`) on target Windows 11 endpoints** |

### Direct `x86_64-pc-windows-msvc` Target Probe Result

Invoking `cargo check --workspace --target x86_64-pc-windows-msvc --locked` on this host exited with code `101`:

```text
error[E0463]: can't find crate for `core`
  = note: the `x86_64-pc-windows-msvc` target may not be installed
  = help: consider downloading the target with `rustup target add x86_64-pc-windows-msvc`
```

Even with the Rust `std` target installed, linking `x86_64-pc-windows-msvc` requires Microsoft Visual Studio C++ Build Tools (`link.exe`, `msvcrt.lib`, `vcruntime.lib`) and the Windows 10/11 SDK (`kernel32.lib`, `user32.lib`, `wintrust.lib`, `crypt32.lib`, `rstrtmgr.lib`). Local compilation, `--locked` Clippy, `--locked` unit testing, native runtime fixture execution, and PE verification were therefore executed natively on the host's 64-bit Windows UCRT target (`x86_64-pc-windows-gnu`), while `.github/workflows/cross-platform-validation.yml` (`windows-msvc-build` on `windows-2022`) enforces native `x86_64-pc-windows-msvc`.

---

## 2. Formatting, `--locked` Static Analysis, Unit Tests & Native Runtime Fixture Execution

All commands executed inside `C:/Users/manhquy/Downloads/zalo-cross-platform-build/desktop-instance-manager` with `--locked` enforced on every Cargo check/clippy/test/build invocation (`build-and-package.mjs --target x86_64-pc-windows-gnu --bundles all`). Zero modifications occurred to `Cargo.lock` (`41` packages) or `apps/desktop/src-tauri/Cargo.lock` (`434` packages).

| Stage | Exact Command | Exit Code | Duration | Verified Outcome |
| :--- | :--- | :---: | ---: | :--- |
| **1. Workspace Format** | `cargo fmt --all -- --check` | `0` | `0.33s` | All 7 workspace crates cleanly formatted |
| **2. Workspace Clippy (`--locked`)** | `cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu --locked -- -D warnings` | `0` | `14.26s` | `0` warnings across all library, binary, and unit-test targets |
| **3. Workspace Unit Tests (`--locked`)** | `cargo test --workspace --all-targets --target x86_64-pc-windows-gnu --locked` | `0` | `16.80s` | **`29 passed; 0 failed; 0 ignored`** across 8 test binaries (`PASSED_NATIVE`) |
| **4. Tauri Shell Format** | `cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml -- --check` | `0` | `0.19s` | `apps/desktop/src-tauri/src/{main,commands}.rs` & `build.rs` cleanly formatted |
| **5. Tauri Shell Clippy (`--locked`)** | `cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --target x86_64-pc-windows-gnu --locked -- -D warnings` | `0` | `93.12s` | `0` Clippy warnings across `dim-desktop v0.1.0` (`tauri v2.12.6`) |
| **6. Tauri Shell Tests (`--locked`)** | `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --target x86_64-pc-windows-gnu --locked` | `0` | `138.84s` | **`0 passed; 0 failed`** (`dim-desktop` test harness compiled & ran natively on `x86_64`) |
| **7. Native Fixture Smoke (`fixture-app.mjs`)** | `node fixtures/test-desktop-app/fixture-app.mjs --stay-ms=100` (isolated `%TEMP%` `APPDATA`) | `0` | `0.20s` | Emitted `{"event":"ready","userDataDir":"...\\ZaloData","launch_count":1,"zalo_user_id":"100001"}` |
| **8. Native `dim-cli.exe` Runtime Smoke** | `target/x86_64-pc-windows-gnu/release/dim-cli.exe --cli --json --data-dir <temp>/dim-data status` | `0` | `0.07s` | Returned `{"ok":true,"platform":"windows","primary_zalo":{"zalo_user_id":"100001",...}}` (`PASSED_NATIVE_FIXTURE_AND_DIM_CLI`) |

### Per-Crate Unit Test Breakdown (`29 / 29` Passed)

| Crate / Target | Passed | Failed | Ignored | Test Execution Time | Verified Coverage Scope |
| :--- | ---: | ---: | ---: | ---: | :--- |
| `dim-core` (`src/lib.rs`) | `9` | `0` | `0` | `0.02s` | Profile ID hex format, ISO timestamps, Vietnamese slug normalization (`đ`/`Đ` & diacritics), platform data-dir contracts, log path/token redaction, `5 GiB` disk threshold, `resolve_safe_child` traversal/reserved-name defense, duplicate `zalo_user_id` guard, `config.json` `db_path.location` resolution |
| `dim-platform-linux` (`src/lib.rs`) | `3` | `0` | `0` | `0.02s` | Official Linux Zalo launch rejection (`UnsupportedPlatform`), experimental AppImage status honesty, XDG/`HOME` + `bwrap` argument isolation |
| `dim-platform-macos` (`src/lib.rs`) | `3` | `0` | `0` | `0.01s` | Hyphenated-hostname `SingletonLock` parser, `open -n --env CFFIXED_USER_HOME` builder, `Info.plist` extraction & `NOT_TESTED` status reporting |
| `dim-platform-windows` (`src/lib.rs`) | `6` | `0` | `0` | `0.19s` | Strict X.500 `CN`/`O=VNG GROUP JSC` RDN pinning, `User Shell Folders` redirection detection, case-insensitive env deduplication, Win32 command-line quoting & `Zalo-*` semver discovery, profile layout pre-seeding, `WinVerifyTrust` Authenticode verification |
| `dim-process-manager` (`src/lib.rs`) | `2` | `0` | `0` | `0.07s` | Primary Zalo PID/path protection (`PROTECTED_PRIMARY_PROCESS` / `PROTECTED_PRIMARY_PATH`), stale lock recovery & duplicate `zalo_user_id` launch refusal |
| `dim-profile-store` (`src/lib.rs`) | `4` | `0` | `0` | `0.10s` | `5 GiB` backup disk-space guard, atomic `profiles.json` CRUD + `.zalo-profile-id`, NTFS junction (`mklink /J`) unlink without harming external target, offline backup & restore with SHA-256 tamper verification |
| `dim-test-fixture` (`bin/dim-cli.rs` + `src/main.rs`) | `2` | `0` | `0` | `0.00s` | Synthetic desktop fixture CLI flags (`--z-update-force-quit`, `--crash`, `--user-id`) & exclusive lockfile acquisition |
| **Total** | **`29`** | **`0`** | **`0`** | **`0.41s`** | **100% pass rate** |

---

## 3. Release Artifact Compilation & MSI / NSIS Packaging Assessment

### 3.1. Compiled Artifacts Inventory (Strictly Distinguished from Installers)

> **Classification & Supply-Chain Honesty Rule:**
> - Raw `.exe` files produced by `cargo build --release` (`dim-desktop.exe`, `dim-cli.exe`, `dim-test-fixture.exe`) are **compiled executables (`is_installer: false`)** — they are **NOT** installers.
> - All three executables have empty PE Authenticode tables (`NotSigned`) and are strictly **`UNSIGNED_DEVELOPMENT_ARTIFACT`** builds, **NOT** signed production releases.
> - Local manifest provenance is strictly **`SELF_REPORTED_UNATTESTED`** (`oidc_slsa_attestation_present: false`); an SBOM is a declared dependency inventory, **not** independent build provenance.
> - Zero VNG `Zalo.exe` or proprietary Zalo assets are embedded or packaged anywhere (`vng_binary_embedded: false`).

| Artifact Path (relative to `desktop-instance-manager/`) | Classification | Size (bytes) | SHA-256 | Signing & Provenance Status |
| :--- | :--- | ---: | :--- | :--- |
| `apps/desktop/src-tauri/target/x86_64-pc-windows-gnu/release/dim-desktop.exe` | `compiled-executable` (`is_installer: false`) | `21,586,008` | `157117cbce3a4309cedb9f279fbce869580f75d8120b33550d445a579bf53f4c` | `UNSIGNED_DEVELOPMENT_ARTIFACT` / `SELF_REPORTED_UNATTESTED` |
| `target/x86_64-pc-windows-gnu/release/dim-cli.exe` | `compiled-executable` (`is_installer: false`) | `2,209,966` | `24132f61429179962f216e200cea04cc52a36a7f675f6408c460764c67c07c98` | `UNSIGNED_DEVELOPMENT_ARTIFACT` / `SELF_REPORTED_UNATTESTED` |
| `target/x86_64-pc-windows-gnu/release/dim-test-fixture.exe` | `compiled-executable` (`is_installer: false`) | `1,616,671` | `4ea0f248b17d087a6a630d9ac58c2f1e862ea9d86784a18164075289ff1bba7c` | `UNSIGNED_DEVELOPMENT_ARTIFACT` / `SELF_REPORTED_UNATTESTED` |
| `apps/desktop/src-tauri/target/x86_64-pc-windows-gnu/release/WebView2Loader.dll` | `shared-library-sidecar` (`is_installer: false`) | `160,880` | `86545b66cdb0603bc26b626fb9ad610cb6e71f28d468f5ea66df23b03dda96d5` | Vendor-signed (`Valid`, `CN=Microsoft Corporation`) |

### 3.2. MSI & NSIS Installer Packaging Status (`SKIPPED_MISSING_TOOLING` Locally vs `windows-2022` CI)

- **Local Status:** `bundle_status = "SKIPPED_MISSING_TOOLING"` — no `.msi` or NSIS `-setup.exe` installer bundles were generated locally because `@tauri-apps/cli@2.12.5` (`cargo-tauri`), WiX Toolset (`candle.exe`/`light.exe`), and NSIS (`makensis.exe`) are not installed on this workstation. No unverified installer binaries were downloaded and no fake archive/wrapper was created.
- **CI `windows-2022` Native Path:** GitHub Actions `windows-2022` runners include Visual Studio 2022 MSVC (`x86_64-pc-windows-msvc`), WiX Toolset v3.14, and NSIS 3.x preinstalled. In `.github/workflows/cross-platform-validation.yml`, `build-and-package.mjs` invokes pinned `@tauri-apps/cli@2.12.5` with `--config scripts/ci-cross-platform/tauri-ci-bundle-overlay.json -- --locked` to produce `bundle/msi/*.msi` and `bundle/nsis/*-setup.exe` (still classified as `UNSIGNED_DEVELOPMENT_ARTIFACT` until Gate G3 Authenticode signing is configured).

---

## 4. PE32+ Header, Embedded XML Manifest, DLL Import & Authenticode Forensics

### 4.1. PE COFF & Optional Header Properties

| Binary | COFF Machine | Optional Magic | Subsystem | `DllCharacteristics` | `IMAGE_DIRECTORY_ENTRY_SECURITY` |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `dim-cli.exe` | `0x8664` (`AMD64`) | `0x020b` (`PE32+`) | `3` (`WINDOWS_CUI` Console) | `0x0160` (`HIGH_ENTROPY_VA \| DYNAMIC_BASE \| NX_COMPAT`) | `RVA=0, Size=0` (`NotSigned`) |
| `dim-test-fixture.exe` | `0x8664` (`AMD64`) | `0x020b` (`PE32+`) | `3` (`WINDOWS_CUI` Console) | `0x0160` (`HIGH_ENTROPY_VA \| DYNAMIC_BASE \| NX_COMPAT`) | `RVA=0, Size=0` (`NotSigned`) |
| `dim-desktop.exe` | `0x8664` (`AMD64`) | `0x020b` (`PE32+`) | `2` (`WINDOWS_GUI` Windowed) | `0x0160` (`HIGH_ENTROPY_VA \| DYNAMIC_BASE \| NX_COMPAT`) | `RVA=0, Size=0` (`NotSigned`) |
| `WebView2Loader.dll` | `0x8664` (`AMD64`) | `0x020b` (`PE32+`) | `3` (`WINDOWS_CUI`) | `0x4160` (`HIGH_ENTROPY_VA \| DYNAMIC_BASE \| NX_COMPAT \| GUARD_CF`) | Present (`Valid`, Microsoft Corp.) |

### 4.2. Embedded Application Manifest (`RT_MANIFEST`) & `IR-006` Finding

All three compiled executables embed `requestedExecutionLevel level="asInvoker" uiAccess="false"` and `longPathAware=true` (zero UAC elevation). Inspection of `tauri-build v2.7.5` (`windows-app-manifest.xml`) and `dim-desktop.exe`'s `.rsrc` section (`blocks_count: 2`, `common_controls_v6: true`) confirmed why MinGW `ld.exe` emitted `.rsrc merge failure: multiple non-default manifests`: `tauri-build`'s default XML only declares Common Controls v6 while `rustc` injects a second manifest (`asInvoker` + `longPathAware`). Patch [`ir-patches/0006-IR-006-release-profile-and-windows-manifest.patch`](./ir-patches/0006-IR-006-release-profile-and-windows-manifest.patch) supplies a unified `WindowsAttributes::app_manifest` containing Common Controls v6, `asInvoker`, `PerMonitorV2` DPI awareness, `longPathAware`, and Windows 10/11 `<supportedOS>` GUID `{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}` plus `-C control-flow-guard=checks` for `x86_64-pc-windows-msvc`.

### 4.3. Dynamic Library Imports & Smart App Control (SAC) Posture

- **`dim-desktop.exe`:** Imports `WebView2Loader.dll` (`160,880` B sidecar in `apps/desktop/src-tauri/target/x86_64-pc-windows-gnu/release/WebView2Loader.dll`), Windows Universal CRT (`api-ms-win-crt-*.dll`), and Win32 system DLLs (`KERNEL32.dll`, `USER32.dll`, `ADVAPI32.dll`, `CRYPT32.dll`, `WINTRUST.dll`, `RstrtMgr.DLL`, `USERENV.dll`, `SHELL32.dll`, `OLE32.dll`, `COMBASE.dll`, `DWMAPI.dll`, `GDI32.dll`), with zero external MinGW runtime DLLs.
- **SAC Policy:** Windows Smart App Control (`VerifiedAndReputablePolicyState = 1`) blocks unsigned PE executables at kernel load time. Never disable SAC or use signed PowerShell bypasses; SAC-enabled Windows 11 endpoints run via the Signed-Host Bridge (`scripts/dim-host.ps1` + `apps/desktop/ui-server.mjs`) until Gate G3 Authenticode signing is provisioned.

---

## 5. Cross-Platform Provenance, Supply-Chain & Atomic Patch Verification

1. **`build-and-package.mjs` + `generate-artifact-manifest.mjs` (`schema_version: 2`):**
   - Verified [`artifact-validation-manifest.json`](./artifact-validation-manifest.json) (`artifacts_count = 3`, `test_status = "PASSED_NATIVE"`, `runtime_fixture_status = "PASSED_NATIVE_FIXTURE_AND_DIM_CLI"`, `bundle_status = "SKIPPED_MISSING_TOOLING"`, `provenance_trust_level = "SELF_REPORTED_UNATTESTED"`, `zero_vng_binary_policy_passed = true`) and [`SHA256SUMS.txt`](./SHA256SUMS.txt).
2. **Real Vulnerability Advisory Scan (`dependency-audit-report.json`):**
   - Verified via live `cargo-deny 0.21.0` against `rustsec/advisory-db` @ `233155b4025b854c68a4b199c0e32d7fd53291a2`: **`0` vulnerabilities, `0` yanked crates, `0` banned crates, `0` SPDX license violations** across both lockfiles (`439` unique crates), strictly separated across all 7 supply-chain dimensions.
3. **Atomic Integration Patch Series (`ir-patches/0001..0007*.patch`):**
   - All 7 single-concern patches documented in [`integration-requests.md`](./integration-requests.md) passed `git -C C:/Users/manhquy/Downloads/zalo apply --check` both individually and sequentially (`exit 0`), leaving `main` and shared files in `build/cross-platform-distribution` completely untouched.
