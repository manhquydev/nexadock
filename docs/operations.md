# Zalo desktop trên Linux (bản không chính thức, đã cài trên máy làm việc)

- **Khi nào dùng:** cần chạy/thử nghiệm Zalo desktop đa instance trên Ubuntu thay vì Zalo web hay Wine; NexaDock nhận diện route này ở trạng thái Experimental.
- **Phạm vi:** máy này (Ubuntu 26.04, x86_64, Wayland/X11). Bản chính thức của VNG vẫn KHÔNG có cho Linux — mọi claim "Linux desktop không hỗ trợ" trong [ADR-001](architecture/ADR-001-unified-workspace.md) vẫn đúng khi nói bản chính thức.
- **Nguồn sự thật:** `~/zalo-linux/README.md` (cây nguồn + scripts); package cài đặt: zalo-linux 26.11.10 (deb, cài tại `/opt/zalo-linux`, lệnh `zalo <tên-instance>`); `zalo-linux_26.11.10_amd64.deb` nằm trong `~/zalo-linux/`.
- **Cách tra cứu:** chạy `zalo <tên>` hoặc `~/zalo-linux/zalo-instance.sh <tên>`; dựng lại từ DMG: `ELECTRON_ZIP=<zip> ~/zalo-linux/rebuild-zalo-linux.sh <dmg>`; đọc `/opt/zalo-linux/README.md` sau khi cài.
- **Truy cập & quyền:** client không chính thức (shell tự dựng quanh client macOS chính thức); rủi ro tài khoản đã được user chấp nhận — thử tài khoản phụ trước.
- **Giới hạn:** bản chính thức VNG vẫn không hỗ trợ Linux; route community vẫn Experimental. Trên Wayland, NexaDock không thể focus/activate cửa sổ native; `wmctrl`/`xdotool` chỉ hỗ trợ nhánh X11/XWayland và đã cài trên máy này. WebIsolated vẫn fail-closed nếu host window không dùng Xlib. Gọi thoại/video và đồng bộ sau xác nhận/restart chưa được kiểm chứng đầu-cuối trên tài khoản thử. Chi tiết runtime/patch thuộc `~/zalo-linux/README.md`.
- **Xác minh & fallback:** xác minh = process sống + data dir riêng tại `~/.local/share/zalo-linux/<tên>`; fallback khi bản repack bị chặn: Zalo web hoặc Wine/Bottles (community projects: `VN-Linux-Family/zalo-for-linux`, `digital-enterprise-projects/zalo-linux`).
