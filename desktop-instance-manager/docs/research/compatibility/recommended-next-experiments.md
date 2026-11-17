# Đề Xuất Hướng Giải Quyết Hợp Chuẩn & Kế Hoạch Thực Nghiệm Tiếp Theo (`recommended-next-experiments.md`)

**Nhánh nghiên cứu:** `research/desktop-compatibility`  
**Đồng bộ truy xuất:** Khớp 100% mã phát hiện (`F-CORE-01..02`, `F-WIN-01..07`, `F-MAC-01..05`, `F-UBU-01..07`), đặc tả kiểm thử (`TC-H1A`, `TC-H1B`, `TC-H2`..`TC-H8` trong `./test-specifications.md`) và yêu cầu tích hợp (`IR-WIN-01..04`, `IR-MAC-01..04`, `IR-UBU-01..05` trong `./implementation-backlog.md`).  
**Nguyên tắc thiết kế bất biến:** Tuyệt đối không chứa mã khai thác, không vá cơ chế xác thực/chữ ký của Zalo (`Zalo.exe`, `Zalo.app`, `app.asar`), không inject DLL/hook kernel, không tắt Windows Smart App Control (`SAC`) hay Ubuntu AppArmor (`--no-sandbox` / `sysctl=0`), không tự động hóa đăng nhập hoặc gọi điện tới người dùng không đồng thuận, và không mô phỏng trái phép máy chủ/endpoint của VNG.

---

## Khung Xếp Hạng Ưu Tiên Theo Giá Trị Sản Phẩm (`Product-Value Ranking Rubric`)

Mọi hạng mục khắc phục và thực nghiệm trong tài liệu này bắt buộc tuân theo thứ bậc tác động sản phẩm từ cao xuống thấp (các rủi ro cấp tài khoản và toàn vẹn dữ liệu luôn đứng trên các tính năng phụ trợ; **tuyệt đối không để công việc Gọi thoại/Video (`P2`) chiếm tài nguyên trước khi khóa chặt an toàn dữ liệu và xác minh đăng nhập độc lập tài khoản thứ hai (`L3`, `P0`)**):

| Thứ bậc giá trị sản phẩm | Nhóm tác động người dùng | Mức ưu tiên bắt buộc |
| :--- | :--- | :--- |
| **1. Nguy cơ mất dữ liệu / hỏng DB** | Sao lưu thiếu DB ngoài (`db_path.location`), xóa nhầm qua NTFS junction, bắn `SIGTERM`/`SIGKILL` sớm khi đang đóng SQLite WAL | **`P0`** |
| **2. Đóng nhầm tiến trình / cửa sổ** | `EnumWindows` chọn nhầm popup `notiWin`/helper để gửi `WM_CLOSE`; `SingletonLock` tái sử dụng PID tiến trình khác | **`P0`** |
| **3. Chặn đăng nhập / khởi chạy tài khoản thứ hai (`L3`)** | Chưa xác minh `L3` 2 tài khoản thật; lỗi chặn mở phiên (`TMPDIR` > 104 byte trên macOS, `bwrap --die-with-parent`, thiếu preflight AppArmor) | **`P0`** |
| **4. Nhắn tin & hộp thoại gửi/nhận tệp** | Thiếu thư mục `Downloads`/`Pictures`/`.config/user-dirs.dirs` làm hộp thoại chọn/lưu tệp mở vào thư mục rỗng | **`P1`** |
| **5. Đóng/mở lại giữ phiên & chuyển cửa sổ** | Đánh thức cửa sổ ẩn dưới System Tray vượt Win32 Foreground Lock; kích hoạt cửa sổ macOS không đòi Accessibility TCC; `focus` trên Linux | **`P1`** |
| **6. Thông báo (`Notifications`)** | Gom nhóm `AppUserModelId("com.vng.zalo")` / `CFBundleIdentifier` vs popup nội bộ `notiWin` & xung đột badge `zalo.desktop` | **`P2`** |
| **7. Gọi điện thoại & Gọi video (`Calls`)** | Xung đột kênh IPC cục bộ của helper gọi (`PipeZCall*`, `/tmp/socketzalo*2021`, cổng TCP `29631/29632`) | **`P2`** |

---

## Phần I — Phân Tích & Hướng Giải Quyết Chi Tiết Cho 8 Giới Hạn Kỹ Thuật Cốt Lõi

*(Áp dụng nghiêm ngặt hệ phân loại bằng chứng 6 bậc: `Confirmed by source code`, `Confirmed by static binary inspection`, `Observed on running system`, `Confirmed by controlled experiment`, `Inference`, `Not tested`.)*

---

### Giới hạn 1: Xung đột kênh IPC cố định của Gọi thoại & Gọi video (`PipeZCall*` / `/tmp/socketzalo*2021` / `127.0.0.1:29631-29632`)
- **Liên kết định danh:** Findings `F-WIN-06`, `F-MAC-05`, `F-UBU-07` | Tests `TC-H1A`, `TC-H1B` (Giai đoạn 2), `TC-H5`, `TC-H8` (Phần 8C) | Integration Requests `IR-WIN-02`, `IR-MAC-04`, `IR-UBU-05`.

