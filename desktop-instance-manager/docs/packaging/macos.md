# macOS Packaging & Distribution Guide (`dim-desktop`)

**Document Scope:** Build, packaging, code-signing/notarization requirements, and runtime honesty boundaries for **Multi-Zalo Desktop — Instance Manager (`dim-desktop`)** on Apple macOS.  
**Target Architectures:** `aarch64-apple-darwin` (Apple Silicon M-series) and `x86_64-apple-darwin` (Intel Mac).  
**Current Artifact Signing State:** `UNSIGNED_DEVELOPMENT_ARTIFACT` (`NOT_NOTARIZED`) — **strictly for development/CI validation, NEVER a production release**.  
**Zalo macOS Runtime Support Status:** **`NOT_TESTED`** (`SupportStatus::NotTested` in [`crates/platform-macos/src/adapter.rs`](../../crates/platform-macos/src/adapter.rs)).

---

> [!IMPORTANT]
> **CRITICAL HONESTY GATE — Manager Shell Build != Zalo macOS Runtime Verification**
>
> Compiling `dim-desktop` and packaging `Multi-Zalo Desktop.app` or `.dmg` on macOS **ONLY** proves that the Rust workspace, `dim-platform-macos` adapter code, and Tauri 2 (`WKWebView`) desktop manager shell compile and bundle cleanly.
> - It is **NOT** proof that official VNG `/Applications/Zalo.app` multi-instance isolation works at runtime on physical macOS hardware (`SupportStatus::NotTested`).
> - Live GitHub Actions macOS workflow execution has **NOT** been run in this repository audit (`CI_LIVE_RUN_STATUS: NOT_EXECUTED / STATICALLY_VALIDATED_ONLY`); macOS build definitions are statically validated only.
> - **Zero VNG Binary Policy:** Neither `Multi-Zalo Desktop.app` nor any `.dmg` artifact embeds, copies, modifies, or redistributes VNG's `Zalo.app`, `ZaloHelper.app`, or `app.asar`. Users must install official Zalo for macOS directly from VNG into `/Applications/Zalo.app`.

---

## 1. Supported macOS Build Targets & Runner Strategy

| Target Triple | CPU Architecture | Primary Build Method | GitHub Actions Runner (`cross-platform-validation.yml`) | Runtime Test Execution Policy |
| :--- | :--- | :--- | :--- | :--- |
| **`aarch64-apple-darwin`** | Apple Silicon (`arm64` — M1/M2/M3/M4) | Native compilation on Apple Silicon host | `macos-14` / `macos-15` (opt-in `enable_macos_arm64`, default `false`) | Native execution of `cargo test` (`aarch64` host == `aarch64` target) |
| **`x86_64-apple-darwin`** | Intel Mac (`x86_64`) | Native Intel build **or** cross-compilation via `rustup target add x86_64-apple-darwin` against Xcode macOS Universal SDK | `macos-14` / `macos-15` with `x86_64-apple-darwin` target (opt-in `enable_macos_intel`, default `false`) | Native on `x86_64` host; on `arm64` runners without Rosetta 2, `build-and-package.mjs` falls back to `cargo build --tests` compilation verification |

### Why Cross-Compile `x86_64-apple-darwin` on `macos-14` / `macos-15`?
1. **Xcode Universal SDK Support:** Apple's Xcode 15/16 Command Line Tools on macOS 14 (Sonoma) and macOS 15 (Sequoia) ship complete fat (`arm64` + `x86_64`) system frameworks (`AppKit`, `WebKit`, `CoreFoundation`, `Security`) and `clang` cross-arch support, allowing deterministic `x86_64-apple-darwin` builds from an Apple Silicon runner without relying on deprecated/retired `macos-13` Intel runner queues.
2. **Discrete Architecture Provenance:** Building explicit `aarch64-apple-darwin` and `x86_64-apple-darwin` targets produces architecture-specific Mach-O binaries (`0xfeedfacf`) with independent SHA-256 digests in `artifact-validation-manifest.json`. (An optional fat `universal-apple-darwin` bundle via `lipo` can be enabled post-Gate G4 once Developer ID signing is provisioned.)
3. **Private-Repository Billing Guard:** GitHub Actions applies a **10x minute multiplier** to macOS runners on private repositories. Consequently, `.github/workflows/cross-platform-validation.yml` uses `workflow_dispatch` only (zero cron schedules) and defaults both `enable_macos_arm64` and `enable_macos_intel` to `false` unless explicitly toggled by a maintainer.

