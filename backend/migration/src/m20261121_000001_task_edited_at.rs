//! A task shows when its content was last edited (spec §9.5). `updated_at`
//! also moves with the card on the board, so the edit time has its own column.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

const UP: &str = r#"
ALTER TABLE tasks ADD COLUMN edited_at TIMESTAMPTZ;
"#;

const DOWN: &str = r#"
ALTER TABLE tasks DROP COLUMN edited_at;
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
