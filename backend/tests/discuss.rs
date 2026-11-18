//! Discuss phase over REST: the order of the groups and their totals, the
//! current group (facilitator), emoji reactions and the tasks written for a
//! group (spec §9.5, §12, §14.4, §14.5).

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

fn group_of(state: &Value, card: &str) -> String {
    state["reflections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == card)
        .unwrap()["groupId"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn vote(app: &TestApp, token: &str, group: &str, times: usize) {
    for _ in 0..times {
        let res = post(
            app,
            token,
            &format!("/api/v1/groups/{group}/vote"),
            Some(json!({ "delta": 1 })),
        )
        .await;
        assert_eq!(res.status, 200, "{}", res.text());
    }
}

/// alice (admin, facilitator), bob and erin (participants); carol is a
/// member who has not joined. Cards: p1 + p2 grouped as "Pair", and the
/// single cards d1, i1 and t1.
struct Room {
    alice: String,
    bob: String,
    erin: String,
    carol: String,
    team: Uuid,
    meeting: String,
    d1: String,
    p1: String,
    i1: String,
    /// Groups, by their first card.
    g_d1: String,
    g_pair: String,
    g_i1: String,
    g_t1: String,
}

/// The room in Vote, with the votes cast: d1 3 (bob), Pair 2 (erin), i1 2
/// (alice), t1 none. Discuss order: d1, Pair (2 cards beat i1's one), i1, t1.
async fn voting_with(app: &TestApp, suffix: &str) -> Room {
    let alice = app.token_for("alice").await;
    let bob = app.token_for("bob").await;
    let erin = app
        .create_user(
            &format!("ezra{suffix}"),
            &format!("ezra{suffix}@example.com"),
            "Erin Galli",
        )
        .await;
    let carol = app
        .create_user(
            &format!("cleo{suffix}"),
            &format!("cleo{suffix}@example.com"),
            "Carol Verdi",
        )
        .await;
    let team = create_team(app, &alice, "Platform").await;
    for token in [&bob, &erin, &carol] {
        join_team(app, &alice, team, token).await;
    }
    let res = post(app, &alice, &format!("/api/v1/teams/{team}/meetings"), None).await;
    assert_eq!(res.status, 201, "{}", res.text());
    let meeting = res.json()["id"].as_str().unwrap().to_owned();
    for token in [&bob, &erin] {
        let res = post(
            app,
            token,
            &format!("/api/v1/meetings/{meeting}/join"),
            None,
        )
        .await;
        assert_eq!(res.status, 200, "{}", res.text());
    }
    next_phase(app, &alice, &meeting).await;
    let p1 = sent(app, &alice, &meeting, "plus", "Release train").await;
    let d1 = sent(app, &bob, &meeting, "delta", "Flaky tests").await;
    let p2 = sent(app, &erin, &meeting, "plus", "Pairing").await;
    let i1 = sent(app, &bob, &meeting, "ideas", "Quarantine").await;
    let t1 = sent(app, &erin, &meeting, "thanks", "Thanks Marco").await;
    next_phase(app, &alice, &meeting).await;
    let res = post(
        app,
        &alice,
        &format!("/api/v1/meetings/{meeting}/groups"),
        Some(json!({ "targetId": p1, "reflectionId": p2 })),
    )
    .await;
    assert_eq!(res.status, 200, "{}", res.text());
    let g_pair = res.json()["groups"][0]["id"].as_str().unwrap().to_owned();
    let res = app
        .call(
            "PATCH",
            &format!("/api/v1/groups/{g_pair}"),
            &alice,
            Some(json!({ "name": "Pair" })),
        )
        .await;
    assert_eq!(res.status, 200, "{}", res.text());
    next_phase(app, &alice, &meeting).await;
    let seen = state(app, &alice, &meeting).await;
    let (g_d1, g_i1, g_t1) = (
        group_of(&seen, &d1),
        group_of(&seen, &i1),
        group_of(&seen, &t1),
    );
    vote(app, &bob, &g_d1, 3).await;
    vote(app, &erin, &g_pair, 2).await;
    vote(app, &alice, &g_i1, 2).await;
    Room {
        alice,
        bob,
        erin,
        carol,
        team,
        meeting,
        d1,
        p1,
        i1,
        g_d1,
        g_pair,
        g_i1,
        g_t1,
    }
}

/// The room in Discuss.
async fn discussing_with(app: &TestApp, suffix: &str) -> Room {
    let r = voting_with(app, suffix).await;
    next_phase(app, &r.alice, &r.meeting).await;
    r
}

async fn discussing(app: &TestApp) -> Room {
    discussing_with(app, "").await
}

async fn set_current(app: &TestApp, token: &str, meeting: &str, group: &str) -> TestResponse {
    app.call(
        "PUT",
        &format!("/api/v1/meetings/{meeting}/discuss/current"),
        token,
        Some(json!({ "groupId": group })),
    )
    .await
}

/// Percent-encodes an emoji for the path.
fn encoded(emoji: &str) -> String {
    emoji
        .bytes()
        .map(|b| format!("%{b:02X}"))
        .collect::<String>()
}

async fn react(app: &TestApp, token: &str, card: &str, emoji: &str, on: bool) -> TestResponse {
    app.call(
        if on { "PUT" } else { "DELETE" },
        &format!("/api/v1/reflections/{card}/reactions/{}", encoded(emoji)),
        token,
        None,
    )
    .await
}

async fn task(app: &TestApp, token: &str, meeting: &str, group: &str, html: &str) -> TestResponse {
    app.discuss_task(token, meeting, group, html, &[]).await
}

fn ok(res: TestResponse) -> Value {
    assert_eq!(res.status, 200, "{}", res.text());
    res.json()
}

fn fails(res: &TestResponse, status: u16, expected: &str) {
    assert_eq!(res.status, status, "{}", res.text());
    assert_eq!(res.json()["code"], expected, "{}", res.text());
}

/// The (emoji, count, mine) on one card, in a state.
fn reactions_on(state: &Value, card: &str) -> Vec<(String, i64, bool)> {
    state["reactions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["reflectionId"] == card)
        .map(|r| {
            (
                r["emoji"].as_str().unwrap().to_owned(),
                r["count"].as_i64().unwrap(),
                r["mine"].as_bool().unwrap(),
            )
        })
        .collect()
}

fn order(state: &Value) -> Vec<(String, i64)> {
    state["discussOrder"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| {
            (
                g["groupId"].as_str().unwrap().to_owned(),
                g["votes"].as_i64().unwrap(),
            )
        })
        .collect()
}

#[tokio::test]
async fn totals_stay_hidden_until_discuss_then_rank_the_groups() {
    let app = TestApp::new().await;
    let r = voting_with(&app, "").await;
    let before = state(&app, &r.bob, &r.meeting).await;
    assert_eq!(before["discussOrder"], json!([]));
    assert_eq!(before["discussGroupId"], Value::Null);
    assert_eq!(before["reactions"], json!([]));
    assert_eq!(before["tasks"], json!([]));

    next_phase(&app, &r.alice, &r.meeting).await;
    for token in [&r.alice, &r.bob, &r.erin] {
        let seen = state(&app, token, &r.meeting).await;
        assert_eq!(
            order(&seen),
            vec![
                (r.g_d1.clone(), 3),
                (r.g_pair.clone(), 2),
                (r.g_i1.clone(), 2),
                (r.g_t1.clone(), 0),
            ]
        );
        assert_eq!(seen["discussOrder"][1]["cards"], 2);
        // Discuss starts on the group with the most votes.
        assert_eq!(seen["discussGroupId"], r.g_d1);
    }
    // Discuss is the last phase, and has no Ready.
    let res = post(
        &app,
        &r.alice,
        &format!("/api/v1/meetings/{}/phase/next", r.meeting),
        None,
    )
    .await;
    fails(&res, 409, "last_phase");
}

#[tokio::test]
async fn a_discuss_without_cards_has_no_current_group() {
    let app = TestApp::new().await;
    let alice = app.token_for("alice").await;
    let team = create_team(&app, &alice, "Quiet").await;
    let res = post(
        &app,
        &alice,
        &format!("/api/v1/teams/{team}/meetings"),
        None,
    )
    .await;
    let meeting = res.json()["id"].as_str().unwrap().to_owned();
    for _ in 0..4 {
        next_phase(&app, &alice, &meeting).await;
    }
    let seen = state(&app, &alice, &meeting).await;
    assert_eq!(seen["phase"], "discuss");
    assert_eq!(seen["discussOrder"], json!([]));
    assert_eq!(seen["discussGroupId"], Value::Null);
    let res = task(
        &app,
        &alice,
        &meeting,
        &Uuid::now_v7().to_string(),
        "<p>x</p>",
    )
    .await;
    fails(&res, 404, "group_not_found");
}

#[tokio::test]
async fn only_the_facilitator_moves_everyone_to_another_group() {
    let app = TestApp::new().await;
    let r = discussing(&app).await;

    let res = set_current(&app, &r.bob, &r.meeting, &r.g_pair).await;
    fails(&res, 403, "not_facilitator");

    let seen = ok(set_current(&app, &r.alice, &r.meeting, &r.g_pair).await);
    assert_eq!(seen["discussGroupId"], r.g_pair);
    assert_eq!(
        state(&app, &r.bob, &r.meeting).await["discussGroupId"],
        r.g_pair
    );
    // Backwards too, and to the same group again.
    ok(set_current(&app, &r.alice, &r.meeting, &r.g_d1).await);
    let seen = ok(set_current(&app, &r.alice, &r.meeting, &r.g_d1).await);
    assert_eq!(seen["discussGroupId"], r.g_d1);

    // Unknown groups, and groups of another meeting.
    let res = set_current(&app, &r.alice, &r.meeting, &Uuid::now_v7().to_string()).await;
    fails(&res, 404, "group_not_found");
    let other = discussing_with(&app, "2").await;
    let res = set_current(&app, &r.alice, &r.meeting, &other.g_d1).await;
    fails(&res, 404, "group_not_found");
}

#[tokio::test]
async fn an_admin_takes_over_before_moving_the_group() {
    let app = TestApp::new().await;
    let r = discussing(&app).await;
    let bob_id = app.get_with_token("/api/v1/me", &r.bob).await.json()["user"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let alice_id = app.get_with_token("/api/v1/me", &r.alice).await.json()["user"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let facilitator = format!("/api/v1/meetings/{}/facilitator", r.meeting);
    let res = post(
        &app,
        &r.alice,
        &facilitator,
        Some(json!({ "userId": bob_id })),
    )
    .await;
    assert_eq!(res.status, 200, "{}", res.text());

    // alice is a team admin, but no longer the facilitator.
    let res = set_current(&app, &r.alice, &r.meeting, &r.g_i1).await;
    fails(&res, 403, "not_facilitator");
    ok(set_current(&app, &r.bob, &r.meeting, &r.g_i1).await);

    let res = post(
        &app,
        &r.alice,
        &facilitator,
        Some(json!({ "userId": alice_id })),
    )
    .await;
    assert_eq!(res.status, 200, "{}", res.text());
    let seen = ok(set_current(&app, &r.alice, &r.meeting, &r.g_t1).await);
    assert_eq!(seen["discussGroupId"], r.g_t1);
}

#[tokio::test]
async fn the_current_group_changes_only_during_discuss() {
    let app = TestApp::new().await;
    let r = voting_with(&app, "").await;
    let res = set_current(&app, &r.alice, &r.meeting, &r.g_d1).await;
    fails(&res, 409, "wrong_phase");

    next_phase(&app, &r.alice, &r.meeting).await;
    let res = post(
        &app,
        &r.alice,
        &format!("/api/v1/meetings/{}/end", r.meeting),
        None,
    )
    .await;
    assert_eq!(res.status, 200, "{}", res.text());
    let res = set_current(&app, &r.alice, &r.meeting, &r.g_pair).await;
    fails(&res, 409, "meeting_ended");
}

#[tokio::test]
async fn reactions_toggle_one_of_each_emoji_per_person() {
    let app = TestApp::new().await;
    let r = discussing(&app).await;

    let first = ok(react(&app, &r.bob, &r.d1, "👍", true).await);
    assert_eq!(
        first,
        json!({ "reflectionId": r.d1, "emoji": "👍", "count": 1, "mine": true })
    );
    assert_eq!(
        ok(react(&app, &r.erin, &r.d1, "👍", true).await)["count"],
        2
    );
    assert_eq!(ok(react(&app, &r.bob, &r.d1, "🎉", true).await)["count"], 1);
    // Again: nothing changes.
    let again = ok(react(&app, &r.bob, &r.d1, "👍", true).await);
    assert_eq!(
        (again["count"].clone(), again["mine"].clone()),
        (json!(2), json!(true))
    );

    // Each person sees the counts, and which ones are theirs.
    let by_bob = state(&app, &r.bob, &r.meeting).await;
    assert_eq!(
        reactions_on(&by_bob, &r.d1),
        vec![("👍".into(), 2, true), ("🎉".into(), 1, true)]
    );
    let by_erin = state(&app, &r.erin, &r.meeting).await;
    assert_eq!(
        reactions_on(&by_erin, &r.d1),
        vec![("👍".into(), 2, true), ("🎉".into(), 1, false)]
    );
    let by_alice = state(&app, &r.alice, &r.meeting).await;
    assert_eq!(
        reactions_on(&by_alice, &r.d1),
        vec![("👍".into(), 2, false), ("🎉".into(), 1, false)]
    );

    // Taking back, twice.
    let back = ok(react(&app, &r.bob, &r.d1, "👍", false).await);
    assert_eq!(
        (back["count"].clone(), back["mine"].clone()),
        (json!(1), json!(false))
    );
    let back = ok(react(&app, &r.bob, &r.d1, "👍", false).await);
    assert_eq!(back["count"], 1);
    ok(react(&app, &r.bob, &r.d1, "🎉", false).await);
    let seen = state(&app, &r.bob, &r.meeting).await;
    assert_eq!(reactions_on(&seen, &r.d1), vec![("👍".into(), 1, false)]);
}

#[tokio::test]
async fn emoji_are_checked_and_stored_fully_qualified() {
    let app = TestApp::new().await;
    let r = discussing(&app).await;
    // "❤" without its variation selector is the same reaction as "❤️".
    assert_eq!(
        ok(react(&app, &r.bob, &r.d1, "❤", true).await)["emoji"],
        "❤️"
    );
    let both = ok(react(&app, &r.erin, &r.d1, "❤️", true).await);
    assert_eq!(
        (both["emoji"].clone(), both["count"].clone()),
        (json!("❤️"), json!(2))
    );
    assert_eq!(
        ok(react(&app, &r.erin, &r.d1, "👍🏽", true).await)["emoji"],
        "👍🏽"
    );

    for bad in ["a", "👍👍", ":tada:"] {
        fails(
            &react(&app, &r.bob, &r.d1, bad, true).await,
            400,
            "invalid_emoji",
        );
    }
}

#[tokio::test]
async fn reactions_need_a_participant_on_the_current_group() {
    let app = TestApp::new().await;
    let r = voting_with(&app, "").await;
    fails(
        &react(&app, &r.bob, &r.d1, "👍", true).await,
        409,
        "wrong_phase",
    );
    next_phase(&app, &r.alice, &r.meeting).await;

    // A previewed group is read-only.
    fails(
        &react(&app, &r.bob, &r.p1, "👍", true).await,
        409,
        "not_current_group",
    );
    fails(
        &react(&app, &r.bob, &r.i1, "👍", false).await,
        409,
        "not_current_group",
    );
    // A member who has not joined.
    fails(
        &react(&app, &r.carol, &r.d1, "👍", true).await,
        403,
        "not_participant",
    );
    // Someone outside the team learns nothing.
    let dave = app
        .create_user("dex1", "dex1@example.com", "Dave Moretti")
        .await;
    fails(
        &react(&app, &dave, &r.d1, "👍", true).await,
        404,
        "reflection_not_found",
    );
    let unknown = Uuid::now_v7().to_string();
    fails(
        &react(&app, &r.bob, &unknown, "👍", true).await,
        404,
        "reflection_not_found",
    );

    // Once the facilitator moves on, the new group takes reactions.
    ok(set_current(&app, &r.alice, &r.meeting, &r.g_pair).await);
    ok(react(&app, &r.bob, &r.p1, "👍", true).await);
    fails(
        &react(&app, &r.bob, &r.d1, "👍", true).await,
        409,
        "not_current_group",
    );

    let res = post(
        &app,
        &r.alice,
        &format!("/api/v1/meetings/{}/end", r.meeting),
        None,
    )
    .await;
    assert_eq!(res.status, 200, "{}", res.text());
    fails(
        &react(&app, &r.bob, &r.p1, "🎉", true).await,
        409,
        "meeting_ended",
    );
    // The ended meeting keeps them.
    let seen = state(&app, &r.bob, &r.meeting).await;
    assert_eq!(reactions_on(&seen, &r.p1), vec![("👍".into(), 1, true)]);
}

#[tokio::test]
async fn tasks_go_to_the_current_group_and_the_active_column() {
    let app = TestApp::new().await;
    let r = discussing(&app).await;
    let erin_id = app.get_with_token("/api/v1/me", &r.erin).await.json()["user"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let html = format!(
        r#"<p><strong>Quarantine</strong> flaky tests <span data-type="mention" data-id="{erin_id}" data-label="Erin Galli">@Erin Galli</span></p>"#
    );
    let res = task(&app, &r.bob, &r.meeting, &r.g_d1, &html).await;
    assert_eq!(res.status, 201, "{}", res.text());
    let tasks = res.json()["tasks"].clone();
    assert_eq!(tasks.as_array().unwrap().len(), 1);
    let t = &tasks[0];
    assert_eq!(t["status"], "active");
    assert_eq!(t["meetingId"], r.meeting);
    assert_eq!(t["groupId"], r.g_d1);
    assert_eq!(t["author"]["displayName"], "Bob Bianchi");
    assert_eq!(t["mentions"][0]["id"], erin_id);
    assert!(
        t["contentHtml"]
            .as_str()
            .unwrap()
            .contains("<strong>Quarantine</strong>")
    );

    // Everyone in the room sees it; the board has it in Active.
    let seen = state(&app, &r.erin, &r.meeting).await;
    assert_eq!(seen["tasks"][0]["id"], t["id"]);
    let board = app
        .get_with_token(&format!("/api/v1/teams/{}/tasks", r.team), &r.alice)
        .await
        .json();
    assert_eq!(board[0]["id"], t["id"]);
    assert_eq!(board[0]["status"], "active");

    // Another group, later: tasks keep their group, oldest first.
    ok(set_current(&app, &r.alice, &r.meeting, &r.g_pair).await);
    let res = task(&app, &r.alice, &r.meeting, &r.g_pair, "<p>Pair weekly</p>").await;
    assert_eq!(res.status, 201, "{}", res.text());
    let tasks = res.json()["tasks"].clone();
    assert_eq!(tasks[0]["groupId"], r.g_d1);
    assert_eq!(tasks[1]["groupId"], r.g_pair);
}

#[tokio::test]
async fn tasks_need_a_participant_during_discuss_on_the_current_group() {
    let app = TestApp::new().await;
    let r = voting_with(&app, "").await;
    fails(
        &task(&app, &r.bob, &r.meeting, &r.g_d1, "<p>x</p>").await,
        409,
        "wrong_phase",
    );
    next_phase(&app, &r.alice, &r.meeting).await;

    fails(
        &task(&app, &r.bob, &r.meeting, &r.g_pair, "<p>x</p>").await,
        409,
        "not_current_group",
    );
    fails(
        &task(&app, &r.carol, &r.meeting, &r.g_d1, "<p>x</p>").await,
        403,
        "not_participant",
    );
    fails(
        &task(
            &app,
            &r.bob,
            &r.meeting,
            &Uuid::now_v7().to_string(),
            "<p>x</p>",
        )
        .await,
        404,
        "group_not_found",
    );
    fails(
        &task(&app, &r.bob, &r.meeting, &r.g_d1, "<p> </p>").await,
        400,
        "invalid_task_content",
    );
    let other = discussing_with(&app, "2").await;
    fails(
        &task(&app, &r.bob, &r.meeting, &other.g_d1, "<p>x</p>").await,
        404,
        "group_not_found",
    );
    let dave = app
        .create_user("dex1", "dex1@example.com", "Dave Moretti")
        .await;
    fails(
        &task(&app, &dave, &r.meeting, &r.g_d1, "<p>x</p>").await,
        404,
        "meeting_not_found",
    );

    let res = post(
        &app,
        &r.alice,
        &format!("/api/v1/meetings/{}/end", r.meeting),
        None,
    )
    .await;
    assert_eq!(res.status, 200, "{}", res.text());
    fails(
        &task(&app, &r.bob, &r.meeting, &r.g_d1, "<p>x</p>").await,
        409,
        "meeting_ended",
    );
    assert_eq!(state(&app, &r.bob, &r.meeting).await["tasks"], json!([]));
}
