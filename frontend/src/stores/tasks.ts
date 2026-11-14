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

export interface TaskFilter {
  mentioned?: string;
  author?: string;
  meeting?: string;
}

const STATUSES = Object.values(TaskStatus);

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
      if (seq === loadSeq) loading.value = false;
    }
  }

  /**
   * Moves a task to `toIndex` of a column (counted without the task itself).
   * The card jumps there at once and the server's answer replaces the guess.
   */
  async function move(taskId: string, status: TaskStatus, toIndex: number): Promise<void> {
    const original = tasks.value.find((t) => t.id === taskId);
    if (!original) return;

    const others = columns.value[status].filter((t) => t.id !== taskId);
    const index = Math.max(0, Math.min(toIndex, others.length));
    const after = others[index - 1];
    const before = others[index];

    const body: MoveTaskRequest = { status };
    if (after) body.afterId = after.id;
    if (before) body.beforeId = before.id;

    // Temporary key: the immediate successor of the previous card's key (so it
    // sorts right after it), or the lowest possible key at the top.
    replace({ ...original, status, position: after ? `${after.position}\u0000` : '' });
    error.value = null;

    try {
      const res = await api.moveTask(taskId, body);
      if (res.status !== 200) throw apiError(res);
      replace(res.data);
    } catch (e) {
      replace(original);
      const code = errorCode(e);
      error.value = code;
      // Stale neighbours: another member rearranged the column meanwhile.
      if (code === 'stale_position' && last) {
        await load(last.teamId, last.filter);
        // The reload cleared the error; the user still needs to know.
        error.value = code;
      }
    }
  }

  /** Edits a task's content; the server's sanitised copy replaces ours. */
  async function update(taskId: string, contentHtml: string): Promise<void> {
    try {
      const res = await api.updateTask(taskId, { content: contentHtml });
      if (res.status !== 200) throw apiError(res);
      error.value = null;
      replace(res.data);
    } catch (e) {
      error.value = errorCode(e);
      throw e;
    }
  }

  async function remove(taskId: string): Promise<void> {
    try {
      const res = await api.deleteTask(taskId);
      if (res.status !== 204) throw apiError(res);
      error.value = null;
      tasks.value = tasks.value.filter((t) => t.id !== taskId);
    } catch (e) {
      error.value = errorCode(e);
      throw e;
    }
  }

  return { tasks, loading, error, columns, load, move, update, remove, reset };
});
