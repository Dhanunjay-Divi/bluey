//! RFC 7636 PKCE (S256) — pure, no I/O.
//!
//! The native public-client OAuth flow (RFC 8252) uses PKCE with the S256
//! method instead of a client secret: we generate a high-entropy
//! `code_verifier`, derive `code_challenge = base64url_nopad(sha256(verifier))`,
//! send the challenge on the authorize request, and prove possession of the
//! verifier on the token exchange.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use sha2::{Digest, Sha256};

/// A generated PKCE pair: the secret `verifier` we keep, and the `challenge`
/// (its S256 hash) we put on the authorize URL.
#[derive(Debug, Clone)]
pub struct Pkce {
    /// The high-entropy secret proven on token exchange (43 base64url chars).
    pub verifier: String,
    /// `base64url_nopad(sha256(verifier))` — sent on the authorize request.
    pub challenge: String,
}

impl Pkce {
    /// Generate a fresh PKCE pair from 32 bytes of OS entropy. The verifier is
    /// the base64url-nopad encoding of the raw bytes (43 chars, all in the
    /// RFC 7636 unreserved set), and the challenge is its SHA-256 digest,
    /// likewise base64url-nopad.
    pub fn generate() -> anyhow::Result<Self> {
        let mut buf = [0u8; 32];
        getrandom::getrandom(&mut buf)
            .map_err(|error| anyhow::anyhow!("getrandom for PKCE verifier failed: {error}"))?;
        let verifier = URL_SAFE_NO_PAD.encode(buf);
        let challenge = challenge_for(&verifier);
        Ok(Self {
            verifier,
            challenge,
        })
    }
}

/// Derive the S256 code challenge for a given verifier:
/// `base64url_nopad(sha256(ascii(verifier)))`.
pub fn challenge_for(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

/// A random, URL-safe `state` value (CSRF guard) from 32 bytes of OS entropy.
pub fn random_state() -> anyhow::Result<String> {
    let mut buf = [0u8; 32];
    getrandom::getrandom(&mut buf)
        .map_err(|error| anyhow::anyhow!("getrandom for OAuth state failed: {error}"))?;
    Ok(URL_SAFE_NO_PAD.encode(buf))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn s256_challenge_matches_rfc7636_derivation() {
        // A fixed verifier with a pre-computed expected challenge. The expected
        // value is base64url-nopad(sha256(verifier)). This is the RFC 7636 S256
        // correctness anchor — if the hashing or encoding ever changes, this
        // fails.
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let challenge = challenge_for(verifier);

        // Recompute the expectation independently (same primitives) to assert
        // the exact string, guarding both the SHA-256 input encoding and the
        // base64url-nopad output alphabet.
        let expected = {
            let digest = Sha256::digest(verifier.as_bytes());
            URL_SAFE_NO_PAD.encode(digest)
        };
        assert_eq!(challenge, expected);
        // The RFC 7636 Appendix B example uses this exact verifier and expects
        // this exact challenge.
        assert_eq!(challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn generate_produces_valid_lengths_and_derivation() {
        let pkce = Pkce::generate().expect("generate");
        // 32 raw bytes → base64url-nopad → 43 chars.
        assert_eq!(pkce.verifier.len(), 43);
        // Verifier must be in the RFC 7636 unreserved set (base64url alphabet).
        assert!(pkce
            .verifier
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
        // Challenge must be the S256 derivation of the verifier.
        assert_eq!(pkce.challenge, challenge_for(&pkce.verifier));
        // sha256 digest is 32 bytes → base64url-nopad → 43 chars.
        assert_eq!(pkce.challenge.len(), 43);
    }

    #[test]
    fn random_state_is_urlsafe_and_unique() {
        let a = random_state().expect("state a");
        let b = random_state().expect("state b");
        assert_ne!(a, b, "state must be random per call");
        assert!(a
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    }
}
