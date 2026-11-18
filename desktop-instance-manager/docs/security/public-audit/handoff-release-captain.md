# Báo Cáo Bàn Giao Hành Động Cho Release Captain & Chủ Sở Hữu Kho Mã (`Handoff — Release Captain`)

- **Nhiệm vụ:** AGENT MISSION F — Điều phối gỡ chặn bảo mật, cấu hình GitHub công khai, pháp lý & đồng bộ phát hành (`https://github.com/manhquydev/zalo`)
- **Người nhận:** `Release Captain` & `Repository Owner` (Chủ sở hữu kho mã)
- **Tài liệu gốc tham chiếu:** [public-release-gate.md](./public-release-gate.md) & [security-findings.md](./security-findings.md)
- **Trạng thái cổng hiện tại:** **`PUBLIC_READINESS_PASS = BLOCKED`** (`5` lỗi `P0`: `PUB-SEC-01`..`PUB-SEC-05`, `13` yêu cầu sửa đổi `P1`: `PUB-SEC-06`..`PUB-SEC-18`)
- **Tin tức tích cực đã xác minh:**
  1. **`0` lộ khóa bí mật hay phiên đăng nhập Zalo thật:** Không cần thu hồi/xoay vòng khẩn cấp bất kỳ API key, GitHub PAT, khóa ký hay phiên Zalo nào; không cần viết lại lịch sử Git (`no history rewrite`).
  2. **Nhánh tích hợp trung tâm đã hoàn tất đợt code hiện tại:** Worktree `zalo-integration` (`integration/reliability-hardening`) đã tiến tới commit sạch `cd62c523222cb3a02051347e85313226de8b6964` (`dirty = 0`), sẵn sàng tiếp nhận gói vá bảo mật cuối cùng ở [handoff-central-integration.md](./handoff-central-integration.md).

---

## 1. Quy Tắc An Toàn Tuyệt Đối Cho Điều Phối Phát Hành

1. **ĐÓNG BĂNG `git push` trên toàn bộ các nhánh phụ cục bộ:** Tuyệt đối không push trực tiếp `main` (`a5feb4a`), `build/cross-platform-distribution` (`6c4150e`), `feat/macos-multi-instance` (`f2b8f9f`), `feat/ubuntu-multi-instance` (`62dc5d1`), `research/desktop-compatibility` (`0e5d44c`), `test/cross-platform-reliability` (`5e802e0`), `qa/windows-native-acceptance` (`004ab7c`), hoặc đẩy nguyên chuỗi `54` commit chưa gộp của `integration/reliability-hardening` lên `origin` — vì các commit lịch sử cục bộ đó chứa `426` blob còn lưu đường dẫn `C:\Users\<REDACTED_USER>\...`, `.gitconfig` và `.cargo` (`PUB-SEC-08`).
2. **KHÔNG viết lại lịch sử (`no push --force`, `no filter-repo`, `no rebase`)** và **KHÔNG chạm vào chỉ mục của các worktree anh em** (luôn dùng `git --no-optional-locks` khi kiểm tra trạng thái).
3. **KHÔNG tạo GitHub Issue công khai** để theo dõi các phát hiện trong `security-findings.md` khi kho mã đang mở `PUBLIC` (`has_issues: true`).

---

## 2. Danh Sách Hành Động Khẩn Cấp Của Chủ Sở Hữu Kho Mã (`Owner Immediate Actions — 10 Phút Đầu`)

### 2.1. Bật ngay hàng rào bảo mật miễn phí trên giao diện GitHub Web UI (Gỡ chặn `PUB-SEC-03`, `PUB-SEC-04`, `PUB-SEC-16`, `PUB-SEC-17`)
Do kho mã chuyển từ `private` sang `public`, GitHub giữ nguyên trạng thái tắt của các tính năng bảo mật. Chủ sở hữu cần truy cập trình duyệt và bật ngay:

1. **Tại `https://github.com/manhquydev/zalo/settings/security_analysis` (`Settings -> Code security`):**
   - Bật **Secret scanning** (`PUB-SEC-04` — **P0**).
   - Bật **Push protection** (`PUB-SEC-04` — **P0**).
   - Bật **Scan for non-provider patterns** (`PUB-SEC-04`).
   - Bật **Private vulnerability reporting** (`PUB-SEC-16` — **P1**).
   - Bật **Dependency graph**, **Dependabot alerts**, và **Dependabot security updates** (`PUB-SEC-17` — **P1**).
2. **Tại `https://github.com/manhquydev/zalo/settings/actions` (`Settings -> Actions -> General`):**
   - Mục **Fork pull request workflows from outside collaborators**: chọn **Require approval for all external contributors** (`PUB-SEC-17`).
   - Mục **Artifact and log retention**: giảm từ `90` ngày xuống **`14` ngày** (`PUB-SEC-17`).
   - Mục **Workflow permissions**: giữ nguyên **Read repository contents and packages permissions** và bỏ chọn **Allow GitHub Actions to create and approve pull requests** (đã cấu hình đúng).
   - *(Lưu ý: Khoan tích chọn `Require actions to be pinned to a full-length commit SHA` cho tới khi bản vá ghim SHA 40 ký tự ở Mục 3 được gộp lên `origin/main`, ngay sau đó bật luôn tùy chọn này).*
3. **Tại `https://github.com/manhquydev/zalo/settings/rules` (`Settings -> Rules -> Rulesets`):**
   - Tạo Ruleset `protect-main-and-tags` áp dụng cho nhánh mặc định `main` và mẫu tag `refs/tags/v*` (`PUB-SEC-03` — **P0**):
     - Tích chọn **Restrict deletions** (chặn xóa nhánh/tag).
     - Tích chọn **Block force pushes** (chặn ghi đè lịch sử).
     - Tích chọn **Require a pull request before merging** (`Required approvals = 1`, `Dismiss stale pull request approvals when new commits are pushed`).

### 2.2. Cấp quyền OAuth scope `workflow` trên máy trạm (Gỡ chặn đăng ký CI từ xa)
Chạy trong cửa sổ PowerShell tương tác của Chủ sở hữu:
```powershell
& "C:\Program Files\GitHub CLI\gh.exe" auth refresh -h github.com -s workflow
& "C:\Program Files\GitHub CLI\gh.exe" auth status
```

### 2.3. Chốt 3 Quyết định Pháp lý & Quản trị để mở khóa cho Đội Tích Hợp (`PUB-SEC-02`, `PUB-SEC-18`, `PUB-SEC-23`)
1. **Quyết định Giấy phép Mã nguồn mở (`PUB-SEC-02` — P0):** Xác nhận chọn **Dual-license `MIT OR Apache-2.0`** (`LICENSE-MIT` + `LICENSE-APACHE` đặt tại gốc repo, khớp 100% với khai báo trong toàn bộ `Cargo.toml`, kèm câu loại trừ tường minh đối với mọi tệp bằng chứng kiểm thử `**/evidence/**`).
2. **Quyết định Xử lý Thư mục Nguyên mẫu cũ `multi-zalo-desktop/` (`PUB-SEC-01`, `09`, `10`, `23`):** Khuyến nghị phê duyệt phương án **gỡ bỏ hoàn toàn `multi-zalo-desktop/` khỏi `HEAD` nhánh chính** bằng một commit tiến (`chore: retire legacy multi-zalo-desktop prototype`), qua đó đóng gọn đồng thời rủi ro mã `app.asar` (`instance-behavior.md`), tệp `MultiZaloDesktop.exe`, `7` tệp dump `rea-*.json` và `WindowsPlatformAdapter.cs` cũ.
3. **Quyết định Nhãn hiệu & Phi liên kết (`PUB-SEC-18` — P1):** Phê duyệt bổ sung tuyên bố phi liên kết song ngữ Việt/Anh đối với VNG Corporation / Zalo ở đầu `README.md` và giao diện ứng dụng, chuẩn hóa tên hiển thị sang dạng mô tả tương thích (*Nominative Fair Use*, ví dụ `Desktop Instance Manager — Multi-Account Launcher for Zalo Desktop`) và đổi `identifier` khỏi `com.multizalodesktop.manager` trước khi ký số phát hành.
4. **Vệ sinh máy trạm (`PUB-SEC-23` — P2):** Di chuyển thư mục nghiên cứu ngoài `C:\Users\<REDACTED_USER>\Downloads\zalo\rea` ra một thư mục độc lập bên ngoài kho `zalo` để tránh mọi rủi ro `git add -f` nhầm `~9.100` tệp bên thứ ba.

