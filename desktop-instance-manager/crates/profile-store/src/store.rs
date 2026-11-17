//! Atomic `profiles.json` store with cross-process advisory locking,
//! `.zalo-profile-id` verification, and junction-safe cleanup.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use dim_core::{
    build_directory_name, ensure_not_primary_zalo_path, generate_profile_id, now_rfc3339_utc,
    resolve_data_dir, resolve_safe_child, validate_directory_input, validate_profile_name,
    AdvisoryFileLock, DimError, ProfileRecord, ProfileStore, ProfilesDocument,
    PROFILES_LOCK_FILE, PROFILES_SCHEMA_VERSION, ZALO_PROFILE_ID_FILE,
    clean_stale_temp_files, write_bytes_atomic, DEFAULT_ADVISORY_LOCK_TIMEOUT,
};

/// File-backed `profiles.json` store using cross-process advisory file locking
/// and atomic unique-tmp-file replacement.
///
/// # Concurrency Safety
///
/// - **In-process:** Guarded by `Mutex<()>` preventing concurrent store mutations within
///   the same `JsonProfileStore` instance.
/// - **Cross-process:** Guarded by exclusive OS advisory file lock on `<data_dir>/profiles.lock`
///   (`LockFileEx` on Windows, `flock` on Unix) held across the entire read-modify-write cycle.
/// - **Atomic writes:** `profiles.json` is written via `write_bytes_atomic` (unique
///   `profiles.json.<pid>.<hex>.tmp` → `sync_all()` → `MoveFileExW MOVEFILE_REPLACE_EXISTING |
///   MOVEFILE_WRITE_THROUGH` on Windows / `fs::rename` on Unix). On failure, the unique `.tmp`
///   is removed and the previous `profiles.json` is left completely untouched.
#[derive(Debug)]
pub struct JsonProfileStore {
    data_dir: PathBuf,
    profiles_dir: PathBuf,
    backups_dir: PathBuf,
    metadata_path: PathBuf,
    lock_path: PathBuf,
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
            lock: Mutex::new(()),
        })
    }

    /// Creates a store using `override_dir`, `DIM_DATA_DIR`, or the OS default data directory.
    pub fn from_default_or_env(override_dir: Option<&Path>) -> Result<Self, DimError> {
        let dir = resolve_data_dir(override_dir)?;
        Self::new(dir)
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

    /// Ensures `<data_dir>`, `Profiles/`, `Backups/`, and `profiles.json` exist.
    pub fn ensure_initialized(&self) -> Result<(), DimError> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| DimError::Io(std::io::Error::other("profile store mutex poisoned")))?;
        let _fs_lock = self.acquire_cross_process_lock()?;
        self.ensure_initialized_locked()
    }

    /// Resolves a profile's isolated directory safely inside `<data_dir>/Profiles`.
    pub fn profile_dir_for(&self, record: &ProfileRecord) -> Result<PathBuf, DimError> {
        resolve_safe_child(&self.profiles_dir, &record.directory_name)
    }

    /// Inserts an already-constructed profile record (used by restore-to-new-profile).
    pub fn insert_restored_profile(
        &self,
        record: ProfileRecord,
    ) -> Result<ProfileRecord, DimError> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| DimError::Io(std::io::Error::other("profile store mutex poisoned")))?;
        let _fs_lock = self.acquire_cross_process_lock()?;
        self.ensure_initialized_locked()?;
        let mut doc = self.read_document_locked()?;
        if doc
            .profiles
            .iter()
            .any(|p| p.name.eq_ignore_ascii_case(&record.name))
        {
            return Err(DimError::DuplicateProfileName(record.name));
        }
        doc.profiles.push(record.clone());
        self.write_document_atomic_locked(&doc)?;
        Ok(record)
    }

    /// Deletes a profile after enforcing `confirm`, stopped state, and pre-delete junction unlinking.
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

        let _guard = self
            .lock
            .lock()
            .map_err(|_| DimError::Io(std::io::Error::other("profile store mutex poisoned")))?;
        let _fs_lock = self.acquire_cross_process_lock()?;
        self.ensure_initialized_locked()?;
        let mut doc = self.read_document_locked()?;
        let idx = find_profile_index(&doc.profiles, id_or_name)
            .ok_or_else(|| DimError::ProfileNotFound(id_or_name.to_owned()))?;
        let removed = doc.profiles.remove(idx);

        let profile_dir = self.profile_dir_for(&removed)?;
        if fs::symlink_metadata(&profile_dir).is_ok() {
            let _ = unlink_junctions(&profile_dir)?;
            remove_dir_without_following_links(&profile_dir)?;
        }

        self.write_document_atomic_locked(&doc)?;
        Ok(())
    }

    /// Acquires the cross-process advisory lock on `<data_dir>/profiles.lock`.
    fn acquire_cross_process_lock(&self) -> Result<AdvisoryFileLock, DimError> {
        AdvisoryFileLock::acquire_exclusive(&self.lock_path, DEFAULT_ADVISORY_LOCK_TIMEOUT)
    }

    fn ensure_initialized_locked(&self) -> Result<(), DimError> {
        fs::create_dir_all(&self.data_dir)?;
        fs::create_dir_all(&self.profiles_dir)?;
        fs::create_dir_all(&self.backups_dir)?;
        if !self.metadata_path.exists() {
            let empty = ProfilesDocument::default();
            self.write_document_atomic_locked(&empty)?;
        }
        Ok(())
    }

    /// Reads and validates `profiles.json`, distinguishing fresh init from corruption.
    ///
    /// # Fail-Closed Behavior (DEFECT-08 / CT-10)
    ///
    /// - **File does not exist:** Returns `Err(Io(NotFound))` — callers should use
    ///   `ensure_initialized_locked()` first which creates it.
    /// - **Empty (0-byte) file:** Returns `Err(Json(...))` — NEVER silently resets to `[]`.
    /// - **Invalid UTF-8:** Returns `Err(Io(...))`.
    /// - **Invalid JSON syntax:** Returns `Err(Json(...))`.
    /// - **Incompatible `schema_version`:** Returns `Err(SchemaError(...))`.
    /// - **Truncated write (valid JSON but missing fields):** Returns `Err(Json(...))`.
    fn read_document_locked(&self) -> Result<ProfilesDocument, DimError> {
        let raw = fs::read_to_string(&self.metadata_path)?;

        // Fail-closed on empty/whitespace-only file: NEVER overwrite to `[]`.
        if raw.trim().is_empty() {
            return Err(DimError::Json(serde_json::from_str::<ProfilesDocument>("").unwrap_err()));
        }

        let doc: ProfilesDocument = serde_json::from_str(&raw)?;
        if doc.schema_version != PROFILES_SCHEMA_VERSION {
            return Err(DimError::SchemaError(format!(
                "unsupported profiles.json schema_version {}; expected {PROFILES_SCHEMA_VERSION}. \
                 Recovery hint: back up profiles.json and restore from a compatible version.",
                doc.schema_version
            )));
        }
        Ok(doc)
    }

    /// Writes `profiles.json` atomically via unique temp file + sync + atomic replace.
    fn write_document_atomic_locked(&self, doc: &ProfilesDocument) -> Result<(), DimError> {
        let mut json = serde_json::to_vec_pretty(doc)?;
        json.push(b'\n');

        // Clean any stale temp files from prior interrupted writes.
        clean_stale_temp_files(&self.data_dir, "profiles.json");

        write_bytes_atomic(&self.metadata_path, &json)
    }
}

