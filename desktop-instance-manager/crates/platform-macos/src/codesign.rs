//! 5-tier macOS cryptographic application identity verification (`codesign`, nested helpers,
//! `spctl` Gatekeeper assessment, read-only `com.apple.quarantine` xattr check, and Apple
//! Designated Requirement `-R=...` publisher pinning).

use std::path::{Path, PathBuf};

use dim_core::DimError;

use crate::bundle::{AppIdentityVerificationTier, ValidatedMacOsBundle};
use crate::ops::MacOsSystemOps;

/// Cryptographic publisher pinning policy enforcing Tier 5 (`VerifiedPublisherIdentity`).
///
/// Never inferred from filenames (`Zalo.app`), self-declared `CFBundleIdentifier`, or loose `"VNG"`
/// substring matches. Must be configured with a verified Apple Developer `TeamIdentifier` (e.g.
/// 10-character Apple Team ID).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinnedPublisherPolicy {
    /// Expected `CFBundleIdentifier` / `codesign` `Identifier` (e.g. `"com.vng.zalo"`).
    pub bundle_identifier: String,
    /// Expected Apple Developer `TeamIdentifier` (`certificate leaf[subject.OU]`).
    pub team_identifier: String,
}

impl PinnedPublisherPolicy {
    /// Constructs a new pinned publisher policy.
    #[must_use]
    pub fn new(bundle_identifier: impl Into<String>, team_identifier: impl Into<String>) -> Self {
        Self {
            bundle_identifier: bundle_identifier.into(),
            team_identifier: team_identifier.into(),
        }
    }

    /// Builds an injection-safe Apple Designated Requirement string for `codesign -R=...`.
    pub fn designated_requirement(&self) -> Result<String, DimError> {
        if !is_safe_requirement_token(&self.bundle_identifier)
            || !is_safe_requirement_token(&self.team_identifier)
        {
            return Err(DimError::UnverifiedBinary(
                "invalid characters in pinned bundle_identifier or team_identifier".to_owned(),
            ));
        }
        Ok(format!(
            "identifier \"{}\" and anchor apple generic and certificate 1[field.1.2.840.113635.100.6.2.6] and certificate leaf[field.1.2.840.113635.100.6.1.13] and certificate leaf[subject.OU] = \"{}\"",
            self.bundle_identifier, self.team_identifier
        ))
    }
}

/// Classification of a macOS bundle or Mach-O binary's cryptographic trust state across Tiers 3–5.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeSignTrustStatus {
    /// Tier 5: Intact signature, Gatekeeper accepted, AND Apple Designated Requirement + pinned `TeamIdentifier` verified.
    VerifiedPublisher,
    /// Signed (Tier 3/4), or lacking `TeamIdentifier`, when no [`PinnedPublisherPolicy`] was configured or metadata is unverifiable.
    UnverifiedPublisherMetadata,
    /// Valid code signature, but Gatekeeper `spctl --assess --type execute` rejected execution.
    GatekeeperRejected,
    /// Ad-hoc signature (`Signature=adhoc`, no Developer ID authority).
    AdHocSigned,
    /// Binary or bundle has no Mach-O / bundle code signature at all.
    Unsigned,
    /// Sealed resources, nested helper, Info.plist, or Mach-O pages were modified after signing or failed `--verify --deep --strict`.
    TamperedSignature,
    /// Signature is intact, but `TeamIdentifier`, `Identifier`, or `-R=<designated_requirement>` mismatched [`PinnedPublisherPolicy`].
    WrongPublisher,
    /// Host OS is not macOS (`codesign` / `spctl` unavailable without Darwin hardware).
    NotVerifiedOnHost,
}

impl CodeSignTrustStatus {
    /// Backwards-compatible alias for [`Self::TamperedSignature`].
    pub const TAMPERED_OR_INVALID: Self = Self::TamperedSignature;
}

/// Structured 5-tier output of `codesign` + nested helper checks + `spctl` + `xattr` + `-R=...` verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeSignReport {
    /// Overall cryptographic trust classification.
    pub status: CodeSignTrustStatus,
    /// Highest [`AppIdentityVerificationTier`] actually proven by OS/cryptographic checks.
    pub highest_tier: Option<AppIdentityVerificationTier>,
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
    /// Number of nested helper bundles/executables individually verified via `codesign`.
    pub verified_nested_helpers: usize,
    /// Concise status string suitable for `ApplicationInstallation::signature_status`.
    pub summary_label: String,
    /// Diagnostic failure detail when below [`CodeSignTrustStatus::VerifiedPublisher`].
    pub detail: Option<String>,
}