1. **Nguyên nhân & Phân tầng bằng chứng đã hiệu chỉnh:**
   - **Windows (`F-WIN-06`):**
     - `Confirmed by source code`: `app.asar/main-dist/main.js:311828..317500` (module `"3Zc2"`) gán cứng `\\.\pipe\PipeZCallRecv` và `\\.\pipe\PipeZCallSend`; nhánh `win32` không gỡ pipe khi gặp `EADDRINUSE`, và trong `serverSend` có gán `O = t => V(e, t)` mỗi khi client mới kết nối.
     - `Confirmed by static binary inspection`: REA `inspect_pe_imports` trên `ZaloCall.exe` (`ev_9145f8bd...`) xác nhận dùng `WINMM.dll` / `qtaudio_wasapi.dll` (WASAPI Shared Mode — tầng phần cứng âm thanh không phải điểm nghẽn; riêng webcam DirectShow UVC có thể bận nếu 2 tiến trình mở đồng thời không qua FrameServer — `Inference`).
     - `Observed on running system`: Trên máy Windows 11 nghiên cứu, 1 phiên Zalo chính đang đăng nhập ở trạng thái nghỉ (idle) có tiến trình `ZaloCall.exe` (`PID 15624`) và giữ mở cả 2 pipe.
     - **Hiệu chỉnh hạ cấp bằng chứng (`Inference` / `Not tested`):** Quan sát pipe mở lúc nghỉ trên Tài khoản #1 **chưa phải bằng chứng thực nghiệm** rằng Tài khoản #2 thất bại ở mọi chế độ gọi (ví dụ WebRTC nội bộ / `zcall_ia32.node`), chưa chứng minh trên Zalo thật `ZaloCall.exe` của Tài khoản #2 được spawn ngay lúc đăng nhập hay chỉ khi bấm nút Gọi, và chưa chứng minh việc ghi đè `O` làm treo vĩnh viễn cuộc gọi đi của Tài khoản #1 ngoài thực tế. Cần kiểm chứng tách bạch qua `TC-H1A` (tự động trên pipe tổng hợp) và `TC-H1B` Giai đoạn 2 (thủ công có đồng thuận).
   - **macOS (`F-MAC-05`):**
     - `Confirmed by source code`: `main.js:311828,314800` gán cứng `"/tmp/socketzalorecv2021"` & `"/tmp/socketzalosend2021"` và gọi `fs.unlink` khi `EADDRINUSE` (phiên khởi tạo sau cùng chiếm socket — `Author claim` từ `ducngfi` `README.md:199`; chưa chạy thực nghiệm tại workspace này: `Not tested`).
   - **Ubuntu (`F-UBU-07` — Phân tách 4 nguồn riêng biệt, không đánh đồng):**
     - *(A) Giới hạn của bản port cộng đồng (`Confirmed by source code`):* `VN-Linux-Family/zalo-for-linux` (`patch-zcall-native.js:307-318`) gán cứng cổng TCP `127.0.0.1:29631` & `29632`.
     - *(B) Lỗi của `dim-platform-linux` adapter (`Confirmed by source code`):* `crates/platform-linux/src/layout.rs:82-131` thiếu bind socket PipeWire/PulseAudio (`$XDG_RUNTIME_DIR/pulse/native`) và `/dev/video*` trong `bwrap`.
     - *(C) Ràng buộc môi trường mạng dùng chung vs cô lập (`Inference` / `Not tested`):* Lỗi `EADDRINUSE` trên cổng `29631/29632` **chỉ xảy ra khi các phiên dùng chung host network namespace (`lo`)**; nếu chạy mỗi phiên trong rootless network namespace riêng (`pasta`), mỗi phiên có giao diện `127.0.0.1` độc lập nên không xung đột cổng TCP (kiểm chứng tại `TC-H8C`). Riêng webcam vật lý V4L2 `/dev/video0` vẫn khóa đơn luồng (`EBUSY`) khi mở đồng thời.
     - *(D) Giới hạn của Zalo VNG chính thức (`Confirmed by source code` — `Unsupported`):* VNG không phát hành bản Linux Desktop chính thức (`main.js:314392`, `file-utilities/index.js:36`).
2. **Những phần chưa biết (`Not tested`):**
   - Thời điểm chính xác `ZaloCall.exe` khởi chạy trên Tài khoản #2 (lúc quét QR xong hay lúc bấm Gọi) và khả năng khôi phục 100% cuộc gọi 1-1/nhóm khi đóng êm các phiên khác trên Windows (`TC-H1B` Giai đoạn 2).
   - Hành vi thực tế của 2 phiên lắng nghe độc lập `127.0.0.1:29631/29632` qua `pasta` kết hợp chia sẻ socket PipeWire trên Ubuntu 24.04 (`TC-H8C`).
3. **Ảnh hưởng UX:**
   - Gọi thoại/video ở tài khoản phụ có thể không kết nối được hoặc chiếm kênh gọi của tài khoản đang mở trước/sau tùy hệ điều hành.
4. **Phương án tương thích hợp lệ (Chỉ triển khai sau khi xong `P0` & `P1`):**
   - **Chẩn đoán thụ động chỉ đọc & Chế độ "Ưu tiên Cuộc gọi" (Designated Call-Owner Mode):**
     - Trên Windows (`IR-WIN-02`): Truy vấn thụ động chỉ đọc PID giữ `\\.\pipe\PipeZCallRecv` (`GetNamedPipeServerProcessId` / đối chiếu cây PID `ZaloCall.exe`) trong `diagnostics` mà không gửi dữ liệu vào pipe; hiển thị nhãn rõ ràng profile nào đang nắm kênh gọi và hướng dẫn đóng êm các phiên khác khi cần thực hiện cuộc gọi quan trọng.
     - Trên macOS (`IR-MAC-04`): Mở rộng `inspect_call_sockets_at` (`crates/platform-macos/src/layout.rs:138-149`) báo cáo chủ sở hữu `/tmp/socketzalo*2021` và cung cấp tùy chọn khởi động lại sau cùng profile cần gọi (tận dụng cơ chế `fs.unlink` tự nhiên).
     - Trên Ubuntu (`IR-UBU-05`): Báo cáo trung thực sự khác biệt 4 tầng, kiểm tra thụ động trạng thái cổng `29631/29632` và hỗ trợ hook tùy chọn `pasta` rootless netns khi có sẵn trên máy.
5. **Phương án loại bỏ vì không an toàn hoặc không khả thi:**
   - Sửa chuỗi tên pipe/socket trong `app.asar` hoặc vá nhị phân `ZaloCall.exe` (làm vỡ chữ ký số VNG và `CALL_NATIVE_HASH` `verifyMd5` tại `main.js:314500`).
   - Inject DLL (`Detours`/`MinHook`/`SbieDll.dll`) hoặc cài kernel driver (vi phạm ranh giới an toàn, bị chặn bởi Smart App Control).
6. **Test cần thiết:** `TC-H1A` (tự động), `TC-H1B` Giai đoạn 2 (thủ công tùy chọn sau `L3`), `TC-H5` (tự động macOS), `TC-H8` Phần `8C` (tự động Ubuntu).
7. **Phân công Agent:** `Windows agent` (`IR-WIN-02`), `macOS agent` (`IR-MAC-04`), `Ubuntu agent` (`IR-UBU-05`), `Human QA opt-in` (`TC-H1B`).
8. **Mức độ ưu tiên:** **`P2` (Tính năng phụ trợ Gọi điện — Hạ từ mức `P0` cũ xuống `P2` theo đúng thang giá trị sản phẩm để tuyệt đối không làm chậm việc sửa lỗi an toàn dữ liệu và xác minh đăng nhập tài khoản thứ 2 `L3`!).**

---

### Giới hạn 2: Hộp thoại gửi/lưu tệp mở vào thư mục rỗng trên Windows & Linux + Chốt an toàn xóa NTFS Junction
- **Liên kết định danh:** Findings `F-WIN-04`, `F-UBU-02` | Tests `TC-H7`, `TC-H8` (Phần 8A) | Integration Requests `IR-WIN-03`, `IR-UBU-02`.

