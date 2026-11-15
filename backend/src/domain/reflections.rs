//! Reflections (spec §9.2): written by participants during Reflect, private to
//! their author until Group, anonymous to everyone, always.
//!
//! Every change publishes `reflection.count_changed` to the meeting (counts
//! only) and `reflection.changed` to the author's own sockets (ADR-0012), so
//! their other tabs re-read the state. Content never leaves for anyone else.

use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbBackend, EntityTrait, FromQueryResult,
    PaginatorTrait, QueryFilter, QueryOrder, Set, Statement, TransactionSession, TransactionTrait,
};
use serde_json::{Value, json};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    domain::{
        meetings::{find_for_member, require_participant},
        rich_text::{Profile, sanitize_as},
    },
    entities::{
        meeting::{self, MeetingPhase, MeetingStatus},
        reflection::{self, ReflectionColumn},
    },
    error::AppError,
    ws,
};

/// Most reflections one author may write in a meeting.
pub const MAX_PER_AUTHOR: u64 = 100;

/// Sent reflections per column ([`ReflectionColumn::index`]).
pub type ColumnCounts = [i64; 4];

/// A reflection as one viewer sees it: never its author.
#[derive(Debug, Clone)]
pub struct ReflectionView {
    pub model: reflection::Model,
    /// Written by the viewer.
    pub mine: bool,
}

/// The reflections part of the meeting room, for one viewer.
#[derive(Debug, Clone, Default)]
pub struct ReflectionsSnapshot {
    /// During Reflect the viewer's own; from Group on, everyone's.
    pub reflections: Vec<ReflectionView>,
    pub counts: ColumnCounts,
}

#[derive(Debug, FromQueryResult)]
struct CountRow {
    column: ReflectionColumn,
    count: i64,
}

/// Sent reflections per column of a meeting.
pub async fn counts(db: &impl ConnectionTrait, meeting_id: Uuid) -> Result<ColumnCounts, AppError> {
    let rows = CountRow::find_by_statement(Statement::from_sql_and_values(
        DbBackend::Postgres,
        r#"SELECT "column", COUNT(*) AS count FROM reflections WHERE meeting_id = $1 GROUP BY "column""#,
        [meeting_id.into()],
    ))
    .all(db)
    .await?;
    let mut counts = [0; 4];
    for row in rows {
        counts[row.column.index()] = row.count;
    }
    Ok(counts)
}

/// The client shape of column counts: `{ "plus": 2, "delta": 0, … }`.
pub fn counts_json(counts: &ColumnCounts) -> Value {
    json!({
        "plus": counts[ReflectionColumn::Plus.index()],
        "delta": counts[ReflectionColumn::Delta.index()],
        "ideas": counts[ReflectionColumn::Ideas.index()],
        "thanks": counts[ReflectionColumn::Thanks.index()],
    })
}

/// What `viewer` may see of a meeting's reflections: their own during
/// Icebreaker and Reflect, everyone's (anonymous) afterwards. The caller has
/// checked membership.
pub async fn snapshot(
    db: &impl ConnectionTrait,
    meeting: &meeting::Model,
    viewer: Uuid,
) -> Result<ReflectionsSnapshot, AppError> {
    let mut query = reflection::Entity::find().filter(reflection::Column::MeetingId.eq(meeting.id));
    if meeting.phase <= MeetingPhase::Reflect {
        query = query.filter(reflection::Column::AuthorId.eq(viewer));
    }
    let reflections = query
        .order_by_asc(reflection::Column::CreatedAt)
        .order_by_asc(reflection::Column::Id)
        .all(db)
        .await?
        .into_iter()
        .map(|model| ReflectionView {
            mine: model.author_id == viewer,
            model,
        })
        .collect();
    Ok(ReflectionsSnapshot {
        reflections,
        counts: counts(db, meeting.id).await?,
    })
}

/// Locks a running meeting in Reflect for one of `actor`'s reflection
/// changes; `actor` must be a participant.
async fn lock_reflect(
    txn: &impl ConnectionTrait,
    meeting_id: Uuid,
    actor: Uuid,
) -> Result<meeting::Model, AppError> {
    let (meeting, _) = find_for_member(txn, meeting_id, actor, true).await?;
    if meeting.status != MeetingStatus::Active {
        return Err(AppError::Conflict("meeting_ended"));
    }
    if meeting.phase != MeetingPhase::Reflect {
        return Err(AppError::Conflict("wrong_phase"));
    }
    require_participant(txn, meeting.id, actor).await?;
    Ok(meeting)
}

