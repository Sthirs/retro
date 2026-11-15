//! The current user, their teams and pending invitations (spec §14.1).

use axum::{
    Json,
    body::{Body, Bytes},
    extract::{Path, Query, State, rejection::BytesRejection},
    http::{HeaderValue, StatusCode, header},
    response::Response,
};
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    api::{UserDto, invitations::PendingInvitationDto, teams::TeamDto},
    auth::CurrentUser,
    domain::{invitations, pictures, teams, users},
    entities::user_identity,
    error::{AppError, AppResult, Problem},
    state::AppState,
};

/// How the user signs in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum SignInMethod {
    /// A username and password.
    Password,
    /// A single sign-on identity from the OIDC provider.
    Sso,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MeResponse {
    pub user: UserDto,
    /// Teams the user belongs to, by name.
    pub teams: Vec<TeamDto>,
    /// Open invitations sent to the user's email, newest first.
    pub invitations: Vec<PendingInvitationDto>,
    /// When the account was created.
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub member_since: OffsetDateTime,
    /// `sso` when the user has a linked identity, else `password`.
    pub sign_in_method: SignInMethod,
}

/// Returns the signed-in user with their teams and pending invitations.
#[utoipa::path(
    get,
    path = "/api/v1/me",
    tag = "users",
    security(("bearer" = [])),
    responses(
        (status = 200, body = MeResponse, description = "The signed-in user"),
        (status = 401, body = Problem, content_type = "application/problem+json", description = "Missing or invalid access token"),
    )
)]
pub async fn me(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> AppResult<Json<MeResponse>> {
    // Independent reads: run them concurrently on the pool.
    let (teams, invitations, linked) = tokio::try_join!(
        teams::list_for_user(&state.db, user.id),
        invitations::pending_for_user(&state.db, &user),
        async {
            user_identity::Entity::find()
                .filter(user_identity::Column::UserId.eq(user.id))
                .count(&state.db)
                .await
                .map_err(AppError::from)
        },
    )?;
    let sign_in_method = if linked > 0 {
        SignInMethod::Sso
    } else {
        SignInMethod::Password
    };
    let member_since = user.created_at;
    Ok(Json(MeResponse {
        member_since,
        sign_in_method,
        user: user.into(),
        teams: teams.into_iter().map(Into::into).collect(),
        invitations: invitations.into_iter().map(Into::into).collect(),
    }))
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMeRequest {
    /// New display name; trimmed, 1 to 80 characters.
    pub display_name: String,
}

/// Changes the signed-in user's display name.
#[utoipa::path(
    patch,
    path = "/api/v1/me",
    tag = "users",
    security(("bearer" = [])),
    request_body = UpdateMeRequest,
    responses(
        (status = 200, body = UserDto, description = "The updated user"),
        (status = 400, body = Problem, content_type = "application/problem+json", description = "`invalid_display_name`"),
        (status = 401, body = Problem, content_type = "application/problem+json", description = "Missing or invalid access token"),
    )
)]
pub async fn update_me(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(body): Json<UpdateMeRequest>,
) -> AppResult<Json<UserDto>> {
    let user = users::update_display_name(&state.db, user, &body.display_name).await?;
    Ok(Json(user.into()))
}

/// Replaces the signed-in user's picture with the request body.
#[utoipa::path(
    put,
    path = "/api/v1/me/picture",
    tag = "users",
    security(("bearer" = [])),
    request_body(content = Vec<u8>, content_type = "image/*", description = "A PNG, JPEG, GIF or WebP image of at most 5 MB"),
    responses(
        (status = 200, body = UserDto, description = "The updated user"),
        (status = 401, body = Problem, content_type = "application/problem+json", description = "Missing or invalid access token"),
        (status = 413, body = Problem, content_type = "application/problem+json", description = "`picture_too_large`"),
        (status = 415, body = Problem, content_type = "application/problem+json", description = "`unsupported_picture_type`"),
    )
)]
pub async fn put_picture(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    body: Result<Bytes, BytesRejection>,
) -> AppResult<Json<UserDto>> {
    // axum's body limit answers oversize uploads with a plain-text 413 unless
    // the rejection is mapped to the same problem JSON as the handler's check.
    let body = body.map_err(|rejection| {
        if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE {
            AppError::PayloadTooLarge
        } else {
            AppError::BadRequest(rejection.body_text())
        }
    })?;
    let user = pictures::set_picture(&state.db, user, body.to_vec()).await?;
    Ok(Json(user.into()))
}

/// Removes the signed-in user's picture.
#[utoipa::path(
    delete,
    path = "/api/v1/me/picture",
    tag = "users",
    security(("bearer" = [])),
    responses(
        (status = 200, body = UserDto, description = "The updated user"),
        (status = 401, body = Problem, content_type = "application/problem+json", description = "Missing or invalid access token"),
    )
)]
pub async fn delete_picture(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> AppResult<Json<UserDto>> {
    let user = pictures::remove_picture(&state.db, user).await?;
    Ok(Json(user.into()))
}

/// Query of [`get_picture`].
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PictureQuery {
    /// The `pictureVersion` the caller knows.
    pub v: Option<String>,
}

/// A user's picture. Visible to the user and to people sharing a team with them.
///
/// The bytes are cached as immutable under their version (spec §20), so they
/// are only sent when `v` is the current `pictureVersion`. Any other `v`, or
/// none, is redirected to the current version's URL.
#[utoipa::path(
    get,
    path = "/api/v1/users/{user_id}/picture",
    tag = "users",
    security(("bearer" = [])),
    params(("user_id" = Uuid, Path, description = "The picture's owner"), PictureQuery),
    responses(
        (status = 200, body = Vec<u8>, content_type = "image/*", description = "The image bytes, when `v` is the current version"),
        (status = 302, description = "`v` is missing or not the current version: `Location` is the current version's URL"),
        (status = 401, body = Problem, content_type = "application/problem+json", description = "Missing or invalid access token"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`picture_not_found`"),
    )
)]
pub async fn get_picture(
    State(state): State<AppState>,
    CurrentUser(viewer): CurrentUser,
    Path(user_id): Path<Uuid>,
    Query(query): Query<PictureQuery>,
) -> AppResult<Response> {
    let picture = pictures::picture_for(&state.db, viewer.id, user_id).await?;
    let version = pictures::version_of(&picture.sha256);
    if query.v.as_deref() != Some(version) {
        // No `Cache-Control` here: the router answers it with `no-store`.
        let location =
            HeaderValue::from_str(&format!("/api/v1/users/{user_id}/picture?v={version}"))
                .map_err(|e| anyhow::anyhow!("picture location: {e}"))?;
        let mut response = Response::new(Body::empty());
        *response.status_mut() = StatusCode::FOUND;
        response.headers_mut().insert(header::LOCATION, location);
        return Ok(response);
    }
    let content_type = HeaderValue::from_str(&picture.content_type)
        .map_err(|e| anyhow::anyhow!("stored picture content type: {e}"))?;
    let etag = HeaderValue::from_str(&format!("\"{}\"", picture.sha256))
        .map_err(|e| anyhow::anyhow!("stored picture hash: {e}"))?;
    let mut response = Response::new(Body::from(picture.data));
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, content_type);
    headers.insert(header::ETAG, etag);
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=31536000, immutable"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    Ok(response)
}
