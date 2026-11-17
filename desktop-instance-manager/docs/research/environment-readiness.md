# Environment Readiness & Host Capability Audit

**Audit Date:** 2026-11-17  
**Target Host:** Windows 11 Pro (`10.0.26300`, 64-bit x64), non-elevated user session (`IsElevated = False`)  
**Scope:** Local toolchain, Windows Smart App Control (WDAC), AgentKit (`ak`), REA MCP (`rea-agents`), official Zalo Desktop installation, and Windows Shell Folder state.

---

## 1. Executive Summary

| Capability / Component | Observed Status | Version / Evidence | Architectural Impact |
| :--- | :--- | :--- | :--- |
| **Operating System & Privilege** | `READY` (Non-elevated) | Windows 11 Pro `10.0.26300` x64, `IsElevated = False` | All user-mode isolation runs under `asInvoker`; no Admin/drivers |
| **Windows Smart App Control (SAC / WDAC)** | `ENFORCING` (`VerifiedAndReputablePolicyState = 1`) | Registry `HKLM:\SYSTEM\CurrentControlSet\Control\CI\Policy` (`SAC_PreviousState = 2`, `SAC_EnforcementReason = 1`) | Blocks unsigned compiled PEs (`MultiZaloDesktop.exe`, `rea-windows-x64.node`); requires Signed-Host Bridge or Authenticode CI signing ([ADR-002](../architecture/adr-002-rust-tauri-and-sac-host-bridge.md)) |
| **Rust Toolchain (`rustc`/`cargo`/`rustup`)** | `NOT_INSTALLED` | `Get-Command rustc, cargo, rustup` returned empty | Rust workspace crates target MSRV `1.78` (Edition 2021) for CI builds (`.github/workflows/ci.yml`), not local compilation |
| **Node.js Runtime** | `READY` (Signed) | `v26.13.1` at `C:\Program Files\nodejs\node.exe` (Authenticode `Valid`, `CN=OpenJS Foundation`) | Runs synthetic desktop fixture (`fixture-app.mjs`), local UI server (`ui-server.mjs`), and automated test runner (`run-fixture-tests.mjs`) |
| **Windows PowerShell** | `READY` (Signed) | `C:\WINDOWS\System32\WindowsPowerShell\v1.0\powershell.exe` (Authenticode `Valid`, `CN=Microsoft Windows`) | Signed host for Win32 P/Invoke bridge (`scripts/dim-host.ps1` via `Add-Type`) and live Zalo PoC (`scripts/run-zalo-poc.ps1`) |
| **Git CLI** | `READY` | `git version 2.56.1.windows.1` | Version control & CI integration |
| **.NET SDK CLI** | `NOT_INSTALLED` | `dotnet --version` reports `No .NET SDKs were found` | Avoid requiring `dotnet` CLI locally; use built-in PowerShell `.NET Framework` host for Win32 P/Invoke |
| **AgentKit (`ak`)** | `READY` (Antigravity native adapter) | `ak 3.1.0`, `kit engineer agy/global/native` (`kit_version: 0.2.0`), 105 `ak-*` skills | `ak skills list` / `ak agents list` / `ak mcp list` scan `~/.claude` and report empty on this Antigravity native adapter |
| **REA MCP (`rea-agents`)** | `PARTIALLY_AVAILABLE` | `rea-agents v6.9.0`, MCP protocol `2026-07-28`, `255` MCP tools, `196` CLI commands | Built-in PE/ASAR/Fuse tools work; `rea-windows-x64.node` blocked by SAC; Ghidra/IDA/Kuna `not_configured` |
| **Official Zalo Desktop** | `INSTALLED` (Signed VNG) | Stub `1.0.0.0`, Electron `Zalo-26.10.20` (`26.10.20.3253`), Installer `ZaloSetup-26.11.12.exe` | Valid `CN=VNG GROUP JSC` Authenticode; `%APPDATA%\ZaloData` active primary profile must remain untouched |

---

## 2. Host Toolchain & Runtime Inventory

Direct shell inspection on `2026-11-17` confirmed the exact execution environment:

