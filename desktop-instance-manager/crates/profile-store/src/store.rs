//! Atomic `profiles.json` store with cross-process advisory locking (`profiles.lock`),
//! fail-closed corrupted metadata detection, `.zalo-profile-id` verification, and
//! defense-in-depth destructive deletion guards.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use dim_core::{
    build_directory_name, check_profile_lockfile_activity, clean_stale_temp_files,
    ensure_not_primary_zalo_path, generate_profile_id, now_rfc3339_utc, resolve_data_dir,
    resolve_safe_child, validate_directory_input, validate_profile_name, verify_runtime_lease,
    write_bytes_atomic, AdvisoryFileLock, DimError, LeaseVerification, ProfileLockActivity,
    ProfileRecord, ProfileStore, ProfilesDocument, DEFAULT_ADVISORY_LOCK_TIMEOUT,
    PROFILES_LOCK_FILE, PROFILES_SCHEMA_VERSION, PROFILE_OP_LOCK_FILE, ZALO_PROFILE_ID_FILE,
};
use serde::de::Error as _;

/// File-backed `profiles.json` store protected by both an intra-process mutex and a cross-process
/// OS advisory lock (`<data_dir>/profiles.lock`), committing via unique same-directory `.tmp`
/// files and atomic OS replacement.
#[derive(Debug)]
pub struct JsonProfileStore {
    data_dir: PathBuf,
    profiles_dir: PathBuf,
    backups_dir: PathBuf,
    metadata_path: PathBuf,
    lock_path: PathBuf,
    lock_timeout: Duration,
    lock: Mutex<()>,
}

impl JsonProfileStore {
    /// Creates a store rooted at `data_dir`.
    pub fn new(data_dir: PathBuf) -> Result<Self, DimError> {
        validate_directory_input(&data_dir)?;
        ensure_not_primary_zalo_path(&data_dir)?;
        let profiles_dir = data_dir.join("Profiles");
        let backups_dir = data_dir.join("Backups");
        let metadata_path = data_dir.join("profiles.json");
        let lock_path = data_dir.join(PROFILES_LOCK_FILE);
        Ok(Self {
            data_dir,
            profiles_dir,
            backups_dir,
            metadata_path,
            lock_path,
            lock_timeout: DEFAULT_ADVISORY_LOCK_TIMEOUT,
            lock: Mutex::new(()),
        })
    }

    /// Creates a store using `override_dir`, `DIM_DATA_DIR`, or the OS default data directory.
    pub fn from_default_or_env(override_dir: Option<&Path>) -> Result<Self, DimError> {
        let dir = resolve_data_dir(override_dir)?;
        Self::new(dir)
    }

    /// Configures the maximum duration to wait when `<data_dir>/profiles.lock` is contended.
    #[must_use]
    pub fn with_lock_timeout(mut self, timeout: Duration) -> Self {
        self.lock_timeout = timeout;
        self
    }

    /// Returns the storage root directory (`<data_dir>`).
    #[must_use]
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// Returns `<data_dir>/Profiles`.
    #[must_use]
    pub fn profiles_dir(&self) -> &Path {
        &self.profiles_dir
    }

    /// Returns `<data_dir>/Backups`.
    #[must_use]
    pub fn backups_dir(&self) -> &Path {
        &self.backups_dir
    }

    /// Returns `<data_dir>/profiles.json`.
    #[must_use]
    pub fn metadata_path(&self) -> &Path {
        &self.metadata_path
    }

    /// Returns `<data_dir>/profiles.lock`.
    #[must_use]
    pub fn lock_path(&self) -> &Path {
        &self.lock_path
    }

    /// Ensures `<data_dir>`, `Profiles/`, `Backups/`, and `profiles.json` exist.
    ///
    /// If `profiles.json` already exists on disk, validates that it is readable and uncorrupted
    /// without ever overwriting or resetting corrupted files.
    pub fn ensure_initialized(&self) -> Result<(), DimError> {
        self.with_store_locks(|store| {
            store.ensure_initialized_locked()?;
            let _ = store.read_document_locked()?;
            Ok(())
        })
    }

