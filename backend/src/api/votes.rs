//! Votes and their settings (spec §9.4, §14.3, §14.4). Each change answers
//! with the meeting state as the caller sees it: their own votes only.

use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    api::{
        client_id::ClientId,
        meetings::{MeetingStateDto, state_of},
    },
    auth::CurrentUser,
    domain::votes::{self, VoteImpactDto},
    error::{AppResult, Problem},
    state::AppState,
};

/// One more vote, or one fewer.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct VoteRequest {
    /// `1` adds a vote; `-1` takes back the caller's most recent vote on the group.
    pub delta: i32,
}

/// Gives a group one more of the caller's votes, or takes one back
/// (participants, during Vote).
/// Not idempotent (POST): each request is one more vote, or one fewer.
#[utoipa::path(
    post,
    path = "/api/v1/groups/{group_id}/vote",
    tag = "votes",
    security(("bearer" = [])),
    params(("group_id" = Uuid, Path, description = "Group id")),
    request_body = VoteRequest,
    responses(
        (status = 200, body = MeetingStateDto, description = "The meeting, with the caller's votes"),
        (status = 400, body = Problem, content_type = "application/problem+json", description = "`invalid_vote`: `delta` is neither 1 nor -1"),
        (status = 403, body = Problem, content_type = "application/problem+json", description = "`not_participant`"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`group_not_found`"),
        (status = 409, body = Problem, content_type = "application/problem+json", description = "`meeting_ended`, `wrong_phase` (not Vote), `vote_budget_exhausted`, `vote_group_limit` (the most votes on one group), or `no_vote_to_remove`"),
    )
)]
pub async fn vote(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    ClientId(client): ClientId,
    Path(group_id): Path<Uuid>,
    Json(body): Json<VoteRequest>,
) -> AppResult<Json<MeetingStateDto>> {
    let meeting_id = votes::vote(&state.db, group_id, user.id, body.delta, client).await?;
    Ok(Json(state_of(&state, meeting_id, user.id).await?))
}

/// The meeting's vote settings.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct VoteSettingsRequest {
    // Bounds and defaults: the domain's `BUDGET_RANGE`, `DEFAULT_BUDGET`,
    // `MAX_PER_GROUP_RANGE` and `DEFAULT_MAX_PER_GROUP` (a test checks).
    /// Votes per person.
    #[schema(minimum = 1, maximum = 20, default = 5)]
    pub vote_budget: i32,
    /// Most votes one person may give the same group.
    #[schema(minimum = 1, maximum = 10, default = 3)]
    pub vote_max_per_group: i32,
}

/// Sets the vote budget and the most votes per group (facilitator, until
/// Vote ends). Lower limits remove each person's most recent votes beyond
/// them.
#[utoipa::path(
    patch,
    path = "/api/v1/meetings/{meeting_id}/settings",
    tag = "votes",
    security(("bearer" = [])),
    params(("meeting_id" = Uuid, Path, description = "Meeting id")),
    request_body = VoteSettingsRequest,
    responses(
        (status = 200, body = MeetingStateDto, description = "The meeting, with its new settings"),
        (status = 400, body = Problem, content_type = "application/problem+json", description = "`invalid_vote_settings`: out of bounds"),
        (status = 403, body = Problem, content_type = "application/problem+json", description = "`not_facilitator`"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`meeting_not_found`"),
        (status = 409, body = Problem, content_type = "application/problem+json", description = "`meeting_ended`, or `wrong_phase` (after Vote)"),
    )
)]
pub async fn update_settings(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    ClientId(client): ClientId,
    Path(meeting_id): Path<Uuid>,
    Json(body): Json<VoteSettingsRequest>,
) -> AppResult<Json<MeetingStateDto>> {
    votes::update_settings(
        &state.db,
        meeting_id,
        user.id,
        body.vote_budget,
        body.vote_max_per_group,
        client,
    )
    .await?;
    Ok(Json(state_of(&state, meeting_id, user.id).await?))
}

/// Limits to preview.
#[derive(Debug, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct VoteImpactQuery {
    /// Votes per person.
    #[param(minimum = 1, maximum = 20)]
    pub vote_budget: i32,
    /// Most votes one person may give the same group.
    #[param(minimum = 1, maximum = 10)]
    pub vote_max_per_group: i32,
}

/// How many people these limits would take votes from, never who
/// (facilitator, until Vote ends): the warning of the settings dialog.
#[utoipa::path(
    get,
    path = "/api/v1/meetings/{meeting_id}/settings/impact",
    tag = "votes",
    security(("bearer" = [])),
    params(("meeting_id" = Uuid, Path, description = "Meeting id"), VoteImpactQuery),
    responses(
        (status = 200, body = VoteImpactDto, description = "How many people would lose votes"),
        (status = 400, body = Problem, content_type = "application/problem+json", description = "`invalid_vote_settings`: out of bounds"),
        (status = 403, body = Problem, content_type = "application/problem+json", description = "`not_facilitator`"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`meeting_not_found`"),
        (status = 409, body = Problem, content_type = "application/problem+json", description = "`meeting_ended`, or `wrong_phase` (after Vote)"),
    )
)]
pub async fn settings_impact(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(meeting_id): Path<Uuid>,
    Query(query): Query<VoteImpactQuery>,
) -> AppResult<Json<VoteImpactDto>> {
    Ok(Json(
        votes::impact(
            &state.db,
            meeting_id,
            user.id,
            query.vote_budget,
            query.vote_max_per_group,
        )
        .await?,
    ))
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use crate::domain::votes::{
        BUDGET_RANGE, DEFAULT_BUDGET, DEFAULT_MAX_PER_GROUP, MAX_PER_GROUP_RANGE,
    };

    /// The OpenAPI bounds are literals (utoipa needs them so): they must
    /// match the domain's, which the frontend reads from the document.
    #[test]
    fn the_documented_settings_bounds_are_the_domains() {
        let doc: Value = serde_json::from_str(&crate::openapi::json()).unwrap();
        let fields = &doc["components"]["schemas"]["VoteSettingsRequest"]["properties"];
        for (name, range, default) in [
            ("voteBudget", BUDGET_RANGE, DEFAULT_BUDGET),
            (
                "voteMaxPerGroup",
                MAX_PER_GROUP_RANGE,
                DEFAULT_MAX_PER_GROUP,
            ),
        ] {
            let field = &fields[name];
            assert_eq!(field["minimum"], *range.start(), "{name}");
            assert_eq!(field["maximum"], *range.end(), "{name}");
            assert_eq!(field["default"], default, "{name}");
        }
    }
}
