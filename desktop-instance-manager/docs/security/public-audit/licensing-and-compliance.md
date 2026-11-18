# Báo cáo Kiểm toán Quản trị Mã nguồn mở, Giấy phép Phụ thuộc & Ranh giới Sở hữu Trí tuệ VNG/Zalo

- **Kho mã nguồn kiểm toán:** `https://github.com/manhquydev/zalo` (vừa chuyển trạng thái `PRIVATE -> PUBLIC`)
- **Nhánh kiểm toán:** `audit/public-repository-security` (worktree độc lập `C:\Users\<REDACTED_USER>\Downloads\zalo-public-security-audit`)
- **Phạm vi kiểm toán (Mục 6 — Mission F):**
  1. Quản trị mã nguồn mở (`LICENSE`, `SECURITY.md`, `CONTRIBUTING.md`, `NOTICE` / `THIRD_PARTY_NOTICES.md`) đối chiếu với tuyên bố trong `Cargo.toml`, `deny.toml`, `package.json`, `tauri.conf.json`, `README.md` và tài liệu hiện có (`docs/platform/ubuntu/security-and-license.md`, `docs/packaging/*.md`).
  2. Giấy phép toàn bộ cây phụ thuộc Rust (`Cargo.toml`, `Cargo.lock`, `deny.toml`), JS/Node (`package.json`, `.mjs`), v1 C#/.NET (`multi-zalo-desktop/`) và thư viện hệ thống/đóng gói trên Windows, macOS, Ubuntu Linux (`LGPL`, `MPL-2.0`, `.deb` vs `.AppImage`).
  3. Ranh giới sở hữu trí tuệ (IP), nhãn hiệu thương mại ("Zalo" — VNG Corporation), định danh ứng dụng, tiêu đề cửa sổ, biểu tượng (`.png`, `.ico`, `.icns`, `<svg>`) và quyền phân phối lại đối với từng thư mục (`desktop-instance-manager/`, `multi-zalo-desktop/`, `rea/`, `evidence/`, `fixtures/`).
- **Chế độ thực thi:** `100% READ-ONLY` (`git --no-optional-locks`) trên toàn bộ `zalo` và 8 worktree nhánh cục bộ. **Không** tự ý tạo tệp `LICENSE` và **không** tự ý áp đặt giấy phép cho toàn bộ kho mã nguồn khi chưa tách bạch các thành phần `LICENSE_REVIEW_REQUIRED`.

---

## 1. Tổng quan Bối cảnh Git & Trạng thái Phơi nhiễm Pháp lý Hiện tại

| Nhóm tham chiếu / Worktree | Nhánh & Commit HEAD | Trạng thái Public trên GitHub | Tóm tắt hiện trạng giấy phép & quản trị |
| :--- | :--- | :---: | :--- |
| **`origin/main`** | `origin/main` (`8cf4703`) | **ĐÃ PUBLIC** | `106` tệp đã đẩy công khai: **không có `LICENSE`**, **không có `SECURITY.md`**, **không có `CONTRIBUTING.md`**, **không có `NOTICE`**; `Cargo.toml` tuyên bố `MIT OR Apache-2.0` và `repository = "https://github.com/local/desktop-instance-manager"`; chứa sẵn nhị phân `multi-zalo-desktop/dist/MultiZaloDesktop.exe` (`59.392 B`) và `10` tệp `multi-zalo-desktop/evidence/rea-*.json` chưa có giấy phép. |
| **`origin/feat/ui-vi-en-quiet-workspace`** (`zalo-ui-worktree`) | `feat/ui-vi-en-quiet-workspace` (`0f87302`) | **ĐÃ PUBLIC** | `138` tệp đã đẩy công khai: bổ sung giao diện đa ngôn ngữ `Multi-Zalo` và `17` ảnh chụp màn hình UI (`docs/design/screenshots/*.png`), vẫn thiếu toàn bộ tệp quản trị OSS (`LICENSE`, `SECURITY.md`, `CONTRIBUTING.md`, `NOTICE`) và thiếu `MPL-2.0` trong `deny.toml`. |
| **`zalo` (Primary Worktree)** | `main` (`a5feb4a`, ahead `1`) | Cục bộ (chưa push commit `a5feb4a`) | Bổ sung `apps/desktop/src-tauri/Cargo.lock` (`434` gói, gồm `7` crate `MPL-2.0`), `MPL-2.0` trong `deny.toml` (nhưng còn `version = 2`), `evidence/sbom-cyclonedx.json`, và chứa thư mục ngoài `rea/` (`morluto/rea` v6.9.0, được chặn bởi `.gitignore:2`). |
| **`zalo-integration`** & **`zalo-windows-acceptance`** | `integration/reliability-hardening` (`bff0674`) / `qa/windows-native-acceptance` (`004ab7c`) | Cục bộ | Hợp nhất sửa lỗi `deny.toml` (gỡ `version = 2`), thêm `publish = false` trên `src-tauri/Cargo.toml`, thêm bộ biểu tượng macOS/Retina (`icon.icns`, Retina `256x256` `.png`), tài liệu `docs/packaging/*.md` và `docs/platform/ubuntu/security-and-license.md`, cùng 2 workspace kiểm thử độc lập (`fixtures/cross-platform`, `tests/cross-platform`). Vẫn chưa có `LICENSE`, `SECURITY.md`, `CONTRIBUTING.md`, `THIRD_PARTY_NOTICES.md`. |
| **`zalo-cross-platform-build`** | `build/cross-platform-distribution` (`6c4150e`) | Cục bộ | Chứa `.github/workflows/cross-platform-validation.yml`, bộ công cụ `scripts/ci-cross-platform/*.mjs` (`dependency-audit.mjs`, `build-and-package.mjs`, `tauri-ci-bundle-overlay.json`) và `plans/reports/build/ir-patches/0001..0007`. |
| **`zalo-macos-worktree`**, **`zalo-ubuntu-worktree`**, **`zalo-compatibility-research`**, **`zalo-cross-platform-tests`** | `f2b8f9f`, `62dc5d1`, `0e5d44c`, `5e802e0` | Cục bộ | Cùng thiếu tệp `LICENSE`/`SECURITY.md`/`CONTRIBUTING.md`/`NOTICE`; `zalo-ubuntu-worktree` chứa `docs/platform/ubuntu/security-and-license.md` khẳng định workspace dùng `MIT OR Apache-2.0` dù tệp văn bản giấy phép chưa tồn tại. |

---

## 2. Kiểm toán Quản trị Mã nguồn mở (OSS Governance Audit)

### 2.1. Đối chiếu Tệp Quản trị Thực tế so với Tuyên bố trong Manifest & Tài liệu

| Hạng mục quản trị | Hiện trạng tại Root (`/`), `desktop-instance-manager/`, `multi-zalo-desktop/` | Hiện trạng trong `rea/` (ngoài Git, `.gitignore:2`) | Tuyên bố hiện tại trong Manifest / Tài liệu | Đánh giá Tuân thủ |
| :--- | :--- | :--- | :--- | :---: |
| **`LICENSE` / `LICENSE-MIT` / `LICENSE-APACHE`** | **KHÔNG TỒN TẠI** (`0` tệp trên mọi nhánh `origin/*` và mọi worktree) | Có `rea/LICENSE` (`MIT`, `Copyright (c) 2026 morluto` — thuộc dự án ngoài) | `desktop-instance-manager/Cargo.toml`, `src-tauri/Cargo.toml`, `fixtures/cross-platform/Cargo.toml`, `tests/cross-platform/Cargo.toml` và `docs/platform/ubuntu/security-and-license.md` đều ghi `license = "MIT OR Apache-2.0"`. `multi-zalo-desktop/` không ghi bất kỳ giấy phép nào. | **VI PHẠM NGHIÊM TRỌNG (`P0` — `LIC-SEC-001`)** |
| **`SECURITY.md`** | **KHÔNG TỒN TẠI** tại `/` hoặc `.github/SECURITY.md` | Có `rea/SECURITY.md` (của upstream `morluto/rea`) | `docs/security/threat-model-and-audit.md` mô tả mô hình đe dọa kỹ thuật nội bộ nhưng không cung cấp kênh báo cáo lỗ hổng bảo mật riêng tư cho cộng đồng. | **THIẾU (`P1` — `LIC-SEC-006`)** |
| **`CONTRIBUTING.md`** | **KHÔNG TỒN TẠI** | Có `rea/CONTRIBUTING.md` (của upstream `morluto/rea`) | Không có quy định đóng góp sạch (clean-room), không có điều khoản DCO/CLA, chưa có quy tắc cấm gửi PR chứa mã trích xuất từ `app.asar`, nhị phân VNG hoặc dữ liệu phiên Zalo thật. | **THIẾU (`P1` — `LIC-SEC-006`)** |
| **`NOTICE` / `THIRD_PARTY_NOTICES.md`** | **KHÔNG TỒN TẠI** | Có `rea/THIRD_PARTY_NOTICES.md` & `rea/licenses/*` | Chưa có tệp ghi nhận bản quyền bên thứ ba và nguồn tải mã nguồn cho `7` crate `MPL-2.0` hoặc các thư viện `LGPL` khi đóng gói `.AppImage` trên Linux. | **THIẾU (`P1` — `LIC-SEC-004`)** |
| **Metadata đóng gói & kho nguồn** | `tauri.conf.json` thiếu `bundle.license`, `bundle.licenseFile`, `bundle.copyright`; `package.json` (`test-desktop-app`) thiếu `"license"` | Không áp dụng | `origin/main` và `origin/feat/ui-vi-en-quiet-workspace` vẫn khai báo `repository = "https://github.com/local/desktop-instance-manager"` trong `desktop-instance-manager/Cargo.toml`. | **KHIẾM KHUYẾT (`P2` — `LIC-SEC-007`)** |

