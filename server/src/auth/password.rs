//! Password hashing (bcrypt cost 12 — ~250ms on modern hardware).
//!
//! The cost factor balances login latency against brute-force resistance.
//! 12 is the default for most production systems in 2025; higher costs
//! help against offline attacks but make login feel sluggish.

use anyhow::Context;
use bcrypt::{hash, verify, DEFAULT_COST};

pub const BCRYPT_COST: u32 = DEFAULT_COST;

pub fn hash_password(password: &str) -> anyhow::Result<String> {
    if password.len() < 8 {
        anyhow::bail!("password must be at least 8 characters");
    }
    if password.len() > 72 {
        // bcrypt silently truncates after 72 bytes; we reject longer.
        anyhow::bail!("password must be at most 72 characters");
    }
    hash(password, BCRYPT_COST).context("bcrypt hash")
}

pub fn verify_password(password: &str, password_hash: &str) -> bool {
    verify(password, password_hash).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_then_verify_roundtrips() {
        let h = hash_password("correct-horse-battery-staple").unwrap();
        assert!(verify_password("correct-horse-battery-staple", &h));
        assert!(!verify_password("wrong-password-here", &h));
    }

    #[test]
    fn rejects_short_password() {
        assert!(hash_password("short").is_err());
    }

    #[test]
    fn rejects_too_long_password() {
        let long = "x".repeat(80);
        assert!(hash_password(&long).is_err());
    }

    #[test]
    fn each_hash_is_unique_via_salt() {
        let h1 = hash_password("same-password-12345").unwrap();
        let h2 = hash_password("same-password-12345").unwrap();
        assert_ne!(h1, h2); // bcrypt salts differ
        assert!(verify_password("same-password-12345", &h1));
        assert!(verify_password("same-password-12345", &h2));
    }
}
