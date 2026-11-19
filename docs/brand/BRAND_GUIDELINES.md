# KHOANG — Brand Guidelines & Design System (Superseded)

> **SUPERSEDED:** This earlier KHOANG v1 brand document is preserved for historical context and has been superseded by the **NexaDock (PROVISIONAL)** brand and Refined Desktop Utility overhaul:
> - Brand & Trademark Clearance Review: [naming-and-trademark-review.md](naming-and-trademark-review.md)
> - Current Brand Guidelines: [brand-guidelines.md](brand-guidelines.md)
> - Current Design System: [../design/design-system.md](../design/design-system.md)

> **Product Name:** `KHOANG` (Technical short name: `khoang`)
> **Positioning:** *Independent Account Workspace*
> **Tagline (EN):** *"Your accounts. Their own space."*
> **Tagline (VI):** *"Một máy. Nhiều tài khoản. Không trộn dữ liệu."*
> **Design Direction:** Calm Precision — warm, precise, minimal, refined.
> **Non-Affiliation:** KHOANG is an independent open-source workspace manager (`MIT OR Apache-2.0`). It is not affiliated with, endorsed by, or bundled with VNG Corporation or Zalo.

---

## 1. Brand Essence & Metaphor

### 1.1 The "Khoang" Metaphor
In Vietnamese, **"khoang"** means a *compartment*, *bay*, or *dedicated cabin*—a clearly bounded, self-contained space that sits alongside other spaces without leaking or colliding.

- **Product Promise:** Each Zalo account lives in its own isolated, calm space (`Profiles/<slug>_<id>/`), while your primary Zalo installation remains untouched and protected.
- **Visual Personality:** Architectural, orderly, quiet, and trustworthy. Inspired by the precision of **Linear** and **Raycast**, the multi-workspace clarity of **Rambox**, and the native-feeling restraint of **Fluent 2**—without copying any of them.
- **Anti-Patterns (Strictly Avoided):**
  - No generic cold SaaS blue dashboards (`#0f6cbd` / `#f5f6f8` / `#131519`).
  - No neon gradients, decorative glows, or `linear-gradient` / `radial-gradient` / `conic-gradient` surfaces (also enforced by `ui-contracts.test.mjs`).
  - No `backdrop-filter` glassmorphism (enforced by `ui-contracts.test.mjs`).
  - No Zalo/VNG visual imitation (no electric `#0068FF` blue, no speech-bubble "Z" motifs).

---

## 2. Logo & Wordmark System

### 2.1 Logo Concept: "The Partitioned Bay"
The KHOANG mark represents **two or more calm, separated compartments inside a single precision frame**:
- **Outer Frame:** A squircle-rounded architectural container (`rx="5"` on a `24×24` grid) symbolizing the desktop host boundary.
- **Inner Partitions:** Two distinct vertical/asymmetric compartments separated by a deliberate, clean **2px breathing gap**—representing isolated account spaces that coexist side-by-side without touching each other's data, locks, or processes.
- **Safety Anchor:** The left compartment is subtly taller/anchored (representing the protected primary/anchor space or active bay), while the right compartment is modular—evoking calm order rather than chat bubbles.

### 2.2 SVG-Ready Construction Rules
All logo assets MUST be inline `<svg>` or local `img-src 'self'` assets (zero external URLs per Tauri CSP).

| Property | Specification |
| :--- | :--- |
| **Artboard / `viewBox`** | `0 0 24 24` (UI/Favicon), `0 0 32 32` (App Icon / Hero) |
| **Stroke / Fill Style** | Geometric solid/stroke hybrid; crisp integer coordinates on a `24×24` pixel grid |
| **Outer Frame (`24×24`)** | `x="2" y="2" width="20" height="20" rx="5"`, `stroke-width="1.75"` |
| **Inner Divider / Bays** | Left bay: `x="5.5" y="5.5" width="5.5" height="13" rx="2"` (solid fill); Right upper bay: `x="13" y="5.5" width="5.5" height="6" rx="1.75"` (solid fill); Right lower bay: `x="13" y="13.5" width="5.5" height="5" rx="1.75"` (accent/60% opacity or outlined) |
| **Minimum Clearspace** | `0.5 × mark width` on all four sides (e.g., `12px` around a `24px` mark) |
| **Minimum Display Size** | `16×16px` (tray/favicon, simplified 2-bay cut), `24×24px` (topbar lockup) |

