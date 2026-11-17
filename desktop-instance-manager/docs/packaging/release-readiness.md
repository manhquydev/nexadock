# Cross-Platform Release Readiness Scorecard & Evidence Report

**Document Owner:** Release Governance & Build Matrix Architecture  
**Target Application:** `Multi-Zalo Desktop` (`desktop-instance-manager` v`0.1.0`, `com.multizalodesktop.manager`)  
**Worktree Branch:** `build/cross-platform-distribution`  
**Evaluated Commit SHA:** `a5feb4a520798064c1508c26fe4c4ac17a3a03cf` (`a5feb4a`, 1 commit ahead of `origin/main` `8cf47035f0f06bd97cc99aff80d81fb103af8`)  
**Assessment Date (UTC):** `2026-11-17`  
**Overall Public Production Release Verdict:** **`NOT_READY_FOR_PUBLIC_RELEASE` (`BLOCKED` at Gates G3, G4, G5 & Live Multi-OS CI Execution)**  
**Local Engineering / Evaluation Verdict:** **`READY_FOR_LOCAL_WINDOWS_DEV_EVALUATION`** (`UNSIGNED_DEVELOPMENT_ARTIFACT` binaries verified locally; SAC-enforcing Windows 11 hosts supported via ADR-002 Signed-Host Bridge without modifying OS security policy)

> [!WARNING]
> **Mandatory Honesty Attestations:**
> 1. **Live GitHub Actions Status:** **`CI_LIVE_RUN_STATUS: NOT_EXECUTED (STATICALLY_VALIDATED_ONLY)`** — remote GitHub Actions workflows have **not** been executed (`0` remote runs on private repository `manhquydev/zalo`). Nothing in this scorecard claims `CI PASS` on remote runners.
> 2. **Binary vs. Installer Distinction:** Locally built `dim-desktop.exe`, `dim-cli.exe`, and `dim-test-fixture.exe` are **compiled executables (`is_installer: false`)**, **never** OS installers. Local `.msi` and NSIS `-setup.exe` bundling was **`SKIPPED_MISSING_TOOLING`**.
> 3. **Signing Posture:** All compiled binaries carry **`signing_status: "UNSIGNED_DEVELOPMENT_ARTIFACT"`** (`Authenticode = NotSigned`) and are **never** production or signed releases.
> 4. **DIM Build != Zalo Runtime Proof:** Compiling or packaging `dim-desktop` proves manager build integrity only — it **never** proves macOS `Zalo.app` multi-instance runtime compatibility (`NOT_TESTED`) or Linux Zalo availability (`UNSUPPORTED` for official VNG Zalo).
> 5. **Zero VNG Binary Policy:** `0` proprietary VNG Zalo binaries or `app.asar` archives are embedded, downloaded, or redistributed anywhere (`zero_vng_binary_policy_passed = true`).

---

## 1. Executive Baseline & Evidence Ledger

Every metric in this scorecard is grounded directly in machine-generated and audited build reports under [`plans/reports/build/`](../../plans/reports/build/):

