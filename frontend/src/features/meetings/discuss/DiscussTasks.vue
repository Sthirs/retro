<script setup lang="ts">
/**
 * The Tasks panel of Discuss (Discuss artboard, spec §9.5): the tasks
 * written for the group on screen, and, on the current group, a composer
 * (rich text with images and @mentions of team members). New tasks land in
 * the Active column of the board. Ctrl/Cmd + Enter adds. Who else is
 * writing a task for the group shows as a placeholder on top of the list
 * (ADR-0015); anyone in the meeting can edit or delete any task.
 */
import { computed, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';

import { api, type PersonDto } from '@/api/client';
import RichTextEditor from '@/components/RichTextEditor.vue';
import AppIcon from '@/components/AppIcon.vue';
import AppButton from '@/components/ui/AppButton.vue';
import DiscussTaskItem from '@/features/meetings/discuss/DiscussTaskItem.vue';
import TaskWritingPlaceholder from '@/features/meetings/discuss/TaskWritingPlaceholder.vue';
import { useMeetingStore } from '@/stores/meeting';
import { hasContent } from '@/stores/reflectDrafts';

const props = defineProps<{ groupId: string; readonly: boolean }>();
const { t } = useI18n();
const store = useMeetingStore();

const tasks = computed(() =>
  (store.meeting?.tasks ?? []).filter((task) => task.groupId === props.groupId),
);
/** Others writing a new task for this group right now. */
const writers = computed(() => store.taskWritersByGroup.get(props.groupId) ?? []);

/** The team's members, the @-mention candidates. */
const members = ref<PersonDto[]>([]);
watch(
  () => store.meeting?.teamId,
  async (teamId) => {
    members.value = [];
    if (!teamId) return;
    try {
      const res = await api.listMembers(teamId);
      if (res.status === 200 && store.meeting?.teamId === teamId) {
        members.value = res.data.map(({ user }) => ({
          id: user.id,
          displayName: user.displayName,
          pictureVersion: user.pictureVersion,
        }));
      }
    } catch {
      // Without them, mentions offer nobody; the task can still be written.
    }
  },
  { immediate: true },
);

/** Kept in the store per group: a move to another group does not lose it. */
const draft = computed({
  get: () => store.taskDrafts[props.groupId] ?? '',
  set: (html: string) => store.setTaskDraft(props.groupId, html),
});
const busy = ref(false);
const uploading = ref(false);
/**
 * Remounts the editor once a task is added, so it starts empty, and on
 * another group, so it shows that group's text.
 */
const editorKey = ref(0);
const canAdd = computed(() => hasContent(draft.value) && !busy.value && !uploading.value);

async function add(): Promise<void> {
  if (!canAdd.value) return;
  busy.value = true;
  // The group may change while the task is on its way: clear only its draft.
  const groupId = props.groupId;
  try {
    if (await store.createTask(groupId, draft.value)) {
      store.setTaskDraft(groupId, '');
      editorKey.value++;
    }
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <aside class="tasks" :aria-label="t('discuss.tasks.label')" data-testid="discuss-tasks">
    <div class="tasks__head">
      <h2 class="tasks__title">
        <AppIcon name="tasks" :size="17" />
        {{ t('discuss.tasks.title') }}
      </h2>
      <span class="tasks__sub">{{ t('discuss.tasks.subtitle') }}</span>
    </div>
    <div v-if="!readonly" class="composer">
      <RichTextEditor
        :key="`${groupId}:${editorKey}`"
        v-model="draft"
        kind="task"
        shortcuts
        :framed="false"
        :members="members"
        :readonly="busy"
        :team-id="store.meeting?.teamId"
        :label="t('discuss.tasks.editor')"
        :placeholder="t('discuss.tasks.placeholder')"
        @submit="add"
        @uploading="uploading = $event"
      />
      <div class="composer__foot">
        <span class="composer__hint">{{ t('discuss.tasks.addHint') }}</span>
        <AppButton variant="primary" size="compact" :disabled="!canAdd" @click="add">
          {{ t('discuss.tasks.add') }}
        </AppButton>
      </div>
    </div>
    <p v-if="tasks.length === 0 && writers.length === 0" class="tasks__none">
      {{ t('discuss.tasks.none') }}
    </p>
    <ul v-else class="tasks__list">
      <TaskWritingPlaceholder v-for="writer in writers" :key="`writing:${writer.id}`" :writer />
      <DiscussTaskItem v-for="task in tasks" :key="task.id" :task :readonly :members />
    </ul>
    <p v-if="!readonly" class="tasks__note">
      <AppIcon name="users" :size="15" />
      <span>{{ t('discuss.tasks.note') }}</span>
    </p>
  </aside>
</template>

<style scoped>
.tasks {
  display: flex;
  flex-direction: column;
  gap: 12px;
  min-width: 0;
  padding: 14px 12px;
  border: 1px solid var(--well-edge);
  border-radius: var(--radius-panel);
  background: var(--well);
}
.tasks__head {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 2px 2px 0;
}
.tasks__title {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: 0;
  font-size: 17px;
  font-weight: 700;
}
.tasks__sub {
  color: var(--muted);
  font-size: 13px;
  font-weight: 600;
}
/* One 2 px frame (the editor draws none of its own): accent while it is
   being written in, a plain line at rest. */
.composer {
  display: flex;
  flex-direction: column;
  border: 2px solid var(--line);
  border-radius: var(--radius-control);
  background: var(--card);
}
.composer:focus-within {
  border-color: var(--accent);
}
.composer__foot {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 6px 8px 8px 12px;
  border-top: 1px solid var(--line);
}
.composer__hint {
  color: var(--muted);
  font-size: 13px;
  font-weight: 600;
}
.tasks__none {
  margin: 0;
  padding: 0 2px;
  color: var(--muted);
  font-size: 13px;
}
.tasks__list {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin: 0;
  padding: 0;
  list-style: none;
}
.tasks__note {
  display: flex;
  gap: 8px;
  margin: 0;
  color: var(--muted);
  font-size: 12px;
  line-height: 1.45;
}
</style>
