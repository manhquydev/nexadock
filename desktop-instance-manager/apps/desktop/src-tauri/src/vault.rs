//! Telegram Vault integration: pairing state machine, per-profile sync engine
//! threads, and queue/log access backing the `vault_*` Tauri commands.
//!
//! The sync path uses the real `dim-telegram-vault` crate primitives end to
//! end: `ZaloReceivedWatcher` -> `VaultQueue` -> a `TelegramTransport` upload
//! with claim/mark-done/mark-failed bookkeeping. One pairing uses exactly one
//! transport layer: either the Telegram Bot API (`BotTransport`, stateless
//! reconnect from a stored token + captured chat id) or the phone-code MTProto
//! session (`LiveTransport`). Telegram MTProto credentials are read from the
//! `TELEGRAM_API_ID` / `TELEGRAM_API_HASH` environment variables at login time
//! and are never persisted; the MTProto session lives in the file
//! `<data_dir>/TelegramVault/sessions/<profile_id>.session`. Telegram
//! Desktop `tdata` is never touched. Bot tokens and chat identifiers are never
//! logged.

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use dim_telegram_vault::queue::{QueueItem, QueueStatus, VaultLogEntry, VaultQueue};
use dim_telegram_vault::store::{Transport, TransportKind, VaultConfig, VaultConfigStore};
use dim_telegram_vault::transport::{
    BotTransport, LiveTransport, TelegramCredentials, TelegramTransport,
};
use dim_telegram_vault::watch::{StableFile, ZaloReceivedWatcher};
use serde::Serialize;

/// Polling cadence for the watcher thread.
const WATCH_INTERVAL: Duration = Duration::from_millis(1500);
/// Delay between reconnection attempts while the Telegram session is unusable.
const RECONNECT_DELAY: Duration = Duration::from_secs(5);
/// Consumer idle timeout; `vault_sync_now` interrupts this wait immediately.
const CONSUMER_IDLE_WAIT: Duration = Duration::from_secs(1);
/// Wall-clock limit of one `vault_pair_bot_wait` chat-capture wait.
const BOT_CHAT_WAIT_SECS: u64 = 60;

/// Wake signal shared between command handlers and engine threads.
#[derive(Debug)]
struct WakeSignal {
    flag: Mutex<bool>,
    cond: Condvar,
}

impl Default for WakeSignal {
    fn default() -> Self {
        Self {
            flag: Mutex::new(false),
            cond: Condvar::new(),
        }
    }
}

impl WakeSignal {
    fn notify(&self) {
        let mut flag = match self.flag.lock() {
            Ok(flag) => flag,
            Err(poisoned) => poisoned.into_inner(),
        };
        *flag = true;
        self.cond.notify_all();
    }

    /// Waits up to `timeout`; returns `true` when explicitly woken.
    fn wait(&self, timeout: Duration) -> bool {
        let deadline = std::time::Instant::now() + timeout;
        let mut flag = match self.flag.lock() {
            Ok(flag) => flag,
            Err(poisoned) => poisoned.into_inner(),
        };
        if *flag {
            *flag = false;
            return true;
        }
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return false;
            }
            let (guard, timed_out) = match self.cond.wait_timeout(flag, remaining) {
                Ok(result) => result,
                Err(poisoned) => poisoned.into_inner(),
            };
            flag = guard;
            if *flag {
                *flag = false;
                return true;
            }
            if timed_out.timed_out() {
                return false;
            }
        }
    }
}

/// Transport handed to one profile's consumer thread at spawn time.
///
/// Bot pairings are stateless (`Ready`); MTProto pairings keep their
/// credentials so the persisted session can be reconnected whenever the
/// initial live transport is absent or dies. `None` means pairing is not yet
/// usable and the thread idles until a resumed manager respawns it.
enum ConsumerTransport {
    /// A fully built upload transport.
    Ready(Box<dyn TelegramTransport>),
    /// MTProto credentials to (re)connect `LiveTransport` from.
    MtprotoReconnect(TelegramCredentials),
    /// No transport is currently available for this profile.
    None,
}

/// Joinable worker handles for one profile's sync engine.
struct ProfileEngine {
    stop: Arc<AtomicBool>,
    wake: Arc<WakeSignal>,
    watcher_handle: Mutex<Option<JoinHandle<()>>>,
    consumer_handle: Mutex<Option<JoinHandle<()>>>,
}

impl ProfileEngine {
    fn stop_and_join(&self) {
        self.stop.store(true, Ordering::Release);
        self.wake.notify();
        if let Ok(mut slot) = self.watcher_handle.lock() {
            if let Some(handle) = slot.take() {
                let _ = handle.join();
            }
        }
        if let Ok(mut slot) = self.consumer_handle.lock() {
            if let Some(handle) = slot.take() {
                let _ = handle.join();
            }
        }
    }
}

