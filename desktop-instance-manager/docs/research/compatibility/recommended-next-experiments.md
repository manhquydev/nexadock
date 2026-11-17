# Đề Xuất Hướng Giải Quyết Hợp Chuẩn & Kế Hoạch Thực Nghiệm Tiếp Theo (`recommended-next-experiments.md`)

**Nhánh nghiên cứu:** `research/desktop-compatibility`  
**Nguyên tắc thiết kế:** Tuyệt đối không chứa mã khai thác, không vá cơ chế xác thực/chữ ký của Zalo, không inject DLL/hook kernel, không tắt Windows Smart App Control hay Ubuntu AppArmor, và không mô phỏng trái phép máy chủ/endpoint của VNG. Mọi đề xuất đều nhắm tới thu hẹp thực chất giới hạn sản phẩm bằng các cơ chế hợp lệ của từng hệ điều hành.

---

## Phần I — Phân Tích & Hướng Giải Quyết Chi Tiết Cho 8 Giới Hạn Kỹ Thuật Cốt Lõi

---

### Giới hạn 1: Xung đột kênh IPC cố định của Gọi thoại & Gọi video (`PipeZCall*` / `/tmp/socketzalo*` / `29631-29632`)

1. **Nguyên nhân có bằng chứng:**
   - Trong `app.asar/main-dist/main.js:311828..317500` (module `"3Zc2"`), Zalo gán cứng tên kênh IPC toàn cục: `\\.\pipe\PipeZCallRecv` & `PipeZCallSend` trên Windows; `/tmp/socketzalorecv2021` & `/tmp/socketzalosend2021` trên macOS; và bản cộng đồng `zalo-for-linux` (`patch-zcall-native.js:307-308`) gán cứng `127.0.0.1:29631` & `29632`.
   - Quan sát thực tế trên host Windows 11 xác nhận phiên đầu tiên đăng nhập khởi chạy sẵn `ZaloCall.exe` (`PID 15624`, `ev_9145f8bd...`) và giữ mở cả 2 pipe ngay ở trạng thái nghỉ (idle). Trên Windows, phiên thứ 2 bị `EADDRINUSE` nhưng vẫn spawn `ZaloCall.exe` kết nối nhầm vào pipe server của phiên 1, làm đè biến hàm gửi `O` của phiên 1; trên macOS, phiên mở sau cùng gọi `fs.unlink` chiếm socket của phiên trước (`ducngfi` `README.md:199`).
2. **Những phần chưa biết:**
   - Trên Windows, nếu đóng êm phiên chính (hoặc phiên đang giữ `PipeZCall*`) rồi mở lại duy nhất một phiên phụ cần gọi, phiên phụ đó có khởi tạo trọn vẹn cuộc gọi 1-1 và gọi nhóm với âm thanh/hình ảnh hai chiều ổn định 100% hay không (`L4-2` chưa chạy với tài khoản thật).
   - Trên Linux (với AppImage cộng đồng trong cấu hình hợp lệ), việc chạy mỗi phiên trong một rootless network namespace riêng (`pasta` / `slirp4netns`, tạo giao diện loopback `127.0.0.1` độc lập cho mỗi phiên mà vẫn có kết nối Internet) có giúp 2 phiên lắng nghe độc lập trên `127.0.0.1:29631/29632` hay không.
3. **Ảnh hưởng UX:**
   - Người dùng mở nhiều tài khoản sẽ thấy nút gọi ở tài khoản phụ không phản hồi, kẹt trạng thái kết nối, hoặc làm mất điều khiển cuộc gọi của tài khoản chính.
4. **Phương án tương thích hợp lệ:**
   - **Điều phối "Chế độ Ưu tiên Cuộc gọi" (Designated Call-Owner Mode) trong Manager UI & CLI:**
     - **Giám sát chủ động:** Dùng Win32 `GetNamedPipeServerProcessId` trên `\\.\pipe\PipeZCallRecv` (Windows) hoặc kiểm tra tiến trình giữ `/tmp/socketzalorecv2021` qua `lsof -U` (macOS) để hiển thị rõ huy hiệu **"Đang giữ quyền Gọi (Call Ready)"** cạnh đúng 1 profile duy nhất trên giao diện Manager!
     - **Chuyển quyền Gọi 1-click (macOS):** Tận dụng hành vi `fs.unlink` tự nhiên của Zalo macOS — cung cấp nút *"Chuyển quyền Gọi sang tài khoản này"* thực hiện đóng êm (`terminate`) và mở lại riêng profile đó (hoặc tạm dừng các phiên phụ khác) để nó nắm `/tmp/socketzalo*2021` sạch sẽ.
     - **Chuyển quyền Gọi có xác nhận (Windows):** Cảnh báo rõ khi `PipeZCallRecv` đang thuộc về một phiên khác (kể cả Zalo chính); nếu là giữa các phiên phụ do Manager quản lý, cho phép tạm dừng êm các phiên phụ đang giữ pipe rồi khởi động lại phiên cần gọi trước, sau đó mở lại các phiên kia ở chế độ chỉ nhắn tin (lưu ý: nếu phiên mở lại sau vẫn tự spawn `ZaloCall.exe` làm đè biến `O` của phiên đang giữ pipe, Manager chỉ mở duy nhất phiên cần gọi trong suốt cuộc gọi rồi khôi phục các phiên khác sau khi gọi xong!).
5. **Phương án nên loại bỏ vì không an toàn hoặc không khả thi:**
   - Sửa chuỗi `PipeZCallSend`/`socketzalorecv2021` trong `app.asar` hoặc vá nhị phân `ZaloCall.exe` (vi phạm bản quyền, làm vỡ chữ ký số VNG / `CALL_NATIVE_HASH` `verifyMd5` tại `main.js:314500`, hỏng cập nhật tự động).
   - Tiêm DLL (`Detours`/`MinHook`/`SbieDll.dll`) hoặc cài kernel driver `SbieDrv.sys` (`GPL-3.0`, bị chặn bởi Smart App Control, đòi quyền Admin).
