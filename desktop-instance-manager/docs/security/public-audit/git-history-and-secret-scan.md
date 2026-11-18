# Báo Cáo Kiểm Toán Lịch Sử Git, Toàn Bộ Nhánh, Worktree & Quét Bí Mật / PII (`Sections 2 & 4`)

- **Dự án kiểm toán:** `https://github.com/manhquydev/zalo` (`AGENT MISSION F`)
- **Phạm vi kiểm toán:** Toàn bộ kho đối tượng Git (`66` commits, `635` trees, `660` blobs = `16.736.930` bytes), toàn bộ `2` nhánh đã công khai trên `origin`, `10` nhánh cục bộ (`refs/heads/*`), `3` commit / `5` blob mồ côi (`unreachable`), và `10` cây làm việc (`worktrees`) đang tồn tại trong thư mục làm việc cục bộ.
- **Chế độ thực thi:** `100% READ-ONLY` (`git --no-optional-locks`) trên kho gốc và mọi worktree anh em; tuân thủ nguyên tắc **Zero Secret/PII Leak** (mọi giá trị nhạy cảm, email, tên máy và đường dẫn người dùng cục bộ đều được che mặt nạ `<REDACTED_SECRET>`, `<REDACTED_EMAIL>`, `<REDACTED_HOST>`, `C:\Users\<REDACTED_USER>\...`).
- **Kết luận tổng quan cho Gate công khai:**
  - **Bí mật xác thực & Phiên Zalo (`P0` Credential/Session):** **`0` phát hiện** (`PASS` về mặt không lộ khóa bí mật thật, token GitHub/Cloud, khóa ký Tauri/Authenticode/Apple, chứng chỉ cá nhân, cookie/token phiên Zalo `zpw_*`, `imei`, UID, số điện thoại thực, hoặc cơ sở dữ liệu LevelDB/SQLite). Không có thông tin xác thực thực nào cần thu hồi khẩn cấp (`revoke/rotate`).
  - **Phơi nhiễm PII đường dẫn hệ điều hành, tệp thực thi nhị phân & vệ sinh Pre-Push (`P1` / `P2`):** **`CHANGES_REQUESTED`** (`5` phát hiện `P1`, `3` phát hiện `P2` — mã `GIT-SEC-001` đến `GIT-SEC-008`). Cần làm sạch các tệp tại `HEAD`, loại bỏ tệp thực thi `.exe` biên dịch sẵn khỏi cây nguồn, bổ sung `.gitignore`, và hợp nhất theo cơ chế gộp sạch (`squash`) để không đẩy `54` commit lịch sử cục bộ chứa đường dẫn máy trạm lên GitHub công khai.

---

## 1. Phương Pháp Luận Quét, Công Cụ Trên Máy Chủ & Giới Hạn Kỹ Thuật

### 1.1. Kiểm tra công cụ quét bí mật chuyên dụng trên đĩa (`Host Scanner Availability`)

Đã kiểm tra toàn bộ `PATH`, `Program Files`, `ProgramData`, `WinGet Packages`, `Scoop`, và `.cargo/bin` ở chế độ chỉ đọc:

| Công cụ | Trạng thái trên máy chủ | Ghi chú xác minh |
| :--- | :---: | :--- |
| `gitleaks` | **`NOT_FOUND`** | Không được cài đặt trong `PATH` hoặc `WinGet`/`Scoop` |
| `git-secrets` | **`NOT_FOUND`** | Không được cài đặt |
| `trufflehog` | **`NOT_FOUND`** | Không được cài đặt |
| `detect-secrets` / `semgrep` | **`NOT_FOUND`** | Không được cài đặt |
| `python.exe` | **`STUB_ONLY`** | Chỉ tồn tại alias 0-byte của Microsoft Store tại `C:\Users\<REDACTED_USER>\AppData\Local\Microsoft\WindowsApps\python.exe` |
| `node.exe` + `git.exe` | **`AVAILABLE`** | `Node.js v26.13.1` + `MinGit` (gọi kèm cờ `--no-optional-locks`) được dùng để chạy bộ quét tất định Zero-Leak ngoài kho Git |

### 1.2. Kiến trúc bộ quét tất định 4 lớp (`Deterministic Regex + Shannon Entropy + Magic-Byte Scanner`)

Do các công cụ CLI bên thứ ba (`gitleaks`, `git-secrets`, `trufflehog`) không có sẵn trên máy trạm và `python.exe` chỉ là stub 0-byte, kiểm toán viên đã xây dựng cả kịch bản tham chiếu Python và bộ quét thực thi trực tiếp trên `Node.js v26.13.1` trong vùng nhớ tạm biệt lập (tuyệt đối nằm ngoài mọi kho Git), gồm 4 lớp:

1. **Lớp 1 — Quét vét cạn 100% kho đối tượng Git (`--batch-all-objects`):**
   - Đọc trực tiếp luồng nhị phân từ `git --no-optional-locks cat-file --batch-all-objects` trên toàn bộ **`66` commit**, **`635` tree**, và **`660` blob** (`16.736.930` bytes) không lấy mẫu (`0%` bỏ sót), bao phủ cả các tệp đã xóa (`--diff-filter=D`), các phiên bản tệp đã bị ghi đè (`--diff-filter=M`), và các đối tượng mồ côi (`unreachable/dangling`) trong `.git/objects`.
2. **Lớp 2 — Nhận diện chữ ký tệp nhị phân (`Magic Header`) & trích xuất chuỗi đa mã hóa:**
   - Kiểm tra byte đầu của mọi blob và tệp trên đĩa để phát hiện thực thi Windows PE (`MZ` + `PE\0\0`), Linux ELF (`\x7fELF`), macOS Mach-O (`0xfeedfacf`/`0xcffaedfe`), cơ sở dữ liệu `SQLite format 3\0`, gói Electron `ASAR`, kho nén (`ZIP`/`GZIP`/`7z`), và ảnh `PNG`.
   - Đối với tệp nhị phân (như `.exe`, `.ico`, `.icns`, chunk văn bản trong `.png`), trích xuất toàn bộ chuỗi ký tự in được cả chuẩn **ASCII** lẫn **UTF-16LE** (độ dài `>= 6` ký tự) để đưa vào bộ lọc biểu thức chính quy và entropy.
3. **Lớp 3 — Bộ quy tắc Biểu thức chính quy (`22` nhóm mẫu) + Phân loại ngữ cảnh + Entropy Shannon:**
   - **Khóa đám mây, API & VCS:** GitHub PAT (`ghp_`, `gho_`, `ghu_`, `ghs_`, `ghr_`, `github_pat_`), AWS (`AKIA`/`ASIA`), Google Cloud (`AIza`, `GOCSPX-`, `"type": "service_account"`), OpenAI/Anthropic/Slack/npm/PyPI/Crates.io token, JWT (`eyJ...`), URL nhúng thông tin đăng nhập (`https://user:pass@...`), header `Authorization: Bearer/Basic`, phép gán khóa cấu hình (`api_key`, `client_secret`, `signing_key`, `password`, `private_key`...).
   - **Khóa mật mã & Ký số ứng dụng:** Khóa riêng PEM/PKCS/OpenSSH/PGP (`-----BEGIN ... PRIVATE KEY-----`), khóa ký Tauri/Minisign (`untrusted comment: minisign ... secret key`, `TAURI_SIGNING_PRIVATE_KEY`), tệp `.pfx`/`.p12`/`.cer`/`.jks`/`.keystore`.
   - **Dữ liệu tài khoản & phiên Zalo:** Tham số/cookie phiên (`zpw_sek`, `zpw_enk`, `zpsid`, `zpsess`, `zlogin`, `__zi`), định danh thiết bị `imei`, UID người dùng, cơ sở dữ liệu Chromium/Electron (`Cookies`, `Local Storage/leveldb`, `IndexedDB`, `Network Persistent State`), số điện thoại Việt Nam/quốc tế.
   - **PII & Định danh máy trạm:** Đường dẫn thư mục người dùng cục bộ trên Windows (`C:\Users\<REDACTED_USER>\...`), macOS (`/Users/<REDACTED_USER>/...`), Linux (`/home/<REDACTED_USER>/...` — tự động tách biệt tài khoản giả lập kiểm thử như `alice`, `demo`, `ubuntu`, `Public` với tên người dùng thực), địa chỉ email, Windows Domain SID (`S-1-5-21-...`), `MachineGuid` / `IOPlatformUUID` / `ComputerName` / hostname máy cục bộ (`<REDACTED_HOST>`).
   - **Phân tích Entropy Shannon:** Tính toán $H(X) = -\sum p_i \log_2 p_i$ trên mọi chuỗi ký tự hỗn hợp chữ hoa/thường/số có độ dài `28..180` ký tự ngoài lockfile; gắn cờ mọi chuỗi có $H \ge 4.45\text{ bits/char}$ để hậu kiểm ngữ cảnh mà không bao giờ in giá trị gốc.
