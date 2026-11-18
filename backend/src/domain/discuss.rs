//! Discuss (spec §9.5): the groups in vote order, one of them current at a
//! time. Only the facilitator changes the current group; everyone follows it
//! (`discuss.group_changed`). Reactions and tasks go to the current group
//! only: a group someone previews is read-only.

use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbBackend, EntityTrait, FromQueryResult,
    QueryFilter, Set, Statement, TransactionSession, TransactionTrait,
};
use serde::Serialize;
use serde_json::json;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    domain::{
        meetings::{find_for_member, lock_as_facilitator, require_participant},
        tasks::{self, Origin},
    },
    entities::{
        meeting::{self, MeetingPhase, MeetingStatus},
        reflection_group,
        task::{self, TaskStatus},
    },
    error::AppError,
    ws,
};

/// A group in Discuss order, with its vote total.
#[derive(Debug, Clone, Serialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DiscussGroupDto {
    pub group_id: Uuid,
    /// Votes the group got, everyone's (revealed in Discuss).
    pub votes: i32,
    /// Cards in the group.
    pub cards: i32,
}

/// What `order` sorts: a group with its totals and name.
#[derive(Debug, Clone, FromQueryResult)]
struct Ranked {
    id: Uuid,
    name: String,
    votes: i64,
    cards: i64,
}

/// Most votes first; ties go to the bigger group, then by name (named
/// groups before untitled ones, case-insensitively), then by id, which
/// follows the order the groups were made in.
fn rank(rows: &mut [Ranked]) {
    rows.sort_by(|a, b| {
        b.votes
            .cmp(&a.votes)
            .then(b.cards.cmp(&a.cards))
            .then_with(|| {
                (a.name.is_empty(), a.name.to_lowercase())
                    .cmp(&(b.name.is_empty(), b.name.to_lowercase()))
            })
            .then(a.id.cmp(&b.id))
    });
}

/// Every group of the meeting with at least one card, in Discuss order.
pub async fn order(
    db: &impl ConnectionTrait,
    meeting_id: Uuid,
) -> Result<Vec<DiscussGroupDto>, AppError> {
    let mut rows = Ranked::find_by_statement(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT g.id, g.name, \
           (SELECT COUNT(*) FROM votes v WHERE v.group_id = g.id) AS votes, \
           (SELECT COUNT(*) FROM reflections r WHERE r.group_id = g.id) AS cards \
         FROM reflection_groups g WHERE g.meeting_id = $1",
        [meeting_id.into()],
    ))
    .all(db)
    .await?;
    rows.retain(|r| r.cards > 0);
    rank(&mut rows);
    Ok(rows
        .into_iter()
        .map(|r| DiscussGroupDto {
            group_id: r.id,
            votes: i32::try_from(r.votes).unwrap_or(i32::MAX),
            cards: i32::try_from(r.cards).unwrap_or(i32::MAX),
        })
        .collect())
}

/// The current group: the one the facilitator chose, else the first.
pub fn current(meeting: &meeting::Model, order: &[DiscussGroupDto]) -> Option<Uuid> {
    meeting
        .discuss_group_id
        .filter(|id| order.iter().any(|g| g.group_id == *id))
        .or_else(|| order.first().map(|g| g.group_id))
}

/// The Discuss part of a state: the order and the current group, from
/// Discuss on (nothing before: totals stay hidden until then).
pub async fn snapshot(
    db: &impl ConnectionTrait,
    meeting: &meeting::Model,
) -> Result<(Vec<DiscussGroupDto>, Option<Uuid>), AppError> {
    if meeting.phase < MeetingPhase::Discuss {
        return Ok((Vec::new(), None));
    }
    let order = order(db, meeting.id).await?;
    let current = current(meeting, &order);
    Ok((order, current))
}

/// Starts Discuss on the group with the most votes. Runs with the phase
/// change, under the meeting lock.
pub async fn start(db: &impl ConnectionTrait, meeting_id: Uuid) -> Result<(), AppError> {
    let first = order(db, meeting_id).await?.first().map(|g| g.group_id);
    meeting::ActiveModel {
        id: Set(meeting_id),
        discuss_group_id: Set(first),
        ..Default::default()
    }
    .update(db)
    .await?;
    Ok(())
}

/// Locks a running meeting in Discuss for a change by `actor`, who must be a
/// participant. A meeting `actor` cannot see answers `missing`.
pub(crate) async fn lock_discussing(
    txn: &impl ConnectionTrait,
    meeting_id: Uuid,
    actor: Uuid,
    missing: &'static str,
) -> Result<meeting::Model, AppError> {
    let (meeting, _) = find_for_member(txn, meeting_id, actor, true)
        .await
        .map_err(|e| match e {
            AppError::NotFound(_) => AppError::NotFound(missing),
            other => other,
        })?;
    if meeting.status != MeetingStatus::Active {
        return Err(AppError::Conflict("meeting_ended"));
    }
    if meeting.phase != MeetingPhase::Discuss {
        return Err(AppError::Conflict("wrong_phase"));
    }
    require_participant(txn, meeting.id, actor).await?;
    Ok(meeting)
}

/// Fails with `not_current_group` unless `group_id` is the current group.
pub(crate) async fn require_current(
    db: &impl ConnectionTrait,
    meeting: &meeting::Model,
    group_id: Option<Uuid>,
) -> Result<(), AppError> {
    let current = match meeting.discuss_group_id {
        Some(id) => Some(id),
        None => order(db, meeting.id).await?.first().map(|g| g.group_id),
    };
    match (group_id, current) {
        (Some(group), Some(current)) if group == current => Ok(()),
        _ => Err(AppError::Conflict("not_current_group")),
    }
}

