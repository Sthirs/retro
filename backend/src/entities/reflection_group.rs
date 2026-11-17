use sea_orm::entity::prelude::*;

use super::reflection::ReflectionColumn;

/// A named stack of reflections (spec §9.3), shown in the column and at the
/// place of the card it was created on.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "reflection_groups")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub meeting_id: Uuid,
    pub column: ReflectionColumn,
    pub position: i32,
    /// 0–80 characters; empty shows as "Untitled group".
    pub name: String,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
