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
  vi.restoreAllMocks();
  localStorage.clear();
  sessionStorage.clear();
});
beforeEach(() => mockSocket());

const card = (id: string, column: string, text: string, groupId: string) => ({
  id,
  column,
  content: `<p>${text}</p>`,
  mine: false,
  groupId,
  position: 0,
  version: 0,
  createdAt: '2026-11-10T17:03:00Z',
  updatedAt: '2026-11-10T17:03:00Z',
});

const task = {
  id: 't1',
  status: 'active',
  position: 'a0',
  contentHtml: '<p>Quarantine flaky tests</p>',
  author: { id: BOB.id, displayName: BOB.displayName, pictureVersion: null },
  mentions: [],
  meetingId: MEETING_ID,
  groupId: 'g1',
  createdAt: '2026-11-10T18:41:00Z',
};

const discussing = (overrides: Record<string, unknown> = {}) =>
  meetingState({
    phase: 'discuss',
    participants: [participant(ALICE, { role: 'admin' }), participant(BOB)],
    reflections: [
      card('c1', 'delta', 'Flaky e2e tests blocked merges', 'g1'),
      card('c2', 'thanks', 'Thanks Marco for the incident', 'g2'),
      card('c3', 'ideas', 'Add a demo slot', 'g3'),
    ],
    groups: [
      { id: 'g1', column: 'delta', position: 0, name: 'Flaky CI' },
      { id: 'g2', column: 'thanks', position: 0, name: 'On-call' },
      { id: 'g3', column: 'ideas', position: 0, name: '' },
    ],
    discussOrder: [
      { groupId: 'g1', votes: 9, cards: 1 },
      { groupId: 'g2', votes: 7, cards: 1 },
      { groupId: 'g3', votes: 1, cards: 1 },
    ],
    discussGroupId: 'g1',
    reactions: [
      { reflectionId: 'c1', emoji: '👍', count: 4, mine: true },
      { reflectionId: 'c1', emoji: '😬', count: 2, mine: false },
    ],
    tasks: [task],
    ...overrides,
  });

const reactionPath = (cardId: string, emoji: string) =>
  `/api/v1/reflections/${cardId}/reactions/${encodeURIComponent(emoji)}`;

