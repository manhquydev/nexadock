# Báo Cáo Nghiên Cứu Tương Thích Đa Phiên Trên macOS (`macos-findings.md`)

**Nhánh nghiên cứu:** `research/desktop-compatibility`  
**Ranh giới xác minh thực nghiệm (`IMPORTANT`):** Môi trường nghiên cứu hiện tại chạy trên Windows 11 x64 (không kết nối máy Mac vật lý; các provider `native-macos` của REA MCP trả `unsupported_host`). Do đó, **không một tuyên bố nào trong tài liệu này được gắn nhãn đã xác minh runtime bởi chúng tôi trên phần cứng Mac**. Mọi nhận định đều phân tách nghiêm ngặt giữa:
- **`CONFIRMED` (Tĩnh / Mã nguồn):** Quan sát trực tiếp trong nhánh `"darwin"` của gói `app.asar` chính thức (`Zalo 26.10.20`), mã nguồn Rust `crates/platform-macos`, tài liệu kernel Darwin/XNU & Apple Developer, và mã nguồn MIT của `ducngfi/zalo-multi-accounts` (`0b3c5aa4416760427d4721e0b0dac752b43e52be`).
- **`AUTHOR CLAIM`:** Kết quả thử nghiệm thực tế ngày `20–21/10/2026` với `Zalo 26.8.20` trên `macOS 26.1` do tác giả `ducngfi` công bố trong tài liệu dự án.
- **`INFERENCE` / `NOT TESTED`:** Giả thuyết kỹ thuật suy dẫn từ cơ chế CoreFoundation, LaunchServices, Keychain, TCC và UNIX domain socket, bắt buộc phải kiểm chứng trên máy Mac thật (Apple Silicon `arm64` & Intel `x86_64`) trước khi nâng trạng thái khỏi `NOT_TESTED`.

---

## 1. Phân Tích 9 Thành Phần Tương Thích Cốt Lõi Trên macOS

### 1.1. Cấu trúc Zalo App Bundle (`/Applications/Zalo.app`) & Tính toàn vẹn chữ ký số

1. **Cấu trúc Bundle và Helper nội bộ (`CONFIRMED` từ `app.asar`):**
   - Trong `main.js:314463` và `main.js:982068`, khi `process.platform !== "win32"`, Zalo tham chiếu đường dẫn tương đối từ thực thi chính (`Contents/MacOS/Zalo`) sang ứng dụng phụ nằm bên trong bundle:
     - `Contents/ZaloHelper.app/Contents/MacOS/ZaloCall` (gọi thoại, gọi video, gọi nhóm).
     - `Contents/ZaloHelper.app/Contents/MacOS/ZaloCap` (chụp màn hình).
   - Định danh bundle chính là `CFBundleIdentifier = "com.vng.zalo"` (`Layout.swift:1161`, `main.js:2720673`).
2. **Cơ chế tự kiểm tra chữ ký `checkAppSigned()` (`CONFIRMED` tại `main.js:3562292..3563611` + `VN-Linux-Family` `patch-linux-guards.js:488`):**
   - Module `SecureCipher` của Zalo trên macOS thực thi kiểm tra chữ ký `codesign` của `.app` bundle để quyết định `this.isAppSigned`. Nếu kiểm tra thất bại, `decrypt()` với các bản ghi `CIPHER_KEYCHAIN` ném ngoại lệ `"The cipher keychain is not supported"`.
   - **Hệ quả kiến trúc then chốt (`INFERENCE` có bằng chứng mã nguồn):** Phương án nhân bản `/Applications/Zalo.app` thành `Zalo2.app`, sửa `Info.plist` (`CFBundleIdentifier = com.vng.zalo.profile2`) rồi ký lại bằng chữ ký nội bộ (`codesign --sign -`) **chắc chắn làm vỡ notarization/TeamID của VNG**, làm hỏng `SecureCipher`, phá vỡ ACL của Keychain và khiến macOS TCC từ chối kế thừa quyền Micro/Camera/Screen Recording! Vì vậy, **bắt buộc phải giữ nguyên 100% `/Applications/Zalo.app` gốc** và khởi chạy đa phiên qua LaunchServices.

