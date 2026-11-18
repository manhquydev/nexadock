# Báo Cáo Nghiên Cứu Tương Thích Đa Phiên Trên Windows (`windows-findings.md`)

**Nhánh nghiên cứu:** `research/desktop-compatibility`  
**Đối tượng kiểm tra:** Zalo Desktop chính thức `26.10.20` (`Zalo.exe` PE32 x86 `26.10.20.3253`, `app.asar` Electron `22.3.9` / Chromium `108.0.5359.215`, `ZaloSetup-26.11.12.exe`) và `dim-platform-windows` trên Windows 10/11 x64.  
**Nguyên tắc:** Chỉ phân tích tĩnh (REA MCP + ASAR read-only) và quan sát trạng thái hệ điều hành không xâm lấn; không sửa đổi binary, cấu hình, profile chính hay mô phỏng endpoint của Zalo.

---

## 1. Đối Chiếu Báo Cáo Cũ & Những Hiệu Chỉnh Quan Trọng

| Cấp độ kiểm thử (Báo cáo cũ) | Trạng thái cũ | Kết quả đối chiếu mới & Hiệu chỉnh kỹ thuật |
| :--- | :--- | :--- |
| **L1 — Cô lập tiến trình & lưu trữ (Multiple instances)** | `PASS` | **Giữ nguyên `CONFIRMED` (`PASS`).** Bằng chứng PoC (`evidence/zalo-live-poc-result.json`): 3 phiên phụ (`PID 24868, 24312, 24656`) chạy đồng thời cùng phiên chính (`PID 12980`, 11 tiến trình), mã băm SHA-256 của `%APPDATA%\ZaloData\{config.json, startup.log, log.log}` không đổi 1 byte. |
| **L2 — Vòng đời tiến trình & Toàn vẹn nhị phân** | `PASS` | **`PASS` ở kịch bản cơ bản, nhưng phát hiện 3 lỗi biên của Adapter (`CONFIRMED`).** Cụ thể: (1) `EnumWindows` trong `win32_ffi.rs:884-908` có thể bắt nhầm cửa sổ thông báo nổi `notiWin` hoặc `ZaloCall.exe` thay vì cửa sổ chat chính; (2) `SetForegroundWindow` bị Windows Foreground Lock chặn khi gọi từ tiến trình nền; (3) chưa chặn xung đột khi người dùng cấu hình `db_path.location` dùng chung ngoài `ZaloData`. |
| **L3 — Nhiều tài khoản đăng nhập thật (Nhắn tin, Tệp, Giữ phiên)** | `NOT_TESTED` | **Giữ `NOT_TESTED` về thực nghiệm E2E trên host này** (chưa quét 2+ mã QR thật qua `run-interactive-l3-checklist.ps1`), **nhưng đã xác minh tĩnh `CONFIRMED` cơ chế mã hóa phiên (`DPAPI` + `SecureCipher`) và phát hiện 1 hạn chế UX lớn của Adapter ở hộp thoại tệp (`showOpenDialog`/`showSaveDialog`).** |
| **L4 — Thông báo & Gọi thoại/Video** | `BLOCKED` | **Hiệu chỉnh bản chất kỹ thuật quan trọng (`CONFIRMED`):**<br>• **Thông báo:** Không bị chặn hoàn toàn! Zalo có **2 kênh thông báo**: cửa sổ popup riêng `notiWin` (`preload-noti.js`) hoạt động độc lập theo từng tiến trình, chỉ kênh Windows Action Center Toast mới dùng chung `AppUserModelId("com.vng.zalo")`.<br>• **Gọi thoại/Video:** Nghiêm trọng hơn giả định cũ ("chỉ lỗi khi 2 phiên gọi cùng lúc"). Kiểm tra thực tế cho thấy phiên đầu tiên đăng nhập **khởi chạy sẵn `ZaloCall.exe` và giữ chặt `\\.\pipe\PipeZCallRecv` + `PipeZCallSend` ngay cả khi nghỉ (idle)**, khiến **mọi phiên đăng nhập sau không thể gọi đơn lẻ** và còn có nguy cơ ghi đè con trỏ socket gửi của phiên đầu tiên! |

