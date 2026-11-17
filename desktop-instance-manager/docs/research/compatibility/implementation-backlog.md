# Backlog Triển Khai & Đặc Tả Integration Requests Đa Nền Tảng (`implementation-backlog.md`)

**Nhánh nghiên cứu:** `research/desktop-compatibility`  
**Phạm vi áp dụng:** `desktop-instance-manager` (`crates/core`, `crates/profile-store`, `crates/process-manager`, `crates/platform-windows`, `crates/platform-macos`, `crates/platform-linux`, `scripts/dim-host.ps1`, `fixtures/test-desktop-app/src/bin/dim-cli.rs`, `apps/desktop/src-tauri/src/commands.rs`)  
**Mục tiêu tài liệu:** Cung cấp đặc tả kỹ thuật **tự khép kín 100% (self-contained)** cho từng Integration Request (`IR-WIN-01..04`, `IR-MAC-01..04`, `IR-UBU-01..05`), được xếp hạng nghiêm ngặt theo **Giá trị sản phẩm & An toàn dữ liệu người dùng (Product-Value Rubric)** và đối chiếu truy vết 1-1 với bộ mã định danh chuẩn (`F-*`, `TC-H*`, phân loại bằng chứng 6 tầng). Bất kỳ agent triển khai trên Windows, macOS hoặc Ubuntu nào cũng có thể nhận trực tiếp một `IR-*` đơn lẻ để lập trình và kiểm thử ngay mà không cần đọc lại các báo cáo nghiên cứu dài của Mission D.

---

## 0. Bất Biến Bảo Mật, Toàn Vẹn Nhị Phân & Phân Định Ranh Giới Sở Hữu Mã Nguồn

### 0.1. Bất biến an toàn tuyệt đối (Global Non-Negotiable Guardrails)
1. **Không sửa đổi nhị phân & mã nguồn Zalo:** Giữ nguyên tuyệt đối mã băm SHA-256 và chữ ký số (`Authenticode` VNG trên Windows, `codesign` + Notarization trên macOS) của `Zalo.exe`, `Zalo.app`, `ZaloCall.exe`, `ZaloHelper.app` và `resources/app.asar`. Không giải nén-đóng gói lại `app.asar`, không tiêm DLL (`Detours`/`MinHook`), không hook Named Pipe / Unix Domain Socket.
2. **Không hạ cấp cơ chế bảo mật hệ điều hành:** Tuyệt đối không tắt hoặc hướng dẫn tắt Windows Smart App Control (`SAC`), không tắt macOS SIP/Gatekeeper, không chạy `sysctl kernel.apparmor_restrict_unprivileged_userns=0`, và **không bao giờ tự động gắn cờ `--no-sandbox` hoặc `ELECTRON_DISABLE_SANDBOX=1`** trên Ubuntu 23.10+/24.04+.
3. **Bảo vệ tuyệt đối phiên Zalo chính (`PrimaryInstanceStatus`):** Không thao tác ghi/xóa/di trú lên `%APPDATA%\ZaloData`, `~/Library/Application Support/ZaloData`, `~/.config/ZaloData`, hay `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Zalo.lnk`; không bao giờ gửi tín hiệu đóng/hủy tới bất kỳ PID nào nằm trong `protected_pids`.
4. **Không phân phối tài sản độc quyền:** Trên Linux, bản chính thức của VNG được báo cáo trung thực là `SupportStatus::Unsupported`; chỉ hỗ trợ thực thi cục bộ do người dùng tự cung cấp (`LaunchTarget::CustomExecutable`) hoặc `LaunchTarget::NodeFixture` ở chế độ `SupportStatus::Experimental` (hoặc `SupportStatus::Blocked` khi vướng chính sách AppArmor `userns`).
5. **An toàn kiểm thử & quyền riêng tư:** Mọi kiểm thử tự động chỉ sử dụng `dim-test-fixture` / `fixture-app.mjs` và dữ liệu giả lập; tuyệt đối không tự động hóa đăng nhập tài khoản Zalo thật, không tự động phát cuộc gọi tới người dùng không đồng thuận, và ẩn danh hóa mọi đường dẫn người dùng (`%USERPROFILE%`, `%APPDATA%`, `<ProfileDir>`, `~`).

### 0.2. Ranh giới sở hữu tệp tin giữa các Agent triển khai (File Ownership Boundaries)
Để tránh xung đột ghi đè (`merge conflict`) khi triển khai song song theo hệ điều hành, quyền ghi trên cây mã nguồn được phân tách cứng như sau:

| Agent Owner | Quyền ghi độc quyền (Owned Paths) | Quy tắc phối hợp trên Crates dùng chung |
| :--- | :--- | :--- |
| **`Core/Windows agent`** *(hoặc Pha A: `Core agent` -> Pha B: `Windows agent`)* | `crates/core/src/{error.rs,models.rs,traits.rs,zalo_identity.rs,lib.rs}`<br>`crates/process-manager/src/{guard.rs,coordinator.rs}`<br>`crates/profile-store/src/{store.rs,backup.rs}` | Chốt chữ ký trait/enum/struct dùng chung (`DimError`, `ExternalStoragePaths`, `ExternalStorageGuard`, `stop_all_secondary_profiles`) trong `IR-WIN-04` và `IR-WIN-03` **trước** khi các adapter OS gọi vào; cập nhật đồng bộ `fixtures/test-desktop-app/src/bin/dim-cli.rs` và `apps/desktop/src-tauri/src/commands.rs`. |
| **`Windows agent`** | `crates/platform-windows/src/{win32_ffi.rs,adapter.rs,layout.rs,lib.rs}`<br>`scripts/dim-host.ps1` | Không sửa `crates/platform-macos/**` hoặc `crates/platform-linux/**`. Giữ tương thích hợp đồng JSON 100% giữa `WindowsPlatformAdapter` và `HostEngine` trong `scripts/dim-host.ps1`. |
| **`macOS agent`** | `crates/platform-macos/src/{layout.rs,adapter.rs,lib.rs}` | Chỉ triển khai các trait của `dim-core`; không sửa `crates/core/**`, `crates/platform-windows/**`, hay `crates/platform-linux/**`. |
| **`Ubuntu agent`** | `crates/platform-linux/src/{layout.rs,adapter.rs,lib.rs}` | Chỉ triển khai các trait của `dim-core`; không sửa `crates/core/**`, `crates/platform-windows/**`, hay `crates/platform-macos/**`. |

---

## 1. Khung Xếp Hạng Ưu Tiên Theo Giá Trị Sản Phẩm (Product-Value Prioritization Rubric — Mục 4)

Toàn bộ các Integration Request và kịch bản kiểm thử (`TC-H*`) bắt buộc tuân theo thang ưu tiên 7 bậc dựa trên **mức độ rủi ro đối với dữ liệu tài khoản và luồng sử dụng cốt lõi của người dùng cuối**:

| Thứ bậc ưu tiên | Nhóm tác động sản phẩm (Product-Value Category) | Mức ưu tiên | Lý do cốt lõi & Quy tắc phân bổ nguồn lực | Integration Requests & Tests ánh xạ |
| :---: | :--- | :---: | :--- | :--- |
| **1** | **Nguy cơ mất dữ liệu / hỏng cơ sở dữ liệu (Data loss & DB corruption risk)** | **`P0`** | Xóa nhầm thư mục thật qua NTFS junction/symlink, sao lưu thiếu `db_path.location` ngoài `<ProfileDir>`, ghi chồng SQLite giữa 2 phiên, hoặc bắn `SIGTERM`/`SIGKILL` sớm khi SQLite WAL đang flush gây mất dữ liệu không thể khôi phục. | `IR-WIN-03` (junction delete guard), `IR-WIN-04`, `IR-MAC-02`, `IR-UBU-01` (safe `stop`) — `TC-H4`, `TC-H7`, `TC-H8` |
| **2** | **Khả năng đóng nhầm tiến trình / cửa sổ (Wrong process or window termination)** | **`P0`** | `EnumWindows` bắt nhầm cửa sổ `notiWin`/helper rồi gửi `WM_CLOSE`, hoặc `SingletonLock` tái sử dụng PID cũ sau khi máy khởi động lại dẫn tới đóng nhầm tiến trình không thuộc phiên đích. | `IR-WIN-01` (window filter), `IR-WIN-02` (`notiWin` exclusion), `IR-MAC-03` (`bundleIdentifier` PID guard) — `TC-H2`, `TC-H4` |
| **3** | **Khả năng không đăng nhập / không mở được tài khoản thứ hai (2nd-account L3 login & launch blockers)** | **`P0`** | Nếu phiên thứ 2 bị chặn ngay lúc mở (`TMPDIR` vượt `sun_path[104]`, `bwrap --die-with-parent` tự hủy tiến trình, thiếu socket `gnome-keyring`/Wayland/X11, vướng AppArmor `userns` không báo rõ) hoặc chưa xác minh độc lập đăng nhập `L3`, sản phẩm chưa đạt giá trị cốt lõi. | `IR-MAC-01`, `IR-UBU-01` (`bwrap` lifetime), `IR-UBU-02` (runtime/keyring sockets), `IR-UBU-04` (AppArmor preflight) — `TC-H1B` (Stage 1), `TC-H3`, `TC-H8` |
| **4** | **Khả năng nhắn tin & gửi/nhận tệp (Messaging & file transfer dialogs)** | **`P1`** | Hộp thoại `showOpenDialog`/`showSaveDialog` mở vào thư mục giả lập rỗng thiếu `Downloads`/`Pictures`/`.config/user-dirs.dirs` làm cản trở thao tác gửi tài liệu hằng ngày. | `IR-WIN-03` (layout + opt-in junctions), `IR-UBU-02` (XDG `user-dirs.dirs`) — `TC-H7`, `TC-H8` |
| **5** | **Khả năng đóng/mở lại giữ phiên & chuyển đổi cửa sổ (Close/reopen persistence & focus/restore)** | **`P1`** | Đánh thức cửa sổ ẩn dưới System Tray qua Win32 Foreground Lock (`second-instance`), kích hoạt cửa sổ macOS không đòi quyền Accessibility TCC, và khôi phục cửa sổ Linux từ stub rỗng. | `IR-WIN-01` (handoff focus), `IR-MAC-03` (AppKit activate), `IR-UBU-03` (`second-instance` focus) — `TC-H1B` (Stage 1), `TC-H4`, `TC-H6`, `TC-H8` |
| **6** | **Thông báo (Notifications & Unread Badges)** | **`P2`** | Tối ưu trải nghiệm nhận biết tin nhắn mới (hướng dẫn dùng popup in-app `notiWin`, phân biệt Dock badge trên macOS với xung đột `LauncherEntry` trên Ubuntu). | `IR-WIN-02` (guidance), `IR-MAC-04` (Dock vs `usernoted` docs), `IR-UBU-05` (`LauncherEntry` docs) — `TC-H2`, `TC-H8` |
| **7** | **Gọi điện thoại & video (Audio/video calls)** | **`P2`** | Tính năng phụ trợ bị giới hạn bởi kiến trúc IPC cố định của ứng dụng (`PipeZCall*`, `/tmp/socketzalo*2021`, TCP `29631/29632`). **Hiệu chỉnh bắt buộc từ Mission D:** Hạ từ `P0` cũ xuống **`P2`** — tuyệt đối **không** để công việc chẩn đoán/điều phối cuộc gọi chiếm dụng tài nguyên kỹ thuật trước khi hoàn tất toàn bộ hạng mục `P0`/`P1` và xác minh đăng nhập độc lập `L3`! | `IR-WIN-02` (passive pipe diagnostics), `IR-MAC-04` (socket owner & restart UX), `IR-UBU-05` (port check & optional `pasta` netns) — `TC-H1A`, `TC-H1B` (Stage 2), `TC-H5`, `TC-H8` |

### Thứ tự triển khai theo 3 Đợt (Execution Waves)
- **Đợt 1 (`Wave 1 — P0 Gate`):** Triển khai toàn bộ phần bảo vệ dữ liệu, chống đóng nhầm tiến trình/cửa sổ và gỡ lỗi chặn khởi chạy (`IR-WIN-04`, phần `P0` của `IR-WIN-01`/`IR-WIN-02`/`IR-WIN-03`, `IR-MAC-01`, `IR-MAC-02`, phần `P0` của `IR-MAC-03`, `IR-UBU-01`, phần `P0` của `IR-UBU-02`, `IR-UBU-04`) + chạy kiểm chứng `TC-H2`, `TC-H3`, `TC-H4`, `TC-H7`, `TC-H8` (8A/8B) và `TC-H1B` (Stage 1 — L3 independent login).
- **Đợt 2 (`Wave 2 — P1 Core UX`):** Hoàn thiện trải nghiệm gửi/nhận tệp và chuyển đổi cửa sổ mượt mà (phần `P1` của `IR-WIN-01` & `IR-WIN-03`, phần `P1` của `IR-MAC-03`, phần `P1` của `IR-UBU-02`, `IR-UBU-03`) + chạy `TC-H6`.
- **Đợt 3 (`Wave 3 — P2 Notifications & Calls`):** Chỉ mở khóa sau khi Đợt 1 & Đợt 2 đạt `PASS`; bổ sung chẩn đoán thụ động quyền sở hữu kênh gọi và hướng dẫn thông báo (`IR-WIN-02` phần `P2`, `IR-MAC-04`, `IR-UBU-05`) + chạy `TC-H1A`, `TC-H1B` (Stage 2), `TC-H5`, `TC-H8` (8C).

---

## 2. Đặc Tả Chi Tiết Integration Requests Theo Từng Nền Tảng (Mục 3)

### 2.1. Nhóm Windows & Core (`IR-WIN-01` .. `IR-WIN-04`)

---

#### `IR-WIN-01`: Lọc quyền sở hữu cửa sổ chính (`EnumWindows`) & Kích hoạt vượt Foreground Lock qua `second-instance`

- **Mức ưu tiên & Phân nhóm giá trị:** **`P0`** *(Nhóm 2: Chống đóng nhầm cửa sổ phụ khi `stop()`)* + **`P1`** *(Nhóm 5: Đánh thức cửa sổ ẩn ở System Tray & chuyển cửa sổ qua Foreground Lock)*.
- **Agent chịu trách nhiệm:** `Windows agent` *(phạm vi tệp: `crates/platform-windows/src/{win32_ffi.rs,adapter.rs}` và `scripts/dim-host.ps1`)*.
- **Liên kết Finding & Trạng thái bằng chứng (6 tầng):**
  - `F-WIN-02`: `Confirmed by source code` (bộ lọc `enum_windows_cb` thiếu kiểm tra class/owner/toolwindow/kích thước trong `win32_ffi.rs:884-908` và `dim-host.ps1:1315-1347`; cấu trúc `notiWin` `352x82` `alwaysOnTop` `skipTaskbar` xác nhận tại `app.asar/main-dist/main.js:2726777`).
  - `F-WIN-03`: `Confirmed by source code` (`focus_or_restore` tại `adapter.rs:651-654` và `FocusProfileInternal` tại `dim-host.ps1:2100-2112` trả về sớm sau `SetForegroundWindow` mà không kiểm tra `GetForegroundWindow() == hwnd`; cơ chế `second-instance` gọi `setAlwaysOnTop(true)` -> `focus()` xác nhận tại `main.js:3565794`).