4. **Lớp 4 — Kiểm toán trực tiếp toàn bộ `10` Worktree trước khi Push:**
   - Đối chiếu `git status --porcelain=v1 -uall` và `--ignored=matching`, đồng thời duyệt cây thư mục thực tế trên tất cả `10` worktree (bỏ qua nội dung bên trong `target/` và `node_modules/` theo phạm vi quy định) để quét toàn bộ tệp **đã sửa đổi (`M`)**, **chưa theo dõi (`??`)**, và **tệp bị ignore nằm ngoài `target/` & `node_modules/`** (bao gồm `rea/`), kết hợp kiểm toán toàn bộ các tệp `.gitignore`.

### 1.3. Giới hạn kỹ thuật được ghi nhận minh bạch (`Scanner Limits`)

1. **Không gọi API xác thực ngoài mạng đối với khóa nghi vấn:** Phân tích tĩnh bằng regex + entropy không tự kết nối tới máy chủ bên thứ ba để kiểm tra trạng thái sống của khóa (không phát sinh nhu cầu này vì thực tế xác nhận `0` khóa thực).
2. **Nội dung điểm ảnh (`Pixel/OCR`) bên trong ảnh chụp màn hình PNG:** Bộ quét nhị phân đã quét toàn bộ chunk metadata/chuỗi văn bản nhúng trong `35` blob ảnh `.png`, nhưng không thực hiện nhận dạng chữ quang học trên vùng điểm ảnh hiển thị giao diện; việc kiểm tra trực quan ảnh chụp màn hình thuộc phạm vi đối chiếu chéo của báo cáo `evidence-exposure-review.md`.
3. **Bỏ qua nội dung sinh tự động trong `target/` và `node_modules/`:** Tuân thủ đúng phạm vi kiểm toán, nội dung biên dịch bên trong `target/` và `node_modules/` không được quét sâu từng tệp (nhưng trạng thái ignore của chúng đã được xác minh qua `git status --ignored=matching`).

---

## 2. Kiểm Toán Toàn Bộ Lịch Sử Git: Phân Tách ĐÃ PUBLIC (`origin/*`) Và CHƯA PUSH (`Local Refs`)

### 2.1. Bảng tổng hợp Topology các nhánh Đã Public và Cục bộ

Xác minh trực tiếp qua `git ls-remote --heads --tags origin` và `git for-each-ref` trên `https://github.com/manhquydev/zalo.git` (`0` Git tag, `0` stash):

| Nhánh (`Ref`) | Trạng thái công khai | HEAD SHA | Tổng số Commit | Số Commit chưa lên `origin/*` | Số tệp Tracked tại `HEAD` | Số tệp tại `HEAD` còn chứa Username OS chủ máy | Tệp `.exe` tại `HEAD` |
| :--- | :---: | :---: | :---: | :---: | :---: | :---: | :---: |
| `origin/main` | **ĐÃ PUBLIC** | `8cf4703` | `1` | `0` | `106` | **`17` tệp** (`~192` vị trí) | `1` (`MultiZaloDesktop.exe`) |
| `origin/feat/ui-vi-en-quiet-workspace` | **ĐÃ PUBLIC** | `0f87302` | `9` | `0` | `138` | **`19` tệp** (`~194` vị trí) | `1` (`MultiZaloDesktop.exe`) |
| `feat/ui-vi-en-quiet-workspace` | Đồng bộ với `origin` | `0f87302` | `9` | `0` *(ahead `8` so với `origin/main`)* | `138` | **`19` tệp** | `1` (`MultiZaloDesktop.exe`) |
| `main` | **CHƯA PUSH (`ahead 1`)** | `a5feb4a` | `2` | `1` | `122` | **`12` tệp** *(đã xóa ở `5` tệp `desktop-instance-manager/`, còn nguyên ở `multi-zalo-desktop/`)* | `1` (`MultiZaloDesktop.exe`) |
| `research/desktop-compatibility` | **CHƯA PUSH** | `0e5d44c` | `4` | `3` | `131` | **`12` tệp** | `1` (`MultiZaloDesktop.exe`) |
| `test/cross-platform-reliability` | **CHƯA PUSH** | `5e802e0` | `4` | `3` | `161` | **`12` tệp** | `1` (`MultiZaloDesktop.exe`) |
| `feat/macos-multi-instance` | **CHƯA PUSH** | `f2b8f9f` | `5` | `4` | `138` | **`14` tệp** *(thêm `2` báo cáo macOS)* | `1` (`MultiZaloDesktop.exe`) |
| `build/cross-platform-distribution` | **CHƯA PUSH** | `6c4150e` | `7` | `6` | `154` | **`19` tệp** *(thêm `7` báo cáo build/CI chứa đường dẫn `.gitconfig`, `.cargo`)* | `1` (`MultiZaloDesktop.exe`) |
| `feat/ubuntu-multi-instance` | **CHƯA PUSH** | `62dc5d1` | `10` | `9` | `138` | **`12` tệp** | `1` (`MultiZaloDesktop.exe`) |
| `qa/windows-native-acceptance` *(mới phát hiện)* | **CHƯA PUSH** | `004ab7c` | `61` | `52` | `281` | **`24` tệp** *(nhánh tách trước commit làm sạch `bff0674`)* | `1` (`MultiZaloDesktop.exe`) |
| `integration/reliability-hardening` | **CHƯA PUSH** | `cd62c52` | `63` | `54` | `283` | **`0` tệp chủ máy** *(nhưng còn `3` tệp chứa đường dẫn Windows bên thứ ba & `54` commit lịch sử chưa gộp)* | `1` (`MultiZaloDesktop.exe`) |
| `audit/public-repository-security` | Nhánh kiểm toán | `a5feb4a` | `2` | `1` | `122` | **`12` tệp** *(kế thừa từ `main` @ `a5feb4a`)* | `1` (`MultiZaloDesktop.exe`) |
| *Đối tượng mồ côi (`unreachable`)* | Chỉ nằm cục bộ `.git/objects` | `0806ffb`, `ab04901`, `bbdef15` | `3` | `3` *(không thuộc nhánh nào)* | — | `2`/`5` blob mồ côi chứa đường dẫn chủ máy | — |

**Phân rã kho Blob (`.git/objects` tổng cộng `660` blobs):**
- **`229` blobs ĐÃ PUBLIC** trên `origin/main` và `origin/feat/ui-vi-en-quiet-workspace` — bất kỳ ai clone kho công khai hiện tại đều tải về toàn bộ `229` blob này.
- **`426` blobs CHƯA PUSH** nằm trên các nhánh phát triển/tích hợp cục bộ — **chưa lộ ra công chúng**, có thể ngăn chặn hoàn toàn việc phát tán lịch sử trung gian nếu hợp nhất bằng cơ chế gộp sạch (`squash`) thay vì đẩy nguyên chuỗi `54` commit lịch sử.
- **`5` blobs mồ côi (`unreachable`)** gắn với `3` commit bị thay thế (`0806ffb`, `ab04901`, `bbdef15`) trong quá trình amend/reset cục bộ; mặc định `git push` sẽ không gửi các đối tượng này lên remote.

### 2.2. Tệp đã xóa nhưng vẫn còn lưu trong lịch sử Git (`--diff-filter=D`) & Tệp bị ghi đè (`--diff-filter=M`)

Quét toàn bộ các cặp `parent -> commit` trên cả `66` commit ghi nhận **`19` sự kiện xóa tệp (`D`)**:

1. **Đã Public trên `origin/feat/ui-vi-en-quiet-workspace` (commit `cc74584`, xóa từ `b1446d6`):**
   - **`16` tệp ảnh chụp màn hình giao diện đời đầu** (`desktop-instance-manager/docs/design/screenshots/backups-vi-light-1160x780.png`, `diagnostics-vi-light-1160x780.png`, `dialog-create-vi-light.png`, `dialog-delete-vi-light.png`, `dialog-details-vi-light.png`, `overflow-menu-vi-light.png`, `profiles-en-US-dark-1160x780.png`, `profiles-en-US-light-1160x780.png`, `profiles-vi-VN-dark-1160x780.png`, `profiles-vi-VN-light-1160x780.png`, `profiles-vi-VN-light-420x540.png`, `profiles-vi-VN-light-768x650.png`, `profiles-vi-VN-light-900x650.png`, `settings-vi-light-1160x780.png`, `state-error-banner-vi-light.png`, `zoom-200-vi-light.png` — tổng dung lượng `~828 KB`).
   - Dù không còn xuất hiện ở `HEAD` (`0f87302`), cả `16` blob PNG này vẫn nằm vĩnh viễn trong lịch sử nhánh công khai `origin/feat/ui-vi-en-quiet-workspace` (cùng với `19` ảnh `account-dock-*.png` đang active tại `HEAD`, tổng cộng `35` blob PNG công khai). Quét chuỗi nhị phân trên cả `35` blob PNG xác nhận **không có đường dẫn người dùng hay khóa bí mật nhúng trong chunk metadata PNG**.
2. **Đang Public trên `origin/main` (`8cf4703`) và bị xóa trên nhánh cục bộ (`a5feb4a`, `1867beb`, `0806ffb`):**
   - `desktop-instance-manager/.github/workflows/ci.yml` (`blob=e61d068083`, `3.956` bytes) đã được dời lên thư mục gốc `.github/workflows/ci.yml` ở các nhánh cục bộ, nhưng bản gốc vẫn còn ở `HEAD` của `origin/main` và trong lịch sử Git. Tệp này không chứa bí mật cứng.
3. **Làm sạch một phần bằng ghi đè (`--diff-filter=M`) tại commit cục bộ `bff0674` (`integration/reliability-hardening`):**
   - Commit `bff0674` (*"path scrub"*) trên `integration/reliability-hardening` đã thay thế đường dẫn `C:\Users\<REDACTED_USER>\...` trong `25` tệp tài liệu/bằng chứng tại `HEAD` (`cd62c52`). Tuy nhiên:
     - Các phiên bản chưa che mặt nạ vẫn nằm nguyên trên **`origin/main`** (`17` tệp), **`origin/feat/ui-vi-en-quiet-workspace`** (`19` tệp), và trong **các commit cha cục bộ** (`a5feb4a`..`004ab7c`).
     - Vẫn còn sót **`3` tệp ngay tại `HEAD` của `integration/reliability-hardening` (`cd62c52`)** chứa đường dẫn `C:\Users\<REDACTED_USER>\...` của nhà phát triển bên thứ ba / máy build upstream (xem `GIT-SEC-005`).

### 2.3. Kiểm toán Email Author / Committer & Chữ ký Commit

- Đã kiểm tra toàn bộ `66` commit (`132` trường Author + Committer, bao gồm `18` lượt trên `origin/*`, `108` lượt trên nhánh cục bộ, `6` lượt trên commit mồ côi):
  - **100% commit sử dụng duy nhất `1` địa chỉ email ẩn danh của GitHub (`<REDACTED_EMAIL>`, thuộc miền ẩn danh chuẩn của GitHub)** đi kèm tên hiển thị tác giả. **Không có bất kỳ địa chỉ email cá nhân thực nào** (Gmail, Outlook, email công ty...) bị lộ trong metadata lịch sử commit hoặc nội dung thông điệp commit (`commit body`).
  - Trong `7` tệp bản vá `desktop-instance-manager/plans/reports/build/ir-patches/0001..0007-*.patch` (chưa push), trường `From:` dùng địa chỉ email giả lập giữ chỗ (`<REDACTED_EMAIL>` thuộc miền ví dụ chuẩn RFC), không làm lộ email cá nhân.
  - Toàn bộ `66` commit hiện ở trạng thái chưa ký mật mã (`%G? = N` — không có chữ ký GPG/SSH).

### 2.4. Kiểm toán Tệp Nhị Phân, Kho Lưu Trữ, Cơ Sở Dữ Liệu, Nhật Ký, Cấu Hình & Sao Lưu Được Track

| Phân loại tệp nhạy cảm | Số lượng trên `origin/*` (Public) | Số lượng chỉ có ở Local Git | Đánh giá chi tiết |
| :--- | :---: | :---: | :--- |
| Thực thi biên dịch sẵn (`.exe`, `.dll`, `.so`, `.dylib`, `.asar`) | **`1`** | `0` | **`multi-zalo-desktop/dist/MultiZaloDesktop.exe`** (`59.392` B, `blob=efc8089112`, PE32/.NET IL assembly không chữ ký số) có mặt trên **mọi nhánh** (`GIT-SEC-003`). Xác nhận `0` tệp VNG `Zalo.exe` hay `app.asar` trong kho. |
| Cơ sở dữ liệu & bộ nhớ phiên (`.db`, `.sqlite*`, LevelDB `.ldb`, `Cookies`) | `0` | `0` | **Sạch (`PASS`)** — không có tệp SQLite hay LevelDB nào từng được commit. |
| Khóa mật mã & chứng chỉ (`.pem`, `.key`, `.pfx`, `.p12`, `.tauri.key`) | `0` | `0` | **Sạch (`PASS`)** — không có khóa riêng tư hay chứng chỉ ký số nào trong Git. |
| Kho nén & gói cài đặt (`.zip`, `.tar.gz`, `.7z`, `.msi`, `.dmg`, `.deb`, `.AppImage`) | `0` | `0` | **Sạch (`PASS`)** — không có gói cài đặt hay tệp nén nào được commit. |
| Tệp môi trường, nhật ký & sao lưu (`.env*`, `*.log`, `*.bak`, `*.rs.bk`) | `0` | `0` | **Sạch (`PASS`)** — không có tệp `.env`, `.log`, hay bản sao lưu nào trong Git. |
| Báo cáo bằng chứng JSON (`evidence/*.json`, `plans/reports/**/*.json`) | **`11`** | `12` | Không chứa secret/cookie phiên Zalo, nhưng `10` tệp JSON trên `origin/*` và `dependency-audit-report.json` cục bộ chứa đường dẫn máy chủ `C:\Users\<REDACTED_USER>\...`; `zalo-live-poc-result.json` chứa mã băm SHA-256 của tệp cấu hình Zalo đang chạy trên máy chủ (`GIT-SEC-001`, `GIT-SEC-002`, `GIT-SEC-008`). |
| Biểu tượng & Ảnh chụp màn hình (`.ico`, `.icns`, `.png`) | `35` blobs (`.png`) | `4` blobs (`.png`, `.ico`, `.icns`) | Chữ ký PNG/ICO/ICNS hợp lệ, không nhúng chuỗi đường dẫn hay PII trong metadata nhị phân (nội dung hiển thị ảnh đối chiếu tại `evidence-exposure-review.md`). |

---

## 3. Kiểm Toán Toàn Bộ Worktree Trước Khi Push (`Pre-Push Worktrees & .gitignore Audit`)

### 3.1. Ma trận trạng thái toàn bộ `10` Worktree trực tiếp trên đĩa

Đã quét lại thư mục `Downloads` và phát hiện thêm worktree **`zalo-windows-acceptance`** (`qa/windows-native-acceptance`) đang hoạt động song song cùng worktree kiểm toán **`zalo-public-security-audit`**:

| Worktree (Vai trò) | Nhánh & HEAD | Tệp Sửa đổi / Staged | Tệp Untracked (`??`) | Thư mục Ignored ngoài `target`/`node_modules` | Rủi ro Pre-Push phát hiện |
| :--- | :--- | :---: | :---: | :--- | :--- |
| `zalo` *(Backend / Main)* | `main` (`a5feb4a`) | `0` | **`3`** (`docs/release/*.md`) | `rea/` (`~9.100` tệp repo clone ngoài); `src-tauri/gen/` | `HEAD` còn `12` tệp lộ `C:\Users\<REDACTED_USER>\...` + `MultiZaloDesktop.exe`; tệp untracked `integration-readiness-matrix.md` ghi rõ OAuth scopes của `gh auth status`; thư mục lớn `rea/` nằm ngay trong root (`GIT-SEC-002`, `GIT-SEC-007`). |
| `zalo-integration` *(Central Integration)* | `integration/reliability-hardening` (`cd62c52`) | `0` | `0` | `src-tauri/gen/` | `HEAD` đã scrub username chủ máy nhưng còn `MultiZaloDesktop.exe`, `3` tệp lộ đường dẫn Windows bên thứ ba, và **`54` commit lịch sử chưa push** chứa `426` blob cục bộ có `.gitconfig`/`.cargo`/worktree paths (`GIT-SEC-002`, `GIT-SEC-003`, `GIT-SEC-005`). |
| `zalo-ui-worktree` *(Account Dock / UI)* | `feat/ui-vi-en-quiet-workspace` (`0f87302`) | `0` | `0` | *(không)* | Đã push lên `origin`; còn `19` tệp lộ `C:\Users\<REDACTED_USER>\...`, `MultiZaloDesktop.exe`, và `.gitignore` thiếu dòng chặn `src-tauri/gen/` (`GIT-SEC-001`, `GIT-SEC-006`). |
| `zalo-macos-worktree` *(macOS A2)* | `feat/macos-multi-instance` (`f2b8f9f`) | `0` | `0` | `src-tauri/gen/` | Sạch tệp tạm, nhưng `HEAD` còn `14` tệp lộ `C:\Users\<REDACTED_USER>\...` (gồm `native-runtime-hardening-report.md` và `plans/reports/macos/test-and-verification-report.md`) (`GIT-SEC-002`). |
| `zalo-ubuntu-worktree` *(Ubuntu B2)* | `feat/ubuntu-multi-instance` (`62dc5d1`) | `0` | `0` | *(không)* | Các tệp mới của Ubuntu dùng đường dẫn giả lập kiểm thử (`/home/<REDACTED_USER>/...` dạng tên mẫu giả lập), nhưng kế thừa `12` tệp lộ `C:\Users\<REDACTED_USER>\...` từ `main` (`GIT-SEC-002`). |
| `zalo-cross-platform-tests` *(Reliability C2)* | `test/cross-platform-reliability` (`5e802e0`) | `0` | `0` | *(không)* | Sạch tệp tạm; kế thừa `12` tệp lộ đường dẫn từ `main` (`GIT-SEC-002`). |
| `zalo-compatibility-research` *(Compatibility D2)* | `research/desktop-compatibility` (`0e5d44c`) | `0` | `0` | *(không)* | Sạch tệp tạm; kế thừa `12` tệp lộ đường dẫn từ `main` (`GIT-SEC-002`). |
| `zalo-cross-platform-build` *(Build E2)* | `build/cross-platform-distribution` (`6c4150e`) | `0` | `0` | `src-tauri/gen/` | **Rủi ro cao nếu push nhánh trực tiếp:** `HEAD` chứa `19` tệp lộ `C:\Users\<REDACTED_USER>\...`, bao gồm cấu hình `C:/Users/<REDACTED_USER>/.gitconfig`, `credential.helper`, `C:\Users\<REDACTED_USER>\.cargo\bin\cargo.exe`, và OAuth token scopes trong `ci-baseline-and-github-audit.md` (`GIT-SEC-002`, `GIT-SEC-008`). |
| `zalo-windows-acceptance` *(Windows QA — mới)* | `qa/windows-native-acceptance` (`004ab7c`) | `0` | **`18`** (`docs/qa/windows/*`, `tests/windows-acceptance/**`) | `src-tauri/gen/` | `18` tệp untracked mới đã dùng placeholder `<USER_HOME>` / `<WORKTREE_ROOT>` (không lộ secret/username chủ máy, dù `installer-env-audit.json` ghi chi tiết build OS & registry WebView2/SAC), **nhưng `HEAD` (`004ab7c`) tách trước `bff0674` nên còn tới `24` tệp lộ `C:\Users\<REDACTED_USER>\...`** (`GIT-SEC-002`, `GIT-SEC-007`). |
| `zalo-public-security-audit` *(Audit F)* | `audit/public-repository-security` (`a5feb4a`) | `0` | Các tệp `.md` trong `docs/security/public-audit/` | *(không)* | Chỉ chứa báo cáo kiểm toán đã che mặt nạ 100%; không push trực tiếp khi chưa qua cổng phê duyệt. |

### 3.2. Phân tích khoảng trống `.gitignore` (`.gitignore` Gap Analysis)

Hiện tại `.gitignore` ở thư mục gốc chỉ có `18` dòng (chặn `rea/`, `target/`, `desktop-instance-manager/apps/desktop/src-tauri/gen/`, `**/*.rs.bk`, `node_modules/`, `.pnpm-store/`, `.env`, `.env.*`, `*.log`, `.DS_Store`, `Thumbs.db`), và riêng trên nhánh công khai `feat/ui-vi-en-quiet-workspace` thậm chí còn thiếu `desktop-instance-manager/apps/desktop/src-tauri/gen/`. Các nhóm mẫu bảo mật trọng yếu đang **thiếu hoàn toàn** (xem `GIT-SEC-006`):

1. **Khóa ký số ứng dụng & chứng chỉ mật mã:** `*.pem`, `*.key`, `*.pfx`, `*.p12`, `*.cer`, `*.crt`, `*.der`, `*.jks`, `*.keystore`, `*.tauri.key`, `.tauri/`.
2. **Dữ liệu phiên runtime Electron / WebView2 / Zalo cô lập:** `*.db`, `*.sqlite`, `*.sqlite3`, `*.sqlite-wal`, `*.sqlite-shm`, `*.ldb`, `Cookies`, `Cookies-journal`, `Local Storage/`, `Session Storage/`, `IndexedDB/`, `ZaloData/`, `MultiZaloDesktop*/`, `Profiles/`.
3. **Thực thi biên dịch sẵn, gói phân phối & crash dump:** `*.exe`, `*.dll`, `*.msi`, `*.dmg`, `*.pkg`, `*.deb`, `*.AppImage`, `*.rpm`, `*.asar`, `*.pdb`, `*.dmp`, `dist/`.
4. **Tệp sao lưu tạm, cấu hình IDE cục bộ & đầu ra kiểm thử thô:** `*.bak`, `*.old`, `*.orig`, `*.tmp`, `*.swp`, `*~`, `Desktop.ini`, `.idea/`, `.vscode/*` (trừ cấu hình chia sẻ có chủ đích).

---

## 4. Kết Quả Quét Chi Tiết Theo Danh Mục Bí Mật, Phiên Zalo & PII (`Section 4`)

| Nhóm dữ liệu mục tiêu | Kết quả trên `origin/*` (Public) | Kết quả trên Local Git & Worktrees | Minh chứng & Giải trình kỹ thuật |
| :--- | :---: | :---: | :--- |
| **API keys, GitHub PATs, Cloud/SaaS tokens** | **`0` phát hiện (`CLEAN`)** | **`0` khóa thực (`CLEAN`)** | Quét toàn bộ `660` blobs và `10` worktrees không có bất kỳ khóa thực nào. *(Các chuỗi mẫu trong `zalo/rea/**/*.test.*` và `generate-mitmproxy-capture.py` là vector kiểm thử giả lập nằm trong thư mục `rea/` đã bị `.gitignore` chặn).* |
| **Thông tin OAuth / Khóa ký số, Private Keys, Chứng chỉ** | **`0` khóa thực (`CLEAN`)** | **`0` khóa thực** *(có lộ thông tin mô tả scope CLI)* | Không có khóa riêng tư RSA/EC/Ed25519/Minisign/Tauri hay chứng chỉ `.pfx`/`.p12`. Tuy nhiên, các báo cáo build cục bộ ghi lại kết quả `gh auth status` gồm danh sách OAuth scopes và cấu hình `credential.helper` (`GIT-SEC-008`). |
| **Session tokens, Cookies, Phiên Zalo (`zpw_*`, `imei`, UIDs, LevelDB/SQLite, SĐT)** | **`0` dữ liệu phiên thực (`CLEAN`)** | **`0` dữ liệu phiên thực (`CLEAN`)** | `17` vị trí khớp chuỗi `zpw_*` đều nằm trong bộ lọc che mặt nạ bảo vệ (`crates/core/src/sanitize.rs`, unit test trong `crates/core/src/lib.rs`, và `tests/windows-acceptance/lib/common.mjs`). Không có `imei` thực, UID Zalo, số điện thoại cá nhân, hay tệp LevelDB/SQLite. Lưu ý: `zalo-live-poc-result.json` công khai mã băm SHA-256 một chiều của `config.json`/`log.log` trên máy chạy PoC (`GIT-SEC-008`). |
| **Đường dẫn hệ thống tệp cá nhân (`C:\Users\<REDACTED_USER>\...`)** | **`PHƠI NHIỄM` (`19` tệp trên `origin/*`)** | **`PHƠI NHIỄM` (`12`–`24` tệp trên các nhánh chưa scrub + `3` tệp bên thứ ba)** | Lộ tên tài khoản người dùng Windows cục bộ trên `origin/main`, `origin/feat/ui-vi-en-quiet-workspace`, và các nhánh cục bộ chưa qua commit `bff0674`; đồng thời còn `3` tệp chứa đường dẫn Windows của tác giả bên thứ ba / PDB trong `app.asar` (`GIT-SEC-001`, `GIT-SEC-002`, `GIT-SEC-005`). |
| **Địa chỉ Email cá nhân** | **`0` email cá nhân thực (`CLEAN`)** | **`0` email cá nhân thực (`CLEAN`)** | Toàn bộ `66` commit chỉ dùng `<REDACTED_EMAIL>` dạng ẩn danh của GitHub; `7` tệp `.patch` dùng email giữ chỗ giả lập. *(Lưu ý kỹ thuật: tên tệp biểu tượng độ phân giải cao có ký tự nối kích thước đã được bộ lọc phân tách chính xác khỏi địa chỉ email).* |
| **Machine IDs, Windows SIDs, UUID phần cứng, Hostnames** | **`0` SID / Hostname thực (`CLEAN`)** | **`0` SID / Hostname thực (`CLEAN`)** | Không phát hiện `S-1-5-21-...`, `MachineGuid`, `IOPlatformUUID`, địa chỉ MAC hay tên máy cục bộ `<REDACTED_HOST>` trong toàn bộ kho Git và worktree. *(Chuỗi mã hiệu báo cáo `DIM-WIN-BUILD-...` được xác minh là nhãn tài liệu tiếng Anh, không phải hostname máy tính).* |

