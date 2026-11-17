# ADR-002: Rust Workspace, Tauri 2 Shell & Windows Smart App Control (SAC) Signed-Host Bridge

**Status:** Accepted  
**Date:** 2026-11-17  
**Related:** [environment-readiness.md](../research/environment-readiness.md) | [ADR-001](./adr-001-hybrid-isolation-architecture.md) | [os-readiness.md](../compatibility/os-readiness.md)

---

## 1. Context & Constraints

Two verified host realities (documented in [environment-readiness.md](../research/environment-readiness.md)) govern how `desktop-instance-manager` is built, tested, and executed:
1. **Windows 11 Smart App Control (WDAC / SAC) Active Enforcement:** `HKLM:\SYSTEM\CurrentControlSet\Control\CI\Policy` reports `VerifiedAndReputablePolicyState = 1`. Locally compiled unsigned PE binaries (`multi-zalo-desktop\dist\MultiZaloDesktop.exe`, `rea-windows-x64.node`) are blocked at load time with `An Application Control policy has blocked this file`. Meanwhile, Microsoft-signed `powershell.exe` (`CN=Microsoft Windows`) and OpenJS-signed `node.exe` (`v26.13.1`, `CN=OpenJS Foundation`) execute cleanly.
2. **No Local `rustc` / `cargo` / `.NET SDK`:** While `.github/workflows/ci.yml` compiles and tests the full Rust workspace across Windows, macOS, and Ubuntu runners, local validation on this Windows 11 host must run reliably without local Rust compilation.
3. **Security & Modern Web Standards:** The UI must have zero third-party JS runtime dependencies (eliminating `npm` supply-chain risk) and comply strictly with Modern Web Guidance (`dialog`, `responsive-table`, `dark-mode`, `tabs`).

---

## 2. Weighted Decision Matrix (UI & Host Execution Strategy)

Weights: SAC Compatibility Today (`25%`), Long-Term Cross-Platform Architecture (`25%`), Supply-Chain & Memory Safety (`20%`), Native OS API Access (`15%`), UI Maintainability & Accessibility (`15%`). *(Note: Rust binary startup latency and Tauri bundle size on Windows are unmeasured locally because `rustc` is not installed.)*

