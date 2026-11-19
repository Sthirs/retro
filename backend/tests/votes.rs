//! Vote phase over REST: the budget, the most votes per group, privacy of
//! votes, single-card groups and the facilitator's settings (spec §9.4, §12,
//! §13, §14.4).

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

/// alice (admin, facilitator), bob and erin (participants) in a meeting;
/// carol is a member who has not joined.
struct Room {
    alice: String,
    bob: String,
    erin: String,
    carol: String,
    meeting: String,
    /// The group of p1 and p2 (plus), made during Group.
    pair: String,
    /// The single-card groups Vote start made for d1 (delta) and i1 (ideas).
    d1: String,
    i1: String,
}

/// The room in Group, with p1 and p2 grouped.
async fn grouping_with(app: &TestApp, suffix: &str) -> (Room, String, String, String, String) {
    let alice = app.token_for("alice").await;
    let bob = app.token_for("bob").await;
    let erin = app
        .create_user(
            &format!("vic{suffix}"),
            &format!("vic{suffix}@example.com"),
            "Erin Galli",
        )
        .await;
    let carol = app
        .create_user(
            &format!("carol{suffix}"),
            &format!("carol{suffix}@example.com"),
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
            &format!("/api/v1/teams/{team}/meetings/{meeting}/join"),
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
    next_phase(app, &alice, &meeting).await;
    let res = post(
        app,
        &alice,
        &format!("/api/v1/meetings/{meeting}/groups"),
        Some(json!({ "targetId": p1, "reflectionId": p2 })),
    )
    .await;
    assert_eq!(res.status, 200, "{}", res.text());
    let pair = res.json()["groups"][0]["id"].as_str().unwrap().to_owned();
    let room = Room {
        alice,
        bob,
        erin,
        carol,
        meeting,
        pair,
        d1: String::new(),
        i1: String::new(),
    };
    (room, p1, p2, d1, i1)
}

/// The room in Vote.
async fn room_with(app: &TestApp, suffix: &str) -> Room {
    let (mut r, _, _, d1, i1) = grouping_with(app, suffix).await;
    next_phase(app, &r.alice, &r.meeting).await;
    let seen = state(app, &r.alice, &r.meeting).await;
    r.d1 = group_of(&seen, &d1);
    r.i1 = group_of(&seen, &i1);
    r
}

async fn room(app: &TestApp) -> Room {
    room_with(app, "").await
}

fn group_of(state: &Value, card: &str) -> String {
    state["reflections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == card)
        .unwrap()["groupId"]
        .as_str()
        .expect("every card has a group in Vote")
        .to_owned()
}

async fn vote(app: &TestApp, token: &str, group: &str, delta: i32) -> TestResponse {
    app.call(
        "POST",
        &format!("/api/v1/groups/{group}/vote"),
        token,
        Some(json!({ "delta": delta })),
    )
    .await
}

async fn settings(
    app: &TestApp,
    token: &str,
    meeting: &str,
    budget: i32,
    max: i32,
) -> TestResponse {
    app.call(
        "PATCH",
        &format!("/api/v1/meetings/{meeting}/settings"),
        token,
        Some(json!({ "voteBudget": budget, "voteMaxPerGroup": max })),
    )
    .await
}

/// The viewer's own votes, by group id.
fn mine(state: &Value) -> Vec<(String, i64)> {
    let mut votes: Vec<(String, i64)> = state["myVotes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            (
                v["groupId"].as_str().unwrap().to_owned(),
                v["count"].as_i64().unwrap(),
            )
        })
        .collect();
    votes.sort();
    votes
}

fn mine_on(state: &Value, group: &str) -> i64 {
    mine(state)
        .into_iter()
        .find(|(g, _)| g == group)
        .map_or(0, |(_, n)| n)
}

fn spent(state: &Value) -> i64 {
    mine(state).iter().map(|(_, n)| n).sum()
}

/// Votes cast overall, as the facilitator sees them.
fn cast(state: &Value) -> i64 {
    state["votesCast"]
        .as_i64()
        .expect("the facilitator sees the votes cast")
}