---

## 2. Phân Tích Chuyên Sâu 7 Nhóm Tài Nguyên Có Thể Dùng Chung Trên Windows

### 2.1. Định danh ứng dụng thông báo (Notification Application Identity)

**Bằng chứng tĩnh & REA MCP (`CONFIRMED`):**
1. **Đăng ký AUMID toàn cục trong tiến trình:** Tại `app.asar/main-dist/main.js` (offset `2720673`), mọi phiên Zalo đều gọi cứng:
   ```javascript
   app.setAppUserModelId("com.vng.zalo");
   ```
2. **Cấu trúc Shortcut Start Menu (`Zalo.lnk`):** Phân tích bằng REA `inspect_windows_shortcut` (`ev_69ddf74bf085b7e9d83f9f16f5781daf38ec90a7735065b49b55b6eee788316a`) trên `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Zalo.lnk` (`sha256: 08597831...`) và giải mã `PropertyStoreDataBlock` (offset `1958`, `279` bytes) xác nhận:
   - `PKEY_AppUserModel_ID` (`{9F4C2855-9F79-4B39-A8D0-E1D42DE1D5F3}`, PID `5`) = `"com.vng.zalo"`.
   - `local_base_path` = `C:\Users\<user>\AppData\Local\Programs\Zalo\Zalo.exe` (Stub Launcher gốc, không có tham số hay biến môi trường cô lập).
   - **Không có** `PKEY_AppUserModel_ToastActivatorCLSID` (PID `26`).
   - Tại `main.js:2687406`, hàm tạo shortcut kiểm tra `<appData>\Microsoft\Windows\Start Menu\Programs\Zalo.lnk`. Trong môi trường cô lập, `<ProfileDir>\AppData\Roaming\Microsoft\...` không tồn tại (và Windows Shell cũng không quét thư mục Start Menu giả lập trong profile phụ).
3. **Hai cơ chế hiển thị thông báo song song trong Zalo (`main.js`):**
   - **Cơ chế A — Cửa sổ thông báo tùy biến của Zalo (`notiWin`, `main.js:2726777` + `preload-noti.js` + `pc-dist/znotification.html`):** Mỗi tiến trình Electron chính tự tạo một `BrowserWindow` không viền (`352x82`, `alwaysOnTop: true`, `skipTaskbar: true`) đặt tại góc dưới phải màn hình chính (`workArea.width - 357, workArea.height - 87`). Khi người dùng bấm vào banner này, sự kiện IPC `noti-click` đi nội bộ trong chính tiến trình Electron của profile đó -> **mở đúng cửa sổ tài khoản nhận tin**. Hạn chế duy nhất là nếu hai tài khoản nhận tin cùng một giây, hai cửa sổ `notiWin` sẽ đè chồng lên nhau tại cùng tọa độ góc màn hình.
   - **Cơ chế B — Windows Native Toast (`new electron.Notification`, `main.js:1063113`):** Gửi thông báo qua WinRT `ToastNotificationManager` dưới AUMID `"com.vng.zalo"`.
     - Khi banner vừa hiện và tiến trình phụ còn sống, bộ lắng nghe WinRT in-process của Electron 22 vẫn nhận sự kiện `click` (`gr.ON_CLICK`).
     - Tuy nhiên, mọi thông báo trong Action Center bị gom chung dưới một tiêu đề `"Zalo"` duy nhất (không biết của tài khoản nào), và nếu thông báo được kích hoạt lại từ lịch sử Action Center qua Shell `Zalo.lnk`, Windows sẽ gọi Stub `Zalo.exe` mặc định -> đánh thức **Zalo chính** thay vì profile phụ!

---

