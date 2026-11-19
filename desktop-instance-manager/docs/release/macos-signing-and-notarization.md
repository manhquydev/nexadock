# macOS Developer ID Signing & Notarization Setup

NexaDock's release workflow (`.github/workflows/release.yml`) is **fail-closed** for macOS production releases: it will not publish an unsigned or unnotarized `.dmg` as a production macOS release.

## Required GitHub Actions Secrets

Configure the following **6 repository secrets** in **Settings → Secrets and variables → Actions**:

| Secret Name | Description | How to Obtain |
| :--- | :--- | :--- |
| `APPLE_CERTIFICATE` | Base64-encoded `.p12` containing your **Developer ID Application** certificate and private key | Export from Keychain Access as `.p12`, then run `base64 -i DeveloperID.p12 \| tr -d '\n'` |
| `APPLE_CERTIFICATE_PASSWORD` | Password used when exporting the `.p12` | Set during `.p12` export |
| `APPLE_SIGNING_IDENTITY` | Exact certificate common name | e.g. `Developer ID Application: Your Name (TEAMID1234)` (`security find-identity -v -p codesigning`) |
| `APPLE_API_KEY` | App Store Connect API **Key ID** | App Store Connect → Users and Access → Integrations → Team Keys |
| `APPLE_API_ISSUER` | App Store Connect **Issuer ID** (UUID) | Shown at the top of the Team Keys page |
| `APPLE_API_PRIVATE_KEY` | Contents of the downloaded `AuthKey_<KEY_ID>.p8` file (or base64 of it) | Downloaded once when creating the App Store Connect API Key |

## What the Release Pipeline Does Automatically

When all 6 secrets are configured and a release tag (e.g. `v0.1.2`) is pushed:
1. `tauri build --bundles dmg -- --locked` signs `NexaDock.app` with Hardened Runtime (`Entitlements.plist`) and your Developer ID Application certificate.
2. `xcrun notarytool submit` + `xcrun stapler staple` notarizes and staples both `NexaDock.app` and the final `.dmg`.
3. `desktop-instance-manager/scripts/ci-cross-platform/macos-release-acceptance.mjs --mode production` verifies:
   - `hdiutil verify` & clean-install `ditto` copy
   - `plutil -lint Info.plist`, `lipo -archs` (`arm64`), `otool -L` (system dylibs only)
   - `codesign --verify --deep --strict`, `TeamIdentifier`, Hardened Runtime (`flags=runtime`)
   - `xcrun stapler validate` on both `.dmg` and `.app`
   - `spctl --assess --type execute` with `com.apple.quarantine` set
   - Clean-install launch survival (5s), `SIGTERM` clean exit, and 3s relaunch
4. `publish-release` generates `SHA256SUMS.txt` and `PROVENANCE.txt` (`macos_arm64_signing_status=SIGNED_AND_NOTARIZED`) and attaches the verified `.dmg` and `macos-arm64-acceptance.json` to the GitHub Release.
