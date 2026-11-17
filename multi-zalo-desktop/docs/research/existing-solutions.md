# Đánh Giá Giải Pháp Hiện Có (`kemonra/ZaloMulti`) & So Sánh Kiến Trúc

## 1. Phân Tích Mã Nguồn Thực Tế Của `kemonra/ZaloMulti`

Thông tin kho lưu trữ đã kiểm tra trực tiếp qua GitHub API và tải mã nguồn các tệp:
- **Repository:** `https://github.com/kemonra/ZaloMulti` (commit duy nhất: `c71e7f2`, ngày `2026-04-27T09:05:10Z`)
- **Tệp mã nguồn đã đọc:** `README.md`, `ZaloMulti.bat`, `ZaloMulti.ps1`, `ZaloMulti.xaml`, `lessons_learned.md`, `profiles_config.json`, `desktop_profiles.json`

### Trả lời 12 câu hỏi đánh giá bắt buộc

1. **Dự án sử dụng cơ chế kỹ thuật nào?**
   - Trong `ZaloMulti.ps1` (dòng 250–266), hàm `Start-ZaloInstance` thiết lập 3 biến môi trường `USERPROFILE`, `APPDATA` (`<profile>\AppData\Roaming`), và `LOCALAPPDATA` (`<profile>\AppData\Local`) qua `System.Diagnostics.ProcessStartInfo` rồi khởi chạy `C:\Users\$($env:USERNAME)\AppData\Local\Programs\Zalo\Zalo.exe`.
   - *Lưu ý mâu thuẫn nội bộ*: Trong `lessons_learned.md`, tác giả lại ghi `"Fix: Use the PWA (Chromium App Mode) approach with --user-data-dir"`, nhưng mã thực thi thực tế trong `ZaloMulti.ps1` vẫn dùng điều hướng biến môi trường cho `Zalo.exe`.

2. **Có thực sự chạy ứng dụng Zalo Desktop gốc không?**
   - **Có**, gọi trực tiếp `Zalo.exe` trong thư mục cài đặt của người dùng. Tuy nhiên đường dẫn được hardcode cứng là `"C:\Users\$($env:USERNAME)\AppData\Local\Programs\Zalo\Zalo.exe"` (sẽ lỗi nếu Windows username khác tên thư mục profile hoặc cài ở ổ đĩa khác).

3. **Có chỉnh sửa file thực thi hay không?**
   - **Không** sửa đổi `Zalo.exe` hay `app.asar`.

4. **Profile được lưu trữ như thế nào?**
   - Lưu trực tiếp tại **`C:\Zalo_Clone_Profiles\<Tên Profile>`** (dòng 113 `ZaloMulti.ps1`), dùng chính tên người dùng nhập làm tên thư mục, kèm tệp `phone.txt` lưu số điện thoại dạng văn bản thuần (plaintext).

5. **Có thực sự tạo được các phiên độc lập không?**
   - Có khả năng tạo các phiên độc lập nhờ điều hướng `USERPROFILE` / `APPDATA` / `LOCALAPPDATA`, **nhưng thiếu pre-seed `migrate-config.json`/`zmeta.json`**, thiếu chuyển hướng `TEMP`/`TMP` và `HOMEDRIVE`/`HOMEPATH`.

6. **Các tiến trình có chia sẻ dữ liệu ngoài ý muốn không?**
   - Vì `TEMP` và `TMP` không được chuyển hướng, các thư mục tạm như `%TEMP%\Zalo Temp`, `%TEMP%\zTemp` và `%TEMP%\zpc-auc\preload.js` vẫn dùng chung trên cùng một máy.
   - Ngoài ra, nếu tên profile bị đổi trong UI (`Add_LostFocus`, dòng 345–356), `Rename-Item` đổi tên trực tiếp thư mục chứa profile trên đĩa — sẽ gây lỗi hoặc hỏng file đang mở nếu phiên Zalo đó đang chạy!

7. **Có ảnh hưởng đến ứng dụng gốc không?**
   - Vì `ZaloMulti.ps1` gọi stub `Programs\Zalo\Zalo.exe`, mỗi lần mở profile stub vẫn có thể ghi nhật ký vào đường dẫn mặc định nếu stub kiểm tra trước khi chuyển sang electron, và nguy hiểm nhất là nút **`BtnKillAll`** (dòng 449–451):
     ```powershell
     Get-Process Zalo | Stop-Process -Force -ErrorAction SilentlyContinue
     ```
     Lệnh này **giết cưỡng bức TẤT CẢ các tiến trình `Zalo` đang chạy trên máy — bao gồm cả Zalo Desktop chính của người dùng**, gây mất dữ liệu đang nhập hoặc hỏng giao dịch ghi WAL của SQLite nếu không flush!

