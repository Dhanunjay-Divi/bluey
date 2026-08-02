//! Bluey account token storage.
//!
//! Desktop builds use the operating system credential store by default:
//! macOS Keychain, Windows Credential Manager, and the keyring backend selected
//! by the `keyring` crate on Unix desktops. Access and refresh tokens are kept
//! together in one versioned credential so a refresh cannot leave a mixed pair.
//! The account JSON file contains profile/API metadata only.
//!
//! Headless or otherwise unsupported environments may explicitly opt into the
//! private account-file fallback with `BLUEY_TOKEN_STORE=file` or
//! `BLUEY_ALLOW_PLAINTEXT_TOKENS=1`. Secure-store errors never silently select
//! that fallback.

use std::collections::HashMap;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

const KEYRING_SERVICE: &str = "bluey_account";
const KEY_TOKEN_BUNDLE: &str = "account_tokens_v1";
const LEGACY_KEY_ACCESS: &str = "access_token";
const LEGACY_KEY_REFRESH: &str = "refresh_token";
const LEGACY_KEY_EMAIL: &str = "account_email";
const SECURE_TOKEN_BUNDLE_VERSION: u8 = 1;
const DEFAULT_BLUEY_API_URL: &str = "https://bluey.sh";
const TOKEN_OPERATION_LOCK_FILE: &str = ".account-token-operation.lock";
const TOKEN_OPERATION_LOCK_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, PartialEq, Eq)]
pub struct Tokens {
    pub access: String,
    pub refresh: String,
    pub email: String,
    pub account_id: Option<String>,
}

/// Never expose bearer credentials through routine debug/error logging.
impl fmt::Debug for Tokens {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Tokens")
            .field("access", &"[redacted]")
            .field("refresh", &"[redacted]")
            .field("email", &self.email)
            .field("account_id", &self.account_id.as_ref().map(|_| "[present]"))
            .finish()
    }
}

/// Trait so tests can inject an in-memory store; production uses the OS-backed
/// [`KeyringStore`].
pub trait TokenStore: Send + Sync {
    fn save(&self, tokens: &Tokens) -> Result<()>;
    fn load(&self) -> Result<Option<Tokens>>;
    fn clear(&self) -> Result<()>;

    /// Atomically replace a token pair only if the persistent value still
    /// matches the pair that initiated an asynchronous refresh.
    fn save_if_current(&self, expected: &Tokens, tokens: &Tokens) -> Result<bool>;
}

fn token_store_error(message: &'static str) -> Error {
    Error::TokenStore(message.to_string())
}

fn validate_tokens(tokens: &Tokens) -> Result<()> {
    if tokens.access.trim().is_empty() {
        return Err(token_store_error(
            "secure token entry is malformed: access token is missing",
        ));
    }
    if !tokens.refresh.is_empty() && tokens.refresh.trim().is_empty() {
        return Err(token_store_error(
            "secure token entry is malformed: refresh token is invalid",
        ));
    }
    if tokens.account_id.as_deref().is_some_and(|account_id| {
        account_id.trim().is_empty()
            || account_id.chars().count() > 200
            || account_id.chars().any(char::is_control)
    }) {
        return Err(token_store_error(
            "secure token entry is malformed: account identity is invalid",
        ));
    }
    Ok(())
}

/// Account-file token storage exists only for the explicit headless fallback
/// and for reading/scrubbing legacy desktop profiles during migration.
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

    fn profile_for_tokens(&self, tokens: &Tokens) -> Result<cue_core::AccountConfig> {
        let mut account = self.load_account()?.unwrap_or_else(|| {
            let mut account = cue_core::AccountConfig::local();
            account.provider = "bluey".to_string();
            account.api_url = default_account_api_url();
            account
        });
        if account.provider == "local" {
            account.provider = "bluey".to_string();
        }
        if !tokens.email.trim().is_empty()
            && !account.user_id.trim().is_empty()
            && account.user_id != tokens.email
            && tokens.account_id.is_none()
        {
            // A stable cloud account id belongs to the old user identity. A
            // cross-store crash must fall back to the new credential email,
            // never pair new bearer tokens with an old owner's local scope.
            account.cloud_account_id = None;
        }
        account.user_id = tokens.email.clone();
        if let Some(account_id) = tokens
            .account_id
            .as_deref()
            .map(str::trim)
            .filter(|account_id| !account_id.is_empty())
        {
            account.cloud_account_id = Some(account_id.to_string());
        }
        Ok(account)
    }

    fn save_profile_without_tokens(&self, tokens: &Tokens) -> Result<()> {
        let mut account = self.profile_for_tokens(tokens)?;
        account.access_token = None;
        account.refresh_token = None;
        self.save_account(&account)
    }

    fn scrub_tokens_preserving_profile(&self) -> Result<()> {
        let Some(mut account) = self.load_account()? else {
            return Ok(());
        };
        if account.access_token.is_none() && account.refresh_token.is_none() {
            return Ok(());
        }
        account.access_token = None;
        account.refresh_token = None;
        self.save_account(&account)
    }

    fn mark_signed_out(&self) -> Result<()> {
        let Some(account) = self.load_account()? else {
            return Ok(());
        };
        let mut signed_out = cue_core::AccountConfig::local();
        // Device identity and the selected deployment endpoint are not account
        // credentials. Preserving them avoids registering a new desktop or
        // silently switching environments on the next browser sign-in.
        signed_out.device_id = account.device_id;
        signed_out.api_url = account.api_url;
        self.save_account(&signed_out)
    }
}

impl TokenStore for AccountFileStore {
    fn save(&self, tokens: &Tokens) -> Result<()> {
        validate_tokens(tokens)?;
        let mut account = self.profile_for_tokens(tokens)?;
        account.access_token = Some(tokens.access.clone());
        account.refresh_token = (!tokens.refresh.is_empty()).then(|| tokens.refresh.clone());
        self.save_account(&account)
    }

