//! The tasks board (spec §9, §14.2).

use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    api::PersonDto,
    auth::CurrentUser,
    domain::tasks::{self, BoardFilter, TaskView},
    entities::task::TaskStatus,
    error::{AppResult, Problem},
    state::AppState,
};

/// A task on the board.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TaskDto {
    pub id: Uuid,
    pub status: TaskStatus,
    /// Ordering key within the column; compare as plain strings.
    pub position: String,
    /// Sanitized rich text.
    pub content_html: String,
    pub author: PersonDto,
    pub mentions: Vec<PersonDto>,
    pub meeting_id: Option<Uuid>,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
    /// Whether the caller may edit, move or delete the task.
    pub can_edit: bool,
}

impl From<TaskView> for TaskDto {
    fn from(v: TaskView) -> Self {
        Self {
            id: v.task.id,
            status: v.task.status,
            position: v.task.position,
            content_html: v.task.content,
            author: v.author.into(),
            mentions: v.mentions.into_iter().map(Into::into).collect(),
            meeting_id: v.task.meeting_id,
            created_at: v.task.created_at,
            can_edit: v.can_edit,
        }
    }
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct BoardQuery {
    /// Only tasks mentioning this user.
    pub mentioned: Option<Uuid>,
    /// Only tasks written by this user.
    pub author: Option<Uuid>,
    /// Only tasks attached to this meeting.
    pub meeting: Option<Uuid>,
}

/// Lists the team's tasks, ordered by status column and position.
#[utoipa::path(
    get,
    path = "/api/v1/teams/{team_id}/tasks",
    tag = "tasks",
    security(("bearer" = [])),
    params(("team_id" = Uuid, Path, description = "Team id"), BoardQuery),
    responses(
        (status = 200, body = Vec<TaskDto>, description = "The tasks, by status then position"),
        (status = 401, body = Problem, content_type = "application/problem+json", description = "Not signed in"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`team_not_found`: no such team or not a member"),
    )
)]
pub async fn list_tasks(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(team_id): Path<Uuid>,
    Query(query): Query<BoardQuery>,
) -> AppResult<Json<Vec<TaskDto>>> {
    let filter = BoardFilter {
        mentioned: query.mentioned,
        author: query.author,
        meeting: query.meeting,
    };
    let tasks = tasks::list(&state.db, team_id, user.id, filter).await?;
    Ok(Json(tasks.into_iter().map(Into::into).collect()))
}
