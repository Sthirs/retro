//! Teams, memberships and the team join link (spec §5, §6.2).
//!
//! Every mutation that can affect the set of admins locks the team's
//! membership rows first, so "at least one admin" holds under concurrency
//! (spec §16.3).

use std::collections::HashMap;

use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbBackend, EntityTrait, FromQueryResult,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set, Statement, TransactionSession,
    TransactionTrait,
};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    domain::random_token,
    entities::{
        team,
        team_member::{self, Role},
        user,
    },
    error::AppError,
};

/// Longest team name, in characters.
pub const TEAM_NAME_MAX: usize = 60;

/// Trims a team name and checks its length.
pub fn validate_team_name(raw: &str) -> Result<String, AppError> {
    let name = raw.trim();
    if name.is_empty() || name.chars().count() > TEAM_NAME_MAX {
        return Err(AppError::Invalid("invalid_team_name"));
    }
    Ok(name.to_owned())
}

/// A team as seen by one of its members.
#[derive(Debug, Clone, FromQueryResult)]
pub struct TeamSummary {
    pub id: Uuid,
    pub name: String,
    pub role: Role,
    pub member_count: i64,
}

/// A member of a team with their user record.
#[derive(Debug, Clone)]
pub struct Member {
    pub user: user::Model,
    pub role: Role,
    pub joined_at: OffsetDateTime,
}

/// What removing a member would do to the team.
#[derive(Debug, PartialEq, Eq)]
pub enum Removal {
    Allowed,
    /// The target is the only admin and others remain.
    LastAdmin,
    /// The target is the only member: the team goes with them.
    DeletesTeam,
}

/// Decides whether a member can leave or be removed (spec §5.1).
pub fn removal_outcome(target_is_admin: bool, admins: usize, members: usize) -> Removal {
    if members <= 1 {
        Removal::DeletesTeam
    } else if target_is_admin && admins <= 1 {
        Removal::LastAdmin
    } else {
        Removal::Allowed
    }
}

const SUMMARY_SQL: &str = r#"
SELECT t.id, t.name, m.role::text AS role,
       (SELECT count(*) FROM team_members c WHERE c.team_id = t.id) AS member_count
FROM teams t
JOIN team_members m ON m.team_id = t.id AND m.user_id = $1
"#;

/// The teams `user_id` belongs to, sorted by name.
pub async fn list_for_user(
    db: &impl ConnectionTrait,
    user_id: Uuid,
) -> Result<Vec<TeamSummary>, AppError> {
    let sql = format!("{SUMMARY_SQL} ORDER BY lower(t.name), t.id");
    Ok(
        TeamSummary::find_by_statement(Statement::from_sql_and_values(
            DbBackend::Postgres,
            sql,
            [user_id.into()],
        ))
        .all(db)
        .await?,
    )
}

/// One team of `user_id`. Non-members get `team_not_found`, so the API does
/// not reveal which teams exist.
pub async fn summary(
    db: &impl ConnectionTrait,
    team_id: Uuid,
    user_id: Uuid,
) -> Result<TeamSummary, AppError> {
    let sql = format!("{SUMMARY_SQL} WHERE t.id = $2");
    TeamSummary::find_by_statement(Statement::from_sql_and_values(
        DbBackend::Postgres,
        sql,
        [user_id.into(), team_id.into()],
    ))
    .one(db)
    .await?
    .ok_or(AppError::NotFound("team_not_found"))
}

/// The caller's role in the team; `team_not_found` when not a member.
pub async fn require_member(
    db: &impl ConnectionTrait,
    team_id: Uuid,
    user_id: Uuid,
) -> Result<Role, AppError> {
    team_member::Entity::find_by_id((team_id, user_id))
        .one(db)
        .await?
        .map(|m| m.role)
        .ok_or(AppError::NotFound("team_not_found"))
}

/// Fails unless the caller is an admin of the team.
pub async fn require_admin(
    db: &impl ConnectionTrait,
    team_id: Uuid,
    user_id: Uuid,
) -> Result<(), AppError> {
    match require_member(db, team_id, user_id).await? {
        Role::Admin => Ok(()),
        Role::Member => Err(AppError::Forbidden),
    }
}

