//! Teams and memberships (spec §5, §12, §14.2).

mod common;

use common::TestApp;
use retro_backend::entities::{invitation, team, team_member};
use sea_orm::{EntityTrait, PaginatorTrait};
use serde_json::{Value, json};

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

async fn user_id(app: &TestApp, token: &str) -> String {
    app.get_with_token("/api/v1/me", token).await.json()["user"]["id"]
        .as_str()
        .unwrap()
        .to_owned()
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
async fn creates_and_lists_teams() {
    let app = TestApp::new().await;
    let alice = app.token_for("alice").await;

    let res = app
        .call(
            "POST",
            "/api/v1/teams",
            &alice,
            Some(json!({ "name": "  Platform Team " })),
        )
        .await;
    assert_eq!(res.status, 201);
    let team = res.json();
    assert_eq!(team["name"], "Platform Team");
    assert_eq!(team["role"], "admin");
    assert_eq!(team["memberCount"], 1);
    create_team(&app, &alice, "alpha").await;

    let list = app.get_with_token("/api/v1/teams", &alice).await.json();
    let names: Vec<&str> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["alpha", "Platform Team"]);

    let me = app.get_with_token("/api/v1/me", &alice).await.json();
    assert_eq!(me["teams"].as_array().unwrap().len(), 2);
    assert_eq!(me["invitations"], json!([]));
}

#[tokio::test]
async fn rejects_invalid_team_names() {
    let app = TestApp::new().await;
    let alice = app.token_for("alice").await;
    for name in ["", "   ", &"x".repeat(61)] {
        let res = app
            .call(
                "POST",
                "/api/v1/teams",
                &alice,
                Some(json!({ "name": name })),
            )
            .await;
        assert_eq!(res.status, 400);
        assert_eq!(res.json()["code"], "invalid_team_name");
    }
}

#[tokio::test]
async fn requires_authentication() {
    let app = TestApp::new().await;
    assert_eq!(app.get("/api/v1/teams").await.status, 401);
}

#[tokio::test]
async fn hides_teams_from_non_members() {
    let app = TestApp::new().await;
    let alice = app.token_for("alice").await;
    let bob = app.token_for("bob").await;
    let team = create_team(&app, &alice, "Secret").await;

    for (method, path, body) in [
        ("GET", format!("/api/v1/teams/{team}"), None),
        (
            "PATCH",
            format!("/api/v1/teams/{team}"),
            Some(json!({ "name": "x" })),
        ),
        ("DELETE", format!("/api/v1/teams/{team}"), None),
        ("GET", format!("/api/v1/teams/{team}/members"), None),
        ("GET", format!("/api/v1/teams/{team}/join-link"), None),
        ("GET", format!("/api/v1/teams/{team}/invitations"), None),
        (
            "POST",
            format!("/api/v1/teams/{team}/invitations"),
            Some(json!({ "email": "x@y.dev" })),
        ),
    ] {
        let res = app.call(method, &path, &bob, body).await;
        assert_eq!(res.status, 404, "{method} {path}");
        assert_eq!(res.json()["code"], "team_not_found");
    }
    assert_eq!(
        app.get_with_token("/api/v1/teams", &bob).await.json(),
        json!([])
    );
}

#[tokio::test]
async fn admins_rename_members_cannot() {
    let app = TestApp::new().await;
    let alice = app.token_for("alice").await;
    let bob = app.token_for("bob").await;
    let team = create_team(&app, &alice, "Old").await;
    join(&app, &alice, &team, &bob).await;

    let path = format!("/api/v1/teams/{team}");
    let res = app
        .call("PATCH", &path, &bob, Some(json!({ "name": "Bob's" })))
        .await;
    assert_eq!(res.status, 403);

    let res = app
        .call("PATCH", &path, &alice, Some(json!({ "name": "New" })))
        .await;
    assert_eq!(res.status, 200);
    assert_eq!(res.json()["name"], "New");
    assert_eq!(app.get_with_token(&path, &bob).await.json()["name"], "New");
    assert_eq!(
        app.get_with_token(&path, &bob).await.json()["role"],
        "member"
    );
}

#[tokio::test]
async fn lists_members_admins_first() {
    let app = TestApp::new().await;
    let alice = app.token_for("alice").await;
    let bob = app.token_for("bob").await;
    let team = create_team(&app, &alice, "T").await;
    join(&app, &alice, &team, &bob).await;

    let members = app
        .get_with_token(&format!("/api/v1/teams/{team}/members"), &bob)
        .await
        .json();
    let members = members.as_array().unwrap();
    assert_eq!(members.len(), 2);
    assert_eq!(members[0]["user"]["displayName"], "Alice Rossi");
    assert_eq!(members[0]["role"], "admin");
    assert_eq!(members[1]["user"]["email"], "bob@retro.local");
    assert_eq!(members[1]["role"], "member");
    assert!(members[1]["joinedAt"].as_str().unwrap().contains('T'));
}

