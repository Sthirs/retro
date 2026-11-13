//! Teams, memberships, invitations and the team join link (spec §5, §6, §13).

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

const UP: &str = r#"
CREATE TYPE team_role AS ENUM ('admin', 'member');
CREATE TYPE invitation_status AS ENUM ('pending', 'accepted', 'declined', 'revoked', 'expired');

CREATE TABLE teams (
    id           uuid PRIMARY KEY,
    name         text NOT NULL CHECK (length(btrim(name)) BETWEEN 1 AND 60),
    join_token   text UNIQUE,
    join_enabled boolean NOT NULL DEFAULT true,
    created_by   uuid REFERENCES users (id) ON DELETE SET NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE team_members (
    team_id   uuid NOT NULL REFERENCES teams (id) ON DELETE CASCADE,
    user_id   uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    role      team_role NOT NULL,
    joined_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (team_id, user_id)
);
CREATE INDEX team_members_user_id_idx ON team_members (user_id);

CREATE TABLE invitations (
    id         uuid PRIMARY KEY,
    team_id    uuid NOT NULL REFERENCES teams (id) ON DELETE CASCADE,
    email      text NOT NULL CHECK (email = lower(email)),
    token      text NOT NULL UNIQUE,
    invited_by uuid REFERENCES users (id) ON DELETE SET NULL,
    status     invitation_status NOT NULL DEFAULT 'pending',
    expires_at timestamptz NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX invitations_team_id_idx ON invitations (team_id);
CREATE INDEX invitations_pending_email_idx ON invitations (email) WHERE status = 'pending';
"#;

const DOWN: &str = r#"
DROP TABLE invitations;
DROP TABLE team_members;
DROP TABLE teams;
DROP TYPE invitation_status;
DROP TYPE team_role;
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
