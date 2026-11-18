import { createPinia, setActivePinia } from 'pinia';
import { beforeEach, describe, expect, it } from 'vitest';
import { nextTick } from 'vue';

import { CLIENT_ID } from '@/api/clientId';
import { useMeetingStore } from '@/stores/meeting';
import { useSessionStore } from '@/stores/session';

import { ALICE, BOB, MEETING_ID, meetingState, mockFetch, problem } from './helpers';

const BASE = `/api/v1/meetings/${MEETING_ID}`;
const isVote = (c: { method: string; path: string }) =>
  c.method === 'POST' && c.path.endsWith('/vote');
const tick = () => new Promise((r) => setTimeout(r, 0));

/** A response that waits for `open()` to be called. */
function gated<T>(value: T) {
  let open!: () => void;
  const gate = new Promise<void>((resolve) => (open = resolve));
  return { open, route: async () => (await gate, value) };
}
const channel = `meeting:${MEETING_ID}`;

const card = (id: string, column: string, position: number, groupId: string) => ({
  id,
  column,
  content: `<p>${id}</p>`,
  mine: false,
  groupId,
  position,
  version: 0,
  createdAt: '2026-11-10T17:03:00Z',
  updatedAt: '2026-11-10T17:03:00Z',
});

const voting = (overrides: Record<string, unknown> = {}) =>
  meetingState({
    phase: 'vote',
    reflections: [
      card('p1', 'plus', 0, 'g1'),
      card('p2', 'plus', 1, 'g1'),
      card('d1', 'delta', 0, 'g2'),
    ],
    groups: [
      { id: 'g1', column: 'plus', position: 0, name: 'Shipping' },
      { id: 'g2', column: 'delta', position: 0, name: '' },
    ],
    ...overrides,
  });

const withVotes = (votes: [string, number][], overrides: Record<string, unknown> = {}) =>
  voting({ myVotes: votes.map(([groupId, count]) => ({ groupId, count })), ...overrides });