/// Snapshot of pairing and queue state returned by `vault_get_status`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultStatus {
    pub paired: bool,
    pub enabled: bool,
    pub session_exists: bool,
    pub engine_running: bool,
    pub queue_db_exists: bool,
    /// Active pairing transport: `"bot"` or `"mtproto"`.
    pub transport: String,
    /// The bot pairing is fully configured (token stored, chat captured).
    pub bot_configured: bool,
    /// Public username of the bot, available while the owner is still connecting it.
    pub bot_username: Option<String>,
    pub pending: u64,
    pub uploading: u64,
    pub failed: u64,
    pub done: u64,
    pub last_synced_at: Option<i64>,
}

/// Result of starting (or short-circuiting) the pairing flow.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairFlow {
    pub paired: bool,
    pub requires_login: bool,
    /// Telegram application credentials are available (stored pairing or env).
    pub credentials_available: bool,
}

/// Result of a successful bot token validation and pairing.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BotPairFlow {
    /// The bot's public username without the leading `@`.
    pub bot_username: String,
    /// `https://t.me/<username>` link the vault owner opens to start the chat.
    pub deep_link: String,
}

/// Outcome of waiting for the vault owner to message the bot.
#[derive(Debug)]
pub enum BotWaitOutcome {
    /// A private chat was captured; carries the resulting vault status.
    Captured(VaultStatus),
    /// The wait timed out before any private chat message arrived.
    TimedOut,
}

/// One row of the per-profile synchronization log, newest first.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultLogRow {
    pub ts: i64,
    pub file: String,
    pub size: u64,
    pub outcome: String,
    pub error: Option<String>,
}

/// Errors produced by the vault integration layer.
#[derive(Debug)]
pub enum VaultManagerError {
    /// The vault crate rejected the request (bad profile id, IO, SQLite, transport).
    Vault(String),
    /// The profile is not known to the profile store.
    ProfileNotFound(String),
    /// Telegram credentials were neither supplied nor present in the environment.
    MissingCredentials,
}

impl fmt::Display for VaultManagerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Vault(detail) => write!(formatter, "vault error: {detail}"),
            Self::ProfileNotFound(profile_id) => {
                write!(formatter, "profile not found: {profile_id}")
            }
            Self::MissingCredentials => write!(
                formatter,
                "Telegram API credentials are required (TELEGRAM_API_ID / TELEGRAM_API_HASH)"
            ),
        }
    }
}

impl std::error::Error for VaultManagerError {}

impl From<dim_telegram_vault::VaultError> for VaultManagerError {
    fn from(error: dim_telegram_vault::VaultError) -> Self {
        Self::Vault(error.to_string())
    }
}

type VaultManagerResult<T> = Result<T, VaultManagerError>;

/// Resolves a profile id to its isolated profile directory.
type ProfileDirResolver = Arc<dyn Fn(&str) -> Result<PathBuf, String> + Send + Sync>;

/// Manages pairing state and per-profile sync engine threads.
#[derive(Clone)]
pub struct VaultManager {
    data_dir: Arc<Path>,
    config_store: Arc<VaultConfigStore>,
    queue_path: Arc<Path>,
    pending_logins: Arc<Mutex<HashMap<String, PendingLogin>>>,
    engines: Arc<Mutex<HashMap<String, Arc<ProfileEngine>>>>,
    profile_dir_for: ProfileDirResolver,
}

struct PendingLogin {
    pending: dim_telegram_vault::transport::PendingPhoneLogin,
    received_dir: PathBuf,
    /// Resolved Telegram application credentials, persisted with the pairing
    /// so the engine can reconnect after an application restart without env.
    api_id: i32,
    api_hash: String,
}

impl VaultManager {
    /// Builds the manager rooted at the application data directory.
    pub fn new(
        data_dir: impl Into<PathBuf>,
        profile_dir_for: ProfileDirResolver,
    ) -> VaultManagerResult<Self> {
        let data_dir: Arc<Path> = Arc::from(data_dir.into());
        let config_store = Arc::new(VaultConfigStore::new(data_dir.as_ref())?);
        let queue_path: Arc<Path> = Arc::from(data_dir.join("TelegramVault").join("queue.db"));
        Ok(Self {
            data_dir,
            config_store,
            queue_path,
            pending_logins: Arc::new(Mutex::new(HashMap::new())),
            engines: Arc::new(Mutex::new(HashMap::new())),
            profile_dir_for,
        })
    }

