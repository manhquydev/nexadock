# Hướng Dẫn Cài Đặt & Kiểm Thử Multi-Zalo Desktop (`v0.1.0-beta.1` — Pre-release)

> [!NOTE]
> **Tuyên bố phi liên kết:** Multi-Zalo Desktop (`Desktop Instance Manager`) là công cụ mã nguồn mở độc lập (`MIT OR Apache-2.0`), **không liên kết, không được tài trợ hay chứng thực bởi VNG Corporation hay Zalo**. `"Zalo"` là nhãn hiệu của VNG Corporation. Gói cài đặt **không** chứa ứng dụng Zalo Desktop — bạn cần cài Zalo Desktop chính thức từ VNG trên Windows hoặc macOS trước khi mở tài khoản.

> [!IMPORTANT]
> **Trạng thái bản phát hành thử nghiệm (`Pre-release`):**
> - Toàn bộ tạo phẩm hiện là **`UNSIGNED_DEVELOPMENT_ARTIFACT`** (chưa ký số Authenticode / Apple Developer ID / Notarization).
> - Kiểm thử đăng nhập 2 tài khoản Zalo thật bằng mã QR (`L3`) đang ở trạng thái **`LIVE_ZALO_NOT_TESTED`** — bản Pre-release này được phát hành để chủ dự án và người thử nghiệm trực tiếp cài đặt và nghiệm thu thực tế.

---

## 1. Hướng Dẫn Trên Windows (Windows 10 / 11 x64)

### 1.1. Chọn đúng bộ cài đặt
- **Khuyến nghị:** `Multi-Zalo-0.1.0-beta.1-windows-x64-setup.exe` (NSIS installer, hỗ trợ cài đặt theo user và gỡ cài đặt chuẩn Windows).
- **Tùy chọn thay thế:** `Multi-Zalo-0.1.0-beta.1-windows-x64.msi` (Windows Installer `.msi`).
- *(Lưu ý Smart App Control — SAC: Nếu máy Windows 11 bật Smart App Control ở chế độ Enforce `SAC=1`, Windows có thể chặn file cài đặt chưa ký số. Tuyệt đối **không** tắt Smart App Control; trên máy `SAC=1`, bạn có thể chạy chế độ **Signed-Host Bridge** từ mã nguồn bằng `node desktop-instance-manager/apps/desktop/ui-server.mjs` sử dụng `powershell.exe` + `node.exe` đã ký số).*

### 1.2. Cài đặt & Khởi chạy
1. Tải `Multi-Zalo-0.1.0-beta.1-windows-x64-setup.exe` và đối chiếu `SHA-256` với `SHA256SUMS.txt`.
2. Chạy file cài đặt (nếu Windows SmartScreen hiện cảnh báo do bản thử nghiệm chưa ký số, kiểm tra đúng `SHA-256` đã tải trước khi chọn tiếp tục; yêu cầu máy có sẵn Microsoft Edge WebView2 Runtime — trình cài đặt NSIS cũng cấu hình `downloadBootstrapper`).
3. Mở **Multi-Zalo Desktop** từ Start Menu hoặc Desktop shortcut.

### 1.3. Kiểm thử thực tế 2 tài khoản Zalo thử nghiệm (Cổng L3 QR)
> Khuyến nghị dùng **2 tài khoản Zalo thử nghiệm** được phép sử dụng; Zalo chính đang dùng sẵn trên máy (`%APPDATA%\ZaloData`) luôn được bảo vệ tách biệt hoàn toàn.

1. **Kiểm tra phát hiện Zalo Desktop:** Ở thanh trạng thái trên cùng, xác nhận ứng dụng báo `"Zalo Desktop đã sẵn sàng"` (và ở thẻ dưới cùng xác nhận Zalo gốc trên máy vẫn giữ nguyên trạng thái).
2. **Tạo không gian tài khoản #1:** Nhấn **Thêm tài khoản**, đặt tên (ví dụ `Thử nghiệm 1`), chọn màu nhận diện → nhấn **Tiếp tục** → nhấn **Mở Zalo để đăng nhập**.
3. **Quét QR tài khoản #1:** Quét mã QR bằng tài khoản thử nghiệm thứ nhất trực tiếp trong cửa sổ Zalo Desktop vừa mở.
4. **Tạo & mở tài khoản #2 đồng thời:** Quay lại Multi-Zalo Desktop, nhấn **Thêm tài khoản** (ví dụ `Thử nghiệm 2`), chọn màu khác và nhấn **Mở Zalo để đăng nhập**, quét QR bằng tài khoản thử nghiệm thứ hai.
5. **Kiểm tra hoạt động song song:**
   - Xác nhận cả 2 cửa sổ Zalo thử nghiệm và Zalo chính (nếu đang mở) cùng hiển thị trạng thái `"Cửa sổ đang mở"` mà **không đăng xuất chéo**.
   - Nhấn nút **Chuyển đến** trên từng thẻ tài khoản để kiểm tra đưa đúng cửa sổ lên trước.
   - Gửi tin nhắn / tệp thử nghiệm trên từng cửa sổ.
