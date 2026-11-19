//! Reflect phase over REST: reflections, the timer and ready flags (spec §8.2,
//! §9.2, §12, §14.3–14.4).

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

async fn user_id(app: &TestApp, token: &str) -> String {
    let me = app.get_with_token("/api/v1/me", token).await.json();
    me["user"]["id"].as_str().unwrap().to_owned()
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

/// alice (admin, facilitator) and bob (member, participant) in a meeting
/// that is in Reflect; carol is a member who has not joined.
struct Room {
    alice: String,
    bob: String,
    carol: String,
    meeting: String,
}

async fn room(app: &TestApp) -> Room {
    let alice = app.token_for("alice").await;
    let bob = app.token_for("bob").await;
    let carol = app
        .create_user("carol", "carol@example.com", "Carol Verdi")
        .await;
    let team = create_team(app, &alice, "Platform").await;
    join_team(app, &alice, team, &bob).await;
    join_team(app, &alice, team, &carol).await;
    let res = post(app, &alice, &format!("/api/v1/teams/{team}/meetings"), None).await;
    assert_eq!(res.status, 201, "{}", res.text());
    let meeting = res.json()["id"].as_str().unwrap().to_owned();
    let res = post(
        app,
        &bob,
        &format!("/api/v1/teams/{team}/meetings/{meeting}/join"),
        None,
    )
    .await;
    assert_eq!(res.status, 200, "{}", res.text());
    next_phase(app, &alice, &meeting).await;
    Room {
        alice,
        bob,
        carol,
        meeting,
    }
}

async fn write(
    app: &TestApp,
    token: &str,
    meeting: &str,
    column: &str,
    html: &str,
) -> TestResponse {
    post(
        app,
        token,
        &format!("/api/v1/meetings/{meeting}/reflections"),
        Some(json!({ "column": column, "content": html })),
    )
    .await
}

/// Sends a reflection and returns its id.
async fn sent(app: &TestApp, token: &str, meeting: &str, column: &str, html: &str) -> String {
    let res = write(app, token, meeting, column, html).await;
    assert_eq!(res.status, 201, "{}", res.text());
    let reflections = res.json()["reflections"].as_array().unwrap().clone();
    reflections.last().unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn counts(state: &Value) -> Value {
    state["reflectionCounts"].clone()
}

// --- Reflections -----------------------------------------------------------

#[tokio::test]
async fn participants_send_reflections_that_only_they_can_read() {
    let app = TestApp::new().await;
    let r = room(&app).await;

    let res = write(
        &app,
        &r.bob,
        &r.meeting,
        "delta",
        "<p>Flaky <s>e2e</s> tests</p>",
    )
    .await;
    assert_eq!(res.status, 201, "{}", res.text());
    let mine = res.json();
    let reflections = mine["reflections"].as_array().unwrap();
    assert_eq!(reflections.len(), 1);
    assert_eq!(reflections[0]["column"], "delta");
    assert_eq!(reflections[0]["content"], "<p>Flaky <s>e2e</s> tests</p>");
    assert_eq!(reflections[0]["mine"], true);
    // Never an author, not even for the author.
    assert!(reflections[0].get("authorId").is_none());
    assert_eq!(
        counts(&mine),
        json!({ "plus": 0, "delta": 1, "ideas": 0, "thanks": 0 })
    );

    // The facilitator sees the count, never the content.
    let theirs = state(&app, &r.alice, &r.meeting).await;
    assert_eq!(theirs["reflections"], json!([]));
    assert_eq!(theirs["reflectionCounts"]["delta"], 1);
    assert!(!theirs.to_string().contains("Flaky"));
}

#[tokio::test]
async fn authors_edit_move_and_delete_their_reflections() {
    let app = TestApp::new().await;
    let r = room(&app).await;
    let id = sent(&app, &r.bob, &r.meeting, "plus", "<p>Good</p>").await;

    let res = app
        .call(
            "PATCH",
            &format!("/api/v1/reflections/{id}"),
            &r.bob,
            Some(json!({ "column": "ideas", "content": "<ul><li><p>Better <code>x</code></p></li></ul>" })),
        )
        .await;
    assert_eq!(res.status, 200, "{}", res.text());
    let state = res.json();
    assert_eq!(state["reflections"][0]["column"], "ideas");
    assert_eq!(
        state["reflections"][0]["content"],
        "<ul><li><p>Better <code>x</code></p></li></ul>"
    );
    assert_eq!(
        counts(&state),
        json!({ "plus": 0, "delta": 0, "ideas": 1, "thanks": 0 })
    );

    let res = app
        .call("DELETE", &format!("/api/v1/reflections/{id}"), &r.bob, None)
        .await;
    assert_eq!(res.status, 200, "{}", res.text());
    assert_eq!(res.json()["reflections"], json!([]));
    assert_eq!(res.json()["reflectionCounts"]["ideas"], 0);
}

#[tokio::test]
async fn other_peoples_reflections_do_not_exist_for_you() {
    let app = TestApp::new().await;
    let r = room(&app).await;
    let id = sent(&app, &r.bob, &r.meeting, "plus", "<p>Mine</p>").await;
    for (method, body) in [
        ("PATCH", Some(json!({ "content": "<p>Hijacked</p>" }))),
        ("DELETE", None),
    ] {
        let res = app
            .call(method, &format!("/api/v1/reflections/{id}"), &r.alice, body)
            .await;
        assert_eq!(res.status, 404, "{}", res.text());
        assert_eq!(res.json()["code"], "reflection_not_found");
    }
    let res = app
        .call(
            "DELETE",
            &format!("/api/v1/reflections/{}", Uuid::now_v7()),
            &r.bob,
            None,
        )
        .await;
    assert_eq!(res.status, 404);
    assert_eq!(
        state(&app, &r.bob, &r.meeting).await["reflections"][0]["content"],
        "<p>Mine</p>"
    );
}

#[tokio::test]
async fn reflection_content_is_sanitized() {
    let app = TestApp::new().await;
    let r = room(&app).await;
    let res = write(
        &app,
        &r.bob,
        &r.meeting,
        "thanks",
        r#"<p onclick="x()">Hi <img src=x onerror="alert(1)"><script>alert(1)</script><a href="javascript:alert(1)">l</a> <strong>b</strong> <em>i</em> <a href="https://x.dev">ok</a></p>"#,
    )
    .await;
    assert_eq!(res.status, 201, "{}", res.text());
    assert_eq!(
        res.json()["reflections"][0]["content"],
        r#"<p>Hi <a rel="noopener noreferrer">l</a> <strong>b</strong> <em>i</em> <a href="https://x.dev" rel="noopener noreferrer">ok</a></p>"#
    );

    for html in ["", "<p>   </p>", "<script>x</script>", &"a".repeat(10_001)] {
        let res = write(&app, &r.bob, &r.meeting, "plus", html).await;
        assert_eq!(res.status, 400, "{html}: {}", res.text());
        assert_eq!(res.json()["code"], "invalid_reflection_content");
    }
    let res = write(&app, &r.bob, &r.meeting, "kudos", "<p>x</p>").await;
    assert_eq!(res.status, 422, "{}", res.text());
}

#[tokio::test]
async fn reflections_are_written_only_by_participants_during_reflect() {
    let app = TestApp::new().await;
    let r = room(&app).await;

    // Carol is a team member who has not joined.
    let res = write(&app, &r.carol, &r.meeting, "plus", "<p>x</p>").await;
    assert_eq!(res.status, 403, "{}", res.text());
    assert_eq!(res.json()["code"], "not_participant");

    // Outsiders do not see the meeting.
    let dave = app
        .create_user("dave", "dave@example.com", "Dave Outsider")
        .await;
    let res = write(&app, &dave, &r.meeting, "plus", "<p>x</p>").await;
    assert_eq!(res.status, 404);
    assert_eq!(res.json()["code"], "meeting_not_found");

    let id = sent(&app, &r.bob, &r.meeting, "plus", "<p>Before Group</p>").await;
    next_phase(&app, &r.alice, &r.meeting).await;
    let res = write(&app, &r.bob, &r.meeting, "plus", "<p>Late</p>").await;
    assert_eq!(res.status, 409);
    assert_eq!(res.json()["code"], "wrong_phase");
    let res = app
        .call(
            "PATCH",
            &format!("/api/v1/reflections/{id}"),
            &r.bob,
            Some(json!({ "content": "<p>Changed</p>" })),
        )
        .await;
    assert_eq!(res.status, 409);
    assert_eq!(res.json()["code"], "wrong_phase");
    let res = app
        .call("DELETE", &format!("/api/v1/reflections/{id}"), &r.bob, None)
        .await;
    assert_eq!(res.status, 409);

    let res = post(
        &app,
        &r.alice,
        &format!("/api/v1/meetings/{}/end", r.meeting),
        None,
    )
    .await;
    assert_eq!(res.status, 200);
    let res = write(&app, &r.bob, &r.meeting, "plus", "<p>x</p>").await;
    assert_eq!(res.status, 409);
    assert_eq!(res.json()["code"], "meeting_ended");
}

#[tokio::test]
async fn reflections_cannot_be_written_in_the_icebreaker() {
    let app = TestApp::new().await;
    let alice = app.token_for("alice").await;
    let team = create_team(&app, &alice, "Platform").await;
    let res = post(
        &app,
        &alice,
        &format!("/api/v1/teams/{team}/meetings"),
        None,
    )
    .await;
    let meeting = res.json()["id"].as_str().unwrap().to_owned();
    let res = write(&app, &alice, &meeting, "plus", "<p>x</p>").await;
    assert_eq!(res.status, 409);
    assert_eq!(res.json()["code"], "wrong_phase");
}

#[tokio::test]
async fn group_reveals_every_reflection_anonymously() {
    let app = TestApp::new().await;
    let r = room(&app).await;
    sent(&app, &r.bob, &r.meeting, "delta", "<p>Bob's</p>").await;
    sent(&app, &r.alice, &r.meeting, "plus", "<p>Alice's</p>").await;
    next_phase(&app, &r.alice, &r.meeting).await;

    let seen = state(&app, &r.alice, &r.meeting).await;
    let reflections = seen["reflections"].as_array().unwrap();
    assert_eq!(reflections.len(), 2);
    let bob_id = user_id(&app, &r.bob).await;
    assert!(!seen["reflections"].to_string().contains(&bob_id));
    let mine: Vec<_> = reflections.iter().map(|r| r["mine"].clone()).collect();
    assert_eq!(mine, vec![json!(false), json!(true)]);
}

#[tokio::test]
async fn each_author_has_a_cap() {
    let app = TestApp::new().await;
    let r = room(&app).await;
    {
        use sea_orm::{ConnectionTrait, DbBackend, Statement};
        let bob_id: Uuid = user_id(&app, &r.bob).await.parse().unwrap();
        let meeting: Uuid = r.meeting.parse().unwrap();
        app.db
            .execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "INSERT INTO reflections (id, meeting_id, author_id, \"column\", content) \
                 SELECT gen_random_uuid(), $1, $2, 'plus', '<p>x</p>' FROM generate_series(1, 100)",
                [meeting.into(), bob_id.into()],
            ))
            .await
            .unwrap();
    }
    let res = write(&app, &r.bob, &r.meeting, "plus", "<p>101</p>").await;
    assert_eq!(res.status, 409);
    assert_eq!(res.json()["code"], "too_many_reflections");
    // Others are not affected.
    assert_eq!(
        write(&app, &r.alice, &r.meeting, "plus", "<p>ok</p>")
            .await
            .status,
        201
    );
}

