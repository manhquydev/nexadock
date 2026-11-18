# Cross-Platform Reliability & Integration Evidence Index

**Branch:** `test/cross-platform-reliability`  
**Frozen Test Baseline Commit (`C1`):** `a2143550935d75c1569123e628eccb3358654834`  
**Product Baseline Commit (`main`):** `a5feb4a520798064c1508c26fe4c4ac17a3a03cf` (`a5feb4a52079`)  
**Integration SHA Verified (`C2`):** `NONE` (`status = AWAITING_INTEGRATION_REVISION`, `verdict = NOT_YET_VERIFIED`)  
**Execution Timestamp (UTC):** `2026-11-17T17:07:34Z`  
**Host Environment:** Windows 11 (`10.0.26200`), `x86_64`, `rustc 1.100.0 (90c541806)`  
**Deterministic Seed:** `0x20261117` (fixture PRNG base `0x5A414C4F5F584631`)  
**Product Source Modifications:** `0` files modified (`crates/**`, `apps/**`, `scripts/**`, root `Cargo.toml`, and `.github/workflows/ci.yml` untouched)

---

## 1. JSON Evidence & Baseline Freeze Artifacts in This Directory

| Artifact | Purpose | Key Summary Metrics & Frozen Integrity |
| :--- | :--- | :--- |
| `baseline-manifest.json` | Canonical Mission `C2` baseline freeze manifest, evidence SHA-256 integrity seal, `55`-case `P0`/`P1`/`P2`/`REGRESSION_GUARD` classification, assertion stability audit, and single-case reproducers | `55` canonical cases (`P0 = 4`, `P1 = 6`, `P2 = 1`, `REGRESSION_GUARD = 44`), `2` static-only findings (`DEFECT-05`, `STATIC-TAURI-DIAG-01`), `status = AWAITING_INTEGRATION_REVISION` |
| `windows-host-execution-evidence.json` | Immutable `C1` machine-readable run output from `xf-test-runner` across all 6 suites (`55` scenarios) + `100`-cycle soak | `total = 55`, `pass = 42`, `fail_defect_reproduced = 11` (`9` runtime/static cataloged IDs), `skipped_host_platform = 2` (`XP-MAC-04`, `XP-LNX-04`), `fail_unexpected = 0` (`52,991` B, SHA-256 `e895ad959568f7bccd10f12ff1656fb50e037e17c32fe5db3b4f9c41cf1ed7b9`) |
| `metrics-benchmark-windows.json` | Immutable `C1` real OS process/filesystem telemetry benchmark (`10` iterations, `NodeFixture` + Native Rust fixture) | Native startup `p50 = 27.72 ms` (`NodeFixture` `p50 = 329.35 ms`), Working Set `4.72 MiB` native / `53.60 MiB` Node, Idle CPU `0.081%` host-normalized, Residuals `= 0`, Failure Rate `= 0.0000` (`3,140` B, SHA-256 `bed0c193b7b6c82f2c45f7821c9bd9e033ebf3b6354e09a38bd7998dc2613ab4`) |
| `defect-reproductions.json` | Immutable `C1` structured catalog of uncovered product defects with severity, affected product files/lines, minimal reproductions, and observed evidence | `9` cataloged defects (`DEFECT-01..04`, `DEFECT-06..10`) + `DEFECT-05` & Tauri IPC `diagnostics_report` row duplication tracked in `baseline-manifest.json` (`11,030` B, SHA-256 `787b55c7a376f8932c11a828b6d4144c86f1c6e18f82a993f9e626964e09577b`) |

---

## 2. Human-Readable Engineering & Regression Handoff Reports (`docs/testing/cross-platform/`)

1. **Test Matrix (`C1`):** `docs/testing/cross-platform/test-matrix.md` — Full `55`-case matrix (`LC-01..09`, `PM-01..08`, `DS-01..08`, `REL-01..08`, `XP-WIN-01..04`, `XP-MAC-01..04`, `XP-LNX-01..04`, `CT-01..10`) with OS-honest statuses.
2. **Reliability & Telemetry Report (`C1`):** `docs/testing/cross-platform/reliability-report.md` — Detailed empirical metrics (`10`-cycle full restarts, `100`-cycle native Rust soak at `p50 = 30.76 ms` startup, crash injection, corruption fail-closed checks) and root-cause analysis for `DEFECT-01` .. `DEFECT-10`.
3. **Adapter & CLI Contract Report (`C1`):** `docs/testing/cross-platform/adapter-contract-report.md` — Public schema, `22` canonical error codes, process ownership, capability honesty, safety guarantees, and differential `dim-cli` vs `scripts/dim-host.ps1` / Tauri IPC divergences.
4. **Workspace & GitHub Actions Integration Guide (`C1`):** `docs/testing/cross-platform/workspace-and-ci-integration.md` — Opt-in instructions for wiring `windows-latest`, `macos-14`, and `ubuntu-24.04` runners without touching shared `Cargo.toml` or CI files in this branch.
5. **Authorized Live Zalo Checklist (`C1`):** `docs/testing/cross-platform/live-zalo-authorized-checklist.md` — Strictly opt-in manual checklist for authorized human testers (never run in CI).
6. **Regression Handoff & Defect Reproducer Guide (`C2`):** `docs/testing/cross-platform/regression-handoff.md` — Prioritized `P0` -> `P1` -> `P2` defect handoff cards (7 mandatory fields per item), single-command reproducers, Section 3 harness vs. product defect classification & assertion stability matrix, and Section 4 controlled regression mode rules.
7. **Post-Fix Verification Ledger & Protocol (`C2`):** `docs/testing/cross-platform/post-fix-verification.md` — Defect lifecycle state ledger (`OPEN -> REPRODUCED -> FIXED_CANDIDATE -> VERIFIED`), current boundary status (`AWAITING_INTEGRATION_REVISION` / `NOT_YET_VERIFIED`), independent post-fix verification checklist, and native OS validation matrix.

---

## 3. Re-Running the Suite & Controlled Regression Modes (`C2`)

```powershell
# Non-destructive baseline audit (exits 0 when unexpected_fail == 0; never overwrites frozen C1 JSON)
cargo run --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner -- --mode baseline-audit --priority p0

# Mandatory post-fix release regression gate (exits 2 on ANY reproduced or unexpected defect; exits 3 under --require-native-os if skipped)
cargo run --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner -- --mode release-regression --priority p0
cargo run --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner -- --mode release-regression --priority p0-p1

# Single-case fast reproduction (any canonical test ID LC-01..CT-10)
cargo run --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner -- --mode release-regression --case LC-08

# Windows PowerShell wrapper
powershell -ExecutionPolicy Bypass -File tests/cross-platform/scripts/run-windows.ps1
```

```bash
# macOS (native Launch Services & CFFIXED_USER_HOME runtime gate)
bash tests/cross-platform/scripts/run-macos.sh

# Ubuntu LTS (native /proc, POSIX signals & bwrap runtime gate)
bash tests/cross-platform/scripts/run-ubuntu.sh

# Cross-platform Node wrapper
node tests/cross-platform/scripts/run-all.mjs
```
