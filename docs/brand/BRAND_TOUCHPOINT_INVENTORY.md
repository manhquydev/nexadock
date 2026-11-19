# KHOANG — Brand Consistency & Touchpoint Inventory

> **Audit Scope:** Full repository (`c:\Users\manhquy\Downloads\zalo`) audited against **Locked Brand Decisions**:
> - **Product Name:** `KHOANG` (Technical short name: `khoang`, Positioning: `Independent Account Workspace`)
> - **Tagline (EN):** `Your accounts. Their own space.`
> - **Tagline (VI):** `Một máy. Nhiều tài khoản. Không trộn dữ liệu.`
> - **Locked Palette:** Primary `#17675F` · Brand Ink `#192A26` · Light Canvas `#F6F5F2` · Mint Highlight `#BCE5D8`
> - **Typography:** `Be Vietnam Pro` + platform-native fallbacks (`JetBrains Mono` for monospace)

---

## 1. Classification Summary

- **Class A — User-facing branding (MUST change to KHOANG):** Window titles, visible UI copy (`i18n` & static HTML fallbacks), app icons & tray icons, NSIS/MSI/DMG/DEB/AppImage display names & artifact filenames, `.desktop` metadata, GitHub Issue Template, active READMEs, and locked palette/tagline alignment.
- **Class B — Internal compatibility identifiers (REVIEW before changing — preserve data & contracts):**
  - `identifier`: `com.multizalodesktop.manager` in `tauri.conf.json` (changing breaks macOS bundle ID / Windows AppUserModelId / upgrade continuity unless migrated).
  - `APP_DIR_NAME`: `"MultiZaloDesktop"` in `crates/core/src/models.rs`, `ui-server.mjs`, `scripts/dim-host.ps1` (changing without migration orphans `%LOCALAPPDATA%\MultiZaloDesktop`, `~/Library/Application Support/MultiZaloDesktop`, `$XDG_DATA_HOME/MultiZaloDesktop`).
  - `localStorage` keys: `multi-zalo.theme`, `multi-zalo.locale`, `multi-zalo.accountColors` in `theme-init.js`, `app.js`, `i18n/index.js`, `ui-contracts.test.mjs`, `visual-cdp.mjs` (keep or read-migrate with fallback).
  - Process/env/crate identifiers: `MULTIZALO_PROFILE_ID`, `MULTIZALO_PROFILE_NAME`, `dim-desktop`, `dim-cli`, `dim-*` crates.
- **Class C — Historical documentation & archived PoC (ALLOWED to keep):**
  - `multi-zalo-desktop/` (PoC v1 historical archive).
  - `desktop-instance-manager/docs/architecture/adr-*.md`, `docs/research/*`, `docs/testing/*`, `docs/reviews/*`, `evidence/*.json`, `tests/windows-acceptance/evidence/*.json` (point-in-time historical records).
- **Class D — Third-party trademark / legal notices (MUST keep accurate):**
  - Non-affiliation notices stating `"Zalo"` is a trademark of VNG Corporation and KHOANG is independent (`MIT OR Apache-2.0`).
  - `LICENSE-MIT`, `LICENSE-APACHE`, `desktop-instance-manager/THIRD_PARTY_NOTICES.md` (note: update `LICENSE-MIT` / `LICENSE-APACHE` copyright line only if project maintainers authorize; keep VNG/Zalo disclaimers intact).

---

## 2. Gaps in Previous Batch vs. Locked Brand Decisions

