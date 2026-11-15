//! The per-replica side of the hub: listens for notifications and fans them
//! out to local sockets; tracks presence across replicas (ADR-0011).

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
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
const PRESENCE_ALIVE: &str = "presence.alive";
/// Heartbeats a replica may miss before the others forget its presence.
const MISSED_HEARTBEATS: u32 = 3;
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
/// meeting → user → their team and the replicas holding a socket for them;
/// plus when each replica was last heard from, so one that stops without
/// saying goodbye (killed, partitioned) is eventually forgotten.
#[derive(Debug, Default)]
struct Online {
    meetings: HashMap<Uuid, HashMap<Uuid, Presence>>,
    seen: HashMap<Uuid, Instant>,
    /// Replicas forgotten for their silence: if one speaks again, every
    /// replica is asked to re-announce.
    expired: HashSet<Uuid>,
}

#[derive(Debug)]
struct Presence {
    team: Uuid,
    replicas: HashSet<Uuid>,
}

/// A user whose overall presence in a meeting changed: (meeting, team, user).
type Change = (Uuid, Uuid, Uuid);

impl Online {
    fn users(&self, meeting: Uuid) -> HashSet<Uuid> {
        self.meetings
            .get(&meeting)
            .map(|users| users.keys().copied().collect())
            .unwrap_or_default()
    }

    /// Folds in a replica's announcement; true when the user's overall
    /// status (online on any replica) changed.
    fn apply(
        &mut self,
        meeting: Uuid,
        team: Uuid,
        user: Uuid,
        replica: Uuid,
        online: bool,
    ) -> bool {
        let users = self.meetings.entry(meeting).or_default();
        let was_online = users.contains_key(&user);
        if online {
            users
                .entry(user)
                .or_insert_with(|| Presence {
                    team,
                    replicas: HashSet::new(),
                })
                .replicas
                .insert(replica);
        } else if let Some(presence) = users.get_mut(&user) {
            presence.replicas.remove(&replica);
            if presence.replicas.is_empty() {
                users.remove(&user);
            }
        }
        let is_online = users.contains_key(&user);
        if users.is_empty() {
            self.meetings.remove(&meeting);
        }
        was_online != is_online
    }

    /// Records that `replica` spoke; true when it had been forgotten.
    fn touch(&mut self, replica: Uuid, now: Instant) -> bool {
        self.seen.insert(replica, now);
        self.expired.remove(&replica)
    }

    /// Forgets the replicas (never `me`) silent for longer than `silence`,
    /// and returns the users that leaves offline.
    fn expire(&mut self, now: Instant, silence: Duration, me: Uuid) -> Vec<Change> {
        let gone: HashSet<Uuid> = self
            .seen
            .iter()
            .filter(|&(&replica, &at)| replica != me && now.duration_since(at) > silence)
            .map(|(&replica, _)| replica)
            .collect();
        if gone.is_empty() {
            return Vec::new();
        }
        self.seen.retain(|replica, _| !gone.contains(replica));
        self.expired.extend(gone.iter().copied());
        let mut offline = Vec::new();
        for (&meeting, users) in &mut self.meetings {
            users.retain(|&user, presence| {
                presence.replicas.retain(|r| !gone.contains(r));
                let keep = !presence.replicas.is_empty();
                if !keep {
                    offline.push((meeting, presence.team, user));
                }
                keep
            });
        }
        self.meetings.retain(|_, users| !users.is_empty());
        offline
    }

    /// Starts over from this replica's own sockets (the others are asked
    /// again): after a listener reconnect even our own announcements may have
    /// been missed.
    fn reset(&mut self, me: Uuid, sockets: &Sockets) {
        *self = Self::default();
        for (&(meeting, user), &(team, _)) in sockets {
            self.apply(meeting, team, user, me, true);
        }
    }
}

/// This replica's sockets: (meeting, user) → (team, socket count).
type Sockets = HashMap<(Uuid, Uuid), (Uuid, usize)>;

