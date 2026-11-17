# Nghiên Cứu REA (Reverse Engineer Anything) & Khả Năng Hoạt Động Trên Windows

**Phiên bản REA đã kiểm chứng:** `rea-agents` v6.9.0  
**Đường dẫn cài đặt:** `C:\Users\manhquy\AppData\Roaming\npm\node_modules\rea-agents\scripts\rea.mjs`  
**Repository cục bộ:** `c:\Users\manhquy\Downloads\zalo\rea` (`https://github.com/morluto/rea`)  
**Giao thức MCP:** `2026-07-28` (`255` công cụ MCP, `196` lệnh CLI)  
**Bằng chứng phiên REA:** [`evidence/rea-binary-session.json`](../evidence/rea-binary-session.json)

---

## 1. Tổng Quan Cách Tích Hợp MCP Và Công Cụ REA

REA vận hành như một máy chủ MCP cục bộ kết nối với AI agent thông qua `stdio` (`rea-agents` v6.9.0 chạy trên Node.js `v26.13.1`). Thông qua `binary_session`, REA công bố trạng thái nạp target, danh sách provider khả dụng/không khả dụng, cơ chế lưu trữ Evidence có định danh (`ev_<sha256>`), và các ràng buộc phạm vi (limitations / unknowns) đi kèm mọi kết quả phân tích.

Mỗi lần gọi công cụ phân tích của REA đều tạo ra một bản ghi Evidence chứa:
- `evidence_id`: định danh duy nhất của bằng chứng (`ev_...`).
- `subject`: tên file, đường dẫn tuyệt đối, SHA-256, định dạng và kiến trúc.
- `confidence` & `authority`: phân biệt giữa `observed` (quan sát trực tiếp trên artifact), `derived` (suy dẫn có cấu trúc) và `inferred`.
- `limitations`: tuyên bố rõ những gì công cụ **không** kiểm chứng (ví dụ: đọc bảng PE certificate nhưng không xác minh chuỗi tin cậy mật mã; đọc PE import directory nhưng không kết luận khả năng gọi lúc runtime).

---

## 2. Chức Năng REA Hỗ Trợ Phân Tích Windows Executable & Electron

Qua kiểm chứng thực tế trên máy Windows 11 x64 với Zalo Desktop, các nhóm công cụ sau hoạt động ổn định, không cần engine bên thứ ba:

