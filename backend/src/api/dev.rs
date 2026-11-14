//! Development-only routes. Mounted only when `SEED_DEV_USERS=true`, which the
//! configuration refuses in production. Not part of the OpenAPI contract.
//!
//! To be removed once the Discuss phase creates tasks.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    api::tasks::TaskDto,
    auth::CurrentUser,
    domain::{tasks, teams},
    entities::task::TaskStatus,
    error::{AppError, AppResult},
    state::AppState,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeedTaskRequest {
    pub status: TaskStatus,
    /// Rich text; sanitized like any task content.
    pub content: String,
    /// The task's author; defaults to the caller. Must be a team member.
    pub author_id: Option<Uuid>,
}

/// Adds a task at the end of a column, optionally on behalf of another member.
/// The caller must be a team member (`team_not_found`); a non-member `authorId`
/// is `invalid_author`.
pub async fn seed_task(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(team_id): Path<Uuid>,
    Json(body): Json<SeedTaskRequest>,
) -> AppResult<(StatusCode, Json<TaskDto>)> {
    let role = teams::require_member(&state.db, team_id, user.id).await?;
    let author_id = body.author_id.unwrap_or(user.id);
    if author_id != user.id
        && teams::require_member(&state.db, team_id, author_id)
            .await
            .is_err()
    {
        return Err(AppError::Invalid("invalid_author"));
    }
    let task = tasks::insert(&state.db, team_id, author_id, body.status, &body.content).await?;
    let view = tasks::view(&state.db, task, role, user.id).await?;
    Ok((StatusCode::CREATED, Json(view.into())))
}
