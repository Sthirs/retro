//! At most one open invitation per email and team (spec §6.1, §13).

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

const UP: &str = r#"
UPDATE invitations SET status = 'expired'
WHERE status = 'pending' AND expires_at <= now();

UPDATE invitations SET status = 'revoked'
WHERE status = 'pending'
  AND id NOT IN (
    SELECT DISTINCT ON (team_id, email) id
    FROM invitations
    WHERE status = 'pending'
    ORDER BY team_id, email, created_at DESC, id DESC
  );

CREATE UNIQUE INDEX invitations_open_email_uq ON invitations (team_id, email)
WHERE status = 'pending';
"#;

const DOWN: &str = "DROP INDEX invitations_open_email_uq;";

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(UP).await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(DOWN).await?;
        Ok(())
    }
}
