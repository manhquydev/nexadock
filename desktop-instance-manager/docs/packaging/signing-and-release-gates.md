# Release Governance, Code Signing & Quality Gates (G0–G6)

**Document Owner:** Release Governance & Build Matrix Architecture  
**Applies To:** `desktop-instance-manager` (`Multi-Zalo Desktop`, `com.multizalodesktop.manager`)  
**Enforcement Tooling:** [`scripts/ci-cross-platform/`](../../scripts/ci-cross-platform/) and [`.github/workflows/cross-platform-validation.yml`](../../../.github/workflows/cross-platform-validation.yml)  
**Current Release Posture:** **`DEVELOPMENT_VALIDATION_ONLY`** — zero artifacts in this repository are signed production releases (`signing_status = "UNSIGNED_DEVELOPMENT_ARTIFACT"`).

---

## 1. Governance Principles & Non-Negotiable Safety Rules

Every build, package, validation run, and future distribution candidate of `desktop-instance-manager` is governed by five non-negotiable invariants:

1. **Zero VNG Proprietary Payload:** No build script, CI job, cache entry, executable, or installer bundle may contain, redistribute, download, extract, or binary-patch any VNG Zalo Desktop binary or archive (`Zalo.exe`, `Zalo.app`, `app.asar`, `ZaloSetup-*.exe`, `ZaloCall.exe`, `ZaloCap.exe`, `Update.exe`, or third-party repackaged `app.asar` bundles).
2. **Strict Artifact Truthfulness:**
   - A **compiled executable** (`dim-desktop.exe`, `dim-cli.exe`, `dim-test-fixture.exe`, `dim-desktop`) is **never** an installer (`is_installer: false`).
   - An **unsigned artifact** is **never** labeled signed, verified, notarized, or production-ready; it must carry `signing_status: "UNSIGNED_DEVELOPMENT_ARTIFACT"`.
   - **Manager compilation/packaging is not Zalo runtime verification:** compiling or bundling `dim-desktop` on Windows, macOS, or Ubuntu proves only that the Rust/Tauri instance-manager shell builds for that OS/CPU target. It **never** proves that VNG Zalo multi-instance isolation works on macOS (`NOT_TESTED` on physical Apple hardware) or that Zalo Desktop works on Linux (`UNSUPPORTED` for official VNG Zalo; `EXPERIMENTAL` and `NOT_TESTED` for user-supplied local AppImages).
3. **Zero OS Security Policy Bypass:** Never disable, weaken, or instruct users or CI scripts to bypass Windows Smart App Control (SAC), Windows Defender Application Control (WDAC), Windows UAC (`asInvoker` strictly enforced), macOS Gatekeeper (`spctl`), or Ubuntu 24.04 AppArmor user-namespace protections. Never use signed PowerShell scripts or system binaries as an execution-policy bypass to launch unsigned executables.
4. **Zero Unauthorized Credentials or External Uploads:** Never fabricate self-signed certificates disguised as release signatures, never commit signing keys (`.pfx`, `.p12`, `.asc`, API keys), and never cache secrets in GitHub Actions caches.
5. **Human-Gated Promotion Only:** Automated CI workflows operate strictly under `permissions: contents: read` and never create GitHub Releases, push Git tags, or publish end-user installers automatically.

---

## 2. Release Gate Summary Matrix (G0–G6)

An artifact candidate may advance from **Development Validation** to **Public Production Release** on a target OS **only when every applicable gate G0 through G6 passes (`PASS`)**. Failure or `BLOCKED` status on any gate halts public release for that target.

