# Ubuntu 24.04 LTS Packaging & Distribution Guide (`dim-desktop`)

**Document Scope:** Build dependencies, packaging formats (`.deb`, `.AppImage`, raw ELF), binary/GLIBC verification, and runtime honesty boundaries for **Multi-Zalo Desktop — Instance Manager (`dim-desktop`)** on Ubuntu Linux.  
**Canonical Baseline:** **Ubuntu 24.04 LTS (Noble Numbat)** — `x86_64-unknown-linux-gnu` (`amd64`).  
**Current Artifact Signing State:** `UNSIGNED_DEVELOPMENT_ARTIFACT` — **strictly for development/CI validation, NEVER a production release**.  
**Official Zalo Linux Support Status:** **`UNSUPPORTED`** (`SupportStatus::Unsupported` — VNG provides no Linux Zalo client); synthetic fixture & user-supplied local AppImages are **`EXPERIMENTAL`** (`SupportStatus::Experimental` in [`crates/platform-linux/src/adapter.rs`](../../crates/platform-linux/src/adapter.rs)).

---

> [!IMPORTANT]
> **CRITICAL HONESTY GATE — Two Completely Separate Realities on Linux**
>
> You must strictly distinguish between **(A)** building the manager application and **(B)** running Zalo Desktop on Linux:
>
> | Dimension | (A) Desktop Instance Manager (`dim-desktop` / `dim-cli`) | (B) Zalo Desktop Client on Linux |
> | :--- | :--- | :--- |
> | **Vendor / Codebase** | Open-source Rust + Tauri 2 (`WKWebView`/WebKit2GTK 4.1) in this repository | Proprietary VNG Corporation client |
> | **Native Linux Build Exists?** | **Yes** — compiles cleanly for `x86_64-unknown-linux-gnu` (`dim-desktop`, `.deb`, `.AppImage`) | **NO** — VNG ships **zero** official Linux binaries, `.deb`, `.rpm`, Flatpak, Snap, or AppImage |
> | **Runtime Status in Code** | Compiles & unit-tests (`dim-platform-linux`); isolates `HOME`/`XDG_*` + optional `bwrap` for fixtures | `LaunchTarget::OfficialZalo` fails closed with `UNSUPPORTED_PLATFORM` (`SupportStatus::Unsupported`) |
> | **Community Ports / Wine / Web Wrappers** | Never bundled, downloaded, or endorsed | External third-party workarounds (e.g., extracted macOS `app.asar` Electron ports or Wine) are **unverified** and legally ineligible for redistribution |
>
> **Packaging `dim-desktop` into a `.deb` or `.AppImage` on Ubuntu 24.04 proves ONLY that the instance manager builds and packages on Linux — it is NEVER proof that Zalo Desktop or any community Linux port works at runtime.** Zero Zalo binaries or `app.asar` archives are ever embedded in any artifact. Furthermore, live GitHub Actions execution was **NOT** run in this repository audit (`CI_LIVE_RUN_STATUS: NOT_EXECUTED / STATICALLY_VALIDATED_ONLY`).

---

## 1. Canonical Baseline: Ubuntu 24.04 LTS (`x86_64-unknown-linux-gnu`)

Ubuntu 24.04 LTS (Noble Numbat) is selected as the canonical Linux packaging baseline (matching GitHub Actions `ubuntu-24.04`):
1. **Native WebKit2GTK 4.1 (`soup3`) Availability:** Tauri v2 (`tauri = "2.12.6"`, `wry = "0.57.2"`, `webkit2gtk = "2.0.2"`) links against `webkit2gtk-4.1` (`libsoup-3.0`). Ubuntu 24.04 ships `libwebkit2gtk-4.1-dev` and `libwebkit2gtk-4.1-0` directly in `main` with active security maintenance.
2. **64-bit `time_t` (`t64`) ABI Transition:** Ubuntu 24.04 completed Debian/Ubuntu's `t64` package naming transition (`libgtk-3-0t64`, `libfuse2t64`, `libssl3t64`), which packaging metadata (`bundle.linux.deb.depends` — see **IR-003**) and runtime installation guides must reflect accurately.
3. **Ayatana AppIndicator Standard:** Ubuntu 24.04 provides `libayatana-appindicator3-dev` / `libayatana-appindicator3-1` in standard repositories (`libappindicator` crate `v0.9.0` dlopens/links Ayatana or legacy AppIndicator3; updating legacy `.github/workflows/ci.yml` from `libappindicator3-dev` to `libayatana-appindicator3-dev` is tracked as **IR-007** in [`integration-requests.md`](../../plans/reports/build/integration-requests.md)).

