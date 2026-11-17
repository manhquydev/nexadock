# Hướng dẫn sử dụng Desktop Instance Manager (Windows MVP)

Tài liệu hướng dẫn vận hành **Desktop Instance Manager (Multi-Zalo Desktop)** trên Windows 10/11 (x64) để chạy đồng thời nhiều phiên **Zalo Desktop** chính thức mà không chỉnh sửa tệp nhị phân của VNG và không xâm phạm phiên Zalo chính tại `%APPDATA%\ZaloData`.

---

## 1. Phạm vi hỗ trợ & Ranh giới trung thực

| Thành phần | Trạng thái hiện tại | Ghi chú quan trọng |
|---|---|---|
| **Mở & chạy đồng thời nhiều phiên Zalo Desktop cô lập (L1/L2)** | **SUPPORTED_AND_TESTED** | Mỗi profile có `USERPROFILE`, `APPDATA`, `LOCALAPPDATA`, `TEMP`, `TMP`, `.zalo-profile-id` riêng biệt |
| **Bảo vệ phiên Zalo chính (`%APPDATA%\ZaloData`)** | **SUPPORTED_AND_TESTED** | Kiểm tra Restart Manager `lockfile` & process tree; từ chối stop/delete/backup/restore đè lên Zalo chính (`PROTECTED_PRIMARY_PROCESS`, `PROTECTED_PRIMARY_PATH`) |
| **Backup offline & Restore sang profile mới** | **SUPPORTED_AND_TESTED** | Ghi manifest `.partial` -> `backup.json` kèm SHA-256 cho từng tệp; chỉ restore vào profile mới |
| **Chat / gửi file / đồng bộ nhiều tài khoản đã đăng nhập (L3)** | **NOT_TESTED tự động** | Yêu cầu người dùng tự quét mã QR trong từng cửa sổ Zalo; sử dụng `scripts/run-interactive-l3-checklist.ps1` |
| **Gọi thoại/video đồng thời nhiều phiên (`ZaloCall.exe`) & thông báo tách biệt (L4)** | **BLOCKED** | Giới hạn từ chính Zalo Desktop: named pipe `\\.\pipe\PipeZCallRecv` / `PipeZCallSend` dùng chung hệ thống và cùng `AppUserModelId("com.vng.zalo")` |
| **Phiên bản native Tauri (`dim-desktop.exe`) trên máy bật Smart App Control** | **Cần chữ ký số (Gate F)** | Binary biên dịch cục bộ/CI chưa gắn chứng chỉ Authenticode; dùng Signed-Host Bridge (`node apps/desktop/ui-server.mjs` + `scripts/dim-host.ps1`) trên máy bật SAC `Enforced` |

> **Cảnh báo an toàn:** Không tắt Windows Smart App Control (`VerifiedAndReputablePolicyState = 1`), không chạy công cụ dưới quyền Administrator trừ khi hệ thống yêu cầu cài đặt khác, và tuyệt đối không copy thủ công thư mục `%APPDATA%\ZaloData` sang profile mới khi Zalo chính đang chạy.

---

## 2. Yêu cầu môi trường trên Windows

1. **Windows 10 / Windows 11 (64-bit x64)**, chạy ở quyền người dùng thường (`asInvoker`).
2. **Zalo Desktop chính thức từ VNG** đã cài đặt tại `%LOCALAPPDATA%\Programs\Zalo\Zalo-<version>\Zalo.exe` (chữ ký Authenticode hợp lệ `CN=VNG GROUP JSC` / `O=VNG GROUP JSC`).
3. Cho chế độ **Signed-Host Bridge** (khuyên dùng trên máy Windows 11 không có chứng chỉ ký PE):
   - **Windows PowerShell** (`powershell.exe` sẵn có trong Windows).
   - **Node.js LTS / Current đã ký** (`node.exe` >= v22, đã thử nghiệm trên `v26.13.1`).