    /// Restarts sync engines for every paired profile that is enabled and has
    /// a persisted session. Called once during application startup; performs
    /// the once-per-process crash recovery of interrupted uploads first.
    pub fn resume_all(&self) {
        // Requeue items left `uploading` by an unclean exit before any
        // consumer can claim work; plain `open()` never touches row state.
        // A failed recovery is surfaced (never swallowed): items stuck in
        // `uploading` would never retry otherwise.
        match VaultQueue::open(&self.queue_path) {
            Ok(queue) => {
                if let Err(error) = queue.recover_interrupted() {
                    eprintln!("NexaDock vault crash recovery failed: {error}");
                }
            }
            Err(error) => {
                eprintln!("NexaDock vault queue unavailable during startup: {error}");
            }
        }
        let Ok(configs) = self.config_store.list() else {
            return;
        };
        for config in configs {
            if !config.enabled || !self.pairing_is_runnable(&config) {
                continue;
            }
            if let Ok(received_dir) = self.received_dir_for(&config.profile_id) {
                let _ =
                    self.spawn_engine(&config, self.consumer_transport_for(&config), received_dir);
            }
        }
    }

    /// Whether a pairing config has everything its transport needs to run:
    /// bot pairings need the token and a captured chat; MTProto pairings need
    /// a persisted session plus reconnect credentials.
    fn pairing_is_runnable(&self, config: &VaultConfig) -> bool {
        match config.transport_kind() {
            Ok(TransportKind::Bot) => {
                config
                    .telegram_bot_token
                    .as_deref()
                    .is_some_and(|token| !token.trim().is_empty())
                    && config.telegram_chat_id.is_some()
            }
            Ok(TransportKind::Mtproto) => {
                self.session_file_exists(&config.profile_id)
                    && self.credentials_for(config).is_some()
            }
            Err(_) => false,
        }
    }

    /// Shuts down every engine thread (used by teardown paths).
    pub fn shutdown(&self) {
        let engines: Vec<String> = {
            let Ok(map) = self.engines.lock() else {
                return;
            };
            map.keys().cloned().collect()
        };
        for profile_id in engines {
            self.stop_engine(&profile_id);
        }
    }

    /// Returns the current pairing/sync status snapshot for one profile.
    pub fn status(&self, profile_id: &str) -> VaultManagerResult<VaultStatus> {
        let config = self.config_store.load(profile_id)?;
        let mut snapshot = VaultStatus {
            paired: config.is_some(),
            enabled: config.as_ref().is_some_and(|value| value.enabled),
            session_exists: self.session_file_exists(profile_id),
            engine_running: self.engine_running(profile_id),
            queue_db_exists: self.queue_path.exists(),
            transport: config
                .as_ref()
                .map(|value| match value.transport {
                    Transport::Bot => "bot",
                    Transport::Mtproto => "mtproto",
                })
                .unwrap_or("mtproto")
                .to_owned(),
            bot_configured: config.as_ref().is_some_and(|value| {
                value.telegram_bot_token.is_some() && value.telegram_chat_id.is_some()
            }),
            bot_username: config
                .as_ref()
                .and_then(|value| value.telegram_bot_username.clone()),
            pending: 0,
            uploading: 0,
            failed: 0,
            done: 0,
            last_synced_at: None,
        };
        if !self.queue_path.exists() {
            return Ok(snapshot);
        }
        let queue = VaultQueue::open(self.queue_path.as_ref())?;
        for item in queue.list()? {
            if item.profile_id != profile_id {
                continue;
            }
            match item.status {
                QueueStatus::Pending => snapshot.pending += 1,
                QueueStatus::Uploading => snapshot.uploading += 1,
                QueueStatus::Failed => snapshot.failed += 1,
                QueueStatus::Done => snapshot.done += 1,
            }
        }
        let entries: Vec<VaultLogEntry> = queue.log(500)?;
        for entry in entries {
            if entry.profile_id == profile_id && entry.outcome == "done" {
                snapshot.last_synced_at = Some(entry.ts);
                break;
            }
        }
        Ok(snapshot)
    }

    /// Starts or resumes pairing. Returns the login requirements for the UI.
    pub fn pair(&self, profile_id: &str) -> VaultManagerResult<PairFlow> {
        let stored = self
            .config_store
            .load(profile_id)?
            .and_then(|config| config_credentials(&config));
        let credentials_available = stored.is_some() || self.env_credentials().is_some();
        if let Some(config) = self.config_store.load(profile_id)? {
            // Bot pairings never use the phone-login flow.
            if config.transport == Transport::Bot && self.pairing_is_runnable(&config) {
                self.ensure_engine(&config)?;
                return Ok(PairFlow {
                    paired: true,
                    requires_login: false,
                    credentials_available,
                });
            }
            if self.session_file_exists(profile_id) {
                // Already paired with a usable session; make sure the engine runs.
                self.ensure_engine(&config)?;
                return Ok(PairFlow {
                    paired: true,
                    requires_login: false,
                    credentials_available,
                });
            }
        }
        Ok(PairFlow {
            paired: false,
            requires_login: true,
            credentials_available,
        })
    }

