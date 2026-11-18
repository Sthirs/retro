import { flushPromises, type VueWrapper } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  ALICE,
  BOB,
  me,
  MEETING_ID,
  meetingState,
  mockFetch,
  mockSocket,
  openSocket,
  participant,
  renderAt,
  session,
  team,
  TEAM_ID,
  wsTicket,
} from './helpers';

const BASE = `/api/v1/meetings/${MEETING_ID}`;
const channel = `meeting:${MEETING_ID}`;
const mounted: VueWrapper[] = [];
afterEach(() => {
  for (const w of mounted.splice(0)) w.unmount();
  vi.restoreAllMocks();
  localStorage.clear();
  sessionStorage.clear();
});
beforeEach(() => mockSocket());

const card = (id: string, column: string, position: number, text: string, groupId: string) => ({
  id,
  column,
  content: `<p>${text}</p>`,
  mine: false,
  groupId,
  position,
  version: 0,
  createdAt: '2026-11-10T17:03:00Z',
  updatedAt: '2026-11-10T17:03:00Z',
});

/** Bob's view (a participant), unless `user` says otherwise. */
const voting = (overrides: Record<string, unknown> = {}) =>
  meetingState({
    phase: 'vote',
    participants: [participant(ALICE, { role: 'admin' }), participant(BOB)],
    reflections: [
      card('d1', 'delta', 0, 'Flaky e2e tests blocked merges', 'g1'),
      card('d2', 'delta', 1, 'CI takes 25 minutes', 'g1'),
      card('i1', 'ideas', 0, 'Add a demo slot to the Friday standup', 'g2'),
    ],
    groups: [
      { id: 'g1', column: 'delta', position: 0, name: 'Flaky CI' },
      { id: 'g2', column: 'ideas', position: 0, name: '' },
    ],
    ...overrides,
  });

function routes(user = BOB, state = voting(), extra: Record<string, unknown> = {}) {
  return {
    'POST /api/v1/auth/refresh': session(user),
    'GET /api/v1/me': me([team({ role: user === ALICE ? 'admin' : 'member' })], [], user),
    'POST /api/v1/ws-ticket': wsTicket(),
    [`GET /api/v1/teams/${TEAM_ID}/meetings`]: { status: 200, body: [] },
    [`POST ${BASE}/join`]: { status: 200, body: state },
    [`GET ${BASE}/state`]: { status: 200, body: state },
    ...extra,
  } as Parameters<typeof mockFetch>[0];
}

async function open() {
  const result = await renderAt(`/teams/${TEAM_ID}/meetings/${MEETING_ID}`);
  mounted.push(result.wrapper);
  await flushPromises();
  return result;
}

const group = (wrapper: VueWrapper, name: string) =>
  wrapper.get(`[data-testid="vote-group"][aria-label="${name}"]`);
const add = (wrapper: VueWrapper, name: string) =>
  wrapper.get(`button[aria-label="Add a vote to ${name}"]`);
const remove = (wrapper: VueWrapper, name: string) =>
  wrapper.get(`button[aria-label="Remove a vote from ${name}"]`);