| Option | SAC Compliance Today (25%) | Cross-Platform Core (25%) | Supply-Chain / Safety (20%) | OS API Access (15%) | UI Quality (15%) | Weighted Score & Verdict |
| :--- | :---: | :---: | :---: | :---: | :---: | :---: |
| **A. Pure PowerShell/WPF Script Only** | 5/5 (1.25) | 0/5 (0.00) — Windows-only | 3/5 (0.60) | 4/5 (0.60) | 2/5 (0.30) | **2.75 / 5.00** (Rejected: no macOS/Linux core) |
| **B. Electron + Node Native Addons** | 2/5 (0.50) — unsigned `.node` blocked by SAC | 4/5 (1.00) | 2/5 (0.40) — large npm tree | 3/5 (0.45) | 4/5 (0.60) | **2.95 / 5.00** (Rejected: bloats RAM alongside Zalo's Electron) |
| **C. Unsigned Rust / Tauri 2 Binary Only** | 0/5 (0.00) — blocked by SAC without EV/OV cert | 5/5 (1.25) | 5/5 (1.00) | 5/5 (0.75) | 5/5 (0.75) | **3.75 / 5.00** (Blocked locally by SAC & missing `rustc`) |
| **D. Rust Workspace + Tauri 2 + Signed-Host Bridge (`dim-host.ps1` + `ui-server.mjs`)** (**Selected**) | **5/5 (1.25)** | **5/5 (1.25)** | **5/5 (1.00)** | **5/5 (0.75)** | **5/5 (0.75)** | **5.00 / 5.00 (Selected)** |
| **E. Disable SAC / Self-Signed Local Cert Injection** (**Rejected**) | 0/5 (0.00) — violates security guidelines | 3/5 (0.75) | 1/5 (0.20) | 4/5 (0.60) | 3/5 (0.45) | **2.00 / 5.00 (Rejected: never bypass OS policy)** |

---

## 3. Decision

We adopt a **dual-host execution architecture** sharing one canonical domain contract and one Modern Web UI:

1. **Canonical Rust Workspace + Tauri 2 (`Cargo.toml`, `crates/*`, `apps/desktop/src-tauri/`):**
   - Edition 2021, MSRV `1.78`, `license = "MIT OR Apache-2.0"`, strict workspace lints (`unsafe_op_in_unsafe_fn = "deny"`, `clippy::unwrap_used = "warn"`, `clippy::expect_used = "warn"`).
   - `deny.toml` enforces `rustls` (banning `openssl`/`openssl-sys`), permissive OSS licenses (`MIT`, `Apache-2.0`, `BSD-*`, `ISC`, `Zlib`, `Unicode-3.0`), crates.io-only sources, and advisory checks.
   - `apps/desktop/src-tauri/` wraps `dim-core`, `dim-profile-store`, `dim-process-manager`, and `dim-platform-*` into Tauri 2 IPC commands (`commands.rs`), compiled and tested on CI (`windows-latest`, `macos-latest`, `ubuntu-latest`).
2. **Windows SAC Signed-Host Bridge (`scripts/dim-host.ps1` + `apps/desktop/ui-server.mjs`):**
   - Runs natively inside Authenticode-signed `powershell.exe` (`CN=Microsoft Windows`) and `node.exe` (`CN=OpenJS Foundation`) without spawning any unsigned PE executable.
   - `scripts/dim-host.ps1` compiles Win32 P/Invoke definitions (`CreateProcessW`, `rstrtmgr.dll`, `CreateToolhelp32Snapshot`, `User32` `WM_CLOSE`, `GetDiskFreeSpaceExW`, `ReparsePoint` unlink) in memory inside PowerShell `Add-Type` and exposes `--cli --json` subcommands (`status`, `discover`, `create`, `rename`, `delete`, `launch`, `focus`, `stop`, `backup`, `restore`, `diagnostics`, `verify-isolation`).
   - `apps/desktop/ui-server.mjs` binds strictly to `127.0.0.1` (ephemeral or configured loopback port, zero `0.0.0.0` exposure), serves static `apps/desktop/ui/` assets with security headers, and translates `/api/*` requests into parameterized `execFile` invocations of `powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts/dim-host.ps1 --cli --json ...` (never `cmd.exe` / shell interpolation).
3. **Zero-Dependency Modern Web UI (`apps/desktop/ui/`):**
   - Pure HTML/CSS/ESM JS implementing `dialog` (`command="show-modal"`, `autofocus` on Cancel, `dialog:modal` `overscroll-behavior: contain`), `responsive-table` (container queries `@container (width < 600px)`, sticky headers), `dark-mode` (`<meta name="color-scheme" content="light dark">`, `light-dark(...)`, `@supports` scrollbar fallback), and `tabs` (`focusgroup="tablist nomemory"`, `hidden="until-found"`, `beforematch`).
   - Auto-detects `window.__TAURI__.core.invoke` vs `/api/*` HTTP transport so the exact same UI runs identically under Tauri 2 and the Signed-Host Bridge.

---

## 4. Security & Operational Consequences

- **Zero SAC Bypass:** Users never disable Smart App Control (`VerifiedAndReputablePolicyState = 1`) or add AV exclusions.
- **Gate F Release Requirement:** Standalone Windows installers/executables from `apps/desktop/src-tauri/` MUST be signed with a trusted Authenticode certificate (OV/EV or Azure Trusted Signing) in release CI before public Windows 11 SAC distribution. Until signed binaries exist, Windows 11 SAC users run via the Signed-Host Bridge.
- **Contract Parity Risk:** Maintaining two implementations of core profile/backup rules (Rust `crates/*` and PowerShell `scripts/dim-host.ps1`) creates drift risk. Mitigation: identical `profiles.json` v1 / `backup.json` v1 schemas, identical `resolve_safe_child` / `slugify` rules, and automated cross-verification in `tests/run-fixture-tests.mjs` + CI Rust unit tests.
