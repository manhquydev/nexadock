# Đánh Giá Bảo Mật & An Toàn Dữ Liệu (`Security Review`)

## 1. Phạm Vi & Mô Hình Đe Dọa

`Multi-Zalo Desktop` chỉ đóng vai trò trình quản lý profile và tiến trình ở tầng hệ điều hành phía ngoài ứng dụng Zalo Desktop. Công cụ:
- **Không** chặn, đọc, sửa hay giải mã lưu lượng mạng của Zalo.
- **Không** trích xuất cookie, token, khóa SQLite hay mật khẩu từ bộ nhớ hoặc ổ đĩa.
- **Không** chỉnh sửa bất kỳ byte nào trong `Zalo.exe`, DLL hoặc `app.asar`.
- **Không** kết nối Internet hay gửi telemetry ra bên ngoài.

---

## 2. Đánh Giá Chi Tiết Các Tiêu Chí An Toàn

### 2.1. Bảo vệ dữ liệu profile chính của người dùng
- **Vị trí tách biệt**: Profile chính của Zalo nằm tại `%APPDATA%\ZaloData`. Dữ liệu của Multi-Zalo Desktop mặc định lưu tại `%LOCALAPPDATA%\MultiZaloDesktop\Profiles\<slug>_<id>`.
- **Kiểm tra chéo PID trước khi dừng tiến trình**: Trước khi gọi `StopProfile`, `InstanceManager` luôn truy vấn `InspectPrimaryZaloStatus()` (qua `rstrtmgr.dll` trên `%APPDATA%\ZaloData\lockfile`) và truyền danh sách `LockingPids` vào danh sách bảo vệ (`protectedPrimaryPids`). Nếu trùng PID, thao tác dừng bị hủy ngay với mã `PROTECTED_PRIMARY_PROCESS`.
- **Không có nút "Kill All" toàn cục**: Tránh hoàn toàn lỗi nguy hiểm của `kemonra/ZaloMulti` (`Get-Process Zalo | Stop-Process -Force`).

### 2.2. Quyền truy cập tệp tin (NTFS ACL) & Lưu trữ
- Khác với `kemonra/ZaloMulti` lưu tại `C:\Zalo_Clone_Profiles` (dễ thừa kế quyền đọc rộng từ gốc ổ `C:\`), toàn bộ profile của `Multi-Zalo Desktop` nằm trong `%LOCALAPPDATA%\MultiZaloDesktop\Profiles`.
- Kiểm tra ACL thực tế trong bài test `Test-12` xác nhận không cấp quyền `Write/Modify/FullControl` cho `Everyone` hoặc `BUILTIN\Users`.
- Không sử dụng symbolic link hay hard link giữa các profile (tránh rò rỉ dữ liệu phiên hoặc xung đột ghi tệp).

### 2.3. Quyền hệ thống (Least Privilege)
- Ứng dụng chạy hoàn toàn ở quyền người dùng chuẩn (`asInvoker`, `IsElevated = false`).
- Không yêu cầu quyền Administrator, không cài Windows Service, không cài kernel driver, không chỉnh sửa Registry hệ thống.

### 2.4. Cookie Encryption & Electron Fuses
- Kết quả REA `inspect_electron_fuses` (`ev_8e05df8e...`) xác nhận `EnableCookieEncryption` đang bật (`enabled`) trong `Zalo-26.10.20\Zalo.exe`.
- Electron dùng Windows **DPAPI** (`CryptProtectData` / `CryptUnprotectData` trong `CRYPT32.dll`, thấy rõ trong `ev_522de365...`) gắn khóa mã hóa với tài khoản Windows đang đăng nhập (`Local State -> os_crypt.encrypted_key`).
- Vì tất cả profile đều chạy dưới cùng user Windows hiện tại nên mỗi profile tự sinh và giải mã `Local State` độc lập trong vùng dữ liệu riêng mà vẫn đảm bảo bảo mật ở cấp độ user Windows.

### 2.5. Tuân Thủ Điều Khoản & Giới Hạn Nghiên Cứu
- **Không vượt xác thực**: Mỗi phiên mở ra màn hình đăng nhập chính thức của Zalo (`Zalo Login` QR / mật khẩu); người dùng tự quét mã QR hoặc nhập thông tin đăng nhập trực tiếp với máy chủ VNG.
- **Không giả mạo thiết bị hay tự động hóa tin nhắn**: Không can thiệp User-Agent, HWID hay gửi tin nhắn tự động.
- **Lưu ý rủi ro điều khoản sử dụng**: Mặc dù công cụ không sửa đổi phần mềm và mỗi phiên là một tiến trình Zalo Desktop thật, việc vận hành đồng thời nhiều tài khoản trên cùng một máy tính vẫn phải tuân thủ chính sách sử dụng hợp lệ của nhà cung cấp dịch vụ (chỉ dùng cho các tài khoản hợp pháp của chính người dùng; tránh hành vi spam hoặc đăng nhập bất thường hàng loạt dẫn đến cảnh báo bảo mật từ phía máy chủ).
- **Khuyến cáo sao lưu**: Khi xóa profile qua UI hoặc `--cli delete <id> --confirm`, toàn bộ dữ liệu cô lập của profile đó sẽ bị xóa vĩnh viễn sau khi người dùng xác nhận và phiên Zalo của profile đã được đóng.