- **Liên kết Test Case:** `TC-H2`, `TC-H6`.
- **Vị trí mã nguồn & Chữ ký hàm cần sửa:**
  1. `crates/platform-windows/src/win32_ffi.rs:399-413` (`#[link(name = "user32")] extern "system"`): Khai báo bổ sung `GetClassNameW`, `GetWindow`, `GetWindowLongW` (hoặc `GetWindowLongPtrW`), `GetWindowRect`, `GetForegroundWindow`, struct `RECT { left: i32, top: i32, right: i32, bottom: i32 }`, cùng các hằng số `GW_OWNER: u32 = 4`, `GWL_EXSTYLE: i32 = -20`, `WS_EX_TOOLWINDOW: isize = 0x0000_0080`.
  2. `crates/platform-windows/src/win32_ffi.rs:878-945`:
     - Cập nhật `WindowSearchCtx`, `enum_windows_cb(hwnd: *mut c_void, lparam: isize) -> i32` (dòng `884-908`) và đổi/bổ sung hàm `pub fn find_main_window_for_profile(main_pid: u32, tree_pids: &[u32]) -> Option<(u64, String)>` (giữ wrapper hoặc cập nhật `find_main_window_for_pids` tại dòng `911-927`).
     - Đổi chữ ký `pub fn focus_window(hwnd_val: u64) -> bool` (dòng `930-944`) trả về `true` chỉ khi `unsafe { GetForegroundWindow() == hwnd }` sau khi gọi `ShowWindow` + `SetForegroundWindow`.
  3. `crates/platform-windows/src/adapter.rs:524-540` (`WindowsPlatformAdapter::inspect_profile`), `adapter.rs:642-673` (`WindowsPlatformAdapter::focus_or_restore`), và `adapter.rs:675-790` (`WindowsPlatformAdapter::stop`, đặc biệt `post_wm_close` tại dòng `743-745`).
  4. `scripts/dim-host.ps1`:
     - `NativeWin32` (`dòng 192-228`): Bổ sung P/Invoke `GetClassName`, `GetWindow`, `GetWindowLongPtr`/`GetWindowLong`, `GetForegroundWindow`.
     - `HostEngine.FindMainWindowForPids` (`dòng 1315-1347`), `InspectProfileRuntime` (`dòng 1385-1396`), `FocusProfileInternal` (`dòng 2097-2141`), và `StopProfile` (`dòng 2248-2252`).
- **Phát biểu vấn đề (Self-Contained):**
  - Hiện tại `enum_windows_cb` trong `win32_ffi.rs:884-908` khớp ngay cửa sổ `IsWindowVisible != 0` đầu tiên của bất kỳ PID nào trong cây tiến trình mà không kiểm tra lớp cửa sổ, cửa sổ cha, kiểu `WS_EX_TOOLWINDOW`, hay kích thước tối thiểu (`dim-host.ps1:1329` chỉ lọc `< 120x120`, vẫn lọt các cửa sổ phụ/call helper hoặc splash). Cửa sổ thông báo nổi `notiWin` (`352x82`, `WS_EX_TOPMOST`) nằm trên đỉnh Z-order sẽ bị nhận nhầm làm `window_handle` chính, khiến `stop()` gửi nhầm `WM_CLOSE` vào popup thay vì cửa sổ chính và `focus_or_restore()` focus nhầm popup.
  - Khi cửa sổ chính ẩn xuống System Tray (`window_handle.is_none()`) hoặc khi tiến trình gọi nền (`ui-server.mjs` / `powershell.exe` / `dim-cli.exe`) bị chặn bởi Win32 Foreground Lock, `SetForegroundWindow(hwnd)` chỉ nhấp nháy biểu tượng dưới Taskbar nhưng `focus_or_restore` (`adapter.rs:653`) và `FocusProfileInternal` (`dim-host.ps1:2111`) đã thoát sớm báo thành công thay vì chuyển tiếp sang cơ chế handoff `second-instance`.
- **Đặc tả thay đổi kỹ thuật:**
  1. **Bộ lọc cửa sổ chính chuẩn xác (`P0`):** Một `HWND` chỉ được chọn làm cửa sổ chính khi thỏa mãn đồng thời **6 điều kiện**:
     - `IsWindowVisible(hwnd) != 0`;
     - PID sở hữu `GetWindowThreadProcessId(hwnd)` bằng đúng `main_pid` của Electron (chỉ fallback sang `tree_pids` nếu chạy custom wrapper nhưng vẫn phải khớp các điều kiện dưới);
     - `GetClassNameW(hwnd)` khớp chính xác `"Chrome_WidgetWin_1"` đối với Zalo/Electron (hoặc lớp cửa sổ test khai báo tường minh của `dim-test-fixture` trong kiểm thử);
     - `GetWindow(hwnd, GW_OWNER).is_null()` (không phải dialog/popup con);
     - `(GetWindowLongPtrW(hwnd, GWL_EXSTYLE) & WS_EX_TOOLWINDOW) == 0`;
     - Tiêu đề cửa sổ khác rỗng (`GetWindowTextLengthW(hwnd) > 0`) **và** kích thước khung `GetWindowRect` đạt `width >= 400 && height >= 400` (loại trừ triệt để `notiWin` `352x82` và các mini-overlay).
  2. **Chuỗi kích hoạt cửa sổ 2 bước (`P1`):** Trong `focus_or_restore` (`adapter.rs:642-673`) và `FocusProfileInternal` (`dim-host.ps1:2097-2141`):
     - Nếu `state.window_handle` tồn tại (`Some(hwnd)`): gọi `focus_window(hwnd)` (`ShowWindow(SW_RESTORE/SW_SHOW)` + `SetForegroundWindow(hwnd)`), chờ ngắn `30–50ms` rồi kiểm tra `GetForegroundWindow() == hwnd`. Nếu đã lên tiền cảnh, trả `Ok(())` (`method = "SetForegroundWindow"`).
     - Nếu `state.window_handle.is_none()` (đang ẩn dưới System Tray) **HOẶC** `GetForegroundWindow() != hwnd` (bị chặn bởi Win32 Foreground Lock): đối với `NodeFixture` thì ghi `.focus-signal`; đối với `OfficialZalo` / `CustomExecutable` thì spawn tiến trình `Zalo-<ver>\Zalo.exe` ngắn hạn với đúng khối biến môi trường cô lập của profile (`build_isolated_env_map`) để kích hoạt sự kiện `second-instance` nội bộ của Zalo (`setAlwaysOnTop(true)` -> `show()` -> `focus()`), ghi nhận `method = "second-instance-handoff"`.
- **Tiêu chí nghiệm thu (Acceptance Criteria):**
  - [ ] Khi một tiến trình test tạo đồng thời cửa sổ chính `900x650` (`Chrome_WidgetWin_1`, `GW_OWNER == 0`, không `WS_EX_TOOLWINDOW`) và cửa sổ popup `352x82` (`WS_EX_TOPMOST | WS_EX_TOOLWINDOW`, tiêu đề `"Zalo Notification"`), cả `win32_ffi::find_main_window_for_pids` và `HostEngine.FindMainWindowForPids` đều trả về đúng `HWND` của cửa sổ `900x650` (`100%` qua `TC-H2`).
  - [ ] Khi chỉ có popup `352x82` hiển thị còn cửa sổ chính ẩn xuống khay hệ thống (`IsWindowVisible == 0`), `inspect_profile` trả `window_handle: None` (`0` trong JSON) thay vì trả `HWND` của popup; `stop()` không bao giờ gửi `WM_CLOSE` vào popup.
  - [ ] Khi cửa sổ chính đang ẩn ở System Tray hoặc nằm dưới một cửa sổ tiền cảnh giữ Foreground Lock, lệnh `focus` đánh thức và đưa đúng cửa sổ profile lên tiền cảnh trong `< 2000ms` qua `second-instance` handoff (`TC-H6`).
- **Phương án bị cấm tuyệt đối (Forbidden Approaches):**
  - **CẤM** dùng `AttachThreadInput` xuyên tiến trình để cưỡng bức `SetForegroundWindow` (gây nguy cơ treo cứng hàng đợi thông điệp `USER32` nếu luồng giao diện đích bị nghẽn).
  - **CẤM** giả lập phím bấm toàn cục qua `SendInput` / `keybd_event` (ví dụ gửi phím `VK_MENU` / `Alt` ảo vào cửa sổ đang làm việc của người dùng).

---

#### `IR-WIN-02`: Phân loại cửa sổ thông báo `notiWin`, hướng dẫn UX thông báo & Chẩn đoán thụ động `PipeZCall*`

- **Mức ưu tiên & Phân nhóm giá trị:** **`P0`** *(Nhóm 2: Loại trừ popup `notiWin` khỏi thao tác đóng/focus)* + **`P2`** *(Nhóm 6 & Nhóm 7: Khuyến nghị thông báo in-app & chẩn đoán thụ động Named Pipe cuộc gọi — **chỉ thực hiện phần `P2` sau khi xong `P0`/`P1` và `L3` login**)*.
- **Agent chịu trách nhiệm:** `Windows agent` *(phạm vi tệp: `crates/platform-windows/src/{win32_ffi.rs,adapter.rs}` và `scripts/dim-host.ps1`)*.
- **Liên kết Finding & Trạng thái bằng chứng (6 tầng):**
  - `F-WIN-02`: `Confirmed by source code` (`win32_ffi.rs:884-908` vs `main.js:2726777`).
  - `F-WIN-05`: `Confirmed by source code` (`app.setAppUserModelId("com.vng.zalo")` tại `main.js:2720673` & `notiWin` `znotification.html` tại `main.js:2726777`) + `Confirmed by static binary inspection` (REA `ev_69ddf74b...` trên `Zalo.lnk`).
  - `F-WIN-06`: **Đã hiệu chỉnh 6 tầng (Recalibrated / Downgraded một phần từ Mission D):** Chuỗi `\\.\pipe\PipeZCallRecv` / `PipeZCallSend` và phép gán `O = t => V(e, t)` là `Confirmed by source code` (`main.js:311828..317500`); import WASAPI/DirectShow của `ZaloCall.exe` là `Confirmed by static binary inspection` (`ev_9145f8bd...`); việc 1 phiên Zalo chính đang rảnh giữ mở cả 2 pipe qua `ZaloCall.exe` (`PID 15624`) là `Observed on running system`. **Tuy nhiên**, suy luận rằng Tài khoản #2 thất bại ngay lúc đăng nhập hay chỉ khi bấm gọi, và việc ghi đè `O` có làm hỏng vĩnh viễn cuộc gọi đi của Tài khoản #1 hay không được hạ xuống `Inference` / `Not tested` chờ kết quả `TC-H1A` & `TC-H1B`!
- **Liên kết Test Case:** `TC-H2`, `TC-H1A`, `TC-H1B`.
- **Vị trí mã nguồn & Chữ ký hàm cần sửa:**
  1. `crates/platform-windows/src/win32_ffi.rs:878-927`: Bổ sung bộ phân loại `pub enum ZaloWindowRole { MainChatWindow, NotificationPopup, AuxiliaryOrSplash }` và hàm `pub fn classify_window_for_pid(...) -> ZaloWindowRole` để nhận diện tường minh popup `notiWin` (~`352x82`, frameless/toolwindow) tách khỏi `MainChatWindow`.
  2. `crates/platform-windows/src/win32_ffi.rs:271-315` *(Tùy chọn Đợt 3 — `P2`)*: Bổ sung truy vấn thụ động chỉ đọc `pub fn query_named_pipe_owner_pid(pipe_name: &str) -> Option<u32>` mở handle chỉ đọc thuộc tính (`dw_desired_access = FILE_READ_ATTRIBUTES` `0x0080`, bọc trong `HandleGuard`) gọi `GetNamedPipeServerProcessId` trên `\\.\pipe\PipeZCallRecv` (hoặc liệt kê `[System.IO.Directory]::GetFiles('\\\\.\\pipe\\')` + đối chiếu PID tiến trình con `ZaloCall.exe` từ `ProcessSnapshot` tại `win32_ffi.rs:765-835`), **tuyệt đối không mở luồng đọc/ghi dữ liệu vào `PipeZCallSend`**! *(Lưu ý kỹ thuật quan trọng: Trong `main.js:315200`, mở kết nối luồng client vào `\\.\pipe\PipeZCallSend` sẽ kích hoạt callback `createServer(e => { O = t => V(e, t) })` của Zalo; do đó `query_named_pipe_owner_pid` bắt buộc được chứng minh qua `TC-H1A` Kịch bản C trên `\\.\pipe\DimTestCallRecv_<run_id>` là không sinh sự kiện `connection` mới và không chạm tới callback `O` trước khi bật cho chẩn đoán thực tế).*
  3. `crates/platform-windows/src/adapter.rs:335-373` (`WindowsPlatformAdapter::capabilities`) & `scripts/dim-host.ps1:2714-2786` (`HostEngine.Diagnostics`).
- **Phát biểu vấn đề (Self-Contained):**
  - Hiện tại `WindowsPlatformAdapter::capabilities()` (`adapter.rs:361-367`) và `Diagnostics()` (`dim-host.ps1:2768`) mới ghi chung chung rằng Windows Toast dùng chung `AppUserModelId("com.vng.zalo")` và `PipeZCall*` chặn gọi đồng thời, chưa phân loại riêng `notiWin` (`352x82`), chưa hướng dẫn người dùng bật thông báo nổi nội bộ của Zalo (`notiWin`) để bấm thông báo mở đúng tài khoản, và chưa phân biệt trạng thái bằng chứng đã hiệu chỉnh của `PipeZCall*`.
- **Đặc tả thay đổi kỹ thuật:**
  1. **Loại trừ tuyệt đối `notiWin` (`P0`):** Tích hợp `ZaloWindowRole::NotificationPopup` với `IR-WIN-01` để đảm bảo cả `focus_or_restore()` lẫn `stop()` không bao giờ chọn `notiWin` làm cửa sổ đích.
  2. **Cập nhật thông điệp hướng dẫn Thông báo & Cuộc gọi trung thực (`P2`):** Trong `capabilities().limitations` và `capabilities().diagnostics` (`adapter.rs:361-371`) cùng `Diagnostics()` (`dim-host.ps1:2768-2785`):
     - Ghi rõ khuyến nghị cấu hình: *"Khuyến nghị bật Thông báo nổi trong ứng dụng Zalo (`notiWin` / `znotification.html`) để sự kiện click định tuyến nội bộ tới đúng cửa sổ tài khoản; Windows Action Center Toast dùng chung `AppUserModelId(\"com.vng.zalo\")` và `Zalo.lnk`."*
     - Cập nhật mô tả `PipeZCallRecv` / `PipeZCallSend` phản ánh đúng phân tầng bằng chứng (cố định đường ống toàn cục từ mã nguồn; chi tiết ảnh hưởng chéo giữa phiên 1 và phiên 2 được kiểm chứng qua `TC-H1A`/`TC-H1B`).
  3. **Chẩn đoán thụ động tiến trình giữ `ZaloCall` (`P2` — chỉ triển khai sau `Wave 1` & `Wave 2`):** Xuất trường chẩn đoán chỉ đọc (ví dụ `call_helper_pids` / `call_pipe_present: bool` từ `FindFirstFileW("\\\\.\\pipe\\PipeZCall*")` + đối chiếu child `ZaloCall.exe` trong `ProcessSnapshot`) trong `diagnostics` mà không mở luồng ghi/kết nối làm kích hoạt callback `serverSend` của Zalo.
