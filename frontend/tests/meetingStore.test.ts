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
      payload: { index: 1, skipped: [ALICE.id] },
    });
    store.apply({
      type: 'icebreaker.person_changed',
      channel,
      seq: 4,
      payload: { index: 9, skipped: [] },
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
    expect(store.meeting!.icebreaker.skipped).toEqual([ALICE.id]);

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

  it('moves to the next person at once and rolls back when refused', async () => {
    const { store, calls } = await opened({
      [`POST /api/v1/meetings/${MEETING_ID}/icebreaker/next`]: problem(403, 'not_facilitator'),
    });
    const pending = store.nextPerson(true);
    expect(store.speakerIndex).toBe(1);
    expect(await pending).toBe(false);
    expect(store.speakerIndex).toBe(0);
    expect(store.actionError).toBe('not_facilitator');
    expect(calls.at(-1)?.body).toEqual({ skip: true });
  });
});
