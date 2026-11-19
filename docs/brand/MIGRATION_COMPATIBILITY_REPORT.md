# KHOANG — Migration & Backward-Compatibility Report

> **Scope:** Upgrading from Multi-Zalo Desktop to **KHOANG** without losing existing user accounts, login sessions, backups, theme/locale preferences, or account color tags.

---

## 1. Compatibility Identifiers Preserved

| Identifier / Storage Path | Value | Status | Rationale & Safety Guarantee |
|---|---|---|---|
| **Tauri App Identifier** | `com.multizalodesktop.manager` | **Preserved** (`tauri.conf.json`) | Keeps the OS WebView2 / WKWebView / WebKitGTK storage origin and app data directory identical across upgrades so existing `localStorage` and session data remain intact. |
| **App Data Directory** | `MultiZaloDesktop` (`crates/core/src/paths.rs`) | **Preserved** | Existing `profiles.json`, `Profiles/<id>/`, and `Backups/` remain untouched at the same filesystem path; zero profile migration or re-login required. |
| **`localStorage` Keys** | `multi-zalo.locale`, `multi-zalo.theme`, `multi-zalo.accountColors` | **Preserved** (`app.js`, `theme-init.js`) | Existing user language, theme, and per-account badge color assignments survive upgrade seamlessly. |
| **New KHOANG Preferences File** | `<data_dir>/khoang-preferences.json` | **Added** (`src-tauri/src/preferences.rs`) | Stores `keep_in_tray` (default `false`) and `locale` (default `"vi-VN"`) atomically via temp-file + rename, independent of `profiles.json`. |
| **New UI View Mode Key** | `khoang.viewMode` (`localStorage`) | **Added** (`app.js`) | Defaults to `"grid"` when absent; never conflicts with `multi-zalo.*` keys. |

---

## 2. Upgrade, Reinstall & Uninstall Behavior

### In-Place Upgrade (Multi-Zalo Desktop -> KHOANG)
1. **Product Branding:** Window title, taskbar/Dock label, Start Menu/NSIS/MSI/DEB/AppImage metadata, and `productName` update to `KHOANG`.
2. **Account Data & Sessions:** Because `com.multizalodesktop.manager` and the `MultiZaloDesktop` data directory are preserved, all previously created account spaces, Zalo login sessions, notes, and backups load immediately on first launch.
3. **Desktop Behavior Defaults:** `keep_in_tray` and OS autostart both default to `false` (off), so upgrading never silently alters window close behavior or adds startup registry/LaunchAgent entries.

### Reinstall
- Reinstalling KHOANG over an existing installation preserves `<data_dir>/profiles.json`, `<data_dir>/Profiles/`, `<data_dir>/Backups/`, and `<data_dir>/khoang-preferences.json`.
- If the user previously enabled autostart (`HKCU\Software\Microsoft\Windows\CurrentVersion\Run\KHOANG` on Windows, `~/Library/LaunchAgents/com.multizalodesktop.manager.plist` on macOS, or `$XDG_CONFIG_HOME/autostart/khoang.desktop` on Linux), the entry remains valid as long as the executable path is unchanged; toggling autostart off/on in Settings updates the path cleanly.

### Uninstall
- **User Data Safety:** Uninstalling the application binary removes the program files only; user account spaces and backups in `<data_dir>` are not automatically wiped, preventing accidental loss of Zalo chat history or backups.
- **Autostart Cleanup:** Users who enabled autostart should toggle it off in Settings before uninstalling (or remove the single `"KHOANG"` Run value / LaunchAgent plist / `.desktop` autostart file).

---

## 3. Verification Evidence
- `node --test desktop-instance-manager/apps/desktop/ui/tests/ui-contracts.test.mjs` — **PASS** (verifies `multi-zalo.*` `localStorage` key preservation and Tauri/HTTP IPC contracts).
- `node desktop-instance-manager/tests/brand-release-gates.mjs` — **PASS** (verifies `tauri.conf.json` preserves `com.multizalodesktop.manager` while enforcing `productName: "KHOANG"`).
- `cargo test --manifest-path desktop-instance-manager/apps/desktop/src-tauri/Cargo.toml --offline` — **PASS** (`defaults_keep_in_tray_off_and_roundtrips_updates`, `formats_autostart_entries_with_khoang_branding`).