---

## 2. Build Host Prerequisites

### 2.1. Required Toolchain & System Utilities
- **Operating System:** macOS 14 Sonoma (`14.x`) or macOS 15 Sequoia (`15.x`) recommended for build hosts; minimum deployment target should be pinned explicitly in `tauri.conf.json` (`bundle.macOS.minimumSystemVersion`, e.g., `"13.0"` — see **IR-003** in [`integration-requests.md`](../../plans/reports/build/integration-requests.md)).
- **Xcode Command Line Tools:** Provides `clang`, `xcrun`, `codesign`, `lipo`, `otool`, `plutil`, `sips`, `iconutil`, and the macOS SDK:
  ```bash
  xcode-select --install
  xcrun --show-sdk-path
  ```
- **Rust Toolchain (`>= 1.90`):**
  - [`apps/desktop/src-tauri/Cargo.toml`](../../apps/desktop/src-tauri/Cargo.toml) declares `rust-version = "1.90"` (required by locked `tauri v2.12.6`, `tauri-build v2.7.5`, `wry v0.57.2`, `tao v0.37.2`).
  - Note: [`desktop-instance-manager/Cargo.toml`](../../Cargo.toml) declares `rust-version = "1.85"`; the effective MSRV for full desktop packaging is **`1.90`** (tracked as **IR-001** in [`integration-requests.md`](../../plans/reports/build/integration-requests.md)).
  ```bash
  rustup toolchain install stable --profile minimal --component rustfmt clippy
  rustup target add aarch64-apple-darwin x86_64-apple-darwin
  ```
- **Node.js `>= 22` LTS:** Executes the zero-dependency ESM build, inspection, and manifest scripts under [`scripts/ci-cross-platform/`](../../scripts/ci-cross-platform/).
- **Tauri CLI v2 (`cargo-tauri`):** Required to assemble `.app` and `.dmg` bundles (`cargo install tauri-cli@^2.0.0 --locked`; pinning tracked in **IR-004**).
- **`hdiutil`:** Standard macOS disk image utility (`/usr/bin/hdiutil`), invoked by Tauri's DMG bundler to build compressed UDZO `.dmg` images.

### 2.2. Known Out-of-Scope Repository Blockers (**IR-002** & **IR-003**)
Because `tauri.conf.json` and `apps/desktop/src-tauri/icons/` are frozen against modification in this packaging audit, two configuration gaps must be resolved via [`integration-requests.md`](../../plans/reports/build/integration-requests.md) before clean production macOS bundling:

1. **IR-002 — Missing `icons/icon.icns` and Retina PNGs:**
   - Current [`apps/desktop/src-tauri/tauri.conf.json`](../../apps/desktop/src-tauri/tauri.conf.json) specifies:
     ```json
     "icon": [
       "icons/32x32.png",
       "icons/128x128.png",
       "icons/icon.ico"
     ]
     ```
   - `apps/desktop/src-tauri/icons/` currently lacks **`icon.icns`** (and `128x128@2x.png`). On macOS, `cargo tauri build` expects an Apple Icon Image (`icon.icns`) to populate `Multi-Zalo Desktop.app/Contents/Resources/icon.icns` and `CFBundleIconFile` in `Info.plist`. Without `icon.icns`, Tauri's macOS bundler either fails icon resolution (`FAILED_BUNDLER` recorded cleanly by `build-and-package.mjs` while preserving the compiled Mach-O `dim-desktop` binary) or produces an app bundle without a valid macOS Dock icon.
   - **Remediation Command (on macOS when landing IR-002):**
     ```bash
     # Starting from a 1024x1024 master PNG:
     mkdir -p /tmp/AppIcon.iconset
     sips -z 16 16     master-1024.png --out /tmp/AppIcon.iconset/icon_16x16.png
     sips -z 32 32     master-1024.png --out /tmp/AppIcon.iconset/icon_16x16@2x.png
     sips -z 32 32     master-1024.png --out /tmp/AppIcon.iconset/icon_32x32.png
     sips -z 64 64     master-1024.png --out /tmp/AppIcon.iconset/icon_32x32@2x.png
     sips -z 128 128   master-1024.png --out /tmp/AppIcon.iconset/icon_128x128.png
     sips -z 256 256   master-1024.png --out /tmp/AppIcon.iconset/icon_128x128@2x.png
     sips -z 256 256   master-1024.png --out /tmp/AppIcon.iconset/icon_256x256.png
     sips -z 512 512   master-1024.png --out /tmp/AppIcon.iconset/icon_256x256@2x.png
     sips -z 512 512   master-1024.png --out /tmp/AppIcon.iconset/icon_512x512.png
     cp master-1024.png /tmp/AppIcon.iconset/icon_512x512@2x.png
     iconutil -c icns /tmp/AppIcon.iconset -o apps/desktop/src-tauri/icons/icon.icns
     ```
2. **IR-003 — Missing Explicit `bundle.macOS` Metadata in `tauri.conf.json`:**
   - Add `bundle.macOS.minimumSystemVersion` (e.g., `"13.0"`), `bundle.macOS.signingIdentity: null` (defaulting safely to unsigned dev builds unless overridden via CI env), and `bundle.macOS.entitlements`.

---

## 3. Reproducible Build, Test & Packaging Commands

Run all commands from the repository root (or adjust paths relative to `desktop-instance-manager/`):

### 3.1. Standardized CI/Local Pipeline (`scripts/ci-cross-platform/*.mjs`)

#### Apple Silicon (`aarch64-apple-darwin`)
```bash
# 1. Verify Xcode CLT, clang, xcrun, hdiutil, codesign, lipo, Rust >= 1.90, and target triple
node desktop-instance-manager/scripts/ci-cross-platform/check-prerequisites.mjs \
  --target aarch64-apple-darwin \
  --out /tmp/dim-macos-arm64/prereq.json

# 2. Audit both Cargo.lock files against deny.toml (SPDX licenses, banned openssl, zero VNG binaries)
node desktop-instance-manager/scripts/ci-cross-platform/dependency-audit.mjs \
  --out-json /tmp/dim-macos-arm64/dependency-audit.json \
  --out-md /tmp/dim-macos-arm64/dependency-audit.md

# 3. Run unit tests, compile release Mach-O binaries, and attempt .app + .dmg bundling
node desktop-instance-manager/scripts/ci-cross-platform/build-and-package.mjs \
  --target aarch64-apple-darwin \
  --bundles all \
  --out /tmp/dim-macos-arm64/build-summary.json

# 4. Inspect artifacts and emit Section 7 traceable manifest + SHA256SUMS.txt
node desktop-instance-manager/scripts/ci-cross-platform/generate-artifact-manifest.mjs \
  --scan-default-targets \
  --target aarch64-apple-darwin \
  --build-summary /tmp/dim-macos-arm64/build-summary.json \
  --os-label macos-14 \
  --arch arm64 \
  --build-log-url "local-dev" \
  --out-manifest /tmp/dim-macos-arm64/artifact-manifest.json \
  --out-sha256 /tmp/dim-macos-arm64/SHA256SUMS.txt \
  --out-md /tmp/dim-macos-arm64/artifact-summary.md
```

