//! Invitations by email address (spec §6.1). The application sends no email:
//! the inviter shares the personal link `/invite/{token}`, and the invitee
//! also sees the invitation in their pending list when their email matches.
//! The token is the credential: whoever holds the link can accept it.

use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbBackend, EntityTrait, FromQueryResult,
    QueryFilter, QueryOrder, QuerySelect, Set, Statement, TransactionSession, TransactionTrait,
};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use crate::{
    domain::{random_token, teams},
    entities::{
        invitation::{self, InvitationStatus},
        team, team_member, user,
    },
    error::AppError,
};

/// How long an invitation stays valid.
pub const INVITATION_TTL: Duration = Duration::days(7);

/// Members shown in an invitation preview.
const PREVIEW_MEMBERS: u64 = 6;

/// Trims and lower-cases an email address and checks its shape.
pub fn normalize_email(raw: &str) -> Result<String, AppError> {
    let email = raw.trim().to_lowercase();
    let valid = email.len() <= 254
        && !email.chars().any(char::is_whitespace)
        && match email.split_once('@') {
            Some((local, domain)) => {
                !local.is_empty()
                    && !domain.is_empty()
                    && !domain.contains('@')
                    && domain.contains('.')
                    && !domain.starts_with('.')
                    && !domain.ends_with('.')
            }
            None => false,
        };
    if valid {
        Ok(email)
    } else {
        Err(AppError::Invalid("invalid_email"))
    }
}

/// Creates an invitation (any member). An open invitation for the same email
/// is returned instead of creating a second one.
pub async fn create(
    db: &impl ConnectionTrait,
    team_id: Uuid,
    actor: Uuid,
    email: &str,
) -> Result<invitation::Model, AppError> {
    teams::require_member(db, team_id, actor).await?;
    let email = normalize_email(email)?;

    if let Some(existing_user) = user::Entity::find()
        .filter(user::Column::Email.eq(email.clone()))
        .one(db)
        .await?
        && team_member::Entity::find_by_id((team_id, existing_user.id))
            .one(db)
            .await?
            .is_some()
    {
        return Err(AppError::Conflict("already_member"));
    }

    let now = OffsetDateTime::now_utc();
    if let Some(open) = invitation::Entity::find()
        .filter(invitation::Column::TeamId.eq(team_id))
        .filter(invitation::Column::Email.eq(email.clone()))
        .filter(invitation::Column::Status.eq(InvitationStatus::Pending))
        .filter(invitation::Column::ExpiresAt.gt(now))
        .one(db)
        .await?
    {
        return Ok(open);
    }

    Ok(invitation::ActiveModel {
        id: Set(Uuid::now_v7()),
        team_id: Set(team_id),
        email: Set(email),
        token: Set(random_token()),
        invited_by: Set(Some(actor)),
        status: Set(InvitationStatus::Pending),
        expires_at: Set(now + INVITATION_TTL),
        created_at: Set(now),
    }
    .insert(db)
    .await?)
}

/// Open invitations of a team with their inviter, newest first (any member).
pub async fn list_pending(
    db: &impl ConnectionTrait,
    team_id: Uuid,
    actor: Uuid,
) -> Result<Vec<(invitation::Model, Option<user::Model>)>, AppError> {
    teams::require_member(db, team_id, actor).await?;
    let rows = invitation::Entity::find()
        .filter(invitation::Column::TeamId.eq(team_id))
        .filter(invitation::Column::Status.eq(InvitationStatus::Pending))
        .filter(invitation::Column::ExpiresAt.gt(OffsetDateTime::now_utc()))
        .order_by_desc(invitation::Column::CreatedAt)
        .all(db)
        .await?;
    let inviters = teams::users_by_id(db, rows.iter().filter_map(|i| i.invited_by)).await?;
    Ok(rows
        .into_iter()
        .map(|i| {
            let inviter = i.invited_by.and_then(|id| inviters.get(&id).cloned());
            (i, inviter)
        })
        .collect())
}

