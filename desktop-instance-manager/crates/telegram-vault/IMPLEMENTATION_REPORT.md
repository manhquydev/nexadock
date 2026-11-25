# Telegram Vault crate implementation report

## Status

Implemented the `dim-telegram-vault` crate with atomic per-profile configuration, a debounced recursive watcher with stable-size SHA-256 detection, SQLite queue and sync log, at-least-once crash recovery and retry backoff, a mock transport, a queue consumer, and an optional grammers MTProto transport. The live transport supports phone-code login with optional 2FA and isolated per-profile JSON file sessions; QR login is not implemented.

## Verification

- `cargo build -p dim-telegram-vault --features live`: passed.
- `cargo clippy -p dim-telegram-vault --features live -- -D warnings`: passed.
- `cargo test -p dim-telegram-vault --features live`: 8 passed (including FileSession round-trips).

## Session storage change (post-review)

`grammers-session`'s default `SqliteSession` backend is **libsql**, which bundles its own SQLite C library. Linked into the same binary as this crate's `rusqlite` (bundled SQLite), it produced duplicate `sqlite3_*` symbols and a `sqlite3_config(SQLITE_CONFIG_SERIALIZED)` initialization conflict. The live transport therefore uses `FileSession` (`src/transport/file_session.rs`): a custom `grammers_session::Session` implementation persisting `SessionData` as one atomic JSON file per profile (write-temp-then-rename), with `grammers-session`'s `serde` feature enabled and its `sqlite-storage` (libsql) feature disabled. Round-trip reopen tests cover home DC and peer-cache persistence.

For the same reason `rusqlite` was upgraded 0.32 → 0.41: `libsqlite3-sys < 0.39` bundles SQLite ≤ 3.53.2, affected by RUSTSEC-2026-0459 (CVE-2026-119375); 0.41 bundles patched SQLite 3.54.1. This raises the workspace `rust-version` floor to 1.89 (README updated accordingly) and one pre-existing `chunks_exact` call in `platform-linux/discovery.rs` was migrated to `as_chunks::<16>()`, which clippy's MSRV-aware lint now requires.

## Limitations

- QR login is unavailable in pinned `grammers-client-0.12.0`: the registry's `src/client/auth.rs` exposes `request_login_code`, `sign_in`, and `check_password`, but has no `request_login_qr` or `QrLogin`; a source search across `grammers-client-0.12.0/src/` found no QR-login symbols. Phone-code and optional 2FA login are implemented.

## Files

- `desktop-instance-manager/Cargo.toml`
- `desktop-instance-manager/crates/telegram-vault/Cargo.toml`
- `desktop-instance-manager/crates/telegram-vault/src/{lib,error,store,watch,queue,transport,engine,tests}.rs`
- `desktop-instance-manager/crates/telegram-vault/src/transport/live.rs`
- `desktop-instance-manager/crates/telegram-vault/src/transport/file_session.rs`
