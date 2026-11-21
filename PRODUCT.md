# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Stack

Astro (static, existing codebase) — deployed to GitHub Pages.

## Users

- **Primary:** kỹ sư / power-user Việt Nam vận hành nhiều tài khoản Zalo desktop song song (CSKH, bán hàng online, vận hành cộng đồng, test) — chủ yếu Windows, mở rộng macOS và Linux.
- **Secondary:** người dùng quen thuộc cơ bản nhưng có nhu cầu nhiều tài khoản Zalo trên cùng một máy và muốn cách làm "sạch" thay vì hack thủ công.
- Người dùng Linux: chạy community repack của client Zalo (VNG không có bản Linux chính thức) — NexaDock nhận diện và quản lý ở trạng thái Experimental.

## Product Purpose

NexaDock (trước là KHOANG / Multi-Zalo Desktop) quản lý nhiều phiên bản Zalo desktop độc lập trên cùng một máy: mỗi phiên bản có dữ liệu, cửa sổ, trạng thái riêng; có backup/restore, diagnostics, bảo vệ tài khoản chính. Zalo PC chính thức chỉ cho phép một tài khoản trên một máy — NexaDock lấp khoảng trống đó bằng client chính thức + isolation, không sửa mã nhị phân Zalo.

## Positioning

Không claim "đầu tiên" (thị trường có ZaloMulti, Zoo Zalo Dock, HAP Multiaccount, ZaloPC Multi). Khác biệt trung thực của NexaDock so với các đối thủ:

1. **Isolation kiểm chứng được, không chỉ redirect thư mục:** WebIsolated zero-IPC (raw wry child WebView), PID-verified stop với bảo vệ primary, quarantine/backup, diagnostics fail-closed — kèm bằng chứng CI và kiểm thử đa nền tảng.
2. **Open-source, không khóa thương mại:** MIT/Apache-2.0, không giới hạn 4 tài khoản, không gia hạn bản quyền 30 ngày, không kèm tự động hóa/bot.
3. **Trung thực có chủ đích:** báo đúng trạng thái hỗ trợ từng nền tảng (Official unsupported trên Linux, Experimental community route, Wayland fail-closed) thay vì oversell.
4. **Cross-platform thật:** Windows + macOS + Linux (Experimental), song ngữ vi-VN/en-US đầy đủ.

## Operating Context

- Chạy trên desktop của người dùng; Zalo client là ứng dụng độc quyền của VNG, người dùng tự cài bản chính thức (Windows/macOS) hoặc community repack (Linux, Experimental).
- Deploy website: GitHub Pages; release artifacts build bằng CI (macOS ad-hoc signed cho đến khi có Apple Developer secrets).
- Repo: manhquydev/nexadock; tài liệu kỹ thuật tại `docs/` và `desktop-instance-manager/docs/`.

## Capabilities and Constraints

- Quản lý profile Zalo desktop: tạo/đổi/xóa, backup-restore, trạng thái runtime, tray, autostart.
- WebIsolated: phiên web Zalo cô lập hoàn toàn (zero-IPC) — opt-in; fail-closed trên Wayland thiếu X11.
- Linux: Experimental route cho community repack `zalo-linux` (audit integrity, ZALO_DATA_DIR 0700 trong profile); gọi thoại/video và sync end-to-end chưa xác minh hoàn toàn.
- Không bao giờ đóng gói/phân phối binary Zalo (clean-room); không mod client; không lạm dụng giao thức Zalo.
- Thuật ngữ bắt buộc: "phiên bản" (instance), "ZaloData" (dữ liệu chính thức), "WebIsolated", "Experimental".

## Brand Commitments

- Tên: **NexaDock**. UI triết lý **Calm Precision**: typography rem/em, tối giản, không gradient, không `innerHTML`, dark-first.
- Song ngữ **vi-VN / en-US** đầy đủ và cân bằng.
- Trung thực về trạng thái hỗ trợ và giới hạn — không oversell.
- Không phân phối tài sản độc quyền của VNG/Zalo.

## Evidence on Hand

- Kho fuente chính thức (repo này): ADR-001, docs kiến trúc, CI logs xanh 3 OS, release artifacts (Windows exe/msi, macOS DMG ad-hoc, Linux deb/AppImage).
- Đã xác minh thực tế trên thiết bị chuẩn: Zalo Linux repack login/sync/giữ phiên; NexaDock launch route Experimental end-to-end.
- Không có: testimonial, số liệu người dùng, press — không được bịa trong nội dung website.

## Product Principles

1. **Calm Precision** — mọi quyết định UI ưu tiên rõ ràng, ít chuyển động thừa, đo bằng rem/em.
2. **Honest status over marketing** — hiển thị đúng thứ được hỗ trợ/thử nghiệm, kể cả khi kém hấp dẫn hơn.
3. **Isolation you can verify** — tính năng isolation phải có test/bằng chứng, không phải lời quảng cáo.
4. **User owns their Zalo** — không chặn, không khóa tính năng sau bản dùng thử; client của người dùng là client chính thức.

## Accessibility & Inclusion

- Song ngữ vi/en đầy đủ (không fallback lẫn lộn).
- Contrast chuẩn WCAG AA trên dark theme; điều hướng bàn phím; không nội dung chèn bằng innerHTML.
