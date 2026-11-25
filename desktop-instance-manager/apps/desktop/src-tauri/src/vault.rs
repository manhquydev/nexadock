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
use std::time::{Duration, Instant};

use dim_telegram_vault::queue::{QueueItem, VaultQueue};
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
/// Maximum time `stop_engine` waits for an active upload thread before detaching.
const ENGINE_STOP_TIMEOUT: Duration = Duration::from_secs(5);
/// Maximum time `RunEvent::Exit` waits for vault threads so app exit never hangs.
pub const EXIT_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);

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

/// Signals when a background engine thread has finished executing so teardown
/// can join with a bounded deadline instead of hanging on uncancellable I/O.
#[derive(Debug, Default)]
struct ExitSignal {
    done: Mutex<bool>,
    cond: Condvar,
}

impl ExitSignal {
    fn mark_done(&self) {
        let mut done = match self.done.lock() {
            Ok(done) => done,
            Err(poisoned) => poisoned.into_inner(),
        };
        *done = true;
        self.cond.notify_all();
    }

    fn wait_until(&self, deadline: Option<Instant>) -> bool {
        let mut done = match self.done.lock() {
            Ok(done) => done,
            Err(poisoned) => poisoned.into_inner(),
        };
        while !*done {
            match deadline {
                None => {
                    done = match self.cond.wait(done) {
                        Ok(done) => done,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                }
                Some(dl) => {
                    let remaining = dl.saturating_duration_since(Instant::now());
                    if remaining.is_zero() {
                        return false;
                    }
                    let (next, timed_out) = match self.cond.wait_timeout(done, remaining) {
                        Ok(result) => result,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    done = next;
                    if timed_out.timed_out() && !*done {
                        return false;
                    }
                }
            }
        }
        true
    }
}

struct ExitGuard(Arc<ExitSignal>);

impl Drop for ExitGuard {
    fn drop(&mut self) {
        self.0.mark_done();
    }
}

/// Joinable worker handles for one profile's sync engine.
///
/// Separate wake signals prevent one thread from consuming a notification
/// meant for the other (or for shutdown).
struct ProfileEngine {
    stop: Arc<AtomicBool>,
    watcher_wake: Arc<WakeSignal>,
    consumer_wake: Arc<WakeSignal>,
    watcher_done: Arc<ExitSignal>,
    consumer_done: Arc<ExitSignal>,
    watcher_handle: Mutex<Option<JoinHandle<()>>>,
    consumer_handle: Mutex<Option<JoinHandle<()>>>,
}

impl ProfileEngine {
    fn signal_stop(&self) {
        self.stop.store(true, Ordering::Release);
        self.watcher_wake.notify();
        self.consumer_wake.notify();
    }

    fn join_until(&self, deadline: Option<Instant>) {
        if self.watcher_done.wait_until(deadline) {
            if let Ok(mut slot) = self.watcher_handle.lock() {
                if let Some(handle) = slot.take() {
                    let _ = handle.join();
                }
            }
        }
        if self.consumer_done.wait_until(deadline) {
            if let Ok(mut slot) = self.consumer_handle.lock() {
                if let Some(handle) = slot.take() {
                    let _ = handle.join();
                }
            }
        }
    }

    fn stop_and_join_until(&self, deadline: Option<Instant>) {
        self.signal_stop();
        self.join_until(deadline);
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
    /// Telegram application credentials are available (stored pairing or env).
    pub credentials_available: bool,
    /// Active pairing transport: `"bot"` or `"mtproto"`.
    pub transport: String,
    /// The bot pairing is fully configured (token stored, chat captured).
    pub bot_configured: bool,
    /// Public username of the bot, available while the owner is still connecting it.
    pub bot_username: Option<String>,
    /// Deep link (`https://t.me/<username>?start=<nonce>`) for continuing half-paired bots.
    pub bot_deep_link: Option<String>,
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
    pairing_epochs: Arc<Mutex<HashMap<String, u64>>>,
    engines: Arc<Mutex<HashMap<String, Arc<ProfileEngine>>>>,
    config_lock: Arc<Mutex<()>>,
    profile_dir_for: ProfileDirResolver,
}

struct PendingLogin {
    pending: dim_telegram_vault::transport::PendingPhoneLogin,
    received_dir: PathBuf,
    epoch: u64,
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
            pairing_epochs: Arc::new(Mutex::new(HashMap::new())),
            engines: Arc::new(Mutex::new(HashMap::new())),
            config_lock: Arc::new(Mutex::new(())),
            profile_dir_for,
        })
    }

    fn current_pairing_epoch(&self, profile_id: &str) -> VaultManagerResult<u64> {
        let epochs = self
            .pairing_epochs
            .lock()
            .map_err(|_| Self::lock_poisoned("pairing epoch"))?;
        Ok(epochs.get(profile_id).copied().unwrap_or(0))
    }

    fn bump_pairing_epoch(&self, profile_id: &str) -> VaultManagerResult<u64> {
        let mut epochs = self
            .pairing_epochs
            .lock()
            .map_err(|_| Self::lock_poisoned("pairing epoch"))?;
        let next = epochs.get(profile_id).copied().unwrap_or(0).wrapping_add(1);
        epochs.insert(profile_id.to_owned(), next);
        Ok(next)
    }

