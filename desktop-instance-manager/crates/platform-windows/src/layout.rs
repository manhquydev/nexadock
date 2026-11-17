//! Pure-Rust Windows environment block builders, command-line quoting, version sorting,
//! PE Authenticode table inspection, and Folder Redirection detection.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use dim_core::{DimError, ProfileRecord, ZALO_PROFILE_ID_FILE};
use dim_profile_store::write_profile_id_marker;

/// Creates the Windows isolated directory tree and pre-seeds migration markers so Zalo Desktop
/// never scans or writes into the user's primary `%APPDATA%\\ZaloData` or `%LOCALAPPDATA%\\ZaloPC`.
pub fn ensure_windows_profile_layout(
    profile: &ProfileRecord,
    profile_dir: &Path,
) -> Result<(), DimError> {
    let roaming = profile_dir.join("AppData").join("Roaming");
    let zalo_data = roaming.join("ZaloData");
    let media_dir = zalo_data.join("media");
    let local_temp = profile_dir.join("AppData").join("Local").join("Temp");
    let documents = profile_dir.join("Documents").join("Zalo Received Files");
    let desktop = profile_dir.join("Desktop");

    fs::create_dir_all(&media_dir)?;
    fs::create_dir_all(&local_temp)?;
    fs::create_dir_all(&documents)?;
    fs::create_dir_all(&desktop)?;

    let zmeta = roaming.join("zmeta.json");
    if !zmeta.exists() {
        fs::write(zmeta, br#"{"failedMigrateCount":0}"#)?;
    }

    let migrate_cfg = zalo_data.join("migrate-config.json");
    if !migrate_cfg.exists() {
        fs::write(migrate_cfg, br#"{"zalopc_m_c":true}"#)?;
    }

    let id_marker = zalo_data.join(ZALO_PROFILE_ID_FILE);
    if !id_marker.exists() {
        write_profile_id_marker(&zalo_data, &profile.id)?;
    }

    Ok(())
}

/// Returns the isolated `ZaloData` path for a Windows profile directory.
#[must_use]
pub fn windows_zalo_data_dir(profile_dir: &Path) -> PathBuf {
    profile_dir.join("AppData").join("Roaming").join("ZaloData")
}

/// Checks whether a raw `REG_EXPAND_SZ` `User Shell Folders\AppData` value has been redirected
/// away from `%USERPROFILE%`.
#[must_use]
pub fn is_appdata_redirected(raw_value: &str) -> bool {
    let trimmed = raw_value.trim();
    if trimmed.is_empty() {
        return false;
    }
    let normalized = trimmed.to_ascii_lowercase().replace('/', "\\");
    !normalized.starts_with("%userprofile%\\")
}

/// Builds a case-insensitively sorted, deduplicated environment map overriding per-profile paths.
#[must_use]
pub fn build_isolated_env_map(
    base_env: &[(String, String)],
    profile: &ProfileRecord,
    profile_dir: &Path,
) -> BTreeMap<String, (String, String)> {
    let mut map: BTreeMap<String, (String, String)> = BTreeMap::new();
    for (k, v) in base_env {
        if !k.is_empty() {
            map.insert(k.to_ascii_uppercase(), (k.clone(), v.clone()));
        }
    }

    let roaming = profile_dir.join("AppData").join("Roaming");
    let local = profile_dir.join("AppData").join("Local");
    let temp = local.join("Temp");

    let mut set_var = |key: &str, val: String| {
        map.insert(key.to_ascii_uppercase(), (key.to_owned(), val));
    };

    let profile_str = profile_dir.to_string_lossy().into_owned();
    set_var("USERPROFILE", profile_str.clone());
    set_var("APPDATA", roaming.to_string_lossy().into_owned());
    set_var("LOCALAPPDATA", local.to_string_lossy().into_owned());
    set_var("TEMP", temp.to_string_lossy().into_owned());
    set_var("TMP", temp.to_string_lossy().into_owned());
    set_var("DIM_PROFILE_ID", profile.id.clone());
    set_var("DIM_PROFILE_NAME", profile.name.clone());
    set_var("MULTIZALO_PROFILE_ID", profile.id.clone());
    set_var("MULTIZALO_PROFILE_NAME", profile.name.clone());

    let bytes = profile_str.as_bytes();
    if bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        set_var("HOMEDRIVE", profile_str[..2].to_owned());
        set_var("HOMEPATH", profile_str[2..].to_owned());
    }

    map
}

/// Encodes an environment map as a double-NUL-terminated UTF-16 block for `CreateProcessW`.
#[must_use]
pub fn encode_windows_env_block_utf16(env: &BTreeMap<String, (String, String)>) -> Vec<u16> {
    let mut block = Vec::with_capacity(env.len() * 48 + 2);
    for (key, val) in env.values() {
        for unit in key.encode_utf16() {
            block.push(unit);
        }
        block.push(b'=' as u16);
        for unit in val.encode_utf16() {
            block.push(unit);
        }
        block.push(0);
    }
    block.push(0);
    block
}

/// Quotes a single Windows command-line argument according to `CommandLineToArgvW` rules.
#[must_use]
pub fn quote_windows_arg(arg: &str) -> String {
    if !arg.is_empty()
        && !arg
            .chars()
            .any(|c| matches!(c, ' ' | '\t' | '\n' | '\r' | '"'))
    {
        return arg.to_owned();
    }

    let mut out = String::with_capacity(arg.len() + 4);
    out.push('"');
    let mut backslashes = 0usize;
    for ch in arg.chars() {
        if ch == '\\' {
            backslashes += 1;
        } else if ch == '"' {
            for _ in 0..(backslashes * 2 + 1) {
                out.push('\\');
            }
            out.push('"');
            backslashes = 0;
        } else {
            for _ in 0..backslashes {
                out.push('\\');
            }
            backslashes = 0;
            out.push(ch);
        }
    }
    for _ in 0..(backslashes * 2) {
        out.push('\\');
    }
    out.push('"');
    out
}

/// Joins executable path and arguments into a safe Windows `CreateProcessW` command line.
#[must_use]
pub fn build_windows_command_line(exe: &Path, args: &[String]) -> String {
    let mut parts = Vec::with_capacity(args.len() + 1);
    parts.push(quote_windows_arg(&exe.to_string_lossy()));
    for a in args {
        parts.push(quote_windows_arg(a));
    }
    parts.join(" ")
}

/// Parses dotted numeric version components (`"26.10.20"` -> `[26, 10, 20]`).
#[must_use]
pub fn parse_version_parts(version: &str) -> Vec<u32> {
    version
        .split('.')
        .map(|part| part.parse::<u32>().unwrap_or(0))
        .collect()
}

/// Compares two parsed version tuples numerically.
#[must_use]
pub fn compare_versions(a: &[u32], b: &[u32]) -> std::cmp::Ordering {
    let max_len = a.len().max(b.len());
    for i in 0..max_len {
        let av = a.get(i).copied().unwrap_or(0);
        let bv = b.get(i).copied().unwrap_or(0);
        let ord = av.cmp(&bv);
        if ord != std::cmp::Ordering::Equal {
            return ord;
        }
    }
    std::cmp::Ordering::Equal
}

/// Inspects a PE binary's optional header `IMAGE_DIRECTORY_ENTRY_SECURITY` entry in pure Rust
/// to check whether an embedded Authenticode signature blob is present.
#[must_use]
pub fn has_pe_authenticode_table(path: &Path) -> bool {
    let Ok(mut file) = File::open(path) else {
        return false;
    };
    let mut header = [0u8; 4096];
    let Ok(read_len) = file.read(&mut header) else {
        return false;
    };
    let data = &header[..read_len];
    if data.len() < 0x40 || &data[0..2] != b"MZ" {
        return false;
    }
    let e_lfanew = u32::from_le_bytes([data[0x3C], data[0x3D], data[0x3E], data[0x3F]]) as usize;
    let Some(opt_offset) = e_lfanew.checked_add(24) else {
        return false;
    };
    if opt_offset >= data.len() || &data[e_lfanew..e_lfanew + 4] != b"PE\0\0" {
        return false;
    }
    if opt_offset + 2 > data.len() {
        return false;
    }
    let magic = u16::from_le_bytes([data[opt_offset], data[opt_offset + 1]]);
    // Security data directory index 4: offset 128 in PE32 (0x10B), offset 144 in PE32+ (0x20B).
    let sec_dir_offset = match magic {
        0x010B => opt_offset + 128,
        0x020B => opt_offset + 144,
        _ => return false,
    };
    if sec_dir_offset + 8 > data.len() {
        return false;
    }
    let cert_rva = u32::from_le_bytes([
        data[sec_dir_offset],
        data[sec_dir_offset + 1],
        data[sec_dir_offset + 2],
        data[sec_dir_offset + 3],
    ]);
    let cert_size = u32::from_le_bytes([
        data[sec_dir_offset + 4],
        data[sec_dir_offset + 5],
        data[sec_dir_offset + 6],
        data[sec_dir_offset + 7],
    ]);
    cert_rva > 0 && cert_size > 0
}
