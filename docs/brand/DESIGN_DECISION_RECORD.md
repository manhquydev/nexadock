# KHOANG — Design Decision Record (DDR)

**Status:** Approved for Phase 1 / Phase 2 Prototype  
**Scope:** Rebranding & UX upgrade from *Multi-Zalo Desktop* to **KHOANG** (*"Your accounts. Their own space."*)

---

## 1. Preliminary Name & Trademark Screening (`KHOANG`)

### 1.1 Web Search Findings (Preliminary)
Preliminary web searches were conducted for `"KHOANG" app OR software OR desktop`, `"Khoang" trademark OR brand software`, `site:github.com "khoang" app OR desktop OR electron OR tauri`, and `"khoang" "zalo" OR "multi-account" OR "workspace" app`:
- **Software / Desktop App Collisions:** No prominent desktop multi-account workspace manager, Tauri app, or Zalo isolation utility named **KHOANG** appeared in search results. Most Vietnamese software hits relate to *"kho hàng"* (warehouse/inventory management) or *"khóa ứng dụng"* (app lockers), not account-isolation spaces.
- **Trademark / Brand Collisions:** No immediate software-category trademark collision surfaced in general web results.

> [!WARNING]
> **Legal Disclaimer:** These web search results are **preliminary informational checks only and NOT legal confirmation or formal trademark clearance**. Before any commercial distribution, app-store submission, or trademark registration, a formal clearance search with qualified IP counsel (e.g., NOIP/IP Vietnam and target jurisdictions) is required.

### 1.2 VNG / Zalo Non-Affiliation Compliance
- Replacing the generic *"Multi-Zalo Desktop"* title with **KHOANG** (*"Your accounts. Their own space."*) reduces trademark confusion with VNG Corporation's *"Zalo"* mark while keeping the product's purpose clear in descriptive copy.
- The visual identity intentionally avoids Zalo's electric blue (`#0068FF`), speech-bubble motifs, and "Z" letterforms.

---

## 2. Design Direction Survey & Trade-Off Analysis

We evaluated three candidate directions against KHOANG's requirements (daily desktop utility, 0 to 20+ accounts, strict Tauri CSP, high-contrast accessibility, and Vietnamese-first typography):

| Criterion | (a) **Calm Precision** *(Recommended)* | (b) **Instrument Panel** | (c) **Native Minimal** |
| :--- | :--- | :--- | :--- |
| **Visual Character** | Warm stone/obsidian neutrals, Deep Teal accent, crisp `1px` hairlines, generous breathing room. | High-density telemetry, dark slate, monospace-heavy, status LEDs, compact tables. | OS-native chrome mimicry (Fluent 2 / macOS Aqua), system grays, minimal custom identity. |
| **Cognitive Load** | **Low.** Clear visual hierarchy separates active spaces, protected primary Zalo, and recovery alerts. | **Medium–High.** Great for power users at 20+ accounts, but intimidating for everyday users. | **Low**, but lacks distinctive brand character and feels generic across cross-platform WebViews. |
| **Scalability (3 → 20+ Accounts)** | **High.** Balanced Grid view for 1–8 accounts + Compact List toggle for 10–20+ accounts. | **Very High.** Built for dense lists, weak at empty-state onboarding. | **Medium.** Can feel cramped or monotonous at 20+ accounts. |
| **Dark Mode & Contrast** | **Strong.** Warm obsidian `#121210` + `#1B1B18` surfaces with `#2F2F2A` hairlines fix the low-contrast dark mode in `04-account-dock-dark-mode.png`. | **Strong** in dark mode, often harsh in light mode. | **Depends on OS**; WebKitGTK/WebView2 rendering diverges across Windows/macOS/Linux. |
| **Contract & CSP Fit** | **100% compliant.** Flat solid surfaces (zero `linear-gradient`, zero `backdrop-filter`), `rem`-based scale, inline SVGs. | **100% compliant**, though dense telemetry risks horizontal overflow at 200% zoom. | **100% compliant**, but system-font-only defaults can clip Vietnamese stacked diacritics. |

**Decision:** Adopt **(a) Calm Precision** as the primary design direction, borrowing **Compact List density** from (b) Instrument Panel as an optional view toggle for users managing 10–20+ accounts.

---

## 3. Architectural & Visual Decisions (Grounded in Phase 1 Audit)

1. **Eliminate Generic Cold SaaS Blue (`#0f6cbd`):**
   - *Context:* `@auditor` noted generic cold blue/slate (`#0f6cbd` / `#f5f6f8` / `#131519`) and weak dark-mode card borders.
   - *Decision:* Adopt Warm Stone (`#F7F5F0` / `#FFFFFF`) in Light mode and Warm Obsidian (`#121210` / `#1B1B18` with `#2F2F2A` borders) in Dark mode, paired with **Calm Deep Teal** (`#0F6B66` light / `#147A74` dark). This differentiates KHOANG from both generic SaaS templates and Zalo's `#0068FF` blue.

2. **Self-Hosted Vietnamese-First Typography (`Be Vietnam Pro` + `JetBrains Mono`):**
   - *Context:* Tauri's CSP (`default-src 'self'`) forbids remote Google Fonts CDNs, and Vietnamese UI copy requires clean stacked diacritic rendering at `rem` scales up to 200% zoom.
   - *Decision:* Standardize on **Be Vietnam Pro** (OFL-1.1) for UI/headings and **JetBrains Mono** (OFL-1.1) for technical identifiers/diagnostics, bundled locally as `.woff2` (or falling back to `Inter` / system UI fonts).

3. **Separate `recovery_required` from `error`:**
   - *Context:* `@auditor` observed in `06-recovery-required-badge.png` that `recovery_required` shared the same danger red as `error` with no border highlight and a generic `Mở Zalo` CTA.
   - *Decision:* Introduce a dedicated **Amber/Ochre Recovery Token** (`#9A4D06` / `#FEF3C7` / `#E5A93C` light; `#FBBF24` / `#2D220B` / `#855812` dark) with a distinct diamond-alert icon, amber card border, inline recovery hint, and explicit recovery action label.

4. **Elevate Primary Zalo Protection Affordance:**
   - *Context:* `@auditor` noted `#primary-zalo-section` appeared as a plain gray card without an explicit "Protected / Read-only" badge.
   - *Decision:* Style `#primary-zalo-section` with `--bg-surface-subtle` and a `--status-protected` shield-lock badge (*"Được bảo vệ · Chỉ đọc"* / *"Protected · Read-only"*) so users immediately understand why it has 0 mutation buttons.

5. **Preserve All Safety, Accessibility & Test Contracts:**
   - All 8 `--id-*` account identity colors are verified `>= 4.5:1` against `#FFFFFF`.
   - Zero gradients (`linear-gradient`, `radial-gradient`, `conic-gradient`), zero `backdrop-filter`, zero `.innerHTML`, and strict `rem`/`em` font sizing are maintained to pass `ui-contracts.test.mjs` and `i18n.test.mjs`.
