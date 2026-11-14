<script setup lang="ts">
/** One task on the board (spec §10.1): date, content, author, actions menu. */
import { useI18n } from 'vue-i18n';

import type { TaskDto, TaskStatus } from '@/api/client';
import RichText from '@/components/RichText.vue';
import UserAvatar from '@/components/UserAvatar.vue';
import TaskMenu from '@/features/tasks/TaskMenu.vue';
import { formatDayMonth } from '@/lib/time';

const props = defineProps<{ task: TaskDto }>();
// The dialogs live outside the draggable list (see TasksBoardView): the card only reports.
const emit = defineEmits<{
  edit: [task: TaskDto];
  delete: [task: TaskDto];
  moved: [status: TaskStatus];
}>();
const { t } = useI18n();
</script>

<template>
  <article class="card" :data-id="task.id">
    <div class="card__meta">
      <span
        >{{ formatDayMonth(task.createdAt)
        }}<template v-if="task.meetingId">
          · <span class="card__meeting">{{ t('tasks.meeting') }}</span></template
        ></span
      >
      <TaskMenu
        :task="task"
        @edit="emit('edit', props.task)"
        @delete="emit('delete', props.task)"
        @moved="emit('moved', $event)"
      />
    </div>
    <RichText :html="task.contentHtml" class="card__content" />
    <span class="card__author">
      <UserAvatar
        :user-id="task.author.id"
        :name="task.author.displayName"
        :picture-version="task.author.pictureVersion"
        :size="22"
        ring="var(--card)"
      />
      {{ task.author.displayName }}
    </span>
  </article>
</template>

<style scoped>
.card {
  position: relative;
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 12px 14px;
  background: var(--card);
  border: 1px solid var(--line);
  border-radius: var(--radius-control);
  box-shadow: var(--shadow-card);
  cursor: grab;
}
.card__meta {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  font-size: 12px;
  color: var(--muted);
}
.card__meeting {
  color: var(--accent-text);
  font-weight: 600;
}
.card__content {
  font-size: 14px;
  line-height: 1.5;
  color: var(--text);
}
.card__author {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
  color: var(--muted);
}
</style>