    /// Resolves a profile's isolated directory safely inside `<data_dir>/Profiles`.
    pub fn profile_dir_for(&self, record: &ProfileRecord) -> Result<PathBuf, DimError> {
        resolve_safe_child(&self.profiles_dir, &record.directory_name)
    }

    /// Updates only `zalo_user_id` for a profile inside a cross-process locked atomic transaction.
    pub fn set_zalo_user_id(
        &self,
        id: &str,
        zalo_user_id: Option<&str>,
    ) -> Result<ProfileRecord, DimError> {
        self.with_store_locks(|store| {
            store.ensure_initialized_locked()?;
            let mut doc = store.read_document_locked()?;
            let idx = find_profile_index(&doc.profiles, id)
                .ok_or_else(|| DimError::ProfileNotFound(id.to_owned()))?;
            doc.profiles[idx].zalo_user_id = zalo_user_id.map(str::to_owned);
            let updated = doc.profiles[idx].clone();
            store.write_document_atomic_locked(&doc)?;
            Ok(updated)
        })
    }

    /// Inserts an already-constructed profile record (used by restore-to-new-profile).
    pub fn insert_restored_profile(
        &self,
        record: ProfileRecord,
    ) -> Result<ProfileRecord, DimError> {
        self.with_store_locks(|store| {
            store.ensure_initialized_locked()?;
            let mut doc = store.read_document_locked()?;
            if doc
                .profiles
                .iter()
                .any(|p| profile_names_eq_case_insensitive(&p.name, &record.name))
            {
                return Err(DimError::DuplicateProfileName(record.name));
            }
            doc.profiles.push(record.clone());
            store.write_document_atomic_locked(&doc)?;
            Ok(record)
        })
    }