1. **Nguyên nhân & Bằng chứng:**
   - `Confirmed by source code`: `crates/platform-windows/src/layout.rs:14-29` đặt `USERPROFILE=<ProfileDir>` nhưng chỉ tạo `Desktop` và `Documents\Zalo Received Files`, thiếu `Downloads`, `Pictures`, `Videos`, `Music`. Khi Zalo mở `dialog.showOpenDialog` / `showSaveDialog` (`main.js:997824`), hộp thoại mở vào thư mục giả lập trống.
   - `Confirmed by source code`: `crates/platform-linux/src/layout.rs:22-42` chưa tạo `Downloads`, `Documents`, `Pictures` và thiếu tệp `.config/user-dirs.dirs` (`patch-xdg-user-dirs.js:395-465`).
   - `Confirmed by source code` & `Confirmed by controlled experiment`: `crates/profile-store/src/store.rs:377-427` đã có `unlink_reparse_points_safe` (`Test-8` trong `run-fixture-tests.mjs`), cần kiểm thử mở rộng với bộ tệp canary đầy đủ cho `Downloads`/`Desktop` trong `TC-H7`.
2. **Những phần chưa biết (`Not tested`):**
   - Cách hiển thị mục *Quick Access* của `IFileOpenDialog` trên Windows 11 khi dùng NTFS junction chọn lọc trên tài khoản thật (`L3-3` trong `TC-H1B`).
3. **Ảnh hưởng UX & An toàn dữ liệu:**
   - Thiếu thư mục làm giảm mạnh tốc độ gửi/lưu tệp hằng ngày (`P1`); nếu triển khai liên kết thư mục mà thiếu kiểm chứng gỡ reparse point trước khi xóa sẽ gây nguy cơ mất dữ liệu host (`P0`).
4. **Phương án tương thích hợp lệ:**
   - **Mặc định cô lập đầy đủ (`IR-WIN-03`, `IR-UBU-02`):** Tạo sẵn `Downloads`, `Pictures`, `Videos`, `Music`, `Desktop`, `Documents\Zalo Received Files` trên Windows và tạo `Downloads`, `Documents`, `Pictures` + `.config/user-dirs.dirs` trên Linux.
   - **Opt-in NTFS Junction có chốt an toàn (`IR-WIN-03`):** Cho phép bật liên kết junction theo từng profile cho `Downloads` và `Desktop`, **bắt buộc giữ `Documents\Zalo Received Files` là thư mục vật lý riêng của từng profile**, và cưỡng bức `unlink_reparse_points_safe` trước mọi thao tác xóa.
5. **Phương án loại bỏ:** Trỏ `USERPROFILE` về thư mục thật của user (mất cô lập `ZaloData`) hoặc gọi xóa đệ quy không kiểm tra `FILE_ATTRIBUTE_REPARSE_POINT`.
6. **Test cần thiết:** `TC-H7` (Phần 7A), `TC-H8` (Phần 8A).
7. **Phân công Agent:** `Windows agent` (`IR-WIN-03`), `Ubuntu agent` (`IR-UBU-02`).
8. **Mức độ ưu tiên:** **`P0` (cho chốt an toàn xóa reparse point chống mất dữ liệu host) / `P1` (cho tạo bố cục thư mục & UX hộp thoại tệp).**

---

### Giới hạn 3: Lỗi chọn nhầm cửa sổ phụ (`EnumWindows`) & Chặn kích hoạt cửa sổ ẩn ở System Tray trên Windows
- **Liên kết định danh:** Findings `F-WIN-02`, `F-WIN-03` | Tests `TC-H2` (Phần 2A), `TC-H6` | Integration Requests `IR-WIN-01`, `IR-WIN-02`.

1. **Nguyên nhân & Bằng chứng:**
   - `Confirmed by source code`: Trong `crates/platform-windows/src/win32_ffi.rs:884-908`, `enum_windows_cb` nhận ngay HWND `IsWindowVisible != 0` đầu tiên của bất kỳ PID nào trong cây mà không lọc `main_pid`, class `Chrome_WidgetWin_1`, `GetWindow(hwnd, GW_OWNER) == 0`, `!WS_EX_TOOLWINDOW`, tiêu đề khác rỗng và kích thước tối thiểu. Cửa sổ popup `notiWin` (`352x82`, `alwaysOnTop: true` tại `main.js:2726777`) hoặc helper nằm đầu Z-order có nguy cơ nhận nhầm `WM_CLOSE` khi `stop()` (`adapter.rs:743-745`)!
   - `Confirmed by source code`: Khi cửa sổ chính ẩn xuống System Tray (`IsWindowVisible == 0`) hoặc bị Win32 Foreground Lock chặn khi gọi `SetForegroundWindow` từ nền (`win32_ffi.rs:930-944`, `adapter.rs:642-665`), chỉ gọi `focus_window(hwnd)` sẽ không đưa được cửa sổ lên trước, trong khi Zalo có sẵn handler `second-instance` (`main.js:3565794`) tự gọi `setAlwaysOnTop(true)` -> `focus()`.
2. **Những phần chưa biết (`Not tested`):**
   - Phân bố độ trễ thực tế (`ms`) của handoff `second-instance` khi ẩn dưới Tray và bị đè bởi cửa sổ tiền cảnh (`TC-H6`).
3. **Ảnh hưởng UX & Độ tin cậy:**
   - Gửi nhầm `WM_CLOSE` vào cửa sổ phụ làm hỏng luồng đóng phiên chuẩn (`P0`); không đánh thức được cửa sổ từ Tray gây cản trở thao tác chuyển tài khoản hằng ngày (`P1`).
4. **Phương án tương thích hợp lệ (`IR-WIN-01`, `IR-WIN-02`):**
   - Siết chặt bộ lọc trong `win32_ffi.rs` và `scripts/dim-host.ps1`: chỉ nhận cửa sổ thuộc `main_pid`, class `Chrome_WidgetWin_1` (hoặc class cửa sổ chính của fixture), `GetWindow(hwnd, GW_OWNER) == 0`, không có `WS_EX_TOOLWINDOW`, tiêu đề khác rỗng và kích thước `>= 400x400`; phân loại riêng `notiWin` (`352x82`) để loại trừ khỏi `stop()`/`focus_or_restore()`.
   - Trong `focus_or_restore`: khi `window_handle.is_none()` (ẩn ở Tray) hoặc sau `focus_window(hwnd)` mà `GetForegroundWindow() != hwnd`, tự động kích hoạt handoff `second-instance` trong môi trường cô lập của profile đó.
5. **Phương án loại bỏ:** Dùng `AttachThreadInput` xuyên tiến trình hoặc giả lập phím `Alt` qua `SendInput` (gây nguy cơ treo message queue `USER32`).
6. **Test cần thiết:** `TC-H2` (Phần 2A — `P0`), `TC-H6` (`P1`).
7. **Phân công Agent:** `Windows agent` (`IR-WIN-01`, `IR-WIN-02`).
8. **Mức độ ưu tiên:** **`P0` (Sửa bộ lọc `EnumWindows` chống đóng nhầm cửa sổ) / `P1` (Handoff đánh thức cửa sổ từ System Tray & vượt Foreground Lock).**

