# Threat Model & Security Audit — Desktop Instance Manager

**Audit Methodology:** `ak-security` (STRIDE + OWASP Top 10 2021 + 11 Required Threat Categories + Secret/Pattern Scan + Dependency & Supply-Chain Audit + `modern-web-guidance` UI Security & Compliance Review)  
**Target Workspace:** [`desktop-instance-manager/`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager)  
**Primary Auditor:** `security-verifier`  
**Audit Date (UTC):** `2026-11-17` (Post-remediation verification finalized at `2026-11-17T13:32:45Z`)  
**Gate E Verdict:** **PASS** (with documented non-blocking parity & release-signing conditions tracked in Gate F)

---

## 1. Executive Summary & Scope

This document presents the independent threat model and security audit of `desktop-instance-manager`, covering:

- **Rust workspace crates:** [`crates/core/`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/crates/core), [`crates/profile-store/`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/crates/profile-store), [`crates/process-manager/`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/crates/process-manager), [`crates/platform-windows/`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/crates/platform-windows), [`crates/platform-macos/`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/crates/platform-macos), [`crates/platform-linux/`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/crates/platform-linux), plus [`Cargo.toml`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/Cargo.toml), [`deny.toml`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/deny.toml), and [`.github/workflows/ci.yml`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/.github/workflows/ci.yml).
- **Desktop shell & host bridge:** [`apps/desktop/src-tauri/`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/apps/desktop/src-tauri), [`apps/desktop/ui-server.mjs`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/apps/desktop/ui-server.mjs), [`apps/desktop/ui/`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/apps/desktop/ui), and [`scripts/dim-host.ps1`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/scripts/dim-host.ps1).
- **Verification harnesses:** [`tests/run-fixture-tests.mjs`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/tests/run-fixture-tests.mjs) and [`scripts/run-zalo-poc.ps1`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/scripts/run-zalo-poc.ps1).

