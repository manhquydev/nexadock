# Windows Native Tauri WebView2 Smoke & Modern Web Guidance Audit Report

- **Section:** Section 3 — Native Tauri vs Browser Mock Acceptance & Modern Web Guidance Audit
- **QA Worktree & Branch:** `<WORKTREE_ROOT>/zalo-windows-acceptance` (`qa/windows-native-acceptance`)
- **Tested Integration Commit SHA:** `cd62c523222cb3a02051347e85313226de8b6964` *(fast-forwarded from `004ab7cd1b31f8dd9c0e1d9bdd659d8f6920df5d`)*
- **Automated Harness:** `desktop-instance-manager/tests/windows-acceptance/native-tauri-smoke.mjs`
- **Redacted JSON Evidence:** `desktop-instance-manager/tests/windows-acceptance/evidence/native-tauri-smoke-report.json`
- **Viewport Screenshots:** `desktop-instance-manager/tests/windows-acceptance/evidence/screenshots/01..06-*.png`
- **Track Automation Gate Status:** **`PASS`** (`11 PASS`, `0 FAIL`, `0 BLOCKED`, `1 NOT_TESTED`)
- **Candidate Workflow Markers:** `UNSIGNED_DEVELOPMENT_ARTIFACT` · `AWAITING_INTEGRATION_BUILD` · `AWAITING_MANUAL_L3`

---

## 1. Candidate Provenance & Release Governance Boundaries

1. **Integrated Candidate Under Test (`cd62c523222cb3a02051347e85313226de8b6964`):**
   - `dim-desktop.exe` (`<TEMP_DIR>/zalo-qa-win-acceptance-target/release/dim-desktop.exe`): `22,672,392` bytes, SHA-256 `2168cacfba423a9f7012b46040b3998d96cad24bd754b3464d13996bb3e1376d`, `is_installer: false`, `UNSIGNED_DEVELOPMENT_ARTIFACT`.
   - `dim-cli.exe` (`<TEMP_DIR>/zalo-qa-win-acceptance-target/release/dim-cli.exe`): `2,567,980` bytes, SHA-256 `6d741ddc3fa5cbfe89b4c9c0cbb19a18fbc4597d7cc52cabcf6aec5026f89008`.
   - Compiled from clean integrated SHA `cd62c523222cb3a02051347e85313226de8b6964` with `stable-x86_64-pc-windows-gnu` (`rustc 1.100.0`, `--locked`) into `%TEMP%` so zero `target/` build artifacts exist in the repository worktree.
2. **Verified Remediations vs Prior `004ab7c` Baseline:**
   - **`IR-NAT-01` (`VERIFIED_CLOSED` in `cd62c52` via `bff0674`):** `apps/desktop/ui/app.js` and `i18n/{vi-VN,en-US}.json` (`275` leaf keys each) now render the dedicated `recoveryRequired` badge (`"Cần mở lại để khôi phục"` / `"Reopen to recover"`) with a rotated diamond indicator (`06-recovery-required-badge.png`) and disable offline Backup/Delete menu items whenever `lifecycle_state === "recovery_required"`.
   - **`IR-NAT-02` (`VERIFIED_CLOSED` in `cd62c52` via `bff0674`):** `apps/desktop/src-tauri/src/commands.rs` now honors `DIM_NODE_FIXTURE_SCRIPT` across `get_status`, `launch_profile`, and `focus_profile`, enabling direct end-to-end fixture process launch and focus over live WebView2 -> Rust Tauri 2 IPC.
3. **Release Captain Installer & L3 Boundaries:**
   - Release Captain's official Windows NSIS (`*-setup.exe`) and WiX (`*.msi`) installer bundles remain **`AWAITING_INTEGRATION_BUILD`** (`NOT_BUILT_YET`). Executing raw `dim-desktop.exe` validates the native Tauri 2 + WebView2 runtime, Rust IPC, and Account Dock UX, **not** Windows installer setup/upgrade/uninstall mechanics (covered in Section 4 `installer-validation.md`).
   - Live 2-account Official VNG Zalo QR login and real Zalo GUI window foreground switching remain **`AWAITING_MANUAL_L3`** (covered in Section 5 `l3-manual-test-checklist.md`).

---

## 2. Explicit Surface Separation: Browser Mock vs Native Tauri

Automated static UI previews and native desktop execution are strictly separated in `native-tauri-smoke.mjs`:

| Dimension | Browser Mock (`BROWSER_MOCK_STATIC_HTTP_PREVIEW`) | Native Tauri (`NATIVE_TAURI_WEBVIEW2_EXE`) |
| :--- | :--- | :--- |
| **Runtime Binary** | Headless `msedge.exe` against ephemeral static Node HTTP server | Compiled `dim-desktop.exe` + Microsoft Edge WebView2 Evergreen (`157.0.4341.46`) |
| **Origin** | `http://127.0.0.1:51283` (ephemeral localhost port) | `http://tauri.localhost` (Tauri 2 custom asset protocol) |
| **`window.__TAURI_INTERNALS__`** | `false` (`undefined`) | `true` (`typeof window.__TAURI_INTERNALS__.invoke === 'function'`) |
| **`window.__TAURI__.core.invoke`** | `false` (`undefined`) | `true` (`withGlobalTauri: true` in `tauri.conf.json`) |
| **Backend Transport** | Falls back to `fetch('/api/...')`; shows `"Chưa kết nối"` pill + plain-language `HOST_EXEC_FAILED` banner when no host bridge is running | Direct Rust IPC into `tauri::Builder` `invoke_handler` (`13` async/sync commands backed by `AppState` + `InstanceCoordinator`) |
| **Settings Connection Label** | `Chưa kết nối` / `Kết nối cục bộ` (`modeBridge`) | `Đã kết nối (Ứng dụng máy tính)` / `Connected (Desktop app)` (`modeTauri`) |
| **What It Proves** | HTML/CSS layout, i18n dictionary loading, client-side fallback banner rendering | Real Win32 HWND creation, Tauri CSP enforcement, WebView2 rendering, Rust `JsonProfileStore` persistence, Restart Manager lockfile inspection, and manager crash/relaunch `.runtime-lease.json` reconciliation |

---

## 3. Section 3 Acceptance Matrix & Measured Timings (`NT-01` – `NT-12`)

### Startup, Shutdown & Rust IPC Latency Summary

| Metric | Measured Value | Evidence Source |
| :--- | :---: | :--- |
| **Host OS & DPI** | `Windows 11 Pro 10.0.26300` (`120 DPI` / `1.25x` native DPR) | Win32 `GetDpiForWindow` + WMI |
| **WebView2 Evergreen Runtime** | `157.0.4341.46` (`Edg/157.0.4341.46`) | `HKLM\...\EdgeUpdate\Clients\{F3017226-...}` + `Browser.getVersion` |
| **Cold Launch -> WebView2 CDP Target Ready** | `695.69 ms` | `NT-01` isolated cold benchmark |
| **Cold Launch -> Win32 HWND + Hydrated Account Dock** | `1,738.92 ms` | `NT-01` (`hwnd: 69469452`, client `1160x780`, window `1174x818`) |
| **Graceful Window Close (`WM_CLOSE`) -> Clean Exit (`0`)** | `499.85 ms` (`0` orphans) | `NT-01` `CloseMainWindow()` + process table diff |
| **Tauri IPC `get_status` Round-Trip** | `14.00 ms` | `NT-02` `window.__TAURI_INTERNALS__.invoke('get_status')` |
| **Tauri IPC `get_diagnostics` / `list_backups`** | `3.20 ms` / `0.80 ms` | `NT-02` live WebView2 `performance.now()` |
| **Tauri IPC `verify_isolation`** | `17.20 ms` | `NT-02` live isolation check suite |

### 12-Point Native Acceptance Results