    fn load(&self) -> Result<Option<Tokens>> {
        let Some(account) = self.load_account()? else {
            return Ok(None);
        };
        tokens_from_account(&account)
    }

    fn clear(&self) -> Result<()> {
        self.scrub_tokens_preserving_profile()
    }

    fn save_if_current(&self, _expected: &Tokens, _tokens: &Tokens) -> Result<bool> {
        // Callers that refresh credentials must use SecureAccountStore, which
        // owns the process and cross-process operation locks. Failing closed
        // here prevents a direct file-store caller from mistaking a load/save
        // sequence for an atomic compare-and-swap.
        Err(token_store_error(
            "account-file token refresh requires the coordinated token store",
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StoragePolicy {
    SecureOnly,
    ExplicitPlaintextFile,
}

fn storage_policy_from_values(
    token_store: Option<&str>,
    allow_plaintext: Option<&str>,
    dev_plaintext: Option<&str>,
) -> StoragePolicy {
    let named_file_store = token_store.is_some_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "file" | "plaintext"
        )
    });
    if named_file_store
        || allow_plaintext.is_some_and(truthy_value)
        || dev_plaintext.is_some_and(truthy_value)
    {
        StoragePolicy::ExplicitPlaintextFile
    } else {
        StoragePolicy::SecureOnly
    }
}

fn storage_policy_from_env() -> StoragePolicy {
    storage_policy_from_values(
        std::env::var("BLUEY_TOKEN_STORE").ok().as_deref(),
        std::env::var("BLUEY_ALLOW_PLAINTEXT_TOKENS")
            .ok()
            .as_deref(),
        std::env::var("BLUEY_DEV_PLAINTEXT_TOKENS").ok().as_deref(),
    )
}

fn truthy_value(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn shared_operation_lock(account_file: PathBuf) -> Arc<Mutex<()>> {
    static LOCKS: OnceLock<Mutex<HashMap<PathBuf, Weak<Mutex<()>>>>> = OnceLock::new();
    let registry = LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut registry = registry
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(existing) = registry.get(&account_file).and_then(Weak::upgrade) {
        return existing;
    }
    let lock = Arc::new(Mutex::new(()));
    registry.insert(account_file, Arc::downgrade(&lock));
    lock
}

/// Default desktop token store. The OS store is authoritative unless the
/// plaintext fallback was explicitly selected before construction.
pub struct SecureAccountStore {
    account_file: AccountFileStore,
    secure_store: Arc<dyn TokenStore>,
    policy: StoragePolicy,
    operation_lock: Arc<Mutex<()>>,
    operation_lock_file: PathBuf,
}

struct TokenOperationGuard<'a> {
    _process_guard: std::sync::MutexGuard<'a, ()>,
    _file: File,
}

impl SecureAccountStore {
    pub fn new(paths: cue_core::app_paths::AppPaths) -> Self {
        Self::with_components(
            paths,
            Arc::new(KeyringStore::new()),
            storage_policy_from_env(),
        )
    }

    fn with_components(
        paths: cue_core::app_paths::AppPaths,
        secure_store: Arc<dyn TokenStore>,
        policy: StoragePolicy,
    ) -> Self {
        let operation_lock = shared_operation_lock(paths.account_file.clone());
        let operation_lock_file = paths.config_dir.join(TOKEN_OPERATION_LOCK_FILE);
        Self {
            account_file: AccountFileStore::new(paths),
            secure_store,
            policy,
            operation_lock,
            operation_lock_file,
        }
    }

