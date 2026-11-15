use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Where a task sits on the board (spec §9).
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize, ToSchema,
)]
#[sea_orm(rs_type = "String", db_type = "Enum", enum_name = "task_status")]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    #[sea_orm(string_value = "future")]
    Future,
    #[sea_orm(string_value = "stuck")]
    Stuck,
    #[sea_orm(string_value = "active")]
    Active,
    #[sea_orm(string_value = "done")]
    Done,
}

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "tasks")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub team_id: Uuid,
    pub meeting_id: Option<Uuid>,
    pub group_id: Option<Uuid>,
    pub author_id: Uuid,
    pub content: String,
    pub status: TaskStatus,
    /// Fractional-index ordering key within `(team_id, status)`; `COLLATE "C"`.
    pub position: String,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
