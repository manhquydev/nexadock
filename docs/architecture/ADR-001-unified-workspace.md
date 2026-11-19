# ADR-001: Kiến trúc Không gian làm việc Hợp nhất KHOANG (Unified Zalo Workspace)

- **Trạng thái**: Đã phê duyệt (Approved — Stage 1 Empirical Synthesis)
- **Ngày**: 2026-11-19
- **Nhánh triển khai**: `feat/khoang-unified-workspace`
- **Phạm vi**: `apps/desktop`, `crates/core`, `crates/process-manager`, `crates/platform-*`, `crates/profile-store`, hợp đồng IPC Tauri 2 & UI KHOANG
- **Bằng chứng thực nghiệm**:
  - Option A Spike: `desktop-instance-manager/spikes/native-hosting/REPORT.md`
  - Option B Spike: `desktop-instance-manager/spikes/web-workspace/README.md`
  - Option C Spike: `desktop-instance-manager/spikes/orchestration-eval/benchmark-results.json`

---

## 1. Bối cảnh & Mục tiêu sản phẩm (Context & Product Goal)

**KHOANG** (tên ứng dụng giữ nguyên định danh nâng cấp `com.multizalodesktop.manager` và thư mục dữ liệu gốc `MultiZaloDesktop`) chuyển đổi từ mô hình bảng điều khiển rời rạc (`1160×780`, thao tác tách biệt `Mở Zalo` / `Chuyển đến` / `Khôi phục & Mở`, không có thanh tài khoản thường trực và không tương tác được với Zalo gốc) sang **Không gian làm việc Hợp nhất (Unified Zalo Workspace)** theo ngôn ngữ thiết kế **Calm Precision**:

- **Thẩm mỹ & Trải nghiệm**: Phẳng, tối giản, ấm áp (`#F6F5F2` / `#FFFFFF` / `#17675F` trên nền sáng; `#121210` / `#1B1B18` / `#1D8278` trên nền tối), không gradient, không `backdrop-filter`, không giả lập giao diện chat hay logo Zalo, luôn kết hợp **Màu sắc + Biểu tượng/Hình dạng + Nhãn chữ** để an toàn với `forced-colors: active` và chuẩn WCAG AA (`>= 4.5:1`).
- **6 ưu tiên sản phẩm cốt lõi (theo thứ tự nghiêm ngặt)**:
  1. **Không mất dữ liệu (Zero data loss)**: Bảo toàn tuyệt đối profile hiện có tại `Profiles/<slug>_<8hex>/`, cơ chế khóa liên tiến trình (`LockFileEx`/`flock`), sao lưu nguyên tử SHA-256, và bảo vệ bất khả xâm phạm đối với **Zalo gốc (Primary Zalo)** tại `%APPDATA%\ZaloData`.
  2. **Định danh chính xác tài khoản / cửa sổ / phiên (Exact account/window/session)**: Không bao giờ đoán nhầm cửa sổ theo tiêu đề hay gửi sự kiện vào nhầm tiến trình; mọi thao tác đều gắn với cây PID đã xác minh (`GetProcessTimes` chống tái sử dụng PID) và `HWND`/WebContext định danh rõ ràng.
  3. **Một cửa sổ hợp nhất ở những nơi khả thi một cách an toàn (Single-window where viable)**: Cho phép chuyển nhanh tài khoản trong một không gian thống nhất khi cơ chế nền tảng chứng minh được độ ổn định và bảo mật.
  4. **Ổn định dài hạn (Long-term stability)**: Không phụ thuộc vào thủ thuật Win32 dễ vỡ khi Zalo Desktop hoặc OS cập nhật; fail-closed khi không đủ điều kiện nền tảng.
  5. **Giữ nguyên toàn bộ tính năng Zalo Desktop đang hoạt động tốt**: Gọi thoại/video `ZaloCall.exe`, chụp màn hình `ZaloCap.exe`, sao lưu/khôi phục cục bộ, `Documents/Zalo Received Files` cô lập, `WinVerifyTrust` chữ ký `VNG GROUP JSC`.
  6. **Giảm tối đa can thiệp vào nội bộ Zalo**: Không inject DLL, không hook nội bộ Electron/ASAR, không `AttachThreadInput` / `SendInput` / `keybd_event`, không sửa biến môi trường hay tệp của Zalo gốc.

---

## 2. Quyết định Kiến trúc Tổng thể (Decision Summary)

Dựa trên kiểm chứng thực nghiệm từ 3 spike độc lập (`native-hosting`, `web-workspace`, `orchestration-eval`) và kiểm tra mã nguồn các crate khóa phiên bản (`tauri 2.12.6`, `wry 0.57.2`, `webview2-com 0.39.1`):

1. **TỪ CHỐI Option A (`SetParent` Native Window Hosting)** — Phân loại **`BLOCKED`**. Phá vỡ 6 tiêu chí sống còn (kill-criteria) đo được trên thực nghiệm; không đưa vào sản phẩm và không bật kể cả ở chế độ thử nghiệm mặc định.
2. **CHỌN Option C (Native Window Orchestration + `AccountActivationService` hợp nhất)** làm **Nền tảng Sản phẩm Chính (Production Foundation)** cho `DesktopPrimary` (Zalo gốc — chỉ đọc, không tự spawn) và `DesktopIsolated` (`Profiles/<slug>_<8hex>/`).
3. **CHỌN Option B (Isolated Web Workspace qua `wry::WebViewBuilder::build_as_child` thuần, zero-IPC)** làm **Chế độ Phiên Web Tự chọn (Opt-in Hybrid Session Mode — `WebIsolated`)**, lưu tách biệt tại `WebSessions/<id>/` (Windows dùng shared-env `with_profile_name`), ưu tiên cho Windows, macOS `>= 14`, và Linux X11; fail-closed trên macOS `< 14` và Linux Wayland.

