//! Meetings: start, list, join, icebreaker, phases, facilitator, end and the
//! meeting invite link (spec §6.3, §8, §9.1, §12, §14.3).

mod common;

use common::TestApp;
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
