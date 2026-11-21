# Ubuntu 24.04 LTS Packaging & Distribution Guide (`dim-desktop`)

**Document Scope:** Native `ubuntu-24.04` build dependencies, 7-stage verification (including `xvfb-run -a` GUI startup & `fixture-app.mjs`/`dim-cli` smoke tests), `.deb` & manager `.AppImage` packaging, ELF/`GLIBC` verification, and strict runtime honesty boundaries for **Multi-Zalo Desktop — Instance Manager (`dim-desktop`)** on Ubuntu Linux.  
**Canonical Native Target:** **Ubuntu 24.04 LTS (Noble Numbat)** — `x86_64-unknown-linux-gnu` (`amd64`, Priority #1 CI job `ubuntu-2404-build`, `$0.006/min` `1x` multiplier).  
**Current Artifact Signing & Provenance State:** `UNSIGNED_DEVELOPMENT_ARTIFACT` (`provenance_trust_level: "SELF_REPORTED_UNATTESTED"`) — **strictly for development/CI validation, NEVER a production release**.  
**Official Zalo Linux Support Status:** **`UNSUPPORTED`** (`SupportStatus::Unsupported` — VNG provides no Linux Zalo client); synthetic fixture & user-supplied local AppImages are **`EXPERIMENTAL`** (`SupportStatus::Experimental` in [`crates/platform-linux/src/adapter.rs`](../../crates/platform-linux/src/adapter.rs)).

---

> [!IMPORTANT]
> **CRITICAL HONESTY GATE — Two Completely Separate Realities on Linux**
>
> You must strictly separate **(A)** building and packaging the manager application from **(B)** running Zalo Desktop on Linux:
>
> | Dimension | (A) Desktop Instance Manager (`dim-desktop` / `dim-cli`) | (B) Zalo Desktop Client on Linux |
> | :--- | :--- | :--- |
> | **Vendor / Codebase** | Open-source Rust + Tauri 2 (`WebKit2GTK 4.1` + `GTK 3`) in this repository | Proprietary VNG Corporation client |
> | **Native Linux Build Exists?** | **Yes** — compiles & packages natively for `x86_64-unknown-linux-gnu` (`dim-desktop` ELF, `.deb`, manager `.AppImage`) | **NO** — VNG ships **zero** official Linux binaries, `.deb`, `.rpm`, Flatpak, Snap, or AppImage |
> | **Runtime Status in Code** | Compiles, runs `xvfb-run -a` startup & `dim-cli`/`fixture-app.mjs` smoke tests; isolates `HOME`/`XDG_*` + optional `bwrap` for fixtures | `LaunchTarget::OfficialZalo` fails closed with `UNSUPPORTED_PLATFORM` (`SupportStatus::Unsupported`) unless the **Experimental community `zalo-linux` repack** is detected (audited-allowlisted `/usr/bin/zalo` + `/opt/zalo-linux`, never official VNG) |
> | **Community Ports / Wine / Web Wrappers** | Never bundled, downloaded, or endorsed | External third-party workarounds are not bundled or endorsed; the locally installed `zalo-linux` route is separately documented as Experimental in [the operations route](../../../docs/operations.md), and local validation does not authorize redistribution |
>
> **Packaging `dim-desktop` into a `.deb` or `.AppImage` on Ubuntu 24.04 proves ONLY that the instance manager builds, links, and launches its shell on Linux — it is NEVER proof that Zalo Desktop or any community Linux port works at runtime.** Zero Zalo binaries or `app.asar` archives are ever embedded in any artifact (`vng_binary_embedded: false`). Furthermore, remote GitHub Actions execution is currently **`BLOCKED`** (`0` remote runs — see [`ci-execution-and-artifact-validation.md`](./ci-execution-and-artifact-validation.md)).

---

## 1. Canonical Native Baseline: Ubuntu 24.04 LTS (`ubuntu-24.04`)

Ubuntu 24.04 LTS (`x86_64-unknown-linux-gnu`) is Priority #1 in [`.github/workflows/cross-platform-validation.yml`](../../../.github/workflows/cross-platform-validation.yml) (`enable_ubuntu: true`, `timeout-minutes: 25`, lowest private-repo cost at **`$0.006/min` / `1x` multiplier**):
1. **Fail-Closed Native Hardware & OS Gate:** `ubuntu-2404-build` verifies `test "$(uname -m)" = "x86_64"`, `grep -E '^VERSION_ID="24\.04"' /etc/os-release`, and `rustc -vV` `host: x86_64-unknown-linux-gnu` before compiling.
2. **Native WebKit2GTK 4.1 (`soup3`) & `t64` ABI:** Tauri v2 (`tauri = "2.12.6"`, `wry = "0.57.2"`, `webkit2gtk = "2.0.2"`) links against `libwebkit2gtk-4.1`. Ubuntu 24.04 ships `libwebkit2gtk-4.1-dev` / `libwebkit2gtk-4.1-0` natively alongside 64-bit `time_t` (`t64`) packages (`libgtk-3-0t64`, `libfuse2t64`).
3. **Ayatana AppIndicator Standard:** Ubuntu 24.04 provides `libayatana-appindicator3-dev` / `libayatana-appindicator3-1` (`libappindicator v0.9.0`). Updating legacy `.github/workflows/ci.yml` from `libappindicator3-dev` to `libayatana-appindicator3-dev` is delivered in atomic patch [`0007-IR-007-ci-yml-locked-and-ubuntu2404-ayatana.patch`](../../plans/reports/build/ir-patches/0007-IR-007-ci-yml-locked-and-ubuntu2404-ayatana.patch).

---

## 2. APT Dependencies: Build-Time vs End-User Runtime

### 2.1. Build-Time & CI Headless Verification APT Packages (Ubuntu 24.04 LTS)
Matches `ubuntu-2404-build` in `.github/workflows/cross-platform-validation.yml` (including `xvfb` for headless X11 GUI startup verification):

```bash
sudo apt-get update
sudo apt-get install -y --no-install-recommends \
  build-essential \
  pkg-config \
  curl \
  wget \
  libssl-dev \
  libxdo-dev \
  libwebkit2gtk-4.1-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev \
  patchelf \
  file \
  binutils \
  dpkg-dev \
  libfuse2t64 \
  xvfb
```

*(Optional for experimental sandbox testing: `sudo apt-get install -y bubblewrap` installs `/usr/bin/bwrap` for `LinuxPlatformAdapter::with_bwrap(true)`.)*

**Additional Required Toolchains:**
- **Rust `>= 1.90` (`--locked` enforced):** [`apps/desktop/src-tauri/Cargo.toml`](../../apps/desktop/src-tauri/Cargo.toml) requires `rust-version = "1.90"` (`tauri 2.12.6`); workspace `Cargo.toml` MSRV alignment (`1.85` -> `1.90`) is provided in [`0001-IR-001-align-workspace-msrv-1.90.patch`](../../plans/reports/build/ir-patches/0001-IR-001-align-workspace-msrv-1.90.patch).
- **Node.js `22` LTS:** Executes `scripts/ci-cross-platform/*.mjs`, `fixtures/test-desktop-app/fixture-app.mjs`, and pinned Tauri CLI.
- **Pinned Tauri CLI (`@tauri-apps/cli@2.12.5`):** Installed with exact version pinning (`npm install --global "@tauri-apps/cli@2.12.5"`, tracked in `IR-004`).

### 2.2. End-User Runtime Shared Libraries & CI Overlay (`IR-003`)
End-users installing `dim-desktop` do **not** need `-dev` headers, `build-essential`, `patchelf`, `xvfb`, or Rust:

| Runtime Package (Ubuntu 24.04) | Shared Object Provided | Required By `.deb` / Raw ELF? | Required By Manager `.AppImage`? | Purpose |
| :--- | :--- | :---: | :---: | :--- |
| **`libwebkit2gtk-4.1-0`** | `libwebkit2gtk-4.1.so.0`, `libjavascriptcoregtk-4.1.so.0` | **Yes** | Bundled (except host GLIBC/DRI) | Tauri 2 (`wry`) HTML/CSS/JS WebView engine |
| **`libgtk-3-0t64`** | `libgtk-3.so.0`, `libgdk-3.so.0` | **Yes** | Bundled | GTK3 windowing (`tao`) & native dialogs |
| **`libayatana-appindicator3-1`** | `libayatana-appindicator3.so.1` | **Yes** | Bundled | System tray / AppIndicator integration |
| **`librsvg2-2`** | `librsvg-2.so.2` | **Yes** | Bundled | SVG icon rasterization |
| **`libfuse2t64`** | `libfuse.so.2` | No | **Yes** (unless `--appimage-extract-and-run` is passed) | Mounts Type-2 SquashFS AppImage filesystem in userspace |

> **Active CI Overlay & Atomic Patch (`IR-003`):** Without mutating shared `tauri.conf.json`, [`scripts/ci-cross-platform/tauri-ci-bundle-overlay.json`](../../scripts/ci-cross-platform/tauri-ci-bundle-overlay.json) configures `bundle.linux.deb` with `"section": "utils"`, `"priority": "optional"`, and `"depends": ["libwebkit2gtk-4.1-0", "libgtk-3-0t64 | libgtk-3-0", "libayatana-appindicator3-1 | libappindicator3-1"]` during `tauri build --config`. The permanent upstream diff is ready in [`0003-IR-003-tauri-os-bundle-metadata.patch`](../../plans/reports/build/ir-patches/0003-IR-003-tauri-os-bundle-metadata.patch).

---

## 3. Native 7-Stage Build, Headless Runtime & Packaging Pipeline

```bash
# Stage 0: Verify native Ubuntu 24.04 x86_64 host and Rust host triple
test "$(uname -m)" = "x86_64"
grep -E '^VERSION_ID="24\.04"' /etc/os-release
rustc -vV | grep -q '^host: x86_64-unknown-linux-gnu$'

# Stage 1: Check /etc/os-release, pkg-config (webkit2gtk-4.1, gtk+-3.0, ayatana-appindicator3-0.1, librsvg-2.0),
#          patchelf, dpkg-deb, ldd/GLIBC, FUSE readiness, and Rust >= 1.90
node desktop-instance-manager/scripts/ci-cross-platform/check-prerequisites.mjs \
  --target x86_64-unknown-linux-gnu \
  --out /tmp/dim-ubuntu2404/prereq.json

# Stage 2: Clippy (-D warnings, --locked) on workspace and Tauri desktop crate
cargo clippy --manifest-path desktop-instance-manager/Cargo.toml --workspace --all-targets --target x86_64-unknown-linux-gnu --locked -- -D warnings
cargo clippy --manifest-path desktop-instance-manager/apps/desktop/src-tauri/Cargo.toml --target x86_64-unknown-linux-gnu --locked -- -D warnings

# Stages 3–6: Locked unit tests, release compile, native runtime fixture smoke, and .deb + .AppImage packaging
#             (uses pinned @tauri-apps/cli@2.12.5 + tauri-ci-bundle-overlay.json)
APPIMAGE_EXTRACT_AND_RUN=1 \
node desktop-instance-manager/scripts/ci-cross-platform/build-and-package.mjs \
  --target x86_64-unknown-linux-gnu \
  --bundles all \
  --out /tmp/dim-ubuntu2404/build-summary.json

# Verify native fixture + dim-cli XDG isolation smoke AND headless Xvfb GUI startup of dim-desktop
BIN_PATH="desktop-instance-manager/apps/desktop/src-tauri/target/x86_64-unknown-linux-gnu/release/dim-desktop"
CLI_PATH="desktop-instance-manager/target/x86_64-unknown-linux-gnu/release/dim-cli"
TMP_FIX="$(mktemp -d)"
HOME="${TMP_FIX}" XDG_CONFIG_HOME="${TMP_FIX}" node desktop-instance-manager/fixtures/test-desktop-app/fixture-app.mjs --stay-ms=100
HOME="${TMP_FIX}" XDG_CONFIG_HOME="${TMP_FIX}" "${CLI_PATH}" --data-dir "${TMP_FIX}/dim-data" status --json
rm -rf "${TMP_FIX}"

# Headless Xvfb GTK/WebKit2GTK startup probe (exit 0, 124 timeout, or 143 SIGTERM confirms clean window init without crash)
timeout 3s xvfb-run -a "${BIN_PATH}" >/dev/null 2>&1 || [ $? -eq 124 ] || [ $? -eq 143 ]

# Stage 7: Inspect ELF/.deb/.AppImage headers, permissions, ldd, and GLIBC symbols; emit Section 7 manifest + SHA256SUMS
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
| `apps/desktop/src-tauri/target/x86_64-unknown-linux-gnu/release/bundle/appimage/*.AppImage` | `linux-appimage` | **`false`** (portable self-mounting manager bundle) | Type-2 AppImage (`\x7fELF` + `AI\x02` magic at byte offset `8`) packaging **only the `dim-desktop` manager shell** (zero Zalo binaries). |

### 4.2. Manager `.AppImage` (`linuxdeploy`) Operational Considerations on Ubuntu 24.04
1. **Build-Time `linuxdeploy` Download:** Unless cached in `~/.cache/tauri/`, Tauri's AppImage bundler downloads `linuxdeploy-x86_64.AppImage` and `linuxdeploy-plugin-gtk.sh` over HTTPS during bundling. If blocked on restricted networks, `build-and-package.mjs` records `FAILED_BUNDLER` diagnostically while preserving `.deb` and raw ELF outputs.
2. **FUSEless CI Execution (`APPIMAGE_EXTRACT_AND_RUN=1`) & `.relr.dyn` Strip Note (`NO_STRIP=true`):** Setting `APPIMAGE_EXTRACT_AND_RUN=1` allows `linuxdeploy` to run without `/dev/fuse` mount privileges in CI containers. If an older cached `linuxdeploy` `strip` binary encounters Ubuntu 24.04 binutils `2.42` `.relr.dyn` (`SHT_RELR`) sections, pass `NO_STRIP=true` and rely on Cargo's native `[profile.release] strip = "symbols"` (`0006-IR-006-...patch`).
3. **End-User Ubuntu 24.04 `libfuse2t64` Requirement:** Fresh Ubuntu 24.04 desktops ship `fuse3` (`libfuse3-3`), whereas Type-2 AppImage runtimes load `libfuse.so.2`. Users running the manager `.AppImage` must install `sudo apt-get install libfuse2t64` or pass `--appimage-extract-and-run`.

---

## 5. Binary, Shared Library, `GLIBC` & Supply-Chain Verification

Every Ubuntu build artifact is checked across five binary gates (automated in [`inspect-artifact.mjs`](../../scripts/ci-cross-platform/inspect-artifact.mjs)) and our 7-pillar supply-chain model:

1. **ELF64 `x86-64` PIE & POSIX Permissions (`0755`):**
   - `file` / `readelf -h` confirms `ELF 64-bit LSB pie executable, x86-64` (`ET_DYN` with `/lib64/ld-linux-x86-64.so.2`), and `test -x` / `stat -c "%a"` confirms `(mode & 0o111) !== 0` (`0755`).
2. **Dynamic Linker Resolution (`ldd`) & Headless `xvfb-run -a` Probe:**
   - `! ldd "$BIN_PATH" | grep -q "not found"` confirms all WebKit2GTK 4.1, GTK 3, Ayatana AppIndicator, and RSVG shared libraries resolve cleanly, and `timeout 3s xvfb-run -a "$BIN_PATH"` verifies GTK/WebKit initialization under a virtual X11 framebuffer.
3. **`GLIBC` Symbol Floor & Ubuntu 22.04 Incompatibility Disclosure:**
   ```bash
   objdump -T "$BIN_PATH" | grep -oE 'GLIBC_[0-9]+\.[0-9]+' | sort -V | uniq | tail -n 5
   ```
   > [!WARNING]
   > **Honest `GLIBC 2.38` / `2.39` Floor Disclosure**
   >
   > Ubuntu 24.04 LTS ships **`glibc 2.39`**. Compiling `dim-desktop` on `ubuntu-24.04` binds dynamic symbols up to **`GLIBC_2.38` / `GLIBC_2.39`**. Consequently, **both the `.deb` and the `.AppImage` built on Ubuntu 24.04 will fail to launch on Ubuntu 22.04 LTS (`GLIBC 2.35`)** or Debian 12 (`GLIBC 2.36`) with `version 'GLIBC_2.38' not found` (AppImages intentionally exclude `libc.so.6` and bind against host `glibc`). Artifacts built on `ubuntu-24.04` strictly target **Ubuntu 24.04 LTS and newer (`GLIBC >= 2.39`)**.
4. **`.deb` Control & File Verification:**
   - `dpkg-deb -I` and `dpkg-deb -c` verify `Architecture: amd64`, overlay `Depends:` (`libwebkit2gtk-4.1-0`, `libgtk-3-0t64 | libgtk-3-0`, `libayatana-appindicator3-1 | libappindicator3-1`), and `/usr/bin/dim-desktop` permissions.
5. **7-Pillar Supply-Chain & Signing Honesty (`UNSIGNED_DEVELOPMENT_ARTIFACT`):**
   - Lockfile SHA-256 checksums (`431/431`), SPDX licenses (`439` crates, `0` violations), and real RustSec vulnerability scanning (`cargo-deny 0.21.0` against `rustsec/advisory-db` @ `233155b4025b854c68a4b199c0e32d7fd53291a2` -> `0` vulnerabilities) are verified; SBOM is strictly a declared dependency inventory (not build provenance); local provenance is `SELF_REPORTED_UNATTESTED`; and `.deb` / `.AppImage` artifacts carry `signing_status: "UNSIGNED_DEVELOPMENT_ARTIFACT"` until GPG/Minisign signing is provisioned under Gate G5 ([`signing-and-release-gates.md`](./signing-and-release-gates.md)).

---

## 6. Linux Adapter Scope & Ubuntu 24.04 AppArmor Caveats (`crates/platform-linux`)

Direct inspection of [`crates/platform-linux/src/adapter.rs`](../../crates/platform-linux/src/adapter.rs) and [`crates/platform-linux/src/layout.rs`](../../crates/platform-linux/src/layout.rs) confirms:
1. **Official Zalo Launch Fails Closed (`UNSUPPORTED_PLATFORM`) When No Client Exists:** `LinuxPlatformAdapter::discover_installation(None)` returns `SupportStatus::Unsupported` and `launch(&LaunchTarget::OfficialZalo { .. }, ...)` returns `Err(DimError::UnsupportedPlatform(...))` (`unit test: official_zalo_launch_is_rejected_on_linux`) whenever the community `zalo-linux` route is not installed. VNG still ships **zero** official Linux binaries; this status is never upgraded to a supported tier.
2. **Experimental Community Repack Route (`zalo-linux`, `SupportStatus::Experimental`):** when both `/usr/bin/zalo` and `/opt/zalo-linux/app` exist, discovery reports an `Experimental` installation with provenance `CommunityRepack (zalo-linux pkg, NOT VNG signed)` under root `/opt/zalo-linux`. The wrapper script is statically hazard-audited and its exact SHA-256 is recorded at discovery time; the app manifest and bundled Electron binary are also hashed, and app-directory uid/mode is snapshotted and re-verified before launch. World-writable app directories and setuid/world-writable wrappers fail closed. The installed community repack may be user-owned and group-writable, so NexaDock does NOT claim to prevent same-user or same-group payload modification; digest/metadata changes between discovery and launch are detected, but the payload is not VNG-signed or authenticated to a vendor key. The allowlist snapshot is in-memory per process and resets on restart; each process re-audits at first use. There remains a verify-then-exec TOCTOU window between the final digest check and process execution; snapshot-and-exec is out of scope. Launches run the wrapper with the profile slug under isolated `HOME`/XDG and pre-created `0700` per-instance `ZALO_DATA_DIR` (`<profile_dir>/.local/share/zalo-linux/<directory_name>`); NexaDock never passes `--no-sandbox`. The wrapper's own internal flag is accepted only for the audited route. The route refuses `bwrap` because the planner does not bind `/opt/zalo-linux`. The coordinator gate stays blocked while `official_zalo_status` is `Unsupported` and permits the route only at `Experimental`.
3. **Experimental XDG/`HOME` & `bwrap` Isolation for Fixtures Only:** `LaunchTarget::NodeFixture` and explicit user-supplied `LaunchTarget::CustomExecutable` paths isolate `HOME`, `XDG_CONFIG_HOME` (`<profile_dir>/.config/ZaloData/.zalo-profile-id`), `XDG_DATA_HOME`, `XDG_CACHE_HOME`, and `TMPDIR`, with optional `/usr/bin/bwrap` (`--die-with-parent`) sandboxing.
4. **Ubuntu 24.04 AppArmor User-Namespace Restriction (`APPARMOR_NO_SANDBOX_NOTE`):** Ubuntu 24.04 sets `kernel.apparmor_restrict_unprivileged_userns = 1` by default, blocking Chromium `chrome-sandbox` inside third-party Electron AppImages unless a system administrator installs a dedicated `/etc/apparmor.d/` `userns` profile (passing `--no-sandbox` weakens renderer isolation). Never disable host AppArmor globally (`sysctl` bypasses are strictly prohibited).

---

## 7. Related Documentation
- [CI Execution & Artifact Validation Report (`ci-execution-and-artifact-validation.md`)](./ci-execution-and-artifact-validation.md)
- [Windows Packaging Guide (`windows.md`)](./windows.md)
- [macOS Packaging Guide (`macos.md`)](./macos.md)
- [Cross-Platform Build Matrix (`build-matrix.md`)](./build-matrix.md)
- [Code Signing & Release Gates (`signing-and-release-gates.md`)](./signing-and-release-gates.md)
- [Release Readiness Scorecard (`release-readiness.md`)](./release-readiness.md)
- [Out-of-Scope Integration Requests & Atomic Patches (`integration-requests.md`)](../../plans/reports/build/integration-requests.md)
- [OS Readiness & Compatibility Matrix (`os-readiness.md`)](../compatibility/os-readiness.md)
