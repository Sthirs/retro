//! WebSocket tickets (spec §3.3, ADR-0011): a 30-second HS256 token that
//! authenticates the WebSocket handshake, since browsers cannot send an
//! `Authorization` header there. Its issuer differs from access tokens', so
//! neither is accepted in place of the other.

use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const ISSUER: &str = "retro-ws";
/// How long a ticket can be used to open a socket.
pub const TICKET_TTL_SECONDS: i64 = 30;

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: Uuid,
    iss: String,
    exp: i64,
}

/// A ticket for `user_id`, and its expiry (unix seconds).
pub fn issue(secret: &str, user_id: Uuid) -> anyhow::Result<(String, i64)> {
    issue_at(
        secret,
        user_id,
        time::OffsetDateTime::now_utc().unix_timestamp(),
    )
}

fn issue_at(secret: &str, user_id: Uuid, now: i64) -> anyhow::Result<(String, i64)> {
    let exp = now + TICKET_TTL_SECONDS;
    let claims = Claims {
        sub: user_id,
        iss: ISSUER.into(),
        exp,
    };
    let token = encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )?;
    Ok((token, exp))
}

/// The user of a valid, unexpired ticket.
pub fn verify(secret: &str, ticket: &str) -> Option<Uuid> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_issuer(&[ISSUER]);
    validation.set_required_spec_claims(&["exp", "iss", "sub"]);
    validation.leeway = 0;
    decode::<Claims>(
        ticket,
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
        let (ticket, _) = issue(SECRET, id).unwrap();
        assert_eq!(verify(SECRET, &ticket), Some(id));
    }

    #[test]
    fn rejects_expired_tickets() {
        let now = time::OffsetDateTime::now_utc().unix_timestamp();
        let (ticket, _) = issue_at(SECRET, Uuid::now_v7(), now - 60).unwrap();
        assert_eq!(verify(SECRET, &ticket), None);
    }

    #[test]
    fn access_tokens_are_not_tickets() {
        let token = crate::auth::jwt::issue(SECRET, Uuid::now_v7(), 60).unwrap();
        assert_eq!(verify(SECRET, &token), None);
        let (ticket, _) = issue(SECRET, Uuid::now_v7()).unwrap();
        assert_eq!(crate::auth::jwt::verify(SECRET, &ticket), None);
    }
}
