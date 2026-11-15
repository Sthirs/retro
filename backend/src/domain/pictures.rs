//! Uploaded profile pictures: validation, storage and visibility.

use sea_orm::{
    ActiveModelTrait, ConnectionTrait, DbBackend, EntityTrait, Set, Statement, TransactionSession,
    TransactionTrait,
};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    entities::{user, user_picture},
    error::AppError,
};

/// Largest accepted picture, in bytes.
pub const PICTURE_MAX_BYTES: usize = 5 * 1024 * 1024;

/// Length of `users.picture_version`: a prefix of the picture's SHA-256 hex.
pub const VERSION_LEN: usize = 16;

/// The image type of `bytes` judged by its magic bytes, if supported.
pub fn sniff_image_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        Some("image/png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

/// The `picture_version` of a picture with this SHA-256 hex digest.
pub fn version_of(sha256: &str) -> &str {
    &sha256[..VERSION_LEN.min(sha256.len())]
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Stores `bytes` as the user's picture and points `users.picture_version`
/// at it, in one transaction. Fails with `picture_too_large` or
/// `unsupported_picture_type`.
pub async fn set_picture(
    db: &(impl TransactionTrait + ConnectionTrait),
    user: user::Model,
    bytes: Vec<u8>,
) -> Result<user::Model, AppError> {
    if bytes.len() > PICTURE_MAX_BYTES {
        return Err(AppError::PayloadTooLarge);
    }
    let content_type = sniff_image_type(&bytes).ok_or(AppError::UnsupportedMediaType)?;
    let sha256 = hex(&Sha256::digest(&bytes));
    let version = version_of(&sha256).to_owned();
    let size = i32::try_from(bytes.len()).map_err(|_| AppError::PayloadTooLarge)?;

    let txn = db.begin().await?;
    txn.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "INSERT INTO user_pictures (user_id, content_type, size_bytes, sha256, data, created_at) \
         VALUES ($1, $2, $3, $4, $5, now()) \
         ON CONFLICT (user_id) DO UPDATE SET content_type = EXCLUDED.content_type, \
           size_bytes = EXCLUDED.size_bytes, sha256 = EXCLUDED.sha256, \
           data = EXCLUDED.data, created_at = now()",
        [
            user.id.into(),
            content_type.into(),
            size.into(),
            sha256.into(),
            bytes.into(),
        ],
    ))
    .await?;
    let mut active: user::ActiveModel = user.into();
    active.picture_version = Set(Some(version));
    active.updated_at = Set(OffsetDateTime::now_utc());
    let user = active.update(&txn).await?;
    txn.commit().await?;
    Ok(user)
}

/// Deletes the user's picture and clears `users.picture_version`, in one
/// transaction. Removing a picture that does not exist is a no-op.
pub async fn remove_picture(
    db: &(impl TransactionTrait + ConnectionTrait),
    user: user::Model,
) -> Result<user::Model, AppError> {
    let txn = db.begin().await?;
    user_picture::Entity::delete_by_id(user.id)
        .exec(&txn)
        .await?;
    let mut active: user::ActiveModel = user.into();
    active.picture_version = Set(None);
    active.updated_at = Set(OffsetDateTime::now_utc());
    let user = active.update(&txn).await?;
    txn.commit().await?;
    Ok(user)
}

/// `owner`'s picture, if `viewer` is the owner or shares a team with them.
/// Otherwise, and when there is no picture, `picture_not_found`.
pub async fn picture_for(
    db: &impl ConnectionTrait,
    viewer: Uuid,
    owner: Uuid,
) -> Result<user_picture::Model, AppError> {
    let not_found = AppError::NotFound("picture_not_found");
    if viewer != owner {
        let row = db
            .query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "SELECT EXISTS(SELECT 1 FROM team_members a JOIN team_members b \
                   ON a.team_id = b.team_id WHERE a.user_id = $1 AND b.user_id = $2) AS shared",
                [viewer.into(), owner.into()],
            ))
            .await?;
        let shared = match row {
            Some(row) => row.try_get::<bool>("", "shared")?,
            None => false,
        };
        if !shared {
            return Err(not_found);
        }
    }
    user_picture::Entity::find_by_id(owner)
        .one(db)
        .await?
        .ok_or(not_found)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_png() {
        let bytes = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0];
        assert_eq!(sniff_image_type(&bytes), Some("image/png"));
    }

    #[test]
    fn recognises_jpeg() {
        assert_eq!(
            sniff_image_type(&[0xFF, 0xD8, 0xFF, 0xE0, 0x00]),
            Some("image/jpeg")
        );
    }

    #[test]
    fn recognises_gif() {
        assert_eq!(sniff_image_type(b"GIF87a\x01\x00"), Some("image/gif"));
        assert_eq!(sniff_image_type(b"GIF89a\x01\x00"), Some("image/gif"));
    }

    #[test]
    fn recognises_webp() {
        assert_eq!(
            sniff_image_type(b"RIFF\x24\x00\x00\x00WEBPVP8 "),
            Some("image/webp")
        );
        assert_eq!(sniff_image_type(b"RIFF\x24\x00\x00\x00WAVEfmt "), None);
    }

    #[test]
    fn rejects_other_content() {
        assert_eq!(sniff_image_type(b"<svg"), None);
        assert_eq!(sniff_image_type(b""), None);
        assert_eq!(sniff_image_type(b"not an image"), None);
    }
}
