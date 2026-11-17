# CI Execution, Native Runner Architecture & Artifact Validation Report

**Document Owner:** Release Governance & Build Matrix Architecture  
**Worktree Branch:** `build/cross-platform-distribution` (Base `main` @ `a5feb4a520798064c1508c26fe4c4ac17a3a03cf`; E1 baseline commit `ac0df10c795208d983a2fb8d03f95ff627adf59c`)  
**Workflow Under Audit:** [`.github/workflows/cross-platform-validation.yml`](../../../.github/workflows/cross-platform-validation.yml)  
**Assessment Date:** `2026-11-18`  
**Remote GitHub Actions Status:** **`BLOCKED (REMOTE_WORKFLOW_DISPATCH_UNREGISTERED_AND_MISSING_WORKFLOW_SCOPE)`** (`0` remote runs; `CI_LIVE_RUN_STATUS: NOT_EXECUTED (STATICALLY_VALIDATED_ONLY)`)  
**Local Windows Native Validation Status:** **`PASS` (`29/29` `--locked` unit tests, Clippy `0` warnings, `cargo-deny 0.21.0` `0` RustSec advisories, native `fixture-app.mjs` + `dim-cli.exe` smoke PASS)**

---

## 1. Final Native Runner Labels & Architecture Verification Gates

All OS jobs in [`.github/workflows/cross-platform-validation.yml`](../../../.github/workflows/cross-platform-validation.yml) use verified standard GitHub-hosted runner labels (`docs.github.com/en/actions/using-github-hosted-runners`) with **zero cross-compilation masquerading as native runtime verification**:

| Priority | Job ID | Runner Label | Hardware Spec | Mandatory Host & Compiler Gate | Plan / Pool Failure Rule |
| :---: | :--- | :--- | :--- | :--- | :--- |
| **1** | `ubuntu-2404-build` | `ubuntu-24.04` | 2-core x86_64, 7 GB RAM, 14 GB SSD | `uname -m == "x86_64"`, `/etc/os-release` `VERSION_ID="24.04"`, `rustc -vV` `host: x86_64-unknown-linux-gnu` | Fail closed (`exit 1`) on arch/distro mismatch |
| **2** | `windows-msvc-build` | `windows-2022` | 4-core x86_64, 16 GB RAM, 14 GB SSD (VS 2022 MSVC, WiX 3.14, NSIS 3.x) | `PROCESSOR_ARCHITECTURE == "AMD64"`, `rustc -vV` `host: x86_64-pc-windows-msvc`, `vswhere` VC++ `cl.exe`/`link.exe` | Fail closed (`throw`) if MSVC or x64 host absent |
| **3** | `macos-arm64-build` | `macos-15` | 3-core Apple M1 (`arm64`), 7 GB RAM, 14 GB SSD | `uname -m == "arm64"`, `sw_vers`, `rustc -vV` `host: aarch64-apple-darwin`, `lipo -archs == "arm64"` | Opt-in `false` (`$0.062/min`, `10x`); fail closed if not `arm64` |
| **4** | `macos-intel-build` | `macos-15-intel` | 4-core Intel (`x86_64`), 14 GB RAM, 14 GB SSD *(replaces retired `macos-13`; Intel runners sunset Fall 2027)* | `uname -m == "x86_64"`, `sw_vers`, `rustc -vV` `host: x86_64-apple-darwin`, `lipo -archs == "x86_64"` | Opt-in `false` (`$0.062/min`, `10x`); if unavailable on repo plan/pool, report **`BLOCKED`** — **never** cross-compile on `arm64` |

**P0 Remediation Summary:** Earlier cross-compilation of `x86_64-apple-darwin` on `macos-14` (`arm64`) was completely removed. `build-and-package.mjs` also inspects `uname -m` vs `rustc -vV` and sets `native_runtime_test_possible: false` if host and target ever diverge.

---

## 2. Workflow Validation & Native Execution Order