impl ProfileStore for JsonProfileStore {
    fn load(&self) -> Result<ProfilesDocument, DimError> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| DimError::Io(std::io::Error::other("profile store mutex poisoned")))?;
        let _fs_lock = self.acquire_cross_process_lock()?;
        self.ensure_initialized_locked()?;
        self.read_document_locked()
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
        let _guard = self
            .lock
            .lock()
            .map_err(|_| DimError::Io(std::io::Error::other("profile store mutex poisoned")))?;
        let _fs_lock = self.acquire_cross_process_lock()?;
        self.ensure_initialized_locked()?;
        let mut doc = self.read_document_locked()?;

        if doc
            .profiles
            .iter()
            .any(|p| p.name.eq_ignore_ascii_case(&clean_name))
        {
            return Err(DimError::DuplicateProfileName(clean_name));
        }

        let mut id = generate_profile_id();
        while doc.profiles.iter().any(|p| p.id == id) {
            id = generate_profile_id();
        }
        let directory_name = build_directory_name(&clean_name, &id);
        let profile_dir = resolve_safe_child(&self.profiles_dir, &directory_name)?;
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
        self.write_document_atomic_locked(&doc)?;
        Ok(record)
    }

    fn rename_profile(&self, id_or_name: &str, new_name: &str) -> Result<ProfileRecord, DimError> {
        let clean_name = validate_profile_name(new_name)?;
        let _guard = self
            .lock
            .lock()
            .map_err(|_| DimError::Io(std::io::Error::other("profile store mutex poisoned")))?;
        let _fs_lock = self.acquire_cross_process_lock()?;
        self.ensure_initialized_locked()?;
        let mut doc = self.read_document_locked()?;

        let idx = find_profile_index(&doc.profiles, id_or_name)
            .ok_or_else(|| DimError::ProfileNotFound(id_or_name.to_owned()))?;

        if doc
            .profiles
            .iter()
            .enumerate()
            .any(|(i, p)| i != idx && p.name.eq_ignore_ascii_case(&clean_name))
        {
            return Err(DimError::DuplicateProfileName(clean_name));
        }

        doc.profiles[idx].name = clean_name;
        let updated = doc.profiles[idx].clone();
        self.write_document_atomic_locked(&doc)?;
        Ok(updated)
    }

    fn delete_profile(&self, id_or_name: &str, confirm: bool) -> Result<(), DimError> {
        self.delete_profile_with_guard(id_or_name, confirm, false, &unlink_reparse_points_safe)
    }

    fn update_last_launched(
        &self,
        id: &str,
        timestamp_rfc3339: &str,
        zalo_user_id: Option<&str>,
    ) -> Result<ProfileRecord, DimError> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| DimError::Io(std::io::Error::other("profile store mutex poisoned")))?;
        let _fs_lock = self.acquire_cross_process_lock()?;
        self.ensure_initialized_locked()?;
        let mut doc = self.read_document_locked()?;
        let idx = find_profile_index(&doc.profiles, id)
            .ok_or_else(|| DimError::ProfileNotFound(id.to_owned()))?;

        doc.profiles[idx].last_launched_at = Some(timestamp_rfc3339.to_owned());
        if let Some(uid) = zalo_user_id {
            doc.profiles[idx].zalo_user_id = Some(uid.to_owned());
        }
        let updated = doc.profiles[idx].clone();
        self.write_document_atomic_locked(&doc)?;
        Ok(updated)
    }
}

