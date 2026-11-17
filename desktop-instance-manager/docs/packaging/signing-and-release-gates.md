# Release Governance, 7-Pillar Supply-Chain Policy & Quality Gates (G0–G6)

**Document Owner:** Release Governance & Build Matrix Architecture  
**Applies To:** `desktop-instance-manager` (`Multi-Zalo Desktop`, `com.multizalodesktop.manager`)  
**Enforcement Tooling:** [`scripts/ci-cross-platform/`](../../scripts/ci-cross-platform/) and [`.github/workflows/cross-platform-validation.yml`](../../../.github/workflows/cross-platform-validation.yml)  
**Current Release Posture:** **`DEVELOPMENT_VALIDATION_ONLY`** — zero artifacts in this repository are signed production releases (`signing_status = "UNSIGNED_DEVELOPMENT_ARTIFACT"`, `provenance_trust_level = "SELF_REPORTED_UNATTESTED"`).

---

## 1. Governance Principles & 7-Pillar Supply-Chain Separation

Every build, package, validation run, and future distribution candidate of `desktop-instance-manager` enforces strict conceptual and technical separation across **7 supply-chain pillars** so one control is **never** conflated with another:

| # | Supply-Chain Pillar | Current Verified Posture | Strict Non-Equivalence Rule |
| :---: | :--- | :--- | :--- |
| **1** | **Lockfile Checksum Verification** | `VERIFIED` (`431/431` `crates.io-index` SHA-256 digests with `--locked`) | Proves tarball immutability against `Cargo.lock`; does **not** prove CVE absence or builder identity. |
| **2** | **License & Crate Ban Policy** | `VERIFIED` (`439` crates pass SPDX allowlist; `0` banned `openssl*`) | Proves open-source license & TLS policy compliance; orthogonal to vulnerabilities and signing. |
| **3** | **Known Vulnerability Advisory Scan** | `VERIFIED_CARGO_DENY_RUSTSEC_CLEAN` (`cargo-deny 0.21.0` against live `rustsec/advisory-db` @ `233155b4...`: `0` CVEs, `0` yanked) | **Never** claim `0 vulnerabilities` unless a real advisory DB was queried (fallback must report `UNVERIFIED_ADVISORY_DB_NOT_SCANNED`). |
| **4** | **SBOM (Software Bill of Materials)** | `DECLARED_DEPENDENCY_INVENTORY_ONLY` (`439` unique crates; `is_independent_build_provenance: false`) | **An SBOM is a declared package inventory ONLY — NEVER independent build provenance, NEVER a vulnerability scan, and NEVER a signature.** |
| **5** | **Build Provenance** | `SELF_REPORTED_UNATTESTED` (`oidc_slsa_attestation_present: false`) | Script-generated Section 7 SHA-256 manifests are self-reported unattested records, distinct from OIDC/hardware-attested SLSA v1.0 provenance (`actions/attest-build-provenance`). |
| **6** | **Artifact Signature** | `UNSIGNED_DEVELOPMENT_ARTIFACT` (`Authenticode = NotSigned`, `NOT_NOTARIZED`) | Unsigned binaries/bundles are **never** labeled `"signed"`, `"trusted"`, or `"production"`. |
| **7** | **Installer Integrity** | `RAW_EXECUTABLES_ONLY_LOCALLY_CI_OVERLAY_READY` (`is_installer: false` on raw `.exe`/ELF) | Raw Cargo binaries are **never** installers; public installers require OS bundlers + inner/outer signing. |

### Non-Negotiable Safety Invariants
- **Zero VNG Proprietary Payload:** Never bundle, download, redistribute, or binary-patch `Zalo.exe`, `Zalo.app`, `app.asar`, `ZaloSetup-*.exe`, or community-repackaged `app.asar` archives.
- **Native Runner Integrity:** Never treat cross-compilation (e.g., building `x86_64-apple-darwin` on an `arm64` host) as native runtime verification; if `macos-15-intel` is unavailable on the account plan/pool, report **`BLOCKED`**.
- **Zero OS Security Policy Bypass:** Never disable Windows Smart App Control (SAC), WDAC, Windows UAC (`asInvoker` enforced), macOS Gatekeeper (`spctl`), or Ubuntu 24.04 AppArmor user-namespace protections, and never use signed PowerShell scripts as an execution-policy bypass to launch unsigned binaries.

---

## 2. Release Gate Summary Matrix (G0–G6)

An artifact candidate advances from **Development Validation** to **Public Production Release** on a target OS **only when every applicable gate G0 through G6 passes (`PASS`)**.

