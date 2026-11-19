import { flushPromises, type VueWrapper } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  ALICE,
  BOB,
  me,
  MEETING_ID,
  meetingState,
  meetingSummary,
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

const LIST = `GET /api/v1/teams/${TEAM_ID}/meetings`;
const mounted: VueWrapper[] = [];
afterEach(() => {
  for (const w of mounted.splice(0)) w.unmount();
});
beforeEach(() => mockSocket());

function routes(list: () => unknown[]) {
  return {
    'POST /api/v1/auth/refresh': session(),
    'GET /api/v1/me': me([team({ memberCount: 6 })]),
    'POST /api/v1/ws-ticket': wsTicket(),
    [LIST]: () => ({ status: 200, body: list() }),
  };
}

async function open() {
  const result = await renderAt(`/teams/${TEAM_ID}/meetings`);
  mounted.push(result.wrapper);
  return result;
}

describe('Meetings page', () => {
  it('lists active meetings as cards and ended ones as rows', async () => {
    const ended = meetingSummary({
      id: 'ended-1',
      title: 'Retro #2',
      status: 'ended',
      createdAt: '2026-10-27T10:00:00Z',
      endedAt: '2026-10-27T10:58:00Z',
      facilitator: { id: BOB.id, displayName: 'Bob Bianchi', pictureVersion: null },
      groupCount: 6,
      taskCount: 4,
    });
    mockFetch(
      routes(() => [
        meetingSummary({ participants: [participant(ALICE), participant(BOB, { online: false })] }),
        ended,
      ]),
    );
    const { wrapper } = await open();

    expect(wrapper.find('h1').text()).toBe('Meetings');
    expect(wrapper.text()).toContain('Platform Team · 6 members');
    const card = wrapper.get('article');
    expect(card.get('h3').text()).toBe('Retro #3');
    expect(card.text()).toMatch(/Started today at \d\d:\d\d/);
    expect(card.text()).toContain('Live');
    expect(card.text()).toContain('Facilitator: Alice Rossi');
    expect(card.text()).toContain('Phase: Reflect');
    expect(card.text()).toContain('2 participants');
    // alice is already in it.
    const join = card.get('a.btn--primary');
    expect(join.text()).toContain('Rejoin');
    expect(join.attributes('href')).toBe(`/teams/${TEAM_ID}/meetings/${MEETING_ID}`);

    const row = wrapper.get('section[aria-labelledby="meetings-ended"] li a');
    expect(row.attributes('href')).toBe(`/teams/${TEAM_ID}/meetings/ended-1/summary`);
    expect(row.text()).toContain('Retro #2');
    expect(row.text()).toContain('27 Oct 2026');
    expect(row.text()).toContain('58 min');
    expect(row.text()).toContain('Bob Bianchi');
    expect(row.text()).toContain('6 groups · 4 tasks');
    expect(row.text()).toContain('Summary');

    // The nav shows how many are running.
    expect(wrapper.get('nav a[aria-current="page"]').text()).toContain('1');
  });

  it("offers Join for a meeting the user isn't in, and empty states", async () => {
    mockFetch(
      routes(() => [
        meetingSummary({
          facilitator: { id: BOB.id, displayName: 'Bob Bianchi', pictureVersion: null },
          participants: [participant(BOB)],
        }),
      ]),
    );
    const { wrapper } = await open();
    expect(wrapper.get('article a.btn--primary').text()).toContain('Join');
    expect(wrapper.text()).toContain('Ended meetings will show up here.');
  });

  it("shows a teammate's new meeting live", async () => {
    let list: unknown[] = [];
    const { calls } = mockFetch(routes(() => list));
    const { wrapper } = await open();
    expect(wrapper.text()).toContain('No meeting is running.');
    const socket = await openSocket();
    expect(socket.channels).toEqual([`team:${TEAM_ID}`]);

    list = [meetingSummary({ title: 'Retro #4' })];
    vi.useFakeTimers();
    socket.emit({
      type: 'meeting.started',
      channel: `team:${TEAM_ID}`,
      seq: 1,
      payload: { meetingId: MEETING_ID },
    });
    await vi.advanceTimersByTimeAsync(200);
    vi.useRealTimers();
    await flushPromises();
    expect(calls.filter((c) => c.path === `/api/v1/teams/${TEAM_ID}/meetings`)).toHaveLength(2);
    expect(wrapper.get('article h3').text()).toBe('Retro #4');
  });

  it('shows people entering and leaving a room without reloading the list', async () => {
    const list = [
      meetingSummary({
        participants: [
          participant(ALICE, { role: 'admin', online: false }),
          participant(BOB, { online: true }),
        ],
      }),
    ];
    const { calls } = mockFetch(routes(() => list));
    const { wrapper } = await open();
    const socket = await openSocket();
    const dots = () =>
      wrapper.findAll('article .presence__dot').map((d) => d.classes('presence__dot--online'));
    expect(dots()).toEqual([false, true]);
    const listCalls = () =>
      calls.filter((c) => c.path === `/api/v1/teams/${TEAM_ID}/meetings`).length;
    expect(listCalls()).toBe(1);

    vi.useFakeTimers();
    const presence = (userId: string, online: boolean, meetingId = MEETING_ID) =>
      socket.emit({
        type: 'presence.changed',
        channel: `team:${TEAM_ID}`,
        payload: { meetingId, userId, online },
      });
    presence(ALICE.id, true);
    presence(BOB.id, false);
    // Unknown meetings and people are ignored.
    presence(BOB.id, true, '0190a4e0-0000-7000-8000-0000000000ff');
    presence('0190a4e0-0000-7000-8000-0000000000ee', true);
    await vi.advanceTimersByTimeAsync(500);
    vi.useRealTimers();
    await flushPromises();

    expect(dots()).toEqual([true, false]);
    expect(listCalls()).toBe(1);
  });

  it('Add meeting starts one and opens its room', async () => {
    const { calls } = mockFetch({
      ...routes(() => []),
      [`POST /api/v1/teams/${TEAM_ID}/meetings`]: { status: 201, body: meetingState() },
      [`POST /api/v1/teams/${TEAM_ID}/meetings/${MEETING_ID}/join`]: {
        status: 200,
        body: meetingState(),
      },
    });
    const { wrapper, router } = await open();
    const buttons = wrapper.findAll('button').filter((b) => b.text() === 'Add meeting');
    // One in the menu, one in the page header.
    expect(buttons).toHaveLength(2);
    await buttons[1]!.trigger('click');
    await flushPromises();
    expect(
      calls.some((c) => c.method === 'POST' && c.path.endsWith(`/teams/${TEAM_ID}/meetings`)),
    ).toBe(true);
    await vi.waitFor(() => expect(router.currentRoute.value.name).toBe('meeting-room'));
    expect(router.currentRoute.value.params.meetingId).toBe(MEETING_ID);
  });
});
