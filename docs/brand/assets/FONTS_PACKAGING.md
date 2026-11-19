# KHOANG — Self-Hosted Font Packaging Guide (Phase 3)

Tauri's strict CSP (`default-src 'self'; style-src 'self'`) forbids loading fonts from remote CDNs such as Google Fonts. All custom fonts MUST be bundled locally under `desktop-instance-manager/apps/desktop/ui/fonts/` or fall back to the offline system UI stack.

## Recommended Fonts (OFL-1.1, Full Vietnamese Diacritics)

1. **Be Vietnam Pro** (UI & Headings) — License: `SIL Open Font License 1.1`
   - Weights needed: `400` (Regular), `500` (Medium), `600` (SemiBold), `700` (Bold)
   - Subsets needed: `vietnamese`, `latin`
   - Source package: `@fontsource/be-vietnam-pro` (npm) or official GitHub releases (`be-vietnam-pro`)
2. **JetBrains Mono** (Diagnostics, Profile IDs, Paths, SHA-256) — License: `SIL Open Font License 1.1`
   - Weights needed: `400` (Regular), `500` (Medium)
   - Subsets needed: `vietnamese`, `latin`
   - Source package: `@fontsource/jetbrains-mono` (npm) or official JetBrains Mono releases

## Phase 3 Local Bundling Steps

1. Extract only the `vietnamese` and `latin` `.woff2` files for the required weights into `desktop-instance-manager/apps/desktop/ui/fonts/` to keep binary size minimal (~120–180 KB total).
2. Declare `@font-face` rules in `ui/tokens.css` with `font-display: swap;` and local relative `url('./fonts/...')` paths.
3. Update `desktop-instance-manager/THIRD_PARTY_NOTICES.md` with the `OFL-1.1` copyright notices for Be Vietnam Pro and JetBrains Mono.
4. Keep the system fallback stack intact (`system-ui, -apple-system, "Segoe UI", Roboto, sans-serif` and `ui-monospace, "Cascadia Mono", Consolas, monospace`) so rendering remains clean even if font files are omitted in minimal test builds.