1. **Operating System & User Context**:
   - `Win32_OperatingSystem`: `Microsoft Windows 11 Pro`, `Version = 10.0.26300`, `OSArchitecture = 64-bit`.
   - Elevation check (`WindowsPrincipal.IsInRole(Administrator)`): `False` — confirming all manager operations, PoCs, and tests execute strictly under standard user privileges (`asInvoker`).
2. **Signed Execution Hosts (Available Locally)**:
   - **Windows PowerShell (`powershell.exe`)**: `C:\WINDOWS\System32\WindowsPowerShell\v1.0\powershell.exe` — Authenticode `Status = Valid`, `Subject = CN=Microsoft Windows, O=Microsoft Corporation, L=Redmond, S=Washington, C=US`. Note: PowerShell 7 (`pwsh.exe`) resolves via `WindowsApps` AppExecutionAlias (`C:\Users\manhquy\AppData\Local\Microsoft\WindowsApps\pwsh.exe`); default Windows PowerShell `powershell.exe -NoProfile -ExecutionPolicy Bypass` is preferred for deterministic script invocation.
   - **Node.js (`node.exe`)**: `v26.13.1` at `C:\Program Files\nodejs\node.exe` — Authenticode `Status = Valid`, `Subject = CN=OpenJS Foundation, O=OpenJS Foundation, L=San Francisco, S=California, C=US`. Note: invoking `npm` via PowerShell (`npm.ps1`) is blocked by default PowerShell script execution policy unless invoked via `cmd /c npm` or `node` directly, so project scripts and test runners execute via `node` directly with zero third-party `npm` runtime dependencies.
3. **Missing Compilers / SDKs**:
   - `rustc`, `cargo`, and `rustup` are **not installed** on this host.
   - `.NET SDK` (`dotnet`) is **not installed** (`No .NET SDKs were found`).
   - **Impact**: Local builds cannot compile the Rust crates in `crates/` or `apps/desktop/src-tauri/` into native Windows binaries, and even if compiled locally without an Authenticode signature, Windows Smart App Control would block execution (Section 3). Therefore, CI builds and unit test execution (`cargo test`, `cargo clippy`, `cargo deny`) live in [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml), while local runtime verification uses the signed-host bridge and Node.js fixture.

---

## 3. Windows Smart App Control (WDAC / SAC) Enforcement

Inspection of `HKLM:\SYSTEM\CurrentControlSet\Control\CI\Policy` on the target Windows 11 host confirmed active Smart App Control enforcement:

```json
{
  "VerifiedAndReputablePolicyState": 1,
  "SAC_PreviousState": 2,
  "SAC_EnforcementReason": 1
}
```

### Direct Enforcement Observations
1. **Unsigned Locally Compiled PE Blocked**:
   - Running the historical unsigned .NET executable [`multi-zalo-desktop/dist/MultiZaloDesktop.exe`](../../../multi-zalo-desktop/dist/MultiZaloDesktop.exe) (`Authenticode Status = NotSigned`) fails immediately at OS loader level:
     > `Program 'MultiZaloDesktop.exe' failed to run: An Application Control policy has blocked this file`
2. **Unsigned Native Addon Blocked (`rea-windows-x64.node`)**:
   - Calling REA's `analyze_javascript_application` failed with `capability_unavailable` (Evidence ID `ev_65a72c27603cd5f817368a4390e8a3b9d8d9ed1bf6592ecf0f57c4adfdc7ee4b`, recorded in [`multi-zalo-desktop/docs/research/rea-capabilities.md`](../../../multi-zalo-desktop/docs/research/rea-capabilities.md)):
     > `Windows refused to load the verified native/windows/build/rea-windows-x64.node: An Application Control policy has blocked this file.`
3. **Signed Hosts Execute Normally**:
   - `powershell.exe` (`CN=Microsoft Windows`), `node.exe` (`CN=OpenJS Foundation`), and all official Zalo executables (`CN=VNG GROUP JSC`) pass Smart App Control checks without modifying system policy.

**Design Constraint:** This project **never** disables, bypasses, or weakens Windows Smart App Control (`VerifiedAndReputablePolicyState = 1`). Instead, [ADR-002](../architecture/adr-002-rust-tauri-and-sac-host-bridge.md) defines a compliant **Signed-Host Bridge** for Windows SAC environments alongside Authenticode release signing for standalone Tauri/Rust binaries in Gate F.