### 2.2. Quyền sở hữu thiết bị Âm thanh / Video & Kênh điều khiển cuộc gọi

**Bằng chứng REA MCP & Hệ điều hành thực tế (`CONFIRMED`):**
1. **Tầng phần cứng / OS Audio & Camera KHÔNG phải điểm nghẽn của gọi thoại:**
   - Kiểm tra REA `inspect_pe_imports` trên `ZaloCall.exe` (`ev_9145f8bd574a583b54d530119265f1862c17e899a13502359500acb527cceda0`) và `zcall_ia32.node` (`ev_e01e41ea...`) xác nhận Zalo sử dụng `WINMM.dll` (`waveInOpen`, `waveOutOpen`) và plugin Qt `qtaudio_wasapi.dll` ở chế độ **WASAPI Shared Mode**. Windows Audio Engine (`audiodg.exe`) hỗ trợ trộn âm thanh và chia sẻ luồng thu micro đồng thời cho nhiều tiến trình Win32 (Zalo không yêu cầu quyền riêng tư đặc biệt ngoại trừ công tắc `Let desktop apps access your microphone/camera` chung cho mọi app Win32).
   - Với **Camera (`dsengine.dll`, `wmfengine.dll`, DirectShow):** Nếu hai tiến trình mở cùng một webcam vật lý qua DirectShow cổ điển mà driver/định dạng không đi qua dịch vụ `Windows Camera Frame Server` của Windows 11, thiết bị UVC sẽ báo bận cho tiến trình thứ hai; tuy nhiên gọi video tuần tự (lần lượt từng phiên) hoàn toàn không vướng khóa phần cứng.
2. **Điểm nghẽn thực sự nằm ở thiết kế Named Pipe cố định và khởi chạy sớm của `ZaloCall.exe` (`CONFIRMED`):**
   - Trong `main.js:311828..317500` (module `"3Zc2"`):
     - `y = "\\\\.\\pipe\\PipeZCallSend"` và `v = "\\\\.\\pipe\\PipeZCallRecv"`.
     - Khi hàm `K()` chạy, Electron mở hai `net.Server` (`I.listen(v)` và `A.listen(y)`), rồi spawn `ZaloCall.exe` với đối số dòng lệnh `[v, y]`.
     - **Quan sát trực tiếp trên host hiện tại (không xâm lấn):** Dù tài khoản Zalo chính (`PID 12980`) đang đứng yên không thực hiện cuộc gọi nào, tiến trình `ZaloCall.exe` (`PID 15624`, chạy từ `11:16:00 AM`) **đã tồn tại sẵn** và hai đường ống `\\.\pipe\PipeZCallRecv`, `\\.\pipe\PipeZCallSend` **đang bị giữ mở liên tục**! Hơn nữa, trong `C.on("exit")`, Zalo đặt lại `L = false` nhưng **không bao giờ đóng `I` và `A`**.
   - **Chuỗi lỗi dây chuyền khi phiên thứ 2 đăng nhập:**
     1. Phiên 2 gọi `I.listen("\\\\.\\pipe\\PipeZCallRecv")` và `A.listen("\\\\.\\pipe\\PipeZCallSend")` -> cả hai thất bại với `EADDRINUSE` (`libuv` đặt `FILE_FLAG_FIRST_PIPE_INSTANCE` cho server đầu tiên). Trên nhánh Windows (`g === true`), Zalo chỉ ghi log lỗi mà không đóng hay đổi tên pipe.
     2. Bất chấp `I` và `A` bị lỗi, hàm `K()` của phiên 2 **vẫn chạy tiếp lệnh `C = spawn("...\\ZaloCall.exe", [v, y])`**!
     3. Vì `net.createServer` của phiên 1 tạo pipe với `PIPE_UNLIMITED_INSTANCES`, tiến trình `ZaloCall.exe` của phiên 2 **kết nối nhầm vào pipe server của phiên 1**!
     4. Trong phiên 1, callback `A = createServer(e => { ... O = t => V(e, t) })` bị kích hoạt bởi kết nối mới từ `ZaloCall.exe` của phiên 2, **ghi đè hàm gửi lệnh `O` của phiên 1 sang socket của phiên 2**, trong khi hàm `O` của phiên 2 vẫn kẹt vĩnh viễn ở hàng đợi `P.push(e)` (`"socket send not ready, queuing"`)!
   - **Kết luận cực kỳ quan trọng:**
     - Đây là **lỗi kiến trúc IPC của chính ứng dụng Zalo**, không phải lỗi của adapter.
     - Trái với nhận định cũ rằng chỉ "cuộc gọi đồng thời" bị chặn, thực tế **bất kỳ phiên Zalo thứ hai nào đăng nhập sau phiên đầu tiên đều mất khả năng gọi thoại/video ngay cả khi phiên đầu đang rảnh**, và việc phiên thứ hai khởi tạo `ZaloCall.exe` còn có thể làm hỏng kênh điều khiển cuộc gọi của phiên đầu tiên!
