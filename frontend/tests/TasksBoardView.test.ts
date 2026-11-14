import { flushPromises, mount, type VueWrapper } from '@vue/test-utils';
import { afterEach, describe, expect, it } from 'vitest';
import { VueDraggable } from 'vue-draggable-plus';

import type { TaskDto } from '@/api/client';
import TaskFilters from '@/features/tasks/TaskFilters.vue';
import { buildI18n } from '@/i18n';

import {
  ALICE,
  BOB,
  me,
  mockFetch,
  OTHER_TEAM_ID,
  renderAt,
  session,
  team,
  TEAM_ID,
} from './helpers';

// Mounted views listen to window `focus`: unmount them so tests stay isolated.
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
  canEdit: true,
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

  it('reloads on window focus and stops listening after unmount', async () => {
    const { calls } = mockFetch(routes([task('a1', 'active', 'a0')]));
    const { wrapper } = await open();
    const loads = () => calls.filter((c) => c.path === TASKS_PATH).length;
    expect(loads()).toBe(1);

    window.dispatchEvent(new Event('focus'));
    await flushPromises();
    expect(loads()).toBe(2);

    wrapper.unmount();
    mounted.length = 0;
    window.dispatchEvent(new Event('focus'));
    await flushPromises();
    expect(loads()).toBe(2);
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
      expect(
        (wrapper.get('select[aria-label="Mentioned"]').element as HTMLSelectElement).value,
      ).toBe(ALICE.id);

      await myTasks(wrapper).trigger('click');
      await flushPromises();
      expect(router.currentRoute.value.query).toEqual({});
      expect(taskCalls(calls).at(-1)).toBe(TASKS_PATH);
    });

    it('choosing another person in Mentioned turns My tasks off', async () => {
      const { calls } = mockFetch(routes([task('a1', 'active', 'a0')]));
      const { wrapper, router } = await open();

      await myTasks(wrapper).trigger('click');
      await flushPromises();
      await wrapper.get('select[aria-label="Mentioned"]').setValue(BOB.id);
      await flushPromises();

      expect(router.currentRoute.value.query).toEqual({ mentioned: BOB.id });
      expect(taskCalls(calls).at(-1)).toBe(`${TASKS_PATH}?mentioned=${BOB.id}`);
      expect(myTasks(wrapper).attributes('aria-pressed')).toBe('false');
    });

    it('author select sends author=', async () => {
      const { calls } = mockFetch(routes([task('a1', 'active', 'a0')]));
      const { wrapper, router } = await open();

      const select = wrapper.get('select[aria-label="Author"]');
      expect(select.findAll('option').map((o) => o.text())).toEqual([
        'anyone',
        'Alice Rossi',
        'Bob Bianchi',
      ]);
      await select.setValue(BOB.id);
      await flushPromises();

      expect(router.currentRoute.value.query).toEqual({ author: BOB.id });
      expect(taskCalls(calls).at(-1)).toBe(`${TASKS_PATH}?author=${BOB.id}`);
    });

    it('reads the filter from the URL on load', async () => {
      const { calls } = mockFetch(routes([task('a1', 'active', 'a0')]));
      const result = await renderAt(`/teams/${TEAM_ID}/tasks?author=${BOB.id}`);
      mounted.push(result.wrapper);

      expect(taskCalls(calls)[0]).toBe(`${TASKS_PATH}?author=${BOB.id}`);
      const select = result.wrapper.get('select[aria-label="Author"]');
      expect((select.element as HTMLSelectElement).value).toBe(BOB.id);
    });

    it('survives a filter value that is no longer a member', async () => {
      mockFetch(routes([task('a1', 'active', 'a0')]));
      const result = await renderAt(`/teams/${TEAM_ID}/tasks?author=gone`);
      mounted.push(result.wrapper);

      const select = result.wrapper.get('select[aria-label="Author"]');
      expect((select.element as HTMLSelectElement).value).toBe('gone');
      expect(result.wrapper.text()).toContain('Clear filters');
    });

    it('keeps the select on the URL value, with a neutral label, when members fail to load', async () => {
      mockFetch({
        ...routes([task('a1', 'active', 'a0')]),
        [`GET /api/v1/teams/${TEAM_ID}/members`]: { status: 500 },
      });
      const result = await renderAt(`/teams/${TEAM_ID}/tasks?author=gone`);
      mounted.push(result.wrapper);

      const select = result.wrapper.get('select[aria-label="Author"]');
      expect((select.element as HTMLSelectElement).value).toBe('gone');
      const selected = select.findAll('option').find((o) => o.element.selected);
      expect(selected?.text()).toBe('Selected member');
      expect(result.wrapper.text()).not.toContain('Unknown member');
    });

    it('keeps the select on the URL value while members are loading', async () => {
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

      const select = result.wrapper.get('select[aria-label="Author"]');
      expect((select.element as HTMLSelectElement).value).toBe(BOB.id);
      expect(
        select
          .findAll('option')
          .find((o) => o.element.selected)
          ?.text(),
      ).toBe('Selected member');

      release();
      await flushPromises();
      expect(
        select
          .findAll('option')
          .find((o) => o.element.selected)
          ?.text(),
      ).toBe('Bob Bianchi');
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

      await wrapper.get('select[aria-label="Author"]').setValue(BOB.id);
      await flushPromises();
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

      await wrapper.get('select[aria-label="Author"]').setValue(BOB.id);
      await flushPromises();
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

    it('meeting filter hidden without meetings', async () => {
      mockFetch(routes([task('a1', 'active', 'a0')]));
      const { wrapper } = await open();

      expect(wrapper.find('select[aria-label="Meeting"]').exists()).toBe(false);
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

      const select = wrapper.get('select[aria-label="Meeting"]');
      expect(select.findAll('option').map((o) => o.text())).toEqual([
        'all',
        'Meeting 1',
        'Meeting 2',
      ]);
      await select.setValue('m2');
      await flushPromises();

      expect(router.currentRoute.value.query).toEqual({ meeting: 'm2' });
      expect(taskCalls(calls).at(-1)).toBe(`${TASKS_PATH}?meeting=m2`);
    });
  });
});
