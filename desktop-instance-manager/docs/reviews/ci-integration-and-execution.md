# CI Integration & Execution Report

> Integration branch: `integration/reliability-hardening`
> Report generated: `2026-11-18`
> Verified Code & Manifest Commit: `20d1729c02011762964914d3696b63656e88120d` (followed only by this documentation commit)
> Integrated Binary Build Commit (`build_commit_sha`): `8aa7a6210871666309667a9a5f0a3efa06d0ddd3`

---

## 1. Integration HEAD & Source Branch Provenance

- **Verified Code & Manifest Commit:** `20d1729c02011762964914d3696b63656e88120d` on `integration/reliability-hardening` (followed only by this documentation commit)
- **Compiled Binary Provenance Commit (`build_commit_sha`):** `8aa7a6210871666309667a9a5f0a3efa06d0ddd3`
- **E2 Packaging Source SHA:** `6c4150ecdb106f07aceca96098aaf3ab0d9a1326` (`build/cross-platform-distribution`, merged at `ec46209`)
- **D2 Compatibility Source SHA:** `0e5d44c0148aa7c5f436e7274597453d5c5501c3` (`research/desktop-compatibility`, merged at `155efc3`)
- **C2 Reliability Source SHA:** `5e802e08119efce56d7df157538122e16ed64801` (`test/cross-platform-reliability`, merged at `5fa798c`)
- **Baseline Merge Base:** `a5feb4a520798064c1508c26fe4c4ac17a3a03cf` (local `main`, `ahead 1` unpushed over `origin/main` `8cf47035f0f06bd97cc99aff80d81fb10b103af8`)

---

## 2. Applied Integration Patches (`IR-002`, Targeted `IR-004`, `IR-007`)

| Patch ID | Commit(s) | Exact Files Modified | Technical Verification Evidence |
| :--- | :---: | :--- | :--- |
| **`IR-002`** | `f83059e`, `8aa7a62` | `apps/desktop/src-tauri/icons/{128x128@2x.png, icon.icns}`, `apps/desktop/src-tauri/tauri.conf.json`, `scripts/ci-cross-platform/tauri-ci-bundle-overlay.json`, `scripts/ci-cross-platform/selftest.mjs` | Deep-validated PNG chunks (`IHDR`/`IDAT`/`IEND`), IEEE 802.3 CRC32, `zlib.inflateSync`, and RGBA (`colorType=6`, `bitDepth=8`) scanline byte counts (`262,400` B for `256x256` and `65,664` B for `128x128`). Removed **all** JSON array keys from `tauri-ci-bundle-overlay.json` (`arrayPaths.length === 0` in `selftest.mjs` `16/16 PASS`) to eliminate RFC 7396 JSON Merge Patch array clobbering. Verified real `dim-desktop.exe` release build (`exit 0`). |
| **`IR-004` (Targeted)** | `a7f1413`, `8aa7a62` | `deny.toml`, `apps/desktop/src-tauri/Cargo.toml`, `.github/workflows/cross-platform-validation.yml` | Removed deprecated `version = 2` from [`deny.toml`](../../deny.toml) for `cargo-deny >= 0.21` compatibility; added `publish = false` and `[package.metadata.packaging] tauri-cli = "2.12.5"` to [`apps/desktop/src-tauri/Cargo.toml`](../../apps/desktop/src-tauri/Cargo.toml). Omitted `rust-toolchain.toml` to preserve developer host toolchains. Simplified `dependency-audit` job to use `--config desktop-instance-manager/deny.toml` directly across all 3 lockfiles (`exit 0`). |
| **`IR-007`** | `5616bcb` | `.github/workflows/ci.yml` | Replaced Ubuntu 24.04 obsolete `libappindicator3-dev` with `libayatana-appindicator3-dev`, enforced `--locked` on every `cargo check`/`clippy`/`test`/`build` invocation, and added `apps/desktop/src-tauri/Cargo.lock` to the Cargo cache hash key. |

---

## 3. Rejected & Deferred Integration Patches (`IR-001`, `IR-003`, `IR-005`, `IR-006`)

