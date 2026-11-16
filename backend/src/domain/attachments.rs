//! Image attachments of reflections and tasks (spec §13, ADR-0013): upload,
//! visibility, references from rich text, and cleanup.

use std::collections::{HashMap, HashSet};

use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbBackend, EntityTrait, QueryFilter,
    QuerySelect, Set, Statement,
};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::{
    domain::{rich_text, teams},
    entities::attachment,
    error::AppError,
    services::images::{self, ImageError},
};

/// Attachments a user may hold that nothing references yet (spec §13).
pub const MAX_PENDING_PER_USER: u64 = 50;
/// Unreferenced attachments older than this are deleted by [`cleanup`].
pub const ORPHAN_TTL: time::Duration = time::Duration::hours(24);
/// Conversions running at once on this replica; the rest wait. Conversion is
/// CPU-bound (up to a few seconds), so this keeps uploads from starving the
/// runtime's blocking pool and the machine's cores.
static CONVERSIONS: Semaphore = Semaphore::const_new(2);

/// What holds a reference to attachments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owner {
    Task(Uuid),
    Reflection(Uuid),
}

impl Owner {
    /// The join table and its owner column.
    fn table(self) -> (&'static str, &'static str, Uuid) {
        match self {
            Self::Task(id) => ("task_attachments", "task_id", id),
            Self::Reflection(id) => ("reflection_attachments", "reflection_id", id),
        }
    }
}

impl From<ImageError> for AppError {
    fn from(e: ImageError) -> Self {
        match e {
            ImageError::Unsupported => AppError::UnsupportedImageType,
            ImageError::Invalid => AppError::Invalid("invalid_image"),
            ImageError::DimensionsTooLarge => AppError::Invalid("image_dimensions_too_large"),
            ImageError::TooLarge => AppError::ImageTooLarge,
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Converts `bytes` to WebP and stores it as an attachment of `team_id`
/// uploaded by `uploader`. Non-members get `team_not_found`; a user holding
/// [`MAX_PENDING_PER_USER`] unreferenced attachments gets
/// `too_many_pending_attachments`.
pub async fn upload(
    db: &impl ConnectionTrait,
    team_id: Uuid,
    uploader: Uuid,
    bytes: Vec<u8>,
) -> Result<attachment::Model, AppError> {
    if bytes.len() > images::MAX_UPLOAD_BYTES {
        return Err(AppError::ImageTooLarge);
    }
    teams::require_member(db, team_id, uploader).await?;
    if pending_count(db, uploader).await? >= MAX_PENDING_PER_USER {
        return Err(AppError::Limited("too_many_pending_attachments"));
    }

    let webp = {
        let _permit = CONVERSIONS
            .acquire()
            .await
            .map_err(|e| anyhow::anyhow!("conversion semaphore: {e}"))?;
        tokio::task::spawn_blocking(move || images::to_webp(&bytes))
            .await
            .map_err(|e| anyhow::anyhow!("image conversion task: {e}"))??
    };

    let size = i32::try_from(webp.data.len()).map_err(|_| AppError::ImageTooLarge)?;
    let to_i32 = |v: u32| i32::try_from(v).map_err(|_| AppError::Invalid("invalid_image"));
    let model = attachment::ActiveModel {
        id: Set(Uuid::now_v7()),
        team_id: Set(team_id),
        uploader_id: Set(uploader),
        content_type: Set("image/webp".into()),
        animated: Set(webp.animated),
        width: Set(to_i32(webp.width)?),
        height: Set(to_i32(webp.height)?),
        size_bytes: Set(size),
        sha256: Set(hex(&Sha256::digest(&webp.data))),
        data: Set(webp.data),
        created_at: Set(OffsetDateTime::now_utc()),
    }
    .insert(db)
    .await?;
    Ok(model)
}

/// Attachments `user` uploaded that no reflection or task references.
async fn pending_count(db: &impl ConnectionTrait, user: Uuid) -> Result<u64, AppError> {
    let row = db
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT count(*) AS n FROM attachments a WHERE a.uploader_id = $1 \
               AND NOT EXISTS (SELECT 1 FROM task_attachments t WHERE t.attachment_id = a.id) \
               AND NOT EXISTS (SELECT 1 FROM reflection_attachments r WHERE r.attachment_id = a.id)",
            [user.into()],
        ))
        .await?;
    let n = match row {
        Some(row) => row.try_get::<i64>("", "n")?,
        None => 0,
    };
    Ok(u64::try_from(n).unwrap_or(0))
}

/// The attachment, if `viewer` may see it: a member of its team who uploaded
/// it, or who can see an item referencing it (a task, or a reflection whose
/// meeting has left the Reflect phase). Otherwise `attachment_not_found`.
pub async fn attachment_for(
    db: &impl ConnectionTrait,
    viewer: Uuid,
    id: Uuid,
) -> Result<attachment::Model, AppError> {
    attachment::Entity::find()
        .from_raw_sql(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT a.* FROM attachments a \
             JOIN team_members tm ON tm.team_id = a.team_id AND tm.user_id = $2 \
             WHERE a.id = $1 AND ( \
               a.uploader_id = $2 \
               OR EXISTS (SELECT 1 FROM task_attachments ta JOIN tasks t ON t.id = ta.task_id \
                          WHERE ta.attachment_id = a.id AND t.team_id = a.team_id) \
               OR EXISTS (SELECT 1 FROM reflection_attachments ra \
                          JOIN reflections r ON r.id = ra.reflection_id \
                          JOIN meetings m ON m.id = r.meeting_id \
                          WHERE ra.attachment_id = a.id AND m.team_id = a.team_id \
                            AND (m.status = 'ended' OR m.phase > 'reflect')))",
            [id.into(), viewer.into()],
        ))
        .one(db)
        .await?
        .ok_or(AppError::NotFound("attachment_not_found"))
}

