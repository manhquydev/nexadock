# macOS Packaging & Distribution Guide (`dim-desktop`)

**Document Scope:** Native runner architecture, build/packaging pipeline, `IR-002`/`IR-003` CI bundle overlay & atomic patches, Apple code-signing/notarization gates, and runtime honesty boundaries for **Multi-Zalo Desktop — Instance Manager (`dim-desktop`)** on Apple macOS.  
**Native Target Architectures:** `aarch64-apple-darwin` (Apple Silicon M-series on `macos-15`) and `x86_64-apple-darwin` (Intel Mac on `macos-15-intel`).  
**Current Artifact Signing & Provenance State:** `UNSIGNED_DEVELOPMENT_ARTIFACT` (`NOT_NOTARIZED`, `provenance_trust_level: "SELF_REPORTED_UNATTESTED"`) — **strictly for development/CI validation, NEVER a production release**.  
**Zalo macOS Runtime Support Status:** **`NOT_TESTED`** (`SupportStatus::NotTested` in [`crates/platform-macos/src/adapter.rs`](../../crates/platform-macos/src/adapter.rs)).

---

> [!IMPORTANT]
> **CRITICAL HONESTY GATE — Manager Shell Build != Zalo macOS Runtime Verification**
>
> 1. **Manager Build Only:** Compiling `dim-desktop` and packaging `Multi-Zalo Desktop.app` or `.dmg` on macOS **ONLY** proves that the Rust workspace, `dim-platform-macos` adapter code, synthetic runtime fixture (`fixture-app.mjs` + `dim-cli`), and Tauri 2 (`WKWebView`) desktop manager shell compile and bundle cleanly. It is **NOT** proof that official VNG `/Applications/Zalo.app` multi-instance isolation works at runtime on physical macOS hardware (`SupportStatus::NotTested`).
> 2. **Native Hardware Only (Zero Cross-Compile Masquerading):** Cross-compiling `x86_64-apple-darwin` on an Apple Silicon (`arm64`) host is **NEVER** treated as native Intel runtime verification. Both `cross-platform-validation.yml` and `build-and-package.mjs` enforce strict `uname -m` and `rustc -vV` native host gates.
> 3. **Remote CI Blocker Honesty:** Remote GitHub Actions execution is currently **`BLOCKED`** (`0` remote runs — `origin/main` at `8cf4703` lacks workflow registration on the default branch and local `gh` OAuth scopes `'gist, read:org, repo'` lack `'workflow'`; see [`ci-execution-and-artifact-validation.md`](./ci-execution-and-artifact-validation.md) and [`ci-baseline-and-github-audit.md`](../../plans/reports/build/ci-baseline-and-github-audit.md)).
> 4. **Zero VNG Binary Policy:** Neither `Multi-Zalo Desktop.app` nor any `.dmg` artifact embeds, copies, modifies, or redistributes VNG's `Zalo.app`, `ZaloHelper.app`, or `app.asar`. Users install official Zalo for macOS directly from VNG into `/Applications/Zalo.app`.

---

## 1. Explicit Native macOS Runner Matrix & Architecture Gates

Per official GitHub-hosted runner documentation (`docs.github.com`, verified in [`ci-baseline-and-github-audit.md`](../../plans/reports/build/ci-baseline-and-github-audit.md)), legacy `macos-13` Intel runners are retired, `macos-14` and `macos-15` are **Apple Silicon (`arm64`)**, and **`macos-15-intel`** is GitHub's explicit Intel `x86_64` macOS runner (scheduled for sunset in Fall 2027).

| Workflow Job & Priority | GitHub Runner Label | Native Hardware Gate (`uname -m`) | Native Rust Host Gate (`rustc -vV`) | Timeout & Dispatch Default | Unavailable Runner Policy |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **3. `macos-arm64-build`** | **`macos-15`** (3-core Apple M1) | `test "$(uname -m)" = "arm64"` | `host: aarch64-apple-darwin` | `25 min`; opt-in `enable_macos_arm64: false` | Fails closed if host is not native `arm64` |
| **4. `macos-intel-build`** | **`macos-15-intel`** (4-core Intel `x86_64`) | `test "$(uname -m)" = "x86_64"` | `host: x86_64-apple-darwin` | `30 min`; opt-in `enable_macos_intel: false` | Reports **`BLOCKED`** if `macos-15-intel` is unavailable on repo billing plan/pool — **never** cross-compiled on `arm64` |

