import { flushPromises, type VueWrapper } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  ALICE,
  BOB,
  me,
  MEETING_ID,
  meetingReport,
  meetingState,
  mockFetch,
  mockSocket,
  problem,
  renderAt,
  session,
  team,
  TEAM_ID,
  wsTicket,
} from './helpers';

const SUMMARY = `/api/v1/meetings/${MEETING_ID}/summary`;
const PATH = `/teams/${TEAM_ID}/meetings/${MEETING_ID}/summary`;
const mounted: VueWrapper[] = [];
afterEach(() => {
  for (const w of mounted.splice(0)) w.unmount();
});
beforeEach(() => mockSocket());

function routes(summary: unknown = { status: 200, body: meetingReport() }) {
  return {
    'POST /api/v1/auth/refresh': session(BOB),
    'GET /api/v1/me': me([team({ role: 'member' })], [], BOB),
    'POST /api/v1/ws-ticket': wsTicket(),
    [`GET /api/v1/teams/${TEAM_ID}/meetings`]: { status: 200, body: [] },
    [`GET ${SUMMARY}`]: summary,
  } as Parameters<typeof mockFetch>[0];
}

async function open(path = PATH) {
  const result = await renderAt(path);
  mounted.push(result.wrapper);
  return result;
}

const fact = (wrapper: VueWrapper, key: string) => wrapper.get(`[data-fact="${key}"]`);
const groups = (wrapper: VueWrapper) => wrapper.findAll('[data-testid="summary-group"]');

describe('Meeting summary', () => {
  it('shows what the meeting was, inside the team shell', async () => {
    mockFetch(routes());
    const { wrapper } = await open();
    expect(wrapper.text()).toContain('Meeting summary · read-only');
    expect(wrapper.get('h1').text()).toBe('Retro #3');
    expect(wrapper.get('[data-testid="ended-at"]').text()).toMatch(/^Ended at \d\d:07$/);
    expect(fact(wrapper, 'date').text()).toBe('Date10 Nov 2026');
    expect(fact(wrapper, 'duration').get('dd').text()).toBe('1 h 5 min');
    expect(fact(wrapper, 'facilitator').get('dd').text()).toBe(ALICE.displayName);
    expect(fact(wrapper, 'votesCast').get('dd').text()).toBe('9');
    // The Meetings section stays current, and the back link goes there.
    expect(wrapper.get('nav a[aria-current="page"]').text()).toContain('Meetings');
    const back = wrapper
      .findAll('a')
      .find((a) => a.text() === 'Meetings' && !a.classes('nav__link'));
    expect(back?.attributes('href')).toBe(`/teams/${TEAM_ID}/meetings`);
  });

  it('ranks the groups, labels each card with its column, renders rich text', async () => {
    mockFetch(routes());
    const { wrapper } = await open();
    const [first, second] = groups(wrapper);
    expect(first!.text()).toContain('#1');
    expect(first!.get('h3').text()).toBe('Flaky CI is slowing us down');
    expect(first!.text()).toContain('5 votes');
    const cards = first!.findAll('li');
    expect(cards.map((c) => c.get('.card__column').text())).toEqual(['Delta', 'Ideas']);
    expect(cards[0]!.get('strong').text()).toBe('merges');
    // A never-grouped card is named after its text; one vote is singular.
    expect(second!.text()).toContain('#2');
    expect(second!.get('h3').text()).toBe('Thanks Marco for covering the incident!');
    expect(second!.text()).toContain('1 vote');
    expect(second!.text()).not.toContain('1 votes');
  });

  it('lists the tasks with their status, mentions and a link to the board', async () => {
    mockFetch(routes());
    const { wrapper } = await open();
    const task = wrapper.get('[data-testid="summary-task"]');
    expect(task.text()).toContain('Rotate the on-call buddy');
    expect(task.text()).toContain('Done');
    expect(task.get('[aria-label="Mentioned"]').text()).toBe(`@${BOB.displayName}`);
    const board = wrapper.findAll('a').find((a) => a.text().includes('Tasks board'));
    expect(board?.attributes('href')).toBe(`/teams/${TEAM_ID}/tasks?meeting=${MEETING_ID}`);
    expect(wrapper.text()).toContain('Participants · 2');
    expect(wrapper.text()).toContain(BOB.displayName);
  });

  it('says so when nothing was revealed and no task was written', async () => {
    mockFetch(
      routes({ status: 200, body: meetingReport({ groups: [], tasks: [], votesCast: 0 }) }),
    );
    const { wrapper } = await open();
    expect(groups(wrapper)).toHaveLength(0);
    expect(wrapper.text()).toContain('No reflections were revealed in this meeting.');
    expect(wrapper.text()).toContain('No tasks were created in this meeting.');
    expect(fact(wrapper, 'votesCast').get('dd').text()).toBe('0');
  });

  it('opens the room of a meeting that is still running', async () => {
    mockFetch({
      ...routes(problem(409, 'meeting_active')),
      [`POST /api/v1/teams/${TEAM_ID}/meetings/${MEETING_ID}/join`]: {
        status: 200,
        body: meetingState(),
      },
      [`GET /api/v1/meetings/${MEETING_ID}/state`]: { status: 200, body: meetingState() },
    });
    const { router } = await open();
    await vi.waitFor(() => expect(router.currentRoute.value.name).toBe('meeting-room'));
    await flushPromises();
  });

  it('does not open the room once the user has left the summary', async () => {
    let release!: () => void;
    const gate = new Promise<void>((resolve) => (release = resolve));
    mockFetch(
      routes(async () => {
        await gate;
        return problem(409, 'meeting_active');
      }),
    );
    const { router } = await open();
    await router.push({ name: 'team-meetings', params: { teamId: TEAM_ID } });
    await flushPromises();
    const replace = vi.spyOn(router, 'replace');
    release();
    await flushPromises();
    expect(replace).not.toHaveBeenCalled();
    expect(router.currentRoute.value.name).toBe('team-meetings');
  });

  it('explains a meeting that cannot be found', async () => {
    mockFetch(routes(problem(404, 'meeting_not_found')));
    const { wrapper } = await open();
    expect(wrapper.text()).toContain('Summary not available');
    expect(wrapper.text()).toContain('This meeting no longer exists.');
    expect(
      wrapper
        .findAll('a')
        .find((a) => a.text() === 'Back to meetings')
        ?.attributes('href'),
    ).toBe(`/teams/${TEAM_ID}/meetings`);
  });

  it("does not show another team's meeting under this team", async () => {
    mockFetch(routes({ status: 200, body: meetingReport({ teamId: 'another-team' }) }));
    const { wrapper } = await open();
    expect(wrapper.text()).toContain('Summary not available');
    expect(wrapper.text()).not.toContain('Flaky CI');
  });
});
