# Post-Swarm Hardening, Build & Validation Report — Desktop Instance Manager

**Target Project:** [`desktop-instance-manager/`](../../) (Repository root: [`zalo`](../../../README.md))  
**Date (UTC):** `2026-11-17`  
**Methodology:** Independent Audit -> Hardening Remediation -> Local Windows Rust/Tauri Compilation -> Cross-Implementation Parity Suite -> Dual Independent Reviews (`code-reviewer` + `tester`) -> Remediation Re-Verification

---

## 1. Baseline Commit & Repository State

- **Baseline Commit Audited:** `8cf47035f0f06bd97cc99aff80d81fb10b103af8` (clean working tree on `main` at start of continuation).
- **Primary Architecture Preserved:** Hybrid environment & filesystem isolation (`APPDATA`/`LOCALAPPDATA`/`TEMP`/`TMP` on Windows; `CFFIXED_USER_HOME` on macOS; `HOME`/`XDG_*` + `bwrap` on Linux) over **official VNG Zalo Desktop**, without patching `Zalo.exe`/`app.asar` and without replacing Zalo Desktop with Zalo Web.

---

## 2. Independent Code Audit & Reviewer Findings Summary

Findings gathered across the initial independent code audit and Phase 10 independent reviews (`code-reviewer` and `tester`):

