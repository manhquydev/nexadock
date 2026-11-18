# Sổ Đăng Ký Tổng Hợp Các Phát Hiện Bảo Mật, Quyền Riêng Tư & Tuân Thủ (`Master Security Findings Register`)

- **Nhiệm vụ:** AGENT MISSION F — Kiểm toán bảo mật, quyền riêng tư & quản trị mã nguồn mở khi chuyển kho mã sang công khai (`https://github.com/manhquydev/zalo`)
- **Nhánh kiểm toán:** `audit/public-repository-security` (tạo từ `main` @ `a5feb4a520798064c1508c26fe4c4ac17a3a03cf` tại worktree `C:\Users\<REDACTED_USER>\Downloads\zalo-public-security-audit`)
- **Tác nhân tổng hợp:** `@gate-synthesizer` (tổng hợp và khử trùng lặp từ `@git-scanner`, `@evidence-auditor`, `@ci-gh-auditor`, `@license-auditor` cùng kiểm toán AppSec/Tauri/Web-UI độc lập)
- **Ngày tổng hợp (UTC):** `2026-11-18`
- **Chế độ kiểm toán:** `100% READ-ONLY` đối với toàn bộ mã nguồn sản phẩm và `9` worktree anh em (`git --no-optional-locks`); tuân thủ tuyệt đối quy tắc **Zero Secret/PII/Proprietary Code Leak**.

---

## 1. Tóm Tắt Điều Hành & Xác Nhận Đường Cơ Sở Sạch (`Baseline Assurance`)

Kho mã `https://github.com/manhquydev/zalo` đã chuyển từ trạng thái **Private** sang **Public** (`visibility: "public"`, `allow_forking: true`, lúc `2026-11-18T01:47:24Z`) trong bối cảnh `2` nhánh đã có mặt trên GitHub công khai (`origin/main` @ `8cf4703` và `origin/feat/ui-vi-en-quiet-workspace` @ `0f87302`) và `10` worktree đang tồn tại cục bộ (bao gồm nhánh tích hợp trung tâm `integration/reliability-hardening` @ `cd62c52`).

### 1.1. Các bề mặt đã kiểm chứng SẠCH (`0` rò rỉ thông tin xác thực / phiên Zalo / nhị phân VNG)
1. **Không lộ khóa bí mật hay thông tin xác thực (`0` Credential P0s):** Quét vét cạn `100%` kho đối tượng Git (`66` commits, `635` trees, `660` blobs = `16.736.930` bytes) và `10` worktree xác nhận **`0`** GitHub PAT, API key đám mây, khóa ký Tauri/Authenticode/Apple/GPG, chứng chỉ `.pfx`/`.p12`/`.pem`, hoặc tệp `.env`. **Không phát sinh yêu cầu hủy/xoay vòng khóa khẩn cấp (`revoke/rotate`).**
2. **Không lộ dữ liệu tài khoản hay phiên đăng nhập Zalo thật (`0` Session P0s):** **`0`** cookie/token phiên (`zpw_sek`, `zpsid`, `zpw_enk`), `imei`, cơ sở dữ liệu `SQLite`/`LevelDB`, khóa DPAPI `Local State`, số điện thoại cá nhân hay UID thật (toàn bộ `17` vị trí khớp `zpw_*` là mẫu lọc bảo vệ trong `sanitize.rs` và bài kiểm thử).
3. **Không lộ email cá nhân, SID hay hostname thật:** `100%` (`66/66`) commit chỉ dùng địa chỉ ẩn danh chuẩn của GitHub (`<REDACTED_EMAIL>`); `0` Windows Domain SID (`S-1-5-21-...`), `0` hostname thật (`<REDACTED_HOST>`).
4. **Không phân phối lại nhị phân VNG & ảnh chụp màn hình hoàn toàn sạch:** **`0`** tệp thực thi VNG (`Zalo.exe`, `ZaloSetup*.exe`, `ZaloCall.exe`, `Zalo.app`, `app.asar`) và **`0`** crate nhiễm copyleft mạnh (`GPL`/`AGPL`) trên `439` gói Rust. Kiểm tra trực quan bằng mắt `36/36` ảnh PNG (`20` ảnh hiện hành + `16` ảnh đã xóa trong lịch sử) xác nhận `100%` là giao diện giả lập (`PUBLIC_SAFE_AS_IS`).

### 1.2. Thống kê các phát hiện sau khi khử trùng lặp (`25` phát hiện chuẩn từ `42` mục chuyên ngành)

| Mức độ (`Severity`) | Số lượng phát hiện | Ý nghĩa điều hành đối với Cổng Sẵn Sàng Công Khai (`PUBLIC_READINESS_PASS`) |
| :---: | :---: | :--- |
| **`P0`** | **`5`** (`PUB-SEC-01` .. `PUB-SEC-05`) | **CHẶN CỨNG (`BLOCKED`):** Phơi nhiễm mã đóng `app.asar` trên nhánh công khai, thiếu hoàn toàn tệp `LICENSE`, tắt Branch Protection & Secret Scanning/Push Protection trên GitHub công khai, và nhánh công khai `origin/main` đang chạy mã Backend trước đợt gia cố bảo mật (`a5feb4a`). |
| **`P1`** | **`13`** (`PUB-SEC-06` .. `PUB-SEC-18`) | **YÊU CẦU SỬA ĐỔI (`CHANGES_REQUESTED`):** Bắt buộc xử lý trước khi hợp nhất/push lên `origin/main` hoặc phát hành gói thử nghiệm công khai (khóa cờ `DIM_NODE_FIXTURE_SCRIPT`, làm sạch đường dẫn OS & gộp squash, gỡ `MultiZaloDesktop.exe` & dump `rea-*.json`, gia cố `.gitignore`, ghim SHA Actions, bật CVD/Dependabot, xử lý nhãn hiệu & `THIRD_PARTY_NOTICES.md`). |
| **`P2`** | **`7`** (`PUB-SEC-19` .. `PUB-SEC-25`) | **GIA CỐ CHIỀU SÂU (`DEFENSE-IN-DEPTH`):** Cải thiện Tauri capabilities/CSP, ẩn đường dẫn bên thứ ba & dấu vết máy trạm, biên tập offset `main.js`, dọn nguyên mẫu cũ/tệp untracked, thêm `--locked` cho script C2 & chuẩn bị Environment ký số Tier 4. |

---

## 2. Ma Trận Truy Vết & Khử Trùng Lặp (`42` Mã Chuyên Ngành -> `25` Mã Tổng Hợp)

