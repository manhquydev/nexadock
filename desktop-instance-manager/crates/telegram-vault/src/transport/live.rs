//! Phone-code authenticated MTProto transport backed by an isolated SQLite session.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use grammers_client::client::LoginToken;
use grammers_client::media::InputMedia;
use grammers_client::sender::SenderPool;
use grammers_client::{Client, SignInError};
use grammers_session::storages::SqliteSession;
use tokio::runtime::Runtime;

use crate::transport::{RemoteRef, TelegramTransport};
use crate::watch::sha256_file;
use crate::{error::VaultError, Result};

/// Credentials used to authorize a Telegram account on its first session use.
pub struct TelegramCredentials {
    /// Telegram application identifier from `my.telegram.org`.
    pub api_id: i32,
    /// Telegram application hash from `my.telegram.org`.
    pub api_hash: String,
    /// Phone number in international format, required only for a new session.
    pub phone: Option<String>,
    /// One-time Telegram login code, required only for a new session.
    pub login_code: Option<String>,
    /// Optional two-factor password, used only during first-time authentication.
    pub two_factor_password: Option<String>,
}

/// Live Telegram client whose persisted MTProto session is isolated by profile.
pub struct LiveTransport {
    profile_id: String,
    session_path: PathBuf,
    runtime: Runtime,
    client: Client,
}

/// In-progress phone-code login retaining its isolated Telegram session.
pub struct PendingPhoneLogin {
    profile_id: String,
    session_path: PathBuf,
    runtime: Runtime,
    client: Client,
    token: LoginToken,
}

impl PendingPhoneLogin {
    /// Completes authorization with the received code and optional two-factor password.
    pub fn complete(
        self,
        login_code: &str,
        two_factor_password: Option<&str>,
    ) -> Result<LiveTransport> {
        finish_phone_login(
            &self.runtime,
            &self.client,
            &self.token,
            login_code,
            two_factor_password,
        )?;
        Ok(LiveTransport {
            profile_id: self.profile_id,
            session_path: self.session_path,
            runtime: self.runtime,
            client: self.client,
        })
    }
}

impl LiveTransport {
    /// Requests a one-time code and returns a pending login for a profile-specific session.
    pub fn begin_phone_login(
        data_dir: impl AsRef<Path>,
        profile_id: impl Into<String>,
        api_id: i32,
        api_hash: &str,
        phone: &str,
    ) -> Result<PendingPhoneLogin> {
        let profile_id = profile_id.into();
        validate_profile_id(&profile_id)?;
        let session_path = profile_session_path(data_dir.as_ref(), &profile_id)?;
        let runtime = build_runtime()?;
        let session = Arc::new(
            runtime
                .block_on(SqliteSession::open(&session_path))
                .map_err(|error| VaultError::Transport(error.to_string()))?,
        );
        let pool = SenderPool::new(session, api_id);
        let client = Client::new(pool.handle.clone());
        runtime.spawn(pool.runner.run());
        let token = runtime
            .block_on(client.request_login_code(phone, api_hash))
            .map_err(|error| VaultError::Transport(error.to_string()))?;
        Ok(PendingPhoneLogin {
            profile_id,
            session_path,
            runtime,
            client,
            token,
        })
    }
    /// Opens `<data_dir>/TelegramVault/sessions/<profile_id>.session` and authenticates if needed.
    ///
    /// The login code is only used when the session has not yet been authorized. Accounts with
    /// two-factor authentication must provide its password; credentials are never persisted in
    /// the vault configuration.
    pub fn connect(
        data_dir: impl AsRef<Path>,
        profile_id: impl Into<String>,
        credentials: &TelegramCredentials,
    ) -> Result<Self> {
        let profile_id = profile_id.into();
        validate_profile_id(&profile_id)?;
        let session_path = profile_session_path(data_dir.as_ref(), &profile_id)?;
        let runtime = build_runtime()?;
        let session = Arc::new(
            runtime
                .block_on(SqliteSession::open(&session_path))
                .map_err(|error| VaultError::Transport(error.to_string()))?,
        );
        let pool = SenderPool::new(session, credentials.api_id);
        let client = Client::new(pool.handle.clone());
        runtime.spawn(pool.runner.run());
        let authorized = runtime
            .block_on(client.is_authorized())
            .map_err(|error| VaultError::Transport(error.to_string()))?;
        if !authorized {
            let phone = credentials.phone.as_deref().ok_or_else(|| {
                VaultError::Transport(
                    "Telegram phone number is required for first-time login".into(),
                )
            })?;
            let login_code = credentials.login_code.as_deref().ok_or_else(|| {
                VaultError::Transport("Telegram login code is required for first-time login".into())
            })?;
            let token = runtime
                .block_on(client.request_login_code(phone, &credentials.api_hash))
                .map_err(|error| VaultError::Transport(error.to_string()))?;
            finish_phone_login(
                &runtime,
                &client,
                &token,
                login_code,
                credentials.two_factor_password.as_deref(),
            )?;
        }
        Ok(Self {
            profile_id,
            session_path,
            runtime,
            client,
        })
    }

