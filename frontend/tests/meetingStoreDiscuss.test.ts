import { createPinia, setActivePinia } from 'pinia';
import { beforeEach, describe, expect, it } from 'vitest';
import { nextTick } from 'vue';

import { CLIENT_ID } from '@/api/clientId';
import { useMeetingStore } from '@/stores/meeting';
import { useSessionStore } from '@/stores/session';

import { ALICE, MEETING_ID, meetingState, mockFetch, problem } from './helpers';

const BASE = `/api/v1/meetings/${MEETING_ID}`;
const channel = `meeting:${MEETING_ID}`;
const tick = () => new Promise((r) => setTimeout(r, 0));

/** A response that waits until `open()` is called. */
function gated<T>(value: T) {
  let open!: () => void;
  const gate = new Promise<void>((resolve) => (open = resolve));
  return { open, route: async () => (await gate, value) };
}

const card = (id: string, groupId: string) => ({
  id,
  column: 'delta',
  content: `<p>${id}</p>`,
  mine: false,
  groupId,
  position: 0,
  version: 0,
  createdAt: '2026-11-10T17:03:00Z',
  updatedAt: '2026-11-10T17:03:00Z',
});

const discussing = (overrides: Record<string, unknown> = {}) =>
  meetingState({
    phase: 'discuss',
    reflections: [card('c1', 'g1'), card('c2', 'g2'), card('c3', 'g3')],
    groups: [
      { id: 'g1', column: 'delta', position: 0, name: 'Flaky CI' },
      { id: 'g2', column: 'delta', position: 1, name: 'On-call' },
      { id: 'g3', column: 'delta', position: 2, name: '' },
    ],
    discussOrder: [
      { groupId: 'g1', votes: 9, cards: 1 },
      { groupId: 'g2', votes: 7, cards: 1 },
      { groupId: 'g3', votes: 0, cards: 1 },
    ],
    discussGroupId: 'g1',
    reactions: [{ reflectionId: 'c1', emoji: '👍', count: 2, mine: false }],
    ...overrides,
  });

