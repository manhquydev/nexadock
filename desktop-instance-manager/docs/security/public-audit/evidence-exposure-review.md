# Báo Cáo Kiểm Toán Phơi Nhiễm Bằng Chứng Nghiên Cứu, Đầu Ra REA, Tài Nguyên Độc Quyền & Dữ Liệu Runtime (`manhquydev/zalo`)

- **Nhiệm vụ:** AGENT MISSION F — Đánh giá bảo mật, quyền riêng tư & ranh giới sở hữu trí tuệ cho kho mã công khai (`https://github.com/manhquydev/zalo`)
- **Tác nhân phụ trách:** `@evidence-auditor` (Evidence, REA Output, Proprietary Binary/Code & Runtime Exposure Auditor)
- **Phạm vi chuyên môn:** `Section 3` (Kiểm toán Evidence, REA Output, Binary/Code Độc Quyền & Dữ liệu Runtime/PoC) & `Section 4` (Kiểm tra trực quan Screenshots, Icons, Fixtures & Lịch sử Blob đã xóa)
- **Ngày kiểm toán (UTC):** `2026-11-18`
- **Chế độ kiểm toán:** `100% READ-ONLY` (`git --no-optional-locks` trên toàn bộ `10` worktree `C:\Users\<REDACTED_USER>\Downloads\zalo*`; tuyệt đối không xóa, không sửa đổi bất kỳ tệp bằng chứng nghiên cứu hay mã nguồn sản phẩm nào; không trích dẫn lại mã nguồn độc quyền của Zalo/VNG trong báo cáo)
- **Trạng thái hiển thị kho mã thực tế:** **`PUBLIC`** (`https://github.com/manhquydev/zalo` — `origin/main` @ `8cf4703` và `origin/feat/ui-vi-en-quiet-workspace` @ `0f87302` đang truy cập công khai trên GitHub)

---

## 1. Tóm Tắt Điều Hành (Executive Summary)

Quá trình kiểm toán độc lập đã kiểm kê và phân tích nội dung từng tệp đối với toàn bộ **`300` đường dẫn lịch sử duy nhất** trong kho đối tượng Git (bao gồm `18` sự kiện xóa/đổi tên blob), **`2` nhánh remote đang hiển thị công khai** (`origin/main`, `origin/feat/ui-vi-en-quiet-workspace`), **`10` nhánh/worktree cục bộ** (cả tệp đã theo dõi, tệp sửa đổi, tệp chưa theo dõi `untracked` và thư mục bị `.gitignore`), thư mục gốc `rea/`, toàn bộ `multi-zalo-desktop/`, `desktop-instance-manager/evidence/`, `desktop-instance-manager/fixtures/`, `desktop-instance-manager/plans/reports/`, cùng kiểm tra thị giác trực tiếp (**`100%` bằng mắt qua công cụ đọc ảnh**) trên **`36` ảnh chụp màn hình** (`20` ảnh hiện hành + `16` ảnh đã xóa trong lịch sử Git) và toàn bộ bộ biểu tượng ứng dụng (`.png`, `.ico`, `.icns`).

### Kết luận tổng quan theo 5 chiều đánh giá bắt buộc

1. **Thông tin xác thực, phiên đăng nhập, Cookie, Token & dữ liệu người dùng Zalo thật (`0` rò rỉ mức `P0`):**
   - **Không phát hiện** bất kỳ khóa phiên (`zpw_sek`, `zpsid`), cookie đăng nhập, khóa DPAPI `Local State` (`encrypted_key`), tệp cơ sở dữ liệu `SQLite`/`LevelDB` của tài khoản thật, số điện thoại cá nhân, mã QR đăng nhập, tin nhắn hay danh bạ Zalo thật nào trong toàn bộ lịch sử Git, các nhánh công khai, các worktree cục bộ và toàn bộ `36` ảnh chụp màn hình.
2. **Nhị phân gốc của VNG, gói `app.asar`, V8 Bytecode & tài nguyên PE/Icon trích xuất (`0` tệp nhị phân VNG bị phân phối lại):**
   - **Không có** tệp thực thi của VNG (`Zalo.exe`, `ZaloSetup*.exe`, `ZaloCall.exe`, `ZaloCap.exe`, `ZaviMeet.exe`, `zocr-host.exe`), gói `app.asar`, addon `.node`, V8 snapshot/bytecode hay biểu tượng đồ họa thương hiệu Zalo nào từng được commit hoặc lưu trong cây mã nguồn. Các tệp `rea-pe-resources-*.json` chỉ lưu metadata cấu trúc bảng tài nguyên (`offset`, `rva`, kích thước byte, mã băm `sha256`), `raw_result` rỗng `{}` và không nhúng dữ liệu nhị phân của icon hay manifest. Toàn bộ biểu tượng Tauri của dự án chỉ là hình vuông màu xanh thuần túy tự tạo.
3. **Mã nguồn trích xuất từ `app.asar` không phù hợp để tái phân phối công khai (`1` phát hiện chặn mức `P0` — `EVID-SEC-01`):**
   - Tệp tài liệu nghiên cứu **`multi-zalo-desktop/docs/research/instance-behavior.md` (dòng `51–83`)** — hiện **đang hiển thị công khai trên `origin/main` (`8cf4703`) và `origin/feat/ui-vi-en-quiet-workspace` (`0f87302`)**, đồng thời vẫn còn nguyên vẹn trên `main` (`a5feb4a`) và `integration/reliability-hardening` (`cd62c52`) — **sao chép nguyên văn (verbatim) toàn bộ hàm khởi động `bootstrap()` dài `23` dòng từ `app.asar/bootstrap.js` (`756` bytes)** cùng đoạn mã `5` dòng từ `main-dist/migration.js` của Zalo Desktop. Ngoài ra, `desktop-instance-manager/docs/research/compatibility/macos-findings.md` (dòng `86–92`, trên `research/desktop-compatibility` và `cd62c52`) chứa `5` dòng mã tái dựng sát tên biến nén từ `main.js`. Đây là điểm vi phạm ranh giới bản quyền/phân phối lại mã đóng của VNG rõ ràng nhất, bắt buộc phải thay thế bằng mô tả hành vi phòng sạch (clean-room prose) trước khi đóng cổng sẵn sàng công khai (`PUBLIC_READINESS_PASS`).
4. **Dữ liệu định danh môi trường cục bộ, đường dẫn người dùng Hệ điều hành & đường dẫn máy trạm nội bộ (`3` phát hiện mức `P1` — `EVID-SEC-02`, `EVID-SEC-03`, `EVID-SEC-04`, `EVID-SEC-06`):**
   - Trên **`origin/main` (`8cf4703`) và `origin/feat/ui-vi-en-quiet-workspace` (`0f87302`) đang công khai**, toàn bộ `10` tệp JSON trong `multi-zalo-desktop/evidence/*.json` và `8` tài liệu Markdown nghiên cứu/kiểm thử/thiết kế làm lộ tên tài khoản người dùng Windows cục bộ (`C:\Users\<REDACTED_USER>\...`, tổng cộng **hơn `130` vị trí xuất hiện**). Dù nhánh nội bộ `integration/reliability-hardening` (`bff0674`/`cd62c52`) đã thực hiện một đợt lọc đường dẫn sang `<USERPROFILE>` / `<WORKTREE_ROOT>`, các bản chưa lọc vẫn đang nằm trực tiếp trên GitHub công khai (`origin/*`), trên nhánh `main` (`a5feb4a`), và trên các nhánh thành phần (`build/*`, `feat/macos-*`, `feat/ui-*`, `qa/*`).
   - Tệp thực thi biên dịch sẵn chưa ký số **`multi-zalo-desktop/dist/MultiZaloDesktop.exe` (`59.392` bytes)** đang được theo dõi trực tiếp trên `origin/main`, `main` và tất cả các nhánh (`EVID-SEC-03`).
   - Nhóm tệp dump thô của REA trong `multi-zalo-desktop/evidence/rea-*.json` (đặc biệt `rea-audit-release-exposure-app-asar.json` và `rea-pe-imports-electron-zalo-26.10.20.json` nặng `158 KB`) phơi bày danh sách tệp nội bộ `app.asar`, các thư viện gốc đóng gói kèm và **`5` đường dẫn máy trạm/CI nội bộ của kỹ sư VNG (`C:\Users\<REDACTED_USER>\...` thuộc máy build của VNG, `D:\GitlabRunner\...`, `F:\TASKZALO\...`)** đọc từ bản ghi debug `.pdb` của các tệp `.node` (`EVID-SEC-04`).
   - Tệp `.gitignore` gốc hiện mới chỉ chặn thư mục công cụ `rea/` mà **chưa có quy tắc chặn** các tệp nhạy cảm có nguy cơ phát sinh khi nghiên cứu/kiểm thử cục bộ như `*.asar`, `.rea/`, `Zalo*.exe`, `ZaloData/`, `Local State`, `Cookies*`, `*.ldb`, `*.sqlite*`, `*.zip` (bản sao lưu hồ sơ), `multi-zalo-desktop/dist/*.exe` (`EVID-SEC-06`).
5. **Dấu vết telemetry runtime trên máy thật, trích dẫn offset sâu & tài liệu điều phối nội bộ (`3` phát hiện mức `P2` — `EVID-SEC-05`, `EVID-SEC-07`, `EVID-SEC-08`):**
   - Các tệp bằng chứng chạy PoC trực tiếp (`zalo-live-poc-result.json`, `poc-verification-result.json`, `integration-test-result.json`) và báo cáo môi trường (`environment-readiness.md`, `windows-local-build-report.md`) lưu lại mã băm SHA-256 của tệp cấu hình/nhật ký Zalo chính đang chạy của tác giả, PID/HWND thực tế, dấu thời gian ghi cấu hình, trạng thái chi tiết của Windows Smart App Control (`VerifiedAndReputablePolicyState`) và cấu trúc thư mục công cụ AI cục bộ (`~/.agentkit`, `~/.gemini`, `~/.claude`).

---

## 2. Phương Pháp Kiểm Toán & Phân Tách Bề Mặt Phơi Nhiễm

### 2.1. Quy trình kiểm toán xác định (Deterministic Read-Only Inspection)

