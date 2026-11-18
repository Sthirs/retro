<script setup lang="ts">
/**
 * The Discuss phase (Discuss artboard, spec §9.5): one group at a time, with
 * its vote total and its cards (anonymous, with their column), reactions on
 * every card, and the Tasks panel. Everyone follows the facilitator's
 * current group; anyone can open another group from the sidebar to look at
 * it, read-only, and come back. When the facilitator moves on, everyone's
 * room follows.
 */
import { computed, ref } from 'vue';
import { useI18n } from 'vue-i18n';

import AppIcon from '@/components/AppIcon.vue';
import RichText from '@/components/RichText.vue';
import AppButton from '@/components/ui/AppButton.vue';
import DiscussTasks from '@/features/meetings/discuss/DiscussTasks.vue';
import ReactionBar from '@/features/meetings/discuss/ReactionBar.vue';
import { useGroupLabel } from '@/features/meetings/discuss/useGroupLabel';
import { useMeetingStore } from '@/stores/meeting';

const { t } = useI18n();
const store = useMeetingStore();
const labelOf = useGroupLabel();

const groupId = computed(() => store.viewingGroupId);
const entry = computed(() => store.discussOrder[store.viewingIndex] ?? null);
const cards = computed(() => (groupId.value ? (store.groupCards.get(groupId.value) ?? []) : []));
/** Reactions and tasks only on the current group, while the meeting runs. */
const readonly = computed(() => store.previewing || !store.isActive);
const busy = ref(false);

/** Moves everyone to the group on screen (facilitator). */
async function discussThis(id: string): Promise<void> {
  if (busy.value) return;
  busy.value = true;
  try {
    await store.setCurrentGroup(id);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div v-if="store.discussOrder.length === 0 || !groupId || !entry" class="empty">
    <AppIcon name="message" :size="28" />
    <h2 class="empty__title">{{ t('discuss.empty') }}</h2>
    <p class="empty__text">{{ t('discuss.emptyHint') }}</p>
  </div>
  <div v-else class="discuss">
    <div v-if="store.previewing" class="preview" role="status" data-testid="discuss-preview">
      <span class="preview__text">
        {{ t('discuss.previewing', { n: store.currentIndex + 1 }) }}
      </span>
      <span class="preview__actions">
        <AppButton size="compact" @click="store.followCurrent()">
          <AppIcon name="chevronLeft" :size="16" />
          {{ t('discuss.backToCurrent') }}
        </AppButton>
        <AppButton
          v-if="store.isFacilitator && store.isActive"
          variant="primary"
          size="compact"
          :disabled="busy"
          @click="discussThis(groupId)"
        >
          {{ t('discuss.discussThis') }}
        </AppButton>
      </span>
    </div>
    <div class="discuss__grid">
      <section
        class="group"
        :aria-label="store.previewing ? t('discuss.viewing') : t('discuss.current')"
        data-testid="discuss-group"
      >
        <div class="group__head">
          <div class="group__meta">
            <span class="group__of">
              {{
                t('discuss.groupOf', {
                  n: store.viewingIndex + 1,
                  total: store.discussOrder.length,
                })
              }}
            </span>
            <span class="group__votes" data-testid="discuss-votes">
              {{ t('discuss.votes', entry.votes) }}
            </span>
          </div>
          <h2 class="group__title">{{ labelOf(groupId) }}</h2>
        </div>
        <div class="group__cards">
          <article
            v-for="card in cards"
            :key="card.id"
            class="card"
            :style="{ '--fg': `var(--col-${card.column}-fg)` }"
            data-testid="discuss-card"
          >
            <span
              class="card__dot"
              role="img"
              :aria-label="t(`reflect.columns.${card.column}.name`)"
              :title="t(`reflect.columns.${card.column}.name`)"
            />
            <RichText class="card__text" :html="card.content" />
            <ReactionBar
              :reflection-id="card.id"
              :reactions="store.reactionsByCard.get(card.id) ?? []"
              :readonly="readonly"
            />
          </article>
        </div>
      </section>
      <DiscussTasks :key="groupId" :group-id="groupId" :readonly="readonly" />
    </div>
  </div>
</template>

<style scoped>
.discuss {
  display: flex;
  flex: 1 1 auto;
  flex-direction: column;
  gap: 16px;
  min-height: 0;
}
.preview {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 10px 12px;
  padding: 10px 14px;
  border: 1px solid var(--accent);
  border-radius: var(--radius-control);
  background: var(--accent-soft);
  color: var(--text);
  font-size: 14px;
}
.preview__text {
  flex: 1 1 240px;
}
.preview__actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}
.discuss__grid {
  display: grid;
  flex: 1 1 auto;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 16px;
  align-items: stretch;
}
.group {
  display: flex;
  flex-direction: column;
  grid-column: span 3;
  gap: 14px;
  min-width: 0;
  padding: 16px;
  border: 1px solid var(--well-edge);
  border-radius: var(--radius-panel);
  background: var(--well);
}
.group__head {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 2px 2px 0;
}
.group__meta {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 10px 14px;
}
.group__of {
  color: var(--muted);
  font-size: 13px;
  font-weight: 600;
}
.group__votes {
  display: inline-flex;
  align-items: center;
  padding: 3px 10px;
  border-radius: var(--radius-pill);
  background: var(--accent);
  color: var(--on-accent);
  font-size: 12px;
  font-weight: 700;
  white-space: nowrap;
}
.group__title {
  margin: 0;
  font-size: 24px;
  font-weight: 700;
  letter-spacing: -0.01em;
  overflow-wrap: anywhere;
}
.group__cards {
  column-count: 3;
  column-gap: 12px;
}
.card {
  position: relative;
  display: flex;
  flex-direction: column;
  gap: 10px;
  margin: 0 0 12px;
  padding: 12px 14px 12px 28px;
  border: 1px solid var(--line);
  border-radius: var(--radius-control);
  background: var(--card);
  box-shadow: var(--shadow-card);
  break-inside: avoid;
}
.card__dot {
  position: absolute;
  top: 12px;
  left: 12px;
  width: 9px;
  height: 9px;
  border-radius: 50%;
  background: var(--fg);
}
.card__text {
  font-size: 14px;
  line-height: 1.5;
  overflow-wrap: anywhere;
}
.empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  padding: 48px 16px;
  border-radius: var(--radius-panel);
  background: var(--well);
  color: var(--muted);
  text-align: center;
}
.empty__title {
  margin: 0;
  color: var(--text);
  font-size: 20px;
}
.empty__text {
  margin: 0;
  font-size: 14px;
}
@media (max-width: 1100px) {
  .discuss__grid {
    grid-template-columns: minmax(0, 1fr);
  }
  .group {
    grid-column: auto;
  }
  .group__cards {
    column-count: 2;
  }
}
@media (max-width: 600px) {
  .group__cards {
    column-count: 1;
  }
}
</style>
