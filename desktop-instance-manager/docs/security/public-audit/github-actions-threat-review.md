# Đánh Giá Mối Đe Dọa GitHub Actions & Kiểm Toán Cấu Hình Bảo Mật Kho Mã Công Khai (`manhquydev/zalo`)

- **Nhiệm vụ:** AGENT MISSION F — Đánh giá bảo mật kho mã công khai (`https://github.com/manhquydev/zalo`)
- **Tác nhân phụ trách:** `@ci-gh-auditor` (Public GitHub Actions Threat Reviewer & GitHub Security Settings Auditor)
- **Ngày kiểm toán (UTC):** `2026-11-18`
- **Chế độ kiểm toán:** `100% READ-ONLY` (`git --no-optional-locks` trên toàn bộ worktree `C:\Users\<REDACTED_USER>\Downloads\zalo*`; chỉ gọi `GET` qua GitHub REST API; tuyệt đối không chỉnh sửa `.github/workflows/*` hoặc cấu hình GitHub trực tiếp)
- **Trạng thái hiển thị kho mã thực tế:** **`PUBLIC`** (`private: false`, `visibility: "public"`, `allow_forking: true`, chuyển trạng thái lúc `2026-11-18T01:47:24Z`)

---

## 1. Tóm Tắt Điều Hành (Executive Summary)

Kho mã `https://github.com/manhquydev/zalo` vừa chuyển từ trạng thái **Private** sang **Public** (`private: false`, `allow_forking: true`) trong khi nhiều nhánh/worktree kỹ thuật đang phát triển song song ở môi trường cục bộ. Quá trình kiểm toán độc lập trên toàn bộ lịch sử Git, `10` worktree cục bộ, `2` nhánh remote công khai (`origin/main`, `origin/feat/ui-vi-en-quiet-workspace`), toàn bộ script CI/đóng gói, và `32` điểm cuối GitHub REST API (`GET`) ghi nhận hai nhóm kết luận trọng yếu:

1. **Về bề mặt GitHub Actions & Chuỗi cung ứng CI/CD (`Section 5` — `7` phát hiện `GHA-SEC-01`..`GHA-SEC-07`):**
   - **Điểm tích cực:** Không có workflow nào sử dụng trigger nguy hiểm `pull_request_target` hoặc `workflow_run` để chạy mã từ PR của fork kèm quyền ghi hay secret. Workflow kiểm chứng đa nền tảng `.github/workflows/cross-platform-validation.yml` (trên `zalo-cross-platform-build` và `zalo-integration`) đã thiết kế tốt ở các khía cạnh: chỉ cho phép `workflow_dispatch`, khai báo `permissions: contents: read`, đặt `timeout-minutes` (`10–30` phút), đặt `retention-days: 7`, không tham chiếu `${{ secrets.* }}`, kiểm tra phần cứng runner thực (`uname -m`, `rustc -vV`), và cưỡng chế chính sách `Zero-VNG-Binary` (`UNSIGNED_DEVELOPMENT_ARTIFACT`).
   - **Các lỗ hổng & khoảng trống cần khắc phục trước khi đẩy workflow lên repo công khai:**
     - Trên `origin/main` (`8cf4703`) đang hiển thị công khai, tệp `ci.yml` nằm sai vị trí lồng `desktop-instance-manager/.github/workflows/ci.yml` nên GitHub Actions bỏ qua hoàn toàn (`0` workflow đăng ký, `0` lượt chạy CI bảo vệ repo công khai), đồng thời bản trên nhánh `main` nội bộ (`a5feb4a`) vẫn thiếu `--locked` ở các bước Cargo (`GHA-SEC-01` — **P1**).
     - Toàn bộ `uses:` trong cả `ci.yml` và `cross-platform-validation.yml` dùng tag/nhánh khả biến (`@v4`, `@v2`, `@stable`) thay vì mã băm commit SHA 40 ký tự; nghiêm trọng hơn, trình kiểm tra tĩnh `validate-workflow.mjs` đang khóa cứng danh sách chuỗi tag khả biến trong `ALLOWED_USES`, khiến CI tự báo lỗi nếu ghim SHA mà không cập nhật đồng bộ script kiểm tra (`GHA-SEC-02` — **P1**).
     - `ci.yml` thiếu `permissions: contents: read` tường minh, thiếu `persist-credentials: false`, thiếu `timeout-minutes` & `concurrency:`, và lưu cache `~/.cargo/bin/` trên nhánh mặc định (`GHA-SEC-03` — **P1**).
     - `cross-platform-validation.yml` nội suy trực tiếp `${{ inputs.bundles }}` (không bọc ngoặc kép trong `shell: pwsh` tại dòng `432`) và `${{ github.* }}` vào khối lệnh `run:` thay vì truyền qua biến môi trường `env:` (`GHA-SEC-04` — **P1**).
     - Các job đóng gói chạy `npm install --global "@tauri-apps/cli@2.12.5"` trực tiếp từ npm registry không có lockfile kiểm chứng SHA-512 (`npm ci`) và không chặn lifecycle scripts (`--ignore-scripts`) (`GHA-SEC-05` — **P1**).
     - Các script kiểm thử C2 (`run-ubuntu.sh`, `run-macos.sh`, `run-windows.ps1`) được gọi từ `cross-platform-validation.yml` bỏ sót `--locked` ở `6` lệnh `cargo build` (`GHA-SEC-06` — **P2**), và chưa tách riêng kiến trúc workflow phát hành có GitHub Environment bảo vệ trước khi nạp khóa ký code Gates G3/G4/G5 (`GHA-SEC-07` — **P2**).

2. **Về cấu hình bảo mật kho mã GitHub trực tiếp (`Section 7` — `6` phát hiện `GHSET-SEC-01`..`GHSET-SEC-06`, có `2` lỗi chặn mức `P0`):**
   - Do kho mã chuyển trực tiếp từ `private` sang `public`, GitHub **không tự động kích hoạt** các hàng rào bảo vệ mặc định. Kiểm tra API trực tiếp xác nhận:
     - **`[P0]` `GHSET-SEC-01`:** Nhánh mặc định `main` hoàn toàn **không được bảo vệ** (`protected: false`, `HTTP 404 Branch not protected`, `rulesets: []`).
     - **`[P0]` `GHSET-SEC-02`:** Toàn bộ **Secret Scanning** và **Secret Scanning Push Protection** đang bị **`DISABLED`** dù được miễn phí hoàn toàn cho kho mã công khai.
     - **`[P1]` `GHSET-SEC-03`..`05`:** **Private Vulnerability Reporting** bị **`DISABLED`** (thiếu cả `SECURITY.md`), **Dependabot Vulnerability Alerts & Security Updates** bị **`DISABLED`**, và chính sách Actions đang để `allowed_actions: "all"`, `sha_pinning_required: false`, `fork_pr_contributor_approval: "first_time_contributors"` (thay vì `"all_external_contributors"`), lưu log `90` ngày.
     - **`[P2]` `GHSET-SEC-06`:** **CodeQL / Code Scanning** ở trạng thái `not-configured` (trong khi GitHub đã nhận diện sẵn `rust`, `javascript-typescript`, `csharp`).

---

## 2. Kiểm Kê Toàn Diện `.github/`, Workflows & Scripts CI/Đóng Gói Qua Các Nhánh/Worktree

### 2.1. Ma trận kiểm kê tệp Workflow trên Remote, Nhánh nội bộ, Worktree và Lịch sử Git

| Nhánh / Worktree | Commit HEAD | Đường dẫn Workflow hiện diện | Blob SHA-1 | Được GitHub kích hoạt? | Trạng thái bảo mật tóm tắt |
| :--- | :--- | :--- | :--- | :---: | :--- |
| **`origin/main` *(ĐANG LIVE CÔNG KHAI)*** | `8cf4703` | `desktop-instance-manager/.github/workflows/ci.yml` | `e61d068083...` | **KHÔNG** *(sai thư mục gốc)* | `0` workflow chạy trên GitHub; thiếu `permissions:`, thiếu `--locked`, thiếu `Cargo.lock` trong cache key, dùng tag `@v4`/`@v2`/`@stable`. |
| **`origin/feat/ui-vi-en-quiet-workspace`** & `zalo-ui-worktree` | `0f87302` | `desktop-instance-manager/.github/workflows/ci.yml` | `e61d068083...` | **KHÔNG** *(sai thư mục gốc)* | Giống hệt `origin/main` (`8cf4703`). |
| **`main`**, `zalo-public-security-audit`, `zalo-compatibility-research`, `zalo-cross-platform-tests`, `zalo-macos-worktree`, `zalo-ubuntu-worktree` | `a5feb4a` *(và các nhánh rẽ từ `main`)* | `.github/workflows/ci.yml` | `e08bd0f17a...` | Chưa push lên `origin` | Đã dời ra gốc `.github/workflows/ci.yml`; vẫn thiếu `permissions:`, thiếu `--locked` trên `cargo check/clippy/test/build`, dùng tag khả biến, cache `~/.cargo/bin/`. |
| **`build/cross-platform-distribution`** (`zalo-cross-platform-build`) | `6c4150e` | `.github/workflows/ci.yml`<br>`.github/workflows/cross-platform-validation.yml` | `e08bd0f17a...`<br>`afc3fdad68...` | Chưa push lên `origin` | `cross-platform-validation.yml` có `permissions: contents: read`, `workflow_dispatch` only, `retention-days: 7`, `timeout-minutes`; nhưng dùng tag khả biến, nội suy `${{ inputs.bundles }}` trực tiếp, `npm install -g` không lockfile. |
| **`integration/reliability-hardening`** (`zalo-integration`) & **`qa/windows-native-acceptance`** (`zalo-windows-acceptance`) | `bff0674` / `004ab7c` | `.github/workflows/ci.yml`<br>`.github/workflows/cross-platform-validation.yml` | `eec1b57bf7...`<br>`583b45e8bd...` | Chưa push lên `origin` | Đã tích hợp `IR-007` (`5616bcb3`: thêm `--locked` vào `ci.yml`, thêm `src-tauri/Cargo.lock` vào cache key) và C2 reliability harness (`67b7447`, `8aa7a62`); tuy nhiên vẫn còn các điểm yếu `GHA-SEC-02`..`07`. |
| *(Thư mục lồng bị `.gitignore`)* `zalo/rea/.github/` | Không theo dõi bởi Git | `rea/.github/workflows/*.yml` (`35` tệp), `rea/.github/actions/*` (`4` composite actions) | *(Untracked / Ignored)* | **KHÔNG** | Đã xác minh qua `.gitignore` dòng `2` (`rea/`) và `git log --all --full-history -- "rea/*"`: **`0` commit từng theo dõi `rea/`**. Phải giữ nguyên quy tắc `rea/` trong `.gitignore` để không vô tình commit workflow của công cụ bên thứ ba. |

### 2.2. Kiểm kê các Script CI, Kiểm thử & Đóng gói liên quan

