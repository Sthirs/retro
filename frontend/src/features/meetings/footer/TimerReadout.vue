<script setup lang="ts">
/** The countdown as everyone but the facilitator sees it (Group artboard): "04:32 left". */
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';

import { TimerStatus } from '@/api/client';
import AppIcon from '@/components/AppIcon.vue';
import { useCountdown } from '@/composables/useCountdown';
import { useMeetingStore } from '@/stores/meeting';

const { t } = useI18n();
const store = useMeetingStore();
const timer = computed(() => store.meeting?.timer ?? null);
const { display } = useCountdown(timer, store.serverTime);
</script>

<template>
  <span v-if="timer" class="readout" data-testid="timer">
    <AppIcon name="clock" :size="18" />
    <span class="readout__time" role="timer" :aria-label="t('timer.remaining', { time: display })">
      {{ display }}
    </span>
    <span class="readout__left">
      {{ timer.status === TimerStatus.paused ? t('timer.paused') : t('timer.left') }}
    </span>
  </span>
</template>

<style scoped>
.readout {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 14px;
  border-radius: var(--radius-control);
  background: var(--well);
  color: var(--text);
}
.readout__time {
  font-size: 20px;
  font-weight: 700;
  font-variant-numeric: tabular-nums;
}
.readout__left {
  color: var(--muted);
  font-size: 13px;
}
</style>
