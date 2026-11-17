# Multi-Zalo Desktop — Account Dock Consumer UX Redesign

## 1. Baseline & Scope
- **Baseline Commits:** `b1446d6` (hardened Quiet Workspace) -> `cc74584` (initial Account Dock redesign) -> `4904e6f4bd48ba8bc747b522edf77efa0e4d85a3` (`4904e6f`) -> `fc91901ebc897c62630c0cfeeebaaed0fd8c469c` (`fc91901`, post-review Account Dock hardening & layout polish on `feat/ui-vi-en-quiet-workspace`)
- **Worktree:** `<ui-worktree>/desktop-instance-manager/` (Target `main` baseline: `a5feb4a` at `<repo-root>/desktop-instance-manager/`)
- **Scope:** Strictly `apps/desktop/ui/**` and `docs/design/**`. Zero modifications to Rust core (`crates/**`), Tauri commands (`apps/desktop/src-tauri/**`), PowerShell bridge (`scripts/dim-host.ps1`), `apps/desktop/ui-server.mjs`, or `.github/workflows/**`.

## 2. Why Quiet Workspace Failed Consumer UX & Why Account Dock Solves It
| Quiet Workspace (`b1446d6`) Issue | Account Dock Redesign (Post-Review Final) |
| :--- | :--- |
| Left 216px admin sidebar (`Hồ sơ Zalo`, `Sao lưu`, `Chẩn đoán`, `Cài đặt`) + bottom loopback status | Compact top bar (`Multi-Zalo` brand + open window count, `<nav>` with 4 views: `Tài khoản`, `Sao lưu`, `Cài đặt`, `Trợ giúp`, and contextual `.topbar-actions` `+ Thêm tài khoản` on Home) |
| Technical jargon (`PID`, `sandbox`, `IPC`, `Authenticode`, `loopback`) on Home | Zero technical jargon on consumer Home/Help/Create flows; technical checks nested under `Cài đặt → Nâng cao → Kiểm tra sự cố` (`#view-diagnostics`) and `Thông tin nâng cao` |
| Plain text profile rows with weak visual identity | 2-column `.account-grid` (`role="list"`, 1-column `<720px`) with letter-initial avatars + WCAG AA 8-color identity palette (`>= 5.02:1` against `#ffffff`) |
| Single-step create dialog closing on failure | 2-step onboarding: Step 1 (name + quick chips + collision-avoiding color + inline `#create-step1-error`) -> Step 2 (`"Không gian \"{name}\" đã sẵn sàng"` + `"Mở Zalo để đăng nhập"`) |
| Technical primary Zalo note (`1 PID được bảo vệ`) | Always-visible calm read-only card (`#primary-zalo-section`) with shape+color `.state-dot` for `running` / `stopped` / `unknown` |
| Help trapped inside a modal (`#safety-dialog`) | Dedicated `#view-help` (`Hướng dẫn sử dụng` / `Quick guide`) with 4 consumer cards + direct `"Mở Kiểm tra sự cố"` drilldown |

## 3. Information Architecture (Before → After)
- **Before:** `[Sidebar: Profiles | Backups | Diagnostics | Settings]` -> Flat list of profiles + 4-section admin layout.
- **After:**
  - **Top Bar (`.topbar`):** Brand home (`#btn-brand-home`), `<nav class="topbar-nav">` (`Tài khoản`, `Sao lưu`, `Cài đặt`, `Trợ giúp`), and `.topbar-actions` (`#btn-open-create`, visible on Home only).
  - **Home (`#view-profiles`):** `#home-toolbar` (`#accounts-count-summary`, `#search-wrap` shown for `>= 2` accounts with `Escape` clear, `#btn-refresh`), `.account-grid` with `.account-actions-primary` / `.account-actions-secondary` + WAI-ARIA `⋯` overflow menu (with `"Đóng Zalo trước"` hints), and read-only `#primary-zalo-section`.
  - **Secondary Views:** `Sao lưu` (`#view-backups`), `Cài đặt` (`#view-settings`), `Trợ giúp` (`#view-help`).
  - **Advanced (`Cài đặt → Nâng cao → Kiểm tra sự cố`):** `#view-diagnostics` (isolation checks, OS readiness matrix, disk size measurement toggle, and filtered logs).

