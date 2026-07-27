//! Keyring-backed calendar-token storage.
//!
//! Each provider stores one versioned JSON bundle in the OS credential vault.
//! Keeping the four token fields in one entry makes save/load atomic and avoids
//! four separate macOS Keychain authorization prompts. Older split keychain
//! entries and the short-lived plaintext development fallback are migrated once
//! and removed after the secure bundle is written.

use std::fmt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

const KEY_TOKEN_BUNDLE: &str = "oauth_tokens_v1";
const LEGACY_KEY_ACCESS: &str = "access_token";
const LEGACY_KEY_REFRESH: &str = "refresh_token";
const LEGACY_KEY_EXPIRES_AT: &str = "expires_at_epoch";
const LEGACY_KEY_EMAIL: &str = "account_email";

fn aggregate_cleanup_errors(scope: &str, errors: Vec<String>) -> Result<()> {
    if errors.is_empty() {
        Ok(())
    } else {
        Err(anyhow!("{scope}: {}", errors.join("; ")))
    }
}

fn attempt_all_cleanup<'a>(
    scope: &str,
    targets: impl IntoIterator<Item = &'a str>,
    mut cleanup: impl FnMut(&str) -> Result<()>,
) -> Result<()> {
    let mut errors = Vec::new();
    for target in targets {
        if let Err(error) = cleanup(target) {
            errors.push(format!("{target}: {error:#}"));
        }
    }
    aggregate_cleanup_errors(scope, errors)
}

fn clear_primary_after_legacy(
    clear_legacy: impl FnOnce() -> Result<()>,
    clear_primary: impl FnOnce() -> Result<()>,
) -> Result<()> {
    clear_legacy()?;
    clear_primary()
}

/// The default clock skew (seconds) treated as "already expired" so we refresh
/// slightly ahead of the real expiry rather than mid-request.
pub const DEFAULT_EXPIRY_SKEW_SECS: u64 = 60;

/// Calendar OAuth tokens for one provider connection.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalTokens {
    /// Bearer access token used on calendar API calls.
    pub access: String,
    /// Long-lived refresh token used to mint new access tokens.
    pub refresh: String,
    /// Absolute expiry of `access` (epoch seconds).
    pub expires_at_epoch: u64,
    /// The connected account's email (for the UI label; best-effort).
    pub email: String,
}

/// Never expose bearer or refresh token material through diagnostics.
impl fmt::Debug for CalTokens {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CalTokens")
            .field("access", &"<redacted>")
            .field("refresh", &"<redacted>")
            .field("expires_at_epoch", &self.expires_at_epoch)
            .field("email", &self.email)
            .finish()
    }
}

/// Read-only compatibility shim for the plaintext store briefly shipped by the
/// development branch. New code never writes this file. A successful migration
/// deletes it immediately.
struct LegacyFileCalStore {
    path: PathBuf,
}

impl LegacyFileCalStore {
    fn for_service(service: &str) -> Option<Self> {
        let home = std::env::var_os("HOME")?;
        Some(Self {
            path: PathBuf::from(home)
                .join(".config")
                .join("bluey")
                .join("tokens")
                .join(format!("{service}.json")),
        })
    }

    #[cfg(test)]
    fn from_path(path: PathBuf) -> Self {
        Self { path }
    }

    fn load(&self) -> Result<Option<CalTokens>> {
        if !self.path.exists() {
            return Ok(None);
        }
        let data = std::fs::read_to_string(&self.path)
            .with_context(|| format!("read legacy calendar token file {}", self.path.display()))?;
        let tokens = serde_json::from_str(&data)
            .with_context(|| format!("parse legacy calendar token file {}", self.path.display()))?;
        Ok(Some(tokens))
    }

    fn clear(&self) -> Result<()> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error).with_context(|| {
                format!("remove legacy calendar token file {}", self.path.display())
            }),
        }
    }
}

/// True when `tokens.access` is expired (or within `skew_secs` of expiry).
pub fn is_expired(tokens: &CalTokens, now_epoch: u64, skew_secs: u64) -> bool {
    tokens.expires_at_epoch <= now_epoch.saturating_add(skew_secs)
}

/// Store abstraction so tests can inject an in-memory store; production uses
/// the [`KeyringCalStore`].
pub trait CalTokenStore: Send + Sync {
    fn save(&self, tokens: &CalTokens) -> Result<()>;
    fn load(&self) -> Result<Option<CalTokens>>;
    fn clear(&self) -> Result<()>;
}

/// Production OS-keyring-backed store.
pub struct KeyringCalStore {
    /// The per-provider keyring service (from `Provider::keyring_service()`).
    service: &'static str,
}

impl KeyringCalStore {
    pub fn new(service: &'static str) -> Self {
        Self { service }
    }

    fn entry(&self, name: &str) -> Result<keyring::Entry> {
        keyring::Entry::new(self.service, name)
            .with_context(|| format!("open calendar keychain entry {name}"))
    }

