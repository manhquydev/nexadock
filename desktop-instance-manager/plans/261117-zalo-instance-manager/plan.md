---
title: "MultiZaloDesktop (desktop-instance-manager) Architecture & Delivery Plan"
slug: 261117-zalo-instance-manager
date: 2026-11-17
status: active
target_workspace: c:\Users\manhquy\Downloads\zalo\desktop-instance-manager
gates: [A, B, C, D, E, F]
---

# Implementation & Verification Plan: `desktop-instance-manager` (`MultiZaloDesktop`)

## 1. Outcome Contract & Scope Boundaries

### 1.1. Goal
Build a production-grade, non-elevated (`asInvoker`), cross-platform multi-instance manager for **official Zalo Desktop** that enables users to create, rename, launch, focus, gracefully close, recover, back up, and restore isolated Zalo profiles concurrently **without modifying VNG binaries or touching the user's primary `%APPDATA%\ZaloData` session**.

### 1.2. Hard Constraints & Invariants
- **Zero Binary Modification:** Never modify `Zalo.exe`, `Zalo.app`, `app.asar`, or Authenticode/Apple notarization signatures.
- **Zero Redistribution of Proprietary IP:** Never bundle, extract, or distribute VNG's `app.asar` (blocking Linux `zalo-for-linux`-style repackaging).
- **Primary Profile Protection:** Primary `%APPDATA%\ZaloData` and its active PIDs are strictly read-only/protected (`PROTECTED_PRIMARY_PROCESS`); no slot-swapping (`MoveFileEx` on `%APPDATA%\ZaloData`) and no global `Get-Process Zalo | Stop-Process`.
- **Smart App Control (SAC / WDAC) Compliance:** Never disable `VerifiedAndReputablePolicyState = 1`; execute locally on Windows 11 SAC hosts via the **Signed-Host Bridge** (`powershell.exe` + `node.exe`) while preserving Rust/Tauri 2 compilation + Authenticode signing for CI/Release (Gate F).
- **Honest Platform & Feature Reporting:** Never overclaim `NOT_TESTED` (macOS), `UNSUPPORTED` (official Linux Zalo), `EXPERIMENTAL` (Linux fixture/local AppImage), or `BLOCKED` (concurrent `ZaloCall.exe` named pipes) capabilities.

### 1.3. Non-Goals
- Bypassing Zalo QR/password authentication, automating chat spam, or spoofing device identifiers.
- Shipping a kernel-mode driver (`SbieDrv.sys`) or GPL-3.0 syscall hook layer to virtualize `\\.\pipe\PipeZCallRecv`.
- Claiming Wine, Zalo Web wrappers, or repackaged macOS `app.asar` as native official Linux support.

---

## 2. Weighted Architectural Decision Matrix (Options A – E)

Weights: Binary Integrity (`20%`), Primary Data Safety (`20%`), Least Privilege / Zero Drivers (`15%`), Windows SAC & Enterprise Compliance (`15%`), Zalo Update Resilience (`10%`), Cross-Platform Maintainability (`10%`), and Resource Overhead (`10%`).

> **Unmeasured Benchmark Notice:** Only historical Windows `26.10.20` PoC/IT baseline measurements (`~1.9–2.4s` startup, `~1.07 GB` WorkingSet/instance across 3 isolated + 1 primary session) are measured in [`multi-zalo-desktop/evidence/`](../../../multi-zalo-desktop/evidence/integration-test-result.json). **macOS runtime latency, Linux AppImage/`bwrap` overhead, Rust binary compile/startup benchmarks (no local `rustc`), and live multi-account voice/notification telemetry are UNMEASURED.**