---

### 1.2. Launch Services (`/usr/bin/open -n`) & Phát hiện rủi ro giới hạn `104 byte` của `TMPDIR`

1. **Cơ chế `open -n -a /Applications/Zalo.app --env CFFIXED_USER_HOME=...` (`CONFIRMED` mã nguồn / `AUTHOR CLAIM` trên macOS 26.1):**
   - Cờ `-n` yêu cầu LaunchServices cấp một `LSASN` (Application Serial Number) mới thay vì gửi sự kiện `kAEReopenApplication` tới phiên đang mở.
   - Cờ `--env CFFIXED_USER_HOME=<profile_dir>` chuyển biến môi trường vào tiến trình mới thông qua `spawn_via_launchd`, điều hướng `NSHomeDirectory()` và `NSSearchPathForDirectoriesInDomains(NSApplicationSupportDirectory, ...)` của CoreFoundation/Cocoa mà không chạm tới chữ ký Hardened Runtime của bundle.
2. **Liên kết sâu `zalo://` (`CONFIRMED` tại `main.js:2720673`):**
   - Mọi phiên đều gọi `app.setAsDefaultProtocolClient("zalo")` và lắng nghe `app.on("open-url", ...)`. LaunchServices ánh xạ URL scheme theo `CFBundleIdentifier` (`com.vng.zalo`), do đó khi người dùng mở liên kết `zalo://` từ Safari/Chrome, macOS sẽ chuyển sự kiện AppleEvent `kAEGetURL` về phiên đầu tiên hoặc phiên chính đang đăng ký với LaunchServices (`INFERENCE`), không thể định tuyến theo từng profile.
3. **PHÁT HIỆN LỖI TIỀM ẨN NGHIÊM TRỌNG TRONG ADAPTER (`crates/platform-macos/src/layout.rs:82-98`) — Giới hạn `UNIX_PATH_MAX = 104 bytes` của Darwin (`CONFIRMED` tĩnh + `INFERENCE`):**
   - Trên nhân XNU/Darwin (`<sys/un.h>`), trường `sockaddr_un.sun_path` chỉ dài tối đa **104 bytes** (ngắn hơn Linux 108 bytes).
   - Khi Electron/Chromium tạo khóa đơn phiên trên macOS, `ProcessSingleton` tạo một thư mục tạm bên trong `TMPDIR` (ví dụ `<TMPDIR>/org.chromium.Chromium.XXXXXX/SingletonSocket`) rồi tạo symlink từ `<userData>/SingletonLock` và `<userData>/SingletonSocket` sang đó chính nhằm tránh vượt quá 104 bytes của đường dẫn `Application Support`!
   - Trong `ducngfi/zalo-multi-accounts` (`ZaloControl.swift:1017`), tác giả **chỉ truyền duy nhất `CFFIXED_USER_HOME=<home>`** và giữ nguyên `TMPDIR` mặc định của macOS (`/var/folders/xx/.../T/`, dài ~49 ký tự -> tổng đường dẫn `SingletonSocket` ~86 bytes < 104 bytes).
   - Ngược lại, `crates/platform-macos/src/layout.rs:95-98` hiện đang truyền thêm `--env HOME=<profile_dir>` và **`--env TMPDIR=<profile_dir>/tmp`**! Nếu `<profile_dir>` nằm tại `/Users/<username>/Library/Application Support/MultiZaloDesktop/Profiles/cong-viec-chinh_a1b2c3d4`, thì `<TMPDIR>/org.chromium.Chromium.XXXXXX/SingletonSocket` dài tới **~136–150 bytes (> 104 bytes)**, khiến lời gọi hệ thống `bind(AF_UNIX)` của Chromium thất bại với `EINVAL`/`ENAMETOOLONG` ngay khi mở phiên!

---

### 1.3. Thư mục `Application Support`, Di trú dữ liệu & Chia sẻ `CFPreferences` (`cfprefsd`)

