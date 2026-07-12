//! Operation-scoped capabilities for the local Bluey Jobs browser.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use subtle::ConstantTimeEq;

type HmacSha256 = Hmac<Sha256>;

const TOKEN_VERSION: u8 = 1;
const TOKEN_AUDIENCE: &str = "bluey-jobs-local-run";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalRunCapabilityClaims {
    pub version: u8,
    pub audience: String,
    pub account_id: String,
    pub application_id: String,
    pub run_id: String,
    pub browser_profile_id: String,
    pub operation: String,
    pub expires_at_ms: i64,
    pub nonce: String,
}

pub fn issue(
    account_id: &str,
    application_id: &str,
    run_id: &str,
    browser_profile_id: &str,
    operation: &str,
    expires_at_ms: i64,
) -> anyhow::Result<String> {
    validate_operation(operation)?;
    if [account_id, application_id, run_id, browser_profile_id]
        .iter()
        .any(|value| value.trim().is_empty() || value.len() > 200)
    {
        anyhow::bail!("invalid local run capability binding");
    }
    let claims = LocalRunCapabilityClaims {
        version: TOKEN_VERSION,
        audience: TOKEN_AUDIENCE.to_string(),
        account_id: account_id.to_string(),
        application_id: application_id.to_string(),
        run_id: run_id.to_string(),
        browser_profile_id: browser_profile_id.to_string(),
        operation: operation.to_string(),
        expires_at_ms,
        nonce: uuid::Uuid::new_v4().simple().to_string(),
    };
    let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims)?);
    let signature = sign(&current_key()?, &payload);
    Ok(format!("{payload}.{signature}"))
}

pub fn verify(
    token: &str,
    expected_run_id: &str,
    expected_operation: &str,
    now_ms: i64,
) -> anyhow::Result<LocalRunCapabilityClaims> {
    validate_operation(expected_operation)?;
    if token.len() > 4_096 {
        anyhow::bail!("invalid local run capability");
    }
    let (payload, supplied_signature) = token
        .split_once('.')
        .ok_or_else(|| anyhow::anyhow!("invalid local run capability"))?;
    if supplied_signature.len() != 64
        || !supplied_signature
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        anyhow::bail!("invalid local run capability");
    }
    let keys = accepted_keys();
    if !keys
        .iter()
        .any(|key| constant_time_eq(&sign(key, payload), supplied_signature))
    {
        anyhow::bail!("invalid local run capability");
    }
    let bytes = URL_SAFE_NO_PAD.decode(payload)?;
    let claims: LocalRunCapabilityClaims = serde_json::from_slice(&bytes)?;
    if claims.version != TOKEN_VERSION
        || claims.audience != TOKEN_AUDIENCE
        || claims.run_id != expected_run_id
        || claims.operation != expected_operation
        || claims.expires_at_ms <= now_ms
        || claims.nonce.len() < 24
    {
        anyhow::bail!("invalid local run capability");
    }
    Ok(claims)
}

fn validate_operation(operation: &str) -> anyhow::Result<()> {
    if matches!(operation, "result" | "resume") {
        Ok(())
    } else {
        anyhow::bail!("invalid local run capability operation")
    }
}

fn sign(key: &str, payload: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(key.as_bytes()).expect("HMAC accepts any key length");
    mac.update(payload.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

fn constant_time_eq(left: &str, right: &str) -> bool {
    left.len() == right.len() && left.as_bytes().ct_eq(right.as_bytes()).unwrap_u8() == 1
}

fn current_key() -> anyhow::Result<String> {
    let key = std::env::var("BLUEY_JOBS_LOCAL_RUN_CAPABILITY_KEY").unwrap_or_default();
    if key.len() >= 32 {
        return Ok(key);
    }
    #[cfg(debug_assertions)]
    return Ok("bluey-jobs-local-run-debug-key-32-bytes".to_string());
    #[cfg(not(debug_assertions))]
    anyhow::bail!("BLUEY_JOBS_LOCAL_RUN_CAPABILITY_KEY is required")
}

fn accepted_keys() -> Vec<String> {
    let mut keys = Vec::new();
    if let Ok(key) = current_key() {
        keys.push(key);
    }
    if let Ok(previous) = std::env::var("BLUEY_JOBS_LOCAL_RUN_CAPABILITY_KEY_PREVIOUS") {
        if previous.len() >= 32 {
            keys.push(previous);
        }
    }
    keys
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    const KEY: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    #[serial]
    fn capability_is_bound_to_operation_run_and_expiry() {
        std::env::set_var("BLUEY_JOBS_LOCAL_RUN_CAPABILITY_KEY", KEY);
        let token = issue("acct", "app", "run", "profile", "result", 2_000).unwrap();
        let claims = verify(&token, "run", "result", 1_000).unwrap();
        assert_eq!(claims.account_id, "acct");
        assert_eq!(claims.browser_profile_id, "profile");
        assert!(verify(&token, "run", "resume", 1_000).is_err());
        assert!(verify(&token, "other-run", "result", 1_000).is_err());
        assert!(verify(&token, "run", "result", 2_000).is_err());

        let mut tampered = token.into_bytes();
        tampered[5] ^= 1;
        assert!(verify(
            &String::from_utf8(tampered).unwrap(),
            "run",
            "result",
            1_000
        )
        .is_err());
        std::env::remove_var("BLUEY_JOBS_LOCAL_RUN_CAPABILITY_KEY");
    }
}
