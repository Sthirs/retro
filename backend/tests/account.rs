//! The "Your account" page: editable display name (spec §14.1).

mod common;

use axum::{
    body::Body,
    http::{Request, header},
};
use common::{TestApp, TestResponse};
use retro_backend::{auth::oidc::Identity, domain::users::sign_in_with_identity};
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

fn identity(subject: &str, email: &str) -> Identity {
    Identity {
        issuer: "http://idp.test".into(),
        subject: subject.into(),
        email: email.into(),
        name: Some("Alice From SSO".into()),
        picture: None,
    }
}

/// A valid 1x1 transparent PNG.
const PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0xF8, 0xFF, 0xFF, 0x3F,
    0x00, 0x05, 0xFE, 0x02, 0xFE, 0xA7, 0x35, 0x81, 0x84, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E,
    0x44, 0xAE, 0x42, 0x60, 0x82,
];

/// A valid GIF header (enough for the magic-byte check).
const GIF: &[u8] = b"GIF89a\x01\x00\x01\x00\x00\x00\x00;";

fn sha_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Sends `method path` with a raw body, like `TestApp::call` does for JSON.
async fn put_bytes(
    app: &TestApp,
    path: &str,
    token: &str,
    content_type: &str,
    bytes: Vec<u8>,
) -> TestResponse {
    app.send(
        Request::builder()
            .method("PUT")
            .uri(path)
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .header(header::CONTENT_TYPE, content_type)
            .body(Body::from(bytes))
            .unwrap(),
    )
    .await
}