    fn lock_operations(&self) -> Result<TokenOperationGuard<'_>> {
        let process_guard = self
            .operation_lock
            .lock()
            .map_err(|_| token_store_error("token storage lock is unavailable"))?;
        if let Some(parent) = self.operation_lock_file.parent() {
            cue_core::app_paths::create_private_dir(parent)
                .map_err(|_| token_store_error("token lock directory is unavailable"))?;
        }
        let file = open_private_lock_file(&self.operation_lock_file)?;
        let deadline = Instant::now() + TOKEN_OPERATION_LOCK_TIMEOUT;
        loop {
            match file.try_lock() {
                Ok(()) => break,
                Err(std::fs::TryLockError::WouldBlock) => {
                    if Instant::now() >= deadline {
                        return Err(token_store_error(
                            "token storage is busy in another Bluey process",
                        ));
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(_) => return Err(token_store_error("token file lock is unavailable")),
            }
        }
        Ok(TokenOperationGuard {
            _process_guard: process_guard,
            _file: file,
        })
    }

    fn secure_failure(action: &'static str) -> Error {
        token_store_error(match action {
            "read" => "OS credential store read failed; plaintext fallback was not used",
            "write" => "OS credential store write failed; plaintext fallback was not used",
            "clear" => "OS credential store clear failed",
            _ => "OS credential store operation failed",
        })
    }

    fn save_locked(&self, tokens: &Tokens) -> Result<()> {
        if self.policy == StoragePolicy::ExplicitPlaintextFile {
            return self.account_file.save(tokens);
        }

        // Secure first, scrub second. If either step fails, a subsequent load
        // retries the scrub and never falls back to plaintext implicitly.
        self.secure_store
            .save(tokens)
            .map_err(|_| Self::secure_failure("write"))?;
        self.account_file.save_profile_without_tokens(tokens)
    }
}

fn open_private_lock_file(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let file = options
        .open(path)
        .map_err(|_| token_store_error("failed to open token operation lock"))?;
    let metadata = file
        .metadata()
        .map_err(|_| token_store_error("failed to inspect token operation lock"))?;
    if !metadata.is_file() {
        return Err(token_store_error(
            "token operation lock is not a regular file",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o777 != 0o600 {
            file.set_permissions(std::fs::Permissions::from_mode(0o600))
                .map_err(|_| token_store_error("failed to secure token operation lock"))?;
        }
    }
    Ok(file)
}

impl TokenStore for SecureAccountStore {
    fn save(&self, tokens: &Tokens) -> Result<()> {
        validate_tokens(tokens)?;
        let _guard = self.lock_operations()?;
        self.save_locked(tokens)
    }

    fn load(&self) -> Result<Option<Tokens>> {
        let _guard = self.lock_operations()?;
        if self.policy == StoragePolicy::ExplicitPlaintextFile {
            return self.account_file.load();
        }

        match self
            .secure_store
            .load()
            .map_err(|_| Self::secure_failure("read"))?
        {
            Some(mut tokens) => {
                validate_tokens(&tokens)?;
                if let Some(profile) = self.account_file.load_account()? {
                    if profile.provider == "local" {
                        self.secure_store
                            .clear()
                            .map_err(|_| Self::secure_failure("clear"))?;
                        self.account_file.mark_signed_out()?;
                        return Ok(None);
                    }
                    if tokens.email.trim().is_empty() && !profile.user_id.trim().is_empty() {
                        tokens.email = profile.user_id;
                        self.account_file.scrub_tokens_preserving_profile()?;
                    } else if !tokens.email.trim().is_empty() && profile.user_id != tokens.email {
                        self.account_file.save_profile_without_tokens(&tokens)?;
                    } else if !tokens.email.trim().is_empty() {
                        self.account_file.scrub_tokens_preserving_profile()?;
                    }
                } else if !tokens.email.trim().is_empty() {
                    self.account_file.save_profile_without_tokens(&tokens)?;
                }
                Ok(Some(tokens))
            }
            None => {
                let Some(tokens) = self.account_file.load()? else {
                    return Ok(None);
                };
                validate_tokens(&tokens)?;
                // One-time legacy migration: do not remove the private-file
                // copy until the complete secure bundle has been written.
                self.secure_store
                    .save(&tokens)
                    .map_err(|_| Self::secure_failure("write"))?;
                self.account_file.scrub_tokens_preserving_profile()?;
                Ok(Some(tokens))
            }
        }
    }

    fn clear(&self) -> Result<()> {
        let _guard = self.lock_operations()?;
        if self.policy == StoragePolicy::ExplicitPlaintextFile {
            // This mode is selected specifically for hosts without a usable OS
            // credential service. Do not touch a keyring that may block while
            // logging out of the explicitly selected file store.
            return self.account_file.mark_signed_out();
        }
        if let Err(error) = self
            .secure_store
            .clear()
            .map_err(|_| Self::secure_failure("clear"))
        {
            // Remove any legacy plaintext copy, but keep the linked-owner
            // marker while a secure credential may still exist.
            let _ = self.account_file.scrub_tokens_preserving_profile();
            return Err(error);
        }
        self.account_file.mark_signed_out()
    }

    fn save_if_current(&self, expected: &Tokens, tokens: &Tokens) -> Result<bool> {
        validate_tokens(tokens)?;
        let _guard = self.lock_operations()?;
        let current = if self.policy == StoragePolicy::ExplicitPlaintextFile {
            self.account_file.load()?
        } else {
            self.secure_store
                .load()
                .map_err(|_| Self::secure_failure("read"))?
        };
        if current.as_ref() != Some(expected) {
            return Ok(false);
        }
        self.save_locked(tokens)?;
        Ok(true)
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
    let store = SecureAccountStore::new(paths.clone());
    save_account_profile_and_tokens_with_store(paths, account, &store)
}

fn save_account_profile_and_tokens_with_store(
    paths: &cue_core::app_paths::AppPaths,
    account: &cue_core::AccountConfig,
    store: &SecureAccountStore,
) -> Result<()> {
    let _guard = store.lock_operations()?;
    let Some(tokens) = tokens_from_account(account)? else {
        return save_account_profile_without_tokens(paths, account);
    };
    if store.policy == StoragePolicy::ExplicitPlaintextFile {
        // The explicit fallback deliberately keeps its credentials in the
        // private file, so write the metadata shell before adding them.
        save_account_profile_without_tokens(paths, account)?;
        store.account_file.save(&tokens)
    } else {
        // Persist the secure copy before any account-file token fields are
        // scrubbed, and retain the operation lock through the final metadata
        // write so a concurrent refresh cannot pair another token with this
        // account profile.
        store.save_locked(&tokens)?;
        save_account_profile_without_tokens(paths, account)
    }
}

pub fn tokens_available(paths: &cue_core::app_paths::AppPaths) -> bool {
    std::env::var_os("BLUEY_CLOUD_TOKEN")
        .or_else(|| std::env::var_os("BLUEY_CLOUD_API_TOKEN"))
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

fn tokens_from_account(account: &cue_core::AccountConfig) -> Result<Option<Tokens>> {
    let access = account.access_token.as_deref();
    let refresh = account.refresh_token.as_deref();
    match (access, refresh) {
        (None, None) => Ok(None),
        (None, Some(_)) => Err(token_store_error(
            "legacy account token entry is incomplete",
        )),
        (Some(access), refresh) => {
            let tokens = Tokens {
                access: access.to_string(),
                refresh: refresh.unwrap_or_default().to_string(),
                email: account.user_id.clone(),
                account_id: account.cloud_account_id.clone(),
            };
            validate_tokens(&tokens)?;
            Ok(Some(tokens))
        }
    }
}

fn default_account_api_url() -> String {
    std::env::var("BLUEY_API_BASE_URL")
        .or_else(|_| std::env::var("BLUEY_CLOUD_API_URL"))
        .or_else(|_| std::env::var("CUE_CLOUD_API_URL"))
        .unwrap_or_else(|_| DEFAULT_BLUEY_API_URL.to_string())
}

#[derive(Serialize, Deserialize)]
struct SecureTokenBundle {
    version: u8,
    access: String,
    refresh: String,
    #[serde(default)]
    email: String,
    #[serde(default)]
    account_id: Option<String>,
}

fn encode_secure_token_bundle(tokens: &Tokens) -> Result<String> {
    validate_tokens(tokens)?;
    serde_json::to_string(&SecureTokenBundle {
        version: SECURE_TOKEN_BUNDLE_VERSION,
        access: tokens.access.clone(),
        refresh: tokens.refresh.clone(),
        email: tokens.email.clone(),
        account_id: tokens.account_id.clone(),
    })
    .map_err(|_| token_store_error("failed to encode secure token bundle"))
}

fn decode_secure_token_bundle(encoded: &str, legacy_email: String) -> Result<Tokens> {
    let bundle = serde_json::from_str::<SecureTokenBundle>(encoded)
        .map_err(|_| token_store_error("secure token bundle is malformed"))?;
    if bundle.version != SECURE_TOKEN_BUNDLE_VERSION {
        return Err(token_store_error(
            "secure token bundle version is unsupported",
        ));
    }
    let tokens = Tokens {
        access: bundle.access,
        refresh: bundle.refresh,
        email: if bundle.email.trim().is_empty() {
            legacy_email
        } else {
            bundle.email
        },
        account_id: bundle.account_id,
    };
    validate_tokens(&tokens)?;
    Ok(tokens)
}

fn decode_legacy_secure_pair(
    access: Option<String>,
    refresh: Option<String>,
    email: String,
) -> Result<Option<Tokens>> {
    match (access, refresh) {
        (None, None) => Ok(None),
        (Some(access), Some(refresh)) => {
            let tokens = Tokens {
                access,
                refresh,
                email,
                account_id: None,
            };
            validate_tokens(&tokens)?;
            Ok(Some(tokens))
        }
        _ => Err(token_store_error(
            "legacy secure token entries are incomplete",
        )),
    }
}

/// Production keyring-backed store. A single JSON credential prevents access
/// and refresh values from being interleaved by concurrent refreshes.
#[derive(Default)]
pub struct KeyringStore {
    operation_lock: Mutex<()>,
}

impl KeyringStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn entry(name: &str) -> Result<keyring::Entry> {
        keyring::Entry::new(KEYRING_SERVICE, name)
            .map_err(|_| token_store_error("failed to open OS credential entry"))
    }

    fn get_optional(name: &str) -> Result<Option<String>> {
        match Self::entry(name)?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(token_store_error("failed to read OS credential entry")),
        }
    }

    fn delete_if_present(name: &str) -> Result<()> {
        match Self::entry(name)?.delete_password() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(token_store_error("failed to clear OS credential entry")),
        }
    }

    fn save_unlocked(&self, tokens: &Tokens) -> Result<()> {
        let encoded = encode_secure_token_bundle(tokens)?;
        Self::entry(KEY_TOKEN_BUNDLE)?
            .set_password(&encoded)
            .map_err(|_| token_store_error("failed to write OS credential entry"))?;
        // The authoritative bundle now contains the email and both token
        // values. Legacy cleanup is best-effort and cannot create a mixed pair.
        let _ = Self::delete_if_present(LEGACY_KEY_EMAIL);
        let _ = Self::delete_if_present(LEGACY_KEY_ACCESS);
        let _ = Self::delete_if_present(LEGACY_KEY_REFRESH);
        Ok(())
    }

    fn load_unlocked(&self) -> Result<Option<Tokens>> {
        if let Some(encoded) = Self::get_optional(KEY_TOKEN_BUNDLE)? {
            let current = decode_secure_token_bundle(&encoded, String::new())?;
            if !current.email.trim().is_empty() {
                return Ok(Some(current));
            }
            let legacy_email = Self::get_optional(LEGACY_KEY_EMAIL)?.unwrap_or_default();
            return decode_secure_token_bundle(&encoded, legacy_email).map(Some);
        }

        let email = Self::get_optional(LEGACY_KEY_EMAIL)?.unwrap_or_default();
        let legacy = decode_legacy_secure_pair(
            Self::get_optional(LEGACY_KEY_ACCESS)?,
            Self::get_optional(LEGACY_KEY_REFRESH)?,
            email,
        )?;
        if let Some(tokens) = legacy.as_ref() {
            self.save_unlocked(tokens)?;
            // Old entries remain secure even if best-effort cleanup fails; the
            // atomic bundle is authoritative from this point forward.
            let _ = Self::delete_if_present(LEGACY_KEY_ACCESS);
            let _ = Self::delete_if_present(LEGACY_KEY_REFRESH);
        }
        Ok(legacy)
    }

    fn lock_operations(&self) -> Result<std::sync::MutexGuard<'_, ()>> {
        self.operation_lock
            .lock()
            .map_err(|_| token_store_error("OS credential store lock is unavailable"))
    }
}

impl TokenStore for KeyringStore {
    fn save(&self, tokens: &Tokens) -> Result<()> {
        let _guard = self.lock_operations()?;
        self.save_unlocked(tokens)
    }