---

### Giới hạn 4: Định danh Thông báo dùng chung (`com.vng.zalo`) & Xung đột Unread Badge trên Windows, macOS, Ubuntu
- **Liên kết định danh:** Findings `F-WIN-05`, `F-MAC-04`, `F-UBU-06` | Tests `TC-H2` (Phần 2B), `TC-H4`, `TC-H8` | Integration Requests `IR-WIN-02`, `IR-MAC-04`, `IR-UBU-02`, `IR-UBU-05`.

1. **Nguyên nhân & Bằng chứng:**
   - `Confirmed by source code` & `Confirmed by static binary inspection`: `main.js:2720673` gọi cứng `app.setAppUserModelId("com.vng.zalo")`; `Zalo.lnk` (`ev_69ddf74b...`) trỏ về Stub `Zalo.exe` gốc. Trên macOS, `usernoted` gom theo `CFBundleIdentifier = "com.vng.zalo"`, nhưng badge đỏ trên Dock (`app.dock.setBadge`, `main.js:1013307`) gắn riêng theo từng tiến trình `open -n` (`Inference` từ kiến trúc AppKit). Trên Ubuntu, bản cộng đồng `patch-notification-badge.js:859-886` phát `LauncherEntry.Update` đè lên chung `application://zalo.desktop` (`Confirmed by source code`), và cần `NOTIFY_IGNORE_PORTAL=1` cho Electron 22 (`main.js:725-727`).
   - `Confirmed by source code`: Zalo có sẵn cửa sổ popup thông báo nội bộ `notiWin` (`main.js:2726777`, `znotification.html` + `preload-noti.js`) chạy riêng trong từng tiến trình Electron (`Inference` định tuyến click chính xác 100%, `Not tested` trên 2 tài khoản thật).
2. **Những phần chưa biết (`Not tested`):**
   - Hành vi click banner gốc của macOS `usernoted` và Windows `notiWin` khi 2+ tài khoản thật đang mở.
3. **Ảnh hưởng UX:**
   - Bấm lại thông báo cũ trong lịch sử Windows Action Center có thể mở Zalo chính; số badge trên Ubuntu Dock nhảy theo phiên cập nhật cuối.
4. **Phương án tương thích hợp lệ:**
   - Hướng dẫn người dùng bật chế độ **popup thông báo nội bộ của Zalo (`notiWin`)** trên Windows (`IR-WIN-02`), tận dụng badge riêng trên từng icon Dock của macOS (`IR-MAC-04`), và ghi nhận `NOTIFY_IGNORE_PORTAL=1` + cảnh báo giới hạn badge `zalo.desktop` trên Ubuntu (`IR-UBU-02`, `IR-UBU-05`).
5. **Phương án loại bỏ:** Sửa `setAppUserModelId` trong `app.asar`, ghi đè `Zalo.lnk` của người dùng chính, hoặc nhân bản/ký lại `Zalo.app` với `CFBundleIdentifier` giả (phá vỡ `SecureCipher.checkAppSigned()` tại `main.js:3562292` và làm hỏng quyền Keychain/TCC).
6. **Test cần thiết:** `TC-H2` (Phần 2B).
7. **Phân công Agent:** `Windows agent` (`IR-WIN-02`), `macOS agent` (`IR-MAC-04`), `Ubuntu agent` (`IR-UBU-05`), `Human QA opt-in` (`TC-H2` Phần 2B).
8. **Mức độ ưu tiên:** **`P2` (Tối ưu cấu hình & hướng dẫn UX thông báo).**

---

### Giới hạn 5: Bốn lỗi tĩnh nghiêm trọng trong `dim-platform-macos` (`TMPDIR` vs 104-byte socket, `SIGTERM` sớm, Accessibility TCC, thiếu kiểm tra `bundleIdentifier` & Pre-seed)
- **Liên kết định danh:** Findings `F-MAC-01`, `F-MAC-02`, `F-MAC-03`, `F-MAC-04` | Tests `TC-H3`, `TC-H4` | Integration Requests `IR-MAC-01`, `IR-MAC-02`, `IR-MAC-03`.

1. **Nguyên nhân & Phân tầng bằng chứng đã hiệu chỉnh:**
   - **Rủi ro đường dẫn socket `104 bytes` & `TMPDIR` (`F-MAC-01`):**
     - `Confirmed by source code`: `crates/platform-macos/src/layout.rs:97` truyền `--env TMPDIR=<profile_dir>/tmp`, và `layout.rs:34-51` thiếu pre-seed `zmeta.json` & `migrate-config.json`. Trong SDK Darwin `<sys/un.h>`, `sockaddr_un.sun_path` có kích thước cố định `104` bytes (`sizeof(sockaddr_un) == 106`); đồng thời cả `libuv` (`uv_pipe_bind`) lẫn Chromium (`ProcessSingletonPosix` / `IPC::SocketPosix`) đều kiểm tra cứng độ dài đường dẫn `< sizeof(sun_path)` = `104` bytes (dù nhân XNU BSD ở tầng syscall thô có lưu ý `SUN_LEN`/`SOCK_MAXADDRLEN = 255`, các thư viện C/C++ user-space của Electron/Node vẫn chặn cứng tại 104 bytes!).
     - **Hiệu chỉnh hạ cấp bằng chứng (`Inference` / `Not tested`):** Vì chưa chạy trên máy Mac vật lý và trong Chromium `base/files/file_util_mac.mm` (`GetTempDir()`) ưu tiên `NSTemporaryDirectory()` / `confstr(_CS_DARWIN_USER_TEMP_DIR)` (`-> /var/folders/...`) trừ khi dùng `MAC_CHROMIUM_TMPDIR` (`< 50` ký tự) trong khi Node.js `os.tmpdir()` lại đọc `getenv("TMPDIR")`, khẳng định lỗi khởi chạy `SingletonSocket` đầu-cuối dưới `/usr/bin/open -n --env CFFIXED_USER_HOME=...` được hạ cấp xuống mức `Inference` / `Not tested` chờ `TC-H3` xác minh.
   - **Lỗi `SIGTERM` sớm trước vòng chờ (`F-MAC-02` — `Confirmed by source code`):** `crates/platform-macos/src/adapter.rs:387-394` bắn `libc::kill(raw_pid, libc::SIGTERM)` **ngay trước** vòng lặp `while Instant::now() < deadline` ở dòng `397`, nguy cơ ngắt ngang quá trình đóng SQLite WAL sau lệnh bất đồng bộ `terminate()`.
   - **Lỗi đòi Accessibility TCC & thiếu kiểm tra `bundleIdentifier` (`F-MAC-03` — `Confirmed by source code`):** `adapter.rs:342-346` gọi `System Events` đòi quyền nhạy cảm `kTCCServiceAccessibility`; `adapter.rs:193` chỉ kiểm tra `kill(pid, 0)` mà không kiểm tra `bundleIdentifier == "com.vng.zalo"`, dễ nhầm tiến trình nếu hệ điều hành tái sử dụng PID của `SingletonLock` cũ.
