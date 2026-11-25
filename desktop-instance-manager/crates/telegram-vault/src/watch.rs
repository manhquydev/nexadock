//! Debounced recursive file watching with startup reconciliation and stable-file hashing.

use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use notify::Watcher;
use notify_debouncer_full::{new_debouncer, DebounceEventResult};
use sha2::{Digest, Sha256};

use crate::{error::VaultError, Result};

/// A newly stable file discovered under a Zalo received-files directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StableFile {
    /// Source file path.
    pub path: PathBuf,
    /// Stable file size in bytes.
    pub size: u64,
    /// Lowercase hexadecimal SHA-256 digest of the complete file.
    pub sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileObservation {
    size: u64,
    modified: Option<SystemTime>,
    created: Option<SystemTime>,
}

impl FileObservation {
    fn from_metadata(metadata: &fs::Metadata) -> Self {
        Self {
            size: metadata.len(),
            modified: metadata.modified().ok(),
            created: metadata.created().ok(),
        }
    }
}

/// Recursive watcher for one profile's `Zalo Received Files` directory.
pub struct ZaloReceivedWatcher {
    root: PathBuf,
    _debouncer: notify_debouncer_full::Debouncer<
        notify::RecommendedWatcher,
        notify_debouncer_full::FileIdMap,
    >,
    events: std::sync::mpsc::Receiver<DebounceEventResult>,
    observed: HashMap<PathBuf, (FileObservation, u8)>,
    emitted: HashSet<PathBuf>,
}

impl ZaloReceivedWatcher {
    /// Starts watching the directory recursively and performs an initial full scan.
    pub fn new(root: impl Into<PathBuf>) -> Result<Self> {
        Self::with_debounce(root, Duration::from_millis(350))
    }

    /// Starts watching using the specified debounce interval.
    pub fn with_debounce(root: impl Into<PathBuf>, interval: Duration) -> Result<Self> {
        let root = root.into();
        if !root.is_dir() {
            return Err(VaultError::io(
                &root,
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "received-files directory missing",
                ),
            ));
        }
        let (sender, events) = std::sync::mpsc::channel();
        let event_sender = sender.clone();
        let mut debouncer = new_debouncer(interval, None, move |result| {
            let _ = event_sender.send(result);
        })
        .map_err(|error| VaultError::Watch(error.to_string()))?;
        debouncer
            .watcher()
            .watch(
                &root,
                notify_debouncer_full::notify::RecursiveMode::Recursive,
            )
            .map_err(|error| VaultError::Watch(error.to_string()))?;
        Ok(Self {
            root,
            _debouncer: debouncer,
            events,
            observed: HashMap::new(),
            emitted: HashSet::new(),
        })
    }

    /// Performs a full recursive scan and returns newly stable files.
    ///
    /// Callers must call [`Self::mark_emitted`] for each path only after durable
    /// enqueue succeeds, so transient queue errors never lose discovered files.
    /// The scan is deliberately also performed after notification events, so dropped OS watcher
    /// events do not permanently omit files from the queue.
    pub fn poll(&mut self) -> Result<Vec<StableFile>> {
        for event in self.events.try_iter() {
            if let Err(errors) = event {
                let detail = errors
                    .into_iter()
                    .map(|error| error.to_string())
                    .collect::<Vec<_>>()
                    .join("; ");
                return Err(VaultError::Watch(detail));
            }
        }
        let paths = scan_files(&self.root)?;
        let present: HashSet<_> = paths.iter().cloned().collect();
        self.observed.retain(|path, _| present.contains(path));
        self.emitted.retain(|path| present.contains(path));

        let mut stable = Vec::new();
        for path in paths {
            let metadata = fs::metadata(&path).map_err(|error| VaultError::io(&path, error))?;
            let observation = FileObservation::from_metadata(&metadata);
            let count = match self.observed.get_mut(&path) {
                Some((previous, stable_polls)) if *previous == observation => {
                    *stable_polls = stable_polls.saturating_add(1);
                    *stable_polls
                }
                Some((previous, stable_polls)) => {
                    *previous = observation;
                    *stable_polls = 1;
                    self.emitted.remove(&path);
                    1
                }
                None => {
                    self.observed.insert(path.clone(), (observation, 1));
                    1
                }
            };
            if count >= 2 && !self.emitted.contains(&path) {
                let (digest, final_observation) = hash_and_observe(&path)?;
                if final_observation != observation {
                    self.observed.insert(path.clone(), (final_observation, 1));
                    self.emitted.remove(&path);
                    continue;
                }
                stable.push(StableFile {
                    path,
                    size: observation.size,
                    sha256: digest,
                });
            }
        }
        Ok(stable)
    }

    /// Records that `path` has been durably enqueued and need not be emitted
    /// again until its metadata changes.
    pub fn mark_emitted(&mut self, path: &Path) {
        self.emitted.insert(path.to_path_buf());
    }

    /// Returns the watched profile directory.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }
}

/// Computes a lowercase SHA-256 digest of a file without reading it all into memory.
pub fn sha256_file(path: &Path) -> Result<String> {
    let mut file = File::open(path).map_err(|error| VaultError::io(path, error))?;
    sha256_reader_with_path(&mut file, path)
}

/// Verifies an already-open file matches `expected_sha256` without reopening `path`,
/// checks metadata stability across the hash read, rewinds to offset 0, and returns the byte length.
pub fn verify_opened_file(file: &mut File, path: &Path, expected_sha256: &str) -> Result<u64> {
    let meta_before = file
        .metadata()
        .map_err(|error| VaultError::io(path, error))?;
    let obs_before = FileObservation::from_metadata(&meta_before);
    file.seek(SeekFrom::Start(0))
        .map_err(|error| VaultError::io(path, error))?;
    let actual_sha256 = sha256_reader_with_path(file, path)?;
    let meta_after = file
        .metadata()
        .map_err(|error| VaultError::io(path, error))?;
    let obs_after = FileObservation::from_metadata(&meta_after);
    if obs_after != obs_before {
        return Err(VaultError::Transport(format!(
            "source file changed during verification: {}",
            path.display()
        )));
    }
    if !actual_sha256.eq_ignore_ascii_case(expected_sha256) {
        return Err(VaultError::Transport(format!(
            "source file changed after enqueue: {}",
            path.display()
        )));
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|error| VaultError::io(path, error))?;
    Ok(obs_before.size)
}

fn hash_and_observe(path: &Path) -> Result<(String, FileObservation)> {
    let mut file = File::open(path).map_err(|error| VaultError::io(path, error))?;
    let digest = sha256_reader_with_path(&mut file, path)?;
    let metadata = file
        .metadata()
        .map_err(|error| VaultError::io(path, error))?;
    Ok((digest, FileObservation::from_metadata(&metadata)))
}

fn sha256_reader_with_path(reader: &mut impl Read, path: &Path) -> Result<String> {
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| VaultError::io(path, error))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn scan_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        let entries =
            fs::read_dir(&directory).map_err(|error| VaultError::io(&directory, error))?;
        for entry in entries {
            let entry = entry.map_err(|error| VaultError::io(&directory, error))?;
            let path = entry.path();
            let metadata =
                fs::symlink_metadata(&path).map_err(|error| VaultError::io(&path, error))?;
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                pending.push(path);
            } else if metadata.is_file() {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}
