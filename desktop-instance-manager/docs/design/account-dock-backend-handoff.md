# Multi-Zalo Desktop — Account Dock Backend Integration & Handoff Contract

**UI Branch:** `feat/ui-vi-en-quiet-workspace` (`<ui-worktree>/desktop-instance-manager/`)  
**Target Main Baseline:** `a5feb4a` (`<repo-root>/desktop-instance-manager/`)  
**Common Ancestor:** `8cf4703`  
**UI Final Code Commit:** `fc91901ebc897c62630c0cfeeebaaed0fd8c469c` (`fc91901`, layout polish on top of `4904e6f`)  
**Scope:** Strictly UI/Design (`apps/desktop/ui/**` and `docs/design/**`). Zero modifications to `crates/**`, `apps/desktop/src-tauri/**`, `apps/desktop/ui-server.mjs`, `scripts/**`, or `.github/workflows/**`.

---

## 1. Runtime Transport & Auth Architecture

The UI controller (`apps/desktop/ui/app.js`) supports two runtime transports through a unified `apiCall(route, method, body, tauriCmd, tauriArgs)` adapter:

1. **Native Tauri 2 IPC (`window.__TAURI__.core.invoke` / `window.__TAURI_INTERNALS__.invoke`)**:
   - Enabled by `withGlobalTauri: true` in `<repo-root>/desktop-instance-manager/apps/desktop/src-tauri/tauri.conf.json`.
   - When `getTauriInvoke()` returns a function and `tauriCmd` is provided, `apiCall()` calls `await invoke(tauriCmd, tauriArgs)` directly, setting `connectionMode = 'tauri'` and `connectionOk = true` on resolution or `connectionOk = false` on rejection.