- **Tiêu chí nghiệm thu (Acceptance Criteria):**
  - [ ] `stop()` và `focus_or_restore()` bỏ qua `100%` cửa sổ phân loại là `NotificationPopup` (`352x82`) trong `TC-H2`.
  - [ ] Kết quả JSON của `diagnostics` trên cả `dim-cli` và `scripts/dim-host.ps1` hiển thị rõ hướng dẫn ưu tiên popup in-app `notiWin` so với Windows Action Center và phản ánh trung thực giới hạn `PipeZCall*`.
  - [ ] Mọi phép đo chẩn đoán `PipeZCall*` (nếu bật ở Đợt 3) được chứng minh bằng `TC-H1A` trên `\\.\pipe\DimTestCallSend` là **không** kích hoạt sự kiện `server.on('connection')` của Node/Electron `libuv`.
- **Phương án bị cấm tuyệt đối (Forbidden Approaches):**
  - **CẤM** sửa đổi hoặc ghi đè tệp `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Zalo.lnk` của người dùng.
  - **CẤM** vá chuỗi `"com.vng.zalo"` hoặc `"\\\\.\\pipe\\PipeZCall*"` trong `resources\app.asar` hay `ZaloCall.exe`.
  - **CẤM** mở kết nối client `CreateFileW(GENERIC_READ | GENERIC_WRITE)` trực tiếp vào `\\.\pipe\PipeZCallSend` của phiên Zalo thật đang chạy vì sẽ kích hoạt callback `O = t => V(e, t)` làm đứt kênh gửi lệnh gọi của người dùng!

---

#### `IR-WIN-03`: Hoàn thiện thư mục người dùng cho Hộp thoại Tệp & Chốt an toàn tuyệt đối khi xóa NTFS Junction

- **Mức ưu tiên & Phân nhóm giá trị:** **`P0`** *(Nhóm 1: Chống mất dữ liệu thư mục thật của người dùng khi xóa profile có NTFS junction)* + **`P1`** *(Nhóm 4: Trải nghiệm hộp thoại gửi/lưu tệp `Downloads`/`Pictures`/`Desktop`)*.
- **Agent chịu trách nhiệm:** `Windows agent` *(phối hợp tối thiểu trên `crates/profile-store/src/store.rs` theo bảng Mục 0.2; phạm vi chính: `crates/platform-windows/src/layout.rs`, `crates/profile-store/src/store.rs`, `scripts/dim-host.ps1`)*.
- **Liên kết Finding & Trạng thái bằng chứng (6 tầng):**
  - `F-WIN-04`: `Confirmed by source code` (`ensure_windows_profile_layout` tại `crates/platform-windows/src/layout.rs:14-46` và `EnsureProfileDirectoriesAndSeeds` tại `scripts/dim-host.ps1:1483-1511` chỉ tạo `Desktop` và `Documents\Zalo Received Files`, thiếu `Downloads`, `Pictures`, `Videos`, `Music`; logic gỡ reparse point hiện có tại `crates/profile-store/src/store.rs:336-427` và `scripts/dim-host.ps1:1513-1584`).
- **Liên kết Test Case:** `TC-H7`.
- **Vị trí mã nguồn & Chữ ký hàm cần sửa:**
  1. `crates/platform-windows/src/layout.rs:14-46` (`ensure_windows_profile_layout`): Bổ sung tạo mặc định các thư mục cô lập `Downloads`, `Pictures`, `Videos`, `Music` bên cạnh `Desktop` và `Documents\Zalo Received Files`; bổ sung hàm opt-in `pub fn configure_windows_host_folder_links(profile_dir: &Path, real_user_profile: &Path, link_downloads: bool, link_desktop: bool) -> Result<(), DimError>`.
  2. `crates/profile-store/src/store.rs:110-144` (`delete_profile_with_guard`) & `store.rs:377-427` (`unlink_reparse_points_safe`, `remove_dir_without_following_links`): Bổ sung bước hậu kiểm (`verify_zero_reparse_points_remaining(&profile_dir)`) **trước** khi xóa đệ quy; nếu bất kỳ reparse point/junction nào gỡ thất bại (`Err`), dừng ngay lập tức và trả lỗi `DimError::Io` thay vì chạy tiếp xóa file bên trong!
  3. `scripts/dim-host.ps1:277-280` (`JunctionFolderNames`), `1483-1511` (`EnsureProfileDirectoriesAndSeeds`), `1513-1584` (`UnlinkJunctionsRecursively`), và `1837-1885` (`DeleteProfile` — hiện dòng `1871-1872` gọi `UnlinkJunctionsRecursively(profilePath); Directory.Delete(profilePath, true);` cần chặn cứng nếu còn bất kỳ thư mục con nào mang cờ `FileAttributes.ReparsePoint` trước khi gọi `Directory.Delete(..., true)`!).
- **Phát biểu vấn đề (Self-Contained):**
  - Khi Zalo mở hộp thoại chọn/lưu tệp (`electron.dialog.showOpenDialog`/`showSaveDialog` tại `main.js:997824`), biến môi trường `USERPROFILE=<ProfileDir>` hướng Common Item Dialog của Windows vào thư mục profile. Vì `ensure_windows_profile_layout` (`layout.rs:18-28`) và `EnsureProfileDirectoriesAndSeeds` (`dim-host.ps1:1485-1495`) chưa tạo `Downloads`, `Pictures`, `Videos`, `Music`, hộp thoại mở vào thư mục rỗng gây bất tiện lớn.
  - Khi bổ sung tính năng opt-in tạo NTFS directory junction cho `Downloads` và `Desktop` trỏ tới thư mục thật của host, bất kỳ lỗi nhỏ nào trong quá trình gỡ junction trước `Directory.Delete(profilePath, true)` (trong `dim-host.ps1:1872` hoặc `SweepDirectoryPattern` dòng `363-364`) đều có thể xóa sạch dữ liệu `Downloads`/`Desktop` thật của người dùng!
- **Đặc tả thay đổi kỹ thuật:**
  1. **Cô lập đầy đủ mặc định (`Isolated by Default — P1`):** Trong cả `ensure_windows_profile_layout` (`layout.rs:14-46`) và `EnsureProfileDirectoriesAndSeeds` (`dim-host.ps1:1483-1511`), luôn tạo đủ 6 thư mục chuẩn bên dưới `<ProfileDir>`: `Desktop`, `Downloads`, `Pictures`, `Videos`, `Music`, và `Documents\Zalo Received Files` (khi đường dẫn chưa tồn tại và không phải reparse point).
  2. **Liên kết NTFS Junction tùy chọn theo từng profile (`Opt-in Host Links — P1`):** Cho phép liên kết tùy chọn riêng cho `Downloads` và `Desktop` sang `%USERPROFILE%\Downloads` và `%USERPROFILE%\Desktop` thật của người dùng (không cần quyền Admin vì NTFS directory junction `FSCTL_SET_REPARSE_POINT` / `cmd /c mklink /J` hoạt động ở quyền user thường). **Bắt buộc giữ `Documents\Zalo Received Files` là thư mục vật lý riêng biệt của từng profile** (tuyệt đối không cho phép tạo junction trên `Documents` hay `Zalo Received Files` để tránh trộn lẫn tệp tự động tải về giữa các tài khoản).
  3. **Chốt chặn hậu kiểm xóa Junction 2 lớp (`Two-Phase Unlink Verification — P0`):**
     - Trong `store.rs:137-140`, `backup.rs:391-394` (`sweep_abandoned_staging_dirs`), và `dim-host.ps1:363-364, 1871-1873, 2638-2639`: Sau khi gọi `unlink_reparse_points_safe` / `UnlinkJunctionsRecursively`, bắt buộc quét xác minh toàn bộ cây `<ProfileDir>` có **0** mục nào mang `FILE_ATTRIBUTE_REPARSE_POINT` (`0x0400`). Nếu còn bất kỳ reparse point nào chưa gỡ được (ví dụ đang bị khóa handle), **hủy bỏ ngay thao tác xóa** và trả lỗi an toàn, tuyệt đối không gọi `Directory.Delete(..., true)` hay xóa đệ quy vào bên trong đích liên kết.
     - Trong `backup.rs:526-528` và `dim-host.ps1:2439, 2476`, giữ nguyên và kiểm thử chặt hành vi bỏ qua mọi reparse point khi tạo gói backup.
- **Tiêu chí nghiệm thu (Acceptance Criteria):**
  - [ ] Tạo mới profile trên Windows tự động có đủ `Desktop`, `Downloads`, `Pictures`, `Videos`, `Music`, và `Documents\Zalo Received Files`.
  - [ ] Khi bật opt-in NTFS junction từ `<ProfileDir>\Downloads` sang thư mục `MockHostDownloads` chứa 5 tệp canary có mã băm SHA-256 cố định, thao tác `backup` không sao chép tệp trong junction, và thao tác `delete --confirm` gỡ sạch junction trong khi giữ nguyên `100%` (5/5) tệp canary cùng SHA-256 trong `MockHostDownloads` (`TC-H7`).
  - [ ] Nếu giả lập một junction bị từ chối quyền gỡ (`RemoveDirectoryW` thất bại), `delete_profile` trả lỗi ngay và **không** xóa bất kỳ tệp nào bên trong thư mục đích của junction (`TC-H7`).
- **Phương án bị cấm tuyệt đối (Forbidden Approaches):**
  - **CẤM** giữ nguyên `USERPROFILE` trỏ vào thư mục thật của người dùng khi khởi chạy Zalo (sẽ làm hỏng cô lập `%USERPROFILE%\AppData\Roaming\ZaloData`).
  - **CẤM** gọi `std::fs::remove_dir_all` thô của thư viện chuẩn hoặc `Directory.Delete(path, true)` mà chưa chạy và xác minh `0` reparse point còn sót lại.
  - **CẤM** liên kết junction chung cho thư mục `Documents\Zalo Received Files` giữa nhiều profile.

---

#### `IR-WIN-04`: Chốt chặn Cơ sở dữ liệu ngoài (`db_path.location` / `resource.location.path`) & Quy trình Dừng an toàn để Cập nhật Zalo

- **Mức ưu tiên & Phân nhóm giá trị:** **`P0`** *(Nhóm 1: Ngăn chặn hỏng SQLite do 2 phiên ghi chung DB ngoài, ngăn chặn sao lưu rỗng gây mất lịch sử tin nhắn, và ngăn lỗi khóa file khi cập nhật Zalo)*.
- **Agent chịu trách nhiệm:** `Core/Windows agent` *(ranh giới tệp rõ ràng: Pha A cập nhật `crates/core/src/{zalo_identity.rs,error.rs,models.rs}`, `crates/process-manager/src/{guard.rs,coordinator.rs}`, `crates/profile-store/src/backup.rs`; Pha B đồng bộ `scripts/dim-host.ps1`, `fixtures/test-desktop-app/src/bin/dim-cli.rs`, `apps/desktop/src-tauri/src/commands.rs`)*.
- **Liên kết Finding & Trạng thái bằng chứng (6 tầng):**
  - `F-CORE-01`: `Confirmed by source code` (`read_configured_db_location` tại `crates/core/src/zalo_identity.rs:35-49` và `DetectZaloUserId` tại `scripts/dim-host.ps1:1200-1234` đã đọc `config.json -> db_path.location`, nhưng `InstanceCoordinator::launch_profile` tại `coordinator.rs:93` chưa chặn 2 profile dùng chung đường dẫn `db_path.location` / `resource.location.path`, và `FileBackupManager::backup_profile` tại `backup.rs:71-165` chỉ quét bên trong `profile_dir`, bỏ sót hoàn toàn DB đặt ở ổ đĩa ngoài!).
  - `F-CORE-02`: `Confirmed by source code` & `Confirmed by static binary inspection` (`main.js:2441895` `quitAndInstall()` chỉ đóng phiên gọi cập nhật; REA `ev_9f322e5a...` trên `Update.exe` và `main.js:3396325` `Squirrel.Mac` xác nhận các phiên phụ đang mở vẫn giữ khóa file trên `Zalo-<oldVer>\Zalo.exe` / `app.asar`).
- **Liên kết Test Case:** `TC-H7`.
- **Vị trí mã nguồn & Chữ ký hàm cần sửa:**
  1. `crates/core/src/error.rs:8-144` (`DimError` & `DimError::code`): Bổ sung 2 biến thể mã lỗi chuẩn:
     - `DimError::SharedExternalDbPath { path: String, conflicting_owner: String }` -> `"SHARED_EXTERNAL_DB_PATH"`
     - `DimError::ExternalDbOutsideProfile { profile_name: String, external_paths: Vec<String> }` -> `"EXTERNAL_DB_OUTSIDE_PROFILE"`
  2. `crates/core/src/zalo_identity.rs:35-49`: Mở rộng `read_configured_db_location` thành struct công khai:
     ```rust
     #[derive(Debug, Clone, Default, PartialEq, Eq)]
     pub struct ConfiguredExternalPaths {
         pub db_location: Option<PathBuf>,
         pub resource_location: Option<PathBuf>,
     }
     pub fn read_configured_external_paths(zalo_data_dir: &Path) -> ConfiguredExternalPaths;
     pub fn external_paths_outside_profile(profile_dir: &Path, zalo_data_dir: &Path) -> Vec<PathBuf>;
     ```
     Đọc cả `db_path.location` và `resource.location.path` (hoặc `resource.location` nếu là chuỗi) từ `<zalo_data_dir>/config.json`.
  3. `crates/process-manager/src/guard.rs:34-92` & `crates/process-manager/src/coordinator.rs:49-128, 472-520`:
     - Bổ sung `ExternalStorageGuard::ensure_no_shared_external_paths(...)` trong `guard.rs` và gọi ngay trong `InstanceCoordinator::launch_profile` (trước dòng `95` `self.adapter.launch(...)`): so sánh đường dẫn chuẩn hóa (`canonicalize_best_effort` / không phân biệt hoa thường trên Windows) của `db_location` và `resource_location` thuộc profile sắp mở với **(a) mọi profile đang chạy khác** và **(b) `%APPDATA%\ZaloData\config.json` của Zalo chính (kể cả khi Zalo chính đang tắt, không cho phép profile phụ trỏ chung vào thư mục DB ngoài của Zalo chính!)**. Nếu trùng hoặc lồng nhau, trả `Err(DimError::SharedExternalDbPath { .. })`.
     - Bổ sung phương thức `pub fn stop_all_secondary_profiles(&self) -> Result<Vec<(String, u32)>, DimError>` trên `InstanceCoordinator` (`coordinator.rs:128`) để đóng êm tuần tự toàn bộ các phiên phụ đang chạy mà **tuyệt đối không chạm vào Zalo chính**, phục vụ quy trình nâng cấp Zalo an toàn (`F-CORE-02`).
  4. `crates/profile-store/src/backup.rs:71-165` (`FileBackupManager::backup_profile`) & `crates/core/src/models.rs:299-321` (`BackupManifest`):
     - Trước khi sao chép tệp tại `backup.rs:106`, gọi `external_paths_outside_profile(profile_dir, &zalo_data_dir)`. Nếu danh sách khác rỗng và người gọi không truyền cờ xác nhận tường minh `allow_incomplete_external_db = true` (qua phương thức mở rộng `backup_profile_with_options` trên `FileBackupManager`), từ chối sao lưu ngay với `Err(DimError::ExternalDbOutsideProfile { .. })` để người dùng không bao giờ lầm tưởng bản backup đã chứa tin nhắn nằm ở ổ `D:\`! Khi có cờ xác nhận tường minh, ghi nhận danh sách đường dẫn bị lược bỏ vào trường `#[serde(default)] pub omitted_external_paths: Vec<String>` trong `BackupManifest`.
  5. `scripts/dim-host.ps1` & CLI/UI surfaces:
     - Cập nhật `LaunchProfile` (`dòng 1947-1976`), `BackupProfile` (`dòng 2343-2435`, thêm cờ `--allow-incomplete-external-db`), và bổ sung lệnh CLI `stop-all` (`DispatchCli` `dòng 2998-3048` + `fixtures/test-desktop-app/src/bin/dim-cli.rs:167-215` + Tauri command `stop_all_profiles` trong `apps/desktop/src-tauri/src/commands.rs`).
