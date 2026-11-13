//! Argon2id password hashing.

use std::sync::LazyLock;

use argon2::{
    Argon2,
    password_hash::{PasswordHasher, PasswordVerifier, phc::PasswordHash},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::Rng;

/// Random password behind [`DUMMY_HASH`], generated once per process.
static DUMMY_PASSWORD: LazyLock<String> = LazyLock::new(random_password);

/// Hash verified when the account does not exist or has no password, so that
/// the response time does not reveal which accounts exist.
static DUMMY_HASH: LazyLock<String> =
    LazyLock::new(|| hash(&DUMMY_PASSWORD).expect("hashing a random password cannot fail"));

fn random_password() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

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

    // Random passwords: CodeQL flags literals passed to `hash`/`verify`.

    #[test]
    fn hashes_and_verifies() {
        let (password, other) = (random_password(), random_password());
        let h = hash(&password).unwrap();
        assert!(h.starts_with("$argon2id$"));
        assert!(verify(&password, Some(&h)));
        assert!(!verify(&other, Some(&h)));
    }

    #[test]
    fn never_verifies_without_a_hash() {
        assert!(!verify(&DUMMY_PASSWORD, None));
    }

    #[test]
    fn rejects_malformed_hash() {
        assert!(!verify(&random_password(), Some("not-a-phc-string")));
    }
}
