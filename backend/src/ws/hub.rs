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
const MEMBER_REMOVED: &str = "member.removed";
const TEAM_DELETED: &str = "team.deleted";
/// Access changes buffered per socket; one that lags re-checks everything.
const ACCESS_CAPACITY: usize = 64;

/// A change in who may follow a team's channels, for every local socket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessChange {
    /// `user` (everyone when `None`: the team was deleted) lost access to the
    /// team's channel and its meetings' channels.
    Revoked { team: Uuid, user: Option<Uuid> },
    /// Changes may have been missed (the listener reconnected): every socket
    /// re-checks its subscriptions.
    Unknown,
}

impl AccessChange {
    /// The access change a team event means, if any.
    fn from_event(notification: &Notification) -> Option<Self> {
        let Ok(Channel::Team(team)) = notification.channel.parse::<Channel>() else {
            return None;
        };
        match notification.kind.as_str() {
            MEMBER_REMOVED => {
                let user = notification
                    .payload
                    .get("userId")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<Uuid>().ok())?;
                Some(Self::Revoked {
                    team,
                    user: Some(user),
                })
            }
            TEAM_DELETED => Some(Self::Revoked { team, user: None }),
            _ => None,
        }
    }

    /// Whether this change ends `user`'s subscription to a channel of `team`.
    pub fn revokes(&self, team: Uuid, user: Uuid) -> bool {
        match *self {
            Self::Revoked { team: t, user: who } => t == team && who.is_none_or(|w| w == user),
            Self::Unknown => false,
        }
    }
}

/// Who is online in which meeting, as this replica sees the cluster:
/// meeting → user → replicas holding at least one socket for them.
#[derive(Debug, Default)]
struct Online(HashMap<Uuid, HashMap<Uuid, HashSet<Uuid>>>);

impl Online {
    fn users(&self, meeting: Uuid) -> HashSet<Uuid> {
        self.0
            .get(&meeting)
            .map(|users| users.keys().copied().collect())
            .unwrap_or_default()
    }

    /// Folds in a replica's announcement; true when the user's overall
    /// status (online on any replica) changed.
    fn apply(&mut self, meeting: Uuid, user: Uuid, replica: Uuid, online: bool) -> bool {
        let users = self.0.entry(meeting).or_default();
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
            self.0.remove(&meeting);
        }
        was_online != is_online
    }

    /// Keeps only `replica`'s entries (the others are asked again).
    fn keep_only(&mut self, replica: Uuid) {
        for users in self.0.values_mut() {
            for replicas in users.values_mut() {
                replicas.retain(|r| *r == replica);
            }
            users.retain(|_, replicas| !replicas.is_empty());
        }
        self.0.retain(|_, users| !users.is_empty());
    }
}

/// This replica's sockets: (meeting, user) → (team, socket count).
type Local = HashMap<(Uuid, Uuid), (Uuid, usize)>;

pub struct Hub {
    replica: Uuid,
    db: DatabaseConnection,
    channels: Mutex<HashMap<String, broadcast::Sender<Arc<str>>>>,
    online: Mutex<Online>,
    /// Held from a socket count transition until its announcement is sent, so
    /// this replica's online/offline notifications commit in transition order
    /// (two tabs of one user entering and leaving at once cannot leave the
    /// cluster believing the opposite of the truth).
    local: tokio::sync::Mutex<Local>,
    access: broadcast::Sender<AccessChange>,
    ready: watch::Sender<bool>,
}

