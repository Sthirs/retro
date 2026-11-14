import { flushPromises, type VueWrapper } from '@vue/test-utils';
import { afterEach, describe, expect, it } from 'vitest';
import { VueDraggable } from 'vue-draggable-plus';

import type { TaskDto } from '@/api/client';

import { ALICE, BOB, me, mockFetch, renderAt, session, team, TEAM_ID } from './helpers';

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

function routes(tasks: TaskDto[]) {
  return {
    'POST /api/v1/auth/refresh': session(),
    'GET /api/v1/me': me([team()]),
    [`GET ${TASKS_PATH}`]: { status: 200, body: tasks },
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
});