| Mã Tổng Hợp | Mức Độ | Mã Báo Cáo Chuyên Ngành Được Hợp Nhất | Nhóm Chủ Đề Chính |
| :--- | :---: | :--- | :--- |
| **`PUB-SEC-01`** | `P0` | `EVID-SEC-01` | Trích nguyên văn mã nguồn JS độc quyền từ `app.asar` (`bootstrap.js`, `migration.js`) |
| **`PUB-SEC-02`** | `P0` | `LIC-SEC-001` | Kho `PUBLIC` thiếu hoàn toàn tệp `LICENSE` dù `Cargo.toml` ghi `MIT OR Apache-2.0` |
| **`PUB-SEC-03`** | `P0` | `GHSET-SEC-01` | Nhánh `main` công khai tắt hoàn toàn Branch Protection & Repository Rulesets |
| **`PUB-SEC-04`** | `P0` | `GHSET-SEC-02` | Secret Scanning & Secret Scanning Push Protection đang `DISABLED` trên repo công khai |
| **`PUB-SEC-05`** | `P0` | `APP-SEC-01` | Nhánh công khai `origin/main` & `origin/feat/ui-*` đang phơi bày Backend tiền-`a5feb4a` chưa xác thực/chưa `WinVerifyTrust` |
| **`PUB-SEC-06`** | `P1` | `APP-SEC-02` | Biến môi trường `DIM_NODE_FIXTURE_SCRIPT` không khóa `cfg` trong bản Release `commands.rs` |
| **`PUB-SEC-07`** | `P1` | `GIT-SEC-001`, `EVID-SEC-02` *(phần `origin/*`)* | Lộ đường dẫn người dùng `C:\Users\<REDACTED_USER>\...` trên `2` nhánh đã công khai `origin/*` |
| **`PUB-SEC-08`** | `P1` | `GIT-SEC-002`, `EVID-SEC-02` *(phần nhánh cục bộ)* | `54` commit cục bộ (`426` blobs) & `8` nhánh chưa push chứa đường dẫn OS, `.gitconfig`, `.cargo` |
| **`PUB-SEC-09`** | `P1` | `GIT-SEC-003`, `EVID-SEC-03`, `LIC-SEC-003` *(phần `.exe`)* | Tệp thực thi không ký số `multi-zalo-desktop/dist/MultiZaloDesktop.exe` được track trên mọi nhánh |
| **`PUB-SEC-10`** | `P1` | `EVID-SEC-04`, `LIC-SEC-003` *(phần `rea-*.json`)* | Công khai `7` tệp dump `rea-*.json` chứa danh mục `app.asar` & `5` đường dẫn `.pdb` nội bộ VNG |
| **`PUB-SEC-11`** | `P1` | `GIT-SEC-006`, `EVID-SEC-06` | Khoảng trống `.gitignore` đối với khóa ký, `*.asar`, `ZaloData/`, `Cookies`, `*.sqlite*`, `*.exe`, `*.zip` |
| **`PUB-SEC-12`** | `P1` | `GIT-SEC-004` | `16` ảnh PNG đã xóa (`--diff-filter=D`) & `ci.yml` cũ còn lưu trong lịch sử nhánh công khai |
| **`PUB-SEC-13`** | `P1` | `GHA-SEC-01`, `GHA-SEC-02` | `ci.yml` trên `origin/*` đặt sai thư mục (`0` CI chạy); toàn bộ Actions dùng tag khả biến xung đột `validate-workflow.mjs` |
| **`PUB-SEC-14`** | `P1` | `GHA-SEC-03` | `ci.yml` thiếu `permissions:`, `persist-credentials: false`, `timeout-minutes`, `concurrency:` & lưu cache `~/.cargo/bin/` |
| **`PUB-SEC-15`** | `P1` | `GHA-SEC-04`, `GHA-SEC-05` | Nội suy trực tiếp `${{ inputs.bundles }}` / `${{ github.* }}` vào `run:` & cài `@tauri-apps/cli` bằng `npm install -g` không lockfile |
| **`PUB-SEC-16`** | `P1` | `GHSET-SEC-03`, `LIC-SEC-006` | Tắt Private Vulnerability Reporting & thiếu cả `SECURITY.md` lẫn `CONTRIBUTING.md` |
| **`PUB-SEC-17`** | `P1` | `GHSET-SEC-04`, `GHSET-SEC-05` | Tắt Dependabot Alerts/Updates (thiếu `dependabot.yml`) & chính sách Actions/Fork PR chưa siết chặt |
| **`PUB-SEC-18`** | `P1` | `LIC-SEC-002`, `LIC-SEC-004`, `LIC-SEC-005` | Rủi ro nhãn hiệu `"Zalo"` không tuyên bố phi liên kết; thiếu `THIRD_PARTY_NOTICES.md` (`MPL-2.0`/`LGPL`); hổng `deny.toml` |
| **`PUB-SEC-19`** | `P2` | `APP-SEC-03`, `APP-SEC-04` | Bật `withGlobalTauri: true` không có `capabilities/default.json`; CSP thiếu Trusted Types; `get_status` trả `data_dir` thô |
| **`PUB-SEC-20`** | `P2` | `GIT-SEC-005`, `EVID-SEC-07` *(phần đường dẫn bên thứ ba)* | Sót `3` tệp tại `HEAD` (`cd62c52`) chứa đường dẫn Windows của máy build upstream / tác giả bên thứ ba |
| **`PUB-SEC-21`** | `P2` | `EVID-SEC-05`, `GIT-SEC-008` | Lộ dấu vết cấu hình SAC/WDAC máy trạm, mã băm tệp Zalo chính, `X-Oauth-Scopes`, commit chưa ký GPG & blob mồ côi |
| **`PUB-SEC-22`** | `P2` | `EVID-SEC-07` *(phần offset `main.js`)* | Tài liệu tương thích ghi chi tiết số byte offset/tên biến rút gọn của `main.js` & cơ chế ghi đè `PipeZCallSend` |
| **`PUB-SEC-23`** | `P2` | `EVID-SEC-08`, `GIT-SEC-007`, `LIC-SEC-008` | Tồn lưu nguyên mẫu `multi-zalo-desktop/`, `plans/reports/**`, `21` tệp `untracked` và kho ngoài `rea/` ngay trong thư mục gốc |
| **`PUB-SEC-24`** | `P2` | `GHA-SEC-06`, `GHA-SEC-07`, `GHSET-SEC-06` | Script C2 thiếu `--locked` ở `6` lệnh build; chưa tách `release.yml` có Environment bảo vệ; CodeQL chưa bật |
| **`PUB-SEC-25`** | `P2` | `LIC-SEC-007`, `LIC-SEC-009` | Thiếu metadata giấy phép trong `tauri.conf.json`/`package.json` & icon vuông xanh `#0057D8` + chữ `"Z"` gần nhận diện Zalo |

---

## 3. Chi Tiết Toàn Bộ Phát Hiện Bảo Mật Chuẩn 9 Trường (`PUB-SEC-01` .. `PUB-SEC-25`)

### Nhóm Mức Độ `P0` — Các Rào Cản Nghiêm Trọng Chặn Trực Tiếp `PUBLIC_READINESS_PASS`

#### `PUB-SEC-01` — Phân phối công khai nguyên văn đoạn mã nguồn JavaScript trích xuất từ `app.asar` (`bootstrap.js` & `migration.js`) của VNG Zalo Desktop
- **ID:** `PUB-SEC-01`
- **Severity:** `P0`
- **Source:** `evidence-exposure-review.md` (`EVID-SEC-01`)
- **Evidence type:** Kiểm tra trực tiếp nội dung blob Git trên nhánh công khai và cây làm việc cục bộ (`Direct Repository Blob & Working-Tree Content Inspection`)
- **File/commit liên quan:** `multi-zalo-desktop/docs/research/instance-behavior.md` (blob `9152e500`, dòng `51–83` chứa nguyên văn `23` dòng hàm `bootstrap()` từ `app.asar/bootstrap.js` và `5` dòng từ `main-dist/migration.js` — đang công khai trên `origin/main` @ `8cf4703`, `origin/feat/ui-vi-en-quiet-workspace` @ `0f87302`, và chưa sửa trên `main` @ `a5feb4a` lẫn `integration/reliability-hardening` @ `cd62c52`); `desktop-instance-manager/docs/research/compatibility/macos-findings.md` (dòng `86–92` trên `0e5d44c`, `004ab7c`, `cd62c52` chứa `5` dòng mã JS tái dựng sát tên biến nén từ `main.js`)
- **Tác động:** Tái phân phối trực tiếp một phần mã nguồn có bản quyền trích xuất từ gói đóng `app.asar` của ứng dụng thương mại Zalo Desktop trên kho công khai cho phép fork (`allow_forking: true`), dẫn tới rủi ro pháp lý trực tiếp về bản quyền/khiếu nại DMCA và phá vỡ nguyên tắc thiết kế tương thích phòng sạch (`clean-room interoperability`) trong `ADR-001`.
- **Khuyến nghị xử lý:** Tạo commit tiến về phía trước (`forward commit`, tuyệt đối không viết lại lịch sử) trên nhánh tích hợp rồi gộp lên `origin/main`: xóa toàn bộ khối mã tại dòng `51–83` của `multi-zalo-desktop/docs/research/instance-behavior.md` và thay bằng văn xuôi mô tả cơ chế cấp cao (hoặc gỡ bỏ toàn bộ thư mục nguyên mẫu cũ `multi-zalo-desktop/` khỏi `HEAD`); viết lại dòng `86–92` của `macos-findings.md` thành câu mô tả kỹ thuật không kèm mã nguồn; đồng thời xóa nhánh phụ `origin/feat/ui-vi-en-quiet-workspace` sau khi đã gộp giao diện vào `main`.
- **Owner:** `Central Integration Agent` & `Release Captain`
- **Trạng thái:** `OPEN` (`LIVE_ON_PUBLIC_ORIGIN_AND_ALL_BRANCHES`)

---

#### `PUB-SEC-02` — Kho mã đã chuyển `PUBLIC` nhưng thiếu hoàn toàn tệp văn bản giấy phép (`LICENSE` / `LICENSE-MIT` / `LICENSE-APACHE`), mâu thuẫn với tuyên bố `MIT OR Apache-2.0` và trộn lẫn các thành phần `LICENSE_REVIEW_REQUIRED`
- **ID:** `PUB-SEC-02`
- **Severity:** `P0`
- **Source:** `licensing-and-compliance.md` (`LIC-SEC-001`)
- **Evidence type:** Kiểm tra cây tệp Git (`git ls-tree`) đối chiếu với khai báo SPDX trong `Cargo.toml` và tài liệu kỹ thuật
- **File/commit liên quan:** Thư mục gốc `/` và `multi-zalo-desktop/` (`0` tệp `LICENSE*` trên mọi nhánh); `desktop-instance-manager/Cargo.toml` và `desktop-instance-manager/apps/desktop/src-tauri/Cargo.toml` (`license = "MIT OR Apache-2.0"` tại `8cf4703`, `0f87302`, `a5feb4a`, `cd62c52`); `desktop-instance-manager/docs/platform/ubuntu/security-and-license.md` (`62dc5d1`)
- **Tác động:** Theo Điều khoản Dịch vụ của GitHub và luật bản quyền, kho công khai không có tệp `LICENSE` mặc định rơi vào chế độ bảo lưu mọi quyền ("All Rights Reserved"), mâu thuẫn trực tiếp với chuỗi `"MIT OR Apache-2.0"` trong `Cargo.toml` và không thỏa điều kiện đính kèm văn bản bản quyền của MIT cũng như Apache-2.0 Mục 4(a). Mặt khác, nếu gắn một tệp `LICENSE` phủ toàn bộ repo khi chưa loại trừ `multi-zalo-desktop/dist/MultiZaloDesktop.exe` và `**/evidence/rea-*.json` (`LICENSE_REVIEW_REQUIRED`) sẽ dẫn tới cấp phép sai phạm vi pháp lý.
- **Khuyến nghị xử lý:** (1) Tách/gỡ các tệp `LICENSE_REVIEW_REQUIRED` (`MultiZaloDesktop.exe`, các tệp dump `rea-*.json`) và ghi rõ phạm vi loại trừ đối với dữ liệu bằng chứng còn giữ lại; (2) Chủ sở hữu kho chính thức phê duyệt và commit văn bản giấy phép cho phần mã nguồn tự viết 100% (khuyến nghị bộ đôi `LICENSE-MIT` + `LICENSE-APACHE` để khớp với mọi `Cargo.toml` và hưởng điều khoản bảo vệ sáng chế/nhãn hiệu của Apache-2.0 Mục 2, 3, 6).
- **Owner:** `Repository Owner` & `Release Captain`
- **Trạng thái:** `OPEN` (`BLOCKED_AWAITING_OWNER_LICENSE_DECISION`)

---

