//! Meetings (spec §8, §9.1, §14.3) and the WebSocket ticket (spec §3.3).

use std::collections::HashSet;

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
    api::{PersonDto, check_limit, record_token_failure},
    auth::CurrentUser,
    domain::meetings::{self, MeetingView, Participant},
    entities::{
        meeting::{MeetingPhase, MeetingStatus},
        team_member::Role,
    },
    error::{AppError, AppResult, Problem},
    state::AppState,
    ws::ticket,
};

/// A participant of a meeting.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ParticipantDto {
    pub id: Uuid,
    pub display_name: String,
    /// Content hash of the uploaded picture; `null` when there is none.
    pub picture_version: Option<String>,
    /// Team role; `null` once they left the team.
    pub role: Option<Role>,
    /// Connected to the meeting room right now.
    pub online: bool,
}

fn participants(view: &MeetingView, online: &HashSet<Uuid>) -> Vec<ParticipantDto> {
    view.participants
        .iter()
        .map(|Participant { user, role }| ParticipantDto {
            id: user.id,
            display_name: user.display_name.clone(),
            picture_version: user.picture_version.clone(),
            role: *role,
            online: online.contains(&user.id),
        })
        .collect()
}

/// A meeting as the Meetings page lists it.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MeetingSummaryDto {
    pub id: Uuid,
    pub team_id: Uuid,
    pub title: String,
    pub status: MeetingStatus,
    pub phase: MeetingPhase,
    pub facilitator: PersonDto,
    /// In joining order.
    pub participants: Vec<ParticipantDto>,
    pub task_count: i64,
    /// Groups formed in the meeting; always 0 until the Group phase exists.
    pub group_count: i64,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub ended_at: Option<OffsetDateTime>,
}

impl MeetingSummaryDto {
    fn new(view: MeetingView, online: &HashSet<Uuid>) -> Self {
        let participants = participants(&view, online);
        Self {
            id: view.meeting.id,
            team_id: view.meeting.team_id,
            title: view.meeting.title,
            status: view.meeting.status,
            phase: view.meeting.phase,
            facilitator: view.facilitator.into(),
            participants,
            task_count: view.task_count,
            group_count: 0,
            created_at: view.meeting.created_at,
            ended_at: view.meeting.ended_at,
        }
    }
}

/// "Hello" in the meeting's language.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GreetingDto {
    pub text: String,
    /// English name of the language, e.g. "Italian".
    pub language: String,
    /// BCP 47 tag for the `lang` attribute, e.g. "it".
    pub lang_code: String,
}

/// Where the icebreaker stands.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IcebreakerDto {
    /// Speaking order (user ids).
    pub order: Vec<Uuid>,
    /// Index into `order` of the person speaking now.
    pub index: i32,
    /// People the facilitator skipped.
    pub skipped: Vec<Uuid>,
}

/// Everything the meeting room shows (spec §14.3 `GET /meetings/{id}/state`).
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MeetingStateDto {
    pub id: Uuid,
    pub team_id: Uuid,
    pub team_name: String,
    pub title: String,
    pub status: MeetingStatus,
    pub phase: MeetingPhase,
    pub facilitator_id: Uuid,
    /// In joining order.
    pub participants: Vec<ParticipantDto>,
    /// Everyone in the meeting room right now, including people whose join
    /// is still on its way.
    pub online_ids: Vec<Uuid>,
    pub greeting: GreetingDto,
    pub question: String,
    pub icebreaker: IcebreakerDto,
    pub task_count: i64,
    /// Last event `seq` included in this snapshot on `meeting:{id}`.
    pub seq: i64,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub ended_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub server_now: OffsetDateTime,
}

impl MeetingStateDto {
    fn new(view: MeetingView, online: &HashSet<Uuid>) -> Self {
        let participants = participants(&view, online);
        let m = view.meeting;
        Self {
            id: m.id,
            team_id: m.team_id,
            team_name: view.team_name,
            title: m.title,
            status: m.status,
            phase: m.phase,
            facilitator_id: m.facilitator_id,
            participants,
            online_ids: {
                let mut ids: Vec<Uuid> = online.iter().copied().collect();
                ids.sort();
                ids
            },
            greeting: GreetingDto {
                text: m.greeting_text,
                language: m.greeting_language,
                lang_code: m.greeting_lang_code,
            },
            question: m.icebreaker_question,
            icebreaker: IcebreakerDto {
                order: m.icebreaker_order,
                index: m.icebreaker_index,
                skipped: m.icebreaker_skipped,
            },
            task_count: view.task_count,
            seq: m.event_seq,
            created_at: m.created_at,
            ended_at: m.ended_at,
            server_now: OffsetDateTime::now_utc(),
        }
    }
}