2. **Những phần chưa biết (`Not tested`):**
   - Thời gian đóng trung bình của `terminate()` trên Apple Silicon (`arm64`) / Intel (`x86_64`) và trải nghiệm hộp thoại Keychain lần đầu (`TC-H3`, `TC-H4`).
3. **Ảnh hưởng UX & An toàn dữ liệu:**
   - Nguy cơ chặn khởi chạy (`P0`), nguy cơ hỏng cơ sở dữ liệu khi bấm Stop (`P0`), nguy cơ báo nhầm/dừng nhầm PID (`P0`), và đòi quyền Accessibility không cần thiết (`P1`).
4. **Phương án tương thích hợp lệ:**
   - `IR-MAC-01` (`P0`): Bỏ truyền `--env TMPDIR=<profile_dir>/tmp` dài trong `build_open_args` (giữ `/var/folders/...` ngắn mặc định của macOS hoặc bảo đảm tổng đường dẫn socket `< 104` bytes); bổ sung pre-seed `zmeta.json` và `migrate-config.json` trong `ensure_macos_profile_layout`.
   - `IR-MAC-02` (`P0`): Trong `MacOsPlatformAdapter::stop`, đặt vòng lặp chờ `timeout_ms` cho `terminate()` **trước** khi xét `SIGTERM`, kiểm tra không còn tệp mở trong `Database/_production/`, tuyệt đối không bắn `SIGKILL`.
   - `IR-MAC-03` (`P0`/`P1`): Đối chiếu `bundleIdentifier == "com.vng.zalo"` (hoặc đường dẫn thực thi) cho PID đọc từ `SingletonLock`; đổi `focus_or_restore` sang `NSRunningApplication(processIdentifier:)?.activate()` không cần `System Events`.
5. **Phương án loại bỏ:** Bắn `SIGKILL` vào phiên đang mở DB hoặc bắt người dùng cấp quyền Accessibility TCC toàn máy.
6. **Test cần thiết:** `TC-H3` (`P0`), `TC-H4` (`P0` / `P1`).
7. **Phân công Agent:** `macOS agent` (`IR-MAC-01`, `IR-MAC-02`, `IR-MAC-03`).
8. **Mức độ ưu tiên:** **`P0` (cho `IR-MAC-01`, `IR-MAC-02` và kiểm tra PID của `IR-MAC-03`) / `P1` (cho kích hoạt cửa sổ AppKit của `IR-MAC-03`).**

---

### Giới hạn 6: Năm lỗi thiết kế trong `dim-platform-linux` (`--die-with-parent`, thiếu bind Desktop Runtime Sockets trong `bwrap`, `focus` rỗng, `SIGKILL` mù)
- **Liên kết định danh:** Findings `F-UBU-01`, `F-UBU-02`, `F-UBU-03`, `F-UBU-05` | Test `TC-H8` (Phần 8A) | Integration Requests `IR-UBU-01`, `IR-UBU-02`, `IR-UBU-03`.

1. **Nguyên nhân & Bằng chứng (`Confirmed by source code`):**
   - `crates/platform-linux/src/layout.rs:84` (`F-UBU-01`): Cờ `--die-with-parent` làm `bwrap` giết ngay phiên ứng dụng khi lệnh ngắn hạn `dim-cli launch` thoát. (Lưu ý phân biệt rõ với phần chính thức của `F-UBU-01`: Zalo VNG chính thức là `Unsupported` trên Linux).
   - `crates/platform-linux/src/layout.rs:82-131` (`F-UBU-02`): `build_bwrap_args` thiếu bind `$XDG_RUNTIME_DIR` (Wayland, PipeWire, PulseAudio, Session D-Bus cho `gnome-keyring` & Notifications), `/tmp/.X11-unix`, `/etc/fonts`, `/etc/machine-id`, `/dev/dri`, `/dev/video*`, `/dev/fuse`.
   - `crates/platform-linux/src/adapter.rs:357` (`F-UBU-03`): Bắn `libc::SIGKILL` vô điều kiện ngay khi hết `timeout_ms` dù cơ sở dữ liệu có thể đang mở.
   - `crates/platform-linux/src/adapter.rs:310-318` (`F-UBU-05`): `focus_or_restore` là stub rỗng `Ok(())`.
2. **Những phần chưa biết (`Not tested`):**
   - Phản hồi `xdg_activation_v1` trên GNOME Wayland Ubuntu 24.04 so với phiên X11 khi kích hoạt qua `second-instance`.
3. **Ảnh hưởng UX & An toàn dữ liệu:**
   - Ứng dụng tắt ngay sau khi mở hoặc mất kết nối GUI/Keyring (`P0`), nguy cơ hỏng DB do `SIGKILL` (`P0`), không chuyển được cửa sổ (`P1`).
4. **Phương án tương thích hợp lệ:**
   - `IR-UBU-01` (`P0`): Xóa `--die-with-parent` trong `build_bwrap_args`; bỏ `SIGKILL` tự động khi còn tệp DB mở trong `.config/ZaloData/Database/_production/`.
   - `IR-UBU-02` (`P0`/`P1`): Bổ sung bind có kiểm tra tồn tại (`--ro-bind-try` / `--bind-try` / `--dev-bind-try`) cho các socket desktop thiết yếu + tạo `Downloads`/`Documents`/`Pictures`/`.config/user-dirs.dirs`.
   - `IR-UBU-03` (`P1`): Cài đặt `focus_or_restore` bằng cách gọi lại thực thi mục tiêu trong cùng biến môi trường `HOME`/`XDG_*` cô lập để kích hoạt `second-instance` (`showMainWindow`), kèm ghi chú trung thực về Focus Stealing Prevention của GNOME Wayland.
5. **Phương án loại bỏ:** Dùng Zalo Web bọc trình duyệt thay cho Desktop hoặc tự tải/phân phối lại `app.asar` độc quyền của VNG.
6. **Test cần thiết:** `TC-H8` (Phần 8A).
7. **Phân công Agent:** `Ubuntu agent` (`IR-UBU-01`, `IR-UBU-02`, `IR-UBU-03`).
8. **Mức độ ưu tiên:** **`P0` (cho `IR-UBU-01` và các bind sống còn của `IR-UBU-02`) / `P1` (cho `.config/user-dirs.dirs` và `IR-UBU-03`).**

---

### Giới hạn 7: Hạn chế AppArmor Unprivileged User Namespaces trên Ubuntu 23.10+ / 24.04 LTS
- **Liên kết định danh:** Finding `F-UBU-04` | Test `TC-H8` (Phần 8B) | Integration Request `IR-UBU-04`.

