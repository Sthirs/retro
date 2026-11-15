//! Meetings: start, list, join, icebreaker, phases, facilitator, end and the
//! meeting invite link (spec §6.3, §8, §9.1, §12, §14.3).

mod common;

use std::time::Duration;

use common::TestApp;
use sea_orm::{ConnectionTrait, DbBackend, Statement, TransactionTrait};
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

async fn start(app: &TestApp, token: &str, team: Uuid) -> Value {
    let res = app
        .call(
            "POST",
            &format!("/api/v1/teams/{team}/meetings"),
            token,
            None,
        )
        .await;
    assert_eq!(res.status, 201, "{}", res.text());
    res.json()
}

fn id(meeting: &Value) -> &str {
    meeting["id"].as_str().unwrap()
}

/// `POST /api/v1/meetings/{id}/{action}`.
async fn act(
    app: &TestApp,
    token: &str,
    meeting: &str,
    action: &str,
    body: Option<Value>,
) -> common::TestResponse {
    app.call(
        "POST",
        &format!("/api/v1/meetings/{meeting}/{action}"),
        token,
        body,
    )
    .await
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

/// alice (admin) with bob and carol (members) in one team.
async fn team_of_three(app: &TestApp) -> (String, String, String, Uuid) {
    let alice = app.token_for("alice").await;
    let bob = app.token_for("bob").await;
    let carol = app
        .create_user("carol", "carol@example.com", "Carol Verdi")
        .await;
    let team = create_team(app, &alice, "Platform").await;
    join_team(app, &alice, team, &bob).await;
    join_team(app, &alice, team, &carol).await;
    (alice, bob, carol, team)
}

#[tokio::test]
async fn starting_a_meeting_makes_the_creator_facilitator() {
    let app = TestApp::new().await;
    let alice = app.token_for("alice").await;
    let alice_id = user_id(&app, &alice).await;
    let team = create_team(&app, &alice, "Platform").await;

    let meeting = start(&app, &alice, team).await;
    assert_eq!(meeting["title"], "Retro #1");
    assert_eq!(meeting["teamName"], "Platform");
    assert_eq!(meeting["status"], "active");
    assert_eq!(meeting["phase"], "icebreaker");
    assert_eq!(meeting["facilitatorId"], alice_id.to_string());
    assert_eq!(meeting["participants"].as_array().unwrap().len(), 1);
    assert_eq!(meeting["participants"][0]["role"], "admin");
    assert_eq!(meeting["icebreaker"]["order"], json!([alice_id]));
    assert_eq!(meeting["icebreaker"]["index"], 0);
    assert!(!meeting["greeting"]["text"].as_str().unwrap().is_empty());
    assert!(!meeting["greeting"]["language"].as_str().unwrap().is_empty());
    assert!(!meeting["greeting"]["langCode"].as_str().unwrap().is_empty());
    assert!(
        meeting["greeting"]["pronunciation"]
            .as_str()
            .unwrap()
            .starts_with('/')
    );
    assert!(!meeting["question"].as_str().unwrap().is_empty());

    let second = start(&app, &alice, team).await;
    assert_eq!(second["title"], "Retro #2");
    // Several meetings may run at once; picks avoid the team's recent ones.
    assert_ne!(second["question"], meeting["question"]);
    assert_ne!(
        second["greeting"]["language"],
        meeting["greeting"]["language"]
    );
}

#[tokio::test]
async fn seed_holds_fifty_greetings_and_questions() {
    use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};
    let app = TestApp::new().await;
    let greetings = retro_backend::entities::greeting::Entity::find()
        .count(&app.db)
        .await
        .unwrap();
    let questions = retro_backend::entities::icebreaker_question::Entity::find()
        .count(&app.db)
        .await
        .unwrap();
    assert_eq!((greetings, questions), (50, 50));

    // Every greeting has its pronunciation.
    let missing = retro_backend::entities::greeting::Entity::find()
        .filter(retro_backend::entities::greeting::Column::Pronunciation.eq(""))
        .count(&app.db)
        .await
        .unwrap();
    assert_eq!(missing, 0);
}