---

## 2. APT Dependencies: Build-Time vs End-User Runtime

### 2.1. Build-Time APT Packages (Ubuntu 24.04 LTS)
Install all headers, C/C++ compiler toolchain, pkg-config metadata, and bundle packaging utilities required to compile `dim-desktop` and build `.deb` + `.AppImage` bundles:

```bash
sudo apt-get update
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev \
  build-essential \
  curl \
  wget \
  file \
  libxdo-dev \
  libssl-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev \
  patchelf \
  dpkg-dev \
  libfuse2t64 \
  binutils \
  pkg-config
```

*(Optional for experimental sandbox testing: `sudo apt-get install -y bubblewrap` provides `/usr/bin/bwrap` for `LinuxPlatformAdapter::with_bwrap(true)`.)*

**Additional Build Toolchains:**
- **Rust `>= 1.90`** (`apps/desktop/src-tauri/Cargo.toml` sets `rust-version = "1.90"` for `tauri 2.12.6`, superseding workspace `Cargo.toml` `1.85` — tracked in **IR-001** in [`integration-requests.md`](../../plans/reports/build/integration-requests.md)):
  ```bash
  rustup toolchain install stable --profile minimal --component rustfmt clippy
  rustup target add x86_64-unknown-linux-gnu
  ```
- **Node.js `>= 22` LTS:** Runs the zero-dependency `node:*` scripts in [`scripts/ci-cross-platform/`](../../scripts/ci-cross-platform/).
- **Tauri CLI v2 (`cargo-tauri`):** Required for `.deb` and `.AppImage` bundling (`cargo install tauri-cli@^2.0.0 --locked`; tracked in **IR-004**).

### 2.2. End-User Runtime Shared Libraries (Ubuntu 24.04 LTS)
End-users installing `dim-desktop` do **not** need `-dev` headers, `build-essential`, `patchelf`, or Rust. The runtime shared library requirements depend on the chosen artifact format:

| Runtime Package (Ubuntu 24.04) | Shared Object Provided | Required By `.deb` / Raw ELF? | Required By `.AppImage`? | Purpose |
| :--- | :--- | :---: | :---: | :--- |
| **`libwebkit2gtk-4.1-0`** | `libwebkit2gtk-4.1.so.0`, `libjavascriptcoregtk-4.1.so.0` | **Yes** | Bundled (except base GLIBC/DRI) | Tauri 2 (`wry`) HTML/CSS/JS WebView rendering engine |
| **`libgtk-3-0t64`** | `libgtk-3.so.0`, `libgdk-3.so.0` | **Yes** | Bundled | GTK3 windowing (`tao`) & native dialogs |
| **`libayatana-appindicator3-1`** | `libayatana-appindicator3.so.1` | **Yes** (if tray used / linked) | Bundled | System tray / AppIndicator integration |
| **`librsvg2-2`** | `librsvg-2.so.2` | **Yes** | Bundled | SVG icon rasterization |
| **`libfuse2t64`** | `libfuse.so.2` | No | **Yes** (unless `--appimage-extract-and-run` is used) | Mounts Type-2 SquashFS AppImage filesystem in userspace |

> **Note on `tauri.conf.json` Debian Dependencies (**IR-003**):** Currently [`apps/desktop/src-tauri/tauri.conf.json`](../../apps/desktop/src-tauri/tauri.conf.json) omits `bundle.linux.deb.depends`, relying on Tauri's automatic `ldd` -> `dpkg -S` package lookup on the build runner. Adding explicit `bundle.linux.deb.depends` (`["libwebkit2gtk-4.1-0", "libgtk-3-0t64", "libayatana-appindicator3-1", "librsvg2-2"]`) is tracked in [`integration-requests.md`](../../plans/reports/build/integration-requests.md) (**IR-003**).