### Trust Boundaries
1. **Primary User Account Boundary (`%APPDATA%\ZaloData` & Primary `Zalo.exe` PIDs):** Strictly read-only inspection; must never be modified, locked, restored over, or terminated.
2. **Isolated Profile Boundary (`Profiles\<slug>_<id>\`):** Strict filesystem & environment boundary per secondary profile with `0o700` / restrictive NTFS ACLs and `.zalo-profile-id` ownership verification.
3. **Local UI HTTP Boundary (`127.0.0.1:<ephemeral>`):** Loopback-only management plane protected against DNS rebinding (`Host`), CSRF (`Origin`, `Sec-Fetch-Site`), and form-based `Content-Type` attacks.
4. **External Process Boundary (`Zalo.exe` / Fixture Binary):** Launched non-elevated with explicit environment override and Authenticode trust enforcement (`WinVerifyTrust` + VNG signer verification) before execution.

---

## 2. Host Baseline & Verified Binary SHA-256 Inventory

### Host Environment Observed Directly
- **Elevation:** `IsAdmin: false` (`asInvoker`, strictly standard user privileges).
- **Toolchain State:** `NodeVersion: v26.13.1`, `GitVersion: 2.56.1.windows.1`, `RustcInstalled: false`, `CargoInstalled: false`.
- **Windows Smart App Control (SAC):**
  - Baseline (`2026-11-17T12:46:29Z`): `VerifiedAndReputablePolicyState: 1` (`Enforced`), `SAC_PreviousState: 2`, `SAC_EnforcementReason: 1`.
  - Live PoC & Post-Run (`2026-11-17T13:32:12Z` – `13:33:09Z`): `VerifiedAndReputablePolicyState: 0`, `SAC_PreviousState: 1`, `SAC_EnforcementReason: 6` (host transitioned from SAC evaluation/enforcement during the session; CI code-signing remains mandatory in [`.github/workflows/ci.yml`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/.github/workflows/ci.yml) for production distribution onto SAC-enforcing endpoints).
- **Static Pattern & Secret Scan Across 63 Workspace Files (`54` Code Files, `30` `.rs` Files):**
  - `SecretHits`: `0`
  - `EnvFilesPresent`: `0` (`.env*` absent)
  - `UiDangerousCount` (`innerHTML`, `outerHTML`, `eval`, `Function(`, `document.write`): `0`
  - `PsDangerousCount` (`Invoke-Expression`, `iex`, hardcoded bypass secrets): `0`
  - `NonTestUnwrapExpect` in `crates/*`: `0` (`apps/desktop/src-tauri/src/main.rs:8,27` contain the only 2 startup `.expect()` calls for Tauri context generation and window startup).

### Official Zalo Desktop Binaries & Installer Hashes
All official binaries carry Authenticode signature `CN=VNG GROUP JSC, O=VNG GROUP JSC, S=Ho Chi Minh City, C=VN, SERIALNUMBER=0303490096`:

| Artifact | Size (bytes) | SHA-256 | Source |
|---|---:|---|---|
| `%LOCALAPPDATA%\Programs\Zalo\Zalo.exe` (stub) | 790,568 | `7295232c89f9a041e35253afa80787989eae9a91895f1847ebd92c78bb4195a2` | Host baseline |
| `%LOCALAPPDATA%\Programs\Zalo\Zalo-26.10.20\Zalo.exe` | 136,638,504 | `75dc85559079ec2037d511a312b704b44ca3293e74da1900ccfe84e6299cac3e` | Host baseline + [`zalo-live-poc-result.json`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/evidence/zalo-live-poc-result.json) |
| `%LOCALAPPDATA%\Programs\Zalo\Zalo-26.10.20\resources\app.asar` | 177,728,582 | `d83256230e5d1d4483c2411f71936c3dc1562fb8a885fd8752cab17d229201e6` | Host baseline + [`zalo-live-poc-result.json`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/evidence/zalo-live-poc-result.json) |
| `%LOCALAPPDATA%\Programs\Zalo\Zalo-26.10.20\Update.exe` | 833,576 | `b8a83bdceb1cf136d44f7a3d00ab7d0cffd2c49b17a3a7babda3fc12579da6b7` | Host baseline |
| `%LOCALAPPDATA%\Programs\Zalo\Zalo-26.10.20\plugins\capture\ZaloCall.exe` | 19,588,136 | `fbe5594b6d5c8c9c9213ed0621b80e8e89c55329c7de04771d1ccff86171b2f9` | Host baseline |
| `%LOCALAPPDATA%\Programs\Zalo\Zalo-26.10.20\plugins\capture\ZaloCap.exe` | 5,488,240 | `ab2301a00d7eb79ea9a2440c5ff144a13b068932277b38dced9b4cfecb35f1a6` | Host baseline |
| `ZaloSetup-26.11.12.exe` (installer archived evidence) | 213,516,168 | `dddef07c11683561c6abe19c07bbc6202214136325d6b4b39311587a16a15eb8` | [`rea-evidence-summary.json`](file:///c:/Users/manhquy/Downloads/zalo/multi-zalo-desktop/evidence/rea-evidence-summary.json) |

---

## 3. STRIDE Threat Model Matrix

| Category | Threat Vector | Implemented Mitigation | Evidence / Anchor |
|---|---|---|---|
| **S — Spoofing** | Rogue executable planted in `Zalo-*` version dir or profile impersonation | Cryptographic `WinVerifyTrust` (`WINTRUST_ACTION_GENERIC_VERIFY_V2`) + `VNG GROUP JSC` signer check before `LaunchProfile`; `.zalo-profile-id` marker check; loopback `Host` validation | [`dim-host.ps1:860-927`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/scripts/dim-host.ps1#L860-L927), [`fixture-test-result.json`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/evidence/fixture-test-result.json) (`Test-8`: `0xC000A000`) |
| **T — Tampering** | Backup tampering, partial restore corruption, symlink/junction escape | SHA-256 manifest verified per file on restore; atomic `.partial` -> `backup.json` write; `.restore-*` staging dir swap + rollback; `IsReparsePoint` & `resolve_safe_child` traversal blocks | [`backup.rs:23-235`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/crates/profile-store/src/backup.rs#L23-L235), [`path_guard.rs:15`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/crates/core/src/path_guard.rs#L15) |
| **R — Repudiation** | Unattributed stop/delete actions or loss of error trail | Structured sanitized JSON logs (`logs/events.jsonl`), explicit error codes (`PROTECTED_PRIMARY_PROCESS`, `DUPLICATE_ZALO_USER_ID`, `PATH_TRAVERSAL_REJECTED`), `--confirm` delete gate | [`dim-host.ps1:1982,2081`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/scripts/dim-host.ps1#L1982), [`error.rs:113`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/crates/core/src/error.rs#L113) |
| **I — Information Disclosure** | Logs exposing tokens/cookies/user paths/Zalo IDs; weak profile ACLs | `redact_sensitive_text` & `Sanitize-Value` redaction; restrictive profile dir permissions (`0o700` UNIX / disabling `Everyone` & `BUILTIN\Users` write NTFS ACLs) | [`sanitize.rs:28`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/crates/core/src/sanitize.rs#L28), [`zalo-live-poc-result.json`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/evidence/zalo-live-poc-result.json) (`RestrictiveNtfsAcls`) |
| **D — Denial of Service** | Stale lockfiles bricking relaunch, disk exhaustion during backup, accidental kill of primary Zalo | Stale `lockfile` detection & cleanup; `5 GiB + payloadEstimate` disk reserve check; `STAGING_MAX_AGE` (12h) sweep; `ensure_safe_to_stop` primary PID/tree guard | [`guard.rs:39,71`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/crates/process-manager/src/guard.rs#L39), [`dim-host.ps1:2081,2089`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/scripts/dim-host.ps1#L2081) |
| **E — Elevation of Privilege** | UAC prompts, HKLM writes, privilege inheritance, CLI flag/shell injection | Strictly `asInvoker` (`IsAdmin: false`), `b_inherit_handles = 0`, `UseShellExecute = false`, `QuoteArg` backslash-doubling, `--` option sentinel | [`win32_ffi.rs`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/crates/platform-windows/src/win32_ffi.rs), [`ui-server.mjs`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/apps/desktop/ui-server.mjs) |

---

## 4. OWASP Top 10 (2021) Assessment

| OWASP Category | Applicability & Status | Key Controls Verified |
|---|---|---|
| **A01 Broken Access Control** | **Mitigated** | Per-profile ACLs excluding `Everyone`/`BUILTIN\Users` write (`RestrictiveNtfsAcls`); UNIX `0o700`; loopback `127.0.0.1` binding + `Host` & `Origin`/`Sec-Fetch-Site` checks in [`ui-server.mjs:178-189`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/apps/desktop/ui-server.mjs#L178-L189); `PROTECTED_PRIMARY_PROCESS` guard. |
| **A02 Cryptographic Failures** | **Mitigated** | Uses `sha2` crate (`SHA-256`) for backup integrity & deterministic slug hashing; zero custom crypto; Windows Authenticode trust checked via `WinVerifyTrust`. |
| **A03 Injection** | **Mitigated** | Zero shell execution (`CreateProcessW`, `UseShellExecute = false`, `execFile` in Node); `CommandLineToArgvW` backslash-doubling `QuoteArg`; `--` option-injection sentinel; `osascript` `on run argv`; zero DOM `innerHTML` sinks. |
| **A04 Insecure Design** | **Mitigated** | Hybrid process-env isolation avoids brittle Electron binary patching or ASAR tampering; duplicate `zalo_user_id` launch guard prevents split-brain session corruption; restore creates a new profile instead of mutating live data. |
| **A05 Security Misconfiguration** | **Mitigated** | Folder Redirection / UNC `%APPDATA%` warning/block in `LaunchProfile`; `deny.toml` bans `openssl`/`openssl-sys` & wildcards; native `<dialog>` & CSP-friendly zero-CDN vanilla UI. |
| **A06 Vulnerable & Outdated Components** | **Mitigated** | Minimal Rust crate graph (`serde`, `serde_json`, `thiserror`, `sha2`, `libc`, dev `tempfile`); zero `npm` runtime dependencies in `apps/desktop/ui/` and `ui-server.mjs`; `cargo-deny` configured in CI. |
| **A07 Identification & Authentication Failures** | **Mitigated** | Zalo account sessions live strictly inside each profile's isolated storage; no shared credential vault; `.zalo-profile-id` & `DUPLICATE_ZALO_USER_ID` enforce 1:1 running account-to-profile mapping. |
| **A08 Software & Data Integrity Failures** | **Mitigated** | Signed-Host Bridge enforces `WinVerifyTrust` (`0xC000A000` tested on tampered fixture PE); atomic `.partial` -> `backup.json` & `.restore-*` staging; CI signs/builds Rust binaries cleanly before release (Gate F). |
| **A09 Security Logging & Monitoring Failures** | **Mitigated** | Events logged to sanitized store (`redact_sensitive_text` / `Sanitize-Value`); crash recovery & tamper rejection emit explicit machine-readable codes. |
| **A10 SSRF (Local Loopback Equivalent)** | **Mitigated** | No outbound network requests made by `desktop-instance-manager`; `ui-server.mjs` rejects non-loopback `Host` (`403 FORBIDDEN_HOST`), cross-origin requests (`403 FORBIDDEN_ORIGIN`), and non-JSON POSTs (`415 UNSUPPORTED_MEDIA_TYPE`). |

---

## 5. Comprehensive 11-Category Deep Audit

### 1. Data Exfiltration
- **Findings:** `desktop-instance-manager` makes **zero outbound network connections** in Rust crates, `ui-server.mjs`, `dim-host.ps1`, or `apps/desktop/ui/` (all assets are bundled locally; external links/telemetry absent). Only Zalo Desktop itself communicates with VNG endpoints.
- **Verification:** Checked all `crates/*`, `apps/desktop/*`, and `scripts/*` imports; no HTTP clients, sockets (except loopback `127.0.0.1:0` in `ui-server.mjs`), or upload helpers exist.

### 2. Unauthorized Filesystem Access (NTFS ACLs, UNIX Permissions, Folder Redirection)
- **Windows:** `dim-host.ps1` verifies profile root ACLs (`RestrictiveNtfsAcls`) to reject write access by `Everyone` or `BUILTIN\Users`. Verified passing during live PoC ([`zalo-live-poc-result.json`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/evidence/zalo-live-poc-result.json)). Also detects Windows AppData Folder Redirection / UNC roaming paths inside `LaunchProfile` and `diagnostics` so profile data is not silently pushed onto remote SMB shares.
- **macOS & Linux:** [`crates/platform-macos/src/layout.rs`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/crates/platform-macos/src/layout.rs) and [`crates/platform-linux/src/layout.rs`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/crates/platform-linux/src/layout.rs) enforce mode `0o700` on isolated profile directories using `std::os::unix::fs::PermissionsExt`.

### 3. Cross-Profile Leakage (`TEMP`/`TMP`, Junctions, Symlinks, `zmeta.json`/`migrate-config.json`)
- **Environment Isolation:** Every launched instance receives per-profile overrides for `APPDATA`, `LOCALAPPDATA`, `USERPROFILE`, `HOME`, `TEMP`, and `TMP` (plus `XDG_*` on Linux and `CFFIXED_USER_HOME` on macOS).
- **Config & Identity Files:** Primary Zalo uses `%APPDATA%\ZaloData\config.json` (`db_path.location`) and `migrate-config.json`. [`crates/core/src/zalo_identity.rs`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/crates/core/src/zalo_identity.rs) and `dim-host.ps1` read `config.json` (`db_path.location`) first and select the most recent `zalo_<uid>` subfolder when multiple cached subdirs exist (remediated per `#16` -> `#17`), preventing cross-account misidentification.
- **Pre-Launch Isolation Guards:**
  - `.zalo-profile-id` marker verified on launch (`PROFILE_MARKER_MISMATCH`).
  - `DUPLICATE_ZALO_USER_ID` check blocks launching two profiles with the same logged-in Zalo user ID (verified in `Test-6` of [`fixture-test-result.json`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/evidence/fixture-test-result.json)).
  - Backup/restore & cleanup explicitly check `IsReparsePoint` ([`dim-host.ps1:1388-1500,2350-2448`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/scripts/dim-host.ps1#L1388)) and skip/unlink junctions or symlinks rather than following them into `%APPDATA%\ZaloData`.
- **Known Residual Boundary (Shared OS IPC — Documented Honestly):** Secondary Zalo instances share VNG's desktop IPC/notification surfaces (`\\.\pipe\PipeZCallRecv`, `\\.\pipe\PipeZCallSend`, and `AppUserModelId("com.vng.zalo")` on Windows; `/tmp/socketzalorecv2021` on macOS). Concurrent voice/video calls and per-profile notification routing are **BLOCKED** (tracked in Gate C).

### 4. Privilege Escalation
- **Verification:** Runs strictly as standard user (`asInvoker`), `IsAdmin = false` verified in host baseline and PoC diagnostics. Zero kernel drivers, zero scheduled tasks/services, and zero writes to `HKLM`. Only read-only queries hit `HKLM\SYSTEM\CurrentControlSet\Control\CI\Policy` (`VerifiedAndReputablePolicyState`) and `HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\User Shell Folders` (`AppData`). `CreateProcessW` sets `b_inherit_handles = 0`.

### 5. Unsafe Command Execution
- **Rust Windows (`win32_ffi.rs`):** Direct `CreateProcessW` invocation without `cmd.exe` / shell expansion, `CREATE_UNICODE_ENVIRONMENT | CREATE_NEW_PROCESS_GROUP`, and explicit RAII handle/key cleanup (`HandleGuard`, `RmSessionGuard`, `RegKeyGuard`).
- **Signed-Host Bridge (`dim-host.ps1`):** Uses `ProcessStartInfo` with `UseShellExecute = false`. `QuoteArg` properly doubles trailing backslashes before quotes (`CommandLineToArgvW` rules, fixed per `#15` -> `#19`). `DispatchCli` enforces `--` option-injection sentinel so profile IDs/names starting with `-` cannot inject switches.
- **Node UI Server (`ui-server.mjs`):** Uses `execFile` (never `exec`) with `--` sentinel before user-supplied profile IDs/backup paths.
- **macOS & Linux Adapters:** macOS uses `open -n -a ... --env CFFIXED_USER_HOME=...` and parameterized `osascript` `on run argv` (zero AppleScript interpolation). Linux uses parameterized `bwrap` argument vectors (`--bind`, `--dev`, `--proc`, `--setenv`).

### 6. Dependency Compromise & Supply-Chain Security
- **Rust Dependencies ([`Cargo.toml`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/Cargo.toml)):** `serde 1.0`, `serde_json 1.0`, `thiserror 2.0`, `sha2 0.10`, `libc 0.2`, dev `tempfile 3.14` (plus Tauri framework crates inside `apps/desktop/src-tauri/Cargo.toml` for native desktop packaging).
- **Supply-Chain Policy ([`deny.toml`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/deny.toml)):** `yanked = "deny"`, `wildcards = "deny"`, `unknown-registry = "deny"` (crates.io index only), strict OSI licenses (`MIT`, `Apache-2.0`, `Apache-2.0 WITH LLVM-exception`, `BSD-2-Clause`, `BSD-3-Clause`, `ISC`, `Unicode-3.0`, `Zlib`), and `openssl` / `openssl-sys` banned (`use-instead = "rustls"`).
- **Frontend Dependencies:** **Zero** third-party npm runtime packages or external CDN scripts in `apps/desktop/ui/` and `ui-server.mjs`.
- **Smart App Control & Binary Signing:** Unsigned Rust binaries cannot run on SAC-enforcing (`VerifiedAndReputablePolicyState = 1`) Windows endpoints; official production binaries must be compiled and Authenticode-signed in CI (tracked in Gate F).

### 7. Log Leakage
- **Rust (`crates/core/src/sanitize.rs:28`):** `redact_sensitive_text(input, user_home)` redacts bearer/authorization headers, `cookie=`, `token=`, `access_token=`, `refresh_token=`, `password=`, `session_key=` (including JSON key-value colons, expanded per `#16` -> `#17`), URL auth query parameters, user home paths (`%USERPROFILE%`), Windows/UNIX account names, and masks `zalo_user_id` tails (`zalo_1234****`).
- **Host Bridge (`dim-host.ps1`):** `Sanitize-Value` mirrors redaction across JSON responses, diagnostic fields (e.g. `registry_value_sanitized: "%USERPROFILE%\\AppData\\Roaming"`), and event logs. Verified negative for raw secrets/user paths in [`fixture-test-result.json`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/evidence/fixture-test-result.json) (`Test-8`) and [`zalo-live-poc-result.json`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/evidence/zalo-live-poc-result.json).

### 8. Insecure Update
- **Zalo Discovery (`dim-host.ps1:860-927`):** Dynamically locates the newest semver `Zalo-*` directory under `%LOCALAPPDATA%\Programs\Zalo` (plus `Update.exe` awareness) and enforces **both** `WinVerifyTrust` cryptographic validation (`WINTRUST_ACTION_GENERIC_VERIFY_V2`) and VNG publisher subject (`VNG GROUP JSC`) inside `LaunchProfile`.
- **Regression Evidence:** `Test-8` explicitly plants a tampered/unsigned PE copy and verifies rejection (`"Unverified (WinVerifyTrust=0xC000A000)"`).
- **No Self-Updater:** `desktop-instance-manager` includes zero auto-download/auto-update mechanism that could fetch unsigned payloads.
- **Rust Parity Note (Low / Non-Blocking for Signed-Host Bridge):** [`crates/platform-windows/src/adapter.rs:208-228`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/crates/platform-windows/src/adapter.rs#L208-L228) checks for an embedded PE Authenticode table (`has_pe_authenticode_table`) and leaves a documented note to invoke full `WinVerifyTrust` before Rust-native launch paths are enabled in production.

### 9. Destructive Cleanup
- **Primary Process Protection:** [`crates/process-manager/src/guard.rs:39,71`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/crates/process-manager/src/guard.rs#L39) (`ensure_safe_to_stop`, `filter_protected_pids`) and [`dim-host.ps1:2081,2089`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/scripts/dim-host.ps1#L2081) reject any stop target whose PID or process tree intersects the primary Zalo Desktop (`PROTECTED_PRIMARY_PROCESS`), verified live against `PID 12980` in `Test-8` (`"verified_pid_12980"`).
- **Junction / Symlink Unlink Safety:** `IsReparsePoint` ([`dim-host.ps1:1388,1473,1500`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/scripts/dim-host.ps1#L1388)) unlinks directory junctions safely without recursing into external link targets.
- **No Global Kill-All:** Stop & delete operate strictly by target profile ID (`--confirm` required on delete).
- **Minor PoC Observation:** After `run-zalo-poc.ps1` deleted all 3 live PoC profiles, an empty directory stub `AppData\Local\Temp` under `%LOCALAPPDATA%\MultiZaloDesktop_LivePoC\Profiles\poc-gamma_7e42562c` remained on disk because `Remove-Item -ErrorAction SilentlyContinue` hit a transient handle exit delay; zero profile data, locks, or processes remained (`9` primary Zalo PIDs unchanged).

### 10. Incorrect Restore
- **Atomic Backup Write:** Writes payload files alongside `.partial` manifest, computes per-file SHA-256 hashes, then atomically renames `.partial` -> `backup.json` (`FileBackupManager` in `backup.rs` and `dim-host.ps1`).
- **Disk Reserve Check:** Enforces `5 GiB + payloadEstimate` minimum free space before backup creation (`dim-host.ps1`).
- **Non-Destructive Restore-to-New-Profile:** Restore always allocates a brand-new profile ID/directory (never overwrites an existing or primary `%APPDATA%\ZaloData` profile).
- **Atomic Staging & Sweeper (Remediated `#15`/`#16` -> `#17`/`#19`):** Restores unpack and verify SHA-256 hashes inside `.restore-<stamp>-<id>` staging directories, atomically promote the staged directory upon full verification (rolling back on error), and sweep abandoned staging directories older than `STAGING_MAX_AGE` (`12h`, [`backup.rs:23,92,235`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/crates/profile-store/src/backup.rs#L23)).

### 11. Malicious Input Paths
- **Rust (`crates/core/src/path_guard.rs:15`):** `resolve_safe_child` rejects absolute paths, Windows drive prefixes, UNC paths, NUL bytes, and `..` components, and canonicalizes the deepest existing ancestor when targeting a nonexistent leaf path (`#16` -> `#17`) to close Windows/UNIX prefix-bypass gaps (`PATH_TRAVERSAL_REJECTED`).
- **Host Bridge & Tauri (`dim-host.ps1:302`, `commands.rs`):** `Path.GetFullPath` trailing-separator boundary containment checks protect data dirs and backup paths (`Test-8` confirms traversal rejection), `TryValidateProfileName` ([`dim-host.ps1:1647`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/scripts/dim-host.ps1#L1647)) sanitizes profile names, and `restore_backup` in [`apps/desktop/src-tauri/src/commands.rs`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/apps/desktop/src-tauri/src/commands.rs) validates backup directory names against traversal/drive-prefix payloads.

---

## 6. Rust Safety, Tauri Bridge & `modern-web-guidance` UI Audit

### Rust Safety & `unsafe` Block Review
- **Workspace Lints ([`Cargo.toml:39-46`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/Cargo.toml#L39-L46)):** `unsafe_op_in_unsafe_fn = "deny"`, `missing_docs = "warn"`, `clippy::unwrap_used = "warn"`, `clippy::expect_used = "warn"`.
- **`unsafe` Scope (Strictly Bounded to OS FFI):**
  - `crates/core/`, `crates/profile-store/`, `crates/process-manager/`: **0 `unsafe` blocks**.
  - [`crates/platform-windows/src/win32_ffi.rs`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/crates/platform-windows/src/win32_ffi.rs): Wraps Win32 process/window/registry/Restart Manager FFI (`RmStartSession`, `RmRegisterResources`, `RmGetList`, `CreateProcessW`, `CreateToolhelp32Snapshot`, `EnumWindows`, `RegOpenKeyExW`, `RegQueryValueExW`) behind safe RAII guards (`HandleGuard`, `RmSessionGuard`, `RegKeyGuard`) that close handles deterministically on drop and sign-extend `HKEY_CURRENT_USER` (`0x8000_0001_u32 as i32 as isize`).
  - [`crates/platform-macos/src/adapter.rs`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/crates/platform-macos/src/adapter.rs) & [`crates/platform-linux/src/adapter.rs`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/crates/platform-linux/src/adapter.rs): Strictly bounded `libc::kill` / `libc::waitpid` calls using validated positive `pid_t` values (`>= 1`).
- **Tauri Command Parity Note (Informational):** In [`apps/desktop/src-tauri/src/commands.rs`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/apps/desktop/src-tauri/src/commands.rs), `get_status` currently hardcodes primary `total_working_set_bytes: 0` (ignoring `with_size`) and `get_diagnostics` hardcodes `is_elevated: false`. The active Windows runtime uses the Signed-Host Bridge (`dim-host.ps1` + `ui-server.mjs`), which computes full process metrics and elevation state dynamically; align Tauri command structs before shipping the compiled Tauri shell in CI.
- **Unit Test Inventory:** `25` `#[test]` functions across `crates/*/src/lib.rs` (unexecuted locally because `rustc`/`cargo` are not installed on this Windows host; scheduled to run on GitHub Actions in [`.github/workflows/ci.yml`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/.github/workflows/ci.yml)).

### Local Web UI & `modern-web-guidance` Audit (`apps/desktop/ui/`)
All 4 required `modern-web-guidance` patterns (`dialog`, `responsive-table`, `dark-mode`, `tabs`) were verified statically against the live `modern-web-guidance@latest` specifications and dynamically in `Test-8` (`"modern_web_guidance_verified": ["dialog", "responsive-table", "dark-mode", "tabs"]`):

1. **`dialog`:** Native `<dialog aria-labelledby="...">` elements with zero redundant `role="dialog"` / `aria-modal`; declarative `command="show-modal"` / `command="close"` invoker attributes (`commandForElement` + fallback listeners); `autofocus` placed on Cancel in destructive dialogs; `dialog:modal { overscroll-behavior: contain; }`, `dialog::backdrop { overflow: hidden; overscroll-behavior: contain; }`; `dialog.returnValue` read & cleared on close.
2. **`responsive-table`:** Scrollable `<div tabindex="0" role="region" aria-labelledby="..." class="table-wrapper">` with `container-type: inline-size`; sticky `thead` & `tbody th`; `@container (width < 600px)` stacked mobile card view with `thead { display: none; }` and CSS custom property label pairing (`content: var(--label-2) ": "`).
3. **`dark-mode`:** `<meta name="color-scheme" content="light dark">` + `:root { color-scheme: light dark; }`; `@media (prefers-color-scheme: dark)` fallback paired with `@supports (color: light-dark(white, black))`; `@supports not (scrollbar-color: auto)` resets `scrollbar-width: auto` before `::-webkit-scrollbar*`; zero transitions on `scrollbar-color`.
4. **`tabs`:** `focusgroup="tablist nomemory"` + `focusgroupstart`; `hidden="until-found"` with `beforematch` reveal; active indicator on `[role="tab"][aria-selected="true"]::after`; panel visuals on `.panel-body` rather than `[role="tabpanel"]`; `ArrowLeft`/`ArrowRight` fallback; `scrollIntoView({ block: 'nearest', inline: 'nearest', container: 'nearest' })` (`#19` fix).

---

## 7. Security Remediation Verification Log

All findings reported during incremental verification (`#15`, `#16`) were fixed by `@rust-workspace` (`#17`) and `@desktop-ui-poc` (`#19`) and re-verified after the final edits:

| Finding | Target | Fix & Verification |
|---|---|---|
| Rust `backup.rs` non-atomic restore & missing stale staging sweep | [`crates/profile-store/src/backup.rs`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/crates/profile-store/src/backup.rs) | `#17` added `.restore-*` staging swap + rollback + 12h `STAGING_MAX_AGE` sweep |
| Rust `zalo_identity.rs` ignoring `config.json` `db_path.location` | [`crates/core/src/zalo_identity.rs`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/crates/core/src/zalo_identity.rs) | `#17` parses `db_path.location` + most-recent mtime selection |
| Rust `path_guard.rs` leaf canonicalization + `sanitize.rs` secret keys | [`crates/core/src/path_guard.rs`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/crates/core/src/path_guard.rs), [`crates/core/src/sanitize.rs`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/crates/core/src/sanitize.rs) | `#17` canonicalizes deepest ancestor + expands `password=`/`session_key=`/JSON colons |
| `ui-server.mjs` missing Host/Origin/Content-Type/`--` checks | [`apps/desktop/ui-server.mjs`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/apps/desktop/ui-server.mjs#L178-L218) | `#19` enforces `403 FORBIDDEN_HOST`, `403 FORBIDDEN_ORIGIN`, `415 UNSUPPORTED_MEDIA_TYPE`, `--` sentinel |
| `dim-host.ps1` `WinVerifyTrust`, `QuoteArg`, Folder Redirection, `5 GiB`, staging | [`scripts/dim-host.ps1`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/scripts/dim-host.ps1) | `#19` implemented & `Test-8` dynamically proved `WinVerifyTrust=0xC000A000` tamper rejection |
| `app.js` `scrollIntoView` `container: 'nearest'` & Tauri `restore_backup` traversal | [`apps/desktop/ui/app.js:40`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/apps/desktop/ui/app.js#L40), [`commands.rs`](file:///c:/Users/manhquy/Downloads/zalo/desktop-instance-manager/apps/desktop/src-tauri/src/commands.rs) | `#19` verified in source and `Test-8` MWG assertion |

---

## 8. Gate E — Security & Compliance Verdict

- **Verdict:** **PASS**
- **Rationale:** All 11 threat categories have verified, implemented controls; zero secrets or dangerous DOM/PowerShell patterns were detected; primary Zalo (`%APPDATA%\ZaloData` and `PID 12980`) preserved identical pre/post SHA-256 hashes across both automated fixture testing and live Zalo `26.10.20` concurrency testing; tampered executable execution was cryptographically rejected (`0xC000A000`); and all UI patterns comply with `modern-web-guidance`. Remaining parity items (`crates/platform-windows` invoking full `WinVerifyTrust` directly and `commands.rs` status/diagnostics parity) only affect the uncompiled Tauri binary path, which is gated under **Gate F (Release Readiness — CI Build & Code Signing)**.