async function opened(state = voting(), routes = {}) {
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

describe('meeting store in Vote', () => {
  beforeEach(() => setActivePinia(createPinia()));

  it('counts the viewer’s own votes and what is left', async () => {
    const { store } = await opened(withVotes([['g1', 2]]));
    expect(store.myVotes.get('g1')).toBe(2);
    expect(store.myVotes.get('g2')).toBeUndefined();
    expect(store.votesUsed).toBe(2);
    expect(store.votesLeft).toBe(3);
    expect(store.voteBudget).toBe(5);
    expect(store.voteMaxPerGroup).toBe(3);
    // Only the facilitator gets the usage; nobody else gets a cast count.
    expect(store.votesCast).toBeNull();
  });

  it('takes the facilitator’s cast count, with their own votes on the way', async () => {
    const answer = gated({ status: 200, body: withVotes([['g1', 1]], { votesCast: 8 }) });
    const { store } = await opened(voting({ votesCast: 7 }), {
      'POST /api/v1/groups/g1/vote': answer.route,
    });
    expect(store.votesCast).toBe(7);
    const done = store.vote('g1', 1);
    expect(store.votesCast).toBe(8);
    answer.open();
    await done;
    expect(store.votesCast).toBe(8);
  });

  it('votes optimistically and takes the answer', async () => {
    let release!: () => void;
    const gate = new Promise<void>((resolve) => (release = resolve));
    const { store, calls } = await opened(voting(), {
      'POST /api/v1/groups/g1/vote': async () => {
        await gate;
        return { status: 200, body: withVotes([['g1', 1]], { seq: 2 }) };
      },
    });
    const done = store.vote('g1', 1);
    expect(store.myVotes.get('g1')).toBe(1);
    expect(store.votesLeft).toBe(4);
    release();
    await expect(done).resolves.toBe(true);
    expect(store.myVotes.get('g1')).toBe(1);
    const sent = calls.find((c) => isVote(c));
    expect(sent?.body).toEqual({ delta: 1 });
    expect(sent?.headers.get('X-Client-Id')).toBe(CLIENT_ID);
  });

  it('sends quick votes one at a time, keeping the later ones on top', async () => {
    const gates: (() => void)[] = [];
    let answered = 0;
    const { store, calls } = await opened(voting(), {
      'POST /api/v1/groups/g1/vote': async () => {
        await new Promise<void>((resolve) => gates.push(resolve));
        answered += 1;
        return { status: 200, body: withVotes([['g1', answered]]) };
      },
    });
    const first = store.vote('g1', 1);
    const second = store.vote('g1', 1);
    expect(store.myVotes.get('g1')).toBe(2);
    await new Promise((r) => setTimeout(r, 0));
    // The second waits for the first.
    expect(calls.filter((c) => isVote(c))).toHaveLength(1);
    gates.shift()!();
    await first;
    // The first answer knows one vote; the second, still on its way, shows.
    expect(store.myVotes.get('g1')).toBe(2);
    await new Promise((r) => setTimeout(r, 0));
    gates.shift()!();
    await second;
    expect(store.myVotes.get('g1')).toBe(2);
    expect(calls.filter((c) => isVote(c))).toHaveLength(2);
  });

  it('refuses locally over the budget or the group limit, and below zero', async () => {
    const { store, calls } = await opened(
      withVotes([
        ['g1', 3],
        ['g2', 2],
      ]),
    );
    await expect(store.vote('g2', 1)).resolves.toBe(false);
    await expect(store.vote('g1', 1)).resolves.toBe(false);
    const fresh = await opened(withVotes([['g1', 3]]));
    await expect(fresh.store.vote('g1', 1)).resolves.toBe(false);
    await expect(fresh.store.vote('g2', -1)).resolves.toBe(false);
    expect([...calls, ...fresh.calls].some((c) => isVote(c))).toBe(false);
  });

  it('takes a refused vote back from a fresh snapshot', async () => {
    const { store } = await opened(voting(), {
      'POST /api/v1/groups/g1/vote': problem(409, 'vote_budget_exhausted'),
    });
    await expect(store.vote('g1', 1)).resolves.toBe(false);
    expect(store.myVotes.get('g1')).toBeUndefined();
    expect(store.actionError).toBe('vote_budget_exhausted');
  });

  it('re-reads on another tab’s vote and skips its own echo', async () => {
    const { store, calls } = await opened();
    const reads = () => calls.filter((c) => c.method === 'GET').length;
    const before = reads();
    store.apply({ type: 'vote.changed', channel, seq: null, payload: { clientId: CLIENT_ID } });
    await new Promise((r) => setTimeout(r, 0));
    expect(reads()).toBe(before);
    store.apply({ type: 'vote.changed', channel, seq: null, payload: { clientId: 'other-tab' } });
    await new Promise((r) => setTimeout(r, 0));
    expect(reads()).toBe(before + 1);
  });

  it('takes new settings from everyone’s event', async () => {
    const { store } = await opened(withVotes([['g1', 2]]));
    store.apply({
      type: 'vote.settings_changed',
      channel,
      seq: 3,
      payload: { budget: 2, maxPerGroup: 1, clientId: null },
    });
    expect(store.voteBudget).toBe(2);
    expect(store.voteMaxPerGroup).toBe(1);
    expect(store.votesLeft).toBe(0);
  });

  it('re-reads when the facilitator changes during Vote', async () => {
    const { store, calls } = await opened();
    const before = calls.filter((c) => c.method === 'GET').length;
    store.apply({
      type: 'facilitator.changed',
      channel,
      seq: 3,
      payload: { facilitatorId: ALICE.id },
    });
    await new Promise((r) => setTimeout(r, 0));
    expect(calls.filter((c) => c.method === 'GET').length).toBeGreaterThan(before);
  });

  it('saves the settings and takes the answer', async () => {
    const { store, calls } = await opened(voting(), {
      [`PATCH ${BASE}/settings`]: {
        status: 200,
        body: voting({ voteBudget: 7, voteMaxPerGroup: 2 }),
      },
    });
    await expect(store.saveVoteSettings(7, 2)).resolves.toBe(true);
    expect(calls.find((c) => c.method === 'PATCH')?.body).toEqual({
      voteBudget: 7,
      voteMaxPerGroup: 2,
    });
    expect(store.voteBudget).toBe(7);
  });

  it('keeps a confirmed vote from a read that started before it', async () => {
    // The read leaves first and comes back last, without the vote.
    const read = gated({ status: 200, body: voting() });
    let reads = 0;
    const { store, calls } = await opened(voting(), {
      [`GET ${BASE}/state`]: () =>
        ++reads === 1 ? read.route() : { status: 200, body: withVotes([['g1', 1]]) },
      'POST /api/v1/groups/g1/vote': { status: 200, body: withVotes([['g1', 1]]) },
    });
    const resync = store.resync();
    await tick();
    await expect(store.vote('g1', 1)).resolves.toBe(true);
    expect(store.myVotes.get('g1')).toBe(1);
    read.open();
    await resync;
    // Its votes are unsure: not taken, and read again.
    expect(store.myVotes.get('g1')).toBe(1);
    await tick();
    expect(calls.filter((c) => c.method === 'GET')).toHaveLength(2);
    expect(store.myVotes.get('g1')).toBe(1);
  });

  it('does not add a pending vote to a read that already holds it', async () => {
    const answer = gated({ status: 200, body: withVotes([['g1', 3]]) });
    const { store } = await opened(withVotes([['g1', 2]]), {
      'POST /api/v1/groups/g1/vote': answer.route,
      // Committed already: the read sees the third vote.
      [`GET ${BASE}/state`]: { status: 200, body: withVotes([['g1', 3]]) },
    });
    const done = store.vote('g1', 1);
    await tick();
    await store.resync();
    expect(store.myVotes.get('g1')).toBe(3);
    expect(store.votesLeft).toBe(2);
    answer.open();
    await done;
    expect(store.myVotes.get('g1')).toBe(3);
  });

  it('takes a refused vote back even when the reload fails, keeping later clicks', async () => {
    const first = gated({ status: 409, body: { code: 'vote_budget_exhausted' } });
    let reads = 0;
    const { store } = await opened(voting(), {
      'POST /api/v1/groups/g1/vote': first.route,
      'POST /api/v1/groups/g2/vote': { status: 200, body: withVotes([['g2', 1]]) },
      [`GET ${BASE}/state`]: () =>
        ++reads === 1
          ? { status: 500, body: { code: 'internal' } }
          : { status: 200, body: voting() },
    });
    const refused = store.vote('g1', 1);
    const later = store.vote('g2', 1);
    expect(store.votesUsed).toBe(2);
    first.open();
    await expect(refused).resolves.toBe(false);
    expect(store.myVotes.get('g1')).toBeUndefined();
    expect(store.myVotes.get('g2')).toBe(1);
    await later;
    expect(store.myVotes.get('g2')).toBe(1);
  });

  it('drops a queued vote when another account signs in', async () => {
    const first = gated({ status: 200, body: withVotes([['g1', 1]]) });
    const { store, calls } = await opened(voting(), {
      'POST /api/v1/groups/g1/vote': first.route,
      'POST /api/v1/groups/g2/vote': { status: 200, body: withVotes([['g2', 1]]) },
    });
    const sent = store.vote('g1', 1);
    const queued = store.vote('g2', 1);
    await tick();
    useSessionStore().setUser({ ...BOB, pictureVersion: null });
    await nextTick();
    // Bob opens the same meeting before alice's answer comes back.
    await store.open(MEETING_ID);
    first.open();
    await expect(sent).resolves.toBe(false);
    await expect(queued).resolves.toBe(false);
    expect(calls.filter(isVote).map((c) => c.path)).toEqual(['/api/v1/groups/g1/vote']);
    expect(store.myVotes.size).toBe(0);
  });

  it('keeps a vote taken meanwhile when a Ready is refused', async () => {
    const ready = gated({ status: 409, body: { code: 'wrong_phase' } });
    const { store } = await opened(voting(), {
      [`PUT ${BASE}/ready`]: ready.route,
      'POST /api/v1/groups/g1/vote': { status: 200, body: withVotes([['g1', 1]]) },
    });
    const refused = store.setReady(true);
    await expect(store.vote('g1', 1)).resolves.toBe(true);
    ready.open();
    await expect(refused).resolves.toBe(false);
    expect(store.myVotes.get('g1')).toBe(1);
  });

  it('shows the cast count once the role arrives, and hides it once it leaves', async () => {
    const { store } = await opened(voting({ facilitatorId: BOB.id }), {
      // The reload after the handoff (seq 3).
      [`GET ${BASE}/state`]: { status: 200, body: voting({ votesCast: 4, seq: 3 }) },
    });
    expect(store.votesCast).toBeNull();
    store.apply({
      type: 'facilitator.changed',
      channel,
      seq: 3,
      payload: { facilitatorId: ALICE.id },
    });
    await tick();
    expect(store.isFacilitator).toBe(true);
    expect(store.votesCast).toBe(4);
    store.apply({
      type: 'facilitator.changed',
      channel,
      seq: 4,
      payload: { facilitatorId: BOB.id },
    });
    // At once, before any reload.
    expect(store.votesCast).toBeNull();
  });

  it('reloads after a settings save whose answer was lost, not after a refusal', async () => {
    let status = 500;
    const { store, calls } = await opened(voting(), {
      [`PATCH ${BASE}/settings`]: () => ({
        status,
        body: { code: status === 500 ? 'internal' : 'invalid_vote_settings' },
      }),
    });
    const reads = () => calls.filter((c) => c.method === 'GET').length;
    let before = reads();
    await expect(store.saveVoteSettings(4, 2)).resolves.toBe(false);
    await tick();
    expect(reads()).toBe(before + 1);
    status = 400;
    before = reads();
    await expect(store.saveVoteSettings(0, 2)).resolves.toBe(false);
    await tick();
    expect(reads()).toBe(before);
  });

  it('asks the server how many people new limits would touch', async () => {
    const { store, calls } = await opened(voting(), {
      [`GET ${BASE}/settings/impact`]: ({ url }: { url: URL }) => ({
        status: 200,
        body: { affected: Number(url.searchParams.get('voteBudget')) < 3 ? 2 : 0 },
      }),
    });
    await expect(store.voteImpact(2, 3)).resolves.toBe(2);
    await expect(store.voteImpact(4, 3)).resolves.toBe(0);
    expect(calls.at(-1)?.path).toBe(`${BASE}/settings/impact?voteBudget=4&voteMaxPerGroup=3`);
  });
});
