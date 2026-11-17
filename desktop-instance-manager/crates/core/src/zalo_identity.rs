//! Zalo logged-in account detection (`config.json` `db_path.location` + `Database/_production/<6+ digits>`),
//! external storage path containment (`IR-WIN-04` / `F-CORE-01`), and duplicate-account guard.

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

use crate::error::DimError;
use crate::models::ProfileRecord;

/// Minimum number of consecutive ASCII digits required in `Database/_production/<uid>` directory names.
pub const MIN_ZALO_USER_ID_LEN: usize = 6;

/// Externalized database and media/resource locations configured inside `<ZaloData>/config.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConfiguredExternalPaths {
    /// Configured `db_path.location` (or string `db_path`), resolved relative to `zalo_data_dir` if relative.
    pub db_location: Option<PathBuf>,
    /// Configured `resource.location.path` (or string `resource.location`), resolved relative to `zalo_data_dir` if relative.
    pub resource_location: Option<PathBuf>,
}

/// Parses `<zalo_data_dir>/config.json` for `db_path.location` and `resource.location.path`
/// without modifying or creating any files.
#[must_use]
pub fn read_configured_external_paths(zalo_data_dir: &Path) -> ConfiguredExternalPaths {
    let cfg_path = zalo_data_dir.join("config.json");
    let Ok(raw) = fs::read_to_string(cfg_path) else {
        return ConfiguredExternalPaths::default();
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return ConfiguredExternalPaths::default();
    };

    let parse_loc = |raw_str: &str| -> Option<PathBuf> {
        let trimmed = raw_str.trim();
        if trimmed.is_empty() {
            return None;
        }
        let p = PathBuf::from(trimmed);
        if p.is_absolute() {
            Some(p)
        } else {
            Some(zalo_data_dir.join(p))
        }
    };

    let db_location = json
        .get("db_path")
        .and_then(|v| {
            v.get("location")
                .and_then(serde_json::Value::as_str)
                .or_else(|| v.as_str())
        })
        .and_then(parse_loc);

    let resource_location = json
        .get("resource")
        .and_then(|r| {
            r.get("location").and_then(|loc| {
                loc.get("path")
                    .and_then(serde_json::Value::as_str)
                    .or_else(|| loc.as_str())
            })
        })
        .and_then(parse_loc);

    ConfiguredExternalPaths {
        db_location,
        resource_location,
    }
}

/// Inspects `<zalo_data_dir>/config.json` (`db_path.location`) for an externalized DB root if configured,
/// then scans `Database/_production/<6+ digits>` and selects the most-recently-modified account ID.
#[must_use]
pub fn detect_zalo_user_id(zalo_data_dir: &Path) -> Option<String> {
    let mut candidate_roots: Vec<PathBuf> = Vec::new();

    if let Some(custom_db_root) = read_configured_db_location(zalo_data_dir) {
        candidate_roots.push(custom_db_root);
        if let Some(custom_db_root2) = read_configured_db_location(zalo_data_dir) {
            candidate_roots.push(custom_db_root2.join("Database").join("_production"));
            candidate_roots.push(custom_db_root2.join("_production"));
        }
    }
    candidate_roots.push(zalo_data_dir.join("Database").join("_production"));

    for prod_dir in candidate_roots {
        if let Some(uid) = find_latest_user_id_in_production(&prod_dir) {
            return Some(uid);
        }
    }

    None
}

fn read_configured_db_location(zalo_data_dir: &Path) -> Option<PathBuf> {
    read_configured_external_paths(zalo_data_dir)
        .db_location
        .filter(|p| p.is_dir())
}

fn find_latest_user_id_in_production(prod_dir: &Path) -> Option<String> {
    let entries = fs::read_dir(prod_dir).ok()?;

    let mut candidates: Vec<(SystemTime, String)> = Vec::new();
    for entry_res in entries {
        let Ok(entry) = entry_res else {
            continue;
        };
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let Some(name_str) = name.to_str() else {
            continue;
        };
        if is_valid_zalo_user_id(name_str) {
            let modified = entry
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            candidates.push((modified, name_str.to_owned()));
        }
    }

    // Prefer the most recently modified account directory, breaking equal-mtime ties deterministically.
    candidates.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    candidates.into_iter().next().map(|(_, id)| id)
}

