# Security Policy

## Reporting a Vulnerability (Báo cáo lỗ hổng bảo mật)

**Please do NOT open public GitHub Issues for security vulnerabilities.**

To report a security issue privately, please use **GitHub Private Vulnerability Reporting** in this repository's Security tab:
`https://github.com/manhquydev/zalo/security/advisories/new`

When reporting a vulnerability, **never** include:
- Real Zalo account credentials, QR tokens, OTPs, cookies (`zpw_*`), `Local State` DPAPI keys, SQLite/LevelDB message databases, or phone numbers.
- Unredacted local user home directory paths containing personal usernames.

## Security Boundaries & Guarantees

- **Primary Zalo Protection:** This project isolates secondary profiles strictly in dedicated directories (`%LOCALAPPDATA%\MultiZaloDesktop\Profiles` on Windows, `~/Library/Application Support/MultiZaloDesktop/Profiles` on macOS, `$XDG_DATA_HOME/MultiZaloDesktop/Profiles` on Linux) and blocks any mutation targeting the primary `%APPDATA%\ZaloData` / `~/Library/Application Support/ZaloData` path or primary Zalo processes.
- **No Binary Modification & No Credential Collection:** Multi-Zalo Desktop never unpacks, patches, or redistributes VNG proprietary binaries (`Zalo.exe`, `Zalo.app`, `app.asar`, `ZaloCall`), and never intercepts or stores Zalo login credentials, cookies, or messages.
- **Publisher Verification:** Official Zalo Desktop binaries are verified before launch (Cryptographic Authenticode + `CN/O=VNG GROUP JSC` pinning on Windows; 5-tier code-signature checks on macOS).
- **Local HTTP Bridge:** When running the Signed-Host Bridge (`ui-server.mjs`), the server binds exclusively to `127.0.0.1` and enforces a per-session 256-bit `X-DIM-Session-Token`, `Host`/`Origin`/`Sec-Fetch-Site` checks, and `Content-Type: application/json`.
