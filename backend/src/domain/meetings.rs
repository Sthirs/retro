//! Meetings: lifecycle, participants, facilitator and the icebreaker
//! (spec §8, §9.1). Every change publishes its real-time events inside its
//! transaction (ADR-0011).

use std::collections::HashMap;

use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbBackend, EntityTrait, FromQueryResult,
    QueryFilter, QueryOrder, QuerySelect, Set, Statement, TransactionSession, TransactionTrait,
};
use serde_json::{Value, json};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    domain::{random_token, teams},
    entities::{
        greeting, icebreaker_question,
        meeting::{self, MeetingPhase, MeetingStatus},
        meeting_participant,
        team::{self},
        team_member::Role,
        user,
    },
    error::AppError,
    ws,
};

/// Recent meetings whose greeting and question a new meeting avoids.
const RECENT_PICKS: u64 = 10;
/// Longest meeting title, in characters.
pub const TITLE_MAX_CHARS: usize = 80;

/// A participant with their current team role (`None` once they left the team).
#[derive(Debug, Clone)]
pub struct Participant {
    pub user: user::Model,
    pub role: Option<Role>,
}

/// A meeting with what its screens show next to it.
#[derive(Debug, Clone)]
pub struct MeetingView {
    pub meeting: meeting::Model,
    pub team_name: String,
    pub facilitator: user::Model,
    /// In joining order.
    pub participants: Vec<Participant>,
    pub task_count: i64,
}

/// The default title of the team's `number`-th meeting.
pub fn default_title(number: i32) -> String {
    format!("Retro #{number}")
}

/// Trims a meeting title; `invalid_title` when empty or too long.
pub fn validate_title(raw: &str) -> Result<String, AppError> {
    let title = raw.trim();
    if title.is_empty() || title.chars().count() > TITLE_MAX_CHARS {
        return Err(AppError::Invalid("invalid_title"));
    }
    Ok(title.to_owned())
}

/// Where a newcomer goes in the icebreaker order. While the first person is
/// still speaking (`index == 0`), they take a random place among those who
/// have not spoken yet (`roll` picks in the given range), which shuffles
/// everyone who joins at the start; afterwards they go to the end (spec §9.1).
pub fn join_position(
    order_len: usize,
    index: usize,
    roll: impl FnOnce(std::ops::RangeInclusive<usize>) -> usize,
) -> usize {
    if index == 0 && order_len >= 1 {
        roll(1..=order_len)
    } else {
        order_len
    }
}

/// Loads the meeting for `actor`, who must be a member of its team
/// (`meeting_not_found` otherwise), and returns their team role.
async fn find_for_member(
    db: &impl ConnectionTrait,
    meeting_id: Uuid,
    actor: Uuid,
    lock: bool,
) -> Result<(meeting::Model, Role), AppError> {
    let meeting = meeting::Entity::find_by_id(meeting_id)
        .one(db)
        .await?
        .ok_or(AppError::NotFound("meeting_not_found"))?;
    let role = teams::require_member(db, meeting.team_id, actor)
        .await
        .map_err(|e| match e {
            AppError::NotFound(_) => AppError::NotFound("meeting_not_found"),
            other => other,
        })?;
    if !lock {
        return Ok((meeting, role));
    }
    let meeting = meeting::Entity::find_by_id(meeting_id)
        .lock_exclusive()
        .one(db)
        .await?
        .ok_or(AppError::NotFound("meeting_not_found"))?;
    Ok((meeting, role))
}

/// Locks a running meeting for a facilitator action.
async fn lock_as_facilitator(
    txn: &impl ConnectionTrait,
    meeting_id: Uuid,
    actor: Uuid,
) -> Result<meeting::Model, AppError> {
    let (meeting, _) = find_for_member(txn, meeting_id, actor, true).await?;
    if meeting.status != MeetingStatus::Active {
        return Err(AppError::Conflict("meeting_ended"));
    }
    if meeting.facilitator_id != actor {
        return Err(AppError::Denied("not_facilitator"));
    }
    Ok(meeting)
}