### 2.2. Phân tích Hệ quả Pháp lý của Mâu thuẫn "Tuyên bố `MIT OR Apache-2.0` nhưng Không có Tệp `LICENSE`"
1. **Mặc định Luật Bản quyền trên GitHub Public Repo:** Theo Điều khoản Dịch vụ của GitHub (GitHub Terms of Service, Mục D.5) và luật sở hữu trí tuệ hiện hành, một kho mã nguồn công khai **không có tệp `LICENSE`** mặc định giữ nguyên chế độ **"All Rights Reserved"** (người dùng bên ngoài chỉ được phép xem và fork trên GitHub, không có quyền hợp pháp để sử dụng, sửa đổi, biên dịch hoặc phân phối lại).
2. **Không thỏa điều kiện hiệu lực của chính MIT và Apache-2.0:** Cả giấy phép MIT (*"The above copyright notice and this permission notice shall be included in all copies..."*) lẫn Apache-2.0 (Mục 4(a) yêu cầu đính kèm bản sao giấy phép và thông báo bản quyền của chủ sở hữu) đều đòi hỏi văn bản giấy phép đầy đủ kèm năm và chủ thể nắm giữ bản quyền. Khai báo chuỗi SPDX `"MIT OR Apache-2.0"` trong `Cargo.toml` mà không có tệp `LICENSE-MIT` và `LICENSE-APACHE` khiến trạng thái cấp phép bị treo pháp lý (ambiguous grant).
3. **Không thể đặt một tệp `LICENSE` duy nhất phủ toàn bộ thư mục gốc ngay lúc này:** Kho mã nguồn hiện đang trộn lẫn mã nguồn tự viết (`desktop-instance-manager/crates/**`, `apps/**`) với nhị phân biên dịch sẵn chưa rõ giấy phép (`multi-zalo-desktop/dist/MultiZaloDesktop.exe`) và các tệp đầu ra phân tích nhị phân VNG (`multi-zalo-desktop/evidence/rea-*.json`). Áp đặt mù quáng giấy phép mã nguồn mở lên toàn bộ gốc repo trước khi tách/loại trừ các tệp `LICENSE_REVIEW_REQUIRED` sẽ cấp phép sai phạm vi đối với dữ liệu không đủ điều kiện cấp phép OSS.

### 2.3. Đánh giá Tài liệu Hiện có `docs/platform/ubuntu/security-and-license.md`
- **Điểm mạnh đã xác minh (`PASS` về mặt ranh giới kỹ thuật):** Tài liệu tại `zalo-ubuntu-worktree` (`62dc5d1`) và `zalo-integration` (`bff0674`) đã phân tách chính xác các thành phần của bản port cộng đồng `VN-Linux-Family/zalo-for-linux` (`BSD-3-Clause` cho script/wrapper so với **Proprietary — VNG Corporation** đối với `app.asar`, `ZaloCall`, `models.zmdl`, và `unfree` đối với gói `.AppImage` tổng hợp), đồng thời thực thi nguyên tắc **nghiêm cấm sao chép, tải về, trích xuất hoặc đóng gói lại bất kỳ byte nhị phân nào của VNG hay `.AppImage` cộng đồng**.
- **Điểm hổng cần hiệu chỉnh:** Dòng đầu tài liệu ghi `Workspace License: MIT OR Apache-2.0 (Cargo.toml, deny.toml)` như một sự việc đã hoàn tất, trong khi tệp `LICENSE-MIT` / `LICENSE-APACHE` chưa hề tồn tại và nghĩa vụ `LGPL` của chính gói **manager `.AppImage`** (`dim-desktop` nhúng `libwebkit2gtk-4.1`, `libgtk-3`, `libayatana-appindicator3`) chưa được đề cập.

---

## 3. Kiểm toán Giấy phép Phụ thuộc (Dependency Licensing Audit)

### 3.1. Tổng hợp Toàn bộ Manifest & Lockfile trên Tất cả Nhánh/Worktree

Quá trình kiểm toán đã đối chiếu trực tiếp toàn bộ `Cargo.lock` với siêu dữ liệu gốc `Cargo.toml` của từng crate trong bộ đệm `~/.cargo/registry/` (`missing_in_cache = 0`, độ phủ **100%** trên mọi crate đã khóa mà không chạm lock của Git/Cargo):

| Thành phần / Lockfile | Nhánh / Worktree hiện diện | Tổng số gói khóa | Phân bổ nguồn | Tóm tắt rủi ro giấy phép |
| :--- | :--- | :---: | :--- | :--- |
| **`desktop-instance-manager/Cargo.lock`** | `main`, `zalo-integration`, `zalo-cross-platform-build`, `zalo-macos-worktree`, `zalo-ubuntu-worktree`, `zalo-compatibility-research`, `zalo-cross-platform-tests`, `zalo-windows-acceptance` *(vắng mặt trên `origin/main` & `zalo-ui-worktree`)* | **`41`** | `7` crate nội bộ + `34` crate `crates.io` | **100% Permissive** (`MIT`, `Apache-2.0`, `Unicode-3.0`, `Unlicense`, và `r-efi@5.3.0` đa giấy phép `MIT OR Apache-2.0 OR LGPL-2.1-or-later` chọn nhánh `MIT`/`Apache-2.0`). `0` `GPL`/`AGPL`, `0` `MPL-2.0`. |
| **`desktop-instance-manager/apps/desktop/src-tauri/Cargo.lock`** | Như trên (`434` gói; tổng hợp với `Cargo.lock` là **`439` gói duy nhất**) | **`434`** | `7` crate nội bộ + `427` crate `crates.io` | **`0` `GPL`/`AGPL`**, **`0` crate không phép**; có **`7` crate chỉ cấp phép `MPL-2.0`** (copyleft yếu cấp tệp) và **liên kết động tới hệ sinh thái `WebKit2GTK`/`GTK3`/`Ayatana` (`LGPL-2.0+/2.1+/3.0`) trên Linux**. |
| **`desktop-instance-manager/fixtures/cross-platform/Cargo.lock`** | `zalo-integration`, `zalo-cross-platform-tests`, `zalo-windows-acceptance` | **`36`** | `2` crate nội bộ + `34` crate `crates.io` | **100% Permissive**; tuy nhiên khai báo `[workspace]` độc lập và **chưa được kiểm tra bởi `cargo-deny` trong mọi workflow CI**. |
| **`desktop-instance-manager/tests/cross-platform/Cargo.lock`** | `zalo-integration`, `zalo-cross-platform-tests`, `zalo-windows-acceptance` | **`42`** | `8` crate nội bộ + `34` crate `crates.io` | **100% Permissive**; được kiểm tra trong `cross-platform-validation.yml` nhưng chưa có trong `ci.yml`. |
| **JS / Node.js (`fixtures/test-desktop-app/package.json`, `apps/desktop/ui/*`, `scripts/*.mjs`)** | Tất cả nhánh/worktree | **`0` gói npm runtime** | Chỉ dùng API chuẩn của trình duyệt và mô-đun tích hợp sẵn của Node.js `>= 20/22` | Không có phụ thuộc runtime bên thứ ba (`node_modules` không sử dụng cho UI/bridge). Trong CI đóng gói chỉ cài công cụ build `@tauri-apps/cli@2.12.5` (`Apache-2.0 OR MIT`) ở chế độ công cụ dòng lệnh, không nhúng vào mã sản phẩm. `package.json` của fixture đặt `"private": true` nhưng thiếu trường `"license"`. |
| **v1 C#/.NET (`multi-zalo-desktop/src/**` & `scripts/build.ps1`)** | Tất cả nhánh/worktree (kể cả `origin/main`) | **`0` gói NuGet** | Biên dịch bằng `csc.exe` với `.NET Framework 4.x` (`System.*`, WPF) của Windows | Không kéo gói bên thứ ba, nhưng sản phẩm biên dịch `dist/MultiZaloDesktop.exe` đang bị theo dõi trực tiếp trong Git mà không kèm giấy phép. |

