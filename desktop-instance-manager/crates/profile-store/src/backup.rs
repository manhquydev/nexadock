//! Offline atomic backup (`.partial` -> `backup.json`) and non-destructive new-profile restore.

use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use dim_core::{
    build_directory_name, detect_zalo_user_id, ensure_min_free_disk_space,
    ensure_not_primary_zalo_path, generate_profile_id, now_compact_utc, now_rfc3339_utc,
    resolve_safe_child, slugify_profile_name, validate_directory_input, validate_profile_name,
    BackupFileEntry, BackupManager, BackupManifest, BackupResult, DimError, ProfileRecord,
    ProfileStore, BACKUP_SCHEMA_VERSION, ZALO_PROFILE_ID_FILE,
};
use sha2::{Digest, Sha256};

use crate::store::{
    is_symlink_or_reparse, unlink_reparse_points_safe, write_profile_id_marker, JsonProfileStore,
};

/// Maximum age (12 hours) before abandoned `.partial` or `.restore-*` staging folders are swept.
pub const STAGING_MAX_AGE: Duration = Duration::from_secs(12 * 3600);

/// Function signature used to query available free bytes at a target path.
pub type DiskSpaceProbe = Arc<dyn Fn(&Path) -> Result<u64, DimError> + Send + Sync>;

/// Atomic offline backup & restore manager bound to a [`JsonProfileStore`].
pub struct FileBackupManager {
    store: Arc<JsonProfileStore>,
    disk_space_probe: Option<DiskSpaceProbe>,
}

impl std::fmt::Debug for FileBackupManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileBackupManager")
            .field("data_dir", &self.store.data_dir())
            .field("has_disk_space_probe", &self.disk_space_probe.is_some())
            .finish()
    }
}

impl FileBackupManager {
    /// Creates a backup manager backed by `store`.
    #[must_use]
    pub fn new(store: Arc<JsonProfileStore>) -> Self {
        Self {
            store,
            disk_space_probe: None,
        }
    }

    /// Attaches a platform disk-space probe enforcing the 5 GiB safety threshold.
    #[must_use]
    pub fn with_disk_space_probe(mut self, probe: DiskSpaceProbe) -> Self {
        self.disk_space_probe = Some(probe);
        self
    }

    fn check_free_space(&self, target_dir: &Path, extra_bytes: u64) -> Result<(), DimError> {
        if let Some(probe) = &self.disk_space_probe {
            let free = probe(target_dir)?;
            ensure_min_free_disk_space(target_dir, free, extra_bytes)?;
        }
        Ok(())
    }

