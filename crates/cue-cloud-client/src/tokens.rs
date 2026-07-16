//! Bluey account token storage.
//!
//! New installs store Bluey account tokens in the private local account
//! profile so normal `bluey on` / Listen / Screen flows do not trigger OS
//! keychain prompts. OS keychain / credential-store access is opt-in for
//! operator builds and can be used as a legacy fallback for older installs.

use crate::error::{Error, Result};

use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

const KEYRING_SERVICE: &str = "bluey_account";
const KEY_ACCESS: &str = "access_token";
const KEY_REFRESH: &str = "refresh_token";
const KEY_EMAIL: &str = "account_email";
const DEFAULT_BLUEY_API_URL: &str = "https://bluey.sh";
const ACCOUNT_WRITE_MAX_ATTEMPTS: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
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

    /// Replace the exact credential snapshot a refresh started with. Returns
    /// false when logout, login, or another refresh changed that snapshot.
    fn compare_and_swap(&self, expected: &Tokens, replacement: &Tokens) -> Result<bool>;
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

    fn save_account_if_generation(
        &self,
        expected_generation: u64,
        account: &cue_core::AccountConfig,
    ) -> Result<bool> {
        cue_core::config::save_account_if_generation(&self.paths, expected_generation, account)
            .map_err(|error| Error::TokenStore(error.to_string()))
    }

    fn save_profile_without_tokens(&self, tokens: &Tokens) -> Result<()> {
        validate_tokens(tokens)?;
        for _ in 0..ACCOUNT_WRITE_MAX_ATTEMPTS {
            let current = self.load_account()?;
            let expected_generation = account_generation(current.as_ref());
            let mut account = account_for_tokens(current, tokens);
            account.access_token = None;
            account.refresh_token = None;
            if self.save_account_if_generation(expected_generation, &account)? {
                return Ok(());
            }
        }
        Err(account_write_contention_error())
    }
}

impl TokenStore for AccountFileStore {
    fn save(&self, tokens: &Tokens) -> Result<()> {
        validate_tokens(tokens)?;
        for _ in 0..ACCOUNT_WRITE_MAX_ATTEMPTS {
            let current = self.load_account()?;
            let expected_generation = account_generation(current.as_ref());
            let account = account_for_tokens(current, tokens);
            if self.save_account_if_generation(expected_generation, &account)? {
                return Ok(());
            }
        }
        Err(account_write_contention_error())
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
        let tokens = Tokens {
            access,
            refresh: account.refresh_token.unwrap_or_default(),
            email: account.user_id,
        };
        validate_tokens(&tokens)?;
        if account.provider.trim().eq_ignore_ascii_case("local") {
            return Err(Error::TokenStore(format!(
                "local account profile {} contains cloud credentials",
                self.paths.account_file.display()
            )));
        }
        Ok(Some(tokens))
    }

    fn clear(&self) -> Result<()> {
        for _ in 0..ACCOUNT_WRITE_MAX_ATTEMPTS {
            let Some(mut account) = self.load_account()? else {
                return Ok(());
            };
            let expected_generation = account.credential_generation;
            account.access_token = None;
            account.refresh_token = None;
            if self.save_account_if_generation(expected_generation, &account)? {
                return Ok(());
            }
        }
        Err(account_write_contention_error())
    }

    fn compare_and_swap(&self, expected: &Tokens, replacement: &Tokens) -> Result<bool> {
        validate_tokens(expected)?;
        validate_tokens(replacement)?;
        if expected.email.trim() != replacement.email.trim() {
            return Err(Error::TokenStore(
                "refusing to refresh credentials into a different account identity".to_string(),
            ));
        }

        let Some(account) = self.load_account()? else {
            return Ok(false);
        };
        let expected_generation = account.credential_generation;
        if tokens_from_account(&account).as_ref() != Some(expected) {
            return Ok(false);
        }
        let replacement_account = account_for_tokens(Some(account), replacement);
        self.save_account_if_generation(expected_generation, &replacement_account)
    }
}

