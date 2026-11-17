//! macOS code-signature, Developer ID authority, Gatekeeper (`spctl`), and quarantine
//! xattr inspection (`codesign --verify --deep --strict`, `codesign -dv`, `spctl --assess`).

use std::path::Path;

use dim_core::DimError;

use crate::ops::MacOsSystemOps;

/// Classification of a macOS bundle or Mach-O binary's cryptographic trust state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeSignTrustStatus {
    /// Intact signature chained to an Apple Developer ID / Apple Root CA authority.
    TrustedDeveloperId,
    /// Ad-hoc signature (`Signature=adhoc`, no TeamIdentifier/Developer ID chain).
    AdHocSigned,
    /// Binary or bundle has no Mach-O / bundle code signature at all.
    Unsigned,
    /// Sealed resources, Info.plist, or Mach-O pages failed `codesign --verify --deep --strict`.
    TamperedOrInvalid,
    /// Valid Developer ID signature, but signer authority does not match expected publisher hint.
    WrongPublisher,
    /// Host OS is not macOS (`codesign` / `spctl` unavailable without hardware).
    NotVerifiedOnHost,
}

/// Structured output of `codesign` + `spctl` + `xattr` verification on a macOS target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeSignReport {
    /// Overall trust classification.
    pub status: CodeSignTrustStatus,
    /// `Identifier` field from `codesign -dv` (e.g., `com.vng.zalo`).
    pub identifier: Option<String>,
    /// `TeamIdentifier` field from `codesign -dv` (`None` when `"not set"`).
    pub team_identifier: Option<String>,
    /// Ordered `Authority=` certificate chain from leaf to root.
    pub authorities: Vec<String>,
    /// Leaf signer common name (`Authority` index 0), if present.
    pub signer_subject: Option<String>,
    /// True if `Signature=adhoc` was reported.
    pub is_adhoc: bool,
    /// Gatekeeper `spctl --assess --type execute` result, if evaluated.
    pub spctl_accepted: Option<bool>,
    /// Gatekeeper assessment source (e.g., `"Notarized Developer ID"`).
    pub spctl_source: Option<String>,
    /// Informational check whether `com.apple.quarantine` xattr is present (never stripped).
    pub quarantine_present: bool,
    /// Concise status string suitable for `ApplicationInstallation::signature_status`.
    pub summary_label: String,
    /// Diagnostic failure detail when untrusted.
    pub detail: Option<String>,
}

impl CodeSignReport {
    /// Returns `true` only when `status == CodeSignTrustStatus::TrustedDeveloperId`.
    #[must_use]
    pub const fn is_trusted_developer_id(&self) -> bool {
        matches!(self.status, CodeSignTrustStatus::TrustedDeveloperId)
    }
}