---

## 3. Ma trận Tương thích 5 Trạng thái (A vs B vs C vs Hybrid Áp dụng)

Quy ước phân loại nghiêm ngặt 5 trạng thái:
- **`SOURCE_REVIEWED`**: Đã kiểm tra mã nguồn crate/OS/repo, chưa chạy kiểm thử thực thi trên cấu hình đó.
- **`FIXTURE_PASS`**: Đạt kiểm chứng thực nghiệm trên fixture kiểm soát (Win32 GUI fixture, Chromium/Edge fixture, `xf-fixture.mjs`, HTTP origin cục bộ).
- **`NATIVE_PASS`**: Đạt kiểm chứng trực tiếp trên phần mềm thật hoặc runtime sản phẩm (`Zalo.exe` chỉ đọc, `dim-desktop.exe`, `https://chat.zalo.me` qua WebView2 157).
- **`BLOCKED`**: Thất bại thực nghiệm hoặc bị khóa cứng bởi giới hạn nền tảng/sandbox/bảo mật.
- **`NOT_TESTED`**: Chủ động không kiểm thử (ví dụ: không đăng nhập tài khoản thật hoặc không chạm dữ liệu Zalo gốc sống).

| Tiêu chí | Option A (`SetParent` Native Hosting) | Option B (`wry` Isolated Web Workspace) | Option C (Native Window Orchestration) | Kiến trúc Hybrid Áp dụng (C + B opt-in) |
|---|---|---|---|---|
| **Nhúng & Thay đổi kích thước (Embed / Resize)** | `BLOCKED` (`558px` D3D overflow, `+36px` chrome offset, 5–22 FPS) | `NATIVE_PASS` (Win WebView2 `build_as_child`; `10.04 ms` `hide/show`) | `FIXTURE_PASS` (cửa sổ OS độc lập; optional Snap `30.09 ms`) | `NATIVE_PASS` / `FIXTURE_PASS` (Web nhúng qua `wry`; Desktop giữ cửa sổ OS độc lập) |
| **Focus & Lan truyền treo UI (Focus / Hang-coupling)** | `BLOCKED` (`1122ms` host freeze khi child stall `1200ms`; `SetFocus` `err=5/87`) | `NATIVE_PASS` (tách biệt theo process WebView; không chặn rail) | `FIXTURE_PASS` (**24/24** & **20/20** 100% đúng HWND, `93.54–94.52 ms` p50; OS foreground interactive `NOT_TESTED`) | `FIXTURE_PASS` (Desktop dùng `AllowSetForegroundWindow` + `ShowWindowAsync`; Web focus nội bộ `wry`) |
| **Tiếng Việt Unicode / IME (Telex / VNI)** | `FIXTURE_PASS` (`WM_CHAR`/IMM32) / `NOT_TESTED` (live TSF/UniKey) | `NATIVE_PASS` (WebView2 157 native) / `SOURCE_REVIEWED` (guard `isComposing \|\| keyCode === 229`) | `NATIVE_PASS` (Zalo Desktop chạy nguyên bản trong cửa sổ OS riêng) | `NATIVE_PASS` (Desktop nguyên bản + WebView2/WKWebView có guard IME) |
| **Chuột, Cuộn & Kéo-thả (Mouse / Scroll / Drag-drop)** | `BLOCKED` (`WM_MOUSEWHEEL` rơi `scrollTop=0`; `WindowFromPoint` xuyên qua DComp) | `NATIVE_PASS` (cuộn/chuột WebView2 chuẩn; `disable_drag_drop_handler` tránh rò rỉ scope) | `NATIVE_PASS` (Zalo Desktop xử lý trực tiếp bởi OS) | `NATIVE_PASS` (Desktop nguyên bản; Web chặn drag-drop Tauri auto-scope) |
| **DPI Scaling (`100%–200%`)** | `BLOCKED` (hạ cấp vĩnh viễn DPI context guest; `err=87` khi mismatch) | `FIXTURE_PASS` (gửi `PhysicalPosition`/`Size` nhân `devicePixelRatio` + `NotifyParentWindowPositionChanged`) | `NATIVE_PASS` (Zalo giữ `<dpiAware>true/pm</dpiAware>`; Companion Snap theo `rcWork`) | `FIXTURE_PASS` + `NATIVE_PASS` (Web tính physical px theo `dpr`; Desktop giữ DPI gốc) |
| **Popup, Hộp thoại & `ZaloCall` / `ZaloCap`** | `BLOCKED` (`#32770` khóa vĩnh viễn host `enabled=false`; `ZaloCall` thoát pane) | `SOURCE_REVIEWED` (`on_new_window` Deny + mở trình duyệt ngoài; WebRTC không đụng named-pipe) | `NATIVE_PASS` (Qt5 `ZaloCall`/`ZaloCap` hoạt động nguyên bản; cảnh báo pipe call trùng) | `NATIVE_PASS` + `SOURCE_REVIEWED` (Desktop giữ `ZaloCall`; Web dùng WebRTC + chặn popup) |
| **Khôi phục khi Host Crash (Crash Recovery)** | `BLOCKED` (`TerminateProcess` tạo `11` zombie Chromium không cửa sổ giữ lock) | `SOURCE_REVIEWED` (WebView2 child đóng theo host; Web không giữ lock `Profiles/`) | `FIXTURE_PASS` (Zalo Desktop sống độc lập; `.runtime-lease.json` + lockfile nhận diện chuẩn) | `FIXTURE_PASS` (Host crash không làm mất hay treo headless Zalo Desktop) |
| **Tách cửa sổ an toàn (Safe Detach)** | `FIXTURE_PASS` (chỉ khi tắt êm; không hoàn tác DPI bị hạ cấp) | `SOURCE_REVIEWED` (không áp dụng — Web là child thuần; đóng/ẩn sạch theo predicate) | `FIXTURE_PASS` (Companion Snap `SetWindowPos` hoàn nguyên chính xác `RECT`) | `FIXTURE_PASS` (Unsnap trả nguyên `RECT`; không bao giờ dùng `SetParent`) |
| **Cô lập lưu trữ (Storage Isolation)** | `NOT_TESTED` (phụ thuộc Option C) | `FIXTURE_PASS` (`localStorage`, `cookie`, `IndexedDB` tách 100%; `with_profile_name` + `ICoreWebView2Profile8::Delete`) | `FIXTURE_PASS` (`Profiles/<slug>_<8hex>/`, `ExternalStorageGuard`, `LockFileEx`, SHA-256 backup) | `FIXTURE_PASS` (Tách biệt hoàn toàn `Profiles/` cho Desktop và `WebSessions/` cho Web) |
| **Ranh giới bảo mật Zero-IPC** | Không áp dụng (hai tiến trình OS riêng) | `NATIVE_PASS` (`raw wry::WebViewBuilder::build_as_child`: 5 globals Tauri = `undefined`, `ipc.localhost` fail) | `FIXTURE_PASS` (Zalo Desktop không có bridge vào KHOANG; `CreateProcessW` env isolation) | `NATIVE_PASS` + `FIXTURE_PASS` (Chặn tuyệt đối `tauri::Window::add_child` cho URL ngoài) |
| **Tải `https://chat.zalo.me` QR thực tế** | Không áp dụng | `NATIVE_PASS` (redirect `id.zalo.me`, `224×224 <svg>` QR, `0` JS/CSP/console errors) | `NATIVE_PASS` (Zalo Desktop gốc) | `NATIVE_PASS` (Xác thực WebRTC/chat sau đăng nhập là `NOT_TESTED` theo quy tắc an toàn) |
| **Tài nguyên RAM / CPU** | `SOURCE_REVIEWED` (`~1.1 GB` WS / Zalo Desktop + host) | `NATIVE_PASS` (`1.36 GB` WS / 14 procs cho 3 Web child dùng shared-env, tiết kiệm `693 MB` & `10` procs so với `2.06 GB`) | `NATIVE_PASS` (`dim-desktop.exe` `345.90 MB` WS tổng / `28.72 MB` Rust host; `inspect` giảm `83 ms`) | `NATIVE_PASS` (Chỉ cấp phát WebView2 khi người dùng bật phiên Web) |
| **Bảo vệ Zalo gốc (Primary Zalo Protection)** | `BLOCKED` (rủi ro chạm HWND/DPI/hang vào Zalo gốc) | `SOURCE_REVIEWED` (Web không chạm `%APPDATA%\ZaloData`) | `FIXTURE_PASS` + `SOURCE_REVIEWED` (chỉ đọc, không spawn, không stop/kill/snap, không ghi `ZaloData`) | `FIXTURE_PASS` + `SOURCE_REVIEWED` (Bất khả xâm phạm cho `DesktopPrimary`) |
| **Đa nền tảng (Win / macOS / Linux)** | `BLOCKED` (Win32 thuần; vô dụng trên macOS/Linux) | `NATIVE_PASS` (Win) / `SOURCE_REVIEWED` (macOS `>=14`, Linux X11; **fail-closed** macOS `<14` & Wayland) | `FIXTURE_PASS` (Win & macOS/Linux unit suites; Linux Desktop chính thức `UNSUPPORTED`) | `FIXTURE_PASS` + `SOURCE_REVIEWED` (Win đầy đủ C+B; macOS `>=14` C+B; Linux ưu tiên B trên X11) |