    /// Restarts sync engines for every paired profile that is enabled and has
    /// a persisted session. Called once during application startup; performs
    /// the once-per-process crash recovery of interrupted uploads first.
    pub fn resume_all(&self) {
        // Requeue items left `uploading` by an unclean exit before any
        // consumer can claim work; plain `open()` never touches row state.
        // If opening or crash recovery fails, do not start consumers with
        // unrecovered `uploading` rows or an unusable queue database.
        let queue = match VaultQueue::open(&self.queue_path) {
            Ok(queue) => queue,
            Err(error) => {
                eprintln!("NexaDock vault queue unavailable during startup: {error}");
                return;
            }
        };
        if let Err(error) = queue.recover_interrupted() {
            eprintln!("NexaDock vault crash recovery failed: {error}");
            return;
        }
        let configs = match self.config_store.list() {
            Ok(configs) => configs,
            Err(error) => {
                eprintln!("NexaDock vault failed to list configs during startup: {error}");
                return;
            }
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

    /// Restarts the sync engine for a single profile if it is paired, enabled,
    /// and runnable (e.g. after a cancelled profile deletion).
    pub fn resume_profile(&self, profile_id: &str) {
        let Ok(_guard) = self.config_lock.lock() else {
            return;
        };
        if let Ok(Some(config)) = self.config_store.load(profile_id) {
            if config.enabled && self.pairing_is_runnable(&config) {
                let _ = self.ensure_engine(&config);
            }
        }
    }

    /// Stops only the active engine for `profile_id` without removing config or session.
    pub fn stop_profile_engine(&self, profile_id: &str) {
        self.stop_engine(profile_id);
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

    /// Shuts down every engine thread with a bounded teardown timeout.
    pub fn shutdown(&self) {
        self.shutdown_with_timeout(EXIT_SHUTDOWN_TIMEOUT);
    }

    /// Signals all engine threads to stop in parallel and waits up to `timeout`
    /// total before detaching any thread still stuck in uncancellable network I/O.
    pub fn shutdown_with_timeout(&self, timeout: Duration) {
        let engines: Vec<Arc<ProfileEngine>> = {
            let Ok(mut map) = self.engines.lock() else {
                return;
            };
            map.drain().map(|(_, engine)| engine).collect()
        };
        for engine in &engines {
            engine.signal_stop();
        }
        let deadline = Instant::now() + timeout;
        for engine in engines {
            engine.join_until(Some(deadline));
        }
    }

    /// Returns the current pairing/sync status snapshot for one profile.
    pub fn status(&self, profile_id: &str) -> VaultManagerResult<VaultStatus> {
        let config = self.config_store.load(profile_id)?;
        let credentials_available = config.as_ref().and_then(config_credentials).is_some()
            || self.env_credentials().is_some();
        let bot_deep_link = config.as_ref().and_then(|value| {
            if value.transport == Transport::Bot && value.telegram_chat_id.is_none() {
                let username = value.telegram_bot_username.as_deref()?;
                match value.telegram_bot_pair_nonce.as_deref() {
                    Some(nonce) if !nonce.is_empty() => {
                        Some(format!("https://t.me/{username}?start={nonce}"))
                    }
                    _ => Some(format!("https://t.me/{username}")),
                }
            } else {
                None
            }
        });
        let mut snapshot = VaultStatus {
            paired: config.is_some(),
            enabled: config.as_ref().is_some_and(|value| value.enabled),
            session_exists: self.session_file_exists(profile_id),
            engine_running: self.engine_running(profile_id),
            queue_db_exists: self.queue_path.exists(),
            credentials_available,
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
            bot_deep_link,
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
        let counts = queue.profile_counts(profile_id)?;
        snapshot.pending = counts.pending;
        snapshot.uploading = counts.uploading;
        snapshot.failed = counts.failed;
        snapshot.done = counts.done;
        snapshot.last_synced_at = queue.last_done_ts(profile_id)?;
        Ok(snapshot)
    }

    /// Starts or resumes pairing. Returns the login requirements for the UI.
    pub fn pair(&self, profile_id: &str) -> VaultManagerResult<PairFlow> {
        let config = self.config_store.load(profile_id)?;
        let stored = config.as_ref().and_then(config_credentials);
        let credentials_available = stored.is_some() || self.env_credentials().is_some();
        if let Some(config) = config {
            // Bot pairings never use the phone-login flow.
            if config.transport == Transport::Bot && self.pairing_is_runnable(&config) {
                if config.enabled {
                    self.ensure_engine(&config)?;
                }
                return Ok(PairFlow {
                    paired: true,
                    requires_login: false,
                    credentials_available,
                });
            }
            if config.transport == Transport::Mtproto
                && self.session_file_exists(profile_id)
                && self.credentials_for(&config).is_some()
            {
                // Already paired MTProto with a usable session and credentials.
                if config.enabled {
                    self.ensure_engine(&config)?;
                }
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
        let (received_dir, epoch) = {
            let _guard = self
                .config_lock
                .lock()
                .map_err(|_| Self::lock_poisoned("config"))?;
            let received_dir = self.received_dir_for(profile_id)?;
            // Stop any running engine for this profile before opening FileSession
            // so two FileSession instances never race on the session file.
            self.stop_engine(profile_id);
            let epoch = self.bump_pairing_epoch(profile_id)?;
            (received_dir, epoch)
        };
        let pending = LiveTransport::begin_phone_login(
            self.data_dir.as_ref(),
            profile_id,
            resolved_api_id,
            &resolved_api_hash,
            phone,
        )?;
        let _guard = self
            .config_lock
            .lock()
            .map_err(|_| Self::lock_poisoned("config"))?;
        if self.current_pairing_epoch(profile_id)? != epoch {
            return Err(VaultManagerError::Vault(
                "pairing was cancelled or replaced while requesting login code".into(),
            ));
        }
        let mut logins = self
            .pending_logins
            .lock()
            .map_err(|_| Self::lock_poisoned("pending login"))?;
        logins.insert(
            profile_id.to_owned(),
            PendingLogin {
                pending,
                received_dir,
                epoch,
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
        self.commit_login_code(
            profile_id,
            pending.epoch,
            pending.api_id,
            pending.api_hash,
            pending.received_dir,
            Box::new(transport),
        )
    }

    fn commit_login_code(
        &self,
        profile_id: &str,
        expected_epoch: u64,
        api_id: i32,
        api_hash: String,
        received_dir: PathBuf,
        transport: Box<dyn TelegramTransport>,
    ) -> VaultManagerResult<()> {
        let _guard = self
            .config_lock
            .lock()
            .map_err(|_| Self::lock_poisoned("config"))?;
        if self.current_pairing_epoch(profile_id)? != expected_epoch {
            return Err(VaultManagerError::Vault(
                "pairing was cancelled or replaced before login completed".into(),
            ));
        }
        self.received_dir_for(profile_id)?;
        let existing = self.config_store.load(profile_id)?;
        let enabled = existing
            .as_ref()
            .filter(|cfg| cfg.transport == Transport::Mtproto)
            .is_none_or(|cfg| cfg.enabled);
        let config = VaultConfig {
            profile_id: profile_id.to_owned(),
            telegram_session_dir: self.session_file_for(profile_id),
            destination: Default::default(),
            enabled,
            transport: Transport::Mtproto,
            telegram_api_id: Some(api_id),
            telegram_api_hash: Some(api_hash),
            telegram_bot_token: None,
            telegram_chat_id: None,
            telegram_bot_username: None,
            telegram_bot_pair_nonce: None,
        };
        self.config_store.save(&config)?;
        if config.enabled {
            self.spawn_engine(&config, ConsumerTransport::Ready(transport), received_dir)?;
        }
        Ok(())
    }

    /// Validates a bot token, generates a per-pair `/start <nonce>` token, stops
    /// any existing engine, and stores the bot transport pairing.
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
        self.commit_pair_bot(profile_id, token, &info.username)
    }

    fn commit_pair_bot(
        &self,
        profile_id: &str,
        token: &str,
        username: &str,
    ) -> VaultManagerResult<BotPairFlow> {
        let nonce = BotTransport::generate_pair_nonce();
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
            telegram_bot_username: Some(username.to_owned()),
            telegram_bot_pair_nonce: Some(nonce.clone()),
        };
        {
            let _guard = self
                .config_lock
                .lock()
                .map_err(|_| Self::lock_poisoned("config"))?;
            self.received_dir_for(profile_id)?;
            self.stop_engine(profile_id);
            self.bump_pairing_epoch(profile_id)?;
            if let Ok(mut logins) = self.pending_logins.lock() {
                logins.remove(profile_id);
            }
            self.config_store.save(&config)?;
        }
        Ok(BotPairFlow {
            bot_username: username.to_owned(),
            deep_link: format!("https://t.me/{username}?start={nonce}"),
        })
    }

    /// Waits for the vault owner to message the bot with `/start <nonce>`,
    /// captures the private chat id, persists it cleanly against fresh state,
    /// and starts the sync engine only if still enabled.
    ///
    /// Blocking (long-polls the Bot API); run it off the UI thread, e.g. via
    /// `tauri::async_runtime::spawn_blocking`.
    pub fn pair_bot_wait(&self, profile_id: &str) -> VaultManagerResult<BotWaitOutcome> {
        self.pair_bot_wait_with(profile_id, |token, nonce| {
            BotTransport::wait_for_chat(token, nonce, BOT_CHAT_WAIT_SECS)
        })
    }

    fn pair_bot_wait_with<F>(
        &self,
        profile_id: &str,
        wait_fn: F,
    ) -> VaultManagerResult<BotWaitOutcome>
    where
        F: FnOnce(&str, &str) -> dim_telegram_vault::Result<i64>,
    {
        let (token, nonce) = {
            let _guard = self
                .config_lock
                .lock()
                .map_err(|_| Self::lock_poisoned("config"))?;
            let config = self.config_store.load(profile_id)?.ok_or_else(|| {
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
            if config.telegram_chat_id.is_some() {
                if config.enabled {
                    self.ensure_engine(&config)?;
                }
                drop(_guard);
                return Ok(BotWaitOutcome::Captured(self.status(profile_id)?));
            }
            let nonce = config
                .telegram_bot_pair_nonce
                .as_deref()
                .map(str::trim)
                .filter(|nonce| !nonce.is_empty())
                .ok_or_else(|| {
                    VaultManagerError::Vault(
                        "bot pairing link expired; submit the bot token again".into(),
                    )
                })?
                .to_owned();
            (token, nonce)
        };

        let chat_id = match wait_fn(&token, &nonce) {
            Ok(chat_id) => chat_id,
            Err(dim_telegram_vault::VaultError::Timeout(_)) => {
                return Ok(BotWaitOutcome::TimedOut);
            }
            Err(error) => return Err(error.into()),
        };

        {
            let _guard = self
                .config_lock
                .lock()
                .map_err(|_| Self::lock_poisoned("config"))?;
            let mut fresh = self.config_store.load(profile_id)?.ok_or_else(|| {
                VaultManagerError::Vault(
                    "vault pairing was removed before bot confirmation completed".into(),
                )
            })?;
            if fresh.transport != Transport::Bot
                || fresh.telegram_bot_token.as_deref().map(str::trim) != Some(token.as_str())
            {
                return Err(VaultManagerError::Vault(
                    "bot pairing changed before confirmation completed".into(),
                ));
            }
            if fresh.telegram_chat_id.is_none() {
                if fresh.telegram_bot_pair_nonce.as_deref() != Some(nonce.as_str()) {
                    return Err(VaultManagerError::Vault(
                        "bot pairing link was replaced; confirm with the latest link".into(),
                    ));
                }
                fresh.telegram_chat_id = Some(chat_id);
                fresh.telegram_bot_pair_nonce = None;
                self.config_store.save(&fresh)?;
            }
            if fresh.enabled {
                self.ensure_engine(&fresh)?;
            }
        }
        Ok(BotWaitOutcome::Captured(self.status(profile_id)?))
    }

    /// Removes the pairing; the persisted MTProto session file is only deleted
    /// when explicitly requested, and Telegram Desktop `tdata` is never touched.
    pub fn unpair(&self, profile_id: &str, delete_session: bool) -> VaultManagerResult<bool> {
        let _guard = self
            .config_lock
            .lock()
            .map_err(|_| Self::lock_poisoned("config"))?;
        self.stop_engine(profile_id);
        self.bump_pairing_epoch(profile_id)?;
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
        let _guard = self
            .config_lock
            .lock()
            .map_err(|_| Self::lock_poisoned("config"))?;
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

    /// Wakes both watcher and consumer threads and returns the resulting queue snapshot.
    pub fn sync_now(&self, profile_id: &str) -> VaultManagerResult<VaultStatus> {
        if let Ok(map) = self.engines.lock() {
            if let Some(engine) = map.get(profile_id) {
                engine.watcher_wake.notify();
                engine.consumer_wake.notify();
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
        for entry in queue.log_for_profile(profile_id, limit)? {
            rows.push(VaultLogRow {
                ts: entry.ts,
                file: entry.file.display().to_string(),
                size: entry.size,
                outcome: entry.outcome,
                error: entry.error,
            });
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
        let watcher_wake = Arc::new(WakeSignal::default());
        let consumer_wake = Arc::new(WakeSignal::default());
        let watcher_done = Arc::new(ExitSignal::default());
        let consumer_done = Arc::new(ExitSignal::default());

        let watcher_stop = Arc::clone(&stop);
        let watcher_self_wake = Arc::clone(&watcher_wake);
        let watcher_consumer_wake = Arc::clone(&consumer_wake);
        let watcher_done_signal = Arc::clone(&watcher_done);
        let watcher_profile = profile_id.clone();
        let watcher_received = received_dir.clone();
        let watcher_queue = Arc::clone(&self.queue_path);
        let watcher_thread = std::thread::Builder::new()
            .name(format!("vault-watch-{watcher_profile}"))
            .spawn(move || {
                let _exit_guard = ExitGuard(watcher_done_signal);
                run_watcher_loop(
                    watcher_profile,
                    watcher_received,
                    watcher_queue,
                    watcher_stop,
                    watcher_self_wake,
                    watcher_consumer_wake,
                );
            })
            .map_err(|error| VaultManagerError::Vault(format!("spawn watcher failed: {error}")))?;

        let consumer_stop = Arc::clone(&stop);
        let consumer_self_wake = Arc::clone(&consumer_wake);
        let consumer_done_signal = Arc::clone(&consumer_done);
        let consumer_profile = profile_id.clone();
        let consumer_queue = Arc::clone(&self.queue_path);
        let consumer_data = Arc::clone(&self.data_dir);
        let consumer_thread = std::thread::Builder::new()
            .name(format!("vault-sync-{consumer_profile}"))
            .spawn(move || {
                let _exit_guard = ExitGuard(consumer_done_signal);
                run_consumer_loop(
                    consumer_profile,
                    consumer_data,
                    consumer_queue,
                    transport,
                    consumer_stop,
                    consumer_self_wake,
                );
            })
            .map_err(|error| {
                stop.store(true, Ordering::Release);
                watcher_wake.notify();
                VaultManagerError::Vault(format!("spawn consumer failed: {error}"))
            })?;

        if let Ok(mut map) = self.engines.lock() {
            map.insert(
                profile_id,
                Arc::new(ProfileEngine {
                    stop,
                    watcher_wake,
                    consumer_wake,
                    watcher_done,
                    consumer_done,
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
            engine.stop_and_join_until(Some(Instant::now() + ENGINE_STOP_TIMEOUT));
        }
    }
}

/// Watches the profile's received-files directory and enqueues stable files.
fn run_watcher_loop(
    profile_id: String,
    received_dir: PathBuf,
    queue_path: Arc<Path>,
    stop: Arc<AtomicBool>,
    watcher_wake: Arc<WakeSignal>,
    consumer_wake: Arc<WakeSignal>,
) {
    while !stop.load(Ordering::Acquire) {
        let mut watcher = match ZaloReceivedWatcher::new(&received_dir) {
            Ok(watcher) => watcher,
            // Directory not created yet (profile never launched); retry later.
            Err(_) => {
                watcher_wake.wait(RECONNECT_DELAY);
                continue;
            }
        };
        loop {
            if stop.load(Ordering::Acquire) {
                return;
            }
            match watcher.poll() {
                Ok(files) if !files.is_empty() => {
                    match enqueue_files(&profile_id, &queue_path, &mut watcher, &files) {
                        Ok(enqueued) if enqueued > 0 => consumer_wake.notify(),
                        Ok(_) => {}
                        Err(error) => {
                            eprintln!("NexaDock vault enqueue failed for {profile_id}: {error}");
                        }
                    }
                }
                Ok(_) => {}
                // Watcher died (directory replaced mid-session); recreate it.
                Err(_) => break,
            }
            watcher_wake.wait(WATCH_INTERVAL);
        }
    }
}

/// Enqueues newly stable files into the durable queue, marking each path emitted
/// in `watcher` only after SQLite durably stores it.
fn enqueue_files(
    profile_id: &str,
    queue_path: &Path,
    watcher: &mut ZaloReceivedWatcher,
    files: &[StableFile],
) -> dim_telegram_vault::Result<usize> {
    let mut queue = VaultQueue::open(queue_path)?;
    let mut enqueued = 0_usize;
    for file in files {
        if queue.enqueue(profile_id, &file.path, &file.sha256, file.size)? {
            enqueued += 1;
        }
        watcher.mark_emitted(&file.path);
    }
    Ok(enqueued)
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
    consumer_wake: Arc<WakeSignal>,
) {
    run_consumer_loop_with_observer(
        profile_id,
        data_dir,
        queue_path,
        transport,
        stop,
        consumer_wake,
        None,
    );
}

fn run_consumer_loop_with_observer(
    profile_id: String,
    data_dir: Arc<Path>,
    queue_path: Arc<Path>,
    transport: ConsumerTransport,
    stop: Arc<AtomicBool>,
    consumer_wake: Arc<WakeSignal>,
    on_queue_open_err: Option<&(dyn Fn() + Sync)>,
) {
    let mut transport = Some(transport);
    let mut recover_needed = false;
    while !stop.load(Ordering::Acquire) {
        let client: Box<dyn TelegramTransport> = match transport.take() {
            Some(ConsumerTransport::Ready(transport)) => transport,
            Some(ConsumerTransport::MtprotoReconnect(credentials)) => {
                match LiveTransport::connect(&data_dir, &profile_id, &credentials) {
                    Ok(live) => Box::new(live),
                    Err(_) => {
                        // Connect failed with credentials present: retry after the delay.
                        transport = Some(ConsumerTransport::MtprotoReconnect(credentials));
                        consumer_wake.wait(RECONNECT_DELAY);
                        continue;
                    }
                }
            }
            Some(ConsumerTransport::None) | None => {
                // No credentials available: wait; a resumed manager spawns a
                // fresh engine once pairing provides them.
                transport = Some(ConsumerTransport::None);
                consumer_wake.wait(RECONNECT_DELAY);
                continue;
            }
        };
        let mut queue = match VaultQueue::open(queue_path.as_ref()) {
            Ok(queue) => queue,
            Err(_) => {
                transport = Some(ConsumerTransport::Ready(client));
                if let Some(hook) = on_queue_open_err {
                    hook();
                }
                consumer_wake.wait(RECONNECT_DELAY);
                continue;
            }
        };
        if recover_needed {
            match queue.recover_interrupted_for_profile(&profile_id) {
                Ok(_) => recover_needed = false,
                Err(error) => {
                    eprintln!(
                        "NexaDock vault profile queue recovery failed for {profile_id}: {error}"
                    );
                    transport = Some(ConsumerTransport::Ready(client));
                    consumer_wake.wait(RECONNECT_DELAY);
                    continue;
                }
            }
        }
        loop {
            if stop.load(Ordering::Acquire) {
                return;
            }
            match queue.claim_next(&profile_id, current_unix_seconds()) {
                Ok(Some(item)) => {
                    if let Err(error) = upload_item(&mut queue, client.as_ref(), &item) {
                        eprintln!("NexaDock vault queue update failed for {profile_id}: {error}");
                        recover_needed = true;
                        break;
                    }
                }
                Ok(None) => {
                    consumer_wake.wait(CONSUMER_IDLE_WAIT);
                }
                // Storage failure: reopen the queue on the next pass.
                Err(_) => break,
            }
        }
        // The transport stays usable; reuse it across queue reopen attempts.
        transport = Some(ConsumerTransport::Ready(client));
        consumer_wake.wait(RECONNECT_DELAY);
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
/// Queue state failures are never swallowed silently: both `mark_done` and
/// `mark_failed` errors propagate to the consumer loop so it reopens the queue
/// and recovers any stranded `uploading` row for this profile.
fn upload_item(
    queue: &mut VaultQueue,
    client: &dyn TelegramTransport,
    item: &QueueItem,
) -> dim_telegram_vault::Result<()> {
    match upload_with_caption(client, item) {
        Ok(()) => {
            if let Err(error) = queue.mark_done(item) {
                queue.mark_failed(item, &format!("mark_done failed: {error}"))?;
            }
            Ok(())
        }
        Err(detail) => queue.mark_failed(item, &detail),
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
            telegram_bot_pair_nonce: None,
        };
        manager
            .config_store
            .save(&config)
            .expect("bot config should save");
        let status = manager.status("work_1").expect("status");
        assert!(status.paired);
        assert_eq!(status.transport, "bot");
        assert_eq!(status.bot_username.as_deref(), Some("vault_bot"));
        assert_eq!(status.bot_deep_link, None);
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
            telegram_bot_pair_nonce: Some("nonce42".into()),
        };
        manager
            .config_store
            .save(&config)
            .expect("token-only bot config should save");
        let status = manager.status("work_1").expect("status");
        assert_eq!(status.transport, "bot");
        assert!(!status.bot_configured);
        assert_eq!(
            status.bot_deep_link.as_deref(),
            Some("https://t.me/vault_bot?start=nonce42")
        );
    }

    #[test]
    fn status_reports_credentials_available_for_stored_mtproto_config() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let manager = test_manager(tmp.path());
        let config = VaultConfig {
            profile_id: "work_1".into(),
            telegram_session_dir: tmp.path().join("TelegramVault/sessions/work_1.session"),
            destination: Default::default(),
            enabled: true,
            transport: Transport::Mtproto,
            telegram_api_id: Some(12345),
            telegram_api_hash: Some("api-hash-secret".into()),
            telegram_bot_token: None,
            telegram_chat_id: None,
            telegram_bot_username: None,
            telegram_bot_pair_nonce: None,
        };
        manager.config_store.save(&config).expect("save config");
        let status = manager.status("work_1").expect("status");
        assert!(status.credentials_available);
        assert!(!status.session_exists);
    }

    #[test]
    fn pair_ignores_leftover_session_when_active_transport_is_half_paired_bot() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let manager = test_manager(tmp.path());
        let session_path = manager.session_file_for("work_1");
        std::fs::create_dir_all(session_path.parent().expect("parent")).expect("mkdir");
        std::fs::write(&session_path, b"{}").expect("dummy session");
        let config = VaultConfig {
            profile_id: "work_1".into(),
            telegram_session_dir: session_path,
            destination: Default::default(),
            enabled: true,
            transport: Transport::Bot,
            telegram_api_id: None,
            telegram_api_hash: None,
            telegram_bot_token: Some("123456789:AAE_abc-DEFxyz".into()),
            telegram_chat_id: None,
            telegram_bot_username: Some("vault_bot".into()),
            telegram_bot_pair_nonce: Some("nonce_bot".into()),
        };
        manager.config_store.save(&config).expect("save bot config");

        let flow = manager.pair("work_1").expect("pair flow");
        assert!(!flow.paired);
        assert!(flow.requires_login);
        assert!(!manager.engine_running("work_1"));
    }

    #[test]
    fn pair_bot_wait_respects_concurrent_unpair_disable_and_nonce_replacement() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let manager = test_manager(tmp.path());
        std::fs::create_dir_all(tmp.path().join("Profiles/work_1/Zalo Received Files"))
            .expect("received dir");

        let save_bot = |nonce: &str| {
            let config = VaultConfig {
                profile_id: "work_1".into(),
                telegram_session_dir: manager.session_file_for("work_1"),
                destination: Default::default(),
                enabled: true,
                transport: Transport::Bot,
                telegram_api_id: None,
                telegram_api_hash: None,
                telegram_bot_token: Some("123456:secret".into()),
                telegram_chat_id: None,
                telegram_bot_username: Some("vault_bot".into()),
                telegram_bot_pair_nonce: Some(nonce.to_owned()),
            };
            manager.config_store.save(&config).expect("save");
        };

        // 1. Concurrent unpair aborts save.
        save_bot("nonce_unpair");
        let err = manager
            .pair_bot_wait_with("work_1", |_, _| {
                manager.unpair("work_1", false).expect("unpair");
                Ok(999)
            })
            .unwrap_err();
        assert!(err.to_string().contains("removed"));
        assert!(manager.config_store.load("work_1").unwrap().is_none());

        // 2. Concurrent set_enabled(false) preserves enabled=false and skips engine spawn.
        save_bot("nonce_disable");
        let outcome = manager
            .pair_bot_wait_with("work_1", |_, _| {
                manager.set_enabled("work_1", false).expect("disable");
                Ok(777)
            })
            .expect("wait");
        match outcome {
            BotWaitOutcome::Captured(status) => {
                assert!(!status.enabled);
                assert!(status.bot_configured);
                assert!(!status.engine_running);
            }
            BotWaitOutcome::TimedOut => panic!("expected Captured"),
        }
        assert!(!manager.engine_running("work_1"));

        // 3. Concurrent nonce replacement aborts stale waiter.
        save_bot("nonce_old");
        let err = manager
            .pair_bot_wait_with("work_1", |_, _| {
                save_bot("nonce_new");
                Ok(888)
            })
            .unwrap_err();
        assert!(err.to_string().contains("replaced"));
        let latest = manager.config_store.load("work_1").unwrap().unwrap();
        assert_eq!(latest.telegram_chat_id, None);
        assert_eq!(latest.telegram_bot_pair_nonce.as_deref(), Some("nonce_new"));
    }

    #[test]
    fn separate_wake_signals_never_swallow_sibling_or_shutdown_wakeups() {
        let watcher_wake = Arc::new(WakeSignal::default());
        let consumer_wake = Arc::new(WakeSignal::default());
        let stop = Arc::new(AtomicBool::new(false));
        let w_done = Arc::new(ExitSignal::default());
        let c_done = Arc::new(ExitSignal::default());

        let w_wake = Arc::clone(&watcher_wake);
        let w_stop = Arc::clone(&stop);
        let w_exit = Arc::clone(&w_done);
        let w_handle = std::thread::spawn(move || {
            let _g = ExitGuard(w_exit);
            while !w_stop.load(Ordering::Acquire) {
                w_wake.wait(Duration::from_secs(30));
            }
        });

        let c_wake = Arc::clone(&consumer_wake);
        let c_stop = Arc::clone(&stop);
        let c_exit = Arc::clone(&c_done);
        let c_handle = std::thread::spawn(move || {
            let _g = ExitGuard(c_exit);
            while !c_stop.load(Ordering::Acquire) {
                c_wake.wait(Duration::from_secs(30));
            }
        });

        let engine = ProfileEngine {
            stop,
            watcher_wake,
            consumer_wake,
            watcher_done: w_done,
            consumer_done: c_done,
            watcher_handle: Mutex::new(Some(w_handle)),
            consumer_handle: Mutex::new(Some(c_handle)),
        };

        let start = Instant::now();
        engine.stop_and_join_until(Some(start + Duration::from_secs(2)));
        assert!(
            start.elapsed() < Duration::from_millis(500),
            "both threads must wake immediately without waiting on timeout"
        );
    }

    #[test]
    fn shutdown_with_timeout_returns_promptly_when_worker_is_blocked() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let manager = test_manager(tmp.path());
        let (tx, rx) = std::sync::mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let watcher_wake = Arc::new(WakeSignal::default());
        let consumer_wake = Arc::new(WakeSignal::default());
        let watcher_done = Arc::new(ExitSignal::default());
        let consumer_done = Arc::new(ExitSignal::default());
        watcher_done.mark_done();
        let c_done = Arc::clone(&consumer_done);
        let handle = std::thread::spawn(move || {
            let _g = ExitGuard(c_done);
            let _ = rx.recv_timeout(Duration::from_secs(10));
        });
        manager.engines.lock().expect("lock").insert(
            "blocked".into(),
            Arc::new(ProfileEngine {
                stop,
                watcher_wake,
                consumer_wake,
                watcher_done,
                consumer_done,
                watcher_handle: Mutex::new(None),
                consumer_handle: Mutex::new(Some(handle)),
            }),
        );

        let start = Instant::now();
        manager.shutdown_with_timeout(Duration::from_millis(60));
        assert!(
            start.elapsed() < Duration::from_millis(500),
            "shutdown_with_timeout must bound app exit instead of blocking on uncancellable I/O"
        );
        let _ = tx.send(());
    }

    #[test]
    fn enqueue_files_returns_err_without_marking_emitted_when_queue_open_fails() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let received = tmp.path().join("received");
        std::fs::create_dir_all(&received).expect("mkdir");
        let file = received.join("item.bin");
        std::fs::write(&file, b"payload").expect("write");
        let mut watcher =
            ZaloReceivedWatcher::with_debounce(&received, Duration::from_millis(5)).expect("watch");
        assert!(watcher.poll().expect("poll 1").is_empty());
        let stable = watcher.poll().expect("poll 2");
        assert_eq!(stable.len(), 1);

        // Point queue_path at a directory so VaultQueue::open fails.
        let bad_queue_path = tmp.path().join("not_a_db_file");
        std::fs::create_dir_all(&bad_queue_path).expect("mkdir");
        let err = enqueue_files("work_1", &bad_queue_path, &mut watcher, &stable).unwrap_err();
        let _ = err;
        // Because enqueue failed, the watcher must still emit the file on the next poll.
        assert_eq!(watcher.poll().expect("retry poll").len(), 1);
    }

    #[test]
    fn resume_all_skips_spawning_when_queue_unavailable() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let manager = test_manager(tmp.path());
        std::fs::create_dir_all(tmp.path().join("Profiles/work_1/Zalo Received Files"))
            .expect("mkdir");
        std::fs::create_dir_all(manager.queue_path.as_ref()).expect("block queue path with dir");
        let config = VaultConfig {
            profile_id: "work_1".into(),
            telegram_session_dir: manager.session_file_for("work_1"),
            destination: Default::default(),
            enabled: true,
            transport: Transport::Bot,
            telegram_api_id: None,
            telegram_api_hash: None,
            telegram_bot_token: Some("123456:secret".into()),
            telegram_chat_id: Some(42),
            telegram_bot_username: Some("vault_bot".into()),
            telegram_bot_pair_nonce: None,
        };
        manager.config_store.save(&config).expect("save");
        manager.resume_all();
        assert!(!manager.engine_running("work_1"));
    }

    #[test]
    fn consumer_loop_retains_transport_after_transient_queue_open_failure() {
        use dim_telegram_vault::transport::MockTransport;
        use dim_telegram_vault::watch::sha256_file;

        let tmp = tempfile::tempdir().expect("tempdir");
        let queue_path = tmp.path().join("queue.db");
        // Block queue_path with a directory so the first VaultQueue::open fails.
        std::fs::create_dir_all(&queue_path).expect("create blocking dir");

        let file = tmp.path().join("doc.bin");
        std::fs::write(&file, b"important-doc").expect("write file");
        let digest = sha256_file(&file).expect("digest");

        #[derive(Clone)]
        struct SharedMock(Arc<MockTransport>);
        impl TelegramTransport for SharedMock {
            fn upload_document(
                &self,
                profile_id: &str,
                path: &Path,
                sha256: &str,
                caption: &str,
            ) -> dim_telegram_vault::Result<dim_telegram_vault::transport::RemoteRef> {
                self.0.upload_document(profile_id, path, sha256, caption)
            }
        }

        let mock = Arc::new(MockTransport::default());
        let transport = ConsumerTransport::Ready(Box::new(SharedMock(Arc::clone(&mock))));
        let stop = Arc::new(AtomicBool::new(false));
        let wake = Arc::new(WakeSignal::default());
        let open_err_signal = Arc::new((Mutex::new(false), std::sync::Condvar::new()));

        let t_stop = Arc::clone(&stop);
        let t_wake = Arc::clone(&wake);
        let t_data: Arc<Path> = Arc::from(tmp.path().to_path_buf());
        let t_queue: Arc<Path> = Arc::from(queue_path.clone());
        let hook_signal = Arc::clone(&open_err_signal);
        let handle = std::thread::spawn(move || {
            let hook = move || {
                let (lock, cv) = &*hook_signal;
                if let Ok(mut hit) = lock.lock() {
                    *hit = true;
                    cv.notify_all();
                }
            };
            run_consumer_loop_with_observer(
                "work_1".into(),
                t_data,
                t_queue,
                transport,
                t_stop,
                t_wake,
                Some(&hook),
            );
        });

        // Deterministically wait until the consumer hits VaultQueue::open -> Err.
        let (lock, cv) = &*open_err_signal;
        let open_deadline = Instant::now() + Duration::from_secs(3);
        let mut guard = lock.lock().expect("signal lock");
        while !*guard {
            let remaining = open_deadline.saturating_duration_since(Instant::now());
            assert!(
                !remaining.is_zero(),
                "timed out waiting for transient queue open failure"
            );
            let (next_guard, _) = cv.wait_timeout(guard, remaining).expect("signal wait");
            guard = next_guard;
        }
        drop(guard);

        std::fs::remove_dir(&queue_path).expect("remove blocking dir");
        let mut queue = VaultQueue::open(&queue_path).expect("open real queue");
        queue
            .enqueue("work_1", &file, &digest, 13)
            .expect("enqueue item");
        wake.notify();

        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if mock.uploads().expect("uploads").len() == 1 {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        stop.store(true, Ordering::Release);
        wake.notify();
        handle.join().expect("consumer thread join");

        let uploads = mock.uploads().expect("uploads");
        assert_eq!(
            uploads.len(),
            1,
            "transport must survive transient queue open failure"
        );
        assert_eq!(uploads[0].sha256, digest);
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
    fn pair_respects_enabled_false_for_bot_and_mtproto() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let manager = test_manager(tmp.path());
        std::fs::create_dir_all(tmp.path().join("Profiles/work_1/Zalo Received Files"))
            .expect("mkdir bot");
        let bot_cfg = VaultConfig {
            profile_id: "work_1".into(),
            telegram_session_dir: manager.session_file_for("work_1"),
            destination: Default::default(),
            enabled: false,
            transport: Transport::Bot,
            telegram_api_id: None,
            telegram_api_hash: None,
            telegram_bot_token: Some("123456:secret".into()),
            telegram_chat_id: Some(42),
            telegram_bot_username: Some("vault_bot".into()),
            telegram_bot_pair_nonce: None,
        };
        manager.config_store.save(&bot_cfg).expect("save bot");
        let bot_flow = manager.pair("work_1").expect("pair bot disabled");
        assert!(bot_flow.paired);
        assert!(!bot_flow.requires_login);
        assert!(
            !manager.engine_running("work_1"),
            "pair() must not auto-start disabled bot pairing"
        );

        let session_path = manager.session_file_for("work_2");
        std::fs::create_dir_all(session_path.parent().expect("session parent"))
            .expect("session mkdir");
        std::fs::write(&session_path, b"stub-session").expect("write stub session");
        std::fs::create_dir_all(tmp.path().join("Profiles/work_2/Zalo Received Files"))
            .expect("mkdir mtp");
        let mtp_cfg = VaultConfig {
            profile_id: "work_2".into(),
            telegram_session_dir: session_path,
            destination: Default::default(),
            enabled: false,
            transport: Transport::Mtproto,
            telegram_api_id: Some(12345),
            telegram_api_hash: Some("0123456789abcdef0123456789abcdef".into()),
            telegram_bot_token: None,
            telegram_chat_id: None,
            telegram_bot_username: None,
            telegram_bot_pair_nonce: None,
        };
        manager.config_store.save(&mtp_cfg).expect("save mtp");
        let mtp_flow = manager.pair("work_2").expect("pair mtp disabled");
        assert!(mtp_flow.paired);
        assert!(!mtp_flow.requires_login);
        assert!(
            !manager.engine_running("work_2"),
            "pair() must not auto-start disabled mtproto pairing"
        );
    }

    #[test]
    fn commit_login_code_checks_epoch_and_preserves_mtproto_enabled_state() {
        use dim_telegram_vault::transport::MockTransport;

        let tmp = tempfile::tempdir().expect("tempdir");
        let manager = test_manager(tmp.path());
        let received_1 = tmp.path().join("Profiles/work_1/Zalo Received Files");
        std::fs::create_dir_all(&received_1).expect("mkdir 1");

        // 1. Stale epoch (bumped by concurrent unpair / pair_bot) aborts without saving config.
        let epoch_1 = manager.current_pairing_epoch("work_1").expect("epoch");
        manager.bump_pairing_epoch("work_1").expect("bump epoch");
        let err = manager
            .commit_login_code(
                "work_1",
                epoch_1,
                12345,
                "hash".into(),
                received_1.clone(),
                Box::new(MockTransport::default()),
            )
            .unwrap_err();
        assert!(err.to_string().contains("cancelled"));
        assert!(
            manager.config_store.load("work_1").expect("load").is_none(),
            "stale epoch must not resurrect unpaired config"
        );

        // 2. Re-auth of a paused MTProto pairing preserves enabled = false and skips engine start.
        let disabled_mtp = VaultConfig {
            profile_id: "work_1".into(),
            telegram_session_dir: manager.session_file_for("work_1"),
            destination: Default::default(),
            enabled: false,
            transport: Transport::Mtproto,
            telegram_api_id: Some(11111),
            telegram_api_hash: Some("oldhash".into()),
            telegram_bot_token: None,
            telegram_chat_id: None,
            telegram_bot_username: None,
            telegram_bot_pair_nonce: None,
        };
        manager.config_store.save(&disabled_mtp).expect("save");
        let epoch_reauth = manager.current_pairing_epoch("work_1").expect("epoch");
        manager
            .commit_login_code(
                "work_1",
                epoch_reauth,
                99999,
                "newhash".into(),
                received_1,
                Box::new(MockTransport::default()),
            )
            .expect("commit reauth");
        let reloaded = manager
            .config_store
            .load("work_1")
            .expect("load")
            .expect("config saved");
        assert!(!reloaded.enabled);
        assert_eq!(reloaded.telegram_api_id, Some(99999));
        assert!(!manager.engine_running("work_1"));

        // 3. Fresh MTProto pairing enables sync and spawns engine.
        let received_2 = tmp.path().join("Profiles/work_2/Zalo Received Files");
        std::fs::create_dir_all(&received_2).expect("mkdir 2");
        let epoch_fresh = manager.current_pairing_epoch("work_2").expect("epoch");
        manager
            .commit_login_code(
                "work_2",
                epoch_fresh,
                22222,
                "freshhash".into(),
                received_2,
                Box::new(MockTransport::default()),
            )
            .expect("commit fresh");
        let fresh_cfg = manager
            .config_store
            .load("work_2")
            .expect("load")
            .expect("fresh config");
        assert!(fresh_cfg.enabled);
        assert!(manager.engine_running("work_2"));
        manager.shutdown_with_timeout(Duration::from_secs(2));
    }

    #[test]
    fn commit_pair_bot_rejects_deleted_profile_inside_lock() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let profile_exists = Arc::new(AtomicBool::new(true));
        let flag = Arc::clone(&profile_exists);
        let profiles_root = tmp.path().join("Profiles");
        let manager = VaultManager::new(
            tmp.path(),
            Arc::new(move |profile_id| {
                if flag.load(Ordering::Acquire) {
                    Ok(profiles_root.join(profile_id))
                } else {
                    Err(format!("profile not found: {profile_id}"))
                }
            }),
        )
        .expect("manager");

        // Pre-check succeeds while profile still exists, then profile is deleted
        // concurrently while token validation runs off-lock.
        assert!(manager.received_dir_for("deleted_prof").is_ok());
        profile_exists.store(false, Ordering::Release);

        let err = manager
            .commit_pair_bot("deleted_prof", "123456:secret", "vault_bot")
            .unwrap_err();
        assert!(err.to_string().contains("deleted_prof"));
        assert!(
            manager
                .config_store
                .load("deleted_prof")
                .expect("load")
                .is_none(),
            "deleted profile must not receive orphaned bot config"
        );
    }

    #[test]
    fn log_without_queue_db_is_empty() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let manager = test_manager(tmp.path());
        assert!(manager.log("work_1", 10).expect("log").is_empty());
    }
}