| Finding ID & Origin | Status | Location | Defect & Resolution Applied |
|---|---|---|---|
| **F-01** (Initial Audit) | **CONFIRMED -> FIXED** | `.github/workflows/ci.yml` | CI YAML lived inside `desktop-instance-manager/.github/workflows/ci.yml` instead of repository root; used relative paths without `working-directory: desktop-instance-manager` and had `if` checks hiding missing Tauri/test runs. Moved to root [`.github/workflows/ci.yml`](../../../.github/workflows/ci.yml) with `defaults.run.working-directory` and direct checks. |
| **F-02** (Initial Audit) | **CONFIRMED -> FIXED** | `Cargo.toml`, `apps/desktop/src-tauri/Cargo.toml` | Workspace MSRV `1.78` failed resolving `getrandom v0.4.3` (`edition2024`); Tauri `2.12.6` / `sha2 0.11.1` required `1.90` (`1.85` failed check by Phase 10 reviewer). Updated workspace MSRV to `1.85` and `src-tauri` MSRV to `1.90`. |
| **F-03** (Initial Audit) | **CONFIRMED -> FIXED** | `crates/core/src/lib.rs:116` | Baseline unit test `data_dir_resolution_matches_platform_contracts` failed to compile (`E0502` borrow error on `env` closure). Fixed closure rebind (`lookup_over`). |
| **F-04** (Initial Audit) | **CONFIRMED -> FIXED** | `crates/platform-windows/src/{adapter.rs,win32_ffi.rs}` | Rust native Windows adapter previously checked `has_pe_authenticode_table()` only. Replaced with `WinVerifyTrust` (`WINTRUST_ACTION_GENERIC_VERIFY_V2`) + CryptoAPI signer extraction. |
| **F-05** (Initial Audit) | **CONFIRMED -> FIXED** | `apps/desktop/src-tauri/src/commands.rs` | Previously hardcoded `delete(..., true)`, `is_elevated: false`, `PrimaryZaloSeparation` `PASS`, and primary RAM `0`. Replaced with explicit `confirm`, live OS probes, real `verify_isolation`, and `working_set_bytes`. |
| **F-06** (Initial Audit) | **CONFIRMED -> FIXED** | `apps/desktop/ui-server.mjs` | Lacked per-instance auth token, rate limiting, timeouts, mutation lock, error sanitization, and external backup restriction. Added `X-DIM-Session-Token`, `30s` timeout (`504`), `429` limiter, `enqueueMutation`, `sanitizeHostError`, and `403 FORBIDDEN_DEST`. |
| **F-07** (Self-Test Catch) | **CONFIRMED -> FIXED** | `crates/platform-windows/src/win32_ffi.rs:544`, `crates/profile-store/src/store.rs:369`, `crates/platform-windows/src/layout.rs` | (a) `CreateProcessW` omitted `CREATE_NO_WINDOW`, leaving console children holding parent pipes in `spawnSync`; (b) `unlink_reparse_points_safe` tested `meta.is_dir()` (always `false` on Windows `symlink_metadata`), causing `os error 5` on NTFS junctions -> fixed using `FILE_ATTRIBUTE_DIRECTORY` + `fs::remove_dir`; (c) added `Desktop/` folder parity in `ensure_windows_profile_layout`. |
| **F-08** (`code-reviewer` Critical #1) | **CONFIRMED -> FIXED** | `crates/platform-windows/src/win32_ffi.rs:1244` | `CertGetNameStringW(CERT_NAME_RDN_TYPE, ...)` passed `CERT_NAME_STR_REVERSE_FLAG` in `dwFlags` and `NULL` in `pvTypePara`. Fixed to pass `&mut (CERT_X500_NAME_STR \| CERT_NAME_STR_REVERSE_FLAG)` per CryptoAPI contract. |
| **F-09** (`code-reviewer` Critical #2) | **CONFIRMED -> FIXED** | `scripts/run-interactive-l3-checklist.ps1:25` | Passed invalid `-DataDir` flag causing `UNKNOWN_COMMAND`. Fixed to `--cli --json --data-dir $DataDir -- @CmdArgs` and added operator note sanitization. |
| **F-10** (`code-reviewer` & `tester` High) | **CONFIRMED -> FIXED** | `win32_ffi.rs`, `adapter.rs`, `dim-host.ps1` | (a) Loose `"VNG"` substring publisher match -> replaced with strict X.500 `CN`/`O=VNG GROUP JSC` RDN pinning; (b) `dim-host.ps1` set `valid = true` on tampered `Zalo.exe` / `custom-exe` and skipped Authenticode check on `custom-exe` launch -> now fails closed (`UNVERIFIED_BINARY`, `valid = false`). |
| **F-11** (`code-reviewer` & `tester` Medium) | **CONFIRMED -> FIXED** | `dim-host.ps1`, `path_guard.rs`, `sanitize.rs`, `coordinator.rs`, `commands.rs`, `index.html`, `app.js`, `ui-server.mjs`, `generate-build-provenance.mjs` | Fixed `DetectZaloUserId` (`config.json` `db_path.location` + mtime), file-symlink unlinking, primary path descendant/trailing-slash checks, `paths_overlap_or_descend` deepest ancestor canonicalize, `NotTested` / `inspect_profile` error reporting in `verify_isolation`, `AppState::op_lock` & `backups_dir` restore confinement in Tauri, `role="tablist"` + `aria-controls` + `state.busy` guards, full CSP parity, and SBOM/audit inclusion of `apps/desktop/src-tauri/Cargo.lock` (439 unique crates). |
| **F-12** (`tester` Note) | **CONFIRMED -> ACCEPTED / DOCUMENTED** | `win32_ffi.rs` (`WTD_REVOKE_NONE`), `run-fixture-tests.mjs` (`PROTECTED_PRIMARY_PROCESS` on CI), `dim-test-fixture.exe` | Online CRL/OCSP revocation (`WTD_REVOKE_NONE`) kept offline-safe for isolated/offline corporate runs; `Test-8` skips live PID probe only when Zalo Desktop is not running on headless CI runners (`PROTECTED_PRIMARY_PATH` and Rust unit tests remain active). |
| **F-13** (Audit Check) | **FALSE_POSITIVE** | `crates/platform-macos/`, `crates/platform-linux/` | Cross-platform stub/experimental boundaries were intentionally preserved with honest `NOT_TESTED` / `UNSUPPORTED` / `EXPERIMENTAL` reporting. |

---

## 3. Complete Inventory of Files Changed, Created & Removed

- **Moved / Created:**
  - `.github/workflows/ci.yml` (relocated from `desktop-instance-manager/.github/workflows/ci.yml` and expanded)
  - `desktop-instance-manager/Cargo.lock`, `desktop-instance-manager/apps/desktop/src-tauri/Cargo.lock`
  - `desktop-instance-manager/apps/desktop/src-tauri/icons/{32x32.png,128x128.png,icon.png,icon.ico}`
  - `desktop-instance-manager/fixtures/test-desktop-app/src/bin/dim-cli.rs`
  - `desktop-instance-manager/scripts/generate-build-provenance.mjs`
  - `desktop-instance-manager/scripts/run-interactive-l3-checklist.ps1`
  - `desktop-instance-manager/tests/run-contract-parity-tests.mjs`
  - `desktop-instance-manager/evidence/{artifact-manifest.json,contract-parity-result.json,dependency-audit.json,sbom-cyclonedx.json}`
  - `desktop-instance-manager/docs/user-guide/windows-mvp-guide.md`
  - `desktop-instance-manager/docs/reviews/post-swarm-hardening-report.md`
- **Modified:**
  - `.gitignore`, `README.md`, `desktop-instance-manager/README.md`
  - `desktop-instance-manager/Cargo.toml`, `desktop-instance-manager/deny.toml`, `desktop-instance-manager/apps/desktop/src-tauri/Cargo.toml`
  - `desktop-instance-manager/crates/core/src/{lib,error,models,path_guard,sanitize,slug,traits}.rs`
  - `desktop-instance-manager/crates/profile-store/src/{lib,store,backup}.rs`
  - `desktop-instance-manager/crates/process-manager/src/{lib,coordinator,recovery}.rs`
  - `desktop-instance-manager/crates/platform-windows/src/{lib,adapter,layout,win32_ffi}.rs`
  - `desktop-instance-manager/crates/platform-macos/{Cargo.toml,src/{lib,adapter}.rs}`
  - `desktop-instance-manager/crates/platform-linux/{Cargo.toml,src/{lib,adapter,layout}.rs}`
  - `desktop-instance-manager/fixtures/test-desktop-app/{Cargo.toml,src/main.rs}`
  - `desktop-instance-manager/apps/desktop/src-tauri/{tauri.conf.json,src/commands.rs}`
  - `desktop-instance-manager/apps/desktop/{ui-server.mjs,ui/{index.html,styles.css,app.js}}`
  - `desktop-instance-manager/scripts/{dim-host.ps1,run-zalo-poc.ps1}`
  - `desktop-instance-manager/tests/run-fixture-tests.mjs`
  - `desktop-instance-manager/evidence/{fixture-test-result.json,zalo-live-poc-result.json}`
  - `desktop-instance-manager/docs/{research/environment-readiness.md,compatibility/os-readiness.md,security/threat-model-and-audit.md,testing/poc-and-test-report.md}`
  - `desktop-instance-manager/plans/261117-zalo-instance-manager/plan.md`
- **Removed:**
  - `desktop-instance-manager/.github/workflows/ci.yml`

---

## 4. CI & Build System Remediation

- **GitHub Actions Root Relocation:** Workflow moved to [`.github/workflows/ci.yml`](../../../.github/workflows/ci.yml) using `defaults.run.working-directory: desktop-instance-manager` and explicit cache keys (`desktop-instance-manager/**/Cargo.toml`, `desktop-instance-manager/Cargo.lock`).
- **Toolchain Established Locally:** Official user-scope `rustup` (`stable-x86_64-pc-windows-gnu`, `rustc 1.100.0`, `cargo 1.100.0`, `rustfmt 1.11.0-stable`, `clippy 0.1.100`) paired with official `winget` package `BrechtSanders.WinLibs.POSIX.UCRT` (`16.2.0-14.0.0-r3`, providing `gcc.exe`, `as.exe`, `dlltool.exe`, `windres.exe`).
- **Accurate MSRV & Reproducible Lockfiles:** Workspace MSRV `1.85` ([`Cargo.toml`](../../Cargo.toml)), Tauri 2 crate MSRV `1.90` ([`apps/desktop/src-tauri/Cargo.toml`](../../apps/desktop/src-tauri/Cargo.toml)); locked dependency graphs committed in [`Cargo.lock`](../../Cargo.lock) (`41` packages) and [`apps/desktop/src-tauri/Cargo.lock`](../../apps/desktop/src-tauri/Cargo.lock) (`434` packages).

---

## 5. Rust Native Security Parity

- **`WinVerifyTrust` + CryptoAPI Contract:** [`win32_ffi.rs`](../../crates/platform-windows/src/win32_ffi.rs) enforces `WINTRUST_ACTION_GENERIC_VERIFY_V2`, extracts the signer certificate, calls `CertGetNameStringW(CERT_NAME_RDN_TYPE, 0, &mut (CERT_X500_NAME_STR | CERT_NAME_STR_REVERSE_FLAG), ...)`, and pins Official Zalo to X.500 `CN=VNG GROUP JSC` or `O=VNG GROUP JSC`. `#[cfg(test)]` gates `with_signature_enforcement` in `adapter.rs`.
- **Native Isolation & OS Diagnostics:** `verify_isolation` in [`coordinator.rs`](../../crates/process-manager/src/coordinator.rs) returns real `PASS`/`FAIL`/`NOT_TESTED`/`BLOCKED` checks (never hardcoded `PASS`, checks `NotTested` before `is_valid`, checks `is_symlink_or_reparse` on `ZaloData`, fails on broken profile inspection). [`win32_ffi.rs`](../../crates/platform-windows/src/win32_ffi.rs) probes real `TokenElevation` (`is_elevated`), `HKLM\...\CI\Policy` (`smart_app_control_state`), and `GetNamedSecurityInfoW` DACLs (`profiles_acl_safe`).
- **NTFS Junction & Console Process Safety:** `unlink_reparse_points_safe` in [`store.rs`](../../crates/profile-store/src/store.rs) detects Windows `FILE_ATTRIBUTE_DIRECTORY` reparse points and calls `fs::remove_dir` instead of `fs::remove_file`. `spawn_process_isolated` includes `CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP` with `bInheritHandles = 0`.
- **Tauri Backend Hardening:** [`commands.rs`](../../apps/desktop/src-tauri/src/commands.rs) adds `AppState::op_lock`, confines `restore_backup` strictly inside `backups_dir`, reads `backup.json` metadata directly in `list_backups`, supports `with_size`, and redacts errors + `folder_redirection.warning`.

---

## 6. Local Host Bridge & Web UI Hardening

- **Server Auth & Isolation ([`ui-server.mjs`](../../apps/desktop/ui-server.mjs)):** Generates per-process random token injected into `<meta name="dim-session-token">`, verifies `X-DIM-Session-Token` via `crypto.timingSafeEqual`, redacts `/api/health` host paths, enforces `127.0.0.1` loopback `Host`/`Origin`/`Sec-Fetch-Site` + `Content-Type: application/json`, adds `30s` command timeout (`504 HOST_TIMEOUT`), `300/min` rate limiter (`429 RATE_LIMITED`), `enqueueMutation` lock, `sanitizeHostError`, `403 FORBIDDEN_DEST` for external backup destinations, and clean shutdown handlers.
- **CSP & Accessibility ([`index.html`](../../apps/desktop/ui/index.html), [`app.js`](../../apps/desktop/ui/app.js)):** Static/API responses enforce `Content-Security-Policy: default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'`. Added `role="tablist"`, `aria-controls`, `'focusGroup' in HTMLElement.prototype`, `state.busy` guards, and `:focus-visible` outlines.

---

## 7. PowerShell vs Rust Contract Parity Verification

Executed `node tests/run-contract-parity-tests.mjs --out evidence/contract-parity-result.json` (`ok: true`, `executed_at_utc: 2026-11-17T15:30:53.306Z`) comparing [`scripts/dim-host.ps1`](../../scripts/dim-host.ps1) against `target/release/dim-cli.exe` across:
1. `discover` (`fixture_valid: true`, tampered PE `tampered_valid: false`, `Unverified (WinVerifyTrust=0xC000A000)`) and `UNVERIFIED_BINARY` launch refusal.
2. Vietnamese profile creation (`duc-cong-viec_<id>`) & `INVALID_NAME` rejection.
3. Live rename preserving immutable directory.
4. `focus` stopped (`NOT_RUNNING`), running (`ok: true`), and `status --with-size`.
5. Running guard on `backup`/`delete` (`PROFILE_RUNNING`).
6. Duplicate `zalo_user_id` guard (`DUPLICATE_ZALO_USER_ID`).
7. `verify-isolation` (all 7 checks `PASS`) & live `diagnostics` (`is_elevated: false`, `sac: 0`, `acl: true`).
8. Offline atomic backup, `BACKUP_ERROR` SHA-256 tamper rejection, non-destructive restore.
9. `CONFIRM_REQUIRED` without `--confirm`, NTFS `mklink /J` junction preservation, `PATH_TRAVERSAL`, and `PROTECTED_PRIMARY_PATH`.

---

## 8. Tests Executed vs Not Executed

| Suite / Check | Status | Result / Observation |
|---|---|---|
| `cargo fmt --all -- --check` & `src-tauri` fmt check | **PASS** | Zero formatting diffs |
| `cargo clippy --workspace --all-targets -- -D warnings` | **PASS** | Zero warnings across 7 workspace crates |
| `cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml -- -D warnings` | **PASS** | Zero clippy warnings (MinGW GNU `ld.exe` emitted upstream `.rsrc merge failure` linker note) |
| `cargo test --workspace --all-targets` | **PASS (`29/29`)** | `dim-core` (9), `dim-platform-linux` (3), `dim-platform-macos` (3), `dim-platform-windows` (6), `dim-process-manager` (2), `dim-profile-store` (4), `dim-test-fixture` (2) |
| `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml` | **PASS (`0` tests)** | Tauri test target compiled & ran cleanly |
| `node tests/run-contract-parity-tests.mjs --out evidence/contract-parity-result.json` | **PASS** | `ok: true`, Rust `dim-cli.exe` == PowerShell `dim-host.ps1` |
| `node tests/run-fixture-tests.mjs --out evidence/fixture-test-result.json` | **PASS (`8/8`)** | `15:38:11Z`, `primary_pid_protected: "verified_pid_12980"`, `ui_server_port: 60391`, `401`/`403`/`415` HTTP security verified |
| `node scripts/generate-build-provenance.mjs` | **PASS** | `3` artifacts hashed, `439` SBOM components, `policy_check_passed: true` |
| `node --check` (5 `.mjs` files + `apps/desktop/ui/app.js`) | **PASS** | All 6 JS files syntactically valid |
| Personal path / leak scan across `evidence/*.json` | **PASS** | `0` hits |
| GitHub Actions remote execution | **BLOCKED** | `gh auth status` scopes (`gist`, `read:org`, `repo`) lack `workflow` scope (see Section 9) |
| Interactive L3 multi-account QR chat/file sync | **NOT_TESTED** | Requires human QR sign-in via `scripts/run-interactive-l3-checklist.ps1` |
| Tauri interactive GUI window smoke (`dim-desktop.exe`) | **NOT_TESTED** | Release `.exe` built & verified structurally via REA (`PE32+` unsigned); interactive GUI launch awaits signed/eval run |
| Physical macOS & Ubuntu hardware runtime tests | **NOT_TESTED** | No Mac/Linux hardware in this Windows session |

---

## 9. GitHub Actions CI Status

- Local inspection via `gh.exe auth status`: logged in to `github.com` with OAuth scopes `'gist', 'read:org', 'repo'` (**missing `'workflow'` scope**).
- `gh.exe workflow list` and `gh.exe run list --limit 5` report **0** registered workflows/runs on `origin`.
- Attempted `git push origin main` after committing root `.github/workflows/ci.yml` (`0806ffb7b0f92f82cb85f85c7d785e3ee3768472`); GitHub rejected the push (`exit 1`):
  > `! [remote rejected] main -> main (refusing to allow an OAuth App to create or update workflow .github/workflows/ci.yml without workflow scope)`
- **Verdict:** GitHub Actions remote workflow execution is **BLOCKED** pending `gh auth refresh -s workflow` (or pushing via a credential/PAT that grants `workflow` scope). All CI checks (`fmt`, `clippy`, `cargo test`, Tauri build, Node fixture + contract parity + provenance audit) were run and verified locally on Windows 11.

---

## 10. Build Artifacts & Provenance ([`artifact-manifest.json`](../../evidence/artifact-manifest.json))

- **Generated (UTC):** `2026-11-17T15:38:14.965Z` (`rustc 1.100.0`, `cargo 1.100.0`, `win32-x64`)
- **Lockfile Hashes:**
  - `Cargo.lock` (`41` packages): `97d5d01eb7eea2f82d2d272c97738d582570474c636514df9c7084ddb3ccc9cf`
  - `apps/desktop/src-tauri/Cargo.lock` (`434` packages): `fcf6f145ee4a1218f03ebdbdedef1d91cbd15a31bf2fd5ef4d71b7ce2446da05`
- **Compiled Unsigned Release Artifacts:**
  - `target/release/dim-cli.exe` — `2,210,406` bytes — SHA-256 `046e0f699ebf511ce50735706d7b14cc9fbf287a039773d4fa104f39294c2b23`
  - `target/release/dim-test-fixture.exe` — `1,616,934` bytes — SHA-256 `e97cd442511cb385dff56d09439dca397f8b3352cff6a2bdc3709c477e2aed88`
  - `apps/desktop/src-tauri/target/release/dim-desktop.exe` — `21,577,450` bytes — SHA-256 `56fe670d0d4f4cc99e931e3564c741489a51d91f6868dae56462ea55dc4978b6`
- **CycloneDX 1.5 SBOM & SPDX Audit:** [`sbom-cyclonedx.json`](../../evidence/sbom-cyclonedx.json) (`439` components) and [`dependency-audit.json`](../../evidence/dependency-audit.json) (`banned_crates_found: []`, `disallowed_sources: []`, `license_review_items: []`, `policy_check_passed: true`).

---

## 11. Installation & Smart App Control Forensics

Read-only registry & Windows Event Log inspection established:
- `HKLM:\SYSTEM\CurrentControlSet\Control\CI\Policy`: transitioned from initial `VerifiedAndReputablePolicyState = 1` (`SAC_PreviousState = 2`, `SAC_EnforcementReason = 1`) at `12:46Z` to `VerifiedAndReputablePolicyState = 0` (`SAC_PreviousState = 1`, `SAC_EnforcementReason = 6`) by `13:32Z`.
- `Microsoft-Windows-CodeIntegrity/Operational` log confirmed Event IDs `3118`/`3077`/`3033` at `19:54:53 +07:00` (`explorer.exe` blocked `ak-gui.exe`), followed by Event IDs `3101`/`3102` CI policy refresh at `19:57:26 +07:00` (turned off externally outside agent control).
- Consequently, unsigned PEs (`dim-desktop.exe`, `dim-cli.exe`) run on non-SAC hosts today, while **production distribution onto SAC=`1` Windows 11 endpoints remains blocked until signed with an Authenticode cert** — preserve the Signed-Host Bridge (`powershell.exe` + `node.exe`) for zero-install SAC compliance.

---

## 12. Real Zalo Desktop Verification Taxonomy (`L1` – `L5`)

| Level | Scope | Verdict | Evidence & Notes |
|---|---|---|---|
| **L1** | Process & Storage Isolation | **PASS** | `3` concurrent Zalo `26.10.20` windows + primary running; distinct PIDs, HWNDs, `lockfile`, `Local State`, `.zalo-profile-id` (`zalo-live-poc-result.json`). |
| **L2** | Lifecycle & Binary Integrity | **PASS** | Focus, live rename, selective shutdown (`3.04s`), relaunch (`4.14s`), crash relaunch (`4.24s`), `official_binary_sha256_unchanged: true`, primary `%APPDATA%\ZaloData` SHA-256 identical. |
| **L3** | Authenticated Multi-Account Usage | **NOT_TESTED** | Requires human QR sign-in across 2+ accounts; interactive checklist repaired and ready in [`scripts/run-interactive-l3-checklist.ps1`](../../scripts/run-interactive-l3-checklist.ps1). |
| **L4** | Notifications, Calls & Deep OS Integration | **BLOCKED** | Window focus works in L2; concurrent `ZaloCall.exe` voice/video (`PipeZCallRecv`/`PipeZCallSend`) and isolated toast routing (`com.vng.zalo` AUMID) remain VNG architectural limits. |
| **L5** | Installer & Auto-Update Resilience | **PARTIAL** | Binary SHA-256 invariance & `Zalo-*` semver dynamic lookup verified; MSI/NSIS installer & live `Update.exe` upgrade soak not run. |

---

## 13. Cross-Platform Readiness Matrix & Quality Gates A–F

| Platform / Gate | Status / Verdict | Evidence & Caveat |
|---|---|---|
| **Windows 10/11 (Core Isolation & Bridge + CLI)** | **SUPPORTED_AND_TESTED** | `29/29` Rust unit tests, `8/8` fixture tests, parity suite, `3x` live Zalo `26.10.20` PoC |
| **Windows 10/11 (Calls / Per-Profile Toasts / Unsigned PE Dist)** | **PARTIALLY_SUPPORTED / BLOCKED (Gate F)** | `PipeZCall*`, shared AUMID, unsigned PEs |
| **macOS (`crates/platform-macos`)** | **NOT_TESTED** | `3` Rust unit tests pass on Windows host; awaiting physical Mac validation & `/tmp/socketzalorecv2021` check |
| **Ubuntu / Linux (Official Zalo / Local AppImage)** | **UNSUPPORTED / EXPERIMENTAL (`NOT_TESTED`)** | Official Linux Zalo absent (`UnsupportedPlatform`); `3` `bwrap`/XDG unit tests pass |
| **Gate A (Feasibility)** | **PASS** | Verified without modifying VNG binaries |
| **Gate B (Data Safety)** | **PASS** | Primary SHA-256 unchanged, `PROTECTED_PRIMARY_PROCESS`, junction unlink verified |
| **Gate C (Functional Compatibility)** | **PARTIAL** | L1/L2 PASS; L3 `NOT_TESTED`; L4 `BLOCKED`; L5 `PARTIAL` |
| **Gate D (Cross-Platform Honesty)** | **PASS** | Strict honest status labels enforced |
| **Gate E (Security & Compliance)** | **PASS** | Dual Phase 10 findings remediated & re-verified; `policy_check_passed: true` (`439` packages) |
| **Gate F (Release Readiness)** | **BLOCKED (Unsigned PEs / Remote CI)** | Ready for local evaluation/SAC Signed-Host Bridge; blocked on Authenticode signing cert & `workflow` scope push |

---

## 14. Remaining Blockers & Prioritized Next Steps

1. **Unblock GitHub Actions CI Remote Push (P0):** Refresh CLI token scope (`gh auth refresh -s workflow`) or push from a credential with `workflow` permission so [`.github/workflows/ci.yml`](../../../.github/workflows/ci.yml) triggers `rust-workspace`, `cargo-deny` (both workspace + `src-tauri`), `node-and-windows-integration`, and `tauri-desktop-check` across Windows/macOS/Linux runners.
2. **Code-Sign Windows Release Binaries & Installer (P0 for Public Release / Gate F):** Sign `dim-desktop.exe`, `dim-cli.exe`, and NSIS/MSI installers with an OV/EV Authenticode certificate in release CI so Windows 11 Smart App Control (`1`) endpoints allow direct native launch without Signed-Host Bridge fallback.
3. **Complete Manual Multi-Account L3 Verification (P1):** Run `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run-interactive-l3-checklist.ps1` with two real phone QR logins to record L3/L4 evidence in `evidence/l3-interactive-checklist-result.json`.
4. **Hardware Validation on macOS & Ubuntu LTS (P2):** Verify `dim-platform-macos` (`CFFIXED_USER_HOME`, Keychain, `/tmp/socketzalorecv2021`) on Apple Silicon/Intel hardware and `dim-platform-linux` AppImage/`bwrap` isolation on Ubuntu 24.04.
