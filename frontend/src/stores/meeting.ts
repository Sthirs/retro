import { defineStore } from 'pinia';
import { computed, ref, toRaw, watch } from 'vue';

import {
  api,
  ApiError,
  apiError,
  errorCode,
  MeetingPhase,
  MeetingStatus,
  Role,
  ReflectionColumn,
  type GroupDto,
  type MeetingStateDto,
  type DiscussGroupDto,
  type MyVoteDto,
  type ParticipantDto,
  type ReactionAnswerDto,
  type ReactionDto,
  type ReflectionDto,
  type Timer,
  type TimerAction,
} from '@/api/client';
import { isOwnEvent } from '@/api/clientId';
import type { RealtimeEvent } from '@/api/realtime';
import { useSessionStore } from '@/stores/session';

export const PHASES = Object.values(MeetingPhase);
/**
 * How long a vote or reaction request may take before it counts as lost (its
 * answer may still have committed: a re-read tells). Later ones wait for it.
 */
export const VOTE_TIMEOUT_MS = 15_000;
/** Task events closer together than this are re-read once. */
export const TASK_RESYNC_DELAY_MS = 250;
const COLUMNS = Object.values(ReflectionColumn);

/** A card whose group changed, as `group.*` events carry it (spec §11.2). */
interface CardMove {
  id: string;
  groupId: string | null;
  version: number;
}

/** What a Group column shows, in order: plain cards and groups (spec §9.3). */
export type ColumnItem =
  | { kind: 'card'; card: ReflectionDto }
  | { kind: 'group'; group: GroupDto; cards: ReflectionDto[] };

/** Cards of a group, in a stable order: by column, then place. */
function byColumnAndPlace(a: ReflectionDto, b: ReflectionDto): number {
  return COLUMNS.indexOf(a.column) - COLUMNS.indexOf(b.column) || a.position - b.position;
}

/**
 * Takes `card` out of its group locally, dissolving the group if a single
 * card is left (an optimistic copy of the server's rule).
 */
function leaveGroup(m: MeetingStateDto, card: ReflectionDto): void {
  const old = card.groupId;
  card.groupId = null;
  if (!old) return;
  const left = m.reflections.filter((r) => r.groupId === old);
  if (left.length > 1) return;
  for (const r of left) r.groupId = null;
  m.groups = m.groups.filter((g) => g.id !== old);
}

/** One vote change this tab sent and the server has not answered yet. */
interface PendingVote {
  groupId: string;
  delta: 1 | -1;
}

/** Adds `delta` to the viewer's own votes on a group (an optimistic copy). */
function bumpVote(m: MeetingStateDto, groupId: string, delta: number): void {
  const mine = m.myVotes.find((v) => v.groupId === groupId);
  if (mine) mine.count += delta;
  else if (delta > 0) m.myVotes.push({ groupId, count: delta });
  m.myVotes = m.myVotes.filter((v) => v.count > 0);
}

/** The key of one emoji on one card. */
function reactionKey(reflectionId: string, emoji: string): string {
  return `${reflectionId} ${emoji}`;
}

/**
 * Toggles of one emoji on one card that the server has not answered yet.
 * They go out one at a time, and are shown on top of the confirmed state
 * until answered, so quick clicks neither flicker nor overtake each other.
 */
interface PendingReaction {
  reflectionId: string;
  emoji: string;
  /** Oldest first, the first one on its way: `true` puts the emoji on. */
  toggles: boolean[];
  /** The count, and whether it is the viewer's, as the server last surely had them. */
  count: number;
  mine: boolean;
  /**
   * The meeting event `seq` `count` is valid at. Events and answers carry
   * one: whichever is higher is newer, in whatever order they arrive.
   */
  seq: number;
  /** The last toggle's request: the next one waits for it. */
  chain: Promise<boolean>;
}

/**
 * Sets the count of one emoji on a card, and whether it is the viewer's;
 * `mine` left out keeps it. A count of zero removes it.
 */
function setReaction(
  m: MeetingStateDto,
  reflectionId: string,
  emoji: string,
  count: number,
  mine?: boolean,
): void {
  const existing = m.reactions.find((r) => r.reflectionId === reflectionId && r.emoji === emoji);
  if (count <= 0) {
    m.reactions = m.reactions.filter((r) => r !== existing);
  } else if (existing) {
    existing.count = count;
    if (mine !== undefined) existing.mine = mine;
  } else {
    m.reactions.push({ reflectionId, emoji, count, mine: mine ?? false });
  }
}

/** A notice in the room: a new facilitator, or this tab's drafts discarded. */
export type MeetingNotice =
  | { kind: 'facilitator'; userId: string; name: string }
  | { kind: 'draftsDiscarded'; count: number };

/**
 * The meeting room's state (spec §15.4): a snapshot from the server, kept up
 * to date by applying the meeting channel's events in `seq` order. A gap in
 * the sequence, or a reconnect, reloads the snapshot.
 */
/** Prefix of a group shown before the server has created it. */
const PENDING_GROUP = 'pending-';

/** Whether a group is a placeholder the server has not created yet. */
export function isPendingGroup(id: string): boolean {
  return id.startsWith(PENDING_GROUP);
}

