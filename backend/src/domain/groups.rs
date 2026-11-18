//! Groups (spec §9.3): during Group, participants stack related reflections
//! into named groups by dropping cards onto cards and groups.
//!
//! The server is authoritative and last-write-wins: every change locks the
//! meeting, applies to whatever the cards look like by then and raises the
//! moved cards' `version`. Each change publishes `group.created`,
//! `group.updated` or `group.deleted` with the group and the cards whose group
//! changed (ids only, ADR-0011), and the caller's `clientId`.

use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbBackend, EntityTrait, QueryFilter,
    QueryOrder, Set, Statement, TransactionSession, TransactionTrait,
};
use serde::Serialize;
use serde_json::{Value, json};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    domain::meetings::{find_for_member, require_participant},
    entities::{
        meeting::{self, MeetingPhase, MeetingStatus},
        reflection::{self, ReflectionColumn},
        reflection_group,
    },
    error::AppError,
    ws,
};

/// Longest group name, in characters.
pub const NAME_MAX_CHARS: usize = 80;

/// A group as clients see it (snapshot and events).
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroupDto {
    pub id: Uuid,
    /// The column it is shown in: the one of the card it was created on.
    pub column: ReflectionColumn,
    /// Its place in that column, among the ungrouped cards' `position`.
    pub position: i32,
    /// Empty shows as "Untitled group".
    pub name: String,
}

impl From<reflection_group::Model> for GroupDto {
    fn from(g: reflection_group::Model) -> Self {
        Self {
            id: g.id,
            column: g.column,
            position: g.position,
            name: g.name,
        }
    }
}

/// A card whose group changed, as events carry it.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
struct CardMove {
    id: Uuid,
    group_id: Option<Uuid>,
    version: i32,
}

/// The trimmed name, or `invalid_group_name` when it is too long.
pub fn normalize_name(raw: &str) -> Result<String, AppError> {
    let name = raw.trim();
    if name.chars().count() > NAME_MAX_CHARS {
        return Err(AppError::Invalid("invalid_group_name"));
    }
    Ok(name.to_owned())
}

/// Gives every reflection of the meeting its place in its column, in the
/// order they were sent. Runs when Group starts.
pub async fn place_cards(db: &impl ConnectionTrait, meeting_id: Uuid) -> Result<(), AppError> {
    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        r#"UPDATE reflections r SET position = o.rank
           FROM (SELECT id, (row_number() OVER (PARTITION BY "column" ORDER BY created_at, id) - 1)::int AS rank
                 FROM reflections WHERE meeting_id = $1) o
           WHERE r.id = o.id"#,
        [meeting_id.into()],
    ))
    .await?;
    Ok(())
}

/// The meeting's groups, from Group on (none before), by column and place.
pub async fn snapshot(
    db: &impl ConnectionTrait,
    meeting: &meeting::Model,
) -> Result<Vec<GroupDto>, AppError> {
    if meeting.phase < MeetingPhase::Group {
        return Ok(Vec::new());
    }
    Ok(reflection_group::Entity::find()
        .filter(reflection_group::Column::MeetingId.eq(meeting.id))
        .order_by_asc(reflection_group::Column::Column)
        .order_by_asc(reflection_group::Column::Position)
        .order_by_asc(reflection_group::Column::Id)
        .all(db)
        .await?
        .into_iter()
        .map(GroupDto::from)
        .collect())
}

/// Locks a running meeting in Group for a change by `actor`, who must be a
/// participant. A meeting `actor` cannot see answers `missing`, so ids of
/// other teams' cards and groups reveal nothing.
async fn lock_group(
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
    if meeting.phase != MeetingPhase::Group {
        return Err(AppError::Conflict("wrong_phase"));
    }
    require_participant(txn, meeting.id, actor).await?;
    Ok(meeting)
}

/// A card of `meeting_id`, or `reflection_not_found`.
async fn card(
    db: &impl ConnectionTrait,
    meeting_id: Uuid,
    reflection_id: Uuid,
) -> Result<reflection::Model, AppError> {
    reflection::Entity::find_by_id(reflection_id)
        .one(db)
        .await?
        .filter(|r| r.meeting_id == meeting_id)
        .ok_or(AppError::NotFound("reflection_not_found"))
}

