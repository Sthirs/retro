//! Cookies set by the auth endpoints.

use axum_extra::extract::cookie::{Cookie, SameSite};
use time::OffsetDateTime;

use crate::config::Config;

pub const REFRESH_COOKIE: &str = "retro.refresh";
pub const REFRESH_COOKIE_PATH: &str = "/api/v1/auth";
pub const OIDC_STATE_COOKIE: &str = "retro.oidc";
pub const OIDC_STATE_COOKIE_PATH: &str = "/api/v1/auth/oidc";

/// The refresh cookie. A session cookie unless "keep me signed in" was ticked.
pub fn refresh(
    config: &Config,
    token: String,
    persistent: bool,
    expires_at: OffsetDateTime,
) -> Cookie<'static> {
    let mut builder = Cookie::build((REFRESH_COOKIE, token))
        .path(REFRESH_COOKIE_PATH)
        .http_only(true)
        .secure(config.cookie_secure)
        .same_site(SameSite::Strict);
    if persistent {
        builder = builder.expires(expires_at);
    }
    builder.build()
}

pub fn clear_refresh(config: &Config) -> Cookie<'static> {
    let mut cookie = refresh(config, String::new(), false, OffsetDateTime::UNIX_EPOCH);
    cookie.make_removal();
    cookie
}

/// Short-lived, encrypted cookie holding the OIDC state, nonce and PKCE verifier.
/// `Lax`, because the IdP redirects back with a cross-site top-level GET.
pub fn oidc_state(config: &Config, value: String) -> Cookie<'static> {
    Cookie::build((OIDC_STATE_COOKIE, value))
        .path(OIDC_STATE_COOKIE_PATH)
        .http_only(true)
        .secure(config.cookie_secure)
        .same_site(SameSite::Lax)
        .max_age(time::Duration::minutes(10))
        .build()
}

pub fn clear_oidc_state(config: &Config) -> Cookie<'static> {
    let mut cookie = oidc_state(config, String::new());
    cookie.make_removal();
    cookie
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_cookie_attributes() {
        let config = Config::for_tests("postgres://unused");
        let session = refresh(&config, "t".into(), false, OffsetDateTime::now_utc());
        assert_eq!(session.http_only(), Some(true));
        assert_eq!(session.same_site(), Some(SameSite::Strict));
        assert_eq!(session.path(), Some(REFRESH_COOKIE_PATH));
        assert!(
            session.expires().is_none(),
            "session cookie must not persist"
        );

        let persistent = refresh(&config, "t".into(), true, OffsetDateTime::now_utc());
        assert!(persistent.expires().is_some());
    }
}
