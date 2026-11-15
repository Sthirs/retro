use sea_orm::entity::prelude::*;

/// "Hello" in one language, shown at the start of a meeting (spec §9.1).
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "greetings")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    /// English name of the language, e.g. "Italian".
    pub language: String,
    /// BCP 47 tag, e.g. "it".
    pub lang_code: String,
    pub text: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