/// A group of `meeting_id`, or `group_not_found`.
async fn group(
    db: &impl ConnectionTrait,
    meeting_id: Uuid,
    group_id: Uuid,
) -> Result<reflection_group::Model, AppError> {
    reflection_group::Entity::find_by_id(group_id)
        .one(db)
        .await?
        .filter(|g| g.meeting_id == meeting_id)
        .ok_or(AppError::NotFound("group_not_found"))
}

/// Events of one change, published together before it commits.
struct Events {
    client: Option<Uuid>,
    list: Vec<(&'static str, Value)>,
}

impl Events {
    fn new(client: Option<Uuid>) -> Self {
        Self {
            client,
            list: Vec::new(),
        }
    }

    fn push(&mut self, kind: &'static str, group: Value, cards: &[CardMove]) {
        self.list.push((
            kind,
            json!({ "group": group, "reflections": cards, "clientId": self.client }),
        ));
    }

    async fn publish(self, txn: &impl ConnectionTrait, meeting_id: Uuid) -> Result<(), AppError> {
        for (kind, payload) in self.list {
            ws::publish_meeting(txn, meeting_id, kind, payload).await?;
        }
        Ok(())
    }
}

/// Puts `card` in `group` (or out of any) and raises its version.
async fn set_group(
    txn: &impl ConnectionTrait,
    card: &reflection::Model,
    group: Option<Uuid>,
) -> Result<CardMove, AppError> {
    let version = card.version + 1;
    reflection::ActiveModel {
        id: Set(card.id),
        group_id: Set(group),
        version: Set(version),
        ..Default::default()
    }
    .update(txn)
    .await?;
    Ok(CardMove {
        id: card.id,
        group_id: group,
        version,
    })
}

/// After a card left `group_id`: dissolves the group when a single card is
/// left, publishing `group.deleted` with that card and `also` (the card that
/// left, when no other event reports it). Otherwise reports `also` in
/// `group.updated`.
async fn after_leaving(
    txn: &impl ConnectionTrait,
    meeting_id: Uuid,
    group_id: Uuid,
    also: Option<CardMove>,
    events: &mut Events,
) -> Result<(), AppError> {
    let left = reflection::Entity::find()
        .filter(reflection::Column::MeetingId.eq(meeting_id))
        .filter(reflection::Column::GroupId.eq(group_id))
        .all(txn)
        .await?;
    let mut cards: Vec<CardMove> = also.into_iter().collect();
    if left.len() > 1 {
        if !cards.is_empty() {
            let current = group(txn, meeting_id, group_id).await?;
            events.push("group.updated", json!(GroupDto::from(current)), &cards);
        }
        return Ok(());
    }
    for card in &left {
        cards.push(set_group(txn, card, None).await?);
    }
    reflection_group::Entity::delete_by_id(group_id)
        .exec(txn)
        .await?;
    events.push("group.deleted", json!({ "id": group_id }), &cards);
    Ok(())
}

/// Moves `card` into `target` (an existing group of the meeting), or out of
/// its group when `None`; dissolves the group it leaves if one card is left.
async fn move_locked(
    txn: &impl ConnectionTrait,
    meeting_id: Uuid,
    card: &reflection::Model,
    target: Option<&reflection_group::Model>,
    events: &mut Events,
) -> Result<(), AppError> {
    let target_id = target.map(|g| g.id);
    if card.group_id == target_id {
        return Ok(());
    }
    let moved = set_group(txn, card, target_id).await?;
    if let Some(target) = target {
        events.push(
            "group.updated",
            json!(GroupDto::from(target.clone())),
            &[moved],
        );
    }
    if let Some(old) = card.group_id {
        let also = target.is_none().then_some(moved);
        after_leaving(txn, meeting_id, old, also, events).await?;
    }
    Ok(())
}

/// Drops `reflection_id` onto the card `target_id`: makes a group of both at
/// the target's place, or adds the card to the target's group if it has one
/// by now. Returns the meeting.
pub async fn create(
    db: &(impl TransactionTrait + ConnectionTrait),
    meeting_id: Uuid,
    actor: Uuid,
    target_id: Uuid,
    reflection_id: Uuid,
    client: Option<Uuid>,
) -> Result<Uuid, AppError> {
    if target_id == reflection_id {
        return Err(AppError::Invalid("same_reflection"));
    }
    let txn = db.begin().await?;
    let meeting = lock_group(&txn, meeting_id, actor, "meeting_not_found").await?;
    let target = card(&txn, meeting.id, target_id).await?;
    let dropped = card(&txn, meeting.id, reflection_id).await?;
    let mut events = Events::new(client);
    if let Some(existing) = target.group_id {
        let existing = group(&txn, meeting.id, existing).await?;
        move_locked(&txn, meeting.id, &dropped, Some(&existing), &mut events).await?;
    } else {
        let now = OffsetDateTime::now_utc();
        let created = reflection_group::ActiveModel {
            id: Set(Uuid::now_v7()),
            meeting_id: Set(meeting.id),
            column: Set(target.column),
            position: Set(target.position),
            name: Set(String::new()),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(&txn)
        .await?;
        let cards = [
            set_group(&txn, &target, Some(created.id)).await?,
            set_group(&txn, &dropped, Some(created.id)).await?,
        ];
        events.push("group.created", json!(GroupDto::from(created)), &cards);
        if let Some(old) = dropped.group_id {
            after_leaving(&txn, meeting.id, old, None, &mut events).await?;
        }
    }
    events.publish(&txn, meeting.id).await?;
    txn.commit().await?;
    Ok(meeting.id)
}

/// Moves a card into a group of its meeting, or out of its group with
/// `None`. Returns the meeting.
pub async fn move_card(
    db: &(impl TransactionTrait + ConnectionTrait),
    reflection_id: Uuid,
    actor: Uuid,
    group_id: Option<Uuid>,
    client: Option<Uuid>,
) -> Result<Uuid, AppError> {
    let found = reflection::Entity::find_by_id(reflection_id)
        .one(db)
        .await?
        .ok_or(AppError::NotFound("reflection_not_found"))?;
    let txn = db.begin().await?;
    let meeting = lock_group(&txn, found.meeting_id, actor, "reflection_not_found").await?;
    let current = card(&txn, meeting.id, reflection_id).await?;
    let target = match group_id {
        Some(id) => Some(group(&txn, meeting.id, id).await?),
        None => None,
    };
    let mut events = Events::new(client);
    move_locked(&txn, meeting.id, &current, target.as_ref(), &mut events).await?;
    events.publish(&txn, meeting.id).await?;
    txn.commit().await?;
    Ok(meeting.id)
}

/// Renames a group (any participant; the last name saved wins). Returns the
/// meeting.
pub async fn rename(
    db: &(impl TransactionTrait + ConnectionTrait),
    group_id: Uuid,
    actor: Uuid,
    raw_name: &str,
    client: Option<Uuid>,
) -> Result<Uuid, AppError> {
    let name = normalize_name(raw_name)?;
    let found = reflection_group::Entity::find_by_id(group_id)
        .one(db)
        .await?
        .ok_or(AppError::NotFound("group_not_found"))?;
    let txn = db.begin().await?;
    let meeting = lock_group(&txn, found.meeting_id, actor, "group_not_found").await?;
    // Re-read under the meeting lock: it may have dissolved meanwhile.
    let current = group(&txn, meeting.id, group_id).await?;
    if current.name != name {
        let updated = reflection_group::ActiveModel {
            id: Set(current.id),
            name: Set(name),
            updated_at: Set(OffsetDateTime::now_utc()),
            ..Default::default()
        }
        .update(&txn)
        .await?;
        let mut events = Events::new(client);
        events.push("group.updated", json!(GroupDto::from(updated)), &[]);
        events.publish(&txn, meeting.id).await?;
    }
    txn.commit().await?;
    Ok(meeting.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_trimmed_and_may_be_empty() {
        assert_eq!(normalize_name("  Flaky CI  ").unwrap(), "Flaky CI");
        assert_eq!(normalize_name("   ").unwrap(), "");
    }

    #[test]
    fn names_hold_at_most_80_characters() {
        let longest = "é".repeat(NAME_MAX_CHARS);
        assert_eq!(normalize_name(&longest).unwrap(), longest);
        assert!(matches!(
            normalize_name(&format!("{longest}x")),
            Err(AppError::Invalid("invalid_group_name"))
        ));
    }

    #[test]
    fn card_moves_serialize_in_camel_case() {
        let id = Uuid::nil();
        let card = CardMove {
            id,
            group_id: None,
            version: 2,
        };
        assert_eq!(
            serde_json::to_value(card).unwrap(),
            json!({ "id": id, "groupId": null, "version": 2 })
        );
    }
}
