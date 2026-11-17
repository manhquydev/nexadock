# Out-of-Scope Integration Requests — Multi-Zalo Cross-Platform Build & Packaging

**Document ID:** `DIM-BUILD-IR-20261117`  
**Branch:** `build/cross-platform-distribution` (Baseline Commit: `a5feb4a520798064c1508c26fe4c4ac17a3a03cf`)  
**Scope Boundary:** During Mission E (Cross-Platform Build & Distribution), repository build manifests (`Cargo.toml`, `Cargo.lock`, `apps/desktop/src-tauri/Cargo.toml`, `apps/desktop/src-tauri/tauri.conf.json`), application icons (`apps/desktop/src-tauri/icons/*`), and existing CI (`.github/workflows/ci.yml`) were strictly read-only to guarantee zero unreviewed churn on core crates and lockfiles.  
**Purpose:** Record every required follow-up change discovered during cross-platform build engineering and local Windows `x86_64` verification, with exact file paths, unified diffs/proposals, severity ratings, and technical rationale.

---

## Summary Matrix

| ID | Severity | Target File(s) | Summary | Blocks |
| :--- | :--- | :--- | :--- | :--- |
| **IR-001** | **Medium** | `desktop-instance-manager/Cargo.toml`, `docs/architecture/adr-002-rust-tauri-and-sac-host-bridge.md` | Align workspace `rust-version` (`"1.85"`) with `apps/desktop/src-tauri/Cargo.toml` (`"1.90"`) and refresh stale ADR-002 MSRV text (`1.78`) | Unified MSRV enforcement across workspace & Tauri shell |
| **IR-002** | **High** | `apps/desktop/src-tauri/icons/*`, `apps/desktop/src-tauri/tauri.conf.json` | Add missing `icons/icon.icns` and `icons/128x128@2x.png`, replace tiny placeholder `.ico`/`.png` files with production-grade multi-resolution RGBA assets | macOS `.app`/`.dmg` icon bundling & Windows HiDPI installer icons |
| **IR-003** | **High** | `apps/desktop/src-tauri/tauri.conf.json` | Add explicit `bundle.windows`, `bundle.macOS`, `bundle.linux`, and publisher metadata blocks (`downloadBootstrapper`, `currentUser` NSIS, `minimumSystemVersion`, Ubuntu 24.04 `deb.depends`) | Deterministic MSI/NSIS, `.app`/`.dmg`, and Ubuntu 24.04 `.deb` dependency metadata |
| **IR-004** | **Medium** | `desktop-instance-manager/Cargo.toml`, `apps/desktop/src-tauri/Cargo.toml`, CI scripts | Evaluate unifying split `[workspace]` / dual `Cargo.lock` (`41` vs `434` crates) and pin `tauri-cli` (`cargo-tauri`) exact version | Single-lockfile dependency auditing & deterministic bundler CLI versions |
| **IR-005** | **Medium** | `apps/desktop/src-tauri/Cargo.toml`, `apps/desktop/src-tauri/src/main.rs` | Enforce workspace-equivalent `[lints.rust]` and `[lints.clippy]` rules (`unsafe_op_in_unsafe_fn = "deny"`, `unwrap_used = "warn"`, `expect_used = "warn"`) in `dim-desktop` | Static safety & panic-surface parity across IPC shell |
| **IR-006** | **Low** | `desktop-instance-manager/Cargo.toml`, `apps/desktop/src-tauri/Cargo.toml` | Add explicit `[profile.release]` (`lto = "thin"`, `codegen-units = 1`, `strip = "symbols"`) and document GNU `ld.exe` `.rsrc` `RT_VERSION` merge note vs MSVC `link.exe` | Smaller release executables & stripped symbol tables |
| **IR-007** | **Medium** | `.github/workflows/ci.yml` | Update `libappindicator3-dev` to `libayatana-appindicator3-dev` for Ubuntu 24.04 runners, include `src-tauri/Cargo.lock` in cache key, and pass `--locked` in `tauri-desktop-check` | Future Ubuntu 24.04 `ubuntu-latest` APT compatibility & lockfile enforcement in push/PR CI |