#[tokio::test]
async fn role_changes_keep_one_admin() {
    let app = TestApp::new().await;
    let alice = app.token_for("alice").await;
    let bob = app.token_for("bob").await;
    let alice_id = user_id(&app, &alice).await;
    let bob_id = user_id(&app, &bob).await;
    let team = create_team(&app, &alice, "T").await;
    join(&app, &alice, &team, &bob).await;
    let member = |id: &str| format!("/api/v1/teams/{team}/members/{id}");

    // Members cannot change roles.
    let res = app
        .call(
            "PATCH",
            &member(&bob_id),
            &bob,
            Some(json!({ "role": "admin" })),
        )
        .await;
    assert_eq!(res.status, 403);

    // The last admin cannot be demoted.
    let res = app
        .call(
            "PATCH",
            &member(&alice_id),
            &alice,
            Some(json!({ "role": "member" })),
        )
        .await;
    assert_eq!(res.status, 409);
    assert_eq!(res.json()["code"], "last_admin");

    // Promote Bob, then Alice can step down.
    let res = app
        .call(
            "PATCH",
            &member(&bob_id),
            &alice,
            Some(json!({ "role": "admin" })),
        )
        .await;
    assert_eq!(res.status, 204);
    let res = app
        .call(
            "PATCH",
            &member(&alice_id),
            &alice,
            Some(json!({ "role": "member" })),
        )
        .await;
    assert_eq!(res.status, 204);

    // Now Alice is a member and can no longer manage roles.
    let res = app
        .call(
            "PATCH",
            &member(&alice_id),
            &alice,
            Some(json!({ "role": "admin" })),
        )
        .await;
    assert_eq!(res.status, 403);
}

#[tokio::test]
async fn removing_and_leaving() {
    let app = TestApp::new().await;
    let alice = app.token_for("alice").await;
    let bob = app.token_for("bob").await;
    let carol = app
        .create_user("carol", "carol@retro.local", "Carol Verdi")
        .await;
    let alice_id = user_id(&app, &alice).await;
    let bob_id = user_id(&app, &bob).await;
    let carol_id = user_id(&app, &carol).await;
    let team = create_team(&app, &alice, "T").await;
    join(&app, &alice, &team, &bob).await;
    join(&app, &alice, &team, &carol).await;
    let member = |id: &str| format!("/api/v1/teams/{team}/members/{id}");

    // A member cannot remove someone else.
    assert_eq!(
        app.call("DELETE", &member(&carol_id), &bob, None)
            .await
            .status,
        403
    );

    // The last admin cannot leave while others remain.
    let res = app.call("DELETE", &member(&alice_id), &alice, None).await;
    assert_eq!(res.status, 409);
    assert_eq!(res.json()["code"], "last_admin");

    // An admin removes Carol; Bob leaves on his own.
    assert_eq!(
        app.call("DELETE", &member(&carol_id), &alice, None)
            .await
            .status,
        204
    );
    assert_eq!(
        app.call("DELETE", &member(&bob_id), &bob, None)
            .await
            .status,
        204
    );
    assert_eq!(
        app.get_with_token(&format!("/api/v1/teams/{team}"), &bob)
            .await
            .status,
        404
    );

    // The last member leaving deletes the team.
    assert_eq!(
        app.call("DELETE", &member(&alice_id), &alice, None)
            .await
            .status,
        204
    );
    assert_eq!(team::Entity::find().count(&app.db).await.unwrap(), 0);
}

#[tokio::test]
async fn deleting_a_team_cascades() {
    let app = TestApp::new().await;
    let alice = app.token_for("alice").await;
    let bob = app.token_for("bob").await;
    let team = create_team(&app, &alice, "Doomed").await;
    join(&app, &alice, &team, &bob).await;
    let res = app
        .call(
            "POST",
            &format!("/api/v1/teams/{team}/invitations"),
            &alice,
            Some(json!({ "email": "nina@acme.dev" })),
        )
        .await;
    assert_eq!(res.status, 201);

    // Members cannot delete.
    let path = format!("/api/v1/teams/{team}");
    assert_eq!(app.call("DELETE", &path, &bob, None).await.status, 403);

    assert_eq!(app.call("DELETE", &path, &alice, None).await.status, 204);
    assert_eq!(team::Entity::find().count(&app.db).await.unwrap(), 0);
    assert_eq!(team_member::Entity::find().count(&app.db).await.unwrap(), 0);
    assert_eq!(invitation::Entity::find().count(&app.db).await.unwrap(), 0);
    assert_eq!(
        app.get_with_token("/api/v1/teams", &bob).await.json(),
        Value::Array(vec![])
    );
}
