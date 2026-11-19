<script setup lang="ts">
/**
 * One task of the Discuss Tasks panel (Discuss artboard, spec §9.5): author
 * and time (or edit time) with Edit and Delete in its header, then its rich
 * text. Edit opens the editor in place (Ctrl/Cmd + Enter saves, Esc
 * cancels); Delete asks first. While someone else edits it, it is outlined
 * with their name. Anyone in the meeting may edit or delete any task.
 */
import { computed, onBeforeUnmount, ref } from 'vue';
import { useI18n } from 'vue-i18n';

import type { PersonDto, TaskDto } from '@/api/client';
import AppIcon from '@/components/AppIcon.vue';
import RichText from '@/components/RichText.vue';
import RichTextEditor from '@/components/RichTextEditor.vue';
import UserAvatar from '@/components/UserAvatar.vue';
import AppButton from '@/components/ui/AppButton.vue';
import ConfirmDialog from '@/components/ui/ConfirmDialog.vue';
import WritingDots from '@/features/meetings/discuss/WritingDots.vue';
import { firstName } from '@/features/meetings/firstName';
import { plainText } from '@/features/meetings/group/plainText';
import { formatTime } from '@/lib/time';
import { useErrorText } from '@/composables/useErrorText';
import { useMeetingStore } from '@/stores/meeting';
import { hasContent } from '@/stores/reflectDrafts';

const props = defineProps<{ task: TaskDto; readonly: boolean; members: PersonDto[] }>();
const { t } = useI18n();
const store = useMeetingStore();
const errorText = useErrorText();

/** The first words of the task, to tell its buttons apart. */
const LABEL_CHARS = 30;
const short = computed(() => {
  const words = plainText(props.task.contentHtml, t('group.image')).split(' ');
  let text = words[0] ?? '';
  for (const word of words.slice(1)) {
    if (text.length + 1 + word.length > LABEL_CHARS) break;
    text += ` ${word}`;
  }
  return text;
});

const meta = computed(() =>
  props.task.editedAt
    ? t('discuss.tasks.metaEdited', {
        name: props.task.author.displayName,
        time: formatTime(props.task.editedAt),
      })
    : t('discuss.tasks.meta', {
        name: props.task.author.displayName,
        time: formatTime(props.task.createdAt),
      }),
);

/** Someone else with the editor open on it. */
const editor = computed(() => store.taskEditors.get(props.task.id) ?? null);
const editing = computed(() => store.editingTaskId === props.task.id);
const draft = ref('');
const saving = ref(false);
const uploading = ref(false);
const canSave = computed(() => hasContent(draft.value) && !saving.value && !uploading.value);

function startEdit(): void {
  draft.value = props.task.contentHtml;
  store.editingTaskId = props.task.id;
}

function stopEdit(): void {
  if (store.editingTaskId === props.task.id) store.editingTaskId = null;
}

async function save(): Promise<void> {
  if (!canSave.value) return;
  saving.value = true;
  try {
    if (await store.updateTask(props.task.id, draft.value)) stopEdit();
  } finally {
    saving.value = false;
  }
}

const confirming = ref(false);
const deleting = ref(false);
const deleteError = ref<string | null>(null);

async function confirmDelete(): Promise<void> {
  deleting.value = true;
  deleteError.value = null;
  try {
    if (await store.deleteTask(props.task.id)) confirming.value = false;
    else deleteError.value = errorText(store.actionError ?? 'unknown');
  } finally {
    deleting.value = false;
  }
}

// Gone (deleted, or another group on screen): its editor closes.
onBeforeUnmount(stopEdit);
</script>