| Gate ID | Gate Name | Scope | Automated Enforcement / Evidence | Current Baseline Status |
| :--- | :--- | :--- | :--- | :--- |
| **G0** | **Source, Lockfile & RustSec Advisory Integrity** | `--locked` dual `Cargo.lock`, SPDX licenses, banned crates, real `cargo-deny` RustSec scan, MSRV | `cargo-deny 0.21.0`, `dependency-audit.mjs`, [`dependency-audit-report.json`](../../plans/reports/build/dependency-audit-report.json) | **PASS** (`0` CVEs across `439` crates; `IR-001` & `IR-004` atomic patches ready) |
| **G1** | **Zero VNG Binary Policy** | Repo tree, Cargo/Tauri targets, installer bundles, CI artifacts | `dependency-audit.mjs`, `build-and-package.mjs`, `generate-artifact-manifest.mjs` (`vng_binary_embedded: false`) | **PASS** |
| **G2** | **Classification, Provenance & Runtime Honesty** | Binary != installer, `UNSIGNED_DEVELOPMENT_ARTIFACT`, `SELF_REPORTED_UNATTESTED`, SBOM != provenance, DIM != Zalo runtime | `inspect-artifact.mjs`, `generate-artifact-manifest.mjs`, [`artifact-validation-manifest.json`](../../plans/reports/build/artifact-validation-manifest.json) | **PASS** |
| **G3** | **Windows Code Signing & SAC Compliance** | OV/EV Authenticode or Azure Trusted Signing, RFC 3161 timestamp, inner+outer MSI/NSIS signing, SAC preservation | `inspect-artifact.mjs` (`IMAGE_DIRECTORY_ENTRY_SECURITY`), `signtool verify /pa` | **BLOCKED for Public Release** (`UNSIGNED_DEVELOPMENT_ARTIFACT`) |
| **G4** | **macOS Signing, Hardened Runtime & Notarization** | Developer ID Application, Hardened Runtime, `notarytool`, `stapler`, Gatekeeper `spctl`, physical Mac test | `inspect-artifact.mjs` (`codesign -dv`), `xcrun notarytool`, `spctl --assess` | **BLOCKED for Public Release** (`UNSIGNED_DEVELOPMENT_ARTIFACT` / `NOT_NOTARIZED` + runtime `NOT_TESTED`) |
| **G5** | **Ubuntu Package Integrity & ABI Disclosure** | `.deb` metadata & GPG signing, AppImage detached signature, `GLIBC >= 2.38/2.39` floor & AppArmor disclosure | `inspect-artifact.mjs` (`dpkg-deb -I`, `ldd`, `GLIBC_*` scan, mode `0755`), `xvfb-run` | **BLOCKED for Public Release** (remote CI `BLOCKED` / `NOT_EXECUTED`; Official Zalo `UNSUPPORTED`) |
| **G6** | **Human Release Approval & Read-Only CI** | `workflow_dispatch` manual control, `permissions: contents: read`, default-branch rollout, zero auto-release | `validate-workflow.mjs` (`6/6 PASS`), [`.github/workflows/cross-platform-validation.yml`](../../../.github/workflows/cross-platform-validation.yml) | **PASS (Guardrail Active)** |

---

## 3. Detailed Gate Specifications

### Gate G0 — Source, Lockfile & Real RustSec Vulnerability Advisory Gate

1. **Dual Lockfile Immutability (`--locked`):**
   - Both [`desktop-instance-manager/Cargo.lock`](../../Cargo.lock) (`41` packages, SHA-256 `deeb58a0...71040d2`) and [`apps/desktop/src-tauri/Cargo.lock`](../../apps/desktop/src-tauri/Cargo.lock) (`434` packages, SHA-256 `0bcb1b65...5213e1fc`) are verified with `--locked` across every `cargo check`, `cargo clippy`, `cargo test`, `cargo build`, and `tauri build -- --locked` command (`439` unique crates; `431/431` `crates.io-index` SHA-256 checksums verified).
2. **Real `cargo-deny` RustSec Advisory Verification:**
   - Local and CI (`EmbarkStudios/cargo-deny-action@v2`) pipelines execute real `cargo-deny` (`0.21.0`) against `https://github.com/rustsec/advisory-db` (verified at commit `233155b4025b854c68a4b199c0e32d7fd53291a2`, `0` vulnerabilities, `0` yanked crates across both lockfiles).
   - If `cargo-deny` and `rustsec/advisory-db` are unavailable, `dependency-audit.mjs` **must** emit `UNVERIFIED_ADVISORY_DB_NOT_SCANNED` (`vulnerabilities_found: null`) and never claim `0` vulnerabilities from checksums alone.
   - *Upstream `deny.toml` Note (`IR-004`):* `cargo-deny >= 0.20` rejects legacy top-level `version = 2` and flags `wildcards = "deny"` on unversioned internal path crates lacking `publish = false`; CI and `dependency-audit.mjs` sanitize a temporary config copy while [`0004-IR-004-pin-tauri-cli-and-lockfile-reproducibility.patch`](../../plans/reports/build/ir-patches/0004-IR-004-pin-tauri-cli-and-lockfile-reproducibility.patch) fixes both cleanly upstream.
