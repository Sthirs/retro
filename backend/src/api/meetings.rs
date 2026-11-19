//! Meetings (spec §8, §9.1, §14.3): lifecycle, icebreaker, timer and ready
//! flags; and the WebSocket ticket (spec §3.3).

use std::collections::HashSet;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use sea_orm::{AccessMode, IsolationLevel, TransactionTrait};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    api::{PersonDto, check_limit, record_token_failure, tasks::TaskDto},
    auth::CurrentUser,
    domain::{
        discuss::{self, DiscussGroupDto},
        groups::{self, GroupDto},
        meetings::{self, MeetingView, Participant},
        reactions::{self, ReactionDto},
        reflections::{self, ReflectionCountsDto, ReflectionsSnapshot},
        summary::{self, MeetingSummary, SummaryGroupDto},
        tasks::{self, TaskView},
        timer::{Timer, TimerAction},
        votes::{self, MyVoteDto},
    },
    entities::{
        meeting::{MeetingPhase, MeetingStatus},
        reflection::ReflectionColumn,
        team_member::Role,
    },
    error::{AppError, AppResult, Problem},
    state::AppState,
    ws::{DragDto, WritingCounts, WritingDto, ticket},
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
    /// Pressed Ready in the current phase.
    pub ready: bool,
}

fn participants(view: &MeetingView, online: &HashSet<Uuid>) -> Vec<ParticipantDto> {
    view.participants
        .iter()
        .map(|Participant { user, role, ready }| ParticipantDto {
            id: user.id,
            display_name: user.display_name.clone(),
            picture_version: user.picture_version.clone(),
            role: *role,
            online: online.contains(&user.id),
            ready: *ready,
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
    /// Groups formed in the meeting.
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
            group_count: view.group_count,
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
    /// IPA, e.g. "/ˈtʃa.o/".
    pub pronunciation: String,
}

/// Where the icebreaker stands.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IcebreakerDto {
    /// Speaking order (user ids).
    pub order: Vec<Uuid>,
    /// Index into `order` of the person speaking now.
    pub index: i32,
}

/// A reflection, as its viewer may see it: never who wrote it.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReflectionDto {
    pub id: Uuid,
    pub column: ReflectionColumn,
    /// Sanitized rich text (HTML).
    pub content: String,
    /// Written by the viewer.
    pub mine: bool,
    /// Its group (from Group on).
    pub group_id: Option<Uuid>,
    /// Place in its column, set when Group starts.
    pub position: i32,
    /// Raised by every move between groups.
    pub version: i32,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: OffsetDateTime,
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
    /// The countdown, if the facilitator set one.
    pub timer: Option<Timer>,
    /// During Reflect the caller's own reflections; from Group on, everyone's
    /// (anonymous).
    pub reflections: Vec<ReflectionDto>,
    /// Sent reflections per column, everyone's.
    pub reflection_counts: ReflectionCountsDto,
    /// Reflections being written right now, per column.
    pub writing: WritingDto,
    /// From Group on, the groups, by column and place.
    pub groups: Vec<GroupDto>,
    /// Cards being dragged right now, and by whom (Group).
    pub drags: Vec<DragDto>,
    /// Votes each participant may give in all (spec §9.4).
    pub vote_budget: i32,
    /// Most votes one participant may give the same group.
    pub vote_max_per_group: i32,
    /// From Vote on, the caller's own votes per group. Nobody sees anyone
    /// else's votes.
    pub my_votes: Vec<MyVoteDto>,
    /// During Vote, votes cast overall, as one number (nothing per group or
    /// per person), for everyone. `null` in the other phases.
    pub votes_cast: Option<i32>,
    /// From Discuss on, every group in Discuss order with its vote total
    /// (spec §9.5); empty before, so totals stay hidden until then.
    pub discuss_order: Vec<DiscussGroupDto>,
    /// During Discuss, the group everyone follows.
    pub discuss_group_id: Option<Uuid>,
    /// From Discuss on, the reactions on every reflection, each emoji in the
    /// order it was first used.
    pub reactions: Vec<ReactionDto>,
    /// From Discuss on, the tasks written in the meeting, oldest first.
    pub tasks: Vec<TaskDto>,
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
    fn new(
        view: MeetingView,
        online: &HashSet<Uuid>,
        reflections: ReflectionsSnapshot,
        live: (WritingCounts, Vec<DragDto>),
        groups: Vec<GroupDto>,
        votes: VotesSnapshot,
        discuss: DiscussSnapshot,
    ) -> Self {
        let participants = participants(&view, online);
        let m = view.meeting;
        let timer = Timer::from_json(m.timer_state.as_ref());
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
                pronunciation: m.greeting_pronunciation,
            },
            question: m.icebreaker_question,
            icebreaker: IcebreakerDto {
                order: m.icebreaker_order,
                index: m.icebreaker_index,
            },
            task_count: view.task_count,
            timer,
            reflections: reflections
                .reflections
                .into_iter()
                .map(|r| ReflectionDto {
                    id: r.model.id,
                    column: r.model.column,
                    content: r.model.content,
                    mine: r.mine,
                    group_id: r.model.group_id,
                    position: r.model.position,
                    version: r.model.version,
                    created_at: r.model.created_at,
                    updated_at: r.model.updated_at,
                })
                .collect(),
            reflection_counts: reflections.counts.into(),
            writing: live.0.into(),
            groups,
            drags: live.1,
            vote_budget: m.vote_budget,
            vote_max_per_group: m.vote_max_per_group,
            my_votes: votes.mine,
            votes_cast: votes.cast,
            discuss_order: discuss.order,
            discuss_group_id: discuss.current,
            reactions: discuss.reactions,
            tasks: discuss.tasks.into_iter().map(TaskDto::from).collect(),
            seq: m.event_seq,
            created_at: m.created_at,
            ended_at: m.ended_at,
            server_now: OffsetDateTime::now_utc(),
        }
    }
}

