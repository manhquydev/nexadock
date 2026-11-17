# Authorized Operator Live Zalo Verification Checklist (Manual Only — Never Run in CI)

> [!IMPORTANT]
> Automated CI and repository test suites **must never** require real Zalo accounts, phone numbers, session cookies, or personal `%APPDATA%\ZaloData` / `~/Library/Application Support/ZaloData` directories. Execute this checklist **only** on an authorized test workstation with dedicated test accounts and explicit user consent.

---

## 1. Pre-Test Safety Gate (All Platforms)

- [ ] **L-PRE-01 (Dedicated Test Storage):** Set `DIM_DATA_DIR` to a dedicated evaluation directory outside your personal profile root (or verify `status` points to `MultiZaloDesktop` and **never** overlaps primary `ZaloData`).
- [ ] **L-PRE-02 (Primary Hash Snapshot):** Record pre-test SHA-256 digests of primary `ZaloData/config.json` and `ZaloData/migrate-config.json` (if a primary Zalo Desktop installation exists) so post-test invariance can be proven.
- [ ] **L-PRE-03 (Official Binary Verification):** Run `discover` and confirm official VNG signature status:
  - **Windows:** `authenticode_status == "Signed (VNG)"` (`CN=VNG GROUP JSC`).
  - **macOS:** Verify `codesign -dv --verbose=4 /Applications/Zalo.app` reports official VNG Apple Developer Team ID and valid notarization (`spctl -a -vv /Applications/Zalo.app`).
  - **Ubuntu LTS:** Official VNG Zalo Desktop is `UNSUPPORTED` (do **not** test patched/repacked proprietary `app.asar` archives; restrict Ubuntu testing to synthetic fixtures or user-supplied local AppImages in an isolated VM).

---

## 2. Windows 10 / 11 Authorized Live Checklist

- [ ] **L-WIN-01 (Primary Coexistence):** Leave primary Zalo Desktop signed in and open (`primary_zalo.is_running == true`), then create and launch 2 isolated profiles (`Test Account A`, `Test Account B`). Verify primary PID and chat window remain uninterrupted.
- [ ] **L-WIN-02 (QR Sign-In & Session Persistence):** Sign into `Test Account A` and `Test Account B` via QR scan on two authorized test phones. Close both profiles via `stop`, relaunch both, and confirm neither prompts for QR login again.
- [ ] **L-WIN-03 (Duplicate Account Guard):** Stop `Test Account B`, copy/login the same Zalo account as `Test Account A`, or attempt launching two profiles logged into the same numeric `zalo_user_id` (`Database/_production/<uid>`): confirm the second launch is blocked with `DUPLICATE_ZALO_USER_ID`.
- [ ] **L-WIN-04 (File Receive Isolation):** Send a test PDF/image to `Test Account A`; confirm download lands inside `<Profile_A>\Documents\Zalo Received Files\` and **not** in the host user's `%USERPROFILE%\Documents\Zalo Received Files\`.
- [ ] **L-WIN-05 (Known Limitations Confirmation):** Document observed behavior of Windows Toast notifications (`AppUserModelId("com.vng.zalo")`) and verify that concurrent voice/video calls are not relied upon (`\\.\pipe\PipeZCallSend` / `PipeZCallRecv`).
- [ ] **L-WIN-06 (Offline Backup & New-Profile Restore):** Stop `Test Account A`, run `backup`, then `restore` into `Test Account A (Restored)`, and verify `.zalo-profile-id` matches the new 8-hex ID before any launch.

---

## 3. macOS (`arm64` & `x86_64`) Authorized Live Checklist

> [!WARNING]
> Complete fixes for `DEFECT-01` (dangling `SingletonLock` detection), `DEFECT-02` (post-spawn readiness), and `DEFECT-06` (`SIGKILL` fallback + child tree cleanup) in `crates/platform-macos/src/adapter.rs` before promoting macOS from `NOT_TESTED` to `PARTIALLY_SUPPORTED`.

- [ ] **L-MAC-01 (`CFFIXED_USER_HOME` Containment):** Launch 2 isolated profiles via `/usr/bin/open -n -a /Applications/Zalo.app --env CFFIXED_USER_HOME=<profile_dir>`. Confirm via `lsof -p <pid>` that SQLite/LevelDB handles open under `<profile_dir>/Library/Application Support/ZaloData/` and **never** under `~/Library/Application Support/ZaloData/`.
- [ ] **L-MAC-02 (Keychain & `os_crypt` Prompts):** Observe macOS Keychain prompt count on first QR login and after restart when `Library/Keychains` is symlinked vs isolated.
- [ ] **L-MAC-03 (Unix Socket Call Conflict):** Inspect `/tmp/socketzalorecv2021` and `/tmp/socketzalosend2021` while launching a second profile during an idle vs active call on profile 1; verify whether `fs.unlink` disrupts `ZaloCall`.
- [ ] **L-MAC-04 (Graceful Quit via `NSRunningApplication`):** Trigger `stop` on profile 2 while profile 1 stays open; confirm SQLite WAL files checkpoint cleanly without corruption.

---

## 4. Post-Test Cleanup & Audit

- [ ] **L-POST-01:** Stop all evaluation profiles and verify `0` secondary Zalo processes remain.
- [ ] **L-POST-02:** Re-compute SHA-256 of primary `ZaloData/config.json` and confirm byte-for-byte identity with `L-PRE-02`.
- [ ] **L-POST-03:** Log out test sessions from phone devices ("Quản lý thiết bị đăng nhập") before deleting the evaluation `DIM_DATA_DIR` (`delete <id> --confirm`).
