//! Keyring-backed calendar-token storage.
//!
//! Mirrors the shape of `cue-cloud-client/src/tokens.rs` (`TokenStore` trait +
//! `KeyringStore` + `MemoryStore`), but with a PER-PROVIDER keyring service
//! (`bluey_calendar_google` / `bluey_calendar_microsoft`) so a Google and a
//! Microsoft connection never collide, and with an added token expiry so the
//! flow can refresh proactively.

use anyhow::Result;

const KEY_ACCESS: &str = "access_token";
const KEY_REFRESH: &str = "refresh_token";
const KEY_EXPIRES_AT: &str = "expires_at_epoch";
const KEY_EMAIL: &str = "account_email";

/// The default clock skew (seconds) treated as "already expired" so we refresh
/// slightly ahead of the real expiry rather than mid-request.
pub const DEFAULT_EXPIRY_SKEW_SECS: u64 = 60;

use serde::{Deserialize, Serialize};

/// Calendar OAuth tokens for one provider connection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

/// Secure file-backed token store fallback (`~/.config/bluey/tokens/`) to bypass macOS Keychain prompt loops during dev.
pub struct FileCalStore {
    path: std::path::PathBuf,
}

impl FileCalStore {
    pub fn new(service: &'static str) -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
        let dir = std::path::PathBuf::from(home)
            .join(".config")
            .join("bluey")
            .join("tokens");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(format!("{service}.json"));
        Self { path }
    }
}

impl CalTokenStore for FileCalStore {
    fn save(&self, tokens: &CalTokens) -> Result<()> {
        let json = serde_json::to_string_pretty(tokens)?;
        std::fs::write(&self.path, json)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }

    fn load(&self) -> Result<Option<CalTokens>> {
        if !self.path.exists() {
            return Ok(None);
        }
        let data = std::fs::read_to_string(&self.path)?;
        let tokens: CalTokens = serde_json::from_str(&data)?;
        Ok(Some(tokens))
    }

    fn clear(&self) -> Result<()> {
        if self.path.exists() {
            let _ = std::fs::remove_file(&self.path);
        }
        Ok(())
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
    /// Returns true if tokens exist in the FILE store only — never touches the
    /// OS keychain, so it is safe to call on every poll tick without triggering
    /// macOS Keychain authorization prompts.
    fn file_connected(&self) -> bool {
        false // overridden by KeyringCalStore
    }
}

/// Production OS-keyring-backed store with local file fallback to eliminate macOS Keychain prompts in dev.
pub struct KeyringCalStore {
    /// The per-provider keyring service (from `Provider::keyring_service()`).
    service: &'static str,
}

impl KeyringCalStore {
    pub fn new(service: &'static str) -> Self {
        Self { service }
    }

    fn entry(&self, name: &str) -> Result<keyring::Entry> {
        Ok(keyring::Entry::new(self.service, name)?)
    }
}

impl CalTokenStore for KeyringCalStore {
    fn save(&self, tokens: &CalTokens) -> Result<()> {
        // File store is the PRIMARY read path — always write it first so that
        // subsequent load() calls never need to touch the OS keychain.
        let file_store = FileCalStore::new(self.service);
        if let Err(e) = file_store.save(tokens) {
            // Log but don't fail — keychain is the fallback.
            tracing::warn!(error = %e, "calendar token file save failed; falling back to keychain-only");
        }

        // Keychain is a secondary backup (useful on a fresh machine before the
        // file exists). Errors are best-effort — the file write above is the
        // authoritative store.
        let _ = self.entry(KEY_ACCESS).and_then(|e| Ok(e.set_password(&tokens.access)?));
        let _ = self.entry(KEY_REFRESH).and_then(|e| Ok(e.set_password(&tokens.refresh)?));
        let _ = self.entry(KEY_EXPIRES_AT).and_then(|e| Ok(e.set_password(&tokens.expires_at_epoch.to_string())?));
        let _ = self.entry(KEY_EMAIL).and_then(|e| Ok(e.set_password(&tokens.email)?));
        Ok(())
    }

    fn load(&self) -> Result<Option<CalTokens>> {
        let file_store = FileCalStore::new(self.service);

        // Fast path: file exists → return immediately, zero keychain reads.
        if let Ok(Some(tokens)) = file_store.load() {
            return Ok(Some(tokens));
        }

        // Slow path (first run / file deleted): read from keychain, then
        // immediately write back to the file so future calls never reach here.
        let access = match self.entry(KEY_ACCESS)?.get_password() {
            Ok(s) => s,
            Err(keyring::Error::NoEntry) => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        let refresh = self.entry(KEY_REFRESH)?.get_password().unwrap_or_default();
        let expires_at_epoch = self
            .entry(KEY_EXPIRES_AT)?
            .get_password()
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        let email = self.entry(KEY_EMAIL)?.get_password().unwrap_or_default();
        let tokens = CalTokens { access, refresh, expires_at_epoch, email };

        // Write to file immediately so the next load() uses the fast path.
        let _ = file_store.save(&tokens);

        Ok(Some(tokens))
    }

    fn clear(&self) -> Result<()> {
        let file_store = FileCalStore::new(self.service);
        let _ = file_store.clear();
        for key in [KEY_ACCESS, KEY_REFRESH, KEY_EXPIRES_AT, KEY_EMAIL] {
            // Best-effort: ignore NoEntry on delete.
            if let Ok(entry) = self.entry(key) {
                let _ = entry.delete_password();
            }
        }
        Ok(())
    }

    /// Check connection status from the file store only — never touches the
    /// OS keychain, so calling this on every daemon poll tick is safe.
    fn file_connected(&self) -> bool {
        FileCalStore::new(self.service).load().ok().flatten().is_some()
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
    fn is_expired_respects_skew() {
        let mut t = sample();
        t.expires_at_epoch = 1000;
        // now=800, skew=60 → threshold 860 < 1000 → not expired.
        assert!(!is_expired(&t, 800, 60));
        // now=950, skew=60 → threshold 1010 >= 1000 → expired (proactive).
        assert!(is_expired(&t, 950, 60));
        // Exactly at expiry counts as expired.
        assert!(is_expired(&t, 1000, 0));
    }

    #[test]
    fn is_expired_saturates_on_overflow() {
        let mut t = sample();
        t.expires_at_epoch = u64::MAX;
        // A huge skew must not panic: `now + skew` saturates to u64::MAX, which
        // equals the expiry, so this reports expired (fail-safe: we'd refresh)
        // rather than overflow-panicking.
        assert!(is_expired(&t, u64::MAX - 1, u64::MAX));
        // With a modest skew and MAX expiry, it is comfortably not expired.
        assert!(!is_expired(&t, 1_000, 60));
    }
}
