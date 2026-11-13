use sea_orm::entity::prelude::*;

/// A single-use refresh token. Only its SHA-256 hash is stored.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "refresh_tokens")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub user_id: Uuid,
    /// All tokens rotated from the same sign-in share a family.
    pub family_id: Uuid,
    #[sea_orm(unique)]
    pub token_hash: String,
    /// "Keep me signed in": the cookie outlives the browser session.
    pub persistent: bool,
    pub expires_at: TimeDateTimeWithTimeZone,
    pub revoked_at: Option<TimeDateTimeWithTimeZone>,
    pub replaced_by: Option<Uuid>,
    pub created_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
