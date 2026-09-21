//! JWT issuance + verification.
//!
//! HS256 (symmetric) for v0.2. Rotation strategy: secret stored as
//! `BLUEY_JWT_SECRET` env var; on rotation, both old + new secrets are
//! tried during verification (not yet implemented; single-secret today).
//!
//! Access tokens: 15 minutes TTL.
//! Refresh tokens: 30 days TTL, stored sha256-hashed in `refresh_tokens`
//! table so a DB leak does not expose live tokens.

use anyhow::{anyhow, Context, Result};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

pub const ACCESS_TTL_SECS: i64 = 15 * 60;
pub const REFRESH_TTL_SECS: i64 = 30 * 24 * 60 * 60;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    Access,
    Refresh,
}

impl TokenKind {
    fn as_str(&self) -> &'static str {
        match self {
            TokenKind::Access => "access",
            TokenKind::Refresh => "refresh",
        }
    }

    fn ttl_secs(&self) -> i64 {
        match self {
            TokenKind::Access => ACCESS_TTL_SECS,
            TokenKind::Refresh => REFRESH_TTL_SECS,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub iat: i64,
    pub exp: i64,
    pub kind: String,
    /// Per-token nonce. Optional while access/refresh tokens issued by older
    /// Bluey versions remain valid during a rolling upgrade.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jti: Option<String>,
}

impl Claims {
    pub fn new(account_id: &str, kind: TokenKind) -> Self {
        let now = Utc::now();
        let exp = now + Duration::seconds(kind.ttl_secs());
        Self {
            sub: account_id.to_string(),
            iat: now.timestamp(),
            exp: exp.timestamp(),
            kind: kind.as_str().to_string(),
            jti: Some(uuid::Uuid::new_v4().to_string()),
        }
    }

    pub fn token_kind(&self) -> Result<TokenKind> {
        match self.kind.as_str() {
            "access" => Ok(TokenKind::Access),
            "refresh" => Ok(TokenKind::Refresh),
            other => Err(anyhow!("unknown token kind: {other}")),
        }
    }
}

/// Issue a JWT for the given account + kind.
pub fn issue(secret: &str, account_id: &str, kind: TokenKind) -> Result<String> {
    let claims = Claims::new(account_id, kind);
    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .context("encode JWT")?;
    Ok(token)
}

/// Verify a JWT. Returns the Claims on success.
pub fn verify(secret: &str, token: &str) -> Result<Claims> {
    let mut validation = Validation::default();
    validation.leeway = 30; // 30s clock skew tolerance
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .context("decode JWT")?;
    Ok(data.claims)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SECRET: &str = "test-secret-at-least-32-characters-long-yes";

    #[test]
    fn issue_and_verify_access_token() {
        let token = issue(TEST_SECRET, "acct-1", TokenKind::Access).unwrap();
        let claims = verify(TEST_SECRET, &token).unwrap();
        assert_eq!(claims.sub, "acct-1");
        assert_eq!(claims.kind, "access");
        assert_eq!(claims.token_kind().unwrap(), TokenKind::Access);
        // exp should be ~15 min in the future.
        let now = Utc::now().timestamp();
        assert!(claims.exp > now);
        assert!(claims.exp - now <= ACCESS_TTL_SECS + 5);
    }

    #[test]
    fn issue_and_verify_refresh_token() {
        let token = issue(TEST_SECRET, "acct-2", TokenKind::Refresh).unwrap();
        let claims = verify(TEST_SECRET, &token).unwrap();
        assert_eq!(claims.kind, "refresh");
        assert!(claims
            .jti
            .as_deref()
            .is_some_and(|value| uuid::Uuid::parse_str(value).is_ok()));
        let now = Utc::now().timestamp();
        assert!(claims.exp - now > ACCESS_TTL_SECS); // longer than access
    }

    #[test]
    fn independently_issued_tokens_are_unique_within_the_same_second() {
        let first = issue(TEST_SECRET, "acct-unique", TokenKind::Refresh).unwrap();
        let second = issue(TEST_SECRET, "acct-unique", TokenKind::Refresh).unwrap();
        assert_ne!(first, second);
        assert_ne!(
            verify(TEST_SECRET, &first).unwrap().jti,
            verify(TEST_SECRET, &second).unwrap().jti
        );
    }

    #[test]
    fn verify_accepts_tokens_issued_before_jti_was_added() {
        #[derive(Serialize)]
        struct LegacyClaims<'a> {
            sub: &'a str,
            iat: i64,
            exp: i64,
            kind: &'a str,
        }

        let now = Utc::now().timestamp();
        let token = encode(
            &Header::default(),
            &LegacyClaims {
                sub: "acct-legacy",
                iat: now,
                exp: now + ACCESS_TTL_SECS,
                kind: "access",
            },
            &EncodingKey::from_secret(TEST_SECRET.as_bytes()),
        )
        .unwrap();
        let claims = verify(TEST_SECRET, &token).unwrap();
        assert_eq!(claims.sub, "acct-legacy");
        assert_eq!(claims.jti, None);
    }

    #[test]
    fn verify_rejects_wrong_secret() {
        let token = issue(TEST_SECRET, "acct-1", TokenKind::Access).unwrap();
        let result = verify("a-different-secret-also-32-chars-long-x", &token);
        assert!(result.is_err());
    }

    #[test]
    fn verify_rejects_garbage() {
        let result = verify(TEST_SECRET, "not.a.real.jwt");
        assert!(result.is_err());
    }
}