| Gate ID | Gate Name | Scope | Automated Enforcement / Evidence | Current Baseline Status |
| :--- | :--- | :--- | :--- | :--- |
| **G0** | **Source & Dependency Integrity** | Lockfiles, SPDX licenses, banned crates, registry pinning, MSRV, SBOM | `dependency-audit.mjs`, `deny.toml`, `plans/reports/build/dependency-audit-report.json` | **PASS (with IR-001 MSRV alignment warning)** |
| **G1** | **Zero VNG Binary Policy** | Repo tree, Cargo/Tauri targets, installer bundles, CI artifacts | `dependency-audit.mjs`, `build-and-package.mjs`, `generate-artifact-manifest.mjs` (`vng_binary_embedded: false`) | **PASS** |
| **G2** | **Artifact Classification & Honesty** | Binary vs installer taxonomy, unsigned labeling, DIM build vs Zalo runtime separation | `inspect-artifact.mjs`, `generate-artifact-manifest.mjs`, `plans/reports/build/artifact-validation-manifest.json` | **PASS** |
| **G3** | **Windows Code Signing & SAC Compliance** | EV/OV Authenticode or Azure Trusted Signing, RFC 3161 timestamp, inner+outer MSI/NSIS signing, SAC policy preservation | `inspect-artifact.mjs` (`IMAGE_DIRECTORY_ENTRY_SECURITY`), `Get-AuthenticodeSignature`, `signtool verify /pa` | **BLOCKED for Public Release** (`UNSIGNED_DEVELOPMENT_ARTIFACT`; no Authenticode cert provisioned) |
| **G4** | **macOS Signing, Hardened Runtime & Notarization** | Developer ID Application, Hardened Runtime, `notarytool`, `stapler`, Gatekeeper `spctl` | `inspect-artifact.mjs` (`codesign -dv`), `xcrun notarytool`, `spctl --assess` | **BLOCKED for Public Release** (`UNSIGNED_DEVELOPMENT_ARTIFACT`; no Apple Developer ID cert provisioned + runtime `NOT_TESTED`) |
| **G5** | **Ubuntu Package Integrity & ABI Disclosure** | `.deb` metadata & GPG signing, AppImage detached signature, `GLIBC` 24.04 floor & AppArmor disclosure | `inspect-artifact.mjs` (`dpkg-deb -I`, `ldd`, `GLIBC_*` scan, ELF mode `0755`), detached SHA-256/GPG | **BLOCKED for Public Release** (unexecuted on live Ubuntu 24.04 runner; no release GPG/minisign key provisioned; Official Zalo `UNSUPPORTED`) |
| **G6** | **Human Release Approval & Read-Only CI** | `workflow_dispatch` manual control, `permissions: contents: read`, zero auto-release | `validate-workflow.mjs`, `.github/workflows/cross-platform-validation.yml` | **PASS (Guardrail Active)** |

---

## 3. Detailed Gate Specifications

### Gate G0 — Source & Dependency Integrity Gate

**Objective:** Guarantee deterministic, auditable, permissive-licensed open-source dependency resolution across both the Rust core workspace and the Tauri 2 desktop shell.

#### Mandatory Controls
1. **Dual Lockfile Immutability (`--locked`):**
   - Both [`desktop-instance-manager/Cargo.lock`](../../Cargo.lock) (workspace crates + CLI + test fixture) and [`desktop-instance-manager/apps/desktop/src-tauri/Cargo.lock`](../../apps/desktop/src-tauri/Cargo.lock) (Tauri `2.12.6` desktop shell) must be committed and verified with `--locked` during every `cargo test`, `cargo check`, `cargo clippy`, and `cargo build` invocation.
   - Any drift that would mutate either `Cargo.lock` fails the build immediately.
2. **Policy Enforcement ([`deny.toml`](../../deny.toml) & [`dependency-audit.mjs`](../../scripts/ci-cross-platform/dependency-audit.mjs)):**
   - **Allowed Registries:** Exclusively `https://github.com/rust-lang/crates.io-index` (`unknown-registry = "deny"`, `unknown-git = "deny"`, `allow-git = []`).
   - **Cryptographic Package Checksums:** Every resolved registry package in both `Cargo.lock` files must carry a non-empty 64-hex SHA-256 `checksum` field; `yanked = "deny"` and `wildcards = "deny"` are enforced.
   - **Allowed SPDX License Expressions:** `MIT`, `Apache-2.0`, `Apache-2.0 WITH LLVM-exception`, `BSD-2-Clause`, `BSD-3-Clause`, `ISC`, `Unicode-3.0`, `Zlib`, `MPL-2.0`. Copyleft (`GPL-*`, `AGPL-*`) or unapproved proprietary licenses fail G0.
   - **Banned Crates:** `openssl` and `openssl-sys` are explicitly denied in favor of memory-safe/native TLS backends (`rustls`), eliminating OpenSSL C toolchain drift across Windows MSVC, macOS, and Ubuntu.
   - **Zero Third-Party Frontend Runtime Dependencies:** `apps/desktop/ui/` and `scripts/ci-cross-platform/*.mjs` use standard browser APIs and Node.js 22 built-in `node:*` modules exclusively (`0` runtime `npm` dependencies).