6. **Test cần thiết:**
   - Kiểm tra API `GetNamedPipeServerProcessId` trên `\\.\pipe\PipeZCallRecv` xác định đúng PID của phiên đang nắm pipe.
   - Thực nghiệm E2E với 2 tài khoản thật trên Windows: đo chính xác xem việc mở phiên 2 sau phiên 1 có làm hỏng cuộc gọi đang rảnh của phiên 1 (do ghi đè biến `O`) hay không.
7. **Những thay đổi có thể giao cho agent Windows/macOS/Ubuntu:**
   - **Windows Agent:** Bổ sung hàm đọc owner PID của `\\.\pipe\PipeZCallRecv` trong `crates/platform-windows/src/win32_ffi.rs` (mở handle đọc thuộc tính không phá kết nối hoặc đối chiếu tiến trình con `ZaloCall.exe`) và xuất trường `call_owner_pid` trong `diagnostics`/`status`.
   - **macOS Agent:** Cập nhật `inspect_call_sockets_at` (`crates/platform-macos/src/layout.rs`) để báo cáo trạng thái sở hữu socket trong `diagnostics`.
   - **UI Agent:** Hiển thị nhãn rõ ràng profile nào đang giữ kênh gọi và cảnh báo khi thực hiện cuộc gọi ở các cửa sổ còn lại.
8. **Mức độ ưu tiên:** **P0 (Cao nhất về minh bạch sản phẩm & bảo vệ ổn định cuộc gọi phiên chính).**

---

### Giới hạn 2: Hộp thoại gửi/lưu tệp (File Dialogs) mở vào thư mục rỗng trên Windows & Linux

1. **Nguyên nhân có bằng chứng:**
   - `crates/platform-windows/src/layout.rs:14-29` đặt `USERPROFILE=<ProfileDir>` nhưng chỉ tạo `Desktop` và `Documents\Zalo Received Files`, bỏ thiếu `Downloads`, `Pictures`, `Videos`, `Music`. Khi Zalo gọi `dialog.showOpenDialog` / `showSaveDialog` (`main.js:997824`), Windows Shell mở vào cây thư mục giả lập trống rỗng.
   - `crates/platform-linux/src/layout.rs:22-35` chỉ tạo `.config/ZaloData`, `.local/share`, `.cache`, `tmp`, thiếu `Downloads`, `Documents` và tệp `.config/user-dirs.dirs` mà `patch-xdg-user-dirs.js:395-465` dựa vào.
2. **Những phần chưa biết:**
   - Khi tạo NTFS junction cho `Downloads` và `Desktop` sang thư mục thật của người dùng trên Windows 11, liệu `IFileOpenDialog` có hiển thị đầy đủ mục *Quick Access / Downloads* như phiên chính hay không.
3. **Ảnh hưởng UX:**
   - Người dùng gửi tài liệu/ảnh hoặc lưu file đính kèm phải bấm nhiều tầng ổ đĩa `C:\Users\<tên>\Downloads` rất bất tiện ở mọi lần gửi file.
4. **Phương án tương thích hợp lệ:**
   - **Mặc định an toàn (Isolated by Default):** Luôn tạo đủ các thư mục con chuẩn (`Downloads`, `Documents\Zalo Received Files`, `Desktop`, `Pictures`, `Videos`, `Music`) trên Windows và tạo thêm `.config/user-dirs.dirs` hợp lệ trên Linux để không bao giờ lỗi đường dẫn.
   - **Tùy chọn liên kết thư mục tiện lợi ( Per-Profile Opt-in Host Folder Links):** Cho phép người dùng bật tùy chọn *"Liên kết thư mục Tải về & Màn hình nền thật"* cho từng profile: tạo NTFS junction (Windows, không cần Admin) hoặc symlink (macOS/Linux) cho `Downloads`, `Desktop`, `Pictures` sang thư mục thật của user, **nhưng vẫn giữ `Documents\Zalo Received Files` tách riêng theo từng profile** (hoặc trong thư mục con `Downloads\Zalo - <Tên Profile>`) để tránh lẫn lộn tệp tự động tải về giữa các tài khoản! Bảo toàn chốt an toàn `unlink_reparse_points_safe` trước khi xóa profile.
5. **Phương án nên loại bỏ vì không an toàn hoặc không khả thi:**
   - Trỏ thẳng `USERPROFILE` về thư mục thật của người dùng (làm mất hoàn toàn khả năng cô lập `AppData\Roaming\ZaloData` trên Windows!).
   - Xóa đệ quy thư mục profile bằng lệnh thường mà không gỡ reparse point trước (nguy cơ xóa nhầm toàn bộ file `Downloads`/`Desktop` thật của người dùng — lỗi kinh điển phải chặn cứng bằng unit test).
6. **Test cần thiết:**
   - Unit test & integration test trên Windows/Linux xác nhận tạo đủ thư mục chuẩn, tạo/gỡ junction `Downloads` an toàn, và xóa profile không chạm vào bất kỳ tệp nào trong `Downloads` thật.
7. **Những thay đổi có thể giao cho agent Windows/macOS/Ubuntu:**
   - **Windows Agent:** Mở rộng `ensure_windows_profile_layout` (`crates/platform-windows/src/layout.rs` + `scripts/dim-host.ps1`) tạo đủ thư mục chuẩn và hỗ trợ cờ cấu hình liên kết junction an toàn.
   - **Ubuntu Agent:** Mở rộng `ensure_linux_profile_layout` (`crates/platform-linux/src/layout.rs`) tạo `Downloads`, `Documents`, `Pictures` và ghi tệp `.config/user-dirs.dirs`.