- **Tiêu chí nghiệm thu (Acceptance Criteria):**
  - [ ] Khi Profile A và Profile B (hoặc Zalo chính) có `config.json` chứa cùng `db_path.location` hoặc `resource.location.path` ngoài `<ProfileDir>`, lệnh `launch` phiên thứ hai bị chặn cứng với `error_code == "SHARED_EXTERNAL_DB_PATH"` trên cả `dim-cli` và `dim-host.ps1` (`TC-H7`).
  - [ ] Khi một profile đã dừng có `config.json -> db_path.location` trỏ ra thư mục ngoài `<ProfileDir>`, lệnh `backup <id>` mặc định trả về `ok: false`, `error_code == "EXTERNAL_DB_OUTSIDE_PROFILE"`, không tạo thư mục backup giả tạo cảm giác an toàn sai (`TC-H7`).
  - [ ] Lệnh `stop-all` dừng êm toàn bộ các profile phụ đang chạy và giữ nguyên `100%` cây tiến trình của Zalo chính (`protected_pids`).
- **Phương án bị cấm tuyệt đối (Forbidden Approaches):**
  - **CẤM** tự động chỉnh sửa tệp `config.json` của Zalo để ghi đè `db_path.location` mà không có thao tác của người dùng trong Zalo.
  - **CẤM** chặn, đổi tên hay vô hiệu hóa `Update.exe` / máy chủ cập nhật của Zalo.

---

### 2.2. Nhóm macOS (`IR-MAC-01` .. `IR-MAC-04`)

---

#### `IR-MAC-01`: Sửa hành vi `TMPDIR` tránh trần `104-byte` `sockaddr_un` trên Darwin & Pre-seed `zmeta.json` / `migrate-config.json`

- **Mức ưu tiên & Phân nhóm giá trị:** **`P0`** *(Nhóm 3: Gỡ bỏ nguy cơ chặn đứng khởi chạy phiên thứ hai & ngăn quét di trú dữ liệu từ phiên chính trên macOS)*.
- **Agent chịu trách nhiệm:** `macOS agent` *(phạm vi tệp: `crates/platform-macos/src/{layout.rs,adapter.rs}`)*.
- **Liên kết Finding & Trạng thái bằng chứng (6 tầng):**
  - `F-MAC-01`: **Đã hiệu chỉnh 6 tầng (Recalibrated / Downgraded một phần từ Mission D):**
    - `Confirmed by source code`: `crates/platform-macos/src/layout.rs:97` (`build_open_args`) và `adapter.rs:292,311` hiện truyền `--env TMPDIR=<profile_dir>/tmp`; `layout.rs:34-51` (`ensure_macos_profile_layout`) chưa tạo `zmeta.json` và `migrate-config.json` mà `migration.js:514900,600854` kiểm tra. Đồng thời, giới hạn `char sun_path[104]` (`sizeof(struct sockaddr_un) == 106`) trong `<sys/un.h>` của Darwin SDK, phép chặn kích thước `104` bytes trong user-space `libuv` (`uv_pipe_bind`) cũng như Chromium `ProcessSingletonPosix` (`arraysize(addr.sun_path) == 104`), và logic `base/files/file_util_mac.mm` (`GetTempDir()` gọi `NSTemporaryDirectory()` / `confstr(_CS_DARWIN_USER_TEMP_DIR)` trừ khi `MAC_CHROMIUM_TMPDIR` `< 50` ký tự) là `Confirmed by source code` (dù một số nhánh kernel XNU BSD kiểm tra `SUN_LEN` lên tới `SOCK_MAXADDRLEN = 255`, các struct C tĩnh `sockaddr_un` của Chromium/`libuv` chặn cứng ở `104` bytes bao gồm byte `NUL`).
    - `Inference` / `Not tested` *(Downgraded from Mission D)*: Tương tác thực tế đầu-cuối trên macOS giữa `open -n --env CFFIXED_USER_HOME=...`, `NSTemporaryDirectory()` / `confstr(_CS_DARWIN_USER_TEMP_DIR)` (`/var/folders/...`) và `getenv("TMPDIR")` khi Electron 22 tạo `SingletonSocket` chưa được chạy đo trên phần cứng Mac vật lý, chờ kiểm chứng bằng `TC-H3`.
- **Liên kết Test Case:** `TC-H3`.
- **Vị trí mã nguồn & Chữ ký hàm cần sửa:**
  1. `crates/platform-macos/src/layout.rs:34-51` (`ensure_macos_profile_layout`): Pre-seed `<profile_dir>/Library/Application Support/zmeta.json` và `<profile_dir>/Library/Application Support/ZaloData/migrate-config.json`.
  2. `crates/platform-macos/src/layout.rs:80-104` (`build_open_args`): Loại bỏ `--env TMPDIR=<profile_dir>/tmp`; bổ sung hằng số `pub const DARWIN_SUN_PATH_MAX: usize = 104;` và hàm kiểm tra `pub fn validate_darwin_singleton_socket_headroom(tmp_dir: &Path) -> Result<(), DimError>` (đảm bảo `<tmp_dir>/org.chromium.Chromium.XXXXXX/SingletonSocket` + `1` byte NUL `<= 104` bytes nếu có truyền custom temp dir ngắn).
  3. `crates/platform-macos/src/adapter.rs:287-319` (`MacOsPlatformAdapter::launch` cho nhánh `CustomExecutable` dòng `292` và `NodeFixture` dòng `311`): Loại bỏ `.env("TMPDIR", profile_dir.join("tmp"))` để giữ nguyên `TMPDIR` ngắn mặc định theo phiên người dùng của macOS (`/var/folders/xx/.../T/`, dài ~`49` bytes -> tổng đường dẫn `SingletonSocket` ~`86` bytes `< 104` bytes), hoặc chỉ truyền thư mục tạm riêng nếu độ dài tổng kiểm tra `< 104` bytes.
- **Phát biểu vấn đề (Self-Contained):**
  - Với đường dẫn mặc định `/Users/<user>/Library/Application Support/MultiZaloDesktop/Profiles/<slug>_<8hex>`, chuỗi `<profile_dir>/tmp/org.chromium.Chromium.XXXXXX/SingletonSocket` dài ~`135–155` bytes, vượt xa giới hạn `104` bytes của `sockaddr_un.sun_path` trong `libuv` và Chromium `ProcessSingletonPosix`. Trong khi đó `CFFIXED_USER_HOME=<profile_dir>` đã cô lập hoàn toàn `Library/Application Support/ZaloData`, còn Chromium chủ đích đặt `SingletonSocket` trong `/var/folders/.../T/` rồi trỏ symlink từ `<ZaloData>/SingletonSocket` sang chính nhằm tránh trần 104 byte!
  - Ngoài ra, `ensure_macos_profile_layout` chưa tạo `zmeta.json` và `migrate-config.json` để ngăn module `migration.js` chạy luồng kiểm tra di trú lần đầu.
- **Đặc tả thay đổi kỹ thuật:**
  1. Trong `build_open_args` (`layout.rs:82-104`), truyền `-n -a <app_bundle> --env CFFIXED_USER_HOME=<profile_dir> --env HOME=<profile_dir> --env DIM_PROFILE_ID=<id>` và **bỏ hẳn `--env TMPDIR=...`** (đồng bộ với `adapter.rs:292,311`).
  2. Trong `ensure_macos_profile_layout` (`layout.rs:34-51`):
     - Tạo `<profile_dir>/Library/Application Support/zmeta.json` với nội dung `{"failedMigrateCount":0}` nếu chưa tồn tại.
     - Tạo `<zalo_data>/migrate-config.json` với nội dung `{"zalopc_m_c":true}` nếu chưa tồn tại.
     - Tạo sẵn `<profile_dir>/Library/Preferences` bên cạnh `<profile_dir>/Library/Application Support/ZaloData/media`.
- **Tiêu chí nghiệm thu (Acceptance Criteria):**
  - [ ] Unit test đa nền tảng trong `crates/platform-macos` xác nhận `build_open_args` không chứa `TMPDIR=` trỏ vào `<profile_dir>/tmp`, ngay cả khi `profile_dir` dài `> 110` ký tự.
  - [ ] Unit test xác nhận `ensure_macos_profile_layout` tạo đầy đủ `Library/Application Support/zmeta.json`, `ZaloData/migrate-config.json`, và `.zalo-profile-id`.
  - [ ] Trên máy Mac kiểm thử (`TC-H3`), khởi chạy 2 profile có đường dẫn dài ~`95+` ký tự tạo `SingletonLock` thành công mà không gặp lỗi `ENAMETOOLONG`/`EINVAL` trên `SingletonSocket`.
- **Phương án bị cấm tuyệt đối (Forbidden Approaches):**
  - **CẤM** tạo thư mục tạm dùng chung quyền `0777` không thuộc sở hữu riêng của UID hiện tại trong `/tmp` mà không dùng `mkdtemp` / quyền `0700` (nguy cơ tấn công symlink cục bộ).

---

#### `IR-MAC-02`: Sửa lỗi bắn `SIGTERM` sớm trước vòng chờ `timeout_ms` & Bổ sung chốt bảo vệ DB đang mở trong `MacOsPlatformAdapter::stop`

- **Mức ưu tiên & Phân nhóm giá trị:** **`P0`** *(Nhóm 1: Ngăn chặn hỏng/mất dữ liệu SQLite WAL do ngắt tiến trình ngay khi vừa gọi `terminate()`)*.
- **Agent chịu trách nhiệm:** `macOS agent` *(phạm vi tệp: `crates/platform-macos/src/adapter.rs`)*.
- **Liên kết Finding & Trạng thái bằng chứng (6 tầng):**
  - `F-MAC-02`: `Confirmed by source code` (tại `crates/platform-macos/src/adapter.rs:380-403`, lệnh `osascript` gọi `appRef's terminate()` ở dòng `380-384`, nhưng ngay dòng `387-394` đã gọi `libc::kill(raw_pid, libc::SIGTERM)` **trước** vòng lặp chờ `while std::time::Instant::now() < deadline` ở dòng `397-402`!).
- **Liên kết Test Case:** `TC-H4`.
- **Vị trí mã nguồn & Chữ ký hàm cần sửa:**
  - `crates/platform-macos/src/adapter.rs:358-405` (`MacOsPlatformAdapter::stop`) và bổ sung các hàm nội bộ `has_open_production_db_files(zalo_data_dir: &Path, pid: u32) -> bool`, `has_alive_renderer_children(parent_pid: u32) -> bool`.
- **Phát biểu vấn đề (Self-Contained):**
  - Phương thức `NSRunningApplication.terminate()` trên macOS gửi sự kiện AppleEvent `kAEQuitApplication` bất đồng bộ để Electron đóng các renderer, flush SQLite WAL trong `Database/_production/<uid>/` và thoát sạch (thường cần `300–2500ms`). Trong mã hiện tại (`adapter.rs:387-394`), ngay mili-giây sau khi `osascript` trả về, `is_unix_pid_alive(pid)` đương nhiên vẫn bằng `true`, làm `libc::kill(raw_pid, libc::SIGTERM)` bắn ngay lập tức và vô hiệu hóa hoàn toàn khoảng ân hạn `timeout_ms`!
- **Đặc tả thay đổi kỹ thuật:**
  1. **Đảo đúng thứ tự đóng êm (`Graceful-First Poll`):**
     - Bước 1: Ghi `.quit-signal` (cho fixture) và phát `NSRunningApplication(processIdentifier: pid).terminate()`.
     - Bước 2: **Chạy ngay vòng lặp polling trong suốt `timeout_ms`** (mỗi `50ms` kiểm tra `!is_unix_pid_alive(pid)`). Nếu tiến trình tự thoát trong hạn `timeout_ms`, xóa `.runtime-pid` và trả về `Ok(1)` mà **không bao giờ gửi `SIGTERM`**.
  2. **Kiểm tra an toàn cơ sở dữ liệu trước khi gửi `SIGTERM` sau `timeout_ms`:**
     - Nếu hết `timeout_ms` mà `pid` vẫn sống: kiểm tra xem còn tiến trình con `--type=renderer` thuộc `pid` hoặc còn file descriptor mở bên trong `<zalo_data_dir>/Database/_production/` hay không (qua `lsof -a -p <pid_list> +D <prod_dir>` với timeout ngắn).
     - Nếu DB đang mở trong phiên đã đăng nhập: chờ thêm một nhịp ân hạn ngắn (`<= 2_000ms`) rồi nếu vẫn chưa đóng thì trả về `Err(DimError::ProcessError("graceful quit timed out while production database is still open".into()))` để người dùng đóng cửa sổ an toàn thay vì cưỡng bức cắt ngang ghi DB.
     - Chỉ gửi `libc::kill(raw_pid, libc::SIGTERM)` khi đã hết `timeout_ms` và xác nhận không còn khóa ghi DB trong `Database/_production/` (kèm chờ thêm tối đa `1_500ms` và dọn `.runtime-pid`).
- **Tiêu chí nghiệm thu (Acceptance Criteria):**
  - [ ] Với tiến trình fixture giả lập giữ mở tệp `Database/_production/100001/msg.db` trong `1500ms` sau khi nhận tín hiệu quit rồi mới thoát sạch (với `timeout_ms = 3500`), `MacOsPlatformAdapter::stop` thoát êm sau ~`1500ms` với `0` tín hiệu `SIGTERM` bị bắn sớm (`TC-H4`).
  - [ ] Nếu tiến trình vẫn giữ mở tệp trong `Database/_production/` quá hạn chờ, `stop()` từ chối bắn `SIGKILL` và không làm hỏng tệp DB (`TC-H4`).
- **Phương án bị cấm tuyệt đối (Forbidden Approaches):**
  - **CẤM** gọi `libc::kill(raw_pid, libc::SIGKILL)` (`kill -9`) lên bất kỳ phiên Zalo macOS nào có thư mục `Database/_production/` đã đăng nhập.
  - **CẤM** chạy `killall Zalo` hoặc `pkill -f Zalo` toàn hệ thống (sẽ giết chết phiên Zalo chính của người dùng!).