| Nhóm Script | Đường dẫn (tương đối từ gốc repo) | Được gọi bởi Workflow nào? | Đánh giá an toàn thực thi |
| :--- | :--- | :--- | :--- |
| **Provenance & SBOM cơ sở** | `desktop-instance-manager/scripts/generate-build-provenance.mjs` | `ci.yml` (`Windows`) | Dùng `execFileSync` mảng đối số an toàn (không qua shell), chạy `cargo metadata --locked`, không truy cập mạng hay secret; ghi ra `evidence/*.json` cục bộ trên runner rồi hủy cùng VM. |
| **Đóng gói & Kiểm chứng đa nền tảng** | `desktop-instance-manager/scripts/ci-cross-platform/{check-prerequisites.mjs, build-and-package.mjs, dependency-audit.mjs, inspect-artifact.mjs, generate-artifact-manifest.mjs, validate-workflow.mjs, selftest.mjs}` | `cross-platform-validation.yml` | Chỉ dùng thư viện chuẩn `node:*` (0 gói npm bên ngoài), dùng `spawnSync` tách mảng đối số, kiểm tra `Zero-VNG-Binary`, đóng nhãn trung thực `UNSIGNED_DEVELOPMENT_ARTIFACT` & `SELF_REPORTED_UNATTESTED`. Tuy nhiên `validate-workflow.mjs` (`ALLOWED_USES`) đang ép cứng tag khả biến `@v4`/`@v2`/`@stable` và `dependency-audit.mjs` mới quét `2/4` tệp `Cargo.lock`. |
| **C2 Cross-Platform Reliability Scripts** | `desktop-instance-manager/tests/cross-platform/scripts/{run-all.mjs, run-ubuntu.sh, run-macos.sh, run-windows.ps1}` | `cross-platform-validation.yml` (`4` OS build jobs) | Tách biệt môi trường tạm (`C2_TMP` / `c2-win-sandbox`), nhưng cả `3` script nền tảng (`run-ubuntu.sh`, `run-macos.sh`, `run-windows.ps1`) đều gọi `2` lệnh `cargo build` **thiếu cờ `--locked`** (`GHA-SEC-06`). |
| **Host Bridge & PoC nội bộ** | `desktop-instance-manager/scripts/{dim-host.ps1, run-tests.ps1, run-fixture-poc.ps1, run-zalo-poc.ps1, run-interactive-l3-checklist.ps1}` & `multi-zalo-desktop/scripts/*.ps1` | Gián tiếp qua `run-fixture-tests.mjs` / `run-contract-parity-tests.mjs` trên `ci.yml` (`Windows`) | Chỉ chạy với fixture giả lập (`fixture-app.mjs`) trong thư mục tạm `%TEMP%` trên CI; `run-zalo-poc.ps1` và `run-interactive-l3-checklist.ps1` không được gọi tự động trên CI công khai. |

---

## 3. Phân Tích Mối Đe Dọa Chi Tiết Từng Workflow (`Section 5`)

### 3.1. Bảng đối chiếu các bề mặt tấn công GitHub Actions trên Repo Công Khai

| Hạng mục kiểm tra mối đe dọa | `.github/workflows/ci.yml` (`main` & `zalo-integration`) | `.github/workflows/cross-platform-validation.yml` (`zalo-cross-platform-build` & `zalo-integration`) | Kết luận kiểm toán |
| :--- | :--- | :--- | :--- |
| **1. Quyền tối thiểu `GITHUB_TOKEN` (`permissions:`)** | **THIẾU khai báo `permissions:`** ở cấp workflow và cấp job (phụ thuộc ngầm định vào cấu hình mặc định của repo). | Đã khai báo tường minh `permissions: contents: read` ở cấp đầu tệp; không có quyền `write`. | **Cần vá `ci.yml` (`GHA-SEC-03`)** — luôn khai báo `permissions: contents: read` trực tiếp trong YAML. |
| **2. PR từ Fork không tin cậy (`pull_request_target`, `workflow_run`)** | Chỉ dùng `pull_request` (nhánh `main`, `master`), `push`, `workflow_dispatch`. **Không** dùng `pull_request_target` hay `workflow_run`. | Chỉ dùng `workflow_dispatch` (yêu cầu quyền `write` trên repo mới kích hoạt được). **Không** chạy trên PR từ fork. | **An toàn về mặt phân tách đặc quyền PR fork** (`0` mã PR từ fork chạy với quyền ghi hoặc secret). Tuy nhiên cần siết chính sách phê duyệt PR fork trên GitHub thành `all_external_contributors` (`GHSET-SEC-05`). |
| **3. Lưu giữ thông tin xác thực Git (`persist-credentials`)** | `actions/checkout` để mặc định (`persist-credentials: true`), lưu `GITHUB_TOKEN` trong `.git/config`. | `actions/checkout` ở cả `7` job để mặc định (`persist-credentials: true`). | **Cần vá cả 2 workflow (`GHA-SEC-03`)** — đặt `persist-credentials: false` ở mọi bước `actions/checkout`. |
| **4. Tiêm lệnh qua `${{ ... }}` (`Script / Template Injection`)** | Chỉ dùng `${{ matrix.os }}` và `${{ runner.os }}` ở trường YAML (`name:`, `runs-on:`, `key:`), không nội suy ngữ cảnh người dùng vào `run:`. | Dòng `254, 588, 744` nội suy `"${{ inputs.bundles }}"` trong `bash`; dòng `432` nội suy **không ngoặc kép** `--bundles ${{ inputs.bundles }}` trong `pwsh`; dòng `326, 502, 653, 809` nội suy `${{ github.repository }}` vào `run:`. | **Cần vá `cross-platform-validation.yml` (`GHA-SEC-04`)** — chuyển toàn bộ `${{ inputs.* }}` và `${{ github.* }}` sang khối `env:` của step. |
| **5. Ghim SHA 40 ký tự so với Tag khả biến** | Dùng `actions/checkout@v4`, `dtolnay/rust-toolchain@stable`, `actions/cache@v4`, `EmbarkStudios/cargo-deny-action@v2`, `actions/setup-node@v4`. | Dùng `7` action qua tag/nhánh khả biến (`@v4`, `@v2`, `@stable`) tại `28` bước; đồng thời `validate-workflow.mjs` ép cứng tag trong `ALLOWED_USES`. | **Cần vá cả 2 workflow + `validate-workflow.mjs` (`GHA-SEC-02`)** — ghim commit SHA 40 ký tự kèm chú thích phiên bản. |
| **6. Cài đặt phụ thuộc (`cargo --locked`, `npm ci`) & Build scripts** | Trên `main` (`a5feb4a`) thiếu `--locked` ở `7` lệnh Cargo; trên `zalo-integration` (`5616bcb3`) đã thêm `--locked`. Không cài gói npm bên ngoài. | Đã có `--locked` ở các bước chính, **nhưng**: (a) cài `@tauri-apps/cli@2.12.5` bằng `npm install --global` không lockfile & không `--ignore-scripts` (`GHA-SEC-05`); (b) script C2 `run-{ubuntu,macos,windows}` thiếu `--locked` ở `6` lệnh `cargo build` (`GHA-SEC-06`). | **Cần áp dụng `IR-007` lên `main`, chuyển Tauri CLI sang cài đặt khóa cứng `npm ci --ignore-scripts` (hoặc lockfile riêng), và thêm `--locked` cho các script C2.** |
| **7. Đầu độc Cache (`Cache Poisoning`)** | Lưu `~/.cargo/bin/` trong `actions/cache` trên cả `push` vào `main`; nếu một binary trong `~/.cargo/bin` bị nhiễm trên nhánh mặc định, các lần chạy sau sẽ tái sử dụng. | Không lưu `~/.cargo/bin/` (chỉ lưu `registry/index`, `registry/cache`, `git/db`, `target/`), nhưng khóa `hashFiles(...)` chưa tính `fixtures/cross-platform/Cargo.lock` và `tests/cross-platform/Cargo.lock`. | **Loại bỏ `~/.cargo/bin/` khỏi đường dẫn cache trong `ci.yml` (hoặc chuyển sang `Swatinem/rust-cache` ghim SHA với `cache-binaries: false`) và bổ sung đủ 4 `Cargo.lock` vào `hashFiles(...)`.** |
| **8. Rò rỉ Artifact & Thời hạn lưu giữ (`retention-days`)** | Không tải lên artifact nào (`0` bước `upload-artifact`). | Tải lên `6` artifact chỉ chứa báo cáo JSON/MD/`SHA256SUMS.txt` từ `${RUNNER_TEMP}` với `retention-days: 7` (không tải lên binary thô hay dữ liệu nhạy cảm). Tuy nhiên trên repo công khai, mọi tài khoản GitHub đăng nhập đều tải được artifact của workflow công khai. | **Thiết kế `retention-days: 7` trong `cross-platform-validation.yml` là hợp lý**; cần giảm thêm cấu hình mặc định cấp repo từ `90` ngày xuống `14` ngày (`GHSET-SEC-05`) và rà soát không bao giờ đưa biến môi trường hay đường dẫn cá nhân vào JSON báo cáo. |
| **9. Quyền phát hành Release, Khóa ký code & Lộ secret qua Log** | Không tham chiếu secret, không tạo Release. | Không tham chiếu secret (`0` `${{ secrets.* }}`), không tạo Release, đóng dấu `UNSIGNED_DEVELOPMENT_ARTIFACT`. | **Hiện tại an toàn**, nhưng bắt buộc thiết lập kiến trúc 4 tầng có GitHub Environment bảo vệ trước khi cấu hình khóa ký code Gates G3/G4/G5 (`GHA-SEC-07`). |

---

## 4. Kiến Trúc Phân Tách Kiểm Soát Bảo Mật Theo 4 Tầng Quy Trình (`4-Tier Pipeline Controls`)

Để bảo vệ kho mã công khai `manhquydev/zalo` cả ở giai đoạn hiện tại lẫn khi tiến tới phát hành bản ký số chính thức, hệ thống GitHub Actions bắt buộc tách biệt nghiêm ngặt thành **4 tầng độc lập** (tuyệt đối không gộp chung vào một tệp workflow duy nhất):