| Công cụ REA | Provider | Chức năng trên Windows PE / Electron | Bằng chứng đã chạy |
| :--- | :--- | :--- | :--- |
| `binary_session` | Built-in | Kiểm tra trạng thái MCP, inventory 255 tool và tình trạng các provider | [`rea-binary-session.json`](../evidence/rea-binary-session.json) |
| `inspect_pe_certificates` | `rea-pe-certificates` | Giải mã bảng `WIN_CERTIFICATE` (PKCS#7 `signedData`, issuer, subject, serial, thời hạn) | `ev_e417c46d...`, `ev_ab238d25...`, `ev_817da2b6...` |
| `inspect_pe_imports` | `rea-pe-imports` | Trích xuất Import Directory, Delay-load Import, IAT/ILT RVA và `imphash` | `ev_095e339d...`, `ev_83087efa...`, `ev_522de365...` |
| `inspect_binary_hardening` | `rea-binary-hardening` | Kiểm tra cờ `DllCharacteristics` (`DYNAMIC_BASE`, `NX_COMPAT`, `GUARD_CF`, `NO_SEH`, `FORCE_INTEGRITY`) và sự hiện diện của bảng Security/Debug | `ev_94316f7b...`, `ev_9f9733b6...`, `ev_fce14a1d...` |
| `inspect_pe_load_config` | `rea-pe-load-config` | Đọc `IMAGE_DIRECTORY_ENTRY_LOAD_CONFIG`, Security Cookie, SafeSEH table, GuardFlags | `ev_d6d7bf98...` |
| `inspect_pe_resources` | `rea-pe-resources` | Liệt kê cây tài nguyên PE (`RT_ICON`, `RT_GROUP_ICON`, `RT_VERSION`, `RT_MANIFEST`, `RT_DIALOG`, ...) | `ev_bf01d539...`, `ev_c13e46e2...` |
| `inspect_electron_fuses` | `electron-fuses` | Quét sentinel fuse v1 trong binary Electron (`RunAsNode`, `EnableCookieEncryption`, `EnableNodeOptionsEnvironmentVariable`, `EnableNodeCliInspectArguments`, `EnableEmbeddedAsarIntegrityValidation`, `OnlyLoadAppFromAsar`) | `ev_8e05df8e...` |
| `inspect_desktop_application` | `rea-desktop-application` | Nhận diện cấu trúc thư mục ứng dụng desktop (phát hiện Electron qua `resources/app.asar`) | `ev_1f988406...`, `ev_46339d33...` |
| `open_binary` + `inspect_artifact` | `rea-artifact-graph` | Mở và lập chỉ mục cấu trúc archive `app.asar` (10.869 node, 13.170 file) | `ev_70919879...` / `ev_2688bbbc...` |
| `audit_release_exposure` | `rea-release-exposure` | Đánh giá phơi bày tĩnh trên `app.asar` và `.node` addon (phát hiện đường dẫn PDB build và payload trích xuất được) | `ev_98cf05ba...` |

---

## 3. Các Giới Hạn Thực Tế Của REA Trên Windows Đã Xác Minh

Không giả định REA hỗ trợ toàn bộ thao tác trên Windows; quá trình chạy thực tế đã ghi nhận các giới hạn cụ thể sau:

1. **Chặn Native Addon `rea-windows-x64.node` bởi Windows Application Control / Smart App Control (`BLOCKED`)**:
   - Khi gọi `analyze_javascript_application` trên `app.asar`, REA trả về lỗi `capability_unavailable` (Evidence `ev_65a72c27603cd5f817368a4390e8a3b9d8d9ed1bf6592ecf0f57c4adfdc7ee4b`):
     > `Windows refused to load the verified native/windows/build/rea-windows-x64.node: An Application Control policy has blocked this file.`
   - **Hệ quả**: Không thể dùng `analyze_javascript_application` qua MCP trên host Windows đang bật chính sách kiểm soát ứng dụng này. Do đó, việc kiểm tra chi tiết bên trong `app.asar` (`bootstrap.js`, `main-dist/migration.js`, `main-dist/main.js`, `main-dist/second-instance.js`) được thực hiện bằng công cụ đọc cấu trúc ASAR cục bộ an toàn (chỉ đọc header JSON và byte offset chuẩn của Electron ASAR, không thực thi mã target).

2. **Chưa cấu hình Disassembler / Decompiler gốc (`not_configured` / `unsupported_host`)**:
   - Theo [`rea-binary-session.json`](../evidence/rea-binary-session.json):
     - `ghidra`: unavailable (`not_configured` — chưa thiết lập `GHIDRA_INSTALL_DIR` trỏ đến Ghidra 12.x >= 12.1).
     - `ida`: unavailable (`not_configured` — chưa cấu hình `REA_IDA_MCP_CONFIG`).
     - `kuna`: unavailable (`not_configured` — chưa cấu hình `REA_KUNA_COMMAND`).
     - `hopper`: unavailable (`unsupported_host` — không hỗ trợ trên `win32`).
   - **Hệ quả**: Các tool `analyze_function`, `procedure_pseudo_code`, `xrefs` của REA chưa khả dụng. Phần giải mã nhánh điều khiển trong stub `Zalo.exe` (PE32 x86, 790.568 bytes) được bổ sung bằng phân tích bảng PE section, tìm XREF địa chỉ chuỗi (`VA 0x435C90..0x436088`) và đối chiếu trực tiếp opcode x86 tại `0x4280..0x45B0`.

3. **Các công cụ chỉ hỗ trợ Linux / macOS**:
   - Theo `tool_availability` trong `binary_session`:
     - Nhóm macOS (`inspect_macho`, `inspect_signature`, `inspect_plist`, `inspect_asset_catalog`, `observe_native_ui`): `unsupported_host` (yêu cầu macOS).
     - Nhóm Linux (`inspect_binary_layout`, `inspect_recorded_crash`, `start_gdb_session`, `start_rizin_debug_session`, `capture_process_scenario`, `recover_javascript_sources` / Wakaru): `unsupported_host` (yêu cầu Linux).

4. **Ngưỡng đọc tệp mặc định (`max_file_bytes = 67108864` / 64 MiB)**:
   - Lần gọi đầu `inspect_pe_imports` trên `ZaloSetup-26.11.12.exe` (213,5 MB) và `inspect_binary_hardening` trên `Zalo-26.10.20\Zalo.exe` (136,6 MB) trả về `resource_constraint`. Nâng tham số `max_file_bytes` lên `268435456` (256 MiB) giải quyết triệt để vấn đề này.

---

## 4. Thiết Lập Môi Trường Phân Tích An Toàn

- **Không sửa đổi cài đặt gốc**: Toàn bộ thao tác REA chạy ở chế độ đọc (`read-only`) đối với `ZaloSetup-26.11.12.exe` và thư mục cài đặt `%LOCALAPPDATA%\Programs\Zalo`.
- **Cô lập dữ liệu cá nhân**: Không đọc nội dung cơ sở dữ liệu tin nhắn (`ftsm_*.db`), khóa phiên hay cookie đăng nhập trong `%APPDATA%\ZaloData`; chỉ kiểm tra sự tồn tại và tên key cấu hình không nhạy cảm (`config.json`, `migrate-config.json`, `zmeta.json`, `log.log`, `startup.log`).
- **Không dùng công cụ bên thứ ba chưa kiểm tra**: Mọi script phân tích bổ sung đều là PowerShell cục bộ thuần túy viết trực tiếp để đọc cấu trúc ASAR và PE header.
