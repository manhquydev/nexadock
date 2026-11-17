# Báo Cáo Đối Chiếu `ak-xia --compare` & Kiểm Tra Năng Lực REA MCP Cho Các Tính Năng Còn Thiếu

**Nhánh nghiên cứu:** `research/desktop-compatibility`  
**Ngày thực hiện:** `2026-11-17`  
**Phạm vi:** Kiểm kê thực tế các công cụ REA MCP hoạt động trên host Windows 11 x64 và đối chiếu chuyên sâu (`ak-xia --compare`) 4 dự án tham chiếu ở **những nhóm tính năng còn thiếu** (gọi thoại/video, thông báo, hộp thoại tệp, kích hoạt cửa sổ, giữ phiên Keychain/DPAPI, cập nhật ứng dụng, tương thích GNOME/Wayland/AppArmor). Không lặp lại lịch sử kiến trúc cơ bản đã có tại [`../../../docs/research/repository-comparison.md`](../../../docs/research/repository-comparison.md).

---

## 1. Trạng Thái Kết Nối & Công Cụ REA MCP Đã Xác Nhận Hoạt Động

Kiểm tra trực tiếp phiên `rea` (`rea-agents@6.9.0`, giao thức MCP `2026-07-28`, `255` MCP tools, `196` CLI commands) qua `binary_session(detail: "complete")`:

### 1.1. Nhóm công cụ hoạt động ổn định và đã sinh bằng chứng mới (`CONFIRMED`)

| Công cụ REA MCP | Trạng thái | Đối tượng kiểm tra mới trong nhiệm vụ này | Mã bằng chứng (`evidence_id`) |
| :--- | :--- | :--- | :--- |
| `binary_session` | `available` | Kiểm kê toàn bộ 255 tool, provider candidates và lý do chặn native addon | Phiên trực tiếp (`2026-11-17T23:12+07:00`) |
| `get_evidence_bundle` | `available` | Đối chiếu 3 bản ghi chứng chỉ PE của `dim-desktop.exe`, `dim-cli.exe`, `dim-test-fixture.exe` | `ev_1fbba146...`, `ev_7d139887...`, `ev_eb437959...` |
| `inspect_windows_shortcut` | `available` | `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Zalo.lnk` (`2.241` bytes) | `ev_69ddf74bf085b7e9d83f9f16f5781daf38ec90a7735065b49b55b6eee788316a` |
| `inspect_pe_imports` | `available` | `plugins\capture\ZaloCall.exe` (`19.588.136` bytes, Qt5 + WinMM + NamedPipe) | `ev_9145f8bd574a583b54d530119265f1862c17e899a13502359500acb527cceda0` |
| `inspect_pe_imports` | `available` | `plugins\capture\ZaviMeet.exe` (`19.822.120` bytes, `SetCurrentProcessExplicitAppUserModelID`) | `ev_b7f40dd7702c3f4e62e023559ad7f7bf097cd9c0dfc24f1dccf922c45e0373f7` |
| `inspect_pe_imports` | `available` | `sl.exe` (`186.528` bytes, `CreateMutexW`, `ShellExecuteW`) | `ev_58587a3b0711c9c3633d75d87ced2ae64cfc2e775bbbc8cb8363fe9713545cf6` |
| `inspect_pe_imports` | `available` | `Update.exe` (`833.576` bytes, `OpenMutexW`/`CreateMutexW`, CryptoAPI hash) | `ev_9f322e5a35397df9d06f559cb0a78083d9ea5f94109a67863226f5a0e28f63c6` |
| `inspect_pe_imports` & `inspect_native_exports` | `available` | `zcall_ia32.node` (`zvideocall_node.node`), `win-utils.node`, `file-utilities.node` | `ev_e01e41ea...`, `ev_8c12c7c1...`, `ev_5bf5ad79...`, `ev_6b5285f8...` |
| `inspect_desktop_application` | `available` | Thư mục cài đặt `Zalo-26.10.20` (`345` file, `132` probed) | `ev_1c421fc5...` (parent `ev_a7236176...`) |

*(Kết hợp với bộ bằng chứng nền tảng đã lưu tại [`../../../../multi-zalo-desktop/evidence/rea-evidence-summary.json`](../../../../multi-zalo-desktop/evidence/rea-evidence-summary.json): `ev_8e05df8e...` Electron fuses, `ev_fce14a1d...` PE hardening, `ev_70919879...` ASAR graph).*