/// The team's meetings, newest first, optionally of one status.
pub async fn list(
    db: &impl ConnectionTrait,
    team_id: Uuid,
    actor: Uuid,
    status: Option<MeetingStatus>,
) -> Result<Vec<MeetingView>, AppError> {
    teams::require_member(db, team_id, actor).await?;
    let mut query = meeting::Entity::find().filter(meeting::Column::TeamId.eq(team_id));
    if let Some(status) = status {
        query = query.filter(meeting::Column::Status.eq(status));
    }
    let meetings = query
        .order_by_desc(meeting::Column::CreatedAt)
        .order_by_desc(meeting::Column::Id)
        .all(db)
        .await?;
    views(db, meetings).await
}

/// One meeting, for a member of its team.
pub async fn view(
    db: &impl ConnectionTrait,
    meeting_id: Uuid,
    actor: Uuid,
) -> Result<MeetingView, AppError> {
    let (meeting, _) = find_for_member(db, meeting_id, actor, false).await?;
    views(db, vec![meeting])
        .await?
        .pop()
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("meeting is missing")))
}

#[derive(Debug, FromQueryResult)]
struct ParticipantRow {
    meeting_id: Uuid,
    user_id: Uuid,
    role: Option<Role>,
}

#[derive(Debug, FromQueryResult)]
struct CountRow {
    meeting_id: Uuid,
    count: i64,
}

/// Attaches team name, facilitator, participants and task counts in bulk.
async fn views(
    db: &impl ConnectionTrait,
    meetings: Vec<meeting::Model>,
) -> Result<Vec<MeetingView>, AppError> {
    if meetings.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<Uuid> = meetings.iter().map(|m| m.id).collect();
    let id_values: Vec<sea_orm::Value> = ids.iter().map(|id| (*id).into()).collect();
    let placeholders = (1..=ids.len())
        .map(|i| format!("${i}"))
        .collect::<Vec<_>>()
        .join(", ");

    let rows = ParticipantRow::find_by_statement(Statement::from_sql_and_values(
        DbBackend::Postgres,
        format!(
            "SELECT p.meeting_id, p.user_id, tm.role \
             FROM meeting_participants p \
             JOIN meetings m ON m.id = p.meeting_id \
             LEFT JOIN team_members tm ON tm.team_id = m.team_id AND tm.user_id = p.user_id \
             WHERE p.meeting_id IN ({placeholders}) \
             ORDER BY p.joined_at, p.user_id"
        ),
        id_values.clone(),
    ))
    .all(db)
    .await?;

    let counts: HashMap<Uuid, i64> = CountRow::find_by_statement(Statement::from_sql_and_values(
        DbBackend::Postgres,
        format!(
            "SELECT meeting_id, COUNT(*) AS count FROM tasks \
             WHERE meeting_id IN ({placeholders}) GROUP BY meeting_id"
        ),
        id_values,
    ))
    .all(db)
    .await?
    .into_iter()
    .map(|r| (r.meeting_id, r.count))
    .collect();

    let team_ids: Vec<Uuid> = meetings.iter().map(|m| m.team_id).collect();
    let team_names: HashMap<Uuid, String> = team::Entity::find()
        .filter(team::Column::Id.is_in(team_ids))
        .all(db)
        .await?
        .into_iter()
        .map(|t| (t.id, t.name))
        .collect();

    let people = teams::users_by_id(
        db,
        rows.iter()
            .map(|r| r.user_id)
            .chain(meetings.iter().map(|m| m.facilitator_id)),
    )
    .await?;

    let mut participants: HashMap<Uuid, Vec<Participant>> = HashMap::new();
    for row in rows {
        if let Some(user) = people.get(&row.user_id) {
            participants
                .entry(row.meeting_id)
                .or_default()
                .push(Participant {
                    user: user.clone(),
                    role: row.role,
                });
        }
    }

    meetings
        .into_iter()
        .map(|meeting| {
            let facilitator = people
                .get(&meeting.facilitator_id)
                .cloned()
                .ok_or_else(|| {
                    AppError::Internal(anyhow::anyhow!("meeting facilitator is missing"))
                })?;
            Ok(MeetingView {
                team_name: team_names
                    .get(&meeting.team_id)
                    .cloned()
                    .unwrap_or_default(),
                facilitator,
                participants: participants.remove(&meeting.id).unwrap_or_default(),
                task_count: counts.get(&meeting.id).copied().unwrap_or(0),
                meeting,
            })
        })
        .collect()
}

