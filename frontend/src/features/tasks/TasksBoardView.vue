<script setup lang="ts">
/** The team's task board (spec §10): four columns of cards, drag and drop. */
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { useRoute, useRouter } from 'vue-router';

import {
  api,
  errorCode,
  TaskStatus,
  type MemberDto,
  type PersonDto,
  type TaskDto,
} from '@/api/client';
import AppIcon from '@/components/AppIcon.vue';
import AppAlert from '@/components/ui/AppAlert.vue';
import ConfirmDialog from '@/components/ui/ConfirmDialog.vue';
import EditTaskDialog from '@/features/tasks/EditTaskDialog.vue';
import TaskColumn from '@/features/tasks/TaskColumn.vue';
import TaskFilters from '@/features/tasks/TaskFilters.vue';
import { useSessionStore } from '@/stores/session';
import { useTasksStore, type TaskFilter } from '@/stores/tasks';
import { useErrorText } from '@/composables/useErrorText';

const COLUMNS = [TaskStatus.future, TaskStatus.stuck, TaskStatus.active, TaskStatus.done] as const;

const { t } = useI18n();
const translateError = useErrorText();
const route = useRoute();
const router = useRouter();
const session = useSessionStore();
const store = useTasksStore();

const teamId = computed(() => String(route.params.teamId));
const loaded = ref(false);
const grid = ref<HTMLElement | null>(null);
const members = ref<MemberDto[]>([]);
const membersLoaded = ref(false);
/** The members as the editor's @-mention candidates. */
const people = computed<PersonDto[]>(() =>
  members.value.map(({ user }) => ({
    id: user.id,
    displayName: user.displayName,
    pictureVersion: user.pictureVersion,
  })),
);

const FILTER_KEYS = ['mentioned', 'author', 'meeting'] as const;
/** The filter lives in the URL (`?mentioned=&author=&meeting=`), so it survives reloads and links. */
const filter = computed<TaskFilter>(() => {
  const result: TaskFilter = {};
  for (const key of FILTER_KEYS) {
    const raw = route.query[key];
    const value = Array.isArray(raw) ? raw[0] : raw;
    if (value) result[key] = value;
  }
  return result;
});
/**
 * The meetings seen on this team's board, in order of first appearance. They
 * accumulate across loads so a meeting keeps its number ("Meeting 2") while a
 * filter hides the others.
 */
const meetingIds = ref<string[]>([]);
watch(
  () => store.tasks,
  (tasks) => {
    const seen = new Set(meetingIds.value);
    const added = tasks
      .map((task) => task.meetingId)
      .filter((id): id is string => !!id && !seen.has(id));
    if (added.length) meetingIds.value = [...meetingIds.value, ...new Set(added)];
  },
);

const filterActive = computed(() => Object.values(filter.value).some(Boolean));
const isEmpty = computed(
  () => loaded.value && !store.error && store.tasks.length === 0 && !filterActive.value,
);
const errorText = computed(() => {
  const code = store.error;
  if (!code) return '';
  return translateError(code);
});

async function load(): Promise<void> {
  await store.load(teamId.value, filter.value);
  loaded.value = true;
}

async function loadMembers(): Promise<void> {
  const id = teamId.value;
  try {
    const res = await api.listMembers(id);
    // Ignore a late answer for a team we have since left.
    if (id !== teamId.value) return;
    members.value = res.status === 200 ? res.data : [];
    membersLoaded.value = res.status === 200;
  } catch {
    if (id === teamId.value) members.value = [];
  }
}

// The Edit and Delete dialogs are rendered once, here, not inside the cards:
// a card is draggable, and its dialog controls must not start a drag.
const editingId = ref<string | null>(null);
const deletingId = ref<string | null>(null);
const deleteBusy = ref(false);
const deleteError = ref<string | null>(null);
const editingTask = computed(() => store.tasks.find((task) => task.id === editingId.value));
const deletingTask = computed(() => store.tasks.find((task) => task.id === deletingId.value));

function askDelete(task: TaskDto): void {
  deleteError.value = null;
  deletingId.value = task.id;
}

