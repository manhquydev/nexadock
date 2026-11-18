# Out-of-Scope Integration Requests & Verified Atomic Patch Series — Multi-Zalo Cross-Platform Build & Packaging

**Document ID:** `DIM-BUILD-IR-20261118-E2`  
**Branch:** `build/cross-platform-distribution` (Base Commit on `main`: `a5feb4a520798064c1508c26fe4c4ac17a3a03cf`)  
**Scope Boundary:** Per Mission E2 worktree isolation rules, shared manifests (`Cargo.toml`, `Cargo.lock`, `deny.toml`, `apps/desktop/src-tauri/Cargo.toml`, `apps/desktop/src-tauri/tauri.conf.json`), application icons (`apps/desktop/src-tauri/icons/*`), and `.github/workflows/ci.yml` remain **untouched** in the `build/cross-platform-distribution` branch index while parallel sibling worktrees remain active. Instead:
1. CI packaging blockers (`IR-002`, `IR-003`, `IR-004`) are mitigated non-destructively at build time via [`scripts/ci-cross-platform/tauri-ci-bundle-overlay.json`](../../../scripts/ci-cross-platform/tauri-ci-bundle-overlay.json), [`scripts/ci-cross-platform/assets/icon.icns`](../../../scripts/ci-cross-platform/assets/icon.icns), [`scripts/ci-cross-platform/assets/128x128@2x.png`](../../../scripts/ci-cross-platform/assets/128x128@2x.png), and pinned `@tauri-apps/cli@2.12.5` + `cargo --locked` in `build-and-package.mjs`.
2. Every upstream repository change (`IR-001` through `IR-007`) is delivered as a **small, standalone, single-concern `git format-patch` file** under [`ir-patches/`](./ir-patches/), verified read-only via `git -C <REPO_ROOT> apply --check` both **individually** and **sequentially across all 7 patches**.

---

## 1. Sibling Worktree Conflict Audit (Verified Against `main` @ `a5feb4a`)

Read-only inspection (`git worktree list` and `git diff --name-only $(git merge-base main <branch>)..<branch>`) across all 6 active worktrees confirmed **zero direct file collisions** with `IR-001`..`IR-007`, while explaining why shared manifests remain isolated until the integration controller merges branches:

| Active Worktree Path | Branch & HEAD | Merge-Base with `main` | Files Modified by Sibling Branch | Direct Overlap with `IR-001`..`IR-007` Target Files? | Integration Ordering Note |
| :--- | :--- | :--- | :--- | :---: | :--- |
| `<REPO_ROOT>` | `main` (`a5feb4a`) | `a5feb4a` | *(clean — 0 dirty files)* | **None** | Baseline target for `git apply --check` |
| `<WORKTREE_ROOT>/zalo-ui-worktree` | `feat/ui-vi-en-quiet-workspace` (`0f87302`) | `8cf4703` | `apps/desktop/ui/**`, `docs/design/**` (`35` files) | **0 files** | Rebase/merge onto `a5feb4a` before or after IR patches without conflict |
| `<WORKTREE_ROOT>/zalo-macos-worktree` | `feat/macos-multi-instance` (`fb2abcf`) | `a5feb4a` | `crates/platform-macos/**`, `docs/platform/macos/**`, `plans/reports/macos/**`, `tests/platform-macos/**` (`17` files) | **0 files** | Orthogonal to `Cargo.toml` root, `src-tauri/`, and `ci.yml` |
| `<WORKTREE_ROOT>/zalo-ubuntu-worktree` | `feat/ubuntu-multi-instance` (`9a10a9f`) | `a5feb4a` | `crates/platform-linux/**`, `docs/platform/ubuntu/**`, `plans/reports/ubuntu/**`, `tests/platform-linux/**` (`16` files) | **0 files** | Orthogonal to `Cargo.toml` root, `src-tauri/`, and `ci.yml` |
| `<WORKTREE_ROOT>/zalo-cross-platform-tests` | `test/cross-platform-reliability` (`a214355`) | `a5feb4a` | `fixtures/cross-platform/**`, `tests/cross-platform/**`, `docs/testing/cross-platform/**`, `plans/reports/reliability/**` (`36` files) | **0 files** | Its handoff doc proposes optional root `Cargo.toml`/`ci.yml` wiring; apply `0001`/`0006`/`0007` cleanly first |
| `<WORKTREE_ROOT>/zalo-compatibility-research` | `research/desktop-compatibility` (`bf08f10`) | `a5feb4a` | `docs/research/compatibility/**`, `plans/reports/compatibility/**` (`6` files) | **0 files** | Documentation-only branch |

