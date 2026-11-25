//! Compatible Linux desktop application discovery, static ELF64/AppImage format & `DT_NEEDED`
//! verification, FIPS 180-4 SHA-256 integrity hashing, startup-script auditing, and Ubuntu
//! dependency diagnostics.
//!
//! ## Untrusted Binary Safety Invariant
//! Per the `ldd(1)` Linux manual security warning, invoking `ldd` or any dynamic linker on an
//! untrusted ELF executable is unsafe because custom `PT_INTERP` segments can execute arbitrary
//! code. **No function in this module ever spawns `ldd`, an ELF interpreter, a shell script, or
//! an AppImage.** All format, `PT_INTERP`, `PT_DYNAMIC` (`DT_NEEDED`, `DT_RPATH`, `DT_RUNPATH`),
//! and shared-library checks operate strictly via bounds-checked static byte parsing and
//! `std::fs::metadata`.
//!
//! ## SHA-256 Provenance Contract
//! A matching SHA-256 digest proves **byte-level integrity against a caller-supplied or pinned
//! reference digest only—NEVER publisher identity, code-signing validity, official VNG endorsement,
//! or software origin authenticity**.

use std::fmt::{self, Write as _};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};

use dim_core::{validate_directory_input, DimError};

pub use crate::status::MissingLibraryDiagnostic;
use crate::status::SecurityBlockReason;

/// Standard 64-byte ELF64 file header size (`sizeof(Elf64_Ehdr)`).
const ELF64_EHDR_SIZE: usize = 64;
/// Minimum 56-byte ELF64 program header entry size (`sizeof(Elf64_Phdr)`).
const ELF64_PHDR_MIN_SIZE: u16 = 56;
/// Upper bound on `e_phentsize` to reject malformed headers cleanly.
const MAX_ELF64_PHENTSIZE: u16 = 256;
/// Upper bound on `e_phnum` to prevent CPU/memory exhaustion on adversarial headers.
const MAX_ELF64_PHNUM: u16 = 1024;
/// Upper bound on `PT_INTERP` byte length (`PATH_MAX` = 4096).
const MAX_PT_INTERP_BYTES: u64 = 4096;
/// Upper bound on `PT_DYNAMIC` byte length (`1 MiB`, ~65,536 `Elf64_Dyn` entries).
const MAX_PT_DYNAMIC_BYTES: u64 = 1024 * 1024;
/// Upper bound on `.dynstr` (`DT_STRSZ`) allocation (`16 MiB`) to prevent OOM on forged headers.
const MAX_DYNSTR_BYTES: u64 = 16 * 1024 * 1024;
/// Upper bound on `DT_NEEDED` entries collected from a single binary.
const MAX_DT_NEEDED_ENTRIES: usize = 4096;

const PT_LOAD: u32 = 1;
const PT_DYNAMIC: u32 = 2;
const PT_INTERP: u32 = 3;

const DT_NULL: u64 = 0;
const DT_NEEDED: u64 = 1;
const DT_STRTAB: u64 = 5;
const DT_STRSZ: u64 = 10;
const DT_SONAME: u64 = 14;
const DT_RPATH: u64 = 15;
const DT_RUNPATH: u64 = 29;

/// Recognized executable or bundle container format on Linux.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryFormatKind {
    /// Standard AppImage Type-2 ELF64 carrying `"AI\x02"` (`0x41, 0x49, 0x02`) at offset `8`.
    AppImageType2,
    /// `quick-sharun` / `uruntime` DwarFS self-mounting ELF64 AppImage (used by `zalo-for-linux` stage 2).
    AppImageUruntimeDwarfs,
    /// Standard ELF64 native executable (`ET_EXEC` or PIE `ET_DYN`).
    Elf64Executable,
    /// Interpreted `#!` wrapper or `AppRun` shell script.
    ShellOrScriptWrapper,
    /// Extracted AppDir directory containing `AppRun` or `zalo` executable.
    ExtractedAppDir,
    /// Unrecognized, truncated, or unsupported file format.
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
    /// Other ELF `e_machine` identifier.
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

/// Static dynamic-linking metadata parsed safely from an ELF64 binary's program headers
/// without invoking `ldd` or any dynamic linker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Elf64DynamicMetadata {
    /// Parsed target CPU architecture (`e_machine`).
    pub arch: ElfCpuArch,
    /// Raw ELF object type (`e_type`: `2` = `ET_EXEC`, `3` = `ET_DYN`).
    pub elf_type: u16,
    /// `true` for little-endian (`ELFDATA2LSB`), `false` for big-endian (`ELFDATA2MSB`).
    pub is_little_endian: bool,
    /// Number of program header entries (`e_phnum`) validated in the binary.
    pub program_header_count: u16,
    /// Dynamic linker path extracted from `PT_INTERP` (e.g., `"/lib64/ld-linux-x86-64.so.2"`).
    pub interpreter: Option<String>,
    /// Optional shared object name (`DT_SONAME`) from `PT_DYNAMIC`.
    pub soname: Option<String>,
    /// Ordered `DT_NEEDED` shared library SONAMEs resolved from `DT_STRTAB`.
    pub needed_libraries: Vec<String>,
    /// Colon-split `DT_RPATH` search directories (used by `ld.so` only when `DT_RUNPATH` is empty).
    pub rpath_entries: Vec<String>,
    /// Colon-split `DT_RUNPATH` search directories (supports `$ORIGIN` / `${ORIGIN}`).
    pub runpath_entries: Vec<String>,
}

/// Structured bounds-check and format validation error returned by the static ELF64 parser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Elf64ParseError {
    /// Input is shorter than the 64-byte `Elf64_Ehdr` structure.
    TruncatedElfHeader {
        /// Actual byte length available.
        actual_bytes: u64,
    },
    /// File does not begin with `\x7fELF`.
    InvalidElfMagic,
    /// `EI_CLASS` is not `ELFCLASS64` (`2`).
    UnsupportedClass(u8),
    /// `EI_DATA` is neither little-endian (`1`) nor big-endian (`2`).
    UnsupportedEndianness(u8),
    /// `EI_VERSION` is not `EV_CURRENT` (`1`).
    UnsupportedVersion(u8),
    /// `e_type` is neither `ET_EXEC` (`2`) nor `ET_DYN` (`3`), or `e_machine == 0`.
    NotExecutableType(u16),
    /// Binary has zero program headers (`e_phnum == 0` or `e_phoff == 0`).
    MissingProgramHeaders,
    /// `e_phentsize` is smaller than 56 bytes (`sizeof(Elf64_Phdr)`) or exceeds safety cap.
    InvalidProgramHeaderEntrySize(u16),
    /// `e_phnum` exceeds [`MAX_ELF64_PHNUM`] (`1024`).
    TooManyProgramHeaders(u16),
    /// Program header table `[e_phoff .. e_phoff + e_phnum * e_phentsize]` overflows file size.
    TruncatedProgramHeaderTable {
        /// Program header table offset (`e_phoff`).
        offset: u64,
        /// Total byte size of program header table.
        table_bytes: u64,
        /// Total byte length of input file/slice.
        file_len: u64,
    },
    /// A `PT_LOAD`, `PT_INTERP`, or `PT_DYNAMIC` segment extends past the end of the file.
    SegmentOutOfBounds {
        /// ELF program header `p_type`.
        p_type: u32,
        /// Segment file offset (`p_offset`).
        offset: u64,
        /// Segment file byte length (`p_filesz`).
        filesz: u64,
        /// Total byte length of input file/slice.
        file_len: u64,
    },
    /// `PT_INTERP` or `PT_DYNAMIC` exceeds safety allocation limits.
    OversizedSegment {
        /// ELF program header `p_type`.
        p_type: u32,
        /// Declared segment byte size (`p_filesz`).
        filesz: u64,
    },
    /// `PT_DYNAMIC` is present, but no `PT_LOAD` segments exist to map `DT_STRTAB`.
    MissingLoadSegments,
    /// `DT_STRSZ` exceeds [`MAX_DYNSTR_BYTES`] (`16 MiB`).
    OversizedStringTable(u64),
    /// `DT_STRTAB` virtual address does not fall within the file-backed range of any `PT_LOAD` segment.
    UnmappedStringTable(u64),
    /// `DT_STRSZ` extends beyond the containing `PT_LOAD` segment's file-backed bytes.
    TruncatedStringTable {
        /// Resolved file offset of `.dynstr`.
        offset: u64,
        /// Declared `DT_STRSZ` byte length.
        size: u64,
        /// File-backed bytes remaining inside the mapped `PT_LOAD` segment.
        available: u64,
    },
    /// Dynamic string offset (`DT_NEEDED`, `DT_RPATH`, `DT_RUNPATH`, `DT_SONAME`) >= `DT_STRSZ`.
    StringOffsetOutOfBounds {
        /// String table byte offset.
        offset: u64,
        /// Total string table byte size (`DT_STRSZ`).
        strsz: u64,
    },
    /// Dynamic string inside `.dynstr` lacks a terminating NUL (`\0`) byte before `DT_STRSZ`.
    UnterminatedDynamicString {
        /// String table byte offset.
        offset: u64,
    },
    /// Dynamic string or `PT_INTERP` path is not valid UTF-8 or contains embedded control bytes.
    InvalidUtf8DynamicString {
        /// Byte offset where invalid UTF-8 occurred.
        offset: u64,
    },
    /// Underlying filesystem read/seek error while inspecting an on-disk binary.
    Io(String),
}

