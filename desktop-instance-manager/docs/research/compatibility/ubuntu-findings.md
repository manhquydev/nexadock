# Báo Cáo Nghiên Cứu Tương Thích Đa Phiên Trên Ubuntu (`ubuntu-findings.md`)

**Nhánh nghiên cứu:** `research/desktop-compatibility`  
**Phạm vi mục tiêu:** Ubuntu 22.04 LTS & Ubuntu 24.04 LTS (GNOME Desktop — Wayland mặc định & X11/XWayland).  
**Ranh giới bắt buộc:**
1. **Không** sử dụng Zalo Web (`chat.zalo.me` / trình duyệt PWA) làm phương án thay thế cho Zalo Desktop.
2. **Không** đề xuất tắt AppArmor (`sysctl kernel.apparmor_restrict_unprivileged_userns=0`) hay gắn cờ `--no-sandbox` để đánh dấu tính năng `PASS`.
3. **Không** đóng gói, tải tự động, trích xuất hay phân phối lại tệp `app.asar` độc quyền của VNG.

---

## 1. Nguồn Zalo Desktop Trên Ubuntu & Phân Loại Pháp Lý / Kỹ Thuật

### 1.1. Bản Zalo Desktop chính thức từ VNG: **`UNSUPPORTED` (`CONFIRMED`)**
Kiểm tra trực tiếp mã nguồn gói `app.asar` chính thức (`Zalo 26.10.20`) và các module native unpacked chứng minh VNG **không phát hành và không hỗ trợ Linux**:
- `main-dist/main.js:314392` (`ZaloCall`) và `main.js:982068` (`ZaloCap`) chỉ rẽ nhánh `"win32" === process.platform` và nhánh `else` gọi cứng `ZaloHelper.app/Contents/MacOS/...` của macOS.
- `native/nativelibs/file-utilities/index.js:36` ném lỗi trực tiếp trên Linux: `throw new Error("Unsupported OS: linux, architecture: x64")`.
- `native/nativelibs/db-cross-v4/dist/binding.js:4-14` nếu không phải `"darwin"` sẽ ép nạp tệp PE của Windows (`../prebuilt/window/electron_x86_64/db-cross-v4-native.node`), gây lỗi `ERR_DLOPEN_FAILED` ngay lập tức trên nhân Linux ELF.

### 1.2. Bản cộng đồng `VN-Linux-Family/zalo-for-linux` (`bbb6f99e...`): Chỉ hỗ trợ ở chế độ `EXPERIMENTAL` (User-Supplied Local AppImage)
- Dự án `zalo-for-linux` tải `.dmg` macOS của VNG, giải nén `app.asar`, áp dụng **35 script vá AST/JS** (`scripts/patches/*.js`), biên dịch lại **10 addon Rust N-API** (`db-cross-v4`, `file-utilities`, `file-utils`, `mp4thumb`, `os-sys-info`, `zfile`, `zimage`, `zjxl`, `zlang`, `zwalker`) cùng **2 engine độc lập** (`zcall` + `zcall-audio` và `zocr`), rồi đóng gói thành `AppImage`.
- Do tệp `.AppImage` đầu ra chứa mã `app.asar` thuộc bản quyền VNG, `desktop-instance-manager` **tuyệt đối không phân phối hay tự tải tệp này**, mà chỉ cho phép người dùng tự cung cấp đường dẫn cục bộ (`LaunchTarget::CustomExecutable`) trong chế độ thử nghiệm (`EXPERIMENTAL`).

---

## 2. Phân Tích Chuyên Sâu 8 Khía Cạnh Kỹ Thuật Trên Ubuntu

### 2.1. Vòng đời AppImage (AppImage Lifecycle) trên Ubuntu 22.04 / 24.04

