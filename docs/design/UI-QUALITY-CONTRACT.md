# NexaDock — UI Quality Contract & Design System Specification (v2)

> **Scope:** Desktop Application (`desktop-instance-manager/apps/desktop/ui/`)  
> **Design Philosophy:** **NexaDock Calm Precision** — warm alabaster/charcoal surfaces, crisp jade accent (`#17675f` light / `#1d8278` dark), zero flashy gradients, zero `backdrop-filter`, zero `innerHTML`, `rem`/`em` typography for 200% OS text-scaling fidelity, 100% `vi-VN` / `en-US` parity.  
> **Invariants:** Primary Zalo strictly read-only; `WebIsolated` visibly labeled **Experimental** (`Thử nghiệm`); zero pixel-art or retro-game UI inside the desktop application.

---

## 1. Design Token Foundation

| Category | Token | Light Value | Dark Value | Purpose |
|---|---|---|---|---|
| **Surfaces** | `--bg-app` | `#f6f5f2` | `#121210` | Shell canvas & inset controls |
| | `--bg-surface` | `#ffffff` | `#1b1b18` | Elevated cards, rail, dialogs, menus |
| | `--bg-subtle` | `#efece5` | `#23231f` | Segmented tracks, hover backgrounds, table headers |
| | `--bg-hover` | `#eae6dd` | `#2a2a25` | Secondary hover / inactive tab hover |
| | `--bg-active` | `#dfd9cd` | `#32322c` | `:active` tactile press state |
| **Borders** | `--border` | `#e2ddd3` | `#2f2f2a` | Hairline cards & dividers |
| | `--border-strong` | `#c7c0b3` | `#44443d` | Input frames, secondary buttons, selected rail |
| **Typography** | `--text-primary` | `#192a26` | `#f2f0ea` | Headings, primary labels (>= 4.5:1) |
| | `--text-secondary` | `#57534e` | `#b5b0a6` | Subtitles, metadata, inactive tabs |
| | `--text-muted` | `#6e6a63` | `#918c82` | Placeholders, disabled hints |
| **Brand & Accent** | `--accent` | `#17675f` | `#1d8278` | Primary CTA, focus rings, active indicators |
| | `--accent-hover` | `#12544d` | `#26998e` | Primary CTA hover & active |
| | `--accent-soft` | `#bce5d8` | `#142e2c` | Tonal CTA & selected rail background |
| **Semantic** | `--ok-fg` / `--ok-bg` / `--ok-border` | `#15683a` / `#e9f5ee` / `#b7dfc6` | `#4ade80` / `#132a1c` / `#225438` | Running / Pass |
| | `--warn-fg` / `--warn-bg` / `--warn-border` | `#9a4d06` / `#fef3c7` / `#e5a93c` | `#fbbf24` / `#2d220b` / `#855812` | Recovery / Experimental / Warning |
| | `--danger-fg` / `--danger-bg` / `--danger-border` | `#b91c1c` / `#fee2e2` / `#fca5a5` | `#f87171` / `#2e1414` / `#7f1d1d` | Inline errors & subtle danger |
| | `--danger-solid` / `--danger-solid-hover` | `#b91c1c` / `#991b1b` | `#b91c1c` / `#991b1b` | Solid destructive CTA (contrast >= 6.47:1 vs `#fff`) |
| **Radii** | `--radius-sm` / `--radius-md` / `--radius-card` (`--radius-lg`) / `--radius-pill` | `6px` / `10px` / `14px` / `999px` | Same | Controls, panels, cards, badges/avatars |
| **Account Identity** | `--id-blue`, `--id-teal`, `--id-green`, `--id-amber`, `--id-coral`, `--id-plum`, `--id-indigo`, `--id-slate` | `#1e56a0`, `#0f6b66`, `#1c6e3b`, `#a14a0a`, `#a92b5b`, `#6d3b9e`, `#4b468e`, `#0e6488` | Same | 8-color WCAG AA (>= 4.5:1 vs `#ffffff`) avatar palette |

---

## 2. Mandatory Content Resilience Contract