    /// Creates an offline atomic backup, rejecting profiles whose `config.json` configures
    /// `db_path.location` / `resource.location.path` outside `profile_dir` unless
    /// `allow_external_db_omit` is explicitly `true` (`IR-WIN-04` / `F-CORE-01`).
    pub fn backup_profile_with_options(
        &self,
        profile: &ProfileRecord,
        profile_dir: &Path,
        dest_dir: Option<&Path>,
        is_running: bool,
        allow_external_db_omit: bool,
    ) -> Result<BackupResult, DimError> {
        if is_running {
            return Err(DimError::ProfileRunning(format!(
                "{} ({}) must be stopped before offline backup",
                profile.name, profile.id
            )));
        }
        if !profile_dir.exists() {
            return Err(DimError::BackupError(format!(
                "source profile directory does not exist: {}",
                profile.name
            )));
        }

        let external_paths = dim_core::external_paths_for_profile_all_layouts(profile_dir);
        if !external_paths.is_empty() && !allow_external_db_omit {
            return Err(DimError::ExternalDbOutsideProfile {
                profile_name: profile.name.clone(),
                external_paths: external_paths
                    .into_iter()
                    .map(|p| p.display().to_string())
                    .collect(),
            });
        }

        if let Some(custom_dest) = dest_dir {
            validate_directory_input(custom_dest)?;
            ensure_not_primary_zalo_path(custom_dest)?;
            if let Ok(meta) = fs::symlink_metadata(custom_dest) {
                if is_symlink_or_reparse(&meta) {
                    return Err(DimError::BackupError(
                        "backup destination cannot be a symlink or reparse point".to_owned(),
                    ));
                }
            }
        }
        let backups_root = dest_dir.unwrap_or_else(|| self.store.backups_dir());
        fs::create_dir_all(backups_root)?;
        let _ = sweep_abandoned_staging_dirs(backups_root, STAGING_MAX_AGE);

        let payload_estimate = estimate_regular_file_bytes(profile_dir)?;
        self.check_free_space(backups_root, payload_estimate)?;

        let slug = slugify_profile_name(&profile.name);
        let stamp = now_compact_utc();
        let backup_id = generate_profile_id();
        let base_name = format!("{slug}_{stamp}");
        let final_name = if backups_root.join(&base_name).exists() {
            format!("{base_name}_{backup_id}")
        } else {
            base_name
        };

        let partial_dir = resolve_safe_child(backups_root, &format!("{final_name}.partial"))?;
        let final_dir = resolve_safe_child(backups_root, &final_name)?;

        if partial_dir.exists() {
            fs::remove_dir_all(&partial_dir)?;
        }
        fs::create_dir_all(&partial_dir)?;

        let copy_res = copy_profile_payload_and_hash(profile_dir, &partial_dir);
        let (files, total_bytes) = match copy_res {
            Ok(res) => res,
            Err(err) => {
                let _ = fs::remove_dir_all(&partial_dir);
                return Err(err);
            }
        };

        let manifest = BackupManifest {
            schema_version: BACKUP_SCHEMA_VERSION,
            backup_id,
            created_at: now_rfc3339_utc(),
            source_profile_id: profile.id.clone(),
            source_profile_name: profile.name.clone(),
            source_directory_name: profile.directory_name.clone(),
            zalo_user_id: profile.zalo_user_id.clone(),
            file_count: files.len() as u64,
            total_bytes,
            files,
        };

        let omitted_strs: Vec<String> = external_paths
            .into_iter()
            .map(|p| p.display().to_string())
            .collect();
        let manifest_tmp = partial_dir.join("backup.json");
        if let Err(err) = write_manifest_synced_with_omissions(&manifest_tmp, &manifest, &omitted_strs) {
            let _ = fs::remove_dir_all(&partial_dir);
            return Err(err);
        }

        if let Err(err) = fs::rename(&partial_dir, &final_dir) {
            let _ = fs::remove_dir_all(&partial_dir);
            return Err(DimError::Io(err));
        }

        Ok(BackupResult {
            manifest_path: final_dir.join("backup.json"),
            backup_dir: final_dir,
            manifest,
        })
    }
}

impl BackupManager for FileBackupManager {
    fn backup_profile(
        &self,
        profile: &ProfileRecord,
        profile_dir: &Path,
        dest_dir: Option<&Path>,
        is_running: bool,
    ) -> Result<BackupResult, DimError> {
        self.backup_profile_with_options(profile, profile_dir, dest_dir, is_running, false)
    }

    fn verify_backup(&self, backup_dir: &Path) -> Result<BackupManifest, DimError> {
        validate_directory_input(backup_dir)?;
        ensure_not_primary_zalo_path(backup_dir)?;
        let dir_meta = fs::symlink_metadata(backup_dir).map_err(|_| {
            DimError::BackupError(format!(
                "backup directory missing: {}",
                backup_dir.display()
            ))
        })?;
        if is_symlink_or_reparse(&dir_meta) || !dir_meta.is_dir() {
            return Err(DimError::BackupError(format!(
                "backup directory is not a regular directory: {}",
                backup_dir.display()
            )));
        }
        let manifest_path = backup_dir.join("backup.json");
        if !manifest_path.exists() {
            return Err(DimError::BackupError(format!(
                "backup manifest missing: {}",
                manifest_path.display()
            )));
        }

        let raw = fs::read_to_string(&manifest_path)?;
        let manifest: BackupManifest = serde_json::from_str(&raw)?;
        if manifest.schema_version != BACKUP_SCHEMA_VERSION {
            return Err(DimError::BackupError(format!(
                "unsupported backup schema_version {}",
                manifest.schema_version
            )));
        }
        if manifest.file_count != manifest.files.len() as u64 {
            return Err(DimError::BackupError(format!(
                "file_count mismatch: manifest declared {}, found {}",
                manifest.file_count,
                manifest.files.len()
            )));
        }

        let mut observed_total = 0u64;
        for entry in &manifest.files {
            if entry.relative_path == "backup.json" {
                return Err(DimError::BackupError(
                    "backup manifest illegally lists backup.json as payload".to_owned(),
                ));
            }
            let file_path = resolve_safe_child(backup_dir, &entry.relative_path)?;
            let meta = fs::symlink_metadata(&file_path).map_err(|e| {
                DimError::BackupError(format!(
                    "missing backed-up file '{}': {e}",
                    entry.relative_path
                ))
            })?;
            if is_symlink_or_reparse(&meta) || !meta.is_file() {
                return Err(DimError::BackupError(format!(
                    "backed-up entry '{}' is not a regular file",
                    entry.relative_path
                )));
            }
            if meta.len() != entry.size_bytes {
                return Err(DimError::BackupError(format!(
                    "size mismatch for '{}': expected {}, got {}",
                    entry.relative_path,
                    entry.size_bytes,
                    meta.len()
                )));
            }
            let actual_hash = sha256_file(&file_path)?;
            if actual_hash != entry.sha256 {
                return Err(DimError::BackupError(format!(
                    "SHA-256 mismatch for '{}': expected {}, got {actual_hash}",
                    entry.relative_path, entry.sha256
                )));
            }
            observed_total = observed_total.saturating_add(entry.size_bytes);
        }

        if observed_total != manifest.total_bytes {
            return Err(DimError::BackupError(format!(
                "total_bytes mismatch: expected {}, got {observed_total}",
                manifest.total_bytes
            )));
        }

        Ok(manifest)
    }