/// Revokes an open invitation: its inviter or a team admin.
pub async fn revoke(
    db: &impl ConnectionTrait,
    team_id: Uuid,
    invitation_id: Uuid,
    actor: Uuid,
) -> Result<(), AppError> {
    let role = teams::require_member(db, team_id, actor).await?;
    let invitation = invitation::Entity::find_by_id(invitation_id)
        .filter(invitation::Column::TeamId.eq(team_id))
        .one(db)
        .await?
        .ok_or(AppError::NotFound("invitation_not_found"))?;
    if invitation.invited_by != Some(actor) && role != team_member::Role::Admin {
        return Err(AppError::Forbidden);
    }
    if invitation.status == InvitationStatus::Pending {
        let mut active: invitation::ActiveModel = invitation.into();
        active.status = Set(InvitationStatus::Revoked);
        active.update(db).await?;
    }
    Ok(())
}

/// What the invitee sees before accepting (`/invite/{token}`).
#[derive(Debug, Clone)]
pub struct Preview {
    pub invitation: invitation::Model,
    pub status: InvitationStatus,
    pub team: team::Model,
    pub inviter: Option<user::Model>,
    /// Empty unless the invitation is still open.
    pub members: Vec<user::Model>,
    pub member_count: i64,
    pub already_member: bool,
}

async fn find_by_token(
    db: &impl ConnectionTrait,
    token: &str,
) -> Result<invitation::Model, AppError> {
    invitation::Entity::find()
        .filter(invitation::Column::Token.eq(token))
        .one(db)
        .await?
        .ok_or(AppError::NotFound("invitation_not_found"))
}

/// Looks up an invitation by its token, for any signed-in holder.
pub async fn preview(
    db: &impl ConnectionTrait,
    token: &str,
    viewer: Uuid,
) -> Result<Preview, AppError> {
    let invitation = find_by_token(db, token).await?;
    let status = invitation.effective_status(OffsetDateTime::now_utc());
    let team = team::Entity::find_by_id(invitation.team_id)
        .one(db)
        .await?
        .ok_or(AppError::NotFound("invitation_not_found"))?;
    let inviter = match invitation.invited_by {
        Some(id) => user::Entity::find_by_id(id).one(db).await?,
        None => None,
    };
    let (members, member_count) = if status == InvitationStatus::Pending {
        teams::member_preview(db, team.id, PREVIEW_MEMBERS).await?
    } else {
        (Vec::new(), 0)
    };
    let already_member = team_member::Entity::find_by_id((team.id, viewer))
        .one(db)
        .await?
        .is_some();
    Ok(Preview {
        invitation,
        status,
        team,
        inviter,
        members,
        member_count,
        already_member,
    })
}

/// The error for an invitation that can no longer be used.
fn unusable(status: InvitationStatus) -> AppError {
    match status {
        InvitationStatus::Expired => AppError::Gone("invitation_expired"),
        InvitationStatus::Revoked => AppError::Gone("invitation_revoked"),
        _ => AppError::Gone("invitation_used"),
    }
}

/// Accepts an invitation: the caller becomes a member. Accepting while
/// already a member succeeds.
pub async fn accept(
    db: &(impl TransactionTrait + ConnectionTrait),
    token: &str,
    user_id: Uuid,
) -> Result<team::Model, AppError> {
    let txn = db.begin().await?;
    let invitation = invitation::Entity::find()
        .filter(invitation::Column::Token.eq(token))
        .lock_exclusive()
        .one(&txn)
        .await?
        .ok_or(AppError::NotFound("invitation_not_found"))?;
    let team = team::Entity::find_by_id(invitation.team_id)
        .one(&txn)
        .await?
        .ok_or(AppError::NotFound("invitation_not_found"))?;
    let status = invitation.effective_status(OffsetDateTime::now_utc());
    let is_member = team_member::Entity::find_by_id((team.id, user_id))
        .one(&txn)
        .await?
        .is_some();

    match status {
        InvitationStatus::Pending => {
            teams::add_member(&txn, team.id, user_id).await?;
            let mut active: invitation::ActiveModel = invitation.into();
            active.status = Set(InvitationStatus::Accepted);
            active.update(&txn).await?;
            txn.commit().await?;
            Ok(team)
        }
        _ if is_member => Ok(team),
        InvitationStatus::Expired => {
            mark_expired(&txn, invitation).await?;
            txn.commit().await?;
            Err(unusable(status))
        }
        other => Err(unusable(other)),
    }
}

