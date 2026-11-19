//! Votes (spec §9.4): during Vote, every participant spends a budget of
//! votes on groups, at most a set number on the same group. Nobody sees
//! anyone else's votes; everyone sees how many were cast overall.
//!
//! Every vote is a row, so limits are counts and "the most recent votes" a
//! lower limit removes are known exactly: those with the highest `ordinal`,
//! the order the server accepted them in. Changes lock the meeting, which
//! checks the limits transactionally. Each publishes `vote.changed
//! {clientId}` (ids only, ADR-0012) to the voter, whose tabs re-read the
//! state, and `vote.cast_changed {votesCast, clientId}` to everyone, whose
//! tabs take the new total as it is.

use std::collections::BTreeSet;

use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbBackend, EntityTrait, FromQueryResult,
    PaginatorTrait, QueryFilter, QueryOrder, Set, Statement, TransactionSession, TransactionTrait,
};
use serde::Serialize;
use serde_json::json;
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    domain::meetings::{find_for_member, lock_as_facilitator, require_participant},
    entities::{
        meeting::{self, MeetingPhase, MeetingStatus},
        reflection, reflection_group, vote,
    },
    error::AppError,
    ws,
};

/// Bounds of the vote budget a facilitator may set.
pub const BUDGET_RANGE: std::ops::RangeInclusive<i32> = 1..=20;
/// Bounds of the votes one person may give the same group.
pub const MAX_PER_GROUP_RANGE: std::ops::RangeInclusive<i32> = 1..=10;
/// The vote budget of a new meeting (the column default of the migration).
pub const DEFAULT_BUDGET: i32 = 5;
/// The most votes per group of a new meeting (the column default).
pub const DEFAULT_MAX_PER_GROUP: i32 = 3;

/// The viewer's own votes on one group.
#[derive(Debug, Clone, Serialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MyVoteDto {
    pub group_id: Uuid,
    pub count: i32,
}

/// What lower limits would do: how many people would lose votes, never who.
#[derive(Debug, Clone, Serialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VoteImpactDto {
    /// People with at least one vote the limits would remove.
    pub affected: i32,
}

/// One change of a vote: one more, or one fewer.
pub fn validate_delta(delta: i32) -> Result<i32, AppError> {
    match delta {
        1 | -1 => Ok(delta),
        _ => Err(AppError::Invalid("invalid_vote")),
    }
}

/// The settings, or `invalid_vote_settings` when out of bounds.
pub fn validate_settings(budget: i32, max_per_group: i32) -> Result<(i32, i32), AppError> {
    if !BUDGET_RANGE.contains(&budget) || !MAX_PER_GROUP_RANGE.contains(&max_per_group) {
        return Err(AppError::Invalid("invalid_vote_settings"));
    }
    Ok((budget, max_per_group))
}

/// Wraps every card left out of a group in a group of its own, at its place,
/// so votes always go to groups (spec §13). Runs when Vote starts, under the
/// meeting lock: one read and one statement, however many cards are loose.
pub async fn wrap_ungrouped(db: &impl ConnectionTrait, meeting_id: Uuid) -> Result<(), AppError> {
    let loose = reflection::Entity::find()
        .filter(reflection::Column::MeetingId.eq(meeting_id))
        .filter(reflection::Column::GroupId.is_null())
        .order_by_asc(reflection::Column::Column)
        .order_by_asc(reflection::Column::Position)
        .all(db)
        .await?;
    if loose.is_empty() {
        return Ok(());
    }
    // One new group id (UUIDv7, so in card order) per loose card.
    let pairs: Vec<serde_json::Value> = loose
        .iter()
        .map(|card| json!({ "reflection_id": card.id, "group_id": Uuid::now_v7() }))
        .collect();
    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        r#"WITH loose AS (
             SELECT * FROM jsonb_to_recordset($2::jsonb) AS x(reflection_id uuid, group_id uuid)
           ), created AS (
             INSERT INTO reflection_groups (id, meeting_id, "column", position, name, created_at, updated_at)
             SELECT loose.group_id, r.meeting_id, r."column", r.position, '', $3, $3
             FROM loose JOIN reflections r ON r.id = loose.reflection_id
             WHERE r.meeting_id = $1 AND r.group_id IS NULL
             RETURNING id
           )
           UPDATE reflections r SET group_id = loose.group_id
           FROM loose JOIN created ON created.id = loose.group_id
           WHERE r.id = loose.reflection_id"#,
        [
            meeting_id.into(),
            serde_json::Value::Array(pairs).into(),
            OffsetDateTime::now_utc().into(),
        ],
    ))
    .await?;
    Ok(())
}