---

## 5. Danh Mục Phát Hiện Chi Tiết (`GIT-SEC-001` .. `GIT-SEC-008` — Đầy Đủ 9 Trường Bắt Buộc)

### `GIT-SEC-001` — Phơi nhiễm đường dẫn hệ điều hành cá nhân (`C:\Users\<REDACTED_USER>\...`) trên các nhánh đã công khai (`origin/main` & `origin/feat/ui-vi-en-quiet-workspace`) và trong lịch sử Git bất biến

- **ID:** `GIT-SEC-001`
- **Severity:** `P1`
- **Source:** Quét lịch sử Git công khai (`refs/remotes/origin/main` @ `8cf4703`, `refs/remotes/origin/feat/ui-vi-en-quiet-workspace` @ `0f87302`) và kho đối tượng `.git/objects` (`229` public blobs).
- **Evidence type:** Biểu thức chính quy đường dẫn người dùng hệ điều hành (`PII_WIN_USER_PATH`, `PII_EXACT_OS_USERNAME`) xác minh trực tiếp trên nội dung blob công khai.
- **File/commit liên quan:**
  - Commit công khai `8cf4703` (`origin/main` — `17` tệp, `~192` vị trí) và chuỗi commit `39ff5c7`..`0f87302` (`origin/feat/ui-vi-en-quiet-workspace` — `19` tệp, `~194` vị trí), tiêu biểu:
    - `desktop-instance-manager/docs/security/threat-model-and-audit.md` (`blob=c65d175210`, `64` vị trí `file:///c:/Users/<REDACTED_USER>/...`)
    - `desktop-instance-manager/README.md` (`blob=f5ae1a7ac5`, `32` vị trí)
    - `desktop-instance-manager/docs/testing/poc-and-test-report.md` (`blob=cecea616cf`, `11` vị trí)
    - `desktop-instance-manager/docs/research/environment-readiness.md` (`blob=ff235ef2a8`, `6` vị trí lộ đường dẫn `.agentkit`, `.gemini/config/skills`, `.claude/plugins`, `Downloads/Programs`)
    - `desktop-instance-manager/plans/261117-zalo-instance-manager/plan.md` (`blob=43f1622cb3`, `1` vị trí)
    - `desktop-instance-manager/docs/design/account-dock-redesign.md` (`blob=206e48af63` trong lịch sử `origin/feat/ui-vi-en-quiet-workspace`), `ui-i18n-implementation-report.md` (`blob=e6373f854a`), `ui-independent-review-and-remediation.md` (`blob=cfccf66754`)
    - `multi-zalo-desktop/docs/research/rea-capabilities.md` (`blob=c0b2a26d07`), `zalo-desktop-analysis.md` (`blob=195ca72f8e`), và toàn bộ `10` tệp JSON trong `multi-zalo-desktop/evidence/*.json` (`poc-verification-result.json`, `rea-audit-release-exposure-app-asar.json`, `integration-test-result.json`, `rea-evidence-summary.json`, `rea-binary-session.json`, `rea-pe-*.json`).
- **Tác động:** Làm lộ công khai tên tài khoản người dùng Windows cục bộ (`<REDACTED_USER>`), cấu trúc cây thư mục máy phát triển, các công cụ agent cục bộ đang cài đặt, và vị trí lưu trữ tệp cài đặt/dữ liệu ứng dụng. Dù commit sửa lỗi sau này xóa khỏi `HEAD`, các blob trong `8cf4703` và `39ff5c7`..`0f87302` vẫn truy xuất được vĩnh viễn trong lịch sử Git công khai nếu không viết lại lịch sử.
- **Khuyến nghị xử lý:**
  1. Theo quy tắc an toàn không viết lại lịch sử (`no history rewrite`), phát hành commit làm sạch tiến về phía trước (`forward commit`) trên `main` để thay toàn bộ đường dẫn tuyệt đối bằng đường dẫn tương đối trong repo hoặc nhãn `<USER_HOME>` / `%LOCALAPPDATA%` / `%APPDATA%`.
  2. Sau khi nhánh giao diện `feat/ui-vi-en-quiet-workspace` đã được tích hợp vào `main`, xóa nhánh phụ `origin/feat/ui-vi-en-quiet-workspace` trên GitHub để thu hẹp bề mặt tham chiếu trực tiếp.
  3. Chủ sở hữu kho (`Repo Owner`) ghi nhận và chấp nhận rủi ro tồn dư mức thấp-trung bình đối với tên người dùng OS cục bộ đã nằm trong commit gốc `8cf4703`.
- **Owner:** Central Integration Agent + Release Captain (phối hợp Repo Owner xác nhận rủi ro lịch sử tồn dư).
- **Trạng thái:** `OPEN` (đã khắc phục một phần trên nhánh cục bộ `integration/reliability-hardening` @ `bff0674`, nhưng **chưa khắc phục trên `origin/main` và `origin/feat/ui-vi-en-quiet-workspace`**).

---

### `GIT-SEC-002` — `54` commit cục bộ chưa push và `HEAD` của `8` nhánh làm việc còn chứa đường dẫn máy chủ (`C:\Users\<REDACTED_USER>\...`), `.gitconfig` và `.cargo` — nguy cơ phát tán lên GitHub nếu push trực tiếp hoặc merge không squash

- **ID:** `GIT-SEC-002`
- **Severity:** `P1`
- **Source:** Quét đối chiếu toàn bộ nhánh cục bộ (`refs/heads/*`) và `426` blob chưa từng xuất hiện trên `origin/*`.
- **Evidence type:** Đối chiếu đồ thị commit (`git rev-list`) kết hợp quét biểu thức chính quy đường dẫn cá nhân trên từng nhánh và từng commit trung gian.
- **File/commit liên quan:**
  - `build/cross-platform-distribution` (`6c4150e` — `19` tệp tại `HEAD`) và `qa/windows-native-acceptance` (`004ab7c` — `24` tệp tại `HEAD`), đặc biệt các tệp chưa từng lộ trên `origin`:
    - `desktop-instance-manager/plans/reports/build/ci-baseline-and-github-audit.md` (`blob=6c96a82e8b`, `d15a163f00`, `26ae6757f1`, `7b1a996123` — lộ `C:/Users/<REDACTED_USER>/.gitconfig`, đường dẫn gói `WinGet` MinGit, `credential.helper`)
    - `desktop-instance-manager/docs/packaging/ci-execution-and-artifact-validation.md` (`blob=c810406464`, `5397d029ee`, `8a45f8f78c`)
    - `desktop-instance-manager/plans/reports/build/dependency-audit-report.json` (`blob=5cc3ac0a27`, `95865d9691` — lộ `C:\Users\<REDACTED_USER>\.cargo\bin\cargo.exe`)
    - `desktop-instance-manager/plans/reports/build/integration-requests.md` (`blob=fe7c756049` — liệt kê toàn bộ đường dẫn tuyệt đối của các worktree cục bộ)
    - `desktop-instance-manager/plans/reports/build/windows-local-build-report.md` (`blob=87493bc3ca`), `docs/packaging/windows.md` (`blob=89d64995d7`), `docs/packaging/release-readiness.md` (`blob=e3d8ef5a2e`)
    - `desktop-instance-manager/docs/platform/macos/native-runtime-hardening-report.md` (`blob=d4082c5898`), `plans/reports/macos/test-and-verification-report.md` (`blob=b843c3fa56`).
  - Chuỗi `54` commit cục bộ trên `integration/reliability-hardening` (`a5feb4a`..`004ab7c` trước `bff0674`): dù `HEAD` (`cd62c52`) đã được scrub, toàn bộ các blob chứa `.gitconfig` và đường dẫn máy chủ nêu trên vẫn nằm trong lịch sử nhánh `integration/reliability-hardening`.