    /// Deletes a profile after enforcing `confirm`, caller-observed `is_running`, AND independent
    /// backend runtime ownership checks (`.dim-profile.lock`, `.runtime-lease.json`, `lockfile`,
    /// `SingletonLock`, `.runtime-pid`), canonical `<data_dir>/Profiles` path containment,
    /// `PROTECTED_PRIMARY_PATH` exclusion, and verified two-phase reparse-point unlinking.
    pub fn delete_profile_with_guard(
        &self,
        id_or_name: &str,
        confirm: bool,
        is_running: bool,
        unlink_junctions: &dyn Fn(&Path) -> Result<usize, DimError>,
    ) -> Result<(), DimError> {
        if !confirm {
            return Err(DimError::ConfirmationRequired(
                "deleting a profile requires explicit confirmation (--confirm)".to_owned(),
            ));
        }
        if is_running {
            return Err(DimError::ProfileRunning(id_or_name.to_owned()));
        }

        self.with_store_locks(|store| {
            store.ensure_initialized_locked()?;
            let mut doc = store.read_document_locked()?;
            let idx = find_profile_index(&doc.profiles, id_or_name)
                .ok_or_else(|| DimError::ProfileNotFound(id_or_name.to_owned()))?;
            let candidate = doc.profiles[idx].clone();

            // 1. Verify Backups root integrity before any destructive operation.
            if let Ok(bk_meta) = fs::symlink_metadata(&store.backups_dir) {
                if is_symlink_or_reparse(&bk_meta) || !bk_meta.is_dir() {
                    return Err(DimError::BackupError(format!(
                        "refusing destructive delete: Backups directory '{}' is not a regular directory",
                        store.backups_dir.display()
                    )));
                }
            }

            // 2. Strict containment inside `<data_dir>/Profiles` & non-overlap with primary ZaloData or Backups.
            let profile_dir = store.profile_dir_for(&candidate)?;
            ensure_not_primary_zalo_path(&profile_dir)?;
            if profile_dir.starts_with(&store.backups_dir) || profile_dir == store.profiles_dir {
                return Err(DimError::PathTraversal(format!(
                    "refused unsafe profile deletion path '{}'",
                    profile_dir.display()
                )));
            }
            if let Ok(canon_profiles_root) = fs::canonicalize(&store.profiles_dir) {
                if let Some(parent) = profile_dir.parent() {
                    if let Ok(canon_parent) = fs::canonicalize(parent) {
                        if canon_parent != canon_profiles_root {
                            return Err(DimError::PathTraversal(format!(
                                "profile parent '{}' escapes canonical Profiles root '{}'",
                                canon_parent.display(),
                                canon_profiles_root.display()
                            )));
                        }
                    }
                }
            }

            // 3. Independent filesystem/OS runtime verification & verified two-phase reparse-safe removal.
            if let Ok(root_meta) = fs::symlink_metadata(&profile_dir) {
                if is_symlink_or_reparse(&root_meta) {
                    remove_reparse_or_symlink(&profile_dir, &root_meta)?;
                } else if root_meta.is_dir() {
                    let op_lock_path = profile_dir.join(PROFILE_OP_LOCK_FILE);
                    let Some(profile_op_guard) =
                        AdvisoryFileLock::try_acquire_exclusive(&op_lock_path)?
                    else {
                        return Err(DimError::ProfileRunning(format!(
                            "profile '{}' ({}) is currently locked by another active process (.dim-profile.lock)",
                            candidate.name, candidate.id
                        )));
                    };

                    // Verify identity markers inside any existing OS ZaloData layout before deleting.
                    for zdir in candidate_zalo_data_dirs(&profile_dir) {
                        ensure_not_primary_zalo_path(&zdir)?;
                        verify_profile_id_marker(&zdir, &candidate.id, false)?;
                    }

                    // Independently verify absence of any live or indeterminate `.runtime-lease.json`.
                    match verify_runtime_lease(&profile_dir, &candidate.id, &[]) {
                        LeaseVerification::Absent | LeaseVerification::StaleDead(_) => {}
                        LeaseVerification::VerifiedLive(lease) => {
                            return Err(DimError::ProfileRunning(format!(
                                "profile '{}' ({}) is running (verified runtime lease PID {})",
                                candidate.name, candidate.id, lease.pid
                            )));
                        }
                        LeaseVerification::InvalidOrIndeterminate(reason) => {
                            return Err(DimError::ProfileRunning(format!(
                                "refused to delete profile '{}' ({}): indeterminate runtime lease ({reason})",
                                candidate.name, candidate.id
                            )));
                        }
                    }

                    // Independently verify absence of any OS-locked or live `lockfile` / `SingletonLock` / `.runtime-pid`.
                    match check_profile_lockfile_activity(&profile_dir) {
                        ProfileLockActivity::Inactive => {}
                        ProfileLockActivity::Active => {
                            return Err(DimError::ProfileRunning(format!(
                                "profile '{}' ({}) has an active or OS-locked lockfile",
                                candidate.name, candidate.id
                            )));
                        }
                        ProfileLockActivity::Indeterminate => {
                            return Err(DimError::ProfileRunning(format!(
                                "refused to delete profile '{}' ({}): indeterminate lockfile ownership",
                                candidate.name, candidate.id
                            )));
                        }
                    }

                    // Two-phase reparse-point unlinking: unlink all junctions/symlinks, then verify
                    // ZERO reparse points remain anywhere in `<profile_dir>` BEFORE deleting contents!
                    let _ = unlink_junctions(&profile_dir)?;
                    verify_zero_reparse_points_remaining(&profile_dir)?;
                    remove_dir_contents_preserving_file(&profile_dir, PROFILE_OP_LOCK_FILE)?;

                    // Release handle to `.dim-profile.lock` so Windows `RemoveDirectoryW` removes `profile_dir` cleanly.
                    drop(profile_op_guard);
                    let _ = fs::remove_file(&op_lock_path);
                    fs::remove_dir(&profile_dir)?;
                } else {
                    fs::remove_file(&profile_dir)?;
                }
            }

            doc.profiles.remove(idx);
            store.write_document_atomic_locked(&doc)?;
            Ok(())
        })
    }