    fn read_entry(&self, name: &str) -> Result<Option<String>> {
        match self.entry(name)?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => {
                Err(error).with_context(|| format!("read calendar keychain entry {name}"))
            }
        }
    }

    fn delete_entry(&self, name: &str) -> Result<()> {
        match self.entry(name)?.delete_password() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => {
                Err(error).with_context(|| format!("delete calendar keychain entry {name}"))
            }
        }
    }

    fn clear_legacy_keychain_entries(&self) -> Result<()> {
        attempt_all_cleanup(
            "clear legacy calendar keychain entries",
            [
                LEGACY_KEY_ACCESS,
                LEGACY_KEY_REFRESH,
                LEGACY_KEY_EXPIRES_AT,
                LEGACY_KEY_EMAIL,
            ],
            |key| self.delete_entry(key),
        )
    }

    fn migrate_legacy_keychain(&self) -> Result<Option<CalTokens>> {
        let Some(access) = self.read_entry(LEGACY_KEY_ACCESS)? else {
            return Ok(None);
        };
        let refresh = self.read_entry(LEGACY_KEY_REFRESH)?.unwrap_or_default();
        let expires_at_epoch = self
            .read_entry(LEGACY_KEY_EXPIRES_AT)?
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0);
        let email = self.read_entry(LEGACY_KEY_EMAIL)?.unwrap_or_default();
        Ok(Some(CalTokens {
            access,
            refresh,
            expires_at_epoch,
            email,
        }))
    }

    fn legacy_file(&self) -> Option<LegacyFileCalStore> {
        LegacyFileCalStore::for_service(self.service)
    }

    /// Remove every obsolete credential location even when one deletion fails.
    /// The aggregate error contains entry names/paths only, never token values.
    fn clear_legacy_locations(&self) -> Result<()> {
        let mut errors = Vec::new();
        if let Err(error) = self.clear_legacy_keychain_entries() {
            errors.push(format!("keychain: {error:#}"));
        }
        if let Some(file) = self.legacy_file() {
            if let Err(error) = file.clear() {
                errors.push(format!("plaintext file: {error:#}"));
            }
        }
        aggregate_cleanup_errors("clear legacy calendar credentials", errors)
    }
}

impl CalTokenStore for KeyringCalStore {
    fn save(&self, tokens: &CalTokens) -> Result<()> {
        let encoded = serde_json::to_string(tokens).context("serialize calendar token bundle")?;
        self.entry(KEY_TOKEN_BUNDLE)?
            .set_password(&encoded)
            .context("save calendar token bundle to OS keychain")?;

        // Retry every legacy cleanup after every secure save. A prior save may
        // have written the bundle successfully but failed to remove one legacy
        // entry or the plaintext development file.
        self.clear_legacy_locations()
    }

    fn load(&self) -> Result<Option<CalTokens>> {
        if let Some(encoded) = self.read_entry(KEY_TOKEN_BUNDLE)? {
            let tokens = serde_json::from_str(&encoded)
                .context("parse calendar token bundle from keychain")?;
            // A secure bundle is not proof that an earlier plaintext cleanup
            // succeeded. Retry on every fresh store load and surface failure.
            self.clear_legacy_locations()?;
            return Ok(Some(tokens));
        }

        // Migrate the plaintext development fallback first because it can be
        // removed only after the secure write succeeds.
        if let Some(file) = self.legacy_file() {
            if let Some(tokens) = file.load()? {
                self.save(&tokens)?;
                return Ok(Some(tokens));
            }
        }

        // Then migrate the original four-entry keychain layout.
        if let Some(tokens) = self.migrate_legacy_keychain()? {
            self.save(&tokens)?;
            return Ok(Some(tokens));
        }
        Ok(None)
    }

    fn clear(&self) -> Result<()> {
        // Clear every legacy location first. If any deletion fails, retain the
        // authoritative secure bundle and report disconnect failure; deleting
        // it first would let a surviving legacy token be migrated back on the
        // next load, apparently resurrecting the disconnected account.
        clear_primary_after_legacy(
            || self.clear_legacy_locations(),
            || self.delete_entry(KEY_TOKEN_BUNDLE),
        )
    }
}

/// Process-local read-through cache around a secure store.
///
/// Calendar pollers ask for a token every refresh tick. Reading the OS keychain
/// on every tick can repeatedly prompt on macOS development builds, so a source
/// loads once and keeps the current bundle in memory. Refresh-token rotation
/// still writes through to the secure backend before updating the cache.
pub struct CachedCalStore {
    backend: Arc<dyn CalTokenStore>,
    cached: Mutex<Option<CalTokens>>,
}

impl CachedCalStore {
    pub fn load_from(backend: Arc<dyn CalTokenStore>) -> Result<Self> {
        let cached = backend.load()?;
        Ok(Self {
            backend,
            cached: Mutex::new(cached),
        })
    }

    pub fn with_tokens(backend: Arc<dyn CalTokenStore>, tokens: CalTokens) -> Self {
        Self {
            backend,
            cached: Mutex::new(Some(tokens)),
        }
    }

    pub fn connected(&self) -> Result<bool> {
        Ok(self
            .cached
            .lock()
            .map_err(|_| anyhow::anyhow!("CachedCalStore mutex poisoned"))?
            .is_some())
    }
}

