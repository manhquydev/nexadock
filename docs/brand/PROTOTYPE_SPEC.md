# KHOANG — Phase 2 Prototype & UX Architecture Specification

> **Product:** KHOANG (*"Your accounts. Their own space."* / *"Một máy. Nhiều tài khoản. Không trộn dữ liệu."*)  
> **Scope:** Desktop UX Architecture, Modern Web Guidance Compliance, Interactive State Matrix, and Contract Preservation Checklist for upgrading `desktop-instance-manager/apps/desktop/ui/`.  
> **Inputs:**
> - Phase 1 Audit (`@auditor`, board `#5`)
> - [BRAND_GUIDELINES.md](file:///c:/Users/manhquy/Downloads/zalo/docs/brand/BRAND_GUIDELINES.md) & [DESIGN_DECISION_RECORD.md](file:///c:/Users/manhquy/Downloads/zalo/docs/brand/DESIGN_DECISION_RECORD.md) (`@brand`, board `#6`)
> - `modern-web-guidance` skill queries & retrieved guides (`@ux`)

---

## 1. Modern Web Guidance Compliance (Exact Guide IDs & Rules)

Every UI pattern in the KHOANG Tauri WebView is grounded in the retrieved `modern-web-guidance` guides:

| Guide ID | Pattern / Component | Mandatory Rules, Gotchas & Tauri WebView Fallbacks |
| :--- | :--- | :--- |
| **`dialog`** | Modal & confirmation dialogs (`#create-profile-dialog`, `#rename-profile-dialog`, `#delete-profile-dialog`, `#restore-backup-dialog`, `#profile-details-dialog`) | - Use native `<dialog>` opened via `.showModal()`. **Do NOT** add `role="dialog"`, `aria-modal`, a manual focus trap, `inert` toggling, or custom `Esc` handlers.<br>- Name every dialog via `aria-labelledby` (or `aria-label`).<br>- Put `autofocus` on the **safe action** (`button[value="cancel"]`) in `#delete-profile-dialog` and on the primary input in create/rename/restore dialogs.<br>- Read `dialog.returnValue` in the `close` listener and reset `dialog.returnValue = ''` immediately so stale values never leak on `Esc`.<br>- **Do NOT** set `display` on `dialog` unconditionally (scope to `dialog[open]`). |
| **`light-dismiss-a-dialog`** | Clicking outside non-destructive modals (`#profile-details-dialog`, `#restore-backup-dialog`) | - Use `closedby="any"` on non-destructive dialogs and `closedby="closerequest"` (or `none` with `cancel` guard) on dirty/destructive forms.<br>- **Fallback (WebKitGTK / Safari):** When `!('closedBy' in HTMLDialogElement.prototype)`, attach `pointerdown` + `pointerup` coordinate-box checks (`getBoundingClientRect()`) rather than `event.target === dialog` alone, so text-selection drags ending on the backdrop do not accidentally dismiss. |
| **`prevent-page-scroll-under-modal-dialog`** | Preventing background scroll while a modal is open | - Apply `overscroll-behavior: contain` to `dialog:modal` **and** `overflow: hidden; overscroll-behavior: contain;` to `dialog::backdrop` (`::backdrop` defaults to `overflow: visible`, which ignores `overscroll-behavior`).<br>- **Do NOT** lock scroll via `overflow: hidden` or `position: fixed` on `html`/`body` (prevents layout shift). |
| **`dark-mode`** & **`dark-mode-toggle`** | System/Light/Dark theme switching (`#theme-system-label`, `input[name="ui-theme"]`) | - Keep `<meta name="color-scheme" content="light dark">` in `<head>` and update it synchronously in `theme-init.js` before first paint (FOUC prevention).<br>- Set `color-scheme: light dark` on `:root` and map `<meta>` overrides in CSS via `:root:has(> head > meta[name="color-scheme"][content="dark"]) { color-scheme: dark; }` (and `light`).<br>- Use `light-dark(lightVal, darkVal)` for tokens, paired with `@media (prefers-color-scheme: dark)` fallbacks where needed.<br>- Expose the 3-state control (`System`, `Light`, `Dark`) inside Settings (`#view-settings`), showing the resolved OS state in `#theme-system-label`. |
| **`color`** | Contrast, status badges, Forced Colors Mode | - **Never use color alone** to indicate state (WCAG 1.4.1): every status badge pairs Color + Distinct Icon + Text Label.<br>- Maintain `>= 4.5:1` for text (including all 8 `--id-*` account colors vs `#FFFFFF`) and `>= 3:1` for UI borders/icons.<br>- Support `@media (forced-colors: active)` using system keywords (`CanvasText`, `Highlight`, `ButtonText`) and `outline` focus rings; never rely on `box-shadow` alone. |
| **`accessibility`** | Landmarks, headings, live regions, keyboard & focus | - Single `<main id="main-content" tabindex="-1">` with skip link, single `<h1>` per view (dialogs may use `<h2>`/`<h1>` as separate windows), sequential heading levels.<br>- Use `aria-disabled="true"` (not `disabled`) on toolbar/menu actions that need to remain keyboard-focusable to explain *why* they are blocked (e.g., *"Đóng Zalo trước"*), while guarding click/keyboard activation in JS.<br>- **Live regions:** Centralize `aria-live="polite"` for status/toast updates and `role="alert"` (`aria-live="assertive"`) only for critical errors (`#create-step1-error`, `#status-banner`). Never place live regions inside `inert` DOM when a modal is open.<br>- Enforce `min-block-size: 24px; min-inline-size: 24px;` (or `1.5rem` / `2.25rem` desktop targets) and `:focus-visible` with `outline` + `outline-offset`. |
| **`persistent-toast-notifications`** | Transient non-blocking feedback (e.g. "Copied diagnostics", "Backup created") | - Reserve inline `#status-banner` for actionable errors/warnings; use `popover="manual"` inside a single `role="region"` container for transient confirmations, suppressed while a modal `<dialog>` is open.<br>- Feature-detect `Object.hasOwn(HTMLElement.prototype, 'popover')` and style `[popover]:is(:popover-open, .\:popover-open)`. |
| **`responsive-design`**, **`size-aware-styling`** & **`css-layout`** | `420px` / `768px` / `1160px` breakpoints & 200% zoom | - Use `@container` (`container-type: inline-size`) on `.account-card` wrappers so cards adapt to their own column width rather than viewport width.<br>- Use `grid-template-columns: repeat(auto-fit, minmax(min(100%, 17.5rem), 1fr))` in Grid mode and `subgrid` (`grid-template-rows: subgrid`) where sibling cards align.<br>- Set `min-inline-size: 0` on all flex/grid items containing account names, notes, or paths so 200% zoom never causes horizontal overflow.<br>- Use `scrollbar-gutter: stable` on `#profiles-list` / main scroll container and `100%` (never `100vw`) for full-width layout. |
| **`card`** | Account cards (`.account-card`) | - Use `<article class="account-card">` with clean direct-child regions (`<header>`, content, `<footer>`) so cards without `notes` collapse cleanly without awkward empty gaps.<br>- Do not wrap the entire card in a `<button>` when it contains independent action buttons (Launch/Focus/Stop + Overflow Menu). |
| **`branded-select-styling`** | Optional custom selects (if any dropdowns are added) | - Use `appearance: base-select` only as progressive enhancement (`CSS.supports('appearance', 'base-select')`) with readable `<option>` text fallback; prefer native `<fieldset>`/`<legend>` radios for `ui-locale` and `ui-theme` as required by `ui-contracts.test.mjs`. |

> **Tauri CSP & Contract Gate:** Zero `linear-gradient`/`radial-gradient`/`conic-gradient`, zero `backdrop-filter`, zero `.innerHTML`, zero remote URLs, and strictly `rem`/`em` font sizes (`<meta name="text-scale" content="scale">`).

---

## 2. Information Architecture & Navigation (Desktop-First Upgrade)

### 2.1 Fixing Phase 1 IA Friction
1. **Eliminate Duplicate Empty-State CTAs:** In `01-empty-account-dock-light.png`, both the topbar (`#btn-open-create`) and empty hero (`#btn-empty-create`) show identical primary buttons. In the prototype, when `profiles.length === 0`, `#btn-open-create` is visually de-emphasized or hidden (`hidden` / secondary ghost) so `#btn-empty-create` is the single unmistakable primary CTA, while keeping `#btn-open-create` in the DOM for contract tests.
2. **Persistent Top App Bar + Contextual Secondary Views:**
   - **Primary Navigation Tabs (`.top-nav-btn[data-view]`):** `Tài khoản` (`profiles`), `Sao lưu` (`backups`), `Cài đặt` (`settings`).
   - **Utility Actions (Right Cluster):** `#btn-nav-help` (icon + label on `>= 768px`, icon + `aria-label` on `< 768px`), `#btn-goto-diagnostics` (or nested under Settings/Utility), `#connection-pill` (`#connection-label`), and `#topbar-open-summary` (e.g., *"2 đang mở"* / *"2 running"*).
   - **Back Links (`.back-link[data-back-to]`):** Keep `.back-link[data-back-to]` in `#view-diagnostics` and `#view-help` (true sub-views not in the main 3-tab strip) and style as a clean desktop breadcrumb header rather than a web-page back link.
3. **Toolbar & View Density Controls (`#view-profiles`):**
   - Single compact toolbar row: `#search-wrap` (`#profile-search`), `#accounts-count-summary`, **Grid / Compact List** segmented toggle (`data-view-mode="grid|list"`, persisted in `localStorage` under `khoang.viewMode`), **"Đóng tất cả" (Stop All)** secondary button (visible/enabled when `runningCount > 1`, wired to `stop_all_profiles`), `#btn-refresh`, and `#btn-open-create`.
4. **Scoped Polling:** `refreshAll()` remains intact for manual `#btn-refresh` and mutations, while background/focus polling only refreshes `get_status` on the Home view unless Backups or Diagnostics is active.

---

## 3. Complete View & State Matrix

### 3.1 Empty State & Onboarding (`profiles.length === 0`)
- **Layout:** Centered calm onboarding panel (`#profiles-empty`) with KHOANG "Partitioned Bay" mark (`32×32`), `#profiles-empty-title`, `#profiles-empty-hint` (1-sentence value prop: each account gets its own isolated space), and single primary CTA `#btn-empty-create`.
- **Primary Zalo Anchor (`#primary-zalo-section`):** Sits below as a recessed `--bg-surface-subtle` horizontal strip with a `--status-protected` Shield-Lock badge (*"Được bảo vệ · Chỉ đọc"* / *"Protected · Read-only"*), `#primary-zalo-state-row`, `#primary-zalo-desc`, and **0 mutation buttons**.

### 3.2 Home with 3 Accounts (Grid Mode Default)
- **Layout:** `grid-template-columns: repeat(auto-fit, minmax(min(100%, 17.5rem), 1fr))` inside `#profiles-list`. At `1160px`, 3 accounts form a balanced **3-column row** (eliminating the 50% orphan gap seen in `03-populated-account-dock-light.png`).
- **Card Anatomy (`.account-card[data-profile-id]`):**
  - `container-type: inline-size` on each card wrapper.
  - **Header:** `.account-avatar` (`--avatar-color` from `--id-1..--id-8`, `>= 4.5:1` vs `#FFFFFF`) + Account Name (single-line ellipsis with tooltip/details access) + Status Badge (Color + Icon + Label).
  - **Optional Notes:** Rendered only when `notes` is non-empty (`line-clamp: 2`), preventing empty vertical gaps on cards without notes.
  - **Footer:** Primary action `.btn-primary` (`Mở Zalo` / `Đưa lên trước` / `Khôi phục & Mở`) + Overflow Menu button (`button[aria-haspopup="menu"]`, `aria-expanded`, `aria-controls`) opening `.overflow-menu[popover="auto"]` with `[role="menuitem"]` items (`Chi tiết`, `Đổi tên & Màu`, `Sao lưu`, `Dừng`, `Xóa`).

### 3.3 Home with 20 Accounts (Grid + Compact List Mode)
- **Compact List Mode (`data-view-mode="list"`):**
  - Single-column dense rows (`min-block-size: 3.25rem`, `1px` `--border-hairline` dividers) displaying: `.account-avatar` (`28px`), Account Name, compact Status Badge, optional truncated note (`hidden` below `768px`), Primary Action button, and Overflow Menu button.
  - 20 accounts fit comfortably with minimal scrolling; `#search-wrap` (`#profile-search`) filters instantaneously client-side and updates `#accounts-count-summary` + `aria-live="polite"` result count.
  - **Grid Mode at 20 Accounts:** Auto-fits 3 columns at `1160px`, 2 columns at `768px`, 1 column at `420px`.

### 3.4 Account Details & Profile Lifecycle States
- **Details Modal (`#profile-details-dialog`):**
  - Clean key-value `<dl>` (`#profile-details-list`) for Name, Status, Created/Last Opened, Storage Size, and Notes; collapsible/secondary `<details>` or section for `#profile-details-advanced-list` (profile path, directory slug) so consumer copy stays jargon-free.
- **All Profile Lifecycle States (`resolveAccountState()`):**
  1. `running`: Green badge + solid dot + *"Đang chạy"* / *"Running"*; primary CTA = `Đưa lên trước` (`focus_profile`); `stop` enabled; `backup` & `delete` disabled with `.menu-item-hint` (*"Đóng Zalo trước"*).
  2. `stopped`: Neutral badge + hollow dot + *"Đã dừng"* / *"Stopped"*; primary CTA = `Mở Zalo` (`launch_profile`); `backup` & `delete` enabled.
  3. `starting` / `stopping` (**Pending**): Teal badge + stepped ring (static under `prefers-reduced-motion`) + *"Đang mở…"* / *"Đang đóng…"*; conflicting actions disabled (`pendingProfileActions`).
  4. `recovery_required`: **Distinct Amber/Ochre** card border (`#E5A93C` light / `#855812` dark), diamond-alert icon + *"Cần khôi phục"* / *"Recovery needed"*, brief inline 1-line explanation (*"Phiên trước đóng chưa sạch"*), primary CTA = *"Khôi phục & Mở"* / *"Recover & Open"*; `backup` & `delete` disabled with *"Đóng Zalo trước"* hint.
  5. `error`: Red badge + triangle-exclamation icon + *"Lỗi"* / *"Error"*; inline error summary + link to Diagnostics.
  6. `unknown` / disconnected: Neutral/muted badge + *"Không rõ"* / *"Unknown"* (never falsely reported as `running` or `stopped`).

### 3.5 Add / Rename / Delete Flows
- **Add Account (`#create-profile-dialog`, 2-step):**
  - `#create-step-1`: `#create-profile-form`, `#create-profile-name` (autofocus), `#create-profile-notes`, 8-swatch color picker (`input[name="create-account-color"]` in `<fieldset>`), inline `#create-step1-error` (`role="alert"`, wired via `aria-invalid="true"` + `aria-describedby="create-step1-error"`), `#btn-cancel-create`, `#btn-submit-create`.
  - `#create-step-2`: Success confirmation with `#create-step2-avatar`, `#create-step2-title`, `#btn-create-launch` (primary), `#btn-create-later` (secondary).
- **Rename & Color (`#rename-profile-dialog`):** `#rename-profile-name` (autofocus), `input[name="rename-account-color"]`, `button[value="cancel"]`, `button[value="rename"]`.
- **Delete Confirmation (`#delete-profile-dialog`):**
  - `#delete-profile-warning` (states account name and that isolated data will be removed, while Primary Zalo is untouched).
  - **`autofocus` on `button[value="cancel"]`**; `button[value="delete"]` styled `.btn-subtle-danger` and positioned so it never shifts under the pointer.
  - Restores focus to the next logical element (`needsFocusRestore()`) after deletion.

### 3.6 Backup & Restore (`#view-backups`)
- **View:** Header with `#btn-refresh-backups`, `#backups-table-wrap` containing semantic `<table>` (`<th scope="col">`) and `#backups-tbody`, or `#backups-empty` when no backups exist.
- **Restore Dialog (`#restore-backup-dialog`):** Non-destructive only (always creates a new profile inheriting the source account color); `#restore-backup-subtitle`, `#restore-profile-name`, `button[value="restore"]`.

### 3.7 Settings, Diagnostics & Help
- **Settings (`#view-settings`):**
  - Native `<fieldset>`/`<legend>` radio groups for Language (`input[name="ui-locale"]`: `vi-VN` / `en-US`) and Theme (`input[name="ui-theme"]`: `system` with `#theme-system-label`, `light`, `dark`).
  - Storage size toggle (`#toggle-sizes`) controlling `get_status({ withSize })`.
  - About section (`#settings-about-list`) with KHOANG version, non-affiliation notice, and quick link to `#btn-goto-diagnostics` / `#btn-nav-help`.
- **Diagnostics (`#view-diagnostics`):**
  - `.back-link[data-back-to]`, `#btn-run-isolation` (`verify_isolation`), `#btn-refresh-diagnostics` (`get_diagnostics`), `#readiness-tbody`, `#diagnostics-tbody`, and `#sanitized-log-output` (monospace `JetBrains Mono`).
  - **Honest Isolation Display:** `resolveIsolationCheckResult()` never converts `NOT_TESTED`, `BLOCKED`, or `UNKNOWN` into `PASS`.
- **Help (`#view-help`):**
  - `.back-link[data-back-to]`, concise troubleshooting FAQ, Primary Zalo safety explanation, and `#btn-help-diagnostics`.

### 3.8 Light / Dark Mode & Error / Loading / Disabled States
- **Light / Dark:** Warm Stone (`#F7F5F0` / `#FFFFFF`) vs Warm Obsidian (`#121210` / `#1B1B18` with `#2F2F2A` borders) + Deep Teal (`#0F6B66` / `#147A74`).
- **Error Banner (`#status-banner`):** `role="alert"` for critical host/IPC failures (`HOST_TIMEOUT`, disconnected bridge), showing `#status-banner-text` (plain user message from `resolveErrorInfo()`), expandable `#status-banner-details` -> `#status-banner-raw` (technical code/detail), and `#btn-dismiss-banner`.
- **Zalo Install Alert (`#zalo-install-alert`):** Shown prominently when Zalo executable is not detected, blocking launch actions with clear guidance.
- **Loading:** Non-shifting skeleton/busy indicator (`aria-busy="true"`) during initial `get_status` or `withSize` calculation.

### 3.9 Responsive Breakpoints (`420px`, `768px`, `1160px`) & 200% Zoom
- **`>= 1160px` (Standard Desktop):** Full topbar with labels, 3-column Grid (or spacious List), side-by-side toolbar.
- **`720px – 1159px` (Compact Desktop, fixing `05-small-window-800x600.png`):** 2-column Grid (`minmax(17.5rem, 1fr)`), topbar wraps cleanly or compacts utility labels, search input flexes (`min-inline-size: 12rem`).
- **`420px – 719px` (Narrow Utility Window):** 1-column stack, compact KHOANG lockup (mark + wordmark), toolbar wraps cleanly (`flex-wrap: wrap`), zero horizontal scroll.
- **200% Zoom:** All typography in `rem`/`em`, `<meta name="text-scale" content="scale">`, `min-inline-size: 0` on flex/grid children, wrapping button groups so no text clips or overflows horizontally.

### 3.10 Localization (`vi-VN` / `en-US`)
- Default `vi-VN`; 100% key and placeholder parity in `i18n/vi-VN.json` and `i18n/en-US.json`.
- Updates `document.documentElement.lang` (`vi` / `en`) and `localStorage['multi-zalo.locale']`.
- Consumer UI copy MUST NOT contain `\b(PID|sandbox|IPC|Authenticode|loopback)\b` (enforced by `i18n.test.mjs`).

---

## 4. Exact Preservation Checklist (Backend IPC & Test Contracts)

### 4.1 Tauri Window & CSP (`src-tauri/tauri.conf.json`)
- Window: `1160×780`, `minWidth: 420`, `minHeight: 540`, `resizable: true`, `withGlobalTauri: true`.
- Strict CSP: `default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'`.

### 4.2 Backend IPC Commands (`src-tauri/src/commands.rs`) & HTTP Bridge
1. `get_status` (`{ withSize }`) — `GET /api/status`
2. `create_profile` (`{ name, notes }`) — `POST /api/profiles`
3. `rename_profile` (`{ id, newName }`) — `POST /api/profiles/:id/rename`
4. `delete_profile` (`{ id, confirm: true }`) — `DELETE /api/profiles/:id`
5. `launch_profile` (`{ id }`) — `POST /api/profiles/:id/launch`
6. `focus_profile` (`{ id }`) — `POST /api/profiles/:id/focus`
7. `stop_profile` (`{ id }`) — `POST /api/profiles/:id/stop`
8. `stop_all_profiles` (`{}`) — *(available in `commands.rs`)*
9. `backup_profile` (`{ id }`) — `POST /api/profiles/:id/backup`
10. `list_backups` (`{}`) — `GET /api/backups`
11. `restore_backup` (`{ backupDir, name }`) — `POST /api/backups/restore`
12. `get_diagnostics` (`{}`) — `GET /api/diagnostics`
13. `verify_isolation` (`{}`) — `GET /api/verify-isolation`
- Preserve `meta[name="dim-session-token"]` / `X-DIM-Session-Token` HTTP bridge auth, `refreshSeq`, `pendingProfileActions`, `pendingGlobalActions`, and `HOST_TIMEOUT`.

### 4.3 `localStorage` Keys & Test-Relied Selectors
- **Storage keys:** `multi-zalo.locale`, `multi-zalo.theme`, `multi-zalo.accountColors` (new view-mode key e.g. `khoang.viewMode` must be additive only).
- **Navigation & Views:** `.top-nav-btn[data-view]`, `#btn-nav-help`, `#btn-brand-home`, `.back-link[data-back-to]`, `#view-profiles`, `#view-backups`, `#view-settings`, `#view-diagnostics`, `#view-help`, `#btn-goto-diagnostics`, `#btn-help-diagnostics`, `#btn-open-create`, `#connection-pill`, `#connection-label`, `#topbar-open-summary`.
- **Home & Cards:** `#status-banner`, `#status-banner-text`, `#status-banner-details`, `#status-banner-raw`, `#btn-dismiss-banner`, `#zalo-install-alert`, `#search-wrap`, `#profile-search`, `#accounts-count-summary`, `#btn-refresh`, `#profiles-list`, `.account-card[data-profile-id]`, `.account-avatar` (`--avatar-color`), `.btn-primary`, `.btn-subtle-danger`, `button[aria-haspopup="menu"]`, `.overflow-menu`, `[role="menuitem"]`, `.menu-item-hint`, `#profiles-empty`, `#profiles-empty-title`, `#profiles-empty-hint`, `#btn-empty-create`, `#primary-zalo-section`, `#primary-zalo-state-row`, `#primary-zalo-desc`.
- **Dialogs & Inputs:** `#create-profile-dialog`, `#create-step-1`, `#create-step-2`, `#create-profile-form`, `#create-profile-name`, `#create-profile-notes`, `#create-step1-error`, `#btn-submit-create`, `#btn-cancel-create`, `#create-step2-title`, `#create-step2-avatar`, `#btn-create-launch`, `#btn-create-later`, `input[name="create-account-color"]`, `#rename-profile-dialog`, `#rename-profile-name`, `input[name="rename-account-color"]`, `button[value="rename"]`, `#delete-profile-dialog`, `#delete-profile-warning`, `button[value="delete"]`, `button[value="cancel"]`, `#restore-backup-dialog`, `#restore-backup-subtitle`, `#restore-profile-name`, `button[value="restore"]`, `#profile-details-dialog`, `#profile-details-list`, `#profile-details-advanced-list`.
- **Backups / Settings / Diagnostics:** `#backups-table-wrap`, `#backups-tbody`, `#backups-empty`, `#btn-refresh-backups`, `input[name="ui-locale"]`, `input[name="ui-theme"]`, `#theme-system-label`, `#settings-about-list`, `#toggle-sizes`, `#btn-run-isolation`, `#btn-refresh-diagnostics`, `#diagnostics-tbody`, `#readiness-tbody`, `#sanitized-log-output`.