---

## 3. Reproducible Build, Test & Packaging Commands

### 3.1. Standardized CI/Local Pipeline (`scripts/ci-cross-platform/*.mjs`)
```bash
# 1. Check Ubuntu 24.04 /etc/os-release, pkg-config (webkit2gtk-4.1, gtk+-3.0, ayatana-appindicator3-0.1, librsvg-2.0),
#    patchelf, dpkg-deb, ldd/GLIBC version, FUSE readiness, and Rust >= 1.90
node desktop-instance-manager/scripts/ci-cross-platform/check-prerequisites.mjs \
  --target x86_64-unknown-linux-gnu \
  --out /tmp/dim-ubuntu2404/prereq.json

# 2. Verify Cargo.lock + src-tauri/Cargo.lock against deny.toml (zero openssl/openssl-sys, SPDX allowlist)
node desktop-instance-manager/scripts/ci-cross-platform/dependency-audit.mjs \
  --out-json /tmp/dim-ubuntu2404/dependency-audit.json \
  --out-md /tmp/dim-ubuntu2404/dependency-audit.md

# 3. Execute workspace & src-tauri tests, compile release ELF binaries, and build .deb + .AppImage bundles
APPIMAGE_EXTRACT_AND_RUN=1 NO_STRIP=true \
node desktop-instance-manager/scripts/ci-cross-platform/build-and-package.mjs \
  --target x86_64-unknown-linux-gnu \
  --bundles all \
  --out /tmp/dim-ubuntu2404/build-summary.json

# 4. Inspect ELF/deb/AppImage headers, permissions, ldd, and GLIBC symbols; emit Section 7 manifest + SHA256SUMS.txt
node desktop-instance-manager/scripts/ci-cross-platform/generate-artifact-manifest.mjs \
  --scan-default-targets \
  --target x86_64-unknown-linux-gnu \
  --build-summary /tmp/dim-ubuntu2404/build-summary.json \
  --os-label ubuntu-24.04 \
  --arch x86_64 \
  --build-log-url "local-dev" \
  --out-manifest /tmp/dim-ubuntu2404/artifact-manifest.json \
  --out-sha256 /tmp/dim-ubuntu2404/SHA256SUMS.txt \
  --out-md /tmp/dim-ubuntu2404/artifact-summary.md
```

### 3.2. Direct Cargo & Tauri Commands (Reference)
```bash
cd desktop-instance-manager

# Format, lint, and unit-test workspace + Tauri crate
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets --target x86_64-unknown-linux-gnu --locked

cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml -- -D warnings
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --target x86_64-unknown-linux-gnu --locked

# Build raw release ELF binaries
cargo build --workspace --release --target x86_64-unknown-linux-gnu --locked
cargo build --manifest-path apps/desktop/src-tauri/Cargo.toml --release --target x86_64-unknown-linux-gnu --locked

# Package .deb and .AppImage via Tauri CLI v2
APPIMAGE_EXTRACT_AND_RUN=1 NO_STRIP=true \
cargo tauri build --target x86_64-unknown-linux-gnu --bundles deb,appimage -- --locked
```

---

## 4. Packaging Outputs, Artifact Classification & AppImage Caveats

### 4.1. Strict Artifact Classification (`compiled-executable` vs Package/Bundle)
Raw ELF binaries are **compiled executables (`is_installer: false`)** and **must never be labeled as installers**:

| Artifact Path (relative to `desktop-instance-manager/`) | `artifact_kind` (`inspect-artifact.mjs`) | `is_installer` | Description & Installation Mechanism |
| :--- | :--- | :---: | :--- |
| `apps/desktop/src-tauri/target/x86_64-unknown-linux-gnu/release/dim-desktop` | `compiled-executable` | **`false`** | Raw 64-bit ELF PIE desktop manager executable. **Not an installer** (no `.desktop` entry, icons, or APT dependency resolution). |
| `target/x86_64-unknown-linux-gnu/release/dim-cli` | `compiled-executable` | **`false`** | Raw 64-bit ELF CLI parity runner (`dim-test-fixture` crate). **Not an installer.** |
| `target/x86_64-unknown-linux-gnu/release/dim-test-fixture` | `compiled-executable` | **`false`** | Raw 64-bit ELF synthetic desktop fixture for automated tests. **Not an installer.** |
| `apps/desktop/src-tauri/target/x86_64-unknown-linux-gnu/release/bundle/deb/*.deb` | `debian-package` | **`true`** | Debian binary package (`ar` archive with `control.tar.*` + `data.tar.*`) built via `dpkg-deb`. Installs `/usr/bin/dim-desktop`, `/usr/share/applications/*.desktop`, and hicolor PNG icons via `sudo apt install ./<pkg>.deb`. |
| `apps/desktop/src-tauri/target/x86_64-unknown-linux-gnu/release/bundle/appimage/*.AppImage` | `linux-appimage` | **`false`** (portable self-mounting bundle) | Type-2 AppImage (`\x7fELF` + `AI\x02` magic at byte offset `8`) bundling `AppRun`, `dim-desktop`, `.desktop`, icons, and non-GLIBC GTK/WebKit shared libraries into a SquashFS payload. |

### 4.2. AppImage (`linuxdeploy`) Operational & CI Considerations on Ubuntu 24.04
1. **Build-Time Network Fetch by Tauri Bundler:** Unless pre-cached in `~/.cache/tauri/`, Tauri's AppImage bundler downloads `linuxdeploy-x86_64.AppImage` and `linuxdeploy-plugin-gtk.sh` over HTTPS from GitHub during `cargo tauri build --bundles appimage`. Air-gapped or restricted CI runners will fail the `appimage` step while `.deb` (`dpkg-deb`) succeeds completely offline; `build-and-package.mjs` records `FAILED_BUNDLER` cleanly if `linuxdeploy` cannot run, without discarding `.deb` or raw ELF outputs.
2. **FUSEless CI Execution (`APPIMAGE_EXTRACT_AND_RUN=1`):** Containerized and hardened CI environments often lack `/dev/fuse` device permissions when invoking `linuxdeploy-x86_64.AppImage`. Setting `APPIMAGE_EXTRACT_AND_RUN=1` instructs `linuxdeploy` and `appimagetool` to self-extract instead of calling `fusermount`.
3. **Ubuntu 24.04 `.relr.dyn` Strip Compatibility (`NO_STRIP=true`):** Ubuntu 24.04's binutils (`ld` 2.42) emits compact `SHT_RELR` (`.relr.dyn`) relative relocations in system/Rust binaries. The older `strip` binary embedded inside upstream `linuxdeploy-x86_64.AppImage` predates `SHT_RELR` and can abort with `unknown type [0x13] section '.relr.dyn'`. Passing `NO_STRIP=true` prevents `linuxdeploy` from running its outdated internal `strip` (Rust release symbol stripping can be controlled cleanly by Cargo/`strip` from Ubuntu 24.04 `binutils`).
4. **End-User Ubuntu 24.04 `libfuse2t64` Requirement:** Ubuntu 22.04 and 24.04 ship `fuse3` (`libfuse3-3`) by default, whereas Type-2 AppImage runtimes dynamically load `libfuse.so.2`. Users on fresh Ubuntu 24.04 desktops must either install `sudo apt-get install libfuse2t64` or run `./Multi-Zalo*.AppImage --appimage-extract-and-run`.

---

## 5. Binary, Shared Library & GLIBC Verification

Every Ubuntu build artifact must pass the following five verification checks (automated in [`scripts/ci-cross-platform/inspect-artifact.mjs`](../../scripts/ci-cross-platform/inspect-artifact.mjs)):

