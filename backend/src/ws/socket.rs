//! One client's WebSocket: subscriptions, forwarding and keep-alive.

use std::{collections::HashMap, time::Duration};

use axum::{
    extract::{
        Query, State, WebSocketUpgrade,
        ws::{Message, Utf8Bytes, WebSocket},
    },
    response::Response,
};
use futures_util::{SinkExt, StreamExt};
use sea_orm::EntityTrait;
use serde::Deserialize;
use serde_json::json;
use tokio::{sync::broadcast::error::RecvError, sync::mpsc, task::JoinHandle};
use uuid::Uuid;

use super::{Channel, ticket};
use crate::{
    domain::teams,
    entities::{
        meeting::{self, MeetingStatus},
        meeting_participant,
    },
    error::AppError,
    state::AppState,
};

const PING_EVERY: Duration = Duration::from_secs(25);
const MAX_SUBSCRIPTIONS: usize = 32;

#[derive(Debug, Deserialize)]
pub struct WsQuery {
    ticket: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum Command {
    Subscribe { channel: String },
    Unsubscribe { channel: String },
}

struct Subscription {
    forward: JoinHandle<()>,
    /// Set for a meeting the user participates in: (meeting, team).
    presence: Option<(Uuid, Uuid)>,
}

/// `GET /api/v1/ws?ticket=…`: opens the real-time socket (ADR-0011). Not in
/// the OpenAPI document: it is not a REST endpoint.
pub async fn ws_handler(
    State(state): State<AppState>,
    Query(query): Query<WsQuery>,
    ws: WebSocketUpgrade,
) -> Result<Response, AppError> {
    let user_id = query
        .ticket
        .as_deref()
        .and_then(|t| ticket::verify(&state.config.jwt_secret, t))
        .ok_or(AppError::Unauthorized)?;
    Ok(ws.on_upgrade(move |socket| serve(socket, state, user_id)))
}

async fn serve(socket: WebSocket, state: AppState, user_id: Uuid) {
    let (mut sink, mut stream) = socket.split();
    let (out, mut outgoing) = mpsc::channel::<Message>(256);
    let mut subscriptions: HashMap<String, Subscription> = HashMap::new();
    let mut ping = tokio::time::interval(PING_EVERY);
    ping.tick().await;

    loop {
        tokio::select! {
            incoming = stream.next() => match incoming {
                Some(Ok(Message::Text(text))) => {
                    let reply = command(&state, user_id, &text, &out, &mut subscriptions).await;
                    if sink.send(text_message(reply)).await.is_err() {
                        break;
                    }
                }
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                Some(Ok(_)) => {}
            },
            Some(message) = outgoing.recv() => {
                if sink.send(message).await.is_err() {
                    break;
                }
            }
            _ = ping.tick() => {
                if sink.send(Message::Ping(Default::default())).await.is_err() {
                    break;
                }
            }
        }
    }

    for (_, subscription) in subscriptions {
        end(&state, user_id, subscription).await;
    }
}

fn text_message(value: serde_json::Value) -> Message {
    Message::Text(Utf8Bytes::from(value.to_string()))
}

async fn end(state: &AppState, user_id: Uuid, subscription: Subscription) {
    subscription.forward.abort();
    if let Some((meeting, team)) = subscription.presence {
        state.hub.leave(meeting, team, user_id).await;
    }
}

/// Runs one client command and returns the reply.
async fn command(
    state: &AppState,
    user_id: Uuid,
    text: &str,
    out: &mpsc::Sender<Message>,
    subscriptions: &mut HashMap<String, Subscription>,
) -> serde_json::Value {
    let error =
        |channel: &str, code: &str| json!({ "type": "error", "channel": channel, "code": code });
    let Ok(command) = serde_json::from_str::<Command>(text) else {
        return error("", "invalid_message");
    };
    match command {
        Command::Unsubscribe { channel } => {
            if let Some(subscription) = subscriptions.remove(&channel) {
                end(state, user_id, subscription).await;
            }
            json!({ "type": "unsubscribed", "channel": channel })
        }
        Command::Subscribe { channel: name } => {
            if subscriptions.contains_key(&name) {
                return json!({ "type": "subscribed", "channel": name });
            }
            if subscriptions.len() >= MAX_SUBSCRIPTIONS {
                return error(&name, "too_many_subscriptions");
            }
            let Ok(channel) = name.parse::<Channel>() else {
                return error(&name, "invalid_channel");
            };
            let presence = match authorize(state, user_id, channel).await {
                Ok(presence) => presence,
                Err(AppError::NotFound(code)) => return error(&name, code),
                Err(_) => return error(&name, "internal"),
            };
            // Subscribe before acknowledging, so no event after the ack is lost.
            let receiver = state.hub.subscribe(&name);
            let forward = tokio::spawn(forward(receiver, out.clone(), name.clone()));
            if let Some((meeting, team)) = presence {
                state.hub.enter(meeting, team, user_id).await;
            }
            subscriptions.insert(name.clone(), Subscription { forward, presence });
            json!({ "type": "subscribed", "channel": name })
        }
    }
}

/// Team members only. A meeting channel also marks a participant of a running
/// meeting as present.
async fn authorize(
    state: &AppState,
    user_id: Uuid,
    channel: Channel,
) -> Result<Option<(Uuid, Uuid)>, AppError> {
    match channel {
        Channel::Team(team_id) => {
            teams::require_member(&state.db, team_id, user_id).await?;
            Ok(None)
        }
        Channel::Meeting(meeting_id) => {
            let meeting = meeting::Entity::find_by_id(meeting_id)
                .one(&state.db)
                .await?
                .ok_or(AppError::NotFound("meeting_not_found"))?;
            teams::require_member(&state.db, meeting.team_id, user_id)
                .await
                .map_err(|e| match e {
                    AppError::NotFound(_) => AppError::NotFound("meeting_not_found"),
                    other => other,
                })?;
            let participant = meeting_participant::Entity::find_by_id((meeting_id, user_id))
                .one(&state.db)
                .await?
                .is_some();
            Ok((participant && meeting.status == MeetingStatus::Active)
                .then_some((meeting_id, meeting.team_id)))
        }
    }
}

/// Copies a channel's messages to the socket. A socket too slow to keep up is
/// told to resync instead of silently missing events.
async fn forward(
    mut receiver: tokio::sync::broadcast::Receiver<std::sync::Arc<str>>,
    out: mpsc::Sender<Message>,
    channel: String,
) {
    loop {
        let message = match receiver.recv().await {
            Ok(text) => Message::Text(Utf8Bytes::from(text.as_ref())),
            Err(RecvError::Lagged(_)) => text_message(
                json!({ "type": "resync", "channel": channel, "seq": null, "payload": {} }),
            ),
            Err(RecvError::Closed) => break,
        };
        if out.send(message).await.is_err() {
            break;
        }
    }
}