#### Intel Mac (`x86_64-apple-darwin`)
```bash
rustup target add x86_64-apple-darwin

node desktop-instance-manager/scripts/ci-cross-platform/check-prerequisites.mjs \
  --target x86_64-apple-darwin \
  --out /tmp/dim-macos-x64/prereq.json

node desktop-instance-manager/scripts/ci-cross-platform/build-and-package.mjs \
  --target x86_64-apple-darwin \
  --bundles all \
  --out /tmp/dim-macos-x64/build-summary.json

node desktop-instance-manager/scripts/ci-cross-platform/generate-artifact-manifest.mjs \
  --scan-default-targets \
  --target x86_64-apple-darwin \
  --build-summary /tmp/dim-macos-x64/build-summary.json \
  --os-label macos-14 \
  --arch x86_64 \
  --build-log-url "local-dev" \
  --out-manifest /tmp/dim-macos-x64/artifact-manifest.json \
  --out-sha256 /tmp/dim-macos-x64/SHA256SUMS.txt \
  --out-md /tmp/dim-macos-x64/artifact-summary.md
```

### 3.2. Direct Cargo & Tauri Commands (Reference)
```bash
cd desktop-instance-manager

# Locked workspace unit tests & release CLI/fixture build
cargo test --workspace --all-targets --target aarch64-apple-darwin --locked
cargo build --workspace --release --target aarch64-apple-darwin --locked

# Locked Tauri shell check, test, and release binary build
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --target aarch64-apple-darwin --locked
cargo build --manifest-path apps/desktop/src-tauri/Cargo.toml --release --target aarch64-apple-darwin --locked

# Optional .app + .dmg bundle build (requires cargo-tauri v2 and IR-002 icon.icns resolution)
cargo tauri build --target aarch64-apple-darwin --bundles app,dmg -- --locked
```

---

## 4. Artifact Classification & Bundle Layout

Raw Mach-O executables and packaged macOS bundles have distinct roles and **must never be conflated**:

| Artifact Path (relative to `desktop-instance-manager/`) | `artifact_kind` (`inspect-artifact.mjs`) | `is_installer` | Format & Contents |
| :--- | :--- | :---: | :--- |
| `apps/desktop/src-tauri/target/<triple>/release/dim-desktop` | `compiled-executable` | **`false`** | Raw 64-bit Mach-O executable (`0xfeedfacf`). **Not an installer or `.app` bundle.** |
| `target/<triple>/release/dim-cli` | `compiled-executable` | **`false`** | Native CLI parity runner (`dim-test-fixture` crate). **Not an installer.** |
| `target/<triple>/release/dim-test-fixture` | `compiled-executable` | **`false`** | Synthetic desktop fixture for automated isolation testing. **Not an installer.** |
| `apps/desktop/src-tauri/target/<triple>/release/bundle/macos/Multi-Zalo Desktop.app` | `macos-app-bundle` | **`false`** (application bundle directory) | Standard macOS application bundle directory (`Contents/Info.plist`, `Contents/MacOS/dim-desktop`, `Contents/Resources/`). |
| `apps/desktop/src-tauri/target/<triple>/release/bundle/dmg/Multi-Zalo Desktop_0.1.0_<arch>.dmg` | `macos-dmg-installer` | **`true`** | Compressed Apple Disk Image (`.dmg`) containing `Multi-Zalo Desktop.app` and an `/Applications` drag-and-drop symlink. |

### `Multi-Zalo Desktop.app` Internal Structure
```text
Multi-Zalo Desktop.app/
└── Contents/
    ├── Info.plist                 # CFBundleIdentifier: com.multizalodesktop.manager
    │                              # CFBundleName: Multi-Zalo Desktop
    │                              # CFBundleShortVersionString: 0.1.0
    │                              # CFBundleExecutable: dim-desktop
    │                              # NSHighResolutionCapable: true
    ├── MacOS/
    │   └── dim-desktop            # Mach-O 64-bit executable (arm64 or x86_64)
    └── Resources/
        └── icon.icns              # Requires IR-002 remediation in tauri.conf.json
```

