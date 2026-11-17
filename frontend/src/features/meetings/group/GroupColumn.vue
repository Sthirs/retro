<script setup lang="ts">
/**
 * A column of the Group phase (Group artboard): its ungrouped cards and the
 * groups placed in it, in their places. Dropping a grouped card on the
 * column itself takes it out of its group.
 */
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';

import type { ReflectionColumn } from '@/api/client';
import GroupCard from '@/features/meetings/group/GroupCard.vue';
import ReflectionGroup from '@/features/meetings/group/ReflectionGroup.vue';
import { useCardDrag } from '@/features/meetings/group/useCardDrag';
import { useMeetingStore } from '@/stores/meeting';

const props = defineProps<{ column: ReflectionColumn }>();
const { t } = useI18n();
const store = useMeetingStore();
const drag = useCardDrag();

const name = computed(() => t(`reflect.columns.${props.column}.name`));
const items = computed(() => store.columnItems.get(props.column) ?? []);
const isOver = computed(
  () => drag.over.value?.kind === 'column' && drag.over.value.id === props.column,
);
</script>

<template>
  <section
    class="gcol"
    :class="{ 'gcol--over': isOver }"
    :aria-label="name"
    :style="{ '--fg': `var(--col-${column}-fg)` }"
    :data-drop="`column:${column}`"
  >
    <h2 class="gcol__name">
      <span class="gcol__dot" aria-hidden="true" />
      {{ name }}
    </h2>
    <template v-for="item in items" :key="item.kind === 'card' ? item.card.id : item.group.id">
      <GroupCard v-if="item.kind === 'card'" :card="item.card" />
      <ReflectionGroup v-else :group="item.group" :cards="item.cards" />
    </template>
  </section>
</template>

<style scoped>
.gcol {
  display: flex;
  flex-direction: column;
  gap: 10px;
  min-width: 0;
  min-height: 420px;
  padding: 14px 12px;
  border: 1px solid var(--well-edge);
  border-radius: var(--radius-panel);
  background: var(--well);
}
.gcol--over {
  outline: 2px dashed var(--accent);
  outline-offset: -2px;
}
.gcol__name {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: 0 2px 2px;
  color: var(--text);
  font-size: 17px;
  font-weight: 700;
}
.gcol__dot {
  width: 10px;
  height: 10px;
  border-radius: 50%;
  background: var(--fg);
}
</style>
