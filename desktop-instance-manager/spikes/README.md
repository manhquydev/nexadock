# KHOANG Empirical Feasibility Spikes Index

Thư mục này chứa **3 bộ spike thực nghiệm độc lập, có thể tháo rời hoàn toàn** dùng để kiểm chứng kiến trúc cho **ADR-001 (KHOANG Unified Zalo Workspace)**.

> **Cam kết an toàn (Zero Live Mutation)**: Cả 3 spike đều chạy trên fixture kiểm soát hoặc kiểm tra **chỉ đọc** (read-only). Không spike nào sửa đổi tệp nguồn được theo dõi của repo, không chạm vào `%APPDATA%\ZaloData`, `%LOCALAPPDATA%\ZaloPC`, hay gửi tín hiệu/Win32 mutation lên bất kỳ tiến trình `Zalo.exe` / `ZaloCall.exe` / `ZaloCap.exe` sống nào.

---

## Danh mục các Spike

| Spike | Phương án đánh giá | Kết luận | Tệp báo cáo / Kết quả |
|---|---|---|---|
| [`native-hosting/`](./native-hosting/REPORT.md) | **Option A**: Nhúng cửa sổ trực tiếp bằng Win32 `SetParent` | **`BLOCKED` (Từ chối)** — Thất bại 6 kill-criteria (`558px` D3D overflow, `1122ms` host freeze, mất `WM_MOUSEWHEEL`, hạ cấp DPI + `err=87`, `#32770` khóa host, `11` zombie Chromium khi crash) | [`native-hosting/REPORT.md`](./native-hosting/REPORT.md) |
| [`web-workspace/`](./web-workspace/README.md) | **Option B**: Phiên Zalo Web cô lập bằng Tauri 2 / raw `wry::WebViewBuilder::build_as_child` | **`VIABLE (Opt-in Hybrid)`** — Zero-IPC qua raw `wry`, tiết kiệm `693 MB` WS bằng `with_profile_name`, tải `chat.zalo.me` QR sạch, `10 ms` switch | [`web-workspace/README.md`](./web-workspace/README.md) |
| [`orchestration-eval/`](./orchestration-eval/benchmark-results.json) | **Option C**: Điều phối cửa sổ OS nguyên bản + `AccountActivationService` hợp nhất | **`PRODUCTION FOUNDATION` (Áp dụng chính)** — `24/24` & `20/20` (100%) chuyển nhanh chính xác, phát hiện & vá lỗi `IsIconic` + `include_size` (`+83 ms`) | [`orchestration-eval/benchmark-results.json`](./orchestration-eval/benchmark-results.json) |

---

## Hướng dẫn Chạy An toàn trên Fixture

### 1. Option A — `native-hosting/`
Đánh giá `SetParent` trên `native-fixture.exe` (Win32) và `msedge.exe --app` (kiến trúc Chromium/Electron) + đọc chỉ đọc manifest/tiến trình Zalo:

```powershell
$env:Path = "$env:USERPROFILE\.cargo\bin;" + $env:Path
Set-Location desktop-instance-manager/spikes/native-hosting
cargo run --bin zalo-inspect   # Chỉ đọc thông số Zalo.exe & plugins\capture\*.exe
cargo build --bins
.\target\debug\host-poc.exe    # Chạy toàn bộ test suite trên fixture (tự dọn tiến trình & thư mục tạm)
```

### 2. Option B — `web-workspace/`
Đánh giá cô lập lưu trữ, ranh giới bảo mật zero-IPC, độ trễ `hide/show` và RAM WebView2 trên các thư mục tạm `--data-root`:

```powershell
$env:Path = "$env:USERPROFILE\.cargo\bin;" + $env:Path
Set-Location desktop-instance-manager/spikes/web-workspace
cargo build --offline

# Kiểm tra cô lập & zero-IPC trên HTTP fixture cục bộ
.\target\debug\spike-web-workspace.exe --mode isolation-write --data-root "$env:TEMP\khoang-spike-b" --port 18432 --out "$env:TEMP\b-write.json"
.\target\debug\spike-web-workspace.exe --mode isolation-verify --data-root "$env:TEMP\khoang-spike-b" --port 18432 --out "$env:TEMP\b-verify.json"

# Kiểm tra không đăng nhập (chỉ màn hình QR https://chat.zalo.me) & đo RAM shared-env
.\target\debug\spike-web-workspace.exe --mode zalo-live --data-root "$env:TEMP\khoang-spike-b-live" --out "$env:TEMP\b-live.json"
.\target\debug\spike-web-workspace.exe --mode deep-probe --data-root "$env:TEMP\khoang-spike-b-deep" --port 18433 --out "$env:TEMP\b-deep.json"
```
*(Lưu ý: Luôn xóa thư mục tạm `--data-root` sau khi chạy xong.)*

### 3. Option C — `orchestration-eval/`
Đo độ trễ kích hoạt, độ chính xác chuyển nhanh `24/24` & `20/20`, lỗi `IsIconic`, nghẽn `dir_size_no_follow`, và Companion Snap bằng `gui_fixture.exe` + `xf-fixture.mjs`:

```powershell
$env:Path = "$env:USERPROFILE\.cargo\bin;" + $env:Path
Set-Location desktop-instance-manager/spikes/orchestration-eval
cargo run --offline --release
```

---

## Vệ sinh & Tháo gỡ (Hygiene & Removal)

- **Trạng thái hiện tại**: Không còn thư mục `target/` hay thư mục tạm nào bên trong `desktop-instance-manager/spikes/`.
- **Tháo gỡ trước khi hoàn tất nhánh phát hành**: Không có bất kỳ `Cargo.toml` hay tệp sản phẩm nào của repo tham chiếu đến `desktop-instance-manager/spikes/`. Chỉ cần xóa toàn bộ thư mục `desktop-instance-manager/spikes/` trước khi merge/commit bản phát hành cuối.