| Criterion (Weight) | Option A: Profile-Aware Launcher (Scripts Only) | Option B: OS-Level Isolation (`Sandboxie` / `runas` / VM) | Option C: Installation Cloning | Option D: Hybrid Rust Core + Tauri 2 + Platform Adapters + SAC Signed-Host Bridge (**Selected**) | Option E: `app.asar` Patching (**Rejected**) |
| :--- | :---: | :---: | :---: | :---: | :---: |
| **Binary Integrity (20%)** | 5/5 (1.00) | 5/5 (1.00) | 4/5 (0.80) | **5/5 (1.00)** | 0/5 (0.00) |
| **Primary Data Safety (20%)** | 2/5 (0.40) | 4/5 (0.80) | 1/5 (0.20) | **5/5 (1.00)** | 2/5 (0.40) |
| **Least Privilege / No Drivers (15%)** | 5/5 (0.75) | 1/5 (0.15) | 5/5 (0.75) | **5/5 (0.75)** | 3/5 (0.45) |
| **SAC & Enterprise Compliance (15%)** | 3/5 (0.45) | 2/5 (0.30) | 3/5 (0.45) | **5/5 (0.75)** | 1/5 (0.15) |
| **Update Resilience (10%)** | 4/5 (0.40) | 3/5 (0.30) | 1/5 (0.10) | **5/5 (0.50)** | 0/5 (0.00) |
| **Cross-Platform Structure (10%)** | 2/5 (0.20) | 1/5 (0.10) | 2/5 (0.20) | **4.5/5 (0.45)** | 1/5 (0.10) |
| **Resource Overhead (10%)** | 5/5 (0.50) | 2/5 (0.20) | 1/5 (0.10) | **4.5/5 (0.45)** *(macOS/Linux unmeasured)* | 5/5 (0.50) |
| **Weighted Total** | **3.70 / 5.00** | **2.85 / 5.00** | **2.60 / 5.00** | **4.90 / 5.00 (Selected)** | **1.60 / 5.00 (Rejected)** |

---

## 3. Phased Execution Roadmap & Ownership

### Phase 0: Environment, Repository & Feasibility Research (`docs-research`)
- **Deliverables:**
  - [environment-readiness.md](../../docs/research/environment-readiness.md)
  - [repository-comparison.md](../../docs/research/repository-comparison.md) (`ak-xia --compare` on `kemonra/ZaloMulti`, `ducngfi/zalo-multi-accounts`, `VN-Linux-Family/zalo-for-linux`, `sandboxie-plus/Sandboxie`, `lza.lol`)
  - [platform-feasibility.md](../../docs/research/platform-feasibility.md)
  - [os-readiness.md](../../docs/compatibility/os-readiness.md)
- **Acceptance Criteria:** All host facts, REA MCP evidence IDs, commit SHAs, licenses, and `CONFIRMED / AUTHOR CLAIM / INFERENCE / UNVERIFIED` labels verified directly against sources.
- **Status:** Completed.

### Phase 1: Architecture Decision Records (`docs-research`)
- **Deliverables:**
  - [adr-001-hybrid-isolation-architecture.md](../../docs/architecture/adr-001-hybrid-isolation-architecture.md)
  - [adr-002-rust-tauri-and-sac-host-bridge.md](../../docs/architecture/adr-002-rust-tauri-and-sac-host-bridge.md)
  - [adr-003-process-ownership-and-graceful-shutdown.md](../../docs/architecture/adr-003-process-ownership-and-graceful-shutdown.md)
  - [adr-004-backup-restore-and-data-safety.md](../../docs/architecture/adr-004-backup-restore-and-data-safety.md)
  - [plan.md](./plan.md)
- **Acceptance Criteria:** Every ADR includes a weighted decision matrix, explicitly states unmeasured benchmarks, defines exact schemas (`profiles.json` v1, `backup.json` v1) and safety invariants, and cross-links cleanly.
- **Status:** Completed.