/// Creates a team with `creator` as its first admin and an enabled join link.
pub async fn create(
    db: &(impl TransactionTrait + ConnectionTrait),
    creator: Uuid,
    name: &str,
) -> Result<TeamSummary, AppError> {
    let name = validate_team_name(name)?;
    let now = OffsetDateTime::now_utc();
    let id = Uuid::now_v7();
    let txn = db.begin().await?;
    team::ActiveModel {
        id: Set(id),
        name: Set(name.clone()),
        join_token: Set(Some(random_token())),
        join_enabled: Set(true),
        created_by: Set(Some(creator)),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(&txn)
    .await?;
    team_member::ActiveModel {
        team_id: Set(id),
        user_id: Set(creator),
        role: Set(Role::Admin),
        joined_at: Set(now),
    }
    .insert(&txn)
    .await?;
    txn.commit().await?;
    Ok(TeamSummary {
        id,
        name,
        role: Role::Admin,
        member_count: 1,
    })
}

/// Renames a team (admin).
pub async fn rename(
    db: &impl ConnectionTrait,
    team_id: Uuid,
    actor: Uuid,
    name: &str,
) -> Result<TeamSummary, AppError> {
    require_admin(db, team_id, actor).await?;
    let name = validate_team_name(name)?;
    team::ActiveModel {
        id: Set(team_id),
        name: Set(name),
        updated_at: Set(OffsetDateTime::now_utc()),
        ..Default::default()
    }
    .update(db)
    .await?;
    summary(db, team_id, actor).await
}

/// Deletes a team and, through `ON DELETE CASCADE`, everything in it (admin).
pub async fn delete(db: &impl ConnectionTrait, team_id: Uuid, actor: Uuid) -> Result<(), AppError> {
    require_admin(db, team_id, actor).await?;
    team::Entity::delete_by_id(team_id).exec(db).await?;
    Ok(())
}

/// The members of a team, admins first, then by name.
pub async fn members(
    db: &impl ConnectionTrait,
    team_id: Uuid,
    actor: Uuid,
) -> Result<Vec<Member>, AppError> {
    require_member(db, team_id, actor).await?;
    let rows = team_member::Entity::find()
        .filter(team_member::Column::TeamId.eq(team_id))
        .all(db)
        .await?;
    let mut users = users_by_id(db, rows.iter().map(|m| m.user_id)).await?;
    let mut members: Vec<Member> = rows
        .into_iter()
        .filter_map(|m| {
            users.remove(&m.user_id).map(|user| Member {
                user,
                role: m.role,
                joined_at: m.joined_at,
            })
        })
        .collect();
    members.sort_by(|a, b| {
        (a.role != Role::Admin)
            .cmp(&(b.role != Role::Admin))
            .then_with(|| {
                a.user
                    .display_name
                    .to_lowercase()
                    .cmp(&b.user.display_name.to_lowercase())
            })
    });
    Ok(members)
}

/// Loads users by id.
pub async fn users_by_id(
    db: &impl ConnectionTrait,
    ids: impl IntoIterator<Item = Uuid>,
) -> Result<HashMap<Uuid, user::Model>, AppError> {
    let ids: Vec<Uuid> = ids.into_iter().collect();
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    Ok(user::Entity::find()
        .filter(user::Column::Id.is_in(ids))
        .all(db)
        .await?
        .into_iter()
        .map(|u| (u.id, u))
        .collect())
}

/// Loads a team's memberships with row locks held until the transaction ends.
async fn lock_members(
    txn: &impl ConnectionTrait,
    team_id: Uuid,
) -> Result<Vec<team_member::Model>, AppError> {
    Ok(team_member::Entity::find()
        .filter(team_member::Column::TeamId.eq(team_id))
        .lock_exclusive()
        .all(txn)
        .await?)
}

/// Promotes or demotes a member (admin). The last admin cannot be demoted.
pub async fn change_role(
    db: &(impl TransactionTrait + ConnectionTrait),
    team_id: Uuid,
    actor: Uuid,
    target: Uuid,
    role: Role,
) -> Result<(), AppError> {
    let txn = db.begin().await?;
    let rows = lock_members(&txn, team_id).await?;
    match rows.iter().find(|m| m.user_id == actor) {
        None => return Err(AppError::NotFound("team_not_found")),
        Some(m) if m.role != Role::Admin => return Err(AppError::Forbidden),
        Some(_) => {}
    }
    let Some(current) = rows.iter().find(|m| m.user_id == target) else {
        return Err(AppError::NotFound("member_not_found"));
    };
    if current.role == role {
        return Ok(());
    }
    let admins = rows.iter().filter(|m| m.role == Role::Admin).count();
    if current.role == Role::Admin && admins <= 1 {
        return Err(AppError::Conflict("last_admin"));
    }
    let mut active: team_member::ActiveModel = current.clone().into();
    active.role = Set(role);
    active.update(&txn).await?;
    txn.commit().await?;
    Ok(())
}

/// Removes `target` from the team: an admin removing someone, or anyone
/// leaving. When the last member leaves, the team is deleted.
pub async fn remove_member(
    db: &(impl TransactionTrait + ConnectionTrait),
    team_id: Uuid,
    actor: Uuid,
    target: Uuid,
) -> Result<(), AppError> {
    let txn = db.begin().await?;
    let rows = lock_members(&txn, team_id).await?;
    let Some(me) = rows.iter().find(|m| m.user_id == actor) else {
        return Err(AppError::NotFound("team_not_found"));
    };
    if actor != target && me.role != Role::Admin {
        return Err(AppError::Forbidden);
    }
    let Some(member) = rows.iter().find(|m| m.user_id == target) else {
        return Err(AppError::NotFound("member_not_found"));
    };
    let admins = rows.iter().filter(|m| m.role == Role::Admin).count();
    match removal_outcome(member.role == Role::Admin, admins, rows.len()) {
        Removal::LastAdmin => return Err(AppError::Conflict("last_admin")),
        Removal::DeletesTeam => {
            team::Entity::delete_by_id(team_id).exec(&txn).await?;
        }
        Removal::Allowed => {
            team_member::Entity::delete_by_id((team_id, target))
                .exec(&txn)
                .await?;
        }
    }
    txn.commit().await?;
    Ok(())
}

/// Adds `user_id` as a member unless they already belong to the team.
/// Returns true when a membership was created.
pub async fn add_member(
    db: &impl ConnectionTrait,
    team_id: Uuid,
    user_id: Uuid,
) -> Result<bool, AppError> {
    let result = db
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "INSERT INTO team_members (team_id, user_id, role, joined_at) \
             VALUES ($1, $2, 'member', now()) ON CONFLICT DO NOTHING",
            [team_id.into(), user_id.into()],
        ))
        .await?;
    Ok(result.rows_affected() == 1)
}