Static validation via [`scripts/ci-cross-platform/validate-workflow.mjs`](../../scripts/ci-cross-platform/validate-workflow.mjs) is intentionally lean (**`6/6 PASS`**, `exit 0`, zero assertion padding) and paired with [`selftest.mjs`](../../scripts/ci-cross-platform/selftest.mjs) (**`16/16 PASS`**, `exit 0`), enforcing `workflow_dispatch`-only triggers, `permissions: contents: read`, explicit native runner labels, pinned `@tauri-apps/cli@2.12.5`, job `timeout-minutes` (`10–30m`), and CLI flag parity.

### Priority Execution Order & 7 Mandatory Native Stages per OS Job
`workflow_dispatch` inputs and `artifact-verification` evaluate targets in strict engineering priority order:
1. **Ubuntu 24.04 LTS x64** (`enable_ubuntu: true`, `timeout-minutes: 25`)
2. **Windows Server 2022 x64 MSVC** (`enable_windows: true`, `timeout-minutes: 30`)
3. **macOS 15 Apple Silicon `arm64`** (`enable_macos_arm64: false` opt-in, `timeout-minutes: 25`)
4. **macOS 15 Intel `x86_64`** (`enable_macos_intel: false` opt-in, `timeout-minutes: 30`)

Every enabled OS job executes all **7 native stages** sequentially:
1. **Compile (`--locked`):** `cargo build --workspace --release --locked` (`dim-cli`, `dim-test-fixture`) + `apps/desktop/src-tauri` (`dim-desktop`).
2. **Test (`--locked`):** Native workspace unit/integration tests + `src-tauri` test binary on target hardware.
3. **Clippy (`--locked -- -D warnings`):** Workspace `--all-targets` + `apps/desktop/src-tauri/Cargo.toml`.
4. **Native Runtime Fixture:** Isolated execution of `fixtures/test-desktop-app/fixture-app.mjs --stay-ms=100` + native `dim-cli status --json` smoke check (plus `xvfb-run -a` GUI dynamic-linker/WebKitGTK startup check on `ubuntu-24.04`).
5. **Tauri Build:** Pinned `@tauri-apps/cli@2.12.5` (`TAURI_CLI_VERSION: "2.12.5"`, never floating `@latest`).
6. **Bundle:** OS-native packagers invoked with `--config scripts/ci-cross-platform/tauri-ci-bundle-overlay.json -- --locked` (`.deb`/`.AppImage`, `.msi`/NSIS `-setup.exe`, `.app`/`.dmg`).
7. **Artifact Verification:** `inspect-artifact.mjs` + `generate-artifact-manifest.mjs`, consolidated by `artifact-verification` (`if: always()`).

---

## 3. GitHub Workflow URL & `workflow_dispatch` Default-Branch Requirement

- **Canonical Workflow URL (post-registration on `origin/main`):** `https://github.com/manhquydev/zalo/actions/workflows/cross-platform-validation.yml`
- **Why Pushing Only to `build/cross-platform-distribution` Cannot Enable `workflow_dispatch`:** Per GitHub Actions platform rules (`Events that trigger workflows -> workflow_dispatch`), GitHub only indexes and exposes `workflow_dispatch` triggers for workflow files committed to the repository's **default branch (`main`)**. Even when targeting `--ref build/cross-platform-distribution`, GitHub rejects manual dispatch with `HTTP 404 / workflow not found` until `.github/workflows/cross-platform-validation.yml` exists on `origin/main`.
- **Safe 6-Step Default-Branch Rollout Plan (Zero Force-Push, Zero Plaintext PAT Scripts):**
  1. **Interactive OAuth Scope Refresh (Human Terminal):**
     ```powershell
     & "C:\Program Files\GitHub CLI\gh.exe" auth refresh -h github.com -s workflow,user
     ```
  2. **Pre-Flight Private-Repo Quota Check:** `gh api users/manhquydev/settings/billing/usage`
  3. **Fast-Forward Sync `main` (`a5feb4a`) to `origin/main`:** `git -C C:/Users/manhquy/Downloads/zalo push origin main`
  4. **Push Feature Branch & Merge Reviewed PR to `main`:** Push `build/cross-platform-distribution` (optionally after `git am` of approved `ir-patches/0001..0007`) and merge cleanly into `main`.
  5. **Verify Workflow Registration:** `gh workflow list --repo manhquydev/zalo`
  6. **Phased `workflow_dispatch` Run:** Dispatch Phase A (`enable_ubuntu=true`, `enable_windows=true`, macOS `false`, ~$0.246) before opting into Phase B (`enable_macos_arm64=true`, `enable_macos_intel=true`, ~$1.80).

