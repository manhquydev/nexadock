# Threat Model & Security Audit — Desktop Instance Manager

**Audit Methodology:** `ak-security` (STRIDE + OWASP Top 10 2021 + 11 Required Threat Categories + Secret/Pattern Scan + Dependency & Supply-Chain Audit + Independent Reviewer Hardening Pass + `modern-web-guidance` UI Security & Compliance Review)  
**Target Workspace:** [`desktop-instance-manager/`](../../)  
**Latest Hardening Audit Date (UTC):** `2026-11-17`  
**Gate E Verdict:** **PASS** (release code-signing for standalone PEs remains gated in **Gate F**)

---

## 1. Executive Summary & Scope

This document presents the threat model and security audit of `desktop-instance-manager`, updated after the Rust/Tauri compilation and independent review hardening pass:

- **Rust workspace crates:** [`crates/core/`](../../crates/core/), [`crates/profile-store/`](../../crates/profile-store/), [`crates/process-manager/`](../../crates/process-manager/), [`crates/platform-windows/`](../../crates/platform-windows/), [`crates/platform-macos/`](../../crates/platform-macos/), [`crates/platform-linux/`](../../crates/platform-linux/), plus [`Cargo.toml`](../../Cargo.toml), [`deny.toml`](../../deny.toml), and root [`.github/workflows/ci.yml`](../../../.github/workflows/ci.yml).
- **Desktop shell & host bridge:** [`apps/desktop/src-tauri/`](../../apps/desktop/src-tauri/), [`apps/desktop/ui-server.mjs`](../../apps/desktop/ui-server.mjs), [`apps/desktop/ui/`](../../apps/desktop/ui/), and [`scripts/dim-host.ps1`](../../scripts/dim-host.ps1).
- **Verification harnesses:** [`tests/run-fixture-tests.mjs`](../../tests/run-fixture-tests.mjs), [`tests/run-contract-parity-tests.mjs`](../../tests/run-contract-parity-tests.mjs), [`scripts/run-zalo-poc.ps1`](../../scripts/run-zalo-poc.ps1), and [`scripts/run-interactive-l3-checklist.ps1`](../../scripts/run-interactive-l3-checklist.ps1).