async function confirmDelete(): Promise<void> {
  const task = deletingTask.value;
  if (!task || deleteBusy.value) return;
  deleteBusy.value = true;
  deleteError.value = null;
  try {
    await store.remove(task.id);
    deletingId.value = null;
    // The card is gone, and so is its button: keep focus on the column it was in.
    await focusColumn(task.status);
  } catch (e) {
    const code = errorCode(e);
    deleteError.value = translateError(code);
    // The dialog shows it: keep the board's own alert quiet.
    store.error = null;
  } finally {
    deleteBusy.value = false;
  }
}

/** A moved or deleted card takes its button with it; focus goes to a column instead. */
async function focusColumn(status: TaskStatus): Promise<void> {
  await nextTick();
  grid.value?.querySelector<HTMLElement>(`section[data-column="${status}"]`)?.focus();
}

function setFilter(next: TaskFilter): void {
  const query = { ...route.query };
  for (const key of FILTER_KEYS) {
    const value = next[key];
    if (value) query[key] = value;
    else delete query[key];
  }
  void router.replace({ query });
}

// Until the real-time hub exists, the board refreshes when the window regains focus.
onMounted(() => {
  void load();
  void loadMembers();
  window.addEventListener('focus', load);
});
onBeforeUnmount(() => window.removeEventListener('focus', load));

// Switching team keeps this view alive: show the new team's board.
watch(teamId, () => {
  loaded.value = false;
  meetingIds.value = [];
  members.value = [];
  membersLoaded.value = false;
  store.reset();
  void load();
  void loadMembers();
});
// Reload only when a filter value changes: `filter` is a new object on every query change.
watch(
  () => JSON.stringify(filter.value),
  () => void load(),
);
</script>

<template>
  <div class="board">
    <div class="board__header">
      <h1 class="board__title">{{ t('tasks.title') }}</h1>
      <p class="board__subtitle">{{ t('tasks.subtitle') }}</p>
    </div>

    <AppAlert v-if="errorText">{{ errorText }}</AppAlert>

    <TaskFilters
      :filter="filter"
      :members="members"
      :members-loaded="membersLoaded"
      :meeting-ids="meetingIds"
      :me-id="session.user?.id ?? null"
      @change="setFilter"
    />

    <section v-if="isEmpty" class="empty">
      <span class="empty__icon" aria-hidden="true">
        <AppIcon name="tasks" :size="26" />
      </span>
      <p class="empty__text">{{ t('tasks.empty') }}</p>
    </section>

    <div class="board__scroll">
      <div ref="grid" class="board__grid">
        <TaskColumn
          v-for="status in COLUMNS"
          :key="status"
          :status="status"
          :tasks="store.columns[status]"
          @edit="editingId = $event.id"
          @delete="askDelete"
          @moved="focusColumn"
        />
      </div>
    </div>

    <EditTaskDialog
      v-if="editingTask"
      open
      :task="editingTask"
      :members="people"
      @close="editingId = null"
    />
    <ConfirmDialog
      v-if="deletingTask"
      open
      :title="t('tasks.deleteTitle')"
      :text="t('tasks.deleteText')"
      :confirm-label="t('tasks.delete')"
      danger
      :busy="deleteBusy"
      :error="deleteError"
      @close="deletingId = null"
      @confirm="confirmDelete"
    />
  </div>
</template>

<style scoped>
.board {
  display: flex;
  flex-direction: column;
  gap: 24px;
  width: 100%;
  min-width: 0;
}
.board__header {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.board__title {
  margin: 0;
  font-size: 28px;
  font-weight: 700;
  letter-spacing: -0.01em;
}
.board__subtitle {
  margin: 0;
  font-size: 14px;
  line-height: 1.5;
  color: var(--muted);
}
.board__scroll {
  /* Narrow screens scroll the columns here, never the page. */
  max-width: 100%;
  overflow-x: auto;
}
.board__grid {
  display: grid;
  grid-template-columns: repeat(4, minmax(240px, 1fr));
  gap: 16px;
  align-items: stretch;
}
.empty {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 14px;
  padding: 28px;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: var(--radius-panel);
  box-shadow: var(--shadow-panel);
}
.empty__icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 52px;
  height: 52px;
  border-radius: 50%;
  background: var(--accent-soft);
  color: var(--accent-text);
}
.empty__text {
  margin: 0;
  color: var(--muted);
}
</style>
