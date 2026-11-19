# KHOANG — Native OS Validation Report

> **Scope:** Tauri 2 native shell (`desktop-instance-manager/apps/desktop/src-tauri/`), System Tray, Autostart, Close-to-Tray, Bundle Metadata, and Cross-Platform Status.  
> **Host Environment:** Windows 10/11 x64 (`cargo` offline workspace + release build, Node v26.13.1, Headless Chrome/Edge CDP).

---

## 1. Validation Matrix (Executed vs. `NOT_TESTED`)

| Platform / Feature | Status | Evidence & Details |
|---|---|---|
| **Windows: Rust Formatting (`cargo fmt --all -- --check`)** | **PASS** | Executed in `desktop-instance-manager/` with 0 formatting diffs. |
| **Windows: Workspace Compile (`cargo check --workspace --all-targets --offline`)** | **PASS** | All workspace crates and targets compiled cleanly offline. |
| **Windows: Workspace Tests (`cargo test --workspace --offline`)** | **PASS** | All unit & integration tests passed, including `src-tauri` (7/7: `autostart`, `tray`, `preferences`, `commands`, `state`). |
| **Windows: Release Binary (`cargo build --manifest-path apps/desktop/src-tauri/Cargo.toml --release --offline`)** | **PASS** | Built release `.exe` with embedded KHOANG icons, `winreg` autostart, and `TrayIconBuilder`. |
| **Windows: System Tray & Menu (`src/tray.rs`)** | **PASS (Unit)** / **NOT_TESTED (Interactive GUI)** | `parses_locale_and_provides_localized_labels` passed; live Windows Explorer notification-area click/menu interaction requires an interactive desktop GUI session. |
| **Windows: Autostart (`src/autostart.rs`)** | **PASS (Unit)** / **NOT_TESTED (Live Registry Write)** | `formats_autostart_entries_with_khoang_branding` passed; live `HKCU\...\Run` write/boot test not executed in CI/headless session. |
| **Windows: Close-to-Tray (`keep_in_tray`)** | **PASS (Unit)** / **NOT_TESTED (Interactive GUI)** | `defaults_keep_in_tray_off_and_roundtrips_updates` passed; live `WindowEvent::CloseRequested` hide/restore requires interactive Tauri window. |
| **Windows: NSIS / WiX MSI Installer Packaging** | **NOT_TESTED** | `tauri.conf.json` bundle config validated; `cargo tauri build` NSIS/MSI bundler not executed in this offline session. |
| **Windows: Authenticode Code Signing** | **NOT_TESTED (Blocked)** | Blocked by absence of an EV/OV code-signing certificate in this environment. |
| **macOS: `.app` / `.dmg` / `WKWebView` / LaunchAgent / Template Tray** | **NOT_TESTED (Blocked)** | Blocked by absence of macOS hardware/runner in this Windows session; `platform-macos` & `autostart` plist formatting unit tests passed on host. |
| **macOS: Developer ID Signing & Notarization** | **NOT_TESTED (Blocked)** | Blocked by absence of Apple Developer ID certificate and macOS host. |
| **Linux (Ubuntu): AppImage / `.deb` / `WebKitGTK` / XDG Autostart / Tray** | **NOT_TESTED (Blocked)** | Blocked by absence of Linux host/display server in this Windows session; `platform-linux` unit/adapter suites & `.desktop` formatting tests passed on host. |

---

## 2. Explicit Blockers Requiring External Infrastructure
1. **Code Signing Certificates:** Windows Authenticode and macOS Developer ID signing + notarization cannot be verified without organization signing credentials.
2. **macOS & Linux Hardware/Runners:** Native `WKWebView` (macOS) and `WebKitGTK` + AppIndicator tray (Ubuntu) rendering and LaunchAgent/XDG autostart execution require `.github/workflows/cross-platform-validation.yml` or dedicated OS runners.
3. **NSIS / WiX Bundler Run:** Generating final `.exe` setup and `.msi` installers requires running `cargo tauri build` with NSIS/WiX toolchains installed.
