//! Emoji reactions on reflections (spec §9.5, ADR-0014): during Discuss,
//! participants react to the cards of the current group. Each person puts at
//! most one of each emoji on a card; reacting again takes it back.
//!
//! Emoji are validated against the Unicode emoji list and stored fully
//! qualified, so "❤" and "❤️" are the same reaction. Every change publishes
//! the emoji's new count to everyone (`reaction.changed`) and whether the
//! reactor now has it to their own tabs (`reaction.mine_changed`, ADR-0012).

use sea_orm::{
    ColumnTrait, ConnectionTrait, DbBackend, EntityTrait, FromQueryResult, PaginatorTrait,
    QueryFilter, Statement, TransactionSession, TransactionTrait, sea_query::OnConflict,
};
use serde::Serialize;
use serde_json::json;
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    domain::discuss::{lock_discussing, require_current},
    entities::{
        meeting::{self, MeetingPhase},
        reaction, reflection,
    },
    error::AppError,
    ws,
};

/// One emoji on one reflection: how many people put it there, and whether
/// the viewer is one of them.
#[derive(Debug, Clone, Serialize, ToSchema, PartialEq, Eq, FromQueryResult)]
#[serde(rename_all = "camelCase")]
pub struct ReactionDto {
    pub reflection_id: Uuid,
    pub emoji: String,
    pub count: i32,
    /// The viewer reacted with this emoji.
    pub mine: bool,
}

/// The fully qualified form of `raw`, or `invalid_emoji` unless it is
/// exactly one Unicode emoji.
pub fn normalize(raw: &str) -> Result<&'static str, AppError> {
    emojis::get(raw)
        .map(emojis::Emoji::as_str)
        .ok_or(AppError::Invalid("invalid_emoji"))
}

/// Every reaction of the meeting as `viewer` sees it, from Discuss on: by
/// reflection, each emoji in the order it was first used.
pub async fn snapshot(
    db: &impl ConnectionTrait,
    meeting: &meeting::Model,
    viewer: Uuid,
) -> Result<Vec<ReactionDto>, AppError> {
    if meeting.phase < MeetingPhase::Discuss {
        return Ok(Vec::new());
    }
    Ok(
        ReactionDto::find_by_statement(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT x.reflection_id, x.emoji, COUNT(*)::int4 AS count, \
                bool_or(x.user_id = $2) AS mine \
         FROM reactions x JOIN reflections r ON r.id = x.reflection_id \
         WHERE r.meeting_id = $1 \
         GROUP BY x.reflection_id, x.emoji \
         ORDER BY x.reflection_id, MIN(x.created_at), x.emoji",
            [meeting.id.into(), viewer.into()],
        ))
        .all(db)
        .await?,
    )
}

/// Puts `emoji` on a reflection (`on`) or takes it back, for `actor`, a
/// participant, during Discuss, on a card of the current group. Doing what
/// is already done changes nothing and publishes nothing.
pub async fn set(
    db: &(impl TransactionTrait + ConnectionTrait),
    reflection_id: Uuid,
    actor: Uuid,
    raw_emoji: &str,
    on: bool,
    client: Option<Uuid>,
) -> Result<ReactionDto, AppError> {
    let emoji = normalize(raw_emoji)?;
    let card = reflection::Entity::find_by_id(reflection_id)
        .one(db)
        .await?
        .ok_or(AppError::NotFound("reflection_not_found"))?;
    let txn = db.begin().await?;
    let meeting = lock_discussing(&txn, card.meeting_id, actor, "reflection_not_found").await?;
    require_current(&txn, &meeting, card.group_id).await?;

    let changed = if on {
        reaction::Entity::insert(reaction::ActiveModel {
            reflection_id: sea_orm::Set(card.id),
            user_id: sea_orm::Set(actor),
            emoji: sea_orm::Set(emoji.to_owned()),
            created_at: sea_orm::Set(OffsetDateTime::now_utc()),
        })
        .on_conflict(
            OnConflict::columns([
                reaction::Column::ReflectionId,
                reaction::Column::UserId,
                reaction::Column::Emoji,
            ])
            .do_nothing()
            .to_owned(),
        )
        .exec_without_returning(&txn)
        .await?
            > 0
    } else {
        reaction::Entity::delete_many()
            .filter(reaction::Column::ReflectionId.eq(card.id))
            .filter(reaction::Column::UserId.eq(actor))
            .filter(reaction::Column::Emoji.eq(emoji))
            .exec(&txn)
            .await?
            .rows_affected
            > 0
    };
    let count = reaction::Entity::find()
        .filter(reaction::Column::ReflectionId.eq(card.id))
        .filter(reaction::Column::Emoji.eq(emoji))
        .count(&txn)
        .await?;
    let count = i32::try_from(count).unwrap_or(i32::MAX);
    if changed {
        ws::publish_meeting(
            &txn,
            meeting.id,
            "reaction.changed",
            json!({ "reflectionId": card.id, "emoji": emoji, "count": count, "clientId": client }),
        )
        .await?;
        ws::publish_user(
            &txn,
            meeting.id,
            actor,
            "reaction.mine_changed",
            json!({ "reflectionId": card.id, "emoji": emoji, "mine": on, "clientId": client }),
        )
        .await?;
    }
    txn.commit().await?;
    Ok(ReactionDto {
        reflection_id: card.id,
        emoji: emoji.to_owned(),
        count,
        mine: on,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emoji_are_validated_and_fully_qualified() {
        assert_eq!(normalize("👍").unwrap(), "👍");
        assert_eq!(normalize("❤").unwrap(), "❤️");
        assert_eq!(normalize("❤️").unwrap(), "❤️");
        assert_eq!(normalize("👍🏽").unwrap(), "👍🏽");
        assert_eq!(normalize("🧑‍💻").unwrap(), "🧑‍💻");
        for bad in [
            "",
            "a",
            "👍👍",
            ":tada:",
            "<b>",
            " 👍",
            "x".repeat(200).as_str(),
        ] {
            assert!(
                matches!(normalize(bad), Err(AppError::Invalid("invalid_emoji"))),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn dtos_serialize_in_camel_case() {
        let id = Uuid::nil();
        assert_eq!(
            serde_json::to_value(ReactionDto {
                reflection_id: id,
                emoji: "🎉".into(),
                count: 2,
                mine: true
            })
            .unwrap(),
            json!({ "reflectionId": id, "emoji": "🎉", "count": 2, "mine": true })
        );
    }
}