| ID | Acceptance Check | Gate Status | Key Empirical Evidence |
| :---: | :--- | :---: | :--- |
| **`NT-01`** | **WebView2 runtime availability & native window creation + clean exit** | **`PASS`** | Evergreen `157.0.4341.46` detected; `dim-desktop.exe` created visible Win32 `HWND` (`1160x780` client area matching `tauri.conf.json`), hydrated `http://tauri.localhost` with `0` JS exceptions, and exited cleanly (`code: 0`) on `WM_CLOSE` in `499.85 ms` with **`0` orphan `dim-*` or `msedgewebview2.exe` processes**. |
| **`NT-02`** | **Tauri IPC (`window.__TAURI_INTERNALS__` / `invoke` vs browser fallback)** | **`PASS`** | Verified `window.__TAURI_INTERNALS__.invoke` and `window.__TAURI__.core.invoke` live on `http://tauri.localhost`; Settings -> About reports `"Đã kết nối (Ứng dụng máy tính)"`; contrasted directly against Browser Mock (`http://127.0.0.1:51283`, `hasTauriInternals: false`, `"Chưa kết nối"` pill). |
| **`NT-03`** | **Frontend loading, CSP enforcement & Account Dock initial render** | **`PASS`** | Tauri CSP (`script-src 'self'`) actively blocked injected inline `<script>` (`securitypolicyviolation`: `script-src-elem`, `blockedURI: "inline"`). Clean `DIM_DATA_DIR` renders empty-state onboarding hero, progressive-disclosure hides search (`< 2` accounts), and renders read-only `"Zalo đang dùng trên máy"` (`0` interactive stop/delete buttons on primary Zalo). |
| **`NT-04`** | **Keyboard navigation (`Tab`, `:focus-visible`, `Space`/`Enter`, `Escape`, focus trap)** | **`PASS`** | Semantic `<header>`, `<nav aria-label>`, single `<main id="main-content" tabindex="-1">`, `.skip-link`, `0` positive `tabindex`, `0` focusable elements inside `aria-hidden`, `0` unlabeled inputs. Real CDP `Tab` sequence reaches all controls with `:focus-visible` `solid` outline (`5.38:1` contrast vs `3.0:1` minimum). `Space` opens `#create-profile-dialog` (`:modal`), `12` `Tab` cycles stay `100%` contained (`0` escapes to inert `.app-shell`), `Escape` closes and restores focus to `#btn-open-create`; card `⋯` menu supports `ArrowDown`/`Escape` focus restore. |
| **`NT-05`** | **Native/DOM `<dialog>` modals (add, rename, backup/restore, delete)** | **`PASS`** | All `5` `<dialog>` elements omit redundant `role`/`aria-modal`, bind `aria-labelledby`, enforce `overscroll-behavior: contain` on `dialog:modal` & `dialog::backdrop` without locking `html`/`body` overflow, and reset `dialog.returnValue` to `""` on close. Verified live Tauri IPC 2-step Add Account, Rename, Offline Backup + Restore-as-New-Account, and Delete Confirmation (`Cancel` button carries `autofocus`; `Escape` keeps all `3` profiles intact; confirming `delete` removes target cleanly). |
| **`NT-06`** | **Window focus & multi-window handoff behavior** | **`PASS`** | Launched `2` concurrent isolated Node fixtures (`PID 34564`, `PID 26796`) **directly via live Tauri 2 IPC `launch_profile`** (`DIM_NODE_FIXTURE_SCRIPT`); Tauri UI live-rendered `"Đang mở 2 cửa sổ"` with `"Chuyển đến"` + `"Đóng Zalo"` per card. Invoked live Tauri `focus_profile` on Profile 1 -> verified isolated `second-instance` focus handoff (`focus_count >= 1` on Profile 1 while Profile 2 stayed `0`), then clicked `"Đóng Zalo"` on Profile 2 -> stopped Profile 2 while Profile 1 survived untouched. Official VNG Zalo 2-window live foreground switching remains `AWAITING_MANUAL_L3`. |
| **`NT-07`** | **System tray (`tauri.conf.json` / `Cargo.toml` / `src-tauri`)** | **`NOT_TESTED`** | Verified honestly against source: `tauri.conf.json` defines no `app.trayIcon`, `src-tauri/Cargo.toml` sets `tauri = { version = "2", features = [] }` (no `tray-icon` feature), and `src/main.rs` registers no `TrayIconBuilder`. System tray is **not implemented in v0.1.0 MVP** by design: closing `dim-desktop.exe` exits the manager (`0` background tray resident) while active Zalo windows keep running independently. |
| **`NT-08`** | **Light / Dark mode (`prefers-color-scheme`, toggle & WCAG contrast)** | **`PASS`** | `<meta name="color-scheme" content="light dark">` + blocking `<head>` `./theme-init.js` + `:root { color-scheme: light dark }` prevent FOUC and theme native scrollbars/controls. Emulated OS `prefers-color-scheme` updates tokens and dynamic `"Theo hệ thống (Sáng)"` / `"Theo hệ thống (Tối)"` label live; explicit Dark override sets `data-theme="dark"` and `meta[content="dark"]`. Measured WCAG 2.2 AA contrast passes across Light (`17.6:1` primary text, `6.84:1` secondary, `4.71:1` muted, `5.38:1` focus ring) and Dark (`14.89:1` primary, `7.74:1` secondary, `5.26:1` muted, `6.03:1` focus ring), plus all `8` `--id-*` avatar swatches on `#ffffff` (`5.02:1` – `7.90:1`), shape+text state indicators, and `@media (forced-colors: active)` rules. |
| **`NT-09`** | **Tiếng Việt (`vi-VN`) / English (`en-US`) completeness & zero jargon** | **`PASS`** | `100%` dictionary parity (`275` leaf keys in `vi-VN.json` and `275` in `en-US.json`, `0` missing keys). Switching locale updates `<html lang="vi"|"en">` and `<title>` immediately while preserving user-entered Vietnamese diacritic account names (`"Đức — Công việc QA"`). Full-text scan across `Accounts`, `Backups`, `Settings`, and `Help` confirmed **`0` raw technical leaks** (`PID`, `IPC`, Rust panics, `CreateProcessW`, `rstrtmgr`, `lockfile`, `.runtime-lease`, raw error codes); technical diagnostics stay progressively disclosed inside `Settings -> Advanced -> Troubleshoot` and collapsed `<details>`. |
| **`NT-10`** | **HiDPI & Windows scaling (`100%`, `125%`, `150%`, `200%`)** | **`PASS`** | Tested `1.0x` (`1160x780`), `1.25x` (`928x624`), `1.5x` (`774x520`), and `2.0x` (`800x600`) DPR tiers plus `200%` page zoom reflow: **`0 px` horizontal overflow** across all tiers, `0` clipped CTAs, and interactive button heights `>= 32 px` (`>= 24x24` WCAG 2.5.8 target minimum). Logged non-blocking CSS `rem` recommendation (`IR-NAT-03`) for `html, body { font-size: 14px }`. |
| **`NT-11`** | **Small window (`<= 800x600`, `640x560` & `420x540` min-size)** | **`PASS`** | Resized native Win32 window + viewport to `800x600`, `640x560`, and `tauri.conf.json` minimum `420x540`: **`0 px` horizontal overflow** at every size; `.account-grid` collapses cleanly from `2` columns (`800x600`) to `1` column (`640x560`, `420x540`); `.topbar` unpins from `sticky` to `static` below `720px` so account cards retain vertical room; `#create-profile-dialog` (`width: min(460px, calc(100vw - 32px))`) fits inside `420x540` without clipping. |
| **`NT-12`** | **Recovery when manager app crashes (`kill` -> relaunch reconciliation & `recoveryRequired` UX)** | **`PASS`** | **12A (Manager Crash):** Abruptly terminated `dim-desktop.exe` (`taskkill /F`) while Profile 1 (`PID 33008`) was running -> Profile 1 survived untouched; relaunched `dim-desktop.exe` reconciled `.runtime-lease.json` -> `running: true`, `lifecycle_state: "running"`, `main_pid: 33008`, rendering `"Cửa sổ đang mở"` + `"Chuyển đến"`. **12B (Fixture Crash & `recoveryRequired` UX):** Hard-killed Profile 2 -> `stale_lockfile_detected: true` detected and auto-cleaned on next launch (`crash_recovered: true`); injected corrupted `.runtime-lease.json` -> Rust `get_status` emitted `lifecycle_state: "recovery_required"`, safely blocked `backup_profile` & `delete_profile` (`PROFILE_RUNNING`), disabled offline Backup/Delete menu items, and rendered the dedicated recovery badge `"Cần mở lại để khôi phục"` (`vi-VN`) / `"Reopen to recover"` (`en-US`) (`06-recovery-required-badge.png`). |