8. **Mức độ ưu tiên:** **P1 (Cải thiện trực tiếp trải nghiệm gửi/nhận tệp hằng ngày).**

---

### Giới hạn 3: Lỗi chọn nhầm cửa sổ phụ (`EnumWindows`) & Chặn kích hoạt cửa sổ ẩn ở Tray trên Windows

1. **Nguyên nhân có bằng chứng:**
   - Trong `crates/platform-windows/src/win32_ffi.rs:884-908`, `enum_windows_cb` nhận ngay cửa sổ `IsWindowVisible != 0` đầu tiên theo thứ tự Z-order của bất kỳ PID nào trong cây tiến trình mà không lọc class `Chrome_WidgetWin_1`, không kiểm tra `GetWindow(hwnd, GW_OWNER) == 0`, không loại bỏ `WS_EX_TOOLWINDOW`, và nhận cả cửa sổ tiêu đề rỗng. Cửa sổ thông báo nổi `notiWin` (`352x82`, `alwaysOnTop: true`, `main.js:2726777`) hoặc `ZaloCall.exe` dễ bị nhận nhầm làm cửa sổ chính!
   - Ngoài ra, khi cửa sổ chính ẩn xuống System Tray (`IsWindowVisible == 0`) hoặc khi gọi `SetForegroundWindow` từ tiến trình nền (`ui-server.mjs`), Win32 Foreground Lock chặn việc đưa cửa sổ lên trước.
2. **Những phần chưa biết:**
   - Tốc độ phản hồi trung bình của cơ chế handoff `second-instance` khi máy đang mở 4–5 phiên Zalo đã đăng nhập đầy đủ.
3. **Ảnh hưởng UX:**
   - Bấm *"Chuyển tới cửa sổ (Focus)"* không mở được cửa sổ đang ẩn dưới khay, hoặc chỉ nhấp nháy Taskbar, hoặc gửi nhầm `WM_CLOSE` vào cửa sổ popup khi đóng phiên.
4. **Phương án tương thích hợp lệ:**
   - Sửa `enum_windows_cb` trong `win32_ffi.rs` và `scripts/dim-host.ps1`:
     1. Chỉ xét PID là `main_pid` của Electron (hoặc lọc class `Chrome_WidgetWin_1`).
     2. Bắt buộc `GetWindow(hwnd, GW_OWNER) == NULL`, không mang cờ `WS_EX_TOOLWINDOW`, có tiêu đề khác rỗng (`"Zalo"`) và kích thước khung hình lớn hơn ngưỡng popup thông báo (`width >= 400 && height >= 400`).
   - Trong `focus_or_restore`: Nếu cửa sổ đang ẩn ở System Tray (`window_handle.is_none()`) hoặc sau khi gọi `focus_window(hwnd)` mà `GetForegroundWindow() != hwnd`, thực hiện gọi handoff `Zalo-<ver>\Zalo.exe` trong môi trường cô lập của profile đó (giống `ducngfi` `ZaloControl.cs:1539`) để chính Zalo kích hoạt `second-instance` (`setAlwaysOnTop(true)` -> `show()` -> `focus()`).
5. **Phương án nên loại bỏ vì không an toàn hoặc không khả thi:**
   - Dùng kỹ thuật giả lập phím bấm toàn cục (`SendInput` `Alt` key) hoặc `AttachThreadInput` xuyên tiến trình để cưỡng bức cướp quyền `SetForegroundWindow` (dễ gây kẹt hàng đợi thông điệp `USER32` nếu tiến trình đích bị treo).
6. **Test cần thiết:**
   - Tạo cửa sổ phụ giả lập (`352x82`, `WS_EX_TOPMOST | WS_EX_TOOLWINDOW`) trong `dim-test-fixture` để xác minh `find_main_window_for_pids` luôn bỏ qua popup và chọn đúng cửa sổ chính.
7. **Những thay đổi có thể giao cho agent Windows:**
   - Sửa `crates/platform-windows/src/win32_ffi.rs` (`enum_windows_cb`, `focus_window`), `crates/platform-windows/src/adapter.rs` (`focus_or_restore`), và đồng bộ sang `scripts/dim-host.ps1`.
8. **Mức độ ưu tiên:** **P0 (Sửa lỗi đúng đắn cốt lõi của điều khiển cửa sổ & đóng phiên trên Windows).**

---

### Giới hạn 4: Định danh Thông báo dùng chung (`com.vng.zalo`) trên Windows, macOS & Ubuntu

1. **Nguyên nhân có bằng chứng:**
   - `main.js:2720673` gọi cứng `app.setAppUserModelId("com.vng.zalo")`; `Zalo.lnk` (`ev_69ddf74b...`) gắn `com.vng.zalo` vào Stub `Zalo.exe` gốc. Trên macOS, `usernoted` định danh theo `CFBundleIdentifier = "com.vng.zalo"`. Trên Ubuntu (`zalo-for-linux`), `patch-notification-badge.js:859-886` phát tín hiệu đếm badge chung cho `application://zalo.desktop`.
   - Điểm sáng phát hiện từ mã nguồn (`main.js:2726777`): Zalo có sẵn hệ thống popup thông báo độc lập `notiWin` (`znotification.html` + `preload-noti.js`) thuộc quyền sở hữu riêng của từng tiến trình Electron!
2. **Những phần chưa biết:**
   - Trên Windows 11 và macOS, cấu hình mặc định của Zalo 26.10.20 sau khi đăng nhập ưu tiên bật `notiWin` hay OS Native `Notification`, và người dùng có thể chuyển đổi hoàn toàn trong mục *Cài đặt → Thông báo* của Zalo hay không.
