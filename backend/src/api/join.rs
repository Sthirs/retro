//! Joining a team through its join link (spec §6.2).

use axum::{
    Json,
    extract::{Path, State},
};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{
    api::{TeamRefDto, check_limit},
    auth::CurrentUser,
    domain::teams,
    error::{AppError, AppResult, Problem},
    state::AppState,
};

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct JoinResponse {
    pub team: TeamRefDto,
    /// The caller was a member before opening the link.
    pub already_member: bool,
}

/// Joins the team behind an enabled join link as a member.
#[utoipa::path(
    post,
    path = "/api/v1/join/{token}",
    tag = "teams",
    security(("bearer" = [])),
    params(("token" = String, Path, description = "Join link token")),
    responses(
        (status = 200, body = JoinResponse, description = "Joined (or already a member)"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`join_link_invalid`: unknown, replaced or disabled"),
        (status = 429, body = Problem, content_type = "application/problem+json", description = "Too many unknown tokens"),
    )
)]
pub async fn join_team(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(token): Path<String>,
) -> AppResult<Json<JoinResponse>> {
    let key = user.id.to_string();
    check_limit(&state.token_limiter, &key)?;
    match teams::join_with_token(&state.db, &token, user.id).await {
        Ok((team, already_member)) => Ok(Json(JoinResponse {
            team: team.into(),
            already_member,
        })),
        Err(err @ AppError::NotFound(_)) => {
            state.token_limiter.record_failure(&key);
            Err(err)
        }
        Err(err) => Err(err),
    }
}