/// Validates that a string is at least 6 ASCII digits (`^\d{6,}$`).
#[must_use]
pub fn is_valid_zalo_user_id(value: &str) -> bool {
    value.len() >= MIN_ZALO_USER_ID_LEN && value.bytes().all(|b| b.is_ascii_digit())
}

/// Refuses launching `target` if another currently running profile shares the same `zalo_user_id`.
pub fn check_duplicate_zalo_user_id(
    target: &ProfileRecord,
    target_detected_uid: Option<&str>,
    running_profiles: &[(ProfileRecord, Option<String>)],
) -> Result<(), DimError> {
    let target_uid = target_detected_uid
        .filter(|s| is_valid_zalo_user_id(s))
        .or_else(|| {
            target
                .zalo_user_id
                .as_deref()
                .filter(|s| is_valid_zalo_user_id(s))
        });

    let Some(uid) = target_uid else {
        return Ok(());
    };

    for (other, other_detected_uid) in running_profiles {
        if other.id.eq_ignore_ascii_case(&target.id) {
            continue;
        }
        let other_uid = other_detected_uid
            .as_deref()
            .filter(|s| is_valid_zalo_user_id(s))
            .or_else(|| {
                other
                    .zalo_user_id
                    .as_deref()
                    .filter(|s| is_valid_zalo_user_id(s))
            });

        if other_uid == Some(uid) {
            return Err(DimError::DuplicateZaloUserId {
                user_id: uid.to_owned(),
                running_profile: format!("{} ({})", other.name, other.id),
            });
        }
    }

    Ok(())
}

fn strip_verbatim_prefix(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{rest}"))
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        path.to_path_buf()
    }
}

fn normalize_lexical_components(path: &Path) -> PathBuf {
    let stripped = strip_verbatim_prefix(path);
    let mut out = PathBuf::new();
    for comp in stripped.components() {
        match comp {
            Component::Prefix(p) => out.push(p.as_os_str()),
            Component::RootDir => out.push(comp.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                // Pop a normal component if present, never popping past root/prefix.
                if matches!(out.components().next_back(), Some(Component::Normal(_))) {
                    let _ = out.pop();
                }
            }
            Component::Normal(seg) => out.push(seg),
        }
    }
    out
}

fn is_meta_symlink_or_reparse(meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        meta.file_type().is_symlink() || (meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT) != 0
    }
    #[cfg(not(windows))]
    {
        meta.file_type().is_symlink()
    }
}

/// Resolves `candidate` (joining onto `base_dir` if relative), stripping Windows verbatim prefixes,
/// collapsing `.` and `..`, AND walking every existing ancestor/component to follow symlinks and
/// NTFS directory junctions even when leaf subdirectories do not yet exist on disk!
#[must_use]
pub fn resolve_effective_path(base_dir: &Path, candidate: &Path) -> PathBuf {
    let joined = if candidate.is_absolute() {
        strip_verbatim_prefix(candidate)
    } else {
        strip_verbatim_prefix(base_dir).join(candidate)
    };
    let lexical = normalize_lexical_components(&joined);

    if let Ok(canon) = fs::canonicalize(&lexical) {
        return normalize_lexical_components(&canon);
    }

    // Walk components from root to leaf, resolving the deepest existing ancestor (including any
    // NTFS junction or symlink ancestor!) and appending any not-yet-created tail segments.
    let comps: Vec<Component<'_>> = lexical.components().collect();
    let mut deepest_resolved: Option<(usize, PathBuf)> = None;
    let mut cur = PathBuf::new();

    for (idx, comp) in comps.iter().enumerate() {
        cur.push(comp.as_os_str());
        if let Ok(meta) = fs::symlink_metadata(&cur) {
            if is_meta_symlink_or_reparse(&meta) {
                if let Ok(canon_target) = fs::canonicalize(&cur) {
                    cur = normalize_lexical_components(&canon_target);
                    deepest_resolved = Some((idx, cur.clone()));
                    continue;
                }
            }
            if let Ok(canon_cur) = fs::canonicalize(&cur) {
                deepest_resolved = Some((idx, normalize_lexical_components(&canon_cur)));
            } else {
                deepest_resolved = Some((idx, cur.clone()));
            }
        }
    }

    if let Some((resolved_idx, mut base_resolved)) = deepest_resolved {
        for tail in comps.iter().skip(resolved_idx + 1) {
            if let Component::Normal(seg) = tail {
                base_resolved.push(seg);
            }
        }
        normalize_lexical_components(&base_resolved)
    } else {
        lexical
    }
}