#[derive(Debug, FromQueryResult)]
struct GroupCount {
    group_id: Uuid,
    count: i64,
}

/// `viewer`'s own votes per group, from Vote on (none before).
pub async fn my_votes(
    db: &impl ConnectionTrait,
    meeting: &meeting::Model,
    viewer: Uuid,
) -> Result<Vec<MyVoteDto>, AppError> {
    if meeting.phase < MeetingPhase::Vote {
        return Ok(Vec::new());
    }
    let rows = GroupCount::find_by_statement(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT group_id, COUNT(*) AS count FROM votes \
         WHERE meeting_id = $1 AND user_id = $2 GROUP BY group_id ORDER BY group_id",
        [meeting.id.into(), viewer.into()],
    ))
    .all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| MyVoteDto {
            group_id: r.group_id,
            count: i32::try_from(r.count).unwrap_or(i32::MAX),
        })
        .collect())
}

#[derive(Debug, FromQueryResult)]
struct CountRow {
    count: i64,
}

/// One person's votes in a meeting: in all, and on one group.
#[derive(Debug, FromQueryResult)]
struct UsageRow {
    total: i64,
    on_group: i64,
}

/// Votes cast overall, as one number (spec §9.4), for everyone while the
/// meeting is in Vote; `None` in the other phases.
pub async fn votes_cast(
    db: &impl ConnectionTrait,
    meeting: &meeting::Model,
) -> Result<Option<i32>, AppError> {
    if meeting.phase != MeetingPhase::Vote {
        return Ok(None);
    }
    Ok(Some(count_cast(db, meeting.id).await?))
}

/// Votes cast overall in a meeting, with no check of who asks.
pub(crate) async fn count_cast(
    db: &impl ConnectionTrait,
    meeting_id: Uuid,
) -> Result<i32, AppError> {
    let cast = vote::Entity::find()
        .filter(vote::Column::MeetingId.eq(meeting_id))
        .count(db)
        .await?;
    Ok(i32::try_from(cast).unwrap_or(i32::MAX))
}

/// How many people `budget` and `max_per_group` would take votes from, as
/// [`trim`] would: any group over the maximum, or more votes left than the
/// budget once those are gone. Facilitator, until Vote ends.
pub async fn impact(
    db: &impl ConnectionTrait,
    meeting_id: Uuid,
    actor: Uuid,
    budget: i32,
    max_per_group: i32,
) -> Result<VoteImpactDto, AppError> {
    let (budget, max_per_group) = validate_settings(budget, max_per_group)?;
    let (meeting, _) = find_for_member(db, meeting_id, actor, false).await?;
    if meeting.status != MeetingStatus::Active {
        return Err(AppError::Conflict("meeting_ended"));
    }
    if meeting.facilitator_id != actor {
        return Err(AppError::Denied("not_facilitator"));
    }
    if meeting.phase > MeetingPhase::Vote {
        return Err(AppError::Conflict("wrong_phase"));
    }
    let row = CountRow::find_by_statement(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT COUNT(*)::bigint AS count FROM ( \
           SELECT bool_or(n > $3) AS over_group, SUM(LEAST(n, $3)) AS kept FROM \
             (SELECT user_id, COUNT(*) AS n FROM votes WHERE meeting_id = $1 \
              GROUP BY user_id, group_id) per_group \
           GROUP BY user_id) per_user \
         WHERE over_group OR kept > $2",
        [
            meeting.id.into(),
            i64::from(budget).into(),
            i64::from(max_per_group).into(),
        ],
    ))
    .one(db)
    .await?;
    Ok(VoteImpactDto {
        affected: row.map_or(0, |r| i32::try_from(r.count).unwrap_or(i32::MAX)),
    })
}