All surfaces must gracefully handle extreme user-generated and localized strings (`1` char, `40`, `80`, `150` chars, `200` continuous unbreakable characters, tall Vietnamese diacritics `Ầ Ợ Ử Ễ`, Emoji + CJK `🚀 測試 ワークスペース`, and `160`-char notes) across `420x540`, `1160x780`, `1440x900`, `1686x864`, and `200%` text scaling:

1. **Equal-Height Grid & Aligned Actions:**
   - `.account-grid` uses `grid-template-columns: repeat(auto-fit, minmax(min(100%, 18.5rem), 1fr))` and `align-items: stretch`.
   - `.account-card` uses `justify-content: space-between; min-inline-size: 0;` so cards on the same row match height and `.account-card-actions` align horizontally within `<= 2px`.
2. **Bounded Multi-Line Clamping:**
   - `.account-name`: `line-height: 1.35`, clamped to `2` lines (`-webkit-line-clamp: 2; overflow: hidden; overflow-wrap: anywhere;`).
   - `.account-note`: `line-height: 1.4`, clamped to `2` lines (`-webkit-line-clamp: 2; overflow: hidden; overflow-wrap: anywhere;`).
3. **Diacritic-Safe Single-Line Rail Truncation:**
   - `.rail-item-name`: `line-height: 1.35`, `white-space: nowrap; overflow: hidden; text-overflow: ellipsis;` preventing vertical clipping of `Ầ`, `Ợ`, `Ử`.
4. **Stage Headings & Breadcrumbs:**
   - `#desktop-stage-name`: `overflow-wrap: anywhere`, `line-height: 1.3`, 2-line clamp.
   - `#web-stage-name`: `min-inline-size: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; overflow-wrap: anywhere;` with `.web-session-badge { flex-shrink: 0; }`.
   - `#desktop-breadcrumb-name`: `min-inline-size: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;`.
5. **Accessible Full-Name Reveal (Mouse + Keyboard + Screen Reader):**
   - Complete untruncated text remains in DOM `textContent` and `aria-label`.
   - `refreshTruncationStates()` / `wireFullNameReveal()` detects actual overflow (`scrollHeight > clientHeight + 1 || scrollWidth > clientWidth + 1`) and assigns `data-truncated="true"` + `tabindex="0"` only when truncated.
   - Focusing or hovering a truncated `.account-name`, `#desktop-stage-name`, `#web-stage-name`, `#desktop-breadcrumb-name`, or `.rail-account-btn` (collapsed OR expanded-truncated) opens `#active-floating-tooltip` (`.rail-tooltip.is-floating-open`, `max-inline-size: 20rem; white-space: normal; overflow-wrap: anywhere;`) linked via `aria-describedby="active-floating-tooltip"` and dismissible via `Escape` (WCAG 1.4.13).

---

## 3. Component Quality Contracts (All 19 Components)

### 3.1 Button (`.btn`)
- **Anatomy:** `<button type="button|submit" class="btn btn-{primary|secondary|danger|subtle-danger|link} [btn-sm|btn-xs|btn-tonal]">`
- **Sizing:** Default `min-block-size: 2.25rem` (`padding: 0 0.875rem`), `.btn-sm` `2rem` (`padding: 0 0.625rem`), `.btn-xs` `1.75rem` (`padding: 0.25rem 0.5rem`), `.btn-link` `min-block-size: 1.5rem` (WCAG 2.5.8 `>= 24px`).
- **States:** Default, `:hover:not(:disabled)`, `:active:not(:disabled)`, `:focus-visible` (`2px` `--accent` outline + `2px` offset), `:disabled` (`opacity: 0.55; cursor: not-allowed`), `[aria-busy="true"]` (`cursor: progress; opacity: 0.82`). `.btn-danger` uses `--danger-solid` / `--danger-solid-hover` tokens (no `filter: brightness`).
- **Overflow / Responsive:** `white-space: nowrap`; secondary toolbar buttons collapse labels where specified (`#btn-refresh` below `35em`).
- **Keyboard / AT:** Native `<button>` semantics (`Enter`/`Space`); `aria-label` when action needs account context (`launchAria`, `stopAria`).

