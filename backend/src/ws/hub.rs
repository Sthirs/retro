//! The per-replica side of the hub: listens for notifications and fans them
//! out to local sockets; tracks presence across replicas (ADR-0011).

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::Duration,
};

use sea_orm::DatabaseConnection;
use serde_json::json;
use sqlx::postgres::PgListener;
use tokio::sync::{broadcast, watch};
use uuid::Uuid;

use super::{Channel, Notification, PG_CHANNEL, notify};

/// Messages buffered per channel before a slow socket is told to resync.
const CHANNEL_CAPACITY: usize = 256;
const PRESENCE_CHANGED: &str = "presence.changed";
const PRESENCE_SYNC: &str = "presence.sync";

/// Who is online in which meeting, as this replica sees the cluster.
#[derive(Default)]
struct Presence {
    /// meeting → user → replicas holding at least one socket for them.
    online: HashMap<Uuid, HashMap<Uuid, HashSet<Uuid>>>,
    /// This replica's sockets: (meeting, user) → (team, socket count).
    local: HashMap<(Uuid, Uuid), (Uuid, usize)>,
}

pub struct Hub {
    replica: Uuid,
    db: DatabaseConnection,
    channels: Mutex<HashMap<String, broadcast::Sender<Arc<str>>>>,
    presence: Mutex<Presence>,
    ready: watch::Sender<bool>,
}

impl Hub {
    pub fn new(db: DatabaseConnection) -> Arc<Self> {
        Arc::new(Self {
            replica: Uuid::now_v7(),
            db,
            channels: Mutex::default(),
            presence: Mutex::default(),
            ready: watch::Sender::new(false),
        })
    }

    /// Starts listening for notifications on a dedicated connection, which
    /// reconnects by itself.
    pub fn start(self: &Arc<Self>, database_url: String) {
        let hub = Arc::clone(self);
        tokio::spawn(async move { hub.listen(database_url).await });
    }

    /// Resolves once the listener is connected (tests wait for it).
    pub async fn wait_ready(&self) {
        let mut ready = self.ready.subscribe();
        let _ = ready.wait_for(|r| *r).await;
    }

    /// A receiver of the messages sent on `channel`, already serialized.
    pub fn subscribe(&self, channel: &str) -> broadcast::Receiver<Arc<str>> {
        let mut channels = self.channels.lock().expect("hub channels");
        channels
            .entry(channel.to_owned())
            .or_insert_with(|| broadcast::channel(CHANNEL_CAPACITY).0)
            .subscribe()
    }

    /// The users online in a meeting, on any replica.
    pub fn online(&self, meeting_id: Uuid) -> HashSet<Uuid> {
        let presence = self.presence.lock().expect("hub presence");
        presence
            .online
            .get(&meeting_id)
            .map(|users| users.keys().copied().collect())
            .unwrap_or_default()
    }

    /// A socket of `user_id` entered the meeting room.
    pub async fn enter(&self, meeting_id: Uuid, team_id: Uuid, user_id: Uuid) {
        let first = {
            let mut presence = self.presence.lock().expect("hub presence");
            let entry = presence
                .local
                .entry((meeting_id, user_id))
                .or_insert((team_id, 0));
            entry.1 += 1;
            entry.1 == 1
        };
        if first {
            self.announce(meeting_id, team_id, user_id, true).await;
        }
    }

    /// A socket of `user_id` left the meeting room.
    pub async fn leave(&self, meeting_id: Uuid, team_id: Uuid, user_id: Uuid) {
        let last = {
            let mut presence = self.presence.lock().expect("hub presence");
            match presence.local.get_mut(&(meeting_id, user_id)) {
                Some(entry) if entry.1 > 1 => {
                    entry.1 -= 1;
                    false
                }
                Some(_) => {
                    presence.local.remove(&(meeting_id, user_id));
                    true
                }
                None => false,
            }
        };
        if last {
            self.announce(meeting_id, team_id, user_id, false).await;
        }
    }

    async fn announce(&self, meeting_id: Uuid, team_id: Uuid, user_id: Uuid, online: bool) {
        let notification = Notification {
            channel: Channel::Meeting(meeting_id).to_string(),
            kind: PRESENCE_CHANGED.into(),
            seq: None,
            payload: json!({
                "meetingId": meeting_id,
                "teamId": team_id,
                "userId": user_id,
                "online": online,
                "replica": self.replica,
            }),
        };
        if let Err(error) = notify(&self.db, &notification).await {
            tracing::warn!(%error, "cannot publish presence");
        }
    }

