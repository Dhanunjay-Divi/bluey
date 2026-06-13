//! OS-keychain–backed credential store for cloud vendors.
//!
//! Every cloud vendor gets its own keychain service name (`bluey_cloud_<vendor>`)
//! and within that service one or more *credential keys* (e.g. `api_key`,
//! `refresh_token`). The store is vendor-agnostic — the registry row tells us
//! the vendor short name + which key(s) to look up.
//!
//! ### Hard rules (enforced by this module)
//!
//! - The token is held in `String` only between `load()` returning and the
//!   caller consuming it; it is **never** placed into a struct that derives
//!   `Debug`.
//! - The custom [`Debug`] impls on this module emit a placeholder, not the
//!   value. Tests assert this.
//! - The store is a trait so tests can inject [`MemoryCredentialStore`]
//!   without writing to the real keychain. CI uses the memory impl;
//!   production uses [`KeychainCredentialStore`].

use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex;

use anyhow::{Context, Result};

/// Service-name prefix for cloud-agent credentials. Each vendor's service is
/// `bluey_cloud_<vendor>` so the namespace stays distinct from the existing
/// `bluey_account` (router OAuth) and `bluey_stt_*` (STT BYOK) services that
/// other parts of the workspace already use.
const SERVICE_PREFIX: &str = "bluey_cloud_";

/// A read/write surface over one vendor's credentials. The trait exists so
/// tests inject [`MemoryCredentialStore`]; production uses
/// [`KeychainCredentialStore`].
pub trait VendorCredentialStore: Send + Sync {
    /// Vendor short name, e.g. `"cursor"`. Used to derive the service name.
    fn vendor(&self) -> &'static str;

    /// Save `value` under credential `key` (e.g. `"api_key"`). Overwrites
    /// any previous value.
    fn save(&self, key: &str, value: &str) -> Result<()>;

    /// Load the value under `key`. Returns `Ok(None)` when the entry is
    /// absent (vendor not connected yet); `Err` only for I/O or
    /// platform-keychain failures.
    fn load(&self, key: &str) -> Result<Option<String>>;

    /// Delete the value under `key`. Best-effort: a missing entry is
    /// success, not failure.
    fn clear(&self, key: &str) -> Result<()>;
}

/// Production: OS keychain. Same `keyring::Entry` API as the rest of the
/// workspace (`cue-cloud-client`, `cue-daemon/secrets`), kept consistent so
/// migration paths are simple.
pub struct KeychainCredentialStore {
    vendor: &'static str,
}

impl KeychainCredentialStore {
    pub fn new(vendor: &'static str) -> Self {
        Self { vendor }
    }

    fn service(&self) -> String {
        format!("{SERVICE_PREFIX}{}", self.vendor)
    }

    fn entry(&self, key: &str) -> Result<keyring::Entry> {
        keyring::Entry::new(&self.service(), key)
            .with_context(|| format!("opening keychain entry for {key}"))
    }
}

impl fmt::Debug for KeychainCredentialStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Vendor is fine (it's static, public, low-risk); never reflect the
        // entries themselves. Future drift on this Debug impl is guarded by
        // `test_keychain_store_debug_carries_no_secret_shape`.
        f.debug_struct("KeychainCredentialStore")
            .field("vendor", &self.vendor)
            .finish_non_exhaustive()
    }
}

impl VendorCredentialStore for KeychainCredentialStore {
    fn vendor(&self) -> &'static str {
        self.vendor
    }

    fn save(&self, key: &str, value: &str) -> Result<()> {
        self.entry(key)?
            .set_password(value)
            .with_context(|| format!("writing {} credential {key}", self.vendor))
    }

    fn load(&self, key: &str) -> Result<Option<String>> {
        match self.entry(key)?.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(anyhow::anyhow!(
                "reading {} credential {key}: {e}",
                self.vendor
            )),
        }
    }

    fn clear(&self, key: &str) -> Result<()> {
        match self.entry(key)?.delete_password() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(anyhow::anyhow!(
                "clearing {} credential {key}: {e}",
                self.vendor
            )),
        }
    }
}