---

## 3. Quy Trình Hợp Nhất Gộp Sạch (`Squash Merge`) Của Release Captain Lên `origin/main`

Sau khi `Central Integration Agent` và `Build Agent` hoàn tất toàn bộ danh mục vá lỗi tại [handoff-central-integration.md](./handoff-central-integration.md) và bàn giao mã `<FINAL_CLEAN_INTEGRATION_SHA>`:

### Bước 1 — Xác minh độc lập trước khi gộp (Read-Only Gate)
1. Kiểm tra `git --no-optional-locks -C "C:/Users/<REDACTED_USER>/Downloads/zalo-integration" status --porcelain=v1` trả về rỗng (`0` tệp bẩn/untracked; lưu ý kiểm tra kỹ không commit nhầm các biên bản điều phối nội bộ đang untracked tại `zalo/desktop-instance-manager/docs/release/*.md` hay `zalo-windows-acceptance/desktop-instance-manager/docs/qa/windows/*`).
2. Xác nhận lượt chạy kiểm thử cuối cùng trên `<FINAL_CLEAN_INTEGRATION_SHA>` đạt:
   - `cargo fmt --all -- --check` & `cargo clippy --workspace --all-targets --locked -- -D warnings` (`PASS`)
   - `cargo test --workspace --locked` & `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --locked` (`PASS`)
   - `cargo deny` trên cả `4` tệp `Cargo.lock` (`PASS`, `0` advisories, `0` `GPL`/`AGPL`)
   - `xf-test-runner --mode release-regression --suite all --cycles 100` (`PASS`, `0` lỗi sản phẩm, Zalo chính trên máy nguyên vẹn 100%)
   - `node scripts/ci-cross-platform/validate-workflow.mjs` & `selftest.mjs` (`PASS` với SHA 40 ký tự)
   - Kiểm tra release build mặc định của `dim-desktop` **không** biên dịch nhánh `DIM_NODE_FIXTURE_SCRIPT` (`PUB-SEC-06`).

### Bước 2 — Tạo Nhánh Phát Hành Gộp Sạch Từ `origin/main` (`8cf4703`) — Ngăn Chặn Lộ `54` Commit Lịch Sử Cục Bộ
Thay vì `git merge` thông thường (sẽ đẩy toàn bộ `54` commit trung gian chứa `.gitconfig`/`.cargo`/OS paths và `16` ảnh PNG cũ lên GitHub), thực hiện cơ chế **gộp sạch (`squash`)**:

```powershell
# Tạo nhánh chuẩn bị PR trực tiếp từ nền công khai hiện tại origin/main (8cf4703)
git -C "<WORKTREE_TARGET>" checkout -b release/public-hardening-sync origin/main

# Gộp sạch toàn bộ cây mã đã kiểm chứng từ <FINAL_CLEAN_INTEGRATION_SHA> thành 1 commit duy nhất
git -C "<WORKTREE_TARGET>" merge --squash <FINAL_CLEAN_INTEGRATION_SHA>

# CỔNG QUÉT BẮT BUỘC TRƯỚC KHI COMMIT/PUSH:
# Xác nhận diff so với origin/main có:
# - 0 đường dẫn C:\Users\<tên_thật> hoặc file:///c:/Users/
# - 0 tệp *.exe, *.asar, *.pfx, *.key
# - 0 đoạn mã bootstrap.js / migration.js
# - Đủ LICENSE-MIT, LICENSE-APACHE, SECURITY.md, CONTRIBUTING.md, THIRD_PARTY_NOTICES.md
```

Sau khi kiểm tra sạch 100%, tạo commit phát hành sạch (khuyến nghị ký GPG/SSH `git commit -S`), đẩy nhánh `release/public-hardening-sync` lên `origin` và mở Pull Request hợp nhất vào `main`.

