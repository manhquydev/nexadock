# Cross-Platform Supply-Chain, RustSec Advisory & Dependency Audit Summary

- **Generated (UTC):** `2026-11-17T17:57:52.333Z`
- **Policy File:** `desktop-instance-manager/deny.toml`
- **Overall Policy Verdict:** **PASS (with MSRV & deny.toml integration warnings)**
- **RustSec Advisory Scanner:** `cargo-deny 0.21.0` against `rustsec/advisory-db` @ `233155b4025b854c68a4b199c0e32d7fd53291a2` (`2026-11-17T11:14:35+01:00`)

---

## 1. Lockfile & Toolchain Scope

| Lockfile | Format | SHA-256 | Total Packages | Notes |
| :--- | :---: | :--- | :---: | :--- |
| `desktop-instance-manager/Cargo.lock` | v4 | `deeb58a04271c4146491f8d0385618838ef64f1cab1194b6f88888e3071040d2` | 41 | Core Rust workspace (`dim-core`, `dim-profile-store`, `dim-process-manager`, OS adapters, `dim-cli`) |
| `desktop-instance-manager/apps/desktop/src-tauri/Cargo.lock` | v4 | `0bcb1b65cc7e53d142c148d306f6e6dfe24a4fce323e18f99c9f6dae5213e1fc` | 434 | Tauri v2 desktop shell (`tauri@2.12.6`, `tauri-build@2.7.5`) |
| **Combined Unique Packages** | — | — | **439** | **431** external `crates.io-index` crates + 8 local workspace crates |

---

## 2. 7-Pillar Supply-Chain Security Separation

Each supply-chain dimension is tracked independently so inventory, checksums, advisories, provenance, signing, and installer integrity are **never** conflated:

| # | Supply-Chain Pillar | Current Status | Grounded Evidence & Strict Boundary |
| :---: | :--- | :--- | :--- |
| **1** | **Lockfile Checksum Verification** | `VERIFIED` | **431 / 431** external `crates.io-index` crates carry valid 64-hex SHA-256 checksums; `0` disallowed registries/git sources; `--locked` enforced. |
| **2** | **License & Crate Ban Policy** | `VERIFIED` | **439** unique crates evaluated via `cargo metadata --locked` & `cargo-deny`: **0** SPDX violations, **0** banned crates (`openssl`, `openssl-sys`). |
| **3** | **Known Vulnerability Advisory Scan** | `VERIFIED_CARGO_DENY_RUSTSEC_CLEAN` | **0 vulnerabilities, 0 yanked crates** via `cargo-deny 0.21.0` against `rustsec/advisory-db` commit `233155b4025b854c68a4b199c0e32d7fd53291a2` on both lockfiles. |
| **4** | **SBOM (Software Bill of Materials)** | `DECLARED_DEPENDENCY_INVENTORY_ONLY` | Declared dependency inventory (439 crates). **An SBOM is an inventory ONLY — NOT independent build provenance, NOT a vulnerability scan, and NOT a signature.** |
| **5** | **Build Provenance** | `SELF_REPORTED_UNATTESTED` | Script-generated Section 7 SHA-256 manifest without OIDC/SLSA v1.0 cryptographic builder attestation (`oidc_slsa_attestation_present: false`). |
| **6** | **Artifact Signature** | `UNSIGNED_DEVELOPMENT_ARTIFACT` | Zero Windows Authenticode (`RVA=0, Size=0`) and zero Apple `Developer ID Application` signatures / notarization tickets. Blocked for production until Gates G3/G4. |
| **7** | **Installer Integrity** | `RAW_EXECUTABLES_ONLY_LOCALLY_CI_OVERLAY_READY` | Raw binaries (`dim-desktop.exe`, `dim-cli.exe`, `dim-desktop`) have `is_installer: false`; CI bundler overlay (`tauri-ci-bundle-overlay.json` + valid `icon.icns`/`128x128@2x.png`) ready for native runner packaging. |

---

## 3. Detailed Policy & Zero-VNG Checks

| Check | Status | Result Details |
| :--- | :---: | :--- |
| **Real `cargo-deny` Advisories (Workspace)** | PASS | `errors=0, warnings=0` |
| **Real `cargo-deny` Advisories (`src-tauri`)** | PASS | `errors=0, warnings=0` |
| **Banned Crates (`openssl`, `openssl-sys`)** | PASS | 0 banned crates found |
| **Allowed Crate Registries (`crates.io-index` only)** | PASS | 0 disallowed registry/git sources |
| **Zero VNG Zalo Binary Policy** | PASS | 0 proprietary VNG Zalo binaries/bundles found in repository |
| **MSRV Alignment (`Cargo.toml` vs `src-tauri/Cargo.toml`)** | WARNING (IR-001) | Workspace = `1.85`, Tauri desktop = `1.90` (effective packaging MSRV = `1.90`) |

---

## 4. SPDX License Expression Distribution

| SPDX Expression | Crate Count |
| :--- | :---: |
| `MIT OR Apache-2.0` | 205 |
| `MIT` | 104 |
| `Apache-2.0 OR MIT` | 34 |
| `Unicode-3.0` | 18 |
| `Zlib OR Apache-2.0 OR MIT` | 17 |
| `MIT/Apache-2.0` | 12 |
| `Unlicense OR MIT` | 9 |
| `MPL-2.0` | 7 |
| `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | 5 |
| `Apache-2.0/MIT` | 3 |
| `Apache-2.0` | 2 |
| `BSD-3-Clause` | 2 |
| `BSD-3-Clause OR MIT OR Apache-2.0` | 2 |
| `MIT OR Apache-2.0 OR LGPL-2.1-or-later` | 2 |
| `MIT OR Apache-2.0 OR Zlib` | 2 |
| `MIT OR Zlib OR Apache-2.0` | 2 |
| `Unlicense/MIT` | 2 |
| `Zlib` | 2 |
| `(MIT OR Apache-2.0) AND Unicode-3.0` | 1 |
| `0BSD OR MIT OR Apache-2.0` | 1 |
| `Apache-2.0 / MIT` | 1 |
| `Apache-2.0 AND MIT` | 1 |
| `Apache-2.0 WITH LLVM-exception` | 1 |
| `BSD-3-Clause AND MIT` | 1 |
| `BSD-3-Clause/MIT` | 1 |
| `CC0-1.0 OR MIT-0 OR Apache-2.0` | 1 |
| `ISC` | 1 |

---

## 5. Audit Warnings & Out-of-Scope Integration Notes

- IR-001 WARNING: MSRV mismatch detected — workspace Cargo.toml specifies rust-version = "1.85" whereas apps/desktop/src-tauri/Cargo.toml specifies rust-version = "1.90". Cross-platform desktop builds require Rust >= 1.90.
- DENY_TOML_COMPAT WARNING: desktop-instance-manager/deny.toml contains deprecated top-level `version = 2` (rejected by cargo-deny >= 0.20) and `wildcards = "deny"` against unversioned internal workspace path crates (`publish = false` unset). Remove `version = 2` and add `publish = false` (or `wildcards = "warn"`) in upstream integration patch.
- IR-004 INFO: Repository maintains two separate Cargo lockfiles (desktop-instance-manager/Cargo.lock and apps/desktop/src-tauri/Cargo.lock) because src-tauri declares an independent [workspace]. Both lockfiles were audited with `--locked`.
- Transitive multi-version crates present (41 crate families across lockfiles; matches deny.toml multiple-versions = "warn").