6. **Kiểm tra đóng/mở lại & giữ đăng nhập (DPAPI):**
   - Nhấn **Đóng Zalo** trên từng thẻ tài khoản thử nghiệm, sau đó đóng hẳn **Multi-Zalo Desktop** rồi mở lại.
   - Nhấn **Mở Zalo** lại cho cả 2 tài khoản và kiểm tra xem phiên đăng nhập có còn giữ nguyên không.
7. **Kiểm tra Sao lưu & Khôi phục:**
   - Đảm bảo tài khoản đang ở trạng thái `"Đã đóng"`.
   - Nhấn nút menu `⋯` → chọn **Sao lưu** (yêu cầu ổ đĩa còn tối thiểu `5 GB` trống + dung lượng dữ liệu).
   - Mở tab **Sao lưu** → chọn **Khôi phục thành tài khoản mới**, đặt tên mới và xác nhận tạo tài khoản mới độc lập mà không ghi đè tài khoản cũ hay Zalo gốc.

### 1.4. Giới hạn đã biết trên Windows
- **Cuộc gọi thoại/video đồng thời:** `ZaloCall.exe` dùng chung Named Pipe hệ thống (`PipeZCallRecv`/`PipeZCallSend`), nên **không thể** thực hiện 2 cuộc gọi đồng thời trên 2 cửa sổ Zalo cùng lúc (gọi tuần tự từng tài khoản hoạt động bình thường).
- **Thông báo Windows Action Center:** Các cửa sổ Zalo dùng chung `AppUserModelId("com.vng.zalo")` nên thông báo hệ thống có thể gom chung; khuyến nghị bật thông báo nổi trong ứng dụng Zalo (`notiWin`).

### 1.5. Gỡ cài đặt an toàn
- Đóng toàn bộ các cửa sổ Zalo phụ trước trong Multi-Zalo.
- Gỡ ứng dụng qua **Windows Settings → Apps → Installed apps → Multi-Zalo Desktop → Uninstall**.
- Lưu ý: Dữ liệu profile tại `%LOCALAPPDATA%\MultiZaloDesktop` (và Zalo chính tại `%APPDATA%\ZaloData`) được giữ lại an toàn khi gỡ cài đặt để tránh mất dữ liệu; xóa thủ công `%LOCALAPPDATA%\MultiZaloDesktop` nếu bạn muốn dọn sạch dữ liệu thử nghiệm.

---

## 2. Hướng Dẫn Trên macOS (Apple Silicon & Intel)

> [!WARNING]
> **Trạng thái tạo phẩm macOS (`EXPERIMENTAL` / `UNSIGNED_DEVELOPMENT_ARTIFACT` / `NOT_NOTARIZED`):**
> - Các tệp `.dmg` được build trực tiếp trên runner `macos-15` (Apple Silicon) và `macos-15-intel` (Intel), **chưa được ký bằng Apple Developer ID và chưa Notarized**.
> - **Chưa kiểm thử thực tế trên máy Mac vật lý với `/Applications/Zalo.app`** (`LIVE_ZALO_NOT_TESTED`; `PinnedPublisherPolicy` mặc định fail-closed cho tới khi xác minh `TeamIdentifier` trên macOS).
> - **KHÔNG** tắt Gatekeeper (`spctl --master-disable`) hay thay đổi chính sách bảo mật hệ điều hành để chạy phần mềm chưa ký số.

### 2.1. Chọn đúng tệp theo kiến trúc CPU
- **Máy Mac dùng chip M1 / M2 / M3 / M4 (Apple Silicon `arm64`):** `Multi-Zalo-0.1.0-beta.1-macos-arm64.dmg`
- **Máy Mac dùng chip Intel (`x86_64`):** `Multi-Zalo-0.1.0-beta.1-macos-x64.dmg`