---

## 4. Run IDs & Verified Concrete GitHub Blocker Evidence

- **Executed Remote GitHub Actions Run IDs:** **`None` (`0` remote runs)**.
- **Read-Only Empirical Evidence ([`ci-baseline-and-github-audit.md`](../../plans/reports/build/ci-baseline-and-github-audit.md)):**
  1. `git ls-remote origin` -> `origin/main` is at `8cf47035f0f06bd97cc99aff80d81fb10b103af8` (initial commit where `ci.yml` lived under nested `desktop-instance-manager/.github/workflows/`, ignored by GitHub). Local `main` (`a5feb4a`) is `ahead 1` unpushed.
  2. `gh api repos/manhquydev/zalo/actions/workflows` -> `{"total_count": 0, "workflows": []}`; `gh run list --repo manhquydev/zalo` -> `0` runs.
  3. `git config --show-origin --get-regexp credential` -> `C:/Users/manhquy/.gitconfig` binds `credential.https://github.com.helper` to `!'C:\Program Files\GitHub CLI\gh.exe' auth git-credential`.
  4. `gh api -i user` -> `X-Oauth-Scopes: gist, read:org, repo` (**missing `'workflow'`** -> GitHub rejects pushing `.github/workflows/*.yml`; **missing `'user'`** -> `/users/manhquydev/settings/billing/usage` returns `HTTP 404`).

---

## 5. Per-Job Execution & Validation Status Table

| Job / Environment | Runner / Host | Execution Status | Root Cause / Verified Evidence |
| :--- | :--- | :--- | :--- |
| `rust-workspace-validation` (Remote CI) | `ubuntu-24.04` | **`BLOCKED` (`NOT_EXECUTED`)** | Workflow unregistered on `origin/main` + missing `workflow` OAuth scope; YAML `6/6` statically validated |
| `dependency-audit` (Remote CI) | `ubuntu-24.04` | **`BLOCKED` (`NOT_EXECUTED`)** | Same remote blocker; equivalent `cargo-deny 0.21.0` + `dependency-audit.mjs` **executed locally (`PASS`)** |
| `1. ubuntu-2404-build` (Remote CI) | `ubuntu-24.04` | **`BLOCKED` (`NOT_EXECUTED`)** | Same remote blocker; `deb.depends` + `xvfb-run` + `APPIMAGE_EXTRACT_AND_RUN=1` configured via CI overlay |
| `2. windows-msvc-build` (Remote CI) | `windows-2022` | **`BLOCKED` (`NOT_EXECUTED`)** | Same remote blocker; local host lacks VS C++ MSVC Build Tools (`E0463`) & WiX/NSIS |
| `3. macos-arm64-build` (Remote CI) | `macos-15` | **`BLOCKED` (`NOT_EXECUTED`)** | Same remote blocker + opt-in `false` (`10x` cost guard); valid `icon.icns` overlay ready |
| `4. macos-intel-build` (Remote CI) | `macos-15-intel` | **`BLOCKED` (`NOT_EXECUTED`)** | Same remote blocker + opt-in `false` (`10x` cost guard; fails closed as `BLOCKED` if Intel pool unavailable) |
| `artifact-verification` (Remote CI) | `ubuntu-24.04` | **`BLOCKED` (`NOT_EXECUTED`)** | Same remote blocker; merge & Section 7 validator tested in `selftest.mjs` (`16/16 PASS`) |
| **Local Windows Native Build & Suite** | Windows 11 Pro `10.0.26300` x64 (`x86_64-pc-windows-gnu`) | **`PASS` (Compiled Binary + Fixture + Advisory)** | `rustc 1.100.0`, `--locked` fmt/Clippy (`0` warnings), `29/29` unit tests, `cargo-deny 0.21.0` (`0` CVEs), `fixture-app.mjs` + `dim-cli.exe` PASS; local MSI/NSIS `SKIPPED_MISSING_TOOLING` |