### Phase 2: Rust Core Workspace, Synthetic Fixture & CI (`rust-workspace`)
- **Owned Paths:** `Cargo.toml`, `deny.toml`, `crates/core/`, `crates/profile-store/`, `crates/process-manager/`, `crates/platform-windows/`, `crates/platform-macos/`, `crates/platform-linux/`, `fixtures/test-desktop-app/`, `.github/workflows/ci.yml`.
- **Key Contracts:**
  - Workspace lints (`unsafe_op_in_unsafe_fn = "deny"`) + `deny.toml` (`rustls` required, `openssl` banned, OSS license allowlist).
  - `dim-core` traits (`ApplicationLocator`, `ProfileStore`, `InstanceLauncher`, `InstanceMonitor`, `PlatformCapabilities`, `BackupManager`, `RecoveryManager`), Vietnamese `đ` -> `d` slugify, `resolve_safe_child`, `5 GiB` disk threshold, `Database/_production/<6+ digits>` account ID scan.
  - `dim-profile-store` atomic `profiles.json.tmp` write + `.partial` -> `backup.json` (SHA-256) + restore-to-new-profile-only.
  - `dim-process-manager` `PrimaryProtectionGuard` + `GracefulQuitSequence` + stale lockfile recovery.
  - Cross-platform `fixtures/test-desktop-app/fixture-app.mjs` (Node.js `v26.13.1`) enforcing single-instance lockfile (`exit 2`), `--z-update-force-quit` -> `.quit-signal`, `--crash` stale lockfile mode, `launch-state.json`, and `Database/_production/<uid>`.
- **Mechanical Verification:** `node fixtures/test-desktop-app/fixture-app.mjs` smoke verification; CI runs `cargo test`, `cargo clippy`, `cargo deny check`.

### Phase 3: Windows SAC Signed-Host Bridge, Desktop UI & Test Harness (`desktop-ui-poc`)
- **Owned Paths:** `apps/desktop/src-tauri/`, `apps/desktop/ui/`, `apps/desktop/ui-server.mjs`, `scripts/dim-host.ps1`, `scripts/run-fixture-poc.ps1`, `scripts/run-zalo-poc.ps1`, `scripts/run-tests.ps1`, `tests/run-fixture-tests.mjs`.
- **Key Contracts:**
  - `scripts/dim-host.ps1`: Win32 P/Invoke (`CreateProcessW`, `rstrtmgr.dll`, `CreateToolhelp32Snapshot`, `WM_CLOSE`, `User Shell Folders\AppData` redirection check, `Get-AuthenticodeSignature`, junction unlinking, `PROTECTED_PRIMARY_PROCESS`, pre-seed `zmeta.json`/`migrate-config.json`/`.zalo-profile-id`).
  - `apps/desktop/ui/` (`index.html`, `styles.css`, `app.js`): zero third-party JS dependencies, strict `modern-web-guidance` compliance (`dialog`, `responsive-table`, `dark-mode`, `tabs`), dual transport (`__TAURI__.core.invoke` or `127.0.0.1` `ui-server.mjs` `/api/*`).
  - `tests/run-fixture-tests.mjs`: automated E2E suite including Vietnamese slugification, 3 concurrent fixtures, lock collision (`exit 2`), **10 consecutive close/reopen cycles**, crash recovery, duplicate `zalo_user_id` guard, offline atomic backup & new-profile restore, path traversal rejection, primary PID protection, log redaction, and UI server smoke.
  - `scripts/run-zalo-poc.ps1`: non-destructive live PoC against official Zalo `26.10.20` in `%LOCALAPPDATA%\MultiZaloDesktop_LivePoC` verifying primary `%APPDATA%\ZaloData` SHA-256 invariance.
- **Mechanical Verification:** `node tests/run-fixture-tests.mjs` exits `0` with all suites passing.

### Phase 4: Security Audit, Independent Verification & Gate Assessment (`security-verifier`)
- **Owned Paths:** `docs/security/threat-model-and-audit.md`, `docs/testing/poc-and-test-report.md`, `evidence/*`, `README.md` (workspace & repo root).
- **Verification Tasks:**
  1. Audit all 11 STRIDE + OWASP threat categories (`data exfiltration`, `filesystem ACLs`, `cross-profile leakage`, `privilege escalation`, `unsafe command execution`, `supply chain`, `log leakage`, `insecure update`, `destructive cleanup`, `incorrect restore`, `malicious paths`).
  2. Independently inspect `apps/desktop/ui/` for Modern Web Guidance compliance and `crates/` for `unsafe` FFI justification.
  3. Independently execute `node tests/run-fixture-tests.mjs` -> `evidence/fixture-test-result.json` and `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run-zalo-poc.ps1` -> `evidence/zalo-live-poc-result.json` (confirming primary `%APPDATA%\ZaloData` SHA-256 identical before & after).
  4. Render explicit **Gate A–F** verdicts.

