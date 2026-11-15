import { flushPromises, type VueWrapper } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import RichTextEditor from '@/components/RichTextEditor.vue';

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
  vi.useRealTimers();
  localStorage.clear();
  sessionStorage.clear();
});
beforeEach(() => mockSocket());

const SERVER_NOW = '2026-11-10T17:10:00Z';
const at = (seconds: number) => new Date(Date.parse(SERVER_NOW) + seconds * 1000).toISOString();

const mine = (overrides: Record<string, unknown> = {}) => ({
  id: 'r1',
  column: 'delta',
  content: '<p>Flaky <strong>e2e</strong> tests</p>',
  mine: true,
  createdAt: '2026-11-10T17:03:00Z',
  updatedAt: '2026-11-10T17:03:00Z',
  ...overrides,
});

const reflect = (overrides: Record<string, unknown> = {}) =>
  meetingState({ phase: 'reflect', serverNow: SERVER_NOW, ...overrides });

function routes(state = reflect(), extra: Record<string, unknown> = {}, user = ALICE) {
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
const column = (wrapper: VueWrapper, name: string) => wrapper.get(`section[aria-label="${name}"]`);

describe('Reflect phase', () => {
  it('shows four columns, my reflections and only counts of the others', async () => {
    mockFetch(
      routes(
        reflect({
          reflections: [mine()],
          reflectionCounts: { plus: 4, delta: 7, ideas: 0, thanks: 0 },
          writing: {
            plus: { new: 0, edit: 0 },
            delta: { new: 1, edit: 0 },
            ideas: { new: 0, edit: 0 },
            thanks: { new: 0, edit: 1 },
          },
        }),
      ),
    );
    const { wrapper } = await open();
    expect(wrapper.get('h1').text()).toBe('Reflect');
    expect(wrapper.text()).toContain("Others only see how many you've written.");
    for (const [name, hint] of [
      ['Plus', 'What went well'],
      ['Delta', 'What to change'],
      ['Ideas', 'What to try'],
      ['Thanks', 'Who to thank'],
    ]) {
      expect(column(wrapper, name!).text()).toContain(hint);
    }
    const delta = column(wrapper, 'Delta');
    expect(delta.text()).toContain('1 mine');
    expect(delta.text()).toContain('Flaky e2e tests');
    expect(delta.find('strong').text()).toBe('e2e');
    expect(delta.text()).toContain('Sent · yours');
    expect(delta.get('[aria-live="polite"]').text()).toBe('6 from the team · 1 being written');
    // Six sent, plus one being written.
    expect(delta.findAll('[data-testid="team-strip"] span[class*="mini"]')).toHaveLength(7);
    expect(delta.findAll('[data-writing]')).toHaveLength(1);

    expect(column(wrapper, 'Plus').get('[aria-live="polite"]').text()).toBe('4 from the team');
    expect(column(wrapper, 'Ideas').get('[aria-live="polite"]').text()).toBe(
      'No reflections from the team yet',
    );
    // An edit in a column with nothing sent by others still shows as being written.
    expect(column(wrapper, 'Thanks').get('[aria-live="polite"]').text()).toContain(
      '1 being written',
    );
    expect(column(wrapper, 'Plus').text()).not.toContain('mine');
  });

  it('sends a reflection from the composer, then shows it as mine', async () => {
    const sent = reflect({
      seq: 3,
      reflections: [mine({ content: '<p>Pairing helped</p>', column: 'plus' })],
      reflectionCounts: { plus: 1, delta: 0, ideas: 0, thanks: 0 },
    });
    const { calls } = mockFetch(
      routes(reflect(), { [`POST ${BASE}/reflections`]: { status: 201, body: sent } }),
    );
    const { wrapper } = await open();
    const socket = await openSocket();
    socket.emit({ type: 'subscribed', channel });
    await flushPromises();

    await wrapper.get('button[aria-label="Write a reflection in Plus"]').trigger('click');
    await flushPromises();
    const editor = wrapper.findComponent(RichTextEditor);
    expect(editor.props('kind')).toBe('reflection');
    expect(editor.props('placeholder')).toBe('What went well…');
    expect(wrapper.text()).toContain('Ctrl + Enter to send');
    expect(socket.sent.at(-1)).toEqual({ type: 'writing', channel, column: 'plus', mode: 'new' });
    // Nothing to send yet.
    expect(button(wrapper, 'Send')!.attributes('disabled')).toBeDefined();

    editor.vm.$emit('update:modelValue', '<p>Pairing helped</p>');
    await flushPromises();
    expect(wrapper.get('[data-testid="unsent-drafts"]').text()).toBe('1 unsent draft');
    await button(wrapper, 'Send')!.trigger('click');
    await flushPromises();

    const post = calls.find((c) => c.method === 'POST' && c.path === `${BASE}/reflections`);
    expect(post?.body).toEqual({ column: 'plus', content: '<p>Pairing helped</p>' });
    expect(wrapper.findComponent(RichTextEditor).exists()).toBe(false);
    expect(column(wrapper, 'Plus').text()).toContain('Pairing helped');
    expect(column(wrapper, 'Plus').text()).toContain('1 mine');
    expect(wrapper.find('[data-testid="unsent-drafts"]').exists()).toBe(false);
    expect(socket.sent.at(-1)).toEqual({ type: 'writing', channel, column: null });
  });

  it('submits on Ctrl + Enter and cancels on Esc through the editor', async () => {
    const { calls } = mockFetch(
      routes(reflect(), {
        [`POST ${BASE}/reflections`]: { status: 201, body: reflect({ seq: 3 }) },
      }),
    );
    const { wrapper } = await open();
    await wrapper.get('button[aria-label="Write a reflection in Ideas"]').trigger('click');
    await flushPromises();
    let editor = wrapper.findComponent(RichTextEditor);
    editor.vm.$emit('update:modelValue', '<p>Demo slot</p>');
    editor.vm.$emit('submit');
    await flushPromises();
    expect(calls.some((c) => c.path === `${BASE}/reflections`)).toBe(true);

    await wrapper.get('button[aria-label="Write a reflection in Ideas"]').trigger('click');
    await flushPromises();
    editor = wrapper.findComponent(RichTextEditor);
    editor.vm.$emit('update:modelValue', '<p>Thrown away</p>');
    editor.vm.$emit('cancel');
    await flushPromises();
    expect(wrapper.findComponent(RichTextEditor).exists()).toBe(false);
    expect(wrapper.find('[data-testid="unsent-drafts"]').exists()).toBe(false);
  });

  it("does not count this tab's own editor as someone else writing", async () => {
    mockFetch(routes());
    const { wrapper } = await open();
    const socket = await openSocket();
    socket.emit({ type: 'subscribed', channel });
    await flushPromises();
    await wrapper.get('button[aria-label="Write a reflection in Delta"]').trigger('click');
    socket.emit({
      type: 'reflection.writing',
      channel,
      payload: {
        counts: {
          plus: { new: 0, edit: 0 },
          delta: { new: 2, edit: 0 },
          ideas: { new: 0, edit: 0 },
          thanks: { new: 0, edit: 0 },
        },
      },
    });
    await flushPromises();
    expect(column(wrapper, 'Delta').get('[aria-live="polite"]').text()).toBe(
      'No reflections from the team yet · 1 being written',
    );
  });

  it('edits, moves and deletes one of my reflections', async () => {
    const state = reflect({
      reflections: [mine()],
      reflectionCounts: { plus: 0, delta: 1, ideas: 0, thanks: 0 },
    });
    const moved = reflect({
      seq: 3,
      reflections: [mine({ column: 'ideas', content: '<p>Rewritten</p>' })],
      reflectionCounts: { plus: 0, delta: 0, ideas: 1, thanks: 0 },
    });
    const { calls } = mockFetch(
      routes(state, {
        'PATCH /api/v1/reflections/r1': { status: 200, body: moved },
        'DELETE /api/v1/reflections/r1': { status: 200, body: reflect({ seq: 4 }) },
      }),
    );
    const { wrapper } = await open();
    await wrapper.get('button[aria-label="Edit reflection"]').trigger('click');
    await flushPromises();
    const editor = wrapper.findComponent(RichTextEditor);
    expect(editor.props('modelValue')).toBe('<p>Flaky <strong>e2e</strong> tests</p>');
    editor.vm.$emit('update:modelValue', '<p>Rewritten</p>');
    await wrapper.get('select').setValue('ideas');
    await button(wrapper, 'Save')!.trigger('click');
    await flushPromises();
    const patch = calls.find((c) => c.method === 'PATCH');
    expect(patch?.body).toEqual({ content: '<p>Rewritten</p>', column: 'ideas' });
    expect(column(wrapper, 'Ideas').text()).toContain('Rewritten');
    expect(column(wrapper, 'Delta').text()).not.toContain('Rewritten');

    await column(wrapper, 'Ideas').get('button[aria-label="Edit reflection"]').trigger('click');
    await flushPromises();
    await button(wrapper, 'Delete')!.trigger('click');
    await flushPromises();
    expect(calls.some((c) => c.method === 'DELETE' && c.path === '/api/v1/reflections/r1')).toBe(
      true,
    );
    expect(wrapper.findAll('[data-testid="my-reflection"]')).toHaveLength(0);
  });

  it('keeps an unsent draft across a reload', async () => {
    localStorage.setItem(
      `retro.reflectDrafts.${MEETING_ID}.${ALICE.id}`,
      JSON.stringify({ thanks: '<p>Thanks Bob</p>' }),
    );
    mockFetch(routes());
    const { wrapper } = await open();
    expect(wrapper.get('[data-testid="unsent-drafts"]').text()).toBe('1 unsent draft');
    await wrapper.get('button[aria-label="Write a reflection in Thanks"]').trigger('click');
    await flushPromises();
    expect(wrapper.findComponent(RichTextEditor).props('modelValue')).toBe('<p>Thanks Bob</p>');
  });
});

describe('Bottom bar', () => {
  it('lets the facilitator set a timer from the presets or a custom duration', async () => {
    const running = reflect({
      seq: 3,
      timer: { status: 'running', endsAt: at(600), durationMs: 600_000 },
    });
    const { calls } = mockFetch(
      routes(reflect(), { [`PUT ${BASE}/timer`]: { status: 200, body: running } }),
    );
    const { wrapper } = await open();
    const trigger = button(wrapper, 'Set timer')!;
    expect(trigger.attributes('aria-expanded')).toBe('false');
    await trigger.trigger('click');
    const dialog = wrapper.get('[role="dialog"]');
    expect(dialog.get('h2').text()).toBe('Timer');
    expect(dialog.get('button[aria-pressed="true"]').text()).toBe('5 min');
    expect(dialog.text()).toContain(
      "Everyone sees the same countdown. It won't move the phase on by itself.",
    );

    // Not a duration: says so, sends nothing.
    await dialog.get('input').setValue('7:75');
    await dialog.get('form').trigger('submit');
    expect(dialog.get('[role="alert"]').text()).toContain('00:01 and 99:59');
    expect(calls.some((c) => c.path === `${BASE}/timer`)).toBe(false);

    await button(wrapper, '10 min')!.trigger('click');
    expect((dialog.get('input').element as HTMLInputElement).value).toBe('10:00');
    await dialog.get('form').trigger('submit');
    await flushPromises();
    expect(calls.find((c) => c.path === `${BASE}/timer`)?.body).toEqual({
      action: 'start',
      durationSeconds: 600,
    });
    expect(wrapper.find('[role="dialog"]').exists()).toBe(false);
    expect(wrapper.get('[role="timer"]').text()).toBe('10:00');
  });

  it('counts down on server time and drives pause, +1 min and stop', async () => {
    vi.useFakeTimers({ toFake: ['Date', 'setInterval', 'clearInterval'] });
    // The local clock is 5 minutes behind the server.
    vi.setSystemTime(Date.parse(SERVER_NOW) - 300_000);
    const state = reflect({ timer: { status: 'running', endsAt: at(272), durationMs: 300_000 } });
    const paused = reflect({
      seq: 3,
      timer: { status: 'paused', remainingMs: 270_000, durationMs: 300_000 },
    });
    const { calls } = mockFetch(
      routes(state, { [`PUT ${BASE}/timer`]: { status: 200, body: paused } }),
    );
    const { wrapper } = await open();
    expect(wrapper.get('[role="timer"]').text()).toBe('04:32');
    vi.advanceTimersByTime(2000);
    await flushPromises();
    expect(wrapper.get('[role="timer"]').text()).toBe('04:30');

    await wrapper.get('button[aria-label="Pause timer"]').trigger('click');
    await flushPromises();
    expect(calls.at(-1)?.body).toEqual({ action: 'pause' });
    expect(wrapper.get('[data-testid="timer"]').text()).toContain('paused');
    expect(wrapper.find('button[aria-label="Resume timer"]').exists()).toBe(true);

    await wrapper.get('button[aria-label="Add one minute"]').trigger('click');
    await flushPromises();
    expect(calls.at(-1)?.body).toEqual({ action: 'add', seconds: 60 });
    await wrapper.get('button[aria-label="Stop timer"]').trigger('click');
    await flushPromises();
    expect(calls.at(-1)?.body).toEqual({ action: 'stop' });
  });

  it('shows the facilitator the ready count and who is still writing', async () => {
    mockFetch(
      routes(
        reflect({
          participants: [participant(ALICE, { role: 'admin' }), participant(BOB, { ready: true })],
        }),
      ),
    );
    const { wrapper } = await open();
    const ready = wrapper.get('[data-testid="ready-count"]');
    expect(ready.text()).toContain('1/2');
    expect(ready.text()).toContain('ready');
    expect(ready.attributes('title')).toBe('Waiting for Alice');
    expect(button(wrapper, 'Next: Group')).toBeTruthy();
    expect(button(wrapper, "I'm ready")!.attributes('aria-pressed')).toBe('false');
  });

  it('shows a participant the countdown, who facilitates and their Ready', async () => {
    const state = reflect({ timer: { status: 'running', endsAt: at(90), durationMs: 120_000 } });
    const ready = reflect({
      seq: 3,
      participants: [participant(ALICE, { role: 'admin' }), participant(BOB, { ready: true })],
      timer: { status: 'running', endsAt: at(90), durationMs: 120_000 },
    });
    const { calls } = mockFetch(
      routes(state, { [`PUT ${BASE}/ready`]: { status: 200, body: ready } }, BOB),
    );
    const { wrapper } = await open();
    const bar = wrapper.get('footer');
    expect(bar.text()).toContain('01:30');
    expect(bar.text()).toContain('left');
    expect(bar.text()).toContain("Alice is facilitating. Press Ready when you're done.");
    expect(bar.find('[data-testid="ready-count"]').exists()).toBe(false);
    expect(button(wrapper, 'Set timer')).toBeUndefined();
    expect(button(wrapper, 'Next: Group')).toBeUndefined();

    await button(wrapper, "I'm ready")!.trigger('click');
    await flushPromises();
    expect(calls.at(-1)?.body).toEqual({ ready: true });
    expect(button(wrapper, "You're ready")!.attributes('aria-pressed')).toBe('true');
  });

  it("tells everyone time's up, until dismissed", async () => {
    vi.useFakeTimers({ toFake: ['Date', 'setInterval', 'clearInterval'] });
    vi.setSystemTime(Date.parse(SERVER_NOW));
    const state = reflect({ timer: { status: 'running', endsAt: at(2), durationMs: 120_000 } });
    mockFetch(routes(state, {}, BOB));
    const { wrapper } = await open();
    expect(wrapper.find('[data-testid="times-up"]').exists()).toBe(false);
    vi.advanceTimersByTime(2500);
    await flushPromises();
    const toast = wrapper.get('[data-testid="times-up"]');
    expect(toast.text()).toBe("Time's up. Alice decides when to move on.");
    // It does not move the phase on.
    expect(wrapper.get('h1').text()).toBe('Reflect');
    await toast.get('button[aria-label="Dismiss"]').trigger('click');
    expect(wrapper.find('[data-testid="times-up"]').exists()).toBe(false);
  });

  it("tells the facilitator it's their call", async () => {
    vi.useFakeTimers({ toFake: ['Date', 'setInterval', 'clearInterval'] });
    vi.setSystemTime(Date.parse(SERVER_NOW));
    mockFetch(
      routes(reflect({ timer: { status: 'running', endsAt: at(-5), durationMs: 60_000 } })),
    );
    const { wrapper } = await open();
    expect(wrapper.get('[data-testid="times-up"]').text()).toBe(
      "Time's up. You decide when to move on.",
    );
    expect(wrapper.get('[role="timer"]').text()).toBe('00:00');
  });

  it('says when unsent drafts are discarded as the meeting moves on', async () => {
    localStorage.setItem(
      `retro.reflectDrafts.${MEETING_ID}.${BOB.id}`,
      JSON.stringify({ delta: '<p>Half a thought</p>' }),
    );
    mockFetch(routes(reflect(), {}, BOB));
    const { wrapper } = await open();
    const socket = await openSocket();
    socket.emit({ type: 'subscribed', channel });
    await flushPromises();
    socket.emit({ type: 'phase.changed', channel, seq: 3, payload: { phase: 'group' } });
    await flushPromises();
    expect(wrapper.text()).toContain('The meeting moved on: your unsent draft was discarded.');
    expect(wrapper.find('[data-testid="unsent-drafts"]').exists()).toBe(false);
  });
});