3. **Ảnh hưởng UX:**
   - Thông báo lưu trong Windows Action Center không ghi thuộc tài khoản nào và bấm vào thông báo cũ trong Action Center có thể mở Zalo chính; số badge đỏ trên Ubuntu Dock bị ghi đè qua lại giữa các phiên.
4. **Phương án tương thích hợp lệ:**
   - Hướng dẫn cấu hình chính thức trong tài liệu & gợi ý trên UI Manager: Khuyến nghị người dùng sử dụng **chế độ popup thông báo nội bộ của Zalo (`notiWin`)** để khi bấm vào thông báo nổi luôn mở chính xác cửa sổ của tài khoản đó.
   - Trên macOS: Tận dụng badge độc lập trên từng biểu tượng Dock của từng phiên `open -n`.
5. **Phương án nên loại bỏ vì không an toàn hoặc không khả thi:**
   - Sửa `app.setAppUserModelId` bên trong `app.asar` theo từng profile hoặc nhân bản/ký lại `Zalo.app` trên macOS với `CFBundleIdentifier` giả (làm vỡ chữ ký số VNG, vỡ `SecureCipher.checkAppSigned()` tại `main.js:3562292` và hỏng quyền Keychain/TCC).
   - Ghi đè tệp `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Zalo.lnk` của người dùng chính.
6. **Test cần thiết:**
   - Kịch bản kiểm thử tương tác `L4-1` (`scripts/run-interactive-l3-checklist.ps1`): gửi tin nhắn tới Phiên Phụ A khi đang thu nhỏ, kiểm tra cả popup `notiWin` và banner hệ thống.
7. **Những thay đổi có thể giao cho agent Windows/macOS/Ubuntu:**
   - Cập nhật hướng dẫn sử dụng (`docs/user-guide/`) và bổ sung bước kiểm tra riêng cho `notiWin` vs Action Center trong script checklist `L4-1`.
8. **Mức độ ưu tiên:** **P2 (Tối ưu cấu hình & hướng dẫn UX, không can thiệp binary).**

---

### Giới hạn 5: Bốn lỗi tĩnh nghiêm trọng trong `dim-platform-macos` (`TMPDIR` > 104 byte, `SIGTERM` sớm, Accessibility TCC, thiếu kiểm tra Bundle ID & Pre-seed)

1. **Nguyên nhân có bằng chứng:**
   - `crates/platform-macos/src/layout.rs:97`: Truyền `--env TMPDIR=<profile_dir>/tmp` làm đường dẫn `<TMPDIR>/org.chromium.Chromium.XXXXXX/SingletonSocket` vượt giới hạn `104 bytes` của `sockaddr_un.sun_path` trên nhân Darwin.
   - `crates/platform-macos/src/adapter.rs:380-394`: Gửi `libc::kill(pid, libc::SIGTERM)` **ngay lập tức** sau lệnh `osascript ... terminate()` trước khi vào vòng lặp chờ `timeout_ms`, nguy cơ ngắt ngang quá trình đóng SQLite WAL!
   - `crates/platform-macos/src/adapter.rs:342-346`: Gọi `tell application "System Events"` đòi quyền Accessibility TCC không cần thiết.
   - `crates/platform-macos/src/adapter.rs:193` & `layout.rs:34-51`: Chỉ kiểm tra `kill(pid, 0)` mà không kiểm tra `bundleIdentifier == "com.vng.zalo"`; thiếu pre-seed `zmeta.json` và `migrate-config.json`.
2. **Những phần chưa biết:**
   - Thời gian trung bình để `NSRunningApplication.terminate()` đóng hoàn toàn Zalo 26.x và `ZaloHelper` trên chip Apple Silicon (`arm64`) và Intel (`x86_64`), cũng như hành vi hộp thoại Keychain ở lần mở đầu tiên.
3. **Ảnh hưởng UX:**
   - Nếu không sửa trước khi chạy trên máy Mac thật: phiên phụ có thể không khởi động được do lỗi `SingletonSocket`, hiện bảng đòi quyền Accessibility nhạy cảm khi chuyển cửa sổ, và có nguy cơ lỗi DB khi bấm Stop!
4. **Phương án tương thích hợp lệ:**
   1. Bỏ ghi đè `TMPDIR` dài trong `build_open_args` (giữ `/var/folders/...` ngắn mặc định của macOS giống `ducngfi` `ZaloControl.swift:1017`, hoặc nếu cần thư mục tạm riêng thì tạo `/tmp/dim-<profile_id>` dài ~20 ký tự với quyền `0700`).
   2. Trong `MacOsPlatformAdapter::stop`: Chờ vòng lặp `timeout_ms` cho `terminate()` hoàn tất trước; chỉ gửi `SIGTERM` khi xác nhận không còn tiến trình `--type=renderer` và không còn tệp mở trong `Database/_production/` (`lsof`).
   3. Chuyển `focus_or_restore` sang gọi `NSRunningApplication.runningApplicationWithProcessIdentifier(pid).activateWithOptions(...)` (không cần `System Events`).
   4. Bổ sung kiểm tra đường dẫn thực thi / `bundleIdentifier` cho PID đọc từ `SingletonLock`, và pre-seed `zmeta.json` + `migrate-config.json` trong `ensure_macos_profile_layout`.
5. **Phương án nên loại bỏ vì không an toàn hoặc không khả thi:**
   - Bắn `SIGKILL` (`kill -9`) vào tiến trình Zalo đang đăng nhập trên macOS.
   - Đòi người dùng cấp quyền Accessibility toàn hệ thống cho trình quản lý.