1. **Kiểm kê đa tầng bằng `git --no-optional-locks`:**
   - Liệt kê toàn bộ nhánh remote công khai (`origin/main` @ `8cf4703`, `origin/feat/ui-vi-en-quiet-workspace` @ `0f87302`) và `10` worktree cục bộ trong `C:\Users\<REDACTED_USER>\Downloads\zalo*` (`main` @ `a5feb4a`, `research/desktop-compatibility` @ `0e5d44c`, `build/cross-platform-distribution` @ `6c4150e`, `test/cross-platform-reliability` @ `5e802e0`, `integration/reliability-hardening` @ `bff0674` -> `cd62c52`, `feat/macos-multi-instance` @ `f2b8f9f`, `feat/ubuntu-multi-instance` @ `62dc5d1`, `feat/ui-vi-en-quiet-workspace` @ `0f87302`, `qa/windows-native-acceptance` @ `004ab7c`, và `audit/public-repository-security` @ `a5feb4a`).
   - Quét toàn bộ lịch sử đối tượng Git (`git log --all --full-history --raw --no-abbrev`), trích xuất và phân tích nội dung từng phiên bản blob của mọi tệp trong `evidence/`, `multi-zalo-desktop/`, `fixtures/`, `docs/`, `plans/`, `scripts/`, `tests/` (tổng cộng `169` cặp tệp–blob văn bản/JSON và `45` đối tượng nhị phân/hình ảnh).
2. **Phân tích cấu trúc nhị phân & PE/ASAR không xâm lấn:**
   - Giải mã bảng chuỗi ASCII/UTF-16LE và tiêu đề PE của `multi-zalo-desktop/dist/MultiZaloDesktop.exe`.
   - Kiểm tra khóa cấu trúc (`raw_result`, `normalized_result`, `resources[*].payload`, `icon_groups`) của toàn bộ đầu ra REA JSON để phân biệt rạch ròi giữa **metadata phân tích hợp lệ** và **dữ liệu tài nguyên/mã nguồn độc quyền không được tái phân phối**.
   - Kiểm tra cây thư mục và trạng thái Git độc lập của thư mục gốc `rea/` (`zalo/rea/.git`).
3. **Kiểm tra thị giác trực tiếp (Visual Inspection) 100% tệp ảnh:**
   - Mở và kiểm tra trực tiếp từng tệp trong số `20` ảnh chụp màn hình hiện hành tại `desktop-instance-manager/docs/design/screenshots/*.png`, `16` ảnh chụp màn hình đã bị xóa tại commit `cc74584^` (nhưng vẫn tồn tại công khai trong lịch sử nhánh `origin/feat/ui-vi-en-quiet-workspace`), và các tệp biểu tượng `icons/*.png`, `assets/128x128@2x.png`.

### 2.2. Hệ phân loại 4 mức chuẩn hóa

Mỗi tệp hoặc nhóm đường dẫn được gán chính xác một trong bốn nhãn phân loại theo yêu cầu của nhiệm vụ:

| Nhãn phân loại | Định nghĩa & Điều kiện áp dụng |
| :--- | :--- |
| **`PUBLIC_SAFE_AS_IS`** | Hoàn toàn do dự án tự viết hoặc sinh từ dữ liệu giả lập (synthetic/mock); không chứa PII, đường dẫn cá nhân, hostname, dữ liệu tài khoản Zalo thật, tài nguyên VNG hay mã nguồn trích xuất từ `app.asar`. An toàn để giữ công khai nguyên trạng. |
| **`PUBLIC_AFTER_SANITIZATION`** | Có giá trị kỹ thuật/minh bạch cho kho mã nguồn mở (tài liệu kiến trúc, báo cáo kiểm thử, kết quả PoC tổng hợp), không chứa bí mật xác thực hay nhị phân độc quyền, nhưng đang vướng đường dẫn người dùng cục bộ (`C:\Users\<REDACTED_USER>\...`), dấu vết máy trạm hoặc trích dẫn nội bộ cần biên tập/ẩn danh trước khi công bố. |
| **`PRIVATE_OR_LOCAL_STORAGE_ONLY`** | Công cụ phân tích bên thứ ba clone cục bộ (`rea/`), tệp `.exe` biên dịch sẵn không ký số (`MultiZaloDesktop.exe`), các bản dump cấu trúc nhị phân thô của REA (`rea-pe-*.json`, `rea-audit-*.json`, `rea-binary-session.json`), hoặc nhật ký điều phối tác nhân nội bộ. Chỉ được lưu ở môi trường cục bộ / kho lưu trữ nội bộ riêng, không đưa lên nhánh công khai. |
| **`CRITICAL_EXPOSURE_REMEDIATE`** | Phơi nhiễm nghiêm trọng về pháp lý/bản quyền hoặc bảo mật (như tái phân phối nguyên văn mã nguồn trích xuất trực tiếp từ `app.asar` của VNG trên kho công khai). Bắt buộc xử lý khắc phục trước khi phê duyệt `PUBLIC_READINESS_PASS`. |

---

## 3. Ma Trận Kiểm Kê & Phân Loại Chi Tiết Từng Hạng Mục (`Inventory & Classification Matrix`)

### 3.1. Nhóm 1 — `multi-zalo-desktop/evidence/` & Toàn Bộ Cây `multi-zalo-desktop/`

> [!WARNING]
> Toàn bộ `37` tệp thuộc `multi-zalo-desktop/` đã được đẩy lên **`origin/main` (`8cf4703`)** và **`origin/feat/ui-vi-en-quiet-workspace` (`0f87302`)** — tức là **đang hiển thị công khai trên GitHub ngay lúc này**, đồng thời vẫn còn nguyên trên nhánh `main` (`a5feb4a`).

| Đường dẫn tệp / Nhóm tệp | Trạng thái trên `origin/*` (Công khai) | Trạng thái trên `main` (`a5feb4a`) vs `zalo-integration` (`cd62c52`) | Đánh giá 5 chiều (PII/OS Path · Nội dung VNG · Mã `app.asar` · Binary/PE VNG · Auth/Session) | Phân loại chuẩn | Mã phát hiện |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `multi-zalo-desktop/docs/research/instance-behavior.md` | **ĐANG CÔNG KHAI** (`9152e500`, `8.604` B) | Chưa sửa trên cả `main` lẫn `cd62c52` | • PII/Auth/Binary: Không.<br>• **Mã `app.asar` Độc Quyền:** Dòng `51–75` trích nguyên văn toàn bộ `23` dòng mã nguồn JS của `app.asar/bootstrap.js` (`756` B); dòng `78–83` trích `5` dòng mã `main-dist/migration.js`. | **`CRITICAL_EXPOSURE_REMEDIATE`** | `EVID-SEC-01` (`P0`) |
| `multi-zalo-desktop/dist/MultiZaloDesktop.exe` *(kèm `.ps1`, `.cmd`)* | **ĐANG CÔNG KHAI** (`efc80891`, `59.392` B) | Vẫn theo dõi trên mọi nhánh/worktree | • Binary .NET biên dịch từ mã C# của dự án (SHA-256 `6da02a946159...`); kiểm tra bảng chuỗi xác nhận **0** mã/icon VNG, **0** đường dẫn `.pdb` cá nhân.<br>• Tuy nhiên là tệp thực thi `.exe` dựng sẵn **không ký số** lưu trực tiếp trong Git công khai. | **`PRIVATE_OR_LOCAL_STORAGE_ONLY`** | `EVID-SEC-03` (`P1`) |
| `multi-zalo-desktop/evidence/rea-audit-release-exposure-app-asar.json` | **ĐANG CÔNG KHAI** (`f75997fb`, `13.705` B) | `main`: lộ `17` đường dẫn `<REDACTED_USER>`.<br>`cd62c52` (`85be3d88`): đã ẩn `<USERPROFILE>`, nhưng **vẫn giữ `5` đường dẫn PDB máy trạm/CI nội bộ của VNG** | • Auth/Mã nguồn: Không trích nội dung mã JS (`raw_result: null`).<br>• **PII & Bề mặt nội bộ VNG:** Lộ `C:\Users\<REDACTED_USER>\...` (trên `origin/main` & `main`), liệt kê tên tệp nội bộ `app.asar` (`main-dist/*.js`, `libcrypto-1_1.dll`, `.node`) và `5` đường dẫn máy build của kỹ sư VNG (`C:\Users\<REDACTED_USER>\...` trên máy kỹ sư VNG, `D:\GitlabRunner\...`, `F:\TASKZALO\...`). | **`PRIVATE_OR_LOCAL_STORAGE_ONLY`** | `EVID-SEC-02`, `EVID-SEC-04` (`P1`) |
| `multi-zalo-desktop/evidence/rea-pe-imports-electron-zalo-26.10.20.json`<br>`rea-pe-imports-stub-zalo.json`<br>`rea-pe-imports-zalosetup-26.11.12.json` | **ĐANG CÔNG KHAI** (`70eb64f6` `158` KB; `67a04597` `12` KB; `c010707c` `20` KB) | `main`: mỗi tệp lộ `4` vị trí `C:\Users\<REDACTED_USER>\...`<br>`cd62c52`: đã thay bằng `<USERPROFILE>` / `<WORKTREE_ROOT>` | • Không chứa mã máy thực thi hay tài nguyên nhúng; chỉ chứa bảng tên DLL/hàm Win32 import, `imphash` và dải offset tệp của `Zalo.exe` / `ZaloSetup-26.11.12.exe`.<br>• Lộ đường dẫn cá nhân trên `origin/*` & `main`; dump chi tiết `158 KB` cấu trúc PE của binary thương mại là dư thừa trên repo công khai. | **`PRIVATE_OR_LOCAL_STORAGE_ONLY`** | `EVID-SEC-02`, `EVID-SEC-04` (`P1`) |
| `multi-zalo-desktop/evidence/rea-pe-resources-stub-zalo.json`<br>`rea-pe-resources-zalosetup-26.11.12.json` | **ĐANG CÔNG KHAI** (`552b9df6` `23` KB; `59b68fe5` `15` KB) | `main`: mỗi tệp lộ `4` vị trí `C:\Users\<REDACTED_USER>\...`<br>`cd62c52`: đã thay bằng `<USERPROFILE>` / `<WORKTREE_ROOT>` | • **Đã kiểm chứng sâu schema:** `raw_result: {}`, `resources[*].payload` chỉ lưu `{rva, location, sha256}`, `icon_groups` chỉ lưu kích thước khung ảnh (`16x16`..`128x128`) — **0 byte ảnh icon hay XML của VNG bị nhúng**.<br>• Lộ đường dẫn người dùng trên `origin/*` & `main`; là đầu ra RE cấp thấp chỉ cần lưu cục bộ. | **`PRIVATE_OR_LOCAL_STORAGE_ONLY`** | `EVID-SEC-02`, `EVID-SEC-04` (`P1`) |
| `multi-zalo-desktop/evidence/rea-binary-session.json` | **ĐANG CÔNG KHAI** (`83aa7868`, `18.569` B) | `main`: lộ `2` đường dẫn npm toàn cục `<REDACTED_USER>`.<br>`cd62c52` (`60eb9303`): đã ẩn `<USERPROFILE>` | • Chỉ lưu danh mục năng lực MCP của công cụ `rea-agents` v6.9.0 (`evidence_records: 0`, không mở binary nào).<br>• Không chứa dữ liệu VNG nhưng làm lộ đường dẫn cài đặt npm cục bộ trên `origin/*` & `main` và không cần thiết trong repo sản phẩm. | **`PRIVATE_OR_LOCAL_STORAGE_ONLY`** | `EVID-SEC-02`, `EVID-SEC-04` (`P1`) |
| `multi-zalo-desktop/evidence/rea-evidence-summary.json` | **ĐANG CÔNG KHAI** (`2b05f08d`, `4.703` B) | `main`: lộ `4` đường dẫn `<REDACTED_USER>`.<br>`cd62c52` (`031e879e`): đã ẩn `<USERPROFILE>` / `<WORKTREE_ROOT>` | • Chỉ chứa bảng tóm tắt mã băm SHA-256, kích thước tệp, phiên bản Electron `22.3.9` và tên pháp nhân chữ ký số `CN=VNG GROUP JSC` của bộ cài chính thức (hợp lệ cho kiểm chứng tương thích). Cần ẩn đường dẫn cá nhân. | **`PUBLIC_AFTER_SANITIZATION`** *(hoặc lưu trữ riêng cùng nguyên mẫu cũ)* | `EVID-SEC-02` (`P1`) |
| `multi-zalo-desktop/evidence/poc-verification-result.json`<br>`multi-zalo-desktop/evidence/integration-test-result.json` | **ĐANG CÔNG KHAI** (`996e0eac` `14` KB; `c8d6d326` `14` KB) | `main`: lộ `18` + `5` vị trí `C:\Users\<REDACTED_USER>\...`<br>`cd62c52`: đã thay bằng `<USERPROFILE>` | • Không chứa token/cookie/UID thật (`PoC Account A/B`, `IT Alpha/Beta/Gamma` chưa đăng nhập).<br>• Lộ đường dẫn cá nhân trên `origin/*` & `main`, kèm PID thực (`12980`), HWND, `LastConfigWriteUtc` của Zalo chính và mã số doanh nghiệp trong Subject chứng thư số VNG (`SERIALNUMBER=<REDACTED_CORP_SERIAL>`). | **`PUBLIC_AFTER_SANITIZATION`** | `EVID-SEC-02` (`P1`), `EVID-SEC-05` (`P2`) |
| `multi-zalo-desktop/docs/research/rea-capabilities.md`<br>`multi-zalo-desktop/docs/research/zalo-desktop-analysis.md` | **ĐANG CÔNG KHAI** (`c0b2a26d`, `195ca72f`) | `main`: vẫn lộ `C:\Users\<REDACTED_USER>\...` (`2` + `1` vị trí).<br>`cd62c52`: đã ẩn đường dẫn | • Không trích mã nguồn `app.asar`; mô tả kiến trúc cấp cao.<br>• Lộ đường dẫn người dùng cục bộ trên `origin/*` & `main`. | **`PUBLIC_AFTER_SANITIZATION`** | `EVID-SEC-02` (`P1`) |
| `multi-zalo-desktop/docs/research/existing-solutions.md` | **ĐANG CÔNG KHAI** (`e363b210`) | Giống nhau trên mọi nhánh | • Phân tích mã nguồn công khai của repo bên thứ ba `kemonra/ZaloMulti`; có trích dẫn chuỗi đường dẫn cứng `C:\Users\<REDACTED_USER>\...` (thuộc mã công khai của tác giả repo bên thứ ba đó). Nên thay bằng `<hardcoded_author_user>` để tránh nhiễu trình quét PII. | **`PUBLIC_AFTER_SANITIZATION`** | `EVID-SEC-07` (`P2`) |
| `multi-zalo-desktop/{README.md, docs/architecture.md, docs/compatibility.md, docs/research/profile-isolation.md, docs/security-review.md, docs/testing-report.md, src/**/*.cs, scripts/*.ps1, tests/IntegrationTests.ps1}` | **ĐANG CÔNG KHAI** (`origin/main`) | Giống nhau trên mọi nhánh | • Mã nguồn C#/PowerShell và tài liệu do dự án tự viết 100%, dùng biến môi trường động (`%APPDATA%`, `%LOCALAPPDATA%`), không chứa PII hay mã VNG.<br>• Lưu ý quản trị: toàn bộ `multi-zalo-desktop/` là nguyên mẫu giai đoạn 1 đã được thay thế bởi `desktop-instance-manager/`. | **`PUBLIC_SAFE_AS_IS`** *(về mặt nội dung; khuyến nghị dọn/lưu trữ nguyên mẫu cũ theo `EVID-SEC-08`)* | `EVID-SEC-08` (`P2`) |