### 3.2 IconButton (`.icon-btn`)
- **Anatomy:** `<button type="button" class="icon-btn [icon-btn-sm|icon-btn-xs]" aria-label="...">` enclosing inline `<svg aria-hidden="true">` or glyph (`?`, `⋯`, `×`).
- **Sizing:** Default `2.25rem × 2.25rem` (`.rail-icon-btn`), `-sm` `2rem × 2rem` (`#btn-web-info`, `#btn-web-more`, card `⋯`), `-xs` `1.5rem × 1.5rem` (`.toast-close`, WCAG 2.5.8 `24×24px` minimum).
- **States:** `:hover`, `:active` (`--bg-active`), `:focus-visible`, `[aria-pressed="true"]` (toggle buttons), `[aria-expanded="true"]` (menu/popover triggers), `:disabled`.
- **Keyboard / AT:** Mandatory `aria-label` (never relies on `title` alone for accessible name).

### 3.3 Input (`.form-field input`, `.search-input`, `.rail-filter-input`)
- **Anatomy:** `<label for="...">` + `<input type="text|search" id="...">` + optional `<p class="form-error" role="alert">`.
- **Sizing:** Dialog text inputs `min-block-size: 2.375rem` (`padding: 0.375rem var(--sp-3)`); toolbar `.search-input` `2.25rem`; compact `.rail-filter-input` `2rem`.
- **States:** Default (`--border-strong`), `:hover:not(:disabled)` (`--text-secondary` border), `:focus-visible`, `:disabled` (`--bg-subtle`, `opacity: 0.55`), `[aria-invalid="true"]` (`--danger-fg` border).
- **Validation Policy:** Client-side and server-side validation on `#create-profile-name`, `#rename-profile-name`, and `#restore-profile-name` sets `aria-invalid="true"`, `aria-describedby`, and focuses the input without silently aborting or closing the modal. Errors clear on `input`.

### 3.4 Select (Radio / Segmented Alternative Contract)
- **Pattern:** Per modern web guidance, exclusive choice sets of `2–5` options (`ui-locale`, `ui-theme`, `create-session-mode`, view layout) use visible `<input type="radio">` inside `<fieldset>`/`<legend>` or `role="group"` segmented controls for zero-click scanning rather than native `<select>`.
- **Future Contract:** Should any setting expand to `6+` exclusive options, a `.select-input` primitive must match `.form-field input` (`min-block-size: 2.375rem`, `border: 1px solid var(--border-strong)`, `border-radius: var(--radius-sm)`, `:hover`/`:disabled`/`[aria-invalid="true"]`/`forced-colors`).

### 3.5 Search (`<search>` + `.search-input` / `.rail-filter-input`)
- **Anatomy:** `<search>` landmark containing visually-hidden `<label>` and `<input type="search">`.
- **Tokens & Clear Control:** Both `.search-input::placeholder` and `.rail-filter-input::placeholder` use `--text-muted`. Native `::-webkit-search-cancel-button` is replaced with a `clip-path` `×` geometry using `--text-muted` -> `--text-primary` on hover (`ButtonText` in `forced-colors: active`), guaranteeing crisp contrast in light and dark themes.
- **Keyboard / AT:** `/` shortcut focuses active search/filter; `Escape` clears non-empty query without closing parent view.

### 3.6 Tabs / Segmented Controls (`.view-segmented`, `.segmented-control`)
- **Anatomy:** `.view-segmented` (`role="group"`, `.view-seg-btn[aria-pressed]`) and `.segmented-control` (`<fieldset>` + `<label class="seg-option"><input type="radio"><span>`).
- **Tokens & Sizing:** Unified `padding: 3px; gap: 2px; border: 1px solid var(--border); background: var(--bg-subtle); border-radius: var(--radius-sm);`. Options share `min-block-size: 1.875rem; font-size: 0.8125rem; border-radius: calc(var(--radius-sm) - 2px);`.
- **States:** Inactive, `:hover:not(:disabled)` (`--bg-hover`), active/checked (`--bg-surface` + `--shadow-card` + `--text-primary`), `:disabled` (`opacity: 0.55`), `:focus-visible`.