6. **Test cần thiết:**
   - Unit test kiểm tra độ dài đường dẫn socket `< 104 bytes`, kiểm tra thứ tự chờ trước `SIGTERM`, và chạy bộ kiểm thử thực tế trên máy Mac (Apple Silicon / macOS 14+).
7. **Những thay đổi có thể giao cho agent macOS:**
   - Sửa trực tiếp `crates/platform-macos/src/layout.rs` và `crates/platform-macos/src/adapter.rs` kèm unit test.
8. **Mức độ ưu tiên:** **P0 (Bắt buộc sửa trước bất kỳ thử nghiệm thực tế nào trên macOS).**

---

### Giới hạn 6: Năm lỗi thiết kế trong `dim-platform-linux` (`--die-with-parent`, thiếu bind Desktop Sockets trong `bwrap`, `focus` rỗng, `SIGKILL` sớm)

1. **Nguyên nhân có bằng chứng:**
   - `crates/platform-linux/src/layout.rs:84`: `--die-with-parent` làm `bwrap` giết ngay tiến trình Zalo khi lệnh ngắn hạn `dim-cli launch` kết thúc.
   - `crates/platform-linux/src/layout.rs:82-131`: `build_bwrap_args` thiếu bind `$XDG_RUNTIME_DIR` (Wayland, PipeWire, PulseAudio, D-Bus `gnome-keyring`/Notifications), `/tmp/.X11-unix`, `/etc/fonts`, `/etc/machine-id`, `/dev/dri`, `/dev/video*` và không tương thích với mount FUSE của `.AppImage`.
   - `crates/platform-linux/src/adapter.rs:310-318`: `focus_or_restore` là hàm rỗng `Ok(())`.
   - `crates/platform-linux/src/adapter.rs:357`: Bắn `SIGKILL` vô điều kiện sau `timeout_ms`.
2. **Những phần chưa biết:**
   - Hành vi kích hoạt lại cửa sổ từ `second-instance` trên GNOME Wayland (Ubuntu 24.04) so với phiên Xorg.
3. **Ảnh hưởng UX:**
   - Bật chế độ `bwrap` hiện tại sẽ làm ứng dụng tắt ngay lập tức hoặc không hiện giao diện/âm thanh; bấm chuyển cửa sổ trên Linux không có tác dụng.
4. **Phương án tương thích hợp lệ:**
   - Bỏ `--die-with-parent` trong `build_bwrap_args`; bổ sung bind có kiểm tra sự tồn tại (`--ro-bind-try` / `--bind-try` / `--dev-bind-try`) cho socket Wayland (`$XDG_RUNTIME_DIR/$WAYLAND_DISPLAY`), PulseAudio/PipeWire (`$XDG_RUNTIME_DIR/pulse`, `$XDG_RUNTIME_DIR/pipewire-0`), D-Bus phiên, `/tmp/.X11-unix`, `/etc/fonts`, `/etc/machine-id`, `/dev/dri`, `/dev/video0` (nếu cho phép camera). Với `.AppImage`, kiểm tra `libfuse2` khi chạy ngoài `bwrap` hoặc hỗ trợ thư mục AppDir đã giải nén sẵn của người dùng.
   - Cài đặt `focus_or_restore` trên Linux bằng cách gọi lại thực thi mục tiêu trong cùng biến môi trường XDG/HOME cô lập để kích hoạt handler `second-instance` (`showMainWindow` tại `bbb6f99e.../main.js:556-598`).
   - Loại bỏ `SIGKILL` tự động đối với phiên đang mở cơ sở dữ liệu tài khoản trong `stop()`.
5. **Phương án nên loại bỏ vì không an toàn hoặc không khả thi:**
   - Dùng Zalo Web bọc trong trình duyệt để giả làm bản Zalo Desktop Linux.
   - Đóng gói hoặc phân phối kèm bản trích xuất `app.asar` từ `zalo-for-linux`.
6. **Test cần thiết:**
   - Chạy `cargo test` + kiểm thử `dim-test-fixture` trên runner Ubuntu 22.04 và 24.04 (có và không có `bwrap` với `xvfb` / D-Bus session).
7. **Những thay đổi có thể giao cho agent Ubuntu:**
   - Sửa `crates/platform-linux/src/layout.rs` và `crates/platform-linux/src/adapter.rs` kèm kiểm thử tích hợp trên CI Linux.
8. **Mức độ ưu tiên:** **P1 (Cần thiết để chế độ `EXPERIMENTAL` trên Ubuntu hoạt động đúng thiết kế).**

---

### Giới hạn 7: Hạn chế AppArmor Unprivileged User Namespaces trên Ubuntu 23.10+ / 24.04 LTS

1. **Nguyên nhân có bằng chứng:**
   - Ubuntu 24.04 bật mặc định `kernel.apparmor_restrict_unprivileged_userns = 1`. Tệp `.AppImage` mount tạm tại `/tmp/.mount_ZaloXXXXXX/zalo` không có AppArmor profile cấp quyền `userns,`, khiến `chrome-sandbox` của Electron từ chối chạy (`bbb6f99e.../known-issues.md:108,119`).
2. **Những phần chưa biết:**
   - Khác biệt chi tiết giữa các bản cập nhật gói `apparmor` trên Ubuntu 24.04.1+ khi áp dụng profile cho đường dẫn AppImage cố định vs thư mục AppDir giải nén cố định tại `~/.local/opt/` hoặc `/opt/`.
3. **Ảnh hưởng UX:**
   - Người dùng Ubuntu 24.04 cấp đường dẫn `.AppImage` sẽ thấy tiến trình thoát ngay lúc khởi chạy nếu hệ thống chưa có cấu hình AppArmor tương ứng.
