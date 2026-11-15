<script setup lang="ts">
/** Reflect to Discuss, until they are built: the room still works around them. */
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';

import AppIcon from '@/components/AppIcon.vue';
import { PHASE_ICONS } from '@/features/meetings/phaseIcons';
import { useMeetingStore } from '@/stores/meeting';

const { t } = useI18n();
const store = useMeetingStore();
const phase = computed(() => store.meeting!.phase);
</script>

<template>
  <section class="soon" :aria-label="t(`phases.${phase}`)">
    <span class="soon__icon" aria-hidden="true">
      <AppIcon :name="PHASE_ICONS[phase]" :size="26" />
    </span>
    <h2 class="soon__title">
      {{ t('meetingRoom.comingSoonTitle', { phase: t(`phases.${phase}`) }) }}
    </h2>
    <p class="soon__text">{{ t('meetingRoom.comingSoonText') }}</p>
  </section>
</template>

<style scoped>
.soon {
  display: flex;
  flex: 1 1 auto;
  flex-direction: column;
  align-items: flex-start;
  gap: 14px;
  padding: 32px;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: var(--radius-panel);
  box-shadow: var(--shadow-panel);
}
.soon__icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 52px;
  height: 52px;
  border-radius: 50%;
  background: var(--accent-soft);
  color: var(--accent-text);
}
.soon__title {
  margin: 0;
  font-size: 20px;
  font-weight: 700;
}
.soon__text {
  margin: 0;
  max-width: 60ch;
  color: var(--muted);
}
</style>