---

## 4. Tổng hợp Bằng chứng Thực nghiệm & Bác bỏ (Empirical Findings & Falsification Summary)

### 4.1. Option A (Native Window Hosting / `SetParent`) — BỊ TỪ CHỐI (`BLOCKED`)

Spike `desktop-instance-manager/spikes/native-hosting/` đã đo lường trực tiếp trên Windows (120 DPI / 125% scaling, layout `HKL 0x0409`) với **Fixture A** (`native-fixture.exe` Win32) và **Fixture B** (`msedge.exe --app`, cấu trúc `Chrome_WidgetWin_1` + `WS_EX_NOREDIRECTIONBITMAP` + `Chrome_RenderWidgetHostHWND` + GPU `Intermediate D3D Window` tương đồng Zalo Desktop), đồng thời đọc chỉ đọc tiến trình **Zalo Desktop thật (`Zalo-26.10.20.3253`)**. Kết quả cho thấy Option A thất bại trên **6 kill-criteria độc lập**:

1. **Vỡ compositor & tràn kích thước D3D (`BLOCKED`)**: Sau khi gỡ `WS_CAPTION | WS_THICKFRAME | WS_SYSMENU`, `Chrome_RenderWidgetHostHWND` vẫn bị đẩy xuống `+36px` (`top=114` so với container `78`, DOM `ih=471` vs `oh=507`). Quan trọng hơn, cửa sổ GPU mức Low-integrity `Intermediate D3D Window` (`pid=17380`) tràn khỏi khung chứa tới `[982×691]` (`+200px` rộng, `+58px` cao) và lệch tối đa **`558px`** (`max_d3d_delta_px=558`) qua 20 lần resize (`897ms–3880ms`, chỉ đạt 5–22 FPS).
2. **Đóng băng đồng bộ Host UI & lỗi Focus (`BLOCKED`)**: `SetParent` liên tiến trình tự động gắn kết hàng đợi nhập liệu `win32k`. Khi child treo `1200ms`, **luồng UI của KHOANG Host bị đóng băng đồng bộ `1122ms`** (lặp lại giống hệt qua 2 lần đo). Ngoài ra, `SetFocus(child)` thất bại (`err=5 ERROR_ACCESS_DENIED` trên Native, `err=87 ERROR_INVALID_PARAMETER` trên Chromium), `focus_back_rail=false`, và `WindowFromPoint` xuyên qua DirectComposition rơi về `SpikeHostWindowClass`.
3. **Mất cuộn chuột (`WM_MOUSEWHEEL`) trên Chromium (`BLOCKED`)**: Dù click trúng `#chat-input`, `WM_MOUSEWHEEL` gửi tới `Chrome_RenderWidgetHostHWND` bị rơi hoàn toàn (`event=None`, `scrollTop=0`).
4. **Hạ cấp vĩnh viễn DPI Awareness & lỗi `err=87` (`BLOCKED`)**: Khi ghép khác DPI (`PMv2` vào `SYSTEM_AWARE` hoặc `UNAWARE`), `GetWindowDpiAwarenessContext` của guest bị hạ cấp vĩnh viễn xuống `SYSTEM_AWARE`/`UNAWARE (96dpi)` và **không tự phục hồi sau `SetParent(NULL)`**. Ở chiều `UNAWARE` host -> `PMv2` Chromium (`WS_CHILD`), `SetParent` trả về **`err=87 (ERROR_INVALID_PARAMETER)`** ngay cả khi bật `DPI_HOSTING_BEHAVIOR_MIXED`.
5. **Modal File Picker khóa chết toàn bộ KHOANG Host (`BLOCKED`)**: Khi mở `<input type="file">` hoặc `GetOpenFileNameW` trong một tài khoản nhúng, hộp thoại `#32770` chiếm quyền `owner=host.hwnd`, vô hiệu hóa toàn bộ cửa sổ Host (`host_enabled_during=false`, khóa chuyển tài khoản), và **để lại `IsWindowEnabled(host) == false` ngay cả sau khi đóng hộp thoại**! Các popup sở hữu, `window.open()` và context menu (`+78px, +59px`) cũng thoát khỏi khung chứa.
6. **Thảm họa Zombie khi Host crash (`BLOCKED`)**: Khi mô phỏng host chết đột ngột (`TerminateProcess(host, 0xDEAD)`), Native fixture bị win32k gửi `WM_NCDESTROY` tức thì (không có `WM_CLOSE`), còn Chromium chết `7/18` tiến trình (`18->11`) và biến **`11` tiến trình còn lại (bao gồm browser PID) thành zombie vô hình (`visible_top_windows=0`)** tiếp tục giữ khóa lockfile mà người dùng không có cách nào khôi phục UI.
- **Bằng chứng kiến trúc `Zalo-26.10.20.3253` (đọc chỉ đọc)**: `Zalo.exe` chạy `17` tiến trình cho 2 instance (`Medium 0x2000` browser, `Low 0x1000` GPU/Audio, 3–4 renderer `--device-scale-factor=1.25`), khai báo `<dpiAware>true/pm</dpiAware>`, nhưng đi kèm các tiến trình Qt5 độc lập trong `plugins\capture\` (`ZaloCall.exe` `19.6 MB`, `ZaloCap.exe` `5.5 MB`, `ZaviMeet.exe` `19.8 MB`) **không có `<dpiAware>` manifest** và nằm hoàn toàn ngoài cây HWND của `Zalo.exe`, giao tiếp qua named pipes `\\.\pipe\PipeZCallRecv` / `PipeZCallSend`.

### 4.2. Option B (Isolated Web Workspace) — ÁP DỤNG LÀM CHẾ ĐỘ PHIÊN WEB TỰ CHỌN (OPT-IN HYBRID)

Spike `desktop-instance-manager/spikes/web-workspace/` kiểm chứng trên `tauri 2.12.6`, `wry 0.57.2`, WebView2 `157.0.4341.60` chứng minh Option B khả thi cho các phiên Zalo Web phụ trợ cô lập, nhưng **chỉ an toàn khi tuân thủ các rào chắn kỹ thuật sau**:

1. **Bắt buộc dùng `wry::WebViewBuilder::build_as_child` thuần thay vì `tauri::Window::add_child` (Zero-IPC)**:
   - Kiểm chứng phát hiện `tauri::Window::add_child(WebviewUrl::External)` (`tauri-2.12.6/src/manager/webview.rs:168-227`) luôn tiêm `window.__TAURI_INTERNALS__`, `window.ipc`, `window.isTauri=true`, `http://ipc.localhost/` (và `window.__TAURI__` khi `withGlobalTauri: true`), đồng thời tự gọi `scopes.allow_file()` khi drag-drop nếu không tắt.
   - Dùng trực tiếp `wry::WebViewBuilder::new_with_web_context(...).build_as_child(&webview_window)` (với `tauri::WebviewWindow` đã implement `HasWindowHandle`, không cần feature `unstable` của Tauri) đạt **zero-IPC tuyệt đối**: tất cả 5 biến toàn cục (`__TAURI__`, `__TAURI_INTERNALS__`, `__TAURI_IPC__`, `window.ipc`, `isTauri`) đều là `"undefined"`, `fetch('http://ipc.localhost/...')` lỗi `TypeError: Failed to fetch` trên cả fixture lẫn `id.zalo.me`.
