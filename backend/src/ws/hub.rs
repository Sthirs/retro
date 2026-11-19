//! The per-replica side of the hub: listens for notifications and fans them
//! out to local sockets; tracks presence across replicas (ADR-0011).

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::postgres::PgListener;
use tokio::sync::{broadcast, watch};
use utoipa::ToSchema;
use uuid::Uuid;

use super::{Channel, Notification, PG_CHANNEL, notify};
use crate::entities::reflection::ReflectionColumn;

/// Messages buffered per channel before a slow socket is told to resync.
const CHANNEL_CAPACITY: usize = 256;
const PRESENCE_CHANGED: &str = "presence.changed";
const PRESENCE_SYNC: &str = "presence.sync";
const PRESENCE_ALIVE: &str = "presence.alive";
/// Heartbeats a replica may miss before the others forget its presence.
const MISSED_HEARTBEATS: u32 = 3;
/// A replica's writing counts for a meeting (internal, ADR-0012).
const WRITING_CHANGED: &str = "writing.changed";
/// The cluster-wide writing counts, as clients get them.
const REFLECTION_WRITING: &str = "reflection.writing";
/// A replica's dragged cards in a meeting (internal, ADR-0012).
const DRAGGING_CHANGED: &str = "dragging.changed";
/// The cluster-wide dragged cards, as clients get them.
const CARD_DRAGGING: &str = "card.dragging";
/// A replica's task writers in a meeting (internal, ADR-0015).
const TASK_WRITING_CHANGED: &str = "task_writing.changed";
/// The cluster-wide task writers, as clients get them.
const TASK_WRITING: &str = "task.writing";
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

/// How a socket is writing a reflection (ADR-0012): a new one, or editing a
/// sent one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WritingMode {
    New = 0,
    Edit = 1,
}

/// Open reflection editors, by column ([`ReflectionColumn::index`]) and mode
/// (new, edit).
pub type WritingCounts = [[u32; 2]; 4];

const NO_WRITING: WritingCounts = [[0; 2]; 4];

/// Reflections being written in a column: new ones, and sent ones being
/// edited.
#[derive(Debug, Serialize, ToSchema)]
pub struct ColumnWritingDto {
    pub new: u32,
    pub edit: u32,
}

/// Reflections being written, per column, as clients see them (snapshot and
/// `reflection.writing`; spec §9.2).
#[derive(Debug, Serialize, ToSchema)]
pub struct WritingDto {
    pub plus: ColumnWritingDto,
    pub delta: ColumnWritingDto,
    pub ideas: ColumnWritingDto,
    pub thanks: ColumnWritingDto,
}

impl From<WritingCounts> for WritingDto {
    fn from(c: WritingCounts) -> Self {
        let column = |column: ReflectionColumn| {
            let [new, edit] = c[column.index()];
            ColumnWritingDto { new, edit }
        };
        Self {
            plus: column(ReflectionColumn::Plus),
            delta: column(ReflectionColumn::Delta),
            ideas: column(ReflectionColumn::Ideas),
            thanks: column(ReflectionColumn::Thanks),
        }
    }
}

/// Cards being dragged in a meeting: card → the user dragging it.
pub type Drags = BTreeMap<Uuid, Uuid>;

/// A card being dragged, and by whom (snapshot and `card.dragging`; spec
/// §9.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DragDto {
    pub reflection_id: Uuid,
    pub user_id: Uuid,
}

fn drag_list(drags: &Drags) -> Vec<DragDto> {
    drags
        .iter()
        .map(|(&reflection_id, &user_id)| DragDto {
            reflection_id,
            user_id,
        })
        .collect()
}

/// What a participant is doing on a task in Discuss (ADR-0015): writing a new
/// one for a group, or editing one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TaskTarget {
    New(Uuid),
    Edit(Uuid),
}

/// Someone writing a task, and what on (snapshot and `task.writing`; spec
/// §9.5): `groupId` for a new task, `taskId` for one being edited.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, ToSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct TaskWriterDto {
    pub user_id: Uuid,
    pub group_id: Option<Uuid>,
    pub task_id: Option<Uuid>,
}

impl TaskWriterDto {
    pub fn new(user_id: Uuid, target: TaskTarget) -> Self {
        let (group_id, task_id) = match target {
            TaskTarget::New(group) => (Some(group), None),
            TaskTarget::Edit(task) => (None, Some(task)),
        };
        Self {
            user_id,
            group_id,
            task_id,
        }
    }
}

