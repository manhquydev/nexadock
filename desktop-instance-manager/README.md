# NexaDock — Desktop Instance Manager

> **Your accounts. Their own space.** / **Một máy. Nhiều tài khoản. Không trộn dữ liệu.**

> **Tuyên bố phi liên kết / Non-Affiliation Notice:** Dự án độc lập, **không liên kết, không được tài trợ hay chứng thực bởi VNG Corporation hay Zalo**. `"Zalo"` là nhãn hiệu của VNG Corporation. Ứng dụng **không** đóng gói hoặc chỉnh sửa nhị phân của VNG (`MIT OR Apache-2.0`, xem [`../LICENSE-MIT`](../LICENSE-MIT), [`../LICENSE-APACHE`](../LICENSE-APACHE) và [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md)).

Cross-platform-structured multi-instance profile manager for **Zalo Desktop**, engineered around non-destructive environment & filesystem isolation (`APPDATA`/`LOCALAPPDATA`/`TEMP`/`TMP` on Windows; `CFFIXED_USER_HOME` on macOS; `HOME`/`XDG_*` + `bwrap` on Linux) without patching or unpacking official VNG binaries.

---

## Key Capabilities & Safety Guarantees

- **Zero Interference with Primary Zalo:** Your existing `%APPDATA%\ZaloData` profile and running primary Zalo process are never killed, modified, backed up over, or restored over (`PROTECTED_PRIMARY_PROCESS` PID + process-tree enforcement, `PROTECTED_PRIMARY_PATH` descendant guards).
- **Concurrent Isolated Instances:** Tested with `3` independent official Zalo Desktop `26.10.20` windows running simultaneously alongside the user's logged-in primary Zalo (`primary_files_sha256_identical = true`).
- **Pre-Launch Isolation & Authenticode Trust:** Both Rust (`WinVerifyTrust` + CryptoAPI `CN/O=VNG GROUP JSC` RDN pinning) and PowerShell enforce cryptographic Authenticode trust before launch, verify `.zalo-profile-id` ownership, block duplicate running `zalo_user_id` accounts (`DUPLICATE_ZALO_USER_ID`), check Folder Redirection / UNC paths, and validate restrictive NTFS ACLs / UNIX `0o700` permissions.
- **Offline Atomic Backup & Non-Destructive Restore:** Writes `.partial` manifests verified via SHA-256 -> `backup.json`, checks `5 GiB + payloadEstimate` free space, unpacks into `.restore-<stamp>-<id>` staging directories with rollback & `12h` stale staging cleanup, and restores exclusively into new profile directories.
- **Zero-Dependency Modern Web UI & Tauri 2 Shell:** Native `<dialog>` invokers, `@container` responsive tables, dual-mode `light-dark()` + `prefers-color-scheme`, and `focusgroup` (`role="tablist"`) behind a loopback-only HTTP server (`127.0.0.1`) enforcing `X-DIM-Session-Token`, strict `CSP`, `Host`/`Origin`/`Sec-Fetch-Site`, rate limiting, and mutation queueing — paired with native Tauri 2 commands in [`apps/desktop/src-tauri/`](apps/desktop/src-tauri/).

---

## Cross-Platform Support Status (Verified Matrix)

| Platform / Mode | Support Status | Evidence & Honest Caveats |
|---|---|---|
| **Windows 10/11 — Core Multi-Profile Isolation** (Signed-Host Bridge + Native Rust `dim-cli.exe` / `dim-desktop.exe`) | **SUPPORTED_AND_TESTED** | `29` Rust unit tests (`cargo test`), Tauri 2 release build, `8/8` fixture/security tests, PowerShell-vs-Rust contract parity tests, and `3` concurrent official Zalo `26.10.20` PoC windows verified alongside primary Zalo. |
| **Windows 10/11 — Live Multi-Account Interactive Chat / Calls / Toasts** | **PARTIALLY_SUPPORTED** | **L3 (`NOT_TESTED`):** multi-account QR chat/file sync awaits manual interactive checklist ([`scripts/run-interactive-l3-checklist.ps1`](scripts/run-interactive-l3-checklist.ps1)). **L4 (`BLOCKED`):** concurrent `ZaloCall.exe` voice/video (`\\.\pipe\PipeZCallRecv`/`PipeZCallSend`) and isolated per-profile toast click attribution (`AppUserModelId("com.vng.zalo")`). |
| **Windows — Standalone Unsigned PE Distribution (`dim-desktop.exe`, `dim-cli.exe`)** | **BLOCKED (Gate F)** | Locally built PE32+ binaries are **unsigned** (`IMAGE_DIRECTORY_ENTRY_SECURITY` absent); public distribution onto Windows Smart App Control (`VerifiedAndReputablePolicyState = 1`) hosts requires an Authenticode code-signing certificate. CI config lives at [`.github/workflows/ci.yml`](../.github/workflows/ci.yml). |
| **macOS** ([`crates/platform-macos`](crates/platform-macos/)) | **NOT_TESTED** | Uses `open -n -a ... --env CFFIXED_USER_HOME=...` and parameterized `osascript`; `/tmp/socketzalorecv2021` shared socket caveat noted. |
| **Ubuntu / Linux — Official Zalo Desktop** | **UNSUPPORTED** | No official native Linux Zalo Desktop release; [`crates/platform-linux`](crates/platform-linux/) explicitly returns `UnsupportedPlatform`. |
| **Ubuntu / Linux — Local `.AppImage` / Fixture** | **EXPERIMENTAL (`NOT_TESTED` on Linux host)** | `bwrap` (`bubblewrap`) + `HOME`/`XDG_*` sandbox isolation implemented; requires Ubuntu `23.10+`/`24.04` AppArmor `--no-sandbox` handling when unprivileged namespaces are restricted. |

