//! macOS `.app` bundle discovery, XML & binary `bplist00` `Info.plist` parsing,
//! helper bundle inspection (`ZaloHelper.app` / `ZaloCall`), and pure-Rust Mach-O
//! CPU architecture slice verification (`arm64`, `x86_64`, Universal2 `FAT_MAGIC`).

use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use dim_core::DimError;

use crate::ops::MacOsSystemOps;

/// Standard system-wide macOS Zalo Desktop bundle path.
pub const DEFAULT_SYSTEM_ZALO_APP: &str = "/Applications/Zalo.app";

/// Maximum path byte length permitted on Darwin (`PATH_MAX` in `<sys/syslimits.h>`).
pub const MACOS_PATH_MAX: usize = 1024;

/// Known official VNG Zalo Desktop bundle identifiers on macOS.
pub const OFFICIAL_ZALO_BUNDLE_IDS: &[&str] =
    &["com.vng.zalo", "com.vng.zalopc", "vn.com.vng.zalo"];

/// Thin 64-bit Mach-O magic (`0xFEEDFACF`).
pub const MH_MAGIC_64: u32 = 0xFEED_FACF;
/// Byte-swapped 64-bit Mach-O cigam (`0xCFFAEDFE`).
pub const MH_CIGAM_64: u32 = 0xCFFA_EDFE;
/// Thin 32-bit Mach-O magic (`0xFEEDFACE`).
pub const MH_MAGIC: u32 = 0xFEED_FACE;
/// Byte-swapped 32-bit Mach-O cigam (`0xCEFAEDFE`).
pub const MH_CIGAM: u32 = 0xCEFA_EDFE;
/// Universal2 32-bit fat header magic (`0xCAFEBABE`, big-endian).
pub const FAT_MAGIC: u32 = 0xCAFE_BABE;
/// Byte-swapped 32-bit fat header cigam (`0xBEBAFECA`).
pub const FAT_CIGAM: u32 = 0xBEBA_FECA;
/// Universal2 64-bit fat header magic (`0xCAFEBABF`, big-endian).
pub const FAT_MAGIC_64: u32 = 0xCAFE_BABF;
/// Byte-swapped 64-bit fat header cigam (`0xBFBAFECA`).
pub const FAT_CIGAM_64: u32 = 0xBFBA_FECA;

/// Darwin `<mach/machine.h>` `CPU_TYPE_X86_64` (`0x01000007`).
pub const CPU_TYPE_X86_64: i32 = 0x0100_0007;
/// Darwin `<mach/machine.h>` `CPU_TYPE_ARM64` (`0x0100000C`).
pub const CPU_TYPE_ARM64: i32 = 0x0100_000C;

/// CPU architecture slice found inside a Mach-O or Universal2 fat executable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacOsArchSlice {
    /// Apple Silicon 64-bit ARM (`aarch64-apple-darwin`).
    Arm64,
    /// Intel 64-bit x86 (`x86_64-apple-darwin`).
    X86_64,
    /// Other Darwin `cpu_type_t` value.
    Other(i32),
}

impl MacOsArchSlice {
    /// Converts a raw Darwin `cputype` integer into a [`MacOsArchSlice`].
    #[must_use]
    pub const fn from_cpu_type(cpu_type: i32) -> Self {
        match cpu_type {
            CPU_TYPE_ARM64 => Self::Arm64,
            CPU_TYPE_X86_64 => Self::X86_64,
            other => Self::Other(other),
        }
    }

    /// Canonical lowercase label (`"arm64"`, `"x86_64"`, or `"other"`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Arm64 => "arm64",
            Self::X86_64 => "x86_64",
            Self::Other(_) => "other",
        }
    }
}

/// Pure-Rust Mach-O header inspection summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachOInspection {
    /// True if the binary begins with a valid thin Mach-O or Universal2 fat header.
    pub is_macho: bool,
    /// True if packaged as a Universal2 fat binary (`FAT_MAGIC` / `FAT_MAGIC_64`).
    pub is_universal_fat: bool,
    /// Ordered list of CPU slices present in the executable.
    pub architectures: Vec<MacOsArchSlice>,
}