| Patch ID | Disposition | Grounded Engineering Rationale |
| :--- | :---: | :--- |
| **`IR-001`** (`rust-version = "1.90"`) | **REJECT** | C2's [`tests/cross-platform/Cargo.toml`](../../tests/cross-platform/Cargo.toml) declares `rust-version = "1.85"` and links workspace crates directly. Raising workspace MSRV to `1.90` creates an immediate MSRV conflict with C2's harness, while workspace crates use zero `1.86..1.90`-exclusive features. |
| **`IR-003`** (OS bundle metadata) | **DEFER** | Untested OS bundler fields (`timestampUrl: ""`, `hardenedRuntime: true` with `signingIdentity: null`, WiX/NSIS installer metadata, and `deb.depends` array) have not yet been executed on remote `ubuntu-24.04`, `windows-2022`, and `macos-15` runners. Deferred until Wave 1 & Wave 2 CI runs validate base bundles. |
| **`IR-005`** (`src-tauri` workspace lints) | **REJECT** | Modifies `apps/desktop/src-tauri/src/main.rs` and adds crate-wide lint attributes with high merge-conflict risk against active UI and macOS/Ubuntu adapter worktrees for zero functional or security benefit. |
| **`IR-006`** (Release LTO/`panic = "abort"`/Win32 manifest) | **REJECT** | `[profile.release] panic = "abort"` breaks `catch_unwind`/`#[should_panic]` tests under release profiles; `lto = "thin"` + `codegen-units = 1` increases private-repo CI build minutes; custom `WindowsAttributes::app_manifest` in `build.rs` has not been verified on `windows-2022` MSVC and risks disturbing Windows PE resource linking. |

---

## 4. Workflow Registration Status & Safe Default-Branch Rollout Plan

- **Workflow Registration Status:** **`NOT_REGISTERED_ON_DEFAULT_BRANCH`**
- **Target Workflow:** [`.github/workflows/cross-platform-validation.yml`](../../../.github/workflows/cross-platform-validation.yml) (`validate-workflow.mjs` `6/6 PASS`, zero `|| true`, zero `continue-on-error`, `workflow_dispatch` only, `permissions: contents: read`, macOS jobs `false` by default).
- **Platform Rule:** GitHub Actions only discovers and exposes `workflow_dispatch` triggers for workflow files present on the repository's **default branch (`main`)**. Currently `origin/main` sits at initial commit `8cf4703` (`0` registered workflows in `gh api repos/manhquydev/zalo/actions/workflows`).

### 8-Step Safe Default-Branch PR Registration Plan (Zero `--force`, Zero Plaintext Tokens)

1. **Interactive OAuth Scope Authorization:** Run `& "C:\Program Files\GitHub CLI\gh.exe" auth refresh -h github.com -s workflow` in an interactive user terminal and confirm `workflow` appears in `gh auth status`.
2. **Pre-Flight Compute Budget Check:** Inspect remaining monthly GitHub Free `$12.00 USD/month` Actions credit in the GitHub Web UI (`Settings -> Billing & licensing`).
3. **Secret & PII Diff Gate:** Re-verify `git diff origin/main..integration/reliability-hardening` contains zero secrets, tokens, or local user home paths (`0` matches verified at `20d1729`).
4. **Fast-Forward Baseline `main`:** Push verified local `main` (`a5feb4a`) to `origin/main` (`git -C <REPO_ROOT> push origin main`).
5. **Push Integration Branch & Open Reviewed PR:** Push `integration/reliability-hardening` (`git push -u origin integration/reliability-hardening`) and open a reviewed Pull Request targeting `main`.
6. **Merge PR & Confirm Registration:** Merge into `main` (without `--force`) and verify registration via `gh workflow list --repo manhquydev/zalo`.
7. **Wave 1 Dispatch (Ubuntu 24.04 + Windows 2022 MSVC):** Trigger `gh workflow run cross-platform-validation.yml --repo manhquydev/zalo --ref main -f enable_ubuntu=true -f enable_windows=true -f enable_macos_arm64=false -f enable_macos_intel=false -f bundles=all` (`$0.246` `ESTIMATED_PRE_RUN`).
8. **Wave 2 Dispatch (Opt-In Native macOS `arm64` & Intel `x86_64`):** Only after Wave 1 passes and macOS compute spend (`$1.798` `ESTIMATED_PRE_RUN`) is approved, dispatch `-f enable_ubuntu=false -f enable_windows=false -f enable_macos_arm64=true -f enable_macos_intel=true -f bundles=all`.

