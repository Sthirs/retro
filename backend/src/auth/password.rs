//! Argon2id password hashing.

use std::sync::LazyLock;

use argon2::{
    Argon2,
    password_hash::{PasswordHasher, PasswordVerifier, phc::PasswordHash},
};

/// Hash verified when the account does not exist or has no password, so that
/// the response time does not reveal which accounts exist.
static DUMMY_HASH: LazyLock<String> =
    LazyLock::new(|| hash("retro-dummy-password").expect("hashing a constant cannot fail"));

pub fn hash(password: &str) -> anyhow::Result<String> {
    Argon2::default()
        .hash_password(password.as_bytes())
        .map(|h| h.to_string())
        .map_err(|e| anyhow::anyhow!("password hashing failed: {e}"))
}

/// Verifies `password` against an optional PHC hash. Always does the work of
/// one verification, even when `hash` is `None`.
pub fn verify(password: &str, hash: Option<&str>) -> bool {
    let (phc, real) = match hash {
        Some(h) => (h, true),
        None => (DUMMY_HASH.as_str(), false),
    };
    let Ok(parsed) = PasswordHash::new(phc) else {
        return false;
    };
    let ok = Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok();
    ok && real
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_and_verifies() {
        let h = hash("password").unwrap();
        assert!(h.starts_with("$argon2id$"));
        assert!(verify("password", Some(&h)));
        assert!(!verify("Password", Some(&h)));
    }

    #[test]
    fn never_verifies_without_a_hash() {
        assert!(!verify("retro-dummy-password", None));
    }

    #[test]
    fn rejects_malformed_hash() {
        assert!(!verify("password", Some("not-a-phc-string")));
    }
}