---

## 4. Open Non-Blocking Polish & Closed Prior Integration Items

| ID | Severity | Component | Status on `cd62c52` | Detail / Recommendation |
| :---: | :---: | :--- | :---: | :--- |
| **`IR-NAT-01`** | `P2` | `apps/desktop/ui/{app.js,i18n/*.json}` | **`VERIFIED_CLOSED`** | Fixed in `bff0674` / verified on `cd62c52`: `recoveryRequired` badge (`"Cần mở lại để khôi phục"` / `"Reopen to recover"`) + unclean menu guard active. |
| **`IR-NAT-02`** | `P3` | `apps/desktop/src-tauri/src/commands.rs` | **`VERIFIED_CLOSED`** | Fixed in `bff0674` / verified on `cd62c52`: `DIM_NODE_FIXTURE_SCRIPT` launch & focus verified live via WebView2 -> Tauri 2 IPC. |
| **`IR-NAT-03`** | **`P3`** | `apps/desktop/ui/{index.html,styles.css}` | **`OPEN (NON_BLOCKING_POLISH)`** | Replace `html, body { font-size: 14px }` fixed-pixel root font sizing with browser-inherited root sizing (`rem` scale, e.g. `0.875rem` body text) + `<meta name="text-scale" content="scale">` so OS-level accessibility font scaling works alongside Windows DPI scaling. |

