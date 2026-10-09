//! Verification for narrowly scoped Pinky-to-Bluey request delegations.
//!
//! This module deliberately only verifies credentials.  It does not issue
//! delegations or decide whether the delegated subject owns a Bluey account.

use std::fmt;

use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use uuid::Uuid;

const MAX_TOKEN_BYTES: usize = 4 * 1024;
const MAX_BODY_BYTES: usize = 16 * 1024;
const MAX_EXPECTED_VALUE_BYTES: usize = 128;
const MAX_SUBJECT_BYTES: usize = 128;
const MAX_LIFETIME_SECONDS: i64 = 60;
const MAX_FUTURE_IAT_SECONDS: i64 = 5;

/// The fixed verifier configuration for one Pinky integration environment.
#[derive(Clone)]
pub struct DelegationConfig {
    secret: String,
    pub issuer: String,
    pub audience: String,
    pub environment: String,
}

impl fmt::Debug for DelegationConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DelegationConfig")
            .field("secret", &"[REDACTED]")
            .field("issuer", &self.issuer)
            .field("audience", &self.audience)
            .field("environment", &self.environment)
            .finish()
    }
}

impl DelegationConfig {
    pub fn new(
        secret: String,
        issuer: String,
        audience: String,
        environment: String,
    ) -> Result<Self, DelegationError> {
        if secret.len() < 32
            || !is_expected_value(&issuer)
            || !is_expected_value(&audience)
            || !is_expected_value(&environment)
        {
            return Err(DelegationError::InvalidConfiguration);
        }

        Ok(Self {
            secret,
            issuer,
            audience,
            environment,
        })
    }
}

/// Signed delegation fields accepted at the Pinky integration boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DelegationClaims {
    pub iss: String,
    pub aud: String,
    pub sub: String,
    pub env: String,
    pub scope: String,
    pub iat: i64,
    pub exp: i64,
    pub jti: String,
    pub method: String,
    pub path: String,
    pub body_sha256: String,
}

/// A deliberately content-free failure returned by delegation verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DelegationError {
    #[error("invalid delegation configuration")]
    InvalidConfiguration,
    #[error("invalid delegation token")]
    InvalidToken,
    #[error("invalid delegated request")]
    InvalidRequest,
}

/// Verify a one-minute, request-bound Pinky delegation.
pub fn verify(
    token: &str,
    config: &DelegationConfig,
    method: &str,
    path: &str,
    body: &[u8],
    now: i64,
) -> Result<DelegationClaims, DelegationError> {
    if token.is_empty() || token.len() > MAX_TOKEN_BYTES || body.len() > MAX_BODY_BYTES {
        return Err(DelegationError::InvalidRequest);
    }
    if !is_allowed_request(method, path) {
        return Err(DelegationError::InvalidRequest);
    }

    let mut validation = Validation::new(Algorithm::HS256);
    validation.leeway = 0;
    validation.set_issuer(&[&config.issuer]);
    validation.set_audience(&[&config.audience]);
    validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);

    let claims = decode::<DelegationClaims>(
        token,
        &DecodingKey::from_secret(config.secret.as_bytes()),
        &validation,
    )
    .map_err(|_| DelegationError::InvalidToken)?
    .claims;

    validate_claims(&claims, config, method, path, body, now)?;
    Ok(claims)
}

fn validate_claims(
    claims: &DelegationClaims,
    config: &DelegationConfig,
    method: &str,
    path: &str,
    body: &[u8],
    now: i64,
) -> Result<(), DelegationError> {
    if claims.iss != config.issuer
        || claims.aud != config.audience
        || claims.env != config.environment
        || request_scope(&claims.method, &claims.path) != Some(claims.scope.as_str())
        || !is_safe_subject(&claims.sub)
        || !is_canonical_uuid(&claims.jti)
        || claims.method != method
        || claims.path != path
        || !is_allowed_request(&claims.method, &claims.path)
        || !is_lower_hex_digest(&claims.body_sha256)
    {
        return Err(DelegationError::InvalidToken);
    }

    let lifetime = claims
        .exp
        .checked_sub(claims.iat)
        .ok_or(DelegationError::InvalidToken)?;
    let latest_iat = now
        .checked_add(MAX_FUTURE_IAT_SECONDS)
        .ok_or(DelegationError::InvalidToken)?;
    if lifetime <= 0
        || lifetime > MAX_LIFETIME_SECONDS
        || claims.exp <= now
        || claims.iat > latest_iat
    {
        return Err(DelegationError::InvalidToken);
    }

    let actual_digest = Sha256::digest(body);
    let expected_digest = hex::encode(actual_digest);
    if expected_digest
        .as_bytes()
        .ct_eq(claims.body_sha256.as_bytes())
        .unwrap_u8()
        != 1
    {
        return Err(DelegationError::InvalidToken);
    }

    Ok(())
}