---

### 3.2. Nhóm 2 — `desktop-instance-manager/evidence/` Qua Các Nhánh & Worktree

| Đường dẫn tệp | Hiện diện trên nhánh / worktree | Đánh giá 5 chiều chi tiết | Phân loại chuẩn | Mã phát hiện |
| :--- | :--- | :--- | :--- | :--- |
| `desktop-instance-manager/evidence/fixture-test-result.json` | `origin/main` (`cdea2797`), `main` & các nhánh (`372330d6`) | Kết quả chạy `8/8` bài kiểm thử trên fixture giả lập (`fixture-app.mjs`); chỉ chứa thư mục mẫu `duc-cong-viec_*`, cổng localhost ngẫu nhiên và PID tạm thời. Không có PII, không chạm Zalo thật. | **`PUBLIC_SAFE_AS_IS`** | — |
| `desktop-instance-manager/evidence/zalo-live-poc-result.json` | `origin/main` (`a13b985e`), `main` & các nhánh (`1333ed50`) | Đã dùng biến môi trường `%USERPROFILE%\AppData\Roaming` (không lộ tên user Windows); chạy ở trạng thái chưa đăng nhập (`L3_authenticated_multi_account: NOT_TESTED`). Tuy nhiên ghi lại mã băm SHA-256 thực tế của `3` tệp `ZaloData\{config.json, startup.log, log.log}` thuộc phiên Zalo chính của máy tác giả, số lượng tiến trình, PID, HWND và `SERIALNUMBER=<REDACTED_CORP_SERIAL>` của chứng thư VNG. | **`PUBLIC_AFTER_SANITIZATION`** | `EVID-SEC-05` (`P2`) |
| `desktop-instance-manager/evidence/artifact-manifest.json` | `main` (`a5feb4a`) & `8` worktree nội bộ (`7a170920`) | Chỉ chứa phiên bản `rustc`/`cargo`, mã băm `Cargo.lock`, kích thước & SHA-256 của `3` binary phát triển chưa ký số (`dim-cli.exe`, `dim-test-fixture.exe`, `dim-desktop.exe`) kèm nhãn cảnh báo rõ `UNSIGNED_LOCAL_OR_CI_TEST_BUILD`. Không có đường dẫn cá nhân hay dữ liệu VNG. | **`PUBLIC_SAFE_AS_IS`** | — |
| `desktop-instance-manager/evidence/contract-parity-result.json` | `main` (`a5feb4a`) & `8` worktree nội bộ (`a9e8f111`) | Kết quả kiểm thử đối xứng hợp đồng JSON giữa Rust CLI và PowerShell bridge trên fixture giả lập. Không có PII, hostname hay dữ liệu thật. | **`PUBLIC_SAFE_AS_IS`** | — |
| `desktop-instance-manager/evidence/dependency-audit.json` | `main` (`a5feb4a`) & `8` worktree nội bộ (`ec4e84e7`) | Thống kê số lượng gói trong `Cargo.lock` (`439` crate duy nhất, `policy_check_passed: true`). Hoàn toàn sạch. | **`PUBLIC_SAFE_AS_IS`** | — |
| `desktop-instance-manager/evidence/sbom-cyclonedx.json` | `main` (`a5feb4a`) & `8` worktree nội bộ (`b2d5da74`, `112` KB) | Danh mục thành phần CycloneDX 1.5 của các crate mã nguồn mở Rust (`serialNumber: urn:uuid:d7fa6bf2-...`, công cụ `generate-build-provenance.mjs`). Không chứa đường dẫn máy trạm, email cá nhân hay thành phần độc quyền. | **`PUBLIC_SAFE_AS_IS`** | — |
| `desktop-instance-manager/evidence/tauri-native-e2e-result.json` | `zalo-integration` (`cd62c52`, blob `efaea237`) | Kết quả kiểm thử 12 bước E2E trên `dim-desktop.exe` + `xf-fixture.mjs`; đường dẫn tạm đã được ẩn danh thành `<TEMP_E2E_ROOT>/dim-data`, trường `detected_zalo_user_id` là mã băm FNV-1a giả lập từ tên thư mục fixture (`<SYNTHETIC_FIXTURE_ID>`). Lưu ý nhỏ: bước 2 ghi `primary_zalo_running: true, primary_protected_pid_count: 11` (chỉ là số đếm tiến trình bảo vệ, không lộ PID/PII). | **`PUBLIC_SAFE_AS_IS`** | — |

---

### 3.3. Nhóm 3 — Thư Mục Gốc `rea/`, `desktop-instance-manager/fixtures/` & Các Thư Mục Build Bị `.gitignore`

| Đường dẫn / Thành phần | Trạng thái Git & Worktree | Kết quả kiểm tra chi tiết | Phân loại chuẩn | Mã phát hiện |
| :--- | :--- | :--- | :--- | :--- |
| Thư mục gốc `rea/` (`C:\Users\<REDACTED_USER>\Downloads\zalo\rea/`) | Bị loại trừ bởi `.gitignore` (`!! rea/`); **`0` commit từng theo dõi trong toàn bộ lịch sử Git** | Là bản clone Git độc lập, nguyên bản của công cụ mã nguồn mở `https://github.com/morluto/rea.git` (`rea-agents` v6.9.0, giấy phép `MIT`, HEAD `e9c72db`, `git status` bên trong `rea/` sạch tuyệt đối). Quét đệ quy toàn bộ `~7.000+` tệp bên trong `rea/` xác nhận **0 tệp `Zalo*`, 0 gói `app.asar`, 0 thư mục phiên `.rea` hay dữ liệu giải nén nào của VNG nằm trong `rea/`**. | **`PRIVATE_OR_LOCAL_STORAGE_ONLY`** *(giữ nguyên trong `.gitignore`, tuyệt đối không commit vào repo này)* | `EVID-SEC-06` (`P1` — cần mở rộng `.gitignore` cho các đầu ra `.rea/` ngoài thư mục này) |
| `desktop-instance-manager/fixtures/test-desktop-app/*` (`Cargo.toml`, `package.json`, `fixture-app.mjs`, `src/main.rs`, `src/bin/dim-cli.rs`) | `origin/main` & mọi nhánh/worktree | Ứng dụng giả lập phòng sạch 100% viết bằng Node.js chuẩn và Rust để mô phỏng cơ chế khóa `lockfile`, tín hiệu `focus`/`quit` và khôi phục sau sự cố (`FIXTURE_ZALO_USER_ID = '100001'`). Không sao chép bất kỳ mã nguồn, giao thức hay tài nguyên nào của VNG. | **`PUBLIC_SAFE_AS_IS`** | — |
| `desktop-instance-manager/fixtures/cross-platform/*` (`Cargo.toml`, `Cargo.lock`, `README.md`, `src/lib.rs`, `src/main.rs`, `xf-fixture.mjs`) | `test/cross-platform-reliability`, `integration/reliability-hardening`, `qa/windows-native-acceptance` | Bộ fixture đa nền tảng (Windows/macOS/Ubuntu) dùng thuật toán sinh dữ liệu giả định hướng hạt giống (`SplitMix64` seed `0x20261117`), mô phỏng `SingletonLock` và cây tiến trình con. Hoàn toàn tự viết, sạch 100%. | **`PUBLIC_SAFE_AS_IS`** | — |
| Các thư mục biên dịch bị `.gitignore` (`**/target/`, `apps/desktop/src-tauri/gen/`) trên `6` worktree | `!!` Ignored (không theo dõi bởi Git) | Chỉ chứa tạo phẩm biên dịch Rust/Tauri cục bộ của dự án (`dim-desktop.exe`, `dim-cli.exe`, `xf-test-runner.exe`, crate trung gian). Không chứa binary hay dữ liệu của Zalo/VNG. | **`PRIVATE_OR_LOCAL_STORAGE_ONLY`** *(đã được `.gitignore` bảo vệ đúng)* | — |