/// Verifies a macOS `.app` bundle or Mach-O binary via `/usr/bin/codesign` and `/usr/sbin/spctl`.
///
/// Never modifies binary signatures, entitlements, Gatekeeper policies, or `com.apple.quarantine`.
pub fn verify_macos_code_signature(
    ops: &dyn MacOsSystemOps,
    target_path: &Path,
    expected_publisher_substring: Option<&str>,
) -> CodeSignReport {
    if !ops.is_native_macos() {
        return CodeSignReport {
            status: CodeSignTrustStatus::NotVerifiedOnHost,
            identifier: None,
            team_identifier: None,
            authorities: Vec::new(),
            signer_subject: None,
            is_adhoc: false,
            spctl_accepted: None,
            spctl_source: None,
            quarantine_present: false,
            summary_label: "NotVerifiedOnHost (use codesign -dv on macOS)".to_owned(),
            detail: None,
        };
    }

    let target_str = target_path.to_string_lossy().into_owned();
    let quarantine_present = check_quarantine_xattr(ops, &target_str);

    // 1. Verify cryptographic seal across nested frameworks/helpers (`--deep --strict`).
    let verify_args = vec![
        "--verify".to_owned(),
        "--deep".to_owned(),
        "--strict".to_owned(),
        "--verbose=2".to_owned(),
        target_str.clone(),
    ];
    let verify_out = match ops.run_command(Path::new("/usr/bin/codesign"), &verify_args, &[]) {
        Ok(out) => out,
        Err(err) => {
            return CodeSignReport {
                status: CodeSignTrustStatus::TamperedOrInvalid,
                identifier: None,
                team_identifier: None,
                authorities: Vec::new(),
                signer_subject: None,
                is_adhoc: false,
                spctl_accepted: None,
                spctl_source: None,
                quarantine_present,
                summary_label: "Unverified (codesign invocation error)".to_owned(),
                detail: Some(err.to_string()),
            };
        }
    };

    let verify_combined = verify_out.combined_lossy();
    if !verify_out.success() {
        let lower = verify_combined.to_ascii_lowercase();
        let is_unsigned =
            lower.contains("not signed at all") || lower.contains("code object is not signed");
        let status = if is_unsigned {
            CodeSignTrustStatus::Unsigned
        } else {
            CodeSignTrustStatus::TamperedOrInvalid
        };
        let label = if is_unsigned {
            "Unsigned".to_owned()
        } else {
            "Unverified (TamperedOrInvalidSignature)".to_owned()
        };
        return CodeSignReport {
            status,
            identifier: None,
            team_identifier: None,
            authorities: Vec::new(),
            signer_subject: None,
            is_adhoc: false,
            spctl_accepted: Some(false),
            spctl_source: None,
            quarantine_present,
            summary_label: label,
            detail: Some(first_non_empty_line(&verify_combined)),
        };
    }

    // 2. Extract signing metadata (`Identifier`, `TeamIdentifier`, `Authority`, `Signature=adhoc`).
    let dv_args = vec![
        "-dv".to_owned(),
        "--verbose=4".to_owned(),
        target_str.clone(),
    ];
    let dv_text = ops
        .run_command(Path::new("/usr/bin/codesign"), &dv_args, &[])
        .map(|o| o.combined_lossy())
        .unwrap_or_default();
    let parsed_dv = parse_codesign_dv_output(&dv_text);

    // 3. Gatekeeper assessment (`spctl --assess --type execute --verbose=4`).
    let spctl_args = vec![
        "--assess".to_owned(),
        "--type".to_owned(),
        "execute".to_owned(),
        "--verbose=4".to_owned(),
        target_str,
    ];
    let (spctl_accepted, spctl_source) =
        match ops.run_command(Path::new("/usr/sbin/spctl"), &spctl_args, &[]) {
            Ok(spctl_out) => parse_spctl_output(spctl_out.success(), &spctl_out.combined_lossy()),
            Err(_) => (None, None),
        };

    if parsed_dv.is_adhoc {
        return CodeSignReport {
            status: CodeSignTrustStatus::AdHocSigned,
            identifier: parsed_dv.identifier,
            team_identifier: parsed_dv.team_identifier,
            authorities: parsed_dv.authorities,
            signer_subject: None,
            is_adhoc: true,
            spctl_accepted,
            spctl_source,
            quarantine_present,
            summary_label: "Unverified (AdHocSigned)".to_owned(),
            detail: Some("bundle is ad-hoc signed without Apple Developer ID authority".to_owned()),
        };
    }

    let signer_subject = parsed_dv.authorities.first().cloned();
    if signer_subject.is_none() || parsed_dv.team_identifier.is_none() {
        return CodeSignReport {
            status: CodeSignTrustStatus::TamperedOrInvalid,
            identifier: parsed_dv.identifier,
            team_identifier: parsed_dv.team_identifier,
            authorities: parsed_dv.authorities,
            signer_subject,
            is_adhoc: false,
            spctl_accepted,
            spctl_source,
            quarantine_present,
            summary_label: "Unverified (MissingDeveloperIdAuthority)".to_owned(),
            detail: Some("signature lacks Developer ID Authority or TeamIdentifier".to_owned()),
        };
    }

    if let Some(expected_pub) = expected_publisher_substring {
        let expected_upper = expected_pub.to_ascii_uppercase();
        let matched = parsed_dv
            .authorities
            .iter()
            .any(|a| a.to_ascii_uppercase().contains(&expected_upper));
        if !matched {
            let subj = signer_subject.clone().unwrap_or_default();
            return CodeSignReport {
                status: CodeSignTrustStatus::WrongPublisher,
                identifier: parsed_dv.identifier,
                team_identifier: parsed_dv.team_identifier,
                authorities: parsed_dv.authorities,
                signer_subject,
                is_adhoc: false,
                spctl_accepted,
                spctl_source,
                quarantine_present,
                summary_label: format!("Unverified (WrongPublisher: {subj})"),
                detail: Some(format!(
                    "expected publisher matching '{expected_pub}', found '{subj}'"
                )),
            };
        }
    }

    if spctl_accepted == Some(false) {
        return CodeSignReport {
            status: CodeSignTrustStatus::TamperedOrInvalid,
            identifier: parsed_dv.identifier,
            team_identifier: parsed_dv.team_identifier,
            authorities: parsed_dv.authorities,
            signer_subject,
            is_adhoc: false,
            spctl_accepted,
            spctl_source,
            quarantine_present,
            summary_label: "Unverified (GatekeeperRejected)".to_owned(),
            detail: Some("Gatekeeper `spctl --assess` rejected bundle execution".to_owned()),
        };
    }

    let source_note = spctl_source.as_deref().unwrap_or("Developer ID");
    CodeSignReport {
        status: CodeSignTrustStatus::TrustedDeveloperId,
        identifier: parsed_dv.identifier,
        team_identifier: parsed_dv.team_identifier,
        authorities: parsed_dv.authorities,
        signer_subject,
        is_adhoc: false,
        spctl_accepted,
        spctl_source: spctl_source.clone(),
        quarantine_present,
        summary_label: format!("Valid ({source_note})"),
        detail: None,
    }
}