| Thuộc tính kiểm soát | Tầng 1: PR Validation (`ci.yml` — sự kiện `pull_request`) | Tầng 2: Main Branch CI (`ci.yml` — sự kiện `push` vào `main`) | Tầng 3: Manual Cross-Platform Validation (`cross-platform-validation.yml`) | Tầng 4: Production Release Build (`release.yml` — *chỉ tạo khi mở Gates G3/G4/G5*) |
| :--- | :--- | :--- | :--- | :--- |
| **Sự kiện kích hoạt (`on:`)** | `pull_request: branches: [main]` *(chặn tự động với mọi cộng tác viên ngoài qua `all_external_contributors`)* | `push: branches: [main]` | `workflow_dispatch` *(chỉ thành viên có quyền write trên nhánh mặc định `main`)* | `workflow_dispatch` với tag phát hành đã ký GPG (`v*.*.*`) + phê duyệt thủ công |
| **Độ tin cậy của mã nguồn chạy trên runner** | **KHÔNG TIN CẬY (`UNTRUSTED`)** — mã từ nhánh PR hoặc fork bên ngoài | **ĐÃ DUYỆT (`REVIEWED`)** — đã qua PR review + cổng trạng thái bắt buộc | **ĐÃ DUYỆT / KIỂM CHỨNG KỸ THUẬT** — kiểm tra đóng gói thử nghiệm không ký số | **ĐÃ KHÓA PHÁT HÀNH (`RELEASE_LOCKED`)** — commit trên nhánh bảo vệ đã qua đầy đủ Gates G0–G6 |
| **Quyền `GITHUB_TOKEN` (`permissions:`)** | `contents: read` *(mọi quyền khác = `none`; `persist-credentials: false`)* | `contents: read` *(mọi quyền khác = `none`; `persist-credentials: false`)* | `contents: read` *(tuyệt đối không cấp `contents: write` hay `id-token: write`)* | Job build/sign: `contents: read`, `id-token: write`, `attestations: write`<br>Job publish tách riêng: `contents: write` |
| **Truy cập GitHub Secrets** | **`0` SECRETS** *(GitHub tự chặn secret với PR từ fork; không gắn bất kỳ secret cấp repo nào)* | **`0` SECRETS** *(không bao giờ đặt khóa ký code ở cấp Repository Secrets)* | **`0` SECRETS** *(chỉ sinh `UNSIGNED_DEVELOPMENT_ARTIFACT`)* | **CHỈ truy cập Environment Secrets** nằm trong GitHub Environment `release-production` có `Required Reviewers` + giới hạn nhánh/tag `main` / `v*` |
| **Chính sách Cache (`actions/cache` / `rust-cache`)** | Chỉ đọc cache từ `main` hoặc ghi trong phạm vi nhánh PR cô lập; **không** cache `~/.cargo/bin/` | Ghi cache chuẩn theo mã băm của cả `4` tệp `Cargo.lock`; **không** cache `~/.cargo/bin/` | Khóa cache tách biệt riêng (`*-xplat-*`), không dùng chung tiền tố với `ci.yml` | Khuyến nghị **clean build (`no-cache` cho `target/`)** hoặc chỉ đọc cache registry crates.io đã kiểm chứng `--locked` |
| **Đầu ra Artifact & Provenance** | Không tải lên binary thực thi | Không tải lên binary phát hành; chỉ kiểm tra cổng chất lượng | Chỉ tải lên manifest/báo cáo JSON, `SHA256SUMS.txt` (`UNSIGNED_DEVELOPMENT_ARTIFACT`, `retention-days: 7`) | Ký số bên trong + bên ngoài (Authenticode / Azure Trusted Signing OIDC, Apple Developer ID + Notarization, GPG), đính kèm chứng thực OIDC SLSA v1.0 (`actions/attest-build-provenance`) |
| **Giới hạn thời gian & Đồng thời** | `timeout-minutes: 20`, `concurrency` hủy lượt chạy cũ cùng PR (`cancel-in-progress: true`) | `timeout-minutes: 20`, `cancel-in-progress: false` trên `main` | `timeout-minutes: 10–30`, macOS `10x` tắt mặc định (`default: false`) | `timeout-minutes: 45`, `concurrency: release-production` (`cancel-in-progress: false`) |

---

## 5. Kiểm Toán Cấu Hình Bảo Mật Trực Tiếp Kho Mã GitHub (`Section 7` — Read-Only `GET` API)

Toàn bộ kết quả dưới đây được thu thập trực tiếp bằng lệnh chỉ đọc `"C:\Program Files\GitHub CLI\gh.exe" api -X GET -i ...` trên `repos/manhquydev/zalo` vào ngày `2026-11-18` (token hiện tại có `X-Oauth-Scopes: gist, read:org, repo` và quyền `admin: true` trên repo). Mọi hạng mục bị giới hạn bởi phạm vi token (`403`/`404` không kết luận được trạng thái cấu hình) đều được phân loại tường minh là **`NOT_VERIFIED`**, tách biệt hoàn toàn với **`ENABLED`** và **`DISABLED`**.

### 5.1. Bảng trạng thái cấu hình bảo mật thực tế trên `repos/manhquydev/zalo`

| Hạng mục kiểm toán | Điểm cuối REST API (`GET`) | Mã HTTP & Phản hồi thực tế | Trạng thái xác nhận | Đánh giá rủi ro công khai |
| :--- | :--- | :--- | :---: | :--- |
| **Hiển thị kho mã (`Visibility`) & Forking** | `repos/manhquydev/zalo` | `200 OK` — `{"private": false, "visibility": "public", "default_branch": "main", "allow_forking": true, "has_issues": true, "license": null}` | **`PUBLIC` (`ENABLED`)** | Kho mã đã mở hoàn toàn cho Internet & cho phép fork; mọi push, PR, log Actions và lịch sử commit đều hiển thị công khai. |
| **Branch Protection trên nhánh `main`** | `repos/manhquydev/zalo/branches/main`<br>`.../branches/main/protection` | `200 OK` (`"protected": false`) & `404 Not Found` (`"Branch not protected"`) | **`DISABLED`** | **`[P0 — GHSET-SEC-01]`** Không bắt buộc PR review, không bắt buộc CI check, không chặn force-push hay xóa nhánh `main`. |
| **Repository Rulesets trên `main`** | `repos/manhquydev/zalo/rulesets`<br>`.../rules/branches/main` | `200 OK` — `[]` (`0` rulesets, `0` branch rules) | **`DISABLED`** | **`[P0 — GHSET-SEC-01]`** Không có bất kỳ ruleset thay thế nào bảo vệ nhánh mặc định hoặc tag phát hành. |
| **Secret Scanning** | `repos/manhquydev/zalo` (`security_and_analysis`) & `.../secret-scanning/alerts` | `200 OK` — `secret_scanning.status: "disabled"`; `404` — `"Secret scanning is disabled on this repository."` | **`DISABLED`** | **`[P0 — GHSET-SEC-02]`** GitHub không quét token/khóa rò rỉ trong lịch sử hoặc các nhánh được push lên repo công khai. |
| **Secret Scanning Push Protection** | `repos/manhquydev/zalo` (`security_and_analysis`) | `200 OK` — `secret_scanning_push_protection.status: "disabled"` | **`DISABLED`** | **`[P0 — GHSET-SEC-02]`** Không chặn tại cổng `git push` nếu lập trình viên hoặc AI agent vô tình commit khóa/token thật. |
| **Secret Scanning Non-Provider Patterns & Validity Checks** | `repos/manhquydev/zalo` (`security_and_analysis`) | `200 OK` — `secret_scanning_non_provider_patterns: "disabled"`, `secret_scanning_validity_checks: "disabled"` | **`DISABLED`** | Bỏ lỡ các mẫu khóa riêng tư tổng quát (PEM/JWT/generic high-confidence patterns) trên repo công khai. |
| **Private Vulnerability Reporting** | `repos/manhquydev/zalo/private-vulnerability-reporting` | `200 OK` — `{"enabled": false}` *(kèm `community/profile` thiếu `SECURITY.md`)* | **`DISABLED`** | **`[P1 — GHSET-SEC-03]`** Nhà nghiên cứu bảo mật bên ngoài không thể gửi báo cáo lỗ hổng riêng tư, dễ đăng thẳng lên Issues công khai (`has_issues: true`). |
| **Security Advisories đã công bố** | `repos/manhquydev/zalo/security-advisories` | `200 OK` — `[]` | **`ENABLED` (`0` advisory)** | Chưa có advisory nào được tạo hoặc công bố. |
| **Dependabot Vulnerability Alerts** | `repos/manhquydev/zalo/vulnerability-alerts` | `404 Not Found` — `"Vulnerability alerts are disabled."` *(theo chuẩn GitHub REST API với quyền `admin: true`, `404` nghĩa là tính năng đang tắt; `.../dependabot/alerts` trả `403` do cả tính năng tắt lẫn thiếu scope `security_events`)* | **`DISABLED`** | **`[P1 — GHSET-SEC-04]`** Không nhận cảnh báo tự động khi phát sinh CVE mới trong `Cargo.lock` hoặc `package.json`. |
| **Dependabot Security Updates** | `repos/manhquydev/zalo/automated-security-fixes` | `200 OK` — `{"enabled": false, "paused": false}` (`dependabot_security_updates: "disabled"`) | **`DISABLED`** | **`[P1 — GHSET-SEC-04]`** Không tự động mở PR vá lỗ hổng phụ thuộc; chưa có tệp `.github/dependabot.yml`. |
| **Code Scanning / CodeQL Default Setup** | `repos/manhquydev/zalo/code-scanning/default-setup`<br>`.../code-scanning/alerts` | `200 OK` — `{"state": "not-configured", "languages": ["csharp", "javascript", "javascript-typescript", "rust", "typescript"]}`; `404` (`"no analysis found"`) | **`DISABLED`** | **`[P2 — GHSET-SEC-06]`** Chưa bật quét SAST tự động CodeQL cho Rust, JS/TS, C# và GitHub Actions workflows. |
| **Quyền mặc định của `GITHUB_TOKEN` & Duyệt PR bởi Actions** | `repos/manhquydev/zalo/actions/permissions/workflow` | `200 OK` — `{"default_workflow_permissions": "read", "can_approve_pull_request_reviews": false}` | **`ENABLED` *(Cấu hình an toàn)*** | **Đạt chuẩn tốt:** `GITHUB_TOKEN` mặc định chỉ đọc (`read`) và GitHub Actions bị cấm tự phê duyệt PR. |
| **Giới hạn Actions bên thứ ba & Bắt buộc ghim SHA cấp repo** | `repos/manhquydev/zalo/actions/permissions` | `200 OK` — `{"enabled": true, "allowed_actions": "all", "sha_pinning_required": false}` | **`DISABLED` *(Chưa siết chặt)*** | **`[P1 — GHSET-SEC-05]`** Cho phép chạy bất kỳ action bên thứ ba nào trên GitHub Marketplace và chưa bật cưỡng chế `sha_pinning_required: true`. |
| **Chính sách phê duyệt chạy Actions cho PR từ Fork** | `repos/manhquydev/zalo/actions/permissions/fork-pr-contributor-approval` | `200 OK` — `{"approval_policy": "first_time_contributors"}` | **`ENABLED` *(Mức trung bình — chưa tối ưu)*** | **`[P1 — GHSET-SEC-05]`** Tài khoản ngoài sau khi có 1 PR nhỏ được merge sẽ được chạy CI tự động ở các PR sau mà không cần duyệt lại; cần nâng lên `all_external_contributors`. |
| **Thời hạn lưu giữ Artifact & Log Actions** | `repos/manhquydev/zalo/actions/permissions/artifact-and-log-retention` | `200 OK` — `{"days": 90, "maximum_allowed_days": 90}` | **`ENABLED` (`90` ngày)** | Thời gian lưu log công khai dài tối đa (`90` ngày); nên giảm xuống `14`–`30` ngày để thu hẹp cửa sổ phơi nhiễm thông tin chẩn đoán. |
| **Workflows, Runs, Artifacts, Caches, Secrets, Variables & Environments hiện có** | `.../actions/{workflows,runs,artifacts,caches,secrets,variables}` & `.../environments`, `.../releases` | Tất cả đều trả `200 OK` với `total_count: 0` / `[]` (`dependabot/secrets: 0`, `codespaces/secrets: 0`) | **Xác nhận `0` bản ghi** | **Không có rò rỉ lịch sử trong Actions logs/artifacts/caches công khai** vì chưa có lượt chạy từ xa nào diễn ra; đồng thời chưa có Environment `release-production`. |
| **Yêu cầu chữ ký DCO (`web_commit_signoff_required`)** | `repos/manhquydev/zalo` | `200 OK` — `web_commit_signoff_required: false` | **`DISABLED`** | Chưa bắt buộc sign-off khi chỉnh sửa qua giao diện web. |
| **Hạn mức chi tiêu Billing cấp tài khoản & Khả dụng thực tế của pool runner `macos-15-intel`** | Hạn chế bởi phạm vi OAuth token (`X-Oauth-Scopes: gist, read:org, repo` — thiếu scope billing cấp tài khoản và scope `workflow` để chạy thử) | Các endpoint billing cấp tài khoản trả `404`/`403`; `workflow_dispatch` chưa đăng ký trên `origin/main` | **`NOT_VERIFIED`** | Cần chủ sở hữu kiểm tra trực tiếp trên giao diện `Settings -> Billing & licensing` và xác nhận thực nghiệm pool `macos-15-intel` sau khi đăng ký workflow đã vá bảo mật. |