/// A random greeting, avoiding the team's recent ones when possible.
async fn pick_greeting(
    db: &impl ConnectionTrait,
    team_id: Uuid,
) -> Result<greeting::Model, AppError> {
    let fresh = greeting::Model::find_by_statement(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT * FROM greetings WHERE language NOT IN ( \
           SELECT greeting_language FROM meetings WHERE team_id = $1 \
           ORDER BY created_at DESC LIMIT $2) \
         ORDER BY random() LIMIT 1",
        [team_id.into(), (RECENT_PICKS as i64).into()],
    ))
    .one(db)
    .await?;
    if let Some(greeting) = fresh {
        return Ok(greeting);
    }
    greeting::Model::find_by_statement(Statement::from_string(
        DbBackend::Postgres,
        "SELECT * FROM greetings ORDER BY random() LIMIT 1",
    ))
    .one(db)
    .await?
    .ok_or_else(|| AppError::Internal(anyhow::anyhow!("no greetings")))
}

/// A random active question, avoiding the team's recent ones when possible.
async fn pick_question(
    db: &impl ConnectionTrait,
    team_id: Uuid,
) -> Result<icebreaker_question::Model, AppError> {
    let fresh = icebreaker_question::Model::find_by_statement(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT * FROM icebreaker_questions WHERE active AND text NOT IN ( \
           SELECT icebreaker_question FROM meetings WHERE team_id = $1 \
           ORDER BY created_at DESC LIMIT $2) \
         ORDER BY random() LIMIT 1",
        [team_id.into(), (RECENT_PICKS as i64).into()],
    ))
    .one(db)
    .await?;
    if let Some(question) = fresh {
        return Ok(question);
    }
    icebreaker_question::Model::find_by_statement(Statement::from_string(
        DbBackend::Postgres,
        "SELECT * FROM icebreaker_questions WHERE active ORDER BY random() LIMIT 1",
    ))
    .one(db)
    .await?
    .ok_or_else(|| AppError::Internal(anyhow::anyhow!("no icebreaker questions")))
}