impl Hub {
    pub fn new(db: DatabaseConnection) -> Arc<Self> {
        Arc::new(Self {
            replica: Uuid::now_v7(),
            db,
            channels: Mutex::default(),
            online: Mutex::default(),
            local: tokio::sync::Mutex::default(),
            access: broadcast::channel(ACCESS_CAPACITY).0,
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

    /// Access changes, for a socket to end the subscriptions it lost. Take it
    /// before authorising any subscription, so none is missed.
    pub fn access_changes(&self) -> broadcast::Receiver<AccessChange> {
        self.access.subscribe()
    }

    /// The users online in a meeting, on any replica.
    pub fn online(&self, meeting_id: Uuid) -> HashSet<Uuid> {
        self.online.lock().expect("hub presence").users(meeting_id)
    }

    /// A socket of `user_id` entered the meeting room.
    pub async fn enter(&self, meeting_id: Uuid, team_id: Uuid, user_id: Uuid) {
        let mut local = self.local.lock().await;
        let entry = local.entry((meeting_id, user_id)).or_insert((team_id, 0));
        entry.1 += 1;
        if entry.1 == 1 {
            self.announce(meeting_id, team_id, user_id, true).await;
        }
    }

    /// A socket of `user_id` left the meeting room.
    pub async fn leave(&self, meeting_id: Uuid, team_id: Uuid, user_id: Uuid) {
        let mut local = self.local.lock().await;
        let last = match local.get_mut(&(meeting_id, user_id)) {
            Some(entry) if entry.1 > 1 => {
                entry.1 -= 1;
                false
            }
            Some(_) => {
                local.remove(&(meeting_id, user_id));
                true
            }
            None => false,
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
        // Held throughout, so a socket leaving meanwhile announces after.
        let local = self.local.lock().await;
        for (&(meeting, user), &(team, _)) in local.iter() {
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
    /// for it again, tell local clients they may have missed events, and have
    /// local sockets re-check their access (a removal may have been missed).
    async fn on_connected(&self) {
        let _ = self.access.send(AccessChange::Unknown);
        self.online
            .lock()
            .expect("hub presence")
            .keep_only(self.replica);
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
            _ => {
                // The event first (the remaining members see it), then the
                // revocation: sockets handle it before any later event.
                self.send(&notification);
                if let Some(change) = AccessChange::from_event(&notification) {
                    let _ = self.access.send(change);
                }
            }
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
        let changed = self
            .online
            .lock()
            .expect("hub presence")
            .apply(meeting, user, replica, online);
        if changed {
            self.send(&Notification {
                channel: notification.channel.clone(),
                kind: PRESENCE_CHANGED.into(),
                seq: None,
                payload: json!({ "userId": user, "online": online }),
            });
            // Meetings pages patch the card's presence: no list reload.
            self.send(&Notification {
                channel: Channel::Team(team).to_string(),
                kind: PRESENCE_CHANGED.into(),
                seq: None,
                payload: json!({ "meetingId": meeting, "userId": user, "online": online }),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_user_is_online_while_any_replica_holds_a_socket() {
        let (meeting, user) = (Uuid::now_v7(), Uuid::now_v7());
        let (a, b) = (Uuid::now_v7(), Uuid::now_v7());
        let mut online = Online::default();
        assert!(online.apply(meeting, user, a, true));
        // Already online: a second replica changes nothing visible.
        assert!(!online.apply(meeting, user, b, true));
        assert!(!online.apply(meeting, user, a, true));
        assert!(!online.apply(meeting, user, a, false));
        assert_eq!(online.users(meeting), HashSet::from([user]));
        assert!(online.apply(meeting, user, b, false));
        assert!(online.users(meeting).is_empty());
        // Nothing left behind; a repeated "offline" is a no-op.
        assert!(online.0.is_empty());
        assert!(!online.apply(meeting, user, b, false));
        assert!(online.0.is_empty());
    }

    #[test]
    fn meetings_are_kept_apart() {
        let (one, two, user, replica) = (
            Uuid::now_v7(),
            Uuid::now_v7(),
            Uuid::now_v7(),
            Uuid::now_v7(),
        );
        let mut online = Online::default();
        online.apply(one, user, replica, true);
        assert!(online.users(two).is_empty());
        assert!(online.apply(two, user, replica, true));
        assert!(online.apply(one, user, replica, false));
        assert_eq!(online.users(two), HashSet::from([user]));
    }

    #[test]
    fn reconnecting_forgets_the_other_replicas_only() {
        let (meeting, mine, theirs) = (Uuid::now_v7(), Uuid::now_v7(), Uuid::now_v7());
        let (me, other, both) = (Uuid::now_v7(), Uuid::now_v7(), Uuid::now_v7());
        let mut online = Online::default();
        online.apply(meeting, me, mine, true);
        online.apply(meeting, other, theirs, true);
        online.apply(meeting, both, mine, true);
        online.apply(meeting, both, theirs, true);
        online.keep_only(mine);
        assert_eq!(online.users(meeting), HashSet::from([me, both]));
        // `both` now depends on this replica alone.
        assert!(online.apply(meeting, both, mine, false));
    }

    fn event(channel: &str, kind: &str, payload: serde_json::Value) -> Notification {
        Notification {
            channel: channel.to_owned(),
            kind: kind.to_owned(),
            seq: Some(1),
            payload,
        }
    }

    #[test]
    fn a_removal_revokes_that_member_only() {
        let (team, kai, jade) = (Uuid::now_v7(), Uuid::now_v7(), Uuid::now_v7());
        let change = AccessChange::from_event(&event(
            &format!("team:{team}"),
            MEMBER_REMOVED,
            json!({ "userId": kai }),
        ))
        .unwrap();
        assert_eq!(
            change,
            AccessChange::Revoked {
                team,
                user: Some(kai)
            }
        );
        assert!(change.revokes(team, kai));
        assert!(!change.revokes(team, jade));
        assert!(!change.revokes(Uuid::now_v7(), kai));
    }

    #[test]
    fn a_deleted_team_revokes_everyone_in_it() {
        let team = Uuid::now_v7();
        let change =
            AccessChange::from_event(&event(&format!("team:{team}"), TEAM_DELETED, json!({})))
                .unwrap();
        assert!(change.revokes(team, Uuid::now_v7()));
        assert!(!change.revokes(Uuid::now_v7(), Uuid::now_v7()));
    }

    #[test]
    fn other_events_change_no_access() {
        let team = Uuid::now_v7();
        let channel = format!("team:{team}");
        assert_eq!(
            AccessChange::from_event(&event(&channel, "task.created", json!({}))),
            None
        );
        // Removals only count on a team channel, with a valid user.
        let meeting = format!("meeting:{team}");
        let kai = json!({ "userId": Uuid::now_v7() });
        assert_eq!(
            AccessChange::from_event(&event(&meeting, MEMBER_REMOVED, kai)),
            None
        );
        let bad = json!({ "userId": "nope" });
        assert_eq!(
            AccessChange::from_event(&event(&channel, MEMBER_REMOVED, bad)),
            None
        );
        assert!(!AccessChange::Unknown.revokes(team, Uuid::now_v7()));
    }
}