    /// Re-announces this replica's sockets (answer to `presence.sync`).
    async fn announce_local(&self) {
        let local: Vec<_> = {
            let presence = self.presence.lock().expect("hub presence");
            presence
                .local
                .iter()
                .map(|(&(meeting, user), &(team, _))| (meeting, team, user))
                .collect()
        };
        for (meeting, team, user) in local {
            self.announce(meeting, team, user, true).await;
        }
    }

    async fn listen(self: Arc<Self>, database_url: String) {
        let mut backoff = Duration::from_secs(1);
        loop {
            match self.listen_once(&database_url).await {
                Ok(()) => backoff = Duration::from_secs(1),
                Err(error) => tracing::warn!(%error, "real-time listener failed"),
            }
            self.ready.send_replace(false);
            tokio::time::sleep(backoff).await;
            backoff = (backoff * 2).min(Duration::from_secs(30));
        }
    }

    async fn listen_once(&self, database_url: &str) -> Result<(), sqlx::Error> {
        let mut listener = PgListener::connect(database_url).await?;
        listener.listen(PG_CHANNEL).await?;
        self.on_connected().await;
        loop {
            match listener.try_recv().await? {
                Some(notification) => self.dispatch(notification.payload()).await,
                // The connection dropped and was re-established: notifications
                // may have been missed.
                None => self.on_connected().await,
            }
        }
    }

    /// After (re)connecting: forget the other replicas' presence and ask them
    /// for it again, and tell local clients they may have missed events.
    async fn on_connected(&self) {
        {
            let mut presence = self.presence.lock().expect("hub presence");
            let replica = self.replica;
            for users in presence.online.values_mut() {
                for replicas in users.values_mut() {
                    replicas.retain(|r| *r == replica);
                }
                users.retain(|_, replicas| !replicas.is_empty());
            }
            presence.online.retain(|_, users| !users.is_empty());
        }
        let channels: Vec<String> = {
            let channels = self.channels.lock().expect("hub channels");
            channels.keys().cloned().collect()
        };
        for channel in channels {
            self.send(&Notification {
                kind: "resync".into(),
                channel,
                seq: None,
                payload: json!({}),
            });
        }
        let request = Notification {
            channel: String::new(),
            kind: PRESENCE_SYNC.into(),
            seq: None,
            payload: json!({ "replica": self.replica }),
        };
        if let Err(error) = notify(&self.db, &request).await {
            tracing::warn!(%error, "cannot request presence");
        }
        self.ready.send_replace(true);
    }

    async fn dispatch(&self, text: &str) {
        let Ok(notification) = serde_json::from_str::<Notification>(text) else {
            tracing::warn!("malformed real-time notification");
            return;
        };
        match notification.kind.as_str() {
            PRESENCE_SYNC => self.announce_local().await,
            PRESENCE_CHANGED => self.presence_changed(&notification),
            _ => self.send(&notification),
        }
    }

    /// Folds a replica's presence change into the cluster view, and tells
    /// clients when the user's overall status changed.
    fn presence_changed(&self, notification: &Notification) {
        let p = &notification.payload;
        let field = |name: &str| {
            p.get(name)
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<Uuid>().ok())
        };
        let (Some(meeting), Some(team), Some(user), Some(replica), Some(online)) = (
            field("meetingId"),
            field("teamId"),
            field("userId"),
            field("replica"),
            p.get("online").and_then(|v| v.as_bool()),
        ) else {
            return;
        };
        let changed = {
            let mut presence = self.presence.lock().expect("hub presence");
            let users = presence.online.entry(meeting).or_default();
            let was_online = users.contains_key(&user);
            if online {
                users.entry(user).or_default().insert(replica);
            } else if let Some(replicas) = users.get_mut(&user) {
                replicas.remove(&replica);
                if replicas.is_empty() {
                    users.remove(&user);
                }
            }
            let is_online = users.contains_key(&user);
            if users.is_empty() {
                presence.online.remove(&meeting);
            }
            was_online != is_online
        };
        if changed {
            self.send(&Notification {
                channel: notification.channel.clone(),
                kind: PRESENCE_CHANGED.into(),
                seq: None,
                payload: json!({ "userId": user, "online": online }),
            });
            self.send(&Notification {
                channel: Channel::Team(team).to_string(),
                kind: "meeting.updated".into(),
                seq: None,
                payload: json!({ "meetingId": meeting }),
            });
        }
    }

    /// Sends a message to this replica's subscribers of its channel.
    fn send(&self, notification: &Notification) {
        let mut channels = self.channels.lock().expect("hub channels");
        let Some(sender) = channels.get(&notification.channel) else {
            return;
        };
        if sender.receiver_count() == 0 {
            channels.remove(&notification.channel);
            return;
        }
        if let Ok(text) = serde_json::to_string(notification) {
            let _ = sender.send(Arc::from(text));
        }
    }
}