#[tokio::test]
async fn lists_active_and_ended_meetings() {
    let app = TestApp::new().await;
    let (alice, bob, _, team) = team_of_three(&app).await;
    let first = start(&app, &alice, team).await;
    let second = start(&app, &bob, team).await;
    assert_eq!(act(&app, &alice, id(&first), "end", None).await.status, 200);

    let list = |q: &'static str| {
        let app = &app;
        let bob = &bob;
        async move {
            let res = app
                .call(
                    "GET",
                    &format!("/api/v1/teams/{team}/meetings{q}"),
                    bob,
                    None,
                )
                .await;
            assert_eq!(res.status, 200, "{}", res.text());
            res.json()
        }
    };
    let all = list("").await;
    assert_eq!(all.as_array().unwrap().len(), 2);
    assert_eq!(all[0]["id"], second["id"]);

    let active = list("?status=active").await;
    assert_eq!(active.as_array().unwrap().len(), 1);
    assert_eq!(active[0]["title"], "Retro #2");
    assert_eq!(active[0]["facilitator"]["displayName"], "Bob Bianchi");
    assert_eq!(active[0]["participants"][0]["displayName"], "Bob Bianchi");
    assert_eq!(active[0]["participants"][0]["online"], false);
    assert_eq!(active[0]["taskCount"], 0);
    assert_eq!(active[0]["endedAt"], Value::Null);

    let ended = list("?status=ended").await;
    assert_eq!(ended.as_array().unwrap().len(), 1);
    assert_eq!(ended[0]["status"], "ended");
    assert!(ended[0]["endedAt"].is_string());
}

#[tokio::test]
async fn non_members_cannot_see_or_start_meetings() {
    let app = TestApp::new().await;
    let alice = app.token_for("alice").await;
    let bob = app.token_for("bob").await;
    let team = create_team(&app, &alice, "Platform").await;
    let meeting = start(&app, &alice, team).await;

    let res = app
        .call(
            "POST",
            &format!("/api/v1/teams/{team}/meetings"),
            &bob,
            None,
        )
        .await;
    assert_eq!(res.status, 404);
    assert_eq!(res.json()["code"], "team_not_found");
    let res = app
        .call("GET", &format!("/api/v1/teams/{team}/meetings"), &bob, None)
        .await;
    assert_eq!(res.status, 404);
    for path in ["state", "invite-link"] {
        let res = app
            .call(
                "GET",
                &format!("/api/v1/meetings/{}/{path}", id(&meeting)),
                &bob,
                None,
            )
            .await;
        assert_eq!(res.status, 404, "{path}");
        assert_eq!(res.json()["code"], "meeting_not_found");
    }
    let res = act(&app, &bob, id(&meeting), "join", None).await;
    assert_eq!(res.status, 404);
}

#[tokio::test]
async fn joining_is_idempotent_and_adds_people_to_the_icebreaker() {
    let app = TestApp::new().await;
    let (alice, bob, carol, team) = team_of_three(&app).await;
    let (alice_id, bob_id, carol_id) = (
        user_id(&app, &alice).await,
        user_id(&app, &bob).await,
        user_id(&app, &carol).await,
    );
    let meeting = start(&app, &alice, team).await;
    let m = id(&meeting);

    // While alice (first) speaks, newcomers land among those not yet heard.
    let res = act(&app, &bob, m, "join", None).await;
    assert_eq!(res.status, 200, "{}", res.text());
    assert_eq!(act(&app, &bob, m, "join", None).await.status, 200);
    let s = state(&app, &bob, m).await;
    assert_eq!(s["participants"].as_array().unwrap().len(), 2);
    assert_eq!(s["icebreaker"]["order"], json!([alice_id, bob_id]));

    // After the first person, newcomers go to the end.
    assert_eq!(
        act(&app, &alice, m, "icebreaker/next", None).await.status,
        200
    );
    assert_eq!(act(&app, &carol, m, "join", None).await.status, 200);
    let s = state(&app, &carol, m).await;
    assert_eq!(
        s["icebreaker"]["order"],
        json!([alice_id, bob_id, carol_id])
    );
    assert_eq!(s["icebreaker"]["index"], 1);
}

#[tokio::test]
async fn the_facilitator_moves_through_the_icebreaker() {
    let app = TestApp::new().await;
    let (alice, bob, carol, team) = team_of_three(&app).await;
    let meeting = start(&app, &alice, team).await;
    let m = id(&meeting);
    act(&app, &bob, m, "join", None).await;
    act(&app, &carol, m, "join", None).await;

    // Only the facilitator.
    let res = act(&app, &bob, m, "icebreaker/next", None).await;
    assert_eq!(res.status, 403);
    assert_eq!(res.json()["code"], "not_facilitator");

    let res = act(&app, &alice, m, "icebreaker/next", None).await;
    assert_eq!(res.status, 200, "{}", res.text());
    assert_eq!(res.json()["icebreaker"]["index"], 1);
    let s = act(&app, &alice, m, "icebreaker/next", None).await.json();
    assert_eq!(s["icebreaker"]["index"], 2);

    // The last person is speaking: next is the Reflect phase.
    let res = act(&app, &alice, m, "icebreaker/next", None).await;
    assert_eq!(res.status, 409);
    assert_eq!(res.json()["code"], "icebreaker_done");
}

