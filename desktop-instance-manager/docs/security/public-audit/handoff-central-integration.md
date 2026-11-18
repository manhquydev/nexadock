# Báo Cáo Bàn Giao Kỹ Thuật Cho Central Integration & Build Agent (`Handoff — Central Integration`)

- **Nhiệm vụ:** AGENT MISSION F — Danh mục sửa đổi mã nguồn, tài liệu, cấu hình đóng gói & CI để đóng toàn bộ phát hiện `P0`/`P1`/`P2` trước khi chốt SHA phát hành công khai
- **Người nhận:** `Central Integration Agent` (chủ sở hữu worktree `zalo-integration`, nhánh `integration/reliability-hardening` hiện tại ở `cd62c523222cb3a02051347e85313226de8b6964`) & `Build Agent`
- **Tài liệu gốc tham chiếu:** [security-findings.md](./security-findings.md), [public-release-gate.md](./public-release-gate.md), [github-actions-threat-review.md](./github-actions-threat-review.md), [evidence-exposure-review.md](./evidence-exposure-review.md), [licensing-and-compliance.md](./licensing-and-compliance.md)

---

## 1. Bối Cảnh & Phạm Vi Bàn Giao

Nhánh `integration/reliability-hardening` (`cd62c52`) hiện là nhánh hoàn chỉnh nhất trong toàn bộ dự án (đã tích hợp UI Account Dock, macOS A2, Ubuntu B2, C2 Reliability, D2 Compatibility, E2 Build, đã có `WinVerifyTrust` + `X-DIM-Session-Token` và đã thực hiện một đợt lọc tên người dùng chủ máy tại `bff0674`).

Tuy nhiên, kiểm toán độc lập toàn diện trước khi đẩy lên kho GitHub **PUBLIC** phát hiện **7 nhóm hạng mục kỹ thuật bắt buộc phải xử lý ngay trên `integration/reliability-hardening`** trước khi bàn giao `<FINAL_CLEAN_INTEGRATION_SHA>` cho `Release Captain` gộp sạch (`squash merge`) lên `origin/main`.

---

## 2. Danh Sách Hạng Mục Cần Vá Theo Thứ Tự Ưu Tiên (`Actionable Technical Checklist`)

### Hạng mục 1 (`[P1 — AppSec Blocker]` `PUB-SEC-06`): Khóa điều kiện biên dịch đối với `DIM_NODE_FIXTURE_SCRIPT` trong `apps/desktop/src-tauri/src/commands.rs`
- **Vấn đề tại `cd62c52`:** Hàm `resolve_tauri_launch_target()` (`desktop-instance-manager/apps/desktop/src-tauri/src/commands.rs` dòng `24–44`, gọi tại dòng `121`, `274`, `291`) đang được biên dịch **vô điều kiện vào cả bản Release** (`dim-desktop.exe` / `dim-desktop`). Khi biến môi trường `DIM_NODE_FIXTURE_SCRIPT` tồn tại, `get_status` báo giả `"authenticode_status": "Signed (Node.js Fixture)"` (`valid: true`) và `launch_profile` / `focus_profile` chạy `node <script_tùy_ý>` (bộ điều hợp Windows chỉ kiểm tra chữ ký của `node.exe` chứ không thể xác minh tệp `.js`), làm thủng ranh giới ghim nhà phát hành `CN/O=VNG GROUP JSC` trên bản sản phẩm.
- **Cách sửa chuẩn:**
  1. Trong `desktop-instance-manager/apps/desktop/src-tauri/Cargo.toml`, khai báo một feature kiểm thử không bật mặc định:
     ```toml
     [features]
     default = []
     e2e-fixture-hook = []
     ```
  2. Trong `desktop-instance-manager/apps/desktop/src-tauri/src/commands.rs`, bọc nhánh đọc biến môi trường `DIM_NODE_FIXTURE_SCRIPT` bên trong `#[cfg(any(debug_assertions, feature = "e2e-fixture-hook"))]`. Ở bản Release mặc định (`not(any(debug_assertions, feature = "e2e-fixture-hook"))`), `resolve_tauri_launch_target()` luôn trả về hằng số `(None, LaunchTarget::OfficialZalo { custom_install_root: None })` và hoàn toàn bỏ qua `DIM_NODE_FIXTURE_SCRIPT`.
  3. Cập nhật `desktop-instance-manager/tests/run-tauri-native-e2e.mjs` để dùng bản build có bật `--features e2e-fixture-hook` (hoặc bản `debug`), đồng thời thêm một bài kiểm tra đơn vị xác nhận bản Release mặc định không nhận `DIM_NODE_FIXTURE_SCRIPT`.