---

## 6. Danh Mục Phát Hiện Bảo Mật Chuẩn 9 Trường (`GHA-SEC-*` & `GHSET-SEC-*`)

### Nhóm A — Phát hiện trên GitHub Actions Workflows & Script CI/Đóng Gói (`GHA-SEC-*`)

#### `GHA-SEC-01` — Workflow CI trên `origin/main` đặt sai thư mục lồng khiến `0` cổng kiểm tra chạy thực tế trên repo công khai, và bản `ci.yml` trên nhánh `main` nội bộ còn thiếu `--locked`
- **ID:** `GHA-SEC-01`
- **Severity:** `P1`
- **Source:** `github-actions-threat-review.md` (`Section 5 — CI/Workflow Placement & Lockfile Integrity Audit`)
- **Evidence type:** `Git Tree Inspection & Live GitHub REST API (GET) Verification`
- **File/commit liên quan:** `desktop-instance-manager/.github/workflows/ci.yml` trên `origin/main` (`8cf47035f0f06bd97cc99aff80d81fb10b103af8`) và `origin/feat/ui-vi-en-quiet-workspace` (`0f873024f4ee16810b9d35cf206803f793fc55cb`); `.github/workflows/ci.yml` trên `main` (`a5feb4a520798064c1508c26fe4c4ac17a3a03cf`, dòng `40–55`, `125`, `165–172`); bản vá `5616bcb322331b66e108002e0a2793177b9de3fe` trên `integration/reliability-hardening`
- **Tác động:** Trên nhánh `origin/main` đang mở công khai, GitHub hoàn toàn bỏ qua tệp `desktop-instance-manager/.github/workflows/ci.yml` (`GET .../actions/workflows` trả về `total_count: 0`), dẫn tới không có bất kỳ cổng kiểm tra biên dịch, `clippy`, kiểm thử đơn vị hay `cargo-deny` nào tự động chạy khi có PR/push vào kho mã công khai. Ngoài ra, tệp `.github/workflows/ci.yml` trên nhánh `main` nội bộ (`a5feb4a`) và `build/cross-platform-distribution` (`6c4150e`) chưa áp dụng bản vá `IR-007`, làm các lệnh `cargo check`, `cargo clippy`, `cargo test`, `cargo build` chạy không có `--locked` và bỏ sót `apps/desktop/src-tauri/Cargo.lock` trong khóa băm cache.
- **Khuyến nghị xử lý:** Áp dụng bản vá `ci.yml` đã hợp nhất `IR-007` kèm các chốt cứng bảo mật (`permissions: contents: read`, ghim SHA 40 ký tự, `--locked` toàn phần — xem Mục 7.1), cấp quyền OAuth scope `workflow` qua phiên tương tác của chủ sở hữu và đưa tệp `.github/workflows/ci.yml` ở thư mục gốc lên `origin/main` để kích hoạt cổng kiểm tra bắt buộc cho Branch Protection.
- **Owner:** `Build Agent` (cập nhật YAML) & `Release Captain / Central Integration Agent` (hợp nhất và đẩy lên `origin/main`)
- **Trạng thái:** `PARTIALLY_MITIGATED_UNPUSHED` (đã dời đường dẫn ở `a5feb4a` và thêm `--locked` ở `5616bcb3` trên `zalo-integration`, nhưng chưa có trên `origin/main`)

---

#### `GHA-SEC-02` — Toàn bộ GitHub Actions dùng tag/nhánh khả biến (`@v4`, `@v2`, `@stable`) thay vì ghim SHA 40 ký tự bất biến, kèm khóa cứng xung đột trong `validate-workflow.mjs`
- **ID:** `GHA-SEC-02`
- **Severity:** `P1`
- **Source:** `github-actions-threat-review.md` (`Section 5 — Supply-Chain Action Pinning Audit`)
- **Evidence type:** `Workflow YAML & Static Validator Source Analysis`
- **File/commit liên quan:** `.github/workflows/ci.yml` (dòng `27, 30, 35, 62, 65, 73, 89, 92, 124, 151, 154` trên `bff0674444496bc057ba42fcc26c4c5b2d93dd7b`); `.github/workflows/cross-platform-validation.yml` (`28` bước `uses:` trên `bff0674444496bc057ba42fcc26c4c5b2d93dd7b` và `6c4150ecdb106f07aceca96098aaf3ab0d9a1326`); `desktop-instance-manager/scripts/ci-cross-platform/validate-workflow.mjs` (dòng `23–31`, `220–233`)
- **Tác động:** Tham chiếu qua tag khả biến (`@v4`, `@v2`) hoặc nhánh trượt (`dtolnay/rust-toolchain@stable`) cho phép kẻ tấn công chiếm quyền kho mã của nhà cung cấp Action bên thứ ba (tương tự sự cố chuỗi cung ứng `tj-actions/changed-files`) trỏ lại tag sang commit độc hại, qua đó thực thi mã tùy ý trên tất cả runner Ubuntu/Windows/macOS của dự án, đánh cắp `GITHUB_TOKEN`, hoặc tiêm mã độc vào gói cài đặt phát hành trong tương lai. Đặc biệt, `validate-workflow.mjs` định nghĩa tập `ALLOWED_USES` chỉ chấp nhận chuỗi `@v4`/`@v2`/`@stable`, nên nếu chỉ ghim SHA trong YAML mà không cập nhật `validate-workflow.mjs`, bước `dependency-audit` sẽ tự đánh trượt CI.
- **Khuyến nghị xử lý:** Ghim toàn bộ `uses:` sang commit SHA 40 ký tự đã xác minh trực tiếp từ GitHub API (xem bảng ánh xạ và bản vá ở Mục 7.1–7.3), đồng thời cập nhật `ALLOWED_USES` / biểu thức kiểm tra trong `validate-workflow.mjs` để yêu cầu bắt buộc định dạng `<owner>/<repo>@<40-hex-sha>` và bật `sha_pinning_required: true` trên cấu hình kho mã GitHub.
- **Owner:** `Build Agent`
- **Trạng thái:** `OPEN`

---

#### `GHA-SEC-03` — Workflow `ci.yml` thiếu `permissions: contents: read`, thiếu `persist-credentials: false`, thiếu `timeout-minutes` / `concurrency:`, và lưu cache `~/.cargo/bin/` trên nhánh mặc định
- **ID:** `GHA-SEC-03`
- **Severity:** `P1`
- **Source:** `github-actions-threat-review.md` (`Section 5 — Least Privilege, Token Persistence & Cache Isolation Audit`)
- **Evidence type:** `Workflow YAML Static Analysis`
- **File/commit liên quan:** `.github/workflows/ci.yml` (dòng `1–174` trên `bff0674444496bc057ba42fcc26c4c5b2d93dd7b` và `a5feb4a520798064c1508c26fe4c4ac17a3a03cf`); `.github/workflows/cross-platform-validation.yml` (các bước `actions/checkout` tại dòng `62, 112, 175, 355, 531, 684, 842` thiếu `persist-credentials: false`)
- **Tác động:** (1) Không khai báo `permissions: contents: read` trong `ci.yml` khiến đặc quyền của `GITHUB_TOKEN` bị lệ thuộc vào cài đặt ngoài tệp YAML; nếu cài đặt cấp tài khoản/repo bị đổi sang `write`, mọi workflow `push`/`workflow_dispatch` lập tức nhận quyền ghi. (2) Để mặc định `persist-credentials: true` khiến `actions/checkout` ghi `GITHUB_TOKEN` vào `.git/config` trên ổ đĩa runner, cho phép bất kỳ Cargo `build.rs`, proc-macro hay tiến trình kiểm thử nào đọc được token. (3) Thiếu `timeout-minutes` trên `10` job ma trận của `ci.yml` giữ mức mặc định `360` phút/job (tối đa `60` giờ runner mỗi lượt treo tiến trình giao diện hoặc lạm dụng hàng đợi trên repo công khai). (4) Lưu `~/.cargo/bin/` trong `actions/cache` trên `main` tạo nguy cơ lưu truyền tệp thực thi bị thay thế qua nhiều lượt chạy kế tiếp trên nhánh mặc định.
- **Khuyến nghị xử lý:** Bổ sung `permissions: contents: read`, khối `concurrency:` hủy lượt chạy PR trùng lặp, `timeout-minutes: 20` cho từng job trong `ci.yml`, đặt `persist-credentials: false` ở mọi bước `actions/checkout` trong cả `ci.yml` lẫn `cross-platform-validation.yml`, và xóa `~/.cargo/bin/` khỏi danh sách đường dẫn `actions/cache` trong `ci.yml` (đồng bộ với `cross-platform-validation.yml`).
- **Owner:** `Build Agent`
- **Trạng thái:** `OPEN`

---

