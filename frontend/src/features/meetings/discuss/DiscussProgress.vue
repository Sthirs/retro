<script setup lang="ts">
/**
 * The groups in Discuss order, under the Discuss step of the sidebar
 * (Discuss artboard): vote total, name, and where the meeting stands (done,
 * Now, Next). Anyone can open a group to look at it; only the facilitator's
 * Next / Previous moves everyone (spec §9.5).
 */
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';

import AppIcon from '@/components/AppIcon.vue';
import { useGroupLabel } from '@/features/meetings/discuss/useGroupLabel';
import { useMeetingStore } from '@/stores/meeting';

const { t } = useI18n();
const store = useMeetingStore();
const labelOf = useGroupLabel();

type EntryState = 'done' | 'current' | 'next' | 'upcoming';
const entries = computed(() =>
  store.discussOrder.map((g, i) => {
    const current = store.currentIndex;
    let state: EntryState = 'upcoming';
    if (current >= 0 && i < current) state = 'done';
    else if (i === current) state = 'current';
    else if (i === current + 1) state = 'next';
    return {
      ...g,
      label: labelOf(g.groupId),
      state,
      viewing: store.previewing && g.groupId === store.viewingGroupId,
    };
  }),
);
</script>

<template>
  <ol class="progress" :aria-label="t('discuss.progress')">
    <li
      v-for="entry in entries"
      :key="entry.groupId"
      :aria-current="entry.state === 'current' ? 'true' : undefined"
    >
      <button
        type="button"
        class="progress__item"
        :class="[`progress__item--${entry.state}`, { 'progress__item--viewing': entry.viewing }]"
        :aria-pressed="entry.viewing || (!store.previewing && entry.state === 'current')"
        data-testid="discuss-entry"
        @click="store.viewGroup(entry.groupId)"
      >
        <span class="progress__votes" :aria-label="t('discuss.votesLabel', entry.votes)">
          {{ entry.votes }}
        </span>
        <span class="progress__name">{{ entry.label }}</span>
        <span v-if="entry.state === 'done'" class="progress__done" :aria-label="t('discuss.done')">
          <AppIcon name="check" :size="16" />
        </span>
        <span v-else-if="entry.state === 'current'" class="progress__now">
          {{ t('discuss.now') }}
        </span>
        <span v-else-if="entry.state === 'next'" class="progress__next">
          {{ t('discuss.next') }}
        </span>
      </button>
    </li>
  </ol>
</template>

<style scoped>
.progress {
  display: flex;
  flex-direction: column;
  gap: 2px;
  margin: 6px 0 4px;
  padding: 6px;
  border-radius: var(--radius-control);
  background: var(--accent-soft);
  list-style: none;
}
.progress__item {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  min-height: 44px;
  box-sizing: border-box;
  padding: 4px 8px;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: inherit;
  font: inherit;
  text-align: left;
  cursor: pointer;
}
.progress__item:hover {
  background: var(--surface);
}
.progress__item--current {
  background: var(--surface);
  box-shadow: var(--shadow-card);
}
.progress__item--viewing {
  background: var(--surface);
  box-shadow: inset 0 0 0 2px var(--accent);
}
.progress__votes {
  display: inline-flex;
  flex: none;
  align-items: center;
  justify-content: center;
  min-width: 28px;
  height: 22px;
  box-sizing: border-box;
  padding: 0 6px;
  border-radius: var(--radius-pill);
  background: var(--surface);
  color: var(--text);
  font-size: 12px;
  font-weight: 700;
}
.progress__item--current .progress__votes {
  background: var(--accent);
  color: var(--on-accent);
}
.progress__name {
  flex: 1 1 auto;
  min-width: 0;
  color: var(--muted);
  font-size: 13px;
  font-weight: 500;
  line-height: 1.3;
  overflow-wrap: anywhere;
}
.progress__item--next .progress__name,
.progress__item--viewing .progress__name {
  color: var(--text);
}
.progress__item--current .progress__name {
  color: var(--text);
  font-weight: 700;
}
.progress__done {
  display: flex;
  color: var(--success);
}
.progress__now {
  padding: 2px 8px;
  border-radius: var(--radius-pill);
  background: var(--accent);
  color: var(--on-accent);
  font-size: 11px;
  font-weight: 700;
  white-space: nowrap;
}
.progress__next {
  color: var(--accent-text);
  font-size: 11px;
  font-weight: 700;
}
</style>