impl MachOInspection {
    /// Returns `true` if an `arm64` slice is present.
    #[must_use]
    pub fn supports_arm64(&self) -> bool {
        self.architectures.contains(&MacOsArchSlice::Arm64)
    }

    /// Returns `true` if an `x86_64` slice is present.
    #[must_use]
    pub fn supports_x86_64(&self) -> bool {
        self.architectures.contains(&MacOsArchSlice::X86_64)
    }

    /// Formats a human-readable summary of the Mach-O slices.
    #[must_use]
    pub fn summary_label(&self) -> String {
        if !self.is_macho {
            return "NonMachO".to_owned();
        }
        let slices: Vec<&str> = self.architectures.iter().map(|s| s.as_str()).collect();
        if self.is_universal_fat {
            format!("Universal2 ({})", slices.join(" + "))
        } else {
            format!("Mach-O ({})", slices.join(", "))
        }
    }
}

/// Parsed fields from `Contents/Info.plist` (XML or `bplist00`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlistMetadata {
    /// `CFBundleIdentifier` (e.g., `"com.vng.zalo"`).
    pub bundle_identifier: Option<String>,
    /// `CFBundleExecutable` (e.g., `"Zalo"`).
    pub bundle_executable: Option<String>,
    /// `CFBundleShortVersionString` (e.g., `"26.10.20"`).
    pub short_version: Option<String>,
    /// `CFBundleVersion` build string.
    pub bundle_version: Option<String>,
    /// `CFBundlePackageType` (expected `"APPL"`).
    pub package_type: Option<String>,
    /// `LSMinimumSystemVersion` (e.g., `"11.0"`).
    pub minimum_system_version: Option<String>,
    /// `LSMultipleInstancesProhibited` boolean flag (`open -n` bypasses this when `true`).
    pub multiple_instances_prohibited: bool,
    /// True if the source `Info.plist` used Apple's binary `bplist00` encoding.
    pub was_binary_plist: bool,
}

/// Validated layout and metadata of a discovered macOS `.app` bundle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedMacOsBundle {
    /// Root `.app` bundle directory.
    pub bundle_path: PathBuf,
    /// `Contents/Info.plist` path.
    pub info_plist_path: PathBuf,
    /// Main Mach-O executable (`Contents/MacOS/<CFBundleExecutable>`).
    pub executable_path: PathBuf,
    /// Optional `Contents/Resources/app.asar` bundle.
    pub app_asar_path: Option<PathBuf>,
    /// Optional `ZaloHelper.app` bundle path inside `Contents/Frameworks` or `Contents/Resources`.
    pub helper_bundle_path: Option<PathBuf>,
    /// Optional `ZaloCall` helper executable path.
    pub zalo_call_path: Option<PathBuf>,
    /// Parsed `Info.plist` properties.
    pub plist: PlistMetadata,
    /// Pure-Rust Mach-O slice inspection result, if main executable is readable.
    pub macho: Option<MachOInspection>,
    /// Diagnostic notes collected during bundle inspection.
    pub notes: Vec<String>,
    /// Structural or architecture validation error when invalid.
    pub validation_error: Option<String>,
}

/// Validates a macOS filesystem path against empty strings, NUL bytes, control characters,
/// `..` traversal segments, and Darwin `PATH_MAX` (`1024` bytes).
pub fn validate_macos_path(path: &Path) -> Result<(), DimError> {
    if path.as_os_str().is_empty() {
        return Err(DimError::PathTraversal(
            "macOS path cannot be empty".to_owned(),
        ));
    }
    let raw = path.to_string_lossy();
    if raw.contains('\0') || raw.chars().any(char::is_control) {
        return Err(DimError::PathTraversal(
            "macOS path contains NUL or control characters".to_owned(),
        ));
    }
    if raw.len() >= MACOS_PATH_MAX {
        return Err(DimError::PathTraversal(format!(
            "macOS path length ({} bytes) reaches or exceeds Darwin PATH_MAX ({MACOS_PATH_MAX})",
            raw.len()
        )));
    }
    if raw.split(['/', '\\']).any(|seg| seg == "..")
        || path.components().any(|c| matches!(c, Component::ParentDir))
    {
        return Err(DimError::PathTraversal(
            "macOS path contains '..' parent traversal segment".to_owned(),
        ));
    }
    Ok(())
}

