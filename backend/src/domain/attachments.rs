//! Image attachments of reflections and tasks (spec §13, ADR-0013): upload,
//! visibility, references from rich text, and cleanup.

use std::collections::{HashMap, HashSet};

use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbBackend, EntityTrait, QueryFilter,
    QuerySelect, Set, Statement, TransactionSession, TransactionTrait, Value,
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
/// Attachments [`cleanup`] locks and deletes per transaction.
const CLEANUP_BATCH: usize = 500;
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

/// Runs the CPU-bound `job` on the blocking pool, at most [`CONVERSIONS`] at
/// once. The permit moves into the job: it is released when the job ends,
/// not when the caller stops waiting (a client that disconnects drops the
/// caller, but the job keeps running).
async fn limited<T: Send + 'static>(
    job: impl FnOnce() -> T + Send + 'static,
) -> Result<T, AppError> {
    let permit = CONVERSIONS
        .acquire()
        .await
        .map_err(|e| anyhow::anyhow!("conversion semaphore: {e}"))?;
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        job()
    })
    .await
    .map_err(|e| anyhow::anyhow!("image conversion task: {e}"))?;
    Ok(result)
}

/// Converts `bytes` to WebP and stores it as an attachment of `team_id`
/// uploaded by `uploader`. Non-members get `team_not_found`; a user holding
/// [`MAX_PENDING_PER_USER`] unreferenced attachments gets
/// `too_many_pending_attachments`.
pub async fn upload(
    db: &(impl TransactionTrait + ConnectionTrait),
    team_id: Uuid,
    uploader: Uuid,
    bytes: impl AsRef<[u8]> + Send + 'static,
) -> Result<attachment::Model, AppError> {
    if bytes.as_ref().len() > images::MAX_UPLOAD_BYTES {
        return Err(AppError::ImageTooLarge);
    }
    teams::require_member(db, team_id, uploader).await?;
    // Checked again under a lock before the insert; this early check only
    // saves the conversion when the user is already at the limit.
    if pending_count(db, uploader).await? >= MAX_PENDING_PER_USER {
        return Err(AppError::Limited("too_many_pending_attachments"));
    }

    let webp = limited(move || images::to_webp(bytes.as_ref())).await??;

    let size = i32::try_from(webp.data.len()).map_err(|_| AppError::ImageTooLarge)?;
    let to_i32 = |v: u32| i32::try_from(v).map_err(|_| AppError::Invalid("invalid_image"));
    let txn = db.begin().await?;
    // Serialises this user's uploads, so parallel ones cannot all pass the
    // pending check.
    txn.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
        [format!("attachments:{uploader}").into()],
    ))
    .await?;
    if pending_count(&txn, uploader).await? >= MAX_PENDING_PER_USER {
        return Err(AppError::Limited("too_many_pending_attachments"));
    }
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
    .insert(&txn)
    .await?;
    txn.commit().await?;
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
/// meeting has left the Reflect phase, like `reflections::snapshot`: a
/// meeting ended during Reflect keeps them hidden). Otherwise
/// `attachment_not_found`.
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
                            AND m.phase > 'reflect'))",
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
        // Held until the item is saved: [`cleanup`] skips locked rows, and
        // once it has locked one, this waits and then no longer finds it.
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

const UNREFERENCED: &str = "NOT EXISTS (SELECT 1 FROM task_attachments t WHERE t.attachment_id = a.id) \
     AND NOT EXISTS (SELECT 1 FROM reflection_attachments r WHERE r.attachment_id = a.id)";

/// Deletes attachments that nothing references and that are older than
/// [`ORPHAN_TTL`]. Idempotent, so every replica may run it. Returns how many
/// were deleted.
///
/// Each batch first locks its rows (skipping those a save holds), then
/// checks the references again in a new statement before deleting: once
/// locked, no reference can be added (its foreign key check waits on the
/// lock), and the new statement sees every reference committed before.
pub async fn cleanup(db: &(impl TransactionTrait + ConnectionTrait)) -> Result<u64, AppError> {
    let cutoff = OffsetDateTime::now_utc() - ORPHAN_TTL;
    let mut deleted = 0;
    loop {
        let txn = db.begin().await?;
        let rows = txn
            .query_all_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                format!(
                    "SELECT a.id FROM attachments a WHERE a.created_at < $1 AND {UNREFERENCED} \
                     LIMIT {CLEANUP_BATCH} FOR UPDATE SKIP LOCKED"
                ),
                [cutoff.into()],
            ))
            .await?;
        let ids = rows
            .iter()
            .map(|row| row.try_get::<Uuid>("", "id"))
            .collect::<Result<Vec<_>, _>>()?;
        if ids.is_empty() {
            txn.commit().await?;
            return Ok(deleted);
        }
        let placeholders = (1..=ids.len())
            .map(|i| format!("${i}"))
            .collect::<Vec<_>>()
            .join(", ");
        let result = txn
            .execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                format!(
                    "DELETE FROM attachments a WHERE a.id IN ({placeholders}) AND {UNREFERENCED}"
                ),
                ids.iter().map(|&id| Value::from(id)).collect::<Vec<_>>(),
            ))
            .await?;
        txn.commit().await?;
        deleted += result.rows_affected();
        if ids.len() < CLEANUP_BATCH {
            return Ok(deleted);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    async fn until(condition: impl Fn() -> bool) {
        tokio::time::timeout(Duration::from_secs(5), async {
            while !condition() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("condition not reached");
    }

    #[tokio::test]
    async fn a_conversion_holds_its_permit_after_the_caller_is_dropped() {
        let total = CONVERSIONS.available_permits();
        let (release, wait) = std::sync::mpsc::channel::<()>();
        let caller = tokio::spawn(limited(move || wait.recv()));
        until(|| CONVERSIONS.available_permits() == total - 1).await;

        // The client disconnects: the request future is dropped.
        caller.abort();
        assert!(caller.await.unwrap_err().is_cancelled());
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(CONVERSIONS.available_permits(), total - 1);

        // The conversion ends: only now is the permit released.
        release.send(()).unwrap();
        until(|| CONVERSIONS.available_permits() == total).await;
    }
}
