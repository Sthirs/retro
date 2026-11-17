//! Groups (spec §9.3, §13): named stacks of reflections made during Group.
//! A group sits at the place of the card it was created on; every card keeps
//! its own place (`position`, set when Group starts) and a `version` raised
//! by every move.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

const UP: &str = r#"
CREATE TABLE reflection_groups (
  id UUID PRIMARY KEY,
  meeting_id UUID NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
  "column" reflection_column NOT NULL,
  position INT NOT NULL,
  name TEXT NOT NULL DEFAULT '',
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX reflection_groups_meeting ON reflection_groups (meeting_id);

ALTER TABLE reflections
  ADD COLUMN group_id UUID NULL REFERENCES reflection_groups(id) ON DELETE SET NULL,
  ADD COLUMN position INT NOT NULL DEFAULT 0,
  ADD COLUMN version INT NOT NULL DEFAULT 0;

CREATE INDEX reflections_group ON reflections (group_id) WHERE group_id IS NOT NULL;
"#;

const DOWN: &str = r#"
ALTER TABLE reflections DROP COLUMN group_id, DROP COLUMN position, DROP COLUMN version;
DROP TABLE reflection_groups;
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