---

### Hạng mục 2 (`[P0 + P1 — Bản quyền & Nhị phân]` `PUB-SEC-01`, `05`, `09`, `10`, `23`): Gỡ mã nguồn trích xuất `app.asar`, tệp `.exe` dựng sẵn và các bản dump `rea-*.json`
- **Vấn đề tại `cd62c52`:**
  1. **`[P0]` `PUB-SEC-01`:** `multi-zalo-desktop/docs/research/instance-behavior.md` (dòng `51–83`) vẫn giữ nguyên văn `23` dòng hàm `bootstrap()` trích từ `app.asar/bootstrap.js` và `5` dòng từ `main-dist/migration.js` của VNG; `desktop-instance-manager/docs/research/compatibility/macos-findings.md` (dòng `86–92`) chứa `5` dòng mã JS tái dựng sát tên biến nén từ `main.js`.
  2. **`[P1]` `PUB-SEC-09`:** `multi-zalo-desktop/dist/MultiZaloDesktop.exe` (`59.392` B, không ký số) vẫn đang được theo dõi trong Git; `multi-zalo-desktop/src/platform/WindowsPlatformAdapter.cs` (dòng `271`) gán `info.IsValid = true` ngay cả khi chứng chỉ lỗi (`PUB-SEC-05`).
  3. **`[P1]` `PUB-SEC-10`:** `7` tệp dump cấp thấp `multi-zalo-desktop/evidence/rea-*.json` vẫn nằm trong Git, trong đó `rea-audit-release-exposure-app-asar.json` (blob `85be3d88`) vẫn lộ `5` đường dẫn `.pdb` máy trạm/GitLab Runner nội bộ của kỹ sư VNG.
