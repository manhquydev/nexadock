# Windows L3 Real-Account Manual QR Acceptance Checklist (`Section 5`)

- **Worktree / Branch:** `<WORKTREE_ROOT>/zalo-windows-acceptance` (`qa/windows-native-acceptance`)
- **Evaluated Integration Commit:** `cd62c523222cb3a02051347e85313226de8b6964` *(clean fast-forward over `bff0674` + `004ab7c`)*
- **Guided Recorder Script:** `desktop-instance-manager/tests/windows-acceptance/run-l3-guided-acceptance.ps1`
- **Current Section 5 Gate Status:** **`NOT_TESTED`** (`8 / 8` checks unexecuted by human operator)
- **Workflow State:** **`AWAITING_MANUAL_L3`** *(Restricts candidate classification to `INTERNAL_TEST_BUILD` under Gate A5 / `BLK-05`)*

---

## 1. Purpose, Scope & Strict Safety Boundaries

Automated unit, fixture, and cross-platform reliability suites (`tests/cross-platform`) validate L1/L2 directory isolation, lockfile guards, and UI contracts using synthetic fixtures only. Per project epistemic rules, **synthetic tests never prove real multi-account VNG Zalo session behavior**. This Section 5 checklist guides a **non-technical human tester** through end-to-end acceptance using **two real, authorized test Zalo accounts**.

> [!IMPORTANT]
> **MANDATORY PRIVACY, SECURITY & ANTI-AUTOMATION RULES**
>
> 1. **Manual QR Scan Only:** Sign in strictly by scanning QR codes manually on two authorized phones inside official Zalo Desktop windows. **Never** automate OTPs, **never** scrape QR tokens, and **never** copy or inspect `Cookies`, `Local Storage`, or Windows DPAPI blobs.
> 2. **100% Primary Zalo Protection:** Leave your everyday Zalo Desktop (`%APPDATA%\ZaloData`) untouched. Multi-Zalo displays it as **"Zalo đang dùng trên máy"** (*"Zalo already on this computer"*) in read-only protected mode.
> 3. **Zero Personal Data in Evidence (`STRICT`):** **Never** record real message text, phone numbers, email addresses, contact names, profile avatars, QR codes, or screenshots of chat conversations in QA notes or evidence JSONs. Only use synthetic test strings (`L3-PING-TEST`) and dummy files (`dummy-qa-test.txt`).
> 4. **Honest Unexecuted State:** Running `run-l3-guided-acceptance.ps1 -NonInteractive` in automated suites immediately emits `overall_gate_status: "NOT_TESTED"` and `workflow_status: "AWAITING_MANUAL_L3"` without faking `PASS`.

---

## 2. Quick Setup Before Starting (2 Minutes)

### What You Need
- A Windows 10 or Windows 11 (`64-bit`) PC with **official Zalo Desktop** installed from VNG (verified on QA host: `Zalo 26.10.20`, signed `CN=VNG GROUP JSC`).
- **Two phones** logged into **two separate, authorized test Zalo accounts** (`Test Account A` and `Test Account B`) that can message each other.
- A dummy test file on your Desktop named `dummy-qa-test.txt` containing only the text `QA_DUMMY_PAYLOAD` (zero personal or company documents).

### Launching the Manager & Guided Recorder
1. **Open Multi-Zalo Desktop** (either via the installed **Multi-Zalo Desktop** app once `BLK-04` RC installer lands, or via `node apps/desktop/ui-server.mjs` / `dim-desktop.exe` during internal testing).
2. *(Optional Guided Evidence Recorder)* Open PowerShell in `<WORKTREE_ROOT>/zalo-windows-acceptance` and run:
   ```powershell
   powershell -NoProfile -ExecutionPolicy Bypass `
     -File desktop-instance-manager/tests/windows-acceptance/run-l3-guided-acceptance.ps1 `
     -JsonOut "$env:TEMP\l3-human-acceptance-result.json"
   ```
   *(For CI / automated suite preflight without human prompts, pass `-NonInteractive`, which returns `AWAITING_MANUAL_L3` in `< 1s`.)*

---

## 3. Step-by-Step Non-Technical Tester Checklist (All 8 Required Checks)

Mark each check strictly as **`PASS`**, **`FAIL`**, **`BLOCKED`**, or **`NOT_TESTED`**.

---

### Check 1 (`L3-ACC-01`) — Independent 2-Account Login via Manual QR Scan
- **Status Today:** `[ ] NOT_TESTED` (`AWAITING_MANUAL_L3`)
- **How to Test (Step by Step):**
  1. On the Multi-Zalo main screen (**Tài khoản Zalo** / **Zalo Accounts**), click **Thêm tài khoản** (*Add account*).
  2. Enter name `Test Account A`, choose **Xanh dương** (*Blue*), click **Tiếp tục** (*Continue*), then click **Mở Zalo để đăng nhập** (*Open Zalo to sign in*).
  3. Repeat Steps 1–2 to create `Test Account B` with color **Xanh ngọc** (*Teal*) and click **Mở Zalo để đăng nhập** (*Open Zalo to sign in*).
  4. In the first Zalo window, use **Phone A** (Zalo app -> QR scanner) to scan the QR code and confirm sign-in on Phone A.
  5. In the second Zalo window, use **Phone B** to scan its QR code and confirm sign-in on Phone B.