---

## 6. Test, Runtime Fixture & Real `cargo-deny` RustSec Advisory Logs Summary

- **Rust Unit & Static Suite (`--locked` on Windows 11 x64):**
  - `cargo fmt --all -- --check` (`0.33s`) & `src-tauri` fmt (`0.19s`): **PASS** (`0` diffs).
  - `cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu --locked -- -D warnings` (`14.26s`) & `src-tauri` Clippy (`93.12s`): **PASS** (`0` warnings).
  - `cargo test --workspace --all-targets --target x86_64-pc-windows-gnu --locked` (`16.80s`): **`29/29 PASS`** (`dim-core` 9, `dim-platform-linux` 3, `dim-platform-macos` 3, `dim-platform-windows` 6, `dim-process-manager` 2, `dim-profile-store` 4, `dim-test-fixture` 2) + `src-tauri` test binary (`138.84s`, `0` failures).
- **Native Runtime Fixture Smoke (`PASSED_NATIVE_FIXTURE_AND_DIM_CLI`):**
  - `node fixtures/test-desktop-app/fixture-app.mjs --stay-ms=100` (`0.20s`, isolated `%TEMP%` `APPDATA`) -> `{"event":"ready","launch_count":1,"zalo_user_id":"100001"}`.
  - `target/x86_64-pc-windows-gnu/release/dim-cli.exe --cli --json --data-dir <temp>/dim-data status` (`0.07s`) -> `{"ok":true,"platform":"windows","primary_zalo":{"zalo_user_id":"100001",...}}`.
- **Real `cargo-deny` RustSec Advisory Scan ([`dependency-audit-report.json`](../../plans/reports/build/dependency-audit-report.json)):**
  - Scanner: **`cargo-deny 0.21.0`** (`~/.cargo/bin/cargo-deny.exe`) querying live **`rustsec/advisory-db` @ `233155b4025b854c68a4b199c0e32d7fd53291a2`** (`2026-11-17T11:14:35+01:00`).
  - Scope: `desktop-instance-manager/Cargo.lock` (`41` pkgs) + `apps/desktop/src-tauri/Cargo.lock` (`434` pkgs) = **`439` unique crates**.
  - Result: **`advisories ok` (`0` vulnerabilities, `0` yanked crates, `0` advisory warnings)**; **`0` banned crates** (`openssl`/`openssl-sys`); **`0` SPDX violations**. *(Note: `cargo-deny 0.21.0` flagged deprecated `version = 2` in `deny.toml` and unversioned internal path crates lacking `publish = false`, handled cleanly via sanitized temp config + `-W wildcard` in CI and permanently resolved in `IR-004`.)*

---

## 7. Artifact Hashes & Classification (`artifact-validation-manifest.json` v2)

Generated from commit `ac0df10c795208d983a2fb8d03f95ff627adf59c` (`rustc 1.100.0`, `tauri 2.12.6`, [`SHA256SUMS.txt`](../../plans/reports/build/SHA256SUMS.txt)):

