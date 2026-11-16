<script setup lang="ts">
/** "I'm ready" / "You're ready" (Reflect and Group artboards): a toggle. */
import { ref } from 'vue';
import { useI18n } from 'vue-i18n';

import AppIcon from '@/components/AppIcon.vue';
import { useMeetingStore } from '@/stores/meeting';

const { t } = useI18n();
const store = useMeetingStore();
const busy = ref(false);

async function toggle(): Promise<void> {
  if (busy.value) return;
  busy.value = true;
  try {
    await store.setReady(!store.iAmReady);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <button
    type="button"
    class="ready-btn"
    :aria-pressed="store.iAmReady"
    :disabled="busy"
    @click="toggle"
  >
    <AppIcon name="check" :size="18" :stroke-width="2.5" />
    {{ store.iAmReady ? t('ready.youAreReady') : t('ready.ready') }}
  </button>
</template>

<style scoped>
.ready-btn {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  min-height: 48px;
  padding: 0 20px;
  border: 2px solid var(--success);
  border-radius: var(--radius-control);
  background: transparent;
  color: var(--text);
  font: inherit;
  font-size: 15px;
  font-weight: 700;
  cursor: pointer;
}
.ready-btn[aria-pressed='true'] {
  background: var(--success);
  color: var(--on-success);
}
.ready-btn:disabled {
  cursor: progress;
}
</style>
