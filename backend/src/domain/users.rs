//! User lookup, SSO account linking and local credentials.

use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, ConnectionTrait, EntityTrait, QueryFilter, Set,
    TransactionSession, TransactionTrait,
};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    auth::oidc::Identity,
    entities::{user, user_identity},
    error::AppError,
};

/// Finds a local account by username or email (case-insensitive).
pub async fn find_by_login(
    db: &impl ConnectionTrait,
    login: &str,
) -> Result<Option<user::Model>, AppError> {
    let login = login.trim().to_lowercase();
    if login.is_empty() {
        return Ok(None);
    }
    Ok(user::Entity::find()
        .filter(
            Condition::any()
                .add(user::Column::Email.eq(login.clone()))
                .add(user::Column::Username.eq(login)),
        )
        .one(db)
        .await?)
}

#[derive(Debug, PartialEq, Eq)]
pub enum SsoOutcome {
    Existing,
    Linked,
    Registered,
}

/// Resolves an SSO identity to a user, in one transaction:
/// known `(issuer, subject)` → that user; else a user with the same email gets
/// the identity linked; else a new user is registered when `auto_register`.
/// Name, email and picture are synced from the provider on every sign-in.
pub async fn sign_in_with_identity(
    db: &(impl TransactionTrait + ConnectionTrait),
    identity: &Identity,
    auto_register: bool,
) -> Result<Option<(user::Model, SsoOutcome)>, AppError> {
    let txn = db.begin().await?;
    let now = OffsetDateTime::now_utc();

    let linked = user_identity::Entity::find()
        .filter(user_identity::Column::Issuer.eq(identity.issuer.clone()))
        .filter(user_identity::Column::Subject.eq(identity.subject.clone()))
        .one(&txn)
        .await?;

    let (existing, outcome) = match linked {
        Some(link) => (
            user::Entity::find_by_id(link.user_id).one(&txn).await?,
            SsoOutcome::Existing,
        ),
        None => {
            let by_email = user::Entity::find()
                .filter(user::Column::Email.eq(identity.email.clone()))
                .one(&txn)
                .await?;
            (by_email, SsoOutcome::Linked)
        }
    };

    let (user, outcome) = match existing {
        Some(user) => {
            let mut active: user::ActiveModel = user.clone().into();
            // Keep the email in sync unless another account already owns it.
            if user.email != identity.email
                && user::Entity::find()
                    .filter(user::Column::Email.eq(identity.email.clone()))
                    .one(&txn)
                    .await?
                    .is_none()
            {
                active.email = Set(identity.email.clone());
            }
            if let Some(name) = &identity.name {
                active.display_name = Set(name.clone());
            }
            if identity.picture.is_some() {
                active.avatar_url = Set(identity.picture.clone());
            }
            active.updated_at = Set(now);
            (active.update(&txn).await?, outcome)
        }
        None if auto_register => {
            let user = user::ActiveModel {
                id: Set(Uuid::now_v7()),
                email: Set(identity.email.clone()),
                username: Set(None),
                display_name: Set(identity
                    .name
                    .clone()
                    .unwrap_or_else(|| identity.email.split('@').next().unwrap_or("").to_owned())),
                avatar_url: Set(identity.picture.clone()),
                password_hash: Set(None),
                created_at: Set(now),
                updated_at: Set(now),
            }
            .insert(&txn)
            .await?;
            (user, SsoOutcome::Registered)
        }
        None => return Ok(None),
    };

    if outcome != SsoOutcome::Existing {
        user_identity::ActiveModel {
            id: Set(Uuid::now_v7()),
            user_id: Set(user.id),
            issuer: Set(identity.issuer.clone()),
            subject: Set(identity.subject.clone()),
            created_at: Set(now),
        }
        .insert(&txn)
        .await?;
    }

    txn.commit().await?;
    Ok(Some((user, outcome)))
}
