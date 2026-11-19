//! The real-time hub over a real socket (spec §11, ADR-0011): tickets,
//! subscriptions, event delivery in `seq` order, presence and live tasks.

mod common;

use std::{collections::HashMap, time::Duration};

use common::TestApp;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::net::TcpStream;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};
use uuid::Uuid;

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

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

async fn user_id(app: &TestApp, token: &str) -> String {
    let me = app.get_with_token("/api/v1/me", token).await.json();
    me["user"]["id"].as_str().unwrap().to_owned()
}

async fn ticket(app: &TestApp, token: &str) -> String {
    let res = app.call("POST", "/api/v1/ws-ticket", token, None).await;
    assert_eq!(res.status, 200, "{}", res.text());
    res.json()["ticket"].as_str().unwrap().to_owned()
}

async fn connect(addr: &str, ticket: &str) -> Socket {
    let (socket, _) = connect_async(format!("ws://{addr}/api/v1/ws?ticket={ticket}"))
        .await
        .expect("websocket handshake");
    socket
}

/// The next JSON text message, skipping pings.
async fn next(socket: &mut Socket) -> Value {
    loop {
        let message = tokio::time::timeout(Duration::from_secs(10), socket.next())
            .await
            .expect("a message within 10 s")
            .expect("socket open")
            .expect("valid frame");
        if let Message::Text(text) = message {
            return serde_json::from_str(&text).expect("JSON message");
        }
    }
}

/// The next message of `kind`, skipping others (e.g. presence).
async fn next_of(socket: &mut Socket, kind: &str) -> Value {
    loop {
        let message = next(socket).await;
        if message["type"] == kind {
            return message;
        }
    }
}

async fn subscribe(socket: &mut Socket, channel: &str) -> Value {
    socket
        .send(Message::text(
            json!({ "type": "subscribe", "channel": channel }).to_string(),
        ))
        .await
        .unwrap();
    next(socket).await
}

#[tokio::test]
async fn the_handshake_needs_a_valid_ticket() {
    let app = TestApp::new().await;
    let addr = app.spawn_server().await;
    let alice = app.token_for("alice").await;

    for url in [
        format!("ws://{addr}/api/v1/ws"),
        format!("ws://{addr}/api/v1/ws?ticket=nope"),
        // An access token is not a ticket.
        format!("ws://{addr}/api/v1/ws?ticket={alice}"),
    ] {
        let err = connect_async(url).await.expect_err("refused");
        let tokio_tungstenite::tungstenite::Error::Http(response) = err else {
            panic!("expected an HTTP refusal, got {err:?}");
        };
        assert_eq!(response.status(), 401);
    }
}

#[tokio::test]
async fn subscriptions_are_limited_to_team_members() {
    let app = TestApp::new().await;
    let addr = app.spawn_server().await;
    let alice = app.token_for("alice").await;
    let bob = app.token_for("bob").await;
    let team = create_team(&app, &alice, "Platform").await;
    let mut socket = connect(&addr, &ticket(&app, &bob).await).await;

    // Errors have the shape of every message: the code is in the payload.
    let channel = format!("team:{team}");
    let reply = subscribe(&mut socket, &channel).await;
    assert_eq!(
        reply,
        json!({
            "type": "error",
            "channel": channel,
            "seq": null,
            "payload": { "code": "team_not_found" },
        })
    );
    let reply = subscribe(&mut socket, "nonsense").await;
    assert_eq!(reply["payload"]["code"], "invalid_channel");
    let reply = subscribe(&mut socket, &format!("meeting:{}", Uuid::now_v7())).await;
    assert_eq!(reply["payload"]["code"], "meeting_not_found");
    assert!(reply.get("code").is_none());
}

#[tokio::test]
async fn channels_must_be_spelled_canonically() {
    let app = TestApp::new().await;
    let addr = app.spawn_server().await;
    let alice = app.token_for("alice").await;
    let team = create_team(&app, &alice, "Platform").await;
    let mut socket = connect(&addr, &ticket(&app, &alice).await).await;

    // Events go out as `team:{lowercase hyphenated}`: another spelling of the
    // same id would authorise but never receive anything.
    let upper = format!("team:{}", team.to_string().to_uppercase());
    let simple = format!("team:{}", team.simple());
    let braced = format!("team:{}", team.braced());
    for name in [&upper, &simple, &braced] {
        let reply = subscribe(&mut socket, name).await;
        assert_eq!(reply["payload"]["code"], "invalid_channel", "{name}");
    }
    let reply = subscribe(&mut socket, &format!("team:{team}")).await;
    assert_eq!(reply["type"], "subscribed");
}

#[tokio::test]
async fn teammates_see_a_new_meeting_live() {
    let app = TestApp::new().await;
    let addr = app.spawn_server().await;
    let alice = app.token_for("alice").await;
    let bob = app.token_for("bob").await;
    let team = create_team(&app, &alice, "Platform").await;
    join_team(&app, &alice, team, &bob).await;

    let mut socket = connect(&addr, &ticket(&app, &bob).await).await;
    let channel = format!("team:{team}");
    let reply = subscribe(&mut socket, &channel).await;
    assert_eq!(reply, json!({ "type": "subscribed", "channel": channel }));

    let meeting = app
        .call(
            "POST",
            &format!("/api/v1/teams/{team}/meetings"),
            &alice,
            None,
        )
        .await
        .json();
    let event = next(&mut socket).await;
    assert_eq!(event["type"], "meeting.started");
    assert_eq!(event["channel"], channel);
    assert_eq!(event["payload"]["meetingId"], meeting["id"]);
    assert!(event["seq"].as_i64().unwrap() >= 1);
}

#[tokio::test]
async fn participants_receive_meeting_events_in_order() {
    let app = TestApp::new().await;
    let addr = app.spawn_server().await;
    let alice = app.token_for("alice").await;
    let bob = app.token_for("bob").await;
    let bob_id = user_id(&app, &bob).await;
    let team = create_team(&app, &alice, "Platform").await;
    join_team(&app, &alice, team, &bob).await;
    let meeting = app
        .call(
            "POST",
            &format!("/api/v1/teams/{team}/meetings"),
            &alice,
            None,
        )
        .await
        .json();
    let m = meeting["id"].as_str().unwrap().to_owned();
    let channel = format!("meeting:{m}");

    let mut alice_socket = connect(&addr, &ticket(&app, &alice).await).await;
    assert_eq!(
        subscribe(&mut alice_socket, &channel).await["type"],
        "subscribed"
    );

    let res = app
        .call(
            "POST",
            &format!("/api/v1/teams/{team}/meetings/{m}/join"),
            &bob,
            None,
        )
        .await;
    assert_eq!(res.status, 200, "{}", res.text());
    let joined = next_of(&mut alice_socket, "participant.joined").await;
    assert_eq!(joined["payload"]["participant"]["id"], bob_id);
    assert_eq!(
        joined["payload"]["participant"]["displayName"],
        "Bob Bianchi"
    );
    let first_seq = joined["seq"].as_i64().unwrap();

    app.call(
        "POST",
        &format!("/api/v1/meetings/{m}/icebreaker/next"),
        &alice,
        Some(json!({ "skip": true })),
    )
    .await;
    let person = next_of(&mut alice_socket, "icebreaker.person_changed").await;
    assert_eq!(person["payload"]["index"], 1);
    assert_eq!(person["seq"].as_i64().unwrap(), first_seq + 1);

    let res = app
        .call(
            "POST",
            &format!("/api/v1/meetings/{m}/icebreaker/question"),
            &alice,
            None,
        )
        .await;
    assert_eq!(res.status, 200, "{}", res.text());
    let question = next_of(&mut alice_socket, "icebreaker.question_changed").await;
    assert_eq!(question["payload"]["question"], res.json()["question"]);
    assert_eq!(question["seq"].as_i64().unwrap(), first_seq + 2);

    app.call(
        "POST",
        &format!("/api/v1/meetings/{m}/facilitator"),
        &alice,
        Some(json!({ "userId": bob_id })),
    )
    .await;
    let changed = next_of(&mut alice_socket, "facilitator.changed").await;
    assert_eq!(changed["payload"]["facilitatorId"], bob_id);
    assert_eq!(changed["seq"].as_i64().unwrap(), first_seq + 3);

    // The snapshot's seq matches the last event.
    let state = app
        .call("GET", &format!("/api/v1/meetings/{m}/state"), &alice, None)
        .await
        .json();
    assert_eq!(state["seq"].as_i64().unwrap(), first_seq + 3);

    app.call("POST", &format!("/api/v1/meetings/{m}/end"), &bob, None)
        .await;
    let ended = next_of(&mut alice_socket, "meeting.ended").await;
    assert!(ended["payload"]["endedAt"].is_string());
}