#### `PUB-SEC-03` — Nhánh mặc định `main` trên kho mã công khai hoàn toàn tắt Branch Protection và không có Repository Ruleset bảo vệ
- **ID:** `PUB-SEC-03`
- **Severity:** `P0`
- **Source:** `github-actions-threat-review.md` (`GHSET-SEC-01`)
- **Evidence type:** Kiểm chứng trực tiếp qua GitHub REST API chỉ đọc (`GET repos/manhquydev/zalo/branches/main`, `.../protection`, `.../rulesets`, `.../rules/branches/main`)
- **File/commit liên quan:** Cấu hình trực tiếp trên `https://github.com/manhquydev/zalo` (`"protected": false` trên nhánh `main` tại `8cf47035f0f06bd97cc99aff80d81fb10b103af8`; `HTTP 404 Branch not protected`; `rulesets: []`)
- **Tác động:** Cho phép push thẳng mã chưa qua kiểm duyệt hoặc chưa chạy CI lên nhánh mặc định `main` đang công khai, không chặn `git push --force` hoặc xóa nhánh mặc định/tag phát hành nếu một phiên làm việc cục bộ hoặc token gặp sự cố.
- **Khuyến nghị xử lý:** Thiết lập ngay **Repository Ruleset** cho nhánh mặc định `main` và mẫu tag `refs/tags/v*` trong `Settings -> Rules -> Rulesets`: bật **Restrict deletions**, **Block force pushes**, **Require a pull request before merging** (`>= 1` lượt duyệt, hủy duyệt cũ khi có commit mới), và thêm các job của `.github/workflows/ci.yml` làm **Required status checks** ngay sau khi đồng bộ workflow lên `origin/main`.
- **Owner:** `Repository Owner` & `Release Captain`
- **Trạng thái:** `OPEN` (`CONFIRMED_DISABLED`)

---

#### `PUB-SEC-04` — Secret Scanning và Secret Scanning Push Protection đều đang bị `DISABLED` trên kho mã công khai
- **ID:** `PUB-SEC-04`
- **Severity:** `P0`
- **Source:** `github-actions-threat-review.md` (`GHSET-SEC-02`)
- **Evidence type:** Kiểm chứng trực tiếp qua GitHub REST API chỉ đọc (`GET repos/manhquydev/zalo` trường `security_and_analysis` & `GET .../secret-scanning/alerts`)
- **File/commit liên quan:** Cấu hình bảo mật trực tiếp của kho `https://github.com/manhquydev/zalo` (`secret_scanning.status = "disabled"`, `secret_scanning_push_protection.status = "disabled"`, `secret_scanning_non_provider_patterns.status = "disabled"`, `HTTP 404 Secret scanning is disabled`)
- **Tác động:** Do kho mã chuyển từ `private` sang `public`, GitHub không tự bật các lớp quét bí mật miễn phí. Nếu một tác nhân AI hoặc lập trình viên vô tình commit khóa API, OAuth token, chứng chỉ ký số hoặc cookie phiên lên bất kỳ nhánh công khai nào, máy chủ GitHub sẽ không chặn ở bước `git push` và bí mật sẽ bị bot thu thập công khai trong vài giây.
- **Khuyến nghị xử lý:** Kích hoạt ngay trong `Settings -> Code security` của kho công khai: (1) **Secret scanning**, (2) **Push protection**, và (3) **Scan for non-provider patterns** để bao phủ cả khóa riêng tư PEM/generic.
- **Owner:** `Repository Owner` & `Release Captain`
- **Trạng thái:** `OPEN` (`CONFIRMED_DISABLED`)

---

#### `PUB-SEC-05` — Nhánh công khai `origin/main` (`8cf4703`) và `origin/feat/ui-vi-en-quiet-workspace` (`0f87302`) đang phân phối bản Backend chưa qua đợt gia cố bảo mật (HTTP Bridge không xác thực, ghi sao lưu ra đường dẫn tùy ý, thiếu `WinVerifyTrust` & bỏ qua xác nhận xóa)
- **ID:** `PUB-SEC-05`
- **Severity:** `P0`
- **Source:** Kiểm toán độc lập AppSec/Tauri/Web-UI bởi `@gate-synthesizer` (`APP-SEC-01`, đối chiếu lịch sử sửa lỗi `F-04`, `F-05`, `F-06`, `F-10` trong `post-swarm-hardening-report.md`)
- **Evidence type:** Phân tích mã nguồn đối chiếu trực tiếp giữa cây công khai `origin/main` (`8cf4703`) / `origin/feat/ui-vi-en-quiet-workspace` (`0f87302`) và các nhánh cục bộ chưa đẩy (`a5feb4a`, `cd62c52`)
- **File/commit liên quan:** Trên `origin/main` (`8cf4703`) và `origin/feat/ui-vi-en-quiet-workspace` (`0f87302`): `desktop-instance-manager/apps/desktop/ui-server.mjs` (dòng `107–360`), `desktop-instance-manager/apps/desktop/src-tauri/src/commands.rs` (dòng `100–310`), `desktop-instance-manager/crates/platform-windows/src/adapter.rs`, `desktop-instance-manager/scripts/dim-host.ps1`, và `multi-zalo-desktop/src/platform/WindowsPlatformAdapter.cs` (dòng `257–272`)
- **Tác động:** Bất kỳ người dùng bên ngoài nào clone nhánh mặc định công khai `https://github.com/manhquydev/zalo` ngay lúc này đều nhận mã nguồn tiền-`a5feb4a` chứa chuỗi lỗ hổng nghiêm trọng: (1) `ui-server.mjs` trên `127.0.0.1:18420` **không có xác thực `X-DIM-Session-Token`**, không kiểm tra `Sec-Fetch-Site` hay `Content-Type: application/json`, không lọc PII trong lỗi (`stderr`), và cho phép truyền `body.dest` tùy ý vào `POST /api/profiles/:id/backup` để trích xuất dữ liệu phiên Zalo ra thư mục ngoài; (2) `adapter.rs` trên `8cf4703` chỉ kiểm tra sự tồn tại bảng header PE (`has_pe_authenticode_table`) mà không gọi `WinVerifyTrust` hay ghim `CN/O=VNG GROUP JSC`; (3) `commands.rs` gán cứng `confirm = true` khi xóa hồ sơ và báo giả `PrimaryZaloSeparation: passed = true`; (4) `multi-zalo-desktop/src/platform/WindowsPlatformAdapter.cs` (dòng `271`) đặt `info.IsValid = true` ngay cả khi `X509Certificate.CreateFromSignedFile` ném ngoại lệ trên tệp thực thi bị giả mạo. *(Toàn bộ bản vá Rust/Bridge mới chỉ nằm cục bộ trên `a5feb4a`/`cd62c52`, chưa lên `origin/main`!).*
- **Khuyến nghị xử lý:** Sau khi hoàn tất xử lý các phát hiện `PUB-SEC-01`, `PUB-SEC-02`, `PUB-SEC-06`..`PUB-SEC-18` trên nhánh tích hợp, hợp nhất gộp sạch (`squash merge`) bản Backend/UI đã gia cố lên `origin/main`, gỡ bỏ hoặc khóa cảnh báo thư mục nguyên mẫu cũ `multi-zalo-desktop/`, và xóa nhánh công khai phụ `origin/feat/ui-vi-en-quiet-workspace` đang mang nền Backend cũ `8cf4703`.
- **Owner:** `Central Integration Agent` & `Release Captain`
- **Trạng thái:** `OPEN` (`FIXED_LOCALLY_IN_INTEGRATION_BUT_LIVE_VULNERABLE_ON_PUBLIC_ORIGIN`)

---

### Nhóm Mức Độ `P1` — Các Lỗ Hổng & Ràng Buộc Bắt Buộc Xử Lý Trước Khi Đẩy / Phát Hành (`CHANGES_REQUESTED`)

#### `PUB-SEC-06` — Biến môi trường kiểm thử `DIM_NODE_FIXTURE_SCRIPT` không được khóa cờ biên dịch trong bản Release `commands.rs`, cho phép chuyển hướng khởi chạy sang script Node.js tùy ý và hiển thị giả trạng thái `"Signed"`
- **ID:** `PUB-SEC-06`
- **Severity:** `P1`
- **Source:** Kiểm toán độc lập AppSec & Tauri IPC bởi `@gate-synthesizer` (`APP-SEC-02`)
- **Evidence type:** Phân tích tĩnh mã nguồn Rust Tauri IPC & bộ điều hợp nền tảng (`Static Rust Source Code Analysis`)
- **File/commit liên quan:** `desktop-instance-manager/apps/desktop/src-tauri/src/commands.rs` (dòng `24–44` hàm `resolve_tauri_launch_target()`, dòng `121–131` trong `get_status`, dòng `274` trong `launch_profile`, dòng `291` trong `focus_profile` — đưa vào từ commit `bff0674444496bc057ba42fcc26c4c5b2d93dd7b` và hiện diện tại `cd62c523222cb3a02051347e85313226de8b6964` trên `integration/reliability-hardening`), kết hợp với `desktop-instance-manager/crates/platform-windows/src/adapter.rs` (dòng `122–145`)
- **Tác động:** Hàm `resolve_tauri_launch_target()` được biên dịch vô điều kiện vào cả bản **Release** chính thức (`dim-desktop.exe` / `dim-desktop`). Nếu biến môi trường của người dùng (`HKCU\Environment`, `.zshrc`/`.bashrc` hoặc tiến trình cha không tin cậy) đặt `DIM_NODE_FIXTURE_SCRIPT` trỏ tới bất kỳ tệp `.js`/`.mjs` nào trên ổ đĩa, `get_status` bỏ qua bước xác minh cài đặt Zalo và trả về `"authenticode_status": "Signed (Node.js Fixture)"` (`valid: true`) trên giao diện; khi người dùng bấm **Mở Zalo**, `WindowsPlatformAdapter` chỉ xác minh chữ ký Authenticode của `node.exe` (`OpenJS Foundation`) rồi thực thi tệp JavaScript không ký số đó với đầy đủ quyền người dùng, vô hiệu hóa rào cản ghim nhà phát hành `CN/O=VNG GROUP JSC`.
- **Khuyến nghị xử lý:** Khóa chặt nhánh đọc `DIM_NODE_FIXTURE_SCRIPT` bằng cờ biên dịch tường minh không bật mặc định (ví dụ `#[cfg(any(debug_assertions, feature = "e2e-fixture-hook"))]`) trong `apps/desktop/src-tauri/Cargo.toml` và `commands.rs`; chỉ truyền `--features e2e-fixture-hook` trong kịch bản kiểm thử E2E cục bộ, đồng thời thêm kiểm tra trong `build-and-package.mjs` / `validate-workflow.mjs` cấm bật feature này trong mọi bản build Release/CI đóng gói.
- **Owner:** `Central Integration Agent`
- **Trạng thái:** `OPEN` (`PRESENT_AT_INTEGRATION_HEAD_cd62c52`)

