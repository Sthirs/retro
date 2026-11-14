//! The tasks board (spec §9, §13, §14).

mod common;

use common::TestApp;
use retro_backend::{
    domain::tasks,
    entities::{task, task::TaskStatus},
};
use sea_orm::{ActiveModelTrait, EntityTrait, PaginatorTrait, Set};
use serde_json::{Value, json};
use uuid::Uuid;

async fn create_team(app: &TestApp, token: &str, name: &str) -> Uuid {
    let res = app
        .call(
            "POST",
            "/api/v1/teams",
            token,
            Some(json!({ "name": name })),
        )
        .await;
    assert_eq!(res.status, 201, "{}", res.text());
    res.json()["id"].as_str().unwrap().parse().unwrap()
}

async fn user_id(app: &TestApp, token: &str) -> Uuid {
    app.get_with_token("/api/v1/me", token).await.json()["user"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

/// Makes `token`'s user a member of `team` through the join link.
async fn join(app: &TestApp, admin: &str, team: Uuid, token: &str) {
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

fn mention(id: Uuid, label: &str) -> String {
    format!(
        r#"<p><span data-type="mention" data-id="{id}" data-label="{label}">@{label}</span></p>"#
    )
}

async fn list(app: &TestApp, token: &str, team: Uuid, query: &str) -> Value {
    let res = app
        .call(
            "GET",
            &format!("/api/v1/teams/{team}/tasks{query}"),
            token,
            None,
        )
        .await;
    assert_eq!(res.status, 200, "{}", res.text());
    res.json()
}

fn ids(list: &Value) -> Vec<String> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn lists_tasks_by_column_and_position() {
    let app = TestApp::new().await;
    let alice = app.token_for("alice").await;
    let bob = app.token_for("bob").await;
    let carol = app.create_user("carol", "carol@example.com", "Carol").await;
    let (alice_id, bob_id) = (user_id(&app, &alice).await, user_id(&app, &bob).await);
    let team = create_team(&app, &alice, "Platform").await;
    join(&app, &alice, team, &bob).await;
    join(&app, &alice, team, &carol).await;

    let first = tasks::insert(&app.db, team, bob_id, TaskStatus::Active, "<p>one</p>")
        .await
        .unwrap();
    let second = tasks::insert(&app.db, team, alice_id, TaskStatus::Active, "<p>two</p>")
        .await
        .unwrap();
    let third = tasks::insert(&app.db, team, bob_id, TaskStatus::Active, "<p>three</p>")
        .await
        .unwrap();
    let done = tasks::insert(&app.db, team, bob_id, TaskStatus::Done, "<p>four</p>")
        .await
        .unwrap();
    assert!(first.position < second.position && second.position < third.position);

    // Admin sees every task, in (status, position) order, and can edit all.
    let body = list(&app, &alice, team, "").await;
    assert_eq!(
        ids(&body),
        [first.id, second.id, third.id, done.id].map(|i| i.to_string())
    );
    let as_admin = body.as_array().unwrap();
    assert!(as_admin.iter().all(|t| t["canEdit"] == true));
    let t = &as_admin[0];
    assert_eq!(t["status"], "active");
    assert_eq!(t["position"], first.position);
    assert_eq!(t["contentHtml"], "<p>one</p>");
    assert_eq!(t["author"]["id"], bob_id.to_string());
    assert_eq!(t["author"]["displayName"], "Bob Bianchi");
    assert_eq!(t["mentions"], json!([]));
    assert_eq!(t["meetingId"], Value::Null);
    assert!(t["createdAt"].as_str().is_some());

    // The author can edit their own tasks only.
    let as_bob = list(&app, &bob, team, "").await;
    let can_edit: Vec<bool> = as_bob
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["canEdit"].as_bool().unwrap())
        .collect();
    assert_eq!(can_edit, [true, false, true, true]);

    // A member who wrote nothing can edit nothing.
    let as_carol = list(&app, &carol, team, "").await;
    assert!(
        as_carol
            .as_array()
            .unwrap()
            .iter()
            .all(|t| t["canEdit"] == false)
    );
}

#[tokio::test]
async fn filters_by_mention_author_and_meeting() {
    let app = TestApp::new().await;
    let alice = app.token_for("alice").await;
    let bob = app.token_for("bob").await;
    let (alice_id, bob_id) = (user_id(&app, &alice).await, user_id(&app, &bob).await);
    let team = create_team(&app, &alice, "Platform").await;
    join(&app, &alice, team, &bob).await;

    let plain = tasks::insert(&app.db, team, alice_id, TaskStatus::Future, "<p>plain</p>")
        .await
        .unwrap();
    let mentions_bob = tasks::insert(
        &app.db,
        team,
        alice_id,
        TaskStatus::Active,
        &mention(bob_id, "Bob"),
    )
    .await
    .unwrap();
    let by_bob = tasks::insert(
        &app.db,
        team,
        bob_id,
        TaskStatus::Stuck,
        &mention(alice_id, "Alice"),
    )
    .await
    .unwrap();

    // A meeting id is a plain column until meetings exist.
    let meeting = Uuid::now_v7();
    let mut active: task::ActiveModel = plain.clone().into();
    active.meeting_id = Set(Some(meeting));
    active.update(&app.db).await.unwrap();

    let body = list(&app, &alice, team, "").await;
    assert_eq!(
        ids(&body),
        [plain.id, by_bob.id, mentions_bob.id].map(|i| i.to_string())
    );
    let mentioned_bob = body
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == mentions_bob.id.to_string())
        .unwrap();
    assert_eq!(mentioned_bob["mentions"].as_array().unwrap().len(), 1);
    assert_eq!(mentioned_bob["mentions"][0]["id"], bob_id.to_string());
    assert_eq!(mentioned_bob["mentions"][0]["displayName"], "Bob Bianchi");

    let by_mention = list(&app, &alice, team, &format!("?mentioned={bob_id}")).await;
    assert_eq!(ids(&by_mention), [mentions_bob.id.to_string()]);

    let by_author = list(&app, &alice, team, &format!("?author={bob_id}")).await;
    assert_eq!(ids(&by_author), [by_bob.id.to_string()]);

    let by_meeting = list(&app, &alice, team, &format!("?meeting={meeting}")).await;
    assert_eq!(ids(&by_meeting), [plain.id.to_string()]);
    assert_eq!(by_meeting[0]["meetingId"], meeting.to_string());

    let none = list(&app, &alice, team, &format!("?meeting={}", Uuid::now_v7())).await;
    assert_eq!(none, json!([]));

    // Filters combine with AND.
    let both = list(
        &app,
        &alice,
        team,
        &format!("?mentioned={alice_id}&author={bob_id}"),
    )
    .await;
    assert_eq!(ids(&both), [by_bob.id.to_string()]);
    let neither = list(
        &app,
        &alice,
        team,
        &format!("?mentioned={bob_id}&author={bob_id}"),
    )
    .await;
    assert_eq!(neither, json!([]));
}

#[tokio::test]
async fn insert_ignores_mentions_of_non_members() {
    let app = TestApp::new().await;
    let alice = app.token_for("alice").await;
    let bob = app.token_for("bob").await;
    let (alice_id, bob_id) = (user_id(&app, &alice).await, user_id(&app, &bob).await);
    let team = create_team(&app, &alice, "Platform").await;

    // Bob is not in the team.
    tasks::insert(
        &app.db,
        team,
        alice_id,
        TaskStatus::Active,
        &mention(bob_id, "Bob"),
    )
    .await
    .unwrap();

    let body = list(&app, &alice, team, "").await;
    assert_eq!(body[0]["mentions"], json!([]));
    let by_mention = list(&app, &alice, team, &format!("?mentioned={bob_id}")).await;
    assert_eq!(by_mention, json!([]));
}

#[tokio::test]
async fn non_members_get_404() {
    let app = TestApp::new().await;
    let alice = app.token_for("alice").await;
    let bob = app.token_for("bob").await;
    let team = create_team(&app, &alice, "Platform").await;

    let res = app
        .call("GET", &format!("/api/v1/teams/{team}/tasks"), &bob, None)
        .await;
    assert_eq!(res.status, 404);
    assert_eq!(res.json()["code"], "team_not_found");

    let res = app
        .call(
            "GET",
            &format!("/api/v1/teams/{}/tasks", Uuid::now_v7()),
            &alice,
            None,
        )
        .await;
    assert_eq!(res.status, 404);
    assert_eq!(res.json()["code"], "team_not_found");
}

#[tokio::test]
async fn deleting_the_team_deletes_its_tasks() {
    let app = TestApp::new().await;
    let alice = app.token_for("alice").await;
    let alice_id = user_id(&app, &alice).await;
    let team = create_team(&app, &alice, "Platform").await;
    tasks::insert(
        &app.db,
        team,
        alice_id,
        TaskStatus::Active,
        &mention(alice_id, "Alice"),
    )
    .await
    .unwrap();
    assert_eq!(task::Entity::find().count(&app.db).await.unwrap(), 1);

    let res = app
        .call("DELETE", &format!("/api/v1/teams/{team}"), &alice, None)
        .await;
    assert_eq!(res.status, 204);

    assert_eq!(task::Entity::find().count(&app.db).await.unwrap(), 0);
    assert_eq!(
        retro_backend::entities::task_mention::Entity::find()
            .count(&app.db)
            .await
            .unwrap(),
        0
    );
}
