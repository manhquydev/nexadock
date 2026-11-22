---
title: Telegram vault two-tier transport
date: 2026-11-22
summary: Bot API transport default (ureq 2 rustls), MTProto optional advanced, config Transport enum, vault_pair_bot/_wait commands, UI mode chooser
---

# Telegram vault two-tier transport

Vault Telegram wave two (tầng hai): Bot API transport mặc định dùng ureq 2 + rustls — getMe/getUpdates polling và sendDocument multipart tự dựng, không SDK; MTProto giữ làm tùy chọn nâng cao. Config thêm Transport enum, default mtproto để tương thích ngược với config cũ; transport_kind chặn trộn hai kênh trong một vault. Commands mới: vault_pair_bot / vault_pair_bot_wait. UI thêm mode chooser (Bot API vs MTProto). Gates xanh: workspace clippy/test, src-tauri 23 tests, vault 17 tests, node 39 tests, cargo-deny. PR #15 commit 5d8b880.

> Historical work record — not durable authority. Prefer docs/specs/ADRs for current decisions.