async fn user_id(app: &TestApp, token: &str) -> String {
    app.get_with_token("/api/v1/me", token).await.json()["user"]["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn create_team(app: &TestApp, token: &str, name: &str) -> String {
    let res = app
        .call(
            "POST",
            "/api/v1/teams",
            token,
            Some(json!({ "name": name })),
        )
        .await;
    assert_eq!(res.status, 201, "{}", res.text());
    res.json()["id"].as_str().unwrap().to_owned()
}

/// Makes `token`'s user a member of `team` through the join link.
async fn join(app: &TestApp, admin: &str, team: &str, token: &str) {
    let link = app
        .call(
            "GET",
            &format!("/api/v1/teams/{team}/join-link"),
            admin,
            None,
        )
        .await
        .json();
    let join_token = link["url"].as_str().unwrap().rsplit('/').next().unwrap();
    let res = app
        .call("POST", &format!("/api/v1/join/{join_token}"), token, None)
        .await;
    assert_eq!(res.status, 200, "{}", res.text());
}

#[tokio::test]
async fn renames_the_current_user() {
    let app = TestApp::new().await;
    let token = app.token_for("alice").await;

    let res = app
        .call(
            "PATCH",
            "/api/v1/me",
            &token,
            Some(json!({ "displayName": "  Alice R. " })),
        )
        .await;
    assert_eq!(res.status, 200, "{}", res.text());
    assert_eq!(res.json()["displayName"], "Alice R.");

    let me = app.get_with_token("/api/v1/me", &token).await;
    assert_eq!(me.json()["user"]["displayName"], "Alice R.");
}

#[tokio::test]
async fn rejects_blank_and_overlong_names() {
    let app = TestApp::new().await;
    let token = app.token_for("alice").await;

    for name in ["   ".to_owned(), "a".repeat(81)] {
        let res = app
            .call(
                "PATCH",
                "/api/v1/me",
                &token,
                Some(json!({ "displayName": name })),
            )
            .await;
        assert_eq!(res.status, 400, "{}", res.text());
        assert_eq!(res.json()["code"], "invalid_display_name");
    }

    let res = app
        .call(
            "PATCH",
            "/api/v1/me",
            &token,
            Some(json!({ "displayName": "a".repeat(80) })),
        )
        .await;
    assert_eq!(res.status, 200, "{}", res.text());
}

#[tokio::test]
async fn rename_requires_auth() {
    let app = TestApp::new().await;
    let res = app
        .call(
            "PATCH",
            "/api/v1/me",
            "",
            Some(json!({ "displayName": "X" })),
        )
        .await;
    assert_eq!(res.status, 401);
}

#[tokio::test]
async fn uploads_and_serves_a_picture() {
    let app = TestApp::new().await;
    let alice = app.token_for("alice").await;
    let id = user_id(&app, &alice).await;
    let sha = sha_hex(PNG);

    let res = put_bytes(
        &app,
        "/api/v1/me/picture",
        &alice,
        "image/png",
        PNG.to_vec(),
    )
    .await;
    assert_eq!(res.status, 200, "{}", res.text());
    assert_eq!(res.json()["pictureVersion"], sha[..16]);

    let res = app
        .get_with_token(&format!("/api/v1/users/{id}/picture"), &alice)
        .await;
    assert_eq!(res.status, 200, "{}", res.text());
    assert_eq!(res.headers["content-type"], "image/png");
    assert_eq!(res.body, PNG);
    assert_eq!(res.headers["etag"], format!("\"{sha}\"").as_str());
    let cache = res.headers["cache-control"].to_str().unwrap();
    assert!(cache.contains("private"), "{cache}");
    assert!(cache.contains("immutable"), "{cache}");
    assert_eq!(res.headers["x-content-type-options"], "nosniff");

    // Error responses are never cached.
    let res = app
        .get_with_token(&format!("/api/v1/users/{}/picture", Uuid::now_v7()), &alice)
        .await;
    assert_eq!(res.status, 404);
    assert_eq!(res.headers["cache-control"], "no-store");
    let res = app
        .get_with_token(&format!("/api/v1/users/{id}/picture"), "")
        .await;
    assert_eq!(res.status, 401);
    assert_eq!(res.headers["cache-control"], "no-store");
}

#[tokio::test]
async fn replacing_the_picture_serves_the_new_bytes() {
    let app = TestApp::new().await;
    let alice = app.token_for("alice").await;
    let id = user_id(&app, &alice).await;

    let first = put_bytes(
        &app,
        "/api/v1/me/picture",
        &alice,
        "image/png",
        PNG.to_vec(),
    )
    .await;
    assert_eq!(first.status, 200, "{}", first.text());
    let second = put_bytes(
        &app,
        "/api/v1/me/picture",
        &alice,
        "image/gif",
        GIF.to_vec(),
    )
    .await;
    assert_eq!(second.status, 200, "{}", second.text());
    let sha = sha_hex(GIF);
    assert_ne!(
        first.json()["pictureVersion"],
        second.json()["pictureVersion"]
    );
    assert_eq!(second.json()["pictureVersion"], sha[..16]);

    let res = app
        .get_with_token(&format!("/api/v1/users/{id}/picture"), &alice)
        .await;
    assert_eq!(res.status, 200, "{}", res.text());
    assert_eq!(res.headers["content-type"], "image/gif");
    assert_eq!(res.headers["etag"], format!("\"{sha}\"").as_str());
    assert_eq!(res.body, GIF);
}

#[tokio::test]
async fn teammates_see_the_picture_strangers_do_not() {
    let app = TestApp::new().await;
    let alice = app.token_for("alice").await;
    let bob = app.token_for("bob").await;
    let id = user_id(&app, &alice).await;
    let res = put_bytes(
        &app,
        "/api/v1/me/picture",
        &alice,
        "image/png",
        PNG.to_vec(),
    )
    .await;
    assert_eq!(res.status, 200, "{}", res.text());

    let team = create_team(&app, &alice, "Squad").await;
    join(&app, &alice, &team, &bob).await;

    let path = format!("/api/v1/users/{id}/picture");
    let res = app.get_with_token(&path, &bob).await;
    assert_eq!(res.status, 200, "{}", res.text());

    let carol = app.create_user("carol", "carol@example.com", "Carol").await;
    let res = app.get_with_token(&path, &carol).await;
    assert_eq!(res.status, 404);
    assert_eq!(res.json()["code"], "picture_not_found");
}

#[tokio::test]
async fn rejects_bytes_that_are_not_an_image() {
    let app = TestApp::new().await;
    let alice = app.token_for("alice").await;
    let res = put_bytes(
        &app,
        "/api/v1/me/picture",
        &alice,
        "image/png",
        b"not an image".to_vec(),
    )
    .await;
    assert_eq!(res.status, 415, "{}", res.text());
    assert_eq!(res.json()["code"], "unsupported_picture_type");
}

#[tokio::test]
async fn rejects_pictures_over_5_mb() {
    let app = TestApp::new().await;
    let alice = app.token_for("alice").await;
    let mut bytes = PNG[..8].to_vec();
    bytes.resize(5 * 1024 * 1024 + 1, 0);
    let res = put_bytes(&app, "/api/v1/me/picture", &alice, "image/png", bytes).await;
    assert_eq!(res.status, 413, "{}", res.text());
    assert_eq!(res.json()["code"], "picture_too_large");
    assert_eq!(res.headers["content-type"], "application/problem+json");
}

#[tokio::test]
async fn oversized_bodies_get_problem_json_too() {
    let app = TestApp::new().await;
    let alice = app.token_for("alice").await;
    let mut bytes = PNG[..8].to_vec();
    bytes.resize(5 * 1024 * 1024 + 1024, 0);
    let res = put_bytes(&app, "/api/v1/me/picture", &alice, "image/png", bytes).await;
    assert_eq!(res.status, 413, "{}", res.text());
    assert_eq!(res.headers["content-type"], "application/problem+json");
    assert_eq!(res.json()["code"], "picture_too_large");
}

#[tokio::test]
async fn removes_the_picture() {
    let app = TestApp::new().await;
    let alice = app.token_for("alice").await;
    let id = user_id(&app, &alice).await;
    put_bytes(
        &app,
        "/api/v1/me/picture",
        &alice,
        "image/png",
        PNG.to_vec(),
    )
    .await;

    let res = app.call("DELETE", "/api/v1/me/picture", &alice, None).await;
    assert_eq!(res.status, 200, "{}", res.text());
    assert!(res.json()["pictureVersion"].is_null());

    let res = app
        .get_with_token(&format!("/api/v1/users/{id}/picture"), &alice)
        .await;
    assert_eq!(res.status, 404);
}

#[tokio::test]
async fn me_lists_picture_version() {
    let app = TestApp::new().await;
    let alice = app.token_for("alice").await;
    let res = put_bytes(
        &app,
        "/api/v1/me/picture",
        &alice,
        "image/png",
        PNG.to_vec(),
    )
    .await;
    let version = res.json()["pictureVersion"].clone();
    assert!(version.is_string());

    let me = app.get_with_token("/api/v1/me", &alice).await;
    assert_eq!(me.json()["user"]["pictureVersion"], version);
}

#[tokio::test]
async fn me_reports_sign_in_method_and_member_since() {
    let app = TestApp::new().await;
    let alice = app.token_for("alice").await;

    let me = app.get_with_token("/api/v1/me", &alice).await.json();
    assert_eq!(me["signInMethod"], "password");
    let since = me["memberSince"].as_str().expect("memberSince is a string");
    time::OffsetDateTime::parse(since, &time::format_description::well_known::Rfc3339)
        .expect("memberSince is RFC 3339");

    sign_in_with_identity(&app.db, &identity("sub-a", "alice@retro.local"), false)
        .await
        .unwrap()
        .unwrap();
    let me = app.get_with_token("/api/v1/me", &alice).await.json();
    assert_eq!(me["signInMethod"], "sso");
}
