# Cross-Platform Release Readiness Scorecard & Evidence Report

**Document Owner:** Release Governance & Build Matrix Architecture  
**Target Application:** `Multi-Zalo Desktop` (`desktop-instance-manager` v`0.1.0`, `com.multizalodesktop.manager`)  
**Worktree Branch:** `build/cross-platform-distribution` (Base `main` @ `a5feb4a520798064c1508c26fe4c4ac17a3a03cf`; Manifest Commit SHA `ac0df10c795208d983a2fb8d03f95ff627adf59c`)  
**Remote `origin/main` Commit:** `8cf47035f0f06bd97cc99aff80d81fb10b103af8` (`8cf4703`, local `main` is `ahead 1` unpushed)  
**Assessment Date:** `2026-11-18`  
**Overall Public Production Release Verdict:** **`NOT_READY_FOR_PUBLIC_RELEASE` (`BLOCKED` at Gates G3, G4, G5 & Remote Multi-OS CI Execution)**  
**Local Engineering / Evaluation Verdict:** **`READY_FOR_LOCAL_WINDOWS_DEV_EVALUATION`** (`UNSIGNED_DEVELOPMENT_ARTIFACT` binaries, real `cargo-deny` RustSec scan, and native runtime fixture verified locally; SAC=`1` Windows 11 hosts supported via ADR-002 Signed-Host Bridge without disabling OS policy)

> [!WARNING]
> **Mandatory Honesty Attestations:**
> 1. **Remote GitHub Actions Status:** **`BLOCKED (REMOTE_WORKFLOW_DISPATCH_UNREGISTERED_AND_MISSING_WORKFLOW_SCOPE)`** — `CI_LIVE_RUN_STATUS: NOT_EXECUTED (STATICALLY_VALIDATED_ONLY)` (`0` remote runs on private repository `manhquydev/zalo`). Never claim `CI PASS` on remote runners.
> 2. **Native Runner Rule:** `macos-15` (`arm64`) and `macos-15-intel` (`x86_64`) use explicit native hardware gates (`uname -m` + `rustc -vV`); cross-compiling is **never** accepted as native runtime testing (`BLOCKED` if `macos-15-intel` unavailable).
> 3. **Binary != Installer & 7-Pillar Supply-Chain Separation:** Raw `.exe`/ELF files have `is_installer: false`; all builds are `UNSIGNED_DEVELOPMENT_ARTIFACT` with `provenance_trust_level: "SELF_REPORTED_UNATTESTED"`; an SBOM (`439` components) is a declared inventory, **never** independent build provenance.
> 4. **DIM Build != Zalo Runtime Proof:** Manager compilation/fixture checks **never** prove macOS `Zalo.app` multi-instance isolation (`NOT_TESTED`) or Linux Zalo support (`UNSUPPORTED` for official VNG Zalo). `0` VNG binaries are bundled anywhere (`zero_vng_binary_policy_passed = true`).

---

## 1. Executive Baseline & 7-Pillar Evidence Ledger

Grounded directly in audited reports under [`plans/reports/build/`](../../plans/reports/build/) and [`ci-execution-and-artifact-validation.md`](./ci-execution-and-artifact-validation.md):