#### `GHA-SEC-04` — Nội suy trực tiếp biểu thức `${{ inputs.bundles }}` (không bọc ngoặc kép trong PowerShell) và `${{ github.* }}` vào khối lệnh `run:` của `cross-platform-validation.yml`
- **ID:** `GHA-SEC-04`
- **Severity:** `P1`
- **Source:** `github-actions-threat-review.md` (`Section 5 — Expression & Shell Injection Audit`)
- **Evidence type:** `Workflow YAML Static Analysis`
- **File/commit liên quan:** `.github/workflows/cross-platform-validation.yml` (dòng `254, 326, 432, 502, 588, 653, 744, 809` trên `bff0674444496bc057ba42fcc26c4c5b2d93dd7b`; tương ứng trên `6c4150ecdb106f07aceca96098aaf3ab0d9a1326`)
- **Tác động:** GitHub Actions mở rộng biểu thức `${{ ... }}` dưới dạng văn bản thuần **trước khi** trình thông dịch `bash` hoặc `pwsh` phân tích cú pháp lệnh. Đặc biệt tại dòng `432` (`windows-msvc-build`, `shell: pwsh`), tham số `--bundles ${{ inputs.bundles }}` được nội suy hoàn toàn không có dấu ngoặc kép. Mặc dù hiện tại `inputs.bundles` đang dùng `type: choice` (`all` / `none`) và chỉ người có quyền ghi mới gọi được `workflow_dispatch`, nhưng bản thân `build-and-package.mjs` đã hỗ trợ chuỗi phân tách bằng dấu phẩy (`msi,nsis`); nếu ai đó đổi `inputs.bundles` sang `type: string` hoặc sao chép mẫu này sang workflow khác có đầu vào tự do, đây sẽ trở thành lỗ hổng tiêm lệnh PowerShell/Bash trực tiếp (`Command Injection`) ngay trên runner biên dịch bản phát hành.
- **Khuyến nghị xử lý:** Tuyệt đối không nội suy `${{ inputs.* }}` hoặc `${{ github.* }}` bên trong chuỗi `run:`. Chuyển toàn bộ sang biến môi trường của bước (`env: INPUT_BUNDLES: ${{ inputs.bundles }}` và dùng `"$INPUT_BUNDLES"` trong `bash` / `"$env:INPUT_BUNDLES"` trong `pwsh`, cùng `"$GITHUB_SERVER_URL/$GITHUB_REPOSITORY/actions/runs/$GITHUB_RUN_ID"` sẵn có của runner).
- **Owner:** `Build Agent`
- **Trạng thái:** `OPEN`

---

#### `GHA-SEC-05` — Cài đặt công cụ đóng gói qua `npm install --global "@tauri-apps/cli@2.12.5"` thiếu lockfile kiểm chứng SHA-512 và không chặn `postinstall` script bên thứ ba
- **ID:** `GHA-SEC-05`
- **Severity:** `P1`
- **Source:** `github-actions-threat-review.md` (`Section 5 — Dependency Installation & Bundler Supply-Chain Audit`)
- **Evidence type:** `Workflow YAML & Packaging Script Analysis`
- **File/commit liên quan:** `.github/workflows/cross-platform-validation.yml` (dòng `231–233`, `404–406`, `567–569`, `723–725` trên `bff0674444496bc057ba42fcc26c4c5b2d93dd7b`)
- **Tác động:** Dù gói gốc `@tauri-apps/cli` đã được ghim đúng phiên bản `2.12.5`, lệnh `npm install --global` vẫn phân giải động các gói phụ thuộc chuyển tiếp (transitive dependencies) theo dải semver tại thời điểm chạy mà không có `package-lock.json` kiểm chứng mã băm `integrity: sha512-...`, đồng thời tự động thực thi các lifecycle script (`preinstall`/`install`/`postinstall`) của cây phụ thuộc npm ngay trên runner biên dịch các gói `.msi`, `-setup.exe`, `.dmg`, `.deb`, `.AppImage`. Nếu một gói phụ thuộc chuyển tiếp trên npm bị tấn công chiếm tài khoản, mã độc sẽ chạy trực tiếp trong môi trường đóng gói.
- **Khuyến nghị xử lý:** Khởi tạo một thư mục công cụ đóng gói có `package.json` + `package-lock.json` được kiểm toán và commit sẵn (ví dụ `desktop-instance-manager/scripts/ci-cross-platform/tooling-tauri-cli/`), chạy `npm ci --ignore-scripts` (hoặc cài qua binary `cargo-tauri` đã khóa SHA-256 / thêm tối thiểu `--ignore-scripts` nếu gói chỉ chứa binary đóng sẵn theo nền tảng) để đảm bảo toàn vẹn mật mã SHA-512 cho toàn bộ cây phụ thuộc npm.
- **Owner:** `Build Agent`
- **Trạng thái:** `OPEN`

---

#### `GHA-SEC-06` — Các script kiểm thử độ tin cậy C2 (`run-ubuntu.sh`, `run-macos.sh`, `run-windows.ps1`) bỏ sót cờ `--locked` ở `6` lệnh `cargo build` và thiếu đồng bộ `Cargo.lock` phụ trong cache/audit
- **ID:** `GHA-SEC-06`
- **Severity:** `P2`
- **Source:** `github-actions-threat-review.md` (`Section 5 — CI Script Reproducibility & Lockfile Coverage Audit`)
- **Evidence type:** `CI/Test Script Static Analysis`
- **File/commit liên quan:** `desktop-instance-manager/tests/cross-platform/scripts/run-ubuntu.sh` (dòng `37, 43`), `run-macos.sh` (dòng `29, 35`), `run-windows.ps1` (dòng `39, 47`); `.github/workflows/cross-platform-validation.yml` (dòng `90, 127–150, 227, 399, 563, 719`); `desktop-instance-manager/scripts/ci-cross-platform/dependency-audit.mjs` (dòng `536–538`)
- **Tác động:** Trong `cross-platform-validation.yml`, cả `4` job OS đều gọi `node tests/cross-platform/scripts/run-all.mjs`, chuyển tiếp xuống `run-ubuntu.sh`, `run-macos.sh` và `run-windows.ps1`. Bên trong `3` script này, lệnh `cargo build --manifest-path fixtures/cross-platform/Cargo.toml --release` và `cargo build --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner` đều thiếu cờ `--locked`, phá vỡ cam kết "100% lệnh Cargo trong CI dùng `--locked`". Đồng thời, `fixtures/cross-platform/Cargo.toml` chưa được quét trong bước `cargo-deny` của `dependency-audit`, và `2` tệp `Cargo.lock` của C2 chưa có trong khóa băm `hashFiles(...)` của `actions/cache`.
- **Khuyến nghị xử lý:** Thêm `--locked` vào cả `6` lệnh `cargo build` trong `run-ubuntu.sh`, `run-macos.sh`, `run-windows.ps1`; bổ sung bước `cargo-deny` cho `fixtures/cross-platform/Cargo.toml` và đưa cả `4` tệp `Cargo.lock` vào biểu thức `hashFiles(...)` của `cross-platform-validation.yml`.
- **Owner:** `Build Agent` & `Central Integration Agent`
- **Trạng thái:** `OPEN`

---

#### `GHA-SEC-07` — Chưa có kiến trúc Workflow Phát hành (Tier 4) dùng GitHub Environment bảo vệ & chứng thực OIDC SLSA v1.0 trước khi nạp khóa ký số; chú thích CI vẫn ghi giả định `private: true`
- **ID:** `GHA-SEC-07`
- **Severity:** `P2`
- **Source:** `github-actions-threat-review.md` (`Section 5 — Release Signing Governance & Public Repo Transition Audit`)
- **Evidence type:** `Workflow Architecture & Packaging Documentation Review`
- **File/commit liên quan:** `.github/workflows/cross-platform-validation.yml` (dòng `1–47`); `desktop-instance-manager/docs/packaging/signing-and-release-gates.md` (dòng `114–118`); `desktop-instance-manager/docs/packaging/release-readiness.md` (dòng `36`); `desktop-instance-manager/docs/packaging/ci-execution-and-artifact-validation.md` (dòng `145–160`)
- **Tác động:** Hiện tại việc chưa có `release.yml` và chưa nạp khóa ký số là đúng đắn cho giai đoạn `UNSIGNED_DEVELOPMENT_ARTIFACT`. Tuy nhiên, nếu khi mở Gates G3/G4/G5 (Authenticode / Apple Developer ID / GPG) mà đội phát triển gắn trực tiếp khóa ký vào `cross-platform-validation.yml` hoặc đặt ở mức Repository Secrets thay vì tách riêng `release.yml` gắn với GitHub Environment có người phê duyệt bắt buộc (`Required Reviewers`), bất kỳ ai có quyền kích hoạt workflow thông thường cũng có thể tạo bản dựng mang chữ ký chính thức. Ngoài ra, tài liệu đóng gói vẫn ghi kho mã là `private: true` trong khi thực tế đã là `public` (runner tiêu chuẩn miễn phí `$0/phút`, nhưng log `90` ngày và artifact tải về được mở công khai).
- **Khuyến nghị xử lý:** Giữ `cross-platform-validation.yml` vĩnh viễn không có secret và không tạo Release; khi triển khai Gates G3/G4/G5, tạo tệp `.github/workflows/release.yml` riêng biệt gắn với GitHub Environment `release-production` (bắt buộc phê duyệt thủ công + giới hạn tag `v*`), ưu tiên ký qua OIDC không lưu khóa tĩnh (Azure Trusted Signing + `actions/attest-build-provenance`), và cập nhật tài liệu đóng gói phản ánh trạng thái kho mã `public`.
- **Owner:** `Build Agent` & `Release Captain`
- **Trạng thái:** `OPEN`

---

### Nhóm B — Phát hiện trên Cấu Hình Bảo Mật Trực Tiếp Kho Mã GitHub (`GHSET-SEC-*`)

#### `GHSET-SEC-01` — Nhánh mặc định `main` trên kho mã công khai hoàn toàn không có Branch Protection hoặc Repository Ruleset
- **ID:** `GHSET-SEC-01`
- **Severity:** `P0`
- **Source:** `github-actions-threat-review.md` (`Section 7 — Live GitHub Branch Protection & Rulesets Audit`)
- **Evidence type:** `Live GitHub REST API (GET) Verification`
- **File/commit liên quan:** `GET repos/manhquydev/zalo/branches/main` (`"protected": false`, `HEAD` công khai `8cf47035f0f06bd97cc99aff80d81fb10b103af8`); `GET repos/manhquydev/zalo/branches/main/protection` (`HTTP 404 Branch not protected`); `GET repos/manhquydev/zalo/rulesets` (`[]`); `GET repos/manhquydev/zalo/rules/branches/main` (`[]`)
- **Tác động:** Trên một kho mã đã công khai (`visibility: "public"`), việc để trống hoàn toàn cả Branch Protection lẫn Repository Rulesets cho phép: (1) push trực tiếp mã chưa qua kiểm duyệt hoặc chưa qua cổng CI/bảo mật thẳng lên `main`; (2) `git push --force` ghi đè lịch sử hoặc xóa nhánh mặc định nếu một phiên làm việc/token cục bộ gặp sự cố hoặc bị lộ; (3) hợp nhất các nhánh tính năng song song mà không có kiểm tra bắt buộc `cargo-deny`, `clippy` hay đánh giá rò rỉ dữ liệu.
- **Khuyến nghị xử lý:** Thiết lập ngay **Repository Ruleset** (hoặc Branch Protection Rule) cho nhánh `main` và mẫu tag `refs/tags/v*` trên GitHub: chặn xóa nhánh (`Restrict deletions`), chặn force-push (`Block force pushes`), bắt buộc mọi thay đổi đi qua Pull Request có ít nhất `1` lượt phê duyệt (`Require a pull request before merging` + `Dismiss stale pull request approvals when new commits are pushed`), bắt buộc lịch sử tuyến tính hoặc commit có chữ ký nếu khả thi, và bắt buộc vượt qua các cổng kiểm tra trạng thái (`rust-workspace`, `cargo-deny`, `node-and-windows-integration`, `tauri-desktop-check`) ngay sau khi `.github/workflows/ci.yml` được đưa lên `origin/main`.
- **Owner:** `Repository Owner / Release Captain`
- **Trạng thái:** `OPEN` (`CONFIRMED_DISABLED`)

