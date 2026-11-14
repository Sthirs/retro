//! The current user, their teams and pending invitations (spec §14.1).

use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    api::{UserDto, invitations::PendingInvitationDto, teams::TeamDto},
    auth::CurrentUser,
    domain::{invitations, teams, users},
    error::{AppResult, Problem},
    state::AppState,
};

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MeResponse {
    pub user: UserDto,
    /// Teams the user belongs to, by name.
    pub teams: Vec<TeamDto>,
    /// Open invitations sent to the user's email, newest first.
    pub invitations: Vec<PendingInvitationDto>,
}

/// Returns the signed-in user with their teams and pending invitations.
#[utoipa::path(
    get,
    path = "/api/v1/me",
    tag = "users",
    security(("bearer" = [])),
    responses(
        (status = 200, body = MeResponse, description = "The signed-in user"),
        (status = 401, body = Problem, content_type = "application/problem+json", description = "Missing or invalid access token"),
    )
)]
pub async fn me(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> AppResult<Json<MeResponse>> {
    let teams = teams::list_for_user(&state.db, user.id).await?;
    let invitations = invitations::pending_for_user(&state.db, &user).await?;
    Ok(Json(MeResponse {
        user: user.into(),
        teams: teams.into_iter().map(Into::into).collect(),
        invitations: invitations.into_iter().map(Into::into).collect(),
    }))
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMeRequest {
    /// New display name; trimmed, 1 to 80 characters.
    pub display_name: String,
}

/// Changes the signed-in user's display name.
#[utoipa::path(
    patch,
    path = "/api/v1/me",
    tag = "users",
    security(("bearer" = [])),
    request_body = UpdateMeRequest,
    responses(
        (status = 200, body = UserDto, description = "The updated user"),
        (status = 400, body = Problem, content_type = "application/problem+json", description = "`invalid_display_name`"),
        (status = 401, body = Problem, content_type = "application/problem+json", description = "Missing or invalid access token"),
    )
)]
pub async fn update_me(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(body): Json<UpdateMeRequest>,
) -> AppResult<Json<UserDto>> {
    let user = users::update_display_name(&state.db, user, &body.display_name).await?;
    Ok(Json(user.into()))
}