/// Default desktop token store.
///
/// Customer builds use the private local account profile by default. Set
/// `BLUEY_USE_OS_KEYCHAIN=1` or `BLUEY_USE_SECURE_STORE=1` to opt into the OS
/// credential store. Set `BLUEY_LEGACY_KEYRING_FALLBACK=1` only when migrating
/// older installs that already have tokens in the previous keyring store.
pub struct SecureAccountStore {
    account_file: AccountFileStore,
    keyring: KeyringStore,
}

impl SecureAccountStore {
    pub fn new(paths: cue_core::app_paths::AppPaths) -> Self {
        let keyring = KeyringStore::with_account_file(&paths.account_file);
        Self {
            account_file: AccountFileStore::new(paths),
            keyring,
        }
    }
}

impl TokenStore for SecureAccountStore {
    fn save(&self, tokens: &Tokens) -> Result<()> {
        if !os_secure_store_enabled() || plaintext_token_fallback_enabled() {
            if plaintext_token_fallback_enabled() {
                tracing::warn!(
                    "using plaintext account-token fallback because BLUEY_ALLOW_PLAINTEXT_TOKENS is enabled"
                );
            }
            return self.account_file.save(tokens);
        }

        self.account_file.save_profile_without_tokens(tokens)?;
        match self.keyring.save(tokens) {
            Ok(()) => Ok(()),
            Err(error) => Err(Error::TokenStore(format!(
                "secure token storage unavailable: {error}. Set BLUEY_USE_OS_KEYCHAIN=0 to use Bluey's private account file instead."
            ))),
        }
    }

