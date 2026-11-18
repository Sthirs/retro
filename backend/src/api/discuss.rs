//! Discuss (spec §9.5, §14.4, §14.5): the current group, reactions and the
//! tasks written for a group.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::Deserialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    api::{
        client_id::ClientId,
        meetings::{MeetingStateDto, state_of},
    },
    auth::CurrentUser,
    domain::{
        discuss,
        reactions::{self, ReactionDto},
    },
    error::{AppResult, Problem},
    state::AppState,
};

/// The group everyone discusses.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CurrentGroupRequest {
    pub group_id: Uuid,
}

/// Makes a group the current one (facilitator, during Discuss): everyone's
/// room follows it.
#[utoipa::path(
    put,
    path = "/api/v1/meetings/{meeting_id}/discuss/current",
    tag = "discuss",
    security(("bearer" = [])),
    params(("meeting_id" = Uuid, Path, description = "Meeting id")),
    request_body = CurrentGroupRequest,
    responses(
        (status = 200, body = MeetingStateDto, description = "The meeting, on its new current group"),
        (status = 403, body = Problem, content_type = "application/problem+json", description = "`not_facilitator`"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`meeting_not_found`, `group_not_found` (not a group of this meeting)"),
        (status = 409, body = Problem, content_type = "application/problem+json", description = "`meeting_ended`, `wrong_phase` (not Discuss)"),
    )
)]
pub async fn set_current_group(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    ClientId(client): ClientId,
    Path(meeting_id): Path<Uuid>,
    Json(body): Json<CurrentGroupRequest>,
) -> AppResult<Json<MeetingStateDto>> {
    discuss::set_current(&state.db, meeting_id, user.id, body.group_id, client).await?;
    Ok(Json(state_of(&state, meeting_id, user.id).await?))
}

/// A task for the current group.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateTaskRequest {
    /// The current group of the meeting.
    pub group_id: Uuid,
    /// Rich text (HTML), sanitized on the server; `@` mentions of team members.
    pub content: String,
}

/// Writes a task for the current group (participants, during Discuss). It
/// lands in the Active column of the team's board.
#[utoipa::path(
    post,
    path = "/api/v1/meetings/{meeting_id}/tasks",
    tag = "discuss",
    security(("bearer" = [])),
    params(("meeting_id" = Uuid, Path, description = "Meeting id")),
    request_body = CreateTaskRequest,
    responses(
        (status = 201, body = MeetingStateDto, description = "The meeting, with the new task"),
        (status = 400, body = Problem, content_type = "application/problem+json", description = "`invalid_task_content` (blank or invalid content), `attachment_not_found`"),
        (status = 403, body = Problem, content_type = "application/problem+json", description = "`not_participant`"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`meeting_not_found`, `group_not_found`"),
        (status = 409, body = Problem, content_type = "application/problem+json", description = "`meeting_ended`, `wrong_phase` (not Discuss), `not_current_group`"),
    )
)]
pub async fn create_task(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    ClientId(client): ClientId,
    Path(meeting_id): Path<Uuid>,
    Json(body): Json<CreateTaskRequest>,
) -> AppResult<(StatusCode, Json<MeetingStateDto>)> {
    discuss::create_task(
        &state.db,
        meeting_id,
        user.id,
        body.group_id,
        &body.content,
        client,
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(state_of(&state, meeting_id, user.id).await?),
    ))
}

/// Reacts to a reflection with an emoji (participants, during Discuss, on a
/// card of the current group). Idempotent.
#[utoipa::path(
    put,
    path = "/api/v1/reflections/{reflection_id}/reactions/{emoji}",
    tag = "discuss",
    security(("bearer" = [])),
    params(
        ("reflection_id" = Uuid, Path, description = "Reflection id"),
        ("emoji" = String, Path, description = "One Unicode emoji, percent-encoded"),
    ),
    responses(
        (status = 200, body = ReactionDto, description = "The emoji on that card, with its new count"),
        (status = 400, body = Problem, content_type = "application/problem+json", description = "`invalid_emoji`"),
        (status = 403, body = Problem, content_type = "application/problem+json", description = "`not_participant`"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`reflection_not_found`"),
        (status = 409, body = Problem, content_type = "application/problem+json", description = "`meeting_ended`, `wrong_phase` (not Discuss), `not_current_group`"),
    )
)]
pub async fn add_reaction(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    ClientId(client): ClientId,
    Path((reflection_id, emoji)): Path<(Uuid, String)>,
) -> AppResult<Json<ReactionDto>> {
    Ok(Json(
        reactions::set(&state.db, reflection_id, user.id, &emoji, true, client).await?,
    ))
}

/// Takes back the caller's reaction (same rules as adding it). Idempotent.
#[utoipa::path(
    delete,
    path = "/api/v1/reflections/{reflection_id}/reactions/{emoji}",
    tag = "discuss",
    security(("bearer" = [])),
    params(
        ("reflection_id" = Uuid, Path, description = "Reflection id"),
        ("emoji" = String, Path, description = "One Unicode emoji, percent-encoded"),
    ),
    responses(
        (status = 200, body = ReactionDto, description = "The emoji on that card, with its new count"),
        (status = 400, body = Problem, content_type = "application/problem+json", description = "`invalid_emoji`"),
        (status = 403, body = Problem, content_type = "application/problem+json", description = "`not_participant`"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`reflection_not_found`"),
        (status = 409, body = Problem, content_type = "application/problem+json", description = "`meeting_ended`, `wrong_phase` (not Discuss), `not_current_group`"),
    )
)]
pub async fn remove_reaction(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    ClientId(client): ClientId,
    Path((reflection_id, emoji)): Path<(Uuid, String)>,
) -> AppResult<Json<ReactionDto>> {
    Ok(Json(
        reactions::set(&state.db, reflection_id, user.id, &emoji, false, client).await?,
    ))
}