---

#### `PUB-SEC-07` — Phơi nhiễm đường dẫn hệ điều hành cá nhân (`C:\Users\<REDACTED_USER>\...`) trên `2` nhánh đang công khai (`origin/main` & `origin/feat/ui-vi-en-quiet-workspace`)
- **ID:** `PUB-SEC-07`
- **Severity:** `P1`
- **Source:** `git-history-and-secret-scan.md` (`GIT-SEC-001`) & `evidence-exposure-review.md` (`EVID-SEC-02` — phần `origin/*`)
- **Evidence type:** Quét biểu thức chính quy tự động trên toàn bộ `229` blob công khai thuộc `origin/*`
- **File/commit liên quan:** `17` tệp (`~192` vị trí) trên `origin/main` (`8cf4703`) và `19` tệp (`~194` vị trí) trên `origin/feat/ui-vi-en-quiet-workspace` (`39ff5c7`..`0f87302`), bao gồm: `desktop-instance-manager/docs/security/threat-model-and-audit.md` (`64` liên kết `file:///c:/Users/<REDACTED_USER>/...`), `desktop-instance-manager/README.md` (`32` vị trí), `docs/testing/poc-and-test-report.md` (`11`), `docs/research/environment-readiness.md` (`6`), `plans/261117-zalo-instance-manager/plan.md` (`1`), `docs/design/{account-dock-redesign,ui-i18n-implementation-report,ui-independent-review-and-remediation}.md`, `multi-zalo-desktop/docs/research/{rea-capabilities,zalo-desktop-analysis}.md`, và toàn bộ `10` tệp `multi-zalo-desktop/evidence/*.json`
- **Tác động:** Làm lộ tên tài khoản người dùng Windows cục bộ (`<REDACTED_USER>`), bố cục thư mục phát triển và danh sách công cụ AI cài đặt trên máy cá nhân ra công chúng; các blob trong `8cf4703` vẫn tồn tại trong lịch sử Git nếu không viết lại lịch sử.
- **Khuyến nghị xử lý:** (1) Đẩy commit tiến (`forward/squash commit`, không viết lại lịch sử) đã làm sạch toàn bộ đường dẫn thành `<USER_HOME>` / `<WORKTREE_ROOT>` / đường dẫn tương đối lên `origin/main`; (2) Xóa nhánh phụ `origin/feat/ui-vi-en-quiet-workspace` sau khi đã gộp vào `main`; (3) Chủ sở hữu kho ký xác nhận chấp nhận rủi ro tồn dư mức thấp đối với tên người dùng OS trong commit khởi tạo `8cf4703`.
- **Owner:** `Central Integration Agent`, `Release Captain` & `Repository Owner`
- **Trạng thái:** `OPEN` (`LIVE_ON_PUBLIC_ORIGIN`)

---

#### `PUB-SEC-08` — `54` commit cục bộ chưa push (`426` blobs) và `HEAD` của `8` nhánh làm việc còn chứa đường dẫn `C:\Users\<REDACTED_USER>\...`, `.gitconfig` (`credential.helper`) và `.cargo`
- **ID:** `PUB-SEC-08`
- **Severity:** `P1`
- **Source:** `git-history-and-secret-scan.md` (`GIT-SEC-002`) & `evidence-exposure-review.md` (`EVID-SEC-02` — phần nhánh cục bộ)
- **Evidence type:** Phân tích đồ thị `git rev-list` và quét regex trên `426` blob chỉ tồn tại cục bộ
- **File/commit liên quan:** `HEAD` của `main` (`a5feb4a` — `12` tệp), `build/cross-platform-distribution` (`6c4150e` — `19` tệp, gồm `plans/reports/build/{ci-baseline-and-github-audit.md,dependency-audit-report.json,integration-requests.md,windows-local-build-report.md}` và `docs/packaging/{ci-execution-and-artifact-validation,release-readiness,windows}.md` lộ `C:/Users/<REDACTED_USER>/.gitconfig` & `.cargo`), `feat/macos-multi-instance` (`f2b8f9f` — `14` tệp), `feat/ubuntu-multi-instance` (`62dc5d1` — `12` tệp), `research/desktop-compatibility` (`0e5d44c` — `12` tệp), `test/cross-platform-reliability` (`5e802e0` — `12` tệp), `qa/windows-native-acceptance` (`004ab7c` — `24` tệp), cùng chuỗi `54` commit lịch sử trước `bff0674` trên `integration/reliability-hardening`
- **Tác động:** Dù `HEAD` của `integration/reliability-hardening` (`cd62c52`) đã lọc tên người dùng chủ máy ở `bff0674`, toàn bộ `54` commit cha vẫn giữ nguyên `426` blob chưa lọc. Nếu đẩy thẳng bất kỳ nhánh thành phần nào hoặc merge không gộp (`non-squash merge`) nhánh `integration/reliability-hardening` lên `origin`, toàn bộ đường dẫn `.gitconfig`/`credential.helper`/worktree mới sẽ bị công khai vĩnh viễn trên GitHub.
- **Khuyến nghị xử lý:** (1) Đóng băng tuyệt đối thao tác `git push` trực tiếp đối với mọi nhánh thành phần cục bộ; (2) Hợp nhất từ `integration/reliability-hardening` vào `main` bằng cơ chế **gộp sạch một commit (`git merge --squash`)** trên nền `origin/main` sau khi qua cổng kiểm tra tự động chống lộ đường dẫn `[A-Za-z]:[\/]Users[\/](?!Public|Default|Demo|<)`.
- **Owner:** `Release Captain` & `Central Integration Agent`
- **Trạng thái:** `OPEN` (`PRE_PUSH_BLOCKER`)

---

#### `PUB-SEC-09` — Tệp thực thi Windows PE/.NET biên dịch sẵn không chữ ký số `multi-zalo-desktop/dist/MultiZaloDesktop.exe` (`59.392` B) đang được theo dõi trực tiếp trên mọi nhánh
- **ID:** `PUB-SEC-09`
- **Severity:** `P1`
- **Source:** `git-history-and-secret-scan.md` (`GIT-SEC-003`), `evidence-exposure-review.md` (`EVID-SEC-03`), `licensing-and-compliance.md` (`LIC-SEC-003` — phần nhị phân)
- **Evidence type:** Phân tích tiêu đề PE32/.NET IL, giải mã bảng chuỗi ASCII/UTF-16LE và kiểm tra `git ls-tree`
- **File/commit liên quan:** `multi-zalo-desktop/dist/MultiZaloDesktop.exe` (blob `efc8089112`, SHA-256 `6da02a9461594808c2f3ce8adef2c8186059bcc6edf55b520af79ac4eec5473c` — hiện diện trên `origin/main` @ `8cf4703`, `origin/feat/ui-vi-en-quiet-workspace` @ `0f87302`, `main` @ `a5feb4a` và `integration/reliability-hardening` @ `cd62c52`)
- **Tác động:** Phân phối trực tiếp tệp thực thi `.exe` không chữ ký số, không đính kèm giấy phép trong cây Git công khai làm suy yếu tính minh bạch chuỗi cung ứng (không thể kiểm chứng tương ứng 1-1 với mã nguồn khi duyệt kho mã), dễ bị Windows Smart App Control / SmartScreen chặn và gây nhầm lẫn với ứng dụng chính `desktop-instance-manager`.
- **Khuyến nghị xử lý:** Chạy `git rm multi-zalo-desktop/dist/MultiZaloDesktop.exe` (hoặc gỡ toàn bộ thư mục nguyên mẫu cũ `multi-zalo-desktop/`) trong commit tiến trên nhánh tích hợp và chặn `*.exe` / `dist/` trong `.gitignore`.
- **Owner:** `Central Integration Agent` & `Build Agent`
- **Trạng thái:** `OPEN` (`LIVE_ON_PUBLIC_ORIGIN_AND_ALL_BRANCHES`)

---

