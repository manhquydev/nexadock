# Contributing to Multi-Zalo Desktop

Thank you for contributing to Multi-Zalo Desktop (`desktop-instance-manager/`).

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

Run from `desktop-instance-manager/`:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --all-targets --locked
node apps/desktop/ui/tests/i18n.test.mjs
node apps/desktop/ui/tests/ui-contracts.test.mjs
```
