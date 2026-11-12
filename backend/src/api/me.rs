//! The current user (spec §14.1).

use axum::Json;
use serde::Serialize;
use utoipa::ToSchema;

use crate::{api::UserDto, auth::CurrentUser, error::Problem};

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MeResponse {
    pub user: UserDto,
}

/// Returns the signed-in user.
#[utoipa::path(
    get,
    path = "/api/v1/me",
    tag = "users",
    security(("bearer" = [])),
    responses(
        (status = 200, body = MeResponse),
        (status = 401, body = Problem, content_type = "application/problem+json"),
    )
)]
pub async fn me(CurrentUser(user): CurrentUser) -> Json<MeResponse> {
    Json(MeResponse { user: user.into() })
}
