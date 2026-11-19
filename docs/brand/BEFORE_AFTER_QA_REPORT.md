# KHOANG — Before/After QA & Regression Verification Report

> **Product:** KHOANG (*"Your accounts. Their own space."*) — upgraded from Multi-Zalo Desktop  
> **Scope:** Phase 2 Interactive Prototype (`docs/prototype/`) & Phase 3 Production UI + Rust/Tauri Workspace (`desktop-instance-manager/`)  
> **Verification Environment:** Windows (Node v26.13.1, Rust/Cargo workspace tests, Headless Chrome/Edge CDP)

---

## 1. Before / After Visual & UX Comparison

### Before (`desktop-instance-manager/tests/windows-acceptance/evidence/screenshots/`)
Observed directly from `01-empty-account-dock-light.png`, `04-account-dock-dark-mode.png`, and `06-recovery-required-badge.png`:
- **Brand & Surface Palette:** Generic "Multi-Zalo" branding with a standard blue (`#0068ff`-adjacent) accent on cool gray/white (`#f5f7fa`) or generic dark slate (`#181a1f`).
- **Empty State:** Showed a dashed-border box with a generic overlapping-squares icon and two separate primary/secondary affordances in the header and empty card.
- **Account Scaling:** Grid-only layout; no List view toggle for users managing 10–20+ accounts.
- **Primary Zalo Protection Card:** Displayed `Zalo đang dùng trên máy` with muted text, but lacked a dedicated "Protected · Read-only" badge pill.
- **Recovery State (`06-recovery-required-badge.png`):** Displayed a red diamond icon + `Cần mở lại để khôi phục` text, which could be mistaken for a fatal error rather than a safe automatic lock-cleanup action.
- **Bulk Stop:** No "Stop all" (`Đóng tất cả`) action when multiple account spaces were running simultaneously.

### After (`desktop-instance-manager/docs/design/screenshots/`)
Observed directly from `account-dock-empty-vi-light.png`, `account-dock-vi-VN-dark-1160x780.png`, `account-dock-3accounts-vi-light.png`, `account-dock-20accounts-vi-light.png`, `account-dock-vi-VN-light-420x540.png`, `account-dock-zoom-200-vi-light.png`, `account-dock-add-step2-vi-light.png`, `account-dock-diagnostics-vi-light.png`, and `account-dock-settings-vi-light.png`:
- **KHOANG Brand Identity ("Calm Precision"):**
  - Original **Partitioned Bay** SVG logo mark in the topbar and empty state (zero Zalo/VNG mark similarity).
  - Locked Warm Stone light canvas (`#F6F5F2` / `#ffffff`) and Warm Obsidian dark surfaces (`#121210` / `#1b1b18`) paired with Deep Teal primary (`#17675F` / hover `#12544D`), Brand Ink (`#192A26`), and Mint highlight (`#BCE5D8`).
  - `Be Vietnam Pro` + `JetBrains Mono` font stacks with Vietnamese-capable system fallbacks.
- **Desktop Behavior Settings (`#settings-behavior-section`):** Added `#toggle-keep-in-tray` and `#toggle-autostart` (both off by default) wired to Tauri IPC (`get_desktop_preferences`, `set_keep_in_tray`, `get_autostart_enabled`, `set_autostart_enabled`, `set_tray_locale`) with graceful disabled fallback + clear hint in HTTP browser preview mode.
- **Grid & List View Toggle:** Added `#btn-view-grid` / `#btn-view-list` (`Lưới` / `Danh sách`, persisted in `khoang.viewMode`) for comfortable scanning across 3–20+ accounts.
- **Bulk Stop Action:** Added `#btn-stop-all` (`Đóng tất cả` / `Stop all`) wired to `stop_all_profiles` Tauri IPC + HTTP bridge fallback when multiple instances are running.
- **Primary Zalo Read-Only Badge:** Added explicit `Được bảo vệ · Chỉ đọc` (`Protected · Read-only`) shield pill on `#primary-zalo-section`.
- **Honest Diagnostics & Status Badges:** Every status indicator pairs color + shape/icon + text label (`Cửa sổ đang mở`, `Đã đóng`, `ĐẠT (PASS)`, `CHƯA KIỂM TRA (NOT_TESTED)`, `BỊ CHẶN (BLOCKED)`).
- **Responsive & 200% Zoom Reflow:** Verified clean single-column reflow at `420x540` and `200%` zoom with `0` horizontal overflow (`scrollWidth <= clientWidth`) and `0` clipped buttons/labels.

---

## 2. Key Improvements & Remaining Limitations