| Artifact | Workspace-Relative Path | `artifact_kind` | `is_installer` | Size (Bytes) | Verified SHA-256 |
| :--- | :--- | :--- | :---: | ---: | :--- |
| **`dim-desktop.exe`** | `apps/desktop/src-tauri/target/x86_64-pc-windows-gnu/release/dim-desktop.exe` | `compiled-executable` | **`false`** *(NOT an installer)* | `21,586,008` | `157117cbce3a4309cedb9f279fbce869580f75d8120b33550d445a579bf53f4c` |
| **`dim-cli.exe`** | `target/x86_64-pc-windows-gnu/release/dim-cli.exe` | `compiled-executable` | **`false`** *(NOT an installer)* | `2,209,966` | `24132f61429179962f216e200cea04cc52a36a7f675f6408c460764c67c07c98` |
| **`dim-test-fixture.exe`** | `target/x86_64-pc-windows-gnu/release/dim-test-fixture.exe` | `compiled-executable` | **`false`** *(NOT an installer)* | `1,616,671` | `4ea0f248b17d087a6a630d9ac58c2f1e862ea9d86784a18164075289ff1bba7c` |
| *`WebView2Loader.dll` (sidecar)* | `apps/desktop/src-tauri/target/x86_64-pc-windows-gnu/release/WebView2Loader.dll` | `shared-library-sidecar` | **`false`** | `160,880` | `86545b66cdb0603bc26b626fb9ad610cb6e71f28d468f5ea66df23b03dda96d5` *(Microsoft-signed)* |

---

## 8. Signing State & 7-Pillar Supply-Chain Separation

Inventory, checksums, advisories, provenance, signatures, and installer packaging are strictly separated so one check never masquerades for another:

| # | Pillar | Verified Status | Exact Meaning & Non-Equivalence Boundary |
| :---: | :--- | :--- | :--- |
| **1** | **Lockfile Checksums** | `VERIFIED` | `431/431` `crates.io-index` SHA-256 digests verified with `--locked`. Does **not** prove CVE-free status or build provenance. |
| **2** | **License & Ban Policy** | `VERIFIED` | `439` crates pass SPDX allowlist; `0` banned `openssl`/`openssl-sys`. Orthogonal to security advisories and signing. |
| **3** | **Vulnerability Advisory Scan** | `VERIFIED_CARGO_DENY_RUSTSEC_CLEAN` | Real `cargo-deny 0.21.0` scan against `rustsec/advisory-db` (`233155b4...`): `0` CVEs, `0` yanked crates. (If unqueried, scripts force `UNVERIFIED_ADVISORY_DB_NOT_SCANNED`.) |
| **4** | **SBOM** | `DECLARED_DEPENDENCY_INVENTORY_ONLY` | Enumerates `439` declared components (`is_independent_build_provenance: false`). **An SBOM is an inventory — NEVER independent build provenance!** |
| **5** | **Build Provenance** | `SELF_REPORTED_UNATTESTED` | Local script-generated Section 7 manifest (`oidc_slsa_attestation_present: false`), distinct from CI OIDC SLSA v1.0 cryptographic builder attestation. |
| **6** | **Artifact Signature** | `UNSIGNED_DEVELOPMENT_ARTIFACT` | PE Authenticode `RVA=0/Size=0` (`NotSigned`), macOS `Developer ID` absent, `NOT_NOTARIZED`. **Never** a signed or production release. |
| **7** | **Installer Integrity** | `RAW_EXECUTABLES_ONLY_LOCALLY_CI_OVERLAY_READY` | Raw `.exe`/ELF files have `is_installer: false`; CI bundler overlay (`tauri-ci-bundle-overlay.json`) ready for native runner packaging; public release requires signed outer + inner installers. |

---

## 9. Per-OS Packaging Validation & Remaining Blockers

