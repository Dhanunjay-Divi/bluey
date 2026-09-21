//! Secure API key storage via OS keychain (keyring crate).

use anyhow::{bail, Result};

#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

const SERVICE: &str = "cue-daemon";

#[cfg(test)]
static KEYRING_ENTRY_CONSTRUCTIONS: AtomicUsize = AtomicUsize::new(0);

fn truthy_env(name: &str) -> bool {
    std::env::var(name).ok().is_some_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

fn keychain_access_disabled() -> bool {
    if std::env::var_os("BLUEY_TEST_WORKSPACE_ROOT").is_some() {
        return true;
    }
    let has_explicit_policy = std::env::var_os("BLUEY_USE_OS_KEYCHAIN").is_some()
        || std::env::var_os("BLUEY_USE_SECURE_STORE").is_some();
    has_explicit_policy
        && !truthy_env("BLUEY_USE_OS_KEYCHAIN")
        && !truthy_env("BLUEY_USE_SECURE_STORE")
}

fn keyring_entry(service: &str, account: &str) -> Result<keyring::Entry> {
    if keychain_access_disabled() {
        bail!("OS keychain access is disabled");
    }
    #[cfg(test)]
    KEYRING_ENTRY_CONSTRUCTIONS.fetch_add(1, Ordering::SeqCst);
    Ok(keyring::Entry::new(service, account)?)
}

pub fn store_api_key(provider: &str, key: &str) -> Result<()> {
    if keychain_access_disabled() {
        bail!("OS keychain access is disabled");
    }
    let entry = keyring_entry(SERVICE, &format!("stt_{provider}"))?;
    entry.set_password(key)?;
    Ok(())
}

pub fn load_api_key(provider: &str) -> Result<Option<String>> {
    if keychain_access_disabled() {
        return Ok(None);
    }
    let entry = keyring_entry(SERVICE, &format!("stt_{provider}"))?;
    match entry.get_password() {
        Ok(pw) => Ok(Some(pw)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn delete_api_key(provider: &str) -> Result<()> {
    if keychain_access_disabled() {
        return Ok(());
    }
    let entry = keyring_entry(SERVICE, &format!("stt_{provider}"))?;
    normalize_delete_result(entry.delete_password())
}

fn normalize_delete_result(result: keyring::Result<()>) -> Result<()> {
    match result {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SERVICE: &str = "cue-daemon-test-ephemeral";

    fn store_test(provider: &str, key: &str) -> Result<()> {
        let entry = keyring_entry(TEST_SERVICE, &format!("stt_{provider}"))?;
        entry.set_password(key)?;
        Ok(())
    }

    fn load_test(provider: &str) -> Result<Option<String>> {
        let entry = keyring_entry(TEST_SERVICE, &format!("stt_{provider}"))?;
        match entry.get_password() {
            Ok(pw) => Ok(Some(pw)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn delete_test(provider: &str) -> Result<()> {
        let entry = keyring_entry(TEST_SERVICE, &format!("stt_{provider}"))?;
        normalize_delete_result(entry.delete_password())
    }

    #[test]
    #[ignore = "requires interactive Keychain access on macOS; run with --ignored locally"]
    fn roundtrip() {
        let provider = "test_roundtrip";
        let _ = delete_test(provider);
        assert_eq!(load_test(provider).unwrap(), None);
        store_test(provider, "sk-secret123").unwrap();
        assert_eq!(load_test(provider).unwrap(), Some("sk-secret123".into()));
        delete_test(provider).unwrap();
        assert_eq!(load_test(provider).unwrap(), None);
    }

    #[test]
    fn delete_missing_result_is_ok_without_platform_access() {
        assert!(normalize_delete_result(Err(keyring::Error::NoEntry)).is_ok());
        assert!(normalize_delete_result(Err(keyring::Error::Invalid(
            "account".to_string(),
            "test failure".to_string(),
        )))
        .is_err());
    }

    #[test]
    fn isolated_test_workspace_never_constructs_a_keyring_entry() {
        let previous = std::env::var_os("BLUEY_TEST_WORKSPACE_ROOT");
        std::env::set_var(
            "BLUEY_TEST_WORKSPACE_ROOT",
            "/tmp/bluey-keychain-disabled-test",
        );
        KEYRING_ENTRY_CONSTRUCTIONS.store(0, Ordering::SeqCst);

        assert_eq!(load_api_key("must_not_touch_keychain").unwrap(), None);
        assert!(store_api_key("must_not_touch_keychain", "not-a-real-key").is_err());
        delete_api_key("must_not_touch_keychain").unwrap();
        assert_eq!(KEYRING_ENTRY_CONSTRUCTIONS.load(Ordering::SeqCst), 0);

        if let Some(value) = previous {
            std::env::set_var("BLUEY_TEST_WORKSPACE_ROOT", value);
        } else {
            std::env::remove_var("BLUEY_TEST_WORKSPACE_ROOT");
        }
    }
}
