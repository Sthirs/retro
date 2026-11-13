<script setup lang="ts">
/** Read-only value with a Copy button that confirms the copy for a moment. */
import { onBeforeUnmount, ref } from 'vue';
import { useI18n } from 'vue-i18n';

import AppIcon from '@/components/AppIcon.vue';
import AppButton from '@/components/ui/AppButton.vue';
import { copyText } from '@/lib/clipboard';

const props = withDefaults(
  defineProps<{
    id: string;
    value: string;
    label: string;
    /** Show `label` above the field; otherwise it only names the input. */
    showLabel?: boolean;
    buttonLabel?: string;
    variant?: 'primary' | 'secondary';
    dense?: boolean;
  }>(),
  { showLabel: true, buttonLabel: undefined, variant: 'secondary', dense: false },
);
const { t } = useI18n();
const copied = ref(false);
let timer: ReturnType<typeof setTimeout> | undefined;

async function copy(): Promise<void> {
  if (!(await copyText(props.value))) return;
  copied.value = true;
  clearTimeout(timer);
  timer = setTimeout(() => (copied.value = false), 2000);
}

function selectAll(event: FocusEvent): void {
  (event.target as HTMLInputElement).select();
}

onBeforeUnmount(() => clearTimeout(timer));
</script>

<template>
  <div class="copy">
    <label v-if="showLabel" :for="id" class="copy__label">{{ label }}</label>
    <div class="copy__row">
      <input
        :id="id"
        class="copy__input"
        :class="{ 'copy__input--dense': dense }"
        type="text"
        readonly
        :value="value"
        :aria-label="showLabel ? undefined : label"
        @focus="selectAll"
      />
      <AppButton :variant="variant" :size="dense ? 'default' : 'touch'" @click="copy">
        <AppIcon :name="copied ? 'check' : 'copy'" :size="16" />
        {{ copied ? t('common.copied') : (buttonLabel ?? t('common.copy')) }}
      </AppButton>
    </div>
    <span class="visually-hidden" aria-live="polite">{{ copied ? t('common.copied') : '' }}</span>
  </div>
</template>

<style scoped>
.copy {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.copy__label {
  font-size: 13px;
  font-weight: 600;
}
.copy__row {
  display: flex;
  gap: 8px;
}
.copy__input {
  flex: 1 1 auto;
  min-width: 0;
  height: 44px;
  padding: 0 14px;
  border-radius: var(--radius-control);
  border: 1px solid var(--line);
  background: var(--card);
  color: var(--text);
  font-family: var(--font-sans);
  font-size: 14px;
}
.copy__input--dense {
  height: 40px;
  padding: 0 12px;
  font-size: 13px;
}
</style>