---

#### `IR-MAC-03`: Xác thực `bundleIdentifier` chống tái sử dụng PID & Kích hoạt cửa sổ bằng AppKit không đòi quyền Accessibility TCC

- **Mức ưu tiên & Phân nhóm giá trị:** **`P0`** *(Nhóm 2: Chống nhận diện sai / đóng nhầm tiến trình khác khi macOS cấp lại số PID cũ trong `SingletonLock`)* + **`P1`** *(Nhóm 5: Đưa cửa sổ lên trước qua AppKit mà không kích hoạt hộp thoại xin quyền nhạy cảm Accessibility TCC)*.
- **Agent chịu trách nhiệm:** `macOS agent` *(phạm vi tệp: `crates/platform-macos/src/adapter.rs`)*.
- **Liên kết Finding & Trạng thái bằng chứng (6 tầng):**
  - `F-MAC-03`: `Confirmed by source code` (`inspect_primary` dòng `159` và `inspect_profile` dòng `193` trong `crates/platform-macos/src/adapter.rs` chỉ gọi `is_unix_pid_alive(p)` bằng `libc::kill(raw, 0)` mà không kiểm tra định danh thực thi/bundle; `focus_or_restore` dòng `342-346` gọi `tell application "System Events" to set frontmost...` bắt buộc quyền `kTCCServiceAccessibility`).
- **Liên kết Test Case:** `TC-H4`.
- **Vị trí mã nguồn & Chữ ký hàm cần sửa:**
  1. `crates/platform-macos/src/adapter.rs:147-170` (`inspect_primary`), `180-225` (`inspect_profile`), và `440-453` (`is_unix_pid_alive`): Bổ sung hàm `fn is_verified_zalo_or_fixture_pid(pid: u32) -> bool` kiểm tra cả sự sống (`kill(pid, 0)`) lẫn định danh tiến trình.
  2. `crates/platform-macos/src/adapter.rs:331-356` (`MacOsPlatformAdapter::focus_or_restore`): Thay thế kịch bản AppleScript `System Events` bằng AppKit `NSRunningApplication`.
  3. `crates/platform-macos/src/adapter.rs:358-372` (`MacOsPlatformAdapter::stop`): Gọi lại `is_verified_zalo_or_fixture_pid(pid)` ngay đầu hàm `stop()` như chốt chặn phòng thủ chiều sâu thứ hai trước bất kỳ thao tác dừng nào.
- **Phát biểu vấn đề (Self-Contained):**
  - Symlink `SingletonLock` lưu `<hostname>-<pid>`. Sau khi máy Mac mất điện hoặc khởi động lại, nhân XNU có thể cấp lại đúng số `<pid>` đó cho một tiến trình hệ thống hoặc ứng dụng hoàn toàn khác của cùng người dùng (`kill(pid, 0)` trả về `0`!). Nếu không kiểm tra `bundleIdentifier == "com.vng.zalo"` / đường dẫn thực thi, `inspect_profile` sẽ báo nhầm profile đang `"Running"` (chặn `StaleLockRecoveryManager`), và nguy hiểm hơn là nếu người dùng bấm `Stop`, `stop()` sẽ gọi `terminate()`/`SIGTERM` nhầm vào ứng dụng vô tội đó!
  - Đồng thời, câu lệnh `tell application "System Events"` tại dòng `344` làm macOS bật hộp thoại đòi cấp quyền nhạy cảm *Accessibility* toàn hệ thống cho trình quản lý.
- **Đặc tả thay đổi kỹ thuật:**
  1. **Xác minh định danh PID (`PID Ownership Guard — P0`):** Cài đặt `is_verified_zalo_or_fixture_pid(pid: u32) -> bool` trên macOS sử dụng FFI `proc_pidpath(pid, buf, PROC_PIDPATHINFO_MAXSIZE)` từ `<libproc.h>` (`libc::proc_pidpath` — nhanh dưới `0.1ms`, không cần spawn tiến trình phụ!) kết hợp kiểm tra tên tệp thực thi kết thúc bằng `/Contents/MacOS/Zalo`, `/node`, hoặc `/dim-test-fixture` (hoặc kiểm tra `bundleIdentifier == "com.vng.zalo"` qua AppKit khi cần). Nếu `SingletonLock` tồn tại nhưng `is_verified_zalo_or_fixture_pid(pid) == false`, đánh dấu `running = false` và `stale_lock_detected = true` để bộ dọn khóa cũ xử lý an toàn.
  2. **Kích hoạt AppKit không cần Accessibility TCC (`P1`):** Trong `focus_or_restore` (`adapter.rs:331-356`):
     - Với `NodeFixture`: ghi tệp `.focus-signal` vào `state.zalo_data_dir`.
     - Với `OfficialZalo` / `CustomExecutable`: dùng `osascript` nạp trực tiếp framework `AppKit` với đối số PID kiểu số (tương tự cấu trúc an toàn dòng `380-384`, tuyệt đối không chạm `System Events`):
       ```applescript
       use framework "AppKit"
       on run argv
         set pidVal to (item 1 of argv) as integer
         set appRef to current application's NSRunningApplication's runningApplicationWithProcessIdentifier:pidVal
         if appRef is not missing value then
           appRef's unhide()
           appRef's activateWithOptions:3
         end if
       end run
       ```
- **Tiêu chí nghiệm thu (Acceptance Criteria):**
  - [ ] Khi tạo symlink `SingletonLock` giả lập trỏ tới PID của một tiến trình không phải Zalo/fixture đang sống (ví dụ `/bin/sleep`), `inspect_profile` trả về `running == false` và `stale_lock_detected == true`, đồng thời `stop()` từ chối gửi bất kỳ tín hiệu nào tới PID đó (`TC-H4`).
  - [ ] `focus_or_restore` đưa cửa sổ profile đích lên tiền cảnh mà không gọi `System Events` và không phát sinh yêu cầu cấp quyền Accessibility TCC (`TC-H4`).
- **Phương án bị cấm tuyệt đối (Forbidden Approaches):**
  - **CẤM** sử dụng `System Events` hoặc CGEvent/AXUIElement đòi hỏi quyền `kTCCServiceAccessibility`.

---

#### `IR-MAC-04`: Chẩn đoán quyền sở hữu `/tmp/socketzalo*2021` & Minh bạch UX Thông báo (`CFBundleIdentifier` vs Dock Badge)

- **Mức ưu tiên & Phân nhóm giá trị:** **`P2`** *(Nhóm 6 & Nhóm 7: Minh bạch hóa thông báo/Dock badge & chẩn đoán/điều phối chủ sở hữu socket gọi thoại trên macOS — thực hiện ở Đợt 3)*.
- **Agent chịu trách nhiệm:** `macOS agent` *(phạm vi tệp: `crates/platform-macos/src/{layout.rs,adapter.rs}`)*.
- **Liên kết Finding & Trạng thái bằng chứng (6 tầng):**
  - `F-MAC-04`: `Confirmed by source code` (`CFBundleIdentifier = "com.vng.zalo"`, `app.dock.setBadge` tại `main.js:1013307-1013367`, và `SecureCipher` + `"Zalo Safe Storage"` wrapping per-profile `secure_ks_<hash>` tại `main.js:3561081..3563611`); hành vi định tuyến click trên `usernoted` và số lần nhắc Keychain thực tế trên máy Mac được giữ ở mức `Inference` / `Not tested` (kèm `Author claim` từ `ducngfi`).
  - `F-MAC-05`: `Confirmed by source code` (`main.js:314800` gán cứng `/tmp/socketzalorecv2021` & `/tmp/socketzalosend2021` và gọi `fs.unlink` khi gặp `EADDRINUSE`); hành vi chuyển quyền cuộc gọi thực tế trên Mac ở mức `Inference` / `Not tested` (kèm `Author claim`), kiểm chứng qua `TC-H5`.
- **Liên kết Test Case:** `TC-H5`, `TC-H4` (phần Keychain/activation).
- **Vị trí mã nguồn & Chữ ký hàm cần sửa:**
  1. `crates/platform-macos/src/layout.rs:136-149` (`inspect_call_sockets_at`): Mở rộng để trả về cấu trúc chi tiết `MacOsCallSocketDiag` (đường dẫn socket, inode, PID tiến trình giữ socket đọc qua `lsof -nP -F pcu -U -- <socket_path>` khi chạy trên macOS).
  2. `crates/platform-macos/src/adapter.rs:102-127` (`MacOsPlatformAdapter::capabilities`): Cập nhật `limitations` và `diagnostics` báo cáo rõ:
     - Profile nào (hoặc Zalo chính) đang sở hữu `/tmp/socketzalorecv2021` (cơ chế *last-writer-wins* do `fs.unlink` tại `main.js:314800`).
     - Sự khác biệt giữa **Dock unread badge độc lập theo từng phiên `open -n`** (`app.dock.setBadge`) và **Notification Center gom nhóm chung theo `CFBundleIdentifier ("com.vng.zalo")`**.
     - Tùy chọn hành động điều phối hợp lệ *"Khởi động lại profile này sau cùng để nhận quyền Gọi"* (`stop` êm -> `launch` lại profile đích) chỉ bật sau khi `IR-MAC-01..03` đạt `PASS`.
- **Tiêu chí nghiệm thu (Acceptance Criteria):**
  - [ ] Trên bộ harness socket giả lập của `TC-H5` (`/tmp/dim-test-zcall-recv`, `/tmp/dim-test-zcall-send` — tuyệt đối không đụng `/tmp/socketzalo*2021` thật), `inspect_call_sockets_at` xác định chính xác PID của phiên mở sau cùng vừa thực hiện `unlink` + `bind`.
  - [ ] Báo cáo `capabilities()` ghi nhận chính xác mô hình Dock badge độc lập vs Notification Center dùng chung và cơ chế symlink `Library/Keychains`.
- **Phương án bị cấm tuyệt đối (Forbidden Approaches):**
  - **CẤM** nhân bản `/Applications/Zalo.app` sang bundle mới, sửa `CFBundleIdentifier` trong `Info.plist` hay ký lại bằng `codesign --sign -` (sẽ làm thất bại `SecureCipher.checkAppSigned()` tại `main.js:3562292`, ném lỗi `"The cipher keychain is not supported"`, và làm hỏng hoàn toàn quyền Keychain/TCC!).
  - **CẤM** tự ý `unlink` tệp `/tmp/socketzalo*2021` từ bên ngoài tiến trình Zalo khi đang có cuộc gọi hoạt động.

---

### 2.3. Nhóm Ubuntu Linux (`IR-UBU-01` .. `IR-UBU-05`)

---

#### `IR-UBU-01`: Khắc phục vòng đời tiến trình `bubblewrap` (`--die-with-parent`) & Loại bỏ `SIGKILL` mù trong `LinuxPlatformAdapter::stop`

- **Mức ưu tiên & Phân nhóm giá trị:** **`P0`** *(Nhóm 3: Gỡ lỗi làm tiến trình tự hủy ngay khi CLI thoát + Nhóm 1: Ngăn hỏng SQLite do bắn `SIGKILL` vô điều kiện khi đóng phiên)*.
- **Agent chịu trách nhiệm:** `Ubuntu agent` *(phạm vi tệp: `crates/platform-linux/src/{layout.rs,adapter.rs}`)*.
- **Liên kết Finding & Trạng thái bằng chứng (6 tầng):**
  - `F-UBU-01`: `Confirmed by source code` (bản chính thức của VNG không hỗ trợ Linux tại `main.js:314392` & `file-utilities/index.js:36`; trong `crates/platform-linux/src/layout.rs:84`, `build_bwrap_args` gắn cứng cờ `"--die-with-parent"` khiến nhân Linux bắn `SIGKILL` hủy ứng dụng ngay khoảnh khắc tiến trình cha ngắn hạn `dim-cli launch` thoát!).
  - `F-UBU-03`: `Confirmed by source code` (tại `crates/platform-linux/src/adapter.rs:357`, ngay khi hết `timeout_ms`, hàm `stop()` gọi thẳng `libc::kill(raw_pid, libc::SIGKILL)` mà không kiểm tra xem cơ sở dữ liệu trong `.config/ZaloData/Database/_production/` có đang mở hay không).
- **Liên kết Test Case:** `TC-H8` (Phần 8A).
- **Vị trí mã nguồn & Chữ ký hàm cần sửa:**
  1. `crates/platform-linux/src/layout.rs:82-131` (`build_bwrap_args`): Xóa bỏ phần tử `"--die-with-parent".to_owned()` tại dòng `84`; thêm `"--new-session".to_owned()` để tách phiên điều khiển terminal an toàn.
  2. `crates/platform-linux/src/adapter.rs:279-299` (`LinuxPlatformAdapter::launch`): Tách nhóm tiến trình con (`process_group(0)` qua `std::os::unix::process::CommandExt`) để tiến trình sống độc lập sau khi `dim-cli` kết thúc, và chờ ngắn (`<= 500ms`) nếu cần để `SingletonLock` / `lockfile` kịp ghi PID thực bên trong sandbox.
  3. `crates/platform-linux/src/adapter.rs:320-362` (`LinuxPlatformAdapter::stop`): Thay thế lệnh `libc::kill(raw_pid, libc::SIGKILL)` vô điều kiện tại dòng `357` bằng hàm kiểm tra `has_open_linux_production_db(&state.zalo_data_dir, &candidate_pids) -> bool` quét symlink trong `/proc/<pid>/fd`.
- **Phát biểu vấn đề (Self-Contained):**
  - Kiến trúc CLI/host của `desktop-instance-manager` gọi lệnh ngắn hạn `dim-cli launch <id>` rồi kết thúc tiến trình CLI để trả JSON. Cờ `--die-with-parent` tại `layout.rs:84` cài đặt `prctl(PR_SET_PDEATHSIG, SIGKILL)`, làm toàn bộ phiên ứng dụng trong `bwrap` chết ngay lập tức sau vài chục mili-giây!
  - Ở chiều đóng phiên, `adapter.rs:357` bắn `SIGKILL` ngay sau `timeout_ms` (`3500ms`), có thể cắt ngang quá trình checkpoint SQLite WAL trên ổ đĩa bận và làm hỏng lịch sử tin nhắn trong `.config/ZaloData/Database/_production/`.
- **Đặc tả thay đổi kỹ thuật:**
  1. Xóa `"--die-with-parent"` khỏi `build_bwrap_args` (`layout.rs:84`) và tách process group khi `spawn()` trong `adapter.rs:281-296`.
  2. Trong `LinuxPlatformAdapter::stop` (`adapter.rs:320-362`):
     - Kiểm tra PID không thuộc `protected_primary_pids` và xác minh `/proc/<pid>/comm` / `/proc/<pid>/cmdline` khớp thực thi quản lý (Zalo AppImage / `electron` / `node` / `dim-test-fixture`) để tránh PID reuse.
     - Ghi `.quit-signal`, gửi `SIGTERM`, và chờ vòng lặp `timeout_ms`.
     - Nếu hết `timeout_ms` mà tiến trình vẫn sống: quét `/proc/<pid>/fd/*` của tiến trình chính và các tiến trình con cùng nhóm. **Nếu bất kỳ fd nào trỏ vào `<profile_dir>/.config/ZaloData/Database/_production/`**, tuyệt đối **không** gửi `SIGKILL` — chờ thêm tối đa `2_000ms` cho `SIGTERM` hoàn tất, nếu vẫn còn mở thì trả `Err(DimError::ProcessError("refused SIGKILL while account database in Database/_production is open".into()))`. Chỉ cho phép dọn dẹp tiến trình treo khi không có tệp DB tài khoản nào đang mở, đồng thời xóa `.runtime-pid` khi tiến trình đã thoát.