describe('Vote phase', () => {
  it('shows the groups, single cards as cards, and the votes left', async () => {
    mockFetch(routes());
    const { wrapper } = await open();

    expect(wrapper.get('h1').text()).toBe('Vote');
    expect(wrapper.get('[data-testid="vote-left"]').text()).toBe('5 of 5 votes left');
    expect(wrapper.text()).toContain('Up to 3 on one group');
    const flaky = group(wrapper, 'Flaky CI');
    expect(flaky.text()).toContain('2 cards');
    expect(flaky.text()).toContain('Flaky e2e tests blocked merges');
    // The never-grouped card shows as itself, named by its text.
    const demo = group(wrapper, 'Add a demo slot to the Friday standup');
    expect(demo.text()).toContain('Ideas');
    expect(demo.find('[data-testid="vote-group-toggle"]').exists()).toBe(false);
    // No totals and no cast count for a participant.
    expect(wrapper.find('[data-testid="vote-cast"]').exists()).toBe(false);
    expect(wrapper.find('[data-testid="vote-settings"]').exists()).toBe(false);
  });

  it('shows only the viewer’s own votes, with + and − limits', async () => {
    mockFetch(routes(BOB, voting({ myVotes: [{ groupId: 'g1', count: 3 }] })));
    const { wrapper } = await open();
    const stepper = group(wrapper, 'Flaky CI').get('[data-testid="vote-stepper"]');
    expect(stepper.attributes('aria-label')).toBe('Your votes on Flaky CI: 3 of 3');
    expect(stepper.get('[data-testid="vote-count"]').text()).toBe('3');
    expect(add(wrapper, 'Flaky CI').attributes('disabled')).toBeDefined();
    expect(remove(wrapper, 'Flaky CI').attributes('disabled')).toBeUndefined();
    const demo = 'Add a demo slot to the Friday standup';
    expect(remove(wrapper, demo).attributes('disabled')).toBeDefined();
    expect(add(wrapper, demo).attributes('disabled')).toBeUndefined();
    expect(wrapper.get('[data-testid="vote-left"]').text()).toBe('2 of 5 votes left');
  });

  it('disables every + once the budget is spent', async () => {
    mockFetch(
      routes(
        BOB,
        voting({
          voteBudget: 4,
          myVotes: [
            { groupId: 'g1', count: 2 },
            { groupId: 'g2', count: 2 },
          ],
        }),
      ),
    );
    const { wrapper } = await open();
    expect(wrapper.get('[data-testid="vote-left"]').text()).toBe('0 of 4 votes left');
    for (const b of wrapper.findAll('[data-testid="vote-add"]')) {
      expect(b.attributes('disabled')).toBeDefined();
    }
  });

  it('votes with + and sends the change', async () => {
    const { calls } = mockFetch(
      routes(BOB, voting(), {
        'PUT /api/v1/groups/g1/vote': {
          status: 200,
          body: voting({ myVotes: [{ groupId: 'g1', count: 1 }] }),
        },
      }),
    );
    const { wrapper } = await open();
    await add(wrapper, 'Flaky CI').trigger('click');
    await flushPromises();
    expect(calls.find((c) => c.method === 'PUT')?.body).toEqual({ delta: 1 });
    expect(group(wrapper, 'Flaky CI').get('[data-testid="vote-count"]').text()).toBe('1');
    expect(wrapper.get('[data-testid="vote-left"]').text()).toBe('4 of 5 votes left');
  });

  it('expands a group locally to read its cards, and votes from there', async () => {
    mockFetch(routes());
    const { wrapper } = await open();
    const flaky = group(wrapper, 'Flaky CI');
    expect(flaky.findAll('[data-testid="vote-card"]')).toHaveLength(0);
    await flaky.get('[data-testid="vote-group-toggle"]').trigger('click');
    expect(flaky.findAll('[data-testid="vote-card"]')).toHaveLength(2);
    expect(flaky.text()).toContain('CI takes 25 minutes');
    expect(flaky.findAll('[data-testid="vote-stepper"]')).toHaveLength(1);
    await flaky.get('[data-testid="vote-group-toggle"]').trigger('click');
    expect(flaky.findAll('[data-testid="vote-card"]')).toHaveLength(0);
  });

  it('shows the facilitator the cast count and the Settings button', async () => {
    mockFetch(
      routes(
        ALICE,
        voting({
          voteUsage: [
            { spent: 5, maxOnGroup: 3 },
            { spent: 1, maxOnGroup: 1 },
          ],
        }),
      ),
    );
    const { wrapper } = await open();
    expect(wrapper.get('[data-testid="vote-cast"]').text()).toBe('6 votes cast so far');
    expect(wrapper.find('[data-testid="vote-settings"]').exists()).toBe(true);
  });

  it('updates the counter when the settings change for everyone', async () => {
    mockFetch(routes());
    const { wrapper } = await open();
    const socket = await openSocket();
    socket.emit({
      type: 'vote.settings_changed',
      channel,
      seq: 3,
      payload: { budget: 7, maxPerGroup: 2, clientId: null },
    });
    await flushPromises();
    expect(wrapper.get('[data-testid="vote-left"]').text()).toBe('7 of 7 votes left');
    expect(wrapper.text()).toContain('Up to 2 on one group');
  });

  it('the settings dialog warns how many people a lower limit trims, and saves', async () => {
    const usage = [
      { spent: 5, maxOnGroup: 3 },
      { spent: 5, maxOnGroup: 2 },
      { spent: 2, maxOnGroup: 1 },
    ];
    const { calls } = mockFetch(
      routes(ALICE, voting({ voteUsage: usage }), {
        [`PATCH ${BASE}/settings`]: {
          status: 200,
          body: voting({ voteUsage: usage, voteBudget: 4, voteMaxPerGroup: 3 }),
        },
      }),
    );
    const { wrapper } = await open();
    await wrapper.get('[data-testid="vote-settings"]').trigger('click');
    await flushPromises();
    const dialog = wrapper.get('dialog[open]');
    expect(dialog.text()).toContain('Vote settings');
    const budget = dialog.get<HTMLInputElement>('#vote-budget');
    expect(budget.element.value).toBe('5');
    expect(dialog.find('[data-testid="vote-warning"]').exists()).toBe(false);

    await dialog.get('button[aria-label="Decrease: Votes per person"]').trigger('click');
    expect(budget.element.value).toBe('4');
    expect(dialog.get('[data-testid="vote-warning"]').text()).toContain(
      '2 people have already used more votes than the new budget of 4.',
    );
    await dialog.get('button[aria-label="Decrease: Max votes per group"]').trigger('click');
    expect(dialog.get('[data-testid="vote-warning"]').text()).toContain(
      '1 person gave one group more votes than the new maximum of 2.',
    );
    await dialog.get('button[aria-label="Increase: Max votes per group"]').trigger('click');

    // The warning follows typing, before the field is left (so Save never
    // moves under the pointer as it is clicked).
    await budget.setValue('1');
    expect(dialog.get('[data-testid="vote-warning"]').text()).toContain(
      '3 people have already used more votes than the new budget of 1.',
    );

    // A typed value is clamped to the bounds.
    await budget.setValue('99');
    await budget.trigger('blur');
    expect(budget.element.value).toBe('20');
    await budget.setValue('4');
    await budget.trigger('blur');

    const save = dialog.findAll('button').find((b) => b.text() === 'Save settings')!;
    await save.trigger('click');
    await flushPromises();
    expect(calls.find((c) => c.method === 'PATCH')?.body).toEqual({
      voteBudget: 4,
      voteMaxPerGroup: 3,
    });
  });
});
