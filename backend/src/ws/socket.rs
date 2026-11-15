//! One client's WebSocket: subscriptions, forwarding and keep-alive.

use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

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
use tokio::{
    sync::{broadcast, broadcast::error::RecvError, mpsc},
    task::JoinHandle,
};
use uuid::Uuid;

use super::{AccessChange, Channel, WritingMode, hub::Outbound, ticket};
use crate::{
    domain::teams,
    entities::{
        meeting::{self, MeetingStatus},
        reflection::ReflectionColumn,
    },
    error::AppError,
    state::AppState,
};

const PING_EVERY: Duration = Duration::from_secs(25);
const MAX_SUBSCRIPTIONS: usize = 32;
/// The code sent on each channel of a team the user lost access to: the team
/// is gone or they are no longer a member.
const REVOKED: &str = "team_not_found";
/// Ephemeral client messages (`writing`) a socket may send per second; extra
/// ones are dropped (ADR-0012).
const EPHEMERAL_PER_SECOND: u32 = 10;

#[derive(Debug, Deserialize)]
pub struct WsQuery {
    ticket: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum Command {
    Subscribe {
        channel: String,
    },
    Unsubscribe {
        channel: String,
    },
    /// The socket's reflection editor opened (`column`) or closed (`null`)
    /// in a meeting (ADR-0012).
    Writing {
        channel: String,
        column: Option<ReflectionColumn>,
        #[serde(default)]
        mode: Option<WritingMode>,
    },
}

/// What one socket is writing in a meeting.
type Writing = (ReflectionColumn, WritingMode);

/// A fixed one-second window counting ephemeral messages.
struct RateLimit {
    window: Instant,
    count: u32,
}

impl RateLimit {
    fn new() -> Self {
        Self {
            window: Instant::now(),
            count: 0,
        }
    }

    /// Counts one message; false when the window's budget is spent.
    fn allow(&mut self, now: Instant) -> bool {
        if now.duration_since(self.window) >= Duration::from_secs(1) {
            self.window = now;
            self.count = 0;
        }
        self.count += 1;
        self.count <= EPHEMERAL_PER_SECOND
    }
}

struct Subscription {
    forward: JoinHandle<()>,
    /// The team the channel belongs to (a meeting's team for `meeting:`).
    team: Uuid,
    /// Set for a meeting the user participates in: (meeting, team).
    presence: Option<(Uuid, Uuid)>,
    /// The reflection this socket is writing in that meeting, if any.
    writing: Option<Writing>,
}

/// A channel's message on its way to the socket, tagged with its channel so
/// messages of a subscription that has just ended are dropped.
type Outgoing = (Arc<str>, Message);

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
    let (out, mut outgoing) = mpsc::channel::<Outgoing>(256);
    // Taken before any subscription is authorised: a removal that commits
    // while one is being authorised still ends it.
    let mut access = state.hub.access_changes();
    let mut subscriptions: HashMap<String, Subscription> = HashMap::new();
    let mut ping = tokio::time::interval(PING_EVERY);
    ping.tick().await;
    let mut limit = RateLimit::new();