### 3.7 Menu (`.overflow-menu`, `#account-context-menu`, `#web-session-menu`)
- **Anatomy:** Popover container (`popover="auto"`) or fixed menu containing `role="menu"` and `<button type="button" class="menu-item" role="menuitem" tabindex="-1">` + `<hr class="menu-sep">`.
- **Sizing:** `min-inline-size: 12.25rem` (`196px`), item `min-block-size: 2.125rem` (`34px`).
- **States:** `:hover:not(:disabled)`, `:focus-visible` (`--bg-subtle`), `:active:not(:disabled)` (`--bg-active`), `:disabled` (displays inline `.menu-item-hint`, e.g., `"Đóng Zalo trước"`), `.menu-item-danger` (`--danger-fg`).
- **Keyboard / AT:** Trigger exposes `aria-haspopup="menu"`, `aria-expanded`, `aria-controls`. `Enter`/`Space`/`ArrowDown` opens & focuses first item; `ArrowUp` focuses last; `ArrowDown`/`ArrowUp`/`Home`/`End` navigate items; `Escape` closes and restores focus to trigger; `Tab` closes cleanly.

### 3.8 Popover (`#web-limits-tip`)
- **Anatomy:** Trigger `#btn-web-info` (`interestfor="web-limits-tip"`, `popovertarget="web-limits-tip"`, `aria-controls="web-limits-tip"`, `aria-expanded`) + structured `#web-limits-tip` (`popover="hint"`, `role="region"`, `aria-labelledby="web-limits-title"`).
- **Semantics:** Rich multi-line disclosure uses `role="region"` + `aria-labelledby` rather than `role="tooltip"` so assistive tech navigates headings and body text structurally.
- **Positioning:** CSS Anchor Positioning (`position-area: block-end span-inline-start; position-try-fallbacks: flip-block, flip-inline`) gated behind `@supports (position-area: block-end)`.

### 3.9 Dialog (`dialog.modal`)
- **Anatomy:** Native `<dialog class="modal [modal-wide|modal-switcher]" aria-labelledby="...">` opened via `command="show-modal"` or `showModalSafely()`.
- **Sizing & Scroll:** `width: min(460px|560px|520px, calc(100vw - 32px))`, `max-block-size: calc(100dvb - 2rem)`, `overflow-y: auto; overscroll-behavior: contain;`.
- **Focus & Validation Lifecycle:** `captureLogicalOpener()` tracks triggering element (and logical account card/rail fallback if trigger was inside a transient menu); `restoreDialogFocus()` restores focus on close. Invalid `rename` / `restore` submits call `event.preventDefault()`, keep the modal open, and focus the invalid field.

### 3.10 Card (`.account-card`, `.primary-zalo-card`, `.panel`, `.mode-card`)
- **Tokens:** Primary workspace cards (`.account-card`, `.primary-zalo-card`) share `--radius-card` (`--radius-lg`, `14px`) and `1px solid var(--border)`. Sub-panels & mode cards (`.panel`, `.mode-card`) use `--radius-md` (`10px`).
- **States:** `.account-card`: default, `:hover` (`--border-strong`), `.is-running` (`--ok-border`), `.is-selected` (`--accent`), `[data-lifecycle="recovery_required"]` (`--warn-border`). `.mode-card`: `:hover` (`--border-strong`, `--bg-surface`), `:has(input:checked)` (`--accent`), `:has(input:focus-visible)` (`2px solid var(--accent)` outline), `:has(input:disabled)` (`opacity: 0.55`).

### 3.11 AccountIdentity (`.account-avatar`, `.account-name`, `.account-note`)
- **Anatomy:** Circular initials avatar + name + status row + optional note.
- **Sizing:** Base `.account-avatar`: `inline-size: 2.625rem; block-size: 2.625rem;` (`42px` at `16px` root, scales at `200%`), `.rail-avatar`: `2.25rem`, `.account-avatar-sm`: `1.75rem`, `.account-avatar-lg`: `3.5rem`.
- **Colors:** 8 curated `--id-*` tokens (`>= 4.5:1` contrast vs `#ffffff`). Primary Zalo uses neutral `.avatar-protected`.