/// How many people `budget` and `max` would take votes from.
async fn impact(app: &TestApp, token: &str, meeting: &str, budget: i32, max: i32) -> TestResponse {
    app.call(
        "GET",
        &format!(
            "/api/v1/meetings/{meeting}/settings/impact?voteBudget={budget}&voteMaxPerGroup={max}"
        ),
        token,
        None,
    )
    .await
}

/// `token`'s votes, oldest accepted first: (vote id, group id).
async fn rows(app: &TestApp, meeting: &str, token: &str) -> Vec<(Uuid, Uuid)> {
    use retro_backend::entities::vote;
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
    let user: Uuid = app.call("GET", "/api/v1/me", token, None).await.json()["user"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    vote::Entity::find()
        .filter(vote::Column::MeetingId.eq(meeting.parse::<Uuid>().unwrap()))
        .filter(vote::Column::UserId.eq(user))
        .order_by_asc(vote::Column::Ordinal)
        .all(&app.db)
        .await
        .unwrap()
        .into_iter()
        .map(|v| (v.id, v.group_id))
        .collect()
}

fn ok(res: TestResponse) -> Value {
    assert_eq!(res.status, 200, "{}", res.text());
    res.json()
}

fn code(res: &TestResponse) -> String {
    res.json()["code"].as_str().unwrap_or_default().to_owned()
}

#[tokio::test]
async fn vote_start_gives_every_loose_card_a_group_of_its_own() {
    let app = TestApp::new().await;
    let (r, p1, p2, d1, i1) = grouping_with(&app, "").await;
    let before = state(&app, &r.bob, &r.meeting).await;
    assert_eq!(before["groups"].as_array().unwrap().len(), 1);
    assert_eq!(before["myVotes"], json!([]));
    assert_eq!(before["voteBudget"], 5);
    assert_eq!(before["voteMaxPerGroup"], 3);

    next_phase(&app, &r.alice, &r.meeting).await;
    let seen = state(&app, &r.bob, &r.meeting).await;
    let groups = seen["groups"].as_array().unwrap();
    assert_eq!(groups.len(), 3);
    // The pair keeps its group; d1 and i1 each get an unnamed one, at their
    // column and place.
    assert_eq!(group_of(&seen, &p1), r.pair);
    assert_eq!(group_of(&seen, &p2), r.pair);
    for card in [&d1, &i1] {
        let id = group_of(&seen, card);
        assert_ne!(id, r.pair);
        let group = groups.iter().find(|g| g["id"] == id.as_str()).unwrap();
        let reflection = seen["reflections"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == card.as_str())
            .unwrap();
        assert_eq!(group["name"], "");
        assert_eq!(group["column"], reflection["column"]);
        assert_eq!(group["position"], reflection["position"]);
    }
    assert_ne!(group_of(&seen, &d1), group_of(&seen, &i1));
}

#[tokio::test]
async fn participants_vote_within_their_budget_and_the_group_limit() {
    let app = TestApp::new().await;
    let r = room(&app).await;

    let seen = ok(vote(&app, &r.bob, &r.pair, 1).await);
    assert_eq!(mine(&seen), vec![(r.pair.clone(), 1)]);
    ok(vote(&app, &r.bob, &r.pair, 1).await);
    let seen = ok(vote(&app, &r.bob, &r.pair, 1).await);
    assert_eq!(mine_on(&seen, &r.pair), 3);

    // A fourth vote on the same group is over the limit of 3.
    let res = vote(&app, &r.bob, &r.pair, 1).await;
    assert_eq!(res.status, 409);
    assert_eq!(code(&res), "vote_group_limit");

    // Two more elsewhere spend the budget of 5.
    ok(vote(&app, &r.bob, &r.d1, 1).await);
    let seen = ok(vote(&app, &r.bob, &r.i1, 1).await);
    assert_eq!(spent(&seen), 5);
    let res = vote(&app, &r.bob, &r.d1, 1).await;
    assert_eq!(res.status, 409);
    assert_eq!(code(&res), "vote_budget_exhausted");

    // Taking one back frees it again.
    let seen = ok(vote(&app, &r.bob, &r.pair, -1).await);
    assert_eq!(mine_on(&seen, &r.pair), 2);
    assert_eq!(spent(&seen), 4);
    let seen = ok(vote(&app, &r.bob, &r.d1, 1).await);
    assert_eq!(mine_on(&seen, &r.d1), 2);

    // Nothing to take back on a group without votes.
    ok(vote(&app, &r.erin, &r.d1, 1).await);
    ok(vote(&app, &r.erin, &r.d1, -1).await);
    let res = vote(&app, &r.erin, &r.d1, -1).await;
    assert_eq!(res.status, 409);
    assert_eq!(code(&res), "no_vote_to_remove");
}

#[tokio::test]
async fn a_vote_changes_by_exactly_one() {
    let app = TestApp::new().await;
    let r = room(&app).await;
    for delta in [0, 2, -2, 5] {
        let res = vote(&app, &r.bob, &r.pair, delta).await;
        assert_eq!(res.status, 400, "delta {delta}");
        assert_eq!(code(&res), "invalid_vote");
    }
    let seen = state(&app, &r.bob, &r.meeting).await;
    assert_eq!(mine(&seen), vec![]);
}

#[tokio::test]
async fn everyone_sees_only_their_own_votes() {
    let app = TestApp::new().await;
    let r = room(&app).await;
    ok(vote(&app, &r.bob, &r.pair, 1).await);
    ok(vote(&app, &r.bob, &r.pair, 1).await);
    ok(vote(&app, &r.erin, &r.d1, 1).await);

    let bob = state(&app, &r.bob, &r.meeting).await;
    assert_eq!(mine(&bob), vec![(r.pair.clone(), 2)]);
    // Everyone sees how many were cast overall, as one number.
    assert_eq!(cast(&bob), 3);
    let erin = state(&app, &r.erin, &r.meeting).await;
    assert_eq!(mine(&erin), vec![(r.d1.clone(), 1)]);
    assert_eq!(cast(&erin), 3);

    // The facilitator has no votes yet, and sees the same number: nothing
    // per voter.
    let alice = state(&app, &r.alice, &r.meeting).await;
    assert_eq!(mine(&alice), vec![]);
    assert_eq!(cast(&alice), 3);
    assert!(alice.get("voteUsage").is_none(), "no per-voter usage");

    // No total appears anywhere: groups carry no votes.
    for group in alice["groups"].as_array().unwrap() {
        let keys: Vec<&String> = group.as_object().unwrap().keys().collect();
        assert_eq!(keys.len(), 4, "{group}");
    }

    // The facilitator votes too.
    let alice = ok(vote(&app, &r.alice, &r.i1, 1).await);
    assert_eq!(mine(&alice), vec![(r.i1.clone(), 1)]);
    assert_eq!(cast(&alice), 4);
}

#[tokio::test]
async fn every_participant_sees_the_cast_count_whoever_facilitates() {
    let app = TestApp::new().await;
    let r = room(&app).await;
    ok(vote(&app, &r.erin, &r.d1, 1).await);
    let bob_id = app.call("GET", "/api/v1/me", &r.bob, None).await.json()["user"]["id"].clone();
    let res = post(
        &app,
        &r.alice,
        &format!("/api/v1/meetings/{}/facilitator", r.meeting),
        Some(json!({ "userId": bob_id })),
    )
    .await;
    assert_eq!(res.status, 200, "{}", res.text());
    for token in [&r.alice, &r.bob, &r.erin] {
        assert_eq!(cast(&state(&app, token, &r.meeting).await), 1);
    }
    // A participant's own vote answers with the new total.
    assert_eq!(cast(&ok(vote(&app, &r.erin, &r.d1, 1).await)), 2);
}

#[tokio::test]
async fn voting_needs_a_participant_during_vote() {
    let app = TestApp::new().await;
    let (r, ..) = grouping_with(&app, "").await;
    // Not yet: Group.
    let res = vote(&app, &r.bob, &r.pair, 1).await;
    assert_eq!(res.status, 409);
    assert_eq!(code(&res), "wrong_phase");
    next_phase(&app, &r.alice, &r.meeting).await;

    // A member who has not joined.
    let res = vote(&app, &r.carol, &r.pair, 1).await;
    assert_eq!(res.status, 403);
    assert_eq!(code(&res), "not_participant");

    // Someone outside the team learns nothing.
    let dave = app
        .create_user("dave", "dave@example.com", "Dave Moretti")
        .await;
    let res = vote(&app, &dave, &r.pair, 1).await;
    assert_eq!(res.status, 404);
    assert_eq!(code(&res), "group_not_found");
    let res = vote(&app, &r.bob, &Uuid::now_v7().to_string(), 1).await;
    assert_eq!(res.status, 404);
    assert_eq!(code(&res), "group_not_found");

    ok(vote(&app, &r.bob, &r.pair, 1).await);

    // Over: Discuss. The votes stay, still the voter's own.
    next_phase(&app, &r.alice, &r.meeting).await;
    let res = vote(&app, &r.bob, &r.pair, 1).await;
    assert_eq!(res.status, 409);
    assert_eq!(code(&res), "wrong_phase");
    let res = vote(&app, &r.bob, &r.pair, -1).await;
    assert_eq!(res.status, 409);
    let seen = state(&app, &r.bob, &r.meeting).await;
    assert_eq!(mine(&seen), vec![(r.pair.clone(), 1)]);
    assert_eq!(
        state(&app, &r.alice, &r.meeting).await["votesCast"],
        Value::Null
    );
}

#[tokio::test]
async fn groups_of_another_meeting_are_unknown() {
    let app = TestApp::new().await;
    let r = room(&app).await;
    let other = room_with(&app, "2").await;
    // erin of the first room is no member of the other team.
    let res = vote(&app, &r.erin, &other.pair, 1).await;
    assert_eq!(res.status, 404);
    assert_eq!(code(&res), "group_not_found");
}

#[tokio::test]
async fn an_ended_meeting_takes_no_votes() {
    let app = TestApp::new().await;
    let r = room(&app).await;
    ok(vote(&app, &r.bob, &r.pair, 1).await);
    let res = post(
        &app,
        &r.alice,
        &format!("/api/v1/meetings/{}/end", r.meeting),
        None,
    )
    .await;
    assert_eq!(res.status, 200, "{}", res.text());
    let res = vote(&app, &r.bob, &r.pair, 1).await;
    assert_eq!(res.status, 409);
    assert_eq!(code(&res), "meeting_ended");
    let res = settings(&app, &r.alice, &r.meeting, 3, 2).await;
    assert_eq!(res.status, 409);
    assert_eq!(code(&res), "meeting_ended");
}

#[tokio::test]
async fn only_the_facilitator_changes_the_settings_within_bounds() {
    let app = TestApp::new().await;
    let r = room(&app).await;
    let res = settings(&app, &r.bob, &r.meeting, 7, 4).await;
    assert_eq!(res.status, 403);
    assert_eq!(code(&res), "not_facilitator");
    for (budget, max) in [(0, 3), (21, 3), (5, 0), (5, 11)] {
        let res = settings(&app, &r.alice, &r.meeting, budget, max).await;
        assert_eq!(res.status, 400, "{budget}/{max}");
        assert_eq!(code(&res), "invalid_vote_settings");
    }
    let seen = ok(settings(&app, &r.alice, &r.meeting, 7, 4).await);
    assert_eq!(seen["voteBudget"], 7);
    assert_eq!(seen["voteMaxPerGroup"], 4);
    // Everyone gets the new limits, and a fourth vote on a group now fits.
    let bob = state(&app, &r.bob, &r.meeting).await;
    assert_eq!(bob["voteBudget"], 7);
    for _ in 0..4 {
        ok(vote(&app, &r.bob, &r.pair, 1).await);
    }
    let res = vote(&app, &r.bob, &r.pair, 1).await;
    assert_eq!(code(&res), "vote_group_limit");

    // Not after Vote.
    next_phase(&app, &r.alice, &r.meeting).await;
    let res = settings(&app, &r.alice, &r.meeting, 5, 3).await;
    assert_eq!(res.status, 409);
    assert_eq!(code(&res), "wrong_phase");
}

#[tokio::test]
async fn settings_can_be_set_before_vote() {
    let app = TestApp::new().await;
    let (r, ..) = grouping_with(&app, "").await;
    ok(settings(&app, &r.alice, &r.meeting, 3, 1).await);
    next_phase(&app, &r.alice, &r.meeting).await;
    let seen = state(&app, &r.bob, &r.meeting).await;
    assert_eq!(seen["voteBudget"], 3);
    assert_eq!(seen["voteMaxPerGroup"], 1);
}

#[tokio::test]
async fn a_lower_budget_removes_the_most_recent_votes() {
    let app = TestApp::new().await;
    let r = room(&app).await;
    // bob, oldest first: pair, pair, d1, i1, d1.
    for group in [&r.pair, &r.pair, &r.d1, &r.i1, &r.d1] {
        ok(vote(&app, &r.bob, group, 1).await);
    }
    // erin spends 2, under any new budget of 3.
    ok(vote(&app, &r.erin, &r.i1, 1).await);
    ok(vote(&app, &r.erin, &r.i1, 1).await);

    ok(settings(&app, &r.alice, &r.meeting, 3, 3).await);
    let bob = state(&app, &r.bob, &r.meeting).await;
    assert_eq!(spent(&bob), 3);
    assert_eq!(mine_on(&bob, &r.pair), 2);
    assert_eq!(mine_on(&bob, &r.d1), 1);
    assert_eq!(mine_on(&bob, &r.i1), 0);
    let erin = state(&app, &r.erin, &r.meeting).await;
    assert_eq!(mine(&erin), vec![(r.i1.clone(), 2)]);
    assert_eq!(cast(&state(&app, &r.alice, &r.meeting).await), 5);

    // Raising it again gives nothing back.
    ok(settings(&app, &r.alice, &r.meeting, 5, 3).await);
    assert_eq!(spent(&state(&app, &r.bob, &r.meeting).await), 3);
}

#[tokio::test]
async fn a_lower_group_limit_removes_the_most_recent_votes_over_it() {
    let app = TestApp::new().await;
    let r = room(&app).await;
    for group in [&r.pair, &r.pair, &r.pair, &r.d1, &r.d1] {
        ok(vote(&app, &r.bob, group, 1).await);
    }
    ok(settings(&app, &r.alice, &r.meeting, 5, 1).await);
    let bob = state(&app, &r.bob, &r.meeting).await;
    assert_eq!(mine(&bob).len(), 2);
    assert_eq!(mine_on(&bob, &r.pair), 1);
    assert_eq!(mine_on(&bob, &r.d1), 1);
    // The freed votes can go elsewhere.
    ok(vote(&app, &r.bob, &r.i1, 1).await);
    let res = vote(&app, &r.bob, &r.pair, 1).await;
    assert_eq!(code(&res), "vote_group_limit");
}

#[tokio::test]
async fn unchanged_settings_change_nothing() {
    let app = TestApp::new().await;
    let r = room(&app).await;
    let before = state(&app, &r.bob, &r.meeting).await["seq"].clone();
    ok(settings(&app, &r.alice, &r.meeting, 5, 3).await);
    assert_eq!(state(&app, &r.bob, &r.meeting).await["seq"], before);
}

#[tokio::test]
async fn a_vote_is_a_post_not_a_put() {
    let app = TestApp::new().await;
    let r = room(&app).await;
    let res = app
        .call(
            "PUT",
            &format!("/api/v1/groups/{}/vote", r.pair),
            &r.bob,
            Some(json!({ "delta": 1 })),
        )
        .await;
    assert_eq!(res.status, 405);
    assert_eq!(spent(&state(&app, &r.bob, &r.meeting).await), 0);
}

#[tokio::test]
async fn the_group_limit_goes_first_and_keeps_the_oldest_votes() {
    let app = TestApp::new().await;
    let r = room(&app).await;
    // bob, oldest first: pair, pair, d1. Group limit 1 first drops the second
    // pair vote; budget 2 then fits pair + d1. (Budget first would keep only
    // the two pair votes, then drop one: pair alone.)
    for group in [&r.pair, &r.pair, &r.d1] {
        ok(vote(&app, &r.bob, group, 1).await);
    }
    let before = rows(&app, &r.meeting, &r.bob).await;
    ok(settings(&app, &r.alice, &r.meeting, 2, 1).await);
    let after = rows(&app, &r.meeting, &r.bob).await;
    assert_eq!(after, vec![before[0], before[2]]);
}

#[tokio::test]
async fn most_recent_means_accepted_last_not_the_clock() {
    use sea_orm::{ConnectionTrait, DbBackend, Statement};
    let app = TestApp::new().await;
    let r = room(&app).await;
    for _ in 0..3 {
        ok(vote(&app, &r.bob, &r.pair, 1).await);
    }
    let votes = rows(&app, &r.meeting, &r.bob).await;
    // The clock stepped back: the newest vote carries the oldest time.
    app.db
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "UPDATE votes SET created_at = created_at - interval '1 hour' WHERE id = $1",
            [votes[2].0.into()],
        ))
        .await
        .unwrap();
    ok(vote(&app, &r.bob, &r.pair, -1).await);
    assert_eq!(rows(&app, &r.meeting, &r.bob).await, votes[..2].to_vec());
    ok(vote(&app, &r.bob, &r.pair, 1).await);
    let votes = rows(&app, &r.meeting, &r.bob).await;
    app.db
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "UPDATE votes SET created_at = created_at - interval '1 hour' WHERE id = $1",
            [votes[2].0.into()],
        ))
        .await
        .unwrap();
    ok(settings(&app, &r.alice, &r.meeting, 5, 2).await);
    assert_eq!(rows(&app, &r.meeting, &r.bob).await, votes[..2].to_vec());
}

