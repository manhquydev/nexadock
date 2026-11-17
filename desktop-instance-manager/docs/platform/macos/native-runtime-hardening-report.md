# macOS Native Runtime Validation & Safety Hardening Report (Mission A2 — Stage 2)

**Agent:** `macos-author`  
**Worktree:** `C:/Users/manhquy/Downloads/zalo-macos-worktree`  
**Branch:** `feat/macos-multi-instance`  
**Date:** `2026-11-18+07:00`

---

## 1. Branch & Commit Lineage (`HEAD`)

- **Repository Baseline (`main`):** `a5feb4a520798064c1508c26fe4c4ac17a3a03cf`
- **Stage 1 Commit (`Mission A`):** `fb2abcfc419c56b9579eee124a3a169697ec2740`
- **Stage 2 Commit (`Mission A2` `HEAD`):** Top commit on `feat/macos-multi-instance` directly parenting `fb2abcfc419c56b9579eee124a3a169697ec2740` (`git -C C:/Users/manhquy/Downloads/zalo-macos-worktree rev-parse HEAD`; exact SHA reported on the swarm message board at handoff).
- **`Cargo.lock` & `crates/platform-macos/Cargo.toml` Diff:** **Zero bytes changed (`100%` untouched).**

---

## 2. Changed & Created Files (Strictly Within Permitted Prefixes)

All modifications from baseline `a5feb4a520798064c1508c26fe4c4ac17a3a03cf` reside exclusively inside the 4 authorized macOS prefixes:

1. [`desktop-instance-manager/crates/platform-macos/src/ops.rs`](../../../crates/platform-macos/src/ops.rs)
2. [`desktop-instance-manager/crates/platform-macos/src/bundle.rs`](../../../crates/platform-macos/src/bundle.rs)
3. [`desktop-instance-manager/crates/platform-macos/src/codesign.rs`](../../../crates/platform-macos/src/codesign.rs)
4. [`desktop-instance-manager/crates/platform-macos/src/layout.rs`](../../../crates/platform-macos/src/layout.rs)
5. [`desktop-instance-manager/crates/platform-macos/src/process.rs`](../../../crates/platform-macos/src/process.rs)
6. [`desktop-instance-manager/crates/platform-macos/src/window.rs`](../../../crates/platform-macos/src/window.rs)
7. [`desktop-instance-manager/crates/platform-macos/src/adapter.rs`](../../../crates/platform-macos/src/adapter.rs)
8. [`desktop-instance-manager/crates/platform-macos/src/lib.rs`](../../../crates/platform-macos/src/lib.rs)
9. [`desktop-instance-manager/crates/platform-macos/examples/dim_macos_cli.rs`](../../../crates/platform-macos/examples/dim_macos_cli.rs)
10. [`desktop-instance-manager/crates/platform-macos/tests/macos_scenarios.rs`](../../../crates/platform-macos/tests/macos_scenarios.rs)
11. [`desktop-instance-manager/tests/platform-macos/fixture-macos-app.mjs`](../../../tests/platform-macos/fixture-macos-app.mjs)
12. [`desktop-instance-manager/tests/platform-macos/run-macos-fixture-tests.mjs`](../../../tests/platform-macos/run-macos-fixture-tests.mjs)
13. [`desktop-instance-manager/docs/platform/macos/implementation-report.md`](./implementation-report.md)
14. [`desktop-instance-manager/docs/platform/macos/compatibility-matrix.md`](./compatibility-matrix.md)
15. [`desktop-instance-manager/docs/platform/macos/integration-handoff.md`](./integration-handoff.md)
16. [`desktop-instance-manager/docs/platform/macos/native-runtime-hardening-report.md`](./native-runtime-hardening-report.md) *(new)*
17. [`desktop-instance-manager/docs/platform/macos/live-zalo-test-checklist.md`](./live-zalo-test-checklist.md) *(new)*
18. [`desktop-instance-manager/plans/reports/macos/test-and-verification-report.md`](../../../plans/reports/macos/test-and-verification-report.md)

