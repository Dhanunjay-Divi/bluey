//! Operation-scoped capabilities for the local Bluey Jobs browser.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use subtle::ConstantTimeEq;

type HmacSha256 = Hmac<Sha256>;

pub(super) const TOKEN_VERSION: u8 = 2;
pub(super) const LEGACY_TOKEN_VERSION: u8 = 1;
const TOKEN_AUDIENCE: &str = "bluey-jobs-local-run";
pub const RECONCILIATION_GRACE_MS: i64 = crate::db::jobs::SUBMISSION_RECONCILIATION_GRACE_MS;

pub fn validate_runtime_config() -> anyhow::Result<()> {
    let local_distribution_enabled = std::env::var("BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED")
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes"
            )
        })
        .unwrap_or(false);
    if local_distribution_enabled {
        current_key().map(|_| ())?;
        crate::db::jobs::validate_browser_release_runtime_config()?;
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release: Option<LocalRunReleaseClaims>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LocalRunReleaseClaims {
    pub descriptor_sha256: String,
    pub manifest_sha256: String,
    pub activation_sha256: String,
    pub artifact_id: String,
    pub artifact_sha256: String,
    pub release_id: String,
    pub build_id: String,
    pub app_version: String,
    pub protocol_version: i64,
    pub platform: String,
    pub architecture: String,
    pub channel: String,
    pub trust_generation: i64,
    pub activation_generation: i64,
    pub channel_sequence: i64,
}

pub fn issue(
    account_id: &str,
    application_id: &str,
    run_id: &str,
    browser_profile_id: &str,
    operation: &str,
    expires_at_ms: i64,
    release: &LocalRunReleaseClaims,
) -> anyhow::Result<String> {
    validate_operation(operation)?;
    if [account_id, application_id, run_id, browser_profile_id]
        .iter()
        .any(|value| value.trim().is_empty() || value.len() > 200)
    {
        anyhow::bail!("invalid local run capability binding");
    }
    validate_release(release)?;
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
        release: Some(release.clone()),
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
    verify_with_late_grace(token, expected_run_id, expected_operation, now_ms, 0)
}

pub fn verify_for_reconciliation(
    token: &str,
    expected_run_id: &str,
    expected_operation: &str,
    now_ms: i64,
) -> anyhow::Result<LocalRunCapabilityClaims> {
    verify_with_late_grace(
        token,
        expected_run_id,
        expected_operation,
        now_ms,
        RECONCILIATION_GRACE_MS,
    )
}

fn verify_with_late_grace(
    token: &str,
    expected_run_id: &str,
    expected_operation: &str,
    now_ms: i64,
    late_grace_ms: i64,
) -> anyhow::Result<LocalRunCapabilityClaims> {
    validate_operation(expected_operation)?;
    if late_grace_ms > 0 && expected_operation != "result" {
        anyhow::bail!("invalid local run capability");
    }
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
    if claims.audience != TOKEN_AUDIENCE
        || claims.run_id != expected_run_id
        || claims.operation != expected_operation
        || claims.expires_at_ms.saturating_add(late_grace_ms) <= now_ms
        || claims.nonce.len() < 24
    {
        anyhow::bail!("invalid local run capability");
    }
    match claims.version {
        TOKEN_VERSION => validate_release(
            claims
                .release
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("invalid local run capability"))?,
        )?,
        LEGACY_TOKEN_VERSION
            if claims.release.is_none() && matches!(expected_operation, "result" | "resume") => {}
        _ => anyhow::bail!("invalid local run capability"),
    }
    Ok(claims)
}

fn validate_release(release: &LocalRunReleaseClaims) -> anyhow::Result<()> {
    let valid_sha256 = |value: &str| {
        value.len() == 64
            && value == value.to_ascii_lowercase()
            && value.bytes().all(|byte| byte.is_ascii_hexdigit())
    };
    let valid_binding = |value: &str| {
        !value.is_empty()
            && value.len() <= 200
            && value.trim() == value
            && !value.chars().any(char::is_control)
    };
    if !valid_sha256(&release.descriptor_sha256)
        || !valid_sha256(&release.manifest_sha256)
        || !valid_sha256(&release.activation_sha256)
        || !valid_sha256(&release.artifact_sha256)
        || [
            &release.artifact_id,
            &release.release_id,
            &release.build_id,
            &release.app_version,
        ]
        .iter()
        .any(|value| !valid_binding(value))
        || release.protocol_version < 1
        || !matches!(release.platform.as_str(), "darwin" | "windows")
        || !matches!(release.architecture.as_str(), "arm64" | "x64")
        || (release.platform == "windows" && release.architecture != "x64")
        || !matches!(release.channel.as_str(), "internal" | "beta" | "stable")
        || release.trust_generation < 1
        || release.activation_generation < 1
        || release.channel_sequence < 1
    {
        anyhow::bail!("invalid local run release binding")
    }
    Ok(())
}

