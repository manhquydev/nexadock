# Multi-Zalo Desktop — Account Dock Final Acceptance Report

## 1. Baseline, Scope & Commit Lineage
- **Branch:** `feat/ui-vi-en-quiet-workspace` (`<ui-worktree>/desktop-instance-manager/`)
- **Target Main Baseline:** `a5feb4a` (`<repo-root>/desktop-instance-manager/`, common ancestor `8cf4703`)
- **UI Evolution Lineage:**
  - `39ff5c7`: Quiet Workspace redesign & `vi-VN`/`en-US` localization
  - `2b1ce6e`: Independent UI review remediation (a11y, async guards, WebView compat, visual QA)
  - `b1446d6`: i18n fetch resilience, out-of-order refresh guard, open menu preservation
  - `cc74584`: Account Dock consumer UX redesign
  - `4904e6f4bd48ba8bc747b522edf77efa0e4d85a3` (`4904e6f`): Post-review Account Dock UX, accessibility, security-contract & test remediation (Commit 1: 29 files changed vs `cc74584`, `1666 insertions(+), 247 deletions(-)`)
  - `c7cf9766a963af74c3306f4304605b6509fdec60` (`c7cf976`): Initial Account Dock final acceptance & backend handoff docs (Commit 2)
  - `fc91901ebc897c62630c0cfeeebaaed0fd8c469c` (`fc91901`): Home search toolbar single-row alignment & single-line status badge polish + regenerated screenshots (Commit 3: 19 files: `styles.css` + 18 PNGs, `20 insertions(+), 3 deletions(-)`)
  - Final documentation update commit (Commit 4)
- **Modified Scope:** Strictly `apps/desktop/ui/**` and `docs/design/**`. Zero changes to `crates/**`, `apps/desktop/src-tauri/**`, `apps/desktop/ui-server.mjs`, `scripts/**`, or `.github/workflows/**`.
- **Privacy & Path Hygiene:** Zero personal machine paths (absolute OS user-home paths) or personal phone numbers included in UI docs; all workspace paths use `<ui-worktree>/desktop-instance-manager/` and `<repo-root>/desktop-instance-manager/`.

---

## 2. Executive Verdict & Acceptance Status
- **Final Verdict:** **`READY_WITH_LIMITATIONS`**
- **Rationale:** All post-review UX, accessibility (WCAG 2.2 AA), localization (`vi-VN`/`en-US` parity + all 27 error codes), backend contract compatibility (`X-DIM-Session-Token`, `delete_profile` `confirm: true`, `AbortController` `32000ms` timeout, `statusData.ok === false` guard), and browser E2E visual checks passed against the hardened CSP-enforced mock bridge (`20/20` unit/contract tests PASS; `visual-cdp.mjs` PASS across 16 matrix checks, journeys A–E, and 20 inspected PNGs at final UI code commit `fc91901`). Live native Windows Tauri WebView2 + Rust core execution, live `ui-server.mjs` + `dim-host.ps1` execution, physical macOS WKWebView, physical Ubuntu WebKitGTK, and real end-user usability testing remain `NOT_TESTED` in this UI-only worktree.

---

## 3. Architecture & Information Architecture (Account Dock Final Form)
- **Top Application Bar (`.topbar`):**
  - Left: Brand home button (`#btn-brand-home`, dynamic open-window counter `#topbar-open-summary`) + `#connection-pill` (shown only when disconnected).
  - Center (`<nav class="topbar-nav" aria-label="Chính" / "Primary">`): Strictly the 4 view tabs — `Tài khoản` (`profiles`), `Sao lưu` (`backups`), `Cài đặt` (`settings`), and `Trợ giúp` (`#btn-nav-help`, `data-view="help"`).
  - Right (`.topbar-actions`): Contextual primary CTA `+ Thêm tài khoản` (`#btn-open-create`), visible on Home (`profiles`) and hidden on secondary views (`backups`, `settings`, `diagnostics`, `help`).