/// This replica's side of presence.
#[derive(Debug, Default)]
struct Local {
    sockets: Sockets,
    /// Announcements that could not be published: (meeting, user) → team.
    /// Each heartbeat retries them with the then-current socket count.
    pending: HashMap<(Uuid, Uuid), Uuid>,
    /// Shutting down: this replica has said goodbye; sockets no longer count.
    closing: bool,
}

pub struct Hub {
    replica: Uuid,
    db: DatabaseConnection,
    /// How often this replica tells the others it is alive; a replica silent
    /// for [`MISSED_HEARTBEATS`] of them is forgotten.
    heartbeat: Duration,
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
    pub fn new(db: DatabaseConnection, heartbeat: Duration) -> Arc<Self> {
        Arc::new(Self {
            replica: Uuid::now_v7(),
            db,
            heartbeat,
            channels: Mutex::default(),
            online: Mutex::default(),
            local: tokio::sync::Mutex::default(),
            access: broadcast::channel(ACCESS_CAPACITY).0,
            ready: watch::Sender::new(false),
        })
    }

    /// Starts listening for notifications on a dedicated connection, which
    /// reconnects by itself, and the heartbeat.
    pub fn start(self: &Arc<Self>, database_url: String) {
        let hub = Arc::clone(self);
        tokio::spawn(async move { hub.listen(database_url).await });
        let hub = Arc::clone(self);
        tokio::spawn(async move { hub.beat().await });
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
        if local.closing {
            return;
        }
        let entry = local
            .sockets
            .entry((meeting_id, user_id))
            .or_insert((team_id, 0));
        entry.1 += 1;
        if entry.1 == 1 {
            self.announce(&mut local, meeting_id, team_id, user_id, true)
                .await;
        }
    }

    /// A socket of `user_id` left the meeting room.
    pub async fn leave(&self, meeting_id: Uuid, team_id: Uuid, user_id: Uuid) {
        let mut local = self.local.lock().await;
        let last = match local.sockets.get_mut(&(meeting_id, user_id)) {
            Some(entry) if entry.1 > 1 => {
                entry.1 -= 1;
                false
            }
            Some(_) => {
                local.sockets.remove(&(meeting_id, user_id));
                true
            }
            None => false,
        };
        if last {
            self.announce(&mut local, meeting_id, team_id, user_id, false)
                .await;
        }
    }

    /// Says goodbye before the replica stops: its users go offline on the
    /// other replicas now, not when its silence runs out.
    pub async fn shutdown(&self) {
        let mut local = self.local.lock().await;
        local.closing = true;
        local.pending.clear();
        for ((meeting, user), (team, _)) in std::mem::take(&mut local.sockets) {
            self.publish_presence(meeting, team, user, false).await;
        }
    }

    /// Publishes a socket count transition; one that cannot be published is
    /// kept for the next heartbeat.
    async fn announce(
        &self,
        local: &mut Local,
        meeting_id: Uuid,
        team_id: Uuid,
        user_id: Uuid,
        online: bool,
    ) {
        if self
            .publish_presence(meeting_id, team_id, user_id, online)
            .await
        {
            local.pending.remove(&(meeting_id, user_id));
        } else {
            local.pending.insert((meeting_id, user_id), team_id);
        }
    }

    async fn publish_presence(
        &self,
        meeting_id: Uuid,
        team_id: Uuid,
        user_id: Uuid,
        online: bool,
    ) -> bool {
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
        match notify(&self.db, &notification).await {
            Ok(()) => true,
            Err(error) => {
                tracing::warn!(%error, "cannot publish presence");
                false
            }
        }
    }

    /// Re-announces this replica's sockets (answer to `presence.sync`).
    async fn announce_local(&self) {
        // Held throughout, so a socket leaving meanwhile announces after.
        let mut local = self.local.lock().await;
        let sockets: Vec<_> = local
            .sockets
            .iter()
            .map(|(&(meeting, user), &(team, _))| (meeting, team, user))
            .collect();
        for (meeting, team, user) in sockets {
            self.announce(&mut local, meeting, team, user, true).await;
        }
    }

