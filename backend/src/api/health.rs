//! Liveness and readiness probes (spec §14.6).

use std::collections::HashSet;

use axum::{extract::State, http::StatusCode};
use migration::{Migrator, MigratorTrait};
use sea_orm::{ConnectionTrait, DatabaseConnection, Statement};

use crate::state::AppState;

/// Liveness: the process is up.
#[utoipa::path(get, path = "/healthz", tag = "operational", responses((status = 200, description = "Alive")))]
pub async fn healthz() -> &'static str {
    "ok"
}

/// Readiness: the database is reachable and every migration this binary knows
/// is applied. Migrations applied by a newer release are ignored, so pods of
/// the previous release stay ready during a rolling upgrade.
#[utoipa::path(
    get,
    path = "/readyz",
    tag = "operational",
    responses((status = 200, description = "Ready"), (status = 503, description = "Not ready"))
)]
pub async fn readyz(State(state): State<AppState>) -> (StatusCode, &'static str) {
    match pending_migrations(&state.db).await {
        Ok(0) => (StatusCode::OK, "ok"),
        Ok(_) => (StatusCode::SERVICE_UNAVAILABLE, "migrations pending"),
        Err(_) => (StatusCode::SERVICE_UNAVAILABLE, "database unavailable"),
    }
}

async fn pending_migrations(db: &DatabaseConnection) -> Result<usize, sea_orm::DbErr> {
    let rows = db
        .query_all_raw(Statement::from_string(
            db.get_database_backend(),
            "SELECT version FROM seaql_migrations",
        ))
        .await?;
    let applied: HashSet<String> = rows
        .iter()
        .filter_map(|row| row.try_get::<String>("", "version").ok())
        .collect();
    Ok(Migrator::migrations()
        .iter()
        .filter(|m| !applied.contains(m.name()))
        .count())
}