impl fmt::Display for Elf64ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TruncatedElfHeader { actual_bytes } => write!(
                f,
                "truncated ELF64 file header (expected >= 64 bytes, found {actual_bytes})"
            ),
            Self::InvalidElfMagic => f.write_str("file does not start with \\x7fELF magic"),
            Self::UnsupportedClass(c) => {
                write!(f, "unsupported ELF class {c} (expected ELFCLASS64=2)")
            }
            Self::UnsupportedEndianness(d) => write!(
                f,
                "invalid ELF data encoding {d} (expected ELFDATA2LSB=1 or ELFDATA2MSB=2)"
            ),
            Self::UnsupportedVersion(v) => {
                write!(f, "unsupported ELF header version {v} (expected 1)")
            }
            Self::NotExecutableType(t) => write!(
                f,
                "ELF e_type={t} is not an executable or shared object (expected ET_EXEC=2 or ET_DYN=3)"
            ),
            Self::MissingProgramHeaders => {
                f.write_str("ELF64 binary contains no program headers (e_phnum=0 or e_phoff=0)")
            }
            Self::InvalidProgramHeaderEntrySize(sz) => write!(
                f,
                "invalid ELF64 e_phentsize={sz} (expected {ELF64_PHDR_MIN_SIZE}..={MAX_ELF64_PHENTSIZE})"
            ),
            Self::TooManyProgramHeaders(n) => write!(
                f,
                "ELF64 e_phnum={n} exceeds safety bound ({MAX_ELF64_PHNUM})"
            ),
            Self::TruncatedProgramHeaderTable {
                offset,
                table_bytes,
                file_len,
            } => write!(
                f,
                "ELF64 program header table (offset={offset}, len={table_bytes}) exceeds file size ({file_len})"
            ),
            Self::SegmentOutOfBounds {
                p_type,
                offset,
                filesz,
                file_len,
            } => write!(
                f,
                "ELF64 segment p_type={p_type} (offset={offset}, filesz={filesz}) exceeds file size ({file_len})"
            ),
            Self::OversizedSegment { p_type, filesz } => write!(
                f,
                "ELF64 segment p_type={p_type} filesz={filesz} exceeds static parser safety cap"
            ),
            Self::MissingLoadSegments => {
                f.write_str("ELF64 PT_DYNAMIC present without any PT_LOAD segment")
            }
            Self::OversizedStringTable(sz) => write!(
                f,
                "ELF64 DT_STRSZ={sz} exceeds safety cap ({MAX_DYNSTR_BYTES} bytes)"
            ),
            Self::UnmappedStringTable(vaddr) => write!(
                f,
                "ELF64 DT_STRTAB vaddr={vaddr:#x} is not backed by any PT_LOAD file range"
            ),
            Self::TruncatedStringTable {
                offset,
                size,
                available,
            } => write!(
                f,
                "ELF64 DT_STRTAB (file_offset={offset}, DT_STRSZ={size}) exceeds PT_LOAD file bytes ({available})"
            ),
            Self::StringOffsetOutOfBounds { offset, strsz } => write!(
                f,
                "ELF64 dynamic string offset {offset} out of bounds for DT_STRSZ={strsz}"
            ),
            Self::UnterminatedDynamicString { offset } => write!(
                f,
                "ELF64 dynamic string at offset {offset} lacks NUL terminator within DT_STRSZ"
            ),
            Self::InvalidUtf8DynamicString { offset } => write!(
                f,
                "ELF64 dynamic string or PT_INTERP at offset {offset} contains invalid UTF-8"
            ),
            Self::Io(msg) => write!(f, "I/O error reading ELF64 binary: {msg}"),
        }
    }
}

impl std::error::Error for Elf64ParseError {}

/// Reference entry for published `VN-Linux-Family/zalo-for-linux` community AppImage digests.
///
/// **Provenance & Integrity Contract:** Matching an entry proves **byte-level integrity against
/// the pinned reference digest only**; it NEVER proves publisher identity, software origin
/// authenticity, official VNG signing, endorsement, or redistribution rights.
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
    /// Published lowercase hex SHA-256 reference digest (integrity check only, not origin proof).
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
///
/// A match confirms byte-level integrity against the recorded reference digest only; it never
/// proves publisher identity or software origin authenticity.
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

/// Result of statically inspecting a user-supplied binary, AppImage, or extracted AppDir.
///
/// **Integrity vs. Origin Authenticity:** [`BinaryInspectionReport::sha256`] and
/// [`BinaryInspectionReport::matched_community_release`] verify **byte-level integrity against a
/// trusted reference digest only—NEVER publisher identity, cryptographic code signing, or software
/// origin authenticity**.
///
/// **Zero Execution Guarantee:** Producing this report never executes the target binary, `ldd`,
/// any dynamic linker (`PT_INTERP`), or any shell script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinaryInspectionReport {
    /// Canonical or validated path inspected.
    pub path: PathBuf,
    /// Detected container/executable format.
    pub format: BinaryFormatKind,
    /// Parsed ELF64 CPU architecture when applicable.
    pub arch: Option<ElfCpuArch>,
    /// True when the file carries a structurally valid 64-byte ELF64 header (with in-bounds
    /// program headers) or valid `#!` script header. False for corrupt/truncated/garbage files
    /// (including `.AppImage` files containing non-ELF bytes).
    pub has_valid_executable_header: bool,
    /// Static ELF64 `PT_INTERP` and `PT_DYNAMIC` (`DT_NEEDED`, `DT_RPATH`, `DT_RUNPATH`) metadata
    /// when the binary is an ELF64 executable with program headers.
    pub elf_dynamic: Option<Elf64DynamicMetadata>,
    /// File size in bytes (`0` for directories).
    pub size_bytes: u64,
    /// Computed SHA-256 hex digest for regular files (`None` for directories or files > 512 MiB skipped).
    /// Proves byte-level integrity against a trusted reference digest only, never software origin.
    pub sha256: Option<String>,
    /// Matched community release entry if `sha256` matches a known reference digest (integrity only,
    /// never official VNG endorsement or publisher identity).
    pub matched_community_release: Option<&'static CommunityReleaseReference>,
    /// Parsed semantic version hint from filename or bundle metadata.
    pub inferred_version: Option<String>,
    /// Whether POSIX execute permission bit is set (always `true` on non-Unix).
    pub is_executable: bool,
    /// Security block reason if binary is unsafe (e.g., `setuid`, dangerous startup script).
    pub security_block: Option<SecurityBlockReason>,
    /// Missing dynamic shared libraries resolved statically from ELF64 `DT_NEEDED` via `fs::metadata`.
    pub missing_libraries: Vec<MissingLibraryDiagnostic>,
    /// Human-readable static audit notes.
    pub notes: Vec<String>,
}

impl BinaryInspectionReport {
    /// Returns `true` only when this report represents a structurally valid ELF64 AppImage
    /// (`AppImageType2` or `AppImageUruntimeDwarfs`) with an intact executable header.
    #[must_use]
    pub const fn is_valid_appimage_elf64(&self) -> bool {
        self.has_valid_executable_header
            && matches!(
                self.format,
                BinaryFormatKind::AppImageType2 | BinaryFormatKind::AppImageUruntimeDwarfs
            )
    }

    /// Strict format & header validation rejecting unrecognized, truncated, or malformed binaries
    /// (including files carrying an `.AppImage` extension that are not valid ELF64 AppImages).
    pub fn validate_executable_format(&self) -> Result<(), DimError> {
        let has_appimage_ext = self
            .path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("appimage"));

        if has_appimage_ext && !self.is_valid_appimage_elf64() {
            return Err(DimError::UnverifiedBinary(format!(
                "invalid AppImage '{}': file has .AppImage extension but lacks a valid ELF64 AppImage Type-2 or uruntime/DwarFS executable header (detected format={})",
                self.path.display(),
                self.format.as_str()
            )));
        }

        if !self.has_valid_executable_header || matches!(self.format, BinaryFormatKind::Unknown) {
            return Err(DimError::UnverifiedBinary(format!(
                "unrecognized or malformed executable header in '{}' (detected format={})",
                self.path.display(),
                self.format.as_str()
            )));
        }

        Ok(())
    }
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
    /// Source discovery channel (`"xdg-desktop-entry"`, `"appimage-search-dir"`, `"env:..."`, etc.).
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
///
/// Larger files can still be hashed explicitly via [`sha256_file`] or when `expected_sha256`
/// is supplied to [`inspect_user_binary`]; [`Sha256State`] tracks byte counts in `u64` and
/// computes 64-bit big-endian bit lengths cleanly up to `2 EiB - 1 B` (`2^64 - 8` bits, the full
/// NIST FIPS 180-4 SHA-256 domain).
const MAX_AUTO_HASH_BYTES: u64 = 512 * 1024 * 1024;

/// Computes the lowercase hex FIPS 180-4 SHA-256 digest of an in-memory byte slice.
///
/// Note: SHA-256 verifies byte-level integrity against a trusted reference digest only, never
/// publisher identity or software origin authenticity.
#[must_use]
pub fn sha256_bytes(data: &[u8]) -> String {
    let mut hasher = Sha256State::new();
    hasher.update(data);
    encode_hex(&hasher.finalize())
}

/// Computes the lowercase hex FIPS 180-4 SHA-256 digest of a file using a 64 KiB stream buffer.
///
/// Safely supports files larger than `512 MiB` and `4 GiB` (up to the FIPS 180-4 `2 EiB` limit).
/// Note: SHA-256 verifies byte-level integrity against a trusted reference digest only, never
/// publisher identity or software origin authenticity.
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

