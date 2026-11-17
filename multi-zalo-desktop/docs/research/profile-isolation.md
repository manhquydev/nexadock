# Nghiên Cứu Lưu Trữ Dữ Liệu & Cơ Chế Cô Lập Profile Trên Windows

## 1. Bản Đồ Vị Trí Lưu Trữ Dữ Liệu Của Zalo Desktop

Qua phân tích tĩnh module `IpzU` (`main.js` offset `1237000..1249000`), `migration.js` (offset `514200..605000`) và đối chiếu trực tiếp thư mục `%APPDATA%\ZaloData` trên máy thực nghiệm:

| Loại dữ liệu | Đường dẫn mặc định trên Windows | Cách Zalo tính đường dẫn trong mã nguồn |
| :--- | :--- | :--- |
| **UserData gốc** (Preferences, Cookies, IndexedDB, Local State, Session Storage, Cache, `lockfile`, `config.json`, `ftsm_*.db`) | `%APPDATA%\ZaloData\` | `path.join(app.getPath("appData"), "ZaloData")` + `app.setPath("userData", d)` (`migration.js:600654`) |
| **Partitions Web/Session** | `%APPDATA%\ZaloData\Partitions\zalo\` | `path.join(app.getPath("userData"), "Partitions")` |
| **Database tin nhắn & FTS** | `%APPDATA%\ZaloData\ftsm_<uid>.db` (+ `-wal`, `-shm`) | `p.getPathSync("userData")` (`main.js:755230`) |
| **Media tải về (cấu trúc mới)** | `%APPDATA%\ZaloData\media\` | `path.join(app.getPath("userData"), "media")` (`main.js:47943`) |
| **Media legacy (kiểm tra migrate)** | `%USERPROFILE%\AppData\Local\ZaloPC\` | `path.join(app.getPath("home"), "AppData", "Local", "ZaloPC")` (`migration.js:514310`) |
| **Zalo Received Files** | `%USERPROFILE%\Documents\Zalo Received Files\` | `path.join(app.getPath("home"), "Documents", "Zalo Received Files")` (`main.js:1244388`) |
| **Kiểm tra cài đặt mới** (`isNewInstall`) | `%USERPROFILE%\AppData\Roaming\ZaloData\Partitions` | `path.join(app.getPath("home"), "AppData", "Roaming", "ZaloData", "Partitions")` (`main.js:2230102`) |
| **Thư mục tạm (`Zalo Temp`, `zTemp`, `zpc-auc`)** | `%TEMP%\Zalo Temp\`, `%TEMP%\zTemp\`, `%TEMP%\zpc-auc\` | `app.getPath("temp")` (`main.js:1244200`) |
| **Cấu hình migrate ngoài ZaloData** | `%APPDATA%\zmeta.json` | `path.join(app.getPath("appData"), "zmeta.json")` (`migration.js:600854`) |
| **Cờ hoàn tất migrate ZaloPC** | `%APPDATA%\ZaloData\migrate-config.json` | `electron-store` tại `userData` với key `zalopc_m_c: true` |

---

## 2. Cơ Chế Cô Lập Profile Của Hệ Điều Hành Windows

### 2.1. Tại sao điều hướng biến môi trường hoạt động trên Windows?
Kiểm tra trực tiếp Registry khóa `HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\User Shell Folders` (giá trị thô `REG_EXPAND_SZ` đọc bằng `RegistryValueOptions.DoNotExpandEnvironmentNames`):
- `AppData` = `%USERPROFILE%\AppData\Roaming`
- `Local AppData` = `%USERPROFILE%\AppData\Local`
- `Personal` = `%USERPROFILE%\Documents`
- `Desktop` = `%USERPROFILE%\Desktop`

Khi một tiến trình con `Zalo-26.10.20\Zalo.exe` được tạo bằng Win32 `CreateProcessW` với khối biến môi trường riêng:
- `USERPROFILE = <ProfileDir>`
- `APPDATA = <ProfileDir>\AppData\Roaming`
- `LOCALAPPDATA = <ProfileDir>\AppData\Local`
- `TEMP = <ProfileDir>\AppData\Local\Temp`
- `TMP = <ProfileDir>\AppData\Local\Temp`
- `HOMEDRIVE` và `HOMEPATH` tương ứng với `<ProfileDir>`

Windows `SHGetKnownFolderPath(FOLDERID_RoamingAppData)` (được Electron/Chromium gọi trong `app.getPath("appData")`) và các API đọc `%USERPROFILE%` / `%APPDATA%` mở rộng `%USERPROFILE%` từ **chính khối biến môi trường của tiến trình con đó**. Nhờ vậy:
1. `app.getPath("appData")` trả về `<ProfileDir>\AppData\Roaming`.
2. `migration.js` gọi `app.setPath("userData", "<ProfileDir>\\AppData\\Roaming\\ZaloData")`.
3. `app.requestSingleInstanceLock()` khóa tệp `<ProfileDir>\AppData\Roaming\ZaloData\lockfile` hoàn toàn tách biệt với `%APPDATA%\ZaloData\lockfile` của người dùng chính.
4. Thư mục `Documents\Zalo Received Files`, `ZaloData\media`, `Partitions\zalo`, `Local State`, `Preferences`, `ftsm_*.db`, `Temp` đều nằm trọn trong `<ProfileDir>`.

### 2.2. Điểm tinh chỉnh bắt buộc phát hiện từ mã nguồn (`home` vs `USERPROFILE`)
Kiểm tra `main.js` và thử nghiệm `[Environment]::GetFolderPath('UserProfile')` / Node.js `os.homedir()` cho thấy:
- `app.getPath("home")` của Electron trên Windows dùng `SHGetKnownFolderPath(FOLDERID_Profile)` hoặc biến `USERPROFILE`; một số hàm trong `main.js` như `getWindowsZaloPCDir` (`offset 1246800`) và `isNewInstall` (`offset 2230102`) ghép đường dẫn `path.join(app.getPath("home"), "AppData", "Local", "ZaloPC")` hoặc `path.join(app.getPath("home"), "AppData", "Roaming", "ZaloData", "Partitions")`.
- Để đảm bảo Zalo không bao giờ kiểm tra nhầm hay đụng vào `C:\Users\<user>\AppData\Local\ZaloPC` khi chạy một profile mới, `WindowsPlatformAdapter.EnsureProfileDirectories` chủ động tạo sẵn hai tệp cấu hình rỗng không nhạy cảm trước khi khởi chạy:
  - `<ProfileDir>\AppData\Roaming\zmeta.json` với nội dung `{"failedMigrateCount":0}`
  - `<ProfileDir>\AppData\Roaming\ZaloData\migrate-config.json` với nội dung `{"zalopc_m_c":true}`
- Bằng chứng thực nghiệm (PoC & Integration Test) xác nhận: sau khi pre-seed 2 tệp trên, toàn bộ 100% file được Zalo tạo ra đều nằm gọn trong `<ProfileDir>\AppData\Roaming\ZaloData\` và hash SHA-256 của `%APPDATA%\ZaloData\startup.log`, `log.log`, `config.json` của tài khoản chính không thay đổi dù chỉ 1 byte.

### 2.3. Theo dõi trạng thái phiên không cần hook hay sửa DLL (`rstrtmgr.dll`)
Thay vì dựa vào việc quét toàn bộ tiến trình `Zalo.exe` trên hệ thống (vốn không thể phân biệt tiến trình nào thuộc profile nào nếu chỉ nhìn tên `Zalo.exe`), giải pháp sử dụng **Windows Restart Manager API (`rstrtmgr.dll`)** (`RmStartSession`, `RmRegisterResources`, `RmGetList`) truy vấn trực tiếp tệp `<ProfileDir>\AppData\Roaming\ZaloData\lockfile`:
- Hệ điều hành Windows trả về chính xác **PID của tiến trình Electron Main Process** đang giữ khóa độc quyền trên `lockfile` của profile đó.
- Từ `MainPid`, công cụ dùng `CreateToolhelp32Snapshot` duyệt cây `ParentProcessId` để thu thập chính xác tập hợp PID con thuộc riêng profile đó (GPU, Renderer, Utility, `ZaloCap`, `ZaloCall`).
- Nếu một tiến trình bị crash bất ngờ để lại `lockfile` rỗng trên đĩa, `RmGetList` trả về rỗng và công cụ nhận diện chính xác trạng thái là `Đã dừng`.