---

## 5. Credential Blocker

- **Credential Status:** **`AWAITING_USER_AUTHORIZATION`**
- **Verified Evidence (`gh auth status` & `gh api -i user`, zero token extraction):** Active GitHub keyring token carries OAuth scopes `'gist', 'read:org', 'repo'` — **missing `'workflow'`**, which causes GitHub's pre-receive hook to reject pushing `.github/workflows/*.yml`.
- **Exact Remediation Command (Least-Privilege `workflow` Scope Only):**
  ```powershell
  & "C:\Program Files\GitHub CLI\gh.exe" auth refresh -h github.com -s workflow
  ```

---

## 6. Remote GitHub Actions Run IDs

- **Run IDs:** **`None — blocked pre-push by AWAITING_USER_AUTHORIZATION` and `NOT_REGISTERED_ON_DEFAULT_BRANCH` (`0` remote runs in `gh run list --repo manhquydev/zalo`).**

---

## 7. Native Job Execution Results (Remote Waves vs. Local Clean-Tree Windows Host)

### 7.1 Remote GitHub-Hosted CI Jobs (`cross-platform-validation.yml`)

| Wave | Job ID | Runner Label | Target Triple | Default Input | Remote Status |
| :---: | :--- | :--- | :--- | :---: | :--- |
| **1** | `rust-workspace-validation` | `ubuntu-24.04` | `x86_64-unknown-linux-gnu` | Always | **`NOT_EXECUTED`** |
| **1** | `dependency-audit` | `ubuntu-24.04` | All 5 triples (`deny.toml`) | Always | **`NOT_EXECUTED`** *(executed locally — `PASS`)* |
| **1** | `ubuntu-2404-build` (+ C2 harness) | `ubuntu-24.04` | `x86_64-unknown-linux-gnu` | `true` | **`NOT_EXECUTED`** |
| **1** | `windows-msvc-build` (+ C2 harness) | `windows-2022` | `x86_64-pc-windows-msvc` | `true` | **`NOT_EXECUTED`** |
| **2** | `macos-arm64-build` (+ C2 harness) | `macos-15` | `aarch64-apple-darwin` | `false` *(opt-in)* | **`NOT_EXECUTED`** |
| **2** | `macos-intel-build` (+ C2 harness) | `macos-15-intel` | `x86_64-apple-darwin` | `false` *(opt-in)* | **`NOT_EXECUTED`** |
| **3** | `artifact-verification` | `ubuntu-24.04` | Cross-OS manifest gate | `if: always()` | **`NOT_EXECUTED`** |

### 7.2 Local Clean-Tree Windows 11 Native Execution (`8aa7a62` Archive + WinLibs UCRT64)

| Local Native Verification Suite | Command | Exit Code | Wall Time | Result Summary |
| :--- | :--- | :---: | :---: | :--- |
| Workspace Unit & Integration Tests | `cargo test --workspace --all-targets --locked` | **`0`** | `19.83s` | **29 passed, 0 failed** (`dim-core` 9, `dim-platform-linux` 3, `dim-platform-macos` 3, `dim-platform-windows` 6, `dim-process-manager` 2, `dim-profile-store` 4, `dim-test-fixture` 2) |
| Tauri Desktop Shell Tests (`IR-002`) | `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --locked` | **`0`** | `158.38s` | **2 passed, 0 failed** |
| C2 Cross-Platform Reliability Runner | `node tests/cross-platform/scripts/run-all.mjs --cycles 5 --metrics-runs 2` | **`0`** | `144.03s` | **55 total: 47 passed, 6 known baseline defects reproduced, 5 P0 `FIXED_CANDIDATE`s, 2 non-host skips, 0 unexpected failures** |
| C2 Cargo Integration Harness | `cargo test --manifest-path tests/cross-platform/Cargo.toml --locked` | **`0`** | `62.65s` | **7 passed, 0 failed, 3 ignored** (`suite_01`..`suite_07`) |
| C2 Native Windows OS Gate | `xf-test-runner.exe --case XP-WIN-01,XP-WIN-02,XP-WIN-03,XP-WIN-04 --skip-metrics --require-native-os` | **`0`** | `1.25s` | **4 passed, 0 skipped, 0 failed** |
| Native Release Build & Smoke Probe | `node scripts/ci-cross-platform/build-and-package.mjs --target x86_64-pc-windows-gnu --bundles all` | **`0`** | `334.35s` | `clippy_status: PASSED`, `test_status: PASSED_NATIVE`, `compile_status: SUCCEEDED`, `runtime_fixture_status: PASSED_NATIVE_FIXTURE_AND_DIM_CLI`, `bundle_status: SKIPPED_MISSING_TOOLING` (no local `cargo-tauri`/WiX/NSIS) |