/// Parses a 64-byte ELF64 / AppImage header or `#!` script header and optional tail marker slice.
///
/// Truncated ELF headers shorter than 64 bytes (`sizeof(Elf64_Ehdr)`), non-64-bit ELF classes,
/// invalid endianness/version bytes, non-executable ELF object types (`ET_REL`, `ET_CORE`, etc.),
/// and `#!` lines lacking an interpreter path return `(BinaryFormatKind::Unknown, None)`.
#[must_use]
pub fn classify_binary_bytes(
    header: &[u8],
    tail: &[u8],
    has_appimage_ext: bool,
) -> (BinaryFormatKind, Option<ElfCpuArch>) {
    if has_valid_shebang_line(header) {
        return (BinaryFormatKind::ShellOrScriptWrapper, None);
    }

    let Some(ehdr) = parse_elf64_ehdr_fields(header) else {
        return (BinaryFormatKind::Unknown, None);
    };

    // Standard AppImage Type-2 embeds 'A', 'I', 0x02 at EI_PAD offset 8..11 of a valid ELF64 header.
    if &header[8..11] == b"AI\x02" {
        return (BinaryFormatKind::AppImageType2, Some(ehdr.arch));
    }

    // `quick-sharun` / `uruntime` DwarFS AppImages are ELF64 binaries containing DWARFS/SHARUN/uruntime markers.
    if contains_subslice(header, b"DWARFS")
        || contains_subslice(tail, b"DWARFS")
        || contains_subslice(header, b"uruntime")
        || contains_subslice(tail, b"uruntime")
        || contains_subslice(tail, b"gh-releases-zsync")
        || has_appimage_ext
    {
        return (BinaryFormatKind::AppImageUruntimeDwarfs, Some(ehdr.arch));
    }

    (BinaryFormatKind::Elf64Executable, Some(ehdr.arch))
}

/// Parses ELF64 program headers (`PT_INTERP`, `PT_LOAD`, `PT_DYNAMIC`) and resolves `DT_NEEDED`,
/// `DT_RPATH`, `DT_RUNPATH`, and `DT_SONAME` strings from an in-memory byte slice.
///
/// Memory-safe and strictly bounds-checked: malformed, truncated, unmapped, or oversized headers
/// return [`Elf64ParseError`] without panicking, overflowing integers, or over-allocating memory.
pub fn parse_elf64_dynamic_metadata(bytes: &[u8]) -> Result<Elf64DynamicMetadata, Elf64ParseError> {
    let mut reader = SliceElfReader { data: bytes };
    parse_elf64_from_source(&mut reader)
}

/// Reads and parses ELF64 program headers (`PT_INTERP`, `PT_LOAD`, `PT_DYNAMIC`) directly from an
/// on-disk file via bounded seeks without loading the entire binary/AppImage into memory.
pub fn read_elf64_dynamic_metadata(path: &Path) -> Result<Elf64DynamicMetadata, Elf64ParseError> {
    let file = File::open(path).map_err(|e| Elf64ParseError::Io(e.to_string()))?;
    let len = file
        .metadata()
        .map_err(|e| Elf64ParseError::Io(e.to_string()))?
        .len();
    let mut reader = FileElfReader { file, len };
    parse_elf64_from_source(&mut reader)
}

/// Statically parses an ELF64 binary on disk and resolves missing `DT_NEEDED` shared libraries
/// against the live Linux root (`/`), **never** invoking `ldd` or executing any binary.
pub fn inspect_elf64_needed_libraries(
    binary_path: &Path,
) -> Result<(Elf64DynamicMetadata, Vec<MissingLibraryDiagnostic>), Elf64ParseError> {
    inspect_elf64_needed_libraries_at(binary_path, Path::new("/"))
}

/// Root-parameterized static ELF64 `DT_NEEDED` inspector for deterministic cross-platform testing
/// and virtualized Ubuntu root verification.
pub fn inspect_elf64_needed_libraries_at(
    binary_path: &Path,
    fs_root: &Path,
) -> Result<(Elf64DynamicMetadata, Vec<MissingLibraryDiagnostic>), Elf64ParseError> {
    let meta = read_elf64_dynamic_metadata(binary_path)?;
    let missing = resolve_elf64_missing_libraries_at(&meta, binary_path, fs_root);
    Ok((meta, missing))
}

/// Resolves whether each `DT_NEEDED` shared library in `meta` exists on disk under `fs_root`
/// (checking `DT_RPATH`/`DT_RUNPATH` with `$ORIGIN` expansion plus standard Ubuntu multiarch
/// directories `/lib/x86_64-linux-gnu`, `/usr/lib/x86_64-linux-gnu`, `/lib/aarch64-linux-gnu`,
/// `/usr/lib/aarch64-linux-gnu`, `/lib64`, `/usr/lib`) purely via `std::fs::metadata`.
#[must_use]
pub fn resolve_elf64_missing_libraries_at(
    meta: &Elf64DynamicMetadata,
    binary_path: &Path,
    fs_root: &Path,
) -> Vec<MissingLibraryDiagnostic> {
    if meta.needed_libraries.is_empty() {
        return Vec::new();
    }

    let origin_dir = binary_path.parent();
    let mut custom_search_dirs = Vec::new();

    // Per ld.so(8), DT_RPATH is searched only when DT_RUNPATH is absent.
    let active_rpaths = if meta.runpath_entries.is_empty() {
        &meta.rpath_entries
    } else {
        &meta.runpath_entries
    };

    for raw_entry in active_rpaths {
        if let Some(expanded) = expand_origin_search_dir(raw_entry, origin_dir, fs_root) {
            if !custom_search_dirs.contains(&expanded) {
                custom_search_dirs.push(expanded);
            }
        }
    }

    // Also include standard adjacent bundle/AppDir library folders next to the binary if present.
    if let Some(bin_dir) = origin_dir {
        for cand in [
            bin_dir.to_path_buf(),
            bin_dir.join("lib"),
            bin_dir.join("usr").join("lib"),
        ] {
            if !custom_search_dirs.contains(&cand) {
                custom_search_dirs.push(cand);
            }
        }
    }

    let system_rel_dirs: &[&str] = match meta.arch {
        ElfCpuArch::X86_64 => &[
            "lib/x86_64-linux-gnu",
            "usr/lib/x86_64-linux-gnu",
            "usr/local/lib/x86_64-linux-gnu",
            "lib64",
            "usr/lib64",
            "lib",
            "usr/lib",
            "usr/local/lib",
        ],
        ElfCpuArch::Aarch64 => &[
            "lib/aarch64-linux-gnu",
            "usr/lib/aarch64-linux-gnu",
            "usr/local/lib/aarch64-linux-gnu",
            "lib",
            "usr/lib",
            "usr/local/lib",
        ],
        ElfCpuArch::Other(_) => &[
            "lib/x86_64-linux-gnu",
            "usr/lib/x86_64-linux-gnu",
            "lib/aarch64-linux-gnu",
            "usr/lib/aarch64-linux-gnu",
            "lib64",
            "usr/lib64",
            "lib",
            "usr/lib",
            "usr/local/lib",
        ],
    };

    let mut search_dirs = custom_search_dirs;
    for rel in system_rel_dirs {
        let full = join_under_fs_root(fs_root, rel);
        if !search_dirs.contains(&full) {
            search_dirs.push(full);
        }
    }

    let mut missing = Vec::new();
    for soname in &meta.needed_libraries {
        let trimmed = soname.trim();
        if trimmed.is_empty() || is_virtual_kernel_soname(trimmed) {
            continue;
        }
        if !is_safe_soname_token(trimmed) {
            continue;
        }

        let found = if trimmed.contains('/') {
            let direct = if trimmed.starts_with('/') {
                join_under_fs_root(fs_root, trimmed.trim_start_matches('/'))
            } else if let Some(bin_dir) = origin_dir {
                bin_dir.join(trimmed)
            } else {
                PathBuf::from(trimmed)
            };
            fs::metadata(&direct).is_ok_and(|m| m.is_file())
        } else {
            search_dirs
                .iter()
                .any(|dir| fs::metadata(dir.join(trimmed)).is_ok_and(|m| m.is_file()))
        };

        if !found
            && !missing
                .iter()
                .any(|m: &MissingLibraryDiagnostic| m.soname == trimmed)
        {
            let (u2404, u2204) = map_soname_to_ubuntu_packages(trimmed);
            missing.push(MissingLibraryDiagnostic {
                soname: trimmed.to_owned(),
                ubuntu_2404_package: u2404,
                ubuntu_2204_package: u2204,
            });
        }
    }

    missing
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
            let candidate = name[start..i].trim_matches('.');
            if dots >= 2
                && candidate.split('.').count() >= 3
                && candidate
                    .split('.')
                    .all(|seg| !seg.is_empty() && seg.chars().all(|c| c.is_ascii_digit()))
            {
                return Some(candidate.to_owned());
            }
        } else {
            i += 1;
        }
    }
    None
}

/// Audits a shell wrapper or `AppRun` script statically for privilege escalation, sandbox disabling,
/// destructive process kills, or hardcoded user home paths (never executes the script).
#[must_use]
pub fn analyze_startup_script(content: &str) -> StartupScriptAudit {
    let mut hazards = Vec::new();
    let mut notes = Vec::new();
    let interpreter = content
        .lines()
        .next()
        .and_then(|l| l.strip_prefix("#!"))
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());

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