export const useMeetingStore = defineStore('meeting', () => {
  const session = useSessionStore();
  const meeting = ref<MeetingStateDto | null>(null);
  const meetingId = ref<string | null>(null);
  const loading = ref(false);
  /** Why the meeting could not be opened (an error code). */
  const loadError = ref<string | null>(null);
  /** Joining said the meeting has ended: its state is not loaded. */
  const ended = ref(false);
  /** The last refused action (an error code). */
  const actionError = ref<string | null>(null);
  const notice = ref<MeetingNotice | null>(null);
  /** Events that arrived before the snapshot. */
  let queued: RealtimeEvent[] = [];
  /**
   * Events applied while a snapshot request (a reload or an action) is on its
   * way, one log per request: the snapshot may predate them, so they are
   * applied again on top of it.
   */
  const logs = new Set<RealtimeEvent[]>();
  /** Set while a log is applied again, so other logs do not record it twice. */
  let replaying = false;
  /** A replayed event changed what the snapshot cannot show (tasks): reload. */
  let replayWantsResync = false;
  /**
   * Who is in the room, participant or not yet: presence can arrive before
   * the participant's join, so it is kept apart and applied to participants.
   */
  let online = new Set<string>();

  function markOnline(): void {
    for (const p of meeting.value?.participants ?? []) p.online = online.has(p.id);
  }
  let loadSeq = 0;
  /** A resync asked for while `open()` was loading: run once it is done. */
  let resyncWanted = false;
  /** Optimistic actions waiting for the server's answer. */
  let optimisticInFlight = 0;
  /**
   * Votes not answered yet (queued or sent), oldest first. They go out one
   * at a time, and are shown on top of the confirmed votes until answered,
   * so quick clicks neither flicker nor overtake each other.
   */
  let pendingVotes: PendingVote[] = [];
  let voteChain: Promise<unknown> = Promise.resolve();
  /** Whether a vote request is on its way (at most one, see `voteChain`). */
  let voteSending = false;
  /**
   * Bumped whenever a vote goes out or comes back. A snapshot requested
   * before a bump may or may not hold that vote, so its votes are not taken.
   */
  let voteEpoch = 0;
  /**
   * The viewer's votes, and the facilitator's cast count, as the server last
   * surely had them; what the room shows is these plus `pendingVotes`.
   */
  let confirmedVotes: MyVoteDto[] = [];
  let confirmedCast: number | null = null;
  /** A snapshot's votes were unsure: reload once the votes are answered. */
  let voteResyncWanted = false;
  /**
   * The phase changed and its snapshot has not come yet: what is shown is
   * still the previous phase's (e.g. loose cards at Vote start, or no
   * reflections at Group start), so the room waits for it.
   */
  const phaseLoading = ref(false);
  /** Bumped on every reset (e.g. another account): stale work stops. */
  let generation = 0;
  /**
   * Server time minus local time, in ms, from the last `serverNow` seen: the
   * timer counts down on server time, whatever the local clock says.
   */
  const clockOffset = ref(0);
  /**
   * The group this viewer previews in Discuss, or `null` while following the
   * current one (spec §9.5). Local: nobody else sees it.
   */
  const viewedGroupId = ref<string | null>(null);
  /** Reaction toggles not answered yet, by key. */
  const pendingReactions = new Map<string, PendingReaction>();
  /** A re-read for task events, waiting for the burst to end. */
  let taskResyncTimer: ReturnType<typeof setTimeout> | undefined;
  /**
   * Unsent task text per group id (Discuss): kept while the room is open, so
   * a move to another group does not lose it.
   */
  const taskDrafts = ref<Record<string, string>>({});

  function syncClock(serverNow: unknown): void {
    if (typeof serverNow !== 'string') return;
    const at = Date.parse(serverNow);
    if (!Number.isNaN(at)) clockOffset.value = at - Date.now();
  }

  /** The current server time, in ms since the epoch. */
  function serverTime(): number {
    return Date.now() + clockOffset.value;
  }

  const me = computed(() => session.user?.id ?? null);
  const isFacilitator = computed(() => !!me.value && meeting.value?.facilitatorId === me.value);
  const myRole = computed(
    () => meeting.value?.participants.find((p) => p.id === me.value)?.role ?? null,
  );
  const isActive = computed(() => meeting.value?.status === MeetingStatus.active);
  const canTakeOver = computed(
    () => isActive.value && !isFacilitator.value && myRole.value === Role.admin,
  );
  const participantsById = computed(
    () => new Map((meeting.value?.participants ?? []).map((p) => [p.id, p])),
  );
  const facilitator = computed(() =>
    meeting.value ? participantsById.value.get(meeting.value.facilitatorId) : undefined,
  );
  const phaseIndex = computed(() => (meeting.value ? PHASES.indexOf(meeting.value.phase) : 0));
  const nextPhase = computed(() => PHASES[phaseIndex.value + 1] ?? null);

  /** The icebreaker queue as participants, in speaking order. */
  const speakers = computed<ParticipantDto[]>(() => {
    const m = meeting.value;
    if (!m) return [];
    return m.icebreaker.order
      .map((id) => participantsById.value.get(id))
      .filter((p): p is ParticipantDto => !!p);
  });
  const speakerIndex = computed(() => meeting.value?.icebreaker.index ?? 0);
  const currentSpeaker = computed(() => speakers.value[speakerIndex.value] ?? null);
  const nextSpeaker = computed(() => speakers.value[speakerIndex.value + 1] ?? null);
  const previousSpeaker = computed(() => speakers.value[speakerIndex.value - 1] ?? null);
  const isLastSpeaker = computed(() => speakerIndex.value >= speakers.value.length - 1);

  const participants = computed(() => meeting.value?.participants ?? []);
  const readyCount = computed(() => participants.value.filter((p) => p.ready).length);
  const iAmReady = computed(
    () => participants.value.find((p) => p.id === me.value)?.ready ?? false,
  );
  /** Ready is pressed in Reflect, Group and Vote (spec §8.2). */
  const hasReady = computed(() => {
    const phase = meeting.value?.phase;
    return (
      phase === MeetingPhase.reflect || phase === MeetingPhase.group || phase === MeetingPhase.vote
    );
  });
  const myReflections = computed(() => (meeting.value?.reflections ?? []).filter((r) => r.mine));

  /** Cards per group id, in a stable order. */
  const groupCards = computed(() => {
    const cards = new Map<string, ReflectionDto[]>();
    for (const r of meeting.value?.reflections ?? []) {
      if (!r.groupId) continue;
      const list = cards.get(r.groupId) ?? [];
      list.push(r);
      cards.set(r.groupId, list);
    }
    for (const list of cards.values()) list.sort(byColumnAndPlace);
    return cards;
  });

  /**
   * What each Group column shows: its ungrouped cards and the groups placed
   * in it, by place (a group before a card sharing its place).
   */
  const columnItems = computed(() => {
    const items = new Map<ReflectionColumn, ColumnItem[]>(COLUMNS.map((c) => [c, []]));
    const m = meeting.value;
    if (!m) return items;
    const order = new Map<ColumnItem, [number, number]>();
    for (const card of m.reflections) {
      if (card.groupId) continue;
      const item: ColumnItem = { kind: 'card', card };
      order.set(item, [card.position, 1]);
      items.get(card.column)!.push(item);
    }
    for (const group of m.groups) {
      const cards = groupCards.value.get(group.id) ?? [];
      if (cards.length === 0) continue;
      const item: ColumnItem = { kind: 'group', group, cards };
      order.set(item, [group.position, 0]);
      items.get(group.column)!.push(item);
    }
    for (const list of items.values()) {
      list.sort((a, b) => {
        const [pa, ka] = order.get(a)!;
        const [pb, kb] = order.get(b)!;
        return pa - pb || ka - kb;
      });
    }
    return items;
  });

  /** The viewer's own votes per group id (spec §9.4): nobody sees others'. */
  const myVotes = computed(
    () => new Map((meeting.value?.myVotes ?? []).map((v) => [v.groupId, v.count])),
  );
  const voteBudget = computed(() => meeting.value?.voteBudget ?? 0);
  const voteMaxPerGroup = computed(() => meeting.value?.voteMaxPerGroup ?? 0);
  const votesUsed = computed(() => {
    let used = 0;
    for (const count of myVotes.value.values()) used += count;
    return used;
  });
  const votesLeft = computed(() => Math.max(0, voteBudget.value - votesUsed.value));
  /** Votes cast overall: the facilitator's view during Vote, else `null`. */
  const votesCast = computed(() => meeting.value?.votesCast ?? null);

  /** The groups in Discuss order, with their vote totals (spec §9.5). */
  const discussOrder = computed<DiscussGroupDto[]>(() => meeting.value?.discussOrder ?? []);
  /** The group everyone follows in Discuss. */
  const currentGroupId = computed(() => meeting.value?.discussGroupId ?? null);
  /** The group this viewer looks at: the one they preview, else the current one. */
  const viewingGroupId = computed(() => {
    const viewed = viewedGroupId.value;
    if (viewed && discussOrder.value.some((g) => g.groupId === viewed)) return viewed;
    return currentGroupId.value;
  });
  /** Whether the viewer looks at a group other than the current one (read-only). */
  const previewing = computed(
    () => viewingGroupId.value !== null && viewingGroupId.value !== currentGroupId.value,
  );
  /** Place of the current group in the Discuss order, or -1. */
  const currentIndex = computed(() =>
    discussOrder.value.findIndex((g) => g.groupId === currentGroupId.value),
  );
  /** Place of the group the viewer looks at, or -1. */
  const viewingIndex = computed(() =>
    discussOrder.value.findIndex((g) => g.groupId === viewingGroupId.value),
  );
  /** Reactions per card id, each emoji in the order it was first used. */
  const reactionsByCard = computed(() => {
    const by = new Map<string, ReactionDto[]>();
    for (const r of meeting.value?.reactions ?? []) {
      const list = by.get(r.reflectionId) ?? [];
      list.push(r);
      by.set(r.reflectionId, list);
    }
    return by;
  });

  /** Who else is dragging each card right now (spec §9.3), by card id. */
  const draggers = computed(() => {
    const by = new Map<string, ParticipantDto>();
    for (const drag of meeting.value?.drags ?? []) {
      if (drag.userId === me.value) continue;
      const who = participantsById.value.get(drag.userId);
      if (who) by.set(drag.reflectionId, who);
    }
    return by;
  });

  /** Applies the cards of a `group.*` event. */
  function applyMoves(m: MeetingStateDto, moves: CardMove[]): void {
    for (const move of moves) {
      const card = m.reflections.find((r) => r.id === move.id);
      if (!card) continue;
      card.groupId = move.groupId;
      card.version = move.version;
    }
  }

  /** Adds or updates a group from a `group.*` event. */
  function upsertGroup(m: MeetingStateDto, group: GroupDto): void {
    const existing = m.groups.find((g) => g.id === group.id);
    if (existing) Object.assign(existing, group);
    else m.groups.push({ ...group });
  }

  function reset(): void {
    loadSeq++;
    meeting.value = null;
    meetingId.value = null;
    loading.value = false;
    loadError.value = null;
    ended.value = false;
    actionError.value = null;
    notice.value = null;
    phaseLoading.value = false;
    queued = [];
    online = new Set();
    resyncWanted = false;
    generation++;
    pendingVotes = [];
    // A request that never comes back must not hold up the next account's.
    voteChain = Promise.resolve();
    voteSending = false;
    confirmedVotes = [];
    confirmedCast = null;
    voteResyncWanted = false;
    viewedGroupId.value = null;
    pendingReactions.clear();
    clearTimeout(taskResyncTimer);
    taskResyncTimer = undefined;
    taskDrafts.value = {};
  }

  /** Shows one emoji of a card as confirmed, with its pending toggles on top. */
  function renderReaction(m: MeetingStateDto, pending: PendingReaction): void {
    let { count, mine } = pending;
    for (const on of pending.toggles) {
      if (on === mine) continue;
      count += on ? 1 : -1;
      mine = on;
    }
    setReaction(m, pending.reflectionId, pending.emoji, count, mine);
  }

  /** Keeps a group's unsent task text; empty forgets it. */
  function setTaskDraft(groupId: string, html: string): void {
    if (html) taskDrafts.value[groupId] = html;
    else delete taskDrafts.value[groupId];
  }

  /** Shows the confirmed votes with the pending ones on top. */
  function renderVotes(): void {
    const m = meeting.value;
    if (!m) return;
    m.myVotes = confirmedVotes.map((v) => ({ ...v }));
    for (const pending of pendingVotes) bumpVote(m, pending.groupId, pending.delta);
    m.votesCast =
      confirmedCast === null
        ? null
        : confirmedCast + pendingVotes.reduce((sum, v) => sum + v.delta, 0);
  }

  /** Reloads now, or once the votes on their way are answered. */
  function resyncVotes(): void {
    if (voteSending || pendingVotes.length > 0) voteResyncWanted = true;
    else void resync();
  }

  watch(
    () => session.user?.id,
    () => reset(),
  );

  /** `request`, or a failure once it took longer than `VOTE_TIMEOUT_MS`. */
  function withTimeout<T>(request: Promise<T>): Promise<T> {
    let timer: ReturnType<typeof setTimeout> | undefined;
    const late = new Promise<never>((_, reject) => {
      timer = setTimeout(() => reject(new Error('timeout')), VOTE_TIMEOUT_MS);
    });
    return Promise.race([request, late]).finally(() => clearTimeout(timer));
  }

  /** Runs a request for a snapshot, logging the events applied meanwhile. */
  async function logged<T>(call: () => Promise<T>): Promise<{ res: T; log: RealtimeEvent[] }> {
    const log: RealtimeEvent[] = [];
    logs.add(log);
    try {
      return { res: await call(), log };
    } finally {
      logs.delete(log);
    }
  }

  /**
   * Takes a snapshot from the server, then applies again the events of `log`
   * (those that arrived while it was on its way) it may predate: sequenced
   * events past its `seq`, and every unsequenced one (writing, presence),
   * which is newer than what the snapshot holds.
   *
   * `since` is `voteEpoch` when the request went out: if a vote went out or
   * came back meanwhile, the snapshot's votes may be older than the ones
   * confirmed, or already hold a pending one, so they are not taken.
   */
  function accept(state: MeetingStateDto, log: RealtimeEvent[], since: number): void {
    if (state.id !== meetingId.value) return;
    const seen = meeting.value?.seq ?? 0;
    if (since === voteEpoch && !voteSending) {
      confirmedVotes = state.myVotes.map((v) => ({ ...v }));
      confirmedCast = state.votesCast ?? null;
    } else {
      // No count at all is sure: not the facilitator, or not in Vote.
      if (state.votesCast == null) confirmedCast = null;
      else confirmedCast ??= state.votesCast;
      resyncVotes();
    }
    // The current group moved while this snapshot was on its way (e.g. while
    // offline): everyone follows it, previews end.
    if (meeting.value && meeting.value.discussGroupId !== state.discussGroupId) {
      viewedGroupId.value = null;
    }
    meeting.value = state;
    // A toggle on its way: the snapshot is confirmed state, newer or not.
    for (const pending of pendingReactions.values()) {
      if (state.seq <= pending.seq) continue;
      const r = state.reactions.find(
        (x) => x.reflectionId === pending.reflectionId && x.emoji === pending.emoji,
      );
      pending.count = r?.count ?? 0;
      pending.mine = r?.mine ?? false;
      pending.seq = state.seq;
    }
    online = new Set(state.onlineIds);
    markOnline();
    syncClock(state.serverNow);
    replaying = true;
    replayWantsResync = false;
    try {
      for (const event of log) apply(event);
    } finally {
      replaying = false;
    }
    renderVotes();
    // The snapshot may or may not hold the reaction toggles on their way:
    // show them on top of what is confirmed, until answered.
    const shown = meeting.value;
    if (shown) for (const pending of pendingReactions.values()) renderReaction(shown, pending);
    // A phase change replayed on it needs the new phase's snapshot (asked).
    phaseLoading.value = meeting.value?.phase !== state.phase;
    // The log should bring it back to every event seen, and the snapshot
    // hold what the replayed events changed; reload if not.
    if (replayWantsResync || (meeting.value && meeting.value.seq < seen)) void resync();
    replayWantsResync = false;
  }

  /**
   * Opens a meeting of the team: joins it and takes its state from the
   * answer. A meeting of another team is `meeting_not_found`; an ended one
   * only sets `ended` (its summary is the place to read it).
   */
  async function open(id: string, teamId: string): Promise<void> {
    if (meetingId.value !== id) {
      reset();
      meetingId.value = id;
    }
    const seq = ++loadSeq;
    const since = voteEpoch;
    loading.value = true;
    loadError.value = null;
    ended.value = false;
    try {
      const res = await api.joinMeeting(teamId, id);
      if (res.status !== 200) throw apiError(res);
      if (seq !== loadSeq) return;
      meeting.value = null;
      accept(res.data, [], since);
      const pending = queued;
      queued = [];
      for (const event of pending) apply(event);
    } catch (e) {
      if (seq !== loadSeq) return;
      const code = errorCode(e);
      if (code === 'meeting_ended') ended.value = true;
      else loadError.value = code;
    } finally {
      if (seq === loadSeq) {
        loading.value = false;
        if (resyncWanted) {
          resyncWanted = false;
          if (meeting.value) void resync();
        }
      }
    }
  }

  /**
   * Reloads the snapshot (after a gap, a subscription, a reconnect or a new
   * phase). While `open()` is loading it waits for it, so the open still
   * completes.
   */
  async function resync(): Promise<void> {
    const id = meetingId.value;
    if (!id) return;
    if (loading.value) {
      resyncWanted = true;
      return;
    }
    const seq = ++loadSeq;
    const since = voteEpoch;
    try {
      const { res, log } = await logged(() => api.getMeetingState(id));
      if (res.status !== 200) throw apiError(res);
      if (seq !== loadSeq) return;
      accept(res.data, log, since);
    } catch (e) {
      if (seq === loadSeq) loadError.value = errorCode(e);
    }
  }

  /** Re-reads the state once the task events coming close together end. */
  function resyncTasksSoon(): void {
    if (taskResyncTimer !== undefined) return;
    taskResyncTimer = setTimeout(() => {
      taskResyncTimer = undefined;
      void resync();
    }, TASK_RESYNC_DELAY_MS);
  }

  /** Applies one event of the meeting channel. */
  function apply(event: RealtimeEvent): void {
    if (event.type === 'resync') {
      void resync();
      return;
    }
    // The server refused or ended the subscription (spec §11.1: the code is
    // in the payload), e.g. the user was removed from the team. The room
    // cannot stay live: drop the snapshot, and any load in flight, and show
    // the error instead.
    if (event.type === 'error') {
      loadSeq++;
      meeting.value = null;
      phaseLoading.value = false;
      queued = [];
      resyncWanted = false;
      loading.value = false;
      loadError.value = (event.payload?.code as string | undefined) ?? 'unknown';
      return;
    }
    const m = meeting.value;
    if (!m) {
      queued.push(event);
      return;
    }
    if (event.seq !== null) {
      if (event.seq <= m.seq) return;
      if (event.seq > m.seq + 1) {
        void resync();
        return;
      }
      m.seq = event.seq;
    }
    if (!replaying) for (const log of logs) log.push(event);
    const p = event.payload;
    switch (event.type) {
      case 'participant.joined': {
        const joined = p.participant as ParticipantDto;
        const existing = m.participants.find((x) => x.id === joined.id);
        if (existing) Object.assign(existing, joined);
        else m.participants.push({ ...joined });
        markOnline();
        m.icebreaker.order = p.order as string[];
        break;
      }
      case 'presence.changed':
        if (p.online) online.add(p.userId as string);
        else online.delete(p.userId as string);
        markOnline();
        break;
      case 'facilitator.changed': {
        m.facilitatorId = p.facilitatorId as string;
        const name = participantsById.value.get(m.facilitatorId)?.displayName ?? '';
        notice.value = { kind: 'facilitator', userId: m.facilitatorId, name };
        // Only the facilitator sees how many votes were cast: hide it at
        // once from the one who left the role, re-read it for the new one.
        if (m.facilitatorId !== me.value) {
          confirmedCast = null;
          m.votesCast = null;
        }
        if (m.phase === MeetingPhase.vote && !replaying) void resync();
        break;
      }
      case 'phase.changed':
        m.phase = p.phase as MeetingStateDto['phase'];
        // Ready flags reset with every phase; the snapshot of the new phase
        // (e.g. every reflection, from Group on) comes from the server, and
        // the room waits for it.
        for (const participant of m.participants) participant.ready = false;
        phaseLoading.value = true;
        void resync();
        break;
      case 'timer.updated':
        syncClock(p.serverNow);
        m.timer = (p.timer as Timer | null) ?? null;
        break;
      case 'ready.changed': {
        const participant = m.participants.find((x) => x.id === p.userId);
        if (participant) participant.ready = p.ready as boolean;
        break;
      }
      case 'reflection.count_changed':
        m.reflectionCounts = p.counts as MeetingStateDto['reflectionCounts'];
        break;
      case 'reflection.writing':
        m.writing = p.counts as MeetingStateDto['writing'];
        break;
      case 'card.dragging':
        m.drags = p.drags as MeetingStateDto['drags'];
        break;
      case 'group.created':
      case 'group.updated':
        // This tab's own change came back with the answer to its request,
        // unless it is replayed on an older answer that predates it.
        if (!replaying && isOwnEvent(p)) break;
        upsertGroup(m, p.group as GroupDto);
        applyMoves(m, p.reflections as CardMove[]);
        break;
      case 'group.deleted':
        if (!replaying && isOwnEvent(p)) break;
        m.groups = m.groups.filter((g) => g.id !== (p.group as { id: string }).id);
        applyMoves(m, p.reflections as CardMove[]);
        break;
      case 'reflection.changed':
        // Another tab of ours changed one of our reflections.
        if (!isOwnEvent(p)) void resync();
        break;
      case 'vote.changed':
        // Our own votes changed (another tab of ours, or a settings change
        // removed some): re-read them. This tab's own came with the answer.
        if (!isOwnEvent(p)) void resync();
        break;
      case 'vote.cast_changed':
        // The facilitator's total, absolute: taken as it is, this tab's own
        // too (it may arrive after the answer). Events arrive in commit
        // order, so the last one, replayed or not, is the newest total.
        if (m.facilitatorId !== me.value) break;
        // It may or may not hold a vote of this tab on its way: that vote's
        // answer brings a sure total, and replays this event after it.
        if (voteSending || pendingVotes.length > 0) break;
        confirmedCast = p.votesCast as number;
        if (!replaying) renderVotes();
        break;
      case 'vote.settings_changed':
        m.voteBudget = p.budget as number;
        m.voteMaxPerGroup = p.maxPerGroup as number;
        break;
      case 'discuss.group_changed':
        m.discussGroupId = p.groupId as string;
        // Everyone follows the facilitator: previews end.
        if (!replaying) viewedGroupId.value = null;
        break;
      case 'reaction.changed': {
        // Absolute and in commit order: taken as it is, unless this tab has
        // a toggle of that emoji on its way. Then it is confirmed state when
        // newer than what the toggle has (its answer may come before or
        // after), with the toggles on top.
        const pending = pendingReactions.get(
          reactionKey(p.reflectionId as string, p.emoji as string),
        );
        if (pending) {
          if (event.seq !== null && event.seq > pending.seq) {
            pending.count = p.count as number;
            pending.seq = event.seq;
            // This tab's own echo: the toggle on its way landed, and the
            // count holds it.
            const sent = pending.toggles[0];
            if (isOwnEvent(p) && sent !== undefined) pending.mine = sent;
            renderReaction(m, pending);
          }
          break;
        }
        setReaction(m, p.reflectionId as string, p.emoji as string, p.count as number);
        break;
      }
      case 'reaction.mine_changed': {
        // Another tab of ours reacted. This tab's own came with the answer,
        // unless it is replayed on an older snapshot that predates it.
        if (!replaying && isOwnEvent(p)) break;
        // A toggle of it on its way: its answer says.
        if (pendingReactions.has(reactionKey(p.reflectionId as string, p.emoji as string))) break;
        const r = m.reactions.find((x) => x.reflectionId === p.reflectionId && x.emoji === p.emoji);
        if (r) r.mine = p.mine as boolean;
        break;
      }
      case 'task.created':
      case 'task.updated':
        // The Tasks panel: re-read, unless this tab made the change (its
        // answer brought it). Replayed, the snapshot predates the change,
        // whoever made it: re-read once the replay is done. Live, a burst
        // (e.g. edits on the board) is re-read once.
        if (replaying) replayWantsResync = true;
        else if (!isOwnEvent(p)) resyncTasksSoon();
        break;
      case 'task.deleted':
        // Nothing to read: it leaves the panel, whoever deleted it.
        m.tasks = m.tasks.filter((task) => task.id !== p.taskId);
        break;
      case 'task.moved':
        // The panel shows neither status nor position.
        break;
      case 'icebreaker.person_changed':
        m.icebreaker.index = p.index as number;
        break;
      case 'meeting.renamed':
        m.title = p.title as string;
        break;
      case 'meeting.ended':
        m.status = MeetingStatus.ended;
        m.endedAt = p.endedAt as string;
        break;
    }
  }

  /** Runs a meeting action and takes the server's state from its answer. */
  async function act(
    call: () => Promise<{ status: number; data: unknown }>,
    optimistic?: () => void,
  ): Promise<boolean> {
    return (await actResult(call, optimistic)).ok;
  }

  /** As `act()`, and says why it failed. */
  async function actResult(
    call: () => Promise<{ status: number; data: unknown }>,
    optimistic?: () => void,
  ): Promise<{ ok: true } | { ok: false; error: unknown }> {
    const before = meeting.value ? structuredClone(toRaw(meeting.value)) : null;
    const since = voteEpoch;
    actionError.value = null;
    if (optimistic) {
      optimisticInFlight++;
      optimistic();
    }
    try {
      const { res, log } = await logged(call);
      if (res.status !== 200 && res.status !== 201) throw apiError(res);
      accept(res.data as MeetingStateDto, log, since);
      return { ok: true };
    } catch (e) {
      if (optimistic && before) undo(before);
      actionError.value = errorCode(e);
      return { ok: false, error: e };
    } finally {
      if (optimistic) optimisticInFlight--;
    }
  }

  /**
   * Takes back a refused optimistic change. If events moved the meeting on
   * meanwhile (or another one is open), `before` is out of date: reload.
   * So it is while another optimistic action is pending: `before` may hold,
   * or lack, that action's change.
   */
  function undo(before: MeetingStateDto): void {
    const now = meeting.value;
    if (now && now.id === before.id && now.seq === before.seq && optimisticInFlight === 1) {
      // Presence, writing and drags carry no seq: keep the current ones.
      // Nor do votes, which the refused action did not touch.
      // (Raw: the next action clones the state, which a proxy would break.)
      before.writing = toRaw(now.writing);
      before.drags = toRaw(now.drags);
      before.myVotes = toRaw(now.myVotes);
      before.votesCast = now.votesCast;
      meeting.value = before;
      markOnline();
    } else if (now?.id === before.id) {
      void resync();
    }
  }

  function nextPerson(): Promise<boolean> {
    const id = meetingId.value!;
    return act(
      () => api.icebreakerNext(id),
      () => {
        meeting.value!.icebreaker.index += 1;
      },
    );
  }

  /** Back to someone who already had their turn; the turn continues from them. */
  function goToPerson(userId: string): Promise<boolean> {
    const id = meetingId.value!;
    return act(
      () => api.icebreakerGoto(id, { userId }),
      () => {
        const m = meeting.value!;
        const position = m.icebreaker.order.indexOf(userId);
        if (position >= 0 && position < m.icebreaker.index) m.icebreaker.index = position;
      },
    );
  }

  function advancePhase(): Promise<boolean> {
    const id = meetingId.value!;
    return act(() => api.nextPhase(id));
  }

  function rename(title: string): Promise<boolean> {
    const id = meetingId.value!;
    return act(
      () => api.renameMeeting(id, { title }),
      () => {
        meeting.value!.title = title.trim();
      },
    );
  }

  function setFacilitator(userId: string): Promise<boolean> {
    const id = meetingId.value!;
    return act(() => api.setFacilitator(id, { userId }));
  }

  function end(): Promise<boolean> {
    const id = meetingId.value!;
    return act(() => api.endMeeting(id));
  }

  function setTimer(action: TimerAction): Promise<boolean> {
    const id = meetingId.value!;
    return act(() => api.setTimer(id, action));
  }

  function setReady(ready: boolean): Promise<boolean> {
    const id = meetingId.value!;
    return act(
      () => api.setReady(id, { ready }),
      () => {
        const mine = meeting.value!.participants.find((p) => p.id === me.value);
        if (mine) mine.ready = ready;
      },
    );
  }

  function createReflection(column: ReflectionColumn, content: string): Promise<boolean> {
    const id = meetingId.value!;
    return act(() => api.createReflection(id, { column, content }));
  }

  function updateReflection(
    reflectionId: string,
    change: { column?: ReflectionColumn; content?: string },
  ): Promise<boolean> {
    return act(() => api.updateReflection(reflectionId, change));
  }

  function deleteReflection(reflectionId: string): Promise<boolean> {
    return act(
      () => api.deleteReflection(reflectionId),
      () => {
        const m = meeting.value!;
        const gone = m.reflections.find((r) => r.id === reflectionId);
        m.reflections = m.reflections.filter((r) => r.id !== reflectionId);
        if (gone) m.reflectionCounts[gone.column] -= 1;
      },
    );
  }

  /**
   * Drops a card onto another card: a new group of both at the target's
   * place, or the target's group if it has one (spec §9.3).
   */
  function dropOnCard(targetId: string, reflectionId: string): Promise<boolean> {
    const id = meetingId.value!;
    return act(
      () => api.createGroup(id, { targetId, reflectionId }),
      () => {
        const m = meeting.value!;
        const target = m.reflections.find((r) => r.id === targetId);
        const card = m.reflections.find((r) => r.id === reflectionId);
        if (!target || !card || target === card) return;
        if (target.groupId) {
          if (card.groupId === target.groupId) return;
          const into = target.groupId;
          leaveGroup(m, card);
          card.groupId = into;
          return;
        }
        // A placeholder until the server's answer brings the real group.
        const group: GroupDto = {
          id: `${PENDING_GROUP}${reflectionId}`,
          column: target.column,
          position: target.position,
          name: '',
        };
        leaveGroup(m, card);
        m.groups.push(group);
        target.groupId = group.id;
        card.groupId = group.id;
      },
    );
  }

  /**
   * Moves a card into a group, or out of its group with `null`. A group not
   * created yet has no id to send: the card is dropped onto one of its cards
   * instead, which joins the group the server makes (spec §9.3).
   */
  function moveToGroup(reflectionId: string, groupId: string | null): Promise<boolean> {
    if (groupId && isPendingGroup(groupId)) {
      const target = meeting.value?.reflections.find(
        (r) => r.groupId === groupId && r.id !== reflectionId,
      );
      return target ? dropOnCard(target.id, reflectionId) : Promise.resolve(false);
    }
    return act(
      () => api.moveCard(reflectionId, { groupId }),
      () => {
        const m = meeting.value!;
        const card = m.reflections.find((r) => r.id === reflectionId);
        if (!card || card.groupId === groupId) return;
        leaveGroup(m, card);
        card.groupId = groupId;
      },
    );
  }

  /** Names a group; empty shows as "Untitled group". */
  function renameGroup(groupId: string, name: string): Promise<boolean> {
    if (isPendingGroup(groupId)) return Promise.resolve(false);
    return act(
      () => api.renameGroup(groupId, { name }),
      () => {
        const group = meeting.value!.groups.find((g) => g.id === groupId);
        if (group) group.name = name.trim();
      },
    );
  }

  /**
   * Gives a group one more of the viewer's votes, or takes one back (spec
   * §9.4). Refused locally when over a limit; requests go out in order.
   */
  function vote(groupId: string, delta: 1 | -1): Promise<boolean> {
    const m = meeting.value;
    if (!m) return Promise.resolve(false);
    const mine = myVotes.value.get(groupId) ?? 0;
    if (delta > 0 && (votesLeft.value <= 0 || mine >= m.voteMaxPerGroup)) {
      return Promise.resolve(false);
    }
    if (delta < 0 && mine <= 0) return Promise.resolve(false);
    const id = m.id;
    // The account and meeting the click was made in: if either changes
    // before it goes out or comes back, it is dropped.
    const gen = generation;
    const entry: PendingVote = { groupId, delta };
    pendingVotes.push(entry);
    renderVotes();
    actionError.value = null;
    const run = voteChain.then(async () => {
      if (gen !== generation) return false;
      voteSending = true;
      voteEpoch++;
      let res: Awaited<ReturnType<typeof api.vote>> | null = null;
      let log: RealtimeEvent[] = [];
      let failure: unknown = null;
      try {
        ({ res, log } = await logged(() => withTimeout(api.vote(groupId, { delta }))));
        if (res.status !== 200) throw apiError(res);
      } catch (e) {
        failure = e;
      }
      if (gen !== generation) return false;
      voteSending = false;
      voteEpoch++;
      pendingVotes = pendingVotes.filter((v) => v !== entry);
      if (failure === null && res?.status === 200) {
        // The answer holds every vote up to this one: sure.
        accept(res.data, log, voteEpoch);
      } else {
        // Take the refused vote back at once, keeping later clicks, then
        // learn why from the server.
        renderVotes();
        if (meetingId.value === id) {
          actionError.value = errorCode(failure);
          voteResyncWanted = true;
        }
      }
      if (voteResyncWanted && pendingVotes.length === 0 && meetingId.value === id) {
        voteResyncWanted = false;
        await resync();
      }
      return failure === null;
    });
    voteChain = run;
    return run;
  }

  /**
   * Sets the vote budget and the most votes per group (facilitator). When the
   * answer is lost or broken (not a refusal), the change may have committed
   * (and trimmed votes): reload the state rather than trust the old one.
   */
  async function saveVoteSettings(budget: number, maxPerGroup: number): Promise<boolean> {
    const id = meetingId.value!;
    const result = await actResult(() =>
      api.updateSettings(id, { voteBudget: budget, voteMaxPerGroup: maxPerGroup }),
    );
    if (result.ok) return true;
    const refused = result.error instanceof ApiError && result.error.status < 500;
    if (!refused && meetingId.value === id) void resync();
    return false;
  }

  /**
   * How many people `budget` and `maxPerGroup` would take votes from
   * (facilitator; the server counts, never says who). `null` if unknown.
   */
  async function voteImpact(budget: number, maxPerGroup: number): Promise<number | null> {
    const id = meetingId.value;
    if (!id) return null;
    try {
      const res = await api.settingsImpact(id, {
        voteBudget: budget,
        voteMaxPerGroup: maxPerGroup,
      });
      return res.status === 200 ? res.data.affected : null;
    } catch {
      return null;
    }
  }

  /** Looks at another group in Discuss, without affecting anyone (spec §9.5). */
  function viewGroup(groupId: string): void {
    viewedGroupId.value = groupId === currentGroupId.value ? null : groupId;
  }

  /** Back to the group everyone follows. */
  function followCurrent(): void {
    viewedGroupId.value = null;
  }

  /** Moves everyone to a group (facilitator, during Discuss). */
  function setCurrentGroup(groupId: string): Promise<boolean> {
    const id = meetingId.value!;
    return act(
      () => api.setCurrentGroup(id, { groupId }),
      () => {
        meeting.value!.discussGroupId = groupId;
        viewedGroupId.value = null;
      },
    );
  }

  /** Writes a task for a group of the meeting (the current one). */
  function createTask(groupId: string, content: string): Promise<boolean> {
    const id = meetingId.value!;
    return act(() => api.createTask(id, { groupId, content }));
  }

  /**
   * Puts the viewer's `emoji` on a card, or takes it back when it is already
   * theirs (spec §9.5: one of each emoji per person). Shown at once; the
   * answer brings the count. Toggles of the same emoji on the same card go
   * out one at a time, in click order.
   */
  function toggleReaction(reflectionId: string, emoji: string): Promise<boolean> {
    const m = meeting.value;
    if (!m) return Promise.resolve(false);
    const key = reactionKey(reflectionId, emoji);
    const existing = m.reactions.find((r) => r.reflectionId === reflectionId && r.emoji === emoji);
    const on = !existing?.mine;
    let pending = pendingReactions.get(key);
    if (!pending) {
      pending = {
        reflectionId,
        emoji,
        toggles: [],
        count: existing?.count ?? 0,
        mine: existing?.mine ?? false,
        seq: m.seq,
        chain: Promise.resolve(true),
      };
      pendingReactions.set(key, pending);
    }
    const entry = pending;
    entry.toggles.push(on);
    renderReaction(m, entry);
    const id = m.id;
    // The account and meeting the click was made in: if either changes
    // before it goes out or comes back, it is dropped.
    const gen = generation;
    actionError.value = null;
    const run = entry.chain.then(async () => {
      if (gen !== generation) return false;
      let answer: ReactionAnswerDto | null = null;
      let failure: unknown = null;
      try {
        const encoded = encodeURIComponent(emoji);
        // A request that never comes back must not hold up the next clicks.
        const res = await withTimeout(
          on ? api.addReaction(reflectionId, encoded) : api.removeReaction(reflectionId, encoded),
        );
        if (res.status !== 200) throw apiError(res);
        answer = res.data;
      } catch (e) {
        failure = e;
      }
      if (gen !== generation) return false;
      entry.toggles.shift();
      if (answer) {
        // An event newer than the answer may have come first: its count
        // already holds this toggle, and stays.
        if (answer.seq > entry.seq) {
          entry.count = answer.count;
          entry.seq = answer.seq;
        }
        entry.mine = answer.mine;
      }
      // A refused toggle is taken back at once, keeping later clicks.
      const last = entry.toggles.length === 0;
      if (last) pendingReactions.delete(key);
      const now = meeting.value;
      if (now?.id === id) {
        renderReaction(now, entry);
        if (failure !== null) actionError.value = errorCode(failure);
        // Timed out: it may have landed or not. The state says.
        if (last && failure instanceof Error && failure.message === 'timeout') void resync();
      }
      return failure === null;
    });
    entry.chain = run;
    return run;
  }

  return {
    meeting,
    meetingId,
    loading,
    loadError,
    ended,
    phaseLoading,
    actionError,
    notice,
    isFacilitator,
    myRole,
    isActive,
    canTakeOver,
    participantsById,
    facilitator,
    phaseIndex,
    nextPhase,
    speakers,
    speakerIndex,
    currentSpeaker,
    nextSpeaker,
    previousSpeaker,
    isLastSpeaker,
    readyCount,
    iAmReady,
    hasReady,
    myReflections,
    groupCards,
    columnItems,
    draggers,
    myVotes,
    voteBudget,
    voteMaxPerGroup,
    votesUsed,
    votesLeft,
    votesCast,
    discussOrder,
    currentGroupId,
    viewedGroupId,
    viewingGroupId,
    previewing,
    currentIndex,
    viewingIndex,
    reactionsByCard,
    clockOffset,
    serverTime,
    open,
    resync,
    apply,
    reset,
    nextPerson,
    goToPerson,
    advancePhase,
    rename,
    setFacilitator,
    end,
    setTimer,
    setReady,
    createReflection,
    updateReflection,
    deleteReflection,
    dropOnCard,
    moveToGroup,
    renameGroup,
    vote,
    saveVoteSettings,
    voteImpact,
    viewGroup,
    followCurrent,
    setCurrentGroup,
    createTask,
    taskDrafts,
    setTaskDraft,
    toggleReaction,
  };
});