// --- Timer -----------------------------------------------------------------

async fn timer(app: &TestApp, token: &str, meeting: &str, body: Value) -> TestResponse {
    app.call(
        "PUT",
        &format!("/api/v1/meetings/{meeting}/timer"),
        token,
        Some(body),
    )
    .await
}

fn time(value: &Value) -> time::OffsetDateTime {
    time::OffsetDateTime::parse(
        value.as_str().unwrap(),
        &time::format_description::well_known::Rfc3339,
    )
    .unwrap()
}

#[tokio::test]
async fn the_facilitator_runs_the_timer() {
    let app = TestApp::new().await;
    let r = room(&app).await;
    assert_eq!(state(&app, &r.bob, &r.meeting).await["timer"], Value::Null);

    let res = timer(
        &app,
        &r.alice,
        &r.meeting,
        json!({ "action": "start", "durationSeconds": 300 }),
    )
    .await;
    assert_eq!(res.status, 200, "{}", res.text());
    let started = res.json();
    let t = &started["timer"];
    assert_eq!(t["status"], "running");
    assert_eq!(t["durationMs"], 300_000);
    let left = time(&t["endsAt"]) - time(&started["serverNow"]);
    assert!(
        left.whole_seconds() > 295 && left.whole_seconds() <= 300,
        "{left}"
    );
    // Everyone sees the same timer.
    assert_eq!(state(&app, &r.bob, &r.meeting).await["timer"], *t);

    let res = timer(&app, &r.alice, &r.meeting, json!({ "action": "pause" })).await;
    let paused = res.json()["timer"].clone();
    assert_eq!(paused["status"], "paused");
    assert_eq!(paused["endsAt"], Value::Null);
    let remaining = paused["remainingMs"].as_i64().unwrap();
    assert!(remaining > 295_000 && remaining <= 300_000, "{remaining}");

    let res = timer(
        &app,
        &r.alice,
        &r.meeting,
        json!({ "action": "add", "seconds": 60 }),
    )
    .await;
    assert_eq!(res.json()["timer"]["remainingMs"], remaining + 60_000);
    assert_eq!(res.json()["timer"]["durationMs"], 360_000);

    let res = timer(&app, &r.alice, &r.meeting, json!({ "action": "resume" })).await;
    assert_eq!(res.json()["timer"]["status"], "running");

    // The timer carries over to the next phase.
    next_phase(&app, &r.alice, &r.meeting).await;
    assert_eq!(
        state(&app, &r.bob, &r.meeting).await["timer"]["status"],
        "running"
    );

    let res = timer(&app, &r.alice, &r.meeting, json!({ "action": "stop" })).await;
    assert_eq!(res.status, 200);
    assert_eq!(res.json()["timer"], Value::Null);
}