---

## 5. Host Safety, Primary Zalo Non-Interference & PII Gate

- **Primary Zalo Config SHA-256 Invariance:** `%APPDATA%\ZaloData\config.json` SHA-256 remained `be8077e13b0f59e6976a7baa549b836a46569ca397f2bdf5d3e0f3a46be07734` before, during manager crash, and after suite completion (`configSha256Identical: true`, `missingPrimaryPids: []`).
- **Smart App Control Integrity:** Host `VerifiedAndReputablePolicyState = 0` inspected read-only (`smartAppControlModified: false`).
- **Zero Orphan Processes:** `0` leftover `dim-desktop.exe`, `dim-cli.exe`, `node.exe` fixture, or `msedgewebview2.exe` processes after suite teardown.
- **PII Scrubbing:** Automated `assertZeroPiiLeaks` verified `0` occurrences of personal usernames, machine hostnames, raw Windows user profile paths, or session tokens in `native-tauri-smoke-report.json`, this report, and all `6` viewport screenshots.

---

### Modern Web Guidance

- **`dialog`**: I audited all five `<dialog>` elements in `native-tauri-smoke.mjs` to confirm modal invocation via `showModal()`, absence of redundant `role="dialog"`/`aria-modal`, `returnValue` reset on `close`, scroll-chaining containment on `dialog:modal` and `dialog::backdrop`, and `autofocus` on `Cancel` inside `#delete-profile-dialog` ("put autofocus on the safe action (such as Cancel or Keep) in a destructive confirmation.").
- **`platform-controls-dismiss-dialog`**: I verified via real CDP `Escape` key dispatch that modals opened with `showModal()` and `command="show-modal"` honor platform close requests without requiring custom `Esc` listeners ("When opened in a modal state, a dialog without the `closedby` attribute responds to close requests the same as explicitly setting the `closedby` attribute to `closerequest`.").
- **`accessibility`**: I added automated DOM assertions verifying zero positive `tabindex` values, zero focusable descendants inside `aria-hidden="true"`, explicit form labels, single `<main tabindex="-1">` landmark, and `>= 3:1` custom focus indicator contrast ("Never apply `tabindex="1"` (or any value > 0) to elements, as doing so imposes a tab order on the page that bears no resemblance to the intended or expected tab order.").
- **`natural-interactions`**: I verified `:focus-visible` outline rendering (`5.38:1` contrast) during sequential `Tab` navigation and keyboard-only popover menu operation via `ArrowDown`/`Escape` so keyboard users always see clear focus feedback without mouse-click noise ("Use `:focus-visible` so keyboard users always see a clear focus ring, without showing it on mouse clicks.").
- **`dark-mode`**: I verified `<meta name="color-scheme" content="light dark">` in `<head>`, `:root` `color-scheme: light dark`, `light-dark()` token resolution, and live `matchMedia("(prefers-color-scheme: dark)")` updates ("Apply the `color-scheme` property to the `html` element or the `:root` pseudo-class.").
- **`dark-mode-toggle`**: I audited synchronous `<head>` execution of `theme-init.js` for FOUC prevention and `:root:has(> head > meta[name="color-scheme"])` CSS synchronization across System, Light, and Dark modes ("**DO NOT** hardcode `color-scheme: light` or `color-scheme: dark` as the root default; the base declaration MUST stay `light dark`.").
- **`color`**: I measured resolved RGB luminance ratios across Light and Dark themes (`>= 4.5:1` text, `>= 5.02:1` avatar palette on white, `>= 5.38:1` focus rings) and confirmed shape-plus-text state indicators alongside `@media (forced-colors: active)` fallbacks ("Do not denote success/errors ONLY with color. Use icons or text (WCAG 1.4.1 Use of Color).").
- **`css-layout`**: I tested native Win32 window resizing at `800x600`, `640x560`, and `420x540` with long Vietnamese profile names to confirm `min-width: 0` flex/grid shrinkage, single-column collapse, and `0px` horizontal overflow ("Set `min-inline-size: 0` (or `min-width: 0`) on flex items that contain long unbreakable content (URLs, code, long strings) — flex items won't shrink below their content size by default, causing overflow.").
- **`respect-os-text-scale`**: I verified `100%–200%` DPI scaling plus `200%` zoom with `0px` horizontal overflow and unclipped CTAs, and logged `IR-NAT-03` because `styles.css` sets a fixed `14px` root font size ("Do not set a custom font size on `:root` or `html`.").
