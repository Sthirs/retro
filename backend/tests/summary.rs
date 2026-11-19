//! The summary of an ended meeting over REST (spec §8.3, §12, §14.3): facts,
//! groups by votes with their anonymous reflections, tasks with their
//! current board status.

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

async fn user_id(app: &TestApp, token: &str) -> Uuid {
    let me = app.get_with_token("/api/v1/me", token).await.json();
    me["user"]["id"].as_str().unwrap().parse().unwrap()
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

fn ok(res: TestResponse) -> Value {
    assert_eq!(res.status, 200, "{}", res.text());
    res.json()
}

fn fails(res: &TestResponse, status: u16, expected: &str) {
    assert_eq!(res.status, status, "{}", res.text());
    assert_eq!(res.json()["code"], expected, "{}", res.text());
}

async fn state(app: &TestApp, token: &str, meeting: &str) -> Value {
    ok(app
        .call(
            "GET",
            &format!("/api/v1/meetings/{meeting}/state"),
            token,
            None,
        )
        .await)
}

async fn summary(app: &TestApp, token: &str, meeting: &str) -> TestResponse {
    app.call(
        "GET",
        &format!("/api/v1/meetings/{meeting}/summary"),
        token,
        None,
    )
    .await
}

async fn next_phase(app: &TestApp, token: &str, meeting: &str) {
    ok(post(
        app,
        token,
        &format!("/api/v1/meetings/{meeting}/phase/next"),
        None,
    )
    .await);
}

async fn end(app: &TestApp, token: &str, meeting: &str) {
    ok(post(app, token, &format!("/api/v1/meetings/{meeting}/end"), None).await);
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
        ok(post(
            app,
            token,
            &format!("/api/v1/groups/{group}/vote"),
            Some(json!({ "delta": 1 })),
        )
        .await);
    }
}

/// Groups `card` with `target` and names the new group.
async fn pair(app: &TestApp, token: &str, meeting: &str, target: &str, card: &str, name: &str) {
    let res = ok(post(
        app,
        token,
        &format!("/api/v1/meetings/{meeting}/groups"),
        Some(json!({ "targetId": target, "reflectionId": card })),
    )
    .await);
    let group = res["groups"][0]["id"].as_str().unwrap().to_owned();
    ok(app
        .call(
            "PATCH",
            &format!("/api/v1/groups/{group}"),
            token,
            Some(json!({ "name": name })),
        )
        .await);
}

/// alice (admin, facilitator), bob and erin (participants, joined in that
/// order); carol is a member who never joined.
struct Room {
    alice: String,
    bob: String,
    erin: String,
    carol: String,
    team: Uuid,
    meeting: String,
}

async fn room(app: &TestApp) -> Room {
    let alice = app.token_for("alice").await;
    let bob = app.token_for("bob").await;
    let erin = app
        .create_user("ezra", "ezra@example.com", "Erin Galli")
        .await;
    let carol = app
        .create_user("cleo", "cleo@example.com", "Carol Verdi")
        .await;
    let team = create_team(app, &alice, "Platform").await;
    for token in [&bob, &erin, &carol] {
        join_team(app, &alice, team, token).await;
    }
    let res = post(app, &alice, &format!("/api/v1/teams/{team}/meetings"), None).await;
    assert_eq!(res.status, 201, "{}", res.text());
    let meeting = res.json()["id"].as_str().unwrap().to_owned();
    for token in [&bob, &erin] {
        ok(post(
            app,
            token,
            &format!("/api/v1/meetings/{meeting}/join"),
            None,
        )
        .await);
    }
    Room {
        alice,
        bob,
        erin,
        carol,
        team,
        meeting,
    }
}