#### `PUB-SEC-10` — Xuất bản công khai `7` tệp dump phân tích nhị phân cấp thấp của REA (`multi-zalo-desktop/evidence/rea-*.json`) chứa cấu trúc nội bộ `app.asar` và `5` đường dẫn `.pdb` nội bộ của kỹ sư VNG
- **ID:** `PUB-SEC-10`
- **Severity:** `P1`
- **Source:** `evidence-exposure-review.md` (`EVID-SEC-04`) & `licensing-and-compliance.md` (`LIC-SEC-003` — phần `rea-*.json`)
- **Evidence type:** Phân tích schema JSON và dữ liệu quan sát nhị phân (`JSON Schema & Payload Inspection`)
- **File/commit liên quan:** `multi-zalo-desktop/evidence/{rea-audit-release-exposure-app-asar.json, rea-pe-imports-electron-zalo-26.10.20.json, rea-pe-imports-stub-zalo.json, rea-pe-imports-zalosetup-26.11.12.json, rea-pe-resources-stub-zalo.json, rea-pe-resources-zalosetup-26.11.12.json, rea-binary-session.json}` (đang công khai trên `origin/main` @ `8cf4703` & `origin/feat/ui-vi-en-quiet-workspace` @ `0f87302`; riêng `rea-audit-release-exposure-app-asar.json` tại `cd62c52` blob `85be3d88` vẫn còn `5` đường dẫn `.pdb` máy trạm/GitLab Runner nội bộ của kỹ sư VNG)
- **Tác động:** Phân loại **`PRIVATE_OR_LOCAL_STORAGE_ONLY`** / **`LICENSE_REVIEW_REQUIRED`**: dù không nhúng mã nguồn JS hay icon của VNG, việc công bố hơn `240 KB` bảng offset/import PE thương mại và đặc biệt là chỉ điểm các đường dẫn `.pdb` máy tính cá nhân/CI nội bộ của nhân viên VNG vượt quá phạm vi tài liệu tương thích cần thiết và không phù hợp cấp phép mã nguồn mở.
- **Khuyến nghị xử lý:** Xóa cả `7` tệp `multi-zalo-desktop/evidence/rea-*.json` khỏi nhánh công khai bằng `git rm` trong forward commit (chỉ lưu ngoại tuyến nội bộ nếu cần) và ghi rõ trong `README.md`/`LICENSE` rằng các tệp bằng chứng kiểm thử còn lại không thuộc phạm vi cấp phép mã phần mềm.
- **Owner:** `Central Integration Agent` & `Release Captain`
- **Trạng thái:** `OPEN` (`LIVE_ON_PUBLIC_ORIGIN_AND_ALL_BRANCHES`)

---

#### `PUB-SEC-11` — Khoảng trống bảo vệ trong `.gitignore` đối với khóa ký số, gói `*.asar`, nhị phân Zalo/trình cài đặt, dữ liệu phiên runtime (`ZaloData`/`Cookies`/`Local State`) và bản sao lưu `.zip`
- **ID:** `PUB-SEC-11`
- **Severity:** `P1`
- **Source:** `git-history-and-secret-scan.md` (`GIT-SEC-006`) & `evidence-exposure-review.md` (`EVID-SEC-06`)
- **Evidence type:** Đối chiếu quy tắc `.gitignore` trên `10` worktree với các tạo phẩm sinh ra khi kiểm thử/đóng gói
- **File/commit liên quan:** `.gitignore` gốc (`20` dòng trên `8cf4703`, `a5feb4a`, `cd62c52`; riêng `feat/ui-vi-en-quiet-workspace` @ `0f87302` còn thiếu cả `desktop-instance-manager/apps/desktop/src-tauri/gen/`)
- **Tác động:** Khi nhiều tác nhân AI và lập trình viên thao tác song song hoặc chạy kiểm thử L3 với tài khoản Zalo thật, việc thiếu quy tắc chặn khiến một thao tác `git add .` rất dễ đưa nhầm khóa ký (`.pfx`, `.pem`, `.tauri.key`), gói `app.asar` / `Zalo*.exe`, hoặc cơ sở dữ liệu phiên đăng nhập (`Cookies`, `Local State`, `*.ldb`, `*.sqlite*`, `ZaloData/`, `Profiles/`, `*.zip`) thẳng lên kho công khai.
- **Khuyến nghị xử lý:** Bổ sung đầy đủ các mẫu chặn khóa mật mã/chứng chỉ, tạo phẩm RE (`.rea/`, `*.asar`, `*.jsc`, `*.pdb`, `*.dmp`), nhị phân/gói cài đặt (`Programs/`, `Zalo*.exe`, `*.exe`, `*.dll`, `*.msi`, `*.dmg`, `*.deb`, `*.AppImage`, `dist/`), và dữ liệu phiên/sao lưu (`ZaloData/`, `MultiZaloDesktop*/`, `Profiles/`, `Local State`, `Cookies*`, `*.ldb`, `*.sqlite*`, `*.db*`, `*.zip`) vào `.gitignore` gốc trước lần push kế tiếp.
- **Owner:** `Central Integration Agent` & `Build Agent`
- **Trạng thái:** `OPEN`

---

#### `PUB-SEC-12` — `16` ảnh chụp màn hình đã xóa (`--diff-filter=D`) và tệp workflow cũ vẫn nằm trong lịch sử Git công khai trên `origin`
- **ID:** `PUB-SEC-12`
- **Severity:** `P1`
- **Source:** `git-history-and-secret-scan.md` (`GIT-SEC-004`, đối chiếu kiểm tra thị giác trong `evidence-exposure-review.md` Mục 3.4)
- **Evidence type:** Kiểm toán `git diff-tree -r --diff-filter=D` kết hợp kiểm tra trực quan `16/16` blob ảnh lịch sử
- **File/commit liên quan:** Commit `cc74584` trên `origin/feat/ui-vi-en-quiet-workspace` (`16` blob ảnh PNG cũ từ `39ff5c7`/`2b1ce6e`/`b1446d6`, dung lượng `~828 KB`); `desktop-instance-manager/.github/workflows/ci.yml` (`blob=e61d068083` tại `HEAD` của `origin/main` @ `8cf4703`)
- **Tác động:** Kiểm tra thị giác của `@evidence-auditor` đã xác nhận toàn bộ `16` ảnh đã xóa chỉ chứa dữ liệu giao diện giả lập (`Skipped in mock mode`, `<SYNTHETIC_DEMO_UID>`), không lộ dữ liệu thật. Tuy nhiên nếu merge thường nhánh `feat/ui-vi-en-quiet-workspace` vào `main`, cả `16` blob nhị phân lỗi thời này sẽ đi vĩnh viễn vào lịch sử nhánh mặc định `origin/main`.
- **Khuyến nghị xử lý:** Sử dụng gộp sạch (`squash merge`) khi đưa kết quả tích hợp lên `origin/main` để không nhập lịch sử `16` blob ảnh đã xóa vào `main`, sau đó xóa nhánh từ xa `origin/feat/ui-vi-en-quiet-workspace`.
- **Owner:** `Release Captain` & `Central Integration Agent`
- **Trạng thái:** `OPEN`

---

#### `PUB-SEC-13` — Workflow `ci.yml` trên `origin/*` đặt sai thư mục lồng khiến `0` cổng CI chạy thực tế, bản trên `main` (`a5feb4a`) thiếu `--locked`, và toàn bộ Actions dùng tag khả biến xung đột với `validate-workflow.mjs`
- **ID:** `PUB-SEC-13`
- **Severity:** `P1`
- **Source:** `github-actions-threat-review.md` (`GHA-SEC-01`, `GHA-SEC-02`)
- **Evidence type:** Kiểm tra cây Git, phân tích YAML & `validate-workflow.mjs`, và xác minh `GET repos/manhquydev/zalo/actions/workflows` (`total_count: 0`)
- **File/commit liên quan:** `desktop-instance-manager/.github/workflows/ci.yml` (`8cf4703`, `0f87302`); `.github/workflows/ci.yml` (`a5feb4a`, `cd62c52`); `.github/workflows/cross-platform-validation.yml` (`6c4150e`, `cd62c52` — `28` bước `uses:`); `desktop-instance-manager/scripts/ci-cross-platform/validate-workflow.mjs` (dòng `23–31`, `220–233`)
- **Tác động:** (1) Kho công khai hiện có **`0` workflow hoạt động** do `ci.yml` trên `origin/main` nằm trong thư mục con; bản `ci.yml` trên `main` cục bộ (`a5feb4a`) lại thiếu `--locked`. (2) Toàn bộ `uses:` dùng tag/nhánh khả biến (`@v4`, `@v2`, `@stable`) có nguy cơ bị tấn công chuỗi cung ứng trỏ lại tag sang mã độc; đồng thời `validate-workflow.mjs` khóa cứng chuỗi tag khả biến trong `ALLOWED_USES`, làm gãy bước kiểm tra nếu chỉ ghim SHA trong YAML mà không cập nhật script.
- **Khuyến nghị xử lý:** Áp dụng bản vá `ci.yml` và `cross-platform-validation.yml` đã ghim commit SHA 40 ký tự xác thực (kèm `IR-007` `--locked`), cập nhật đồng bộ hàm `isImmutablePinnedActionRef` trong `validate-workflow.mjs` (theo Mục 7.1–7.3 của `github-actions-threat-review.md`), cấp quyền OAuth scope `workflow` qua thiết bị đầu cuối của chủ sở hữu và đẩy `.github/workflows/ci.yml` lên `origin/main`.
- **Owner:** `Build Agent` & `Release Captain`
- **Trạng thái:** `OPEN` (`PARTIALLY_MITIGATED_UNPUSHED`)

---

