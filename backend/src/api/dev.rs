//! Development-only routes. Mounted only when `SEED_DEV_USERS=true`, which the
//! configuration refuses in production. Not part of the OpenAPI contract.
//!
//! To be removed once the Discuss phase creates tasks.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use sea_orm::{ActiveModelTrait, EntityTrait, Set};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    api::client_id::ClientId,
    api::tasks::TaskDto,
    auth::CurrentUser,
    domain::{tasks, teams},
    entities::{
        meeting,
        task::{self, TaskStatus},
    },
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
    /// The source meeting, which must belong to the team.
    pub meeting_id: Option<Uuid>,
}

/// Adds a task at the end of a column, optionally on behalf of another member.
/// The caller must be a team member (`team_not_found`); a non-member `authorId`
/// is `invalid_author`, a meeting of another team `invalid_meeting`.
pub async fn seed_task(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    ClientId(client): ClientId,
    Path(team_id): Path<Uuid>,
    Json(body): Json<SeedTaskRequest>,
) -> AppResult<(StatusCode, Json<TaskDto>)> {
    teams::require_member(&state.db, team_id, user.id).await?;
    let author_id = body.author_id.unwrap_or(user.id);
    if author_id != user.id
        && teams::require_member(&state.db, team_id, author_id)
            .await
            .is_err()
    {
        return Err(AppError::Invalid("invalid_author"));
    }
    if let Some(meeting_id) = body.meeting_id {
        let meeting = meeting::Entity::find_by_id(meeting_id)
            .one(&state.db)
            .await?;
        if meeting.is_none_or(|m| m.team_id != team_id) {
            return Err(AppError::Invalid("invalid_meeting"));
        }
    }
    let mut task = tasks::insert_from(
        &state.db,
        team_id,
        author_id,
        body.status,
        &body.content,
        client,
    )
    .await?;
    if let Some(meeting_id) = body.meeting_id {
        let mut active: task::ActiveModel = task.into();
        active.meeting_id = Set(Some(meeting_id));
        task = active.update(&state.db).await?;
    }
    let view = tasks::view(&state.db, task).await?;
    Ok((StatusCode::CREATED, Json(view.into())))
}