    fn with_store_locks<T, F>(&self, op: F) -> Result<T, DimError>
    where
        F: FnOnce(&Self) -> Result<T, DimError>,
    {
        let _thread_guard = self
            .lock
            .lock()
            .map_err(|_| DimError::Io(io::Error::other("profile store mutex poisoned")))?;
        fs::create_dir_all(&self.data_dir)?;
        let _file_guard = AdvisoryFileLock::acquire_exclusive(&self.lock_path, self.lock_timeout)?;
        op(self)
    }

    fn ensure_initialized_locked(&self) -> Result<(), DimError> {
        fs::create_dir_all(&self.data_dir)?;
        fs::create_dir_all(&self.profiles_dir)?;
        fs::create_dir_all(&self.backups_dir)?;

        match fs::symlink_metadata(&self.metadata_path) {
            Ok(_) => {
                // Existing `profiles.json` MUST NEVER be overwritten or reset during initialization.
                Ok(())
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                let empty = ProfilesDocument::default();
                self.write_document_atomic_locked(&empty)
            }
            Err(err) => Err(DimError::Io(err)),
        }
    }

    fn read_document_locked(&self) -> Result<ProfilesDocument, DimError> {
        let meta = fs::symlink_metadata(&self.metadata_path)?;
        if is_symlink_or_reparse(&meta) || meta.is_dir() {
            return Err(corrupt_profiles_json_error(
                &self.metadata_path,
                "profiles.json cannot be a directory, symlink, or reparse point",
            ));
        }

        let raw_bytes = fs::read(&self.metadata_path)?;
        if raw_bytes.is_empty() || raw_bytes.iter().all(u8::is_ascii_whitespace) {
            return Err(corrupt_profiles_json_error(
                &self.metadata_path,
                "file is empty (0 bytes or whitespace-only)",
            ));
        }

        let raw_text = String::from_utf8(raw_bytes).map_err(|e| {
            corrupt_profiles_json_error(&self.metadata_path, &format!("invalid UTF-8 bytes ({e})"))
        })?;

        let val: serde_json::Value = serde_json::from_str(&raw_text).map_err(|e| {
            corrupt_profiles_json_error(&self.metadata_path, &format!("invalid JSON syntax ({e})"))
        })?;

        let Some(obj) = val.as_object() else {
            return Err(corrupt_profiles_json_error(
                &self.metadata_path,
                "root JSON value must be an object",
            ));
        };

        let Some(schema_val) = obj.get("schema_version") else {
            return Err(DimError::SchemaError(format!(
                "missing 'schema_version' in '{}'; original file preserved untouched for recovery",
                self.metadata_path.display()
            )));
        };

        let Some(version_u64) = schema_val.as_u64() else {
            return Err(DimError::SchemaError(format!(
                "invalid non-integer 'schema_version' in '{}'; original file preserved untouched for recovery",
                self.metadata_path.display()
            )));
        };

        if version_u64 != u64::from(PROFILES_SCHEMA_VERSION) {
            return Err(DimError::SchemaError(format!(
                "unsupported profiles.json schema_version {version_u64} in '{}' (expected {PROFILES_SCHEMA_VERSION}); original file preserved untouched for recovery",
                self.metadata_path.display()
            )));
        }

        let doc: ProfilesDocument = serde_json::from_value(val).map_err(|e| {
            corrupt_profiles_json_error(
                &self.metadata_path,
                &format!("malformed profile record schema ({e})"),
            )
        })?;
        Ok(doc)
    }