/// The room in Discuss. Cards: p1 + p2 grouped as "Pair", and the single
/// cards d1, i1 and t1. Votes: d1 3 (bob), Pair 2 (erin), i1 2 (alice), t1
/// none, so the order is d1, Pair (2 cards beat i1's one), i1, t1.
async fn discussing(app: &TestApp) -> Room {
    let r = room(app).await;
    let m = &r.meeting;
    next_phase(app, &r.alice, m).await;
    let p1 = sent(app, &r.alice, m, "plus", "Release train").await;
    let d1 = sent(app, &r.bob, m, "delta", "Flaky <strong>tests</strong>").await;
    let p2 = sent(app, &r.erin, m, "plus", "Pairing").await;
    let i1 = sent(app, &r.bob, m, "ideas", "Quarantine").await;
    let t1 = sent(app, &r.erin, m, "thanks", "Thanks Marco").await;
    next_phase(app, &r.alice, m).await;
    pair(app, &r.alice, m, &p1, &p2, "Pair").await;
    next_phase(app, &r.alice, m).await;
    let seen = state(app, &r.alice, m).await;
    vote(app, &r.bob, &group_of(&seen, &d1), 3).await;
    vote(app, &r.erin, &group_of(&seen, &p1), 2).await;
    vote(app, &r.alice, &group_of(&seen, &i1), 2).await;
    let _ = t1;
    next_phase(app, &r.alice, m).await;
    r
}

/// Writes a task for the current Discuss group; returns its id.
async fn task(app: &TestApp, r: &Room, content: &str) -> String {
    let seen = state(app, &r.alice, &r.meeting).await;
    let group = seen["discussGroupId"].as_str().unwrap();
    let res = app
        .discuss_task(&r.alice, &r.meeting, group, content, &[])
        .await;
    assert_eq!(res.status, 201, "{}", res.text());
    TestApp::last_task(&res.json())
}