    fn load(&self) -> Result<Option<Tokens>> {
        if !os_secure_store_enabled() || plaintext_token_fallback_enabled() {
            if let Some(tokens) = self.account_file.load()? {
                return Ok(Some(tokens));
            }
            if legacy_keyring_fallback_enabled() {
                match self.keyring.load() {
                    Ok(Some(tokens)) => {
                        self.account_file.save(&tokens)?;
                        return Ok(Some(tokens));
                    }
                    Ok(None) => {}
                    Err(error) => {
                        tracing::debug!(error = %error, "legacy keyring fallback unavailable");
                    }
                }
            }
            return Ok(None);
        }

        // The profile is authoritative for account identity even when secrets
        // live in the opt-in OS credential store. Parse it before touching the
        // keychain so malformed JSON cannot silently look signed out.
        let profile = self.account_file.load_account()?;
        let keyring_error = match self.keyring.load() {
            Ok(Some(tokens)) => {
                if let Some(profile) = profile.as_ref() {
                    validate_profile_tokens(profile, &tokens)?;
                } else {
                    self.account_file.save_profile_without_tokens(&tokens)?;
                }
                return Ok(Some(tokens));
            }
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
        let file_result = self.account_file.clear();
        if os_secure_store_enabled() || legacy_keyring_fallback_enabled() {
            return self.keyring.clear().and(file_result);
        }
        file_result
    }

    fn compare_and_swap(&self, expected: &Tokens, replacement: &Tokens) -> Result<bool> {
        if !os_secure_store_enabled() || plaintext_token_fallback_enabled() {
            return self.account_file.compare_and_swap(expected, replacement);
        }

        let Some(profile_before) = self.account_file.load_account()? else {
            return Ok(false);
        };
        validate_profile_tokens(&profile_before, expected)?;
        let generation = profile_before.credential_generation;
        if !self.keyring.compare_and_swap(expected, replacement)? {
            return Ok(false);
        }

        let Some(profile_after) = self.account_file.load_account()? else {
            self.keyring.clear_if_current(replacement)?;
            return Ok(false);
        };
        let unchanged = profile_after.credential_generation == generation
            && profile_after.owner_account_id_with_token_state(true)
                == profile_before.owner_account_id_with_token_state(true);
        if !unchanged {
            self.keyring.clear_if_current(replacement)?;
        }
        Ok(unchanged)
    }
}

fn os_secure_store_enabled() -> bool {
    truthy_env("BLUEY_USE_OS_KEYCHAIN") || truthy_env("BLUEY_USE_SECURE_STORE")
}

fn legacy_keyring_fallback_enabled() -> bool {
    truthy_env("BLUEY_LEGACY_KEYRING_FALLBACK")
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
    let store = SecureAccountStore::new(paths.clone());
    store.clear()?;
    save_account_profile_without_tokens(paths, account)?;
    if let Some(tokens) = tokens_from_account(account) {
        store.save(&tokens)?;
    }
    Ok(())
}

pub fn owner_account_id(paths: &cue_core::app_paths::AppPaths) -> Result<Option<String>> {
    let Some(tokens) = SecureAccountStore::new(paths.clone()).load()? else {
        return Ok(None);
    };
    let account = cue_core::load_account(paths)
        .map_err(|error| Error::TokenStore(error.to_string()))?
        .ok_or_else(|| {
            Error::TokenStore(
                "stored credentials are missing their account profile identity".to_string(),
            )
        })?;
    validate_profile_tokens(&account, &tokens)?;
    Ok(account
        .owner_account_id_with_token_state(true)
        .map(ToString::to_string))
}

pub fn try_tokens_available(paths: &cue_core::app_paths::AppPaths) -> Result<bool> {
    // A corrupt profile must remain visible even when another credential
    // source is configured.
    cue_core::load_account(paths).map_err(|error| Error::TokenStore(error.to_string()))?;
    if environment_token_available() {
        return Ok(true);
    }
    Ok(SecureAccountStore::new(paths.clone()).load()?.is_some())
}

pub fn tokens_available(paths: &cue_core::app_paths::AppPaths) -> bool {
    match try_tokens_available(paths) {
        Ok(available) => available,
        Err(error) => {
            tracing::error!(error = %error, "could not determine account token state");
            // Treat an unreadable profile as an account error, never as an
            // unsigned/local profile.
            true
        }
    }
}

fn environment_token_available() -> bool {
    std::env::var_os("BLUEY_CLOUD_TOKEN")
        .or_else(|| std::env::var_os("BLUEY_CLOUD_API_TOKEN"))
        .or_else(|| std::env::var_os("BLUEY_API_TOKEN"))
        .or_else(|| std::env::var_os("CUE_CLOUD_TOKEN"))
        .or_else(|| std::env::var_os("CUE_API_TOKEN"))
        .filter(|value| !value.is_empty())
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

fn account_generation(account: Option<&cue_core::AccountConfig>) -> u64 {
    account
        .map(|account| account.credential_generation)
        .unwrap_or_default()
}

fn account_for_tokens(
    account: Option<cue_core::AccountConfig>,
    tokens: &Tokens,
) -> cue_core::AccountConfig {
    let mut account = account.unwrap_or_else(|| {
        let mut account = cue_core::AccountConfig::local();
        account.api_url = default_account_api_url();
        account
    });
    let previous_user_id = account.user_id.trim();
    let next_user_id = tokens.email.trim();
    if previous_user_id != next_user_id && previous_user_id != "local-user" {
        account.cloud_account_id = None;
        account.workspace_id = "default".to_string();
        account.linked_at = cue_core::clock::now_epoch_ms_string();
    }
    if account.provider.trim().eq_ignore_ascii_case("local") {
        account.provider = "bluey".to_string();
    }
    account.user_id = next_user_id.to_string();
    account.access_token = Some(tokens.access.trim().to_string());
    account.refresh_token =
        (!tokens.refresh.trim().is_empty()).then(|| tokens.refresh.trim().to_string());
    account
}

fn validate_tokens(tokens: &Tokens) -> Result<()> {
    if tokens.access.trim().is_empty() {
        return Err(Error::TokenStore(
            "stored account credentials have an empty access token".to_string(),
        ));
    }
    if tokens.email.trim().is_empty() || tokens.email.trim() == "local-user" {
        return Err(Error::TokenStore(
            "stored account credentials have no cloud account identity".to_string(),
        ));
    }
    Ok(())
}

fn validate_profile_tokens(account: &cue_core::AccountConfig, tokens: &Tokens) -> Result<()> {
    validate_tokens(tokens)?;
    if account.provider.trim().eq_ignore_ascii_case("local") {
        return Err(Error::TokenStore(
            "local account profile cannot establish signed-in cloud identity".to_string(),
        ));
    }
    let profile_user_id = account.user_id.trim();
    if profile_user_id.is_empty()
        || profile_user_id == "local-user"
        || profile_user_id != tokens.email.trim()
    {
        return Err(Error::TokenStore(format!(
            "account profile identity {:?} does not match stored credentials",
            account.user_id
        )));
    }
    if account.owner_account_id_with_token_state(true).is_none() {
        return Err(Error::TokenStore(
            "signed-in account profile has no stable owner identity".to_string(),
        ));
    }
    Ok(())
}

fn account_write_contention_error() -> Error {
    Error::TokenStore(
        "account profile changed repeatedly while credentials were being updated".to_string(),
    )
}

#[cfg(test)]
fn plaintext_token_fallback_enabled() -> bool {
    true
}

#[cfg(not(test))]
fn plaintext_token_fallback_enabled() -> bool {
    truthy_env("BLUEY_ALLOW_PLAINTEXT_TOKENS") || truthy_env("BLUEY_DEV_PLAINTEXT_TOKENS")
}

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
pub struct KeyringStore {
    operation_lock_path: Option<PathBuf>,
}

impl KeyringStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn with_account_file(account_file: &Path) -> Self {
        Self {
            operation_lock_path: Some(keyring_lock_path(account_file)),
        }
    }

    fn entry(name: &str) -> Result<keyring::Entry> {
        Ok(keyring::Entry::new(KEYRING_SERVICE, name)?)
    }

    fn save_unlocked(tokens: &Tokens) -> Result<()> {
        validate_tokens(tokens)?;
        // Access is the bearer capability. Remove it first and publish the new
        // access token last so a partial write cannot preserve the old bearer.
        Self::delete_unlocked(KEY_ACCESS)?;
        Self::entry(KEY_REFRESH)?.set_password(tokens.refresh.trim())?;
        Self::entry(KEY_EMAIL)?.set_password(tokens.email.trim())?;
        Self::entry(KEY_ACCESS)?.set_password(tokens.access.trim())?;
        Ok(())
    }

    fn load_unlocked() -> Result<Option<Tokens>> {
        let access = match Self::entry(KEY_ACCESS)?.get_password() {
            Ok(value) => value,
            Err(keyring::Error::NoEntry) => return Ok(None),
            Err(error) => return Err(Error::TokenStore(error.to_string())),
        };
        let refresh = match Self::entry(KEY_REFRESH)?.get_password() {
            Ok(value) => value,
            Err(keyring::Error::NoEntry) => String::new(),
            Err(error) => return Err(Error::TokenStore(error.to_string())),
        };
        let email = match Self::entry(KEY_EMAIL)?.get_password() {
            Ok(value) => value,
            Err(keyring::Error::NoEntry) => {
                return Err(Error::TokenStore(
                    "OS credential store contains a bearer token without account identity"
                        .to_string(),
                ));
            }
            Err(error) => return Err(Error::TokenStore(error.to_string())),
        };
        let tokens = Tokens {
            access,
            refresh,
            email,
        };
        validate_tokens(&tokens)?;
        Ok(Some(tokens))
    }

    fn clear_unlocked() -> Result<()> {
        for key in [KEY_ACCESS, KEY_REFRESH, KEY_EMAIL] {
            Self::delete_unlocked(key)?;
        }
        Ok(())
    }

    fn delete_unlocked(key: &str) -> Result<()> {
        match Self::entry(key)?.delete_password() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(Error::TokenStore(error.to_string())),
        }
    }