| Dimension / Pillar | Verified Status & Exact Metrics | Primary Evidence File |
| :--- | :--- | :--- |
| **Git Lineage** | Base `main`: `a5feb4a520798064c1508c26fe4c4ac17a3a03cf`; Manifest build SHA: `ac0df10c795208d983a2fb8d03f95ff627adf59c`; `origin/main`: `8cf47035f0f06bd97cc99aff80d81fb10b103af8` | [`ci-baseline-and-github-audit.md`](../../plans/reports/build/ci-baseline-and-github-audit.md) |
| **Toolchain & Pinned Bundler** | `rustc 1.100.0 (0ef1bc386 2026-11-09)` (`x86_64-pc-windows-gnu` local), effective MSRV **`1.90`**, `tauri 2.12.6` (`tauri-build 2.7.5`), pinned **`@tauri-apps/cli@2.12.5`** | [`windows-local-build-report.md`](../../plans/reports/build/windows-local-build-report.md) |
| **Pillar 1 — Lockfile Checksums** | **`VERIFIED`** (`431/431` `crates.io-index` SHA-256 digests across `41` workspace + `434` Tauri lockfile entries; `--locked` enforced) | [`dependency-audit-report.json`](../../plans/reports/build/dependency-audit-report.json) |
| **Pillar 2 — License & Ban Policy** | **`VERIFIED`** (`439` unique crates evaluated via `cargo metadata --locked` & `cargo-deny`; `0` SPDX violations; `0` banned `openssl*`) | [`dependency-audit-summary.md`](../../plans/reports/build/dependency-audit-summary.md) |
| **Pillar 3 — Vulnerability Advisories** | **`VERIFIED_CARGO_DENY_RUSTSEC_CLEAN`** (`cargo-deny 0.21.0` vs live `rustsec/advisory-db` @ `233155b4025b854c68a4b199c0e32d7fd53291a2`: **`0` CVEs, `0` yanked**) | [`dependency-audit-report.json`](../../plans/reports/build/dependency-audit-report.json) |
| **Pillar 4 — SBOM Inventory** | **`DECLARED_DEPENDENCY_INVENTORY_ONLY`** (`439` components; `is_independent_build_provenance: false`) | [`dependency-audit-report.json`](../../plans/reports/build/dependency-audit-report.json) |
| **Pillar 5 — Build Provenance** | **`SELF_REPORTED_UNATTESTED`** (`oidc_slsa_attestation_present: false` locally) | [`artifact-validation-manifest.json`](../../plans/reports/build/artifact-validation-manifest.json) |
| **Pillar 6 — Artifact Signature** | **`UNSIGNED_DEVELOPMENT_ARTIFACT`** (`Authenticode RVA=0/Size=0`, `NOT_NOTARIZED`) | [`artifact-validation-manifest.json`](../../plans/reports/build/artifact-validation-manifest.json) |
| **Pillar 7 — Installer Integrity** | **`RAW_EXECUTABLES_ONLY_LOCALLY_CI_OVERLAY_READY`** (`is_installer: false` on raw binaries; `tauri-ci-bundle-overlay.json` + valid `icon.icns` ready) | [`windows-local-build-report.md`](../../plans/reports/build/windows-local-build-report.md) |
| **Remote CI vs. Static Tooling** | Remote: **`BLOCKED` (`0` runs)** (`X-Oauth-Scopes: gist, read:org, repo` missing `workflow` + `origin/main` unregistered). Static: `validate-workflow.mjs` **`6/6 PASS`**, `selftest.mjs` **`16/16 PASS`** | [`ci-baseline-and-github-audit.md`](../../plans/reports/build/ci-baseline-and-github-audit.md) |
| **Private-Repo Cost Model** | `manhquydev/zalo` (`private: true`): `$0.006/m` Linux (`1x`), `$0.010/m` Win (`2x`), `$0.062/m` macOS (`10x`) -> **`$0.246/run` default** (`45` mult min) vs **`$2.044/run` 4-OS** (`335` mult min; **88.0% saved**) | [`ci-baseline-and-github-audit.md`](../../plans/reports/build/ci-baseline-and-github-audit.md) |

---

## 2. Per-OS & Per-Target Readiness Scorecard (Priority Order 1 -> 4)

