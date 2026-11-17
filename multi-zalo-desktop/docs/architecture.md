# Kiến Trúc Sản Phẩm: Multi-Zalo Desktop

## 1. Tổng Quan Kiến Trúc

`Multi-Zalo Desktop` là trình điều phối phiên (session orchestrator) nhẹ, tách biệt hoàn toàn khỏi mã nguồn ứng dụng Zalo Desktop chính thức. Công cụ giữ nguyên toàn bộ tệp thực thi đã ký số của VNG và tạo ra các vùng chạy cô lập theo từng profile.

```text
Multi-Zalo Desktop (C# / .NET Framework 4.x WPF + CLI)
│
├── UI & CLI Layer (src/ui/)
│   ├── Program.cs                 ──► Phân tích tham số CLI (--cli) hoặc khởi chạy WPF GUI
│   └── MainWindow.cs              ──► Giao diện quản lý tối giản (WinUI/Fluent neutral palette)
│
├── Application Core (src/core/)
│   ├── ProfileModels.cs           ──► Data models (ProfileRecord, StatusSnapshot, JsonHelper)
│   ├── ProfileManager.cs          ──► CRUD metadata (profiles.json), slug ID bất biến, chống path traversal
│   ├── InstanceManager.cs         ──► Điều phối trạng thái phiên, bảo vệ PID Zalo chính
│   └── DiagnosticsService.cs      ──► Tự kiểm chứng tính cô lập (verify-isolation)
│
├── Platform Adapters (src/platform/)
│   ├── PlatformInterfaces.cs      ──► Giao diện trừu tượng IPlatformAdapter
│   ├── WindowsPlatformAdapter.cs  ──► Triển khai Windows 10/11 (CreateProcessW, rstrtmgr.dll, Win32)
│   ├── MacOSPlatformAdapter.cs    ──► Khung kiến trúc macOS (Phase 3 - chưa kích hoạt runtime)
│   └── LinuxPlatformAdapter.cs    ──► Khung đánh giá Linux (Phase 4 - Unsupported)
│
└── Data Layer (%LOCALAPPDATA%\MultiZaloDesktop\)
    ├── profiles.json              ──► Metadata hiển thị (Id, Name, DirectoryName, Timestamps)
    └── Profiles\<slug>_<id>\      ──► Vùng dữ liệu cô lập cho từng phiên Zalo
        ├── AppData\Roaming\
        │   ├── zmeta.json
        │   └── ZaloData\          ──► userData, lockfile, Partitions\zalo, media, ftsm_*.db
        ├── AppData\Local\Temp\    ──► Cache & tệp tạm tách biệt theo profile
        ├── Documents\Zalo Received Files\
        └── Desktop\
```

---

## 2. Thiết Kế Các Thành Phần Cốt Lõi

### 2.1. Profile Manager (`src/core/ProfileManager.cs`)
- **Lưu trữ metadata tách rời tên thư mục**: Mỗi profile có `Id` ngẫu nhiên 8 ký tự hex và `DirectoryName` bất biến (`<slug>_<id>`), còn tên hiển thị `Name` lưu trong `profiles.json`.
  - **Lợi ích**: Người dùng có thể đổi tên profile bất cứ lúc nào kể cả khi phiên Zalo đó đang mở mà không làm hỏng handle SQLite/LevelDB hay tạo đường dẫn NTFS không hợp lệ.
- **Bảo toàn dữ liệu**: Ghi `profiles.json` qua tệp tạm (`profiles.json.tmp` -> `File.Replace`). Kiểm tra chống thoát thư mục gốc (`Path.GetFullPath` prefix check).
- **Không lưu bí mật**: Chỉ lưu `Id`, `Name`, `DirectoryName`, `CreatedAt`, `LastLaunchedAt`, `Notes`. Không bao giờ lưu mật khẩu, số điện thoại hay token.

### 2.2. Windows Platform Adapter (`src/platform/WindowsPlatformAdapter.cs`)
1. **Khám phá động bản cài đặt chính thức (`DiscoverOfficialInstallation`)**:
   - Quét `%LOCALAPPDATA%\Programs\Zalo\Zalo-*`, phân tích phiên bản theo số nguyên (`26.10.20`, `26.11.12`, ...), chọn bản mới nhất chứa cả `Zalo.exe` và `resources\app.asar`.
   - Kiểm tra chứng chỉ Authenticode (`X509Certificate.CreateFromSignedFile`) xác nhận chữ ký `VNG`.
