//! Meetings (spec §8, §9.1, §13): meetings, their participants, and the
//! greetings and icebreaker questions a new meeting picks from (50 of each).
//! Also adds the per-channel event counters of the real-time hub (spec §11,
//! ADR-0011) and the foreign key `tasks.meeting_id` that the tasks migration
//! left out.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

const UP: &str = r#"
-- UUIDv7 for the seed rows (PostgreSQL < 18 has no uuidv7()).
CREATE FUNCTION pg_temp.retro_uuid_v7() RETURNS uuid AS $$
  SELECT encode(
    set_bit(set_bit(
      overlay(uuid_send(gen_random_uuid())
        PLACING substring(int8send(floor(extract(epoch FROM clock_timestamp()) * 1000)::bigint) FROM 3)
        FROM 1 FOR 6),
      52, 1), 53, 1),
    'hex')::uuid
$$ LANGUAGE sql VOLATILE;

CREATE TYPE meeting_status AS ENUM ('active', 'ended');
CREATE TYPE meeting_phase AS ENUM ('icebreaker', 'reflect', 'group', 'vote', 'discuss');

CREATE TABLE greetings (
  id UUID PRIMARY KEY,
  language TEXT NOT NULL UNIQUE,
  lang_code TEXT NOT NULL,
  text TEXT NOT NULL
);

CREATE TABLE icebreaker_questions (
  id UUID PRIMARY KEY,
  text TEXT NOT NULL UNIQUE,
  active BOOLEAN NOT NULL DEFAULT true
);

CREATE TABLE meetings (
  id UUID PRIMARY KEY,
  team_id UUID NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
  number INTEGER NOT NULL,
  title TEXT NOT NULL,
  created_by UUID NOT NULL REFERENCES users(id),
  facilitator_id UUID NOT NULL REFERENCES users(id),
  status meeting_status NOT NULL DEFAULT 'active',
  invite_token TEXT NULL UNIQUE,
  phase meeting_phase NOT NULL DEFAULT 'icebreaker',
  greeting_text TEXT NOT NULL,
  greeting_language TEXT NOT NULL,
  greeting_lang_code TEXT NOT NULL,
  icebreaker_question TEXT NOT NULL,
  icebreaker_order UUID[] NOT NULL DEFAULT '{}',
  icebreaker_index INTEGER NOT NULL DEFAULT 0,
  icebreaker_skipped UUID[] NOT NULL DEFAULT '{}',
  vote_budget INTEGER NOT NULL DEFAULT 5,
  vote_max_per_group INTEGER NOT NULL DEFAULT 3,
  discuss_group_id UUID NULL,
  timer_state JSONB NULL,
  event_seq BIGINT NOT NULL DEFAULT 0,
  created_at TIMESTAMPTZ NOT NULL,
  ended_at TIMESTAMPTZ NULL,
  UNIQUE (team_id, number)
);

CREATE INDEX meetings_team_status_idx ON meetings (team_id, status, created_at DESC);

CREATE TABLE meeting_participants (
  meeting_id UUID NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
  user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  joined_at TIMESTAMPTZ NOT NULL,
  is_ready BOOLEAN NOT NULL DEFAULT false,
  PRIMARY KEY (meeting_id, user_id)
);

CREATE INDEX meeting_participants_user_idx ON meeting_participants (user_id);

ALTER TABLE teams ADD COLUMN event_seq BIGINT NOT NULL DEFAULT 0;

-- Development data seeded made-up meeting ids before meetings existed.
UPDATE tasks SET meeting_id = NULL WHERE meeting_id IS NOT NULL;
ALTER TABLE tasks
  ADD CONSTRAINT tasks_meeting_id_fkey
  FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE SET NULL;
CREATE INDEX tasks_meeting_idx ON tasks (meeting_id) WHERE meeting_id IS NOT NULL;

