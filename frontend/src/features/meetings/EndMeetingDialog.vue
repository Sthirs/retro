<script setup lang="ts">
/** End this meeting? (EndMeeting artboard). */
import { computed, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';

import AppIcon from '@/components/AppIcon.vue';
import AppAlert from '@/components/ui/AppAlert.vue';
import AppButton from '@/components/ui/AppButton.vue';
import AppDialog from '@/components/ui/AppDialog.vue';
import { useErrorText } from '@/composables/useErrorText';
import { useMeetingStore } from '@/stores/meeting';

const props = defineProps<{ open: boolean }>();
const emit = defineEmits<{ close: [] }>();
const { t } = useI18n();
const errorText = useErrorText();
const store = useMeetingStore();
const taskCount = computed(() => store.meeting?.taskCount ?? 0);
const busy = ref(false);
const error = ref<string | null>(null);

watch(
  () => props.open,
  (open) => {
    if (open) error.value = null;
  },
);

async function confirm(): Promise<void> {
  if (busy.value) return;
  busy.value = true;
  error.value = null;
  try {
    if (await store.end()) emit('close');
    else error.value = store.actionError;
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <AppDialog :open="open" :title="t('endMeeting.title')" :width="500" @close="emit('close')">
    <template #icon>
      <span class="end__badge" aria-hidden="true"><AppIcon name="flag" :size="20" /></span>
    </template>
    <AppAlert v-if="error">{{ errorText(error) }}</AppAlert>
    <ul class="list">
      <li class="list__item">
        <span class="list__icon"><AppIcon name="lock" :size="16" /></span>
        {{ t('endMeeting.readOnly') }}
      </li>
      <li class="list__item">
        <span class="list__icon"><AppIcon name="link" :size="16" /></span>
        {{ t('endMeeting.link') }}
      </li>
      <li v-if="taskCount > 0" class="list__item">
        <span class="list__icon"><AppIcon name="tasks" :size="16" /></span>
        <i18n-t keypath="endMeeting.tasks" :plural="taskCount" tag="span">
          <template #tasks>
            <strong>{{ t('meetings.tasks', { count: taskCount }, taskCount) }}</strong>
          </template>
        </i18n-t>
      </li>
    </ul>
    <template #footer>
      <AppButton variant="ghost" @click="emit('close')">{{ t('endMeeting.keepGoing') }}</AppButton>
      <AppButton variant="danger" :disabled="busy" @click="confirm">
        {{ t('endMeeting.confirm') }}
      </AppButton>
    </template>
  </AppDialog>
</template>

<style scoped>
.end__badge {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex: none;
  width: 40px;
  height: 40px;
  border-radius: 50%;
  background: var(--col-delta-bg);
  color: var(--danger);
}
.list {
  display: flex;
  flex-direction: column;
  gap: 10px;
  margin: 0;
  padding: 0;
  list-style: none;
}
.list__item {
  display: flex;
  gap: 10px;
  font-size: 14px;
  line-height: 1.45;
}
.list__icon {
  display: flex;
  flex: none;
  margin-top: 1px;
  color: var(--muted);
}
</style>
