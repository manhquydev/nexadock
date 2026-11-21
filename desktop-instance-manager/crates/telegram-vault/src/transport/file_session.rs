//! Atomic JSON file-backed MTProto session storage.
//!
//! Implements the `grammers_session::Session` trait with `SessionData` mirrored to a
//! single JSON file via write-temp-then-rename, avoiding the default `libsql` storage:
//! `libsql` bundles its own SQLite C library, which collides with this crate's
//! `rusqlite`-bundled SQLite when both are linked into one binary (duplicate
//! `sqlite3_*` symbols). A file session also keeps the per-profile storage inspectable.

use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use grammers_session::types::{
    ChannelState, DcOption, PeerId, PeerInfo, UpdateState, UpdatesState,
};
use grammers_session::{BoxFuture, Session, SessionData};
use serde::{Deserialize, Serialize};

/// Failures that can occur while loading or persisting the session file.
#[derive(Debug)]
pub enum FileSessionError {
    /// The shared session state lock was poisoned.
    Poisoned,
    /// Reading or writing the session file failed.
    Io(std::io::Error),
    /// The session file content was not valid session JSON.
    Serde(serde_json::Error),
}

impl fmt::Display for FileSessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Poisoned => write!(f, "file session state lock was poisoned"),
            Self::Io(error) => write!(f, "file session io error: {error}"),
            Self::Serde(error) => write!(f, "file session parse error: {error}"),
        }
    }
}

impl std::error::Error for FileSessionError {}

impl From<std::io::Error> for FileSessionError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for FileSessionError {
    fn from(error: serde_json::Error) -> Self {
        Self::Serde(error)
    }
}

/// JSON mirror of [`SessionData`]. Peer entries are stored as a vector of pairs because
/// `PeerId` is a newtype that cannot serve as a JSON map key.
#[derive(Serialize, Deserialize)]
struct FileSessionData {
    home_dc: i32,
    dc_options: HashMap<i32, DcOption>,
    peer_infos: Vec<(PeerId, PeerInfo)>,
    updates_state: UpdatesState,
}

impl From<&SessionData> for FileSessionData {
    fn from(data: &SessionData) -> Self {
        Self {
            home_dc: data.home_dc,
            dc_options: data.dc_options.clone(),
            peer_infos: data
                .peer_infos
                .iter()
                .map(|(id, info)| (*id, info.clone()))
                .collect(),
            updates_state: data.updates_state.clone(),
        }
    }
}

impl From<FileSessionData> for SessionData {
    fn from(data: FileSessionData) -> Self {
        Self {
            home_dc: data.home_dc,
            dc_options: data.dc_options,
            peer_infos: data.peer_infos.into_iter().collect(),
            updates_state: data.updates_state,
        }
    }
}

/// MTProto session persisted as one atomic JSON file per profile.
pub struct FileSession {
    path: PathBuf,
    data: Mutex<SessionData>,
}

impl FileSession {
    /// Loads the session file, or starts a fresh session when the file does not exist yet.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, FileSessionError> {
        let path = path.as_ref().to_path_buf();
        let data = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice::<FileSessionData>(&bytes)?.into(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => SessionData::default(),
            Err(error) => return Err(FileSessionError::Io(error)),
        };
        Ok(Self {
            path,
            data: Mutex::new(data),
        })
    }

    fn data(&self) -> Result<MutexGuard<'_, SessionData>, FileSessionError> {
        self.data.lock().map_err(|_| FileSessionError::Poisoned)
    }

    /// Atomically writes the current state: serialize, write a temp sibling, rename over.
    fn persist(&self, data: &SessionData) -> Result<(), FileSessionError> {
        let bytes = serde_json::to_vec(&FileSessionData::from(data))?;
        let temp = self.path.with_extension("session.tmp");
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&temp, bytes)?;
        fs::rename(&temp, &self.path)?;
        Ok(())
    }
}

impl Session for FileSession {
    type Error = FileSessionError;

    fn home_dc_id(&self) -> Result<i32, Self::Error> {
        Ok(self.data()?.home_dc)
    }

    fn set_home_dc_id(&self, dc_id: i32) -> BoxFuture<'_, Result<(), Self::Error>> {
        Box::pin(async move {
            let mut data = self.data()?;
            data.home_dc = dc_id;
            self.persist(&data)
        })
    }

    fn dc_option(&self, dc_id: i32) -> Result<Option<DcOption>, Self::Error> {
        Ok(self.data()?.dc_options.get(&dc_id).cloned())
    }

    fn set_dc_option(&self, dc_option: &DcOption) -> BoxFuture<'_, Result<(), Self::Error>> {
        let dc_option = dc_option.clone();
        Box::pin(async move {
            let mut data = self.data()?;
            data.dc_options.insert(dc_option.id, dc_option);
            self.persist(&data)
        })
    }

    fn peer(&self, peer: PeerId) -> BoxFuture<'_, Result<Option<PeerInfo>, Self::Error>> {
        Box::pin(async move { Ok(self.data()?.peer_infos.get(&peer).cloned()) })
    }

    fn cache_peer(&self, peer: PeerInfo) -> BoxFuture<'_, Result<(), Self::Error>> {
        Box::pin(async move {
            let mut data = self.data()?;
            data.peer_infos
                .entry(peer.id())
                .or_insert_with(|| peer.clone())
                .extend_info(&peer);
            self.persist(&data)
        })
    }

    fn updates_state(&self) -> BoxFuture<'_, Result<UpdatesState, Self::Error>> {
        Box::pin(async move { Ok(self.data()?.updates_state.clone()) })
    }

    fn set_update_state(&self, update: UpdateState) -> BoxFuture<'_, Result<(), Self::Error>> {
        Box::pin(async move {
            let mut data = self.data()?;
            match update {
                UpdateState::All(updates_state) => {
                    data.updates_state = updates_state;
                }
                UpdateState::Primary { pts, date, seq } => {
                    data.updates_state.pts = pts;
                    data.updates_state.date = date;
                    data.updates_state.seq = seq;
                }
                UpdateState::Secondary { qts } => {
                    data.updates_state.qts = qts;
                }
                UpdateState::Channel { id, pts } => {
                    data.updates_state.channels.retain(|c| c.id != id);
                    data.updates_state.channels.push(ChannelState { id, pts });
                }
            }
            self.persist(&data)
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::FileSession;
    use grammers_session::types::{PeerId, PeerInfo};
    use grammers_session::Session;
    use std::future::Future;

    fn block_on<F: Future>(future: F) -> F::Output {
        tokio::runtime::Runtime::new().unwrap().block_on(future)
    }

    #[test]
    fn file_session_round_trips_home_dc_across_reopen() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("profile-a.session");
        {
            let session = FileSession::open(&path).unwrap();
            block_on(async {
                session.set_home_dc_id(4).await.unwrap();
            });
        }
        // A fresh instance reloads the persisted state from disk.
        let session = FileSession::open(&path).unwrap();
        assert_eq!(session.home_dc_id().unwrap(), 4);
    }

    #[test]
    fn file_session_persists_peer_cache_across_reopen() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("profile-b.session");
        let session = FileSession::open(&path).unwrap();
        let peer = PeerInfo::Chat { id: 42 };
        block_on(async {
            session.cache_peer(peer).await.unwrap();
        });
        let reopened = FileSession::open(&path).unwrap();
        let cached = block_on(async { reopened.peer(PeerId::chat(42).unwrap()).await.unwrap() });
        assert!(cached.is_some());
        // The temp sibling must never linger after a successful persist.
        assert!(!path.with_extension("session.tmp").exists());
    }
}
