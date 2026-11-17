# Zalo Desktop Multi-Instance Research & Manager Workspace

Workspace nghiên cứu dịch ngược ứng dụng **Zalo Desktop** chính thức bằng **REA (Reverse Engineer Anything)** và phát triển trình quản lý đa phiên cô lập dữ liệu (**Desktop Instance Manager**).

## Primary Workspace — `desktop-instance-manager/`

Kiến trúc Rust workspace + Tauri desktop bridge + Windows Signed-Host Bridge (`PowerShell` + loopback Web UI chuẩn `modern-web-guidance`), hỗ trợ chạy đồng thời nhiều phiên Zalo Desktop cô lập hoàn toàn khỏi `%APPDATA%\ZaloData`:

- **Bắt đầu nhanh & Hướng dẫn sử dụng:** [`desktop-instance-manager/README.md`](desktop-instance-manager/README.md)
- **Kiến trúc & Quyết định thiết kế (ADR 001–004):**
  - [`desktop-instance-manager/docs/architecture/adr-001-hybrid-isolation-architecture.md`](desktop-instance-manager/docs/architecture/adr-001-hybrid-isolation-architecture.md)
  - [`desktop-instance-manager/docs/architecture/adr-002-rust-tauri-and-sac-host-bridge.md`](desktop-instance-manager/docs/architecture/adr-002-rust-tauri-and-sac-host-bridge.md)
- **Đánh giá Bảo mật (STRIDE + OWASP + 11 Threat Categories):** [`desktop-instance-manager/docs/security/threat-model-and-audit.md`](desktop-instance-manager/docs/security/threat-model-and-audit.md)
- **Báo cáo Kiểm thử & Live Zalo 26.10.20 PoC (Gate A–F):** [`desktop-instance-manager/docs/testing/poc-and-test-report.md`](desktop-instance-manager/docs/testing/poc-and-test-report.md)
- **Ma trận Hỗ trợ Nền tảng (Windows / macOS / Ubuntu):** [`desktop-instance-manager/docs/compatibility/os-readiness.md`](desktop-instance-manager/docs/compatibility/os-readiness.md)
- **Bằng chứng JSON đã xác minh:** [`desktop-instance-manager/evidence/fixture-test-result.json`](desktop-instance-manager/evidence/fixture-test-result.json), [`desktop-instance-manager/evidence/zalo-live-poc-result.json`](desktop-instance-manager/evidence/zalo-live-poc-result.json)

---

## Historical Research & v1 Windows PoC — `multi-zalo-desktop/`

Phiên bản tiền nhiệm (C#/.NET PoC trên Windows + bộ bằng chứng dịch ngược REA đối với `Zalo-26.10.20` và `ZaloSetup-26.11.12.exe`):

- [`multi-zalo-desktop/README.md`](multi-zalo-desktop/README.md)
- [`multi-zalo-desktop/docs/architecture.md`](multi-zalo-desktop/docs/architecture.md)
- [`multi-zalo-desktop/docs/security-review.md`](multi-zalo-desktop/docs/security-review.md)
- [`multi-zalo-desktop/docs/testing-report.md`](multi-zalo-desktop/docs/testing-report.md)
- [`multi-zalo-desktop/evidence/rea-evidence-summary.json`](multi-zalo-desktop/evidence/rea-evidence-summary.json)