<template>
  <li
    class="task"
    :class="{ 'task--active': editing || editor }"
    data-testid="discuss-task"
    :data-editing="editing || undefined"
  >
    <div class="task__head">
      <span class="task__meta">
        <UserAvatar
          :user-id="task.author.id"
          :name="task.author.displayName"
          :picture-version="task.author.pictureVersion"
          :size="20"
          ring="var(--card)"
        />
        {{ meta }}
      </span>
      <span v-if="!readonly && !editing" class="task__actions">
        <button
          type="button"
          class="task__btn"
          :aria-label="t('discuss.tasks.edit', { task: short })"
          @click="startEdit"
        >
          <AppIcon name="pencil" :size="18" />
        </button>
        <button
          type="button"
          class="task__btn task__btn--danger"
          :aria-label="t('discuss.tasks.delete', { task: short })"
          @click="
            deleteError = null;
            confirming = true;
          "
        >
          <AppIcon name="trash" :size="18" />
        </button>
      </span>
    </div>
    <div v-if="editing" class="task__editor">
      <RichTextEditor
        v-model="draft"
        kind="task"
        shortcuts
        :framed="false"
        autofocus
        :members="members"
        :readonly="saving"
        :team-id="store.meeting?.teamId"
        :label="t('discuss.tasks.editEditor')"
        @submit="save"
        @cancel="stopEdit"
        @uploading="uploading = $event"
      />
      <div class="task__foot">
        <span class="task__hint">{{ t('discuss.tasks.addHint') }}</span>
        <span class="task__buttons">
          <AppButton variant="ghost" size="compact" :disabled="saving" @click="stopEdit">
            {{ t('discuss.tasks.cancel') }}
          </AppButton>
          <AppButton variant="primary" size="compact" :disabled="!canSave" @click="save">
            {{ t('discuss.tasks.save') }}
          </AppButton>
        </span>
      </div>
    </div>
    <RichText v-else :html="task.contentHtml" class="task__content" />
    <span v-if="editor && !editing" class="task__editing" role="status">
      <UserAvatar
        :user-id="editor.id"
        :name="editor.displayName"
        :picture-version="editor.pictureVersion"
        :size="18"
        ring="var(--card)"
      />
      {{ t('discuss.tasks.editing', { name: firstName(editor.displayName) }) }}
      <WritingDots />
    </span>
    <ConfirmDialog
      v-if="confirming"
      open
      :title="t('tasks.deleteTitle')"
      :text="t('tasks.deleteText')"
      :confirm-label="t('tasks.delete')"
      danger
      :busy="deleting"
      :error="deleteError"
      @close="confirming = false"
      @confirm="confirmDelete"
    />
  </li>
</template>

<style scoped>
.task {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 10px 12px 12px 14px;
  border: 1px solid var(--line);
  border-radius: var(--radius-control);
  background: var(--card);
}
/* Edited here or elsewhere: the 2 px accent frame; padding keeps the text still. */
.task--active {
  padding: 9px 11px 11px 13px;
  border: 2px solid var(--accent);
}
.task__head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 8px;
}
.task__meta {
  display: flex;
  align-items: center;
  gap: 8px;
  padding-top: 4px;
  color: var(--muted);
  font-size: 12px;
}
.task__actions {
  display: flex;
  flex: none;
  gap: 2px;
  margin: -4px -6px 0 0;
}
.task__btn {
  position: relative;
  display: inline-flex;
  flex: none;
  align-items: center;
  justify-content: center;
  width: 30px;
  height: 30px;
  padding: 0;
  border: 0;
  border-radius: var(--radius-control);
  background: transparent;
  color: var(--muted);
  cursor: pointer;
}
.task__btn::before {
  /* 44 px touch target around the 30 px control. */
  content: '';
  position: absolute;
  inset: -7px;
}
.task__btn:hover {
  background: var(--accent-soft);
  color: var(--text);
}
.task__btn--danger,
.task__btn--danger:hover {
  color: var(--danger);
}
.task__content {
  font-size: 14px;
  line-height: 1.5;
  overflow-wrap: anywhere;
}
.task__editor {
  display: flex;
  flex-direction: column;
  margin: 0 -11px -11px -13px;
  border-top: 1px solid var(--line);
}
.task__foot {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 6px 8px 8px 12px;
  border-top: 1px solid var(--line);
}
.task__hint {
  color: var(--muted);
  font-size: 13px;
  font-weight: 600;
}
.task__buttons {
  display: flex;
  gap: 8px;
}
.task__editing {
  display: flex;
  align-items: center;
  gap: 8px;
  color: var(--accent-text);
  font-size: 12px;
  font-weight: 600;
}
</style>
