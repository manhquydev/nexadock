//! Compatible Linux desktop application discovery, ELF/AppImage format verification,
//! SHA-256 provenance checks, startup-script auditing, and Ubuntu dependency diagnostics.

use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use dim_core::{validate_directory_input, DimError};

use crate::status::SecurityBlockReason;

/// Recognized executable or bundle container format on Linux.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryFormatKind {
    /// Standard AppImage Type-2 ELF64 carrying `"AI\x02"` (`0x41, 0x49, 0x02`) at offset `8`.
    AppImageType2,
    /// `quick-sharun` / `uruntime` DwarFS self-mounting ELF64 AppImage (used by `zalo-for-linux` stage 2).
    AppImageUruntimeDwarfs,
    /// Standard ELF64 native executable (e.g., extracted Electron binary or compiled `dim-test-fixture`).
    Elf64Executable,
    /// Interpreted `#!` wrapper or `AppRun` shell script.
    ShellOrScriptWrapper,
    /// Extracted AppDir directory containing `AppRun` or `zalo` executable.
    ExtractedAppDir,
    /// Unrecognized or unsupported file format.
    Unknown,
}

impl BinaryFormatKind {
    /// Machine-readable format label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AppImageType2 => "appimage-type2-elf64",
            Self::AppImageUruntimeDwarfs => "appimage-uruntime-dwarfs-elf64",
            Self::Elf64Executable => "elf64-executable",
            Self::ShellOrScriptWrapper => "script-wrapper",
            Self::ExtractedAppDir => "extracted-appdir",
            Self::Unknown => "unknown",
        }
    }
}

/// Target CPU architecture parsed from an ELF64 `e_machine` header field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElfCpuArch {
    /// AMD64 / Intel 64 (`EM_X86_64 = 0x3E`).
    X86_64,
    /// ARM 64-bit (`EM_AARCH64 = 0xB7`).
    Aarch64,
    /// Other or non-ELF architecture.
    Other(u16),
}

impl ElfCpuArch {
    /// Canonical Linux `uname -m` style architecture identifier.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::X86_64 => "x86_64",
            Self::Aarch64 => "aarch64",
            Self::Other(_) => "other",
        }
    }

    /// Checks whether this ELF binary matches the compiled host CPU architecture.
    #[must_use]
    pub const fn matches_current_arch(self) -> bool {
        if cfg!(target_arch = "x86_64") {
            matches!(self, Self::X86_64)
        } else if cfg!(target_arch = "aarch64") {
            matches!(self, Self::Aarch64)
        } else {
            true
        }
    }
}

/// Reference entry for published `VN-Linux-Family/zalo-for-linux` community AppImage digests.
///
/// Matching an entry records provenance transparency ONLY; it never implies official VNG signing,
/// endorsement, or redistribution rights.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommunityReleaseReference {
    /// Upstream Zalo macOS base version tag (e.g., `"26.11.10"`).
    pub version: &'static str,
    /// Variant (`"original"` or `"zadark"`).
    pub variant: &'static str,
    /// Target CPU architecture (`"x86_64"` or `"aarch64"`).
    pub arch: &'static str,
    /// Published asset filename on GitHub Releases.
    pub asset_name: &'static str,
    /// Published lowercase hex SHA-256 digest.
    pub sha256: &'static str,
}

/// Pinned SHA-256 reference catalog observed from `VN-Linux-Family/zalo-for-linux` releases (`bbb6f99`).
pub const KNOWN_COMMUNITY_RELEASES: &[CommunityReleaseReference] = &[
    CommunityReleaseReference {
        version: "26.11.10",
        variant: "original",
        arch: "x86_64",
        asset_name: "Zalo-26.11.10-Original-e080c70-x86_64.AppImage",
        sha256: "ea150bcb290838959075d9d8948ac4bbab386ac885a99a293ce4e614c68d96ed",
    },
    CommunityReleaseReference {
        version: "26.11.10",
        variant: "zadark",
        arch: "x86_64",
        asset_name: "Zalo-26.11.10+ZaDark-26.3-e080c70-x86_64.AppImage",
        sha256: "565f328b64085e3b61d355a0ee38b6437e3cd62c7a4ca06630a30ec7a712fa4a",
    },
    CommunityReleaseReference {
        version: "26.11.10",
        variant: "original",
        arch: "aarch64",
        asset_name: "Zalo-26.11.10-Original-e080c70-aarch64.AppImage",
        sha256: "1df516bc6ba536588c1d6215a73c98adab6293977261e81a816d9977b0b953b5",
    },
    CommunityReleaseReference {
        version: "26.11.10",
        variant: "zadark",
        arch: "aarch64",
        asset_name: "Zalo-26.11.10+ZaDark-26.3-e080c70-aarch64.AppImage",
        sha256: "4800ea36e420ddb1ee93e8f4419697edbd486147657aa139b525a411a2bd4704",
    },
    CommunityReleaseReference {
        version: "26.10.20",
        variant: "original",
        arch: "x86_64",
        asset_name: "Zalo-26.10.20-Original-20a1fe7-x86_64.AppImage",
        sha256: "ec970dcf74044dee8e4b5eda39e0b7d0aed7900238a1022089f41596ccf4a7ba",
    },
    CommunityReleaseReference {
        version: "26.10.20",
        variant: "zadark",
        arch: "x86_64",
        asset_name: "Zalo-26.10.20+ZaDark-26.3-20a1fe7-x86_64.AppImage",
        sha256: "b225cfb36d145e8c55bd64189b9e92ce8fc4faa9f08c43306504db718281fd25",
    },
    CommunityReleaseReference {
        version: "26.10.20",
        variant: "original",
        arch: "aarch64",
        asset_name: "Zalo-26.10.20-Original-20a1fe7-aarch64.AppImage",
        sha256: "b49709d216cf96b3fc17d790c7930b992a2473a502203de51e5fc4f7d6e1a001",
    },
    CommunityReleaseReference {
        version: "26.10.20",
        variant: "zadark",
        arch: "aarch64",
        asset_name: "Zalo-26.10.20+ZaDark-26.3-20a1fe7-aarch64.AppImage",
        sha256: "ab5dd27d87e20a197bcd84363bd8c33f260cfabcd0c48162c137b46d434d8802",
    },
];

