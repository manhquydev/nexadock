# Shared Cargo Workspace & GitHub Actions CI Integration Guide

Because Mission C enforces strict file ownership boundaries (`tests/cross-platform/**`, `fixtures/cross-platform/**`, `docs/testing/cross-platform/**`, `plans/reports/reliability/**`), both `fixtures/cross-platform/Cargo.toml` (`dim-xf-fixture`) and `tests/cross-platform/Cargo.toml` (`dim-cross-platform-tests`) declare standalone `[workspace]` tables so they compile and execute today with **zero edits** to shared repository manifests.

When merging `test/cross-platform-reliability` into `main`, repository maintainers may either keep the standalone workspaces (recommended for strict test isolation) or integrate them into the shared Cargo workspace and `.github/workflows/ci.yml` using the exact patches below.

---

## Option A (Recommended): Keep Standalone Test Workspaces & Add CI Matrix Job

Add the following job to [`.github/workflows/ci.yml`](../../../../.github/workflows/ci.yml) so GitHub-hosted runners execute the OS-native suites independently on `windows-latest`, `macos-latest`, and `ubuntu-latest` without requiring any real Zalo Desktop account:

```yaml
  cross-platform-reliability:
    name: Cross-Platform Reliability & Contract Suite (${{ matrix.os }})
    runs-on: ${{ matrix.os }}
    strategy:
      fail-fast: false
      matrix:
        os: [windows-latest, macos-latest, ubuntu-latest]
    steps:
      - name: Checkout repository
        uses: actions/checkout@v4

      - name: Setup Node.js
        uses: actions/setup-node@v4
        with:
          node-version: "22"

      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy

      - name: Install optional Bubblewrap on Ubuntu
        if: runner.os == 'Linux'
        run: |
          sudo apt-get update
          sudo apt-get install -y bubblewrap

      - name: Lint cross-platform fixture & test harness
        run: |
          cargo fmt --manifest-path fixtures/cross-platform/Cargo.toml -- --check
          cargo fmt --manifest-path tests/cross-platform/Cargo.toml -- --check
          cargo clippy --manifest-path fixtures/cross-platform/Cargo.toml --all-targets -- -D warnings
          cargo clippy --manifest-path tests/cross-platform/Cargo.toml --all-targets -- -D warnings

      - name: Execute OS-specific cross-platform reliability & contract runner
        run: node tests/cross-platform/scripts/run-all.mjs

      - name: Upload per-OS reliability & telemetry artifacts
        if: always()
        uses: actions/upload-artifact@v4
        with:
          name: reliability-evidence-${{ runner.os }}
          path: desktop-instance-manager/plans/reports/reliability/*.json
```

> [!NOTE]
> Once product maintainers resolve `DEFECT-01` .. `DEFECT-10`, append `-- --fail-on-product-defects` (or `-FailOnProductDefects` on Windows) to `run-all.mjs` so any regression in adapter parity fails the CI job with exit code `2`.

---

## Option B: Merge Crates Into Shared `desktop-instance-manager/Cargo.toml`

If maintainers prefer a single unified `Cargo.lock` across product crates and cross-platform test crates:

1. **Remove** the trailing `[workspace]` line from:
   - [`fixtures/cross-platform/Cargo.toml`](../../../fixtures/cross-platform/Cargo.toml)
   - [`tests/cross-platform/Cargo.toml`](../../../tests/cross-platform/Cargo.toml)
2. **Update** `desktop-instance-manager/Cargo.toml` `members` list:
   ```toml
   [workspace]
   resolver = "2"
   members = [
       "crates/core",
       "crates/profile-store",
       "crates/process-manager",
       "crates/platform-windows",
       "crates/platform-macos",
       "crates/platform-linux",
       "fixtures/test-desktop-app",
       "fixtures/cross-platform",
       "tests/cross-platform",
   ]
   ```
3. **Update** `dim-cross-platform-tests::harness::xf_fixture_bin_path()` fallback (optional) or keep `CARGO_BIN_EXE_xf-desktop-fixture` resolution: when unified under one workspace, `cargo build --workspace --release` places `xf-desktop-fixture(.exe)` in `desktop-instance-manager/target/release/`.
4. Regenerate the unified lockfile (`cargo generate-lockfile`) and re-run `node scripts/generate-build-provenance.mjs`.

---

## Remote GitHub Runner Preflight Note

On the current audit workstation (`2026-11-17`), `gh auth status` carries scopes `'gist', 'read:org', 'repo'` (without `'workflow'`), and `gh workflow list --repo manhquydev/zalo` reports `0` registered workflows on `origin`. Pushing `.github/workflows/ci.yml` from an account/token with `workflow` permission (`gh auth refresh -s workflow`) will immediately enable the matrix runners on `windows-latest`, `macos-latest`, and `ubuntu-latest`.
