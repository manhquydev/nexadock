---
title: "Zalo Linux: paste, tray, icons, session-resume fixes"
date: 2026-11-21
summary: Root-caused and fixed four Linux repack UX defects; verified session persistence end-to-end on the real profile.
---

# Zalo Linux: paste, tray, icons, session-resume fixes

## What happened
Fixed four verified defects in the zalo-linux repack (macOS client 26.11.10 in Electron 22.3.9):
1. Clipboard image paste: renderer paste handler has no Linux image branch — Ctrl+V degraded to file transfer. Vendored community patch-pasting-img.js (standalone) adds clipboard helpers to 5 preload bundles + paste handler feeding drag/drop overlay. Marker-verified on 26.11.10; rebuild reproduces byte-identical preloads.
2. Missing window icon (generic gear in dock): BrowserWindow options had no icon; also only a single 1024px icon was installed so Gtk lookup at dash sizes (96/128) returned NOT-FOUND. Injected zalo.png into all 5 window-creation sites; build-deb.sh now ships the full hicolor size set (16-1024). Gtk lookup verified FOUND at 96/128/256.
3. No tray (macOS client uses Dock; close-to-background was unreachable): shim now creates a Linux tray (official icon, click reopens user windows, menu quit). First version revealed hidden utility windows (sqlite.html/shared-worker.html); fixed with URL filter (index/login only).
4. Session lost on restart: resume gate reads zpw_sek from persist:zalo partition; earlier stub-sqlite3 build broke the login chain so cookies/DB were never established. After the sqlite3/db-cross/sync fixes, a real-profile restart verified live: straight to index.html, chat history loaded (screenshot evidence), zpw_sek persistent to 2027 in Partitions/zalo/Cookies.
Note: userData under /tmp is unsafe — the app's quit-time cleanup removed /tmp/zalo-e2e entirely. Real profiles under ~/.local/share/zalo-linux are unaffected; NexaDock route pins ZALO_DATA_DIR inside the profile dir (safe).

## Decision
- Vendored patch-pasting-img.js as standalone (no fs-extra/logger deps) like the sync patch; both hard-FATAL when missing from rebuild pipeline.
- Tray shows only index/login windows; utility windows stay hidden forever.
- build-deb.sh generates the full icon size set instead of one 1024px PNG.

## Next steps
- User completes the interrupted message sync (click "Đồng bộ ngay").
- test_17c quit-signal coverage with a live route process remains a tracked gap.
- paste smoke is manual (user Ctrl+V); automated CDP paste test not yet built.

> Historical work record — not durable authority. Prefer docs/specs/ADRs for current decisions.