#[tokio::test]
async fn presence_follows_the_participants_sockets() {
    let app = TestApp::new().await;
    let addr = app.spawn_server().await;
    let alice = app.token_for("alice").await;
    let bob = app.token_for("bob").await;
    let bob_id = user_id(&app, &bob).await;
    let team = create_team(&app, &alice, "Platform").await;
    join_team(&app, &alice, team, &bob).await;
    let meeting = app
        .call(
            "POST",
            &format!("/api/v1/teams/{team}/meetings"),
            &alice,
            None,
        )
        .await
        .json();
    let m = meeting["id"].as_str().unwrap().to_owned();
    app.call(
        "POST",
        &format!("/api/v1/teams/{team}/meetings/{m}/join"),
        &bob,
        None,
    )
    .await;
    let channel = format!("meeting:{m}");

    let mut alice_socket = connect(&addr, &ticket(&app, &alice).await).await;
    subscribe(&mut alice_socket, &channel).await;
    let mut team_socket = connect(&addr, &ticket(&app, &alice).await).await;
    subscribe(&mut team_socket, &format!("team:{team}")).await;

    let mut bob_socket = connect(&addr, &ticket(&app, &bob).await).await;
    subscribe(&mut bob_socket, &channel).await;
    let online = loop {
        let event = next_of(&mut alice_socket, "presence.changed").await;
        if event["payload"]["userId"] == bob_id {
            break event;
        }
    };
    assert_eq!(online["payload"]["online"], true);
    assert_eq!(online["seq"], Value::Null);
    // The Meetings page hears about it too, as presence: no reload needed.
    let presence = next(&mut team_socket).await;
    assert_eq!(
        presence,
        json!({
            "type": "presence.changed",
            "channel": format!("team:{team}"),
            "seq": null,
            "payload": { "meetingId": m, "userId": bob_id, "online": true },
        })
    );

    let state = app
        .call("GET", &format!("/api/v1/meetings/{m}/state"), &alice, None)
        .await
        .json();
    let bob_entry = state["participants"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == bob_id)
        .unwrap()
        .clone();
    assert_eq!(bob_entry["online"], true);

    bob_socket.close(None).await.unwrap();
    let offline = loop {
        let event = next_of(&mut alice_socket, "presence.changed").await;
        if event["payload"]["userId"] == bob_id {
            break event;
        }
    };
    assert_eq!(offline["payload"]["online"], false);
    let presence = next(&mut team_socket).await;
    assert_eq!(presence["type"], "presence.changed");
    assert_eq!(presence["payload"]["online"], false);
    // Presence alone never asks Meetings pages to reload the list.
    assert_silent(&mut team_socket, Duration::from_millis(300)).await;
}

#[tokio::test]
async fn joining_still_updates_the_meetings_list() {
    let app = TestApp::new().await;
    let addr = app.spawn_server().await;
    let alice = app.token_for("alice").await;
    let bob = app.token_for("bob").await;
    let team = create_team(&app, &alice, "Platform").await;
    join_team(&app, &alice, team, &bob).await;
    let m = start_meeting(&app, &alice, team).await;
    let mut team_socket = connect(&addr, &ticket(&app, &alice).await).await;
    subscribe(&mut team_socket, &format!("team:{team}")).await;

    // A join (no socket in the room) changes the participants.
    app.call(
        "POST",
        &format!("/api/v1/teams/{team}/meetings/{m}/join"),
        &bob,
        None,
    )
    .await;
    let updated = next(&mut team_socket).await;
    assert_eq!(updated["type"], "meeting.updated");
    assert_eq!(updated["payload"]["meetingId"], m);
}

#[tokio::test]
async fn task_changes_reach_the_team_channel() {
    let app = TestApp::new().await;
    let addr = app.spawn_server().await;
    let alice = app.token_for("alice").await;
    let team = create_team(&app, &alice, "Platform").await;
    let (m, group) = app.discussing(&alice, team).await;
    let mut socket = connect(&addr, &ticket(&app, &alice).await).await;
    subscribe(&mut socket, &format!("team:{team}")).await;

    let res = app
        .discuss_task(&alice, &m, &group, "<p>Write docs</p>", &[])
        .await;
    assert_eq!(res.status, 201, "{}", res.text());
    let task = TestApp::last_task(&res.json());
    let created = next_of(&mut socket, "task.created").await;
    assert_eq!(created["payload"]["taskId"], task);
    assert_eq!(created["payload"]["status"], "active");

    app.call(
        "PUT",
        &format!("/api/v1/tasks/{task}/position"),
        &alice,
        Some(json!({ "status": "done" })),
    )
    .await;
    let moved = next_of(&mut socket, "task.moved").await;
    assert_eq!(moved["payload"]["status"], "done");
    assert_eq!(
        moved["seq"].as_i64().unwrap(),
        created["seq"].as_i64().unwrap() + 1
    );
}

#[tokio::test]
async fn people_in_the_room_are_present_before_their_join_lands() {
    let app = TestApp::new().await;
    let addr = app.spawn_server().await;
    let alice = app.token_for("alice").await;
    let bob = app.token_for("bob").await;
    let bob_id = user_id(&app, &bob).await;
    let team = create_team(&app, &alice, "Platform").await;
    join_team(&app, &alice, team, &bob).await;
    let meeting = app
        .call(
            "POST",
            &format!("/api/v1/teams/{team}/meetings"),
            &alice,
            None,
        )
        .await
        .json();
    let m = meeting["id"].as_str().unwrap().to_owned();
    let channel = format!("meeting:{m}");
    let mut alice_socket = connect(&addr, &ticket(&app, &alice).await).await;
    subscribe(&mut alice_socket, &channel).await;

    // Bob's room subscribes first, then joins.
    let mut bob_socket = connect(&addr, &ticket(&app, &bob).await).await;
    subscribe(&mut bob_socket, &channel).await;
    let online = loop {
        let event = next_of(&mut alice_socket, "presence.changed").await;
        if event["payload"]["userId"] == bob_id {
            break event;
        }
    };
    assert_eq!(online["payload"]["online"], true);
    let state = app
        .call("GET", &format!("/api/v1/meetings/{m}/state"), &alice, None)
        .await
        .json();
    assert!(
        state["onlineIds"]
            .as_array()
            .unwrap()
            .contains(&Value::String(bob_id.clone()))
    );

    let res = app
        .call(
            "POST",
            &format!("/api/v1/teams/{team}/meetings/{m}/join"),
            &bob,
            None,
        )
        .await;
    let bob_entry = res.json()["participants"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == bob_id)
        .unwrap()
        .clone();
    assert_eq!(bob_entry["online"], true);
}

// --- Losing access (spec §11.1) --------------------------------------------

async fn start_meeting(app: &TestApp, token: &str, team: Uuid) -> String {
    let res = app
        .call(
            "POST",
            &format!("/api/v1/teams/{team}/meetings"),
            token,
            None,
        )
        .await;
    assert_eq!(res.status, 201, "{}", res.text());
    res.json()["id"].as_str().unwrap().to_owned()
}

async fn remove(app: &TestApp, token: &str, team: Uuid, user: &str) {
    let res = app
        .call(
            "DELETE",
            &format!("/api/v1/teams/{team}/members/{user}"),
            token,
            None,
        )
        .await;
    assert_eq!(res.status, 204, "{}", res.text());
}

async fn rename(app: &TestApp, token: &str, meeting: &str, title: &str) {
    let res = app
        .call(
            "PATCH",
            &format!("/api/v1/meetings/{meeting}"),
            token,
            Some(json!({ "title": title })),
        )
        .await;
    assert_eq!(res.status, 200, "{}", res.text());
}

async fn create_task(app: &TestApp, token: &str, team: Uuid) -> Value {
    let (m, group) = app.discussing(token, team).await;
    let res = app
        .discuss_task(token, &m, &group, "<p>Later</p>", &[])
        .await;
    assert_eq!(res.status, 201, "{}", res.text());
    json!({ "id": TestApp::last_task(&res.json()) })
}

/// Reads until an `error` arrived on each of `channels`, and returns them.
async fn errors_on(socket: &mut Socket, channels: &[&str]) -> HashMap<String, Value> {
    let mut errors = HashMap::new();
    while errors.len() < channels.len() {
        let message = next(socket).await;
        if message["type"] == "error" {
            let channel = message["channel"].as_str().unwrap().to_owned();
            assert!(channels.contains(&channel.as_str()), "{message}");
            errors.insert(channel, message);
        }
    }
    errors
}

/// Fails if a message arrives within `wait`.
async fn assert_silent(socket: &mut Socket, wait: Duration) {
    let message = tokio::time::timeout(wait, async {
        loop {
            match socket.next().await {
                Some(Ok(Message::Text(text))) => return Some(text.to_string()),
                Some(Ok(_)) => continue,
                _ => return None,
            }
        }
    })
    .await;
    if let Ok(Some(text)) = message {
        panic!("unexpected message: {text}");
    }
}

