use sea_orm::entity::prelude::*;

/// An image attachment, re-encoded as WebP on upload (spec §13, ADR-0013).
/// `uploader_id` is never exposed: reflection images would reveal their author.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "attachments")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub team_id: Uuid,
    pub uploader_id: Uuid,
    pub content_type: String,
    pub animated: bool,
    pub width: i32,
    pub height: i32,
    pub size_bytes: i32,
    pub sha256: String,
    pub data: Vec<u8>,
    pub created_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