### Why Native Architecture Gates Are Enforced
1. **Cross-Compilation != Native Runtime Testing:** Compiling `x86_64-apple-darwin` against Xcode's universal `MacOSX.sdk` on an `arm64` runner does not exercise native Intel `x86_64` execution, dynamic linking, or runtime fixture behavior (`build-and-package.mjs` sets `native_runtime_test_possible: false` whenever host architecture != target architecture). Therefore, `macos-intel-build` runs exclusively on **`macos-15-intel`** and aborts immediately if `uname -m` is not `x86_64`.
2. **Transparent `BLOCKED` Reporting:** If a repository's GitHub billing plan or regional runner queue does not provision `macos-15-intel` (or after GitHub's Fall 2027 Intel runner sunset), Intel macOS CI validation must be recorded honestly as **`BLOCKED`** until verified on dedicated Intel hardware — never masked by an `arm64` cross-build.
3. **Private-Repository Cost Governance (`$0.062/min`, `10x` Multiplier):** On private repositories (`manhquydev/zalo`), GitHub Actions bills standard macOS runners at **`$0.062/min`** (`10x` minute multiplier vs Linux `$0.006/min` `1x`). Keeping `enable_macos_arm64` and `enable_macos_intel` opt-in (`false`) on manual `workflow_dispatch` reduces routine validation cost from `~$2.044/run` (`335` multiplier minutes) to `~$0.246/run` (`45` multiplier minutes — an **88.0%** reduction), with zero cron schedules permitted.

---

## 2. Build Host Prerequisites & Packaging Blocker Mitigations (`IR-002` / `IR-003` / `IR-004`)

### 2.1. Required Native macOS Toolchains
- **Operating System:** macOS 15 Sequoia (`macos-15` / `macos-15-intel`) or macOS 14 Sonoma build host; deployment floor set to `minimumSystemVersion: "11.0"` via CI overlay and `IR-003`.
- **Xcode Command Line Tools:** Provides `clang`, `xcrun`, `codesign`, `lipo`, `otool`, `plutil`, `hdiutil`, `sips`, and `iconutil`:
  ```bash
  xcode-select --install
  sw_vers && uname -m && xcrun --show-sdk-path
  ```
- **Rust Toolchain (`>= 1.90`, `--locked` enforced):**
  - [`apps/desktop/src-tauri/Cargo.toml`](../../apps/desktop/src-tauri/Cargo.toml) enforces `rust-version = "1.90"` (required by locked `tauri v2.12.6`, `tauri-build v2.7.5`, `wry v0.57.2`, `tao v0.37.2`).
  - Workspace [`Cargo.toml`](../../Cargo.toml) alignment (`1.85` -> `1.90`) is delivered in atomic patch [`0001-IR-001-align-workspace-msrv-1.90.patch`](../../plans/reports/build/ir-patches/0001-IR-001-align-workspace-msrv-1.90.patch).
- **Node.js `22` LTS:** Runs the zero-dependency `node:*` scripts in [`scripts/ci-cross-platform/`](../../scripts/ci-cross-platform/) and installs the exact pinned Tauri CLI.
- **Pinned Tauri CLI (`@tauri-apps/cli@2.12.5`):** Never use floating `@latest` or unpinned CLI installs (`IR-004`):
  ```bash
  npm install --global "@tauri-apps/cli@2.12.5"
  tauri --version
  ```
- **`hdiutil`:** Built-in `/usr/bin/hdiutil` utility used by Tauri's DMG bundler to package `.dmg` images.

### 2.2. Non-Destructive CI Overlay & Atomic Patches for `IR-002` & `IR-003`
Because shared `apps/desktop/src-tauri/tauri.conf.json` and `apps/desktop/src-tauri/icons/*` are kept untouched in `build/cross-platform-distribution` to preserve clean sibling worktree isolation, macOS icon and bundle metadata gaps are solved at two layers:

1. **Active Build-Time Mitigation (`tauri-ci-bundle-overlay.json`):**
   - [`scripts/ci-cross-platform/assets/icon.icns`](../../scripts/ci-cross-platform/assets/icon.icns) (`1,142` bytes, valid Apple Icon Image `0x69636e73` header containing `ic07` `128x128` PNG and `ic08` `256x256` PNG chunks, verified by `inspect-artifact.mjs` and `selftest.mjs` `16/16 PASS`) and [`scripts/ci-cross-platform/assets/128x128@2x.png`](../../scripts/ci-cross-platform/assets/128x128@2x.png) (`716` bytes, `256x256` 8-bit RGBA) provide valid original Multi-Zalo Desktop icons (zero VNG branding).
   - [`scripts/ci-cross-platform/tauri-ci-bundle-overlay.json`](../../scripts/ci-cross-platform/tauri-ci-bundle-overlay.json) injects both icons plus `"macOS": { "minimumSystemVersion": "11.0" }` and `"category": "Utility"` automatically when `build-and-package.mjs` invokes `tauri build --config scripts/ci-cross-platform/tauri-ci-bundle-overlay.json -- --locked`.
2. **Upstream Atomic Patches (`git apply --check` PASS on `main`):**
   - [`0002-IR-002-macos-icns-and-retina-icons.patch`](../../plans/reports/build/ir-patches/0002-IR-002-macos-icns-and-retina-icons.patch) installs `icon.icns` and `128x128@2x.png` directly into `apps/desktop/src-tauri/icons/` and updates `tauri.conf.json` `bundle.icon`.
   - [`0003-IR-003-tauri-os-bundle-metadata.patch`](../../plans/reports/build/ir-patches/0003-IR-003-tauri-os-bundle-metadata.patch) adds explicit `bundle.macOS` (`minimumSystemVersion: "11.0"`, `hardenedRuntime: true`, `signingIdentity: null`, `entitlements: null`) to `tauri.conf.json`.

---

## 3. Native 7-Stage Build, Runtime Fixture & Packaging Pipeline

Every native macOS build (`aarch64-apple-darwin` on Apple Silicon; `x86_64-apple-darwin` on Intel Mac) runs all 7 verification stages with `--locked` enforced:

### 3.1. Apple Silicon Native Pipeline (`macos-15`, `aarch64-apple-darwin`)
```bash
# Stage 0: Fail-closed native Apple Silicon hardware & toolchain check
test "$(uname -m)" = "arm64"
rustc -vV | grep -q '^host: aarch64-apple-darwin$'

# Stage 1: Check Xcode CLT, hdiutil, codesign, lipo, Rust >= 1.90, and passive signing state
node desktop-instance-manager/scripts/ci-cross-platform/check-prerequisites.mjs \
  --target aarch64-apple-darwin \
  --out /tmp/dim-macos-arm64/prereq.json

# Stage 2: Clippy (-D warnings, --locked) across workspace & Tauri crate
cargo clippy --manifest-path desktop-instance-manager/Cargo.toml --workspace --all-targets --target aarch64-apple-darwin --locked -- -D warnings
cargo clippy --manifest-path desktop-instance-manager/apps/desktop/src-tauri/Cargo.toml --target aarch64-apple-darwin --locked -- -D warnings

# Stages 3–6: Locked unit tests, release compile, native runtime fixture smoke, and .app + .dmg bundle via overlay
node desktop-instance-manager/scripts/ci-cross-platform/build-and-package.mjs \
  --target aarch64-apple-darwin \
  --bundles all \
  --out /tmp/dim-macos-arm64/build-summary.json

# Verify native fixture + dim-cli smoke, Mach-O arch, Info.plist XML, and DMG container
TMP_FIX="$(mktemp -d)"
HOME="${TMP_FIX}" node desktop-instance-manager/fixtures/test-desktop-app/fixture-app.mjs --stay-ms=100
HOME="${TMP_FIX}" desktop-instance-manager/target/aarch64-apple-darwin/release/dim-cli --data-dir "${TMP_FIX}/dim-data" status --json
rm -rf "${TMP_FIX}"
test "$(lipo -archs desktop-instance-manager/apps/desktop/src-tauri/target/aarch64-apple-darwin/release/dim-desktop)" = "arm64"
plutil -lint "desktop-instance-manager/apps/desktop/src-tauri/target/aarch64-apple-darwin/release/bundle/macos/Multi-Zalo Desktop.app/Contents/Info.plist"
hdiutil imageinfo desktop-instance-manager/apps/desktop/src-tauri/target/aarch64-apple-darwin/release/bundle/dmg/*.dmg >/dev/null

# Stage 7: Inspect Mach-O/.app/.dmg and emit Section 7 manifest + SHA256SUMS.txt
node desktop-instance-manager/scripts/ci-cross-platform/generate-artifact-manifest.mjs \
  --scan-default-targets \
  --target aarch64-apple-darwin \
  --build-summary /tmp/dim-macos-arm64/build-summary.json \
  --os-label macos-15 \
  --arch arm64 \
  --build-log-url "local-dev" \
  --out-manifest /tmp/dim-macos-arm64/artifact-manifest.json \
  --out-sha256 /tmp/dim-macos-arm64/SHA256SUMS.txt \
  --out-md /tmp/dim-macos-arm64/artifact-summary.md
```