1. **Nguyên nhân & Bằng chứng:**
   - `Confirmed by source code` & tài liệu kernel Ubuntu 24.04: `kernel.apparmor_restrict_unprivileged_userns = 1` chặn `chrome-sandbox` của các `.AppImage` mount ngẫu nhiên tại `/tmp/.mount_ZaloXXXXXX` chưa có AppArmor profile cấp quyền `userns,`. Hiện `crates/platform-linux` mới có chuỗi hằng `APPARMOR_NO_SANDBOX_NOTE` (`layout.rs:12-13`), chưa đọc sysfs để kiểm tra preflight trung thực.
2. **Những phần chưa biết (`Not tested`):**
   - Khác biệt hành vi giữa AppArmor profile cho đường dẫn `.AppImage` cố định so với thư mục AppDir giải nén cố định trên Ubuntu 24.04.1+.
3. **Ảnh hưởng UX & Bảo mật:**
   - Tiến trình thoát ngay khi khởi chạy mà không có giải thích rõ ràng (`P0` launch blocker trên Ubuntu 24.04).
4. **Phương án tương thích hợp lệ (`IR-UBU-04`):**
   - Đọc `/proc/sys/kernel/apparmor_restrict_unprivileged_userns` và `/sys/module/apparmor/parameters/enabled` trước khi chạy; nếu đang bật (`1`) và đường dẫn thực thi chưa có AppArmor profile hợp lệ, trả về trạng thái chẩn đoán trung thực `BLOCKED` kèm mẫu hướng dẫn quản trị viên cấu hình `/etc/apparmor.d/` với quyền tối thiểu `userns,` (giữ nguyên 100% Chromium renderer sandbox).
5. **Phương án loại bỏ tuyệt đối:** Truyền `--no-sandbox` / `ELECTRON_DISABLE_SANDBOX=1` hoặc yêu cầu tắt AppArmor toàn hệ thống (`sysctl=0`).
6. **Test cần thiết:** `TC-H8` (Phần 8B).
7. **Phân công Agent:** `Ubuntu agent` (`IR-UBU-04`).
8. **Mức độ ưu tiên:** **`P0` (Chặn khởi chạy có kiểm soát & tuân thủ bảo mật tuyệt đối trên Ubuntu 24.04).**

---

### Giới hạn 8: Rủi ro cấu hình ngoài biến môi trường (`db_path.location` ổ đĩa ngoài, Cập nhật khi đang mở nhiều phiên & Xác minh Đăng nhập Độc lập `L3`)
- **Liên kết định danh:** Findings `F-CORE-01`, `F-CORE-02`, `F-WIN-01`, `F-WIN-07` | Tests `TC-H7` (Phần 7B), `TC-H1B` (Giai đoạn 1) | Integration Request `IR-WIN-04`.

1. **Nguyên nhân & Bằng chứng:**
   - `Confirmed by source code`: `crates/core/src/zalo_identity.rs:35-49` đọc `config.json -> db_path.location`, nhưng `crates/process-manager/src/guard.rs` **chưa chặn 2 profile (hoặc 1 profile phụ và Zalo chính) cùng mở chung một thư mục `db_path.location` / `resource.location.path` bên ngoài**, và `crates/profile-store/src/backup.rs` **chưa chặn/cảnh báo khi `backup` một profile có DB đặt ngoài `<ProfileDir>`** (`F-CORE-01`) — dẫn tới nguy cơ hỏng DB hoặc tạo bản sao lưu rỗng lịch sử tin nhắn!
   - `Confirmed by source code` & `Confirmed by static binary inspection`: Khi `Update.exe` (`ev_9f322e5a...`, `main.js:2441895`) hoặc `Squirrel.Mac` (`main.js:3396325`) cập nhật trong lúc các phiên phụ vẫn mở, tệp nhị phân bị khóa file hoặc lệch ánh xạ `app.asar` (`F-CORE-02`); các khóa `HKCU` (`zalo://`, `Run`) dùng chung (`F-WIN-01`).
   - `Confirmed by static binary inspection` / `Not tested` (runtime E2E): Cơ chế DPAPI `EnableCookieEncryption` + `SecureCipher` (`F-WIN-07`) đã xác minh tĩnh, nhưng đăng nhập thật 2 tài khoản song song (`L3`) trên host vẫn là `Not tested` (`TC-H1B` Giai đoạn 1).
2. **Những phần chưa biết (`Not tested`):**
   - Khả năng đổi `db_path.location` trên giao diện Zalo macOS 26.10+ và kết quả thực tế của `L3-1..L3-4` với 2 tài khoản thật.
3. **Ảnh hưởng An toàn dữ liệu & Sản phẩm:**
   - Nguy cơ mất dữ liệu tin nhắn khi khôi phục từ bản sao lưu thiếu DB ngoài và nguy cơ ghi đè hỏng SQLite giữa 2 phiên (`P0`).
4. **Phương án tương thích hợp lệ (`IR-WIN-04`):**
   - Cài đặt `ExternalStorageGuard` xuyên suốt `dim-core`, `dim-process-manager`, `dim-profile-store` và `scripts/dim-host.ps1`:
     1. Chặn `launch` với mã lỗi `SHARED_EXTERNAL_DB_PATH` nếu `db_path.location` hoặc `resource.location.path` trùng với bất kỳ phiên đang chạy nào (kể cả `%APPDATA%\ZaloData\config.json` của Zalo chính).
     2. Chặn `backup` mặc định với mã lỗi `EXTERNAL_DB_OUTSIDE_PROFILE` khi `db_path.location` nằm ngoài `<ProfileDir>` trừ khi người dùng truyền cờ xác nhận tường minh.
     3. Cung cấp thao tác *"Đóng êm toàn bộ phiên phụ để Cập nhật Zalo"* (`F-CORE-02`).
5. **Phương án loại bỏ:** Chặn `Update.exe` của Zalo hoặc tự ý sửa tệp `config.json` của tài khoản người dùng.
6. **Test cần thiết:** `TC-H7` (Phần 7B — tự động), `TC-H1B` (Giai đoạn 1 — xác minh thủ công `L3`).
7. **Phân công Agent:** `Windows agent` (`IR-WIN-04`) và `Human QA opt-in` (`TC-H1B` Giai đoạn 1).
8. **Mức độ ưu tiên:** **`P0` (Ưu tiên số 1 về bảo toàn dữ liệu sao lưu, chống hỏng DB & xác minh đăng nhập cốt lõi `L3`).**

---

## Phần II — 8 Giả Thuyết Kỹ Thuật Có Giá Trị Kiểm Chứng Cao Nhất (`H1` – `H8`)

*(Giữ nguyên mã định danh `H1`–`H8` để bảo toàn tham chiếu chéo, nhưng gắn nhãn ưu tiên `P0`/`P1`/`P2` theo đúng thang Giá trị Sản phẩm và liên kết trực tiếp tới `F-*`, `TC-H*`, `IR-*`.)*

