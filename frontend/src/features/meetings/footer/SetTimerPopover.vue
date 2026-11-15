<script setup lang="ts">
/**
 * The Timer popover (Timer artboard): presets of 3, 5, 10 and 15 minutes or
 * a custom `mm:ss`, then Start. Everyone sees the same countdown; it never
 * moves the phase on (spec §8.2).
 */
import { computed, ref, useId } from 'vue';
import { useI18n } from 'vue-i18n';

import type { TimerAction } from '@/api/client';
import AppIcon from '@/components/AppIcon.vue';
import AppButton from '@/components/ui/AppButton.vue';
import { formatClock, parseDuration } from '@/lib/clock';

const props = defineProps<{ start: (action: TimerAction) => Promise<boolean> }>();
const emit = defineEmits<{ close: []; started: [] }>();
const { t } = useI18n();
const uid = useId();

const PRESETS = [3, 5, 10, 15];
const custom = ref('05:00');
const invalid = ref(false);
const seconds = computed(() => parseDuration(custom.value));

function pick(minutes: number): void {
  custom.value = formatClock(minutes * 60_000);
  invalid.value = false;
}

async function submit(): Promise<void> {
  if (seconds.value === null) {
    invalid.value = true;
    return;
  }
  if (await props.start({ action: 'start', durationSeconds: seconds.value })) emit('started');
}
</script>

<template>
  <div class="pop" role="dialog" :aria-labelledby="`${uid}-title`">
    <div class="pop__head">
      <h2 :id="`${uid}-title`" class="pop__title">{{ t('timer.title') }}</h2>
      <button
        type="button"
        class="pop__close"
        :aria-label="t('timer.close')"
        @click="emit('close')"
      >
        <AppIcon name="close" :size="18" />
      </button>
    </div>
    <div class="pop__presets">
      <button
        v-for="minutes in PRESETS"
        :key="minutes"
        type="button"
        class="pop__preset"
        :aria-pressed="seconds === minutes * 60"
        @click="pick(minutes)"
      >
        {{ t('timer.minutes', { n: minutes }) }}
      </button>
    </div>
    <form class="pop__field" novalidate @submit.prevent="submit">
      <label class="pop__label" :for="`${uid}-custom`">{{ t('timer.custom') }}</label>
      <input
        :id="`${uid}-custom`"
        v-model="custom"
        class="pop__input"
        type="text"
        inputmode="numeric"
        autocomplete="off"
        :aria-invalid="invalid"
        :aria-describedby="`${uid}-help`"
        @input="invalid = false"
      />
      <span v-if="invalid" class="pop__error" role="alert">{{ t('timer.invalid') }}</span>
      <span :id="`${uid}-help`" class="pop__help">{{ t('timer.help') }}</span>
      <AppButton class="pop__start" type="submit" variant="primary" size="touch" block>
        <AppIcon name="play" :size="15" />{{ t('timer.start') }}
      </AppButton>
    </form>
  </div>
</template>

<style scoped>
.pop {
  display: flex;
  flex-direction: column;
  gap: 14px;
  box-sizing: border-box;
  width: min(320px, calc(100vw - 32px));
  padding: 16px;
  border: 1px solid var(--line);
  border-radius: var(--radius-panel);
  background: var(--surface);
  box-shadow: var(--shadow-dialog);
}
.pop__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.pop__title {
  margin: 0;
  color: var(--text);
  font-size: 16px;
  font-weight: 700;
  letter-spacing: -0.01em;
}
.pop__close {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 36px;
  height: 36px;
  padding: 0;
  border: none;
  border-radius: var(--radius-control);
  background: transparent;
  color: var(--muted);
  cursor: pointer;
}
.pop__presets {
  display: flex;
  gap: 6px;
}
.pop__preset {
  flex: 1 1 0;
  height: 40px;
  border: 1px solid var(--line);
  border-radius: var(--radius-control);
  background: transparent;
  color: var(--text);
  font: inherit;
  font-size: 14px;
  font-weight: 600;
  cursor: pointer;
}
.pop__preset[aria-pressed='true'] {
  border-color: var(--accent);
  background: var(--accent-soft);
}
.pop__field {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.pop__label {
  color: var(--text);
  font-size: 13px;
  font-weight: 600;
}
.pop__input {
  box-sizing: border-box;
  height: 44px;
  padding: 0 14px;
  border: 1px solid var(--line);
  border-radius: var(--radius-control);
  background: var(--surface);
  color: var(--text);
  font: inherit;
  font-size: 14px;
}
.pop__input[aria-invalid='true'] {
  border-color: var(--danger);
}
.pop__error {
  color: var(--danger);
  font-size: 12px;
}
.pop__help {
  color: var(--muted);
  font-size: 12px;
}
.pop__start {
  margin-top: 8px;
}
</style>