function routes(user = BOB, state = discussing(), extra: Record<string, unknown> = {}) {
  return {
    'POST /api/v1/auth/refresh': session(user),
    'GET /api/v1/me': me([team({ role: user === ALICE ? 'admin' : 'member' })], [], user),
    'POST /api/v1/ws-ticket': wsTicket(),
    [`GET /api/v1/teams/${TEAM_ID}/meetings`]: { status: 200, body: [] },
    [`GET /api/v1/teams/${TEAM_ID}/members`]: { status: 200, body: [] },
    [`POST /api/v1/teams/${TEAM_ID}/meetings/${MEETING_ID}/join`]: { status: 200, body: state },
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

const entries = (wrapper: VueWrapper) => wrapper.findAll('[data-testid="discuss-entry"]');
const entry = (wrapper: VueWrapper, name: string) =>
  entries(wrapper).find((e) => e.text().includes(name))!;
const groupTitle = (wrapper: VueWrapper) => wrapper.get('[data-testid="discuss-group"] h2').text();
const footer = (wrapper: VueWrapper) => wrapper.get('footer');
const button = (wrapper: VueWrapper, text: string) =>
  wrapper.findAll('button').find((b) => b.text().trim() === text);

describe('Discuss phase', () => {
  it('ranks the groups and shows the current one with its cards and reactions', async () => {
    mockFetch(routes());
    const { wrapper } = await open();

    expect(wrapper.get('h1').text()).toBe('Discuss');
    const list = entries(wrapper);
    expect(list.map((e) => e.find('.progress__votes').text())).toEqual(['9', '7', '1']);
    expect(list[0]!.text()).toContain('Flaky CI');
    expect(list[0]!.text()).toContain('Now');
    expect(list[1]!.text()).toContain('Next');
    // A never-grouped card is named by its text.
    expect(list[2]!.text()).toContain('Add a demo slot');

    expect(groupTitle(wrapper)).toBe('Flaky CI');
    const group = wrapper.get('[data-testid="discuss-group"]');
    expect(group.text()).toContain('Group 1 of 3');
    expect(group.get('[data-testid="discuss-votes"]').text()).toBe('9 votes');
    expect(group.get('[data-testid="discuss-card"]').text()).toContain('Flaky e2e tests');
    expect(group.get('[role="img"]').attributes('aria-label')).toBe('Delta');

    const chips = group.findAll('[data-testid="reaction-chip"]');
    expect(chips.map((c) => c.attributes('aria-label'))).toEqual([
      '👍 reaction, 4',
      '😬 reaction, 2',
    ]);
    expect(chips[0]!.attributes('aria-pressed')).toBe('true');
    expect(chips[1]!.attributes('aria-pressed')).toBe('false');

    const tasks = wrapper.get('[data-testid="discuss-tasks"]');
    expect(tasks.text()).toContain('Quarantine flaky tests');
    expect(tasks.text()).toContain('Bob Bianchi');
    expect(footer(wrapper).text()).toContain('Group 1 of 3 · everyone follows the facilitator');
    // A participant cannot move the group.
    expect(button(wrapper, 'Next group')).toBeUndefined();
  });

  it('lets a participant preview another group, read-only, and come back', async () => {
    mockFetch(routes());
    const { wrapper } = await open();

    await entry(wrapper, 'Add a demo slot').trigger('click');
    expect(groupTitle(wrapper)).toBe('Add a demo slot');
    expect(wrapper.get('[data-testid="discuss-group"]').text()).toContain('Group 3 of 3');
    const banner = wrapper.get('[data-testid="discuss-preview"]');
    expect(banner.text()).toContain('Everyone else is on group 1');
    // Read-only: no new reactions, no composer, the group's own tasks.
    expect(wrapper.find('[data-testid="add-reaction"]').exists()).toBe(false);
    expect(wrapper.findComponent(RichTextEditor).exists()).toBe(false);
    expect(wrapper.get('[data-testid="discuss-tasks"]').text()).toContain('No tasks');
    expect(button(wrapper, 'Discuss this group')).toBeUndefined();
    // The others still see "Now" on the current group.
    expect(entry(wrapper, 'Flaky CI').text()).toContain('Now');

    await button(wrapper, 'Back to current group')!.trigger('click');
    expect(groupTitle(wrapper)).toBe('Flaky CI');
    expect(wrapper.find('[data-testid="discuss-preview"]').exists()).toBe(false);
    expect(wrapper.find('[data-testid="add-reaction"]').exists()).toBe(true);
  });

  it('follows the facilitator to the new group, even from a preview', async () => {
    mockFetch(routes());
    const { wrapper } = await open();
    await entry(wrapper, 'Add a demo slot').trigger('click');

    const socket = await openSocket();
    socket.emit({
      type: 'discuss.group_changed',
      channel,
      seq: 3,
      payload: { groupId: 'g2', clientId: null },
    });
    await flushPromises();
    expect(groupTitle(wrapper)).toBe('On-call');
    expect(wrapper.find('[data-testid="discuss-preview"]').exists()).toBe(false);
    expect(entry(wrapper, 'Flaky CI').find('[aria-label="done"]').exists()).toBe(true);
    expect(entry(wrapper, 'On-call').text()).toContain('Now');
    expect(footer(wrapper).text()).toContain('Group 2 of 3');
  });

  it('moves everyone with Next and Previous group, then offers End meeting', async () => {
    const { calls } = mockFetch(
      routes(ALICE, discussing(), {
        [`PUT ${BASE}/discuss/current`]: ({ body }: { body: unknown }) => ({
          status: 200,
          body: discussing({ discussGroupId: (body as { groupId: string }).groupId, seq: 3 }),
        }),
      }),
    );
    const { wrapper } = await open();
    expect(button(wrapper, 'Previous group')!.attributes('disabled')).toBeDefined();

    await button(wrapper, 'Next group')!.trigger('click');
    await flushPromises();
    expect(calls.filter((c) => c.method === 'PUT').map((c) => c.body)).toEqual([{ groupId: 'g2' }]);
    expect(groupTitle(wrapper)).toBe('On-call');

    await button(wrapper, 'Next group')!.trigger('click');
    await flushPromises();
    expect(groupTitle(wrapper)).toBe('Add a demo slot');
    expect(button(wrapper, 'Next group')).toBeUndefined();
    expect(
      footer(wrapper)
        .findAll('button')
        .some((b) => b.text().includes('End meeting')),
    ).toBe(true);

    await button(wrapper, 'Previous group')!.trigger('click');
    await flushPromises();
    expect(groupTitle(wrapper)).toBe('On-call');
  });

  it('lets the facilitator discuss the group they preview', async () => {
    const { calls } = mockFetch(
      routes(ALICE, discussing(), {
        [`PUT ${BASE}/discuss/current`]: {
          status: 200,
          body: discussing({ discussGroupId: 'g3', seq: 3 }),
        },
      }),
    );
    const { wrapper } = await open();
    await entry(wrapper, 'Add a demo slot').trigger('click');
    await button(wrapper, 'Discuss this group')!.trigger('click');
    await flushPromises();
    expect(calls.find((c) => c.method === 'PUT')?.body).toEqual({ groupId: 'g3' });
    expect(wrapper.find('[data-testid="discuss-preview"]').exists()).toBe(false);
    expect(entry(wrapper, 'Add a demo slot').text()).toContain('Now');
  });

  it('toggles a reaction from its chip', async () => {
    const { calls } = mockFetch(
      routes(BOB, discussing(), {
        [`PUT ${reactionPath('c1', '😬')}`]: {
          status: 200,
          body: { reflectionId: 'c1', emoji: '😬', count: 3, mine: true, seq: 3 },
        },
        [`DELETE ${reactionPath('c1', '👍')}`]: {
          status: 200,
          body: { reflectionId: 'c1', emoji: '👍', count: 3, mine: false, seq: 4 },
        },
      }),
    );
    const { wrapper } = await open();
    const chip = (emoji: string) =>
      wrapper.get(`[data-testid="reaction-chip"][data-emoji="${emoji}"]`);

    await chip('😬').trigger('click');
    await flushPromises();
    expect(chip('😬').attributes('aria-label')).toBe('😬 reaction, 3');
    expect(chip('😬').attributes('aria-pressed')).toBe('true');
    await chip('👍').trigger('click');
    await flushPromises();
    expect(chip('👍').attributes('aria-pressed')).toBe('false');
    expect(calls.filter((c) => c.path.includes('/reactions/')).map((c) => c.method)).toEqual([
      'PUT',
      'DELETE',
    ]);
    // Adding counts as a use of the emoji, taking back does not.
    const usage = JSON.parse(localStorage.getItem(`retro.emojiUsage.${BOB.id}`)!);
    expect(usage.counts).toEqual({ '😬': 1 });
  });

  it('shows a refused reaction as an error and takes it back', async () => {
    mockFetch(
      routes(BOB, discussing(), {
        [`PUT ${reactionPath('c1', '😬')}`]: problem(409, 'not_current_group'),
      }),
    );
    const { wrapper } = await open();
    await wrapper.get('[data-testid="reaction-chip"][data-emoji="😬"]').trigger('click');
    await flushPromises();
    expect(wrapper.text()).toContain('The facilitator moved on to another group');
    expect(
      wrapper.get('[data-testid="reaction-chip"][data-emoji="😬"]').attributes('aria-label'),
    ).toBe('😬 reaction, 2');
  });

  it('reacts from the picker, whose first row is the most used emoji', async () => {
    localStorage.setItem(
      `retro.emojiUsage.${BOB.id}`,
      JSON.stringify({ counts: { '🚀': 5, '🦄': 2 }, recent: ['🦄', '🚀'] }),
    );
    const { calls } = mockFetch(
      routes(BOB, discussing(), {
        [`PUT ${reactionPath('c1', '🚀')}`]: {
          status: 200,
          body: { reflectionId: 'c1', emoji: '🚀', count: 1, mine: true, seq: 3 },
        },
      }),
    );
    const { wrapper } = await open();
    const trigger = wrapper.get('[data-testid="add-reaction"]');
    await trigger.trigger('click');
    await flushPromises();
    const picker = wrapper.get('[data-testid="emoji-picker"]');
    expect(picker.attributes('role')).toBe('dialog');
    expect(trigger.attributes('aria-expanded')).toBe('true');
    const quick = picker.findAll('[data-testid="quick-reaction"]');
    expect(quick.map((q) => q.text())).toEqual(['🚀', '🦄', '👍', '❤️', '😂', '🎉', '😮', '😬']);
    // The viewer's own reactions on this card are pressed.
    expect(quick[2]!.attributes('aria-pressed')).toBe('true');
    expect(quick[0]!.attributes('aria-pressed')).toBe('false');
    // Category tabs are labelled stroke icons, not emoji.
    const tabs = picker.findAll('[role="tab"]');
    expect(tabs).toHaveLength(7);
    for (const tab of tabs) {
      expect(tab.find('svg').exists()).toBe(true);
      expect(tab.text()).toBe('');
      expect(tab.attributes('aria-label')).toBeTruthy();
    }

    await quick[0]!.trigger('click');
    await flushPromises();
    expect(wrapper.find('[data-testid="emoji-picker"]').exists()).toBe(false);
    expect(calls.some((c) => c.method === 'PUT' && c.path === reactionPath('c1', '🚀'))).toBe(true);
    expect(
      wrapper.get('[data-testid="reaction-chip"][data-emoji="🚀"]').attributes('aria-pressed'),
    ).toBe('true');
    const usage = JSON.parse(localStorage.getItem(`retro.emojiUsage.${BOB.id}`)!);
    expect(usage.counts['🚀']).toBe(6);
    expect(usage.recent[0]).toBe('🚀');
  });

  it('searches the picker, reacts with Enter and closes on Esc', async () => {
    const { calls } = mockFetch(
      routes(BOB, discussing(), {
        [`PUT ${reactionPath('c1', '🎉')}`]: {
          status: 200,
          body: { reflectionId: 'c1', emoji: '🎉', count: 1, mine: true, seq: 3 },
        },
      }),
    );
    const { wrapper } = await open();
    await wrapper.get('[data-testid="add-reaction"]').trigger('click');
    await flushPromises();
    await vi.waitFor(() => expect(wrapper.find('[data-emoji="😀"]').exists()).toBe(true));
    const picker = wrapper.get('[data-testid="emoji-picker"]');
    expect(picker.findAll('[role="tab"]').map((t) => t.attributes('aria-label'))).toEqual([
      'Recently used',
      'Smileys',
      'People & gestures',
      'Nature',
      'Food',
      'Objects',
      'Symbols',
    ]);
    // Nothing used yet.
    expect(picker.text()).toContain('Emoji you react with show up here.');

    const input = picker.get('input[aria-label="Search emoji"]');
    await input.setValue('party popper');
    expect(picker.get('[data-emoji="🎉"]').attributes('aria-label')).toBe('emoji Party popper');
    expect(picker.get('[data-emoji="🎉"]').attributes('aria-pressed')).toBe('false');
    await picker.get('[data-emoji="🎉"]').trigger('focus');
    expect(picker.text()).toContain(':party_popper: · Enter to react');
    await input.trigger('keydown', { key: 'Enter' });
    await flushPromises();
    expect(calls.some((c) => c.method === 'PUT' && c.path === reactionPath('c1', '🎉'))).toBe(true);

    await wrapper.get('[data-testid="add-reaction"]').trigger('click');
    await flushPromises();
    await wrapper.get('[data-testid="emoji-picker"]').trigger('keydown', { key: 'Escape' });
    await flushPromises();
    expect(wrapper.find('[data-testid="emoji-picker"]').exists()).toBe(false);
  });

  it('hangs the picker fixed from its button: below, or above with no room below', async () => {
    mockFetch(routes(BOB, discussing()));
    vi.spyOn(HTMLElement.prototype, 'offsetWidth', 'get').mockReturnValue(340);
    vi.spyOn(HTMLElement.prototype, 'offsetHeight', 'get').mockReturnValue(450);
    let anchor = { top: 100, bottom: 130, left: 50, right: 80 };
    const original = Element.prototype.getBoundingClientRect;
    vi.spyOn(Element.prototype, 'getBoundingClientRect').mockImplementation(function (
      this: Element,
    ) {
      return this.matches('[data-testid="add-reaction"]')
        ? ({ ...anchor, width: 30, height: 30, x: anchor.left, y: anchor.top } as DOMRect)
        : original.call(this);
    });
    const { wrapper } = await open();
    const style = () => wrapper.get<HTMLElement>('[data-testid="emoji-picker"]').element.style;

    // Room below (jsdom's window is 1024 × 768): under the button, 8 px left of it.
    await wrapper.get('[data-testid="add-reaction"]').trigger('click');
    await flushPromises();
    expect(style().top).toBe('138px');
    expect(style().left).toBe('42px');
    expect(style().bottom).toBe('');
    await wrapper.get('[data-testid="add-reaction"]').trigger('click');
    await flushPromises();

    // Near the bottom right: above the button, kept 16 px from the right edge.
    anchor = { top: 600, bottom: 630, left: 900, right: 930 };
    await wrapper.get('[data-testid="add-reaction"]').trigger('click');
    await flushPromises();
    expect(style().bottom).toBe('176px');
    expect(style().left).toBe('668px');
    expect(style().top).toBe('');
  });

  it('writes a task for the current group', async () => {
    const created = {
      ...task,
      id: 't2',
      contentHtml: '<p>Split the CI pipeline</p>',
      author: { id: ALICE.id, displayName: ALICE.displayName, pictureVersion: null },
    };
    const { calls } = mockFetch(
      routes(ALICE, discussing(), {
        [`POST ${BASE}/tasks`]: {
          status: 201,
          body: discussing({ tasks: [task, created], seq: 3 }),
        },
      }),
    );
    const { wrapper } = await open();
    const editor = wrapper.findComponent(RichTextEditor);
    expect(editor.props('kind')).toBe('task');
    expect(editor.props('teamId')).toBe(TEAM_ID);
    editor.vm.$emit('update:modelValue', '<p>Split the CI pipeline</p>');
    await flushPromises();
    await button(wrapper, 'Add task')!.trigger('click');
    await flushPromises();
    expect(calls.find((c) => c.path === `${BASE}/tasks`)?.body).toEqual({
      groupId: 'g1',
      content: '<p>Split the CI pipeline</p>',
    });
    const tasks = wrapper.findAll('[data-testid="discuss-task"]');
    expect(tasks.map((t) => t.text())).toEqual([
      expect.stringContaining('Quarantine flaky tests'),
      expect.stringContaining('Split the CI pipeline'),
    ]);
    expect(wrapper.findComponent(RichTextEditor).props('modelValue')).toBe('');
  });

  it('keeps an unsent task when the facilitator moves to another group and back', async () => {
    const { calls } = mockFetch(routes(BOB));
    const { wrapper } = await open();
    wrapper.findComponent(RichTextEditor).vm.$emit('update:modelValue', '<p>Half a thought</p>');
    await flushPromises();

    const socket = await openSocket();
    socket.emit({
      type: 'discuss.group_changed',
      channel,
      seq: 3,
      payload: { groupId: 'g2', clientId: null },
    });
    await flushPromises();
    expect(groupTitle(wrapper)).toBe('On-call');
    expect(wrapper.findComponent(RichTextEditor).props('modelValue')).toBe('');

    socket.emit({
      type: 'discuss.group_changed',
      channel,
      seq: 4,
      payload: { groupId: 'g1', clientId: null },
    });
    await flushPromises();
    expect(groupTitle(wrapper)).toBe('Flaky CI');
    expect(wrapper.findComponent(RichTextEditor).props('modelValue')).toBe('<p>Half a thought</p>');
    // The @-mention candidates are read once, not on every group.
    expect(calls.filter((c) => c.path === `/api/v1/teams/${TEAM_ID}/members`)).toHaveLength(1);
  });

  it('adds a task with Ctrl + Enter, as the hint says', async () => {
    const { calls } = mockFetch(
      routes(BOB, discussing(), {
        [`POST ${BASE}/tasks`]: { status: 201, body: discussing({ seq: 3 }) },
      }),
    );
    const { wrapper } = await open();
    const composer = wrapper.get('.composer');
    expect(composer.text()).toContain('Ctrl + Enter');
    const editor = wrapper.findComponent(RichTextEditor);
    expect(editor.props('shortcuts')).toBe(true);
    expect(editor.props('framed')).toBe(false);
    editor.vm.$emit('update:modelValue', '<p>Pin the runner image</p>');
    await flushPromises();
    editor.vm.$emit('submit');
    await flushPromises();
    expect(calls.find((c) => c.path === `${BASE}/tasks`)?.body).toEqual({
      groupId: 'g1',
      content: '<p>Pin the runner image</p>',
    });
    expect(wrapper.text()).toContain('Anyone in the meeting can edit or delete any task.');
  });

  it('edits a task in place and tells the others while the editor is open', async () => {
    const edited = {
      ...task,
      contentHtml: '<p>Quarantine flaky tests after two failures</p>',
      editedAt: '2026-11-10T18:47:00Z',
    };
    const { calls } = mockFetch(
      routes(BOB, discussing(), {
        'PATCH /api/v1/tasks/t1': { status: 200, body: edited },
      }),
    );
    const { wrapper } = await open();
    const socket = await openSocket();
    socket.emit({ type: 'subscribed', channel });
    await flushPromises();
    const item = () => wrapper.get('[data-testid="discuss-task"]');
    expect(item().text()).not.toContain('edited');

    await wrapper.get('button[aria-label="Edit task: Quarantine flaky tests"]').trigger('click');
    await flushPromises();
    expect(item().classes()).toContain('task--active');
    expect(socket.sent.at(-1)).toEqual({
      type: 'taskWriting',
      channel,
      groupId: null,
      taskId: 't1',
    });
    const editor = item().findComponent(RichTextEditor);
    expect(editor.props('modelValue')).toBe('<p>Quarantine flaky tests</p>');
    editor.vm.$emit('update:modelValue', edited.contentHtml);
    await flushPromises();
    await item()
      .findAll('button')
      .find((b) => b.text() === 'Save')!
      .trigger('click');
    await flushPromises();

    expect(calls.find((c) => c.method === 'PATCH')?.body).toEqual({
      content: edited.contentHtml,
    });
    expect(item().findComponent(RichTextEditor).exists()).toBe(false);
    expect(item().text()).toContain('Quarantine flaky tests after two failures');
    expect(item().text()).toContain('Bob Bianchi · edited');
    expect(socket.sent.at(-1)).toEqual({
      type: 'taskWriting',
      channel,
      groupId: null,
      taskId: null,
    });
  });

  it('deletes a task once confirmed', async () => {
    const { calls } = mockFetch(
      routes(BOB, discussing(), { 'DELETE /api/v1/tasks/t1': { status: 204 } }),
    );
    const { wrapper } = await open();
    await wrapper.get('button[aria-label="Delete task: Quarantine flaky tests"]').trigger('click');
    await flushPromises();
    const dialog = wrapper.get('dialog');
    expect(dialog.get('h2').text()).toBe('Delete this task?');
    expect(calls.some((c) => c.method === 'DELETE')).toBe(false);
    await dialog
      .findAll('button')
      .find((b) => b.text() === 'Delete')!
      .trigger('click');
    await flushPromises();
    expect(calls.find((c) => c.method === 'DELETE')?.path).toBe('/api/v1/tasks/t1');
    expect(wrapper.find('[data-testid="discuss-task"]').exists()).toBe(false);
  });

  it('shows who else is writing or editing a task, never the viewer', async () => {
    mockFetch(routes(BOB));
    const { wrapper } = await open();
    const socket = await openSocket();
    socket.emit({
      type: 'task.writing',
      channel,
      payload: {
        writers: [
          { userId: ALICE.id, groupId: 'g1', taskId: null },
          { userId: ALICE.id, groupId: null, taskId: 't1' },
          { userId: BOB.id, groupId: 'g1', taskId: null },
          { userId: ALICE.id, groupId: 'g2', taskId: null },
        ],
      },
    });
    await flushPromises();
    const writing = wrapper.findAll('[data-testid="discuss-task-writing"]');
    expect(writing).toHaveLength(1);
    expect(writing[0]!.attributes('aria-label')).toBe('Alice is writing a task');
    const item = wrapper.get('[data-testid="discuss-task"]');
    expect(item.classes()).toContain('task--active');
    expect(item.text()).toContain('Alice is editing');

    socket.emit({ type: 'task.writing', channel, payload: { writers: [] } });
    await flushPromises();
    expect(wrapper.find('[data-testid="discuss-task-writing"]').exists()).toBe(false);
    expect(wrapper.get('[data-testid="discuss-task"]').classes()).not.toContain('task--active');
  });

  it('tells the others while an unsent task is held for the current group', async () => {
    mockFetch(routes(BOB));
    const { wrapper } = await open();
    const socket = await openSocket();
    socket.emit({ type: 'subscribed', channel });
    await flushPromises();
    const writes = () => socket.sent.filter((m) => m.type === 'taskWriting');
    expect(writes()).toEqual([]);

    wrapper.findComponent(RichTextEditor).vm.$emit('update:modelValue', '<p>Pin</p>');
    await flushPromises();
    expect(writes().at(-1)).toEqual({ type: 'taskWriting', channel, groupId: 'g1', taskId: null });
    wrapper.findComponent(RichTextEditor).vm.$emit('update:modelValue', '<p></p>');
    await flushPromises();
    expect(writes().at(-1)).toEqual({ type: 'taskWriting', channel, groupId: null, taskId: null });
  });

  it('offers no edit or delete on a previewed group', async () => {
    mockFetch(routes(BOB, discussing({ tasks: [task, { ...task, id: 't3', groupId: 'g2' }] })));
    const { wrapper } = await open();
    expect(wrapper.find('button[aria-label^="Edit task"]').exists()).toBe(true);
    await entry(wrapper, 'On-call').trigger('click');
    await flushPromises();
    expect(wrapper.findAll('[data-testid="discuss-task"]')).toHaveLength(1);
    expect(wrapper.find('button[aria-label^="Edit task"]').exists()).toBe(false);
    expect(wrapper.find('button[aria-label^="Delete task"]').exists()).toBe(false);
  });

  it('shows an empty Discuss when no group was made', async () => {
    mockFetch(
      routes(
        ALICE,
        discussing({
          reflections: [],
          groups: [],
          discussOrder: [],
          discussGroupId: null,
          reactions: [],
          tasks: [],
        }),
      ),
    );
    const { wrapper } = await open();
    expect(wrapper.text()).toContain('There are no groups to discuss.');
    expect(wrapper.find('[data-testid="discuss-entry"]').exists()).toBe(false);
    expect(
      footer(wrapper)
        .findAll('button')
        .some((b) => b.text().includes('End meeting')),
    ).toBe(true);
  });

  it('is read-only once the meeting ended', async () => {
    mockFetch(routes(BOB, discussing({ status: 'ended', endedAt: '2026-11-10T19:00:00Z' })));
    const { wrapper } = await open();
    // The ended meeting goes to its summary, not the room.
    expect(wrapper.find('[data-testid="add-reaction"]').exists()).toBe(false);
  });
});
