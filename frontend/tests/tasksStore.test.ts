import { flushPromises } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { TaskDto } from '@/api/client';
import { useTasksStore } from '@/stores/tasks';

import { ALICE, mockFetch, problem, TEAM_ID } from './helpers';

const task = (id: string, status: TaskDto['status'], position: string, extra = {}): TaskDto => ({
  id,
  status,
  position,
  contentHtml: `<p>${id}</p>`,
  author: { id: ALICE.id, displayName: ALICE.displayName, pictureVersion: null },
  mentions: [],
  meetingId: null,
  createdAt: '2026-11-14T10:00:00Z',
  ...extra,
});

const TASKS_PATH = `/api/v1/teams/${TEAM_ID}/tasks`;

const board = () => [
  task('d1', 'done', 'a0'),
  task('a2', 'active', 'a2'),
  task('a1', 'active', 'a1'),
  task('a3', 'active', 'a3'),
  task('f1', 'future', 'a0'),
];

const ids = (list: TaskDto[]) => list.map((t) => t.id);

describe('tasks store', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it('groups and sorts by column', async () => {
    mockFetch({ [`GET ${TASKS_PATH}`]: { status: 200, body: board() } });
    const store = useTasksStore();
    await store.load(TEAM_ID, {});

    expect(ids(store.columns.active)).toEqual(['a1', 'a2', 'a3']);
    expect(ids(store.columns.done)).toEqual(['d1']);
    expect(ids(store.columns.future)).toEqual(['f1']);
    expect(store.columns.stuck).toEqual([]);
    expect(store.loading).toBe(false);
  });

  it('passes the filter as query parameters', async () => {
    const { calls } = mockFetch({ [`GET ${TASKS_PATH}`]: { status: 200, body: [] } });
    const store = useTasksStore();
    await store.load(TEAM_ID, { mentioned: 'u1', meeting: 'm1' });

    expect(calls[0]?.path).toBe(`${TASKS_PATH}?mentioned=u1&meeting=m1`);
  });

  it('move sends the visible neighbours', async () => {
    const moved = task('d1', 'active', 'a15');
    const { calls } = mockFetch({
      [`GET ${TASKS_PATH}`]: { status: 200, body: board() },
      'PUT /api/v1/tasks/d1/position': { status: 200, body: moved },
    });
    const store = useTasksStore();
    await store.load(TEAM_ID, {});

    const pending = store.move('d1', 'active', 1);
    // Optimistic: shown at index 1 before the server answers.
    expect(store.tasks.find((t) => t.id === 'd1')?.status).toBe('active');
    expect(ids(store.columns.active)).toEqual(['a1', 'd1', 'a2', 'a3']);
    await pending;

    const put = calls.find((c) => c.method === 'PUT');
    expect(put?.body).toEqual({ status: 'active', afterId: 'a1', beforeId: 'a2' });
    expect(ids(store.columns.active)).toEqual(['a1', 'd1', 'a2', 'a3']);
    expect(store.tasks.find((t) => t.id === 'd1')?.position).toBe('a15');
    expect(ids(store.columns.done)).toEqual([]);
  });

  it('move to the top and the bottom sends one neighbour', async () => {
    const { calls } = mockFetch({
      [`GET ${TASKS_PATH}`]: { status: 200, body: board() },
      'PUT /api/v1/tasks/d1/position': { status: 200, body: task('d1', 'active', 'a0') },
      'PUT /api/v1/tasks/a1/position': { status: 200, body: task('a1', 'active', 'a4') },
    });
    const store = useTasksStore();
    await store.load(TEAM_ID, {});

    const top = store.move('d1', 'active', 0);
    expect(ids(store.columns.active)).toEqual(['d1', 'a1', 'a2', 'a3']);
    await top;
    const bottom = store.move('a1', 'active', 3);
    expect(ids(store.columns.active)).toEqual(['d1', 'a2', 'a3', 'a1']);
    await bottom;

    const puts = calls.filter((c) => c.method === 'PUT').map((c) => c.body);
    expect(puts[0]).toEqual({ status: 'active', beforeId: 'a1' });
    expect(puts[1]).toEqual({ status: 'active', afterId: 'a3' });
  });

  it('move to an empty column sends no neighbours', async () => {
    const { calls } = mockFetch({
      [`GET ${TASKS_PATH}`]: { status: 200, body: board() },
      'PUT /api/v1/tasks/a1/position': { status: 200, body: task('a1', 'stuck', 'a0') },
    });
    const store = useTasksStore();
    await store.load(TEAM_ID, {});

    const pending = store.move('a1', 'stuck', 0);
    expect(ids(store.columns.stuck)).toEqual(['a1']);
    await pending;

    expect(calls.find((c) => c.method === 'PUT')?.body).toEqual({ status: 'stuck' });
  });

  it('move rolls back and reloads on 409', async () => {
    const { calls } = mockFetch({
      [`GET ${TASKS_PATH}`]: { status: 200, body: board() },
      'PUT /api/v1/tasks/d1/position': problem(409, 'stale_position'),
    });
    const store = useTasksStore();
    await store.load(TEAM_ID, { author: 'u1' });

    await store.move('d1', 'active', 1);
    await flushPromises();

    expect(store.error).toBe('stale_position');
    expect(ids(store.columns.active)).toEqual(['a1', 'a2', 'a3']);
    expect(ids(store.columns.done)).toEqual(['d1']);
    const gets = calls.filter((c) => c.method === 'GET');
    expect(gets).toHaveLength(2);
    expect(gets[1]?.path).toBe(`${TASKS_PATH}?author=u1`);
  });

  it('move rolls back without reloading on other errors', async () => {
    const { calls } = mockFetch({
      [`GET ${TASKS_PATH}`]: { status: 200, body: board() },
      'PUT /api/v1/tasks/d1/position': problem(404, 'not_found'),
    });
    const store = useTasksStore();
    await store.load(TEAM_ID, {});

    await store.move('d1', 'active', 1);

    expect(store.error).toBe('not_found');
    expect(ids(store.columns.done)).toEqual(['d1']);
    expect(calls.filter((c) => c.method === 'GET')).toHaveLength(1);
  });

  it.each(['invalid_position', 'task_not_found'])('move reloads on %s', async (code) => {
    const { calls } = mockFetch({
      [`GET ${TASKS_PATH}`]: { status: 200, body: board() },
      'PUT /api/v1/tasks/d1/position': problem(code === 'task_not_found' ? 404 : 422, code),
    });
    const store = useTasksStore();
    await store.load(TEAM_ID, { meeting: 'm1' });

    await store.move('d1', 'active', 1);

    expect(store.error).toBe(code);
    expect(ids(store.columns.done)).toEqual(['d1']);
    const gets = calls.filter((c) => c.method === 'GET');
    expect(gets).toHaveLength(2);
    expect(gets[1]?.path).toBe(`${TASKS_PATH}?meeting=m1`);
  });

  it.each([
    [
      'update',
      'PATCH /api/v1/tasks/a2',
      (s: ReturnType<typeof useTasksStore>) => s.update('a2', '<p>x</p>'),
    ],
    ['remove', 'DELETE /api/v1/tasks/a2', (s: ReturnType<typeof useTasksStore>) => s.remove('a2')],
  ])('%s reloads on task_not_found and rethrows', async (_name, route, act) => {
    const { calls } = mockFetch({
      [`GET ${TASKS_PATH}`]: { status: 200, body: board() },
      [route]: problem(404, 'task_not_found'),
    });
    const store = useTasksStore();
    await store.load(TEAM_ID, { author: 'u1' });

    await expect(act(store)).rejects.toBeDefined();

    expect(store.error).toBe('task_not_found');
    const gets = calls.filter((c) => c.method === 'GET');
    expect(gets).toHaveLength(2);
    expect(gets[1]?.path).toBe(`${TASKS_PATH}?author=u1`);
  });

  it('update and remove do not reload on other errors', async () => {
    const { calls } = mockFetch({
      [`GET ${TASKS_PATH}`]: { status: 200, body: board() },
      'PATCH /api/v1/tasks/a2': problem(403, 'forbidden'),
    });
    const store = useTasksStore();
    await store.load(TEAM_ID, {});

    await expect(store.update('a2', '<p>x</p>')).rejects.toBeDefined();

    expect(store.error).toBe('forbidden');
    expect(calls.filter((c) => c.method === 'GET')).toHaveLength(1);
  });

  it('moveToEnd sends only the status and places the card last', async () => {
    const { calls } = mockFetch({
      [`GET ${TASKS_PATH}`]: { status: 200, body: board() },
      'PUT /api/v1/tasks/d1/position': { status: 200, body: task('d1', 'active', 'a9') },
    });
    const store = useTasksStore();
    await store.load(TEAM_ID, {});

    const pending = store.moveToEnd('d1', 'active');
    expect(ids(store.columns.active)).toEqual(['a1', 'a2', 'a3', 'd1']);
    await pending;

    expect(calls.find((c) => c.method === 'PUT')?.body).toEqual({ status: 'active' });
    expect(store.tasks.find((t) => t.id === 'd1')?.position).toBe('a9');
  });

  it('a failed older move does not undo a newer one', async () => {
    mockFetch({ [`GET ${TASKS_PATH}`]: { status: 200, body: board() } });
    const store = useTasksStore();
    await store.load(TEAM_ID, {});

    // Two PUTs the test answers by hand, in the order of its choosing.
    const answers: ((r: Response) => void)[] = [];
    vi.stubGlobal(
      'fetch',
      vi.fn(
        () =>
          new Promise<Response>((resolve) => {
            answers.push(resolve);
          }),
      ),
    );
    const json = (status: number, body: unknown) =>
      new Response(JSON.stringify(body), {
        status,
        headers: { 'Content-Type': 'application/json' },
      });

    const first = store.move('d1', 'active', 0);
    const second = store.move('d1', 'stuck', 0);
    await flushPromises();
    expect(answers).toHaveLength(2);

    // The first fails after the second started: the card must stay in Stuck.
    answers[0]?.(
      json(404, { type: 'about:blank', title: 'Error', status: 404, code: 'not_found' }),
    );
    await first;
    expect(store.tasks.find((t) => t.id === 'd1')?.status).toBe('stuck');
    expect(ids(store.columns.stuck)).toEqual(['d1']);

    answers[1]?.(json(200, task('d1', 'stuck', 'a0')));
    await second;
    expect(store.tasks.find((t) => t.id === 'd1')?.position).toBe('a0');
  });

  describe('quiet refresh', () => {
    const json = (status: number, body: unknown) =>
      new Response(body === undefined ? null : JSON.stringify(body), {
        status,
        headers: { 'Content-Type': 'application/json' },
      });

    /** A fetch the test answers by hand, request by request. */
    function manualFetch() {
      const pending: { path: string; method: string; answer: (r: Response) => void }[] = [];
      vi.stubGlobal(
        'fetch',
        vi.fn(
          (input: Request) =>
            new Promise<Response>((answer) => {
              pending.push({ path: new URL(input.url).pathname, method: input.method, answer });
            }),
        ),
      );
      return pending;
    }

    async function loaded() {
      mockFetch({ [`GET ${TASKS_PATH}`]: { status: 200, body: board() } });
      const store = useTasksStore();
      await store.load(TEAM_ID, {});
      return store;
    }

    it('does not bring back a task deleted while it was on its way', async () => {
      const store = await loaded();
      const pending = manualFetch();
      const refreshing = store.refresh();
      const removing = store.remove('a2');
      await flushPromises();
      // The delete lands first; the reload's answer predates it.
      pending[1]!.answer(json(204, undefined));
      await removing;
      pending[0]!.answer(json(200, board()));
      await refreshing;
      await flushPromises();
      expect(ids(store.columns.active)).toEqual(['a1', 'a3']);
      // It was asked again, and that answer is applied.
      expect(pending).toHaveLength(3);
      pending[2]!.answer(
        json(
          200,
          board().filter((t) => t.id !== 'a2'),
        ),
      );
      await flushPromises();
      expect(ids(store.columns.active)).toEqual(['a1', 'a3']);
    });

    it('asks again when a move started while it was on its way', async () => {
      const store = await loaded();
      const pending = manualFetch();
      const refreshing = store.refresh();
      const moving = store.move('f1', 'stuck', 0);
      await flushPromises();
      pending[0]!.answer(json(200, board()));
      await refreshing;
      // Dropped: it would put the card back in Future.
      expect(ids(store.columns.stuck)).toEqual(['f1']);
      pending[1]!.answer(json(200, task('f1', 'stuck', 'a0')));
      await moving;
      await flushPromises();
      expect(pending).toHaveLength(3);
      expect(pending[2]!.path).toBe(TASKS_PATH);
    });

    it('waits for a load in progress instead of overtaking it', async () => {
      const store = useTasksStore();
      const pending = manualFetch();
      const loading = store.load(TEAM_ID, {});
      await store.refresh();
      expect(store.loading).toBe(true);
      expect(pending).toHaveLength(1);
      pending[0]!.answer(json(200, board()));
      await loading;
      expect(store.loading).toBe(false);
      expect(ids(store.columns.active)).toEqual(['a1', 'a2', 'a3']);
      // Then the reload it held back runs.
      await flushPromises();
      expect(pending).toHaveLength(2);
    });
  });

  it('remove deletes and drops the task', async () => {
    const { calls } = mockFetch({
      [`GET ${TASKS_PATH}`]: { status: 200, body: board() },
      'DELETE /api/v1/tasks/a2': { status: 204 },
    });
    const store = useTasksStore();
    await store.load(TEAM_ID, {});

    await store.remove('a2');

    expect(calls.some((c) => c.method === 'DELETE' && c.path === '/api/v1/tasks/a2')).toBe(true);
    expect(ids(store.columns.active)).toEqual(['a1', 'a3']);
  });

  it('update replaces the task with the server copy', async () => {
    const { calls } = mockFetch({
      [`GET ${TASKS_PATH}`]: { status: 200, body: board() },
      'PATCH /api/v1/tasks/a2': {
        status: 200,
        body: task('a2', 'active', 'a2', { contentHtml: '<p>edited</p>' }),
      },
    });
    const store = useTasksStore();
    await store.load(TEAM_ID, {});

    await store.update('a2', '<p>edited</p>');

    expect(calls.find((c) => c.method === 'PATCH')?.body).toEqual({ content: '<p>edited</p>' });
    expect(store.tasks.find((t) => t.id === 'a2')?.contentHtml).toBe('<p>edited</p>');
  });

  it('records the error code when loading fails', async () => {
    mockFetch({ [`GET ${TASKS_PATH}`]: problem(403, 'forbidden') });
    const store = useTasksStore();
    await store.load(TEAM_ID, {});

    expect(store.error).toBe('forbidden');
    expect(store.tasks).toEqual([]);
    expect(store.loading).toBe(false);
  });

  it('is reset when the signed-in user changes', async () => {
    mockFetch({ [`GET ${TASKS_PATH}`]: { status: 200, body: board() } });
    const { useSessionStore } = await import('@/stores/session');
    const session = useSessionStore();
    session.user = ALICE;
    const store = useTasksStore();
    await store.load(TEAM_ID, {});
    expect(store.tasks).toHaveLength(5);

    session.user = null;
    await flushPromises();
    expect(store.tasks).toEqual([]);
    vi.unstubAllGlobals();
  });
});
