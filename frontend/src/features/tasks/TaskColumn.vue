<script setup lang="ts">
/** One board column: header with a count pill and a drop zone of cards. */
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';
import { VueDraggable } from 'vue-draggable-plus';

import type { TaskDto, TaskStatus } from '@/api/client';
import TaskCard from '@/features/tasks/TaskCard.vue';
import { useTasksStore } from '@/stores/tasks';

const props = defineProps<{ status: TaskStatus; tasks: TaskDto[] }>();
const emit = defineEmits<{
  edit: [task: TaskDto];
  delete: [task: TaskDto];
  moved: [status: TaskStatus];
}>();
const { t } = useI18n();
const store = useTasksStore();

const name = computed(() => t(`tasks.columns.${props.status}`));
// A copy, so the library never touches the store's arrays: the store stays the
// source of truth and its optimistic move re-renders the columns.
const cards = computed(() => [...props.tasks]);

interface DropEvent {
  newIndex?: number;
  item: { dataset: { id?: string } };
}

/** A card was dropped here, from another column or from within this one. */
function onDrop(event: DropEvent): void {
  const id = event.item.dataset.id;
  if (id === undefined || event.newIndex === undefined) return;
  void store.move(id, props.status, event.newIndex);
}
</script>

<template>
  <!-- tabindex -1: focus lands here after a card is moved or deleted from its menu. -->
  <section class="column" :aria-label="name" :data-column="status" tabindex="-1">
    <div class="column__header">
      <span class="pill" :class="`pill--${status}`">{{ name }}</span>
      <span class="column__count" data-testid="count">{{ tasks.length }}</span>
    </div>
    <VueDraggable
      :model-value="cards"
      group="tasks"
      :animation="150"
      ghost-class="column__ghost"
      class="column__list"
      :data-status="status"
      @add="onDrop"
      @update="onDrop"
    >
      <TaskCard
        v-for="task in cards"
        :key="task.id"
        :task="task"
        @edit="emit('edit', $event)"
        @delete="emit('delete', $event)"
        @moved="emit('moved', $event)"
      />
    </VueDraggable>
  </section>
</template>

<style scoped>
.column {
  display: flex;
  flex-direction: column;
  gap: 10px;
  min-width: 0;
  padding: 14px 12px;
  background: var(--well);
  border-radius: var(--radius-panel);
}
.column:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
}
.column__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 4px;
}
.pill {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 3px 10px;
  border-radius: var(--radius-pill);
  font-size: 12px;
  font-weight: 700;
  white-space: nowrap;
}
.pill--future {
  background: var(--well);
  color: var(--muted);
}
.pill--stuck {
  background: var(--col-delta-bg);
  color: var(--col-delta-fg);
}
.pill--active {
  background: var(--accent-soft);
  color: var(--accent-text);
}
.pill--done {
  background: var(--col-plus-bg);
  color: var(--col-plus-fg);
}
.column__count {
  font-size: 13px;
  font-weight: 700;
  color: var(--muted);
}
.column__list {
  display: flex;
  flex-direction: column;
  gap: 10px;
  /* An empty column stays a drop target. */
  flex: 1 1 auto;
  min-height: 72px;
}
.column__list :deep(.column__ghost) {
  height: 72px;
  overflow: hidden;
  border: 2px dashed var(--accent);
  border-radius: var(--radius-control);
  background: var(--accent-soft);
  box-shadow: none;
}
.column__list :deep(.column__ghost > *) {
  visibility: hidden;
}
</style>
