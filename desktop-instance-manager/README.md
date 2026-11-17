# Desktop Instance Manager (Multi-Zalo Desktop v2)

Cross-platform-structured multi-instance profile manager for **Zalo Desktop**, engineered around non-destructive environment & filesystem isolation (`APPDATA`/`LOCALAPPDATA`/`TEMP`/`TMP` on Windows; `CFFIXED_USER_HOME` on macOS; `HOME`/`XDG_*` + `bwrap` on Linux) without patching or unpacking official VNG binaries.

---

## Key Capabilities & Safety Guarantees

- **Zero Interference with Primary Zalo:** Your existing `%APPDATA%\ZaloData` profile and running primary Zalo process are never killed, modified, backed up over, or restored over (`PROTECTED_PRIMARY_PROCESS` PID + process-tree enforcement).
- **Concurrent Isolated Instances:** Tested with `3` independent official Zalo Desktop `26.10.20` windows running simultaneously alongside the user's logged-in primary Zalo (`primary_files_sha256_identical = true`).
- **Pre-Launch Isolation & Authenticode Trust:** Verifies `WinVerifyTrust` + `CN=VNG GROUP JSC` signature before launch, checks `.zalo-profile-id` directory ownership, blocks duplicate running `zalo_user_id` accounts (`DUPLICATE_ZALO_USER_ID`), checks Folder Redirection / UNC paths, and validates restrictive NTFS ACLs / UNIX `0o700` permissions.
- **Offline Atomic Backup & Non-Destructive Restore:** Writes `.partial` manifests verified via SHA-256 -> `backup.json`, checks `5 GiB + payloadEstimate` free space, unpacks into `.restore-<stamp>-<id>` staging directories with rollback & `12h` stale staging cleanup, and restores exclusively into new profile directories.
- **Zero-Dependency Modern Web UI:** Native `<dialog>` invokers, `@container` responsive tables, dual-mode `light-dark()` + `prefers-color-scheme`, and `focusgroup` / `hidden="until-found"` tabs behind a loopback-only HTTP server (`127.0.0.1`) rejecting foreign `Host`/`Origin`/`Sec-Fetch-Site` headers.

---

## Cross-Platform Support Status (Verified Matrix)

| Platform / Mode | Support Status | Evidence & Honest Caveats |
|---|---|---|
| **Windows 10/11 — Core Multi-Profile Isolation** (via Signed-Host Bridge) | **SUPPORTED_AND_TESTED** | `8/8` fixture/security tests + `3` concurrent official Zalo `26.10.20` PoC instances verified alongside primary `PID 12980`. |
| **Windows 10/11 — Full Desktop Feature Parity** | **PARTIALLY_SUPPORTED** | **BLOCKED:** concurrent `ZaloCall.exe` voice/video (`\\.\pipe\PipeZCallRecv`/`PipeZCallSend`) and isolated per-profile toast click attribution (`AppUserModelId("com.vng.zalo")`). |
| **Windows — Compiled Rust / Tauri Standalone PE** | **BLOCKED (Gate F)** | Workspace crates compile in CI ([`.github/workflows/ci.yml`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/.github/workflows/ci.yml)); production PE distribution onto Windows Smart App Control hosts requires CI compilation + Authenticode code-signing certificate. |
| **macOS** ([`crates/platform-macos`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/crates/platform-macos)) | **NOT_TESTED** | Uses `open -n -a ... --env CFFIXED_USER_HOME=...` and parameterized `osascript`; `/tmp/socketzalorecv2021` shared socket noted. |
| **Ubuntu / Linux — Official Zalo Desktop** | **UNSUPPORTED** | No official native Linux Zalo Desktop release; [`crates/platform-linux`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/crates/platform-linux) explicitly returns `UnsupportedPlatform`. |
| **Ubuntu / Linux — Local `.AppImage` / Fixture** | **EXPERIMENTAL** | `bwrap` (`bubblewrap`) + `HOME`/`XDG_*` sandbox isolation implemented; requires Ubuntu `23.10+`/`24.04` AppArmor `--no-sandbox` handling when unprivileged namespaces are restricted. |

