# Live Official `Zalo.app` Hardware Validation Checklist (`LIVE_ZALO_NOT_TESTED` Exit Gate)

**Current Status:** **`NOT_TESTED`** (`LIVE_ZALO_NOT_TESTED` — `SupportStatus::NotTested`)  
**Required Hardware:** Physical Apple Silicon Mac (`arm64`, macOS 14 Sonoma / macOS 15 Sequoia+) and Intel Mac (`x86_64`, macOS 14/15) with an interactive GUI desktop session.

> **Strict Safety & Privacy Rules (Mandatory):**
> 1. **User-driven login ONLY:** A human operator scans QR codes interactively inside `Zalo.app`. Automated scripts must **never** read, copy, export, log, or inspect real Zalo messages, contacts, cookies, SQLite databases, or macOS Keychain items (`login.keychain-db` / `"Zalo Safe Storage"`).
> 2. **Zero binary/Gatekeeper tampering:** Never re-sign, patch, strip `com.apple.quarantine`, or weaken SIP/Gatekeeper on `/Applications/Zalo.app`.
> 3. **Primary Zalo protection:** Back up or verify primary `~/Library/Application Support/ZaloData` before testing and confirm zero writes/deletions occur in primary `ZaloData`.
> 4. **Auto-update precaution (`Squirrel.Mac`):** Do not trigger an in-app Zalo update while multiple isolated profiles are running concurrently; close all isolated profiles before updating `/Applications/Zalo.app`.

---

## Step-by-Step Hardware Verification Procedure

### Phase A — Cryptographic Identity & Publisher Pinning (`P0-1`)

- [ ] **Step 1 — Record Official VNG `TeamIdentifier`:** Run:
  ```bash
  /usr/bin/codesign -dv --verbose=4 /Applications/Zalo.app 2>&1
  /usr/sbin/spctl --assess --type execute --verbose=4 /Applications/Zalo.app 2>&1
  ```
  Record exact `Identifier=...`, `TeamIdentifier=...` (10-char Apple Team ID), `Authority=...` chain, and `source=Notarized Developer ID`.
- [ ] **Step 2 — Verify Apple Designated Requirement (`-R`):** Run:
  ```bash
  /usr/bin/codesign --verify --deep --strict -R='identifier "com.vng.zalo" and anchor apple generic and certificate 1[field.1.2.840.113635.100.6.2.6] and certificate leaf[field.1.2.840.113635.100.6.1.13] and certificate leaf[subject.OU] = "<TEAM_ID>"' /Applications/Zalo.app
  ```
  Confirm exit status `0`, and verify nested helpers (`ZaloHelper.app`, `Zalo Helper (Renderer).app`) pass `codesign --verify --deep --strict`.
- [ ] **Step 3 — Native Rust & Fixture Suite:** Run:
  ```bash
  cargo test --workspace --all-targets --locked
  node tests/platform-macos/run-macos-fixture-tests.mjs --out target/darwin-fixture-summary.json
  ```
  Confirm `verification_state == "DARWIN_NATIVE_TESTED"` and `15/15` scenarios `PASS`.
- [ ] **Step 4 — `dim_macos_cli discover` With Pinned Publisher:** Run:
  ```bash
  cargo run -p dim-platform-macos --example dim_macos_cli --locked -- \
    --pinned-bundle-id com.vng.zalo --pinned-team-id <TEAM_ID> discover
  ```
  Confirm `is_valid == true` and `signature_status` starts with `"VerifiedPublisher"`.

### Phase B — Primary Zalo Coexistence & 12-Surface Live Isolation (`P0-2`, `P0-3`)

