//! Path traversal prevention, Windows reserved device checks, and disk-space guards.

use std::path::{Component, Path, PathBuf};

use crate::error::DimError;
use crate::models::MIN_FREE_DISK_BYTES;

const RESERVED_WINDOWS_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Resolves a relative path safely inside `root`, rejecting traversal, absolute prefixes,
/// NUL bytes, Windows reserved device names, and existing symlink/reparse escapes.
pub fn resolve_safe_child(root: &Path, relative: &str) -> Result<PathBuf, DimError> {
    if relative.is_empty() {
        return Err(DimError::PathTraversal(
            "relative child path cannot be empty".to_owned(),
        ));
    }
    if relative.contains('\0') {
        return Err(DimError::PathTraversal(
            "relative child path contains NUL byte".to_owned(),
        ));
    }
    if relative.starts_with('/') || relative.starts_with('\\') {
        return Err(DimError::PathTraversal(format!(
            "absolute-prefixed child path rejected: '{relative}'"
        )));
    }
    if has_windows_drive_prefix(relative) {
        return Err(DimError::PathTraversal(format!(
            "drive-prefixed child path rejected: '{relative}'"
        )));
    }

    let mut resolved = root.to_path_buf();
    let mut segment_count = 0usize;

    // Normalize both '/' and '\\' on every host so Windows-style payloads are rejected on POSIX too.
    for raw_seg in relative.split(['/', '\\']) {
        if raw_seg.is_empty() || raw_seg == "." || raw_seg == ".." {
            return Err(DimError::PathTraversal(format!(
                "invalid or traversal segment '{raw_seg}' in '{relative}'"
            )));
        }
        validate_segment_characters(raw_seg)?;
        resolved.push(raw_seg);
        segment_count += 1;
    }

    if segment_count == 0 {
        return Err(DimError::PathTraversal(
            "relative path resolved to root itself".to_owned(),
        ));
    }

    // Lexical verification using std::path::Path components as defense in depth.
    let rel_path = Path::new(relative);
    for comp in rel_path.components() {
        if !matches!(comp, Component::Normal(_)) {
            return Err(DimError::PathTraversal(format!(
                "non-normal path component in '{relative}'"
            )));
        }
    }

    // Verify canonical containment against existing targets (including broken symlinks) or the
    // deepest existing ancestor so pre-existing symlinked/junctioned subdirectories cannot escape `root`.
    if root.exists() {
        let canon_root = std::fs::canonicalize(root)?;
        if resolved.exists() || std::fs::symlink_metadata(&resolved).is_ok() {
            let canon_target = std::fs::canonicalize(&resolved).map_err(|_| {
                DimError::PathTraversal(format!(
                    "unresolvable or broken symlink target inside '{}'",
                    root.display()
                ))
            })?;
            if !canon_target.starts_with(&canon_root) {
                return Err(DimError::PathTraversal(format!(
                    "canonicalized path escapes root '{}'",
                    root.display()
                )));
            }
        } else {
            let mut probe = resolved.as_path();
            while let Some(parent) = probe.parent() {
                if std::fs::symlink_metadata(parent).is_ok() {
                    let canon_ancestor = std::fs::canonicalize(parent).map_err(|_| {
                        DimError::PathTraversal(format!(
                            "unresolvable or broken symlink ancestor inside '{}'",
                            root.display()
                        ))
                    })?;
                    if !canon_ancestor.starts_with(&canon_root) {
                        return Err(DimError::PathTraversal(format!(
                            "existing ancestor symlink escapes root '{}'",
                            root.display()
                        )));
                    }
                    break;
                }
                if parent == root {
                    break;
                }
                probe = parent;
            }
        }
    }

    Ok(resolved)
}

/// Verifies available disk space meets at least `required_bytes`.
pub fn check_disk_space_bytes(
    target_path: &Path,
    available_bytes: u64,
    required_bytes: u64,
) -> Result<(), DimError> {
    if available_bytes < required_bytes {
        return Err(DimError::InsufficientDiskSpace {
            path: target_path.display().to_string(),
            available_bytes,
            required_bytes,
        });
    }
    Ok(())
}

/// Enforces the default 5 GiB free disk threshold (`MIN_FREE_DISK_BYTES`) plus `extra_bytes`.
pub fn ensure_min_free_disk_space(
    target_path: &Path,
    available_bytes: u64,
    extra_bytes: u64,
) -> Result<(), DimError> {
    let required = MIN_FREE_DISK_BYTES.saturating_add(extra_bytes);
    check_disk_space_bytes(target_path, available_bytes, required)
}

fn has_windows_drive_prefix(s: &str) -> bool {
    let bytes = s.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

fn validate_segment_characters(seg: &str) -> Result<(), DimError> {
    if seg.ends_with(' ') || seg.ends_with('.') {
        return Err(DimError::PathTraversal(format!(
            "segment cannot end with space or dot: '{seg}'"
        )));
    }
    for ch in seg.chars() {
        if ch.is_control() || matches!(ch, '<' | '>' | ':' | '"' | '|' | '?' | '*') {
            return Err(DimError::PathTraversal(format!(
                "forbidden character '{ch}' in segment '{seg}'"
            )));
        }
    }
    let stem = seg.split('.').next().unwrap_or(seg);
    if RESERVED_WINDOWS_NAMES
        .iter()
        .any(|reserved| stem.eq_ignore_ascii_case(reserved))
    {
        return Err(DimError::PathTraversal(format!(
            "reserved Windows device name rejected: '{seg}'"
        )));
    }
    Ok(())
}