impl CalTokenStore for CachedCalStore {
    fn save(&self, tokens: &CalTokens) -> Result<()> {
        self.backend.save(tokens)?;
        *self
            .cached
            .lock()
            .map_err(|_| anyhow::anyhow!("CachedCalStore mutex poisoned"))? = Some(tokens.clone());
        Ok(())
    }

    fn load(&self) -> Result<Option<CalTokens>> {
        Ok(self
            .cached
            .lock()
            .map_err(|_| anyhow::anyhow!("CachedCalStore mutex poisoned"))?
            .clone())
    }

    fn clear(&self) -> Result<()> {
        self.backend.clear()?;
        *self
            .cached
            .lock()
            .map_err(|_| anyhow::anyhow!("CachedCalStore mutex poisoned"))? = None;
        Ok(())
    }
}

/// In-memory implementation for tests + headless CI.
#[derive(Default)]
pub struct MemoryCalStore {
    inner: std::sync::Mutex<Option<CalTokens>>,
}

impl MemoryCalStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl CalTokenStore for MemoryCalStore {
    fn save(&self, tokens: &CalTokens) -> Result<()> {
        *self
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("MemoryCalStore mutex poisoned"))? = Some(tokens.clone());
        Ok(())
    }

    fn load(&self) -> Result<Option<CalTokens>> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("MemoryCalStore mutex poisoned"))?
            .clone())
    }

    fn clear(&self) -> Result<()> {
        *self
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("MemoryCalStore mutex poisoned"))? = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> CalTokens {
        CalTokens {
            access: "access-abc".into(),
            refresh: "refresh-xyz".into(),
            expires_at_epoch: 1_800_000_000,
            email: "person@example.com".into(),
        }
    }

    #[test]
    fn memory_store_round_trips() {
        let store = MemoryCalStore::new();
        assert!(store.load().unwrap().is_none());
        store.save(&sample()).unwrap();
        let loaded = store.load().unwrap().unwrap();
        assert_eq!(loaded, sample());
        store.clear().unwrap();
        assert!(store.load().unwrap().is_none());
    }

    #[test]
    fn cached_store_writes_through_and_serves_memory() {
        let backend = Arc::new(MemoryCalStore::new());
        let backend_trait: Arc<dyn CalTokenStore> = backend.clone();
        let cache = CachedCalStore::load_from(backend_trait).unwrap();
        assert!(!cache.connected().unwrap());

        cache.save(&sample()).unwrap();
        assert_eq!(cache.load().unwrap(), Some(sample()));
        assert_eq!(backend.load().unwrap(), Some(sample()));

        cache.clear().unwrap();
        assert!(cache.load().unwrap().is_none());
        assert!(backend.load().unwrap().is_none());
    }

    #[test]
    fn token_debug_output_is_redacted() {
        let output = format!("{:?}", sample());
        assert!(!output.contains("access-abc"));
        assert!(!output.contains("refresh-xyz"));
        assert!(output.contains("<redacted>"));
    }

    #[test]
    fn legacy_file_is_read_only_and_removable() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("calendar.json");
        std::fs::write(&path, serde_json::to_vec(&sample()).unwrap()).unwrap();
        let store = LegacyFileCalStore::from_path(path.clone());

        assert_eq!(store.load().unwrap(), Some(sample()));
        store.clear().unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn cleanup_attempts_every_target_and_aggregates_failures() {
        let mut attempted = Vec::new();
        let error = attempt_all_cleanup(
            "test cleanup",
            ["access", "refresh", "expiry", "email"],
            |target| {
                attempted.push(target.to_owned());
                if target == "access" || target == "expiry" {
                    Err(anyhow!("blocked"))
                } else {
                    Ok(())
                }
            },
        )
        .expect_err("two targets should fail");

        assert_eq!(attempted, ["access", "refresh", "expiry", "email"]);
        let message = format!("{error:#}");
        assert!(message.contains("access: blocked"));
        assert!(message.contains("expiry: blocked"));
        assert!(!message.contains("refresh:"));
        assert!(!message.contains("email:"));
    }

    #[test]
    fn failed_legacy_cleanup_retains_primary_bundle() {
        let primary_deleted = std::cell::Cell::new(false);
        let error = clear_primary_after_legacy(
            || Err(anyhow!("legacy entry blocked")),
            || {
                primary_deleted.set(true);
                Ok(())
            },
        )
        .expect_err("legacy cleanup should fail disconnect");

        assert!(!primary_deleted.get());
        assert!(format!("{error:#}").contains("legacy entry blocked"));
    }

    #[test]
    fn is_expired_respects_skew() {
        let mut tokens = sample();
        tokens.expires_at_epoch = 1000;
        assert!(!is_expired(&tokens, 800, 60));
        assert!(is_expired(&tokens, 950, 60));
        assert!(is_expired(&tokens, 1000, 0));
    }

    #[test]
    fn is_expired_saturates_on_overflow() {
        let mut tokens = sample();
        tokens.expires_at_epoch = u64::MAX;
        assert!(is_expired(&tokens, u64::MAX - 1, u64::MAX));
        assert!(!is_expired(&tokens, 1_000, 60));
    }
}
