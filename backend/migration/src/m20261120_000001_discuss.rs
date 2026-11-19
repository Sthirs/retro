//! Discuss (spec §9.5, §13): emoji reactions on reflections, one row per
//! person and emoji, and the current group of the meeting as a reference to
//! its group. Tasks link to the group they were written for.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

const UP: &str = r#"
CREATE TABLE reactions (
  reflection_id UUID NOT NULL REFERENCES reflections(id) ON DELETE CASCADE,
  user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  emoji TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (reflection_id, user_id, emoji)
);

UPDATE meetings m SET discuss_group_id = NULL
WHERE discuss_group_id IS NOT NULL
  AND NOT EXISTS (SELECT 1 FROM reflection_groups g WHERE g.id = m.discuss_group_id);
ALTER TABLE meetings ADD CONSTRAINT meetings_discuss_group_fk
  FOREIGN KEY (discuss_group_id) REFERENCES reflection_groups(id) ON DELETE SET NULL;

UPDATE tasks t SET group_id = NULL
WHERE group_id IS NOT NULL
  AND NOT EXISTS (SELECT 1 FROM reflection_groups g WHERE g.id = t.group_id);
ALTER TABLE tasks ADD CONSTRAINT tasks_group_fk
  FOREIGN KEY (group_id) REFERENCES reflection_groups(id) ON DELETE SET NULL;
"#;

const DOWN: &str = r#"
ALTER TABLE tasks DROP CONSTRAINT tasks_group_fk;
ALTER TABLE meetings DROP CONSTRAINT meetings_discuss_group_fk;
DROP TABLE reactions;
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