    fn restore_backup_as_new_profile(
        &self,
        backup_dir: &Path,
        new_name: Option<&str>,
    ) -> Result<ProfileRecord, DimError> {
        let manifest = self.verify_backup(backup_dir)?;
        fs::create_dir_all(self.store.profiles_dir())?;
        let _ = sweep_abandoned_staging_dirs(self.store.profiles_dir(), STAGING_MAX_AGE);
        self.check_free_space(self.store.profiles_dir(), manifest.total_bytes)?;

        let existing = self.store.list_profiles()?;
        let desired_name =
            choose_unique_restored_name(new_name, &manifest.source_profile_name, &existing)?;

        let mut new_id = generate_profile_id();
        while existing.iter().any(|p| p.id == new_id) {
            new_id = generate_profile_id();
        }
        let stamp = now_compact_utc();
        let dir_name = build_directory_name(&desired_name, &new_id);
        let staging_dir = resolve_safe_child(
            self.store.profiles_dir(),
            &format!(".restore-{stamp}-{new_id}"),
        )?;
        let dest_profile_dir = resolve_safe_child(self.store.profiles_dir(), &dir_name)?;
        if dest_profile_dir.exists() {
            return Err(DimError::RestoreError(format!(
                "destination directory already exists: {}",
                dest_profile_dir.display()
            )));
        }
        if staging_dir.exists() {
            let _ = unlink_reparse_points_safe(&staging_dir);
            fs::remove_dir_all(&staging_dir)?;
        }
        fs::create_dir_all(&staging_dir)?;

        let restore_stage_res = (|| -> Result<(), DimError> {
            let mut copied_bytes = 0u64;
            let mut copied_count = 0u64;
            for entry in &manifest.files {
                let src = resolve_safe_child(backup_dir, &entry.relative_path)?;
                let dst = resolve_safe_child(&staging_dir, &entry.relative_path)?;
                if let Some(parent) = dst.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(&src, &dst)?;
                let dst_meta = fs::symlink_metadata(&dst)?;
                if dst_meta.len() != entry.size_bytes {
                    return Err(DimError::RestoreError(format!(
                        "staged file size mismatch for '{}'",
                        entry.relative_path
                    )));
                }
                let actual_hash = sha256_file(&dst)?;
                if actual_hash != entry.sha256 {
                    return Err(DimError::RestoreError(format!(
                        "staged SHA-256 mismatch for '{}'",
                        entry.relative_path
                    )));
                }
                copied_bytes = copied_bytes.saturating_add(entry.size_bytes);
                copied_count = copied_count.saturating_add(1);
            }
            if copied_count != manifest.file_count || copied_bytes != manifest.total_bytes {
                return Err(DimError::RestoreError(
                    "staged file count or byte total mismatch".to_owned(),
                ));
            }
            rewrite_profile_identity_markers(&staging_dir, &new_id)?;
            Ok(())
        })();

        if let Err(err) = restore_stage_res {
            let _ = unlink_reparse_points_safe(&staging_dir);
            let _ = fs::remove_dir_all(&staging_dir);
            return Err(err);
        }

        if let Err(err) = fs::rename(&staging_dir, &dest_profile_dir) {
            let _ = unlink_reparse_points_safe(&staging_dir);
            let _ = fs::remove_dir_all(&staging_dir);
            return Err(DimError::Io(err));
        }

        let detected_uid = detect_zalo_user_id(
            &dest_profile_dir
                .join("AppData")
                .join("Roaming")
                .join("ZaloData"),
        )
        .or(manifest.zalo_user_id);

        let record = ProfileRecord {
            id: new_id,
            name: desired_name,
            directory_name: dir_name,
            created_at: now_rfc3339_utc(),
            last_launched_at: None,
            zalo_user_id: detected_uid,
            notes: format!("Restored from backup {}", manifest.backup_id),
        };

        if let Err(err) = self.store.insert_restored_profile(record.clone()) {
            let _ = unlink_reparse_points_safe(&dest_profile_dir);
            let _ = fs::remove_dir_all(&dest_profile_dir);
            return Err(err);
        }

        Ok(record)
    }
}