/// Tells `users` that their votes changed (each re-reads its own state
/// unless the event is its own echo), and everyone the new number of votes
/// cast overall, which their tabs take as it is.
async fn publish_changed(
    txn: &impl ConnectionTrait,
    meeting: &meeting::Model,
    users: impl IntoIterator<Item = Uuid>,
    client: Option<Uuid>,
) -> Result<(), AppError> {
    let to: BTreeSet<Uuid> = users.into_iter().collect();
    for user in to {
        ws::publish_user(
            txn,
            meeting.id,
            user,
            "vote.changed",
            json!({ "clientId": client }),
        )
        .await?;
    }
    ws::publish_meeting(
        txn,
        meeting.id,
        "vote.cast_changed",
        json!({ "votesCast": count_cast(txn, meeting.id).await?, "clientId": client }),
    )
    .await?;
    Ok(())
}

/// Gives a group one more vote (`delta` 1) or takes one back (-1): the
/// caller's most recent vote on it. Participants, during Vote. Returns the
/// meeting.
pub async fn vote(
    db: &(impl TransactionTrait + ConnectionTrait),
    group_id: Uuid,
    actor: Uuid,
    delta: i32,
    client: Option<Uuid>,
) -> Result<Uuid, AppError> {
    let delta = validate_delta(delta)?;
    let found = reflection_group::Entity::find_by_id(group_id)
        .one(db)
        .await?
        .ok_or(AppError::NotFound("group_not_found"))?;
    let txn = db.begin().await?;
    let (meeting, _) = find_for_member(&txn, found.meeting_id, actor, true)
        .await
        .map_err(|e| match e {
            AppError::NotFound(_) => AppError::NotFound("group_not_found"),
            other => other,
        })?;
    // Read again under the meeting lock: the group may have been dissolved
    // (in Group) between the first read and the lock.
    reflection_group::Entity::find_by_id(group_id)
        .filter(reflection_group::Column::MeetingId.eq(meeting.id))
        .one(&txn)
        .await?
        .ok_or(AppError::NotFound("group_not_found"))?;
    if meeting.status != MeetingStatus::Active {
        return Err(AppError::Conflict("meeting_ended"));
    }
    if meeting.phase != MeetingPhase::Vote {
        return Err(AppError::Conflict("wrong_phase"));
    }
    require_participant(&txn, meeting.id, actor).await?;
    if delta > 0 {
        let usage = UsageRow::find_by_statement(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT COUNT(*) AS total, COUNT(*) FILTER (WHERE group_id = $3) AS on_group \
             FROM votes WHERE meeting_id = $1 AND user_id = $2",
            [meeting.id.into(), actor.into(), group_id.into()],
        ))
        .one(&txn)
        .await?
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("vote usage: no row")))?;
        if usage.total >= i64::from(meeting.vote_budget) {
            return Err(AppError::Conflict("vote_budget_exhausted"));
        }
        if usage.on_group >= i64::from(meeting.vote_max_per_group) {
            return Err(AppError::Conflict("vote_group_limit"));
        }
        vote::ActiveModel {
            id: Set(Uuid::now_v7()),
            meeting_id: Set(meeting.id),
            user_id: Set(actor),
            group_id: Set(group_id),
            created_at: Set(OffsetDateTime::now_utc()),
            ..Default::default()
        }
        .insert(&txn)
        .await?;
    } else {
        let latest = vote::Entity::find()
            .filter(vote::Column::MeetingId.eq(meeting.id))
            .filter(vote::Column::UserId.eq(actor))
            .filter(vote::Column::GroupId.eq(group_id))
            .order_by_desc(vote::Column::Ordinal)
            .one(&txn)
            .await?
            .ok_or(AppError::Conflict("no_vote_to_remove"))?;
        vote::Entity::delete_by_id(latest.id).exec(&txn).await?;
    }
    publish_changed(&txn, &meeting, [actor], client).await?;
    txn.commit().await?;
    Ok(meeting.id)
}

#[derive(Debug, FromQueryResult)]
struct TrimmedRow {
    user_id: Uuid,
}