impl CodeSignReport {
    /// Returns `true` ONLY when Tier 5 (`VerifiedPublisherIdentity`) has been cryptographically proven.
    #[must_use]
    pub const fn is_verified_publisher(&self) -> bool {
        matches!(self.status, CodeSignTrustStatus::VerifiedPublisher)
    }
}

/// Verifies a validated macOS `.app` bundle across Tiers 3–5 (`ValidCodeSignature`, `GatekeeperAssessed`,
/// and `VerifiedPublisherIdentity`), including nested helper bundles in `Contents/Frameworks/`.
pub fn verify_macos_bundle_identity(
    ops: &dyn MacOsSystemOps,
    bundle: &ValidatedMacOsBundle,
    pinned_policy: Option<&PinnedPublisherPolicy>,
) -> CodeSignReport {
    verify_macos_path_with_nested(
        ops,
        &bundle.bundle_path,
        &bundle.nested_signable_paths,
        bundle.structural_tier,
        pinned_policy,
    )
}

/// Verifies a standalone Mach-O binary or `.app` path via `codesign` + `spctl` + optional [`PinnedPublisherPolicy`].
///
/// Never modifies binary signatures, entitlements, Gatekeeper policies, or `com.apple.quarantine`.
pub fn verify_macos_code_signature(
    ops: &dyn MacOsSystemOps,
    target_path: &Path,
    pinned_policy: Option<&PinnedPublisherPolicy>,
) -> CodeSignReport {
    verify_macos_path_with_nested(
        ops,
        target_path,
        &[],
        Some(AppIdentityVerificationTier::ValidMachO),
        pinned_policy,
    )
}

