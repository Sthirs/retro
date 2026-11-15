import { defineStore } from 'pinia';
import { computed, ref, watch } from 'vue';

import {
  api,
  apiError,
  errorCode,
  TaskStatus,
  type MoveTaskRequest,
  type TaskDto,
} from '@/api/client';
import { useSessionStore } from '@/stores/session';

/** The board's filter, as sent to the API and kept in the URL. */
export interface TaskFilter {
  /** Comma-separated user ids: tasks mentioning any of them. */
  mentioned?: string;
  author?: string;
  meeting?: string;
}

/** The ids of a comma-separated list, without blanks or repeats. */
export function splitIds(list: string | undefined): string[] {
  return [...new Set((list ?? '').split(',').map((id) => id.trim()))].filter(Boolean);
}

/** The inverse of `splitIds`. */
export function joinIds(ids: string[]): string {
  return [...new Set(ids.filter(Boolean))].join(',');
}

const STATUSES = Object.values(TaskStatus);
// Refusals that mean the local board is out of date: reload it.
const MOVE_RELOAD_CODES: ReadonlySet<string> = new Set([
  'stale_position',
  'invalid_position',
  'task_not_found',
]);
const GONE_CODES: ReadonlySet<string> = new Set(['task_not_found']);

/**
 * The Tasks board's data: a team's tasks (spec §9), grouped into columns, with
 * optimistic moves that roll back when the server refuses.
 */