| Priority & Target | Verified Runner Label & Triple | DIM Compile, Test & Native Fixture Status | OS Installer / Bundle Status | Signing & Provenance Status | Live GitHub Actions Status | Zalo Multi-Instance Runtime Status | Production Release Verdict |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **1. Ubuntu 24.04 LTS x64** | `ubuntu-24.04` (`x86_64-unknown-linux-gnu`) | `3/3` host-agnostic unit tests PASS locally; native 7-stage CI + `xvfb-run -a` + `fixture-app.mjs` configured | Configured via `tauri-ci-bundle-overlay.json` (`.deb` Noble `t64` `depends` + `.AppImage` `APPIMAGE_EXTRACT_AND_RUN=1`) | `UNSIGNED_DEVELOPMENT_ARTIFACT` / `SELF_REPORTED_UNATTESTED` | **`BLOCKED` (`NOT_EXECUTED`)** | **Official VNG Zalo: `UNSUPPORTED`**; Fixture / Local AppImage: **`EXPERIMENTAL` (`NOT_TESTED` on Linux)** | **`BLOCKED` (Gate G5, Remote CI; Official Zalo Unsupported)** |
| **2a. Windows x64 *(Canonical CI)*** | `windows-2022` (`x86_64-pc-windows-msvc`) | Native 7-stage MSVC pipeline configured *(local workstation lacks VS C++ Build Tools `E0463`)* | Configured on `windows-2022` (preinstalled WiX v3.14 `.msi` + NSIS 3.x `-setup.exe` + `downloadBootstrapper` overlay) | `UNSIGNED_DEVELOPMENT_ARTIFACT` / `SELF_REPORTED_UNATTESTED` | **`BLOCKED` (`NOT_EXECUTED`)** | **L1/L2 PASS Locally** (`3` concurrent Zalo `26.10.20` + primary); L3 QR `NOT_TESTED`; L4 calls/toasts `BLOCKED` | **`BLOCKED` (Gate G3 & Remote `windows-2022` Run)** |
| **2b. Windows x64 *(Local Native Host)*** | Windows 11 Pro `10.0.26300` (`x86_64-pc-windows-gnu`) | **Executed & PASS (`--locked`):** Clippy `0` warnings, `29/29` unit tests PASS, `fixture-app.mjs` + `dim-cli.exe` smoke **`PASSED_NATIVE_FIXTURE_AND_DIM_CLI`** | **`SKIPPED_MISSING_TOOLING`** (`@tauri-apps/cli@2.12.5`, WiX, NSIS absent locally; zero fake installers) | `UNSIGNED_DEVELOPMENT_ARTIFACT` / `SELF_REPORTED_UNATTESTED` | Local Workstation Execution | **L1/L2 PASS Locally** (Rust `dim-cli.exe` == PowerShell `dim-host.ps1` parity verified) | **`LOCAL_DEV_ONLY` (`BLOCKED` for Public SAC=`1` Release at Gate G3)** |
| **3. macOS 15 Apple Silicon** | `macos-15` (`aarch64-apple-darwin`, native `arm64`) | `3/3` host-agnostic unit tests PASS locally; native `arm64` 7-stage CI configured | Configured (`.app` + `.dmg` with valid generated `assets/icon.icns` `1,142` B via CI overlay) | `UNSIGNED_DEVELOPMENT_ARTIFACT` / `NOT_NOTARIZED` | **`BLOCKED` (`NOT_EXECUTED`, opt-in `false` `10x`)** | **`NOT_TESTED`** on physical Mac hardware (`/tmp/socketzalorecv2021` calls `BLOCKED`) | **`BLOCKED` (Gate G4, Remote CI & Physical Mac Test)** |
| **4. macOS 15 Intel Mac** | `macos-15-intel` (`x86_64-apple-darwin`, native `x86_64`) | Native 4-core Intel 7-stage CI configured (`macos-13` retired; never cross-compiled on `arm64`) | Configured (`.app` + `.dmg` with valid generated `assets/icon.icns` `1,142` B via CI overlay) | `UNSIGNED_DEVELOPMENT_ARTIFACT` / `NOT_NOTARIZED` | **`BLOCKED` (`NOT_EXECUTED`, opt-in `false` `10x`)** | **`NOT_TESTED`** on physical Intel Mac hardware (`/tmp/socketzalorecv2021` calls `BLOCKED`) | **`BLOCKED` (Gate G4, Remote CI & Physical Mac Test)** |

---

## 3. Verified Local Windows `x86_64` Build Artifacts (`artifact-validation-manifest.json` v2)

Recorded in [`artifact-validation-manifest.json`](../../plans/reports/build/artifact-validation-manifest.json) (`schema_version: 2`, commit `ac0df10c795208d983a2fb8d03f95ff627adf59c`) and [`SHA256SUMS.txt`](../../plans/reports/build/SHA256SUMS.txt):