#[tokio::test]
async fn the_facilitator_goes_back_in_the_icebreaker() {
    let app = TestApp::new().await;
    let (alice, bob, carol, team) = team_of_three(&app).await;
    let meeting = start(&app, &alice, team).await;
    let m = id(&meeting);
    act(&app, &bob, m, "join", None).await;
    act(&app, &carol, m, "join", None).await;
    let order = state(&app, &alice, m).await["icebreaker"]["order"].clone();
    let goto = |who: &Value| Some(json!({ "userId": who }));

    // Nobody has had their turn yet.
    let res = act(&app, &alice, m, "icebreaker/goto", goto(&order[0])).await;
    assert_eq!(res.status, 409);
    assert_eq!(res.json()["code"], "icebreaker_not_reached");

    act(&app, &alice, m, "icebreaker/next", None).await;
    act(&app, &alice, m, "icebreaker/next", None).await;

    // Only the facilitator; only people who already had their turn.
    let res = act(&app, &bob, m, "icebreaker/goto", goto(&order[0])).await;
    assert_eq!(res.status, 403);
    assert_eq!(res.json()["code"], "not_facilitator");
    let res = act(&app, &alice, m, "icebreaker/goto", goto(&order[2])).await;
    assert_eq!(res.status, 409);
    assert_eq!(res.json()["code"], "icebreaker_not_reached");

    // Previous person.
    let res = act(&app, &alice, m, "icebreaker/goto", goto(&order[1])).await;
    assert_eq!(res.status, 200, "{}", res.text());
    assert_eq!(res.json()["icebreaker"]["index"], 1);

    // Back to the first, then the turn continues from there.
    let res = act(&app, &alice, m, "icebreaker/goto", goto(&order[0])).await;
    assert_eq!(res.json()["icebreaker"]["index"], 0);
    let s = act(&app, &alice, m, "icebreaker/next", None).await.json();
    assert_eq!(s["icebreaker"]["index"], 1);
    assert_eq!(s["icebreaker"]["order"], order);

    // Not once the icebreaker is over.
    act(&app, &alice, m, "phase/next", None).await;
    let res = act(&app, &alice, m, "icebreaker/goto", goto(&order[0])).await;
    assert_eq!(res.status, 409);
    assert_eq!(res.json()["code"], "wrong_phase");
}

#[tokio::test]
async fn phases_advance_forward_only_by_the_facilitator() {
    let app = TestApp::new().await;
    let (alice, bob, _, team) = team_of_three(&app).await;
    let meeting = start(&app, &alice, team).await;
    let m = id(&meeting);
    act(&app, &bob, m, "join", None).await;

    let res = act(&app, &bob, m, "phase/next", None).await;
    assert_eq!(res.status, 403);

    for phase in ["reflect", "group", "vote", "discuss"] {
        let res = act(&app, &alice, m, "phase/next", None).await;
        assert_eq!(res.status, 200, "{}", res.text());
        assert_eq!(res.json()["phase"], phase);
    }
    let res = act(&app, &alice, m, "phase/next", None).await;
    assert_eq!(res.status, 409);
    assert_eq!(res.json()["code"], "last_phase");

    let res = act(&app, &alice, m, "icebreaker/next", None).await;
    assert_eq!(res.status, 409);
    assert_eq!(res.json()["code"], "wrong_phase");
}

#[tokio::test]
async fn the_facilitator_hands_the_role_to_a_participant() {
    let app = TestApp::new().await;
    let (alice, bob, carol, team) = team_of_three(&app).await;
    let (bob_id, carol_id) = (user_id(&app, &bob).await, user_id(&app, &carol).await);
    let meeting = start(&app, &alice, team).await;
    let m = id(&meeting);
    act(&app, &bob, m, "join", None).await;

    // carol has not joined.
    let res = act(
        &app,
        &alice,
        m,
        "facilitator",
        Some(json!({ "userId": carol_id })),
    )
    .await;
    assert_eq!(res.status, 400);
    assert_eq!(res.json()["code"], "target_not_participant");

    let res = act(
        &app,
        &alice,
        m,
        "facilitator",
        Some(json!({ "userId": bob_id })),
    )
    .await;
    assert_eq!(res.status, 200, "{}", res.text());
    assert_eq!(res.json()["facilitatorId"], bob_id.to_string());

    // alice stays a participant without controls.
    let res = act(&app, &alice, m, "icebreaker/next", None).await;
    assert_eq!(res.status, 403);
    let s = state(&app, &alice, m).await;
    assert_eq!(s["participants"].as_array().unwrap().len(), 2);

    // A plain member cannot take over.
    act(&app, &carol, m, "join", None).await;
    let res = act(
        &app,
        &carol,
        m,
        "facilitator",
        Some(json!({ "userId": carol_id })),
    )
    .await;
    assert_eq!(res.status, 403);
    assert_eq!(res.json()["code"], "not_facilitator");
}