/// Starts a meeting; `actor` (a member) becomes its facilitator and first
/// participant. Publishes `meeting.started` on the team channel.
pub async fn create(
    db: &(impl TransactionTrait + ConnectionTrait),
    team_id: Uuid,
    actor: Uuid,
) -> Result<meeting::Model, AppError> {
    let txn = db.begin().await?;
    teams::require_member(&txn, team_id, actor).await?;
    // Serialises meeting numbers per team.
    team::Entity::find_by_id(team_id)
        .lock_exclusive()
        .one(&txn)
        .await?
        .ok_or(AppError::NotFound("team_not_found"))?;
    let number: i32 = meeting::Entity::find()
        .filter(meeting::Column::TeamId.eq(team_id))
        .select_only()
        .column_as(meeting::Column::Number.max(), "max")
        .into_tuple::<Option<i32>>()
        .one(&txn)
        .await?
        .flatten()
        .unwrap_or(0)
        + 1;
    let greeting = pick_greeting(&txn, team_id).await?;
    let question = pick_question(&txn, team_id).await?;

    let now = OffsetDateTime::now_utc();
    let meeting = meeting::ActiveModel {
        id: Set(Uuid::now_v7()),
        team_id: Set(team_id),
        number: Set(number),
        title: Set(default_title(number)),
        created_by: Set(actor),
        facilitator_id: Set(actor),
        status: Set(MeetingStatus::Active),
        invite_token: Set(Some(random_token())),
        phase: Set(MeetingPhase::Icebreaker),
        greeting_text: Set(greeting.text),
        greeting_language: Set(greeting.language),
        greeting_lang_code: Set(greeting.lang_code),
        greeting_pronunciation: Set(greeting.pronunciation),
        icebreaker_question: Set(question.text),
        icebreaker_order: Set(vec![actor]),
        icebreaker_index: Set(0),
        vote_budget: Set(5),
        vote_max_per_group: Set(3),
        discuss_group_id: Set(None),
        timer_state: Set(None),
        event_seq: Set(0),
        created_at: Set(now),
        ended_at: Set(None),
    }
    .insert(&txn)
    .await?;
    meeting_participant::ActiveModel {
        meeting_id: Set(meeting.id),
        user_id: Set(actor),
        joined_at: Set(now),
        is_ready: Set(false),
    }
    .insert(&txn)
    .await?;
    ws::publish_team(
        &txn,
        team_id,
        "meeting.started",
        json!({ "meetingId": meeting.id }),
    )
    .await?;
    txn.commit().await?;
    Ok(meeting)
}

/// Publishes `meeting.updated` on the meeting's team channel, so Meetings
/// pages refresh the entry.
async fn team_updated(db: &impl ConnectionTrait, meeting: &meeting::Model) -> Result<(), AppError> {
    ws::publish_team(
        db,
        meeting.team_id,
        "meeting.updated",
        json!({ "meetingId": meeting.id }),
    )
    .await?;
    Ok(())
}

/// Makes `actor` a participant of a running meeting (idempotent). During the
/// icebreaker a newcomer is added to the speaking order (see [`join_position`]).
pub async fn join(
    db: &(impl TransactionTrait + ConnectionTrait),
    meeting_id: Uuid,
    actor: Uuid,
) -> Result<(), AppError> {
    let txn = db.begin().await?;
    let (meeting, role) = find_for_member(&txn, meeting_id, actor, true).await?;
    join_locked(&txn, meeting, actor, Some(role)).await?;
    txn.commit().await?;
    Ok(())
}

/// [`join`] on a meeting already locked by the caller's transaction.
async fn join_locked(
    txn: &impl ConnectionTrait,
    meeting: meeting::Model,
    actor: Uuid,
    role: Option<Role>,
) -> Result<(), AppError> {
    if meeting.status != MeetingStatus::Active {
        return Err(AppError::Conflict("meeting_ended"));
    }
    if meeting_participant::Entity::find_by_id((meeting.id, actor))
        .one(txn)
        .await?
        .is_some()
    {
        return Ok(());
    }
    meeting_participant::ActiveModel {
        meeting_id: Set(meeting.id),
        user_id: Set(actor),
        joined_at: Set(OffsetDateTime::now_utc()),
        is_ready: Set(false),
    }
    .insert(txn)
    .await?;

    let mut order = meeting.icebreaker_order.clone();
    if meeting.phase == MeetingPhase::Icebreaker && !order.contains(&actor) {
        let index = usize::try_from(meeting.icebreaker_index).unwrap_or(0);
        let position = join_position(order.len(), index, rand::random_range);
        order.insert(position, actor);
        meeting::ActiveModel {
            id: Set(meeting.id),
            icebreaker_order: Set(order.clone()),
            ..Default::default()
        }
        .update(txn)
        .await?;
    }

    let user = user::Entity::find_by_id(actor)
        .one(txn)
        .await?
        .ok_or(AppError::Unauthorized)?;
    ws::publish_meeting(
        txn,
        meeting.id,
        "participant.joined",
        json!({
            "participant": participant_json(&user, role),
            "order": order,
        }),
    )
    .await?;
    team_updated(txn, &meeting).await
}