- **Expected Result (`PASS` Criteria):**
  - Both Zalo windows sign in smoothly without asking Multi-Zalo for any password or OTP.
  - Multi-Zalo never shows technical prompts (`PID`, `IPC`, `lockfile`).
  - If your main personal Zalo was already open on the computer, it stays signed in and uninterrupted.

---

### Check 2 (`L3-ACC-02`) — Both Accounts Open Simultaneously
- **Status Today:** `[ ] NOT_TESTED` (`AWAITING_MANUAL_L3`)
- **How to Test (Step by Step):**
  1. Place both signed-in Zalo windows side by side on your screen and look at Multi-Zalo's **Tài khoản Zalo** (*Zalo Accounts*) list.
  2. Confirm both `Test Account A` and `Test Account B` show the green status **"Cửa sổ đang mở"** (*"Window open"*), and the header shows **"Đang mở 2 cửa sổ"** (*"2 open"*).
  3. *(Bonus Safety Check)* If you attempt to sign in to the *exact same* Zalo account in two spaces and reopen them, confirm Multi-Zalo blocks the duplicate window with the plain-language message: *"Tài khoản Zalo này đang được mở ở một cửa sổ khác."* (*"This Zalo account is already open in another window."*, `DUPLICATE_ZALO_USER_ID`).
- **Expected Result (`PASS` Criteria):**
  - Both test accounts remain responsive side by side at the same time, with separate chat workspaces and color badges preserved on their cards.

---

### Check 3 (`L3-ACC-03`) — Zero Cross-Logout Across Restart (Session & DPAPI Isolation)
- **Status Today:** `[ ] NOT_TESTED` (`AWAITING_MANUAL_L3`)
- **How to Test (Step by Step):**
  1. Close the **Multi-Zalo** manager window completely, wait 5 seconds, and reopen **Multi-Zalo**.
  2. Next, click **Đóng Zalo** (*Close Zalo*) on both `Test Account A` and `Test Account B`, wait until both cards show **"Đã đóng"** (*"Closed"*), and click **Mở Zalo** (*Open Zalo*) on both cards again.
- **Expected Result (`PASS` Criteria):**
  - Neither `Test Account A`, `Test Account B`, nor your computer's main Zalo logs out another account or invalidates another account's encrypted Windows DPAPI / `SecureCipher` session.
  - Both accounts reopen directly into their own accounts without swapping identities.

---

