# CI Integration & Execution Report

> Integration branch: `integration/reliability-hardening`
> Report generated: 2026-11-18
> Final Integration HEAD: `ab04901` (docs commit on `integration/reliability-hardening`)

---

## 1. Integration HEAD & E2 Source SHA

- **Integration HEAD:** `ab04901` on `integration/reliability-hardening`
- **E2 Source SHA:** `6c4150e` on `build/cross-platform-distribution` (merged at `ec46209`)
- **Merge base:** `a5feb4a` (local `main`, 1 commit ahead of `origin/main` at `8cf4703`)
- **C2 merged at:** `5fa798c` (source `5e802e0`)
- **D2 merged at:** `155efc3` (source `0e5d44c`)

---

## 2. Applied Patches (3)

| IR | Commit | Scope | Verification |
| :--- | :---: | :--- | :--- |
| **IR-002** | `f83059e` | `icons/128x128@2x.png`, `icons/icon.icns` → `src-tauri/icons/`; `tauri.conf.json` `bundle.icon` updated; overlay `icon` array REMOVED (JSON Merge Patch fix) | Deep PNG validation: CRC32, zlib inflate, RGBA pixel byte count verified for ic07 (128×128), ic08 (256×256), and standalone 128x128@2x.png (256×256). `selftest.mjs` asserts overlay has zero icon array. |
| **IR-004** (targeted) | `a7f1413` | `deny.toml` (`version = 2` removed); `src-tauri/Cargo.toml` (`publish = false`, `[package.metadata.packaging] tauri-cli = "2.12.5"`) | `cargo deny check` exit 0 on all 3 manifests. `rust-toolchain.toml` omitted intentionally. |
| **IR-007** | `5616bcb` | `.github/workflows/ci.yml`: `libayatana-appindicator3-dev`, `--locked`, `src-tauri/Cargo.lock` cache hash | `validate-workflow.mjs` 6/6 pass. Ubuntu 24.04 package name verified against `apt-cache show`. |

## 3. Rejected/Deferred Patches (4)

| IR | Decision | Evidence |
| :--- | :--- | :--- |
| **IR-001** (MSRV 1.90) | **REJECT** | C2's `tests/cross-platform/Cargo.toml` declares `rust-version = "1.85"`. Raising workspace MSRV creates immediate conflict. |
| **IR-003** (OS bundle metadata) | **DEFER** | Untested on target runners. `timestampUrl: ""`, `hardenedRuntime: true` with null signing identity, WiX/NSIS overrides need Wave 1/2 verification. |
| **IR-005** (workspace lints) | **REJECT** | High merge-conflict risk against `feat/macos-multi-instance`, `feat/ubuntu-multi-instance`. Zero functional gain. |
| **IR-006** (release profile) | **REJECT** | `panic = "abort"` breaks `#[should_panic]` tests. `lto = "thin"` + `codegen-units = 1` inflates CI cost. `WindowsAttributes::app_manifest` unverified on MSVC. |

---

## 4. Workflow Registration Status

**Status: `NOT_REGISTERED_ON_DEFAULT_BRANCH`**

`.github/workflows/cross-platform-validation.yml` exists on `integration/reliability-hardening` but NOT on `origin/main` (`8cf4703`). Per GitHub Actions platform rules, `workflow_dispatch` triggers are only indexed for workflow files on the **default branch**.

### 8-Step Safe Default-Branch Registration Plan

1. Run `gh auth refresh -h github.com -s workflow` (interactive OAuth consent)
2. Pre-flight private-repo quota check (`gh api users/manhquydev/settings/billing/usage`)
3. Fast-forward `origin/main` to local `main` (`a5feb4a`) via `git push origin main`
4. Create feature branch from `integration/reliability-hardening` → `main` PR
5. Human review + squash/merge to `main`
6. Verify `gh workflow list --repo manhquydev/zalo` shows the new workflow
7. Dispatch Wave 1 (Ubuntu + Windows): `gh workflow run cross-platform-validation.yml`
8. If Wave 1 passes, opt-in dispatch Wave 2 (macOS arm64 + Intel)

---

## 5. Credential Blocker

**Status: `AWAITING_USER_AUTHORIZATION`**

Current `gh` token scopes: `'gist', 'read:org', 'repo'` — **missing `'workflow'`**.

```powershell
& "C:\Program Files\GitHub CLI\gh.exe" auth refresh -h github.com -s workflow
```

This requires interactive browser OAuth consent. The `workflow` scope is needed to push `.github/workflows/*.yml` files to `origin`.

> **Note:** Only `workflow` scope is requested. The `,user` scope was removed from all documentation (unnecessary for CI execution; billing API visibility is a convenience, not a requirement).

---

## 6. Run IDs

**None — blocked pre-push by `AWAITING_USER_AUTHORIZATION`.**

Zero remote GitHub Actions runs have been executed. `gh run list --repo manhquydev/zalo` returns `0` runs. `gh workflow list --repo manhquydev/zalo` returns `0` registered workflows.

---

## 7. Native Job Results

### Remote CI

| Job | Status |
| :--- | :--- |
| Wave 1: `ubuntu-2404-build` | `NOT_EXECUTED` |
| Wave 1: `windows-msvc-build` | `NOT_EXECUTED` |
| Wave 2: `macos-arm64-build` | `NOT_EXECUTED` |
| Wave 2: `macos-intel-build` | `NOT_EXECUTED` |
| Wave 3: `artifact-verification` | `NOT_EXECUTED` |