/// Inspects up to 4096 header bytes of a file to identify Mach-O thin or Universal2 fat slices.
pub fn inspect_macho_file(path: &Path) -> Result<MachOInspection, DimError> {
    let mut file = File::open(path)?;
    let mut buf = [0u8; 4096];
    let n = file.read(&mut buf)?;
    Ok(inspect_macho_bytes(&buf[..n]))
}

/// Pure-Rust parser for thin Mach-O (`MH_MAGIC_64`/`MH_MAGIC`) and Universal2 fat (`FAT_MAGIC`/`FAT_MAGIC_64`) headers.
#[must_use]
pub fn inspect_macho_bytes(bytes: &[u8]) -> MachOInspection {
    if bytes.len() < 8 {
        return MachOInspection {
            is_macho: false,
            is_universal_fat: false,
            architectures: Vec::new(),
        };
    }

    let magic_be = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let magic_le = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);

    // Thin 64-bit or 32-bit little-endian Mach-O (`CF FA ED FE` or `CE FA ED FE` on disk).
    if magic_le == MH_MAGIC_64 || magic_le == MH_MAGIC {
        let cpu_type = i32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        return MachOInspection {
            is_macho: true,
            is_universal_fat: false,
            architectures: vec![MacOsArchSlice::from_cpu_type(cpu_type)],
        };
    }

    // Thin big-endian Mach-O (`FE ED FA CF` or `FE ED FA CE` on disk).
    if magic_le == MH_CIGAM_64 || magic_le == MH_CIGAM {
        let cpu_type = i32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        return MachOInspection {
            is_macho: true,
            is_universal_fat: false,
            architectures: vec![MacOsArchSlice::from_cpu_type(cpu_type)],
        };
    }

    // Universal2 32-bit fat header (`CA FE BA BE` on disk, big-endian).
    if magic_be == FAT_MAGIC || magic_be == FAT_CIGAM {
        let swap = magic_be == FAT_CIGAM;
        let nfat = read_u32(&bytes[4..8], !swap) as usize;
        if nfat == 0 || nfat > 16 {
            return MachOInspection {
                is_macho: false,
                is_universal_fat: false,
                architectures: Vec::new(),
            };
        }
        let mut slices = Vec::with_capacity(nfat);
        for i in 0..nfat {
            let offset = 8 + i * 20;
            if offset + 20 > bytes.len() {
                return MachOInspection {
                    is_macho: false,
                    is_universal_fat: false,
                    architectures: Vec::new(),
                };
            }
            let cpu_type = read_i32(&bytes[offset..offset + 4], !swap);
            let slice = MacOsArchSlice::from_cpu_type(cpu_type);
            if !slices.contains(&slice) {
                slices.push(slice);
            }
        }
        return MachOInspection {
            is_macho: true,
            is_universal_fat: true,
            architectures: slices,
        };
    }

    // Universal2 64-bit fat header (`CA FE BA BF` on disk, big-endian, 32-byte entries).
    if magic_be == FAT_MAGIC_64 || magic_be == FAT_CIGAM_64 {
        let swap = magic_be == FAT_CIGAM_64;
        let nfat = read_u32(&bytes[4..8], !swap) as usize;
        if nfat == 0 || nfat > 16 {
            return MachOInspection {
                is_macho: false,
                is_universal_fat: false,
                architectures: Vec::new(),
            };
        }
        let mut slices = Vec::with_capacity(nfat);
        for i in 0..nfat {
            let offset = 8 + i * 32;
            if offset + 32 > bytes.len() {
                return MachOInspection {
                    is_macho: false,
                    is_universal_fat: false,
                    architectures: Vec::new(),
                };
            }
            let cpu_type = read_i32(&bytes[offset..offset + 4], !swap);
            let slice = MacOsArchSlice::from_cpu_type(cpu_type);
            if !slices.contains(&slice) {
                slices.push(slice);
            }
        }
        return MachOInspection {
            is_macho: true,
            is_universal_fat: true,
            architectures: slices,
        };
    }

    MachOInspection {
        is_macho: false,
        is_universal_fat: false,
        architectures: Vec::new(),
    }
}

