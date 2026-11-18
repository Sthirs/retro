<script setup lang="ts">
/**
 * The Group phase (Group artboard, spec §9.3): everyone's reflections,
 * anonymous, in their columns. Participants drop cards onto cards to make
 * groups, onto groups to add them, and out of groups to remove them; by
 * keyboard through each card's Move to…. Everyone sees who drags which card.
 */
import { nextTick, onBeforeUnmount } from 'vue';
import { useI18n } from 'vue-i18n';

import { realtime } from '@/api/realtime';
import AppIcon from '@/components/AppIcon.vue';
import GroupColumn from '@/features/meetings/group/GroupColumn.vue';
import { provideGroupView } from '@/features/meetings/group/groupView';
import { provideCardDrag, type DropTarget } from '@/features/meetings/group/useCardDrag';
import { COLUMNS } from '@/features/meetings/reflect/columns';
import { useMeetingStore } from '@/stores/meeting';

const { t } = useI18n();
const store = useMeetingStore();

function channel(): string | null {
  return store.meetingId ? `meeting:${store.meetingId}` : null;
}

provideCardDrag({
  onLift(cardId) {
    const name = channel();
    if (name) realtime.setDragging(name, cardId);
  },
  onDrop(cardId, target) {
    void drop(cardId, target);
  },
});

provideGroupView((cardId) => {
  void nextTick(() => {
    document
      .querySelector<HTMLElement>(`[data-card-id="${CSS.escape(cardId)}"] [data-move-trigger]`)
      ?.focus();
  });
});

/** What a drop means (spec §9.3): onto a card, onto a group, or out of its group. */
async function drop(cardId: string, target: DropTarget | null): Promise<void> {
  const card = store.meeting?.reflections.find((r) => r.id === cardId);
  if (!card || !target) return;
  switch (target.kind) {
    case 'card': {
      const other = store.meeting?.reflections.find((r) => r.id === target.id);
      if (!other || (card.groupId && other.groupId === card.groupId)) return;
      await store.dropOnCard(target.id, cardId);
      return;
    }
    case 'group':
      if (card.groupId !== target.id) await store.moveToGroup(cardId, target.id);
      return;
    case 'column':
      if (card.groupId) await store.moveToGroup(cardId, null);
      return;
  }
}

// The phase is over (or the room closed): this tab drags nothing.
onBeforeUnmount(() => {
  const name = channel();
  if (name) realtime.setDragging(name, null);
});
</script>

<template>
  <div class="grouping">
    <p class="grouping__hint">
      <AppIcon name="move" :size="16" />
      <i18n-t keypath="group.hint" tag="span">
        <template #moveTo>
          <strong>{{ t('group.moveTo') }}</strong>
        </template>
      </i18n-t>
    </p>
    <div class="grouping__columns">
      <GroupColumn v-for="column in COLUMNS" :key="column" :column="column" />
    </div>
  </div>
</template>

<style scoped>
.grouping {
  display: flex;
  flex: 1 1 auto;
  flex-direction: column;
  gap: 16px;
  min-height: 0;
}
.grouping__hint {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: 0;
  color: var(--muted);
  font-size: 13px;
}
.grouping__hint strong {
  color: var(--text);
}
.grouping__columns {
  display: grid;
  flex: 1 1 auto;
  grid-template-columns: repeat(auto-fit, minmax(230px, 1fr));
  gap: 16px;
  align-items: stretch;
}
</style>