#[tokio::test]
async fn a_team_admin_takes_over_facilitation() {
    let app = TestApp::new().await;
    let (alice, bob, _, team) = team_of_three(&app).await;
    let alice_id = user_id(&app, &alice).await;
    let meeting = start(&app, &bob, team).await;
    let m = id(&meeting);

    // alice (admin) has not even joined: taking over makes her a participant.
    let res = act(
        &app,
        &alice,
        m,
        "facilitator",
        Some(json!({ "userId": alice_id })),
    )
    .await;
    assert_eq!(res.status, 200, "{}", res.text());
    let s = res.json();
    assert_eq!(s["facilitatorId"], alice_id.to_string());
    assert_eq!(s["participants"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn the_facilitator_renames_the_meeting() {
    let app = TestApp::new().await;
    let (alice, bob, _, team) = team_of_three(&app).await;
    let meeting = start(&app, &alice, team).await;
    let m = id(&meeting);
    let path = format!("/api/v1/meetings/{m}");

    let res = app
        .call(
            "PATCH",
            &path,
            &alice,
            Some(json!({ "title": "  Incident review  " })),
        )
        .await;
    assert_eq!(res.status, 200, "{}", res.text());
    assert_eq!(res.json()["title"], "Incident review");

    let res = app
        .call("PATCH", &path, &alice, Some(json!({ "title": " " })))
        .await;
    assert_eq!(res.status, 400);
    assert_eq!(res.json()["code"], "invalid_title");
    let res = app
        .call(
            "PATCH",
            &path,
            &alice,
            Some(json!({ "title": "x".repeat(81) })),
        )
        .await;
    assert_eq!(res.status, 400);

    act(&app, &bob, m, "join", None).await;
    let res = app
        .call("PATCH", &path, &bob, Some(json!({ "title": "Mine" })))
        .await;
    assert_eq!(res.status, 403);
}

#[tokio::test]
async fn ending_makes_the_meeting_read_only() {
    let app = TestApp::new().await;
    let (alice, bob, _, team) = team_of_three(&app).await;
    let meeting = start(&app, &alice, team).await;
    let m = id(&meeting);
    act(&app, &bob, m, "join", None).await;

    let res = act(&app, &bob, m, "end", None).await;
    assert_eq!(res.status, 403);

    let res = act(&app, &alice, m, "end", None).await;
    assert_eq!(res.status, 200, "{}", res.text());
    let s = res.json();
    assert_eq!(s["status"], "ended");
    assert!(s["endedAt"].is_string());

    for (action, body) in [
        ("end", None),
        ("phase/next", None),
        ("icebreaker/next", None),
        ("icebreaker/goto", Some(json!({ "userId": Uuid::nil() }))),
        ("join", None),
    ] {
        let res = act(&app, &alice, m, action, body).await;
        assert_eq!(res.status, 409, "{action}");
        assert_eq!(res.json()["code"], "meeting_ended", "{action}");
    }
    let res = app
        .call(
            "GET",
            &format!("/api/v1/meetings/{m}/invite-link"),
            &alice,
            None,
        )
        .await;
    assert_eq!(res.status, 409);
    // Ended meetings stay readable.
    assert_eq!(state(&app, &bob, m).await["status"], "ended");
}

async fn invite_token(app: &TestApp, token: &str, meeting: &str) -> String {
    let res = app
        .call(
            "GET",
            &format!("/api/v1/meetings/{meeting}/invite-link"),
            token,
            None,
        )
        .await;
    assert_eq!(res.status, 200, "{}", res.text());
    let url = res.json()["url"].as_str().unwrap().to_owned();
    assert!(url.contains("/m/"), "{url}");
    url.rsplit('/').next().unwrap().to_owned()
}

#[tokio::test]
async fn the_invite_link_adds_strangers_to_the_team_and_the_meeting() {
    let app = TestApp::new().await;
    let (alice, bob, _, team) = team_of_three(&app).await;
    let dave = app
        .create_user("dave", "dave@example.com", "Dave Costa")
        .await;
    let meeting = start(&app, &alice, team).await;
    let m = id(&meeting);
    // Any member may copy the link.
    let token = invite_token(&app, &bob, m).await;

    let res = app
        .call(
            "POST",
            &format!("/api/v1/meeting-invites/{token}/accept"),
            &dave,
            None,
        )
        .await;
    assert_eq!(res.status, 200, "{}", res.text());
    let body = res.json();
    assert_eq!(body["meetingId"], m);
    assert_eq!(body["teamId"], team.to_string());
    assert_eq!(body["title"], "Retro #1");
    assert_eq!(body["joinedTeam"], true);
    let s = state(&app, &dave, m).await;
    assert_eq!(s["participants"].as_array().unwrap().len(), 2);
    assert_eq!(s["participants"][1]["role"], "member");

    // Members follow it too, without joining the team again.
    let res = app
        .call(
            "POST",
            &format!("/api/v1/meeting-invites/{token}/accept"),
            &bob,
            None,
        )
        .await;
    assert_eq!(res.json()["joinedTeam"], false);

    // Regenerating (facilitator only) invalidates the old link.
    let res = act(&app, &bob, m, "invite-link", None).await;
    assert_eq!(res.status, 403);
    let res = act(&app, &alice, m, "invite-link", None).await;
    assert_eq!(res.status, 200);
    let res = app
        .call(
            "POST",
            &format!("/api/v1/meeting-invites/{token}/accept"),
            &bob,
            None,
        )
        .await;
    assert_eq!(res.status, 404);
    assert_eq!(res.json()["code"], "meeting_invite_invalid");

    // Ending invalidates the current one.
    let token = invite_token(&app, &alice, m).await;
    act(&app, &alice, m, "end", None).await;
    let res = app
        .call(
            "POST",
            &format!("/api/v1/meeting-invites/{token}/accept"),
            &bob,
            None,
        )
        .await;
    assert_eq!(res.status, 404);
}

#[tokio::test]
async fn ws_tickets_need_a_signed_in_user() {
    let app = TestApp::new().await;
    let alice = app.token_for("alice").await;
    let res = app.call("POST", "/api/v1/ws-ticket", &alice, None).await;
    assert_eq!(res.status, 200, "{}", res.text());
    assert!(res.json()["ticket"].as_str().is_some_and(|t| !t.is_empty()));
    assert!(res.json()["expiresAt"].is_string());

    let res = app.call("POST", "/api/v1/ws-ticket", "nope", None).await;
    assert_eq!(res.status, 401);
}

// --- Lock order (team row first, then the meeting row) ---------------------

/// `app.call` with an owned path, for requests built ahead of time.
async fn call_at(
    app: &TestApp,
    method: &str,
    path: String,
    token: &str,
    body: Option<Value>,
) -> common::TestResponse {
    app.call(method, &path, token, body).await
}

/// Whether a request of this test's database waits for a row lock.
async fn someone_waits_for_a_lock(app: &TestApp) -> bool {
    let row = app
        .db
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT count(*) AS waiting FROM pg_stat_activity \
             WHERE datname = current_database() AND wait_event_type = 'Lock'",
        ))
        .await
        .unwrap()
        .unwrap();
    row.try_get::<i64>("", "waiting").unwrap() > 0
}

/// Runs `request` while another transaction holds the team row, and checks
/// that the request waits for the team without holding the meeting row: the
/// order of a team delete (team row, then its meetings through the cascade),
/// so the two cannot deadlock.
async fn takes_the_team_before_the_meeting(
    app: &TestApp,
    team: Uuid,
    meeting: &str,
    request: impl std::future::Future<Output = common::TestResponse>,
) -> common::TestResponse {
    let meeting: Uuid = meeting.parse().unwrap();
    let holder = app.db.begin().await.unwrap();
    holder
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT id FROM teams WHERE id = $1 FOR UPDATE",
            [team.into()],
        ))
        .await
        .unwrap();
    let check = async move {
        let mut waiting = false;
        for _ in 0..500 {
            if someone_waits_for_a_lock(app).await {
                waiting = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(waiting, "the request never waited for the team row");
        let meeting_row = holder
            .execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "SELECT id FROM meetings WHERE id = $1 FOR UPDATE NOWAIT",
                [meeting.into()],
            ))
            .await;
        assert!(
            meeting_row.is_ok(),
            "the request holds the meeting row while it waits for the team: {meeting_row:?}"
        );
        holder.rollback().await.unwrap();
    };
    let (res, ()) = tokio::join!(request, check);
    res
}

