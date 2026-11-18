# Ma Trận Nghiệm Thu Đa Nền Tảng — Multi-Zalo Desktop (`v0.1.0-beta.1`)

> **Mục đích:** Tách bạch trung thực kết quả kiểm thử giữa nguồn đã biên dịch (`SOURCE_COMPILED`), kiểm thử giả lập (`NATIVE_FIXTURE_PASSED`), kiểm thử trình quản lý thật (`NATIVE_MANAGER_SMOKE_PASSED`), và kiểm thử tài khoản Zalo thật (`LIVE_ZALO_MULTIPLE_ACCOUNTS_VERIFIED`).

## 1. Mức Độ Kiểm Chứng Tổng Thể Theo Hệ Điều Hành

| Hệ điều hành / Kiến trúc | Mức kiểm chứng hiện tại | Bằng chứng xác thực | Giới hạn & điều kiện còn lại |
| :--- | :--- | :--- | :--- |
| **Windows 10/11 (`x86_64`)** | `NATIVE_MANAGER_SMOKE_PASSED` *(L1/L2 Zalo thật `26.10.20` đã qua ở PoC trước; L3 QR 2 tài khoản `NOT_TESTED`)* | `cargo test --workspace` + `src-tauri`, `xf-test-runner` `53/55 PASS`, `run-fixture-tests.mjs` `8/8`, `run-contract-parity-tests.mjs`, `run-tauri-native-e2e.mjs` `12/12` | `UNSIGNED_DEVELOPMENT_ARTIFACT` (bị chặn trên máy bật `SAC=1` nếu không dùng Signed-Host Bridge); gọi thoại đồng thời bị chặn bởi `PipeZCall*`; L3 2-account QR cần chủ dự án tự quét. |
| **macOS 15 (`arm64` Apple Silicon)** | `SOURCE_COMPILED` *(+ unit/scenario tests trên Windows host; chờ native `macos-15` CI)* | `dim-platform-macos` `14/14` unit/scenario tests + `XP-MAC-01..03`; `XP-MAC-04` `SKIPPED_HOST_PLATFORM` trên Windows | Chưa chạy trên kernel Darwin thực và `/Applications/Zalo.app` thật; chưa ghim `TeamIdentifier` 10 ký tự của VNG; `UNSIGNED` / `NOT_NOTARIZED`; socket `/tmp/socketzalo*2021` giới hạn gọi tuần tự. |
| **macOS 15 (`x86_64` Intel)** | `SOURCE_COMPILED` *(chờ native `macos-15-intel` CI)* | Chung mã nguồn `dim-platform-macos` với `arm64` | Như `arm64`, phụ thuộc khả dụng runner `macos-15-intel` và chưa test phần cứng Intel Mac thật. |
| **Ubuntu 24.04 LTS (`x86_64`)** | `SOURCE_COMPILED` *(+ `46/46` portable Linux tests trên Windows host; chờ native `ubuntu-24.04` CI)* | `dim-platform-linux` `46/46` tests + `XP-LNX-01..03`; `XP-LNX-04` `SKIPPED_HOST_PLATFORM` trên Windows | **Zalo Desktop chính thức: `UNSUPPORTED`** (VNG không phát hành bản Linux). Manager & AppImage ở mức `EXPERIMENTAL`, chưa kiểm thử Wayland `second-instance` (`F-UBU-05`). |

---

## 2. Ma Trận Chức Năng Chi Tiết (`Feature × Windows × macOS × Ubuntu × Evidence × Status`)