fn names(list: &Value) -> Vec<String> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|p| p["displayName"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn summarises_an_ended_meeting() {
    let app = TestApp::new().await;
    let r = discussing(&app).await;
    let bob_id = user_id(&app, &r.bob).await;
    let mention = format!(
        r#"<p>Quarantine them <span data-type="mention" data-id="{bob_id}" data-label="Bob">@Bob</span></p>"#
    );
    let first = task(&app, &r, &mention).await;
    let second = task(&app, &r, "<p>Split the pipeline</p>").await;
    end(&app, &r.alice, &r.meeting).await;

    let s = ok(summary(&app, &r.bob, &r.meeting).await);
    assert_eq!(s["id"], r.meeting.as_str());
    assert_eq!(s["teamId"], r.team.to_string());
    assert_eq!(s["teamName"], "Platform");
    assert_eq!(s["title"], "Retro #1");
    assert!(s["createdAt"].is_string());
    assert!(s["endedAt"].is_string());
    assert_eq!(s["facilitator"]["displayName"], "Alice Rossi");
    assert_eq!(
        names(&s["participants"]),
        vec!["Alice Rossi", "Bob Bianchi", "Erin Galli"]
    );
    assert_eq!(s["votesCast"], 7);

    // Groups by votes, ties to the bigger group; loose cards stand alone.
    let groups = s["groups"].as_array().unwrap();
    let votes: Vec<i64> = groups
        .iter()
        .map(|g| g["votes"].as_i64().unwrap())
        .collect();
    assert_eq!(votes, vec![3, 2, 2, 0]);
    let named: Vec<&str> = groups.iter().map(|g| g["name"].as_str().unwrap()).collect();
    assert_eq!(named, vec!["", "Pair", "", ""]);
    let contents: Vec<Vec<&str>> = groups
        .iter()
        .map(|g| {
            g["reflections"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| c["content"].as_str().unwrap())
                .collect()
        })
        .collect();
    assert_eq!(
        contents,
        vec![
            vec!["<p>Flaky <strong>tests</strong></p>"],
            vec!["<p>Release train</p>", "<p>Pairing</p>"],
            vec!["<p>Quarantine</p>"],
            vec!["<p>Thanks Marco</p>"],
        ]
    );
    assert_eq!(groups[0]["reflections"][0]["column"], "delta");
    assert_eq!(groups[3]["reflections"][0]["column"], "thanks");

    // Anonymous: a card holds its id, column and content, nothing else.
    for g in groups {
        for card in g["reflections"].as_array().unwrap() {
            let mut keys: Vec<&String> = card.as_object().unwrap().keys().collect();
            keys.sort();
            assert_eq!(keys, vec!["column", "content", "id"]);
        }
    }

    // Tasks, oldest first, with their mentions and status.
    let tasks = s["tasks"].as_array().unwrap();
    let ids: Vec<&str> = tasks.iter().map(|t| t["id"].as_str().unwrap()).collect();
    assert_eq!(ids, vec![first.as_str(), second.as_str()]);
    assert_eq!(tasks[0]["status"], "active");
    assert_eq!(names(&tasks[0]["mentions"]), vec!["Bob Bianchi"]);
    assert_eq!(tasks[0]["author"]["displayName"], "Alice Rossi");
    assert_eq!(tasks[0]["meetingId"], r.meeting.as_str());
    assert_eq!(tasks[1]["mentions"], json!([]));
}

#[tokio::test]
async fn any_member_reads_it_and_nobody_else() {
    let app = TestApp::new().await;
    let r = discussing(&app).await;
    end(&app, &r.alice, &r.meeting).await;

    // carol never joined the meeting, but is a member of its team.
    let s = ok(summary(&app, &r.carol, &r.meeting).await);
    assert_eq!(s["groups"].as_array().unwrap().len(), 4);

    let outsider = app
        .create_user("olga", "olga@example.com", "Olga Esposito")
        .await;
    fails(
        &summary(&app, &outsider, &r.meeting).await,
        404,
        "meeting_not_found",
    );
    fails(
        &summary(&app, &r.alice, &Uuid::now_v7().to_string()).await,
        404,
        "meeting_not_found",
    );
    let anonymous = app
        .get(&format!("/api/v1/meetings/{}/summary", r.meeting))
        .await;
    assert_eq!(anonymous.status, 401, "{}", anonymous.text());
}

#[tokio::test]
async fn a_running_meeting_has_no_summary() {
    let app = TestApp::new().await;
    let r = discussing(&app).await;
    fails(
        &summary(&app, &r.alice, &r.meeting).await,
        409,
        "meeting_active",
    );
    fails(
        &summary(&app, &r.carol, &r.meeting).await,
        409,
        "meeting_active",
    );
}

#[tokio::test]
async fn tasks_show_their_current_board_status() {
    let app = TestApp::new().await;
    let r = discussing(&app).await;
    let moved = task(&app, &r, "<p>Move me</p>").await;
    let edited = task(&app, &r, "<p>Edit me</p>").await;
    let deleted = task(&app, &r, "<p>Delete me</p>").await;
    end(&app, &r.alice, &r.meeting).await;

    // The board keeps changing after the meeting.
    ok(app
        .call(
            "PUT",
            &format!("/api/v1/tasks/{moved}/position"),
            &r.bob,
            Some(json!({ "status": "done" })),
        )
        .await);
    ok(app
        .call(
            "PATCH",
            &format!("/api/v1/tasks/{edited}"),
            &r.carol,
            Some(json!({ "content": "<p>Edited</p>" })),
        )
        .await);
    let res = app
        .call("DELETE", &format!("/api/v1/tasks/{deleted}"), &r.erin, None)
        .await;
    assert!(res.status == 200 || res.status == 204, "{}", res.text());

    let s = ok(summary(&app, &r.bob, &r.meeting).await);
    let tasks = s["tasks"].as_array().unwrap();
    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks[0]["id"], moved.as_str());
    assert_eq!(tasks[0]["status"], "done");
    assert_eq!(tasks[1]["id"], edited.as_str());
    assert_eq!(tasks[1]["status"], "active");
    assert_eq!(tasks[1]["contentHtml"], "<p>Edited</p>");
}

