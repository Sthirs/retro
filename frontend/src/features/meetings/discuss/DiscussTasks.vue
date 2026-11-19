<script setup lang="ts">
/**
 * The Tasks panel of Discuss (Discuss artboard, spec §9.5): the tasks
 * written for the group on screen, and, on the current group, a composer
 * (rich text with images and @mentions of team members). New tasks land in
 * the Active column of the board. Ctrl/Cmd + Enter adds.
 */
import { computed, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';

import { api, type PersonDto } from '@/api/client';
import RichText from '@/components/RichText.vue';
import RichTextEditor from '@/components/RichTextEditor.vue';
import UserAvatar from '@/components/UserAvatar.vue';
import AppIcon from '@/components/AppIcon.vue';
import AppButton from '@/components/ui/AppButton.vue';
import { formatTime } from '@/lib/time';
import { useMeetingStore } from '@/stores/meeting';
import { hasContent } from '@/stores/reflectDrafts';

const props = defineProps<{ groupId: string; readonly: boolean }>();
const { t } = useI18n();
const store = useMeetingStore();

const tasks = computed(() =>
  (store.meeting?.tasks ?? []).filter((task) => task.groupId === props.groupId),
);

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
        :members="members"
        :readonly="busy"
        :team-id="store.meeting?.teamId"
        :label="t('discuss.tasks.editor')"
        :placeholder="t('discuss.tasks.placeholder')"
        @submit="add"
        @uploading="uploading = $event"
      />
      <div class="composer__foot">
        <AppButton variant="primary" size="compact" :disabled="!canAdd" @click="add">
          {{ t('discuss.tasks.add') }}
        </AppButton>
      </div>
    </div>
    <p v-if="tasks.length === 0" class="tasks__none">{{ t('discuss.tasks.none') }}</p>
    <ul v-else class="tasks__list">
      <li v-for="task in tasks" :key="task.id" class="task" data-testid="discuss-task">
        <RichText :html="task.contentHtml" class="task__content" />
        <span class="task__meta">
          <UserAvatar
            :user-id="task.author.id"
            :name="task.author.displayName"
            :picture-version="task.author.pictureVersion"
            :size="20"
            ring="var(--card)"
          />
          {{
            t('discuss.tasks.meta', {
              name: task.author.displayName,
              time: formatTime(task.createdAt),
            })
          }}
        </span>
      </li>
    </ul>
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
.composer {
  display: flex;
  flex-direction: column;
  border: 2px solid var(--accent);
  border-radius: var(--radius-control);
  background: var(--card);
}
.composer__foot {
  display: flex;
  justify-content: flex-end;
  padding: 6px 8px 8px;
  border-top: 1px solid var(--line);
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
.task {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 12px 14px;
  border: 1px solid var(--line);
  border-radius: var(--radius-control);
  background: var(--card);
}
.task__content {
  font-size: 14px;
  line-height: 1.5;
  overflow-wrap: anywhere;
}
.task__meta {
  display: flex;
  align-items: center;
  gap: 8px;
  color: var(--muted);
  font-size: 12px;
}
</style>