---

### 3.4. Nhóm 4 — Kiểm Tra Thị Giác Trực Tiếp Toàn Bộ Ảnh Chụp Màn Hình (`36` Tệp PNG) & Biểu Tượng Ứng Dụng (`Icons`)

Toàn bộ **`36` tệp ảnh chụp màn hình** (gồm `20` ảnh hiện hành và `16` ảnh đã xóa trong lịch sử Git) cùng các tệp biểu tượng đã được mở và kiểm tra trực quan từng bức ảnh:

| Nhóm tệp hình ảnh / Biểu tượng | Số lượng & Vị trí nhánh / Commit | Kết quả kiểm tra thị giác trực tiếp (`Visual Inspection`) | Phân loại chuẩn |
| :--- | :--- | :--- | :--- |
| **Ảnh chụp giao diện Account Dock hiện hành:**<br>`desktop-instance-manager/docs/design/screenshots/account-dock-*.png` *(đủ 20 tệp: `20accounts`, `3accounts`, `add-step1`, `add-step2`, `backups`, `details`, `diagnostics`, `empty`, `en-US-dark`, `en-US-light`, `error`, `help`, `menu`, `settings`, `vi-VN-dark`, `vi-VN-light` 4 kích thước, `zoom-200`)* | **`20` tệp PNG** — **ĐANG CÔNG KHAI** trên `origin/feat/ui-vi-en-quiet-workspace` (`0f87302`), và có trên `zalo-ui-worktree`, `zalo-integration`, `zalo-windows-acceptance` | • Được kết xuất tự động ở chế độ giả lập (`visual-cdp.mjs` mock mode, hiển thị rõ `"Skipped in mock mode"` trong ảnh `diagnostics`).<br>• Chỉ hiển thị giao diện trình quản lý `Multi-Zalo` với các hồ sơ mẫu giả định (`Đức — Công việc & Đối tác miền Nam`, `Cá nhân`, `Bán hàng`, `Hỗ trợ khách hàng`, `Kế toán nội bộ`, ...), tệp sao lưu mẫu giả định (`duc-work-*.zip`), và đường dẫn thư mục mẫu giả định (tài khoản mẫu `Demo` dưới `%USERPROFILE%\AppData\Local\MultiZaloDesktop`).<br>• **Xác nhận `0` cửa sổ chat Zalo thật, `0` tin nhắn, `0` mã QR, `0` số điện thoại thật, `0` UID thật, `0` tên người dùng Windows hay thanh tác vụ hệ điều hành của máy tác giả, `0` logo thương hiệu VNG.** | **`PUBLIC_SAFE_AS_IS`** |
| **Ảnh chụp giao diện phiên bản cũ đã xóa trong lịch sử Git:**<br>`desktop-instance-manager/docs/design/screenshots/{backups-vi-light-1160x780, diagnostics-vi-light-1160x780, dialog-create-vi-light, dialog-delete-vi-light, dialog-details-vi-light, overflow-menu-vi-light, profiles-en-US-dark-1160x780, profiles-en-US-light-1160x780, profiles-vi-VN-dark-1160x780, profiles-vi-VN-light-1160x780, profiles-vi-VN-light-420x540, profiles-vi-VN-light-768x650, profiles-vi-VN-light-900x650, settings-vi-light-1160x780, state-error-banner-vi-light, zoom-200-vi-light}.png` | **`16` tệp PNG** — Thêm vào ở `39ff5c7` / `2b1ce6e`, xóa ở `cc74584`, nhưng **vẫn truy cập được công khai trong lịch sử đối tượng Git của `origin/feat/ui-vi-en-quiet-workspace`** | • Trích xuất toàn bộ `16` blob từ `cc74584^` và kiểm tra trực tiếp bằng mắt từng ảnh:<br>• Ảnh `diagnostics-vi-light-1160x780.png` hiển thị đường dẫn thư mục mẫu giả định (tài khoản mẫu `Demo` dưới `%USERPROFILE%\AppData\Local\MultiZaloDesktop`, `Skipped in mock mode`).<br>• Ảnh `dialog-details-vi-light.png` hiển thị hồ sơ mẫu `prof-1` (`profile-duc-work`), `PID chính: 8820`, và trường `Zalo User ID` mang dãy số tuần tự giả lập rõ ràng (`<SYNTHETIC_DEMO_UID>`, không phải thuê bao thật).<br>• **Xác nhận toàn bộ `16` blob lịch sử đều là ảnh giả lập UI, không làm rò rỉ bất kỳ dữ liệu cá nhân hay tài khoản Zalo thật nào** (không cần viết lại lịch sử Git vì lý do ảnh chụp màn hình). | **`PUBLIC_SAFE_AS_IS`** |
| **Biểu tượng đóng gói ứng dụng (App Icons):**<br>`desktop-instance-manager/apps/desktop/src-tauri/icons/{32x32.png, 128x128.png, 128x128@2x.png, icon.png, icon.ico, icon.icns}` & `scripts/ci-cross-platform/assets/{128x128@2x.png, icon.icns}` | `main` & các nhánh/worktree đóng gói | • Kiểm tra trực quan & mã băm SHA-256: là hình vuông nền xanh dương đồng nhất có viền xanh đậm kích thước nhỏ (`115` B – `1.142` B) phục vụ trình đóng gói Tauri.<br>• **Không sao chép hay biến tấu biểu tượng bong bóng chữ "Zalo" của VNG.** | **`PUBLIC_SAFE_AS_IS`** |

---

### 3.5. Nhóm 5 — Tài Liệu Nghiên Cứu, Báo Cáo Kế Hoạch (`plans/reports/**`) & Tệp `Untracked` Trên Các Worktree

