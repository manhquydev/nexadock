# Ubuntu / Linux Platform Adapter Test Suite

This directory contains the runtime multi-instance fixture harness for `dim-platform-linux`, paired with the Cargo integration test suite in [`../../crates/platform-linux/tests/ubuntu_adapter_suite.rs`](../../crates/platform-linux/tests/ubuntu_adapter_suite.rs).

## Running on Ubuntu 24.04 LTS / 22.04 LTS

```bash
cd desktop-instance-manager

# 1. Compile, lint, and execute Rust unit + 16 integration tests (including native Linux launch/stop)
cargo fmt --package dim-platform-linux -- --check
cargo clippy -p dim-platform-linux --all-targets --locked -- -D warnings
cargo test -p dim-platform-linux --locked

# 2. Execute the 15-scenario Node.js multi-instance XDG/HOME fixture suite
node --check tests/platform-linux/run-ubuntu-fixture-tests.mjs
node tests/platform-linux/run-ubuntu-fixture-tests.mjs
```

## Acceptance Status Semantics

The test runner strictly separates verification states and never collapses them into a single `"Supported"` badge:

- `LINUX_ADAPTER_TESTED`: Emitted as `PASS` **only** when executed natively on a Linux kernel (`process.platform === 'linux'`).
- `DESKTOP_FIXTURE_TESTED`: Emitted as `PASS` when `fixture-app.mjs` multi-instance XDG/HOME/lockfile/recovery scenarios pass on Linux (`PORTABLE_HOST_VERIFIED_UBUNTU_RUNTIME_NOT_TESTED` when run on non-Linux hosts).
- `COMMUNITY_PORT_TESTED`: Remains `NOT_TESTED` / `BLOCKED` unless a user-supplied `.AppImage` is manually verified in a disposable non-production Ubuntu desktop session.
- `OFFICIAL_ZALO_UNSUPPORTED`: Always enforced because VNG publishes no official Linux desktop application.