### `H1` — Giả thuyết Phân tách Pha Khởi tạo Pipe Lúc Đăng nhập vs Ghi đè Callback `O` Lúc Gọi trên Windows
- **Ưu tiên Giá trị Sản phẩm:** **`P0` cho Giai đoạn 1 (`L3` đăng nhập độc lập)** / **`P2` cho Giai đoạn 2 & Harness ống gọi (`Calls`)**
- **Liên kết:** `F-WIN-06`, `F-WIN-07` | `TC-H1A` (tự động), `TC-H1B` (thủ công 2 giai đoạn) | `IR-WIN-02`
- **Trạng thái bằng chứng hiện tại:** Mã gán cứng `PipeZCall*` và `O = t => V(e, t)` là `Confirmed by source code`; 1 phiên chính mở pipe khi nghỉ là `Observed on running system`; việc Tài khoản #2 lỗi ở thời điểm nào và có làm treo cuộc gọi đi của Tài khoản #1 hay không là **`Inference` / `Not tested`**.
- **Cách kiểm chứng an toàn:** Chạy tự động `TC-H1A` trên pipe tổng hợp `\\.\pipe\DimTestCall*` + kiểm tra thủ công có đồng thuận `TC-H1B` (hoàn thành Giai đoạn 1 `L3` đăng nhập/nhắn tin trước, sau đó mới đo Giai đoạn 2 cuộc gọi giữa chính 2 tài khoản của kiểm thử viên).

### `H2` — Giả thuyết Loại trừ Cửa sổ Phụ `notiWin` trong `EnumWindows` & Định tuyến Click Nội bộ trên Windows
- **Ưu tiên Giá trị Sản phẩm:** **`P0` cho bộ lọc `EnumWindows` chống đóng nhầm cửa sổ** / **`P2` cho UX click thông báo `notiWin`**
- **Liên kết:** `F-WIN-02`, `F-WIN-05` | `TC-H2` (Phần 2A tự động, Phần 2B thủ công tùy chọn) | `IR-WIN-01`, `IR-WIN-02`
- **Trạng thái bằng chứng hiện tại:** Thiếu bộ lọc trong `win32_ffi.rs:884-908` và kiến trúc `notiWin` (`352x82`) tại `main.js:2726777` là `Confirmed by source code`; click thực tế trên 2 tài khoản thật là `Inference` / `Not tested`.
- **Cách kiểm chứng an toàn:** Chạy tự động `TC-H2` Phần 2A tạo đồng thời cửa sổ chính `>= 800x600` và popup `352x82` (`WS_EX_TOPMOST | WS_EX_TOOLWINDOW`) để chứng minh `stop()`/`focus` không bao giờ chạm nhầm popup; kiểm tra tùy chọn click `notiWin` trong `TC-H2` Phần 2B.

### `H3` — Giả thuyết Ranh giới `104-byte` `sockaddr_un` trên Darwin & Xung đột `TMPDIR` dưới `open -n`
- **Ưu tiên Giá trị Sản phẩm:** **`P0` (Chặn khởi chạy đa phiên trên macOS)**
- **Liên kết:** `F-MAC-01` | `TC-H3` | `IR-MAC-01`
- **Trạng thái bằng chứng hiện tại:** `layout.rs:97` truyền `--env TMPDIR=<profile_dir>/tmp` và kiểm tra `< 104` bytes của `libuv` (`uv_pipe_bind`) / Chromium (`ProcessSingletonPosix`) là `Confirmed by source code`; tương tác runtime giữa `getenv("TMPDIR")` và `NSTemporaryDirectory()` (`confstr(_CS_DARWIN_USER_TEMP_DIR)`) dưới `open -n --env CFFIXED_USER_HOME=...` là **`Inference` / `Not tested`**.
- **Cách kiểm chứng an toàn:** Thực thi `TC-H3` tự động trên macOS 14+ với đường dẫn ngắn (`< 30` ký tự) đối chứng đường dẫn thực tế (`~95` ký tự) và xác nhận bản sửa `IR-MAC-01` (không ghi đè `TMPDIR` dài + pre-seed `zmeta.json`/`migrate-config.json`).

### `H4` — Giả thuyết Đóng êm Bảo toàn DB trước `SIGTERM`, Chống Nhầm PID & Chia sẻ Khóa Bọc Keychain trên macOS
- **Ưu tiên Giá trị Sản phẩm:** **`P0` (Chống hỏng DB khi `stop` & chống nhầm PID)** / **`P1` (Kích hoạt cửa sổ không cần Accessibility TCC & giữ phiên Keychain)**
- **Liên kết:** `F-MAC-02`, `F-MAC-03`, `F-MAC-04` | `TC-H4` | `IR-MAC-02`, `IR-MAC-03`
- **Trạng thái bằng chứng hiện tại:** Lỗi bắn `SIGTERM` ở dòng `391` trước vòng chờ dòng `397`, lỗi gọi `System Events` dòng `342`, thiếu kiểm tra `bundleIdentifier` dòng `193` và cơ chế `SecureCipher` (`secure_ks_<hash>`) là `Confirmed by source code`; hành vi giữ phiên thực tế trên Mac là `Author claim` / `Not tested`.
- **Cách kiểm chứng an toàn:** Chạy tự động `TC-H4` Phần 4A–4C với fixture giữ khóa `test.db` `1500ms` và tiến trình `foreign_pid` giả lập tái sử dụng PID; tùy chọn kiểm tra Keychain thủ công ở Phần 4D.

### `H5` — Giả thuyết Chuyển quyền Socket Gọi Tức thì trên macOS nhờ Cơ chế `fs.unlink` (Last-Writer-Wins)
- **Ưu tiên Giá trị Sản phẩm:** **`P2` (Điều phối tính năng phụ trợ Gọi điện trên macOS)**
- **Liên kết:** `F-MAC-05`, `F-MAC-04` | `TC-H5` | `IR-MAC-04`
- **Trạng thái bằng chứng hiện tại:** Nhánh `!g` gọi `fs.unlink("/tmp/socketzalo*2021")` tại `main.js:314800` là `Confirmed by source code`; quan sát chiếm socket là `Author claim`, chuyển giao qua lại có kiểm soát là `Not tested`.
- **Cách kiểm chứng an toàn:** Chạy tự động `TC-H5` trên cặp socket tổng hợp `/tmp/dim-test-zcall-*` (không chạm `/tmp/socketzalo*2021` thật) kết hợp kiểm thử `inspect_call_sockets_at`.