---

## 3. Verified Technical Findings (`STATIC_VERIFIED` & `WINDOWS_HOST_FIXTURE_TESTED`)

1. **5-Tier Publisher Trust Separation (`STATIC_VERIFIED` + `WINDOWS_HOST_FIXTURE_TESTED`):** Apple's `codesign --verify --deep --strict` proves bundle integrity (`VALID_CODESIGN`), and `spctl --assess --type execute` proves notarization (`GATEKEEPER_ASSESSED`), but **neither** proves the binary was signed by VNG rather than another Apple Developer ID holder unless `codesign -R='identifier "<bundle_id>" ... and certificate leaf[subject.OU] = "<TEAM_ID>"'` is enforced against an explicitly pinned `TeamIdentifier`.
2. **Chromium `MAC_CHROMIUM_TMPDIR` 50-Char Threshold & Darwin `SUN_LEN` `104B` (`STATIC_VERIFIED` + `WINDOWS_HOST_FIXTURE_TESTED`):** Darwin `sockaddr_un.sun_path` is `104` bytes (`103` usable), and Chromium `base/files/file_util_mac.mm` checks `TMPDIR` length `< 50` (`49` bytes) because `/.org.chromium.Chromium.XXXXXX/SingletonSocket` appends `46` bytes (`49 + 46 = 95 <= 103`). Two-level `/tmp/dz-<uid>/<id>` is `20` bytes (`20 + 46 = 66 <= 103`) and eliminates shared world-writable `/tmp` symlink-squatting races when created non-recursively with `0o700` + `lstat` UID check.
3. **Same-UID, Same-Binary `Zalo.app` PID Reuse Hazard (`STATIC_VERIFIED` + `WINDOWS_HOST_FIXTURE_TESTED`):** Because LaunchServices (`open -n`) spawns `/Applications/Zalo.app/Contents/MacOS/Zalo` under `launchd` (`PPID = 1`) for every profile, checking only `uid == geteuid()` and executable name `"Zalo"` cannot distinguish a newly spawned primary or sibling `Zalo.app` that recycled a crashed profile's PID. Binding `.runtime-launch.json` to kernel creation time (`ps -o lstart=` / Win32 `GetProcessTimes`) closes this gap deterministically.
4. **macOS 14+ Cooperative Window Activation (`STATIC_VERIFIED` + `WINDOWS_HOST_FIXTURE_TESTED`):** On macOS 14 Sonoma / 15 Sequoia+, `NSRunningApplication.activate` is cooperative and returns `false` when focus stealing is declined, while `System Events` triggers invasive TCC Accessibility (`-1719` / `-25211`) or Automation (`-1743`) prompts. Removing `System Events` and inspecting `activateWithOptions:3` + `isActive()` eliminates TCC popups while failing closed when activation is denied.
5. **Externalized Zalo DB Location (`F-CORE-01`, `STATIC_VERIFIED` + `WINDOWS_HOST_FIXTURE_TESTED`):** Official Zalo `app.asar` supports custom database paths via `<ZaloData>/config.json -> db_path.location`, which must be validated against primary `ZaloData` and sibling `/Profiles/<other>` directories.

---

## 4. Refuted Findings & Corrected Stage-1 Assumptions

