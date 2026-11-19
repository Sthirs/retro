import { createPinia, setActivePinia } from 'pinia';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { nextTick } from 'vue';

import { CLIENT_ID } from '@/api/clientId';
import { TASK_RESYNC_DELAY_MS, useMeetingStore, VOTE_TIMEOUT_MS } from '@/stores/meeting';
import { useSessionStore } from '@/stores/session';

import {
  ALICE,
  BOB,
  MEETING_ID,
  meetingState,
  mockFetch,
  participant,
  problem,
  TEAM_ID,
} from './helpers';

const BASE = `/api/v1/meetings/${MEETING_ID}`;
const channel = `meeting:${MEETING_ID}`;
const tick = () => new Promise((r) => setTimeout(r, 0));
/** Waits until task events coming close together are re-read. */
const settled = () => new Promise((r) => setTimeout(r, TASK_RESYNC_DELAY_MS + 20));

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
    [`POST /api/v1/teams/${TEAM_ID}/meetings/${MEETING_ID}/join`]: { status: 200, body: state },
    [`GET ${BASE}/state`]: { status: 200, body: state },
    ...routes,
  });
  const store = useMeetingStore();
  useSessionStore().setUser({ ...ALICE, pictureVersion: null });
  await nextTick();
  await store.open(MEETING_ID, TEAM_ID);
  return { store, ...fetch };
}

const reactionPath = (card: string, emoji: string) =>
  `/api/v1/reflections/${card}/reactions/${encodeURIComponent(emoji)}`;