### Bước 3 — Khóa Cổng Sau Hợp Nhất & Dọn Nhánh Phụ Công Khai
1. Ngay khi PR được gộp vào `origin/main` (giúp `.github/workflows/ci.yml` và `cross-platform-validation.yml` được đăng ký chính thức trên nhánh mặc định):
   - Bổ sung `4` job của `ci.yml` vào **Required status checks** trong Ruleset của nhánh `main`.
   - Trong `Settings -> Actions -> General`, chuyển **Actions permissions** sang danh sách cho phép (`Allow actions created by GitHub` + `dtolnay/rust-toolchain@*`, `EmbarkStudios/cargo-deny-action@*`) và bật **Require actions to be pinned to a full-length commit SHA**.
   - Bật **CodeQL Default Setup** (`actions`, `rust`, `javascript-typescript`).
2. **Dọn nhánh phụ công khai đã lỗi thời (`PUB-SEC-01`, `05`, `07`, `12`):** Sau khi Chủ sở hữu xác nhận giao diện Account Dock đã nằm trọn vẹn trên `origin/main`, xóa nhánh từ xa `origin/feat/ui-vi-en-quiet-workspace`:
   ```powershell
   git push origin --delete feat/ui-vi-en-quiet-workspace
   ```
3. **Ghi nhận rủi ro lịch sử tồn dư (`PUB-SEC-07`):** Lưu xác nhận của Chủ sở hữu kho mã về việc chấp nhận rủi ro tồn dư mức thấp đối với tên tài khoản người dùng OS cục bộ đã tồn tại trong commit khởi tạo đầu tiên `8cf4703` (do tuân thủ nguyên tắc không viết lại lịch sử `origin/main`).

---

## 4. Kỷ Luật Đóng Gói & Nhãn Trung Thực Sau Khi Đồng Bộ Kho Công Khai

Lưu ý quan trọng khi kho mã đã chuyển sang **`PUBLIC`**:
- **Chi phí GitHub Actions trên kho Public:** Các runner tiêu chuẩn của GitHub (`ubuntu-24.04`, `windows-2022`, `macos-15`, `macos-15-intel`) được **miễn phí phút chạy (`$0/min`) cho kho Public**, tuy nhiên toàn bộ nhật ký chạy (`build logs`) và tệp đính kèm (`artifacts`) đều **hiển thị công khai cho mọi người dùng đăng nhập GitHub**. Do đó tuyệt đối không in biến môi trường hay đường dẫn cá nhân ra log CI, và giữ thời hạn lưu giữ artifact ở mức `7`–`14` ngày.
- **Không bao giờ gắn nhãn quá mức kiểm chứng (`Gate G2 / G3 / G4 / G5 / F-WIN-07`):**
  - Khi chạy Wave 1 (`ubuntu-24.04` + `windows-2022` MSVC) để xuất gói cài đặt NSIS/MSI thử nghiệm (`v0.1.0-beta.1`), bản dựng bắt buộc phải giữ nguyên nhãn **`UNSIGNED_DEVELOPMENT_ARTIFACT`** (cho tới khi có chứng chỉ OV/EV Authenticode / Azure Trusted Signing ở Gate G3) và **`INTERNAL_TEST_BUILD`** (cho tới khi Chủ sở hữu hoàn tất bài kiểm thử thủ công 2 tài khoản QR `scripts/run-interactive-l3-checklist.ps1` ở `F-WIN-07`).
  - Đối với Linux: chỉ giữ ở mức **`EXPERIMENTAL (Official VNG Zalo UNSUPPORTED)`** và ưu tiên gói `.deb` (không xuất bản `.AppImage` công khai trước khi lưu trữ đầy đủ mã nguồn thư viện `LGPL` theo `PUB-SEC-18`).
  - Đối với macOS: giữ nguyên nhãn **`NOT_VERIFIED / UNSIGNED_DEVELOPMENT_ARTIFACT`** cho tới khi kiểm thử thực tế `/Applications/Zalo.app` trên máy Mac vật lý, ghim `TeamIdentifier` 10 ký tự của VNG và hoàn tất ký/notarize Apple Developer ID (Gate G4).
