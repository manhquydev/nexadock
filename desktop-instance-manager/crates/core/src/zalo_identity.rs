//! Zalo logged-in account detection (`config.json` `db_path.location` + `Database/_production/<6+ digits>`)
//! and duplicate-account guard.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::error::DimError;
use crate::models::ProfileRecord;

/// Minimum number of consecutive ASCII digits required in `Database/_production/<uid>` directory names.
pub const MIN_ZALO_USER_ID_LEN: usize = 6;

/// Inspects `<zalo_data_dir>/config.json` (`db_path.location`) for an externalized DB root if configured,
/// then scans `Database/_production/<6+ digits>` and selects the most-recently-modified account ID.
#[must_use]
pub fn detect_zalo_user_id(zalo_data_dir: &Path) -> Option<String> {
    let mut candidate_roots: Vec<PathBuf> = Vec::new();

    if let Some(custom_db_root) = read_configured_db_location(zalo_data_dir) {
        candidate_roots.push(custom_db_root.join("Database").join("_production"));
        candidate_roots.push(custom_db_root.join("_production"));
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
    let cfg_path = zalo_data_dir.join("config.json");
    let raw = fs::read_to_string(cfg_path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let loc = json
        .get("db_path")
        .and_then(|v| v.get("location"))
        .and_then(serde_json::Value::as_str)?
        .trim();
    if loc.is_empty() {
        return None;
    }
    let path = PathBuf::from(loc);
    path.is_dir().then_some(path)
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