    /// Requests a Telegram login code and parks a pending login for the profile.
    pub fn login_phone(
        &self,
        profile_id: &str,
        phone: &str,
        api_id: Option<i32>,
        api_hash: Option<&str>,
    ) -> VaultManagerResult<()> {
        // Resolution order: explicit arguments (UI fields) > previously stored
        // pairing credentials > environment variables.
        let stored = self
            .config_store
            .load(profile_id)?
            .and_then(|config| config_credentials(&config));
        let resolved = match (api_id, api_hash) {
            (Some(id), Some(hash)) if !hash.trim().is_empty() => TelegramCredentials {
                api_id: id,
                api_hash: hash.trim().to_owned(),
                phone: None,
                login_code: None,
                two_factor_password: None,
            },
            _ => stored
                .or_else(|| {
                    self.env_credentials()
                        .map(|(api_id, api_hash)| TelegramCredentials {
                            api_id,
                            api_hash,
                            phone: None,
                            login_code: None,
                            two_factor_password: None,
                        })
                })
                .ok_or(VaultManagerError::MissingCredentials)?,
        };
        let resolved_api_id = resolved.api_id;
        let resolved_api_hash = resolved.api_hash.clone();
        let received_dir = self.received_dir_for(profile_id)?;
        let pending = LiveTransport::begin_phone_login(
            self.data_dir.as_ref(),
            profile_id,
            resolved_api_id,
            &resolved_api_hash,
            phone,
        )?;
        let mut logins = self
            .pending_logins
            .lock()
            .map_err(|_| Self::lock_poisoned("pending login"))?;
        logins.insert(
            profile_id.to_owned(),
            PendingLogin {
                pending,
                received_dir,
                api_id: resolved_api_id,
                api_hash: resolved_api_hash,
            },
        );
        Ok(())
    }

    /// Completes a pending login with the received code (plus optional 2FA
    /// password), persists the pairing config, and starts the sync engine.
    pub fn login_code(
        &self,
        profile_id: &str,
        code: &str,
        password: Option<&str>,
    ) -> VaultManagerResult<()> {
        let pending = {
            let mut logins = self
                .pending_logins
                .lock()
                .map_err(|_| Self::lock_poisoned("pending login"))?;
            logins.remove(profile_id)
        };
        let Some(pending) = pending else {
            return Err(VaultManagerError::Vault(
                "no pending Telegram login for this profile; request a code first".into(),
            ));
        };
        let transport = pending.pending.complete(code, password)?;
        let config = VaultConfig {
            profile_id: profile_id.to_owned(),
            telegram_session_dir: self.session_file_for(profile_id),
            destination: Default::default(),
            enabled: true,
            transport: Transport::Mtproto,
            telegram_api_id: Some(pending.api_id),
            telegram_api_hash: Some(pending.api_hash),
            telegram_bot_token: None,
            telegram_chat_id: None,
            telegram_bot_username: None,
        };
        self.config_store.save(&config)?;
        self.spawn_engine(
            &config,
            ConsumerTransport::Ready(Box::new(transport)),
            pending.received_dir,
        )?;
        Ok(())
    }

    /// Validates a bot token and stores a bot transport pairing for the
    /// profile. The chat is not captured yet; `pair_bot_wait` completes it.
    pub fn pair_bot(&self, profile_id: &str, token: &str) -> VaultManagerResult<BotPairFlow> {
        // Reject unknown/invalid profile identifiers through the resolver.
        self.received_dir_for(profile_id)?;
        let token = token.trim();
        if token.is_empty() {
            return Err(VaultManagerError::Vault(
                "Telegram bot token is required".into(),
            ));
        }
        let info = BotTransport::validate_token(token)?;
        let config = VaultConfig {
            profile_id: profile_id.to_owned(),
            telegram_session_dir: self.session_file_for(profile_id),
            destination: Default::default(),
            enabled: true,
            transport: Transport::Bot,
            telegram_api_id: None,
            telegram_api_hash: None,
            telegram_bot_token: Some(token.to_owned()),
            telegram_chat_id: None,
            telegram_bot_username: Some(info.username.clone()),
        };
        self.config_store.save(&config)?;
        Ok(BotPairFlow {
            bot_username: info.username.clone(),
            deep_link: format!("https://t.me/{}", info.username),
        })
    }