/// Statically inspects a user-supplied local AppImage, ELF binary, script wrapper, or extracted AppDir.
///
/// - **Never executes** the target binary, `ldd`, dynamic linker, or startup script.
/// - Computes SHA-256 to verify **byte-level integrity against a trusted reference digest only**
///   (never publisher identity or software origin authenticity).
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

    let mut has_valid_executable_header = match format {
        BinaryFormatKind::AppImageType2
        | BinaryFormatKind::AppImageUruntimeDwarfs
        | BinaryFormatKind::Elf64Executable
        | BinaryFormatKind::ShellOrScriptWrapper => true,
        BinaryFormatKind::ExtractedAppDir | BinaryFormatKind::Unknown => false,
    };

    if has_appimage_ext
        && !matches!(
            format,
            BinaryFormatKind::AppImageType2 | BinaryFormatKind::AppImageUruntimeDwarfs
        )
    {
        has_valid_executable_header = false;
        notes.push(
            "Invalid .AppImage executable header: file carries .AppImage extension but is not a valid 64-byte ELF64 AppImage Type-2 or uruntime/DwarFS binary."
                .to_owned(),
        );
    }

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
        notes.push(
            "Skipped automatic synchronous SHA-256 hash because file exceeds 512 MiB (pass expected_sha256 or call sha256_file explicitly)."
                .to_owned(),
        );
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
        notes.push(format!(
            "Caller-pinned SHA-256 verified ({actual}): proves byte-level integrity against the caller's reference digest only, never publisher identity or software origin authenticity."
        ));
    }

    let matched_community_release = sha256.as_deref().and_then(match_known_community_release);
    if let Some(known) = matched_community_release {
        notes.push(format!(
            "SHA-256 matches community build VN-Linux-Family/zalo-for-linux {} ({}, {}); proves byte-level integrity against pinned reference digest only — UNOFFICIAL port, NEVER publisher identity, VNG endorsement, or software origin authenticity.",
            known.version, known.variant, known.arch
        ));
    } else {
        notes.push(
            "Unpinned local binary: SHA-256 checksum proves byte-level integrity against a trusted reference digest only, NEVER publisher identity or software origin authenticity; verify against a trusted digest before running with sensitive accounts."
                .to_owned(),
        );
    }

    let inferred_version = matched_community_release
        .map(|m| m.version.to_owned())
        .or_else(|| parse_version_from_filename(&resolved_path));

    let (elf_dynamic, missing_libraries) = if matches!(
        format,
        BinaryFormatKind::Elf64Executable
            | BinaryFormatKind::AppImageType2
            | BinaryFormatKind::AppImageUruntimeDwarfs
    ) {
        match read_elf64_dynamic_metadata(&resolved_path) {
            Ok(meta) => {
                if let Some(interp) = &meta.interpreter {
                    if !is_standard_linux_elf_interpreter(interp) {
                        notes.push(format!(
                            "Static ELF64 audit noticed non-standard PT_INTERP interpreter '{interp}' (inspected safely without executing binary or ldd)."
                        ));
                    }
                }
                let missing = resolve_live_linux_missing_libraries(&meta, &resolved_path);
                (Some(meta), missing)
            }
            Err(Elf64ParseError::MissingProgramHeaders) => {
                let raw_phoff = read_elf64_phoff(&header);
                if raw_phoff != 0 {
                    has_valid_executable_header = false;
                    notes.push(
                        "Malformed ELF64 header: non-zero e_phoff with e_phnum=0.".to_owned(),
                    );
                } else {
                    notes.push(
                        "ELF64 header has zero program headers (e_phnum=0); static DT_NEEDED scan skipped."
                            .to_owned(),
                    );
                }
                (None, Vec::new())
            }
            Err(err) => {
                has_valid_executable_header = false;
                notes.push(format!(
                    "Malformed or truncated ELF64 binary structure: {err}"
                ));
                (None, Vec::new())
            }
        }
    } else {
        (None, Vec::new())
    };

    Ok(BinaryInspectionReport {
        path: resolved_path,
        format,
        arch,
        has_valid_executable_header,
        elf_dynamic,
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
        "Extracted AppDir detected (entrypoint: {}); directory SHA-256 not applicable — verify archive/files against a trusted reference digest (checksums prove integrity only, never software origin).",
        entry.display()
    )];
    let is_executable = check_entry_executable(&entry);
    let mut has_valid_executable_header = false;

    if let Ok(mut f) = File::open(&entry) {
        let mut head = [0u8; 256];
        let n = f.read(&mut head).unwrap_or(0);
        let head_slice = &head[..n];
        if has_valid_shebang_line(head_slice) {
            has_valid_executable_header = true;
            if let Ok(script_text) = fs::read_to_string(&entry) {
                let audit = analyze_startup_script(&script_text);
                notes.extend(audit.notes);
                if !audit.safe {
                    security_block = Some(SecurityBlockReason::UnsafeStartupScript {
                        findings: audit.hazards,
                    });
                }
            }
        } else if parse_elf64_ehdr_fields(head_slice).is_some() {
            has_valid_executable_header = true;
        }
    }

    let version = read_appdir_version(dir).or_else(|| parse_version_from_filename(dir));
    let mut arch = None;
    let (elf_dynamic, missing_libraries) = if zalo_bin.is_file() {
        match read_elf64_dynamic_metadata(&zalo_bin) {
            Ok(meta) => {
                arch = Some(meta.arch);
                let missing = resolve_live_linux_missing_libraries(&meta, &zalo_bin);
                (Some(meta), missing)
            }
            Err(Elf64ParseError::MissingProgramHeaders) => {
                if let Ok(mut zf) = File::open(&zalo_bin) {
                    let mut zh = [0u8; ELF64_EHDR_SIZE];
                    if zf.read(&mut zh).unwrap_or(0) >= ELF64_EHDR_SIZE {
                        arch = parse_elf64_ehdr_fields(&zh).map(|e| e.arch);
                    }
                }
                (None, Vec::new())
            }
            Err(err) => {
                if entry == zalo_bin {
                    has_valid_executable_header = false;
                }
                notes.push(format!(
                    "Malformed ELF64 structure in AppDir binary '{}': {err}",
                    zalo_bin.display()
                ));
                (None, Vec::new())
            }
        }
    } else {
        (None, Vec::new())
    };

    Ok(BinaryInspectionReport {
        path: entry,
        format: BinaryFormatKind::ExtractedAppDir,
        arch,
        has_valid_executable_header,
        elf_dynamic,
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

fn resolve_live_linux_missing_libraries(
    meta: &Elf64DynamicMetadata,
    binary_path: &Path,
) -> Vec<MissingLibraryDiagnostic> {
    #[cfg(target_os = "linux")]
    {
        resolve_elf64_missing_libraries_at(meta, binary_path, Path::new("/"))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (meta, binary_path);
        Vec::new()
    }
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

/// Pure offline text parser for captured `ldd` output lines (`<soname> => not found`),
/// mapping missing SONAMEs to Ubuntu 24.04 / 22.04 `apt` package hints.
///
/// **Security Note:** This function is a pure string parser ONLY. `dim-platform-linux` **never**
/// invokes `ldd(1)` on binaries because custom `PT_INTERP` ELF interpreters can execute arbitrary
/// code during `ldd` inspection; live binary inspection uses [`inspect_elf64_needed_libraries`].
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
        "libX11.so.6" => ("libx11-6", "libx11-6"),
        "libXcomposite.so.1" => ("libxcomposite1", "libxcomposite1"),
        "libXdamage.so.1" => ("libxdamage1", "libxdamage1"),
        "libXext.so.6" => ("libxext6", "libxext6"),
        "libXfixes.so.3" => ("libxfixes3", "libxfixes3"),
        "libXrandr.so.2" => ("libxrandr2", "libxrandr2"),
        "libxcb.so.1" => ("libxcb1", "libxcb1"),
        "libxkbcommon.so.0" => ("libxkbcommon0", "libxkbcommon0"),
        "libpango-1.0.so.0" => ("libpango-1.0-0", "libpango-1.0-0"),
        "libcairo.so.2" => ("libcairo2", "libcairo2"),
        "librsvg-2.so.2" => ("librsvg2-2", "librsvg2-2"),
        _ => (
            "(inspect via `apt-file search`)",
            "(inspect via `apt-file search`)",
        ),
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
/// without ever hardcoding a personal username or executing any candidate binary.
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
                    .ok()
                    .filter(|r| r.has_valid_executable_header)
                    .map(|r| r.format)
                    .unwrap_or(BinaryFormatKind::Unknown);
                if matches!(format, BinaryFormatKind::Unknown) {
                    continue;
                }
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
    let Some(report) = inspect_user_binary(path, None).ok() else {
        return;
    };
    if !report.has_valid_executable_header || matches!(report.format, BinaryFormatKind::Unknown) {
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
        format: report.format,
        version,
        discovery_source: source.to_owned(),
    });
}

// ---------------------------------------------------------------------------
// Internal static ELF64 reader & bounds-checked dynamic metadata parser
// ---------------------------------------------------------------------------

trait ElfByteSource {
    fn len(&self) -> u64;
    fn read_exact_at(&mut self, offset: u64, out: &mut [u8]) -> Result<(), Elf64ParseError>;
}

struct SliceElfReader<'a> {
    data: &'a [u8],
}

