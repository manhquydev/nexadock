# Báo Cáo Kiểm Thử Thực Nghiệm (`Testing Report`)

**Môi trường kiểm thử:** Windows 11 x64 (không nâng quyền Admin), Zalo Desktop chính thức phiên bản `26.10.20` (`Zalo-26.10.20\Zalo.exe`, Electron `22.3.9`).  
**Trạng thái máy trước khi test:** 1 phiên Zalo Desktop chính thức của người dùng đang chạy tại `PID 12980` (khóa `%APPDATA%\ZaloData\lockfile`, 11 tiến trình con).  
**Dữ liệu thô kiểm chứng:**
- PoC: [`evidence/poc-verification-result.json`](../evidence/poc-verification-result.json) (`scripts/run-poc.ps1`)
- Integration Suite: [`evidence/integration-test-result.json`](../evidence/integration-test-result.json) (`scripts/run-tests.ps1`)

---

## 1. Bảng Tổng Hợp Ma Trận 12 Nhóm Kiểm Thử (+ GUI Smoke)

| Mã Test | Hạng mục kiểm thử | Trạng thái | Bằng chứng đo đạc & Ghi nhận thực tế |
| :--- | :--- | :--- | :--- |
| **Test 1** | **Single instance** (1 profile cô lập) | **PASS** | `IT Alpha` mở thành công với `MainPid=11172`, `9` tiến trình, cửa sổ `HWND=9110846`, thời gian lên cửa sổ `1.932 ms`, `WorkingSet=1030,28 MB`. |
| **Test 2** | **Two instances** (2 profile cô lập song song + Zalo chính) | **PASS** | `IT Beta` (`PID 8020`, `HWND=984404`, `2.187 ms`) chạy đồng thời với `IT Alpha` (`PID 11172`) và Zalo chính (`PID 12980`) — tổng **3 phiên song song**. |
| **Test 3** | **Three instances** (3 profile cô lập song song + Zalo chính) | **PASS** | `IT Gamma` (`PID 2432`, `2.381 ms`) chạy đồng thời cùng `IT Alpha` (`11172`), `IT Beta` (`8020`) và Zalo chính (`12980`) — tổng **4 phiên Zalo Desktop song song**. |
| **Test 4** | **Restart** (Đổi tên khi chạy, đóng 1 phiên, mở lại) | **PASS** | Đổi tên `IT Beta` -> `IT Beta Renamed` khi đang mở mà không đứt handle; đóng riêng `IT Beta` trong `1.640 ms` (Alpha, Gamma và Zalo chính không bị ảnh hưởng); mở lại `IT Beta` thành công với `PID 1392` trong `2.178 ms`. |
| **Test 5** | **Data isolation** (Cô lập `userData`, cache, cấu hình) | **PASS** | `verify-isolation` đạt `AllPassed=true`. Cả 3 profile tự sinh `lockfile`, `Local State`, `config.json`, `migrate-config.json`, `Partitions\zalo` riêng trong `%LOCALAPPDATA%\MultiZaloDesktop_IT\Profiles\...`. |
| **Test 6** | **Notifications** (Thông báo giữa các tài khoản) | **BLOCKED** | Chưa kiểm thử thực tế do cần >= 2 tài khoản Zalo thật đăng nhập đồng thời nhận tin nhắn. Phân tích tĩnh (`main.js:1063016`, `2720673`): mỗi phiên tạo `Notification` Electron riêng nhưng dùng chung `AppUserModelId("com.vng.zalo")`. |
| **Test 7** | **File handling** (Gửi, nhận, mở, lưu file thử nghiệm) | **BLOCKED** | Chưa kiểm thử truyền file do cần phiên đăng nhập sống. Phân tích tĩnh (`IpzU`): `Documents\Zalo Received Files` và `ZaloData\media` đều phân giải qua biến môi trường cô lập của từng profile. |
| **Test 8** | **Calls** (Gọi thoại và video đồng thời) | **BLOCKED** | Chưa kiểm thử cuộc gọi do cần tài khoản thật và thiết bị đối ứng. Phân tích tĩnh (`main.js:311822`, `314800`): `ZaloCall.exe` dùng global named pipe `\\.\pipe\PipeZCallSend` / `PipeZCallRecv`; phiên thứ 2 bắt lỗi `EADDRINUSE` không crash nhưng không giữ server pipe gọi đồng thời. |
| **Test 9** | **Resource usage** (CPU, RAM, Disk, Startup time) | **PASS** | Đo trên 3 phiên cô lập chạy đồng thời ở trạng thái chờ (xem bảng chi tiết mục 2 bên dưới). Tổng `WorkingSet = 3.231,61 MB` (~1,07 GB/phiên), khởi động ~`1,9s – 2,4s`/phiên. |
| **Test 10** | **Updates** (Tương thích cập nhật & toàn vẹn binary) | **PASS** | Cơ chế tìm động thư mục `Zalo-*` mới nhất hoạt động đúng (`Zalo-26.10.20`), SHA-256 của `Zalo.exe` không đổi trước/sau thử nghiệm, giữ nguyên chữ ký `CN=VNG GROUP JSC`. |
| **Test 11** | **Failure recovery** (Crash, mở trùng, bảo vệ xóa) | **PASS** | Mở trùng `IT Alpha` khi đang chạy chỉ focus lại `PID 11172`; thử xóa profile đang chạy bị từ chối (`PROFILE_RUNNING`); giết nóng `IT Gamma` (`PID 2432`), công cụ nhận diện chính xác `IsRunning=false` dù còn stale `lockfile` và khởi động phục hồi sạch với `PID 23348`. |
| **Test 12** | **Security** (Không rò rỉ dữ liệu, ACL, non-admin) | **PASS** | `profiles.json` không chứa mật khẩu/token/cookie; chạy ở quyền user chuẩn (`non_elevated=true`); ACL thư mục profile không cho `Everyone`/`Users` ghi; hash SHA-256 các tệp `startup.log`, `log.log`, `config.json` của Zalo chính giữ nguyên 100%. |
| **GUI Smoke** | **WPF Desktop Manager UI Launch** | **PASS** | Cửa sổ WPF `"Multi-Zalo Desktop"` mở thành công với `HWND=7997028` trong `2.951 ms` qua host PowerShell đã ký và đóng sạch bằng `WM_CLOSE`. |