/// Parses an Apple XML `Info.plist` payload into [`PlistMetadata`].
#[must_use]
pub fn parse_info_plist_xml(plist_xml: &str) -> PlistMetadata {
    PlistMetadata {
        bundle_identifier: extract_plist_string_key(plist_xml, "CFBundleIdentifier"),
        bundle_executable: extract_plist_string_key(plist_xml, "CFBundleExecutable"),
        short_version: extract_plist_string_key(plist_xml, "CFBundleShortVersionString"),
        bundle_version: extract_plist_string_key(plist_xml, "CFBundleVersion"),
        package_type: extract_plist_string_key(plist_xml, "CFBundlePackageType"),
        minimum_system_version: extract_plist_string_key(plist_xml, "LSMinimumSystemVersion"),
        multiple_instances_prohibited: extract_plist_bool_key(
            plist_xml,
            "LSMultipleInstancesProhibited",
        )
        .unwrap_or(false),
        was_binary_plist: false,
    }
}

/// Extracts a `<string>` value for `key` from a macOS XML `Info.plist`.
#[must_use]
pub fn extract_plist_string_key(plist_xml: &str, key: &str) -> Option<String> {
    let marker = format!("<key>{key}</key>");
    let idx = plist_xml.find(&marker)?;
    let rest = plist_xml[idx + marker.len()..].trim_start();
    let rest = skip_xml_comments(rest);
    if !rest.starts_with("<string>") {
        return None;
    }
    let after_open = &rest["<string>".len()..];
    let end_idx = after_open.find("</string>")?;
    let val = decode_xml_entities(after_open[..end_idx].trim());
    (!val.is_empty()).then_some(val)
}

/// Extracts a `<true/>` or `<false/>` value for `key` from a macOS XML `Info.plist`.
#[must_use]
pub fn extract_plist_bool_key(plist_xml: &str, key: &str) -> Option<bool> {
    let marker = format!("<key>{key}</key>");
    let idx = plist_xml.find(&marker)?;
    let rest = plist_xml[idx + marker.len()..].trim_start();
    let rest = skip_xml_comments(rest);
    if rest.starts_with("<true/>") || rest.starts_with("<true />") {
        Some(true)
    } else if rest.starts_with("<false/>") || rest.starts_with("<false />") {
        Some(false)
    } else {
        None
    }
}

/// Reads and parses `Contents/Info.plist`, automatically converting binary `bplist00` files via
/// `/usr/bin/plutil -convert xml1 -o -` when available on macOS (or mock harness).
pub fn read_bundle_plist(
    ops: &dyn MacOsSystemOps,
    plist_path: &Path,
) -> Result<PlistMetadata, DimError> {
    let raw_bytes = fs::read(plist_path)?;
    if raw_bytes.starts_with(b"bplist00") {
        let args = vec![
            "-convert".to_owned(),
            "xml1".to_owned(),
            "-o".to_owned(),
            "-".to_owned(),
            plist_path.to_string_lossy().into_owned(),
        ];
        if let Ok(out) = ops.run_command(Path::new("/usr/bin/plutil"), &args, &[]) {
            if out.success() {
                let mut parsed = parse_info_plist_xml(&out.stdout_lossy());
                parsed.was_binary_plist = true;
                return Ok(parsed);
            }
        }
        let mut fallback = parse_binary_plist_ascii_fallback(&raw_bytes);
        fallback.was_binary_plist = true;
        return Ok(fallback);
    }

    let xml = String::from_utf8_lossy(&raw_bytes);
    Ok(parse_info_plist_xml(&xml))
}