    /// Waits for the vault owner to message the bot, captures the private
    /// chat id, persists it, and starts the sync engine.
    ///
    /// Blocking (long-polls the Bot API); run it off the UI thread, e.g. via
    /// `tauri::async_runtime::spawn_blocking`.
    pub fn pair_bot_wait(&self, profile_id: &str) -> VaultManagerResult<BotWaitOutcome> {
        let mut config = self.config_store.load(profile_id)?.ok_or_else(|| {
            VaultManagerError::Vault("profile is not paired with a bot yet".into())
        })?;
        if config.transport != Transport::Bot {
            return Err(VaultManagerError::Vault(
                "this profile is not paired with a bot token".into(),
            ));
        }
        let token = config
            .telegram_bot_token
            .as_deref()
            .map(str::trim)
            .filter(|token| !token.is_empty())
            .ok_or_else(|| {
                VaultManagerError::Vault("this profile is not paired with a bot token".into())
            })?
            .to_owned();
        if config.telegram_chat_id.is_none() {
            let chat_id = match BotTransport::wait_for_chat(&token, BOT_CHAT_WAIT_SECS) {
                Ok(chat_id) => chat_id,
                Err(dim_telegram_vault::VaultError::Timeout(_)) => {
                    return Ok(BotWaitOutcome::TimedOut);
                }
                Err(error) => return Err(error.into()),
            };
            config.telegram_chat_id = Some(chat_id);
            self.config_store.save(&config)?;
        }
        self.ensure_engine(&config)?;
        Ok(BotWaitOutcome::Captured(self.status(profile_id)?))
    }