**Reference Inline SVG (`24×24`):**
```xml
<svg viewBox="0 0 24 24" width="24" height="24" fill="none" aria-hidden="true">
  <rect x="2" y="2" width="20" height="20" rx="5" stroke="currentColor" stroke-width="1.75" />
  <rect x="5.5" y="5.5" width="5.5" height="13" rx="1.75" fill="currentColor" />
  <rect x="13" y="5.5" width="5.5" height="6" rx="1.5" fill="var(--brand-accent, currentColor)" />
  <rect x="13" y="13" width="5.5" height="5.5" rx="1.5" stroke="currentColor" stroke-width="1.5" />
</svg>
```

### 2.3 Wordmark & Lockup Rules
- **Wordmark:** `KHOANG` in uppercase with `+0.04em` letter-spacing (`font-weight: 700`) for the primary brand lockup; `Khoang` in title case when referenced inline in body copy or legal notices.
- **Primary Horizontal Lockup:** `24×24` mark + `10px` (`0.625rem`) gap + `KHOANG` wordmark (`1rem` / `16px`, `font-weight: 700`).
- **Compact Lockup (Narrow Viewports `< 520px`):** `24×24` mark + `KHOANG` wordmark (subtitle/tagline hides cleanly rather than wrapping awkwardly).
- **Color Variants:**
  - *Light Mode:* Brand Ink `#192A26` frame + Primary `#17675F` accent bay on Light Canvas `#F6F5F2`.
  - *Dark Mode:* Mint Highlight `#BCE5D8` / Warm Ivory `#F2F0EA` frame + `#2DD4BF` / `#17675F` accent bay on `#121210` dark canvas.
  - *Monochrome / `forced-colors: active`:* `currentColor` / `CanvasText` throughout (`khoang-mono-black.svg`, `khoang-mono-white.svg`).
- **Optical Correction (`16px–24px`):** `khoang-mark-small.svg` thickens the outer frame stroke (`2.2px` on `24×24` grid) and simplifies the right-hand bay into a single solid `#17675F` block so sub-pixel edges stay crisp at `16×16` and `20×20` taskbar/tray sizes.

---

## 3. Typography System

### 3.1 Font Families (Vietnamese-Verified & Permissive Open License)
Every font in the KHOANG stack is licensed under **SIL Open Font License 1.1 (OFL-1.1)**, supports **full Vietnamese diacritics** (stacked tone marks, circumflexes, horns, and hooks without clipping), and MUST be **self-hosted as local WOFF2 files** (or fall back cleanly to system UI fonts) to comply with Tauri's strict CSP (`default-src 'self'`).

| Role | Primary Face | License | Vietnamese Support | Fallback Stack |
| :--- | :--- | :--- | :--- | :--- |
| **UI & Headings** | **Be Vietnam Pro** | `OFL-1.1` | Native, purpose-built for Vietnamese diacritic vertical metrics | `"Be Vietnam Pro", "Inter", system-ui, -apple-system, "Segoe UI", Roboto, sans-serif` |
| **Secondary UI** | **Inter** | `OFL-1.1` | Full Latin Extended / Vietnamese glyph coverage | `"Inter", system-ui, -apple-system, "Segoe UI", sans-serif` |
| **Monospace / Tech** | **JetBrains Mono** | `OFL-1.1` | Full Vietnamese diacritics across all weights | `"JetBrains Mono", ui-monospace, "Cascadia Mono", "SFMono-Regular", Consolas, monospace` |

> **CSP & Packaging Note:** Never load fonts from `fonts.googleapis.com` or any remote CDN. Bundle `.woff2` subsets (`vietnamese` + `latin`) locally under `ui/fonts/` or rely on the system fallback stack if font files are not yet vendored.

### 3.2 Type Scale (Strictly `rem` / `em`)
All font sizes MUST use `rem` or `em` (never `px`) to respect `<meta name="text-scale" content="scale">` and OS/browser text scaling up to 200% (`ui-contracts.test.mjs` contract).

| Token | Size (`rem`) | Approx (`16px` base) | Weight | Line Height | Usage |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `--text-xs` | `0.75rem` | `12px` | `500` | `1.35` | Metadata labels, keyboard hints, table headers |
| `--text-sm` | `0.8125rem` | `13px` | `400` / `500` | `1.45` | Secondary descriptions, status badges, menu hints |
| `--text-base` | `0.9375rem` | `15px` | `400` / `500` | `1.5` | Body copy, form inputs, card titles (`1rem` bold) |
| `--text-lg` | `1.125rem` | `18px` | `600` | `1.35` | Section headers, dialog titles |
| `--text-xl` | `1.375rem` | `22px` | `700` | `1.25` | View titles (`Tài khoản`, `Sao lưu`), empty-state hero |

