# Cross-Platform Dependency, License & Policy Audit Summary

- **Generated (UTC):** `2026-11-17T16:44:08.076Z`
- **Policy File:** `desktop-instance-manager/deny.toml`
- **Overall Policy Verdict:** **PASS (with MSRV alignment warning IR-001)**

---

## 1. Lockfile & Toolchain Scope

| Lockfile | Format | SHA-256 | Total Packages | Notes |
| :--- | :---: | :--- | :---: | :--- |
| `desktop-instance-manager/Cargo.lock` | v4 | `deeb58a04271c4146491f8d0385618838ef64f1cab1194b6f88888e3071040d2` | 41 | Core Rust workspace (`dim-core`, `dim-profile-store`, `dim-process-manager`, OS adapters, `dim-cli`) |
| `desktop-instance-manager/apps/desktop/src-tauri/Cargo.lock` | v4 | `0bcb1b65cc7e53d142c148d306f6e6dfe24a4fce323e18f99c9f6dae5213e1fc` | 434 | Tauri v2 desktop shell (`tauri@2.12.6`, `tauri-build@2.7.5`) |
| **Combined Unique Packages** | — | — | **439** | **431** external `crates.io-index` crates + local workspace crates |

---

## 2. Security, Registry, License & Binary Policy Checks

| Check | Status | Result Details |
| :--- | :---: | :--- |
| **Banned Crates (`openssl`, `openssl-sys`)** | PASS | 0 banned crates found |
| **Allowed Crate Registries (`crates.io-index` only)** | PASS | 0 disallowed registry/git sources |
| **Registry SHA-256 Checksum Integrity** | PASS | 431 / 431 registry crates carry valid 64-hex SHA-256 checksums |
| **SPDX License Allowlist (`deny.toml`)** | PASS | Mode: `CARGO_METADATA_LOCKED` (439 unique crates evaluated, 0 violations) |
| **Zero VNG Zalo Binary Policy** | PASS | 0 proprietary VNG Zalo binaries/bundles found in repository |
| **MSRV Alignment (`Cargo.toml` vs `src-tauri/Cargo.toml`)** | WARNING (IR-001) | Workspace = `1.85`, Tauri desktop = `1.90` (effective packaging MSRV = `1.90`; highest transitive crate MSRV = `1.90` via `dim-desktop@0.1.0`) |

---

## 3. SPDX License Expression Distribution

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

## 4. Audit Warnings & Out-of-Scope Integration Notes

- IR-001 WARNING: MSRV mismatch detected — workspace Cargo.toml specifies rust-version = "1.85" whereas apps/desktop/src-tauri/Cargo.toml specifies rust-version = "1.90". Cross-platform desktop builds require Rust >= 1.90.
- IR-004 INFO: Repository maintains two separate Cargo lockfiles (desktop-instance-manager/Cargo.lock and apps/desktop/src-tauri/Cargo.lock) because src-tauri declares an independent [workspace]. Both lockfiles were audited.
- Transitive multi-version crates present (41 crate families across lockfiles; matches deny.toml multiple-versions = "warn").