/// Enforces `DimError::UnverifiedBinary` when `require_signature_trust` is active on macOS.
pub fn enforce_signature_or_err(
    report: &CodeSignReport,
    require_signature_trust: bool,
) -> Result<(), DimError> {
    if !require_signature_trust {
        return Ok(());
    }
    match report.status {
        CodeSignTrustStatus::TrustedDeveloperId | CodeSignTrustStatus::NotVerifiedOnHost => Ok(()),
        CodeSignTrustStatus::Unsigned
        | CodeSignTrustStatus::AdHocSigned
        | CodeSignTrustStatus::TamperedOrInvalid
        | CodeSignTrustStatus::WrongPublisher => Err(DimError::UnverifiedBinary(
            report
                .detail
                .clone()
                .unwrap_or_else(|| report.summary_label.clone()),
        )),
    }
}

/// Parsed fields from `codesign -dv --verbose=4` output.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedCodesignDv {
    /// `Identifier=...` value.
    pub identifier: Option<String>,
    /// `TeamIdentifier=...` value (`None` when `"not set"`).
    pub team_identifier: Option<String>,
    /// Ordered `Authority=...` lines.
    pub authorities: Vec<String>,
    /// True if `Signature=adhoc` or `flags=...adhoc...` appeared.
    pub is_adhoc: bool,
}

/// Pure-Rust parser for `codesign -dv --verbose=4` stderr/stdout text.
#[must_use]
pub fn parse_codesign_dv_output(text: &str) -> ParsedCodesignDv {
    let mut out = ParsedCodesignDv::default();
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if let Some(val) = line.strip_prefix("Identifier=") {
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                out.identifier = Some(trimmed.to_owned());
            }
        } else if let Some(val) = line.strip_prefix("TeamIdentifier=") {
            let trimmed = val.trim();
            if !trimmed.is_empty() && !trimmed.eq_ignore_ascii_case("not set") {
                out.team_identifier = Some(trimmed.to_owned());
            }
        } else if let Some(val) = line.strip_prefix("Authority=") {
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                out.authorities.push(trimmed.to_owned());
            }
        } else if let Some(val) = line.strip_prefix("Signature=") {
            if val.trim().eq_ignore_ascii_case("adhoc") {
                out.is_adhoc = true;
            }
        } else if line.starts_with("CodeDirectory ") && line.contains("(adhoc)") {
            out.is_adhoc = true;
        }
    }
    out
}

/// Pure-Rust parser for `/usr/sbin/spctl --assess --type execute --verbose=4` output.
#[must_use]
pub fn parse_spctl_output(exit_ok: bool, text: &str) -> (Option<bool>, Option<String>) {
    let mut source = None;
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if let Some(src) = line.strip_prefix("source=") {
            let trimmed = src.trim();
            if !trimmed.is_empty() {
                source = Some(trimmed.to_owned());
            }
        }
    }
    let lower = text.to_ascii_lowercase();
    let accepted = if exit_ok || lower.contains(": accepted") {
        Some(true)
    } else if lower.contains(": rejected") || !exit_ok {
        Some(false)
    } else {
        None
    };
    (accepted, source)
}

fn check_quarantine_xattr(ops: &dyn MacOsSystemOps, target_str: &str) -> bool {
    let args = vec![
        "-p".to_owned(),
        "com.apple.quarantine".to_owned(),
        target_str.to_owned(),
    ];
    ops.run_command(Path::new("/usr/bin/xattr"), &args, &[])
        .is_ok_and(|out| out.success() && !out.stdout.is_empty())
}

fn first_non_empty_line(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("codesign verification failed")
        .to_owned()
}