/// Checks the images of sanitized content saved by `actor` into an item of
/// `team_id` (`owner`, unless it is being created), and returns its HTML with
/// the images' dimensions.
///
/// Each image must be an attachment of the team that the item already
/// references or that `actor` uploaded: nobody can reveal someone else's
/// image (an unrevealed reflection's, say) by copying its id. Otherwise
/// `attachment_not_found`.
pub async fn resolve(
    db: &impl ConnectionTrait,
    team_id: Uuid,
    actor: Uuid,
    owner: Option<Owner>,
    content: &rich_text::Sanitized,
) -> Result<String, AppError> {
    if content.attachment_ids.is_empty() {
        return Ok(content.html.clone());
    }
    let found: Vec<(Uuid, Uuid, i32, i32)> = attachment::Entity::find()
        .select_only()
        .columns([
            attachment::Column::Id,
            attachment::Column::UploaderId,
            attachment::Column::Width,
            attachment::Column::Height,
        ])
        .filter(attachment::Column::Id.is_in(content.attachment_ids.clone()))
        .filter(attachment::Column::TeamId.eq(team_id))
        // Held until the item is saved: the cleanup cannot delete them meanwhile.
        .lock_shared()
        .into_tuple()
        .all(db)
        .await?;
    let kept: HashSet<Uuid> = match owner {
        Some(owner) => references(db, owner).await?.into_iter().collect(),
        None => HashSet::new(),
    };
    let mut sizes = HashMap::new();
    for (id, uploader, width, height) in found {
        if uploader == actor || kept.contains(&id) {
            sizes.insert(id, (width, height));
        }
    }
    if content
        .attachment_ids
        .iter()
        .any(|id| !sizes.contains_key(id))
    {
        return Err(AppError::Invalid("attachment_not_found"));
    }
    Ok(rich_text::with_image_sizes(&content.html, &sizes))
}

/// The attachments `owner` references.
async fn references(db: &impl ConnectionTrait, owner: Owner) -> Result<Vec<Uuid>, AppError> {
    let (table, column, id) = owner.table();
    let rows = db
        .query_all_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            format!("SELECT attachment_id FROM {table} WHERE {column} = $1"),
            [id.into()],
        ))
        .await?;
    rows.iter()
        .map(|row| row.try_get::<Uuid>("", "attachment_id").map_err(Into::into))
        .collect()
}

/// Replaces the attachments `owner` references by `ids` (already checked by
/// [`resolve`]).
pub async fn set_references(
    db: &impl ConnectionTrait,
    owner: Owner,
    ids: &[Uuid],
) -> Result<(), AppError> {
    let (table, column, owner_id) = owner.table();
    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        format!("DELETE FROM {table} WHERE {column} = $1"),
        [owner_id.into()],
    ))
    .await?;
    for id in ids {
        db.execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            format!("INSERT INTO {table} ({column}, attachment_id) VALUES ($1, $2)"),
            [owner_id.into(), (*id).into()],
        ))
        .await?;
    }
    Ok(())
}

/// Deletes attachments that nothing references and that are older than
/// [`ORPHAN_TTL`]. Idempotent, so every replica may run it. Returns how many
/// were deleted.
pub async fn cleanup(db: &impl ConnectionTrait) -> Result<u64, AppError> {
    let cutoff = OffsetDateTime::now_utc() - ORPHAN_TTL;
    let result = db
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "DELETE FROM attachments a WHERE a.created_at < $1 \
               AND NOT EXISTS (SELECT 1 FROM task_attachments t WHERE t.attachment_id = a.id) \
               AND NOT EXISTS (SELECT 1 FROM reflection_attachments r WHERE r.attachment_id = a.id)",
            [cutoff.into()],
        ))
        .await?;
    Ok(result.rows_affected())
}
