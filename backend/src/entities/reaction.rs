use sea_orm::entity::prelude::*;

/// One emoji a participant put on a reflection during Discuss (spec §9.5):
/// at most one of each emoji per person and reflection.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "reactions")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub reflection_id: Uuid,
    #[sea_orm(primary_key, auto_increment = false)]
    pub user_id: Uuid,
    /// A fully qualified Unicode emoji sequence.
    #[sea_orm(primary_key, auto_increment = false)]
    pub emoji: String,
    pub created_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