fn validate_operation(operation: &str) -> anyhow::Result<()> {
    if matches!(operation, "result" | "resume" | "submit") {
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
        let release = release();
        let token = issue("acct", "app", "run", "profile", "result", 2_000, &release).unwrap();
        let claims = verify(&token, "run", "result", 1_000).unwrap();
        assert_eq!(claims.account_id, "acct");
        assert_eq!(claims.browser_profile_id, "profile");
        assert_eq!(claims.release, Some(release.clone()));
        assert!(verify(&token, "run", "resume", 1_000).is_err());
        assert!(verify(&token, "run", "submit", 1_000).is_err());
        assert!(verify(&token, "other-run", "result", 1_000).is_err());

        let submit = issue("acct", "app", "run", "profile", "submit", 2_000, &release).unwrap();
        assert_eq!(
            verify(&submit, "run", "submit", 1_000).unwrap().operation,
            "submit"
        );
        assert!(verify(&submit, "run", "result", 1_000).is_err());
        assert!(verify(&token, "run", "result", 2_000).is_err());
        assert!(verify_for_reconciliation(&token, "run", "result", 2_000).is_ok());
        assert!(verify_for_reconciliation(
            &token,
            "run",
            "result",
            2_000 + RECONCILIATION_GRACE_MS
        )
        .is_err());

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

    #[test]
    #[serial]
    fn legacy_capability_is_recovery_only_and_keeps_result_reconciliation_grace() {
        std::env::set_var("BLUEY_JOBS_LOCAL_RUN_CAPABILITY_KEY", KEY);
        let result = issue_legacy_v1("result", 2_000);
        let claims = verify(&result, "run", "result", 1_000).unwrap();
        assert_eq!(claims.version, LEGACY_TOKEN_VERSION);
        assert!(claims.release.is_none());
        assert!(verify_for_reconciliation(&result, "run", "result", 2_000).is_ok());
        assert!(verify_for_reconciliation(
            &result,
            "run",
            "result",
            2_000 + RECONCILIATION_GRACE_MS,
        )
        .is_err());

        let resume = issue_legacy_v1("resume", 2_000);
        assert!(verify(&resume, "run", "resume", 1_000).is_ok());
        assert!(verify_for_reconciliation(&resume, "run", "resume", 1_000).is_err());

        let submit = issue_legacy_v1("submit", 2_000);
        assert!(verify(&submit, "run", "submit", 1_000).is_err());
        std::env::remove_var("BLUEY_JOBS_LOCAL_RUN_CAPABILITY_KEY");
    }

    fn issue_legacy_v1(operation: &str, expires_at_ms: i64) -> String {
        let claims = serde_json::json!({
            "version": LEGACY_TOKEN_VERSION,
            "audience": TOKEN_AUDIENCE,
            "account_id": "acct",
            "application_id": "app",
            "run_id": "run",
            "browser_profile_id": "profile",
            "operation": operation,
            "expires_at_ms": expires_at_ms,
            "nonce": "0123456789abcdef0123456789abcdef",
        });
        let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).unwrap());
        let signature = sign(KEY, &payload);
        format!("{payload}.{signature}")
    }

    fn release() -> LocalRunReleaseClaims {
        LocalRunReleaseClaims {
            descriptor_sha256: "a".repeat(64),
            manifest_sha256: "b".repeat(64),
            activation_sha256: "c".repeat(64),
            artifact_id: "browser-artifact-1".to_string(),
            artifact_sha256: "d".repeat(64),
            release_id: "browser-release-603-1".to_string(),
            build_id: "browser-603.1".to_string(),
            app_version: "0.1.0".to_string(),
            protocol_version: 1,
            platform: "darwin".to_string(),
            architecture: "arm64".to_string(),
            channel: "beta".to_string(),
            trust_generation: 1,
            activation_generation: 1,
            channel_sequence: 1,
        }
    }
}
