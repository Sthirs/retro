//! Image attachments of reflections and tasks (spec §13, §14.4, ADR-0013).

use axum::{
    Json,
    body::{Body, Bytes},
    extract::{Path, State, rejection::BytesRejection},
    http::{HeaderValue, StatusCode, header},
    response::Response,
};
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    api::check_limit,
    auth::CurrentUser,
    domain::attachments,
    entities::attachment,
    error::{AppError, AppResult, Problem},
    state::AppState,
};

/// An uploaded image, converted to WebP. Rich text references it as
/// `<img data-attachment-id="{id}">`.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentDto {
    pub id: Uuid,
    /// Always `image/webp`.
    pub content_type: String,
    pub width: i32,
    pub height: i32,
    pub animated: bool,
}

impl From<attachment::Model> for AttachmentDto {
    fn from(a: attachment::Model) -> Self {
        Self {
            id: a.id,
            content_type: a.content_type,
            width: a.width,
            height: a.height,
            animated: a.animated,
        }
    }
}

/// Uploads an image for a reflection or task of the team. The image is
/// decoded and re-encoded as WebP: only its pixels (and frames) are kept.
#[utoipa::path(
    post,
    path = "/api/v1/teams/{team_id}/attachments",
    tag = "attachments",
    security(("bearer" = [])),
    params(("team_id" = Uuid, Path, description = "The team")),
    request_body(content = Vec<u8>, content_type = "image/*", description = "A PNG, JPEG, GIF or WebP image of at most 5 MB"),
    responses(
        (status = 201, body = AttachmentDto, description = "The stored image"),
        (status = 400, body = Problem, content_type = "application/problem+json", description = "`invalid_image` or `image_dimensions_too_large`"),
        (status = 401, body = Problem, content_type = "application/problem+json", description = "Missing or invalid access token"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`team_not_found`"),
        (status = 413, body = Problem, content_type = "application/problem+json", description = "`image_too_large`"),
        (status = 415, body = Problem, content_type = "application/problem+json", description = "`unsupported_image_type`"),
        (status = 429, body = Problem, content_type = "application/problem+json", description = "`too_many_requests` or `too_many_pending_attachments`"),
    )
)]
pub async fn upload_attachment(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(team_id): Path<Uuid>,
    body: Result<Bytes, BytesRejection>,
) -> AppResult<(StatusCode, Json<AttachmentDto>)> {
    // axum's body limit answers oversize uploads with a plain-text 413 unless
    // the rejection is mapped to problem JSON.
    let body = body.map_err(|rejection| {
        if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE {
            AppError::ImageTooLarge
        } else {
            AppError::BadRequest(rejection.body_text())
        }
    })?;
    let key = user.id.to_string();
    check_limit(&state.upload_limiter, &key)?;
    // Every attempt counts, failed ones too: conversion costs CPU either way.
    state.upload_limiter.record_failure(&key);
    let attachment = attachments::upload(&state.db, team_id, user.id, body).await?;
    Ok((StatusCode::CREATED, Json(attachment.into())))
}

/// An image, as WebP, for team members who uploaded it or can see an item
/// referencing it. Attachments never change, so it is cached as immutable.
#[utoipa::path(
    get,
    path = "/api/v1/attachments/{attachment_id}",
    tag = "attachments",
    security(("bearer" = [])),
    params(("attachment_id" = Uuid, Path, description = "The attachment")),
    responses(
        (status = 200, body = Vec<u8>, content_type = "image/webp", description = "The image bytes"),
        (status = 401, body = Problem, content_type = "application/problem+json", description = "Missing or invalid access token"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`attachment_not_found`"),
    )
)]
pub async fn get_attachment(
    State(state): State<AppState>,
    CurrentUser(viewer): CurrentUser,
    Path(attachment_id): Path<Uuid>,
) -> AppResult<Response> {
    let attachment = attachments::attachment_for(&state.db, viewer.id, attachment_id).await?;
    let etag = HeaderValue::from_str(&format!("\"{}\"", attachment.sha256))
        .map_err(|e| anyhow::anyhow!("stored attachment hash: {e}"))?;
    let mut response = Response::new(Body::from(attachment.data));
    let headers = response.headers_mut();
    // Never the stored type: only WebP is ever written, and only WebP is served.
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/webp"));
    headers.insert(header::ETAG, etag);
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=31536000, immutable"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    // Opened directly, the response can run nothing and load nothing.
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'none'; sandbox"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("inline; filename=\"image.webp\""),
    );
    headers.insert(
        "cross-origin-resource-policy",
        HeaderValue::from_static("same-origin"),
    );
    Ok(response)
}
