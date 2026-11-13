use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Lifecycle of an invitation (spec §6.1).
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize, ToSchema,
)]
#[sea_orm(rs_type = "String", db_type = "Enum", enum_name = "invitation_status")]
#[serde(rename_all = "lowercase")]
pub enum InvitationStatus {
    #[sea_orm(string_value = "pending")]
    Pending,
    #[sea_orm(string_value = "accepted")]
    Accepted,
    #[sea_orm(string_value = "declined")]
    Declined,
    #[sea_orm(string_value = "revoked")]
    Revoked,
    #[sea_orm(string_value = "expired")]
    Expired,
}

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "invitations")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub team_id: Uuid,
    /// Always stored lower-cased; matched against `users.email`.
    pub email: String,
    /// Secret of the personal accept link (`/invite/{token}`).
    #[sea_orm(unique)]
    pub token: String,
    pub invited_by: Option<Uuid>,
    pub status: InvitationStatus,
    pub expires_at: TimeDateTimeWithTimeZone,
    pub created_at: TimeDateTimeWithTimeZone,
}

impl Model {
    /// The status as seen now: a pending invitation past its expiry is expired.
    pub fn effective_status(&self, now: TimeDateTimeWithTimeZone) -> InvitationStatus {
        if self.status == InvitationStatus::Pending && self.expires_at <= now {
            InvitationStatus::Expired
        } else {
            self.status
        }
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
