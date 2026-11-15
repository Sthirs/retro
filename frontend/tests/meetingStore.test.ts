import { createPinia, setActivePinia } from 'pinia';
import { beforeEach, describe, expect, it } from 'vitest';
import { nextTick } from 'vue';

import { useMeetingStore } from '@/stores/meeting';
import { useSessionStore } from '@/stores/session';

import { ALICE, BOB, MEETING_ID, meetingState, mockFetch, participant, problem } from './helpers';

const STATE = `GET /api/v1/meetings/${MEETING_ID}/state`;
const JOIN = `POST /api/v1/meetings/${MEETING_ID}/join`;
const channel = `meeting:${MEETING_ID}`;

async function opened(routes: Parameters<typeof mockFetch>[0] = {}) {
  const fetch = mockFetch({ [JOIN]: { status: 200, body: meetingState() }, ...routes });
  const store = useMeetingStore();
  useSessionStore().setUser({ ...ALICE, pictureVersion: null });
  // A new user resets the store: let that happen before opening.
  await nextTick();
  await store.open(MEETING_ID);
  return { store, ...fetch };
}

describe('meeting store', () => {
  beforeEach(() => setActivePinia(createPinia()));

  it('joins and derives the room from the snapshot', async () => {
    const { store } = await opened();
    expect(store.meeting?.title).toBe('Retro #3');
    expect(store.isFacilitator).toBe(true);
    expect(store.currentSpeaker?.id).toBe(ALICE.id);
    expect(store.nextSpeaker?.id).toBe(BOB.id);
    expect(store.nextPhase).toBe('reflect');
  });

  it('reads an ended meeting that can no longer be joined', async () => {
    const { store } = await opened({
      [JOIN]: problem(409, 'meeting_ended'),
      [STATE]: { status: 200, body: meetingState({ status: 'ended' }) },
    });
    expect(store.isActive).toBe(false);
  });

  it('applies events in seq order and ignores old ones', async () => {
    const { store } = await opened();
    const carol = {
      ...BOB,
      id: '0190a4e0-0000-7000-8000-000000000003',
      displayName: 'Carol Verdi',
    };
    store.apply({
      type: 'participant.joined',
      channel,
      seq: 3,
      payload: {
        participant: participant(carol, { online: false }),
        order: [ALICE.id, carol.id, BOB.id],
      },
    });
    store.apply({
      type: 'icebreaker.person_changed',
      channel,
      seq: 4,
      payload: { index: 1 },
    });
    store.apply({
      type: 'icebreaker.person_changed',
      channel,
      seq: 4,
      payload: { index: 9 },
    });
    store.apply({
      type: 'presence.changed',
      channel,
      seq: null,
      payload: { userId: carol.id, online: true },
    });
    expect(store.meeting!.seq).toBe(4);
    expect(store.currentSpeaker?.displayName).toBe('Carol Verdi');
    expect(store.currentSpeaker?.online).toBe(true);

    store.apply({
      type: 'facilitator.changed',
      channel,
      seq: 5,
      payload: { facilitatorId: BOB.id },
    });
    expect(store.isFacilitator).toBe(false);
    expect(store.notice).toEqual({ kind: 'facilitator', userId: BOB.id, name: 'Bob Bianchi' });

    store.apply({
      type: 'meeting.ended',
      channel,
      seq: 6,
      payload: { endedAt: '2026-11-10T18:00:00Z' },
    });
    expect(store.isActive).toBe(false);
  });

  it('keeps presence that arrives before the join', async () => {
    const { store } = await opened();
    const dana = { ...BOB, id: '0190a4e0-0000-7000-8000-000000000004', displayName: 'Dana Neri' };
    store.apply({
      type: 'presence.changed',
      channel,
      seq: null,
      payload: { userId: dana.id, online: true },
    });
    store.apply({
      type: 'participant.joined',
      channel,
      seq: 3,
      payload: {
        participant: participant(dana, { online: false }),
        order: [ALICE.id, BOB.id, dana.id],
      },
    });
    expect(store.participantsById.get(dana.id)?.online).toBe(true);
    store.apply({
      type: 'presence.changed',
      channel,
      seq: null,
      payload: { userId: dana.id, online: false },
    });
    expect(store.participantsById.get(dana.id)?.online).toBe(false);
  });

  it('reloads the snapshot after a gap or a resync', async () => {
    const { store, calls } = await opened({
      [STATE]: { status: 200, body: meetingState({ seq: 9, phase: 'group' }) },
    });
    store.apply({ type: 'phase.changed', channel, seq: 5, payload: { phase: 'vote' } });
    await new Promise((r) => setTimeout(r));
    expect(calls.filter((c) => c.path.endsWith('/state'))).toHaveLength(1);
    expect(store.meeting!.phase).toBe('group');
    expect(store.meeting!.seq).toBe(9);

    store.apply({ type: 'resync', channel, seq: null, payload: {} });
    await new Promise((r) => setTimeout(r));
    expect(calls.filter((c) => c.path.endsWith('/state'))).toHaveLength(2);
  });

  it('lets a resync asked for during the open wait for it', async () => {
    const store = useMeetingStore();
    useSessionStore().setUser({ ...ALICE, pictureVersion: null });
    await nextTick();
    const { calls } = mockFetch({
      [JOIN]: { status: 200, body: meetingState({ seq: 2 }) },
      [STATE]: { status: 200, body: meetingState({ seq: 3, title: 'Renamed' }) },
    });
    const opening = store.open(MEETING_ID);
    // The subscription is acknowledged, and an event lands, before the join answers.
    store.apply({ type: 'resync', channel, seq: null, payload: {} });
    store.apply({ type: 'phase.changed', channel, seq: 3, payload: { phase: 'reflect' } });
    await opening;
    // The open completed: not loading, its queued event replayed.
    expect(store.loading).toBe(false);
    expect(store.meeting!.phase).toBe('reflect');
    await new Promise((r) => setTimeout(r));
    // Then the resync ran.
    expect(calls.filter((c) => c.path.endsWith('/state'))).toHaveLength(1);
    expect(store.meeting!.title).toBe('Renamed');
  });

  it('ignores a resync snapshot older than the events applied meanwhile', async () => {
    let answer: (() => void) | null = null;
    const { store, fetchMock } = await opened();
    fetchMock.mockImplementationOnce((async () => {
      await new Promise<void>((r) => (answer = r));
      return new Response(JSON.stringify(meetingState({ seq: 2, title: 'Old' })), {
        status: 200,
        headers: { 'Content-Type': 'application/json' },
      });
    }) as never);
    const resyncing = store.resync();
    store.apply({ type: 'meeting.renamed', channel, seq: 3, payload: { title: 'New' } });
    answer!();
    await resyncing;
    expect(store.meeting!.title).toBe('New');
    expect(store.meeting!.seq).toBe(3);
  });

  it('keeps events that landed while a refused action was on its way', async () => {
    let refuse: (() => void) | null = null;
    const { store, fetchMock } = await opened({
      [STATE]: { status: 200, body: meetingState({ seq: 3, status: 'ended' }) },
    });
    fetchMock.mockImplementationOnce((async () => {
      await new Promise<void>((r) => (refuse = r));
      return new Response(JSON.stringify({ code: 'not_facilitator' }), {
        status: 403,
        headers: { 'Content-Type': 'application/problem+json' },
      });
    }) as never);
    const pending = store.nextPerson();
    store.apply({
      type: 'meeting.ended',
      channel,
      seq: 3,
      payload: { endedAt: '2026-11-15T10:00:00Z' },
    });
    refuse!();
    expect(await pending).toBe(false);
    // Not rolled back to the running meeting of before.
    expect(store.meeting!.status).toBe('ended');
    await new Promise((r) => setTimeout(r));
    expect(store.meeting!.status).toBe('ended');
    expect(store.meeting!.seq).toBe(3);
  });

  it('moves to the next person at once and rolls back when refused', async () => {
    const { store, calls } = await opened({
      [`POST /api/v1/meetings/${MEETING_ID}/icebreaker/next`]: problem(403, 'not_facilitator'),
    });
    const pending = store.nextPerson();
    expect(store.speakerIndex).toBe(1);
    expect(await pending).toBe(false);
    expect(store.speakerIndex).toBe(0);
    expect(store.actionError).toBe('not_facilitator');
    expect(calls.at(-1)?.path).toBe(`/api/v1/meetings/${MEETING_ID}/icebreaker/next`);
  });

  it('goes back to someone who already had their turn, at once', async () => {
    const { store, calls } = await opened({
      [JOIN]: {
        status: 200,
        body: meetingState({ icebreaker: { order: [ALICE.id, BOB.id], index: 1 } }),
      },
      [`POST /api/v1/meetings/${MEETING_ID}/icebreaker/goto`]: {
        status: 200,
        body: meetingState({ icebreaker: { order: [ALICE.id, BOB.id], index: 0 }, seq: 3 }),
      },
    });
    expect(store.previousSpeaker?.id).toBe(ALICE.id);
    const pending = store.goToPerson(ALICE.id);
    expect(store.speakerIndex).toBe(0);
    expect(await pending).toBe(true);
    expect(store.currentSpeaker?.id).toBe(ALICE.id);
    expect(store.previousSpeaker).toBeNull();
    expect(calls.at(-1)?.body).toEqual({ userId: ALICE.id });
  });

  it('shows the code of a refused or ended subscription, from the payload', async () => {
    const { store } = await opened();
    store.apply({
      type: 'error',
      channel,
      seq: null,
      payload: { code: 'meeting_not_found' },
    });
    expect(store.loadError).toBe('meeting_not_found');
    expect(store.meeting).toBeNull();
    // Later events find no room to update.
    store.apply({ type: 'meeting.renamed', channel, seq: 3, payload: { title: 'Leak' } });
    expect(store.meeting).toBeNull();
  });

  it('falls back to "unknown" for an error without a code', async () => {
    const { store } = await opened();
    store.apply({ type: 'error', channel, seq: null, payload: {} });
    expect(store.loadError).toBe('unknown');
  });

  it('does not queue an error that arrives before the snapshot', async () => {
    const store = useMeetingStore();
    useSessionStore().setUser({ ...ALICE, pictureVersion: null });
    await nextTick();
    mockFetch({ [JOIN]: { status: 200, body: meetingState() } });
    const opening = store.open(MEETING_ID);
    store.apply({ type: 'error', channel, seq: null, payload: { code: 'internal' } });
    expect(store.loadError).toBe('internal');
    await opening;
    // The snapshot that answers afterwards is dropped: the room is not live.
    expect(store.meeting).toBeNull();
    expect(store.loadError).toBe('internal');
    expect(store.loading).toBe(false);
  });
});