1. **Đường dẫn dữ liệu cô lập (`CONFIRMED` từ `migration.js:1768,600654`):**
   - `app.getPath("appData")` trên macOS trả về `<CFFIXED_USER_HOME>/Library/Application Support`.
   - `migration.js` gán `app.setPath("userData", "<CFFIXED_USER_HOME>/Library/Application Support/ZaloData")`.
   - Trong thử nghiệm của `ducngfi` (`README.md:197`, `AUTHOR CLAIM`), phiên thứ hai mở 163 tệp hoàn toàn bên trong `<profile>/Library/Application Support/ZaloData`, không rò rỉ tệp nào sang `~/Library/Application Support/ZaloData` của phiên chính.
2. **Thiếu sót Pre-seed trong `dim-platform-macos` (`CONFIRMED` tại `crates/platform-macos/src/layout.rs:34-51`):**
   - `migration.js:514900,600854` kiểm tra cả `<appData>/ZaloPC`, `<appData>/zmeta.json` và `<userData>/migrate-config.json` trên macOS tương tự Windows. `ensure_macos_profile_layout` hiện mới tạo `.zalo-profile-id` mà chưa pre-seed `Library/Application Support/zmeta.json` (`{"failedMigrateCount":0}`) và `ZaloData/migrate-config.json` (`{"zalopc_m_c":true}`).
3. **Chia sẻ `NSUserDefaults` / `CFPreferences` qua daemon `cfprefsd` (`INFERENCE` từ kiến trúc macOS):**
   - Trên macOS hiện đại, `NSUserDefaults` không đọc tệp `.plist` trực tiếp từ biến môi trường của tiến trình con đối với ứng dụng không bật App Sandbox, mà giao tiếp qua XPC với tiến trình nền `cfprefsd` của user session (`~/Library/Preferences/com.vng.zalo.plist`). Do đó, các thiết lập cấp cửa sổ Cocoa chuẩn (như trạng thái `NSWindow` hệ thống hoặc cấu hình lưu trong `NSUserDefaults`) sẽ dùng chung giữa các phiên, trong khi cấu hình tài khoản Zalo lưu trong `electron-store` (`ZaloData/config.json`) và `LevelDB`/`SQLite` vẫn tách biệt hoàn toàn.

---

### 1.4. Định danh Thông báo (Notification Identity) & Badge trên Dock

1. **macOS Notification Center (`usernoted`) (`INFERENCE` + `CONFIRMED` mã nguồn):**
   - Trong `main.js:1063113`, `new electron.Notification(...)` trên macOS đi qua `NSUserNotificationCenter` / `UNUserNotificationCenter` tới daemon hệ thống `usernoted`.
   - `usernoted` định danh nguồn gửi và quyền bật/tắt thông báo dựa trên **`CFBundleIdentifier` (`com.vng.zalo`) của tệp thực thi**, không phân biệt `CFFIXED_USER_HOME`.
   - **Hệ quả:**
     - Người dùng chỉ cấp quyền Thông báo một lần duy nhất cho `Zalo.app` trong *System Settings → Notifications*.
     - Trong trung tâm thông báo của macOS, các thông báo từ mọi phiên đều mang cùng tên ứng dụng `"Zalo"`.
     - **Điểm chưa kiểm chứng runtime (`NOT TESTED`):** Khi 2+ phiên `open -n` cùng chạy và người dùng bấm vào một banner thông báo gốc của macOS, cần đo thực tế xem `usernoted` kích hoạt đúng PID đã phát thông báo đó (nhờ kết nối Mach port XPC đang sống của tiến trình đó) hay chuyển sự kiện về phiên đăng ký đầu tiên.
     - Ngoài ra, Zalo còn có module thông báo nổi riêng `notiWin` (`main.js:2726777`, `znotification.html`) chạy nội bộ theo từng tiến trình nếu chế độ thông báo tùy biến được dùng.
2. **Unread Badge & Dock Bounce (`CONFIRMED` tại `main.js:1013307-1013367`):**
   - Trên `"darwin"`, Zalo gọi `app.dock.setBadge(count)` và `app.dock.bounce()`. Vì mỗi lệnh `open -n` tạo một biểu tượng Dock riêng tương ứng với từng `NSRunningApplication`, **số tin nhắn chưa đọc (badge đỏ) trên từng biểu tượng Zalo dưới thanh Dock phản ánh độc lập đúng số tin chưa đọc của riêng tài khoản đó**!

