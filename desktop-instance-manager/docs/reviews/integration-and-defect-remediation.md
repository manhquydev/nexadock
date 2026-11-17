# Integration & Defect Remediation Report

> Integration branch: `integration/reliability-hardening`
> Report generated: 2026-11-18
> Integration HEAD: `ab04901` (final docs commit)

---

## 1. Integrated Branches

| Branch | Source Commit | Merge Commit | Agent | Content |
| :--- | :---: | :---: | :--- | :--- |
| `test/cross-platform-reliability` (C2) | `5e802e0` | `5fa798c` | Agent C2 | Cross-platform reliability test harness, OS-native fixture runner |
| `research/desktop-compatibility` (D2) | `0e5d44c` | `155efc3` | Agent D2 | Compatibility triage, H1-H8 test specs, integration backlog |
| `build/cross-platform-distribution` (E2) | `6c4150e` | `ec46209` | Agent E (ci-lead) | CI workflow, packaging scripts, docs, IR patches, build reports |

Merge base for all branches: `a5feb4a` (local `main`).

## 2. Sibling Branch Preservation Audit

| Worktree | Branch | HEAD | Status |
| :--- | :--- | :---: | :--- |
| `zalo` | `main` | `a5feb4a` | **Untouched** (read-only source) |
| `zalo-cross-platform-build` | `build/cross-platform-distribution` | `6c4150e` | **Untouched** (E2 final) |
| `zalo-macos-worktree` | `feat/macos-multi-instance` | `8628224` | **Untouched** (A2; committed since `fb2abcf`, 0 dirty files) |
| `zalo-ubuntu-worktree` | `feat/ubuntu-multi-instance` | `62dc5d1` | **Untouched** |
| `zalo-ui-worktree` | `feat/ui-vi-en-quiet-workspace` | `0f87302` | **Untouched** |
| `zalo-cross-platform-tests` | `test/cross-platform-reliability` | `5e802e0` | **Untouched** |
| `zalo-compatibility-research` | `research/desktop-compatibility` | `0e5d44c` | **Untouched** |

### Pre-Existing Uncommitted Changes in `zalo-integration`

The following files were found uncommitted in `zalo-integration` prior to E2 intake (NOT created by this agent):

- `M desktop-instance-manager/crates/core/src/lib.rs` (adds `pub mod fs_lock; pub mod lease;`)
- `M desktop-instance-manager/crates/core/src/models.rs`
- `?? desktop-instance-manager/crates/core/src/fs_lock.rs`
- `?? desktop-instance-manager/crates/core/src/lease.rs`

These files introduce a `write_runtime_lease` function with a type error (`Result<(), DimError>` vs `Result<(), io::Error>`) that blocks `cargo test --workspace --all-targets --locked`. They are NOT part of this integration and were left untouched.

---

## 3. IR Patch Review Decisions

| IR | Title | Decision | Evidence |
| :--- | :--- | :--- | :--- |
| **IR-001** | Align workspace MSRV to 1.90 | **REJECT** | C2's `tests/cross-platform/Cargo.toml` declares `rust-version = "1.85"` and depends on workspace crates. Raising workspace MSRV to `1.90` creates an MSRV conflict with C2's test crate. `cargo metadata` confirms root workspace crates do NOT require `1.90`. |
| **IR-002** | macOS icns and Retina icons | **ACCEPT** | Applied as commit `f83059e`. Copied `128x128@2x.png` (256×256 RGBA) and `icon.icns` (ic07 128×128 + ic08 256×256) to `src-tauri/icons/`, added to `tauri.conf.json` `bundle.icon`. Deep-validated: PNG chunk CRC32, `zlib.inflateSync`, RGBA pixel byte count (`height * (1 + width * 4)`) all verified. |
| **IR-003** | Tauri OS bundle metadata | **DEFER** | Untested OS bundle fields (`timestampUrl: ""`, `hardenedRuntime: true` with `signingIdentity: null`, WiX/NSIS overrides) not bundle-tested on target OS runners. Deferred until Wave 1/2 runners verify bundles. |
| **IR-004** | Pin tauri-cli and lockfile reproducibility | **PARTIAL ACCEPT** | Applied as commit `a7f1413`. Only `deny.toml` (`version = 2` removal for cargo-deny ≥0.20) + `src-tauri/Cargo.toml` (`publish = false`, `[package.metadata.packaging] tauri-cli = "2.12.5"`). Omitted `rust-toolchain.toml` to avoid overriding developer host toolchains. |
| **IR-005** | src-tauri workspace lints parity | **REJECT** | Modifies `src/main.rs` and adds crate-wide lints with high merge-conflict risk against sibling UI/platform branches for zero functional gain. |
| **IR-006** | Release profile and Windows manifest | **REJECT** | `panic = "abort"` breaks `catch_unwind`/`#[should_panic]` tests. `lto = "thin"` + `codegen-units = 1` inflates CI build cost. `WindowsAttributes::app_manifest` in `build.rs` unverified on MSVC (`windows-2022`). |
| **IR-007** | ci.yml --locked and Ubuntu 24.04 ayatana | **ACCEPT** | Applied as commit `5616bcb`. Replaced `libappindicator3-dev` with `libayatana-appindicator3-dev`, added `--locked` to all cargo commands, included `src-tauri/Cargo.lock` in cache key hash. |

