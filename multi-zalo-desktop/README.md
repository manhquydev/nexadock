# Multi-Zalo Desktop

Công cụ quản lý và vận hành nhiều phiên **Zalo Desktop chính thức** đồng thời và độc lập trên Windows 10/11, được xây dựng dựa trên kết quả nghiên cứu dịch ngược bằng **REA (Reverse Engineer Anything)** và kiểm thử thực nghiệm.

---

## 1. Cấu Trúc Dự Án

```text
multi-zalo-desktop/
├── docs/
│   ├── research/
│   │   ├── rea-capabilities.md        # Đánh giá khả năng & giới hạn REA v6.9.0 trên Windows
│   │   ├── zalo-desktop-analysis.md   # Phân tích ZaloSetup-26.11.12.exe, Zalo 26.10.20, app.asar
│   │   ├── instance-behavior.md       # Giải mã stub Zalo.exe, bootstrap.js, single-instance lock
│   │   ├── profile-isolation.md       # Cơ chế USERPROFILE/APPDATA/User Shell Folders & rstrtmgr
│   │   └── existing-solutions.md      # Kiểm toán mã nguồn kemonra/ZaloMulti & so sánh Option A-E
│   ├── architecture.md                # Thiết kế kiến trúc ứng dụng
│   ├── security-review.md             # Đánh giá bảo mật, ACL & Smart App Control
│   ├── compatibility.md             # Tương thích cập nhật Windows, nghiên cứu macOS & Linux
│   └── testing-report.md              # Ma trận 12 bài kiểm thử thực nghiệm
├── evidence/                          # Bằng chứng kỹ thuật đã làm sạch (log PoC/IT; các tệp rea-*.json thô đã gỡ trước khi công khai)
├── src/
│   ├── core/                          # ProfileModels, ProfileManager, InstanceManager, Diagnostics
│   ├── platform/                      # WindowsPlatformAdapter, MacOS & Linux research stubs
│   └── ui/                            # MainWindow (WPF) & Program (CLI/GUI entrypoint)
├── tests/
│   └── IntegrationTests.ps1           # Bộ kiểm thử tự động ma trận 12 mục + GUI smoke
├── scripts/
│   ├── build.ps1                      # Biên dịch dist/MultiZaloDesktop.exe cục bộ qua csc.exe (không commit nhị phân chưa ký vào repo công khai)
│   ├── run-poc.ps1                    # Chạy Proof of Concept 2 phiên cô lập + kiểm tra Zalo chính
│   └── run-tests.ps1                  # Chạy IntegrationTests.ps1
└── dist/
    ├── MultiZaloDesktop.ps1           # Launcher PowerShell đã ký host (tương thích Smart App Control)
    └── MultiZaloDesktop.cmd           # Script mở nhanh (Double-click mở GUI hoặc truyền --cli)
```

---

## 2. Hướng Dẫn Sử Dụng & Kiểm Thử

### Mở giao diện quản lý (GUI)
- Nhấp đúp [`dist/MultiZaloDesktop.cmd`](dist/MultiZaloDesktop.cmd) hoặc chạy:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -STA -File ".\dist\MultiZaloDesktop.ps1"
```
*(Nếu hệ thống không bật Windows Smart App Control chặn PE chưa ký, bạn có thể tự biên dịch cục bộ qua `scripts/build.ps1` rồi chạy `dist\MultiZaloDesktop.exe`; tệp nhị phân chưa ký không còn lưu sẵn trên repository công khai.)*

### Sử dụng qua CLI
```powershell
# Kiểm tra trạng thái Zalo chính thức và danh sách profile
.\dist\MultiZaloDesktop.cmd --cli status

# Tạo profile mới
.\dist\MultiZaloDesktop.cmd --cli create "Cong viec"

# Mở / Đưa cửa sổ profile lên trước
.\dist\MultiZaloDesktop.cmd --cli launch "Cong viec"

# Đóng riêng một profile (an toàn, không chạm tới Zalo chính)
.\dist\MultiZaloDesktop.cmd --cli stop "Cong viec"

# Xác minh cô lập dữ liệu và PID
.\dist\MultiZaloDesktop.cmd --cli verify-isolation
```

### Biên dịch và chạy kiểm thử tự động
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File ".\scripts\build.ps1"
powershell -NoProfile -ExecutionPolicy Bypass -File ".\scripts\run-poc.ps1"
powershell -NoProfile -ExecutionPolicy Bypass -File ".\scripts\run-tests.ps1"
```

---

## 3. Báo Cáo Tổng Kết Cuối Dự Án (Trả Lời 15 Câu Hỏi Kỹ Thuật — Mục 15.1)