4. **Phương án tương thích hợp lệ:**
   - **Kiểm tra Preflight trung thực trong `dim-platform-linux`:** Đọc `/proc/sys/kernel/apparmor_restrict_unprivileged_userns` và `/sys/module/apparmor/parameters/enabled`. Nếu đang bật (`1`), kiểm tra xem đường dẫn binary của người dùng đã có profile AppArmor hợp lệ hay chưa; nếu chưa, trả về trạng thái chẩn đoán rõ ràng (`BLOCKED: AppArmor restricts unprivileged userns for unprofiled AppImage`) kèm mẫu tài liệu hướng dẫn quản trị viên tạo profile AppArmor chuẩn tại `/etc/apparmor.d/` (chỉ cấp đúng quyền `userns,` cho đường dẫn binary cố định đó, giữ nguyên toàn bộ Chromium renderer sandbox!).
5. **Phương án nên loại bỏ vì không an toàn hoặc không khả thi:**
   - Tự động truyền `--no-sandbox` hay `ELECTRON_DISABLE_SANDBOX=1` để ép ứng dụng nhắn tin chạy mất hoàn toàn lớp cô lập renderer của Chromium rồi đánh dấu `PASS`!
   - Yêu cầu người dùng tắt bảo vệ AppArmor toàn hệ thống (`sysctl kernel.apparmor_restrict_unprivileged_userns=0`).
6. **Test cần thiết:**
   - Kiểm thử hàm đọc sysctl AppArmor preflight trên Ubuntu 24.04 và xác minh thông báo lỗi/hướng dẫn rõ ràng.
7. **Những thay đổi có thể giao cho agent Ubuntu:**
   - Bổ sung hàm chẩn đoán `apparmor_userns_restricted()` vào `crates/platform-linux` và xuất vào báo cáo `diagnostics`.
8. **Mức độ ưu tiên:** **P1 (Đảm bảo tuân thủ bảo mật tuyệt đối trên Ubuntu 24.04).**

---

### Giới hạn 8: Rủi ro cấu hình ngoài biến môi trường (`db_path.location` ổ đĩa ngoài, `HKCU\...\Run` & Cập nhật khi đang mở nhiều phiên)

1. **Nguyên nhân có bằng chứng:**
   - Trong Cài đặt của Zalo Windows, người dùng có thể chuyển nơi lưu tin nhắn (`config.json -> db_path.location`) và tệp (`resource.location.path`) ra ngoài `ZaloData` (`ducngfi` `how-it-works.md:995`). Hiện tại `dim-core` (`crates/core/src/zalo_identity.rs`) mới đọc `db_path.location` để lấy `zalo_user_id`, nhưng `dim-profile-store` **chưa kiểm tra khi `backup`** (dẫn tới nguy cơ tạo bản sao lưu thiếu toàn bộ cơ sở dữ liệu tin nhắn!) và `dim-process-manager` **chưa chặn 2 profile cùng trỏ `db_path.location` vào một thư mục ngoài**.
   - Khi `Update.exe` (`main.js:2441895`) hoặc `Squirrel.Mac` (`main.js:3396325`) chạy trong lúc các phiên phụ vẫn mở, tệp nhị phân của phiên bản cũ bị khóa file (Windows) hoặc bị thay thế dưới chân tiến trình đang chạy (macOS).
2. **Những phần chưa biết:**
   - Liệu Zalo 26.10+ trên macOS có cho phép đổi `db_path.location` ra ngoài `ZaloData` tương tự bản Windows hay không.