### `H6` — Giả thuyết Đánh thức Cửa sổ Ẩn dưới System Tray Vượt Foreground Lock nhờ Handoff `second-instance` trên Windows
- **Ưu tiên Giá trị Sản phẩm:** **`P1` (Chuyển đổi & khôi phục cửa sổ hằng ngày)**
- **Liên kết:** `F-WIN-03` | `TC-H6` | `IR-WIN-01`
- **Trạng thái bằng chứng hiện tại:** Handler `second-instance` (`setAlwaysOnTop(true)` -> `focus()`) tại `main.js:3565794` và nhánh thiếu fallback Foreground Lock tại `adapter.rs:651-654` là `Confirmed by source code`; đo độ trễ `< 2000ms` với cửa sổ ẩn dưới Tray là `Inference` / `Not tested`.
- **Cách kiểm chứng an toàn:** Chạy tự động `TC-H6` với GUI fixture ẩn `SW_HIDE` phía sau một cửa sổ tiền cảnh độc lập, gọi từ tiến trình nền và xác minh `GetForegroundWindow() == target_hwnd` trong `< 2000ms`.

### `H7` — Giả thuyết Tương thích Hộp thoại Tệp qua NTFS Junction Chọn lọc & Bảo vệ Toàn vẹn `ExternalStorageGuard`
- **Ưu tiên Giá trị Sản phẩm:** **`P0` (Chống xóa xuyên Junction, chống ghi chung DB ngoài & chống Backup rỗng)** / **`P1` (UX Hộp thoại Tệp)**
- **Liên kết:** `F-WIN-04`, `F-CORE-01`, `F-CORE-02` | `TC-H7` | `IR-WIN-03`, `IR-WIN-04`
- **Trạng thái bằng chứng hiện tại:** Thiếu thư mục `Downloads` trong `layout.rs:14-29` và thiếu kiểm tra `db_path.location` khi `launch`/`backup` trong `guard.rs` & `backup.rs` là `Confirmed by source code`; hàm `unlink_reparse_points_safe` đã `Confirmed by controlled experiment` ở mức cơ bản và cần mở rộng kiểm tra canary + parity mã lỗi.
- **Cách kiểm chứng an toàn:** Chạy tự động `TC-H7` với thư mục `mock-host` chứa tệp canary kiểm tra SHA-256 và `config.json` tổng hợp kiểm tra mã lỗi `SHARED_EXTERNAL_DB_PATH` & `EXTERNAL_DB_OUTSIDE_PROFILE`.

### `H8` — Giả thuyết Sống sót `bwrap`, Preflight AppArmor Trung thực & Cô lập Cổng `29631/29632` bằng Rootless `pasta` trên Ubuntu
- **Ưu tiên Giá trị Sản phẩm:** **`P0` (8A sống sót tiến trình/an toàn `stop` & 8B AppArmor preflight không `--no-sandbox`)** / **`P1` (8A `user-dirs.dirs` & `focus`)** / **`P2` (8C cô lập cổng gọi `29631/29632` bằng `pasta`)**
- **Liên kết:** `F-UBU-01..07` | `TC-H8` (`TC-H8A`, `TC-H8B`, `TC-H8C`) | `IR-UBU-01..05`
- **Trạng thái bằng chứng hiện tại:** 4 nguồn giới hạn trên Ubuntu (VNG `Unsupported`, 5 lỗi `dim-platform-linux`, cổng cứng `29631/29632` của bản cộng đồng, và khác biệt shared netns vs `pasta` netns) đã phân định rõ từ `Confirmed by source code`; khả năng chạy song song 2 cổng `29631/29632` trong `pasta` kèm chia sẻ PipeWire là `Inference` / `Not tested`.
- **Cách kiểm chứng an toàn:** Chạy tự động bộ 3 phần `TC-H8A`, `TC-H8B`, `TC-H8C` trên Ubuntu 22.04/24.04 bằng `fixture-app.mjs` và probe TCP (không tài khoản thật, không `--no-sandbox`).

---

## Phần III — Lộ Trình Thực Thi Bắt Buộc Theo Thứ Tự Giá Trị Sản Phẩm (`P0 -> P1 -> P2`)

| Giai đoạn triển khai | Hạng mục Yêu cầu Tích hợp (`IR-*`) | Bài kiểm thử nghiệm thu (`TC-*`) | Mục tiêu mở khóa |
| :--- | :--- | :--- | :--- |
| **Đợt 1 — `P0` (An toàn dữ liệu, Chống đóng nhầm & Mở khóa Khởi chạy / Đăng nhập `L3`)** | `IR-WIN-04`, `IR-WIN-03` (phần delete guard), `IR-WIN-01` & `IR-WIN-02` (phần lọc HWND), `IR-MAC-01`, `IR-MAC-02`, `IR-MAC-03` (phần PID guard), `IR-UBU-01`, `IR-UBU-02` (phần runtime mounts), `IR-UBU-04` | `TC-H7`, `TC-H2` (Phần 2A), `TC-H1B` (**Giai đoạn 1 — `L3` Login**), `TC-H3`, `TC-H4` (Phần 4A–4B), `TC-H8` (`8A` & `8B`) | Đảm bảo 0% nguy cơ mất dữ liệu backup/xóa junction/hỏng WAL, 0% nguy cơ đóng nhầm cửa sổ/PID, khởi chạy sạch trên cả 3 OS và xác nhận đăng nhập độc lập 2 tài khoản `L3`. |
| **Đợt 2 — `P1` (Hộp thoại tệp, Giữ phiên & Chuyển cửa sổ mượt mà)** | `IR-WIN-03` (bố cục `Downloads` & opt-in junction), `IR-WIN-01` (handoff System Tray / Foreground Lock), `IR-MAC-03` (AppKit `activate`), `IR-UBU-02` (`.config/user-dirs.dirs`), `IR-UBU-03` (`focus_or_restore`) | `TC-H6`, `TC-H7` (Phần 7A), `TC-H4` (Phần 4C–4D), `TC-H8` (`8A` phần `user-dirs` & `focus`) | Hoàn thiện trải nghiệm nhắn tin, gửi/lưu tệp và chuyển đổi cửa sổ không bị chặn Foreground Lock hay đòi quyền Accessibility TCC. |
| **Đợt 3 — `P2` (Thông báo & Chẩn đoán / Điều phối Kênh Gọi điện)** | `IR-WIN-02` (chẩn đoán thụ động `PipeZCallRecv` & hướng dẫn `notiWin`), `IR-MAC-04` (`inspect_call_sockets_at` & Dock badge), `IR-UBU-05` (chẩn đoán cổng `29631/29632`, badge & hook `pasta`) | `TC-H1A`, `TC-H1B` (**Giai đoạn 2 tùy chọn — Calls**), `TC-H2` (Phần 2B), `TC-H5`, `TC-H8` (`8C`) | Minh bạch hóa quyền sở hữu kênh gọi và tối ưu thông báo sau khi nền tảng cốt lõi `P0`/`P1` đã vững chắc 100%. |