1. **Zalo Desktop hiện tại sử dụng kiến trúc gì?**
   - **Electron `22.3.9` + Chromium `108.0.5359.215` (PE32 x86)**, gồm stub launcher `Programs\Zalo\Zalo.exe` (790.568 bytes), thư mục phiên bản `Zalo-26.10.20\Zalo.exe` (136.638.504 bytes), gói `resources\app.asar` (177.728.582 bytes), 16 thư viện C++ `.node` trong `app.asar.unpacked` và các tiến trình phụ trợ Qt5/ONNX (`ZaloCall.exe`, `ZaloCap.exe`, `zocr-host.exe`). *(Đo trực tiếp qua REA + ASAR parser.)*

2. **Vì sao nhiều phiên chưa hoạt động theo nhu cầu?**
   - Trong `app.asar/bootstrap.js`, Zalo gọi `require('./main-dist/migration')` trước khi gọi `app.requestSingleInstanceLock()`. File `migration.js` tính `d = path.join(app.getPath("appData"), "ZaloData")` và ghi đè `app.setPath("userData", d)`. Khi mở lần thứ 2 trong cùng môi trường Windows, `app.getPath("appData")` đều trỏ về `%APPDATA%`, dẫn tới `requestSingleInstanceLock()` đụng khóa độc quyền trên `%APPDATA%\ZaloData\lockfile`, chuyển hướng qua `second-instance.js` (ghi `startup.log` và gọi `app.quit()`), còn phiên đầu nhận sự kiện `second-instance` (`main.js:3565794`) để focus cửa sổ cũ. *(Quan sát trực tiếp từ mã nguồn.)*

3. **REA giúp xác minh được những gì?**
   - REA (`rea-agents` v6.9.0) đã xác minh chứng chỉ số Authenticode (`CN=VNG GROUP JSC`), bảng import PE (`OpenMutexW`, `CreateMutexW`, `CryptProtectData`, `SHGetKnownFolderPath`), cờ bảo vệ PE (`DYNAMIC_BASE`, `NX_COMPAT`, `GUARD_CF`), toàn bộ 7 Electron Fuses (`EnableCookieEncryption=enabled`, `EnableEmbeddedAsarIntegrityValidation=disabled`), cấu trúc `app.asar` (10.869 node, 13.170 file), tài nguyên PE manifest của `ZaloSetup-26.11.12.exe`, và tự báo cáo giới hạn khi `rea-windows-x64.node` bị Windows Application Control chặn. *(Các Evidence ID đã đối chiếu từ bản xuất REA trước khi gỡ bỏ tệp `rea-*.json` khỏi repository công khai.)*

4. **Có thể dùng cơ chế nào để tách các phiên?**
   - **Per-process environment isolation** bằng `CreateProcessW` (`bInheritHandles = false`, `CREATE_UNICODE_ENVIRONMENT`): điều hướng `USERPROFILE`, `APPDATA`, `LOCALAPPDATA`, `TEMP`, `TMP`, `HOMEDRIVE`, `HOMEPATH` cho từng tiến trình `Zalo-<ver>\Zalo.exe`, kết hợp pre-seed `zmeta.json` và `migrate-config.json` (`zalopc_m_c: true`), và định danh PID chính xác từng phiên qua khóa `lockfile` bằng Windows Restart Manager (`rstrtmgr.dll`). *(Đã kiểm chứng thực nghiệm thành công.)*

