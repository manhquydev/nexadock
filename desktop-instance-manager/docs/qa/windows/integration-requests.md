# Windows Product Acceptance & UX Integration Requests

- **Audit Branch / Worktree:** `qa/windows-native-acceptance` (`<WORKTREE_ROOT>/zalo-windows-acceptance`)
- **Audited Candidate SHA:** `cd62c523222cb3a02051347e85313226de8b6964` (`integration/reliability-hardening`, advancing initial audit baseline `004ab7cd1b31f8dd9c0e1d9bdd659d8f6920df5d` via `bff0674` + `cd62c52`)
- **Release Packaging State:** `AWAITING_INTEGRATION_BUILD` (`<WORKTREE_ROOT>/zalo-integration` is now clean at `cd62c52`, closing `BLK-01`; Release Captain NSIS/MSI installer remains `NOT_BUILT_YET` under `BLK-03`/`BLK-04`)
- **Author:** Mission G Windows Native User Acceptance & Installation Lab (`ux-docs`, consolidating findings from `journeys-and-errors.mjs` and `native-tauri-smoke.mjs`)
- **Governance Rule:** Per Mission G isolation policy, zero product files (`crates/**`, `apps/desktop/**`, `scripts/**`, `.github/workflows/**`) were modified in `<WORKTREE_ROOT>/zalo-windows-acceptance`. All product UX, error-copy, state-mapping, parity, and accessibility improvements identified during Windows native user acceptance are recorded below as **Integration Requests** for Central Integration / Release Captain.

---

## Summary Table (`cd62c523222cb3a02051347e85313226de8b6964`)