/// Task writers in a meeting; a user writing the same thing in two tabs
/// counts once.
pub type TaskWriters = BTreeSet<TaskWriterDto>;

/// A message on its way to a channel's local subscribers: the serialized
/// event, and the only user who may receive it (ADR-0012).
#[derive(Debug, Clone)]
pub struct Outbound {
    pub user: Option<Uuid>,
    pub text: Arc<str>,
}

/// A per-meeting state each replica announces in full, absolute rather than
/// as a change, so a repeated or missed announcement is corrected by the
/// next (ADR-0012): reflections being written, cards being dragged. Every
/// replica folds the announcements into a cluster-wide total for clients.
trait ReplicaState: Sized {
    /// A replica's state in a meeting, and the cluster's total.
    type Value: Clone + PartialEq + std::fmt::Debug;
    /// What this replica tracks per meeting to make its [`Self::Value`].
    type Local: Default + std::fmt::Debug;
    /// The internal kind replicas announce their state with.
    const INTERNAL: &'static str;
    /// The kind clients get the cluster-wide total with.
    const CLIENT: &'static str;
    /// The field of the internal payload that holds the state.
    const FIELD: &'static str;
    /// What the state is, for logs.
    const WHAT: &'static str;

    fn empty() -> Self::Value;
    /// Adds up the replicas' states, in replica order.
    fn total<'a>(replicas: impl Iterator<Item = &'a Self::Value>) -> Self::Value
    where
        Self::Value: 'a;
    fn value(local: &Self::Local) -> Self::Value;
    fn encode(value: &Self::Value) -> serde_json::Value;
    fn decode(field: &serde_json::Value) -> Option<Self::Value>;
    /// The payload of the [`Self::CLIENT`] event.
    fn client_payload(value: &Self::Value) -> serde_json::Value;
    fn track(local: &mut Local) -> &mut Track<Self>;
    fn shared(online: &mut Online) -> &mut Shared<Self>;
}

/// Open reflection editors (spec §9.2).
#[derive(Debug, Default)]
struct Writing;

impl ReplicaState for Writing {
    type Value = WritingCounts;
    type Local = WritingCounts;
    const INTERNAL: &'static str = WRITING_CHANGED;
    const CLIENT: &'static str = REFLECTION_WRITING;
    const FIELD: &'static str = "counts";
    const WHAT: &'static str = "writing state";

    fn empty() -> WritingCounts {
        NO_WRITING
    }

    fn total<'a>(replicas: impl Iterator<Item = &'a WritingCounts>) -> WritingCounts {
        let mut total = NO_WRITING;
        for counts in replicas {
            for (column, modes) in counts.iter().enumerate() {
                for (mode, n) in modes.iter().enumerate() {
                    total[column][mode] += n;
                }
            }
        }
        total
    }

    fn value(local: &WritingCounts) -> WritingCounts {
        *local
    }

    fn encode(value: &WritingCounts) -> serde_json::Value {
        json!(value)
    }

    fn decode(field: &serde_json::Value) -> Option<WritingCounts> {
        serde_json::from_value(field.clone()).ok()
    }

    fn client_payload(value: &WritingCounts) -> serde_json::Value {
        json!({ "counts": WritingDto::from(*value) })
    }

    fn track(local: &mut Local) -> &mut Track<Self> {
        &mut local.writing
    }

    fn shared(online: &mut Online) -> &mut Shared<Self> {
        &mut online.writing
    }
}

/// Cards being dragged (spec §9.3).
#[derive(Debug, Default)]
struct Dragging;

impl ReplicaState for Dragging {
    type Value = Drags;
    /// One (card, user) entry per dragging socket.
    type Local = Vec<(Uuid, Uuid)>;
    const INTERNAL: &'static str = DRAGGING_CHANGED;
    const CLIENT: &'static str = CARD_DRAGGING;
    const FIELD: &'static str = "drags";
    const WHAT: &'static str = "drag state";

    fn empty() -> Drags {
        Drags::new()
    }

