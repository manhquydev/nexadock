# Zalo Linux repack — Nghiên cứu patch cộng đồng cho 3 triệu chứng còn lại

- Ngày: 2026-11-21
- Phạm vi: read-only research. Nguồn: VN-Linux-Family/zalo-for-linux @ b75697b (clone full history 446 commits), DMKha241/zalo-for-linux, GitHub issues/PRs của cả hai repo, và app gốc 26.11.10 đã trích tại cây làm việc zalo-linux cục bộ.

## 0. Quan hệ hai repo

Working tree của hai repo **giống hệt nhau byte-for-byte** (diff chỉ khác `.git`). DMKha241/zalo-for-linux hiện là mirror của VN-Linux-Family/zalo-for-linux tại đúng commit `b75697b`; DMKha241 là contributor có nhiều PR đã merge upstream (#24, #25, #51, #214, #215, #217, #218). Mọi phân tích dưới đây áp dụng cho cả hai.

## 1. Triệu chứng 2 — Không có nút đóng/thu nhỏ/phóng to (đã có nguyên nhân + patch chính xác)

### Nguyên nhân gốc (tài liệu hoá trong issues #57, #98, #188)

1. Cửa sổ login/main của Zalo được tạo với `frame:!1` + `titleBarStyle:"hidden"` — trên macOS, các nút traffic-light do hệ thống vẽ ở chế độ hiddenInset/hidden, Zalo tự vẽ thanh tiêu đề HTML `#titleBar` (vùng `-webkit-app-region: drag`) nhưng nút đóng/thu nhỏ/phóng chỉ có trên macOS.
2. Trên Linux (Electron 22), `frame:false` = cửa sổ không trang trí → **không có nút nào cả** ở X11 lẫn Wayland.
3. Regression 26.11.10 (issue #188): Zalo 26.11 thêm cửa sổ `title:"Zalo App Usage"` đứng **trước** cửa sổ chính trong `main.js`. Bản patch AST cũ lấy cặp `frame: <bool>` **đầu tiên** trong file nên vá nhầm vào cửa sổ App Usage, cửa sổ chính vẫn `frame:!1`. Kiểm tra "đã vá" (`content.includes('frame:!global.__zaloNativeWayland')`) chỉ hỏi "có chuỗi đó đâu đó không" nên vẫn báo thành công.
4. Riêng Wayland native (`--ozone-platform=wayland`): Electron 22 **không vẽ khung dù `frame:true`**, và `hide()` → `show()` lại cửa sổ đã ẩn bị crash SIGTRAP `gtk_shell::get_gtk_surface already requested` (PR #98, cũ hơn là issue #57 — không kéo được cửa sổ).

### Patch của cộng đồng

| File | Làm gì | Commit/PR |
|---|---|---|
| `scripts/patches/patch-titlebar.js` | Sửa `frame:!1` → `frame:!global.__zaloNativeWayland` trong `app/main-dist/main.js`. Hàm `findMainFramePair()` neo AST vào object có cặp liền kề `minHeight` == `MIN_LOGIN_HEIGHT` (chỉ cửa sổ login/main dùng hằng này) — fix cho #188. Fallback regex: `/T,frame:!(?:1|0)(?![\w.])/g` → `T,frame:!global.__zaloNativeWayland` | `6f86cc6` (PR #189, fix #188); AST migration `85c737c` (PR #182) |
| `plugins/wayland-titlebar/index.js` | Nửa main-process: phát hiện Wayland native (6 tổ hợp cờ/phiên) **trước khi** Zalo tạo cửa sổ → đặt `global.__zaloNativeWayland` (gọi qua `plugins.initEarly` trong `main.js` của launcher); xử lý IPC thu nhỏ/phóng/đóng; đổi `hide()` → `minimize()` cho cửa sổ đã từng hiện để tránh crash Electron 22 | `43fe9ec` (PR #98, closes #57); `fc0b9ad` (minimize thay hide) |
| `scripts/patches/patch-wayland-titlebar.js` | Nối vào `app/main-dist/preload-render.js` một script preload: chỉ bật khi main trả `zalo-wayland-titlebar:enabled === true`; inject 3 nút − □ ✕ (div `#zalo-wayland-controls`, fixed top-right, `z-index:2147483647`) vào `#titleBar` của Zalo; click gửi IPC `zalo-wayland-titlebar:action`; lắng nghe `zalo-wayland-titlebar:maximized` để đổi icon maximize/restore; CSS đẩy `#titleBar .titlebar__btns` sang `right:120px` tránh đè nút update của Zalo | `43fe9ec` |
| `main.js` (launcher) + `plugins/window-state` | `browser-window-created` đặt icon + removeMenu cho mọi cửa sổ; lưu/khôi phục vị trí, kích thước, trạng thái maximized qua `hide/show` (#96) | `4b7326e` (PR #97) |

So với repack hiện tại của bạn: `rebuild-zalo-linux.sh` (dòng 52–64) đã tự làm tương đương `patch-titlebar` cho X11/XWayland (`frame:!1,titleBarStyle:"hidden"` → `frame:!0` + `titleBarStyle` theo platform) — app đã trích có `MIN_LOGIN_HEIGHT,frame:!0`. Phần cộng đồng làm thêm mà bạn chưa có: (a) neo chính xác vào cửa sổ main (bạn thay mọi match của regex — 26.11.10 còn 3 `frame:!1` khác trong file, may là chúng không kèm `titleBarStyle:"hidden"`); (b) cả nhánh Wayland native (frameless + nút inject). Nếu bạn không chạy `--ozone-platform=wayland` thì (b) không cần.

## 2. Triệu chứng 3 — Nút "Đồng bộ ngay" không hoạt động (đã có nguyên nhân + patch chính xác)

### Nguyên nhân gốc (issue #87 + comment trong patch)

Luồng sync tin nhắn của Zalo 26.11 desktop đi qua `SyncController` (Sync V2) và bị chặn tại nhiều tầng trên bản repack Linux:

1. **Network manager (`gwig`) báo sai trạng thái mạng**: renderer/worker đọc `this.getStateNetwork()` thay vì state machine nội bộ `this.stateCur` → luồng khởi động nghiễm nhiên offline; `main-startup` throw `NO_NETWORK (1106)` ("mạng ảo" CORS). Các biến thể probe giả (`_pingToDomain` stub luôn-resolve — "upstream #87, never detects offline", hoặc nối `navigator.onLine` — kẹt false sau resume từ suspend) đều sai; bản fix giữ nguyên probe XHR thật (commit `c655cc0`, PR #113 — reconnect sau khi mất mạng/máy ngủ dậy).
2. **Feature flag Sync V2 do server config**: `transfer_msg` control event chỉ xử lý qua `SyncController._processSync2Ctrl` khi `SyncController.isFeatureEnabled()` đúng; trên Linux cờ này false → **không có nhánh nào chạy** — desktop không gửi push xác nhận sang mobile, nên "Đồng bộ ngay" im lặng không có tác dụng.
3. **`cross_setting` config**: `isEnable()`, `isEnableResume()`, `isEnableBySrc()` đọc config `cross_setting.*` và trả false → mọi sync source (manual/"Đồng bộ ngay", first-login, e2ee-missing…) bị từ chối.

### Patch của cộng đồng — `scripts/patches/patch-network-and-sync-fix.js` (giới thiệu `53d7751`, PR #87; tinh chỉnh `0144208`, `c655cc0` #113)

File vá bằng string/regex replace trên các bundle đã minify:

1. **gwig** (trong `lazy/default-login-main-startup-shared-worker-znotification.*.js`, `compact-app-pc.*.js`, `search-worker.*.js`, `sync-v2-sub-worker.*.js`):
   - `networkConnected(){this.getStateNetwork()!==u.CONNECTED&&` → `networkConnected(){this.stateCur!==u.CONNECTED&&` (và 2 biến thể CHECKING/CONNECTED tương tự);
   - sửa lại hậu quả của patch cũ: `getStateNetwork(){return u.CONNECTED}` → `getStateNetwork(){return this.stateCur}`;
   - **cố ý không đụng** `_pingToDomain` (probe XHR thật mới phát hiện offline/wifi-off và giải CHECKING đúng sau resume).
2. **`lazy/main-startup.*.js`**:
   - guard khởi động `X.getStateNetwork()===Y.DISCONNECT||X.getSocketState()!==Z.OPEN` → `!1`;
   - `checkFeatureEnabled(){...reason:"feature_disabled"...}` → `checkFeatureEnabled(){return{ok:!0}}`;
   - `checkNetwork(){...reason:"network_disconnected"...}` → `checkNetwork(){return{ok:!0}}`;
   - ép `cross_setting`: `isEnable(){return !0}`, `isEnableResume(){return !0}`, `isEnableBySrc(e){return!0}` (cho phép mọi sync source, gồm **manual sync = nút Đồng bộ ngay**).
3. **Route về Sync V1** (mọi bundle ở mục 1): regex
   `case"transfer_msg":[\w.]+\.ModuleContainer\.resolve\([\w.]+\.SyncController\)\.isFeatureEnabled\(\)\?[\w.]+\._processSync2Ctrl\(\w+\[\w+\]\):(…SyncMessageController\.handleTransferAfterLoginCtrlEvents\(\w+\[\w+\]\);break;)` → `case"transfer_msg":$1`
   — bỏ nhánh V2, luôn gọi `SyncMessageController.handleTransferAfterLoginCtrlEvents` (Sync **V1**: gửi push xác nhận sang Zalo mobile để duyệt, rồi lưu dữ liệu qua db-cross-v4/sqlite3). Đồng thời ép `isFeatureEnabled(){return !1}` / `isFeatureEnabledBySyncSource($1){return !1}` của SyncController/syncConfigService để V2 không bao giờ bật lại.
4. **`preload-sqlite.js`**: `cross_setting:{offFeature:!1,isMobileSupport:!1,` → `cross_setting:{enable:!0,offFeature:!1,isMobileSupport:!0,` (+ 3 replace bật gọi nhóm/video call).
5. **Backup data adapter** (`main.js`: `const a=!1,…;const d=!1` → `!0…!0`; `utility-process-sqlite.js`: `const s=!1,…;const l=!1` → `!0…!0`) — bật cờ `--backupdata` của SQLite adapter trong main process.

**Phụ thuộc cứng:** Sync V1 lưu dữ liệu qua addon native `db-cross-v4` ("Backup decryption & E2EE message synchronization", viết lại bằng Rust tại `native/addons/db-cross-v4`, cài bởi `scripts/patches/patch-db-cross-v4.js` vào `app/native/nativelibs/db-cross-v4/prebuilt/linux/electron/<arch>/db-cross-v4-native.node` + vá `dist/binding.js` nhận `process.platform === 'linux'`). Repack của bạn hiện **stub db-cross-v4** (chỉ có `prebuilt/darwin`, shim bắt `MODULE_NOT_FOUND` → Proxy trả lỗi qua callback) → bước lưu DB của sync sẽ lỗi. Đây là điều kiện bắt buộc để symptoms 3 chạy trọn.

Kèm theo: **platform ID** trong `main-dist/main.js` — module `iqBP`, hàm `getClientType()`: `case"LINUX":return 25;` → `case"LINUX":return 23;` (giả macOS). Comment trong `patch-db-cross-v4.js`: bắt buộc "để các feature của patch chạy được"; khác biệt quan sát được duy nhất là icon tray (25 → icon Zalo riêng hiện thêm cạnh icon launcher; 23 → chỉ icon launcher). Ví #87 vá platform ID cùng lúc với sync.

## 3. Triệu chứng 1 — Thoát app → mở lại phải quét QR (session không persist)

### Kết quả research chính: cộng đồng KHÔNG có patch riêng cho triệu chứng này

Đã kiểm tra toàn bộ: 35 file `scripts/patches/*.js`, toàn bộ `plugins/`, `docs/` (VI+EN), toàn bộ 446 commit (grep session/login/QR/remember/persist), và issue tracker của cả hai repo. `docs/user/known-issues.vi.md` chỉ ghi nhận lỗi **đơ màn hình login** (Issue #13 — thiếu binary sqlite3 Linux, fix `de360b9`/`2eb930e`), không có mục nào về mất phiên/QR lại. Kết luận: trong bản dựng cộng đồng, session persist **tự chạy đúng** khi các mảnh nền tảng đúng — không cần patch thêm. Các cơ chế giữ phiên mà bản dựng của họ dựa vào:

1. **sqlite3 native Linux** — bạn đã làm ✓ (login chạy, DB user `2286909460042304184` đã ghi đầy đủ trong `~/.local/share/zalo-linux/default/Database/_production/…`).
2. **Quit đúng luồng graceful của Zalo**: `appQuitFlow.requestQuitApp()` (main.js của app) → `signalBeforeQuitToRender()` gửi IPC `before-quit` cho renderer → renderer lưu state/flush → quit lần nữa. Launcher cộng đồng không bao giờ kill cứng; mọi đường thoát (tray "Thoát", close-to-quit khi không có tray host, logout) đều đi qua `requestQuitApp` — `patch-quit-no-show.js` (`da24861`, PR #105) chỉ bỏ `mainWindow.show()` trên Linux trong hàm này, giữ nguyên handshake. Trên app gốc: `window-all-closed` → `"darwin"!==process.platform && app.quit()`.
3. **Credential store bền**: `zsafe-storage`/`pin-vault` (`userData/pin-vault-data/pin-vault.json`) — khoá mã hoá bằng Electron `safeStorage` (prefix `k:`) hoặc fallback key AES-256-GCM (prefix `f:`, khoá lưu trong store). Trên máy bạn: `pin-vault.json` có entry `k:` → safeStorage **available và persist tốt** ✓ (đã đối chiếu thực tế trên `~/.local/share/zalo-linux/default`).
4. **`checkAppSigned()`** — bạn đã vá rồi (rebuild script chèn `if("darwin"!==process.platform){this.isAppSigned=!1;return!1;}` — trùng khít `patch-linux-guards.js` `PLATFORM_GUARD` của cộng đồng; fix việc spawn `codesign` ENOENT làm gãy promise chain login). ✓
5. **os-sys-info** — đã loại trừ: addon cộng đồng (`native/addons/os-sys-info/src/lib.rs`) chỉ cung cấp `getOsArchitecture`, `getOsMachine` (x64/arm64 qua `uname`), `getProcessSnapshot` — **không** liên quan device fingerprint/định danh thiết bị. Không phải nguyên nhân QR.

### Nguyên nhân khả dĩ còn lại cho repack của bạn (xếp hạng theo evidence)

| # | Giả thuyết | Evidence | Cách xác minh | Fix tương ứng |
|---|---|---|---|---|
| H1 | **Quit không graceful** — kill process (không có nút close/tray dùng được) làm mất flush `before-quit` của renderer | `patch-quit-no-show.js` ghi chú renderer lưu state qua `before-quit`; `cal/*.zlog` channel `exit-app-flow` log `"request quit app:"` mỗi lần quit graceful | Quit bằng menu app CmdOrCtrl+Q (`Quit → requestQuitApp()`) so với `pkill` — nếu Q không bị QR mà kill bị → H1 | Đảm bảo mọi đường thoát đi qua `requestQuitApp` (thêm tray với item Thoát, hoặc không kill) |
| H2 | **Server chặn resume session cho client_type 25 (LINUX)** — app bạn vẫn gửi `getClientType()=25`; cộng đồng gửi 23 (macOS) | Platform patch nằm ngay trong bộ patch sync (`patch-db-cross-v4.js`), comment "required to make the patch's features work"; khác biệt server-side chưa được cộng đồng chứng minh công khai | Vá 1 dòng `case"LINUX":return 25;` → `return 23;` trong `main-dist/main.js`, login + restart, so sánh | 1-line patch như trên |
| H3 | **db-cross-v4 stub** — mọi lời gọi qua `nativelibs/index.js#dbUtils()` nhận lỗi từ Proxy shim; nếu luồng session/E2EE phụ thuộc nó thì persist gãy | `app/native/nativelibs/db-cross-v4/prebuilt` chỉ có `darwin`; shim stub mọi `.node` fail dlopen | Grep `ZALO_LINUX_STUBBED` env/log khi chạy; hoặc thử thay stub bằng addon Rust thật của cộng đồng (`node native/builder.js native/addons/db-cross-v4`) | Build + cài addon như `patch-db-cross-v4.js` làm |
| H4 | safeStorage/credential store mất giữa các lần chạy | **Đã loại trừ**: `pin-vault.json` persist, entry `k:` giải mã được (app chạy lại không recreate khoá), DB đã ghi | — | — |

Lưu ý BẬT: 3 giả thuyết trên là suy luận từ evidence tĩnh (chưa chạy app, chưa decode được `cal/*.zlog` — format zlog nén riêng của VNG). Cách phân biệt nhanh nhất là thí nghiệm A/B ở cột "Cách xác minh".

## 4. Ma trận trade-off — áp dụng patch nào vào repack của bạn

| Patch | Lợi ích | Rủi ro/chi phí | Khuyến nghị |
|---|---|---|---|
| `patch-network-and-sync-fix.js` | Fix trực tiếp triệu chứng 3 | Replace regex minify — dễ gãy khi Zalo đổi bundle; buộc cần db-cross-v4 thật; giữ V1 thay V2 (V2 là hướng server đang đẩy) | **Áp dụng** khi đã có db-cross-v4 Linux |
| Platform ID 25→23 | Có thể liên quan H2 (QR) + sync; 1 dòng | 2 icon tray (nếu tray Zalo render được) hoặc mất icon Zalo tự vẽ | **Áp dụng** — rủi ro thấp, test cả H2 |
| db-cross-v4 Rust (`native/addons/db-cross-v4`) | Bắt buộc cho sync V1 + E2EE | Phải build Rust (cargo) cho đúng ABI Electron 22 napi | **Bắt buộc** cho triệu chứng 3 |
| `patch-titlebar.js` (bản neo MIN_LOGIN_HEIGHT) | Chống regression khi 26.12+ thêm cửa sổ mới | Regex của bạn hiện thay tất cả match — đủ dùng trên 26.11.10 | Giữ patch hiện tại; cân nhắc neo theo `MIN_LOGIN_HEIGHT` |
| Wayland-native titlebar (plugin + preload) | Nút điều khiển + không crash khi ozone-wayland | Chỉ cần khi chạy `--ozone-platform=wayland` | Chỉ áp dụng nếu cần Wayland native |
| `patch-quit-no-show.js` | Chớp cửa sổ lúc thoát (cosmetic) | Nhỏ | Tùy |

## 5. Giới hạn của research

- Chạy tĩnh, không chạy thử app và không decode được log `cal/*.zlog` (format nhị phân riêng) → H1/H2/H3 chưa có xác nhận runtime; thứ tự ưu tiên dựa trên evidence tĩnh + comment của cộng đồng.
- Server-side hành vi với client_type 25 (H2) là suy luận từ comment patch + phạm vi patch, chưa có issue công khai nào nêu rõ.
- Issue tracker DMK gần như trống (mirror) — toàn bộ evidence đến từ VN-Linux-Family.
- Chưa kiểm tra Electron 22.3.9 cụ thể so với Electron mà DMG đóng gói (rebuild script đọc version từ DMG — đã khớp theo thiết kế).

## 6. Câu hỏi còn mở

1. Bạn hiện thoát app bằng cách nào (menu Quit, pkill, hay đóng cửa sổ)? → quyết định H1.
2. Có chạy (hoặc dự định chạy) `--ozone-platform=wayland` không? → quyết định có cần bộ Wayland-native không.
3. Server có chấp nhận resume session cho client_type 25 không — chỉ trả lời được bằng thí nghiệm H2.