/// State of a team's join link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JoinLink {
    pub token: Option<String>,
    pub enabled: bool,
}

/// What an admin does to the join link.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinLinkAction {
    Regenerate,
    Enable,
    Disable,
}

/// The join link of a team (any member may copy it).
pub async fn join_link(
    db: &impl ConnectionTrait,
    team_id: Uuid,
    actor: Uuid,
) -> Result<JoinLink, AppError> {
    require_member(db, team_id, actor).await?;
    let team = team::Entity::find_by_id(team_id)
        .one(db)
        .await?
        .ok_or(AppError::NotFound("team_not_found"))?;
    Ok(JoinLink {
        token: team.join_token,
        enabled: team.join_enabled,
    })
}

/// Regenerates (invalidating the old token), enables or disables the join
/// link (admin).
pub async fn update_join_link(
    db: &impl ConnectionTrait,
    team_id: Uuid,
    actor: Uuid,
    action: JoinLinkAction,
) -> Result<JoinLink, AppError> {
    require_admin(db, team_id, actor).await?;
    let team = team::Entity::find_by_id(team_id)
        .one(db)
        .await?
        .ok_or(AppError::NotFound("team_not_found"))?;
    let (token, enabled) = match action {
        JoinLinkAction::Regenerate => (Some(random_token()), team.join_enabled),
        JoinLinkAction::Enable => (team.join_token.or_else(|| Some(random_token())), true),
        JoinLinkAction::Disable => (team.join_token, false),
    };
    team::ActiveModel {
        id: Set(team_id),
        join_token: Set(token.clone()),
        join_enabled: Set(enabled),
        updated_at: Set(OffsetDateTime::now_utc()),
        ..Default::default()
    }
    .update(db)
    .await?;
    Ok(JoinLink { token, enabled })
}

/// Joins the team behind an enabled join link as a member. Returns the team
/// and whether the user was already a member.
pub async fn join_with_token(
    db: &impl ConnectionTrait,
    token: &str,
    user_id: Uuid,
) -> Result<(team::Model, bool), AppError> {
    let team = team::Entity::find()
        .filter(team::Column::JoinToken.eq(token))
        .filter(team::Column::JoinEnabled.eq(true))
        .one(db)
        .await?
        .ok_or(AppError::NotFound("join_link_invalid"))?;
    let added = add_member(db, team.id, user_id).await?;
    Ok((team, !added))
}

/// Up to `limit` members of a team (for previews), with the total count.
pub async fn member_preview(
    db: &impl ConnectionTrait,
    team_id: Uuid,
    limit: u64,
) -> Result<(Vec<user::Model>, i64), AppError> {
    let rows = team_member::Entity::find()
        .filter(team_member::Column::TeamId.eq(team_id))
        .order_by_asc(team_member::Column::JoinedAt)
        .limit(limit)
        .all(db)
        .await?;
    let mut by_id = users_by_id(db, rows.iter().map(|m| m.user_id)).await?;
    let users = rows
        .iter()
        .filter_map(|m| by_id.remove(&m.user_id))
        .collect();
    let count = team_member::Entity::find()
        .filter(team_member::Column::TeamId.eq(team_id))
        .count(db)
        .await?;
    Ok((users, count as i64))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_team_names() {
        assert_eq!(
            validate_team_name("  Platform Team ").unwrap(),
            "Platform Team"
        );
        assert!(validate_team_name("   ").is_err());
        assert!(validate_team_name("").is_err());
        assert!(validate_team_name(&"x".repeat(TEAM_NAME_MAX)).is_ok());
        assert!(validate_team_name(&"x".repeat(TEAM_NAME_MAX + 1)).is_err());
        // Characters, not bytes.
        assert!(validate_team_name(&"é".repeat(TEAM_NAME_MAX)).is_ok());
    }

    #[test]
    fn removal_rules() {
        assert_eq!(removal_outcome(true, 1, 1), Removal::DeletesTeam);
        assert_eq!(removal_outcome(true, 1, 3), Removal::LastAdmin);
        assert_eq!(removal_outcome(true, 2, 3), Removal::Allowed);
        assert_eq!(removal_outcome(false, 1, 3), Removal::Allowed);
    }
}
