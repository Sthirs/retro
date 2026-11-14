<script setup lang="ts">
/** The team's task board (spec §10): four columns of cards, drag and drop. */
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { useRoute, useRouter } from 'vue-router';

import { api, TaskStatus, type MemberDto, type PersonDto } from '@/api/client';
import AppIcon from '@/components/AppIcon.vue';
import AppAlert from '@/components/ui/AppAlert.vue';
import TaskColumn from '@/features/tasks/TaskColumn.vue';
import TaskFilters from '@/features/tasks/TaskFilters.vue';
import { useSessionStore } from '@/stores/session';
import { useTasksStore, type TaskFilter } from '@/stores/tasks';

const COLUMNS = [TaskStatus.future, TaskStatus.stuck, TaskStatus.active, TaskStatus.done] as const;

const { t, te } = useI18n();
const route = useRoute();
const router = useRouter();
const session = useSessionStore();
const store = useTasksStore();

const teamId = computed(() => String(route.params.teamId));
const loaded = ref(false);
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
const meetingIds = computed(() => [
  ...new Set(store.tasks.map((task) => task.meetingId).filter((id): id is string => !!id)),
]);

const filterActive = computed(() => Object.values(filter.value).some(Boolean));
const isEmpty = computed(
  () => loaded.value && !store.error && store.tasks.length === 0 && !filterActive.value,
);
const errorText = computed(() => {
  const code = store.error;
  if (!code) return '';
  return te(`errors.${code}`) ? t(`errors.${code}`) : t('errors.unknown');
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
      <div class="board__grid">
        <TaskColumn
          v-for="status in COLUMNS"
          :key="status"
          :status="status"
          :tasks="store.columns[status]"
          :people="people"
        />
      </div>
    </div>
  </div>
</template>

<style scoped>
.board {
  display: flex;
  flex-direction: column;
  gap: 20px;
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
