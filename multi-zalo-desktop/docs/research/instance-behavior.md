# Phân Tích Cơ Chế Quản Lý Phiên & Nguyên Nhân Không Chạy Được Nhiều Tài Khoản

Tài liệu này phân tích bằng chứng ở cấp độ mã máy x86 và mã JavaScript trong `app.asar` để giải thích chính xác điều gì xảy ra khi mở Zalo Desktop lần thứ hai trên Windows.

---

## 1. Chuỗi Khởi Chạy (Startup Call Chain)

```text
Người dùng mở %LOCALAPPDATA%\Programs\Zalo\Zalo.exe (Stub Launcher)
  │
  ├─► 1. OpenMutexW / CreateMutexW("ZaloStubExecutable")
  ├─► 2. Quét thư mục con \Zalo-* -> chọn bản mới nhất có Zalo.exe + resources\app.asar
  ├─► 3. Ghi log vào %APPDATA%\ZaloData\log.log
  ├─► 4. CreateProcessW("...\Zalo-26.10.20\Zalo.exe") (kế thừa Environment Block)
  ├─► 5. AllowSetForegroundWindow + WaitForInputIdle(hProcess, 5000)
  └─► 6. ReleaseMutex + Exit
          │
          ▼
Zalo-26.10.20\Zalo.exe (Electron 22.3.9 Main Process)
  │
  └─► bootstrap.js
        ├─► perf.record(perf.STARTUP)
        ├─► require('./main-dist/migration')
        │     ├─ d = path.join(app.getPath("appData"), "ZaloData")
        │     ├─ Kiểm tra %APPDATA%\ZaloData\migrate-config.json & ZaloPC cũ
        │     └─ app.setPath("userData", d)   ◄── GHI ĐÈ userData TRƯỚC KHI KHÓA SINGLE INSTANCE!
        │
        └─► app.requestSingleInstanceLock()
              ├── true  ──► require('./main-dist/main')
              │               └─ Khởi tạo cửa sổ, IndexedDB, SQLite, ZaloCap, ZaloCall
              │               └─ app.on("second-instance", focusWindow & bringToTop)
              └── false ──► require('./main-dist/second-instance')
                              └─ Ghi timestamp vào <appData>\ZaloData\startup.log
                              └─ app.quit()
```

---

## 2. Bằng Chứng Kỹ Thuật Tại Từng Tầng

### 2.1. Tầng 1: Stub Launcher (`Programs\Zalo\Zalo.exe`)

Kiểm tra `inspect_pe_imports` (`ev_83087efa...`) và giải mã offset chuỗi/lệnh tại `VA 0x404F80..0x4051B0` (file offset `0x4380..0x45B0`):
- **Offset `217744` (`VA 0x435E90`)**: chuỗi Unicode `L"ZaloStubExecutable"` được truyền vào `OpenMutexW(0x001F0001, FALSE, L"ZaloStubExecutable")` tại `0x4391` (`FF 15 18 90 42 00`).
- Nếu mutex chưa tồn tại, stub gọi `CreateMutexW(NULL, FALSE, L"ZaloStubExecutable")` (`FF 15 1C 90 42 00`), rồi tìm thư mục `\Zalo-*` (`VA 0x435C90`), ghi log vào `%APPDATA%\ZaloData\log.log` (`VA 0x436080` + `0x436088`), gọi `CreateProcessW` (`FF 15 24 90 42 00`), chờ `WaitForInputIdle(hProcess, 5000)`, và giải phóng mutex bằng `ReleaseMutex` (`FF 15 28 90 42 00`).
- **Kết luận quan trọng**: `ZaloStubExecutable` chỉ là mutex bảo vệ quá trình khởi động ngắn hạn (~vài trăm ms đến tối đa 5 giây) của chính stub launcher — **nó KHÔNG phải mutex giữ phiên chạy suốt đời của Zalo Desktop**. Nếu khởi chạy trực tiếp `Zalo-<ver>\Zalo.exe`, mutex này thậm chí không được tạo.

### 2.2. Tầng 2: Electron `bootstrap.js` & `migration.js`

Đoạn mã gốc trích trực tiếp từ `app.asar/bootstrap.js` (756 bytes):
```javascript
function bootstrap() {
  require('./libs/perf-tracing/runtime');
  perf.record(perf.STARTUP);
  require('./main-dist/migration');
  perf.record(perf.MIGRATION_DONE);

  const isCompactApp = process.argv.some(e => e.startsWith('--launch-compact-app'));
  if (isCompactApp) {
    if (require('electron').app.requestSingleInstanceLock()) {
      return handleEntryCompactApp();
    } else {
      require('./main-dist/second-instance');
    }
  }

  if (require('electron').app.requestSingleInstanceLock()) {
    perf.record(perf.MAIN_SCRIPT);
    require('./main-dist/main');
  } else {
    require('./main-dist/second-instance');
  }
}
```