- **Cách sửa chuẩn:**
  - **Phương án A *(Khuyến nghị tối ưu sau khi Chủ sở hữu xác nhận)*:** Nghỉ hưu toàn bộ thư mục nguyên mẫu giai đoạn 1 bằng `git rm -r multi-zalo-desktop` (xử lý dứt điểm cùng lúc `instance-behavior.md`, `MultiZaloDesktop.exe`, `7` tệp `rea-*.json` và `WindowsPlatformAdapter.cs` cũ). Nếu có tài liệu nào trong `desktop-instance-manager/docs/` dẫn liên kết tương đối sang `../../../multi-zalo-desktop/...` (ví dụ dòng `45` của `docs/security/threat-model-and-audit.md`), hãy cập nhật lại thành bảng giá trị tĩnh tự thân.
  - **Phương án B *(Nếu Chủ sở hữu muốn giữ mã tham khảo C# cũ)*:** (i) Xóa toàn bộ hai khối mã JS tại dòng `51–83` của `multi-zalo-desktop/docs/research/instance-behavior.md` và thay bằng đoạn văn xuôi phòng sạch mô tả thứ tự đọc biến môi trường của Electron; (ii) Chạy `git rm multi-zalo-desktop/dist/MultiZaloDesktop.exe` và `git rm multi-zalo-desktop/evidence/rea-*.json`; (iii) Sửa lỗi `WindowsPlatformAdapter.cs` dòng `265–271` để từ chối thực thi khi chứng chỉ không hợp lệ.
  - **Bắt buộc cho cả hai phương án:** Trong `desktop-instance-manager/docs/research/compatibility/macos-findings.md` (dòng `86–92`), thay khối mã `5` dòng bằng mô tả văn xuôi về hành vi `unlink` UNIX domain socket khi gặp `EADDRINUSE`; đồng thời lược bỏ các địa chỉ byte offset cụ thể và tên biến nén nội bộ của `main.js` trong `docs/research/compatibility/{windows-findings,macos-findings,evidence-quality-review}.md` (`PUB-SEC-22`).

---

### Hạng mục 3 (`[P1 — Vệ sinh `.gitignore` & PII sót]` `PUB-SEC-11`, `PUB-SEC-20`, `PUB-SEC-21`): Gia cố `.gitignore` và làm sạch triệt để đường dẫn bên thứ ba / thông tin trinh sát
1. **Mở rộng `.gitignore` gốc (`PUB-SEC-11`):** Bổ sung đầy đủ các nhóm chặn:
   - Khóa ký & chứng chỉ: `*.pem`, `*.key`, `*.pfx`, `*.p12`, `*.cer`, `*.crt`, `*.der`, `*.jks`, `*.keystore`, `*.tauri.key`, `.tauri/`
   - Không gian RE & dump: `.rea/`, `**/rea-session*.json`, `*.asar`, `*.jsc`, `*.pdb`, `*.dmp`, `*.idb`, `*.i64`
   - Nhị phân, bộ cài đặt & thư mục phân phối: `Programs/`, `Zalo*.exe`, `ZaloCall.exe`, `ZaloCap.exe`, `ZaviMeet.exe`, `zocr-host.exe`, `*.exe`, `*.dll`, `*.msi`, `*.dmg`, `*.pkg`, `*.deb`, `*.AppImage`, `*.rpm`, `dist/`
   - Dữ liệu phiên Zalo/Chromium runtime & bản sao lưu: `ZaloData/`, `MultiZaloDesktop*/`, `Profiles/`, `Partitions/`, `Local State`, `Cookies`, `Cookies-journal`, `*.ldb`, `*.sqlite`, `*.sqlite3`, `*.sqlite-wal`, `*.sqlite-shm`, `*.db`, `*.db-wal`, `*.db-shm`, `*.zip`
   - Tệp tạm: `*.bak`, `*.old`, `*.orig`, `*.tmp`, `*.swp`, `*~`, `Desktop.ini`
2. **Làm sạch đường dẫn Windows bên thứ ba & dấu vết máy trạm (`PUB-SEC-20`, `PUB-SEC-21`):**
   - Trong `desktop-instance-manager/docs/research/repository-comparison.md` (dòng `42`) và `multi-zalo-desktop/docs/research/existing-solutions.md` (nếu còn giữ), thay chuỗi tên người dùng trong đường dẫn cứng trích dẫn bằng `<hardcoded_author_user>`.
   - Trong `desktop-instance-manager/evidence/zalo-live-poc-result.json`, thay các giá trị mã băm SHA-256 cụ thể của `primary_zalo_baseline` (`config.json`, `startup.log`, `log.log`) bằng cờ boolean `primary_files_sha256_identical: true` và che mã số doanh nghiệp trong chuỗi Subject chứng thư.
   - Trong `docs/research/environment-readiness.md`, `docs/packaging/*` và `docs/reviews/*`, lược bỏ thông số Registry SAC/WDAC cụ thể của máy cá nhân, danh sách thư mục công cụ AI cục bộ và chuỗi `X-Oauth-Scopes` của `gh auth status`; đồng thời cân nhắc loại các tệp biên bản nội bộ `desktop-instance-manager/plans/reports/**` khỏi nhánh công khai `main` (`PUB-SEC-23`).
   - Bổ sung một bước kiểm tra tự động trong `selftest.mjs` / CI quét toàn bộ tệp `.md`, `.json`, `.toml`, `.yml` và đánh trượt nếu khớp mẫu `[A-Za-z]:[\/]Users[\/](?!Public|Default|Demo|<)` hoặc `file:///`.

---

### Hạng mục 4 (`[P0 + P1 — Pháp lý, Quản trị OSS & Nhãn hiệu]` `PUB-SEC-02`, `16`, `18`, `25`): Bổ sung tệp quản trị, `THIRD_PARTY_NOTICES.md`, tuyên bố phi liên kết và đồng bộ `deny.toml`
1. **Tệp giấy phép & quản trị cộng đồng (`PUB-SEC-02`, `PUB-SEC-16`):**
   - Sau khi Chủ sở hữu xác nhận giấy phép (khuyến nghị `MIT OR Apache-2.0`), thêm `LICENSE-MIT` và `LICENSE-APACHE` tại gốc kho mã, đồng thời ghi rõ trong `README.md` rằng giấy phép áp dụng cho mã nguồn phần mềm tự viết và **loại trừ** các tệp bằng chứng kiểm thử trong `evidence/`.
   - Thêm `SECURITY.md` tại gốc kho mã hướng dẫn báo cáo lỗ hổng qua **GitHub Private Vulnerability Reporting** (`https://github.com/manhquydev/zalo/security/advisories/new`, tuyệt đối không mở Issue công khai và không ghi email cá nhân chưa che chắn).
   - Thêm `CONTRIBUTING.md` quy định đóng góp theo nguyên tắc `inbound = outbound` và nghiêm cấm gửi PR chứa mã trích xuất từ `app.asar`, nhị phân/biểu tượng của VNG hoặc dữ liệu phiên đăng nhập Zalo thật.
2. **Thông báo bản quyền bên thứ ba & nghĩa vụ `MPL-2.0` / `LGPL` (`PUB-SEC-18`):**
   - Tạo `desktop-instance-manager/THIRD_PARTY_NOTICES.md` liệt kê giấy phép các crate phụ thuộc, đặc biệt đính kèm văn bản `MPL-2.0` và đường dẫn tới dạng mã nguồn (`crates.io` / repo gốc) của đủ **`7` crate `MPL-2.0`** trong `apps/desktop/src-tauri/Cargo.lock` (`cssparser@0.37.0`, `cssparser@0.38.0`, `cssparser-macros@0.7.1`, `dtoa-short@0.3.5`, `option-ext@0.2.0`, `selectors@0.38.0`, `selectors@0.41.0`) để tuân thủ **MPL-2.0 Mục 3.2(a)**, kèm hướng dẫn nghĩa vụ `LGPL-2.0+/2.1+/3.0` đối với thư viện hệ thống trên Ubuntu Linux.
   - Cấu hình `bundle.license`, `bundle.licenseFile` và `bundle.copyright` trong `desktop-instance-manager/apps/desktop/src-tauri/tauri.conf.json`; thêm `"license"` vào `fixtures/test-desktop-app/package.json` (`PUB-SEC-25`).
3. **Tuyên bố phi liên kết nhãn hiệu VNG/Zalo (`PUB-SEC-18`, `PUB-SEC-25`):**
   - Bổ sung hộp thông báo phi liên kết song ngữ Việt/Anh nổi bật ở ngay phần mở đầu của `README.md` gốc, `desktop-instance-manager/README.md`, và trong mục `Cài đặt -> Thông tin ứng dụng` (`apps/desktop/ui/i18n/vi-VN.json` & `en-US.json` + `app.js`): *"Dự án độc lập, không liên kết, không được tài trợ hay chứng thực bởi VNG Corporation hay Zalo. 'Zalo' là nhãn hiệu của VNG Corporation."*
   - Theo quyết định của Chủ sở hữu, cập nhật tên hiển thị sang cấu trúc mô tả tương thích (*`... for Zalo Desktop`*), đổi `identifier` khỏi `com.multizalodesktop.manager`, và thay huy hiệu `"Z"` / biểu tượng vuông xanh `#0057D8` bằng biểu tượng đa cửa sổ trung tính (sau đó chạy lại `visual-cdp.mjs` cập nhật ảnh chụp màn hình và `ui-contracts.test.mjs`).
4. **Độ phủ `deny.toml` & `publish = false` (`PUB-SEC-18`):**
   - Thêm `publish = false` vào toàn bộ `Cargo.toml` của các crate nội bộ (`crates/*/Cargo.toml`, `fixtures/*/Cargo.toml`, `tests/*/Cargo.toml`).
   - Bổ sung kiểm tra `cargo deny --config deny.toml --locked` đối với cả `fixtures/cross-platform/Cargo.toml` và `tests/cross-platform/Cargo.toml` trong `ci.yml`, `cross-platform-validation.yml` và `scripts/ci-cross-platform/dependency-audit.mjs`.

---

### Hạng mục 5 (`[P1 + P2 — GitHub Actions & CI Scripts]` `PUB-SEC-13`, `14`, `15`, `24`): Áp dụng Gói Vá Chuỗi Cung Ứng CI Của `@ci-gh-auditor`
Thực hiện bởi `Build Agent` (hoặc trực tiếp trên `zalo-integration`) theo đúng bản vá chi tiết tại **Mục 7.1 – 7.4 của [github-actions-threat-review.md](./github-actions-threat-review.md)**:
1. **Ghim commit SHA 40 ký tự cho toàn bộ `uses:`** trong cả `.github/workflows/ci.yml` và `.github/workflows/cross-platform-validation.yml` (`actions/checkout@11d5960a...`, `actions/setup-node@49933ea5...`, `actions/cache@0057852b...`, `actions/upload-artifact@ea165f8d...`, `actions/download-artifact@d3f86a10...`, `EmbarkStudios/cargo-deny-action@70f81531...`, `dtolnay/rust-toolchain@39008827...`).
2. **Bắt buộc cập nhật đồng bộ `desktop-instance-manager/scripts/ci-cross-platform/validate-workflow.mjs`:** Thay tập hợp chuỗi cứng `ALLOWED_USES` (dòng `23–31`, `221`) bằng hàm kiểm tra `isImmutablePinnedActionRef(ref)` yêu cầu định dạng `<owner>/<repo>@<40-hex-sha>` nằm trong `ALLOWED_ACTION_REPOS`, đồng thời quét cả `ci.yml` và chặn nội suy `${{ ... }}` bên trong khối `run:`.
3. **Gia cố `.github/workflows/ci.yml`:** Thêm `permissions: contents: read`, khối `concurrency:`, `timeout-minutes: 20` (và `10` cho `cargo-deny`), đặt `persist-credentials: false` trên mọi bước `actions/checkout`, và xóa `~/.cargo/bin/` khỏi đường dẫn `actions/cache`.
4. **Gia cố `.github/workflows/cross-platform-validation.yml`:** Đặt `persist-credentials: false` ở cả `7` job; chuyển toàn bộ `${{ inputs.bundles }}` và `${{ github.* }}` sang biến môi trường `env:` của step (`INPUT_BUNDLES`, `GITHUB_SERVER_URL`, `GITHUB_REPOSITORY`, `GITHUB_RUN_ID`); đưa đủ `4` tệp `Cargo.lock` vào biểu thức `hashFiles(...)`; và thay `npm install --global "@tauri-apps/cli@2.12.5"` bằng thư mục công cụ có `package-lock.json` đã commit + `npm ci --ignore-scripts`.
5. **Thêm `--locked` cho `6` lệnh build của C2 & thêm Dependabot (`PUB-SEC-17`, `PUB-SEC-24`):**
   - Bổ sung cờ `--locked` vào `2` lệnh `cargo build` trong mỗi tệp `desktop-instance-manager/tests/cross-platform/scripts/{run-ubuntu.sh, run-macos.sh, run-windows.ps1}` (tổng cộng `6` vị trí).
   - Tạo tệp `.github/dependabot.yml` theo dõi `github-actions` và các workspace `cargo`; cập nhật chú thích trong `docs/packaging/*.md` từ kho `private: true` sang kho `public`.

---

### Hạng mục 6 (`[P2 — Gia cố Tauri v2 & Giao diện Chẩn đoán]` `PUB-SEC-19`)
1. Tạo tệp cấu hình quyền tường minh `desktop-instance-manager/apps/desktop/src-tauri/capabilities/default.json` giới hạn quyền truy cập IPC cho duy nhất cửa sổ `"main"` (`"windows": ["main"]`, danh sách quyền `core:default` tối thiểu).
2. Trong `apps/desktop/src-tauri/src/commands.rs` (`get_status` dòng `186`) và `scripts/dim-host.ps1`, áp dụng `redact_sensitive_text` cho chuỗi hiển thị `data_dir` (hoặc che tên người dùng trên giao diện `#view-diagnostics` trong `apps/desktop/ui/app.js` dòng `1013`), và ưu tiên truyền `backup_name` tương đối thay vì đường dẫn tuyệt đối `backup_dir` khi gọi `restore_backup` để người dùng chụp màn hình báo lỗi không bị lộ đường dẫn cá nhân.

---

## 3. Tiêu Chí Nghiệm Thu Trước Khi Bàn Giao `<FINAL_CLEAN_INTEGRATION_SHA>` Cho Release Captain

Trước khi thông báo hoàn tất cho `Release Captain`, chạy xác minh trên cây làm việc sạch của `zalo-integration`:
1. `cargo fmt --all -- --check` và `cargo clippy --workspace --all-targets --locked -- -D warnings` (cùng crate `src-tauri`) đạt `exit 0`.
2. `cargo test --workspace --all-targets --locked` và `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --locked` đạt `exit 0` (xác nhận bản Release mặc định từ chối biến môi trường `DIM_NODE_FIXTURE_SCRIPT`).
3. `cargo deny --config deny.toml --locked check advisories bans licenses sources` đạt `exit 0` trên cả `4` tệp `Cargo.lock` (`Cargo.toml`, `apps/desktop/src-tauri/Cargo.toml`, `fixtures/cross-platform/Cargo.toml`, `tests/cross-platform/Cargo.toml`).
4. `node scripts/ci-cross-platform/validate-workflow.mjs`, `node scripts/ci-cross-platform/selftest.mjs`, và `node --test apps/desktop/ui/tests/i18n.test.mjs apps/desktop/ui/tests/ui-contracts.test.mjs` đều đạt `exit 0`.
5. Quét toàn bộ tệp được theo dõi tại `HEAD` xác nhận: **`0`** đường dẫn `C:\Users\<tên_thật>`, **`0`** liên kết `file:///`, **`0`** tệp `*.exe`, **`0`** tệp dump `rea-*.json`, **`0`** đoạn mã trích từ `app.asar`, và hiện diện đầy đủ `LICENSE-MIT`, `LICENSE-APACHE`, `SECURITY.md`, `CONTRIBUTING.md`, `desktop-instance-manager/THIRD_PARTY_NOTICES.md`.