/// A team (alice admin, bob member) with a running meeting alice facilitates.
async fn fresh_meeting(app: &TestApp, alice: &str, bob: &str, name: &str) -> (Uuid, String) {
    let team = create_team(app, alice, name).await;
    join_team(app, alice, team, bob).await;
    let meeting = start(app, alice, team).await;
    (team, id(&meeting).to_owned())
}

#[tokio::test]
async fn meeting_writes_lock_the_team_before_the_meeting() {
    let app = TestApp::new().await;
    let alice = app.token_for("alice").await;
    let bob = app.token_for("bob").await;
    let alice_id = user_id(&app, &alice).await;
    let bob_id = user_id(&app, &bob).await;
    let dave = app
        .create_user("dave", "dave@example.com", "Dave Costa")
        .await;

    // Joining.
    let (team, m) = fresh_meeting(&app, &alice, &bob, "Join").await;
    let res =
        takes_the_team_before_the_meeting(&app, team, &m, act(&app, &bob, &m, "join", None)).await;
    assert_eq!(res.status, 200, "join: {}", res.text());

    // The icebreaker: next person and going back.
    let (team, m) = fresh_meeting(&app, &alice, &bob, "Next").await;
    act(&app, &bob, &m, "join", None).await;
    let res = takes_the_team_before_the_meeting(
        &app,
        team,
        &m,
        act(&app, &alice, &m, "icebreaker/next", None),
    )
    .await;
    assert_eq!(res.status, 200, "next: {}", res.text());
    let first = state(&app, &alice, &m).await["icebreaker"]["order"][0].clone();
    let res = takes_the_team_before_the_meeting(
        &app,
        team,
        &m,
        act(
            &app,
            &alice,
            &m,
            "icebreaker/goto",
            Some(json!({ "userId": first })),
        ),
    )
    .await;
    assert_eq!(res.status, 200, "goto: {}", res.text());

    // Phase, handover, rename, invite link, end.
    let (team, m) = fresh_meeting(&app, &alice, &bob, "Phase").await;
    act(&app, &bob, &m, "join", None).await;
    let res = takes_the_team_before_the_meeting(
        &app,
        team,
        &m,
        act(&app, &alice, &m, "phase/next", None),
    )
    .await;
    assert_eq!(res.status, 200, "phase: {}", res.text());
    let res = takes_the_team_before_the_meeting(
        &app,
        team,
        &m,
        act(
            &app,
            &alice,
            &m,
            "facilitator",
            Some(json!({ "userId": bob_id })),
        ),
    )
    .await;
    assert_eq!(res.status, 200, "handover: {}", res.text());
    // alice, an admin, takes it back.
    let res = takes_the_team_before_the_meeting(
        &app,
        team,
        &m,
        act(
            &app,
            &alice,
            &m,
            "facilitator",
            Some(json!({ "userId": alice_id })),
        ),
    )
    .await;
    assert_eq!(res.status, 200, "take over: {}", res.text());
    let rename = call_at(
        &app,
        "PATCH",
        format!("/api/v1/meetings/{m}"),
        &alice,
        Some(json!({ "title": "Locked" })),
    );
    let res = takes_the_team_before_the_meeting(&app, team, &m, rename).await;
    assert_eq!(res.status, 200, "rename: {}", res.text());
    let res = takes_the_team_before_the_meeting(
        &app,
        team,
        &m,
        act(&app, &alice, &m, "invite-link", None),
    )
    .await;
    assert_eq!(res.status, 200, "invite link: {}", res.text());
    let token = invite_token(&app, &alice, &m).await;
    let accept = call_at(
        &app,
        "POST",
        format!("/api/v1/meeting-invites/{token}/accept"),
        &dave,
        None,
    );
    let res = takes_the_team_before_the_meeting(&app, team, &m, accept).await;
    assert_eq!(res.status, 200, "accept invite: {}", res.text());
    let res =
        takes_the_team_before_the_meeting(&app, team, &m, act(&app, &alice, &m, "end", None)).await;
    assert_eq!(res.status, 200, "end: {}", res.text());
}