### Check 4 (`L3-ACC-04`) — Accurate Window Switching to the Right Account
- **Status Today:** `[ ] NOT_TESTED` (`AWAITING_MANUAL_L3`)
- **How to Test (Step by Step):**
  1. Overlap both Zalo windows (or minimize `Test Account A` and close `Test Account B`'s window to the Windows System Tray via its `X` button).
  2. In Multi-Zalo, click **Chuyển đến** (*Switch to window*) on `Test Account A`'s card -> verify `Test Account A`'s main chat window comes to the front.
  3. Click **Chuyển đến** (*Switch to window*) on `Test Account B`'s card -> verify `Test Account B`'s main chat window restores from the System Tray and comes to the front (`second-instance` foreground handoff).
- **Expected Result (`PASS` Criteria):**
  - Clicking **Chuyển đến** (*Switch to window*) always brings forward the **exact matching account's main chat window** (never focusing the wrong account, never focusing Primary Zalo, and never getting stuck on a small notification popup).

---

### Check 5 (`L3-ACC-05`) — Independent Single-Account Close
- **Status Today:** `[ ] NOT_TESTED` (`AWAITING_MANUAL_L3`)
- **How to Test (Step by Step):**
  1. With both `Test Account A` and `Test Account B` open (plus Primary Zalo if running), click **Đóng Zalo** (*Close Zalo*) **only** on `Test Account B`.
  2. Observe `Test Account A` and Primary Zalo for 10 seconds.
- **Expected Result (`PASS` Criteria):**
  - `Test Account B` transitions cleanly to **"Đã đóng"** (*"Closed"*) and its Zalo window/tray icon exits completely.
  - `Test Account A` remains **"Cửa sổ đang mở"** (*"Window open"*) and usable without interruption.
  - Primary Zalo (`%APPDATA%\ZaloData`) remains 100% untouched.

---

### Check 6 (`L3-ACC-06`) — Clean Reopen With Session Preserved
- **Status Today:** `[ ] NOT_TESTED` (`AWAITING_MANUAL_L3`)
- **How to Test (Step by Step):**
  1. After `Test Account B` has been closed in Check 5, wait 5 seconds and click **Mở Zalo** (*Open Zalo*) on `Test Account B`.
- **Expected Result (`PASS` Criteria):**
  - `Test Account B` launches cleanly and signs straight back into `Test Account B` **without asking to scan a QR code again** and without displaying stale-lockfile errors.

---

### Check 7 (`L3-ACC-07`) — Synthetic Test Message & Dummy File Transfer (Strict Zero-PII Rule)
- **Status Today:** `[ ] NOT_TESTED` (`AWAITING_MANUAL_L3`)
- **How to Test (Step by Step):**
  1. From `Test Account A`, send the exact synthetic text `L3-PING-TEST` to `Test Account B` and confirm `Test Account B` receives it in real time (then reply `L3-PONG-TEST`).
  2. Attach and send the synthetic file `dummy-qa-test.txt` (`QA_DUMMY_PAYLOAD`) from `Test Account A` to `Test Account B`, and click **Lưu** / **Download** inside `Test Account B`.
  3. Verify the downloaded file lands inside `Test Account B`'s isolated profile folder (`<DataDir>\profiles\<AccountB-Slug>\Documents\Zalo Received Files\dummy-qa-test.txt`) and **not** inside the computer's main `%USERPROFILE%\Documents\Zalo Received Files\`.
- **Mandatory Privacy & Evidence Rule:**
  - Record **only** `PASS` / `FAIL` and whether file path isolation succeeded.
  - **NEVER** copy message text, phone numbers, real contact names, profile avatars, or chat window screenshots into test logs, Markdown reports, or `evidence/` folders!

---

### Check 8 (`L3-ACC-08`) — Honest Verification & Disclosure of Voice/Video Call & Windows Toast Boundaries
- **Status Today:** `[ ] NOT_TESTED` *(Human UX Walkthrough)* | **Vendor Architecture:** Calls `BLOCKED` *(Concurrent/Secondary)*, Windows Toast `PARTIALLY_SUPPORTED`
- **Live Host Preflight Fact Verified Today:**
  - Read-only inspection via `run-l3-guided-acceptance.ps1 -NonInteractive` confirmed `ZaloCall.exe` is **actively running even while Primary Zalo is idle** (`primary_zalocall_pipe_holder_active: true`), holding global Windows named pipes `\\.\pipe\PipeZCallRecv` and `\\.\pipe\PipeZCallSend`.
- **How to Verify Honestly (Step by Step):**
  1. Open Multi-Zalo -> click **Trợ giúp** (*Help*) in the top navigation bar -> read Card 4 (**"4. Xử lý lỗi thường gặp & giới hạn thực tế"** / **"4. Common issues & known limits"**). Confirm the UI clearly warns users in plain language about call limits without promising simultaneous multi-window calls.
  2. **Notifications (`com.vng.zalo` vs `notiWin`):**
     - Minimize `Test Account B` and send `L3-PING-TEST` from `Test Account A`.
     - Observe Zalo's built-in bottom-right floating banner (`notiWin`, `352x82`): clicking this in-app banner opens the **exact receiving account (`Test Account B`)**.
     - Observe Windows 11 Action Center history: because official `Zalo.exe` hardcodes `AppUserModelId("com.vng.zalo")`, native Windows Action Center groups banners under a single `"Zalo"` header, and clicking an old entry from Action Center history may route via `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Zalo.lnk` to Primary Zalo. Confirm testers/users are guided to keep Zalo's in-app popup notifications enabled.
  3. **Voice/Video Calls (`\\.\pipe\PipeZCallRecv` & `PipeZCallSend`):**
     - Confirm that because the **first signed-in Zalo instance** spawns `ZaloCall.exe` and binds global `\\.\pipe\PipeZCallRecv` + `PipeZCallSend` for its entire session, secondary Zalo windows **cannot** independently place/receive calls at the same time.
     - Verify the documented **sequential single-account call handoff** workaround: closing other open Zalo windows first and reopening only the account needed for a call allows that account to bind `PipeZCall*` cleanly without binary/ASAR modification.
- **Expected Result (`PASS` Criteria for Honest Acceptance):**
  - Multi-Zalo UI (`vi-VN` & `en-US`) and user documentation disclose both vendor boundaries transparently in plain, non-deceptive language, and sequential single-account call handoff + in-app `notiWin` click-to-focus behave as documented.

---

## 4. Post-Test Cleanup & Sign-Off Verification

1. Click **Đóng Zalo** (*Close Zalo*) on both `Test Account A` and `Test Account B` until Multi-Zalo shows **"Chưa có cửa sổ nào đang mở"** (*"No windows open"*).
2. If using temporary test accounts on a shared QA workstation, sign out both test sessions from **Phone A** and **Phone B** (*Quản lý thiết bị đăng nhập* / *Logged-in devices*) before clicking `⋯` -> **Xóa tài khoản** (*Remove account*).
3. Confirm the guided recorder output JSON reports:
   - `safety_and_privacy_invariants.primary_zalo_unchanged == true` (`%APPDATA%\ZaloData\config.json` SHA-256 unchanged).
   - `safety_and_privacy_invariants.chat_messages_or_pii_captured == false`.
   - Zero occurrences of `<USER_HOME>` raw paths, usernames, hostnames, or phone numbers in the saved JSON.
