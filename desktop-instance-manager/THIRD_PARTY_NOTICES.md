# Third-Party Notices & Open-Source Licenses

Multi-Zalo Desktop (`desktop-instance-manager/`) is original software dual-licensed under `MIT OR Apache-2.0` (`../LICENSE-MIT`, `../LICENSE-APACHE`).

> **Non-Affiliation Notice:** Multi-Zalo Desktop is an independent open-source project and is **not** affiliated with, sponsored by, or endorsed by VNG Corporation or Zalo. "Zalo" is a trademark of VNG Corporation. Zero proprietary VNG binaries or assets are included in this repository or in distributed builds.

---

## 1. Rust Crate Dependencies Summary

All Rust dependencies locked in `Cargo.lock`, `apps/desktop/src-tauri/Cargo.lock`, `fixtures/cross-platform/Cargo.lock`, and `tests/cross-platform/Cargo.lock` are verified by `cargo-deny` (`deny.toml`) and contain **zero `GPL` or `AGPL` crates**.

Permissive license families used across the dependency tree include:
- `MIT`
- `Apache-2.0` (and `Apache-2.0 WITH LLVM-exception`)
- `BSD-2-Clause` / `BSD-3-Clause`
- `ISC`
- `Zlib`
- `Unicode-3.0`
- `Unlicense` / `CC0-1.0` / `0BSD`

---

## 2. Weak Copyleft Crates (`MPL-2.0` — Section 3.2(a) Source Availability Notice)

The Tauri 2 desktop binary (`dim-desktop` / `dim-desktop.exe`) statically links the following unmodified **Mozilla Public License 2.0 (`MPL-2.0`)** crates via `tauri` / `tauri-utils`:

| Crate | Version | Source Repository / Crates.io Location |
| :--- | :--- | :--- |
| `cssparser` | `0.37.0`, `0.38.0` | `https://crates.io/crates/cssparser` · `https://github.com/servo/rust-cssparser` |
| `cssparser-macros` | `0.7.1` | `https://crates.io/crates/cssparser-macros` · `https://github.com/servo/rust-cssparser` |
| `dtoa-short` | `0.3.5` | `https://crates.io/crates/dtoa-short` · `https://github.com/servo/dtoa-short` |
| `option-ext` | `0.2.0` | `https://crates.io/crates/option-ext` · `https://github.com/soc/option-ext` |
| `selectors` | `0.38.0`, `0.41.0` | `https://crates.io/crates/selectors` · `https://github.com/servo/stylo` |

Under **MPL-2.0 Sections 1.7, 3.2(a), and 3.3**, unmodified `MPL-2.0` files combined into a Larger Work remain governed by `MPL-2.0` (full license text: `https://www.mozilla.org/en-US/MPL/2.0/`), and their Source Code Form is publicly available at the upstream crates.io and GitHub locations listed above.

---

## 3. Linux System Libraries & AppImage Notice (`LGPL-2.0+` / `LGPL-2.1+` / `LGPL-3.0`)

On Ubuntu/Linux, `dim-desktop` dynamically links against standard GTK/WebKitGTK shared libraries provided by the host distribution:
- `libwebkit2gtk-4.1-0` (`LGPL-2.1+` / `BSD-2-Clause`)
- `libgtk-3-0t64` (`LGPL-2.0+`)
- `libglib2.0-0t64` (`LGPL-2.1+`)
- `libsoup-3.0-0` (`LGPL-2.0+`)
- `librsvg2-2` (`LGPL-2.1+`)
- `libayatana-appindicator3-1` (`LGPL-2.1` / `LGPL-3.0`)

For `.deb` installations, these libraries remain external OS-managed shared libraries (`Depends:` in package metadata).
For portable `.AppImage` bundles built on Ubuntu 24.04 (`Experimental`), users may extract and replace dynamically linked shared libraries via `--appimage-extract`, and the corresponding Ubuntu upstream source packages are available via standard `apt-get source` for the Ubuntu 24.04 LTS (`noble`) release (`https://packages.ubuntu.com/noble/`).
