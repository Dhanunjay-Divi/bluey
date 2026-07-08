//! Secure API key storage via OS keychain (keyring crate).

use anyhow::Result;

const SERVICE: &str = "cue-daemon";

pub fn store_api_key(provider: &str, key: &str) -> Result<()> {
    let entry = keyring::Entry::new(SERVICE, &format!("stt_{provider}"))?;
    entry.set_password(key)?;
    Ok(())
}

pub fn load_api_key(provider: &str) -> Result<Option<String>> {
    let entry = keyring::Entry::new(SERVICE, &format!("stt_{provider}"))?;
    match entry.get_password() {
        Ok(pw) => Ok(Some(pw)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn delete_api_key(provider: &str) -> Result<()> {
    let entry = keyring::Entry::new(SERVICE, &format!("stt_{provider}"))?;
    match entry.delete_password() {
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
        let entry = keyring::Entry::new(TEST_SERVICE, &format!("stt_{provider}"))?;
        entry.set_password(key)?;
        Ok(())
    }

    fn load_test(provider: &str) -> Result<Option<String>> {
        let entry = keyring::Entry::new(TEST_SERVICE, &format!("stt_{provider}"))?;
        match entry.get_password() {
            Ok(pw) => Ok(Some(pw)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn delete_test(provider: &str) -> Result<()> {
        let entry = keyring::Entry::new(TEST_SERVICE, &format!("stt_{provider}"))?;
        match entry.delete_password() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.into()),
        }
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

    #[cfg_attr(
        not(any(target_os = "macos", target_os = "windows")),
        ignore = "requires a desktop keyring backend"
    )]
    #[test]
    fn delete_nonexistent_is_ok() {
        assert!(delete_test("nonexistent_provider_xyz").is_ok());
    }
}