1. **Phụ thuộc FUSE 2 (`libfuse2t64`):**
   - AppImage chuẩn Type-2 liên kết động với `libfuse.so.2` để mount SquashFS vào `/tmp/.mount_ZaloXXXXXX` qua `/dev/fuse`.
   - Trên **Ubuntu 22.04 LTS** và **Ubuntu 24.04 LTS**, hệ điều hành chỉ cài sẵn `fuse3` (`libfuse3.so.3`), **không cài sẵn `libfuse2` / `libfuse2t64`**. Nếu chạy trực tiếp `./Zalo-*.AppImage` trên máy Ubuntu mới cài, tiến trình báo lỗi `dlopen(): error loading libfuse.so.2` trừ khi cài gói hệ thống `libfuse2t64` (Ubuntu 24.04) / `libfuse2` (Ubuntu 22.04).
   - Nếu chạy với biến môi trường `APPIMAGE_EXTRACT_AND_RUN=1` (hoặc `--appimage-extract-and-run`) để không cần FUSE, mỗi phiên Zalo khi mở sẽ tự giải nén toàn bộ ~350–500 MB vào `TMPDIR` rồi xóa khi thoát — gây tốn I/O đĩa, chậm thời gian khởi động thêm vài giây và nhân dung lượng tạm theo số lượng phiên mở đồng thời (`INFERENCE`).
2. **Cập nhật AppImage khi đang mở nhiều phiên:**
   - Khi tệp `.AppImage` trên đĩa được cập nhật bằng thao tác thay thế nguyên tử (`rename`), các phiên đang chạy vẫn giữ mount point FUSE `/tmp/.mount_ZaloXXXXXX` trỏ tới inode cũ cho tới khi đóng hẳn; các phiên mở mới sẽ đọc bản mới. Tuy nhiên nếu ghi đè trực tiếp lên cùng inode (`cp` đè tệp đang mount), các phiên đang mở sẽ gặp lỗi đọc SquashFS (`EIO`).

---

### 2.2. Thông báo Desktop (Desktop Notifications) & Xung đột Unread Badge

1. **Định tuyến thông báo qua D-Bus (`org.freedesktop.Notifications`) (`CONFIRMED` từ `VN-Linux-Family` `main.js:724-728`):**
   - Trên Ubuntu GNOME, thông báo của Electron gửi qua Session D-Bus tới GNOME Shell (`org.freedesktop.Notifications`). Lưu ý trong `zalo-for-linux` (`main.js:725-727`), Electron 22 có lỗi crash trong `Notification::Close()` nếu đi qua XDG Portal backend của `libnotify`, nên cần đặt `NOTIFY_IGNORE_PORTAL=1` khi chạy trong môi trường đóng gói.
   - Khác với Windows (dùng AUMID qua shortcut `.lnk`), giao thức D-Bus `org.freedesktop.Notifications` gắn mỗi thông báo với kết nối D-Bus duy nhất (`:1.xxx`) của tiến trình đã phát thông báo. Khi người dùng bấm vào banner thông báo lúc tiến trình còn chạy, tín hiệu `ActionInvoked` trả về **đúng tiến trình của profile đó** (`INFERENCE` từ chuẩn Desktop Notifications Spec; trạng thái thực nghiệm: `NOT TESTED`).
2. **Xung đột ghi đè số tin chưa đọc trên thanh Dock (`CONFIRMED` tại `patch-notification-badge.js:859-920`):**
   - Để hiển thị số đỏ trên biểu tượng ứng dụng của Ubuntu Dock (`dash-to-dock`), `patch-notification-badge.js` phát tín hiệu D-Bus `com.canonical.Unity.LauncherEntry.Update` cho cùng một định danh `application://zalo.desktop` (hoặc tên tệp `.AppImage`).
   - Khi chạy 2+ phiên từ cùng một tệp `.AppImage`, **mọi phiên đều phát tín hiệu cập nhật `count` đè lên cùng một `application://...desktop`**, khiến con số tin chưa đọc trên Ubuntu Dock nhảy hỗn loạn theo phiên nào vừa cập nhật tiêu đề cửa sổ gần nhất!

---

### 2.3. Âm thanh (Audio / PipeWire / PulseAudio) — Phát hiện Xung đột Cổng Cố Định `29631 / 29632`