    loop {
        tokio::select! {
            // Access changes first: the hub sends a revocation before any
            // later event of the channel, so with this order no event after a
            // removal reaches the former member.
            biased;
            change = access.recv() => {
                let lost = match change {
                    // Changes may have been missed: ask the database. If it
                    // is unreachable, the listener's reconnect asks again.
                    Ok(AccessChange::Unknown) | Err(RecvError::Lagged(_)) => {
                        lost_on_recheck(&state, user_id, &subscriptions).await.unwrap_or_default()
                    }
                    Ok(change) => lost_to(&change, user_id, &subscriptions),
                    Err(RecvError::Closed) => break,
                };
                let mut closed = false;
                for name in lost {
                    let Some(subscription) = subscriptions.remove(&name) else { continue };
                    end(&state, user_id, subscription).await;
                    if sink.send(text_message(error_message(&name, REVOKED))).await.is_err() {
                        closed = true;
                    }
                }
                if closed {
                    break;
                }
            }
            incoming = stream.next() => match incoming {
                Some(Ok(Message::Text(text))) => {
                    let reply =
                        command(&state, user_id, &text, &out, &mut subscriptions, &mut limit).await;
                    if let Some(reply) = reply
                        && sink.send(text_message(reply)).await.is_err()
                    {
                        break;
                    }
                }
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                Some(Ok(_)) => {}
            },
            Some((channel, message)) = outgoing.recv() => {
                if !subscriptions.contains_key(channel.as_ref()) {
                    continue;
                }
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

/// The subscriptions `change` ends for `user_id`.
fn lost_to(
    change: &AccessChange,
    user_id: Uuid,
    subscriptions: &HashMap<String, Subscription>,
) -> Vec<String> {
    subscriptions
        .iter()
        .filter(|(_, s)| change.revokes(s.team, user_id))
        .map(|(name, _)| name.clone())
        .collect()
}

/// The subscriptions the user may no longer have, checked against the
/// database (after missed access changes).
async fn lost_on_recheck(
    state: &AppState,
    user_id: Uuid,
    subscriptions: &HashMap<String, Subscription>,
) -> Result<Vec<String>, AppError> {
    let mut lost = Vec::new();
    for name in subscriptions.keys() {
        let Ok(channel) = name.parse::<Channel>() else {
            continue;
        };
        match authorize(state, user_id, channel).await {
            Ok(_) => {}
            Err(AppError::NotFound(_)) => lost.push(name.clone()),
            Err(other) => return Err(other),
        }
    }
    Ok(lost)
}

/// An error about a channel, in the shape of every message (spec §11.1).
fn error_message(channel: &str, code: &str) -> serde_json::Value {
    json!({ "type": "error", "channel": channel, "seq": null, "payload": { "code": code } })
}

fn text_message(value: serde_json::Value) -> Message {
    Message::Text(Utf8Bytes::from(value.to_string()))
}

async fn end(state: &AppState, user_id: Uuid, subscription: Subscription) {
    subscription.forward.abort();
    if let Some((meeting, team)) = subscription.presence {
        if subscription.writing.is_some() {
            state
                .hub
                .set_writing(meeting, subscription.writing, None)
                .await;
        }
        state.hub.leave(meeting, team, user_id).await;
    }
}

/// Runs one client command and returns the reply, if it has one.
async fn command(
    state: &AppState,
    user_id: Uuid,
    text: &str,
    out: &mpsc::Sender<Outgoing>,
    subscriptions: &mut HashMap<String, Subscription>,
    limit: &mut RateLimit,
) -> Option<serde_json::Value> {
    let Ok(command) = serde_json::from_str::<Command>(text) else {
        return Some(error_message("", "invalid_message"));
    };
    if let Command::Writing {
        channel,
        column,
        mode,
    } = command
    {
        if !limit.allow(Instant::now()) {
            return None;
        }
        return writing(state, &channel, column, mode, subscriptions)
            .await
            .err()
            .map(|code| error_message(&channel, code));
    }
    Some(subscription_command(state, user_id, command, out, subscriptions).await)
}

/// Records what the socket is writing in a running meeting it is in. Its
/// errors (`not_subscribed`, `not_writable`) are its own codes: clients must
/// not take them for a lost subscription.
async fn writing(
    state: &AppState,
    channel: &str,
    column: Option<ReflectionColumn>,
    mode: Option<WritingMode>,
    subscriptions: &mut HashMap<String, Subscription>,
) -> Result<(), &'static str> {
    let subscription = subscriptions.get_mut(channel).ok_or("not_subscribed")?;
    let (meeting, _) = subscription.presence.ok_or("not_writable")?;
    let mut to = column.map(|c| (c, mode.unwrap_or(WritingMode::New)));
    // The meeting may have ended since the subscription: then the socket
    // stops writing, and is told so. Stopping needs no check.
    let running = to.is_none() || is_running(state, meeting).await;
    if !running {
        to = None;
    }
    let from = std::mem::replace(&mut subscription.writing, to);
    state.hub.set_writing(meeting, from, to).await;
    if running { Ok(()) } else { Err("not_writable") }
}

/// Whether the meeting is still running; false when it cannot be read.
async fn is_running(state: &AppState, meeting_id: Uuid) -> bool {
    match meeting::Entity::find_by_id(meeting_id).one(&state.db).await {
        Ok(found) => found.is_some_and(|m| m.status == MeetingStatus::Active),
        Err(error) => {
            tracing::warn!(%error, "cannot read the meeting status");
            false
        }
    }
}

/// Subscribes or unsubscribes, and returns the reply.
async fn subscription_command(
    state: &AppState,
    user_id: Uuid,
    command: Command,
    out: &mpsc::Sender<Outgoing>,
    subscriptions: &mut HashMap<String, Subscription>,
) -> serde_json::Value {
    let error = error_message;
    match command {
        Command::Writing { .. } => error("", "invalid_message"),
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
            // Only the canonical spelling (lowercase, hyphenated UUID): events
            // are published under it, another spelling would never get any.
            let Some(channel) = name
                .parse::<Channel>()
                .ok()
                .filter(|c| c.to_string() == name)
            else {
                return error(&name, "invalid_channel");
            };
            let (team, presence) = match authorize(state, user_id, channel).await {
                Ok(access) => access,
                Err(AppError::NotFound(code)) => return error(&name, code),
                Err(_) => return error(&name, "internal"),
            };
            // Subscribe before acknowledging, so no event after the ack is lost.
            let receiver = state.hub.subscribe(&name);
            let forward = tokio::spawn(forward(
                receiver,
                out.clone(),
                Arc::from(name.as_str()),
                user_id,
            ));
            if let Some((meeting, team)) = presence {
                state.hub.enter(meeting, team, user_id).await;
            }
            subscriptions.insert(
                name.clone(),
                Subscription {
                    forward,
                    team,
                    presence,
                    writing: None,
                },
            );
            json!({ "type": "subscribed", "channel": name })
        }
    }
}

/// Team members only; returns the channel's team. Subscribing to a running
/// meeting's channel means being in its room, so it also marks the member as
/// present (the room joins the meeting right after; presence does not wait
/// for that).
async fn authorize(
    state: &AppState,
    user_id: Uuid,
    channel: Channel,
) -> Result<(Uuid, Option<(Uuid, Uuid)>), AppError> {
    match channel {
        Channel::Team(team_id) => {
            teams::require_member(&state.db, team_id, user_id).await?;
            Ok((team_id, None))
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
            let presence =
                (meeting.status == MeetingStatus::Active).then_some((meeting_id, meeting.team_id));
            Ok((meeting.team_id, presence))
        }
    }
}

/// Copies a channel's messages to the socket, skipping those meant for
/// another user. A socket too slow to keep up is told to resync instead of
/// silently missing events.
async fn forward(
    mut receiver: broadcast::Receiver<Outbound>,
    out: mpsc::Sender<Outgoing>,
    channel: Arc<str>,
    user_id: Uuid,
) {
    loop {
        let message = match receiver.recv().await {
            Ok(Outbound {
                user: Some(user), ..
            }) if user != user_id => continue,
            Ok(Outbound { text, .. }) => Message::Text(Utf8Bytes::from(text.as_ref())),
            Err(RecvError::Lagged(_)) => text_message(
                json!({ "type": "resync", "channel": channel.as_ref(), "seq": null, "payload": {} }),
            ),
            Err(RecvError::Closed) => break,
        };
        if out.send((Arc::clone(&channel), message)).await.is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_have_the_shape_of_every_message() {
        assert_eq!(
            error_message("meeting:x", "meeting_not_found"),
            json!({
                "type": "error",
                "channel": "meeting:x",
                "seq": null,
                "payload": { "code": "meeting_not_found" },
            })
        );
    }

    #[test]
    fn ephemeral_messages_are_limited_per_second() {
        let mut limit = RateLimit::new();
        let start = limit.window;
        for _ in 0..EPHEMERAL_PER_SECOND {
            assert!(limit.allow(start));
        }
        assert!(!limit.allow(start + Duration::from_millis(999)));
        assert!(limit.allow(start + Duration::from_secs(1)));
    }
}
