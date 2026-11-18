//! Database schema migrations. The backend owns the schema (spec §13).

pub use sea_orm_migration::prelude::*;

mod m20261112_000001_auth;
mod m20261113_000001_teams;
mod m20261113_000002_open_invitation_unique;
mod m20261114_000001_account;
mod m20261114_000002_tasks;
mod m20261115_000001_meetings;
mod m20261115_000002_icebreaker_back;
mod m20261116_000001_reflections;
mod m20261117_000001_attachments;
mod m20261118_000001_groups;
mod m20261119_000001_votes;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20261112_000001_auth::Migration),
            Box::new(m20261113_000001_teams::Migration),
            Box::new(m20261113_000002_open_invitation_unique::Migration),
            Box::new(m20261114_000001_account::Migration),
            Box::new(m20261114_000002_tasks::Migration),
            Box::new(m20261115_000001_meetings::Migration),
            Box::new(m20261115_000002_icebreaker_back::Migration),
            Box::new(m20261116_000001_reflections::Migration),
            Box::new(m20261117_000001_attachments::Migration),
            Box::new(m20261118_000001_groups::Migration),
            Box::new(m20261119_000001_votes::Migration),
        ]
    }
}
