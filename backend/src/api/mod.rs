//! HTTP routers and handlers, one module per resource (spec §14).

pub mod auth;
pub mod health;
pub mod me;

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