4. Dung lượng ổ đĩa trống tối thiểu **5 GiB** cộng dung lượng ước tính khi tạo backup hoặc restore.
5. Không bật OneDrive/GPO **Folder Redirection** trỏ `User Shell Folders\AppData` ra ngoài `%USERPROFILE%` hoặc đường dẫn mạng UNC (hệ thống sẽ chặn khởi chạy cô lập để tránh lộ dữ liệu về thư mục chuyển hướng).

---

## 3. Hai cách chạy trên Windows

Mọi lệnh bên dưới thực thi từ thư mục `desktop-instance-manager/`.

### Cách A: Giao diện Web cục bộ qua Signed-Host Bridge (Khuyên dùng khi chưa ký PE)

```powershell
node apps/desktop/ui-server.mjs
```

- Server chỉ bind tại `http://127.0.0.1:18420/` (có thể đổi cổng bằng `--port <số>` hoặc biến môi trường `DIM_PORT`).
- Mở đường dẫn `http://127.0.0.1:18420/` trên trình duyệt Edge/Chrome.
- Cơ chế bảo mật mặc định của loopback server:
  - Chặn `Host` khác loopback (`403 FORBIDDEN_HOST`), chặn cross-origin `Origin` / `Sec-Fetch-Site` (`403 FORBIDDEN_ORIGIN`), bắt buộc JSON `Content-Type` (`415`), kiểm tra session token `X-DIM-Session-Token` gắn theo phiên (`401 UNAUTHORIZED`), rate limit, tuần tự hóa mutation và timeout 30 giây.
- Phím tắt trên giao diện Web UI:
  - `Alt + N`: Mở hộp thoại tạo profile mới (`Create Isolated Profile`).
  - `Alt + R`: Làm mới trạng thái toàn bộ profile, Zalo chính và diagnostics.

### Cách B: Ứng dụng Tauri 2 Native (`dim-desktop.exe` — dành cho môi trường Dev / không bị SAC chặn unsigned PE)

Đường dẫn binary release sau khi biên dịch cục bộ:

```text
apps/desktop/src-tauri/target/release/dim-desktop.exe
```

Hoặc CLI Rust thuần:

```powershell
.\target\release\dim-cli.exe --cli --json status
```

> Lưu ý: Không phân phối `dim-desktop.exe` chưa ký Authenticode ra máy người dùng cuối có bật Smart App Control (`VerifiedAndReputablePolicyState = 1`).

---

## 4. Vận hành hàng ngày bằng giao diện hoặc CLI

### 4.1. Tạo, đổi tên, khởi chạy và đóng có kiểm soát

Qua Signed-Host CLI (`scripts/dim-host.ps1`):

```powershell
# Kiểm tra môi trường, quyền UAC, Smart App Control, ACL và Folder Redirection
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dim-host.ps1 --cli --json diagnostics

# Kiểm tra 7 bước cô lập tổng quát
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dim-host.ps1 --cli --json verify-isolation

# Tạo profile cô lập (hỗ trợ tên tiếng Việt, thư mục slug hóa giữ hậu tố 8 hex bất biến)
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dim-host.ps1 --cli --json -- create "Zalo Công Việc"

# Xem danh sách profile và ID
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dim-host.ps1 --cli --json status

# Khởi chạy hoặc focus một profile theo ID (thay <id> bằng mã 8 ký tự hex)
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dim-host.ps1 --cli --json -- launch <id>
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dim-host.ps1 --cli --json -- focus <id>

# Đóng an toàn theo cây PID (gửi --z-update-force-quit -> WM_CLOSE -> fallback cây tiến trình con)
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dim-host.ps1 --cli --json -- stop <id>
```

### 4.2. Đăng nhập nhiều tài khoản Zalo (Quy trình thủ công an toàn)

