//! Bluey account token storage.
//!
//! New installs store account tokens in OS secure storage. The local Bluey
//! account profile keeps non-secret metadata such as API URL, workspace, and
//! device identity. Account-file token storage is retained only as an explicit
//! development fallback and as a one-time migration path for older installs.

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

    fn save_profile_without_tokens(&self, tokens: &Tokens) -> Result<()> {
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
        account.access_token = None;
        account.refresh_token = None;
        self.save_account(&account)
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

/// Default desktop token store.
///
/// Tokens are stored in the OS credential store (macOS Keychain, Windows
/// Credential Manager, or libsecret-compatible stores on Linux). If an older
/// install still has tokens in `account.json`, the first successful load
/// migrates them into the secure store and clears them from disk.
pub struct SecureAccountStore {
    account_file: AccountFileStore,
    keyring: KeyringStore,
}

impl SecureAccountStore {
    pub fn new(paths: cue_core::app_paths::AppPaths) -> Self {
        Self {
            account_file: AccountFileStore::new(paths),
            keyring: KeyringStore::new(),
        }
    }
}

impl TokenStore for SecureAccountStore {
    fn save(&self, tokens: &Tokens) -> Result<()> {
        self.account_file.save_profile_without_tokens(tokens)?;
        if plaintext_token_fallback_enabled() {
            tracing::warn!(
                "using plaintext account-token fallback because BLUEY_ALLOW_PLAINTEXT_TOKENS is enabled"
            );
            self.account_file.save(tokens)?;
            return Ok(());
        }
        match self.keyring.save(tokens) {
            Ok(()) => {
                self.account_file.clear()?;
                Ok(())
            }
            Err(error) => Err(Error::TokenStore(format!(
                "secure token storage unavailable: {error}. Bluey will not write account tokens to disk; fix OS credential storage or set BLUEY_ALLOW_PLAINTEXT_TOKENS=1 for local development only."
            ))),
        }
    }

    fn load(&self) -> Result<Option<Tokens>> {
        if plaintext_token_fallback_enabled() {
            return self.account_file.load();
        }

        let keyring_error = match self.keyring.load() {
            Ok(Some(tokens)) => return Ok(Some(tokens)),
            Ok(None) => None,
            Err(error) => Some(error),
        };

        let Some(tokens) = self.account_file.load()? else {
            if let Some(error) = keyring_error {
                tracing::debug!(error = %error, "secure token storage unavailable and no legacy account-file tokens exist");
            }
            return Ok(None);
        };
        match self.keyring.save(&tokens) {
            Ok(()) => {
                self.account_file.clear()?;
                Ok(Some(tokens))
            }
            Err(error) => Err(Error::TokenStore(format!(
                "found legacy plaintext account tokens but could not migrate them to secure storage: {error}"
            ))),
        }
    }

    fn clear(&self) -> Result<()> {
        let keyring_result = self.keyring.clear();
        let file_result = self.account_file.clear();
        keyring_result.and(file_result)
    }
}

pub fn save_account_profile_without_tokens(
    paths: &cue_core::app_paths::AppPaths,
    account: &cue_core::AccountConfig,
) -> Result<()> {
    let mut profile = account.clone();
    profile.access_token = None;
    profile.refresh_token = None;
    cue_core::save_account(paths, &profile).map_err(|error| Error::TokenStore(error.to_string()))
}

pub fn save_account_profile_and_tokens(
    paths: &cue_core::app_paths::AppPaths,
    account: &cue_core::AccountConfig,
) -> Result<()> {
    save_account_profile_without_tokens(paths, account)?;
    if let Some(tokens) = tokens_from_account(account) {
        SecureAccountStore::new(paths.clone()).save(&tokens)?;
    }
    Ok(())
}

pub fn tokens_available(paths: &cue_core::app_paths::AppPaths) -> bool {
    std::env::var_os("BLUEY_CLOUD_TOKEN")
        .or_else(|| std::env::var_os("BLUEY_API_TOKEN"))
        .or_else(|| std::env::var_os("CUE_CLOUD_TOKEN"))
        .or_else(|| std::env::var_os("CUE_API_TOKEN"))
        .filter(|value| !value.is_empty())
        .is_some()
        || SecureAccountStore::new(paths.clone())
            .load()
            .ok()
            .flatten()
            .is_some()
}

fn tokens_from_account(account: &cue_core::AccountConfig) -> Option<Tokens> {
    let access = account
        .access_token
        .as_deref()
        .map(str::trim)
        .filter(|token| !token.is_empty())?
        .to_string();
    Some(Tokens {
        access,
        refresh: account.refresh_token.clone().unwrap_or_default(),
        email: account.user_id.clone(),
    })
}

#[cfg(test)]
fn plaintext_token_fallback_enabled() -> bool {
    true
}

#[cfg(not(test))]
fn plaintext_token_fallback_enabled() -> bool {
    truthy_env("BLUEY_ALLOW_PLAINTEXT_TOKENS") || truthy_env("BLUEY_DEV_PLAINTEXT_TOKENS")
}

#[cfg(not(test))]
fn truthy_env(name: &str) -> bool {
    std::env::var(name).ok().is_some_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
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
    fn save_account_profile_without_tokens_strips_secret_fields() {
        let base = std::env::temp_dir().join(format!(
            "bluey-account-profile-no-tokens-{}-{}",
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
        account.access_token = Some("secret-access".into());
        account.refresh_token = Some("secret-refresh".into());

        save_account_profile_without_tokens(&paths, &account).unwrap();

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