/// Looks up a lowercase SHA-256 hex digest in [`KNOWN_COMMUNITY_RELEASES`].
#[must_use]
pub fn match_known_community_release(
    sha256_hex: &str,
) -> Option<&'static CommunityReleaseReference> {
    let clean = sha256_hex.trim();
    KNOWN_COMMUNITY_RELEASES
        .iter()
        .find(|entry| entry.sha256.eq_ignore_ascii_case(clean))
}

/// Static audit result for a shell wrapper or `AppRun` startup script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartupScriptAudit {
    /// Interpreter parsed from `#!` shebang line, if present.
    pub interpreter: Option<String>,
    /// Whether the script is safe to execute under unprivileged profile isolation.
    pub safe: bool,
    /// Hazardous directives detected (`sudo`, `--no-sandbox`, `killall`, `LD_PRELOAD`, etc.).
    pub hazards: Vec<String>,
    /// Non-blocking informational observations (e.g., sets `APPDIR` or `LD_LIBRARY_PATH`).
    pub notes: Vec<String>,
}

/// Result of inspecting a user-supplied binary, AppImage, or extracted AppDir.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinaryInspectionReport {
    /// Canonical or validated path inspected.
    pub path: PathBuf,
    /// Detected container/executable format.
    pub format: BinaryFormatKind,
    /// Parsed ELF64 CPU architecture when applicable.
    pub arch: Option<ElfCpuArch>,
    /// File size in bytes (`0` for directories).
    pub size_bytes: u64,
    /// Computed SHA-256 hex digest for regular files (`None` for directories or files > 512 MiB skipped).
    pub sha256: Option<String>,
    /// Matched community release entry if `sha256` matches a known published artifact.
    pub matched_community_release: Option<&'static CommunityReleaseReference>,
    /// Parsed semantic version hint from filename or bundle metadata.
    pub inferred_version: Option<String>,
    /// Whether POSIX owner-execute permission bit is set (always `true` on non-Unix).
    pub is_executable: bool,
    /// Security block reason if binary is unsafe (e.g., `setuid`, dangerous startup script, digest mismatch).
    pub security_block: Option<SecurityBlockReason>,
    /// Missing dynamic shared libraries (`ldd` `=> not found`) with Ubuntu package remediation hints.
    pub missing_libraries: Vec<MissingLibraryDiagnostic>,
    /// Human-readable audit notes.
    pub notes: Vec<String>,
}

/// Missing shared library entry paired with Ubuntu 24.04 / 22.04 `apt` remediation hints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingLibraryDiagnostic {
    /// Shared object SONAME reported missing (e.g., `"libopus.so.0"`).
    pub soname: String,
    /// Recommended package on Ubuntu 24.04 LTS (`noble`).
    pub ubuntu_2404_package: &'static str,
    /// Recommended package on Ubuntu 22.04 LTS (`jammy`).
    pub ubuntu_2204_package: &'static str,
}

/// Compatible desktop application entry discovered via XDG `.desktop` files or standard AppImage directories.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredLinuxApp {
    /// Display name from `.desktop` `Name=` or filename stem.
    pub name: String,
    /// Resolved executable or `.AppImage` path.
    pub executable_path: PathBuf,
    /// Optional `.desktop` file that registered this application.
    pub desktop_file: Option<PathBuf>,
    /// Optional `StartupWMClass` from `.desktop`.
    pub startup_wm_class: Option<String>,
    /// Detected format of `executable_path`.
    pub format: BinaryFormatKind,
    /// Inferred version string if present in filename or metadata.
    pub version: Option<String>,
    /// Source discovery channel (`"xdg-desktop-entry"`, `"user-applications-dir"`, `"env-override"`, etc.).
    pub discovery_source: String,
}