| Area | Previous Batch State | Locked Brand Decision | Gap / Required Action |
| :--- | :--- | :--- | :--- |
| **Color Palette (`styles.css`)** | Light canvas `#f7f5f0`, text `#181816`, accent `#0f6b66` / `#147a74`, soft `#e6f4f2` | Light canvas `#F6F5F2`, Brand Ink `#192A26`, Primary `#17675F`, Mint Highlight `#BCE5D8` | **Needs Update** in `desktop-instance-manager/apps/desktop/ui/styles.css` (and `docs/prototype/`) |
| **Tagline VI (`vi-VN.json`)** | `"app.subtitle": "Mỗi tài khoản. Một không gian riêng."` | `"Một máy. Nhiều tài khoản. Không trộn dữ liệu."` | **Needs Update** in `desktop-instance-manager/apps/desktop/ui/i18n/vi-VN.json` (and `README.md`) |
| **Remaining "Multi-Zalo" in UI copy** | `help.cards.mentalModel.body`, `dialogs.delete.warning`, `dialogs.help.title`, `dialogs.help.items[1]`, `errors.EXTERNAL_DB_OUTSIDE_PROFILE`, `index.html:364` still say `Multi-Zalo` | `KHOANG` / `Khoang` | **Needs Update** in `vi-VN.json`, `en-US.json`, and `index.html` |
| **App & Tray Icons (`src-tauri/icons/`)** | 6 files (`32x32.png`, `128x128.png`, `128x128@2x.png`, `icon.png`, `icon.ico`, `icon.icns`) using `#0F6B66`/`#181816`; no `tray/` folder; no `16x16`/`20x20`/`24x24`/`48x48`/`64x64`/`256x256` | Locked `#17675F` / `#192A26` / `#F6F5F2` / `#BCE5D8` master + optical 16–24px + `tray/` icons | **Needs Regeneration** by `@brand` in Phase 2 |
| **Native Shell / Tray / Autostart** | `main.rs` has no `TrayIconBuilder`, no close-to-tray, no autostart, panic strings say `Multi-Zalo Desktop` | Tray + localized menu + transparent Autostart (off by default) + KHOANG strings | **Not Implemented** (Phase 3 `@tauri-native` scope) |

---

## 3. Complete Brand Touchpoint Inventory

