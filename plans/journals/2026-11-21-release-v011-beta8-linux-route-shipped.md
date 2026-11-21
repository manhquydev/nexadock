---
title: Release v0.1.1-beta.8 — Linux route shipped
date: 2026-11-21
summary: "Beta.8 released: Linux Experimental route, resume verified live, paste/tray/icons shipped; unsigned macOS DMG via dispatch flow."
---

# Release v0.1.1-beta.8 — Linux route shipped

## What happened
Tagged v0.1.1-beta.8 at 2f018f9. Tag release run failed macOS as designed (no Apple secrets); dispatched workflow_dispatch with allow_unsigned_dev_artifacts → all jobs green, 9 assets published (NexaDock deb + Linux AppImage/deb + Windows + unsigned macOS DMG + provenance).
Discriminating session test on the real profile: graceful quit (SIGTERM → SNI unregister, cookie flush to disk confirmed by mtime) → relaunch → straight to index.html (main window geometry 1000x720 vs login 420w), chat history loaded. Root cause of the user's earlier "QR again" report: their failing cycle ran with ZALO_DATA_DIR under /tmp, which the app's own quit-time cleanup deletes (appData=/tmp). Real profiles under ~/.local/share are safe; NexaDock route pins data inside the profile dir.

## Decision
- Release notes explicitly document unsigned macOS DMG + Experimental route limits.
- Left the user's live chat instance running (debug port 9361 attached from verification; disappears on next natural restart).

## Next steps
- User: click "Đồng bộ ngay" once to finish the interrupted sync; visually confirm dock logo after one more restart.
- Track: test_17c live-process quit-signal coverage; bwrap /opt bind if bwrap route ever enabled.

> Historical work record — not durable authority. Prefer docs/specs/ADRs for current decisions.