3. **MSRV Governance & Known Alignment Item (`IR-001`):**
   - `desktop-instance-manager/Cargo.toml` declares `rust-version = "1.85"`, whereas `apps/desktop/src-tauri/Cargo.toml` declares `rust-version = "1.90"` (required by `tauri 2.12.6` / transitive `edition2024` crates).
   - **Effective Release MSRV:** **`1.90.0`**. `dependency-audit.mjs` flags this 1.85 vs 1.90 divergence as an explicit audit warning tracked in [`plans/reports/build/integration-requests.md`](../../plans/reports/build/integration-requests.md) (`IR-001`) until unified in a source-maintenance PR.
4. **SBOM & Provenance Retention:**
   - Release candidates must publish machine-readable dependency audit output (`dependency-audit-report.json`) alongside CycloneDX 1.5 SBOM metadata covering the union of both lockfiles.

---

### Gate G1 — Zero VNG Binary Policy Gate

**Objective:** Maintain strict legal, licensing, and supply-chain separation between `Multi-Zalo Desktop` (MIT/Apache-2.0 instance manager) and VNG Corporation's proprietary Zalo Desktop client.

#### Mandatory Controls
1. **Absolute Prohibition on Bundling or Redistribution:**
   - No repository directory, Tauri `bundle.resources` / `bundle.externalBin` entry, Docker/CI layer, GitHub Actions artifact upload, `.msi`, NSIS `-setup.exe`, `.app`, `.dmg`, `.deb`, or `.AppImage` may include:
     - Windows VNG binaries (`Zalo.exe`, `ZaloCall.exe`, `ZaloCap.exe`, `Update.exe`, `ZaloSetup-*.exe`, `resources/app.asar`).
     - macOS VNG bundles (`Zalo.app`, `ZaloHelper.app`, `Contents/Resources/app.asar`, `Zalo-*.dmg`).
     - Unofficial Linux community Electron/Wine redistributions containing extracted VNG `app.asar` payloads.
2. **Zero Patching or Injection:**
   - `desktop-instance-manager` never modifies `Zalo.exe`, never repacks `app.asar`, never injects DLLs/`DYLD_INSERT_LIBRARIES`/`LD_PRELOAD`, and never alters `%APPDATA%\ZaloData` or `~/Library/Application Support/ZaloData`.
3. **Automated Pre- & Post-Build Enforcement:**
   - `dependency-audit.mjs` scans the repository workspace to confirm zero embedded VNG binaries.
   - `build-and-package.mjs` and `generate-artifact-manifest.mjs` scan compiled output and bundle staging trees and assert `"vng_binary_embedded": false` on every artifact entry. Any match aborts packaging with a non-zero exit code.
4. **End-User Acquisition Model:**
   - End users install official Zalo Desktop directly from VNG on supported operating systems. At launch time on Windows, `dim-platform-windows` dynamically locates `%LOCALAPPDATA%\Programs\Zalo\Zalo-<ver>\Zalo.exe` and cryptographically verifies its VNG Authenticode signature (`WinVerifyTrust` + exact X.500 `CN=VNG GROUP JSC` / `O=VNG GROUP JSC` RDN pinning) before spawning an isolated process.

---

### Gate G2 — Artifact Classification & Honesty Gate

**Objective:** Prevent misleading terminology or overclaimed platform support in manifests, CI summaries, release notes, and user documentation.

#### Three Mandatory Classification Axes
1. **Axis 1 — Compiled Executable vs. Installer / Bundle (`is_installer`):**
   - Raw Cargo outputs (`target/<triple>/release/dim-desktop.exe`, `dim-cli.exe`, `dim-test-fixture.exe`, `dim-desktop`) are classified strictly as `artifact_kind: "compiled-executable"` with `is_installer: false`. **Calling a raw `.exe` or ELF binary an "installer" is a Gate G2 failure.**
   - Only genuine installer packages produced by OS bundlers (`*.msi` -> `installer-msi`, `*-setup.exe` -> `installer-nsis`, `*.dmg` -> `macos-dmg-installer`, `*.deb` -> `debian-package`) set `is_installer: true`. Portable application bundles (`*.app` -> `macos-app-bundle`, `*.AppImage` -> `linux-appimage`) are explicitly typed by container format.