/// Parsed fields from a freedesktop.org `.desktop` file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DesktopEntryFields {
    /// `Name=` field inside `[Desktop Entry]`.
    pub name: Option<String>,
    /// Unquoted first token of `Exec=` inside `[Desktop Entry]`.
    pub exec_program: Option<PathBuf>,
    /// Raw `Exec=` arguments excluding field codes (`%u`, `%U`, `%f`, `%F`).
    pub exec_args: Vec<String>,
    /// `TryExec=` path if specified.
    pub try_exec: Option<PathBuf>,
    /// `StartupWMClass=` if specified.
    pub startup_wm_class: Option<String>,
    /// `Icon=` value if specified.
    pub icon: Option<String>,
    /// True when `NoDisplay=true` or `Hidden=true`.
    pub hidden: bool,
}

/// Maximum file size hashed automatically during synchronous discovery (`512 MiB`).
const MAX_AUTO_HASH_BYTES: u64 = 512 * 1024 * 1024;

/// Computes the lowercase hex FIPS 180-4 SHA-256 digest of an in-memory byte slice.
#[must_use]
pub fn sha256_bytes(data: &[u8]) -> String {
    let mut hasher = Sha256State::new();
    hasher.update(data);
    encode_hex(&hasher.finalize())
}

/// Computes the lowercase hex FIPS 180-4 SHA-256 digest of a file using a 64 KiB stream buffer.
pub fn sha256_file(path: &Path) -> Result<String, DimError> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256State::new();
    let mut buf = [0u8; 65_536];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(encode_hex(&hasher.finalize()))
}

/// Parses an ELF64 / AppImage / script header and optional tail marker slice.
#[must_use]
pub fn classify_binary_bytes(
    header: &[u8],
    tail: &[u8],
    has_appimage_ext: bool,
) -> (BinaryFormatKind, Option<ElfCpuArch>) {
    if header.starts_with(b"#!") {
        return (BinaryFormatKind::ShellOrScriptWrapper, None);
    }
    if header.len() >= 20 && &header[0..4] == b"\x7fELF" {
        let ei_class = header[4];
        let ei_data = header[5];
        let raw_machine = if ei_data == 2 {
            u16::from_be_bytes([header[18], header[19]])
        } else {
            u16::from_le_bytes([header[18], header[19]])
        };
        let arch = match raw_machine {
            0x3E => Some(ElfCpuArch::X86_64),
            0xB7 => Some(ElfCpuArch::Aarch64),
            other => Some(ElfCpuArch::Other(other)),
        };

        // Standard AppImage Type-2 embeds 'A', 'I', 0x02 at EI_PAD offset 8..11.
        if header.len() >= 11 && &header[8..11] == b"AI\x02" {
            return (BinaryFormatKind::AppImageType2, arch);
        }

        // `quick-sharun` / `uruntime` DwarFS AppImages are ELF64 binaries containing DWARFS/SHARUN/uruntime markers.
        if contains_subslice(header, b"DWARFS")
            || contains_subslice(tail, b"DWARFS")
            || contains_subslice(header, b"uruntime")
            || contains_subslice(tail, b"uruntime")
            || contains_subslice(tail, b"gh-releases-zsync")
            || (has_appimage_ext && ei_class == 2)
        {
            return (BinaryFormatKind::AppImageUruntimeDwarfs, arch);
        }

        if ei_class == 2 {
            return (BinaryFormatKind::Elf64Executable, arch);
        }
    }
    (BinaryFormatKind::Unknown, None)
}

/// Extracts a semantic version string (e.g., `"26.11.10"`) from an AppImage or binary filename.
#[must_use]
pub fn parse_version_from_filename(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    let bytes = name.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            let mut dots = 0usize;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                if bytes[i] == b'.' {
                    dots += 1;
                }
                i += 1;
            }
            let candidate = &name[start..i].trim_matches('.');
            if dots >= 2
                && candidate.split('.').count() >= 3
                && candidate
                    .split('.')
                    .all(|seg| !seg.is_empty() && seg.chars().all(|c| c.is_ascii_digit()))
            {
                return Some((*candidate).to_owned());
            }
        } else {
            i += 1;
        }
    }
    None
}