| Baseline Dimension | Verified Value | Primary Evidence File |
| :--- | :--- | :--- |
| **Git Commit SHA (`HEAD`)** | `a5feb4a520798064c1508c26fe4c4ac17a3a03cf` (`a5feb4a`) | [`artifact-validation-manifest.json`](../../plans/reports/build/artifact-validation-manifest.json) |
| **Remote `origin/main` State** | `8cf47035f0f06bd97cc99aff80d81fb103af8` (`local main ahead 1`, unpushed) | [`ci-baseline-and-github-audit.md`](../../plans/reports/build/ci-baseline-and-github-audit.md) |
| **Repository Visibility & Cost Tier** | **`private: true`** (`manhquydev/zalo`; Actions multipliers: Linux `1x`, Windows `2x`, macOS `10x`) | [`ci-baseline-and-github-audit.md`](../../plans/reports/build/ci-baseline-and-github-audit.md) |
| **Local Rust Toolchain** | `rustc 1.100.0 (0ef1bc386 2026-11-09)` / `cargo 1.100.0 (3acc79c0c 2026-11-05)` (`host: x86_64-pc-windows-gnu`, WinLibs MinGW-w64 UCRT64 `gcc 16.2.0`) | [`windows-local-build-report.md`](../../plans/reports/build/windows-local-build-report.md) |
| **Effective Packaging MSRV** | **`1.90`** (`apps/desktop/src-tauri/Cargo.toml`; root `Cargo.toml` is `1.85` — flagged as `IR-001`) | [`dependency-audit-report.json`](../../plans/reports/build/dependency-audit-report.json) |
| **Locked Tauri 2 Version** | `tauri 2.12.6` (`tauri-build 2.7.5`, `tauri-runtime 2.12.3`, `tauri-utils 2.10.3`) | [`dependency-audit-report.json`](../../plans/reports/build/dependency-audit-report.json) |
| **Dual Lockfile Audit (`G0`)** | **PASS** (`439` unique crates across `41` workspace + `434` Tauri; `431/431` `crates.io` SHA-256 checksums; `0` banned `openssl*`; `0` SPDX violations) | [`dependency-audit-summary.md`](../../plans/reports/build/dependency-audit-summary.md) |
| **Zero VNG Binary Scan (`G1`)** | **PASS** (`0` VNG binaries in repo or build outputs; `vng_binary_embedded: false` on `3/3` artifacts) | [`artifact-validation-manifest.json`](../../plans/reports/build/artifact-validation-manifest.json) |
| **Static CI & Tooling Validation** | **`STATICALLY_VALIDATED_PASS`** (`validate-workflow.mjs`: `47/47 PASS`; `selftest.mjs`: `15/15 PASS`) | [`ci-baseline-and-github-audit.md`](../../plans/reports/build/ci-baseline-and-github-audit.md) |
| **Live GitHub Actions Execution** | **`CI_LIVE_RUN_STATUS: NOT_EXECUTED (STATICALLY_VALIDATED_ONLY)`** (`0` remote workflows/runs; `gh` OAuth lacks `workflow` & `user` scopes) | [`ci-baseline-and-github-audit.md`](../../plans/reports/build/ci-baseline-and-github-audit.md) |
| **Local Rust Unit & Lint Suite** | **PASS (`--locked`)** (`cargo fmt`, `cargo clippy -D warnings` `0` warnings, `29/29` workspace tests PASS + `src-tauri` test harness PASS) | [`windows-local-build-report.md`](../../plans/reports/build/windows-local-build-report.md) |

---

## 2. Per-OS & Per-Target Readiness Scorecard

Notice how **DIM Manager Build/Package Readiness** and **Zalo Multi-Instance Runtime Readiness** are tracked in separate columns so compile/bundle progress is never conflated with Zalo client compatibility:

| OS & Architecture | Rust Target Triple | DIM Compile & Unit Test Status | OS Installer / Bundle Status | Signing & Notarization Status | Live GitHub Actions Runner Status | Zalo Multi-Instance Runtime Status | Production Release Gate Verdict |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **Windows 10/11 x64 *(Canonical CI Target)*** | `x86_64-pc-windows-msvc` | **Statically Validated** *(local host lacks VS C++ MSVC Build Tools / Windows SDK; `cargo check` exits `101` `E0463`)* | **Configured in CI** (`.msi` via WiX v3.14 + `-setup.exe` via NSIS 3.x on `windows-latest`); **Not Built Locally** | `UNSIGNED_DEVELOPMENT_ARTIFACT` *(Gate G3 blocked: no OV/EV Authenticode cert)* | **`NOT_EXECUTED`** (`STATICALLY_VALIDATED_ONLY`) | **L1/L2 PASS Locally** (`3` concurrent official Zalo `26.10.20` instances + primary); L3 QR `NOT_TESTED`; L4 calls/toasts `BLOCKED` | **`BLOCKED` (Gates G3 & Remote MSVC CI Run)** |
| **Windows 10/11 x64 *(Local UCRT64 Host Build)*** | `x86_64-pc-windows-gnu` | **Executed & Verified Locally** (`29/29` workspace tests PASS, `0` Clippy warnings, `3` PE32+ release `.exe` binaries built `--locked`) | **`SKIPPED_MISSING_TOOLING`** (`cargo-tauri`, WiX, and NSIS absent on local host; zero fake installers created) | `UNSIGNED_DEVELOPMENT_ARTIFACT` (`Authenticode = NotSigned`; blocked by SAC=`1` unless using Signed-Host Bridge) | Local Workstation Build *(not canonical CI target)* | **L1/L2 PASS Locally** (same Windows host runtime; Rust `dim-cli.exe` == PowerShell `dim-host.ps1` contract parity verified) | **`LOCAL_DEV_ONLY` (`BLOCKED` for Public Release at Gate G3)** |
| **macOS 14+ Apple Silicon** | `aarch64-apple-darwin` | **CI-Configured & Host-Agnostic Unit Tests Verified** (`3/3` `dim-platform-macos` unit tests pass on Windows) | **CI-Configured** (`.app` + `.dmg` in `macos-arm64-build`, opt-in `false`); **Blocked on `IR-002`** (`icons/icon.icns` missing) | `UNSIGNED_DEVELOPMENT_ARTIFACT` / `NOT_NOTARIZED` *(Gate G4 blocked; Gatekeeper quarantines)* | **`NOT_EXECUTED`** (`STATICALLY_VALIDATED_ONLY`, 10x cost guard active) | **`NOT_TESTED`** on physical Mac hardware (`open -n --env CFFIXED_USER_HOME` unverified; `/tmp/socketzalorecv2021` calls `BLOCKED`) | **`BLOCKED` (Gate G4, `IR-002`, Live CI & Physical Mac Test)** |
| **macOS 14+ Intel Mac** | `x86_64-apple-darwin` | **CI-Configured** (`macos-14` universal Xcode SDK cross-target + `lipo -archs` `x86_64` check) | **CI-Configured** (`.app` + `.dmg` in `macos-intel-build`, opt-in `false`); **Blocked on `IR-002`** (`icons/icon.icns` missing) | `UNSIGNED_DEVELOPMENT_ARTIFACT` / `NOT_NOTARIZED` *(Gate G4 blocked; Gatekeeper quarantines)* | **`NOT_EXECUTED`** (`STATICALLY_VALIDATED_ONLY`, 10x cost guard active) | **`NOT_TESTED`** on physical Intel Mac hardware (`/tmp/socketzalorecv2021` calls `BLOCKED`) | **`BLOCKED` (Gate G4, `IR-002`, Live CI & Physical Mac Test)** |
| **Ubuntu 24.04 LTS (Noble)** | `x86_64-unknown-linux-gnu` | **CI-Configured & Host-Agnostic Unit Tests Verified** (`3/3` `dim-platform-linux` unit tests pass on Windows) | **CI-Configured** (`.deb` + `.AppImage` attempt in `ubuntu-2404-build` with WebKitGTK 4.1 & Ayatana AppIndicator); needs `IR-003` `deb.depends` | `UNSIGNED_DEVELOPMENT_ARTIFACT` *(Gate G5 blocked: no detached GPG/minisign release signature)* | **`NOT_EXECUTED`** (`STATICALLY_VALIDATED_ONLY`) | **Official VNG Zalo: `UNSUPPORTED`** (no VNG Linux build); Fixture / User AppImage: **`EXPERIMENTAL` (`NOT_TESTED` on Linux host)** | **`BLOCKED` (Gate G5, Live CI, Linux Host Test; Official Zalo Unsupported)** |

---

## 3. Verified Local Windows `x86_64` Build Artifacts (`artifact-validation-manifest.json`)

Because Visual Studio C++ MSVC Build Tools (`cl.exe`/`link.exe`), the Windows 10/11 SDK, `cargo-tauri`, WiX, and NSIS are not installed on this Windows 11 Pro (`10.0.26300` x64) workstation (and unverified binary downloads were strictly prohibited), local `--locked` compilation and PE32+ verification were executed using the installed `rustc 1.100.0` (`x86_64-pc-windows-gnu`) + WinLibs MinGW-w64 UCRT64 (`gcc 16.2.0`) toolchain.

### 3.1 Deliverable Executables Recorded in [`artifact-validation-manifest.json`](../../plans/reports/build/artifact-validation-manifest.json) & [`SHA256SUMS.txt`](../../plans/reports/build/SHA256SUMS.txt)