- **Tiêu chí nghiệm thu (Acceptance Criteria):**
  - [ ] Unit test xác nhận `build_bwrap_args` không chứa `"--die-with-parent"`.
  - [ ] Trên Ubuntu (`TC-H8` 8A), chạy `dim-cli launch <id>` với `prefer_bwrap = true` thoát trả mã `0`, và sau `3 giây` tiến trình fixture bên trong `bwrap` vẫn sống bình thường (`running == true`).
  - [ ] Khi fixture giữ mở một tệp giả lập trong `.config/ZaloData/Database/_production/100001/data.db` quá `timeout_ms`, `stop()` không bắn `SIGKILL` làm đứt ghi tệp (`TC-H8` 8A).
- **Phương án bị cấm tuyệt đối (Forbidden Approaches):**
  - **CẤM** giữ `--die-with-parent` bằng cách ép `dim-cli` chạy treo ngầm vô thời hạn (phá vỡ hợp đồng CLI JSON đồng bộ).
  - **CẤM** bắn `SIGKILL` mù vào tiến trình đang giữ file descriptor mở trong `Database/_production/`.

---

#### `IR-UBU-02`: Bổ sung Bind Mounts tài nguyên Desktop Runtime (`Wayland`/`X11`/`PipeWire`/`D-Bus`/`DRI`/`FUSE`) & Thư mục XDG `user-dirs.dirs`

- **Mức ưu tiên & Phân nhóm giá trị:** **`P0`** *(Nhóm 3: Cho phép mở cửa sổ GUI, âm thanh và truy cập `gnome-keyring` để giữ phiên đăng nhập trong `bwrap`)* + **`P1`** *(Nhóm 4: Khởi tạo `Downloads`, `Documents`, `Pictures` và `.config/user-dirs.dirs` cho hộp thoại tệp)*.
- **Agent chịu trách nhiệm:** `Ubuntu agent` *(phạm vi tệp: `crates/platform-linux/src/{layout.rs,adapter.rs}`)*.
- **Liên kết Finding & Trạng thái bằng chứng (6 tầng):**
  - `F-UBU-02`: `Confirmed by source code` (`ensure_linux_profile_layout` tại `crates/platform-linux/src/layout.rs:22-42` chỉ tạo `.config/ZaloData`, `.local/share`, `.cache`, `tmp`; `build_bwrap_args` tại `layout.rs:82-131` thiếu toàn bộ mount cho `$XDG_RUNTIME_DIR`, `/tmp/.X11-unix`, `/etc/fonts`, `/etc/machine-id`, `/dev/dri`, `/dev/video*`, `/dev/fuse`, cũng như thư mục chứa tệp thực thi `.AppImage`/fixture nếu nằm ngoài `/usr`).
  - `F-UBU-06` *(phần biến môi trường tương thích thông báo)*: `Confirmed by source code` (`VN-Linux-Family/zalo-for-linux` `main.js:724-728` đặt `NOTIFY_IGNORE_PORTAL=1` để tránh lỗi crash `Notification::Close()` của Electron 22 trên backend portal `libnotify`).
- **Liên kết Test Case:** `TC-H8` (Phần 8A).
- **Vị trí mã nguồn & Chữ ký hàm cần sửa:**
  1. `crates/platform-linux/src/layout.rs:22-42` (`ensure_linux_profile_layout`): Tạo các thư mục XDG chuẩn và ghi tệp `.config/user-dirs.dirs`.
  2. `crates/platform-linux/src/layout.rs:46-78` (`build_linux_isolated_env`): Bảo toàn các biến phiên desktop (`XDG_RUNTIME_DIR`, `WAYLAND_DISPLAY`, `DISPLAY`, `XAUTHORITY`, `DBUS_SESSION_BUS_ADDRESS`, `XDG_CURRENT_DESKTOP`, `XDG_SESSION_TYPE`) và đặt mặc định `NOTIFY_IGNORE_PORTAL=1` (nếu người dùng chưa đặt tường minh).
  3. `crates/platform-linux/src/layout.rs:82-131` (`build_bwrap_args`): Bổ sung đầy đủ các cờ `--ro-bind-try`, `--bind-try`, `--dev-bind-try` có kiểm tra sự tồn tại của đường dẫn trên host.
- **Đặc tả thay đổi kỹ thuật:**
  1. **Cấu trúc XDG User Directories (`ensure_linux_profile_layout` — `P1`):**
     - Tạo mặc định `<profile_dir>/Downloads`, `<profile_dir>/Documents/Zalo Received Files`, `<profile_dir>/Pictures`, `<profile_dir>/Desktop`.
     - Nếu `<profile_dir>/.config/user-dirs.dirs` chưa tồn tại, ghi tệp cấu hình chuẩn:
       ```ini
       XDG_DESKTOP_DIR="$HOME/Desktop"
       XDG_DOWNLOAD_DIR="$HOME/Downloads"
       XDG_DOCUMENTS_DIR="$HOME/Documents"
       XDG_PICTURES_DIR="$HOME/Pictures"
       ```
     - Hỗ trợ tùy chọn opt-in symlink `Downloads`/`Desktop` sang `$HOME` thật của host (đảm bảo `unlink_reparse_points_safe` gỡ sạch khi xóa profile, và nếu dùng `bwrap` thì thêm `--bind-try` tương ứng cho thư mục đích opt-in đó).
  2. **Bind Mounts Desktop Runtime cho `bwrap` (`build_bwrap_args` — `P0`):**
     - **Hiển thị & Phông chữ:** `--ro-bind-try /tmp/.X11-unix /tmp/.X11-unix`, `--ro-bind-try <$XAUTHORITY> <$XAUTHORITY>`, `--ro-bind-try <$XDG_RUNTIME_DIR/$WAYLAND_DISPLAY> <...>`, `--ro-bind-try /etc/fonts /etc/fonts`, `--ro-bind-try /etc/machine-id /etc/machine-id`, `--ro-bind-try /var/lib/dbus/machine-id /var/lib/dbus/machine-id`, `--ro-bind-try /etc/localtime /etc/localtime`.
     - **Âm thanh & Session D-Bus (`gnome-keyring` + Thông báo):** Tạo `--dir <$XDG_RUNTIME_DIR>` (quyền `0700`) trong sandbox và chỉ bind chọn lọc các socket cần thiết trong `$XDG_RUNTIME_DIR` gồm `pipewire-0`, `pulse` (`pulse/native`), `bus` (Session D-Bus cho `org.freedesktop.secrets` / `gnome-keyring` và `org.freedesktop.Notifications`), và `dconf` (nếu cần) thông qua `--ro-bind-try` / `--bind-try` (không bind bừa bãi toàn bộ socket nhạy cảm không liên quan trong `/run/user/$UID`).
     - **Tăng tốc GPU, Camera & FUSE:** `--dev-bind-try /dev/dri /dev/dri`, `--dev-bind-try /dev/fuse /dev/fuse` (kèm kiểm tra chẩn đoán thư viện `libfuse.so.2` / gói `libfuse2t64` trên Ubuntu 24.04), và `--dev-bind-try /dev/video0 /dev/video0` (quét các thiết bị `/dev/video*` hiện có).
     - **Đường dẫn thực thi người dùng:** Nếu `executable` nằm ngoài `/usr`, `/bin`, `/lib` và ngoài `profile_dir` (ví dụ `/opt/zalo/Zalo.AppImage` hoặc thư mục `fixtures/`), thêm `--ro-bind` cho chính tệp `executable` (hoặc thư mục chứa script fixture) để `bwrap` nhìn thấy tệp thực thi!
     - Đồng bộ truyền `--setenv` cho `XDG_RUNTIME_DIR`, `DBUS_SESSION_BUS_ADDRESS`, `WAYLAND_DISPLAY`, `DISPLAY`, `NOTIFY_IGNORE_PORTAL=1`.
- **Tiêu chí nghiệm thu (Acceptance Criteria):**
  - [ ] `ensure_linux_profile_layout` tạo đầy đủ `Downloads`, `Documents/Zalo Received Files`, `Pictures`, `Desktop` và tệp `.config/user-dirs.dirs` hợp lệ (`TC-H8` 8A).
  - [ ] Trong `TC-H8` (8A), tiến trình fixture chạy bên trong `bwrap` đọc được `.config/user-dirs.dirs`, nhìn thấy các mount `/etc/fonts`, `/etc/machine-id`, socket runtime giả lập trong `$XDG_RUNTIME_DIR`, biến `NOTIFY_IGNORE_PORTAL=1`, và **không** nhìn thấy thư mục `$HOME/.config/ZaloData` thật của host.
- **Phương án bị cấm tuyệt đối (Forbidden Approaches):**
  - **CẤM** bind toàn bộ thư mục `$HOME` thật của người dùng vào bên trong `bwrap` (làm mất hoàn toàn cô lập dữ liệu giữa các profile).
  - **CẤM** đóng gói hoặc tự tải bất kỳ tệp `.AppImage` / `app.asar` nào của Zalo vào kho mã nguồn.

---

#### `IR-UBU-03`: Hiện thực hóa kích hoạt cửa sổ X11/Wayland & Khôi phục từ Khay hệ thống (`focus_or_restore`)

- **Mức ưu tiên & Phân nhóm giá trị:** **`P1`** *(Nhóm 5: Thay thế hàm rỗng `Ok(())` để hỗ trợ chuyển cửa sổ và đánh thức ứng dụng từ System Tray trên Linux)*.
- **Agent chịu trách nhiệm:** `Ubuntu agent` *(phạm vi tệp: `crates/platform-linux/src/adapter.rs`)*.
- **Liên kết Finding & Trạng thái bằng chứng (6 tầng):**
  - `F-UBU-05`: `Confirmed by source code` (`LinuxPlatformAdapter::focus_or_restore` tại `crates/platform-linux/src/adapter.rs:310-318` hiện là hàm rỗng `Ok(())`; cơ chế nhận sự kiện `second-instance` -> `showMainWindow` của bản cộng đồng xác nhận tại `bbb6f99e.../main.js:556-598`); hành vi chặn đánh cắp tiêu điểm (`xdg_activation_v1` -> banner `"Zalo is ready"`) của GNOME Wayland Mutter ở mức `Inference` / `Not tested` trên runner thực tế, kiểm chứng qua `TC-H8`.
- **Liên kết Test Case:** `TC-H8` (Phần 8A).
- **Vị trí mã nguồn & Chữ ký hàm cần sửa:**
  - `crates/platform-linux/src/adapter.rs:310-318` (`LinuxPlatformAdapter::focus_or_restore`) và `adapter.rs:105-127` (`capabilities`).
- **Phát biểu vấn đề (Self-Contained):**
  - Hiện tại bấm nút Focus cho một profile đang chạy trên Ubuntu không làm gì cả (`Ok(())`), khiến người dùng không thể mở lại cửa sổ đã ẩn xuống khay AppIndicator hoặc phân biệt cửa sổ của profile nào tương ứng với tài khoản nào.
- **Đặc tả thay đổi kỹ thuật:**
  1. Trong `LinuxPlatformAdapter::focus_or_restore(target, profile, profile_dir, state)`:
     - Kiểm tra `!state.running` -> trả `Err(DimError::NotRunning(profile.name.clone()))`.
     - Với `LaunchTarget::NodeFixture { .. }`: ghi dấu thời gian RFC 3339 vào `<state.zalo_data_dir>/.focus-signal` để fixture ghi nhận sự kiện kích hoạt.
     - Với `LaunchTarget::CustomExecutable { path, args }`: khởi chạy một tiến trình ngắn hạn của chính `path` với cùng `build_linux_isolated_env(profile, profile_dir)` (hoặc `build_bwrap_args` nếu `self.prefer_bwrap` bật). Tiến trình thứ hai này sẽ gặp `SingletonLock` trong `<profile_dir>/.config/ZaloData`, chuyển giao tín hiệu `second-instance` cho tiến trình chính gọi `showMainWindow()` (`restore()` + `show()` + `focus()`) rồi tự thoát ngay.
     - Với `LaunchTarget::OfficialZalo { .. }`: trả `Err(DimError::UnsupportedPlatform(...))`.
  2. Trong `capabilities().limitations` / `diagnostics`: Ghi chú trung thực giới hạn giao thức Wayland trên GNOME (`XDG_SESSION_TYPE=wayland`): khi kích hoạt từ tiến trình nền không có token `xdg_activation_v1` đang hoạt động, GNOME Shell có thể hiển thị thông báo hệ thống `"is ready"` thay vì cưỡng bức nhảy đè lên cửa sổ người dùng đang gõ phím, nhưng cửa sổ đang ẩn dưới khay hệ thống vẫn được `show()` hiển thị trở lại.
- **Tiêu chí nghiệm thu (Acceptance Criteria):**
  - [ ] Gọi `focus_or_restore` trên profile đang chạy ở chế độ `NodeFixture` tạo `.focus-signal` và tăng bộ đếm focus của fixture trong `TC-H8` (8A).
  - [ ] Gọi `focus_or_restore` trên thực thi đơn phiên (`CustomExecutable`) kích hoạt đúng callback `second-instance` của phiên tương ứng mà không mở thêm phiên trùng lặp và không chạm tới các profile khác (`TC-H8` 8A).
- **Phương án bị cấm tuyệt đối (Forbidden Approaches):**
  - **CẤM** cài đặt extension GNOME Shell trái phép hoặc tiêm mã vào compositor Mutter để vượt cơ chế Focus Stealing Prevention của Wayland.

---

#### `IR-UBU-04`: Kiểm tra Preflight chính sách AppArmor Unprivileged User Namespaces trên Ubuntu 23.10+ / 24.04+ (Tuyệt đối không dùng `--no-sandbox`)

- **Mức ưu tiên & Phân nhóm giá trị:** **`P0`** *(Nhóm 3: Phát hiện sớm điểm chặn khởi chạy trên Ubuntu 24.04 LTS và cung cấp chẩn đoán/mẫu cấu hình hợp chuẩn mà không phá vỡ Chromium sandbox)*.
- **Agent chịu trách nhiệm:** `Ubuntu agent` *(phạm vi tệp: `crates/platform-linux/src/{layout.rs,adapter.rs}`)*.
- **Liên kết Finding & Trạng thái bằng chứng (6 tầng):**
  - `F-UBU-04`: `Confirmed by source code` (`crates/platform-linux/src/layout.rs:12-13` đang chứa hằng số `APPARMOR_NO_SANDBOX_NOTE` nhắc tới `--no-sandbox`, và `adapter.rs` hoàn toàn chưa đọc `/proc/sys/kernel/apparmor_restrict_unprivileged_userns`); tác động chặn `unshare(CLONE_NEWUSER)` trên `.AppImage` chưa gắn profile ở Ubuntu 24.04 được phân loại `Confirmed by source code` (tài liệu nhân Ubuntu / `known-issues.md:108,119`) + `Inference` / `Not tested` trên máy thực tế trong kho này, kiểm chứng qua `TC-H8` (8B).
