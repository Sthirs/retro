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
        'POST /api/v1/groups/g1/vote': {
          status: 200,
          body: voting({ myVotes: [{ groupId: 'g1', count: 1 }] }),
        },
      }),
    );
    const { wrapper } = await open();
    await add(wrapper, 'Flaky CI').trigger('click');
    await flushPromises();
    expect(calls.find((c) => c.path.endsWith('/vote'))?.body).toEqual({ delta: 1 });
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
    mockFetch(routes(ALICE, voting({ votesCast: 6 })));
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

  it('the settings dialog warns how many people lower limits touch, and saves', async () => {
    // The server counts; the client never sees anyone's usage.
    const affected: Record<string, number> = { '4/3': 2, '4/2': 2, '1/3': 3 };
    const { calls } = mockFetch(
      routes(ALICE, voting({ votesCast: 12 }), {
        [`GET ${BASE}/settings/impact`]: ({ url }: { url: URL }) => ({
          status: 200,
          body: {
            affected:
              affected[
                `${url.searchParams.get('voteBudget')}/${url.searchParams.get('voteMaxPerGroup')}`
              ] ?? 0,
          },
        }),
        [`PATCH ${BASE}/settings`]: {
          status: 200,
          // The server trimmed two votes.
          body: voting({ votesCast: 10, voteBudget: 4, voteMaxPerGroup: 3 }),
        },
      }),
    );
    const { wrapper } = await open();
    expect(wrapper.get('[data-testid="vote-cast"]').text()).toBe('12 votes cast so far');
    await wrapper.get('[data-testid="vote-settings"]').trigger('click');
    await flushPromises();
    const dialog = wrapper.get('dialog[open]');
    expect(dialog.text()).toContain('Vote settings');
    const budget = dialog.get<HTMLInputElement>('#vote-budget');
    const save = () => dialog.findAll('button').find((b) => b.text() === 'Save settings')!;
    expect(budget.element.value).toBe('5');
    expect(dialog.find('[data-testid="vote-warning"]').exists()).toBe(false);
    expect(save().attributes('disabled')).toBeUndefined();

    // A lower limit waits for the server's count before Save.
    await dialog.get('button[aria-label="Decrease: Votes per person"]').trigger('click');
    expect(budget.element.value).toBe('4');
    expect(save().attributes('disabled')).toBeDefined();
    await counted();
    expect(dialog.get('[data-testid="vote-warning"]').text()).toContain(
      '2 people have more votes than the new limits allow.',
    );
    expect(save().attributes('disabled')).toBeUndefined();

    // The warning follows typing, before the field is left.
    await budget.setValue('1');
    await counted();
    expect(dialog.get('[data-testid="vote-warning"]').text()).toContain('3 people');

    // Anything but a whole number blocks Save, rather than save a part of it.
    await budget.setValue('1x');
    await budget.trigger('blur');
    expect(budget.element.value).toBe('1x');
    expect(dialog.text()).toContain('Enter a whole number.');
    expect(save().attributes('disabled')).toBeDefined();

    // A whole number is clamped to the bounds.
    await budget.setValue('99');
    await budget.trigger('blur');
    expect(budget.element.value).toBe('20');
    await budget.setValue('4');
    await budget.trigger('blur');
    await counted();

    await save().trigger('click');
    await flushPromises();
    expect(calls.find((c) => c.method === 'PATCH')?.body).toEqual({
      voteBudget: 4,
      voteMaxPerGroup: 3,
    });
    // The room takes the server's trimmed count.
    expect(wrapper.find('dialog[open]').exists()).toBe(false);
    expect(wrapper.get('[data-testid="vote-cast"]').text()).toBe('10 votes cast so far');
  });

  it('the settings warning follows votes cast while the dialog is open', async () => {
    let affected = 0;
    const { calls } = mockFetch(
      routes(ALICE, voting({ votesCast: 4 }), {
        [`GET ${BASE}/settings/impact`]: () => ({ status: 200, body: { affected } }),
        [`PATCH ${BASE}/settings`]: {
          status: 200,
          body: voting({ votesCast: 4, voteBudget: 4 }),
        },
      }),
    );
    const { wrapper } = await open();
    const socket = await openSocket();
    await wrapper.get('[data-testid="vote-settings"]').trigger('click');
    await flushPromises();
    const dialog = wrapper.get('dialog[open]');
    const save = () => dialog.findAll('button').find((b) => b.text() === 'Save settings')!;
    await dialog.get('button[aria-label="Decrease: Votes per person"]').trigger('click');
    await counted();
    expect(dialog.find('[data-testid="vote-warning"]').exists()).toBe(false);

    // Someone votes over the draft limit: the count is asked again.
    affected = 1;
    socket.emit({
      type: 'vote.cast_changed',
      channel,
      seq: null,
      payload: { votesCast: 5, clientId: 'other-tab' },
    });
    await counted();
    expect(dialog.get('[data-testid="vote-warning"]').text()).toContain('1 person');

    // One more votes just before Save: Save shows the new count, not saves.
    affected = 2;
    await save().trigger('click');
    await flushPromises();
    expect(calls.some((c) => c.method === 'PATCH')).toBe(false);
    expect(dialog.get('[data-testid="vote-warning"]').text()).toContain('2 people');

    // Seen: the next Save saves.
    await save().trigger('click');
    await flushPromises();
    expect(calls.filter((c) => c.method === 'PATCH')).toHaveLength(1);
    expect(wrapper.find('dialog[open]').exists()).toBe(false);
  });

  it('an earlier save neither closes nor fails a reopened settings dialog', async () => {
    let release!: () => void;
    const gate = new Promise<void>((resolve) => (release = resolve));
    mockFetch(
      routes(ALICE, voting({ votesCast: 0 }), {
        [`PATCH ${BASE}/settings`]: async () => {
          await gate;
          return { status: 200, body: voting({ votesCast: 0, voteBudget: 6 }) };
        },
      }),
    );
    const { wrapper } = await open();
    await wrapper.get('[data-testid="vote-settings"]').trigger('click');
    await flushPromises();
    const dialog = wrapper.get('dialog[open]');
    await dialog.get('button[aria-label="Increase: Votes per person"]').trigger('click');
    await dialog
      .findAll('button')
      .find((b) => b.text() === 'Save settings')!
      .trigger('click');
    await dialog
      .findAll('button')
      .find((b) => b.text() === 'Cancel')!
      .trigger('click');
    await flushPromises();
    await wrapper.get('[data-testid="vote-settings"]').trigger('click');
    await flushPromises();
    release();
    await flushPromises();
    expect(wrapper.find('dialog[open]').exists()).toBe(true);
  });

  it('offers the facilitator the settings before Vote too', async () => {
    mockFetch(
      routes(
        ALICE,
        voting({
          phase: 'group',
          myVotes: [],
        }),
      ),
    );
    const { wrapper } = await open();
    expect(wrapper.get('h1').text()).toBe('Group');
    expect(wrapper.find('[data-testid="vote-settings"]').exists()).toBe(true);
  });

  it('names the vote controls of same-named groups apart', async () => {
    mockFetch(
      routes(
        BOB,
        voting({
          reflections: [
            card('d1', 'delta', 0, 'Flaky e2e tests blocked merges', 'g1'),
            card('d2', 'delta', 1, 'CI takes 25 minutes', 'g1'),
            card('i1', 'ideas', 0, 'Demo slot', 'g2'),
            card('i2', 'ideas', 1, 'Friday standup', 'g2'),
          ],
          groups: [
            { id: 'g1', column: 'delta', position: 0, name: '' },
            { id: 'g2', column: 'ideas', position: 0, name: '' },
          ],
        }),
      ),
    );
    const { wrapper } = await open();
    add(wrapper, 'Untitled group (Delta, group 1)');
    add(wrapper, 'Untitled group (Ideas, group 1)');
  });

  it('waits for the Vote snapshot before showing the ballot', async () => {
    let reads = 0;
    let release!: () => void;
    const gate = new Promise<void>((resolve) => (release = resolve));
    // Group: d1 and d2 grouped, i1 loose.
    const grouping = voting({
      phase: 'group',
      reflections: [
        card('d1', 'delta', 0, 'Flaky e2e tests blocked merges', 'g1'),
        card('d2', 'delta', 1, 'CI takes 25 minutes', 'g1'),
        { ...card('i1', 'ideas', 0, 'Add a demo slot to the Friday standup', 'g2'), groupId: null },
      ],
      groups: [{ id: 'g1', column: 'delta', position: 0, name: 'Flaky CI' }],
    });
    mockFetch(
      routes(BOB, grouping, {
        [`GET ${BASE}/state`]: async () => {
          if (++reads === 1) return { status: 200, body: grouping };
          await gate;
          return { status: 200, body: voting({ seq: 3 }) };
        },
      }),
    );
    const { wrapper } = await open();
    const socket = await openSocket();
    socket.emit({ type: 'subscribed', channel });
    await flushPromises();
    socket.emit({ type: 'phase.changed', channel, seq: 3, payload: { phase: 'vote' } });
    await flushPromises();
    expect(wrapper.get('h1').text()).toBe('Vote');
    expect(wrapper.get('[data-testid="phase-loading"]').text()).toBe('Loading the Vote phase…');
    expect(wrapper.findAll('[data-testid="vote-group"]')).toHaveLength(0);
    release();
    await flushPromises();
    expect(wrapper.find('[data-testid="phase-loading"]').exists()).toBe(false);
    expect(wrapper.findAll('[data-testid="vote-group"]')).toHaveLength(2);
  });
});

/** Lets the settings dialog ask for, and get, the count of its draft. */
async function counted(): Promise<void> {
  await new Promise((r) => setTimeout(r, 250));
  await flushPromises();
}
