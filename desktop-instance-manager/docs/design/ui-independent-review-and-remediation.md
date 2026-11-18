# Independent UI Review, Hardening & Visual QA Report

## 1. Baseline & Scope
- **Baseline Commit**: `39ff5c7` on branch `feat/ui-vi-en-quiet-workspace`
- **Worktree**: `<WORKTREE_ROOT>/zalo-ui-worktree/desktop-instance-manager/`
- **Scope**: `apps/desktop/ui/**` and `docs/design/**` only. Zero backend, Rust, Tauri, PowerShell, or CI files modified.

## 2. Review Findings & Disposition (P0 / P1 / P2)
| Severity | Finding | Disposition | Evidence / Fix |
| :--- | :--- | :--- | :--- |
| P1 | Visual testing (`visual-cdp.mjs`) did not capture PNG screenshots or cover edge states/200% zoom, and exited 0 when browser missing | **Reproduced & Fixed** | Upgraded `tests/visual-cdp.mjs` to exit non-zero if browser missing, capture 18 PNG screenshots under `docs/design/screenshots/`, and verify 200% zoom (`580x390`). |
| P1 | Overflow menu lacked full WAI-ARIA Menu Button keyboard navigation | **Reproduced & Fixed** | Added `aria-haspopup="menu"`, `aria-expanded`, `aria-controls`, `Enter`/`Space`/`ArrowDown`/`ArrowUp`/`Home`/`End`/`Escape` handling, focus-in on open, focus-restore on close, and viewport-safe positioning in `app.js`. |
| P1 | Settings language & theme controls used `role="radio"` buttons instead of native `<input type="radio">` | **Reproduced & Fixed** | Converted both controls in `index.html` + `styles.css` to `<fieldset>` + `<legend>` + native `<input type="radio">` styled as segmented controls. |
| P1 | Async profile/global actions lacked per-profile & double-click pending guards | **Reproduced & Fixed** | Added `pendingProfileActions` (`Map`) and `pendingGlobalActions` (`Set`) in `app.js` with `finally` cleanup; launching profile A disables A while B remains operable. |
| P1 | `translateErrorCode()` concatenated raw English backend `res.message` into Vietnamese banner text, and active banners/dialogs did not re-translate on live language switch | **Reproduced & Fixed** | Split `resolveErrorInfo()` into clean localized `userMessage` + expandable `<details>` (`#status-banner-details`) for `rawCode`/`rawDetail`; stored `state.banner` and `state.openDialogContext` for live re-translation. |
| P1 | `import ... with { type: 'json' }` in `i18n/index.js` risks breaking older WebKitGTK / WKWebView | **Reproduced & Fixed** | Replaced with async `initI18n()` using `fetch()` under `connect-src 'self'` (with `node:fs` fallback in Node tests). |
| P2 | Profile row showed PID, Zalo User ID, and disk size inline, cluttering default row hierarchy | **Reproduced & Fixed** | Simplified row to profile name, state badge, optional short note, primary action, secondary Stop (if running), and overflow menu; technical metadata lives in `#profile-details-dialog`. |

## 3. File Changes
- `apps/desktop/ui/index.html`: `<fieldset>`/`<legend>`/`<input type="radio">` for Settings; expandable `#status-banner-details` inside `#status-banner`.
- `apps/desktop/ui/styles.css`: `.segmented-fieldset`, `.seg-option` radio styling with `:checked` and `:focus-visible`, `.banner-details`, `.overflow-menu` positioning.
- `apps/desktop/ui/i18n/index.js`: `initI18n()` via `fetch()`/`node:fs`, `resolveErrorInfo()` separating localized message from raw code/detail.
- `apps/desktop/ui/i18n/vi-VN.json` & `en-US.json`: Added `statusBar.loading`, `errors.technicalDetails`, `errors.rawCodeLabel`, `errors.rawMessageLabel`.
- `apps/desktop/ui/app.js`: `pendingProfileActions`, `pendingGlobalActions`, WAI-ARIA `buildOverflowMenu()`, simplified profile row, live banner/dialog re-translation, native radio listeners.
- `apps/desktop/ui/tests/i18n.test.mjs`, `ui-contracts.test.mjs`, `visual-cdp.mjs`: Expanded unit, contract, E2E, a11y, and screenshot checks.

## 4. Verification Results
- **Unit & Contract Tests** (`node --test apps/desktop/ui/tests/i18n.test.mjs apps/desktop/ui/tests/ui-contracts.test.mjs`):
  - **14 passed, 0 failed** (including placeholder parity between `vi-VN` and `en-US`, XSS interpolation, `NOT_TESTED`/`BLOCKED`/`UNKNOWN` preservation, no `import ... with { type: 'json' }`, native radio `<fieldset>` checks).
- **Headless Edge CDP E2E, A11y & Visual Matrix** (`node apps/desktop/ui/tests/visual-cdp.mjs`):
  - **PASS**: 16 viewport/locale/theme combinations (`vi-VN`/`en-US` × `light`/`dark` × `1160x780`, `900x650`, `768x650`, `420x540`), 200% zoom (`580x390`), WAI-ARIA menu keyboard flow, native radio arrow-key switching, per-profile async pending isolation, error banner `<details>` separation, all dialogs, 0 horizontal overflow, 0 runtime errors.
  - **Screenshots**: Saved in `docs/design/screenshots/` and visually inspected (`profiles-vi-VN-light-1160x780.png`, `profiles-vi-VN-dark-1160x780.png`, `profiles-vi-VN-light-420x540.png`).