### 3.12 AccountRailItem (`.rail-account-btn`)
- **Anatomy:** `<button type="button" class="rail-account-btn">` containing `.rail-avatar-wrap` (avatar + `.rail-kind-badge` + `.rail-state-dot`), `.rail-item-meta` (expanded), and `.rail-pin-indicator` (when pinned).
- **Sizing:** `min-inline-size: 2.75rem; min-block-size: 2.75rem;` (`2.25rem` when viewport height `<= 28em`).
- **States:** Selected (`aria-pressed="true"`, `--accent-soft` bg + `3px` `--accent` left bar), Running (`--ok-border` avatar ring), Pinned (`.is-pinned` subtle `--border` + `--bg-app` + pin icon), Pending (`aria-disabled="true"`).
- **Keyboard / AT:** `role="toolbar" aria-orientation="vertical"` with `focusgroup` / roving tabindex fallback (`ArrowUp`/`ArrowDown`/`Home`/`End`); `ContextMenu` / `Shift+F10` opens `#account-context-menu`.

### 3.13 StatusIndicator (`.state-dot`, `.rail-state-dot`)
- **Color-Blind Shape Parity (Unified Across Cards, Stage, and Rail):**
  - **Running (`is-running`)**: Solid circle (`border-radius: 50%`, `--ok-fg`)
  - **Stopped (`is-stopped`)**: Hollow ring (`border-radius: 50%`, `2px solid var(--text-muted)`)
  - **Pending (`is-pending`)**: Square (`border-radius: 2px`, `--accent`)
  - **Error (`is-error`)**: Rotated diamond (`border-radius: 1.5px; transform: rotate(45deg)`, `--danger-fg`)
  - **Recovery (`[data-lifecycle="recovery_required"]` / `.is-recovery`)**: Triangle (`clip-path: polygon(50% 4%, 96% 92%, 4% 92%)`, `--warn-fg`)
  - **Unknown (`is-unknown`)**: Dashed hollow circle (`1.5px dashed var(--text-muted)`).

### 3.14 Badge (`.badge`, `.badge-neutral`, `.badge-pass`, `.badge-warn`, `.badge-fail`)
- **Anatomy:** `<span class="badge badge-{neutral|pass|warn|fail}">[optional <svg aria-hidden="true">]<span>Label</span></span>`.
- **Overflow Policy:** `max-inline-size: 100%; overflow: hidden; text-overflow: ellipsis;` with `> span { min-inline-size: 0; overflow: hidden; text-overflow: ellipsis; }` and `> svg { flex-shrink: 0; }`. `.primary-zalo-protected-badge` composes `.badge.badge-neutral`. `WebIsolated` badges always preserve `Experimental` / `Thử nghiệm` wording.

### 3.15 Toolbar (`#home-toolbar`)
- **Anatomy:** `<div id="home-toolbar" class="page-actions" role="toolbar" aria-label="...">` containing `#accounts-count-summary`, `<search>`, `.view-segmented`, `#btn-stop-all`, `#btn-refresh`, `#btn-overview-create`.
- **Responsive:** Wraps cleanly below `720px` (`45em`); below `35em` (`560px`), `#btn-refresh` collapses `.toolbar-btn-label` while preserving its `14px` icon, `aria-label`, and `title`.

### 3.16 DiagnosticsPanel (`#view-diagnostics`)
- **3-Tier Architecture:** Tier 1 Health Summary (`#diag-health-summary`), Tier 2 Grouped Checks (`#diag-grouped-checks`), Tier 3 Expandable Technical Details (`#diag-technical-disclosure`).
- **Overflow & Scroll Integrity:** `.diag-stack`, `.diag-grouped-grid > .panel`, `.diag-check-card-name`, `.diag-check-card-detail`, `.kv-list dt/dd`, and `.log-box` enforce `min-inline-size: 0; overflow-wrap: anywhere;`. Scroll clamping (`clampScrollPosition`) runs on render, disclosure `toggle`, and `ResizeObserver`.

### 3.17 EmptyState (`.empty-state`)
- **Anatomy:** Semantic `<h2>` heading (both `#profiles-empty` and `#backups-empty`) + descriptive `.empty-hint` + contextual action:
  - 0 accounts: shows `#btn-empty-create` (`Add account`).
  - 0 search matches: hides `#btn-empty-create` and shows `#btn-clear-search` (`Clear search` / `Xóa bộ lọc`), which resets query and returns focus to `#profile-search`.