3. **SPDX Licenses, Banned Crates & Pinned Tooling:**
   - Allowed SPDX tokens: `MIT`, `Apache-2.0`, `Apache-2.0 WITH LLVM-exception`, `BSD-2-Clause`, `BSD-3-Clause`, `ISC`, `Unicode-3.0`, `Zlib`, `MPL-2.0`.
   - `openssl` and `openssl-sys` are hard-denied in favor of `rustls`.
   - Effective packaging MSRV is **`1.90`** (`IR-001` aligns root `Cargo.toml` from `1.85` to `1.90`), and Tauri bundler CLI is pinned to exact **`@tauri-apps/cli@2.12.5`** (never floating `@latest`).

---

### Gate G1 — Zero VNG Binary Policy Gate

- **Absolute Prohibition:** No repository path, CI cache, or bundle (`.msi`, `-setup.exe`, `.app`, `.dmg`, `.deb`, `.AppImage`) may contain `Zalo.exe`, `Zalo.app`, `app.asar`, `ZaloCall.exe`, `ZaloCap.exe`, `Update.exe`, or `ZaloSetup-*.exe`.
- **Automated Enforcement:** `dependency-audit.mjs`, `build-and-package.mjs`, `generate-artifact-manifest.mjs`, and CI `artifact-verification` scan all outputs and enforce `"vng_binary_embedded": false`. End users install official Zalo from VNG separately; on Windows, `dim-platform-windows` verifies VNG's Authenticode signature (`WinVerifyTrust` + `CN/O=VNG GROUP JSC` RDN pinning) at launch time.

---

### Gate G2 — Classification, Provenance & Runtime Honesty Gate

1. **Compiled Executable vs. Installer (`is_installer`):** Raw binaries (`dim-desktop.exe`, `dim-cli.exe`, `dim-test-fixture.exe`, `dim-desktop`) are strictly `artifact_kind: "compiled-executable"` with `is_installer: false`. Calling a raw binary an installer fails Gate G2.
2. **SBOM vs. Build Provenance (`provenance_trust_level`):** SBOM files (`DECLARED_DEPENDENCY_INVENTORY_ONLY`) list declared dependencies and are **never** treated as build provenance. Script-generated Section 7 manifests carry `provenance_trust_level: "SELF_REPORTED_UNATTESTED"` until paired with GitHub OIDC SLSA v1.0 cryptographic provenance (`actions/attest-build-provenance`) in an approved release workflow.
3. **Unsigned Posture (`signing_status`):** Unverified builds strictly record `"UNSIGNED_DEVELOPMENT_ARTIFACT"`.
4. **Manager Build != Zalo Runtime Compatibility:**
   - **Windows 10/11 x64:** L1/L2 official Zalo `26.10.20` isolation verified locally; L3 QR multi-account chat `NOT_TESTED`; L4 concurrent calls/toasts `BLOCKED` by VNG global pipes/AUMID.
   - **macOS (`macos-15` `arm64` & `macos-15-intel` `x86_64`):** Compiling/bundling `dim-desktop` proves manager build integrity only — Zalo macOS multi-instance isolation (`open -n --env CFFIXED_USER_HOME`) remains **`NOT_TESTED`** on physical Apple hardware.
   - **Ubuntu 24.04 LTS (`ubuntu-24.04` `x86_64`):** Official VNG Zalo Desktop on Linux is **`UNSUPPORTED`** (no VNG Linux build exists); fixture and user-supplied local AppImage isolation are **`EXPERIMENTAL` (`NOT_TESTED` on Linux host)**.

---

### Gate G3 — Windows Code Signing & Smart App Control (SAC) Gate

1. **Certificate & Key Custody:** Require an **OV/EV Authenticode Certificate** (Microsoft Trusted Root Program) or **Azure Trusted Signing** backed by FIPS 140-2 Level 2+ HSM / Cloud HSM custody (zero repo-stored `.pfx` files).
2. **SHA-256 + RFC 3161 Timestamping & Multi-Layer Signing Order:**
   - Sign in strict inside-out order using `signtool.exe sign /fd SHA256 /tr "http://timestamp.acs.microsoft.com" /td SHA256 /v`:
     1. Inner `dim-desktop.exe` and `dim-cli.exe` (confirming vendor signature on `WebView2Loader.dll`).
     2. NSIS `uninstall.exe` via Tauri `bundle.windows.signCommand`.
     3. Outer `*.msi` and `*-setup.exe` installers, verified via `signtool.exe verify /pa /tw /v` (`Status = Valid`).
