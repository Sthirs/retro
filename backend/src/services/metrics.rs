//! Periodic export of `pg_notification_queue_usage()` (spec §11.3): the share
//! of PostgreSQL's global `NOTIFY` queue in use, from 0 to 1.

use std::time::Duration;

use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, DbErr, Statement};

/// How often each replica samples the queue.
const INTERVAL: Duration = Duration::from_secs(15);

/// The share of the notification queue in use (0 to 1).
pub async fn notification_queue_usage(db: &impl ConnectionTrait) -> Result<f64, DbErr> {
    let row = db
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT pg_notification_queue_usage() AS usage",
        ))
        .await?
        .ok_or_else(|| DbErr::RecordNotFound("pg_notification_queue_usage".into()))?;
    row.try_get::<f64>("", "usage")
}

/// Starts sampling on the current Tokio runtime. Uses the pool, never the
/// hub's listener connection, so the listener is never blocked.
pub fn start(db: DatabaseConnection) {
    tokio::spawn(async move {
        let mut ticks = tokio::time::interval(INTERVAL);
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticks.tick().await;
            match notification_queue_usage(&db).await {
                Ok(usage) => metrics::gauge!("pg_notification_queue_usage").set(usage),
                Err(e) => tracing::warn!(error = %e, "notification queue sampling failed"),
            }
        }
    });
}