/// Declines an open invitation.
pub async fn decline(
    db: &(impl TransactionTrait + ConnectionTrait),
    token: &str,
) -> Result<(), AppError> {
    let txn = db.begin().await?;
    let invitation = invitation::Entity::find()
        .filter(invitation::Column::Token.eq(token))
        .lock_exclusive()
        .one(&txn)
        .await?
        .ok_or(AppError::NotFound("invitation_not_found"))?;
    match invitation.effective_status(OffsetDateTime::now_utc()) {
        InvitationStatus::Pending => {
            let mut active: invitation::ActiveModel = invitation.into();
            active.status = Set(InvitationStatus::Declined);
            active.update(&txn).await?;
            txn.commit().await?;
            Ok(())
        }
        InvitationStatus::Declined => Ok(()),
        InvitationStatus::Expired => {
            mark_expired(&txn, invitation).await?;
            txn.commit().await?;
            Err(unusable(InvitationStatus::Expired))
        }
        other => Err(unusable(other)),
    }
}

async fn mark_expired(
    db: &impl ConnectionTrait,
    invitation: invitation::Model,
) -> Result<(), AppError> {
    if invitation.status == InvitationStatus::Pending {
        let mut active: invitation::ActiveModel = invitation.into();
        active.status = Set(InvitationStatus::Expired);
        active.update(db).await?;
    }
    Ok(())
}

/// An open invitation addressed to the signed-in user's email.
#[derive(Debug, Clone, FromQueryResult)]
pub struct PendingForUser {
    pub token: String,
    pub expires_at: OffsetDateTime,
    pub team_id: Uuid,
    pub team_name: String,
    pub inviter_id: Option<Uuid>,
    pub inviter_name: Option<String>,
}

/// Open invitations for `user`'s email, skipping teams they already joined.
pub async fn pending_for_user(
    db: &impl ConnectionTrait,
    user: &user::Model,
) -> Result<Vec<PendingForUser>, AppError> {
    Ok(
        PendingForUser::find_by_statement(Statement::from_sql_and_values(
            DbBackend::Postgres,
            r#"
SELECT i.token, i.expires_at, t.id AS team_id, t.name AS team_name,
       u.id AS inviter_id, u.display_name AS inviter_name
FROM invitations i
JOIN teams t ON t.id = i.team_id
LEFT JOIN users u ON u.id = i.invited_by
WHERE i.email = $1 AND i.status = 'pending' AND i.expires_at > now()
  AND NOT EXISTS (SELECT 1 FROM team_members m WHERE m.team_id = i.team_id AND m.user_id = $2)
ORDER BY i.created_at DESC
"#,
            [user.email.clone().into(), user.id.into()],
        ))
        .all(db)
        .await?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_emails() {
        assert_eq!(
            normalize_email("  Nina.Weber@Acme.dev ").unwrap(),
            "nina.weber@acme.dev"
        );
        for bad in [
            "",
            "nina",
            "@acme.dev",
            "nina@",
            "nina@acme",
            "a b@acme.dev",
            "a@b@c.dev",
            "a@.dev",
        ] {
            assert!(normalize_email(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn expiry_is_computed() {
        let now = OffsetDateTime::now_utc();
        let mut inv = invitation::Model {
            id: Uuid::now_v7(),
            team_id: Uuid::now_v7(),
            email: "a@b.dev".into(),
            token: "t".into(),
            invited_by: None,
            status: InvitationStatus::Pending,
            expires_at: now + Duration::days(1),
            created_at: now,
        };
        assert_eq!(inv.effective_status(now), InvitationStatus::Pending);
        inv.expires_at = now - Duration::seconds(1);
        assert_eq!(inv.effective_status(now), InvitationStatus::Expired);
        inv.status = InvitationStatus::Revoked;
        assert_eq!(inv.effective_status(now), InvitationStatus::Revoked);
    }
}