/// Audits a shell wrapper or `AppRun` script for privilege escalation, sandbox disabling,
/// destructive process kills, or hardcoded user home paths.
#[must_use]
pub fn analyze_startup_script(content: &str) -> StartupScriptAudit {
    let mut hazards = Vec::new();
    let mut notes = Vec::new();
    let interpreter = content
        .lines()
        .next()
        .and_then(|l| l.strip_prefix("#!"))
        .map(|s| s.trim().to_owned());

    for (idx, raw_line) in content.lines().enumerate() {
        let line_no = idx + 1;
        let trimmed = raw_line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let lower = trimmed.to_ascii_lowercase();

        if contains_shell_command(&lower, "sudo")
            || contains_shell_command(&lower, "pkexec")
            || contains_shell_command(&lower, "su")
        {
            hazards.push(format!(
                "line {line_no}: privilege escalation command detected"
            ));
        }
        if lower.contains("--no-sandbox") || lower.contains("--disable-setuid-sandbox") {
            hazards.push(format!(
                "line {line_no}: forces --no-sandbox / disables Chromium sandbox"
            ));
        }
        if lower.contains("apparmor_restrict_unprivileged_userns")
            || contains_shell_command(&lower, "sysctl")
            || contains_shell_command(&lower, "apparmor_parser")
        {
            hazards.push(format!(
                "line {line_no}: attempts to mutate kernel/AppArmor policy"
            ));
        }
        if contains_shell_command(&lower, "killall") || contains_shell_command(&lower, "pkill") {
            hazards.push(format!(
                "line {line_no}: broad process termination (killall/pkill) risks closing other profiles"
            ));
        }
        if lower.contains("ld_preload=") {
            hazards.push(format!("line {line_no}: injects LD_PRELOAD shared object"));
        }
        if (contains_shell_command(&lower, "curl") || contains_shell_command(&lower, "wget"))
            && (lower.contains("| sh") || lower.contains("| bash") || lower.contains("|sh"))
        {
            hazards.push(format!(
                "line {line_no}: remote shell pipe execution detected"
            ));
        }
        if let Some(rest) = trimmed.find("/home/") {
            let after = &trimmed[rest + "/home/".len()..];
            let seg: String = after
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
                .collect();
            if !seg.is_empty() {
                hazards.push(format!(
                    "line {line_no}: hardcoded user home path '/home/{seg}' breaks multi-profile isolation"
                ));
            }
        }
        if trimmed.contains("APPDIR") || trimmed.contains("SHARUN") {
            notes.push(format!(
                "line {line_no}: sets AppImage/sharun runtime variable"
            ));
        }
    }

    StartupScriptAudit {
        interpreter,
        safe: hazards.is_empty(),
        hazards,
        notes,
    }
}

/// Inspects a user-supplied local AppImage, ELF binary, script wrapper, or extracted AppDir.
pub fn inspect_user_binary(
    path: &Path,
    expected_sha256: Option<&str>,
) -> Result<BinaryInspectionReport, DimError> {
    validate_directory_input(path)?;
    let meta = fs::symlink_metadata(path).map_err(|_| {
        DimError::InstallNotFound(format!(
            "custom binary or AppImage not found: {}",
            path.display()
        ))
    })?;

    let resolved_path = if meta.file_type().is_symlink() {
        fs::canonicalize(path).map_err(|_| {
            DimError::PathTraversal(format!(
                "broken or unresolvable symlink binary path: {}",
                path.display()
            ))
        })?
    } else {
        path.to_path_buf()
    };

    let target_meta = fs::metadata(&resolved_path)?;
    if target_meta.is_dir() {
        return inspect_extracted_appdir(&resolved_path);
    }
    if !target_meta.is_file() {
        return Err(DimError::InstallNotFound(format!(
            "target is neither a regular file nor an AppDir: {}",
            resolved_path.display()
        )));
    }

    let size_bytes = target_meta.len();
    let (is_executable, mut security_block, mut notes) =
        inspect_unix_mode_bits(&resolved_path, &target_meta);

    let mut file = File::open(&resolved_path)?;
    let mut header = vec![0u8; 4096.min(usize::try_from(size_bytes).unwrap_or(4096))];
    let n_head = file.read(&mut header)?;
    header.truncate(n_head);

    let mut tail = Vec::new();
    if size_bytes > 4096 {
        let tail_len = 8192u64.min(size_bytes);
        if file
            .seek(SeekFrom::End(-i64::try_from(tail_len).unwrap_or(4096)))
            .is_ok()
        {
            tail.resize(usize::try_from(tail_len).unwrap_or(0), 0u8);
            if let Ok(n_tail) = file.read(&mut tail) {
                tail.truncate(n_tail);
            }
        }
    }

    let has_appimage_ext = resolved_path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("appimage"));
    let (format, arch) = classify_binary_bytes(&header, &tail, has_appimage_ext);

    if format == BinaryFormatKind::ShellOrScriptWrapper {
        let max_script = usize::try_from(size_bytes.min(256 * 1024)).unwrap_or(256 * 1024);
        let mut raw_script = Vec::with_capacity(max_script);
        let mut f2 = File::open(&resolved_path)?;
        let _ = (&mut f2)
            .take(max_script as u64)
            .read_to_end(&mut raw_script)?;
        let text = String::from_utf8_lossy(&raw_script);
        let audit = analyze_startup_script(&text);
        notes.extend(audit.notes);
        if !audit.safe && security_block.is_none() {
            security_block = Some(SecurityBlockReason::UnsafeStartupScript {
                findings: audit.hazards,
            });
        }
    }

    let sha256 = if size_bytes <= MAX_AUTO_HASH_BYTES {
        Some(sha256_file(&resolved_path)?)
    } else {
        notes.push("Skipped synchronous SHA-256 hash because file exceeds 512 MiB.".to_owned());
        None
    };

    if let Some(expected) = expected_sha256.map(str::trim).filter(|s| !s.is_empty()) {
        let actual = match &sha256 {
            Some(d) => d.clone(),
            None => sha256_file(&resolved_path)?,
        };
        if !actual.eq_ignore_ascii_case(expected) {
            return Err(DimError::UnverifiedBinary(format!(
                "SHA-256 mismatch for '{}': expected {expected}, computed {actual}",
                resolved_path.display()
            )));
        }
        notes.push(format!("Caller-pinned SHA-256 verified ({actual})."));
    }

    let matched_community_release = sha256.as_deref().and_then(match_known_community_release);
    if let Some(known) = matched_community_release {
        notes.push(format!(
            "SHA-256 matches community build VN-Linux-Family/zalo-for-linux {} ({}, {}); UNOFFICIAL port — NOT endorsed or signed by VNG.",
            known.version, known.variant, known.arch
        ));
    } else {
        notes.push(
            "Unpinned local binary: community source availability does not guarantee binary provenance; verify SHA-256 before running with sensitive accounts."
                .to_owned(),
        );
    }

    let inferred_version = matched_community_release
        .map(|m| m.version.to_owned())
        .or_else(|| parse_version_from_filename(&resolved_path));

    let missing_libraries = if matches!(
        format,
        BinaryFormatKind::Elf64Executable
            | BinaryFormatKind::AppImageType2
            | BinaryFormatKind::AppImageUruntimeDwarfs
    ) {
        probe_ldd_missing_libraries(&resolved_path)
    } else {
        Vec::new()
    };

    Ok(BinaryInspectionReport {
        path: resolved_path,
        format,
        arch,
        size_bytes,
        sha256,
        matched_community_release,
        inferred_version,
        is_executable,
        security_block,
        missing_libraries,
        notes,
    })
}