#[tokio::test]
async fn deleting_a_team_while_its_meeting_changes_never_fails() {
    let app = TestApp::new().await;
    let alice = app.token_for("alice").await;
    let bob = app.token_for("bob").await;
    let bob_id = user_id(&app, &bob).await;
    for round in 0..20 {
        let (team, m) = fresh_meeting(&app, &alice, &bob, &format!("Race {round}")).await;
        let token = invite_token(&app, &alice, &m).await;
        let delete = call_at(
            &app,
            "DELETE",
            format!("/api/v1/teams/{team}"),
            &alice,
            None,
        );
        let writes = futures_util::future::join_all([
            Box::pin(act(&app, &bob, &m, "join", None))
                as std::pin::Pin<Box<dyn std::future::Future<Output = common::TestResponse>>>,
            Box::pin(act(&app, &alice, &m, "phase/next", None)),
            Box::pin(act(&app, &alice, &m, "invite-link", None)),
            Box::pin(act(
                &app,
                &alice,
                &m,
                "facilitator",
                Some(json!({ "userId": bob_id })),
            )),
            Box::pin(call_at(
                &app,
                "PATCH",
                format!("/api/v1/meetings/{m}"),
                &alice,
                Some(json!({ "title": "Racing" })),
            )),
            Box::pin(call_at(
                &app,
                "POST",
                format!("/api/v1/meeting-invites/{token}/accept"),
                &bob,
                None,
            )),
            Box::pin(act(&app, &alice, &m, "end", None)),
        ]);
        let (deleted, results) = tokio::join!(delete, writes);
        assert_eq!(deleted.status, 204, "round {round}: {}", deleted.text());
        for res in results {
            assert!(
                [200, 204, 400, 403, 404, 409].contains(&res.status),
                "round {round}: {} {}",
                res.status,
                res.text()
            );
        }
    }
}