### 3.2. Phân bổ Chi tiết Biểu thức Giấy phép SPDX trong `apps/desktop/src-tauri/Cargo.lock` (`434` gói)

| Biểu thức SPDX trong `Cargo.toml` của Crate | Số lượng | Phân loại Pháp lý | Ghi chú Tuân thủ khi Phân phối Nhị phân |
| :--- | :---: | :--- | :--- |
| `MIT OR Apache-2.0` / `Apache-2.0 OR MIT` / `MIT/Apache-2.0` / `Apache-2.0/MIT` / `Apache-2.0 / MIT` | `245` | Permissive (chọn 1 trong 2) | Yêu cầu giữ bản sao giấy phép và dòng bản quyền theo nhánh được chọn. |
| `MIT` | `104` | Permissive | Bao gồm các crate FFI hệ thống Linux (`webkit2gtk-sys`, `gtk-sys`, `glib-sys`, `soup3-sys`, `libappindicator-sys`, v.v.); lưu ý bản thân crate Rust là `MIT` nhưng thư viện `.so` đích trên Linux là `LGPL` (xem Mục 3.3). |
| `Unicode-3.0` (`18`) & `(MIT OR Apache-2.0) AND Unicode-3.0` (`1`) | `19` | Permissive (Unicode) | Đính kèm văn bản giấy phép `Unicode-3.0` trong `THIRD_PARTY_NOTICES.md`. |
| `Zlib OR Apache-2.0 OR MIT` (`17`), `MIT OR Zlib OR Apache-2.0` (`2`), `MIT OR Apache-2.0 OR Zlib` (`2`), `Zlib` (`2`) | `23` | Permissive | Tương thích hoàn toàn với cả phân phối mã nguồn và nhị phân. |
| `Unlicense OR MIT` (`9`) & `Unlicense/MIT` (`2`) | `11` | Permissive / Public Domain | Thỏa mãn qua nhánh `MIT` (nên thêm `Unlicense` vào `deny.toml` để tường minh). |
| **`MPL-2.0`** | **`7`** | **Weak Copyleft (cấp tệp)** | Gồm `cssparser@0.37.0`, `cssparser@0.38.0`, `cssparser-macros@0.7.1`, `dtoa-short@0.3.5`, `option-ext@0.2.0`, `selectors@0.38.0`, `selectors@0.41.0` (xem Mục 3.3). |
| `WORKSPACE_LOCAL` (`dim-*`) | `7` | Nội bộ dự án | Tuyên bố `MIT OR Apache-2.0` trong `Cargo.toml` nhưng thiếu tệp `LICENSE`. |
| `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` (`3`), `Apache-2.0 WITH LLVM-exception` (`1`), `Apache-2.0` (`2`), `Apache-2.0 AND MIT` (`1`) | `7` | Permissive | Đã quét toàn bộ `427` thư mục crate trong `~/.cargo/registry/src/`: **`0` crate đính kèm tệp `NOTICE` riêng** theo Apache-2.0 Mục 4(d), nhưng vẫn phải kèm bản sao giấy phép Apache-2.0 theo Mục 4(a). |
| `BSD-3-Clause` (`2`), `BSD-3-Clause OR MIT OR Apache-2.0` (`2`), `BSD-3-Clause AND MIT` (`1`), `BSD-3-Clause/MIT` (`1`) | `6` | Permissive | Yêu cầu giữ thông báo bản quyền `BSD-3-Clause` khi phân phối nhị phân (`THIRD_PARTY_NOTICES.md`). |
| `MIT OR Apache-2.0 OR LGPL-2.1-or-later` | `2` | Đa giấy phép (`r-efi@5.3.0`, `r-efi@6.0.0`) | Được phép chọn nhánh `MIT` hoặc `Apache-2.0` — **không kích hoạt nghĩa vụ `LGPL-2.1`**. |
| `0BSD OR MIT OR Apache-2.0` (`adler2@2.0.1`), `CC0-1.0 OR MIT-0 OR Apache-2.0` (`dunce@1.0.5`), `ISC` (`1`) | `3` | Permissive | Thỏa mãn qua nhánh `MIT`/`Apache-2.0`/`ISC`. |

### 3.3. Phân tích Nghĩa vụ Copyleft (`MPL-2.0` & Linux `LGPL` Bundling)

1. **Copyleft mạnh (`GPL-*` / `AGPL-*`):** Xác nhận **`0` crate** trong toàn bộ 4 tệp `Cargo.lock` mang giấy phép `GPL` hoặc `AGPL`. Dự án không dùng `openssl`/`openssl-sys` (đã chặn trong `[bans]` của `deny.toml`).
2. **Copyleft yếu cấp tệp (`MPL-2.0` — `7` crate trong `src-tauri`):**
   - Các crate `cssparser`, `cssparser-macros`, `selectors`, `dtoa-short`, `option-ext` được biên dịch tĩnh vào `dim-desktop` (thông qua `tauri`/`tauri-utils`).
   - Theo **MPL-2.0 Mục 1.7, 3.2(a) và 3.3**, việc liên kết tĩnh các crate `MPL-2.0` nguyên bản (không sửa đổi mã nguồn của chúng) vào một tác phẩm lớn hơn (`Larger Work`) **không** buộc mã nguồn của `desktop-instance-manager` phải đổi sang `MPL-2.0`.
   - **Tuy nhiên**, Mục 3.2(a) bắt buộc khi phân phối tệp thực thi (`dim-desktop.exe`, `.msi`, `*-setup.exe`, `.dmg`, `.deb`, `.AppImage`), nhà phân phối **phải đính kèm thông báo chỉ rõ nơi người nhận có thể lấy dạng Mã nguồn (Source Code Form) của 7 crate `MPL-2.0` đó** (ví dụ liên kết crates.io/upstream kèm bản sao văn bản `MPL-2.0` trong `THIRD_PARTY_NOTICES.md`). Hiện tại dự án chưa có tệp này.
3. **Nghĩa vụ `LGPL-2.0+` / `LGPL-2.1+` / `LGPL-3.0` khi đóng gói trên Ubuntu Linux (`.deb` so với `.AppImage`):**
   - Trên Linux (`ubuntu-2404-build`), `dim-desktop` liên kết động tới `libwebkit2gtk-4.1-0` (`LGPL-2.1+` / `BSD-2-Clause`), `libgtk-3-0t64` (`LGPL-2.0+`), `libglib2.0-0t64` (`LGPL-2.1+`), `libsoup-3.0-0` (`LGPL-2.0+`), `librsvg2-2` (`LGPL-2.1+`) và `libayatana-appindicator3-1` (`LGPL-2.1` / `LGPL-3.0`).
   - **Với gói `.deb`:** Gói `.deb` chỉ khai báo `Depends:` tới thư viện chia sẻ có sẵn của hệ điều hành Ubuntu (`libwebkit2gtk-4.1-0`, `libgtk-3-0t64`, `libayatana-appindicator3-1`) và không phân phối kèm tệp `.so` của bên thứ ba. Cơ chế liên kết động chuẩn ELF thỏa mãn quyền thay thế thư viện của người dùng theo `LGPL-2.1` Mục 6(b), với điều kiện gói `.deb` có tệp `/usr/share/doc/dim-desktop/copyright` đầy đủ.
   - **Với gói `.AppImage` (`build-and-package.mjs --bundles all` bao gồm `appimage`):** Trình đóng gói AppImage (`linuxdeploy`) **sao chép và đóng gói trực tiếp các tệp `.so` mang giấy phép `LGPL-2.0+`/`LGPL-2.1+`/`LGPL-3.0` vào bên trong ảnh SquashFS `.AppImage`**. Khi phát hành tệp `.AppImage` này ra công chúng, chủ dự án trở thành bên phân phối nhị phân thư viện `LGPL` và bắt buộc phải tuân thủ `LGPL-2.1` Mục 4 & Mục 6 (cũng như `LGPL-3.0` Mục 4 đối với Ayatana): (i) kèm văn bản giấy phép `LGPL-2.0`/`2.1`/`3.0` và bản quyền, (ii) bảo toàn khả năng giải nén `--appimage-extract` để người dùng thay thế `.so` liên kết động, và (iii) **lưu trữ hoặc kèm lời đề nghị bằng văn bản (hiệu lực tối thiểu 3 năm) cung cấp đúng gói mã nguồn upstream tương ứng (`apt-get source`) của các thư viện `.so` đã nhúng** (chỉ dẫn chung chung tới kho Ubuntu không đủ pháp lý khi mirror Ubuntu thay đổi phiên bản). Nếu chưa thiết lập quy trình lưu trữ source tarball cho AppImage, chỉ nên phát hành `.deb` (hoặc mã nguồn) và tắt xuất bản `.AppImage` công khai.

### 3.4. Lỗ hổng Cấu hình `deny.toml` & Độ phủ Kiểm tra trong CI