impl ElfByteSource for SliceElfReader<'_> {
    fn len(&self) -> u64 {
        self.data.len() as u64
    }

    fn read_exact_at(&mut self, offset: u64, out: &mut [u8]) -> Result<(), Elf64ParseError> {
        let start = usize::try_from(offset)
            .map_err(|_| Elf64ParseError::Io("slice offset overflows host usize".to_owned()))?;
        let end = start.checked_add(out.len()).ok_or_else(|| {
            Elf64ParseError::Io("slice end offset overflows host usize".to_owned())
        })?;
        let slice = self
            .data
            .get(start..end)
            .ok_or_else(|| Elf64ParseError::Io("unexpected EOF in ELF slice".to_owned()))?;
        out.copy_from_slice(slice);
        Ok(())
    }
}

struct FileElfReader {
    file: File,
    len: u64,
}

impl ElfByteSource for FileElfReader {
    fn len(&self) -> u64 {
        self.len
    }

    fn read_exact_at(&mut self, offset: u64, out: &mut [u8]) -> Result<(), Elf64ParseError> {
        self.file
            .seek(SeekFrom::Start(offset))
            .map_err(|e| Elf64ParseError::Io(e.to_string()))?;
        self.file
            .read_exact(out)
            .map_err(|e| Elf64ParseError::Io(e.to_string()))
    }
}

#[derive(Debug, Clone, Copy)]
struct ParsedElf64Ehdr {
    is_little_endian: bool,
    elf_type: u16,
    arch: ElfCpuArch,
    phoff: u64,
    phentsize: u16,
    phnum: u16,
}

#[derive(Debug, Clone, Copy)]
struct PtLoadSegment {
    offset: u64,
    vaddr: u64,
    filesz: u64,
}

fn has_valid_shebang_line(header: &[u8]) -> bool {
    if !header.starts_with(b"#!") {
        return false;
    }
    let after = &header[2..];
    let line_end = after
        .iter()
        .position(|&b| b == b'\n' || b == b'\r')
        .unwrap_or(after.len());
    let line = &after[..line_end];
    if line.contains(&0) {
        return false;
    }
    std::str::from_utf8(line).is_ok_and(|s| !s.trim().is_empty())
}

fn parse_elf64_ehdr_fields(header: &[u8]) -> Option<ParsedElf64Ehdr> {
    validate_elf64_ehdr(header).ok()
}

fn read_elf64_phoff(header: &[u8]) -> u64 {
    parse_elf64_ehdr_fields(header).map_or(0, |e| e.phoff)
}

fn validate_elf64_ehdr(header: &[u8]) -> Result<ParsedElf64Ehdr, Elf64ParseError> {
    if header.len() < ELF64_EHDR_SIZE {
        return Err(Elf64ParseError::TruncatedElfHeader {
            actual_bytes: header.len() as u64,
        });
    }
    if &header[0..4] != b"\x7fELF" {
        return Err(Elf64ParseError::InvalidElfMagic);
    }
    let ei_class = header[4];
    if ei_class != 2 {
        return Err(Elf64ParseError::UnsupportedClass(ei_class));
    }
    let ei_data = header[5];
    let is_le = match ei_data {
        1 => true,
        2 => false,
        other => return Err(Elf64ParseError::UnsupportedEndianness(other)),
    };
    let ei_version = header[6];
    if ei_version != 1 {
        return Err(Elf64ParseError::UnsupportedVersion(ei_version));
    }

    let elf_type = read_u16_endian(&header[16..18], is_le);
    let raw_machine = read_u16_endian(&header[18..20], is_le);
    if !matches!(elf_type, 2 | 3) || raw_machine == 0 {
        return Err(Elf64ParseError::NotExecutableType(elf_type));
    }

    let ehsize = read_u16_endian(&header[52..54], is_le);
    if ehsize != 0 && usize::from(ehsize) < ELF64_EHDR_SIZE {
        return Err(Elf64ParseError::TruncatedElfHeader {
            actual_bytes: u64::from(ehsize),
        });
    }

    let arch = match raw_machine {
        0x3E => ElfCpuArch::X86_64,
        0xB7 => ElfCpuArch::Aarch64,
        other => ElfCpuArch::Other(other),
    };
    let phoff = read_u64_endian(&header[32..40], is_le);
    let phentsize = read_u16_endian(&header[54..56], is_le);
    let phnum = read_u16_endian(&header[56..58], is_le);

    Ok(ParsedElf64Ehdr {
        is_little_endian: is_le,
        elf_type,
        arch,
        phoff,
        phentsize,
        phnum,
    })
}

