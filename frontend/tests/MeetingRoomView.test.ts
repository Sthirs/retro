import { flushPromises, type VueWrapper } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  ALICE,
  BOB,
  me,
  MEETING_ID,
  meetingState,
  mockFetch,
  MockWebSocket,
  mockSocket,
  openSocket,
  participant,
  problem,
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
});
beforeEach(() => mockSocket());

function routes(state = meetingState(), extra: Record<string, unknown> = {}, user = ALICE) {
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
  return result;
}

const button = (wrapper: VueWrapper, text: string) =>
  wrapper.findAll('button').find((b) => b.text().includes(text));

describe('Meeting room', () => {
  it('shows the icebreaker to the facilitator, with the controls', async () => {
    const { calls } = mockFetch(routes());
    const { wrapper } = await open();
    expect(calls.some((c) => c.method === 'POST' && c.path === `${BASE}/join`)).toBe(true);

    expect(wrapper.text()).toContain('Retro #3');
    expect(wrapper.text()).toContain('Platform Team · 10 Nov 2026');
    expect(wrapper.text()).toContain('Alice Rossi (you)');
    expect(wrapper.get('h1').text()).toBe('Icebreaker');
    expect(wrapper.text()).toContain('Phase 1 of 5');
    expect(wrapper.get('[lang="it"]').text()).toBe('Ciao!');
    expect(wrapper.text()).toContain('/ˈtʃa.o/');
    expect(wrapper.text()).toContain('Italian');
    expect(wrapper.text()).toContain("What's a small thing that made your week better?");
    expect(wrapper.text()).toContain('Now answering · 1 of 2');
    expect(wrapper.text()).toContain('Up next: Bob Bianchi');
    expect(wrapper.get('[aria-current="step"]').text()).toContain('1/5');
    expect(
      wrapper.get('ol[aria-label="Icebreaker progress"] [aria-current="true"]').text(),
    ).toContain('Now');

    const toolbar = wrapper.get('[role="toolbar"]');
    expect(toolbar.attributes('aria-label')).toBe('Facilitator actions');
    expect(toolbar.text()).toContain('Transfer facilitator');
    expect(toolbar.text()).toContain('End meeting');
    expect(button(wrapper, 'Previous person')!.attributes('disabled')).toBeDefined();
    expect(button(wrapper, 'Next person')).toBeTruthy();
    expect(wrapper.find('button[aria-label="Rename meeting"]').exists()).toBe(true);
    const socket = await openSocket();
    expect(socket.channels).toContain(channel);
  });

  it('moves to the next person, then offers the next phase', async () => {
    const after = meetingState({
      icebreaker: { order: [ALICE.id, BOB.id], index: 1 },
      seq: 3,
    });
    const { calls } = mockFetch(
      routes(meetingState(), {
        [`POST ${BASE}/icebreaker/next`]: { status: 200, body: after },
        [`POST ${BASE}/phase/next`]: { status: 200, body: { ...after, phase: 'reflect', seq: 4 } },
      }),
    );
    const { wrapper } = await open();
    await button(wrapper, 'Next person')!.trigger('click');
    await flushPromises();
    expect(calls.some((c) => c.path === `${BASE}/icebreaker/next`)).toBe(true);
    expect(wrapper.text()).toContain('Now answering · 2 of 2');
    expect(wrapper.text()).toContain('Last one to answer');

    await button(wrapper, 'Next: Reflect')!.trigger('click');
    await flushPromises();
    expect(wrapper.get('h1').text()).toBe('Reflect');
    expect(wrapper.text()).toContain('The Reflect phase is on its way');
  });

  it.each([
    ['the previous person', (w: VueWrapper) => button(w, 'Previous person')!],
    [
      'anyone in the speaking order who had their turn',
      (w: VueWrapper) => w.get('button[aria-label="Go back to Alice Rossi"]'),
    ],
  ])('goes back to %s', async (_, control) => {
    const second = meetingState({ icebreaker: { order: [ALICE.id, BOB.id], index: 1 } });
    const first = meetingState({ icebreaker: { order: [ALICE.id, BOB.id], index: 0 }, seq: 3 });
    const { calls } = mockFetch(
      routes(second, { [`POST ${BASE}/icebreaker/goto`]: { status: 200, body: first } }),
    );
    const { wrapper } = await open();
    expect(wrapper.text()).toContain('Now answering · 2 of 2');
    // On the last person: Previous and the next phase.
    expect(button(wrapper, 'Next: Reflect')).toBeTruthy();

    await control(wrapper).trigger('click');
    await flushPromises();
    expect(calls.find((c) => c.path === `${BASE}/icebreaker/goto`)?.body).toEqual({
      userId: ALICE.id,
    });
    expect(wrapper.text()).toContain('Now answering · 1 of 2');
    expect(button(wrapper, 'Next person')).toBeTruthy();
  });

  it("follows the facilitator's changes live as a participant", async () => {
    mockFetch(routes(meetingState(), {}, BOB));
    const { wrapper } = await open();
    expect(wrapper.find('[role="toolbar"]').exists()).toBe(false);
    expect(button(wrapper, 'Next person')).toBeUndefined();
    expect(wrapper.find('button[aria-label="Rename meeting"]').exists()).toBe(false);
    expect(wrapper.text()).toContain('Bob Bianchi (you)');

    const socket = await openSocket();
    socket.emit({
      type: 'icebreaker.person_changed',
      channel,
      seq: 3,
      payload: { index: 1 },
    });
    await flushPromises();
    expect(wrapper.text()).toContain('Now answering · 2 of 2');
    expect(wrapper.find('ol[aria-label="Icebreaker progress"] [aria-label="done"]').exists()).toBe(
      true,
    );
    // Only the facilitator goes back.
    expect(wrapper.find('ol[aria-label="Icebreaker progress"] button').exists()).toBe(false);

    socket.emit({
      type: 'meeting.renamed',
      channel,
      seq: 4,
      payload: { title: 'Incident review' },
    });
    socket.emit({
      type: 'facilitator.changed',
      channel,
      seq: 5,
      payload: { facilitatorId: BOB.id },
    });
    await flushPromises();
    expect(wrapper.text()).toContain('Incident review');
    expect(wrapper.get('[role="toolbar"]').attributes('aria-label')).toBe('Facilitator actions');
    expect(wrapper.text()).toContain('You are now the facilitator.');
  });

  it('lets a team admin take over facilitation', async () => {
    const state = meetingState({
      facilitatorId: BOB.id,
      participants: [participant(BOB), participant(ALICE, { role: 'admin' })],
    });
    const { calls } = mockFetch(
      routes(state, {
        [`POST ${BASE}/facilitator`]: {
          status: 200,
          body: { ...state, facilitatorId: ALICE.id, seq: 3 },
        },
      }),
    );
    const { wrapper } = await open();
    const toolbar = wrapper.get('[role="toolbar"]');
    expect(toolbar.attributes('aria-label')).toBe('Admin actions');
    await button(wrapper, 'Take over facilitation')!.trigger('click');
    await flushPromises();
    expect(calls.find((c) => c.path === `${BASE}/facilitator`)?.body).toEqual({ userId: ALICE.id });
    expect(wrapper.get('[role="toolbar"]').attributes('aria-label')).toBe('Facilitator actions');
  });

  it('transfers the role to an online participant', async () => {
    const carol = {
      ...BOB,
      id: '0190a4e0-0000-7000-8000-000000000003',
      displayName: 'Carol Verdi',
    };
    const dave = { ...BOB, id: '0190a4e0-0000-7000-8000-000000000004', displayName: 'Dave Costa' };
    const state = meetingState({
      participants: [
        participant(ALICE, { role: 'admin' }),
        participant(BOB, { role: 'admin' }),
        participant(carol, { online: false }),
        // Removed from the team, though still in the room.
        participant(dave, { role: null }),
      ],
    });
    const { calls } = mockFetch(
      routes(state, {
        [`POST ${BASE}/facilitator`]: {
          status: 200,
          body: { ...state, facilitatorId: BOB.id, seq: 3 },
        },
      }),
    );
    const { wrapper } = await open();
    await button(wrapper, 'Transfer facilitator')!.trigger('click');
    await flushPromises();

    const dialog = wrapper.get('dialog[open]');
    expect(dialog.text()).toContain('Transfer facilitator role');
    expect(dialog.text()).toContain('You stay in the meeting as a participant.');
    const options = dialog.findAll('label');
    // Not the facilitator, not offline people, not former team members.
    expect(options).toHaveLength(1);
    expect(options[0]!.text()).toContain('Bob Bianchi');
    expect(options[0]!.text()).toContain('Admin');
    const confirm = dialog.findAll('button').at(-1)!;
    expect(confirm.text()).toBe('Make facilitator');
    expect(confirm.attributes('disabled')).toBeDefined();

    await options[0]!.get('input').setValue(true);
    expect(confirm.text()).toBe('Make Bob facilitator');
    await confirm.trigger('click');
    await flushPromises();
    expect(calls.find((c) => c.path === `${BASE}/facilitator`)?.body).toEqual({ userId: BOB.id });
    // alice is an admin: she keeps only the take-over action.
    expect(wrapper.get('[role="toolbar"]').attributes('aria-label')).toBe('Admin actions');
    expect(wrapper.find('dialog[open]').exists()).toBe(false);
  });

  it('ends the meeting after confirmation, and everyone sees it ended', async () => {
    const ended = meetingState({ status: 'ended', endedAt: '2026-11-10T18:00:00Z', seq: 3 });
    const { calls } = mockFetch(
      routes(meetingState({ taskCount: 5 }), {
        [`POST ${BASE}/end`]: { status: 200, body: ended },
      }),
    );
    const { wrapper } = await open();
    await wrapper.get('[role="toolbar"]').findAll('button').at(-1)!.trigger('click');
    await flushPromises();
    const dialog = wrapper.get('dialog[open]');
    expect(dialog.text()).toContain('End this meeting?');
    expect(dialog.text()).toContain(
      'The meeting becomes read-only and everyone goes to its summary.',
    );
    expect(dialog.text()).toContain('The meeting invite link stops working.');
    expect(dialog.get('strong').text()).toBe('5 tasks');
    expect(button(wrapper, 'Keep going')).toBeTruthy();

    await dialog.findAll('button').at(-1)!.trigger('click');
    await flushPromises();
    expect(calls.some((c) => c.method === 'POST' && c.path === `${BASE}/end`)).toBe(true);
    expect(wrapper.text()).toContain('This meeting has ended');
    expect(wrapper.text()).toContain(
      'It ran on 10 Nov 2026 for 58 min, facilitated by Alice Rossi.',
    );
  });

  it('lands participants on the ended view when the facilitator ends it', async () => {
    mockFetch(routes(meetingState(), {}, BOB));
    const { wrapper } = await open();
    const socket = await openSocket();
    socket.emit({
      type: 'meeting.ended',
      channel,
      seq: 3,
      payload: { endedAt: '2026-11-10T18:00:00Z' },
    });
    await flushPromises();
    expect(wrapper.text()).toContain('This meeting has ended');
    expect(wrapper.find('a[href$="/meetings"]').text()).toBe('Back to meetings');
  });

  it('renames the meeting inline', async () => {
    const { calls } = mockFetch(
      routes(meetingState(), {
        [`PATCH ${BASE}`]: { status: 200, body: meetingState({ title: 'Kick-off', seq: 3 }) },
      }),
    );
    const { wrapper } = await open();
    await wrapper.get('button[aria-label="Rename meeting"]').trigger('click');
    const input = wrapper.get('input#meeting-title');
    await input.setValue('  Kick-off ');
    await wrapper.get('form').trigger('submit');
    await flushPromises();
    expect(calls.find((c) => c.method === 'PATCH')?.body).toEqual({ title: 'Kick-off' });
    expect(wrapper.text()).toContain('Kick-off');
  });

  it('copies the invite link', async () => {
    const writeText = vi.fn(async () => undefined);
    vi.stubGlobal('navigator', { ...navigator, clipboard: { writeText } });
    mockFetch(
      routes(meetingState(), {
        [`GET ${BASE}/invite-link`]: { status: 200, body: { url: 'https://retro.test/m/tok' } },
      }),
    );
    const { wrapper } = await open();
    await wrapper.get('button[aria-label="Invite: copy meeting invite link"]').trigger('click');
    await flushPromises();
    expect(writeText).toHaveBeenCalledWith('https://retro.test/m/tok');
    expect(wrapper.text()).toContain('Meeting invite link copied.');
    vi.unstubAllGlobals();
  });

  it('shows no reconnecting banner while the room first connects', async () => {
    mockFetch(routes());
    const { wrapper } = await open();
    await flushPromises();
    // The socket exists but has not opened yet.
    expect(MockWebSocket.last?.readyState).toBe(MockWebSocket.CONNECTING);
    expect(wrapper.get('h1').text()).toBe('Icebreaker');
    expect(wrapper.text()).not.toContain('Reconnecting…');
    await openSocket();
    expect(wrapper.text()).not.toContain('Reconnecting…');
  });

  it('shows the banner when the first connection fails', async () => {
    mockFetch(routes(meetingState(), { 'POST /api/v1/ws-ticket': problem(503, 'unavailable') }));
    const { wrapper } = await open();
    await flushPromises();
    expect(wrapper.get('[role="status"]').text()).toContain('Reconnecting…');
  });

  it('shows the reconnecting banner while the socket is down', async () => {
    mockFetch(routes());
    const { wrapper } = await open();
    const socket = await openSocket();
    expect(wrapper.text()).not.toContain('Reconnecting…');
    socket.drop();
    await flushPromises();
    expect(wrapper.get('[role="status"]').text()).toContain('Reconnecting…');
  });

  it('explains a meeting that cannot be opened', async () => {
    mockFetch(routes(meetingState(), { [`POST ${BASE}/join`]: problem(404, 'meeting_not_found') }));
    const { wrapper } = await open();
    expect(wrapper.text()).toContain('This meeting no longer exists');
  });

  it('leaves the room when the server ends the subscription', async () => {
    mockFetch(routes());
    const { wrapper } = await open();
    const socket = await openSocket();
    expect(wrapper.get('h1').text()).toBe('Icebreaker');
    // E.g. an admin removed this user from the team (spec §11.1).
    socket.emit({ type: 'error', channel, payload: { code: 'meeting_not_found' } });
    await flushPromises();
    expect(wrapper.text()).toContain('This meeting no longer exists');
    expect(wrapper.find('[role="toolbar"]').exists()).toBe(false);
  });
});