- **Tác động:** Nếu bất kỳ nhánh thành phần nào (`build/*`, `feat/*`, `qa/*`, `research/*`, `test/*`) được push thẳng lên `origin`, hoặc nếu `integration/reliability-hardening` được đẩy/merge thường (không squash) lên `origin/main`, toàn bộ `426` blob cục bộ (chứa cấu hình `.gitconfig`, trình quản lý thông tin xác thực Git và cây worktree) sẽ bị công khai vĩnh viễn trên GitHub.
- **Khuyến nghị xử lý:**
  1. **Tuyệt đối KHÔNG push trực tiếp** các nhánh thành phần chưa làm sạch (`build/cross-platform-distribution`, `feat/macos-multi-instance`, `feat/ubuntu-multi-instance`, `research/desktop-compatibility`, `test/cross-platform-reliability`, `qa/windows-native-acceptance`) lên `origin` công khai.
  2. Khi đưa kết quả tích hợp từ `integration/reliability-hardening` lên `main` để chuẩn bị push lên `origin/main`, sử dụng **commit gộp sạch (`git merge --squash` hoặc tạo commit phát hành sạch trên nền `origin/main`)** sau khi đã hoàn tất mọi bước kiểm tra bảo mật — nhờ đó không viết lại `origin/main` mà vẫn ngăn chặn hoàn toàn việc tải `54` commit trung gian chứa PII lên GitHub.
- **Owner:** Release Captain + Central Integration Agent.
- **Trạng thái:** `OPEN` (chặn thao tác `git push` trực tiếp các nhánh cục bộ trước khi gộp sạch).

---

### `GIT-SEC-003` — Tệp thực thi nhị phân biên dịch sẵn không chữ ký số `multi-zalo-desktop/dist/MultiZaloDesktop.exe` (`59.392` bytes) đang được track trên tất cả các nhánh

- **ID:** `GIT-SEC-003`
- **Severity:** `P1`
- **Source:** Bộ nhận diện chữ ký nhị phân (`Magic Header = WINDOWS_PE_EXECUTABLE`) và kiểm tra cây tệp `git ls-tree` trên toàn bộ `12` tham chiếu nhánh (`origin/*` và `refs/heads/*`).
- **Evidence type:** Phân tích header nhị phân PE32/.NET IL assembly và trích xuất chuỗi ASCII/UTF-16LE trên `blob=efc8089112` (`59.392` bytes).
- **File/commit liên quan:**
  - `multi-zalo-desktop/dist/MultiZaloDesktop.exe` (`blob=efc8089112`, đưa vào từ commit công khai đầu tiên `8cf4703` trên `origin/main`, hiện vẫn còn nguyên tại `HEAD` của tất cả các nhánh bao gồm cả `integration/reliability-hardening` @ `cd62c52`).
- **Tác động:**
  - Kiểm tra chuỗi nhị phân xác nhận tệp này là bản build PoC `.NET` (`MultiZaloDesktop.Core`/`UI`, `MyApplication.app v1.0.0.0`) không nhúng khóa bí mật hay đường dẫn PDB cá nhân, nên không ở mức `P0`.
  - Tuy nhiên, việc lưu trữ tệp thực thi `.exe` biên dịch sẵn, không có chữ ký số (`unsigned PE executable`) ngay trong cây mã nguồn công khai gây rủi ro chuỗi cung ứng mã nguồn mở (người dùng/kiểm toán viên không thể xác minh tính toàn vẹn tương ứng 1-1 với mã nguồn, dễ kích hoạt cảnh báo giả từ trình diệt virus / Windows SmartScreen / Smart App Control khi clone repo, và gây nhầm lẫn với kiến trúc Rust/Tauri chính thức trong `desktop-instance-manager/`).
- **Khuyến nghị xử lý:**
  1. Xóa `multi-zalo-desktop/dist/MultiZaloDesktop.exe` khỏi cây theo dõi Git bằng commit tiến (`git rm multi-zalo-desktop/dist/MultiZaloDesktop.exe`) trên nhánh tích hợp/chính trước khi phát hành công khai.
  2. Bổ sung `dist/` và `*.exe` vào `.gitignore` gốc; chỉ phân phối tệp thực thi qua GitHub Releases kèm tệp băm `SHA256SUMS.txt` và tuyên bố trạng thái chữ ký số rõ ràng.
- **Owner:** Central Integration Agent + Build Agent.
- **Trạng thái:** `OPEN`.

---

### `GIT-SEC-004` — `16` ảnh chụp màn hình giao diện đã xóa (`--diff-filter=D`) và tệp workflow cũ vẫn tồn tại vĩnh viễn trong lịch sử Git công khai trên `origin`

- **ID:** `GIT-SEC-004`
- **Severity:** `P1`
- **Source:** Kiểm toán lịch sử xóa tệp (`git diff-tree -r --diff-filter=D`) trên toàn bộ `66` commit.
- **Evidence type:** Đối chiếu danh sách blob bị xóa nhưng vẫn thuộc đồ thị truy cập công khai `refs/remotes/origin/*`.
- **File/commit liên quan:**
  - Commit công khai `cc74584` trên `origin/feat/ui-vi-en-quiet-workspace` xóa `16` ảnh `.png` từng được commit ở `39ff5c7`/`2b1ce6e`/`b1446d6`: `desktop-instance-manager/docs/design/screenshots/backups-vi-light-1160x780.png` (`blob=06923042d6`), `diagnostics-vi-light-1160x780.png` (`blob=f1618098ce`), `dialog-create-vi-light.png` (`blob=5438705313`), `dialog-delete-vi-light.png` (`blob=2ddcbefb88`), `dialog-details-vi-light.png` (`blob=a6a4e0aa4f`), `overflow-menu-vi-light.png` (`blob=603156d403`), `profiles-en-US-dark-1160x780.png` (`blob=51009bd94f`), `profiles-en-US-light-1160x780.png` (`blob=75303d849f`), `profiles-vi-VN-dark-1160x780.png` (`blob=c3cdc01d89`), `profiles-vi-VN-light-1160x780.png` (`blob=8881c88757`), `profiles-vi-VN-light-420x540.png` (`blob=2c48700567`), `profiles-vi-VN-light-768x650.png` (`blob=ab55f30fc1`), `profiles-vi-VN-light-900x650.png` (`blob=9fe7cb6267`), `settings-vi-light-1160x780.png` (`blob=8c8023aa7f`), `state-error-banner-vi-light.png` (`blob=2ddcbefb88`), `zoom-200-vi-light.png` (`blob=8b872c353f`).
  - Commit cục bộ `a5feb4a` (`main`) và `1867beb` (`integration`) xóa `desktop-instance-manager/.github/workflows/ci.yml` (`blob=e61d068083`), nhưng tệp này vẫn đang hiện diện tại `HEAD` của `origin/main` (`8cf4703`).
- **Tác động:** Mọi tệp từng được push lên `origin/feat/ui-vi-en-quiet-workspace` dù đã xóa ở commit sau vẫn tải về được đầy đủ qua `git clone` hoặc URL commit trên GitHub. Nếu hợp nhất nhánh UI bằng merge thường vào `main`, cả `16` blob ảnh cũ đã xóa (`~828 KB`) sẽ đi vào lịch sử vĩnh viễn của nhánh mặc định `origin/main`.
- **Khuyến nghị xử lý:**
  1. Đối chiếu kết quả kiểm tra thị giác (`visual inspection`) của cả `16` ảnh đã xóa và `19` ảnh đang active trong báo cáo `evidence-exposure-review.md` để xác nhận không lộ hội thoại/tài khoản Zalo thật.
  2. Hợp nhất vào `main` bằng cơ chế gộp sạch (`squash`) để lịch sử nhánh `main` không phải gánh thêm `16` blob ảnh nhị phân đã lỗi thời, sau đó xóa nhánh từ xa `origin/feat/ui-vi-en-quiet-workspace` khi hoàn tất phát hành.