/// Loads a group of `meeting`; `group_not_found` when it belongs elsewhere.
async fn group_of(
    db: &impl ConnectionTrait,
    meeting_id: Uuid,
    group_id: Uuid,
) -> Result<reflection_group::Model, AppError> {
    reflection_group::Entity::find_by_id(group_id)
        .filter(reflection_group::Column::MeetingId.eq(meeting_id))
        .one(db)
        .await?
        .ok_or(AppError::NotFound("group_not_found"))
}

/// Makes `group_id` the current group (facilitator, during Discuss).
/// Everyone follows: publishes `discuss.group_changed`.
pub async fn set_current(
    db: &(impl TransactionTrait + ConnectionTrait),
    meeting_id: Uuid,
    actor: Uuid,
    group_id: Uuid,
    client: Option<Uuid>,
) -> Result<(), AppError> {
    let txn = db.begin().await?;
    let meeting = lock_as_facilitator(&txn, meeting_id, actor).await?;
    if meeting.phase != MeetingPhase::Discuss {
        return Err(AppError::Conflict("wrong_phase"));
    }
    group_of(&txn, meeting.id, group_id).await?;
    if meeting.discuss_group_id != Some(group_id) {
        meeting::ActiveModel {
            id: Set(meeting.id),
            discuss_group_id: Set(Some(group_id)),
            ..Default::default()
        }
        .update(&txn)
        .await?;
        ws::publish_meeting(
            &txn,
            meeting.id,
            "discuss.group_changed",
            json!({ "groupId": group_id, "clientId": client }),
        )
        .await?;
    }
    txn.commit().await?;
    Ok(())
}

/// Writes a task for the current group (participants, during Discuss). It
/// starts in Active, at the end of the column (spec §9.5).
pub async fn create_task(
    db: &(impl TransactionTrait + ConnectionTrait),
    meeting_id: Uuid,
    actor: Uuid,
    group_id: Uuid,
    content: &str,
    client: Option<Uuid>,
) -> Result<task::Model, AppError> {
    let txn = db.begin().await?;
    let meeting = lock_discussing(&txn, meeting_id, actor, "meeting_not_found").await?;
    group_of(&txn, meeting.id, group_id).await?;
    require_current(&txn, &meeting, Some(group_id)).await?;
    let task = tasks::insert_from(
        &txn,
        meeting.team_id,
        actor,
        TaskStatus::Active,
        content,
        Some(Origin {
            meeting_id: meeting.id,
            group_id,
        }),
        client,
    )
    .await?;
    txn.commit().await?;
    Ok(task)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ranked(id: u128, name: &str, votes: i64, cards: i64) -> Ranked {
        Ranked {
            id: Uuid::from_u128(id),
            name: name.to_owned(),
            votes,
            cards,
        }
    }

    #[test]
    fn groups_rank_by_votes_then_size_then_name() {
        let mut rows = vec![
            ranked(1, "", 3, 1),
            ranked(2, "zebra", 3, 2),
            ranked(3, "Apple", 3, 2),
            ranked(4, "", 3, 2),
            ranked(5, "Most", 9, 1),
            ranked(6, "", 0, 1),
            ranked(7, "apple", 3, 2),
        ];
        rank(&mut rows);
        let ids: Vec<u128> = rows.iter().map(|r| r.id.as_u128()).collect();
        assert_eq!(ids, vec![5, 3, 7, 2, 4, 1, 6]);
    }

    #[test]
    fn the_current_group_falls_back_to_the_first() {
        let order = vec![
            DiscussGroupDto {
                group_id: Uuid::from_u128(1),
                votes: 2,
                cards: 1,
            },
            DiscussGroupDto {
                group_id: Uuid::from_u128(2),
                votes: 1,
                cards: 1,
            },
        ];
        let mut meeting = meeting_model();
        assert_eq!(current(&meeting, &order), Some(Uuid::from_u128(1)));
        meeting.discuss_group_id = Some(Uuid::from_u128(2));
        assert_eq!(current(&meeting, &order), Some(Uuid::from_u128(2)));
        meeting.discuss_group_id = Some(Uuid::from_u128(9));
        assert_eq!(current(&meeting, &order), Some(Uuid::from_u128(1)));
        assert_eq!(current(&meeting, &[]), None);
    }

    #[test]
    fn dtos_serialize_in_camel_case() {
        let id = Uuid::nil();
        assert_eq!(
            serde_json::to_value(DiscussGroupDto {
                group_id: id,
                votes: 3,
                cards: 2
            })
            .unwrap(),
            json!({ "groupId": id, "votes": 3, "cards": 2 })
        );
    }

    fn meeting_model() -> meeting::Model {
        let now = time::OffsetDateTime::UNIX_EPOCH;
        meeting::Model {
            id: Uuid::nil(),
            team_id: Uuid::nil(),
            created_by: Uuid::nil(),
            facilitator_id: Uuid::nil(),
            number: 1,
            title: String::new(),
            status: MeetingStatus::Active,
            invite_token: None,
            phase: MeetingPhase::Discuss,
            greeting_text: String::new(),
            greeting_language: String::new(),
            greeting_lang_code: String::new(),
            greeting_pronunciation: String::new(),
            icebreaker_question: String::new(),
            icebreaker_order: Vec::new(),
            icebreaker_index: 0,
            vote_budget: 5,
            vote_max_per_group: 3,
            discuss_group_id: None,
            timer_state: None,
            event_seq: 0,
            created_at: now,
            ended_at: None,
        }
    }
}