fn inspect_extracted_appdir(dir: &Path) -> Result<BinaryInspectionReport, DimError> {
    let apprun = dir.join("AppRun");
    let zalo_bin = dir.join("zalo");
    let entry = if apprun.is_file() {
        apprun
    } else if zalo_bin.is_file() {
        zalo_bin.clone()
    } else {
        return Err(DimError::InstallNotFound(format!(
            "extracted AppDir missing AppRun or zalo executable: {}",
            dir.display()
        )));
    };

    let mut security_block = None;
    let mut notes = vec![format!(
        "Extracted AppDir detected (entrypoint: {}).",
        entry.display()
    )];
    let is_executable = check_entry_executable(&entry);

    if let Ok(mut f) = File::open(&entry) {
        let mut head = [0u8; 2];
        if f.read(&mut head).unwrap_or(0) == 2 && &head == b"#!" {
            if let Ok(script_text) = fs::read_to_string(&entry) {
                let audit = analyze_startup_script(&script_text);
                notes.extend(audit.notes);
                if !audit.safe {
                    security_block = Some(SecurityBlockReason::UnsafeStartupScript {
                        findings: audit.hazards,
                    });
                }
            }
        }
    }

    let version = read_appdir_version(dir).or_else(|| parse_version_from_filename(dir));
    let missing_libraries = if zalo_bin.is_file() {
        probe_ldd_missing_libraries(&zalo_bin)
    } else {
        Vec::new()
    };

    Ok(BinaryInspectionReport {
        path: entry,
        format: BinaryFormatKind::ExtractedAppDir,
        arch: None,
        size_bytes: 0,
        sha256: None,
        matched_community_release: None,
        inferred_version: version,
        is_executable,
        security_block,
        missing_libraries,
        notes,
    })
}

fn inspect_unix_mode_bits(
    resolved_path: &Path,
    target_meta: &fs::Metadata,
) -> (bool, Option<SecurityBlockReason>, Vec<String>) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = target_meta.permissions().mode();
        let is_executable = (mode & 0o111) != 0;
        let mut security_block = None;
        let mut notes = Vec::new();
        if (mode & 0o6000) != 0 {
            security_block = Some(SecurityBlockReason::SetuidBinaryRejected {
                mode: mode & 0o7777,
            });
        } else if (mode & 0o002) != 0 {
            security_block = Some(SecurityBlockReason::InsecurePermissions {
                path: resolved_path.display().to_string(),
                detail: format!("binary is world-writable (mode={:#o})", mode & 0o7777),
            });
        }
        if !is_executable {
            notes.push(
                "Binary lacks executable bit (+x); run `chmod +x <file>` before launching."
                    .to_owned(),
            );
        }
        (is_executable, security_block, notes)
    }
    #[cfg(not(unix))]
    {
        let _ = (resolved_path, target_meta);
        (true, None, Vec::new())
    }
}