    fn write_document_atomic_locked(&self, doc: &ProfilesDocument) -> Result<(), DimError> {
        let _ = clean_stale_temp_files(&self.data_dir, "profiles.json");
        let mut json = serde_json::to_vec_pretty(doc)?;
        json.push(b'\n');
        write_bytes_atomic(&self.metadata_path, &json)
    }
}

impl ProfileStore for JsonProfileStore {
    fn load(&self) -> Result<ProfilesDocument, DimError> {
        self.with_store_locks(|store| {
            store.ensure_initialized_locked()?;
            store.read_document_locked()
        })
    }

    fn list_profiles(&self) -> Result<Vec<ProfileRecord>, DimError> {
        Ok(self.load()?.profiles)
    }

    fn get_profile(&self, id_or_name: &str) -> Result<ProfileRecord, DimError> {
        let doc = self.load()?;
        let idx = find_profile_index(&doc.profiles, id_or_name)
            .ok_or_else(|| DimError::ProfileNotFound(id_or_name.to_owned()))?;
        Ok(doc.profiles[idx].clone())
    }

    fn create_profile(&self, name: &str, notes: &str) -> Result<ProfileRecord, DimError> {
        let clean_name = validate_profile_name(name)?;
        self.with_store_locks(|store| {
            store.ensure_initialized_locked()?;
            let mut doc = store.read_document_locked()?;

            if doc
                .profiles
                .iter()
                .any(|p| profile_names_eq_case_insensitive(&p.name, &clean_name))
            {
                return Err(DimError::DuplicateProfileName(clean_name));
            }

            let mut id = generate_profile_id();
            while doc.profiles.iter().any(|p| p.id == id) {
                id = generate_profile_id();
            }
            let directory_name = build_directory_name(&clean_name, &id);
            let profile_dir = resolve_safe_child(&store.profiles_dir, &directory_name)?;
            let dir_existed_before = profile_dir.exists();
            fs::create_dir_all(&profile_dir)?;

            let record = ProfileRecord {
                id,
                name: clean_name,
                directory_name,
                created_at: now_rfc3339_utc(),
                last_launched_at: None,
                zalo_user_id: None,
                notes: notes.trim().to_owned(),
            };

            doc.profiles.push(record.clone());
            if let Err(err) = store.write_document_atomic_locked(&doc) {
                if !dir_existed_before {
                    let _ = fs::remove_dir_all(&profile_dir);
                }
                return Err(err);
            }
            Ok(record)
        })
    }

    fn rename_profile(&self, id_or_name: &str, new_name: &str) -> Result<ProfileRecord, DimError> {
        let clean_name = validate_profile_name(new_name)?;
        self.with_store_locks(|store| {
            store.ensure_initialized_locked()?;
            let mut doc = store.read_document_locked()?;

            let idx = find_profile_index(&doc.profiles, id_or_name)
                .ok_or_else(|| DimError::ProfileNotFound(id_or_name.to_owned()))?;

            if doc
                .profiles
                .iter()
                .enumerate()
                .any(|(i, p)| i != idx && profile_names_eq_case_insensitive(&p.name, &clean_name))
            {
                return Err(DimError::DuplicateProfileName(clean_name));
            }

            doc.profiles[idx].name = clean_name;
            let updated = doc.profiles[idx].clone();
            store.write_document_atomic_locked(&doc)?;
            Ok(updated)
        })
    }

    fn delete_profile(&self, id_or_name: &str, confirm: bool) -> Result<(), DimError> {
        // `delete_profile_with_guard` independently verifies `.dim-profile.lock`,
        // `.runtime-lease.json`, `lockfile`, `SingletonLock`, and `.runtime-pid` ownership
        // before unlinking any files or modifying `profiles.json`.
        self.delete_profile_with_guard(id_or_name, confirm, false, &unlink_reparse_points_safe)
    }