---

## Detailed Integration Proposals

### IR-001 — Rust MSRV Alignment (`1.85` vs `1.90`)

- **Severity:** Medium (Build Governance & Toolchain Consistency)
- **Affected Files:**
  - `desktop-instance-manager/Cargo.toml` (line 19)
  - `desktop-instance-manager/apps/desktop/src-tauri/Cargo.toml` (line 5)
  - `desktop-instance-manager/docs/architecture/adr-002-rust-tauri-and-sac-host-bridge.md` (line 37)
- **Observed Evidence:**
  - `desktop-instance-manager/Cargo.toml` declares `rust-version = "1.85"`, whereas `apps/desktop/src-tauri/Cargo.toml` declares `rust-version = "1.90"`.
  - Inspection of `apps/desktop/src-tauri/Cargo.lock` (`434` packages) shows `tauri v2.12.6` and transitive crates (`sha2 v0.11.1`, `tauri-runtime-wry v2.12.4`, `wry v0.57.2`) require Rust `>= 1.90`. Meanwhile, `adr-002-rust-tauri-and-sac-host-bridge.md` still cites historical MSRV `1.78`.
  - A contributor or CI job provisioning Rust `1.85` based on the root `Cargo.toml` succeeds on `crates/*` (`dim-cli`, `dim-test-fixture`) but fails immediately when compiling `apps/desktop/src-tauri` (`dim-desktop`).
- **Proposed Change (`desktop-instance-manager/Cargo.toml`):**

```diff
 [workspace.package]
 version = "0.1.0"
 edition = "2021"
-rust-version = "1.85"
+rust-version = "1.90"
 license = "MIT OR Apache-2.0"
```

- **Rationale:** Raising the workspace MSRV floor to `1.90` establishes a single truthful minimum Rust toolchain requirement across both the core CLI/library crates and the Tauri 2 desktop application.

---

### IR-002 — Missing macOS `.icns` / Retina Icons & Placeholder Icon Assets

- **Severity:** High (Blocks or degrades macOS `.app`/`.dmg` packaging & Windows HiDPI installer presentation)
- **Affected Files:**
  - `desktop-instance-manager/apps/desktop/src-tauri/icons/` (`32x32.png` = `115` B, `128x128.png` = `402` B, `icon.png` = `402` B, `icon.ico` = `137` B)
  - `desktop-instance-manager/apps/desktop/src-tauri/tauri.conf.json` (lines 28–32)