| Đường dẫn tệp / Nhóm tệp | Trạng thái nhánh / Worktree | Kết quả đánh giá rò rỉ đường dẫn, hostname, nội dung VNG & dữ liệu nội bộ | Phân loại chuẩn | Mã phát hiện |
| :--- | :--- | :--- | :--- | :--- |
| `desktop-instance-manager/docs/security/threat-model-and-audit.md`<br>`desktop-instance-manager/docs/testing/poc-and-test-report.md`<br>`desktop-instance-manager/docs/research/environment-readiness.md`<br>`desktop-instance-manager/plans/261117-zalo-instance-manager/plan.md` | **ĐANG CÔNG KHAI bản chưa lọc** trên `origin/main` (`8cf4703`) & `origin/feat/ui-vi-en-quiet-workspace` (`0f87302`).<br>*(Trên `main` `a5feb4a` đã lọc đường dẫn nhưng chưa push)* | • Trên `origin/*` đang công khai: chứa tổng cộng **`82` liên kết/đường dẫn tuyệt đối `file:///c:/Users/<REDACTED_USER>/...` và `C:\Users\<REDACTED_USER>\...`** (gồm cả cấu trúc `.agentkit`, `.gemini`, `.claude` trong `environment-readiness.md`).<br>• Ngay cả trên `main` (`a5feb4a`), `environment-readiness.md` vẫn giữ nguyên thông số WDAC/SAC chi tiết của máy tác giả và `threat-model-and-audit.md` giữ `SERIALNUMBER=<REDACTED_CORP_SERIAL>`. | **`PUBLIC_AFTER_SANITIZATION`** | `EVID-SEC-02` (`P1`), `EVID-SEC-05` (`P2`) |
| `desktop-instance-manager/docs/design/ui-i18n-implementation-report.md`<br>`desktop-instance-manager/docs/design/ui-independent-review-and-remediation.md` | **ĐANG CÔNG KHAI** trên `origin/feat/ui-vi-en-quiet-workspace` (`0f87302`) và có trên `qa/windows-native-acceptance` (`004ab7c`). *(Đã lọc trên `cd62c52`)* | • Dòng `4–5` làm lộ đường dẫn worktree cá nhân `C:/Users/<REDACTED_USER>/Downloads/zalo-ui-worktree/...`. Không chứa dữ liệu Zalo thật. | **`PUBLIC_AFTER_SANITIZATION`** | `EVID-SEC-02` (`P1`) |
| `desktop-instance-manager/docs/research/compatibility/macos-findings.md` | `research/desktop-compatibility` (`0e5d44c`), `integration/reliability-hardening` (`cd62c52`), `qa/windows-native-acceptance` (`004ab7c`) | • Không chứa PII.<br>• Dòng `86–92` trích `5` dòng mã JS tái dựng sát tên biến nén (`y`, `v`, `I`, `A`) từ `main.js:311828,314800` và dòng `71–75` mô tả chi tiết tên khóa nội bộ `SecureCipher` (`secure_ks_<hash>`, `"Zalo Safe Storage"`). Cần viết lại đoạn dòng `86–92` thành mô tả văn xuôi và khái quát hóa offset nội bộ. | **`PUBLIC_AFTER_SANITIZATION`** | `EVID-SEC-01` *(phần phụ)* / `EVID-SEC-07` (`P2`) |
| `desktop-instance-manager/docs/research/compatibility/{windows-findings.md, ubuntu-findings.md, evidence-quality-review.md, feature-parity-matrix.md, implementation-backlog.md, recommended-next-experiments.md, test-specifications.md}` & `plans/reports/compatibility/ak-xia-missing-features-comparison.md` | `research/desktop-compatibility`, `integration/reliability-hardening`, `qa/windows-native-acceptance` | • Không lộ đường dẫn cá nhân `<REDACTED_USER>`.<br>• `windows-findings.md` chỉ nêu lời gọi API Electron chuẩn `app.setAppUserModelId("com.vng.zalo")`, nhưng mô tả rất sâu kịch bản ghi đè con trỏ socket của `PipeZCallSend` (`PIPE_UNLIMITED_INSTANCES`) kèm offset byte của `main.js` và PID tiến trình chạy thực tế trên máy tác giả (`PID 12980, 15624`). Khuyến nghị lược bỏ offset nội bộ & PID máy trạm trước khi công bố. | **`PUBLIC_AFTER_SANITIZATION`** | `EVID-SEC-07` (`P2`) |
| `desktop-instance-manager/docs/research/repository-comparison.md` | **ĐANG CÔNG KHAI** trên `origin/main` (`0d6e7859`) & mọi nhánh | • Nghiên cứu đối sánh phòng sạch (`ak-xia --compare`); dòng `42` trích dẫn lỗi đường dẫn cứng `C:\Users\<REDACTED_USER>\...` từ repo công khai `kemonra/ZaloMulti`. Nên ẩn thành `<hardcoded_author_user>`. | **`PUBLIC_AFTER_SANITIZATION`** | `EVID-SEC-07` (`P2`) |
| `desktop-instance-manager/docs/packaging/{ci-execution-and-artifact-validation.md, release-readiness.md, windows.md}` & `plans/reports/build/{ci-baseline-and-github-audit.md, dependency-audit-report.json, integration-requests.md, windows-local-build-report.md}` | Nhánh `build/cross-platform-distribution` (`6c4150e`) & `qa/windows-native-acceptance` (`004ab7c`). *(Đã lọc đường dẫn user trên `cd62c52`)* | • Trên `build/*` & `qa/*`: lộ hàng chục vị trí `C:/Users/<REDACTED_USER>/Downloads/...`, `C:\Users\<REDACTED_USER>\.cargo\bin\cargo.exe`, và cấu hình `C:/Users/<REDACTED_USER>/.gitconfig` (`credential.helper`).<br>• Ghi chú xác minh: chuỗi `DIM-WIN-BUILD-*-E2` là mã báo cáo chung, **không phải** hostname thật; trường `From:` trong `ir-patches/0001..0007.patch` dùng địa chỉ giả định nội bộ đuôi `.local` (`<REDACTED_EMAIL>`), không phải email cá nhân. | **`PUBLIC_AFTER_SANITIZATION`** *(hoặc tách riêng `plans/reports/` sang lưu trữ nội bộ theo `EVID-SEC-08`)* | `EVID-SEC-02` (`P1`), `EVID-SEC-08` (`P2`) |
| `desktop-instance-manager/docs/platform/macos/native-runtime-hardening-report.md` & `plans/reports/macos/test-and-verification-report.md` | Nhánh `feat/macos-multi-instance` (`f2b8f9f`) & `qa/windows-native-acceptance` (`004ab7c`). *(Đã lọc trên `cd62c52`)* | • Lộ đường dẫn `C:/Users/<REDACTED_USER>/Downloads/zalo-macos-worktree` trên `feat/macos-multi-instance` và `qa/windows-native-acceptance`. | **`PUBLIC_AFTER_SANITIZATION`** | `EVID-SEC-02` (`P1`) |
| `desktop-instance-manager/plans/reports/reliability/{baseline-manifest.json, defect-reproductions.json, executive-summary.md, metrics-benchmark-windows.json, post-fix-integration-windows-evidence.json, windows-host-execution-evidence.json}` & `plans/reports/ubuntu/{fixture-suite-host-evidence.json, xia-zalo-for-linux-report.md}` | `test/cross-platform-reliability`, `feat/ubuntu-multi-instance`, `integration/reliability-hardening`, `qa/windows-native-acceptance` | • Đã xác minh các đường dẫn Unix xuất hiện trong JSON/test (`/Users/<REDACTED_USER>`, `/home/<REDACTED_USER>` với các tên mẫu giả lập cố định `tester`, `ubuntu`, `hostuser`, `alice`) **100% là chuỗi kiểm thử giả lập**, không phải người dùng thật.<br>• Tuy nhiên `metrics-benchmark-windows.json` & `baseline-manifest.json` là hồ sơ điều phối nội bộ của tác nhân AI (`AGENT_MISSION_C2`) kèm thông số phần cứng/OS build của máy phát triển. | **`PRIVATE_OR_LOCAL_STORAGE_ONLY`** *(cho `plans/reports/` nội bộ; giữ tóm tắt sạch trong `docs/testing/`)* | `EVID-SEC-08` (`P2`) |
| **Các tệp `Untracked` đang nằm trên đĩa cục bộ:**<br>• Trong `zalo`: `desktop-instance-manager/docs/release/{coordination-checkpoint.md, integration-readiness-matrix.md, release-blocker-register.md}`<br>• Trong `zalo-windows-acceptance`: `docs/qa/windows/{integration-requests.md, user-journeys.md}`, `tests/windows-acceptance/{README.md, installer-sandbox-check.ps1, lib/common.mjs}` | `Untracked` (`??`) — chưa commit | • Đã quét toàn văn cả `8` tệp: **`0` rò rỉ tên user, email, hostname, SID hay secret**; bộ script `windows-acceptance` còn chủ động kiểm tra chống gói nhầm binary VNG (`$forbiddenNames = @('zalo.exe', ..., 'app.asar')`) và lọc regex `zpw_sek`.<br>• Tuy nhiên nhóm `docs/release/*` và `docs/qa/windows/integration-requests.md` là biên bản điều phối nội bộ giữa các worktree AI, cần rà soát kỹ trước khi quyết định có đưa vào nhánh công khai hay chỉ giữ nội bộ. | `tests/windows-acceptance/*`: **`PUBLIC_SAFE_AS_IS`**<br>`docs/release/*` & `docs/qa/windows/integration-requests.md`: **`PRIVATE_OR_LOCAL_STORAGE_ONLY`** | `EVID-SEC-08` (`P2`) |

---

## 4. Đánh Giá Chuyên Sâu Ranh Giới Pháp Lý & Kỹ Thuật Của Đầu Ra REA (Không Đánh Đồng Mọi Đầu Ra REA Là Vi Phạm)

Theo yêu cầu kiểm toán khách quan từng hạng mục, cần phân biệt rõ **3 tầng đầu ra** từ quá trình sử dụng công cụ Reverse-Engineer-Anything (`REA`) trong dự án này:

1. **Tầng A — Sự kiện tương thích cấp hệ điều hành (Clean-Room Interoperability Facts) -> ĐƯỢC PHÉP CÔNG BỐ (`PUBLIC_SAFE_AS_IS` / `PUBLIC_AFTER_SANITIZATION`):**
   - Các quan sát hành vi bên ngoài phục vụ khả năng tương tác mà không sửa đổi binary gốc: Zalo Desktop sử dụng biến môi trường chuẩn của hệ điều hành (`APPDATA`/`LOCALAPPDATA`/`USERPROFILE` trên Windows, `CFFIXED_USER_HOME` trên macOS), khóa tệp đơn phiên `<userData>/lockfile` / `SingletonLock` của Electron/Chromium, tên tiến trình con, và việc bộ cài chính thức mang chữ ký số hợp lệ của `CN=VNG GROUP JSC`.
   - Các tài liệu như `docs/architecture/adr-001..004.md` và `docs/research/platform-feasibility.md` trình bày các sự kiện này bằng ngôn ngữ kỹ thuật độc lập của dự án và hoàn toàn phù hợp cho kho mã nguồn mở sau khi ẩn đường dẫn máy cá nhân.
2. **Tầng B — Dump cấu trúc PE / danh mục `app.asar` / đường dẫn máy build nội bộ của VNG (`multi-zalo-desktop/evidence/rea-*.json`) -> CHỈ LƯU CỤC BỘ (`PRIVATE_OR_LOCAL_STORAGE_ONLY`):**
   - Các tệp `rea-pe-imports-*.json` và `rea-pe-resources-*.json` không sao chép mã nguồn hay tài nguyên đồ họa của VNG, nhưng việc công bố hàng trăm kilobyte bảng ánh xạ địa chỉ/offset PE và đặc biệt là `rea-audit-release-exposure-app-asar.json` (liệt kê tên tệp nội bộ trong `app.asar` cùng `5` đường dẫn `.pdb` máy tính cá nhân/GitLab Runner nội bộ của kỹ sư VNG dưới nhãn `"audit_release_exposure"`) vượt quá mức cần thiết để chứng minh tính tương thích và dễ gây tranh chấp không đáng có về điều khoản dịch vụ hoặc tiết lộ thông tin hạ tầng nội bộ của bên thứ ba.
3. **Tầng C — Tái phân phối trực tiếp đoạn mã nguồn trích xuất từ `app.asar` (`multi-zalo-desktop/docs/research/instance-behavior.md` dòng `51–83`) -> VI PHẠM RANH GIỚI PHÂN PHỐI LẠI (`CRITICAL_EXPOSURE_REMEDIATE` — `P0`):**
   - Việc trích nguyên văn toàn bộ nội dung tệp `app.asar/bootstrap.js` (`23` dòng mã nguồn đóng của VNG) kèm đoạn mã `migration.js` lên kho GitHub công khai không còn là mô tả hành vi tương thích phòng sạch mà là tái phân phối trực tiếp một phần mã nguồn có bản quyền của phần mềm thương mại đóng. Hạng mục này bắt buộc phải gỡ bỏ khỏi nhánh công khai và thay bằng sơ đồ luồng/mô tả văn xuôi độc lập.

---

## 5. Danh Mục Phát Hiện Kiểm Toán Chi Tiết (`9-Field Security & Exposure Findings`)

> [!IMPORTANT]
> Mọi phát hiện dưới đây tuân thủ đầy đủ cấu trúc **9 trường bắt buộc** (`ID`, `Severity`, `Source`, `Evidence type`, `File/commit liên quan`, `Tác động`, `Khuyến nghị xử lý`, `Owner`, `Trạng thái`), đồng thời tuân thủ quy tắc **không ghi lại lịch sử Git gây phá hủy (no history rewrite)** đối với các nhánh đang được nhiều worktree phát triển song song và **không mở GitHub Issue công khai** cho các vấn đề bảo mật/phơi nhiễm.

---

