<script setup lang="ts">
/**
 * The facilitator's timer in the bottom bar (Reflect and Timer artboards):
 * "Set timer" opens the Timer popover; while a timer is set, its countdown
 * with pause or resume, "+1 min" and stop.
 */
import { computed, ref } from 'vue';
import { useI18n } from 'vue-i18n';

import { TimerStatus, type TimerAction } from '@/api/client';
import AppIcon from '@/components/AppIcon.vue';
import AppButton from '@/components/ui/AppButton.vue';
import { useCountdown } from '@/composables/useCountdown';
import { usePopover } from '@/composables/usePopover';
import SetTimerPopover from '@/features/meetings/footer/SetTimerPopover.vue';
import { useMeetingStore } from '@/stores/meeting';

const { t } = useI18n();
const store = useMeetingStore();
const timer = computed(() => store.meeting?.timer ?? null);
const { display } = useCountdown(timer, store.serverTime);
const paused = computed(() => timer.value?.status === TimerStatus.paused);

const root = ref<HTMLElement | null>(null);
const trigger = ref<HTMLElement | null>(null);
const { open, toggle, close } = usePopover(root, trigger);
const busy = ref(false);

async function run(action: TimerAction): Promise<boolean> {
  if (busy.value) return false;
  busy.value = true;
  try {
    return await store.setTimer(action);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div ref="root" class="timer">
    <div v-if="timer" class="timer__box" data-testid="timer">
      <AppIcon name="clock" :size="18" />
      <span class="timer__time" role="timer" :aria-label="t('timer.remaining', { time: display })">
        {{ display }}
      </span>
      <span v-if="paused" class="timer__paused">{{ t('timer.paused') }}</span>
      <button
        type="button"
        class="timer__btn"
        :aria-label="paused ? t('timer.resume') : t('timer.pause')"
        :disabled="busy"
        @click="run({ action: paused ? 'resume' : 'pause' })"
      >
        <AppIcon :name="paused ? 'play' : 'pause'" :size="16" />
      </button>
      <button
        type="button"
        class="timer__btn timer__add"
        :aria-label="t('timer.addMinuteLabel')"
        :disabled="busy"
        @click="run({ action: 'add', seconds: 60 })"
      >
        {{ t('timer.addMinute') }}
      </button>
      <button
        type="button"
        class="timer__btn"
        :aria-label="t('timer.stop')"
        :disabled="busy"
        @click="run({ action: 'stop' })"
      >
        <AppIcon name="stop" :size="15" />
      </button>
    </div>
    <span v-else ref="trigger" class="timer__trigger">
      <AppButton size="touch" aria-haspopup="dialog" :aria-expanded="open" @click="toggle">
        <AppIcon name="clock" :size="18" />{{ t('timer.set') }}
      </AppButton>
    </span>
    <SetTimerPopover
      v-if="open && !timer"
      class="timer__popover"
      :start="run"
      @close="close(true)"
      @started="close()"
    />
  </div>
</template>

<style scoped>
.timer {
  position: relative;
}
.timer__trigger {
  display: inline-flex;
}
.timer__box {
  display: flex;
  align-items: center;
  gap: 2px;
  padding: 4px 4px 4px 14px;
  border: 1px solid var(--line);
  border-radius: var(--radius-control);
  background: var(--surface);
  color: var(--text);
}
.timer__time {
  margin: 0 8px 0 6px;
  font-size: 20px;
  font-weight: 700;
  font-variant-numeric: tabular-nums;
}
.timer__paused {
  margin-right: 6px;
  color: var(--muted);
  font-size: 13px;
}
.timer__btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 36px;
  height: 36px;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text);
  font: inherit;
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
}
.timer__btn:hover {
  background: var(--well);
}
.timer__add {
  padding: 0 10px;
}
.timer__popover {
  position: absolute;
  bottom: calc(100% + 24px);
  left: 0;
  z-index: 3;
}
</style>