    /// Publishes the announcements that failed, as the sockets now stand: a
    /// user who came back meanwhile is announced online, not offline.
    async fn retry_pending(&self) {
        let mut local = self.local.lock().await;
        let pending: Vec<_> = local.pending.iter().map(|(&k, &t)| (k, t)).collect();
        for ((meeting, user), team) in pending {
            let online = local.sockets.contains_key(&(meeting, user));
            self.announce(&mut local, meeting, team, user, online).await;
        }
    }

    /// Every heartbeat: tells the other replicas this one is alive, retries
    /// failed announcements and forgets the replicas that went silent.
    async fn beat(self: Arc<Self>) {
        let mut tick = tokio::time::interval(self.heartbeat);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tick.tick().await;
            let alive = Notification {
                channel: String::new(),
                kind: PRESENCE_ALIVE.into(),
                seq: None,
                payload: json!({ "replica": self.replica }),
            };
            if let Err(error) = notify(&self.db, &alive).await {
                tracing::warn!(%error, "cannot send the presence heartbeat");
            }
            self.retry_pending().await;
            // While our listener is down we hear no one: silence proves nothing
            // (the reconnect starts presence over anyway).
            if !*self.ready.borrow() {
                continue;
            }
            let gone = self.online.lock().expect("hub presence").expire(
                Instant::now(),
                self.heartbeat * MISSED_HEARTBEATS,
                self.replica,
            );
            for (meeting, team, user) in gone {
                self.send_presence(meeting, team, user, false);
            }
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

    /// After (re)connecting: start presence over from this replica's own
    /// sockets and ask the others for theirs, tell local clients they may have
    /// missed events, and have local sockets re-check their access (a removal
    /// may have been missed).
    async fn on_connected(&self) {
        let _ = self.access.send(AccessChange::Unknown);
        {
            let local = self.local.lock().await;
            self.online
                .lock()
                .expect("hub presence")
                .reset(self.replica, &local.sockets);
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
        self.request_presence().await;
        self.ready.send_replace(true);
    }

    /// Asks every replica (this one included) to re-announce its sockets.
    async fn request_presence(&self) {
        let request = Notification {
            channel: String::new(),
            kind: PRESENCE_SYNC.into(),
            seq: None,
            payload: json!({ "replica": self.replica }),
        };
        if let Err(error) = notify(&self.db, &request).await {
            tracing::warn!(%error, "cannot request presence");
        }
    }

    /// Notes that `replica` is alive. One that had been forgotten for its
    /// silence lost its presence here: everyone re-announces.
    async fn heard_from(&self, replica: Uuid) {
        let returned = self
            .online
            .lock()
            .expect("hub presence")
            .touch(replica, Instant::now());
        if returned {
            self.request_presence().await;
        }
    }

    async fn dispatch(&self, text: &str) {
        let Ok(notification) = serde_json::from_str::<Notification>(text) else {
            tracing::warn!("malformed real-time notification");
            return;
        };
        match notification.kind.as_str() {
            PRESENCE_SYNC => self.announce_local().await,
            PRESENCE_CHANGED => self.presence_changed(&notification).await,
            PRESENCE_ALIVE => {
                if let Some(replica) = uuid_field(&notification.payload, "replica") {
                    self.heard_from(replica).await;
                }
            }
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
    async fn presence_changed(&self, notification: &Notification) {
        let p = &notification.payload;
        let field = |name: &str| uuid_field(p, name);
        let (Some(meeting), Some(team), Some(user), Some(replica), Some(online)) = (
            field("meetingId"),
            field("teamId"),
            field("userId"),
            field("replica"),
            p.get("online").and_then(|v| v.as_bool()),
        ) else {
            return;
        };
        self.heard_from(replica).await;
        let changed = self
            .online
            .lock()
            .expect("hub presence")
            .apply(meeting, team, user, replica, online);
        if changed {
            self.send_presence(meeting, team, user, online);
        }
    }

    /// Tells local clients a user's overall presence in a meeting changed.
    fn send_presence(&self, meeting: Uuid, team: Uuid, user: Uuid, online: bool) {
        self.send(&Notification {
            channel: Channel::Meeting(meeting).to_string(),
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

/// A UUID field of a notification payload.
fn uuid_field(payload: &serde_json::Value, name: &str) -> Option<Uuid> {
    payload
        .get(name)
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<Uuid>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids<const N: usize>() -> [Uuid; N] {
        std::array::from_fn(|_| Uuid::now_v7())
    }

    #[test]
    fn a_user_is_online_while_any_replica_holds_a_socket() {
        let [meeting, team, user, a, b] = ids();
        let mut online = Online::default();
        assert!(online.apply(meeting, team, user, a, true));
        // Already online: a second replica changes nothing visible.
        assert!(!online.apply(meeting, team, user, b, true));
        assert!(!online.apply(meeting, team, user, a, true));
        assert!(!online.apply(meeting, team, user, a, false));
        assert_eq!(online.users(meeting), HashSet::from([user]));
        assert!(online.apply(meeting, team, user, b, false));
        assert!(online.users(meeting).is_empty());
        // Nothing left behind; a repeated "offline" is a no-op.
        assert!(online.meetings.is_empty());
        assert!(!online.apply(meeting, team, user, b, false));
        assert!(online.meetings.is_empty());
    }

    #[test]
    fn meetings_are_kept_apart() {
        let [one, two, team, user, replica] = ids();
        let mut online = Online::default();
        online.apply(one, team, user, replica, true);
        assert!(online.users(two).is_empty());
        assert!(online.apply(two, team, user, replica, true));
        assert!(online.apply(one, team, user, replica, false));
        assert_eq!(online.users(two), HashSet::from([user]));
    }

    #[test]
    fn reconnecting_starts_over_from_the_local_sockets() {
        let [meeting, team, mine, theirs, me, other, both, stale] = ids();
        let mut online = Online::default();
        online.apply(meeting, team, me, mine, true);
        online.apply(meeting, team, other, theirs, true);
        online.apply(meeting, team, both, mine, true);
        online.apply(meeting, team, both, theirs, true);
        // Its "offline" was missed while the listener was down.
        online.apply(meeting, team, stale, mine, true);
        let sockets = Sockets::from([((meeting, me), (team, 1)), ((meeting, both), (team, 2))]);
        online.reset(mine, &sockets);
        assert_eq!(online.users(meeting), HashSet::from([me, both]));
        // `both` now depends on this replica alone.
        assert!(online.apply(meeting, team, both, mine, false));
    }

    #[test]
    fn a_silent_replica_is_forgotten() {
        let [meeting, team, me, alive, dead, mine, theirs, shared] = ids();
        let silence = Duration::from_secs(45);
        let start = Instant::now();
        let mut online = Online::default();
        for replica in [me, alive, dead] {
            online.touch(replica, start);
        }
        online.apply(meeting, team, mine, me, true);
        online.apply(meeting, team, theirs, dead, true);
        online.apply(meeting, team, shared, dead, true);
        online.apply(meeting, team, shared, alive, true);
        // Not silent long enough yet.
        assert!(online.expire(start + silence, silence, me).is_empty());

        online.touch(alive, start + silence);
        let later = start + silence + Duration::from_secs(1);
        // `me` is never expired, however long since our own heartbeat.
        assert_eq!(
            online.expire(later, silence, me),
            vec![(meeting, team, theirs)]
        );
        assert_eq!(online.users(meeting), HashSet::from([mine, shared]));
        assert!(online.expire(later, silence, me).is_empty());
        // The forgotten replica speaking again is noticed (once).
        assert!(online.touch(dead, later));
        assert!(!online.touch(dead, later));
        assert!(!online.touch(alive, later));
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