#### `EVID-SEC-01` — Trích dẫn và phân phối công khai nguyên văn mã nguồn JavaScript độc quyền từ `app.asar` (`bootstrap.js` & `migration.js`) của VNG Zalo Desktop
- **ID:** `EVID-SEC-01`
- **Severity:** `P0`
- **Source:** `evidence-exposure-review.md` (`Section 3 — Proprietary Source & app.asar Redistribution Audit`)
- **Evidence type:** `Direct Repository Blob & Working-Tree Content Inspection`
- **File/commit liên quan:**
  - **Đang công khai (`origin/main` @ `8cf4703`, `origin/feat/ui-vi-en-quiet-workspace` @ `0f87302`) và hiện diện trên tất cả `10` nhánh/worktree cục bộ (kể cả `main` @ `a5feb4a` và `integration/reliability-hardening` @ `cd62c52`):** `multi-zalo-desktop/docs/research/instance-behavior.md` (blob `9152e500`, dòng `51–75`: toàn bộ hàm `bootstrap()` trích nguyên văn từ `app.asar/bootstrap.js`; dòng `78–83`: đoạn mã từ `main-dist/migration.js`).
  - **Nhánh cục bộ chưa đẩy (`research/desktop-compatibility` @ `bf08f10`/`0e5d44c`, `integration/reliability-hardening` @ `cd62c52`, `qa/windows-native-acceptance` @ `004ab7c`):** `desktop-instance-manager/docs/research/compatibility/macos-findings.md` (dòng `86–92`: `5` dòng mã JS tái dựng sát tên biến nén từ `app.asar/main-dist/main.js`).
- **Tác động:** Tái phân phối trực tiếp mã nguồn đóng trích xuất từ gói `app.asar` của ứng dụng thương mại Zalo Desktop trên kho GitHub công khai (`allow_forking: true`), tạo rủi ro pháp lý trực tiếp về bản quyền phần mềm (khiếu nại gỡ bỏ DMCA / vi phạm quyền tác giả đối với mã nguồn đóng của VNG) và làm suy yếu lập trường thiết kế phòng sạch (clean-room interoperability) mà dự án tuyên bố trong `ADR-001`.
- **Khuyến nghị xử lý:**
  1. Trong một commit tiến về phía trước (forward commit, không viết lại lịch sử) trên nhánh tích hợp và đẩy đồng bộ lên `origin/main`: xóa bỏ hoàn toàn hai khối mã JavaScript tại dòng `51–83` của `multi-zalo-desktop/docs/research/instance-behavior.md` (hoặc gỡ bỏ toàn bộ thư mục nguyên mẫu cũ `multi-zalo-desktop/` khỏi nhánh phát hành chính theo `EVID-SEC-08`) và thay thế bằng mô tả hành vi văn xuôi phòng sạch.
  2. Trong `desktop-instance-manager/docs/research/compatibility/macos-findings.md` (dòng `86–92`), thay đoạn mã `5` dòng bằng câu mô tả kỹ thuật tổng quát về cơ chế `unlink` UNIX domain socket khi gặp `EADDRINUSE` trên macOS mà không trích lại tên biến hay cấu trúc hàm nội bộ của `main.js`.
  3. Cập nhật/đồng bộ hoặc xóa nhánh công khai phụ `origin/feat/ui-vi-en-quiet-workspace` sau khi đã gộp vào `main` để không còn nhánh công khai nào hiển thị tệp chưa xử lý trên giao diện GitHub.
- **Owner:** `Central Integration Agent` & `Release Captain`
- **Trạng thái:** `OPEN` (`LIVE_ON_PUBLIC_ORIGIN_AND_ALL_BRANCHES`)

---

#### `EVID-SEC-02` — Rò rỉ diện rộng tên tài khoản người dùng Hệ điều hành cục bộ (`C:\Users\<REDACTED_USER>\...`) và đường dẫn cấu hình máy trạm trên các nhánh công khai `origin/*` và các nhánh thành phần chưa đồng bộ
- **ID:** `EVID-SEC-02`
- **Severity:** `P1`
- **Source:** `evidence-exposure-review.md` (`Section 3 — Personal Filesystem Path & Local Host Exposure Audit`)
- **Evidence type:** `Automated Multi-Ref Blob Regex Scan & Cross-Branch Diff Verification`
- **File/commit liên quan:**
  - **Đang công khai trên cả `origin/main` (`8cf4703`) & `origin/feat/ui-vi-en-quiet-workspace` (`0f87302`), và VẪN CHƯA LỌC trên nhánh `main` (`a5feb4a`) cùng `8/10` worktree cục bộ:**
    - `10` tệp JSON trong `multi-zalo-desktop/evidence/`: `poc-verification-result.json` (`18` vị trí), `rea-audit-release-exposure-app-asar.json` (`17` vị trí), `integration-test-result.json` (`5` vị trí), `rea-evidence-summary.json` (`4` vị trí), `rea-pe-imports-electron-zalo-26.10.20.json` (`4` vị trí), `rea-pe-imports-stub-zalo.json` (`4` vị trí), `rea-pe-imports-zalosetup-26.11.12.json` (`4` vị trí), `rea-pe-resources-stub-zalo.json` (`4` vị trí), `rea-pe-resources-zalosetup-26.11.12.json` (`4` vị trí), `rea-binary-session.json` (`2` vị trí).
    - `multi-zalo-desktop/docs/research/rea-capabilities.md` (dòng `4–5`) và `multi-zalo-desktop/docs/research/zalo-desktop-analysis.md` (dòng `11`).
  - **Đang công khai trên `origin/main` (`8cf4703`) & `origin/feat/ui-vi-en-quiet-workspace` (`0f87302`) *(đã lọc trên `main` `a5feb4a` nhưng chưa push lên `origin/main`)*:**
    - `desktop-instance-manager/docs/security/threat-model-and-audit.md` (`c65d1752`, `64` liên kết `file:///c:/Users/<REDACTED_USER>/...`).
    - `desktop-instance-manager/docs/testing/poc-and-test-report.md` (`cecea616`, `11` liên kết).
    - `desktop-instance-manager/docs/research/environment-readiness.md` (`ff235ef2`, `6` đường dẫn gồm `.agentkit`, `.gemini`, `.claude`).
    - `desktop-instance-manager/plans/261117-zalo-instance-manager/plan.md` (`43f1622c`, dòng `6`).
  - **Đang công khai trên `origin/feat/ui-vi-en-quiet-workspace` (`0f87302`) & nhánh cục bộ `qa/windows-native-acceptance` (`004ab7c`):**
    - `desktop-instance-manager/docs/design/ui-i18n-implementation-report.md` (dòng `4`) và `ui-independent-review-and-remediation.md` (dòng `5`).
  - **Nhánh cục bộ chưa đẩy (`build/cross-platform-distribution` @ `6c4150e`, `feat/macos-multi-instance` @ `f2b8f9f`, `qa/windows-native-acceptance` @ `004ab7c`):**
    - `desktop-instance-manager/docs/packaging/{ci-execution-and-artifact-validation.md, release-readiness.md, windows.md}` (bao gồm cả cấu hình `C:/Users/<REDACTED_USER>/.gitconfig` `credential.helper`), `plans/reports/build/{ci-baseline-and-github-audit.md, dependency-audit-report.json, integration-requests.md, windows-local-build-report.md}`, `docs/platform/macos/native-runtime-hardening-report.md`, `plans/reports/macos/test-and-verification-report.md`, `docs/reviews/ci-integration-and-execution.md`.
  - *(Ghi nhận đối chứng: Commit `bff0674` trên `integration/reliability-hardening` đã lọc các đường dẫn này sang `<USERPROFILE>` / `<WORKTREE_ROOT>`, nhưng bản vá đó chưa có trên `main`, `origin/*` hay `qa/windows-native-acceptance`).*
- **Tác động:** Phơi bày định danh tài khoản hệ điều hành cục bộ, bố cục thư mục phát triển, đường dẫn công cụ quản lý thông tin xác thực Git (`gh.exe auth git-credential` trong `.gitconfig`), và cấu trúc cài đặt công cụ trên máy cá nhân của nhà phát triển ra công chúng; đồng thời nếu đẩy nhầm bất kỳ nhánh thành phần nào (`build/*`, `feat/macos-*`, `qa/*`) trước khi hợp nhất qua nhánh đã lọc sẽ tái phát tán PII lên GitHub.
- **Khuyến nghị xử lý:**
  1. Tuyệt đối **không đẩy trực tiếp** các nhánh thành phần (`build/cross-platform-distribution`, `feat/macos-multi-instance`, `qa/windows-native-acceptance`) lên `origin` khi chưa áp dụng bước chuẩn hóa đường dẫn tương đương `bff0674`.
  2. Đưa toàn bộ bản chuẩn hóa đường dẫn từ `integration/reliability-hardening` về `main` (kết hợp xử lý `EVID-SEC-01`, `EVID-SEC-03`, `EVID-SEC-04`), bổ sung trình kiểm tra tự động trong CI từ chối mọi đường dẫn khớp mẫu `[A-Za-z]:[\/]Users[\/](?!Public|Default|Demo|<)` hoặc `file:///` trong `.md`/`.json`, rồi mới cập nhật `origin/main` và dọn nhánh phụ `origin/feat/ui-vi-en-quiet-workspace`.
- **Owner:** `Central Integration Agent` (hợp nhất bản lọc + thêm cổng CI check) & `Release Captain` (đồng bộ nhánh công khai)
- **Trạng thái:** `OPEN` (`LIVE_ON_PUBLIC_ORIGIN_PARTIALLY_SCRUBBED_IN_INTEGRATION`)

---

#### `EVID-SEC-03` — Tệp thực thi Windows biên dịch sẵn chưa ký số (`multi-zalo-desktop/dist/MultiZaloDesktop.exe`) đang được theo dõi và phân phối trực tiếp trong kho Git công khai
- **ID:** `EVID-SEC-03`
- **Severity:** `P1`
- **Source:** `evidence-exposure-review.md` (`Section 3 — Tracked Binary & Redistributable Artifact Audit`)
- **Evidence type:** `PE Binary Inspection, String Table Extraction & Git Tree Audit`
- **File/commit liên quan:** `multi-zalo-desktop/dist/MultiZaloDesktop.exe` (`59.392` bytes, SHA-256 `6da02a9461594808c2f3ce8adef2c8186059bcc6edf55b520af79ac4eec5473c`, blob `efc80891`), đi kèm `MultiZaloDesktop.cmd` và `MultiZaloDesktop.ps1` — đang công khai trên `origin/main` (`8cf4703`), `origin/feat/ui-vi-en-quiet-workspace` (`0f87302`), và hiện diện trên toàn bộ `10` nhánh/worktree cục bộ (bao gồm `main` và `cd62c52`).
- **Tác động:** Mặc dù kết quả giải mã chuỗi PE xác nhận đây là binary .NET biên dịch từ mã nguồn C# của nguyên mẫu giai đoạn 1 (không nhúng mã VNG hay đường dẫn `.pdb` cá nhân), việc lưu trữ và phát tán trực tiếp một tệp `.exe` không ký số Authenticode trong cây mã nguồn công khai vi phạm trực tiếp chính sách chuỗi cung ứng của chính dự án (`Zero Unsigned Distributable Binaries`), không thể kiểm chứng tính tái lập (reproducible build) bằng mắt khi duyệt PR, bị chặn bởi Windows Smart App Control / SmartScreen khi người dùng tải kho mã về máy, và tạo rủi ro nhầm lẫn với ứng dụng chính `desktop-instance-manager`.
- **Khuyến nghị xử lý:** Thực hiện `git rm multi-zalo-desktop/dist/MultiZaloDesktop.exe` trong commit tiến về phía trước trên nhánh tích hợp/`main` và bổ sung quy tắc chặn `multi-zalo-desktop/dist/*.exe` (cùng `*.exe`, `*.dll` ngoài whitelist nếu có) vào `.gitignore` gốc; người dùng có nhu cầu thử nghiệm nguyên mẫu C# cũ chỉ biên dịch cục bộ từ mã nguồn qua `scripts/build.ps1`.
- **Owner:** `Central Integration Agent`
- **Trạng thái:** `OPEN` (`LIVE_ON_PUBLIC_ORIGIN_AND_ALL_BRANCHES`)