/// Sweeps abandoned `.partial` or `.restore-*` staging directories older than `max_age`.
pub fn sweep_abandoned_staging_dirs(root: &Path, max_age: Duration) -> Result<usize, DimError> {
    if !root.exists() {
        return Ok(0);
    }
    let now = SystemTime::now();
    let mut swept = 0usize;
    for entry_res in fs::read_dir(root)? {
        let entry = entry_res?;
        let path = entry.path();
        let meta = fs::symlink_metadata(&path)?;
        if is_symlink_or_reparse(&meta) || !meta.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let Some(name_str) = name.to_str() else {
            continue;
        };
        if !name_str.ends_with(".partial") && !name_str.starts_with(".restore-") {
            continue;
        }
        let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let age = now.duration_since(modified).unwrap_or(Duration::ZERO);
        if age >= max_age {
            let _ = unlink_reparse_points_safe(&path)?;
            crate::store::verify_zero_reparse_points_remaining(&path)?;
            fs::remove_dir_all(&path)?;
            swept += 1;
        }
    }
    Ok(swept)
}

fn choose_unique_restored_name(
    requested: Option<&str>,
    source_name: &str,
    existing: &[ProfileRecord],
) -> Result<String, DimError> {
    if let Some(name) = requested {
        let valid = validate_profile_name(name)?;
        if existing.iter().any(|p| p.name.eq_ignore_ascii_case(&valid)) {
            return Err(DimError::DuplicateProfileName(valid));
        }
        return Ok(valid);
    }

    let base = format!("{source_name} (Restored)");
    let base_trimmed: String = base.chars().take(dim_core::MAX_PROFILE_NAME_LEN).collect();
    if !existing
        .iter()
        .any(|p| p.name.eq_ignore_ascii_case(&base_trimmed))
    {
        return Ok(base_trimmed);
    }

    for idx in 2..=999 {
        let suffix = format!(" (Restored #{idx})");
        let max_prefix = dim_core::MAX_PROFILE_NAME_LEN.saturating_sub(suffix.len());
        let prefix: String = source_name.chars().take(max_prefix).collect();
        let candidate = format!("{prefix}{suffix}");
        if !existing
            .iter()
            .any(|p| p.name.eq_ignore_ascii_case(&candidate))
        {
            return Ok(candidate);
        }
    }

    Err(DimError::RestoreError(
        "could not allocate unique restored profile name".to_owned(),
    ))
}

fn rewrite_profile_identity_markers(profile_dir: &Path, new_id: &str) -> Result<(), DimError> {
    let candidates = [
        profile_dir.join("AppData").join("Roaming").join("ZaloData"),
        profile_dir
            .join("Library")
            .join("Application Support")
            .join("ZaloData"),
        profile_dir.join(".config").join("ZaloData"),
    ];
    let mut updated_any = false;
    for dir in candidates {
        if dir.exists() || dir.join(ZALO_PROFILE_ID_FILE).exists() {
            write_profile_id_marker(&dir, new_id)?;
            updated_any = true;
        }
    }
    if !updated_any {
        write_profile_id_marker(
            &profile_dir.join("AppData").join("Roaming").join("ZaloData"),
            new_id,
        )?;
    }
    Ok(())
}