/// The Discuss part of a state, as its viewer may see it.
struct DiscussSnapshot {
    order: Vec<DiscussGroupDto>,
    current: Option<Uuid>,
    reactions: Vec<ReactionDto>,
    tasks: Vec<TaskView>,
}

/// The votes part of a state, as its viewer may see it.
struct VotesSnapshot {
    mine: Vec<MyVoteDto>,
    cast: Option<i32>,
}

/// The meeting's current state, as `actor` sees it. Its reads share one
/// snapshot, so cards never point at a group a concurrent change removed.
pub(crate) async fn state_of(
    state: &AppState,
    meeting_id: Uuid,
    actor: Uuid,
) -> AppResult<MeetingStateDto> {
    let txn = state
        .db
        .begin_with_config(
            Some(IsolationLevel::RepeatableRead),
            Some(AccessMode::ReadOnly),
        )
        .await?;
    let view = meetings::view(&txn, meeting_id, actor).await?;
    let reflections = reflections::snapshot(&txn, &view.meeting, actor).await?;
    let groups = groups::snapshot(&txn, &view.meeting).await?;
    let votes = VotesSnapshot {
        mine: votes::my_votes(&txn, &view.meeting, actor).await?,
        cast: votes::votes_cast(&txn, &view.meeting).await?,
    };
    let (order, current) = discuss::snapshot(&txn, &view.meeting).await?;
    let discuss = DiscussSnapshot {
        order,
        current,
        reactions: reactions::snapshot(&txn, &view.meeting, actor).await?,
        tasks: tasks::of_meeting(&txn, &view.meeting).await?,
    };
    txn.commit().await?;
    let online = state.hub.online(meeting_id);
    // An ended meeting has no editors open, whatever sockets still say.
    let live = if view.meeting.status == MeetingStatus::Active {
        (state.hub.writing(meeting_id), state.hub.drags(meeting_id))
    } else {
        (WritingCounts::default(), Vec::new())
    };
    Ok(MeetingStateDto::new(
        view,
        &online,
        reflections,
        live,
        groups,
        votes,
        discuss,
    ))
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

/// An ended meeting's read-only summary (spec §8.3).
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EndedMeetingSummaryDto {
    pub id: Uuid,
    pub team_id: Uuid,
    pub team_name: String,
    pub title: String,
    pub facilitator: PersonDto,
    /// In joining order, including people who left the team since.
    pub participants: Vec<PersonDto>,
    /// Votes cast overall; `null` when the meeting ended before Discuss:
    /// nobody saw the totals.
    pub votes_cast: Option<i32>,
    /// Most votes first (Discuss order). Empty when the meeting ended
    /// during Reflect: its reflections were never revealed. Ended during
    /// Group or Vote: by size, then id, with no totals.
    pub groups: Vec<SummaryGroupDto>,
    /// The tasks written in the meeting, oldest first, with their current
    /// board status.
    pub tasks: Vec<TaskDto>,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub ended_at: OffsetDateTime,
}

impl EndedMeetingSummaryDto {
    fn new(summary: MeetingSummary) -> AppResult<Self> {
        let MeetingSummary {
            view,
            votes_cast,
            groups,
            tasks,
        } = summary;
        let ended_at = view
            .meeting
            .ended_at
            .ok_or_else(|| AppError::Internal(anyhow::anyhow!("ended meeting without ended_at")))?;
        Ok(Self {
            id: view.meeting.id,
            team_id: view.meeting.team_id,
            team_name: view.team_name,
            title: view.meeting.title,
            facilitator: view.facilitator.into(),
            participants: view
                .participants
                .into_iter()
                .map(|p| p.user.into())
                .collect(),
            votes_cast,
            groups,
            tasks: tasks.into_iter().map(Into::into).collect(),
            created_at: view.meeting.created_at,
            ended_at,
        })
    }
}

/// The summary of an ended meeting: facts, groups by votes with their
/// anonymous reflections, and its tasks with their current status.
#[utoipa::path(
    get,
    path = "/api/v1/meetings/{meeting_id}/summary",
    tag = "meetings",
    security(("bearer" = [])),
    params(("meeting_id" = Uuid, Path, description = "Meeting id")),
    responses(
        (status = 200, body = EndedMeetingSummaryDto, description = "The summary"),
        (status = 401, body = Problem, content_type = "application/problem+json", description = "Not signed in"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`meeting_not_found`: no such meeting or not a team member"),
        (status = 409, body = Problem, content_type = "application/problem+json", description = "`meeting_active`: the meeting has not ended"),
    )
)]
pub async fn get_meeting_summary(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(meeting_id): Path<Uuid>,
) -> AppResult<Json<EndedMeetingSummaryDto>> {
    let summary = summary::of_meeting(&state.db, meeting_id, user.id).await?;
    Ok(Json(EndedMeetingSummaryDto::new(summary)?))
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

/// Joins a running meeting of the team as a participant (idempotent).
#[utoipa::path(
    post,
    path = "/api/v1/teams/{team_id}/meetings/{meeting_id}/join",
    tag = "meetings",
    security(("bearer" = [])),
    params(
        ("team_id" = Uuid, Path, description = "Team id"),
        ("meeting_id" = Uuid, Path, description = "Meeting id"),
    ),
    responses(
        (status = 200, body = MeetingStateDto, description = "The meeting, with the caller as a participant"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`meeting_not_found`: no such meeting, not a team member, or a meeting of another team"),
        (status = 409, body = Problem, content_type = "application/problem+json", description = "`meeting_ended`"),
    )
)]
pub async fn join_meeting(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path((team_id, meeting_id)): Path<(Uuid, Uuid)>,
) -> AppResult<Json<MeetingStateDto>> {
    meetings::join(&state.db, team_id, meeting_id, user.id).await?;
    Ok(Json(state_of(&state, meeting_id, user.id).await?))
}