#[tokio::test]
async fn ended_during_reflect_reveals_nothing() {
    let app = TestApp::new().await;
    let r = room(&app).await;
    next_phase(&app, &r.alice, &r.meeting).await;
    sent(&app, &r.bob, &r.meeting, "delta", "Secret worry").await;
    end(&app, &r.alice, &r.meeting).await;

    for token in [&r.alice, &r.bob, &r.carol] {
        let res = summary(&app, token, &r.meeting).await;
        assert!(!res.text().contains("Secret worry"), "{}", res.text());
        let s = ok(res);
        assert_eq!(s["groups"], json!([]));
        assert_eq!(s["tasks"], json!([]));
        assert_eq!(s["votesCast"], 0);
        assert_eq!(s["participants"].as_array().unwrap().len(), 3);
    }
}

#[tokio::test]
async fn ended_during_icebreaker_is_empty() {
    let app = TestApp::new().await;
    let r = room(&app).await;
    end(&app, &r.alice, &r.meeting).await;
    let s = ok(summary(&app, &r.erin, &r.meeting).await);
    assert_eq!(s["groups"], json!([]));
    assert_eq!(s["tasks"], json!([]));
    assert_eq!(s["facilitator"]["displayName"], "Alice Rossi");
}

#[tokio::test]
async fn ended_during_group_shows_loose_cards_alone() {
    let app = TestApp::new().await;
    let r = room(&app).await;
    let m = &r.meeting;
    next_phase(&app, &r.alice, m).await;
    let a = sent(&app, &r.alice, m, "ideas", "Loose idea").await;
    let b = sent(&app, &r.bob, m, "plus", "Pair one").await;
    let c = sent(&app, &r.erin, m, "delta", "Pair two").await;
    next_phase(&app, &r.alice, m).await;
    pair(&app, &r.alice, m, &b, &c, "Twins").await;
    end(&app, &r.alice, m).await;

    let s = ok(summary(&app, &r.bob, m).await);
    let groups = s["groups"].as_array().unwrap();
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0]["name"], "Twins");
    assert_eq!(groups[0]["votes"], 0);
    let cards: Vec<&str> = groups[0]["reflections"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    // By column: Plus before Delta.
    assert_eq!(cards, vec![b.as_str(), c.as_str()]);
    assert_eq!(groups[1]["name"], "");
    assert_eq!(groups[1]["id"], a.as_str());
    assert_eq!(groups[1]["votes"], 0);
    assert_eq!(groups[1]["reflections"][0]["content"], "<p>Loose idea</p>");
    assert_eq!(s["votesCast"], 0);
}

#[tokio::test]
async fn people_who_left_stay_listed_but_lose_access() {
    let app = TestApp::new().await;
    let r = discussing(&app).await;
    ok(app
        .call(
            "PATCH",
            &format!("/api/v1/meetings/{}", r.meeting),
            &r.alice,
            Some(json!({ "title": "Sprint 42" })),
        )
        .await);
    end(&app, &r.alice, &r.meeting).await;
    let erin_id = user_id(&app, &r.erin).await;
    let res = app
        .call(
            "DELETE",
            &format!("/api/v1/teams/{}/members/{erin_id}", r.team),
            &r.erin,
            None,
        )
        .await;
    assert!(res.status == 200 || res.status == 204, "{}", res.text());

    let s = ok(summary(&app, &r.bob, &r.meeting).await);
    assert_eq!(s["title"], "Sprint 42");
    assert_eq!(
        names(&s["participants"]),
        vec!["Alice Rossi", "Bob Bianchi", "Erin Galli"]
    );
    fails(
        &summary(&app, &r.erin, &r.meeting).await,
        404,
        "meeting_not_found",
    );
}
