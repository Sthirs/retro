//! Invitations by email address (spec §6.1, §14.2). No email is sent: the
//! response carries the personal link the inviter shares.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    api::{PersonDto, TeamRefDto, check_limit},
    auth::CurrentUser,
    domain::invitations,
    entities::{
        invitation::{self, InvitationStatus},
        user,
    },
    error::{AppError, AppResult, Problem},
    state::AppState,
};

#[derive(Debug, Deserialize, ToSchema)]
pub struct InviteRequest {
    pub email: String,
}

/// An open invitation, as listed in team settings.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InvitationDto {
    pub id: Uuid,
    pub email: String,
    /// Personal accept link (`/invite/{token}`) to share with the invitee.
    pub url: String,
    pub invited_by: Option<PersonDto>,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub expires_at: OffsetDateTime,
}

fn invitation_dto(
    state: &AppState,
    invitation: invitation::Model,
    inviter: Option<user::Model>,
) -> InvitationDto {
    InvitationDto {
        id: invitation.id,
        url: invite_url(state, &invitation.token),
        email: invitation.email,
        invited_by: inviter.map(Into::into),
        expires_at: invitation.expires_at,
    }
}

fn invite_url(state: &AppState, token: &str) -> String {
    state.config.public(&format!("/invite/{token}")).to_string()
}

/// What `/invite/{token}` shows before the invitee answers.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InvitationPreviewDto {
    /// `expired` once past `expiresAt`, whatever is stored.
    pub status: InvitationStatus,
    pub email: String,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub expires_at: OffsetDateTime,
    pub team: TeamRefDto,
    pub invited_by: Option<PersonDto>,
    /// Up to six members; empty unless the invitation is open.
    pub members: Vec<PersonDto>,
    pub member_count: i64,
    /// The caller already belongs to the team.
    pub already_member: bool,
}

/// An open invitation addressed to the signed-in user (user menu, onboarding).
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PendingInvitationDto {
    pub token: String,
    pub team: TeamRefDto,
    pub invited_by: Option<PersonDto>,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub expires_at: OffsetDateTime,
}

impl From<invitations::PendingForUser> for PendingInvitationDto {
    fn from(p: invitations::PendingForUser) -> Self {
        Self {
            token: p.token,
            team: TeamRefDto {
                id: p.team_id,
                name: p.team_name,
            },
            invited_by: p
                .inviter_id
                .zip(p.inviter_name)
                .map(|(id, display_name)| PersonDto { id, display_name }),
            expires_at: p.expires_at,
        }
    }
}

/// Counts failed token lookups against the caller.
fn record_token_failure<T>(state: &AppState, user_id: Uuid, result: AppResult<T>) -> AppResult<T> {
    if let Err(AppError::NotFound(_)) = &result {
        state.token_limiter.record_failure(&user_id.to_string());
    }
    result
}

/// Invites someone by email (any member). Returns the open invitation for
/// that email if there already is one.
#[utoipa::path(
    post,
    path = "/api/v1/teams/{team_id}/invitations",
    tag = "invitations",
    security(("bearer" = [])),
    params(("team_id" = Uuid, Path, description = "Team id")),
    request_body = InviteRequest,
    responses(
        (status = 201, body = InvitationDto, description = "The invitation and its personal link"),
        (status = 400, body = Problem, content_type = "application/problem+json", description = "`invalid_email`"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`team_not_found`"),
        (status = 409, body = Problem, content_type = "application/problem+json", description = "`already_member`"),
        (status = 429, body = Problem, content_type = "application/problem+json", description = "Too many invitations"),
    )
)]
pub async fn create_invitation(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(team_id): Path<Uuid>,
    Json(body): Json<InviteRequest>,
) -> AppResult<(StatusCode, Json<InvitationDto>)> {
    let key = user.id.to_string();
    check_limit(&state.invite_limiter, &key)?;
    let invitation = invitations::create(&state.db, team_id, user.id, &body.email).await?;
    state.invite_limiter.record_failure(&key);
    let dto = invitation_dto(&state, invitation, Some(user));
    Ok((StatusCode::CREATED, Json(dto)))
}