- **Owner:** Release Captain + Evidence Auditor (xác minh nội dung ảnh) + Central Integration Agent.
- **Trạng thái:** `OPEN`.

---

### `GIT-SEC-005` — Sót đường dẫn người dùng Windows của bên thứ ba và máy build upstream (`C:\Users\<REDACTED_USER>\...`) ngay tại `HEAD` của mọi nhánh (kể cả sau commit scrub `bff0674`)

- **ID:** `GIT-SEC-005`
- **Severity:** `P2`
- **Source:** Quét biểu thức chính quy tổng quát `PII_WIN_USER_PATH` trên cây tệp tại `HEAD` của `integration/reliability-hardening` (`cd62c52`) và `origin/main` (`8cf4703`).
- **Evidence type:** Phát hiện mẫu đường dẫn `C:\Users\<REDACTED_USER>\...` mang định danh cá nhân khác với tên tài khoản chủ máy hiện tại nên bị bỏ sót trong đợt tìm-thay-thế theo chuỗi cố định tại commit `bff0674`.
- **File/commit liên quan:**
  - `multi-zalo-desktop/evidence/rea-audit-release-exposure-app-asar.json` (`blob=f75997fb7a` trên `origin/main`; `blob=85be3d88f1` tại `cd62c52` trên `integration/reliability-hardening`): trường `"samples"` còn lưu `2` đường dẫn PDB từ máy biên dịch native addon bên trong `app.asar` (`C:\Users\<REDACTED_USER>\Desktop\src-code\native\build\Release\binding.pdb` và biến thể hậu tố `local`).
  - `desktop-instance-manager/docs/research/repository-comparison.md` (dòng `42`, `blob=0d6e78592e`) và `multi-zalo-desktop/docs/research/existing-solutions.md` (dòng `42`, `blob=e363b2109c`): trích dẫn nguyên văn đường dẫn cứng `C:\Users\<REDACTED_USER>\ZaloMulti\ZaloMulti.ps1` của tác giả một kho tham khảo bên ngoài.
- **Tác động:** Tiết lộ tên người dùng máy trạm nội bộ của nhà phát triển upstream/bên thứ ba trong tài liệu và tệp bằng chứng JSON phân phối công khai, đồng thời có thể làm cảnh báo các công cụ quét PII tự động trong pipeline kiểm toán phát hành.
- **Khuyến nghị xử lý:**
  1. Cập nhật quy tắc làm sạch đường dẫn sang dạng tổng quát theo mẫu `C:\Users\<REDACTED_USER>\...` (bằng regex thay vì chỉ khớp một tên người dùng cố định) đối với cả `3` tệp trên trước khi đóng gói commit tích hợp cuối cùng.
- **Owner:** Central Integration Agent + Compatibility/Evidence Owner.
- **Trạng thái:** `OPEN`.

---

### `GIT-SEC-006` — Khoảng trống bảo mật trong `.gitignore` đối với khóa ký số, dữ liệu phiên Zalo/Electron runtime, và tệp nhị phân biên dịch sẵn

- **ID:** `GIT-SEC-006`
- **Severity:** `P1`
- **Source:** Kiểm toán toàn bộ các tệp `.gitignore` trên `10` worktree (`zalo`, `zalo-integration`, `zalo-ui-worktree`, `zalo-cross-platform-build`, v.v.).
- **Evidence type:** Phân tích cấu hình kiểm soát phiên bản tĩnh đối chiếu với vòng đời build/chạy thử của ứng dụng Tauri v2 + Zalo Desktop đa phiên bản.
- **File/commit liên quan:**
  - `.gitignore` tại thư mục gốc trên tất cả các nhánh (`origin/main` @ `8cf4703`, `main` @ `a5feb4a`, `integration/reliability-hardening` @ `cd62c52`, và đặc biệt bản thiếu `desktop-instance-manager/apps/desktop/src-tauri/gen/` trên `feat/ui-vi-en-quiet-workspace` @ `0f87302`).
- **Tác động:** Khi dự án chuyển sang kho công khai có nhiều tác nhân AI và cộng tác viên cùng làm việc trên nhiều worktree hoặc chạy thử nghiệm L3 với tài khoản Zalo thật, việc `.gitignore` không chặn các tệp khóa ký (`.pfx`, `.p12`, `.pem`, `.key`, `.tauri.key`), cơ sở dữ liệu phiên Chromium/Zalo (`*.sqlite*`, `*.ldb`, `Cookies`, `ZaloData/`, `Profiles/`), và gói nhị phân (`*.exe`, `*.msi`, `*.dmg`, `*.AppImage`, `*.asar`, `dist/`) dẫn tới nguy cơ rất cao một lệnh `git add .` sẽ vô tình commit thẳng khóa ký phát hành hoặc dữ liệu phiên đăng nhập vào kho công khai.
- **Khuyến nghị xử lý:**
  1. Nâng cấp ngay `.gitignore` gốc trên nhánh tích hợp/chính để bổ sung đầy đủ các nhóm chặn bắt buộc:
     - Khóa & chứng chỉ: `*.pem`, `*.key`, `*.pfx`, `*.p12`, `*.cer`, `*.crt`, `*.der`, `*.jks`, `*.keystore`, `*.tauri.key`, `.tauri/`
     - Dữ liệu phiên & hồ sơ runtime: `*.db`, `*.sqlite`, `*.sqlite3`, `*.sqlite-wal`, `*.sqlite-shm`, `*.ldb`, `Cookies`, `Cookies-journal`, `Local Storage/`, `Session Storage/`, `IndexedDB/`, `ZaloData/`, `MultiZaloDesktop*/`, `Profiles/`
     - Thực thi, gói cài đặt & dump: `*.exe`, `*.dll`, `*.msi`, `*.dmg`, `*.pkg`, `*.deb`, `*.AppImage`, `*.rpm`, `*.asar`, `*.pdb`, `*.dmp`, `dist/`
     - Tệp tạm & sao lưu: `*.bak`, `*.old`, `*.orig`, `*.tmp`, `*.swp`, `*~`, `Desktop.ini`.
  2. Đồng bộ dòng `desktop-instance-manager/apps/desktop/src-tauri/gen/` trên mọi nhánh đang hoạt động.
- **Owner:** Build Agent + Central Integration Agent.
- **Trạng thái:** `OPEN`.

---

### `GIT-SEC-007` — Tồn tại `21` tệp chưa theo dõi (`untracked ??`) và thư mục clone ngoài `rea/` (`~9.100` tệp) nằm trực tiếp trong các worktree đang hoạt động (`zalo` và `zalo-windows-acceptance`)

- **ID:** `GIT-SEC-007`
- **Severity:** `P2`
- **Source:** Kiểm toán trạng thái cây làm việc trực tiếp (`git status --porcelain=v1 -uall` & `--ignored=matching`) trên toàn bộ `10` worktree.
- **Evidence type:** Quét nội dung trực tiếp trên đĩa đối với mọi tệp chưa theo dõi và tệp bị ignore ngoài `target/` & `node_modules/`.
- **File/commit liên quan:**
  - Worktree `zalo` (`main` @ `a5feb4a`):
    - `3` tệp untracked: `desktop-instance-manager/docs/release/coordination-checkpoint.md`, `desktop-instance-manager/docs/release/integration-readiness-matrix.md` (dòng `23` ghi chi tiết trạng thái xác thực CLI và phạm vi OAuth scopes hiện hành), `desktop-instance-manager/docs/release/release-blocker-register.md`.
    - Thư mục bị ignore `rea/` (`~9.100` tệp thuộc một kho công cụ phân tích độc lập có chứa hàng trăm vector kiểm thử URL basic-auth và khóa giả lập trong `*.test.ts` / `scripts/fixtures/*`), chỉ được bảo vệ bởi duy nhất một dòng `rea/` trong `.gitignore`.
  - Worktree `zalo-windows-acceptance` (`qa/windows-native-acceptance` @ `004ab7c`):
    - `18` tệp untracked trong `desktop-instance-manager/docs/qa/windows/*.md` và `desktop-instance-manager/tests/windows-acceptance/**` (bao gồm `evidence/installer-env-audit.json` và `evidence/native-tauri-smoke-report.json` ghi chi tiết phiên bản build OS `10.0.26300`, khóa registry chính sách Smart App Control, đường dẫn/phiên bản WebView2 và mã băm SHA-256 của tệp thực thi thử nghiệm cục bộ).