fn verify_macos_path_with_nested(
    ops: &dyn MacOsSystemOps,
    target_path: &Path,
    nested_paths: &[PathBuf],
    structural_tier: Option<AppIdentityVerificationTier>,
    pinned_policy: Option<&PinnedPublisherPolicy>,
) -> CodeSignReport {
    if !ops.is_native_macos() {
        let tier_label = structural_tier.map_or("NONE", AppIdentityVerificationTier::as_str);
        return CodeSignReport {
            status: CodeSignTrustStatus::NotVerifiedOnHost,
            highest_tier: structural_tier,
            identifier: None,
            team_identifier: None,
            authorities: Vec::new(),
            signer_subject: None,
            is_adhoc: false,
            spctl_accepted: None,
            spctl_source: None,
            quarantine_present: false,
            verified_nested_helpers: 0,
            summary_label: format!(
                "NotVerifiedOnHost (highest_tier={tier_label}; codesign/spctl require macOS)"
            ),
            detail: None,
        };
    }

    let target_str = target_path.to_string_lossy().into_owned();
    let quarantine_present = check_quarantine_xattr(ops, &target_str);

    // 1. Verify cryptographic seal on root bundle/executable (`--verify --deep --strict`).
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
                status: CodeSignTrustStatus::TamperedSignature,
                highest_tier: structural_tier,
                identifier: None,
                team_identifier: None,
                authorities: Vec::new(),
                signer_subject: None,
                is_adhoc: false,
                spctl_accepted: None,
                spctl_source: None,
                quarantine_present,
                verified_nested_helpers: 0,
                summary_label: "Unverified (codesign invocation error)".to_owned(),
                detail: Some(err.to_string()),
            };
        }
    };

    let verify_combined = verify_out.combined_lossy();
    if !verify_out.success() {
        let (status, label) = classify_codesign_verify_failure(&verify_combined);
        return CodeSignReport {
            status,
            highest_tier: structural_tier,
            identifier: None,
            team_identifier: None,
            authorities: Vec::new(),
            signer_subject: None,
            is_adhoc: false,
            spctl_accepted: Some(false),
            spctl_source: None,
            quarantine_present,
            verified_nested_helpers: 0,
            summary_label: label,
            detail: Some(first_non_empty_line(&verify_combined)),
        };
    }

    // 2. Verify every nested helper bundle/executable in `Contents/Frameworks/` & `ZaloHelper.app`.
    let mut verified_nested = 0usize;
    for nested in nested_paths {
        let nested_args = vec![
            "--verify".to_owned(),
            "--deep".to_owned(),
            "--strict".to_owned(),
            "--verbose=2".to_owned(),
            nested.to_string_lossy().into_owned(),
        ];
        match ops.run_command(Path::new("/usr/bin/codesign"), &nested_args, &[]) {
            Ok(out) if out.success() => {
                verified_nested += 1;
            }
            Ok(out) => {
                let msg = first_non_empty_line(&out.combined_lossy());
                return CodeSignReport {
                    status: CodeSignTrustStatus::TamperedSignature,
                    highest_tier: structural_tier,
                    identifier: None,
                    team_identifier: None,
                    authorities: Vec::new(),
                    signer_subject: None,
                    is_adhoc: false,
                    spctl_accepted: Some(false),
                    spctl_source: None,
                    quarantine_present,
                    verified_nested_helpers: verified_nested,
                    summary_label: "Unverified (TamperedNestedHelper)".to_owned(),
                    detail: Some(format!(
                        "nested helper '{}' failed codesign verification: {msg}",
                        nested.display()
                    )),
                };
            }
            Err(err) => {
                return CodeSignReport {
                    status: CodeSignTrustStatus::TamperedSignature,
                    highest_tier: structural_tier,
                    identifier: None,
                    team_identifier: None,
                    authorities: Vec::new(),
                    signer_subject: None,
                    is_adhoc: false,
                    spctl_accepted: None,
                    spctl_source: None,
                    quarantine_present,
                    verified_nested_helpers: verified_nested,
                    summary_label: "Unverified (NestedCodesignError)".to_owned(),
                    detail: Some(err.to_string()),
                };
            }
        }
    }

    // 3. Extract signing metadata (`Identifier`, `TeamIdentifier`, `Authority`, `Signature=adhoc`).
    let dv_args = vec![
        "-dv".to_owned(),
        "--verbose=4".to_owned(),
        target_str.clone(),
    ];
    let dv_out = match ops.run_command(Path::new("/usr/bin/codesign"), &dv_args, &[]) {
        Ok(o) if o.success() => o.combined_lossy(),
        Ok(o) => {
            return CodeSignReport {
                status: CodeSignTrustStatus::UnverifiedPublisherMetadata,
                highest_tier: structural_tier,
                identifier: None,
                team_identifier: None,
                authorities: Vec::new(),
                signer_subject: None,
                is_adhoc: false,
                spctl_accepted: None,
                spctl_source: None,
                quarantine_present,
                verified_nested_helpers: verified_nested,
                summary_label: "Unverified (UnreadableSignatureMetadata)".to_owned(),
                detail: Some(first_non_empty_line(&o.combined_lossy())),
            };
        }
        Err(err) => {
            return CodeSignReport {
                status: CodeSignTrustStatus::UnverifiedPublisherMetadata,
                highest_tier: structural_tier,
                identifier: None,
                team_identifier: None,
                authorities: Vec::new(),
                signer_subject: None,
                is_adhoc: false,
                spctl_accepted: None,
                spctl_source: None,
                quarantine_present,
                verified_nested_helpers: verified_nested,
                summary_label: "Unverified (SignatureMetadataError)".to_owned(),
                detail: Some(err.to_string()),
            };
        }
    };
    let parsed_dv = parse_codesign_dv_output(&dv_out);

    if parsed_dv.is_adhoc {
        return CodeSignReport {
            status: CodeSignTrustStatus::AdHocSigned,
            highest_tier: structural_tier,
            identifier: parsed_dv.identifier,
            team_identifier: parsed_dv.team_identifier,
            authorities: parsed_dv.authorities,
            signer_subject: None,
            is_adhoc: true,
            spctl_accepted: None,
            spctl_source: None,
            quarantine_present,
            verified_nested_helpers: verified_nested,
            summary_label: "Unverified (AdHocSigned)".to_owned(),
            detail: Some("bundle is ad-hoc signed without Apple Developer ID authority".to_owned()),
        };
    }

    let signer_subject = parsed_dv.authorities.first().cloned();
    if signer_subject.is_none() || parsed_dv.team_identifier.is_none() {
        return CodeSignReport {
            status: CodeSignTrustStatus::UnverifiedPublisherMetadata,
            highest_tier: structural_tier,
            identifier: parsed_dv.identifier,
            team_identifier: parsed_dv.team_identifier,
            authorities: parsed_dv.authorities,
            signer_subject,
            is_adhoc: false,
            spctl_accepted: None,
            spctl_source: None,
            quarantine_present,
            verified_nested_helpers: verified_nested,
            summary_label: "Unverified (MissingTeamIdentifierOrAuthority)".to_owned(),
            detail: Some(
                "signature metadata lacks verifiable Apple Developer ID Authority or TeamIdentifier"
                    .to_owned(),
            ),
        };
    }

    // Tier 3 (`ValidCodeSignature`) reached!
    let mut current_tier = Some(AppIdentityVerificationTier::ValidCodeSignature);

    // 4. Gatekeeper assessment (`spctl --assess --type execute --verbose=4`).
    let spctl_args = vec![
        "--assess".to_owned(),
        "--type".to_owned(),
        "execute".to_owned(),
        "--verbose=4".to_owned(),
        target_str.clone(),
    ];
    let (spctl_accepted, spctl_source) =
        match ops.run_command(Path::new("/usr/sbin/spctl"), &spctl_args, &[]) {
            Ok(spctl_out) => parse_spctl_output(spctl_out.success(), &spctl_out.combined_lossy()),
            Err(_) => (None, None),
        };

    if spctl_accepted != Some(true) {
        return CodeSignReport {
            status: CodeSignTrustStatus::GatekeeperRejected,
            highest_tier: current_tier,
            identifier: parsed_dv.identifier,
            team_identifier: parsed_dv.team_identifier,
            authorities: parsed_dv.authorities,
            signer_subject,
            is_adhoc: false,
            spctl_accepted,
            spctl_source,
            quarantine_present,
            verified_nested_helpers: verified_nested,
            summary_label: "Unverified (GatekeeperRejected)".to_owned(),
            detail: Some(
                "Gatekeeper `spctl --assess --type execute` did not accept bundle execution"
                    .to_owned(),
            ),
        };
    }

    // Tier 4 (`GatekeeperAssessed`) reached!
    current_tier = Some(AppIdentityVerificationTier::GatekeeperAssessed);

    // 5. Tier 5 (`VerifiedPublisherIdentity`): require explicit pinned TeamIdentifier + `-R=...` check!
    let Some(policy) = pinned_policy else {
        return CodeSignReport {
            status: CodeSignTrustStatus::UnverifiedPublisherMetadata,
            highest_tier: current_tier,
            identifier: parsed_dv.identifier,
            team_identifier: parsed_dv.team_identifier.clone(),
            authorities: parsed_dv.authorities,
            signer_subject,
            is_adhoc: false,
            spctl_accepted,
            spctl_source,
            quarantine_present,
            verified_nested_helpers: verified_nested,
            summary_label: format!(
                "UnverifiedPublisher (GatekeeperAssessed; TeamID={} unpinned)",
                parsed_dv.team_identifier.as_deref().unwrap_or("unknown")
            ),
            detail: Some(
                "publisher TeamIdentifier is not pinned in PinnedPublisherPolicy; refusing to infer publisher trust from app name or bundle ID"
                    .to_owned(),
            ),
        };
    };

    if parsed_dv.identifier.as_deref() != Some(policy.bundle_identifier.as_str())
        || parsed_dv.team_identifier.as_deref() != Some(policy.team_identifier.as_str())
    {
        let found_id = parsed_dv.identifier.clone().unwrap_or_default();
        let found_team = parsed_dv.team_identifier.clone().unwrap_or_default();
        return CodeSignReport {
            status: CodeSignTrustStatus::WrongPublisher,
            highest_tier: current_tier,
            identifier: parsed_dv.identifier,
            team_identifier: parsed_dv.team_identifier,
            authorities: parsed_dv.authorities,
            signer_subject,
            is_adhoc: false,
            spctl_accepted,
            spctl_source,
            quarantine_present,
            verified_nested_helpers: verified_nested,
            summary_label: format!(
                "Unverified (WrongPublisher: id='{found_id}', team='{found_team}')"
            ),
            detail: Some(format!(
                "expected pinned bundle_id='{}' and team_id='{}', found id='{found_id}' and team='{found_team}'",
                policy.bundle_identifier, policy.team_identifier
            )),
        };
    }

    let req_expr = match policy.designated_requirement() {
        Ok(r) => r,
        Err(err) => {
            return CodeSignReport {
                status: CodeSignTrustStatus::UnverifiedPublisherMetadata,
                highest_tier: current_tier,
                identifier: parsed_dv.identifier,
                team_identifier: parsed_dv.team_identifier,
                authorities: parsed_dv.authorities,
                signer_subject,
                is_adhoc: false,
                spctl_accepted,
                spctl_source,
                quarantine_present,
                verified_nested_helpers: verified_nested,
                summary_label: "Unverified (InvalidPinnedRequirement)".to_owned(),
                detail: Some(err.to_string()),
            };
        }
    };

    let req_args = vec![
        "--verify".to_owned(),
        "--deep".to_owned(),
        "--strict".to_owned(),
        format!("-R={req_expr}"),
        target_str,
    ];
    match ops.run_command(Path::new("/usr/bin/codesign"), &req_args, &[]) {
        Ok(req_out) if req_out.success() => {}
        Ok(req_out) => {
            return CodeSignReport {
                status: CodeSignTrustStatus::WrongPublisher,
                highest_tier: current_tier,
                identifier: parsed_dv.identifier,
                team_identifier: parsed_dv.team_identifier,
                authorities: parsed_dv.authorities,
                signer_subject,
                is_adhoc: false,
                spctl_accepted,
                spctl_source,
                quarantine_present,
                verified_nested_helpers: verified_nested,
                summary_label: "Unverified (DesignatedRequirementFailed)".to_owned(),
                detail: Some(first_non_empty_line(&req_out.combined_lossy())),
            };
        }
        Err(err) => {
            return CodeSignReport {
                status: CodeSignTrustStatus::UnverifiedPublisherMetadata,
                highest_tier: current_tier,
                identifier: parsed_dv.identifier,
                team_identifier: parsed_dv.team_identifier,
                authorities: parsed_dv.authorities,
                signer_subject,
                is_adhoc: false,
                spctl_accepted,
                spctl_source,
                quarantine_present,
                verified_nested_helpers: verified_nested,
                summary_label: "Unverified (DesignatedRequirementError)".to_owned(),
                detail: Some(err.to_string()),
            };
        }
    }

    let source_note = spctl_source.as_deref().unwrap_or("Notarized Developer ID");
    CodeSignReport {
        status: CodeSignTrustStatus::VerifiedPublisher,
        highest_tier: Some(AppIdentityVerificationTier::VerifiedPublisherIdentity),
        identifier: parsed_dv.identifier,
        team_identifier: parsed_dv.team_identifier.clone(),
        authorities: parsed_dv.authorities,
        signer_subject,
        is_adhoc: false,
        spctl_accepted,
        spctl_source: spctl_source.clone(),
        quarantine_present,
        verified_nested_helpers: verified_nested,
        summary_label: format!(
            "VerifiedPublisher (TeamID={}, {source_note})",
            policy.team_identifier
        ),
        detail: None,
    }
}