2. **Tiết kiệm RAM trên Windows với Shared-Env `with_profile_name`**:
   - Dùng `data_directory` riêng cho mỗi tài khoản tạo mỗi nơi một `ICoreWebView2Environment` 6 tiến trình, tốn **`2,055.7 MB` WS (`1,402.9 MB` Priv) / `24` procs** cho 3 tài khoản Web.
   - Chuyển sang một `ICoreWebView2Environment` dùng chung tại `<data_dir>/WebSessions/_shared_env/` kết hợp `wry::WebViewBuilderExtWindows::with_profile_name` (tên dạng `khoang_web_<id>_g<gen>`) giảm xuống **`1,362.5 MB` WS (`913.0 MB` Priv) / `14` procs** ở 3 tài khoản — **tiết kiệm `693.2 MB` WS và `10` tiến trình** (mỗi tài khoản thêm chỉ tốn `+1` renderer và `~200–238 MB` WS).
3. **Xóa/Reset an toàn với `ICoreWebView2Profile8::Delete()` + Generation Tombstones**:
   - Tuyệt đối không `fs::remove_dir_all` thư mục con trong `_shared_env/EBWebView/` khi sibling WebView đang chạy vì dùng chung `Local State` và lock DB.
   - Khi đang mount: gọi `ICoreWebView2_13::Profile()?.cast::<ICoreWebView2Profile8>()?.Delete()`, sau đó drop `wry::WebView`. Khi reset hoặc runtime `< 119`: gọi `clear_all_browsing_data()`, tăng thế hệ `_g<gen+1>` để cấp phiên sạch ngay lập tức, và ghi profile cũ vào `pending_profile_deletes` trong `web-sessions.json` để dọn khi khởi động hoặc khi không còn child WebView nào mở.