fn check_entry_executable(entry: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(entry).is_ok_and(|m| (m.permissions().mode() & 0o111) != 0)
    }
    #[cfg(not(unix))]
    {
        let _ = entry;
        true
    }
}

fn read_appdir_version(dir: &Path) -> Option<String> {
    let candidates = [
        dir.join("app").join("pc-dist").join("build-info.json"),
        dir.join("app").join("package.json.bak"),
        dir.join("resources").join("app").join("package.json"),
    ];
    for cand in candidates {
        if let Ok(raw) = fs::read_to_string(&cand) {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) {
                if let Some(ver) = val.get("version").and_then(serde_json::Value::as_str) {
                    let trimmed = ver.trim();
                    if !trimmed.is_empty() {
                        return Some(trimmed.to_owned());
                    }
                }
            }
        }
    }
    None
}

/// Parses `ldd` stdout text and extracts missing shared objects with Ubuntu 24.04 / 22.04 package hints.
#[must_use]
pub fn parse_ldd_missing_libraries(ldd_stdout: &str) -> Vec<MissingLibraryDiagnostic> {
    let mut missing = Vec::new();
    for line in ldd_stdout.lines() {
        let trimmed = line.trim();
        if let Some((left, right)) = trimmed.split_once("=>") {
            if right.trim().starts_with("not found") {
                let soname = left.trim().to_owned();
                if !soname.is_empty()
                    && !missing
                        .iter()
                        .any(|m: &MissingLibraryDiagnostic| m.soname == soname)
                {
                    let (u2404, u2204) = map_soname_to_ubuntu_packages(&soname);
                    missing.push(MissingLibraryDiagnostic {
                        soname,
                        ubuntu_2404_package: u2404,
                        ubuntu_2204_package: u2204,
                    });
                }
            }
        }
    }
    missing
}

/// Maps common Electron, AppImage, WebKitGTK, and `zalo-for-linux` shared library SONAMEs
/// to their exact Ubuntu 24.04 (`t64` transition) and Ubuntu 22.04 `apt` package names.
#[must_use]
pub fn map_soname_to_ubuntu_packages(soname: &str) -> (&'static str, &'static str) {
    match soname {
        "libfuse.so.2" => ("libfuse2t64", "libfuse2"),
        "libfuse3.so.3" => ("libfuse3-3", "libfuse3-3"),
        "libopus.so.0" => ("libopus0", "libopus0"),
        "libpulse.so.0" | "libpulse-simple.so.0" => ("libpulse0", "libpulse0"),
        "libasound.so.2" => ("libasound2t64", "libasound2"),
        "libgtk-3.so.0" => ("libgtk-3-0t64", "libgtk-3-0"),
        "libnss3.so" => ("libnss3", "libnss3"),
        "libgbm.so.1" => ("libgbm1", "libgbm1"),
        "libatk-1.0.so.0" => ("libatk1.0-0t64", "libatk1.0-0"),
        "libatk-bridge-2.0.so.0" => ("libatk-bridge2.0-0t64", "libatk-bridge2.0-0"),
        "libcups.so.2" => ("libcups2t64", "libcups2"),
        "libdrm.so.2" => ("libdrm2", "libdrm2"),
        "libwebkit2gtk-4.1.so.0" => ("libwebkit2gtk-4.1-0", "libwebkit2gtk-4.1-0"),
        "libayatana-appindicator3.so.1" => {
            ("libayatana-appindicator3-1", "libayatana-appindicator3-1")
        }
        _ => (
            "(inspect via `apt-file search`)",
            "(inspect via `apt-file search`)",
        ),
    }
}

fn probe_ldd_missing_libraries(binary_path: &Path) -> Vec<MissingLibraryDiagnostic> {
    #[cfg(target_os = "linux")]
    {
        use std::process::Command;
        if binary_path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("appimage"))
        {
            // Self-contained AppImage outer runtimes (`uruntime`) are static or only link libc/libdl;
            // running `ldd` on untrusted ELF files can invoke dynamic linkers on older glibc, so only
            // probe `ldd` if it is a regular non-setuid file owned or readable safely.
            return Vec::new();
        }
        if let Ok(out) = Command::new("ldd").arg("--").arg(binary_path).output() {
            if out.status.success() {
                let text = String::from_utf8_lossy(&out.stdout);
                return parse_ldd_missing_libraries(&text);
            }
        }
        Vec::new()
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = binary_path;
        Vec::new()
    }
}

