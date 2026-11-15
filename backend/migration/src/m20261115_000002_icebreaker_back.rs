//! Icebreaker (spec §9.1): each greeting gets its pronunciation (IPA), copied
//! onto the meeting like the rest of the greeting; the facilitator goes back to
//! earlier people instead of skipping, so `icebreaker_skipped` goes away.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

const UP: &str = r#"
ALTER TABLE greetings ADD COLUMN pronunciation TEXT NOT NULL DEFAULT '';

UPDATE greetings g SET pronunciation = v.pronunciation FROM (VALUES
  ('English', '/həˈloʊ/'),
  ('Italian', '/ˈtʃa.o/'),
  ('Spanish', '/ˈo.la/'),
  ('French', '/bɔ̃.ʒuʁ/'),
  ('German', '/ˈha.lo/'),
  ('Portuguese', '/oˈla/'),
  ('Dutch', '/ɦɔi̯/'),
  ('Swedish', '/hɛj/'),
  ('Norwegian', '/hæi̯/'),
  ('Danish', '/ɡoˈdæˀ/'),
  ('Finnish', '/moi̯/'),
  ('Icelandic', '/ˈhalːou/'),
  ('Polish', '/t͡ʂɛɕt͡ɕ/'),
  ('Czech', '/ˈaɦoj/'),
  ('Slovak', '/ˈdɔbriː ˈɟɛɲ/'),
  ('Hungarian', '/ˈsiɒ/'),
  ('Romanian', '/saˈlut/'),
  ('Greek', '/ˈʝa su/'),
  ('Turkish', '/ˈmeɾ.ha.ba/'),
  ('Russian', '/prʲɪˈvʲet/'),
  ('Ukrainian', '/prɪˈwʲit/'),
  ('Bulgarian', '/zdrɐˈvɛj/'),
  ('Serbian', '/ˈzdra.ʋo/'),
  ('Croatian', '/bok/'),
  ('Slovenian', '/ˈʒiːʋjɔ/'),
  ('Lithuanian', '/ˈlaːbas/'),
  ('Latvian', '/ˈsvɛi̯ki/'),
  ('Estonian', '/ˈtere/'),
  ('Irish', '/ˌdʲiə ˈɡɪtʲ/'),
  ('Welsh', '/ʃʊˈmaɪ/'),
  ('Catalan', '/bɔn ˈdi.ə/'),
  ('Basque', '/ˈkai̯ʃo/'),
  ('Albanian', '/pəɾʃənˈdɛtjɛ/'),
  ('Arabic', '/ˈmar.ħa.ban/'),
  ('Hebrew', '/ʃaˈlom/'),
  ('Persian', '/sæˈlɒːm/'),
  ('Hindi', '/nəˈməs.teː/'),
  ('Bengali', '/nɔ.moʃ.kaɾ/'),
  ('Tamil', '/ʋa.ɳak.kam/'),
  ('Japanese', '/koɲ.ɲi.tɕi.wa/'),
  ('Korean', '/an.njʌŋ.ha.se.jo/'),
  ('Mandarin Chinese', '/ni˨˩ xaʊ˨˩˦/'),
  ('Vietnamese', '/sin˧ t͡ɕaːw˨˩/'),
  ('Thai', '/sa˨˩.wat̚˨˩.diː˧/'),
  ('Indonesian', '/ˈha.lo/'),
  ('Filipino', '/kuˈmus.ta/'),
  ('Swahili', '/ˈʄam.bo/'),
  ('Zulu', '/sa.wu.ˈɓɔː.na/'),
  ('Hawaiian', '/aˈlo.ha/'),
  ('Māori', '/ˈki.a ˈɔ.ɾa/')
) AS v(language, pronunciation)
WHERE g.language = v.language;

ALTER TABLE meetings ADD COLUMN greeting_pronunciation TEXT NOT NULL DEFAULT '';

UPDATE meetings m SET greeting_pronunciation = g.pronunciation
FROM greetings g WHERE g.language = m.greeting_language;

ALTER TABLE meetings DROP COLUMN icebreaker_skipped;
"#;

const DOWN: &str = r#"
ALTER TABLE meetings ADD COLUMN icebreaker_skipped UUID[] NOT NULL DEFAULT '{}';
ALTER TABLE meetings DROP COLUMN greeting_pronunciation;
ALTER TABLE greetings DROP COLUMN pronunciation;
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