---

## 4. Color System (Warm Neutrals + Calm Teal Accent)

All color tokens use CSS `light-dark()` with `:root { color-scheme: light dark; }` and explicit `[data-theme="light"]` / `[data-theme="dark"]` overrides. **No gradients** are permitted.

### 4.1 Surface & Text Tokens

| Token | Light Mode | Dark Mode | Role & Contrast Notes |
| :--- | :--- | :--- | :--- |
| `--bg-canvas` | `#F6F5F2` | `#121210` | Warm stone alabaster (`#F6F5F2` locked) / warm obsidian app canvas |
| `--bg-surface` | `#FFFFFF` | `#1B1B18` | Elevated card, dialog, and panel surface |
| `--bg-surface-subtle` | `#EFECE5` | `#23231F` | Recessed regions (Primary Zalo protected zone, table headers) |
| `--bg-surface-hover` | `#EAE6DD` | `#2A2A25` | Subtle hover state for ghost buttons and list rows |
| `--border-hairline` | `#E2DDD3` | `#2F2F2A` | Crisp `1px` card, divider, and input borders (clearly visible in Dark mode) |
| `--border-strong` | `#C7C0B3` | `#44443D` | Hover borders, segmented control outlines |
| `--text-primary` | `#192A26` | `#F2F0EA` | Brand Ink (`#192A26` locked) & warm ivory body (`> 13:1` contrast on surface) |
| `--text-secondary` | `#57534E` | `#B5B0A6` | Descriptions, metadata (`> 6.5:1` contrast on surface) |
| `--text-muted` | `#6E6A63` | `#918C82` | Captions, placeholders (`>= 4.6:1` contrast on surface) |

### 4.2 Brand Accent Tokens (Distinct from Zalo Blue)

| Token | Light Mode | Dark Mode | Usage & WCAG AA Verification |
| :--- | :--- | :--- | :--- |
| `--accent-primary` | `#17675F` | `#1E7E74` | Primary CTA fill (`#17675F` locked; `#FFFFFF` text: `6.6:1` light / `4.9:1` dark) |
| `--accent-primary-hover` | `#12534C` | `#25968B` | Primary CTA hover |
| `--accent-primary-text` | `#135851` | `#BCE5D8` | Text links, active nav indicator (`#BCE5D8` mint highlight locked in Dark mode) |
| `--accent-primary-tint` | `#E4F2EE` | `#142E2B` | Active tab/pill background, focus/selection tint (paired with `#BCE5D8` border) |
| `--focus-ring` | `#17675F` | `#BCE5D8` | `2px solid` `:focus-visible` outline with `2px` offset |

### 4.3 Semantic Status Tokens (Never Color-Alone)
Every status indicator MUST combine **Color + Distinct Icon + Text Label** (and border/surface emphasis where needed) so status is unmistakable in grayscale, color-vision deficiency, and `forced-colors: active`.

| State | Light (Text / Tint / Border) | Dark (Text / Tint / Border) | Required Icon + Visual Distinction |
| :--- | :--- | :--- | :--- |
| **`running`** | `#15683A` / `#E9F5EE` / `#B7DFC6` | `#4ADE80` / `#132A1C` / `#225438` | Solid filled circle / active indicator + *"Đang chạy"* / *"Running"* |
| **`stopped`** | `#57534E` / `#EFECE5` / `#D6D0C4` | `#A8A29E` / `#242420` / `#3D3D36` | Hollow circle / neutral indicator + *"Đã dừng"* / *"Stopped"* |
| **`pending`** (`starting`/`stopping`) | `#0D5E5A` / `#E6F4F2` / `#99D5D0` | `#5EEAD4` / `#132A28` / `#1F4D49` | Stepped/spinning ring (static in `prefers-reduced-motion`) + *"Đang mở…"* / *"Đang đóng…"* |
| **`recovery_required`** | `#9A4D06` / `#FEF3C7` / `#E5A93C` | `#FBBF24` / `#2D220B` / `#855812` | Diamond-wrench/alert icon + **explicit amber card border** + *"Cần khôi phục"* / *"Recovery needed"* |
| **`error`** | `#B91C1C` / `#FEE2E2` / `#FCA5A5` | `#F87171` / `#2E1414` / `#7F1D1D` | Triangle-exclamation icon + *"Lỗi"* / *"Error"* |
| **`protected`** (Primary Zalo) | `#44403C` / `#EFECE5` / `#D6D0C4` | `#D6D3CD` / `#23231F` / `#3F3F38` | Shield-lock icon + *"Được bảo vệ · Chỉ đọc"* / *"Protected · Read-only"* |