    /// Removes the pairing; the persisted MTProto session file is only deleted
    /// when explicitly requested, and Telegram Desktop `tdata` is never touched.
    pub fn unpair(&self, profile_id: &str, delete_session: bool) -> VaultManagerResult<bool> {
        self.stop_engine(profile_id);
        {
            let mut logins = self
                .pending_logins
                .lock()
                .map_err(|_| Self::lock_poisoned("pending login"))?;
            logins.remove(profile_id);
        }
        let removed = self.config_store.remove(profile_id)?;
        if delete_session {
            // Only ever removes this profile's own session file.
            match std::fs::remove_file(self.session_file_for(profile_id)) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(VaultManagerError::Vault(format!(
                        "removing session file failed: {error}"
                    )));
                }
            }
        }
        Ok(removed)
    }

    /// Enables or disables synchronization for one profile.
    pub fn set_enabled(&self, profile_id: &str, enabled: bool) -> VaultManagerResult<()> {
        let mut config = self
            .config_store
            .load(profile_id)?
            .ok_or_else(|| VaultManagerError::Vault("profile is not paired".into()))?;
        config.enabled = enabled;
        self.config_store.save(&config)?;
        if enabled {
            self.ensure_engine(&config)?;
        } else {
            self.stop_engine(profile_id);
        }
        Ok(())
    }

    /// Wakes the consumer thread and returns the resulting queue snapshot.
    pub fn sync_now(&self, profile_id: &str) -> VaultManagerResult<VaultStatus> {
        if let Ok(map) = self.engines.lock() {
            if let Some(engine) = map.get(profile_id) {
                engine.wake.notify();
            }
        }
        self.status(profile_id)
    }

    /// Returns the most recent log rows for one profile, newest first.
    pub fn log(&self, profile_id: &str, limit: usize) -> VaultManagerResult<Vec<VaultLogRow>> {
        let mut rows = Vec::new();
        if !self.queue_path.exists() {
            return Ok(rows);
        }
        let queue = VaultQueue::open(self.queue_path.as_ref())?;
        for entry in queue.log(limit.saturating_mul(10).max(limit))? {
            if entry.profile_id != profile_id {
                continue;
            }
            rows.push(VaultLogRow {
                ts: entry.ts,
                file: entry.file.display().to_string(),
                size: entry.size,
                outcome: entry.outcome,
                error: entry.error,
            });
            if rows.len() >= limit {
                break;
            }
        }
        Ok(rows)
    }

    // ------------------------------------------------------------------
    // Internals
    // ------------------------------------------------------------------

    fn lock_poisoned(what: &'static str) -> VaultManagerError {
        VaultManagerError::Vault(format!("{what} lock was poisoned"))
    }

    /// Resolves reconnect credentials for a paired profile: stored pairing
    /// credentials first, then environment variables.
    fn credentials_for(&self, config: &VaultConfig) -> Option<TelegramCredentials> {
        if let Some(credentials) = config_credentials(config) {
            return Some(credentials);
        }
        let (api_id, api_hash) = self.env_credentials()?;
        Some(TelegramCredentials {
            api_id,
            api_hash,
            phone: None,
            login_code: None,
            two_factor_password: None,
        })
    }

    fn env_credentials(&self) -> Option<(i32, String)> {
        let api_id = std::env::var("TELEGRAM_API_ID").ok()?;
        let api_id = api_id.trim().parse::<i32>().ok()?;
        let api_hash = std::env::var("TELEGRAM_API_HASH").ok()?;
        let api_hash = api_hash.trim().to_owned();
        if api_hash.is_empty() {
            return None;
        }
        Some((api_id, api_hash))
    }

    fn session_file_for(&self, profile_id: &str) -> PathBuf {
        // Must match `profile_session_path` in the vault crate's live transport.
        self.data_dir
            .join("TelegramVault")
            .join("sessions")
            .join(format!("{profile_id}.session"))
    }

    fn session_file_exists(&self, profile_id: &str) -> bool {
        self.session_file_for(profile_id).is_file()
    }

    fn received_dir_for(&self, profile_id: &str) -> VaultManagerResult<PathBuf> {
        let profile_dir =
            (self.profile_dir_for)(profile_id).map_err(VaultManagerError::ProfileNotFound)?;
        Ok(profile_dir.join("Zalo Received Files"))
    }

    fn engine_running(&self, profile_id: &str) -> bool {
        self.engines
            .lock()
            .map(|map| map.contains_key(profile_id))
            .unwrap_or(false)
    }

    fn ensure_engine(&self, config: &VaultConfig) -> VaultManagerResult<()> {
        if self.engine_running(&config.profile_id) {
            return Ok(());
        }
        let received_dir = self.received_dir_for(&config.profile_id)?;
        let transport = self.consumer_transport_for(config);
        self.spawn_engine(config, transport, received_dir)
    }

    /// Resolves the consumer transport for one pairing config.
    ///
    /// Bot pairings are rebuilt statelessly from the stored token and chat
    /// (nothing to reconnect). MTProto pairings hand an existing live session
    /// to `Ready` when present and otherwise park the credentials in
    /// `MtprotoReconnect` so the consumer can (re)connect by itself.
    fn consumer_transport_for(&self, config: &VaultConfig) -> ConsumerTransport {
        match config.transport_kind() {
            Ok(TransportKind::Bot) => {
                let token = config
                    .telegram_bot_token
                    .as_deref()
                    .map(str::trim)
                    .unwrap_or_default();
                match (token.is_empty(), config.telegram_chat_id) {
                    (false, Some(chat_id)) => ConsumerTransport::Ready(Box::new(
                        BotTransport::new(token.to_owned(), chat_id),
                    )),
                    _ => ConsumerTransport::None,
                }
            }
            Ok(TransportKind::Mtproto) => {
                let initial = if self.session_file_exists(&config.profile_id) {
                    self.connect_existing_session(config).ok()
                } else {
                    None
                };
                match initial {
                    Some(transport) => ConsumerTransport::Ready(Box::new(transport)),
                    None => match self.credentials_for(config) {
                        Some(credentials) => ConsumerTransport::MtprotoReconnect(credentials),
                        None => ConsumerTransport::None,
                    },
                }
            }
            // Invalid transport credentials are rejected when the store saves configs;
            // a consumer cannot run either layer for them.
            Err(_) => ConsumerTransport::None,
        }
    }

    fn connect_existing_session(&self, config: &VaultConfig) -> VaultManagerResult<LiveTransport> {
        let Some(credentials) = self.credentials_for(config) else {
            return Err(VaultManagerError::MissingCredentials);
        };
        Ok(LiveTransport::connect(
            self.data_dir.as_ref(),
            &config.profile_id,
            &credentials,
        )?)
    }

    fn spawn_engine(
        &self,
        config: &VaultConfig,
        transport: ConsumerTransport,
        received_dir: PathBuf,
    ) -> VaultManagerResult<()> {
        let profile_id = config.profile_id.clone();
        self.stop_engine(&profile_id);

        let stop = Arc::new(AtomicBool::new(false));
        let wake = Arc::new(WakeSignal::default());

        let watcher_stop = Arc::clone(&stop);
        let watcher_wake = Arc::clone(&wake);
        let watcher_profile = profile_id.clone();
        let watcher_received = received_dir.clone();
        let watcher_queue = Arc::clone(&self.queue_path);
        let watcher_thread = std::thread::Builder::new()
            .name(format!("vault-watch-{watcher_profile}"))
            .spawn(move || {
                run_watcher_loop(
                    watcher_profile,
                    watcher_received,
                    watcher_queue,
                    watcher_stop,
                    watcher_wake,
                );
            })
            .map_err(|error| VaultManagerError::Vault(format!("spawn watcher failed: {error}")))?;

        let consumer_stop = Arc::clone(&stop);
        let consumer_wake = Arc::clone(&wake);
        let consumer_profile = profile_id.clone();
        let consumer_queue = Arc::clone(&self.queue_path);
        let consumer_data = Arc::clone(&self.data_dir);
        let consumer_thread = std::thread::Builder::new()
            .name(format!("vault-sync-{consumer_profile}"))
            .spawn(move || {
                run_consumer_loop(
                    consumer_profile,
                    consumer_data,
                    consumer_queue,
                    transport,
                    consumer_stop,
                    consumer_wake,
                );
            })
            .map_err(|error| {
                stop.store(true, Ordering::Release);
                VaultManagerError::Vault(format!("spawn consumer failed: {error}"))
            })?;

        if let Ok(mut map) = self.engines.lock() {
            map.insert(
                profile_id,
                Arc::new(ProfileEngine {
                    stop,
                    wake,
                    watcher_handle: Mutex::new(Some(watcher_thread)),
                    consumer_handle: Mutex::new(Some(consumer_thread)),
                }),
            );
        }
        Ok(())
    }

    fn stop_engine(&self, profile_id: &str) {
        let engine = self
            .engines
            .lock()
            .ok()
            .and_then(|mut map| map.remove(profile_id));
        if let Some(engine) = engine {
            engine.stop_and_join();
        }
    }
}