// --- Leaving the team ------------------------------------------------------

/// `DELETE /teams/{team}/members/{user}` by `token` (removal, or leaving).
async fn remove(app: &TestApp, token: &str, team: Uuid, user: Uuid) -> common::TestResponse {
    app.call(
        "DELETE",
        &format!("/api/v1/teams/{team}/members/{user}"),
        token,
        None,
    )
    .await
}

async fn accept(app: &TestApp, token: &str, invite: &str) -> common::TestResponse {
    app.call(
        "POST",
        &format!("/api/v1/meeting-invites/{invite}/accept"),
        token,
        None,
    )
    .await
}

#[tokio::test]
async fn a_removed_member_cannot_rejoin_with_a_meeting_link_they_knew() {
    let app = TestApp::new().await;
    let (alice, _, _, team) = team_of_three(&app).await;
    let dave = app
        .create_user("dave", "dave@example.com", "Dave Costa")
        .await;
    let dave_id = user_id(&app, &dave).await;
    let meeting = start(&app, &alice, team).await;
    let m = id(&meeting);
    let token = invite_token(&app, &alice, m).await;
    let res = accept(&app, &dave, &token).await;
    assert_eq!(res.json()["joinedTeam"], true);

    let res = remove(&app, &alice, team, dave_id).await;
    assert_eq!(res.status, 204, "{}", res.text());

    let res = accept(&app, &dave, &token).await;
    assert_eq!(res.status, 404, "{}", res.text());
    assert_eq!(res.json()["code"], "meeting_invite_invalid");
    // Still out of the team and the meeting.
    let res = app
        .call("GET", &format!("/api/v1/meetings/{m}/state"), &dave, None)
        .await;
    assert_eq!(res.status, 404);
    let res = app
        .call(
            "GET",
            &format!("/api/v1/teams/{team}/meetings"),
            &dave,
            None,
        )
        .await;
    assert_eq!(res.status, 404);
}

#[tokio::test]
async fn removing_a_member_regenerates_only_the_active_meetings_links() {
    let app = TestApp::new().await;
    let (alice, _, carol, team) = team_of_three(&app).await;
    let carol_id = user_id(&app, &carol).await;
    let dave = app
        .create_user("dave", "dave@example.com", "Dave Costa")
        .await;
    let first = start(&app, &alice, team).await;
    let ended = start(&app, &alice, team).await;
    act(&app, &alice, id(&ended), "end", None).await;
    let second = start(&app, &alice, team).await;
    let other_team = create_team(&app, &alice, "Elsewhere").await;
    let other = start(&app, &alice, other_team).await;

    let first_before = invite_token(&app, &alice, id(&first)).await;
    let second_before = invite_token(&app, &alice, id(&second)).await;
    let other_before = invite_token(&app, &alice, id(&other)).await;

    let res = remove(&app, &alice, team, carol_id).await;
    assert_eq!(res.status, 204, "{}", res.text());

    let first_after = invite_token(&app, &alice, id(&first)).await;
    let second_after = invite_token(&app, &alice, id(&second)).await;
    assert_ne!(first_after, first_before);
    assert_ne!(second_after, second_before);
    assert_ne!(first_after, second_after);
    // The ended meeting has no link; the other team's link is untouched.
    let res = app
        .call(
            "GET",
            &format!("/api/v1/meetings/{}/invite-link", id(&ended)),
            &alice,
            None,
        )
        .await;
    assert_eq!(res.status, 409);
    assert_eq!(invite_token(&app, &alice, id(&other)).await, other_before);
    assert_eq!(accept(&app, &carol, &other_before).await.status, 200);

    // Old links are dead for everyone; the new one works for newcomers.
    assert_eq!(accept(&app, &dave, &first_before).await.status, 404);
    let res = accept(&app, &dave, &first_after).await;
    assert_eq!(res.status, 200, "{}", res.text());
    assert_eq!(res.json()["joinedTeam"], true);
}