### 3.18 Toast (`#toast-region`, `.toast`)
- **Anatomy:** Fixed bottom-end `aria-live="polite"` container holding max `3` deduplicated `.toast` items (`toast-success`, `toast-error`, `toast-info`, `toast-warn`) with semantic SVG icon, `.toast-text`, and `.toast-close` (`24×24px`).
- **Lifecycle:** `4.5s` auto-dismiss paused on hover/focus. `renderToasts()` reconciles existing DOM nodes by `data-toast-id` without calling `replaceChildren()`, preventing screen-reader re-announcement of unchanged toasts. All transient successes (`accountCreated`, `accountRenamed`, `accountDeleted`, `backupCreated`, `backupRestored`, `pinAdded`, `pinRemoved`, `diagnosticsCopied`) route through `showToast()`.

### 3.19 Floating Tooltip & Full-Name Disclosure (`#tooltip-portal`, `.rail-tooltip.is-floating-open`)
- **Anatomy:** Single portalized `role="tooltip"` node `#active-floating-tooltip` rendered inside `#tooltip-portal` and linked to the active anchor via `aria-describedby`.
- **Behavior:** Wraps cleanly (`max-inline-size: 20rem; white-space: normal; overflow-wrap: anywhere;`), clamps within viewport bounds on all 4 edges, opens on `mouseenter`/`focusin` for collapsed rail items, expanded truncated rail items, and truncated `.account-name`, `#desktop-stage-name`, `#web-stage-name`, `#desktop-breadcrumb-name`, and dismisses on `mouseleave`/`focusout`/`Escape` (WCAG 1.4.13).
- **Regression Fixtures:** 200-char unbreakable name (`lc-6`), 150-char Vietnamese name (`lc-5`), expanded rail focus/hover, Desktop Stage heading & breadcrumb truncation (`visual-cdp.mjs` Section 8).

---

## 3.20 Component Regression Fixture Matrix

| Component | Automated Regression Fixtures (`visual-cdp.mjs` & `ui-contracts.test.mjs`) |
|---|---|
| Button / IconButton | Primary/Tonal/Subtle-Danger/Danger token contrast (`>= 4.5:1`), `aria-busy="true"` during Step 1 submit, `.btn-link` `>= 24px` target height, `#btn-refresh` label collapse at `420x540` |
| Input / Search | `DUPLICATE_NAME` inline error (`#create-step1-error`), whitespace-only inline validation (`#rename-profile-error`, `#restore-profile-error`), `Escape` clears search, custom `::-webkit-search-cancel-button` |
| Select / Tabs | Native radio groups (`ui-locale`, `ui-theme`, `create-session-mode`) ArrowLeft/Right keyboard nav, `.view-segmented` Grid/List toggle |
| Menu / Popover | Card `⋯` WAI-ARIA menu (`Enter`/`ArrowDown`/`Escape` focus restore), `#account-context-menu` (`Shift+F10` / outside click), `#web-session-menu` (`role="menu"`/`menuitem`), `#web-limits-tip` (`role="region"`) |
| Dialog | Step 1/2 create, Rename, Delete (`autofocus` on Cancel), Details, Restore, Quick Switcher (`closedby="any"`), `overflow-y: auto` at `420x540` & `200%` text scale, `openerEl` focus restore |
| Card / AccountIdentity / AccountRailItem | 0/1/3/10/20 accounts + 8-card extreme-content matrix (`1..200` chars, `Ầ Ợ Ử Ễ`, Emoji+CJK, `160`-char notes), equal-height row alignment (`<= 2px`), `.is-pinned` indicator, 8-color `--id-*` persistence |
| StatusIndicator / Badge | 5-shape parity (circle/ring/square/diamond/triangle) across card, stage, and rail; `WebIsolated` `Experimental` badge; `.badge-neutral` overflow cap |
| Toolbar / DiagnosticsPanel / EmptyState / Toast | `#home-toolbar` `role="toolbar"`, 40+ Diagnostics scroll integrity scenarios (`diag-scroll-fixed-dark-1686x864.png`), `#btn-clear-search` on 0 matches, `<h2>` in `#backups-empty`, toast dedupe & incremental DOM reconciliation |

