# KHOANG — Component UX Audit & Design Contracts

> **Branch:** `feat/ux-component-reconstruction` (rebased onto `origin/main` @ `cd533cb`)  
> **Scope:** Component-level UX, information architecture, interaction design, feedback architecture, and desktop ergonomics overhaul.  
> **Brand Source of Truth Preserved:** `KHOANG` — no product renaming or new logo work in this reconstruction.

---

## 1. UX Audit (Evidence-Based Defect Classification)

| Area | Category | Defect & Concrete Evidence |
|---|---|---|
| **Back Navigation** (`index.html:258,396,439,520,597`) | **Component Design + IA + Interaction** | All 5 subviews use a raw text button `<button class="btn-link back-link">← Quay lại ...</button>` with a Unicode arrow inside i18n strings. Top-level Rail tabs (`Backups`, `Settings`, `Help`) show redundant Back links even though they are peer Rail views (`aria-current="page"`). `Diagnostics` hardcodes `data-back-to="settings"` even when entered from `Help` (`#btn-help-diagnostics`), and Back never restores focus to the triggering element. |
| **In-App Feedback** (`app.js:303-358`, `index.html:171-185`) | **Feedback Architecture + Interaction** | Single global `state.banner` handles everything: transient successes (`accountCreated`, `accountRenamed`, `accountDeleted`, `backupCreated`, `backupRestored`) push the entire `#workspace-main` layout downward and stick until manual close or view switch; `clearBanner()` on actions wipes errors; recoverable per-account errors (`PROCESS_ERROR`, `PROTECTED_PRIMARY_PROCESS`) appear at the top of the window instead of next to the failing account; `#status-banner` stays `role="status"` instead of `role="alert"` on errors; no deduplication or toast queue. |
| **Account Rail** (`app.js:931-1059`, `styles.css:563-688`, `account-dock-error-vi-light.png`) | **Component Design + Interaction** | Clicking any avatar immediately navigates AND fires `activate_account` with no right-click context menu for pin/focus/details/rename/stop without leaving the current view. Selected vs Running vs Pinned vs Protected states lack dedicated semantic styling (running uses only a `10px` dot; collapsed tooltips rely on Chromium-only `interestfor` falling back to delayed OS `title`). |
| **Account Workspace & Cards** (`app.js:1803-1930`, `account-dock-3accounts-vi-light.png`) | **Component Design + Visual Hierarchy** | Grid cards have excessive empty height below 2 metadata lines, and every running card renders a heavy solid-fill `.btn-primary` (`Đưa cửa sổ lên trên`), creating visual noise when 3+ accounts run simultaneously. Cards have no right-click context menu and no inline error container for contextual recovery. |
| **Page Toolbar** (`index.html:332-351`, `account-dock-vi-VN-light-420x540.png`) | **Responsive + Component Design** | At `420x540`, `#home-toolbar` wraps into 3 uncoordinated rows (search -> segmented + refresh -> add button), pushing account cards down the viewport. All toolbar controls have equal visual weight with no icon-collapse or compact secondary grouping. |
| **Desktop Stage** (`index.html:256-322`, `account-dock-desktop-stage-vi-light.png`) | **Information Architecture + Component Design** | Renders 3 disconnected identical white `.panel` cards stacked vertically; 5 flat buttons sit in one row (`Ghim`, `Đổi tên`, `Sao lưu`, `Thông tin`, `Xóa tài khoản`) with destructive `Xóa tài khoản` adjacent to safe `Thông tin tài khoản`. |
| **Diagnostics** (`index.html:519-593`, `app.js:2019-2174`, `account-dock-diagnostics-vi-light.png`) | **Information Architecture + Copy** | Leads with raw admin key-value dumps (`authenticode_status`, `smart_app_control_state`, `path_boundary`, `ntfs_acl`, `call_pipe`) instead of plain-language consumer answers (*Is Zalo detected? Are accounts isolated? What needs action?*). Lacks Copy Diagnostics and progressive disclosure of technical IDs. |
| **Typography & Fonts** (`styles.css:19-20`, `apps/desktop/ui/`) | **Platform + Token Verification** | Declares `"Inter", "Be Vietnam Pro", ...` with **zero `.woff2` files bundled** in `apps/desktop/ui/`. On Windows, WebView2/Edge falls back to `Segoe UI` / `Consolas` — must be verified via `document.fonts` and computed font metrics in CDP rather than assumed. |

---

## 2. Component Design Contracts

### A. Back Navigation (`<nav class="view-nav-bar">`)
1. **Purpose:** Return cleanly to the parent view with accurate hierarchical context.
2. **Context:** Only genuine child views: **Desktop Stage** (`Account Spaces / <Name>`) and **Diagnostics** (`Settings / Diagnostics` or `Help / Diagnostics` based on `state.diagnosticsReturnView`). Top-level Rail views (`Accounts`, `Backups`, `Settings`, `Help`) omit redundant back links on desktop.
3. **Primitive:** `<nav aria-label="...">` containing `<button type="button" class="back-btn" data-back-to="...">` + SVG left-chevron + breadcrumb trail `<span class="view-breadcrumb">`.
4. **Anatomy:** `[Chevron Icon + Parent Label] / [Current Page/Account Name]`.
5. **States:** Default, Hover (`--bg-subtle`), Active, Focus-visible (`2px` `--accent-link`).
6. **Interaction:** Click / Enter / Space navigates to parent view and restores focus to logical origin (`#btn-goto-diagnostics`, `#btn-help-diagnostics`, or the active account card).
7. **Copy:** Clean parent label without Unicode `←` inside dictionary strings (`"Tài khoản"`, `"Cài đặt"`, `"Trợ giúp"`).
8. **Responsive:** Single compact bar (`min-block-size: 2.25rem`); long account names truncate with `text-overflow: ellipsis`.
9. **Accessibility:** `min-block-size: 2.25rem`, `aria-label` describing full back destination.
10. **Acceptance:** Clicking Diagnostics from Help returns to Help; from Settings returns to Settings; focus restores cleanly.