---

## 8. Integrated Build Artifacts & SHA-256 Hashes (`build_commit_sha: 8aa7a62`)

Recorded in [`plans/reports/build/artifact-validation-manifest.json`](../../plans/reports/build/artifact-validation-manifest.json) and [`plans/reports/build/SHA256SUMS.txt`](../../plans/reports/build/SHA256SUMS.txt) (committed in `20d1729`):

| Artifact Relative Path | Kind | Installer? | Target Triple | Size (Bytes) | SHA-256 Digest | Signing & Provenance Status |
| :--- | :--- | :---: | :--- | ---: | :--- | :--- |
| `apps/desktop/src-tauri/target/x86_64-pc-windows-gnu/release/dim-desktop.exe` | `compiled-executable` | **`false`** | `x86_64-pc-windows-gnu` | `22,408,513` | `093c28961bba7ba0d43595bad21783da096a8269266642b389e2699e9491dd29` | `UNSIGNED_DEVELOPMENT_ARTIFACT` / `SELF_REPORTED_UNATTESTED` |
| `target/x86_64-pc-windows-gnu/release/dim-cli.exe` | `compiled-executable` | **`false`** | `x86_64-pc-windows-gnu` | `2,392,277` | `473941b7efd87bc2a9c089d20172b564c056a700af34accd30aae67450d9d8bd` | `UNSIGNED_DEVELOPMENT_ARTIFACT` / `SELF_REPORTED_UNATTESTED` |
| `target/x86_64-pc-windows-gnu/release/dim-test-fixture.exe` | `compiled-executable` | **`false`** | `x86_64-pc-windows-gnu` | `1,684,695` | `a383eda161e4bfe80c6fbb1150b3a0e400ba1bbb024f6bd595384313f45ee82d` | `UNSIGNED_DEVELOPMENT_ARTIFACT` / `SELF_REPORTED_UNATTESTED` |

- **Zero VNG Binary Policy:** `vng_binary_embedded = false` (`zero_vng_binary_policy_passed = true` across all 3 executables).

---

## 9. Supply-Chain & RustSec Advisory DB Audit (`cargo-deny 0.21.0`)

| Attribute | Verified Value |
| :--- | :--- |
| **Advisory Database URL** | `https://github.com/rustsec/advisory-db` (`~/.cargo/advisory-dbs/advisory-db-3157b0e258782691`) |
| **Advisory DB Revision SHA** | `233155b4025b854c68a4b199c0e32d7fd53291a2` |
| **Advisory DB Commit Timestamp** | `2026-11-17 11:14:35 +0100` |
| **Exact Commands Executed** | `cargo deny --config deny.toml --manifest-path Cargo.toml --all-features --locked check -W wildcard advisories bans licenses sources`<br>`cargo deny --config deny.toml --manifest-path apps/desktop/src-tauri/Cargo.toml --all-features --locked check -W wildcard advisories bans licenses sources`<br>`cargo deny --config deny.toml --manifest-path tests/cross-platform/Cargo.toml --all-features --locked check -W wildcard advisories bans licenses sources` |
| **Lockfiles & Target Triples Covered** | 3 lockfiles (`desktop-instance-manager/Cargo.lock`, `apps/desktop/src-tauri/Cargo.lock`, `tests/cross-platform/Cargo.lock`) across all 5 configured triples (`x86_64-pc-windows-msvc`, `x86_64-pc-windows-gnu`, `x86_64-unknown-linux-gnu`, `aarch64-apple-darwin`, `x86_64-apple-darwin`) |
| **Exit Codes** | **`0`**, **`0`**, **`0`** (`advisories ok, bans ok, licenses ok, sources ok`) |
| **Provenance Trust Level** | **`SELF_REPORTED_UNATTESTED`** (SBOM/lockfile inventory != cryptographic SLSA OIDC build provenance) |

