# Integration & Defect Remediation Report

> Integration branch: `integration/reliability-hardening`
> Report generated: `2026-11-18`
> Verified Code & Manifest Commit: `20d1729c02011762964914d3696b63656e88120d` (followed only by this documentation commit)
> Integrated Binary Build Commit (`build_commit_sha`): `8aa7a6210871666309667a9a5f0a3efa06d0ddd3`

---

## 1. Integrated Branches & Defect Fixes

| Branch / Stream | Source Commit | Merge / Landing Commit | Owner | Content |
| :--- | :---: | :---: | :--- | :--- |
| `test/cross-platform-reliability` (C2) | `5e802e0` | `5fa798c` | Agent C2 | Cross-platform 55-case reliability & contract test harness (`xf-test-runner`), synthetic `xf-desktop-fixture` |
| `research/desktop-compatibility` (D2) | `0e5d44c` | `155efc3` | Agent D2 | Evidence triage, H1–H8 runtime test specs, prioritized integration backlog |
| `build/cross-platform-distribution` (E2) | `6c4150e` | `ec46209` | Agent E (`ci-lead`) | Manual-only 7-job native CI workflow, provenance/audit scripts, packaging docs, atomic `IR-001..0007` patches |
| P0 Reliability Defect Stream (`DEFECT-03/04/06/08c/09`) | — | `e53a0dc`, `5f95211`, `186e922`, `7024a1f` | Reliability Fixer | Per-profile launch serialization, atomic `profiles.json` writes, pre-lock startup window, junction delete ownership, fail-closed CLI parity |

Common merge base across all streams: `a5feb4a` (local `main`).

---

## 2. Sibling Worktree Preservation Audit

All 7 non-integration worktrees were audited read-only and remain **100% untouched**:

| Worktree | Branch | HEAD | Preservation Status |
| :--- | :--- | :---: | :--- |
| `zalo` | `main` | `a5feb4a` | **Untouched** (read-only baseline; `ahead 1` over `origin/main` `8cf4703`) |
| `zalo-cross-platform-build` | `build/cross-platform-distribution` | `6c4150e` | **Untouched** (E2 final commit) |
| `zalo-macos-worktree` | `feat/macos-multi-instance` | `8628224` | **Untouched** (Agent A2 worktree) |
| `zalo-ubuntu-worktree` | `feat/ubuntu-multi-instance` | `62dc5d1` | **Untouched** (Agent B2 worktree) |
| `zalo-ui-worktree` | `feat/ui-vi-en-quiet-workspace` | `0f87302` | **Untouched** (UI worktree) |
| `zalo-cross-platform-tests` | `test/cross-platform-reliability` | `5e802e0` | **Untouched** (Agent C2 worktree) |
| `zalo-compatibility-research` | `research/desktop-compatibility` | `0e5d44c` | **Untouched** (Agent D2 worktree) |

> **Concurrent Worktree Isolation Note:** Because a peer reliability agent actively modifies `crates/*` and `apps/desktop/src-tauri/src/*` inside `zalo-integration`, all CI intake commits used `git commit --only -- <exact-paths>` and all native Rust/Tauri/C2 builds and tests executed inside an isolated `$env:TEMP` archive exported via `git archive HEAD` (`8aa7a62`).

---

## 3. Selective IR Patch Review & Decisions (`IR-001` .. `IR-007`)

All 7 candidate patches in [`plans/reports/build/ir-patches/`](../../plans/reports/build/ir-patches/) were evaluated against the integrated C2+D2+E2 codebase:

| IR ID | Patch File | Decision | Applied Commit & Technical Evidence |
| :--- | :--- | :--- | :--- |
| **IR-001** | [`0001-IR-001-align-workspace-msrv-1.90.patch`](../../plans/reports/build/ir-patches/0001-IR-001-align-workspace-msrv-1.90.patch) | **REJECT** | C2's [`tests/cross-platform/Cargo.toml`](../../tests/cross-platform/Cargo.toml) declares `rust-version = "1.85"` and depends directly on workspace crates (`dim-core`, `dim-profile-store`, `dim-process-manager`, `dim-platform-*`). Raising workspace `rust-version` to `1.90` creates an immediate MSRV conflict, while `cargo metadata` confirms workspace crates compile cleanly under `1.85`. |
| **IR-002** | [`0002-IR-002-macos-icns-and-retina-icons.patch`](../../plans/reports/build/ir-patches/0002-IR-002-macos-icns-and-retina-icons.patch) | **ACCEPT** | Applied in `f83059e` (+ zero-array overlay completion in `8aa7a62`). Added `icons/128x128@2x.png` (`716` B, `256x256` RGBA) and `icons/icon.icns` (`1,142` B, `ic07` `128x128` + `ic08` `256x256` RGBA) to `apps/desktop/src-tauri/icons/` and `tauri.conf.json`. Deep-validated all PNG chunks (`IHDR`/`IDAT`/`IEND`), IEEE 802.3 CRC32 checksums, `zlib.inflateSync` payloads, and `height * (1 + width * 4)` RGBA scanline byte counts (`65,664` B and `262,400` B). |
| **IR-003** | [`0003-IR-003-tauri-os-bundle-metadata.patch`](../../plans/reports/build/ir-patches/0003-IR-003-tauri-os-bundle-metadata.patch) | **DEFER** | Untested OS bundle fields (`timestampUrl: ""`, `hardenedRuntime: true` with `signingIdentity: null`, WiX/NSIS installer metadata, and `deb.depends` array) have not yet been verified on remote `ubuntu-24.04`, `windows-2022`, and `macos-15` runners. Deferred until Wave 1 & Wave 2 CI runs complete. |
| **IR-004** | [`0004-IR-004-pin-tauri-cli-and-lockfile-reproducibility.patch`](../../plans/reports/build/ir-patches/0004-IR-004-pin-tauri-cli-and-lockfile-reproducibility.patch) | **PARTIAL ACCEPT** | Applied in `a7f1413` (+ workflow simplification in `8aa7a62`). Removed deprecated `version = 2` from [`deny.toml`](../../deny.toml) so `cargo-deny >= 0.21` validates repo `deny.toml` directly without temp-file `sed` stripping; added `publish = false` and `[package.metadata.packaging] tauri-cli = "2.12.5"` to [`apps/desktop/src-tauri/Cargo.toml`](../../apps/desktop/src-tauri/Cargo.toml). Omitted `rust-toolchain.toml` so developer host toolchains are not unexpectedly overridden. |
| **IR-005** | [`0005-IR-005-src-tauri-workspace-lints-parity.patch`](../../plans/reports/build/ir-patches/0005-IR-005-src-tauri-workspace-lints-parity.patch) | **REJECT** | Touches `apps/desktop/src-tauri/src/main.rs` and enforces crate-wide lints with high merge-conflict exposure against active UI/platform branches (`feat/ui-vi-en-quiet-workspace`, `feat/macos-multi-instance`, `feat/ubuntu-multi-instance`) for zero runtime reliability benefit. |
| **IR-006** | [`0006-IR-006-release-profile-and-windows-manifest.patch`](../../plans/reports/build/ir-patches/0006-IR-006-release-profile-and-windows-manifest.patch) | **REJECT** | `[profile.release] panic = "abort"` breaks `catch_unwind`/`#[should_panic]` tests under `--release`; `lto = "thin"` + `codegen-units = 1` inflates private-repo CI build minutes; custom `WindowsAttributes::app_manifest` in `build.rs` has not been tested on `windows-2022` MSVC (`rc.exe`/`mt.exe`) and already triggers `.rsrc` manifest-merge warnings on GNU `ld.exe`. |
| **IR-007** | [`0007-IR-007-ci-yml-locked-and-ubuntu2404-ayatana.patch`](../../plans/reports/build/ir-patches/0007-IR-007-ci-yml-locked-and-ubuntu2404-ayatana.patch) | **ACCEPT** | Applied in `5616bcb`. Updated [`.github/workflows/ci.yml`](../../../.github/workflows/ci.yml) to install `libayatana-appindicator3-dev` (replacing removed `libappindicator3-dev` on Ubuntu 24.04), enforce `--locked` on all Cargo commands, and hash `apps/desktop/src-tauri/Cargo.lock` in the Cargo cache key. |