fn revoked(channel: &str) -> Value {
    json!({
        "type": "error",
        "channel": channel,
        "seq": null,
        "payload": { "code": "team_not_found" },
    })
}

/// alice (admin) and bob in a team, with a meeting alice runs and bob joined.
async fn team_with_meeting(app: &TestApp) -> (String, String, String, Uuid, String) {
    let alice = app.token_for("alice").await;
    let bob = app.token_for("bob").await;
    let bob_id = user_id(app, &bob).await;
    let team = create_team(app, &alice, "Platform").await;
    join_team(app, &alice, team, &bob).await;
    let m = start_meeting(app, &alice, team).await;
    let res = app
        .call(
            "POST",
            &format!("/api/v1/teams/{team}/meetings/{m}/join"),
            &bob,
            None,
        )
        .await;
    assert_eq!(res.status, 200, "{}", res.text());
    (alice, bob, bob_id, team, m)
}

async fn online_ids(app: &TestApp, token: &str, meeting: &str) -> Vec<Value> {
    app.call(
        "GET",
        &format!("/api/v1/meetings/{meeting}/state"),
        token,
        None,
    )
    .await
    .json()["onlineIds"]
        .as_array()
        .unwrap()
        .clone()
}

#[tokio::test]
async fn a_removed_member_loses_the_team_and_meeting_channels() {
    let app = TestApp::new().await;
    let addr = app.spawn_server().await;
    let (alice, bob, bob_id, team, m) = team_with_meeting(&app).await;
    let (team_channel, meeting_channel) = (format!("team:{team}"), format!("meeting:{m}"));
    let mut bob_socket = connect(&addr, &ticket(&app, &bob).await).await;
    subscribe(&mut bob_socket, &team_channel).await;
    subscribe(&mut bob_socket, &meeting_channel).await;
    let mut alice_socket = connect(&addr, &ticket(&app, &alice).await).await;
    subscribe(&mut alice_socket, &team_channel).await;
    subscribe(&mut alice_socket, &meeting_channel).await;
    assert!(online_ids(&app, &alice, &m).await.contains(&json!(bob_id)));

    remove(&app, &alice, team, &bob_id).await;

    let errors = errors_on(&mut bob_socket, &[&team_channel, &meeting_channel]).await;
    assert_eq!(errors[&team_channel], revoked(&team_channel));
    assert_eq!(errors[&meeting_channel], revoked(&meeting_channel));
    // The others hear about it, and bob is no longer in the room.
    let removed = next_of(&mut alice_socket, "member.removed").await;
    assert_eq!(removed["channel"], team_channel);
    assert_eq!(removed["payload"]["userId"], bob_id);
    let offline = loop {
        let event = next_of(&mut alice_socket, "presence.changed").await;
        if event["payload"]["userId"] == bob_id {
            break event;
        }
    };
    assert_eq!(offline["payload"]["online"], false);
    assert!(!online_ids(&app, &alice, &m).await.contains(&json!(bob_id)));

    // Later events reach the members only.
    rename(&app, &alice, &m, "After bob").await;
    create_task(&app, &alice, team).await;
    next_of(&mut alice_socket, "meeting.renamed").await;
    next_of(&mut alice_socket, "task.created").await;
    assert_silent(&mut bob_socket, Duration::from_millis(500)).await;

    // Subscribing again is refused, as for any non-member.
    let reply = subscribe(&mut bob_socket, &meeting_channel).await;
    assert_eq!(reply["payload"]["code"], "meeting_not_found");
}

#[tokio::test]
async fn a_member_who_leaves_loses_the_channels() {
    let app = TestApp::new().await;
    let addr = app.spawn_server().await;
    let (alice, bob, bob_id, team, m) = team_with_meeting(&app).await;
    let (team_channel, meeting_channel) = (format!("team:{team}"), format!("meeting:{m}"));
    let mut bob_socket = connect(&addr, &ticket(&app, &bob).await).await;
    subscribe(&mut bob_socket, &team_channel).await;
    subscribe(&mut bob_socket, &meeting_channel).await;

    remove(&app, &bob, team, &bob_id).await;

    errors_on(&mut bob_socket, &[&team_channel, &meeting_channel]).await;
    rename(&app, &alice, &m, "Without bob").await;
    assert_silent(&mut bob_socket, Duration::from_millis(500)).await;
}

#[tokio::test]
async fn deleting_a_team_ends_everyones_subscriptions() {
    let app = TestApp::new().await;
    let addr = app.spawn_server().await;
    let (alice, bob, _, team, m) = team_with_meeting(&app).await;
    let (team_channel, meeting_channel) = (format!("team:{team}"), format!("meeting:{m}"));
    let mut sockets = Vec::new();
    for token in [&alice, &bob] {
        let mut socket = connect(&addr, &ticket(&app, token).await).await;
        subscribe(&mut socket, &team_channel).await;
        subscribe(&mut socket, &meeting_channel).await;
        sockets.push(socket);
    }

    let res = app
        .call("DELETE", &format!("/api/v1/teams/{team}"), &alice, None)
        .await;
    assert_eq!(res.status, 204, "{}", res.text());

    for socket in &mut sockets {
        let errors = errors_on(socket, &[&team_channel, &meeting_channel]).await;
        assert_eq!(errors[&team_channel], revoked(&team_channel));
        assert_eq!(errors[&meeting_channel], revoked(&meeting_channel));
        assert_silent(socket, Duration::from_millis(300)).await;
    }
}

#[tokio::test]
async fn a_removal_keeps_the_other_teams_channels() {
    let app = TestApp::new().await;
    let addr = app.spawn_server().await;
    let alice = app.token_for("alice").await;
    let bob = app.token_for("bob").await;
    let bob_id = user_id(&app, &bob).await;
    let left = create_team(&app, &alice, "Left").await;
    let kept = create_team(&app, &alice, "Kept").await;
    join_team(&app, &alice, left, &bob).await;
    join_team(&app, &alice, kept, &bob).await;
    let mut bob_socket = connect(&addr, &ticket(&app, &bob).await).await;
    subscribe(&mut bob_socket, &format!("team:{left}")).await;
    subscribe(&mut bob_socket, &format!("team:{kept}")).await;

    remove(&app, &alice, left, &bob_id).await;

    errors_on(&mut bob_socket, &[&format!("team:{left}")]).await;
    let task = create_task(&app, &alice, kept).await;
    let created = next_of(&mut bob_socket, "task.created").await;
    assert_eq!(created["channel"], format!("team:{kept}"));
    assert_eq!(created["payload"]["taskId"], task["id"]);
}

#[tokio::test]
async fn a_member_added_back_can_subscribe_again() {
    let app = TestApp::new().await;
    let addr = app.spawn_server().await;
    let (alice, bob, bob_id, team, _) = team_with_meeting(&app).await;
    let channel = format!("team:{team}");
    let mut bob_socket = connect(&addr, &ticket(&app, &bob).await).await;
    subscribe(&mut bob_socket, &channel).await;
    remove(&app, &alice, team, &bob_id).await;
    errors_on(&mut bob_socket, &[&channel]).await;

    join_team(&app, &alice, team, &bob).await;
    let reply = subscribe(&mut bob_socket, &channel).await;
    assert_eq!(reply["type"], "subscribed");
    let task = create_task(&app, &alice, team).await;
    let created = next_of(&mut bob_socket, "task.created").await;
    assert_eq!(created["payload"]["taskId"], task["id"]);
}

// --- Presence with several tabs --------------------------------------------

async fn unsubscribe(socket: &mut Socket, channel: &str) -> Value {
    socket
        .send(Message::text(
            json!({ "type": "unsubscribe", "channel": channel }).to_string(),
        ))
        .await
        .unwrap();
    next_of(socket, "unsubscribed").await
}

/// bob's last announced presence on alice's socket, once nothing more arrives.
async fn settled_presence(socket: &mut Socket, user: &str) -> Option<bool> {
    let mut last = None;
    while let Ok(Some(Ok(message))) =
        tokio::time::timeout(Duration::from_millis(700), socket.next()).await
    {
        let Message::Text(text) = message else {
            continue;
        };
        let event: Value = serde_json::from_str(&text).unwrap();
        if event["type"] == "presence.changed" && event["payload"]["userId"] == user {
            last = event["payload"]["online"].as_bool();
        }
    }
    last
}