### 1.2. Nhóm công cụ không khả dụng trên host và ranh giới tuân thủ (`CONFIRMED`)

- **Native addon `rea-windows-x64.node` bị chặn bởi Application Control (SAC/WDAC):** Khi gọi `inspect_sqlite_database` trên `zavi_db.db` hoặc các công cụ phụ thuộc `rea-windows-x64.node` (`analyze_javascript_application`, Windows Ghidra bridge), Windows từ chối nạp `rea-windows-x64.node` với thông báo `An Application Control policy has blocked this file`. Tuân thủ nghiêm ngặt chính sách bảo mật: **không tắt SAC, không can thiệp WDAC hay ép nạp addon chưa ký**.
- **Provider khác hệ điều hành / chưa cấu hình:** `native-macos` (`inspect_macho`, `inspect_plist`, `inspect_signature`), `hopper`, và các công cụ Linux (`inspect_binary_layout`, `start_gdb_session`) trả `unsupported_host` trên `win32`; `ghidra`, `ida`, `kuna` trả `not_configured`.

---

## 2. Manifest Nguồn Đối Chiếu (`ak-xia --compare`)

| Mã | Repository | Commit ghim kiểm tra | Giấy phép | Phạm vi đối chiếu bổ sung trong báo cáo này |
| :--- | :--- | :--- | :--- | :--- |
| **Repo B** | [`ducngfi/zalo-multi-accounts`](https://github.com/ducngfi/zalo-multi-accounts) | `0b3c5aa4416760427d4721e0b0dac752b43e52be` | `MIT` | Hộp thoại tệp (NTFS junctions / macOS symlinks), kích hoạt cửa sổ, treo màn hình đăng nhập khi thoát, Keychain, `db_path` ngoài, giới hạn cuộc gọi thực nghiệm trên macOS 26.1 & Windows 10 |
| **Repo A** | [`kemonra/ZaloMulti`](https://github.com/kemonra/ZaloMulti) | `c71e7f2209647721f299a1ef464316d6de2502e1` | Không có (`All rights reserved`) | Shortcut `.lnk` riêng theo màu icon (`Assets/zalo_01..10_*.ico`), thiếu cô lập `TEMP`, `Stop-Process -Force` gây hỏng WAL |
| **Repo C** | [`VN-Linux-Family/zalo-for-linux`](https://github.com/VN-Linux-Family/zalo-for-linux) | `bbb6f99e4fe5ead55895825ebc5387181ebf21c5` | `BSD-3-Clause` (code) + VNG `app.asar` độc quyền | Kiến trúc `zcall-native` + `zcall-audio` (cổng cố định `127.0.0.1:29631/29632`), `NOTIFY_IGNORE_PORTAL=1`, `patch-notification-badge.js`, `patch-xdg-user-dirs.js`, `tray-host` trên GNOME, AppArmor Ubuntu 24.04 |
| **Repo D** | [`sandboxie-plus/Sandboxie`](https://github.com/sandboxie-plus/Sandboxie) | `f23a6cfd50f506413634e8f2906bfc2888d2b1c3` | `GPL-3.0` + `LICENSE.Plus` | Ảo hóa không gian tên `\Device\NamedPipe\<Box>\` (`SbieDrv.sys` + `SbieDll.dll`) so với giới hạn user-mode `asInvoker` |

---

## 3. Phân Tích Chuyên Sâu Các Khoảng Trống Tính Năng Từ 4 Repository

### 3.1. Hộp thoại chọn/lưu tệp (File Dialogs) & Thư mục nhận tệp (`Repo B` vs `Repo C` vs `desktop-instance-manager`)

1. **Cách `ducngfi/zalo-multi-accounts` xử lý (`CONFIRMED`):**
   - Trên macOS (`ProfileManager.swift:1119-1125`), `prepareHome()` tạo symlink cho `Library/Keychains`, `Downloads`, `Desktop`, `Documents`, `Pictures`, `Movies`, `Music` trỏ về thư mục thật của người dùng (`realHome`).
   - Trên Windows (`how-it-works.md:908`), mỗi hồ sơ tạo NTFS junction (không cần quyền Admin) tới `Desktop`, `Documents`, `Downloads`, `Pictures`, `Videos`, `Music` thật; khi xóa hồ sơ sẽ gỡ junction trước khi đưa vào Recycle Bin.
   - **Hệ quả UX:** Khi người dùng bấm gửi tệp (`showOpenDialog`) hoặc lưu tệp (`showSaveDialog`), hộp thoại hệ thống mở ngay đúng `Downloads`/`Documents`/`Desktop` thật của người dùng, và tệp tải về đi vào thư mục quen thuộc. Đổi lại, các phiên dùng chung thư mục `Zalo Received Files` / `Downloads`.
2. **Cách `VN-Linux-Family/zalo-for-linux` xử lý (`CONFIRMED`):**
   - `scripts/patches/patch-xdg-user-dirs.js:395-465` vá hàm lấy đường dẫn của module `IpzU` và service `VVt6` để đọc `app.getPath("documents")` và `app.getPath("downloads")` theo chuẩn XDG (`$XDG_DOWNLOAD_DIR`) thay vì ép cứng `~/Documents/Zalo Received Files`.
3. **Khoảng trống trong `desktop-instance-manager` hiện tại (`CONFIRMED`):**
   - `crates/platform-windows/src/layout.rs:14-29` đặt `USERPROFILE=<ProfileDir>` nhưng chỉ tạo thư mục rỗng `Desktop` và `Documents\Zalo Received Files` bên trong profile, **không** tạo `Downloads`, `Pictures`, `Videos` và chưa có tùy chọn bật junction sang thư mục thật. Hộp thoại mở tệp của Windows Shell giải tham chiếu `%USERPROFILE%\Downloads` sẽ rơi vào đường dẫn không tồn tại hoặc thư mục rỗng khó tìm.
   - `crates/platform-linux/src/layout.rs:22-35` chỉ tạo `.config/ZaloData`, `.local/share`, `.cache`, `tmp` mà chưa tạo `Downloads`, `Documents` hay `.config/user-dirs.dirs`.

### 3.2. Gọi thoại & Gọi video (`Repo B` vs `Repo C` vs `Repo D` vs Phân tích `app.asar` thực tế)

1. **Phát hiện mới vượt qua mô tả của `Repo B` (`CONFIRMED`):**
   - `ducngfi` (`README.md:788,998`) ghi nhận: *"Gọi thoại và gọi video chỉ ổn định ở một cửa sổ"* và khuyên đóng các cửa sổ khác rồi mở lại cửa sổ cần gọi.
   - Tuy nhiên, kết hợp kiểm tra `main.js:311828..317500` (module `"3Zc2"`) và kiểm tra tiến trình/pipe thực tế trên host Windows 11 cho thấy cơ chế xung đột **nghiêm trọng và bất đối xứng giữa Windows và macOS**:
     - **Trên Windows:** Phiên Zalo đầu tiên đăng nhập (ví dụ Zalo chính `PID 12980`) khởi chạy sẵn `ZaloCall.exe` (`PID 15624`) **ngay ở trạng thái nghỉ (idle)** và giữ cố định `\\.\pipe\PipeZCallRecv` + `\\.\pipe\PipeZCallSend` suốt vòng đời phiên (hàm `C.on("exit")` không bao giờ đóng server `I` và `A`). Khi mở phiên thứ hai và đăng nhập, `I.listen` và `A.listen` của phiên 2 bị lỗi `EADDRINUSE`, nhưng phiên 2 **vẫn spawn `ZaloCall.exe` con** kết nối thẳng vào pipe server của phiên 1 (`uv_pipe_t` mở `PIPE_UNLIMITED_INSTANCES`), ghi đè biến hàm gửi `O = t => V(e, t)` của phiên 1! Hệ quả: phiên 2 **không thể gọi dù phiên 1 đang rảnh**, đồng thời còn làm lệch kênh điều khiển `ZaloCall.exe` của phiên 1!
     - **Trên macOS:** Nhánh `!g` trong `main.js:314800` gọi `fs.unlink("/tmp/socketzalorecv2021")` và `fs.unlink("/tmp/socketzalosend2021")` khi gặp `EADDRINUSE`. Do đó, **phiên khởi tạo sau cùng** chiếm quyền nhận kết nối mới trên `/tmp/socketzalo*2021` (xác nhận khớp với ghi chú thực nghiệm của `ducngfi` trên macOS 26.1 ngày `20/10/2026`).
2. **Phát hiện mới trong `VN-Linux-Family/zalo-for-linux` (`CONFIRMED`):**
   - Dù `zcall/window.js` dùng cổng ngẫu nhiên (`ZCALL_UI_PORT`) cho cửa sổ gọi, tệp vá `scripts/patches/patch-zcall-native.js:307-318` lại **gán cứng 2 cổng TCP loopback `127.0.0.1:29631` (recv) và `127.0.0.1:29632` (send)** và tắt nhánh `unlink` khi `EADDRINUSE`!
   - Do đó, ngay cả với bản `zalo-for-linux` AppImage chạy nhiều phiên trên cùng network namespace của Ubuntu, **phiên thứ hai cũng bị xung đột `EADDRINUSE` trên `127.0.0.1:29631` và `29632`** — trừ khi mỗi phiên chạy trong network namespace riêng có loopback độc lập (ví dụ rootless `pasta`/`slirp4netns`) hoặc upstream `zalo-for-linux` đổi sang cổng động `0`!
3. **Vì sao loại bỏ hướng của `Repo D` (`Sandboxie Plus`) (`CONFIRMED`):**
   - `SbieDrv.sys` + `SbieDll.dll` (`file_pipe.c`) giải quyết được `PipeZCall*` nhờ viết lại đường dẫn pipe ở tầng kernel/hook `NtCreateNamedPipeFile`, nhưng đòi hỏi quyền Admin, cài driver mức nhân ký EV/WHQL, hook tiến trình trái nguyên tắc an toàn và mang giấy phép `GPL-3.0`.

### 3.3. Kích hoạt cửa sổ (Window Activation), Đóng xuống khay & Thoát an toàn (`Repo B` & `Repo C`)

1. **Cách `ducngfi` kích hoạt cửa sổ (`CONFIRMED`):**
   - **Windows (`ZaloControl.cs:1539-1544`):** Không dùng `EnumWindows` + `SetForegroundWindow` (dễ chọn nhầm cửa sổ phụ hoặc bị chặn Foreground Lock), mà gọi lại `Zalo.exe` với cùng biến môi trường của profile đó để cơ chế `second-instance` chính chủ của Zalo (`main.js:3565794`) tự mở lại cửa sổ từ System Tray và đưa lên trên cùng (`setAlwaysOnTop(true)` -> `focus()`).
   - **macOS (`ZaloControl.swift:1020-1022`):** Gọi trực tiếp `NSRunningApplication(processIdentifier: pid)?.activate()` của AppKit (không cần quyền Accessibility TCC của `System Events` như cách gọi `osascript` hiện tại trong `dim-platform-macos`).
2. **Xử lý treo tiến trình ở màn hình đăng nhập macOS (`CONFIRMED` trong `ZaloControl.swift:965-985`):**
   - Trên macOS, khi Zalo đang ở màn hình QR (chưa đăng nhập) mà nhận lệnh `terminate()`, Zalo đóng cửa sổ renderer nhưng tiến trình chính đôi khi không thoát. `ducngfi` kiểm tra không còn `--type=renderer` suốt `> 3 giây` **và** kiểm tra `lsof` xác nhận không mở tệp nào trong `Database/_production/<uid>/` trước khi gửi `SIGTERM`. Ngược lại, `crates/platform-macos/src/adapter.rs:380-394` hiện đang gửi `SIGTERM` ngay lập tức sau `osascript`, tiềm ẩn nguy cơ ngắt ngang quá trình đóng DB của tài khoản đang đăng nhập!
3. ** Hành vi khay hệ thống (System Tray) trên Ubuntu GNOME (`CONFIRMED` trong `Repo C` `tray-host/index.js:700-750` & `main.js:649-662`):**
   - `zalo-for-linux` kiểm tra D-Bus `org.kde.StatusNotifierWatcher` qua `gdbus`. Trên Ubuntu Desktop mặc định (có sẵn extension `ubuntu-appindicators`), đóng cửa sổ sẽ ẩn xuống khay; trên GNOME thuần (không có extension AppIndicator), đóng cửa sổ sẽ **thoát hẳn ứng dụng** để tránh kẹt tiến trình ngầm không thể mở lại.

### 3.4. Chữ ký ứng dụng, Mã hóa Keychain/DPAPI & Chốt chặn `db_path` ngoài (`Repo B` + `app.asar`)

1. **`SecureCipher` & `checkAppSigned()` (`CONFIRMED` tại `main.js:3561081..3563611`):**
   - Zalo kiểm tra chữ ký ứng dụng và dùng `electron.safeStorage` để bọc khóa ngẫu nhiên 32 byte (`secure_ks_<hash>`) lưu riêng trong `electron-store` của từng thư mục `ZaloData`.
   - Trên macOS, symlink `Library/Keychains -> ~/Library/Keychains` kết hợp giữ nguyên `/Applications/Zalo.app` có chữ ký VNG giúp mọi phiên đọc chung mục `"Zalo Safe Storage"` trong `login.keychain-db` mà không ghi đè khóa dữ liệu của nhau.
2. **Cảnh báo cấu hình lưu tin nhắn ở ổ đĩa ngoài (`CONFIRMED` tại `ducngfi` `how-it-works.md:995`):**
   - Trong Cài đặt Zalo Windows, người dùng có thể đổi nơi lưu tin nhắn (`config.json -> db_path.location`) và media (`resource.location.path`) ra ngoài `ZaloData`. `desktop-instance-manager` đã đọc `db_path.location` để phát hiện `zalo_user_id`, nhưng **chưa từ chối/cảnh báo khi sao lưu (`backup`) một profile có `db_path.location` trỏ ra ngoài thư mục profile**, cũng như chưa kiểm tra xung đột hai profile cùng trỏ `db_path.location` / `resource.location.path` vào một thư mục dùng chung!

---

## 4. Ma Trận Quyết Định Thách Thức (`ak-xia` Challenge Matrix — Missing Features)

| Vấn đề kỹ thuật | Cách của Repo tham chiếu | Hiện trạng `desktop-instance-manager` | Quyết định kiến trúc đề xuất |
| :--- | :--- | :--- | :--- |
| **Kích hoạt cửa sổ Windows (kể cả khi ẩn ở Tray)** | `Repo B`: Gọi lại `Zalo.exe` trong env cô lập để kích hoạt `second-instance` | `EnumWindows` chọn HWND đầu tiên (dễ dính `notiWin` 352x82) + fallback spawn khi `hwnd == None` | **Kết hợp an toàn:** Lọc chặt class/title/owner cho `Chrome_WidgetWin_1` chính; ưu tiên cơ chế handoff `second-instance` khi ẩn ở Tray hoặc khi bị chặn Foreground Lock |
| **Hộp thoại tệp & Thư mục người dùng Windows** | `Repo B`: Luôn tạo NTFS junction tới 6 thư mục người dùng thật | Không tạo `Downloads`, không có junction -> hộp thoại tệp mở vào thư mục rỗng | **Cải tiến có kiểm soát:** Tạo đầy đủ `Downloads`/`Pictures`/`Videos` cô lập mặc định + hỗ trợ cờ opt-in `--link-user-folders` (tạo junction tới `Downloads`/`Desktop` thật nhưng giữ riêng `Zalo Received Files`, gỡ junction an toàn khi xóa) |
| **Giới hạn `TMPDIR` & `SIGTERM` trên macOS** | `Repo B`: Chỉ đặt `CFFIXED_USER_HOME`, giữ `/var/folders` ngắn; chỉ `SIGTERM` khi không mở DB | Đặt `TMPDIR=<profile>/tmp` (>104 byte dễ vỡ `SingletonSocket`) và gửi `SIGTERM` ngay sau `terminate()` | **Sửa bắt buộc trong `dim-platform-macos`:** Bỏ ghi đè `TMPDIR` dài (hoặc dùng `/tmp/dim-<id>`), xác thực `bundleIdentifier`, và chờ `terminate()` trọn thời gian trước khi xét `SIGTERM` |
| **Chạy AppImage trên Ubuntu 24.04 & `bwrap`** | `Repo C`: Đòi `--no-sandbox` hoặc tắt AppArmor userns; gán cứng cổng `29631/29632` | `bwrap` thiếu socket Wayland/PipeWire/D-Bus và đặt `--die-with-parent`; `focus` là no-op | **Không dùng `--no-sandbox` để đánh PASS:** Sửa `bwrap` (bỏ `--die-with-parent`, bind socket desktop), tài liệu hóa profile AppArmor hợp chuẩn `/etc/apparmor.d/`, và ghi nhận rõ giới hạn cổng `29631/29632` |
| **Sao lưu khi `db_path` nằm ngoài `ZaloData`** | `Repo B`: Từ chối sao lưu nếu `db_path.location` nằm ngoài `ZaloData`, chặn 2 profile trùng `db_path` | Mới chỉ đọc `db_path.location` để tìm `zalo_user_id` | **Bổ sung kiểm tra preflight & backup guard** trong `dim-core` / `dim-profile-store` để tránh bản sao lưu thiếu cơ sở dữ liệu tin nhắn |