3. **Ảnh hưởng UX:**
   - Người dùng tưởng đã sao lưu xong nhưng khi khôi phục sang máy khác lại mất lịch sử tin nhắn (nếu từng đổi thư mục lưu sang ổ `D:\`); hoặc cập nhật Zalo báo lỗi không xóa được thư mục cũ.
4. **Phương án tương thích hợp lệ:**
   - **Bổ sung `ExternalStorageGuard` trong `dim-core` & `dim-profile-store`:**
     1. Trước khi `launch`: Kiểm tra `db_path.location` và `resource.location.path` trong `config.json` của mọi profile đang chạy và cả Zalo chính — nếu trùng đường dẫn đích với một phiên đang mở, từ chối mở với mã lỗi rõ ràng `SHARED_EXTERNAL_DB_PATH`.
     2. Trước khi `backup`: Nếu `db_path.location` nằm ngoài thư mục `<ProfileDir>`, từ chối sao lưu mặc định (hoặc yêu cầu cờ xác nhận rõ ràng rằng DB ngoài không nằm trong gói backup) để không tạo cảm giác an toàn giả!
   - **Hỗ trợ quy trình Cập nhật Sạch (Safe Upgrade Workflow):** Thêm lệnh/nút *"Đóng êm mọi phiên phụ để Cập nhật Zalo"* trước khi người dùng bấm cập nhật trong Zalo chính.
5. **Phương án nên loại bỏ vì không an toàn hoặc không khả thi:**
   - Chặn hoặc vô hiệu hóa `Update.exe` / máy chủ cập nhật của Zalo (khiến người dùng kẹt ở phiên bản cũ có lỗ hổng bảo mật hoặc bị máy chủ Zalo ngừng hỗ trợ giao thức).
   - Tự ý sửa tệp `config.json` của Zalo để ép đổi `db_path.location`.
6. **Test cần thiết:**
   - Unit test trong `dim-profile-store` và `dim-process-manager` với `config.json` giả lập có `db_path.location` trỏ ra ngoài profile và trùng giữa 2 profile.
7. **Những thay đổi có thể giao cho agent Core/Windows:**
   - Bổ sung kiểm tra `db_path.location` / `resource.location` vào `crates/core/src/zalo_identity.rs`, `crates/process-manager/src/guard.rs`, `crates/profile-store/src/backup.rs` và `scripts/dim-host.ps1`.
8. **Mức độ ưu tiên:** **P1 (Bảo vệ an toàn dữ liệu sao lưu & chống hỏng DB).**

---

## Phần II — 8 Giả Thuyết Kỹ Thuật Có Giá Trị Kiểm Chứng Cao Nhất (High-Value Testable Hypotheses)

Dưới đây là 8 giả thuyết có tác động quyết định lớn nhất tới kiến trúc và lộ trình sản phẩm, sắp xếp theo tỷ suất giá trị kỹ thuật / chi phí kiểm chứng:

### H1. Giả thuyết "Chiếm đoạt ngược Pipe gửi (`O` Callback Hijack) trên Windows"
- **Phát biểu:** Khi Zalo chính đang đăng nhập (giữ `\\.\pipe\PipeZCallRecv` và `PipeZCallSend` ở chế độ `PIPE_UNLIMITED_INSTANCES`), việc mở và đăng nhập thêm một phiên Zalo phụ thứ hai sẽ khiến tiến trình `ZaloCall.exe` con của phiên phụ kết nối vào `serverSend` của Zalo chính và ghi đè biến hàm `O` (`main.js:315200`), làm **hỏng luôn khả năng phát cuộc gọi đi của chính Zalo chính** cho tới khi Zalo chính khởi động lại `ZaloCall.exe`.
- **Cách kiểm chứng:** Đăng nhập Tài khoản A ở Zalo chính, kiểm tra gọi thử 1 cuộc thành công -> mở và đăng nhập Tài khoản B ở Phiên phụ -> bấm gọi đi từ Tài khoản A (Zalo chính) và quan sát xem cuộc gọi của Tài khoản A có bị treo không phản hồi hay không.
- **Giá trị mở khóa:** Nếu **Đúng**, việc cảnh báo đơn thuần "phiên phụ không gọi được" là chưa đủ — Manager cần cảnh báo rõ ràng rằng mở phiên phụ thứ 2 có thể ảnh hưởng tới cuộc gọi của phiên chính và cung cấp nút *"Khôi phục cuộc gọi cho phiên chính"* (đóng phiên phụ + khởi động lại `ZaloCall.exe` / phiên chính).

### H2. Giả thuyết "Định tuyến chuẩn xác của Popup thông báo nội bộ `notiWin` trên Windows"
- **Phát biểu:** Khi bật chế độ thông báo nổi của Zalo (`znotification.html` / `preload-noti.js` tại `main.js:2726777`), mỗi phiên Zalo cô lập tự hiển thị và nhận sự kiện click nội bộ mà không đi qua `AppUserModelId("com.vng.zalo")` của Windows Action Center, nhờ đó bấm vào thông báo luôn mở đúng cửa sổ tài khoản nhận tin 100% ngay cả khi mở 3+ tài khoản cùng lúc.
- **Cách kiểm chứng:** Chạy `scripts/run-interactive-l3-checklist.ps1` (`L4-1`) với 2 tài khoản đã đăng nhập và thu nhỏ cả hai xuống khay hệ thống; gửi tin nhắn lần lượt tới từng tài khoản và bấm trực tiếp vào popup góc màn hình.
- **Giá trị mở khóa:** Chuyển trạng thái Thông báo trên Windows từ `BLOCKED` sang **`PARTIALLY_SUPPORTED` (hoạt động tốt qua in-app banner)** chỉ bằng cấu hình sẵn có trong Zalo.

### H3. Giả thuyết "Vỡ `SingletonSocket` trên macOS do `TMPDIR` vượt 104 ký tự (`UNIX_PATH_MAX`)"
- **Phát biểu:** Trên macOS 14+, lệnh `open -n` hiện tại trong `crates/platform-macos/src/layout.rs:97` truyền `--env TMPDIR=<profile_dir>/tmp` sẽ làm Chromium thất bại khi `bind()` `SingletonSocket` nếu tổng đường dẫn vượt 104 bytes; bỏ ghi đè `TMPDIR` (giữ `/var/folders/.../T/` mặc định của macOS) sẽ giúp Chromium tạo `SingletonLock` thành công mà vẫn giữ cô lập 100% `Library/Application Support/ZaloData`.
- **Cách kiểm chứng:** Chạy thử nghiệm 2 trường hợp độ dài `<profile_dir>` (ngắn `< 30` ký tự vs thực tế `~95` ký tự trong `~/Library/Application Support/MultiZaloDesktop/Profiles/...`) trên máy Mac thật và kiểm tra `SingletonLock` + `lsof` các tệp mở trong `/var/folders`.
- **Giá trị mở khóa:** Ngăn chặn lỗi chặn đứng (showstopper) ngay lần chạy đầu tiên của `dim-platform-macos` trên máy người dùng thật.

### H4. Giả thuyết "Chia sẻ êm khóa bọc `"Zalo Safe Storage"` trong macOS Keychain không gây hộp thoại lặp"
- **Phát biểu:** Vì `SecureCipher` (`main.js:3561081`) lưu khóa đối xứng 32-byte `secure_ks_<hash>` riêng trong từng thư mục `ZaloData` và chỉ dùng mục `"Zalo Safe Storage"` trong `login.keychain-db` làm khóa bọc chung cho cùng nhị phân `/Applications/Zalo.app` có chữ ký VNG, việc mở đồng thời 3 profile có symlink `Library/Keychains` sẽ giữ vững phiên đăng nhập qua nhiều lần khởi động lại mà không làm hỏng khóa của nhau và không hỏi mật khẩu Keychain ở mỗi lần mở.
- **Cách kiểm chứng:** Trên macOS thật, đăng nhập 2 tài khoản ở 2 profile cô lập -> đóng cả hai -> khởi động lại máy Mac -> mở lại đồng thời cả 2 profile và đếm số lần xuất hiện hộp thoại Keychain (kỳ vọng: `0` lần sau lần cấp quyền chuẩn đầu tiên của Zalo).
- **Giá trị mở khóa:** Xác nhận mô hình lưu trữ phiên không xâm lấn trên macOS đạt chuẩn sản phẩm thương mại.

### H5. Giả thuyết "Chuyển quyền cuộc gọi tức thì trên macOS nhờ cơ chế `fs.unlink` tự nhiên"
- **Phát biểu:** Trên macOS, do nhánh `!g` trong `main.js:314800` chủ động `fs.unlink("/tmp/socketzalorecv2021")` và `socketzalosend2021` khi khởi chạy module gọi, bất kỳ profile phụ nào được khởi động lại sau cùng (khi các profile khác đang rảnh hoặc tạm đóng) sẽ thực hiện được đầy đủ cuộc gọi thoại và gọi video (kèm quyền TCC Micro/Camera kế thừa từ `/Applications/Zalo.app`).
- **Cách kiểm chứng:** Mở Phiên 1 rồi mở Phiên 2 trên máy Mac thật -> thực hiện cuộc gọi thoại & video từ Phiên 2 -> sau đó đóng/mở lại Phiên 1 và thực hiện cuộc gọi từ Phiên 1.
- **Giá trị mở khóa:** Cho phép cung cấp tính năng **"Chuyển micro/cuộc gọi sang cửa sổ này"** mượt mà trên macOS mà không cần đặt lại làm Zalo chính!

### H6. Giả thuyết "Kích hoạt cửa sổ ẩn dưới System Tray không bị chặn Foreground Lock nhờ Handoff `second-instance`"
- **Phát biểu:** Trên Windows 11, gọi thực thi `Zalo-<ver>\Zalo.exe` với đúng khối biến môi trường cô lập của một profile đang chạy ngầm dưới System Tray sẽ luôn đánh thức và đưa cửa sổ đó lên tiền cảnh thành công 100% (nhờ tiến trình đang giữ khóa tự gọi `setAlwaysOnTop(true)` + `focus()` tại `main.js:3565794`), ngay cả khi lệnh được phát từ Web UI server chạy nền.
- **Cách kiểm chứng:** Ẩn 3 phiên Zalo cô lập xuống System Tray (bấm nút `X` đóng cửa sổ), mở một trình duyệt toàn màn hình đè lên, sau đó bấm nút *"Focus"* trên giao diện Web UI cho từng phiên và đo tỷ lệ hiện lên trên cùng + độ trễ (`ms`).
- **Giá trị mở khóa:** Giải quyết dứt điểm bài toán chuyển đổi cửa sổ tin cậy trên cả Tauri lẫn Signed-Host Web UI.

### H7. Giả thuyết "Tương thích trọn vẹn Hộp thoại Tệp Windows qua NTFS Junction chọn lọc (`Downloads` + `Desktop`)"
- **Phát biểu:** Khi tạo NTFS junction cho `Downloads` và `Desktop` trỏ về thư mục thật của user nhưng giữ `Documents\Zalo Received Files` là thư mục thật tách riêng bên trong `<ProfileDir>`, hộp thoại gửi/lưu tệp của Zalo sẽ truy cập tức thì tệp của người dùng trong khi tệp tự động tải về của từng tài khoản vẫn tách biệt 100% và sao lưu/xóa profile không bao giờ đi xuyên qua junction.
- **Cách kiểm chứng:** Bật junction `Downloads`/`Desktop` trên 2 profile Windows -> gửi tệp từ `Downloads`, nhận tệp tự động vào `Zalo Received Files` -> chạy `backup` và `delete` trên 1 profile -> kiểm tra toàn vẹn `Downloads` thật của host.
- **Giá trị mở khóa:** Đạt điểm cân bằng tối ưu giữa tiện lợi UX và cô lập dữ liệu tệp đính kèm.

### H8. Giả thuyết "Cô lập hoàn toàn cổng gọi `29631/29632` trên Ubuntu bằng Rootless Network Namespace (`pasta`) + AppArmor Profile đường dẫn cố định"
- **Phát biểu:** Trên Ubuntu 24.04 LTS có cài đặt một AppArmor profile hợp chuẩn tại `/etc/apparmor.d/` cho đường dẫn cố định của AppImage do người dùng cung cấp (không dùng `--no-sandbox`), việc bọc mỗi phiên trong không gian mạng rootless riêng (`pasta` tạo loopback `lo` riêng cho từng phiên nhưng vẫn chuyển tiếp UDP/TCP ra Internet) kết hợp bind socket PipeWire (`$XDG_RUNTIME_DIR/pulse/native`) sẽ cho phép nhiều phiên khởi tạo độc lập cổng `127.0.0.1:29631/29632` mà không gặp lỗi `EADDRINUSE`!
- **Cách kiểm chứng:** Trên máy ảo Ubuntu 24.04 LTS, cấu hình AppArmor profile chuẩn cho 1 AppImage kiểm thử -> khởi chạy 2 phiên trong 2 namespace `pasta` riêng cùng bind socket `pipewire-pulse` -> kiểm tra `ss -tlnp` trong từng namespace đều lắng nghe `127.0.0.1:29631/29632` thành công và phát/thu âm qua PipeWire đồng thời.
- **Giá trị mở khóa:** Chứng minh rằng ngay cả giới hạn cổng cứng trên Linux cũng giải quyết được sạch sẽ bằng cơ chế namespace chuẩn của nhân Linux mà không cần sửa mã ứng dụng hay tắt AppArmor.
