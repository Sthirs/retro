//! API errors rendered as RFC 9457 problem details (spec §14).

use axum::{
    Json,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Serialize;
use utoipa::ToSchema;

/// RFC 9457 problem details body.
#[derive(Debug, Serialize, ToSchema)]
pub struct Problem {
    /// URI reference identifying the problem type.
    #[serde(rename = "type")]
    pub kind: String,
    pub title: String,
    pub status: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Stable machine-readable error code, e.g. `invalid_credentials`.
    pub code: String,
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("invalid credentials")]
    InvalidCredentials,
    #[error("authentication required")]
    Unauthorized,
    #[error("too many attempts")]
    TooManyRequests,
    #[error("{0} is disabled")]
    Disabled(&'static str),
    #[error("bad request: {0}")]
    BadRequest(String),
    /// A rejected input field, with a stable code such as `invalid_team_name`.
    #[error("invalid input: {0}")]
    Invalid(&'static str),
    /// The resource does not exist or the caller may not see it.
    #[error("not found: {0}")]
    NotFound(&'static str),
    #[error("forbidden")]
    Forbidden,
    /// The request conflicts with the current state, e.g. `last_admin`.
    #[error("conflict: {0}")]
    Conflict(&'static str),
    /// The resource existed but can no longer be used, e.g. `invitation_expired`.
    #[error("gone: {0}")]
    Gone(&'static str),
    #[error("service unavailable: {0}")]
    Unavailable(String),
    #[error(transparent)]
    Database(#[from] sea_orm::DbErr),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl AppError {
    fn parts(&self) -> (StatusCode, &'static str, Option<String>) {
        match self {
            Self::InvalidCredentials => (
                StatusCode::UNAUTHORIZED,
                "invalid_credentials",
                Some("Incorrect username or password.".into()),
            ),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized", None),
            Self::TooManyRequests => (
                StatusCode::TOO_MANY_REQUESTS,
                "too_many_requests",
                Some("Too many attempts. Try again in a few minutes.".into()),
            ),
            Self::Disabled(what) => (
                StatusCode::NOT_FOUND,
                "disabled",
                Some(format!("{what} is disabled.")),
            ),
            Self::BadRequest(detail) => {
                (StatusCode::BAD_REQUEST, "bad_request", Some(detail.clone()))
            }
            Self::Invalid(code) => (StatusCode::BAD_REQUEST, code, None),
            Self::NotFound(code) => (StatusCode::NOT_FOUND, code, None),
            Self::Forbidden => (StatusCode::FORBIDDEN, "forbidden", None),
            Self::Conflict(code) => (StatusCode::CONFLICT, code, None),
            Self::Gone(code) => (StatusCode::GONE, code, None),
            Self::Unavailable(_) => (StatusCode::SERVICE_UNAVAILABLE, "unavailable", None),
            Self::Database(_) | Self::Internal(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "internal", None)
            }
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, detail) = self.parts();
        if status.is_server_error() {
            tracing::error!(error = %self, "request failed");
        }
        let body = Problem {
            kind: "about:blank".into(),
            title: status.canonical_reason().unwrap_or("Error").into(),
            status: status.as_u16(),
            detail,
            code: code.into(),
        };
        let mut response = (status, Json(body)).into_response();
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/problem+json"),
        );
        response
    }
}

pub type AppResult<T> = Result<T, AppError>;
