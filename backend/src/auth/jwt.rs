//! Short-lived access tokens (HS256 JWT) issued by Retro.

use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const ISSUER: &str = "retro";

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Claims {
    pub sub: Uuid,
    pub iss: String,
    pub iat: i64,
    pub exp: i64,
}

pub fn issue(secret: &str, user_id: Uuid, ttl_seconds: i64) -> anyhow::Result<String> {
    issue_at(
        secret,
        user_id,
        ttl_seconds,
        time::OffsetDateTime::now_utc().unix_timestamp(),
    )
}

fn issue_at(secret: &str, user_id: Uuid, ttl_seconds: i64, now: i64) -> anyhow::Result<String> {
    let claims = Claims {
        sub: user_id,
        iss: ISSUER.into(),
        iat: now,
        exp: now + ttl_seconds,
    };
    Ok(encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )?)
}

/// Validates signature, issuer and expiry; returns the user id.
pub fn verify(secret: &str, token: &str) -> Option<Uuid> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_issuer(&[ISSUER]);
    validation.set_required_spec_claims(&["exp", "iss", "sub"]);
    validation.leeway = 5;
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .ok()
    .map(|data| data.claims.sub)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn round_trips() {
        let id = Uuid::now_v7();
        let token = issue(SECRET, id, 60).unwrap();
        assert_eq!(verify(SECRET, &token), Some(id));
    }

    #[test]
    fn rejects_wrong_secret() {
        let token = issue(SECRET, Uuid::now_v7(), 60).unwrap();
        assert_eq!(verify("another-secret-another-secret-0000", &token), None);
    }

    #[test]
    fn rejects_expired() {
        let now = time::OffsetDateTime::now_utc().unix_timestamp();
        let token = issue_at(SECRET, Uuid::now_v7(), 60, now - 3600).unwrap();
        assert_eq!(verify(SECRET, &token), None);
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(verify(SECRET, "not.a.jwt"), None);
    }
}