1. **Tầng âm thanh hệ thống hoàn toàn hỗ trợ đa phiên (`CONFIRMED` tại `audio_backend.rs:527-633`):**
   - Ubuntu 22.04 (PulseAudio) và Ubuntu 24.04 (PipeWire + `pipewire-pulse`) cung cấp socket âm thanh tại `$XDG_RUNTIME_DIR/pulse/native` (`/run/user/$UID/pulse/native`).
   - Binary `zcall-audio` của `zalo-for-linux` gọi `pa_simple_new` từ `libpulse-simple` (luồng `"ZaloCall"`, `16 kHz`, `s16le` mono). PipeWire/PulseAudio hỗ trợ trộn nhiều luồng thu/phát đồng thời mà không khóa độc quyền thiết bị.
2. **PHÁT HIỆN ĐIỂM NGHẼN MỚI TRONG `zalo-for-linux` (`CONFIRMED` tại `scripts/patches/patch-zcall-native.js:306-318`):**
   - Mặc dù tài liệu `zcall/README.md` mô tả chung chung là dùng *"the two loopback ports"*, kiểm tra trực tiếp mã nguồn `scripts/patches/patch-zcall-native.js` dòng `307-308` xác nhận bản vá đã **gán cứng hai cổng TCP cố định trên `127.0.0.1`**:
     - `29631` cho `serverRecv` (`helper -> Zalo`)
     - `29632` cho `serverSend` (`Zalo -> helper`)
   - Đồng thời dòng `314-318` vô hiệu hóa nhánh khôi phục khi gặp `EADDRINUSE`, và giống hệt bản gốc của VNG, hai `net.Server` này **không bao giờ đóng lại sau khi khởi tạo**!
   - Thêm vào đó, `plugins/zcall/index.js:46-60` quét `/proc/<pid>/cmdline` toàn cục theo đường dẫn `zcall-native.js`.
   - **Hệ quả trực tiếp:** Nếu chạy 2 phiên `zalo-for-linux` AppImage trên cùng không gian mạng loopback `127.0.0.1` của host Ubuntu, **phiên thứ hai sẽ bị lỗi `EADDRINUSE` trên cổng `29631`/`29632` và không thể thực hiện cuộc gọi thoại/video** — tương tự lỗi `PipeZCall*` trên Windows!

---

### 2.4. Camera (`V4L2` `/dev/video*` vs PipeWire)

- Trong `zalo-for-linux` (`plugins/zcall/window.js:419-435`), luồng thu hình camera cho gọi video 1-1 và gọi nhóm chạy trong cửa sổ `BrowserWindow` (`ui/call.html`) thông qua API WebRTC `getUserMedia` + `WebCodecs` (mã hóa H.264 baseline).
- Trên Electron 22 / Chromium 108 cho Linux, `getUserMedia` cho webcam đọc trực tiếp qua driver nhân **Video4Linux2 (`V4L2`, `/dev/video0`)** (cờ `WebRTCPipeWireCapturer` được `plugins/zcall/index.js:195` bật chỉ áp dụng cho chia sẻ màn hình Wayland qua XDG Desktop Portal, không áp dụng cho webcam trên Chromium 108).
- Driver `uvcvideo` của Linux áp đặt **khóa độc quyền luồng thu (`EBUSY` khi gọi `VIDIOC_STREAMON`)** trên mỗi node `/dev/video0`:
  - Gọi video tuần tự (phiên A cúp máy rồi phiên B gọi — nếu không vướng cổng `29631/29632`): thiết bị `/dev/video0` được giải phóng khi `call.html` đóng.
  - Gọi video đồng thời 2 phiên trên cùng 1 webcam vật lý: phiên thứ hai sẽ không mở được camera (`NotReadableError` / `Device in use`).

---

### 2.5. XDG User Directories & Lưu trữ khóa đăng nhập (`gnome-keyring`)

