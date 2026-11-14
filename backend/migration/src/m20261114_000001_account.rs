//! Account page: custom display name flag and uploaded profile pictures
//! (spec §4, §13).

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

const UP: &str = r#"
ALTER TABLE users ADD COLUMN display_name_custom BOOLEAN NOT NULL DEFAULT false,
                  ADD COLUMN picture_version TEXT NULL;

CREATE TABLE user_pictures (
  user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
  content_type TEXT NOT NULL,
  size_bytes INTEGER NOT NULL,
  sha256 TEXT NOT NULL,
  data BYTEA NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
"#;

const DOWN: &str = r#"
DROP TABLE user_pictures;

ALTER TABLE users DROP COLUMN picture_version,
                  DROP COLUMN display_name_custom;
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