4. **Tốc độ chuyển đổi & Trạng thái nền (`10 ms` / zero reload)**:
   - Chuyển đổi `hide()` / `show()` đồng bộ COM (`n=20`): trung vị **`10.04 ms`** (`11.22 ms` p95), không reload trang (`no_reload=true`). Khi ẩn (`SetIsVisible(false)`), Chromium điều tiết timer `50ms` xuống **1 tick / `500ms`** và có thể tạm ngưng sau ~5 phút. (Cách đặt offscreen `(-10000,-10000)` đạt `3.63 ms` nhưng tốn CPU/GPU nền và rủi ro focus traversal nên chọn `hide/show`).
5. **Tải `https://chat.zalo.me` thực tế (`NATIVE_PASS`)**:
   - Redirect sạch sang `id.zalo.me/account?continue=...`, render đầy đủ `<svg>` QR `224×224` trong `.qr-container`, `0` lỗi JS, `0` vi phạm CSP, `0` lỗi console, `0` tài nguyên lỗi.
6. **Các rào chắn bắt buộc cho `wry` child**:
   - **Guard điều hướng, popup, tải xuống & quyền**: `wry` mặc định **cho phép mọi lượt tải xuống** (`wry-0.57.2/src/lib.rs:1361`). Bắt buộc gắn `with_download_started_handler` (từ chối hoặc hỏi đích lưu an toàn), `with_permission_handler` (từ chối mặc định trừ mic/cam/thông báo do người dùng cho phép), `with_navigation_handler` (chỉ cho phép `https:` trên `zalo.me`, `*.zalo.me`, `*.zaloapp.com` và `about:blank`), `with_new_window_req_handler` (`Deny` + mở trình duyệt mặc định cho `https:`).
   - **Đồng bộ vị trí, DPI & Z-order**: Lắng nghe `WindowEvent::Moved` để gọi `controller.NotifyParentWindowPositionChanged()` (tránh trôi popup `<select>`/IME trên Windows); frontend gửi tọa độ vật lý `Math.round(rect * window.devicePixelRatio)` để chính xác ở cả 125% DPI lẫn 200% zoom; ẩn toàn bộ child WebView (`visible = main_window_visible && active_stage == WebIsolated && overlay_blockers.is_empty()`) trước khi mở `<dialog>`/`[popover]` và tắt `popover="hint"` dạng `inline-end` trên Account Rail khi WebView đang hiển thị (dùng OS `title` + pill header `44px`).
   - **Cổng chặn đa nền tảng (Fail-closed)**: macOS `< 14` (`data_store_identifier` không tồn tại, `wry` âm thầm rơi về `defaultDataStore()` gây rò rỉ cookie) -> chặn cứng `WEB_WORKSPACE_REQUIRES_MACOS_14`. Linux Wayland (`build_as_child` trả `UnsupportedWindowHandle` và Tauri `GtkBox` xếp dọc) -> kiểm tra `RawWindowHandle::Xlib` và chặn cứng `WEB_WORKSPACE_WAYLAND_UNSUPPORTED` trên Wayland.
7. **Bảng so sánh trung thực Zalo Web vs Zalo Desktop (công bố rõ trong UI)**:
   - Không thể thay thế **Zalo gốc (Primary)** hay chạy song song 2 phiên Web cùng 1 tài khoản (Zalo Web chỉ cho phép **1 phiên web hoạt động / tài khoản**, đăng nhập nơi khác sẽ đá phiên cũ).
   - Không có sao lưu/xuất lịch sử cục bộ hay khôi phục nguyên tử SHA-256 như `Profiles/<slug>_<8hex>/`.
   - Gọi thoại/video dùng WebRTC của trình duyệt (không đụng named-pipe `PipeZCallRecv`, nhưng kém ổn định hơn `ZaloCall.exe` khi gọi nhóm/gọi dài và phụ thuộc đồng bộ khóa E2EE từ điện thoại/zCloud).
   - Không xem được lịch sử ngoại tuyến đáng tin cậy; bị điều tiết thông báo khi ẩn cửa sổ quá lâu.

