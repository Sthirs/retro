//! Rotating, single-use refresh tokens with reuse detection (ADR-0005).
//!
//! A refresh token is 32 random bytes (base64url). Only its SHA-256 is stored.
//! Every refresh revokes the presented token and issues a successor in the
//! same family. Presenting a revoked token again revokes the whole family,
//! except within a short grace window that absorbs two tabs refreshing at once.

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::Rng;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QuerySelect, Set,
    TransactionSession, TransactionTrait, sea_query::Expr,
};
use sha2::{Digest, Sha256};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use crate::{config::Config, entities::refresh_token, error::AppError};

/// A freshly issued refresh token, to be put in the cookie.
#[derive(Debug)]
pub struct Issued {
    pub token: String,
    pub user_id: Uuid,
    pub persistent: bool,
    pub expires_at: OffsetDateTime,
}

#[derive(Debug, PartialEq, Eq)]
enum Decision {
    Rotate,
    /// Already rotated moments ago (concurrent tabs): issue a sibling.
    GraceReuse,
    Expired,
    /// Replayed after the grace window: likely stolen.
    Reuse,
}

fn classify(token: &refresh_token::Model, now: OffsetDateTime, grace_seconds: i64) -> Decision {
    if token.expires_at <= now {
        return Decision::Expired;
    }
    match token.revoked_at {
        None => Decision::Rotate,
        Some(revoked)
            if token.replaced_by.is_some() && now - revoked <= Duration::seconds(grace_seconds) =>
        {
            Decision::GraceReuse
        }
        Some(_) => Decision::Reuse,
    }
}

pub fn hash_token(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn generate() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn ttl(config: &Config, persistent: bool) -> Duration {
    Duration::seconds(if persistent {
        config.refresh_token_ttl_seconds
    } else {
        config.session_refresh_ttl_seconds
    })
}

/// Starts a new token family for a fresh sign-in.
pub async fn issue(
    db: &impl ConnectionTrait,
    config: &Config,
    user_id: Uuid,
    persistent: bool,
) -> Result<Issued, AppError> {
    let (issued, _) = insert(db, config, user_id, Uuid::now_v7(), persistent).await?;
    Ok(issued)
}

async fn insert(
    db: &impl ConnectionTrait,
    config: &Config,
    user_id: Uuid,
    family_id: Uuid,
    persistent: bool,
) -> Result<(Issued, Uuid), AppError> {
    let token = generate();
    let now = OffsetDateTime::now_utc();
    let expires_at = now + ttl(config, persistent);
    let id = Uuid::now_v7();
    refresh_token::ActiveModel {
        id: Set(id),
        user_id: Set(user_id),
        family_id: Set(family_id),
        token_hash: Set(hash_token(&token)),
        persistent: Set(persistent),
        expires_at: Set(expires_at),
        revoked_at: Set(None),
        replaced_by: Set(None),
        created_at: Set(now),
    }
    .insert(db)
    .await?;
    Ok((
        Issued {
            token,
            user_id,
            persistent,
            expires_at,
        },
        id,
    ))
}

/// Exchanges a refresh token for its successor.
pub async fn rotate(
    db: &(impl TransactionTrait + ConnectionTrait),
    config: &Config,
    presented: &str,
) -> Result<Issued, AppError> {
    let txn = db.begin().await?;
    let Some(current) = refresh_token::Entity::find()
        .filter(refresh_token::Column::TokenHash.eq(hash_token(presented)))
        .lock_exclusive()
        .one(&txn)
        .await?
    else {
        return Err(AppError::Unauthorized);
    };

    let now = OffsetDateTime::now_utc();
    match classify(&current, now, config.refresh_reuse_grace_seconds) {
        Decision::Expired => Err(AppError::Unauthorized),
        Decision::Reuse => {
            revoke_family_in(&txn, current.family_id).await?;
            txn.commit().await?;
            tracing::warn!(user_id = %current.user_id, "refresh token reuse detected; family revoked");
            Err(AppError::Unauthorized)
        }
        Decision::GraceReuse => {
            let (issued, _) = insert(
                &txn,
                config,
                current.user_id,
                current.family_id,
                current.persistent,
            )
            .await?;
            txn.commit().await?;
            Ok(issued)
        }
        Decision::Rotate => {
            let (issued, new_id) = insert(
                &txn,
                config,
                current.user_id,
                current.family_id,
                current.persistent,
            )
            .await?;
            let mut active: refresh_token::ActiveModel = current.into();
            active.revoked_at = Set(Some(now));
            active.replaced_by = Set(Some(new_id));
            active.update(&txn).await?;
            txn.commit().await?;
            Ok(issued)
        }
    }
}

/// Revokes the family of `presented`, if it exists. Used by sign-out.
pub async fn revoke(db: &impl ConnectionTrait, presented: &str) -> Result<(), AppError> {
    if let Some(token) = refresh_token::Entity::find()
        .filter(refresh_token::Column::TokenHash.eq(hash_token(presented)))
        .one(db)
        .await?
    {
        revoke_family_in(db, token.family_id).await?;
    }
    Ok(())
}

async fn revoke_family_in(db: &impl ConnectionTrait, family_id: Uuid) -> Result<(), AppError> {
    refresh_token::Entity::update_many()
        .col_expr(
            refresh_token::Column::RevokedAt,
            Expr::value(OffsetDateTime::now_utc()),
        )
        .filter(refresh_token::Column::FamilyId.eq(family_id))
        .filter(refresh_token::Column::RevokedAt.is_null())
        .exec(db)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token(revoked_ago: Option<i64>, replaced: bool, expires_in: i64) -> refresh_token::Model {
        let now = OffsetDateTime::now_utc();
        refresh_token::Model {
            id: Uuid::now_v7(),
            user_id: Uuid::now_v7(),
            family_id: Uuid::now_v7(),
            token_hash: String::new(),
            persistent: true,
            expires_at: now + Duration::seconds(expires_in),
            revoked_at: revoked_ago.map(|s| now - Duration::seconds(s)),
            replaced_by: replaced.then(Uuid::now_v7),
            created_at: now,
        }
    }

    #[test]
    fn classifies_tokens() {
        let now = OffsetDateTime::now_utc();
        assert_eq!(classify(&token(None, false, 60), now, 10), Decision::Rotate);
        assert_eq!(
            classify(&token(None, false, -1), now, 10),
            Decision::Expired
        );
        assert_eq!(
            classify(&token(Some(2), true, 60), now, 10),
            Decision::GraceReuse
        );
        assert_eq!(
            classify(&token(Some(30), true, 60), now, 10),
            Decision::Reuse
        );
        // Revoked by sign-out (no successor): never accepted.
        assert_eq!(
            classify(&token(Some(1), false, 60), now, 10),
            Decision::Reuse
        );
    }

    #[test]
    fn generates_unique_url_safe_tokens() {
        let a = generate();
        let b = generate();
        assert_ne!(a, b);
        assert_eq!(a.len(), 43);
        assert!(
            a.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        );
    }

    #[test]
    fn hashes_are_stable_hex() {
        assert_eq!(
            hash_token("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
