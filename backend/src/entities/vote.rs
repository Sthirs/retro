use sea_orm::entity::prelude::*;

/// One vote a participant gave a group during Vote (spec §9.4).
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "votes")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub meeting_id: Uuid,
    pub user_id: Uuid,
    pub group_id: Uuid,
    pub created_at: TimeDateTimeWithTimeZone,
    /// The order the server accepted votes in (generated): "most recent"
    /// means the highest ordinal, never the latest `created_at`.
    pub ordinal: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
