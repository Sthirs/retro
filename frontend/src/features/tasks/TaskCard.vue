<script setup lang="ts">
/** One task on the board (spec §10.1): date, content, author. */
import { ref } from 'vue';
import { useI18n } from 'vue-i18n';

import { errorCode, type PersonDto, type TaskDto } from '@/api/client';
import RichText from '@/components/RichText.vue';
import ConfirmDialog from '@/components/ui/ConfirmDialog.vue';
import UserAvatar from '@/components/UserAvatar.vue';
import EditTaskDialog from '@/features/tasks/EditTaskDialog.vue';
import TaskMenu from '@/features/tasks/TaskMenu.vue';
import { formatDayMonth } from '@/lib/time';
import { useTasksStore } from '@/stores/tasks';

const props = defineProps<{ task: TaskDto; people: PersonDto[] }>();
const { t, te } = useI18n();
const store = useTasksStore();

const editing = ref(false);
const deleting = ref(false);
const deleteBusy = ref(false);
const deleteError = ref<string | null>(null);

function askDelete(): void {
  deleteError.value = null;
  deleting.value = true;
}

async function confirmDelete(): Promise<void> {
  if (deleteBusy.value) return;
  deleteBusy.value = true;
  deleteError.value = null;
  try {
    await store.remove(props.task.id);
    deleting.value = false;
  } catch (e) {
    const code = errorCode(e);
    deleteError.value = te(`errors.${code}`) ? t(`errors.${code}`) : t('errors.unknown');
    // The dialog shows it: keep the board's own alert quiet.
    store.error = null;
  } finally {
    deleteBusy.value = false;
  }
}
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
      <TaskMenu :task="task" @edit="editing = true" @delete="askDelete" />
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
    <EditTaskDialog :open="editing" :task="task" :members="people" @close="editing = false" />
    <ConfirmDialog
      :open="deleting"
      :title="t('tasks.deleteTitle')"
      :text="t('tasks.deleteText')"
      :confirm-label="t('tasks.delete')"
      danger
      :busy="deleteBusy"
      :error="deleteError"
      @close="deleting = false"
      @confirm="confirmDelete"
    />
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
