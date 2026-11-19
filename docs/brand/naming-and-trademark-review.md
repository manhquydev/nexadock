# Naming & Trademark Risk Assessment — **NexaDock** (`PROVISIONAL`)

> **Document Status:** Working Brand Clearance Assessment (Not Legal Advice)  
> **Selected Working Name:** **`NexaDock`** — **`PROVISIONAL` (Not Legally Cleared)**  
> **Project:** Independent desktop account manager compatible with the officially installed Zalo Desktop application  
> **Repository:** `https://github.com/manhquydev/zalo` (remote repository NOT renamed pending formal clearance & owner authorization)

---

## 1. Mandatory Brand Independence & Compatibility Policy

1. **"Zalo" is never used as the primary product brand, logo, icon, or publisher identity.**
2. **Compatibility Statement (EN):** *"Independent desktop account manager compatible with the officially installed Zalo Desktop application."*
3. **Compatibility Statement (VI):** *"Trình quản lý đa tài khoản độc lập trên máy tính, tương thích với ứng dụng Zalo Desktop cài đặt chính thức."*
4. **Non-Affiliation Disclaimer:** NexaDock is an independent open-source project (`MIT OR Apache-2.0`) and is **not affiliated with, sponsored by, or endorsed by VNG Corporation or Zalo**. `"Zalo"` is a trademark of VNG Corporation. No VNG binaries are bundled, patched, or redistributed.

---

## 2. Candidate Name Evaluation & Clearance-Oriented Search

We evaluated the working candidate **`NexaDock`** alongside two distinct alternatives (**`SiloDock`**, **`KovaDock`**) and the prior internal baseline (**`KHOANG`**) across GitHub, web search results, commercial software directories, domain patterns, and accessible trademark registry references (WIPO Global Brand Database / IP Vietnam NOIP / USPTO Class 9 & Class 42 software).

> [!WARNING]
> **Legal Clearance Disclaimer:** Automated web, GitHub, and public directory screening cannot constitute formal legal clearance. Comprehensive availability searches in **WIPO Madrid**, **IP Vietnam (Cục Sở hữu trí tuệ — NOIP)**, and **USPTO/EUIPO** databases under Nice Classes **09** (downloadable desktop utility software) and **42** (software tools) require qualified IP counsel before commercial launch, domain registration, code-signing certificate issuance under the brand name, or public repository renaming.

### 2.1 Candidate Screening Findings

| Candidate | Search Findings (Web, GitHub, Products, Domains, Trademark Context) | Residual Collision / Confusion Risks |
| :--- | :--- | :--- |
| **`NexaDock`** *(Rank 1 — Selected Provisional)* | • **GitHub & Open-Source Software:** Zero active multi-account managers, Tauri shells, or Zalo profile isolation apps found under `NexaDock`.<br>• **Commercial Products & Hardware:** "NexaDock" appears on generic e-commerce multi-port laptop hubs/docking stands. **NexDock** (Nex Computer LLC) is an established smartphone-to-laptop hardware dock brand.<br>• **Domains & Marketplaces:** Brandable `-dock` variations appear on startup domain marketplaces (e.g., Atom).<br>• **Trademark Status:** Formal WIPO/NOIP Class 09 software search is incomplete → **`PROVISIONAL`**. | **Low-to-Moderate** in desktop account management software; **Moderate** phonetic similarity to *NexDock* (hardware peripheral / Class 09 lapdocks). |
| **`SiloDock`** *(Rank 2 — Alternative A)* | • **GitHub & Open-Source Software:** Zero exact desktop profile-manager collisions; adjacent projects exist using `Silo` (container/browser isolation) or `-dock` (Docker UIs).<br>• **Commercial Software:** **Silo** (Authentic8) is a well-known isolated web browser enterprise product in cybersecurity.<br>• **Trademark Status:** Incomplete → **`PROVISIONAL`**. | **Moderate** category overlap in cybersecurity/browser isolation due to *Silo* brand awareness. |
| **`KovaDock`** *(Rank 3 — Alternative B)* | • **GitHub & Open-Source Software:** Zero significant open-source repositories or commercial desktop products found under `KovaDock`.<br>• **Pronunciation & Meaning:** Coined root (`Kova`, evoking cove/compartment and echoing Vietnamese *"khoang"* / *"khóa"*) paired with `Dock`.<br>• **Trademark Status:** Incomplete → **`PROVISIONAL`**. | **Low** collision density, though less immediately descriptive to English-speaking desktop users than `NexaDock`. |
| **`KHOANG`** *(Rank 4 — Prior Internal Baseline)* | • **Web / Vietnamese Software:** Collides heavily in Vietnamese web searches with *"kho hàng"* (warehouse/inventory management) and *"khóa ứng dụng"*.<br>• **Cross-Platform / English Fit:** Difficult spelling and pronunciation for non-Vietnamese users. | **Moderate-to-High** search discoverability noise in Vietnamese enterprise software. |

### 2.2 Comparative Ranking Matrix (1 = Weakest, 5 = Strongest)

