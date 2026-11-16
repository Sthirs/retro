//! Image attachments (spec §13, ADR-0013): images re-encoded as WebP, and the
//! reflections and tasks that reference them (derived from their content).

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

const UP: &str = r#"
CREATE TABLE attachments (
  id UUID PRIMARY KEY,
  team_id UUID NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
  uploader_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  content_type TEXT NOT NULL,
  animated BOOLEAN NOT NULL,
  width INTEGER NOT NULL CHECK (width > 0),
  height INTEGER NOT NULL CHECK (height > 0),
  size_bytes INTEGER NOT NULL CHECK (size_bytes > 0),
  sha256 TEXT NOT NULL,
  data BYTEA NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX attachments_uploader ON attachments (uploader_id, created_at);
CREATE INDEX attachments_created ON attachments (created_at);

CREATE TABLE reflection_attachments (
  reflection_id UUID NOT NULL REFERENCES reflections(id) ON DELETE CASCADE,
  attachment_id UUID NOT NULL REFERENCES attachments(id) ON DELETE CASCADE,
  PRIMARY KEY (reflection_id, attachment_id)
);

CREATE INDEX reflection_attachments_attachment ON reflection_attachments (attachment_id);

CREATE TABLE task_attachments (
  task_id UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  attachment_id UUID NOT NULL REFERENCES attachments(id) ON DELETE CASCADE,
  PRIMARY KEY (task_id, attachment_id)
);

CREATE INDEX task_attachments_attachment ON task_attachments (attachment_id);
"#;

const DOWN: &str = r#"
DROP TABLE task_attachments;
DROP TABLE reflection_attachments;
DROP TABLE attachments;
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
