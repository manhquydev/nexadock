# Cross-Platform Reliability & Integration Test Harness (`dim-cross-platform-tests`)

Independent Rust + Node.js integration, fault-injection, OS-boundary, soak, and differential contract test system for `Desktop Instance Manager` across **Windows 10/11**, **macOS**, and **Ubuntu LTS**.

## Design Principles

1. **Zero Product Mutation:** Operates as an independent Cargo `[workspace]` under `tests/cross-platform/` paired with `fixtures/cross-platform/` without modifying shared `Cargo.toml`, `.github/workflows/ci.yml`, or platform adapter source code.
2. **Zero Real Zalo Dependency:** Uses `xf-desktop-fixture` (native Rust) and `xf-fixture.mjs` (signed Node companion) inside RAII `IsolatedSandbox` temp directories; never touches `%APPDATA%\ZaloData`, `~/Library/Application Support/ZaloData`, or `~/.config/ZaloData`.
3. **Self-Upgrading Defect Reproductions:** Tests assert the true specification. When an unpatched product bug is triggered, the runner records `FAIL (DEFECT_REPRODUCED)` with exact `file:line` references and minimal reproduction steps. Passing `--fail-on-product-defects` (or `cargo test -- --ignored`) turns every reproduced defect into a hard merge-blocking CI failure once adapters are patched.
4. **Cross-Platform Honesty:** Windows runs never mark macOS or Ubuntu runtime gates as `PASS`; OS-kernel-specific gates report `SKIPPED (REQUIRES_TARGET_OS)` outside their target OS.

## Per-OS Execution Commands

### Unified Auto-Detecting Runner
```bash
node desktop-instance-manager/tests/cross-platform/scripts/run-all.mjs
```

### Windows 10 / 11 (PowerShell)
```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File desktop-instance-manager/tests/cross-platform/scripts/run-windows.ps1 -Cycles 100
```

### macOS (`arm64` & `x86_64`)
```bash
bash desktop-instance-manager/tests/cross-platform/scripts/run-macos.sh
```

### Ubuntu 22.04 / 24.04 LTS
```bash
bash desktop-instance-manager/tests/cross-platform/scripts/run-ubuntu.sh
```

### Cargo Test Integration Target & Strict Post-Fix Gate
```bash
# Standard suite execution (fast 25-cycle soak)
cargo test --manifest-path desktop-instance-manager/tests/cross-platform/Cargo.toml

# Strict zero-defect gate (run after patching DEFECT-01..DEFECT-10 in product crates)
cargo test --manifest-path desktop-instance-manager/tests/cross-platform/Cargo.toml -- --ignored
```
