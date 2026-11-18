//! Votes keep the order the server accepted them in (spec §9.4): "the most
//! recent votes" a lower limit or a take-back removes follow this ordinal,
//! never the wall clock, which can step backwards.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

const UP: &str = r#"
ALTER TABLE votes ADD COLUMN ordinal BIGINT GENERATED ALWAYS AS IDENTITY;
CREATE INDEX votes_meeting_ordinal ON votes (meeting_id, ordinal);
"#;

const DOWN: &str = r#"
DROP INDEX votes_meeting_ordinal;
ALTER TABLE votes DROP COLUMN ordinal;
"#;

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