1. **Biến môi trường XDG & `user-dirs.dirs` (`CONFIRMED` từ `patch-xdg-user-dirs.js:395-465` & `crates/platform-linux/src/layout.rs:46-78`):**
   - Electron trên Linux tính `app.getPath("appData")` từ `$XDG_CONFIG_HOME` (mặc định `$HOME/.config`), nên `ZaloData` nằm tại `<profile_dir>/.config/ZaloData`.
   - Tuy nhiên, khi `HOME=<profile_dir>` và `XDG_CONFIG_HOME=<profile_dir>/.config`, Chromium tra cứu đường dẫn `Downloads` và `Documents` trong tệp `<profile_dir>/.config/user-dirs.dirs`.
   - Hiện tại `ensure_linux_profile_layout` (`crates/platform-linux/src/layout.rs:22-42`) **chưa tạo thư mục `Downloads`, `Documents` và chưa tạo tệp `.config/user-dirs.dirs`**, khiến hộp thoại lưu/gửi tệp và thư mục nhận tệp mặc định rơi vào `$HOME` trống của profile hoặc báo lỗi đường dẫn chưa tồn tại!
2. **Giữ phiên đăng nhập với `gnome-keyring` / `org.freedesktop.secrets` (`INFERENCE` từ kiến trúc Electron Linux):**
   - Electron 22 trên Ubuntu GNOME lưu khóa bọc `"Chrome Safe Storage"` / `"Zalo Safe Storage"` vào `gnome-keyring-daemon` thông qua Session D-Bus (`org.freedesktop.secrets`).
   - Khác với macOS (kiểm tra chữ ký CDHash), `org.freedesktop.secrets` trên D-Bus phiên truyền thống cho phép mọi tiến trình chạy dưới cùng UID truy cập collection `login` đang mở khóa theo tên ứng dụng (`app.setName("Zalo")`), trong khi dữ liệu cookie và `secure_ks_*` thực tế nằm tách biệt trong `<profile_dir>/.config/ZaloData`. Nhờ đó các phiên có thể giữ đăng nhập độc lập qua các lần khởi động lại miễn là tiến trình vẫn kết nối được tới socket D-Bus phiên (`$DBUS_SESSION_BUS_ADDRESS`).

---

### 2.6. Cô lập Filesystem & 5 Lỗi Nghiêm Trọng Phát Hiện Trong `crates/platform-linux`

Kiểm tra trực tiếp mã nguồn `crates/platform-linux/src/layout.rs` và `adapter.rs` phát hiện **5 lỗi thiết kế của Adapter sẽ làm hỏng tính năng ngay khi chạy thực tế trên Ubuntu** (`CONFIRMED`):

1. **Lỗi 1 (`P0`) — Cờ `--die-with-parent` trong `build_bwrap_args` (`layout.rs:84`) tự giết chết Zalo ngay khi lệnh CLI kết thúc:**
   - Khi người dùng hoặc bridge gọi `dim-cli launch <id>`, tiến trình `dim-cli` spawn `bwrap --die-with-parent ...` rồi thoát sau vài chục mili-giây để trả kết quả JSON. Ngay khoảnh khắc `dim-cli` thoát, nhân Linux gửi `SIGKILL` (`PR_SET_PDEATHSIG`) hủy toàn bộ phiên Zalo bên trong `bwrap`!
2. **Lỗi 2 (`P0`) — Sandbox `bwrap` (`layout.rs:82-131`) thiếu toàn bộ socket hiển thị, âm thanh, D-Bus và thiết bị phần cứng:**
   - `build_bwrap_args` hiện chỉ bind `/usr`, `/lib`, `/lib64`, `/bin`, `/etc/ssl`, `/etc/resolv.conf`, `--proc /proc`, `--dev /dev` và `<profile_dir>`.
   - Nó **KHÔNG bind**:
     - `$XDG_RUNTIME_DIR` (`/run/user/$UID`) -> mất socket Wayland (`wayland-0`), PipeWire (`pipewire-0`), PulseAudio (`pulse/native`), Session D-Bus (`bus` -> mất thông báo, mất `gnome-keyring`, mất portal chụp màn hình)!
     - `/tmp/.X11-unix` và `$XAUTHORITY` -> mất kết nối X11/XWayland, cửa sổ GUI không thể mở (`Missing X server or $DISPLAY`)!
     - `/etc/fonts`, `/etc/machine-id`, `/var/lib/dbus/machine-id` -> lỗi vỡ font chữ ô vuông và lỗi khởi tạo D-Bus/Chromium!
     - `/dev/dri` (tăng tốc phần cứng GPU Mesa/Vulkan) và `/dev/video*` (webcam) — vì `--dev /dev` của `bwrap` chỉ tạo `/dev` giả lập tối giản (`null`, `zero`, `urandom`, `tty`), không có GPU hay camera!
     - `/dev/fuse` -> tệp `.AppImage` không thể tự mount FUSE bên trong `bwrap`!
