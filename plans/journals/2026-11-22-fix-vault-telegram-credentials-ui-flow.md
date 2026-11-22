---
title: Fix vault Telegram credentials UI flow
date: 2026-11-22
summary: UI API ID/Hash fields + persisted 0600 config + reconnect from config
---

# Fix vault Telegram credentials UI flow

Telegram Vault fix: UI API ID/Hash fields (visible when credentialsAvailable=false) sent with vault_login_phone; phone input normalization; PairFlow credentialsFromEnv -> credentialsAvailable; VaultConfig gains telegram_api_id/api_hash stored 0600 for reconnect (config-first, env fallback). Evidence: fmt/clippy/test gates green (known pre-existing #14 macos_scenarios failure excluded), node UI/i18n/brand gates 33/33, deb built+installed with new markers. PR #15 commit b3cd549.

> Historical work record — not durable authority. Prefer docs/specs/ADRs for current decisions.