describe('meeting store in Discuss', () => {
  beforeEach(() => setActivePinia(createPinia()));
  afterEach(() => vi.useRealTimers());

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
      body: { reflectionId: 'c1', emoji: '👍', count: 3, mine: true, seq: 3 },
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
        body: { reflectionId: 'c1', emoji: '🎉', count: 0, mine: false, seq: 3 },
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

  it('takes counts from events, and while a toggle is on its way, the newest by seq', async () => {
    const gate = gated({
      status: 200,
      body: { reflectionId: 'c1', emoji: '👍', count: 3, mine: true, seq: 4 },
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
    await tick();
    // The toggle's own echo, then someone else's reaction: newer than the answer.
    store.apply({
      type: 'reaction.changed',
      channel,
      seq: 4,
      payload: { reflectionId: 'c1', emoji: '👍', count: 3, clientId: CLIENT_ID },
    });
    store.apply({
      type: 'reaction.changed',
      channel,
      seq: 5,
      payload: { reflectionId: 'c1', emoji: '👍', count: 4, clientId: null },
    });
    // Someone else's reaction shows at once; the echo already holds this toggle.
    expect(store.reactionsByCard.get('c1')?.[0]).toMatchObject({ count: 4, mine: true });
    gate.open();
    await done;
    // The answer only says the reaction is ours; the event's count stays.
    expect(store.reactionsByCard.get('c1')?.[0]).toMatchObject({ count: 4, mine: true });
  });

  it('takes the answer over an event that came before the toggle echo', async () => {
    const gate = gated({
      status: 200,
      body: { reflectionId: 'c1', emoji: '👍', count: 4, mine: true, seq: 4 },
    });
    const { store } = await opened(discussing(), {
      [`PUT ${reactionPath('c1', '👍')}`]: gate.route,
    });
    const done = store.toggleReaction('c1', '👍');
    await tick();
    // Someone else reacted just before this toggle committed: older.
    store.apply({
      type: 'reaction.changed',
      channel,
      seq: 3,
      payload: { reflectionId: 'c1', emoji: '👍', count: 3, clientId: null },
    });
    gate.open();
    await done;
    expect(store.reactionsByCard.get('c1')?.[0]).toMatchObject({ count: 4, mine: true });
  });

  it('sends toggles of one emoji one at a time, in click order', async () => {
    const put = gated({
      status: 200,
      body: { reflectionId: 'c1', emoji: '👍', count: 3, mine: true, seq: 3 },
    });
    const del = gated({
      status: 200,
      body: { reflectionId: 'c1', emoji: '👍', count: 2, mine: false, seq: 4 },
    });
    const { store, calls } = await opened(discussing(), {
      [`PUT ${reactionPath('c1', '👍')}`]: put.route,
      [`DELETE ${reactionPath('c1', '👍')}`]: del.route,
    });
    const sent = () => calls.filter((c) => c.path.includes('/reactions/')).map((c) => c.method);
    const first = store.toggleReaction('c1', '👍');
    const second = store.toggleReaction('c1', '👍');
    expect(store.reactionsByCard.get('c1')?.[0]).toMatchObject({ count: 2, mine: false });
    await tick();
    // The take-back waits for the add: it cannot overtake it.
    expect(sent()).toEqual(['PUT']);
    del.open();
    put.open();
    expect(await first).toBe(true);
    // The add is answered; the take-back still shows on top of it.
    expect(store.reactionsByCard.get('c1')?.[0]).toMatchObject({ count: 2, mine: false });
    expect(await second).toBe(true);
    expect(sent()).toEqual(['PUT', 'DELETE']);
    expect(store.reactionsByCard.get('c1')?.[0]).toMatchObject({ count: 2, mine: false });
  });

  it('keeps later clicks when an earlier toggle is refused', async () => {
    const { store } = await opened(discussing(), {
      [`PUT ${reactionPath('c1', '🚀')}`]: problem(409, 'not_current_group'),
      [`DELETE ${reactionPath('c1', '🚀')}`]: {
        status: 200,
        body: { reflectionId: 'c1', emoji: '🚀', count: 0, mine: false, seq: 2 },
      },
    });
    const first = store.toggleReaction('c1', '🚀');
    const second = store.toggleReaction('c1', '🚀');
    expect(await first).toBe(false);
    expect(await second).toBe(true);
    expect(store.reactionsByCard.get('c1')).toEqual([
      { reflectionId: 'c1', emoji: '👍', count: 2, mine: false },
    ]);
  });

  it('keeps its own reaction when a reload that predates it comes back late', async () => {
    const reload = gated({ status: 200, body: discussing() });
    const { store } = await opened(discussing(), {
      [`GET ${BASE}/state`]: reload.route,
      [`PUT ${reactionPath('c1', '🚀')}`]: {
        status: 200,
        body: { reflectionId: 'c1', emoji: '🚀', count: 1, mine: true, seq: 3 },
      },
    });
    const resyncing = store.resync();
    expect(await store.toggleReaction('c1', '🚀')).toBe(true);
    store.apply({
      type: 'reaction.changed',
      channel,
      seq: 3,
      payload: { reflectionId: 'c1', emoji: '🚀', count: 1, clientId: CLIENT_ID },
    });
    store.apply({
      type: 'reaction.mine_changed',
      channel,
      seq: null,
      payload: { reflectionId: 'c1', emoji: '🚀', mine: true, clientId: CLIENT_ID },
    });
    reload.open();
    await resyncing;
    expect(store.reactionsByCard.get('c1')?.[1]).toEqual({
      reflectionId: 'c1',
      emoji: '🚀',
      count: 1,
      mine: true,
    });
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
    await settled();
    expect(reads()).toBe(before + 1);
  });

  it('re-reads the tasks when a late answer predates a task event', async () => {
    const mine = {
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
    const theirs = { ...mine, id: 't2', contentHtml: '<p>Page less</p>' };
    const answer = gated({ status: 201, body: discussing({ tasks: [mine], seq: 3 }) });
    const { store, calls } = await opened(discussing(), {
      [`POST ${BASE}/tasks`]: answer.route,
      [`GET ${BASE}/state`]: { status: 200, body: discussing({ tasks: [mine, theirs], seq: 4 }) },
    });
    const reads = () => calls.filter((c) => c.method === 'GET' && c.path.endsWith('/state')).length;
    const done = store.createTask('g1', '<p>Fix CI</p>');
    await tick();
    store.apply({
      type: 'task.created',
      channel,
      seq: 3,
      payload: { taskId: 't1', status: 'active', clientId: CLIENT_ID },
    });
    store.apply({
      type: 'task.created',
      channel,
      seq: 4,
      payload: { taskId: 't2', status: 'active', clientId: 'other-tab' },
    });
    await settled();
    expect(store.meeting?.tasks.map((t) => t.id)).toEqual(['t1', 't2']);
    const before = reads();
    answer.open();
    expect(await done).toBe(true);
    await tick();
    expect(reads()).toBe(before + 1);
    expect(store.meeting?.tasks.map((t) => t.id)).toEqual(['t1', 't2']);
  });

  it("shows someone else's count when its own toggle is refused", async () => {
    const gate = gated(problem(409, 'not_current_group'));
    const { store } = await opened(discussing(), {
      [`PUT ${reactionPath('c1', '👍')}`]: gate.route,
    });
    const done = store.toggleReaction('c1', '👍');
    await tick();
    store.apply({
      type: 'reaction.changed',
      channel,
      seq: 3,
      payload: { reflectionId: 'c1', emoji: '👍', count: 3, clientId: null },
    });
    expect(store.reactionsByCard.get('c1')?.[0]).toMatchObject({ count: 4, mine: true });
    gate.open();
    expect(await done).toBe(false);
    expect(store.reactionsByCard.get('c1')?.[0]).toMatchObject({ count: 3, mine: false });
  });

  it('ignores a late echo of the first of two quick toggles', async () => {
    const put = gated({
      status: 200,
      body: { reflectionId: 'c1', emoji: '👍', count: 3, mine: true, seq: 3 },
    });
    const del = gated({
      status: 200,
      body: { reflectionId: 'c1', emoji: '👍', count: 2, mine: false, seq: 4 },
    });
    const { store } = await opened(discussing(), {
      [`PUT ${reactionPath('c1', '👍')}`]: put.route,
      [`DELETE ${reactionPath('c1', '👍')}`]: del.route,
    });
    const first = store.toggleReaction('c1', '👍');
    const second = store.toggleReaction('c1', '👍');
    put.open();
    await first;
    await tick();
    // The add's echo comes once the take-back is on its way.
    store.apply({
      type: 'reaction.changed',
      channel,
      seq: 3,
      payload: { reflectionId: 'c1', emoji: '👍', count: 3, clientId: CLIENT_ID },
    });
    del.open();
    expect(await second).toBe(true);
    expect(store.reactionsByCard.get('c1')?.[0]).toMatchObject({ count: 2, mine: false });
  });

  it('takes a reload as confirmed while a toggle is on its way', async () => {
    const gate = gated({
      status: 200,
      body: { reflectionId: 'c1', emoji: '👍', count: 4, mine: true, seq: 4 },
    });
    const { store } = await opened(discussing(), {
      [`PUT ${reactionPath('c1', '👍')}`]: gate.route,
      // Someone else reacted meanwhile; this toggle has not landed yet.
      [`GET ${BASE}/state`]: {
        status: 200,
        body: discussing({
          seq: 3,
          reactions: [{ reflectionId: 'c1', emoji: '👍', count: 3, mine: false }],
        }),
      },
    });
    const done = store.toggleReaction('c1', '👍');
    await store.resync();
    expect(store.reactionsByCard.get('c1')?.[0]).toMatchObject({ count: 4, mine: true });
    gate.open();
    await done;
    expect(store.reactionsByCard.get('c1')?.[0]).toMatchObject({ count: 4, mine: true });
  });

  it('sends the next toggle once a hung one times out, and re-reads the state', async () => {
    const never = new Promise<never>(() => {});
    let reads = 0;
    const { store, calls } = await opened(discussing(), {
      [`PUT ${reactionPath('c1', '👍')}`]: () => never,
      [`PUT ${reactionPath('c1', '🎉')}`]: () => never,
      [`DELETE ${reactionPath('c1', '👍')}`]: {
        status: 200,
        body: { reflectionId: 'c1', emoji: '👍', count: 2, mine: false, seq: 2 },
      },
      [`GET ${BASE}/state`]: () => (++reads, { status: 200, body: discussing() }),
    });
    const sent = () => calls.filter((c) => c.path.includes('/reactions/')).map((c) => c.method);
    vi.useFakeTimers();
    const hung = store.toggleReaction('c1', '👍');
    const next = store.toggleReaction('c1', '👍');
    await vi.advanceTimersByTimeAsync(VOTE_TIMEOUT_MS - 1);
    expect(sent()).toEqual(['PUT']);
    await vi.advanceTimersByTimeAsync(1);
    await expect(hung).resolves.toBe(false);
    await expect(next).resolves.toBe(true);
    expect(sent()).toEqual(['PUT', 'DELETE']);
    expect(reads).toBe(0);

    // The last toggle lost: it may have landed or not, the state says.
    const lost = store.toggleReaction('c1', '🎉');
    await vi.advanceTimersByTimeAsync(VOTE_TIMEOUT_MS);
    await expect(lost).resolves.toBe(false);
    await vi.runOnlyPendingTimersAsync();
    expect(reads).toBe(1);
  });

  it('re-reads a burst of task changes once, and never for moves', async () => {
    const { store, calls } = await opened(discussing(), {
      [`GET ${BASE}/state`]: { status: 200, body: discussing({ seq: 10 }) },
    });
    const reads = () => calls.filter((c) => c.method === 'GET' && c.path.endsWith('/state')).length;
    const before = reads();
    for (let seq = 3; seq <= 7; seq++) {
      store.apply({
        type: 'task.moved',
        channel,
        seq,
        payload: { taskId: 't1', status: 'done', clientId: 'other-tab' },
      });
    }
    await settled();
    expect(reads()).toBe(before);
    for (let seq = 8; seq <= 10; seq++) {
      store.apply({
        type: 'task.updated',
        channel,
        seq,
        payload: { taskId: 't1', status: 'active', clientId: 'other-tab' },
      });
    }
    await settled();
    expect(reads()).toBe(before + 1);
  });

  it('drops a deleted task without re-reading', async () => {
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
    const { store, calls } = await opened(discussing({ tasks: [task, { ...task, id: 't2' }] }));
    const reads = () => calls.filter((c) => c.method === 'GET' && c.path.endsWith('/state')).length;
    const before = reads();
    store.apply({
      type: 'task.deleted',
      channel,
      seq: 3,
      payload: { taskId: 't1', status: 'active', clientId: 'other-tab' },
    });
    await settled();
    expect(store.meeting?.tasks.map((t) => t.id)).toEqual(['t2']);
    expect(reads()).toBe(before);
  });

  it('keeps unsent task text per group until it is cleared', async () => {
    const { store } = await opened();
    store.setTaskDraft('g1', '<p>Half</p>');
    store.setTaskDraft('g2', '<p>Other</p>');
    expect(store.taskDrafts).toEqual({ g1: '<p>Half</p>', g2: '<p>Other</p>' });
    store.setTaskDraft('g1', '');
    expect(store.taskDrafts).toEqual({ g2: '<p>Other</p>' });
    store.reset();
    expect(store.taskDrafts).toEqual({});
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

  describe('editing, deleting and who writes tasks', () => {
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

    it('edits a task and takes it from the answer', async () => {
      const edited = {
        ...task,
        contentHtml: '<p>Fix CI now</p>',
        editedAt: '2026-11-10T18:47:00Z',
      };
      const { store, calls } = await opened(discussing({ tasks: [task, { ...task, id: 't2' }] }), {
        'PATCH /api/v1/tasks/t1': { status: 200, body: edited },
      });
      expect(await store.updateTask('t1', '<p>Fix CI now</p>')).toBe(true);
      expect(store.meeting?.tasks.map((t) => [t.id, t.contentHtml, t.editedAt])).toEqual([
        ['t1', '<p>Fix CI now</p>', '2026-11-10T18:47:00Z'],
        ['t2', '<p>Fix CI</p>', undefined],
      ]);
      expect(calls.find((c) => c.method === 'PATCH')?.body).toEqual({
        content: '<p>Fix CI now</p>',
      });
    });

    it('keeps the task and says why when an edit is refused', async () => {
      const { store } = await opened(discussing({ tasks: [task] }), {
        'PATCH /api/v1/tasks/t1': problem(400, 'invalid_task_content'),
      });
      expect(await store.updateTask('t1', '<p></p>')).toBe(false);
      expect(store.actionError).toBe('invalid_task_content');
      expect(store.meeting?.tasks[0]?.contentHtml).toBe('<p>Fix CI</p>');
    });

    it('deletes a task, also one someone else deleted first', async () => {
      const { store } = await opened(
        discussing({ tasks: [task, { ...task, id: 't2' }, { ...task, id: 't3' }] }),
        {
          'DELETE /api/v1/tasks/t1': { status: 204 },
          'DELETE /api/v1/tasks/t2': problem(404, 'task_not_found'),
          'DELETE /api/v1/tasks/t3': problem(500, 'internal'),
        },
      );
      expect(await store.deleteTask('t1')).toBe(true);
      expect(await store.deleteTask('t2')).toBe(true);
      expect(store.meeting?.tasks.map((t) => t.id)).toEqual(['t3']);
      expect(await store.deleteTask('t3')).toBe(false);
      expect(store.meeting?.tasks.map((t) => t.id)).toEqual(['t3']);
    });

    it('knows who else writes a new task per group and who edits which task', async () => {
      const { store } = await opened(
        discussing({
          participants: [participant(ALICE), participant(BOB)],
          taskWriters: [{ userId: BOB.id, groupId: 'g1', taskId: null }],
        }),
      );
      expect(store.taskWritersByGroup.get('g1')?.map((p) => p.id)).toEqual([BOB.id]);
      store.apply({
        type: 'task.writing',
        channel,
        seq: null,
        payload: {
          writers: [
            { userId: ALICE.id, groupId: 'g1', taskId: null },
            { userId: BOB.id, groupId: null, taskId: 't1' },
            { userId: 'stranger', groupId: 'g2', taskId: null },
          ],
        },
      });
      // The viewer (Alice) and people who are not participants are left out.
      expect(store.taskWritersByGroup.size).toBe(0);
      expect(store.taskEditors.get('t1')?.id).toBe(BOB.id);
    });
  });
});