/// Removes, for every voter, the most recent votes beyond the limits: first
/// those over `max_per_group` on a group, then those over `budget`. Returns
/// whose votes were removed.
async fn trim(
    txn: &impl ConnectionTrait,
    meeting_id: Uuid,
    budget: i32,
    max_per_group: i32,
) -> Result<BTreeSet<Uuid>, AppError> {
    let mut trimmed = BTreeSet::new();
    for (partition, limit) in [("user_id, group_id", max_per_group), ("user_id", budget)] {
        let rows = TrimmedRow::find_by_statement(Statement::from_sql_and_values(
            DbBackend::Postgres,
            format!(
                "DELETE FROM votes v USING ( \
                   SELECT id, row_number() OVER (PARTITION BY {partition} \
                                                 ORDER BY ordinal) AS n \
                   FROM votes WHERE meeting_id = $1) ranked \
                 WHERE v.id = ranked.id AND ranked.n > $2 \
                 RETURNING v.user_id"
            ),
            [meeting_id.into(), limit.into()],
        ))
        .all(txn)
        .await?;
        trimmed.extend(rows.into_iter().map(|r| r.user_id));
    }
    Ok(trimmed)
}

/// Sets the vote budget and the most votes per group (facilitator, until
/// Vote ends). Lower limits remove each voter's most recent votes beyond
/// them. Publishes `vote.settings_changed` to everyone.
pub async fn update_settings(
    db: &(impl TransactionTrait + ConnectionTrait),
    meeting_id: Uuid,
    actor: Uuid,
    budget: i32,
    max_per_group: i32,
    client: Option<Uuid>,
) -> Result<(), AppError> {
    let (budget, max_per_group) = validate_settings(budget, max_per_group)?;
    let txn = db.begin().await?;
    let meeting = lock_as_facilitator(&txn, meeting_id, actor).await?;
    if meeting.phase > MeetingPhase::Vote {
        return Err(AppError::Conflict("wrong_phase"));
    }
    if meeting.vote_budget == budget && meeting.vote_max_per_group == max_per_group {
        txn.commit().await?;
        return Ok(());
    }
    meeting::ActiveModel {
        id: Set(meeting.id),
        vote_budget: Set(budget),
        vote_max_per_group: Set(max_per_group),
        ..Default::default()
    }
    .update(&txn)
    .await?;
    let trimmed = trim(&txn, meeting.id, budget, max_per_group).await?;
    ws::publish_meeting(
        &txn,
        meeting.id,
        "vote.settings_changed",
        json!({ "budget": budget, "maxPerGroup": max_per_group, "clientId": client }),
    )
    .await?;
    if !trimmed.is_empty() {
        publish_changed(&txn, &meeting, trimmed, client).await?;
    }
    txn.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_vote_changes_by_one() {
        assert_eq!(validate_delta(1).unwrap(), 1);
        assert_eq!(validate_delta(-1).unwrap(), -1);
        for delta in [0, 2, -2, i32::MAX] {
            assert!(matches!(
                validate_delta(delta),
                Err(AppError::Invalid("invalid_vote"))
            ));
        }
    }

    #[test]
    fn settings_stay_within_bounds() {
        assert_eq!(validate_settings(5, 3).unwrap(), (5, 3));
        assert_eq!(validate_settings(1, 1).unwrap(), (1, 1));
        assert_eq!(validate_settings(20, 10).unwrap(), (20, 10));
        for (budget, max) in [(0, 3), (21, 3), (5, 0), (5, 11), (-1, -1)] {
            assert!(matches!(
                validate_settings(budget, max),
                Err(AppError::Invalid("invalid_vote_settings"))
            ));
        }
    }

    #[test]
    fn dtos_serialize_in_camel_case() {
        let id = Uuid::nil();
        assert_eq!(
            serde_json::to_value(MyVoteDto {
                group_id: id,
                count: 2
            })
            .unwrap(),
            json!({ "groupId": id, "count": 2 })
        );
        assert_eq!(
            serde_json::to_value(VoteImpactDto { affected: 2 }).unwrap(),
            json!({ "affected": 2 })
        );
    }
}