    fn update_last_launched(
        &self,
        id: &str,
        timestamp_rfc3339: &str,
        zalo_user_id: Option<&str>,
    ) -> Result<ProfileRecord, DimError> {
        self.with_store_locks(|store| {
            store.ensure_initialized_locked()?;
            let mut doc = store.read_document_locked()?;
            let idx = find_profile_index(&doc.profiles, id)
                .ok_or_else(|| DimError::ProfileNotFound(id.to_owned()))?;

            doc.profiles[idx].last_launched_at = Some(timestamp_rfc3339.to_owned());
            if let Some(uid) = zalo_user_id {
                doc.profiles[idx].zalo_user_id = Some(uid.to_owned());
            }
            let updated = doc.profiles[idx].clone();
            store.write_document_atomic_locked(&doc)?;
            Ok(updated)
        })
    }
}

fn corrupt_profiles_json_error(path: &Path, detail: &str) -> DimError {
    DimError::Json(serde_json::Error::custom(format!(
        "corrupted metadata in '{}': {detail}; original file preserved untouched for recovery (restore from backup or repair JSON manually)",
        path.display()
    )))
}

fn candidate_zalo_data_dirs(profile_dir: &Path) -> [PathBuf; 3] {
    [
        profile_dir.join("AppData").join("Roaming").join("ZaloData"),
        profile_dir
            .join("Library")
            .join("Application Support")
            .join("ZaloData"),
        profile_dir.join(".config").join("ZaloData"),
    ]
}

/// Writes `<zalo_data_dir>/.zalo-profile-id` containing the profile's 8-hex ID.
pub fn write_profile_id_marker(zalo_data_dir: &Path, profile_id: &str) -> Result<(), DimError> {
    fs::create_dir_all(zalo_data_dir)?;
    let marker = zalo_data_dir.join(ZALO_PROFILE_ID_FILE);
    write_bytes_atomic(&marker, profile_id.as_bytes())
}

/// Verifies `<zalo_data_dir>/.zalo-profile-id` matches `expected_id`.
pub fn verify_profile_id_marker(
    zalo_data_dir: &Path,
    expected_id: &str,
    require_present: bool,
) -> Result<(), DimError> {
    let marker = zalo_data_dir.join(ZALO_PROFILE_ID_FILE);
    if !marker.exists() {
        if require_present {
            return Err(DimError::ProfileIdentityMismatch {
                expected: expected_id.to_owned(),
                actual: "<missing>".to_owned(),
            });
        }
        return Ok(());
    }
    let actual = fs::read_to_string(marker)?.trim().to_owned();
    if !actual.eq_ignore_ascii_case(expected_id) {
        return Err(DimError::ProfileIdentityMismatch {
            expected: expected_id.to_owned(),
            actual,
        });
    }
    Ok(())
}

/// Returns true when metadata represents a POSIX symlink or Windows reparse point/junction.
#[must_use]
pub fn is_symlink_or_reparse(meta: &fs::Metadata) -> bool {
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

fn is_windows_dir_reparse(meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
        if meta.is_dir() || (meta.file_attributes() & FILE_ATTRIBUTE_DIRECTORY) != 0 {
            return true;
        }
    }
    #[cfg(not(windows))]
    {
        let _ = meta;
    }
    false
}

fn remove_reparse_or_symlink(path: &Path, meta: &fs::Metadata) -> Result<(), DimError> {
    if is_windows_dir_reparse(meta) || meta.is_dir() {
        fs::remove_dir(path)?;
    } else {
        fs::remove_file(path)?;
    }
    Ok(())
}

/// Unlinks any symlinks or directory junctions inside `root` without deleting target contents.
pub fn unlink_reparse_points_safe(root: &Path) -> Result<usize, DimError> {
    if fs::symlink_metadata(root).is_err() {
        return Ok(0);
    }
    let mut unlinked = 0usize;
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(DimError::Io(e)),
        };
        for entry_res in entries {
            let entry = entry_res?;
            let path = entry.path();
            let meta = fs::symlink_metadata(&path)?;
            if is_symlink_or_reparse(&meta) {
                remove_reparse_or_symlink(&path, &meta)?;
                unlinked += 1;
            } else if meta.is_dir() {
                stack.push(path);
            }
        }
    }
    Ok(unlinked)
}