### B. In-App Feedback System (`#toast-region`, `#workspace-alerts`, inline `[data-profile-error]`)
1. **Purpose:** Deliver feedback at the exact scope, severity, and persistence required without shifting layout or hiding critical failures.
2. **Categories & Primitives:**
   - *Transient Success / Info (`A`):* Non-blocking **Toast** (`#toast-region`, `aria-live="polite"`, `4.5s` auto-dismiss, hover/focus pause, max 3 visible, deduplicated by `id`).
   - *Persistent Informational (`B`):* Neutral inline **Callout** (`role="status"` / `is-info`), e.g. Protected Primary Zalo or security-invariant protection guards (`PROTECTED_PRIMARY_PROCESS`, `PROTECTED_PRIMARY_PATH`, `PATH_TRAVERSAL*`) classified by `classifyErrorBehavior(errorCode)` as `info_only` (`retryable: false`, never showing a `Thử lại` / `Retry` button and never using assertive `role="alert"`).
   - *Actionable Warning (`C`) & Recoverable Per-Account Error (`D`):* **Inline Contextual Alert** inside the affected account card/stage (`[data-profile-error]`). `classifyErrorBehavior(errorCode)` distinguishes `action_required` (`role="status"`, `is-warn`, `retryable: false`, targeted Diagnostics CTA when applicable) and `non_retryable` (`role="status"`, `retryable: false`) from `retryable` errors (`PROCESS_ERROR`, `role="alert"`, `retryable: true` with contextual Retry CTA). Dismissing an inline error restores focus to the card's primary/action button.
   - *Critical System Error (`E`):* Global **System Banner** (`#status-banner`, `role="alert"` / `aria-live="assertive"`) reserved for `HOST_EXEC_FAILED`, `HOST_TIMEOUT`, `UNAUTHORIZED` or missing Zalo Desktop (`#zalo-install-alert`). Never overwritten by transient success toasts.
   - *Destructive Confirmation (`F`):* Native `<dialog>` with explicit consequences and safe `Cancel` default autofocus.
   - *Long-Running Operation (`G`):* Task-scoped `aria-busy="true"` + inline progress label on the triggered control.
3. **Acceptance:** Success toast does not shift layout or clear `#status-banner`; repeated polling never spawns duplicate banners/toasts; per-account error renders inline on the account card and Desktop Stage.

### C. Account Rail (`#account-rail`)
1. **Purpose:** Fast, reliable signature workspace switcher and quick-action anchor.
2. **States:** `Selected` (`--accent-soft` + `3px` `--accent` left bar + `aria-pressed="true"`), `Running` (`--ok-fg` emerald dot + subtle emerald avatar ring), `Pinned` (`pinned` group + pin indicator), `Protected` (shield badge, neutral ring), `Recovery` (amber diamond).
3. **Interaction:** Left-click activates/focuses; **Right-click / `Shift+F10`** opens shared Account Context Menu (`Bring to front / Open`, `Pin / Unpin`, `Rename`, `Details`, `Close Zalo`, `Remove`) without leaving the current screen. Custom `#tooltip-portal` hover/focus tooltip when collapsed (suppressed during active WebView).

### D. Account Workspace (`#view-profiles`) & E. Page Toolbar (`#home-toolbar`)
1. **Toolbar Hierarchy:** Primary CTA (`Thêm tài khoản`) anchored top-right; Search + Segmented View Toggle (`Lưới` / `Danh sách`) + Secondary actions (`Làm mới`, `Đóng tất cả`). Below `560px`, secondary `Làm mới` collapses cleanly to icon+label compact or icon button while Search + Segmented stay aligned without 3-row sprawl.
2. **Account Items:** Running accounts use a calmer tonal `.btn-primary.is-tonal` (`Đưa cửa sổ lên trên`), reserving solid `.btn-primary` fill for primary launch (`Mở & Đưa lên trên`) and recovery (`Khôi phục & Mở`). Right-click context menu supported on every card/row; inline `[data-profile-error]` slot renders recoverable errors in context.

### F. Desktop Stage (`#stage-desktop-session`)
- Unified hero card + honest non-embedding notice + inline remediation alert + sectioned **Window Arrangement** (OS-honest companion snap) + **Account Management** (frequent actions separated cleanly from right-aligned Danger Zone `Xóa tài khoản`) + collapsible `<details>` technical storage rows.

### G. Diagnostics (`#view-diagnostics`)
- **Tier 1:** Plain-language System Health Summary (`#diag-health-summary`) answering 5 consumer questions + **Copy Diagnostics** button + refresh/measure controls.
- **Tier 2:** Grouped practical checks (`Tách biệt dữ liệu`, `Quyền & Bảo vệ`, `Giới hạn đã biết`) preserving exact `PASS` / `NOT_TESTED` / `BLOCKED` / `UNKNOWN` states with actionable guidance.
- **Tier 3:** Expandable `<details id="diag-technical-disclosure">` housing raw IDs (`path_boundary`, `ntfs_acl`, `call_pipe`), OS matrix table, and sanitized logs.