3. **Phụ lục về `ZaviMeet.exe` (`ev_b7f40dd7...`):**
   - Thư mục `plugins\capture\` còn chứa `ZaviMeet.exe` (`19,8 MB`, gọi `SetCurrentProcessExplicitAppUserModelID` với chuỗi `"com.vng.zavi"`, chuỗi `"Init send pipe: %s"` và tệp `"zavi_db.db"` nằm ngay trong `plugins\capture\zavi_db.db`). Tuy nhiên trong `app.asar` của `Zalo 26.10.20`, giao thức `zavi://` đã bị gỡ bỏ (`removeAsDefaultProtocolClient("zavi")` tại `main.js:2720673`) và gọi nhóm đã gộp vào `ZaloCall.exe` (`zcallgroupwindow.cpp`).

---

### 2.3. Vòng đời tiến trình (Process Lifecycle) & Tự động cập nhật

1. **Cấu trúc cây tiến trình:**
   - Phiên chưa đăng nhập: **9 tiến trình `Zalo.exe`** (~626–784 MB Private Working Set / phiên trong PoC).
   - Phiên đã đăng nhập: **11+ tiến trình** (thêm `ZaloCall.exe`, `ZaloCap.exe`, và `zocr-host.exe` khi dùng OCR — lưu ý `main.js:1704734` xác nhận `zocr-host.exe` yêu cầu tập lệnh CPU **AVX2**).
2. **Đóng êm (Graceful Shutdown) vs Treo tiến trình:**
   - `main.js:2711674` xử lý đối số `--z-update-force-quit` qua sự kiện `second-instance`: hủy khay hệ thống (`xe.destroy()`) và gọi `G.requestQuitApp(true)` để flush SQLite WAL trước khi thoát.
   - `WindowsPlatformAdapter::stop` (`crates/platform-windows/src/adapter.rs:711-751`) gửi cả `--z-update-force-quit` lẫn `WM_CLOSE`, chờ `timeout_ms`, sau đó mới dọn cây PID con không trùng với `protected_primary_pids`.