1. **Refuted — Substring `"VNG"` Publisher Matching (`codesign.rs` Stage 1):** Matching `Authority=...` for substring `"VNG"` could match a spoofed CN or organization unit and conflated Gatekeeper assessment with verified publisher identity. Replaced with `PinnedPublisherPolicy` (`None` by default) + `codesign -R=...` Apple Designated Requirement verification.
2. **Refuted — Default Shared `Downloads` & `Documents` Symlinks (`layout.rs` Stage 1):** Symlinking `Downloads` and `Documents` (`Documents/Zalo Received Files`) to real `$HOME` by default leaked downloaded attachments across profiles AND caused cross-platform contract test `CT-03` (`unlink_profile_junctions == Ok(0)`) to fail on Unix. Changed `MacOsLayoutOptions::default()` to `{ symlink_user_dirs: false, symlink_keychain: false }`.
3. **Refuted — Unconditional `cfprefsd` Runtime Isolation Claim:** Downgraded `cfprefsd` daemon runtime isolation in `compatibility-matrix.md` to **`NOT_TESTED`** until live Darwin `NSUserDefaults` verification runs.
4. **Refuted — Binary-Name Fallback in `verify_candidate_pid_ownership` (`process.rs` Stage 1):** Removed the fallback that accepted any `Zalo` process when `--user-data-dir` was absent without `.runtime-launch.json` `lstart` corroboration.
5. **Refuted — Silent Success on Timed-Out `stop()` (`process.rs` Stage 1):** Previously, `stop_isolated_profile_tree` unlinked `.runtime-pid` and returned `Ok(...)` even if a hung process ignored `.quit-signal` and `SIGTERM`. Fixed to keep `.runtime-launch.json` intact and return `Err(DimError::ProcessError)` whenever any owned PID survives past `timeout_ms`.

---

## 5. Concrete Bugs & Safety Defects Fixed in Stage 2

| Defect ID | File(s) | Root Cause | Fix Implemented |
| :--- | :--- | :--- | :--- |
| **`P0-1` / `DEFECT-01`** | `bundle.rs`, `codesign.rs`, `adapter.rs` | Loose `"VNG"` substring check, unverified nested helpers, silent XML/bplist fallback, no bundle symlink-escape check | Added 5-tier `AppIdentityVerificationTier`, `ensure_bundle_no_symlink_escape`, `validate_plist_xml_structure`, fail-closed `plutil` check, per-helper `codesign --verify`, and `PinnedPublisherPolicy` (`-R=...`). |
| **`P0-2`** | `layout.rs`, `adapter.rs`, `fixture-macos-app.mjs` | Default `Downloads`/`Documents` symlinks shared files across profiles; only 3 surfaces checked | Set `MacOsLayoutOptions::default()` to isolated `0o700` across all `12` surfaces (`MACOS_12_ISOLATED_SURFACES`), added `verify_external_db_path_isolated` (`F-CORE-01`) and `verify_12_surface_runtime_isolation`. |
| **`P0-3`** | `ops.rs`, `process.rs`, `adapter.rs` | No `lstart` start-time check against same-binary PID reuse; `ppid == 1` orphans missed | Added `RuntimeLaunchRecord` (`.runtime-launch.json`), `ps -o lstart=` parsing, Win32 `FILETIME` start tokens, fail-closed `verify_candidate_pid_ownership`, and `collect_profile_owned_pids`. |
| **`P0-4` / `DEFECT-02`** | `layout.rs`, `adapter.rs`, `fixture-macos-app.mjs` | Empty `extra_args` caused `fixture-macos-app.mjs` to fall back to constant `userId = "100001"`, breaking `XP-MAC-03` | Added `derive_fixture_user_id(&profile.id)` (`--user-id=84xxxxxxx`) in `adapter.rs` and removed constant `'100001'` fallback in `fixture-macos-app.mjs`. |
| **`P0-4` / `DEFECT-03`** | `adapter.rs`, `process.rs` | Slow-starting child (`--slow-startup-pre-lock-ms=3100`, `LC-07`) reported `running = false` before `SingletonLock` appeared | `launch()` writes `.runtime-launch.json` + `.runtime-pid` immediately after spawn, and `inspect_profile()` reads `read_runtime_pid(profile_dir)` so ownership holds immediately. |
| **`P0-4` / `DEFECT-04`** | `layout.rs` | `.macos-profile-meta.json.tmp` collided under parallel `ensure_profile_layout` (`LC-08`) | Unique `.macos-profile-meta.{pid}.{nanos}.{seq}.tmp` atomic temp filenames. |
| **`P0-4` / `DEFECT-05`** | `adapter.rs`, `process.rs` | `.exists()` returns `false` on dangling `SingletonLock` symlinks (`LC-06`) | Replaced remaining `.exists()` lock checks with `symlink_or_file_present` (`fs::symlink_metadata`). |
| **`P0-4` / `DEFECT-06`** | `layout.rs`, `adapter.rs` | Default symlinks broke `CT-03` (`LC-09`) on Unix | Default `symlink_user_dirs: false, symlink_keychain: false` yields `0` symlinks unless explicitly opted in. |
| **`P1-1`** | `layout.rs`, `process.rs` | Flat `/tmp/dim-zalo-<uid>-<id>` (`54B` threshold) left leftover dirs on stop | Hardened to `/tmp/dz-<uid>/<id>` (`<= 49B`, `MAC_CHROMIUM_TMPDIR`, atomic non-recursive `0o700`, `lstat` anti-squat) + `cleanup_profile_short_tmpdir`. |
| **`P1-2`** | `process.rs`, `ops.rs` | `SIGTERM` could fire prematurely; hung processes reported stopped | Enforced dedicated `2/3` graceful window before `SIGTERM`, `ppid == 1` orphan reaping, and fail-closed `Err(DimError::ProcessError)` when hung. |
| **`P1-3`** | `window.rs`, `adapter.rs` | `System Events` triggered TCC prompts; macOS 14+ `activate() -> false` ignored; focus didn't re-verify PID ownership | Removed `System Events`, verified `activated=true` & `active=true` fail-closed, and re-verified live ownership in `focus_or_restore` before touching `.focus-signal` or `osascript`. |

