//! Business rules, independent of HTTP.

pub mod invitations;
pub mod teams;
pub mod users;

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::Rng;

/// 32 random bytes, base64url without padding: an unguessable token for
/// refresh tokens, invitation links and join links.
pub fn random_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_tokens_are_url_safe_and_distinct() {
        let a = random_token();
        let b = random_token();
        assert_ne!(a, b);
        assert_eq!(a.len(), 43);
        assert!(
            a.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        );
    }
}