---

#### `EVID-SEC-04` — Công khai các tệp dump phân tích ngược cấp thấp của REA (`multi-zalo-desktop/evidence/rea-*.json`) chứa danh mục cấu trúc nội bộ `app.asar` và đường dẫn máy trạm/CI nội bộ của kỹ sư VNG
- **ID:** `EVID-SEC-04`
- **Severity:** `P1`
- **Source:** `evidence-exposure-review.md` (`Section 3 — REA Output & Third-Party Internal Metadata Exposure Audit`)
- **Evidence type:** `JSON Schema & Payload Inspection`
- **File/commit liên quan:**
  - `multi-zalo-desktop/evidence/rea-audit-release-exposure-app-asar.json` (`f75997fb` trên `origin/main` & `main`; `85be3d88` trên `cd62c52` — vẫn giữ nguyên `5` đường dẫn `.pdb` nội bộ của VNG trong `checks[7].observation.samples`: `C:\Users\<REDACTED_USER>\Desktop\src-code\native\...` trên máy kỹ sư VNG, `D:\GitlabRunner\builds\...\zalo-pc-app\...`, `F:\TASKZALO\...` cùng danh sách các `.node` addon và `libcrypto-1_1.dll` đóng gói bên trong `app.asar`).
  - `multi-zalo-desktop/evidence/rea-pe-imports-electron-zalo-26.10.20.json` (`158.271` B), `rea-pe-imports-stub-zalo.json` (`12.356` B), `rea-pe-imports-zalosetup-26.11.12.json` (`20.421` B), `rea-pe-resources-stub-zalo.json` (`22.992` B), `rea-pe-resources-zalosetup-26.11.12.json` (`15.122` B), `rea-binary-session.json` (`18.569` B) — tất cả đang công khai trên `origin/main` (`8cf4703`) và `origin/feat/ui-vi-en-quiet-workspace` (`0f87302`).
- **Tác động:** Dù các tệp này không chứa mã nguồn JS hay tài nguyên đồ họa nhị phân của VNG, việc xuất bản công khai báo cáo quét phơi nhiễm phát hành (`audit_release_exposure`) chỉ điểm các đường dẫn máy tính cá nhân của nhân viên VNG, cấu trúc GitLab Runner nội bộ của VNG, các thư viện gốc cũ bên trong `app.asar` và hơn `240 KB` bảng offset/import PE thô của các binary thương mại vượt xa nhu cầu tài liệu hóa khả năng tương tác, làm gia tăng rủi ro phản ứng pháp lý/bảo mật từ phía nhà phát hành Zalo.
- **Khuyến nghị xử lý:** Chuyển nhóm `7` tệp dump REA cấp thấp (`rea-audit-release-exposure-app-asar.json`, `rea-pe-imports-*.json`, `rea-pe-resources-*.json`, `rea-binary-session.json`) sang chế độ **`PRIVATE_OR_LOCAL_STORAGE_ONLY`**: gỡ khỏi nhánh công khai bằng `git rm` trong forward commit (lưu trữ ngoại tuyến phục vụ đối chiếu kỹ thuật nội bộ nếu cần); trên kho công khai chỉ giữ lại tóm tắt kiểm chứng cấp cao đã ẩn danh đường dẫn (như bảng SHA-256/phiên bản/chữ ký số trong tài liệu kiến trúc).
- **Owner:** `Central Integration Agent` & `Release Captain`
- **Trạng thái:** `OPEN` (`LIVE_ON_PUBLIC_ORIGIN_AND_ALL_BRANCHES`)

---

#### `EVID-SEC-05` — Phơi nhiễm dấu vết máy trạm phát triển, trạng thái chính sách bảo mật Windows (SAC/WDAC), PID/HWND thực và mã băm tệp Zalo chính đang chạy trong các tệp bằng chứng PoC & báo cáo môi trường
- **ID:** `EVID-SEC-05`
- **Severity:** `P2`
- **Source:** `evidence-exposure-review.md` (`Section 3 — Live Host PoC & Workstation Fingerprint Audit`)
- **Evidence type:** `JSON Telemetry & Markdown Host Audit Inspection`
- **File/commit liên quan:**
  - `desktop-instance-manager/evidence/zalo-live-poc-result.json` (`a13b985e` trên `origin/main`; `1333ed50` trên `main` & `cd62c52`).
  - `multi-zalo-desktop/evidence/poc-verification-result.json` (`996e0eac` trên `origin/main`; `0eeac790` trên `cd62c52`) và `multi-zalo-desktop/evidence/integration-test-result.json` (`c8d6d326` / `611b5909`).
  - `desktop-instance-manager/docs/research/environment-readiness.md` (trên `origin/main` và `main` — ghi chi tiết trạng thái khóa Registry `HKLM:\SYSTEM\CurrentControlSet\Control\CI\Policy` `VerifiedAndReputablePolicyState`, bản dựng Windows Insider `10.0.26300`, và bố cục `105` kỹ năng AI cục bộ).
  - `desktop-instance-manager/plans/reports/build/windows-local-build-report.md` (dòng `30`: ghi `VerifiedAndReputablePolicyState = 0`, `SAC_PreviousState = 1`, phiên bản WebView2 cụ thể trên máy tác giả).
- **Tác động:** Không làm lộ token hay mật khẩu, nhưng tiết lộ cấu hình phòng thủ cụ thể của máy trạm nhà phát triển (ví dụ thời điểm Smart App Control đang tắt `= 0`, bản dựng OS chính xác, dấu thời gian ghi `config.json` cuối cùng của tài khoản Zalo chính trên máy tác giả và mã băm SHA-256 của các tệp cấu hình đó), tạo điều kiện thu thập thông tin trinh sát (host fingerprinting) nhắm vào máy tính của maintainer dự án mã nguồn mở.
- **Khuyến nghị xử lý:** Biên tập các tài liệu công khai (`environment-readiness.md`, `windows-local-build-report.md`) để chỉ giữ các yêu cầu tương thích hệ điều hành tổng quát (loại bỏ giá trị Registry trạng thái phòng thủ thực tế của máy cá nhân và danh sách thư mục công cụ AI cá nhân); trong `zalo-live-poc-result.json`, thay giá trị mã băm SHA-256 cụ thể của `primary_zalo_baseline.hashes_before/after` bằng cờ xác nhận boolean (`primary_files_sha256_identical: true`) và chuẩn hóa chuỗi `SERIALNUMBER` trong thông tin chứng thư số.
- **Owner:** `Central Integration Agent`
- **Trạng thái:** `OPEN` (`LIVE_ON_PUBLIC_ORIGIN_AND_LOCAL_BRANCHES`)

---

#### `EVID-SEC-06` — Khoảng trống nghiêm trọng trong `.gitignore` đối với gói `app.asar`, nhị phân Zalo, dữ liệu phiên đăng nhập (`ZaloData`/`Cookies`/`Local State`), tệp sao lưu `.zip` và thư mục phiên `.rea/`
- **ID:** `EVID-SEC-06`
- **Severity:** `P1`
- **Source:** `evidence-exposure-review.md` (`Section 3 — Gitignore & Accidental Ingestion Guardrail Audit`)
- **Evidence type:** `Gitignore Rule Evaluation & Worktree Artifact Pattern Analysis`
- **File/commit liên quan:** `.gitignore` ở gốc kho mã (`9db80c51` trên `origin/main`, `main` và toàn bộ `10` worktree — hiện chỉ có `20` dòng chặn `rea/`, `target/`, `src-tauri/gen/`, `node_modules/`, `.env*`, `*.log`). Tham chiếu thực tế trong `rea-evidence-summary.json` cho thấy từng có thao tác trỏ tới `<WORKTREE_ROOT>\Programs\ZaloSetup-26.11.12.exe`.
- **Tác động:** Khi kho mã đã mở công khai (đặc biệt trong bối cảnh GitHub Push Protection đang tắt theo `GHSET-SEC-02`), chỉ cần một nhà phát triển hoặc tác nhân AI chạy thử nghiệm cục bộ lưu nhầm gói `app.asar`, bộ cài `ZaloSetup*.exe`, thư mục giải nén `.rea/`, thư mục hồ sơ thử nghiệm `ZaloData/` (chứa `Cookies`, `Local State`, `*.ldb`, `*.sqlite`) hoặc tệp sao lưu hồ sơ `.zip` vào bên trong cây thư mục dự án rồi chạy `git add .`, toàn bộ nhị phân độc quyền của VNG hoặc phiên đăng nhập Zalo thật sẽ bị commit và đẩy thẳng lên GitHub công khai.
- **Khuyến nghị xử lý:** Bổ sung ngay khối quy tắc phòng vệ nhiều lớp vào `.gitignore` gốc trước lần push tiếp theo:
  ```gitignore
  # Reverse-engineering workspaces, caches & raw dumps
  .rea/
  **/rea-session*.json
  *.asar
  *.jsc
  *.pdb
  *.dmp
  *.idb
  *.i64

  # Third-party / proprietary desktop binaries & installers (Zero-VNG-Binary guard)
  Programs/
  Zalo*.exe
  ZaloCall.exe
  ZaloCap.exe
  ZaviMeet.exe
  zocr-host.exe
  multi-zalo-desktop/dist/*.exe
  *.msi
  *.dmg
  *.pkg
  *.AppImage
  *.deb

  # Live profile data, Chromium/Electron session stores & offline profile backups
  ZaloData/
  MultiZaloDesktop*/
  Partitions/
  Local State
  Cookies
  Cookies-journal
  *.ldb
  *.sqlite
  *.sqlite3
  *.sqlite-wal
  *.sqlite-shm
  *.db
  *.db-wal
  *.db-shm
  *.zip
  ```