## 4. Design Tokens & Component Inventory
- **Tokens:** Spacing `4/8/12/16/24/32px`, `color-scheme: light dark`, `:has(> head > meta[name="color-scheme"])` sync, `light-dark()` + `@media (prefers-color-scheme: dark)` fallback, synchronous `theme-init.js` reading `multi-zalo.theme` (fallback `color-scheme`), and WCAG 2.2 AA 8-color identity palette (`--id-blue: #1d4ed8`, `--id-teal: #0f766e`, `--id-green: #15803d`, `--id-amber: #b45309`, `--id-coral: #be123c`, `--id-plum: #7e22ce`, `--id-indigo: #4338ca`, `--id-slate: #475569`).
- **Non-Color & Forced-Colors Cues:** Distinct `.state-dot` shapes/borders (`running` solid dot, `stopped` hollow ring, `pending` hollow square, `error` rotated diamond, `unknown` dashed ring), `✓` inside checked `.color-swatch-dot`, and `@media (forced-colors: active)` overrides.
- **Components:** `.topbar` (unsticks on `@media (max-width: 719px), (max-height: 520px)` for 200% zoom), `.account-grid`, `.account-card`, `.account-avatar`, `.primary-zalo-card`, `.overflow-menu` (WAI-ARIA `role="menu"` with upward edge flip), `.help-grid`, `.empty-hero`, `.segmented-control` (`<fieldset>`/`<legend>` + native `<input type="radio">`), `.color-palette` (Create + Rename), native `<dialog>` modals (`showModal()`), and semantic tables (`<caption class="visually-hidden">` + `<th scope="row">`).

## 5. UX Writing & Localization (`vi-VN` & `en-US`)
- `Hồ sơ Zalo` -> `Tài khoản Zalo` / `Tài khoản`
- `Tạo hồ sơ` -> `Thêm tài khoản`
- `Mở cửa sổ` -> `Chuyển đến` (`focusAria`: `Chuyển đến — {name}`)
- `Dừng` -> `Đóng Zalo` (`stopAria`: `Đóng Zalo — {name}`)
- `Chẩn đoán hệ thống` -> `Kiểm tra sự cố`
- `Sao lưu ngoại tuyến` -> `Sao lưu`
- `Chi tiết kỹ thuật hồ sơ` -> `Thông tin tài khoản` (adds `Màu nhận diện`)
- `Đang mở` -> `Cửa sổ đang mở` (distinguishes running window from in-app Zalo login)
- **Full Error Parity:** All 27 backend/bridge/UI error codes + 2 legacy aliases localized with clean consumer wording and collapsed `▶ Chi tiết kỹ thuật`.