---

## Quickstart

All commands below assume [`desktop-instance-manager/`](./) as the working directory. See [`docs/user-guide/windows-mvp-guide.md`](docs/user-guide/windows-mvp-guide.md) for the complete Vietnamese end-user manual.

### 1. Launch Desktop Management UI (`http://127.0.0.1:18420/`)
```powershell
node apps/desktop/ui-server.mjs
```
Optional flags: `--port <n>` (or `DIM_PORT`), `--data-dir <path>`, `--node-fixture`, `--app-path <path>`.

### 2. Operate Directly via Signed-Host CLI or Native Rust CLI
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dim-host.ps1 --cli --json status
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dim-host.ps1 --cli --json diagnostics
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dim-host.ps1 --cli --json -- create "Cong viec"

# Or native Rust CLI after `cargo build --workspace --release`:
.\target\release\dim-cli.exe --cli --json status
```

### 3. Build, Test & Verify Provenance
```powershell
# Rust workspace & Tauri 2 build + tests (requires Rust >= 1.85 for workspace, >= 1.90 for Tauri 2, and MinGW-w64 / MSVC linker on Windows)
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
cargo build --workspace --release
cargo build --manifest-path apps/desktop/src-tauri/Cargo.toml --release

# Full 8-suite PowerShell + UI integration suite
node tests/run-fixture-tests.mjs --out evidence/fixture-test-result.json

# PowerShell vs Rust native dim-cli.exe contract parity suite
node tests/run-contract-parity-tests.mjs --out evidence/contract-parity-result.json

# Reproducible build manifest (SHA-256), CycloneDX SBOM, and SPDX license audit
node scripts/generate-build-provenance.mjs

# Live Official Zalo Desktop PoC (non-destructive; preserves %APPDATA%\ZaloData)
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run-zalo-poc.ps1

# Guided L3/L4 manual QR login checklist
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run-interactive-l3-checklist.ps1
```

---

## Workspace Structure & Documentation

- **User Guide & Post-Audit Hardening Report:**
  - [`docs/user-guide/windows-mvp-guide.md`](docs/user-guide/windows-mvp-guide.md)
  - [`docs/reviews/post-swarm-hardening-report.md`](docs/reviews/post-swarm-hardening-report.md)
- **Rust Workspace & CI:** [`../.github/workflows/ci.yml`](../.github/workflows/ci.yml), [`Cargo.toml`](Cargo.toml), [`deny.toml`](deny.toml), [`crates/core/`](crates/core/), [`crates/profile-store/`](crates/profile-store/), [`crates/process-manager/`](crates/process-manager/), [`crates/platform-windows/`](crates/platform-windows/), [`crates/platform-macos/`](crates/platform-macos/), [`crates/platform-linux/`](crates/platform-linux/)
- **Desktop Shell, Bridge & UI:** [`apps/desktop/src-tauri/`](apps/desktop/src-tauri/), [`apps/desktop/ui-server.mjs`](apps/desktop/ui-server.mjs), [`apps/desktop/ui/`](apps/desktop/ui/)
- **Windows Host Bridge & Scripts:** [`scripts/dim-host.ps1`](scripts/dim-host.ps1), [`scripts/run-fixture-poc.ps1`](scripts/run-fixture-poc.ps1), [`scripts/run-zalo-poc.ps1`](scripts/run-zalo-poc.ps1), [`scripts/run-interactive-l3-checklist.ps1`](scripts/run-interactive-l3-checklist.ps1), [`scripts/run-tests.ps1`](scripts/run-tests.ps1), [`scripts/generate-build-provenance.mjs`](scripts/generate-build-provenance.mjs)
- **Research & Architecture:**
  - [`docs/research/environment-readiness.md`](docs/research/environment-readiness.md)
  - [`docs/research/repository-comparison.md`](docs/research/repository-comparison.md)
  - [`docs/research/platform-feasibility.md`](docs/research/platform-feasibility.md)
  - [`docs/compatibility/os-readiness.md`](docs/compatibility/os-readiness.md)
  - [`docs/architecture/adr-001-hybrid-isolation-architecture.md`](docs/architecture/adr-001-hybrid-isolation-architecture.md)
  - [`docs/architecture/adr-002-rust-tauri-and-sac-host-bridge.md`](docs/architecture/adr-002-rust-tauri-and-sac-host-bridge.md)
  - [`docs/architecture/adr-003-process-ownership-and-graceful-shutdown.md`](docs/architecture/adr-003-process-ownership-and-graceful-shutdown.md)
  - [`docs/architecture/adr-004-backup-restore-and-data-safety.md`](docs/architecture/adr-004-backup-restore-and-data-safety.md)
- **Security Audit, Verification Reports & Evidence:**
  - [`docs/security/threat-model-and-audit.md`](docs/security/threat-model-and-audit.md)
  - [`docs/testing/poc-and-test-report.md`](docs/testing/poc-and-test-report.md)
  - [`evidence/fixture-test-result.json`](evidence/fixture-test-result.json)
  - [`evidence/contract-parity-result.json`](evidence/contract-parity-result.json)
  - [`evidence/zalo-live-poc-result.json`](evidence/zalo-live-poc-result.json)
  - [`evidence/artifact-manifest.json`](evidence/artifact-manifest.json) | [`evidence/sbom-cyclonedx.json`](evidence/sbom-cyclonedx.json) | [`evidence/dependency-audit.json`](evidence/dependency-audit.json)
