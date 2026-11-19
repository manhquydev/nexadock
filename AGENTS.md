# AGENTS.md — KHOANG Repository Context

## Hard Invariants (Never Violate)
- **Primary Zalo is strictly read-only**: never stop, kill, backup-over, snap (`SetWindowPos`), or mutate `ZaloData` / primary processes.
- **No invasive Win32 / client hacks**: zero `SetParent`, `WriteProcessMemory`, `CreateRemoteThread`, `SendInput`, DLL injection, binary patching, or unofficial Zalo protocol emulation.
- **Zero-IPC isolated WebViews**: `WebIsolated` child WebViews in `desktop-instance-manager/apps/desktop/src-tauri/src/web_workspace.rs` must use raw `wry::WebViewBuilder::build_as_child` without Tauri IPC, custom protocols, or filesystem/shell capabilities.
- **WebView2 profile lifecycle**: never use raw `fs::remove_dir_all` inside `EBWebView/`; manage deletion/reset through `ICoreWebView2Profile8::Delete()` and `deleting_profiles` / `pending_profile_deletes`.
- **UI & Branding**: preserve KHOANG Calm Precision (`rem`/`em` typography, zero `innerHTML`, zero gradients/`backdrop-filter`, 100% `vi-VN`/`en-US` parity). Keep `WebIsolated` labeled Experimental until live QR acceptance passes.

## Pre-Push CI Parity Gates
`apps/desktop/src-tauri` and `tests/cross-platform` are **outside** the root Cargo workspace. Before pushing any Rust or UI change, run the full check list in [CONTRIBUTING.md](CONTRIBUTING.md) (`workspace`, `src-tauri`, `tests/cross-platform`, and `node --test` UI/i18n/brand gates) and verify non-Windows `#[cfg]` / `clippy -D warnings` hygiene.

## Authoritative References
- Architecture decision & evidence matrix: [docs/architecture/ADR-001-unified-workspace.md](docs/architecture/ADR-001-unified-workspace.md)
- Contributor & pre-PR verification commands: [CONTRIBUTING.md](CONTRIBUTING.md)