---

## 6. Regression Test IDs (`39/39` Rust + `15/15` E2E CLI Scenarios)

- **Rust Unit Tests (`crates/platform-macos/src/lib.rs`):**
  - `tests::singleton_lock_parser_handles_hyphenated_hostnames`
  - `tests::open_command_builder_sets_cffixed_user_home`
  - `tests::macos_layout_defaults_isolated_and_not_tested_status`
- **Rust Integration Suites (`crates/platform-macos/tests/macos_scenarios.rs`):**
  - `macho_inspector_parses_arm64_x86_64_universal2_and_fat64`
  - `p0_1_all_seven_bundle_and_codesign_identity_failure_modes_plus_tier5_verified_publisher`
  - `p0_2_twelve_surface_data_isolation_ct03_and_external_db_path_guard`
  - `p0_3_all_eight_process_ownership_pid_reuse_and_primary_protection_cases`
  - `p0_4_defects_01_to_06_xp_mac_03_static_and_runtime_contracts`
  - `p1_1_sun_len_short_tmpdir_symlink_squatting_and_foreign_slot_hardening`
  - `p1_2_all_seven_graceful_shutdown_timeout_hung_and_orphan_helper_scenarios`
  - `p1_3_all_four_macos_14_window_activation_and_zero_tcc_cases`
  - `scenario_7_concurrent_duplicate_launch_guard_blocks_parallel_race`
  - `scenario_10_11_12_13_symlink_unlink_unicode_nfc_nfd_path_max_and_permissions`
- **Live E2E CLI Scenarios (`tests/platform-macos/run-macos-fixture-tests.mjs`):**
  - Scenarios `1..13` (`single_session`, `two_sessions`, `three_sessions`, `isolated_data_12_surfaces`, `relaunch`, `individual_close`, `duplicate_operation_guard`, `crash_recovery`, `ownership_and_primary_protection`, `safe_symlink_deletion`, `unicode_nfc_nfd`, `long_paths_sun_len_path_max`, `permission_failures`) + Scenario `14` (`slow_prelock_and_auto_userid`) + Scenario `15` (`graceful_delayed_hung_orphan_shutdown`).

---

## 7. Native macOS Test Evidence & Exact Blocker Analysis