fn write_manifest_synced_with_omissions(
    path: &Path,
    manifest: &BackupManifest,
    omitted_external_paths: &[String],
) -> Result<(), DimError> {
    let mut val = serde_json::to_value(manifest)?;
    if !omitted_external_paths.is_empty() {
        if let Some(obj) = val.as_object_mut() {
            obj.insert(
                "external_db_omitted".to_owned(),
                serde_json::Value::Bool(true),
            );
            obj.insert(
                "omitted_external_paths".to_owned(),
                serde_json::to_value(omitted_external_paths)?,
            );
        }
    }
    let json = serde_json::to_string_pretty(&val)?;
    let mut f = File::create(path)?;
    f.write_all(json.as_bytes())?;
    f.write_all(b"\n")?;
    f.sync_all()?;
    Ok(())
}

fn estimate_regular_file_bytes(root: &Path) -> Result<u64, DimError> {
    let mut total = 0u64;
    let mut stack = vec![PathBuf::new()];
    while let Some(rel) = stack.pop() {
        let dir = if rel.as_os_str().is_empty() {
            root.to_path_buf()
        } else {
            root.join(&rel)
        };
        for entry_res in fs::read_dir(&dir)? {
            let entry = entry_res?;
            let path = entry.path();
            let meta = fs::symlink_metadata(&path)?;
            if is_symlink_or_reparse(&meta) {
                continue;
            }
            let child_rel = if rel.as_os_str().is_empty() {
                PathBuf::from(entry.file_name())
            } else {
                rel.join(entry.file_name())
            };
            if should_skip_backup_rel(&child_rel) {
                continue;
            }
            if meta.is_dir() {
                stack.push(child_rel);
            } else if meta.is_file() {
                total = total.saturating_add(meta.len());
            }
        }
    }
    Ok(total)
}

fn copy_profile_payload_and_hash(
    src_root: &Path,
    dst_root: &Path,
) -> Result<(Vec<BackupFileEntry>, u64), DimError> {
    let mut entries = Vec::new();
    let mut total_bytes = 0u64;
    let mut stack = vec![PathBuf::new()];

    while let Some(rel) = stack.pop() {
        let src_dir = if rel.as_os_str().is_empty() {
            src_root.to_path_buf()
        } else {
            src_root.join(&rel)
        };
        for entry_res in fs::read_dir(&src_dir)? {
            let entry = entry_res?;
            let src_path = entry.path();
            let meta = fs::symlink_metadata(&src_path)?;
            if is_symlink_or_reparse(&meta) {
                continue;
            }
            let child_rel = if rel.as_os_str().is_empty() {
                PathBuf::from(entry.file_name())
            } else {
                rel.join(entry.file_name())
            };
            if should_skip_backup_rel(&child_rel) {
                continue;
            }

            if meta.is_dir() {
                stack.push(child_rel);
            } else if meta.is_file() {
                let rel_norm = normalize_rel_path(&child_rel);
                let dst_path = resolve_safe_child(dst_root, &rel_norm)?;
                if let Some(parent) = dst_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(&src_path, &dst_path)?;
                let digest = sha256_file(&dst_path)?;
                let size = meta.len();
                total_bytes = total_bytes.saturating_add(size);
                entries.push(BackupFileEntry {
                    relative_path: rel_norm,
                    size_bytes: size,
                    sha256: digest,
                });
            }
        }
    }

    entries.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    Ok((entries, total_bytes))
}

fn should_skip_backup_rel(rel: &Path) -> bool {
    let norm = normalize_rel_path(rel);
    let file_name = rel.file_name().and_then(|s| s.to_str()).unwrap_or_default();
    matches!(
        file_name,
        "lockfile"
            | "SingletonLock"
            | ".quit-signal"
            | ".focus-signal"
            | ".runtime-pid"
            | ".runtime-lease.json"
            | ".dim-profile.lock"
    ) || norm == "AppData/Local/Temp"
        || norm.starts_with("AppData/Local/Temp/")
        || norm == "tmp"
        || norm.starts_with("tmp/")
}

fn normalize_rel_path(rel: &Path) -> String {
    rel.components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect::<Vec<_>>()
        .join("/")
}

fn sha256_file(path: &Path) -> Result<String, DimError> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65_536];
    loop {
        let read = file.read(&mut buf)?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }
    Ok(encode_hex_lower(&hasher.finalize()))
}

fn encode_hex_lower(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(&mut out, "{b:02x}");
    }
    out
}
