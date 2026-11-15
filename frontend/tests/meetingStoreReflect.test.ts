import { createPinia, setActivePinia } from 'pinia';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { nextTick } from 'vue';

import { CLIENT_ID } from '@/api/clientId';
import { useMeetingStore } from '@/stores/meeting';
import { useSessionStore } from '@/stores/session';

import { ALICE, BOB, MEETING_ID, meetingState, mockFetch, participant } from './helpers';

const BASE = `/api/v1/meetings/${MEETING_ID}`;
const channel = `meeting:${MEETING_ID}`;

async function opened(state = meetingState({ phase: 'reflect' }), routes = {}) {
  const fetch = mockFetch({
    [`POST ${BASE}/join`]: { status: 200, body: state },
    [`GET ${BASE}/state`]: { status: 200, body: state },
    ...routes,
  });
  const store = useMeetingStore();
  useSessionStore().setUser({ ...ALICE, pictureVersion: null });
  await nextTick();
  await store.open(MEETING_ID);
  return { store, ...fetch };
}

describe('meeting store in Reflect', () => {
  beforeEach(() => setActivePinia(createPinia()));
  afterEach(() => vi.useRealTimers());

  it('keeps server time from the snapshot and timer events', async () => {
    vi.useFakeTimers({ toFake: ['Date'] });
    vi.setSystemTime(new Date('2026-11-10T17:00:00Z'));
    const { store } = await opened(
      meetingState({ phase: 'reflect', serverNow: '2026-11-10T17:00:30Z' }),
    );
    expect(store.clockOffset).toBe(30_000);
    expect(store.serverTime()).toBe(Date.parse('2026-11-10T17:00:30Z'));

    const timer = { status: 'running', endsAt: '2026-11-10T17:05:00Z', durationMs: 300_000 };
    store.apply({
      type: 'timer.updated',
      channel,
      seq: 3,
      payload: { timer, serverNow: '2026-11-10T16:59:50Z' },
    });
    expect(store.meeting?.timer).toEqual(timer);
    expect(store.clockOffset).toBe(-10_000);

    store.apply({
      type: 'timer.updated',
      channel,
      seq: 4,
      payload: { timer: null, serverNow: '2026-11-10T17:00:00Z' },
    });
    expect(store.meeting?.timer).toBeNull();
  });

  it('follows ready flags, and resets them with the phase', async () => {
    const { store } = await opened();
    expect(store.hasReady).toBe(true);
    store.apply({
      type: 'ready.changed',
      channel,
      seq: 3,
      payload: { userId: BOB.id, ready: true, readyCount: 1 },
    });
    expect(store.readyCount).toBe(1);
    expect(store.iAmReady).toBe(false);
    store.apply({
      type: 'ready.changed',
      channel,
      seq: 4,
      payload: { userId: ALICE.id, ready: true, readyCount: 2 },
    });
    expect(store.iAmReady).toBe(true);
    store.apply({ type: 'phase.changed', channel, seq: 5, payload: { phase: 'group' } });
    expect(store.readyCount).toBe(0);
  });

  it('has no Ready in the icebreaker or Discuss', async () => {
    const { store } = await opened(meetingState());
    expect(store.hasReady).toBe(false);
    store.apply({ type: 'phase.changed', channel, seq: 3, payload: { phase: 'discuss' } });
    expect(store.hasReady).toBe(false);
  });

  it('applies counts and writing totals', async () => {
    const { store } = await opened();
    store.apply({
      type: 'reflection.count_changed',
      channel,
      seq: 3,
      payload: { counts: { plus: 2, delta: 1, ideas: 0, thanks: 0 } },
    });
    expect(store.meeting?.reflectionCounts.plus).toBe(2);
    const writing = {
      plus: { new: 1, edit: 0 },
      delta: { new: 0, edit: 1 },
      ideas: { new: 0, edit: 0 },
      thanks: { new: 0, edit: 0 },
    };
    // Writing is ephemeral: no seq.
    store.apply({ type: 'reflection.writing', channel, seq: null, payload: { counts: writing } });
    expect(store.meeting?.writing).toEqual(writing);
    expect(store.meeting?.seq).toBe(3);
  });

  it("re-reads the state when another tab changed one of my reflections, not for this tab's", async () => {
    const { store, calls } = await opened();
    const reads = () =>
      calls.filter((c) => c.method === 'GET' && c.path === `${BASE}/state`).length;
    store.apply({
      type: 'reflection.changed',
      channel,
      seq: null,
      payload: { id: 'r1', clientId: CLIENT_ID },
    });
    await nextTick();
    expect(reads()).toBe(0);
    store.apply({
      type: 'reflection.changed',
      channel,
      seq: null,
      payload: { id: 'r1', clientId: 'other-tab' },
    });
    await vi.waitFor(() => expect(reads()).toBe(1));
  });

  it('sends, edits and deletes reflections, taking the state from the answer', async () => {
    const mine = {
      id: 'r1',
      column: 'delta',
      content: '<p>Flaky</p>',
      mine: true,
      createdAt: '2026-11-10T17:03:00Z',
      updatedAt: '2026-11-10T17:03:00Z',
    };
    const withMine = meetingState({
      phase: 'reflect',
      seq: 3,
      reflections: [mine],
      reflectionCounts: { plus: 0, delta: 1, ideas: 0, thanks: 0 },
    });
    const { store, calls } = await opened(undefined, {
      [`POST ${BASE}/reflections`]: { status: 201, body: withMine },
      'PATCH /api/v1/reflections/r1': {
        status: 200,
        body: { ...withMine, seq: 4, reflections: [{ ...mine, column: 'ideas' }] },
      },
      'DELETE /api/v1/reflections/r1': {
        status: 200,
        body: meetingState({ phase: 'reflect', seq: 5 }),
      },
    });
    expect(await store.createReflection('delta', '<p>Flaky</p>')).toBe(true);
    expect(calls.at(-1)?.body).toEqual({ column: 'delta', content: '<p>Flaky</p>' });
    expect(store.myReflections).toHaveLength(1);

    expect(await store.updateReflection('r1', { column: 'ideas' })).toBe(true);
    expect(calls.at(-1)?.body).toEqual({ column: 'ideas' });
    expect(store.myReflections[0]?.column).toBe('ideas');

    expect(await store.deleteReflection('r1')).toBe(true);
    expect(store.myReflections).toHaveLength(0);
  });

  it('takes back an optimistic delete the server refused', async () => {
    const mine = {
      id: 'r1',
      column: 'plus',
      content: '<p>x</p>',
      mine: true,
      createdAt: '2026-11-10T17:03:00Z',
      updatedAt: '2026-11-10T17:03:00Z',
    };
    const { store } = await opened(
      meetingState({
        phase: 'reflect',
        reflections: [mine],
        reflectionCounts: { plus: 1, delta: 0, ideas: 0, thanks: 0 },
      }),
      {
        'DELETE /api/v1/reflections/r1': {
          status: 409,
          body: { status: 409, code: 'wrong_phase' },
        },
      },
    );
    expect(await store.deleteReflection('r1')).toBe(false);
    expect(store.actionError).toBe('wrong_phase');
    expect(store.myReflections).toHaveLength(1);
    expect(store.meeting?.reflectionCounts.plus).toBe(1);
  });

  it('sets the timer and ready through the API', async () => {
    const ready = meetingState({
      phase: 'reflect',
      seq: 3,
      participants: [participant(ALICE, { role: 'admin', ready: true }), participant(BOB)],
    });
    const { store, calls } = await opened(undefined, {
      [`PUT ${BASE}/timer`]: { status: 200, body: meetingState({ phase: 'reflect', seq: 3 }) },
      [`PUT ${BASE}/ready`]: { status: 200, body: ready },
    });
    await store.setTimer({ action: 'start', durationSeconds: 300 });
    expect(calls.at(-1)?.body).toEqual({ action: 'start', durationSeconds: 300 });
    await store.setReady(true);
    expect(calls.at(-1)?.body).toEqual({ ready: true });
    expect(store.iAmReady).toBe(true);
  });
});