1. **Lỗi tương thích `version = 2` trên `cargo-deny >= 0.20`:** Trên `origin/main` (`8cf4703`), `origin/feat/ui-vi-en-quiet-workspace` (`0f87302`), `main` (`a5feb4a`), `zalo-cross-platform-build`, `zalo-macos-worktree`, `zalo-ubuntu-worktree`, `zalo-compatibility-research` và `zalo-cross-platform-tests`, tệp `desktop-instance-manager/deny.toml` vẫn chứa khóa cấp cao nhất `version = 2` đã bị loại bỏ từ `cargo-deny 0.20+` (gây lỗi `unexpected-keys` khi chạy trực tiếp `cargo deny check` hoặc `EmbarkStudios/cargo-deny-action@v2`). Bản sửa mới chỉ nằm cục bộ tại `zalo-integration` (`bff0674`), `zalo-windows-acceptance` (`004ab7c`) và bản vá `0004-IR-004`.
2. **Thiếu `MPL-2.0` trên các nhánh đã Public (`origin/*`):** `deny.toml` trên `origin/main` và `origin/feat/ui-vi-en-quiet-workspace` chưa có `"MPL-2.0"` trong `allow = [...]`; ngay khi kiểm tra `apps/desktop/src-tauri/Cargo.toml`, `7` crate `MPL-2.0` sẽ làm lệnh kiểm tra thất bại.
3. **Phân mảnh 4 Workspace Rust độc lập & Cờ `publish = false`:**
   - `desktop-instance-manager/Cargo.toml` đặt `exclude = ["apps/desktop/src-tauri"]`; do đó chạy `cargo deny check` mặc định tại `desktop-instance-manager/` sẽ bỏ qua hoàn toàn `434` crate của Tauri!
   - Hai workspace kiểm thử độc lập `fixtures/cross-platform/Cargo.toml` (`36` gói) và `tests/cross-platform/Cargo.toml` (`42` gói) không có trong `.github/workflows/ci.yml`; riêng `fixtures/cross-platform/Cargo.toml` cũng chưa được gọi `cargo-deny` trong `cross-platform-validation.yml` hay trong `dependency-audit.mjs`.
   - `deny.toml` bật `wildcards = "deny"`, nhưng các crate nội bộ (`dim-core`, `dim-profile-store`, `dim-process-manager`, `dim-platform-*`, `dim-test-fixture`) chưa khai báo `publish = false` (mới chỉ có `publish = false` trên `src-tauri/Cargo.toml` ở nhánh `zalo-integration`).

---

## 4. Ranh giới Sở hữu Trí tuệ (IP), Nhãn hiệu VNG/Zalo & Quyền Phân phối lại

### 4.1. Ranh giới Kỹ thuật giữa Mã Quản lý của Dự án và Ứng dụng Độc quyền VNG Zalo
- **Xác minh Kiến trúc Sạch (`CLEAN_INTEROPERABILITY_BOUNDARY`):** Kiểm tra toàn bộ mã nguồn Rust (`crates/*`, `apps/desktop/src-tauri/src/*`), PowerShell (`scripts/dim-host.ps1`), Web UI (`apps/desktop/ui/*`) và v1 C# (`multi-zalo-desktop/src/*`) xác nhận dự án chỉ thực hiện điều phối tiến trình hệ điều hành và cô lập biến môi trường/thư mục dữ liệu (`APPDATA`/`LOCALAPPDATA`/`TEMP`/`TMP` trên Windows; `CFFIXED_USER_HOME` trên macOS; `HOME`/`XDG_*` + `bwrap` trên Linux).
- **Không xâm phạm mã nhị phân VNG:** Kho mã nguồn chứa **`0` tệp nhị phân của VNG** (`Zalo.exe`, `ZaloSetup*.exe`, `Zalo.app`, `app.asar`, `ZaloCall.exe`, `.dll` của VNG đều bằng `0`), **không** giải nén hay vá `app.asar`, **không** tiêm DLL/hook bộ nhớ, **không** can thiệp giao thức hay vượt cơ chế xác thực của Zalo, và chủ động xác minh chữ ký số `WinVerifyTrust` (`CN/O=VNG GROUP JSC`) trước khi khởi chạy bản cài đặt hợp pháp có sẵn trên máy người dùng.

### 4.2. Đánh giá Nhãn hiệu Thương mại ("Zalo"), Tên Ứng dụng, Bundle ID & Biểu tượng

| Bề mặt định danh | Giá trị hiện tại trong Repo (`origin/*` & Worktrees) | Đánh giá Rủi ro Nhãn hiệu / Trade Dress | Khuyến nghị Chuẩn hóa theo Nominative Fair Use |
| :--- | :--- | :--- | :--- |
| **Đường dẫn Repo GitHub** | `https://github.com/manhquydev/zalo` (tên kho trùng khít nhãn hiệu gốc `zalo`) | **Cao (`P1`)** — Dễ gây hiểu nhầm là kho mã chính thức hoặc SDK của Zalo khi hiển thị trên kết quả tìm kiếm. | Đổi tên repo thành dạng mô tả công cụ độc lập (ví dụ `desktop-instance-manager` hoặc `instance-manager-for-zalo`; GitHub tự chuyển hướng git/web hoặc cập nhật remote có kiểm soát) và ghi rõ phi liên kết ở dòng đầu `README.md`. |
| **Tên sản phẩm & Tiêu đề cửa sổ** | `"productName": "Multi-Zalo Desktop"`, `"title": "Multi-Zalo Desktop — Instance Manager"` (`tauri.conf.json`); `"Multi-Zalo"` (`ui/index.html`, `vi-VN.json`, `en-US.json`); `"Multi-Zalo Desktop"` (`MainWindow.cs`, `MultiZaloDesktop.exe`) | **Trung bình – Cao (`P1`)** — Đặt nhãn hiệu đã đăng ký `"Zalo"` làm danh từ chính của tên thương hiệu sản phẩm (`Multi-Zalo`) thay vì dùng giới từ mô tả tính tương thích (*"... for Zalo Desktop"*). | Đổi tên thương hiệu chính sang tên riêng của dự án kèm cụm mô tả tương thích hợp pháp: ví dụ **`Desktop Instance Manager (for Zalo Desktop)`** hoặc **`DIM — Multi-Account Launcher for Zalo Desktop`**. |
| **Định danh gói (`identifier`)** | `"com.multizalodesktop.manager"` (`tauri.conf.json` trên mọi nhánh) | **Trung bình – Cao (`P1`)** — Vừa nhúng nhãn hiệu `zalo` vào reverse-DNS App ID (macOS `CFBundleIdentifier`, Windows AppUserModelID/Uninstall key, Linux `.desktop`), vừa dùng tên miền `multizalodesktop.com` chưa thuộc quyền sở hữu xác thực. | Chuyển sang reverse-DNS thuộc quyền kiểm soát thực tế của chủ dự án và không chứa chuỗi `zalo`, ví dụ `com.<owner-domain>.dim` hoặc tên miền riêng của tác giả. |
| **Tuyên bố phi liên kết (Disclaimer)** | **`0` lần xuất hiện** trong toàn bộ `README.md`, `ui/index.html`, `vi-VN.json`, `en-US.json`, `Settings -> About`, `Help` | **Cao (`P1`)** — Thiếu điều kiện thứ 3 của học thuyết *Nominative Fair Use* (khẳng định rõ không có sự tài trợ, liên kết hay chứng thực từ chủ sở hữu nhãn hiệu VNG Corporation). | Bổ sung tuyên bố song ngữ Việt/Anh nổi bật ở đầu `README.md`, `desktop-instance-manager/README.md`, màn hình `Cài đặt -> Thông tin ứng dụng` và hộp thoại cài đặt: *"Dự án độc lập, không liên kết, không được tài trợ hay chứng thực bởi VNG Corporation hay Zalo. 'Zalo' là nhãn hiệu của VNG Corporation."* |
| **Biểu tượng ứng dụng & Huy hiệu UI** | `icons/32x32.png` (`115 B`), `128x128.png` / `icon.png` (`402 B`), Retina `256x256` `.png` (`716 B`), `icon.ico` (`137 B`), `icon.icns` (`1.142 B`); `<svg>` trong `ui/index.html`; huy hiệu chữ `"Z"` (`ui/index.html:154,446`) | **Thấp về Bản quyền / Trung bình về Trade Dress (`P2`)** — Kiểm tra giải mã pixel xác nhận 100% tệp icon chỉ là hình vuông tổng hợp 2 màu thuần (`#0057D8` nền + `#0C2D78` viền, **không phải** tài nguyên trích xuất từ PE của VNG) và `<svg>` là hình 2 ô cửa sổ chồng nhau. Tuy nhiên ô vuông xanh `#0057D8` đi kèm huy hiệu chữ `"Z"` và tên `"Multi-Zalo"` có sắc thái gần bộ nhận diện màu xanh chữ Z của Zalo. | Thay biểu tượng placeholder 2 màu bằng biểu tượng đồ họa gốc của dự án (ví dụ biểu tượng nhiều lớp cửa sổ/không gian làm việc đa sắc) và đổi huy hiệu `"Z"` mặc định sang ký hiệu trung tính (như chữ cái đầu tên tài khoản hoặc biểu tượng cửa sổ). |