/// Verifies that zero symlinks or NTFS reparse points/junctions remain anywhere under `root`
/// prior to recursive file deletion (`IR-WIN-03` two-phase unlink verification).
pub fn verify_zero_reparse_points_remaining(root: &Path) -> Result<(), DimError> {
    let root_meta = match fs::symlink_metadata(root) {
        Ok(m) => m,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(DimError::Io(e)),
    };
    if is_symlink_or_reparse(&root_meta) {
        return Err(DimError::Io(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "aborted recursive delete: root '{}' is still a symlink or reparse point",
                root.display()
            ),
        )));
    }
    if !root_meta.is_dir() {
        return Ok(());
    }

    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry_res in fs::read_dir(&dir)? {
            let entry = entry_res?;
            let path = entry.path();
            let meta = fs::symlink_metadata(&path)?;
            if is_symlink_or_reparse(&meta) {
                return Err(DimError::Io(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    format!(
                        "aborted recursive delete: remaining reparse point or symlink '{}' was not unlinked",
                        path.display()
                    ),
                )));
            }
            if meta.is_dir() {
                stack.push(path);
            }
        }
    }
    Ok(())
}

fn remove_dir_contents_preserving_file(
    dir: &Path,
    preserved_root_filename: &str,
) -> Result<(), DimError> {
    for entry_res in fs::read_dir(dir)? {
        let entry = entry_res?;
        if entry.file_name().to_str() == Some(preserved_root_filename) {
            continue;
        }
        let path = entry.path();
        let meta = fs::symlink_metadata(&path)?;
        if is_symlink_or_reparse(&meta) {
            return Err(DimError::Io(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "aborted delete: unexpected reparse point '{}' encountered during content removal",
                    path.display()
                ),
            )));
        } else if meta.is_dir() {
            remove_dir_without_following_links(&path)?;
        } else {
            fs::remove_file(&path)?;
        }
    }
    Ok(())
}

fn remove_dir_without_following_links(dir: &Path) -> Result<(), DimError> {
    let root_meta = match fs::symlink_metadata(dir) {
        Ok(m) => m,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(DimError::Io(e)),
    };
    if is_symlink_or_reparse(&root_meta) {
        return Err(DimError::Io(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "aborted delete: unexpected reparse point '{}' encountered during subdirectory removal",
                dir.display()
            ),
        )));
    }
    for entry_res in fs::read_dir(dir)? {
        let entry = entry_res?;
        let path = entry.path();
        let meta = fs::symlink_metadata(&path)?;
        if is_symlink_or_reparse(&meta) {
            return Err(DimError::Io(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "aborted delete: unexpected reparse point '{}' encountered during file removal",
                    path.display()
                ),
            )));
        } else if meta.is_dir() {
            remove_dir_without_following_links(&path)?;
        } else {
            fs::remove_file(&path)?;
        }
    }
    fs::remove_dir(dir)?;
    Ok(())
}

pub(crate) fn profile_names_eq_case_insensitive(a: &str, b: &str) -> bool {
    a.trim()
        .chars()
        .flat_map(char::to_lowercase)
        .eq(b.trim().chars().flat_map(char::to_lowercase))
}

fn find_profile_index(profiles: &[ProfileRecord], needle: &str) -> Option<usize> {
    let trimmed = needle.trim();
    if trimmed.is_empty() {
        return None;
    }
    profiles
        .iter()
        .position(|p| p.id.eq_ignore_ascii_case(trimmed))
        .or_else(|| {
            profiles
                .iter()
                .position(|p| profile_names_eq_case_insensitive(&p.name, trimmed))
        })
}
