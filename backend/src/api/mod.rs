//! HTTP routers and handlers, one module per resource (spec §14).

pub mod auth;
pub mod health;
pub mod invitations;
pub mod join;
pub mod me;
pub mod teams;

use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::entities::user;

/// A user as exposed by the API.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserDto {
    pub id: Uuid,
    pub email: String,
    pub username: Option<String>,
    pub display_name: String,
    pub avatar_url: Option<String>,
}

impl From<user::Model> for UserDto {
    fn from(u: user::Model) -> Self {
        Self {
            id: u.id,
            email: u.email,
            username: u.username,
            display_name: u.display_name,
            avatar_url: u.avatar_url,
        }
    }
}

/// A user reduced to what other team members may see next to their avatar.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PersonDto {
    pub id: Uuid,
    pub display_name: String,
}

impl From<user::Model> for PersonDto {
    fn from(u: user::Model) -> Self {
        Self {
            id: u.id,
            display_name: u.display_name,
        }
    }
}

/// A team reference: id and name only.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TeamRefDto {
    pub id: Uuid,
    pub name: String,
}

impl From<crate::entities::team::Model> for TeamRefDto {
    fn from(t: crate::entities::team::Model) -> Self {
        Self {
            id: t.id,
            name: t.name,
        }
    }
}

/// Fails with `too_many_requests` when `key` is blocked on `limiter`.
pub(crate) fn check_limit(
    limiter: &crate::auth::rate_limit::RateLimiter,
    key: &str,
) -> Result<(), crate::error::AppError> {
    if limiter.is_blocked(key) {
        Err(crate::error::AppError::TooManyRequests)
    } else {
        Ok(())
    }
}

/// Counts a failed token lookup (`NotFound`) against `user_id` on `limiter`,
/// so unknown invitation and join tokens cannot be guessed at speed.
pub(crate) fn record_token_failure<T>(
    limiter: &crate::auth::rate_limit::RateLimiter,
    user_id: Uuid,
    result: crate::error::AppResult<T>,
) -> crate::error::AppResult<T> {
    if let Err(crate::error::AppError::NotFound(_)) = &result {
        limiter.record_failure(&user_id.to_string());
    }
    result
}
