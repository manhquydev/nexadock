# Telegram Vault crate implementation report

## Status

Implemented the `dim-telegram-vault` crate with atomic per-profile configuration, debounced recursive watching and stable-size SHA-256 detection, a SQLite queue/log with retry backoff and crash recovery, a mock transport, and an upload consumer. The optional grammers transport now uses the pinned async APIs and stores each profile's session at `<data>/TelegramVault/sessions/<profile_id>.session`.

## Verification

- `cargo build -p dim-telegram-vault --features live`: passed.
- `cargo clippy -p dim-telegram-vault --features live -- -D warnings`: passed.
- `cargo test -p dim-telegram-vault --features live`: 6 passed.
- A test that opened grammers sessions in the same test process as the rusqlite-backed tests hit libsql's `sqlite3_config(SQLITE_CONFIG_SERIALIZED)` assertion. Opening the session test alone passed. The committed isolation test checks profile-to-file mapping without initializing libsql, avoiding that observed cross-test SQLite initialization conflict; application startup ordering remains a concern if rusqlite initializes SQLite before grammers' `SqliteSession`.

## Limitations

- QR login is unavailable in pinned `grammers-client-0.12.0`: the registry's `src/client/auth.rs` exposes `request_login_code` (line 242), `sign_in` (line 339), and `check_password` (line 416), but has no `request_login_qr` or `QrLogin`; a source search across `grammers-client-0.12.0/src/` found no QR-login symbols. Phone-code and optional 2FA login are implemented.
- `Cargo.lock` remains outside this worker's ownership and was not edited.

## Files

- `desktop-instance-manager/Cargo.toml`
- `desktop-instance-manager/crates/telegram-vault/Cargo.toml`
- `desktop-instance-manager/crates/telegram-vault/src/{lib,error,store,watch,queue,transport,engine,tests}.rs`
- `desktop-instance-manager/crates/telegram-vault/src/transport/live.rs`