5. **Dự án `ZaloMulti` hiện có hoạt động như thế nào?**
   - `kemonra/ZaloMulti` (commit `c71e7f2`) dùng script PowerShell `ZaloMulti.ps1` thiết lập `USERPROFILE`, `APPDATA`, `LOCALAPPDATA` trỏ vào `C:\Zalo_Clone_Profiles\<Tên>` rồi gọi stub `Zalo.exe`. Tuy nhiên nó tồn tại nhiều rủi ro nghiêm trọng: dùng `Get-Process Zalo | Stop-Process -Force` giết nhầm Zalo chính, lưu dữ liệu nhạy cảm ngay dưới `C:\`, lưu số điện thoại plaintext trong `phone.txt`, không cô lập `TEMP`, xóa hàng loạt shortcut Desktop, và đổi tên trực tiếp thư mục profile đang chạy. *(Đọc trực tiếp mã nguồn GitHub.)*

6. **Giải pháp được lựa chọn có ưu điểm gì?**
   - Giữ nguyên 100% file thực thi chính thức đã ký số của VNG; lưu trữ an toàn theo user trong `%LOCALAPPDATA%\MultiZaloDesktop\Profiles`; tách `DirectoryName` bất biến khỏi tên hiển thị để đổi tên được cả khi đang chạy; khóa bảo vệ chống dừng nhầm PID Zalo chính; hỗ trợ cả WPF GUI và CLI tự động hóa; tự tương thích khi Zalo cập nhật phiên bản.

7. **Có cần chỉnh sửa `Zalo.exe` không?**
   - **Không.** Không sửa `Zalo.exe`, không sửa `app.asar`, không hook DLL, không vô hiệu hóa chữ ký số.

8. **Có bao nhiêu phiên đã được thử nghiệm thành công?**
   - Đã chạy thử nghiệm thực nghiệm thành công **3 phiên Zalo Desktop cô lập đồng thời (`PID 11172`, `8020`, `2432`) cùng lúc với 1 phiên Zalo chính của máy (`PID 12980`) — tổng cộng 4 phiên Zalo Desktop song song độc lập** (cửa sổ `Zalo Login` thật, khóa `lockfile` riêng). *(Đã đo trong `evidence/integration-test-result.json`.)*

9. **Tình trạng lưu phiên đăng nhập ra sao?**
   - Mỗi profile giữ nguyên cơ chế lưu phiên Chromium `Partitions\zalo` và mã hóa cookie bằng DPAPI (`EnableCookieEncryption=enabled`) gắn với tài khoản Windows hiện tại. Kiểm tra khởi động lại (`Test-4`, `Test-11`) xác nhận tệp `Local State`, `config.json`, `Partitions\zalo` được bảo toàn giữa các lần tắt/mở. Việc duy trì trạng thái đăng nhập dài hạn sau khi quét QR phụ thuộc vào chính sách máy chủ Zalo và chưa đo dài ngày với nhiều tài khoản thật.

10. **Có vấn đề bảo mật hay nguy cơ khóa tài khoản không?**
    - Về kỹ thuật cục bộ: chạy không cần quyền Admin, không thu thập bí mật, ACL thư mục nằm dưới `%LOCALAPPDATA%` của riêng user.
    - Về phía dịch vụ: không can thiệp giao thức, xác thực hay tự động hóa tin nhắn. Tuy nhiên cần lưu ý về giới hạn named pipe `\\.\pipe\PipeZCallSend`/`PipeZCallRecv` của `ZaloCall.exe` khi chạy song song, và người dùng cần sử dụng đúng mục đích cho các tài khoản hợp lệ do mình sở hữu theo điều khoản của Zalo.

11. **Mức độ giữ nguyên tính năng Zalo Desktop là bao nhiêu?**
    - **100% giao diện và lõi Zalo Desktop chính thức** cho chat, đăng nhập, cấu hình, bộ nhớ cache và DB cục bộ. Hạn chế kỹ thuật đã xác định từ phân tích tĩnh: `ZaloCall.exe` dùng tên named pipe toàn cục cố định nên chỉ một phiên giữ pipe server gọi thoại/video tại một thời điểm (`Test 8` ở trạng thái `BLOCKED`); thông báo dùng chung `AppUserModelId("com.vng.zalo")`; và link giao thức `zalo://` mở về profile mặc định của hệ điều hành.

12. **Có thể triển khai cho người dùng Windows khác không?**
    - **Có**, công cụ khám phá động thư mục `%LOCALAPPDATA%\Programs\Zalo\Zalo-*` theo user hiện tại và không chứa đường dẫn cứng theo máy. Lưu ý: trên máy Windows 11 bật Smart App Control (SAC), binary `MultiZaloDesktop.exe` chưa ký số sẽ bị SAC chặn, nên người dùng sử dụng `dist\MultiZaloDesktop.cmd` / `MultiZaloDesktop.ps1` (chạy trên host PowerShell đã ký của Microsoft) hoặc ký mã cho `MultiZaloDesktop.exe` bằng chứng chỉ tin cậy nội bộ.

13. **Cập nhật Zalo có thể gây lỗi gì?**
    - Vì launcher khám phá động phiên bản `Zalo-*` mới nhất, các bản cập nhật thông thường tự động tương thích. Rủi ro tiềm ẩn: (a) nếu nhiều phiên phụ đang mở trong lúc `Update.exe` chạy, quá trình dọn thư mục phiên bản cũ có thể bị giữ lock tạm thời; (b) nếu phiên bản Zalo tương lai chuyển sang Global Mutex hoặc đọc Registry bỏ qua biến môi trường `APPDATA`/`USERPROFILE`, cần cập nhật adapter tương ứng.

14. **Có thể mở rộng sang macOS không?**
    - **Có khả năng cao về mặt kiến trúc (Phase 3)** dựa trên mã nguồn `app.asar` (`"darwin"` dùng `~/Library/Application Support/ZaloData`, `ZaloHelper.app`), thông qua việc chạy binary bên trong `Zalo.app/Contents/MacOS/Zalo` với `HOME` và `TMPDIR` tách biệt. Tuy nhiên cần lưu ý xung đột Unix socket `/tmp/socketzalorecv2021` của `ZaloCall` (`fs.unlink` trong `main.js:314800`) và tương tác Keychain. **Trạng thái hiện tại: CHƯA KIỂM THỬ** (cần phần cứng macOS thật).

15. **Linux có khả thi không?**
    - **Không hỗ trợ (`UNSUPPORTED`).** VNG không phát hành ứng dụng Zalo Desktop chính thức cho Linux (nhánh `main.js` cho helper native `ZaloCall`/`ZaloCap` chỉ tồn tại cho `win32` và `darwin`). Không sử dụng Wine hay Zalo Web để thay thế.