fn participant_json(user: &user::Model, role: Option<Role>) -> Value {
    json!({
        "id": user.id,
        "displayName": user.display_name,
        "pictureVersion": user.picture_version,
        "role": role,
        "online": false,
    })
}

/// Moves the icebreaker to the next person (facilitator). After the last
/// person, the facilitator moves to the next phase instead (`icebreaker_done`).
pub async fn icebreaker_next(
    db: &(impl TransactionTrait + ConnectionTrait),
    meeting_id: Uuid,
    actor: Uuid,
) -> Result<(), AppError> {
    let txn = db.begin().await?;
    let meeting = lock_icebreaker(&txn, meeting_id, actor).await?;
    let index = usize::try_from(meeting.icebreaker_index).unwrap_or(0);
    if index + 1 >= meeting.icebreaker_order.len() {
        return Err(AppError::Conflict("icebreaker_done"));
    }
    set_speaker(&txn, &meeting, meeting.icebreaker_index + 1).await?;
    txn.commit().await?;
    Ok(())
}

/// Goes back to `user_id`, who already had their turn (facilitator). The turn
/// continues from them: everyone after them is up again (spec §9.1).
pub async fn icebreaker_goto(
    db: &(impl TransactionTrait + ConnectionTrait),
    meeting_id: Uuid,
    actor: Uuid,
    user_id: Uuid,
) -> Result<(), AppError> {
    let txn = db.begin().await?;
    let meeting = lock_icebreaker(&txn, meeting_id, actor).await?;
    let position = meeting
        .icebreaker_order
        .iter()
        .position(|&id| id == user_id)
        .and_then(|p| i32::try_from(p).ok())
        .filter(|&p| p < meeting.icebreaker_index)
        .ok_or(AppError::Conflict("icebreaker_not_reached"))?;
    set_speaker(&txn, &meeting, position).await?;
    txn.commit().await?;
    Ok(())
}

/// The meeting, locked, for a facilitator action in the icebreaker.
async fn lock_icebreaker(
    txn: &impl ConnectionTrait,
    meeting_id: Uuid,
    actor: Uuid,
) -> Result<meeting::Model, AppError> {
    let meeting = lock_as_facilitator(txn, meeting_id, actor).await?;
    if meeting.phase != MeetingPhase::Icebreaker {
        return Err(AppError::Conflict("wrong_phase"));
    }
    Ok(meeting)
}

/// Makes the person at `index` in the speaking order the one speaking now.
async fn set_speaker(
    txn: &impl ConnectionTrait,
    meeting: &meeting::Model,
    index: i32,
) -> Result<(), AppError> {
    meeting::ActiveModel {
        id: Set(meeting.id),
        icebreaker_index: Set(index),
        ..Default::default()
    }
    .update(txn)
    .await?;
    ws::publish_meeting(
        txn,
        meeting.id,
        "icebreaker.person_changed",
        json!({ "index": index }),
    )
    .await?;
    Ok(())
}

/// Advances to the next phase (facilitator); ready flags reset. After Discuss
/// there is none (`last_phase`).
pub async fn next_phase(
    db: &(impl TransactionTrait + ConnectionTrait),
    meeting_id: Uuid,
    actor: Uuid,
) -> Result<(), AppError> {
    let txn = db.begin().await?;
    let meeting = lock_as_facilitator(&txn, meeting_id, actor).await?;
    let phase = meeting
        .phase
        .next()
        .ok_or(AppError::Conflict("last_phase"))?;
    meeting::ActiveModel {
        id: Set(meeting.id),
        phase: Set(phase),
        ..Default::default()
    }
    .update(&txn)
    .await?;
    meeting_participant::Entity::update_many()
        .col_expr(meeting_participant::Column::IsReady, false.into())
        .filter(meeting_participant::Column::MeetingId.eq(meeting.id))
        .exec(&txn)
        .await?;
    ws::publish_meeting(&txn, meeting.id, "phase.changed", json!({ "phase": phase })).await?;
    team_updated(&txn, &meeting).await?;
    txn.commit().await?;
    Ok(())
}