### 2.2. Kiểm tra & Chạy thử nghiệm
1. Mở `.dmg` đã đối chiếu `SHA-256` để kiểm tra cấu trúc gói `Multi-Zalo Desktop.app`.
2. Trên macOS cấu hình Gatekeeper mặc định, hệ điều hành có thể chặn mở ứng dụng chưa ký/chưa notarized — đây là hành vi bảo vệ chuẩn của macOS đối với bản phát hành nội bộ chưa có Apple Developer ID. Trong trường hợp phát triển/kiểm thử nội bộ trên máy cá nhân, bạn có thể biên dịch trực tiếp từ mã nguồn (`cargo build`) với Xcode Command Line Tools theo đúng quy trình nhà phát triển.
3. Cần cài đặt sẵn `/Applications/Zalo.app` chính thức từ VNG.
4. **Giới hạn kiến trúc macOS đã biết:**
   - Gọi thoại/video: `ZaloCall` sử dụng socket `/tmp/socketzalo*2021`, nên phiên mở sau cùng sẽ giữ quyền lắng nghe socket gọi (chỉ hỗ trợ gọi tuần tự, không đồng thời).
   - Notification Center: Nhóm banner theo `com.vng.zalo` (Dock badge vẫn tách theo từng tiến trình).
   - Chưa ghim `TeamIdentifier` 10 ký tự của VNG trong `MacOsPlatformAdapter` (`PinnedPublisherPolicy = None` -> có thể báo `UNVERIFIED_BINARY` khi mở Zalo thật cho tới khi nghiệm thu trên phần cứng macOS thực tế).

---

## 3. Hướng Dẫn Trên Ubuntu 24.04 LTS x64 (`.deb` & `.AppImage`)

> [!CAUTION]
> **Giới hạn quan trọng trên Linux / Ubuntu:**
> - **VNG KHÔNG phát hành ứng dụng Zalo Desktop chính thức cho Linux (`UNSUPPORTED`).**
> - Bộ cài **Multi-Zalo Desktop** trên Ubuntu (`.deb` và `.AppImage`) là bản **Experimental** chứng minh trình quản lý Tauri + Rust chạy được trên Ubuntu 24.04 và hoạt động với **test fixture** (hoặc bản Electron/AppImage cục bộ do chính người dùng cung cấp).
> - **Tuyệt đối không** đóng gói lại hay phân phối nhị phân Zalo cộng đồng, và **không** tự thêm cờ `--no-sandbox` để vượt AppArmor.

### 3.1. Cài đặt bằng gói `.deb` (Khuyến nghị trên Ubuntu 24.04 LTS)
1. Đối chiếu `SHA-256` của `Multi-Zalo-0.1.0-beta.1-ubuntu24.04-amd64.deb`.
2. Cài đặt cùng các thư viện hệ thống (`libwebkit2gtk-4.1-0`, `libgtk-3-0t64`, `libayatana-appindicator3-1`, `bubblewrap`):
   ```bash
   sudo apt-get update
   sudo apt-get install -y ./Multi-Zalo-0.1.0-beta.1-ubuntu24.04-amd64.deb bubblewrap
   ```
3. Gỡ cài đặt an toàn:
   ```bash
   sudo apt-get remove multi-zalo-desktop
   ```

### 3.2. Chạy thử bằng `.AppImage` (Portable)
1. Cấp quyền thực thi cho tệp:
   ```bash
   chmod +x Multi-Zalo-0.1.0-beta.1-linux-x86_64.AppImage
   ./Multi-Zalo-0.1.0-beta.1-linux-x86_64.AppImage
   ```
2. Nếu hệ thống thiếu `libfuse2t64` trên Ubuntu 24.04, cài thêm `sudo apt-get install -y libfuse2t64` hoặc giải nén chạy trực tiếp bằng `--appimage-extract`.

### 3.3. Cách kiểm tra adapter Linux & fixture
- Kiểm tra trình quản lý mở được giao diện Account Dock (WebKitGTK 4.1), chuyển đổi `vi-VN` / `en-US`, chế độ sáng/tối, và mở màn hình `Cài đặt → Kiểm tra sự cố` để xác nhận bảng hỗ trợ ghi trung thực `Ubuntu LTS (Official Zalo): Không hỗ trợ` và `Ubuntu LTS (Fixture / Local AppImage): Thử nghiệm`.
- Để kiểm thử vòng đời `bwrap` / `XDG_*` với synthetic fixture trên Ubuntu, chạy từ mã nguồn:
  ```bash
  cd desktop-instance-manager
  bash tests/cross-platform/scripts/run-ubuntu.sh --mode release-regression --require-native-os
  ```

---

## 4. Báo Cáo Lỗi Trong Quá Trình Thử Nghiệm

Nếu phát hiện lỗi, vui lòng mở Bug Report tại:
`https://github.com/manhquydev/zalo/issues/new?template=bug_report.md`

Luôn lấy nhật ký từ mục **Cài đặt → Kiểm tra sự cố → Nhật ký hệ thống (đã lọc)** và kiểm tra lại để đảm bảo không chứa tên user cá nhân, cookie, OTP hay dữ liệu tin nhắn Zalo.