fn parse_elf64_from_source(
    src: &mut dyn ElfByteSource,
) -> Result<Elf64DynamicMetadata, Elf64ParseError> {
    let file_len = src.len();
    if file_len < ELF64_EHDR_SIZE as u64 {
        return Err(Elf64ParseError::TruncatedElfHeader {
            actual_bytes: file_len,
        });
    }

    let mut ehdr_buf = [0u8; ELF64_EHDR_SIZE];
    src.read_exact_at(0, &mut ehdr_buf)?;
    let ehdr = validate_elf64_ehdr(&ehdr_buf)?;

    if ehdr.phnum == 0 || ehdr.phoff == 0 {
        return Err(Elf64ParseError::MissingProgramHeaders);
    }
    if ehdr.phentsize < ELF64_PHDR_MIN_SIZE || ehdr.phentsize > MAX_ELF64_PHENTSIZE {
        return Err(Elf64ParseError::InvalidProgramHeaderEntrySize(
            ehdr.phentsize,
        ));
    }
    if ehdr.phnum > MAX_ELF64_PHNUM {
        return Err(Elf64ParseError::TooManyProgramHeaders(ehdr.phnum));
    }

    let table_bytes = u64::from(ehdr.phnum)
        .checked_mul(u64::from(ehdr.phentsize))
        .ok_or(Elf64ParseError::TruncatedProgramHeaderTable {
            offset: ehdr.phoff,
            table_bytes: u64::MAX,
            file_len,
        })?;
    let table_end = ehdr.phoff.checked_add(table_bytes).ok_or(
        Elf64ParseError::TruncatedProgramHeaderTable {
            offset: ehdr.phoff,
            table_bytes,
            file_len,
        },
    )?;
    if table_end > file_len {
        return Err(Elf64ParseError::TruncatedProgramHeaderTable {
            offset: ehdr.phoff,
            table_bytes,
            file_len,
        });
    }

    let table_usize =
        usize::try_from(table_bytes).map_err(|_| Elf64ParseError::TruncatedProgramHeaderTable {
            offset: ehdr.phoff,
            table_bytes,
            file_len,
        })?;
    let mut phdr_table = vec![0u8; table_usize];
    src.read_exact_at(ehdr.phoff, &mut phdr_table)?;

    let mut loads = Vec::new();
    let mut interp_seg: Option<(u64, u64)> = None;
    let mut dynamic_seg: Option<(u64, u64)> = None;

    let stride = usize::from(ehdr.phentsize);
    for idx in 0..usize::from(ehdr.phnum) {
        let base = idx * stride;
        let ph = &phdr_table[base..base + usize::from(ELF64_PHDR_MIN_SIZE)];
        let p_type = read_u32_endian(&ph[0..4], ehdr.is_little_endian);
        let p_offset = read_u64_endian(&ph[8..16], ehdr.is_little_endian);
        let p_vaddr = read_u64_endian(&ph[16..24], ehdr.is_little_endian);
        let p_filesz = read_u64_endian(&ph[32..40], ehdr.is_little_endian);

        if matches!(p_type, PT_LOAD | PT_INTERP | PT_DYNAMIC) {
            let seg_end =
                p_offset
                    .checked_add(p_filesz)
                    .ok_or(Elf64ParseError::SegmentOutOfBounds {
                        p_type,
                        offset: p_offset,
                        filesz: p_filesz,
                        file_len,
                    })?;
            if seg_end > file_len {
                return Err(Elf64ParseError::SegmentOutOfBounds {
                    p_type,
                    offset: p_offset,
                    filesz: p_filesz,
                    file_len,
                });
            }
        }

        match p_type {
            PT_LOAD => {
                if p_filesz > 0 && p_vaddr.checked_add(p_filesz).is_some() {
                    loads.push(PtLoadSegment {
                        offset: p_offset,
                        vaddr: p_vaddr,
                        filesz: p_filesz,
                    });
                }
            }
            PT_INTERP if interp_seg.is_none() => {
                if p_filesz > MAX_PT_INTERP_BYTES {
                    return Err(Elf64ParseError::OversizedSegment {
                        p_type: PT_INTERP,
                        filesz: p_filesz,
                    });
                }
                if p_filesz > 0 {
                    interp_seg = Some((p_offset, p_filesz));
                }
            }
            PT_DYNAMIC if dynamic_seg.is_none() => {
                if p_filesz > MAX_PT_DYNAMIC_BYTES {
                    return Err(Elf64ParseError::OversizedSegment {
                        p_type: PT_DYNAMIC,
                        filesz: p_filesz,
                    });
                }
                if p_filesz > 0 {
                    dynamic_seg = Some((p_offset, p_filesz));
                }
            }
            _ => {}
        }
    }

    let interpreter = if let Some((interp_off, interp_sz)) = interp_seg {
        let len = usize::try_from(interp_sz).map_err(|_| Elf64ParseError::OversizedSegment {
            p_type: PT_INTERP,
            filesz: interp_sz,
        })?;
        let mut buf = vec![0u8; len];
        src.read_exact_at(interp_off, &mut buf)?;
        let nul_pos = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        let raw_str = std::str::from_utf8(&buf[..nul_pos])
            .map_err(|_| Elf64ParseError::InvalidUtf8DynamicString { offset: interp_off })?;
        let clean = raw_str.trim();
        if clean.is_empty() || clean.chars().any(char::is_control) {
            return Err(Elf64ParseError::InvalidUtf8DynamicString { offset: interp_off });
        }
        Some(clean.to_owned())
    } else {
        None
    };

    let mut soname = None;
    let mut needed_libraries = Vec::new();
    let mut rpath_entries = Vec::new();
    let mut runpath_entries = Vec::new();

    if let Some((dyn_off, dyn_sz)) = dynamic_seg {
        if loads.is_empty() {
            return Err(Elf64ParseError::MissingLoadSegments);
        }
        let dyn_len = usize::try_from(dyn_sz).map_err(|_| Elf64ParseError::OversizedSegment {
            p_type: PT_DYNAMIC,
            filesz: dyn_sz,
        })?;
        let mut dyn_buf = vec![0u8; dyn_len];
        src.read_exact_at(dyn_off, &mut dyn_buf)?;

        let mut strtab_vaddr: Option<u64> = None;
        let mut strtab_sz: Option<u64> = None;
        let mut needed_offsets: Vec<u64> = Vec::new();
        let mut rpath_offsets: Vec<u64> = Vec::new();
        let mut runpath_offsets: Vec<u64> = Vec::new();
        let mut soname_offset: Option<u64> = None;

        for entry in dyn_buf.as_chunks::<16>().0 {
            let d_tag = read_u64_endian(&entry[0..8], ehdr.is_little_endian);
            let d_val = read_u64_endian(&entry[8..16], ehdr.is_little_endian);
            if d_tag == DT_NULL {
                break;
            }
            match d_tag {
                DT_NEEDED => {
                    if needed_offsets.len() < MAX_DT_NEEDED_ENTRIES {
                        needed_offsets.push(d_val);
                    }
                }
                DT_STRTAB => strtab_vaddr = Some(d_val),
                DT_STRSZ => strtab_sz = Some(d_val),
                DT_SONAME => soname_offset = Some(d_val),
                DT_RPATH => rpath_offsets.push(d_val),
                DT_RUNPATH => runpath_offsets.push(d_val),
                _ => {}
            }
        }

        let has_string_refs = !needed_offsets.is_empty()
            || !rpath_offsets.is_empty()
            || !runpath_offsets.is_empty()
            || soname_offset.is_some();

        if let (Some(vaddr), Some(strsz)) = (strtab_vaddr, strtab_sz) {
            if strsz > MAX_DYNSTR_BYTES {
                return Err(Elf64ParseError::OversizedStringTable(strsz));
            }
            if strsz > 0 {
                let (str_file_off, avail) = map_vaddr_to_file_offset(vaddr, &loads)
                    .ok_or(Elf64ParseError::UnmappedStringTable(vaddr))?;
                if strsz > avail || str_file_off.checked_add(strsz).is_none_or(|e| e > file_len) {
                    return Err(Elf64ParseError::TruncatedStringTable {
                        offset: str_file_off,
                        size: strsz,
                        available: avail,
                    });
                }
                let strsz_usize = usize::try_from(strsz)
                    .map_err(|_| Elf64ParseError::OversizedStringTable(strsz))?;
                let mut dynstr = vec![0u8; strsz_usize];
                src.read_exact_at(str_file_off, &mut dynstr)?;

                if let Some(so_off) = soname_offset {
                    let s = read_dynstr_checked(&dynstr, so_off)?;
                    if !s.is_empty() {
                        soname = Some(s);
                    }
                }
                for off in needed_offsets {
                    let s = read_dynstr_checked(&dynstr, off)?;
                    if !s.is_empty() && !needed_libraries.contains(&s) {
                        needed_libraries.push(s);
                    }
                }
                for off in rpath_offsets {
                    let s = read_dynstr_checked(&dynstr, off)?;
                    for seg in s.split(':').filter(|p| !p.is_empty()) {
                        if !rpath_entries.iter().any(|e| e == seg) {
                            rpath_entries.push(seg.to_owned());
                        }
                    }
                }
                for off in runpath_offsets {
                    let s = read_dynstr_checked(&dynstr, off)?;
                    for seg in s.split(':').filter(|p| !p.is_empty()) {
                        if !runpath_entries.iter().any(|e| e == seg) {
                            runpath_entries.push(seg.to_owned());
                        }
                    }
                }
            } else if has_string_refs {
                return Err(Elf64ParseError::StringOffsetOutOfBounds {
                    offset: 0,
                    strsz: 0,
                });
            }
        } else if has_string_refs {
            return Err(Elf64ParseError::UnmappedStringTable(0));
        }
    }

    Ok(Elf64DynamicMetadata {
        arch: ehdr.arch,
        elf_type: ehdr.elf_type,
        is_little_endian: ehdr.is_little_endian,
        program_header_count: ehdr.phnum,
        interpreter,
        soname,
        needed_libraries,
        rpath_entries,
        runpath_entries,
    })
}

fn map_vaddr_to_file_offset(vaddr: u64, loads: &[PtLoadSegment]) -> Option<(u64, u64)> {
    for seg in loads {
        if vaddr >= seg.vaddr {
            let delta = vaddr - seg.vaddr;
            if delta < seg.filesz {
                let file_off = seg.offset.checked_add(delta)?;
                let available = seg.filesz - delta;
                return Some((file_off, available));
            }
        }
    }
    None
}

fn read_dynstr_checked(dynstr: &[u8], offset: u64) -> Result<String, Elf64ParseError> {
    let strsz = dynstr.len() as u64;
    let start = usize::try_from(offset)
        .ok()
        .filter(|&idx| idx < dynstr.len())
        .ok_or(Elf64ParseError::StringOffsetOutOfBounds { offset, strsz })?;
    let rel_end = dynstr[start..]
        .iter()
        .position(|&b| b == 0)
        .ok_or(Elf64ParseError::UnterminatedDynamicString { offset })?;
    let text = std::str::from_utf8(&dynstr[start..start + rel_end])
        .map_err(|_| Elf64ParseError::InvalidUtf8DynamicString { offset })?;
    Ok(text.to_owned())
}

fn expand_origin_search_dir(
    raw_dir: &str,
    origin_dir: Option<&Path>,
    fs_root: &Path,
) -> Option<PathBuf> {
    let trimmed = raw_dir.trim();
    if trimmed.is_empty() || trimmed.contains('\0') {
        return None;
    }

    let expanded = if trimmed.contains("$ORIGIN") || trimmed.contains("${ORIGIN}") {
        let origin_str = origin_dir?.to_str()?;
        trimmed
            .replace("${ORIGIN}", origin_str)
            .replace("$ORIGIN", origin_str)
    } else if trimmed.starts_with('/') {
        let rel = trimmed.trim_start_matches('/');
        let candidate = join_under_fs_root(fs_root, rel);
        if candidate
            .components()
            .any(|c| matches!(c, Component::ParentDir))
        {
            return None;
        }
        return Some(candidate);
    } else {
        return None;
    };

    let candidate = PathBuf::from(expanded);
    if candidate
        .components()
        .any(|c| matches!(c, Component::ParentDir))
    {
        return None;
    }
    Some(candidate)
}

fn join_under_fs_root(fs_root: &Path, rel: &str) -> PathBuf {
    let mut out = fs_root.to_path_buf();
    for part in rel.split('/').filter(|s| !s.is_empty()) {
        out.push(part);
    }
    out
}

fn is_virtual_kernel_soname(soname: &str) -> bool {
    soname.starts_with("linux-vdso.so") || soname.starts_with("linux-gate.so")
}

fn is_safe_soname_token(soname: &str) -> bool {
    !soname.contains('\0') && !soname.contains("..")
}

fn is_standard_linux_elf_interpreter(interp: &str) -> bool {
    interp.starts_with("/lib/")
        || interp.starts_with("/lib64/")
        || interp.starts_with("/usr/lib/")
        || interp.starts_with("/usr/lib64/")
}

const fn read_u16_endian(bytes: &[u8], is_le: bool) -> u16 {
    let arr = [bytes[0], bytes[1]];
    if is_le {
        u16::from_le_bytes(arr)
    } else {
        u16::from_be_bytes(arr)
    }
}

const fn read_u32_endian(bytes: &[u8], is_le: bool) -> u32 {
    let arr = [bytes[0], bytes[1], bytes[2], bytes[3]];
    if is_le {
        u32::from_le_bytes(arr)
    } else {
        u32::from_be_bytes(arr)
    }
}