### 4.3. Option C (Native Window Orchestration) — NỀN TẢNG SẢN PHẨM CHÍNH (`PRODUCTION FOUNDATION`)

Spike `desktop-instance-manager/spikes/orchestration-eval/` kiểm chứng toàn diện Option C và phát hiện 2 lỗi nền tảng quan trọng trong mã nguồn hiện tại:

1. **Độ chính xác & Tốc độ chuyển tài khoản (`FIXTURE_PASS`)**:
   - Chuyển nhanh hỗn hợp 4 tài khoản (**24/24 — 100.0%**) và chuyển nhanh liên tục 3 tài khoản phụ (**20/20 — 100.0%**): đúng `HWND`, đúng Z-top, `0` kích hoạt nhầm cửa sổ, `0` bấm nhầm popup `notiWin` `352×82` hay hộp thoại sở hữu, bảo toàn trạng thái Maximize, và **`0` lần chạm vào Zalo gốc**. Độ trễ p50 **`93.54 ms`** (`24` switches) và **`94.52 ms`** (`20` switches).
   - Tra cứu `EnumWindows` 4 tài khoản chỉ tốn **`0.09 ms` p50** (`0.20 ms` max). Khởi chạy lạnh `gui_fixture.exe`: **`35.26–46.07 ms`**; `InstanceCoordinator` + `xf-fixture.mjs`: **`333.30 ms`** đơn lẻ / **`1,705.70 ms`** tường cho 3 profile song song.
   - Tài nguyên KHOANG shell thật (`dim-desktop.exe`, `NATIVE_PASS`): **`28.72 MB`** Rust host, **`345.90 MB`** tổng RAM (gồm 6 tiến trình WebView2), file thực thi `26.61 MB`, `2.08s` CPU trong 3.5s đầu.
2. **Sửa lỗi cửa sổ thu nhỏ (`IsIconic`) bằng `GetWindowPlacement`**:
   - Hiện tại `win32_ffi.rs:1089-1114` và `dim-host.ps1:2183-2187` dùng `GetWindowRect` trả về `[0,986,199,1020]` (`199×34`) cho cửa sổ đang thu nhỏ, phân loại nhầm thành `NotificationPopup` (`None`) và ép mở tiến trình thứ hai (`188.89 ms` p50).
   - Bản vá kiểm tra `IsIconic(hwnd) != 0` và đọc `GetWindowPlacement(hwnd).rcNormalPosition` (`900×650`): khôi phục trực tiếp **8/8 (100%)** tại **`167.20 ms` p50** không cần spawn tiến trình phụ.
3. **Sửa nghẽn cổ chai `include_size` (`+83 ms` / 1,500 tệp)**:
   - `adapter.rs:645` (Windows), `platform-macos/src/adapter.rs:415`, và `platform-linux/src/adapter.rs:638,701` đều gọi `dir_size_no_follow` vô điều kiện kể cả khi không cần dung lượng (`101.98 ms` -> `185.38 ms` với 1,500 tệp; Zalo thật có 10k–50k+ tệp). Tách `inspect_profile_with_options(..., include_size: bool)` và bổ sung `get_workspace_snapshot()` fast-path (`< 15 ms`, không gọi `list_backups`/`verify_isolation` mỗi lần chuyển).
4. **Dịch vụ `AccountActivationService` hợp nhất & Bất biến bảo vệ Zalo gốc**:
   - Hợp nhất các nút rời rạc qua một lệnh `activate_account({ id, kind })` có khóa tuần tự theo profile (`ProfileSyncEntry` + debounce 250ms tránh double-spawn khi ở khay hệ thống), xác minh lại `GetWindowThreadProcessId(hwnd)` trước khi focus, và dùng `AllowSetForegroundWindow` + `ShowWindowAsync`.
   - **Zalo gốc (`DesktopPrimary`) là 100% chỉ đọc và KHÔNG BAO GIỜ SPAWN**: Được phép đọc `inspect_primary()`, tìm `HWND` hợp lệ và focus nếu cửa sổ đang hiển thị hoặc thu nhỏ. **Cấm tuyệt đối** stop/kill/`WM_CLOSE`, ghi vào `%APPDATA%\ZaloData`, đổi biến môi trường, Companion Snap, hoặc tự ý chạy `Zalo.exe` khi Zalo gốc đang tắt/ẩn trong khay (trả `ManualActionRequired` hướng dẫn người dùng mở thủ công).
5. **Companion Side-by-Side Snap (Tùy chọn phụ, TẮT mặc định) & Trung thực tuyệt đối**:
   - Chỉ áp dụng cho `DesktopIsolated` (cấm `DesktopPrimary` bằng lỗi `PRIMARY_PROTECTED`). Dùng `SetWindowPos` theo `rcWork` (`30.09 ms`, `GetParent() == NULL`, giữ nguyên `GWL_STYLE`/`GWL_EXSTYLE`, unsnap trả đúng `RECT` gốc).
   - Không bao giờ giả lập nhúng: giao diện Desktop Stage luôn ghi rõ *"Đang chạy trong cửa sổ Zalo Desktop độc lập — không nhúng vào KHOANG."*

---

## 5. Quyết định Kiến trúc Dài hạn & Lý do (Long-Term Architectural Decisions)

1. **Giữ nguyên Stack Tauri 2 + Vanilla ES/HTML/CSS + Rust Crates**:
   - Không đổi sang Electron, React/Next.js hay framework nặng khác. Lý do: Tauri 2 giữ kích thước gói `26.61 MB`, Rust host `28.72 MB`, kiểm soát trực tiếp Win32 FFI (`dim-platform-windows`), khóa OS (`LockFileEx`/`flock`), bảo mật CSP `default-src 'self'`, và không phá vỡ 102 unit/integration tests hiện có.