### Phase 5: CI Compilation, Release Signing & Multi-OS Hardware Validation (Post-Delivery)
- **Targets:**
  - Run `.github/workflows/ci.yml` on GitHub Actions (`windows-latest`, `macos-latest`, `ubuntu-latest`) to execute `cargo test --workspace`, `cargo clippy`, and `cargo deny check`.
  - Provision Authenticode OV/EV signing certificate in CI before shipping standalone Windows Tauri/Rust binaries to SAC-enabled Windows 11 hosts (**Gate F**).
  - Execute physical Apple Silicon + Intel macOS validation for `dim-platform-macos` (`CFFIXED_USER_HOME`, Keychain, `/tmp/socketzalorecv2021` `fs.unlink` impact).
  - Validate Ubuntu 22.04/24.04 AppArmor `userns` / `bwrap` behavior for `EXPERIMENTAL` local fixture/AppImage execution.

---

## 4. Gate A – F Exit Criteria

| Gate | Scope | Exit Requirement |
| :--- | :--- | :--- |
| **Gate A: Feasibility** | Static & PoC mechanism validation | VNG binary signatures intact; `app.asar` / PE evidence verified; core isolation proven without modifying Zalo |
| **Gate B: Data Safety** | Backup, restore, primary protection, paths | Offline `.partial` -> SHA-256 `backup.json` passes; restore-to-new-profile passes; primary `%APPDATA%\ZaloData` SHA-256 unchanged; `resolve_safe_child` + junction unlink enforced |
| **Gate C: Functional Compatibility** | Multi-instance lifecycle & honest blocks | Launch, focus, live rename, 10x restart, crash recovery, duplicate account guard pass; concurrent calls (`PipeZCallRecv`) & shared AUMID notifications honestly marked `BLOCKED` |
| **Gate D: Platform Readiness** | OS support honesty | Windows `PARTIALLY_SUPPORTED` (`SUPPORTED_AND_TESTED` core isolation); macOS `NOT_TESTED`; Ubuntu official Zalo `UNSUPPORTED` / fixture `EXPERIMENTAL` |
| **Gate E: Security & Compliance** | STRIDE/OWASP + Modern Web + Rust safety | All 11 threat categories audited; `asInvoker`; zero 3P UI JS runtime deps; `127.0.0.1`-only bridge; sanitized logs |
| **Gate F: Release Readiness** | Packaging & Windows SAC signing | Signed-Host Bridge ready for local SAC evaluation; standalone Rust/Tauri PE release **blocked on CI build + Authenticode signing cert** |

---

## 5. Risk Register & Rollback Strategy

1. **Risk: Zalo future update bypasses `%APPDATA%` env or adds Global Mutex** -> *Mitigation:* Dynamic discovery + preflight verification + isolated `PlatformCapabilities` adapter layer; rollback by pausing launch via preflight check rather than risking primary account collision.
2. **Risk: Rust/PowerShell schema drift** -> *Mitigation:* Single canonical `profiles.json` v1 / `backup.json` v1 specification + CI Rust tests + Windows E2E fixture tests.
3. **Risk: macOS `/tmp/socketzalorecv2021` `fs.unlink` disrupts active calls** -> *Mitigation:* Keep macOS explicitly `NOT_TESTED` until physical Mac verification measures call behavior.
4. **Risk: Unsigned binary blocked by Windows SAC** -> *Mitigation:* Signed-Host Bridge (`powershell.exe` + `node.exe`) for immediate compliant execution; Gate F blocks unsigned standalone EXE release.