---

## 2. Số Liệu Đo Tài Nguyên Chi Tiết (Test 9 — 3 Phiên Cô Lập Chạy Song Song)

| Profile | Main PID | Số tiến trình | Working Set (MB) | Private Memory (MB) | Dung lượng đĩa (MB) | Tổng CPU (s) | Thời gian lên cửa sổ (ms) |
| :--- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| **IT Alpha** | `11172` | 9 | `1.074,84` | `650,68` | `32,46` | `18,39` | `1.932` |
| **IT Beta** | `8020` | 9 | `1.041,58` | `618,05` | `26,39` | `18,89` | `2.187` |
| **IT Gamma** | `2432` | 9 | `1.115,19` | `700,57` | `19,06` | `17,17` | `2.381` |
| **Tổng cộng (3 phiên cô lập)** | — | **27** | **`3.231,61`** | **`1.969,30`** | **`77,91`** | **`54,45`** | Trung bình **`2.167`** |

*Ghi chú: Phiên Zalo chính của người dùng (`PID 12980`, đã đăng nhập đầy đủ gồm `ZaloCall.exe` và `ZaloCap.exe`, 11 tiến trình) vẫn tiếp tục chạy bình thường trong suốt bài đo mà không bị gián đoạn.*

---

## 3. Phân Loại Kết Quả Theo Tiêu Chuẩn

- **PASS:** `10 / 13` hạng mục kiểm thử (`Test 1, 2, 3, 4, 5, 9, 10, 11, 12, GUI-Smoke`).
- **BLOCKED:** `3 / 13` hạng mục (`Test 6, 7, 8`) — bị chặn do đòi hỏi từ 2 tài khoản Zalo thật đã xác thực và phát sinh lưu lượng nhắn tin/gọi thoại trực tiếp (tuân thủ nguyên tắc không tự vượt xác thực và không tạo tài khoản ảo/spam).
- **FAIL:** `0 / 13`.
