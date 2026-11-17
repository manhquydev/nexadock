# Đánh Giá Tương Thích Cập Nhật & Đa Nền Tảng (Windows / macOS / Linux)

## 1. Khả Năng Tương Thích Khi Zalo Cập Nhật Trên Windows 10/11

Qua phân tích tệp `update_meta.json`, `main.js` (offset `2441500`) và nhật ký cập nhật thực tế `%APPDATA%\ZaloData\update.log`:
1. Khi có bản mới (ví dụ cập nhật từ `26.10.11` lên `26.10.20`, hoặc cài bản `26.11.12`), Zalo tải bản cập nhật vào `%TEMP%` và gọi `Update.exe --meta-file update_meta.json --app-root <Programs\Zalo>`.
2. `Update.exe` tạo thư mục phiên bản mới `Zalo-<NewVersion>`, cập nhật stub `..\Zalo.exe` từ `ZaloExecutable.exe`, kiểm tra SHA-1 của `resources\app.asar`, và dọn thư mục cũ.
3. **Vì sao `Multi-Zalo Desktop` bền vững trước các bản cập nhật**:
   - `DiscoverOfficialInstallation()` luôn quét động mọi thư mục `Zalo-*` tại thời điểm chạy và chọn phiên bản cao nhất hợp lệ — không bao giờ hardcode đường dẫn phiên bản.
   - Dữ liệu profile nằm hoàn toàn bên ngoài thư mục cài đặt (`%LOCALAPPDATA%\MultiZaloDesktop\Profiles\`), không bị ảnh hưởng khi `Update.exe` thay thế thư mục `Zalo-*`.
4. **Rủi ro có thể gặp khi Zalo phát hành phiên bản mới trong tương lai**:
   - **Nếu VNG thay đổi tên hoặc vị trí khóa single-instance**: Nếu một phiên bản Zalo tương lai đổi sang dùng Named Mutex toàn cục dạng `Global\ZaloApp...` hoặc đổi `migration.js` bỏ `app.getPath("appData")` để gọi API đọc trực tiếp Registry `Shell Folders` bỏ qua biến môi trường, cơ chế env redirection sẽ cần bổ sung tham số hoặc điều chỉnh tại lớp `WindowsPlatformAdapter`.
   - **Khi nhiều phiên đang mở cùng lúc lúc auto-update**: Nếu nhiều instance đều bật tự động cập nhật chạy nền, tiến trình `Update.exe` có thể gặp file lock trên thư mục `Zalo-<oldVer>` khiến việc dọn thư mục cũ chưa sạch ngay (như đã quan sát thấy thư mục `Zalo-26.10.11` và `Zalo-26.9.10` còn tồn tại trên máy thử nghiệm). Khuyến nghị: đóng các phiên phụ trước khi áp dụng bản cập nhật lớn của Zalo, hoặc cập nhật thông qua phiên chính.

---

## 2. Nghiên Cứu Khả Năng Hỗ Trợ macOS (Phase 3)

Không giả định macOS dùng chung cơ chế lưu trữ với Windows. Từ bằng chứng mã nguồn trích trực tiếp trong `app.asar/main-dist/main.js` và `migration.js` (cả hai nền tảng đều được đóng gói chung trong bundle Electron của Zalo):

| Thành phần | Bằng chứng trong `main.js` / `migration.js` | Hệ quả triển khai trên macOS |
| :--- | :--- | :--- |
| **App Bundle & Helper** | `main.js:314463` & `982174`: `ZaloHelper.app/Contents/MacOS/ZaloCall` và `ZaloCap` nằm bên trong `/Applications/Zalo.app/Contents/...` | Không sửa bundle đã ký và notarized của VNG (tránh vô hiệu hóa Gatekeeper / Code Signing / Hardened Runtime). |
| **User Data Directory** | `main.js:21950` (`"darwin"===process.platform` -> `~/Library/Application Support/...`); `migration.js:514471` (`ZaloPC` -> `app.getPath("userData")/media`); `migration.js:600654` (`app.setPath("userData", path.join(app.getPath("appData"), "ZaloData"))`). | Trên macOS, `app.getPath("appData")` của Electron gọi Cocoa `NSSearchPathForDirectoriesInDomains(NSApplicationSupportDirectory, NSUserDomainMask, YES)`, thực chất trỏ về `$HOME/Library/Application Support`. |
| **Single-Instance & Launch** | `bootstrap.js` dùng `app.requestSingleInstanceLock()`. Trên macOS, nếu dùng `open -a Zalo`, LaunchServices chỉ giữ 1 instance cho mỗi bundle ID trừ khi dùng `open -n` hoặc gọi trực tiếp binary `/Applications/Zalo.app/Contents/MacOS/Zalo` kèm `HOME=<ProfileDir>`. | Mở trực tiếp binary bên trong `.app/Contents/MacOS/Zalo` với biến môi trường `HOME=<ProfileDir>` và `TMPDIR=<ProfileDir>/tmp` có khả năng tách `~/Library/Application Support/ZaloData` về `<ProfileDir>/Library/Application Support/ZaloData`. |
| **Keychain & TCC Permissions** | `EnableCookieEncryption` trên macOS dùng **macOS Keychain** (`Safe Storage`) theo tài khoản đăng nhập macOS hiện tại (cùng service name trong Keychain cho mọi instance cùng bundle ID). | Nếu nhiều instance dùng chung một Keychain item `Zalo Safe Storage` dưới cùng user macOS thì vẫn giải mã được, nhưng cần kiểm chứng thực tế trên máy macOS thật xem Keychain access prompt và Chromium `SingletonSocket` trong `<userData>` có hoạt động ổn định với `HOME` tùy biến hay không. |
| **Unix Domain Sockets (ZaloCall)** | `main.js:311822`: `y = "/tmp/socketzalosend2021"`, `v = "/tmp/socketzalorecv2021"`. Offset `314800`: khi `EADDRINUSE`, code gọi `fs.unlink(v, ...)` rồi listen lại! | **Khác biệt lớn với Windows**: trên macOS, phiên thứ 2 sẽ `unlink` socket `/tmp/socketzalorecv2021` của phiên thứ 1 nếu cả hai gọi `ZaloCall`. Cần kiểm chứng thực nghiệm trên macOS xem có làm gián đoạn cuộc gọi của phiên đầu hay không. |

**Trạng thái macOS hiện tại**: Đã có thiết kế `MacOSPlatformAdapter` và phương án kỹ thuật có cơ sở từ mã nguồn, nhưng **CHƯA KIỂM THỬ** vì môi trường hiện tại là Windows 11. Không tuyên bố hỗ trợ macOS cho đến khi chạy kiểm thử trên phần cứng macOS thực tế.

---

## 3. Đánh Giá Khả Năng Hỗ Trợ Linux (Phase 4)

Kiểm tra toàn diện mã nguồn `main.js`, `migration.js` và trang phân phối chính thức của Zalo (`https://zalo.me/pc` tham chiếu trong stub `Zalo.exe`):
1. **Có bản Zalo Desktop chính thức cho Linux không?**
   - **Không.** VNG chỉ phát hành bộ cài đặt desktop chính thức cho **Windows** (`.exe`) và **macOS** (`.dmg`).
2. **Bằng chứng trực tiếp trong mã nguồn `main-dist/main.js`**:
   - Khối khởi chạy `ZaloCall` (`offset 314392`) và `ZaloCap` (`offset 981878`) chỉ xử lý hai nhánh: `"win32" === process.platform` và nhánh `else` giả định `ZaloHelper.app/Contents/MacOS/...` của macOS (`darwin`).
   - Không có binary `ZaloCall`, `ZaloCap`, hay `zocr-host` biên dịch cho Linux ELF.
   - Dù hàm định dạng chuỗi hiển thị hệ thống và `electron-updater` có nhánh `linux` thừa hưởng từ thư viện chuẩn của Electron/Node, ứng dụng thực tế không được đóng gói hay phân phối cho Linux.
3. **Tuân thủ tiêu chí dự án**:
   - Không coi Wine, Zalo Web hay wrapper trình duyệt là native Linux support.

**Kết luận Linux**: **CHƯA HỖ TRỢ (`UNSUPPORTED`)** do không tồn tại ứng dụng Zalo Desktop chính thức cho Linux từ nhà phát triển VNG.