#### `PUB-SEC-14` — Workflow `ci.yml` thiếu `permissions: contents: read`, thiếu `persist-credentials: false`, thiếu `timeout-minutes` / `concurrency:`, và lưu cache `~/.cargo/bin/` trên nhánh mặc định
- **ID:** `PUB-SEC-14`
- **Severity:** `P1`
- **Source:** `github-actions-threat-review.md` (`GHA-SEC-03`)
- **Evidence type:** Phân tích tĩnh cấu hình bảo mật GitHub Actions YAML (`Workflow YAML Static Analysis`)
- **File/commit liên quan:** `.github/workflows/ci.yml` (dòng `1–174` trên `a5feb4a` và `cd62c52`); `.github/workflows/cross-platform-validation.yml` (`7` bước `actions/checkout` tại dòng `62, 112, 175, 355, 531, 684, 842`)
- **Tác động:** Không chốt cứng `permissions: contents: read` trực tiếp trong `ci.yml`; để mặc định `persist-credentials: true` khiến `GITHUB_TOKEN` lưu trên đĩa trong `.git/config` cho mọi `build.rs`/proc-macro đọc được; thiếu `timeout-minutes` để mặc định `360` phút/job dễ gây cạn kiệt hàng đợi runner; và lưu cache `~/.cargo/bin/` trên `main` tạo rủi ro lây nhiễm tệp thực thi qua các lượt chạy kế tiếp.
- **Khuyến nghị xử lý:** Thêm `permissions: contents: read`, khối `concurrency:`, `timeout-minutes: 20` vào `ci.yml`; đặt `persist-credentials: false` trên mọi bước `actions/checkout` ở cả hai workflow; và xóa `~/.cargo/bin/` khỏi đường dẫn `actions/cache` trong `ci.yml`.
- **Owner:** `Build Agent`
- **Trạng thái:** `OPEN`

---

#### `PUB-SEC-15` — Nội suy trực tiếp `${{ inputs.bundles }}` (không ngoặc kép trong PowerShell) & `${{ github.* }}` vào `run:`, và cài `@tauri-apps/cli@2.12.5` qua `npm install --global` thiếu lockfile / thiếu `--ignore-scripts`
- **ID:** `PUB-SEC-15`
- **Severity:** `P1`
- **Source:** `github-actions-threat-review.md` (`GHA-SEC-04`, `GHA-SEC-05`)
- **Evidence type:** Phân tích tiêm biểu thức YAML & chuỗi cung ứng trình đóng gói npm
- **File/commit liên quan:** `.github/workflows/cross-platform-validation.yml` (dòng `231–233, 254, 326, 404–406, 432, 502, 567–569, 588, 653, 723–725, 744, 809` trên `6c4150e` và `cd62c52`)
- **Tác động:** (1) Mở rộng trực tiếp `${{ inputs.bundles }}` không ngoặc kép trong khối `shell: pwsh` (dòng `432`) và `${{ github.* }}` trong `run:` tạo mẫu nguy hiểm dẫn tới tiêm lệnh shell nếu kiểu input thay đổi hoặc được sao chép sang workflow khác. (2) Lệnh `npm install --global "@tauri-apps/cli@2.12.5"` phân giải động các gói phụ thuộc chuyển tiếp không qua kiểm chứng SHA-512 `package-lock.json` và cho phép chạy `postinstall` script bên thứ ba ngay trên runner tạo gói `.msi`/`.dmg`/`.deb`/`.AppImage`.
- **Khuyến nghị xử lý:** Chuyển toàn bộ `${{ inputs.* }}` và `${{ github.* }}` sang khối biến môi trường `env:` của step (`"$INPUT_BUNDLES"` / `"$env:INPUT_BUNDLES"`); chuyển cài đặt `@tauri-apps/cli@2.12.5` sang thư mục công cụ có `package-lock.json` đã commit kèm `npm ci --ignore-scripts`.
- **Owner:** `Build Agent`
- **Trạng thái:** `OPEN`

---

#### `PUB-SEC-16` — Private Vulnerability Reporting đang bị `DISABLED` và kho mã công khai thiếu cả `SECURITY.md` lẫn `CONTRIBUTING.md`
- **ID:** `PUB-SEC-16`
- **Severity:** `P1`
- **Source:** `github-actions-threat-review.md` (`GHSET-SEC-03`) & `licensing-and-compliance.md` (`LIC-SEC-006`)
- **Evidence type:** Kiểm chứng GitHub REST API (`GET .../private-vulnerability-reporting` -> `{"enabled": false}`) và kiểm tra cây tệp gốc `/` & `.github/`
- **File/commit liên quan:** Cấu hình trực tiếp trên `repos/manhquydev/zalo` (`has_issues: true`, `private-vulnerability-reporting: false`) và thư mục gốc `/` trên mọi nhánh (`8cf4703`, `0f87302`, `a5feb4a`, `cd62c52`)
- **Tác động:** Người phát hiện lỗ hổng không có kênh báo cáo bảo mật riêng tư nên dễ đăng chi tiết khai thác zero-day thẳng lên GitHub Issues công khai; đồng thời thiếu `CONTRIBUTING.md` khiến dự án không có quy định DCO và không có hàng rào cấm cộng tác viên gửi PR chứa mã giải dịch `app.asar`, tài nguyên VNG hoặc dữ liệu phiên Zalo thật.
- **Khuyến nghị xử lý:** Bật **Private vulnerability reporting** trong `Settings -> Code security`; thêm `SECURITY.md` hướng dẫn báo cáo qua tab `Security -> Report a vulnerability` (không lộ email cá nhân, cấm mở Issue công khai cho lỗi bảo mật) và thêm `CONTRIBUTING.md` quy định đóng góp phòng sạch (`inbound = outbound`, cấm nhị phân/mã VNG và PII).
- **Owner:** `Repository Owner`, `Release Captain` & `Central Integration Agent`
- **Trạng thái:** `OPEN` (`CONFIRMED_DISABLED`)

---

#### `PUB-SEC-17` — Dependabot Alerts & Security Updates đang bị `DISABLED` (thiếu `.github/dependabot.yml`), và chính sách GitHub Actions / phê duyệt PR từ Fork chưa siết chặt tối đa
- **ID:** `PUB-SEC-17`
- **Severity:** `P1`
- **Source:** `github-actions-threat-review.md` (`GHSET-SEC-04`, `GHSET-SEC-05`)
- **Evidence type:** Kiểm chứng GitHub REST API (`GET .../vulnerability-alerts`, `.../automated-security-fixes`, `.../actions/permissions`, `.../fork-pr-contributor-approval`, `.../artifact-and-log-retention`)
- **File/commit liên quan:** Cấu hình kho `repos/manhquydev/zalo` (`vulnerability-alerts: 404 disabled`, `automated-security-fixes: false`, `allowed_actions: "all"`, `sha_pinning_required: false`, `approval_policy: "first_time_contributors"`, `retention: 90 days`) và thiếu `.github/dependabot.yml`
- **Tác động:** Không tự động cảnh báo khi phát sinh CVE mới trong `439` crate Rust hoặc GitHub Actions khi kho không có commit mới; cho phép chạy mọi Action bên thứ ba chưa ghim SHA; cho phép cộng tác viên ngoài sau 1 PR nhỏ được tự động kích hoạt runner ở các PR tiếp theo; và giữ log chẩn đoán công khai tới `90` ngày.
- **Khuyến nghị xử lý:** (1) Bật **Dependency graph**, **Dependabot alerts**, **Dependabot security updates** và thêm `.github/dependabot.yml` (`github-actions` + `cargo`); (2) Trong `Settings -> Actions -> General`, giới hạn danh sách Actions được phép, bật `sha_pinning_required: true` (sau khi vá `PUB-SEC-13`), nâng phê duyệt PR từ fork lên `all_external_contributors`, và giảm thời hạn lưu log/artifact xuống `14` ngày.
- **Owner:** `Repository Owner`, `Release Captain` & `Build Agent`
- **Trạng thái:** `OPEN` (`CONFIRMED_DISABLED_OR_SUBOPTIMAL`)

---

#### `PUB-SEC-18` — Rủi ro nhãn hiệu `"Zalo"` thiếu tuyên bố phi liên kết; thiếu `THIRD_PARTY_NOTICES.md` cho `7` crate `MPL-2.0` & thư viện `LGPL` trong `.AppImage` Linux; và sai lệch/khoảng trống cấu hình `deny.toml`
- **ID:** `PUB-SEC-18`
- **Severity:** `P1`
- **Source:** `licensing-and-compliance.md` (`LIC-SEC-002`, `LIC-SEC-004`, `LIC-SEC-005`)
- **Evidence type:** Kiểm toán nhãn hiệu/UI, phân tích SPDX toàn bộ `434` gói trong `src-tauri/Cargo.lock`, pipeline đóng gói Linux và cấu hình `deny.toml`
- **File/commit liên quan:** URL kho `https://github.com/manhquydev/zalo`, `desktop-instance-manager/apps/desktop/src-tauri/tauri.conf.json` (`productName: "Multi-Zalo Desktop"`, `identifier: "com.multizalodesktop.manager"`), `apps/desktop/ui/{index.html,i18n/vi-VN.json,i18n/en-US.json}`, `apps/desktop/src-tauri/Cargo.lock` (`7` crate `MPL-2.0`: `cssparser`, `cssparser-macros`, `dtoa-short`, `option-ext`, `selectors`), `scripts/ci-cross-platform/build-and-package.mjs` (`.AppImage`), `desktop-instance-manager/deny.toml` (`version = 2` trên `8cf4703`/`a5feb4a`, thiếu `MPL-2.0` trên `origin/*`), và `fixtures/cross-platform/Cargo.toml`
- **Tác động:** (1) Dùng `"Zalo"` làm danh từ thương hiệu chính và tên repo mà có **`0` tuyên bố phi liên kết** với VNG Corporation dễ dẫn đến khiếu nại nhãn hiệu. (2) Phân phối nhị phân liên kết tĩnh `7` crate `MPL-2.0` mà thiếu thông báo nơi tải mã nguồn vi phạm **MPL-2.0 Mục 3.2(a)**; đóng gói `.AppImage` nhúng trực tiếp các `.so` `LGPL-2.0+/2.1+/3.0` (`libwebkit2gtk-4.1`, `libgtk-3`, `libayatana-appindicator3`) mà không kèm văn bản `LGPL` và kho lưu trữ mã nguồn tương ứng vi phạm **LGPL-2.1 Mục 4 & 6 / LGPL-3.0 Mục 4**. (3) `deny.toml` trên `origin/*` và `main` làm gãy `cargo-deny >= 0.20`, đồng thời bỏ sót `fixtures/cross-platform/Cargo.toml` và thiếu `publish = false` ở các crate nội bộ.
- **Khuyến nghị xử lý:** (1) Thêm tuyên bố phi liên kết song ngữ Việt/Anh nổi bật ở đầu `README.md` và `Settings -> About`, chuyển tên hiển thị sang dạng *Nominative Fair Use* (`... for Zalo Desktop`) và đổi `identifier` sang namespace riêng không chứa `zalo`; (2) Tạo `desktop-instance-manager/THIRD_PARTY_NOTICES.md` (đáp ứng `MPL-2.0` §3.2(a)) và chỉ phát hành gói `.deb` trên Linux (tạm ngưng xuất bản `.AppImage` công khai cho đến khi lưu trữ đầy đủ gói mã nguồn `LGPL`); (3) Đồng bộ `deny.toml` đã gỡ `version = 2`, thêm `publish = false` cho mọi crate nội bộ và quét `cargo-deny` phủ đủ cả `4` tệp `Cargo.lock`.
- **Owner:** `Repository Owner`, `Central Integration Agent`, `Account Dock/UI Owner` & `Build Agent`
- **Trạng thái:** `OPEN`