- **Home (`#view-profiles`):**
  - `#home-toolbar`: `#accounts-count-summary` (shown when `>= 6` profiles or active search), `#search-wrap` (shown when `>= 2` profiles, supports `Escape` clear), and `#btn-refresh` (`Làm mới`).
  - `#profiles-list` (`.account-grid`, `role="list"`, 2 columns on `>= 720px`, 1 column on `< 720px`): Account cards with letter-initial avatar in 8 WCAG AA colors, window state row with shape+color `.state-dot`, optional note, `.account-actions-primary` (`Mở Zalo` / `Chuyển đến`), and `.account-actions-secondary` (`Đóng Zalo` + WAI-ARIA `⋯` overflow menu).
  - `#primary-zalo-section`: Always-visible read-only protected Primary Zalo card (`running` / `stopped` / `unknown`).
- **Secondary & Nested Views:** `Sao lưu` (`#view-backups`), `Cài đặt` (`#view-settings`), `Hướng dẫn sử dụng` (`#view-help`, 4 plain-language cards + `#btn-help-diagnostics`), and `Kiểm tra sự cố` (`#view-diagnostics`, nested under `Cài đặt -> Nâng cao` and linked from Help).
- **Modals:** Native `<dialog>` (`showModal()`) for Add Account (2-step onboarding with `#create-step1-error`), Rename Account (with `#rename-color-palette`), Remove Account (`value="cancel" autofocus`), Restore Backup, and Account Info (`#profile-details-dialog` with account color + expandable `Thông tin nâng cao`).

---

## 4. Defects Observed in `cc74584` Screenshots & Post-Review Remediation Inventory

Visual inspection of the 18 `cc74584` PNG screenshots (plus follow-up inspection of `4904e6f`) uncovered the following defects, all remediated across `4904e6f` and `fc91901ebc897c62630c0cfeeebaaed0fd8c469c` (`fc91901`):