    /// Returns the isolated session database path for this profile.
    #[must_use]
    pub fn session_path(&self) -> &Path {
        &self.session_path
    }
}

impl TelegramTransport for LiveTransport {
    fn upload_document(
        &self,
        profile_id: &str,
        path: &Path,
        sha256: &str,
        caption: &str,
    ) -> Result<RemoteRef> {
        if profile_id != self.profile_id {
            return Err(VaultError::Transport(format!(
                "transport belongs to profile {}, not {profile_id}",
                self.profile_id
            )));
        }
        let actual_sha256 = sha256_file(path)?;
        if actual_sha256 != sha256 {
            return Err(VaultError::Transport(format!(
                "source file changed after enqueue: {}",
                path.display()
            )));
        }
        let upload = self
            .runtime
            .block_on(self.client.upload_file(path))
            .map_err(|error| VaultError::Transport(error.to_string()))?;
        let me = self
            .runtime
            .block_on(self.client.get_me())
            .map_err(|error| VaultError::Transport(error.to_string()))?;
        let peer = self
            .runtime
            .block_on(me.to_ref())
            .map_err(|error| VaultError::Transport(error.to_string()))?
            .ok_or_else(|| VaultError::Transport("Telegram self peer is unavailable".into()))?;
        let media = InputMedia::new().document(upload).caption(caption);
        let mut messages = self
            .runtime
            .block_on(self.client.send_album(peer, vec![media]))
            .map_err(|error| VaultError::Transport(error.to_string()))?;
        let message = messages
            .pop()
            .flatten()
            .ok_or_else(|| VaultError::Transport("Telegram returned no uploaded message".into()))?;
        Ok(RemoteRef {
            peer_id: "saved_messages".into(),
            message_id: i64::from(message.id()),
        })
    }
}

fn profile_session_path(data_dir: &Path, profile_id: &str) -> Result<PathBuf> {
    let sessions_dir = data_dir.join("TelegramVault").join("sessions");
    std::fs::create_dir_all(&sessions_dir).map_err(|error| VaultError::io(&sessions_dir, error))?;
    Ok(sessions_dir.join(format!("{profile_id}.session")))
}

fn build_runtime() -> Result<Runtime> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| VaultError::Transport(error.to_string()))
}
fn finish_phone_login(
    runtime: &Runtime,
    client: &Client,
    token: &LoginToken,
    login_code: &str,
    two_factor_password: Option<&str>,
) -> Result<()> {
    match runtime.block_on(client.sign_in(token, login_code)) {
        Ok(_) => Ok(()),
        Err(SignInError::PasswordRequired(password_token)) => {
            let password = two_factor_password.ok_or_else(|| {
                VaultError::Transport("Telegram account requires a two-factor password".into())
            })?;
            runtime
                .block_on(client.check_password(password_token, password.as_bytes()))
                .map(|_| ())
                .map_err(|error| VaultError::Transport(error.to_string()))
        }
        Err(error) => Err(VaultError::Transport(error.to_string())),
    }
}
fn validate_profile_id(profile_id: &str) -> Result<()> {
    if profile_id.is_empty()
        || !profile_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err(VaultError::InvalidRecord(
            "invalid profile identifier".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use super::profile_session_path;

    #[test]
    fn telegram_sessions_are_separate_per_profile() {
        let tmp = tempfile::tempdir().unwrap();
        let first_path = profile_session_path(tmp.path(), "profile-a").unwrap();
        let second_path = profile_session_path(tmp.path(), "profile-b").unwrap();
        let sessions_dir = tmp.path().join("TelegramVault/sessions");
        assert_eq!(first_path, sessions_dir.join("profile-a.session"));
        assert_eq!(second_path, sessions_dir.join("profile-b.session"));
        assert_ne!(first_path, second_path);
    }
}