#[tokio::test]
async fn presence_survives_two_tabs_coming_and_going_at_once() {
    let app = TestApp::new().await;
    let addr = app.spawn_server().await;
    let (alice, bob, bob_id, _, m) = team_with_meeting(&app).await;
    let channel = format!("meeting:{m}");
    let mut alice_socket = connect(&addr, &ticket(&app, &alice).await).await;
    subscribe(&mut alice_socket, &channel).await;
    let mut tabs = [
        connect(&addr, &ticket(&app, &bob).await).await,
        connect(&addr, &ticket(&app, &bob).await).await,
    ];
    subscribe(&mut tabs[0], &channel).await;

    // One tab leaves the room while the other enters it: bob stays in the
    // room throughout (his count goes 1 → 0 → 1 or 1 → 2 → 1).
    for round in 0..50 {
        let [first, second] = &mut tabs;
        let (leaving, entering) = if round % 2 == 0 {
            (first, second)
        } else {
            (second, first)
        };
        let (left, entered) = tokio::join!(
            unsubscribe(leaving, &channel),
            subscribe(entering, &channel)
        );
        assert_eq!(left["type"], "unsubscribed");
        assert_eq!(entered["type"], "subscribed", "{entered}");
    }
    assert_eq!(
        settled_presence(&mut alice_socket, &bob_id).await,
        Some(true)
    );
    assert!(online_ids(&app, &alice, &m).await.contains(&json!(bob_id)));

    // The last tab leaves (unsubscribing a channel not followed is a no-op):
    // bob is offline everywhere.
    for tab in &mut tabs {
        unsubscribe(tab, &channel).await;
    }
    assert_eq!(
        settled_presence(&mut alice_socket, &bob_id).await,
        Some(false)
    );
    assert!(!online_ids(&app, &alice, &m).await.contains(&json!(bob_id)));
}

// --- Echo suppression --------------------------------------------------------

#[tokio::test]
async fn task_events_name_the_tab_that_made_the_change() {
    let app = TestApp::new().await;
    let addr = app.spawn_server().await;
    let alice = app.token_for("alice").await;
    let team = create_team(&app, &alice, "Platform").await;
    let (m, group) = app.discussing(&alice, team).await;
    let mut socket = connect(&addr, &ticket(&app, &alice).await).await;
    subscribe(&mut socket, &format!("team:{team}")).await;
    let tab = Uuid::now_v7().to_string();
    let from_tab = [("X-Client-Id", tab.as_str())];

    let res = app
        .discuss_task(&alice, &m, &group, "<p>Echo</p>", &from_tab)
        .await;
    assert_eq!(res.status, 201, "{}", res.text());
    let task = TestApp::last_task(&res.json());
    let created = next_of(&mut socket, "task.created").await;
    assert_eq!(created["payload"]["clientId"], tab);

    let res = app
        .call_with(
            "PUT",
            &format!("/api/v1/tasks/{task}/position"),
            &alice,
            Some(json!({ "status": "done" })),
            &from_tab,
        )
        .await;
    assert_eq!(res.status, 200, "{}", res.text());
    assert_eq!(
        next_of(&mut socket, "task.moved").await["payload"]["clientId"],
        tab
    );

    let res = app
        .call_with(
            "PATCH",
            &format!("/api/v1/tasks/{task}"),
            &alice,
            Some(json!({ "content": "<p>Edited</p>" })),
            &from_tab,
        )
        .await;
    assert_eq!(res.status, 200, "{}", res.text());
    assert_eq!(
        next_of(&mut socket, "task.updated").await["payload"]["clientId"],
        tab
    );

    // Without the header, or with garbage in it, no tab is named.
    let res = app
        .call(
            "PATCH",
            &format!("/api/v1/tasks/{task}"),
            &alice,
            Some(json!({ "content": "<p>Plain</p>" })),
        )
        .await;
    assert_eq!(res.status, 200, "{}", res.text());
    assert_eq!(
        next_of(&mut socket, "task.updated").await["payload"]["clientId"],
        Value::Null
    );
    let res = app
        .call_with(
            "PATCH",
            &format!("/api/v1/tasks/{task}"),
            &alice,
            Some(json!({ "content": "<p>Odd</p>" })),
            &[("X-Client-Id", "not-a-uuid")],
        )
        .await;
    assert_eq!(res.status, 200, "{}", res.text());
    assert_eq!(
        next_of(&mut socket, "task.updated").await["payload"]["clientId"],
        Value::Null
    );

    let res = app
        .call_with(
            "DELETE",
            &format!("/api/v1/tasks/{task}"),
            &alice,
            None,
            &from_tab,
        )
        .await;
    assert_eq!(res.status, 204, "{}", res.text());
    let deleted = next_of(&mut socket, "task.deleted").await;
    assert_eq!(deleted["payload"]["clientId"], tab);
    assert_eq!(deleted["payload"]["taskId"], task);
}

/// Publishes a notification as another replica would.
async fn notify_raw(app: &TestApp, notification: Value) {
    use sea_orm::{ConnectionTrait, DbBackend, Statement};
    app.db
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT pg_notify('retro_events', $1)",
            [notification.to_string().into()],
        ))
        .await
        .unwrap();
}

/// The next presence change of `user`.
async fn presence_of(socket: &mut Socket, user: &str) -> bool {
    loop {
        let event = next_of(socket, "presence.changed").await;
        if event["payload"]["userId"] == user {
            return event["payload"]["online"].as_bool().unwrap();
        }
    }
}

#[tokio::test]
async fn a_replica_that_goes_silent_loses_its_presence() {
    let app = TestApp::with_config(|c| c.presence_heartbeat = Duration::from_millis(100)).await;
    let addr = app.spawn_server().await;
    let alice = app.token_for("alice").await;
    let bob = app.token_for("bob").await;
    let bob_id = user_id(&app, &bob).await;
    let team = create_team(&app, &alice, "Platform").await;
    join_team(&app, &alice, team, &bob).await;
    let m = start_meeting(&app, &alice, team).await;
    let mut socket = connect(&addr, &ticket(&app, &alice).await).await;
    subscribe(&mut socket, &format!("meeting:{m}")).await;

    // Bob's socket lives on another replica, which then dies without a word.
    notify_raw(
        &app,
        json!({
            "channel": format!("meeting:{m}"),
            "type": "presence.changed",
            "seq": null,
            "payload": {
                "meetingId": m, "teamId": team, "userId": bob_id,
                "online": true, "replica": Uuid::now_v7(),
            },
        }),
    )
    .await;
    assert!(presence_of(&mut socket, &bob_id).await);
    // Three missed heartbeats later, Bob is offline.
    assert!(!presence_of(&mut socket, &bob_id).await);
    let ids = online_ids(&app, &alice, &m).await;
    assert!(!ids.contains(&json!(bob_id)), "{ids:?}");
}

#[tokio::test]
async fn a_stopping_replica_takes_its_users_offline() {
    let app = TestApp::new().await;
    let addr = app.spawn_server().await;
    let alice = app.token_for("alice").await;
    let bob = app.token_for("bob").await;
    let bob_id = user_id(&app, &bob).await;
    let team = create_team(&app, &alice, "Platform").await;
    join_team(&app, &alice, team, &bob).await;
    let m = start_meeting(&app, &alice, team).await;
    let channel = format!("meeting:{m}");
    let mut alice_socket = connect(&addr, &ticket(&app, &alice).await).await;
    subscribe(&mut alice_socket, &channel).await;
    let mut bob_socket = connect(&addr, &ticket(&app, &bob).await).await;
    subscribe(&mut bob_socket, &channel).await;
    assert!(presence_of(&mut alice_socket, &bob_id).await);

    // The goodbye goes out although Bob's socket is still open.
    app.state.hub.shutdown().await;
    assert!(!presence_of(&mut alice_socket, &bob_id).await);
}

// --- Reflect: counts, author-only events, writing state (ADR-0012) ---------

async fn to_reflect(app: &TestApp, alice: &str, m: &str) {
    let res = app
        .call(
            "POST",
            &format!("/api/v1/meetings/{m}/phase/next"),
            alice,
            None,
        )
        .await;
    assert_eq!(res.status, 200, "{}", res.text());
}

async fn send_writing(socket: &mut Socket, m: &str, column: Value, mode: &str) {
    socket
        .send(Message::text(
            json!({ "type": "writing", "channel": format!("meeting:{m}"), "column": column, "mode": mode })
                .to_string(),
        ))
        .await
        .unwrap();
}

/// The next `reflection.writing` event's counts.
async fn writing_counts(socket: &mut Socket) -> Value {
    next_of(socket, "reflection.writing").await["payload"]["counts"].clone()
}

fn writing(column: &str, new: u32, edit: u32) -> Value {
    let mut counts = json!({
        "plus": { "new": 0, "edit": 0 },
        "delta": { "new": 0, "edit": 0 },
        "ideas": { "new": 0, "edit": 0 },
        "thanks": { "new": 0, "edit": 0 },
    });
    counts[column] = json!({ "new": new, "edit": edit });
    counts
}

