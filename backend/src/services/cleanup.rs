//! Periodic removal of attachments nothing references (spec §13).

use std::time::Duration;

use sea_orm::DatabaseConnection;

use crate::domain::attachments;

/// How often each replica looks for orphaned attachments.
const INTERVAL: Duration = Duration::from_secs(3600);

/// Starts the hourly cleanup on the current Tokio runtime. Runs on every
/// replica: the delete is idempotent.
pub fn start(db: DatabaseConnection) {
    tokio::spawn(async move {
        let mut ticks = tokio::time::interval(INTERVAL);
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticks.tick().await;
            match attachments::cleanup(&db).await {
                Ok(0) => {}
                Ok(n) => tracing::info!(deleted = n, "removed unreferenced attachments"),
                Err(e) => tracing::warn!(error = %e, "attachment cleanup failed"),
            }
        }
    });
}