2. **Authenticated Local Loopback HTTP Bridge (`apps/desktop/ui-server.mjs`)**:
   - Binds strictly to `127.0.0.1` (`--port <n>` or `DIM_PORT`, default `18420`, or ephemeral `0` in tests).
   - **Session Token Auth (`X-DIM-Session-Token`)**: On startup, `ui-server.mjs` generates a 32-byte random hex token (`crypto.randomBytes(32).toString('hex')`) and injects `<meta name="dim-session-token" content="<64-hex>">` into `index.html` `<head>` when serving `/`.
   - `app.js` reads `getSessionToken()` (`document.querySelector('meta[name="dim-session-token"]')?.content || ''`) and attaches `X-DIM-Session-Token` on every `/api/*` request. `ui-server.mjs` compares with `crypto.timingSafeEqual` and rejects missing/forged tokens with `401 UNAUTHORIZED`.
   - **Timeout & Disconnect Handling**: `app.js` wraps every `fetch()` in an `AbortController` using `Number(globalThis.__MZ_API_TIMEOUT_MS) > 0 ? Number(globalThis.__MZ_API_TIMEOUT_MS) : 32000` (exceeding `ui-server.mjs`'s `30_000` ms `runHostCli` timeout so server `504 HOST_TIMEOUT` JSON arrives first when the server is responsive, while hung sockets still abort cleanly as `HOST_TIMEOUT`). HTTP `401`, `>= 500`, or `data.error_code` in `['HOST_EXEC_FAILED', 'HOST_TIMEOUT', 'UNAUTHORIZED']` flips `updateConnectionBadge('bridge', false)`.
   - **Bridge Security Headers & Guards (`ui-server.mjs`)**:
     - Non-loopback `Host` header -> `403 FORBIDDEN_HOST`
     - Non-loopback `Origin` or `Sec-Fetch-Site: cross-site` -> `403 FORBIDDEN_ORIGIN`
     - Rate limit (`300` req/min default) -> `429 RATE_LIMITED`
     - State-changing (`POST`/`PUT`/`PATCH`/`DELETE`) without `Content-Type: application/json` -> `415 UNSUPPORTED_MEDIA_TYPE`
     - JSON request body limit: `64 * 1024` bytes (`400 BAD_REQUEST`)
     - Sequential mutation queue (`enqueueMutation`) serializes write commands; `POST /api/profiles/:id/backup` rejects custom `body.dest` unless `allowCustomBackupDest` is enabled (`403 FORBIDDEN_DEST`).
     - CSP header matches `tauri.conf.json`: `default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'`.

---

## 2. Tauri 2 IPC Command Table (JS `invoke` Args vs Rust Signatures)

All 12 commands registered in `<repo-root>/desktop-instance-manager/apps/desktop/src-tauri/src/main.rs` (`tauri::generate_handler!`) and implemented in `apps/desktop/src-tauri/src/commands.rs`:

> **Tauri 2 Argument Case Convention:** `#[tauri::command]` in Tauri 2 defaults to `rename_all = "camelCase"` for JS payloads. Therefore JS `withSize`, `newName`, and `backupDir` map automatically to Rust `with_size`, `new_name`, and `backup_dir`.

| # | Tauri Command (`tauriCmd`) | JS `tauriArgs` Passed by `app.js` | Rust Signature (`commands.rs`) | Rust Return Type | Notes / Parity Status |
|---|---|---|---|---|---|
| 1 | `get_status` | `{ withSize }` (`boolean`) | `get_status(state: State<'_, AppState>, with_size: Option<bool>)` | `Result<StatusResponse, String>` | `with_size` controls `data_size_bytes` (`Some(bytes)` vs `None`). |
| 2 | `create_profile` | `{ name, notes }` (`string`, `string`) | `create_profile(state, name: String, notes: Option<String>)` | `ApiEnvelope<ProfileRecord>` | Acquires `op_lock`; calls `ensure_profile_layout`. |
| 3 | `rename_profile` | `{ id, newName }` (`string`, `string`) | `rename_profile(state, id: String, new_name: String)` | `ApiEnvelope<ProfileRecord>` | Acquires `op_lock`; preserves immutable `directory_name`. |
| 4 | `delete_profile` | `{ id, confirm: true }` (`string`, `boolean`) | `delete_profile(state, id: String, confirm: Option<bool>)` | `ApiEnvelope<serde_json::Value>` | Fixed in post-review hardening (`confirm: true` required or `CONFIRM_REQUIRED`). |
| 5 | `launch_profile` | `{ id }` (`string`) | `launch_profile(state, id: String)` | `ApiEnvelope<ProfileRuntimeState>` | Acquires `op_lock`; launches `LaunchTarget::OfficialZalo`. |
| 6 | `focus_profile` | `{ id }` (`string`) | `focus_profile(state, id: String)` | `ApiEnvelope<ProfileRuntimeState>` | Returns `NOT_RUNNING` if stopped; restores/focuses window. |
| 7 | `stop_profile` | `{ id }` (`string`) | `stop_profile(state, id: String)` | `ApiEnvelope<u32>` | Acquires `op_lock`; enforces `PrimaryProtectionGuard`. |
| 8 | `backup_profile` | `{ id }` (`string`) | `backup_profile(state, id: String)` | `ApiEnvelope<BackupResult>` | Acquires `op_lock`; fails with `PROFILE_RUNNING` if open. |
| 9 | `list_backups` | `{}` | `list_backups(state)` | `BackupsResponse` | Reads `backup.json` (`schema_version == 1`), skips `*.partial`. |
| 10 | `restore_backup` | `{ backupDir, name }` (`string`, `string`) | `restore_backup(state, backup_dir: String, name: Option<String>)` | `ApiEnvelope<ProfileRecord>` | Acquires `op_lock`; confines `backup_dir` inside `backups_dir`; empty `name` becomes `None` (auto-named). |
| 11 | `get_diagnostics` | `{}` | `get_diagnostics(state)` | `Result<serde_json::Value, String>` | Returns live elevation, SAC state, disk free, ACL safety, OS matrix, redacted logs. |
| 12 | `verify_isolation` | `{}` | `verify_isolation(state)` | `Result<serde_json::Value, String>` | Delegates to `InstanceCoordinator::verify_isolation` (`IsolationVerificationReport`). |

---

## 3. HTTP Bridge Routes (`ui-server.mjs`)

| Method & Route | Auth Required | Request Body / Query | Backend CLI (`dim-host.ps1`) | Called by `app.js`? | Notes |
|---|---|---|---|---|---|
| `GET /` (static assets) | No | Static file path | — | Yes | Injects `<meta name="dim-session-token">` into `index.html`. |
| `GET /api/health` | No | None | — | No (probe/tests) | Returns `{ ok: true, bind: '127.0.0.1', auth_required: true }`. |
| `GET /api/status` | `X-DIM-Session-Token` | Optional `?withSize=1` | `status [--with-size]` | Yes | Passes `--with-size` when `withSize=1`. |
| `GET /api/discover` | `X-DIM-Session-Token` | None | `discover` | No | UI consumes `installation` embedded inside `/api/status`. |
| `GET /api/backups` | `X-DIM-Session-Token` | None | `backups` | Yes | Lists manifests under managed backups root. |
| `GET /api/diagnostics` | `X-DIM-Session-Token` | None | `diagnostics` | Yes | Returns elevation, SAC, disk, ACL, OS matrix, logs. |
| `GET /api/verify-isolation` | `X-DIM-Session-Token` | None | `verify-isolation` | Yes | Returns 7-check isolation report. |
| `POST /api/profiles` | `X-DIM-Session-Token` + JSON | `{ name, notes }` | `create <name> [--notes <notes>]` | Yes | Enqueued via `enqueueMutation`. |
| `POST /api/backups/restore` | `X-DIM-Session-Token` + JSON | `{ backupDir, name }` | `restore <backupDir> [--name <name>]` | Yes | Always creates a new profile; never overwrites existing data. |
| `POST /api/profiles/:id/rename` | `X-DIM-Session-Token` + JSON | `{ newName }` | `rename <id> <newName>` | Yes | Validates safe `:id` param (`INVALID_ID` -> `400`). |
| `DELETE /api/profiles/:id` | `X-DIM-Session-Token` + JSON | `{ confirm: true }` | `delete <id> --confirm` | Yes | `confirm: true` maps to `--confirm`. |
| `POST /api/profiles/:id/launch` | `X-DIM-Session-Token` + JSON | `{}` | `launch <id>` | Yes | Enqueued via `enqueueMutation`. |
| `POST /api/profiles/:id/focus` | `X-DIM-Session-Token` + JSON | `{}` | `focus <id>` | Yes | Enqueued via `enqueueMutation`. |
| `POST /api/profiles/:id/stop` | `X-DIM-Session-Token` + JSON | `{}` | `stop <id>` | Yes | Enqueued via `enqueueMutation`. |
| `POST /api/profiles/:id/backup` | `X-DIM-Session-Token` + JSON | `{}` (`dest` forbidden unless configured) | `backup <id>` | Yes | `body.dest` rejected with `403 FORBIDDEN_DEST` by default. |

---

## 4. Response Schemas & Cross-Backend Differences

### 4.1 `ApiEnvelope<T>` (Mutations: Create, Rename, Delete, Launch, Focus, Stop, Backup, Restore)
```json
{
  "ok": true,
  "error_code": null,
  "message": "Created profile 'Work'.",
  "data": { ... }
}
```
On failure, `ok: false`, `error_code: "<UPPER_SNAKE_CASE>"`, `message: "<redacted string>"`, `data: null`.

### 4.2 `StatusResponse` (`get_status` / `GET /api/status`)
```json
{
  "ok": true,
  "platform": "windows",
  "data_dir": "<redacted_path>",
  "installation": {
    "valid": true,
    "mode": "official-zalo",
    "version": "26.10.20",
    "authenticode_status": "Valid",
    "signer_subject": "VNG GROUP JSC",
    "error": null
  },
  "primary_zalo": {
    "is_running": true,
    "locking_pids": [4120],
    "process_count": 1,
    "total_working_set_bytes": 268435456,
    "zalo_user_id": null
  },
  "profiles": [
    {
      "id": "a1b2c3d4",
      "name": "Công việc",
      "directory_name": "cong-viec_a1b2c3d4",
      "created_at": "2026-11-17T10:00:00Z",
      "last_launched_at": null,
      "zalo_user_id": null,
      "notes": "",
      "running": false,
      "main_pid": null,
      "process_count": 0,
      "window_handle": 0,
      "window_title": null,
      "total_working_set_bytes": 0,
      "data_size_bytes": null,
      "stale_lockfile_detected": false
    }
  ]
}
```
**Observed Rust vs `dim-host.ps1` Schema Deltas (safely handled by `app.js`):**
- `app_version`: Neither `commands.rs` nor `dim-host.ps1` emits `app_version`; `app.js` falls back to `UI_VERSION` (`'0.1.0'`).
- `data_size_bytes`: `commands.rs` emits `null` when `with_size` is false; `dim-host.ps1` omits the key when `includeSize` is false. `formatBytes()` renders both `null` and `undefined` as `'—'`.
- `installation.mode`: `commands.rs` emits `"official-zalo"`; `dim-host.ps1` emits `"official-zalo"`, `"node-fixture"`, or `"custom-exe"`.

### 4.3 `BackupsResponse` (`list_backups` / `GET /api/backups`)
```json
{
  "ok": true,
  "backups": [
    {
      "backup_name": "cong-viec_20261117T101500Z",
      "backup_dir": "<managed_backups_root>/cong-viec_20261117T101500Z",
      "backup_id": "...",
      "created_at": "2026-11-17T10:15:00Z",
      "file_count": 142,
      "total_bytes": 104857600,
      "source_profile_id": "a1b2c3d4",
      "source_profile_name": "Công việc"
    }
  ]
}
```
`app.js` also tolerates `bk.source_profile?.id` / `bk.source_profile?.name` for legacy/mock compatibility.

### 4.4 Diagnostics (`get_diagnostics` / `GET /api/diagnostics`)
```json
{
  "ok": true,
  "is_elevated": false,
  "smart_app_control_state": 0,
  "disk_free_bytes": 53687091200,
  "disk_free_ok": true,
  "profiles_acl_safe": true,
  "folder_redirection": { "redirected": false, "warning": null },
  "platform_support_matrix": [
    { "platform": "Windows 10/11", "status": "PARTIALLY_SUPPORTED", "strategy": "...", "limits": "..." }
  ],
  "sanitized_logs": ["..."]
}
```
Note: `SupportStatus` values in `platform_support_matrix[].status` are normalized by `translateSupportStatus()` across both `SCREAMING_SNAKE_CASE` (`SUPPORTED_AND_TESTED`, `PARTIALLY_SUPPORTED`, `EXPERIMENTAL`, `BLOCKED`, `UNSUPPORTED`, `NOT_TESTED`) and Rust `PascalCase` (`SupportedAndTested`, `PartiallySupported`, etc.).

### 4.5 Isolation Verification (`verify_isolation` / `GET /api/verify-isolation`)
```json
{
  "ok": true,
  "all_passed": true,
  "timestamp_utc": "2026-11-17T15:30:53Z",
  "checks": [
    { "check_name": "ApplicationDiscovery", "status": "PASS", "passed": true, "detail": "..." }
  ]
}
```
7 backend checks (`coordinator.rs` & `dim-host.ps1`): `ApplicationDiscovery`, `PrimaryZaloDataSeparation`, `DistinctProfilePathsAndMarkers`, `DistinctProcessOwnership`, `DistinctZaloUserIds`, `RestrictiveNtfsAcls`, `FolderRedirectionCheck`.

---

## 5. Complete Error Code Contract (All 27 Localized Codes + Transport/CLI Codes)

`resolveErrorInfo(errorCode, rawMessage)` in `apps/desktop/ui/i18n/index.js` separates user-facing localized copy (`errors.<CODE>`) from raw `error_code` and `message` (placed inside the collapsed `<details id="status-banner-details">` box).

### 5.1 All 27 Localized Error Codes (`vi-VN.json` & `en-US.json`)

| # | `error_code` | Origin | Category |
|---|---|---|---|
| 1 | `INVALID_NAME` | `DimError::InvalidProfileName` / `dim-host.ps1` | Profile validation |
| 2 | `DUPLICATE_NAME` | `DimError::DuplicateProfileName` / `dim-host.ps1` | Profile validation |
| 3 | `NOT_FOUND` | `DimError::ProfileNotFound` / `dim-host.ps1` / `ui-server.mjs` (404) | Resource lookup |
| 4 | `PROFILE_RUNNING` | `DimError::ProfileRunning` / `dim-host.ps1` | Lifecycle guard |
| 5 | `PROFILE_MARKER_MISMATCH` | `DimError::ProfileIdentityMismatch` / `dim-host.ps1` | `.zalo-profile-id` integrity |
| 6 | `PROTECTED_PRIMARY_PROCESS` | `DimError::ProtectedPrimaryProcess` / `dim-host.ps1` | Primary Zalo guard |
| 7 | `PROTECTED_PRIMARY_PATH` | `DimError::ProtectedPrimaryPath` / `dim-host.ps1` | Primary Zalo guard |
| 8 | `DUPLICATE_ZALO_USER_ID` | `DimError::DuplicateZaloUserId` / `dim-host.ps1` | Session uniqueness guard |
| 9 | `INSUFFICIENT_DISK_SPACE` | `DimError::InsufficientDiskSpace` / `dim-host.ps1` | Storage preflight (`< 5 GiB`) |
| 10 | `PATH_TRAVERSAL` | `DimError::PathTraversal` / `dim-host.ps1` | Path confinement |
| 11 | `FOLDER_REDIRECTION_DETECTED` | `DimError::FolderRedirectionBlocked` / `dim-host.ps1` | Windows AppData redirection |
| 12 | `INSTALL_NOT_FOUND` | `DimError::InstallNotFound` / `dim-host.ps1` | Zalo Desktop discovery |
| 13 | `UNVERIFIED_BINARY` | `DimError::UnverifiedBinary` / `dim-host.ps1` | Authenticode / publisher check |
| 14 | `NOT_RUNNING` | `DimError::NotRunning` / `dim-host.ps1` | Focus guard |
| 15 | `UNSUPPORTED_PLATFORM` | `DimError::UnsupportedPlatform` | OS boundary |
| 16 | `BACKUP_ERROR` | `DimError::BackupError` / `dim-host.ps1` | Atomic backup / SHA-256 verify |
| 17 | `RESTORE_ERROR` | `DimError::RestoreError` | Backup restore |
| 18 | `PROCESS_ERROR` | `DimError::ProcessError` | Process spawn / stop / focus |
| 19 | `SCHEMA_ERROR` | `DimError::SchemaError` | `profiles.json` / `backup.json` schema |
| 20 | `CONFIRM_REQUIRED` | `DimError::ConfirmationRequired` / `dim-host.ps1` | Explicit delete confirmation |
| 21 | `IO_ERROR` | `DimError::Io` | Filesystem I/O |
| 22 | `JSON_ERROR` | `DimError::Json` | JSON serialization |
| 23 | `HOST_EXEC_FAILED` | `ui-server.mjs` (500) / `app.js` catch | Host or IPC transport failure |
| 24 | `HOST_TIMEOUT` | `ui-server.mjs` (504) / `app.js` `AbortController` | Command timeout (`30s`/`32s`) |
| 25 | `UNAUTHORIZED` | `ui-server.mjs` (401) | Missing/invalid `X-DIM-Session-Token` |
| 26 | `RATE_LIMITED` | `ui-server.mjs` (429) | Loopback rate limiter (`> 300/min`) |
| 27 | `UNKNOWN` | `app.js` / `i18n/index.js` fallback | Unmapped / generic fallback |

> **Note:** Two legacy UI dictionary aliases (`PROFILE_IDENTITY_MISMATCH` and `CONFIRMATION_REQUIRED`) are also retained in `errors.*` for backwards compatibility with older mocks alongside canonical backend codes `PROFILE_MARKER_MISMATCH` and `CONFIRM_REQUIRED`.

### 5.2 Additional HTTP Transport & PowerShell CLI Codes
`ui-server.mjs` and `dim-host.ps1` can also emit protocol/CLI codes outside the 27 domain/infra keys:
- `ui-server.mjs`: `BAD_REQUEST` (400), `INVALID_ID` (400), `FORBIDDEN_HOST` (403), `FORBIDDEN_ORIGIN` (403), `FORBIDDEN_DEST` (403), `PATH_TRAVERSAL_REJECTED` (403), `ROUTE_NOT_FOUND` (404), `METHOD_NOT_ALLOWED` (405), `UNSUPPORTED_MEDIA_TYPE` (415).
- `dim-host.ps1`: `PATH_TRAVERSAL_REJECTED`, `MISSING_ARG`, `UNKNOWN_COMMAND`, `UNHANDLED_EXCEPTION`, `INIT_FAILED`.
All of these gracefully map via `resolveErrorInfo()` to the localized `errors.UNKNOWN` user message while exposing the exact `rawCode` and sanitized `rawDetail` inside `#status-banner-details`.

---

## 6. Profile Status & Honesty Semantics

1. **Window State != Login State:** `prof.running === true` renders `"Cửa sổ đang mở"` / `"Window open"` — it explicitly avoids claiming `"Signed in"`, because process/lockfile inspection only confirms an active Zalo Desktop window tree.
2. **Account-Level Technical Metadata Hidden by Default:** `main_pid`, `process_count`, `window_title`, `total_working_set_bytes`, `data_size_bytes`, `zalo_user_id`, `directory_name`, and `stale_lockfile_detected` remain inside `#profile-details-dialog` -> `<details class="advanced-details">`.
3. **Primary Zalo Card Honesty (`#primary-zalo-section`):** Always visible on Home as a read-only protection card with 3 states:
   - `running`: `state.connectionOk && primary.is_running === true`
   - `stopped`: `state.connectionOk && primary.is_running === false`
   - `unknown`: `!state.connectionOk || !primary || typeof primary.is_running !== 'boolean'`
4. **Isolation & OS Matrix Honesty (`resolveIsolationCheckResult` & `translateSupportStatus`):** `NOT_TESTED`, `BLOCKED`, `UNSUPPORTED`, and `UNKNOWN` are preserved verbatim even if a legacy payload sets `passed: true`.

---

## 7. Color, Theme & Locale Persistence

- **Account Identity Colors (`localStorage` key `multi-zalo.accountColors`):**
  - Stored as a JSON map `{ "<profile_id>": "<color_id>" }` across 8 WCAG AA colors (`blue`, `teal`, `green`, `amber`, `coral`, `plum`, `indigo`, `slate`).
  - **Lifecycle in `app.js`:**
    - Create: `pickNextAvailableColorId(profiles)` avoids collisions with existing accounts.
    - Rename: `#rename-color-palette` lets users update an account's color alongside its name.
    - Delete: `removeAccountColor(profileId)` removes stale entries on success.
    - Restore: Inherits color from `bk.source_profile_id` (or `bk.source_profile?.id`) for the newly created restored profile ID.
    - Fallback: Deterministic hash of `prof.id || prof.name || 'z'` when no saved entry exists.
  - **Optional Non-Blocking Backend Enhancement:** Because `localStorage` is origin-scoped (meaning `http://127.0.0.1:<ephemeral_port>` and Tauri `tauri://localhost` do not share `localStorage`), backend may optionally add `#[serde(default)] pub color: Option<String>` to `ProfileRecord` (`crates/core/src/models.rs`) and optional `color` args to `create_profile` / `rename_profile`. Not required for current integration.
- **Theme (`localStorage` key `multi-zalo.theme`):** `'system' | 'light' | 'dark'`, read synchronously before first paint by `apps/desktop/ui/theme-init.js` (also falls back to legacy `'color-scheme'` key) to prevent FOUC and keep `meta[name="color-scheme"]` and `document.documentElement[data-theme]` synchronized.
- **Locale (`localStorage` key `multi-zalo.locale`):** `'vi-VN'` (default) or `'en-US'`, synced to `document.documentElement.lang` (`'vi'` / `'en'`).

---

## 8. Resolved `INTEGRATION_BLOCKER`s (Pre-Wave 1 `cc74584` -> Final)

| Blocker ID | Defect in `cc74584` | Impact Against `a5feb4a` Backend | Resolution |
|---|---|---|---|
| `IB-01` | `apiCall()` did not send `X-DIM-Session-Token` from `<meta name="dim-session-token">` | All `/api/*` calls failed with `401 UNAUTHORIZED` against `ui-server.mjs` | Added `getSessionToken()` + `X-DIM-Session-Token` header + `AbortController` `32000ms` timeout + `401`/`>= 500` disconnect state. |
| `IB-02` | `delete_profile` invoked `{ id: targetId }` without `confirm: true` | Tauri `commands.rs` rejected delete with `CONFIRM_REQUIRED` | Updated `tauriArgs` to `{ id: targetId, confirm: true }`. |
| `IB-03` | `refreshAll()` assigned `state.status = statusData` even when `statusData.ok === false` | Masked bridge/CLI errors and corrupted UI state | Added `statusData?.ok === false` check calling `updateConnectionBadge(..., false)` + `showErrorBanner` without overwriting `state.status`. |
| `IB-04` | Missing 11 backend/bridge error keys in `i18n/{vi-VN,en-US}.json` | Backend errors (`CONFIRM_REQUIRED`, `PROFILE_MARKER_MISMATCH`, `UNVERIFIED_BINARY`, `HOST_TIMEOUT`, `UNAUTHORIZED`, etc.) fell back to generic `UNKNOWN` | Added all 11 error keys with 100% `vi-VN`/`en-US` parity. |

---

## 9. Git Merge Conflict Guidance (`main` `a5feb4a` <-> `feat/ui-vi-en-quiet-workspace`)

Comparing `8cf4703..a5feb4a` (`main`) against `8cf4703..fc91901` (`feat/ui-vi-en-quiet-workspace`), **exactly 3 files overlap** and will produce merge conflicts:

1. `desktop-instance-manager/apps/desktop/ui/index.html`
2. `desktop-instance-manager/apps/desktop/ui/styles.css`
3. `desktop-instance-manager/apps/desktop/ui/app.js`

Zero conflicts exist in `crates/**`, `apps/desktop/src-tauri/**`, `apps/desktop/ui-server.mjs`, `scripts/**`, `docs/design/**`, `apps/desktop/ui/i18n/**`, `apps/desktop/ui/theme-init.js`, or `apps/desktop/ui/tests/**`.

### Recommended Merge Resolution
1. **Resolve the 3 conflicting UI files in favor of `feat/ui-vi-en-quiet-workspace` (`--theirs` when merging into `main`)**:
   - Every security/hardening fix introduced in `main` `a5feb4a` for those 3 files (`X-DIM-Session-Token`, `delete_profile` `confirm: true`, `statusData.ok === false` guard, `:focus-visible`, `'focusGroup' in HTMLElement.prototype`, full CSP compliance, no `.innerHTML`, `data_size_bytes` handling, `NOT_TESTED`/`BLOCKED` honesty) has already been ported into `feat/ui-vi-en-quiet-workspace`.
2. **Update legacy DOM regex assertions in `tests/run-fixture-tests.mjs` (`Test-8`, `a5feb4a` lines 376–417 and line 429)**:
   - On `main` `a5feb4a`, `Test-8` asserts regexes tailored to the pre-redesign v1 table/tab layout:
     - Lines 384–385: `#safety-dialog` and literal `>Cancel</button>` (replaced by `#view-help` and `data-i18n="dialogs.cancel"` -> `Hủy`)
     - Lines 395–399: `#profiles-table-caption`, `@container (width < 600px)`, `--label-2` (replaced by `.account-grid` cards)
     - Lines 410–416: `role="tablist"`, `hidden="until-found"`, `beforematch`, `scrollIntoView` (replaced by `<nav class="topbar-nav">` + `.view[hidden]`)
     - Line 429: `assert.match(indexHtml, /Multi-Zalo Desktop/)` (title is now `Multi-Zalo — Tài khoản`)
   - **Action in Merge PR:** Replace `Test-8`'s static UI regexes (lines 376–417 and line 429) with execution/checks of `apps/desktop/ui/tests/i18n.test.mjs` and `apps/desktop/ui/tests/ui-contracts.test.mjs` (or match Account Dock markup), while **keeping lines 418–545** (`ui-server.mjs` auth, CSRF `Origin`/`Content-Type`, non-loopback `Host`, `FORBIDDEN_DEST`, and path traversal tests) intact.

---

## 10. Mock-Only & Environment Test Hooks in `apps/desktop/ui/`

Verify during backend security review that no production bypasses exist:
- `globalThis.__MZ_API_TIMEOUT_MS` (`app.js`): Optional positive number overriding the `32000` ms `AbortController` fetch timeout during automated timeout tests. Ignored unless explicitly set in the JS runtime.
- `globalThis.__TEST_FETCH__` (`i18n/index.js`): Optional fetch replacement used solely by `tests/i18n.test.mjs` to simulate 404/invalid JSON/abort errors.
- `node:fs` dictionary loader branch (`i18n/index.js`): Runs only when `process.versions?.node` is truthy and the dictionary URL protocol is `file:`.
- `window.__TAURI_INTERNALS__.invoke` (`app.js` / `tests/visual-cdp.mjs`): Native Tauri IPC detection hook (`getTauriInvoke()`). `visual-cdp.mjs` temporarily attaches a synthetic `window.__TAURI_INTERNALS__ = { invoke }` stub inside headless Edge to verify `delete_profile` (`{ id, confirm: true }`) and `get_status` argument serialization, then deletes it immediately (`delete window.__TAURI_INTERNALS__`). Verified zero extra hidden flags or auth bypasses exist in `app.js`, `theme-init.js`, or `i18n/index.js`.

---

## 11. Native Windows Integration Test Checklist (Post-Merge on `main`)

Run from `<repo-root>/desktop-instance-manager/` after merging `feat/ui-vi-en-quiet-workspace` and updating `Test-8` in `tests/run-fixture-tests.mjs`:

- [ ] **Rust Formatting & Lints:**
  - `cargo fmt --all -- --check`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml -- --check`
  - `cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml -- -D warnings`
- [ ] **Rust Unit & Integration Suites:**
  - `cargo test --workspace --all-targets`
  - `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml`
- [ ] **UI Unit, Contract & Headless Edge CDP Visual Suites:**
  - `node --test apps/desktop/ui/tests/i18n.test.mjs apps/desktop/ui/tests/ui-contracts.test.mjs`
  - `node apps/desktop/ui/tests/visual-cdp.mjs`
- [ ] **Signed-Host Bridge, Auth & Contract Parity Suites:**
  - `node tests/run-fixture-tests.mjs`
  - `node tests/run-contract-parity-tests.mjs`
- [ ] **Live `ui-server.mjs` Smoke (Fixture + Official Zalo):**
  - Start `node apps/desktop/ui-server.mjs --port 18420 --node-fixture --app-path fixtures/test-desktop-app/fixture-app.mjs`
  - Verify `<meta name="dim-session-token">` injection, `X-DIM-Session-Token` auth across all routes, Step 1 error display on duplicate/invalid name, account create -> Step 2 launch -> focus -> stop -> rename+color -> backup -> restore -> delete, language/theme persistence across reload, and Diagnostics isolation checks.
- [ ] **Live Tauri 2 WebView2 Desktop Smoke (`dim-desktop.exe` / `cargo tauri dev`):**
  - Verify `getTauriInvoke()` resolves native IPC (`settings.about.connection` shows `Ứng dụng máy tính` / `Desktop app`), all 12 commands succeed, `delete_profile` passes `confirm: true`, `restore_backup` handles blank `name`, CSP allows all UI scripts/styles/SVGs without console errors, and Primary Zalo protection blocks termination.
- [ ] **Human-Guided L3/L4 Interactive Checklist (Physical Zalo QR Sign-In):**
  - `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run-interactive-l3-checklist.ps1`
- [ ] **Cross-Platform Hardware Verification (Outside Windows):**
  - Verify macOS WKWebView + `dim-platform-macos` (`NOT_TESTED`) and Ubuntu LTS WebKitGTK + `dim-platform-linux` (`EXPERIMENTAL` / `NOT_TESTED`).