---

### Nhóm Mức Độ `P2` — Gia Cố Chiều Sâu & Hoàn Thiện Trước Phát Hành Chính Thức

#### `PUB-SEC-19` — Bật `withGlobalTauri: true` nhưng thiếu cấu hình ACL tường minh `src-tauri/capabilities/default.json`; CSP chưa bật Trusted Types (và bản trên `feat/ui-*` thiếu `object-src`/`form-action`); `get_status` trả đường dẫn `data_dir` thô lên UI
- **ID:** `PUB-SEC-19`
- **Severity:** `P2`
- **Source:** Kiểm toán độc lập AppSec, Tauri v2 & Web UI bởi `@gate-synthesizer` (`APP-SEC-03`, `APP-SEC-04`, đối chiếu hướng dẫn `modern-web-guidance`: `security`, `trusted-types`, `sanitize-untrusted-html`)
- **Evidence type:** Kiểm tra cấu hình `tauri.conf.json`, `gen/schemas/capabilities.json`, `commands.rs` và toàn bộ mã frontend `apps/desktop/ui/**`
- **File/commit liên quan:** `desktop-instance-manager/apps/desktop/src-tauri/tauri.conf.json` (dòng `10` `withGlobalTauri: true`, dòng `22` CSP; bản `138` ký tự trên `0f87302` thiếu `object-src 'none'` và `form-action 'self'`), `apps/desktop/src-tauri/gen/schemas/capabilities.json` (`{}`), `apps/desktop/src-tauri/src/commands.rs` (dòng `186` `data_dir`, dòng `386` `backup_dir`), `apps/desktop/ui/app.js` (dòng `1013`)
- **Tác động:** Mặc dù toàn bộ `apps/desktop/ui/**` đã tuân thủ nghiêm ngặt việc dùng `createElement`/`textContent` (`0` vị trí dùng `.innerHTML`, `.outerHTML`, `eval`) và CSP chặn script ngoại lai, việc thiếu tệp `apps/desktop/src-tauri/capabilities/default.json` khiến giới hạn cửa sổ/URL của Tauri v2 ACL không được khai báo tường minh trong mã nguồn; việc thiếu `require-trusted-types-for 'script'` bỏ lỡ hàng rào chặn DOM XSS ở mức runtime trên WebView2/WKWebView; và việc hiển thị `state.status.data_dir` chưa che mặt nạ trong màn hình `Kiểm tra sự cố` dễ làm lộ tên user Windows nếu người dùng chụp ảnh màn hình báo lỗi.
- **Khuyến nghị xử lý:** Bổ sung `apps/desktop/src-tauri/capabilities/default.json` giới hạn quyền IPC cho duy nhất cửa sổ `"main"` trên origin nội bộ; áp dụng `redact_sensitive_text` cho trường hiển thị `data_dir` trên UI (và truyền `backup_name` tương đối thay cho đường dẫn tuyệt đối `backup_dir` khi gọi `restore_backup`); thử nghiệm bổ sung `require-trusted-types-for 'script'` ở chế độ tương thích đa nền tảng.
- **Owner:** `Central Integration Agent` & `Account Dock/UI Owner`
- **Trạng thái:** `OPEN`

---

#### `PUB-SEC-20` — Còn sót `3` tệp ngay tại `HEAD` của mọi nhánh (kể cả sau đợt lọc `bff0674`) chứa đường dẫn `C:\Users\<REDACTED_USER>\...` của máy build upstream và tác giả kho tham khảo bên thứ ba
- **ID:** `PUB-SEC-20`
- **Severity:** `P2`
- **Source:** `git-history-and-secret-scan.md` (`GIT-SEC-005`) & `evidence-exposure-review.md` (`EVID-SEC-07` — phần đường dẫn bên thứ ba)
- **Evidence type:** Quét biểu thức chính quy tổng quát `PII_WIN_USER_PATH` trên cây tệp tại `cd62c52` và `8cf4703`
- **File/commit liên quan:** `multi-zalo-desktop/evidence/rea-audit-release-exposure-app-asar.json` (blob `f75997fb` / `85be3d88`), `desktop-instance-manager/docs/research/repository-comparison.md` (dòng `42`, blob `0d6e7859`), `multi-zalo-desktop/docs/research/existing-solutions.md` (dòng `16, 42, 46`, blob `e363b210`)
- **Tác động:** Đợt lọc `bff0674` chỉ khớp theo tên tài khoản của chủ máy hiện tại nên bỏ sót đường dẫn cá nhân Windows của kỹ sư upstream và tác giả kho bên ngoài, vừa làm lộ tên người dùng bên thứ ba vừa gây cảnh báo trên bộ quét PII tự động.
- **Khuyến nghị xử lý:** Gỡ `rea-audit-release-exposure-app-asar.json` theo `PUB-SEC-10` và thay tên người dùng trong chuỗi trích dẫn tại `repository-comparison.md` / `existing-solutions.md` bằng `<hardcoded_author_user>`.
- **Owner:** `Central Integration Agent`
- **Trạng thái:** `OPEN`

---

#### `PUB-SEC-21` — Phơi nhiễm thông tin trinh sát môi trường vận hành (trạng thái Registry SAC/WDAC, mã băm tệp Zalo chính đang chạy, `X-Oauth-Scopes` của `gh`, commit chưa ký GPG/SSH và `2` blob mồ côi chứa PII)
- **ID:** `PUB-SEC-21`
- **Severity:** `P2`
- **Source:** `evidence-exposure-review.md` (`EVID-SEC-05`) & `git-history-and-secret-scan.md` (`GIT-SEC-008`)
- **Evidence type:** Kiểm tra nội dung JSON/Markdown chẩn đoán, chữ ký commit (`%G? = N`) và đối tượng không thể truy cập trong `.git/objects`
- **File/commit liên quan:** `desktop-instance-manager/evidence/zalo-live-poc-result.json`, `multi-zalo-desktop/evidence/{poc-verification-result,integration-test-result}.json`, `desktop-instance-manager/docs/research/environment-readiness.md`, `plans/reports/build/{windows-local-build-report,ci-baseline-and-github-audit}.md`, `docs/packaging/ci-execution-and-artifact-validation.md`, `docs/reviews/integration-and-defect-remediation.md`, và `3` commit / `2` blob mồ côi trong `.git/objects` cục bộ
- **Tác động:** Cung cấp thông tin trinh sát máy trạm nhà phát triển (thời điểm `VerifiedAndReputablePolicyState = 0`, bản dựng OS, PID/HWND và mã băm SHA-256 tệp `config.json` của tài khoản Zalo chính, danh sách OAuth scopes hiện hành); `66/66` commit chưa ký số làm giảm khả năng xác thực danh tính người đóng góp; các blob mồ côi có thể lộ PII nếu sao chép thô thư mục `.git`.
- **Khuyến nghị xử lý:** Thay mã băm cụ thể của tệp Zalo chính trong `zalo-live-poc-result.json` bằng cờ boolean (`primary_files_sha256_identical: true`), chuẩn hóa thông số máy cá nhân và lược bỏ nguyên văn `X-Oauth-Scopes` trong báo cáo công khai; bật `commit.gpgsign = true` (SSH/GPG) với Vigilant Mode; không bao giờ chia sẻ nén thư mục `.git` cục bộ.
- **Owner:** `Central Integration Agent`, `Release Captain` & `Repository Owner`
- **Trạng thái:** `OPEN`

---

#### `PUB-SEC-22` — Tài liệu nghiên cứu tương thích ghi chi tiết địa chỉ byte offset & tên biến rút gọn trong `main.js` cùng cơ chế ghi đè con trỏ socket `PipeZCallSend`
- **ID:** `PUB-SEC-22`
- **Severity:** `P2`
- **Source:** `evidence-exposure-review.md` (`EVID-SEC-07` — phần biên giới tài liệu nghiên cứu)
- **Evidence type:** Đánh giá tĩnh nội dung tài liệu nghiên cứu (`Static Documentation Boundary Review`)
- **File/commit liên quan:** `desktop-instance-manager/docs/research/compatibility/{windows-findings.md, macos-findings.md, evidence-quality-review.md}` và `desktop-instance-manager/plans/reports/compatibility/ak-xia-missing-features-comparison.md` (trên `0e5d44c`, `004ab7c`, `cd62c52`)
- **Tác động:** Việc công bố chi tiết các dải byte offset (`main.js:311xxx..317xxx`, `3561xxx..3563xxx`), tên hàm/biến nén nội bộ (`SecureCipher`, `secure_ks_<hash>`) và từng bước ghi đè callback trên `\\.\pipe\PipeZCallSend` vượt quá mức cần thiết của tài liệu hướng dẫn tương thích công khai.
- **Khuyến nghị xử lý:** Biên tập gọn các tài liệu trong `docs/research/compatibility/` để giữ nguyên kết luận trung thực về giới hạn hệ điều hành/IPC nhưng lược bỏ địa chỉ byte offset cụ thể và tên biến nén nội bộ của `main.js`.
- **Owner:** `Central Integration Agent`
- **Trạng thái:** `OPEN`