| Artifact Name | Workspace-Relative Path | `artifact_kind` (`is_installer`) | Target Triple & PE Format | Exact Size (Bytes) | Verified SHA-256 Digest | `signing_status` | PE Header & Manifest Forensics |
| :--- | :--- | :--- | :--- | ---: | :--- | :--- | :--- |
| **`dim-desktop.exe`** | `apps/desktop/src-tauri/target/x86_64-pc-windows-gnu/release/dim-desktop.exe` | `compiled-executable` (**`false`** — *NOT an installer*) | `x86_64-pc-windows-gnu` (`PE32+` `0x8664`, `WINDOWS_GUI`) | **`21,586,008`** | `eb1a024ef4a5b0ff6bb75596484658650ad348cd176bc3a611e8dc1b315fe4eb` | `UNSIGNED_DEVELOPMENT_ARTIFACT` | `asInvoker` (`uiAccess="false"`, `longPathAware=true`), `HIGH_ENTROPY_VA \| DYNAMIC_BASE \| NX_COMPAT`, imports `WebView2Loader.dll` + Windows UCRT (`0` MinGW runtime DLL deps), `Authenticode RVA=0/Size=0` |
| **`dim-cli.exe`** | `target/x86_64-pc-windows-gnu/release/dim-cli.exe` | `compiled-executable` (**`false`** — *NOT an installer*) | `x86_64-pc-windows-gnu` (`PE32+` `0x8664`, `WINDOWS_CUI`) | **`2,209,966`** | `24132f61429179962f216e200cea04cc52a36a7f675f6408c460764c67c07c98` | `UNSIGNED_DEVELOPMENT_ARTIFACT` | `asInvoker` (`uiAccess="false"`, `longPathAware=true`), `HIGH_ENTROPY_VA \| DYNAMIC_BASE \| NX_COMPAT`, Windows UCRT + Win32 `WinVerifyTrust`/`RstrtMgr`, `Authenticode RVA=0/Size=0` |
| **`dim-test-fixture.exe`** | `target/x86_64-pc-windows-gnu/release/dim-test-fixture.exe` | `compiled-executable` (**`false`** — *NOT an installer*) | `x86_64-pc-windows-gnu` (`PE32+` `0x8664`, `WINDOWS_CUI`) | **`1,616,671`** | `4ea0f248b17d087a6a630d9ac58c2f1e862ea9d86784a18164075289ff1bba7c` | `UNSIGNED_DEVELOPMENT_ARTIFACT` | `asInvoker` (`uiAccess="false"`, `longPathAware=true`), `HIGH_ENTROPY_VA \| DYNAMIC_BASE \| NX_COMPAT`, synthetic lockfile/lifecycle test target, `Authenticode RVA=0/Size=0` |

### 3.2 Companion Runtime Sidecar & Local Bundler Status
- **`WebView2Loader.dll` Sidecar:** Emitted by `webview2-com-sys v0.39.1` alongside `dim-desktop.exe` at `apps/desktop/src-tauri/target/x86_64-pc-windows-gnu/release/WebView2Loader.dll` (`160,880` bytes, SHA-256 `86545b66cdb0603bc26b626fb9ad610cb6e71f28d468f5ea66df23b03dda96d5`, vendor-signed `CN=Microsoft Corporation`). Verified via `dim-desktop.exe`'s `inspection.pe.sidecar_webview2_loader` record; intentionally excluded from standalone deliverable rows by `generate-artifact-manifest.mjs` because `.dll` files are companion shared libraries, not standalone executables or installers. Host WebView2 Evergreen Runtime `157.0.4341.46` was verified read-only.
- **Local Windows Bundler Status (`SKIPPED_MISSING_TOOLING`):** Zero `.msi` or NSIS `-setup.exe` files exist locally because `cargo-tauri`, WiX (`candle.exe`/`light.exe`), and NSIS (`makensis.exe`) are not installed on this workstation. Both WiX v3.14 and NSIS 3.x are preinstalled on GitHub Actions `windows-latest` for `windows-msvc-build`.

---

## 4. Detailed Platform Readiness Assessments