- **Liên kết Test Case:** `TC-H8` (Phần 8B).
- **Vị trí mã nguồn & Chữ ký hàm cần sửa:**
  1. `crates/platform-linux/src/layout.rs:11-14`: Đổi tên và viết lại nội dung hằng số cảnh báo sang `pub const APPARMOR_USERNS_POLICY_NOTE: &str` (xóa hoàn toàn gợi ý dùng `--no-sandbox`; thay bằng hướng dẫn cấu hình AppArmor profile đường dẫn cố định tại `/etc/apparmor.d/`).
  2. `crates/platform-linux/src/layout.rs`: Bổ sung bộ kiểm tra thuần Rust có thể truyền đường dẫn sysfs/procfs giả lập để viết unit test đa nền tảng:
     ```rust
     #[derive(Debug, Clone, PartialEq, Eq)]
     pub struct AppArmorPreflightStatus {
         pub apparmor_enabled: bool,
         pub restrict_unprivileged_userns: bool,
         pub target_is_unprofiled_appimage: bool,
         pub blocked: bool,
         pub remediation_profile_snippet: Option<String>,
     }
     pub fn evaluate_apparmor_userns_at(
         apparmor_enabled_path: &Path,
         restrict_sysctl_path: &Path,
         apparmor_profiles_dir: &Path,
         target_executable: Option<&Path>,
     ) -> AppArmorPreflightStatus;
     ```
  3. `crates/platform-linux/src/adapter.rs:48-127` (`discover_installation`, `capabilities`) & `adapter.rs:241-308` (`launch`).
- **Phát biểu vấn đề (Self-Contained):**
  - Trên Ubuntu 23.10+ và 24.04 LTS, nhân Linux bật mặc định `/proc/sys/kernel/apparmor_restrict_unprivileged_userns = 1`. Khi người dùng chạy một tệp `.AppImage` chưa có AppArmor profile riêng (mount tạm ngẫu nhiên tại `/tmp/.mount_ZaloXXXXXX/zalo`), `chrome-sandbox` của Electron bị từ chối `EACCES` và sập ngay lúc khởi chạy. Hằng số `APPARMOR_NO_SANDBOX_NOTE` hiện tại trong `layout.rs:12-13` vô tình nhắc tới `--no-sandbox`, đi ngược lại nguyên tắc bảo mật không bao giờ vô hiệu hóa sandbox của ứng dụng nhắn tin!
- **Đặc tả thay đổi kỹ thuật:**
  1. Đọc chỉ đọc `/sys/module/apparmor/parameters/enabled` (`"Y"`) và `/proc/sys/kernel/apparmor_restrict_unprivileged_userns` (`"1"`).
  2. Khi cả hai cùng bật và `target_executable` là `.AppImage` (hoặc thực thi Electron chưa có profile tương ứng trong `/etc/apparmor.d/`):
     - `discover_installation(Some(custom))` và `capabilities()` chuyển `appimage_status` / `support_status` sang **`SupportStatus::Blocked`** (thay vì `Experimental` mù quáng) kèm chẩn đoán rõ ràng và mẫu tệp cấu hình chuẩn `/etc/apparmor.d/opt.zalo` dành cho quản trị viên (chỉ cấp quyền `userns,` cho đường dẫn cố định như `/opt/zalo/Zalo.AppImage`, giữ nguyên 100% Chromium renderer sandbox!).
     - Trong `launch()`, nếu `blocked == true`, trả về lỗi tường minh với hướng dẫn cấu hình AppArmor hợp chuẩn thay vì để Electron crash ngầm với mã lỗi khó hiểu.
- **Tiêu chí nghiệm thu (Acceptance Criteria):**
  - [ ] Không còn bất kỳ dòng mã hay chuỗi thông báo nào trong `crates/platform-linux` đề xuất người dùng gắn `--no-sandbox`, `ELECTRON_DISABLE_SANDBOX=1` hay chạy `sysctl ... = 0`.
  - [ ] Kiểm thử `evaluate_apparmor_userns_at` với tệp giả lập `enabled = "Y\n"`, `restrict = "1\n"` và đường dẫn `/home/demo/Zalo.AppImage` không có profile trong thư mục `apparmor.d` giả lập trả về `blocked == true`, `SupportStatus::Blocked`, kèm mẫu AppArmor `abi <abi/4.0>,` có `userns,` (`TC-H8` 8B).
  - [ ] Khi thư mục `apparmor.d` có profile khớp đường dẫn cố định `/opt/zalo/Zalo.AppImage` (hoặc trên Ubuntu 22.04 nơi `restrict == 0`), preflight trả về `blocked == false` (`TC-H8` 8B).
- **Phương án bị cấm tuyệt đối (Forbidden Approaches):**
  - **CẤM TUYỆT ĐỐI** truyền `--no-sandbox` hoặc `--disable-setuid-sandbox` hoặc đặt biến môi trường `ELECTRON_DISABLE_SANDBOX=1` trong `build_bwrap_args` hay `Command::new`.
  - **CẤM TUYỆT ĐỐI** hướng dẫn tắt tính năng bảo vệ nhân qua `sysctl -w kernel.apparmor_restrict_unprivileged_userns=0`.

---

#### `IR-UBU-05`: Tách bạch 4 nguồn giới hạn Gọi điện/Badge trên Ubuntu, Chẩn đoán cổng `29631/29632` & Tùy chọn Rootless Network Namespace (`pasta`)

- **Mức ưu tiên & Phân nhóm giá trị:** **`P2`** *(Nhóm 6 & Nhóm 7: Chẩn đoán xung đột badge/cuộc gọi của bản cộng đồng và móc nối tùy chọn cô lập mạng rootless `pasta` — chỉ thực hiện ở Đợt 3 sau khi hoàn tất `IR-UBU-01..04`)*.
- **Agent chịu trách nhiệm:** `Ubuntu agent` *(phạm vi tệp: `crates/platform-linux/src/{adapter.rs,layout.rs}`)*.
- **Liên kết Finding & Trạng thái bằng chứng (6 tầng):**
  - `F-UBU-06`: `Confirmed by source code` (bản vá cộng đồng `patch-notification-badge.js:859-886` phát tín hiệu `com.canonical.Unity.LauncherEntry.Update` chung cho `application://zalo.desktop`).
  - `F-UBU-07`: **Đã hiệu chỉnh tách 4 nguồn độc lập (4-Way Split Recalibration):**
    - *(A) Giới hạn của bản port cộng đồng (`VN-Linux-Family/zalo-for-linux`):* `Confirmed by source code` (`patch-zcall-native.js:307-318` gán cứng cổng TCP `127.0.0.1:29631` và `127.0.0.1:29632`, tắt phục hồi `EADDRINUSE`).
    - *(B) Lỗi thiết kế của `dim-platform-linux`:* `Confirmed by source code` (đã giải quyết ở `IR-UBU-01..04`).
    - *(C) Ràng buộc môi trường Shared Network Namespace vs Isolated `pasta` Netns:* `Inference` / `Not tested` — lỗi đụng cổng `29631/29632` **chỉ xảy ra khi các phiên dùng chung giao diện loopback `lo`**; nếu bọc mỗi phiên trong một rootless network namespace riêng qua `pasta` (gói `passt`), mỗi phiên có `127.0.0.1` riêng biệt mà vẫn giữ kết nối Internet và dùng chung socket âm thanh PipeWire (`$XDG_RUNTIME_DIR/pipewire-0`, `pulse/native`). Kiểm chứng bằng `TC-H8` (8C).
    - *(D) Giới hạn của Zalo chính thức & phần cứng V4L2:* `Confirmed by source code` (Zalo chính thức của VNG là `UNSUPPORTED` trên Linux) + `Inference` kiến trúc driver nhân Linux `uvcvideo` (khóa độc quyền `EBUSY` trên `/dev/video0` khi 2 tiến trình mở cùng 1 webcam vật lý đồng thời qua V4L2).
- **Liên kết Test Case:** `TC-H8` (Phần 8C).
- **Vị trí mã nguồn & Chữ ký hàm cần sửa:**
  1. `crates/platform-linux/src/layout.rs`:
     - Bổ sung hằng số `pub const COMMUNITY_ZCALL_TCP_PORTS: [u16; 2] = [29631, 29632];`.
     - Bổ sung hàm kiểm tra thụ động `pub fn inspect_loopback_ports_occupied(ports: &[u16]) -> Vec<u16>` (đọc chỉ đọc `/proc/net/tcp` tìm địa chỉ `0100007F:73BF` / `0100007F:73C0` ở trạng thái `0A` `TCP_LISTEN`, không gửi gói tin tới ứng dụng).
     - Bổ sung builder tùy chọn `pub fn wrap_with_pasta_args(inner_program: &Path, inner_args: &[String]) -> Vec<String>` khi người dùng bật `with_isolated_netns(true)` và hệ thống có cài sẵn binary `pasta` trong `PATH`.
  2. `crates/platform-linux/src/adapter.rs:26-45` (`LinuxPlatformAdapter` — thêm cờ tùy chọn `prefer_pasta_netns: bool`) và `adapter.rs:105-127` (`capabilities` — báo cáo rành mạch cả 4 nhóm giới hạn A/B/C/D, trạng thái cổng `29631/29632` trên host netns, sự hiện diện của `pasta`, giới hạn camera đơn phiên V4L2 `/dev/video0`, và hiện tượng ghi đè số đếm badge `zalo.desktop` trên Ubuntu Dock).
- **Tiêu chí nghiệm thu (Acceptance Criteria):**
  - [ ] Báo cáo `capabilities()` trên Linux tách rõ ràng 4 nguồn giới hạn (Official VNG `UNSUPPORTED`, Community TCP `29631/29632` & `LauncherEntry` badge, V4L2 `/dev/video0` single-opener, và Shared netns vs `pasta` isolated netns) — tuyệt đối không đánh đồng mọi phương pháp cô lập đều chung một lỗi!
  - [ ] Hàm đọc thụ động `/proc/net/tcp` phát hiện chính xác khi cổng `127.0.0.1:29631` hoặc `29632` đang ở trạng thái `LISTEN` mà không kết nối vào socket.
  - [ ] Trong `TC-H8` (8C), khi `pasta` có sẵn trên máy Ubuntu kiểm thử, 2 phiên fixture chạy đồng thời trong 2 namespace `pasta` đều `bind` thành công `127.0.0.1:29631` và `127.0.0.1:29632` cùng lúc mà không gặp `EADDRINUSE` và vẫn truy cập được socket PipeWire được bind vào.
- **Phương án bị cấm tuyệt đối (Forbidden Approaches):**
  - **CẤM** sửa tệp `patch-zcall-native.js` hoặc can thiệp vào bên trong gói `.AppImage` của người dùng để đổi cổng TCP.
  - **CẤM** đòi quyền `root` / `CAP_SYS_ADMIN` để tạo network namespace hệ thống.

---

## 3. Master Traceability & Triage Matrix (Bảng Truy Vết Tổng Thể 21 Findings)

Bảng dưới đây hợp nhất toàn bộ **21 Finding chuẩn hóa (`F-CORE-01..02`, `F-WIN-01..07`, `F-MAC-01..05`, `F-UBU-01..07`)** với kết quả hiệu chỉnh bằng chứng 6 tầng từ Mục 1, đặc tả kiểm thử `TC-H*` từ Mục 2, Integration Requests `IR-*` từ Mục 3 và thứ bậc ưu tiên giá trị sản phẩm từ Mục 4:

| Finding ID | Evidence Revision (6-Tier Label & Đối chiếu Mission D) | Test ID | Integration Request ID | Priority & Product-Value Category | Agent Owner | Unresolved Blockers |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **`F-CORE-01`** | `Confirmed by source code` *(Giữ xác nhận mã nguồn)*: `zalo_identity.rs:35-49` đọc `db_path.location` nhưng `coordinator.rs` chưa chặn 2 phiên dùng chung DB ngoài và `backup.rs` bỏ sót DB ngoài `<ProfileDir>`. | `TC-H7` | `IR-WIN-04` | **`P0`** — #1 Nguy cơ mất dữ liệu / hỏng DB | `Core/Windows agent` | Cần xác nhận thêm trên macOS thật xem Zalo 26.x trên Darwin có hiển thị tùy chọn đổi `db_path.location` tương tự Windows hay không. |
| **`F-CORE-02`** | `Confirmed by source code` (`main.js:2441895, 3396325`) + `Confirmed by static binary inspection` (`Update.exe` `ev_9f322e5a...`) *(Giữ xác nhận tĩnh; rủi ro lỗi lazy-load khi swap `.app`/`.AppImage` đang mở được gắn `Inference`)*. | `TC-H7` | `IR-WIN-04` | **`P0`** — #1 Nguy cơ mất dữ liệu / khóa nhị phân khi nâng cấp | `Core/Windows agent` | Quy trình nâng cấp vẫn yêu cầu người dùng bấm nút *"Dừng các phiên phụ"* trước khi cập nhật Zalo chính. |
| **`F-WIN-01`** | `Confirmed by controlled experiment` (`zalo-live-poc-result.json`: 3 phiên phụ + 1 phiên chính chạy song song, SHA-256 `%APPDATA%\ZaloData\*` bất biến) + `Confirmed by source code` (`migration.js:600654`; ghi đè `HKCU` `zalo://` & `Run` tại `main.js:2720633`) *(Giữ xác nhận)*. | `TC-H1B` *(Stage 1)* | `N/A` *(Đã hoạt động ở tầng env; giới hạn `HKCU` xử lý qua tài liệu UX)* | **`P1`** — #5 Đóng/mở lại & quản lý khởi động cùng OS | `Windows agent` | Windows `CreateProcessW` không cô lập `HKCU`; liên kết `zalo://` từ trình duyệt luôn mở về Zalo chính. |
| **`F-WIN-02`** | `Confirmed by source code` *(Giữ xác nhận mã nguồn)*: `win32_ffi.rs:884-908` & `dim-host.ps1:1315-1347` thiếu lọc `Chrome_WidgetWin_1`, `GW_OWNER == 0`, `!WS_EX_TOOLWINDOW` và `>= 400x400`, dễ bắt nhầm `notiWin` (`352x82`, `main.js:2726777`). | `TC-H2` | `IR-WIN-01`, `IR-WIN-02` | **`P0`** — #2 Chống đóng nhầm / focus nhầm cửa sổ | `Windows agent` | Không có blocker hệ điều hành; sửa triệt để trong Rust FFI và C# `HostEngine`. |
| **`F-WIN-03`** | `Confirmed by source code` *(Giữ xác nhận mã nguồn)*: `adapter.rs:642-673` & `dim-host.ps1:2097-2141` thoát sớm sau `SetForegroundWindow` khi bị chặn Foreground Lock; cơ chế handoff `second-instance` xác nhận tại `main.js:3565794`. | `TC-H6` | `IR-WIN-01` | **`P1`** — #5 Chuyển đổi cửa sổ & khôi phục từ System Tray | `Windows agent` | Độ trễ thực tế của handoff `second-instance` khi mở đồng thời 4–5 phiên đã đăng nhập cần đo trong `TC-H6` (mục tiêu `< 2000ms`). |
| **`F-WIN-04`** | `Confirmed by source code` *(Giữ xác nhận mã nguồn)*: `crates/platform-windows/src/layout.rs:14-46` & `dim-host.ps1:1483-1511` thiếu `Downloads`, `Pictures`, `Videos`, `Music`; cần chốt xác minh hậu kiểm cho `unlink_reparse_points_safe` (`store.rs:377-402`). | `TC-H7` | `IR-WIN-03` | **`P0`** *(an toàn xóa junction — #1)* + **`P1`** *(UX hộp thoại tệp — #4)* | `Windows agent` | Cần kiểm tra hiển thị mục *Quick Access* của `IFileOpenDialog` trên Windows 11 khi bật junction `Downloads`. |
| **`F-WIN-05`** | `Confirmed by source code` (`main.js:2720673, 2726777`) + `Confirmed by static binary inspection` (`Zalo.lnk` `ev_69ddf74b...`) *(Giữ xác nhận tĩnh; hành vi click thực tế với 2+ tài khoản đăng nhập là `Not tested` chờ kiểm chứng thủ công)* | `TC-H2` | `IR-WIN-02` | **`P2`** — #6 Thông báo (Notifications) | `Windows agent` | Banner lưu trong lịch sử Windows Action Center luôn gom dưới `com.vng.zalo` và mở về Zalo chính nếu kích hoạt lại qua `Zalo.lnk`. |
| **`F-WIN-06`** | **Hiệu chỉnh / Hạ bậc một phần (Downgraded from Mission D):** Chuỗi cứng `\\.\pipe\PipeZCall*` & gán `O = t => V(e, t)` là `Confirmed by source code` (`main.js:311828..317500`); import WASAPI/UVC là `Confirmed by static binary inspection` (`ev_9145f8bd...`); 1 phiên chính rảnh giữ cả 2 pipe (`PID 15624`) là `Observed on running system`. **Hạ bậc xuống `Inference` / `Not tested`:** thời điểm phát lỗi ở Tài khoản #2 (lúc đăng nhập vs lúc bấm gọi) và việc ghi đè `O` có làm liệt cuộc gọi đi của Tài khoản #1 hay không! | `TC-H1A`, `TC-H1B` *(Stage 2)* | `IR-WIN-02` | **`P2`** — #7 Gọi điện (Audio/video calls — *hạ từ `P0` cũ xuống `P2`*) | `Windows agent` | Kiến trúc Named Pipe toàn cục của Zalo không cho phép 2 phiên cùng giữ `ZaloCall.exe` hoạt động đồng thời mà không can thiệp trái phép; chờ kết quả `TC-H1A`/`TC-H1B` Stage 2. |
| **`F-WIN-07`** | `Confirmed by source code` (`SecureCipher` `secure_ks_<hash>` tại `main.js:3561081`) + `Confirmed by static binary inspection` (Fuse `EnableCookieEncryption` `ev_8e05df8e...`); **Đăng nhập thực tế 2+ tài khoản thật, gửi tin nhắn & giữ phiên sau khởi động lại:** `Not tested` *(Giữ nguyên ranh giới trung thực)*. | `TC-H1B` *(Stage 1)* | `N/A` *(Kiểm chứng cổng chất lượng `L3` thực tế trước khi nâng `SupportStatus`)* | **`P0`** — #3 Đăng nhập độc lập tài khoản thứ hai (`L3`) | `Windows agent` *(phối hợp Human QA opt-in)* | Cần người kiểm thử đồng thuận thực hiện thủ công `TC-H1B` Stage 1 với 2 tài khoản hợp lệ của chính người kiểm thử. |
| **`F-MAC-01`** | **Hiệu chỉnh / Hạ bậc một phần (Downgraded from Mission D runtime claim):** `--env TMPDIR=<profile_dir>/tmp` (`layout.rs:97`), thiếu pre-seed `zmeta.json`/`migrate-config.json` (`layout.rs:34-51`), và giới hạn `sun_path[104]` trong `<sys/un.h>` + `libuv` `uv_pipe_bind` / Chromium `ProcessSingletonPosix` + `file_util_mac.mm` `GetTempDir()` là `Confirmed by source code`. **Hạ bậc xuống `Inference` / `Not tested`:** lỗi vỡ `SingletonSocket` thực tế trên máy Mac (do chưa đo trực tiếp tương tác `CFFIXED_USER_HOME` + `NSTemporaryDirectory()` vs `TMPDIR` trên phần cứng Apple). | `TC-H3` | `IR-MAC-01` | **`P0`** — #3 Chặn đứng khởi chạy phiên thứ hai trên macOS | `macOS agent` | Chưa có runner/máy Mac vật lý (`arm64`/`x86_64`) kết nối trực tiếp trong phiên nghiên cứu hiện tại để chạy `TC-H3`. |
| **`F-MAC-02`** | `Confirmed by source code` *(Giữ xác nhận mã nguồn)*: `crates/platform-macos/src/adapter.rs:391` bắn `libc::kill(raw_pid, libc::SIGTERM)` ngay trước vòng chờ `timeout_ms` tại dòng `397`. | `TC-H4` | `IR-MAC-02` | **`P0`** — #1 Nguy cơ mất dữ liệu / ngắt ngang SQLite WAL khi đóng | `macOS agent` | Cần chạy thực nghiệm `TC-H4` trên macOS thật để đo thời gian thoát trung bình của `Zalo.app` + `ZaloHelper.app`. |
| **`F-MAC-03`** | `Confirmed by source code` *(Giữ xác nhận mã nguồn)*: `adapter.rs:342-346` gọi `System Events` đòi quyền Accessibility TCC; `adapter.rs:159, 193` thiếu kiểm tra `bundleIdentifier == "com.vng.zalo"` / đường dẫn thực thi cho PID đọc từ `SingletonLock`. | `TC-H4` | `IR-MAC-03` | **`P0`** *(chống đóng nhầm PID tái sử dụng — #2)* + **`P1`** *(kích hoạt cửa sổ không TCC — #5)* | `macOS agent` | Cần biên dịch và chạy xác nhận `proc_pidpath` + AppKit `activateWithOptions:3` trên macOS 14+. |
| **`F-MAC-04`** | `Confirmed by source code` (`CFBundleIdentifier = "com.vng.zalo"`, `app.dock.setBadge` tại `main.js:1013307`, `SecureCipher` + `"Zalo Safe Storage"` tại `main.js:3561081`) + `Inference` / `Not tested` cho định tuyến click trên `usernoted` & tần suất hộp thoại Keychain thực tế (kèm tham khảo `Author claim` từ `ducngfi`). | `TC-H4` | `IR-MAC-04` | **`P1`** *(giữ phiên Keychain — #5)* + **`P2`** *(thông báo `usernoted` vs Dock badge — #6)* | `macOS agent` | Chờ kiểm chứng thủ công trên máy Mac xem lần đầu mở phiên phụ có hiện tối đa 1 hộp thoại xác nhận quyền `"Zalo Safe Storage"` hay không. |
| **`F-MAC-05`** | `Confirmed by source code` (`main.js:314800` gán cứng `/tmp/socketzalo*2021` và gọi `fs.unlink` khi `EADDRINUSE`) + `Inference` / `Not tested` cho luồng chuyển quyền gọi E2E trên máy Mac (kèm tham khảo `Author claim`). | `TC-H5` | `IR-MAC-04` | **`P2`** — #7 Gọi điện (Audio/video calls) | `macOS agent` | Hai phiên Zalo trên macOS vẫn không thể gọi đồng thời cùng một giây do dùng chung đường dẫn `/tmp/socketzalo*2021`; chỉ chuyển quyền tuần tự được. |
| **`F-UBU-01`** | `Confirmed by source code` *(Giữ xác nhận mã nguồn)*: Zalo chính thức của VNG không hỗ trợ Linux (`main.js:314392`, `file-utilities/index.js:36`); `crates/platform-linux/src/layout.rs:84` truyền `--die-with-parent` tự hủy tiến trình khi `dim-cli` thoát. | `TC-H8` *(8A)* | `IR-UBU-01` | **`P0`** — #3 Chặn đứng vòng đời tiến trình sau khi khởi chạy | `Ubuntu agent` | Bản Zalo chính thức từ VNG vĩnh viễn ở trạng thái `UNSUPPORTED` cho tới khi VNG phát hành bản Linux chính thức. |
| **`F-UBU-02`** | `Confirmed by source code` *(Giữ xác nhận mã nguồn)*: `crates/platform-linux/src/layout.rs:22-131` thiếu thư mục XDG + `.config/user-dirs.dirs` và thiếu bind `$XDG_RUNTIME_DIR` (Wayland, PipeWire, PulseAudio, D-Bus `gnome-keyring`/Notifications), `/tmp/.X11-unix`, `/etc/fonts`, `/etc/machine-id`, `/dev/dri`, `/dev/video*`, `/dev/fuse`. | `TC-H8` *(8A)* | `IR-UBU-02` | **`P0`** *(mở GUI & `gnome-keyring` — #3)* + **`P1`** *(hộp thoại tệp XDG — #4)* | `Ubuntu agent` | Trên Ubuntu 22.04/24.04 mới cài, người dùng chạy `.AppImage` Type-2 vẫn cần tự cài gói hệ thống `libfuse2` / `libfuse2t64` (và `wl-clipboard` nếu dán ảnh trên Wayland). |
| **`F-UBU-03`** | `Confirmed by source code` *(Giữ xác nhận mã nguồn)*: `crates/platform-linux/src/adapter.rs:357` gọi `libc::kill(raw_pid, libc::SIGKILL)` vô điều kiện ngay sau `timeout_ms`. | `TC-H8` *(8A)* | `IR-UBU-01` | **`P0`** — #1 Nguy cơ mất dữ liệu / hỏng SQLite WAL khi đóng | `Ubuntu agent` | Không có blocker hệ điều hành; sửa trực tiếp trong `LinuxPlatformAdapter::stop`. |
| **`F-UBU-04`** | `Confirmed by source code` (thiếu preflight sysctl trong `adapter.rs` & chuỗi `APPARMOR_NO_SANDBOX_NOTE` chưa chuẩn tại `layout.rs:12-13`) + `Inference` / `Not tested` trên host hiện tại cho tương tác thực tế của AppArmor profile trên Ubuntu 24.04. | `TC-H8` *(8B)* | `IR-UBU-04` | **`P0`** — #3 Chặn khởi chạy tuân thủ bảo mật trên Ubuntu 24.04 | `Ubuntu agent` | Trên Ubuntu 24.04 LTS, người dùng bắt buộc phải nhờ quản trị viên cài AppArmor profile cấp quyền `userns,` cho đường dẫn binary cố định (tuyệt đối không tự vượt bằng `--no-sandbox`). |
| **`F-UBU-05`** | `Confirmed by source code` (`adapter.rs:310-318` là stub rỗng `Ok(())`) + `Inference` / `Not tested` cho hành vi thông báo `"is ready"` của GNOME Wayland `xdg_activation_v1`. | `TC-H8` *(8A)* | `IR-UBU-03` | **`P1`** — #5 Chuyển đổi cửa sổ & khôi phục từ khay hệ thống | `Ubuntu agent` | Trên GNOME Wayland thuần không có extension AppIndicator, bấm nút đóng `X` trên bản cộng đồng sẽ thoát ứng dụng thay vì ẩn xuống khay; Mutter chặn cưỡng bức cướp tiêu điểm không có token. |
| **`F-UBU-06`** | `Confirmed by source code` (bản vá cộng đồng `patch-notification-badge.js:859-886` phát `LauncherEntry.Update` chung cho `application://zalo.desktop`; `main.js:725` cần `NOTIFY_IGNORE_PORTAL=1`) + `Inference` / `Not tested` cho định tuyến click D-Bus trực tiếp trên GNOME Shell. | `TC-H8` *(8A)* | `IR-UBU-02`, `IR-UBU-05` | **`P2`** — #6 Thông báo & Unread Badge trên Ubuntu Dock | `Ubuntu agent` | Số đếm tin chưa đọc trên biểu tượng Ubuntu Dock vẫn dùng chung `application://zalo.desktop` trừ khi người dùng tạo `.desktop` riêng cho từng wrapper. |
| **`F-UBU-07`** | **Hiệu chỉnh tách 4 nguồn (4-Way Split Recalibrated):** (A) Cổng cứng TCP `127.0.0.1:29631/29632` của `zalo-for-linux` (`patch-zcall-native.js:307-318`) & (D) Zalo chính thức `UNSUPPORTED` là `Confirmed by source code`; (B) lỗi thiếu bind của `dim-platform-linux` là `Confirmed by source code`; (C) khả năng cô lập hoàn toàn cổng `29631/29632` bằng rootless netns `pasta` (so với shared netns bị `EADDRINUSE`) và khóa đơn phiên V4L2 `/dev/video0` (`EBUSY`) được phân loại `Inference` / `Not tested`. | `TC-H8` *(8C)* | `IR-UBU-05` | **`P2`** — #7 Gọi điện (Audio/video calls) | `Ubuntu agent` | Ngay cả khi dùng `pasta` giải quyết xung đột cổng TCP `29631/29632`, gọi video đồng thời 2 phiên trên cùng 1 webcam vật lý vẫn chịu khóa phần cứng đơn phiên V4L2 (`/dev/video0` `EBUSY`). |

---

## 4. Checklist Tự Kiểm Tra Trước Khi Bàn Giao Cho Agent Triển Khai (Definition of Done)

Mỗi PR thực thi một `IR-*` từ backlog này chỉ được phép đóng (`Done`) khi vượt qua đủ 5 cổng kiểm tra sau:
1. **Đúng phạm vi tệp (`File Scope`):** Chỉ chạm đúng các tệp thuộc quyền sở hữu của `Agent Owner` đã khai báo ở Mục 0.2 và trong từng `IR-*`.
2. **Không vi phạm bất biến bảo mật:** `0` thay đổi trên nhị phân Zalo, `0` cờ `--no-sandbox` / `ELECTRON_DISABLE_SANDBOX=1`, `0` lệnh can thiệp SAC/AppArmor hệ thống, `0` thao tác chạm vào `protected_pids` hoặc thư mục `ZaloData` chính.
3. **Đồng bộ hợp đồng CLI / Host / UI:** Mọi mã lỗi mới (`SHARED_EXTERNAL_DB_PATH`, `EXTERNAL_DB_OUTSIDE_PROFILE`) và trường JSON chẩn đoán mới trên Windows phải đồng bộ giữa `crates/*`, `dim-cli` và `scripts/dim-host.ps1`.
4. **Đạt toàn bộ `TC-H*` liên kết:** Mọi kiểm thử tự động trong `test-specifications.md` gắn với `IR-*` đó phải chạy `PASS` hoàn toàn bằng `dim-test-fixture` / `fixture-app.mjs` (không dùng tài khoản thật).
5. **Bảo mật thông tin cá nhân trong bằng chứng & log:** Quét xác nhận `0` chuỗi tên người dùng cục bộ hoặc token phiên xuất hiện trong mã nguồn, unit test và tệp báo cáo JSON.
