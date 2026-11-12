//! Database schema migrations. The backend owns the schema (spec §13).

pub use sea_orm_migration::prelude::*;

mod m20261112_000001_auth;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(m20261112_000001_auth::Migration)]
    }
}