/// Writes `<zalo_data_dir>/.zalo-profile-id` containing the profile's 8-hex ID.
pub fn write_profile_id_marker(zalo_data_dir: &Path, profile_id: &str) -> Result<(), DimError> {
    fs::create_dir_all(zalo_data_dir)?;
    let marker = zalo_data_dir.join(ZALO_PROFILE_ID_FILE);
    fs::write(marker, profile_id.as_bytes())?;
    Ok(())
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
    if meta.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        if (meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT) != 0 {
            return true;
        }
    }
    false
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
    if !root.exists() {
        return Ok(0);
    }
    let mut unlinked = 0usize;
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
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

fn remove_dir_without_following_links(dir: &Path) -> Result<(), DimError> {
    if !dir.exists() {
        return Ok(());
    }
    let root_meta = fs::symlink_metadata(dir)?;
    if is_symlink_or_reparse(&root_meta) {
        remove_reparse_or_symlink(dir, &root_meta)?;
        return Ok(());
    }
    for entry_res in fs::read_dir(dir)? {
        let entry = entry_res?;
        let path = entry.path();
        let meta = fs::symlink_metadata(&path)?;
        if is_symlink_or_reparse(&meta) {
            remove_reparse_or_symlink(&path, &meta)?;
        } else if meta.is_dir() {
            remove_dir_without_following_links(&path)?;
        } else {
            fs::remove_file(&path)?;
        }
    }
    fs::remove_dir(dir)?;
    Ok(())
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
                .position(|p| p.name.eq_ignore_ascii_case(trimmed))
        })
}
