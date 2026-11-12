//! Shared application state handed to every handler.

use std::sync::Arc;

use axum::extract::FromRef;
use axum_extra::extract::cookie::Key;
use sea_orm::DatabaseConnection;

use crate::{
    auth::{oidc::OidcProvider, rate_limit::RateLimiter},
    config::Config,
};

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub config: Arc<Config>,
    pub oidc: Option<Arc<OidcProvider>>,
    pub limiter: Arc<RateLimiter>,
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
