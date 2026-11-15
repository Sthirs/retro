<script setup lang="ts">
/** Transfer facilitator role (Transfer artboard): pick an online participant who is still a team member. */
import { computed, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';

import { Role } from '@/api/client';
import PresenceAvatar from '@/components/PresenceAvatar.vue';
import AppAlert from '@/components/ui/AppAlert.vue';
import AppButton from '@/components/ui/AppButton.vue';
import AppDialog from '@/components/ui/AppDialog.vue';
import { useErrorText } from '@/composables/useErrorText';
import { firstName } from '@/features/meetings/firstName';
import { useMeetingStore } from '@/stores/meeting';

const props = defineProps<{ open: boolean }>();
const emit = defineEmits<{ close: [] }>();
const { t } = useI18n();
const errorText = useErrorText();
const store = useMeetingStore();

const candidates = computed(() =>
  (store.meeting?.participants ?? []).filter(
    // Someone who left the team cannot run the meeting (the server refuses it).
    (p) => p.online && p.role !== null && p.id !== store.meeting?.facilitatorId,
  ),
);
const selected = ref<string | null>(null);
const busy = ref(false);
const error = ref<string | null>(null);
const chosen = computed(() => candidates.value.find((p) => p.id === selected.value) ?? null);

watch(
  () => props.open,
  (open) => {
    if (open) {
      selected.value = null;
      error.value = null;
    }
  },
);
// Someone who went offline can no longer be picked.
watch(candidates, (list) => {
  if (selected.value && !list.some((p) => p.id === selected.value)) selected.value = null;
});

async function confirm(): Promise<void> {
  if (!chosen.value || busy.value) return;
  busy.value = true;
  error.value = null;
  try {
    if (await store.setFacilitator(chosen.value.id)) emit('close');
    else error.value = store.actionError;
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <AppDialog :open="open" :title="t('transfer.title')" :width="480" @close="emit('close')">
    <p class="text">{{ t('transfer.text') }}</p>
    <AppAlert v-if="error">{{ errorText(error) }}</AppAlert>
    <fieldset class="options">
      <legend class="options__legend">{{ t('transfer.online') }}</legend>
      <p v-if="!candidates.length" class="text">{{ t('transfer.none') }}</p>
      <label
        v-for="p in candidates"
        :key="p.id"
        class="option"
        :class="{ 'option--selected': selected === p.id }"
      >
        <input
          v-model="selected"
          class="option__radio"
          type="radio"
          name="facilitator"
          :value="p.id"
        />
        <PresenceAvatar
          :user-id="p.id"
          :name="p.displayName"
          :picture-version="p.pictureVersion"
          :online="p.online"
          :size="30"
        />
        <span class="option__name">{{ p.displayName }}</span>
        <span v-if="p.role === Role.admin" class="option__tag">{{ t('transfer.admin') }}</span>
      </label>
    </fieldset>
    <template #footer>
      <AppButton variant="ghost" @click="emit('close')">{{ t('common.cancel') }}</AppButton>
      <AppButton variant="primary" :disabled="!chosen || busy" @click="confirm">
        {{
          chosen
            ? t('transfer.confirm', { name: firstName(chosen.displayName) })
            : t('transfer.confirmEmpty')
        }}
      </AppButton>
    </template>
  </AppDialog>
</template>

<style scoped>
.text {
  margin: 0;
  color: var(--muted);
  font-size: 14px;
  line-height: 1.5;
}
.options {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin: 0;
  padding: 0;
  border: none;
}
.options__legend {
  margin-bottom: 8px;
  padding: 0;
  font-size: 13px;
  font-weight: 600;
}
.option {
  display: flex;
  align-items: center;
  gap: 12px;
  min-height: 52px;
  padding: 0 12px;
  border: 1px solid var(--line);
  border-radius: var(--radius-control);
  cursor: pointer;
}
.option--selected {
  border-color: var(--accent);
  background: var(--accent-soft);
}
.option__radio {
  width: 18px;
  height: 18px;
  margin: 0;
  accent-color: var(--accent);
}
.option__name {
  flex: 1 1 auto;
  font-size: 14px;
  font-weight: 600;
}
.option__tag {
  padding: 3px 10px;
  border-radius: var(--radius-pill);
  background: var(--tag-neutral);
  color: var(--muted);
  font-size: 12px;
  font-weight: 700;
}
</style>