### Local Windows Native Verification

| Test | Exit | Notes |
| :--- | :---: | :--- |
| `cargo check --workspace` | `0` | All 7 crates compile |
| `cargo test --workspace --all-targets --locked` | `101` | Pre-existing: (1) uncommitted `lease.rs` type error (not ours); (2) MinGW linker missing `rstrtmgr.lib`/`wintrust.lib` (MSVC-only). Both reproduce on `main`. |
| `cargo test --manifest-path tests/cross-platform/Cargo.toml` | `101` | Pre-existing: `dlltool.exe` not found (requires MSVC/Windows SDK). C2 tests target MSVC runners. |
| `validate-workflow.mjs` | `0` | 6/6 invariants pass |
| `selftest.mjs` | `0` | 16/16 tests pass |

> **Limitation:** Local host uses `x86_64-pc-windows-gnu` (MinGW UCRT64), not `x86_64-pc-windows-msvc`. Full workspace `cargo test --all-targets` requires MSVC Build Tools (`cl.exe`/`link.exe`). The CI workflow correctly uses `windows-2022` (MSVC) runners.

---

## 8. Artifacts & SHA-256 Hashes

No artifacts were built on the integrated `zalo-integration` HEAD because:
1. `cargo-tauri` is not installed on the local host
2. Local host lacks MSVC Build Tools needed for `x86_64-pc-windows-msvc` target

Artifact manifests from E2 (committed on `build/cross-platform-distribution`) remain at:
- [`plans/reports/build/artifact-validation-manifest.json`](../../plans/reports/build/artifact-validation-manifest.json)
- [`plans/reports/build/SHA256SUMS.txt`](../../plans/reports/build/SHA256SUMS.txt)

These hashes reflect the E2 build, NOT the integrated HEAD. Fresh hashes will be generated by the first remote CI run.

---

## 9. RustSec Advisory DB

| Property | Value |
| :--- | :--- |
| **DB Source** | `github.com/rustsec/advisory-db` |
| **Revision** | `233155b4025b854c68a4b199c0e32d7fd53291a2` |
| **Freshness** | `2026-11-17 11:14:35 +0100` |
| **Commands** | `cargo deny --manifest-path <M> check -W wildcard advisories bans licenses sources` |
| **Manifests Scanned** | `Cargo.toml` (workspace), `apps/desktop/src-tauri/Cargo.toml`, `tests/cross-platform/Cargo.toml` |
| **Exit Codes** | `0`, `0`, `0` |
| **Advisories Found** | `0` across all 3 lockfiles |
| **Provenance** | `SELF_REPORTED_UNATTESTED` |

> **Scope Disclaimer:** `0 advisories` is a point-in-time snapshot of known published advisories in the RustSec advisory-db as of `2026-11-17`. This is NOT a guarantee of zero vulnerabilities, undiscovered issues, or supply-chain integrity beyond the advisory database's coverage.

---

## 10. Billing Estimate

| Metric | Value |
| :--- | :--- |
| **Plan** | GitHub Free |
| **Included Compute** | **`$12.00 USD/month`** |
| **Ubuntu x64 rate** | `$0.006/min` |
| **Windows x64 rate** | `$0.010/min` |
| **macOS rate** | `$0.062/min` |
| **Default Dispatch (Ubuntu + Windows) est.** | `$0.246` / `33 min wall-clock` (`ESTIMATED_PRE_RUN`) |
| **Full 4-OS Dispatch est.** | `$2.044` / `62 min wall-clock` (`ESTIMATED_PRE_RUN`) |
| **Actual consumption** | `$0.00 / NOT_MEASURED_YET` (0 remote runs executed) |

---

## 11. Remaining Blockers & Release Readiness

| Priority | Blocker | Status |
| :--- | :--- | :--- |
| **P0** | OAuth `workflow` scope | `AWAITING_USER_AUTHORIZATION` |
| **P0** | Default-branch workflow registration | `NOT_REGISTERED_ON_DEFAULT_BRANCH` |
| **P0** | Gate G3: Windows Authenticode signing | Not started |
| **P1** | Gate G4: macOS Developer ID + notarization | Not started |
| **P1** | Human 2-account QR functional verification | Not started |
| **P2** | Gate G5: Ubuntu package signing | Not started |
| **P2** | Pre-existing `crates/core/lease.rs` type error (uncommitted) | Needs resolution by owning agent |

---

## Verdict

### **`CI_NOT_REGISTERED`**

The cross-platform validation workflow (`cross-platform-validation.yml`) is authored, locally validated (6/6 + 16/16), and committed on `integration/reliability-hardening`. However, it cannot be executed remotely because:

1. The workflow file is not on `origin/main` (required for `workflow_dispatch` registration)
2. The `gh` OAuth token lacks `workflow` scope (required to push workflow files)
3. Zero remote CI runs have been executed

The workflow is ready for registration and execution once the two blockers (`AWAITING_USER_AUTHORIZATION` + `NOT_REGISTERED_ON_DEFAULT_BRANCH`) are resolved via the 8-step safe rollout plan documented in Section 4.