### 4.3. Ma trận Phân loại Bản quyền, Quyền Phân phối lại & Đề xuất Cấp phép theo Từng Thành phần

> **Nguyên tắc bắt buộc:** Mọi thành phần chưa xác minh rõ nguồn gốc/quyền tái phân phối đều được gắn cờ tường minh **`LICENSE_REVIEW_REQUIRED`** (hoặc `NON_REDISTRIBUTABLE_EXTERNAL`). Các phương án giấy phép mã nguồn mở bên dưới **chỉ được đề xuất cho những thành phần 100% do dự án tự viết và sở hữu trọn vẹn**.

| Thành phần / Đường dẫn | Nguồn gốc & Chủ sở hữu | Hiện trạng trên `origin/*` | Phân loại Quyền Phân phối lại | Đề xuất Cấp phép / Hành động Xử lý |
| :--- | :--- | :---: | :--- | :--- |
| **`desktop-instance-manager/crates/**`**, **`apps/desktop/**`** *(trừ icon placeholder nếu thay mới)*, **`scripts/**`**, **`tests/**`**, **`fixtures/**`** | **100% mã gốc do dự án tự phát triển** (Rust, PowerShell, Node `.mjs`, HTML/CSS/JS thuần, tài liệu kỹ thuật kiến trúc) | Đã Public một phần (`origin/main`, `origin/feat/ui-...`) | **`PROJECT_WHOLLY_OWNED` (Đủ điều kiện cấp phép OSS sau khi tách/loại trừ các mục bên dưới)** | **Chỉ đề xuất cho chủ dự án lựa chọn (không tự áp đặt):**<br>• **Phương án A (Khuyến nghị cao nhất cho hệ sinh thái Rust/Tauri): Dual-license `MIT OR Apache-2.0`** (`LICENSE-MIT` + `LICENSE-APACHE`) — khớp 100% với khai báo hiện có trong mọi `Cargo.toml`, đồng thời nhờ **Apache-2.0 Mục 2 & 3** (bảo vệ bằng sáng chế) và **Mục 6** (tuyên bố không cấp quyền nhãn hiệu).<br>• **Phương án B: `Apache-2.0` đơn nhất** — bảo vệ pháp lý/nhãn hiệu/sáng chế chặt chẽ nhất.<br>• **Phương án C: `MIT` đơn nhất** — gọn nhẹ nhưng không có điều khoản sáng chế/nhãn hiệu tường minh (cần sửa lại `Cargo.toml` nếu chọn đơn nhất). |
| **`desktop-instance-manager/docs/design/screenshots/*.png`** (`17` tệp ảnh giao diện) | Ảnh chụp giao diện Web UI của dự án với dữ liệu giả lập | **Đã Public** trên `origin/feat/ui-vi-en-quiet-workspace` | **`LICENSE_REVIEW_REQUIRED`** *(chờ đối chiếu đồng bộ với kết quả quét hình ảnh của `@evidence-auditor`)* | Chỉ đưa vào phạm vi giấy phép tài liệu/mã nguồn của dự án sau khi xác nhận `0` thông tin cá nhân/tài khoản thật và cập nhật lại ảnh nếu đổi tên thương hiệu UI. |
| **`desktop-instance-manager/evidence/*.json`** (`artifact-manifest.json`, `sbom-cyclonedx.json`, `zalo-live-poc-result.json`, v.v.) | Tệp nhật ký kiểm thử runtime, SBOM và kết quả PoC chạy cùng Zalo `26.10.20` trên máy cục bộ | Đã Public `2` tệp (`fixture-test-result.json`, `zalo-live-poc-result.json`) trên `origin/main` | **`LICENSE_REVIEW_REQUIRED`** | Tách khỏi phạm vi cấp phép mã nguồn phần mềm (thêm tuyên bố loại trừ `evidence/**` trong `README.md`/`LICENSE` hoặc chuyển sang CI build artifacts lưu trữ ngắn hạn; làm sạch mọi đường dẫn máy trạm/dữ liệu phiên theo báo cáo của `@evidence-auditor`). |
| **`multi-zalo-desktop/src/**`**, **`scripts/*.ps1`**, **`tests/*.ps1`**, **`docs/**`** | Mã nguồn C#/.NET và tài liệu nghiên cứu v1 PoC do dự án viết | **Đã Public** trên `origin/main` | **`LICENSE_REVIEW_REQUIRED`** *(mã tự viết nhưng hiện chưa có bất kỳ khai báo giấy phép nào và gắn chặt với thư mục `multi-zalo-desktop/evidence/`)* | Chủ dự án cần quyết định: (1) lưu trữ riêng/gỡ thư mục lịch sử `multi-zalo-desktop/` ở commit tiếp theo trên `main`, hoặc (2) nếu giữ lại mã C# lịch sử để tham khảo thì phải bổ sung thông báo giấy phép tường minh cho riêng `multi-zalo-desktop/src/**` và đổi tên thương hiệu `MultiZaloDesktop`. |
| **`multi-zalo-desktop/dist/MultiZaloDesktop.exe`** (`59.392 B`) | Tệp thực thi PE `.NET` biên dịch sẵn, không chữ ký số, không nhúng thông tin giấy phép | **Đã Public** trên `origin/main` & `origin/feat/ui-...` | **`LICENSE_REVIEW_REQUIRED` (Không nên phân phối nhị phân dựng sẵn trong Git)** | Gỡ tệp nhị phân biên dịch sẵn `MultiZaloDesktop.exe` trong một commit tiến (forward commit, không viết lại lịch sử) và thêm `multi-zalo-desktop/dist/*.exe` vào `.gitignore`; người dùng chỉ tự biên dịch từ mã nguồn nếu cần. |
| **`multi-zalo-desktop/evidence/rea-*.json`** (`10` tệp phân tích PE imports/resources của `Zalo.exe`, `ZaloSetup-26.11.12.exe`, `app.asar`) | Đầu ra JSON tự động từ công cụ REA quét lên tệp thực thi độc quyền của VNG trên máy cục bộ (chứa cấu trúc offset/RVA/import và đường dẫn cục bộ `C:\Users\<REDACTED_USER>\...`) | **Đã Public** trên `origin/main` & `origin/feat/ui-...` | **`LICENSE_REVIEW_REQUIRED` (Không cấp phép OSS; chứa đường dẫn cục bộ & siêu dữ liệu nhị phân bên thứ ba)** | Dù không chứa mã nguồn JS giải dịch của `app.asar` hay bitmap icon thô, các tệp này phản ánh cấu trúc nhị phân độc quyền VNG và chứa đường dẫn cá nhân. Khuyến nghị **loại trừ hoàn toàn khỏi phạm vi giấy phép OSS** và gỡ khỏi nhánh `main` hiện hành (chỉ giữ bản tóm tắt kiến trúc sạch không chứa đường dẫn cá nhân). |
| **Thư mục gốc `rea/`** (chỉ có trong worktree `zalo`) | Bản clone độc lập của kho ngoài `https://github.com/morluto/rea.git` (`rea-agents` v6.9.0, HEAD `e9c72db`): lõi `MIT` (`Copyright (c) 2026 morluto`), nhưng `rea/licenses/` và `rea/third_party/` chứa tài liệu/thành phần `GPL-3.0`, `LGPL-2.0`, `Apache-2.0` (Ghidra), Frida, `pwndbg` | **Chưa từng Public** (`0` tệp trong Git nhờ `.gitignore:2` `rea/`) | **`THIRD_PARTY_EXTERNAL_REPO` (`LICENSE_REVIEW_REQUIRED` — Nghiêm cấm đưa vào repo)** | Giữ nguyên quy tắc chặn `rea/` trong `.gitignore`, **tuyệt đối không chạy `git add -f rea`**, và khuyến nghị chủ dự án di chuyển bản clone `rea/` ra thư mục ngang cấp bên ngoài `Downloads\zalo` để triệt tiêu nguy cơ đóng gói nhầm thành phần `GPL-3.0`/`LGPL-2.0`. |
| **Ứng dụng Zalo Desktop chính thức (`Zalo.exe`, `Zalo.app`, `app.asar`, `ZaloCall`) & bản port Linux cộng đồng (`zalo-for-linux` `.AppImage`)** | VNG Corporation (Độc quyền — All Rights Reserved) / Gói tổng hợp `unfree` | **Không có trong Repo** (`0` tệp — đã xác minh) | **`STRICTLY_FORBIDDEN_TO_REDISTRIBUTE`** | Duy trì vĩnh viễn cổng kiểm tra `scanForVngProprietaryBinaries` trong CI; người dùng tự cài đặt Zalo chính thức từ nguồn của VNG. |

