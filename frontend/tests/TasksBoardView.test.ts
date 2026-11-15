import { flushPromises, mount, type VueWrapper } from '@vue/test-utils';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { VueDraggable } from 'vue-draggable-plus';

import type { TaskDto } from '@/api/client';
import { CLIENT_ID } from '@/api/clientId';
import RichTextEditor from '@/components/RichTextEditor.vue';
import TaskFilters from '@/features/tasks/TaskFilters.vue';
import { buildI18n } from '@/i18n';

import {
  ALICE,
  BOB,
  me,
  mockFetch,
  mockSocket,
  openSocket,
  OTHER_TEAM_ID,
  renderAt,
  session,
  team,
  TEAM_ID,
  wsTicket,
} from './helpers';

// Mounted views listen to the real-time socket: unmount them so tests stay isolated.
const mounted: VueWrapper[] = [];
afterEach(() => {
  for (const w of mounted.splice(0)) w.unmount();
});
async function open() {
  const result = await renderAt(`/teams/${TEAM_ID}/tasks`);
  mounted.push(result.wrapper);
  return result;
}

const TASKS_PATH = `/api/v1/teams/${TEAM_ID}/tasks`;

const task = (id: string, status: TaskDto['status'], position: string, extra = {}): TaskDto => ({
  id,
  status,
  position,
  contentHtml: `<p>${id}</p>`,
  author: { id: ALICE.id, displayName: ALICE.displayName, pictureVersion: null },
  mentions: [],
  meetingId: null,
  createdAt: '2026-11-10T10:00:00Z',
  ...extra,
});

const member = (user: typeof ALICE, role = 'member') => ({
  user,
  role,
  joinedAt: '2026-09-01T10:00:00Z',
});

function routes(tasks: TaskDto[]) {
  return {
    'POST /api/v1/auth/refresh': session(),
    'GET /api/v1/me': me([team()]),
    [`GET ${TASKS_PATH}`]: { status: 200, body: tasks },
    [`GET /api/v1/teams/${TEAM_ID}/members`]: {
      status: 200,
      body: [member(ALICE, 'admin'), member(BOB)],
    },
  };
}

const column = (wrapper: VueWrapper, name: string) => wrapper.get(`section[aria-label="${name}"]`);