/// Parses a freedesktop.org `.desktop` entry text (`[Desktop Entry]` section only).
#[must_use]
pub fn parse_desktop_entry(content: &str) -> DesktopEntryFields {
    let mut fields = DesktopEntryFields::default();
    let mut in_main_section = false;

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            in_main_section = line == "[Desktop Entry]";
            continue;
        }
        if !in_main_section {
            continue;
        }
        let Some((key, val)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let val = val.trim();
        match key {
            "Name" if fields.name.is_none() => fields.name = Some(val.to_owned()),
            "Exec" => {
                let tokens = split_desktop_exec_tokens(val);
                let mut iter = tokens.into_iter().filter(|t| !t.starts_with('%'));
                if let Some(first) = iter.next() {
                    if first == "env" {
                        for tok in iter.by_ref() {
                            if !tok.contains('=') {
                                fields.exec_program = Some(PathBuf::from(tok));
                                break;
                            }
                        }
                    } else {
                        fields.exec_program = Some(PathBuf::from(first));
                    }
                    fields.exec_args = iter.collect();
                }
            }
            "TryExec" => fields.try_exec = Some(PathBuf::from(val)),
            "StartupWMClass" => fields.startup_wm_class = Some(val.to_owned()),
            "Icon" => fields.icon = Some(val.to_owned()),
            "NoDisplay" | "Hidden" if val.eq_ignore_ascii_case("true") => {
                fields.hidden = true;
            }
            _ => {}
        }
    }

    fields
}

fn split_desktop_exec_tokens(raw: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = raw.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '"' => in_quotes = !in_quotes,
            '\\' if in_quotes => {
                if let Some(&next_ch) = chars.peek() {
                    if matches!(next_ch, '"' | '\\' | '$' | '`') {
                        let _ = chars.next();
                        current.push(next_ch);
                    } else {
                        current.push('\\');
                    }
                } else {
                    current.push('\\');
                }
            }
            c if c.is_whitespace() && !in_quotes => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

/// Discovers compatible Zalo community AppImages or `.desktop`-registered launchers across standard XDG locations
/// without ever hardcoding a personal username.
#[must_use]
pub fn discover_compatible_apps_from_env(
    env_lookup: &dyn Fn(&str) -> Option<String>,
) -> Vec<DiscoveredLinuxApp> {
    let mut desktop_dirs = Vec::new();
    let mut binary_dirs = Vec::new();

    if let Some(explicit) = env_lookup("DIM_LINUX_APPIMAGE_PATH").filter(|s| !s.trim().is_empty()) {
        let p = PathBuf::from(explicit.trim());
        if p.is_file() {
            let mut single = Vec::new();
            push_candidate_file(&mut single, &p, "env:DIM_LINUX_APPIMAGE_PATH");
            if !single.is_empty() {
                return single;
            }
        }
    }

    if let Some(xdg_data) = env_lookup("XDG_DATA_HOME").filter(|s| !s.trim().is_empty()) {
        desktop_dirs.push(PathBuf::from(xdg_data).join("applications"));
    }
    if let Some(home) = env_lookup("HOME").filter(|s| !s.trim().is_empty()) {
        let home_path = PathBuf::from(home);
        let local_apps = home_path.join(".local").join("share").join("applications");
        if !desktop_dirs.contains(&local_apps) {
            desktop_dirs.push(local_apps);
        }
        binary_dirs.push(home_path.join("Applications"));
        binary_dirs.push(home_path.join(".local").join("bin"));
    }

    let data_dirs_raw = env_lookup("XDG_DATA_DIRS")
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".to_owned());
    for part in data_dirs_raw.split(':').filter(|s| !s.trim().is_empty()) {
        desktop_dirs.push(PathBuf::from(part).join("applications"));
    }
    binary_dirs.push(PathBuf::from("/opt/zalo"));
    binary_dirs.push(PathBuf::from("/opt/Zalo"));

    discover_compatible_apps_in_roots(&desktop_dirs, &binary_dirs)
}

/// Deterministic directory-parameterized scanner for XDG `.desktop` files and AppImage directories.
#[must_use]
pub fn discover_compatible_apps_in_roots(
    desktop_dirs: &[PathBuf],
    binary_dirs: &[PathBuf],
) -> Vec<DiscoveredLinuxApp> {
    let mut found = Vec::new();

    for ddir in desktop_dirs {
        let Ok(entries) = fs::read_dir(ddir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path
                .extension()
                .and_then(|s| s.to_str())
                .is_none_or(|ext| !ext.eq_ignore_ascii_case("desktop"))
            {
                continue;
            }
            let file_stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            let Ok(raw) = fs::read_to_string(&path) else {
                continue;
            };
            let parsed = parse_desktop_entry(&raw);
            if parsed.hidden {
                continue;
            }
            let matches_zalo = file_stem.contains("zalo")
                || parsed
                    .name
                    .as_deref()
                    .is_some_and(|n| n.to_ascii_lowercase().contains("zalo"))
                || parsed
                    .startup_wm_class
                    .as_deref()
                    .is_some_and(|w| w.to_ascii_lowercase().contains("zalo"));
            if !matches_zalo {
                continue;
            }
            let Some(exec_prog) = parsed.exec_program else {
                continue;
            };
            if exec_prog.is_absolute() && exec_prog.is_file() {
                if found
                    .iter()
                    .any(|f: &DiscoveredLinuxApp| f.executable_path == exec_prog)
                {
                    continue;
                }
                let format = inspect_user_binary(&exec_prog, None)
                    .map(|r| r.format)
                    .unwrap_or(BinaryFormatKind::Unknown);
                let version = parse_version_from_filename(&exec_prog);
                found.push(DiscoveredLinuxApp {
                    name: parsed
                        .name
                        .unwrap_or_else(|| "Zalo (Desktop Entry)".to_owned()),
                    executable_path: exec_prog,
                    desktop_file: Some(path),
                    startup_wm_class: parsed.startup_wm_class,
                    format,
                    version,
                    discovery_source: "xdg-desktop-entry".to_owned(),
                });
            }
        }
    }

    for bdir in binary_dirs {
        let Ok(entries) = fs::read_dir(bdir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(fname) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            let lower = fname.to_ascii_lowercase();
            if !lower.contains("zalo") {
                continue;
            }
            if path.is_file() {
                push_candidate_file(&mut found, &path, "appimage-search-dir");
            }
        }
    }

    found
}

fn push_candidate_file(out: &mut Vec<DiscoveredLinuxApp>, path: &Path, source: &str) {
    if out.iter().any(|f| f.executable_path == path) {
        return;
    }
    let format = inspect_user_binary(path, None)
        .map(|r| r.format)
        .unwrap_or(BinaryFormatKind::Unknown);
    if matches!(format, BinaryFormatKind::Unknown) {
        return;
    }
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Zalo")
        .to_owned();
    let version = parse_version_from_filename(path);
    out.push(DiscoveredLinuxApp {
        name,
        executable_path: path.to_path_buf(),
        desktop_file: None,
        startup_wm_class: None,
        format,
        version,
        discovery_source: source.to_owned(),
    });
}

fn contains_subslice(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || haystack.len() < needle.len() {
        return false;
    }
    haystack.windows(needle.len()).any(|w| w == needle)
}

fn contains_shell_command(lower_line: &str, cmd: &str) -> bool {
    lower_line
        .split(|c: char| {
            c.is_whitespace() || matches!(c, ';' | '|' | '&' | '(' | ')' | '`' | '"' | '\'')
        })
        .any(|tok| tok == cmd || tok.ends_with(&format!("/{cmd}")))
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(&mut s, "{b:02x}");
    }
    s
}