> **Mandatory Scope Disclaimer:** Reporting `0 advisories` is strictly a **point-in-time snapshot of known published advisories** in `rustsec/advisory-db` at commit `233155b4` (`2026-11-17`) across Cargo crate dependencies. It is **NOT** a guarantee of zero vulnerabilities, zero undisclosed CVEs, or OS/system-library security (`WebView2`, `WebKitGTK`, `AppKit`).

---

## 10. Private-Repository GitHub Actions Billing Estimate vs. Actual Consumption

Repository `manhquydev/zalo` is **private** (`private: true`, `owner_type: User`). GitHub Free includes **`$12.00 USD/month`** of standard runner compute credit (jobs round up to the nearest whole minute):

| Dispatch Scenario | Per-Minute Rates (`USD/min`) | Est. Rounded Wall-Clock (`ESTIMATED_PRE_RUN`) | Est. Compute Cost (`ESTIMATED_PRE_RUN`) | Share of `$12.00/mo` Credit | Actual Remote Spend To Date |
| :--- | :--- | :---: | :---: | :---: | :---: |
| **Wave 1 Default (`enable_ubuntu=true`, `enable_windows=true`, macOS `false`)** | Ubuntu x64 `$0.006/min` (`21m`), Windows x64 `$0.010/min` (`12m`) | `33 min` | **`$0.246 / run`** | `2.05%` | **`$0.00 / NOT_MEASURED_YET`** (`0` runs) |
| **Wave 2 Opt-In macOS Only (`macos-15` + `macos-15-intel`)** | macOS `$0.062/min` (`29m`), Ubuntu gate `$0.006/min` (`11m`) | `40 min` | **`$1.864 / run`** | `15.53%` | **`$0.00 / NOT_MEASURED_YET`** (`0` runs) |
| **Full 4-OS Combined Dispatch** | Ubuntu `$0.006/min`, Windows `$0.010/min`, macOS `$0.062/min` | `62 min` | **`$2.044 / run`** | `17.03%` | **`$0.00 / NOT_MEASURED_YET`** (`0` runs) |

---

## 11. Remaining Blockers & Production Release Readiness

1. **`[P0 — Credential & Registration]` Remote CI Unregistered (`AWAITING_USER_AUTHORIZATION` + `NOT_REGISTERED_ON_DEFAULT_BRANCH`):** Requires human `gh auth refresh -h github.com -s workflow` and reviewed PR merge to `main` before Wave 1/2/3 remote runners can execute.
2. **`[P0 — Gate G3]` Windows Code Signing:** Local binaries are `UNSIGNED_DEVELOPMENT_ARTIFACT` (`is_installer: false`); production Windows `.msi`/NSIS installers require OV/EV Authenticode / Azure Trusted Signing + RFC 3161 timestamping for Windows 11 Smart App Control (`SAC=1` hosts require ADR-002 Signed-Host Bridge until signed — never disable SAC).
3. **`[P1 — Gate G4 & Physical Mac]` macOS Signing, Notarization & Runtime Proof:** Requires Apple Developer ID Application signing + `notarytool` + `stapler` AND empirical multi-instance testing against `/Applications/Zalo.app` on physical Mac hardware (`NOT_TESTED`).
4. **`[P1 — Product Reliability]` Remaining P1/P2 Baseline Defects:** While 5 P0 cases transitioned to `FIXED_CANDIDATE` at `8aa7a62`, 6 P1/P2 baseline defects remain tracked on `integration/reliability-hardening` before enabling `--mode release-regression` (`--fail-on-product-defects`).
5. **`[P2 — Gate G5 & Linux Scope]` Ubuntu 24.04 Checksum Signing & Scope Disclosure:** Official VNG Zalo Desktop for Linux is **`UNSUPPORTED`**; Linux manager packages remain experimental (`GLIBC >= 2.38/2.39`, zero VNG binaries).

---

## Final Verdict

**`CI_NOT_REGISTERED`**