/// Watches the profile's received-files directory and enqueues stable files.
fn run_watcher_loop(
    profile_id: String,
    received_dir: PathBuf,
    queue_path: Arc<Path>,
    stop: Arc<AtomicBool>,
    wake: Arc<WakeSignal>,
) {
    while !stop.load(Ordering::Acquire) {
        let mut watcher = match ZaloReceivedWatcher::new(&received_dir) {
            Ok(watcher) => watcher,
            // Directory not created yet (profile never launched); retry later.
            Err(_) => {
                wake.wait(RECONNECT_DELAY);
                continue;
            }
        };
        loop {
            if stop.load(Ordering::Acquire) {
                return;
            }
            match watcher.poll() {
                Ok(files) => {
                    if !files.is_empty() {
                        let enqueued = enqueue_files(&profile_id, &queue_path, &files);
                        if enqueued > 0 {
                            wake.notify();
                        }
                    }
                }
                // Watcher died (directory replaced mid-session); recreate it.
                Err(_) => break,
            }
            wake.wait(WATCH_INTERVAL);
        }
    }
}

/// Enqueues newly stable files into the durable queue.
fn enqueue_files(profile_id: &str, queue_path: &Path, files: &[StableFile]) -> usize {
    let mut queue = match VaultQueue::open(queue_path) {
        Ok(queue) => queue,
        Err(_) => return 0,
    };
    let mut enqueued = 0_usize;
    for file in files {
        if queue
            .enqueue(profile_id, &file.path, &file.sha256, file.size)
            .unwrap_or(false)
        {
            enqueued += 1;
        }
    }
    enqueued
}

/// Drains the upload queue through whichever transport the pairing provides.
///
/// The transport is resolved once at spawn time: `Ready` pairs (bot, or an
/// MTProto session that already connected) reuse the built transport, while
/// `MtprotoReconnect` keeps retrying the persisted-session connect on each
/// outer pass. `None` idles until pairing completes and a fresh engine is
/// spawned.
fn run_consumer_loop(
    profile_id: String,
    data_dir: Arc<Path>,
    queue_path: Arc<Path>,
    transport: ConsumerTransport,
    stop: Arc<AtomicBool>,
    wake: Arc<WakeSignal>,
) {
    let mut transport = Some(transport);
    while !stop.load(Ordering::Acquire) {
        let client: Box<dyn TelegramTransport> = match transport.take() {
            Some(ConsumerTransport::Ready(transport)) => transport,
            Some(ConsumerTransport::MtprotoReconnect(credentials)) => {
                match LiveTransport::connect(&data_dir, &profile_id, &credentials) {
                    Ok(live) => Box::new(live),
                    Err(_) => {
                        // Connect failed with credentials present: retry after the delay.
                        transport = Some(ConsumerTransport::MtprotoReconnect(credentials));
                        wake.wait(RECONNECT_DELAY);
                        continue;
                    }
                }
            }
            Some(ConsumerTransport::None) | None => {
                // No credentials available: wait; a resumed manager spawns a
                // fresh engine once pairing provides them.
                transport = Some(ConsumerTransport::None);
                wake.wait(RECONNECT_DELAY);
                continue;
            }
        };
        let mut queue = match VaultQueue::open(queue_path.as_ref()) {
            Ok(queue) => queue,
            Err(_) => {
                wake.wait(RECONNECT_DELAY);
                continue;
            }
        };
        loop {
            if stop.load(Ordering::Acquire) {
                return;
            }
            match queue.claim_next(&profile_id, current_unix_seconds()) {
                Ok(Some(item)) => {
                    upload_item(&mut queue, client.as_ref(), &item);
                }
                Ok(None) => {
                    wake.wait(CONSUMER_IDLE_WAIT);
                }
                // Storage failure: reopen the queue on the next pass.
                Err(_) => break,
            }
        }
        // The transport stays usable; reuse it across queue reopen attempts.
        transport = Some(ConsumerTransport::Ready(client));
        wake.wait(RECONNECT_DELAY);
    }
}

/// Resolves stored Telegram application credentials from a pairing config.
fn config_credentials(config: &VaultConfig) -> Option<TelegramCredentials> {
    let api_hash = config.telegram_api_hash.as_ref()?.trim().to_owned();
    if api_hash.is_empty() {
        return None;
    }
    Some(TelegramCredentials {
        api_id: config.telegram_api_id?,
        api_hash,
        phone: None,
        login_code: None,
        two_factor_password: None,
    })
}