#[tokio::test]
async fn the_settings_preview_counts_people_never_names_them() {
    let app = TestApp::new().await;
    let r = room(&app).await;
    // bob: pair x3, d1 x2 (5). erin: i1 x2 (2).
    for group in [&r.pair, &r.pair, &r.pair, &r.d1, &r.d1] {
        ok(vote(&app, &r.bob, group, 1).await);
    }
    ok(vote(&app, &r.erin, &r.i1, 1).await);
    ok(vote(&app, &r.erin, &r.i1, 1).await);

    for (budget, max, affected) in [
        (5, 3, 0),
        (4, 3, 1),
        (5, 2, 1),
        (5, 1, 2),
        (1, 3, 2),
        // Group limit first: bob keeps pair x2 + d1 x2 = 4, within 4.
        (4, 2, 1),
        (20, 10, 0),
    ] {
        let res = impact(&app, &r.alice, &r.meeting, budget, max).await;
        assert_eq!(ok(res), json!({ "affected": affected }), "{budget}/{max}");
    }
    // The preview changes nothing.
    assert_eq!(spent(&state(&app, &r.bob, &r.meeting).await), 5);

    // Facilitator only, within bounds.
    let res = impact(&app, &r.bob, &r.meeting, 4, 3).await;
    assert_eq!(res.status, 403);
    assert_eq!(code(&res), "not_facilitator");
    let res = impact(&app, &r.alice, &r.meeting, 0, 3).await;
    assert_eq!(res.status, 400);
    assert_eq!(code(&res), "invalid_vote_settings");
    let dave = app
        .create_user("dave", "dave@example.com", "Dave Moretti")
        .await;
    let res = impact(&app, &dave, &r.meeting, 4, 3).await;
    assert_eq!(res.status, 404);

    // Not after Vote.
    next_phase(&app, &r.alice, &r.meeting).await;
    let res = impact(&app, &r.alice, &r.meeting, 4, 3).await;
    assert_eq!(res.status, 409);
    assert_eq!(code(&res), "wrong_phase");
}

#[tokio::test]
async fn the_settings_preview_works_before_vote() {
    let app = TestApp::new().await;
    let (r, ..) = grouping_with(&app, "").await;
    let res = impact(&app, &r.alice, &r.meeting, 1, 1).await;
    assert_eq!(ok(res), json!({ "affected": 0 }));
}