2. **Axis 2 — Cryptographic Signing & Trust Posture (`signing_status`):**
   - Every artifact inspected without a chain-trusted code signature (or Apple notarization where required) **must** record `signing_status: "UNSIGNED_DEVELOPMENT_ARTIFACT"`.
   - Labels such as `"signed"`, `"trusted"`, `"verified-release"`, or `"production"` are prohibited unless verified by platform-native cryptographic tooling (`signtool verify /pa`, `codesign --verify` + `spctl --assess`, or GPG/minisign verification).
3. **Axis 3 — Manager Build Status vs. Zalo Runtime Multi-Instance Status (`zalo_runtime_compat_status`):**
   - Manifests and readiness reports must report **two separate statuses** per target:
     1. *DIM Build & Package Status* (whether `dim-desktop` / `dim-cli` compiled, passed unit tests, and bundled).
     2. *Zalo Multi-Instance Runtime Compatibility Status* (whether official VNG Zalo Desktop is proven to run isolated concurrent accounts on that OS).
   - Specifically:
     - **Windows 10/11 x64:** DIM build verified locally; Zalo runtime verified at L1/L2 (`3` concurrent official Zalo `26.10.20` instances + protected primary); L3 QR multi-account chat (`NOT_TESTED`) and L4 concurrent calls/per-profile toasts (`BLOCKED` by VNG global pipes/AUMID) disclosed honestly.
     - **macOS (`aarch64` & `x86_64`):** Even when `dim-desktop` compiles and packages into `.app`/`.dmg`, Zalo macOS runtime multi-instance isolation (`open -n --env CFFIXED_USER_HOME=...`) remains **`NOT_TESTED`** until empirically validated on physical Apple hardware.
     - **Ubuntu 24.04 LTS (`x86_64`):** Even when `dim-desktop` compiles and packages into `.deb`/`.AppImage`, Official VNG Zalo Desktop on Linux is **`UNSUPPORTED`** (VNG ships no Linux client), and user-supplied local AppImage / fixture isolation is **`EXPERIMENTAL` (`NOT_TESTED` on Linux runtime)**.

---

### Gate G3 — Windows Code Signing & Smart App Control (SAC) Gate

**Objective:** Meet Windows 11 Smart App Control (`VerifiedAndReputablePolicyState = 1`), Microsoft Defender SmartScreen, and enterprise WDAC cryptographic requirements without ever weakening host security policies.

#### Production Signing Requirements (Required to Clear G3)
1. **Certificate Authority & Key Custody:**
   - Provision an **Extended Validation (EV)** or **Organization Validation (OV) Authenticode Code Signing Certificate** issued by a CA in the Microsoft Trusted Root Program, **or** an **Azure Trusted Signing** (formerly Azure Artifact Signing) identity profile.
   - Private keys must reside in a FIPS 140-2 Level 2+ hardware token / Cloud HSM (per CA/Browser Forum Code Signing Baseline Requirements). Exporting raw `.pfx` files into repository storage or unencrypted CI artifacts is prohibited.
2. **Digest Algorithm & RFC 3161 Timestamping:**
   - Sign using `SHA-256` file digest (`/fd SHA256`) and an **RFC 3161 compliant timestamp server** (`/tr <https-tsa-url> /td SHA256`) so signatures remain cryptographically valid after the signing certificate expires:
     ```powershell
     signtool.exe sign /fd SHA256 /tr "http://timestamp.acs.microsoft.com" /td SHA256 /v <target-file>
     ```
