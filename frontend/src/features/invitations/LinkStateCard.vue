<script setup lang="ts">
/** Outcome of opening an invitation or join link (LinkStates artboard). */
import AppIcon from '@/components/AppIcon.vue';
import type { IconName } from '@/components/icons';

withDefaults(
  defineProps<{
    title: string;
    text: string;
    icon: IconName;
    tone?: 'success' | 'warning' | 'accent';
  }>(),
  { tone: 'warning' },
);
</script>

<template>
  <section class="state" aria-live="polite">
    <span class="state__icon" :class="`state__icon--${tone}`" aria-hidden="true">
      <AppIcon :name="icon" :size="26" />
    </span>
    <div class="state__text">
      <h1 class="state__title">{{ title }}</h1>
      <p class="state__body">{{ text }}</p>
    </div>
    <div v-if="$slots.default" class="state__actions">
      <slot />
    </div>
  </section>
</template>

<style scoped>
.state {
  display: flex;
  flex-direction: column;
  gap: 16px;
  width: 100%;
  max-width: 480px;
  padding: 28px;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: var(--radius-panel);
  box-shadow: var(--shadow-panel);
}
.state__icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 52px;
  height: 52px;
  border-radius: 50%;
}
.state__icon--success {
  background: var(--col-plus-bg);
  color: var(--col-plus-fg);
}
.state__icon--warning {
  background: var(--col-delta-bg);
  color: var(--col-delta-fg);
}
.state__icon--accent {
  background: var(--accent-soft);
  color: var(--accent-text);
}
.state__text {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.state__title {
  margin: 0;
  font-size: 20px;
  font-weight: 700;
  letter-spacing: -0.01em;
}
.state__body {
  margin: 0;
  color: var(--muted);
}
</style>