    /// Two replicas dragging one card (a race the soft lock does not
    /// prevent) show one dragger: the later replica's.
    fn total<'a>(replicas: impl Iterator<Item = &'a Drags>) -> Drags {
        let mut total = Drags::new();
        for drags in replicas {
            total.extend(drags.iter().map(|(&card, &user)| (card, user)));
        }
        total
    }

    fn value(local: &Vec<(Uuid, Uuid)>) -> Drags {
        local.iter().copied().collect()
    }

    fn encode(value: &Drags) -> serde_json::Value {
        json!(drag_list(value))
    }

    fn decode(field: &serde_json::Value) -> Option<Drags> {
        let drags: Vec<DragDto> = serde_json::from_value(field.clone()).ok()?;
        Some(
            drags
                .into_iter()
                .map(|d| (d.reflection_id, d.user_id))
                .collect(),
        )
    }

    fn client_payload(value: &Drags) -> serde_json::Value {
        json!({ "drags": drag_list(value) })
    }

    fn track(local: &mut Local) -> &mut Track<Self> {
        &mut local.drags
    }

    fn shared(online: &mut Online) -> &mut Shared<Self> {
        &mut online.drags
    }
}

/// Participants writing or editing tasks (spec §9.5).
#[derive(Debug, Default)]
struct TaskWriting;

impl ReplicaState for TaskWriting {
    type Value = TaskWriters;
    /// One entry per writing socket.
    type Local = Vec<TaskWriterDto>;
    const INTERNAL: &'static str = TASK_WRITING_CHANGED;
    const CLIENT: &'static str = TASK_WRITING;
    const FIELD: &'static str = "writers";
    const WHAT: &'static str = "task writing state";

    fn empty() -> TaskWriters {
        TaskWriters::new()
    }

    fn total<'a>(replicas: impl Iterator<Item = &'a TaskWriters>) -> TaskWriters {
        replicas.flatten().copied().collect()
    }

    fn value(local: &Vec<TaskWriterDto>) -> TaskWriters {
        local.iter().copied().collect()
    }

    fn encode(value: &TaskWriters) -> serde_json::Value {
        json!(value)
    }

    fn decode(field: &serde_json::Value) -> Option<TaskWriters> {
        serde_json::from_value(field.clone()).ok()
    }

    fn client_payload(value: &TaskWriters) -> serde_json::Value {
        json!({ "writers": value })
    }

    fn track(local: &mut Local) -> &mut Track<Self> {
        &mut local.task_writing
    }

    fn shared(online: &mut Online) -> &mut Shared<Self> {
        &mut online.task_writing
    }
}

/// A [`ReplicaState`] as this replica sees the cluster: meeting → replica
/// (ordered, so totals are stable) → that replica's state.
#[derive(Debug)]
struct Shared<S: ReplicaState> {
    meetings: HashMap<Uuid, BTreeMap<Uuid, S::Value>>,
}

impl<S: ReplicaState> Default for Shared<S> {
    fn default() -> Self {
        Self {
            meetings: HashMap::new(),
        }
    }
}

impl<S: ReplicaState> Shared<S> {
    /// The cluster-wide state of a meeting.
    fn total(&self, meeting: Uuid) -> S::Value {
        S::total(
            self.meetings
                .get(&meeting)
                .into_iter()
                .flat_map(|r| r.values()),
        )
    }

    /// Folds in a replica's state for a meeting; returns the new total when
    /// it changed.
    fn set(&mut self, meeting: Uuid, replica: Uuid, value: S::Value) -> Option<S::Value> {
        let before = self.total(meeting);
        let replicas = self.meetings.entry(meeting).or_default();
        if value == S::empty() {
            replicas.remove(&replica);
        } else {
            replicas.insert(replica, value);
        }
        if replicas.is_empty() {
            self.meetings.remove(&meeting);
        }
        let after = self.total(meeting);
        (before != after).then_some(after)
    }

    /// Forgets the `gone` replicas; returns the meetings whose total changed,
    /// with the new totals.
    fn forget(&mut self, gone: &HashSet<Uuid>) -> Vec<(Uuid, S::Value)> {
        let mut changed = Vec::new();
        let meetings: Vec<Uuid> = self.meetings.keys().copied().collect();
        for meeting in meetings {
            let before = self.total(meeting);
            if let Some(replicas) = self.meetings.get_mut(&meeting) {
                replicas.retain(|replica, _| !gone.contains(replica));
                if replicas.is_empty() {
                    self.meetings.remove(&meeting);
                }
            }
            let after = self.total(meeting);
            if after != before {
                changed.push((meeting, after));
            }
        }
        changed
    }
}