| Surface | Current state | Expected KHOANG state | Source file | Operating system | Implementation status | Verification evidence |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **App UI — Window Title & Brand Header (HTML)** | `<title>KHOANG — Tài khoản</title>`, `#btn-brand-home` SVG mark & `KHOANG` | `KHOANG — Tài khoản` + Partitioned Bay SVG | `desktop-instance-manager/apps/desktop/ui/index.html` | All (Win/Mac/Linux) | **Done (prev batch)** | `read_file` lines 8, 20–31 |
| **App UI — Static HTML Fallback (`help` body)** | Line 364 still says `Multi-Zalo tạo các không gian...` | `KHOANG tạo các không gian...` | `desktop-instance-manager/apps/desktop/ui/index.html` | All | **Gap (Class A)** | `git grep` line 364 |
| **App UI — Theme Tokens & Palette** | `#f7f5f0`, `#181816`, `#0f6b66` / `#147a74` | `#F6F5F2`, `#192A26`, `#17675F`, `#BCE5D8` | `desktop-instance-manager/apps/desktop/ui/styles.css` | All | **Gap vs Locked** | `read_file` lines 20–59 |
| **App UI — `i18n` App Name & Tagline** | `app.name: "KHOANG"`, `vi-VN` subtitle `"Mỗi tài khoản. Một không gian riêng."`, `en-US` `"Your accounts. Their own space."` | `vi-VN` subtitle: `"Một máy. Nhiều tài khoản. Không trộn dữ liệu."` | `desktop-instance-manager/apps/desktop/ui/i18n/vi-VN.json`, `en-US.json` | All | **Partial (VI Tagline gap)** | `read_file` lines 1–13 |
| **App UI — `i18n` Body Copy (`Multi-Zalo` leftovers)** | 5 keys in `vi-VN.json` & `en-US.json` still say `Multi-Zalo` (`help.cards.mentalModel.body`, `dialogs.delete.warning`, `dialogs.help.title`, `dialogs.help.items[1]`, `errors.EXTERNAL_DB_OUTSIDE_PROFILE`) | Replace `Multi-Zalo` with `KHOANG` | `desktop-instance-manager/apps/desktop/ui/i18n/vi-VN.json`, `en-US.json` | All | **Gap (Class A)** | `git grep` lines 226, 278, 290, 293, 346 |
| **App UI — `localStorage` Keys** | `multi-zalo.theme`, `multi-zalo.locale`, `multi-zalo.accountColors` | Keep as Class B compatibility keys (or migrate with read fallback) | `ui/app.js`, `ui/theme-init.js`, `ui/i18n/index.js`, `ui/tests/ui-contracts.test.mjs` | All | **Preserve (Class B)** | `ui-contracts.test.mjs` lines 107–115 |
| **Tauri Config — Product & Window Title** | `productName: "KHOANG"`, `windows[0].title: "KHOANG"` | `productName: "KHOANG"`, `windows[0].title: "KHOANG"` | `desktop-instance-manager/apps/desktop/src-tauri/tauri.conf.json` | All | **Done (prev batch)** | `read_file` lines 3, 13 |
| **Tauri Config — Bundle Identifier** | `identifier: "com.multizalodesktop.manager"` | Review (Class B): keep for upgrade/data continuity or migrate explicitly | `desktop-instance-manager/apps/desktop/src-tauri/tauri.conf.json` | Windows / macOS / Linux | **Review (Class B)** | `read_file` line 5 |
| **Tauri Rust Shell — Panic / Log Strings** | `expect("failed to initialize Multi-Zalo Desktop state")` & `"error while running Multi-Zalo Desktop Tauri application"` | `KHOANG` | `desktop-instance-manager/apps/desktop/src-tauri/src/main.rs` | All | **Gap (Class A/B)** | `read_file` lines 9, 29 |
| **Tauri Cargo Metadata** | `name = "dim-desktop"`, `description = "...MultiZaloDesktop..."` | Update `description` to `KHOANG` (keep `dim-desktop` bin or review) | `desktop-instance-manager/apps/desktop/src-tauri/Cargo.toml` | All | **Gap (Class A/B)** | `read_file` lines 2, 7 |
| **Windows — App Icons (`.ico`, PNGs)** | Prev-batch Partitioned Bay (`#0F6B66`), missing `16/20/24/48/64/256` | Locked `#17675F`/`#192A26`/`#F6F5F2` master + `icon.ico` (16..256) | `desktop-instance-manager/apps/desktop/src-tauri/icons/` | Windows | **Needs Update** | `Get-ChildItem` icon sizes |
| **Windows — NSIS / MSI Installer & CI Artifacts** | `Multi-Zalo-0.1.0-beta.1-windows-x64-setup.exe`, `.msi`, `Multi-Zalo Desktop_*.exe` in `selftest.mjs` | `KHOANG-0.1.0-...-windows-x64-setup.exe` / `.msi` (driven by `productName: "KHOANG"`) | `.github/workflows/cross-platform-validation.yml`, `scripts/ci-cross-platform/selftest.mjs` | Windows | **Gap (Class A)** | `git grep` lines 605, 612; `selftest.mjs` 281–282 |
| **Windows — Authenticode / Smart App Control** | `UNSIGNED_DEVELOPMENT_ARTIFACT` (no EV/OV cert in repo) | Honest unsigned notice + Signed-Host Bridge (`ui-server.mjs`) | `README.md`, `.github/workflows/cross-platform-validation.yml` | Windows 10/11 | **Honest Limitation** | `README.md` line 72 |
| **macOS — `.app` Bundle, Dock, DMG & CI** | `Multi-Zalo-0.1.0-beta.1-macos-{arm64,x64}.dmg`, `Multi-Zalo-Desktop-macos-*.app`, `CFBundleName: Multi-Zalo Desktop` in `selftest.mjs` | `KHOANG.app`, `KHOANG-0.1.0-...-macos-*.dmg`, `icon.icns` | `.github/workflows/cross-platform-validation.yml`, `scripts/ci-cross-platform/selftest.mjs` | macOS | **Gap (Class A)** | `git grep` lines 780–983; `selftest.mjs` 313–335 |
| **Linux — `.deb`, `.AppImage`, `.desktop`, Tray** | `Multi-Zalo-0.1.0-beta.1-ubuntu24.04-amd64.deb`, `...AppImage`, `multi-zalo-desktop_*` in `selftest.mjs` | `khoang_0.1.0_amd64.deb` / `KHOANG-0.1.0-linux-x86_64.AppImage` | `.github/workflows/cross-platform-validation.yml`, `scripts/ci-cross-platform/selftest.mjs` | Ubuntu / Linux | **Gap (Class A)** | `git grep` lines 386–401; `selftest.mjs` 372–373 |
| **System Tray (`TrayIconBuilder` + Icons)** | Not implemented in `main.rs`; no `icons/tray/` directory | Tray icon (`tray-windows.ico/png`, `tray-macos-template*.png`, `tray-linux.png`) + localized menu + close-to-tray | `src-tauri/src/main.rs`, `src-tauri/icons/tray/` | Win / Mac / Linux | **Not Implemented** | `read_file main.rs` |
| **Notifications** | In-app status banner / toast only; Zalo OS toasts share VNG `com.vng.zalo` | KHOANG in-app notifications; document VNG toast grouping limit | `ui/app.js`, `README.md` | All | **Partial (In-app only)** | `README.md` line 144 |
| **Autostart** | Not implemented | Off by default, transparent toggle in Settings (Win Run key / macOS LaunchAgent / Linux `.desktop`) | `src-tauri/`, `ui/` | Win / Mac / Linux | **Not Implemented** | `Cargo.toml` / `main.rs` |
| **Profile Data Dir (`APP_DIR_NAME`)** | `"MultiZaloDesktop"` (`%LOCALAPPDATA%\MultiZaloDesktop`, etc.) | Preserve `"MultiZaloDesktop"` (Class B) or implement safe read-migration | `crates/core/src/models.rs`, `apps/desktop/ui-server.mjs`, `scripts/dim-host.ps1` | All | **Preserve (Class B)** | `git grep` `models.rs:44`, `ui-server.mjs:375` |
| **Process Env Vars** | `MULTIZALO_PROFILE_ID`, `MULTIZALO_PROFILE_NAME` | Preserve (Class B) | `crates/platform-windows/src/layout.rs`, `scripts/dim-host.ps1` | Windows | **Preserve (Class B)** | `git grep` `layout.rs:199` |
| **CI Workflows & Provenance Scripts** | `name: MultiZaloDesktop CI`, `Multi-Zalo Cross-Platform...`, `vendor: 'MultiZaloDesktop'` | Update display names/descriptions to `KHOANG` | `.github/workflows/ci.yml`, `cross-platform-validation.yml`, `scripts/ci-cross-platform/*.mjs`, `scripts/generate-build-provenance.mjs` | CI | **Gap (Class A)** | `git grep` |
| **Issue Template & Community Docs** | `.github/ISSUE_TEMPLATE/bug_report.md` (`Multi-Zalo Desktop`), `CONTRIBUTING.md`, `SECURITY.md` | Update to `KHOANG` | `.github/ISSUE_TEMPLATE/bug_report.md`, `CONTRIBUTING.md`, `SECURITY.md` | GitHub | **Gap (Class A)** | `git grep` |
| **Primary READMEs** | `README.md`, `README.en.md`, `desktop-instance-manager/README.md` updated to `KHOANG` header, still reference `Multi-Zalo-0.1.0-beta.1-*` download filenames & old `Multi-Zalo` alt text / table headers | Update download filenames & table headers to `KHOANG` once release artifacts rename | `README.md`, `README.en.md`, `desktop-instance-manager/README.md` | Docs | **Partial** | `read_file` / `git grep` |
| **Third-Party / Legal Notices** | Accurate VNG/Zalo non-affiliation in `README.md`, `README.en.md`, `desktop-instance-manager/README.md`, `THIRD_PARTY_NOTICES.md` | Keep accurate (Class D) | `README.md`, `README.en.md`, `LICENSE-*`, `THIRD_PARTY_NOTICES.md` | Docs / Legal | **Compliant (Class D)** | `read_file` |
| **Historical PoC & Evidence** | `multi-zalo-desktop/` & `desktop-instance-manager/docs/{architecture,research,testing,reviews}/`, `evidence/*.json` | Keep as historical record (Class C) | `multi-zalo-desktop/`, `desktop-instance-manager/docs/`, `evidence/` | Archive | **Allowed (Class C)** | `git grep` |