/// Discovers and validates the macOS Zalo `.app` bundle across `custom_root`,
/// `/Applications/Zalo.app`, `~/Applications/Zalo.app`, and Spotlight (`mdfind`).
pub fn discover_zalo_bundle(
    ops: &dyn MacOsSystemOps,
    custom_root: Option<&Path>,
) -> Result<Option<ValidatedMacOsBundle>, DimError> {
    if let Some(explicit) = custom_root {
        validate_macos_path(explicit)?;
        if !explicit.exists() {
            return Ok(None);
        }
        return Ok(Some(inspect_bundle_at(ops, explicit)?));
    }

    let mut candidates = vec![PathBuf::from(DEFAULT_SYSTEM_ZALO_APP)];
    if let Some(home) = ops.user_home_dir() {
        candidates.push(home.join("Applications").join("Zalo.app"));
    }

    for cand in &candidates {
        if cand.exists() {
            return Ok(Some(inspect_bundle_at(ops, cand)?));
        }
    }

    // Spotlight fallback on macOS when Zalo.app was moved outside /Applications.
    if ops.is_native_macos() {
        for bundle_id in OFFICIAL_ZALO_BUNDLE_IDS {
            let query = format!("kMDItemCFBundleIdentifier == '{bundle_id}'");
            let args = vec![query];
            if let Ok(out) = ops.run_command(Path::new("/usr/bin/mdfind"), &args, &[]) {
                if out.success() {
                    for line in out.stdout_lossy().lines() {
                        let trimmed = line.trim();
                        if trimmed.is_empty() || !trimmed.ends_with(".app") {
                            continue;
                        }
                        let cand = PathBuf::from(trimmed);
                        if validate_macos_path(&cand).is_ok() && cand.exists() {
                            return Ok(Some(inspect_bundle_at(ops, &cand)?));
                        }
                    }
                }
            }
        }
    }

    Ok(None)
}

/// Inspects an existing `.app` directory for plist metadata, executable slices, `app.asar`, and `ZaloHelper.app`.
pub fn inspect_bundle_at(
    ops: &dyn MacOsSystemOps,
    bundle_path: &Path,
) -> Result<ValidatedMacOsBundle, DimError> {
    validate_macos_path(bundle_path)?;

    let contents_dir = bundle_path.join("Contents");
    let info_plist_path = contents_dir.join("Info.plist");
    let mut notes = Vec::new();

    let plist = if info_plist_path.is_file() {
        read_bundle_plist(ops, &info_plist_path)?
    } else {
        return Ok(ValidatedMacOsBundle {
            bundle_path: bundle_path.to_path_buf(),
            info_plist_path,
            executable_path: contents_dir.join("MacOS").join("Zalo"),
            app_asar_path: None,
            helper_bundle_path: None,
            zalo_call_path: None,
            plist: PlistMetadata::default(),
            macho: None,
            notes,
            validation_error: Some(format!(
                "missing Contents/Info.plist inside {}",
                bundle_path.display()
            )),
        });
    };

    if plist.was_binary_plist {
        notes.push("Parsed binary Info.plist (bplist00).".to_owned());
    }
    if plist.multiple_instances_prohibited {
        notes.push(
            "Info.plist sets LSMultipleInstancesProhibited=true; `open -n` overrides this safely."
                .to_owned(),
        );
    }

    let exe_name = plist
        .bundle_executable
        .as_deref()
        .filter(|s| !s.contains('/') && !s.contains('\\') && *s != "..")
        .unwrap_or("Zalo");
    let executable_path = contents_dir.join("MacOS").join(exe_name);
    let asar_candidate = contents_dir.join("Resources").join("app.asar");
    let app_asar_path = asar_candidate.is_file().then_some(asar_candidate);

    let (helper_bundle_path, zalo_call_path) = locate_zalo_helper(&contents_dir);
    if zalo_call_path.is_some() {
        notes.push(
            "Detected ZaloCall helper (binds global /tmp/socketzalorecv2021 on calls).".to_owned(),
        );
    }

    if !executable_path.is_file() {
        return Ok(ValidatedMacOsBundle {
            bundle_path: bundle_path.to_path_buf(),
            info_plist_path,
            executable_path: executable_path.clone(),
            app_asar_path,
            helper_bundle_path,
            zalo_call_path,
            plist,
            macho: None,
            notes,
            validation_error: Some(format!(
                "bundle executable missing at {}",
                executable_path.display()
            )),
        });
    }

    if let Some(pkg_type) = plist.package_type.clone() {
        if pkg_type != "APPL" {
            return Ok(ValidatedMacOsBundle {
                bundle_path: bundle_path.to_path_buf(),
                info_plist_path,
                executable_path,
                app_asar_path,
                helper_bundle_path,
                zalo_call_path,
                plist,
                macho: None,
                notes,
                validation_error: Some(format!(
                    "unexpected CFBundlePackageType '{pkg_type}' (expected 'APPL')"
                )),
            });
        }
    }

    let macho = inspect_macho_file(&executable_path).ok();
    let mut arch_error: Option<String> = None;

    if let Some(ref m) = macho {
        notes.push(format!("Executable binary format: {}.", m.summary_label()));
        if m.is_macho {
            let host_arch = ops.host_arch();
            if host_arch == "x86_64" && !m.supports_x86_64() && m.supports_arm64() {
                arch_error = Some(
                    "Apple Silicon arm64-only Mach-O bundle cannot execute on Intel x86_64 host"
                        .to_owned(),
                );
            } else if host_arch == "aarch64" && !m.supports_arm64() && m.supports_x86_64() {
                notes.push(
                    "Intel x86_64-only Mach-O bundle on Apple Silicon (arm64) host: requires Rosetta 2 (oahd)."
                        .to_owned(),
                );
            }
        }
    }

    Ok(ValidatedMacOsBundle {
        bundle_path: bundle_path.to_path_buf(),
        info_plist_path,
        executable_path,
        app_asar_path,
        helper_bundle_path,
        zalo_call_path,
        plist,
        macho,
        notes,
        validation_error: arch_error,
    })
}

