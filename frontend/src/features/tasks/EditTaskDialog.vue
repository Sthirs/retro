<script setup lang="ts">
/** Edit a task's text (spec §10.1): the rich-text editor in a dialog. */
import { ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';

import { errorCode, type PersonDto, type TaskDto } from '@/api/client';
import RichTextEditor from '@/components/RichTextEditor.vue';
import AppAlert from '@/components/ui/AppAlert.vue';
import AppButton from '@/components/ui/AppButton.vue';
import AppDialog from '@/components/ui/AppDialog.vue';
import { useTasksStore } from '@/stores/tasks';
import { useErrorText } from '@/composables/useErrorText';

const props = defineProps<{
  open: boolean;
  task: TaskDto;
  members: PersonDto[];
  /** The task's team, which images are uploaded to. */
  teamId: string;
}>();
const emit = defineEmits<{ close: [] }>();
const { t } = useI18n();
const errorText = useErrorText();
const store = useTasksStore();

const content = ref(props.task.contentHtml);
const error = ref<string | null>(null);
const busy = ref(false);
/** Images are still uploading: the text is not final yet. */
const uploading = ref(false);

// Every opening starts from the task as it is now.
watch(
  () => props.open,
  (open) => {
    if (!open) return;
    content.value = props.task.contentHtml;
    error.value = null;
  },
);

async function save(): Promise<void> {
  if (busy.value || uploading.value) return;
  busy.value = true;
  error.value = null;
  try {
    await store.update(props.task.id, content.value);
    emit('close');
  } catch (e) {
    const code = errorCode(e);
    error.value = errorText(code);
    // The dialog shows it: keep the board's own alert quiet.
    store.error = null;
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <AppDialog :open="open" :title="t('tasks.editTitle')" @close="emit('close')">
    <RichTextEditor
      v-model="content"
      :members="members"
      :team-id="teamId"
      :label="t('tasks.editLabel')"
      :readonly="busy"
      @uploading="uploading = $event"
    />
    <AppAlert v-if="error">{{ error }}</AppAlert>
    <template #footer>
      <AppButton variant="ghost" @click="emit('close')">{{ t('common.cancel') }}</AppButton>
      <AppButton variant="primary" :disabled="busy || uploading" @click="save">{{
        t('common.save')
      }}</AppButton>
    </template>
  </AppDialog>
</template>
