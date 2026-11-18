# Phân Tích Kiến Trúc Ứng Dụng Zalo Desktop (Windows)

Báo cáo này tổng hợp bằng chứng tĩnh thu thập trực tiếp từ bộ cài chính thức `ZaloSetup-26.11.12.exe` và bản cài đặt hiện hành trên máy `Zalo 26.10.20` bằng REA `rea-agents` v6.9.0 cùng các công cụ kiểm tra PE/ASAR cục bộ.

---

## 1. Xác Minh Nguồn Gốc, Chữ Ký Số, Hash & Kiến Trúc

| Thành phần | Đường dẫn | Kích thước (bytes) | Phiên bản | Kiến trúc | SHA-256 | Chữ ký Authenticode | Bằng chứng REA |
| :--- | :--- | ---: | :--- | :--- | :--- | :--- | :--- |
| Bộ cài đặt chính thức | `<USERPROFILE>\Downloads\Programs\ZaloSetup-26.11.12.exe` | `213.516.168` | `26.11.12.3564` | PE32 (`x86`) | `dddef07c11683561c6abe19c07bbc6202214136325d6b4b39311587a16a15eb8` | Valid — `CN=VNG GROUP JSC` (Serial `033b7444b0d9c6403783620bcc5f4caf`) | `ev_e417c46d...`, `ev_095e339d...` |
| Stub Launcher gốc | `%LOCALAPPDATA%\Programs\Zalo\Zalo.exe` | `790.568` | `1.0.0.0` (`ZaloStubExecutable`) | PE32 (`x86`) | `7295232c89f9a041e35253afa80787989eae9a91895f1847ebd92c78bb4195a2` | Valid — `CN=VNG GROUP JSC` | `ev_ab238d25...`, `ev_83087efa...` |
| Electron Main Binary | `%LOCALAPPDATA%\Programs\Zalo\Zalo-26.10.20\Zalo.exe` | `136.638.504` | `26.10.20.3253` | PE32 (`x86`, chạy trên WOW64) | `75dc85559079ec2037d511a312b704b44ca3293e74da1900ccfe84e6299cac3e` | Valid — `CN=VNG GROUP JSC` | `ev_8e05df8e...`, `ev_817da2b6...` |
| Gói ứng dụng Electron | `%LOCALAPPDATA%\Programs\Zalo\Zalo-26.10.20\resources\app.asar` | `177.728.582` | `26.10.20` | ASAR Archive | `d83256230e5d1d4483c2411f71936c3dc1562fb8a885fd8752cab17d229201e6` | N/A (gói dữ liệu JS/Assets) | `ev_70919879...`, `ev_98cf05ba...` |
| Trình cập nhật | `%LOCALAPPDATA%\Programs\Zalo\Zalo-26.10.20\Update.exe` | `833.576` | — | PE32 | `b8a83bdceb1cf136d44f7a3d00ab7d0cffd2c49b17a3a7babda3fc12579da6b7` | Valid — `CN=VNG GROUP JSC` | Kiểm tra PowerShell |
| Shortcut Helper | `%LOCALAPPDATA%\Programs\Zalo\Zalo-26.10.20\sl.exe` | `186.528` | — | PE32 | `4f350a1cabc60c8600d95506ee50d0ab00bdbdfb094b0b0a2c10c17ed02224a0` | Valid — `CN=VNG GROUP JSC` | Kiểm tra PowerShell |
| Công cụ gọi thoại/video | `%LOCALAPPDATA%\Programs\Zalo\Zalo-26.10.20\plugins\capture\ZaloCall.exe` | `19.588.136` | — | PE32 (`Qt5`) | `fbe5594b6d5c8c9c9213ed0621b80e8e89c55329c7de04771d1ccff86171b2f9` | Valid — `CN=VNG GROUP JSC` | Kiểm tra PowerShell |
| Công cụ chụp màn hình | `%LOCALAPPDATA%\Programs\Zalo\Zalo-26.10.20\plugins\capture\ZaloCap.exe` | `5.488.240` | — | PE32 (`Qt5`) | `ab2301a00d7eb79ea9a2440c5ff144a13b068932277b38dced9b4cfecb35f1a6` | Valid — `CN=VNG GROUP JSC` | Kiểm tra PowerShell |
| Tiến trình OCR | `%LOCALAPPDATA%\Programs\Zalo\Zalo-26.10.20\plugins\ocr\zocr-host.exe` | `5.075.056` | — | PE32 (`ONNX`) | `ac2e302214d751136727e5528f8e58ac8842834a1ba7f298a9e9b73a8423a308` | Valid — `CN=VNG GROUP JSC` | Kiểm tra PowerShell |

