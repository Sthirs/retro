use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// The Reflect columns (spec §9.2).
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    EnumIter,
    DeriveActiveEnum,
    Serialize,
    Deserialize,
    ToSchema,
)]
#[sea_orm(rs_type = "String", db_type = "Enum", enum_name = "reflection_column")]
#[serde(rename_all = "lowercase")]
pub enum ReflectionColumn {
    #[sea_orm(string_value = "plus")]
    Plus,
    #[sea_orm(string_value = "delta")]
    Delta,
    #[sea_orm(string_value = "ideas")]
    Ideas,
    #[sea_orm(string_value = "thanks")]
    Thanks,
}

impl ReflectionColumn {
    pub const ALL: [Self; 4] = [Self::Plus, Self::Delta, Self::Ideas, Self::Thanks];

    /// Position in [`Self::ALL`].
    pub fn index(self) -> usize {
        self as usize
    }
}

/// A sent reflection. `author_id` is never exposed to other users.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "reflections")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub meeting_id: Uuid,
    pub author_id: Uuid,
    pub column: ReflectionColumn,
    pub content: String,
    /// Its group, from Group on.
    pub group_id: Option<Uuid>,
    /// Place in its column, set when Group starts.
    pub position: i32,
    /// Raised by every move between groups.
    pub version: i32,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

#[cfg(test)]
mod tests {
    use super::ReflectionColumn;

    #[test]
    fn columns_index_in_order() {
        for (i, column) in ReflectionColumn::ALL.into_iter().enumerate() {
            assert_eq!(column.index(), i);
        }
    }
}