---

## 5. Danh mục Phát hiện Kiểm toán Chi tiết (`LIC-SEC-*` — Đầy đủ 9 Trường Chuẩn)

### `LIC-SEC-001` — Thiếu hoàn toàn tệp văn bản giấy phép (`LICENSE` / `LICENSE-MIT` / `LICENSE-APACHE`) trên kho PUBLIC dù `Cargo.toml` tuyên bố `MIT OR Apache-2.0`

| Trường | Nội dung chi tiết |
| :--- | :--- |
| **ID** | `LIC-SEC-001` |
| **Severity** | `P0` |
| **Source** | Kiểm toán quản trị mã nguồn mở & đối chiếu manifest trên `origin/*` và toàn bộ 9 worktree |
| **Evidence type** | Kiểm tra cây tệp Git (`git ls-tree`) & nội dung manifest `Cargo.toml` / tài liệu |
| **File/commit liên quan** | `desktop-instance-manager/Cargo.toml` (`license = "MIT OR Apache-2.0"` tại `8cf4703`, `0f87302`, `a5feb4a`, `bff0674`), `desktop-instance-manager/apps/desktop/src-tauri/Cargo.toml`, `desktop-instance-manager/docs/platform/ubuntu/security-and-license.md` (`62dc5d1`), toàn bộ thư mục gốc `/` và `multi-zalo-desktop/` |
| **Tác động** | Kho mã nguồn đã chuyển sang `PUBLIC` nhưng không có bất kỳ tệp `LICENSE` nào khiến toàn bộ mã rơi vào trạng thái pháp lý mâu thuẫn: mặc định bị khóa quyền theo luật bản quyền ("All Rights Reserved"), vi phạm điều kiện đính kèm văn bản giấy phép của chính MIT và Apache-2.0 Mục 4(a), đồng thời để ngỏ trạng thái bản quyền của `multi-zalo-desktop/` đang nằm chung repo. Đây là rào cản pháp lý chặn trực tiếp cổng `PUBLIC_READINESS_PASS`. |
| **Khuyến nghị xử lý** | 1. Sau khi tách/loại trừ các đường dẫn `LICENSE_REVIEW_REQUIRED` (`multi-zalo-desktop/dist/`, `**/evidence/`), Chủ dự án quyết định chính thức giấy phép cho phần mã tự viết (khuyến nghị bộ đôi `LICENSE-MIT` + `LICENSE-APACHE` để khớp với `Cargo.toml`, hoặc cập nhật lại mọi `Cargo.toml` nếu chọn giấy phép đơn).<br>2. Ghi rõ trong `README.md` phạm vi tệp nào thuộc giấy phép mã nguồn mở và tệp bằng chứng nào bị loại trừ. |
| **Owner** | Project Owner + Release Captain |
| **Trạng thái** | `OPEN` (`BLOCKED` chờ quyết định giấy phép của Chủ dự án) |

---

### `LIC-SEC-002` — Rủi ro xung đột nhãn hiệu "Zalo" (VNG Corporation) trong tên ứng dụng, Bundle ID, tên kho GitHub và thiếu hoàn toàn tuyên bố phi liên kết

| Trường | Nội dung chi tiết |
| :--- | :--- |
| **ID** | `LIC-SEC-002` |
| **Severity** | `P1` |
| **Source** | Kiểm toán ranh giới sở hữu trí tuệ, nhãn hiệu thương mại & giao diện người dùng (UI/Packaging) |
| **Evidence type** | Phân tích cấu hình đóng gói, chuỗi đa ngôn ngữ UI, mã C# v1 và URL kho nguồn |
| **File/commit liên quan** | `https://github.com/manhquydev/zalo` (`8cf4703`, `0f87302`), `desktop-instance-manager/apps/desktop/src-tauri/tauri.conf.json` (`productName: "Multi-Zalo Desktop"`, `identifier: "com.multizalodesktop.manager"`), `desktop-instance-manager/apps/desktop/ui/index.html`, `desktop-instance-manager/apps/desktop/ui/i18n/vi-VN.json`, `desktop-instance-manager/apps/desktop/ui/i18n/en-US.json`, `multi-zalo-desktop/src/ui/MainWindow.cs` |
| **Tác động** | Sử dụng trực tiếp nhãn hiệu đã đăng ký `"Zalo"` làm tên kho GitHub (`zalo`), tên sản phẩm (`Multi-Zalo Desktop` / `Multi-Zalo`) và định danh gói (`com.multizalodesktop.manager`) mà **không có bất kỳ tuyên bố phi liên kết (Non-Affiliation Disclaimer) nào** trong README hay giao diện ứng dụng dễ gây nhầm lẫn về nguồn gốc thương mại hoặc sự chứng thực của VNG Corporation, làm tăng nguy cơ nhận khiếu nại nhãn hiệu/DMCA/takedown trên GitHub và các kho phân phối hệ điều hành. |
| **Khuyến nghị xử lý** | 1. Bổ sung ngay tuyên bố phi liên kết song ngữ Việt/Anh ở đầu `README.md`, `desktop-instance-manager/README.md`, mục `Settings -> About` và trình cài đặt.<br>2. Đổi tên hiển thị sản phẩm theo chuẩn *Nominative Fair Use* mô tả tính tương thích (ví dụ `Desktop Instance Manager — Multi-Account Launcher for Zalo Desktop`), đổi `identifier` sang namespace riêng không chứa từ `zalo` (ví dụ `com.<owner-domain>.dim`), và cân nhắc đổi tên repo GitHub sang `desktop-instance-manager`. |
| **Owner** | Project Owner + Account Dock/UI Owner + Central Integration |
| **Trạng thái** | `OPEN` |

---

### `LIC-SEC-003` — Các tệp nhị phân biên dịch sẵn và đầu ra phân tích REA (`LICENSE_REVIEW_REQUIRED`) đang nằm chung cây mã nguồn PUBLIC mà chưa tách phạm vi bản quyền

| Trường | Nội dung chi tiết |
| :--- | :--- |
| **ID** | `LIC-SEC-003` |
| **Severity** | `P1` |
| **Source** | Kiểm toán quyền phân phối lại trên `origin/main` (`8cf4703`), `origin/feat/ui-vi-en-quiet-workspace` (`0f87302`) và các worktree |
| **Evidence type** | Kiểm kê nhị phân PE và cấu trúc JSON đầu ra REA (`rea-pe-resources-*.json`, `rea-pe-imports-*.json`, `rea-audit-release-exposure-app-asar.json`) |
| **File/commit liên quan** | `multi-zalo-desktop/dist/MultiZaloDesktop.exe` (`8cf4703`), `multi-zalo-desktop/evidence/rea-*.json` (`10` tệp tại `8cf4703`), `desktop-instance-manager/evidence/*.json` (`8cf4703`, `a5feb4a`, `bff0674`) |
| **Tác động** | `MultiZaloDesktop.exe` là tệp thực thi PE không chữ ký số, không đính kèm thông báo bản quyền/giấy phép đang được phân phối trực tiếp qua Git công khai; các tệp `multi-zalo-desktop/evidence/rea-*.json` chứa bảng cấu trúc PE/offset của trình cài đặt VNG kèm đường dẫn hệ thống cục bộ `C:\Users\<REDACTED_USER>\...`. Việc để lẫn các thành phần `LICENSE_REVIEW_REQUIRED` này mà không loại trừ phạm vi cấp phép tạo rủi ro pháp lý và vệ sinh kho nguồn. |
| **Khuyến nghị xử lý** | 1. Thực hiện commit tiến (forward commit trên nhánh tích hợp/`main`, không rewrite lịch sử) để gỡ `multi-zalo-desktop/dist/MultiZaloDesktop.exe` khỏi cây làm việc hiện hành và chặn `*.exe` trong `.gitignore`.<br>2. Rà soát cùng `@evidence-auditor` để gỡ hoặc làm sạch `multi-zalo-desktop/evidence/rea-*.json` và `desktop-instance-manager/evidence/*.json`, đồng thời ghi rõ trong tệp giấy phép rằng mọi dữ liệu `evidence/` nằm ngoài phạm vi cấp phép phần mềm mã nguồn mở. |
| **Owner** | Project Owner + Central Integration |
| **Trạng thái** | `OPEN` |

---

### `LIC-SEC-004` — Chưa có `THIRD_PARTY_NOTICES.md` và quy trình đáp ứng nghĩa vụ `MPL-2.0` (`7` crate) cũng như `LGPL-2.0+/2.1+/3.0` khi đóng gói `.AppImage` trên Linux