- **Observed Evidence:**
  - `apps/desktop/src-tauri/icons/` contains only 4 minimal placeholder files (`115`–`402` bytes) and lacks `icons/icon.icns` (required by Tauri's macOS `.app` and `.dmg` bundler for `CFBundleIconFile`) and `icons/128x128@2x.png` (`256x256` Retina icon).
  - `tauri.conf.json` `bundle.icon` lists only `["icons/32x32.png", "icons/128x128.png", "icons/icon.ico"]`. When `cargo tauri build` runs on macOS (`aarch64-apple-darwin` or `x86_64-apple-darwin`) with `--bundles app,dmg`, missing `.icns` causes Tauri's macOS bundler to fail or omit standard Finder/Dock bundle icons.
  - On Windows, `icon.ico` (`137` bytes) lacks standard multi-resolution frames (`16x16`, `24x24`, `32x32`, `48x48`, `64x64`, `256x256` RGBA) expected by WiX (`.msi` ARPPRODUCTICON) and NSIS (`MUI_ICON`).
- **Proposed Change:**
  1. Generate clean original Multi-Zalo Desktop brand icon assets (strictly original artwork — **never** copying VNG's copyrighted Zalo logo) via `cargo tauri icon <source-1024x1024.png>` to populate `32x32.png`, `128x128.png`, `128x128@2x.png`, `icon.icns`, and multi-size `icon.ico`.
  2. Update `apps/desktop/src-tauri/tauri.conf.json`:

```diff
   "bundle": {
     "active": true,
     "targets": "all",
     "icon": [
       "icons/32x32.png",
       "icons/128x128.png",
+      "icons/128x128@2x.png",
+      "icons/icon.icns",
       "icons/icon.ico"
     ]
   }
```

---

### IR-003 — Explicit OS Bundle Configuration Blocks in `tauri.conf.json`

- **Severity:** High (Required before public installer distribution on Windows, macOS, and Ubuntu 24.04 LTS)
- **Affected File:** `desktop-instance-manager/apps/desktop/src-tauri/tauri.conf.json` (lines 25–33)
- **Observed Evidence:**
  - `tauri.conf.json` currently relies entirely on implicit Tauri defaults under `"bundle": { "active": true, "targets": "all", "icon": [...] }`.
  - **Windows (`x86_64-pc-windows-msvc`):** Lacks explicit `webviewInstallMode` (`downloadBootstrapper`), explicit SHA-256 digest setting (`digestAlgorithm: "sha256"`), and `nsis.installMode: "currentUser"` (critical because Multi-Zalo Desktop runs strictly `asInvoker` in standard user scope under `%LOCALAPPDATA%` without UAC elevation).
  - **macOS (`aarch64-apple-darwin`, `x86_64-apple-darwin`):** Lacks explicit `minimumSystemVersion` (`"13.0"`), `hardenedRuntime: true`, and `signingIdentity: null` default for unsigned local/CI builds prior to Apple Developer ID provisioning. Note: packaging the manager `.app`/`.dmg` only packages `dim-desktop` and does **not** validate VNG `Zalo.app` multi-instance runtime behavior on macOS.
  - **Linux (`x86_64-unknown-linux-gnu`, Ubuntu 24.04 LTS):** Lacks explicit `bundle.linux.deb.depends` pinning Ubuntu 24.04 runtime shared libraries (`libwebkit2gtk-4.1-0`, `libgtk-3-0t64`, `libayatana-appindicator3-1`) so `apt install ./multi-zalo-desktop_*.deb` resolves noble's `t64` and WebKitGTK 4.1 packages cleanly. Note: packaging `dim-desktop` on Ubuntu 24.04 does **not** bundle or validate any Zalo Linux client (VNG provides no official Linux Zalo build).
- **Proposed Change (`apps/desktop/src-tauri/tauri.conf.json`):**

```diff
   "bundle": {
     "active": true,
     "targets": "all",
+    "publisher": "MultiZaloDesktop Contributors",
+    "category": "Utility",
+    "shortDescription": "Isolated profile manager for official desktop messaging instances",
+    "longDescription": "User-mode desktop profile and process isolation manager. Does not bundle, patch, or redistribute third-party proprietary messaging binaries.",
+    "license": "MIT OR Apache-2.0",
     "icon": [
       "icons/32x32.png",
       "icons/128x128.png",
+      "icons/128x128@2x.png",
+      "icons/icon.icns",
       "icons/icon.ico"
     ],
+    "windows": {
+      "certificateThumbprint": null,
+      "digestAlgorithm": "sha256",
+      "timestampUrl": "",
+      "webviewInstallMode": {
+        "type": "downloadBootstrapper",
+        "silent": true
+      },
+      "wix": {
+        "language": "en-US"
+      },
+      "nsis": {
+        "installMode": "currentUser",
+        "installerIcon": "icons/icon.ico"
+      }
+    },
+    "macOS": {
+      "minimumSystemVersion": "13.0",
+      "hardenedRuntime": true,
+      "signingIdentity": null,
+      "entitlements": null
+    },
+    "linux": {
+      "deb": {
+        "section": "utils",
+        "priority": "optional",
+        "depends": [
+          "libwebkit2gtk-4.1-0",
+          "libgtk-3-0t64",
+          "libayatana-appindicator3-1"
+        ]
+      },
+      "appimage": {
+        "bundleMediaFramework": false
+      }
+    }
   }
```

---

### IR-004 — Dual `[workspace]` / Split `Cargo.lock` & Unpinned `tauri-cli` Version

- **Severity:** Medium (Reproducibility, Cache Efficiency & Supply-Chain Governance)
- **Affected Files:**
  - `desktop-instance-manager/Cargo.toml` (lines 12–14: `exclude = ["apps/desktop/src-tauri"]`)
  - `desktop-instance-manager/apps/desktop/src-tauri/Cargo.toml` (line 29: `[workspace]`)
  - `desktop-instance-manager/Cargo.lock` (`41` packages) & `desktop-instance-manager/apps/desktop/src-tauri/Cargo.lock` (`434` packages)
- **Observed Evidence:**
  - Because `apps/desktop/src-tauri` is excluded from the parent workspace, Cargo maintains two independent dependency graphs and two separate `target/` directories (`desktop-instance-manager/target` and `apps/desktop/src-tauri/target`).
  - Version skew already appears across the two graphs: `desktop-instance-manager/Cargo.lock` resolves `sha2 v0.10.9`, `thiserror v2.0.21`, `windows-sys v0.61.2`, and `getrandom v0.4.3`, whereas `apps/desktop/src-tauri/Cargo.lock` resolves duplicate major/minor versions alongside them (`sha2 v0.10.9` + `v0.11.1`, `thiserror v1.0.69` + `v2.0.21`, `windows-sys v0.45.0` + `v0.61.2`, `getrandom v0.3.4` + `v0.4.3`).
  - Additionally, `tauri-cli` (`cargo-tauri`) is not pinned in a repo manifest or toolchain file; running unpinned `cargo install tauri-cli` at packaging time can pull newer `2.x` bundler versions over time.
- **Proposed Options & Recommendation:**
  1. **Option A (Keep Split Workspace on Linux-less Hosts, Pin `tauri-cli`):** Keep `exclude = ["apps/desktop/src-tauri"]` if contributors need `cargo test --workspace` on Linux hosts without `libwebkit2gtk-4.1-dev` installed, **but** explicitly audit both lockfiles in every CI/audit script (already implemented in `scripts/ci-cross-platform/dependency-audit.mjs`) and pin `TAURI_CLI_VERSION="2.5.0"` (installed with `--version "=2.5.0" --locked`) in CI workflows.
  2. **Option B (Unified Workspace):** Remove `exclude` and `src-tauri`'s `[workspace]`, use `default-members` on core crates so `cargo test` without `--workspace` skips GTK/WebKitGTK when desired, and maintain a single root `Cargo.lock`.

---

### IR-005 — Missing Workspace Lint Parity in `apps/desktop/src-tauri/Cargo.toml`

- **Severity:** Medium (Static Safety & IPC Boundary Hardening)
- **Affected Files:**
  - `desktop-instance-manager/apps/desktop/src-tauri/Cargo.toml`
  - `desktop-instance-manager/apps/desktop/src-tauri/src/main.rs` (lines 8, 27)
- **Observed Evidence:**
  - `desktop-instance-manager/Cargo.toml` enforces `unsafe_op_in_unsafe_fn = "deny"`, `missing_docs = "warn"`, `clippy::unwrap_used = "warn"`, and `clippy::expect_used = "warn"`, inherited via `[lints] workspace = true` in all 7 workspace crates.
  - Because `apps/desktop/src-tauri` is an excluded standalone workspace, it does **not** inherit `[workspace.lints]` and currently defines no `[lints]` section. Consequently, `cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml -- -D warnings` does not flag `.expect(...)` calls in `src/main.rs` or guard future `unsafe` blocks in `src-tauri`.
- **Proposed Change (`apps/desktop/src-tauri/Cargo.toml`):**

```diff
+[lints.rust]
+unsafe_op_in_unsafe_fn = "deny"
+
+[lints.clippy]
+all = { level = "warn", priority = -1 }
+unwrap_used = "warn"
+expect_used = "warn"
```
*(Accompanied by structured startup error exit in `apps/desktop/src-tauri/src/main.rs` instead of `.expect(...)`.)*

---

### IR-006 — Explicit `[profile.release]` Optimization, MSVC `GUARD_CF`, & GNU `.rsrc` Manifest Merge Fix

- **Severity:** Low (Binary Footprint, Hardening & Toolchain Diagnostics)
- **Affected Files:**
  - `desktop-instance-manager/Cargo.toml`
  - `desktop-instance-manager/apps/desktop/src-tauri/Cargo.toml`
- **Observed Evidence:**
  - Neither root `Cargo.toml` nor `apps/desktop/src-tauri/Cargo.toml` defines `[profile.release]`. On local Windows UCRT GNU builds (`x86_64-pc-windows-gnu`), unstripped DWARF symbol sections leave `dim-desktop.exe` at `21,586,008` bytes (`dim-cli.exe` `2,209,966` bytes, `dim-test-fixture.exe` `1,616,671` bytes).
  - Direct PE inspection of `apps/desktop/src-tauri/target/x86_64-pc-windows-gnu/release/dim-desktop.exe` revealed why MinGW Binutils `ld.exe` (`2.47`) emitted `warning: linker stderr: ... ld.exe: .rsrc merge failure: multiple non-default manifests`: both Rust's MinGW target linker driver and `tauri-winres v0.3.7` / `embed-resource v3.0.12` (`build.rs`) emit an `RT_MANIFEST` (`type 24, name 1`), causing GNU `ld.exe` to retain only the first manifest (`<longPathAware>true</longPathAware>` + `<requestedExecutionLevel level="asInvoker" uiAccess="false"/>`) and drop Tauri's second manifest block (`PerMonitorV2` DPI awareness + Windows 10/11 `<supportedOS>` GUIDs). Canonical CI builds on `x86_64-pc-windows-msvc` (`link.exe` + `rc.exe`/`mt.exe`) embed Tauri's full manifest cleanly and also support Control Flow Guard (`IMAGE_DLLCHARACTERISTICS_GUARD_CF` `0x4000`, compared to `0x0160` `HIGH_ENTROPY_VA | DYNAMIC_BASE | NX_COMPAT` on GNU `ld.exe`).
- **Proposed Change (both `Cargo.toml` roots):**

```toml
[profile.release]
lto = "thin"
codegen-units = 1
strip = "symbols"
panic = "abort"
```
*(Note: preserve PDB / split debug symbols in CI `x86_64-pc-windows-msvc` release builds prior to stripping if crash symbolication is required, and consider `-C control-flow-guard=checks` in `.cargo/config.toml` for `x86_64-pc-windows-msvc`.)*

---

### IR-007 — Existing `.github/workflows/ci.yml` Ubuntu 24.04 Package & `--locked` Alignment

- **Severity:** Medium (CI Maintenance & Determinism)
- **Affected File:** `.github/workflows/ci.yml` (lines 43, 55, 126, 160, 166–172)
- **Observed Evidence:**
  - Line 43 caches `desktop-instance-manager/target/` keyed only on `hashFiles('desktop-instance-manager/**/Cargo.toml', 'desktop-instance-manager/Cargo.lock')`, omitting `desktop-instance-manager/apps/desktop/src-tauri/Cargo.lock`.
  - Line 160 installs `libappindicator3-dev` on `ubuntu-latest`; on Ubuntu 24.04 LTS (`noble`), `libayatana-appindicator3-dev` is the canonical package required alongside `libwebkit2gtk-4.1-dev`.
  - Lines 49, 52, 55, 126, and 166–172 invoke `cargo check`, `cargo clippy`, `cargo test`, and `cargo build` without `--locked`, allowing `Cargo.lock` drift to pass unnoticed if a `Cargo.toml` edit is pushed without regenerating lockfiles.
- **Proposed Change (`.github/workflows/ci.yml`):**
  - Replace `libappindicator3-dev` with `libayatana-appindicator3-dev` in `tauri-desktop-check`.
  - Append `--locked` to every `cargo check`, `cargo clippy`, `cargo test`, and `cargo build` invocation.
  - Include `'desktop-instance-manager/apps/desktop/src-tauri/Cargo.lock'` in Cargo cache `hashFiles(...)` expressions.
