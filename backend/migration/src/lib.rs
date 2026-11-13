//! Database schema migrations. The backend owns the schema (spec §13).

pub use sea_orm_migration::prelude::*;

mod m20261112_000001_auth;
mod m20261113_000001_teams;
mod m20261113_000002_open_invitation_unique;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20261112_000001_auth::Migration),
            Box::new(m20261113_000001_teams::Migration),
            Box::new(m20261113_000002_open_invitation_unique::Migration),
        ]
    }
}