/// Lists a team's open invitations (any member).
#[utoipa::path(
    get,
    path = "/api/v1/teams/{team_id}/invitations",
    tag = "invitations",
    security(("bearer" = [])),
    params(("team_id" = Uuid, Path, description = "Team id")),
    responses(
        (status = 200, body = Vec<InvitationDto>, description = "Open invitations, newest first"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`team_not_found`"),
    )
)]
pub async fn list_invitations(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(team_id): Path<Uuid>,
) -> AppResult<Json<Vec<InvitationDto>>> {
    let rows = invitations::list_pending(&state.db, team_id, user.id).await?;
    Ok(Json(
        rows.into_iter()
            .map(|(i, inviter)| invitation_dto(&state, i, inviter))
            .collect(),
    ))
}

/// Revokes an invitation (its inviter or an admin).
#[utoipa::path(
    delete,
    path = "/api/v1/teams/{team_id}/invitations/{invitation_id}",
    tag = "invitations",
    security(("bearer" = [])),
    params(
        ("team_id" = Uuid, Path, description = "Team id"),
        ("invitation_id" = Uuid, Path, description = "Invitation id"),
    ),
    responses(
        (status = 204, description = "Revoked"),
        (status = 403, body = Problem, content_type = "application/problem+json", description = "Neither the inviter nor an admin"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`team_not_found` or `invitation_not_found`"),
    )
)]
pub async fn revoke_invitation(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path((team_id, invitation_id)): Path<(Uuid, Uuid)>,
) -> AppResult<StatusCode> {
    invitations::revoke(&state.db, team_id, invitation_id, user.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Shows an invitation to whoever holds its link.
#[utoipa::path(
    get,
    path = "/api/v1/invitations/{token}",
    tag = "invitations",
    security(("bearer" = [])),
    params(("token" = String, Path, description = "Invitation token")),
    responses(
        (status = 200, body = InvitationPreviewDto, description = "The invitation, in any state"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`invitation_not_found`"),
        (status = 429, body = Problem, content_type = "application/problem+json", description = "Too many unknown tokens"),
    )
)]
pub async fn get_invitation(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(token): Path<String>,
) -> AppResult<Json<InvitationPreviewDto>> {
    check_limit(&state.token_limiter, &user.id.to_string())?;
    let preview = record_token_failure(
        &state,
        user.id,
        invitations::preview(&state.db, &token, user.id).await,
    )?;
    Ok(Json(InvitationPreviewDto {
        status: preview.status,
        email: preview.invitation.email,
        expires_at: preview.invitation.expires_at,
        team: preview.team.into(),
        invited_by: preview.inviter.map(Into::into),
        members: preview.members.into_iter().map(Into::into).collect(),
        member_count: preview.member_count,
        already_member: preview.already_member,
    }))
}

/// Accepts an invitation: the caller joins the team as a member.
#[utoipa::path(
    post,
    path = "/api/v1/invitations/{token}/accept",
    tag = "invitations",
    security(("bearer" = [])),
    params(("token" = String, Path, description = "Invitation token")),
    responses(
        (status = 200, body = TeamRefDto, description = "The team joined"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`invitation_not_found`"),
        (status = 410, body = Problem, content_type = "application/problem+json", description = "`invitation_expired`, `invitation_revoked` or `invitation_used`"),
        (status = 429, body = Problem, content_type = "application/problem+json", description = "Too many unknown tokens"),
    )
)]
pub async fn accept_invitation(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(token): Path<String>,
) -> AppResult<Json<TeamRefDto>> {
    check_limit(&state.token_limiter, &user.id.to_string())?;
    let team = record_token_failure(
        &state,
        user.id,
        invitations::accept(&state.db, &token, user.id).await,
    )?;
    Ok(Json(team.into()))
}

/// Declines an invitation.
#[utoipa::path(
    post,
    path = "/api/v1/invitations/{token}/decline",
    tag = "invitations",
    security(("bearer" = [])),
    params(("token" = String, Path, description = "Invitation token")),
    responses(
        (status = 204, description = "Declined"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`invitation_not_found`"),
        (status = 410, body = Problem, content_type = "application/problem+json", description = "`invitation_expired`, `invitation_revoked` or `invitation_used`"),
        (status = 429, body = Problem, content_type = "application/problem+json", description = "Too many unknown tokens"),
    )
)]
pub async fn decline_invitation(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(token): Path<String>,
) -> AppResult<StatusCode> {
    check_limit(&state.token_limiter, &user.id.to_string())?;
    record_token_failure(
        &state,
        user.id,
        invitations::decline(&state.db, &token).await,
    )?;
    Ok(StatusCode::NO_CONTENT)
}
