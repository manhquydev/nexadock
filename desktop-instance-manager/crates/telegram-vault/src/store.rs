//! Atomic per-profile Telegram vault configuration storage.

use std::fs;
use std::path::{Path, PathBuf};

use dim_core::write_bytes_atomic;
use serde::{Deserialize, Serialize};

use crate::{error::VaultError, Result};

/// Telegram destination selected for uploads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Destination {
    /// The authenticated Telegram account's Saved Messages.
    #[default]
    SavedMessages,
}

/// Telegram transport layer used by a pairing.
///
/// Defaults to MTProto so configurations written before the Bot API transport
/// existed keep working without migration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Transport {
    /// Phone-code MTProto session (the pre-existing pairing flow).
    #[default]
    Mtproto,
    /// Telegram Bot API with a chat captured through the bot deep link.
    Bot,
}

/// Which transport layer a resolved configuration actually uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportKind {
    /// The Telegram Bot API transport.
    Bot,
    /// The MTProto session transport.
    Mtproto,
}

/// Pairing and synchronization settings for one isolated Zalo profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultConfig {
    /// Stable Zalo profile identifier.
    pub profile_id: String,
    /// Telegram MTProto session directory for this profile.
    pub telegram_session_dir: PathBuf,
    /// Telegram destination for uploaded documents.
    #[serde(default)]
    pub destination: Destination,
    /// Whether the profile is currently synchronized.
    pub enabled: bool,
    /// Transport layer this pairing uses; defaults to MTProto for legacy configs.
    #[serde(default)]
    pub transport: Transport,
    /// Telegram application ID used to reconnect the isolated session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub telegram_api_id: Option<i32>,
    /// Telegram application hash used to reconnect the isolated session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub telegram_api_hash: Option<String>,
    /// Telegram Bot API token used by the `bot` transport.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub telegram_bot_token: Option<String>,
    /// Telegram chat identifier that first messaged the bot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub telegram_chat_id: Option<i64>,
    /// Public username of the validated Telegram bot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub telegram_bot_username: Option<String>,
}

impl VaultConfig {
    /// Resolves which transport layer this configuration actually uses.
    ///
    /// The Bot API transport requires a non-empty bot token; the MTProto
    /// transport requires both the application ID and hash. Configurations
    /// carrying both layers' credentials, or neither, are invalid because one
    /// pairing must use exactly one transport.
    pub fn transport_kind(&self) -> Result<TransportKind> {
        let bot = self
            .telegram_bot_token
            .as_deref()
            .is_some_and(|token| !token.trim().is_empty());
        let mtproto = self.telegram_api_id.is_some()
            && self
                .telegram_api_hash
                .as_deref()
                .is_some_and(|hash| !hash.trim().is_empty());
        match (bot, mtproto) {
            (true, false) => Ok(TransportKind::Bot),
            (false, true) => Ok(TransportKind::Mtproto),
            _ => Err(VaultError::InvalidRecord(
                "mixed or missing vault transport".into(),
            )),
        }
    }
}

/// Stores configurations as `<data_dir>/TelegramVault/profiles/<profile_id>.json`.
#[derive(Debug, Clone)]
pub struct VaultConfigStore {
    root: PathBuf,
}

impl VaultConfigStore {
    /// Creates the vault configuration store beneath the supplied application data directory.
    pub fn new(data_dir: impl Into<PathBuf>) -> Result<Self> {
        let root = data_dir.into().join("TelegramVault").join("profiles");
        fs::create_dir_all(&root).map_err(|e| VaultError::io(&root, e))?;
        Ok(Self { root })
    }

    /// Returns the directory containing per-profile configuration files.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Atomically writes one profile configuration.
    pub fn save(&self, config: &VaultConfig) -> Result<()> {
        validate_profile_id(&config.profile_id)?;
        config.transport_kind()?;
        let path = self.config_path(&config.profile_id)?;
        let bytes = serde_json::to_vec_pretty(config)?;
        write_bytes_atomic(&path, &bytes)?;
        // The configuration can carry Telegram application credentials; keep it
        // readable only by the owning user.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(&path)
                .map_err(|e| VaultError::io(&path, e))?
                .permissions();
            permissions.set_mode(0o600);
            fs::set_permissions(&path, permissions).map_err(|e| VaultError::io(&path, e))?;
        }
        Ok(())
    }

    /// Loads a profile configuration, returning `None` when no pairing exists.
    pub fn load(&self, profile_id: &str) -> Result<Option<VaultConfig>> {
        let path = self.config_path(profile_id)?;
        match fs::read(&path) {
            Ok(bytes) => {
                let config: VaultConfig = serde_json::from_slice(&bytes)?;
                if config.profile_id != profile_id {
                    return Err(VaultError::InvalidRecord(format!(
                        "configuration at {} belongs to profile {}",
                        path.display(),
                        config.profile_id
                    )));
                }
                Ok(Some(config))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(VaultError::io(path, error)),
        }
    }

    /// Lists all valid profile configurations in deterministic order.
    pub fn list(&self) -> Result<Vec<VaultConfig>> {
        let entries = fs::read_dir(&self.root).map_err(|e| VaultError::io(&self.root, e))?;
        let mut configs = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| VaultError::io(&self.root, e))?;
            if entry.path().extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let path = entry.path();
            let bytes = fs::read(&path).map_err(|error| VaultError::io(&path, error))?;
            let config: VaultConfig = serde_json::from_slice(&bytes)?;
            validate_profile_id(&config.profile_id)?;
            if path.file_stem().and_then(|value| value.to_str()) != Some(config.profile_id.as_str())
            {
                return Err(VaultError::InvalidRecord(format!(
                    "configuration filename does not match profile {}",
                    config.profile_id
                )));
            }
            configs.push(config);
        }
        configs.sort_by(|a, b| a.profile_id.cmp(&b.profile_id));
        Ok(configs)
    }

    /// Removes a profile pairing, returning whether a file was removed.
    pub fn remove(&self, profile_id: &str) -> Result<bool> {
        let path = self.config_path(profile_id)?;
        match fs::remove_file(&path) {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(VaultError::io(path, error)),
        }
    }

    fn config_path(&self, profile_id: &str) -> Result<PathBuf> {
        validate_profile_id(profile_id)?;
        Ok(self.root.join(format!("{profile_id}.json")))
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
