//! Bluey account token storage.
//!
//! New installs store tokens in the local Bluey account profile so CLI,
//! daemon, and dashboard agree on the API URL and device identity. Legacy
//! keyring storage remains available as an explicit fallback.

use crate::error::{Error, Result};

const KEYRING_SERVICE: &str = "bluey_account";
const KEY_ACCESS: &str = "access_token";
const KEY_REFRESH: &str = "refresh_token";
const KEY_EMAIL: &str = "account_email";
const DEFAULT_BLUEY_API_URL: &str = "https://bluey.sh";

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

/// Account-config-backed store for terminal installs where the OS keychain can
/// block or be unavailable. It uses the same on-disk account profile that
/// `bluey login` writes, so automatic refreshes survive across CLI invocations.
#[derive(Clone)]
pub struct AccountFileStore {
    paths: cue_core::app_paths::AppPaths,
}

impl AccountFileStore {
    pub fn new(paths: cue_core::app_paths::AppPaths) -> Self {
        Self { paths }
    }

    fn load_account(&self) -> Result<Option<cue_core::AccountConfig>> {
        cue_core::load_account(&self.paths).map_err(|error| Error::TokenStore(error.to_string()))
    }

    fn save_account(&self, account: &cue_core::AccountConfig) -> Result<()> {
        cue_core::save_account(&self.paths, account)
            .map_err(|error| Error::TokenStore(error.to_string()))
    }
}

impl TokenStore for AccountFileStore {
    fn save(&self, tokens: &Tokens) -> Result<()> {
        let mut account = self.load_account()?.unwrap_or_else(|| {
            let mut account = cue_core::AccountConfig::local();
            account.provider = "bluey".to_string();
            account.api_url = default_account_api_url();
            account
        });
        account.provider = if account.provider == "local" {
            "bluey".to_string()
        } else {
            account.provider
        };
        account.user_id = tokens.email.clone();
        account.access_token = Some(tokens.access.clone());
        account.refresh_token = (!tokens.refresh.trim().is_empty()).then(|| tokens.refresh.clone());
        self.save_account(&account)
    }

    fn load(&self) -> Result<Option<Tokens>> {
        let Some(account) = self.load_account()? else {
            return Ok(None);
        };
        let Some(access) = account
            .access_token
            .filter(|token| !token.trim().is_empty())
        else {
            return Ok(None);
        };
        Ok(Some(Tokens {
            access,
            refresh: account.refresh_token.unwrap_or_default(),
            email: account.user_id,
        }))
    }

    fn clear(&self) -> Result<()> {
        let Some(mut account) = self.load_account()? else {
            return Ok(());
        };
        account.access_token = None;
        account.refresh_token = None;
        self.save_account(&account)
    }
}

fn default_account_api_url() -> String {
    std::env::var("BLUEY_API_BASE_URL")
        .or_else(|_| std::env::var("BLUEY_CLOUD_API_URL"))
        .or_else(|_| std::env::var("CUE_CLOUD_API_URL"))
        .unwrap_or_else(|_| DEFAULT_BLUEY_API_URL.to_string())
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
        let refresh = Self::entry(KEY_REFRESH)?.get_password().unwrap_or_default();
        let email = Self::entry(KEY_EMAIL)?.get_password().unwrap_or_default();
        Ok(Some(Tokens {
            access,
            refresh,
            email,
        }))
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

    #[test]
    fn account_file_store_persists_refreshed_tokens() {
        let base = std::env::temp_dir().join(format!(
            "bluey-account-file-store-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ));
        let paths = cue_core::app_paths::AppPaths {
            data_dir: base.join("data"),
            config_dir: base.join("config"),
            runtime_dir: base.join("run"),
            state_file: base.join("run/daemon-state.json"),
            account_file: base.join("config/account.json"),
            settings_file: base.join("config/settings.json"),
        };

        let mut account = cue_core::AccountConfig::local();
        account.provider = "bluey".into();
        account.api_url = "https://bluey.sh".into();
        account.user_id = "old@example.com".into();
        account.access_token = Some("old-access".into());
        account.refresh_token = Some("old-refresh".into());
        cue_core::save_account(&paths, &account).unwrap();

        let store = AccountFileStore::new(paths.clone());
        store
            .save(&Tokens {
                access: "new-access".into(),
                refresh: "new-refresh".into(),
                email: "new@example.com".into(),
            })
            .unwrap();

        let loaded = cue_core::load_account(&paths).unwrap().unwrap();
        assert_eq!(loaded.provider, "bluey");
        assert_eq!(loaded.api_url, "https://bluey.sh");
        assert_eq!(loaded.user_id, "new@example.com");
        assert_eq!(loaded.access_token.as_deref(), Some("new-access"));
        assert_eq!(loaded.refresh_token.as_deref(), Some("new-refresh"));

        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn account_file_store_save_seeds_api_url_from_environment() {
        let base = std::env::temp_dir().join(format!(
            "bluey-account-file-env-url-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ));
        let paths = cue_core::app_paths::AppPaths {
            data_dir: base.join("data"),
            config_dir: base.join("config"),
            runtime_dir: base.join("run"),
            state_file: base.join("run/daemon-state.json"),
            account_file: base.join("config/account.json"),
            settings_file: base.join("config/settings.json"),
        };

        std::env::set_var("BLUEY_API_BASE_URL", "http://127.0.0.1:8787");
        let store = AccountFileStore::new(paths.clone());
        store
            .save(&Tokens {
                access: "access".into(),
                refresh: "refresh".into(),
                email: "local@example.com".into(),
            })
            .unwrap();
        std::env::remove_var("BLUEY_API_BASE_URL");

        let loaded = cue_core::load_account(&paths).unwrap().unwrap();
        assert_eq!(loaded.provider, "bluey");
        assert_eq!(loaded.api_url, "http://127.0.0.1:8787");
        assert_eq!(loaded.user_id, "local@example.com");
        assert_eq!(loaded.access_token.as_deref(), Some("access"));
        assert_eq!(loaded.refresh_token.as_deref(), Some("refresh"));

        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn account_file_store_clear_preserves_account_profile() {
        let base = std::env::temp_dir().join(format!(
            "bluey-account-file-clear-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ));
        let paths = cue_core::app_paths::AppPaths {
            data_dir: base.join("data"),
            config_dir: base.join("config"),
            runtime_dir: base.join("run"),
            state_file: base.join("run/daemon-state.json"),
            account_file: base.join("config/account.json"),
            settings_file: base.join("config/settings.json"),
        };

        let mut account = cue_core::AccountConfig::local();
        account.provider = "bluey".into();
        account.api_url = "https://staging.bluey.sh".into();
        account.user_id = "user@example.com".into();
        account.workspace_id = "workspace-1".into();
        account.device_id = "device-1".into();
        account.access_token = Some("access".into());
        account.refresh_token = Some("refresh".into());
        cue_core::save_account(&paths, &account).unwrap();

        let store = AccountFileStore::new(paths.clone());
        store.clear().unwrap();

        let loaded = cue_core::load_account(&paths).unwrap().unwrap();
        assert_eq!(loaded.provider, "bluey");
        assert_eq!(loaded.api_url, "https://staging.bluey.sh");
        assert_eq!(loaded.user_id, "user@example.com");
        assert_eq!(loaded.workspace_id, "workspace-1");
        assert_eq!(loaded.device_id, "device-1");
        assert!(loaded.access_token.is_none());
        assert!(loaded.refresh_token.is_none());

        let _ = std::fs::remove_dir_all(base);
    }
}