- **Windows (`x86_64`):** Local `x86_64-pc-windows-gnu` binaries, `asInvoker` PE manifest, `WebView2Loader.dll` import, and native runtime fixture verified (`PASS`). Blockers: (a) live `windows-2022` `x86_64-pc-windows-msvc` `.msi`/NSIS CI run (`BLOCKED` on OAuth/default-branch registration); (b) **Gate G3** OV/EV Authenticode / Azure Trusted Signing + RFC 3161 timestamping for Windows 11 Smart App Control (`SAC=1` hosts must use ADR-002 Signed-Host Bridge until signed — never disable SAC); (c) human two-phone QR `L3` checklist; (d) VNG `L4` global call-pipe/AUMID limits.
- **macOS (`arm64` & Intel `x86_64`):** Missing `icon.icns` (`1,142` B) and `128x128@2x.png` (`716` B) mitigated for CI via `tauri-ci-bundle-overlay.json` and patched upstream in `0002-IR-002`. Blockers: (a) live `macos-15` and `macos-15-intel` CI execution (`BLOCKED` on OAuth/default-branch + `10x` opt-in budget); (b) **Gate G4** Apple Developer ID Application signing + Hardened Runtime + `notarytool` notarization + `stapler`; (c) physical Mac `Zalo.app` runtime verification (`NOT_TESTED` — manager build != Zalo macOS multi-instance proof).
- **Ubuntu 24.04 LTS (`x86_64`):** `deb.depends` (`libwebkit2gtk-4.1-0`, `libgtk-3-0t64 | libgtk-3-0`, `libayatana-appindicator3-1 | libappindicator3-1`) supplied via `tauri-ci-bundle-overlay.json` + `xvfb-run -a` probe in CI. Blockers: (a) live `ubuntu-24.04` `.deb`/`.AppImage` CI run (`BLOCKED` on OAuth/default-branch); (b) **Gate G5** detached GPG/minisign signing & `GLIBC >= 2.38/2.39` floor disclosure (incompatible with Ubuntu 22.04 `glibc 2.35`); (c) **Official VNG Zalo Linux is `UNSUPPORTED`** (no VNG Linux build exists; zero Zalo binaries bundled).

---

## 10. Grounded CI Cost & Quota Calculation (`manhquydev/zalo` Private Repo)

Verified from official GitHub Actions billing docs (`docs.github.com/.../about-billing-for-github-actions`) for `private: true` (`owner_type: User`, jobs rounded up to whole minutes; Free includes `2,000` Linux-eq min / `$12.00` credit, Pro includes `3,000` min / `$18.00` credit):

| Job ID | Runner | Verified Rate (USD/min) | Classic Multiplier | Est. Rounded Min | Est. USD / Run | Multiplier Min | Default Enabled? |
| :--- | :--- | :---: | :---: | :---: | :---: | :---: | :---: |
| `rust-workspace-validation` | `ubuntu-24.04` | `$0.006` | `1x` | `6m` | `$0.036` | `6m` | Always |
| `dependency-audit` | `ubuntu-24.04` | `$0.006` | `1x` | `3m` | `$0.018` | `3m` | Always |
| `1. ubuntu-2404-build` | `ubuntu-24.04` | `$0.006` | `1x` | `10m` | `$0.060` | `10m` | **`true`** |
| `2. windows-msvc-build` | `windows-2022` | `$0.010` | `2x` (`~1.67x` $) | `12m` | `$0.120` | `24m` | **`true`** |
| `3. macos-arm64-build` | `macos-15` | `$0.062` | `10x` (`~10.33x` $) | `14m` | `$0.868` | `140m` | **`false`** *(opt-in)* |
| `4. macos-intel-build` | `macos-15-intel` | `$0.062` | `10x` (`~10.33x` $) | `15m` | `$0.930` | `150m` | **`false`** *(opt-in)* |
| `artifact-verification` | `ubuntu-24.04` | `$0.006` | `1x` | `2m` | `$0.012` | `2m` | Always |
| **Default Dispatch (Ubuntu + Windows)** | — | — | — | **`33m`** | **`$0.246`** | **`45m`** (`2.25%` of 2,000) | Routine validation |
| **Full 4-OS Dispatch (+ 2x macOS)** | — | — | — | **`62m`** | **`$2.044`** | **`335m`** (`16.75%` of 2,000) | **88.0%** saved when off |

---

## 11. Configurations Requiring Upstream Integration (`ir-patches/0001..0007`)