#[tokio::test]
async fn only_the_facilitator_sets_the_timer() {
    let app = TestApp::new().await;
    let r = room(&app).await;
    let res = timer(
        &app,
        &r.bob,
        &r.meeting,
        json!({ "action": "start", "durationSeconds": 60 }),
    )
    .await;
    assert_eq!(res.status, 403);
    assert_eq!(res.json()["code"], "not_facilitator");
}

#[tokio::test]
async fn timer_requests_are_validated() {
    let app = TestApp::new().await;
    let r = room(&app).await;
    for body in [
        json!({ "action": "start", "durationSeconds": 0 }),
        json!({ "action": "start", "durationSeconds": 6000 }),
    ] {
        let res = timer(&app, &r.alice, &r.meeting, body).await;
        assert_eq!(res.status, 400, "{}", res.text());
        assert_eq!(res.json()["code"], "invalid_duration");
    }
    for action in ["pause", "resume"] {
        let res = timer(&app, &r.alice, &r.meeting, json!({ "action": action })).await;
        assert_eq!(res.status, 409);
        assert_eq!(res.json()["code"], "no_timer");
    }
    let res = timer(
        &app,
        &r.alice,
        &r.meeting,
        json!({ "action": "add", "seconds": 60 }),
    )
    .await;
    assert_eq!(res.json()["code"], "no_timer");
    let res = timer(&app, &r.alice, &r.meeting, json!({ "action": "explode" })).await;
    assert_eq!(res.status, 422);
}