| Trường | Nội dung chi tiết |
| :--- | :--- |
| **ID** | `LIC-SEC-004` |
| **Severity** | `P1` |
| **Source** | Kiểm toán `apps/desktop/src-tauri/Cargo.lock` (`434` gói) và pipeline đóng gói Linux (`build-and-package.mjs`, `docs/packaging/ubuntu.md`) |
| **Evidence type** | Phân tích SPDX toàn bộ crate trong `~/.cargo/registry/` & cấu hình đóng gói Tauri `.deb`/`.AppImage` |
| **File/commit liên quan** | `desktop-instance-manager/apps/desktop/src-tauri/Cargo.lock` (`a5feb4a`, `bff0674`), `desktop-instance-manager/scripts/ci-cross-platform/build-and-package.mjs` (`6c4150e`), `desktop-instance-manager/docs/packaging/ubuntu.md` (`6c4150e`, `bff0674`) |
| **Tác động** | (1) Bản build `dim-desktop` liên kết tĩnh `7` crate chỉ mang giấy phép `MPL-2.0` (`cssparser@0.37.0/0.38.0`, `cssparser-macros@0.7.1`, `dtoa-short@0.3.5`, `option-ext@0.2.0`, `selectors@0.38.0/0.41.0`): phân phối tệp thực thi mà không chỉ dẫn nơi nhận mã nguồn của chúng sẽ vi phạm **MPL-2.0 Mục 3.2(a)**.<br>(2) Trên Linux, chế độ `--bundles all` tạo gói `.AppImage` nhúng trực tiếp các thư viện `.so` `LGPL-2.0+/2.1+` (`libwebkit2gtk-4.1`, `libgtk-3`, `libglib-2.0`, `librsvg-2`) và `LGPL-2.1/3.0` (`libayatana-appindicator3`) vào ảnh SquashFS; phát hành `.AppImage` công khai mà không đính kèm văn bản `LGPL` và cam kết/lưu trữ gói mã nguồn upstream tương ứng sẽ vi phạm **LGPL-2.1 Mục 4 & 6 / LGPL-3.0 Mục 4**. |
| **Khuyến nghị xử lý** | 1. Tạo và nhúng tệp `desktop-instance-manager/THIRD_PARTY_NOTICES.md` vào mọi gói cài đặt, liệt kê toàn bộ bản quyền bên thứ ba (`MIT`, `Apache-2.0`, `BSD-3-Clause`, `ISC`, `Zlib`, `Unicode-3.0`) kèm văn bản `MPL-2.0` và liên kết mã nguồn của 7 crate `MPL-2.0`.<br>2. Đối với Linux: ưu tiên chỉ phân phối gói `.deb` (liên kết động tới thư viện hệ thống của Ubuntu) cho đến khi thiết lập kho lưu trữ tự động các gói mã nguồn tương ứng (`apt-get source`) và thông báo `LGPL-2.1`/`LGPL-3.0` cho gói `.AppImage`. |
| **Owner** | Build Agent + Release Captain |
| **Trạng thái** | `OPEN` |

---

### `LIC-SEC-005` — Sai lệch cấu hình và khoảng trống độ phủ của `deny.toml` giữa các nhánh (`version = 2`, thiếu `MPL-2.0` trên `origin/*`, bỏ sót workspace con)

| Trường | Nội dung chi tiết |
| :--- | :--- |
| **ID** | `LIC-SEC-005` |
| **Severity** | `P1` |
| **Source** | Kiểm toán chính sách `cargo-deny` và kịch bản CI trên 9 worktree & `origin/*` |
| **Evidence type** | So sánh phiên bản `deny.toml`, `Cargo.toml` (`[workspace]`) và `.github/workflows/{ci,cross-platform-validation}.yml` |
| **File/commit liên quan** | `desktop-instance-manager/deny.toml` (`8cf4703`, `0f87302`, `a5feb4a`, `6c4150e`, `bff0674`), `desktop-instance-manager/Cargo.toml` (`exclude = ["apps/desktop/src-tauri"]`), `desktop-instance-manager/fixtures/cross-platform/Cargo.toml`, `desktop-instance-manager/tests/cross-platform/Cargo.toml`, `.github/workflows/ci.yml` |
| **Tác động** | (1) `deny.toml` trên `origin/main` và `origin/feat/ui-vi-en-quiet-workspace` thiếu `"MPL-2.0"` và chứa khóa lỗi thời `version = 2` (cũng còn tồn tại trên `main` `a5feb4a` và 5 nhánh tính năng), làm `cargo-deny >= 0.20` báo lỗi cấu hình ngay lập tức.<br>(2) `fixtures/cross-platform/Cargo.toml` khai báo `[workspace]` riêng nhưng không được quét bởi `cargo-deny` trong cả `ci.yml`, `cross-platform-validation.yml` lẫn `dependency-audit.mjs`; `tests/cross-platform/Cargo.toml` cũng vắng mặt trong `ci.yml`.<br>(3) Các crate nội bộ thiếu `publish = false` trong khi `wildcards = "deny"` đang bật. |
| **Khuyến nghị xử lý** | 1. Đồng bộ bản `deny.toml` đã gỡ `version = 2` từ `zalo-integration` (`bff0674`) lên `main`, bổ sung tường minh `"0BSD"`, `"Unlicense"`, `"CC0-1.0"`, `"MIT-0"` nếu muốn tránh phụ thuộc ngầm vào nhánh `OR`, đồng thời giữ cấm tuyệt đối `GPL-*`/`AGPL-*`.<br>2. Thêm `publish = false` cho toàn bộ crate nội bộ (`crates/*/Cargo.toml`, `fixtures/*/Cargo.toml`, `tests/*/Cargo.toml`).<br>3. Bổ sung bước `cargo-deny check --config desktop-instance-manager/deny.toml --locked` cho cả `fixtures/cross-platform/Cargo.toml` và `tests/cross-platform/Cargo.toml` trong `ci.yml` và `dependency-audit.mjs`. |
| **Owner** | Central Integration + Build Agent |
| **Trạng thái** | `OPEN` (đã sửa một phần trên `zalo-integration`, chưa hợp nhất lên `main` / `origin/main`) |

---

### `LIC-SEC-006` — Kho mã nguồn PUBLIC thiếu `SECURITY.md` (công bố lỗ hổng có điều phối) và `CONTRIBUTING.md` (hàng rào sở hữu trí tuệ sạch cho PR bên ngoài)

| Trường | Nội dung chi tiết |
| :--- | :--- |
| **ID** | `LIC-SEC-006` |
| **Severity** | `P1` |
| **Source** | Kiểm toán quản trị bảo mật & đóng góp cộng đồng trên kho PUBLIC |
| **Evidence type** | Kiểm tra sự hiện diện tệp tại `/`, `.github/`, `desktop-instance-manager/` trên toàn bộ các nhánh |
| **File/commit liên quan** | Thư mục gốc `/` và `.github/` trên `origin/main` (`8cf4703`), `origin/feat/ui-vi-en-quiet-workspace` (`0f87302`), `main` (`a5feb4a`), `zalo-integration` (`bff0674`) |
| **Tác động** | Khi kho nguồn mở công khai, việc thiếu `SECURITY.md` khiến người phát hiện lỗ hổng (ví dụ lỗi cô lập profile, localhost token hoặc symlink) có xu hướng mở GitHub Issue công khai kèm PoC/dữ liệu nhạy cảm thay vì gửi qua GitHub Private Vulnerability Reporting. Đồng thời, thiếu `CONTRIBUTING.md` khiến dự án không có điều khoản cam kết nguồn gốc đóng góp (DCO) và không có quy tắc cấm cộng tác viên gửi PR chứa mã dịch ngược từ `app.asar`, tài nguyên VNG hoặc dữ liệu tài khoản Zalo thật. |
| **Khuyến nghị xử lý** | 1. Bổ sung `SECURITY.md` tại gốc repo hướng dẫn báo cáo lỗ hổng qua tính năng **GitHub Private Vulnerability Reporting** (không tạo Issue công khai cho lỗi bảo mật, không đưa email cá nhân chưa che chắn).<br>2. Bổ sung `CONTRIBUTING.md` quy định rõ: (a) đóng góp tuân theo cùng giấy phép của dự án (inbound = outbound), (b) nghiêm cấm gửi mã trích xuất/dịch ngược của VNG Zalo, nhị phân thực thi hoặc tệp phiên/cookie/log cá nhân trong PR và Issue. |
| **Owner** | Project Owner + Release Captain |
| **Trạng thái** | `OPEN` |

---

### `LIC-SEC-007` — Thiếu khai báo trường giấy phép/bản quyền trong cấu hình gói cài đặt (`tauri.conf.json`, `package.json`) và URL `repository` giả lập trên `origin/*`

