//! The current user, their teams and pending invitations (spec §14.1).

use axum::{
    Json,
    body::{Body, Bytes},
    extract::{Path, State},
    http::{HeaderValue, header},
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
    error::{AppResult, Problem},
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
    let teams = teams::list_for_user(&state.db, user.id).await?;
    let invitations = invitations::pending_for_user(&state.db, &user).await?;
    let linked = user_identity::Entity::find()
        .filter(user_identity::Column::UserId.eq(user.id))
        .count(&state.db)
        .await?;
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
    body: Bytes,
) -> AppResult<Json<UserDto>> {
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

/// A user's picture. Visible to the user and to people sharing a team with them.
///
/// The URL changes with `pictureVersion`, so the bytes are cached as immutable.
#[utoipa::path(
    get,
    path = "/api/v1/users/{user_id}/picture",
    tag = "users",
    security(("bearer" = [])),
    params(("user_id" = Uuid, Path, description = "The picture's owner")),
    responses(
        (status = 200, body = Vec<u8>, content_type = "image/*", description = "The image bytes"),
        (status = 401, body = Problem, content_type = "application/problem+json", description = "Missing or invalid access token"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "`picture_not_found`"),
    )
)]
pub async fn get_picture(
    State(state): State<AppState>,
    CurrentUser(viewer): CurrentUser,
    Path(user_id): Path<Uuid>,
) -> AppResult<Response> {
    let picture = pictures::picture_for(&state.db, viewer.id, user_id).await?;
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
    Ok(response)
}