INSERT INTO greetings (id, language, lang_code, text)
SELECT pg_temp.retro_uuid_v7(), v.language, v.lang_code, v.text FROM (VALUES
  ('English', 'en', 'Hello!'),
  ('Italian', 'it', 'Ciao!'),
  ('Spanish', 'es', '¡Hola!'),
  ('French', 'fr', 'Bonjour !'),
  ('German', 'de', 'Hallo!'),
  ('Portuguese', 'pt', 'Olá!'),
  ('Dutch', 'nl', 'Hoi!'),
  ('Swedish', 'sv', 'Hej!'),
  ('Norwegian', 'nb', 'Hei!'),
  ('Danish', 'da', 'Goddag!'),
  ('Finnish', 'fi', 'Moi!'),
  ('Icelandic', 'is', 'Halló!'),
  ('Polish', 'pl', 'Cześć!'),
  ('Czech', 'cs', 'Ahoj!'),
  ('Slovak', 'sk', 'Dobrý deň!'),
  ('Hungarian', 'hu', 'Szia!'),
  ('Romanian', 'ro', 'Salut!'),
  ('Greek', 'el', 'Γεια σου!'),
  ('Turkish', 'tr', 'Merhaba!'),
  ('Russian', 'ru', 'Привет!'),
  ('Ukrainian', 'uk', 'Привіт!'),
  ('Bulgarian', 'bg', 'Здравей!'),
  ('Serbian', 'sr', 'Здраво!'),
  ('Croatian', 'hr', 'Bok!'),
  ('Slovenian', 'sl', 'Živjo!'),
  ('Lithuanian', 'lt', 'Labas!'),
  ('Latvian', 'lv', 'Sveiki!'),
  ('Estonian', 'et', 'Tere!'),
  ('Irish', 'ga', 'Dia duit!'),
  ('Welsh', 'cy', 'Shwmae!'),
  ('Catalan', 'ca', 'Bon dia!'),
  ('Basque', 'eu', 'Kaixo!'),
  ('Albanian', 'sq', 'Përshëndetje!'),
  ('Arabic', 'ar', 'مرحبا!'),
  ('Hebrew', 'he', 'שלום!'),
  ('Persian', 'fa', 'سلام!'),
  ('Hindi', 'hi', 'नमस्ते!'),
  ('Bengali', 'bn', 'নমস্কার!'),
  ('Tamil', 'ta', 'வணக்கம்!'),
  ('Japanese', 'ja', 'こんにちは！'),
  ('Korean', 'ko', '안녕하세요!'),
  ('Mandarin Chinese', 'zh', '你好！'),
  ('Vietnamese', 'vi', 'Xin chào!'),
  ('Thai', 'th', 'สวัสดี!'),
  ('Indonesian', 'id', 'Halo!'),
  ('Filipino', 'fil', 'Kumusta!'),
  ('Swahili', 'sw', 'Jambo!'),
  ('Zulu', 'zu', 'Sawubona!'),
  ('Hawaiian', 'haw', 'Aloha!'),
  ('Māori', 'mi', 'Kia ora!')
) AS v(language, lang_code, text);

INSERT INTO icebreaker_questions (id, text)
SELECT pg_temp.retro_uuid_v7(), v.text FROM (VALUES
  ('What''s a small thing that made your week better?'),
  ('What''s the best thing you''ve eaten recently?'),
  ('If you could learn any skill instantly, what would it be?'),
  ('What''s a song you''ve had on repeat lately?'),
  ('What''s your favourite way to spend a Sunday?'),
  ('What''s the last thing that made you laugh out loud?'),
  ('Which fictional place would you most like to visit?'),
  ('What''s a tool or app you couldn''t work without?'),
  ('What did you want to be when you were a child?'),
  ('What''s the best piece of advice you''ve ever received?'),
  ('Coffee, tea, or something else entirely?'),
  ('What''s a hobby you''d like to pick up?'),
  ('What''s the most interesting thing you''ve read this month?'),
  ('If you could have dinner with anyone, who would it be?'),
  ('What''s your go-to comfort food?'),
  ('Where would you go on a surprise free trip?'),
  ('What''s a film or series you''d recommend to everyone here?'),
  ('What''s something you''re looking forward to?'),
  ('Are you an early bird or a night owl?'),
  ('What''s a talent most people don''t know you have?'),
  ('What''s the best gift you''ve ever received?'),
  ('What''s your favourite season, and why?'),
  ('What''s a place that feels like home to you?'),
  ('What''s one thing you''d like to get better at this quarter?'),
  ('If you had an extra hour every day, how would you use it?'),
  ('What''s a small win you had this week?'),
  ('What''s your favourite board game or video game?'),
  ('Which superpower would you pick?'),
  ('What''s the most useful thing you learned since our last retro?'),
  ('What''s a habit that changed your life for the better?'),
  ('What''s the strangest food combination you enjoy?'),
  ('What''s your favourite sound?'),
  ('If you could live in any decade, which would you pick?'),
  ('What''s a book that changed how you think?'),
  ('What''s your favourite thing about where you live?'),
  ('Mountains or the sea?'),
  ('What''s a skill you learned from a colleague?'),
  ('What would your autobiography be called?'),
  ('What''s the best concert or show you''ve been to?'),
  ('What''s a word or phrase you use too often?'),
  ('Who made your work easier recently?'),
  ('What''s a podcast or channel you''d recommend?'),
  ('What''s the first thing you do after work?'),
  ('If a pet could talk, what do you think it would say about you?'),
  ('What''s your favourite holiday tradition?'),
  ('What''s one thing on your bucket list?'),
  ('What did you do for fun last weekend?'),
  ('What''s a recent "aha" moment you had?'),
  ('What''s the best view you''ve ever seen?'),
  ('If you had a boat, what would you name it?')
) AS v(text);

DROP FUNCTION pg_temp.retro_uuid_v7();
"#;

const DOWN: &str = r#"
DROP INDEX tasks_meeting_idx;
ALTER TABLE tasks DROP CONSTRAINT tasks_meeting_id_fkey;
ALTER TABLE teams DROP COLUMN event_seq;
DROP TABLE meeting_participants;
DROP TABLE meetings;
DROP TABLE icebreaker_questions;
DROP TABLE greetings;
DROP TYPE meeting_phase;
DROP TYPE meeting_status;
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