1. Mở 1 hoặc nhiều profile cô lập bằng nút **Launch** trên UI hoặc lệnh `launch <id>`.
2. Trong từng cửa sổ Zalo mới, dùng điện thoại của tài khoản tương ứng **quét mã QR đăng nhập trực tiếp trong ứng dụng Zalo Desktop chính thức**.
3. Sau khi đăng nhập, trình quản lý sẽ phát hiện `zalo_user_id` trong `<profile>\AppData\Roaming\ZaloData\Database\_production\<uid>` (hoặc từ `config.json` -> `db_path.location`) và tự động khóa không cho khởi chạy 2 profile trùng cùng `zalo_user_id` (`DUPLICATE_ZALO_USER_ID`).

### 4.3. Sao lưu (Backup) & Khôi phục (Restore)

```powershell
# Chỉ backup khi profile đã Stop
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dim-host.ps1 --cli --json -- backup <id>

# Xem danh sách bản sao lưu
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dim-host.ps1 --cli --json backups

# Khôi phục ra một profile mới hoàn toàn (xác minh SHA-256 trước khi promote)
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dim-host.ps1 --cli --json -- restore <backup_dir> --name "Zalo Phục Hồi"
```

### 4.4. Xóa profile cô lập

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dim-host.ps1 --cli --json -- delete <id> --confirm
```

- Lệnh luôn yêu cầu `--confirm` ở backend, kiểm tra profile đang tắt (`PROFILE_RUNNING` nếu còn chạy), gỡ các NTFS junction/reparse point trước khi xóa để không ảnh hưởng thư mục bên ngoài.

---

## 5. Chạy kiểm thử & Checklist L3 thủ công

1. **Kiểm thử tự động PowerShell + UI Bridge (không chạm Zalo chính):**
   ```powershell
   node tests/run-fixture-tests.mjs --out evidence/fixture-test-result.json
   ```
2. **Kiểm thử đối chiếu hợp đồng PowerShell vs Rust `dim-cli.exe`:**
   ```powershell
   node tests/run-contract-parity-tests.mjs --out evidence/contract-parity-result.json
   ```
3. **Checklist L3/L4 có hướng dẫn cho người dùng tự đăng nhập QR:**
   ```powershell
   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run-interactive-l3-checklist.ps1
   ```
   Kết quả được lưu tại `evidence/l3-interactive-checklist-result.json` (tự động lọc đường dẫn user và ghi chú).

---

## 6. Xử lý sự cố thường gặp

| Mã lỗi / Hiện tượng | Nguyên nhân | Cách khắc phục |
|---|---|---|
| `PROTECTED_PRIMARY_PROCESS` | Thao tác vô tình nhắm vào PID trùng Zalo chính tại `%APPDATA%\ZaloData` | Không đóng nhầm Zalo chính; tạo profile cô lập mới thay thế |
| `UNVERIFIED_BINARY` | `Zalo.exe` hoặc binary tùy chỉnh bị thay đổi/chưa ký hoặc không phải chữ ký `VNG GROUP JSC` | Cài lại bản Zalo Desktop chính thức từ VNG; không dùng binary đã chỉnh sửa |
| `DUPLICATE_ZALO_USER_ID` | Tài khoản Zalo này đang đăng nhập và chạy ở một profile khác (hoặc Zalo chính) | Đóng profile đang dùng tài khoản đó trước, hoặc đăng nhập tài khoản khác |
| `FOLDER_REDIRECTION_DETECTED` | `AppData` trong Registry trỏ sang OneDrive/mạng UNC | Tạm tắt Folder Redirection cho `AppData` hoặc chạy trên tài khoản Windows cục bộ |
| `INSUFFICIENT_DISK_SPACE` | Ổ đĩa còn ít hơn `5 GiB + kích thước backup` | Giải phóng dung lượng ổ đĩa trước khi backup/restore |
| Không nghe được cuộc gọi đồng thời trên 2 phiên | Xung đột named pipe hệ thống `PipeZCallRecv` của VNG | Đóng phiên phụ không gọi điện; giữ chỉ 1 phiên sử dụng `ZaloCall.exe` tại một thời điểm |