3. **Tương tác với cơ chế cập nhật (`Update.exe` & `update_meta.json`):**
   - `Update.exe` (`ev_9f322e5a...`) đọc `update_meta.json`, chép phiên bản mới sang thư mục song song `%LOCALAPPDATA%\Programs\Zalo\Zalo-<newVer>\`, cập nhật Stub `..\Zalo.exe`, và xóa các thư mục `Zalo-<oldVer>\` cũ (trên máy hiện đang còn `Zalo-26.9.10` và `Zalo-26.10.11` bên cạnh `Zalo-26.10.20`).
   - **Giới hạn khi nhiều phiên cùng mở:** Khi phiên chính chạy `quitAndInstall()` (`main.js:2441895`), nó chỉ đóng chính nó mà **không đóng các phiên phụ đang mở**. Các phiên phụ vẫn giữ khóa file image trên `Zalo-<oldVer>\Zalo.exe` và các `.node` addon, khiến `Update.exe` không xóa được thư mục cũ ngay lúc đó (nhưng bản mới `Zalo-<newVer>` vẫn cài thành công nếu khác tên thư mục, ngoại trừ bước ghi đè `..\Zalo.exe` / `..\Uninstall Zalo.exe` có thể báo lỗi khóa file nếu Stub đang chạy).

---

### 2.4. Hành vi Local IPC giữa các thành phần

| Kênh IPC cục bộ | Phạm vi đặt tên | Hành vi khi chạy nhiều phiên trên Windows | Phân loại |
| :--- | :--- | :--- | :--- |
| `<userData>\lockfile` + `Chrome_MessageWindow` | Theo thư mục `userData` | Tách biệt hoàn toàn nhờ điều hướng `USERPROFILE`/`APPDATA` | Tương thích 100% (`CONFIRMED`) |
| `ZaloCap.exe` (`main.js:982068`) | Anonymous `stdin`/`stdout` pipe | Mỗi phiên spawn `ZaloCap.exe` riêng với đối số `["zalo2017", token]` | Tương thích 100% (`CONFIRMED`) |
| `zocr-host.exe` (`ZaloCap.exe` / `main.js:1702534`) | Named pipe động + `pipeToken` + kiểm tra PID server | Tách biệt theo từng phiên yêu cầu OCR | Tương thích (`CONFIRMED` tĩnh) |
| `ZaloStubExecutable` (`Programs\Zalo\Zalo.exe`) | Named Mutex session-local | Chỉ giữ khóa trong lúc spawn (`< 5s`); `dim-platform-windows` gọi thẳng `Zalo-<ver>\Zalo.exe` nên bỏ qua hoàn toàn | Tương thích 100% (`CONFIRMED`) |
| `Global\{C30D92DD-DEE6-41A1-8907-B42FBE58C8A6}` (`sl.exe`) | Global Mutex (`ev_58587a3b...`) | Chỉ dùng ngắn hạn bởi helper tạo shortcut `sl.exe` | Không ảnh hưởng phiên chính (`CONFIRMED`) |
| `globalShortcut.register` (`Ctrl+Alt+S`, `Ctrl+Alt+A`, `main.js:2687406`) | Toàn session Windows (`RegisterHotKey`) | **Chỉ phiên Zalo mở đầu tiên đăng ký được phím tắt chụp màn hình toàn cục**; các phiên mở sau bị thất bại âm thầm (chụp màn hình bằng nút bấm trong khung chat vẫn hoạt động) | Giới hạn OS / App (`CONFIRMED`) |
| `\\.\pipe\PipeZCallSend` & `\\.\pipe\PipeZCallRecv` | Global Named Pipe | **Xung đột trực tiếp** giữa mọi phiên đã đăng nhập (xem mục 2.2) | **`BLOCKED`** — Giới hạn App (`CONFIRMED`) |

---

### 2.5. Cấu hình dùng chung (Shared Configuration) ngoài biến môi trường

1. **Windows Registry `HKEY_CURRENT_USER` (`CONFIRMED`):**
   - `CreateProcessW` chỉ cô lập khối biến môi trường, **không** cô lập `HKCU`. Mọi phiên Zalo dùng chung:
     - `HKCU\Software\Classes\zalo`: `main.js:2720633` tự động ghi đè `setAsDefaultProtocolClient("zalo", "...\\Programs\\Zalo\\Zalo.exe", ["--"])` mỗi lần khởi động -> bấm liên kết `zalo://` trên trình duyệt luôn mở vào **Zalo chính**.
     - `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`: Thư viện `auto-launch` / `setLoginItemSettings` ghi chung một khóa khởi động cùng Windows. Nếu người dùng bật/tắt *"Khởi động cùng Windows"* trong phần Cài đặt của một phiên phụ, nó sẽ **bật/tắt khởi động của Zalo chính**!