| ID | Observed Defect in `cc74584` (Screenshots / Audit) | Remediation Implemented |
|---|---|---|
| `D-01` | **Topbar nav mixed actions & views** (`account-dock-empty-vi-light.png`, `account-dock-settings-vi-light.png`): `Làm mới` and `Thêm tài khoản` sat inside `<nav>` and remained visible on Settings/Backups/Diagnostics. | Separated `<nav class="topbar-nav">` (4 views only) from `.topbar-actions` (`#btn-open-create`, hidden outside `profiles`); moved `#btn-refresh` into `#home-toolbar`. |
| `D-02` | **200% zoom sticky topbar consumed viewport** (`account-dock-zoom-200-vi-light.png`, `580x390`): sticky `.topbar` wrapped to 3 rows (~170px), leaving minimal vertical space for content. | Added `@media (max-width: 719px), (max-height: 520px)` unsticking `.topbar` (`position: static`) and tightening `.topbar-inner` padding/gap. |
| `D-03` | **Sticky success banner persisted across view changes & actions** (`account-dock-3accounts-vi-light.png`, `backups`, `settings`, `diagnostics`): `"Đã hoàn tất thao tác."` remained stuck across `launch`/`focus`/`stop` and when navigating tabs. | Suppressed sticky `messages.actionSuccess` banner on `launch`/`focus`/`stop` (kept `messages.backupCreated`); `selectView()` clears non-error banners on navigation. |
| `D-04` | **WCAG 2.2 AA contrast failure on avatar & swatch palette** (`account-dock-vi-VN-dark-1160x780.png`, `account-dock-3accounts-vi-light.png`): `teal` (`#0d9488`, `3.74:1`), `green` (`#16a34a`, `3.30:1`), and `amber` (`#d97706`, `3.19:1`) failed `4.5:1` with white text. | Darkened palette tokens in `styles.css` (`--id-blue: #1d4ed8` `6.70:1`, `--id-teal: #0f766e` `5.47:1`, `--id-green: #15803d` `5.02:1`, `--id-amber: #b45309` `5.02:1`, `--id-coral: #be123c` `6.29:1`, `--id-plum: #7e22ce` `6.98:1`, `--id-indigo: #4338ca` `7.90:1`, `--id-slate: #475569` `7.58:1`). |
| `D-05` | **Color-only status dots & swatches** (`account-dock-3accounts-vi-light.png`, `account-dock-add-step1-vi-light.png`): `.state-dot` differed only by color; selected color swatch had only a border ring; no `forced-colors` rules. | Added distinct shapes/borders per state (solid dot `running`, hollow ring `stopped`, hollow square `pending`, rotated diamond `error`, dashed ring `unknown`), white `✓` checkmark on checked `.color-swatch-dot`, and `@media (forced-colors: active)` rules. |
| `D-06` | **Disabled overflow menu items lacked reason hint & upward flip** (`account-dock-menu-vi-light.png`): disabled `Sao lưu` / `Xóa tài khoản` gave no hint why disabled, and `positionMenu()` did not flip upward near bottom edges. | Added `.menu-item-hint` (`Đóng Zalo trước` / `Close Zalo first`) on disabled backup/delete items and upward flip / clamped positioning in `positionMenu()`. |
| `D-07` | **Card footer button grouping & ARIA labels** (`account-dock-3accounts-vi-light.png`): `Chuyển đến` and `Đóng Zalo` sat side-by-side in `.account-actions-left` without separating primary vs secondary/destructive controls. | Split footer into `.account-actions-primary` (left: `Mở Zalo` / `Chuyển đến`) and `.account-actions-secondary` (right: `Đóng Zalo` + `⋯`) with per-account `aria-label`s (`{launch,focus,stop}Aria`). |
| `D-08` | **Search bar shown for 1 account & missing count summary** (`account-dock-add-step2-vi-light.png`): `#search-wrap` appeared with only 1 profile (`Bán hàng`). | Hid `#search-wrap` when `allProfiles.length < 2`; added `#accounts-count-summary` when `allProfiles.length >= 6` or search active; added `Escape` clear on `#profile-search`. |
| `D-09` | **Add Account Step 1 errors closed the modal & lacked submitting state** (`cc74584` `app.js`): API errors closed `#create-profile-dialog` and dumped error on Home banner; color picker always defaulted to `'blue'`. | Added inline `#create-step1-error` keeping modal open on Step 1 with inputs intact and focusing `#create-profile-name`; added submitting busy label; added `pickNextAvailableColorId()` collision avoidance. |
| `D-10` | **Color editing, deletion cleanup, restore inheritance & details row missing** (`account-dock-details-vi-light.png`): colors could not be changed in Rename, persisted in `localStorage` after Delete, were lost on Restore, and weren't shown in Details. | Added `#rename-color-palette`, `removeAccountColor()`, restore `source_profile_id` color inheritance, and `accounts.details.color` row in `#profile-details-list`. |
| `D-11` | **Theme reload FOUC & missing Primary Zalo `unknown` state** (`theme-init.js`, `app.js`, `index.html`): `theme-init.js` read `'color-scheme'` while `app.js` saved `'multi-zalo.theme'`; Primary Zalo card hid on missing status or omitted `.state-dot`. | Fixed `theme-init.js` + `initApp()` + `storage` listener; wrapped `#primary-zalo-desc` in `#primary-zalo-state-row` and always displayed Primary Zalo card (`running` / `stopped` / `unknown`). |
| `D-12` | **Help lived in modal (`#safety-dialog`) & tables lacked `<caption>` / `<th scope="row">`** (`account-dock-backups-vi-light.png`, `diagnostics`): limited scanning & screen-reader semantics. | Replaced `#safety-dialog` with dedicated `#view-help` (4 consumer guide cards + `#btn-help-diagnostics`), added `<caption class="visually-hidden">` on all 3 tables, and `<th scope="row">` on tbody rows. |
| `D-13` | **Backend Integration Blockers (`X-DIM-Session-Token`, `delete_profile` `confirm: true`, `statusData.ok === false`, 11 missing error keys)** (`cc74584` `app.js`, `i18n/*.json`). | Fixed `apiCall()` (`X-DIM-Session-Token`, `32000ms` timeout, disconnect badge), `delete_profile` (`{ id, confirm: true }`), `refreshAll()` error guard, and added all 11 missing backend/bridge error keys. |
| `D-14` | **Toolbar wrapping & badge pill wrapping observed in `4904e6f`** (`account-dock-3accounts-vi-light.png`, `20accounts`, `420x540`, `backups`): `#btn-refresh` wrapped onto a second line below `#search-wrap`, and `.badge` `"Sẵn sàng khôi phục"` wrapped inside its pill. | Added `.page-header > div:first-child { flex: 1 1 280px; min-width: 0; }`, `#home-toolbar { flex: 0 1 auto; }` (`flex-wrap: nowrap` at `>= 720px`), `.header-search-wrap { width: 210px; min-width: 160px; max-width: 240px; flex: 1 1 180px; }`, and `.badge { white-space: nowrap; }` in `fc91901`. |