## 5. Cross-Platform WebView Compatibility Matrix
| Capability | Windows WebView2 | macOS WKWebView | Ubuntu WebKitGTK | Status |
| :--- | :--- | :--- | :--- | :--- |
| `fetch()` JSON i18n (`initI18n`) | Supported | Supported | Supported | Tested in Edge CDP (Windows); `NOT_TESTED` on macOS/Linux |
| `light-dark()` + `@media (prefers-color-scheme)` fallback | Supported | Fallback via media query + `[data-theme]` | Fallback via media query + `[data-theme]` | Tested in Edge CDP (Windows) |
| Native `<dialog>` + `showModal()` | Supported | Supported | Supported | Tested in Edge CDP (Windows) |
| Popover overflow menu | Supported | Fallback `is-fallback-open` + outside-click | Fallback `is-fallback-open` + outside-click | Native tested in Edge CDP; fallback `NOT_TESTED` on WebKitGTK |

## 6. Outstanding Risks & Integration Notes
- Live Tauri WebView2 / Rust backend integration (`cargo tauri dev`) was not executed in this UI-only worktree; all browser E2E checks ran against a CSP-enforced mock HTTP bridge.
- macOS WKWebView and Ubuntu WebKitGTK rendering remain `NOT_TESTED` until run on those hosts.

---

## Final Acceptance — Commit 2b1ce6e (and Follow-up Hardening)

- **Reviewed Baseline**: `2b1ce6ef4d332a5927b527acd3c1f711ca357c7d`
- **GitHub Branch**: `https://github.com/manhquydev/zalo/tree/feat/ui-vi-en-quiet-workspace`
- **Scope Verified**: Only `apps/desktop/ui/**` and `docs/design/**` modified; zero changes to `crates/**`, `scripts/dim-host.ps1`, `apps/desktop/src-tauri/**`, `apps/desktop/ui-server.mjs`, or `.github/workflows/**`. Zero secrets or PII in diff or screenshots.

### Regressions & Edge Cases Found and Fixed
1. **i18n fetch resilience (`apps/desktop/ui/i18n/index.js`)**: Added `AbortController` timeout (`8000ms`), `Promise.allSettled` so one failed dictionary does not discard the other, JSON object validation, and a guard in `applyDomTranslations()` so static Vietnamese HTML is never overwritten with raw translation keys if both dictionaries fail to load.
2. **Out-of-order `refreshAll()` protection (`apps/desktop/ui/app.js`)**: Added `state.refreshSeq` so slower stale `/api/status` responses cannot overwrite newer state.
3. **Overflow menu across re-renders (`apps/desktop/ui/app.js`)**: Tracked `state.openMenuProfileId` and `menuWrap.isConnected` cleanup so re-rendering profiles while a menu is open preserves the open menu and avoids leaking document click listeners in non-Popover fallback mode.
4. **Exception banner sanitization (`apps/desktop/ui/app.js`)**: Replaced 5 `showRawBanner(err.message, 'error')` calls in `runProfileAction` and dialog `close` handlers with `showErrorBanner('UNKNOWN', err?.message || '')` so raw English/network error text stays inside expandable `<details>`.
5. **Test harness fixes (`apps/desktop/ui/tests/visual-cdp.mjs`)**: Fixed mock `rename` handler double-reading `req` stream and ensured `prof-2` is stopped before testing `launch` double-click deduplication.

### Test Results
- **Unit & Contract Tests** (`node --test apps/desktop/ui/tests/i18n.test.mjs apps/desktop/ui/tests/ui-contracts.test.mjs`): **15/15 passed, 0 failed** (including `initI18n` HTTP 404, invalid JSON, timeout, rapid `setLocale` switching, and no-overwrite fallback).
- **Headless Edge CDP E2E & Visual QA** (`node apps/desktop/ui/tests/visual-cdp.mjs`): **PASS** (16 viewport/locale/theme combinations, 200% zoom `580×390`, all 8 API routes/payloads recorded, double-click dedup, out-of-order `/api/status`, open overflow menu across re-render, WAI-ARIA menu keyboard nav, native radio fieldsets, error `<details>` separation, 0 horizontal overflow, 0 runtime errors).
- **Visual Inspection**: Inspected generated PNGs in `docs/design/screenshots/` (`profiles-vi-VN-light-1160x780.png`, `profiles-vi-VN-dark-1160x780.png`, `profiles-vi-VN-light-420x540.png`, `overflow-menu-vi-light.png`, `settings-vi-light-1160x780.png`, `backups-vi-light-1160x780.png`, `diagnostics-vi-light-1160x780.png`, `state-error-banner-vi-light.png`, `zoom-200-vi-light.png`). Layout, spacing, Vietnamese diacritics, focus rings, and contrast are clean and readable.

### Platform & Integration Status
| Target | Status | Notes |
| :--- | :--- | :--- |
| Windows Edge browser (CSP mock bridge) | **PASS** | Verified via `visual-cdp.mjs` + screenshot inspection |
| Windows Tauri WebView2 (live Rust core) | **NOT_TESTED** | Pending parallel backend session completion |
| macOS WKWebView | **NOT_TESTED** | No macOS host in this environment |
| Ubuntu WebKitGTK | **NOT_TESTED** | No Linux host in this environment |

### Verdict
**`READY_WITH_LIMITATIONS`** — UI, i18n, accessibility, async/race safety, and browser visual QA are verified and ready for integration; live Tauri WebView2, macOS WKWebView, and Ubuntu WebKitGTK runtime verification remain `NOT_TESTED` until merged with the backend build.
