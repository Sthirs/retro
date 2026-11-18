//! Votes (spec §9.4, §13): one row per vote a participant gives a group
//! during Vote. Rows rather than a count, so "the most recent votes" a lower
//! limit removes are exactly known.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

const UP: &str = r#"
CREATE TABLE votes (
  id UUID PRIMARY KEY,
  meeting_id UUID NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
  user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  group_id UUID NOT NULL REFERENCES reflection_groups(id) ON DELETE CASCADE,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX votes_meeting_user ON votes (meeting_id, user_id);
CREATE INDEX votes_group ON votes (group_id);

-- Meetings already in Vote or later never pass through the Vote start:
-- wrap their ungrouped cards in single-card groups now, the same way.
WITH loose AS (
  SELECT r.id, r.meeting_id, r."column", r.position, gen_random_uuid() AS group_id
  FROM reflections r JOIN meetings m ON m.id = r.meeting_id
  WHERE m.phase >= 'vote' AND r.group_id IS NULL
), created AS (
  INSERT INTO reflection_groups (id, meeting_id, "column", position, name)
  SELECT group_id, meeting_id, "column", position, '' FROM loose
)
UPDATE reflections r SET group_id = loose.group_id
FROM loose WHERE r.id = loose.id;
"#;

const DOWN: &str = r#"
DROP TABLE votes;
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