#[tokio::test]
async fn leaving_the_team_also_regenerates_the_meeting_links() {
    let app = TestApp::new().await;
    let (alice, bob, _, team) = team_of_three(&app).await;
    let bob_id = user_id(&app, &bob).await;
    let meeting = start(&app, &alice, team).await;
    let m = id(&meeting);
    let token = invite_token(&app, &bob, m).await;

    let res = remove(&app, &bob, team, bob_id).await;
    assert_eq!(res.status, 204, "{}", res.text());

    assert_ne!(invite_token(&app, &alice, m).await, token);
    let res = accept(&app, &bob, &token).await;
    assert_eq!(res.status, 404);
    assert_eq!(res.json()["code"], "meeting_invite_invalid");
}

#[tokio::test]
async fn a_refused_removal_keeps_the_meeting_link() {
    let app = TestApp::new().await;
    let (alice, bob, carol, team) = team_of_three(&app).await;
    let carol_id = user_id(&app, &carol).await;
    let meeting = start(&app, &alice, team).await;
    let m = id(&meeting);
    let token = invite_token(&app, &alice, m).await;

    // bob is a member, not an admin.
    assert_eq!(remove(&app, &bob, team, carol_id).await.status, 403);
    assert_eq!(invite_token(&app, &alice, m).await, token);
}

#[tokio::test]
async fn the_role_cannot_go_to_someone_who_left_the_team() {
    let app = TestApp::new().await;
    let (alice, bob, carol, team) = team_of_three(&app).await;
    let alice_id = user_id(&app, &alice).await;
    let carol_id = user_id(&app, &carol).await;
    let meeting = start(&app, &alice, team).await;
    let m = id(&meeting);
    act(&app, &bob, m, "join", None).await;
    act(&app, &carol, m, "join", None).await;

    assert_eq!(remove(&app, &alice, team, carol_id).await.status, 204);
    let res = act(
        &app,
        &alice,
        m,
        "facilitator",
        Some(json!({ "userId": carol_id })),
    )
    .await;
    assert_eq!(res.status, 400, "{}", res.text());
    assert_eq!(res.json()["code"], "target_not_participant");

    // Nothing changed: alice still runs the meeting.
    let s = state(&app, &alice, m).await;
    assert_eq!(s["facilitatorId"], alice_id.to_string());
    let res = act(&app, &alice, m, "icebreaker/next", None).await;
    assert_eq!(res.status, 200, "{}", res.text());
}

#[tokio::test]
async fn the_role_still_goes_to_a_current_member() {
    let app = TestApp::new().await;
    let (alice, bob, carol, team) = team_of_three(&app).await;
    let bob_id = user_id(&app, &bob).await;
    let carol_id = user_id(&app, &carol).await;
    let meeting = start(&app, &alice, team).await;
    let m = id(&meeting);
    act(&app, &bob, m, "join", None).await;
    act(&app, &carol, m, "join", None).await;
    assert_eq!(remove(&app, &alice, team, carol_id).await.status, 204);

    let res = act(
        &app,
        &alice,
        m,
        "facilitator",
        Some(json!({ "userId": bob_id })),
    )
    .await;
    assert_eq!(res.status, 200, "{}", res.text());
    assert_eq!(
        state(&app, &bob, m).await["facilitatorId"],
        bob_id.to_string()
    );
}

#[tokio::test]
async fn an_admin_takes_over_from_a_facilitator_who_left() {
    let app = TestApp::new().await;
    let (alice, bob, _, team) = team_of_three(&app).await;
    let alice_id = user_id(&app, &alice).await;
    let bob_id = user_id(&app, &bob).await;
    let meeting = start(&app, &bob, team).await;
    let m = id(&meeting);
    act(&app, &alice, m, "join", None).await;

    assert_eq!(remove(&app, &alice, team, bob_id).await.status, 204);
    let res = act(
        &app,
        &alice,
        m,
        "facilitator",
        Some(json!({ "userId": alice_id })),
    )
    .await;
    assert_eq!(res.status, 200, "{}", res.text());
    let res = act(&app, &alice, m, "icebreaker/next", None).await;
    assert_eq!(res.status, 200, "{}", res.text());
}
