# Multi-Zalo Desktop — Quiet Workspace UI & vi-VN/en-US Localization Report

## 1. Scope & Workspace Compliance
- **Branch**: `feat/ui-vi-en-quiet-workspace` (worktree `<WORKTREE_ROOT>/zalo-ui-worktree/desktop-instance-manager/`)
- **Modified / Created Files**:
  - `apps/desktop/ui/index.html`
  - `apps/desktop/ui/styles.css`
  - `apps/desktop/ui/app.js`
  - `apps/desktop/ui/theme-init.js`
  - `apps/desktop/ui/i18n/index.js`
  - `apps/desktop/ui/i18n/vi-VN.json`
  - `apps/desktop/ui/i18n/en-US.json`
  - `apps/desktop/ui/tests/i18n.test.mjs`
  - `apps/desktop/ui/tests/ui-contracts.test.mjs`
  - `apps/desktop/ui/tests/visual-cdp.mjs`
  - `docs/design/ui-i18n-implementation-report.md`
- **Untouched Backend / Security / CI**: Zero changes to `crates/**`, `scripts/dim-host.ps1`, `apps/desktop/src-tauri/**`, `apps/desktop/ui-server.mjs`, or `.github/workflows/**`.

## 2. Quiet Workspace UI/UX Architecture
- **Task-First Navigation Shell**: Left sidebar (`216px` on desktop, compact horizontal nav at `<= 768px`) with 4 sections:
  1. `Hồ sơ Zalo` (`profiles` — default)
  2. `Sao lưu` (`backups`)
  3. `Chẩn đoán` (`diagnostics`)
  4. `Cài đặt` (`settings`)
- **Zero Dashboard Clutter**: Removed top KPI summary cards, gradients, glassmorphism, neon/heavy shadows, and emoji icons.
- **Profiles View**:
  - 1-line status bar (`Zalo Desktop đã phát hiện · X hồ sơ đang chạy` or `Chưa tìm thấy Zalo Desktop...`), plus primary Zalo lock status.
  - Search input (`#profile-search`), `Đo dung lượng đĩa` (`#toggle-sizes`), and primary `Tạo hồ sơ` CTA.
  - Clean profile list rows with name, state badge, primary action (`Mở cửa sổ` / `Mở Zalo`), secondary `Dừng` when running, and an overflow menu (`Đổi tên`, `Sao lưu`, `Chi tiết kỹ thuật`, `Xóa hồ sơ`).
  - Technical details (ID, directory, PID, process count, window title, RAM, disk size, stale lockfile, timestamps, notes) are tucked into progressive disclosure (`#profile-details-dialog`).
- **Backups View**: Lists backups with source profile, file count & size, created timestamp, SHA-256 verification readiness badge, `Khôi phục thành hồ sơ mới` action, and a prominent non-destructive notice stating restore always creates a new profile.
- **Diagnostics View**: Grouped into Application, Security & Isolation, Profiles, Environment & OS Readiness, and Sanitized Logs. `NOT_TESTED`, `BLOCKED`, and `UNKNOWN` statuses are preserved verbatim and never converted into `PASS`.
- **Settings View**: Instant `vi-VN` / `en-US` language switcher (persisted in `localStorage`, updates `document.documentElement.lang`), 3-state appearance selector (`system`, `light`, `dark` with live system preference label), and application/connection metadata.