    fn load(&self) -> Result<Option<Tokens>> {
        let _guard = self.lock_operations()?;
        self.load_unlocked()
    }

    fn clear(&self) -> Result<()> {
        let _guard = self.lock_operations()?;
        let mut first_error = None;
        for key in [
            KEY_TOKEN_BUNDLE,
            LEGACY_KEY_ACCESS,
            LEGACY_KEY_REFRESH,
            LEGACY_KEY_EMAIL,
        ] {
            if let Err(error) = Self::delete_if_present(key) {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    fn save_if_current(&self, expected: &Tokens, tokens: &Tokens) -> Result<bool> {
        validate_tokens(tokens)?;
        let _guard = self.lock_operations()?;
        if self.load_unlocked()?.as_ref() != Some(expected) {
            return Ok(false);
        }
        self.save_unlocked(tokens)?;
        Ok(true)
    }
}

/// In-memory implementation retained for tests and explicitly constructed
/// headless clients. It is never selected implicitly by desktop production.
#[derive(Default)]
pub struct MemoryStore {
    inner: Mutex<Option<Tokens>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl TokenStore for MemoryStore {
    fn save(&self, tokens: &Tokens) -> Result<()> {
        validate_tokens(tokens)?;
        *self
            .inner
            .lock()
            .map_err(|_| token_store_error("memory token store lock is unavailable"))? =
            Some(tokens.clone());
        Ok(())
    }

    fn load(&self) -> Result<Option<Tokens>> {
        self.inner
            .lock()
            .map(|tokens| tokens.clone())
            .map_err(|_| token_store_error("memory token store lock is unavailable"))
    }

    fn clear(&self) -> Result<()> {
        *self
            .inner
            .lock()
            .map_err(|_| token_store_error("memory token store lock is unavailable"))? = None;
        Ok(())
    }

    fn save_if_current(&self, expected: &Tokens, tokens: &Tokens) -> Result<bool> {
        validate_tokens(tokens)?;
        let mut current = self
            .inner
            .lock()
            .map_err(|_| token_store_error("memory token store lock is unavailable"))?;
        if current.as_ref() != Some(expected) {
            return Ok(false);
        }
        *current = Some(tokens.clone());
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::thread;
    use std::time::Duration;

    use super::*;

    fn test_paths(label: &str) -> (PathBuf, cue_core::app_paths::AppPaths) {
        let base = std::env::temp_dir().join(format!(
            "bluey-token-store-{label}-{}-{}",
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
        (base, paths)
    }

    fn tokens(access: &str, refresh: &str, email: &str) -> Tokens {
        Tokens {
            access: access.to_string(),
            refresh: refresh.to_string(),
            email: email.to_string(),
            account_id: None,
        }
    }

    fn profile_with_tokens(access: &str, refresh: &str) -> cue_core::AccountConfig {
        let mut account = cue_core::AccountConfig::local();
        account.provider = "bluey".to_string();
        account.api_url = "https://staging.bluey.sh".to_string();
        account.cloud_account_id = Some("account-1".to_string());
        account.user_id = "user@example.com".to_string();
        account.workspace_id = "workspace-1".to_string();
        account.device_id = "device-1".to_string();
        account.access_token = Some(access.to_string());
        account.refresh_token = Some(refresh.to_string());
        account
    }

    #[derive(Default)]
    struct OrderingStore {
        paths: Mutex<Option<cue_core::app_paths::AppPaths>>,
        inner: Mutex<Option<Tokens>>,
        fail_save: AtomicBool,
        saw_plaintext_during_save: AtomicBool,
        save_calls: AtomicUsize,
        clear_calls: AtomicUsize,
    }

    impl OrderingStore {
        fn for_paths(paths: cue_core::app_paths::AppPaths) -> Self {
            Self {
                paths: Mutex::new(Some(paths)),
                ..Self::default()
            }
        }
    }

    impl TokenStore for OrderingStore {
        fn save(&self, value: &Tokens) -> Result<()> {
            self.save_calls.fetch_add(1, Ordering::SeqCst);
            if let Some(paths) = self.paths.lock().unwrap().as_ref() {
                let account = cue_core::load_account(paths).unwrap().unwrap();
                self.saw_plaintext_during_save.store(
                    account.access_token.is_some() || account.refresh_token.is_some(),
                    Ordering::SeqCst,
                );
            }
            if self.fail_save.load(Ordering::SeqCst) {
                return Err(Error::TokenStore(format!(
                    "backend rejected secret {}",
                    value.access
                )));
            }
            *self.inner.lock().unwrap() = Some(value.clone());
            Ok(())
        }

        fn load(&self) -> Result<Option<Tokens>> {
            Ok(self.inner.lock().unwrap().clone())
        }

        fn clear(&self) -> Result<()> {
            self.clear_calls.fetch_add(1, Ordering::SeqCst);
            *self.inner.lock().unwrap() = None;
            Ok(())
        }

        fn save_if_current(&self, expected: &Tokens, value: &Tokens) -> Result<bool> {
            let mut current = self.inner.lock().unwrap();
            if current.as_ref() != Some(expected) {
                return Ok(false);
            }
            *current = Some(value.clone());
            Ok(true)
        }
    }

    #[derive(Default)]
    struct FailingStore {
        save_calls: AtomicUsize,
        fail_clear: AtomicBool,
    }

    impl TokenStore for FailingStore {
        fn save(&self, value: &Tokens) -> Result<()> {
            self.save_calls.fetch_add(1, Ordering::SeqCst);
            Err(Error::TokenStore(format!(
                "write failed for {}",
                value.access
            )))
        }

        fn load(&self) -> Result<Option<Tokens>> {
            Err(Error::TokenStore(
                "read failed with secret-token".to_string(),
            ))
        }

        fn clear(&self) -> Result<()> {
            if self.fail_clear.load(Ordering::SeqCst) {
                Err(Error::TokenStore(
                    "clear failed with secret-token".to_string(),
                ))
            } else {
                Ok(())
            }
        }

        fn save_if_current(&self, _expected: &Tokens, value: &Tokens) -> Result<bool> {
            Err(Error::TokenStore(format!(
                "compare-and-save failed for {}",
                value.access
            )))
        }
    }

    #[derive(Default)]
    struct SlowStore {
        inner: Mutex<Option<Tokens>>,
        active_saves: AtomicUsize,
        max_active_saves: AtomicUsize,
    }

    impl TokenStore for SlowStore {
        fn save(&self, value: &Tokens) -> Result<()> {
            let active = self.active_saves.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_active_saves.fetch_max(active, Ordering::SeqCst);
            thread::sleep(Duration::from_millis(5));
            *self.inner.lock().unwrap() = Some(value.clone());
            self.active_saves.fetch_sub(1, Ordering::SeqCst);
            Ok(())
        }

        fn load(&self) -> Result<Option<Tokens>> {
            Ok(self.inner.lock().unwrap().clone())
        }

        fn clear(&self) -> Result<()> {
            *self.inner.lock().unwrap() = None;
            Ok(())
        }

        fn save_if_current(&self, expected: &Tokens, value: &Tokens) -> Result<bool> {
            let mut current = self.inner.lock().unwrap();
            if current.as_ref() != Some(expected) {
                return Ok(false);
            }
            *current = Some(value.clone());
            Ok(true)
        }
    }

    #[test]
    fn memory_store_round_trips_and_debug_redacts_tokens() {
        let store = MemoryStore::new();
        let value = tokens("access-secret", "refresh-secret", "e@example.com");
        let debug = format!("{value:?}");
        assert!(!debug.contains("access-secret"));
        assert!(!debug.contains("refresh-secret"));

        assert!(store.load().unwrap().is_none());
        store.save(&value).unwrap();
        assert_eq!(store.load().unwrap(), Some(value.clone()));
        store.clear().unwrap();
        assert!(store.load().unwrap().is_none());

        let refreshed = tokens("new-access", "new-refresh", "e@example.com");
        assert!(!store.save_if_current(&value, &refreshed).unwrap());
        assert!(store.load().unwrap().is_none());
    }

    #[test]
    fn secure_bundle_and_legacy_pair_fail_closed_without_leaking_values() {
        let malformed =
            decode_secure_token_bundle(r#"{"version":1,"access":"secret-access"}"#, String::new())
                .unwrap_err();
        assert!(!malformed.to_string().contains("secret-access"));

        let partial =
            decode_legacy_secure_pair(Some("secret-access".to_string()), None, String::new())
                .unwrap_err();
        assert!(partial.to_string().contains("incomplete"));
        assert!(!partial.to_string().contains("secret-access"));

        let value = tokens("access", "", "user@example.com");
        let encoded = encode_secure_token_bundle(&value).unwrap();
        assert_eq!(
            decode_secure_token_bundle(&encoded, "stale@example.com".to_string()).unwrap(),
            value
        );
    }

    #[test]
    fn migration_writes_secure_copy_before_scrubbing_and_preserves_profile() {
        let (base, paths) = test_paths("migration-order");
        let account = profile_with_tokens("legacy-access", "legacy-refresh");
        cue_core::save_account(&paths, &account).unwrap();
        let secure = Arc::new(OrderingStore::for_paths(paths.clone()));
        let store = SecureAccountStore::with_components(
            paths.clone(),
            secure.clone(),
            StoragePolicy::SecureOnly,
        );

        let loaded = store.load().unwrap().unwrap();
        assert_eq!(loaded.access, "legacy-access");
        assert_eq!(loaded.account_id.as_deref(), Some("account-1"));
        assert!(secure.saw_plaintext_during_save.load(Ordering::SeqCst));
        assert_eq!(secure.save_calls.load(Ordering::SeqCst), 1);

        let profile = cue_core::load_account(&paths).unwrap().unwrap();
        assert_eq!(profile.api_url, "https://staging.bluey.sh");
        assert_eq!(profile.cloud_account_id.as_deref(), Some("account-1"));
        assert_eq!(profile.workspace_id, "workspace-1");
        assert_eq!(profile.device_id, "device-1");
        assert!(profile.access_token.is_none());
        assert!(profile.refresh_token.is_none());
        let profile_json = std::fs::read_to_string(&paths.account_file).unwrap();
        assert!(!profile_json.contains("legacy-access"));
        assert!(!profile_json.contains("legacy-refresh"));

        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn failed_migration_keeps_legacy_file_and_error_is_redacted() {
        let (base, paths) = test_paths("migration-failure");
        cue_core::save_account(
            &paths,
            &profile_with_tokens("legacy-secret-access", "legacy-secret-refresh"),
        )
        .unwrap();
        let secure = Arc::new(OrderingStore::for_paths(paths.clone()));
        secure.fail_save.store(true, Ordering::SeqCst);
        let store = SecureAccountStore::with_components(
            paths.clone(),
            secure.clone(),
            StoragePolicy::SecureOnly,
        );

        let error = store.load().unwrap_err().to_string();
        assert!(error.contains("plaintext fallback was not used"));
        assert!(!error.contains("legacy-secret-access"));
        assert!(!error.contains("legacy-secret-refresh"));
        let legacy = cue_core::load_account(&paths).unwrap().unwrap();
        assert_eq!(legacy.access_token.as_deref(), Some("legacy-secret-access"));
        assert_eq!(
            legacy.refresh_token.as_deref(),
            Some("legacy-secret-refresh")
        );

        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn existing_secure_tokens_scrub_stale_plaintext_profile() {
        let (base, paths) = test_paths("secure-scrub");
        cue_core::save_account(
            &paths,
            &profile_with_tokens("stale-access", "stale-refresh"),
        )
        .unwrap();
        let secure = Arc::new(MemoryStore::new());
        secure
            .save(&tokens(
                "current-access",
                "current-refresh",
                "keyring@example.com",
            ))
            .unwrap();
        let store =
            SecureAccountStore::with_components(paths.clone(), secure, StoragePolicy::SecureOnly);

        let loaded = store.load().unwrap().unwrap();
        assert_eq!(loaded.access, "current-access");
        assert_eq!(loaded.refresh, "current-refresh");
        assert_eq!(loaded.email, "keyring@example.com");
        let profile = cue_core::load_account(&paths).unwrap().unwrap();
        assert!(profile.access_token.is_none());
        assert!(profile.refresh_token.is_none());
        assert_eq!(profile.user_id, "keyring@example.com");
        assert!(profile.cloud_account_id.is_none());
        assert_eq!(profile.workspace_id, "workspace-1");

        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn secure_save_and_clear_remove_all_copies_and_reset_the_owner_marker() {
        let (base, paths) = test_paths("save-clear");
        let mut profile = profile_with_tokens("old-access", "old-refresh");
        profile.access_token = None;
        profile.refresh_token = None;
        cue_core::save_account(&paths, &profile).unwrap();
        let secure = Arc::new(MemoryStore::new());
        let store = SecureAccountStore::with_components(
            paths.clone(),
            secure.clone(),
            StoragePolicy::SecureOnly,
        );

        store
            .save(&tokens("new-access", "new-refresh", "new@example.com"))
            .unwrap();
        let saved_profile = cue_core::load_account(&paths).unwrap().unwrap();
        assert_eq!(saved_profile.api_url, "https://staging.bluey.sh");
        assert_eq!(saved_profile.workspace_id, "workspace-1");
        assert_eq!(saved_profile.device_id, "device-1");
        assert_eq!(saved_profile.user_id, "new@example.com");
        assert!(saved_profile.access_token.is_none());
        assert!(saved_profile.refresh_token.is_none());
        let profile_json = std::fs::read_to_string(&paths.account_file).unwrap();
        assert!(!profile_json.contains("new-access"));
        assert!(!profile_json.contains("new-refresh"));

        // Simulate a stale legacy copy so clear proves both locations are
        // scrubbed even when the active policy is secure-only.
        let mut stale = saved_profile.clone();
        stale.access_token = Some("stale-access".to_string());
        stale.refresh_token = Some("stale-refresh".to_string());
        cue_core::save_account(&paths, &stale).unwrap();
        store.clear().unwrap();
        assert!(secure.load().unwrap().is_none());
        let cleared = cue_core::load_account(&paths).unwrap().unwrap();
        assert_eq!(cleared.api_url, "https://staging.bluey.sh");
        assert_eq!(cleared.provider, "local");
        assert_eq!(cleared.user_id, "local-user");
        assert!(cleared.cloud_account_id.is_none());
        assert_eq!(cleared.workspace_id, "default");
        assert_eq!(cleared.device_id, "device-1");
        assert!(cleared.linked_owner_id().is_none());
        assert!(cleared.access_token.is_none());
        assert!(cleared.refresh_token.is_none());

        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn fallback_requires_explicit_policy_and_never_follows_secure_failure() {
        assert_eq!(
            storage_policy_from_values(None, None, None),
            StoragePolicy::SecureOnly
        );
        assert_eq!(
            storage_policy_from_values(Some("secure"), Some("0"), Some("false")),
            StoragePolicy::SecureOnly
        );
        assert_eq!(
            storage_policy_from_values(Some("file"), None, None),
            StoragePolicy::ExplicitPlaintextFile
        );
        assert_eq!(
            storage_policy_from_values(None, Some("yes"), None),
            StoragePolicy::ExplicitPlaintextFile
        );

        let (secure_base, secure_paths) = test_paths("no-silent-fallback");
        let failing = Arc::new(FailingStore::default());
        let secure_store = SecureAccountStore::with_components(
            secure_paths.clone(),
            failing.clone(),
            StoragePolicy::SecureOnly,
        );
        let error = secure_store
            .save(&tokens(
                "never-write-plaintext",
                "refresh",
                "user@example.com",
            ))
            .unwrap_err()
            .to_string();
        assert!(error.contains("plaintext fallback was not used"));
        assert!(!error.contains("never-write-plaintext"));
        assert!(cue_core::load_account(&secure_paths).unwrap().is_none());
        let read_error = secure_store.load().unwrap_err().to_string();
        assert!(read_error.contains("plaintext fallback was not used"));
        assert!(!read_error.contains("secret-token"));

        let (fallback_base, fallback_paths) = test_paths("explicit-fallback");
        let fallback_store = SecureAccountStore::with_components(
            fallback_paths.clone(),
            failing.clone(),
            StoragePolicy::ExplicitPlaintextFile,
        );
        fallback_store
            .save(&tokens(
                "explicit-file-access",
                "explicit-file-refresh",
                "user@example.com",
            ))
            .unwrap();
        let fallback = cue_core::load_account(&fallback_paths).unwrap().unwrap();
        assert_eq!(
            fallback.access_token.as_deref(),
            Some("explicit-file-access")
        );
        assert_eq!(failing.save_calls.load(Ordering::SeqCst), 1);

        let fallback_profile = profile_with_tokens("profile-save-access", "profile-save-refresh");
        save_account_profile_and_tokens_with_store(
            &fallback_paths,
            &fallback_profile,
            &fallback_store,
        )
        .unwrap();
        let saved_fallback = cue_core::load_account(&fallback_paths).unwrap().unwrap();
        assert_eq!(
            saved_fallback.access_token.as_deref(),
            Some("profile-save-access")
        );
        assert_eq!(saved_fallback.workspace_id, "workspace-1");

        let _ = std::fs::remove_dir_all(secure_base);
        let _ = std::fs::remove_dir_all(fallback_base);
    }

    #[test]
    fn explicit_file_logout_succeeds_when_os_store_is_unavailable() {
        let (base, paths) = test_paths("headless-clear");
        let failing = Arc::new(FailingStore::default());
        failing.fail_clear.store(true, Ordering::SeqCst);
        let store = SecureAccountStore::with_components(
            paths.clone(),
            failing,
            StoragePolicy::ExplicitPlaintextFile,
        );
        store
            .save(&tokens("file-access", "file-refresh", "file@example.com"))
            .unwrap();

        store
            .clear()
            .expect("headless logout must not require keyring");
        assert!(store.load().unwrap().is_none());
        let profile = cue_core::load_account(&paths).unwrap().unwrap();
        assert_eq!(profile.provider, "local");
        assert!(profile.linked_owner_id().is_none());
        assert!(profile.access_token.is_none());
        assert!(profile.refresh_token.is_none());
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn local_signed_out_marker_prevents_stale_secure_token_resurrection() {
        let (base, paths) = test_paths("signed-out-tombstone");
        cue_core::save_account(&paths, &cue_core::AccountConfig::local()).unwrap();
        let secure = Arc::new(MemoryStore::new());
        secure
            .save(&tokens("stale-access", "stale-refresh", "old@example.com"))
            .unwrap();
        let store =
            SecureAccountStore::with_components(paths, secure.clone(), StoragePolicy::SecureOnly);

        assert!(store.load().unwrap().is_none());
        assert!(secure.load().unwrap().is_none());
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn secure_email_mismatch_clears_stale_owner_id_after_cross_store_crash() {
        let (base, paths) = test_paths("owner-crash-recovery");
        let old_profile = profile_with_tokens("old-access", "old-refresh");
        let mut old_profile_without_tokens = old_profile;
        old_profile_without_tokens.access_token = None;
        old_profile_without_tokens.refresh_token = None;
        cue_core::save_account(&paths, &old_profile_without_tokens).unwrap();

        let secure = Arc::new(MemoryStore::new());
        secure
            .save(&tokens("new-access", "new-refresh", "new@example.com"))
            .unwrap();
        let store =
            SecureAccountStore::with_components(paths.clone(), secure, StoragePolicy::SecureOnly);

        let loaded = store.load().unwrap().unwrap();
        assert_eq!(loaded.email, "new@example.com");
        let recovered = cue_core::load_account(&paths).unwrap().unwrap();
        assert_eq!(recovered.user_id, "new@example.com");
        assert!(recovered.cloud_account_id.is_none());
        assert_eq!(recovered.linked_owner_id(), Some("new@example.com"));
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn stable_account_id_survives_an_email_change() {
        let (base, paths) = test_paths("stable-owner-email-change");
        let mut profile = profile_with_tokens("old-access", "old-refresh");
        profile.access_token = None;
        profile.refresh_token = None;
        cue_core::save_account(&paths, &profile).unwrap();

        let secure = Arc::new(MemoryStore::new());
        let mut current = tokens("new-access", "new-refresh", "renamed@example.com");
        current.account_id = Some("account-1".to_string());
        secure.save(&current).unwrap();
        let store =
            SecureAccountStore::with_components(paths.clone(), secure, StoragePolicy::SecureOnly);

        assert_eq!(store.load().unwrap(), Some(current));
        let recovered = cue_core::load_account(&paths).unwrap().unwrap();
        assert_eq!(recovered.user_id, "renamed@example.com");
        assert_eq!(recovered.cloud_account_id.as_deref(), Some("account-1"));
        assert_eq!(recovered.linked_owner_id(), Some("account-1"));
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn concurrent_profile_and_token_saves_keep_matching_owner_metadata() {
        let (base, paths) = test_paths("profile-token-race");
        let secure = Arc::new(SlowStore::default());
        let store = Arc::new(SecureAccountStore::with_components(
            paths.clone(),
            secure.clone(),
            StoragePolicy::SecureOnly,
        ));

        let mut first = profile_with_tokens("access-a", "refresh-a");
        first.user_id = "a@example.com".to_string();
        first.cloud_account_id = Some("account-a".to_string());
        let mut second = profile_with_tokens("access-b", "refresh-b");
        second.user_id = "b@example.com".to_string();
        second.cloud_account_id = Some("account-b".to_string());

        let first_store = Arc::clone(&store);
        let first_paths = paths.clone();
        let first_thread = thread::spawn(move || {
            save_account_profile_and_tokens_with_store(&first_paths, &first, &first_store).unwrap();
        });
        let second_store = Arc::clone(&store);
        let second_paths = paths.clone();
        let second_thread = thread::spawn(move || {
            save_account_profile_and_tokens_with_store(&second_paths, &second, &second_store)
                .unwrap();
        });
        first_thread.join().unwrap();
        second_thread.join().unwrap();

        let profile = cue_core::load_account(&paths).unwrap().unwrap();
        let saved = secure.load().unwrap().unwrap();
        assert_eq!(saved.email, profile.user_id);
        match saved.access.as_str() {
            "access-a" => assert_eq!(profile.cloud_account_id.as_deref(), Some("account-a")),
            "access-b" => assert_eq!(profile.cloud_account_id.as_deref(), Some("account-b")),
            other => panic!("unexpected access token {other}"),
        }
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn secure_store_serializes_concurrent_refresh_saves_without_mixed_pairs() {
        let (base, paths) = test_paths("concurrent-save");
        let secure = Arc::new(SlowStore::default());
        let store = Arc::new(SecureAccountStore::with_components(
            paths,
            secure.clone(),
            StoragePolicy::SecureOnly,
        ));

        let handles = (0..8)
            .map(|index| {
                let store = Arc::clone(&store);
                thread::spawn(move || {
                    store
                        .save(&tokens(
                            &format!("access-{index}"),
                            &format!("refresh-{index}"),
                            "user@example.com",
                        ))
                        .unwrap();
                })
            })
            .collect::<Vec<_>>();
        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(secure.max_active_saves.load(Ordering::SeqCst), 1);
        let loaded = secure.load().unwrap().unwrap();
        let access_index = loaded.access.strip_prefix("access-").unwrap();
        let refresh_index = loaded.refresh.strip_prefix("refresh-").unwrap();
        assert_eq!(access_index, refresh_index);

        let _ = std::fs::remove_dir_all(base);
    }
}