---

## 2. Verified Atomic Patch Series (`plans/reports/build/ir-patches/`)

Every patch below was generated from clean `a5feb4a520798064c1508c26fe4c4ac17a3a03cf` with non-overlapping diff hunks so that an integrator can apply **any single patch standalone** OR **all 7 patches in sequence (`0001` -> `0007`)** without merge conflicts:

| Patch File | IR ID & Severity | Target File(s) Touched | Diffstat & Size | Standalone `git apply --check` (`main`) | Sequential 7-Patch `git apply --check` (`main`) |
| :--- | :--- | :--- | :--- | :---: | :---: |
| [`0001-IR-001-align-workspace-msrv-1.90.patch`](./ir-patches/0001-IR-001-align-workspace-msrv-1.90.patch) | **IR-001** (Medium) | `desktop-instance-manager/Cargo.toml`, `desktop-instance-manager/docs/architecture/adr-002-rust-tauri-and-sac-host-bridge.md` | `2 files, +2 / -2` (`2,628` B) | **PASS (`exit 0`)** | **PASS (`exit 0`)** |
| [`0002-IR-002-macos-icns-and-retina-icons.patch`](./ir-patches/0002-IR-002-macos-icns-and-retina-icons.patch) | **IR-002** (High) | `desktop-instance-manager/apps/desktop/src-tauri/icons/icon.icns` (`1,142` B binary), `icons/128x128@2x.png` (`716` B binary), `tauri.conf.json` | `3 files, +2` + 2 binary assets (`2,618` B) | **PASS (`exit 0`)** | **PASS (`exit 0`)** |
| [`0003-IR-003-tauri-os-bundle-metadata.patch`](./ir-patches/0003-IR-003-tauri-os-bundle-metadata.patch) | **IR-003** (High) | `desktop-instance-manager/apps/desktop/src-tauri/tauri.conf.json` | `1 file, +41 / -0` (`2,374` B) | **PASS (`exit 0`)** | **PASS (`exit 0`)** |
| [`0004-IR-004-pin-tauri-cli-and-lockfile-reproducibility.patch`](./ir-patches/0004-IR-004-pin-tauri-cli-and-lockfile-reproducibility.patch) | **IR-004** (Medium) | `desktop-instance-manager/apps/desktop/src-tauri/Cargo.toml`, `desktop-instance-manager/deny.toml`, `desktop-instance-manager/rust-toolchain.toml` | `3 files, +11 / -2` (`2,408` B) | **PASS (`exit 0`)** | **PASS (`exit 0`)** |
| [`0005-IR-005-src-tauri-workspace-lints-parity.patch`](./ir-patches/0005-IR-005-src-tauri-workspace-lints-parity.patch) | **IR-005** (Medium) | `desktop-instance-manager/apps/desktop/src-tauri/Cargo.toml`, `desktop-instance-manager/apps/desktop/src-tauri/src/main.rs` | `2 files, +19 / -3` (`2,458` B) | **PASS (`exit 0`)** | **PASS (`exit 0`)** |
| [`0006-IR-006-release-profile-and-windows-manifest.patch`](./ir-patches/0006-IR-006-release-profile-and-windows-manifest.patch) | **IR-006** (Low) | `desktop-instance-manager/Cargo.toml`, `desktop-instance-manager/apps/desktop/src-tauri/.cargo/config.toml`, `desktop-instance-manager/apps/desktop/src-tauri/build.rs` | `3 files, +55 / -1` (`3,816` B) | **PASS (`exit 0`)** | **PASS (`exit 0`)** |
| [`0007-IR-007-ci-yml-locked-and-ubuntu2404-ayatana.patch`](./ir-patches/0007-IR-007-ci-yml-locked-and-ubuntu2404-ayatana.patch) | **IR-007** (Medium) | `.github/workflows/ci.yml` | `1 file, +9 / -9` (`3,360` B) | **PASS (`exit 0`)** | **PASS (`exit 0`)** |

### Integrator Verification & Application Commands