---

## 5. Localization (`vi-VN` & `en-US`) & Consumer Copy Compliance
- **Default Locale:** `vi-VN`; **Secondary Locale:** `en-US` (persisted via `localStorage['multi-zalo.locale']`, updating `document.documentElement.lang`).
- **Key & Placeholder Parity:** `271` keys in `vi-VN.json` and `271` keys in `en-US.json` (`errors` holds 32 keys: 3 banner metadata labels + 27 active error codes + 2 legacy aliases) with 100% key parity and `{placeholder}` parity across `vi-VN.json` and `en-US.json`.
- **Consumer Copy Jargon Guard:** Verified zero occurrences of `\b(PID|sandbox|IPC|Authenticode|loopback)\b` across consumer-facing dictionary sections (`app`, `nav`, `statusBar`, `accounts.states`, `accounts.actions`, `accounts.primaryZalo`, `dialogs.create`, `dialogs.help`, `help`).
- **Error Coverage:** All 27 backend/bridge/UI error codes + 2 legacy aliases mapped with clean localized `userMessage` and collapsed `#status-banner-details` (`rawCode` + `rawDetail`).
- **Resilience:** `initI18n()` uses `fetch()` with `8000ms` `AbortController` timeout + `Promise.allSettled`, never overwriting static Vietnamese HTML with raw keys if loading fails.

---

## 6. Accessibility (WCAG 2.2 AA & WAI-ARIA) Verification
- **Color Contrast & Non-Color Cues:** All 8 identity colors `>= 5.02:1` against `#ffffff`; `.state-dot` uses distinct shapes/borders; selected `.color-swatch-dot` renders `✓`; `@media (forced-colors: active)` preserves swatch fidelity and indicator contrast.
- **Landmarks & Navigation:** Skip link (`nav.skipToMain` -> `#main-content`), `<nav class="topbar-nav">` with `aria-current="page"`, dynamic `document.title` per view, `role="list"` on `#profiles-list`.
- **Tables & Forms:** `<caption class="visually-hidden">` on all 3 tables, `<th scope="row">` on identifying cells, native `<fieldset>`/`<legend>`/`<input type="radio">` in Settings and Create/Rename color palettes, `role="alert"` on `#create-step1-error`.
- **Dialogs & Menus:** Native `<dialog>` + `showModal()` (zero redundant `role="dialog"` / `aria-modal`), `autofocus` on cancel in `#delete-profile-dialog`, focus restoration on Create/Delete close, WAI-ARIA `role="menu"` keyboard navigation (`Enter`, `Space`, `ArrowDown`, `ArrowUp`, `Home`, `End`, `Escape`, `Tab`) with upward edge flip.
- **Reflow / 200% Zoom:** Verified `580x390` (1160×780 at 200% zoom) and `420x540` with static un-stuck `.topbar` and zero horizontal overflow (`scrollWidth <= clientWidth`).

---