8. **Có vấn đề bảo mật hoặc quyền truy cập file không?**
   - **Rất nghiêm trọng**:
     - Lưu toàn bộ dữ liệu tài khoản và DB tin nhắn tại `C:\Zalo_Clone_Profiles`. Trên Windows, thư mục gốc `C:\` cho phép `Authenticated Users` quyền đọc/tạo hoặc kế thừa ACL rộng hơn nhiều so với `%LOCALAPPDATA%` của riêng user; người dùng khác trên cùng máy có thể đọc được dữ liệu nếu không cấu hình ACL.
     - Tạo tệp `phone.txt` lưu số điện thoại plaintext trong thư mục profile.
     - `ZaloMulti.bat` chạy `powershell -ExecutionPolicy Bypass` với đường dẫn tác giả hardcode (`C:\Users\truongit\ZaloMulti\ZaloMulti.ps1`), và shortcut Desktop cũng nhúng `-ExecutionPolicy Bypass -WindowStyle Hidden`.
     - `New-AppShortcut` (dòng 270–274) xóa tất cả shortcut `Zalo *.lnk` trên Desktop mỗi lần refresh UI.

9. **Có phụ thuộc vào phiên bản Zalo cụ thể không?**
   - Có phụ thuộc ngầm vào đường dẫn hardcode `C:\Users\$env:USERNAME\AppData\Local\Programs\Zalo\Zalo.exe`, nhưng không phụ thuộc vào số phiên bản `Zalo-26.x.y` do gọi qua stub `Zalo.exe`.

10. **Có hoạt động với bản Zalo hiện tại không?**
    - Cơ chế lõi (điều hướng `USERPROFILE`/`APPDATA`/`LOCALAPPDATA`) tương thích về mặt khái niệm với Zalo `26.10.20`, nhưng việc không tách `TEMP`, không có cơ chế đóng riêng từng phiên, và `BtnKillAll` giết cả Zalo chính khiến nó không đạt chuẩn an toàn sản phẩm.

11. **Có tồn tại nguy cơ mất dữ liệu khi cập nhật Zalo không?**
    - Có nguy cơ hỏng dữ liệu hoặc kẹt khóa file nếu người dùng bấm `BtnKillAll` (`Stop-Process -Force`) khi SQLite đang ghi `ftsm_*.db-wal`, hoặc đổi tên profile khi phiên đang mở.

12. **Có thể kế thừa ý tưởng hoặc thành phần mã nguồn nào một cách hợp lệ?**
    - **Kế thừa ý tưởng cốt lõi**: Điều hướng biến môi trường ở cấp tiến trình con (`USERPROFILE`, `APPDATA`, `LOCALAPPDATA`) thay vì vá binary.
    - **Không kế thừa mã nguồn**: Toàn bộ phần UI WPF, quản lý tiến trình (`Stop-Process` toàn cục), lưu tại `C:\Zalo_Clone_Profiles`, `phone.txt`, đường dẫn hardcode `truongit`, và xóa shortcut Desktop hàng loạt đều bị loại bỏ và viết lại hoàn toàn bằng C# kiến trúc sạch.

---

## 2. So Sánh Các Phương Án Kiến Trúc (Options A – E)

| Tiêu chí đánh giá | Option A: Per-Process Env Isolation (Đã chọn) | Option B: OS User Context (`runas` / Secondary Logon) | Option C: Sandboxie / Windows Sandbox / Container | Option D: Official Multi-Account Config | Option E: Patch `app.asar` / `--user-data-dir` hook |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Giữ nguyên ứng dụng gốc** | **100%** (chạy thẳng `Zalo.exe` đã ký VNG) | 100% | 100% | N/A (Zalo Desktop không hỗ trợ) | **0%** (phá vỡ chữ ký/checksum `app.asar`) |
| **Mức độ độc lập dữ liệu** | **Cao** (`ZaloData`, `Partitions`, `media`, `Temp`, `Documents` riêng biệt) | Rất cao (cả HKCU Registry tách biệt) | Rất cao | N/A | Cao nhưng dễ gãy khi cập nhật |
| **Chạy đồng thời nhiều cửa sổ** | **Có** (kiểm chứng 4 phiên song song) | Có (nhưng phức tạp về Audio/Clipboard/UAC/Desktop) | Có (Sandboxie hạn chế bản free / driver kernel) | Không | Có |
| **Hiệu năng & RAM** | **Tối ưu** (chỉ tốn RAM của các tiến trình Zalo thật, ~0% overhead từ launcher) | Kém hơn (nạp profile Windows user phụ) | Tốn overhead ảo hóa / driver | N/A | Tối ưu |
| **Độ ổn định** | **Cao** (cô lập bằng API chuẩn Windows + `rstrtmgr.dll`) | Trung bình (lỗi drag-drop file, clipboard, GPU giữa user) | Phụ thuộc phần mềm thứ 3 | N/A | Thấp (vỡ khi auto-update) |
| **Khả năng cập nhật Zalo** | **Tự động 100%** (khám phá động `Zalo-*`) | Tự động | Khó (bản ghi sandbox ghi đè file update) | N/A | Bị ghi đè sau mỗi lần update |
| **Rủi ro bảo mật / quyền Admin** | **Thấp** (chạy quyền user thường, dữ liệu trong `%LOCALAPPDATA%`) | Cần tạo tài khoản Windows & giữ mật khẩu OS | Thường cần quyền Admin để cài driver | N/A | Rủi ro toàn vẹn mã nguồn |
| **Rủi ro điều khoản sử dụng** | Không sửa binary, không giả mạo API, đăng nhập chính thức | Tương tự | Tương tự | N/A | Cao (sửa mã ứng dụng) |
| **Chi phí bảo trì** | **Thấp** | Cao | Cao | N/A | Rất cao |
| **Khả năng mở rộng macOS/Linux** | macOS: kiến trúc `HOME` tương đương (cần máy macOS test); Linux: không có app native | Khác biệt hoàn toàn theo OS | Khác biệt hoàn toàn theo OS | N/A | Dễ vỡ |

**Kết luận lựa chọn**: **Option A (Per-Process Environment Isolation + Stateless Lockfile PID Tracking qua `rstrtmgr.dll`)** là phương án tối ưu nhất vì giữ nguyên 100% ứng dụng Zalo Desktop chính thức, không cần quyền Admin, không sửa binary, tự động tương thích khi Zalo cập nhật và đã được kiểm chứng thực nghiệm thành công.