fn locate_zalo_helper(contents_dir: &Path) -> (Option<PathBuf>, Option<PathBuf>) {
    let helper_candidates = [
        contents_dir.join("Frameworks").join("ZaloHelper.app"),
        contents_dir.join("Resources").join("ZaloHelper.app"),
        contents_dir.join("Frameworks").join("Zalo Helper.app"),
    ];
    for helper in helper_candidates {
        if helper.is_dir() {
            let call = helper.join("Contents").join("MacOS").join("ZaloCall");
            let call_opt = call.is_file().then_some(call);
            return (Some(helper), call_opt);
        }
    }
    (None, None)
}

fn read_u32(bytes: &[u8], big_endian: bool) -> u32 {
    let arr = [bytes[0], bytes[1], bytes[2], bytes[3]];
    if big_endian {
        u32::from_be_bytes(arr)
    } else {
        u32::from_le_bytes(arr)
    }
}

fn read_i32(bytes: &[u8], big_endian: bool) -> i32 {
    let arr = [bytes[0], bytes[1], bytes[2], bytes[3]];
    if big_endian {
        i32::from_be_bytes(arr)
    } else {
        i32::from_le_bytes(arr)
    }
}

fn skip_xml_comments(mut input: &str) -> &str {
    while let Some(after_open) = input.strip_prefix("<!--") {
        if let Some(end_idx) = after_open.find("-->") {
            input = after_open[end_idx + 3..].trim_start();
        } else {
            break;
        }
    }
    input
}

fn decode_xml_entities(raw: &str) -> String {
    raw.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

/// Best-effort ASCII token extractor when inspecting a `bplist00` fixture on a non-macOS host without `plutil`.
fn parse_binary_plist_ascii_fallback(bytes: &[u8]) -> PlistMetadata {
    let runs = extract_ascii_runs(bytes);
    let find_after = |key: &str| -> Option<String> {
        let pos = runs.iter().position(|s| s == key)?;
        runs.get(pos + 1).cloned()
    };
    PlistMetadata {
        bundle_identifier: find_after("CFBundleIdentifier"),
        bundle_executable: find_after("CFBundleExecutable"),
        short_version: find_after("CFBundleShortVersionString"),
        bundle_version: find_after("CFBundleVersion"),
        package_type: find_after("CFBundlePackageType"),
        minimum_system_version: find_after("LSMinimumSystemVersion"),
        multiple_instances_prohibited: false,
        was_binary_plist: true,
    }
}

fn extract_ascii_runs(bytes: &[u8]) -> Vec<String> {
    let mut runs = Vec::new();
    let mut current = Vec::new();
    for &b in bytes.iter().skip(8) {
        if (0x20..=0x7E).contains(&b) {
            current.push(b);
        } else {
            if current.len() >= 2 {
                if let Ok(s) = String::from_utf8(current.clone()) {
                    runs.push(s);
                }
            }
            current.clear();
        }
    }
    if current.len() >= 2 {
        if let Ok(s) = String::from_utf8(current) {
            runs.push(s);
        }
    }
    runs
}