fn normalized_component_keys(path: &Path) -> Vec<String> {
    normalize_lexical_components(path)
        .components()
        .filter_map(|c| match c {
            Component::RootDir => None,
            other => {
                let s = other.as_os_str().to_string_lossy();
                let trimmed = s.trim_end_matches(['\\', '/']);
                if trimmed.is_empty() {
                    None
                } else if cfg!(windows) {
                    Some(trimmed.to_lowercase())
                } else {
                    Some(trimmed.to_owned())
                }
            }
        })
        .collect()
}

/// Returns `true` ONLY when `candidate` truly resolves inside `profile_dir` after expanding `..`,
/// stripping verbatim `\\?\` prefixes, case-folding on Windows, and resolving every symlink / NTFS
/// junction along `candidate`'s ancestor chain (defeating junction/symlink escapes inside `<profile_dir>`).
#[must_use]
pub fn is_path_truly_contained_in_profile(profile_dir: &Path, candidate: &Path) -> bool {
    let resolved_profile = resolve_effective_path(profile_dir, profile_dir);
    let resolved_candidate = resolve_effective_path(profile_dir, candidate);

    let prof_keys = normalized_component_keys(&resolved_profile);
    let cand_keys = normalized_component_keys(&resolved_candidate);

    if prof_keys.is_empty() || cand_keys.len() < prof_keys.len() {
        return false;
    }
    prof_keys
        .iter()
        .zip(cand_keys.iter())
        .all(|(p, c)| p == c)
}

/// Returns `true` when two already-resolved storage paths are identical or one is an ancestor of the other.
#[must_use]
pub fn paths_overlap_or_equal(a: &Path, b: &Path) -> bool {
    let ka = normalized_component_keys(a);
    let kb = normalized_component_keys(b);
    if ka.is_empty() || kb.is_empty() {
        return false;
    }
    let min_len = ka.len().min(kb.len());
    ka[..min_len] == kb[..min_len]
}

/// Returns all configured `db_path.location` / `resource.location.path` targets from `<zalo_data_dir>/config.json`
/// that resolve OUTSIDE `profile_dir` (including via `..` traversal or symlink/NTFS junction escape).
#[must_use]
pub fn external_paths_outside_profile(profile_dir: &Path, zalo_data_dir: &Path) -> Vec<PathBuf> {
    let cfg = read_configured_external_paths(zalo_data_dir);
    let mut out: Vec<PathBuf> = Vec::new();

    for raw_opt in [cfg.db_location, cfg.resource_location] {
        let Some(raw_path) = raw_opt else {
            continue;
        };
        let resolved = resolve_effective_path(zalo_data_dir, &raw_path);
        if !is_path_truly_contained_in_profile(profile_dir, &resolved)
            && !out.iter().any(|existing| {
                normalized_component_keys(existing) == normalized_component_keys(&resolved)
            })
        {
            out.push(resolved);
        }
    }
    out
}

/// Scans all 3 OS `ZaloData` candidate layouts inside `profile_dir` (`AppData/Roaming/ZaloData`,
/// `Library/Application Support/ZaloData`, `.config/ZaloData`) and collects any configured storage
/// paths that resolve outside `profile_dir`.
#[must_use]
pub fn external_paths_for_profile_all_layouts(profile_dir: &Path) -> Vec<PathBuf> {
    let candidates = [
        profile_dir.join("AppData").join("Roaming").join("ZaloData"),
        profile_dir
            .join("Library")
            .join("Application Support")
            .join("ZaloData"),
        profile_dir.join(".config").join("ZaloData"),
    ];
    let mut out: Vec<PathBuf> = Vec::new();
    for zdir in candidates {
        for ext in external_paths_outside_profile(profile_dir, &zdir) {
            if !out
                .iter()
                .any(|e| normalized_component_keys(e) == normalized_component_keys(&ext))
            {
                out.push(ext);
            }
        }
    }
    out
}
