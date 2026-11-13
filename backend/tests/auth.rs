mod common;

use common::TestApp;
use retro_backend::seed;
use sea_orm::{EntityTrait, PaginatorTrait};

#[tokio::test]
async fn auth_config_reflects_settings() {
    let app = TestApp::with_config(|c| {
        c.password_reset_url = Some("https://id.example.com/reset".parse().unwrap());
    })
    .await;
    let res = app.get("/api/v1/auth/config").await;
    assert_eq!(res.status, 200);
    let json = res.json();
    assert_eq!(json["localLogin"], true);
    assert_eq!(json["sso"]["enabled"], false);
    assert_eq!(json["passwordResetUrl"], "https://id.example.com/reset");
    assert_eq!(res.headers["cache-control"], "no-store");
}

#[tokio::test]
async fn signs_in_with_username_or_email() {
    let app = TestApp::new().await;

    let by_username = app.login("alice", "password", false).await;
    assert_eq!(by_username.status, 200, "{}", by_username.text());
    let json = by_username.json();
    assert_eq!(json["user"]["displayName"], "Alice Rossi");
    assert_eq!(json["user"]["email"], "alice@retro.local");
    assert!(json["accessToken"].as_str().unwrap().len() > 20);

    let by_email = app.login("  BOB@retro.local ", "password", false).await;
    assert_eq!(by_email.status, 200);
    assert_eq!(by_email.json()["user"]["username"], "bob");
}

#[tokio::test]
async fn rejects_wrong_password_and_unknown_user_alike() {
    let app = TestApp::new().await;
    for (login, password) in [("alice", "wrong"), ("nobody", "password"), ("", "")] {
        let res = app.login(login, password, false).await;
        assert_eq!(res.status, 401);
        assert_eq!(res.headers["content-type"], "application/problem+json");
        assert_eq!(res.json()["code"], "invalid_credentials");
        assert!(res.set_cookie("retro.refresh").is_none());
    }
}

#[tokio::test]
async fn rate_limits_failed_attempts() {
    let app = TestApp::with_config(|c| c.login_rate_limit = 2).await;
    assert_eq!(app.login("alice", "x", false).await.status, 401);
    assert_eq!(app.login("alice", "x", false).await.status, 401);
    let blocked = app.login("alice", "password", false).await;
    assert_eq!(blocked.status, 429);
    assert_eq!(blocked.json()["code"], "too_many_requests");
}

#[tokio::test]
async fn local_login_can_be_disabled() {
    let app = TestApp::with_config(|c| c.local_login_enabled = false).await;
    assert_eq!(app.login("alice", "password", false).await.status, 404);
    assert_eq!(
        app.get("/api/v1/auth/config").await.json()["localLogin"],
        false
    );
}

#[tokio::test]
async fn remember_me_controls_cookie_lifetime() {
    let app = TestApp::new().await;
    let session = app.login("alice", "password", false).await;
    let cookie = session.set_cookie("retro.refresh").unwrap();
    assert!(cookie.contains("HttpOnly"));
    assert!(cookie.contains("SameSite=Strict"));
    assert!(cookie.contains("Path=/api/v1/auth"));
    assert!(!cookie.contains("Expires"), "session cookie: {cookie}");

    let persistent = app.login("alice", "password", true).await;
    assert!(
        persistent
            .set_cookie("retro.refresh")
            .unwrap()
            .contains("Expires")
    );
}

#[tokio::test]
async fn me_requires_a_valid_token() {
    let app = TestApp::new().await;
    assert_eq!(app.get("/api/v1/me").await.status, 401);
    assert_eq!(
        app.get_with_token("/api/v1/me", "garbage").await.status,
        401
    );

    let token = app.login("bob", "password", false).await.json()["accessToken"]
        .as_str()
        .unwrap()
        .to_owned();
    let me = app.get_with_token("/api/v1/me", &token).await;
    assert_eq!(me.status, 200);
    assert_eq!(me.json()["user"]["displayName"], "Bob Bianchi");
}

#[tokio::test]
async fn refresh_rotates_and_detects_reuse() {
    let app = TestApp::new().await;
    let login = app.login("alice", "password", true).await;
    let first = login.cookie_pair("retro.refresh").unwrap();

    let refreshed = app.post_with_cookie("/api/v1/auth/refresh", &first).await;
    assert_eq!(refreshed.status, 200, "{}", refreshed.text());
    assert_eq!(refreshed.json()["user"]["username"], "alice");
    let second = refreshed.cookie_pair("retro.refresh").unwrap();
    assert_ne!(first, second);
    assert!(
        refreshed
            .set_cookie("retro.refresh")
            .unwrap()
            .contains("Expires")
    );

    // Replaying the first token (grace window is 0 in tests) revokes the family…
    let replay = app.post_with_cookie("/api/v1/auth/refresh", &first).await;
    assert_eq!(replay.status, 401);
    // …so the legitimate successor stops working too.
    assert_eq!(
        app.post_with_cookie("/api/v1/auth/refresh", &second)
            .await
            .status,
        401
    );
}

#[tokio::test]
async fn refresh_without_cookie_is_unauthorized() {
    let app = TestApp::new().await;
    let res = app
        .post_json("/api/v1/auth/refresh", serde_json::json!({}))
        .await;
    assert_eq!(res.status, 401);
    let res = app
        .post_with_cookie("/api/v1/auth/refresh", "retro.refresh=unknown")
        .await;
    assert_eq!(res.status, 401);
    assert!(
        res.set_cookie("retro.refresh")
            .unwrap()
            .contains("Max-Age=0")
    );
}

#[tokio::test]
async fn logout_revokes_the_session() {
    let app = TestApp::new().await;
    let cookie = app
        .login("alice", "password", false)
        .await
        .cookie_pair("retro.refresh")
        .unwrap();
    let out = app.post_with_cookie("/api/v1/auth/logout", &cookie).await;
    assert_eq!(out.status, 204);
    assert!(
        out.set_cookie("retro.refresh")
            .unwrap()
            .contains("Max-Age=0")
    );
    assert_eq!(
        app.post_with_cookie("/api/v1/auth/refresh", &cookie)
            .await
            .status,
        401
    );
}

#[tokio::test]
async fn sso_start_without_provider_returns_to_login() {
    let app = TestApp::new().await;
    let res = app.get("/api/v1/auth/oidc").await;
    assert_eq!(res.status, 303);
    assert_eq!(res.headers["location"], "/login?error=sso_unavailable");
    let cb = app.get("/api/v1/auth/oidc/callback?code=x&state=y").await;
    assert_eq!(cb.status, 303);
    assert_eq!(cb.headers["location"], "/login?error=sso_unavailable");
}

#[tokio::test]
async fn dev_seed_is_idempotent() {
    let app = TestApp::new().await;
    seed::seed_dev_users(&app.db).await.unwrap();
    seed::seed_dev_users(&app.db).await.unwrap();
    let count = retro_backend::entities::user::Entity::find()
        .count(&app.db)
        .await
        .unwrap();
    assert_eq!(count, seed::DEV_USERS.len() as u64);
    assert_eq!(
        app.login("alice", seed::DEV_PASSWORD, false).await.status,
        200
    );
}