| Trường | Nội dung chi tiết |
| :--- | :--- |
| **ID** | `LIC-SEC-007` |
| **Severity** | `P2` |
| **Source** | Kiểm toán manifest đóng gói đa nền tảng (`tauri.conf.json`, `tauri-ci-bundle-overlay.json`, `Cargo.toml`, `package.json`) |
| **Evidence type** | Kiểm tra cấu trúc JSON/TOML trên các nhánh |
| **File/commit liên quan** | `desktop-instance-manager/apps/desktop/src-tauri/tauri.conf.json`, `desktop-instance-manager/scripts/ci-cross-platform/tauri-ci-bundle-overlay.json`, `desktop-instance-manager/Cargo.toml` (`8cf4703`, `0f87302`), `desktop-instance-manager/fixtures/test-desktop-app/package.json` |
| **Tác động** | (1) `tauri.conf.json` và `tauri-ci-bundle-overlay.json` đều chưa khai báo `bundle.license`, `bundle.licenseFile` và `bundle.copyright`, dẫn đến các gói `.deb` (`/usr/share/doc/.../copyright`), trình cài đặt Windows `.msi`/NSIS và macOS `Info.plist` sinh ra không hiển thị điều khoản giấy phép và thông báo bản quyền.<br>(2) Trên `origin/main` và `origin/feat/ui-vi-en-quiet-workspace`, `Cargo.toml` vẫn trỏ `repository` tới URL không tồn tại `https://github.com/local/desktop-instance-manager`; `fixtures/test-desktop-app/package.json` chưa có `"license": "UNLICENSED"` (hoặc SPDX sau khi chốt giấy phép). |
| **Khuyến nghị xử lý** | Sau khi Chủ dự án chốt giấy phép ở `LIC-SEC-001`, cập nhật đồng bộ `bundle.license`, `bundle.licenseFile`, `bundle.copyright` trong `tauri.conf.json`, cập nhật `repository = "https://github.com/manhquydev/zalo"` (hoặc URL mới nếu đổi tên repo) trong `Cargo.toml`, và khai báo trường `"license"` trong `package.json`. |
| **Owner** | Build Agent + Central Integration |
| **Trạng thái** | `OPEN` |

---

### `LIC-SEC-008` — Bản clone kho ngoài `rea/` (chứa hỗn hợp giấy phép `MIT`, `GPL-3.0`, `LGPL-2.0`, `Apache-2.0`) nằm ngay trong thư mục gốc worktree `main`

| Trường | Nội dung chi tiết |
| :--- | :--- |
| **ID** | `LIC-SEC-008` |
| **Severity** | `P2` |
| **Source** | Kiểm toán cấu trúc thư mục gốc `C:\Users\<REDACTED_USER>\Downloads\zalo\rea` |
| **Evidence type** | Kiểm tra `.gitignore`, `rea/.git`, `rea/LICENSE`, `rea/licenses/*` và `rea/third_party/*` |
| **File/commit liên quan** | `.gitignore` (dòng 2: `rea/`), thư mục cục bộ `rea/` (remote `https://github.com/morluto/rea.git` tại commit `e9c72db`, chỉ tồn tại trên ổ đĩa worktree `zalo`, `0` tệp trên Git) |
| **Tác động** | Dù `.gitignore` đã chặn `rea/` nên chưa có byte nào bị đẩy lên GitHub, việc đặt toàn bộ cây mã của một dự án bên thứ ba (trong đó `rea/licenses/` và `rea/third_party/` chứa các văn bản/thành phần `GPL-3.0.txt`, `LGPL-2.0.txt`, Frida, Ghidra, `pwndbg`) bên trong thư mục gốc của kho nguồn tạo rủi ro thao tác nhầm (`git add -f`, đóng gói nén thư mục nguồn cục bộ, hoặc quét nhầm công cụ bảo mật). |
| **Khuyến nghị xử lý** | Giữ nguyên dòng chặn `rea/` trong `.gitignore` như chốt chặn phòng thủ thứ hai, đồng thời di chuyển thư mục `rea/` ra khỏi cây thư mục `Downloads\zalo` sang một đường dẫn nghiên cứu độc lập trên máy trạm. |
| **Owner** | Project Owner |
| **Trạng thái** | `OPEN` (đang được giảm thiểu bởi `.gitignore`) |

---

### `LIC-SEC-009` — Biểu tượng ứng dụng placeholder màu xanh đồng nhất (`#0057D8`) và huy hiệu chữ `"Z"` trên UI có nguy cơ tương đồng nhận diện thương hiệu (Trade Dress)

| Trường | Nội dung chi tiết |
| :--- | :--- |
| **ID** | `LIC-SEC-009` |
| **Severity** | `P2` |
| **Source** | Kiểm toán trực quan & giải mã nhị phân toàn bộ tệp biểu tượng (`.png`, `.ico`, `.icns`, `<svg>`) |
| **Evidence type** | Giải mã kênh màu PNG IDAT (`#0057D8` / `#0C2D78`), kiểm tra kích thước nhị phân (`115 B` – `1.142 B`) và mã HTML/SVG giao diện |
| **File/commit liên quan** | `desktop-instance-manager/apps/desktop/src-tauri/icons/{32x32.png,128x128.png,icon.png,icon.ico,icon.icns}` (và biến thể Retina `256x256` `.png` tại `scripts/ci-cross-platform/assets/`), `desktop-instance-manager/apps/desktop/ui/index.html` (dòng `154`, `446`) |
| **Tác động** | Các tệp icon hiện tại được xác minh **100% là hình vuông 2 màu tự sinh** (không sao chép tài nguyên đồ họa từ tệp PE của VNG nên không vi phạm bản quyền tác phẩm mỹ thuật ứng dụng). Tuy nhiên, việc sử dụng mảng màu xanh dương đậm `#0057D8` kết hợp huy hiệu chữ cái `"Z"` màu xanh và tên gọi `"Multi-Zalo"` có thể làm tăng rủi ro nhầm lẫn thị giác (trade dress) với biểu tượng Zalo chính thức trên thanh tác vụ/Dock của người dùng. |
| **Khuyến nghị xử lý** | Thiết kế bộ biểu tượng gốc mang bản sắc riêng của trình quản lý đa phiên (ví dụ biểu tượng các khung cửa sổ xếp lớp với bảng màu trung tính/đa sắc) và thay ký tự `"Z"` cứng trên huy hiệu mặc định bằng biểu tượng hồ sơ hoặc chữ cái đầu của tên không gian do người dùng đặt. |
| **Owner** | Account Dock/UI Owner + Build Agent |
| **Trạng thái** | `OPEN` |

---

## 6. Kết luận Đánh giá Cổng Pháp lý & Bản quyền cho `public-release-gate.md`

- **Phân loại trạng thái trụ cột Pháp lý, Quản trị OSS & Sở hữu Trí tuệ:** **`BLOCKED`** đối với quyết định đạt chuẩn `PUBLIC_READINESS_PASS` cho đến khi xử lý xong phát hiện **`P0` (`LIC-SEC-001` — thiếu hoàn toàn tệp `LICENSE` và chưa tách phạm vi cấp phép giữa mã gốc với các thành phần `LICENSE_REVIEW_REQUIRED`)**, và **`CHANGES_REQUESTED`** đối với các hạng mục **`P1` (`LIC-SEC-002`..`LIC-SEC-006`)** trước khi công bố bản phát hành nhị phân/cài đặt rộng rãi.
- **Điểm sáng cốt lõi đã được kiểm chứng thực nghiệm:**
  1. **Không vi phạm mã nguồn/nhị phân độc quyền VNG (`PASS`):** `0` tệp thực thi VNG, `0` tệp `app.asar`, `0` hành vi dịch ngược sửa mã hay vượt xác thực; toàn bộ cơ chế đa phiên dựa trên cô lập môi trường hệ điều hành hợp pháp và kiểm tra chữ ký số `WinVerifyTrust` (`CN/O=VNG GROUP JSC`).
  2. **Không nhiễm Copyleft mạnh (`PASS`):** `0` crate `GPL` hoặc `AGPL` trên toàn bộ `439` gói Rust đã khóa; toàn bộ biểu tượng hiện tại là hình học tự sinh (không trích xuất từ tài nguyên PE của VNG).
- **Thứ tự hành động ưu tiên cho Chủ dự án & Nhóm Tích hợp (không viết lại lịch sử Git):**
  1. Tách/loại trừ các tệp `LICENSE_REVIEW_REQUIRED` (`multi-zalo-desktop/dist/MultiZaloDesktop.exe`, `**/evidence/*.json`) và chốt văn bản giấy phép chính thức (khuyến nghị `LICENSE-MIT` + `LICENSE-APACHE` cho mã nguồn tự viết của `desktop-instance-manager/`).
  2. Bổ sung tuyên bố phi liên kết nhãn hiệu với VNG/Zalo, chuẩn hóa lại tên hiển thị & `identifier` theo nguyên tắc *Nominative Fair Use*, và thêm `SECURITY.md` + `CONTRIBUTING.md` + `THIRD_PARTY_NOTICES.md` (đáp ứng `MPL-2.0` Mục 3.2(a) và quy định `LGPL` trên Linux).
  3. Hợp nhất bản sửa `deny.toml` (gỡ `version = 2`, giữ `MPL-2.0`) và mở rộng `cargo-deny` phủ đủ 4 workspace Rust trong CI.
