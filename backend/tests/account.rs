//! The "Your account" page: editable display name (spec §14.1).

mod common;

use common::TestApp;
use serde_json::json;

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
