//! The team join link (spec §6.2).

mod common;

use common::TestApp;
use serde_json::json;

async fn setup() -> (TestApp, String, String, String) {
    let app = TestApp::new().await;
    let alice = app.token_for("alice").await;
    let bob = app.token_for("bob").await;
    let res = app
        .call(
            "POST",
            "/api/v1/teams",
            &alice,
            Some(json!({ "name": "Platform Team" })),
        )
        .await;
    let team = res.json()["id"].as_str().unwrap().to_owned();
    (app, alice, bob, team)
}

fn token_of(link: &serde_json::Value) -> String {
    link["url"]
        .as_str()
        .unwrap()
        .rsplit('/')
        .next()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn joins_through_the_link() {
    let (app, alice, bob, team) = setup().await;
    let link = app
        .call(
            "GET",
            &format!("/api/v1/teams/{team}/join-link"),
            &alice,
            None,
        )
        .await
        .json();
    assert_eq!(link["enabled"], true);
    assert!(
        link["url"]
            .as_str()
            .unwrap()
            .starts_with("http://retro.test/join/")
    );

    let path = format!("/api/v1/join/{}", token_of(&link));
    let res = app.call("POST", &path, &bob, None).await;
    assert_eq!(res.status, 200);
    assert_eq!(res.json()["team"]["name"], "Platform Team");
    assert_eq!(res.json()["alreadyMember"], false);

    // Opening it again is harmless.
    let res = app.call("POST", &path, &bob, None).await;
    assert_eq!(res.json()["alreadyMember"], true);

    let team_res = app
        .get_with_token(&format!("/api/v1/teams/{team}"), &bob)
        .await
        .json();
    assert_eq!(team_res["role"], "member");
    assert_eq!(team_res["memberCount"], 2);

    // Any member can read the link.
    let res = app
        .call(
            "GET",
            &format!("/api/v1/teams/{team}/join-link"),
            &bob,
            None,
        )
        .await;
    assert_eq!(res.status, 200);
}

#[tokio::test]
async fn admins_regenerate_and_disable() {
    let (app, alice, bob, team) = setup().await;
    let carol = app
        .create_user("carol", "carol@retro.local", "Carol Verdi")
        .await;
    let path = format!("/api/v1/teams/{team}/join-link");
    let old = app.call("GET", &path, &alice, None).await.json();
    app.call(
        "POST",
        &format!("/api/v1/join/{}", token_of(&old)),
        &bob,
        None,
    )
    .await;

    // Members cannot change the link.
    let res = app
        .call("POST", &path, &bob, Some(json!({ "action": "regenerate" })))
        .await;
    assert_eq!(res.status, 403);

    let new = app
        .call(
            "POST",
            &path,
            &alice,
            Some(json!({ "action": "regenerate" })),
        )
        .await
        .json();
    assert_ne!(token_of(&old), token_of(&new));
    assert_eq!(new["enabled"], true);

    let res = app
        .call(
            "POST",
            &format!("/api/v1/join/{}", token_of(&old)),
            &carol,
            None,
        )
        .await;
    assert_eq!(res.status, 404);
    assert_eq!(res.json()["code"], "join_link_invalid");

    let disabled = app
        .call("POST", &path, &alice, Some(json!({ "action": "disable" })))
        .await
        .json();
    assert_eq!(disabled["enabled"], false);
    let res = app
        .call(
            "POST",
            &format!("/api/v1/join/{}", token_of(&new)),
            &carol,
            None,
        )
        .await;
    assert_eq!(res.status, 404);

    let enabled = app
        .call("POST", &path, &alice, Some(json!({ "action": "enable" })))
        .await
        .json();
    assert_eq!(token_of(&enabled), token_of(&new));
    let res = app
        .call(
            "POST",
            &format!("/api/v1/join/{}", token_of(&new)),
            &carol,
            None,
        )
        .await;
    assert_eq!(res.status, 200);
}

#[tokio::test]
async fn rate_limits_unknown_tokens() {
    let (app, _alice, bob, _team) = setup().await;
    for _ in 0..20 {
        let res = app.call("POST", "/api/v1/join/nope", &bob, None).await;
        assert_eq!(res.status, 404);
    }
    let res = app.call("POST", "/api/v1/join/nope", &bob, None).await;
    assert_eq!(res.status, 429);
}

/// A join-link change racing a member removal must not restore the token the
/// removed member knew (spec §6.2).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn removal_racing_a_link_change_keeps_the_new_token() {
    let (app, alice, bob, _) = setup().await;
    let bob_id = app.get_with_token("/api/v1/me", &bob).await.json()["user"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    for round in 0..20 {
        let team = app
            .call(
                "POST",
                "/api/v1/teams",
                &alice,
                Some(json!({ "name": format!("Race {round}") })),
            )
            .await
            .json()["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let link_path = format!("/api/v1/teams/{team}/join-link");
        let known = token_of(&app.call("GET", &link_path, &alice, None).await.json());
        let join = format!("/api/v1/join/{known}");
        assert_eq!(app.call("POST", &join, &bob, None).await.status, 200);

        let remove = format!("/api/v1/teams/{team}/members/{bob_id}");
        let (disabled, removed) = tokio::join!(
            app.call(
                "POST",
                &link_path,
                &alice,
                Some(json!({ "action": "disable" }))
            ),
            app.call("DELETE", &remove, &alice, None),
        );
        assert_eq!(disabled.status, 200, "round {round}");
        assert_eq!(removed.status, 204, "round {round}");
        app.call(
            "POST",
            &link_path,
            &alice,
            Some(json!({ "action": "enable" })),
        )
        .await;

        let res = app.call("POST", &join, &bob, None).await;
        assert_eq!(res.status, 404, "round {round}: the old token came back");
    }
}