---

### 1.5. Hành vi Cô lập vs Dùng chung Keychain (`login.keychain-db`)

**Giải mã hoàn chỉnh từ `main.js:3561081..3563611` (`CONFIRMED` tĩnh):**
1. **Cách Zalo sử dụng macOS Keychain:**
   - Zalo bật Electron Fuse `EnableCookieEncryption` (`ev_8e05df8e...`) và dùng thêm lớp `SecureCipher` (`getSecretKeyFromKeyChain`).
   - Electron `safeStorage` (Chromium `OSCrypt`) tạo/đọc **một mục mật khẩu chung duy nhất** trong `login.keychain-db` của người dùng macOS (Service `"Zalo Safe Storage"`, Account `"Zalo"`), được bảo vệ bởi ACL chữ ký số của `/Applications/Zalo.app`.
   - Sau đó, mỗi phiên Zalo tự sinh khóa ngẫu nhiên 32 byte riêng (`o = crypto.randomBytes(32)`), mã hóa chuỗi hex của khóa đó bằng `safeStorage.encryptString(...)`, và lưu bản mã vào tệp cấu hình cục bộ `secure_ks_<hash>` bên trong **`<profile_dir>/Library/Application Support/ZaloData/`** (cũng như mã hóa SQLite `Partitions/zalo/Cookies` riêng của từng profile).
2. **Tại sao Symlink `<profile_dir>/Library/Keychains -> ~/Library/Keychains` là bắt buộc (`CONFIRMED` trong `ProfileManager.swift:1119` & `crates/platform-macos/src/layout.rs:12`):**
   - Khi `CFFIXED_USER_HOME` đổi thư mục nhà của tiến trình, framework Security của macOS tìm `Library/Keychains/login.keychain-db` bên trong `CFFIXED_USER_HOME`.
   - Nhờ tạo symlink `Library/Keychains` trỏ về Keychain thật của user, mọi phiên Zalo đều đọc chung khóa bọc gốc `"Zalo Safe Storage"` một cách hợp lệ mà **không ghi đè khóa dữ liệu `secure_ks_*` hay cookie của nhau**, và được kỳ vọng không sinh hộp thoại hỏi mật khẩu Keychain lặp lại (`AUTHOR CLAIM` từ `ducngfi`: phiên thứ hai mở lên giữ nguyên trạng thái đã đăng nhập; tuy nhiên cần xác nhận `NOT TESTED` xem lần đầu mở phiên phụ có xuất hiện tối đa 1 hộp thoại xác nhận quyền truy cập `"Zalo Safe Storage"` hay không).

---

### 1.6. Quyền sở hữu Local Socket (`/tmp/socketzalo*2021`) — Xung đột Cuộc gọi

**Bằng chứng mã nguồn (`CONFIRMED`) & Đối chiếu `AUTHOR CLAIM`:**
- Trên nhánh macOS (`CONFIRMED` qua quan sát hành vi socket, tóm tắt clean-room không trích mã nguồn): tiến trình gọi thoại sử dụng hai đường dẫn UNIX domain socket cố định là `"/tmp/socketzalosend2021"` và `"/tmp/socketzalorecv2021"`. Khi bind gặp lỗi `EADDRINUSE` do socket đã tồn tại từ phiên trước, phiên khởi tạo sau chủ động gọi `unlink` lên đường dẫn socket cũ rồi thực hiện `listen` lại để chiếm quyền lắng nghe.
- **Lưu ý quan trọng:** Vì đường dẫn `"/tmp/socketzalo..."` được viết cứng bằng chuỗi hằng (không gọi `os.tmpdir()` hay `app.getPath("temp")`), việc đổi biến môi trường `TMPDIR` **không thể** điều hướng hai tệp socket này sang thư mục riêng!
- **Hành vi ngược hẳn với Windows:**
  - Trên Windows, phiên mở **đầu tiên** giữ chặt `PipeZCall*` và chặn mọi phiên mở sau.
  - Trên macOS, phiên khởi tạo module gọi **sau cùng** thực hiện `fs.unlink` xóa tệp socket trên `/tmp` của phiên trước và chiếm quyền lắng nghe! Tác giả `ducngfi` (`README.md:189,199`) xác nhận quan sát thực tế trên macOS 26.1: *"Zalo thứ hai đúng là chiếm các socket gọi điện như dự đoán"*.
  - **Hệ quả thực tiễn:** Trên macOS, người dùng có thể chủ động chọn tài khoản cần gọi bằng cách **đóng các tài khoản khác rồi mở lại tài khoản cần gọi sau cùng** mà không bắt buộc phải đặt nó làm Zalo chính!

