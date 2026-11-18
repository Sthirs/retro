//! Groups (spec §9.3, §14.4). Each change answers with the meeting state as
//! the caller sees it.

use axum::{
    Json,
    extract::{Path, State},
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
    domain::groups,
    error::{AppResult, Problem},
    state::AppState,
};

/// A card dropped onto another card.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateGroupRequest {
    /// The card it was dropped onto.
    pub target_id: Uuid,
    /// The card that was dropped.
    pub reflection_id: Uuid,
}

/// Drops a card onto another card: groups both, or adds the card to the
/// target's group if it has one by now (participants, during Group).
#[utoipa::path(
    post,
    path = "/api/v1/meetings/{meeting_id}/groups",
    tag = "groups",
    security(("bearer" = [])),
    params(("meeting_id" = Uuid, Path, description = "Meeting id")),
    request_body = CreateGroupRequest,
    responses(
        (status = 200, body = MeetingStateDto, description = "The meeting, with the cards grouped"),
        (status = 400, body = Problem, content_type = "application/problem+json", description = "`same_reflection`: a card dropped onto itself"),
        (status = 403, body = Problem, content_type = "application/problem+json", description = "`not_participant`"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`meeting_not_found`, or `reflection_not_found` (not a card of this meeting)"),
        (status = 409, body = Problem, content_type = "application/problem+json", description = "`meeting_ended` or `wrong_phase` (not Group)"),
    )
)]
pub async fn create_group(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    ClientId(client): ClientId,
    Path(meeting_id): Path<Uuid>,
    Json(body): Json<CreateGroupRequest>,
) -> AppResult<Json<MeetingStateDto>> {
    groups::create(
        &state.db,
        meeting_id,
        user.id,
        body.target_id,
        body.reflection_id,
        client,
    )
    .await?;
    Ok(Json(state_of(&state, meeting_id, user.id).await?))
}

/// A group's new name.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RenameGroupRequest {
    /// 0–80 characters once trimmed; empty shows as "Untitled group".
    pub name: String,
}

/// Renames a group (participants, during Group; the last name saved wins).
#[utoipa::path(
    patch,
    path = "/api/v1/groups/{group_id}",
    tag = "groups",
    security(("bearer" = [])),
    params(("group_id" = Uuid, Path, description = "Group id")),
    request_body = RenameGroupRequest,
    responses(
        (status = 200, body = MeetingStateDto, description = "The meeting"),
        (status = 400, body = Problem, content_type = "application/problem+json", description = "`invalid_group_name`: over 80 characters"),
        (status = 403, body = Problem, content_type = "application/problem+json", description = "`not_participant`"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`group_not_found` (also once dissolved)"),
        (status = 409, body = Problem, content_type = "application/problem+json", description = "`meeting_ended` or `wrong_phase` (not Group)"),
    )
)]
pub async fn rename_group(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    ClientId(client): ClientId,
    Path(group_id): Path<Uuid>,
    Json(body): Json<RenameGroupRequest>,
) -> AppResult<Json<MeetingStateDto>> {
    let meeting_id = groups::rename(&state.db, group_id, user.id, &body.name, client).await?;
    Ok(Json(state_of(&state, meeting_id, user.id).await?))
}

/// Where a card goes.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MoveCardRequest {
    /// The group to move it into; `null` takes it out of its group.
    pub group_id: Option<Uuid>,
}

/// Moves a card into a group, or out of its group (participants, during
/// Group). A group left with one card dissolves.
#[utoipa::path(
    put,
    path = "/api/v1/reflections/{reflection_id}/group",
    tag = "groups",
    security(("bearer" = [])),
    params(("reflection_id" = Uuid, Path, description = "Reflection id")),
    request_body = MoveCardRequest,
    responses(
        (status = 200, body = MeetingStateDto, description = "The meeting"),
        (status = 403, body = Problem, content_type = "application/problem+json", description = "`not_participant`"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`reflection_not_found`, or `group_not_found` (not a group of this meeting, or dissolved)"),
        (status = 409, body = Problem, content_type = "application/problem+json", description = "`meeting_ended` or `wrong_phase` (not Group)"),
    )
)]
pub async fn move_card(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    ClientId(client): ClientId,
    Path(reflection_id): Path<Uuid>,
    Json(body): Json<MoveCardRequest>,
) -> AppResult<Json<MeetingStateDto>> {
    let meeting_id =
        groups::move_card(&state.db, reflection_id, user.id, body.group_id, client).await?;
    Ok(Json(state_of(&state, meeting_id, user.id).await?))
}
