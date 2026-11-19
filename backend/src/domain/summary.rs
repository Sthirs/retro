//! The summary of an ended meeting (spec §8.3): what it was, who was there,
//! its groups by votes with their anonymous reflections, and the tasks
//! written in it with their current board status. Read-only.

use std::collections::HashMap;

use sea_orm::{
    AccessMode, ColumnTrait, ConnectionTrait, DbBackend, EntityTrait, FromQueryResult,
    IsolationLevel, QueryFilter, Statement, TransactionSession, TransactionTrait,
};
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    domain::{
        discuss::{Ranked, rank},
        meetings::{self, MeetingView},
        tasks::{self, TaskView},
        votes,
    },
    entities::{
        meeting::{MeetingPhase, MeetingStatus},
        reflection::{self, ReflectionColumn},
        reflection_group,
    },
    error::AppError,
};

/// A reflection as the summary shows it: anonymous.
#[derive(Debug, Clone, Serialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SummaryReflectionDto {
    pub id: Uuid,
    pub column: ReflectionColumn,
    /// Sanitized rich text.
    pub content: String,
}

/// A group with its vote total and its reflections. A card that was never
/// grouped is a group of one, with no name.
#[derive(Debug, Clone, Serialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SummaryGroupDto {
    /// The group's id, or the card's for a card that was never grouped.
    pub id: Uuid,
    /// Empty when unnamed ("Untitled group", or a lone card's text).
    pub name: String,
    /// `null` when the meeting ended before Discuss: nobody saw the totals.
    pub votes: Option<i32>,
    /// By column (Plus, Delta, Ideas, Thanks), then place.
    pub reflections: Vec<SummaryReflectionDto>,
}

/// An ended meeting's summary.
#[derive(Debug, Clone)]
pub struct MeetingSummary {
    pub view: MeetingView,
    /// `None` when the meeting ended before Discuss.
    pub votes_cast: Option<i32>,
    /// In Discuss order: most votes first (by size, then id, when the meeting
    /// ended before Discuss).
    pub groups: Vec<SummaryGroupDto>,
    /// Oldest first.
    pub tasks: Vec<TaskView>,
}

#[derive(Debug, FromQueryResult)]
struct VoteCount {
    group_id: Uuid,
    votes: i64,
}

/// The summary of an ended meeting, for a member of its team
/// (`meeting_not_found` otherwise); `meeting_active` while it runs.
pub async fn of_meeting(
    db: &impl TransactionTrait,
    meeting_id: Uuid,
    actor: Uuid,
) -> Result<MeetingSummary, AppError> {
    let txn = db
        .begin_with_config(
            Some(IsolationLevel::RepeatableRead),
            Some(AccessMode::ReadOnly),
        )
        .await?;
    // The summary loads its tasks and groups itself: no counts needed.
    let view = meetings::view_without_counts(&txn, meeting_id, actor).await?;
    if view.meeting.status == MeetingStatus::Active {
        return Err(AppError::Conflict("meeting_active"));
    }
    // A meeting ended during Reflect never revealed its reflections, and
    // one ended before Discuss never revealed its vote totals (§9.4).
    let reveal_votes = view.meeting.phase >= MeetingPhase::Discuss;
    let groups = if view.meeting.phase > MeetingPhase::Reflect {
        groups_of(&txn, meeting_id, reveal_votes).await?
    } else {
        Vec::new()
    };
    let votes_cast = if reveal_votes {
        Some(votes::count_cast(&txn, meeting_id).await?)
    } else {
        None
    };
    let tasks = tasks::written_in(&txn, meeting_id).await?;
    txn.commit().await?;
    Ok(MeetingSummary {
        view,
        votes_cast,
        groups,
        tasks,
    })
}

async fn groups_of(
    db: &impl ConnectionTrait,
    meeting_id: Uuid,
    reveal_votes: bool,
) -> Result<Vec<SummaryGroupDto>, AppError> {
    let names: HashMap<Uuid, String> = reflection_group::Entity::find()
        .filter(reflection_group::Column::MeetingId.eq(meeting_id))
        .all(db)
        .await?
        .into_iter()
        .map(|g| (g.id, g.name))
        .collect();
    let cards = reflection::Entity::find()
        .filter(reflection::Column::MeetingId.eq(meeting_id))
        .all(db)
        .await?;
    if !reveal_votes {
        return Ok(arrange(&names, cards, None));
    }
    let votes = VoteCount::find_by_statement(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT group_id, COUNT(*) AS votes FROM votes WHERE meeting_id = $1 GROUP BY group_id",
        [meeting_id.into()],
    ))
    .all(db)
    .await?
    .into_iter()
    .map(|v| (v.group_id, v.votes))
    .collect();
    Ok(arrange(&names, cards, Some(&votes)))
}