/// The meeting's current state, as `actor` sees it.
async fn state_of(state: &AppState, meeting_id: Uuid, actor: Uuid) -> AppResult<MeetingStateDto> {
    let view = meetings::view(&state.db, meeting_id, actor).await?;
    let online = state.hub.online(meeting_id);
    Ok(MeetingStateDto::new(view, &online))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct MeetingsQuery {
    /// Only meetings in this state.
    pub status: Option<MeetingStatus>,
}

/// Lists the team's meetings, newest first.
#[utoipa::path(
    get,
    path = "/api/v1/teams/{team_id}/meetings",
    tag = "meetings",
    security(("bearer" = [])),
    params(("team_id" = Uuid, Path, description = "Team id"), MeetingsQuery),
    responses(
        (status = 200, body = Vec<MeetingSummaryDto>, description = "The meetings, newest first"),
        (status = 401, body = Problem, content_type = "application/problem+json", description = "Not signed in"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`team_not_found`: no such team or not a member"),
    )
)]
pub async fn list_meetings(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(team_id): Path<Uuid>,
    Query(query): Query<MeetingsQuery>,
) -> AppResult<Json<Vec<MeetingSummaryDto>>> {
    let views = meetings::list(&state.db, team_id, user.id, query.status).await?;
    Ok(Json(
        views
            .into_iter()
            .map(|view| {
                let online = state.hub.online(view.meeting.id);
                MeetingSummaryDto::new(view, &online)
            })
            .collect(),
    ))
}

/// Starts a meeting; the caller becomes its facilitator.
#[utoipa::path(
    post,
    path = "/api/v1/teams/{team_id}/meetings",
    tag = "meetings",
    security(("bearer" = [])),
    params(("team_id" = Uuid, Path, description = "Team id")),
    responses(
        (status = 201, body = MeetingStateDto, description = "The new meeting"),
        (status = 401, body = Problem, content_type = "application/problem+json", description = "Not signed in"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`team_not_found`: no such team or not a member"),
    )
)]
pub async fn start_meeting(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(team_id): Path<Uuid>,
) -> AppResult<(StatusCode, Json<MeetingStateDto>)> {
    let meeting = meetings::create(&state.db, team_id, user.id).await?;
    Ok((
        StatusCode::CREATED,
        Json(state_of(&state, meeting.id, user.id).await?),
    ))
}

/// The meeting room's full state; used on entry and after a reconnect.
#[utoipa::path(
    get,
    path = "/api/v1/meetings/{meeting_id}/state",
    tag = "meetings",
    security(("bearer" = [])),
    params(("meeting_id" = Uuid, Path, description = "Meeting id")),
    responses(
        (status = 200, body = MeetingStateDto, description = "The meeting"),
        (status = 401, body = Problem, content_type = "application/problem+json", description = "Not signed in"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`meeting_not_found`: no such meeting or not a team member"),
    )
)]
pub async fn get_meeting_state(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(meeting_id): Path<Uuid>,
) -> AppResult<Json<MeetingStateDto>> {
    Ok(Json(state_of(&state, meeting_id, user.id).await?))
}

/// A new title.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RenameMeetingRequest {
    /// 1–80 characters after trimming.
    pub title: String,
}

/// Renames a running meeting (facilitator).
#[utoipa::path(
    patch,
    path = "/api/v1/meetings/{meeting_id}",
    tag = "meetings",
    security(("bearer" = [])),
    params(("meeting_id" = Uuid, Path, description = "Meeting id")),
    request_body = RenameMeetingRequest,
    responses(
        (status = 200, body = MeetingStateDto, description = "The renamed meeting"),
        (status = 400, body = Problem, content_type = "application/problem+json", description = "`invalid_title`: blank or longer than 80 characters"),
        (status = 403, body = Problem, content_type = "application/problem+json", description = "`not_facilitator`"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`meeting_not_found`"),
        (status = 409, body = Problem, content_type = "application/problem+json", description = "`meeting_ended`"),
    )
)]
pub async fn rename_meeting(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(meeting_id): Path<Uuid>,
    Json(body): Json<RenameMeetingRequest>,
) -> AppResult<Json<MeetingStateDto>> {
    meetings::rename(&state.db, meeting_id, user.id, &body.title).await?;
    Ok(Json(state_of(&state, meeting_id, user.id).await?))
}