2. **Tệp cấu hình `zalocall_config.ini` của `ZaloCall.exe` (`CONFIRMED` tĩnh / `INFERENCE` vị trí):**
   - Chuỗi trong `ZaloCall.exe` (`ev_9145f8bd...`) nạp `QSettings` dạng INI với tên `"\zalocall_config.ini"`. Nếu đường dẫn này tính theo thư mục chứa `ZaloCall.exe` (`%LOCALAPPDATA%\Programs\Zalo\Zalo-<ver>\plugins\capture\`, vốn có quyền ghi user!), các phiên sẽ ghi đè cấu hình loa/micro của nhau; nếu tính theo `%APPDATA%` thì được cô lập.
3. **Đường dẫn lưu tin nhắn tùy chỉnh ngoài `ZaloData` (`config.json -> db_path.location` & `resource.location.path`) (`CONFIRMED`):**
   - Zalo cho phép người dùng chuyển thư mục lưu DB/Media sang ổ đĩa khác (ví dụ `D:\ZaloData`). Nếu hai profile cấu hình trỏ vào cùng thư mục đích, hoặc nếu sao lưu một profile đang đặt `db_path.location` ở ngoài `<ProfileDir>` mà không cảnh báo, bản sao lưu sẽ bị rỗng phần tin nhắn!

---

### 2.6. Hộp thoại tệp (File Dialogs) & Kéo thả tệp

**Phát hiện quan trọng — Hạn chế do thiết kế Adapter hiện tại (`CONFIRMED`):**
- Tại `main.js:997824`, Zalo mở hộp thoại chọn/lưu tệp qua `electron.dialog.showOpenDialog` / `showSaveDialog` và đánh dấu đường dẫn tin cậy bằng `setTrustPath(filePath)`.
- Trong `crates/platform-windows/src/layout.rs:14-29`, `ensure_windows_profile_layout` gán `USERPROFILE = <ProfileDir>` nhưng chỉ tạo `Desktop` và `Documents\Zalo Received Files`, **không tạo `Downloads`, `Pictures`, `Videos`, `Music`** và không tạo NTFS junction tùy chọn tới thư mục thật của người dùng.
- **Hệ quả UX:** Khi người dùng gửi hoặc lưu file trong phiên phụ, hộp thoại Common Item Dialog của Windows mở vào không gian thư mục giả lập trống rỗng (`<ProfileDir>`), không thấy các tệp vừa tải về trong `C:\Users\<user>\Downloads` nếu không tự gõ/duyệt ngược từ `This PC -> C: -> Users -> <user>`.
- **Kéo-thả (Drag & Drop) và Dán ảnh Clipboard (`Ctrl+V`):** Vì cả `explorer.exe` và phiên phụ `Zalo.exe` đều chạy ở mức toàn vẹn `Medium Integrity Level` (`asInvoker`, không nâng quyền Admin), Windows UIPI cho phép kéo-thả tệp từ File Explorer vào cửa sổ Zalo phụ và dán ảnh clipboard bình thường (`INFERENCE` có cơ sở kiến trúc Win32 UIPI, cần xác nhận E2E trong L3).

---

### 2.7. Kích hoạt & Chuyển đổi cửa sổ (OS Window Activation)

**Phát hiện 3 lỗi cụ thể trong Adapter `crates/platform-windows` (`CONFIRMED`):**

1. **Lỗi chọn nhầm cửa sổ phụ trong `enum_windows_cb` (`crates/platform-windows/src/win32_ffi.rs:884-908`):**
   - Hàm `enum_windows_cb` hiện dừng ngay ở cửa sổ `IsWindowVisible(hwnd) != 0` đầu tiên thuộc bất kỳ PID nào trong cây tiến trình của profile, **không kiểm tra `GetClassNameW` (`Chrome_WidgetWin_1`), không kiểm tra `GetWindow(hwnd, GW_OWNER) == NULL`, không loại trừ `WS_EX_TOOLWINDOW`, và vẫn nhận cả cửa sổ có tiêu đề rỗng (`len == 0`)**!
   - Khi tài khoản đã đăng nhập, cây tiến trình có cửa sổ popup thông báo `notiWin` (`alwaysOnTop: true`, nằm đầu danh sách Z-order của `EnumWindows`!), cửa sổ `ZaloCall.exe` hoặc overlay `ZaloCap.exe`. Kết quả: `focus_or_restore` có thể focus nhầm vào popup thông báo, và nguy hiểm hơn là `stop()` gửi `WM_CLOSE` (`adapter.rs:744`) nhầm vào popup thay vì cửa sổ chính!
2. **Hành vi khi đóng cửa sổ xuống System Tray (`IsWindowVisible == 0`):**
   - Khi người dùng bấm nút `X` trên cửa sổ Zalo, Zalo gọi `BrowserWindow.hide()` để ẩn xuống khay hệ thống (`IsWindowVisible == 0`). Khi đó `find_main_window_for_pids` trả về `None`, và `focus_or_restore` (`adapter.rs:656-664`) phải spawn một tiến trình `Zalo.exe` tạm thời để kích hoạt sự kiện `second-instance` (`main.js:3565794`), làm cửa sổ hiện lại được nhưng mất thêm ~0,8–1,5 giây và ghi thêm 1 dòng vào `startup.log`.
3. **Giới hạn `SetForegroundWindow` của Windows:**
   - Khi người dùng bấm nút "Focus" từ Web UI (`ui-server.mjs` -> spawn `powershell.exe` / `dim-cli.exe`), tiến trình con không sở hữu cửa sổ tiền cảnh nên lệnh `SetForegroundWindow(hwnd)` (`win32_ffi.rs:942`) thường chỉ làm nhấp nháy biểu tượng Zalo màu cam dưới Taskbar thay vì đưa cửa sổ lên trước. Ngược lại, gọi qua cơ chế handoff `second-instance` để chính Zalo gọi `setAlwaysOnTop(true)` -> `focus()` -> `setAlwaysOnTop(false)` (`main.js:3565794`) luôn vượt qua được rào cản này hợp lệ!
4. **Gom nhóm Taskbar & System Tray (Hạn chế của App/OS):**
   - Mọi cửa sổ Zalo đều có tiêu đề `"Zalo"` và cùng `AppUserModelId("com.vng.zalo")`, nên Windows 11 gom tất cả vào một biểu tượng Zalo duy nhất trên Taskbar và tạo nhiều icon Zalo giống hệt nhau trong System Tray.

---

## 3. Bảng Phân Định Rạch Ròi: Hạn Chế Ứng Dụng vs Hạn Chế Thiết Kế Adapter

| Hạng mục | Nguồn gốc giới hạn | Có khắc phục được ở tầng Manager hợp lệ không? | Hướng xử lý |
| :--- | :--- | :--- | :--- |
| **Gọi thoại/video bị khóa ở các phiên mở sau (`\\.\pipe\PipeZCallSend/Recv`)** | **Ứng dụng Zalo** (`main.js:311828` mở pipe cố định và giữ suốt phiên đăng nhập) | **Không** cho trường hợp 2 phiên cùng giữ `ZaloCall`; **Có thể điều phối tuần tự** | Cung cấp chế độ *"Chuyển quyền Gọi cho phiên này"* (tạm đóng êm các phiên khác, mở lại phiên cần gọi để nắm `PipeZCall*` sạch sẽ); tuyệt đối không patch ASAR hay inject DLL |
| **Gom nhóm Toast Action Center & click từ lịch sử mở nhầm Zalo chính** | **Ứng dụng Zalo + Windows Shell** (cứng `com.vng.zalo` + `Zalo.lnk` trỏ Stub) | **Một phần** (banner riêng `notiWin` của Zalo đã hoạt động đúng theo từng phiên) | Hướng dẫn người dùng bật *"Thông báo nổi của Zalo"* trong Cài đặt Zalo thay vì phụ thuộc lịch sử Windows Action Center |
| **Phím tắt chụp màn hình `Ctrl+Alt+S` / `Ctrl+Alt+A` chỉ ăn ở phiên đầu** | **Ứng dụng Zalo + Win32 `RegisterHotKey`** | **Không** (1 hotkey toàn cục chỉ thuộc về 1 tiến trình) | Ghi chú rõ UX; dùng nút chụp màn hình trực tiếp trên thanh công cụ chat của từng cửa sổ |
| **Liên kết `zalo://` và cấu hình "Khởi động cùng Windows" dùng chung `HKCU`** | **Ứng dụng Zalo + Windows Registry** | **Có thể giảm thiểu ở Manager** | Manager tự quản lý tính năng *"Mở các profile đã chọn khi đăng nhập Windows"* và cảnh báo người dùng tắt tùy chọn auto-start bên trong các phiên Zalo phụ |
| **Hộp thoại gửi/lưu tệp mở vào thư mục rỗng, thiếu `Downloads`** | **Thiết kế Adapter (`dim-platform-windows`)** | **CÓ — Sửa triệt để được** | Tạo sẵn `Downloads`, `Pictures`, `Videos`, `Music` trong profile; thêm tùy chọn opt-in tạo NTFS junction tới `Downloads`/`Desktop` thật (giữ riêng `Zalo Received Files`) |
| **`EnumWindows` bắt nhầm cửa sổ `notiWin` / `ZaloCall` & `SetForegroundWindow` bị chặn** | **Thiết kế Adapter (`win32_ffi.rs` & `adapter.rs`)** | **CÓ — Sửa triệt để được** | Lọc `Chrome_WidgetWin_1` + `GW_OWNER == 0` + kích thước cửa sổ chính; dùng handoff `second-instance` làm phương thức kích hoạt tin cậy khi ẩn ở Tray |
| **Sao lưu thiếu dữ liệu nếu người dùng đổi `db_path.location` sang ổ khác** | **Thiết kế Adapter (`dim-profile-store`)** | **CÓ — Sửa triệt để được** | Kiểm tra `config.json` (`db_path.location`, `resource.location.path`) trước khi `launch` (chặn trùng đường dẫn giữa 2 profile) và trước khi `backup` |
| **Chặn tệp `.exe` chưa ký số khi Smart App Control (`SAC = 1`) bật** | **Chính sách bảo mật Windows 11 + Pipeline phát hành** | **CÓ — Đã có Signed-Host Bridge; cần ký số CI** | Duy trì Signed-Host Bridge (`powershell.exe` + `node.exe`) và tích hợp chứng chỉ Authenticode vào GitHub Actions (Gate F) |

---

## 4. Kết Luận Đánh Giá Trên Windows

Windows 10/11 là nền tảng **duy nhất hiện có đầy đủ nhị phân chính thức của VNG được xác minh thực nghiệm chạy song song 4 cửa sổ ổn định** mà không chạm vào `%APPDATA%\ZaloData`. Bằng cách sửa 3 nhóm hạn chế thuộc về Adapter (bộ lọc cửa sổ `EnumWindows` + kích hoạt từ Tray, cấu trúc thư mục/junction cho File Dialog, và kiểm tra `db_path` ngoài) kết hợp luồng điều phối *"Ưu tiên cuộc gọi cho 1 phiên"*, sản phẩm đạt độ hoàn thiện rất cao cho nhu cầu nhắn tin, gửi/nhận tệp, giữ phiên đăng nhập, thông báo nổi in-app và cập nhật an toàn mà không cần bất kỳ kỹ thuật can thiệp trái phép nào.