### 3.2. Intel Mac Native Pipeline (`macos-15-intel`, `x86_64-apple-darwin`)
Run **only on native Intel `x86_64` macOS hardware** (never on an `arm64` host):
```bash
# Stage 0: Fail-closed native Intel x86_64 hardware & toolchain check
test "$(uname -m)" = "x86_64"
rustc -vV | grep -q '^host: x86_64-apple-darwin$'

node desktop-instance-manager/scripts/ci-cross-platform/check-prerequisites.mjs \
  --target x86_64-apple-darwin \
  --out /tmp/dim-macos-intel/prereq.json

node desktop-instance-manager/scripts/ci-cross-platform/build-and-package.mjs \
  --target x86_64-apple-darwin \
  --bundles all \
  --out /tmp/dim-macos-intel/build-summary.json

test "$(lipo -archs desktop-instance-manager/apps/desktop/src-tauri/target/x86_64-apple-darwin/release/dim-desktop)" = "x86_64"

node desktop-instance-manager/scripts/ci-cross-platform/generate-artifact-manifest.mjs \
  --scan-default-targets \
  --target x86_64-apple-darwin \
  --build-summary /tmp/dim-macos-intel/build-summary.json \
  --os-label macos-15-intel \
  --arch x86_64 \
  --build-log-url "local-dev" \
  --out-manifest /tmp/dim-macos-intel/artifact-manifest.json \
  --out-sha256 /tmp/dim-macos-intel/SHA256SUMS.txt \
  --out-md /tmp/dim-macos-intel/artifact-summary.md
```

---

## 4. Artifact Classification & Bundle Structure

Raw Mach-O executables and packaged macOS bundles have distinct roles and **must never be conflated**:

| Artifact Path (relative to `desktop-instance-manager/`) | `artifact_kind` (`inspect-artifact.mjs`) | `is_installer` | Format & Contents |
| :--- | :--- | :---: | :--- |
| `apps/desktop/src-tauri/target/<triple>/release/dim-desktop` | `compiled-executable` | **`false`** | Raw 64-bit Mach-O executable (`0xfeedfacf`). **Not an installer or `.app` bundle.** |
| `target/<triple>/release/dim-cli` | `compiled-executable` | **`false`** | Native CLI parity runner (`dim-test-fixture` crate). **Not an installer.** |
| `target/<triple>/release/dim-test-fixture` | `compiled-executable` | **`false`** | Synthetic desktop fixture for automated isolation testing. **Not an installer.** |
| `apps/desktop/src-tauri/target/<triple>/release/bundle/macos/Multi-Zalo Desktop.app` | `macos-app-bundle` | **`false`** (application bundle directory) | Standard macOS `.app` bundle (`Contents/Info.plist`, `Contents/MacOS/dim-desktop`, `Contents/Resources/icon.icns`). |
| `apps/desktop/src-tauri/target/<triple>/release/bundle/dmg/Multi-Zalo Desktop_0.1.0_<arch>.dmg` | `macos-dmg-installer` | **`true`** | Compressed UDZO Apple Disk Image (`koly` trailer) containing `Multi-Zalo Desktop.app` and `/Applications` drop symlink. |

### `Multi-Zalo Desktop.app` Internal Structure
```text
Multi-Zalo Desktop.app/
└── Contents/
    ├── Info.plist                 # CFBundleIdentifier: com.multizalodesktop.manager
    │                              # CFBundleName: Multi-Zalo Desktop
    │                              # CFBundleShortVersionString: 0.1.0
    │                              # CFBundleExecutable: dim-desktop
    │                              # LSMinimumSystemVersion: 11.0 (via overlay / IR-003)
    │                              # NSHighResolutionCapable: true
    ├── MacOS/
    │   └── dim-desktop            # Native Mach-O 64-bit executable (arm64 or x86_64)
    └── Resources/
        └── icon.icns              # Supplied via tauri-ci-bundle-overlay.json (or 0002-IR-002 patch)
```

---

## 5. Code Signing, Notarization, Gatekeeper & 7-Pillar Supply-Chain Posture

