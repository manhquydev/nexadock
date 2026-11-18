# Đặc Tả Kiểm Thử Thực Thi Cho Giả Thuyết Tương Thích Đa Nền Tảng (`test-specifications.md`)

**Nhánh nghiên cứu:** `research/desktop-compatibility`  
**Phạm vi tài liệu:** Đặc tả kiểm thử độc lập, khả thi ngay (self-contained & immediately actionable) cho toàn bộ nhóm giả thuyết `H1`–`H8` (`TC-H1A`, `TC-H1B`, `TC-H2`..`TC-H8`), liên kết đồng bộ 100% với mã phát hiện (`F-CORE-01..02`, `F-WIN-01..07`, `F-MAC-01..05`, `F-UBU-01..07`) và yêu cầu tích hợp (`IR-WIN-01..04`, `IR-MAC-01..04`, `IR-UBU-01..05`). Một agent triển khai trên Windows, macOS hoặc Ubuntu có thể nhận bất kỳ `TC-H*` nào dưới đây và thực thi ngay mà không cần đọc lại các báo cáo nghiên cứu nền.

---

## 1. Quy Tắc An Toàn Bắt Buộc & Hệ Phân Loại Bằng Chứng 6 Bậc

### 1.1. Ranh giới an toàn tuyệt đối cho mọi bài kiểm thử (`STRICT SAFETY INVARIANTS`)
1. **Không dùng tài khoản Zalo thật trong kiểm thử tự động:** 100% các bài kiểm thử tự động (`TC-H1A`, `TC-H2` Phần 2A, `TC-H3`, `TC-H4` Phần 4A–4C, `TC-H5`, `TC-H6`, `TC-H7`, `TC-H8`) chỉ sử dụng fixture tổng hợp (`fixtures/test-desktop-app/fixture-app.mjs`, probe Node.js/Rust cục bộ, thư mục dữ liệu tạm cô lập) và đặt tên tài nguyên IPC thử nghiệm tách biệt hoàn toàn với tài nguyên của Zalo thật.
2. **Không tự động hóa xác thực hoặc cuộc gọi:** Tuyệt đối không dùng script quét mã QR, không tự động nhập mật khẩu/OTP, không trích xuất cookie/khóa phiên, và **tuyệt đối không tự động phát cuộc gọi hay tin nhắn tới bất kỳ người dùng không đồng thuận nào**. Kịch bản thủ công (`TC-H1B`, và phần tùy chọn của `TC-H2`/`TC-H4`) chỉ được thực hiện bởi kiểm thử viên con người tự nguyện (`Human QA opt-in`) trên chính các tài khoản thử nghiệm thuộc quyền sở hữu của người đó.
3. **Không can thiệp nhị phân, không hạ cấp bảo mật hệ điều hành:** Tuyệt đối không sửa đổi `Zalo.exe`, `/Applications/Zalo.app`, `app.asar`, `ZaloCall.exe` hay `Zalo.lnk`; không inject DLL/hook API; không tắt Windows Smart App Control (`SAC`); không tắt Ubuntu AppArmor (`sysctl kernel.apparmor_restrict_unprivileged_userns=0`) và **không bao giờ gắn cờ `--no-sandbox` / `ELECTRON_DISABLE_SANDBOX=1`** để ép bài test đạt `PASS`; không giả mạo máy chủ/endpoint của VNG.
4. **Bảo vệ dữ liệu người dùng và quyền riêng tư log:** Mọi lệnh kiểm thử phải truyền `--data-dir` vào thư mục tạm cô lập (không bao giờ trỏ vào `%APPDATA%\ZaloData`, `~/Library/Application Support/ZaloData` hay `~/.config/ZaloData`), và mọi tệp bằng chứng JSON/log xuất ra phải ẩn danh tên người dùng cục bộ (`%USERPROFILE%`, `<USER_HOME>`, `~`).

### 1.2. Hệ phân loại bằng chứng 6 bậc dùng trong trường `Evidence hiện có`
- `Confirmed by source code`: Xác minh trực tiếp trong mã nguồn Rust/JS/PS1 của repo, gói `app.asar` chính thức (`Zalo 26.10.20`), header SDK hệ điều hành hoặc mã nguồn tham chiếu đã ghim commit.
- `Confirmed by static binary inspection`: Xác minh qua phân tích tĩnh nhị phân không xâm lấn (bảng PE import, Electron Fuses, cấu trúc `.lnk`, chữ ký Authenticode/Mach-O với mã bằng chứng REA `ev_*`).
- `Observed on running system`: Quan sát thụ động chỉ đọc trên một hệ thống đang chạy (ví dụ liệt kê tiến trình/pipe ở trạng thái nghỉ), chưa qua thực nghiệm kiểm soát đa biến.
- `Confirmed by controlled experiment`: Đã chứng minh bằng kịch bản thực nghiệm có đối chứng, lặp lại được và có tệp kết quả lưu trữ (`fixture-test-result.json`, `zalo-live-poc-result.json`).
- `Inference`: Suy luận kỹ thuật có căn cứ từ mã nguồn và kiến trúc OS/Electron nhưng chưa chạy kiểm chứng đầu-cuối cho kịch bản cụ thể đó.
- `Not tested`: Chưa thực thi kiểm thử trên phần cứng mục tiêu hoặc chưa kiểm thử thủ công với tài khoản thật.

### 1.3. Thang ưu tiên theo giá trị sản phẩm (`Product-Value Priority`)
- **`P0` (Chặn phát hành / Bảo toàn dữ liệu & Đăng nhập cốt lõi):** Chống mất dữ liệu (`backup` thiếu DB ngoài, xóa nhầm qua NTFS junction, `SIGTERM`/`SIGKILL` sớm khi đang đóng SQLite WAL), chống đóng nhầm tiến trình/cửa sổ (`EnumWindows` bắt nhầm popup, tái sử dụng PID), sửa lỗi chặn khởi chạy đa phiên (`TMPDIR` > 104 byte trên macOS, `bwrap --die-with-parent`, preflight AppArmor `userns`), và xác minh đăng nhập độc lập tài khoản thứ 2 (`L3` Giai đoạn 1).
- **`P1` (Trải nghiệm sử dụng cốt lõi hằng ngày):** Nhắn tin, gửi/lưu tệp qua hộp thoại chuẩn (`Downloads`, `.config/user-dirs.dirs`), giữ phiên khi đóng/mở lại và đánh thức/chuyển đổi cửa sổ từ System Tray (`focus_or_restore`).
- **`P2` (Tính năng phụ trợ — Không bao giờ xếp trước `P0`/`P1`):** Tối ưu hiển thị thông báo (`notiWin`, Dock badge) và chẩn đoán/điều phối kênh Gọi thoại & Gọi video (`PipeZCall*`, `/tmp/socketzalo*2021`, cổng `29631/29632`).

---

## 2. Bảng Truy Xuất Tổng Thể Các Đặc Tả Kiểm Thử (`TC-H1A` – `TC-H8`)

| Test ID | Giả thuyết | Finding IDs (`F-*`) | Integration Requests (`IR-*`) | Ưu tiên | Hệ điều hành | Chế độ | Người/agent chịu trách nhiệm |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **`TC-H1A`** | `H1` (Phần tự động) | `F-WIN-06` | `IR-WIN-02` | `P2` | Windows 10/11 x64 | Tự động (Synthetic Pipe + Passive Probe) | `Windows agent` |
| **`TC-H1B`** | `H1` (Phần thủ công 2 giai đoạn) | `F-WIN-06`, `F-WIN-07`, `F-WIN-01` | `IR-WIN-02`, `IR-WIN-04` | `P0` (Stage 1 L3 Login) / `P2` (Stage 2 Call) | Windows 10/11 x64 | Thủ công có kiểm soát (Zero Automation) | `Human QA opt-in` |
| **`TC-H2`** | `H2` | `F-WIN-02`, `F-WIN-05` | `IR-WIN-01`, `IR-WIN-02` | `P0` (Window Filter) / `P2` (`notiWin` UX) | Windows 10/11 x64 | Tự động (Fixture HWND) + Thủ công tùy chọn | `Windows agent` (`P0`) / `Human QA opt-in` (`P2`) |
| **`TC-H3`** | `H3` | `F-MAC-01` | `IR-MAC-01` | `P0` | macOS 14+ (`arm64` / `x86_64`) | Tự động (Synthetic Socket & `open -n` Probe) | `macOS agent` |
| **`TC-H4`** | `H4` | `F-MAC-02`, `F-MAC-03`, `F-MAC-04` | `IR-MAC-02`, `IR-MAC-03` | `P0` (Stop & PID Guard) / `P1` (Activate & Keychain) | macOS 14+ (`arm64` / `x86_64`) | Tự động (Fixture App) + Thủ công tùy chọn | `macOS agent` (`P0`/`P1`) / `Human QA opt-in` (Keychain) |
| **`TC-H5`** | `H5` | `F-MAC-05`, `F-MAC-04` | `IR-MAC-04` | `P2` | macOS 14+ (`arm64` / `x86_64`) | Tự động (Synthetic Unix Socket Harness) | `macOS agent` |
| **`TC-H6`** | `H6` | `F-WIN-03` | `IR-WIN-01` | `P1` | Windows 10/11 x64 | Tự động (Hidden Tray HWND + Foreground Lock) | `Windows agent` |
| **`TC-H7`** | `H7` | `F-WIN-04`, `F-CORE-01`, `F-CORE-02` | `IR-WIN-03`, `IR-WIN-04` | `P0` (Delete & DB Guard) / `P1` (File Dialog Layout) | Windows 10/11 x64 & Cross-platform | Tự động (Canary Junction + Synthetic `config.json`) | `Windows agent` |
| **`TC-H8`** | `H8` (Phần 8A, 8B, 8C) | `F-UBU-01`, `F-UBU-02`, `F-UBU-03`, `F-UBU-04`, `F-UBU-05`, `F-UBU-06`, `F-UBU-07` | `IR-UBU-01`, `IR-UBU-02`, `IR-UBU-03`, `IR-UBU-04`, `IR-UBU-05` | `P0` (8A, 8B) / `P1` (8A) / `P2` (8C) | Ubuntu 22.04 LTS & 24.04 LTS | Tự động (`bwrap` + AppArmor Preflight + `pasta`) | `Ubuntu agent` |

---

## 3. Chi Tiết Đặc Tả 9 Bài Kiểm Thử (Đủ 11 Trường Bắt Buộc)

---

### `TC-H1A` — Kiểm thử tự động cơ chế xung đột Named Pipe hai pha & Bộ dò chủ sở hữu Pipe thụ động trên Windows (`P2`)