| Criterion | `NexaDock` *(Provisional)* | `SiloDock` *(Alt A)* | `KovaDock` *(Alt B)* | `KHOANG` *(Baseline)* |
| :--- | :---: | :---: | :---: | :---: |
| **Distinctiveness vs. Zalo / VNG** | 5 | 5 | 5 | 5 |
| **Memorability** | 5 | 4 | 4 | 3 |
| **Vietnamese & English Pronunciation** | 5 (*"Nê-xa-đốc"*) | 4 (*"Sai-lô-đốc"*) | 4 (*"Cô-va-đốc"*) | 2 (hard for EN) |
| **Cross-Platform Desktop Utility Fit** | 5 | 4 | 4 | 3 |
| **Future Extensibility (Beyond Zalo)** | 5 | 5 | 4 | 3 |
| **Search Discoverability** | 4 | 4 | 5 | 2 |
| **Trademark / Confusion Risk (Inverse; 5 = Lowest Risk)** | 3 (`PROVISIONAL`) | 3 (`PROVISIONAL`) | 4 (`PROVISIONAL`) | 3 (`PROVISIONAL`) |
| **Weighted Total** | **32 / 35** | **29 / 35** | **30 / 35** | **21 / 35** |

**Decision:** Adopt **`NexaDock`** as the **provisional** working product brand across the UI, Tauri metadata, brand assets, and documentation via reversible configuration, while explicitly flagging `PROVISIONAL` legal-clearance status until formal WIPO/NOIP/USPTO IP clearance is completed.

---

## 3. Codebase Occurrence Classification Inventory (Zero-Data-Loss Migration)

All occurrences of `"zalo"`, `"Multi-Zalo"`, and `"KHOANG"` across the repository were classified into five strict categories before modification:

| Category | Scope & Rule | Concrete Touchpoints in This Repository |
| :--- | :--- | :--- |
| **A. Public-Facing Product Branding** | **Replaced with `NexaDock`** (reversible config/strings) | • `tauri.conf.json`: `productName: "NexaDock"`, `windows[0].title: "NexaDock"`, `bundle.shortDescription`, `bundle.longDescription`, `bundle.publisher: "NexaDock Contributors"`<br>• `apps/desktop/src-tauri/Cargo.toml` (`description`), `src/main.rs` (init/run messages), `capabilities/default.json` (`description`)<br>• `src/tray.rs`: Localized tray tooltips & menu labels (`"Mở NexaDock"`, `"Thoát NexaDock"`, `"Open NexaDock"`, `"Quit NexaDock"`)<br>• `src/autostart.rs`: `AUTOSTART_ENTRY_NAME = "NexaDock"`, `nexadock.desktop` (with safe cleanup of legacy `"KHOANG"` Run key / `khoang.desktop` entry)<br>• `apps/desktop/ui/index.html`, `i18n/vi-VN.json`, `i18n/en-US.json`<br>• `README.md`, `README.en.md`, `desktop-instance-manager/README.md`, `.github/ISSUE_TEMPLATE/bug_report.md` |
| **B. Necessary Compatibility References** | **Retained accurately** as external target references | • `"Zalo"`, `"Zalo Desktop"`, `"Zalo Web (Thử nghiệm)"`, `"Zalo đang dùng trên máy"`, `"VNG Corporation"` in UI copy, help cards, diagnostics groups, and disclaimers |
| **C. Native Executable Paths, Process Names & Security Checks** | **Preserved 100%** | • `Zalo.exe`, `ZaloCall.exe`, `ZaloCap.exe`, `ZaviMeet.exe`, `/Applications/Zalo.app`<br>• `%APPDATA%\ZaloData`, `~/Library/Application Support/ZaloData`<br>• `WinVerifyTrust`, `CN/O=VNG GROUP JSC`, `com.vng.zalo`, `PipeZCallRecv`, `socketzalorecv2021`<br>• `PROTECTED_PRIMARY_PROCESS`, `PROTECTED_PRIMARY_PATH`, `DUPLICATE_ZALO_USER_ID` |
| **D. Persistent Identifiers, Storage Paths & IPC Contracts** | **Preserved 100%** (zero profile reset / zero re-login) | • `tauri.conf.json` `identifier`: `"com.multizalodesktop.manager"`<br>• `crates/core/src/models.rs` `APP_DIR_NAME`: `"MultiZaloDesktop"` (`%LOCALAPPDATA%\MultiZaloDesktop`, etc.)<br>• `localStorage`: `'multi-zalo.locale'`, `'multi-zalo.theme'`, `'multi-zalo.accountColors'`, `'khoang.pinnedAccounts'`, `'khoang.railExpanded'`, `'khoang.activeAccountId'`, `'khoang.viewMode'`<br>• `preferences.rs`: `"khoang-preferences.json"`; `tray.rs`: `"khoang-tray"`; Tauri events: `"khoang://navigate"`, `"khoang://activate-request"`, `"khoang://snapshot-changed"`<br>• Bridge auth: `X-DIM-Session-Token` & `meta[name="dim-session-token"]` |
| **E. Tests, Fixtures & Documentation** | **Updated to validate `NexaDock` + preserved keys** | • `tests/brand-release-gates.mjs`, `tests/run-fixture-tests.mjs`, `tests/run-tauri-native-e2e.mjs`, `tests/windows-acceptance/installer-sandbox-check.ps1`, `ui/tests/visual-cdp.mjs`<br>• New deliverables under `docs/brand/` and `docs/design/` |

---

## 4. Release & Governance Gates (Not Executed Autonomously)

The following irreversible external actions remain **explicitly gated** on formal trademark clearance and repository owner approval:
1. Renaming the remote GitHub repository (`manhquydev/zalo`).
2. Registering public domains (e.g., `nexadock.*`) or trademark filings.
3. Publishing signed production installers (`Authenticode OV/EV`, `Apple Developer ID + Notarization`).