| Artifact Name | Workspace-Relative Path | `artifact_kind` (`is_installer`) | Size (Bytes) | Verified SHA-256 Digest | Signing & Provenance Status | PE32+ & Runtime Verification |
| :--- | :--- | :--- | ---: | :--- | :--- | :--- |
| **`dim-desktop.exe`** | `apps/desktop/src-tauri/target/x86_64-pc-windows-gnu/release/dim-desktop.exe` | `compiled-executable` (**`false`** — *NOT an installer*) | **`21,586,008`** | `157117cbce3a4309cedb9f279fbce869580f75d8120b33550d445a579bf53f4c` | `UNSIGNED_DEVELOPMENT_ARTIFACT` / `SELF_REPORTED_UNATTESTED` | `PE32+` `0x8664` `WINDOWS_GUI`, `asInvoker`, `longPathAware=true`, `HIGH_ENTROPY_VA \| DYNAMIC_BASE \| NX_COMPAT`, imports `WebView2Loader.dll` + UCRT |
| **`dim-cli.exe`** | `target/x86_64-pc-windows-gnu/release/dim-cli.exe` | `compiled-executable` (**`false`** — *NOT an installer*) | **`2,209,966`** | `24132f61429179962f216e200cea04cc52a36a7f675f6408c460764c67c07c98` | `UNSIGNED_DEVELOPMENT_ARTIFACT` / `SELF_REPORTED_UNATTESTED` | `PE32+` `0x8664` `WINDOWS_CUI`, `asInvoker`, `PASSED_NATIVE_FIXTURE_AND_DIM_CLI` (`0.07s` live JSON status check) |
| **`dim-test-fixture.exe`** | `target/x86_64-pc-windows-gnu/release/dim-test-fixture.exe` | `compiled-executable` (**`false`** — *NOT an installer*) | **`1,616,671`** | `4ea0f248b17d087a6a630d9ac58c2f1e862ea9d86784a18164075289ff1bba7c` | `UNSIGNED_DEVELOPMENT_ARTIFACT` / `SELF_REPORTED_UNATTESTED` | `PE32+` `0x8664` `WINDOWS_CUI`, `asInvoker`, `PASSED_NATIVE` (`2/2` fixture unit tests) |

- **Companion Sidecar:** `apps/desktop/src-tauri/target/x86_64-pc-windows-gnu/release/WebView2Loader.dll` (`160,880` B, SHA-256 `86545b66cdb0603bc26b626fb9ad610cb6e71f28d468f5ea66df23b03dda96d5`, vendor-signed `CN=Microsoft Corporation`, host WebView2 Evergreen `157.0.4341.46`).

---

## 4. Verified Atomic Integration Patches (`plans/reports/build/ir-patches/`)

All **7 standalone single-concern patches** in [`plans/reports/build/integration-requests.md`](../../plans/reports/build/integration-requests.md) pass `git -C C:/Users/manhquy/Downloads/zalo apply --check` both individually and sequentially (`exit 0`), with **`0` file overlaps** across all 6 active worktrees (`main`, `feat/ui-vi-en-quiet-workspace`, `feat/macos-multi-instance`, `feat/ubuntu-multi-instance`, `test/cross-platform-reliability`, `research/desktop-compatibility`):