3. **PE Manifest & Strict SAC Non-Bypass Mandate:**
   - Preserve `<requestedExecutionLevel level="asInvoker" uiAccess="false" />`, `longPathAware=true`, `PerMonitorV2`, Windows 10/11 `<supportedOS>` GUIDs, and MSVC `GUARD_CF` (`0006-IR-006`).
   - **NEVER** disable Smart App Control (`VerifiedAndReputablePolicyState = 1`), modify `HKLM\...\CI\Policy`, or use signed PowerShell bypasses to launch unsigned `.exe` files. Until G3 is cleared, SAC=`1` Windows 11 endpoints evaluate via ADR-002's **Signed-Host Bridge** (`scripts/dim-host.ps1` + `apps/desktop/ui-server.mjs`).

---

### Gate G4 — macOS Code Signing, Hardened Runtime & Notarization Gate

1. **Native Hardware & Bundle Overlay:** Build `aarch64-apple-darwin` natively on `macos-15` (`arm64`) and `x86_64-apple-darwin` natively on `macos-15-intel` (`x86_64`), injecting valid `icon.icns` (`1,142` B) and `128x128@2x.png` (`716` B) via `tauri-ci-bundle-overlay.json` (or upstream patch `0002-IR-002`).
2. **Developer ID & Hardened Runtime:** Sign inside-out with `Developer ID Application: <Org> (<TeamID>)`, `--options runtime --timestamp`, and minimal `entitlements.plist` (avoiding `disable-library-validation` or `allow-unsigned-executable-memory`).
3. **Notarization & Stapling:** Submit `.dmg` via `xcrun notarytool submit --wait`, attach offline ticket via `xcrun stapler staple` + `stapler validate`, and verify via `codesign --verify --deep --strict` and `spctl --assess --type execute`.
4. **Credential & Gatekeeper Hygiene:** Zero unauthorized Apple keys in CI; current builds remain `UNSIGNED_DEVELOPMENT_ARTIFACT` / `NOT_NOTARIZED` (quarantined by Gatekeeper on clean Macs; `sudo spctl --master-disable` is strictly forbidden). Clearing G4 also requires physical Mac runtime verification of `/Applications/Zalo.app`.

---

### Gate G5 — Ubuntu 24.04 LTS Package Integrity & ABI Gate

1. **ELF, Xvfb & Shared-Library Verification:** Verify 64-bit PIE ELF mode `0755`, `ldd` zero unresolved symbols, `xvfb-run -a` WebKitGTK startup probe, and `.deb` dependencies (`libwebkit2gtk-4.1-0`, `libgtk-3-0t64 | libgtk-3-0`, `libayatana-appindicator3-1 | libappindicator3-1` via `tauri-ci-bundle-overlay.json` / `0003-IR-003`).
2. **Cryptographic Checksum & Package Signatures:** Sign published `SHA256SUMS` / APT `InRelease` with a dedicated release OpenPGP key and `.AppImage` (`AI\x02` Type 2, documenting `libfuse2t64` or `--appimage-extract-and-run`) with detached GPG/minisign signatures.
3. **`GLIBC` Floor & AppArmor Disclosure:** Label Ubuntu 24.04 artifacts **`Ubuntu 24.04+ (GLIBC >= 2.38/2.39)`** — never claim Ubuntu 22.04 (`glibc 2.35`) compatibility. Disclose Ubuntu 24.04 AppArmor `kernel.apparmor_restrict_unprivileged_userns = 1` behavior and keep Linux positioned strictly as **Experimental (Official VNG Zalo Unsupported)**.

---

### Gate G6 — Human Release Approval, Default-Branch Rollout & Least-Privilege CI Gate

1. **Read-Only Permissions & Zero Auto-Release:** `.github/workflows/cross-platform-validation.yml` enforces top-level `permissions: contents: read`, zero `${{ secrets.* }}` references, `retention-days: 7`, and zero release-creation steps.
2. **Controlled Rollout & Private-Repo Cost Guard:** Manual `workflow_dispatch` requires landing the workflow on `origin/main` first via interactive `gh auth refresh -h github.com -s workflow,user` and reviewed fast-forward/PR merge (zero `--force`, zero plaintext PAT scripts). Both `$0.062/min` (`10x`) macOS jobs default to `false` (`$0.246/run` default vs `$2.044/run` full 4-OS, saving `88.0%`).

---

## 4. Related Packaging & Governance Documents

- [`ci-execution-and-artifact-validation.md`](./ci-execution-and-artifact-validation.md) | [`build-matrix.md`](./build-matrix.md) | [`release-readiness.md`](./release-readiness.md)
- [`windows.md`](./windows.md) | [`macos.md`](./macos.md) | [`ubuntu.md`](./ubuntu.md)
- [`../security/threat-model-and-audit.md`](../security/threat-model-and-audit.md) | [`../compatibility/os-readiness.md`](../compatibility/os-readiness.md)