#[tokio::test]
async fn reflection_content_reaches_only_its_author() {
    let app = TestApp::new().await;
    let addr = app.spawn_server().await;
    let (alice, bob, _, _, m) = team_with_meeting(&app).await;
    to_reflect(&app, &alice, &m).await;
    let channel = format!("meeting:{m}");
    let mut alice_socket = connect(&addr, &ticket(&app, &alice).await).await;
    subscribe(&mut alice_socket, &channel).await;
    let mut bob_tab = connect(&addr, &ticket(&app, &bob).await).await;
    subscribe(&mut bob_tab, &channel).await;

    let tab = Uuid::now_v7().to_string();
    let res = app
        .call_with(
            "POST",
            &format!("/api/v1/meetings/{m}/reflections"),
            &bob,
            Some(json!({ "column": "delta", "content": "<p>Secret</p>" })),
            &[("x-client-id", &tab)],
        )
        .await;
    assert_eq!(res.status, 201, "{}", res.text());
    let id = res.json()["reflections"][0]["id"].clone();

    // Everyone gets the counts, with a seq.
    let counts = next_of(&mut alice_socket, "reflection.count_changed").await;
    assert!(counts["seq"].is_i64());
    assert_eq!(
        counts["payload"]["counts"],
        json!({ "plus": 0, "delta": 1, "ideas": 0, "thanks": 0 })
    );
    // Bob's tabs also learn which reflection changed and which tab did it.
    let mut bob_events = Vec::new();
    while bob_events.len() < 2 {
        let event = next(&mut bob_tab).await;
        if event["type"] == "reflection.count_changed" || event["type"] == "reflection.changed" {
            bob_events.push(event);
        }
    }
    let changed = bob_events
        .iter()
        .find(|e| e["type"] == "reflection.changed")
        .unwrap();
    assert_eq!(changed["payload"], json!({ "id": id, "clientId": tab }));
    assert_eq!(changed["seq"], Value::Null);
    assert!(changed.get("user").is_none());
    // Alice never gets the author-only event, let alone the content.
    let leaked = tokio::time::timeout(Duration::from_millis(500), async {
        loop {
            let event = next(&mut alice_socket).await;
            if event["type"] != "presence.changed" {
                return event;
            }
        }
    })
    .await;
    assert!(leaked.is_err(), "unexpected: {leaked:?}");
}

#[tokio::test]
async fn everyone_sees_how_many_reflections_are_being_written() {
    let app = TestApp::new().await;
    let addr = app.spawn_server().await;
    let (alice, bob, _, _, m) = team_with_meeting(&app).await;
    to_reflect(&app, &alice, &m).await;
    let channel = format!("meeting:{m}");
    let mut alice_socket = connect(&addr, &ticket(&app, &alice).await).await;
    subscribe(&mut alice_socket, &channel).await;
    let mut bob_socket = connect(&addr, &ticket(&app, &bob).await).await;
    subscribe(&mut bob_socket, &channel).await;

    send_writing(&mut bob_socket, &m, json!("delta"), "new").await;
    assert_eq!(
        writing_counts(&mut alice_socket).await,
        writing("delta", 1, 0)
    );
    // The snapshot has it too.
    let state = app
        .call("GET", &format!("/api/v1/meetings/{m}/state"), &alice, None)
        .await
        .json();
    assert_eq!(state["writing"], writing("delta", 1, 0));

    // Switching to editing a sent card in Ideas moves the count.
    send_writing(&mut bob_socket, &m, json!("ideas"), "edit").await;
    assert_eq!(
        writing_counts(&mut alice_socket).await,
        writing("ideas", 0, 1)
    );
    send_writing(&mut bob_socket, &m, Value::Null, "new").await;
    assert_eq!(
        writing_counts(&mut alice_socket).await,
        writing("plus", 0, 0)
    );

    // A closed socket stops writing.
    send_writing(&mut bob_socket, &m, json!("thanks"), "new").await;
    assert_eq!(
        writing_counts(&mut alice_socket).await,
        writing("thanks", 1, 0)
    );
    bob_socket.close(None).await.unwrap();
    assert_eq!(
        writing_counts(&mut alice_socket).await,
        writing("plus", 0, 0)
    );
}

#[tokio::test]
async fn two_tabs_writing_add_up_and_unsubscribing_stops_writing() {
    let app = TestApp::new().await;
    let addr = app.spawn_server().await;
    let (alice, bob, _, _, m) = team_with_meeting(&app).await;
    to_reflect(&app, &alice, &m).await;
    let channel = format!("meeting:{m}");
    let mut alice_socket = connect(&addr, &ticket(&app, &alice).await).await;
    subscribe(&mut alice_socket, &channel).await;
    let mut one = connect(&addr, &ticket(&app, &bob).await).await;
    subscribe(&mut one, &channel).await;
    let mut two = connect(&addr, &ticket(&app, &bob).await).await;
    subscribe(&mut two, &channel).await;

    send_writing(&mut one, &m, json!("plus"), "new").await;
    assert_eq!(
        writing_counts(&mut alice_socket).await,
        writing("plus", 1, 0)
    );
    send_writing(&mut two, &m, json!("plus"), "new").await;
    assert_eq!(
        writing_counts(&mut alice_socket).await,
        writing("plus", 2, 0)
    );
    // Repeating the same state changes nothing.
    send_writing(&mut two, &m, json!("plus"), "new").await;
    unsubscribe(&mut one, &channel).await;
    assert_eq!(
        writing_counts(&mut alice_socket).await,
        writing("plus", 1, 0)
    );
}

#[tokio::test]
async fn writing_needs_a_running_meeting_the_socket_follows() {
    let app = TestApp::new().await;
    let addr = app.spawn_server().await;
    let (alice, _, _, team, m) = team_with_meeting(&app).await;
    let mut socket = connect(&addr, &ticket(&app, &alice).await).await;

    send_writing(&mut socket, &m, json!("plus"), "new").await;
    let error = next(&mut socket).await;
    assert_eq!(error["type"], "error");
    assert_eq!(error["payload"]["code"], "not_subscribed");

    // A team channel has no reflections.
    let team_channel = format!("team:{team}");
    subscribe(&mut socket, &team_channel).await;
    socket
        .send(Message::text(
            json!({ "type": "writing", "channel": team_channel, "column": "plus" }).to_string(),
        ))
        .await
        .unwrap();
    let error = next_of(&mut socket, "error").await;
    assert_eq!(error["payload"]["code"], "not_writable");

    // An unknown column is not a valid message.
    subscribe(&mut socket, &format!("meeting:{m}")).await;
    socket
        .send(Message::text(
            json!({ "type": "writing", "channel": format!("meeting:{m}"), "column": "kudos" })
                .to_string(),
        ))
        .await
        .unwrap();
    let error = next_of(&mut socket, "error").await;
    assert_eq!(error["payload"]["code"], "invalid_message");
}

#[tokio::test]
async fn an_ended_meeting_has_no_writers() {
    let app = TestApp::new().await;
    let addr = app.spawn_server().await;
    let (alice, bob, _, _, m) = team_with_meeting(&app).await;
    to_reflect(&app, &alice, &m).await;
    let channel = format!("meeting:{m}");
    let mut bob_socket = connect(&addr, &ticket(&app, &bob).await).await;
    subscribe(&mut bob_socket, &channel).await;
    send_writing(&mut bob_socket, &m, json!("plus"), "new").await;
    assert_eq!(writing_counts(&mut bob_socket).await, writing("plus", 1, 0));

    let res = app
        .call("POST", &format!("/api/v1/meetings/{m}/end"), &alice, None)
        .await;
    assert_eq!(res.status, 200, "{}", res.text());
    // The snapshot of an ended meeting shows no editors.
    let state = app
        .call("GET", &format!("/api/v1/meetings/{m}/state"), &alice, None)
        .await
        .json();
    assert_eq!(state["writing"], writing("plus", 0, 0));

    // Writing in it is refused, and stops what the socket was writing.
    send_writing(&mut bob_socket, &m, json!("delta"), "new").await;
    let error = next_of(&mut bob_socket, "error").await;
    assert_eq!(error["payload"]["code"], "not_writable");
    assert_eq!(writing_counts(&mut bob_socket).await, writing("plus", 0, 0));
}

