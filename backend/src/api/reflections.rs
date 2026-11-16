//! Reflections (spec §9.2, §14.4). Each change answers with the meeting state
//! as the caller sees it.

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
    domain::reflections,
    entities::reflection::ReflectionColumn,
    error::{AppResult, Problem},
    state::AppState,
};

/// A reflection to send.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateReflectionRequest {
    pub column: ReflectionColumn,
    /// Rich text (HTML); sanitized by the server.
    pub content: String,
}

/// Sends a reflection (participants, during Reflect).
#[utoipa::path(
    post,
    path = "/api/v1/meetings/{meeting_id}/reflections",
    tag = "reflections",
    security(("bearer" = [])),
    params(("meeting_id" = Uuid, Path, description = "Meeting id")),
    request_body = CreateReflectionRequest,
    responses(
        (status = 201, body = MeetingStateDto, description = "The meeting, with the new reflection"),
        (status = 400, body = Problem, content_type = "application/problem+json", description = "`invalid_reflection_content`: blank or too long"),
        (status = 403, body = Problem, content_type = "application/problem+json", description = "`not_participant`"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`meeting_not_found`"),
        (status = 409, body = Problem, content_type = "application/problem+json", description = "`meeting_ended`, `wrong_phase` (not Reflect) or `too_many_reflections`"),
    )
)]
pub async fn create_reflection(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    ClientId(client): ClientId,
    Path(meeting_id): Path<Uuid>,
    Json(body): Json<CreateReflectionRequest>,
) -> AppResult<(StatusCode, Json<MeetingStateDto>)> {
    reflections::create(
        &state.db,
        meeting_id,
        user.id,
        body.column,
        &body.content,
        client,
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(state_of(&state, meeting_id, user.id).await?),
    ))
}

/// Changes to the caller's own reflection.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateReflectionRequest {
    /// Moves it to this column.
    pub column: Option<ReflectionColumn>,
    /// New rich text (HTML); sanitized by the server.
    pub content: Option<String>,
}

/// Edits or moves the caller's own reflection (during Reflect).
#[utoipa::path(
    patch,
    path = "/api/v1/reflections/{reflection_id}",
    tag = "reflections",
    security(("bearer" = [])),
    params(("reflection_id" = Uuid, Path, description = "Reflection id")),
    request_body = UpdateReflectionRequest,
    responses(
        (status = 200, body = MeetingStateDto, description = "The meeting"),
        (status = 400, body = Problem, content_type = "application/problem+json", description = "`invalid_reflection_content`"),
        (status = 403, body = Problem, content_type = "application/problem+json", description = "`not_participant`"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`reflection_not_found` (also someone else's) or `meeting_not_found`"),
        (status = 409, body = Problem, content_type = "application/problem+json", description = "`meeting_ended` or `wrong_phase` (reflections are immutable after Reflect)"),
    )
)]
pub async fn update_reflection(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    ClientId(client): ClientId,
    Path(reflection_id): Path<Uuid>,
    Json(body): Json<UpdateReflectionRequest>,
) -> AppResult<Json<MeetingStateDto>> {
    let meeting_id = reflections::update(
        &state.db,
        reflection_id,
        user.id,
        body.column,
        body.content.as_deref(),
        client,
    )
    .await?;
    Ok(Json(state_of(&state, meeting_id, user.id).await?))
}

/// Deletes the caller's own reflection (during Reflect).
#[utoipa::path(
    delete,
    path = "/api/v1/reflections/{reflection_id}",
    tag = "reflections",
    security(("bearer" = [])),
    params(("reflection_id" = Uuid, Path, description = "Reflection id")),
    responses(
        (status = 200, body = MeetingStateDto, description = "The meeting"),
        (status = 403, body = Problem, content_type = "application/problem+json", description = "`not_participant`"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`reflection_not_found` (also someone else's) or `meeting_not_found`"),
        (status = 409, body = Problem, content_type = "application/problem+json", description = "`meeting_ended` or `wrong_phase`"),
    )
)]
pub async fn delete_reflection(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    ClientId(client): ClientId,
    Path(reflection_id): Path<Uuid>,
) -> AppResult<Json<MeetingStateDto>> {
    let meeting_id = reflections::delete(&state.db, reflection_id, user.id, client).await?;
    Ok(Json(state_of(&state, meeting_id, user.id).await?))
}