| Chức năng | Windows | macOS | Ubuntu | Evidence | Status |
| :--- | :---: | :---: | :---: | :--- | :--- |
| **Phát hiện Zalo Desktop** | `PASS` (`WinVerifyTrust` + `VNG GROUP JSC`) | `SOURCE_COMPILED` *(fail-closed chờ `TeamIdentifier`)* | `UNSUPPORTED` *(chính thức)* / `EXPERIMENTAL` *(AppImage cục bộ)* | `adapter.rs`, `run-contract-parity-tests.mjs`, `XP-WIN/MAC/LNX-01` | `PARTIAL_CROSS_PLATFORM` |
| **Tạo không gian tài khoản** | `PASS` | `SOURCE_COMPILED` | `SOURCE_COMPILED` | `dim-profile-store`, `run-tauri-native-e2e.mjs` Step 3 | `VERIFIED_WIN_CORE` |
| **Đặt / đổi tên tài khoản (NFC Unicode)** | `PASS` | `SOURCE_COMPILED` | `SOURCE_COMPILED` | `CT-08`, `CT-10`, `run-tauri-native-e2e.mjs` Step 4 | `VERIFIED_WIN_CORE` |
| **Mở phiên độc lập** | `PASS` (`NATIVE_MANAGER_SMOKE_PASSED`) | `NOT_TESTED` *(chờ macOS CI)* | `NOT_TESTED` *(chờ Ubuntu CI, chỉ fixture/AppImage)* | `run-tauri-native-e2e.mjs` Step 5, `XP-WIN-02/03` | `VERIFIED_WIN_FIXTURE` |
| **Chuyển đến cửa sổ (Focus)** | `PASS` (`6-cond EnumWindows` + Foreground Lock) | `NOT_TESTED` *(AppKit `activateWithOptions:3`)* | `PARTIAL` *(X11 `wmctrl`/`xdotool`; Wayland `OPEN` P1)* | `tc_h2`, `tc_h6`, `run-tauri-native-e2e.mjs` Step 6 | `VERIFIED_WIN_FIXTURE` |
| **Đóng riêng từng phiên** | `PASS` (`RestartManager` + causal `GetProcessTimes`) | `NOT_TESTED` *(`lsof` WAL guard)* | `NOT_TESTED` *(`/proc/<pid>/fd` WAL guard)* | `REL-07`, `run-tauri-native-e2e.mjs` Step 7 | `VERIFIED_WIN_FIXTURE` |
| **Khởi động lại & giữ phiên** | `PASS` *(fixture state; L3 DPAPI `NOT_TESTED`)* | `NOT_TESTED` | `NOT_TESTED` | `run-fixture-tests.mjs` Test-4, `run-tauri-native-e2e.mjs` Step 11 | `VERIFIED_WIN_FIXTURE` |
| **Chống mở trùng ngoài ý muốn** | `PASS` (`LaunchAdmissionClaim` + `DUPLICATE_ZALO_USER_ID`) | `SOURCE_COMPILED` | `SOURCE_COMPILED` | `p0_coordinator_concurrency`, `run-fixture-tests.mjs` Test-3/6 | `VERIFIED_WIN_CORE` |
| **Khôi phục sau crash (`recovery_required`)** | `PASS` | `SOURCE_COMPILED` | `SOURCE_COMPILED` | `REL-08`, `run-fixture-tests.mjs` Test-5, `ui-contracts.test.mjs` | `VERIFIED_WIN_CORE` |
| **Sao lưu nguyên tử SHA-256** | `PASS` | `SOURCE_COMPILED` | `SOURCE_COMPILED` | `REL-04`, `run-tauri-native-e2e.mjs` Step 8 | `VERIFIED_WIN_CORE` |
| **Khôi phục backup thành profile mới** | `PASS` | `SOURCE_COMPILED` | `SOURCE_COMPILED` | `data_safety.rs`, `run-tauri-native-e2e.mjs` Step 9 | `VERIFIED_WIN_CORE` |
| **Đổi tên / Xóa profile an toàn** | `PASS` (`VerifiedDeletionState` + junction guard) | `SOURCE_COMPILED` | `SOURCE_COMPILED` | `p0_store_reliability`, `tc_h7`, `run-tauri-native-e2e.mjs` Step 10 | `VERIFIED_WIN_CORE` |
| **Đóng Manager & mở lại** | `PASS` | `NOT_TESTED` | `NOT_TESTED` | `run-tauri-native-e2e.mjs` Step 11 | `VERIFIED_WIN_NATIVE` |
| **Bảo vệ Zalo chính** | `PASS` (`config.json` SHA-256 bất biến 100%) | `SOURCE_COMPILED` | `SOURCE_COMPILED` | `CT-06`, `PRIMARY_ZALO_CONFIG_BEFORE == AFTER` | `VERIFIED_WIN_HOST` |
| **Thao tác đồng thời (same/distinct profile)** | `PASS` | `SOURCE_COMPILED` | `SOURCE_COMPILED` | `p0_coordinator_concurrency`, `REL-05` | `VERIFIED_WIN_CORE` |
| **Giao diện `vi-VN` / `en-US` & Dark/Light** | `PASS` | `NOT_TESTED` *(WKWebView)* | `NOT_TESTED` *(WebKitGTK)* | `i18n.test.mjs`, `ui-contracts.test.mjs`, `run-tauri-native-e2e.mjs` | `VERIFIED_WIN_NATIVE` |
| **Keyboard accessibility & OS text-scale** | `PASS` | `NOT_TESTED` | `NOT_TESTED` | `ui-contracts.test.mjs`, `meta[name="text-scale"]`, `styles.css` | `VERIFIED_WIN_NATIVE` |
| **Trạng thái lỗi (kể cả `profiles.json` hỏng lúc mở app)** | `PASS` | `SOURCE_COMPILED` | `SOURCE_COMPILED` | Live `dim-desktop.exe` probe (`JSON_ERROR`, file gốc bất biến) | `VERIFIED_WIN_NATIVE` |
| **L3 — QR Login 2 tài khoản Zalo thật** | `NOT_TESTED` *(LIVE_ZALO_NOT_TESTED)* | `NOT_TESTED` | `UNSUPPORTED` | Cần chủ dự án chạy `huong-dan-cai-dat-va-kiem-thu-vi.md` | `PENDING_HUMAN_L3` |