### Binary & Bundle Verification Checks
```bash
# Verify Mach-O architecture and 64-bit magic
file "apps/desktop/src-tauri/target/aarch64-apple-darwin/release/dim-desktop"
lipo -archs "apps/desktop/src-tauri/target/aarch64-apple-darwin/release/bundle/macos/Multi-Zalo Desktop.app/Contents/MacOS/dim-desktop"

# Validate Info.plist XML syntax and bundle identifier
plutil -lint "apps/desktop/src-tauri/target/aarch64-apple-darwin/release/bundle/macos/Multi-Zalo Desktop.app/Contents/Info.plist"
plutil -p "apps/desktop/src-tauri/target/aarch64-apple-darwin/release/bundle/macos/Multi-Zalo Desktop.app/Contents/Info.plist" | grep -E "CFBundleIdentifier|CFBundleShortVersionString|CFBundleExecutable"

# Run automated header & plist inspector
node scripts/ci-cross-platform/inspect-artifact.mjs \
  --file "apps/desktop/src-tauri/target/aarch64-apple-darwin/release/dim-desktop"
```

---

## 5. Code Signing, Hardened Runtime, Notarization & Gatekeeper

### 5.1. Current Repository & CI Signing Status (`UNSIGNED_DEVELOPMENT_ARTIFACT`)
> [!WARNING]
> **No Apple Signing Credentials Configured**
>
> - Zero Apple Developer certificates (`.p12`), provisioning profiles, or App Store Connect API keys (`.p8`) are stored, configured, or requested in this repository or in `.github/workflows/cross-platform-validation.yml`.
> - Every macOS executable, `.app` bundle, and `.dmg` produced by local developer builds or CI is strictly marked:
>   - `signing_status`: **`"UNSIGNED_DEVELOPMENT_ARTIFACT"`** (Note: on `aarch64-apple-darwin`, Xcode's linker emits an ad-hoc `linker-signed` signature `flags=0x2(adhoc)` because Apple Silicon kernels refuse to exec completely unsigned arm64 pages; ad-hoc signing provides **zero** publisher identity or Gatekeeper trust and is still classified as `UNSIGNED_DEVELOPMENT_ARTIFACT`.)
>   - `notarization_status`: **`"NOT_NOTARIZED"`**
> - **Gatekeeper Quarantine Impact:** When downloaded over HTTPS on a clean macOS machine, browsers attach the `com.apple.quarantine` extended attribute. macOS Gatekeeper (`syspolicyd`) will **block** `Multi-Zalo Desktop.app` ("Apple could not verify 'Multi-Zalo Desktop' is free of malware that may harm your Mac or compromise your privacy").
> - **Strict Policy:** Unsigned/unnotarized macOS bundles are strictly development/CI test artifacts — **NEVER** label them as production releases, and **NEVER** instruct end-users to disable Gatekeeper (`sudo spctl --master-disable`).

### 5.2. Production Release Requirements (Gate G4 — `signing-and-release-gates.md`)
Before promoting any macOS `.app` or `.dmg` beyond development testing (see Gate G4 in [`signing-and-release-gates.md`](./signing-and-release-gates.md)), a human-approved release pipeline with an enrolled **Apple Developer Program** account must execute all five steps below:

1. **Developer ID Application Certificate & Hardened Runtime (`--options runtime`):**
   - Sign inside-out using a valid `Developer ID Application: <Organization> (<TeamID>)` certificate in an ephemeral keychain, enabling Hardened Runtime (`--options runtime`) and secure Apple RFC 3161 timestamping (`--timestamp`):
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
2. **Least-Privilege Entitlements (`entitlements.plist`):**
   - `dim-desktop` uses macOS native `WKWebView` (out-of-process XPC rendering managed by Apple's signed WebKit frameworks) and spawns `/usr/bin/open` and `/usr/bin/osascript` as child processes without injecting dylibs.
   - Do **not** grant dangerous entitlements such as `com.apple.security.cs.disable-library-validation`, `com.apple.security.cs.allow-unsigned-executable-memory`, or `com.apple.security.cs.allow-dyld-environment-variables`.
   - Note on `osascript` focus/quit helpers: `MacOsPlatformAdapter` invokes AppleEvents targeting `System Events` (for window focus) and `NSRunningApplication` (`terminate`). Under Hardened Runtime, if AppleEvents (`NSAppleEventsUsageDescription` in `Info.plist` + `com.apple.security.automation.apple-events` entitlement) are used, macOS TCC prompts the user cleanly for Automation consent.
3. **DMG Packaging & Signing:**
   - Assemble the `.dmg` from the signed `.app`, then sign the `.dmg` container with the same `Developer ID Application` identity:
     ```bash
     codesign --force --timestamp \
       --sign "Developer ID Application: <Org Name> (<TeamID>)" \
       "Multi-Zalo Desktop_0.1.0_aarch64.dmg"
     ```
4. **Apple Notarization & Ticket Stapling (`notarytool` + `stapler`):**
   - Submit the signed `.dmg` to Apple's Notary Service using an App Store Connect API key (`--key`, `--key-id`, `--issuer`) and staple the resulting ticket offline:
     ```bash
     xcrun notarytool submit "Multi-Zalo Desktop_0.1.0_aarch64.dmg" \
       --key "$APPLE_API_KEY_PATH" \
       --key-id "$APPLE_API_KEY_ID" \
       --issuer "$APPLE_API_ISSUER_ID" \
       --wait

     xcrun stapler staple "Multi-Zalo Desktop_0.1.0_aarch64.dmg"
     xcrun stapler validate "Multi-Zalo Desktop_0.1.0_aarch64.dmg"
     ```
5. **Gatekeeper & Signature Verification (`codesign` + `spctl`):**
   ```bash
   codesign --verify --deep --strict --verbose=2 "Multi-Zalo Desktop.app"
   spctl --assess --type execute --verbose=4 "Multi-Zalo Desktop.app"
   spctl --assess --type open --context context:primary-signature --verbose=4 "Multi-Zalo Desktop_0.1.0_aarch64.dmg"
   ```

---

## 6. Runtime Capability & Honesty Audit (`crates/platform-macos`)

Even after `Multi-Zalo Desktop.app` is compiled, signed, and notarized, **packaging the manager shell does not verify Zalo multi-instance isolation on macOS**. A direct code audit of [`crates/platform-macos/src/adapter.rs`](../../crates/platform-macos/src/adapter.rs), [`crates/platform-macos/src/layout.rs`](../../crates/platform-macos/src/layout.rs), and [`docs/compatibility/os-readiness.md`](../compatibility/os-readiness.md) establishes the exact boundary between implemented Rust logic and unverified macOS runtime behavior:

### 6.1. What `dim-platform-macos` Actually Implements vs Stubs

| Subsystem | Code Implementation (`crates/platform-macos`) | Runtime Verification Status | Technical Caveat / Gap |
| :--- | :--- | :---: | :--- |
| **Capability Reporting** | `MacOsPlatformAdapter::capabilities()` explicitly returns `overall_status: NotTested`, `official_zalo_status: NotTested`, `fixture_status: Experimental`, `concurrent_calls_status: Blocked`. | **Unit-tested (`3/3` tests)** | Honest self-reporting enforced in code; never claims `SupportedAndTested`. |
| **Zalo.app Discovery** | Checks `/Applications/Zalo.app/Contents/MacOS/Zalo`, `Contents/Resources/app.asar`, and parses `CFBundleShortVersionString` from `Contents/Info.plist`. | **`NOT_TESTED` on Mac** | Sets `signature_status: "NotVerifiedOnHost (use codesign -dv on macOS)"`. Unlike Windows (`WinVerifyTrust` + `VNG GROUP JSC` RDN pinning), macOS native `SecStaticCodeCheckValidity` FFI is **not yet implemented** in Rust. |
| **Isolated Profile Layout** | Creates `<profile_dir>/Library/Application Support/ZaloData/.zalo-profile-id`, `tmp/`, `Library/`, and symlinks host `Library/Keychains`, `Downloads`, `Desktop`, `Documents`, `Pictures`, `Movies`, `Music`. | **Layout unit-tested; runtime `NOT_TESTED`** | Symlinked `Library/Keychains` behavior across 2+ simultaneous Zalo instances (Keychain `Safe Storage` prompts/locks) has **never been tested on physical macOS hardware**. |
| **Multi-Instance Launch** | Executes `/usr/bin/open -n -a /Applications/Zalo.app --env CFFIXED_USER_HOME=<profile_dir> --env HOME=<profile_dir> --env TMPDIR=<profile_dir>/tmp`. | **`NOT_TESTED` on Mac** | Must verify on macOS 14/15 physical hardware that LaunchServices forwards `CFFIXED_USER_HOME` to VNG's hardened/notarized `Zalo.app` and that Electron's `app.getPath("appData")` resolves inside `<profile_dir>` before `migration.js` runs. |
| **Process & Telemetry Inspection** | Parses Chromium `SingletonLock` symlink (`<hostname>-<pid>`) and probes PID liveness via `libc::kill(pid, 0)`. Protects `~/Library/Application Support/ZaloData/SingletonLock` primary PID. | **Parser unit-tested; runtime `NOT_TESTED`** | In `inspect_profile`, `working_set_bytes: 0`, `window_handle: None`, `window_title: None`, and `disk_usage_bytes: 0` are currently unimplemented placeholders, and PID reuse verification does not yet inspect `NSRunningApplication.bundleIdentifier`. |
| **Focus & Graceful Stop** | Focuses via parameterized `osascript` (`System Events` `unix id`); stops via `.quit-signal` + `osascript` (`NSRunningApplication.terminate()`) + `libc::kill(pid, SIGTERM)`. | **`NOT_TESTED` on Mac** | Requires Automation (`System Events`) TCC permission testing on macOS 14/15. |
| **Concurrent Voice/Video Calls** | Inspects `/tmp/socketzalorecv2021` and `/tmp/socketzalosend2021` via `inspect_call_sockets_at`. | **`BLOCKED` (Static ASAR Evidence)** | `ZaloHelper.app/Contents/MacOS/ZaloCall` (`main.js:314800`) runs `fs.unlink("/tmp/socketzalorecv2021")` on `EADDRINUSE` and re-binds, risking active call hijacking when a second instance initializes. |

### 6.2. Mandatory Exit Criteria Before Upgrading macOS Status from `NOT_TESTED`
Per [`os-readiness.md`](../compatibility/os-readiness.md#35-macos--ubuntu-readiness-gates), macOS Zalo multi-instance support **must remain labeled `NOT_TESTED`** until all four checks below are executed and recorded with live evidence on physical Apple Silicon (`arm64`) and Intel (`x86_64`) Macs:
1. Resolve **IR-002** (`icons/icon.icns`) and **IR-003** (`bundle.macOS` config), and implement macOS `SecStaticCodeCheckValidity` Team ID / publisher verification for `/Applications/Zalo.app` before launch.
2. Empirically confirm `/usr/bin/open -n -a /Applications/Zalo.app --env CFFIXED_USER_HOME=<profile_dir>` isolates `Library/Application Support/ZaloData` with zero writes to `~/Library/Application Support/ZaloData`.
3. Verify macOS Keychain `Safe Storage` encryption (`EnableCookieEncryption`) across 2+ logged-in Zalo accounts without destructive Keychain lockouts or infinite authorization prompts.
4. Measure the exact impact of `/tmp/socketzalorecv2021` / `/tmp/socketzalosend2021` `fs.unlink` collision when a secondary instance launches during an active primary call.

---

## 7. Related Documentation
- [Windows Packaging Guide (`windows.md`)](./windows.md)
- [Ubuntu 24.04 LTS Packaging Guide (`ubuntu.md`)](./ubuntu.md)
- [Cross-Platform Build Matrix (`build-matrix.md`)](./build-matrix.md)
- [Code Signing & Release Gates (`signing-and-release-gates.md`)](./signing-and-release-gates.md)
- [Release Readiness Scorecard (`release-readiness.md`)](./release-readiness.md)
- [Out-of-Scope Integration Requests (`integration-requests.md`)](../../plans/reports/build/integration-requests.md)
- [OS Readiness & Compatibility Matrix (`os-readiness.md`)](../compatibility/os-readiness.md)
