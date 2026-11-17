//! Group phase over REST: making, joining, leaving, dissolving and naming
//! groups (spec §9.3, §12, §14.4).

mod common;

use common::{TestApp, TestResponse};
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

/// Adds `token`'s user to the team through its join link.
async fn join_team(app: &TestApp, admin: &str, team: Uuid, token: &str) {
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

async fn post(app: &TestApp, token: &str, path: &str, body: Option<Value>) -> TestResponse {
    app.call("POST", path, token, body).await
}

async fn state(app: &TestApp, token: &str, meeting: &str) -> Value {
    let res = app
        .call(
            "GET",
            &format!("/api/v1/meetings/{meeting}/state"),
            token,
            None,
        )
        .await;
    assert_eq!(res.status, 200, "{}", res.text());
    res.json()
}

async fn next_phase(app: &TestApp, token: &str, meeting: &str) {
    let res = post(
        app,
        token,
        &format!("/api/v1/meetings/{meeting}/phase/next"),
        None,
    )
    .await;
    assert_eq!(res.status, 200, "{}", res.text());
}

/// Sends a reflection and returns its id.
async fn sent(app: &TestApp, token: &str, meeting: &str, column: &str, text: &str) -> String {
    let res = post(
        app,
        token,
        &format!("/api/v1/meetings/{meeting}/reflections"),
        Some(json!({ "column": column, "content": format!("<p>{text}</p>") })),
    )
    .await;
    assert_eq!(res.status, 201, "{}", res.text());
    let reflections = res.json()["reflections"].as_array().unwrap().clone();
    reflections.last().unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// alice (admin, facilitator) and bob (participant) in a meeting where
/// they wrote cards; carol is a member who has not joined.
struct Room {
    alice: String,
    bob: String,
    carol: String,
    team: Uuid,
    meeting: String,
    /// plus: p1, p2 (alice); delta: d1 (bob), d2 (alice); ideas: i1 (bob).
    p1: String,
    p2: String,
    d1: String,
    d2: String,
    i1: String,
}

/// The room, still in Reflect.
async fn reflecting(app: &TestApp) -> Room {
    reflecting_with(app, "carol").await
}

/// The room, still in Reflect, with `carol` as the member who did not join.
async fn reflecting_with(app: &TestApp, carol: &str) -> Room {
    let alice = app.token_for("alice").await;
    let bob = app.token_for("bob").await;
    let carol = app
        .create_user(carol, &format!("{carol}@example.com"), "Carol Verdi")
        .await;
    let team = create_team(app, &alice, "Platform").await;
    join_team(app, &alice, team, &bob).await;
    join_team(app, &alice, team, &carol).await;
    let res = post(app, &alice, &format!("/api/v1/teams/{team}/meetings"), None).await;
    assert_eq!(res.status, 201, "{}", res.text());
    let meeting = res.json()["id"].as_str().unwrap().to_owned();
    let res = post(app, &bob, &format!("/api/v1/meetings/{meeting}/join"), None).await;
    assert_eq!(res.status, 200, "{}", res.text());
    next_phase(app, &alice, &meeting).await;
    let p1 = sent(app, &alice, &meeting, "plus", "Release train").await;
    let d1 = sent(app, &bob, &meeting, "delta", "Flaky tests").await;
    let p2 = sent(app, &alice, &meeting, "plus", "Pairing").await;
    let i1 = sent(app, &bob, &meeting, "ideas", "Quarantine").await;
    let d2 = sent(app, &alice, &meeting, "delta", "Slow CI").await;
    Room {
        alice,
        bob,
        carol,
        team,
        meeting,
        p1,
        p2,
        d1,
        d2,
        i1,
    }
}

/// The room, in Group.
async fn room(app: &TestApp) -> Room {
    let r = reflecting(app).await;
    next_phase(app, &r.alice, &r.meeting).await;
    r
}

async fn drop_on(app: &TestApp, token: &str, r: &Room, card: &str, target: &str) -> TestResponse {
    post(
        app,
        token,
        &format!("/api/v1/meetings/{}/groups", r.meeting),
        Some(json!({ "targetId": target, "reflectionId": card })),
    )
    .await
}

async fn move_to(app: &TestApp, token: &str, card: &str, group: Option<&str>) -> TestResponse {
    app.call(
        "PUT",
        &format!("/api/v1/reflections/{card}/group"),
        token,
        Some(json!({ "groupId": group })),
    )
    .await
}

async fn rename(app: &TestApp, token: &str, group: &str, name: &str) -> TestResponse {
    app.call(
        "PATCH",
        &format!("/api/v1/groups/{group}"),
        token,
        Some(json!({ "name": name })),
    )
    .await
}

fn card<'a>(state: &'a Value, id: &str) -> &'a Value {
    state["reflections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == id)
        .unwrap()
}

fn group_of(state: &Value, id: &str) -> Option<String> {
    card(state, id)["groupId"].as_str().map(str::to_owned)
}

fn groups(state: &Value) -> Vec<Value> {
    state["groups"].as_array().unwrap().clone()
}

fn ok(res: TestResponse) -> Value {
    assert_eq!(res.status, 200, "{}", res.text());
    res.json()
}

fn code(res: &TestResponse) -> String {
    res.json()["code"].as_str().unwrap_or_default().to_owned()
}

#[tokio::test]
async fn cards_take_their_place_in_their_column_when_group_starts() {
    let app = TestApp::new().await;
    let r = reflecting(&app).await;
    let before = state(&app, &r.alice, &r.meeting).await;
    assert_eq!(before["groups"], json!([]));
    next_phase(&app, &r.alice, &r.meeting).await;

    let seen = state(&app, &r.bob, &r.meeting).await;
    let place = |id: &str| card(&seen, id)["position"].as_i64().unwrap();
    assert_eq!((place(&r.p1), place(&r.p2)), (0, 1));
    assert_eq!((place(&r.d1), place(&r.d2)), (0, 1));
    assert_eq!(place(&r.i1), 0);
    for id in [&r.p1, &r.p2, &r.d1, &r.d2, &r.i1] {
        assert_eq!(card(&seen, id)["groupId"], Value::Null);
        assert_eq!(card(&seen, id)["version"], 0);
    }
    assert_eq!(seen["groups"], json!([]));
    assert_eq!(seen["drags"], json!([]));
}

#[tokio::test]
async fn dropping_a_card_on_a_card_groups_both_at_the_target_place() {
    let app = TestApp::new().await;
    let r = room(&app).await;
    // An idea dropped on the second delta card.
    let seen = ok(drop_on(&app, &r.bob, &r, &r.i1, &r.d2).await);

    let all = groups(&seen);
    assert_eq!(all.len(), 1);
    let group = &all[0];
    assert_eq!(group["column"], "delta");
    assert_eq!(group["position"], 1);
    assert_eq!(group["name"], "");
    let id = group["id"].as_str().unwrap();
    assert_eq!(group_of(&seen, &r.i1).as_deref(), Some(id));
    assert_eq!(group_of(&seen, &r.d2).as_deref(), Some(id));
    // Cards keep their own column and place.
    assert_eq!(card(&seen, &r.i1)["column"], "ideas");
    assert_eq!(card(&seen, &r.i1)["version"], 1);
    assert_eq!(card(&seen, &r.d2)["version"], 1);
    // Everyone sees the same, still anonymously.
    let theirs = state(&app, &r.alice, &r.meeting).await;
    assert_eq!(theirs["groups"], seen["groups"]);
    let bob_id = app.get_with_token("/api/v1/me", &r.bob).await.json()["user"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(!theirs["reflections"].to_string().contains(&bob_id));
}

#[tokio::test]
async fn cards_join_and_leave_groups_and_a_group_of_one_dissolves() {
    let app = TestApp::new().await;
    let r = room(&app).await;
    let seen = ok(drop_on(&app, &r.alice, &r, &r.d1, &r.d2).await);
    let group = groups(&seen)[0]["id"].as_str().unwrap().to_owned();

    // Onto the group, and onto a grouped card: both join it.
    let seen = ok(move_to(&app, &r.bob, &r.p1, Some(&group)).await);
    assert_eq!(group_of(&seen, &r.p1).as_deref(), Some(group.as_str()));
    let seen = ok(drop_on(&app, &r.bob, &r, &r.i1, &r.d1).await);
    assert_eq!(group_of(&seen, &r.i1).as_deref(), Some(group.as_str()));
    assert_eq!(groups(&seen).len(), 1);

    // Out again, one by one, until a single card is left.
    for id in [&r.p1, &r.i1, &r.d1] {
        let seen = ok(move_to(&app, &r.alice, id, None).await);
        assert_eq!(group_of(&seen, id), None);
    }
    let seen = state(&app, &r.bob, &r.meeting).await;
    assert_eq!(seen["groups"], json!([]));
    assert_eq!(group_of(&seen, &r.d2), None);
    // Each move raised the moved card's version.
    assert_eq!(card(&seen, &r.d1)["version"], 2);
    assert_eq!(card(&seen, &r.d2)["version"], 2);
    // Moving where it already is changes nothing.
    let seen = ok(move_to(&app, &r.alice, &r.d1, None).await);
    assert_eq!(card(&seen, &r.d1)["version"], 2);
}

#[tokio::test]
async fn a_card_moving_between_groups_dissolves_the_one_it_leaves() {
    let app = TestApp::new().await;
    let r = room(&app).await;
    ok(drop_on(&app, &r.alice, &r, &r.p2, &r.p1).await);
    let seen = ok(drop_on(&app, &r.alice, &r, &r.i1, &r.d1).await);
    assert_eq!(groups(&seen).len(), 2);

    // p2 leaves the plus group (now one card) for a new group with d2.
    let seen = ok(drop_on(&app, &r.bob, &r, &r.p2, &r.d2).await);
    let all = groups(&seen);
    assert_eq!(all.len(), 2);
    assert_eq!(group_of(&seen, &r.p1), None);
    let with_d2 = group_of(&seen, &r.d2).unwrap();
    assert_eq!(group_of(&seen, &r.p2), Some(with_d2));
}

#[tokio::test]
async fn the_last_move_wins_when_the_target_was_grouped_meanwhile() {
    let app = TestApp::new().await;
    let r = room(&app).await;
    let seen = ok(drop_on(&app, &r.alice, &r, &r.d1, &r.d2).await);
    let group = groups(&seen)[0]["id"].as_str().unwrap().to_owned();
    // Bob still sees d2 alone and drops p1 onto it: p1 joins d2's group.
    let seen = ok(drop_on(&app, &r.bob, &r, &r.p1, &r.d2).await);
    assert_eq!(groups(&seen).len(), 1);
    assert_eq!(group_of(&seen, &r.p1), Some(group.clone()));
    // A move into a group that dissolved meanwhile fails.
    for id in [&r.p1, &r.d1] {
        ok(move_to(&app, &r.alice, id, None).await);
    }
    let res = move_to(&app, &r.bob, &r.i1, Some(&group)).await;
    assert_eq!(res.status, 404);
    assert_eq!(code(&res), "group_not_found");
    let res = rename(&app, &r.bob, &group, "Gone").await;
    assert_eq!(res.status, 404);
    assert_eq!(code(&res), "group_not_found");
}

#[tokio::test]
async fn any_participant_names_a_group() {
    let app = TestApp::new().await;
    let r = room(&app).await;
    let seen = ok(drop_on(&app, &r.alice, &r, &r.d1, &r.d2).await);
    let group = groups(&seen)[0]["id"].as_str().unwrap().to_owned();

    let seen = ok(rename(&app, &r.bob, &group, "  Flaky CI is slowing us down ").await);
    assert_eq!(groups(&seen)[0]["name"], "Flaky CI is slowing us down");
    let longest = "é".repeat(80);
    let seen = ok(rename(&app, &r.alice, &group, &longest).await);
    assert_eq!(groups(&seen)[0]["name"], longest.as_str());
    let res = rename(&app, &r.alice, &group, &format!("{longest}!")).await;
    assert_eq!(res.status, 400);
    assert_eq!(code(&res), "invalid_group_name");
    // Empty is allowed: shown as "Untitled group".
    let seen = ok(rename(&app, &r.alice, &group, "   ").await);
    assert_eq!(groups(&seen)[0]["name"], "");
}

#[tokio::test]
async fn a_card_cannot_be_dropped_onto_itself() {
    let app = TestApp::new().await;
    let r = room(&app).await;
    let res = drop_on(&app, &r.alice, &r, &r.p1, &r.p1).await;
    assert_eq!(res.status, 400);
    assert_eq!(code(&res), "same_reflection");
}

#[tokio::test]
async fn grouping_needs_a_participant_during_group() {
    let app = TestApp::new().await;
    let r = reflecting(&app).await;
    // Not yet: Reflect.
    let res = drop_on(&app, &r.alice, &r, &r.p1, &r.p2).await;
    assert_eq!(res.status, 409);
    assert_eq!(code(&res), "wrong_phase");
    next_phase(&app, &r.alice, &r.meeting).await;

    // A member who has not joined.
    let res = drop_on(&app, &r.carol, &r, &r.p1, &r.p2).await;
    assert_eq!(res.status, 403);
    assert_eq!(code(&res), "not_participant");
    let res = move_to(&app, &r.carol, &r.p1, None).await;
    assert_eq!(res.status, 403);

    // Someone outside the team learns nothing.
    let dave = app
        .create_user("dave", "dave@example.com", "Dave Moretti")
        .await;
    let res = drop_on(&app, &dave, &r, &r.p1, &r.p2).await;
    assert_eq!(res.status, 404);
    let res = move_to(&app, &dave, &r.p1, None).await;
    assert_eq!(res.status, 404);
    assert_eq!(code(&res), "reflection_not_found");

    let seen = ok(drop_on(&app, &r.bob, &r, &r.p1, &r.p2).await);
    let group = groups(&seen)[0]["id"].as_str().unwrap().to_owned();
    let res = rename(&app, &dave, &group, "Mine").await;
    assert_eq!(res.status, 404);
    assert_eq!(code(&res), "group_not_found");

    // Over: Vote.
    next_phase(&app, &r.alice, &r.meeting).await;
    let res = move_to(&app, &r.bob, &r.p1, None).await;
    assert_eq!(res.status, 409);
    assert_eq!(code(&res), "wrong_phase");
    let res = rename(&app, &r.bob, &group, "Late").await;
    assert_eq!(res.status, 409);
    // The groups stay for the next phases.
    let seen = state(&app, &r.bob, &r.meeting).await;
    assert_eq!(groups(&seen).len(), 1);
}

#[tokio::test]
async fn ids_from_another_meeting_are_unknown() {
    let app = TestApp::new().await;
    let r = room(&app).await;
    let other = reflecting_with(&app, "carol2").await;
    next_phase(&app, &other.alice, &other.meeting).await;
    let seen = ok(drop_on(&app, &other.alice, &other, &other.p1, &other.p2).await);
    let foreign_group = groups(&seen)[0]["id"].as_str().unwrap().to_owned();

    let res = drop_on(&app, &r.alice, &r, &other.d1, &r.d1).await;
    assert_eq!(res.status, 404);
    assert_eq!(code(&res), "reflection_not_found");
    let res = move_to(&app, &r.alice, &r.d1, Some(&foreign_group)).await;
    assert_eq!(res.status, 404);
    assert_eq!(code(&res), "group_not_found");
}

#[tokio::test]
async fn an_ended_meeting_is_read_only() {
    let app = TestApp::new().await;
    let r = room(&app).await;
    let seen = ok(drop_on(&app, &r.alice, &r, &r.d1, &r.d2).await);
    let group = groups(&seen)[0]["id"].as_str().unwrap().to_owned();
    let res = post(
        &app,
        &r.alice,
        &format!("/api/v1/meetings/{}/end", r.meeting),
        None,
    )
    .await;
    assert_eq!(res.status, 200, "{}", res.text());

    let res = rename(&app, &r.bob, &group, "After").await;
    assert_eq!(res.status, 409);
    assert_eq!(code(&res), "meeting_ended");
    let res = drop_on(&app, &r.bob, &r, &r.p1, &r.p2).await;
    assert_eq!(res.status, 409);
    assert_eq!(code(&res), "meeting_ended");
}

#[tokio::test]
async fn meeting_lists_count_the_groups() {
    let app = TestApp::new().await;
    let r = room(&app).await;
    ok(drop_on(&app, &r.alice, &r, &r.d1, &r.d2).await);
    ok(drop_on(&app, &r.alice, &r, &r.p1, &r.p2).await);
    let res = app
        .call(
            "GET",
            &format!("/api/v1/teams/{}/meetings", r.team),
            &r.alice,
            None,
        )
        .await;
    assert_eq!(res.status, 200, "{}", res.text());
    assert_eq!(res.json()[0]["groupCount"], 2);
}