/// Joins a running meeting as a participant (idempotent).
#[utoipa::path(
    post,
    path = "/api/v1/meetings/{meeting_id}/join",
    tag = "meetings",
    security(("bearer" = [])),
    params(("meeting_id" = Uuid, Path, description = "Meeting id")),
    responses(
        (status = 200, body = MeetingStateDto, description = "The meeting, with the caller as a participant"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`meeting_not_found`"),
        (status = 409, body = Problem, content_type = "application/problem+json", description = "`meeting_ended`"),
    )
)]
pub async fn join_meeting(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(meeting_id): Path<Uuid>,
) -> AppResult<Json<MeetingStateDto>> {
    meetings::join(&state.db, meeting_id, user.id).await?;
    Ok(Json(state_of(&state, meeting_id, user.id).await?))
}

/// How to leave the current icebreaker speaker.
#[derive(Debug, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IcebreakerNextRequest {
    /// Mark the current person as skipped instead of done.
    #[serde(default)]
    pub skip: bool,
}

/// Moves the icebreaker to the next person (facilitator).
#[utoipa::path(
    post,
    path = "/api/v1/meetings/{meeting_id}/icebreaker/next",
    tag = "meetings",
    security(("bearer" = [])),
    params(("meeting_id" = Uuid, Path, description = "Meeting id")),
    request_body = IcebreakerNextRequest,
    responses(
        (status = 200, body = MeetingStateDto, description = "The meeting"),
        (status = 403, body = Problem, content_type = "application/problem+json", description = "`not_facilitator`"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`meeting_not_found`"),
        (status = 409, body = Problem, content_type = "application/problem+json", description = "`meeting_ended`, `wrong_phase`, or `icebreaker_done` (the last person is speaking)"),
    )
)]
pub async fn icebreaker_next(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(meeting_id): Path<Uuid>,
    Json(body): Json<IcebreakerNextRequest>,
) -> AppResult<Json<MeetingStateDto>> {
    meetings::icebreaker_next(&state.db, meeting_id, user.id, body.skip).await?;
    Ok(Json(state_of(&state, meeting_id, user.id).await?))
}

/// Advances to the next phase (facilitator).
#[utoipa::path(
    post,
    path = "/api/v1/meetings/{meeting_id}/phase/next",
    tag = "meetings",
    security(("bearer" = [])),
    params(("meeting_id" = Uuid, Path, description = "Meeting id")),
    responses(
        (status = 200, body = MeetingStateDto, description = "The meeting"),
        (status = 403, body = Problem, content_type = "application/problem+json", description = "`not_facilitator`"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`meeting_not_found`"),
        (status = 409, body = Problem, content_type = "application/problem+json", description = "`meeting_ended` or `last_phase`"),
    )
)]
pub async fn next_phase(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(meeting_id): Path<Uuid>,
) -> AppResult<Json<MeetingStateDto>> {
    meetings::next_phase(&state.db, meeting_id, user.id).await?;
    Ok(Json(state_of(&state, meeting_id, user.id).await?))
}

/// The new facilitator.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetFacilitatorRequest {
    /// Another participant (transfer), or the caller (team admin take-over).
    pub user_id: Uuid,
}

/// Transfers the facilitator role (facilitator) or takes it over (team admin).
#[utoipa::path(
    post,
    path = "/api/v1/meetings/{meeting_id}/facilitator",
    tag = "meetings",
    security(("bearer" = [])),
    params(("meeting_id" = Uuid, Path, description = "Meeting id")),
    request_body = SetFacilitatorRequest,
    responses(
        (status = 200, body = MeetingStateDto, description = "The meeting"),
        (status = 400, body = Problem, content_type = "application/problem+json", description = "`target_not_participant`"),
        (status = 403, body = Problem, content_type = "application/problem+json", description = "`not_facilitator`: neither the facilitator nor an admin taking over"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`meeting_not_found`"),
        (status = 409, body = Problem, content_type = "application/problem+json", description = "`meeting_ended`"),
    )
)]
pub async fn set_facilitator(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(meeting_id): Path<Uuid>,
    Json(body): Json<SetFacilitatorRequest>,
) -> AppResult<Json<MeetingStateDto>> {
    meetings::set_facilitator(&state.db, meeting_id, user.id, body.user_id).await?;
    Ok(Json(state_of(&state, meeting_id, user.id).await?))
}

/// Ends the meeting (facilitator).
#[utoipa::path(
    post,
    path = "/api/v1/meetings/{meeting_id}/end",
    tag = "meetings",
    security(("bearer" = [])),
    params(("meeting_id" = Uuid, Path, description = "Meeting id")),
    responses(
        (status = 200, body = MeetingStateDto, description = "The ended meeting"),
        (status = 403, body = Problem, content_type = "application/problem+json", description = "`not_facilitator`"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`meeting_not_found`"),
        (status = 409, body = Problem, content_type = "application/problem+json", description = "`meeting_ended`"),
    )
)]
pub async fn end_meeting(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(meeting_id): Path<Uuid>,
) -> AppResult<Json<MeetingStateDto>> {
    meetings::end(&state.db, meeting_id, user.id).await?;
    Ok(Json(state_of(&state, meeting_id, user.id).await?))
}