/// Uploads one claimed item through the live transport and records the outcome.
///
/// Queue state failures are never swallowed silently: a `mark_done` that finds
/// the row no longer `uploading` demotes the item to `failed` with the reason
/// so it becomes visible in the sync log instead of re-uploading forever.
fn upload_item(queue: &mut VaultQueue, client: &dyn TelegramTransport, item: &QueueItem) {
    match upload_with_caption(client, item) {
        Ok(()) => {
            if let Err(error) = queue.mark_done(item) {
                let _ = queue.mark_failed(item, &format!("mark_done failed: {error}"));
            }
        }
        Err(detail) => {
            let _ = queue.mark_failed(item, &detail);
        }
    }
}

/// Builds the metadata caption and uploads the document, like `SyncEngine`.
fn upload_with_caption(client: &dyn TelegramTransport, item: &QueueItem) -> Result<(), String> {
    let uploaded_at = current_unix_seconds();
    let file_name = item
        .path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file");
    let caption = serde_json::json!({
        "file_name": file_name,
        "sha256": item.sha256,
        "profile_id": item.profile_id,
        "uploaded_at": uploaded_at,
    })
    .to_string();
    match client.upload_document(&item.profile_id, &item.path, &item.sha256, &caption) {
        Ok(_remote) => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

fn current_unix_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| i64::try_from(duration.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_manager(dir: &Path) -> VaultManager {
        let data_dir = dir.to_path_buf();
        let profiles_root = dir.join("Profiles");
        VaultManager::new(
            data_dir,
            Arc::new(move |profile_id| Ok(profiles_root.join(profile_id))),
        )
        .expect("vault manager should initialize")
    }

    #[test]
    fn status_of_unpaired_profile_reports_defaults() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let manager = test_manager(tmp.path());
        let status = manager.status("work_1").expect("status");
        assert!(!status.paired);
        assert!(!status.enabled);
        assert!(!status.session_exists);
        assert!(!status.engine_running);
        assert_eq!(status.pending, 0);
        assert_eq!(status.last_synced_at, None);
        assert_eq!(status.transport, "mtproto");
        assert!(!status.bot_configured);
    }

    #[test]
    fn status_of_bot_pairing_reports_transport_and_capture_state() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let manager = test_manager(tmp.path());
        std::fs::create_dir_all(tmp.path().join("Profiles/work_1")).expect("profile dir");
        let config = VaultConfig {
            profile_id: "work_1".into(),
            telegram_session_dir: tmp.path().join("TelegramVault/sessions/work_1.session"),
            destination: Default::default(),
            enabled: true,
            transport: dim_telegram_vault::store::Transport::Bot,
            telegram_api_id: None,
            telegram_api_hash: None,
            telegram_bot_token: Some("123456789:AAE_abc-DEFxyz".into()),
            telegram_chat_id: Some(987_654_321),
            telegram_bot_username: Some("vault_bot".into()),
        };
        manager
            .config_store
            .save(&config)
            .expect("bot config should save");
        let status = manager.status("work_1").expect("status");
        assert!(status.paired);
        assert_eq!(status.transport, "bot");
        assert_eq!(status.bot_username.as_deref(), Some("vault_bot"));
        assert!(status.bot_configured);
        assert!(!status.session_exists);
    }

    #[test]
    fn status_of_token_only_bot_pairing_is_not_configured_yet() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let manager = test_manager(tmp.path());
        std::fs::create_dir_all(tmp.path().join("Profiles/work_1")).expect("profile dir");
        let config = VaultConfig {
            profile_id: "work_1".into(),
            telegram_session_dir: tmp.path().join("TelegramVault/sessions/work_1.session"),
            destination: Default::default(),
            enabled: true,
            transport: dim_telegram_vault::store::Transport::Bot,
            telegram_api_id: None,
            telegram_api_hash: None,
            telegram_bot_token: Some("123456789:AAE_abc-DEFxyz".into()),
            telegram_chat_id: None,
            telegram_bot_username: Some("vault_bot".into()),
        };
        manager
            .config_store
            .save(&config)
            .expect("token-only bot config should save");
        let status = manager.status("work_1").expect("status");
        assert_eq!(status.transport, "bot");
        assert!(!status.bot_configured);
    }

    #[test]
    fn pair_of_unpaired_profile_requires_login() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let manager = test_manager(tmp.path());
        let flow = manager.pair("work_1").expect("pair flow");
        assert!(!flow.paired);
        assert!(flow.requires_login);
    }

    #[test]
    fn log_without_queue_db_is_empty() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let manager = test_manager(tmp.path());
        assert!(manager.log("work_1", 10).expect("log").is_empty());
    }
}