3. **Lỗi 3 (`P1`) — `build_linux_isolated_env` (`layout.rs:46-78`) kết hợp `Command::envs` có nguy cơ thiếu biến môi trường hiển thị khi lọc sạch env:** Dù `Command::envs` kế thừa env cha, nhưng trong nhánh `bwrap`, nếu không truyền rõ hoặc không bind đường dẫn tương ứng của `XDG_RUNTIME_DIR`, `DBUS_SESSION_BUS_ADDRESS`, `WAYLAND_DISPLAY`, `DISPLAY`, ứng dụng không thể kết nối tới desktop session.
4. **Lỗi 4 (`P1`) — `focus_or_restore` là hàm rỗng `Ok(())` (`adapter.rs:310-318`):** Bấm chuyển cửa sổ trên Linux hoàn toàn không thực hiện bất kỳ hành động nào! (Trong khi có thể kích hoạt lại thông qua cơ chế chạy lần 2 cùng biến môi trường để kích hoạt sự kiện `second-instance` của Electron).
5. **Lỗi 5 (`P1`) — `stop()` bắn `SIGKILL` không kiểm tra cơ sở dữ liệu đang mở (`adapter.rs:356-359`):** Nếu sau `timeout_ms` (ví dụ máy chậm khi flush SQLite WAL lớn) mà `main_pid` chưa kịp tắt, `stop()` gọi thẳng `libc::kill(raw_pid, libc::SIGKILL)`, có nguy cơ làm hỏng cơ sở dữ liệu tin nhắn!

---

### 2.7. Tương thích với môi trường GNOME trên Ubuntu 22.04 / 24.04

1. **Khay hệ thống (System Tray) giữa Ubuntu GNOME và Vanilla GNOME (`CONFIRMED` tại `tray-host/index.js:700-750` & `main.js:649-662`):**
   - Bản Ubuntu Desktop chính thức cài sẵn extension `ubuntu-appindicators` (`org.kde.StatusNotifierWatcher`), nên các phiên chạy song song đều hiện biểu tượng Zalo trên góc phải thanh Top Bar (nhưng mọi biểu tượng đều giống hệt nhau, không phân biệt được tên tài khoản).
   - Trên phiên **GNOME thuần** (`gnome-session` không cài extension AppIndicator), `StatusNotifierWatcher` vắng mặt -> khi người dùng bấm nút đóng `X`, `zalo-for-linux` **tự động thoát hẳn ứng dụng (`app.quit()`)** thay vì ẩn xuống nền.
2. **Giới hạn của Wayland trên GNOME (`CONFIRMED` tại `main.js:619` + kiến trúc Mutter):**
   - Giao thức `xdg_toplevel` trên Wayland không cho phép ứng dụng tự đặt tọa độ cửa sổ trên màn hình (`canPosition: !global.__zaloNativeWayland`).
   - Khi kích hoạt cửa sổ nền từ một tiến trình quản lý khác mà không có token `xdg_activation_v1` hợp lệ từ cửa sổ đang giữ tiêu điểm, GNOME Shell (Mutter) chặn đánh cắp tiêu điểm (*Focus Stealing Prevention*) và chỉ hiện thông báo `"Zalo is ready"` thay vì tự động bật cửa sổ đè lên ứng dụng đang gõ phím.
3. **Dán ảnh từ Clipboard (`Ctrl+V`):** Trên phiên Wayland của Ubuntu, tính năng dán ảnh chụp vào khung chat của `zalo-for-linux` yêu cầu gói `wl-clipboard` (`wl-paste`); trên X11 yêu cầu `xclip` (`known-issues.md:110`).

---

### 2.8. Giới hạn AppArmor trên Ubuntu 23.10+ / 24.04 LTS (Tuyệt Đối Không Bypass)

