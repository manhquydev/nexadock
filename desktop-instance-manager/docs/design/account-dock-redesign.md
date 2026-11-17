# Multi-Zalo Desktop — Account Dock Consumer UX Redesign

## 1. Baseline & Scope
- **Baseline Commit**: `b1446d670155cd663f4f927bba2c8df8c0735044` (`feat/ui-vi-en-quiet-workspace`)
- **Worktree**: `C:/Users/manhquy/Downloads/zalo-ui-worktree/desktop-instance-manager/`
- **Scope**: Strictly `apps/desktop/ui/**` and `docs/design/**`. Zero modifications to Rust core, Tauri commands, PowerShell bridge, `ui-server.mjs`, or CI.

## 2. Why Quiet Workspace Failed Consumer UX & Why Account Dock Solves It
| Quiet Workspace (`b1446d6`) Issue | Account Dock Redesign |
| :--- | :--- |
| Left 216px admin sidebar (`Hồ sơ Zalo`, `Sao lưu`, `Chẩn đoán`, `Cài đặt`) + bottom loopback status | Compact top bar (`Multi-Zalo`, `Tài khoản`, `Sao lưu`, `Cài đặt`, `Trợ giúp`, `Làm mới`, `+ Thêm tài khoản`) |
| Technical jargon (`PID`, `sandbox`, `127.0.0.1`, `Đo dung lượng đĩa`, `Thông số an toàn`) on Home | Zero technical jargon on Home; technical checks moved under `Cài đặt → Nâng cao → Kiểm tra sự cố` |
| Plain text profile rows with weak visual identity | 2-column Account Grid (1-column on `<720px`) with letter-initial avatars + 8-color identity palette |
| Single-step create dialog ending in generic banner | 2-step onboarding: Step 1 (name + quick chips + color) → Step 2 (`"Mở Zalo để đăng nhập"`) |
| Technical primary Zalo note (`1 PID được bảo vệ`) | Calm read-only card: `"Zalo đang dùng trên máy — Đang mở · Được bảo vệ, không thể đóng hoặc xóa từ đây"` |

## 3. Information Architecture (Before → After)
- **Before**: `[Sidebar: Profiles | Backups | Diagnostics | Settings]` → Flat list of profiles + 4-section admin layout.
- **After**:
  - **Home (`Tài khoản Zalo`)**: Top bar + Account Grid + Read-only Primary Zalo card (`Zalo đang dùng trên máy`).
  - **Secondary Views**: `Sao lưu` (`view-backups`), `Cài đặt` (`view-settings`), `Trợ giúp` (`#safety-dialog`).
  - **Advanced (`Cài đặt → Nâng cao → Kiểm tra sự cố`)**: `view-diagnostics` (retains all isolation checks, OS readiness matrix, disk size measurement toggle, and filtered logs).

## 4. Design Tokens & Component Inventory
- **Tokens**: Spacing `4/8/12/16/24/32px`, `color-scheme: light dark`, `light-dark()` + `@media (prefers-color-scheme: dark)` fallback, curated 8-color identity palette (`--id-blue`, `--id-teal`, `--id-green`, `--id-amber`, `--id-coral`, `--id-plum`, `--id-indigo`, `--id-slate`).
- **Components**: `.topbar`, `.account-grid`, `.account-card`, `.account-avatar`, `.primary-zalo-card`, `.overflow-menu` (WAI-ARIA `role="menu"`), `.empty-hero`, `.segmented-control` (native `<input type="radio">`), `.color-palette` (native `<input type="radio">`), native `<dialog>` modals (`showModal()`).

## 5. UX Writing Changes (`vi-VN` & `en-US`)
- `Hồ sơ Zalo` → `Tài khoản Zalo` / `Tài khoản`
- `Tạo hồ sơ` → `Thêm tài khoản`
- `Mở cửa sổ` → `Chuyển đến`
- `Dừng` → `Đóng Zalo`
- `Chẩn đoán hệ thống` → `Kiểm tra sự cố`
- `Sao lưu ngoại tuyến` → `Sao lưu`
- `Chi tiết kỹ thuật hồ sơ` → `Thông tin tài khoản`
- `Không tìm thấy executable` → `Không tìm thấy Zalo trên máy`
- `Host communication failed` → `Không thể kết nối với ứng dụng`

## 6. User Journeys & Visual Review Evidence
Screenshots generated in `docs/design/screenshots/` and visually inspected:
- **Journey A (New user, 0 accounts)**: `account-dock-empty-vi-light.png` → `account-dock-add-step1-vi-light.png` → `account-dock-add-step2-vi-light.png` (2-step onboarding + `"Mở Zalo để đăng nhập"`).
- **Journey B (3 accounts, 2 running)**: `account-dock-3accounts-vi-light.png`, `account-dock-vi-VN-dark-1160x780.png`, `account-dock-vi-VN-light-420x540.png`.
- **Journey C (Error state)**: `account-dock-error-vi-light.png` (clean Vietnamese banner + collapsed `▶ Chi tiết kỹ thuật`).
- **Journey D (Account management)**: `account-dock-menu-vi-light.png`, `account-dock-details-vi-light.png`, `account-dock-backups-vi-light.png`.
- **Journey E (Settings, Diagnostics, English)**: `account-dock-settings-vi-light.png`, `account-dock-diagnostics-vi-light.png`, `account-dock-en-US-light-1160x780.png`, `account-dock-zoom-200-vi-light.png`.
- **Visual defect caught & fixed during inspection**: `#connection-pill` had `display: inline-flex` overriding HTML `[hidden]`, showing an empty pill in the topbar; fixed with `[hidden] { display: none !important; }` in `styles.css` and re-verified.

## 7. Test Results
- `node --test apps/desktop/ui/tests/i18n.test.mjs apps/desktop/ui/tests/ui-contracts.test.mjs`: **17/17 passed, 0 failed** (including `initI18n` 404/invalid JSON/timeout tests, consumer-copy jargon check, `getAccountInitials`, and contract checks).
- `node apps/desktop/ui/tests/visual-cdp.mjs`: **PASS** (16 viewport/locale/theme combinations, Journeys A–E, WAI-ARIA menu keyboard nav, native radio arrow keys, per-account async pending guards, double-click dedup, out-of-order `/api/status`, 200% zoom, 0 overflow, 0 runtime errors).

## 8. Backend Integration Requests & Untested Items
- **Optional Backend Enhancement (Non-blocking)**: Account identity colors are currently stored locally in `localStorage` (`multi-zalo.accountColors`) with deterministic hash fallback. If cross-origin/unified persistence is desired, backend may optionally add an optional `color?: string` field to profile metadata in `create_profile` / `ProfileInfo` (safe fallback already works without it).
- **`NOT_TESTED` Platforms**: Live Windows Tauri WebView2 + Rust core build, macOS WKWebView, and Ubuntu WebKitGTK remain `NOT_TESTED` in this UI-only worktree.
- **Usability Hypothesis**: Real consumer usability testing with non-technical users has not yet been conducted; current evidence comes from automated E2E journeys and visual inspection.
