use sea_orm::entity::prelude::*;

/// An uploaded profile picture, stored re-encoded as bytes (one per user).
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "user_pictures")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub user_id: Uuid,
    pub content_type: String,
    pub size_bytes: i32,
    pub sha256: String,
    pub data: Vec<u8>,
    pub created_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