- **Local Execution Evidence (`WINDOWS_HOST_FIXTURE_TESTED` & `STATIC_VERIFIED`):** Executed on Windows x64 (`rustc 1.100.0`, `node v26.13.1`); all `39/39` workspace Rust tests and `15/15` E2E CLI scenarios passed with `0` warnings across host + `aarch64-apple-darwin` + `x86_64-apple-darwin`.
- **Native Darwin Execution (`DARWIN_NATIVE_TESTED`):** **`NOT_TESTED` (`BLOCKED` in this local environment)** due to three verified facts:
  1. No physical Apple Silicon or Intel Mac is attached to this Windows workstation.
  2. Local `gh` (`2.104.0`) OAuth token has scopes `'gist, read:org, repo'` and lacks `'workflow'`, and `origin/main` currently has `0` registered GitHub Actions workflows.
  3. Triggering private-repository GitHub-hosted macOS runners incurs a 10x Actions billing multiplier (`$0.062/min`) and pushing branches to `origin` is unauthorized under our swarm scope rules.

---

## 8. Apple Silicon (`arm64`) & Intel (`x86_64`) Architecture Status

| Architecture | Rust Target | Compile & `clippy -D warnings` | Mach-O Slice & Rosetta Logic | Native Darwin Runtime |
| :--- | :--- | :---: | :---: | :---: |
| **Apple Silicon (`arm64`)** | `aarch64-apple-darwin` | **`STATIC_VERIFIED`** (`0` warnings) | **`WINDOWS_HOST_FIXTURE_TESTED`** (Universal2 + thin `arm64` + `x86_64` Rosetta 2 diagnostic) | **`NOT_TESTED`** (target runner: `macos-15`) |
| **Intel Mac (`x86_64`)** | `x86_64-apple-darwin` | **`STATIC_VERIFIED`** (`0` warnings) | **`WINDOWS_HOST_FIXTURE_TESTED`** (Universal2 + thin `x86_64`; rejects `arm64`-only fail-closed) | **`NOT_TESTED`** (target runner: `macos-15-intel`; `macos-13` retired) |

---

## 9. 12-Surface Profile Data Isolation Status (`P0-2`)

- **Fixture Runtime Status:** **`WINDOWS_HOST_FIXTURE_TESTED`** (`verify_12_surface_runtime_isolation` verifies non-vacuous runtime files across all 11 non-Keychain surfaces + isolated `0o700` `Library/Keychains`, `3` distinct `TMPDIR`s, `3` distinct `Caches`, non-symlinked `Downloads` & `Documents/Zalo Received Files`, `F-CORE-01` `db_path.location` guard, and untouched primary `ZaloData`).
- **Native Darwin / `cfprefsd` / Live `Zalo.app` Status:** **`NOT_TESTED`**.

---

## 10. Primary Zalo & Unmanaged Process Protection Status (`P0-3`)

- **Status:** **`STATIC_VERIFIED` + `WINDOWS_HOST_FIXTURE_TESTED`** across all 8 required ownership cases (`p0_3_all_eight_process_ownership_pid_reuse_and_primary_protection_cases` + E2E Scenario 9).
- Primary `~/Library/Application Support/ZaloData` (NFC and NFD!) is blocked by `DimError::ProtectedPrimaryPath`, and all primary + unmanaged `Zalo.app` PIDs are protected by `DimError::ProtectedPrimaryProcess`. Same-UID, same-binary `Zalo.app` PID reuse with mismatched `lstart` is rejected fail-closed.

---

## 11. 5-Tier `codesign`, Gatekeeper & Publisher Pinning Status (`P0-1`)

- **Status:** **`STATIC_VERIFIED` + `WINDOWS_HOST_FIXTURE_TESTED`** across all 7 negative failure modes + positive Tier-5 `VerifiedPublisherIdentity` (`p0_1_all_seven_bundle_and_codesign_identity_failure_modes_plus_tier5_verified_publisher`).
- `MacOsAdapterConfig::default().pinned_publisher_policy` is `None` so an unpinned build **never** falsely reports `VerifiedPublisher`. Live VNG `TeamIdentifier` capture is gated on Step 1 of [`live-zalo-test-checklist.md`](./live-zalo-test-checklist.md) (`NOT_TESTED`).

---