export const useTasksStore = defineStore('tasks', () => {
  const session = useSessionStore();
  const tasks = ref<TaskDto[]>([]);
  const loading = ref(false);
  const error = ref<string | null>(null);
  let last: { teamId: string; filter: TaskFilter } | null = null;
  // Only the newest load may write its result: filters change while loading.
  let loadSeq = 0;

  /** The tasks of each column, sorted by `position` (plain string order). */
  const columns = computed(() => {
    const result = Object.fromEntries(STATUSES.map((s) => [s, [] as TaskDto[]])) as Record<
      TaskStatus,
      TaskDto[]
    >;
    for (const t of tasks.value) result[t.status].push(t);
    for (const list of Object.values(result)) {
      list.sort((a, b) => (a.position < b.position ? -1 : a.position > b.position ? 1 : 0));
    }
    return result;
  });

  function reset(): void {
    loadSeq++;
    tasks.value = [];
    loading.value = false;
    error.value = null;
    last = null;
  }

  // A different user (or none) invalidates everything.
  watch(
    () => session.user?.id,
    () => reset(),
  );

  function replace(task: TaskDto): void {
    tasks.value = tasks.value.map((t) => (t.id === task.id ? task : t));
  }

  /** Loads a team's tasks, remembering the filter for later reloads. */
  async function load(teamId: string, filter: TaskFilter): Promise<void> {
    last = { teamId, filter };
    const seq = ++loadSeq;
    loading.value = true;
    error.value = null;
    try {
      const res = await api.listTasks(teamId, filter);
      if (res.status !== 200) throw apiError(res);
      if (seq === loadSeq) tasks.value = res.data;
    } catch (e) {
      if (seq === loadSeq) error.value = errorCode(e);
    } finally {
      if (seq === loadSeq) {
        loading.value = false;
        flushRefresh();
      }
    }
  }

  /**
   * Reloads the board quietly after another member changed it (a real-time
   * event). While a load or one of our own moves is waiting for its answer,
   * the reload waits too, so the card does not jump back and forth. An answer
   * overtaken by one of our own changes is dropped and asked again: it may
   * predate that change, whose own echo does not reload the board.
   */
  let refreshWanted = false;
  // Bumped by each of our own changes that lands.
  let mutationRev = 0;
  async function refresh(): Promise<void> {
    if (!last) return;
    if (latestMove.size > 0 || loading.value) {
      refreshWanted = true;
      return;
    }
    refreshWanted = false;
    const { teamId, filter } = last;
    const seq = ++loadSeq;
    const rev = mutationRev;
    try {
      const res = await api.listTasks(teamId, filter);
      if (res.status !== 200) throw apiError(res);
      if (seq !== loadSeq) return;
      if (latestMove.size > 0 || rev !== mutationRev) {
        refreshWanted = true;
        flushRefresh();
        return;
      }
      tasks.value = res.data;
    } catch {
      // Keep the current board; the next event or load tries again.
    }
  }

  /** Runs the reload that had to wait, once nothing holds it back. */
  function flushRefresh(): void {
    if (refreshWanted && latestMove.size === 0 && !loading.value) void refresh();
  }

  /**
   * After a refused change that means our copy of the board is out of date,
   * reloads it with the last filter. The reload clears the error, but the user
   * still needs to know what happened, so the code is put back.
   */
  async function refreshAfter(code: string, codes: ReadonlySet<string>): Promise<void> {
    error.value = code;
    if (!codes.has(code) || !last) return;
    await load(last.teamId, last.filter);
    error.value = code;
  }

  /**
   * Moves a task to `toIndex` of a column (counted without the task itself).
   * The card jumps there at once and the server's answer replaces the guess.
   */
  async function move(taskId: string, status: TaskStatus, toIndex: number): Promise<void> {
    const others = columns.value[status].filter((t) => t.id !== taskId);
    const index = Math.max(0, Math.min(toIndex, others.length));
    const after = others[index - 1];
    const before = others[index];

    const body: MoveTaskRequest = { status };
    if (after) body.afterId = after.id;
    if (before) body.beforeId = before.id;
    await send(taskId, body, after?.position);
  }

  /**
   * Moves a task to the true end of a column. Only the status is sent, so the
   * server appends after the whole column, including cards a filter hides.
   */
  async function moveToEnd(taskId: string, status: TaskStatus): Promise<void> {
    const others = columns.value[status].filter((t) => t.id !== taskId);
    await send(taskId, { status }, others[others.length - 1]?.position);
  }

  // Which move of a task is the newest one; older answers must not undo it.
  let moveSeq = 0;
  const latestMove = new Map<string, number>();

  async function send(
    taskId: string,
    body: MoveTaskRequest,
    afterPosition: string | undefined,
  ): Promise<void> {
    const original = tasks.value.find((t) => t.id === taskId);
    if (!original) return;
    const status = body.status;

    // Temporary key: the immediate successor of the previous card's key (so it
    // sorts right after it), or the lowest possible key at the top.
    const temporary = afterPosition === undefined ? '' : `${afterPosition}\u0000`;
    const mine = ++moveSeq;
    latestMove.set(taskId, mine);
    replace({ ...original, status, position: temporary });
    error.value = null;

    const isLatest = () => latestMove.get(taskId) === mine;
    try {
      const res = await api.moveTask(taskId, body);
      if (res.status !== 200) throw apiError(res);
      mutationRev++;
      if (isLatest()) {
        latestMove.delete(taskId);
        replace(res.data);
      }
    } catch (e) {
      // Restore only if the card still shows this move's guess: a newer move
      // or a fresher load already put it somewhere more current.
      const current = tasks.value.find((t) => t.id === taskId);
      const stillMine = isLatest() && current?.status === status && current.position === temporary;
      if (isLatest()) latestMove.delete(taskId);
      if (stillMine) replace(original);
      // Stale neighbours or a task that is gone: another member changed the
      // board meanwhile.
      await refreshAfter(errorCode(e), MOVE_RELOAD_CODES);
    } finally {
      flushRefresh();
    }
  }

  /** Edits a task's content; the server's sanitised copy replaces ours. */
  async function update(taskId: string, contentHtml: string): Promise<void> {
    try {
      const res = await api.updateTask(taskId, { content: contentHtml });
      if (res.status !== 200) throw apiError(res);
      mutationRev++;
      error.value = null;
      replace(res.data);
      flushRefresh();
    } catch (e) {
      await refreshAfter(errorCode(e), GONE_CODES);
      throw e;
    }
  }

  async function remove(taskId: string): Promise<void> {
    try {
      const res = await api.deleteTask(taskId);
      if (res.status !== 204) throw apiError(res);
      mutationRev++;
      error.value = null;
      tasks.value = tasks.value.filter((t) => t.id !== taskId);
      flushRefresh();
    } catch (e) {
      await refreshAfter(errorCode(e), GONE_CODES);
      throw e;
    }
  }

  return { tasks, loading, error, columns, load, refresh, move, moveToEnd, update, remove, reset };
});