---

#### `GHSET-SEC-02` — Secret Scanning và Secret Scanning Push Protection đều đang bị `DISABLED` trên kho mã công khai
- **ID:** `GHSET-SEC-02`
- **Severity:** `P0`
- **Source:** `github-actions-threat-review.md` (`Section 7 — Live GitHub Secret Scanning & Push Protection Audit`)
- **Evidence type:** `Live GitHub REST API (GET) Verification`
- **File/commit liên quan:** `GET repos/manhquydev/zalo` (`security_and_analysis`: `secret_scanning.status = "disabled"`, `secret_scanning_push_protection.status = "disabled"`, `secret_scanning_non_provider_patterns.status = "disabled"`, `secret_scanning_validity_checks.status = "disabled"`); `GET repos/manhquydev/zalo/secret-scanning/alerts` (`HTTP 404 Secret scanning is disabled on this repository`)
- **Tác động:** Vì kho mã được tạo ở chế độ `private` rồi mới chuyển sang `public`, GitHub giữ nguyên trạng thái tắt của Secret Scanning và Push Protection. Trên kho mã công khai, bất kỳ khóa API, OAuth token, chứng chỉ ký code hay thông tin phiên nào bị vô tình push lên bất kỳ nhánh nào sẽ lập tức bị các bot thu thập công khai trên GitHub ghi nhận trong vài giây và không thể thu hồi khỏi các bản sao bên ngoài mà chỉ có thể hủy/xoay vòng khóa.
- **Khuyến nghị xử lý:** Bật ngay lập tức trên giao diện `Settings -> Code security` của kho mã công khai (hoàn toàn miễn phí trên GitHub Public Repositories): (1) **Secret scanning**, (2) **Push protection** (chặn từ phía máy chủ GitHub trước khi commit chứa secret kịp ghi vào kho công khai), và (3) **Scan for non-provider patterns** để bao phủ cả khóa riêng tư định dạng PEM/generic.
- **Owner:** `Repository Owner / Release Captain`
- **Trạng thái:** `OPEN` (`CONFIRMED_DISABLED`)

---

#### `GHSET-SEC-03` — Private Vulnerability Reporting đang bị `DISABLED` và kho mã thiếu tệp `SECURITY.md` công khai
- **ID:** `GHSET-SEC-03`
- **Severity:** `P1`
- **Source:** `github-actions-threat-review.md` (`Section 7 — Vulnerability Disclosure & Advisories Audit`)
- **Evidence type:** `Live GitHub REST API (GET) Verification`
- **File/commit liên quan:** `GET repos/manhquydev/zalo/private-vulnerability-reporting` (`{"enabled": false}`); `GET repos/manhquydev/zalo/community/profile` (`health_percentage: 28`, thiếu `SECURITY.md`, trong khi `has_issues: true` trên `repos/manhquydev/zalo`)
- **Tác động:** Khi người dùng hoặc nhà nghiên cứu bảo mật phát hiện lỗ hổng trong cơ chế cô lập profile, kiểm tra chữ ký Authenticode/Developer ID, hoặc IPC của Tauri, họ không có kênh báo cáo bảo mật khép kín qua GitHub Security Advisories và buộc phải mở Issue công khai (`has_issues: true`), làm lộ chi tiết khai thác zero-day trước khi có bản vá.
- **Khuyến nghị xử lý:** Bật **Private vulnerability reporting** trong `Settings -> Code security` và bổ sung tệp `SECURITY.md` ở gốc kho mã hướng dẫn rõ quy trình báo cáo lỗ hổng qua tab `Security -> Report a vulnerability` (tuyệt đối không mở Issue công khai cho lỗi bảo mật).
- **Owner:** `Repository Owner / Release Captain` (bật tính năng GitHub) & `Central Integration Agent` (bổ sung `SECURITY.md`)
- **Trạng thái:** `OPEN` (`CONFIRMED_DISABLED`)

---

#### `GHSET-SEC-04` — Dependabot Vulnerability Alerts & Security Updates đang bị `DISABLED` và chưa có `.github/dependabot.yml`
- **ID:** `GHSET-SEC-04`
- **Severity:** `P1`
- **Source:** `github-actions-threat-review.md` (`Section 7 — Supply-Chain Dependency Monitoring Audit`)
- **Evidence type:** `Live GitHub REST API (GET) Verification & Repository Tree Check`
- **File/commit liên quan:** `GET repos/manhquydev/zalo/vulnerability-alerts` (`HTTP 404 Vulnerability alerts are disabled`); `GET repos/manhquydev/zalo/automated-security-fixes` (`{"enabled": false, "paused": false}`); thiếu `.github/dependabot.yml` ở gốc kho mã
- **Tác động:** Dù dự án có `cargo-deny` kiểm tra RustSec khi CI chạy, nếu không có commit mới kích hoạt CI thì dự án sẽ không nhận được cảnh báo tức thời khi phát sinh lỗ hổng CVE mới trong `439` crate Rust (`desktop-instance-manager/Cargo.lock`, `apps/desktop/src-tauri/Cargo.lock`, `tests/cross-platform/Cargo.lock`, `fixtures/cross-platform/Cargo.lock`) hoặc khi các GitHub Actions đã ghim SHA có bản vá bảo mật mới.
- **Khuyến nghị xử lý:** Bật **Dependency graph**, **Dependabot alerts**, và **Dependabot security updates** trong `Settings -> Code security`, đồng thời thêm tệp `.github/dependabot.yml` theo dõi cả hệ sinh thái `github-actions` (tự động cập nhật mã băm SHA 40 ký tự khi có bản vá) và `cargo` cho các thư mục chứa `Cargo.lock` (xem mẫu cấu hình ở Mục 7.4).
- **Owner:** `Repository Owner` (bật cài đặt) & `Build Agent` (thêm `.github/dependabot.yml`)
- **Trạng thái:** `OPEN` (`CONFIRMED_DISABLED`)

---

#### `GHSET-SEC-05` — Chính sách GitHub Actions cho phép mọi Action bên thứ ba (`allowed_actions: "all"`), chưa bật `sha_pinning_required`, chính sách duyệt PR fork ở mức `first_time_contributors`, và lưu log `90` ngày
- **ID:** `GHSET-SEC-05`
- **Severity:** `P1`
- **Source:** `github-actions-threat-review.md` (`Section 7 — GitHub Actions Permissions & Fork Approval Audit`)
- **Evidence type:** `Live GitHub REST API (GET) Verification`
- **File/commit liên quan:** `GET repos/manhquydev/zalo/actions/permissions` (`{"enabled": true, "allowed_actions": "all", "sha_pinning_required": false}`); `GET .../actions/permissions/fork-pr-contributor-approval` (`{"approval_policy": "first_time_contributors"}`); `GET .../actions/permissions/artifact-and-log-retention` (`{"days": 90, "maximum_allowed_days": 90}`)
- **Tác động:** (1) `allowed_actions: "all"` kết hợp `sha_pinning_required: false` không ngăn chặn việc vô tình thêm action bên thứ ba chưa kiểm duyệt hoặc chưa ghim SHA trong tương lai. (2) Chính sách `first_time_contributors` cho phép một tài khoản ngoài sau khi được merge 1 PR sửa lỗi chính tả nhỏ có thể tự động kích hoạt `ci.yml` ở tất cả các PR tiếp theo mà không cần maintainer bấm duyệt chạy workflow (dễ bị lợi dụng để chạy đào tiền ảo hoặc làm nghẽn hàng đợi `20` runner đồng thời của tài khoản miễn phí). (3) Thời gian lưu log `90` ngày duy trì bề mặt phơi nhiễm thông tin chẩn đoán quá lâu.
- **Khuyến nghị xử lý:** Trong `Settings -> Actions -> General`: (1) chuyển **Actions permissions** sang `Allow select actions and reusable workflows` (`Allow actions created by GitHub` + danh sách cho phép tường minh `dtolnay/rust-toolchain@*`, `EmbarkStudios/cargo-deny-action@*`, `Swatinem/rust-cache@*`) và tích chọn **Require actions to be pinned to a full-length commit SHA** (`sha_pinning_required: true`) ngay sau khi áp dụng bản vá Mục 7; (2) chuyển **Fork pull request workflows from outside collaborators** sang **Require approval for all external contributors** (`all_external_contributors`); (3) giảm **Artifact and log retention** từ `90` ngày xuống `14` ngày; (4) giữ nguyên `Read repository contents and packages permissions` và tắt `Allow GitHub Actions to create and approve pull requests` (đang cấu hình đúng).
- **Owner:** `Repository Owner / Release Captain`
- **Trạng thái:** `OPEN` (`CONFIRMED_SUBOPTIMAL`)

---

#### `GHSET-SEC-06` — GitHub Code Scanning / CodeQL chưa được kích hoạt trên kho mã công khai
- **ID:** `GHSET-SEC-06`
- **Severity:** `P2`
- **Source:** `github-actions-threat-review.md` (`Section 7 — Automated SAST & Workflow Security Scanning Audit`)
- **Evidence type:** `Live GitHub REST API (GET) Verification`
- **File/commit liên quan:** `GET repos/manhquydev/zalo/code-scanning/default-setup` (`{"state": "not-configured", "languages": ["csharp", "javascript", "javascript-typescript", "rust", "typescript"]}`); `GET repos/manhquydev/zalo/code-scanning/alerts` (`HTTP 404 no analysis found`)
- **Tác động:** Bỏ lỡ lớp phân tích tĩnh tự động (SAST) miễn phí của GitHub CodeQL cho mã Rust, JavaScript/TypeScript (Tauri frontend / Node scripts), C# (`multi-zalo-desktop`), và đặc biệt là bộ quy tắc `actions` của CodeQL chuyên phát hiện lỗi tiêm biểu thức workflow, lộ quyền token và checkout không an toàn trên mỗi PR.
- **Khuyến nghị xử lý:** Bật **CodeQL Default Setup** (hoặc workflow CodeQL ghim SHA với ngôn ngữ `actions`, `javascript-typescript`, `rust`) trong `Settings -> Code security -> Code scanning` sau khi đồng bộ nhánh `main`.
- **Owner:** `Repository Owner / Build Agent`
- **Trạng thái:** `OPEN` (`CONFIRMED_DISABLED`)

---

## 7. Đề Xuất Bản Vá YAML & Script Sẵn Sàng Áp Dụng Cho Build Agent (`Ready-to-Apply Patches`)