/// Sorts the cards into their groups (a never-grouped card into a group of
/// its own) and the groups into Discuss order. Authors are dropped here.
/// Without `votes` (never revealed), groups sort by size, then id, and carry
/// no total.
fn arrange(
    names: &HashMap<Uuid, String>,
    mut cards: Vec<reflection::Model>,
    votes: Option<&HashMap<Uuid, i64>>,
) -> Vec<SummaryGroupDto> {
    cards.sort_by(|a, b| {
        (a.column.index(), a.position, a.created_at, a.id).cmp(&(
            b.column.index(),
            b.position,
            b.created_at,
            b.id,
        ))
    });
    let mut members: HashMap<Uuid, Vec<SummaryReflectionDto>> = HashMap::new();
    for card in cards {
        members
            .entry(card.group_id.unwrap_or(card.id))
            .or_default()
            .push(SummaryReflectionDto {
                id: card.id,
                column: card.column,
                content: card.content,
            });
    }
    let mut rows: Vec<Ranked> = members
        .iter()
        .map(|(id, list)| Ranked {
            id: *id,
            name: names.get(id).cloned().unwrap_or_default(),
            votes: votes.and_then(|v| v.get(id).copied()).unwrap_or(0),
            cards: i64::try_from(list.len()).unwrap_or(i64::MAX),
        })
        .collect();
    rank(&mut rows);
    rows.into_iter()
        .map(|r| SummaryGroupDto {
            reflections: members.remove(&r.id).unwrap_or_default(),
            id: r.id,
            name: r.name,
            votes: votes.map(|_| i32::try_from(r.votes).unwrap_or(i32::MAX)),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use time::OffsetDateTime;

    use super::*;

    fn card(
        id: u128,
        column: ReflectionColumn,
        position: i32,
        group: Option<u128>,
    ) -> reflection::Model {
        reflection::Model {
            id: Uuid::from_u128(id),
            meeting_id: Uuid::nil(),
            author_id: Uuid::from_u128(999),
            column,
            content: format!("<p>card {id}</p>"),
            group_id: group.map(Uuid::from_u128),
            position,
            version: 0,
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn cards_go_to_their_groups_and_loose_ones_stand_alone() {
        let names = HashMap::from([(Uuid::from_u128(100), "Pair".to_owned())]);
        let cards = vec![
            card(1, ReflectionColumn::Ideas, 0, Some(100)),
            card(2, ReflectionColumn::Plus, 3, Some(100)),
            card(3, ReflectionColumn::Delta, 0, None),
            card(4, ReflectionColumn::Thanks, 1, None),
        ];
        let votes = HashMap::from([(Uuid::from_u128(4), 2)]);
        let groups = arrange(&names, cards, Some(&votes));
        let ids: Vec<u128> = groups.iter().map(|g| g.id.as_u128()).collect();
        // Most votes first, then the bigger group, then by id.
        assert_eq!(ids, vec![4, 100, 3]);
        assert_eq!(groups[0].votes, Some(2));
        assert_eq!(groups[1].name, "Pair");
        assert_eq!(groups[2].name, "");
        // Inside a group: by column, then place.
        let pair: Vec<u128> = groups[1]
            .reflections
            .iter()
            .map(|r| r.id.as_u128())
            .collect();
        assert_eq!(pair, vec![2, 1]);
    }

    #[test]
    fn no_cards_no_groups() {
        assert!(arrange(&HashMap::new(), Vec::new(), Some(&HashMap::new())).is_empty());
    }

    #[test]
    fn unrevealed_votes_sort_by_size_and_show_no_total() {
        let cards = vec![
            card(1, ReflectionColumn::Plus, 0, None),
            card(2, ReflectionColumn::Plus, 1, Some(100)),
            card(3, ReflectionColumn::Delta, 0, Some(100)),
        ];
        let groups = arrange(&HashMap::new(), cards, None);
        let ids: Vec<u128> = groups.iter().map(|g| g.id.as_u128()).collect();
        assert_eq!(ids, vec![100, 1]);
        assert!(groups.iter().all(|g| g.votes.is_none()));
    }
}
