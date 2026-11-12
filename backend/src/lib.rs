//! Retro backend: REST API (and, later, WebSocket hub) for team retrospectives.

pub mod api;
pub mod app;
pub mod auth;
pub mod config;
pub mod domain;
pub mod entities;
pub mod error;
pub mod openapi;
pub mod seed;
pub mod services;
pub mod state;
pub mod ws;

use migration::{Migrator, MigratorTrait};
use sea_orm::{ConnectionTrait, Database, DatabaseConnection, TransactionTrait};

/// Version baked in at build time (`APP_VERSION` build argument).
pub const VERSION: &str = match option_env!("APP_VERSION") {
    Some(v) => v,
    None => "dev",
};

/// Arbitrary key for the advisory lock that serialises migrations across replicas.
const MIGRATION_LOCK_KEY: i64 = 0x5245_5452_4f00; // "RETRO\0"

pub async fn connect(database_url: &str) -> anyhow::Result<DatabaseConnection> {
    Ok(Database::connect(database_url).await?)
}

/// Applies pending migrations. Concurrent callers wait on a transaction-scoped
/// advisory lock, so only one replica migrates at a time.
pub async fn migrate(db: &DatabaseConnection) -> anyhow::Result<()> {
    let txn = db.begin().await?;
    txn.execute_unprepared(&format!(
        "SELECT pg_advisory_xact_lock({MIGRATION_LOCK_KEY})"
    ))
    .await?;
    Migrator::up(&txn, None).await?;
    txn.commit().await?;
    Ok(())
}
