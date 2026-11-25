# Contributing to NexaDock

Thank you for contributing to **NexaDock** (`desktop-instance-manager/`).

## Licensing (`Inbound = Outbound`)

All software source code in `desktop-instance-manager/` is dual-licensed under **MIT OR Apache-2.0** (`LICENSE-MIT` and `LICENSE-APACHE`).
By submitting a pull request, you agree that your contributions to software source code are licensed under **MIT OR Apache-2.0**.

> **Note:** Test execution logs and historical evidence files under `**/evidence/**` are informational artifacts and are excluded from the software license grant unless explicitly stated.

## Clean-Room & Safety Rules (Mandatory)

Pull requests will be rejected if they contain:
1. **Proprietary VNG / Zalo Assets:** Any extracted source code from `app.asar`, proprietary executables/libraries (`Zalo.exe`, `Zalo.app`, `ZaloCall.exe`, `*.asar`, `*.dll`), or copyrighted icons/assets.
2. **Binary Patching or Security Bypasses:** Code that modifies official Zalo binaries, disables Windows Smart App Control (`SAC`), disables macOS Gatekeeper, or passes `--no-sandbox` on Linux.
3. **Secrets, Credentials, or PII:** API keys, signing certificates (`.pfx`, `.pem`, `.p12`, `.tauri.key`), real Zalo cookies/databases, or unredacted local user home paths (`C:\Users\<username>\...`).

## Local Verification Before Opening a PR

`apps/desktop/src-tauri` and `tests/cross-platform` are **excluded** from the root Cargo workspace, so `--workspace` alone will miss Tauri and C2 harness regressions. Run all three manifests plus the Node gates from `desktop-instance-manager/` before pushing:

```sh
# 1. Workspace (core, platform adapters, process-manager, profile-store)
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --all-targets --locked

# 2. Tauri desktop crate (excluded from workspace)
cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets --locked -- -D warnings
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --locked

# 3. Cross-platform C2 harness (excluded from workspace; catches dim_core struct drift)
cargo fmt --manifest-path tests/cross-platform/Cargo.toml -- --check
cargo clippy --manifest-path tests/cross-platform/Cargo.toml --all-targets --locked -- -D warnings
cargo check --manifest-path tests/cross-platform/Cargo.toml --tests --locked

# 4. UI, i18n, and brand release gates
node --test apps/desktop/ui/tests/i18n.test.mjs apps/desktop/ui/tests/ui-contracts.test.mjs apps/desktop/ui/tests/vault-ui-contracts.test.mjs apps/desktop/ui/tests/vault-ui-parity.test.mjs tests/brand-release-gates.mjs
```

### Cross-OS `#[cfg]` & Clippy Hygiene

CI runs `clippy -D warnings` on **Ubuntu, Windows, and macOS**. When editing platform-gated code (`crates/platform-*`, `apps/desktop/src-tauri/src/{web_workspace,commands,autostart}.rs`):
- Gate OS-specific imports, helpers, and `mut` bindings under the matching `#[cfg(target_os = "...")]` (or `#[allow(dead_code)]` where intentionally shared).
- Avoid `let _ = unit_var;` on `#[cfg(not(target_os = "windows"))]` paths where the binding is `()` (`clippy::let_unit_value`).
