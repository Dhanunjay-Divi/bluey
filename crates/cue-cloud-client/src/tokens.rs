//! OS keyring–backed token storage.
//!
//! Tokens are stored under the keyring service `bluey_account` with
//! username keys `access_token` / `refresh_token` / `account_email`.
//! This deliberately uses a different keyring service than the dev-mode
//! BYOK keys (`llm_*`) so the namespaces stay distinct.

use crate::error::{Error, Result};

const KEYRING_SERVICE: &str = "bluey_account";
const KEY_ACCESS: &str = "access_token";
const KEY_REFRESH: &str = "refresh_token";
const KEY_EMAIL: &str = "account_email";

#[derive(Debug, Clone)]
pub struct Tokens {
    pub access: String,
    pub refresh: String,
    pub email: String,
}

/// Trait so tests can inject an in-memory store; production uses the
/// `KeyringStore` impl below.
pub trait TokenStore: Send + Sync {
    fn save(&self, tokens: &Tokens) -> Result<()>;
    fn load(&self) -> Result<Option<Tokens>>;
    fn clear(&self) -> Result<()>;
}

/// Production keyring-backed store.
#[derive(Default)]
pub struct KeyringStore;

impl KeyringStore {
    pub fn new() -> Self {
        Self
    }

    fn entry(name: &str) -> Result<keyring::Entry> {
        Ok(keyring::Entry::new(KEYRING_SERVICE, name)?)
    }
}

impl TokenStore for KeyringStore {
    fn save(&self, tokens: &Tokens) -> Result<()> {
        Self::entry(KEY_ACCESS)?.set_password(&tokens.access)?;
        Self::entry(KEY_REFRESH)?.set_password(&tokens.refresh)?;
        Self::entry(KEY_EMAIL)?.set_password(&tokens.email)?;
        Ok(())
    }

    fn load(&self) -> Result<Option<Tokens>> {
        let access = match Self::entry(KEY_ACCESS)?.get_password() {
            Ok(s) => s,
            Err(keyring::Error::NoEntry) => return Ok(None),
            Err(e) => return Err(Error::TokenStore(e.to_string())),
        };
        let refresh = Self::entry(KEY_REFRESH)?
            .get_password()
            .unwrap_or_default();
        let email = Self::entry(KEY_EMAIL)?.get_password().unwrap_or_default();
        Ok(Some(Tokens { access, refresh, email }))
    }

    fn clear(&self) -> Result<()> {
        for key in [KEY_ACCESS, KEY_REFRESH, KEY_EMAIL] {
            // Best-effort: ignore NoEntry on delete.
            if let Ok(entry) = Self::entry(key) {
                let _ = entry.delete_password();
            }
        }
        Ok(())
    }
}

/// In-memory implementation for tests + headless CI.
#[derive(Default)]
pub struct MemoryStore {
    inner: std::sync::Mutex<Option<Tokens>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl TokenStore for MemoryStore {
    fn save(&self, tokens: &Tokens) -> Result<()> {
        *self.inner.lock().unwrap() = Some(tokens.clone());
        Ok(())
    }
    fn load(&self) -> Result<Option<Tokens>> {
        Ok(self.inner.lock().unwrap().clone())
    }
    fn clear(&self) -> Result<()> {
        *self.inner.lock().unwrap() = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_store_round_trips() {
        let store = MemoryStore::new();
        assert!(store.load().unwrap().is_none());
        store
            .save(&Tokens {
                access: "a".into(),
                refresh: "r".into(),
                email: "e@example.com".into(),
            })
            .unwrap();
        let loaded = store.load().unwrap().unwrap();
        assert_eq!(loaded.access, "a");
        assert_eq!(loaded.refresh, "r");
        assert_eq!(loaded.email, "e@example.com");
        store.clear().unwrap();
        assert!(store.load().unwrap().is_none());
    }
}