/// `actor`'s reflection, or `reflection_not_found` (also for someone else's:
/// ids of other people's reflections reveal nothing).
async fn find_own(
    db: &impl ConnectionTrait,
    reflection_id: Uuid,
    actor: Uuid,
) -> Result<reflection::Model, AppError> {
    reflection::Entity::find_by_id(reflection_id)
        .one(db)
        .await?
        .filter(|r| r.author_id == actor)
        .ok_or(AppError::NotFound("reflection_not_found"))
}

/// Tells everyone the new counts and the author's tabs that their reflection
/// changed.
async fn publish_change(
    txn: &impl ConnectionTrait,
    meeting_id: Uuid,
    author: Uuid,
    reflection_id: Uuid,
    client: Option<Uuid>,
    counts_changed: bool,
) -> Result<(), AppError> {
    if counts_changed {
        let counts = counts(txn, meeting_id).await?;
        ws::publish_meeting(
            txn,
            meeting_id,
            "reflection.count_changed",
            json!({ "counts": counts_json(&counts) }),
        )
        .await?;
    }
    ws::publish_user(
        txn,
        meeting_id,
        author,
        "reflection.changed",
        json!({ "id": reflection_id, "clientId": client }),
    )
    .await?;
    Ok(())
}

/// Sends a reflection (a participant, during Reflect). Returns its id.
pub async fn create(
    db: &(impl TransactionTrait + ConnectionTrait),
    meeting_id: Uuid,
    actor: Uuid,
    column: ReflectionColumn,
    raw_content: &str,
    client: Option<Uuid>,
) -> Result<Uuid, AppError> {
    let content = sanitize_as(raw_content, Profile::Reflection)?.html;
    let txn = db.begin().await?;
    let meeting = lock_reflect(&txn, meeting_id, actor).await?;
    let written = reflection::Entity::find()
        .filter(reflection::Column::MeetingId.eq(meeting.id))
        .filter(reflection::Column::AuthorId.eq(actor))
        .count(&txn)
        .await?;
    if written >= MAX_PER_AUTHOR {
        return Err(AppError::Conflict("too_many_reflections"));
    }
    let now = OffsetDateTime::now_utc();
    let id = Uuid::now_v7();
    reflection::ActiveModel {
        id: Set(id),
        meeting_id: Set(meeting.id),
        author_id: Set(actor),
        column: Set(column),
        content: Set(content),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(&txn)
    .await?;
    publish_change(&txn, meeting.id, actor, id, client, true).await?;
    txn.commit().await?;
    Ok(id)
}

/// Edits or moves the author's own reflection, during Reflect. Returns its
/// meeting.
pub async fn update(
    db: &(impl TransactionTrait + ConnectionTrait),
    reflection_id: Uuid,
    actor: Uuid,
    column: Option<ReflectionColumn>,
    raw_content: Option<&str>,
    client: Option<Uuid>,
) -> Result<Uuid, AppError> {
    let content = raw_content
        .map(|raw| sanitize_as(raw, Profile::Reflection).map(|s| s.html))
        .transpose()?;
    let found = find_own(db, reflection_id, actor).await?;
    let txn = db.begin().await?;
    let meeting = lock_reflect(&txn, found.meeting_id, actor).await?;
    // Re-read under the meeting lock: it may have been deleted meanwhile.
    let current = find_own(&txn, reflection_id, actor).await?;
    let moved = column.is_some_and(|c| c != current.column);
    let edited = content.as_ref().is_some_and(|c| *c != current.content);
    if moved || edited {
        reflection::ActiveModel {
            id: Set(current.id),
            column: Set(column.unwrap_or(current.column)),
            content: Set(content.unwrap_or(current.content)),
            updated_at: Set(OffsetDateTime::now_utc()),
            ..Default::default()
        }
        .update(&txn)
        .await?;
        publish_change(&txn, meeting.id, actor, current.id, client, moved).await?;
    }
    txn.commit().await?;
    Ok(meeting.id)
}

/// Deletes the author's own reflection, during Reflect. Returns its meeting.
pub async fn delete(
    db: &(impl TransactionTrait + ConnectionTrait),
    reflection_id: Uuid,
    actor: Uuid,
    client: Option<Uuid>,
) -> Result<Uuid, AppError> {
    let found = find_own(db, reflection_id, actor).await?;
    let txn = db.begin().await?;
    let meeting = lock_reflect(&txn, found.meeting_id, actor).await?;
    let current = find_own(&txn, reflection_id, actor).await?;
    reflection::Entity::delete_by_id(current.id)
        .exec(&txn)
        .await?;
    publish_change(&txn, meeting.id, actor, current.id, client, true).await?;
    txn.commit().await?;
    Ok(meeting.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_serialize_by_column_name() {
        let counts = [1, 0, 3, 2];
        assert_eq!(
            counts_json(&counts),
            json!({ "plus": 1, "delta": 0, "ideas": 3, "thanks": 2 })
        );
    }
}