/// This replica's side of a [`ReplicaState`].
#[derive(Debug)]
struct Track<S: ReplicaState> {
    /// What this replica's sockets do, per meeting.
    meetings: HashMap<Uuid, S::Local>,
    /// Meetings whose state could not be published.
    pending: HashSet<Uuid>,
}

impl<S: ReplicaState> Default for Track<S> {
    fn default() -> Self {
        Self {
            meetings: HashMap::new(),
            pending: HashSet::new(),
        }
    }
}

impl<S: ReplicaState> Track<S> {
    /// This replica's state in a meeting.
    fn value(&self, meeting: Uuid) -> S::Value {
        self.meetings.get(&meeting).map_or_else(S::empty, S::value)
    }

    /// Folds this replica's own states into `shared` (as replica `me`).
    fn share(&self, me: Uuid, shared: &mut Shared<S>) {
        for (&meeting, local) in &self.meetings {
            shared.set(meeting, me, S::value(local));
        }
    }
}

/// What forgetting silent replicas changed.
#[derive(Debug, Default, PartialEq)]
struct Expired {
    /// Users now offline.
    offline: Vec<Change>,
    /// Meetings whose writing totals changed, with the new totals.
    writing: Vec<(Uuid, WritingCounts)>,
    /// Meetings whose dragged cards changed, with the cards now dragged.
    drags: Vec<(Uuid, Drags)>,
    /// Meetings whose task writers changed, with the writers now.
    task_writing: Vec<(Uuid, TaskWriters)>,
}

