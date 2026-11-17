# Cross-Platform Reliability & Integration Test Harness (`dim-cross-platform-tests`)

Independent Rust + Node.js integration, fault-injection, OS-boundary, soak, and differential contract test system for `Desktop Instance Manager` across **Windows 10/11**, **macOS**, and **Ubuntu LTS**.

## Design Principles

1. **Zero Product Mutation:** Operates as an independent Cargo `[workspace]` under `tests/cross-platform/` paired with `fixtures/cross-platform/` without modifying shared `Cargo.toml`, `.github/workflows/ci.yml`, or platform adapter source code.
2. **Zero Real Zalo Dependency:** Uses `xf-desktop-fixture` (native Rust) and `xf-fixture.mjs` (signed Node companion) inside RAII `IsolatedSandbox` temp directories; never touches `%APPDATA%\ZaloData`, `~/Library/Application Support/ZaloData`, or `~/.config/ZaloData`.
3. **Controlled Regression Modes (`baseline-audit` vs `release-regression`):**
   - `--mode baseline-audit` (default): Distinguishes known baseline defects (`FAIL_DEFECT_REPRODUCED`) from unexpected harness failures (`FAIL_UNEXPECTED`, exit `1`) and flags `[FIXED_CANDIDATE]` when any baseline defect transitions to `PASS`. Writes **zero** JSON files unless `--report-out <path>` is passed (frozen C1 JSON files under `plans/reports/reliability/` are write-protected unless `--overwrite-baseline-reports` is explicitly provided).
   - `--mode release-regression`: Mandatory post-fix merge gate. **Never** treats `FAIL_DEFECT_REPRODUCED` (`EXPECTED_FAILURE`) as `PASS`; prints `RELEASE_GATE_FAILED` and exits `2` on any reproduced defect or unexpected failure, and exits `3` when `--require-native-os` hits `SKIPPED_HOST_PLATFORM` or `STATIC_CONFIRMED_NON_HOST`.
4. **Cross-Platform Honesty:** Windows runs never mark macOS or Ubuntu runtime gates as `PASS`; OS-kernel-specific gates report `SKIPPED (REQUIRES_TARGET_OS)` (`XP-MAC-04`, `XP-LNX-04`) or `STATIC_CONFIRMED_NON_HOST` (`XP-MAC-03`, `XP-LNX-02`) outside their native OS.

## Single-Case & Priority Reproduction Commands

Run from `desktop-instance-manager/`:

```bash
# Reproduce a single test case in seconds (case-insensitive IDs LC-01..CT-10)
cargo run --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner -- \
  --mode release-regression --case LC-08

# Mandatory P0 gate (LC-07, LC-08, PM-08, CT-10)
cargo run --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner -- \
  --mode release-regression --priority p0

# Mandatory P0+P1 gate (10 prioritized safety & parity cases)
cargo run --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner -- \
  --mode release-regression --priority p0-p1

# Non-destructive baseline audit (exits 0 when unexpected_fail == 0)
cargo run --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner -- \
  --mode baseline-audit --priority p0
```

## Per-OS Execution Scripts

```bash
# Unified auto-detecting runner (forwards CLI flags cleanly; does not overwrite frozen baseline JSON)
node desktop-instance-manager/tests/cross-platform/scripts/run-all.mjs --mode release-regression --priority p0-p1

# Windows 10 / 11 (PowerShell)
powershell.exe -NoProfile -ExecutionPolicy Bypass -File desktop-instance-manager/tests/cross-platform/scripts/run-windows.ps1 -Mode baseline-audit -Cycles 100

# macOS (arm64 & x86_64)
bash desktop-instance-manager/tests/cross-platform/scripts/run-macos.sh --mode release-regression --require-native-os

# Ubuntu 22.04 / 24.04 LTS
bash desktop-instance-manager/tests/cross-platform/scripts/run-ubuntu.sh --mode release-regression --require-native-os
```

## Cargo Test Integration Target & Mandatory Post-Fix Gates

```bash
# Standard suite execution (fast 25-cycle soak + filter/mode contract checks)
cargo test --manifest-path desktop-instance-manager/tests/cross-platform/Cargo.toml --release -- --test-threads=1

# Mandatory post-fix gates (release_gate_p0_mandatory, release_gate_p0_p1_mandatory, strict_zero_product_defects_gate)
cargo test --manifest-path desktop-instance-manager/tests/cross-platform/Cargo.toml --release -- --ignored --test-threads=1
```