### 5.1. Current Signing & Supply-Chain Posture (`UNSIGNED_DEVELOPMENT_ARTIFACT`)
> [!WARNING]
> **No Apple Signing Credentials Configured — Strictly Development/Test Artifacts**
>
> - Zero Apple Developer certificates (`.p12`), provisioning profiles, or App Store Connect API keys (`.p8`) are stored, configured, or requested in this repository or in `.github/workflows/cross-platform-validation.yml`.
> - Across our 7-pillar supply-chain model (see [`dependency-audit-summary.md`](../../plans/reports/build/dependency-audit-summary.md)):
>   1. **Lockfile Checksums:** `431/431` `crates.io-index` SHA-256 digests verified (`--locked`).
>   2. **License Policy:** `439` unique crates verified against `deny.toml` (`0` SPDX violations, `0` banned OpenSSL crates).
>   3. **Vulnerability Advisory Scan:** `VERIFIED_CARGO_DENY_RUSTSEC_CLEAN` (`0` vulnerabilities via real `cargo-deny 0.21.0` against `rustsec/advisory-db` @ `233155b4025b854c68a4b199c0e32d7fd53291a2`).
>   4. **SBOM:** Declared dependency inventory only — **not** independent build provenance.
>   5. **Build Provenance:** Local builds are `SELF_REPORTED_UNATTESTED` (no OIDC SLSA attestation until remote CI runs).
>   6. **Artifact Signature:** **`UNSIGNED_DEVELOPMENT_ARTIFACT`** (`notarization_status: "NOT_NOTARIZED"`). Note that Xcode's linker emits an ad-hoc `linker-signed` signature (`flags=0x2(adhoc)`) on `arm64` Mach-O pages solely for local kernel execution; ad-hoc signing carries **zero** publisher identity or Gatekeeper trust.
>   7. **Installer Integrity:** SHA-256 checksums only (`SHA256SUMS.txt`); clean macOS hosts attach `com.apple.quarantine` to browser downloads and **Gatekeeper will block execution**. Never represent unsigned `.app`/`.dmg` files as production releases, and never advise end-users to disable Gatekeeper (`sudo spctl --master-disable`).

### 5.2. Production Release Requirements (Gate G4 — `signing-and-release-gates.md`)
Before promoting any macOS `.app` or `.dmg` to end-users (Gate G4 in [`signing-and-release-gates.md`](./signing-and-release-gates.md)), a human-approved release pipeline with an enrolled **Apple Developer Program** account must execute:

1. **Developer ID Application Signing & Hardened Runtime (`--options runtime`):**
   ```bash
   codesign --force --options runtime --timestamp \
     --entitlements apps/desktop/src-tauri/entitlements.plist \
     --sign "Developer ID Application: <Org Name> (<TeamID>)" \
     "Multi-Zalo Desktop.app/Contents/MacOS/dim-desktop"

   codesign --force --options runtime --timestamp \
     --entitlements apps/desktop/src-tauri/entitlements.plist \
     --sign "Developer ID Application: <Org Name> (<TeamID>)" \
     "Multi-Zalo Desktop.app"
   ```
   *(Keep entitlements minimal — never enable `disable-library-validation`, `allow-unsigned-executable-memory`, or `allow-dyld-environment-variables`. Include `com.apple.security.automation.apple-events` + `NSAppleEventsUsageDescription` only if `osascript` `System Events` window-focus automation is retained.)*
2. **DMG Container Signing, `notarytool` Submission & `stapler` Ticket Stapling:**
   ```bash
   codesign --force --timestamp \
     --sign "Developer ID Application: <Org Name> (<TeamID>)" \
     "Multi-Zalo Desktop_0.1.0_aarch64.dmg"

   xcrun notarytool submit "Multi-Zalo Desktop_0.1.0_aarch64.dmg" \
     --key "$APPLE_API_KEY_PATH" \
     --key-id "$APPLE_API_KEY_ID" \
     --issuer "$APPLE_API_ISSUER_ID" \
     --wait

   xcrun stapler staple "Multi-Zalo Desktop_0.1.0_aarch64.dmg"
   xcrun stapler validate "Multi-Zalo Desktop_0.1.0_aarch64.dmg"
   ```
3. **Gatekeeper Verification (`codesign` + `spctl`):**
   ```bash
   codesign --verify --deep --strict --verbose=2 "Multi-Zalo Desktop.app"
   spctl --assess --type execute --verbose=4 "Multi-Zalo Desktop.app"
   spctl --assess --type open --context context:primary-signature --verbose=4 "Multi-Zalo Desktop_0.1.0_aarch64.dmg"
   ```

