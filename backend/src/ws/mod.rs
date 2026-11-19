//! Real-time hub (spec §11, ADR-0011): one WebSocket per client, channels
//! `meeting:{id}` and `team:{id}`, fan-out between replicas through
//! PostgreSQL `LISTEN/NOTIFY`.
//!
//! Writers call [`publish`] inside the transaction of the change, so an event
//! goes out only when the change commits. Each replica's [`Hub`] listens for
//! those notifications and forwards them to its own sockets.

mod channel;
pub(crate) mod hub;
mod socket;
pub mod ticket;

pub use channel::Channel;
pub use hub::{
    AccessChange, ColumnWritingDto, DragDto, Hub, TaskTarget, TaskWriterDto, WritingCounts,
    WritingDto, WritingMode,
};
pub use socket::ws_handler;

use sea_orm::{ConnectionTrait, DbBackend, DbErr, Statement};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// The PostgreSQL notification channel every replica listens on.
pub(crate) const PG_CHANNEL: &str = "retro_events";

/// A notification as sent through PostgreSQL.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct Notification {
    pub channel: String,
    #[serde(rename = "type")]
    pub kind: String,
    /// Per-channel sequence number; `None` for ephemeral events (presence)
    /// and user-targeted ones.
    pub seq: Option<i64>,
    pub payload: Value,
    /// Only this user's sockets receive the event (ADR-0012); never sent to
    /// clients.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<Uuid>,
}

/// Publishes an event on `channel`. Call it inside the transaction that makes
/// the change: PostgreSQL delivers the notification only on commit.
pub async fn publish(
    db: &impl ConnectionTrait,
    channel: Channel,
    seq: Option<i64>,
    kind: &str,
    payload: Value,
) -> Result<(), DbErr> {
    let notification = Notification {
        channel: channel.to_string(),
        kind: kind.to_owned(),
        seq,
        payload,
        user: None,
    };
    notify(db, &notification).await
}

/// Publishes an event on a meeting's channel for `user`'s sockets only
/// (ADR-0012). It carries no `seq`: the other subscribers never see it.
pub async fn publish_user(
    db: &impl ConnectionTrait,
    meeting_id: Uuid,
    user: Uuid,
    kind: &str,
    payload: Value,
) -> Result<(), DbErr> {
    let notification = Notification {
        channel: Channel::Meeting(meeting_id).to_string(),
        kind: kind.to_owned(),
        seq: None,
        payload,
        user: Some(user),
    };
    notify(db, &notification).await
}

pub(crate) async fn notify(
    db: &impl ConnectionTrait,
    notification: &Notification,
) -> Result<(), DbErr> {
    let text = serde_json::to_string(notification).map_err(|e| DbErr::Custom(e.to_string()))?;
    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT pg_notify($1, $2)",
        [PG_CHANNEL.into(), text.into()],
    ))
    .await?;
    Ok(())
}

/// Takes the next `seq` of `meeting:{id}`. Locks the meeting row until the
/// transaction ends, which keeps the sequence in commit order.
pub async fn next_meeting_seq(db: &impl ConnectionTrait, meeting_id: Uuid) -> Result<i64, DbErr> {
    next_seq(db, "meetings", meeting_id).await
}

/// Takes the next `seq` of `team:{id}`, locking the team row.
pub async fn next_team_seq(db: &impl ConnectionTrait, team_id: Uuid) -> Result<i64, DbErr> {
    next_seq(db, "teams", team_id).await
}

async fn next_seq(db: &impl ConnectionTrait, table: &str, id: Uuid) -> Result<i64, DbErr> {
    let row = db
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            format!(
                "UPDATE {table} SET event_seq = event_seq + 1 WHERE id = $1 RETURNING event_seq"
            ),
            [id.into()],
        ))
        .await?
        .ok_or_else(|| DbErr::RecordNotFound(format!("{table} {id}")))?;
    row.try_get("", "event_seq")
}

/// Publishes a persisted event on a meeting's channel, with its next `seq`,
/// and returns that `seq`.
pub async fn publish_meeting(
    db: &impl ConnectionTrait,
    meeting_id: Uuid,
    kind: &str,
    payload: Value,
) -> Result<i64, DbErr> {
    let seq = next_meeting_seq(db, meeting_id).await?;
    publish(db, Channel::Meeting(meeting_id), Some(seq), kind, payload).await?;
    Ok(seq)
}

/// Publishes a persisted event on a team's channel, with its next `seq`.
pub async fn publish_team(
    db: &impl ConnectionTrait,
    team_id: Uuid,
    kind: &str,
    payload: Value,
) -> Result<(), DbErr> {
    let seq = next_team_seq(db, team_id).await?;
    publish(db, Channel::Team(team_id), Some(seq), kind, payload).await
}
