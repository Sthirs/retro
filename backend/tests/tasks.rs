//! The tasks board (spec §9, §13, §14).

mod common;

use common::TestApp;
use retro_backend::{
    domain::tasks,
    entities::{task, task::TaskStatus, task_mention},
};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, Set};
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

async fn start_meeting(app: &TestApp, token: &str, team: Uuid) -> Uuid {
    let res = app
        .call(
            "POST",
            &format!("/api/v1/teams/{team}/meetings"),
            token,
            None,
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

    // Every task, in (status, position) order.
    let body = list(&app, &alice, team, "").await;
    assert_eq!(
        ids(&body),
        [first.id, second.id, third.id, done.id].map(|i| i.to_string())
    );
    let t = &body.as_array().unwrap()[0];
    assert_eq!(t["status"], "active");
    assert_eq!(t["position"], first.position);
    assert_eq!(t["contentHtml"], "<p>one</p>");
    assert_eq!(t["author"]["id"], bob_id.to_string());
    assert_eq!(t["author"]["displayName"], "Bob Bianchi");
    assert_eq!(t["mentions"], json!([]));
    assert_eq!(t["meetingId"], Value::Null);
    assert!(t["createdAt"].as_str().is_some());

    // A plain member who wrote nothing sees the same board.
    let as_carol = list(&app, &carol, team, "").await;
    assert_eq!(ids(&as_carol), ids(&body));
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

    let meeting = start_meeting(&app, &alice, team).await;
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
async fn mentioned_filter_matches_any_of_several_users() {
    let app = TestApp::new().await;
    let alice = app.token_for("alice").await;
    let bob = app.token_for("bob").await;
    let (alice_id, bob_id) = (user_id(&app, &alice).await, user_id(&app, &bob).await);
    let team = create_team(&app, &alice, "Platform").await;
    join(&app, &alice, team, &bob).await;

    tasks::insert(&app.db, team, alice_id, TaskStatus::Future, "<p>plain</p>")
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
    let mentions_alice = tasks::insert(
        &app.db,
        team,
        bob_id,
        TaskStatus::Stuck,
        &mention(alice_id, "Alice"),
    )
    .await
    .unwrap();
    let mentions_both = tasks::insert(
        &app.db,
        team,
        bob_id,
        TaskStatus::Done,
        &format!(
            r#"<p><span data-type="mention" data-id="{alice_id}" data-label="Alice">@Alice</span> <span data-type="mention" data-id="{bob_id}" data-label="Bob">@Bob</span></p>"#
        ),
    )
    .await
    .unwrap();

    // Any of the users matches, and a task mentioning both appears once.
    let either = list(
        &app,
        &alice,
        team,
        &format!("?mentioned={alice_id},{bob_id}"),
    )
    .await;
    let mut got = ids(&either);
    got.sort();
    let mut want = [mentions_bob.id, mentions_alice.id, mentions_both.id].map(|i| i.to_string());
    want.sort();
    assert_eq!(got, want);

    // Still combined with the other filters by AND.
    let by_alice = list(
        &app,
        &alice,
        team,
        &format!("?mentioned={alice_id},{bob_id}&author={alice_id}"),
    )
    .await;
    assert_eq!(ids(&by_alice), [mentions_bob.id.to_string()]);

    // An invalid id is refused.
    let res = app
        .call(
            "GET",
            &format!("/api/v1/teams/{team}/tasks?mentioned={alice_id},nope"),
            &alice,
            None,
        )
        .await;
    assert_eq!(res.status, 400, "{}", res.text());
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

/// Alice (admin) and Bob (member) share a team with tasks `a`, `b`, `c` in
/// the active column and `d` in the done column, all written by Bob.
struct Board {
    app: TestApp,
    alice: String,
    bob: String,
    team: Uuid,
    a: task::Model,
    b: task::Model,
    c: task::Model,
    d: task::Model,
}

async fn board() -> Board {
    let app = TestApp::new().await;
    let alice = app.token_for("alice").await;
    let bob = app.token_for("bob").await;
    let bob_id = user_id(&app, &bob).await;
    let team = create_team(&app, &alice, "Platform").await;
    join(&app, &alice, team, &bob).await;
    let mut tasks_in = Vec::new();
    for (status, text) in [
        (TaskStatus::Active, "a"),
        (TaskStatus::Active, "b"),
        (TaskStatus::Active, "c"),
        (TaskStatus::Done, "d"),
    ] {
        tasks_in.push(
            tasks::insert(&app.db, team, bob_id, status, &format!("<p>{text}</p>"))
                .await
                .unwrap(),
        );
    }
    let mut it = tasks_in.into_iter();
    let (a, b, c, d) = (
        it.next().unwrap(),
        it.next().unwrap(),
        it.next().unwrap(),
        it.next().unwrap(),
    );
    Board {
        app,
        alice,
        bob,
        team,
        a,
        b,
        c,
        d,
    }
}

async fn move_to(
    app: &TestApp,
    token: &str,
    task: Uuid,
    status: &str,
    after: Option<Uuid>,
    before: Option<Uuid>,
) -> common::TestResponse {
    app.call(
        "PUT",
        &format!("/api/v1/tasks/{task}/position"),
        token,
        Some(json!({ "status": status, "afterId": after, "beforeId": before })),
    )
    .await
}

fn column(list: &Value, status: &str) -> Vec<String> {
    list.as_array()
        .unwrap()
        .iter()
        .filter(|t| t["status"] == status)
        .map(|t| t["id"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn moves_to_the_end_of_another_column() {
    let b = board().await;
    let res = move_to(&b.app, &b.alice, b.a.id, "done", None, None).await;
    assert_eq!(res.status, 200, "{}", res.text());
    let body = res.json();
    assert_eq!(body["id"], b.a.id.to_string());
    assert_eq!(body["status"], "done");
    assert!(body["position"].as_str().unwrap() > b.d.position.as_str());
    assert_eq!(body["author"]["displayName"], "Bob Bianchi");

    let all = list(&b.app, &b.alice, b.team, "").await;
    assert_eq!(
        column(&all, "done"),
        [b.d.id, b.a.id].map(|i| i.to_string())
    );
    assert_eq!(
        column(&all, "active"),
        [b.b.id, b.c.id].map(|i| i.to_string())
    );

    // Within a column, "no neighbours" sends the task to the end too.
    let res = move_to(&b.app, &b.alice, b.b.id, "active", None, None).await;
    assert_eq!(res.status, 200, "{}", res.text());
    let all = list(&b.app, &b.alice, b.team, "").await;
    assert_eq!(
        column(&all, "active"),
        [b.c.id, b.b.id].map(|i| i.to_string())
    );

    // An empty column takes the task as its only one.
    let res = move_to(&b.app, &b.alice, b.c.id, "stuck", None, None).await;
    assert_eq!(res.status, 200, "{}", res.text());
    assert_eq!(res.json()["status"], "stuck");
}

#[tokio::test]
async fn moves_between_neighbours() {
    let b = board().await;
    // d (done) goes between a and b of the active column.
    let res = move_to(
        &b.app,
        &b.alice,
        b.d.id,
        "active",
        Some(b.a.id),
        Some(b.b.id),
    )
    .await;
    assert_eq!(res.status, 200, "{}", res.text());
    let position = res.json()["position"].as_str().unwrap().to_owned();
    assert!(b.a.position < position && position < b.b.position);

    let all = list(&b.app, &b.alice, b.team, "").await;
    assert_eq!(
        column(&all, "active"),
        [b.a.id, b.d.id, b.b.id, b.c.id].map(|i| i.to_string())
    );
    assert!(column(&all, "done").is_empty());
}

#[tokio::test]
async fn moves_between_non_adjacent_neighbours() {
    let b = board().await;
    // The neighbours are not adjacent: the task lands between them anyway.
    let res = move_to(
        &b.app,
        &b.alice,
        b.d.id,
        "active",
        Some(b.a.id),
        Some(b.c.id),
    )
    .await;
    assert_eq!(res.status, 200, "{}", res.text());
    let position = res.json()["position"].as_str().unwrap().to_owned();
    assert!(b.a.position < position && position < b.c.position);
    let all = list(&b.app, &b.alice, b.team, "").await;
    assert_eq!(
        column(&all, "active"),
        [b.a.id, b.d.id, b.b.id, b.c.id].map(|i| i.to_string())
    );

    // A task can also move within its own column, relative to others.
    let res = move_to(&b.app, &b.alice, b.c.id, "active", None, Some(b.a.id)).await;
    assert_eq!(res.status, 200, "{}", res.text());
    let all = list(&b.app, &b.alice, b.team, "").await;
    assert_eq!(
        column(&all, "active"),
        [b.c.id, b.a.id, b.d.id, b.b.id].map(|i| i.to_string())
    );
}

#[tokio::test]
async fn rejects_foreign_neighbours() {
    let b = board().await;
    let carol = b
        .app
        .create_user("carol", "carol@example.com", "Carol")
        .await;
    let carol_id = user_id(&b.app, &carol).await;
    let other_team = create_team(&b.app, &carol, "Other").await;
    let foreign = tasks::insert(
        &b.app.db,
        other_team,
        carol_id,
        TaskStatus::Active,
        "<p>x</p>",
    )
    .await
    .unwrap();

    // A task from another team.
    for (after, before) in [(Some(foreign.id), None), (None, Some(foreign.id))] {
        let res = move_to(&b.app, &b.alice, b.d.id, "active", after, before).await;
        assert_eq!(res.status, 400, "{}", res.text());
        assert_eq!(res.json()["code"], "invalid_position");
    }
    // A task from another column than the target one.
    let res = move_to(&b.app, &b.alice, b.a.id, "active", Some(b.d.id), None).await;
    assert_eq!(res.status, 400, "{}", res.text());
    assert_eq!(res.json()["code"], "invalid_position");
    // The task itself, or a task that does not exist.
    let res = move_to(&b.app, &b.alice, b.a.id, "active", Some(b.a.id), None).await;
    assert_eq!(res.status, 400, "{}", res.text());
    assert_eq!(res.json()["code"], "invalid_position");
    let res = move_to(
        &b.app,
        &b.alice,
        b.a.id,
        "active",
        None,
        Some(Uuid::now_v7()),
    )
    .await;
    assert_eq!(res.status, 400, "{}", res.text());
    assert_eq!(res.json()["code"], "invalid_position");

    // Nothing moved.
    let all = list(&b.app, &b.alice, b.team, "").await;
    assert_eq!(
        column(&all, "active"),
        [b.a.id, b.b.id, b.c.id].map(|i| i.to_string())
    );
}

#[tokio::test]
async fn stale_neighbours_conflict() {
    let b = board().await;
    // The neighbours changed order under the client: after is past before.
    let res = move_to(
        &b.app,
        &b.alice,
        b.d.id,
        "active",
        Some(b.c.id),
        Some(b.a.id),
    )
    .await;
    assert_eq!(res.status, 409, "{}", res.text());
    assert_eq!(res.json()["code"], "stale_position");
    let res = move_to(
        &b.app,
        &b.alice,
        b.d.id,
        "active",
        Some(b.a.id),
        Some(b.a.id),
    )
    .await;
    assert_eq!(res.status, 409, "{}", res.text());
    assert_eq!(res.json()["code"], "stale_position");

    let stored = task::Entity::find_by_id(b.d.id)
        .one(&b.app.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.status, TaskStatus::Done);
    assert_eq!(stored.position, b.d.position);
}

#[tokio::test]
async fn identical_drops_both_land_after_the_neighbour() {
    let b = board().await;
    let carol = b
        .app
        .create_user("carol", "carol@example.com", "Carol")
        .await;
    join(&b.app, &b.alice, b.team, &carol).await;
    let extra = tasks::insert(
        &b.app.db,
        b.team,
        user_id(&b.app, &b.bob).await,
        TaskStatus::Future,
        "<p>e</p>",
    )
    .await
    .unwrap();

    // Two clients drop different tasks between the same neighbours: both
    // succeed, the second landing directly after the neighbour, above the first.
    let first = move_to(
        &b.app,
        &b.alice,
        b.d.id,
        "active",
        Some(b.a.id),
        Some(b.b.id),
    )
    .await;
    assert_eq!(first.status, 200, "{}", first.text());
    let second = move_to(
        &b.app,
        &carol,
        extra.id,
        "active",
        Some(b.a.id),
        Some(b.b.id),
    )
    .await;
    assert_eq!(second.status, 200, "{}", second.text());

    let all = list(&b.app, &b.alice, b.team, "").await;
    assert_eq!(
        column(&all, "active"),
        [b.a.id, extra.id, b.d.id, b.b.id, b.c.id].map(|i| i.to_string())
    );
}

#[tokio::test]
async fn before_only_lands_directly_before() {
    let b = board().await;
    let res = move_to(&b.app, &b.alice, b.c.id, "active", None, Some(b.b.id)).await;
    assert_eq!(res.status, 200, "{}", res.text());
    let all = list(&b.app, &b.alice, b.team, "").await;
    assert_eq!(
        column(&all, "active"),
        [b.a.id, b.c.id, b.b.id].map(|i| i.to_string())
    );
}

#[tokio::test]
async fn after_only_lands_directly_after() {
    let b = board().await;
    let res = move_to(&b.app, &b.alice, b.d.id, "active", Some(b.a.id), None).await;
    assert_eq!(res.status, 200, "{}", res.text());
    let all = list(&b.app, &b.alice, b.team, "").await;
    assert_eq!(
        column(&all, "active"),
        [b.a.id, b.d.id, b.b.id, b.c.id].map(|i| i.to_string())
    );
}

#[tokio::test]
async fn moves_past_hidden_tasks() {
    let b = board().await;
    // The client sees a and c only (b is hidden by a filter) and drops d
    // between them: it lands directly after a, never skipping or colliding
    // with the hidden b.
    let res = move_to(
        &b.app,
        &b.alice,
        b.d.id,
        "active",
        Some(b.a.id),
        Some(b.c.id),
    )
    .await;
    assert_eq!(res.status, 200, "{}", res.text());
    let all = list(&b.app, &b.alice, b.team, "").await;
    assert_eq!(
        column(&all, "active"),
        [b.a.id, b.d.id, b.b.id, b.c.id].map(|i| i.to_string())
    );
}

#[tokio::test]
async fn any_member_can_move() {
    let b = board().await;
    // Alice wrote nothing, Bob wrote everything; the admin moves Bob's task
    // and a plain member moves a task of someone else just as well.
    let carol = b
        .app
        .create_user("carol", "carol@example.com", "Carol")
        .await;
    join(&b.app, &b.alice, b.team, &carol).await;
    let res = move_to(&b.app, &carol, b.a.id, "stuck", None, None).await;
    assert_eq!(res.status, 200, "{}", res.text());
    let res = move_to(&b.app, &b.alice, b.b.id, "stuck", Some(b.a.id), None).await;
    assert_eq!(res.status, 200, "{}", res.text());
    let res = move_to(&b.app, &b.bob, b.c.id, "future", None, None).await;
    assert_eq!(res.status, 200, "{}", res.text());
}

#[tokio::test]
async fn strangers_get_404() {
    let b = board().await;
    let carol = b
        .app
        .create_user("carol", "carol@example.com", "Carol")
        .await;
    let res = move_to(&b.app, &carol, b.a.id, "done", None, None).await;
    assert_eq!(res.status, 404, "{}", res.text());
    assert_eq!(res.json()["code"], "task_not_found");
    let res = move_to(&b.app, &b.alice, Uuid::now_v7(), "done", None, None).await;
    assert_eq!(res.status, 404, "{}", res.text());
    assert_eq!(res.json()["code"], "task_not_found");

    let stored = task::Entity::find_by_id(b.a.id)
        .one(&b.app.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.status, TaskStatus::Active);
}

async fn edit(app: &TestApp, token: &str, task: Uuid, content: &str) -> common::TestResponse {
    app.call(
        "PATCH",
        &format!("/api/v1/tasks/{task}"),
        token,
        Some(json!({ "content": content })),
    )
    .await
}

async fn delete(app: &TestApp, token: &str, task: Uuid) -> common::TestResponse {
    app.call("DELETE", &format!("/api/v1/tasks/{task}"), token, None)
        .await
}

#[tokio::test]
async fn any_member_can_edit() {
    let b = board().await;
    let carol = b
        .app
        .create_user("carol", "carol@example.com", "Carol")
        .await;
    join(&b.app, &b.alice, b.team, &carol).await;

    // Bob wrote the task.
    let res = edit(&b.app, &b.bob, b.a.id, "<p>by author</p>").await;
    assert_eq!(res.status, 200, "{}", res.text());
    assert_eq!(res.json()["contentHtml"], "<p>by author</p>");
    // Alice is the admin.
    let res = edit(&b.app, &b.alice, b.a.id, "<p>by admin</p>").await;
    assert_eq!(res.status, 200, "{}", res.text());
    assert_eq!(res.json()["contentHtml"], "<p>by admin</p>");
    // Carol is a plain member who did not write it.
    let res = edit(&b.app, &carol, b.a.id, "<p>by carol</p>").await;
    assert_eq!(res.status, 200, "{}", res.text());
    assert_eq!(res.json()["contentHtml"], "<p>by carol</p>");
    assert_eq!(res.json()["author"]["displayName"], "Bob Bianchi");

    let stored = task::Entity::find_by_id(b.a.id)
        .one(&b.app.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.content, "<p>by carol</p>");
    assert_eq!(stored.author_id, b.a.author_id);
}

#[tokio::test]
async fn edit_of_unknown_or_foreign_task_is_404() {
    let b = board().await;
    let carol = b
        .app
        .create_user("carol", "carol@example.com", "Carol")
        .await;
    let res = edit(&b.app, &carol, b.a.id, "<p>x</p>").await;
    assert_eq!(res.status, 404, "{}", res.text());
    assert_eq!(res.json()["code"], "task_not_found");
    let res = edit(&b.app, &b.alice, Uuid::now_v7(), "<p>x</p>").await;
    assert_eq!(res.status, 404, "{}", res.text());
    assert_eq!(res.json()["code"], "task_not_found");
}

#[tokio::test]
async fn edit_sanitizes_content() {
    let b = board().await;
    let res = edit(
        &b.app,
        &b.bob,
        b.a.id,
        r#"<p onclick="x()">hi<script>alert(1)</script> <strong>there</strong></p>"#,
    )
    .await;
    assert_eq!(res.status, 200, "{}", res.text());
    let html = res.json()["contentHtml"].as_str().unwrap().to_owned();
    assert!(!html.contains("script"), "{html}");
    assert!(!html.contains("onclick"), "{html}");
    assert!(html.contains("<strong>there</strong>"), "{html}");
}

#[tokio::test]
async fn edit_keeps_only_member_mentions() {
    let b = board().await;
    let alice_id = user_id(&b.app, &b.alice).await;
    let dave = b.app.create_user("dave", "dave@example.com", "Dave").await;
    let dave_id = user_id(&b.app, &dave).await;

    let res = edit(&b.app, &b.bob, b.a.id, &mention(alice_id, "Alice")).await;
    assert_eq!(res.status, 200, "{}", res.text());
    let mentions = res.json()["mentions"].clone();
    assert_eq!(mentions.as_array().unwrap().len(), 1);
    assert_eq!(mentions[0]["id"], alice_id.to_string());

    // Dave is a stranger to the team; the previous mention is replaced.
    let res = edit(&b.app, &b.bob, b.a.id, &mention(dave_id, "Dave")).await;
    assert_eq!(res.status, 200, "{}", res.text());
    assert_eq!(res.json()["mentions"], json!([]));
    let by_dave = list(&b.app, &b.bob, b.team, &format!("?mentioned={dave_id}")).await;
    assert_eq!(by_dave, json!([]));
    let by_alice = list(&b.app, &b.bob, b.team, &format!("?mentioned={alice_id}")).await;
    assert_eq!(by_alice, json!([]));
}

#[tokio::test]
async fn blank_content_is_rejected() {
    let b = board().await;
    for content in ["", "<p></p>", "   "] {
        let res = edit(&b.app, &b.bob, b.a.id, content).await;
        assert_eq!(res.status, 400, "{content:?}: {}", res.text());
        assert_eq!(res.json()["code"], "invalid_task_content");
    }
    let stored = task::Entity::find_by_id(b.a.id)
        .one(&b.app.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.content, "<p>a</p>");
}

#[tokio::test]
async fn any_member_can_delete() {
    let b = board().await;
    let carol = b
        .app
        .create_user("carol", "carol@example.com", "Carol")
        .await;
    join(&b.app, &b.alice, b.team, &carol).await;

    // Carol, a plain member, deletes a task Bob wrote; Bob and the admin
    // delete theirs as well.
    let res = delete(&b.app, &carol, b.a.id).await;
    assert_eq!(res.status, 204, "{}", res.text());
    let res = delete(&b.app, &b.bob, b.b.id).await;
    assert_eq!(res.status, 204, "{}", res.text());
    let res = delete(&b.app, &b.alice, b.c.id).await;
    assert_eq!(res.status, 204, "{}", res.text());

    let remaining = list(&b.app, &b.alice, b.team, "").await;
    assert_eq!(ids(&remaining), [b.d.id].map(|i| i.to_string()));

    let res = delete(&b.app, &b.alice, b.a.id).await;
    assert_eq!(res.status, 404, "{}", res.text());
    assert_eq!(res.json()["code"], "task_not_found");
}

#[tokio::test]
async fn delete_by_non_member_is_404() {
    let b = board().await;
    let carol = b
        .app
        .create_user("carol", "carol@example.com", "Carol")
        .await;
    let res = delete(&b.app, &carol, b.a.id).await;
    assert_eq!(res.status, 404, "{}", res.text());
    assert_eq!(res.json()["code"], "task_not_found");
}

#[tokio::test]
async fn delete_removes_mentions() {
    let b = board().await;
    let alice_id = user_id(&b.app, &b.alice).await;
    let res = edit(&b.app, &b.bob, b.a.id, &mention(alice_id, "Alice")).await;
    assert_eq!(res.status, 200, "{}", res.text());

    let res = delete(&b.app, &b.bob, b.a.id).await;
    assert_eq!(res.status, 204, "{}", res.text());

    let left = task_mention::Entity::find()
        .filter(task_mention::Column::TaskId.eq(b.a.id))
        .count(&b.app.db)
        .await
        .unwrap();
    assert_eq!(left, 0);
    let by_alice = list(&b.app, &b.alice, b.team, &format!("?mentioned={alice_id}")).await;
    assert_eq!(by_alice, json!([]));
}

async fn seed(app: &TestApp, token: &str, team: Uuid, body: Value) -> common::TestResponse {
    app.call(
        "POST",
        &format!("/api/v1/dev/teams/{team}/tasks"),
        token,
        Some(body),
    )
    .await
}

#[tokio::test]
async fn dev_route_seeds_tasks_when_enabled() {
    let app = TestApp::new().await;
    let alice = app.token_for("alice").await;
    let bob = app.token_for("bob").await;
    let team = create_team(&app, &alice, "Core").await;
    join(&app, &alice, team, &bob).await;
    let bob_id = user_id(&app, &bob).await;

    let res = seed(
        &app,
        &alice,
        team,
        json!({ "status": "future", "content": "<p>first</p>" }),
    )
    .await;
    assert_eq!(res.status, 201, "{}", res.text());
    let first = res.json();
    assert_eq!(first["status"], "future");
    assert_eq!(first["contentHtml"], "<p>first</p>");
    assert_eq!(first["author"]["displayName"], "Alice Rossi");

    let res = seed(
        &app,
        &alice,
        team,
        json!({ "status": "future", "content": "<p>second</p>", "authorId": bob_id }),
    )
    .await;
    assert_eq!(res.status, 201, "{}", res.text());
    let second = res.json();
    assert_eq!(second["author"]["id"], bob_id.to_string());
    assert!(second["position"].as_str() > first["position"].as_str());

    let meeting = start_meeting(&app, &alice, team).await;
    let res = seed(
        &app,
        &alice,
        team,
        json!({ "status": "done", "content": "<p>third</p>", "meetingId": meeting }),
    )
    .await;
    assert_eq!(res.status, 201, "{}", res.text());
    let third = res.json();
    assert_eq!(third["meetingId"], meeting.to_string());

    let board = list(&app, &alice, team, "").await;
    assert_eq!(
        ids(&board),
        [&first, &second, &third].map(|t| t["id"].as_str().unwrap().to_owned())
    );
}

#[tokio::test]
async fn dev_route_rejects_non_member_caller_and_author() {
    let app = TestApp::new().await;
    let alice = app.token_for("alice").await;
    let bob = app.token_for("bob").await;
    let team = create_team(&app, &alice, "Core").await;
    let body = json!({ "status": "future", "content": "<p>x</p>" });

    let res = seed(&app, &bob, team, body.clone()).await;
    assert_eq!(res.status, 404, "{}", res.text());
    assert_eq!(res.json()["code"], "team_not_found");

    let bob_id = user_id(&app, &bob).await;
    let res = seed(
        &app,
        &alice,
        team,
        json!({ "status": "future", "content": "<p>x</p>", "authorId": bob_id }),
    )
    .await;
    assert_eq!(res.status, 400, "{}", res.text());
    assert_eq!(res.json()["code"], "invalid_author");

    let other_team = create_team(&app, &alice, "Other").await;
    let elsewhere = start_meeting(&app, &alice, other_team).await;
    for meeting in [Uuid::now_v7(), elsewhere] {
        let res = seed(
            &app,
            &alice,
            team,
            json!({ "status": "future", "content": "<p>x</p>", "meetingId": meeting }),
        )
        .await;
        assert_eq!(res.status, 400, "{}", res.text());
        assert_eq!(res.json()["code"], "invalid_meeting");
    }
    assert_eq!(list(&app, &alice, team, "").await, json!([]));
}

#[tokio::test]
async fn dev_route_is_absent_when_disabled() {
    let app = TestApp::with_config(|c| c.seed_dev_users = false).await;
    let alice = app.create_user("alice", "alice@example.com", "Alice").await;
    let team = create_team(&app, &alice, "Core").await;

    let res = seed(
        &app,
        &alice,
        team,
        json!({ "status": "future", "content": "<p>x</p>" }),
    )
    .await;
    assert_eq!(res.status, 404, "{}", res.text());
}