/// Pure safe Rust FIPS 180-4 SHA-256 implementation (zero external crate lockfile churn).
struct Sha256State {
    state: [u32; 8],
    buf: [u8; 64],
    buf_len: usize,
    total_bytes: u64,
}

const SHA256_K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

impl Sha256State {
    const fn new() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            buf: [0u8; 64],
            buf_len: 0,
            total_bytes: 0,
        }
    }

    fn update(&mut self, mut input: &[u8]) {
        self.total_bytes = self.total_bytes.wrapping_add(input.len() as u64);
        if self.buf_len > 0 {
            let take = (64 - self.buf_len).min(input.len());
            self.buf[self.buf_len..self.buf_len + take].copy_from_slice(&input[..take]);
            self.buf_len += take;
            input = &input[take..];
            if self.buf_len == 64 {
                let block = self.buf;
                self.compress_block(&block);
                self.buf_len = 0;
            }
        }
        while input.len() >= 64 {
            let mut block = [0u8; 64];
            block.copy_from_slice(&input[..64]);
            self.compress_block(&block);
            input = &input[64..];
        }
        if !input.is_empty() {
            self.buf[..input.len()].copy_from_slice(input);
            self.buf_len = input.len();
        }
    }

    fn finalize(mut self) -> [u8; 32] {
        let bit_len = self.total_bytes.wrapping_mul(8);
        self.buf[self.buf_len] = 0x80;
        self.buf_len += 1;

        if self.buf_len > 56 {
            for b in &mut self.buf[self.buf_len..64] {
                *b = 0;
            }
            let block = self.buf;
            self.compress_block(&block);
            self.buf_len = 0;
        }
        for b in &mut self.buf[self.buf_len..56] {
            *b = 0;
        }
        self.buf[56..64].copy_from_slice(&bit_len.to_be_bytes());
        let final_block = self.buf;
        self.compress_block(&final_block);

        let mut out = [0u8; 32];
        for (i, word) in self.state.iter().enumerate() {
            out[i * 4..(i + 1) * 4].copy_from_slice(&word.to_be_bytes());
        }
        out
    }

    fn compress_block(&mut self, block: &[u8; 64]) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.state;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(SHA256_K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        self.state[0] = self.state[0].wrapping_add(a);
        self.state[1] = self.state[1].wrapping_add(b);
        self.state[2] = self.state[2].wrapping_add(c);
        self.state[3] = self.state[3].wrapping_add(d);
        self.state[4] = self.state[4].wrapping_add(e);
        self.state[5] = self.state[5].wrapping_add(f);
        self.state[6] = self.state[6].wrapping_add(g);
        self.state[7] = self.state[7].wrapping_add(h);
    }
}