    fn clear_if_current(&self, expected: &Tokens) -> Result<bool> {
        let _guard = self.operation_lock()?;
        if Self::load_unlocked()?.as_ref() != Some(expected) {
            return Ok(false);
        }
        Self::clear_unlocked()?;
        Ok(true)
    }

    fn operation_lock(&self) -> Result<KeyringOperationGuard> {
        static PROCESS_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

        let process = PROCESS_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .map_err(|_| Error::TokenStore("OS credential-store lock was poisoned".to_string()))?;
        let lock_path = match self.operation_lock_path.as_ref() {
            Some(path) => path.clone(),
            None => {
                let paths = cue_core::app_paths::AppPaths::discover()
                    .map_err(|error| Error::TokenStore(error.to_string()))?;
                paths
                    .ensure()
                    .map_err(|error| Error::TokenStore(error.to_string()))?;
                keyring_lock_path(&paths.account_file)
            }
        };
        if let Some(parent) = lock_path.parent() {
            cue_core::app_paths::create_private_dir(parent)
                .map_err(|error| Error::TokenStore(error.to_string()))?;
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|error| {
                Error::TokenStore(format!(
                    "failed to open credential-store lock {}: {error}",
                    lock_path.display()
                ))
            })?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&lock_path, fs::Permissions::from_mode(0o600)).map_err(
                |error| {
                    Error::TokenStore(format!(
                        "failed to protect credential-store lock {}: {error}",
                        lock_path.display()
                    ))
                },
            )?;
        }

        file.lock().map_err(|error| {
            Error::TokenStore(format!(
                "failed to lock credential store {}: {error}",
                lock_path.display()
            ))
        })?;
        Ok(KeyringOperationGuard {
            _process: process,
            _file: file,
        })
    }
}