## 7. Security, CSP & Backend Contract Hardening
- **Authenticated HTTP Bridge:** Sends `X-DIM-Session-Token` from `<meta name="dim-session-token">` on every `/api/*` request; flags `401`, `>= 500`, `HOST_EXEC_FAILED`, `HOST_TIMEOUT`, `UNAUTHORIZED` as disconnected.
- **Tauri IPC Contract:** Preserves all 12 Tauri commands (`commands.rs`), including `{ id: targetId, confirm: true }` on `delete_profile` and `{ backupDir, name }` on `restore_backup`.
- **XSS & CSP Compliance:** Zero `.innerHTML` usage (all DOM built with `createElement`/`textContent`), zero inline `<script>`, zero inline `style=""` HTML attributes, zero remote URLs/CDNs, zero CSS gradients or `backdrop-filter`; matches `tauri.conf.json` and `ui-server.mjs` strict CSP.

---

## 8. Developer Journey Testing Matrix (Journeys A–E & Edge Cases)
> **Important Label:** All checks below are **developer journey tested** via automated headless Edge CDP + mock HTTP bridge (`apps/desktop/ui/tests/visual-cdp.mjs`), NOT real consumer usability validation.

| Journey | Scenario | Status |
|---|---|---|
| **Journey A** | Empty state (`0` accounts) -> `+ Thêm tài khoản` -> Step 1 inline error recovery -> Create account -> Step 2 `"Mở Zalo để đăng nhập"` & focus restore | **developer journey tested — PASS** |
| **Journey B** | Multi-account grid (3 accounts & 6/10/20 accounts count summary), search filter + `Escape` clear, Primary Zalo protection card (`running`/`stopped`/`unknown`) | **developer journey tested — PASS** |
| **Journey C** | Error handling: protected Primary Zalo error banner + collapsed `▶ Chi tiết kỹ thuật`, `401 UNAUTHORIZED`, `AbortController` `HOST_TIMEOUT`, `statusData.ok === false` | **developer journey tested — PASS** |
| **Journey D** | Overflow menu WAI-ARIA keyboard nav + disabled `Đóng Zalo trước` hints + upward flip, Rename + color change, Details color display, Backup + non-destructive Restore color inheritance, Delete + focus restore | **developer journey tested — PASS** |
| **Journey E** | Dedicated Help view (`#view-help`) -> Diagnostics drilldown (`#btn-help-diagnostics`), `vi-VN` <-> `en-US` live re-translation, `light`/`dark` reload persistence (`theme-init.js`), `200%` zoom (`580x390`), `420x540` mobile | **developer journey tested — PASS** |
| **Async / Race Guards** | Per-account pending isolation (`pendingProfileActions`), double-click launch dedup (`pendingGlobalActions` / pending map), out-of-order `/api/status` sequence protection (`refreshSeq`), open overflow menu preserved across re-render | **developer journey tested — PASS** |

---

## 9. Automated Test Execution & Verification Results

1. **JS Syntax Check (`node --check`):**
   - Command: `node --check apps/desktop/ui/app.js apps/desktop/ui/theme-init.js apps/desktop/ui/i18n/index.js apps/desktop/ui/tests/i18n.test.mjs apps/desktop/ui/tests/ui-contracts.test.mjs apps/desktop/ui/tests/visual-cdp.mjs`
   - Result: **exit `0`** (all 6 files syntactically valid, pure LF line endings preserved).
2. **Unit & Contract Tests (`node --test`):**
   - Command: `node --test apps/desktop/ui/tests/i18n.test.mjs apps/desktop/ui/tests/ui-contracts.test.mjs`
   - Result: **exit `0`** — `tests 20 | pass 20 | fail 0 | cancelled 0 | skipped 0 | todo 0` (`12` tests in `i18n.test.mjs`, `8` tests in `ui-contracts.test.mjs`), covering default `vi-VN` locale, `271/271` key & `{placeholder}` parity, diacritics, static key existence, safe interpolation, missing-key recording, `SupportStatus` normalization, `resolveErrorInfo` separation, `Intl` formatters, `initI18n` 404/invalid JSON/timeout resilience, consumer jargon exclusion (including `dict.help`), all 22 `DimError::code()` + 4 bridge codes + `UNKNOWN` fallback, HTML/CSS/CSP contracts, `resolveIsolationCheckResult` honesty, `getAccountInitials`, ID-keyed color persistence + 8-color collision avoidance + removal, WCAG 2.2 AA `>= 4.5:1` contrast across all 8 `--id-*` hex colors, `delete_profile` `confirm: true`, `X-DIM-Session-Token`, `theme-init.js` `multi-zalo.theme`, and locale persistence.