Tại `main-dist/migration.js` (offset `600654..605000`):
```javascript
p = path.join(app.getPath("appData"), o.appFolderOld); // "ZaloApp"
let d = path.join(app.getPath("appData"), o.appFolder); // "ZaloData"
...
app.setPath("userData", d);
```
**Hệ quả trực tiếp:**
- Việc `migration.js` gọi `app.setPath("userData", path.join(app.getPath("appData"), "ZaloData"))` **trước** `app.requestSingleInstanceLock()` khiến tham số dòng lệnh `--user-data-dir=...` của Chromium bị ghi đè hoàn toàn nếu `appData` không thay đổi.
- Hàm `app.requestSingleInstanceLock()` của Electron 22 sử dụng cơ chế `ProcessSingleton` của Chromium trên Windows: mở khóa độc quyền (exclusive file handle `FileShare.None`) trên tệp `<userData>\lockfile`, kèm cửa sổ ẩn thông báo (`Chrome_MessageWindow`) để chuyển tiếp tham số dòng lệnh cho tiến trình đang giữ khóa.

### 2.3. Tầng 3: `second-instance.js` và `main.js`

- Khi tiến trình thứ 2 gọi `app.requestSingleInstanceLock()` trên cùng thư mục `userData` (`%APPDATA%\ZaloData`), Windows từ chối mở `lockfile` với lỗi sharing violation, Electron gửi sự kiện `second-instance` sang tiến trình đầu tiên và trả về `false` cho tiến trình thứ 2.
- Tiến trình thứ 2 lập tức tải `main-dist/second-instance.js`: ghi một dòng thời gian vào `<appData>\ZaloData\startup.log` và gọi `app.quit()`.
- Tiến trình đầu tiên nhận sự kiện `On.on("second-instance", ...)` (`main.js` offset `3565794`): đặt cửa sổ chính `setAlwaysOnTop(true)` -> `focus()` -> khôi phục vị trí/kích thước nếu đang ẩn ở system tray.

---

## 3. Phân Biệt Rõ 4 Loại Giới Hạn

| Loại giới hạn | Cơ chế thực tế trong Zalo Desktop | Kết luận bằng chứng |
| :--- | :--- | :--- |
| **1. Giới hạn khởi chạy nhiều tiến trình** | **Không** có mutex toàn cục chặn `Zalo-<ver>\Zalo.exe` chạy nhiều tiến trình. Stub chỉ giữ mutex `ZaloStubExecutable` trong lúc spawn (< 5s). | Khi 2 tiến trình có `app.getPath("appData")` khác nhau, mỗi tiến trình lấy được `requestSingleInstanceLock()` trên `lockfile` riêng và chạy song song ổn định. |
| **2. Giới hạn dùng chung dữ liệu cục bộ** | Electron `app.getPath("appData")` dùng `SHGetKnownFolderPath(FOLDERID_RoamingAppData)`. Mặc định mọi lần mở đều trỏ vào `%APPDATA%\ZaloData`. | Nếu không cô lập biến môi trường, hai tiến trình buộc phải dùng chung `lockfile`, `Partitions\zalo`, `Local State`, `ftsm_*.db` -> dẫn tới khóa file hoặc xung đột SQLite/LevelDB. |
| **3. Giới hạn quản lý cửa sổ** | `second-instance` trong `main.js` (offset `3565794`) tự động focus lại cửa sổ đang mở thay vì tạo cửa sổ đăng nhập mới. | Chỉ là hệ quả của `requestSingleInstanceLock() == false`. Khi mỗi phiên có `userData\lockfile` riêng, mỗi phiên mở cửa sổ `Chrome_WidgetWin_1` độc lập hoàn toàn. |
| **4. Giới hạn phiên xác thực phía máy chủ** | Giao thức đăng nhập QR/mật khẩu và session token được quản lý bởi máy chủ Zalo và lưu cục bộ trong Chromium `Partitions\zalo` (cookie mã hóa DPAPI theo user Windows). | Mỗi profile đăng nhập qua luồng chính thức của Zalo cho một tài khoản riêng; công cụ không can thiệp vào xác thực hay giả lập thiết bị. |

---

## 4. Tương Tác Tiến Trình Con & Local IPC Giữa Các Phiên

Phân tích `main.js` xác định 2 thành phần IPC đáng chú ý khi nhiều phiên chạy song song:
1. **`ZaloCap.exe` (Chụp màn hình)**:
   - Mỗi phiên spawn bản `ZaloCap.exe` riêng và giao tiếp qua anonymous `stdin`/`stdout` pipe của tiến trình cha (`main.js` offset `981878`). Hoàn toàn độc lập giữa các phiên.
2. **`ZaloCall.exe` (Gọi thoại/Video — `main.js` offset `311822`)**:
   - Dùng tên pipe cố định toàn cục trên Windows: `\\.\pipe\PipeZCallSend` và `\\.\pipe\PipeZCallRecv`.
   - Khi một phiên thứ hai lắng nghe trên cùng pipe, `main.js` bắt lỗi `EADDRINUSE` (`offset 314800`: `serverRecv address in use` / `serverSend address in use`) nên ứng dụng **không bị crash**. Tuy nhiên, tại một thời điểm chỉ một phiên sở hữu named pipe điều khiển `ZaloCall.exe` từ phía ngoài nếu các phiên khởi tạo đồng thời server pipe này (đã ghi nhận rõ là hạn chế kỹ thuật không sửa binary gốc).