3. **Mandatory Multi-Layer ("Inner + Outer") Signing Order:**
   - Windows Smart App Control inspects **every** PE binary extracted to disk, not merely the outer installer wrapper. Production CI must sign in strict inside-out order:
     1. **Inner Compiled Executables & Sidecars:** Sign `dim-desktop.exe` and `dim-cli.exe` (and verify Microsoft's signature on bundled `WebView2Loader.dll`) *before* invoking WiX or NSIS.
     2. **NSIS Uninstaller Stub:** Configure Tauri v2 `bundle.windows.signCommand` so Tauri's bundler invokes `signtool.exe` on the generated NSIS `uninstall.exe` before packing the outer setup executable.
     3. **Outer Installer Packages:** Sign the final `.msi` Windows Installer package and `*-setup.exe` NSIS installer executable.
4. **PE Header & Embedded Manifest Verification:**
   - Confirm PE Optional Header `IMAGE_DIRECTORY_ENTRY_SECURITY` virtual address and byte size are non-zero (`inspect-artifact.mjs`).
   - Confirm embedded application manifest preserves `<requestedExecutionLevel level="asInvoker" uiAccess="false" />` (never `requireAdministrator`), Per-Monitor V2 DPI awareness, and Windows 10/11 `supportedOS` GUIDs.
   - Verify signature chain and timestamp on a clean Windows runner:
     ```powershell
     signtool.exe verify /pa /tw /v <artifact-path>
     ```
     (`Get-AuthenticodeSignature` must report `Status = Valid` and non-null `TimeStamperCertificate`.)

#### Strict Smart App Control (SAC) & PowerShell Non-Bypass Policy
- **Prohibited Actions:**
  - **NEVER** modify `HKLM\SYSTEM\CurrentControlSet\Control\CI\Policy\VerifiedAndReputablePolicyState` or advise users to turn off Smart App Control (turning off SAC on Windows 11 is irreversible without a full OS reset).
  - **NEVER** add Microsoft Defender exclusions, inject self-signed root certificates into `Cert:\LocalMachine\Root` to spoof production trust, or use signed PowerShell scripts / LOLBins to bypass application control and execute unsigned PE binaries.
- **Sanctioned Pre-G3 Local Evaluation Path (ADR-002):**
  - While standalone Rust/Tauri PE binaries remain `UNSIGNED_DEVELOPMENT_ARTIFACT`, users on SAC-enforcing Windows 11 hosts evaluate `Multi-Zalo Desktop` exclusively via the **Signed-Host Bridge** (`scripts/dim-host.ps1` + `apps/desktop/ui-server.mjs`), which runs native domain logic inside Microsoft-signed `powershell.exe` and OpenJS-signed `node.exe` without spawning any unsigned `.exe`.

---

### Gate G4 — macOS Code Signing, Hardened Runtime & Notarization Gate

**Objective:** Satisfy Apple macOS Gatekeeper, XProtect, and notarization enforcement on macOS 14 (Sonoma) and macOS 15 (Sequoia) across both Apple Silicon (`aarch64-apple-darwin`) and Intel (`x86_64-apple-darwin`).

#### Production Signing & Notarization Requirements (Required to Clear G4)
1. **Bundle Completeness Prerequisite (`IR-002` & `IR-003`):**
   - Resolve `IR-002` (ship `apps/desktop/src-tauri/icons/icon.icns` and register it in `tauri.conf.json`) and `IR-003` (`bundle.macOS.minimumSystemVersion`) before building release `.app` / `.dmg` candidates.
2. **Developer ID Application & Inside-Out Signing:**
   - Sign using an official Apple Developer Program **`Developer ID Application: <Organization> (<TeamID>)`** certificate (never ad-hoc `-` signing for public distribution).
   - Sign strictly inside-out (nested Mach-O helpers/dylibs first, then `Multi-Zalo Desktop.app/Contents/MacOS/dim-desktop`, then `Multi-Zalo Desktop.app`) with Hardened Runtime and secure Apple timestamping:
     ```bash
     codesign --force --options runtime --timestamp \
       --entitlements apps/desktop/src-tauri/entitlements.plist \
       --sign "Developer ID Application: <Org> (<TeamID>)" \
       "Multi-Zalo Desktop.app"
     ```
3. **Least-Privilege Hardened Runtime Entitlements:**
   - Keep `entitlements.plist` minimal. Because Tauri 2 renders via macOS system `WKWebView` out-of-process XPC services, do **not** grant `com.apple.security.cs.disable-library-validation`, `com.apple.security.cs.allow-unsigned-executable-memory`, or `com.apple.security.cs.allow-dyld-environment-variables` on `dim-desktop` (note: child Zalo launch passes `CFFIXED_USER_HOME` via `/usr/bin/open -n -a /Applications/Zalo.app --env ...`, which requires empirical verification on physical Mac hardware before clearing runtime readiness).
4. **Apple Notarization & Ticket Stapling:**
   - Package the signed `.app` into `.dmg`, sign the `.dmg`, and submit to Apple Notary Service via App Store Connect API key stored exclusively in protected release environment secrets:
     ```bash
     xcrun notarytool submit "Multi-Zalo Desktop_0.1.0_aarch64.dmg" \
       --key "$APPLE_API_KEY_PATH" --key-id "$APPLE_KEY_ID" --issuer "$APPLE_ISSUER_ID" --wait
     xcrun stapler staple "Multi-Zalo Desktop_0.1.0_aarch64.dmg"
     xcrun stapler validate "Multi-Zalo Desktop_0.1.0_aarch64.dmg"
     ```
5. **Gatekeeper Verification Commands:**
   ```bash
   codesign --verify --deep --strict --verbose=2 "Multi-Zalo Desktop.app"
   spctl --assess --type execute --verbose=4 "Multi-Zalo Desktop.app"
   spctl --assess --type open --context context:primary-signature --verbose=4 "Multi-Zalo Desktop_0.1.0_aarch64.dmg"
   ```

#### Strict Apple Credential & Gatekeeper Governance
- **Zero Unauthorized Key Handling:** CI validation workflows must never prompt for Apple IDs, never upload repository-resident keys, and never fail validation runs merely because Apple signing secrets are absent — instead, `check-prerequisites.mjs` and `generate-artifact-manifest.mjs` record `signing_status: "UNSIGNED_DEVELOPMENT_ARTIFACT"` and `notarization_status: "NOT_NOTARIZED"`.
- **No Gatekeeper Disabling:** Documentation and scripts must **never** instruct users to run `sudo spctl --master-disable` or strip quarantine attributes globally as a substitute for proper Developer ID notarization.
- **Runtime Gate Dependency:** Clearing G4 cryptographic notarization alone does **not** promote macOS to production-ready; physical macOS runtime verification of `dim-platform-macos` (`CFFIXED_USER_HOME`, Keychain behavior, `/tmp/socketzalorecv2021` call socket conflict) is also required (see [`os-readiness.md`](../compatibility/os-readiness.md)).

---

### Gate G5 — Ubuntu 24.04 LTS Package Integrity & ABI Gate

**Objective:** Ensure Linux packages built on Ubuntu 24.04 LTS (`x86_64-unknown-linux-gnu`) carry verifiable cryptographic integrity, accurate Debian shared-library dependencies, and transparent `GLIBC` / AppArmor disclosures.

#### Package Integrity & Verification Requirements (Required to Clear G5)
1. **ELF Binary Hardening & Permissions:**
   - `dim-desktop` and `dim-cli` must be 64-bit `x86-64` PIE ELF executables with standard owner/group/world read-execute permissions (`0755`), stripped of local build-path RPATHs (`patchelf`), and verified via `ldd` with zero `"not found"` shared objects on clean Ubuntu 24.04 LTS.
2. **Debian Package (`.deb`) Control & Signature Integrity:**
   - `dpkg-deb -I` must declare explicit runtime dependencies matching Ubuntu 24.04 Noble package names (`IR-003`): `libwebkit2gtk-4.1-0`, `libgtk-3-0t64`, `libayatana-appindicator3-1` (preferred over legacy `libappindicator3-1`), and `librsvg2-2`.
   - Public `.deb` releases must publish `SHA256SUMS` signed with a dedicated project release OpenPGP key (`SHA256SUMS.asc` / `InRelease` for APT repositories, or `debsigs` / `dpkg-sig` archive signatures).
3. **AppImage (`.AppImage`) Verification & Runtime Disclosure:**
   - Verify ELF Type 2 AppImage magic (`AI\x02` at byte offset `8`) and `0755` executable mode.
   - Publish `SHA256SUMS` plus a detached Ed25519 (`minisign`) or OpenPGP (`.asc`) signature for every `.AppImage`.
   - Document that Ubuntu 24.04 LTS does **not** preinstall legacy `libfuse.so.2`; users running Type 2 AppImages require `sudo apt install libfuse2t64` or `--appimage-extract-and-run`.
4. **Mandatory `GLIBC` ABI Floor Disclosure:**
   - Artifacts compiled on `ubuntu-24.04` link against Ubuntu Noble's `glibc 2.39` toolchain and `libwebkit2gtk-4.1-0` (`inspect-artifact.mjs` extracts maximum `GLIBC_2.xx` symbol versions).
   - **Rule:** Ubuntu 24.04 build artifacts must be explicitly labeled **`Ubuntu 24.04+ (GLIBC >= 2.38/2.39)`** and must **never** be advertised as compatible with Ubuntu 22.04 LTS (`glibc 2.35`) unless rebuilt inside an Ubuntu 22.04 sysroot/container with WebKitGTK 4.1 backports.
5. **AppArmor User-Namespace & Official Zalo Honesty:**
   - Ubuntu 24.04 enables `kernel.apparmor_restrict_unprivileged_userns = 1` by default. `dim-platform-linux` must never create privileged root AppArmor overrides automatically and must disclose `--no-sandbox` tradeoffs if a user supplies a local Electron AppImage.
   - Because VNG ships **no** official Linux Zalo Desktop client (`SupportStatus::Unsupported`), Linux packages of `dim-desktop` may only be positioned as **experimental developer/fixture builds** unless VNG releases an official Linux client.

---

### Gate G6 — Human Release Approval & Least-Privilege CI Gate

**Objective:** Prevent accidental or unauthorized publication, runaway private-repository Actions billing, and unreviewed promotion of development builds.

#### Mandatory Controls
1. **Read-Only Workflow Permissions:**
   - [`.github/workflows/cross-platform-validation.yml`](../../../.github/workflows/cross-platform-validation.yml) enforces top-level `permissions: contents: read`.
   - No job may request `contents: write`, `releases: write`, or `packages: write`, and no step may invoke `gh release create`, `softprops/action-gh-release`, or `git push --tags`.
2. **Manual `workflow_dispatch` & Cost Protection:**
   - Cross-platform packaging runs **only** via manual `workflow_dispatch` (zero `schedule` crons, zero duplicate `push`/`pull_request` triggers overlapping [`.github/workflows/ci.yml`](../../../.github/workflows/ci.yml)).
   - High-multiplier macOS jobs (`10x` GitHub Actions billing multiplier on private repositories) default to `false` (`enable_macos_arm64: false`, `enable_macos_intel: false`) unless a human operator intentionally enables them for a targeted validation run.
3. **Ephemeral Artifact Retention & Cache Hygiene:**
   - Validation artifacts uploaded via `actions/upload-artifact@v4` use short retention (`retention-days: 7`) and include `artifact-manifest.json` + `SHA256SUMS.txt` marking every file `UNSIGNED_DEVELOPMENT_ARTIFACT`.
   - Cargo caches key strictly on lockfile hashes (`Cargo.toml` / `Cargo.lock`) and never store signing keys, tokens, or user data.
4. **Human Release Sign-Off Checklist:**
   Before any future `vX.Y.Z` production release tag is approved by a human maintainer:
   - [ ] **G0:** `dependency-audit.mjs` and `cargo-deny` pass on both lockfiles with zero errors (`IR-001` resolved).
   - [ ] **G1:** `vng_binary_embedded == false` confirmed across all bundles.
   - [ ] **G2:** Manifest `artifact_kind`, `is_installer`, `signing_status`, and `zalo_runtime_compat_status` verified accurate.
   - [ ] **G3 (Windows):** Inner `.exe`s, NSIS uninstaller, `.msi`, and `-setup.exe` signed with OV/EV Authenticode + RFC 3161 timestamp; verified on Windows 11 with Smart App Control `VerifiedAndReputablePolicyState = 1`.
   - [ ] **G4 (macOS):** Physical Mac L1–L3 Zalo runtime verification passed + Developer ID signed, Hardened Runtime enabled, Apple notarized, and stapled (`spctl --assess` `accepted`).
   - [ ] **G5 (Ubuntu):** Detached GPG/minisign signatures verified, `GLIBC` floor documented, and runtime scope accurately constrained.
   - [ ] **G6:** Maintainer reviews `plans/reports/build/artifact-validation-manifest.json` and `SHA256SUMS.txt` and signs off explicitly.

---

## 4. Related Packaging & Governance Documents

- Cross-Platform Build & Runner Matrix: [`build-matrix.md`](./build-matrix.md)
- Evidence-Backed Release Readiness Scorecard: [`release-readiness.md`](./release-readiness.md)
- Platform Packaging Guides: [`windows.md`](./windows.md) | [`macos.md`](./macos.md) | [`ubuntu.md`](./ubuntu.md)
- Threat Model & Security Audit: [`../security/threat-model-and-audit.md`](../security/threat-model-and-audit.md)
- OS Runtime Compatibility Matrix: [`../compatibility/os-readiness.md`](../compatibility/os-readiness.md)