1. **`Test ID`:** `TC-H1A` (Liên kết: `F-WIN-06`, `IR-WIN-02` | Ưu tiên: `P2`)
2. **`Hypothesis`:** Trong kiến trúc IPC tương đương module `"3Zc2"` (`main.js:311828..317500`), hai hiện tượng kỹ thuật có thể được phân tách và chứng minh độc lập trên cặp Named Pipe tổng hợp (`\\.\pipe\DimTestCallRecv_<run_id>`, `\\.\pipe\DimTestCallSend_<run_id>`) mà không chạm Zalo thật: **(a) Pha khởi tạo pipe server ở thời điểm đăng nhập/nghỉ (login-time / idle):** Phiên #1 giữ mở `DimTestCallRecv`/`DimTestCallSend`, khiến Phiên #2 nhận lỗi `EADDRINUSE` trên `listen()` nhưng luồng chính vẫn sống bình thường; **(b) Pha khởi chạy helper gọi điện (call-initiation / helper-spawn):** Nếu Phiên #2 vẫn spawn tiến trình helper con kết nối khách (`net.connect`) vào `DimTestCallSend`, kết nối mới đó lọt vào `serverSend` của Phiên #1 và ghi đè biến hàm gửi `O` của Phiên #1 sang helper của Phiên #2, trong khi hàng đợi gửi của Phiên #2 bị treo ở trạng thái chưa sẵn sàng. Đồng thời, bộ dò Win32 thụ động (`GetNamedPipeServerProcessId` / liệt kê chỉ đọc `\\.\pipe\`) xác định chính xác PID chủ sở hữu pipe mà không làm gián đoạn hay ghi đè callback `O` của server đang mở.
3. **`Evidence hiện có`:**
   - `Confirmed by source code`: `app.asar/main-dist/main.js:311828..317500` đặt hằng số `\\.\pipe\PipeZCallRecv` và `\\.\pipe\PipeZCallSend`; nhánh `win32` không xóa/đổi tên pipe khi gặp `EADDRINUSE`; callback `A = net.createServer(e => { ... O = t => V(e, t) })` gán lại `O` cho mọi kết nối client mới tới `PipeZCallSend`.
   - `Confirmed by static binary inspection`: Phân tích PE import trên `ZaloCall.exe` (`ev_9145f8bd574a583b54d530119265f1862c17e899a13502359500acb527cceda0`) xác nhận dùng `WINMM.dll` và `qtaudio_wasapi.dll` (WASAPI Shared Mode — phần cứng âm thanh không khóa độc quyền).
   - `Observed on running system`: Trên máy Windows 11 nghiên cứu, 1 phiên Zalo chính đã đăng nhập nhưng đang nghỉ (idle) có sẵn tiến trình con `ZaloCall.exe` và giữ mở cả hai pipe `PipeZCallRecv`/`PipeZCallSend`.
   - `Inference` / `Not tested` (hiệu chỉnh từ báo cáo cũ): Quan sát 1 phiên nghỉ trên host chưa chứng minh bằng thực nghiệm có kiểm soát rằng Tài khoản #2 lỗi ở mọi phương thức gọi (ví dụ WebRTC nội bộ / `zcall_ia32.node`), chưa chứng minh trên Zalo thật hàm `K()` chạy ngay lúc vừa đăng nhập Tài khoản #2 hay chỉ khi bấm Gọi, và chưa chứng minh việc ghi đè `O` làm mất khả năng gọi đi của Tài khoản #1 ngoài đời thực (phần ứng dụng thật do `TC-H1B` kiểm chứng thủ công).
4. **`Preconditions`:**
   - Máy chạy Windows 10/11 x64 không nâng quyền Admin (`is_elevated == false`).
   - Có sẵn `node.exe` (`>= 20.x`), `powershell.exe`, và toolchain Rust (`cargo test -p dim-platform-windows`).
   - Tuyệt đối không mở, không đóng và không can thiệp bất kỳ tiến trình `Zalo.exe` hay đường ống `\\.\pipe\PipeZCall*` thật nào nếu đang tồn tại trên máy.
5. **`Test fixture`:**
   - Harness Node.js độc lập `tests/SyntheticCallPipeHarness` (hoặc kịch bản kiểm thử trong `crates/platform-windows/tests/tc_h1a_pipe_probe.rs` kết hợp `node -e` inline) chỉ sử dụng tên pipe ngẫu nhiên mang tiền tố riêng: `\\.\pipe\DimTestCallRecv_<8hex>` và `\\.\pipe\DimTestCallSend_<8hex>`.
   - Hỗ trợ 2 cờ mô phỏng rõ rệt cho Phiên #2: `--spawn-helper-on-start=false` (mô phỏng chỉ tạo server ở pha đăng nhập, chưa spawn helper gọi) và `--spawn-helper-on-start=true` (mô phỏng spawn helper kết nối vào `DimTestCallSend` ngay sau `EADDRINUSE`).
6. **`Các bước xác minh an toàn`:**
   1. Tạo thư mục bằng chứng tạm `$TestDir = Join-Path $env:TEMP "dim-tc-h1a-$([guid]::NewGuid().ToString('N').Substring(0,8))"` và sinh `run_id` 8 ký tự hex.
   2. **Kịch bản A (Phân biệt Idle Server vs Call-Time Helper):**
      - Khởi chạy **Fixture Instance 1** lắng nghe trên `\\.\pipe\DimTestCallRecv_<run_id>` & `DimTestCallSend_<run_id>` và spawn `Helper 1` kết nối vào cả 2 pipe. Gửi thông điệp thử `"PING_FROM_1"` qua hàm `O` của Instance 1 -> xác nhận `Helper 1` nhận đúng `"PING_FROM_1"`.
      - Khởi chạy **Fixture Instance 2** ở chế độ `--spawn-helper-on-start=false` (chỉ thử `listen()`): xác nhận Instance 2 ghi nhận `EADDRINUSE` trên cả 2 pipe, tiến trình chính Instance 2 vẫn sống (`exitCode === null`), và khi Instance 1 gửi `"PING_AFTER_IDLE_2"`, **`Helper 1` vẫn nhận đúng 100%** (callback `O` của Instance 1 chưa bị ghi đè!).
   3. **Kịch bản B (Mô phỏng Ghi đè Callback `O` khi Spawn Helper):**
      - Từ Instance 2, kích hoạt hành động `initiateSyntheticCall()` (spawn `Helper 2` gọi `net.connect('\\\\.\\pipe\\DimTestCallSend_<run_id>')`).
      - Gửi `"CALL_CMD_FROM_1"` từ Instance 1 và `"CALL_CMD_FROM_2"` từ Instance 2: đo xem gói `"CALL_CMD_FROM_1"` bị chuyển nhầm sang `Helper 2` (chứng minh cơ chế ghi đè `O`) và `"CALL_CMD_FROM_2"` nằm lại trong `pendingQueue` của Instance 2.
   4. **Kịch bản C (Kiểm chứng Bộ dò Pipe Thụ động Chỉ Đọc cho `IR-WIN-02`):**
      - Khôi phục Instance 1 sạch, chạy hàm Rust `query_named_pipe_owner_pid("\\\\.\\pipe\\DimTestCallRecv_<run_id>")` (mở handle với `FILE_READ_ATTRIBUTES` + `GetNamedPipeServerProcessId`, đóng handle ngay qua `HandleGuard`, tuyệt đối không ghi/đọc luồng dữ liệu) và liệt kê `[System.IO.Directory]::GetFiles('\\.\pipe\') | Where-Object { $_ -like "*DimTestCall*" }`.
      - Ngay sau khi bộ dò chạy xong, gửi `"PING_POST_PROBE"` từ Instance 1 -> xác nhận `Helper 1` vẫn nhận nguyên vẹn (chứng minh bộ dò thụ động **không** kích hoạt sự kiện `connection` mới hay làm hỏng `O`!).
   5. Đóng toàn bộ tiến trình fixture, xác nhận không còn pipe `DimTestCall*_<run_id>` tồn tại và xóa thư mục tạm công việc (giữ lại tệp JSON kết quả).
7. **`Expected behavior`:**
   - Ở Kịch bản A, Instance 2 gặp `EADDRINUSE` nhưng không crash và **không** làm hỏng kênh gửi của Instance 1 chừng nào `Helper 2` chưa kết nối vào `DimTestCallSend`.
   - Ở Kịch bản B, ngay khi `Helper 2` gọi `net.connect` vào pipe `PIPE_UNLIMITED_INSTANCES` của `libuv`, biến callback `O` của Instance 1 bị trỏ sang socket của `Helper 2`.
   - Ở Kịch bản C, hàm chẩn đoán trả về đúng `server_pid == instance1.pid` trong `< 50ms` và không làm phát sinh kết nối khách mới trên `net.Server` của Instance 1.
8. **`Failure conditions`:**
   - Bộ dò thụ động (`GetNamedPipeServerProcessId` / `CreateFileW`) vô tình mở kết nối client đầy đủ làm ghi đè callback `O` hoặc trả về `ERROR_PIPE_BUSY` / sai PID.
   - Bài kiểm thử chạm vào tên pipe thật `PipeZCallRecv` / `PipeZCallSend` hoặc chạm vào bất kỳ PID nào của Zalo thật.
   - Còn sót tiến trình `node.exe` con hoặc pipe `DimTestCall*` sau khi kết thúc.
9. **`OS`:** Windows 10 / Windows 11 x64.
10. **`Bằng chứng cần lưu`:**
    - Tệp JSON `evidence/tc-h1a-pipe-harness-result.json` chứa: `executed_at_utc`, `scenario_a_idle_no_hijack: true`, `scenario_b_helper_connect_overwrites_callback: true`, `scenario_c_passive_probe_pid_match: true`, `post_probe_channel_intact: true`, danh sách mã lỗi `EADDRINUSE`, và mã thoát `0`.
11. **`Người/agent chịu trách nhiệm`:** `Windows agent`.

---

### `TC-H1B` — Quy trình xác minh thủ công 2 giai đoạn với tài khoản đồng thuận: Đăng nhập độc lập `L3` (`P0`) & Hành vi cuộc gọi thực tế (`P2`)

1. **`Test ID`:** `TC-H1B` (Liên kết: `F-WIN-07`, `F-WIN-06`, `F-WIN-01`, `IR-WIN-02`, `IR-WIN-04` | Ưu tiên: **`P0` cho Giai đoạn 1 (`L3`)** / **`P2` cho Giai đoạn 2 tùy chọn (`L4-2`)**)
2. **`Hypothesis`:**
   - **Giai đoạn 1 (`P0` — Đăng nhập độc lập `L3`, Nhắn tin & Giữ phiên DPAPI):** Hai profile phụ cô lập (`Account Alpha`, `Account Beta`) mở qua `scripts/dim-host.ps1` cho phép người kiểm thử quét mã QR đăng nhập 2 tài khoản Zalo độc lập chạy song song với Zalo chính, tự động nhận diện 2 `zalo_user_id` khác nhau, gửi/nhận tin nhắn & tệp đính kèm cô lập hoàn toàn trong từng `<ProfileDir>`, giữ nguyên trạng thái đăng nhập sau khi đóng êm (`stop`) và mở lại (`launch`), và **không hiển thị lỗi hay tự đóng ứng dụng ở thời điểm đăng nhập/nhắn tin**.
   - **Giai đoạn 2 (`P2` — Tùy chọn kiểm chứng thời điểm phát sinh lỗi cuộc gọi):** Xác định thực nghiệm trên Zalo `26.10.20` xem việc mở và đăng nhập Tài khoản #2 có spawn `ZaloCall.exe` ngay lúc đăng nhập hay chỉ khi bấm nút Gọi, Tài khoản #2 báo lỗi ở bước nào, và cuộc gọi đi từ Tài khoản #1 (giữa chính 2 tài khoản của kiểm thử viên) có bị ảnh hưởng hay khôi phục sạch sau khi đóng êm Tài khoản #2 hay không.
3. **`Evidence hiện có`:**
   - `Confirmed by controlled experiment`: Chạy song song 3 phiên phụ + 1 phiên Zalo chính ở màn hình QR (`evidence/zalo-live-poc-result.json`) giữ bất biến 100% SHA-256 của `%APPDATA%\ZaloData\{config.json, startup.log, log.log}`; chốt chặn trùng `zalo_user_id` đạt `PASS` trên fixture (`Test-6` trong `evidence/fixture-test-result.json`).
   - `Confirmed by static binary inspection` & `Confirmed by source code`: Electron Fuse `EnableCookieEncryption` bật (`ev_8e05df8e...`) kết hợp `SecureCipher` (`secure_ks_<hash>` tại `main.js:3561081`) mã hóa theo DPAPI của Windows user hiện tại trong từng `<ProfileDir>\AppData\Roaming\ZaloData`.
   - `Not tested` / `Inference`: Chưa thực hiện quét 2 mã QR thật trên máy nghiên cứu hiện tại (`L3-1..L3-4` và `L4-2` đang ở trạng thái `NOT_TESTED`); tương tác cuộc gọi thực tế giữa Tài khoản #1 và Tài khoản #2 là `Inference` / `Not tested`.