---

## 4. RFC 7396 JSON Merge Patch Remediation (`tauri-ci-bundle-overlay.json`)

- **Defect:** Tauri's `--config <file>` applies RFC 7396 JSON Merge Patch semantics, where **any JSON array in the overlay completely replaces the target array** in `tauri.conf.json`. Leaving `"icon": [...]` or `"depends": [...]` inside [`scripts/ci-cross-platform/tauri-ci-bundle-overlay.json`](../../scripts/ci-cross-platform/tauri-ci-bundle-overlay.json) risked clobbering base arrays (`bundle.icon`, `bundle.resources`, `bundle.externalBin`, or future `deb.depends`).
- **Fix (`f83059e` & `8aa7a62`):** Moved `icons/128x128@2x.png` and `icons/icon.icns` permanently into [`apps/desktop/src-tauri/tauri.conf.json`](../../apps/desktop/src-tauri/tauri.conf.json) (`IR-002`) and stripped **all array keys** (`bundle.icon` and `bundle.linux.deb.depends`) from `tauri-ci-bundle-overlay.json`. Only scalar/object merge keys remain (`webviewInstallMode`, `minimumSystemVersion`, `deb.section`, `deb.priority`), while Tauri auto-resolves `.deb` `shlibs` via `ldd`/`dpkg -S` until `IR-003` is runner-verified.
- **Automated Regression Guard:** [`scripts/ci-cross-platform/selftest.mjs`](../../scripts/ci-cross-platform/selftest.mjs) recursively walks `tauri-ci-bundle-overlay.json` and asserts `arrayPaths.length === 0` (`16/16 PASS`).

---

## 5. Clean-Tree Native Windows Build, C2 Reliability Suite & Supply-Chain Verification

Executed on clean exported `git archive` of commit `8aa7a6210871666309667a9a5f0a3efa06d0ddd3` on Windows 11 Pro (`10.0.26300` x64) with `rustc 1.100.0 (0ef1bc386 2026-11-09)` (`x86_64-pc-windows-gnu`), WinLibs MinGW-w64 UCRT64 `gcc 16.2.0` + `dlltool.exe`, Node `v22.x`+, and isolated temp `APPDATA`/`LOCALAPPDATA`:

| Verification Step | Command | Exit Code | Duration | Verified Outcome |
| :--- | :--- | :---: | :---: | :--- |
| Static Workflow Gate | `node scripts/ci-cross-platform/validate-workflow.mjs` | **`0`** | `0.4s` | `6/6 PASS`; zero `\|\| true` or `continue-on-error`; macOS `10x` jobs default `false` |
| Packaging Tooling Self-Test | `node scripts/ci-cross-platform/selftest.mjs` | **`0`** | `4.2s` | `16/16 PASS`; recursive zero-array overlay check + `tauri.conf.json` icon check |
| Core Workspace Unit & Integration Tests | `cargo test --workspace --all-targets --locked` | **`0`** | `19.83s` | **29 passed, 0 failed** across all 7 workspace crates (`dim-core`..`dim-test-fixture`) |
| Tauri Desktop Shell Tests (`IR-002` icons) | `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --locked` | **`0`** | `158.38s` | **2 passed, 0 failed**; `tauri.conf.json` + `.icns`/`@2x.png` compiled cleanly |
| C2 Cross-Platform Reliability Runner | `node tests/cross-platform/scripts/run-all.mjs --cycles 5 --metrics-runs 2` | **`0`** | `144.03s` | `total=55 \| pass=47 \| defects_reproduced=6 \| fixed_candidates=5 \| skipped_host=2 \| unexpected_fail=0` (**5 P0 cases transitioned to `FIXED_CANDIDATE`!**) |
| C2 Cargo Integration Suite | `cargo test --manifest-path tests/cross-platform/Cargo.toml --locked` | **`0`** | `62.65s` | **7 passed, 0 failed, 3 ignored** (`suite_01`..`suite_07` all green) |
| C2 Native Host OS Gate (`Windows`) | `xf-test-runner.exe --case XP-WIN-01,XP-WIN-02,XP-WIN-03,XP-WIN-04 --skip-metrics --require-native-os` | **`0`** | `1.25s` | `total=4 \| pass=4 \| skipped_host=0 \| unexpected_fail=0` |
| Supply-Chain Audit (3 Lockfiles) | `cargo deny --config deny.toml --manifest-path <Cargo.toml> --all-features --locked check -W wildcard advisories bans licenses sources` | **`0` / `0` / `0`** | `5.71s` | `advisories ok, bans ok, licenses ok, sources ok` across workspace, `src-tauri`, and C2 `tests/cross-platform` (`rustsec/advisory-db` @ `233155b4025b854c68a4b199c0e32d7fd53291a2`, `2026-11-17 11:14:35 +0100`; point-in-time snapshot of known published advisories, NOT a guarantee of zero vulnerabilities; `SELF_REPORTED_UNATTESTED`) |
| Native Release Build & Provenance Manifest | `build-and-package.mjs` + `generate-artifact-manifest.mjs` (`8aa7a62`) | **`0`** | `334.48s` | `dim-desktop.exe` (`093c28961bba7ba0...`), `dim-cli.exe` (`473941b7efd87bc2...`), `dim-test-fixture.exe` (`a383eda161e4bfe8...`); updated [`artifact-validation-manifest.json`](../../plans/reports/build/artifact-validation-manifest.json) & [`SHA256SUMS.txt`](../../plans/reports/build/SHA256SUMS.txt) in `20d1729` |

---

## 6. E2 Integration Commit Ledger (`integration/reliability-hardening`)

| Commit SHA | Commit Subject | Scope |
| :---: | :--- | :--- |
| `ec46209` | `Merge branch 'build/cross-platform-distribution' (E2 6c4150e) into integration/reliability-hardening` | E2 merge (`32` files) |
| `f83059e` | `fix(icons): add macOS icns/retina icons, remove overlay icon array (IR-002)` | `IR-002` icons + `tauri.conf.json` |
| `a7f1413` | `fix(supply-chain): remove deny.toml version=2 for cargo-deny 0.21, pin tauri-cli contract (IR-004 targeted)` | Targeted `IR-004` (`deny.toml` + `src-tauri/Cargo.toml`) |
| `5616bcb` | `fix(ci): add --locked, libayatana-appindicator3-dev, and src-tauri lockfile hash (IR-007)` | `IR-007` (`.github/workflows/ci.yml`) |
| `67b7447` | `ci(integration): add C2 reliability harness, fix billing to USD-based, remove ,user scope (Step 4)` | Initial C2 CI wiring & USD/auth doc updates |
| `8aa7a62` | `fix(ci-integration): wire C2 prerequisite builds, enforce zero-array overlay, and clean auth/billing docs` | C2 fixture/CLI prereq builds, zero `\|\| true`, recursive zero-array overlay, Web UI billing check |
| `20d1729` | `chore(build-reports): refresh integrated Windows release SHA256SUMS and artifact manifest (8aa7a62)` | Fresh integrated SHA-256 digests & Schema v2 manifest |