fn is_expected_value(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_EXPECTED_VALUE_BYTES
        && value
            .as_bytes()
            .iter()
            .all(|byte| (b'!'..=b'~').contains(byte))
}

fn is_safe_subject(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_SUBJECT_BYTES
        && value
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(*byte, b'_' | b'-' | b'.' | b':'))
}

fn is_canonical_uuid(value: &str) -> bool {
    Uuid::parse_str(value)
        .map(|uuid| !uuid.is_nil() && uuid.hyphenated().to_string() == value)
        .unwrap_or(false)
}

fn is_lower_hex_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .as_bytes()
            .iter()
            .all(|byte| matches!(*byte, b'0'..=b'9' | b'a'..=b'f'))
}

fn is_allowed_request(method: &str, path: &str) -> bool {
    request_scope(method, path).is_some()
}

fn request_scope(method: &str, path: &str) -> Option<&'static str> {
    if method != "POST" {
        return None;
    }
    match path {
        "/integrations/pinky/sessions" | "/integrations/pinky/sessions/close" => Some("ai:session"),
        "/integrations/pinky/ask/stream" => Some("ai:ask"),
        "/integrations/pinky/asks/cancel" => Some("ai:cancel"),
        "/integrations/pinky/asks/status" => Some("ai:status"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use jsonwebtoken::{encode, EncodingKey, Header};

    use super::*;

    const SECRET: &str = "a-test-secret-that-is-at-least-thirty-two-bytes";
    const NOW: i64 = 1_900_000_000;

    #[test]
    fn verifies_exact_pinky_go_lifecycle_golden_vector() {
        // Identical synthetic vector in Pinky's assistdelegation Go tests.
        let config = DelegationConfig::new(
            "synthetic-test-secret-at-least-thirty-two-bytes".into(),
            "pinky-preprod".into(),
            "bluey-assist".into(),
            "preprod".into(),
        )
        .unwrap();
        let body = br#"{"session_id":"123e4567-e89b-12d3-a456-426614174000"}"#;
        let token = concat!(
            "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJwaW5reS1wcmVwcm9kIiwiYXVkIjoiYmx",
            "1ZXktYXNzaXN0Iiwic3ViIjoicGlua3lfdXNlci0xIiwiZW52IjoicHJlcHJvZCIsInNjb3BlIjoiYWk",
            "6c2Vzc2lvbiIsImlhdCI6MTkwMDAwMDAwMCwiZXhwIjoxOTAwMDAwMDYwLCJqdGkiOiIxMjNlNDU2Ny1",
            "lODliLTEyZDMtYTQ1Ni00MjY2MTQxNzQwMDAiLCJtZXRob2QiOiJQT1NUIiwicGF0aCI6Ii9pbnRlZ3J",
            "hdGlvbnMvcGlua3kvc2Vzc2lvbnMiLCJib2R5X3NoYTI1NiI6IjY0OTZkN2ZiYjBkNmNhMjUzZDUzMzV",
            "jMTkwYmE0OTMyNTRhZGNhZGE3N2I1NDFkMmIyZWNjNzgyMjEyNWJmOWYifQ.bMaEpw_AR485QhGSIRWj",
            "VP-7QViFjbJRFpDD3RNN5JU",
        );
        let claims = verify(
            token,
            &config,
            "POST",
            "/integrations/pinky/sessions",
            body,
            NOW,
        )
        .unwrap();
        assert_eq!(claims.sub, "pinky_user-1");
        assert_eq!(claims.exp, NOW + 60);
        assert!(verify(
            token,
            &config,
            "POST",
            "/integrations/pinky/sessions/close",
            body,
            NOW
        )
        .is_err());
    }

    fn config() -> DelegationConfig {
        DelegationConfig::new(
            SECRET.into(),
            "pinky-preprod".into(),
            "bluey-assist".into(),
            "preprod".into(),
        )
        .unwrap()
    }

    fn claims(body: &[u8]) -> DelegationClaims {
        DelegationClaims {
            iss: "pinky-preprod".into(),
            aud: "bluey-assist".into(),
            sub: "pinky_user-1".into(),
            env: "preprod".into(),
            scope: "ai:session".into(),
            iat: NOW - 1,
            exp: NOW + 59,
            jti: "123e4567-e89b-12d3-a456-426614174000".into(),
            method: "POST".into(),
            path: "/integrations/pinky/sessions".into(),
            body_sha256: hex::encode(Sha256::digest(body)),
        }
    }

    fn token(claims: &DelegationClaims) -> String {
        encode(
            &Header::new(Algorithm::HS256),
            claims,
            &EncodingKey::from_secret(SECRET.as_bytes()),
        )
        .unwrap()
    }

    #[test]
    fn verifies_a_request_bound_delegation() {
        let body = br#"{\"source\":\"mic\"}"#;
        assert_eq!(
            verify(
                &token(&claims(body)),
                &config(),
                "POST",
                "/integrations/pinky/sessions",
                body,
                NOW
            )
            .unwrap()
            .sub,
            "pinky_user-1"
        );
    }

    #[test]
    fn rejects_signature_algorithm_and_standard_claim_mismatches() {
        let body = b"{}";
        let valid_claims = claims(body);
        let wrong_secret = encode(
            &Header::new(Algorithm::HS256),
            &valid_claims,
            &EncodingKey::from_secret(b"another-secret-that-is-at-least-32-bytes"),
        )
        .unwrap();
        assert_eq!(
            verify(
                &wrong_secret,
                &config(),
                "POST",
                "/integrations/pinky/sessions",
                body,
                NOW
            ),
            Err(DelegationError::InvalidToken)
        );

        let wrong_algorithm = encode(
            &Header::new(Algorithm::HS384),
            &valid_claims,
            &EncodingKey::from_secret(SECRET.as_bytes()),
        )
        .unwrap();
        assert_eq!(
            verify(
                &wrong_algorithm,
                &config(),
                "POST",
                "/integrations/pinky/sessions",
                body,
                NOW
            ),
            Err(DelegationError::InvalidToken)
        );

        let mut changed = valid_claims.clone();
        changed.iss = "other".into();
        assert_eq!(
            verify(
                &token(&changed),
                &config(),
                "POST",
                "/integrations/pinky/sessions",
                body,
                NOW
            ),
            Err(DelegationError::InvalidToken)
        );
        changed = valid_claims.clone();
        changed.aud = "other".into();
        assert_eq!(
            verify(
                &token(&changed),
                &config(),
                "POST",
                "/integrations/pinky/sessions",
                body,
                NOW
            ),
            Err(DelegationError::InvalidToken)
        );
        changed = valid_claims.clone();
        changed.env = "other".into();
        assert_eq!(
            verify(
                &token(&changed),
                &config(),
                "POST",
                "/integrations/pinky/sessions",
                body,
                NOW
            ),
            Err(DelegationError::InvalidToken)
        );
        changed = valid_claims.clone();
        changed.scope = "ai:admin".into();
        assert_eq!(
            verify(
                &token(&changed),
                &config(),
                "POST",
                "/integrations/pinky/sessions",
                body,
                NOW
            ),
            Err(DelegationError::InvalidToken)
        );
    }

    #[test]
    fn rejects_wrong_request_body_and_routes() {
        let body = b"{}";
        let signed = token(&claims(body));
        assert!(verify(
            &signed,
            &config(),
            "POST",
            "/integrations/pinky/sessions",
            b"other",
            NOW
        )
        .is_err());
        assert!(verify(
            &signed,
            &config(),
            "GET",
            "/integrations/pinky/sessions",
            body,
            NOW
        )
        .is_err());
        assert!(verify(
            &signed,
            &config(),
            "POST",
            "/integrations/pinky/sessions?x=1",
            body,
            NOW
        )
        .is_err());
        assert!(verify(
            &signed,
            &config(),
            "POST",
            "/integrations/pinky/not-a-route",
            body,
            NOW
        )
        .is_err());
    }

    #[test]
    fn integration_operations_require_distinct_scopes() {
        let body = b"{}";
        for (path, scope) in [
            ("/integrations/pinky/ask/stream", "ai:ask"),
            ("/integrations/pinky/asks/cancel", "ai:cancel"),
            ("/integrations/pinky/asks/status", "ai:status"),
        ] {
            let mut delegated = claims(body);
            delegated.path = path.into();
            delegated.scope = scope.into();
            assert!(verify(&token(&delegated), &config(), "POST", path, body, NOW).is_ok());
            delegated.scope = "ai:session".into();
            assert!(verify(&token(&delegated), &config(), "POST", path, body, NOW).is_err());
        }
    }

    #[test]
    fn rejects_bad_ids_times_and_size_limits() {
        let body = b"{}";
        let mut changed = claims(body);
        changed.sub = "bad subject".into();
        assert!(verify(
            &token(&changed),
            &config(),
            "POST",
            "/integrations/pinky/sessions",
            body,
            NOW
        )
        .is_err());
        changed = claims(body);
        changed.jti = "123E4567-E89B-12D3-A456-426614174000".into();
        assert!(verify(
            &token(&changed),
            &config(),
            "POST",
            "/integrations/pinky/sessions",
            body,
            NOW
        )
        .is_err());
        changed = claims(body);
        changed.jti = Uuid::nil().hyphenated().to_string();
        assert!(verify(
            &token(&changed),
            &config(),
            "POST",
            "/integrations/pinky/sessions",
            body,
            NOW
        )
        .is_err());
        changed = claims(body);
        changed.body_sha256 = "g".repeat(64);
        assert!(verify(
            &token(&changed),
            &config(),
            "POST",
            "/integrations/pinky/sessions",
            body,
            NOW
        )
        .is_err());
        changed = claims(body);
        changed.iat = NOW + 6;
        changed.exp = NOW + 7;
        assert!(verify(
            &token(&changed),
            &config(),
            "POST",
            "/integrations/pinky/sessions",
            body,
            NOW
        )
        .is_err());
        changed = claims(body);
        changed.exp = changed.iat;
        assert!(verify(
            &token(&changed),
            &config(),
            "POST",
            "/integrations/pinky/sessions",
            body,
            NOW
        )
        .is_err());
        changed = claims(body);
        changed.iat = i64::MIN;
        changed.exp = i64::MAX;
        assert!(verify(
            &token(&changed),
            &config(),
            "POST",
            "/integrations/pinky/sessions",
            body,
            NOW
        )
        .is_err());
        changed = claims(body);
        changed.exp = NOW;
        assert!(verify(
            &token(&changed),
            &config(),
            "POST",
            "/integrations/pinky/sessions",
            body,
            NOW
        )
        .is_err());
        assert!(verify(
            "",
            &config(),
            "POST",
            "/integrations/pinky/sessions",
            body,
            NOW
        )
        .is_err());
        assert!(verify(
            &"a".repeat(MAX_TOKEN_BYTES + 1),
            &config(),
            "POST",
            "/integrations/pinky/sessions",
            body,
            NOW
        )
        .is_err());
        assert!(verify(
            &token(&claims(body)),
            &config(),
            "POST",
            "/integrations/pinky/sessions",
            &vec![0; MAX_BODY_BYTES + 1],
            NOW
        )
        .is_err());
    }

    #[test]
    fn rejects_extra_claims_and_redacts_secret() {
        let body = b"{}";
        let extra = format!(
            "{{\"iss\":\"pinky-preprod\",\"aud\":\"bluey-assist\",\"sub\":\"pinky_user-1\",\"env\":\"preprod\",\"scope\":\"ai:session\",\"iat\":{},\"exp\":{},\"jti\":\"123e4567-e89b-12d3-a456-426614174000\",\"method\":\"POST\",\"path\":\"/integrations/pinky/sessions\",\"body_sha256\":\"{}\",\"extra\":true}}",
            NOW - 1,
            NOW + 59,
            hex::encode(Sha256::digest(body))
        );
        let extra_token = encode(
            &Header::new(Algorithm::HS256),
            &serde_json::from_str::<serde_json::Value>(&extra).unwrap(),
            &EncodingKey::from_secret(SECRET.as_bytes()),
        )
        .unwrap();
        assert!(verify(
            &extra_token,
            &config(),
            "POST",
            "/integrations/pinky/sessions",
            body,
            NOW
        )
        .is_err());
        assert!(!format!("{:?}", config()).contains(SECRET));
    }

    #[test]
    fn rejects_invalid_configuration_values() {
        assert!(matches!(
            DelegationConfig::new(
                "short".into(),
                "issuer".into(),
                "audience".into(),
                "preprod".into()
            ),
            Err(DelegationError::InvalidConfiguration)
        ));
        assert!(matches!(
            DelegationConfig::new(
                SECRET.into(),
                "issuer with whitespace".into(),
                "audience".into(),
                "preprod".into()
            ),
            Err(DelegationError::InvalidConfiguration)
        ));
    }
}