/// Moves the icebreaker to the next person (facilitator).
#[utoipa::path(
    post,
    path = "/api/v1/meetings/{meeting_id}/icebreaker/next",
    tag = "meetings",
    security(("bearer" = [])),
    params(("meeting_id" = Uuid, Path, description = "Meeting id")),
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
) -> AppResult<Json<MeetingStateDto>> {
    meetings::icebreaker_next(&state.db, meeting_id, user.id).await?;
    Ok(Json(state_of(&state, meeting_id, user.id).await?))
}

/// Replaces the icebreaker question with another one (facilitator).
#[utoipa::path(
    post,
    path = "/api/v1/meetings/{meeting_id}/icebreaker/question",
    tag = "meetings",
    security(("bearer" = [])),
    params(("meeting_id" = Uuid, Path, description = "Meeting id")),
    responses(
        (status = 200, body = MeetingStateDto, description = "The meeting, with its new question"),
        (status = 403, body = Problem, content_type = "application/problem+json", description = "`not_facilitator`"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`meeting_not_found`"),
        (status = 409, body = Problem, content_type = "application/problem+json", description = "`meeting_ended` or `wrong_phase`"),
    )
)]
pub async fn icebreaker_question(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(meeting_id): Path<Uuid>,
) -> AppResult<Json<MeetingStateDto>> {
    meetings::icebreaker_change_question(&state.db, meeting_id, user.id).await?;
    Ok(Json(state_of(&state, meeting_id, user.id).await?))
}