| Patch File (`plans/reports/build/ir-patches/`) | IR & Severity | Target File(s) | Summary & CI Mitigation Status |
| :--- | :--- | :--- | :--- |
| [`0001-IR-001-align-workspace-msrv-1.90.patch`](../../plans/reports/build/ir-patches/0001-IR-001-align-workspace-msrv-1.90.patch) | **IR-001** (Med) | `Cargo.toml`, `adr-002-...md` | Aligns root workspace `rust-version = "1.90"` with `src-tauri` (`tauri 2.12.6`) and refreshes ADR-002. |
| [`0002-IR-002-macos-icns-and-retina-icons.patch`](../../plans/reports/build/ir-patches/0002-IR-002-macos-icns-and-retina-icons.patch) | **IR-002** (High) | `icons/icon.icns`, `icons/128x128@2x.png`, `tauri.conf.json` | Adds valid `icon.icns` (`1,142` B) & `128x128@2x.png` (`716` B); **already mitigated in CI via `tauri-ci-bundle-overlay.json`**. |
| [`0003-IR-003-tauri-os-bundle-metadata.patch`](../../plans/reports/build/ir-patches/0003-IR-003-tauri-os-bundle-metadata.patch) | **IR-003** (High) | `apps/desktop/src-tauri/tauri.conf.json` | Explicit Windows `downloadBootstrapper`/`currentUser` NSIS, macOS `11.0`, Ubuntu 24.04 `deb.depends`; **mitigated in CI via overlay**. |
| [`0004-IR-004-pin-tauri-cli-and-lockfile-reproducibility.patch`](../../plans/reports/build/ir-patches/0004-IR-004-pin-tauri-cli-and-lockfile-reproducibility.patch) | **IR-004** (Med) | `src-tauri/Cargo.toml`, `deny.toml`, `rust-toolchain.toml` | Pins `@tauri-apps/cli@2.12.5`, sets `publish = false`, removes deprecated `version = 2` in `deny.toml` for `cargo-deny 0.21.0`. |
| [`0005-IR-005-src-tauri-workspace-lints-parity.patch`](../../plans/reports/build/ir-patches/0005-IR-005-src-tauri-workspace-lints-parity.patch) | **IR-005** (Med) | `src-tauri/Cargo.toml`, `src-tauri/src/main.rs` | Enforces `unsafe_op_in_unsafe_fn = "deny"` & replaces startup `.expect(...)` with structured exit. |
| [`0006-IR-006-release-profile-and-windows-manifest.patch`](../../plans/reports/build/ir-patches/0006-IR-006-release-profile-and-windows-manifest.patch) | **IR-006** (Low) | `Cargo.toml`, `src-tauri/.cargo/config.toml`, `build.rs` | `[profile.release]` LTO/strip, MSVC `GUARD_CF`, unified `asInvoker` + `PerMonitorV2` + Win10/11 GUID manifest. |
| [`0007-IR-007-ci-yml-locked-and-ubuntu2404-ayatana.patch`](../../plans/reports/build/ir-patches/0007-IR-007-ci-yml-locked-and-ubuntu2404-ayatana.patch) | **IR-007** (Med) | `.github/workflows/ci.yml` | Adds `--locked` across all Cargo steps, dual-lockfile cache hash, and `libayatana-appindicator3-dev`. |

---

## 5. Prioritized Production Release Blockers

1. **`[P0 — Remote CI Rollout]` OAuth `workflow` Scope & Default-Branch Registration:** Run interactive `gh auth refresh -h github.com -s workflow`, check private-repo quota, fast-forward `a5feb4a` + reviewed PR merge of `cross-platform-validation.yml` (and `ir-patches/0001..0007`) onto `origin/main` so GitHub exposes `workflow_dispatch`, then execute Phase A (`ubuntu-24.04` + `windows-2022`, `$0.246`) and opt-in Phase B (`macos-15` + `macos-15-intel`, `$1.80`).
2. **`[P0 — Gate G3]` Windows OV/EV Authenticode / Azure Trusted Signing:** Sign inner `dim-desktop.exe`/`dim-cli.exe`, NSIS `uninstall.exe`, outer `.msi`, and `*-setup.exe` with SHA-256 + RFC 3161 timestamping so Windows 11 Smart App Control (`VerifiedAndReputablePolicyState = 1`) allows standalone launch without needing the Signed-Host Bridge.
3. **`[P1 — Windows L3]` Human Two-Account QR Functional Verification:** Run `scripts/run-interactive-l3-checklist.ps1` with two live Zalo accounts while disclosing VNG's `L4` concurrent-call/toast limits.
4. **`[P1 — Gate G4 & macOS Runtime]` Physical Mac Validation + Developer ID Notarization:** Empirically validate `/Applications/Zalo.app` multi-instance behavior (`CFFIXED_USER_HOME`, Keychain, `/tmp/socketzalorecv2021`) on physical Apple Silicon & Intel Macs, then sign with Developer ID Application + `notarytool` + `stapler`.
5. **`[P2 — Gate G5 & Linux Scope]` Ubuntu 24.04 Package Signing & Experimental Scope Lock:** Sign `.deb` / `.AppImage` checksum manifests with release GPG/minisign keys, disclose `Ubuntu 24.04+ (GLIBC >= 2.38/2.39)` & `libfuse2t64`, and keep Linux positioned strictly as **Experimental (Official VNG Zalo Unsupported)**.

---

## 6. Related Packaging & Governance Documents

- [`ci-execution-and-artifact-validation.md`](./ci-execution-and-artifact-validation.md) | [`build-matrix.md`](./build-matrix.md) | [`signing-and-release-gates.md`](./signing-and-release-gates.md)
- [`windows.md`](./windows.md) | [`macos.md`](./macos.md) | [`ubuntu.md`](./ubuntu.md)