/// Enforces fail-closed `DimError::UnverifiedBinary` when `require_signature_trust` is active on macOS.
///
/// For `OfficialZalo` (`require_pinned_publisher == true`), requires full Tier 5 (`VerifiedPublisher`).
/// For user-supplied `CustomExecutable` (`require_pinned_publisher == false`), requires at least Tier 4
/// (`GatekeeperAssessed` with non-adhoc Developer ID signature).
pub fn enforce_signature_or_err(
    report: &CodeSignReport,
    require_signature_trust: bool,
) -> Result<(), DimError> {
    enforce_signature_tier_or_err(report, require_signature_trust, true)
}

/// Tier-aware variant of [`enforce_signature_or_err`] distinguishing `OfficialZalo` (requires Tier 5
/// pinned publisher) from `CustomExecutable` (requires Tier 4 signed + Gatekeeper-accepted binary).
pub fn enforce_signature_tier_or_err(
    report: &CodeSignReport,
    require_signature_trust: bool,
    require_pinned_publisher: bool,
) -> Result<(), DimError> {
    if !require_signature_trust {
        return Ok(());
    }
    match report.status {
        CodeSignTrustStatus::VerifiedPublisher | CodeSignTrustStatus::NotVerifiedOnHost => Ok(()),
        CodeSignTrustStatus::UnverifiedPublisherMetadata
            if !require_pinned_publisher
                && report.highest_tier >= Some(AppIdentityVerificationTier::GatekeeperAssessed) =>
        {
            Ok(())
        }
        CodeSignTrustStatus::UnverifiedPublisherMetadata
        | CodeSignTrustStatus::GatekeeperRejected
        | CodeSignTrustStatus::Unsigned
        | CodeSignTrustStatus::AdHocSigned
        | CodeSignTrustStatus::TamperedSignature
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
    let accepted = if exit_ok && !lower.contains(": rejected") {
        Some(true)
    } else if lower.contains(": rejected") || !exit_ok {
        Some(false)
    } else {
        None
    };
    (accepted, source)
}

fn classify_codesign_verify_failure(combined: &str) -> (CodeSignTrustStatus, String) {
    let lower = combined.to_ascii_lowercase();
    if lower.contains("not signed at all") || lower.contains("code object is not signed") {
        return (CodeSignTrustStatus::Unsigned, "Unsigned".to_owned());
    }
    if lower.contains("sealed resource is missing or invalid")
        || lower.contains("file modified")
        || lower.contains("file added")
        || lower.contains("modified")
    {
        return (
            CodeSignTrustStatus::TamperedSignature,
            "Unverified (TamperedBundleResource)".to_owned(),
        );
    }
    (
        CodeSignTrustStatus::TamperedSignature,
        "Unverified (InvalidCodeSignature)".to_owned(),
    )
}

fn is_safe_requirement_token(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
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
