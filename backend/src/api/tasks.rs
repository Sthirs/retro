//! The tasks board (spec §9, §14.2).

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    api::PersonDto,
    api::client_id::ClientId,
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
        }
    }
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct BoardQuery {
    /// Comma-separated user ids: only tasks mentioning any of them.
    #[serde(default, deserialize_with = "uuid_list")]
    #[param(value_type = Option<String>)]
    pub mentioned: Vec<Uuid>,
    /// Only tasks written by this user.
    pub author: Option<Uuid>,
    /// Only tasks attached to this meeting.
    pub meeting: Option<Uuid>,
}

/// Parses a comma-separated list of UUIDs; empty items are skipped.
fn uuid_list<'de, D: serde::Deserializer<'de>>(de: D) -> Result<Vec<Uuid>, D::Error> {
    let raw = Option::<String>::deserialize(de)?.unwrap_or_default();
    let mut ids = Vec::new();
    for item in raw.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let id = Uuid::parse_str(item).map_err(serde::de::Error::custom)?;
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    Ok(ids)
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

/// Where to put a task.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MoveTaskRequest {
    /// The column to move the task to.
    pub status: TaskStatus,
    /// The task the moved one should land directly after, if any.
    pub after_id: Option<Uuid>,
    /// The task the moved one should land directly before when `afterId` is
    /// absent. With both, the task lands directly after `afterId`, and
    /// `beforeId` must be below it.
    pub before_id: Option<Uuid>,
}

/// Moves a task between or within columns. With neither neighbour, the task
/// goes to the end of the column. Any team member may move any task.
#[utoipa::path(
    put,
    path = "/api/v1/tasks/{task_id}/position",
    tag = "tasks",
    security(("bearer" = [])),
    params(("task_id" = Uuid, Path, description = "Task id")),
    request_body = MoveTaskRequest,
    responses(
        (status = 200, body = TaskDto, description = "The moved task"),
        (status = 400, body = Problem, content_type = "application/problem+json", description = "`invalid_position`: a neighbour is not in the team and target column, or is the task itself"),
        (status = 401, body = Problem, content_type = "application/problem+json", description = "Not signed in"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`task_not_found`: no such task or not a member of its team"),
        (status = 409, body = Problem, content_type = "application/problem+json", description = "`stale_position`: `afterId` is not before `beforeId`"),
    )
)]
pub async fn move_task(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    ClientId(client): ClientId,
    Path(task_id): Path<Uuid>,
    Json(body): Json<MoveTaskRequest>,
) -> AppResult<Json<TaskDto>> {
    let task = tasks::move_task(
        &state.db,
        task_id,
        user.id,
        body.status,
        body.after_id,
        body.before_id,
        client,
    )
    .await?;
    Ok(Json(task.into()))
}

/// New content for a task.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateTaskRequest {
    /// Rich text; sanitized by the server.
    pub content: String,
}

/// Edits a task's content. Any team member may.
#[utoipa::path(
    patch,
    path = "/api/v1/tasks/{task_id}",
    tag = "tasks",
    security(("bearer" = [])),
    params(("task_id" = Uuid, Path, description = "Task id")),
    request_body = UpdateTaskRequest,
    responses(
        (status = 200, body = TaskDto, description = "The edited task"),
        (status = 400, body = Problem, content_type = "application/problem+json", description = "`invalid_task_content`: blank or invalid content"),
        (status = 401, body = Problem, content_type = "application/problem+json", description = "Not signed in"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`task_not_found`: no such task or not a member of its team"),
    )
)]
pub async fn update_task(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    ClientId(client): ClientId,
    Path(task_id): Path<Uuid>,
    Json(body): Json<UpdateTaskRequest>,
) -> AppResult<Json<TaskDto>> {
    let task = tasks::edit(&state.db, task_id, user.id, &body.content, client).await?;
    Ok(Json(task.into()))
}

/// Deletes a task. Any team member may.
#[utoipa::path(
    delete,
    path = "/api/v1/tasks/{task_id}",
    tag = "tasks",
    security(("bearer" = [])),
    params(("task_id" = Uuid, Path, description = "Task id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 401, body = Problem, content_type = "application/problem+json", description = "Not signed in"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`task_not_found`: no such task or not a member of its team"),
    )
)]
pub async fn delete_task(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    ClientId(client): ClientId,
    Path(task_id): Path<Uuid>,
) -> AppResult<StatusCode> {
    tasks::delete(&state.db, task_id, user.id, client).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(query: &str) -> Result<BoardQuery, serde_json::Error> {
        serde_json::from_str(query)
    }

    #[test]
    fn mentioned_is_empty_when_absent() {
        assert!(parse("{}").unwrap().mentioned.is_empty());
    }

    #[test]
    fn mentioned_accepts_one_or_several_ids() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        assert_eq!(
            parse(&format!(r#"{{"mentioned":"{a}"}}"#))
                .unwrap()
                .mentioned,
            vec![a]
        );
        assert_eq!(
            parse(&format!(r#"{{"mentioned":"{a}, {b},,{a}"}}"#))
                .unwrap()
                .mentioned,
            vec![a, b]
        );
    }

    #[test]
    fn mentioned_rejects_an_invalid_id() {
        assert!(parse(r#"{"mentioned":"nope"}"#).is_err());
    }
}
