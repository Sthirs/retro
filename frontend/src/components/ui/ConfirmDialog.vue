<script setup lang="ts">
/** Asks for confirmation before a consequential action. */
import AppButton from '@/components/ui/AppButton.vue';
import AppDialog from '@/components/ui/AppDialog.vue';
import AppAlert from '@/components/ui/AppAlert.vue';
import { useI18n } from 'vue-i18n';

withDefaults(
  defineProps<{
    open: boolean;
    title: string;
    text: string;
    confirmLabel: string;
    danger?: boolean;
    busy?: boolean;
    error?: string | null;
  }>(),
  { danger: false, busy: false, error: null },
);
const emit = defineEmits<{ close: []; confirm: [] }>();
const { t } = useI18n();
</script>

<template>
  <AppDialog :open="open" :title="title" :width="460" @close="emit('close')">
    <p class="text">{{ text }}</p>
    <AppAlert v-if="error">{{ error }}</AppAlert>
    <template #footer>
      <AppButton variant="ghost" autofocus @click="emit('close')">{{
        t('common.cancel')
      }}</AppButton>
      <AppButton :variant="danger ? 'danger' : 'primary'" :disabled="busy" @click="emit('confirm')">
        {{ confirmLabel }}
      </AppButton>
    </template>
  </AppDialog>
</template>

<style scoped>
.text {
  margin: 0;
  color: var(--muted);
}
</style>
