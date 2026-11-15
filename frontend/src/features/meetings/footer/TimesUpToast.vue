<script setup lang="ts">
/**
 * "Time's up" for everyone in the room (Group artboard; spec §8.2): shown
 * when the countdown reaches zero, until dismissed. It never moves the phase
 * on; the facilitator decides.
 */
import { computed, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';

import AppIcon from '@/components/AppIcon.vue';
import { useCountdown } from '@/composables/useCountdown';
import { firstName } from '@/features/meetings/firstName';
import { useMeetingStore } from '@/stores/meeting';

const { t } = useI18n();
const store = useMeetingStore();
const timer = computed(() => store.meeting?.timer ?? null);
const { expired } = useCountdown(timer, store.serverTime);

const KEY = 'retro.timesUpDismissed';
/** The `endsAt` whose notice was dismissed (kept for the tab's session). */
const dismissed = ref<string | null>(read());

function read(): string | null {
  try {
    return sessionStorage.getItem(`${KEY}.${store.meetingId}`);
  } catch {
    return null;
  }
}

watch(
  () => store.meetingId,
  () => (dismissed.value = read()),
);

const visible = computed(() => expired.value && dismissed.value !== timer.value?.endsAt);
const text = computed(() =>
  store.isFacilitator
    ? t('timer.timesUpYou')
    : t('timer.timesUp', { name: firstName(store.facilitator?.displayName ?? '') }),
);

function dismiss(): void {
  dismissed.value = timer.value?.endsAt ?? null;
  try {
    if (dismissed.value) sessionStorage.setItem(`${KEY}.${store.meetingId}`, dismissed.value);
  } catch {
    // Storage unavailable: dismissed for this page only.
  }
}
</script>

<template>
  <div class="times-up" role="status">
    <div v-if="visible" class="times-up__toast" data-testid="times-up">
      <AppIcon name="clock" :size="18" />
      <span>{{ text }}</span>
      <button
        type="button"
        class="times-up__close"
        :aria-label="t('timer.dismiss')"
        @click="dismiss"
      >
        <AppIcon name="close" :size="16" />
      </button>
    </div>
  </div>
</template>

<style scoped>
.times-up {
  position: fixed;
  bottom: 100px;
  left: 50%;
  z-index: 3;
  transform: translateX(-50%);
  max-width: calc(100vw - 32px);
}
.times-up__toast {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px 14px 12px 16px;
  border-radius: var(--radius-panel);
  background: var(--text);
  color: var(--bg);
  box-shadow: var(--shadow-menu);
  font-size: 14px;
  font-weight: 600;
}
.times-up__close {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 36px;
  min-height: 36px;
  padding: 4px;
  border: none;
  background: transparent;
  color: var(--bg);
  cursor: pointer;
}
</style>