### 4.1 Windows 10 / 11 (`x86_64`)
- **What Is Proven Today:**
  - Full Rust workspace (`7` crates, `29/29` unit tests) and Tauri `2.12.6` desktop shell compile cleanly with `--locked` and zero Clippy warnings.
  - Real official VNG Zalo Desktop (`26.10.20`, verified via `WinVerifyTrust` + `CN/O=VNG GROUP JSC` RDN pinning) is verified at **L1 (Process & Storage Isolation)** and **L2 (Lifecycle & Binary Integrity)** with `3` concurrent isolated instances running alongside an untouched primary `%APPDATA%\ZaloData` session, plus 100% contract parity between `dim-cli.exe` and `scripts/dim-host.ps1`.
- **What Remains Unexecuted or Blocked:**
  - Canonical `x86_64-pc-windows-msvc` compile and `.msi` / NSIS `-setup.exe` bundling on `windows-latest` (`CI_LIVE_RUN_STATUS: NOT_EXECUTED`).
  - **Gate G3 (Authenticode Code Signing):** Without OV/EV Authenticode or Azure Trusted Signing + RFC 3161 timestamping across inner `.exe`s, NSIS uninstaller, `.msi`, and `-setup.exe`, Windows 11 endpoints with Smart App Control enforced (`VerifiedAndReputablePolicyState = 1`) block standalone PE execution. (SAC must **never** be disabled; local evaluation on SAC=`1` hosts uses ADR-002's Signed-Host Bridge.)
  - **L3 Interactive Multi-Account QR Verification:** `NOT_TESTED` (requires human two-phone QR sign-in via `scripts/run-interactive-l3-checklist.ps1`).
  - **L4 Concurrent Calls & Per-Profile Toast Attribution:** `BLOCKED` by VNG Zalo's global named pipes (`\\.\pipe\PipeZCallRecv` / `PipeZCallSend`) and hardcoded `AppUserModelId("com.vng.zalo")`.

### 4.2 macOS 14+ (`aarch64-apple-darwin` & `x86_64-apple-darwin`)
- **What Is Proven Today:**
  - `dim-platform-macos` compiles cleanly and passes `3/3` host-agnostic unit tests (`SingletonLock` `<host>-<pid>` symlink parsing, `open -n --env CFFIXED_USER_HOME` argument construction, and `Info.plist` version extraction).
  - `.github/workflows/cross-platform-validation.yml` defines `macos-arm64-build` (`macos-14`, `aarch64-apple-darwin`) and `macos-intel-build` (`macos-14` universal Xcode `MacOSX.sdk` -> `x86_64-apple-darwin` verified with `lipo -archs`), passing `47/47` static workflow checks (`validate-workflow.mjs`).
- **What Remains Unexecuted or Blocked:**
  - Live `macos-14` CI execution (`NOT_EXECUTED`; opt-in `false` by default due to private-repo `10x` billing multiplier).
  - **`IR-002` Icon Blocker:** `apps/desktop/src-tauri/icons/icon.icns` is currently missing from the repository and `tauri.conf.json`, which degrades or blocks Tauri `.app`/`.dmg` bundling on macOS.
  - **Gate G4 (Developer ID Signing & Notarization):** No Apple Developer ID Application certificate or App Store Connect notarization credentials are provisioned; unsigned `.app`/`.dmg` builds (`UNSIGNED_DEVELOPMENT_ARTIFACT` / `NOT_NOTARIZED`) are quarantined by macOS Gatekeeper on clean Macs.
  - **Zalo macOS Runtime Compatibility (`NOT_TESTED`):** Even after `.app`/`.dmg` packaging succeeds on CI, `open -n -a /Applications/Zalo.app --env CFFIXED_USER_HOME=...`, macOS Keychain prompt behavior, and `/tmp/socketzalorecv2021` call socket contention have **not** been tested on physical Apple hardware.

### 4.3 Ubuntu 24.04 LTS (`x86_64-unknown-linux-gnu`)
- **What Is Proven Today:**
  - `dim-platform-linux` compiles cleanly and passes `3/3` unit tests (`SupportStatus::Unsupported` enforcement on `OfficialZalo`, `Experimental` status on local AppImage/fixture, and `HOME`/`XDG_*` + `bwrap` argument generation).
  - `ubuntu-2404-build` in `cross-platform-validation.yml` specifies exact Noble Numbat build packages (`libwebkit2gtk-4.1-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`, `patchelf`, `dpkg-dev`, `libfuse2t64`), `APPIMAGE_EXTRACT_AND_RUN=1`, `ldd` unresolved-symbol failure checks, and `GLIBC_*` symbol floor inspection.
- **What Remains Unexecuted or Blocked:**
  - Live `ubuntu-24.04` CI execution (`NOT_EXECUTED`) and Linux desktop runtime testing (`NOT_TESTED`).
  - **`IR-003` Debian Metadata & Gate G5 Integrity:** `tauri.conf.json` lacks explicit `bundle.linux.deb.depends` (`libwebkit2gtk-4.1-0`, `libgtk-3-0t64`, `libayatana-appindicator3-1`), and no detached release GPG/minisign signing key is provisioned.
  - **`GLIBC` 24.04 Floor & AppArmor Caveat:** Binaries built on Ubuntu 24.04 bind `GLIBC >= 2.38/2.39` and `libwebkit2gtk-4.1-0` (incompatible with Ubuntu 22.04 `glibc 2.35`), and Ubuntu 24.04 restricts unprivileged user namespaces via AppArmor (`kernel.apparmor_restrict_unprivileged_userns = 1`).
  - **Zalo Linux Runtime Compatibility (`UNSUPPORTED`):** VNG Corporation publishes **no** official native Linux Zalo Desktop client. Compiling or packaging `dim-desktop` into `.deb` or `.AppImage` never provides a Linux Zalo client and never validates third-party community Electron/Wine wrappers.

---

## 5. Out-of-Scope Integration Requests Summary (`IR-001` – `IR-007`)

Because `Cargo.toml`, `Cargo.lock`, `tauri.conf.json`, `icons/`, and `.github/workflows/ci.yml` were protected as read-only during Mission E, [`plans/reports/build/integration-requests.md`](../../plans/reports/build/integration-requests.md) catalogs seven concrete follow-up PR items:

| IR ID | Severity | Target File(s) | Required Change & Impact |
| :--- | :---: | :--- | :--- |
| **`IR-001`** | **Medium** | `desktop-instance-manager/Cargo.toml`, `adr-002-rust-tauri-and-sac-host-bridge.md` | Raise root workspace `rust-version` from `"1.85"` to `"1.90"` to match `apps/desktop/src-tauri/Cargo.toml` (`tauri 2.12.6`) and update stale `1.78` text in ADR-002. |
| **`IR-002`** | **High** | `apps/desktop/src-tauri/icons/*`, `apps/desktop/src-tauri/tauri.conf.json` | Generate original non-VNG multi-resolution RGBA icons including `icons/icon.icns` and `icons/128x128@2x.png` (required for macOS `.app`/`.dmg` bundling and HiDPI Windows installers). |
| **`IR-003`** | **High** | `apps/desktop/src-tauri/tauri.conf.json` | Add explicit `bundle.windows` (`downloadBootstrapper`, `nsis.installMode: "currentUser"`, `sha256`), `bundle.macOS` (`minimumSystemVersion: "13.0"`, `hardenedRuntime: true`), and `bundle.linux.deb.depends` (`libwebkit2gtk-4.1-0`, `libgtk-3-0t64`, `libayatana-appindicator3-1`). |
| **`IR-004`** | **Medium** | `Cargo.toml`, `apps/desktop/src-tauri/Cargo.toml`, CI workflows | Pin `tauri-cli` (`cargo-tauri`) exact version (e.g., `=2.5.0`) instead of `^2.0.0`, and evaluate unifying or formally maintaining dual `Cargo.lock` graphs (`41` vs `434` packages). |
| **`IR-005`** | **Medium** | `apps/desktop/src-tauri/Cargo.toml`, `apps/desktop/src-tauri/src/main.rs` | Add workspace-equivalent `[lints.rust]` (`unsafe_op_in_unsafe_fn = "deny"`) and `[lints.clippy]` (`unwrap_used = "warn"`, `expect_used = "warn"`) to `dim-desktop`. |
| **`IR-006`** | **Low** | `Cargo.toml`, `apps/desktop/src-tauri/Cargo.toml` | Add `[profile.release]` (`lto = "thin"`, `codegen-units = 1`, `strip = "symbols"`, `panic = "abort"`) and run release Windows builds on canonical `x86_64-pc-windows-msvc` (`GUARD_CF` + clean `RT_MANIFEST` embedding without GNU `ld.exe` `.rsrc` duplicate manifest warnings). |
| **`IR-007`** | **Medium** | `.github/workflows/ci.yml` | Replace `libappindicator3-dev` with `libayatana-appindicator3-dev` for Ubuntu 24.04 compatibility, add `src-tauri/Cargo.lock` to cache hash inputs, and pass `--locked` on all Cargo commands. |

---

## 6. Prioritized Production Release Blockers

Before any public `v0.1.0` end-user installer release is approved under **Gate G6**, the following blockers must be resolved in priority order:

1. **`[P0 — Gate G3]` Provision Windows Authenticode Signing & Timestamping:** Acquire an OV/EV Authenticode certificate or Azure Trusted Signing profile in protected release CI; sign inner `dim-desktop.exe`/`dim-cli.exe`, NSIS `uninstall.exe`, outer `.msi`, and `*-setup.exe` with SHA-256 + RFC 3161 timestamping; verify execution on Windows 11 with Smart App Control `VerifiedAndReputablePolicyState = 1`.
2. **`[P0 — CI & Packaging]` Authorize Remote Push & Execute `cross-platform-validation.yml` on Canonical Runners:** Refresh local GitHub CLI scopes (`gh auth refresh -h github.com -s workflow,user`), inspect private-repo Actions minute balance, merge `IR-002`/`IR-003` (`icon.icns` + `tauri.conf.json` bundle blocks), push with maintainer approval, and run `windows-msvc-build` (`x86_64-pc-windows-msvc`) + `ubuntu-2404-build` (and opt-in `macos-arm64-build` / `macos-intel-build` within 10x minute budget).
3. **`[P1 — Windows L3]` Complete Human Two-Account QR Functional Checklist:** Execute `scripts/run-interactive-l3-checklist.ps1` on Windows 10/11 with two live Zalo accounts to record authenticated message/file isolation evidence (`L3`) while clearly documenting VNG's `L4` concurrent-call/toast limitations in release notes.
4. **`[P1 — Gate G4 & macOS Runtime]` Physical Mac Validation + Apple Developer ID Notarization:** Validate `dim-platform-macos` (`open -n --env CFFIXED_USER_HOME`, Keychain behavior, `/tmp/socketzalorecv2021` impact) on physical Apple Silicon and Intel Macs with `/Applications/Zalo.app`, then provision Apple Developer ID Application signing + `notarytool` notarization + `stapler` ticket stapling.
5. **`[P2 — Gate G5 & Linux Scope]` Ubuntu 24.04 Package Signing & Experimental Scope Lock:** Sign `.deb` / `.AppImage` checksum manifests with a dedicated release GPG/minisign key, verify installation on clean Ubuntu 24.04 LTS (`GLIBC >= 2.38/2.39`, `libfuse2t64`), and keep Linux clearly labeled **Experimental (Fixture / User-Supplied Local AppImage Only — Official VNG Zalo Unsupported)**.

---

## 7. Related Packaging & Governance Documents

- Cross-Platform Build & Provenance Matrix: [`build-matrix.md`](./build-matrix.md)
- Release Gate Framework (G0–G6): [`signing-and-release-gates.md`](./signing-and-release-gates.md)
- Per-OS Packaging Guides: [`windows.md`](./windows.md) | [`macos.md`](./macos.md) | [`ubuntu.md`](./ubuntu.md)
- Raw Build & Audit Evidence (`plans/reports/build/`):
  - [`ci-baseline-and-github-audit.md`](../../plans/reports/build/ci-baseline-and-github-audit.md)
  - [`dependency-audit-summary.md`](../../plans/reports/build/dependency-audit-summary.md) & [`dependency-audit-report.json`](../../plans/reports/build/dependency-audit-report.json)
  - [`windows-local-build-report.md`](../../plans/reports/build/windows-local-build-report.md)
  - [`artifact-validation-manifest.json`](../../plans/reports/build/artifact-validation-manifest.json) & [`SHA256SUMS.txt`](../../plans/reports/build/SHA256SUMS.txt)
  - [`integration-requests.md`](../../plans/reports/build/integration-requests.md)