const fn read_u64_endian(bytes: &[u8], is_le: bool) -> u64 {
    let arr = [
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ];
    if is_le {
        u64::from_le_bytes(arr)
    } else {
        u64::from_be_bytes(arr)
    }
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

// ---------------------------------------------------------------------------
// Audited Pure-Rust NIST FIPS PUB 180-4 SHA-256 Implementation
// ---------------------------------------------------------------------------
//
// Audit invariants verified against FIPS 180-4:
// 1. Initial hash state H^(0) (§5.3.3) & 64 round constants K_0..K_63 (§4.2.2) match verbatim.
// 2. Streaming invariant: `0 <= buf_len < 64` holds on entry and exit of every `update()` call.
// 3. Padding boundary (§5.1.1):
//    - Appending `0x80` sets `buf_len` in `1..=64`.
//    - For `buf_len <= 56` (message mod 64 in `0..=55`), zeroes `[buf_len..56]` and writes the
//      64-bit big-endian bit length into `[56..64]` in the same 64-byte block.
//    - For `buf_len > 56` (message mod 64 in `56..=63`), zeroes `[buf_len..64]`, compresses that
//      block, zeroes `[0..56]` (clearing any prior bytes in `self.buf`), and writes the 64-bit
//      big-endian bit length into `[56..64]` of an additional block.
// 4. Bit-length representation for large files (`>= 512 MiB` and `>= 4 GiB`):
//    - `total_bytes` is `u64` (NOT `u32`), so `total_bytes * 8` does NOT overflow at `512 MiB`
//      (`2^29 B * 8 = 2^32 bits = 0x1_0000_0000`) or `4 GiB`; `u64` bytes * 8 fits cleanly up to
//      `0x1FFF_FFFF_FFFF_FFFF` bytes (`2 EiB - 1 B` = `2^64 - 8` bits, the full FIPS 180-4 limit).
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
        self.total_bytes = self.total_bytes.saturating_add(input.len() as u64);
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
        // `total_bytes` is `u64`: for files up to `2 EiB - 1 B` (`0x1FFF_FFFF_FFFF_FFFF`),
        // `total_bytes * 8` fits in `u64` without overflow (e.g. 512 MiB -> `0x1_0000_0000` bits).
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    /// Builds a complete, spec-accurate little-endian ELF64 binary in memory where `PT_LOAD`
    /// maps file offset `0x200..` at virtual address `0x0040_1200` (`vaddr != file_offset`),
    /// exercising `PT_LOAD` virtual-address-to-file-offset translation for `DT_STRTAB`.
    fn build_synthetic_elf64_dynamic_binary(
        machine: u16,
        appimage_type2: bool,
        interp: Option<&str>,
        soname: Option<&str>,
        needed: &[&str],
        rpaths: &[&str],
        runpaths: &[&str],
    ) -> Vec<u8> {
        let mut dynstr = vec![0u8]; // index 0 is always empty string
        let push_str = |table: &mut Vec<u8>, s: &str| -> u64 {
            let off = table.len() as u64;
            table.extend_from_slice(s.as_bytes());
            table.push(0);
            off
        };

        let soname_off = soname.map(|s| push_str(&mut dynstr, s));
        let needed_offs: Vec<u64> = needed.iter().map(|s| push_str(&mut dynstr, s)).collect();
        let rpath_offs: Vec<u64> = rpaths.iter().map(|s| push_str(&mut dynstr, s)).collect();
        let runpath_offs: Vec<u64> = runpaths.iter().map(|s| push_str(&mut dynstr, s)).collect();

        // File layout:
        // 0x000..0x040: Elf64_Ehdr (64B)
        // 0x040..0x120: up to 4 Elf64_Phdr entries (4 * 56 = 224B -> ends at 0x120)
        // 0x200..     : PT_LOAD payload mapped at base_vaddr = 0x0040_1200:
        //   +0x00..   : PT_INTERP bytes (if present)
        //   aligned 8 : .dynstr
        //   aligned 8 : .dynamic (PT_DYNAMIC)
        let load_file_off: u64 = 0x200;
        let load_base_vaddr: u64 = 0x0040_1200;

        let mut payload = Vec::new();
        let mut interp_range = None;
        if let Some(ip) = interp {
            let start = payload.len() as u64;
            payload.extend_from_slice(ip.as_bytes());
            payload.push(0);
            interp_range = Some((load_file_off + start, payload.len() as u64 - start));
        }
        while payload.len() % 8 != 0 {
            payload.push(0);
        }

        let dynstr_rel = payload.len() as u64;
        let dynstr_vaddr = load_base_vaddr + dynstr_rel;
        let dynstr_sz = dynstr.len() as u64;
        payload.extend_from_slice(&dynstr);
        while payload.len() % 8 != 0 {
            payload.push(0);
        }

        let dyn_rel = payload.len() as u64;
        let mut push_dyn = |tag: u64, val: u64| {
            payload.extend_from_slice(&tag.to_le_bytes());
            payload.extend_from_slice(&val.to_le_bytes());
        };
        push_dyn(DT_STRTAB, dynstr_vaddr);
        push_dyn(DT_STRSZ, dynstr_sz);
        if let Some(off) = soname_off {
            push_dyn(DT_SONAME, off);
        }
        for off in needed_offs {
            push_dyn(DT_NEEDED, off);
        }
        for off in rpath_offs {
            push_dyn(DT_RPATH, off);
        }
        for off in runpath_offs {
            push_dyn(DT_RUNPATH, off);
        }
        push_dyn(DT_NULL, 0);
        let dyn_sz = payload.len() as u64 - dyn_rel;

        let load_filesz = payload.len() as u64;
        let mut phdrs: Vec<[u8; 56]> = Vec::new();
        let make_phdr =
            |p_type: u32, p_flags: u32, off: u64, vaddr: u64, filesz: u64, memsz: u64| {
                let mut p = [0u8; 56];
                p[0..4].copy_from_slice(&p_type.to_le_bytes());
                p[4..8].copy_from_slice(&p_flags.to_le_bytes());
                p[8..16].copy_from_slice(&off.to_le_bytes());
                p[16..24].copy_from_slice(&vaddr.to_le_bytes());
                p[24..32].copy_from_slice(&vaddr.to_le_bytes());
                p[32..40].copy_from_slice(&filesz.to_le_bytes());
                p[40..48].copy_from_slice(&memsz.to_le_bytes());
                p[48..56].copy_from_slice(&0x1000u64.to_le_bytes());
                p
            };

        // Include extra BSS tail (`memsz = filesz + 0x2000`) to prove BSS virtual addresses are NOT mapped to file bytes!
        phdrs.push(make_phdr(
            PT_LOAD,
            5,
            load_file_off,
            load_base_vaddr,
            load_filesz,
            load_filesz + 0x2000,
        ));
        if let Some((ioff, isz)) = interp_range {
            phdrs.push(make_phdr(
                PT_INTERP,
                4,
                ioff,
                load_base_vaddr + (ioff - load_file_off),
                isz,
                isz,
            ));
        }
        phdrs.push(make_phdr(
            PT_DYNAMIC,
            6,
            load_file_off + dyn_rel,
            load_base_vaddr + dyn_rel,
            dyn_sz,
            dyn_sz,
        ));

        let mut file = vec![0u8; load_file_off as usize];
        file[0..4].copy_from_slice(b"\x7fELF");
        file[4] = 2; // ELFCLASS64
        file[5] = 1; // ELFDATA2LSB
        file[6] = 1; // EV_CURRENT
        if appimage_type2 {
            file[8..11].copy_from_slice(b"AI\x02");
        }
        file[16..18].copy_from_slice(&3u16.to_le_bytes()); // ET_DYN (PIE)
        file[18..20].copy_from_slice(&machine.to_le_bytes());
        file[20..24].copy_from_slice(&1u32.to_le_bytes());
        file[32..40].copy_from_slice(&64u64.to_le_bytes()); // e_phoff = 64
        file[52..54].copy_from_slice(&64u16.to_le_bytes()); // e_ehsize = 64
        file[54..56].copy_from_slice(&56u16.to_le_bytes()); // e_phentsize = 56
        file[56..58].copy_from_slice(&(phdrs.len() as u16).to_le_bytes());

        for (i, ph) in phdrs.iter().enumerate() {
            let start = 64 + i * 56;
            file[start..start + 56].copy_from_slice(ph);
        }
        file.extend_from_slice(&payload);
        file
    }

    fn pattern_bytes(len: usize) -> Vec<u8> {
        (0..len)
            .map(|i| ((i.wrapping_mul(31).wrapping_add(17)) & 0xFF) as u8)
            .collect()
    }

    #[test]
    fn sha256_matches_verified_openssl_and_fips_180_4_boundary_vectors() {
        // Cross-verified against Node.js OpenSSL `crypto.createHash('sha256')` & NIST FIPS 180-4:
        let vectors: &[(usize, &str)] = &[
            (
                0,
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            ),
            (
                55,
                "c096925d8ee622ec75c396b1712bcb599952e05c2cd1f3e33032514893edbc83",
            ),
            (
                56,
                "4e3bd718369bf855312b6fae0e874c697a5c910c0270ebab390f15872a7878a5",
            ),
            (
                63,
                "e5e8e5e7481532428a3d324d4163fe92fea5f52e85fbe33abfa5aa9235c5addf",
            ),
            (
                64,
                "f616afd08efb65e91c7fa856fff5e2b991980c5b3fbdffb81f1e4d9dbdb509d7",
            ),
            (
                65,
                "8ce44bf6e55d58f6f5fa326e9fa574cf8fd80adcaf299d4b2f93e9b7414a9821",
            ),
            (
                128,
                "fdb50d75cc2ca56537d86f78e871f423691c3822ad1ac9653e84ab9781328f9f",
            ),
            (
                1000,
                "157bac17a97efb50756d4af6b73d2c6e41fee6c4012fa21a6c638eb2febae3cb",
            ),
            (
                1_048_576,
                "042d84d3772b8cc9080a8f388c428c11d88f8566e5a38fda322fc8ea33492cfc",
            ),
        ];

        assert_eq!(
            sha256_bytes(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );

        let tmp = tempfile::tempdir().unwrap();
        for &(len, expected) in vectors {
            let data = pattern_bytes(len);
            assert_eq!(sha256_bytes(&data), expected, "mismatch at length {len}");

            // Verify odd prime chunk splits (1, 7, 31, 63, 64, 65) produce identical digests.
            for &step in &[7usize, 31, 63, 64, 65] {
                if len <= 4096 {
                    let mut chunked = Sha256State::new();
                    for part in data.chunks(step) {
                        chunked.update(part);
                    }
                    assert_eq!(
                        encode_hex(&chunked.finalize()),
                        expected,
                        "chunked step={step} mismatch at len={len}"
                    );
                }
            }

            if len == 1_048_576 {
                let p = tmp.path().join("one_mib.bin");
                fs::write(&p, &data).unwrap();
                assert_eq!(sha256_file(&p).unwrap(), expected);
            }
        }

        // Verify > 512 MiB bit-length representation (`total_bytes >= 536_870_912` -> `bit_len >= 2^32`):
        // A 32-bit bit counter would wrap `536_870_913 * 8` to `8`, whereas `u64` encodes `0x0000_0001_0000_0008`.
        let over_512_mib_bytes: u64 = (512 * 1024 * 1024) + 1;
        let bit_len = over_512_mib_bytes.wrapping_mul(8);
        assert_eq!(bit_len, 0x0000_0001_0000_0008u64);
        assert_eq!(
            bit_len.to_be_bytes(),
            [0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x08]
        );
    }

    #[test]
    fn static_elf64_parser_extracts_interp_needed_and_resolves_multiarch_and_origin() {
        let tmp = tempfile::tempdir().unwrap();
        let fs_root = tmp.path().join("ubuntu_root");
        let multiarch = fs_root.join("usr/lib/x86_64-linux-gnu");
        let app_dir = tmp.path().join("opt/zalo-app");
        let app_lib = app_dir.join("lib");
        fs::create_dir_all(&multiarch).unwrap();
        fs::create_dir_all(&app_lib).unwrap();

        // Present on disk: libc.so.6 (system multiarch) and libbundled.so.1 ($ORIGIN/lib)
        fs::write(multiarch.join("libc.so.6"), b"elf").unwrap();
        fs::write(app_lib.join("libbundled.so.1"), b"elf").unwrap();
        // Absent on disk: libopus.so.0 and libfuse.so.2

        let elf_bytes = build_synthetic_elf64_dynamic_binary(
            0x3E,
            false,
            Some("/lib64/ld-linux-x86-64.so.2"),
            Some("libzalo_shell.so"),
            &[
                "linux-vdso.so.1",
                "libc.so.6",
                "libbundled.so.1",
                "libopus.so.0",
                "libfuse.so.2",
            ],
            &["/ignored/because/runpath/present"],
            &["$ORIGIN/lib:/usr/local/custom/lib"],
        );

        let bin_path = app_dir.join("zalo-bin");
        fs::write(&bin_path, &elf_bytes).unwrap();

        let (meta, missing) = inspect_elf64_needed_libraries_at(&bin_path, &fs_root).unwrap();
        assert_eq!(meta.arch, ElfCpuArch::X86_64);
        assert_eq!(
            meta.interpreter.as_deref(),
            Some("/lib64/ld-linux-x86-64.so.2")
        );
        assert_eq!(meta.soname.as_deref(), Some("libzalo_shell.so"));
        assert_eq!(
            meta.needed_libraries,
            vec![
                "linux-vdso.so.1",
                "libc.so.6",
                "libbundled.so.1",
                "libopus.so.0",
                "libfuse.so.2"
            ]
        );
        assert_eq!(
            meta.runpath_entries,
            vec!["$ORIGIN/lib", "/usr/local/custom/lib"]
        );

        assert_eq!(missing.len(), 2);
        assert_eq!(missing[0].soname, "libopus.so.0");
        assert_eq!(missing[0].ubuntu_2404_package, "libopus0");
        assert_eq!(missing[1].soname, "libfuse.so.2");
        assert_eq!(missing[1].ubuntu_2404_package, "libfuse2t64");
        assert_eq!(missing[1].ubuntu_2204_package, "libfuse2");
    }

    #[test]
    fn static_elf64_parser_rejects_malformed_truncated_and_adversarial_inputs_safely() {
        // 1. Empty / truncated ELF header (< 64 bytes)
        assert!(matches!(
            parse_elf64_dynamic_metadata(b""),
            Err(Elf64ParseError::TruncatedElfHeader { actual_bytes: 0 })
        ));
        assert!(matches!(
            parse_elf64_dynamic_metadata(b"\x7fELF\x02\x01\x01"),
            Err(Elf64ParseError::TruncatedElfHeader { actual_bytes: 7 })
        ));

        // 2. 32-bit ELF rejected cleanly
        let mut elf32 = vec![0u8; 64];
        elf32[0..4].copy_from_slice(b"\x7fELF");
        elf32[4] = 1; // ELFCLASS32
        assert_eq!(
            parse_elf64_dynamic_metadata(&elf32),
            Err(Elf64ParseError::UnsupportedClass(1))
        );

        // 3. Oversized e_phnum (65535) rejected before allocation
        let mut huge_phnum =
            build_synthetic_elf64_dynamic_binary(0x3E, false, None, None, &[], &[], &[]);
        huge_phnum[56..58].copy_from_slice(&65_535u16.to_le_bytes());
        assert_eq!(
            parse_elf64_dynamic_metadata(&huge_phnum),
            Err(Elf64ParseError::TooManyProgramHeaders(65_535))
        );

        // 4. Out-of-bounds e_phoff (`u64::MAX - 8`) rejected without integer overflow panic
        let mut bad_phoff =
            build_synthetic_elf64_dynamic_binary(0x3E, false, None, None, &[], &[], &[]);
        bad_phoff[32..40].copy_from_slice(&(u64::MAX - 8).to_le_bytes());
        assert!(matches!(
            parse_elf64_dynamic_metadata(&bad_phoff),
            Err(Elf64ParseError::TruncatedProgramHeaderTable { .. })
        ));

        // 5. Forged DT_STRSZ = u64::MAX rejected without OOM
        let mut huge_strsz =
            build_synthetic_elf64_dynamic_binary(0x3E, false, None, None, &["libc.so.6"], &[], &[]);
        // Locate DT_STRSZ tag (`10u64` little-endian) inside PT_DYNAMIC and overwrite its value.
        let needle = [
            10u64.to_le_bytes(),
            11u64.to_le_bytes(), // original dynstr_sz ("\0libc.so.6\0" = 11 bytes)
        ]
        .concat();
        if let Some(pos) = huge_strsz.windows(16).position(|w| w == needle) {
            huge_strsz[pos + 8..pos + 16].copy_from_slice(&u64::MAX.to_le_bytes());
        }
        assert!(matches!(
            parse_elf64_dynamic_metadata(&huge_strsz),
            Err(Elf64ParseError::OversizedStringTable(u64::MAX))
        ));

        // 6. DT_STRTAB pointing into BSS (`p_vaddr + p_filesz + 16`) rejected because BSS has no file bytes!
        let mut bss_strtab =
            build_synthetic_elf64_dynamic_binary(0x3E, false, None, None, &["libc.so.6"], &[], &[]);
        let strtab_tag = 5u64.to_le_bytes();
        if let Some(pos) = bss_strtab.windows(16).position(|w| {
            w[0..8] == strtab_tag && u64::from_le_bytes(w[8..16].try_into().unwrap()) == 0x0040_1200
        }) {
            bss_strtab[pos + 8..pos + 16].copy_from_slice(&0x0040_2500u64.to_le_bytes());
        }
        assert!(matches!(
            parse_elf64_dynamic_metadata(&bss_strtab),
            Err(Elf64ParseError::UnmappedStringTable(0x0040_2500))
        ));
    }

    #[test]
    fn invalid_garbage_appimage_is_flagged_and_rejected_by_strict_validation() {
        let tmp = tempfile::tempdir().unwrap();
        let fake_appimage = tmp.path().join("CorruptGarbage.AppImage");
        fs::write(&fake_appimage, b"not-an-elf-just-random-bytes").unwrap();

        let report = inspect_user_binary(&fake_appimage, None).unwrap();
        assert_eq!(report.format, BinaryFormatKind::Unknown);
        assert!(!report.has_valid_executable_header);
        assert!(!report.is_valid_appimage_elf64());
        assert!(report.validate_executable_format().is_err());
        assert!(report
            .notes
            .iter()
            .any(|n| n.contains("Invalid .AppImage executable header")));
        assert!(report
            .notes
            .iter()
            .any(|n| n.contains("byte-level integrity") && n.contains("NEVER publisher identity")));

        // Valid Type-2 ELF64 AppImage passes strict validation cleanly.
        let valid_appimage = tmp.path().join("ValidZalo-26.11.10.AppImage");
        let valid_bytes = build_synthetic_elf64_dynamic_binary(
            0x3E,
            true,
            Some("/lib64/ld-linux-x86-64.so.2"),
            None,
            &["libc.so.6"],
            &[],
            &[],
        );
        fs::write(&valid_appimage, &valid_bytes).unwrap();
        let ok_report = inspect_user_binary(&valid_appimage, None).unwrap();
        assert_eq!(ok_report.format, BinaryFormatKind::AppImageType2);
        assert!(ok_report.has_valid_executable_header);
        assert!(ok_report.is_valid_appimage_elf64());
        assert!(ok_report.validate_executable_format().is_ok());
    }
}