4. **`Preconditions`:**
   - **RÀNG BUỘC ĐẠO ĐỨC & AN TOÀN BẮT BUỘC:** **TUYỆT ĐỐI KHÔNG TỰ ĐỘNG HÓA (`NEVER AUTOMATED`), KHÔNG BAO GIỜ DÙNG BOT QUÉT QR/NHẬP MẬT KHẨU, VÀ TUYỆT ĐỐI KHÔNG GỌI ĐIỆN TỚI BẤT KỲ NGƯỜI DÙNG NÀO KHÔNG ĐỒNG THUẬN (`NEVER CALLS NON-CONSENTING USERS`)!**
   - Kiểm thử viên con người tự nguyện (`Human QA opt-in`) chuẩn bị 2 tài khoản Zalo hợp lệ của chính mình trên 2 thiết bị di động đặt cạnh máy tính.
   - Ghi lại mã băm SHA-256 ban đầu của `%LOCALAPPDATA%\Programs\Zalo\Zalo-26.10.20\Zalo.exe` và `resources\app.asar` trước khi bắt đầu.
5. **`Test fixture`:**
   - Script tương tác có sẵn `scripts/run-interactive-l3-checklist.ps1` chạy trên bộ nhị phân Zalo Desktop chính thức nguyên bản có chữ ký số `VNG GROUP JSC`, lưu dữ liệu thử nghiệm tách biệt tại `-DataDir "$env:TEMP\dim-l3-checklist"`.
6. **`Các bước xác minh an toàn`:**
   1. **Giai đoạn 1 (`P0` — Bắt buộc thực hiện trước mọi công việc `P2` về cuộc gọi):**
      - Mở PowerShell thường (không Admin) tại thư mục `desktop-instance-manager` và chạy:
        ```powershell
        powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\run-interactive-l3-checklist.ps1 `
          -DataDir "$env:TEMP\dim-l3-checklist" `
          -OutFile ".\evidence\l3-interactive-checklist-result.json"
        ```
      - **`L3-1`:** Tự tay quét mã QR Tài khoản thử nghiệm #1 vào cửa sổ `Account Alpha` và Tài khoản thử nghiệm #2 vào `Account Beta`. Quan sát xem cả 2 đăng nhập thành công mà không hiện popup lỗi nào, đồng thời Zalo chính (nếu đang mở) không bị đăng xuất.
      - **`L3-2`:** Chạy lệnh chỉ đọc kiểm tra `status` trên `-DataDir "$env:TEMP\dim-l3-checklist"` (trong một cửa sổ PowerShell thứ hai) để xác nhận `Account Alpha` và `Account Beta` có 2 giá trị `zalo_user_id` khác nhau, không trùng Zalo chính.
      - **`L3-3`:** Gửi 1 tin nhắn văn bản và 1 tệp văn bản mẫu không chứa thông tin nhạy cảm (`sample-test.txt`) giữa chính `Account Alpha` và `Account Beta`; xác nhận tệp tải về nằm riêng trong `<ProfileDir>\Documents\Zalo Received Files`.
      - **`L3-4`:** Đóng êm `Account Beta` (`dim-host.ps1 ... stop <beta_id>`) rồi mở lại (`launch <beta_id>`); xác nhận `Account Beta` vào thẳng màn hình chat không cần quét lại QR.
   2. **Giai đoạn 2 (`P2` — Chỉ thực hiện nếu kiểm thử viên đồng ý kiểm tra thêm `L4-2` giữa chính 2 tài khoản của mình; nếu bỏ qua, nhập `NOT_TESTED`):**
      - Quan sát thụ động danh sách tiến trình con của `Account Alpha` và `Account Beta` ngay sau khi đăng nhập nghỉ (chưa bấm gọi): kiểm tra chỉ đọc bằng `Get-CimInstance Win32_Process -Filter "Name='ZaloCall.exe'" | Select-Object ProcessId, ParentProcessId` xem `Account Beta` đã spawn `ZaloCall.exe` ngay lúc đăng nhập hay chưa.
      - Thử bấm gọi thoại từ `Account Beta` sang chính `Account Alpha` (trên điện thoại/máy của kiểm thử viên), ghi nhận hiện tượng thực tế, cúp máy ngay sau 5 giây.
      - Thử bấm gọi thoại từ `Account Alpha` (hoặc Zalo chính nếu thuộc kiểm thử viên) sang tài khoản thử nghiệm còn lại để xác nhận cuộc gọi đi của phiên đầu có bị treo do ghi đè `O` hay vẫn đổ chuông bình thường.
      - Đóng êm `Account Beta`, khởi động lại duy nhất `Account Alpha` và xác nhận cuộc gọi hoạt động bình thường trở lại.
   3. Kết thúc script checklist để tự động dừng các phiên tạm và ghi kết quả đã khử định danh cá nhân (`Sanitize-OperatorNote`). Kiểm tra lại SHA-256 của `Zalo.exe` và `app.asar`.
7. **`Expected behavior`:**
   - **Giai đoạn 1 (`P0`):** `L3-1`, `L3-2`, `L3-3`, `L3-4` đều đạt `PASS`; `isolation_check_ok == true`; Zalo chính hoàn toàn không bị ảnh hưởng; SHA-256 nhị phân Zalo không đổi.
   - **Giai đoạn 2 (`P2`):** Xác định rõ ràng bằng chứng thực tế (`Confirmed by controlled experiment` bởi người thật) cho câu hỏi: `ZaloCall.exe` của phiên thứ 2 xuất hiện lúc đăng nhập hay lúc bấm gọi, và việc đóng êm các phiên phụ khác có khôi phục hoàn toàn cuộc gọi cho phiên được chọn hay không.
8. **`Failure conditions`:**
   - Bất kỳ lỗi đăng xuất chéo nào giữa `Account Alpha`, `Account Beta` và Zalo chính (`L3-1` / `L3-4` = `FAIL`).
   - Dữ liệu tệp hoặc SQLite ghi chéo sang `%APPDATA%\ZaloData` hoặc trùng `zalo_user_id` không bị chặn (`L3-2` / `L3-3` = `FAIL`).
   - Ghi chú kết quả trong JSON làm lộ tên người dùng hệ điều hành, họ tên cá nhân hoặc số điện thoại.
9. **`OS`:** Windows 10 / Windows 11 x64.
10. **`Bằng chứng cần lưu`:**
    - Tệp `evidence/l3-interactive-checklist-result.json` đã khử toàn bộ thông tin cá nhân (`<USER_HOME>`, `<USER>`, không chứa số điện thoại/UID thật — chỉ ghi nhận `distinct_uid_count: 2`), kèm kết quả từng mục `L3-1..L3-4`, `L4-1`, `L4-2` (`PASS`/`FAIL`/`BLOCKED`/`NOT_TESTED`) và bảng đối chiếu SHA-256 nhị phân trước/sau.
11. **`Người/agent chịu trách nhiệm`:** `Human QA opt-in` (kiểm thử viên con người thực hiện thủ công; `Windows agent` chỉ chuẩn bị công cụ hỗ trợ chỉ đọc, tuyệt đối không tự chạy tự động).

---

### `TC-H2` — Kiểm thử bộ lọc cửa sổ `EnumWindows` chống bắt nhầm Popup `notiWin` (`P0`) & Định tuyến click thông báo nội bộ (`P2`)

1. **`Test ID`:** `TC-H2` (Liên kết: `F-WIN-02`, `F-WIN-05`, `IR-WIN-01`, `IR-WIN-02` | Ưu tiên: **`P0` cho Phần 2A bộ lọc cửa sổ** / **`P2` cho Phần 2B UX thông báo**)
2. **`Hypothesis`:**
   - **Phần 2A (`P0` — Tự động):** Khi một phiên có đồng thời cửa sổ chính (`>= 800x600`, `GW_OWNER == 0`, không mang cờ `WS_EX_TOOLWINDOW`, tiêu đề `"Zalo"`, class chuẩn `Chrome_WidgetWin_1` hoặc class cửa sổ chính của fixture) và một cửa sổ thông báo nổi frameless nằm đè lên đầu danh sách Z-order (`352x82`, `WS_POPUP`, `WS_EX_TOPMOST | WS_EX_TOOLWINDOW`), hàm `find_main_window_for_pids` (`crates/platform-windows/src/win32_ffi.rs`) và `scripts/dim-host.ps1` sau khi cập nhật `IR-WIN-01`/`IR-WIN-02` sẽ **không bao giờ** chọn hoặc gửi `WM_CLOSE` nhầm vào cửa sổ popup `352x82`.
   - **Phần 2B (`P2` — Thủ công tùy chọn):** Ở chế độ thông báo nổi nội bộ (`notiWin`, `znotification.html`), mỗi tiến trình tự xử lý sự kiện click IPC mà không đi qua `AppUserModelId("com.vng.zalo")` của Windows Action Center, giúp click vào banner góc màn hình mở đúng cửa sổ tài khoản nhận tin.
3. **`Evidence hiện có`:**
   - `Confirmed by source code`: `crates/platform-windows/src/win32_ffi.rs:884-908` (`enum_windows_cb`) hiện lấy ngay cửa sổ `IsWindowVisible(hwnd) != 0` đầu tiên của mọi PID trong cây mà không kiểm tra `GetClassNameW`, `GetWindow(hwnd, GW_OWNER) == 0`, `WS_EX_TOOLWINDOW` hay kích thước tối thiểu; `adapter.rs:743-745` gửi thẳng `post_wm_close(hwnd)` vào handle này. Trong `app.asar/main-dist/main.js:2720673` gọi `app.setAppUserModelId("com.vng.zalo")` và tại `main.js:2726777` tạo `notiWin` (`352x82`, `alwaysOnTop: true`, `skipTaskbar: true`).
   - `Confirmed by static binary inspection`: REA `inspect_windows_shortcut` (`ev_69ddf74bf085b7e9d83f9f16f5781daf38ec90a7735065b49b55b6eee788316a`) xác nhận `Zalo.lnk` gắn `PKEY_AppUserModel_ID = "com.vng.zalo"` trỏ về Stub `Zalo.exe` gốc, không có `ToastActivatorCLSID`.
   - `Inference` / `Not tested`: Tỷ lệ định tuyến click thực tế của `notiWin` khi mở 2+ tài khoản thật trên Windows 11 là `Inference` từ mã nguồn, trạng thái thực nghiệm `Not tested`.
4. **`Preconditions`:**
   - Máy Windows 10/11 x64 có phiên desktop tương tác (`explorer.exe` / Win32 GUI session hoạt động để tạo HWND thực).
   - Toolchain Rust (`cargo test -p dim-platform-windows`) và PowerShell 5.1+/7+.