#[tokio::test]
async fn writing_messages_are_rate_limited() {
    let app = TestApp::new().await;
    let addr = app.spawn_server().await;
    let (alice, bob, _, _, m) = team_with_meeting(&app).await;
    to_reflect(&app, &alice, &m).await;
    let channel = format!("meeting:{m}");
    let mut alice_socket = connect(&addr, &ticket(&app, &alice).await).await;
    subscribe(&mut alice_socket, &channel).await;
    let mut bob_socket = connect(&addr, &ticket(&app, &bob).await).await;
    subscribe(&mut bob_socket, &channel).await;

    // Ten toggles go through; the eleventh (opening Ideas) is dropped.
    for i in 0..10 {
        let column = if i % 2 == 0 {
            json!("plus")
        } else {
            Value::Null
        };
        send_writing(&mut bob_socket, &m, column, "new").await;
    }
    send_writing(&mut bob_socket, &m, json!("ideas"), "new").await;
    // Each accepted toggle changes the total: one event each.
    for i in 0..10 {
        let event = next_of(&mut alice_socket, "reflection.writing").await;
        let expected = if i % 2 == 0 { 1 } else { 0 };
        assert_eq!(event["payload"]["counts"], writing("plus", expected, 0));
    }
    // Past the window, the next message goes through, and its event is the
    // next one: the eleventh (Ideas), handled before it, was dropped.
    tokio::time::sleep(Duration::from_millis(1100)).await;
    send_writing(&mut bob_socket, &m, json!("delta"), "new").await;
    let event = next_of(&mut alice_socket, "reflection.writing").await;
    assert_eq!(event["payload"]["counts"], writing("delta", 1, 0));
    let state = app
        .call("GET", &format!("/api/v1/meetings/{m}/state"), &alice, None)
        .await
        .json();
    assert_eq!(state["writing"], writing("delta", 1, 0));
}

#[tokio::test]
async fn a_silent_replica_stops_writing() {
    let app = TestApp::with_config(|c| c.presence_heartbeat = Duration::from_millis(100)).await;
    let addr = app.spawn_server().await;
    let (alice, _, _, _, m) = team_with_meeting(&app).await;
    let mut socket = connect(&addr, &ticket(&app, &alice).await).await;
    subscribe(&mut socket, &format!("meeting:{m}")).await;

    let mut counts = [[0u32; 2]; 4];
    counts[1][0] = 2;
    notify_raw(
        &app,
        json!({
            "channel": format!("meeting:{m}"),
            "type": "writing.changed",
            "seq": null,
            "payload": { "meetingId": m, "replica": Uuid::now_v7(), "counts": counts },
        }),
    )
    .await;
    assert_eq!(writing_counts(&mut socket).await, writing("delta", 2, 0));
    // Three missed heartbeats later, nobody is writing there.
    assert_eq!(writing_counts(&mut socket).await, writing("plus", 0, 0));
}

#[tokio::test]
async fn timer_and_ready_changes_reach_everyone() {
    let app = TestApp::new().await;
    let addr = app.spawn_server().await;
    let (alice, bob, bob_id, _, m) = team_with_meeting(&app).await;
    to_reflect(&app, &alice, &m).await;
    let mut socket = connect(&addr, &ticket(&app, &bob).await).await;
    subscribe(&mut socket, &format!("meeting:{m}")).await;

    let res = app
        .call(
            "PUT",
            &format!("/api/v1/meetings/{m}/timer"),
            &alice,
            Some(json!({ "action": "start", "durationSeconds": 120 })),
        )
        .await;
    assert_eq!(res.status, 200, "{}", res.text());
    let event = next_of(&mut socket, "timer.updated").await;
    assert!(event["seq"].is_i64());
    assert_eq!(event["payload"]["timer"], res.json()["timer"]);
    assert!(event["payload"]["serverNow"].is_string());

    app.call(
        "PUT",
        &format!("/api/v1/meetings/{m}/timer"),
        &alice,
        Some(json!({ "action": "stop" })),
    )
    .await;
    let event = next_of(&mut socket, "timer.updated").await;
    assert_eq!(event["payload"]["timer"], Value::Null);

    let res = app
        .call(
            "PUT",
            &format!("/api/v1/meetings/{m}/ready"),
            &bob,
            Some(json!({ "ready": true })),
        )
        .await;
    assert_eq!(res.status, 200, "{}", res.text());
    let event = next_of(&mut socket, "ready.changed").await;
    assert_eq!(
        event["payload"],
        json!({ "userId": bob_id, "ready": true, "readyCount": 1 })
    );
}

// --- Group ----------------------------------------------------------------