## 3. Modern Web Guidance & CSP Compliance
- **Theme & FOUC Prevention**: `<meta name="color-scheme" content="light dark">` followed by synchronous external `<script src="./theme-init.js"></script>` (CSP `script-src 'self'` compliant, no inline scripts). CSS uses `color-scheme: light dark`, `:root:has(> head > meta[name="color-scheme"][content=...])`, `[data-theme]`, `light-dark()`, and `@media (prefers-color-scheme: dark)` fallback.
- **Native `<dialog>` Modals**: All modals (`create-profile-dialog`, `rename-profile-dialog`, `delete-profile-dialog`, `restore-backup-dialog`, `profile-details-dialog`, `safety-dialog`) use native `<dialog>` + `showModal()`, without `role="dialog"`, `aria-modal`, manual focus traps, or manual `Esc` handlers. `autofocus` is placed on the first form field or the safe `Cancel` button in `delete-profile-dialog`. Every `close` listener resets `dialog.returnValue = ''`. CSS includes `dialog:modal { overscroll-behavior: contain; }` and `dialog::backdrop { overflow: hidden; overscroll-behavior: contain; }`.
- **Security**: Zero `innerHTML` usage with backend/user data (all DOM built with `document.createElement` + `textContent`), zero remote fonts/CDNs/telemetry.

## 4. i18n (`vi-VN` & `en-US`) Implementation
- Default locale: `vi-VN` with full Vietnamese diacritics and natural desktop phrasing.
- Complete `en-US` parity verified by automated tests.
- `t(key, params)` with fallback (`active -> en-US -> vi-VN -> key`), missing-key tracking, and plain-text `{param}` interpolation.
- Locale-aware `Intl.NumberFormat` and `Intl.DateTimeFormat` for numbers, byte sizes, and timestamps.
- Maps backend `error_code` values and `SupportStatus` enums (both `SCREAMING_SNAKE_CASE` and Rust `PascalCase`) while preserving raw codes and keeping user profile names, paths, PIDs, and raw logs untranslated.

## 5. Backend Contract Preservation
- `getTauriInvoke()` and `apiCall(route, method, body, tauriCmd, tauriArgs)` are preserved unchanged.
- All 12 Tauri commands (`get_status`, `create_profile`, `rename_profile`, `delete_profile`, `launch_profile`, `focus_profile`, `stop_profile`, `backup_profile`, `list_backups`, `restore_backup`, `get_diagnostics`, `verify_isolation`) and HTTP routes (`/api/status`, `/api/backups`, `/api/diagnostics`, `/api/verify-isolation`, `/api/profiles`, `/api/backups/restore`, `/api/profiles/:id/*`) are preserved.

## 6. Verification & Test Results
### Automated Unit & Contract Tests (`node --test`)
Command:
```sh
node --test apps/desktop/ui/tests/i18n.test.mjs apps/desktop/ui/tests/ui-contracts.test.mjs
```
Result: **14 passed, 0 failed, 0 skipped** (`duration_ms 98.2587`):
- `i18n.test.mjs` (9/9 passed): default `vi-VN`, 100% `vi-VN`/`en-US` key parity, Vietnamese diacritics check, static key coverage in `index.html` & `app.js`, safe interpolation, missing-key detection, `SupportStatus` normalization, `error_code` preservation, `Intl` formatters.
- `ui-contracts.test.mjs` (5/5 passed): HTML/CSS Modern Web Guidance & CSP compliance, Quiet Workspace rules (no gradients/glassmorphism/remote URLs), Tauri/HTTP contract & no-`innerHTML` checks, `resolveIsolationCheckResult` honesty (`NOT_TESTED`/`BLOCKED`/`UNKNOWN` never mapped to `PASS`), locale persistence.

- **Headless Edge CDP E2E & Visual Matrix** (`node apps/desktop/ui/tests/visual-cdp.mjs`):
  - Result: **`PASS (16 matrix combinations, WAI-ARIA menu, native radios, async pending, error banner, dialogs, 200% zoom, 0 runtime errors)`**
  - Screenshots captured under `docs/design/screenshots/` across viewports (`1160x780`, `900x650`, `768x650`, `420x540`), `vi-VN`/`en-US`, `light`/`dark`, 200% zoom, dialogs, and error states.
  - See [ui-independent-review-and-remediation.md](./ui-independent-review-and-remediation.md) for the full hardening audit and WebView compatibility matrix.