---

## 4. AgentKit CLI (`ak`) & Native Adapter Topology

Direct inspection of `ak` CLI and `C:\Users\manhquy\.agentkit\`:

- **CLI Version**: `ak 3.1.0`
- **Installed Kit**: Engineer Kit (`kit engineer agy/global/native`), manifest `kit_version: 0.2.0` located at `C:\Users\manhquy\.agentkit\adapters\agy\engineer\.agentkit\install-manifest.json` (with `AGY-MODEL-ROUTING.md`, `native-skill-hashes.json`, `native-skill-paths.json`).
- **Skills Root**: `105` `ak-*` skill directories verified under `C:\Users\manhquy\.gemini\config\skills\ak-*`.
- **CLI Inspection Caveat (`ak skills list` / `ak agents list` / `ak mcp list` / `ak doctor`)**:
  - `ak skills list` -> `No skills installed.`
  - `ak agents list` -> `No agents installed.`
  - `ak mcp list` -> `[i] No MCP servers found.`
  - `ak doctor` confirms why: `[i] Installed kits: install root C:\Users\manhquy\.claude\plugins does not exist, no plugin-delivered kits to verify (native installs are not checked here)` and `[OK] Kit cache: 1 kit(s), 1 cached version(s) verified`.
  - **Explanation**: `ak` subcommands for `skills`/`agents`/`mcp` query `~/.claude/`, whereas this host runs the **Antigravity native adapter** (`agy/global/native`).

---

## 5. REA MCP (`rea-agents` v6.9.0) Capabilities & Gaps

Verified from [`multi-zalo-desktop/evidence/rea-binary-session.json`](../../../multi-zalo-desktop/evidence/rea-binary-session.json) and [`multi-zalo-desktop/evidence/rea-evidence-summary.json`](../../../multi-zalo-desktop/evidence/rea-evidence-summary.json):

- **Server Identity**: `rea-agents` `v6.9.0`, MCP SDK `2.3.1`, negotiated protocol `2026-07-28`, exposing `255` MCP tools, `196` CLI commands, `6` MCP prompts.
- **Working Built-In Capabilities (Verified on Zalo Artifacts)**:
  - `inspect_pe_certificates` (`rea-pe-certificates`): extracts PKCS#7 Authenticode subject, issuer, serial, timestamp.
  - `inspect_pe_imports` (`rea-pe-imports`): extracts Import/Delay-load IAT/ILT RVAs and `imphash` (set `max_file_bytes = 268435456` for binaries > 64 MiB).
  - `inspect_binary_hardening` (`rea-binary-hardening`): reads `DYNAMIC_BASE`, `NX_COMPAT`, `GUARD_CF`, security/debug directory presence.
  - `inspect_pe_load_config` (`rea-pe-load-config`): inspects Load Config / Security Cookie / GuardFlags.
  - `inspect_pe_resources` (`rea-pe-resources`): enumerates `RT_ICON`, `RT_GROUP_ICON`, `RT_VERSION`, `RT_MANIFEST` (`asInvoker` verified in `ZaloSetup-26.11.12.exe`).
  - `inspect_electron_fuses` (`electron-fuses`): inspects Electron v1 fuse sentinel bits.
  - `open_binary` + `inspect_artifact` (`rea-artifact-graph`): indexes `app.asar` (`10,869` nodes, `13,170` files, `62` directories).
  - `audit_release_exposure` (`rea-release-exposure`): detects extractable ASAR payloads and CodeView PDB build paths.
- **Unavailable / Blocked Capabilities**:
  - `rea-windows-x64.node`: blocked by Windows Smart App Control (`ev_65a72c27603cd5f817368a4390e8a3b9d8d9ed1bf6592ecf0f57c4adfdc7ee4b`), disabling `analyze_javascript_application`.
  - External decompilers: `ghidra` (`not_configured` — requires `GHIDRA_INSTALL_DIR` >= 12.1), `ida` (`not_configured`), `kuna` (`not_configured`), `hopper` (`unsupported_host` on `win32`).
  - OS-specific providers: `native-macos` (`inspect_macho`, `inspect_plist`, `inspect_signature`, `inspect_asset_catalog`) and Linux debuggers/Wakaru (`gdb`, `rizin`, `recover_javascript_sources`) report `unsupported_host`.

---

## 6. Target Zalo Installation & Windows Shell Folder Preflight

### 6.1. Official Zalo Artifacts Observed on Host

All measurements below come from [`multi-zalo-desktop/evidence/rea-evidence-summary.json`](../../../multi-zalo-desktop/evidence/rea-evidence-summary.json), [`multi-zalo-desktop/docs/research/zalo-desktop-analysis.md`](../../../multi-zalo-desktop/docs/research/zalo-desktop-analysis.md), and direct read-only inspection of `%LOCALAPPDATA%\Programs\Zalo`:

| Component | Path | Size (bytes) | SHA-256 | Authenticode Signer |
| :--- | :--- | ---: | :--- | :--- |
| Official NSIS Installer | `C:\Users\manhquy\Downloads\Programs\ZaloSetup-26.11.12.exe` | `213,516,168` | `dddef07c11683561c6abe19c07bbc6202214136325d6b4b39311587a16a15eb8` | `CN=VNG GROUP JSC` |
| Stub Launcher | `%LOCALAPPDATA%\Programs\Zalo\Zalo.exe` | `790,568` | `7295232c89f9a041e35253afa80787989eae9a91895f1847ebd92c78bb4195a2` | `CN=VNG GROUP JSC` |
| Electron Host (`26.10.20`) | `%LOCALAPPDATA%\Programs\Zalo\Zalo-26.10.20\Zalo.exe` | `136,638,504` | `75dc85559079ec2037d511a312b704b44ca3293e74da1900ccfe84e6299cac3e` | `CN=VNG GROUP JSC` |
| Electron ASAR Bundle | `%LOCALAPPDATA%\Programs\Zalo\Zalo-26.10.20\resources\app.asar` | `177,728,582` | `d83256230e5d1d4483c2411f71936c3dc1562fb8a885fd8752cab17d229201e6` | N/A (ASAR archive) |

Also present in `%LOCALAPPDATA%\Programs\Zalo`: earlier version directories `Zalo-26.9.10` and `Zalo-26.10.11`, helper binaries `Update.exe`, `sl.exe`, `plugins\capture\ZaloCall.exe`, `plugins\capture\ZaloCap.exe`, `plugins\ocr\zocr-host.exe`.

### 6.2. Windows User Shell Folders (Folder Redirection Check)

Reading `HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\User Shell Folders` with `RegistryValueOptions.DoNotExpandEnvironmentNames`:

- `AppData` = `%USERPROFILE%\AppData\Roaming`
- `Local AppData` = `%USERPROFILE%\AppData\Local`
- `Personal` = `%USERPROFILE%\Documents`
- `Desktop` = `%USERPROFILE%\Desktop`

**Status:** Standard `%USERPROFILE%`-relative definitions are intact (no network/OneDrive redirection on `AppData`). Because enterprise/OneDrive hosts *can* redirect `User Shell Folders\AppData` away from `%USERPROFILE%\AppData\Roaming`, `dim-platform-windows` and `scripts/dim-host.ps1` enforce a mandatory preflight check before launching any isolated Windows profile.

---

## 7. Readiness Conclusion

- **Local Development & Verification Ready:** Yes, using signed `node.exe` (`v26.13.1`) + signed `powershell.exe` for the synthetic test suite (`tests/run-fixture-tests.mjs`), UI server (`apps/desktop/ui-server.mjs`), and live Zalo Desktop PoC (`scripts/run-zalo-poc.ps1`).
- **Rust Compilation & Release Signing:** Deferred to GitHub Actions CI ([`.github/workflows/ci.yml`](../../.github/workflows/ci.yml)) and requires an Authenticode signing certificate for standalone Windows distribution on SAC-enabled hosts (Gate F).

**Related Documents:** [repository-comparison.md](./repository-comparison.md) | [platform-feasibility.md](./platform-feasibility.md) | [os-readiness.md](../compatibility/os-readiness.md) | [adr-002-rust-tauri-and-sac-host-bridge.md](../architecture/adr-002-rust-tauri-and-sac-host-bridge.md)