/// Who is online in which meeting, as this replica sees the cluster:
/// meeting → user → their team and the replicas holding a socket for them;
/// plus when each replica was last heard from, so one that stops without
/// saying goodbye (killed, partitioned) is eventually forgotten.
#[derive(Debug, Default)]
struct Online {
    meetings: HashMap<Uuid, HashMap<Uuid, Presence>>,
    writing: Shared<Writing>,
    drags: Shared<Dragging>,
    task_writing: Shared<TaskWriting>,
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
    /// and returns the users that leaves offline and the meetings whose
    /// writing or dragged cards changed.
    fn expire(&mut self, now: Instant, silence: Duration, me: Uuid) -> Expired {
        let gone: HashSet<Uuid> = self
            .seen
            .iter()
            .filter(|&(&replica, &at)| replica != me && now.duration_since(at) > silence)
            .map(|(&replica, _)| replica)
            .collect();
        if gone.is_empty() {
            return Expired::default();
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
        let writing = self.writing.forget(&gone);
        let drags = self.drags.forget(&gone);
        let task_writing = self.task_writing.forget(&gone);
        Expired {
            offline,
            writing,
            drags,
            task_writing,
        }
    }

    /// Starts over from this replica's own sockets (the others are asked
    /// again): after a listener reconnect even our own announcements may have
    /// been missed.
    fn reset(&mut self, me: Uuid, local: &Local) {
        *self = Self::default();
        for (&(meeting, user), &(team, _)) in &local.sockets {
            self.apply(meeting, team, user, me, true);
        }
        local.writing.share(me, &mut self.writing);
        local.drags.share(me, &mut self.drags);
        local.task_writing.share(me, &mut self.task_writing);
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
    /// This replica's writing counts per meeting (ADR-0012).
    writing: Track<Writing>,
    /// This replica's dragged cards per meeting (ADR-0012).
    drags: Track<Dragging>,
    /// This replica's task writers per meeting (ADR-0015).
    task_writing: Track<TaskWriting>,
    /// Shutting down: this replica has said goodbye; sockets no longer count.
    closing: bool,
}

pub struct Hub {
    replica: Uuid,
    db: DatabaseConnection,
    /// How often this replica tells the others it is alive; a replica silent
    /// for [`MISSED_HEARTBEATS`] of them is forgotten.
    heartbeat: Duration,
    channels: Mutex<HashMap<String, broadcast::Sender<Outbound>>>,
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
    pub fn subscribe(&self, channel: &str) -> broadcast::Receiver<Outbound> {
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

    /// The reflections being written in a meeting, on any replica.
    pub fn writing(&self, meeting_id: Uuid) -> WritingCounts {
        self.online
            .lock()
            .expect("hub presence")
            .writing
            .total(meeting_id)
    }

    /// The cards being dragged in a meeting, on any replica.
    pub fn drags(&self, meeting_id: Uuid) -> Vec<DragDto> {
        drag_list(
            &self
                .online
                .lock()
                .expect("hub presence")
                .drags
                .total(meeting_id),
        )
    }

    /// Who is writing or editing a task in a meeting, on any replica.
    pub fn task_writers(&self, meeting_id: Uuid) -> Vec<TaskWriterDto> {
        self.online
            .lock()
            .expect("hub presence")
            .task_writing
            .total(meeting_id)
            .into_iter()
            .collect()
    }

    /// A socket of `user` went from writing `from` to writing `to` on a
    /// task in a meeting (`None`: nothing).
    pub async fn set_task_writing(
        &self,
        meeting_id: Uuid,
        user: Uuid,
        from: Option<TaskTarget>,
        to: Option<TaskTarget>,
    ) {
        if from == to {
            return;
        }
        let mut local = self.local.lock().await;
        if local.closing {
            return;
        }
        let writers = local.task_writing.meetings.entry(meeting_id).or_default();
        if let Some(target) = from
            && let Some(i) = writers
                .iter()
                .position(|&w| w == TaskWriterDto::new(user, target))
        {
            writers.swap_remove(i);
        }
        if let Some(target) = to {
            writers.push(TaskWriterDto::new(user, target));
        }
        self.announce_state::<TaskWriting>(&mut local, meeting_id)
            .await;
    }

    /// A socket of `user` stopped dragging card `from` and/or started
    /// dragging card `to` in a meeting (`None`: no card).
    pub async fn set_dragging(
        &self,
        meeting_id: Uuid,
        user: Uuid,
        from: Option<Uuid>,
        to: Option<Uuid>,
    ) {
        if from == to {
            return;
        }
        let mut local = self.local.lock().await;
        if local.closing {
            return;
        }
        let draggers = local.drags.meetings.entry(meeting_id).or_default();
        if let Some(card) = from
            && let Some(i) = draggers.iter().position(|&d| d == (card, user))
        {
            draggers.swap_remove(i);
        }
        if let Some(card) = to {
            draggers.push((card, user));
        }
        self.announce_state::<Dragging>(&mut local, meeting_id)
            .await;
    }

    /// A socket's writing state in a meeting went from `from` to `to`
    /// (`None`: not writing).
    pub async fn set_writing(
        &self,
        meeting_id: Uuid,
        from: Option<(ReflectionColumn, WritingMode)>,
        to: Option<(ReflectionColumn, WritingMode)>,
    ) {
        if from == to {
            return;
        }
        let mut local = self.local.lock().await;
        if local.closing {
            return;
        }
        let counts = local.writing.meetings.entry(meeting_id).or_default();
        if let Some((column, mode)) = from {
            let n = &mut counts[column.index()][mode as usize];
            *n = n.saturating_sub(1);
        }
        if let Some((column, mode)) = to {
            counts[column.index()][mode as usize] += 1;
        }
        self.announce_state::<Writing>(&mut local, meeting_id).await;
    }

    /// Publishes this replica's state in a meeting; one that cannot be
    /// published is retried on the next heartbeat.
    async fn announce_state<S: ReplicaState>(&self, local: &mut Local, meeting_id: Uuid) {
        let track = S::track(local);
        let value = track.value(meeting_id);
        if value == S::empty() {
            track.meetings.remove(&meeting_id);
        }
        let published = self.publish_state::<S>(meeting_id, &value).await;
        let pending = &mut S::track(local).pending;
        if published {
            pending.remove(&meeting_id);
        } else {
            pending.insert(meeting_id);
        }
    }

    async fn publish_state<S: ReplicaState>(&self, meeting_id: Uuid, value: &S::Value) -> bool {
        let mut payload = json!({ "meetingId": meeting_id, "replica": self.replica });
        payload[S::FIELD] = S::encode(value);
        let notification = Notification {
            channel: Channel::Meeting(meeting_id).to_string(),
            kind: S::INTERNAL.into(),
            seq: None,
            payload,
            user: None,
        };
        match notify(&self.db, &notification).await {
            Ok(()) => true,
            Err(error) => {
                tracing::warn!(%error, "cannot publish {}", S::WHAT);
                false
            }
        }
    }

    /// Re-announces this replica's state in every meeting it has one in.
    async fn announce_all<S: ReplicaState>(&self, local: &mut Local) {
        let meetings: Vec<Uuid> = S::track(local).meetings.keys().copied().collect();
        for meeting in meetings {
            self.announce_state::<S>(local, meeting).await;
        }
    }

    /// Publishes the states that failed, as they now stand.
    async fn retry_state<S: ReplicaState>(&self, local: &mut Local) {
        let pending: Vec<Uuid> = S::track(local).pending.iter().copied().collect();
        for meeting in pending {
            self.announce_state::<S>(local, meeting).await;
        }
    }

    /// Withdraws this replica's state from every meeting (shutting down).
    async fn withdraw<S: ReplicaState>(&self, local: &mut Local) {
        let track = S::track(local);
        track.pending.clear();
        for meeting in std::mem::take(&mut track.meetings).into_keys() {
            self.publish_state::<S>(meeting, &S::empty()).await;
        }
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
        self.withdraw::<Writing>(&mut local).await;
        self.withdraw::<Dragging>(&mut local).await;
        self.withdraw::<TaskWriting>(&mut local).await;
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
            user: None,
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
        self.announce_all::<Writing>(&mut local).await;
        self.announce_all::<Dragging>(&mut local).await;
        self.announce_all::<TaskWriting>(&mut local).await;
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
        self.retry_state::<Writing>(&mut local).await;
        self.retry_state::<Dragging>(&mut local).await;
        self.retry_state::<TaskWriting>(&mut local).await;
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
                user: None,
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
            // Announced under the lock, like every change of the cluster
            // view: a later change cannot be announced before this one.
            let mut online = self.online.lock().expect("hub presence");
            let expired = online.expire(
                Instant::now(),
                self.heartbeat * MISSED_HEARTBEATS,
                self.replica,
            );
            for (meeting, team, user) in expired.offline {
                self.send_presence(meeting, team, user, false);
            }
            for (meeting, counts) in expired.writing {
                self.send_state::<Writing>(meeting, &counts);
            }
            for (meeting, drags) in expired.drags {
                self.send_state::<Dragging>(meeting, &drags);
            }
            for (meeting, writers) in expired.task_writing {
                self.send_state::<TaskWriting>(meeting, &writers);
            }
            drop(online);
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
                .reset(self.replica, &local);
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
                user: None,
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
            user: None,
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
            WRITING_CHANGED => self.state_changed::<Writing>(&notification).await,
            DRAGGING_CHANGED => self.state_changed::<Dragging>(&notification).await,
            TASK_WRITING_CHANGED => self.state_changed::<TaskWriting>(&notification).await,
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
        // Announced under the lock: changes reach clients in the order they
        // were folded in (the heartbeat's expiry included).
        let mut view = self.online.lock().expect("hub presence");
        if view.apply(meeting, team, user, replica, online) {
            self.send_presence(meeting, team, user, online);
        }
    }

    /// Folds a replica's state into the cluster view, and tells clients when
    /// the meeting's total changed.
    async fn state_changed<S: ReplicaState>(&self, notification: &Notification) {
        let p = &notification.payload;
        let (Some(meeting), Some(replica), Some(value)) = (
            uuid_field(p, "meetingId"),
            uuid_field(p, "replica"),
            p.get(S::FIELD).and_then(S::decode),
        ) else {
            return;
        };
        self.heard_from(replica).await;
        // Announced under the lock: a total cannot overtake a newer one (the
        // heartbeat's expiry included).
        let mut view = self.online.lock().expect("hub presence");
        if let Some(total) = S::shared(&mut view).set(meeting, replica, value) {
            self.send_state::<S>(meeting, &total);
        }
    }

    /// Tells local clients a meeting's cluster-wide state.
    fn send_state<S: ReplicaState>(&self, meeting: Uuid, value: &S::Value) {
        self.send(&Notification {
            channel: Channel::Meeting(meeting).to_string(),
            kind: S::CLIENT.into(),
            seq: None,
            payload: S::client_payload(value),
            user: None,
        });
    }

    /// Tells local clients a user's overall presence in a meeting changed.
    fn send_presence(&self, meeting: Uuid, team: Uuid, user: Uuid, online: bool) {
        self.send(&Notification {
            channel: Channel::Meeting(meeting).to_string(),
            kind: PRESENCE_CHANGED.into(),
            seq: None,
            payload: json!({ "userId": user, "online": online }),
            user: None,
        });
        // Meetings pages patch the card's presence: no list reload.
        self.send(&Notification {
            channel: Channel::Team(team).to_string(),
            kind: PRESENCE_CHANGED.into(),
            seq: None,
            payload: json!({ "meetingId": meeting, "userId": user, "online": online }),
            user: None,
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
        // The target user is for this hub only: clients never see it.
        let mut message = notification.clone();
        let user = message.user.take();
        if let Ok(text) = serde_json::to_string(&message) {
            let _ = sender.send(Outbound {
                user,
                text: Arc::from(text),
            });
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
        let local = Local {
            sockets: Sockets::from([((meeting, me), (team, 1)), ((meeting, both), (team, 2))]),
            ..Local::default()
        };
        online.reset(mine, &local);
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
        assert_eq!(
            online.expire(start + silence, silence, me),
            Expired::default()
        );

        online.touch(alive, start + silence);
        let later = start + silence + Duration::from_secs(1);
        // `me` is never expired, however long since our own heartbeat.
        assert_eq!(
            online.expire(later, silence, me).offline,
            vec![(meeting, team, theirs)]
        );
        assert_eq!(online.users(meeting), HashSet::from([mine, shared]));
        assert_eq!(online.expire(later, silence, me), Expired::default());
        // The forgotten replica speaking again is noticed (once).
        assert!(online.touch(dead, later));
        assert!(!online.touch(dead, later));
        assert!(!online.touch(alive, later));
    }

    fn counts(column: ReflectionColumn, mode: WritingMode, n: u32) -> WritingCounts {
        let mut counts = NO_WRITING;
        counts[column.index()][mode as usize] = n;
        counts
    }

    #[test]
    fn task_writers_merge_across_replicas_once_per_user_and_target() {
        let [meeting, a, b] = ids();
        let (user, group, task) = (Uuid::now_v7(), Uuid::now_v7(), Uuid::now_v7());
        let new = TaskWriterDto::new(user, TaskTarget::New(group));
        let edit = TaskWriterDto::new(user, TaskTarget::Edit(task));
        assert_eq!(new.group_id, Some(group));
        assert_eq!(edit.task_id, Some(task));
        let mut online = Online::default();
        let both = TaskWriting::value(&vec![new, edit, edit]);
        assert_eq!(both.len(), 2);
        assert_eq!(
            online.task_writing.set(meeting, a, both.clone()),
            Some(both.clone())
        );
        // The same writer on another replica (another tab) shows once.
        let only_edit: TaskWriters = [edit].into();
        assert_eq!(online.task_writing.set(meeting, b, only_edit.clone()), None);
        assert_eq!(
            online.task_writing.set(meeting, a, TaskWriters::new()),
            Some(only_edit)
        );
        let gone: HashSet<Uuid> = [b].into();
        assert_eq!(
            online.task_writing.forget(&gone),
            vec![(meeting, TaskWriters::new())]
        );
        assert!(online.task_writing.meetings.is_empty());
    }

    #[test]
    fn writing_counts_add_up_across_replicas() {
        let [meeting, a, b] = ids();
        let mut online = Online::default();
        let delta = counts(ReflectionColumn::Delta, WritingMode::New, 1);
        assert_eq!(online.writing.set(meeting, a, delta), Some(delta));
        let total = online
            .writing
            .set(
                meeting,
                b,
                counts(ReflectionColumn::Delta, WritingMode::New, 2),
            )
            .unwrap();
        assert_eq!(total[ReflectionColumn::Delta.index()], [3, 0]);
        // Absolute counts: repeating an announcement changes nothing.
        assert_eq!(online.writing.set(meeting, a, delta), None);
        online.writing.set(meeting, a, NO_WRITING);
        online.writing.set(meeting, b, NO_WRITING);
        assert_eq!(online.writing.total(meeting), NO_WRITING);
        assert!(online.writing.meetings.is_empty());
    }

    #[test]
    fn a_silent_replica_stops_writing() {
        let [meeting, me, dead] = ids();
        let silence = Duration::from_secs(45);
        let start = Instant::now();
        let mut online = Online::default();
        online.touch(me, start);
        online.touch(dead, start);
        online.writing.set(
            meeting,
            me,
            counts(ReflectionColumn::Plus, WritingMode::Edit, 1),
        );
        online.writing.set(
            meeting,
            dead,
            counts(ReflectionColumn::Plus, WritingMode::New, 1),
        );
        let expired = online.expire(start + silence * 2, silence, me);
        assert_eq!(
            expired.writing,
            vec![(
                meeting,
                counts(ReflectionColumn::Plus, WritingMode::Edit, 1)
            )]
        );
    }

    #[test]
    fn reconnecting_keeps_only_our_own_writers() {
        let [meeting, other, me, them] = ids();
        let mut online = Online::default();
        online.writing.set(
            meeting,
            them,
            counts(ReflectionColumn::Ideas, WritingMode::New, 1),
        );
        let ours = counts(ReflectionColumn::Thanks, WritingMode::New, 2);
        let local = Local {
            writing: Track {
                meetings: HashMap::from([(other, ours)]),
                ..Track::default()
            },
            ..Local::default()
        };
        online.reset(me, &local);
        assert_eq!(online.writing.total(meeting), NO_WRITING);
        assert_eq!(online.writing.total(other), ours);
    }

    #[test]
    fn writing_serializes_every_column_by_name() {
        let json = serde_json::to_value(WritingDto::from(counts(
            ReflectionColumn::Delta,
            WritingMode::Edit,
            1,
        )))
        .unwrap();
        assert_eq!(json["delta"], json!({ "new": 0, "edit": 1 }));
        assert_eq!(json["plus"], json!({ "new": 0, "edit": 0 }));
        assert_eq!(json.as_object().unwrap().len(), 4);
    }

    #[test]
    fn dragged_cards_add_up_across_replicas() {
        let [meeting, a, b, one, two, kai, jade] = ids();
        let mut online = Online::default();
        let first = Drags::from([(one, kai)]);
        assert_eq!(
            online.drags.set(meeting, a, first.clone()),
            Some(first.clone())
        );
        let both = Drags::from([(one, kai), (two, jade)]);
        assert_eq!(
            online.drags.set(meeting, b, Drags::from([(two, jade)])),
            Some(both.clone())
        );
        // Absolute: repeating an announcement changes nothing.
        assert_eq!(online.drags.set(meeting, a, first), None);
        assert_eq!(drag_list(&online.drags.total(meeting)).len(), 2);
        online.drags.set(meeting, a, Drags::new());
        online.drags.set(meeting, b, Drags::new());
        assert!(online.drags.total(meeting).is_empty());
        assert!(online.drags.meetings.is_empty());
    }

    #[test]
    fn a_silent_replica_stops_dragging() {
        let [meeting, me, dead, mine, theirs, kai, jade] = ids();
        let silence = Duration::from_secs(45);
        let start = Instant::now();
        let mut online = Online::default();
        online.touch(me, start);
        online.touch(dead, start);
        online.drags.set(meeting, me, Drags::from([(mine, kai)]));
        online
            .drags
            .set(meeting, dead, Drags::from([(theirs, jade)]));
        let expired = online.expire(start + silence * 2, silence, me);
        assert_eq!(expired.drags, vec![(meeting, Drags::from([(mine, kai)]))]);
    }

    #[test]
    fn reconnecting_keeps_only_our_own_drags() {
        let [meeting, me, them, mine, theirs, kai, jade] = ids();
        let mut online = Online::default();
        online
            .drags
            .set(meeting, them, Drags::from([(theirs, jade)]));
        let local = Local {
            drags: Track {
                meetings: HashMap::from([(meeting, vec![(mine, kai)])]),
                ..Track::default()
            },
            ..Local::default()
        };
        online.reset(me, &local);
        assert_eq!(online.drags.total(meeting), Drags::from([(mine, kai)]));
    }

    #[test]
    fn drags_serialize_in_camel_case() {
        let [card, user] = ids();
        assert_eq!(
            serde_json::to_value(drag_list(&Drags::from([(card, user)]))).unwrap(),
            json!([{ "reflectionId": card, "userId": user }])
        );
    }

    fn event(channel: &str, kind: &str, payload: serde_json::Value) -> Notification {
        Notification {
            channel: channel.to_owned(),
            kind: kind.to_owned(),
            seq: Some(1),
            payload,
            user: None,
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
