<script setup lang="ts">
/**
 * The Vote phase (Vote artboard, spec §9.4): every group, in its column and
 * place, with the viewer's own votes on it and how many they have left.
 * Nobody sees anyone else's votes; totals stay hidden until Discuss.
 * Everyone also sees how many of the team's votes are left overall.
 */
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';

import AppIcon from '@/components/AppIcon.vue';
import { provideGroupView } from '@/features/meetings/group/groupView';
import { COLUMNS } from '@/features/meetings/reflect/columns';
import VoteGroup from '@/features/meetings/vote/VoteGroup.vue';
import { voteLabel } from '@/features/meetings/vote/voteLabel';
import { useMeetingStore } from '@/stores/meeting';

const { t } = useI18n();
const store = useMeetingStore();

// Which groups are expanded: local to this viewer.
provideGroupView(() => {});

/**
 * For groups whose name another group shares: their column and place, so
 * their vote controls stay apart for a screen reader. By group id.
 */
const where = computed(() => {
  const words = { untitled: t('group.untitled'), image: t('group.image') };
  const placed: { id: string; label: string; column: string; n: number }[] = [];
  for (const column of COLUMNS) {
    let n = 0;
    for (const item of store.columnItems.get(column) ?? []) {
      if (item.kind !== 'group') continue;
      n++;
      placed.push({
        id: item.group.id,
        label: voteLabel(item.group, item.cards, words),
        column,
        n,
      });
    }
  }
  const uses = new Map<string, number>();
  for (const p of placed) uses.set(p.label, (uses.get(p.label) ?? 0) + 1);
  return new Map(
    placed
      .filter((p) => (uses.get(p.label) ?? 0) > 1)
      .map((p) => [
        p.id,
        t('vote.where', { column: t(`reflect.columns.${p.column}.name`), n: p.n }),
      ]),
  );
});

/** One thumb per vote of the budget: filled while still left to give. */
const thumbs = computed(() =>
  Array.from({ length: store.voteBudget }, (_, i) => i < store.votesLeft),
);

/** How much of the team's votes are cast, in percent, for the bar. */
const teamCastPercent = computed(() => {
  const total = store.teamVoteBudget;
  if (!total || store.votesCast === null) return 0;
  return Math.min(100, (store.votesCast / total) * 100);
});
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
        <span class="budget__team" data-testid="vote-cast">
          <span class="budget__team-label">
            <AppIcon name="users" :size="17" />
            {{ t('vote.team') }}
          </span>
          <span
            class="budget__bar"
            role="progressbar"
            :aria-label="t('vote.teamCast')"
            aria-valuemin="0"
            :aria-valuemax="store.teamVoteBudget"
            :aria-valuenow="Math.min(store.votesCast, store.teamVoteBudget)"
          >
            <span class="budget__bar-fill" :style="{ width: `${teamCastPercent}%` }" />
          </span>
          <i18n-t
            keypath="vote.teamLeft"
            tag="span"
            class="budget__team-left"
            data-testid="vote-team-left"
          >
            <template #left>
              <strong>{{ store.teamVotesLeft }}</strong>
            </template>
            <template #total>{{ store.teamVoteBudget }}</template>
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
            :where="where.get(item.group.id)"
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
.budget__team {
  display: flex;
  align-items: center;
  gap: 12px;
}
.budget__team-label {
  display: flex;
  align-items: center;
  gap: 8px;
  color: var(--muted);
  font-size: 14px;
}
.budget__bar {
  display: block;
  flex: none;
  width: 120px;
  height: 8px;
  overflow: hidden;
  border-radius: 999px;
  background: var(--well);
}
.budget__bar-fill {
  display: block;
  height: 100%;
  background: var(--accent);
}
.budget__team-left {
  color: var(--muted);
  font-size: 14px;
}
.budget__team-left strong {
  color: var(--text);
  font-size: 18px;
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