/// In-memory store for tests and headless CI environments where the real
/// keychain is unavailable. NOT exported for production code (the type itself
/// is `pub` so tests across the workspace can mock against the trait, but the
/// daemon must wire up [`KeychainCredentialStore`] in `main`).
pub struct MemoryCredentialStore {
    vendor: &'static str,
    inner: Mutex<HashMap<String, String>>,
}

impl MemoryCredentialStore {
    pub fn new(vendor: &'static str) -> Self {
        Self {
            vendor,
            inner: Mutex::new(HashMap::new()),
        }
    }
}

impl fmt::Debug for MemoryCredentialStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MemoryCredentialStore")
            .field("vendor", &self.vendor)
            .finish_non_exhaustive()
    }
}

impl VendorCredentialStore for MemoryCredentialStore {
    fn vendor(&self) -> &'static str {
        self.vendor
    }

    fn save(&self, key: &str, value: &str) -> Result<()> {
        self.inner
            .lock()
            .expect("memory store lock")
            .insert(key.to_string(), value.to_string());
        Ok(())
    }

    fn load(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .inner
            .lock()
            .expect("memory store lock")
            .get(key)
            .cloned())
    }

    fn clear(&self, key: &str) -> Result<()> {
        self.inner.lock().expect("memory store lock").remove(key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_store_round_trip() {
        let store = MemoryCredentialStore::new("cursor");
        assert!(store.load("api_key").unwrap().is_none());
        store.save("api_key", "crsr_sekrit").unwrap();
        assert_eq!(
            store.load("api_key").unwrap().as_deref(),
            Some("crsr_sekrit")
        );
        store.clear("api_key").unwrap();
        assert!(store.load("api_key").unwrap().is_none());
        // Clearing an already-missing entry is success, not failure.
        store.clear("api_key").unwrap();
    }

    #[test]
    fn test_memory_store_isolates_keys() {
        let store = MemoryCredentialStore::new("cursor");
        store.save("api_key", "first").unwrap();
        store.save("refresh", "second").unwrap();
        assert_eq!(store.load("api_key").unwrap().as_deref(), Some("first"));
        assert_eq!(store.load("refresh").unwrap().as_deref(), Some("second"));
        store.clear("api_key").unwrap();
        assert_eq!(store.load("api_key").unwrap(), None);
        // Clearing one key MUST NOT touch sibling keys.
        assert_eq!(store.load("refresh").unwrap().as_deref(), Some("second"));
    }

    #[test]
    fn test_keychain_store_debug_carries_no_secret_shape() {
        // Defense-in-depth: the production Debug impl must not even hint at
        // the credential. Future drift on the impl would be caught here.
        let store = KeychainCredentialStore::new("cursor");
        let dbg = format!("{store:?}");
        assert!(dbg.contains("KeychainCredentialStore"));
        assert!(dbg.contains("cursor"));
        for forbidden in ["token", "api_key", "password", "sekrit", "Bearer"] {
            assert!(
                !dbg.to_lowercase().contains(&forbidden.to_lowercase()),
                "Debug impl leaked {forbidden:?} into {dbg}"
            );
        }
    }

    #[test]
    fn test_memory_store_debug_carries_no_secret_shape() {
        let store = MemoryCredentialStore::new("cursor");
        store.save("api_key", "crsr_top_secret").unwrap();
        let dbg = format!("{store:?}");
        assert!(!dbg.contains("crsr_top_secret"));
        assert!(!dbg.to_lowercase().contains("api_key"));
    }

    #[test]
    fn test_service_prefix_is_namespaced() {
        // The cloud namespace must not collide with existing services like
        // `bluey_account` (router OAuth) or `bluey_stt_*` (STT BYOK).
        let store = KeychainCredentialStore::new("cursor");
        let svc = store.service();
        assert!(svc.starts_with(SERVICE_PREFIX));
        assert!(svc.contains("cursor"));
        assert_ne!(svc, "bluey_account");
        assert!(!svc.starts_with("bluey_stt_"));
    }

    #[test]
    fn test_vendor_short_name_is_static() {
        // Vendors are added as registry rows (static), so the vendor short
        // name must be `&'static str` — no allocation per call.
        let store: Box<dyn VendorCredentialStore> =
            Box::new(KeychainCredentialStore::new("cursor"));
        let v: &'static str = store.vendor();
        assert_eq!(v, "cursor");
    }
}