#[tokio::test]
async fn adding_time_to_an_expired_timer_restarts_it() {
    let app = TestApp::new().await;
    let r = room(&app).await;
    timer(
        &app,
        &r.alice,
        &r.meeting,
        json!({ "action": "start", "durationSeconds": 1 }),
    )
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
    let res = timer(
        &app,
        &r.alice,
        &r.meeting,
        json!({ "action": "add", "seconds": 60 }),
    )
    .await;
    let state = res.json();
    let left = time(&state["timer"]["endsAt"]) - time(&state["serverNow"]);
    assert!(
        left.whole_seconds() >= 58 && left.whole_seconds() <= 60,
        "{left}"
    );
}

#[tokio::test]
async fn ending_the_meeting_removes_the_timer() {
    let app = TestApp::new().await;
    let r = room(&app).await;
    timer(
        &app,
        &r.alice,
        &r.meeting,
        json!({ "action": "start", "durationSeconds": 60 }),
    )
    .await;
    post(
        &app,
        &r.alice,
        &format!("/api/v1/meetings/{}/end", r.meeting),
        None,
    )
    .await;
    assert_eq!(state(&app, &r.bob, &r.meeting).await["timer"], Value::Null);
    let res = timer(&app, &r.alice, &r.meeting, json!({ "action": "stop" })).await;
    assert_eq!(res.status, 409);
    assert_eq!(res.json()["code"], "meeting_ended");
}

