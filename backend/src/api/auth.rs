//! Sign-in, SSO, session refresh and sign-out (spec §3, §14.1).

use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::{CookieJar, PrivateCookieJar};
use sea_orm::EntityTrait;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::{
    api::UserDto,
    auth::{
        cookies, jwt,
        oidc::{LoginState, sanitize_redirect},
        password, refresh,
    },
    config::Config,
    domain::users,
    entities::user,
    error::{AppError, AppResult, Problem},
    state::AppState,
};

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SsoConfig {
    pub enabled: bool,
    pub button_text: String,
}

/// Which sign-in methods the sign-in page offers.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AuthConfig {
    pub local_login: bool,
    pub sso: SsoConfig,
    /// Shown as "Forgot password?" when set. Retro sends no email itself.
    pub password_reset_url: Option<String>,
}

/// Returns the sign-in options.
#[utoipa::path(get, path = "/api/v1/auth/config", tag = "auth", responses((status = 200, body = AuthConfig, description = "Sign-in options")))]
pub async fn config(State(state): State<AppState>) -> Json<AuthConfig> {
    Json(AuthConfig {
        local_login: state.config.local_login_enabled,
        sso: SsoConfig {
            enabled: state.oidc.is_some(),
            button_text: state
                .oidc
                .as_ref()
                .map(|o| o.button_text().to_owned())
                .unwrap_or_default(),
        },
        password_reset_url: state
            .config
            .password_reset_url
            .as_ref()
            .map(|u| u.to_string()),
    })
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoginRequest {
    /// Username or email address.
    pub login: String,
    pub password: String,
    /// "Keep me signed in".
    #[serde(default)]
    pub remember: bool,
}

/// A new session: a short-lived access token (the refresh token is in a cookie).
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionResponse {
    pub access_token: String,
    /// Access token lifetime in seconds.
    pub expires_in: i64,
    pub user: UserDto,
}

fn session(config: &Config, user: user::Model) -> AppResult<SessionResponse> {
    Ok(SessionResponse {
        access_token: jwt::issue(&config.jwt_secret, user.id, config.access_token_ttl_seconds)?,
        expires_in: config.access_token_ttl_seconds,
        user: user.into(),
    })
}

/// Client address as seen by the ingress controller.
fn client_ip(headers: &HeaderMap) -> String {
    headers
        .get("x-real-ip")
        .or_else(|| headers.get("x-forwarded-for"))
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(|v| v.trim().to_owned())
        .unwrap_or_else(|| "unknown".into())
}

/// Signs in with a username or email and a password.
#[utoipa::path(
    post,
    path = "/api/v1/auth/login",
    tag = "auth",
    request_body = LoginRequest,
    responses(
        (status = 200, body = SessionResponse, description = "Signed in; sets the refresh cookie"),
        (status = 401, body = Problem, content_type = "application/problem+json", description = "Invalid credentials"),
        (status = 404, body = Problem, content_type = "application/problem+json", description = "Local sign-in disabled"),
        (status = 429, body = Problem, content_type = "application/problem+json", description = "Too many attempts"),
    )
)]
pub async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    Json(body): Json<LoginRequest>,
) -> AppResult<(CookieJar, Json<SessionResponse>)> {
    if !state.config.local_login_enabled {
        return Err(AppError::Disabled("Password sign-in"));
    }
    let login_key = format!("login:{}", body.login.trim().to_lowercase());
    let ip_key = format!("ip:{}", client_ip(&headers));
    if state.limiter.is_blocked(&login_key) || state.limiter.is_blocked(&ip_key) {
        return Err(AppError::TooManyRequests);
    }

    let user = users::find_by_login(&state.db, &body.login).await?;
    let hash = user.as_ref().and_then(|u| u.password_hash.clone());
    let password = body.password;
    let valid = tokio::task::spawn_blocking(move || password::verify(&password, hash.as_deref()))
        .await
        .map_err(anyhow::Error::from)?;
    let Some(user) = user.filter(|_| valid) else {
        state.limiter.record_failure(&login_key);
        state.limiter.record_failure(&ip_key);
        return Err(AppError::InvalidCredentials);
    };
    state.limiter.reset(&login_key);

    let issued = refresh::issue(&state.db, &state.config, user.id, body.remember).await?;
    let jar = jar.add(cookies::refresh(
        &state.config,
        issued.token,
        issued.persistent,
        issued.expires_at,
    ));
    Ok((jar, Json(session(&state.config, user)?)))
}

/// Exchanges the refresh cookie for a new access token (and rotates the cookie).
#[utoipa::path(
    post,
    path = "/api/v1/auth/refresh",
    tag = "auth",
    responses(
        (status = 200, body = SessionResponse, description = "Session renewed; rotates the refresh cookie"),
        (status = 401, body = Problem, content_type = "application/problem+json", description = "Missing, expired or revoked refresh cookie"),
    )
)]
pub async fn refresh_session(State(state): State<AppState>, jar: CookieJar) -> Response {
    let Some(presented) = jar
        .get(cookies::REFRESH_COOKIE)
        .map(|c| c.value().to_owned())
    else {
        return AppError::Unauthorized.into_response();
    };
    let result: AppResult<_> = async {
        let issued = refresh::rotate(&state.db, &state.config, &presented).await?;
        let user = user::Entity::find_by_id(issued.user_id)
            .one(&state.db)
            .await?
            .ok_or(AppError::Unauthorized)?;
        Ok((issued, user))
    }
    .await;
    match result {
        Ok((issued, user)) => match session(&state.config, user) {
            Ok(body) => {
                let jar = jar.add(cookies::refresh(
                    &state.config,
                    issued.token,
                    issued.persistent,
                    issued.expires_at,
                ));
                (jar, Json(body)).into_response()
            }
            Err(e) => e.into_response(),
        },
        Err(e @ AppError::Unauthorized) => {
            (jar.add(cookies::clear_refresh(&state.config)), e).into_response()
        }
        Err(e) => e.into_response(),
    }
}

