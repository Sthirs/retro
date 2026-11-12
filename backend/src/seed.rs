//! Development seed data. Only runs when `SEED_DEV_USERS=true`, which the
//! configuration refuses in production.

use sea_orm::{ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, Set};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{auth::password, entities::user, error::AppError};

pub struct DevUser {
    pub username: &'static str,
    pub email: &'static str,
    pub display_name: &'static str,
}

/// Password of every seeded account. Matches the dev identity provider's users.
pub const DEV_PASSWORD: &str = "password";

pub const DEV_USERS: &[DevUser] = &[
    DevUser {
        username: "alice",
        email: "alice@retro.local",
        display_name: "Alice Rossi",
    },
    DevUser {
        username: "bob",
        email: "bob@retro.local",
        display_name: "Bob Bianchi",
    },
];

/// Creates the dev users, or resets their name and password. Idempotent.
pub async fn seed_dev_users(db: &impl ConnectionTrait) -> Result<(), AppError> {
    let hash = password::hash(DEV_PASSWORD)?;
    let now = OffsetDateTime::now_utc();
    for dev in DEV_USERS {
        let existing = user::Entity::find()
            .filter(user::Column::Email.eq(dev.email))
            .one(db)
            .await?;
        match existing {
            Some(u) => {
                let mut active: user::ActiveModel = u.into();
                active.username = Set(Some(dev.username.into()));
                active.display_name = Set(dev.display_name.into());
                active.password_hash = Set(Some(hash.clone()));
                active.updated_at = Set(now);
                active.update(db).await?;
            }
            None => {
                user::ActiveModel {
                    id: Set(Uuid::now_v7()),
                    email: Set(dev.email.into()),
                    username: Set(Some(dev.username.into())),
                    display_name: Set(dev.display_name.into()),
                    avatar_url: Set(None),
                    password_hash: Set(Some(hash.clone())),
                    created_at: Set(now),
                    updated_at: Set(now),
                }
                .insert(db)
                .await?;
            }
        }
    }
    tracing::info!(count = DEV_USERS.len(), "seeded dev users");
    Ok(())
}
