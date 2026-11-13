//! Teams, members and the team join link (spec §5, §6.2, §14.2).

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
    api::UserDto,
    auth::CurrentUser,
    domain::teams::{self, JoinLink, JoinLinkAction, Member, TeamSummary},
    entities::team_member::Role,
    error::{AppResult, Problem},
    state::AppState,
};

/// A team as seen by the caller.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TeamDto {
    pub id: Uuid,
    pub name: String,
    /// The caller's role.
    pub role: Role,
    pub member_count: i64,
}

impl From<TeamSummary> for TeamDto {
    fn from(t: TeamSummary) -> Self {
        Self {
            id: t.id,
            name: t.name,
            role: t.role,
            member_count: t.member_count,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct TeamNameRequest {
    /// 1–60 characters after trimming.
    pub name: String,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemberDto {
    pub user: UserDto,
    pub role: Role,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub joined_at: OffsetDateTime,
}

impl From<Member> for MemberDto {
    fn from(m: Member) -> Self {
        Self {
            user: m.user.into(),
            role: m.role,
            joined_at: m.joined_at,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RoleRequest {
    pub role: Role,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct JoinLinkDto {
    /// Absolute URL of the link; absent if none was ever generated.
    pub url: Option<String>,
    /// Whether opening the link lets people join.
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum JoinLinkActionDto {
    Regenerate,
    Enable,
    Disable,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct JoinLinkRequest {
    pub action: JoinLinkActionDto,
}

fn join_link_dto(state: &AppState, link: JoinLink) -> JoinLinkDto {
    JoinLinkDto {
        url: link
            .token
            .map(|t| state.config.public(&format!("/join/{t}")).to_string()),
        enabled: link.enabled,
    }
}

/// Lists the caller's teams.
#[utoipa::path(
    get,
    path = "/api/v1/teams",
    tag = "teams",
    security(("bearer" = [])),
    responses(
        (status = 200, body = Vec<TeamDto>, description = "The caller's teams, by name"),
        (status = 401, body = Problem, content_type = "application/problem+json", description = "Not signed in"),
    )
)]
pub async fn list_teams(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> AppResult<Json<Vec<TeamDto>>> {
    let teams = teams::list_for_user(&state.db, user.id).await?;
    Ok(Json(teams.into_iter().map(Into::into).collect()))
}

/// Creates a team. The caller becomes its first admin.
#[utoipa::path(
    post,
    path = "/api/v1/teams",
    tag = "teams",
    security(("bearer" = [])),
    request_body = TeamNameRequest,
    responses(
        (status = 201, body = TeamDto, description = "The new team"),
        (status = 400, body = Problem, content_type = "application/problem+json", description = "`invalid_team_name`"),
        (status = 401, body = Problem, content_type = "application/problem+json", description = "Not signed in"),
    )
)]
pub async fn create_team(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(body): Json<TeamNameRequest>,
) -> AppResult<(StatusCode, Json<TeamDto>)> {
    let team = teams::create(&state.db, user.id, &body.name).await?;
    Ok((StatusCode::CREATED, Json(team.into())))
}

/// Returns one of the caller's teams.
#[utoipa::path(
    get,
    path = "/api/v1/teams/{team_id}",
    tag = "teams",
    security(("bearer" = [])),
    params(("team_id" = Uuid, Path, description = "Team id")),
    responses(
        (status = 200, body = TeamDto, description = "The team"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`team_not_found`: no such team or not a member"),
    )
)]
pub async fn get_team(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(team_id): Path<Uuid>,
) -> AppResult<Json<TeamDto>> {
    Ok(Json(
        teams::summary(&state.db, team_id, user.id).await?.into(),
    ))
}

/// Renames a team (admin).
#[utoipa::path(
    patch,
    path = "/api/v1/teams/{team_id}",
    tag = "teams",
    security(("bearer" = [])),
    params(("team_id" = Uuid, Path, description = "Team id")),
    request_body = TeamNameRequest,
    responses(
        (status = 200, body = TeamDto, description = "The renamed team"),
        (status = 400, body = Problem, content_type = "application/problem+json", description = "`invalid_team_name`"),
        (status = 403, body = Problem, content_type = "application/problem+json", description = "Not an admin"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`team_not_found`"),
    )
)]
pub async fn rename_team(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(team_id): Path<Uuid>,
    Json(body): Json<TeamNameRequest>,
) -> AppResult<Json<TeamDto>> {
    Ok(Json(
        teams::rename(&state.db, team_id, user.id, &body.name)
            .await?
            .into(),
    ))
}

/// Deletes a team and everything in it (admin).
#[utoipa::path(
    delete,
    path = "/api/v1/teams/{team_id}",
    tag = "teams",
    security(("bearer" = [])),
    params(("team_id" = Uuid, Path, description = "Team id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 403, body = Problem, content_type = "application/problem+json", description = "Not an admin"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`team_not_found`"),
    )
)]
pub async fn delete_team(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(team_id): Path<Uuid>,
) -> AppResult<StatusCode> {
    teams::delete(&state.db, team_id, user.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Lists the members of a team, admins first.
#[utoipa::path(
    get,
    path = "/api/v1/teams/{team_id}/members",
    tag = "teams",
    security(("bearer" = [])),
    params(("team_id" = Uuid, Path, description = "Team id")),
    responses(
        (status = 200, body = Vec<MemberDto>, description = "Members"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`team_not_found`"),
    )
)]
pub async fn list_members(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(team_id): Path<Uuid>,
) -> AppResult<Json<Vec<MemberDto>>> {
    let members = teams::members(&state.db, team_id, user.id).await?;
    Ok(Json(members.into_iter().map(Into::into).collect()))
}

/// Promotes or demotes a member (admin). The last admin cannot be demoted.
#[utoipa::path(
    patch,
    path = "/api/v1/teams/{team_id}/members/{user_id}",
    tag = "teams",
    security(("bearer" = [])),
    params(
        ("team_id" = Uuid, Path, description = "Team id"),
        ("user_id" = Uuid, Path, description = "Member's user id"),
    ),
    request_body = RoleRequest,
    responses(
        (status = 204, description = "Role changed"),
        (status = 403, body = Problem, content_type = "application/problem+json", description = "Not an admin"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`team_not_found` or `member_not_found`"),
        (status = 409, body = Problem, content_type = "application/problem+json", description = "`last_admin`"),
    )
)]
pub async fn update_member(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path((team_id, user_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<RoleRequest>,
) -> AppResult<StatusCode> {
    teams::change_role(&state.db, team_id, user.id, user_id, body.role).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Removes a member (admin) or leaves the team (own user id). When the last
/// member leaves, the team is deleted.
#[utoipa::path(
    delete,
    path = "/api/v1/teams/{team_id}/members/{user_id}",
    tag = "teams",
    security(("bearer" = [])),
    params(
        ("team_id" = Uuid, Path, description = "Team id"),
        ("user_id" = Uuid, Path, description = "Member's user id"),
    ),
    responses(
        (status = 204, description = "Removed"),
        (status = 403, body = Problem, content_type = "application/problem+json", description = "Not an admin"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`team_not_found` or `member_not_found`"),
        (status = 409, body = Problem, content_type = "application/problem+json", description = "`last_admin`"),
    )
)]
pub async fn remove_member(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path((team_id, user_id)): Path<(Uuid, Uuid)>,
) -> AppResult<StatusCode> {
    teams::remove_member(&state.db, team_id, user.id, user_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Returns the team join link (any member).
#[utoipa::path(
    get,
    path = "/api/v1/teams/{team_id}/join-link",
    tag = "teams",
    security(("bearer" = [])),
    params(("team_id" = Uuid, Path, description = "Team id")),
    responses(
        (status = 200, body = JoinLinkDto, description = "The join link"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`team_not_found`"),
    )
)]
pub async fn get_join_link(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(team_id): Path<Uuid>,
) -> AppResult<Json<JoinLinkDto>> {
    let link = teams::join_link(&state.db, team_id, user.id).await?;
    Ok(Json(join_link_dto(&state, link)))
}

/// Regenerates, enables or disables the join link (admin).
#[utoipa::path(
    post,
    path = "/api/v1/teams/{team_id}/join-link",
    tag = "teams",
    security(("bearer" = [])),
    params(("team_id" = Uuid, Path, description = "Team id")),
    request_body = JoinLinkRequest,
    responses(
        (status = 200, body = JoinLinkDto, description = "The updated join link"),
        (status = 403, body = Problem, content_type = "application/problem+json", description = "Not an admin"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`team_not_found`"),
    )
)]
pub async fn update_join_link(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(team_id): Path<Uuid>,
    Json(body): Json<JoinLinkRequest>,
) -> AppResult<Json<JoinLinkDto>> {
    let action = match body.action {
        JoinLinkActionDto::Regenerate => JoinLinkAction::Regenerate,
        JoinLinkActionDto::Enable => JoinLinkAction::Enable,
        JoinLinkActionDto::Disable => JoinLinkAction::Disable,
    };
    let link = teams::update_join_link(&state.db, team_id, user.id, action).await?;
    Ok(Json(join_link_dto(&state, link)))
}