## 12. IPC, `SUN_LEN` (`104B`) `SingletonSocket` & Global `ZaloCall` Limitations (`P1-1`)

- **Chromium `SingletonSocket`:** **`STATIC_VERIFIED` + `WINDOWS_HOST_FIXTURE_TESTED`** (`/tmp/dz-<uid>/<id>` keeps socket path at `66` bytes `<= 103`, anti-squat verified, cleaned on `stop()`/`recover()`).
- **`ZaloCall` Voice/Video Helper (`/tmp/socketzalorecv2021`, `/tmp/socketzalosend2021`):** **`BLOCKED`** for concurrent calls across multiple instances (`SupportStatus::Blocked`). `dim-platform-macos` inspects these sockets read-only for diagnostics and **never** unlinks them.
- **`UNUserNotificationCenter` & `zalo://` Deep Links:** **`BLOCKED`** from per-profile differentiation due to shared `CFBundleIdentifier` (`com.vng.zalo`).

---

## 13. L3 (Darwin Native Fixture) & L4 (Live Official `Zalo.app`) Status

- **L1 (Static / Cross-Compile / Lint) & L2 (Host Mock + Detached Node Fixture E2E):** **COMPLETE** (`STATIC_VERIFIED` + `WINDOWS_HOST_FIXTURE_TESTED`).
- **L3 (Native Darwin Kernel Fixture Suite on `macos-15` & `macos-15-intel`):** **`NOT_TESTED`** (ready to run verbatim via `node tests/platform-macos/run-macos-fixture-tests.mjs` once `IR-MACOS-CI-01` is scheduled).
- **L4 (Live Official VNG `/Applications/Zalo.app` Multi-Account QR Validation):** **`NOT_TESTED`** (`LIVE_ZALO_NOT_TESTED`, `SupportStatus::NotTested`; governed by [`live-zalo-test-checklist.md`](./live-zalo-test-checklist.md)).

---

## 14. Cross-Worktree Integration Requests

Documented with full 6-field contracts in [`docs/platform/macos/integration-handoff.md`](./integration-handoff.md):
1. **`IR-MACOS-CI-01`** (`.github/workflows/**`): Schedule native `macos-15` (`arm64`) and `macos-15-intel` (`x86_64`) validation jobs (`macos-13` retired).
2. **`IR-TAURI-01`** (`apps/desktop/src-tauri/**`): Wire verified VNG `TeamIdentifier` into `.with_pinned_publisher("com.vng.zalo", TEAM_ID)` after physical Mac capture, and sign `MultiZaloDesktop.app` with Hardened Runtime (non-sandboxed).
3. **`IR-CORE-01`** (`crates/profile-store/**`, `crates/core/**`): Clean up `clippy::needless_bool` in `store.rs:337` on non-Windows targets and optionally promote `config.json -> db_path.location` check to `dim-core`.
4. **`IR-RECOVERY-01`** (`crates/process-manager/**`): Include `"SingletonSocket"` and `"SingletonCookie"` in shared `STALE_DATA_DIR_ARTIFACTS`.

---

## 15. Remaining External Blockers & Exit Criteria

1. **GitHub Actions Darwin Runner Execution (`BLOCKED` on authorization + `'workflow'` token scope):** Once `cross-platform-validation.yml` (`macos-15` + `macos-15-intel`) is merged and triggered by an authorized maintainer, capture `darwin-fixture-summary.json` (`15/15 PASS`) to advance L3 from `NOT_TESTED` to `DARWIN_NATIVE_TESTED`.
2. **Physical Mac Live `Zalo.app` Execution (`BLOCKED` on physical Mac + human QR login):** Execute all 16 steps in [`live-zalo-test-checklist.md`](./live-zalo-test-checklist.md) on `arm64` and `x86_64` Macs, pin the verified 10-character VNG `TeamIdentifier` (`IR-TAURI-01`), and only then advance `official_zalo_status` from `SupportStatus::NotTested` (`LIVE_ZALO_NOT_TESTED`).
