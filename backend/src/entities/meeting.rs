use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Whether a meeting is running (spec §8.1).
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize, ToSchema,
)]
#[sea_orm(rs_type = "String", db_type = "Enum", enum_name = "meeting_status")]
#[serde(rename_all = "lowercase")]
pub enum MeetingStatus {
    #[sea_orm(string_value = "active")]
    Active,
    #[sea_orm(string_value = "ended")]
    Ended,
}

/// The retrospective phases, in order (spec §9).
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    EnumIter,
    DeriveActiveEnum,
    Serialize,
    Deserialize,
    ToSchema,
)]
#[sea_orm(rs_type = "String", db_type = "Enum", enum_name = "meeting_phase")]
#[serde(rename_all = "lowercase")]
pub enum MeetingPhase {
    #[sea_orm(string_value = "icebreaker")]
    Icebreaker,
    #[sea_orm(string_value = "reflect")]
    Reflect,
    #[sea_orm(string_value = "group")]
    Group,
    #[sea_orm(string_value = "vote")]
    Vote,
    #[sea_orm(string_value = "discuss")]
    Discuss,
}

impl MeetingPhase {
    /// The phase after this one; `None` after Discuss.
    pub fn next(self) -> Option<Self> {
        match self {
            Self::Icebreaker => Some(Self::Reflect),
            Self::Reflect => Some(Self::Group),
            Self::Group => Some(Self::Vote),
            Self::Vote => Some(Self::Discuss),
            Self::Discuss => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "meetings")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub team_id: Uuid,
    /// 1, 2, … per team; gives the default title "Retro #N".
    pub number: i32,
    pub title: String,
    pub created_by: Uuid,
    pub facilitator_id: Uuid,
    pub status: MeetingStatus,
    /// Token of the meeting invite link (`/m/{token}`); `None` once ended.
    #[sea_orm(unique)]
    pub invite_token: Option<String>,
    pub phase: MeetingPhase,
    pub greeting_text: String,
    pub greeting_language: String,
    pub greeting_lang_code: String,
    /// IPA, e.g. "/ˈtʃa.o/".
    pub greeting_pronunciation: String,
    pub icebreaker_question: String,
    /// Speaking order of the icebreaker (user ids).
    pub icebreaker_order: Vec<Uuid>,
    /// Index into `icebreaker_order` of the person speaking now.
    pub icebreaker_index: i32,
    pub vote_budget: i32,
    pub vote_max_per_group: i32,
    pub discuss_group_id: Option<Uuid>,
    pub timer_state: Option<Json>,
    /// Last `seq` sent on `meeting:{id}` (ADR-0011).
    pub event_seq: i64,
    pub created_at: TimeDateTimeWithTimeZone,
    pub ended_at: Option<TimeDateTimeWithTimeZone>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

#[cfg(test)]
mod tests {
    use super::MeetingPhase;

    #[test]
    fn phases_advance_in_order_and_stop_after_discuss() {
        let mut phase = MeetingPhase::Icebreaker;
        let mut seen = vec![phase];
        while let Some(next) = phase.next() {
            assert!(next > phase);
            seen.push(next);
            phase = next;
        }
        assert_eq!(seen.len(), 5);
        assert_eq!(phase, MeetingPhase::Discuss);
    }
}
