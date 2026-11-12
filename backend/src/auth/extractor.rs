//! `CurrentUser` extractor: validates `Authorization: Bearer <access token>`.

use axum::{extract::FromRequestParts, http::request::Parts};
use sea_orm::EntityTrait;

use crate::{entities::user, error::AppError, state::AppState};

use super::jwt;

pub struct CurrentUser(pub user::Model);

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or(AppError::Unauthorized)?;
        let user_id = jwt::verify(&state.config.jwt_secret, token).ok_or(AppError::Unauthorized)?;
        let user = user::Entity::find_by_id(user_id)
            .one(&state.db)
            .await?
            .ok_or(AppError::Unauthorized)?;
        Ok(Self(user))
    }
}
