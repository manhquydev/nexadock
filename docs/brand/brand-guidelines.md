# NexaDock — Brand Guidelines *(PROVISIONAL)*

> **Product Name:** `NexaDock` *(Status: `PROVISIONAL` pending formal trademark clearance)*  
> **Positioning:** Independent desktop account manager compatible with the officially installed Zalo Desktop application  
> **Tagline (EN):** *"Your accounts. Their own isolated space."*  
> **Tagline (VI):** *"Một máy. Nhiều tài khoản. Không trộn dữ liệu."*  
> **Design Direction:** *Refined Desktop Utility* — calm, capable, precise, trustworthy  
> **Non-Affiliation:** NexaDock is an independent open-source project (`MIT OR Apache-2.0`). It is not affiliated with, sponsored by, or endorsed by VNG Corporation or Zalo.

---

## 1. Brand Brief & Visual Exclusions

- **Product Purpose:** Isolate multiple desktop messaging accounts into independent local profile directories, backups, and OS windows without disturbing the user's primary installation.
- **Target Users:** Professionals, customer-support operators, educators, and small-business teams managing 2–20+ accounts daily on Windows, macOS, or experimental Linux hosts.
- **Brand Character:** Calm, architectural, trustworthy, keyboard-first, and honest about OS constraints.
- **Strict Exclusions:**
  - Zero Zalo/VNG brand imitation (no `#0068FF` electric blue, no chat speech bubbles, no `"Z"` brand mark).
  - Zero generic SaaS tropes (no neon gradients, no `backdrop-filter` glassmorphism, no decorative glows, no emoji-based icons).

---

## 2. Logo System — "The Docked Bays" (Concept A)

### 2.1 Concept Selection Rationale
Three original geometric concepts were crafted and tested at `16px`, `24px`, `32px`, `64px`, `128px`, and `256px` across light (`#F6F7F5`), dark (`#0F171A`), monochrome, and 100% grayscale surfaces (`docs/brand/assets/concepts/concept-comparison-sheet.png`):

1. **Concept A — "The Docked Bays" (`concept-a-docked-bays.svg`, SELECTED):** Combines a persistent left dock rail spine with two vertically separated workspace bays (an active `#247D70` upper bay with precision status pin and a distinct lower standby bay). Directly mirrors NexaDock's actual rail-and-stage desktop architecture, scales cleanly down to `16px` via a thickened optical small variant, and creates zero confusion with Zalo's speech-bubble mark.
2. **Concept B — "The Nexus Bridge" (`concept-b-nexus-bridge.svg`, Rejected):** Dual vertical bars joined by a diagonal bridge forming an `N`. Readable at 64px+, but risks reading as a generic developer-tool monogram rather than isolated account bays.
3. **Concept C — "The Shielded Compartment" (`concept-c-shielded-stack.svg`, Rejected):** Hexagonal shield outline around two horizontal bars. Rejected because shield geometry is an overused security cliché and loses corner crispness at `16px`.

### 2.2 Master SVG & Icon Inventory (`docs/brand/assets/` & `src-tauri/icons/`)
All assets are deterministically reproducible via `node docs/brand/scripts/generate-brand-assets.mjs`:

| Asset | Purpose | Location |
| :--- | :--- | :--- |
| **Primary Mark (`24×24`)** | Light UI mark & documentation | `docs/brand/assets/nexadock-mark.svg`, `nexadock-light.svg` |
| **Optical Small Mark (`16×16`)** | Small-size favicon & crisp 16–24px rendering | `docs/brand/assets/nexadock-mark-small.svg`, `marketing/favicon.svg` |
| **Dark & Monochrome Marks** | Dark mode, print, `forced-colors: active` | `nexadock-dark.svg`, `nexadock-mono-black.svg`, `nexadock-mono-white.svg` |
| **Wordmark & Lockups** | Horizontal header & compact lockup | `nexadock-wordmark.svg`, `nexadock-lockup.svg`, `nexadock-lockup-compact.svg` |
| **App Icon Master (`256×256`)** | Rounded `#182B32` Deep Harbor squircle tile | `docs/brand/assets/nexadock-app-icon.svg` |
| **Desktop Bundle Icons** | Tauri Windows/macOS/Linux packages | `desktop-instance-manager/apps/desktop/src-tauri/icons/` (`16`..`1024.png`, `icon.ico`, `icon.icns`) |
| **System Tray Icons** | OS taskbar / menu bar status icons | `src-tauri/icons/tray/` (`tray-windows.ico/png`, `tray-macos-template*.png`, `tray-linux.png`, `tray-attention.png`) |
| **README & Release Graphics** | GitHub hero banner, social card, release header | `docs/brand/assets/marketing/` (`github-readme-hero.svg/png`, `release-banner.svg/png`, `github-social-preview.png`) |

### 2.3 Clearspace & Optical Rules
- **Minimum Clearspace:** `0.5 × mark width` on all four sides.
- **Minimum Sizes:** `16×16px` (`nexadock-mark-small.svg` / `APP_ICON_MICRO_SVG` with thickened strokes and solid bays); `20×20px` in the app dock rail; `24×24px` in horizontal lockups.
- **Wordmark Styling:** `NexaDock` in title/camel case (`Nexa` in Brand Ink `#182B32` or `#EAF2EF`, `Dock` in Teal Accent `#247D70` or `#2DD4BF` on dark), `-0.015em` tracking.

---

## 3. Core Palette & Typography Summary

- **Light Canvas / Surface / Ink:** `#F6F7F5` canvas · `#FFFFFF` surface · `#EDF2EF` subtle · `#182B32` ink · `#247D70` primary teal accent (`4.95:1` with `#FFFFFF`) · `#DCE8E2` soft tint.
- **Dark Canvas / Surface / Ink (Deep Harbor):** `#0F171A` canvas · `#162126` surface · `#1D2B31` subtle · `#EAF2EF` ink · `#247D70` fill / `#5EEAD4` text link accent · `#173632` soft tint.
- **Typography:** `"Inter", "Be Vietnam Pro", "Segoe UI Variable Text", "Segoe UI", -apple-system, BlinkMacSystemFont, "Noto Sans", sans-serif` (UI), `"JetBrains Mono", ui-monospace, "Cascadia Mono", Consolas, monospace` (paths, codes, logs). Strictly `rem`/`em` scaling for 200% OS zoom compatibility, with zero external CDN requests per Tauri CSP (`default-src 'self'`).