**Thông tin nhà phát hành chứng chỉ số (từ `ev_e417c46d...` và `ev_817da2b6...`):**
- **Subject:** `CN=VNG GROUP JSC, O=VNG GROUP JSC, S=Ho Chi Minh City, C=VN, SERIALNUMBER=0303490096`
- **Issuer:** `CN=DigiCert Trusted G4 Code Signing RSA4096 SHA384 2021 CA1, O=DigiCert, Inc., C=US`
- **Hiệu lực chứng chỉ:** `2026-03-04` đến `2026-12-09` (kèm timestamp counter-signature OID `1.3.6.1.4.1.311.3.3.1`).

---

## 2. Định Dạng Bộ Cài Đặt `ZaloSetup-26.11.12.exe`

- `inspect_pe_resources` (`ev_c13e46e2...`) đọc được manifest `RT_MANIFEST` tại offset `447840` (`1059` bytes):
  - `name="Nullsoft.NSIS.exehead"`, `description="Nullsoft Install System v3.03"`
  - `<requestedExecutionLevel level="asInvoker" uiAccess="false"/>` (cài đặt vào `%LOCALAPPDATA%\Programs\Zalo`, không yêu cầu quyền Administrator).
- Phần overlay sau ảnh PE (bắt đầu tại offset `448896`) chứa chữ ký `NullsoftInst` (+136) và luồng nén `7z` solid archive (+8288, +16458).

---

## 3. Kiến Trúc Framework & Các Thành Phần Ứng Dụng

Zalo Desktop sử dụng kiến trúc **Electron đa tiến trình + Qt5/Native C++ Addons**:

1. **Framework chính**: **Electron `22.3.9`** nhúng **Chromium `108.0.5359.215`** (xác minh từ chuỗi phiên bản trong `Zalo-26.10.20\Zalo.exe`). Kiến trúc nhị phân là **32-bit x86** (`Machine = 332 / IMAGE_FILE_MACHINE_I386`), chạy qua lớp tương thích WOW64 trên Windows 10/11 x64.
2. **File `.exe` ở thư mục gốc (`Programs\Zalo\Zalo.exe`)**:
   - Là **Stub Launcher** nhỏ (790 KB, tên nội bộ `ZaloStubExecutable`, đường dẫn PDB build gốc `D:\workspace\StubExecutableZalo\bin\Release\Zalo.pdb`).
   - Bản sao trùng khớp hoàn toàn (cùng SHA-256 `7295232c89...`) nằm trong `Zalo-26.10.20\ZaloExecutable.exe` và được `Update.exe` chép đè ra thư mục cha khi cập nhật phiên bản.
   - Nhiệm vụ duy nhất của stub là quét các thư mục `\Zalo-*`, chọn phiên bản mới nhất có `Zalo.exe` và `resources\app.asar`, rồi gọi `CreateProcessW` khởi chạy `Zalo-<ver>\Zalo.exe`.
3. **File `Zalo-26.10.20\Zalo.exe` (Electron Host)**:
   - Chứa runtime Chromium + Node.js.
   - Kết quả quét Electron Fuses (`ev_8e05df8e...`, sentinel tại offset `112166917`):
     - `RunAsNode`: **enabled**
     - `EnableCookieEncryption`: **enabled** (mã hóa cookie bằng DPAPI trên Windows)
     - `EnableNodeOptionsEnvironmentVariable`: **enabled**
     - `EnableNodeCliInspectArguments`: **enabled**
     - `EnableEmbeddedAsarIntegrityValidation`: **disabled**
     - `OnlyLoadAppFromAsar`: **disabled**
     - `LoadBrowserProcessSpecificV8Snapshot`: **disabled**
   - Kết quả kiểm tra Hardening (`ev_fce14a1d...`): bật `DYNAMIC_BASE` (ASLR), `NX_COMPAT` (DEP), `GUARD_CF` (Control Flow Guard), `authenticode_present`.