### 5.1. ELF `x86-64` PIE & Hardening Check
```bash
BIN="apps/desktop/src-tauri/target/x86_64-unknown-linux-gnu/release/dim-desktop"
file "$BIN"
# Expected: ELF 64-bit LSB pie executable, x86-64, version 1 (SYSV), dynamically linked, interpreter /lib64/ld-linux-x86-64.so.2, for GNU/Linux ...

readelf -h "$BIN" | grep -E "Class:|Machine:|Type:"
# Expected: Class: ELF64 | Machine: Advanced Micro Devices X86-64 | Type: DYN (Position-Independent Executable file)
```

### 5.2. POSIX Executable Permissions (`0755`)
```bash
stat -c "%a %n" "$BIN"
# Expected: 755 (or (mode & 0o111) != 0)
```
*(Note: `.AppImage` files downloaded by users via a web browser do not preserve executable bits and require `chmod 0755 ./Multi-Zalo*.AppImage` before execution; `.deb` packages preserve `0755` on `/usr/bin/dim-desktop` inside `data.tar`.)*

### 5.3. Dynamic Shared Library Resolution (`ldd`)
```bash
ldd "$BIN"
# Verify zero '=> not found' lines!
! ldd "$BIN" | grep -q "not found"
```
Also ensure `deny.toml` compliance: `dim-desktop` uses pure-Rust crates where applicable and links system GTK/WebKit2GTK without pulling banned `openssl` / `openssl-sys` Rust crates into `Cargo.lock`.

### 5.4. `GLIBC` Symbol Floor & Ubuntu 22.04 Incompatibility Disclosure
```bash
objdump -T "$BIN" | grep -oE 'GLIBC_[0-9]+\.[0-9]+' | sort -uV | tail -n 5
```
> [!WARNING]
> **Honest GLIBC Symbol Floor Disclosure (`GLIBC 2.38` / `2.39` on Ubuntu 24.04)**
>
> - Ubuntu 24.04 LTS ships **GNU C Library `2.39`** (`libc6 2.39-0ubuntu8.x`).
> - Compiling `dim-desktop` on `ubuntu-24.04` binds dynamic symbol versions up to **`GLIBC_2.38` / `GLIBC_2.39`** from the host C toolchain and `libwebkit2gtk-4.1-dev` headers.
> - **Consequence:** Both the `.deb` **and the `.AppImage`** built on Ubuntu 24.04 **will NOT run on Ubuntu 22.04 LTS (Jammy Jellyfish, which ships `GLIBC 2.35`)** or Debian 12 Bookworm (`GLIBC 2.36`), failing at startup with `version 'GLIBC_2.38' not found` (AppImages intentionally exclude `libc.so.6` and bind against the host OS `glibc`).
> - **Policy:** Artifacts built on `ubuntu-24.04` strictly target **Ubuntu 24.04 LTS and newer distributions (`GLIBC >= 2.39`)**. Supporting Ubuntu 22.04 LTS in the future would require compiling inside an Ubuntu 22.04 container (`GLIBC 2.35` + `libwebkit2gtk-4.1-dev`) rather than Ubuntu 24.04.

### 5.5. Debian Package (`.deb`) & AppImage Header Verification
```bash
# Inspect .deb control metadata (Package, Version, Architecture: amd64, Depends) and file permissions
dpkg-deb -I apps/desktop/src-tauri/target/x86_64-unknown-linux-gnu/release/bundle/deb/*.deb
dpkg-deb -c apps/desktop/src-tauri/target/x86_64-unknown-linux-gnu/release/bundle/deb/*.deb

# Inspect AppImage ELF + 'AI\x02' magic bytes at offset 8 via inspect-artifact.mjs
node scripts/ci-cross-platform/inspect-artifact.mjs \
  --file apps/desktop/src-tauri/target/x86_64-unknown-linux-gnu/release/bundle/appimage/*.AppImage
```

### 5.6. Signing & Integrity Status (`UNSIGNED_DEVELOPMENT_ARTIFACT`)
- Neither `.deb` packages nor `.AppImage` bundles are currently GPG-signed (`dpkg-sig` / APT `InRelease` GPG key or detached `.asc` / Minisign signature — see Gate G5 in [`signing-and-release-gates.md`](./signing-and-release-gates.md)).
- All Linux build outputs carry `signing_status: "UNSIGNED_DEVELOPMENT_ARTIFACT"` and are verified strictly by SHA-256 digest (`SHA256SUMS.txt` and `artifact-validation-manifest.json`) for development/CI testing only.

