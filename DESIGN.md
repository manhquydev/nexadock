# DESIGN.md — NexaDock website visual world

<!-- impeccable:design-schema 1 -->
World: **Rack Console** (homelab rack manager). Mode: **Persuade** (landing) / **Read** (docs, install guide).

## Mechanism this world carries

NexaDock = quản lý nhiều phiên bản Zalo như quản lý thiết bị thật: mỗi phiên bản là một unit trên rack với đèn trạng thái riêng, dữ liệu và vòng đời độc lập. Website phải cho cảm giác "đang điều khiển thiết bị đã được kiểm chứng", không phải landing SaaS chung chung.

## Palette

- **Ground:** near-black `#0b0e11` (page), panel `#14181d`, raised `#1a1f26` — không gradient.
- **Ink:** primary text `#e6e9ee`; secondary `#9aa3ad`; hairline border `#232a32`.
- **LED accents (duy nhất được phép dùng màu):**
  - running `#3fb950` (green)
  - attention/experimental `#d29922` (amber)
  - idle/stopped `#6e7681` (gray)
  - danger/blocked `#f85149` (red)
  - brand action `#1f6feb` (blue, white-text contrast 4.63:1; CTA only)
- LED chiếm tối đa ~5% diện tích; mọi màu khác trung tính. Calm Precision: không gradient, không glow quá mức (box-shadow 1 lớp, alpha thấp).

## Type

- Body: system sans stack (`-apple-system, "Segoe UI", Roboto, sans-serif`), 1rem/1.6, measure 65–75ch.
- Mono (ui-monospace stack) **chỉ dùng cho dữ liệu đo đạc thực**: số unit trên rail, PID/size/trạng thái kỹ thuật, bảng phiên bản — không dùng làm heading (tránh mono-as-costume).
- Headings: sans, phân cấp bằng weight + tracking (-0.02em) + vị trí; tối đa 3 cỡ hiển thị mỗi viewport; không có label/eyebrow phía trên heading.
- Rank = vị trí trên rail + số unit mono, KHÔNG phải cỡ chữ bất thường.

## Topology / navigation

- **Rack rail**: thanh điều hướng ngang cố định như ray rack; mỗi mục = 1 unit với số mono (`01`, `02`…) — số chỉ tồn tại trên rail (thông tin vị trí rack thật), KHÔNG lặp lại làm eyebrow trên heading section.
- **Nút điều hướng KHÔNG hiển thị phiên bản** — chỉ nhãn "Releases" trỏ tới GitHub releases. Phiên bản hiện tại chỉ xuất hiện ở 1 chỗ: footer + khối download.
- Sections xếp dọc; mỗi section mở bằng heading sans đậm, không label phía trên.

## Controls / state

- Trạng thái là **đèn LED (chấm 8px) + nhãn chữ**, không phải hue nền: running (xanh), experimental (hổ phách), stopped (xám), blocked (đỏ).
- Nút: hình chữ nhật bo 4px, viền hairline, nền raised; CTA chính nền `#1f6feb` chữ trắng.

## Shared token contract (BaseLayout :root — agents BẮT BUỘC dùng các tên này)

`--nd-bg #0b0e11` · `--nd-panel #14181d` · `--nd-raised #1a1f26` · `--nd-ink #e6e9ee` · `--nd-ink-2 #9aa3ad` · `--nd-line #232a32` · `--nd-led-running #3fb950` · `--nd-led-attention #d29922` · `--nd-led-idle #6e7681` · `--nd-led-danger #f85149` · `--nd-cta #1f6feb` · `--nd-mono` (ui-monospace stack) · `--nd-sans` (system sans)

## Responsive / motion

- Mobile: rail thu thành hàng ngang cuộn được; rack unit xếp chồng full-width; số mono giữ nguyên.
- Motion: tối giản — LED pulse 2s cho trạng thái running; không scroll-jack; `prefers-reduced-motion` tắt pulse.
- Breakpoints: 360 / 768 / 1080.

## Quality floor (ràng buộc từ Calm Precision)

- Không `innerHTML`; không gradient; không `backdrop-filter`; contrast AA; song ngữ vi-VN/en-US đầy đủ và cân bằng.
- Không hiển thị phiên bản ở nút điều hướng; phiên bản chỉ ở footer và khối download.
- Không bao giờ nhúng hoặc quảng bá binary Zalo; mọi liên kết tải Zalo trỏ nguồn chính thức (zalo.me) hoặc GitHub Releases của NexaDock.
