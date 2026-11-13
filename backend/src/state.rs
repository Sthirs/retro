//! Shared application state handed to every handler.

use std::sync::Arc;

use axum::extract::FromRef;
use axum_extra::extract::cookie::Key;
use sea_orm::DatabaseConnection;

use crate::{
    auth::{oidc::OidcProvider, rate_limit::RateLimiter},
    config::Config,
};

/// Unknown invitation or join tokens a user may try in 10 minutes.
const TOKEN_FAILURE_LIMIT: u32 = 20;
/// Invitations a user may create in an hour.
const INVITATION_LIMIT: u32 = 100;

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub config: Arc<Config>,
    pub oidc: Option<Arc<OidcProvider>>,
    pub limiter: Arc<RateLimiter>,
    /// Failed invitation and join-link token lookups, per user (spec §20).
    pub token_limiter: Arc<RateLimiter>,
    /// Invitations created, per user (spec §20).
    pub invite_limiter: Arc<RateLimiter>,
    /// Encrypts short-lived cookies (OIDC login state).
    pub cookie_key: Key,
}

impl AppState {
    pub fn new(db: DatabaseConnection, config: Config) -> Self {
        let config = Arc::new(config);
        let oidc = config.oidc.clone().map(|oidc| {
            Arc::new(OidcProvider::new(
                oidc,
                config.public("/api/v1/auth/oidc/callback"),
            ))
        });
        let cookie_key = Key::derive_from(config.state_secret.as_bytes());
        Self {
            db,
            oidc,
            limiter: Arc::new(RateLimiter::new(
                config.login_rate_limit,
                std::time::Duration::from_secs(300),
            )),
            token_limiter: Arc::new(RateLimiter::new(
                TOKEN_FAILURE_LIMIT,
                std::time::Duration::from_secs(600),
            )),
            invite_limiter: Arc::new(RateLimiter::new(
                INVITATION_LIMIT,
                std::time::Duration::from_secs(3600),
            )),
            cookie_key,
            config,
        }
    }
}

impl FromRef<AppState> for Key {
    fn from_ref(state: &AppState) -> Self {
        state.cookie_key.clone()
    }
}