/// Hands the facilitator role to `target` (by the facilitator, to another
/// participant) or takes it over (a team admin naming themselves, who joins
/// the meeting if needed). Anything else is `not_facilitator`.
pub async fn set_facilitator(
    db: &(impl TransactionTrait + ConnectionTrait),
    meeting_id: Uuid,
    actor: Uuid,
    target: Uuid,
) -> Result<(), AppError> {
    let txn = db.begin().await?;
    let (meeting, role) = find_for_member(&txn, meeting_id, actor, true).await?;
    if meeting.status != MeetingStatus::Active {
        return Err(AppError::Conflict("meeting_ended"));
    }
    let take_over = target == actor && role == Role::Admin;
    if meeting.facilitator_id != actor && !take_over {
        return Err(AppError::Denied("not_facilitator"));
    }
    if target == meeting.facilitator_id {
        txn.commit().await?;
        return Ok(());
    }
    if take_over {
        join_locked(&txn, meeting.clone(), actor, Some(role)).await?;
    } else if meeting_participant::Entity::find_by_id((meeting.id, target))
        .one(&txn)
        .await?
        .is_none()
    {
        return Err(AppError::Invalid("target_not_participant"));
    }
    meeting::ActiveModel {
        id: Set(meeting.id),
        facilitator_id: Set(target),
        ..Default::default()
    }
    .update(&txn)
    .await?;
    ws::publish_meeting(
        &txn,
        meeting.id,
        "facilitator.changed",
        json!({ "facilitatorId": target, "previousId": meeting.facilitator_id, "takeOver": take_over }),
    )
    .await?;
    team_updated(&txn, &meeting).await?;
    txn.commit().await?;
    Ok(())
}

/// Renames a running meeting (facilitator).
pub async fn rename(
    db: &(impl TransactionTrait + ConnectionTrait),
    meeting_id: Uuid,
    actor: Uuid,
    raw_title: &str,
) -> Result<(), AppError> {
    let title = validate_title(raw_title)?;
    let txn = db.begin().await?;
    let meeting = lock_as_facilitator(&txn, meeting_id, actor).await?;
    meeting::ActiveModel {
        id: Set(meeting.id),
        title: Set(title.clone()),
        ..Default::default()
    }
    .update(&txn)
    .await?;
    ws::publish_meeting(
        &txn,
        meeting.id,
        "meeting.renamed",
        json!({ "title": title }),
    )
    .await?;
    ws::publish_team(
        &txn,
        meeting.team_id,
        "meeting.renamed",
        json!({ "meetingId": meeting.id, "title": title }),
    )
    .await?;
    txn.commit().await?;
    Ok(())
}

/// Ends a meeting (facilitator): read-only from now on, invite link gone.
pub async fn end(
    db: &(impl TransactionTrait + ConnectionTrait),
    meeting_id: Uuid,
    actor: Uuid,
) -> Result<(), AppError> {
    let txn = db.begin().await?;
    let meeting = lock_as_facilitator(&txn, meeting_id, actor).await?;
    let now = OffsetDateTime::now_utc();
    meeting::ActiveModel {
        id: Set(meeting.id),
        status: Set(MeetingStatus::Ended),
        ended_at: Set(Some(now)),
        invite_token: Set(None),
        timer_state: Set(None),
        ..Default::default()
    }
    .update(&txn)
    .await?;
    let ended_at = now
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|e| AppError::Internal(e.into()))?;
    ws::publish_meeting(
        &txn,
        meeting.id,
        "meeting.ended",
        json!({ "endedAt": ended_at }),
    )
    .await?;
    ws::publish_team(
        &txn,
        meeting.team_id,
        "meeting.ended",
        json!({ "meetingId": meeting.id }),
    )
    .await?;
    txn.commit().await?;
    Ok(())
}

