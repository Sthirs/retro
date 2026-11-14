//! Tasks board: tasks and the users they mention (spec §13).
//! `meeting_id` and `group_id` are plain columns here; the meetings migration
//! adds their foreign keys.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

const UP: &str = r#"
CREATE TYPE task_status AS ENUM ('future', 'stuck', 'active', 'done');

CREATE TABLE tasks (
  id UUID PRIMARY KEY,
  team_id UUID NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
  meeting_id UUID NULL,
  group_id UUID NULL,
  author_id UUID NOT NULL REFERENCES users(id),
  content TEXT NOT NULL,
  status task_status NOT NULL,
  position TEXT COLLATE "C" NOT NULL,
  created_at TIMESTAMPTZ NOT NULL,
  updated_at TIMESTAMPTZ NOT NULL,
  UNIQUE (team_id, status, position)
);

CREATE TABLE task_mentions (
  task_id UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  PRIMARY KEY (task_id, user_id)
);

CREATE INDEX task_mentions_user_idx ON task_mentions (user_id);
"#;

const DOWN: &str = r#"
DROP TABLE task_mentions;
DROP TABLE tasks;
DROP TYPE task_status;
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