4. **Gói `resources\app.asar` và các Native Addons (`app.asar.unpacked`)**:
   - `inspect_artifact` (`ev_70919879...` / view `ev_2688bbbc...`): `app.asar` gồm `10.869` node, `13.170` file, `62` thư mục.
   - `package.json` bên trong `app.asar` khai báo `"name": "Zalo"`, `"version": "26.10.20"`, `"main": "bootstrap.js"`, cùng các thư viện chính: `electron-store 8.1.0`, `electron-updater 4.0.0`, `electron-log 4.2.2`, `react 16.14.0`, `@reduxjs/toolkit 1.6.2`, `@sentry/electron 4.8.0`, `@tensorflow/tfjs 4.13.0`.
   - `main-dist/` chứa các bundle Webpack đã biên dịch cho Main/Preload/Utility Process:
     - `migration.js` (1.112.675 bytes) — chạy đầu tiên để di chuyển dữ liệu cũ và gán `app.setPath("userData", ...)`.
     - `main.js` (4.342.755 bytes) — toàn bộ logic tiến trình chính Electron, quản lý cửa sổ, IPC, database, updater, ZaloCall/ZaloCap.
     - `second-instance.js` (1.516 bytes) — xử lý khi instance thứ 2 không lấy được khóa đơn phiên.
     - `compact-app.js` / `compact-app-preload.js`, `preload-render.js`, `preload-noti.js`, `preload-shared-worker.js`, `preload-sqlite.js`, `utility-process-media.js`, `utility-process-sqlite.js`, `sentry.js`.
   - `app.asar.unpacked\native\nativelibs\`: chứa 16 module `.node` C++ native (`db-cross-v4`, `sqlite3`, `zcall`, `zfile`, `zimage`, `zjxl`, `zlang`, `zocr`, `zwalker`, `win-utils`, `os-sys-info`, `file-utils`, `file-utilities`, `logger`, `mp4thumb`, `v8-profiles`).
5. **Tiến trình con (Child Processes) khi một phiên Zalo chạy**:
   - Kiểm chứng thực nghiệm trên phiên Zalo chính (`PID 12980`) và phiên PoC (`PID 15228`, `PID 14368`):
     - 1 tiến trình chính Electron Browser (`Zalo.exe`).
     - Nhiều tiến trình con Electron (`--type=gpu-process`, `--type=utility` cho Network Service / Audio / Storage / SQLite, và `--type=renderer` cho UI chính + Shared Worker + SQLite worker) — tổng cộng **9 tiến trình `Zalo.exe`** cho một phiên login mới, hoặc **11 tiến trình** (gồm `ZaloCall.exe` và `ZaloCap.exe`) cho phiên đã đăng nhập đầy đủ.
     - `plugins\capture\ZaloCap.exe`: ứng dụng Qt5 riêng cho chụp màn hình/clipboard, được spawn bởi `main.js` (offset `981878`) với tham số `["zalo2017", ...]` và giao tiếp qua anonymous `stdin`/`stdout`.
     - `plugins\capture\ZaloCall.exe`: ứng dụng Qt5 riêng cho gọi thoại/video (đi kèm `ZaviMeet.pdb` trong thư mục `pdbs`), được spawn bởi `main.js` (offset `314392`).
     - `plugins\ocr\zocr-host.exe`: host tiến trình tách biệt chạy ONNX Runtime cho OCR ảnh.

---

## 4. Cơ Chế Cập Nhật (`Update.exe` & `update_meta.json`)

Theo `Zalo-26.10.20\update_meta.json`, `main.js` (offset `2441500`) và nhật ký thực tế `%APPDATA%\ZaloData\update.log` (ngày `2026-11-01`):
- Zalo kiểm tra và tải bản cập nhật về `%TEMP%\tmp-...`.
- Kiểm tra checksum SHA-256, sau đó gọi `Update.exe --meta-file update_meta.json --app-root <Programs\Zalo>`.
- `Update.exe` giải nén/sao chép vào thư mục `Zalo-<newVer>` (ví dụ `Zalo-26.10.20`), cập nhật `..\Zalo.exe` (stub mới từ `ZaloExecutable.exe`), kiểm tra SHA-1 của `resources\app.asar`, và dọn dẹp các phiên bản cũ (`26.10.11`, `26.9.10`).
- **Kết luận cho thiết kế Multi-Zalo**: Công cụ quản lý tuyệt đối **không** được hardcode đường dẫn `Zalo-26.10.20\Zalo.exe`; phải quét động thư mục `Zalo-*` mới nhất tại mỗi lần khởi chạy (hoặc gọi stub) để tương thích liên tục với cơ chế tự động cập nhật của Zalo.
