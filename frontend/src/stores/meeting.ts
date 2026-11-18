import { defineStore } from 'pinia';
import { computed, ref, toRaw, watch } from 'vue';

import {
  api,
  apiError,
  errorCode,
  MeetingPhase,
  MeetingStatus,
  Role,
  ReflectionColumn,
  type GroupDto,
  type MeetingStateDto,
  type ParticipantDto,
  type ReflectionDto,
  type Timer,
  type TimerAction,
} from '@/api/client';
import { isOwnEvent } from '@/api/clientId';
import type { RealtimeEvent } from '@/api/realtime';
import { useSessionStore } from '@/stores/session';

export const PHASES = Object.values(MeetingPhase);
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
   * Server time minus local time, in ms, from the last `serverNow` seen: the
   * timer counts down on server time, whatever the local clock says.
   */
  const clockOffset = ref(0);

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
    actionError.value = null;
    notice.value = null;
    queued = [];
    online = new Set();
    resyncWanted = false;
  }

  watch(
    () => session.user?.id,
    () => reset(),
  );

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
   */
  function accept(state: MeetingStateDto, log: RealtimeEvent[] = []): void {
    if (state.id !== meetingId.value) return;
    const seen = meeting.value?.seq ?? 0;
    meeting.value = state;
    online = new Set(state.onlineIds);
    markOnline();
    syncClock(state.serverNow);
    replaying = true;
    try {
      for (const event of log) apply(event);
    } finally {
      replaying = false;
    }
    // The log should bring it back to every event seen; reload if it did not.
    if (meeting.value && meeting.value.seq < seen) void resync();
  }

  /** Opens a meeting: joins it while it runs, then loads its state. */
  async function open(id: string): Promise<void> {
    if (meetingId.value !== id) {
      reset();
      meetingId.value = id;
    }
    const seq = ++loadSeq;
    loading.value = true;
    loadError.value = null;
    try {
      // An ended meeting cannot be joined, but can still be read.
      const joined = await api.joinMeeting(id);
      const res = joined.status === 409 ? await api.getMeetingState(id) : joined;
      if (res.status !== 200) throw apiError(res);
      if (seq !== loadSeq) return;
      meeting.value = null;
      accept(res.data);
      const pending = queued;
      queued = [];
      for (const event of pending) apply(event);
    } catch (e) {
      if (seq === loadSeq) loadError.value = errorCode(e);
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
    try {
      const { res, log } = await logged(() => api.getMeetingState(id));
      if (res.status !== 200) throw apiError(res);
      if (seq !== loadSeq) return;
      accept(res.data, log);
    } catch (e) {
      if (seq === loadSeq) loadError.value = errorCode(e);
    }
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
        break;
      }
      case 'phase.changed':
        m.phase = p.phase as MeetingStateDto['phase'];
        // Ready flags reset with every phase; the snapshot of the new phase
        // (e.g. every reflection, from Group on) comes from the server.
        for (const participant of m.participants) participant.ready = false;
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
    const before = meeting.value ? structuredClone(toRaw(meeting.value)) : null;
    actionError.value = null;
    if (optimistic) {
      optimisticInFlight++;
      optimistic();
    }
    try {
      const { res, log } = await logged(call);
      if (res.status !== 200 && res.status !== 201) throw apiError(res);
      accept(res.data as MeetingStateDto, log);
      return true;
    } catch (e) {
      if (optimistic && before) undo(before);
      actionError.value = errorCode(e);
      return false;
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
      // (Raw: the next action clones the state, which a proxy would break.)
      before.writing = toRaw(now.writing);
      before.drags = toRaw(now.drags);
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

  return {
    meeting,
    meetingId,
    loading,
    loadError,
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
  };
});