## 6. Developer Journey & Visual Review Evidence
All journeys below are **developer journey tested** via automated headless Edge CDP + mock bridge (`apps/desktop/ui/tests/visual-cdp.mjs`):
- **Journey A (Empty State & 2-Step Onboarding):** Empty hero, Step 1 inline `#create-step1-error` recovery, Step 2 (`"Không gian \"{name}\" đã sẵn sàng"` + `"Mở Zalo để đăng nhập"`), and post-close focus restoration.
- **Journey B (Account Grid, Search & Protection Card):** 2-column grid, 6+ accounts count summary, search filter + `Escape` clear, and honest `running` / `stopped` / `unknown` Primary Zalo card.
- **Journey C (Error & Bridge Auth Handling):** Protected Primary Zalo banner with collapsed `▶ Chi tiết kỹ thuật`, `X-DIM-Session-Token` enforcement, `401 UNAUTHORIZED`, `AbortController` `HOST_TIMEOUT`, and `statusData.ok === false` guard.
- **Journey D (Account Management):** Overflow menu WAI-ARIA keyboard nav + disabled `"Đóng Zalo trước"` hints + bottom-edge upward flip, Rename + color edit, Details color display, Backup + non-destructive Restore color inheritance, Delete + `{ id, confirm: true }` + focus restoration.
- **Journey E (Help, Settings, Diagnostics, Theme & Zoom):** `#view-help` -> `#btn-help-diagnostics`, `vi-VN`/`en-US` live switch, `light`/`dark` reload persistence via `theme-init.js`, `420x540` mobile, and `580x390` (200% zoom).
- **Pre-Fix `cc74584` & `4904e6f` Defects Observed and Fixed (`4904e6f` + `fc91901`):** Topbar mixed actions into `<nav>` and kept `+ Thêm tài khoản` visible outside Home; sticky `actionSuccess` banner leaked across view changes; 200% zoom sticky topbar consumed ~170px; `teal`/`green`/`amber` failed WCAG 4.5:1 contrast; state dots/swatches lacked non-color shapes; disabled menu items lacked hints; Step 1 errors closed the create modal; `theme-init.js` read `'color-scheme'` instead of `'multi-zalo.theme'`; `apiCall()` / `delete_profile` lacked `X-DIM-Session-Token` / `confirm: true`; and (`D-14` in `fc91901`) `#home-toolbar` `#btn-refresh` wrapped under `#search-wrap` while `.badge` wrapped `"Sẵn sàng khôi phục"`. All 20 regenerated PNGs under `docs/design/screenshots/` were visually inspected post-fix and confirmed single-row toolbar alignment across `420px`–`1160px`, single-line status badges, clean left/right card footer separation, `"Đóng Zalo trước"` disabled menu hints, high-contrast avatars + active `✓` swatches, non-sticky 2-row topbar on `420x540` & `zoom-200`, 4-card `#view-help`, and 20-account search + `"Hiển thị 20 / 20 tài khoản"` count summary.

## 7. Test Results
- `node --check apps/desktop/ui/app.js apps/desktop/ui/theme-init.js apps/desktop/ui/i18n/index.js apps/desktop/ui/tests/i18n.test.mjs apps/desktop/ui/tests/ui-contracts.test.mjs apps/desktop/ui/tests/visual-cdp.mjs`: **exit `0`** (all 6 JS files syntactically valid).
- `node --test apps/desktop/ui/tests/i18n.test.mjs apps/desktop/ui/tests/ui-contracts.test.mjs`: **exit `0`** — **`tests 20 | pass 20 | fail 0 | skipped 0`** (`12` i18n tests including `271/271` parity, consumer jargon check, and all 22 `DimError::code()` + 4 bridge error codes + `UNKNOWN` fallback; `8` contract tests including ID-keyed color persistence, 8-way collision avoidance, WCAG 2.2 AA `>= 4.5:1` luminance check across all 8 `--id-*` hex colors, `delete_profile` `confirm: true`, `X-DIM-Session-Token`, and `theme-init.js` `multi-zalo.theme`).
- `node apps/desktop/ui/tests/visual-cdp.mjs`: **exit `0`** — **`PASS (Account Dock: 16 matrix combinations, session token, theme reload, scale 0/1/3/10/20, Step1/Step2 resilience, color persistence/restore, primary Zalo honest states, timeout, Tauri invoke, Help view, WAI-ARIA menu, out-of-order status, 200% zoom, 0 runtime errors)`** across **20** PNG screenshots under `docs/design/screenshots/`.

## 8. Backend Handoff, `NOT_TESTED` Platforms & Boundaries
- **Backend Handoff & Merge Guide:** See [`account-dock-backend-handoff.md`](./account-dock-backend-handoff.md) for the complete 12-command Tauri IPC table, HTTP bridge routes + `X-DIM-Session-Token`, all 27 error codes, resolved `INTEGRATION_BLOCKER`s, and 3-file merge guidance (`index.html`, `styles.css`, `app.js` + `Test-8` adaptation in `tests/run-fixture-tests.mjs`).
- **Final Acceptance:** See [`account-dock-final-acceptance.md`](./account-dock-final-acceptance.md) (Verdict: **`READY_WITH_LIMITATIONS`**).
- **`NOT_TESTED` Platforms:** Live Windows Tauri WebView2 + Rust core build, live `ui-server.mjs` + `dim-host.ps1`, macOS WKWebView, and Ubuntu WebKitGTK remain `NOT_TESTED` in this UI-only worktree.
- **Usability Boundary:** All journeys A–E are **developer journey tested**; real consumer usability testing with non-technical users has not yet been conducted.