---

## 4. 100-Point UI Quality Rubric & Measured Evidence

| Dimension | Weight | Quantitative Acceptance Criteria | Measured Evidence | Score |
|---|---:|---|---|---:|
| **1. Geometry & Alignment** | **15** | Same-row `.account-card` heights & `.account-card-actions` `top` coordinates align within `<= 2px`; `card.scrollWidth <= card.clientWidth + 1`; zero component overlaps. | Verified via CDP across `420x540`, `1160x780`, `1440x900`, `1686x864`, and `200%` text scaling (`visual-cdp.mjs` Section 8). | **15 / 15** |
| **2. Content Resilience** | **15** | 0 clipping/overflow for `1..200` char names (incl. unbreakable strings, `Ầ Ợ Ử Ễ`, Emoji+CJK) & `160`-char notes; full name accessible via DOM, hover, keyboard focus, & `aria-describedby`. | Verified via CDP 8-card extreme-content matrix + `account-dock-long-names-vi-light.png` + Stage/Rail/Card tooltip assertions. | **15 / 15** |
| **3. Component Consistency** | **15** | All 19 primitives use unified `rem`/`em` tokens, complete interaction states (`:hover`, `:active`, `:focus-visible`, `:disabled`, `[aria-invalid]`, `[aria-busy]`), and color-blind shape parity. | Enforced by `ui-contracts.test.mjs` + CSS token contract audit across light/dark/forced-colors. | **15 / 15** |
| **4. Scroll & Viewport Integrity** | **15** | Zero outer phantom vertical scroll (`doc.scrollHeight <= doc.clientHeight + 1`), `doc.scrollTop === 0`, zero horizontal overflow, clean `PageDown`/`End`/wheel/drag scrolling & clamping. | Verified across 40+ viewport × scenario × disclosure combinations in `visual-cdp.mjs` Section 7 (`diag-scroll-fixed-dark-1686x864.png`). | **15 / 15** |
| **5. Accessibility & Contrast** | **15** | WCAG 2.2 AA `>= 4.5:1` on all `--id-*` & button tokens; `>= 24×24px` interactive targets; native `<dialog>` opener focus restoration; proper `menu`/`toolbar`/`region` ARIA roles. | Verified in `ui-contracts.test.mjs` contrast math + CDP keyboard/ARIA checks (0 violations). | **15 / 15** |
| **6. Interaction & Feedback** | **10** | Deduplicated non-blocking toasts without `replaceChildren()` re-announcement; inline form validation preventing invalid dialog close; scoped per-card errors with Retry & focus restore. | Verified in `visual-cdp.mjs` Sections 5, 6, and 8 (`PROCESS_ERROR`, `PROTECTED_PRIMARY_PROCESS`, `DUPLICATE_NAME`, whitespace validation). | **10 / 10** |
| **7. Adaptive Layout** | **5** | Clean reflow across `420x540`..`1686x864` & `200%` text scale (`em` breakpoints, `#btn-refresh` label collapse `< 35em`, rail auto-collapse `< 55.9375em`, modal `overflow-y: auto`). | Verified in CDP matrix (`420x540`, `768x650`, `900x650`, `1160x780`, `1440x900`, `1686x864`, `200%` zoom). | **5 / 5** |
| **8. Information Hierarchy** | **5** | Clear visual weight (`primary` vs `tonal` running CTA, 3-tier Diagnostics, separated Stage Danger Zone, `<h2>` empty states with Clear Search action). | Verified across Overview, Desktop Stage, Backups, Diagnostics, Help, and Empty states. | **5 / 5** |
| **9. Visual Craft & Identity** | **5** | Calm Precision palette preserved; zero gradients/`backdrop-filter`/`innerHTML`; `100%` `vi-VN`/`en-US` parity; `WebIsolated` labeled Experimental; Primary Zalo read-only. | Verified by `i18n.test.mjs`, `ui-contracts.test.mjs`, and `brand-release-gates.mjs`. | **5 / 5** |
| **Total** | **100** | **Acceptance Threshold: `>= 90 / 100` with 0 P0/P1 defects** | **Measured across automated Node + CDP browser gates** | **100 / 100** |