- [ ] **Step 5 — Launch Primary Zalo First:** Open `/Applications/Zalo.app` normally from Finder/Spotlight so primary `~/Library/Application Support/ZaloData` is active. Run `dim_macos_cli status` and confirm `primary.running == true` and primary PIDs appear in `primary.protected_pids`.
- [ ] **Step 6 — Create & Launch Isolated Profile A & Profile B:** Create two profiles (`Work A`, `Personal B`) with long Vietnamese/CJK names so `<profile_dir>/tmp` exceeds `49` bytes and exercises `/tmp/dz-<uid>/<id>`. Launch both with `--pinned-bundle-id com.vng.zalo --pinned-team-id <TEAM_ID>`.
- [ ] **Step 7 — Verify Open File Descriptors (`lsof`) Across All 12 Surfaces:** Without reading any file contents, inspect open paths via `lsof -Fn -p <pid_A>` and `lsof -Fn -p <pid_B>`:
  - Confirm `ZaloData`, `Session Storage`, `Local Storage/leveldb`, `Network/Cookies`, `Caches/com.vng.zalo`, `HTTPStorages/com.vng.zalo`, `Crashpad`, and `Preferences/com.vng.zalo.plist` resolve strictly under `<profile_dir_A>` and `<profile_dir_B>` respectively.
  - Confirm **zero** writable file descriptors in `<pid_A>` or `<pid_B>` point to primary `~/Library/Application Support/ZaloData` or to each other's profile folder.
- [ ] **Step 8 — Check `cfprefsd` & Keychain Prompt Behavior:** Observe whether macOS displays any `"Zalo Safe Storage"` Keychain prompt when logging in via QR on Profile A and Profile B (`symlink_keychain = false` vs opt-in `--symlink-keychain`), and verify via modification timestamps (`stat -x`) whether `<profile_dir>/Library/Preferences/com.vng.zalo.plist` is written per-profile.
- [ ] **Step 9 — Verify `Downloads` & `Documents/Zalo Received Files` Isolation:** Save a benign test file in Profile A and confirm it lands in `<profile_dir_A>/Downloads` or `<profile_dir_A>/Documents/Zalo Received Files` — never inside Profile B or primary `~/Downloads`.

### Phase C — Window Activation, Shutdown, Restart & Known Limitations (`P1-1`..`P1-3`)

- [ ] **Step 10 — Zero-TCC Window Inspection & Cooperative Focus (`macOS 14/15`):** Run `dim_macos_cli focus <id_A>` and `dim_macos_cli focus <id_B>`. Confirm **zero** macOS Accessibility or Screen Recording permission popups appear, and record whether `activateWithOptions:3` brings the target window frontmost or returns a fail-closed `PROCESS_ERROR` when another app holds secure input.
- [ ] **Step 11 — Individual Graceful Stop (QR Screen vs Logged-In):** Test `dim_macos_cli stop <id_B>` both while Profile B sits on the QR login screen and while logged in:
  - Confirm Profile B + all `Zalo Helper` children exit cleanly (`0` orphan `Zalo Helper` PIDs in `ps -axo pid,ppid,lstart,args | grep <id_B>`).
  - Confirm `/tmp/dz-<uid>/<id_B>` is removed while Primary Zalo and Profile A stay running uninterrupted.
- [ ] **Step 12 — Manager Restart Re-Attachment:** While Profile A is running, restart `MultiZaloDesktop` / run a fresh `dim_macos_cli status` process and confirm Profile A remains `running == true` with matching `.runtime-launch.json` `lstart`.
- [ ] **Step 13 — Relaunch Session Persistence:** Stop Profile A, relaunch Profile A, and confirm Profile A restores its own session without affecting Primary Zalo.
- [ ] **Step 14 — Crash Recovery (`SIGKILL` Simulation on Isolated Profile):** Send `kill -9 <pid_A>` to isolated Profile A only, run `dim_macos_cli status` (`stale_lock_detected == true`), then `dim_macos_cli launch <id_A>` (`crash_recovered == true`).
- [ ] **Step 15 — Global `ZaloCall` Socket Non-Interference Check (`BLOCKED` Concurrent Calls):** Check `ls -la /tmp/socketzalo*2021` before and after stopping/recovering isolated profiles to verify `dim-platform-macos` never unlinks `/tmp/socketzalorecv2021` or `/tmp/socketzalosend2021`, and confirm single-instance call audio works when only one profile places a call.
- [ ] **Step 16 — Both CPU Architectures:** Record `uname -m`, `sw_vers`, and `Zalo.app` `CFBundleShortVersionString` on both Apple Silicon (`arm64`) and Intel (`x86_64`) before updating any subsystem row in `compatibility-matrix.md` from `NOT_TESTED` to `LIVE_ZALO_TESTED`.