- **Owner:** `Central Integration Agent` & `Build Agent`
- **Trạng thái:** `OPEN` (`CONFIRMED_GAP_ACROSS_ALL_BRANCHES`)

---

#### `EVID-SEC-07` — Trích dẫn chi tiết địa chỉ offset nội bộ của `main.js`, cơ chế khóa bọc `SecureCipher` và kịch bản chiếm quyền `PipeZCallSend` trong tài liệu nghiên cứu công khai, kèm đường dẫn cứng của tác giả bên thứ ba
- **ID:** `EVID-SEC-07`
- **Severity:** `P2`
- **Source:** `evidence-exposure-review.md` (`Section 3 — Research Documentation Boundary & IPC Exposure Review`)
- **Evidence type:** `Static Documentation Review`
- **File/commit liên quan:**
  - `desktop-instance-manager/docs/research/compatibility/{windows-findings.md, macos-findings.md, evidence-quality-review.md}` và `plans/reports/compatibility/ak-xia-missing-features-comparison.md` (trên `research/desktop-compatibility`, `integration/reliability-hardening`, `qa/windows-native-acceptance`).
  - `desktop-instance-manager/docs/research/repository-comparison.md` (dòng `42`, công khai trên `origin/main`) và `multi-zalo-desktop/docs/research/existing-solutions.md` (dòng `16, 42, 46`, công khai trên `origin/main`).
- **Tác động:**
  1. Các báo cáo tương thích ghi chi tiết dải byte offset trong `app.asar/main-dist/main.js` (`311xxx..317xxx`, `3561xxx..3563xxx`), tên hàm/biến rút gọn (`SecureCipher`, `secure_ks_<hash>`, biến callback socket `O`, `I`, `A`) và mô tả từng bước cách tiến trình thứ hai kết nối vào `\\.\pipe\PipeZCallSend` (`PIPE_UNLIMITED_INSTANCES`) làm ghi đè con trỏ socket điều khiển cuộc gọi của tiến trình thứ nhất. Ở góc độ tài liệu công khai cho người dùng/nhà phát triển mã nguồn mở, chỉ cần nêu rõ giới hạn kỹ thuật ("Zalo Desktop dùng cố định một cặp Named Pipe toàn cục không tách theo profile nên tính năng gọi thoại/video bị giới hạn ở phiên đầu tiên") thay vì công bố bản đồ offset giải dịch chi tiết của mã đóng.
  2. Chuỗi `C:\Users\<REDACTED_USER>\...` trích từ mã công khai của `kemonra/ZaloMulti` dễ gây cảnh báo giả (false positive) cho các công cụ quét PII tự động trên kho công khai.
- **Khuyến nghị xử lý:** Biên tập gọn các báo cáo trong `docs/research/compatibility/` để giữ nguyên các kết luận kỹ thuật chuẩn xác về giới hạn OS/IPC nhưng lược bỏ số byte offset cụ thể và tên biến rút gọn trong `main.js`; đồng thời thay tên người dùng cứng trích từ script bên thứ ba trong `repository-comparison.md` và `existing-solutions.md` thành `<hardcoded_author_username>`.
- **Owner:** `Compatibility Research Owner / Central Integration Agent`
- **Trạng thái:** `OPEN` (`UNPUSHED_COMPATIBILITY_DOCS_AND_PUBLIC_COMPARISON_DOC`)

---

#### `EVID-SEC-08` — Tồn lưu thư mục nguyên mẫu cũ `multi-zalo-desktop/` trên nhánh chính và các biên bản điều phối đa tác nhân AI nội bộ (`plans/reports/**`, `docs/release/*`, `docs/qa/*`)
- **ID:** `EVID-SEC-08`
- **Severity:** `P2`
- **Source:** `evidence-exposure-review.md` (`Section 3 — Repository Surface Minimization & Untracked Artifact Audit`)
- **Evidence type:** `Cross-Worktree Directory & Untracked File Audit`
- **File/commit liên quan:**
  - Cây thư mục nguyên mẫu cũ `multi-zalo-desktop/` (`37` tệp đang công khai trên `origin/main` và còn trên mọi nhánh).
  - Cây báo cáo điều phối tác nhân `desktop-instance-manager/plans/261117-zalo-instance-manager/plan.md` và `desktop-instance-manager/plans/reports/{build,compatibility,macos,reliability,ubuntu}/**` (`25` tệp trên `integration/reliability-hardening` và `qa/windows-native-acceptance`).
  - Các tệp `Untracked` chưa commit: `desktop-instance-manager/docs/release/{coordination-checkpoint.md, integration-readiness-matrix.md, release-blocker-register.md}` (trong worktree `zalo`) và `desktop-instance-manager/docs/qa/windows/{integration-requests.md, user-journeys.md}` (trong worktree `zalo-windows-acceptance`).
- **Tác động:**
  1. Thư mục `multi-zalo-desktop/` là bản thử nghiệm C#/WinForms/WPF giai đoạn đầu đã ngừng phát triển (được thay thế hoàn toàn bởi `desktop-instance-manager/` viết bằng Rust + Tauri), nhưng lại chính là nguồn phát sinh phần lớn rủi ro nghiêm trọng nhất của kho mã (`EVID-SEC-01`, `EVID-SEC-02`, `EVID-SEC-03`, `EVID-SEC-04`).
  2. Các tệp trong `plans/reports/**` và `docs/release/*` chứa nhật ký bàn giao nội bộ giữa các tác nhân AI (`AGENT_MISSION_*`, `IR-001..007` format-patch, trạng thái các worktree cục bộ), làm phình bề mặt thông tin nội bộ không cần thiết đối với người dùng cuối hoặc cộng tác viên bên ngoài.
- **Khuyến nghị xử lý:**
  1. Đề xuất `Release Captain` và `Central Integration Agent` cân nhắc gỡ bỏ toàn bộ thư mục nguyên mẫu cũ `multi-zalo-desktop/` khỏi nhánh phát hành chính `main` bằng một commit dọn dẹp tường minh (`chore: retire legacy multi-zalo-desktop prototype before public release`), qua đó triệt tiêu đồng thời bề mặt `EVID-SEC-01`, `EVID-SEC-03`, `EVID-SEC-04` trên HEAD mới.
  2. Không commit các biên bản điều phối nội bộ đang `untracked` (`docs/release/coordination-checkpoint.md`, `docs/qa/windows/integration-requests.md`) lên nhánh công khai; chỉ giữ lại các tài liệu hướng dẫn kiến trúc, bảo mật, đóng gói và kiểm thử hướng tới người dùng/nhà đóng góp trong `desktop-instance-manager/docs/`, và chuyển `plans/reports/` sang kho lưu trữ hồ sơ kỹ thuật nội bộ (hoặc đảm bảo đã đi qua cổng lọc tự động 100% trước khi hợp nhất).
- **Owner:** `Release Captain` & `Central Integration Agent`
- **Trạng thái:** `OPEN` (`ARCHITECTURAL_CLEANUP_RECOMMENDED`)

---

## 6. Bảng Tổng Hợp Đối Chiếu Nhanh Cho `@gate-synthesizer` & `@verifier`

| Mã ID | Mức độ (`Severity`) | Trạng thái trên GitHub Công Khai (`origin/*`) | Phân loại xử lý | Điều kiện đóng cổng `PUBLIC_READINESS_PASS` |
| :--- | :---: | :--- | :--- | :--- |
| **`EVID-SEC-01`** | **`P0`** | **Đang hiển thị công khai** (`instance-behavior.md` dòng `51–83`) | `CRITICAL_EXPOSURE_REMEDIATE` | **BẮT BUỘC KHẮC PHỤC TRƯỚC KHI PASS** (Xóa mã trích nguyên văn từ `app.asar/bootstrap.js` & `migration.js`, thay bằng văn xuôi phòng sạch hoặc gỡ thư mục `multi-zalo-desktop/` trên HEAD công khai + dọn nhánh phụ `origin/feat/ui-vi-en-quiet-workspace`). |
| **`EVID-SEC-02`** | **`P1`** | **Đang hiển thị công khai** (`10` tệp JSON + `8` tệp `.md`) | `PUBLIC_AFTER_SANITIZATION` | Hợp nhất bản lọc đường dẫn `<USERPROFILE>`/`<WORKTREE_ROOT>` về `main`, kiểm tra không nhánh con chưa lọc nào được push thẳng, bổ sung kiểm tra CI chống lộ đường dẫn cá nhân. |
| **`EVID-SEC-03`** | **`P1`** | **Đang hiển thị công khai** (`MultiZaloDesktop.exe` `59 KB`) | `PRIVATE_OR_LOCAL_STORAGE_ONLY` | Gỡ `multi-zalo-desktop/dist/MultiZaloDesktop.exe` khỏi HEAD nhánh chính và chặn trong `.gitignore`. |
| **`EVID-SEC-04`** | **`P1`** | **Đang hiển thị công khai** (`7` tệp dump `rea-*.json`) | `PRIVATE_OR_LOCAL_STORAGE_ONLY` | Gỡ các tệp dump PE/ASAR cấp thấp (đặc biệt `rea-audit-release-exposure-app-asar.json` chứa đường dẫn `.pdb` nội bộ VNG) khỏi kho công khai. |
| **`EVID-SEC-05`** | **`P2`** | **Đang hiển thị công khai** (`zalo-live-poc-result.json`, `environment-readiness.md`) | `PUBLIC_AFTER_SANITIZATION` | Ẩn danh dấu vết cấu hình SAC/Registry, thư mục AI cá nhân và mã băm tệp Zalo chính trên máy tác giả. |
| **`EVID-SEC-06`** | **`P1`** | **Đang thiếu quy tắc bảo vệ** (`.gitignore` chỉ có `20` dòng) | `PUBLIC_AFTER_SANITIZATION` *(gia cố `.gitignore`)* | Cập nhật `.gitignore` chặn `*.asar`, `.rea/`, `Zalo*.exe`, `ZaloData/`, `Local State`, `Cookies*`, `*.ldb`, `*.sqlite*`, `*.db`, `*.zip`. |
| **`EVID-SEC-07`** | **`P2`** | Một phần trên `origin/main`, phần chính trên nhánh `research/*` & `cd62c52` | `PUBLIC_AFTER_SANITIZATION` | Lược bỏ số offset byte và tên biến nén nội bộ của `main.js` trong tài liệu tương thích; ẩn tên user bên thứ ba. |
| **`EVID-SEC-08`** | **`P2`** | `multi-zalo-desktop/` đang công khai; `plans/reports/` & `docs/release/*` ở cục bộ | `PRIVATE_OR_LOCAL_STORAGE_ONLY` | Nghỉ hưu thư mục nguyên mẫu `multi-zalo-desktop/` trên nhánh công khai và không commit nhật ký điều phối AI nội bộ. |
