<script setup lang="ts">
/** A task board status as a coloured pill (Tasks, Summary artboards). */
import { useI18n } from 'vue-i18n';

import type { TaskStatus } from '@/api/client';

defineProps<{ status: TaskStatus }>();
const { t } = useI18n();
</script>

<template>
  <span class="pill" :class="`pill--${status}`"
    ><slot>{{ t(`tasks.columns.${status}`) }}</slot></span
  >
</template>

<style scoped>
.pill {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 3px 10px;
  border-radius: var(--radius-pill);
  font-size: 12px;
  font-weight: 700;
  white-space: nowrap;
}
.pill--future {
  background: var(--tag-neutral);
  color: var(--muted);
}
.pill--stuck {
  background: var(--col-delta-bg);
  color: var(--col-delta-fg);
}
.pill--active {
  background: var(--accent-soft);
  color: var(--accent-text);
}
.pill--done {
  background: var(--col-plus-bg);
  color: var(--col-plus-fg);
}
</style>