/// Who the icebreaker goes back to.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IcebreakerGotoRequest {
    /// Someone who already had their turn.
    pub user_id: Uuid,
}

/// Goes back to someone who already had their turn (facilitator); the turn
/// continues from them.
#[utoipa::path(
    post,
    path = "/api/v1/meetings/{meeting_id}/icebreaker/goto",
    tag = "meetings",
    security(("bearer" = [])),
    params(("meeting_id" = Uuid, Path, description = "Meeting id")),
    request_body = IcebreakerGotoRequest,
    responses(
        (status = 200, body = MeetingStateDto, description = "The meeting"),
        (status = 403, body = Problem, content_type = "application/problem+json", description = "`not_facilitator`"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`meeting_not_found`"),
        (status = 409, body = Problem, content_type = "application/problem+json", description = "`meeting_ended`, `wrong_phase`, or `icebreaker_not_reached` (they have not had their turn yet)"),
    )
)]
pub async fn icebreaker_goto(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(meeting_id): Path<Uuid>,
    Json(body): Json<IcebreakerGotoRequest>,
) -> AppResult<Json<MeetingStateDto>> {
    meetings::icebreaker_goto(&state.db, meeting_id, user.id, body.user_id).await?;
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

/// Sets, pauses, resumes, extends or stops the timer (facilitator, any phase).
#[utoipa::path(
    put,
    path = "/api/v1/meetings/{meeting_id}/timer",
    tag = "meetings",
    security(("bearer" = [])),
    params(("meeting_id" = Uuid, Path, description = "Meeting id")),
    request_body = TimerAction,
    responses(
        (status = 200, body = MeetingStateDto, description = "The meeting, with its timer"),
        (status = 400, body = Problem, content_type = "application/problem+json", description = "`invalid_duration`: start outside 1 s–99:59, or add outside 1 s–1 h"),
        (status = 403, body = Problem, content_type = "application/problem+json", description = "`not_facilitator`"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`meeting_not_found`"),
        (status = 409, body = Problem, content_type = "application/problem+json", description = "`meeting_ended`, or `no_timer` (pause, resume or add with no timer set)"),
    )
)]
pub async fn set_timer(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(meeting_id): Path<Uuid>,
    Json(body): Json<TimerAction>,
) -> AppResult<Json<MeetingStateDto>> {
    meetings::set_timer(&state.db, meeting_id, user.id, body).await?;
    Ok(Json(state_of(&state, meeting_id, user.id).await?))
}

/// The caller's ready flag.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetReadyRequest {
    pub ready: bool,
}

/// Sets the caller's own ready flag (participants, in Reflect, Group and Vote).
#[utoipa::path(
    put,
    path = "/api/v1/meetings/{meeting_id}/ready",
    tag = "meetings",
    security(("bearer" = [])),
    params(("meeting_id" = Uuid, Path, description = "Meeting id")),
    request_body = SetReadyRequest,
    responses(
        (status = 200, body = MeetingStateDto, description = "The meeting"),
        (status = 403, body = Problem, content_type = "application/problem+json", description = "`not_participant`"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`meeting_not_found`"),
        (status = 409, body = Problem, content_type = "application/problem+json", description = "`meeting_ended`, or `wrong_phase` (no Ready in this phase)"),
    )
)]
pub async fn set_ready(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(meeting_id): Path<Uuid>,
    Json(body): Json<SetReadyRequest>,
) -> AppResult<Json<MeetingStateDto>> {
    meetings::set_ready(&state.db, meeting_id, user.id, body.ready).await?;
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