// --- Ready -----------------------------------------------------------------

async fn ready(app: &TestApp, token: &str, meeting: &str, value: bool) -> TestResponse {
    app.call(
        "PUT",
        &format!("/api/v1/meetings/{meeting}/ready"),
        token,
        Some(json!({ "ready": value })),
    )
    .await
}

fn ready_of(state: &Value, user: &str) -> bool {
    state["participants"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == user)
        .unwrap()["ready"]
        .as_bool()
        .unwrap()
}

#[tokio::test]
async fn participants_press_ready_until_the_phase_changes() {
    let app = TestApp::new().await;
    let r = room(&app).await;
    let bob_id = user_id(&app, &r.bob).await;

    let res = ready(&app, &r.bob, &r.meeting, true).await;
    assert_eq!(res.status, 200, "{}", res.text());
    assert!(ready_of(&res.json(), &bob_id));
    assert!(ready_of(&state(&app, &r.alice, &r.meeting).await, &bob_id));

    let res = ready(&app, &r.bob, &r.meeting, false).await;
    assert!(!ready_of(&res.json(), &bob_id));

    ready(&app, &r.bob, &r.meeting, true).await;
    next_phase(&app, &r.alice, &r.meeting).await;
    assert!(!ready_of(&state(&app, &r.alice, &r.meeting).await, &bob_id));
    // Group and Vote have Ready too; Discuss does not.
    assert_eq!(ready(&app, &r.bob, &r.meeting, true).await.status, 200);
    next_phase(&app, &r.alice, &r.meeting).await;
    assert_eq!(ready(&app, &r.bob, &r.meeting, true).await.status, 200);
    next_phase(&app, &r.alice, &r.meeting).await;
    let res = ready(&app, &r.bob, &r.meeting, true).await;
    assert_eq!(res.status, 409);
    assert_eq!(res.json()["code"], "wrong_phase");
}

#[tokio::test]
async fn ready_needs_a_participant_in_a_phase_with_ready() {
    let app = TestApp::new().await;
    let alice = app.token_for("alice").await;
    let team = create_team(&app, &alice, "Platform").await;
    let res = post(
        &app,
        &alice,
        &format!("/api/v1/teams/{team}/meetings"),
        None,
    )
    .await;
    let meeting = res.json()["id"].as_str().unwrap().to_owned();
    let res = ready(&app, &alice, &meeting, true).await;
    assert_eq!(res.status, 409, "the icebreaker has no Ready");
    assert_eq!(res.json()["code"], "wrong_phase");

    let r = room(&app).await;
    let res = ready(&app, &r.carol, &r.meeting, true).await;
    assert_eq!(res.status, 403);
    assert_eq!(res.json()["code"], "not_participant");
}
