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
// codeql[rust/hard-coded-cryptographic-value] Dev-only; seeding is refused in production.
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
    // Not in the dev identity provider: password sign-in only. The end-to-end
    // account spec owns this user, so it can rename it and change its picture
    // without disturbing alice and bob.
    DevUser {
        username: "dana",
        email: "dana@retro.local",
        display_name: "Dana Neri",
    },
    // Also password only. The end-to-end tasks spec owns these two, so it can
    // build boards with several members without touching alice, bob and dana.
    DevUser {
        username: "erin",
        email: "erin@retro.local",
        display_name: "Erin Gallo",
    },
    DevUser {
        username: "finn",
        email: "finn@retro.local",
        display_name: "Finn Moretti",
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
                active.display_name_custom = Set(false);
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
                    display_name_custom: Set(false),
                    picture_version: Set(None),
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