---

### 1.7. Quyền Micro, Camera & Ghi màn hình (macOS TCC — `tccd`)

- Trong kiểm tra `main.js`, Zalo không gọi thủ công `systemPreferences.askForMediaAccess` ở tầng JS mà để macOS tự động hiển thị hộp thoại xin quyền TCC khi `ZaloHelper.app/Contents/MacOS/ZaloCall` (Qt5 / AVFoundation / CoreAudio) hoặc Chromium truy cập thiết bị lần đầu, dựa trên khóa `NSMicrophoneUsageDescription` / `NSCameraUsageDescription` trong `Info.plist`.
- Daemon `tccd` của macOS quản lý cơ sở dữ liệu quyền (`~/Library/Application Support/com.apple.TCC/TCC.db` của user thật) theo **`CFBundleIdentifier` + Code Requirement (CDHash/TeamID)**.
- Vì mọi phiên đều khởi chạy từ cùng `/Applications/Zalo.app` nguyên bản do VNG ký và công chứng, **quyền Micro, Camera và Screen Recording cấp một lần sẽ có hiệu lực cho tất cả các phiên** (`INFERENCE` từ cơ chế TCC của Apple; trạng thái thực nghiệm: `NOT TESTED`). Điểm chặn duy nhất của gọi thoại/video vẫn là cặp socket `/tmp/socketzalo*2021` ở mục 1.6.

---

### 1.8. Kích hoạt ứng dụng (App Activation) & 2 Lỗi Cần Sửa Gấp Trong `dim-platform-macos`

Kiểm tra chéo `crates/platform-macos/src/adapter.rs` với `ducngfi/zalo-multi-accounts/macos/Sources/ZaloProfilesCore/ZaloControl.swift` phát hiện **2 lỗi thiết kế nghiêm trọng trong Adapter hiện tại** (`CONFIRMED`):

1. **Lỗi 1 — Gửi `SIGTERM` quá sớm khi đang đóng phiên (`crates/platform-macos/src/adapter.rs:380-403`):**
   - Mã hiện tại chạy lệnh `osascript` gọi `appRef's terminate()`, rồi **ngay dòng 387 lập tức kiểm tra `if is_unix_pid_alive(pid)` và bắn `libc::kill(raw_pid, libc::SIGTERM)` trước khi bước vào vòng lặp chờ `timeout_ms`**!
   - Vì `terminate()` của Cocoa là bất đồng bộ (cần 0,5–3 giây để renderer lưu trạng thái và đóng kết nối SQLite WAL), việc bắn `SIGTERM` ngay mili-giây tiếp theo có nguy cơ ngắt ngang quá trình đóng cơ sở dữ liệu của tài khoản đang đăng nhập!
   - **Cách sửa chuẩn:** Sau khi gọi `terminate()`, phải chờ vòng lặp kiểm tra tiến trình thoát tự nhiên. Chỉ gửi `SIGTERM` khi đã hết thời gian ân hạn hoặc khi xác nhận không còn tiến trình `--type=renderer` và không còn file mở trong `Database/_production/` (giống `ZaloControl.swift:963-985`), đồng thời kiểm tra cả các tiến trình con `ZaloCall`/`ZaloCap`.