- **Tác động:** Mặc dù cả `21` tệp untracked hiện không chứa khóa bí mật hay đường dẫn `C:\Users\<REDACTED_USER>\...` (đã dùng biến giữ chỗ `<USER_HOME>` và `<WORKTREE_ROOT>`), việc để các báo cáo điều phối nội bộ, hồ sơ cấu hình môi trường máy chủ và kho ngoài `rea/` nằm chung trong cây worktree tạo nguy cơ rò rỉ thông tin vận hành nội bộ nếu thực hiện `git add -A` hoặc chỉnh sửa nhầm `.gitignore`.
- **Khuyến nghị xử lý:**
  1. Rà soát từng tệp untracked trong `zalo` và `zalo-windows-acceptance` trước khi stage (`git add` tường minh từng đường dẫn thay vì `git add .`); lược bỏ thông tin OAuth scopes nội bộ trong `integration-readiness-matrix.md` nếu dự định đưa tệp này lên kho công khai.
  2. Khuyến nghị di chuyển thư mục clone ngoài `rea/` ra một thư mục độc lập ngang hàng bên ngoài gốc kho `zalo` để triệt tiêu hoàn toàn nguy cơ `git add -f` nhầm `~9.100` tệp bên thứ ba.
- **Owner:** Release Captain + Windows QA Agent.
- **Trạng thái:** `OPEN`.

---

### `GIT-SEC-008` — Phơi nhiễm thông tin trinh sát vận hành (OAuth scopes của `gh`, mã băm tệp cấu hình Zalo đang chạy, commit chưa ký số và blob mồ côi chứa PII trong `.git/objects`)

- **ID:** `GIT-SEC-008`
- **Severity:** `P2`
- **Source:** Kiểm toán nội dung báo cáo CI/bằng chứng JSON, metadata chữ ký commit (`%G?`), và các đối tượng không thể truy cập (`unreachable objects`) trong `.git/objects`.
- **Evidence type:** Quét nội dung tệp JSON/Markdown kết hợp kiểm tra kho đối tượng Git mức thấp.
- **File/commit liên quan:**
  - Báo cáo CI trên các nhánh cục bộ (`build/cross-platform-distribution`, `integration/reliability-hardening`, `qa/windows-native-acceptance`): `desktop-instance-manager/plans/reports/build/ci-baseline-and-github-audit.md`, `desktop-instance-manager/docs/packaging/ci-execution-and-artifact-validation.md`, `desktop-instance-manager/docs/reviews/integration-and-defect-remediation.md` (dòng `247`) công bố đầu ra `gh auth status` và `gh api -i user` gồm danh sách OAuth scopes cụ thể của token đang đăng nhập trên máy chủ (`gist, read:org, repo`) cùng phản hồi API thanh toán.
  - Bằng chứng PoC đã public trên `origin/main` (`8cf4703`): `desktop-instance-manager/evidence/zalo-live-poc-result.json` công bố thông số phiên Zalo chính đang chạy trên máy chủ (`process_count: 11`, danh sách PID, `window_handle`) và mã băm SHA-256 của tệp thực tế `%APPDATA%\ZaloData\config.json`, `startup.log`, `log.log`.
  - Kho đối tượng cục bộ `.git/objects`: `100%` (`66/66`) commit chưa được ký GPG/SSH (`%G? = N`), và tồn tại `3` commit mồ côi (`0806ffb`, `ab04901`, `bbdef15`) kèm `2` blob mồ côi (`25454dd696`, `93a8af08c3`) vẫn lưu đường dẫn `C:/Users/<REDACTED_USER>/...`.
- **Tác động:** Không làm lộ giá trị token hay nội dung tệp cấu hình gốc, nhưng cung cấp thông tin trinh sát môi trường vận hành của chủ sở hữu kho; việc thiếu chữ ký số trên commit trên một kho công khai chưa bật Branch Protection (`GHSET-SEC-01`) làm giảm khả năng chống giả mạo danh tính người đóng góp; các blob mồ côi có thể bị lộ nếu sao lưu/chia sẻ trực tiếp thư mục `.git` dưới dạng tệp nén.
- **Khuyến nghị xử lý:**
  1. Biên tập gọn các báo cáo CI trước khi phát hành công khai để chỉ nêu kết luận trạng thái cổng kiểm tra thay vì dán nguyên văn header `X-Oauth-Scopes` và cấu hình `credential.helper` của máy cá nhân.
  2. Bật ký số commit (`commit.gpgsign = true` với khóa SSH hoặc GPG đã xác minh trên GitHub) kèm chế độ Vigilant Mode cho các commit phát hành chính thức.
  3. Không bao giờ đóng gói/chia sẻ nguyên thư mục `.git` cục bộ ra bên ngoài (chỉ giao tiếp qua `git push` có kiểm soát sau khi qua cổng kiểm toán).
- **Owner:** Repo Owner + Release Captain + Central Integration Agent.
- **Trạng thái:** `OPEN`.

---

## 6. Lộ Trình Xử Lý An Toàn Trước Khi Mở Cổng Phát Hành Công Khai (Không Viết Lại Lịch Sử `origin`)

Để xử lý triệt để toàn bộ các phát hiện `GIT-SEC-001`..`GIT-SEC-008` mà **tuân thủ nghiêm ngặt quy tắc không viết lại lịch sử (`no history rewrite`, không `push --force` lên `origin/main`)** và không khóa các worktree đang chạy song song:

1. **Bước 1 — Chặn đẩy trực tiếp các nhánh thành phần (`Immediate Push Freeze`):** Giữ nguyên trạng thái cục bộ cho toàn bộ các nhánh phụ (`build/*`, `feat/*`, `qa/*`, `research/*`, `test/*`); không push bất kỳ nhánh phụ nào lên `origin` để ngăn `426` blob cục bộ chứa đường dẫn `.gitconfig`/`.cargo`/worktree phát tán ra công khai (`GIT-SEC-002`).
2. **Bước 2 — Hoàn thiện làm sạch trên nhánh tích hợp trung tâm (`Forward Scrubbing`):**
   - Thực hiện `git rm multi-zalo-desktop/dist/MultiZaloDesktop.exe` (`GIT-SEC-003`).
   - Quét và thay thế nốt `3` tệp còn chứa đường dẫn `C:\Users\<REDACTED_USER>\...` của bên thứ ba / máy build upstream (`GIT-SEC-005`) và lược bỏ chi tiết `X-Oauth-Scopes` / `credential.helper` trong các báo cáo đóng gói (`GIT-SEC-008`).
   - Bổ sung bộ quy tắc `.gitignore` đầy đủ cho khóa ký, cơ sở dữ liệu phiên runtime, và tệp thực thi/gói cài đặt (`GIT-SEC-006`).
3. **Bước 3 — Hợp nhất gộp sạch (`Squash Merge`) lên nền `origin/main`:** Khi tất cả cổng kiểm toán đạt yêu cầu, tạo một commit gộp sạch duy nhất từ trạng thái cây nguồn đã kiểm chứng lên nhánh `main` (nối tiếp trực tiếp sau `origin/main` @ `8cf4703`) thay vì đẩy toàn bộ `54` commit lịch sử trung gian. Cách này vừa giữ nguyên tính toàn vẹn lịch sử công khai của `8cf4703`, vừa làm sạch hoàn toàn `HEAD` trên `origin/main` và giữ `0` rò rỉ mới từ quá trình phát triển đa tác nhân.
4. **Bước 4 — Thu dọn nhánh phụ công khai & xác nhận rủi ro tồn dư:** Sau khi `main` mới đã lên `origin/main`, xóa nhánh từ xa không còn cần thiết `origin/feat/ui-vi-en-quiet-workspace` (sau khi chủ sở hữu kho xác nhận) và lưu hồ sơ chấp nhận rủi ro tồn dư đối với đường dẫn tên người dùng cục bộ trong commit khởi tạo `8cf4703` (`GIT-SEC-001`, `GIT-SEC-004`).