Shared manifests remain untouched on `build/cross-platform-distribution` while 5 sibling worktrees (`feat/ui-vi-en-quiet-workspace`, `feat/macos-multi-instance`, `feat/ubuntu-multi-instance`, `test/cross-platform-reliability`, `research/desktop-compatibility`) are active (`0` direct file collisions verified in [`integration-requests.md`](../../plans/reports/build/integration-requests.md)). All **7 standalone atomic patches** pass `git -C C:/Users/manhquy/Downloads/zalo apply --check` both individually and sequentially (`exit 0`):

| Patch File (`plans/reports/build/ir-patches/`) | IR | Target File(s) | Single-Concern Scope | `git apply --check` |
| :--- | :---: | :--- | :--- | :---: |
| [`0001-IR-001-align-workspace-msrv-1.90.patch`](../../plans/reports/build/ir-patches/0001-IR-001-align-workspace-msrv-1.90.patch) | `IR-001` | `Cargo.toml`, `adr-002-...md` | Align workspace `rust-version = "1.90"` & ADR-002 text | **PASS** |
| [`0002-IR-002-macos-icns-and-retina-icons.patch`](../../plans/reports/build/ir-patches/0002-IR-002-macos-icns-and-retina-icons.patch) | `IR-002` | `icons/icon.icns`, `icons/128x128@2x.png`, `tauri.conf.json` | Add valid macOS ICNS (`1,142` B) & `@2x` PNG (`716` B) icons | **PASS** |
| [`0003-IR-003-tauri-os-bundle-metadata.patch`](../../plans/reports/build/ir-patches/0003-IR-003-tauri-os-bundle-metadata.patch) | `IR-003` | `apps/desktop/src-tauri/tauri.conf.json` | Explicit Windows WebView2/NSIS, macOS `11.0`, Ubuntu 24.04 `deb.depends` | **PASS** |
| [`0004-IR-004-pin-tauri-cli-and-lockfile-reproducibility.patch`](../../plans/reports/build/ir-patches/0004-IR-004-pin-tauri-cli-and-lockfile-reproducibility.patch) | `IR-004` | `src-tauri/Cargo.toml`, `deny.toml`, `rust-toolchain.toml` | Pin `@tauri-apps/cli@2.12.5`, `publish = false`, fix `deny.toml` for `cargo-deny 0.21` | **PASS** |
| [`0005-IR-005-src-tauri-workspace-lints-parity.patch`](../../plans/reports/build/ir-patches/0005-IR-005-src-tauri-workspace-lints-parity.patch) | `IR-005` | `src-tauri/Cargo.toml`, `src-tauri/src/main.rs` | Enforce `unsafe_op_in_unsafe_fn = "deny"` & remove startup `.expect()` | **PASS** |
| [`0006-IR-006-release-profile-and-windows-manifest.patch`](../../plans/reports/build/ir-patches/0006-IR-006-release-profile-and-windows-manifest.patch) | `IR-006` | `Cargo.toml`, `src-tauri/.cargo/config.toml`, `build.rs` | `[profile.release]` LTO/strip, MSVC `GUARD_CF`, unified `PerMonitorV2` Win10/11 manifest | **PASS** |
| [`0007-IR-007-ci-yml-locked-and-ubuntu2404-ayatana.patch`](../../plans/reports/build/ir-patches/0007-IR-007-ci-yml-locked-and-ubuntu2404-ayatana.patch) | `IR-007` | `.github/workflows/ci.yml` | Add `--locked`, dual-lockfile cache hash, and `libayatana-appindicator3-dev` | **PASS** |

---

## 12. Related Packaging & Governance Documents

- [`build-matrix.md`](./build-matrix.md) | [`signing-and-release-gates.md`](./signing-and-release-gates.md) | [`release-readiness.md`](./release-readiness.md)
- [`windows.md`](./windows.md) | [`macos.md`](./macos.md) | [`ubuntu.md`](./ubuntu.md)