---

## 6. Runtime Architecture & Ubuntu 24.04 AppArmor Caveats (`crates/platform-linux`)

A direct audit of [`crates/platform-linux/src/adapter.rs`](../../crates/platform-linux/src/adapter.rs) and [`crates/platform-linux/src/layout.rs`](../../crates/platform-linux/src/layout.rs) defines exact Linux runtime behavior:

1. **Official Zalo Launch Fails Closed (`UNSUPPORTED_PLATFORM`):**
   - `LinuxPlatformAdapter::discover_installation(None)` returns `is_valid: false`, `support_status: SupportStatus::Unsupported`, and `error: Some("Official VNG Zalo Desktop is unavailable on Linux (SupportStatus::Unsupported).")`.
   - `LinuxPlatformAdapter::launch(&LaunchTarget::OfficialZalo { .. }, ...)` returns `Err(DimError::UnsupportedPlatform(...))` (`code() == "UNSUPPORTED_PLATFORM"`), verified by unit test `official_zalo_launch_is_rejected_on_linux`.
2. **Experimental Isolation for Synthetic Fixtures & User-Supplied Local AppImages:**
   - For `LaunchTarget::NodeFixture` (`fixtures/test-desktop-app/fixture-app.mjs`) and explicit user-supplied `LaunchTarget::CustomExecutable`, `dim-platform-linux` creates `<profile_dir>/.config/ZaloData/.zalo-profile-id`, `.local/share/`, `.cache/`, and `tmp/`, overriding `HOME`, `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `XDG_CACHE_HOME`, and `TMPDIR`.
   - When `LinuxPlatformAdapter::with_bwrap(true)` is enabled and `/usr/bin/bwrap` is in `PATH`, it wraps execution in Bubblewrap (`bwrap --die-with-parent --ro-bind /usr /usr ... --bind <profile_dir> <profile_dir>`).
3. **Ubuntu 24.04 AppArmor Unprivileged User Namespace Restriction (`kernel.apparmor_restrict_unprivileged_userns = 1`):**
   - Ubuntu 23.10+ and Ubuntu 24.04 LTS restrict unprivileged user namespaces via AppArmor by default.
   - As documented in `APPARMOR_NO_SANDBOX_NOTE` ([`crates/platform-linux/src/layout.rs`](../../crates/platform-linux/src/layout.rs#L12-L13)):
     - User-supplied third-party Electron AppImages embed Chromium's `chrome-sandbox` helper, which fails to create an unprivileged user namespace on Ubuntu 24.04 unless an administrator installs a dedicated `/etc/apparmor.d/` `userns` profile for that binary (passing `--no-sandbox` bypasses Chromium's renderer sandbox and reduces security).
     - Ubuntu 24.04 ships `/etc/apparmor.d/bwrap-userns-restrict` allowing `/usr/bin/bwrap` itself, but nested Chromium user-namespace creation *inside* `bwrap` remains restricted.
   - **Security Rule:** Never disable Ubuntu AppArmor globally (`sysctl -w kernel.apparmor_restrict_unprivileged_userns=0` must **never** be run or recommended in scripts).

---

## 7. Related Documentation
- [Windows Packaging Guide (`windows.md`)](./windows.md)
- [macOS Packaging Guide (`macos.md`)](./macos.md)
- [Cross-Platform Build Matrix (`build-matrix.md`)](./build-matrix.md)
- [Code Signing & Release Gates (`signing-and-release-gates.md`)](./signing-and-release-gates.md)
- [Release Readiness Scorecard (`release-readiness.md`)](./release-readiness.md)
- [Out-of-Scope Integration Requests (`integration-requests.md`)](../../plans/reports/build/integration-requests.md)
- [OS Readiness & Compatibility Matrix (`os-readiness.md`)](../compatibility/os-readiness.md)
