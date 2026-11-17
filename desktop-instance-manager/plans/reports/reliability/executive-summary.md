# Cross-Platform Reliability & Integration Evidence Index

**Branch:** `test/cross-platform-reliability`  
**Baseline Commit:** `a5feb4a52079` (`main`)  
**Execution Timestamp (UTC):** `2026-11-17T17:07:34Z`  
**Host Environment:** Windows 11 (`10.0.26200`), `x86_64`, `rustc 1.100.0 (90c541806)`  
**Deterministic Seed:** `0x5A414C4F5F584631`  
**Product Source Modifications:** `0` files modified (`crates/**`, `apps/**`, `scripts/**`, root `Cargo.toml`, and `.github/workflows/ci.yml` untouched)

---

## 1. JSON Evidence Artifacts in This Directory

| Artifact | Purpose | Key Summary Metrics |
| :--- | :--- | :--- |
| `windows-host-execution-evidence.json` | Complete machine-readable run output from `xf-test-runner` across all 6 suites (`55` scenarios) + `100`-cycle soak | `total = 55`, `pass = 42`, `fail_defect_reproduced = 11` (`10` unique defects), `skipped_host_platform = 2` (`XP-MAC-04`, `XP-LNX-04`), `fail_unexpected = 0` |
| `metrics-benchmark-windows.json` | Real OS process/filesystem telemetry benchmark (`10` iterations, `NodeFixture` + child worker tree) | Startup `p50 = 259.64 ms` (`p95 = 266.20 ms`), Shutdown `p50 = 78.66 ms`, Working Set `mean = 49.06 MiB`, Idle CPU `mean = 0.00%`, Residuals `= 0`, Failure Rate `= 0.0000` |
| `defect-reproductions.json` | Structured catalog of uncovered product defects with severity, affected product files/lines, minimal reproductions, and observed evidence | `9` dynamically reproduced defects (`DEFECT-01..04`, `DEFECT-06..10`) + `DEFECT-05` documented in contract/reliability reports |

---

## 2. Human-Readable Engineering Reports (`docs/testing/cross-platform/`)

1. **Test Matrix:** `docs/testing/cross-platform/test-matrix.md` — Full `55`-case matrix (`LC-01..09`, `PM-01..08`, `DS-01..08`, `REL-01..08`, `XP-WIN-01..04`, `XP-MAC-01..04`, `XP-LNX-01..04`, `CT-01..10`) with OS-honest statuses.
2. **Reliability & Telemetry Report:** `docs/testing/cross-platform/reliability-report.md` — Detailed empirical metrics (`10`-cycle full restarts, `100`-cycle native Rust soak at `p50 = 30.76 ms` startup, crash injection, corruption fail-closed checks) and root-cause analysis for `DEFECT-01` .. `DEFECT-10`.
3. **Adapter & CLI Contract Report:** `docs/testing/cross-platform/adapter-contract-report.md` — Public schema, `22` canonical error codes, process ownership, capability honesty, safety guarantees, and differential `dim-cli` vs `scripts/dim-host.ps1` / Tauri IPC divergences.
4. **Workspace & GitHub Actions Integration Guide:** `docs/testing/cross-platform/workspace-and-ci-integration.md` — Opt-in instructions for wiring `windows-latest`, `macos-14`, and `ubuntu-24.04` runners without touching shared `Cargo.toml` or CI files in this branch.
5. **Authorized Live Zalo Checklist:** `docs/testing/cross-platform/live-zalo-authorized-checklist.md` — Strictly opt-in manual checklist for authorized human testers (never run in CI).

---

## 3. Re-Running the Suite

```powershell
# Windows PowerShell
powershell -ExecutionPolicy Bypass -File tests/cross-platform/scripts/run-windows.ps1
```

```bash
# macOS
bash tests/cross-platform/scripts/run-macos.sh

# Ubuntu LTS
bash tests/cross-platform/scripts/run-ubuntu.sh

# Cross-platform Node wrapper
node tests/cross-platform/scripts/run-all.mjs
```
