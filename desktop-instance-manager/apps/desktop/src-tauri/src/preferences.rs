//! Persistent desktop-shell preferences for KHOANG (`khoang-preferences.json`).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use dim_core::{write_bytes_atomic, DimError};
use serde::{Deserialize, Serialize};

pub const PREFERENCES_FILE_NAME: &str = "khoang-preferences.json";
const DEFAULT_LOCALE: &str = "vi-VN";

/// User-configured desktop shell preferences persisted under `<data_dir>/khoang-preferences.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct DesktopPreferences {
    /// Whether closing the main window hides to the system tray instead of exiting KHOANG.
    pub keep_in_tray: bool,
    /// Active UI locale (`"vi-VN"` or `"en-US"`) used for localized tray labels and tooltips.
    pub locale: String,
    /// Account IDs pinned to the top of the workspace rail (stored only in `khoang-preferences.json`).
    #[serde(default)]
    pub pinned_accounts: Vec<String>,
}

impl Default for DesktopPreferences {
    fn default() -> Self {
        Self {
            keep_in_tray: false,
            locale: DEFAULT_LOCALE.to_owned(),
            pinned_accounts: Vec::new(),
        }
    }
}

/// Thread-safe, Tauri-managed desktop preferences state.
#[derive(Clone)]
pub struct DesktopPrefsState {
    data_dir: PathBuf,
    inner: Arc<RwLock<DesktopPreferences>>,
}

impl DesktopPrefsState {
    /// Initializes desktop preferences from `<data_dir>/khoang-preferences.json` (defaults if absent).
    pub fn initialize(data_dir: &Path) -> Self {
        let prefs = load_preferences(data_dir);
        Self {
            data_dir: data_dir.to_path_buf(),
            inner: Arc::new(RwLock::new(prefs)),
        }
    }

    /// Returns a cloned snapshot of current preferences.
    pub fn snapshot(&self) -> DesktopPreferences {
        self.inner
            .read()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    /// Fast synchronous read for `WindowEvent::CloseRequested`.
    pub fn keep_in_tray(&self) -> bool {
        self.inner
            .read()
            .map(|guard| guard.keep_in_tray)
            .unwrap_or(false)
    }

    /// Updates `keep_in_tray` and persists to disk.
    pub fn set_keep_in_tray(&self, enabled: bool) -> Result<DesktopPreferences, DimError> {
        self.update(|prefs| {
            prefs.keep_in_tray = enabled;
        })
    }

    /// Updates `locale` (normalized to `"vi-VN"` or `"en-US"`) and persists to disk.
    pub fn set_locale(&self, locale: &str) -> Result<DesktopPreferences, DimError> {
        let normalized = normalize_locale(locale).to_owned();
        self.update(|prefs| {
            prefs.locale = normalized;
        })
    }

    /// Pins or unpins an account ID in `khoang-preferences.json`.
    pub fn set_account_pinned(
        &self,
        id: &str,
        pinned: bool,
    ) -> Result<DesktopPreferences, DimError> {
        let trimmed = id.trim();
        if trimmed.is_empty() {
            return Err(DimError::InvalidProfileName(
                "account id cannot be empty".to_owned(),
            ));
        }
        let owned = trimmed.to_owned();
        self.update(|prefs| {
            if pinned {
                if !prefs
                    .pinned_accounts
                    .iter()
                    .any(|existing| existing == &owned)
                {
                    prefs.pinned_accounts.push(owned);
                }
            } else {
                prefs.pinned_accounts.retain(|existing| existing != &owned);
            }
        })
    }

    /// Returns `true` if `id` is currently pinned.
    #[cfg(test)]
    pub fn is_pinned(&self, id: &str) -> bool {
        self.inner
            .read()
            .map(|guard| guard.pinned_accounts.iter().any(|x| x == id))
            .unwrap_or(false)
    }

    fn update(
        &self,
        mutate: impl FnOnce(&mut DesktopPreferences),
    ) -> Result<DesktopPreferences, DimError> {
        let mut guard = self
            .inner
            .write()
            .map_err(|_| DimError::Io(std::io::Error::other("preferences lock poisoned")))?;
        let mut next = guard.clone();
        mutate(&mut next);
        save_preferences(&self.data_dir, &next)?;
        *guard = next.clone();
        Ok(next)
    }
}

/// Normalizes a locale tag to `"vi-VN"` or `"en-US"`.
pub fn normalize_locale(raw: &str) -> &'static str {
    let trimmed = raw.trim().to_ascii_lowercase();
    if trimmed.starts_with("en") {
        "en-US"
    } else {
        "vi-VN"
    }
}

/// Loads preferences from `<data_dir>/khoang-preferences.json`, falling back to defaults on missing/corrupt files.
pub fn load_preferences(data_dir: &Path) -> DesktopPreferences {
    let path = data_dir.join(PREFERENCES_FILE_NAME);
    let Ok(raw) = fs::read_to_string(&path) else {
        return DesktopPreferences::default();
    };
    let mut parsed: DesktopPreferences = serde_json::from_str(&raw).unwrap_or_default();
    parsed.locale = normalize_locale(&parsed.locale).to_owned();
    parsed
}

/// Atomically persists preferences to `<data_dir>/khoang-preferences.json`.
pub fn save_preferences(data_dir: &Path, prefs: &DesktopPreferences) -> Result<(), DimError> {
    fs::create_dir_all(data_dir)?;
    let target = data_dir.join(PREFERENCES_FILE_NAME);
    let payload = serde_json::to_string_pretty(prefs)?;
    write_bytes_atomic(&target, payload.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_prefs_dir(suffix: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("khoang-prefs-test-{suffix}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn defaults_keep_in_tray_off_and_roundtrips_updates() {
        let dir = temp_prefs_dir("roundtrip");
        let state = DesktopPrefsState::initialize(&dir);
        assert!(!state.keep_in_tray());
        assert_eq!(state.snapshot().locale, "vi-VN");

        let updated = state.set_keep_in_tray(true).expect("save keep_in_tray");
        assert!(updated.keep_in_tray);
        assert!(state.keep_in_tray());

        let updated = state.set_locale("en-US").expect("save locale");
        assert_eq!(updated.locale, "en-US");

        let reloaded = DesktopPrefsState::initialize(&dir);
        assert!(reloaded.keep_in_tray());
        assert_eq!(reloaded.snapshot().locale, "en-US");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn pinned_accounts_persist_deduplicate_and_unpin() {
        let dir = temp_prefs_dir("pinned");
        let state = DesktopPrefsState::initialize(&dir);
        assert!(state.snapshot().pinned_accounts.is_empty());

        state
            .set_account_pinned("primary", true)
            .expect("pin primary");
        state
            .set_account_pinned("primary", true)
            .expect("dedupe pin");
        state
            .set_account_pinned("abc12345", true)
            .expect("pin secondary");
        assert_eq!(
            state.snapshot().pinned_accounts,
            vec!["primary".to_owned(), "abc12345".to_owned()]
        );
        assert!(state.is_pinned("primary"));

        state
            .set_account_pinned("primary", false)
            .expect("unpin primary");
        let reloaded = DesktopPrefsState::initialize(&dir);
        assert_eq!(
            reloaded.snapshot().pinned_accounts,
            vec!["abc12345".to_owned()]
        );
        assert!(!reloaded.is_pinned("primary"));

        let _ = fs::remove_dir_all(&dir);
    }
}