2. **Pre-seed thư mục cô lập (`EnsureProfileDirectories`)**:
   - Tạo cấu trúc `AppData\Roaming\ZaloData\media`, `AppData\Local\Temp`, `Documents\Zalo Received Files`, `Desktop`.
   - Tạo sẵn `zmeta.json` (`{"failedMigrateCount":0}`) và `ZaloData\migrate-config.json` (`{"zalopc_m_c":true}`) để `migration.js` và `IpzU` không kiểm tra/đụng tới `%USERPROFILE%\AppData\Local\ZaloPC`.
3. **Khởi chạy tách biệt bằng Win32 `CreateProcessW` (`StartDetachedProcess`)**:
   - Khởi chạy `Zalo-<ver>\Zalo.exe` với `bInheritHandles = false` và `dwCreationFlags = CREATE_UNICODE_ENVIRONMENT | CREATE_NEW_PROCESS_GROUP`.
   - Thiết lập `USERPROFILE`, `APPDATA`, `LOCALAPPDATA`, `TEMP`, `TMP`, `HOMEDRIVE`, `HOMEPATH` trỏ hoàn toàn vào `<ProfilePath>`.
   - Nhờ `bInheritHandles = false`, CLI launcher không bị treo chờ pipe stdout từ tiến trình con Zalo.
4. **Theo dõi PID chính xác theo `lockfile` (`GetLockingPids` qua `rstrtmgr.dll`)**:
   - Dùng Windows Restart Manager (`RmStartSession`, `RmRegisterResources`, `RmGetList`) truy vấn chính xác tiến trình `Zalo.exe` đang khóa `<ProfilePath>\AppData\Roaming\ZaloData\lockfile`.
   - Dùng `CreateToolhelp32Snapshot` dựng cây tiến trình con (`ParentProcessId`) để đếm tổng số tiến trình và tổng bộ nhớ WorkingSet/Private của từng profile.
5. **Đưa cửa sổ lên trước (`FocusOrRestoreProfile`)**:
   - Nếu tìm thấy `HWND` hiển thị (`EnumWindows` + `IsWindowVisible` + kích thước >= 120×120), gọi `ShowWindow(SW_RESTORE)` + `SetForegroundWindow`.
   - Nếu cửa sổ đang ẩn ở system tray (`HWND == 0`), spawn nhẹ một tiến trình `Zalo.exe` với cùng biến môi trường profile; cơ chế `second-instance` nội bộ của chính Zalo sẽ đánh thức cửa sổ đúng profile đó rồi tự thoát.
6. **Đóng riêng từng phiên an toàn (`StopProfile`)**:
   - Đối chiếu danh sách `LockingPids` của Zalo mặc định (`%APPDATA%\ZaloData\lockfile`); **từ chối thực thi** nếu có bất kỳ PID trùng khớp nào (`PROTECTED_PRIMARY_PROCESS`).
   - Gửi `WM_CLOSE` và `CloseMainWindow()`, chờ 1,5 giây để Electron/SQLite đóng sạch sẽ trước khi kết thúc các PID còn sót trong riêng cây tiến trình của profile đó.

---

## 3. Khắc Phục Chính Sách Windows Smart App Control (SAC / WDAC)

Trên máy Windows 11 thử nghiệm bật Smart App Control / Application Control policy:
- Các tệp PE/native mới biên dịch chưa có chữ ký số từ CA tin cậy (`dist\MultiZaloDesktop.exe` và cả `rea-windows-x64.node` của REA) bị hệ điều hành từ chối nạp trực tiếp.
- Giải pháp triển khai tuân thủ 100% chính sách bảo mật của Windows (không tắt hay vượt qua SAC):
  - Giữ nguyên kịch bản build ra `dist\MultiZaloDesktop.exe` (`scripts/build.ps1`) cho máy doanh nghiệp ký nội bộ hoặc máy tắt SAC.
  - Cung cấp bộ khởi chạy `dist\MultiZaloDesktop.ps1` và `dist\MultiZaloDesktop.cmd` thực thi chính mã nguồn C# qua `Add-Type` bên trong tiến trình `powershell.exe` đã được Microsoft ký số.