```powershell
# Read-only verification against main (already executed & verified 0 errors):
git -C <REPO_ROOT> apply --check `
  <WORKTREE_ROOT>/zalo-cross-platform-build/desktop-instance-manager/plans/reports/build/ir-patches/*.patch

# Apply as individual reviewable commits on an integration branch when authorized:
git am <WORKTREE_ROOT>/zalo-cross-platform-build/desktop-instance-manager/plans/reports/build/ir-patches/*.patch
```

---

## 3. Detailed Integration Specifications

### IR-001 — Rust MSRV Alignment (`1.85` -> `1.90`)
- **Patch:** [`ir-patches/0001-IR-001-align-workspace-msrv-1.90.patch`](./ir-patches/0001-IR-001-align-workspace-msrv-1.90.patch)
- **Problem:** `desktop-instance-manager/Cargo.toml` sets `rust-version = "1.85"` while `apps/desktop/src-tauri/Cargo.toml` sets `rust-version = "1.90"` (`tauri v2.12.6`, `sha2 v0.11.1`, `wry v0.57.2`), and `adr-002-rust-tauri-and-sac-host-bridge.md` cites historical MSRV `1.78`.
- **Resolution:** Updates `[workspace.package] rust-version = "1.90"` in `desktop-instance-manager/Cargo.toml` and aligns ADR-002 line 37 to `1.90`.

### IR-002 — macOS `icon.icns` & Retina `128x128@2x.png` Bundle Icons
- **Patch:** [`ir-patches/0002-IR-002-macos-icns-and-retina-icons.patch`](./ir-patches/0002-IR-002-macos-icns-and-retina-icons.patch)
- **Active CI Mitigation Today:** [`scripts/ci-cross-platform/tauri-ci-bundle-overlay.json`](../../../scripts/ci-cross-platform/tauri-ci-bundle-overlay.json) injects [`scripts/ci-cross-platform/assets/icon.icns`](../../../scripts/ci-cross-platform/assets/icon.icns) (`1,142` B, `icns` magic `0x69636e73` with `ic07` `128x128` PNG + `ic08` `256x256` PNG) and [`scripts/ci-cross-platform/assets/128x128@2x.png`](../../../scripts/ci-cross-platform/assets/128x128@2x.png) (`716` B, `256x256` 8-bit RGBA) via `tauri build --config`.
- **Upstream Resolution:** Installs `icon.icns` and `128x128@2x.png` directly into `apps/desktop/src-tauri/icons/` (strictly original Multi-Zalo Desktop icon geometry — zero VNG branding) and adds `"icons/128x128@2x.png"` and `"icons/icon.icns"` to `tauri.conf.json` `bundle.icon`.

### IR-003 — Explicit OS Bundle Configuration in `tauri.conf.json`
- **Patch:** [`ir-patches/0003-IR-003-tauri-os-bundle-metadata.patch`](./ir-patches/0003-IR-003-tauri-os-bundle-metadata.patch)
- **Active CI Mitigation Today:** Provided via `tauri-ci-bundle-overlay.json` passed to `tauri build --config`.
- **Upstream Resolution (Validated against `tauri-utils v2.10.3` `#[serde(rename_all = "camelCase", deny_unknown_fields)]`):**
  - `bundle.publisher`, `bundle.category = "Utility"`, `bundle.shortDescription`, `bundle.longDescription`, `bundle.license = "MIT OR Apache-2.0"`.
  - `bundle.windows`: `digestAlgorithm = "sha256"`, `webviewInstallMode = { "type": "downloadBootstrapper", "silent": true }`, `wix = { "language": "en-US" }`, `nsis = { "installMode": "currentUser", "installerIcon": "icons/icon.ico" }` (preserving `asInvoker` non-elevated user-scope installation).
  - `bundle.macOS`: `minimumSystemVersion = "11.0"`, `hardenedRuntime = true`, `signingIdentity = null`, `entitlements = null`.
  - `bundle.linux`: `deb = { "section": "utils", "priority": "optional", "depends": ["libwebkit2gtk-4.1-0", "libgtk-3-0t64 | libgtk-3-0", "libayatana-appindicator3-1 | libappindicator3-1"] }`, `appimage = { "bundleMediaFramework": false }`.

### IR-004 — `@tauri-apps/cli@2.12.5` Pinning, Dual-Lockfile Contract & `cargo-deny 0.21` Schema Fix
- **Patch:** [`ir-patches/0004-IR-004-pin-tauri-cli-and-lockfile-reproducibility.patch`](./ir-patches/0004-IR-004-pin-tauri-cli-and-lockfile-reproducibility.patch)
- **Observed Evidence:**
  1. `desktop-instance-manager/Cargo.lock` (`41` crates) and `apps/desktop/src-tauri/Cargo.lock` (`434` crates; `439` unique crates) intentionally remain split so core CLI/library crates compile without Linux WebKitGTK headers, requiring explicit `--locked` and dual-lockfile auditing.
  2. Live execution of **`cargo-deny 0.21.0`** revealed two upstream config issues: `deny.toml:2` contains deprecated top-level `version = 2` (`error[unexpected-keys]` in `cargo-deny >= 0.20`), and `apps/desktop/src-tauri/Cargo.toml` lacks `publish = false`, triggering `wildcards = "deny"` on unversioned internal path dependencies (`dim-core`, `dim-profile-store`, `dim-process-manager`, `dim-platform-*`).
- **Resolution:** Removes `version = 2` from `deny.toml`, sets `publish = false` and `[package.metadata.packaging]` (`tauri_cli_exact_version = "2.12.5"`, `cargo_locked_required = true`) in `apps/desktop/src-tauri/Cargo.toml`, and adds `desktop-instance-manager/rust-toolchain.toml`.

### IR-005 — Workspace Lint Parity in `apps/desktop/src-tauri`
- **Patch:** [`ir-patches/0005-IR-005-src-tauri-workspace-lints-parity.patch`](./ir-patches/0005-IR-005-src-tauri-workspace-lints-parity.patch)
- **Resolution:** Adds `[lints.rust]` (`unsafe_op_in_unsafe_fn = "deny"`) and `[lints.clippy]` (`all = "warn"`, `unwrap_used = "warn"`, `expect_used = "warn"`) to `apps/desktop/src-tauri/Cargo.toml`, and replaces both startup `.expect(...)` calls in `apps/desktop/src-tauri/src/main.rs` with explicit error logging and `std::process::exit(1)`.

### IR-006 — `[profile.release]` LTO/Strip, MSVC `GUARD_CF`, & Explicit Windows Manifest (`PerMonitorV2` + Win10/11 GUIDs)
- **Patch:** [`ir-patches/0006-IR-006-release-profile-and-windows-manifest.patch`](./ir-patches/0006-IR-006-release-profile-and-windows-manifest.patch)
- **Observed Evidence:** Inspection of `tauri-build v2.7.5` (`windows-app-manifest.xml`) and compiled `dim-desktop.exe` confirmed Tauri's default resource only declares Common Controls v6 while `rustc` injects a separate `asInvoker` + `longPathAware` manifest (causing MinGW `ld.exe` `.rsrc merge failure: multiple non-default manifests` and omitting `PerMonitorV2` + Windows 10/11 `<supportedOS>` GUIDs). Furthermore, unstripped GNU release builds measure `21.59 MB` and lack `IMAGE_DLLCHARACTERISTICS_GUARD_CF` (`0x4000`).
- **Resolution:** Adds `[profile.release]` (`lto = "thin"`, `codegen-units = 1`, `strip = "symbols"`, `panic = "abort"`) to `Cargo.toml` and `apps/desktop/src-tauri/.cargo/config.toml`, enables `-C control-flow-guard=checks` on `x86_64-pc-windows-msvc`, and configures `tauri_build::WindowsAttributes::new().app_manifest(...)` in `apps/desktop/src-tauri/build.rs` with Common Controls v6, `asInvoker` (`uiAccess="false"`), `PerMonitorV2`, `longPathAware`, and Windows 10/11 compatibility GUID `{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}`.

### IR-007 — `.github/workflows/ci.yml` `--locked`, Dual-Lockfile Cache Key, & Ubuntu 24.04 Ayatana Package
- **Patch:** [`ir-patches/0007-IR-007-ci-yml-locked-and-ubuntu2404-ayatana.patch`](./ir-patches/0007-IR-007-ci-yml-locked-and-ubuntu2404-ayatana.patch)
- **Resolution:** Adds `desktop-instance-manager/apps/desktop/src-tauri/Cargo.lock` to the Cargo `actions/cache@v4` `hashFiles(...)` key, replaces legacy `libappindicator3-dev` with `libayatana-appindicator3-dev` for Ubuntu 24.04 runners, and appends `--locked` to every `cargo check`, `cargo clippy`, `cargo test`, and `cargo build` command in `.github/workflows/ci.yml`.