2. **Tách biệt hoàn toàn `Profiles/<slug>_<8hex>/` và `WebSessions/`**:
   - `Profiles/` chỉ dành cho `DesktopIsolated` với `.dim-profile.lock`, `.runtime-lease.json`, `zmeta.json`, `migrate-config.json`, `Documents/Zalo Received Files`, và sao lưu SHA-256.
   - `WebSessions/` (`_shared_env/` trên Windows, thư mục riêng trên macOS 14+ / Linux X11) chỉ dành cho `WebIsolated`, quản lý danh mục tại `web-sessions.json`. Tách riêng giúp tránh mọi xung đột lockfile hoặc rủi ro xóa nhầm dữ liệu Desktop khi reset phiên Web.
3. **Giữ nguyên Định danh Nâng cấp (`com.multizalodesktop.manager` & `MultiZaloDesktop`)**:
   - Bảo đảm người dùng nâng cấp từ các bản trước không bị mất dữ liệu, không tạo thư mục dữ liệu thứ hai, và giữ nguyên khóa `localStorage` (`multi-zalo.locale`, `multi-zalo.theme`, `multi-zalo.accountColors`) cùng các khóa mở rộng `khoang.*` và tệp `khoang-preferences.json` (lưu trạng thái ghim `pinned` thay vì sửa `profiles.json`).
4. **Định tuyến mọi điểm vào qua một `AccountActivationService` & Hợp đồng IPC Đóng băng**:
   - Mọi điểm kích hoạt (Left Account Rail, Quick Switcher `Ctrl+K`, phím tắt `Ctrl+1..9`, System Tray menu qua sự kiện `khoang://activate-request`, và thẻ Overview) đều gọi chung `activate_account({ id, kind })`.
   - Hợp đồng IPC đóng băng (`ApiEnvelope { ok, data, error_code, message }`, tham số JS camelCase):
     - `get_workspace_snapshot()` (fast-path)
     - `activate_account({ id, kind })` -> `AccountActivationResult` (`focused | restored | launched | pending | manual_action_required | blocked | failed`)
     - `set_account_pinned({ id, pinned })`
     - `set_companion_snap({ id, enabled })`
     - `web_session_create`, `web_session_show`, `web_session_set_bounds`, `web_session_hide`, `web_session_hide_all`, `web_session_reload`, `web_session_reset`, `web_session_delete`, `web_session_rename`
     - Sự kiện backend: `khoang://navigate`, `khoang://activate-request`, `khoang://snapshot-changed`.

---

## 6. Ma trận Hỗ trợ, Không hỗ trợ & Giới hạn Nền tảng

| Nền tảng / Hệ điều hành | `DesktopPrimary` (Zalo gốc) | `DesktopIsolated` (Option C) | `WebIsolated` (Option B — `wry` child) | Companion Snap (`SetWindowPos`) |
|---|---|---|---|---|
| **Windows 10 / 11** | **Hỗ trợ** (Chỉ đọc, focus khi visible/minimized; không spawn/kill/snap) | **Hỗ trợ đầy đủ** (`Profiles/`, backup SHA-256, `ZaloCall`, tray/focus) | **Hỗ trợ (Opt-in)** (WebView2 shared-env `with_profile_name`, `ICoreWebView2Profile8::Delete`) | **Hỗ trợ (Opt-in, TẮT mặc định)** (chỉ Secondary Desktop) |
| **macOS 14+ (Sonoma trở lên)** | **Hỗ trợ** (Chỉ đọc, `NSRunningApplication` / `CGWindowList`) | **Hỗ trợ** (Cocoa/AppKit focus; kiểm thử trên máy Mac vật lý là `NOT_TESTED`) | **Hỗ trợ (Opt-in)** (`WKWebsiteDataStore` `data_store_identifier` UUID) | **Không hỗ trợ** (giới hạn Win32 `rcWork`) |
| **macOS 11 – 13** | **Hỗ trợ** (Chỉ đọc) | **Hỗ trợ** | **KHÔNG HỖ TRỢ (Fail-closed)** (`WEB_WORKSPACE_REQUIRES_MACOS_14` — tránh rò rỉ `defaultDataStore()`) | **Không hỗ trợ** |
| **Linux X11** | **Không hỗ trợ** (Zalo Desktop không có bản Linux chính thức) | **Hạn chế / Thử nghiệm** (`wmctrl`/`xdotool` theo PID; không có Zalo Linux chính thức) | **Hỗ trợ (Opt-in)** (WebKitGTK `XCreateSimpleWindow`, `WebContext` tách thư mục) | **Không hỗ trợ** |
| **Linux Wayland** | **Không hỗ trợ** | **KHÔNG HỖ TRỢ** (`UNSUPPORTED_OPERATION`, thiếu `xdg-activation-v1`) | **KHÔNG HỖ TRỢ (Fail-closed)** (`WEB_WORKSPACE_WAYLAND_UNSUPPORTED`) | **Không hỗ trợ** |

---

## 7. Chiến lược Rollback & Danh sách Kiểm tra Nghiệm thu (Rollback Strategy & Acceptance Checklist)

### 7.1. Chiến lược Rollback Từng bước
Kiến trúc được thiết kế để hoàn nguyên an toàn với **zero data loss**:
1. **Cô lập nhánh Git**: Toàn bộ thay đổi nằm trên nhánh `feat/khoang-unified-workspace`; nhánh `main` giữ nguyên không bị ghi đè.
2. **Không di trú phá hủy schema (Zero schema-breaking migrations)**:
   - `profiles.json` và cấu trúc `Profiles/<slug>_<8hex>/` giữ nguyên định dạng tương thích ngược; trạng thái `pinned` lưu riêng trong `khoang-preferences.json`.
   - `WebSessions/` và `web-sessions.json` nằm hoàn toàn ngoài `Profiles/`. Nếu tắt hoặc rollback tính năng Web Workspace, dữ liệu `Profiles/` và bản sao lưu Desktop không hề bị ảnh hưởng.