### Key Improvements Verified
1. **CSS `[hidden]` Integrity:** Prototype review caught `.btn` and step containers overriding HTML `[hidden]`; fixed with `[hidden] { display: none !important; }` in both prototype and production UI.
2. **Test Suite Alignment:** Updated `desktop-instance-manager/apps/desktop/ui/tests/visual-cdp.mjs` line 787 to assert the new `'KHOANG — Hướng dẫn sử dụng'` document title while preserving all `multi-zalo.*` `localStorage` keys and IPC/HTTP bridge contracts.
3. **Zero Regressions Across Contracts:** All 13 Tauri IPC commands, HTTP bridge routes (`X-DIM-Session-Token`), `localStorage` persistence (`multi-zalo.locale`, `multi-zalo.theme`, `multi-zalo.accountColors`), `resolveAccountState()`, `resolveIsolationCheckResult()`, and WCAG 2.2 AA (`>= 4.5:1` for all 8 `--id-*` colors) pass.

### Remaining Limitations
- **Font Packaging:** Production UI uses the `Be Vietnam Pro` / `JetBrains Mono` font stack with Vietnamese-capable system fallbacks (`Segoe UI`, `SF Pro Text`, `Noto Sans`); offline `.woff2` font files are not yet bundled in `ui/fonts/` (documented in `docs/brand/assets/FONTS_PACKAGING.md` for optional follow-up).
- **Cross-Platform WebViews:** Headless Chrome/Edge CDP on Windows was executed directly; macOS `WKWebView` and Ubuntu `WebKitGTK` rendering were not executed on those host OSes in this session.
- **Live Zalo Desktop Process:** CDP verification runs against the mock HTTP/Tauri bridge (`visual-cdp.mjs`); live Zalo Desktop binary launch/focus was not executed in this headless run.

---

## 3. QA Execution Matrix

| Check / Surface | Status | Scope & Observed Result |
|---|---|---|
| `node --test desktop-instance-manager/apps/desktop/ui/tests/ui-contracts.test.mjs desktop-instance-manager/apps/desktop/ui/tests/i18n.test.mjs` | **PASS** | 21/21 tests passed, 0 failed, 0 skipped (HTML/CSS/JS contracts, i18n parity, `vi-VN` diacritics, `--id-*` contrast, `DimError` coverage). |
| `node desktop-instance-manager/apps/desktop/ui/tests/visual-cdp.mjs` | **PASS** | Exited 0: 16 matrix combinations, session token, theme reload, 0/1/3/10/20 accounts, Step 1/Step 2 resilience, color persistence/restore, primary Zalo honest states, timeout, Tauri invoke, Help view, WAI-ARIA menu, out-of-order status, `420x540`, `768x650`, `900x650`, `1160x780`, `200%` zoom, 0 runtime errors. |
| Phase 2 Prototype CDP Verification (`docs/prototype/`) | **PASS** | 23 scenarios verified via headless Chrome CDP (`01`–`23` in `docs/prototype/screenshots/`): 0 console/page errors, 0 horizontal overflow, 0 clipped elements, Cancel autofocus on delete, `Escape` closes `<dialog>`, visible Tab focus rings. |
| `node desktop-instance-manager/tests/brand-release-gates.mjs` | **PASS** | 5/5 passed (icons, tray icons, `tauri.conf.json`, `Cargo.toml`/`main.rs`, `README*.md` + VNG disclaimer). |
| `cargo fmt --all -- --check` & `cargo check --workspace --all-targets --offline` | **PASS** | Exited 0 in `desktop-instance-manager/` (0 formatting diffs, 0 compile errors). |
| `cargo test --workspace --offline` (`desktop-instance-manager/`) | **PASS** | Exited 0 after icon, tray, autostart, and `tauri.conf.json` updates across all workspace crates (`dim-core`, `dim-platform-linux`, `dim-platform-macos`, `dim-platform-windows`, `dim-process-manager`, `dim-profile-store`, `dim-test-fixture`, and `dim-desktop` 7/7). |
| `cargo build --manifest-path apps/desktop/src-tauri/Cargo.toml --release --offline` | **PASS** | Release `.exe` compiled cleanly (`Finished release profile [optimized]`). |
| Visual Screenshot Inspection (`read_file`) | **PASS** | Visually inspected Before (`01`, `04`, `06`), Prototype (`01`–`23` key states), and Production After (`empty`, `dark`, `3accounts`, `20accounts`, `420x540`, `zoom-200`, `add-step2`, `diagnostics`, `settings`) PNGs. |
| macOS `WKWebView` Live Render | **NOT_TESTED** | Not executed on macOS host in this Windows session. |
| Ubuntu `WebKitGTK` Live Render | **NOT_TESTED** | Not executed on Linux host in this Windows session. |
| Live Zalo Desktop Binary E2E | **NOT_TESTED** | Headless CDP uses mock bridge; live Zalo Desktop process not launched. |