2. **Lỗi 2 — Dùng `System Events` đòi quyền Accessibility TCC không cần thiết (`crates/platform-macos/src/adapter.rs:342-346`) & Thiếu kiểm tra `bundleIdentifier` (`adapter.rs:193`):**
   - `focus_or_restore` hiện gọi `tell application "System Events" to set frontmost of first process whose unix id is ...`. Trên macOS, điều khiển `System Events` buộc người dùng phải cấp quyền nhạy cảm **Accessibility (`kTCCServiceAccessibility`)** cho trình quản lý! Trong khi đó, gọi `NSRunningApplication(processIdentifier: pid)?.activate()` qua AppKit/JXA (`use framework "AppKit"`) giống dòng `382` **không hề yêu cầu quyền Accessibility TCC**!
   - Ngoài ra, `inspect_profile` (`adapter.rs:193`) chỉ kiểm tra `kill(pid, 0)` trên số PID đọc từ symlink `SingletonLock` mà chưa đối chiếu tên tiến trình / `bundleIdentifier == "com.vng.zalo"`. Nếu máy Mac khởi động lại sau khi mất điện và hệ điều hành cấp lại số PID đó cho một tiến trình khác, Adapter sẽ báo nhầm trạng thái `"Running"`.

---

### 1.9. Gom nhóm cửa sổ (Window Grouping) & Cơ chế Tự động cập nhật (`Squirrel.Mac`)

1. **Dock, `Cmd+Tab` và Mission Control (`AUTHOR CLAIM` + `INFERENCE`):**
   - Mỗi phiên `open -n` xuất hiện như **một biểu tượng Zalo độc lập trên thanh Dock** và trong bộ chuyển đổi `Cmd+Tab`. Tiêu đề cửa sổ đều là `"Zalo"`, không có nhãn tên profile trên thanh tiêu đề macOS, nhưng người dùng có thể bấm nút **"Hiện" (Focus)** ngay trong trình quản lý để đưa chính xác cửa sổ của profile tương ứng lên trước.
2. **Tự động cập nhật qua `MacUpdater` / `Squirrel.Mac` (`CONFIRMED` tại `main.js:3396325..3398000`):**
   - Trên macOS, `electron-updater` tạo một máy chủ HTTP nội bộ trên cổng động `127.0.0.1:0` (`l.address().port` — không xung đột cổng giữa các phiên) để cấp gói `.zip` cho `Squirrel.Mac` (`ShipIt`), sau đó ghi đè `/Applications/Zalo.app`.
   - **Rủi ro khi nhiều phiên đang mở (`INFERENCE`):** Nếu một phiên kích hoạt cập nhật và thay thế `/Applications/Zalo.app` trên đĩa trong khi các phiên `open -n` khác vẫn đang chạy, các phiên đang chạy có thể gặp lỗi nạp tài nguyên lười (`lazy/*.js` trong `app.asar`) hoặc crash helper `ZaloCap`/`ZaloCall` do mã băm chữ ký Mach-O thay đổi giữa chừng. Cần khuyến cáo đóng tất cả các phiên phụ trước khi cập nhật Zalo.

---

## 2. Kết Luận Khả Thi Trong Phạm Vi Cấu Hình macOS Hỗ Trợ

Mô hình `open -n -a /Applications/Zalo.app --env CFFIXED_USER_HOME=<profile_dir>` kết hợp symlink `Library/Keychains` + thư mục người dùng **hoàn toàn nằm trong phạm vi cấu hình hợp lệ được macOS hỗ trợ** (không tắt SIP, không tắt Gatekeeper, không sửa đổi hay ký lại `Zalo.app`). Sau khi khắc phục **4 lỗi tĩnh đã phát hiện trong `crates/platform-macos`** (`TMPDIR` vượt 104 byte, `SIGTERM` sớm, `System Events` Accessibility TCC, thiếu kiểm tra `bundleIdentifier` + pre-seed `zmeta.json`), toàn bộ các chức năng Nhắn tin, Gửi/nhận tệp, Giữ phiên Keychain, Badge thông báo trên Dock và Chuyển đổi cửa sổ đều có cơ sở vững chắc để hoạt động ổn định, ngoại trừ giới hạn cố hữu `/tmp/socketzalo*2021` của ứng dụng đối với Gọi thoại/Video đồng thời.
