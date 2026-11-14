<script setup lang="ts">
/** The team's task board (spec §10): four columns of cards, drag and drop. */
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { useRoute } from 'vue-router';

import { TaskStatus } from '@/api/client';
import AppIcon from '@/components/AppIcon.vue';
import AppAlert from '@/components/ui/AppAlert.vue';
import TaskColumn from '@/features/tasks/TaskColumn.vue';
import { useTasksStore, type TaskFilter } from '@/stores/tasks';

const COLUMNS = [TaskStatus.future, TaskStatus.stuck, TaskStatus.active, TaskStatus.done] as const;

const { t, te } = useI18n();
const route = useRoute();
const store = useTasksStore();

const teamId = computed(() => String(route.params.teamId));
const filter = ref<TaskFilter>({});
const loaded = ref(false);

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

// Until the real-time hub exists, the board refreshes when the window regains focus.
onMounted(() => {
  void load();
  window.addEventListener('focus', load);
});
onBeforeUnmount(() => window.removeEventListener('focus', load));

// Switching team keeps this view alive: show the new team's board.
watch(teamId, () => {
  loaded.value = false;
  store.reset();
  void load();
});
</script>

<template>
  <div class="board">
    <div class="board__header">
      <h1 class="board__title">{{ t('tasks.title') }}</h1>
      <p class="board__subtitle">{{ t('tasks.subtitle') }}</p>
    </div>

    <AppAlert v-if="errorText">{{ errorText }}</AppAlert>

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