struct KeyringOperationGuard {
    _process: std::sync::MutexGuard<'static, ()>,
    _file: File,
}

fn keyring_lock_path(account_file: &Path) -> PathBuf {
    let mut path = account_file.as_os_str().to_os_string();
    path.push(".keyring.lock");
    PathBuf::from(path)
}

impl TokenStore for KeyringStore {
    fn save(&self, tokens: &Tokens) -> Result<()> {
        let _guard = self.operation_lock()?;
        Self::save_unlocked(tokens)
    }

    fn load(&self) -> Result<Option<Tokens>> {
        let _guard = self.operation_lock()?;
        Self::load_unlocked()
    }

    fn clear(&self) -> Result<()> {
        let _guard = self.operation_lock()?;
        Self::clear_unlocked()
    }

    fn compare_and_swap(&self, expected: &Tokens, replacement: &Tokens) -> Result<bool> {
        validate_tokens(expected)?;
        validate_tokens(replacement)?;
        if expected.email.trim() != replacement.email.trim() {
            return Err(Error::TokenStore(
                "refusing to refresh credentials into a different account identity".to_string(),
            ));
        }
        let _guard = self.operation_lock()?;
        if Self::load_unlocked()?.as_ref() != Some(expected) {
            return Ok(false);
        }
        Self::save_unlocked(replacement)?;
        Ok(true)
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
        validate_tokens(tokens)?;
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

    fn compare_and_swap(&self, expected: &Tokens, replacement: &Tokens) -> Result<bool> {
        validate_tokens(expected)?;
        validate_tokens(replacement)?;
        if expected.email.trim() != replacement.email.trim() {
            return Err(Error::TokenStore(
                "refusing to refresh credentials into a different account identity".to_string(),
            ));
        }
        let mut current = self.inner.lock().unwrap();
        if current.as_ref() != Some(expected) {
            return Ok(false);
        }
        *current = Some(replacement.clone());
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_paths(name: &str) -> cue_core::app_paths::AppPaths {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let id = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let base = std::env::temp_dir().join(format!(
            "bluey-token-test-{name}-{}-{id}",
            std::process::id()
        ));
        cue_core::app_paths::AppPaths {
            data_dir: base.join("data"),
            config_dir: base.join("config"),
            runtime_dir: base.join("run"),
            state_file: base.join("run/daemon-state.json"),
            account_file: base.join("config/account.json"),
            settings_file: base.join("config/settings.json"),
        }
    }

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
    fn owner_account_id_is_fallible_and_requires_stored_credentials() {
        let paths = test_paths("owner-id");
        let mut account = cue_core::AccountConfig::local();
        account.provider = "bluey".to_string();
        account.cloud_account_id = Some("account-123".to_string());
        account.user_id = "user@example.com".to_string();
        cue_core::save_account(&paths, &account).unwrap();
        assert_eq!(owner_account_id(&paths).unwrap(), None);

        AccountFileStore::new(paths.clone())
            .save(&Tokens {
                access: "access".to_string(),
                refresh: "refresh".to_string(),
                email: "user@example.com".to_string(),
            })
            .unwrap();
        assert_eq!(
            owner_account_id(&paths).unwrap().as_deref(),
            Some("account-123")
        );

        let mut account = cue_core::load_account(&paths).unwrap().unwrap();
        account.cloud_account_id = None;
        cue_core::save_account(&paths, &account).unwrap();
        assert_eq!(
            owner_account_id(&paths).unwrap().as_deref(),
            Some("user@example.com")
        );

        AccountFileStore::new(paths.clone()).clear().unwrap();
        assert_eq!(owner_account_id(&paths).unwrap(), None);
        let _ = std::fs::remove_dir_all(paths.config_dir);
    }

    #[test]
    fn malformed_profile_fails_closed_for_load_identity_and_save() {
        let paths = test_paths("malformed");
        paths.ensure().unwrap();
        let malformed = br#"{"provider":"bluey","access_token": nope}"#;
        std::fs::write(&paths.account_file, malformed).unwrap();
        let store = AccountFileStore::new(paths.clone());

        for error in [
            store.load().unwrap_err(),
            owner_account_id(&paths).unwrap_err(),
            try_tokens_available(&paths).unwrap_err(),
            store
                .save(&Tokens {
                    access: "new-access".to_string(),
                    refresh: "new-refresh".to_string(),
                    email: "new@example.com".to_string(),
                })
                .unwrap_err(),
        ] {
            let message = error.to_string();
            assert!(message.contains("failed to parse"), "{message}");
            assert!(
                message.contains(paths.account_file.to_str().unwrap()),
                "{message}"
            );
        }
        assert!(tokens_available(&paths));
        assert_eq!(std::fs::read(&paths.account_file).unwrap(), malformed);
        let _ = std::fs::remove_dir_all(paths.config_dir);
    }

    #[test]
    fn account_switch_invalidates_old_identity_and_refresh_snapshot() {
        let paths = test_paths("account-switch");
        let mut account = cue_core::AccountConfig::local();
        account.provider = "bluey".to_string();
        account.cloud_account_id = Some("account-a".to_string());
        account.user_id = "a@example.com".to_string();
        account.access_token = Some("access-a".to_string());
        account.refresh_token = Some("refresh-a".to_string());
        cue_core::save_account(&paths, &account).unwrap();

        let store = AccountFileStore::new(paths.clone());
        let stale = store.load().unwrap().unwrap();
        let current = Tokens {
            access: "access-b".to_string(),
            refresh: "refresh-b".to_string(),
            email: "b@example.com".to_string(),
        };
        store.save(&current).unwrap();

        assert!(!store
            .compare_and_swap(
                &stale,
                &Tokens {
                    access: "late-access-a".to_string(),
                    refresh: "late-refresh-a".to_string(),
                    email: stale.email.clone(),
                },
            )
            .unwrap());
        assert_eq!(store.load().unwrap(), Some(current));
        let profile = cue_core::load_account(&paths).unwrap().unwrap();
        assert_eq!(profile.cloud_account_id, None);
        assert_eq!(
            owner_account_id(&paths).unwrap().as_deref(),
            Some("b@example.com")
        );
        let _ = std::fs::remove_dir_all(paths.config_dir);
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

    #[test]
    fn secure_account_store_defaults_to_account_file_tokens() {
        let base = std::env::temp_dir().join(format!(
            "bluey-secure-store-file-default-{}-{}",
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

        let store = SecureAccountStore::new(paths.clone());
        store
            .save(&Tokens {
                access: "access".into(),
                refresh: "refresh".into(),
                email: "user@example.com".into(),
            })
            .unwrap();

        let loaded = store.load().unwrap().unwrap();
        assert_eq!(loaded.access, "access");
        assert_eq!(loaded.refresh, "refresh");
        assert_eq!(loaded.email, "user@example.com");

        let account = cue_core::load_account(&paths).unwrap().unwrap();
        assert_eq!(account.provider, "bluey");
        assert_eq!(account.user_id, "user@example.com");
        assert_eq!(account.access_token.as_deref(), Some("access"));
        assert_eq!(account.refresh_token.as_deref(), Some("refresh"));

        store.clear().unwrap();
        let cleared = cue_core::load_account(&paths).unwrap().unwrap();
        assert_eq!(cleared.user_id, "user@example.com");
        assert!(cleared.access_token.is_none());
        assert!(cleared.refresh_token.is_none());

        let _ = std::fs::remove_dir_all(base);
    }
}