/// The meeting invite link.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MeetingInviteLinkDto {
    /// Absolute URL (`/m/{token}`).
    pub url: String,
}

fn invite_link(state: &AppState, token: &str) -> MeetingInviteLinkDto {
    MeetingInviteLinkDto {
        url: state.config.public(&format!("/m/{token}")).to_string(),
    }
}

/// The invite link of a running meeting (any team member).
#[utoipa::path(
    get,
    path = "/api/v1/meetings/{meeting_id}/invite-link",
    tag = "meetings",
    security(("bearer" = [])),
    params(("meeting_id" = Uuid, Path, description = "Meeting id")),
    responses(
        (status = 200, body = MeetingInviteLinkDto, description = "The link"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`meeting_not_found`"),
        (status = 409, body = Problem, content_type = "application/problem+json", description = "`meeting_ended`"),
    )
)]
pub async fn get_invite_link(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(meeting_id): Path<Uuid>,
) -> AppResult<Json<MeetingInviteLinkDto>> {
    let token = meetings::invite_token(&state.db, meeting_id, user.id).await?;
    Ok(Json(invite_link(&state, &token)))
}

/// Replaces the invite link, invalidating the previous one (facilitator).
#[utoipa::path(
    post,
    path = "/api/v1/meetings/{meeting_id}/invite-link",
    tag = "meetings",
    security(("bearer" = [])),
    params(("meeting_id" = Uuid, Path, description = "Meeting id")),
    responses(
        (status = 200, body = MeetingInviteLinkDto, description = "The new link"),
        (status = 403, body = Problem, content_type = "application/problem+json", description = "`not_facilitator`"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`meeting_not_found`"),
        (status = 409, body = Problem, content_type = "application/problem+json", description = "`meeting_ended`"),
    )
)]
pub async fn regenerate_invite_link(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(meeting_id): Path<Uuid>,
) -> AppResult<Json<MeetingInviteLinkDto>> {
    let token = meetings::regenerate_invite_token(&state.db, meeting_id, user.id).await?;
    Ok(Json(invite_link(&state, &token)))
}

/// Where a meeting invite link led.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MeetingInviteAcceptedDto {
    pub team_id: Uuid,
    pub meeting_id: Uuid,
    pub title: String,
    /// The caller was added to the team by this link.
    pub joined_team: bool,
}

/// Follows a meeting invite link: joins the team if needed, then the meeting.
#[utoipa::path(
    post,
    path = "/api/v1/meeting-invites/{token}/accept",
    tag = "meetings",
    security(("bearer" = [])),
    params(("token" = String, Path, description = "Meeting invite token")),
    responses(
        (status = 200, body = MeetingInviteAcceptedDto, description = "Joined"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`meeting_invite_invalid`: unknown, replaced, or the meeting ended"),
        (status = 429, body = Problem, content_type = "application/problem+json", description = "Too many unknown tokens"),
    )
)]
pub async fn accept_meeting_invite(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(token): Path<String>,
) -> AppResult<Json<MeetingInviteAcceptedDto>> {
    check_limit(&state.token_limiter, &user.id.to_string())?;
    let (meeting, joined_team) = record_token_failure(
        &state.token_limiter,
        user.id,
        meetings::accept_invite(&state.db, &token, user.id).await,
    )?;
    Ok(Json(MeetingInviteAcceptedDto {
        team_id: meeting.team_id,
        meeting_id: meeting.id,
        title: meeting.title,
        joined_team,
    }))
}

/// A ticket to open the real-time WebSocket.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WsTicketDto {
    /// Pass as `?ticket=` to `/api/v1/ws` within 30 seconds.
    pub ticket: String,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub expires_at: OffsetDateTime,
}

/// Issues a short-lived ticket for the WebSocket handshake (spec §3.3).
#[utoipa::path(
    post,
    path = "/api/v1/ws-ticket",
    tag = "realtime",
    security(("bearer" = [])),
    responses(
        (status = 200, body = WsTicketDto, description = "The ticket"),
        (status = 401, body = Problem, content_type = "application/problem+json", description = "Not signed in"),
    )
)]
pub async fn ws_ticket(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> AppResult<Json<WsTicketDto>> {
    let (ticket, exp) = ticket::issue(&state.config.jwt_secret, user.id)?;
    let expires_at =
        OffsetDateTime::from_unix_timestamp(exp).map_err(|e| AppError::Internal(e.into()))?;
    Ok(Json(WsTicketDto { ticket, expires_at }))
}