3. **Headless Edge CDP E2E & Visual Matrix (`visual-cdp.mjs`):**
   - Command: `node apps/desktop/ui/tests/visual-cdp.mjs`
   - Result: **exit `0`** — `PASS (Account Dock: 16 matrix combinations, session token, theme reload, scale 0/1/3/10/20, Step1/Step2 resilience, color persistence/restore, primary Zalo honest states, timeout, Tauri invoke, Help view, WAI-ARIA menu, out-of-order status, 200% zoom, 0 runtime errors)` against CSP-enforced mock bridge.

---

## 10. Visual QA & Post-Fix Screenshot Inspection

All **20** post-fix PNG screenshots generated under `docs/design/screenshots/` at final UI code commit `fc91901` were visually inspected during verification:
- **Regenerated PNG Inventory (20 files at `fc91901`):** `account-dock-20accounts-vi-light.png` (`75293 B`), `account-dock-3accounts-vi-light.png` (`57391 B`), `account-dock-add-step1-vi-light.png` (`65485 B`), `account-dock-add-step2-vi-light.png` (`57570 B`), `account-dock-backups-vi-light.png` (`38767 B`), `account-dock-details-vi-light.png` (`76634 B`), `account-dock-diagnostics-vi-light.png` (`62561 B`), `account-dock-empty-vi-light.png` (`43669 B`), `account-dock-en-US-dark-1160x780.png` (`46076 B`), `account-dock-en-US-light-1160x780.png` (`46098 B`), `account-dock-error-vi-light.png` (`60209 B`), `account-dock-help-vi-light.png` (`72728 B`), `account-dock-menu-vi-light.png` (`65495 B`), `account-dock-settings-vi-light.png` (`46592 B`), `account-dock-vi-VN-dark-1160x780.png` (`47113 B`), `account-dock-vi-VN-light-1160x780.png` (`47093 B`), `account-dock-vi-VN-light-420x540.png` (`33077 B`), `account-dock-vi-VN-light-768x650.png` (`45644 B`), `account-dock-vi-VN-light-900x650.png` (`45822 B`), `account-dock-zoom-200-vi-light.png` (`73584 B`).
- **Inspection Findings Confirming `D-01`..`D-14` Fixes:**
  - `account-dock-3accounts-vi-light.png` & `account-dock-en-US-light-1160x780.png`: `[Tìm tài khoản...]` and `[Làm mới]` sit aligned on one single row; clean left/right card footer separation (`Chuyển đến` / `Switch to window` on left vs `Đóng Zalo` / `Close Zalo` + `⋯` on right), solid dot vs hollow ring `.state-dot`, darker high-contrast avatars, zero sticky `"Đã hoàn tất thao tác."` banner, and Primary Zalo card displaying `.state-dot`.
  - `account-dock-menu-vi-light.png`: Disabled `Sao lưu` and `Xóa tài khoản` items show `"Đóng Zalo trước"` inline hint.
  - `account-dock-add-step1-vi-light.png`: Checked swatch renders crisp white `✓` checkmark; consumer-friendly Vietnamese copy without technical jargon.
  - `account-dock-backups-vi-light.png`, `settings`, `diagnostics`, `help`: `Sẵn sàng khôi phục` pill remains single-line (`D-14`), `+ Thêm tài khoản` hidden off Home, and zero stale success banners across view transitions.
  - `account-dock-help-vi-light.png`: 2x2 grid of 4 guide cards + `"Mở Kiểm tra sự cố"` CTA bar.
  - `account-dock-20accounts-vi-light.png`: Clean 2-column grid with `"Hiển thị 20 / 20 tài khoản"` + search input + `"Làm mới"` aligned on one row.
  - `account-dock-vi-VN-light-420x540.png` & `account-dock-zoom-200-vi-light.png`: Non-sticky compact 2-row topbar with `[Tìm tài khoản...]` + `[Làm mới]` aligned on one row and zero horizontal overflow (`scrollWidth <= clientWidth`).