### Trust Boundaries
1. **Primary User Account Boundary (`%APPDATA%\ZaloData` & Primary `Zalo.exe` PIDs):** Strictly read-only inspection; never modified, locked, restored over, or terminated (`PROTECTED_PRIMARY_PROCESS`, `PROTECTED_PRIMARY_PATH` including descendants).
2. **Isolated Profile Boundary (`Profiles\<slug>_<id>\`):** Strict filesystem & environment boundary per secondary profile with `0o700` UNIX / restrictive NTFS ACLs and `.zalo-profile-id` ownership verification.
3. **Local UI HTTP Boundary (`127.0.0.1:<ephemeral>`):** Loopback-only management plane protected against DNS rebinding (`Host`), CSRF (`Origin`, `Sec-Fetch-Site`), cross-process unauthorized calls (`X-DIM-Session-Token`), form-based `Content-Type` attacks, and external backup writes (`403 FORBIDDEN_DEST`).
4. **External Process Boundary (`Zalo.exe` / Custom Binary / Fixture):** Launched non-elevated (`asInvoker`, `bInheritHandles = 0`, `CREATE_NO_WINDOW`) with cryptographic Authenticode trust (`WinVerifyTrust` + `CN/O=VNG GROUP JSC` RDN pinning for Official Zalo) enforced across both Rust and PowerShell.

---

## 2. Host Baseline & Official Binary SHA-256 Inventory

### Host Environment & Hardening Summary
- **Elevation:** `IsElevated = false` (`asInvoker`, queried dynamically via Win32 `OpenProcessToken` / `TokenElevation`).
- **Toolchain & Runtimes:** `Node.js v26.13.1`, `Git 2.56.1.windows.1`, `rustc 1.100.0` (`stable-x86_64-pc-windows-gnu`), WinLibs MinGW-w64 `16.2.0` (`BrechtSanders.WinLibs.POSIX.UCRT`).
- **Windows Smart App Control (SAC):** Observed `VerifiedAndReputablePolicyState = 1` at initial baseline, transitioning externally to `0` (`SAC_PreviousState: 1`, `SAC_EnforcementReason: 6`; CodeIntegrity event log records `3118`/`3077`/`3033` before `3101`/`3102` refresh). Local release `.exe` binaries (`dim-desktop.exe`, `dim-cli.exe`, `dim-test-fixture.exe`) remain **unsigned** (`IMAGE_DIRECTORY_ENTRY_SECURITY` absent) — public distribution onto SAC-enforcing endpoints stays blocked under **Gate F**.
- **Static Pattern & Secret Hygiene:** Zero `.env*` secrets, zero `innerHTML` / `eval` / `Invoke-Expression` sinks, zero `unwrap`/`expect` panics in `crates/*` production code (`apps/desktop/src-tauri/src/main.rs` contains only standard startup Tauri window creation `.expect()`), and zero personal OS usernames in source code or `evidence/*.json`.

### Official Zalo Desktop Binaries & Installer Hashes
All official binaries carry Authenticode signature `CN=VNG GROUP JSC, O=VNG GROUP JSC, S=Ho Chi Minh City, C=VN, SERIALNUMBER=0303490096`:

| Artifact | Size (bytes) | SHA-256 | Source |
|---|---:|---|---|
| `%LOCALAPPDATA%\Programs\Zalo\Zalo.exe` (stub) | 790,568 | `7295232c89f9a041e35253afa80787989eae9a91895f1847ebd92c78bb4195a2` | Host baseline |
| `%LOCALAPPDATA%\Programs\Zalo\Zalo-26.10.20\Zalo.exe` | 136,638,504 | `75dc85559079ec2037d511a312b704b44ca3293e74da1900ccfe84e6299cac3e` | Host baseline + [`zalo-live-poc-result.json`](../../evidence/zalo-live-poc-result.json) |
| `%LOCALAPPDATA%\Programs\Zalo\Zalo-26.10.20\resources\app.asar` | 177,728,582 | `d83256230e5d1d4483c2411f71936c3dc1562fb8a885fd8752cab17d229201e6` | Host baseline + [`zalo-live-poc-result.json`](../../evidence/zalo-live-poc-result.json) |
| `%LOCALAPPDATA%\Programs\Zalo\Zalo-26.10.20\Update.exe` | 833,576 | `b8a83bdceb1cf136d44f7a3d00ab7d0cffd2c49b17a3a7babda3fc12579da6b7` | Host baseline |
| `%LOCALAPPDATA%\Programs\Zalo\Zalo-26.10.20\plugins\capture\ZaloCall.exe` | 19,588,136 | `fbe5594b6d5c8c9c9213ed0621b80e8e89c55329c7de04771d1ccff86171b2f9` | Host baseline |
| `%LOCALAPPDATA%\Programs\Zalo\Zalo-26.10.20\plugins\capture\ZaloCap.exe` | 5,488,240 | `ab2301a00d7eb79ea9a2440c5ff144a13b068932277b38dced9b4cfecb35f1a6` | Host baseline |
| `ZaloSetup-26.11.12.exe` (archived installer evidence) | 213,516,168 | `dddef07c11683561c6abe19c07bbc6202214136325d6b4b39311587a16a15eb8` | Historical REA static verification *(raw JSON removed per `PUB-SEC-10`)* |

---

## 3. STRIDE Threat Model Matrix

| Category | Threat Vector | Implemented Mitigation | Evidence / Anchor |
|---|---|---|---|
| **S — Spoofing** | Rogue executable planted in `Zalo-*`, wrong-publisher binary matching loose substring, or cross-origin localhost call | `WinVerifyTrust` (`WINTRUST_ACTION_GENERIC_VERIFY_V2`) + exact `CN`/`O=VNG GROUP JSC` X.500 RDN pinning (`win32_ffi.rs` & `dim-host.ps1`); `.zalo-profile-id` verification; `Host` + `Origin` + `Sec-Fetch-Site` + `X-DIM-Session-Token` timing-safe check | [`crates/platform-windows/src/win32_ffi.rs`](../../crates/platform-windows/src/win32_ffi.rs), [`scripts/dim-host.ps1`](../../scripts/dim-host.ps1), [`evidence/contract-parity-result.json`](../../evidence/contract-parity-result.json) |
| **T — Tampering** | Backup tampering, partial restore corruption, symlink/junction escape | SHA-256 + byte-size verification per payload file; atomic `.partial` -> `backup.json` write; `.restore-*` staging dir swap + rollback; `IsReparsePoint` & `FILE_ATTRIBUTE_DIRECTORY` junction unlink (`remove_dir` on Windows junctions); deepest-ancestor `canonicalize` (`path_guard.rs`) | [`crates/profile-store/src/backup.rs`](../../crates/profile-store/src/backup.rs), [`crates/profile-store/src/store.rs`](../../crates/profile-store/src/store.rs), [`crates/core/src/path_guard.rs`](../../crates/core/src/path_guard.rs) |
| **R — Repudiation** | Unattributed stop/delete actions or silent check masking | Structured sanitized JSON logs, explicit error codes (`PROTECTED_PRIMARY_PROCESS`, `PROTECTED_PRIMARY_PATH`, `DUPLICATE_ZALO_USER_ID`, `UNVERIFIED_BINARY`, `CONFIRM_REQUIRED`), honest `PASS`/`FAIL`/`NOT_TESTED`/`BLOCKED` checks | [`crates/core/src/error.rs`](../../crates/core/src/error.rs), [`crates/process-manager/src/coordinator.rs`](../../crates/process-manager/src/coordinator.rs) |
| **I — Information Disclosure** | Logs/IPC errors leaking home paths, usernames, tokens, or Folder Redirection UNC paths | `redact_sensitive_text` (case-insensitive user home + username + tokens/JSON secrets) applied in `dim-cli.rs`, `commands.rs`, `ui-server.mjs`, `dim-host.ps1`; restrictive profile ACLs (`0o700` UNIX / disabling `Everyone` & `BUILTIN\Users` write) | [`crates/core/src/sanitize.rs`](../../crates/core/src/sanitize.rs), [`apps/desktop/src-tauri/src/commands.rs`](../../apps/desktop/src-tauri/src/commands.rs), [`apps/desktop/ui-server.mjs`](../../apps/desktop/ui-server.mjs) |
| **D — Denial of Service** | Stale lockfiles bricking relaunch, disk exhaustion, recycled PID spoofing, HTTP floods, accidental kill of primary Zalo | Stale `lockfile` cleanup + PID exe-name verification (`adapter.rs`), `5 GiB + payloadEstimate` disk reserve check, `12h` staging cleanup, `ui-server.mjs` rate limiter (`429`) + 30s timeout (`504`) + `enqueueMutation`, `AppState::op_lock`, `PrimaryProtectionGuard` | [`crates/process-manager/src/guard.rs`](../../crates/process-manager/src/guard.rs), [`apps/desktop/ui-server.mjs`](../../apps/desktop/ui-server.mjs) |
| **E — Elevation of Privilege** | UAC prompts, HKLM writes, handle inheritance, shell/option injection | Strictly `asInvoker` (live `TokenElevation`), `b_inherit_handles = 0` + `CREATE_NO_WINDOW`, `UseShellExecute = false`, `QuoteArg` backslash-doubling, `--` option sentinel | [`crates/platform-windows/src/win32_ffi.rs`](../../crates/platform-windows/src/win32_ffi.rs), [`apps/desktop/ui-server.mjs`](../../apps/desktop/ui-server.mjs) |

---

## 4. OWASP Top 10 (2021) & 11-Category Deep Audit Summary

| Category | Status | Hardened Controls |
|---|---|---|
| **1. Data Exfiltration** | **Mitigated** | Zero outbound network requests across all crates, UI, and PowerShell scripts; loopback HTTP (`127.0.0.1`) only. |
| **2. Unauthorized Filesystem Access** | **Mitigated** | NTFS DACL checks rejecting `Everyone` / `BUILTIN\Users` write (`win32_ffi.rs` + `dim-host.ps1`); UNIX `0o700`; Folder Redirection guard. |
| **3. Cross-Profile Leakage** | **Mitigated** | Per-profile `APPDATA`/`LOCALAPPDATA`/`USERPROFILE`/`TEMP`/`TMP`; `DetectZaloUserId` reading `config.json` `db_path.location` + newest mtime in both Rust and PowerShell; `.zalo-profile-id` & `DUPLICATE_ZALO_USER_ID` (including Primary Zalo); shared pipe/AUMID documented honestly as `BLOCKED`. |
| **4. Privilege Escalation** | **Mitigated** | Strictly `asInvoker`, zero drivers/services/HKLM writes; live OS `TokenElevation` probe in both Rust and PowerShell. |
| **5. Unsafe Command Execution** | **Mitigated** | Direct `CreateProcessW` / `execFile`, `QuoteArg`, `--` sentinel, parameterized AppleScript `on run argv` / Linux `bwrap`. |
| **6. Supply-Chain Security** | **Mitigated** | Locked `Cargo.lock` + `apps/desktop/src-tauri/Cargo.lock`; `deny.toml` (`yanked = "deny"`, `wildcards = "deny"`, `openssl` banned); `generate-build-provenance.mjs` audits BOTH lockfiles against SPDX allowlist; zero 3P npm/CDN deps. |
| **7. Log & Error Leakage** | **Mitigated** | `redact_sensitive_text` & `sanitizeHostError` sanitize CLI/IPC errors, `folder_redirection.warning`, and diagnostic logs. |
| **8. Binary & Update Integrity** | **Mitigated** | Rust `adapter.rs` & PowerShell `dim-host.ps1` enforce `WinVerifyTrust` + `CN/O=VNG GROUP JSC` RDN pinning across `official-zalo` and reject unsigned/tampered `custom-exe`/`node-fixture`; tampered PE triggers `UNVERIFIED_BINARY` (`WinVerifyTrust=0xC000A000`). |
| **9. Destructive Cleanup** | **Mitigated** | Backend `--confirm` / `confirm=true` enforcement, `op_lock` / `enqueueMutation` serialization, `PrimaryProtectionGuard`, `IsReparsePoint` + `FILE_ATTRIBUTE_DIRECTORY` junction/file-symlink unlinking before deletion (`delete_profile_unlinks_junctions_or_symlinks_without_touching_external_target` PASS). |
| **10. Incorrect Restore** | **Mitigated** | Atomic `.partial` -> `backup.json` (`schema_version == 1`), `5 GiB` disk check, SHA-256 verification on staging copy, restore strictly into new profile, Tauri `restore_backup` confined under `backups_dir`. |
| **11. Malicious Input Paths** | **Mitigated** | Deepest-ancestor canonicalization (`path_guard.rs`), trailing-separator primary-Zalo containment (`HostEngine`, `BackupProfile`, `RestoreBackup`, `run-zalo-poc.ps1`), profile name validation on restore `--name`. |

---

## 5. Rust Safety, Tauri Bridge & `modern-web-guidance` UI Verification

- **Workspace & Tauri Lints:** `unsafe_op_in_unsafe_fn = "deny"` cleanly enforced across all workspace crates; `src-tauri` MSRV updated accurately to `1.90` for `tauri 2.12.6`.
- **`unsafe` Scope:** Bounded exclusively to [`win32_ffi.rs`](../../crates/platform-windows/src/win32_ffi.rs) (`WinVerifyTrust`, CryptoAPI `CryptQueryObject`/`CryptMsgGetParam`/`CertFindCertificateInStore`/`CertGetNameStringW` with valid `CERT_X500_NAME_STR | CERT_NAME_STR_REVERSE_FLAG` `DWORD`, `OpenProcessToken`, `GetNamedSecurityInfoW`, `RmStartSession`, `CreateProcessW`, `CreateToolhelp32Snapshot`, `EnumWindows`, `RegOpenKeyExW`) wrapped in RAII Drop guards, plus bounded `libc::kill`/`waitpid` on macOS/Linux.
- **Tauri Command & Bridge Security:** `AppState::op_lock` serializes mutating commands; `ui-server.mjs` enforces per-session `X-DIM-Session-Token`, `Content-Security-Policy` (`object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'`), `X-Content-Type-Options: nosniff`, `Cross-Origin-Resource-Policy: same-origin`, `Cross-Origin-Opener-Policy: same-origin`, rate limiting, timeouts, and error redaction.
- **Web UI Accessibility & Dialogs:** `index.html` declares `role="tablist"` + `aria-controls` alongside `focusgroup` and `hidden="until-found"`; native `<dialog>` elements reset `returnValue = ''`, place `autofocus` safely on Cancel in `#delete-profile-dialog`, honor `state.busy` guards, and contain zero `innerHTML` sinks.

---

## 6. Gate E — Security & Compliance Verdict

- **Verdict:** **PASS**
- **Release Caveat (Gate F):** Locally compiled `.exe` binaries (`dim-desktop.exe`, `dim-cli.exe`, `dim-test-fixture.exe`) are unsigned test builds (`IMAGE_DIRECTORY_ENTRY_SECURITY` absent) and remain **BLOCKED** for public Windows Smart App Control distribution until signed with an Authenticode certificate in CI. Full post-hardening verification details are recorded in [`docs/reviews/post-swarm-hardening-report.md`](../reviews/post-swarm-hardening-report.md).
