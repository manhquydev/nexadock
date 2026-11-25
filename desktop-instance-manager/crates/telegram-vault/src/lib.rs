//! Zalo received-file synchronization into a Telegram account's Saved Messages.
//!
//! The optional live transport uses grammers, a pure-Rust MTProto implementation, rather than
//! TDLib/tdjson: it avoids a C++ build toolchain and native-library packaging across Windows
//! MSVC, macOS arm64, and Linux. No client modification or injection is performed.

pub mod engine;
pub mod error;
pub mod queue;
pub mod store;
pub mod transport;
pub mod watch;

#[cfg(test)]
mod tests;

pub use error::{Result, VaultError};