---

## 11. Cross-Platform & Integration Readiness Matrix

| Target / Environment | Status | Evidence / Notes |
|---|---|---|
| **Windows 11 — Microsoft Edge CDP (CSP Mock Bridge)** | **PASS** | `20/20` `node --test` pass, `visual-cdp.mjs` PASS (exit `0`, `0` runtime errors), 20 post-fix PNGs inspected |
| **Windows 10/11 — Native Tauri 2 WebView2 + Rust Core** | **`NOT_TESTED`** | UI-only worktree; awaiting merge into `main` (`a5feb4a`) and native build/smoke |
| **Windows 10/11 — Live `ui-server.mjs` + `dim-host.ps1`** | **`NOT_TESTED`** | UI-only worktree; `X-DIM-Session-Token` verified via mock bridge & contract alignment with `ui-server.mjs` |
| **macOS — WKWebView + `dim-platform-macos`** | **`NOT_TESTED`** | No macOS hardware in this session; Safari/WKWebView compatibility guards (`role="list"`, `'focusGroup' in ...`, popover fallback) added statically |
| **Ubuntu LTS — WebKitGTK + `dim-platform-linux`** | **`NOT_TESTED`** | No Linux hardware in this session; `fetch()` JSON loader and `@media (prefers-color-scheme: dark)` fallback implemented |
| **Consumer Usability Study (Non-Technical Users)** | **`NOT_TESTED`** | Journeys A–E are developer journey tested; real user testing remains pending |

---

## 12. Known Limitations & Honest Boundaries
1. **Mock-Bridge Scope:** All browser E2E checks in this worktree ran against `visual-cdp.mjs`'s mock HTTP server rather than live `dim-desktop.exe` (Tauri WebView2) or `ui-server.mjs` + `dim-host.ps1`.
2. **Merge & `Test-8` Adaptation Required on `main`:** Merging `feat/ui-vi-en-quiet-workspace` into `main` (`a5feb4a`) hits 3 UI file conflicts (`index.html`, `styles.css`, `app.js`) and requires updating legacy DOM regex assertions in `tests/run-fixture-tests.mjs` (`Test-8`, lines 376–417 and line 429) as detailed in `account-dock-backend-handoff.md`.
3. **Origin-Scoped Identity Colors:** `multi-zalo.accountColors` lives in `localStorage`, so colors will not automatically migrate across different origins (`http://127.0.0.1:<port>` vs `tauri://localhost`) unless backend optionally persists `color` on `ProfileRecord`.
4. **Upstream Zalo Desktop Limits:** Concurrent voice/video call pipe (`PipeZCallRecv`/`PipeZCallSend`) and shared Windows AUMID (`com.vng.zalo`) toast routing remain upstream VNG Zalo Desktop constraints, surfaced honestly in `#view-help` and `#view-diagnostics`.

---

## 13. Handoff & Sign-Off Summary
- **UI Code Commits:** `4904e6f4bd48ba8bc747b522edf77efa0e4d85a3` (`4904e6f`) + `fc91901ebc897c62630c0cfeeebaaed0fd8c469c` (`fc91901`)
- **Design & Acceptance Artifacts:**
  - `docs/design/account-dock-redesign.md`
  - `docs/design/account-dock-final-acceptance.md` (this document; Verdict: **`READY_WITH_LIMITATIONS`**)
  - `docs/design/account-dock-backend-handoff.md`
- **Next Action:** Backend integration engineer merges `feat/ui-vi-en-quiet-workspace` into `main` (`a5feb4a`) following `account-dock-backend-handoff.md` Section 9 and executes the Native Windows Integration Test Checklist (Section 11).