### 4.4 Account Identity Palette (`--id-1` .. `--id-8`)
All 8 account avatar colors are calibrated to maintain **>= 4.5:1 WCAG AA contrast against `#FFFFFF`** (for white initials inside `.account-avatar`):

| Token | Hex | Hue Name | Contrast vs `#FFFFFF` |
| :--- | :--- | :--- | :--- |
| `--id-1` | `#0F6B66` | Deep Teal (Default) | `6.3:1` |
| `--id-2` | `#1E56A0` | Maritime Blue | `7.1:1` |
| `--id-3` | `#6D3B9E` | Plum Violet | `7.4:1` |
| `--id-4` | `#A92B5B` | Berry Rose | `6.4:1` |
| `--id-5` | `#A14A0A` | Burnt Amber | `5.9:1` |
| `--id-6` | `#1C6E3B` | Forest Pine | `6.2:1` |
| `--id-7` | `#0E6488` | Deep Cerulean | `6.3:1` |
| `--id-8` | `#4B468E` | Muted Indigo | `7.9:1` |

---

## 5. Iconography, Layout, Spacing & Motion

### 5.1 Iconography
- **Style:** Clean geometric line icons (`1.5px` stroke, `stroke-linecap="round"`, `stroke-linejoin="round"`), inline `<svg>` only.
- **Sizing:** `16px` (`1rem`) inside buttons, badges, and menus; `20px` (`1.25rem`) in navigation and section headers; `32px` (`2rem`) in empty states.

### 5.2 Layout & 4px/8px Spacing Scale
- **Spacing Scale:** `4px` (`0.25rem`), `8px` (`0.5rem`), `12px` (`0.75rem`), `16px` (`1rem`), `20px` (`1.25rem`), `24px` (`1.5rem`), `32px` (`2rem`), `48px` (`3rem`).
- **Radii:** `6px` (chips/inputs/small buttons), `8px` (standard buttons/menus), `12px` (account cards & panels), `16px` (modals/dialogs), `9999px` (status pills & avatar badges).
- **Grid & Scale:**
  - Support both **Grid View** (auto-fit cards with balanced height, no awkward empty gaps when notes are absent) and **Compact List View** (essential for 10–20+ accounts).
  - Responsive breakpoints at **`420px`** (narrow utility), **`768px`** (compact desktop), and **`1160px`** (standard window), with zero horizontal overflow up to **200% text/browser zoom**.

### 5.3 Motion
- **Timing:** `140ms cubic-bezier(0.2, 0, 0, 1)` for hover, focus ring, and surface state transitions; `180ms` for dialog entrance.
- **Reduced Motion:** Under `@media (prefers-reduced-motion: reduce)`, all transitions and keyframe animations MUST resolve immediately (`transition-duration: 0.01ms !important; animation-duration: 0.01ms !important;`).

---

## 6. Component States Matrix

Every interactive component MUST implement all 9 states consistently:

| State | Visual & Behavioral Specification |
| :--- | :--- |
| **Default** | Crisp `1px` `--border-hairline`, `--bg-surface` or `--accent-primary` fill, clear label/icon. |
| **Hover** | Subtle surface shift (`--bg-surface-hover` or `--accent-primary-hover`) and `--border-strong`; never shifts layout. |
| **Active / Pressed** | Slight tactile darken or `scale(0.99)` (disabled under `prefers-reduced-motion`). |
| **Focus-Visible** | `2px solid var(--focus-ring)` with `2px` `outline-offset`; never suppressed with `outline: none`. |
| **Disabled** | `opacity: 0.48`, `cursor: not-allowed`, descriptive tooltip/hint when blocked by safety rule (e.g., *"Đóng Zalo trước"*). |
| **Loading** | Inline spinner or skeleton placeholder + `aria-busy="true"`; preserves layout dimensions to prevent CLS. |
| **Pending** | Transitional state (`starting` / `stopping`) with disabled conflicting actions and clear status badge. |
| **Error** | `--status-error` border/tint, `role="alert"` inline message, and non-destructive recovery path or diagnostic link. |
| **Recovery** (`recovery_required`) | Distinct `--status-recovery` amber border + badge + inline explanation + explicit recovery action label (*"Khôi phục & Mở"* / *"Recover & Open"*). |
