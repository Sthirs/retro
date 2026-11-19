<script setup lang="ts">
/** One group of an ended meeting (Summary artboard): rank, name, votes, anonymous cards. */
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';

import type { SummaryGroupDto } from '@/api/client';
import RichText from '@/components/RichText.vue';
import { excerpt, plainText } from '@/features/meetings/group/plainText';

const props = defineProps<{ group: SummaryGroupDto; rank: number }>();
const { t } = useI18n();

/** Its name, a never-grouped card's text, or "Untitled group" (as in Vote). */
const label = computed(() => {
  if (props.group.name) return props.group.name;
  const [only, ...rest] = props.group.reflections;
  if (!only || rest.length > 0) return t('group.untitled');
  return excerpt(plainText(only.content, t('group.image'))) || t('group.untitled');
});
const headingId = computed(() => `summary-group-${props.group.id}`);
</script>

<template>
  <article class="group" :aria-labelledby="headingId" data-testid="summary-group">
    <div class="group__header">
      <span class="group__rank">{{ t('summary.rank', { rank }) }}</span>
      <h3 :id="headingId" class="group__name">{{ label }}</h3>
      <!-- No total when the meeting ended before Discuss: nobody saw it. -->
      <span
        v-if="group.votes != null"
        class="group__votes"
        :class="{ 'group__votes--top': rank === 1 && group.votes > 0 }"
        data-testid="summary-group-votes"
      >
        {{ t('summary.votes', { count: group.votes }, group.votes) }}
      </span>
    </div>
    <ul class="group__cards">
      <li
        v-for="card in group.reflections"
        :key="card.id"
        class="card"
        :style="{ '--fg': `var(--col-${card.column}-fg)` }"
      >
        <span class="card__column">{{ t(`reflect.columns.${card.column}.name`) }}</span>
        <RichText :html="card.content" class="card__content" />
      </li>
    </ul>
  </article>
</template>

<style scoped>
.group {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 16px 20px;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: var(--radius-panel);
  box-shadow: var(--shadow-panel);
}
.group__header {
  display: flex;
  align-items: center;
  gap: 12px;
}
.group__rank {
  color: var(--muted);
  font-size: 15px;
  font-weight: 800;
}
.group__name {
  flex: 1 1 auto;
  min-width: 0;
  margin: 0;
  font-size: 17px;
  font-weight: 700;
  letter-spacing: -0.01em;
  overflow-wrap: anywhere;
}
.group__votes {
  display: inline-flex;
  align-items: center;
  padding: 3px 10px;
  border-radius: var(--radius-pill);
  background: var(--accent-soft);
  color: var(--accent-text);
  font-size: 12px;
  font-weight: 700;
  white-space: nowrap;
}
.group__votes--top {
  background: var(--accent);
  color: var(--on-accent);
}
.group__cards {
  margin: 0;
  padding: 0;
  list-style: none;
}
.card {
  display: flex;
  gap: 10px;
  padding: 8px 0;
  border-top: 1px solid var(--line);
  font-size: 14px;
  line-height: 1.45;
}
.card__column {
  flex: none;
  width: 64px;
  color: var(--fg);
  font-size: 12px;
  font-weight: 700;
  line-height: 20px;
}
.card__content {
  flex: 1 1 auto;
  min-width: 0;
  overflow-wrap: anywhere;
}
</style>
