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
        .call("POST", &format!("/api/v1/meetings/{m}/join"), &bob, None)
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

    app.call(
        "POST",
        &format!("/api/v1/meetings/{m}/facilitator"),
        &alice,
        Some(json!({ "userId": bob_id })),
    )
    .await;
    let changed = next_of(&mut alice_socket, "facilitator.changed").await;
    assert_eq!(changed["payload"]["facilitatorId"], bob_id);
    assert_eq!(changed["seq"].as_i64().unwrap(), first_seq + 2);

    // The snapshot's seq matches the last event.
    let state = app
        .call("GET", &format!("/api/v1/meetings/{m}/state"), &alice, None)
        .await
        .json();
    assert_eq!(state["seq"].as_i64().unwrap(), first_seq + 2);

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
    app.call("POST", &format!("/api/v1/meetings/{m}/join"), &bob, None)
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
    // The Meetings page hears about it too.
    let updated = next_of(&mut team_socket, "meeting.updated").await;
    assert_eq!(updated["payload"]["meetingId"], m);

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
}

#[tokio::test]
async fn task_changes_reach_the_team_channel() {
    let app = TestApp::new().await;
    let addr = app.spawn_server().await;
    let alice = app.token_for("alice").await;
    let team = create_team(&app, &alice, "Platform").await;
    let mut socket = connect(&addr, &ticket(&app, &alice).await).await;
    subscribe(&mut socket, &format!("team:{team}")).await;

    let res = app
        .call(
            "POST",
            &format!("/api/v1/dev/teams/{team}/tasks"),
            &alice,
            Some(json!({ "status": "future", "content": "<p>Write docs</p>" })),
        )
        .await;
    assert_eq!(res.status, 201, "{}", res.text());
    let task = res.json();
    let created = next_of(&mut socket, "task.created").await;
    assert_eq!(created["payload"]["taskId"], task["id"]);

    app.call(
        "PUT",
        &format!("/api/v1/tasks/{}/position", task["id"].as_str().unwrap()),
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
        .call("POST", &format!("/api/v1/meetings/{m}/join"), &bob, None)
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
    let res = app
        .call(
            "POST",
            &format!("/api/v1/dev/teams/{team}/tasks"),
            token,
            Some(json!({ "status": "future", "content": "<p>Later</p>" })),
        )
        .await;
    assert_eq!(res.status, 201, "{}", res.text());
    res.json()
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
        .call("POST", &format!("/api/v1/meetings/{m}/join"), &bob, None)
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