---

## Quickstart (Windows Signed-Host Bridge — Zero Compilation Required)

All commands assume [`desktop-instance-manager/`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager) as the working directory:

### 1. Launch Desktop Management UI (`http://127.0.0.1:18420/`)
```powershell
node apps/desktop/ui-server.mjs
```
Optional flags: `--port <n>` (or `DIM_PORT`), `--data-dir <path>`, `--node-fixture`, `--app-path <path>`.

### 2. Operate Directly via CLI
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dim-host.ps1 --cli --json status
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dim-host.ps1 --cli --json diagnostics
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dim-host.ps1 --cli --json -- create "Cong viec"
```

### 3. Run Automated Tests & PoCs
```powershell
# Safe isolated fixture PoC
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run-fixture-poc.ps1

# Full 8-suite integration + security regression suite
node tests/run-fixture-tests.mjs --out evidence/fixture-test-result.json

# Live Official Zalo Desktop PoC (non-destructive; preserves %APPDATA%\ZaloData)
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run-zalo-poc.ps1
```

*(Note: Rust unit tests — `25` `#[test]` functions across `crates/*/src/lib.rs` — and `cargo-deny` run via [`.github/workflows/ci.yml`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/.github/workflows/ci.yml) on hosts with `rustc`/`cargo` installed.)*

---

## Workspace Structure & Documentation

- **Rust Workspace:** [`Cargo.toml`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/Cargo.toml), [`deny.toml`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/deny.toml), [`crates/core/`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/crates/core), [`crates/profile-store/`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/crates/profile-store), [`crates/process-manager/`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/crates/process-manager), [`crates/platform-windows/`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/crates/platform-windows), [`crates/platform-macos/`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/crates/platform-macos), [`crates/platform-linux/`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/crates/platform-linux)
- **Desktop Shell & UI:** [`apps/desktop/src-tauri/`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/apps/desktop/src-tauri), [`apps/desktop/ui-server.mjs`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/apps/desktop/ui-server.mjs), [`apps/desktop/ui/`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/apps/desktop/ui)
- **Windows Host Bridge & Scripts:** [`scripts/dim-host.ps1`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/scripts/dim-host.ps1), [`scripts/run-fixture-poc.ps1`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/scripts/run-fixture-poc.ps1), [`scripts/run-zalo-poc.ps1`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/scripts/run-zalo-poc.ps1), [`scripts/run-tests.ps1`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/scripts/run-tests.ps1)
- **Research & Architecture:**
  - [`docs/research/environment-readiness.md`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/docs/research/environment-readiness.md)
  - [`docs/research/repository-comparison.md`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/docs/research/repository-comparison.md)
  - [`docs/research/platform-feasibility.md`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/docs/research/platform-feasibility.md)
  - [`docs/compatibility/os-readiness.md`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/docs/compatibility/os-readiness.md)
  - [`docs/architecture/adr-001-hybrid-isolation-architecture.md`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/docs/architecture/adr-001-hybrid-isolation-architecture.md)
  - [`docs/architecture/adr-002-rust-tauri-and-sac-host-bridge.md`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/docs/architecture/adr-002-rust-tauri-and-sac-host-bridge.md)
  - [`docs/architecture/adr-003-process-ownership-and-graceful-shutdown.md`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/docs/architecture/adr-003-process-ownership-and-graceful-shutdown.md)
  - [`docs/architecture/adr-004-backup-restore-and-data-safety.md`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/docs/architecture/adr-004-backup-restore-and-data-safety.md)
- **Security Audit, Verification Reports & Evidence:**
  - [`docs/security/threat-model-and-audit.md`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/docs/security/threat-model-and-audit.md)
  - [`docs/testing/poc-and-test-report.md`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/docs/testing/poc-and-test-report.md)
  - [`evidence/fixture-test-result.json`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/evidence/fixture-test-result.json)
  - [`evidence/zalo-live-poc-result.json`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/evidence/zalo-live-poc-result.json)