/// The invite token of a running meeting (any team member).
pub async fn invite_token(
    db: &impl ConnectionTrait,
    meeting_id: Uuid,
    actor: Uuid,
) -> Result<String, AppError> {
    let (meeting, _) = find_for_member(db, meeting_id, actor, false).await?;
    meeting
        .invite_token
        .filter(|_| meeting.status == MeetingStatus::Active)
        .ok_or(AppError::Conflict("meeting_ended"))
}

/// Replaces the invite token, invalidating the old link (facilitator).
pub async fn regenerate_invite_token(
    db: &(impl TransactionTrait + ConnectionTrait),
    meeting_id: Uuid,
    actor: Uuid,
) -> Result<String, AppError> {
    let txn = db.begin().await?;
    let meeting = lock_as_facilitator(&txn, meeting_id, actor).await?;
    let token = random_token();
    meeting::ActiveModel {
        id: Set(meeting.id),
        invite_token: Set(Some(token.clone())),
        ..Default::default()
    }
    .update(&txn)
    .await?;
    txn.commit().await?;
    Ok(token)
}

/// Follows a meeting invite link: joins the team as a member if needed, then
/// the meeting. Unknown tokens and ended meetings are `meeting_invite_invalid`.
/// Returns the meeting and whether the user was added to the team.
pub async fn accept_invite(
    db: &(impl TransactionTrait + ConnectionTrait),
    token: &str,
    actor: Uuid,
) -> Result<(meeting::Model, bool), AppError> {
    let txn = db.begin().await?;
    let meeting = meeting::Entity::find()
        .filter(meeting::Column::InviteToken.eq(token))
        .filter(meeting::Column::Status.eq(MeetingStatus::Active))
        .lock_exclusive()
        .one(&txn)
        .await?
        .ok_or(AppError::NotFound("meeting_invite_invalid"))?;
    let joined_team = teams::add_member(&txn, meeting.team_id, actor).await?;
    if joined_team {
        crate::domain::invitations::close_for_member(&txn, meeting.team_id, actor).await?;
    }
    let role = teams::require_member(&txn, meeting.team_id, actor).await?;
    join_locked(&txn, meeting.clone(), actor, Some(role)).await?;
    txn.commit().await?;
    Ok((meeting, joined_team))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_titles_count_meetings() {
        assert_eq!(default_title(1), "Retro #1");
        assert_eq!(default_title(12), "Retro #12");
    }

    #[test]
    fn titles_are_trimmed_and_bounded() {
        assert_eq!(validate_title("  Kick-off  ").unwrap(), "Kick-off");
        assert!(validate_title("   ").is_err());
        assert!(validate_title(&"é".repeat(TITLE_MAX_CHARS)).is_ok());
        assert!(validate_title(&"é".repeat(TITLE_MAX_CHARS + 1)).is_err());
    }

    #[test]
    fn newcomers_are_shuffled_in_while_the_first_person_speaks() {
        // Only places after the current speaker are offered.
        let mut offered = None;
        let position = join_position(3, 0, |range| {
            offered = Some(range.clone());
            *range.end()
        });
        assert_eq!(offered, Some(1..=3));
        assert_eq!(position, 3);
        assert_eq!(join_position(1, 0, |range| *range.start()), 1);
    }

    #[test]
    fn late_newcomers_go_to_the_end() {
        assert_eq!(join_position(4, 1, |_| unreachable!()), 4);
        assert_eq!(join_position(4, 3, |_| unreachable!()), 4);
    }
}