async function opened(state = discussing(), routes = {}) {
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

const reactionPath = (card: string, emoji: string) =>
  `/api/v1/reflections/${card}/reactions/${encodeURIComponent(emoji)}`;

describe('meeting store in Discuss', () => {
  beforeEach(() => setActivePinia(createPinia()));

  it('follows the current group and previews others locally', async () => {
    const { store } = await opened();
    expect(store.currentGroupId).toBe('g1');
    expect(store.viewingGroupId).toBe('g1');
    expect(store.currentIndex).toBe(0);
    expect(store.previewing).toBe(false);

    store.viewGroup('g3');
    expect(store.viewingGroupId).toBe('g3');
    expect(store.viewingIndex).toBe(2);
    expect(store.previewing).toBe(true);
    expect(store.currentGroupId).toBe('g1');

    // Opening the current group ends the preview.
    store.viewGroup('g1');
    expect(store.previewing).toBe(false);
    store.viewGroup('g2');
    store.followCurrent();
    expect(store.viewingGroupId).toBe('g1');
  });

  it('ignores a preview of a group that is not in the order', async () => {
    const { store } = await opened();
    store.viewGroup('gone');
    expect(store.viewingGroupId).toBe('g1');
    expect(store.previewing).toBe(false);
  });

  it('jumps everyone to the new current group, ending previews', async () => {
    const { store } = await opened();
    store.viewGroup('g3');
    store.apply({
      type: 'discuss.group_changed',
      channel,
      seq: 3,
      payload: { groupId: 'g2', clientId: null },
    });
    expect(store.currentGroupId).toBe('g2');
    expect(store.viewingGroupId).toBe('g2');
    expect(store.previewing).toBe(false);
  });

  it('ends a preview when a reload finds the group moved', async () => {
    const { store } = await opened(discussing(), {
      [`GET ${BASE}/state`]: { status: 200, body: discussing({ discussGroupId: 'g2', seq: 5 }) },
    });
    store.viewGroup('g3');
    await store.resync();
    expect(store.currentGroupId).toBe('g2');
    expect(store.previewing).toBe(false);
  });

  it('keeps a preview when a reload finds the same group', async () => {
    const { store } = await opened();
    store.viewGroup('g3');
    await store.resync();
    expect(store.viewingGroupId).toBe('g3');
  });

  it('moves everyone at once when the facilitator picks a group', async () => {
    const gate = gated({ status: 200, body: discussing({ discussGroupId: 'g3', seq: 3 }) });
    const { store, calls } = await opened(discussing(), {
      [`PUT ${BASE}/discuss/current`]: gate.route,
    });
    store.viewGroup('g3');
    const done = store.setCurrentGroup('g3');
    expect(store.currentGroupId).toBe('g3');
    expect(store.previewing).toBe(false);
    gate.open();
    expect(await done).toBe(true);
    expect(calls.find((c) => c.method === 'PUT')?.body).toEqual({ groupId: 'g3' });
  });

  it('puts the current group back when the move is refused', async () => {
    const { store } = await opened(discussing(), {
      [`PUT ${BASE}/discuss/current`]: problem(403, 'not_facilitator'),
    });
    expect(await store.setCurrentGroup('g2')).toBe(false);
    expect(store.currentGroupId).toBe('g1');
    expect(store.actionError).toBe('not_facilitator');
  });

  it('toggles a reaction at once and takes the count from the answer', async () => {
    const gate = gated({
      status: 200,
      body: { reflectionId: 'c1', emoji: '👍', count: 3, mine: true },
    });
    const { store, calls } = await opened(discussing(), {
      [`PUT ${reactionPath('c1', '👍')}`]: gate.route,
    });
    const done = store.toggleReaction('c1', '👍');
    expect(store.reactionsByCard.get('c1')).toEqual([
      { reflectionId: 'c1', emoji: '👍', count: 3, mine: true },
    ]);
    gate.open();
    expect(await done).toBe(true);
    expect(store.reactionsByCard.get('c1')?.[0]).toMatchObject({ count: 3, mine: true });
    expect(calls.some((c) => c.method === 'PUT' && c.path === reactionPath('c1', '👍'))).toBe(true);
  });

  it('takes a reaction back, and drops an emoji nobody has left', async () => {
    const state = discussing({
      reactions: [{ reflectionId: 'c1', emoji: '🎉', count: 1, mine: true }],
    });
    const { store } = await opened(state, {
      [`DELETE ${reactionPath('c1', '🎉')}`]: {
        status: 200,
        body: { reflectionId: 'c1', emoji: '🎉', count: 0, mine: false },
      },
    });
    expect(await store.toggleReaction('c1', '🎉')).toBe(true);
    expect(store.reactionsByCard.get('c1')).toBeUndefined();
  });

  it('rolls a refused reaction back', async () => {
    const { store } = await opened(discussing(), {
      [`PUT ${reactionPath('c1', '🚀')}`]: problem(409, 'not_current_group'),
    });
    expect(await store.toggleReaction('c1', '🚀')).toBe(false);
    expect(store.reactionsByCard.get('c1')).toEqual([
      { reflectionId: 'c1', emoji: '👍', count: 2, mine: false },
    ]);
    expect(store.actionError).toBe('not_current_group');
  });

  it('takes counts from events, except while its own toggle is on its way', async () => {
    const gate = gated({
      status: 200,
      body: { reflectionId: 'c1', emoji: '👍', count: 3, mine: true },
    });
    const { store } = await opened(discussing(), {
      [`PUT ${reactionPath('c1', '👍')}`]: gate.route,
    });
    store.apply({
      type: 'reaction.changed',
      channel,
      seq: 3,
      payload: { reflectionId: 'c2', emoji: '💯', count: 1, clientId: null },
    });
    expect(store.reactionsByCard.get('c2')).toEqual([
      { reflectionId: 'c2', emoji: '💯', count: 1, mine: false },
    ]);

    const done = store.toggleReaction('c1', '👍');
    // Someone else's reaction lands first: the toggle's answer is older.
    store.apply({
      type: 'reaction.changed',
      channel,
      seq: 4,
      payload: { reflectionId: 'c1', emoji: '👍', count: 4, clientId: null },
    });
    expect(store.reactionsByCard.get('c1')?.[0]?.count).toBe(3);
    gate.open();
    await done;
    // The answer only says the reaction is ours; the event's count stays.
    expect(store.reactionsByCard.get('c1')?.[0]).toMatchObject({ count: 3, mine: true });
    store.apply({
      type: 'reaction.changed',
      channel,
      seq: 5,
      payload: { reflectionId: 'c1', emoji: '👍', count: 4, clientId: null },
    });
    expect(store.reactionsByCard.get('c1')?.[0]).toMatchObject({ count: 4, mine: true });
  });

  it("learns about another tab's reaction", async () => {
    const { store } = await opened();
    store.apply({
      type: 'reaction.changed',
      channel,
      seq: 3,
      payload: { reflectionId: 'c1', emoji: '👍', count: 3, clientId: 'other-tab' },
    });
    store.apply({
      type: 'reaction.mine_changed',
      channel,
      seq: null,
      payload: { reflectionId: 'c1', emoji: '👍', mine: true, clientId: 'other-tab' },
    });
    expect(store.reactionsByCard.get('c1')?.[0]).toMatchObject({ count: 3, mine: true });
    // Its own echo changes nothing.
    store.apply({
      type: 'reaction.mine_changed',
      channel,
      seq: null,
      payload: { reflectionId: 'c1', emoji: '👍', mine: false, clientId: CLIENT_ID },
    });
    expect(store.reactionsByCard.get('c1')?.[0]?.mine).toBe(true);
  });

  it("re-reads the state on someone else's task, not on its own", async () => {
    const { store, calls } = await opened(discussing(), {
      [`GET ${BASE}/state`]: { status: 200, body: discussing({ seq: 4 }) },
    });
    const reads = () => calls.filter((c) => c.method === 'GET' && c.path.endsWith('/state')).length;
    const before = reads();
    store.apply({
      type: 'task.created',
      channel,
      seq: 3,
      payload: { taskId: 't1', status: 'active', clientId: CLIENT_ID },
    });
    await tick();
    expect(reads()).toBe(before);
    store.apply({
      type: 'task.created',
      channel,
      seq: 4,
      payload: { taskId: 't2', status: 'active', clientId: 'other-tab' },
    });
    await tick();
    expect(reads()).toBe(before + 1);
  });

  it('writes a task for a group and takes the state from the answer', async () => {
    const task = {
      id: 't1',
      status: 'active',
      position: 'a0',
      contentHtml: '<p>Fix CI</p>',
      author: { id: ALICE.id, displayName: ALICE.displayName, pictureVersion: null },
      mentions: [],
      meetingId: MEETING_ID,
      groupId: 'g1',
      createdAt: '2026-11-10T18:41:00Z',
    };
    const { store, calls } = await opened(discussing(), {
      [`POST ${BASE}/tasks`]: { status: 201, body: discussing({ tasks: [task], seq: 3 }) },
    });
    expect(await store.createTask('g1', '<p>Fix CI</p>')).toBe(true);
    expect(store.meeting?.tasks).toEqual([task]);
    expect(calls.find((c) => c.path === `${BASE}/tasks`)?.body).toEqual({
      groupId: 'g1',
      content: '<p>Fix CI</p>',
    });
  });
});