5. **`Test fixture`:**
   - **Phần 2A (Tự động):** Integration test Rust `crates/platform-windows/tests/tc_h2_window_filter.rs` (hoặc helper Win32 C#/PowerShell `Add-Type` chạy dưới tiến trình con tạm) đăng ký class `Chrome_WidgetWin_1` trong không gian class cục bộ của tiến trình test và tạo 2 cửa sổ thật thuộc cùng PID:
     1. `hwnd_main`: `WS_OVERLAPPEDWINDOW | WS_VISIBLE`, kích thước `960x680`, `hwndParent = NULL`, tiêu đề `"Zalo"`.
     2. `hwnd_popup`: `WS_POPUP | WS_VISIBLE`, extended style `WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE`, kích thước `352x82`, đặt `SetWindowPos(HWND_TOPMOST, ...)` để đảm bảo `EnumWindows` duyệt gặp `hwnd_popup` **trước** `hwnd_main`! Tạo thêm 1 cửa sổ thứ 3 kích thước `800x600` nhưng có `hwndParent = hwnd_main` (`GW_OWNER != 0`) và 1 cửa sổ tiêu đề rỗng `""` để kiểm tra biên.
   - **Phần 2B (Thủ công tùy chọn):** Bước `L4-1` trong `scripts/run-interactive-l3-checklist.ps1`.
6. **`Các bước xác minh an toàn`:**
   1. **Thực thi Phần 2A (`P0` — Tự động 100%, không cần Zalo thật):**
      - Chạy lệnh:
        ```powershell
        cargo test -p dim-platform-windows --test tc_h2_window_filter -- --nocapture
        ```
      - Bên trong bài test:
        a. Xác nhận `EnumWindows` thô nhìn thấy `hwnd_popup` (`352x82`, topmost) nằm trước `hwnd_main` (`960x680`) theo thứ tự Z-order.
        b. Gọi `find_main_window_for_pids(&[child_pid])`: kiểm tra hàm bỏ qua `hwnd_popup` (`WS_EX_TOOLWINDOW` & kích thước `< 400x400`), bỏ qua cửa sổ `GW_OWNER != 0`, bỏ qua cửa sổ tiêu đề rỗng, và trả về chính xác `(hwnd_main as u64, "Zalo")`.
        c. Gọi quy trình dừng cửa sổ (`post_wm_close` thông qua handle trả về): kiểm tra vòng lặp message `WM_CLOSE` đến đúng `hwnd_main`, trong khi bộ đếm `wm_close_received` của `hwnd_popup` bằng `0`.
      - Chạy đối chiếu tương đương trên nhánh PowerShell Signed-Host (`scripts/dim-host.ps1`) với cùng tiến trình tạo 2 cửa sổ để đảm bảo parity 100%.
   2. **Thực thi Phần 2B (`P2` — Tùy chọn thủ công trong đợt chạy `TC-H1B`):**
      - Thu nhỏ `Account Alpha` và `Account Beta` xuống nền; bật thông báo nổi trong phần Cài đặt của Zalo; gửi tin nhắn từ `Account Beta` sang `Account Alpha`, bấm trực tiếp vào banner `notiWin` góc dưới phải và xác nhận cửa sổ `Account Alpha` hiện lên trước.
7. **`Expected behavior`:**
   - Phần 2A: `selected_hwnd == hwnd_main`, `popup_selected == false`, `owned_dialog_selected == false`, `empty_title_selected == false`, `popup_wm_close_count == 0`, `main_wm_close_count == 1` trên cả Rust và `dim-host.ps1`.
   - Phần 2B (nếu chạy): Bấm vào `notiWin` mở đúng 100% cửa sổ của profile nhận tin.
8. **`Failure conditions`:**
   - `find_main_window_for_pids` trả về `hwnd_popup` (`352x82`), cửa sổ con có owner (`GW_OWNER != 0`), hoặc cửa sổ không có tiêu đề.
   - `stop()` gửi `WM_CLOSE` vào `hwnd_popup` khiến popup tắt nhưng tiến trình chính không nhận tín hiệu đóng cửa sổ.
9. **`OS`:** Windows 10 / Windows 11 x64.
10. **`Bằng chứng cần lưu`:**
    - Đầu ra `cargo test` mã thoát `0` và tệp JSON `evidence/tc-h2-window-filter-result.json` ghi rõ `z_order_first_hwnd`, `popup_hwnd`, `main_hwnd`, `returned_hwnd`, `popup_wm_close_count: 0`, `main_wm_close_count: 1`, `powershell_parity_ok: true`.
11. **`Người/agent chịu trách nhiệm`:** `Windows agent` (bắt buộc cho Phần 2A `P0`); `Human QA opt-in` (tùy chọn cho Phần 2B `P2`).

---

### `TC-H3` — Kiểm thử tự động giới hạn `104-byte` `sockaddr_un` trên Darwin & Hành vi `TMPDIR` vs `NSTemporaryDirectory()` dưới `open -n` (`P0`)

1. **`Test ID`:** `TC-H3` (Liên kết: `F-MAC-01`, `IR-MAC-01` | Ưu tiên: `P0`)
2. **`Hypothesis`:** Trên macOS 14+, dù một số đường dẫn nhân BSD hỗ trợ `SUN_LEN` linh hoạt ở tầng C thô, cả Node.js (`libuv` `uv_pipe_bind`) lẫn Electron/Chromium (`ProcessSingletonPosix` / `IPC::SocketPosix`) đều kiểm tra cứng độ dài đường dẫn Unix domain socket `< sizeof(sockaddr_un.sun_path)` = **`104` bytes** (`<= 103` ký tự ASCII). Khi `build_open_args` truyền `--env TMPDIR=<profile_dir>/tmp` với đường dẫn thực tế dài `~95` ký tự (`~/Library/Application Support/MultiZaloDesktop/Profiles/<slug>_<id>`), bất kỳ thành phần nào giải quyết thư mục tạm qua `getenv("TMPDIR")` để tạo `<TMPDIR>/org.chromium.Chromium.XXXXXX/SingletonSocket` (`~136–150` bytes) sẽ thất bại với `EINVAL`; ngược lại, bỏ ghi đè `TMPDIR` dài trong `build_open_args` (giữ nguyên `/var/folders/.../T/` mặc định của macOS từ `confstr(_CS_DARWIN_USER_TEMP_DIR)` dài `~49` ký tự -> tổng đường dẫn socket `~86` bytes `< 104`) giúp tạo `SingletonSocket`/`SingletonLock` thành công 100% mà vẫn cô lập hoàn toàn `<profile_dir>/Library/Application Support/ZaloData` và pre-seed đầy đủ `zmeta.json` + `migrate-config.json`.
3. **`Evidence hiện có`:**
   - `Confirmed by source code`:
     - `crates/platform-macos/src/layout.rs:97` hiện truyền cứng `--env TMPDIR=<profile_dir>/tmp`, và `layout.rs:34-51` (`ensure_macos_profile_layout`) chưa tạo `zmeta.json` / `migrate-config.json`.
     - Header Darwin `<sys/un.h>` định nghĩa `char sun_path[104]` (`sizeof(struct sockaddr_un) == 106`); `libuv` (`src/unix/pipe.c`: `uv_pipe_bind`) và Chromium (`chrome/browser/process_singleton_posix.cc` + `base/files/file_util_mac.mm` `GetTempDir()` với `MAC_CHROMIUM_TMPDIR` `< 50` ký tự) kiểm tra độ dài đường dẫn `< sizeof(saddr.sun_path)` (`104` bytes trên macOS).
   - `Inference` / `Not tested` (hiệu chỉnh từ báo cáo cũ): Mặc dù XNU BSD ở tầng syscall thô hỗ trợ `SUN_LEN`/`SOCK_MAXADDRLEN = 255` và Chromium `GetTempDir()` trên macOS ưu tiên `NSTemporaryDirectory()` / `confstr(_CS_DARWIN_USER_TEMP_DIR)` trừ khi đặt `MAC_CHROMIUM_TMPDIR`, cần thực nghiệm trực tiếp trên macOS thật để đo rõ hành vi dưới `/usr/bin/open -n --env CFFIXED_USER_HOME=...` cho cả Chromium lẫn Node.js (`os.tmpdir()` đọc `TMPDIR`), và xác nhận lỗi khởi chạy thực tế ở đường dẫn dài chưa được đo trên máy Mac vật lý (`Not tested`).
4. **`Preconditions`:**
   - Máy hoặc CI runner chạy macOS 14+ (`arm64` hoặc `x86_64`) có `/usr/bin/open`, `node` (`>= 20.x`), toolchain Rust (`cargo`), và tùy chọn nhị phân Electron trống (vanilla Electron binary hoặc minimal `.app` wrapper chạy `fixture-app.mjs` — **không dùng tài khoản Zalo thật**).
5. **`Test fixture`:**
   - Kịch bản tự động `tests/macos/run-tc-h3-socket-tmpdir.mjs` kèm một minimal unsigned/ad-hoc test bundle tạm `/tmp/DimSocketProbe.app` (chứa shell/Node executable đọc `process.env.CFFIXED_USER_HOME`, `process.env.TMPDIR`, `os.tmpdir()`, kết quả `getconf DARWIN_USER_TEMP_DIR`, và gọi `net.createServer().listen(socketPath)` mô phỏng đúng mẫu `<tempDir>/org.chromium.Chromium.Ab12Cd/SingletonSocket`) kết hợp `cargo test -p dim-platform-macos`.
6. **`Các bước xác minh an toàn`:**
   1. **Bước 1 — Đo chính xác ranh giới `104-byte` của `libuv`/Unix socket trên Darwin:**
      - Tạo các đường dẫn socket thử nghiệm có tổng độ dài byte ASCII lần lượt là `102`, `103`, `104`, `105`, và `142` bytes bên trong thư mục tạm `/tmp/dim-h3-...`.
      - Gọi `net.createServer().listen(p)` cho từng độ dài và ghi lại chính xác độ dài tối đa thành công (`expected_max_ascii_len == 103`) và mã lỗi ở `>= 104` bytes (`EINVAL` / `ENAMETOOLONG`).
   2. **Bước 2 — Đo hành vi `/usr/bin/open -n` với 3 cấu hình biến môi trường:**
      - Tạo thư mục gốc mô phỏng đúng độ dài đường dẫn thực tế (~95 ký tự): `<TempRoot>/Library/Application Support/MultiZaloDesktop/Profiles/cong-viec-chinh_a1b2c3d4`.
      - **Case A (Đường dẫn ngắn `< 30` ký tự):** `/tmp/dim-p1` + `--env TMPDIR=/tmp/dim-p1/tmp` -> xác nhận đường dẫn `<TMPDIR>/org.chromium.Chromium.XXXXXX/SingletonSocket` `< 104` bytes và `bind()` thành công.
      - **Case B (Đường dẫn thực tế dài `~95` ký tự + ghi đè `TMPDIR=<profile_dir>/tmp` như mã cũ):** Chạy `/usr/bin/open -n -a /tmp/DimSocketProbe.app --env CFFIXED_USER_HOME=<long_dir> --env TMPDIR=<long_dir>/tmp` -> ghi nhận cả giá trị `os.tmpdir()` lẫn giá trị `NSTemporaryDirectory()` (qua `osascript -l JavaScript -e 'ObjC.import("Foundation"); $.NSTemporaryDirectory().js'`), và xác nhận `bind()` trên `<TMPDIR>/org.chromium.Chromium.XXXXXX/SingletonSocket` (`> 135` bytes) thất bại.
      - **Case C (Cấu hình sau khi sửa `IR-MAC-01` — Không truyền `TMPDIR` dài):** Chạy `build_open_args` mới từ `crates/platform-macos` trên `<long_dir>` -> xác nhận `SingletonSocket` được đặt trong `/var/folders/.../T/` (hoặc đường dẫn ngắn `< 104` bytes), `bind()` thành công 100%, hai phiên `DimSocketProbe.app` mở song song cho 2 profile dài khác nhau đều sống độc lập, dữ liệu ghi vào `<long_dir>/Library/Application Support/ZaloData` không rò rỉ ra `$HOME` thật.
   3. **Bước 3 — Kiểm tra Pre-seed & Unit Test Rust:**
      - Chạy `cargo test -p dim-platform-macos`: xác nhận `ensure_macos_profile_layout` tạo đầy đủ `Library/Application Support/zmeta.json` (`{"failedMigrateCount":0}`), `Library/Application Support/ZaloData/migrate-config.json` (`{"zalopc_m_c":true}`), `.zalo-profile-id`, và `build_open_args` không bao giờ sinh ra biến `TMPDIR` làm đường dẫn `SingletonSocket` `>= 104` bytes.
   4. Dọn sạch `/tmp/DimSocketProbe.app` và thư mục tạm.
7. **`Expected behavior`:**
   - Đường dẫn socket `<= 103` bytes bind `PASS`, `>= 104` bytes thất bại `EINVAL` rõ ràng.
   - Với `IR-MAC-01` áp dụng (Case C), ngay cả khi `<profile_dir>` dài 95–120 ký tự, tổng đường dẫn `SingletonSocket` vẫn `< 104` bytes (`~86` bytes trong `/var/folders/...`), 2 phiên song song khởi chạy thành công 100%, và đủ 3 tệp marker (`zmeta.json`, `migrate-config.json`, `.zalo-profile-id`) hiện diện.
8. **`Failure conditions`:**
   - `build_open_args` vẫn xuất `--env TMPDIR=<profile_dir>/tmp` khiến tổng độ dài `<TMPDIR>/org.chromium.Chromium.XXXXXX/SingletonSocket >= 104` bytes.
   - Bất kỳ tệp nào được ghi nhầm vào `~/Library/Application Support/ZaloData` thật của máy Mac.
   - Thiếu `zmeta.json` hoặc `migrate-config.json` sau khi chạy `ensure_macos_profile_layout`.
9. **`OS`:** macOS 14+ (`arm64` Apple Silicon & `x86_64` Intel).
10. **`Bằng chứng cần lưu`:**
    - Tệp `evidence/tc-h3-macos-socket-result.json` chứa: `darwin_kernel_version`, `max_bound_socket_path_bytes`, kết quả `case_a_short`, `case_b_long_tmpdir_override` (kèm `getenv_tmpdir` vs `ns_temporary_directory`), `case_c_ir_mac_01_fixed`, `preseed_files_verified: true`, và kết quả `cargo test -p dim-platform-macos` (`exit code 0`).
11. **`Người/agent chịu trách nhiệm`:** `macOS agent`.

---

### `TC-H4` — Kiểm thử thứ tự đóng êm bảo vệ DB (`SIGTERM` trễ), Chống nhầm PID tái sử dụng (`bundleIdentifier`) & Kích hoạt AppKit không đòi Accessibility TCC trên macOS (`P0` / `P1`)

1. **`Test ID`:** `TC-H4` (Liên kết: `F-MAC-02`, `F-MAC-03`, `F-MAC-04`, `IR-MAC-02`, `IR-MAC-03` | Ưu tiên: **`P0` cho Phần 4A & 4B** / **`P1` cho Phần 4C & 4D**)
2. **`Hypothesis`:**
   - **Phần 4A (`P0` — Chống hỏng DB khi `stop`):** Khi một phiên đang giữ khóa ghi cơ sở dữ liệu trong `Database/_production/<uid>/test.db` và cần `1500ms` để flush WAL sau khi nhận yêu cầu thoát êm (`NSRunningApplication.terminate()` / `.quit-signal`), `MacOsPlatformAdapter::stop` (`crates/platform-macos/src/adapter.rs:358-405`) sau khi sửa theo `IR-MAC-02` sẽ chờ vòng lặp `timeout_ms` (`3500ms`) hoàn tất **trước** khi xét gửi `SIGTERM`, tuyệt đối không ngắt ngang quá trình flush DB ở mili-giây đầu tiên và không bao giờ bắn `SIGKILL` vào phiên đang mở DB tài khoản.
   - **Phần 4B (`P0` — Chống nhầm PID tái sử dụng):** Khi `SingletonLock` trỏ tới một PID đang sống nhưng thuộc về tiến trình khác không mang `bundleIdentifier == "com.vng.zalo"` (hoặc thực thi tương ứng), `inspect_profile` nhận diện chính xác trạng thái `Stopped (Stale Lock)` và `stop()` không bao giờ gửi tín hiệu tới tiến trình ngoại lai đó.
   - **Phần 4C (`P1` — Kích hoạt không cần Accessibility TCC):** `focus_or_restore` dùng `NSRunningApplication(processIdentifier:)?.activate(...)` đưa ứng dụng lên trước thành công mà không gọi `System Events` và không phát sinh lỗi thiếu quyền `kTCCServiceAccessibility`.
   - **Phần 4D (`P1` — Tùy chọn thủ công Keychain):** Symlink `Library/Keychains -> ~/Library/Keychains` cho phép giữ phiên qua các lần mở lại mà không hỏi mật khẩu lặp lại cho mục `"Zalo Safe Storage"`.
3. **`Evidence hiện có`:**
   - `Confirmed by source code`:
     - Trong `crates/platform-macos/src/adapter.rs:380-403`, dòng `391` gọi `libc::kill(raw_pid, libc::SIGTERM)` **ngay trước** vòng lặp chờ `while Instant::now() < deadline` ở dòng `397`.
     - Trong `adapter.rs:342-346`, `focus_or_restore` gọi `tell application "System Events" to set frontmost...` (yêu cầu Accessibility TCC).
     - Trong `adapter.rs:193`, `inspect_profile` chỉ kiểm tra `is_unix_pid_alive(pid)` (`kill(pid, 0)`) mà không đối chiếu `bundleIdentifier` hay đường dẫn thực thi.
     - Trong `app.asar/main-dist/main.js:3561081..3563611`, `SecureCipher` bọc khóa ngẫu nhiên 32-byte riêng `secure_ks_<hash>` trong từng `ZaloData` bằng khóa chung `"Zalo Safe Storage"` của `login.keychain-db` và kiểm tra chữ ký `checkAppSigned()`.
   - `Author claim` / `Not tested`: Tác giả `ducngfi` ghi nhận mở lại giữ nguyên phiên trên macOS 26.1; chưa kiểm thử runtime trên máy Mac tại workspace này (`Not tested`).
4. **`Preconditions`:**
   - Máy macOS 14+ (`arm64` / `x86_64`) không cấp quyền Accessibility TCC cho terminal/runner (`kTCCServiceAccessibility` tắt mặc định).
   - Có sẵn `cargo`, `node`, `/usr/bin/osascript`.
5. **`Test fixture`:**
   - Mở rộng chế độ thử nghiệm tổng hợp (wrapper `.app` tối giản `/tmp/DimLifecycleFixture.app` bọc `fixtures/test-desktop-app/fixture-app.mjs` hoặc script test riêng `tests/macos/slow-flush-fixture.mjs`):
     - Mở giữ descriptor tệp `<profile_dir>/Library/Application Support/ZaloData/Database/_production/100001/test.db` và `test.db-wal`.
     - Đăng ký bẫy `SIGTERM`: nếu nhận `SIGTERM` trong khi đang flush (`< 1500ms` kể từ lúc nhận lệnh đóng êm), ghi `"PREMATURE_SIGTERM_DURING_DB_FLUSH"` vào tệp trạng thái và thoát mã `99`! Nếu được chờ đủ `1500ms` thoát tự nhiên sau `.quit-signal` / `terminate()`, ghi `"WAL_CHECKPOINT_COMPLETE"`, xóa `-wal` và thoát mã `0`.
6. **`Các bước xác minh an toàn`:**
   1. **Thực thi Phần 4A (`P0` — Kiểm tra trình tự `stop()` và bảo toàn DB):**
      - Tạo profile thử nghiệm trong thư mục tạm `--data-dir /tmp/dim-tc-h4-...`, khởi chạy `DimLifecycleFixture.app` ở chế độ giữ khóa `test.db` `1500ms` khi đóng.
      - Gọi `adapter.stop(&state, &[], 3_500)` và đo `elapsed_ms`.
      - Xác nhận: `1450 <= elapsed_ms < 3500`, tiến trình thoát với mã `0`, tệp `test.db` chứa `"WAL_CHECKPOINT_COMPLETE"`, `test.db-wal` đã được dọn sạch, và cờ `premature_sigterm_received == false`.
   2. **Thực thi Phần 4B (`P0` — Kiểm tra chống nhầm PID tái sử dụng):**
      - Spawn một tiến trình nền vô hại của chính bài test (`sleep 60`, lưu `foreign_pid`).
      - Tạo symlink giả `<zalo_data>/SingletonLock -> localhost-<foreign_pid>` trên một profile đang tắt.
      - Gọi `adapter.inspect_profile(...)` và `adapter.stop(...)`: xác nhận `state.running == false`, `state.stale_lock_detected == true`, và `kill(foreign_pid, 0)` xác nhận tiến trình `sleep 60` **vẫn sống nguyên vẹn** (sau đó bài test tự dọn tiến trình `sleep 60` của mình).
   3. **Thực thi Phần 4C (`P1` — Kiểm tra kích hoạt cửa sổ không đòi Accessibility TCC):**
      - Khi `DimLifecycleFixture.app` đang chạy nền (ẩn sau một cửa sổ khác), gọi `adapter.focus_or_restore(...)`.
      - Kiểm tra mã trả về `Ok(())`, stderr của `osascript`/AppKit không chứa lỗi `1002` (`not allowed assistive access`), và `NSRunningApplication.isActive` chuyển thành `true`.
   4. **Thực thi Phần 4D (`P1` — Tùy chọn thủ công `Human QA opt-in` trên máy Mac có tài khoản thử nghiệm của tester):**
      - Mở 2 profile cô lập có symlink `Library/Keychains`, đăng nhập thủ công 2 tài khoản của tester, đóng êm cả 2, mở lại đồng thời cả 2 và đếm số lần xuất hiện hộp thoại hỏi mật khẩu Keychain (ghi `NOT_TESTED` nếu không thực hiện kiểm thử thủ công).
7. **`Expected behavior`:**
   - Phần 4A: `premature_sigterm_received == false`, `wal_checkpoint_complete == true`, `sigkill_sent == false`.
   - Phần 4B: `foreign_pid_untouched == true`, `stale_lock_detected == true`.
   - Phần 4C: Kích hoạt thành công `is_active == true` với `accessibility_tcc_prompt_or_error == false`.
   - Phần 4D (nếu chạy): `0` hộp thoại Keychain lặp lại ở các lần mở lại tiếp theo.
8. **`Failure conditions`:**
   - `SIGTERM` được gửi trước khi hết thời gian chờ đóng êm hoặc khi tệp trong `Database/_production/` vẫn đang mở flush.
   - `inspect_profile` báo `running == true` hoặc `stop()` bắn tín hiệu vào `foreign_pid` không mang định danh ứng dụng mục tiêu.
   - `focus_or_restore` gọi `System Events` hoặc thất bại do thiếu quyền Accessibility TCC.
9. **`OS`:** macOS 14+ (`arm64` / `x86_64`).
10. **`Bằng chứng cần lưu`:**
    - Tệp `evidence/tc-h4-macos-lifecycle-result.json` gồm: `stop_elapsed_ms`, `premature_sigterm_received: false`, `wal_checkpoint_complete: true`, `foreign_pid_guard_passed: true`, `appkit_activate_without_accessibility_passed: true`, và trường `manual_keychain_check: "PASS" | "NOT_TESTED"`.
11. **`Người/agent chịu trách nhiệm`:** `macOS agent` (cho Phần 4A, 4B, 4C tự động); `Human QA opt-in` (cho Phần 4D tùy chọn).

---

### `TC-H5` — Kiểm thử tự động cơ chế chuyển quyền Socket Gọi `fs.unlink` (Last-Writer-Wins) & Chẩn đoán chủ sở hữu Socket trên macOS (`P2`)

1. **`Test ID`:** `TC-H5` (Liên kết: `F-MAC-05`, `F-MAC-04`, `IR-MAC-04` | Ưu tiên: `P2`)
2. **`Hypothesis`:** Trong nhánh Darwin (`main.js:314800`), khi gặp `EADDRINUSE` trên cặp Unix domain socket cố định, tiến trình khởi động sau gọi `fs.unlink` xóa inode socket cũ và `listen()` tạo inode mới (last-writer-wins). Do đó, bất kỳ phiên nào được khởi động lại sau cùng sẽ nắm trọn kênh IPC gọi điện sạch sẽ, và hàm `inspect_call_sockets_at` mở rộng (`IR-MAC-04`) có thể xác định chính xác inode / PID đang sở hữu socket để hiển thị trên `diagnostics` mà không bao giờ cần nhân bản hay ký lại `Zalo.app`.
3. **`Evidence hiện có`:**
   - `Confirmed by source code`: `app.asar/main-dist/main.js:311828,314800` gán cứng `"/tmp/socketzalorecv2021"` & `"/tmp/socketzalosend2021"` và gọi `fs.unlink(v, ...)` khi `EADDRINUSE`; `crates/platform-macos/src/layout.rs:138-149` có sẵn hàm `inspect_call_sockets_at(paths: &[&Path])`.
   - `Author claim` / `Not tested`: `ducngfi` (`README.md:189,199`) ghi nhận phiên thứ 2 chiếm socket gọi của phiên thứ 1 trên macOS 26.1; chưa chạy kiểm thử tự động xác nhận chuyển giao qua lại và truy vấn PID chủ sở hữu trong repo này (`Not tested`).
4. **`Preconditions`:**
   - Máy chạy macOS 14+ (có `lsof`, `node`, `cargo`).
   - **Tuyệt đối không chạm** `/tmp/socketzalorecv2021` hoặc `/tmp/socketzalosend2021` thật trên máy; chỉ truyền đường dẫn socket thử nghiệm riêng `/tmp/dim-test-zcall-recv-<run_id>` và `/tmp/dim-test-zcall-send-<run_id>` vào harness và vào tham số `paths` của `inspect_call_sockets_at`.
5. **`Test fixture`:**
   - Harness Node.js `tests/macos/run-tc-h5-unix-call-handoff.mjs` mô phỏng chính xác chuỗi `listen() -> on('error', EADDRINUSE => fs.unlink -> listen())` của nhánh Darwin kèm helper client giả lập gửi gói `"SYNTHETIC_CALL_PING"`, phối hợp với integration test trong `crates/platform-macos`.
6. **`Các bước xác minh an toàn`:**
   1. Tạo `run_id` ngẫu nhiên và đặt `recvSock = /tmp/dim-test-zcall-recv-<run_id>`, `sendSock = /tmp/dim-test-zcall-send-<run_id>`.
   2. Khởi chạy **Instance A**: xác nhận cả 2 socket được tạo trên đĩa, ghi nhận `(inode_a, pid_a)`. Gọi hàm chẩn đoán `inspect_call_sockets_at(&[recvSock, sendSock])` (phiên bản mở rộng của `IR-MAC-04`): xác nhận báo cáo đúng `owner_pid == pid_a`.
   3. Khởi chạy **Instance B** cùng cấu hình đường dẫn socket thử nghiệm: xác nhận Instance B gặp `EADDRINUSE`, gọi `fs.unlink`, tạo lại socket với `inode_b != inode_a`. Gọi lại hàm chẩn đoán: xác nhận `owner_pid == pid_b`; gửi gói `"SYNTHETIC_CALL_PING"` tới đường dẫn socket -> xác nhận **Instance B** nhận được 100%, đồng thời Instance A vẫn sống ổn định cho tác vụ nền (không crash).
   4. Mô phỏng thao tác *"Chuyển quyền Gọi về Instance A"*: đóng êm Instance A rồi mở lại **Instance A'** sau cùng -> xác nhận `owner_pid == pid_a_prime` và gói `"SYNTHETIC_CALL_PING"` định tuyến sạch về Instance A'.
   5. Đóng cả 2 instance và xóa sạch 2 tệp socket `/tmp/dim-test-zcall-*`.
7. **`Expected behavior`:**
   - Quyền sở hữu socket chuyển giao tất định (`pid_a -> pid_b -> pid_a_prime`) cho phiên khởi tạo sau cùng; `inspect_call_sockets_at` phản ánh chính xác chủ sở hữu ở cả 3 bước; không tiến trình nào bị crash ngoài ý muốn.
8. **`Failure conditions`:**
   - Hàm chẩn đoán báo sai PID chủ sở hữu sau khi `fs.unlink` thay đổi inode.
   - Bài test đụng vào đường dẫn `/tmp/socketzalo*2021` của Zalo thật hoặc để sót tệp socket trong `/tmp`.
9. **`OS`:** macOS 14+ (`arm64` / `x86_64`).
10. **`Bằng chứng cần lưu`:**
    - Tệp `evidence/tc-h5-macos-call-socket-handoff.json` chứa chuỗi chuyển đổi `[{step: 1, inode, owner_pid}, {step: 2, inode, owner_pid}, {step: 3, inode, owner_pid}]`, kết quả `ping_routed_to_latest_owner: true`, và kết quả `cargo test -p dim-platform-macos` mã thoát `0`.
11. **`Người/agent chịu trách nhiệm`:** `macOS agent`.

---

### `TC-H6` — Kiểm thử tự động đánh thức cửa sổ ẩn ở System Tray vượt Win32 Foreground Lock qua Handoff `second-instance` (`P1`)

1. **`Test ID`:** `TC-H6` (Liên kết: `F-WIN-03`, `IR-WIN-01` | Ưu tiên: `P1`)
2. **`Hypothesis`:** Trên Windows 10/11, khi cửa sổ chính của một profile bị ẩn xuống System Tray (`IsWindowVisible(hwnd) == 0`, khiến `find_main_window_for_pids` trả `None`) hoặc khi tiến trình gọi lệnh `focus` chạy ở nền (như `ui-server.mjs`) bị Win32 Foreground Lock chặn `SetForegroundWindow`, việc `focus_or_restore` (`crates/platform-windows/src/adapter.rs:642-665` & `scripts/dim-host.ps1`) kích hoạt tiến trình handoff thứ hai trong cùng khối biến môi trường cô lập của profile đó sẽ đánh thức chính tiến trình chủ đang chạy tự gọi chuỗi khôi phục cửa sổ (`ShowWindow` + `setAlwaysOnTop(true)` -> `SetForegroundWindow` -> `setAlwaysOnTop(false)`) và đưa đúng cửa sổ của profile đó lên tiền cảnh (`GetForegroundWindow() == target_hwnd`) trong thời gian `< 2000ms`, không cần `AttachThreadInput` hay giả lập phím `SendInput`.
3. **`Evidence hiện có`:**
   - `Confirmed by source code`:
     - `app.asar/main-dist/main.js:3565794` xử lý sự kiện `second-instance` bằng cách hiện lại cửa sổ ẩn ở Tray và gọi `setAlwaysOnTop(true)` -> `focus()` -> `setAlwaysOnTop(false)`.
     - `crates/platform-windows/src/adapter.rs:651-664` hiện gọi thẳng `win32_ffi::focus_window(hwnd)` và `return Ok(())` ngay cả khi `SetForegroundWindow(hwnd)` bị Windows từ chối âm thầm do Foreground Lock, chỉ chạy nhánh spawn handoff khi `state.window_handle.is_none()`.
   - `Confirmed by controlled experiment` (một phần): `focus_count` qua tệp `.focus-signal` trên fixture không giao diện đã đạt `PASS` (`Test-2` trong `run-fixture-tests.mjs`), nhưng kịch bản cửa sổ Win32 thật ẩn dưới Tray (`IsWindowVisible == 0`) bị đè bởi cửa sổ tiền cảnh khác và kiểm tra `GetForegroundWindow() == target_hwnd` đang ở mức `Inference` / `Not tested`.
4. **`Preconditions`:**
   - Máy Windows 10/11 x64 có desktop tương tác đang hoạt động (không khóa màn hình `Win+L` trong vài giây chạy test để `GetForegroundWindow()` trả về HWND hợp lệ).
   - Có sẵn `cargo`, `powershell.exe` và `node.exe`.
5. **`Test fixture`:**
   - Helper GUI kiểm thử nhẹ trong `crates/platform-windows/tests/tc_h6_tray_foreground_handoff.rs` (spawn 2 phiên GUI fixture cho Profile A và Profile B + 1 cửa sổ `"Blocker Foreground Window"` độc lập đang giữ tiêu điểm bàn phím):
     - Khi chạy lần đầu cho một `<ProfileDir>`, GUI fixture giữ khóa đơn phiên riêng trong `<ProfileDir>\AppData\Roaming\ZaloData\lockfile`, tạo cửa sổ chính `900x650`, và hỗ trợ lệnh ẩn xuống khay (`ShowWindow(hwnd, SW_HIDE)` -> `IsWindowVisible == 0`).
     - Khi một tiến trình thứ hai khởi chạy trong cùng khối biến môi trường `<ProfileDir>` (handoff `second-instance`), nó gửi thông điệp IPC cục bộ báo cho tiến trình chủ của chính profile đó gọi `ShowWindow(SW_RESTORE)` + `SetWindowPos(HWND_TOPMOST)` + `SetForegroundWindow` + bỏ `HWND_TOPMOST`, rồi tiến trình thứ hai thoát mã `2` (giống hệt hành vi Electron `requestSingleInstanceLock()` + `main.js:3565794`).
6. **`Các bước xác minh an toàn`:**
   1. Khởi tạo `--data-dir` tạm, tạo `Profile A` và `Profile B`, khởi chạy cả hai GUI fixture.
   2. **Trường hợp 1 — Cửa sổ ẩn hoàn toàn ở System Tray (`IsWindowVisible == 0`):**
      - Ẩn cả cửa sổ `Profile A` và `Profile B` (`ShowWindow(SW_HIDE)`), xác nhận `IsWindowVisible(hwnd_a) == 0` và `find_main_window_for_pids(&[pid_a]) == None`.
      - Đưa cửa sổ độc lập `"Blocker Foreground Window"` lên tiền cảnh (`GetForegroundWindow() == hwnd_blocker`).
      - Từ một tiến trình nền tách biệt (không sở hữu cửa sổ tiền cảnh, mô phỏng `ui-server.mjs`), gọi `focus` cho `Profile A` và bấm giờ `latency_ms`.
      - Xác nhận trong `< 2000ms`: `IsWindowVisible(hwnd_a) != 0`, `GetForegroundWindow() == hwnd_a`, trong khi `IsWindowVisible(hwnd_b) == 0` (`Profile B` vẫn nằm yên ở Tray!).
   3. **Trường hợp 2 — Cửa sổ đang hiện nhưng nằm chìm sau `"Blocker Foreground Window"` dưới Foreground Lock:**
      - Đưa `"Blocker Foreground Window"` đè lên trên `hwnd_a` (`IsWindowVisible(hwnd_a) != 0` nhưng `GetForegroundWindow() == hwnd_blocker`).
      - Gọi `focus_or_restore` từ tiến trình nền cho `Profile A`: xác nhận nếu `focus_window(hwnd_a)` bị Foreground Lock chặn (`GetForegroundWindow() != hwnd_a`), adapter tự động fallback sang handoff `second-instance` và đưa `GetForegroundWindow() == hwnd_a` thành công trong `< 2000ms`.
   4. Kiểm tra mã nguồn `crates/platform-windows/src/win32_ffi.rs` đảm bảo **không** sử dụng `AttachThreadInput` hay `SendInput`. Dừng sạch mọi tiến trình test.
7. **`Expected behavior`:**
   - Cả Trường hợp 1 (ẩn dưới Tray) và Trường hợp 2 (bị chặn Foreground Lock khi gọi từ tiến trình nền) đều đưa đúng `hwnd_a` lên làm `GetForegroundWindow()` trong `< 2000ms` (100% qua 5 lần lặp liên tiếp), không đánh thức nhầm `Profile B`.
8. **`Failure conditions`:**
   - `IsWindowVisible(hwnd_a)` vẫn bằng `0` hoặc `GetForegroundWindow()` vẫn là `hwnd_blocker` (chỉ nhấp nháy Taskbar mà không lên tiền cảnh) sau `2000ms`.
   - Độ trễ vượt quá `2000ms`, làm hiện nhầm `Profile B`, hoặc dùng `AttachThreadInput` / `SendInput`.
9. **`OS`:** Windows 10 / Windows 11 x64.
10. **`Bằng chứng cần lưu`:**
    - Tệp `evidence/tc-h6-tray-focus-result.json` ghi nhận 5 vòng đo `tray_restore_latency_ms` (`min`, `max`, `p95 < 2000`), `foreground_lock_fallback_triggered: true`, `foreground_hwnd_matched: true`, `other_profile_remained_hidden: true`, `forbidden_apis_absent: true`.
11. **`Người/agent chịu trách nhiệm`:** `Windows agent`.

---

### `TC-H7` — Kiểm thử tự động An toàn Xóa NTFS Junction (`Downloads`/`Desktop`) & Chốt chặn `ExternalStorageGuard` (`db_path.location`) (`P0` / `P1`)

1. **`Test ID`:** `TC-H7` (Liên kết: `F-WIN-04`, `F-CORE-01`, `F-CORE-02`, `IR-WIN-03`, `IR-WIN-04` | Ưu tiên: **`P0` cho an toàn xóa Junction & `ExternalStorageGuard`** / **`P1` cho bố cục thư mục hộp thoại tệp**)
2. **`Hypothesis`:**
   - **Phần 7A (`P0`/`P1` — NTFS Junction chọn lọc & Bảo toàn 100% tệp Canary của Host):** Khi `ensure_windows_profile_layout` tạo đủ `Downloads`, `Pictures`, `Videos`, `Music`, `Desktop`, `Documents\Zalo Received Files` và người dùng bật tùy chọn tạo NTFS junction cho `<ProfileDir>\Downloads` & `<ProfileDir>\Desktop` trỏ sang thư mục host, thư mục `<ProfileDir>\Documents\Zalo Received Files` vẫn tách biệt 100% theo từng profile, gói `backup` không đi xuyên qua junction, và lệnh `delete --confirm` (`unlink_reparse_points_safe`) gỡ sạch reparse point trước khi xóa đệ quy — giữ nguyên vẹn 100% tệp canary trong thư mục host.
   - **Phần 7B (`P0` — `ExternalStorageGuard` chống hỏng DB chung & chống Backup rỗng):** Khi `config.json` của một profile có `db_path.location` hoặc `resource.location.path` trỏ ra ngoài `<ProfileDir>`: (1) nếu trùng đường dẫn với một profile đang chạy khác hoặc trùng với Zalo chính, lệnh `launch` bị chặn cứng với mã lỗi `SHARED_EXTERNAL_DB_PATH`; (2) khi gọi `backup` mặc định không kèm cờ xác nhận tường minh, lệnh bị chặn với mã lỗi `EXTERNAL_DB_OUTSIDE_PROFILE` để người dùng không bao giờ tin nhầm vào một bản sao lưu thiếu toàn bộ tin nhắn; và (3) lệnh kiểm tra sẵn sàng cập nhật (`prepare-update` / kiểm tra phiên đang mở cho `F-CORE-02`) báo cáo chính xác các phiên phụ đang khóa nhị phân để đóng êm trước khi cập nhật.
3. **`Evidence hiện có`:**
   - `Confirmed by source code`:
     - `crates/platform-windows/src/layout.rs:14-29` hiện chỉ tạo `Desktop` và `Documents\Zalo Received Files`, thiếu `Downloads`, `Pictures`, `Videos`, `Music`.
     - `crates/profile-store/src/store.rs:377-427` đã cài đặt `unlink_reparse_points_safe` và `remove_dir_without_following_links` (`Confirmed by controlled experiment` với 1 junction đơn giản trong `Test-8` của `run-fixture-tests.mjs`).
     - `crates/core/src/zalo_identity.rs:35-49` đã đọc `config.json -> db_path.location` để tìm UID, nhưng `crates/process-manager/src/guard.rs` và `crates/profile-store/src/backup.rs:71-165` **chưa kiểm tra xung đột `db_path.location` / `resource.location.path` khi `launch` và chưa cảnh báo/chặn khi `backup`**.
4. **`Preconditions`:**
   - Máy Windows 10/11 x64 (cho phần NTFS junction `mklink /J` và parity PowerShell `dim-host.ps1`); phần unit test `ExternalStorageGuard` chạy đa nền tảng (`cargo test --workspace`).
   - Toàn bộ thư mục `"mock-host"` được tạo bên trong `$env:TEMP\dim-tc-h7-...`, tuyệt đối không tạo junction trỏ vào thư mục `Downloads`/`Desktop` thật của người dùng trên máy chạy test!
5. **`Test fixture`:**
   - Script kiểm thử tự động `tests/run-tc-h7-storage-safety.mjs` chạy song song trên cả Track A (`scripts/dim-host.ps1`) và Track B (`dim-cli.exe`), kết hợp unit test Rust trong `crates/core`, `crates/process-manager`, `crates/profile-store` và `crates/platform-windows`.
6. **`Các bước xác minh an toàn`:**
   1. Tạo `$TempRoot` cô lập, khởi tạo `<TempRoot>\mock-host\Downloads` và `<TempRoot>\mock-host\Desktop` chứa 3 tệp canary (`canary-1.pdf`, `canary-2.png`, `sub\canary-3.txt`), tính và lưu bảng mã băm SHA-256 ban đầu của cả 3 tệp.
   2. **Kiểm tra Bố cục & Cô lập Tệp nhận (`IR-WIN-03`):**
      - Tạo `Profile A` và `Profile B`: xác nhận mặc định cả hai đều có đủ thư mục thực `Downloads`, `Pictures`, `Videos`, `Music`, `Desktop`, `Documents\Zalo Received Files` (không phải reparse point khi chưa bật cờ opt-in).
      - Bật liên kết junction opt-in cho `Profile A` (`Downloads` -> `mock-host\Downloads`, `Desktop` -> `mock-host\Desktop`). Ghi tệp `auto-recv-a.pdf` vào `<ProfileA>\Documents\Zalo Received Files`: xác nhận tệp này **không** xuất hiện trong `Profile B` hay `mock-host`, và `fs::symlink_metadata` xác nhận `Documents\Zalo Received Files` là thư mục thường (`is_symlink_or_reparse == false`).
   3. **Kiểm tra Backup & Delete an toàn với NTFS Junction (`P0`):**
      - Chạy `backup <ProfileA>`: kiểm tra `backup.json` chứa `auto-recv-a.pdf` nhưng **không** chứa `canary-1.pdf..canary-3.txt` (không đi xuyên qua junction).
      - Chạy `delete <ProfileA> --confirm`: xác nhận thư mục `<ProfileA>` đã bị xóa hoàn toàn, `unlinked_junctions >= 2`, và cả 3 tệp canary trong `mock-host\Downloads` & `mock-host\Desktop` vẫn tồn tại với **SHA-256 trùng khớp 100%**.
   4. **Kiểm tra `ExternalStorageGuard` trên `launch` & `backup` (`IR-WIN-04`, `P0`):**
      - Tạo thư mục `<TempRoot>\ext-db-shared\Database\_production\8888888`.
      - Tạo `Profile C` và `Profile D`, ghi tệp `AppData\Roaming\ZaloData\config.json` của cả hai có `"db_path": { "location": "<TempRoot>\\ext-db-shared" }` và `"resource": { "location": { "path": "<TempRoot>\\ext-media-shared" } }`.
      - Chạy `launch <ProfileC>` với `--node-fixture` -> `ok == true`.
      - Trong khi `Profile C` đang chạy, gọi `launch <ProfileD>` -> bắt buộc bị từ chối với `exit code != 0` và `error_code == "SHARED_EXTERNAL_DB_PATH"` trên cả `dim-cli` và `dim-host.ps1`. Kiểm tra tương tự khi `db_path.location` của `Profile D` trùng với cấu hình giả lập của primary Zalo.
      - Gọi `stop <ProfileC>`. Sau khi dừng, gọi `backup <ProfileC>` mặc định -> bắt buộc bị từ chối với `error_code == "EXTERNAL_DB_OUTSIDE_PROFILE"`; gọi lại với cờ xác nhận tường minh (ví dụ `--allow-external-db-omit`) -> thành công kèm trường cảnh báo `external_db_omitted: true` trong `backup.json`.
7. **`Expected behavior`:**
   - Đủ 6 thư mục người dùng chuẩn được tạo sẵn trong profile Windows.
   - 100% tệp canary trong `mock-host` giữ nguyên SHA-256 sau khi `backup` và `delete --confirm` profile có gắn junction.
   - Cả Rust CLI và PowerShell host đều chặn đồng nhất `SHARED_EXTERNAL_DB_PATH` khi `launch` trùng DB ngoài và `EXTERNAL_DB_OUTSIDE_PROFILE` khi `backup` mặc định.
8. **`Failure conditions`:**
   - Mất hoặc thay đổi bất kỳ byte nào của tệp canary trong `mock-host` khi xóa profile.
   - Cho phép 2 profile đang chạy mở chung một `db_path.location` hoặc `resource.location.path` bên ngoài.
   - Cho phép `backup` âm thầm thành công mà không báo lỗi/cảnh báo khi `db_path.location` nằm ngoài `<ProfileDir>`.
   - Bất đồng mã lỗi giữa `dim-cli` (Rust) và `scripts/dim-host.ps1` (PowerShell).
9. **`OS`:** Windows 10 / Windows 11 x64 (toàn bộ suite) & macOS/Linux (cho phần unit test Rust `ExternalStorageGuard` + symlink delete safety).
10. **`Bằng chứng cần lưu`:**
    - Tệp `evidence/tc-h7-storage-safety-result.json` chứa: bảng `canary_hashes_before` vs `canary_hashes_after` (khớp 100%), `unlinked_junctions`, kết quả parity `rust_vs_powershell_error_codes` (`SHARED_EXTERNAL_DB_PATH`, `EXTERNAL_DB_OUTSIDE_PROFILE`), và kết quả `cargo test --workspace` mã thoát `0`.
11. **`Người/agent chịu trách nhiệm`:** `Windows agent` (phối hợp cập nhật `crates/core`, `crates/process-manager`, `crates/profile-store`, `crates/platform-windows` và `scripts/dim-host.ps1` theo ranh giới sở hữu trong `IR-WIN-03` & `IR-WIN-04`).

---

### `TC-H8` — Bộ kiểm thử tự động 3 phần trên Ubuntu: Vòng đời `bwrap` & Runtime Binds (8A), Preflight AppArmor Trung thực không `--no-sandbox` (8B) & Cô lập cổng gọi `29631/29632` bằng Rootless `pasta` (8C) (`P0` / `P1` / `P2`)

1. **`Test ID`:** `TC-H8` — gồm 3 bài con bắt buộc `TC-H8A` (`P0`/`P1`, nghiệm thu `IR-UBU-01`, `IR-UBU-02`, `IR-UBU-03`), `TC-H8B` (`P0`, nghiệm thu `IR-UBU-04`), `TC-H8C` (`P2`, nghiệm thu `IR-UBU-05`) (Liên kết đầy đủ: `F-UBU-01`, `F-UBU-02`, `F-UBU-03`, `F-UBU-04`, `F-UBU-05`, `F-UBU-06`, `F-UBU-07`)
2. **`Hypothesis`:** Sau khi phân tách rạch ròi 4 nguồn giới hạn trên Ubuntu (Bản chính thức VNG `UNSUPPORTED`, Lỗi của `dim-platform-linux` adapter, Cổng cứng `29631/29632` của bản cộng đồng, và Ràng buộc chung network namespace vs `pasta` riêng):
   - **`TC-H8A` (`P0`/`P1` — `F-UBU-01`, `F-UBU-02`, `F-UBU-03`, `F-UBU-05`):** Gỡ bỏ `--die-with-parent`, bổ sung bind có kiểm tra tồn tại cho `$XDG_RUNTIME_DIR` (Wayland, PipeWire, PulseAudio, D-Bus `bus`), `/tmp/.X11-unix`, `/etc/fonts`, `/etc/machine-id`, `/dev/dri`, tạo `.config/user-dirs.dirs` + `Downloads`/`Documents`/`Pictures`, cài đặt `focus_or_restore` qua `second-instance`, và bỏ `SIGKILL` mù khi đang đóng DB giúp `fixture-app.mjs` sống độc lập sau khi `dim-cli` thoát, truy cập đủ tài nguyên desktop session, nhận tín hiệu `focus`, và dừng êm an toàn.
   - **`TC-H8B` (`P0` — `F-UBU-04`):** Trên Ubuntu 23.10+/24.04 có `kernel.apparmor_restrict_unprivileged_userns = 1`, bộ kiểm tra preflight phát hiện chính xác `.AppImage` chưa có AppArmor profile và trả về trạng thái `BLOCKED` trung thực kèm mẫu hướng dẫn quản trị `/etc/apparmor.d/`, **tuyệt đối không tự chèn `--no-sandbox` hay `ELECTRON_DISABLE_SANDBOX=1`**.
   - **`TC-H8C` (`P2` — `F-UBU-07`):** Hai tiến trình chạy trong 2 không gian mạng rootless `pasta` riêng biệt có giao diện loopback `127.0.0.1` cô lập nên cả hai cùng `bind()` thành công các cổng cố định `127.0.0.1:29631` và `127.0.0.1:29632` đồng thời mà không gặp `EADDRINUSE`, trong khi vẫn dùng chung được socket Unix âm thanh PipeWire/PulseAudio được bind vào từng phiên.
3. **`Evidence hiện có`:**
   - `Confirmed by source code`:
     - Nguồn (D) — Zalo VNG chính thức: `main.js:314392` và `native/nativelibs/file-utilities/index.js:36` ném lỗi trực tiếp trên `linux` (`Unsupported`).
     - Nguồn (B) — Lỗi Adapter hiện tại: `crates/platform-linux/src/layout.rs:84` truyền `--die-with-parent`; `layout.rs:22-131` thiếu tạo `Downloads`/`Documents`/`.config/user-dirs.dirs` và thiếu bind `$XDG_RUNTIME_DIR`, `/tmp/.X11-unix`, `/etc/fonts`, `/etc/machine-id`, `/dev/dri`; `adapter.rs:310-318` để `focus_or_restore` rỗng `Ok(())`; `adapter.rs:357` bắn `libc::SIGKILL` vô điều kiện sau `timeout_ms`.
     - Nguồn (A) — Bản cộng đồng `VN-Linux-Family/zalo-for-linux` (`bbb6f99e...`): `scripts/patches/patch-zcall-native.js:307-318` gán cứng cổng TCP `127.0.0.1:29631` & `29632`; `patch-notification-badge.js:859-886` ghi đè chung `zalo.desktop`.
   - `Inference` / `Not tested`: Khả năng 2 phiên cùng lắng nghe `127.0.0.1:29631/29632` đồng thời nhờ rootless `pasta` kết hợp chia sẻ socket PipeWire là `Inference` từ cơ chế Linux network/mount namespaces, chưa kiểm chứng thực nghiệm trong repo (`Not tested`).
4. **`Preconditions`:**
   - Máy hoặc container/VM chạy Ubuntu 22.04 LTS và Ubuntu 24.04 LTS x86_64 có cài đặt: `cargo`, `node` (`>= 20.x`), `bubblewrap` (`bwrap`), `dbus-run-session` (hoặc desktop session đang chạy), và gói `passt` (cung cấp lệnh `pasta` không cần root cho phần `8C`).
   - Không chạy với quyền `root`, không thay đổi `sysctl` của host, không dùng bất kỳ tệp `.AppImage` chứa `app.asar` độc quyền nào của VNG.
5. **`Test fixture`:**
   - `fixtures/test-desktop-app/fixture-app.mjs` kết hợp script kiểm thử `tests/linux/run-tc-h8-ubuntu-suite.mjs`:
     - Tạo trước trong `$XDG_RUNTIME_DIR` giả lập của bài test các Unix domain socket dummy (`wayland-0`, `pipewire-0`, `pulse/native`, `bus`) để kiểm tra khả năng kết nối từ bên trong `bwrap` ngay cả trên headless CI runner.
     - Tạo một tệp ELF/script giả lập mang phần mở rộng `.AppImage` (`/tmp/dim-h8-dummy.AppImage`, không chứa mã VNG) để kiểm tra preflight AppArmor trong `8B`.
     - Probe Node.js 20 dòng lắng nghe đồng thời `127.0.0.1:29631` và `127.0.0.1:29632` + kết nối thử vào socket Unix `pulse/native` cho phần `8C`.
6. **`Các bước xác minh an toàn`:**
   1. **Thực thi `TC-H8A` (`P0`/`P1` — `bwrap` Survival, Runtime Binds, `user-dirs.dirs`, `focus` & Safe `stop`):**
      - Biên dịch `cargo build -p dim-cli` trên Linux và tạo thư mục tạm `$TempData`.
      - Tạo 2 profile và khởi chạy qua lệnh ngắn hạn `dim-cli --cli --json --data-dir "$TempData" --node-fixture --app-path fixtures/test-desktop-app/fixture-app.mjs launch <p1_id>` (với chế độ `bwrap` bật khi `bwrap` khả dụng và được phép bởi kernel/AppArmor trên runner, kèm kiểm thử cả chế độ XDG env trực tiếp).
      - Chờ `600ms` sau khi tiến trình `dim-cli` đã thoát hẳn: gọi `dim-cli ... status` và xác nhận `running == true` (tiến trình con **không** bị giết theo `dim-cli` — chứng minh đã xóa `--die-with-parent`).
      - Kiểm tra cây thư mục `<p1_dir>`: xác nhận tồn tại `Downloads`, `Documents`, `Pictures` và tệp `.config/user-dirs.dirs` có khai báo hợp lệ `XDG_DOWNLOAD_DIR="$HOME/Downloads"` và `XDG_DOCUMENTS_DIR="$HOME/Documents"`; bên trong sandbox đọc được socket dummy `$XDG_RUNTIME_DIR/pulse/native` & `bus` nhưng không nhìn thấy thư mục riêng tư ngoài `$HOME` thật.
      - Gọi `dim-cli ... focus <p1_id>`, chờ `200ms`: đọc `<p1_dir>/.config/ZaloData/launch-state.json` xác nhận `focus_count >= 1` (`focus_or_restore` hoạt động thực sự).
      - Mô phỏng giữ mở tệp DB `.config/ZaloData/Database/_production/100001/test.db` trong `1200ms` khi đóng, gọi `dim-cli ... stop <p1_id>`: xác nhận tiến trình thoát êm (`clean_shutdown == true`) và không bị bắn `SIGKILL` khi DB đang mở.
   2. **Thực thi `TC-H8B` (`P0` — AppArmor Unprivileged Userns Preflight, Zero `--no-sandbox`):**
      - Chạy unit test & integration test với cả giá trị thật của `/proc/sys/kernel/apparmor_restrict_unprivileged_userns` trên máy Ubuntu 24.04 lẫn mock sysfs (`1` và `0`).
      - Khi `restrict == 1` và truyền đường dẫn `/tmp/dim-h8-dummy.AppImage` chưa có profile trong `/etc/apparmor.d/`: xác nhận `discover` / `diagnostics` / `launch` báo cáo rõ ràng trạng thái `BLOCKED` (`error_code == "APPARMOR_USERNS_RESTRICTED"` hoặc chẩn đoán tương đương) kèm hướng dẫn quản trị viên tạo profile `userns,`.
      - Quét toàn bộ vector đối số (`build_bwrap_args` + `launch` argv) và hằng số trong `crates/platform-linux`: xác nhận **không hề truyền `--no-sandbox` hay `ELECTRON_DISABLE_SANDBOX=1`**.
   3. **Thực thi `TC-H8C` (`P2` — Đối chứng Shared Netns vs Rootless `pasta` Netns trên cổng `29631/29632`):**
      - *Đối chứng âm (Shared Netns):* Chạy 2 tiến trình probe TCP trên cùng host network namespace cùng gọi `server.listen(29631, '127.0.0.1')` và `29632` -> xác nhận Phiên 1 `PASS`, Phiên 2 thất bại ngay với `EADDRINUSE`.
      - *Đối chứng dương (Isolated Rootless `pasta` Netns):* Khởi chạy 2 tiến trình probe tương tự bên trong 2 phiên `pasta --config-net -- node tcp-29631-probe.mjs` riêng biệt trong khi cả hai vẫn truy cập cùng đường dẫn socket Unix giả lập `$XDG_RUNTIME_DIR/pulse/native`.
      - Xác nhận **cả 2 phiên đều lắng nghe đồng thời trên `127.0.0.1:29631` và `127.0.0.1:29632` thành công (`0` lỗi `EADDRINUSE`)**, nhận phản hồi echo nội bộ trên từng namespace riêng, và kết nối thành công vào chung socket `pulse/native`!
7. **`Expected behavior`:**
   - `8A`: Tiến trình sống sót sau khi `dim-cli` kết thúc (`running == true` sau `600ms`), đủ `Downloads`/`Documents`/`.config/user-dirs.dirs`, kết nối được socket desktop runtime, `focus_count >= 1`, `clean_shutdown == true`, không bắn `SIGKILL` vào phiên đang mở DB.
   - `8B`: Phát hiện chính xác `apparmor_restrict_unprivileged_userns == 1`, trả `BLOCKED` trung thực cho `.AppImage` chưa cấu hình profile, số lần truyền `--no-sandbox` bằng `0`.
   - `8C`: Trong cùng netns bị `EADDRINUSE` ở phiên 2; trong 2 netns `pasta` riêng biệt cả 2 phiên cùng bind `127.0.0.1:29631` & `29632` thành công 100% và cùng giao tiếp được với socket PipeWire/PulseAudio.
8. **`Failure conditions`:**
   - Tiến trình con tắt ngay khi `dim-cli launch` kết thúc (còn sót `--die-with-parent`).
   - `focus_or_restore` vẫn là hàm rỗng không tăng `focus_count`, hoặc `stop()` bắn `SIGKILL` làm đứt ngang phiên đang giữ file DB mở.
   - Adapter tự động truyền `--no-sandbox` / `ELECTRON_DISABLE_SANDBOX=1` hoặc báo `PASS` giả tạo trên Ubuntu 24.04 khi AppArmor đang chặn.
   - Hai phiên trong `pasta` riêng vẫn xung đột cổng `29631/29632` hoặc làm mất kết nối tới socket Unix âm thanh.
9. **`OS`:** Ubuntu 22.04 LTS & Ubuntu 24.04 LTS (`x86_64`, GNOME Wayland & X11 / headless CI với `dbus-run-session`).
10. **`Bằng chứng cần lưu`:**
    - Tệp `evidence/tc-h8-ubuntu-suite-result.json` chứa kết quả tách bạch 3 phần:
      - `part_8a`: `{ detached_survival_ok, xdg_user_dirs_ok, runtime_sockets_mounted_ok, focus_count, safe_stop_no_sigkill_ok }`
      - `part_8b`: `{ kernel_apparmor_restrict_value, unprofiled_appimage_reported_blocked: true, no_sandbox_arg_injected: false }`
      - `part_8c`: `{ shared_netns_second_bind_eaddrinuse: true, pasta_dual_bind_29631_29632_ok: true, shared_pulse_socket_reachable_ok: true }`
    - Log chạy `cargo test -p dim-platform-linux` với mã thoát `0`.
11. **`Người/agent chịu trách nhiệm`:** `Ubuntu agent`.
