import { defineStore } from 'pinia';
import { computed, ref, toRaw, watch } from 'vue';

import {
  api,
  apiError,
  errorCode,
  MeetingPhase,
  MeetingStatus,
  Role,
  type MeetingStateDto,
  type ParticipantDto,
} from '@/api/client';
import type { RealtimeEvent } from '@/api/realtime';
import { useSessionStore } from '@/stores/session';

export const PHASES = Object.values(MeetingPhase);

/** A notice for everyone in the room, e.g. a new facilitator. */
export interface MeetingNotice {
  kind: 'facilitator';
  userId: string;
  name: string;
}

/**
 * The meeting room's state (spec §15.4): a snapshot from the server, kept up
 * to date by applying the meeting channel's events in `seq` order. A gap in
 * the sequence, or a reconnect, reloads the snapshot.
 */
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
   * Who is in the room, participant or not yet: presence can arrive before
   * the participant's join, so it is kept apart and applied to participants.
   */
  let online = new Set<string>();

  function markOnline(): void {
    for (const p of meeting.value?.participants ?? []) p.online = online.has(p.id);
  }
  let loadSeq = 0;

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
  const isLastSpeaker = computed(() => speakerIndex.value >= speakers.value.length - 1);

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
  }

  watch(
    () => session.user?.id,
    () => reset(),
  );

  /** Takes a snapshot from the server unless ours is newer. */
  function accept(state: MeetingStateDto): void {
    if (state.id !== meetingId.value) return;
    if (meeting.value && meeting.value.seq > state.seq) return;
    meeting.value = state;
    online = new Set(state.onlineIds);
    markOnline();
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
      if (seq === loadSeq) loading.value = false;
    }
  }

  /** Reloads the snapshot (after a gap or a reconnect). */
  async function resync(): Promise<void> {
    const id = meetingId.value;
    if (!id) return;
    const seq = ++loadSeq;
    try {
      const res = await api.getMeetingState(id);
      if (res.status !== 200) throw apiError(res);
      if (seq !== loadSeq) return;
      meeting.value = res.data;
      online = new Set(res.data.onlineIds);
      markOnline();
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
        break;
      case 'icebreaker.person_changed':
        m.icebreaker.index = p.index as number;
        m.icebreaker.skipped = p.skipped as string[];
        break;
      case 'meeting.renamed':
        m.title = p.title as string;
        break;
      case 'meeting.ended':
        m.status = MeetingStatus.ended;
        m.endedAt = p.endedAt as string;
        break;
      case 'error':
        loadError.value = (p?.code as string | undefined) ?? 'unknown';
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
    optimistic?.();
    try {
      const res = await call();
      if (res.status !== 200) throw apiError(res);
      accept(res.data as MeetingStateDto);
      return true;
    } catch (e) {
      if (optimistic && before) meeting.value = before;
      actionError.value = errorCode(e);
      return false;
    }
  }

  function nextPerson(skip = false): Promise<boolean> {
    const id = meetingId.value!;
    return act(
      () => api.icebreakerNext(id, { skip }),
      () => {
        const m = meeting.value!;
        const current = m.icebreaker.order[m.icebreaker.index];
        if (skip && current && !m.icebreaker.skipped.includes(current)) {
          m.icebreaker.skipped = [...m.icebreaker.skipped, current];
        }
        m.icebreaker.index += 1;
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
    isLastSpeaker,
    open,
    resync,
    apply,
    reset,
    nextPerson,
    advancePhase,
    rename,
    setFacilitator,
    end,
  };
});
