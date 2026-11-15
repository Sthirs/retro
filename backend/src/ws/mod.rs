//! Real-time hub (spec §11, ADR-0011): one WebSocket per client, channels
//! `meeting:{id}` and `team:{id}`, fan-out between replicas through
//! PostgreSQL `LISTEN/NOTIFY`.
//!
//! Writers call [`publish`] inside the transaction of the change, so an event
//! goes out only when the change commits. Each replica's [`Hub`] listens for
//! those notifications and forwards them to its own sockets.

mod channel;
mod hub;
mod socket;
pub mod ticket;

pub use channel::Channel;
pub use hub::{AccessChange, Hub};
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
    /// Per-channel sequence number; `None` for ephemeral events (presence).
    pub seq: Option<i64>,
    pub payload: Value,
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

/// Publishes a persisted event on a meeting's channel, with its next `seq`.
pub async fn publish_meeting(
    db: &impl ConnectionTrait,
    meeting_id: Uuid,
    kind: &str,
    payload: Value,
) -> Result<(), DbErr> {
    let seq = next_meeting_seq(db, meeting_id).await?;
    publish(db, Channel::Meeting(meeting_id), Some(seq), kind, payload).await
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
