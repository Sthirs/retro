use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "users")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    /// Always stored lower-cased.
    #[sea_orm(unique)]
    pub email: String,
    /// Always stored lower-cased. Only local accounts have one.
    #[sea_orm(unique)]
    pub username: Option<String>,
    pub display_name: String,
    pub avatar_url: Option<String>,
    /// Argon2id PHC string. `None` for accounts that only sign in through SSO.
    pub password_hash: Option<String>,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