---

## 6. Runtime Capability & Honesty Audit (`crates/platform-macos`)

Even after `Multi-Zalo Desktop.app` is compiled, signed, and notarized, **packaging the manager shell does not verify Zalo multi-instance isolation on macOS**. Direct inspection of [`crates/platform-macos/src/adapter.rs`](../../crates/platform-macos/src/adapter.rs), [`crates/platform-macos/src/layout.rs`](../../crates/platform-macos/src/layout.rs), and [`docs/compatibility/os-readiness.md`](../compatibility/os-readiness.md) defines what is implemented vs unverified:

| Subsystem | Code Implementation (`crates/platform-macos`) | Runtime Verification Status | Technical Caveat / Gap |
| :--- | :--- | :---: | :--- |
| **Capability Reporting** | `MacOsPlatformAdapter::capabilities()` returns `overall_status: NotTested`, `official_zalo_status: NotTested`, `fixture_status: Experimental`, `concurrent_calls_status: Blocked`. | **Unit-tested (`3/3` PASS)** | Honest self-reporting enforced in code; never claims `SupportedAndTested`. |
| **Zalo.app Discovery** | Checks `/Applications/Zalo.app/Contents/MacOS/Zalo`, `Contents/Resources/app.asar`, and parses `CFBundleShortVersionString` from `Contents/Info.plist`. | **`NOT_TESTED` on Mac** | Reports `signature_status: "NotVerifiedOnHost (use codesign -dv on macOS)"`; native `SecStaticCodeCheckValidity` publisher pinning is not yet implemented in Rust. |
| **Isolated Profile Layout** | Creates `<profile_dir>/Library/Application Support/ZaloData/.zalo-profile-id`, `tmp/`, `Library/`, and symlinks host `Library/Keychains`, `Downloads`, `Desktop`, `Documents`, `Pictures`, `Movies`, `Music`. | **Layout unit-tested; runtime `NOT_TESTED`** | Symlinked `Library/Keychains` `Safe Storage` behavior across 2+ simultaneous Zalo instances has never been tested on physical Mac hardware. |
| **Multi-Instance Launch** | Spawns `/usr/bin/open -n -a /Applications/Zalo.app --env CFFIXED_USER_HOME=<profile_dir> --env HOME=<profile_dir> --env TMPDIR=<profile_dir>/tmp`. | **`NOT_TESTED` on Mac** | Must verify on physical macOS 14/15 hardware that LaunchServices forwards `CFFIXED_USER_HOME` to VNG's hardened/notarized `Zalo.app`. |
| **Process & Telemetry Inspection** | Parses Chromium `SingletonLock` symlink (`<hostname>-<pid>`) + `libc::kill(pid, 0)` liveness probe; protects primary PID. | **Parser unit-tested; runtime `NOT_TESTED`** | `working_set_bytes`, `window_handle`, `window_title`, and `disk_usage_bytes` currently return `0`/`None` placeholders in `inspect_profile`. |
| **Concurrent Voice/Video Calls** | Inspects `/tmp/socketzalorecv2021` and `/tmp/socketzalosend2021`. | **`BLOCKED` (Static ASAR Evidence)** | `ZaloHelper.app/Contents/MacOS/ZaloCall` (`main.js:314800`) runs `fs.unlink("/tmp/socketzalorecv2021")` on `EADDRINUSE`, risking active call hijacking across instances. |

---

## 7. Related Documentation
- [CI Execution & Artifact Validation Report (`ci-execution-and-artifact-validation.md`)](./ci-execution-and-artifact-validation.md)
- [Windows Packaging Guide (`windows.md`)](./windows.md)
- [Ubuntu 24.04 LTS Packaging Guide (`ubuntu.md`)](./ubuntu.md)
- [Cross-Platform Build Matrix (`build-matrix.md`)](./build-matrix.md)
- [Code Signing & Release Gates (`signing-and-release-gates.md`)](./signing-and-release-gates.md)
- [Release Readiness Scorecard (`release-readiness.md`)](./release-readiness.md)
- [Out-of-Scope Integration Requests & Atomic Patches (`integration-requests.md`)](../../plans/reports/build/integration-requests.md)
- [OS Readiness & Compatibility Matrix (`os-readiness.md`)](../compatibility/os-readiness.md)