> [!IMPORTANT]
> Theo quy tắc phân quyền nghiêm ngặt của nhiệm vụ, `@ci-gh-auditor` **không trực tiếp sửa đổi** bất kỳ tệp nào ngoài báo cáo này. Toàn bộ mã băm commit SHA 40 ký tự dưới đây đã được xác minh trực tiếp qua `GET repos/<owner>/<repo>/commits/<sha>` trên GitHub API ngày `2026-11-18` để `Build Agent` áp dụng ngay mà không gặp lỗi phân giải Action.

### Bảng tra cứu Commit SHA 40 ký tự đã xác minh cho các GitHub Actions

| Action gốc | Tag / Nhánh hiện tại | Commit SHA 40 ký tự đã xác minh (`GET /commits/<sha>`) |
| :--- | :--- | :--- |
| `actions/checkout` | `@v4` | `11d5960a326750d5838078e36cf38b85af677262` |
| `actions/setup-node` | `@v4` | `49933ea5288caeca8642d1e84afbd3f7d6820020` |
| `actions/cache` | `@v4` | `0057852bfaa89a56745cba8c7296529d2fc39830` |
| `actions/upload-artifact` | `@v4` | `ea165f8d65b6e75b540449e92b4886f43607fa02` |
| `actions/download-artifact` | `@v4` | `d3f86a106a0bac45b974a628896c90dbdf5c8093` |
| `EmbarkStudios/cargo-deny-action` | `@v2` | `70f81531115945ba6eb6b941ec9d39d063438eb9` |
| `dtolnay/rust-toolchain` | `@stable` | `39008827e6b4319a349e9a42dfc90579dd25f1ca` |
| *(Tùy chọn)* `Swatinem/rust-cache` | `@v2` | `d1527c8f67ef9af357290475d4af4b0fab8f12e0` |
| *(Cho Tier 4 Release)* `actions/attest-build-provenance` | `@v2` | `e8998f949152b193b063cb0ec769d69d929409be` |

---

### 7.1. Bản vá chuẩn hóa `.github/workflows/ci.yml` (Khắc phục `GHA-SEC-01`, `GHA-SEC-02`, `GHA-SEC-03`)

Bản cấu hình hoàn chỉnh dưới đây hợp nhất `IR-007` (`--locked` toàn phần + khóa băm kép `Cargo.lock` + gói Ubuntu 24.04 `libayatana-appindicator3-dev`) đồng thời bổ sung:
1. `permissions: contents: read` tường minh ở cấp cao nhất.
2. `concurrency:` tự động hủy lượt chạy PR cũ khi có commit mới nhưng giữ nguyên các lượt chạy trên `main`.
3. `timeout-minutes: 20` (hoặc `10` cho `cargo-deny`) trên mọi job.
4. `persist-credentials: false` trên mọi bước `actions/checkout`.
5. Ghim toàn bộ `uses:` bằng commit SHA 40 ký tự bất biến.
6. Loại bỏ `~/.cargo/bin/` khỏi đường dẫn lưu cache để ngăn lưu giữ binary bị nhiễm trên nhánh mặc định.

```yaml
name: MultiZaloDesktop CI

on:
  push:
    branches: [main, master]
  pull_request:
    branches: [main, master]
  workflow_dispatch:

permissions:
  contents: read

concurrency:
  group: ci-${{ github.workflow }}-${{ github.event.pull_request.number || github.ref }}
  cancel-in-progress: ${{ github.event_name == 'pull_request' }}

env:
  CARGO_TERM_COLOR: always

defaults:
  run:
    working-directory: desktop-instance-manager

jobs:
  rust-workspace:
    name: Rust fmt, check, clippy, and unit tests (${{ matrix.os }})
    runs-on: ${{ matrix.os }}
    timeout-minutes: 20
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, windows-latest, macos-latest]
    steps:
      - name: Checkout repository
        uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262 # v4
        with:
          persist-credentials: false

      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@39008827e6b4319a349e9a42dfc90579dd25f1ca # stable
        with:
          components: rustfmt, clippy

      - name: Cache Cargo registry and target (excludes ~/.cargo/bin)
        uses: actions/cache@0057852bfaa89a56745cba8c7296529d2fc39830 # v4
        with:
          path: |
            ~/.cargo/registry/index/
            ~/.cargo/registry/cache/
            ~/.cargo/git/db/
            desktop-instance-manager/target/
          key: ${{ runner.os }}-cargo-${{ hashFiles('desktop-instance-manager/**/Cargo.toml', 'desktop-instance-manager/Cargo.lock', 'desktop-instance-manager/apps/desktop/src-tauri/Cargo.lock') }}

      - name: Check formatting
        run: cargo fmt --all -- --check

      - name: Type-check workspace
        run: cargo check --workspace --all-targets --locked

      - name: Run Clippy with warnings denied
        run: cargo clippy --workspace --all-targets --locked -- -D warnings

      - name: Run workspace unit tests
        run: cargo test --workspace --all-targets --locked

  cargo-deny:
    name: Dependency licenses & security advisories (cargo-deny)
    runs-on: ubuntu-latest
    timeout-minutes: 10
    steps:
      - name: Checkout repository
        uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262 # v4
        with:
          persist-credentials: false

      - name: Run cargo-deny check (workspace)
        uses: EmbarkStudios/cargo-deny-action@70f81531115945ba6eb6b941ec9d39d063438eb9 # v2
        with:
          command: check
          manifest-path: desktop-instance-manager/Cargo.toml
          arguments: --all-features --locked
          command-arguments: ""

      - name: Run cargo-deny check (Tauri desktop crate)
        uses: EmbarkStudios/cargo-deny-action@70f81531115945ba6eb6b941ec9d39d063438eb9 # v2
        with:
          command: check
          manifest-path: desktop-instance-manager/apps/desktop/src-tauri/Cargo.toml
          arguments: --all-features --locked
          command-arguments: ""

  node-and-windows-integration:
    name: Node.js, PowerShell & Rust parity tests (${{ matrix.os }})
    runs-on: ${{ matrix.os }}
    timeout-minutes: 20
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, windows-latest, macos-latest]
    steps:
      - name: Checkout repository
        uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262 # v4
        with:
          persist-credentials: false

      - name: Setup Node.js
        uses: actions/setup-node@49933ea5288caeca8642d1e84afbd3f7d6820020 # v4
        with:
          node-version: "22"

      - name: Verify Node script syntax
        run: |
          node --check fixtures/test-desktop-app/fixture-app.mjs
          node --check apps/desktop/ui-server.mjs
          node --check apps/desktop/ui/app.js
          node --check tests/run-fixture-tests.mjs
          node --check tests/run-contract-parity-tests.mjs
          node --check scripts/generate-build-provenance.mjs

      - name: Run cross-platform fixture smoke check
        shell: bash
        run: |
          node --input-type=module <<'EOF'
          import { execFileSync } from 'node:child_process';
          import fs from 'node:fs';
          import os from 'node:os';
          import path from 'node:path';

          const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'dim-ci-'));
          execFileSync(process.execPath, ['fixtures/test-desktop-app/fixture-app.mjs', '--stay-ms=100'], {
            env: { ...process.env, APPDATA: tmp, HOME: tmp, XDG_CONFIG_HOME: tmp },
            stdio: 'inherit'
          });
          fs.rmSync(tmp, { recursive: true, force: true });
          EOF

      - name: Install Rust for Windows parity tests
        if: runner.os == 'Windows'
        uses: dtolnay/rust-toolchain@39008827e6b4319a349e9a42dfc90579dd25f1ca # stable

      - name: Build native dim-cli for contract parity tests
        if: runner.os == 'Windows'
        run: cargo build -p dim-test-fixture --bin dim-cli --release --locked

      - name: Run Windows PowerShell + UI integration suite
        if: runner.os == 'Windows'
        run: node tests/run-fixture-tests.mjs

      - name: Run PowerShell vs Rust contract parity suite
        if: runner.os == 'Windows'
        run: node tests/run-contract-parity-tests.mjs

      - name: Generate reproducible build provenance manifest & SBOM
        if: runner.os == 'Windows'
        run: node scripts/generate-build-provenance.mjs

  tauri-desktop-check:
    name: Tauri desktop crate check (${{ matrix.os }})
    runs-on: ${{ matrix.os }}
    timeout-minutes: 25
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, windows-latest, macos-latest]
    steps:
      - name: Checkout repository
        uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262 # v4
        with:
          persist-credentials: false

      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@39008827e6b4319a349e9a42dfc90579dd25f1ca # stable
        with:
          components: clippy

      - name: Install Linux GTK/WebKitGTK dependencies
        if: runner.os == 'Linux'
        run: |
          sudo apt-get update
          sudo apt-get install -y libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev patchelf

      - name: Format-check Tauri desktop crate
        run: cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml -- --check

      - name: Check Tauri desktop crate
        run: cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml --locked

      - name: Clippy Tauri desktop crate
        run: cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --locked -- -D warnings

      - name: Test Tauri desktop crate
        run: cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --locked
```

---

### 7.2. Bản vá các khối nhạy cảm trong `.github/workflows/cross-platform-validation.yml` (Khắc phục `GHA-SEC-02`, `GHA-SEC-03`, `GHA-SEC-04`, `GHA-SEC-05`, `GHA-SEC-06`)

Áp dụng cho cả `7` job trong `.github/workflows/cross-platform-validation.yml`:

```yaml
# 1. Ghim SHA 40 ký tự + persist-credentials: false cho mọi bước Checkout (7 jobs):
      - name: Checkout repository
        uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262 # v4
        with:
          persist-credentials: false

# 2. Ghim SHA 40 ký tự cho các Action còn lại:
#    - actions/setup-node@49933ea5288caeca8642d1e84afbd3f7d6820020 # v4
#    - dtolnay/rust-toolchain@39008827e6b4319a349e9a42dfc90579dd25f1ca # stable
#    - actions/cache@0057852bfaa89a56745cba8c7296529d2fc39830 # v4
#    - EmbarkStudios/cargo-deny-action@70f81531115945ba6eb6b941ec9d39d063438eb9 # v2
#    - actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02 # v4
#    - actions/download-artifact@d3f86a106a0bac45b974a628896c90dbdf5c8093 # v4

# 3. Đưa đủ 4 tệp Cargo.lock vào khóa băm actions/cache:
          key: ${{ runner.os }}-<job-suffix>-${{ hashFiles('desktop-instance-manager/**/Cargo.toml', 'desktop-instance-manager/Cargo.lock', 'desktop-instance-manager/apps/desktop/src-tauri/Cargo.lock', 'desktop-instance-manager/fixtures/cross-platform/Cargo.lock', 'desktop-instance-manager/tests/cross-platform/Cargo.lock') }}

# 4. Cài đặt @tauri-apps/cli@2.12.5 có khóa toàn vẹn (hoặc tối thiểu chặn lifecycle scripts):
#    Khuyến nghị chuẩn: dùng thư mục tooling có package-lock.json đã commit sẵn + npm ci --ignore-scripts:
      - name: Install pinned @tauri-apps/cli@2.12.5 with lockfile integrity & scripts disabled
        if: ${{ inputs.bundles != 'none' }}
        run: |
          npm --prefix desktop-instance-manager/scripts/ci-cross-platform/tooling-tauri-cli ci --ignore-scripts
          echo "${GITHUB_WORKSPACE}/desktop-instance-manager/scripts/ci-cross-platform/tooling-tauri-cli/node_modules/.bin" >> "${GITHUB_PATH}"

# 5. Xóa bỏ hoàn toàn nội suy trực tiếp ${{ inputs.bundles }} và ${{ github.* }} trong run:
#    - Với Bash (ubuntu-2404-build, macos-arm64-build, macos-intel-build):
      - name: Compile, test, run runtime fixture & package bundles
        env:
          APPIMAGE_EXTRACT_AND_RUN: "1"
          INPUT_BUNDLES: ${{ inputs.bundles }}
        run: |
          node desktop-instance-manager/scripts/ci-cross-platform/build-and-package.mjs \
            --target "${TARGET_TRIPLE}" \
            --bundles "${INPUT_BUNDLES}" \
            --out "${OUT_DIR}/build-summary.json"

      - name: Generate Section 7 artifact manifest and SHA256SUMS
        run: |
          LOG_URL="${GITHUB_SERVER_URL}/${GITHUB_REPOSITORY}/actions/runs/${GITHUB_RUN_ID}"
          node desktop-instance-manager/scripts/ci-cross-platform/generate-artifact-manifest.mjs \
            --scan-default-targets \
            --target "${TARGET_TRIPLE}" \
            --build-summary "${OUT_DIR}/build-summary.json" \
            --os-label "${OS_LABEL}" \
            --arch "${ARCH_LABEL}" \
            --build-log-url "${LOG_URL}" \
            --out-manifest "${OUT_DIR}/artifact-manifest.json" \
            --out-sha256 "${OUT_DIR}/SHA256SUMS.txt" \
            --out-md "${OUT_DIR}/artifact-summary.md"
          cat "${OUT_DIR}/artifact-summary.md" >> "${GITHUB_STEP_SUMMARY}"

#    - Với PowerShell (windows-msvc-build, dòng 426-434 & 498-514):
      - name: Compile, test, run runtime fixture & package Windows bundles (MSI & NSIS)
        shell: pwsh
        env:
          INPUT_BUNDLES: ${{ inputs.bundles }}
        run: |
          $outDir = Join-Path $env:RUNNER_TEMP "ci-windows-msvc"
          node desktop-instance-manager/scripts/ci-cross-platform/build-and-package.mjs `
            --target x86_64-pc-windows-msvc `
            --bundles "$env:INPUT_BUNDLES" `
            --out "$outDir/build-summary.json"

      - name: Generate Windows Section 7 artifact manifest and SHA256SUMS
        shell: pwsh
        run: |
          $outDir = Join-Path $env:RUNNER_TEMP "ci-windows-msvc"
          $logUrl = "$env:GITHUB_SERVER_URL/$env:GITHUB_REPOSITORY/actions/runs/$env:GITHUB_RUN_ID"
          node desktop-instance-manager/scripts/ci-cross-platform/generate-artifact-manifest.mjs `
            --scan-default-targets `
            --target x86_64-pc-windows-msvc `
            --build-summary "$outDir/build-summary.json" `
            --os-label windows-2022 `
            --arch x86_64 `
            --build-log-url "$logUrl" `
            --out-manifest "$outDir/artifact-manifest.json" `
            --out-sha256 "$outDir/SHA256SUMS.txt" `
            --out-md "$outDir/artifact-summary.md"
          Get-Content "$outDir/artifact-summary.md" | Out-File -FilePath $env:GITHUB_STEP_SUMMARY -Append -Encoding utf8
```

---

### 7.3. Bản vá đồng bộ `validate-workflow.mjs` & các script C2 `run-*` (Bắt buộc đi kèm khi ghim SHA)

1. **Trong `desktop-instance-manager/scripts/ci-cross-platform/validate-workflow.mjs` (thay thế `ALLOWED_USES` dòng `23–31` và thêm kiểm tra chống tiêm `${{ ... }}` trong `run:`):**

```javascript
const ALLOWED_ACTION_REPOS = new Set([
  'actions/checkout',
  'actions/setup-node',
  'dtolnay/rust-toolchain',
  'actions/cache',
  'EmbarkStudios/cargo-deny-action',
  'actions/upload-artifact',
  'actions/download-artifact',
]);

function isImmutablePinnedActionRef(ref) {
  const m = String(ref).match(/^([A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+)@([0-9a-f]{40})$/);
  return Boolean(m && ALLOWED_ACTION_REPOS.has(m[1]));
}
```
*(Tại bước kiểm tra số 4 dòng `221`, đổi `!ALLOWED_USES.has(u)` thành `!isImmutablePinnedActionRef(u)`, đồng thời kiểm tra cả `ci.yml` để đảm bảo không còn bất kỳ tag khả biến `@v4`/`@v2`/`@stable` nào).*

2. **Trong `desktop-instance-manager/tests/cross-platform/scripts/{run-ubuntu.sh, run-macos.sh, run-windows.ps1}`:**
   - Thay `cargo build --manifest-path fixtures/cross-platform/Cargo.toml --release` bằng:
     `cargo build --manifest-path fixtures/cross-platform/Cargo.toml --release --locked`
   - Thay `cargo build --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner` bằng:
     `cargo build --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner --locked`

---

### 7.4. Đề xuất tệp mới `.github/dependabot.yml` (Khắc phục `GHSET-SEC-04`)

```yaml
version: 2
updates:
  - package-ecosystem: "github-actions"
    directory: "/"
    schedule:
      interval: "weekly"
    open-pull-requests-limit: 5

  - package-ecosystem: "cargo"
    directory: "/desktop-instance-manager"
    schedule:
      interval: "weekly"
    open-pull-requests-limit: 5

  - package-ecosystem: "cargo"
    directory: "/desktop-instance-manager/apps/desktop/src-tauri"
    schedule:
      interval: "weekly"
    open-pull-requests-limit: 5
```

---

### 7.5. Khuôn mẫu kiến trúc bảo vệ cho Workflow Phát hành Tương lai (`.github/workflows/release.yml` — Tier 4)

Khi dự án sẵn sàng mở **Gates G3 / G4 / G5** (ký số Authenticode, Apple Developer ID + Notarization, GPG), `Build Agent` phải tuân thủ khuôn mẫu phân tách sau (tuyệt đối không thêm khóa ký vào `ci.yml` hay `cross-platform-validation.yml`):

```yaml
name: Multi-Zalo Signed Production Release

on:
  workflow_dispatch:
    inputs:
      release_tag:
        description: "Verified signed Git tag on main (e.g. v0.2.0)"
        required: true
        type: string

permissions:
  contents: read

concurrency:
  group: production-release-${{ inputs.release_tag }}
  cancel-in-progress: false

jobs:
  build-and-sign:
    runs-on: ${{ matrix.runner }}
    timeout-minutes: 45
    # BẮT BUỘC: GitHub Environment có Required Reviewers + Deployment branch rule chỉ cho phép tag v* từ main
    environment:
      name: release-production
    permissions:
      contents: read
      id-token: write       # Dùng cho Azure Trusted Signing OIDC & SLSA v1.0 attestation
      attestations: write   # Dùng cho actions/attest-build-provenance
    steps:
      - uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262 # v4
        with:
          ref: ${{ inputs.release_tag }}
          persist-credentials: false
      # ... Build --locked, ký số bên trong + bên ngoài, kiểm tra Zero-VNG-Binary ...
      - name: Generate cryptographic OIDC SLSA v1.0 build provenance attestation
        uses: actions/attest-build-provenance@e8998f949152b193b063cb0ec769d69d929409be # v2
        with:
          subject-path: "${{ runner.temp }}/signed-release-bundles/*"
```

---

## 8. Lộ Trình Khắc Phục Theo Thứ Tự Ưu Tiên Cho Chủ Sở Hữu Kho Mã & Đội Tích Hợp

### Bước Khẩn Cấp Ngay Lập Tức (`P0` — Chặn rủi ro trên Repo đã mở Công Khai)
1. **Bật Secret Scanning & Push Protection (`GHSET-SEC-02`):** Truy cập `https://github.com/manhquydev/zalo/settings/security_analysis` -> Bật **Secret scanning**, **Push protection**, và **Non-provider patterns**.
2. **Thiết lập Ruleset bảo vệ nhánh `main` (`GHSET-SEC-01`):** Truy cập `https://github.com/manhquydev/zalo/settings/rules` -> Tạo Ruleset cho nhánh mặc định `main` và tag `v*`: bật **Restrict deletions**, **Block force pushes**, **Require a pull request before merging** (`>= 1` approval, dismiss stale approvals).

### Bước Trước Khi Chấp Nhận PR Ngoài Hoặc Đẩy Workflow Lên `origin/main` (`P1`)
3. **Bật Private Vulnerability Reporting & Thêm `SECURITY.md` (`GHSET-SEC-03`):** Bật trong `Settings -> Code security` và commit `SECURITY.md` lên `main`.
4. **Siết chính sách GitHub Actions & PR từ Fork (`GHSET-SEC-05`):** Trong `Settings -> Actions -> General`:
   - Đặt **Fork pull request workflows from outside collaborators** = **Require approval for all external contributors**.
   - Giảm **Artifact and log retention** từ `90` ngày xuống `14` ngày.
   - Giới hạn **Actions permissions** sang danh sách Action được phép và bật **Require actions to be pinned to a full-length commit SHA** ngay sau khi hoàn tất Bước 5.
5. **Áp dụng gói bản vá của Build Agent (`GHA-SEC-01`..`05`) & Đưa `ci.yml` ra gốc trên `origin/main`:**
   - `Build Agent` áp dụng các bản vá ở Mục 7.1–7.4 (`ci.yml`, `cross-platform-validation.yml`, `validate-workflow.mjs`, `.github/dependabot.yml`).
   - Chủ sở hữu cấp scope `workflow` qua thiết bị đầu cuối tương tác (`gh auth refresh -h github.com -s workflow`), hợp nhất lên `origin/main`, và thêm `4` job của `ci.yml` vào danh sách **Required status checks** của Ruleset nhánh `main`.
6. **Bật Dependabot Alerts & Security Updates (`GHSET-SEC-04`):** Kích hoạt trong `Settings -> Code security`.

### Bước Gia Cố Chiều Sâu & Chuẩn Bị Cổng Phát Hành (`P2`)
7. **Bật CodeQL Default Setup (`GHSET-SEC-06`):** Quét tự động `actions`, `rust`, `javascript-typescript`.
8. **Bổ sung `--locked` cho các script C2 (`GHA-SEC-06`) & Thiết lập Environment `release-production` (`GHA-SEC-07`):** Chỉ cấu hình khóa ký code trong Environment `release-production` có phê duyệt thủ công khi mở Gates G3/G4/G5.