3. **Tùy chọn hoàn nguyên UI/IPC**:
   - Các lệnh Tauri và HTTP bridge cũ (`get_status`, `launch_profile`, `focus_profile`, `stop_profile`, `backup_profile`, `restore_backup`, v.v.) vẫn được duy trì.
   - Để rollback nhanh: tắt hiển thị chế độ tạo Web Session trong UI (hoặc checkout lại `main` và cài đè bản phát hành trước). Người dùng chỉ cần đóng KHOANG, dữ liệu `MultiZaloDesktop\Profiles\` tiếp tục hoạt động bình thường.

### 7.2. Danh sách Kiểm tra Nghiệm thu & Bằng chứng Kiểm chứng (Acceptance Checklist & Verification Evidence)
- [x] **Bảo toàn & Nâng cấp**: Giữ nguyên `com.multizalodesktop.manager`, `MultiZaloDesktop`, đọc đúng `Profiles/<slug>_<8hex>/` cũ, không đụng `%APPDATA%\ZaloData` (`SOURCE_REVIEWED` + `FIXTURE_PASS`).
- [x] **Zalo gốc (`DesktopPrimary`)**: Hiển thị trên Account Rail với huy hiệu khiên chỉ đọc; bấm kích hoạt khi đang mở/thu nhỏ đưa cửa sổ lên trên; trả về `ManualActionRequired` khi ẩn khay/tắt; **0 nút đột biến** (không Stop/Rename/Backup/Delete/Snap) (`FIXTURE_PASS` + `SOURCE_REVIEWED`).
- [x] **Chuyển tài khoản Desktop (`DesktopIsolated`)**: 1 chạm trên Rail / `Ctrl+K` / `Ctrl+1..9` / Tray để mở mới, khôi phục cửa sổ đang thu nhỏ (`GetWindowPlacement`), hoặc đưa cửa sổ đang chạy lên trên; không mở trùng tiến trình; hiển thị trung thực là cửa sổ độc lập (`FIXTURE_PASS` + `NATIVE_PASS` qua `run-tauri-native-e2e.mjs`).
- [x] **Phiên Web Tách biệt (`WebIsolated`) — Tích hợp & Cô lập**: Tạo mới có cảnh báo rõ khác biệt Web vs Desktop; cấu trúc `wry::WebViewBuilder::build_as_child` không Tauri IPC (`window.__TAURI__`/`__TAURI_INTERNALS__`/`window.ipc` `undefined`); `WebViewOverlayCoordinator` ẩn child WebView trước khi mở `<dialog>`/`[popover]`; đồng bộ `Moved` và `devicePixelRatio` (`200%` zoom); `ICoreWebView2Profile8::Delete()` + tombstones (`SOURCE_REVIEWED` + `FIXTURE_PASS` + spike `NATIVE_PASS` trên `chat.zalo.me` QR).
- [x] **Cổng an toàn & Chất lượng (Stage 2A/2B + Phase 2 Hardening)**:
  - `cargo fmt --all -- --check` & `cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml -- --check`: **PASS**
  - `cargo clippy --workspace --all-targets --offline --locked -- -D warnings` & `cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked -- -D warnings`: **PASS**
  - `cargo test --workspace --offline`: **109 passed, 0 failed, 0 ignored** (`FIXTURE_PASS`)
  - `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --offline`: **17 passed, 0 failed, 0 ignored** (`FIXTURE_PASS`)
  - `cargo build --manifest-path apps/desktop/src-tauri/Cargo.toml --release --offline --locked`: **PASS**
  - `node --test apps/desktop/ui/tests/ui-contracts.test.mjs apps/desktop/ui/tests/i18n.test.mjs tests/brand-release-gates.mjs`: **26 passed, 0 failed**
  - `node tests/run-fixture-tests.mjs`: **8/8 passed, 0 failed** (`ok: true`)
  - `node tests/run-contract-parity-tests.mjs`: **PASS** (`ok: true`)
  - `node apps/desktop/ui/tests/visual-cdp.mjs`: **PASS** (16 ma trận + scale 0/1/3/10/20 + Desktop Stage + Quick Switcher + 200% zoom, 0 runtime errors)
  - `node tests/run-tauri-native-e2e.mjs`: **16/16 steps PASS, 0 failed** (`15/15` Windows native user journeys, `real_tauri_ipc_verified: true`, `webview_runtime_errors: 0`)
- [ ] **Hạng mục Chưa Kiểm chứng / Cần Người dùng Xác nhận (`NOT_TESTED` / `BLOCKED_USER_ACTION`)**:
  - Đăng nhập QR thật và gọi WebRTC/thoại sau đăng nhập trên `WebIsolated` (`BLOCKED_USER_ACTION` / `NOT_TESTED` — không chạm tài khoản Zalo thật).
  - Kéo cửa sổ qua nhiều màn hình khác DPI (`multi-monitor DPI drag`) trên `dim-desktop.exe` thực tế (`BLOCKED_USER_ACTION` / `NOT_TESTED`).
  - Giao diện GUI vật lý trên **macOS** (`14+` / `<14`) và **Linux** (X11 / Wayland) (`NOT_TESTED` trên phần cứng ngoài Windows).
  - Đo lường RSS / CPU % tiến trình trong bài kiểm tải J15 (`NOT_TESTED` trong chạy E2E tự động — mới đo độ trễ `1601 ms` với 10 profile + 3 phiên hoạt động).
