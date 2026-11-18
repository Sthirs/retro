<script setup lang="ts">
/**
 * The Vote phase (Vote artboard, spec §9.4): every group, in its column and
 * place, with the viewer's own votes on it and how many they have left.
 * Nobody sees anyone else's votes; totals stay hidden until Discuss. The
 * facilitator also sees how many votes were cast overall.
 */
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';

import AppIcon from '@/components/AppIcon.vue';
import { provideGroupView } from '@/features/meetings/group/groupView';
import { COLUMNS } from '@/features/meetings/reflect/columns';
import VoteGroup from '@/features/meetings/vote/VoteGroup.vue';
import { useMeetingStore } from '@/stores/meeting';

const { t } = useI18n();
const store = useMeetingStore();

// Which groups are expanded: local to this viewer.
provideGroupView(() => {});

/** One thumb per vote of the budget: filled while still left to give. */
const thumbs = computed(() =>
  Array.from({ length: store.voteBudget }, (_, i) => i < store.votesLeft),
);
</script>

<template>
  <div class="voting">
    <div class="budget" data-testid="vote-budget">
      <span class="budget__left">
        <span class="budget__thumbs" aria-hidden="true">
          <span
            v-for="(left, i) in thumbs"
            :key="i"
            class="budget__thumb"
            :class="{ 'budget__thumb--left': left }"
          >
            <AppIcon name="vote" :size="22" />
          </span>
        </span>
        <i18n-t keypath="vote.left" tag="span" class="budget__count" data-testid="vote-left">
          <template #count>
            <strong>
              {{ t('vote.leftCount', { left: store.votesLeft, budget: store.voteBudget }) }}
            </strong>
          </template>
        </i18n-t>
      </span>
      <span class="budget__hint">{{ t('vote.hint', { max: store.voteMaxPerGroup }) }}</span>
      <template v-if="store.votesCast !== null">
        <span class="budget__spacer" />
        <span class="budget__cast" data-testid="vote-cast">
          <AppIcon name="users" :size="16" />
          <i18n-t keypath="vote.cast" tag="span" :plural="store.votesCast">
            <template #count>
              <strong>{{ store.votesCast }}</strong>
            </template>
          </i18n-t>
        </span>
      </template>
    </div>
    <div class="voting__columns">
      <section
        v-for="column in COLUMNS"
        :key="column"
        class="vcol"
        :aria-label="t(`reflect.columns.${column}.name`)"
        :style="{ '--fg': `var(--col-${column}-fg)` }"
      >
        <h2 class="vcol__name">
          <span class="vcol__dot" aria-hidden="true" />
          {{ t(`reflect.columns.${column}.name`) }}
        </h2>
        <template v-for="item in store.columnItems.get(column) ?? []">
          <VoteGroup
            v-if="item.kind === 'group'"
            :key="item.group.id"
            :group="item.group"
            :cards="item.cards"
          />
        </template>
      </section>
    </div>
  </div>
</template>

<style scoped>
.voting {
  display: flex;
  flex: 1 1 auto;
  flex-direction: column;
  gap: 16px;
  min-height: 0;
}
.budget {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 12px 24px;
  padding: 14px 20px;
  border: 1px solid var(--line);
  border-radius: var(--radius-panel);
  background: var(--surface);
  box-shadow: var(--shadow-panel);
}
.budget__left {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px 14px;
}
.budget__thumbs {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}
.budget__thumb {
  display: inline-flex;
  color: var(--muted);
}
.budget__thumb--left {
  color: var(--accent);
}
.budget__thumb--left :deep(svg) {
  fill: currentColor;
}
.budget__count {
  font-size: 15px;
}
.budget__hint {
  color: var(--muted);
  font-size: 13px;
}
.budget__spacer {
  flex: 1 1 0;
}
.budget__cast {
  display: flex;
  align-items: center;
  gap: 8px;
  color: var(--muted);
  font-size: 14px;
}
.budget__cast strong {
  color: var(--text);
}
.voting__columns {
  display: grid;
  flex: 1 1 auto;
  grid-template-columns: repeat(auto-fit, minmax(230px, 1fr));
  gap: 16px;
  align-items: stretch;
}
.vcol {
  display: flex;
  flex-direction: column;
  gap: 10px;
  min-width: 0;
  padding: 14px 12px;
  border: 1px solid var(--well-edge);
  border-radius: var(--radius-panel);
  background: var(--well);
}
.vcol__name {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: 0 2px 2px;
  color: var(--text);
  font-size: 17px;
  font-weight: 700;
}
.vcol__dot {
  width: 10px;
  height: 10px;
  border-radius: 50%;
  background: var(--fg);
}
</style>