| Request ID | Cross-Ref ID | Priority | Category | Target Files | Status at `cd62c52` | Blocks `INTERNAL_TEST_BUILD`? | Blocks `READY_FOR_BETA`? |
| :--- | :---: | :---: | :--- | :--- | :--- | :---: | :---: |
| **`IR-QA-UX-01`** | — | **`P1`** | Crash / Startup Resilience (Error #7: Pre-existing Corrupted `profiles.json`) | `apps/desktop/src-tauri/src/main.rs`, `apps/desktop/src-tauri/src/state.rs` | **OPEN** at `cd62c52` | No *(fail-closed preserves data)* | **YES** |
| **`IR-QA-UX-02`** | — | **`P1`** | Tauri Query Error Envelope & Error-Code Parity (`JSON_ERROR`, `PATH_TRAVERSAL_REJECTED`) | `apps/desktop/src-tauri/src/commands.rs`, `apps/desktop/ui/app.js`, `apps/desktop/ui/i18n/{vi-VN,en-US}.json` | **OPEN** at `cd62c52` | No | **YES** |
| **`IR-QA-UX-03`** | `IR-NAT-01` | **`P1`** | Crash Recovery & Transitional Lifecycle UI (`recovery_required`, `starting`, `stopping`) | `apps/desktop/ui/app.js`, `apps/desktop/ui/i18n/{vi-VN,en-US}.json` | **VERIFIED CLOSED in `cd62c52`** (`bff0674`) | Closed | Closed |
| **`IR-QA-UX-04`** | `OBS-RESTORE-CORRUPT-COPY` | **`P2`** | Misleading `BACKUP_ERROR` Copy During Corrupted Backup Restore & Silent Manifest Skip | `crates/profile-store/src/backup.rs`, `apps/desktop/src-tauri/src/commands.rs`, `apps/desktop/ui/i18n/{vi-VN,en-US}.json` | **OPEN** at `cd62c52` | No | Recommended |
| **`IR-QA-UX-05`** | `OBS-COLOR-PERSISTENCE` | **`P3`** | Persist Account Identity Color Badge in `profiles.json` & `backup.json` | `crates/core/src/models.rs`, `crates/profile-store/src/{store,backup}.rs`, `apps/desktop/ui/app.js` | **OPEN** at `cd62c52` | No | No *(polish)* |
| **`IR-QA-UX-06`** | `IR-NAT-02` | **`P3`** | Inline Recovery CTAs, 5 GB Disk Copy, ACL Diagnostics & Tauri Fixture Hook | `apps/desktop/src-tauri/src/commands.rs`, `apps/desktop/ui/{index.html,app.js}`, `apps/desktop/ui/i18n/{vi-VN,en-US}.json` | **`DIM_NODE_FIXTURE_SCRIPT` CLOSED in `cd62c52`** (`bff0674`); copy polish **OPEN** | No | No *(polish)* |
| **`IR-QA-UX-07`** | `OBS-PS1-ACL-ERROR-CODE` | **`P2`** | Signed-Host PowerShell NTFS ACL `UnauthorizedAccessException` -> `IO_ERROR` Parity | `scripts/dim-host.ps1` | **OPEN** at `cd62c52` (`dim-cli.exe` returns `IO_ERROR`; PS1 returns `UNHANDLED_EXCEPTION`) | No | Recommended |
| **`IR-QA-UX-08`** | `IR-NAT-03` | **`P3`** | OS Text-Scale Accessibility (`rem` Root Font Sizing + `meta[name="text-scale"]`) | `apps/desktop/ui/index.html`, `apps/desktop/ui/styles.css` | **OPEN** at `cd62c52` | No | No *(a11y polish)* |

---

## Detailed Integration Requests

### `IR-QA-UX-01` (`P1`, OPEN) — Graceful Degraded Startup When `profiles.json` Is Corrupted Before Native Tauri Launch

- **User Scenario:** Error Handling Scenario #7 (*Corrupted `profiles.json` on disk prior to opening Multi-Zalo Desktop*).
- **Evidence at `cd62c52`:**
  - `apps/desktop/src-tauri/src/main.rs:8-9` executes:
    ```rust
    let app_state = AppState::initialize().expect("failed to initialize Multi-Zalo Desktop state");
    ```
  - `AppState::from_data_dir` (`apps/desktop/src-tauri/src/state.rs:46-47`) calls `store.ensure_initialized()?`, which calls `read_document_locked()?` (`crates/profile-store/src/store.rs:116-121`).
  - Because release builds compile with `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]` (`main.rs:1`), a pre-existing `0`-byte or malformed `<LOCALAPPDATA>\MultiZaloDesktop\profiles.json` causes `dim-desktop.exe` to panic and terminate before creating any WebView2 window, with no visible dialog or explanation for a non-technical user.
  - Note: Data safety **passes** (the original `profiles.json` is preserved untouched per `DEFECT-08c`), and the Signed-Host Bridge (`ui-server.mjs`) survives startup and surfaces an error banner in the UI.
- **Requested Change:**
  1. Allow `AppState::initialize()` to construct `AppState` even when `ensure_initialized()` returns `DimError::Json` / `DimError::SchemaError` / `DimError::Io`, recording the startup health error on `AppState` instead of panicking in `main.rs:9` (aligning with `0005-IR-005`).
  2. Open the Tauri window normally so the Account Dock renders a plain-language recovery banner guiding the user to **Troubleshoot (`Kiểm tra sự cố`)** or **Backups (`Sao lưu`)** while keeping `profiles.json` read-only until repaired or restored.

---

### `IR-QA-UX-02` (`P1`, OPEN) — Structured Error Codes on Tauri Read Commands & Actionable Copy for `JSON_ERROR` / `PATH_TRAVERSAL_REJECTED`

- **User Scenario:** Error Handling Scenarios #2 (*Backend failure*), #3 (*Insufficient directory permissions*), and #7 (*Corrupted metadata*).
- **Evidence at `cd62c52`:**
  1. **Tauri read commands strip `error_code`:** `get_status`, `get_diagnostics`, and `verify_isolation` in `apps/desktop/src-tauri/src/commands.rs` return `Result<_, String>` via `.map_err(redact_err_text)`, unlike mutation commands which return `ApiEnvelope<T>` with `error_code`. When `get_status` fails in Native Tauri, `apps/desktop/ui/app.js:176-179` unconditionally sets `tauriErr.errorCode = 'HOST_EXEC_FAILED'`, showing *"Không thể kết nối với ứng dụng. Thử lại."* / *"Could not connect to the app. Try again."* instead of specific `IO_ERROR` or `SCHEMA_ERROR` guidance.
  2. **`JSON_ERROR` copy misleads users when `profiles.json` is corrupted on disk:** `corrupt_profiles_json_error` (`crates/profile-store/src/store.rs:543-548`) emits `DimError::Json` (`error_code: "JSON_ERROR"`). In `vi-VN.json` and `en-US.json`, `JSON_ERROR` translates to *"Dữ liệu phản hồi không đúng định dạng. Hãy thử làm mới lại."* / *"Received an invalid data response. Please try refreshing."* — instructing the user to click Refresh, which can never fix a corrupted `profiles.json` file.
  3. **Dictionary key drift on `PATH_TRAVERSAL_REJECTED`:** `scripts/dim-host.ps1:383` and `apps/desktop/ui-server.mjs:171,177,287` return `error_code: "PATH_TRAVERSAL_REJECTED"`, whereas `crates/core/src/error.rs:154` and `vi-VN.json` / `en-US.json` define `"PATH_TRAVERSAL"`, causing `resolveErrorInfo()` to fall back to generic `errors.UNKNOWN`.
- **Requested Change:**
  1. Return `ApiEnvelope` (with `ok`, `error_code`, `message`, `data`) from `get_status`, `get_diagnostics`, and `verify_isolation` (or serialize `{ error_code, message }` on Tauri `Err`), and preserve `err.error_code` in `app.js:175-180`.
  2. Map `corrupt_profiles_json_error` to `DimError::SchemaError` (or add `CORRUPT_METADATA` to `DimError` + `vi-VN.json` / `en-US.json`) with plain-language recovery copy:
     - `vi-VN`: `"Tệp cấu hình danh sách tài khoản bị hỏng (dữ liệu gốc vẫn được giữ nguyên). Hãy mở Cài đặt → Kiểm tra sự cố hoặc khôi phục từ mục Sao lưu."`
     - `en-US`: `"Account list configuration file is damaged (your original data was kept safe). Open Settings → Troubleshoot or restore from Backups."`
  3. Add `"PATH_TRAVERSAL_REJECTED"` alias (matching `"PATH_TRAVERSAL"`) and `"INVALID_ID"` in `vi-VN.json` and `en-US.json`.

---

### `IR-QA-UX-03` (`P1`, `IR-NAT-01` — VERIFIED CLOSED IN `cd62c52`) — `ProfileLifecycleState::RecoveryRequired`, `Starting`, and `Stopping` UI Mapping

- **Initial Finding at `004ab7c`:** `commands.rs` exposed `lifecycle_state`, but `004ab7c` `app.js` ignored `prof.lifecycle_state`, rendering `recovery_required` profiles as `"Đã đóng"` / `"Closed"` and leaving Backup/Delete enabled until clicked.
- **Closure Verification at `cd62c52` (`bff0674`):**
  - `apps/desktop/ui/app.js` now implements `normalizeLifecycleState()` and `isProfileActiveOrUnclean()`, mapping `recovery_required` to `accounts.states.recoveryRequired` (`"Cần mở lại để khôi phục"` / `"Reopen to recover"`, `kind: 'error'`), mapping `starting`/`stopping` explicitly, and disabling offline Backup/Delete menu items whenever a profile is unclean.

---

### `IR-QA-UX-04` (`P2`, `OBS-RESTORE-CORRUPT-COPY`, OPEN) — Distinct Plain-Language Error When Restoring a Corrupted Backup & Visibility of Damaged Backup Folders

- **User Scenario:** Error Handling Scenario #7 (*Corrupted backup archive / tampered file / malformed `backup.json`*).
- **Evidence at `cd62c52`:**
  1. **Wrong direction in error copy during Restore:** `FileBackupManager::restore_backup_as_new_profile` (`crates/profile-store/src/backup.rs:293`) calls `self.verify_backup(backup_dir)?`, which returns `DimError::BackupError(...)` (`error_code: "BACKUP_ERROR"`) on missing files or SHA-256 hash mismatches (`backup.rs:249-282`), or `DimError::Json` (`"JSON_ERROR"`) on malformed `backup.json` (`backup.rs:225`). When the user clicks **Khôi phục / Restore**, `vi-VN.json` / `en-US.json` displays `"BACKUP_ERROR"`:
     - `vi-VN`: `"Không thể sao lưu khi Zalo đang mở hoặc thư mục đang bận."`
     - `en-US`: `"Cannot back up while Zalo is open or the folder is busy."`
     This blames an open Zalo window for a *backup creation* failure when the user actually attempted to *restore* a corrupted backup.
  2. **Silent skip in `list_backups`:** `commands.rs` silently `continue`s past any directory under `Backups/` whose `backup.json` is unreadable or malformed, causing a damaged backup to vanish from the Backups table without telling the user why.
- **Requested Change:**
  1. Map integrity/manifest failures inside `restore_backup_as_new_profile` to `DimError::RestoreError` (or `CORRUPT_BACKUP`) with clear restore-specific copy:
     - `vi-VN`: `"Không thể khôi phục vì bản sao lưu bị thiếu tệp hoặc dữ liệu không còn nguyên vẹn."`
     - `en-US`: `"Cannot restore because this backup is missing files or failed integrity verification."`
  2. Optionally return damaged non-`.partial` backup directories from `list_backups` with `status: "corrupted"` and a disabled Restore button so non-technical users know a backup folder on disk is damaged.

---

### `IR-QA-UX-05` (`P3`, `OBS-COLOR-PERSISTENCE`, OPEN) — Persist Account Color Badge in `profiles.json` Instead of Origin-Scoped `localStorage`

- **User Scenario:** Account Management Step #3 (*Choose distinguishing color*) & Daily Usage Invariant #5 (*Retain color badge*).
- **Evidence at `cd62c52`:**
  - `apps/desktop/ui/app.js` persists `colorId` in `window.localStorage['multi-zalo.accountColors']`.
  - Within a single origin (`http://tauri.localhost` in `dim-desktop.exe`, or a fixed port on `127.0.0.1`), colors persist across restarts. However, switching between Signed-Host Bridge (`http://127.0.0.1:18420`) and Native Tauri (`http://tauri.localhost`), changing `--port`, clearing WebView2 site storage, or restoring a backup on a new workstation resets user-chosen colors to deterministic hash defaults.
- **Requested Change:**
  - Add `#[serde(default)] pub color: Option<String>` to `ProfileRecord` (`crates/core/src/models.rs`) and `BackupManifest`, accepting optional `color` in `create_profile` / `rename_profile` while falling back to `localStorage` for backward compatibility.

---

### `IR-QA-UX-06` (`P3`, `IR-NAT-02` Closed + Copy Polish Open) — Actionable Recovery CTAs, 5 GB Disk Copy & ACL Row in Diagnostics

- **Closure Note (`IR-NAT-02`):** `DIM_NODE_FIXTURE_SCRIPT` support in `apps/desktop/src-tauri/src/commands.rs` (`get_status`, `launch_profile`, `focus_profile`, `verify_isolation`) is **VERIFIED CLOSED in `cd62c52`** (`bff0674`).
- **Remaining Open Copy / UI Polish:**
  1. **`INSUFFICIENT_DISK_SPACE` copy specificity (`vi-VN.json`, `en-US.json`):** State the 5 GB safety reserve directly in the primary message (*"Ổ đĩa cần tối thiểu 5 GB dung lượng trống cộng thêm kích thước dữ liệu tài khoản."* / *"At least 5 GB of free disk space (plus account data size) is required."*) so users know how much space to free without expanding Technical Details.
  2. **Render `profiles_acl_safe` in Diagnostics (`app.js`):** `commands.rs` computes `profiles_acl_safe`, but `renderDiagnostics()` omits it from `#diag-security-list`. Add a plain-language row (*"Quyền thư mục dữ liệu" / "Profile folder permissions"*).
  3. **Localize `platform_support_matrix` strings (`commands.rs`):** Move hardcoded English `strategy` and `limits` strings in the OS readiness table to `i18n/{vi-VN,en-US}.json`.
  4. **One-click recovery buttons on banners:** Add a **"Kiểm tra lại" / "Check again"** button inside `#zalo-install-alert` and, when `DUPLICATE_ZALO_USER_ID` identifies a `running_profile`, offer a **"Chuyển đến cửa sổ đang mở" / "Switch to open window"** button.

---

### `IR-QA-UX-07` (`P2`, `OBS-PS1-ACL-ERROR-CODE`, OPEN) — Signed-Host PowerShell `UnauthorizedAccessException` -> `IO_ERROR` Parity

- **User Scenario:** Error Handling Scenario #3 (*Insufficient directory permissions / NTFS write-deny ACL on `<data_dir>` under Signed-Host Bridge*).
- **Empirical Evidence (`journeys-and-errors.mjs` `ERR-03` on `cd62c52`):**
  - When NTFS write permissions are denied on `<data_dir>`, native Rust `dim-cli.exe` / `dim-desktop.exe` cleanly emits `error_code: "IO_ERROR"` (*"Không thể đọc hoặc ghi dữ liệu trên ổ đĩa. Hãy kiểm tra dung lượng trống và quyền truy cập."*).
  - By contrast, `scripts/dim-host.ps1` catches `System.UnauthorizedAccessException` in its top-level fallback and emits `error_code: "UNHANDLED_EXCEPTION"`, causing `resolveErrorInfo()` in `app.js` to fall back to generic `errors.UNKNOWN` (*"Đã xảy ra sự cố..."*).
- **Requested Change:**
  - Map `UnauthorizedAccessException` and `IOException` inside `scripts/dim-host.ps1`'s CLI error serializer directly to `"IO_ERROR"` so Bridge users on SAC-enforced Windows 11 hosts receive the same plain-language permission guidance as Native Tauri users.

---

### `IR-QA-UX-08` (`P3`, `IR-NAT-03`, OPEN) — OS Text-Scale Accessibility (`rem` Root Typography & `meta[name="text-scale"]`)

- **User Scenario:** Section 3 Check #10 (*Windows OS Accessibility Text Scaling alongside DPI Scaling*).
- **Empirical Evidence (`native-tauri-smoke.mjs` `NT-10`, Modern Web Guidance `respect-os-text-scale`):**
  - `apps/desktop/ui/styles.css:223` sets `html, body { font-size: 14px; }`. While Windows DPI scaling (`100%`–`200%`) and `200%` page zoom reflow cleanly with `0 px` horizontal overflow, a hardcoded pixel `font-size` on `html` overrides browser/WebView2 OS text-scale inheritance (`Settings -> Accessibility -> Text size`).
- **Requested Change:**
  - Remove fixed `px` `font-size` from `html`, add `<meta name="text-scale" content="scale">` in `apps/desktop/ui/index.html`, and express typography tokens in `rem` (`0.875rem` base body text) so OS-level accessibility font scaling works seamlessly alongside Windows DPI scaling.