1. **Cơ chế chặn của nhân Ubuntu 24.04 (`CONFIRMED`):**
   - Ubuntu 24.04 LTS bật mặc định `kernel.apparmor_restrict_unprivileged_userns = 1`. Mọi tiến trình không có AppArmor profile cấp quyền `userns,` tường minh khi gọi `unshare(CLONE_NEWUSER)` sẽ bị từ chối với `EACCES` (`Permission denied`).
   - **Tác động lên Electron AppImage:** Khi một tệp `.AppImage` mount tạm vào `/tmp/.mount_ZaloXXXXXX/zalo` (đường dẫn ngẫu nhiên mỗi lần chạy!), đường dẫn này không khớp với bất kỳ AppArmor profile có sẵn nào trong `/etc/apparmor.d/`. Do đó `chrome-sandbox` của Chromium thất bại ngay lúc khởi động (`FATAL:zygote_host_impl_linux.cc: No usable sandbox!`).
   - **Tác động lên `bwrap` (`bubblewrap`):** Trên Ubuntu 24.04, gói `bubblewrap` có profile `/etc/apparmor.d/bwrap-userns-restrict` cho phép bản thân `bwrap` tạo user namespace, nhưng áp đặt hạn chế chuyển tiếp quyền bên trong sandbox khiến Electron bên trong `bwrap` vẫn không thể tự tạo nested userns sandbox của Chromium!
2. **Ranh giới an toàn bắt buộc:**
   - **Loại bỏ hoàn toàn:** Không bao giờ tự động chèn `--no-sandbox` để biến kết quả kiểm thử thành `PASS`, và không bao giờ hướng dẫn người dùng tắt bảo vệ nhân (`sysctl -w kernel.apparmor_restrict_unprivileged_userns=0`).
   - **Hướng tiếp cận hợp chuẩn duy nhất trên Ubuntu 24.04:**
     - Đặt tệp thực thi/AppImage tại một đường dẫn cố định thuộc quyền quản trị kiểm soát (ví dụ `/opt/zalo-appimage/Zalo.AppImage` hoặc thư mục giải nén cố định) và quản trị viên hệ thống cài đặt một **AppArmor profile riêng cho đường dẫn cố định đó** tại `/etc/apparmor.d/opt.zalo` (cấp quyền `userns,` theo đúng chuẩn mà gói chính thức của Ubuntu 24.04 áp dụng cho `/etc/apparmor.d/chrome` và `/etc/apparmor.d/code`).
     - Nếu máy Ubuntu 24.04 chưa được quản trị viên cấu hình AppArmor profile hợp lệ cho đường dẫn binary đó, `dim-platform-linux` phải kiểm tra preflight (`/proc/sys/kernel/apparmor_restrict_unprivileged_userns`) và báo cáo trung thực trạng thái **`BLOCKED` (AppArmor `userns` policy)** thay vì hạ cấp bảo mật bằng `--no-sandbox`.

---

## 3. Tóm Tắt Đánh Giá Trên Ubuntu

| Đối tượng | Trạng thái tổng thể | Nguyên nhân cốt lõi |
| :--- | :--- | :--- |
| **Zalo Desktop chính thức từ VNG** | **`UNSUPPORTED`** (`CONFIRMED`) | VNG không phát hành bản Linux; `app.asar` và `.node` addons chỉ hỗ trợ `win32` và `darwin`. |
| **AppImage do người dùng tự cung cấp (ví dụ `zalo-for-linux`)** | **`EXPERIMENTAL` / `BLOCKED` mặc định trên Ubuntu 24.04 chưa cấu hình AppArmor** | (1) Cần sửa 5 lỗi trong `dim-platform-linux` (`--die-with-parent`, thiếu bind Wayland/PipeWire/D-Bus/X11, thiếu `Downloads`/`user-dirs.dirs`, `focus` rỗng, `SIGKILL` sớm); (2) trên Ubuntu 24.04 cần `libfuse2t64` + AppArmor `userns` profile hợp chuẩn; (3) gọi thoại/video đa phiên vẫn vướng cổng cố định `127.0.0.1:29631/29632` và khóa độc quyền V4L2 `/dev/video0`. |