/// Sends a reflection; returns its id.
async fn send_reflection(app: &TestApp, token: &str, m: &str, column: &str) -> String {
    let res = app
        .call(
            "POST",
            &format!("/api/v1/meetings/{m}/reflections"),
            token,
            Some(json!({ "column": column, "content": "<p>Card</p>" })),
        )
        .await;
    assert_eq!(res.status, 201, "{}", res.text());
    res.json()["reflections"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// A meeting in Group with two of bob's cards (delta, ideas).
async fn grouping(app: &TestApp) -> (String, String, String, String, String, String) {
    let (alice, bob, bob_id, _, m) = team_with_meeting(app).await;
    to_reflect(app, &alice, &m).await;
    let one = send_reflection(app, &bob, &m, "delta").await;
    let two = send_reflection(app, &bob, &m, "ideas").await;
    to_reflect(app, &alice, &m).await;
    (alice, bob, bob_id, m, one, two)
}

async fn send_drag(socket: &mut Socket, m: &str, card: Value) {
    socket
        .send(Message::text(
            json!({ "type": "drag", "channel": format!("meeting:{m}"), "reflectionId": card })
                .to_string(),
        ))
        .await
        .unwrap();
}

/// The next `card.dragging` event's cards.
async fn drags(socket: &mut Socket) -> Value {
    next_of(socket, "card.dragging").await["payload"]["drags"].clone()
}

#[tokio::test]
async fn everyone_sees_who_drags_which_card() {
    let app = TestApp::new().await;
    let addr = app.spawn_server().await;
    let (alice, bob, bob_id, m, one, two) = grouping(&app).await;
    let channel = format!("meeting:{m}");
    let mut alice_socket = connect(&addr, &ticket(&app, &alice).await).await;
    subscribe(&mut alice_socket, &channel).await;
    let mut bob_socket = connect(&addr, &ticket(&app, &bob).await).await;
    subscribe(&mut bob_socket, &channel).await;

    send_drag(&mut bob_socket, &m, json!(one)).await;
    let dragging = json!([{ "reflectionId": one, "userId": bob_id }]);
    assert_eq!(drags(&mut alice_socket).await, dragging);
    // In the snapshot too.
    let state = app
        .call("GET", &format!("/api/v1/meetings/{m}/state"), &alice, None)
        .await
        .json();
    assert_eq!(state["drags"], dragging);

    // Picking up another card drops the first; dropping clears it.
    send_drag(&mut bob_socket, &m, json!(two)).await;
    assert_eq!(
        drags(&mut alice_socket).await,
        json!([{ "reflectionId": two, "userId": bob_id }])
    );
    send_drag(&mut bob_socket, &m, Value::Null).await;
    assert_eq!(drags(&mut alice_socket).await, json!([]));

    // A closed socket drops its card.
    send_drag(&mut bob_socket, &m, json!(one)).await;
    assert_eq!(drags(&mut alice_socket).await, dragging);
    bob_socket.close(None).await.unwrap();
    assert_eq!(drags(&mut alice_socket).await, json!([]));
}

#[tokio::test]
async fn only_cards_of_a_meeting_in_group_can_be_dragged() {
    let app = TestApp::new().await;
    let addr = app.spawn_server().await;
    let (alice, bob, _, _, m) = team_with_meeting(&app).await;
    to_reflect(&app, &alice, &m).await;
    let card = send_reflection(&app, &bob, &m, "plus").await;
    let channel = format!("meeting:{m}");
    let mut bob_socket = connect(&addr, &ticket(&app, &bob).await).await;
    subscribe(&mut bob_socket, &channel).await;

    // Still in Reflect.
    send_drag(&mut bob_socket, &m, json!(card)).await;
    let error = next_of(&mut bob_socket, "error").await;
    assert_eq!(error["payload"]["code"], "not_draggable");

    // In Group, a card of no meeting of ours.
    to_reflect(&app, &alice, &m).await;
    send_drag(&mut bob_socket, &m, json!(Uuid::now_v7())).await;
    let error = next_of(&mut bob_socket, "error").await;
    assert_eq!(error["payload"]["code"], "not_draggable");
    // Unsubscribed channels are refused as for writing.
    send_drag(&mut bob_socket, &Uuid::now_v7().to_string(), json!(card)).await;
    let error = next_of(&mut bob_socket, "error").await;
    assert_eq!(error["payload"]["code"], "not_subscribed");
}

#[tokio::test]
async fn only_participants_drag_cards() {
    let app = TestApp::new().await;
    let addr = app.spawn_server().await;
    let (alice, bob, _, team, m) = team_with_meeting(&app).await;
    to_reflect(&app, &alice, &m).await;
    let card = send_reflection(&app, &bob, &m, "delta").await;
    to_reflect(&app, &alice, &m).await;
    // Carol is in the team and in the room, but did not join the meeting.
    let carol = app.create_user("carol", "carol@example.com", "Carol").await;
    join_team(&app, &alice, team, &carol).await;
    let channel = format!("meeting:{m}");
    let mut carol_socket = connect(&addr, &ticket(&app, &carol).await).await;
    subscribe(&mut carol_socket, &channel).await;

    send_drag(&mut carol_socket, &m, json!(card)).await;
    let error = next_of(&mut carol_socket, "error").await;
    assert_eq!(error["payload"]["code"], "not_draggable");
    let state = app
        .call("GET", &format!("/api/v1/meetings/{m}/state"), &alice, None)
        .await
        .json();
    assert_eq!(state["drags"], json!([]));
}

#[tokio::test]
async fn group_changes_reach_everyone_in_order() {
    let app = TestApp::new().await;
    let addr = app.spawn_server().await;
    let (alice, bob, _, m, one, two) = grouping(&app).await;
    let channel = format!("meeting:{m}");
    let mut alice_socket = connect(&addr, &ticket(&app, &alice).await).await;
    subscribe(&mut alice_socket, &channel).await;
    let tab = Uuid::now_v7().to_string();

    let res = app
        .call_with(
            "POST",
            &format!("/api/v1/meetings/{m}/groups"),
            &bob,
            Some(json!({ "targetId": one, "reflectionId": two })),
            &[("x-client-id", &tab)],
        )
        .await;
    assert_eq!(res.status, 200, "{}", res.text());
    let seq = res.json()["seq"].as_i64().unwrap();
    let created = next_of(&mut alice_socket, "group.created").await;
    assert_eq!(created["seq"], seq);
    let group = created["payload"]["group"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        created["payload"],
        json!({
            "group": { "id": group, "column": "delta", "position": 0, "name": "" },
            "reflections": [
                { "id": one, "groupId": group, "version": 1 },
                { "id": two, "groupId": group, "version": 1 },
            ],
            "clientId": tab,
        })
    );

    let res = app
        .call(
            "PATCH",
            &format!("/api/v1/groups/{group}"),
            &bob,
            Some(json!({ "name": "CI" })),
        )
        .await;
    assert_eq!(res.status, 200, "{}", res.text());
    let updated = next_of(&mut alice_socket, "group.updated").await;
    assert_eq!(updated["seq"], seq + 1);
    assert_eq!(updated["payload"]["group"]["name"], "CI");
    assert_eq!(updated["payload"]["reflections"], json!([]));
    assert_eq!(updated["payload"]["clientId"], Value::Null);

    // Taking a card out of a pair dissolves the group.
    let res = app
        .call(
            "PUT",
            &format!("/api/v1/reflections/{two}/group"),
            &bob,
            Some(json!({ "groupId": null })),
        )
        .await;
    assert_eq!(res.status, 200, "{}", res.text());
    let deleted = next_of(&mut alice_socket, "group.deleted").await;
    assert_eq!(deleted["seq"], seq + 2);
    assert_eq!(deleted["payload"]["group"], json!({ "id": group }));
    assert_eq!(
        deleted["payload"]["reflections"],
        json!([
            { "id": two, "groupId": null, "version": 2 },
            { "id": one, "groupId": null, "version": 2 },
        ])
    );
}

// --- Vote: own votes only, cast count to the facilitator ------------------

/// A meeting in Vote: alice facilitates, bob (two cards) and dana take part.
/// Returns the group of bob's first card.
async fn voting(app: &TestApp) -> (String, String, String, String, String) {
    let (alice, bob, _, m, one, _) = grouping(app).await;
    let dana = app.token_for("dana").await;
    let team = app
        .call("GET", &format!("/api/v1/meetings/{m}/state"), &alice, None)
        .await
        .json()["teamId"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    join_team(app, &alice, team, &dana).await;
    let res = app
        .call(
            "POST",
            &format!("/api/v1/teams/{team}/meetings/{m}/join"),
            &dana,
            None,
        )
        .await;
    assert_eq!(res.status, 200, "{}", res.text());
    to_reflect(app, &alice, &m).await;
    let state = app
        .call("GET", &format!("/api/v1/meetings/{m}/state"), &bob, None)
        .await
        .json();
    let group = state["reflections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == one.as_str())
        .unwrap()["groupId"]
        .as_str()
        .unwrap()
        .to_owned();
    (alice, bob, dana, m, group)
}

async fn put_vote(app: &TestApp, token: &str, group: &str, tab: &str) {
    let res = app
        .call_with(
            "POST",
            &format!("/api/v1/groups/{group}/vote"),
            token,
            Some(json!({ "delta": 1 })),
            &[("x-client-id", tab)],
        )
        .await;
    assert_eq!(res.status, 200, "{}", res.text());
}

/// The events `socket` receives before the next `kind`, presence aside: a
/// checkpoint published after the event under test, so a late delivery of
/// that event still shows up here.
async fn events_before(socket: &mut Socket, kind: &str) -> Vec<Value> {
    let mut seen = Vec::new();
    loop {
        let event = next(socket).await;
        if event["type"] == kind {
            return seen;
        }
        if event["type"] != "presence.changed" {
            seen.push(event);
        }
    }
}

fn kinds(events: &[Value]) -> Vec<&str> {
    events.iter().map(|e| e["type"].as_str().unwrap()).collect()
}

#[tokio::test]
async fn votes_reach_the_voter_and_the_count_reaches_everyone() {
    let app = TestApp::new().await;
    let addr = app.spawn_server().await;
    let (alice, bob, dana, m, group) = voting(&app).await;
    let channel = format!("meeting:{m}");
    let mut alice_socket = connect(&addr, &ticket(&app, &alice).await).await;
    subscribe(&mut alice_socket, &channel).await;
    let mut bob_socket = connect(&addr, &ticket(&app, &bob).await).await;
    subscribe(&mut bob_socket, &channel).await;
    let mut dana_socket = connect(&addr, &ticket(&app, &dana).await).await;
    subscribe(&mut dana_socket, &channel).await;

    let bob_tab = Uuid::now_v7().to_string();
    put_vote(&app, &bob, &group, &bob_tab).await;
    rename(&app, &alice, &m, "After bob's vote").await;

    // Bob's tabs learn that his votes changed, and which tab did it: no
    // group, no count, no seq. Everyone gets the new total, as it is.
    let before = events_before(&mut bob_socket, "meeting.renamed").await;
    assert_eq!(
        kinds(&before),
        vec!["vote.changed", "vote.cast_changed"],
        "{before:?}"
    );
    assert_eq!(before[0]["payload"], json!({ "clientId": bob_tab }));
    assert_eq!(before[0]["seq"], Value::Null);
    assert!(before[0].get("user").is_none());
    for socket in [&mut alice_socket, &mut dana_socket] {
        let before = events_before(socket, "meeting.renamed").await;
        assert_eq!(kinds(&before), vec!["vote.cast_changed"], "{before:?}");
        assert_eq!(
            before[0]["payload"],
            json!({ "votesCast": 1, "clientId": bob_tab })
        );
        assert!(before[0]["seq"].is_i64(), "{before:?}");
        assert!(before[0].get("user").is_none());
    }

    // Dana's vote reaches dana; only its total reaches the others.
    let dana_tab = Uuid::now_v7().to_string();
    put_vote(&app, &dana, &group, &dana_tab).await;
    rename(&app, &alice, &m, "After dana's vote").await;
    let before = events_before(&mut dana_socket, "meeting.renamed").await;
    assert_eq!(
        kinds(&before),
        vec!["vote.changed", "vote.cast_changed"],
        "{before:?}"
    );
    assert_eq!(before[0]["payload"], json!({ "clientId": dana_tab }));
    for socket in [&mut alice_socket, &mut bob_socket] {
        let before = events_before(socket, "meeting.renamed").await;
        assert_eq!(kinds(&before), vec!["vote.cast_changed"], "{before:?}");
        assert_eq!(
            before[0]["payload"],
            json!({ "votesCast": 2, "clientId": dana_tab })
        );
    }

    // The facilitator's own vote: their other tabs re-read, everyone gets
    // the total.
    let alice_tab = Uuid::now_v7().to_string();
    put_vote(&app, &alice, &group, &alice_tab).await;
    rename(&app, &alice, &m, "After alice's vote").await;
    let before = events_before(&mut alice_socket, "meeting.renamed").await;
    assert_eq!(
        kinds(&before),
        vec!["vote.changed", "vote.cast_changed"],
        "{before:?}"
    );
    assert_eq!(before[1]["payload"]["votesCast"], 3);
    for socket in [&mut bob_socket, &mut dana_socket] {
        let before = events_before(socket, "meeting.renamed").await;
        assert_eq!(kinds(&before), vec!["vote.cast_changed"], "{before:?}");
        assert_eq!(before[0]["payload"]["votesCast"], 3);
    }
}

#[tokio::test]
async fn new_vote_settings_reach_everyone_and_trimmed_voters_reload() {
    let app = TestApp::new().await;
    let addr = app.spawn_server().await;
    let (alice, bob, dana, m, group) = voting(&app).await;
    let tab = Uuid::now_v7().to_string();
    put_vote(&app, &bob, &group, &tab).await;
    put_vote(&app, &bob, &group, &tab).await;
    let channel = format!("meeting:{m}");
    let mut alice_socket = connect(&addr, &ticket(&app, &alice).await).await;
    subscribe(&mut alice_socket, &channel).await;
    let mut bob_socket = connect(&addr, &ticket(&app, &bob).await).await;
    subscribe(&mut bob_socket, &channel).await;
    let mut dana_socket = connect(&addr, &ticket(&app, &dana).await).await;
    subscribe(&mut dana_socket, &channel).await;
    let seq = app
        .call("GET", &format!("/api/v1/meetings/{m}/state"), &dana, None)
        .await
        .json()["seq"]
        .as_i64()
        .unwrap();

    let settings_tab = Uuid::now_v7().to_string();
    let res = app
        .call_with(
            "PATCH",
            &format!("/api/v1/meetings/{m}/settings"),
            &alice,
            Some(json!({ "voteBudget": 1, "voteMaxPerGroup": 1 })),
            &[("x-client-id", &settings_tab)],
        )
        .await;
    assert_eq!(res.status, 200, "{}", res.text());
    rename(&app, &alice, &m, "After the settings").await;

    for socket in [&mut bob_socket, &mut dana_socket] {
        let changed = next_of(socket, "vote.settings_changed").await;
        assert_eq!(changed["seq"], seq + 1);
        assert_eq!(
            changed["payload"],
            json!({ "budget": 1, "maxPerGroup": 1, "clientId": settings_tab })
        );
    }
    // Bob lost a vote: his tabs re-read. Dana lost none, now or late.
    // Everyone gets the total left after the trim.
    let before = events_before(&mut bob_socket, "meeting.renamed").await;
    assert_eq!(
        kinds(&before),
        vec!["vote.changed", "vote.cast_changed"],
        "{before:?}"
    );
    assert_eq!(before[0]["payload"], json!({ "clientId": settings_tab }));
    assert_eq!(
        before[1]["payload"],
        json!({ "votesCast": 1, "clientId": settings_tab })
    );
    let before = events_before(&mut dana_socket, "meeting.renamed").await;
    assert_eq!(kinds(&before), vec!["vote.cast_changed"], "{before:?}");
    let before = events_before(&mut alice_socket, "meeting.renamed").await;
    assert_eq!(
        kinds(&before),
        vec!["vote.settings_changed", "vote.cast_changed"],
        "{before:?}"
    );
    assert_eq!(
        before[1]["payload"],
        json!({ "votesCast": 1, "clientId": settings_tab })
    );
}

// --- Discuss: the current group, reactions and tasks ------------------------

/// A meeting in Discuss: alice facilitates, bob (two single-card groups) and
/// dana take part. Returns the current group, the other group, and the card
/// of the current group.
async fn discussing(app: &TestApp) -> (String, String, String, String, String, String, String) {
    let (alice, bob, dana, m, _) = voting(app).await;
    to_reflect(app, &alice, &m).await;
    let state = app
        .call("GET", &format!("/api/v1/meetings/{m}/state"), &alice, None)
        .await
        .json();
    let current = state["discussGroupId"].as_str().unwrap().to_owned();
    let other = state["discussOrder"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["groupId"].as_str().unwrap().to_owned())
        .find(|g| *g != current)
        .unwrap();
    let card = state["reflections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["groupId"] == current.as_str())
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    (alice, bob, dana, m, current, other, card)
}

#[tokio::test]
async fn everyone_follows_the_current_group_in_order() {
    let app = TestApp::new().await;
    let addr = app.spawn_server().await;
    let (alice, bob, dana, m, _, other, _) = discussing(&app).await;
    let channel = format!("meeting:{m}");
    let mut sockets = Vec::new();
    for token in [&alice, &bob, &dana] {
        let mut socket = connect(&addr, &ticket(&app, token).await).await;
        subscribe(&mut socket, &channel).await;
        sockets.push(socket);
    }

    let tab = Uuid::now_v7().to_string();
    let res = app
        .call_with(
            "PUT",
            &format!("/api/v1/meetings/{m}/discuss/current"),
            &alice,
            Some(json!({ "groupId": other })),
            &[("x-client-id", &tab)],
        )
        .await;
    assert_eq!(res.status, 200, "{}", res.text());
    let seq = res.json()["seq"].clone();
    for socket in &mut sockets {
        let changed = next_of(socket, "discuss.group_changed").await;
        assert_eq!(changed["seq"], seq);
        assert_eq!(
            changed["payload"],
            json!({ "groupId": other, "clientId": tab })
        );
    }
}

#[tokio::test]
async fn reaction_counts_reach_everyone_and_mine_reaches_the_reactor() {
    let app = TestApp::new().await;
    let addr = app.spawn_server().await;
    let (alice, bob, dana, m, _, _, card) = discussing(&app).await;
    let channel = format!("meeting:{m}");
    let mut alice_socket = connect(&addr, &ticket(&app, &alice).await).await;
    subscribe(&mut alice_socket, &channel).await;
    let mut bob_socket = connect(&addr, &ticket(&app, &bob).await).await;
    subscribe(&mut bob_socket, &channel).await;
    let mut dana_socket = connect(&addr, &ticket(&app, &dana).await).await;
    subscribe(&mut dana_socket, &channel).await;

    let tab = Uuid::now_v7().to_string();
    let res = app
        .call_with(
            "PUT",
            &format!("/api/v1/reflections/{card}/reactions/%F0%9F%8E%89"),
            &bob,
            None,
            &[("x-client-id", &tab)],
        )
        .await;
    assert_eq!(res.status, 200, "{}", res.text());
    // The same again changes nothing, and says nothing.
    let res = app
        .call(
            "PUT",
            &format!("/api/v1/reflections/{card}/reactions/%F0%9F%8E%89"),
            &bob,
            None,
        )
        .await;
    assert_eq!(res.status, 200, "{}", res.text());
    rename(&app, &alice, &m, "After the reaction").await;

    let changed = json!({ "reflectionId": card, "emoji": "🎉", "count": 1, "clientId": tab });
    let before = events_before(&mut bob_socket, "meeting.renamed").await;
    assert_eq!(
        kinds(&before),
        vec!["reaction.changed", "reaction.mine_changed"],
        "{before:?}"
    );
    assert_eq!(before[0]["payload"], changed);
    assert!(before[0]["seq"].is_i64());
    assert_eq!(
        before[1]["payload"],
        json!({ "reflectionId": card, "emoji": "🎉", "mine": true, "clientId": tab })
    );
    assert_eq!(before[1]["seq"], Value::Null);
    // The others get the count, never who reacted.
    for socket in [&mut alice_socket, &mut dana_socket] {
        let before = events_before(socket, "meeting.renamed").await;
        assert_eq!(kinds(&before), vec!["reaction.changed"], "{before:?}");
        assert_eq!(before[0]["payload"], changed);
    }
}

#[tokio::test]
async fn tasks_written_in_discuss_reach_the_room_and_the_board() {
    let app = TestApp::new().await;
    let addr = app.spawn_server().await;
    let (alice, bob, _, m, current, _, _) = discussing(&app).await;
    let team = app
        .call("GET", &format!("/api/v1/meetings/{m}/state"), &alice, None)
        .await
        .json()["teamId"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut socket = connect(&addr, &ticket(&app, &alice).await).await;
    subscribe(&mut socket, &format!("meeting:{m}")).await;
    subscribe(&mut socket, &format!("team:{team}")).await;

    let tab = Uuid::now_v7().to_string();
    let res = app
        .discuss_task(
            &bob,
            &m,
            &current,
            "<p>Fix CI</p>",
            &[("x-client-id", &tab)],
        )
        .await;
    assert_eq!(res.status, 201, "{}", res.text());
    let task = TestApp::last_task(&res.json());
    let mut channels = Vec::new();
    for _ in 0..2 {
        let created = next_of(&mut socket, "task.created").await;
        assert_eq!(
            created["payload"],
            json!({ "taskId": task, "status": "active", "clientId": tab })
        );
        channels.push(created["channel"].as_str().unwrap().to_owned());
    }
    channels.sort();
    assert_eq!(
        channels,
        vec![format!("meeting:{m}"), format!("team:{team}")]
    );

    // Once the meeting ended, board changes reach the team channel only.
    let res = app
        .call("POST", &format!("/api/v1/meetings/{m}/end"), &alice, None)
        .await;
    assert_eq!(res.status, 200, "{}", res.text());
    let res = app
        .call(
            "PUT",
            &format!("/api/v1/tasks/{task}/position"),
            &alice,
            Some(json!({ "status": "done" })),
        )
        .await;
    assert_eq!(res.status, 200, "{}", res.text());
    let moved = next_of(&mut socket, "task.moved").await;
    assert_eq!(moved["channel"], format!("team:{team}"));
    assert_silent(&mut socket, Duration::from_millis(500)).await;
}