---

#### `PUB-SEC-23` — Tồn lưu thư mục nguyên mẫu cũ `multi-zalo-desktop/`, `25` báo cáo điều phối tác nhân `plans/reports/**`, `21` tệp `untracked` trên các worktree đang chạy và kho clone ngoài `rea/` ngay trong thư mục gốc
- **ID:** `PUB-SEC-23`
- **Severity:** `P2`
- **Source:** `evidence-exposure-review.md` (`EVID-SEC-08`), `git-history-and-secret-scan.md` (`GIT-SEC-007`), `licensing-and-compliance.md` (`LIC-SEC-008`)
- **Evidence type:** Kiểm toán cấu trúc thư mục liên worktree, trạng thái `git status -uall --ignored` và giấy phép bên thứ ba trong `rea/`
- **File/commit liên quan:** Cây `multi-zalo-desktop/` (`37` tệp trên mọi nhánh); `desktop-instance-manager/plans/reports/**` (`25` tệp trên `cd62c52`); `3` tệp untracked `docs/release/*.md` trong worktree `zalo`; `18` tệp untracked `docs/qa/windows/*` & `tests/windows-acceptance/**` trong `zalo-windows-acceptance`; thư mục cục bộ `zalo/rea/` (`~9.100` tệp của `morluto/rea` @ `e9c72db` chứa hỗn hợp `MIT`, `GPL-3.0`, `LGPL-2.0`, `Apache-2.0`, đang được chặn bởi dòng `2` của `.gitignore`)
- **Tác động:** `multi-zalo-desktop/` đã ngừng phát triển nhưng lại là nguồn gây ra phần lớn phát hiện `P0`/`P1` (`PUB-SEC-01`, `05`, `09`, `10`); các biên bản điều phối AI nội bộ làm phình bề mặt thông tin vận hành; và việc để kho ngoài `rea/` nằm ngay trong `zalo/` tiềm ẩn nguy cơ `git add -f` nhầm thành phần `GPL-3.0`/`LGPL-2.0` hoặc vector khóa giả lập.
- **Khuyến nghị xử lý:** Khuyến nghị nghỉ hưu (`git rm -r multi-zalo-desktop`) khỏi `HEAD` nhánh `main` công khai; không commit các biên bản điều phối nội bộ (`docs/release/coordination-checkpoint.md`, `plans/reports/**`) lên nhánh phát hành chính; luôn `git add` tường minh từng đường dẫn; và di chuyển thư mục `zalo/rea/` sang một đường dẫn nghiên cứu độc lập bên ngoài kho `zalo` (vẫn giữ dòng `rea/` trong `.gitignore`).
- **Owner:** `Release Captain`, `Central Integration Agent` & `Repository Owner`
- **Trạng thái:** `OPEN`

---

#### `PUB-SEC-24` — Các script kiểm thử C2 bỏ sót `--locked` ở `6` lệnh `cargo build`; chưa tách kiến trúc `.github/workflows/release.yml` có GitHub Environment bảo vệ & OIDC SLSA v1.0; CodeQL chưa kích hoạt; tài liệu đóng gói vẫn ghi `private: true`
- **ID:** `PUB-SEC-24`
- **Severity:** `P2`
- **Source:** `github-actions-threat-review.md` (`GHA-SEC-06`, `GHA-SEC-07`, `GHSET-SEC-06`)
- **Evidence type:** Phân tích script kiểm thử CI, kiến trúc quản trị phát hành và trạng thái `GET .../code-scanning/default-setup` (`not-configured`)
- **File/commit liên quan:** `desktop-instance-manager/tests/cross-platform/scripts/{run-ubuntu.sh, run-macos.sh, run-windows.ps1}`; `.github/workflows/cross-platform-validation.yml`; `desktop-instance-manager/docs/packaging/{signing-and-release-gates,release-readiness,ci-execution-and-artifact-validation}.md`; cấu hình CodeQL trên `repos/manhquydev/zalo`
- **Tác động:** `6` lệnh `cargo build` thiếu `--locked` trong script C2 làm hổng cam kết khóa cứng phụ thuộc 100%; chưa có ranh giới Environment `release-production` tách biệt cho khóa ký số Gates G3/G4/G5 trong tương lai; chưa tận dụng SAST tự động miễn phí của GitHub CodeQL (`actions`, `rust`, `javascript-typescript`); và tài liệu chi phí/lưu trữ log vẫn giả định kho là `private: true`.
- **Khuyến nghị xử lý:** Thêm `--locked` cho cả `6` lệnh `cargo build` trong `run-{ubuntu.sh,macos.sh,windows.ps1}` và đưa đủ `4` tệp `Cargo.lock` vào khóa băm cache; bật **CodeQL Default Setup** sau khi đồng bộ `main`; cập nhật tài liệu đóng gói phản ánh trạng thái `public`; và chỉ triển khai ký số phát hành trong tệp `.github/workflows/release.yml` tách biệt gắn với GitHub Environment `release-production` có phê duyệt thủ công + `actions/attest-build-provenance`.
- **Owner:** `Build Agent` & `Release Captain`
- **Trạng thái:** `OPEN`

---

#### `PUB-SEC-25` — Thiếu khai báo trường giấy phép/bản quyền trong manifest đóng gói (`tauri.conf.json`, `package.json`), URL `repository` giả lập trên `origin/*`, và bộ icon vuông xanh `#0057D8` kèm huy hiệu `"Z"` có nguy cơ tương đồng nhận diện (Trade Dress)
- **ID:** `PUB-SEC-25`
- **Severity:** `P2`
- **Source:** `licensing-and-compliance.md` (`LIC-SEC-007`, `LIC-SEC-009`)
- **Evidence type:** Kiểm tra cấu trúc `tauri.conf.json`, `Cargo.toml`, `package.json` và giải mã kênh màu pixel của toàn bộ tệp biểu tượng (`.png`, `.ico`, `.icns`, `<svg>`)
- **File/commit liên quan:** `desktop-instance-manager/apps/desktop/src-tauri/tauri.conf.json`, `desktop-instance-manager/Cargo.toml` (`8cf4703`, `0f87302`), `desktop-instance-manager/fixtures/test-desktop-app/package.json`, `apps/desktop/src-tauri/icons/*`, và `apps/desktop/ui/index.html` (dòng `135`, `427`)
- **Tác động:** Trình cài đặt `.deb`/`.msi`/NSIS/macOS chưa tự động nhúng trường giấy phép và bản quyền; `Cargo.toml` trên `origin/*` trỏ về URL không tồn tại `https://github.com/local/desktop-instance-manager`; và mặc dù bộ icon hiện tại được chứng minh 100% là hình vuông 2 màu tự sinh (không sao chép bitmap của VNG), sắc xanh dương `#0057D8` đi kèm huy hiệu chữ `"Z"` có thể gây nhầm lẫn thị giác trên thanh tác vụ với Zalo chính thức.
- **Khuyến nghị xử lý:** Sau khi chốt giấy phép ở `PUB-SEC-02`, bổ sung `bundle.license`, `bundle.licenseFile`, `bundle.copyright` vào `tauri.conf.json` và `"license"` vào `package.json`; đồng bộ URL `repository` chính xác; thiết kế bộ biểu tượng đa cửa sổ mang bản sắc riêng của dự án và thay huy hiệu `"Z"` mặc định bằng biểu tượng không gian làm việc trung tính.
- **Owner:** `Build Agent`, `Account Dock/UI Owner` & `Central Integration Agent`
- **Trạng thái:** `OPEN`

---

## 4. Tham Chiếu Bộ Báo Cáo Chuyên Ngành Trong Cùng Thư Mục Kiểm Toán

- [git-history-and-secret-scan.md](./git-history-and-secret-scan.md) — Kiểm toán lịch sử Git, `660` đối tượng blob, `10` worktree & quét bí mật/PII (`@git-scanner`)
- [evidence-exposure-review.md](./evidence-exposure-review.md) — Kiểm toán bằng chứng nghiên cứu, đầu ra REA, mã đóng VNG & kiểm tra thị giác `36` ảnh chụp màn hình (`@evidence-auditor`)
- [github-actions-threat-review.md](./github-actions-threat-review.md) — Đánh giá mối đe dọa GitHub Actions, bản vá ghim SHA 40 ký tự & cấu hình GitHub API trực tiếp (`@ci-gh-auditor`)
- [licensing-and-compliance.md](./licensing-and-compliance.md) — Kiểm toán quản trị OSS, giấy phép `439` crate phụ thuộc & ranh giới sở hữu trí tuệ/nhãn hiệu VNG (`@license-auditor`)
- [public-release-gate.md](./public-release-gate.md) — Đánh giá quyết định cổng `PUBLIC_READINESS_PASS` & lộ trình gỡ chặn `P0` (`@gate-synthesizer`)
- [handoff-release-captain.md](./handoff-release-captain.md) & [handoff-central-integration.md](./handoff-central-integration.md) — Báo cáo bàn giao hành động tách biệt cho từng đầu mối thực thi
