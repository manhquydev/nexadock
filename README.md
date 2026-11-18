# Desktop Instance Manager (Multi-Zalo Desktop)

> **Tuyên bố phi liên kết / Non-Affiliation Notice:** Đây là dự án mã nguồn mở độc lập, **không liên kết, không được tài trợ hay chứng thực bởi VNG Corporation hay Zalo**. `"Zalo"` là nhãn hiệu thuộc sở hữu của VNG Corporation. Kho mã nguồn và các bộ cài đặt **không** chứa, không chỉnh sửa và không phân phối lại bất kỳ nhị phân hay tài nguyên độc quyền nào của VNG — người dùng tự cài đặt Zalo Desktop chính thức từ VNG.  
> *(An independent open-source project, not affiliated with, sponsored by, or endorsed by VNG Corporation or Zalo. Zero proprietary VNG binaries are bundled or redistributed.)*

**Giấy phép / License:** Mã nguồn phần mềm trong `desktop-instance-manager/` được cấp phép kép theo **MIT OR Apache-2.0** ([`LICENSE-MIT`](LICENSE-MIT), [`LICENSE-APACHE`](LICENSE-APACHE)), kèm thông báo phụ thuộc tại [`desktop-instance-manager/THIRD_PARTY_NOTICES.md`](desktop-instance-manager/THIRD_PARTY_NOTICES.md). Các tệp nhật ký/bằng chứng kiểm thử dưới `**/evidence/**` là tài liệu ghi nhận kiểm thử và nằm ngoài phạm vi cấp phép phần mềm.

Workspace phát triển trình quản lý đa phiên cô lập dữ liệu (**Desktop Instance Manager**) cho **Zalo Desktop**.

## Primary Workspace — `desktop-instance-manager/`

Kiến trúc Rust workspace + Tauri 2 desktop shell (`dim-desktop.exe`) + native CLI (`dim-cli.exe`) + Windows Signed-Host Bridge (`PowerShell` + loopback Web UI chuẩn `modern-web-guidance`), hỗ trợ chạy đồng thời nhiều phiên Zalo Desktop cô lập hoàn toàn khỏi `%APPDATA%\ZaloData`:

- **Bắt đầu nhanh & README Workspace:** [`desktop-instance-manager/README.md`](desktop-instance-manager/README.md)
- **Hướng dẫn sử dụng Windows MVP:** [`desktop-instance-manager/docs/user-guide/windows-mvp-guide.md`](desktop-instance-manager/docs/user-guide/windows-mvp-guide.md)
- **Báo cáo Hardening & Xác minh sau kiểm toán:** [`desktop-instance-manager/docs/reviews/post-swarm-hardening-report.md`](desktop-instance-manager/docs/reviews/post-swarm-hardening-report.md)
- **CI Workflow (Repo Root):** [`.github/workflows/ci.yml`](.github/workflows/ci.yml)
- **Kiến trúc & Quyết định thiết kế (ADR 001–004):**
  - [`desktop-instance-manager/docs/architecture/adr-001-hybrid-isolation-architecture.md`](desktop-instance-manager/docs/architecture/adr-001-hybrid-isolation-architecture.md)
  - [`desktop-instance-manager/docs/architecture/adr-002-rust-tauri-and-sac-host-bridge.md`](desktop-instance-manager/docs/architecture/adr-002-rust-tauri-and-sac-host-bridge.md)
- **Đánh giá Bảo mật (STRIDE + OWASP + 11 Threat Categories):** [`desktop-instance-manager/docs/security/threat-model-and-audit.md`](desktop-instance-manager/docs/security/threat-model-and-audit.md)
- **Báo cáo Kiểm thử & Live Zalo 26.10.20 PoC (Gate A–F):** [`desktop-instance-manager/docs/testing/poc-and-test-report.md`](desktop-instance-manager/docs/testing/poc-and-test-report.md)
- **Ma trận Hỗ trợ Nền tảng (Windows / macOS / Ubuntu):** [`desktop-instance-manager/docs/compatibility/os-readiness.md`](desktop-instance-manager/docs/compatibility/os-readiness.md)
- **Bằng chứng JSON & Provenance đã xác minh:**
  - [`desktop-instance-manager/evidence/fixture-test-result.json`](desktop-instance-manager/evidence/fixture-test-result.json)
  - [`desktop-instance-manager/evidence/contract-parity-result.json`](desktop-instance-manager/evidence/contract-parity-result.json)
  - [`desktop-instance-manager/evidence/zalo-live-poc-result.json`](desktop-instance-manager/evidence/zalo-live-poc-result.json)
  - [`desktop-instance-manager/evidence/artifact-manifest.json`](desktop-instance-manager/evidence/artifact-manifest.json) | [`sbom-cyclonedx.json`](desktop-instance-manager/evidence/sbom-cyclonedx.json) | [`dependency-audit.json`](desktop-instance-manager/evidence/dependency-audit.json)

---

## Historical Research & v1 Windows PoC — `multi-zalo-desktop/`

Phiên bản tiền nhiệm (C#/.NET PoC trên Windows + bộ bằng chứng dịch ngược REA đối với `Zalo-26.10.20` và `ZaloSetup-26.11.12.exe`):

- [`multi-zalo-desktop/README.md`](multi-zalo-desktop/README.md)
- [`multi-zalo-desktop/docs/architecture.md`](multi-zalo-desktop/docs/architecture.md)
- [`multi-zalo-desktop/docs/security-review.md`](multi-zalo-desktop/docs/security-review.md)
- [`multi-zalo-desktop/docs/testing-report.md`](multi-zalo-desktop/docs/testing-report.md)