---

## 4. JSON Merge Patch Critical Fix (IR-002)

**Problem:** `tauri-ci-bundle-overlay.json` contained an `"icon": [...]` array. When used with `tauri build --config <overlay>`, RFC 7396 JSON Merge Patch **replaces** (clobbers) the entire target array rather than merging. This would have silently overwritten `tauri.conf.json`'s `bundle.icon`, `bundle.resources`, or `bundle.externalBin` arrays.

**Fix:** Removed the `"icon"` array from `tauri-ci-bundle-overlay.json`. The overlay now contains **zero array fields** that could clobber source-of-truth arrays. Icons are defined exclusively in `tauri.conf.json` `bundle.icon`.

**Verification:** `selftest.mjs` updated to assert `overlay.bundle.icon === undefined` and verify icons from `tauri.conf.json` instead. 16/16 tests pass.

---

## 5. Local Native Test & C2 Reliability Suite Results

### Environment
- **Host:** Windows x86_64, Rust 1.100.0 (`x86_64-pc-windows-gnu`, WinLibs MinGW UCRT64)
- **Missing:** MSVC Build Tools, `cargo-tauri`, WiX, NSIS, `dlltool.exe`

### Test Results

| Command | Exit Code | Notes |
| :--- | :---: | :--- |
| `cargo check --workspace` | `0` | All 7 crates check OK |
| `cargo test --workspace --all-targets --locked` | `101` | Pre-existing: (1) `dim-core/lease.rs` type mismatch from uncommitted changes (not ours); (2) MinGW linker cannot find `rstrtmgr.lib`/`wintrust.lib` (MSVC-only system libs). Both issues reproduce identically on `main`. |
| `cargo test --manifest-path tests/cross-platform/Cargo.toml` | `101` | Pre-existing: `dlltool.exe` not found (requires MSVC/Windows SDK). C2's tests are designed for GitHub Actions MSVC runners. |
| `cargo deny --manifest-path Cargo.toml check -W wildcard` | `0` | advisories ok, bans ok, licenses ok, sources ok |
| `cargo deny --manifest-path apps/desktop/src-tauri/Cargo.toml check -W wildcard` | `0` | advisories ok, bans ok, licenses ok, sources ok |
| `cargo deny --manifest-path tests/cross-platform/Cargo.toml check -W wildcard` | `0` | advisories ok, bans ok, licenses ok, sources ok |
| `validate-workflow.mjs` | `0` | 6/6 checks pass |
| `selftest.mjs` | `0` | 16/16 tests pass |

### RustSec Advisory DB
- **Revision:** `233155b4025b854c68a4b199c0e32d7fd53291a2`
- **Freshness:** `2026-11-17 11:14:35 +0100`
- **Result:** `0 advisories` across all 3 lockfiles
- **Scope Disclaimer:** `0 advisories` is a point-in-time snapshot of known published advisories in the RustSec advisory-db, NOT a guarantee of zero vulnerabilities. Status: `SELF_REPORTED_UNATTESTED`.

---

## 6. Integration Commit History

| SHA | Message | Files |
| :---: | :--- | :---: |
| `ec46209` | Merge branch 'build/cross-platform-distribution' (E2 `6c4150e`) | 32 |
| `f83059e` | `fix(icons): add macOS icns/retina icons, remove overlay icon array (IR-002)` | 4 |
| `a7f1413` | `fix(supply-chain): remove deny.toml version=2 for cargo-deny 0.21, pin tauri-cli contract (IR-004 targeted)` | 2 |
| `5616bcb` | `fix(ci): add --locked, libayatana-appindicator3-dev, and src-tauri lockfile hash (IR-007)` | 1 |
| `67b7447` | `ci(integration): add C2 reliability harness, fix billing to USD-based, remove ,user scope (Step 4)` | 7 |
| `bbdef15` → `ab04901` | `docs(reviews): add integration remediation and CI execution reports` | 2 |