describe('Tasks board', () => {
  it('renders four columns with counts in order', async () => {
    mockFetch(
      routes([
        task('f1', 'future', 'a0'),
        task('f2', 'future', 'a1'),
        task('s1', 'stuck', 'a0'),
        task('d1', 'done', 'a0'),
      ]),
    );
    const { wrapper } = await open();

    expect(wrapper.get('h1').text()).toBe('Tasks');
    expect(wrapper.text()).toContain(
      'Created during the Discuss phase of a meeting. Drag cards between columns, or use Move to… from the keyboard.',
    );
    const sections = wrapper.findAll('section[aria-label]');
    expect(sections.map((s) => s.attributes('aria-label'))).toEqual([
      'Future',
      'Stuck',
      'Active',
      'Done',
    ]);
    expect(sections.map((s) => s.get('[data-testid="count"]').text())).toEqual([
      '2',
      '1',
      '0',
      '1',
    ]);
    expect(column(wrapper, 'Future').findAll('article')).toHaveLength(2);
    expect(wrapper.text()).not.toContain('No tasks yet');
  });

  it("renders a card's date, content with mention chip and author", async () => {
    mockFetch(
      routes([
        task('a1', 'active', 'a0', {
          contentHtml:
            '<p>Write a runbook <span data-type="mention" data-id="u2" data-label="Bob Bianchi">@Bob Bianchi</span></p>',
          author: { id: BOB.id, displayName: BOB.displayName, pictureVersion: null },
          createdAt: '2026-09-10T10:00:00Z',
        }),
      ]),
    );
    const { wrapper } = await open();

    const card = column(wrapper, 'Active').get('article');
    expect(card.text()).toContain('10 Sep');
    expect(card.text()).not.toContain('10 Sept');
    expect(card.text()).toContain('Write a runbook');
    expect(card.get('span[data-type="mention"]').text()).toBe('@Bob Bianchi');
    expect(card.text()).toContain('Bob Bianchi');
    expect(card.text()).toContain('BB');
    expect(card.find('button[aria-label="Task actions"]').exists()).toBe(true);
    expect(card.find('a').exists()).toBe(false);
  });

  it('shows the source meeting only when the task has one', async () => {
    mockFetch(routes([task('a1', 'active', 'a0', { meetingId: 'm1' })]));
    const { wrapper } = await open();

    expect(column(wrapper, 'Active').get('article').text()).toMatch(/10 Nov\s+·\s+Meeting/);
  });

  it('shows the empty state when there are no tasks', async () => {
    mockFetch(routes([]));
    const { wrapper } = await open();

    expect(wrapper.text()).toContain(
      'No tasks yet. Tasks are created in the Discuss phase of a meeting.',
    );
    expect(wrapper.findAll('section[aria-label]')).toHaveLength(4);
  });

  it("reloads live when the team channel reports another member's change", async () => {
    await mockSocket();
    const { calls } = mockFetch({
      ...routes([task('a1', 'active', 'a0')]),
      'POST /api/v1/ws-ticket': wsTicket(),
    });
    const { wrapper } = await open();
    const loads = () => calls.filter((c) => c.path === TASKS_PATH).length;
    const socket = await openSocket();
    expect(socket.url).toContain('/api/v1/ws?ticket=ws-ticket');
    expect(socket.channels).toContain(`team:${TEAM_ID}`);
    expect(loads()).toBe(1);

    socket.emit({ type: 'task.moved', channel: `team:${TEAM_ID}`, seq: 3, payload: {} });
    await flushPromises();
    expect(loads()).toBe(2);
    // Meeting events on the same channel leave the board alone.
    socket.emit({ type: 'meeting.started', channel: `team:${TEAM_ID}`, seq: 4, payload: {} });
    await flushPromises();
    expect(loads()).toBe(2);

    wrapper.unmount();
    mounted.length = 0;
    socket.emit({ type: 'task.created', channel: `team:${TEAM_ID}`, seq: 5, payload: {} });
    await flushPromises();
    expect(loads()).toBe(2);
  });

  it("skips the echo of this tab's own changes, not of the user's other tabs", async () => {
    await mockSocket();
    const { calls } = mockFetch({
      ...routes([task('a1', 'active', 'a0'), task('d1', 'done', 'a0')]),
      'POST /api/v1/ws-ticket': wsTicket(),
      'PUT /api/v1/tasks/a1/position': { status: 200, body: task('a1', 'done', 'a1') },
    });
    const { wrapper } = await open();
    const loads = () => calls.filter((c) => c.path === TASKS_PATH).length;
    const socket = await openSocket();

    // Every request names this tab.
    wrapper
      .findAllComponents(VueDraggable)[3]!
      .vm.$emit('add', { newIndex: 1, item: { dataset: { id: 'a1' } } });
    await flushPromises();
    const put = calls.find((c) => c.method === 'PUT')!;
    expect(put.headers.get('X-Client-Id')).toBe(CLIENT_ID);
    expect(loads()).toBe(1);

    const moved = (clientId: unknown) =>
      socket.emit({
        type: 'task.moved',
        channel: `team:${TEAM_ID}`,
        payload: { taskId: 'a1', status: 'done', clientId },
      });
    moved(CLIENT_ID);
    await flushPromises();
    expect(loads()).toBe(1);
    // Another tab, or a change without a tab id (another client), reloads.
    moved('0190a4e0-0000-7000-8000-0000000000ff');
    await flushPromises();
    expect(loads()).toBe(2);
    moved(null);
    await flushPromises();
    expect(loads()).toBe(3);
    // A reconnect always reloads.
    socket.emit({ type: 'resync', channel: `team:${TEAM_ID}` });
    await flushPromises();
    expect(loads()).toBe(4);
  });

  it('dropping a card calls PUT position', async () => {
    const { calls } = mockFetch({
      ...routes([task('a1', 'active', 'a0'), task('a2', 'active', 'a1'), task('d1', 'done', 'a0')]),
      'PUT /api/v1/tasks/a1/position': {
        status: 200,
        body: task('a1', 'done', 'a1'),
      },
    });
    const { wrapper } = await open();

    const draggables = wrapper.findAllComponents(VueDraggable);
    expect(draggables).toHaveLength(4);
    const done = draggables[3]!;
    done.vm.$emit('add', { newIndex: 1, item: { dataset: { id: 'a1' } } });
    await flushPromises();

    const put = calls.find((c) => c.method === 'PUT');
    expect(put?.path).toBe('/api/v1/tasks/a1/position');
    expect(put?.body).toEqual({ status: 'done', afterId: 'd1' });
    expect(column(wrapper, 'Done').findAll('article')).toHaveLength(2);
    expect(column(wrapper, 'Active').findAll('article')).toHaveLength(1);
  });

  it('reordering inside a column calls PUT position', async () => {
    const { calls } = mockFetch({
      ...routes([task('a1', 'active', 'a0'), task('a2', 'active', 'a1')]),
      'PUT /api/v1/tasks/a2/position': { status: 200, body: task('a2', 'active', 'Zz') },
    });
    const { wrapper } = await open();

    const active = wrapper.findAllComponents(VueDraggable)[2]!;
    active.vm.$emit('update', { newIndex: 0, item: { dataset: { id: 'a2' } } });
    await flushPromises();

    expect(calls.find((c) => c.method === 'PUT')?.body).toEqual({
      status: 'active',
      beforeId: 'a1',
    });
  });

  it('shows an alert when a move is refused', async () => {
    mockFetch({
      ...routes([task('a1', 'active', 'a0')]),
      'PUT /api/v1/tasks/a1/position': {
        status: 409,
        body: { type: 'about:blank', title: 'Conflict', status: 409, code: 'stale_position' },
      },
    });
    const { wrapper } = await open();

    wrapper
      .findAllComponents(VueDraggable)[3]!
      .vm.$emit('add', { newIndex: 0, item: { dataset: { id: 'a1' } } });
    await flushPromises();

    expect(wrapper.get('[role="alert"]').text()).toContain('board changed');
  });

  describe('filters', () => {
    const myTasks = (wrapper: VueWrapper) =>
      wrapper.get('[role="group"][aria-label="Filters"] button[aria-pressed]');
    const taskCalls = (calls: { path: string }[]) =>
      calls.filter((c) => c.path.startsWith(TASKS_PATH)).map((c) => c.path);
    const query = (params: Record<string, string>) =>
      `${TASKS_PATH}?${new URLSearchParams(params).toString()}`;
    const menu = (wrapper: VueWrapper, name: string) =>
      wrapper.get(`[data-testid="filter-${name}"]`);
    const trigger = (wrapper: VueWrapper, name: string) =>
      menu(wrapper, name).get('button[aria-haspopup="listbox"]');
    async function openMenu(wrapper: VueWrapper, name: string) {
      await trigger(wrapper, name).trigger('click');
      await flushPromises();
      return menu(wrapper, name);
    }
    const labels = (wrapper: VueWrapper, name: string) =>
      menu(wrapper, name)
        .findAll('[role="option"] .option__label')
        .map((o) => o.text());
    const option = (wrapper: VueWrapper, name: string, label: string) =>
      menu(wrapper, name)
        .findAll('[role="option"]')
        .find((o) => o.get('.option__label').text() === label)!;
    async function pick(wrapper: VueWrapper, name: string, label: string) {
      if (trigger(wrapper, name).attributes('aria-expanded') !== 'true') {
        await openMenu(wrapper, name);
      }
      await option(wrapper, name, label).trigger('click');
      await flushPromises();
    }

    it('My tasks filters by mentioned=me and reflects in the URL', async () => {
      const { calls } = mockFetch(routes([task('a1', 'active', 'a0')]));
      const { wrapper, router } = await open();

      const toggle = myTasks(wrapper);
      expect(toggle.text()).toBe('My tasks');
      expect(toggle.attributes('aria-pressed')).toBe('false');

      await toggle.trigger('click');
      await flushPromises();

      expect(router.currentRoute.value.query).toEqual({ mentioned: ALICE.id });
      expect(taskCalls(calls).at(-1)).toBe(`${TASKS_PATH}?mentioned=${ALICE.id}`);
      expect(myTasks(wrapper).attributes('aria-pressed')).toBe('true');
      expect(trigger(wrapper, 'mentioned').text()).toBe('Mentioned: me');
      await openMenu(wrapper, 'mentioned');
      expect(option(wrapper, 'mentioned', 'Alice Rossi (you)').attributes('aria-selected')).toBe(
        'true',
      );

      await myTasks(wrapper).trigger('click');
      await flushPromises();
      expect(router.currentRoute.value.query).toEqual({});
      expect(taskCalls(calls).at(-1)).toBe(TASKS_PATH);
    });

    it('Mentioned picks several people, and My tasks is on only for just me', async () => {
      const { calls } = mockFetch(routes([task('a1', 'active', 'a0')]));
      const { wrapper, router } = await open();

      await myTasks(wrapper).trigger('click');
      await flushPromises();
      await openMenu(wrapper, 'mentioned');
      expect(labels(wrapper, 'mentioned')).toEqual(['Alice Rossi (you)', 'Bob Bianchi']);
      expect(
        menu(wrapper, 'mentioned').get('[role="listbox"]').attributes('aria-multiselectable'),
      ).toBe('true');

      await pick(wrapper, 'mentioned', 'Bob Bianchi');
      // A multiple choice keeps the list open.
      expect(trigger(wrapper, 'mentioned').attributes('aria-expanded')).toBe('true');
      expect(router.currentRoute.value.query).toEqual({ mentioned: `${ALICE.id},${BOB.id}` });
      expect(taskCalls(calls).at(-1)).toBe(query({ mentioned: `${ALICE.id},${BOB.id}` }));
      expect(trigger(wrapper, 'mentioned').text()).toBe('Mentioned: me, Bob');
      expect(myTasks(wrapper).attributes('aria-pressed')).toBe('false');

      await pick(wrapper, 'mentioned', 'Alice Rossi (you)');
      expect(router.currentRoute.value.query).toEqual({ mentioned: BOB.id });
      expect(trigger(wrapper, 'mentioned').text()).toBe('Mentioned: Bob');

      // Done closes the list.
      const done = menu(wrapper, 'mentioned')
        .findAll('button')
        .find((b) => b.text() === 'Done')!;
      await done.trigger('click');
      expect(trigger(wrapper, 'mentioned').attributes('aria-expanded')).toBe('false');
    });

    it('the search field narrows the people', async () => {
      mockFetch(routes([task('a1', 'active', 'a0')]));
      const { wrapper } = await open();

      await openMenu(wrapper, 'mentioned');
      await menu(wrapper, 'mentioned').get('input[aria-label="Search people"]').setValue('bia');
      expect(labels(wrapper, 'mentioned')).toEqual(['Bob Bianchi']);

      await menu(wrapper, 'mentioned').get('input').setValue('zzz');
      expect(labels(wrapper, 'mentioned')).toEqual([]);
      expect(menu(wrapper, 'mentioned').text()).toContain('No matches');
    });

    it('a chip ✕ clears only its filter', async () => {
      mockFetch(routes([task('a1', 'active', 'a0')]));
      const result = await renderAt(
        `/teams/${TEAM_ID}/tasks?mentioned=${BOB.id}&author=${ALICE.id}`,
      );
      mounted.push(result.wrapper);

      await menu(result.wrapper, 'mentioned')
        .get('button[aria-label="Clear Mentioned filter"]')
        .trigger('click');
      await flushPromises();

      expect(result.router.currentRoute.value.query).toEqual({ author: ALICE.id });
      expect(trigger(result.wrapper, 'mentioned').text()).toBe('Mentioned: anyone');
    });

    it('author picks one person and sends author=', async () => {
      const { calls } = mockFetch(routes([task('a1', 'active', 'a0')]));
      const { wrapper, router } = await open();

      await openMenu(wrapper, 'author');
      expect(labels(wrapper, 'author')).toEqual(['Anyone', 'Alice Rossi (you)', 'Bob Bianchi']);
      await pick(wrapper, 'author', 'Bob Bianchi');

      expect(router.currentRoute.value.query).toEqual({ author: BOB.id });
      expect(taskCalls(calls).at(-1)).toBe(`${TASKS_PATH}?author=${BOB.id}`);
      // A single choice closes the list.
      expect(trigger(wrapper, 'author').attributes('aria-expanded')).toBe('false');
      expect(trigger(wrapper, 'author').text()).toBe('Author: Bob');
    });

    it('reads the filter from the URL on load', async () => {
      const { calls } = mockFetch(routes([task('a1', 'active', 'a0')]));
      const result = await renderAt(`/teams/${TEAM_ID}/tasks?author=${BOB.id}`);
      mounted.push(result.wrapper);

      expect(taskCalls(calls)[0]).toBe(`${TASKS_PATH}?author=${BOB.id}`);
      expect(trigger(result.wrapper, 'author').text()).toBe('Author: Bob');
    });

    it('survives a filter value that is no longer a member', async () => {
      mockFetch(routes([task('a1', 'active', 'a0')]));
      const result = await renderAt(`/teams/${TEAM_ID}/tasks?author=gone`);
      mounted.push(result.wrapper);

      expect(trigger(result.wrapper, 'author').text()).toBe('Author: Unknown member');
      await openMenu(result.wrapper, 'author');
      expect(option(result.wrapper, 'author', 'Unknown member').attributes('aria-selected')).toBe(
        'true',
      );
      expect(result.wrapper.text()).toContain('Clear filters');
    });

    it('keeps the URL value, with a neutral label, when members fail to load', async () => {
      mockFetch({
        ...routes([task('a1', 'active', 'a0')]),
        [`GET /api/v1/teams/${TEAM_ID}/members`]: { status: 500 },
      });
      const result = await renderAt(`/teams/${TEAM_ID}/tasks?author=gone`);
      mounted.push(result.wrapper);

      expect(trigger(result.wrapper, 'author').text()).toBe('Author: Selected member');
      expect(result.wrapper.text()).not.toContain('Unknown member');
    });

    it('keeps the URL value while members are loading', async () => {
      let release: () => void = () => {};
      const gate = new Promise<void>((resolve) => (release = resolve));
      const { fetchMock } = mockFetch(routes([task('a1', 'active', 'a0')]));
      const inner = fetchMock.getMockImplementation()!;
      fetchMock.mockImplementation(async (input: Request) => {
        if (new URL(input.url).pathname.endsWith('/members')) await gate;
        return inner(input);
      });
      const result = await renderAt(`/teams/${TEAM_ID}/tasks?author=${BOB.id}`);
      mounted.push(result.wrapper);

      expect(trigger(result.wrapper, 'author').text()).toBe('Author: Selected member');

      release();
      await flushPromises();
      expect(trigger(result.wrapper, 'author').text()).toBe('Author: Bob');
    });

    it('disables My tasks without a current user', () => {
      const wrapper = mount(TaskFilters, {
        props: { filter: {}, members: [], membersLoaded: false, meetingIds: [], meId: null },
        global: { plugins: [buildI18n()] },
      });
      expect(wrapper.get('button[aria-pressed]').attributes('disabled')).toBeDefined();
    });

    it('reloads once on mount, once per filter change and once per team switch', async () => {
      const { calls } = mockFetch({
        ...routes([task('a1', 'active', 'a0')]),
        'GET /api/v1/me': me([team(), team({ id: OTHER_TEAM_ID, name: 'Mobile Squad' })]),
        [`GET /api/v1/teams/${OTHER_TEAM_ID}/tasks`]: { status: 200, body: [] },
        [`GET /api/v1/teams/${OTHER_TEAM_ID}/members`]: { status: 200, body: [] },
      });
      const { wrapper, router } = await open();
      const gets = () => calls.filter((c) => /\/tasks(\?|$)/.test(c.path)).length;
      expect(gets()).toBe(1);

      await pick(wrapper, 'author', 'Bob Bianchi');
      expect(gets()).toBe(2);

      // A navigation that leaves the filter values alone (here an unrelated param) must not reload.
      await router.replace({ query: { author: BOB.id, other: '1' } });
      await flushPromises();
      expect(gets()).toBe(2);

      await router.push({ path: `/teams/${OTHER_TEAM_ID}/tasks` });
      await flushPromises();
      expect(gets()).toBe(3);
      expect(calls.at(-1)?.path).toBe(`/api/v1/teams/${OTHER_TEAM_ID}/members`);
      expect(
        calls.filter((c) => c.path.startsWith(`/api/v1/teams/${OTHER_TEAM_ID}/tasks`)),
      ).toHaveLength(1);
    });

    it('Clear filters resets the query', async () => {
      const { calls } = mockFetch(routes([]));
      const { wrapper, router } = await open();
      expect(wrapper.text()).not.toContain('Clear filters');

      await pick(wrapper, 'author', 'Bob Bianchi');
      // A filter that matches nothing is not the "no tasks yet" state.
      expect(wrapper.text()).not.toContain('No tasks yet');
      const clear = wrapper.findAll('button').find((b) => b.text() === 'Clear filters');
      expect(clear).toBeDefined();

      await clear!.trigger('click');
      await flushPromises();

      expect(router.currentRoute.value.query).toEqual({});
      expect(taskCalls(calls).at(-1)).toBe(TASKS_PATH);
      expect(wrapper.text()).not.toContain('Clear filters');
      expect(wrapper.text()).toContain('No tasks yet');
    });

    it('a meeting keeps its number while the filter hides the others', async () => {
      const all = [
        task('a1', 'active', 'a0', { meetingId: 'm1' }),
        task('a2', 'active', 'a1', { meetingId: 'm2' }),
      ];
      const { fetchMock } = mockFetch(routes(all));
      const inner = fetchMock.getMockImplementation()!;
      fetchMock.mockImplementation(async (input: Request) => {
        const url = new URL(input.url);
        if (url.pathname === TASKS_PATH && url.searchParams.get('meeting') === 'm2') {
          return new Response(JSON.stringify([all[1]]), {
            status: 200,
            headers: { 'content-type': 'application/json' },
          });
        }
        return inner(input);
      });
      const { wrapper } = await open();

      await pick(wrapper, 'meeting', 'Meeting 2');
      expect(column(wrapper, 'Active').findAll('article')).toHaveLength(1);
      expect(trigger(wrapper, 'meeting').text()).toBe('Meeting: Meeting 2');
      await openMenu(wrapper, 'meeting');
      expect(labels(wrapper, 'meeting')).toEqual(['All meetings', 'Meeting 1', 'Meeting 2']);
    });

    it('meeting filter hidden without meetings', async () => {
      mockFetch(routes([task('a1', 'active', 'a0')]));
      const { wrapper } = await open();

      expect(wrapper.find('[data-testid="filter-meeting"]').exists()).toBe(false);
    });

    it('meeting filter lists the loaded tasks meetings and sends meeting=', async () => {
      const { calls } = mockFetch(
        routes([
          task('a1', 'active', 'a0', { meetingId: 'm1' }),
          task('a2', 'active', 'a1', { meetingId: 'm1' }),
          task('a3', 'active', 'a2', { meetingId: 'm2' }),
        ]),
      );
      const { wrapper, router } = await open();

      expect(trigger(wrapper, 'meeting').text()).toBe('Meeting: all');
      await openMenu(wrapper, 'meeting');
      expect(labels(wrapper, 'meeting')).toEqual(['All meetings', 'Meeting 1', 'Meeting 2']);
      expect(option(wrapper, 'meeting', 'All meetings').attributes('aria-selected')).toBe('true');
      await pick(wrapper, 'meeting', 'Meeting 2');

      expect(router.currentRoute.value.query).toEqual({ meeting: 'm2' });
      expect(taskCalls(calls).at(-1)).toBe(`${TASKS_PATH}?meeting=m2`);
      expect(trigger(wrapper, 'meeting').text()).toBe('Meeting: Meeting 2');
    });
  });

  describe('task actions menu', () => {
    const actions = (wrapper: VueWrapper, name: string, index = 0) =>
      column(wrapper, name).findAll('button[aria-label="Task actions"]')[index]!;
    // The app shell has menus of its own: look only inside the board.
    const board = (wrapper: VueWrapper) => wrapper.get('.board');
    const items = (wrapper: VueWrapper) =>
      board(wrapper)
        .findAll('[role="menu"] [role="menuitem"]')
        .map((i) => i.text());
    const item = (wrapper: VueWrapper, label: string) =>
      board(wrapper)
        .findAll('[role="menuitem"]')
        .find((i) => i.text() === label)!;
    // Each card holds two dialogs; the open one is the one that renders a title.
    const dialog = (wrapper: VueWrapper) =>
      board(wrapper)
        .findAll('dialog')
        .find((d) => d.find('h2').exists())!;
    const hasMenu = (wrapper: VueWrapper) => board(wrapper).find('[role="menu"]').exists();

    it('menu shows Edit, Move to and Delete on every card', async () => {
      mockFetch(routes([task('a1', 'active', 'a0'), task('a2', 'active', 'a1')]));
      const { wrapper } = await open();
      expect(hasMenu(wrapper)).toBe(false);

      const button = actions(wrapper, 'Active', 0);
      expect(button.attributes('aria-expanded')).toBe('false');
      await button.trigger('click');
      expect(button.attributes('aria-expanded')).toBe('true');
      expect(items(wrapper)).toEqual(['Edit', 'Move to…', 'Delete']);
      await button.trigger('click');
      expect(hasMenu(wrapper)).toBe(false);

      await actions(wrapper, 'Active', 1).trigger('click');
      expect(items(wrapper)).toEqual(['Edit', 'Move to…', 'Delete']);
    });

    it('Move to lists the other columns, and Escape returns focus to the button', async () => {
      mockFetch(routes([task('a1', 'active', 'a0')]));
      const { wrapper } = await open();
      document.body.appendChild(wrapper.element);

      const button = actions(wrapper, 'Active');
      await button.trigger('click');
      await item(wrapper, 'Move to…').trigger('click');
      const submenu = board(wrapper).get('[role="menu"][aria-label="Move to…"]');
      expect(submenu.findAll('[role="menuitem"]').map((i) => i.text())).toEqual([
        'Future',
        'Stuck',
        'Done',
      ]);

      await submenu.trigger('keydown', { key: 'Escape' });
      expect(board(wrapper).find('[role="menu"][aria-label="Move to…"]').exists()).toBe(false);
      expect(board(wrapper).find('[role="menu"][aria-label="Task actions"]').exists()).toBe(true);

      await board(wrapper).get('[role="menu"]').trigger('keydown', { key: 'Escape' });
      expect(hasMenu(wrapper)).toBe(false);
      expect(document.activeElement).toBe(button.element);
    });

    it('arrow keys move between the menu items', async () => {
      mockFetch(routes([task('a1', 'active', 'a0')]));
      const { wrapper } = await open();
      document.body.appendChild(wrapper.element);

      await actions(wrapper, 'Active').trigger('click');
      await flushPromises();
      const all = board(wrapper).findAll('[role="menu"] [role="menuitem"]');
      expect(document.activeElement).toBe(all[0]!.element);
      await all[0]!.trigger('keydown', { key: 'ArrowDown' });
      expect(document.activeElement).toBe(all[1]!.element);
      await all[1]!.trigger('keydown', { key: 'ArrowDown' });
      await all[2]!.trigger('keydown', { key: 'ArrowDown' });
      expect(document.activeElement).toBe(all[0]!.element);
      await all[0]!.trigger('keydown', { key: 'ArrowUp' });
      expect(document.activeElement).toBe(all[2]!.element);
    });

    it('arrow keys, Home and End walk the Move to submenu, and Escape leaves it', async () => {
      mockFetch(routes([task('a1', 'active', 'a0')]));
      const { wrapper } = await open();
      document.body.appendChild(wrapper.element);

      await actions(wrapper, 'Active').trigger('click');
      await flushPromises();
      const move = item(wrapper, 'Move to…');
      await move.trigger('click');
      await flushPromises();
      const sub = board(wrapper).findAll('[role="menu"][aria-label="Move to…"] [role="menuitem"]');
      expect(sub.map((i) => i.text())).toEqual(['Future', 'Stuck', 'Done']);
      expect(document.activeElement).toBe(sub[0]!.element);

      await sub[0]!.trigger('keydown', { key: 'ArrowDown' });
      expect(document.activeElement).toBe(sub[1]!.element);
      await sub[1]!.trigger('keydown', { key: 'ArrowDown' });
      expect(document.activeElement).toBe(sub[2]!.element);
      await sub[2]!.trigger('keydown', { key: 'ArrowDown' });
      expect(document.activeElement).toBe(sub[0]!.element);
      await sub[0]!.trigger('keydown', { key: 'ArrowUp' });
      expect(document.activeElement).toBe(sub[2]!.element);
      await sub[2]!.trigger('keydown', { key: 'Home' });
      expect(document.activeElement).toBe(sub[0]!.element);
      await sub[0]!.trigger('keydown', { key: 'End' });
      expect(document.activeElement).toBe(sub[2]!.element);

      await sub[2]!.trigger('keydown', { key: 'Escape' });
      expect(document.activeElement).toBe(move.element);
      expect(board(wrapper).find('[role="menu"][aria-label="Move to…"]').exists()).toBe(false);
      expect(hasMenu(wrapper)).toBe(true);
    });

    it('ArrowDown or ArrowRight on Move to enters the open submenu', async () => {
      mockFetch(routes([task('a1', 'active', 'a0')]));
      const { wrapper } = await open();
      document.body.appendChild(wrapper.element);

      await actions(wrapper, 'Active').trigger('click');
      await flushPromises();
      const move = item(wrapper, 'Move to…');
      await move.trigger('keydown', { key: 'ArrowRight' });
      await flushPromises();
      const sub = () =>
        board(wrapper).findAll('[role="menu"][aria-label="Move to…"] [role="menuitem"]');
      expect(document.activeElement).toBe(sub()[0]!.element);

      (move.element as HTMLElement).focus();
      await move.trigger('keydown', { key: 'ArrowDown' });
      await flushPromises();
      expect(document.activeElement).toBe(sub()[0]!.element);
    });

    it('the panel is fixed, so the board scroll box cannot clip it, and flips up near the bottom', async () => {
      mockFetch(routes([task('a1', 'active', 'a0')]));
      const { wrapper } = await open();
      const button = actions(wrapper, 'Active');
      const panel = () => board(wrapper).get('[role="menu"]').element as HTMLElement;

      await button.trigger('click');
      await flushPromises();
      expect(panel().style.position).toBe('fixed');
      expect(panel().style.top).not.toBe('');
      expect(panel().style.bottom).toBe('');
      await button.trigger('click');

      // A button near the bottom of the window, with a panel too tall to hang below it.
      const rect = { top: 700, bottom: 744, left: 900, right: 944, width: 44, height: 44 };
      vi.spyOn(button.element, 'getBoundingClientRect').mockReturnValue(rect as DOMRect);
      const height = vi.spyOn(HTMLElement.prototype, 'offsetHeight', 'get').mockReturnValue(230);
      try {
        await button.trigger('click');
        await flushPromises();
        expect(panel().style.position).toBe('fixed');
        expect(panel().style.top).toBe('');
        expect(panel().style.bottom).toBe(`${window.innerHeight - rect.top + 4}px`);
      } finally {
        height.mockRestore();
      }
    });

    it('closes when the page scrolls', async () => {
      mockFetch(routes([task('a1', 'active', 'a0')]));
      const { wrapper } = await open();

      await actions(wrapper, 'Active').trigger('click');
      expect(hasMenu(wrapper)).toBe(true);
      document.dispatchEvent(new Event('scroll'));
      await flushPromises();
      expect(hasMenu(wrapper)).toBe(false);
    });

    it('the Edit and Delete dialogs are not inside the draggable cards', async () => {
      mockFetch(routes([task('a1', 'active', 'a0')]));
      const { wrapper } = await open();

      await actions(wrapper, 'Active').trigger('click');
      await item(wrapper, 'Edit').trigger('click');
      await flushPromises();
      expect(dialog(wrapper).exists()).toBe(true);
      expect(board(wrapper).find('article dialog').exists()).toBe(false);
      expect(board(wrapper).find('.board__grid dialog').exists()).toBe(false);
    });

    it('focus moves to the target column after Move to, and to the source column after Delete', async () => {
      mockFetch({
        ...routes([task('a1', 'active', 'a0'), task('a2', 'active', 'a1')]),
        'PUT /api/v1/tasks/a1/position': { status: 200, body: task('a1', 'done', 'a0') },
        'DELETE /api/v1/tasks/a2': { status: 204 },
      });
      const { wrapper } = await open();
      document.body.appendChild(wrapper.element);

      await actions(wrapper, 'Active', 0).trigger('click');
      await item(wrapper, 'Move to…').trigger('click');
      await item(wrapper, 'Done').trigger('click');
      await flushPromises();
      expect(document.activeElement).toBe(column(wrapper, 'Done').element);
      expect(column(wrapper, 'Done').attributes('tabindex')).toBe('-1');

      await actions(wrapper, 'Active', 0).trigger('click');
      await item(wrapper, 'Delete').trigger('click');
      await flushPromises();
      await dialog(wrapper)
        .findAll('button')
        .find((b) => b.text() === 'Delete')!
        .trigger('click');
      await flushPromises();
      expect(column(wrapper, 'Active').findAll('article')).toHaveLength(0);
      expect(document.activeElement).toBe(column(wrapper, 'Active').element);
    });

    it('Move to Done appends to Done', async () => {
      const { calls } = mockFetch({
        ...routes([task('a1', 'active', 'a0'), task('d1', 'done', 'a0'), task('d2', 'done', 'a1')]),
        'PUT /api/v1/tasks/a1/position': { status: 200, body: task('a1', 'done', 'a2') },
      });
      const { wrapper } = await open();

      await actions(wrapper, 'Active').trigger('click');
      await item(wrapper, 'Move to…').trigger('click');
      await item(wrapper, 'Done').trigger('click');
      await flushPromises();

      // No neighbours: the server appends after the whole column.
      expect(calls.find((c) => c.method === 'PUT')?.body).toEqual({ status: 'done' });
      expect(hasMenu(wrapper)).toBe(false);
      expect(column(wrapper, 'Done').findAll('article')).toHaveLength(3);
      expect(column(wrapper, 'Active').findAll('article')).toHaveLength(0);
    });

    it('Move to an empty column sends only the status', async () => {
      const { calls } = mockFetch({
        ...routes([task('a1', 'active', 'a0')]),
        'PUT /api/v1/tasks/a1/position': { status: 200, body: task('a1', 'done', 'a0') },
      });
      const { wrapper } = await open();

      await actions(wrapper, 'Active').trigger('click');
      await item(wrapper, 'Move to…').trigger('click');
      await item(wrapper, 'Done').trigger('click');
      await flushPromises();

      expect(calls.find((c) => c.method === 'PUT')?.body).toEqual({ status: 'done' });
    });

    it('edit saves new content', async () => {
      const { calls } = mockFetch({
        ...routes([task('a1', 'active', 'a0')]),
        'PATCH /api/v1/tasks/a1': {
          status: 200,
          body: task('a1', 'active', 'a0', { contentHtml: '<p>Rewritten</p>' }),
        },
      });
      const { wrapper } = await open();

      await actions(wrapper, 'Active').trigger('click');
      await item(wrapper, 'Edit').trigger('click');
      await flushPromises();
      expect(hasMenu(wrapper)).toBe(false);
      expect(dialog(wrapper).get('h2').text()).toBe('Edit task');
      const editor = wrapper.findComponent(RichTextEditor);
      expect(editor.props('label')).toBe('Task');
      expect(editor.props('modelValue')).toBe('<p>a1</p>');
      expect(editor.props('members').map((m: { id: string }) => m.id)).toEqual([ALICE.id, BOB.id]);

      editor.vm.$emit('update:modelValue', '<p>Rewritten</p>');
      await flushPromises();
      const save = dialog(wrapper)
        .findAll('button')
        .find((b) => b.text() === 'Save')!;
      await save.trigger('click');
      await flushPromises();

      const put = calls.find((c) => c.method === 'PATCH');
      expect(put?.path).toBe('/api/v1/tasks/a1');
      expect(put?.body).toEqual({ content: '<p>Rewritten</p>' });
      expect(column(wrapper, 'Active').get('article').text()).toContain('Rewritten');
      expect(board(wrapper).find('dialog h2').exists()).toBe(false);
    });

    it('edit keeps the dialog open and shows the error when saving fails', async () => {
      mockFetch({
        ...routes([task('a1', 'active', 'a0')]),
        'PATCH /api/v1/tasks/a1': {
          status: 400,
          body: { type: 'about:blank', title: 'Bad', status: 400, code: 'invalid_task_content' },
        },
      });
      const { wrapper } = await open();

      await actions(wrapper, 'Active').trigger('click');
      await item(wrapper, 'Edit').trigger('click');
      await flushPromises();
      await dialog(wrapper)
        .findAll('button')
        .find((b) => b.text() === 'Save')!
        .trigger('click');
      await flushPromises();

      expect(dialog(wrapper).get('[role="alert"]').text()).toContain('task text is empty');
      expect(dialog(wrapper).get('h2').text()).toBe('Edit task');
      expect(board(wrapper).findAll('[role="alert"]')).toHaveLength(1);

      await dialog(wrapper)
        .findAll('button')
        .find((b) => b.text() === 'Cancel')!
        .trigger('click');
      await flushPromises();
      expect(board(wrapper).find('dialog h2').exists()).toBe(false);
    });

    it('delete asks for confirmation then removes the card', async () => {
      const { calls } = mockFetch({
        ...routes([task('a1', 'active', 'a0'), task('a2', 'active', 'a1')]),
        'DELETE /api/v1/tasks/a1': { status: 204 },
      });
      const { wrapper } = await open();

      await actions(wrapper, 'Active').trigger('click');
      await item(wrapper, 'Delete').trigger('click');
      await flushPromises();
      expect(hasMenu(wrapper)).toBe(false);
      expect(dialog(wrapper).get('h2').text()).toBe('Delete this task?');
      expect(dialog(wrapper).text()).toContain("This can't be undone.");
      expect(calls.some((c) => c.method === 'DELETE')).toBe(false);

      await dialog(wrapper)
        .findAll('button')
        .find((b) => b.text() === 'Delete')!
        .trigger('click');
      await flushPromises();

      expect(calls.find((c) => c.method === 'DELETE')?.path).toBe('/api/v1/tasks/a1');
      expect(column(wrapper, 'Active').findAll('article')).toHaveLength(1);
      expect(column(wrapper, 'Active').get('article').text()).toContain('a2');
    });

    it('delete shows the error in the dialog when it fails', async () => {
      mockFetch({
        ...routes([task('a1', 'active', 'a0')]),
        'DELETE /api/v1/tasks/a1': {
          status: 404,
          body: { type: 'about:blank', title: 'Not found', status: 404, code: 'task_not_found' },
        },
      });
      const { wrapper } = await open();

      await actions(wrapper, 'Active').trigger('click');
      await item(wrapper, 'Delete').trigger('click');
      await flushPromises();
      await dialog(wrapper)
        .findAll('button')
        .find((b) => b.text() === 'Delete')!
        .trigger('click');
      await flushPromises();

      expect(dialog(wrapper).get('[role="alert"]').text()).toContain('no longer exists');
      expect(board(wrapper).findAll('[role="alert"]')).toHaveLength(1);
      expect(column(wrapper, 'Active').findAll('article')).toHaveLength(1);
    });
  });
});