/// Signs out: revokes the session and clears the cookie.
#[utoipa::path(post, path = "/api/v1/auth/logout", tag = "auth", responses((status = 204, description = "Signed out; clears the refresh cookie")))]
pub async fn logout(
    State(state): State<AppState>,
    jar: CookieJar,
) -> AppResult<(StatusCode, CookieJar)> {
    if let Some(cookie) = jar.get(cookies::REFRESH_COOKIE) {
        refresh::revoke(&state.db, cookie.value()).await?;
    }
    Ok((
        StatusCode::NO_CONTENT,
        jar.add(cookies::clear_refresh(&state.config)),
    ))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct OidcStartQuery {
    /// Local path to open after signing in.
    pub redirect: Option<String>,
    /// "Keep me signed in".
    pub remember: Option<bool>,
}

#[derive(Serialize, Deserialize)]
struct StoredState {
    #[serde(flatten)]
    login: LoginState,
    remember: bool,
}

fn login_error(code: &str) -> Redirect {
    Redirect::to(&format!("/login?error={code}"))
}

/// Starts single sign-on: redirects to the identity provider.
#[utoipa::path(
    get,
    path = "/api/v1/auth/oidc",
    tag = "auth",
    params(OidcStartQuery),
    responses((status = 303, description = "Redirect to the identity provider"))
)]
pub async fn oidc_start(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Query(query): Query<OidcStartQuery>,
) -> Response {
    let Some(oidc) = state.oidc.clone() else {
        return login_error("sso_unavailable").into_response();
    };
    let redirect = sanitize_redirect(query.redirect.as_deref());
    match oidc.authorize(redirect).await {
        Ok((url, login)) => {
            let stored = StoredState {
                login,
                remember: query.remember.unwrap_or(false),
            };
            let value = serde_json::to_string(&stored).expect("serializable");
            let jar = jar.add(cookies::oidc_state(&state.config, value));
            (jar, Redirect::to(url.as_str())).into_response()
        }
        Err(_) => login_error("sso_unavailable").into_response(),
    }
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct OidcCallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
}

/// Completes single sign-on: verifies the provider's answer, signs the user
/// in, sets the refresh cookie and returns to the app. No token is put in the URL.
#[utoipa::path(
    get,
    path = "/api/v1/auth/oidc/callback",
    tag = "auth",
    params(OidcCallbackQuery),
    responses((status = 303, description = "Redirect to /auth/callback, or to /login?error=… on failure"))
)]
pub async fn oidc_callback(
    State(state): State<AppState>,
    private: PrivateCookieJar,
    jar: CookieJar,
    Query(query): Query<OidcCallbackQuery>,
) -> Response {
    let stored: Option<StoredState> = private
        .get(cookies::OIDC_STATE_COOKIE)
        .and_then(|c| serde_json::from_str(c.value()).ok());
    let private = private.remove(cookies::clear_oidc_state(&state.config));

    let outcome: Result<_, &'static str> = async {
        let oidc = state.oidc.clone().ok_or("sso_unavailable")?;
        if let Some(error) = &query.error {
            tracing::info!(%error, "identity provider returned an error");
            return Err("sso_failed");
        }
        let stored = stored.ok_or("sso_failed")?;
        let (code, returned_state) = query
            .code
            .clone()
            .zip(query.state.clone())
            .ok_or("sso_failed")?;
        let identity = oidc
            .complete(&stored.login, &returned_state, code)
            .await
            .map_err(|e| {
                tracing::warn!(error = %e, "SSO callback rejected");
                "sso_failed"
            })?;
        let (user, _) = users::sign_in_with_identity(&state.db, &identity, oidc.auto_register())
            .await
            .map_err(|_| "sso_failed")?
            .ok_or("sso_not_registered")?;
        let issued = refresh::issue(&state.db, &state.config, user.id, stored.remember)
            .await
            .map_err(|_| "sso_failed")?;
        Ok((issued, stored.login.redirect))
    }
    .await;

    match outcome {
        Ok((issued, redirect)) => {
            let jar = jar.add(cookies::refresh(
                &state.config,
                issued.token,
                issued.persistent,
                issued.expires_at,
            ));
            let target = format!(
                "/auth/callback?redirect={}",
                url::form_urlencoded::byte_serialize(redirect.as_bytes()).collect::<String>()
            );
            (private, jar, Redirect::to(&target)).into_response()
        }
        Err(code) => (private, login_error(code)).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_client_ip_from_ingress_headers() {
        let mut headers = HeaderMap::new();
        assert_eq!(client_ip(&headers), "unknown");
        headers.insert("x-forwarded-for", "10.0.0.1, 10.0.0.2".parse().unwrap());
        assert_eq!(client_ip(&headers), "10.0.0.1");
        headers.insert("x-real-ip", "192.168.1.9".parse().unwrap());
        assert_eq!(client_ip(&headers), "192.168.1.9");
    }
}
